mod config;
mod devices;
mod device_layouts;
mod actions;
mod locale;
mod hid_gesture;
mod key_simulator;
mod mouse_hook;
mod app_detector;
mod app_catalog;
mod accessibility;
mod startup;
mod logging;
mod single_instance;
mod ui;
mod engine;

#[cfg(target_os = "macos")]
mod macos_objc {
    extern "C" {
        pub fn objc_getClass(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
        pub fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
        #[link_name = "objc_msgSend"]
        pub fn msg_send_void(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
        #[link_name = "objc_msgSend"]
        pub fn msg_send_i64(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, val: i64) -> i64;
        #[link_name = "objc_msgSend"]
        pub fn msg_send_f64(cls: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, val: f64) -> *mut std::ffi::c_void;
        #[link_name = "objc_msgSend"]
        pub fn msg_send_obj(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, arg: *mut std::ffi::c_void);
    }
}

use std::sync::{Arc, Mutex};

use engine::{Engine, EngineConfig};
use hid_gesture::HidBackend;
use ui::{TrayManager, UiMessage, UiState};

fn parse_args() -> (HidBackend, bool, bool, bool) {
    let mut hid_backend = HidBackend::Auto;
    let mut start_hidden = false;
    let mut debug = false;
    let mut open_settings = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--start-hidden" => start_hidden = true,
            "--debug" => debug = true,
            "--settings" => open_settings = true,
            s if s.starts_with("--hid-backend=") => {
                let val = &s["--hid-backend=".len()..];
                hid_backend = match val {
                    "hidapi" => HidBackend::Hidapi,
                    "iokit" => HidBackend::IOKit,
                    _ => HidBackend::Auto,
                };
            }
            _ => {}
        }
    }
    (hid_backend, start_hidden, debug, open_settings)
}

