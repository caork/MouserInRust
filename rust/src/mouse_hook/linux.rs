//! Linux mouse hook via `evdev`.
//!
//! Grabs the Logitech (or best available) mouse device for exclusive access,
//! then re-emits all non-blocked events through a UInput virtual device.
//!
//! Requires:
//!   • The user is in the `input` group (for `/dev/input/event*` read access).
//!   • `/dev/uinput` is writable (typically via `udev` rule or group `uinput`).
//!
//! The outer `_evdev_loop` handles reconnection: if the grabbed device
//! disconnects it waits 1 s and rescans.  If the HID gesture listener
//! reconnects and the current grabbed device is not Logitech, a rescan is
//! requested immediately.
#![allow(dead_code)]

use super::{
    CallbackMap, GestureConfig, GestureDetector, MouseEvent, MouseEventData, MouseEventType,
    MouseHook,
};

// ---------------------------------------------------------------------------
// Real Linux implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod imp {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };
    use std::thread;
    use std::time::Duration;

    use evdev::{
        uinput::VirtualDevice, Device, EventType, InputEvent, Key, RelativeAxisType,
    };

    // Logitech USB vendor ID
    const LOGI_VENDOR: u16 = 0x046D;

    // Button-code constants (from linux/input-event-codes.h, re-exposed by evdev)
    // BTN_SIDE  = back button  (XButton1 / XBUTTON1_DOWN)
    // BTN_EXTRA = forward button (XButton2 / XBUTTON2_DOWN)
    // REL_HWHEEL_HI_RES is defined in newer kernels; fall back to 0x0C
    const REL_HWHEEL_HI_RES: u16 = 0x0C;
    const REL_WHEEL_HI_RES: u16 = 0x0B;

    // ---------------------------------------------------------------------------
    // Shared state
    // ---------------------------------------------------------------------------

    struct HookState {
        callbacks: CallbackMap,
        blocked: HashSet<MouseEventType>,
        gesture: GestureDetector,
        connection_cb: Option<Box<dyn Fn(bool) + Send + 'static>>,
        /// Set by the HID gesture listener to request a device rescan.
        rescan_requested: bool,
    }

    impl HookState {
        fn new() -> Self {
            Self {
                callbacks: CallbackMap::new(),
                blocked: HashSet::new(),
                gesture: GestureDetector::new(GestureConfig::default()),
                connection_cb: None,
                rescan_requested: false,
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Device discovery
    // ---------------------------------------------------------------------------

    /// Find the best mouse device: prefer Logitech, prefer devices with side buttons.
    fn find_mouse_device() -> Option<Device> {
        let mut logi_mice: Vec<(Device, bool)> = Vec::new();
        let mut other_mice: Vec<(Device, bool)> = Vec::new();

        for path in evdev::enumerate().map(|(path, _)| path) {
            let dev = match Device::open(&path) {
                Ok(d) => d,
                Err(_) => continue,
            };

            // Must have REL_X, REL_Y (movement) and BTN_LEFT (mouse button)
            let has_rel_x = dev
                .supported_relative_axes()
                .map(|a| a.contains(RelativeAxisType::REL_X))
                .unwrap_or(false);
            let has_rel_y = dev
                .supported_relative_axes()
                .map(|a| a.contains(RelativeAxisType::REL_Y))
                .unwrap_or(false);
            let has_btn_left = dev
                .supported_keys()
                .map(|k| k.contains(Key::BTN_LEFT))
                .unwrap_or(false);

            if !has_rel_x || !has_rel_y || !has_btn_left {
                continue;
            }

            let has_side = dev
                .supported_keys()
                .map(|k| k.contains(Key::BTN_SIDE) || k.contains(Key::BTN_EXTRA))
                .unwrap_or(false);

            if dev.input_id().vendor() == LOGI_VENDOR {
                logi_mice.push((dev, has_side));
            } else {
                other_mice.push((dev, has_side));
            }
        }

        // Sort: side-button devices first within each group
        logi_mice.sort_by_key(|(_, s)| !*s);
        other_mice.sort_by_key(|(_, s)| !*s);

        let mut ordered: Vec<(Device, bool)> = logi_mice.into_iter().chain(other_mice).collect();
        if ordered.is_empty() {
            return None;
        }

        let (chosen, _) = ordered.remove(0);
        // Close the rest
        drop(ordered);

        log::info!(
            "[MouseHook] Found mouse: {} vendor=0x{:04X}",
            chosen.name().unwrap_or("unknown"),
            chosen.input_id().vendor(),
        );
        Some(chosen)
    }

    // ---------------------------------------------------------------------------
    // Inner listen loop
    // ---------------------------------------------------------------------------

    /// Process one input event.  Returns `true` if the event should be forwarded
    /// to the virtual device.
    fn handle_event(
        event: &InputEvent,
        state: &mut HookState,
        uinput: &mut VirtualDevice,
    ) -> bool {
        match event.event_type() {
            EventType::SYNCHRONIZATION => {
                let _ = uinput.emit(&[*event]);
                return false; // handled directly
            }

            EventType::KEY => {
                let key = Key::new(event.code());
                let value = event.value(); // 1=press, 0=release, 2=repeat

                let (mouse_ev, block_key) = match key {
                    Key::BTN_SIDE => {
                        if value == 1 {
                            (
                                Some(MouseEvent::XButton1Down),
                                state.blocked.contains(&MouseEventType::XButton1Down),
                            )
                        } else if value == 0 {
                            (
                                Some(MouseEvent::XButton1Up),
                                state.blocked.contains(&MouseEventType::XButton1Up),
                            )
                        } else {
                            (None, false)
                        }
                    }
                    Key::BTN_EXTRA => {
                        if value == 1 {
                            (
                                Some(MouseEvent::XButton2Down),
                                state.blocked.contains(&MouseEventType::XButton2Down),
                            )
                        } else if value == 0 {
                            (
                                Some(MouseEvent::XButton2Up),
                                state.blocked.contains(&MouseEventType::XButton2Up),
                            )
                        } else {
                            (None, false)
                        }
                    }
                    Key::BTN_MIDDLE => {
                        if value == 1 {
                            (
                                Some(MouseEvent::MiddleDown),
                                state.blocked.contains(&MouseEventType::MiddleDown),
                            )
                        } else if value == 0 {
                            (
                                Some(MouseEvent::MiddleUp),
                                state.blocked.contains(&MouseEventType::MiddleUp),
                            )
                        } else {
                            (None, false)
                        }
                    }
                    _ => (None, false),
                };

                if let Some(ev) = mouse_ev {
                    state.callbacks.dispatch(ev, None);
                }

                if block_key {
                    return false; // do not forward
                }
                let _ = uinput.emit(&[*event]);
                return false;
            }

            EventType::RELATIVE => {
                let code = event.code();
                let value = event.value();

                // Movement axes
                if code == RelativeAxisType::REL_X.0 || code == RelativeAxisType::REL_Y.0 {
                    // Suppress cursor movement during gesture tracking so the
                    // pointer doesn't drift while the user swipes.
                    if state.gesture.enabled {
                        // accumulate — gesture detector handles the rest
                        let (dx, dy) = if code == RelativeAxisType::REL_X.0 {
                            (value as f64, 0.0)
                        } else {
                            (0.0, value as f64)
                        };
                        if let Some(gesture_ev) = state.gesture.accumulate(dx, dy, "evdev") {
                            let data = Some(MouseEventData {
                                delta_x: dx,
                                delta_y: dy,
                                ..Default::default()
                            });
                            state.callbacks.dispatch(gesture_ev, data);
                        }
                        return false; // suppress cursor during gesture
                    }
                    let _ = uinput.emit(&[*event]);
                    return false;
                }

                // Vertical scroll
                if code == RelativeAxisType::REL_WHEEL.0 || code == REL_WHEEL_HI_RES {
                    let _ = uinput.emit(&[*event]);
                    return false;
                }

                // Horizontal scroll
                if code == RelativeAxisType::REL_HWHEEL.0 || code == REL_HWHEEL_HI_RES {
                    let mut should_block = false;
                    // Only dispatch action from the low-res axis to avoid double-trigger
                    if code == RelativeAxisType::REL_HWHEEL.0 {
                        if value > 0 {
                            should_block =
                                state.blocked.contains(&MouseEventType::HScrollRight);
                            state.callbacks.dispatch(
                                MouseEvent::HScrollRight,
                                Some(MouseEventData {
                                    scroll_delta: value.unsigned_abs() as f64,
                                    ..Default::default()
                                }),
                            );
                        } else if value < 0 {
                            should_block =
                                state.blocked.contains(&MouseEventType::HScrollLeft);
                            state.callbacks.dispatch(
                                MouseEvent::HScrollLeft,
                                Some(MouseEventData {
                                    scroll_delta: value.unsigned_abs() as f64,
                                    ..Default::default()
                                }),
                            );
                        }
                    }

                    if should_block {
                        return false;
                    }
                    let _ = uinput.emit(&[*event]);
                    return false;
                }

                // All other relative axes — forward as-is
                let _ = uinput.emit(&[*event]);
                return false;
            }

            _ => {
                let _ = uinput.emit(&[*event]);
                return false;
            }
        }
    }

    fn listen_loop(device: &mut Device, uinput: &mut VirtualDevice, state: &Arc<Mutex<HookState>>) {
        use std::os::unix::io::AsRawFd;
        use nix::poll::{poll, PollFd, PollFlags};
        use std::os::fd::BorrowedFd;

        let raw_fd = device.as_raw_fd();
        loop {
            {
                if state.lock().unwrap().rescan_requested {
                    log::info!("[MouseHook] Rescan requested; leaving listen loop");
                    return;
                }
            }

            // Poll with 500 ms timeout so we can check rescan_requested regularly
            let borrowed = unsafe { BorrowedFd::borrow_raw(raw_fd) };
            let mut fds = [PollFd::new(borrowed, PollFlags::POLLIN)];
            let ready = poll(&mut fds, 500).unwrap_or(0);
            if ready == 0 {
                continue;
            }

            match device.fetch_events() {
                Ok(events) => {
                    let events: Vec<InputEvent> = events.collect();
                    let mut state_guard = state.lock().unwrap();
                    for event in &events {
                        handle_event(event, &mut state_guard, uinput);
                    }
                }
                Err(e) => {
                    log::warn!("[MouseHook] fetch_events error: {e}");
                    return;
                }
            }
        }
    }

    fn evdev_loop(state: Arc<Mutex<HookState>>, running: Arc<AtomicBool>) {
        while running.load(Ordering::SeqCst) {
            {
                let mut s = state.lock().unwrap();
                s.rescan_requested = false;
            }

            let mut device = match find_mouse_device() {
                Some(d) => d,
                None => {
                    log::warn!("[MouseHook] No mouse device found; retrying in 2 s");
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
            };

            // Build a virtual device mirroring all capabilities of the real one
            let mut builder = evdev::uinput::VirtualDeviceBuilder::new()
                .expect("Failed to create VirtualDeviceBuilder");
            builder = builder.name("Mouser Virtual Mouse");
            if let Some(keys) = device.supported_keys() {
                builder = builder.with_keys(keys).expect("with_keys");
            }
            if let Some(axes) = device.supported_relative_axes() {
                builder = builder.with_relative_axes(axes).expect("with_relative_axes");
            }
            let mut uinput = match builder.build() {
                Ok(u) => u,
                Err(e) => {
                    log::error!("[MouseHook] Failed to create UInput device: {e}");
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
            };

            if let Err(e) = device.grab() {
                log::error!("[MouseHook] Failed to grab device: {e}. Add user to 'input' group.");
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
            log::info!(
                "[MouseHook] Grabbed {}",
                device.name().unwrap_or("unknown")
            );

            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                listen_loop(&mut device, &mut uinput, &state);
            })) {
                log::warn!("[MouseHook] listen_loop panicked: {:?}", e);
            }

            // Ungrab and clean up
            let _ = device.ungrab();
            log::info!("[MouseHook] evdev device released");
            drop(uinput);

            if running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Public struct
    // ---------------------------------------------------------------------------

    pub struct LinuxMouseHook {
        state: Arc<Mutex<HookState>>,
        running: Arc<AtomicBool>,
        evdev_thread: Option<thread::JoinHandle<()>>,
    }

    impl LinuxMouseHook {
        pub fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(HookState::new())),
                running: Arc::new(AtomicBool::new(false)),
                evdev_thread: None,
            }
        }
    }

    impl MouseHook for LinuxMouseHook {
        fn register(
            &mut self,
            event_type: MouseEventType,
            cb: Box<dyn Fn(MouseEvent, Option<MouseEventData>) + Send + 'static>,
        ) {
            self.state.lock().unwrap().callbacks.register(event_type, cb);
        }

        fn block(&mut self, event_type: MouseEventType) {
            self.state.lock().unwrap().blocked.insert(event_type);
        }

        fn unblock(&mut self, event_type: MouseEventType) {
            self.state.lock().unwrap().blocked.remove(&event_type);
        }

        fn reset_bindings(&mut self) {
            let mut s = self.state.lock().unwrap();
            s.callbacks.clear();
            s.blocked.clear();
        }

        fn configure_gestures(&mut self, config: GestureConfig) {
            self.state.lock().unwrap().gesture.configure(config);
        }

        fn set_connection_change_callback(&mut self, cb: Box<dyn Fn(bool) + Send + 'static>) {
            self.state.lock().unwrap().connection_cb = Some(cb);
        }

        fn start(&mut self) -> anyhow::Result<()> {
            if self.running.load(Ordering::SeqCst) {
                return Ok(());
            }
            self.running.store(true, Ordering::SeqCst);

            let state = Arc::clone(&self.state);
            let running = Arc::clone(&self.running);
            let handle = thread::Builder::new()
                .name("MouseHook-evdev".into())
                .spawn(move || evdev_loop(state, running))?;
            self.evdev_thread = Some(handle);
            log::info!("[MouseHook] Linux evdev hook started");
            Ok(())
        }

        fn stop(&mut self) {
            if !self.running.load(Ordering::SeqCst) {
                return;
            }
            self.running.store(false, Ordering::SeqCst);
            // Request rescan so the listen_loop exits promptly
            self.state.lock().unwrap().rescan_requested = true;
            if let Some(handle) = self.evdev_thread.take() {
                let _ = handle.join();
            }
            log::info!("[MouseHook] Linux evdev hook stopped");
        }
    }

    impl crate::engine::MouseHookGestureInput for LinuxMouseHook {}
}

// ---------------------------------------------------------------------------
// Stub for non-Linux platforms
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;

    pub struct LinuxMouseHook;

    impl LinuxMouseHook {
        pub fn new() -> Self {
            Self
        }
    }

    impl MouseHook for LinuxMouseHook {
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
            Err(anyhow::anyhow!("Linux hook not available on this platform"))
        }
        fn stop(&mut self) {}
    }

    impl crate::engine::MouseHookGestureInput for LinuxMouseHook {}
}

pub use imp::LinuxMouseHook;
