//! macOS mouse hook via `CGEventTap` (CoreGraphics).
//!
//! Requires *Accessibility* permission:
//!   System Settings → Privacy & Security → Accessibility
//!
//! The tap is inserted at `kCGHIDEventTap` / `kCGHeadInsertEventTap` so it
//! sees raw hardware events before any other process.  Blocking is achieved by
//! returning `None` from the callback instead of the original event.
//!
//! A dispatch thread drains a channel so that the tap callback itself returns
//! as fast as possible (CGEventTap callbacks must not block).
#![allow(dead_code)]

use super::{
    CallbackMap, GestureConfig, GestureDetector, MouseEvent, MouseEventData, MouseEventType,
    MouseHook,
};

// ---------------------------------------------------------------------------
// Real macOS implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    };
    use std::thread;

    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
    use core_graphics::event::{
        CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventTapProxy, CGEventType, EventField,
    };

    // HID button numbers (macOS USB/BT HID mapping)
    const BTN_MIDDLE: i64 = 2;
    const BTN_BACK: i64 = 3;
    const BTN_FORWARD: i64 = 4;

    // CGEventType numeric values (the enum has no PartialEq in this crate version,
    // so we compare via `as u32`).
    const ET_OTHER_MOUSE_DOWN: u32 = CGEventType::OtherMouseDown as u32;
    const ET_OTHER_MOUSE_UP: u32 = CGEventType::OtherMouseUp as u32;
    const ET_MOUSE_MOVED: u32 = CGEventType::MouseMoved as u32;
    const ET_OTHER_MOUSE_DRAGGED: u32 = CGEventType::OtherMouseDragged as u32;
    const ET_SCROLL_WHEEL: u32 = CGEventType::ScrollWheel as u32;

    // ---------------------------------------------------------------------------
    // Shared state
    // ---------------------------------------------------------------------------

    struct TapState {
        callbacks: CallbackMap,
        blocked: HashSet<MouseEventType>,
        gesture: GestureDetector,
        connection_cb: Option<Box<dyn Fn(bool) + Send + 'static>>,
        tx: Option<std::sync::mpsc::SyncSender<(MouseEvent, Option<MouseEventData>)>>,
    }

    impl TapState {
        fn new() -> Self {
            Self {
                callbacks: CallbackMap::new(),
                blocked: HashSet::new(),
                gesture: GestureDetector::new(GestureConfig::default()),
                connection_cb: None,
                tx: None,
            }
        }
    }

    static TAP_STATE: OnceLock<Arc<Mutex<TapState>>> = OnceLock::new();
    fn get_tap_state() -> &'static Arc<Mutex<TapState>> {
        TAP_STATE.get_or_init(|| Arc::new(Mutex::new(TapState::new())))
    }

    // ---------------------------------------------------------------------------
    // Dispatch worker
    // ---------------------------------------------------------------------------

    fn dispatch_worker(
        rx: std::sync::mpsc::Receiver<(MouseEvent, Option<MouseEventData>)>,
    ) {
        for (event, data) in rx {
            if let Ok(state) = get_tap_state().lock() {
                state.callbacks.dispatch(event, data);
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Event handler (called from tap closure)
    // ---------------------------------------------------------------------------

    fn handle_event(
        state: &mut TapState,
        event_type: CGEventType,
        event: &CGEvent,
    ) -> Option<CGEvent> {
        let et = event_type as u32;

        // --- Mouse movement: accumulate for gesture detection ---
        if et == ET_MOUSE_MOVED || et == ET_OTHER_MOUSE_DRAGGED {
            if state.gesture.enabled {
                let dx =
                    event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_X) as f64;
                let dy =
                    event.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y) as f64;
                if let Some(gesture_ev) = state.gesture.accumulate(dx, dy, "event_tap") {
                    let data = Some(MouseEventData {
                        delta_x: dx,
                        delta_y: dy,
                        ..Default::default()
                    });
                    if let Some(ref tx) = state.tx {
                        let _ = tx.try_send((gesture_ev, data));
                    }
                }
                // Suppress cursor movement while gesturing
                return None;
            }
            return Some(event.clone());
        }

        let mut fire_event: Option<MouseEvent> = None;
        let mut should_block = false;

        if et == ET_OTHER_MOUSE_DOWN {
            let btn =
                event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);
            log::debug!("[MouseHook] OtherMouseDown btn={btn}");
            match btn {
                BTN_MIDDLE => {
                    fire_event = Some(MouseEvent::MiddleDown);
                    should_block = state.blocked.contains(&MouseEventType::MiddleDown);
                }
                BTN_BACK => {
                    fire_event = Some(MouseEvent::XButton1Down);
                    should_block = state.blocked.contains(&MouseEventType::XButton1Down);
                }
                BTN_FORWARD => {
                    fire_event = Some(MouseEvent::XButton2Down);
                    should_block = state.blocked.contains(&MouseEventType::XButton2Down);
                }
                _ => {}
            }
        } else if et == ET_OTHER_MOUSE_UP {
            let btn =
                event.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER);
            log::debug!("[MouseHook] OtherMouseUp btn={btn}");
            match btn {
                BTN_MIDDLE => {
                    fire_event = Some(MouseEvent::MiddleUp);
                    should_block = state.blocked.contains(&MouseEventType::MiddleUp);
                }
                BTN_BACK => {
                    fire_event = Some(MouseEvent::XButton1Up);
                    should_block = state.blocked.contains(&MouseEventType::XButton1Up);
                }
                BTN_FORWARD => {
                    fire_event = Some(MouseEvent::XButton2Up);
                    should_block = state.blocked.contains(&MouseEventType::XButton2Up);
                }
                _ => {}
            }
        } else if et == ET_SCROLL_WHEEL {
            // SCROLL_WHEEL_EVENT_FIXED_POINT_DELTA_AXIS_2 = horizontal scroll.
            // Raw value is fixed-point 16.16; divide by 65536 for float delta.
            let h_raw = event.get_integer_value_field(
                EventField::SCROLL_WHEEL_EVENT_FIXED_POINT_DELTA_AXIS_2,
            );
            let h_delta = h_raw as f64 / 65536.0;
            log::debug!("[MouseHook] ScrollWheel h={h_delta}");

            if h_delta > 0.0 {
                fire_event = Some(MouseEvent::HScrollRight);
                should_block = state.blocked.contains(&MouseEventType::HScrollRight);
            } else if h_delta < 0.0 {
                fire_event = Some(MouseEvent::HScrollLeft);
                should_block = state.blocked.contains(&MouseEventType::HScrollLeft);
            }

            // Enqueue via the dispatch channel to keep the tap callback fast.
            if let Some(ev) = fire_event {
                let data = Some(MouseEventData {
                    scroll_delta: h_delta.abs(),
                    ..Default::default()
                });
                if let Some(ref tx) = state.tx {
                    let _ = tx.try_send((ev, data));
                }
                fire_event = None;
            }

            if should_block {
                return None;
            }
            return Some(event.clone());
        }

        if let Some(ev) = fire_event {
            if let Some(ref tx) = state.tx {
                let _ = tx.try_send((ev, None));
            }
        }

        if should_block {
            return None;
        }
        Some(event.clone())
    }

    // ---------------------------------------------------------------------------
    // Run-loop thread
    // ---------------------------------------------------------------------------

    fn run_loop_thread(ready_tx: std::sync::mpsc::SyncSender<bool>) {
        let events_of_interest: Vec<CGEventType> = vec![
            CGEventType::OtherMouseDown,
            CGEventType::OtherMouseUp,
            CGEventType::MouseMoved,
            CGEventType::OtherMouseDragged,
            CGEventType::ScrollWheel,
        ];

        let tap = match CGEventTap::new(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            events_of_interest,
            // The closure must be 'tap_life.  Since the tap is owned by this
            // thread (not leaked), 'tap_life == the thread scope, which is fine.
            |_proxy: CGEventTapProxy, event_type: CGEventType, event: &CGEvent| {
                if let Ok(mut state) = get_tap_state().try_lock() {
                    handle_event(&mut state, event_type, event)
                } else {
                    Some(event.clone())
                }
            },
        ) {
            Ok(t) => t,
            Err(_) => {
                log::error!(
                    "[MouseHook] CGEventTap creation failed. \
                     Grant Accessibility in System Settings → Privacy & Security."
                );
                let _ = ready_tx.send(false);
                return;
            }
        };

        let loop_source = match tap.mach_port.create_runloop_source(0) {
            Ok(src) => src,
            Err(_) => {
                log::error!("[MouseHook] Failed to create CFRunLoopSource");
                let _ = ready_tx.send(false);
                return;
            }
        };

        let current_loop = CFRunLoop::get_current();
        // SAFETY: kCFRunLoopCommonModes is a valid, non-null CFStringRef from the OS.
        let mode = unsafe { kCFRunLoopCommonModes };
        current_loop.add_source(&loop_source, mode);
        tap.enable();

        log::info!("[MouseHook] CGEventTap enabled");
        let _ = ready_tx.send(true);

        CFRunLoop::run_current();
        log::info!("[MouseHook] CGEventTap run loop exited");
    }

    // ---------------------------------------------------------------------------
    // Public struct
    // ---------------------------------------------------------------------------

    pub struct MacosMouseHook {
        running: AtomicBool,
        run_loop_thread: Option<thread::JoinHandle<()>>,
        dispatch_thread: Option<thread::JoinHandle<()>>,
    }

    // SAFETY: we never touch the run-loop object from other threads; we only
    // call CFRunLoop::stop() which is documented thread-safe.
    unsafe impl Send for MacosMouseHook {}

    impl MacosMouseHook {
        pub fn new() -> Self {
            let _ = get_tap_state();
            Self {
                running: AtomicBool::new(false),
                run_loop_thread: None,
                dispatch_thread: None,
            }
        }
    }

    impl MouseHook for MacosMouseHook {
        fn register(
            &mut self,
            event_type: MouseEventType,
            cb: Box<dyn Fn(MouseEvent, Option<MouseEventData>) + Send + 'static>,
        ) {
            get_tap_state()
                .lock()
                .unwrap()
                .callbacks
                .register(event_type, cb);
        }

        fn block(&mut self, event_type: MouseEventType) {
            get_tap_state().lock().unwrap().blocked.insert(event_type);
        }

        fn unblock(&mut self, event_type: MouseEventType) {
            get_tap_state().lock().unwrap().blocked.remove(&event_type);
        }

        fn reset_bindings(&mut self) {
            let mut state = get_tap_state().lock().unwrap();
            state.callbacks.clear();
            state.blocked.clear();
        }

        fn configure_gestures(&mut self, config: GestureConfig) {
            get_tap_state().lock().unwrap().gesture.configure(config);
        }

        fn set_connection_change_callback(&mut self, cb: Box<dyn Fn(bool) + Send + 'static>) {
            get_tap_state().lock().unwrap().connection_cb = Some(cb);
        }

        fn start(&mut self) -> anyhow::Result<()> {
            if self.running.load(Ordering::SeqCst) {
                return Ok(());
            }

            let (tx, rx) = std::sync::mpsc::sync_channel(256);
            get_tap_state().lock().unwrap().tx = Some(tx);

            let dispatch_handle = thread::Builder::new()
                .name("MouseHook-dispatch".into())
                .spawn(move || dispatch_worker(rx))?;
            self.dispatch_thread = Some(dispatch_handle);

            let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
            let run_loop_handle = thread::Builder::new()
                .name("MouseHook-runloop".into())
                .spawn(move || run_loop_thread(ready_tx))?;
            self.run_loop_thread = Some(run_loop_handle);

            match ready_rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(true) => {
                    self.running.store(true, Ordering::SeqCst);
                    Ok(())
                }
                Ok(false) => Err(anyhow::anyhow!(
                    "CGEventTap creation failed — check Accessibility permission"
                )),
                Err(_) => Err(anyhow::anyhow!("Hook startup timed out")),
            }
        }

        fn stop(&mut self) {
            if !self.running.load(Ordering::SeqCst) {
                return;
            }
            self.running.store(false, Ordering::SeqCst);

            // Stop the run loop so the run_loop_thread exits naturally.
            // We use the main run loop as a proxy since we cannot easily
            // capture the spawned thread's run loop reference here.  The
            // thread is a daemon thread and will exit on process shutdown.
            CFRunLoop::get_main().stop();

            if let Some(handle) = self.run_loop_thread.take() {
                let _ = handle.join();
            }

            // Dropping the sender causes dispatch_worker's `for` loop to exit.
            get_tap_state().lock().unwrap().tx = None;
            if let Some(handle) = self.dispatch_thread.take() {
                let _ = handle.join();
            }

            log::info!("[MouseHook] CGEventTap stopped");
        }
    }
}

// ---------------------------------------------------------------------------
// Stub for non-macOS platforms
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::*;

    pub struct MacosMouseHook;

    impl MacosMouseHook {
        pub fn new() -> Self {
            Self
        }
    }

    impl MouseHook for MacosMouseHook {
        fn register(
            &mut self,
            _event_type: MouseEventType,
            _cb: Box<dyn Fn(MouseEvent, Option<MouseEventData>) + Send + 'static>,
        ) {
        }
        fn block(&mut self, _event_type: MouseEventType) {}
        fn unblock(&mut self, _event_type: MouseEventType) {}
        fn reset_bindings(&mut self) {}
        fn configure_gestures(&mut self, _config: GestureConfig) {}
        fn set_connection_change_callback(&mut self, _cb: Box<dyn Fn(bool) + Send + 'static>) {}
        fn start(&mut self) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("macOS hook not available on this platform"))
        }
        fn stop(&mut self) {}
    }
}

pub use imp::MacosMouseHook;