fn main() {
    let (hid_backend, start_hidden, debug, open_settings_flag) = parse_args();

    // ---- Settings-only mode: just open the UI, no daemon ----
    if open_settings_flag {
        let cfg = config::load_config().unwrap_or_default();
        let ui_state = Arc::new(Mutex::new(UiState::default()));
        let config = Arc::new(Mutex::new(cfg));
        let (tx, _rx) = std::sync::mpsc::channel::<UiMessage>();
        ui::run_settings_window(tx, ui_state, config);
        return;
    }

    // ---- Single instance guard ----
    let _instance_guard = match single_instance::SingleInstance::try_acquire() {
        Some(g) => g,
        None => {
            eprintln!("Mouser is already running.");
            std::process::exit(0);
        }
    };

    // ---- Load config ----
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config: {e}");
            config::Config::default()
        }
    };

    // ---- Logging ----
    let effective_debug = debug || cfg.settings.debug_mode;
    if let Err(e) = logging::setup_logging(effective_debug) {
        eprintln!("Failed to setup logging: {e}");
    }
    log::info!("Mouser v{} starting (debug={})", env!("CARGO_PKG_VERSION"), effective_debug);

    // ---- macOS accessibility check ----
    #[cfg(target_os = "macos")]
    {
        if !accessibility::is_process_trusted() {
            log::warn!("Accessibility permission not granted — requesting...");
            accessibility::request_accessibility();
        }
    }

    // ---- Shared UI state ----
    let ui_state = Arc::new(Mutex::new({
        let mut s = UiState::default();
        s.current_profile = cfg.active_profile.clone();
        s.dpi = cfg.settings.dpi;
        s.smart_shift_mode = cfg.settings.smart_shift_mode.clone();
        s
    }));

    // ---- Channel: UI → Engine ----
    let (tx, rx) = std::sync::mpsc::channel::<UiMessage>();

    // ---- Engine ----
    let mut engine = Engine::new(
        cfg,
        EngineConfig { hid_backend, debug },
        Some(ui_state.clone()),
    );
    engine.set_debug_callback(Box::new(|msg| log::debug!("[event] {msg}")));
    {
        let s2 = ui_state.clone();
        engine.set_gesture_callback(Box::new(move |evt, btn, action| {
            log::debug!("[gesture] {evt} {btn} → {action}");
            drop(s2.lock());
        }));
    }
    if let Err(e) = engine.start() {
        log::error!("Engine failed to start: {e}");
    }

    // ---- Tray ----
    let initial_state = ui_state.lock().unwrap().clone();
    let tray = match TrayManager::new(tx.clone(), &initial_state) {
        Ok(t) => { log::info!("Tray icon created"); Some(t) }
        Err(e) => { log::error!("Tray failed: {e}"); None }
    };

    // ---- Engine message loop (background thread) ----
    let cfg_for_ui = engine.get_config();
    {
        let mut eng = engine;
        let state = ui_state.clone();
        let cfg_arc = cfg_for_ui.clone();
        std::thread::spawn(move || {
            while let Ok(msg) = rx.recv() {
                match msg {
                    UiMessage::Quit => { eng.stop(); std::process::exit(0); }
                    UiMessage::SetDpi(dpi) => {
                        eng.set_dpi(dpi);
                        if let Ok(mut s) = state.lock() { s.dpi = dpi; }
                    }
                    UiMessage::SetSmartShift(mode) => {
                        eng.set_smart_shift(&mode);
                        if let Ok(mut s) = state.lock() { s.smart_shift_mode = mode; }
                    }
                    UiMessage::SetMapping { profile, button, action_id } => {
                        if let Ok(mut c) = cfg_arc.lock() { config::set_mapping(&mut c, &button, &action_id, &profile).ok(); }
                        eng.reload_mappings();
                    }
                    UiMessage::SwitchProfile(name) => {
                        if let Ok(mut c) = cfg_arc.lock() { c.active_profile = name.clone(); config::save_config(&c).ok(); }
                        eng.reload_mappings();
                        if let Ok(mut s) = state.lock() { s.current_profile = name; }
                    }
                    UiMessage::CreateProfile { name, label, copy_from } => {
                        if let Ok(mut c) = cfg_arc.lock() { config::create_profile(&mut c, &name, &label, copy_from.as_deref()).ok(); }
                    }
                    UiMessage::DeleteProfile(name) => {
                        if let Ok(mut c) = cfg_arc.lock() { config::delete_profile(&mut c, &name).ok(); }
                        eng.reload_mappings();
                    }
                    UiMessage::SetStartAtLogin(v) => {
                        startup::set_login_item(v).ok();
                        if let Ok(mut c) = cfg_arc.lock() { c.settings.start_at_login = v; config::save_config(&c).ok(); }
                    }
                    UiMessage::SetStartMinimized(v) => {
                        if let Ok(mut c) = cfg_arc.lock() { c.settings.start_minimized = v; config::save_config(&c).ok(); }
                    }
                    UiMessage::SetInvertHScroll(v) => {
                        if let Ok(mut c) = cfg_arc.lock() { c.settings.invert_hscroll = v; config::save_config(&c).ok(); }
                        eng.reload_mappings();
                    }
                    UiMessage::SetInvertVScroll(v) => {
                        if let Ok(mut c) = cfg_arc.lock() { c.settings.invert_vscroll = v; config::save_config(&c).ok(); }
                    }
                    UiMessage::SetHScrollThreshold(v) => {
                        if let Ok(mut c) = cfg_arc.lock() { c.settings.hscroll_threshold = v; config::save_config(&c).ok(); }
                        eng.reload_mappings();
                    }
                    UiMessage::SetLanguage(lang) => {
                        if let Ok(mut c) = cfg_arc.lock() { c.settings.language = lang; config::save_config(&c).ok(); }
                    }
                    UiMessage::SetDebugMode(v) => {
                        eng.set_debug_enabled(v);
                        if let Ok(mut c) = cfg_arc.lock() { c.settings.debug_mode = v; config::save_config(&c).ok(); }
                    }
                    UiMessage::ShowSettings | UiMessage::HideSettings => {}
                }
            }
        });
    }

    log::info!("Mouser ready (start_minimized={})", start_hidden);

    // ---- Main event loop ----
    // On macOS, tray-icon needs a Cocoa event loop.  We use [NSApp run]
    // which properly handles NSMenu events.  A GCD timer polls the tray
    // channel and opens the settings window (via launching a subprocess
    // with --settings flag) when requested.
    //
    // Memory: ~10 MB daemon.  Settings window runs as the same binary
    // with --settings and is a separate eframe instance.

    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::Ordering;

        unsafe {
            extern "C" {
                static _dispatch_main_q: std::ffi::c_void;
                fn dispatch_source_create(ty: *const std::ffi::c_void, handle: usize, mask: usize, queue: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
                fn dispatch_source_set_timer(source: *mut std::ffi::c_void, start: u64, interval: u64, leeway: u64);
                fn dispatch_source_set_event_handler_f(source: *mut std::ffi::c_void, handler: extern "C" fn(*mut std::ffi::c_void));
                fn dispatch_set_context(obj: *mut std::ffi::c_void, ctx: *mut std::ffi::c_void);
                fn dispatch_resume(obj: *mut std::ffi::c_void);
                static _dispatch_source_type_timer: std::ffi::c_void;
                fn dispatch_time(when: u64, delta: i64) -> u64;
            }

            struct Ctx { tray: Option<TrayManager>, ui_state: Arc<Mutex<UiState>> }
            static mut CTX: *mut Ctx = std::ptr::null_mut();

            extern "C" fn tick(_: *mut std::ffi::c_void) {
                unsafe {
                    if CTX.is_null() { return; }
                    let ctx = &*CTX;
                    if let Some(ref tray) = ctx.tray {
                        tray.poll_events();
                        if let Ok(state) = ctx.ui_state.lock() { tray.update(&state); }
                        if tray.show_settings_flag.swap(false, Ordering::Relaxed) {
                            log::info!("Opening settings subprocess...");
                            let exe = std::env::current_exe().unwrap_or_default();
                            let _ = std::process::Command::new(exe).arg("--settings").spawn();
                        }
                        if tray.quit_flag.load(Ordering::Relaxed) {
                            std::process::exit(0);
                        }
                    }
                }
            }

            CTX = Box::into_raw(Box::new(Ctx { tray, ui_state }));

            let queue = &_dispatch_main_q as *const _ as *mut std::ffi::c_void;
            let timer = dispatch_source_create(&_dispatch_source_type_timer as *const _ as *const _, 0, 0, queue);
            dispatch_source_set_timer(timer, dispatch_time(0, 0), 100_000_000, 10_000_000);
            dispatch_source_set_event_handler_f(timer, tick);
            dispatch_set_context(timer, CTX as *mut _);
            dispatch_resume(timer);

            use cocoa::appkit::{NSApp, NSApplication};
            NSApp().run();
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        use std::sync::atomic::Ordering;
        loop {
            if let Some(ref tray) = tray {
                tray.poll_events();
                if let Ok(state) = ui_state.lock() { tray.update(&state); }
                if tray.show_settings_flag.swap(false, Ordering::Relaxed) {
                    ui::run_settings_window(tx.clone(), ui_state.clone(), cfg_for_ui.clone());
                }
                if tray.quit_flag.load(Ordering::Relaxed) { break; }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
