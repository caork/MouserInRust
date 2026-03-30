//! Windows low-level mouse hook via `SetWindowsHookExW` (WH_MOUSE_LL).
//!
//! Intercepts `WM_XBUTTONDOWN/UP`, `WM_MBUTTONDOWN/UP`, and `WM_MOUSEHWHEEL`
//! before they reach any application.  Blocking is achieved by returning 1
//! from the hook callback instead of calling `CallNextHookEx`.
//!
//! A dedicated OS thread runs the Win32 message pump required to keep a
//! low-level hook alive.
#![allow(dead_code)]

use super::{
    CallbackMap, GestureConfig, GestureDetector, MouseEvent, MouseEventData, MouseEventType,
    MouseHook,
};

// ---------------------------------------------------------------------------
// Real Windows implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;

    use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, DispatchMessageW, MSG, WH_MOUSE_LL, WM_MBUTTONDOWN, WM_MBUTTONUP,
        WM_MOUSEHWHEEL, WM_QUIT, WM_XBUTTONDOWN, WM_XBUTTONUP, HHOOK, MSLLHOOKSTRUCT,
    };

    // HHOOK contains a raw *mut c_void and is !Send in the windows crate.
    // We only ever access the handle through a Mutex, so this is safe.
    struct SendHHOOK(HHOOK);
    unsafe impl Send for SendHHOOK {}
    unsafe impl Sync for SendHHOOK {}

    // Windows constants not re-exported by the `windows` crate helpers
    const HC_ACTION: i32 = 0;
    const XBUTTON1: u16 = 0x0001;
    const XBUTTON2: u16 = 0x0002;
    /// Flag set when the event was injected (e.g. by our own scroll injector).
    const INJECTED_FLAG: u32 = 0x00000001;

    /// Extract the HIWORD of a DWORD as a signed i16.
    #[inline]
    fn hiword(dword: u32) -> i16 {
        ((dword >> 16) & 0xFFFF) as i16
    }

    // ------------------------------------------------------------------
    // Global hook state (accessible from the static hook procedure)
    // ------------------------------------------------------------------

    struct HookState {
        callbacks: CallbackMap,
        blocked: HashSet<MouseEventType>,
        gesture: GestureDetector,
        connection_cb: Option<Box<dyn Fn(bool) + Send + 'static>>,
    }

    impl HookState {
        fn new() -> Self {
            Self {
                callbacks: CallbackMap::new(),
                blocked: HashSet::new(),
                gesture: GestureDetector::new(GestureConfig::default()),
                connection_cb: None,
            }
        }
    }

    // OnceLock so the static is only initialised once; the Arc<Mutex<…>>
    // inside can be cloned and shared with the hook thread.
    static HOOK_STATE: OnceLock<Arc<Mutex<HookState>>> = OnceLock::new();

    fn get_hook_state() -> &'static Arc<Mutex<HookState>> {
        HOOK_STATE.get_or_init(|| Arc::new(Mutex::new(HookState::new())))
    }

    // The HHOOK handle used inside the callback to call CallNextHookEx.
    // Must be set before the message pump starts.
    // Wrapped in SendHHOOK because HHOOK contains *mut c_void and is !Send.
    static HOOK_HANDLE: OnceLock<Mutex<Option<SendHHOOK>>> = OnceLock::new();
    fn hook_handle_lock() -> &'static Mutex<Option<SendHHOOK>> {
        HOOK_HANDLE.get_or_init(|| Mutex::new(None))
    }

    // ------------------------------------------------------------------
    // Low-level hook procedure (must be a plain `extern "system" fn`)
    // ------------------------------------------------------------------

    unsafe extern "system" fn ll_mouse_proc(
        n_code: i32,
        w_param: WPARAM,
        l_param: LPARAM,
    ) -> LRESULT {
        if n_code != HC_ACTION {
            let hh = hook_handle_lock()
                .lock()
                .unwrap()
                .as_ref()
                .map(|s| s.0);
            return CallNextHookEx(hh, n_code, w_param, l_param);
        }

        let info = &*(l_param.0 as *const MSLLHOOKSTRUCT);
        let mouse_data = info.mouseData;
        let flags = info.flags;

        // Skip injected events (e.g. those produced by our own scroll injector).
        if flags & INJECTED_FLAG != 0 {
            let hh = hook_handle_lock()
                .lock()
                .unwrap()
                .as_ref()
                .map(|s| s.0);
            return CallNextHookEx(hh, n_code, w_param, l_param);
        }

        let msg = w_param.0 as u32;

        let mut fire_event: Option<MouseEvent> = None;
        let mut should_block = false;
        let mut scroll_data: Option<MouseEventData> = None;
        let state_arc = get_hook_state().clone();

        {
            // Use try_lock to avoid blocking the hook thread; if the lock is
            // contended we simply pass the event through.
            if let Ok(mut state) = state_arc.try_lock() {
                match msg {
                    WM_XBUTTONDOWN => {
                        let xbtn = hiword(mouse_data) as u16;
                        if xbtn == XBUTTON1 {
                            fire_event = Some(MouseEvent::XButton1Down);
                            should_block = state.blocked.contains(&MouseEventType::XButton1Down);
                        } else if xbtn == XBUTTON2 {
                            fire_event = Some(MouseEvent::XButton2Down);
                            should_block = state.blocked.contains(&MouseEventType::XButton2Down);
                        }
                    }
                    WM_XBUTTONUP => {
                        let xbtn = hiword(mouse_data) as u16;
                        if xbtn == XBUTTON1 {
                            fire_event = Some(MouseEvent::XButton1Up);
                            should_block = state.blocked.contains(&MouseEventType::XButton1Up);
                        } else if xbtn == XBUTTON2 {
                            fire_event = Some(MouseEvent::XButton2Up);
                            should_block = state.blocked.contains(&MouseEventType::XButton2Up);
                        }
                    }
                    WM_MBUTTONDOWN => {
                        fire_event = Some(MouseEvent::MiddleDown);
                        should_block = state.blocked.contains(&MouseEventType::MiddleDown);
                    }
                    WM_MBUTTONUP => {
                        fire_event = Some(MouseEvent::MiddleUp);
                        should_block = state.blocked.contains(&MouseEventType::MiddleUp);
                    }
                    WM_MOUSEHWHEEL => {
                        let delta = hiword(mouse_data) as i32;
                        if delta > 0 {
                            fire_event = Some(MouseEvent::HScrollLeft);
                            should_block = state.blocked.contains(&MouseEventType::HScrollLeft);
                            scroll_data = Some(MouseEventData {
                                scroll_delta: delta.abs() as f64,
                                ..Default::default()
                            });
                        } else if delta < 0 {
                            fire_event = Some(MouseEvent::HScrollRight);
                            should_block = state.blocked.contains(&MouseEventType::HScrollRight);
                            scroll_data = Some(MouseEventData {
                                scroll_delta: delta.unsigned_abs() as f64,
                                ..Default::default()
                            });
                        }
                    }
                    _ => {}
                }

                if let Some(ev) = fire_event {
                    state.callbacks.dispatch(ev, scroll_data);
                }
            }
        }

        if should_block {
            return LRESULT(1);
        }

        let hh = hook_handle_lock()
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.0);
        CallNextHookEx(hh, n_code, w_param, l_param)
    }

    // ------------------------------------------------------------------
    // Hook thread
    // ------------------------------------------------------------------

    fn run_hook_thread(ready_tx: std::sync::mpsc::SyncSender<bool>, thread_id_slot: Arc<Mutex<u32>>) {
        unsafe {
            let tid = GetCurrentThreadId();
            *thread_id_slot.lock().unwrap() = tid;

            let hmod: HINSTANCE = GetModuleHandleW(None)
                .map(|m| m.into())
                .unwrap_or_default();
            let hook = match SetWindowsHookExW(WH_MOUSE_LL, Some(ll_mouse_proc), Some(hmod), 0) {
                Ok(h) => h,
                Err(e) => {
                    log::error!("[MouseHook] SetWindowsHookExW failed: {e}");
                    let _ = ready_tx.send(false);
                    return;
                }
            };

            *hook_handle_lock().lock().unwrap() = Some(SendHHOOK(hook));
            log::info!("[MouseHook] Hook installed successfully");
            let _ = ready_tx.send(true);

            // Message pump — required to keep WH_MOUSE_LL alive
            let mut msg = MSG::default();
            loop {
                let ret = GetMessageW(&mut msg, None, 0, 0);
                match ret.0 {
                    0 | -1 => break,
                    _ => {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            }

            let _ = UnhookWindowsHookEx(hook);
            *hook_handle_lock().lock().unwrap() = None;
            log::info!("[MouseHook] Hook removed");
        }
    }

    // ------------------------------------------------------------------
    // Public struct
    // ------------------------------------------------------------------

    pub struct WindowsMouseHook {
        running: AtomicBool,
        thread_id: Arc<Mutex<u32>>,
    }

    impl WindowsMouseHook {
        pub fn new() -> Self {
            // Ensure the global state is initialised early
            let _ = get_hook_state();
            Self {
                running: AtomicBool::new(false),
                thread_id: Arc::new(Mutex::new(0)),
            }
        }
    }

    impl MouseHook for WindowsMouseHook {
        fn register(
            &mut self,
            event_type: MouseEventType,
            cb: Box<dyn Fn(MouseEvent, Option<MouseEventData>) + Send + 'static>,
        ) {
            get_hook_state().lock().unwrap().callbacks.register(event_type, cb);
        }

        fn block(&mut self, event_type: MouseEventType) {
            get_hook_state().lock().unwrap().blocked.insert(event_type);
        }

        fn unblock(&mut self, event_type: MouseEventType) {
            get_hook_state().lock().unwrap().blocked.remove(&event_type);
        }

        fn reset_bindings(&mut self) {
            let mut state = get_hook_state().lock().unwrap();
            state.callbacks.clear();
            state.blocked.clear();
        }

        fn configure_gestures(&mut self, config: GestureConfig) {
            get_hook_state().lock().unwrap().gesture.configure(config);
        }

        fn set_connection_change_callback(&mut self, cb: Box<dyn Fn(bool) + Send + 'static>) {
            get_hook_state().lock().unwrap().connection_cb = Some(cb);
        }

        fn start(&mut self) -> anyhow::Result<()> {
            if self.running.load(std::sync::atomic::Ordering::SeqCst) {
                return Ok(());
            }

            let tid_slot = Arc::clone(&self.thread_id);
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("MouseHook-win32".into())
                .spawn(move || run_hook_thread(tx, tid_slot))?;

            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(true) => {
                    self.running.store(true, Ordering::SeqCst);
                    Ok(())
                }
                Ok(false) => Err(anyhow::anyhow!("Hook installation failed")),
                Err(_) => Err(anyhow::anyhow!("Hook startup timed out")),
            }
        }

        fn stop(&mut self) {
            if !self.running.load(std::sync::atomic::Ordering::SeqCst) {
                return;
            }
            self.running.store(false, std::sync::atomic::Ordering::SeqCst);
            let tid = *self.thread_id.lock().unwrap();
            if tid != 0 {
                unsafe {
                    let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
                }
            }
        }
    }

    impl crate::engine::MouseHookGestureInput for WindowsMouseHook {}
} // mod imp

// ---------------------------------------------------------------------------
// Stub for non-Windows platforms (keeps the module compilable everywhere)
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "windows"))]
mod imp {
    use super::*;

    pub struct WindowsMouseHook;

    impl WindowsMouseHook {
        pub fn new() -> Self {
            Self
        }
    }

    impl MouseHook for WindowsMouseHook {
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
            Err(anyhow::anyhow!("Windows hook not available on this platform"))
        }
        fn stop(&mut self) {}
    }

    impl crate::engine::MouseHookGestureInput for WindowsMouseHook {}
}

pub use imp::WindowsMouseHook;
