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

use std::sync::{Arc, Mutex};

use engine::{Engine, EngineConfig};
use hid_gesture::HidBackend;
use ui::{TrayManager, UiMessage, UiState};

fn parse_args() -> (HidBackend, bool, bool) {
    let mut hid_backend = HidBackend::Auto;
    let mut start_hidden = false;
    let mut debug = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--start-hidden" => start_hidden = true,
            "--debug" => debug = true,
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
    (hid_backend, start_hidden, debug)
}

fn main() {
    let (hid_backend, start_hidden, debug) = parse_args();

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

    let start_minimized = cfg.settings.start_minimized || start_hidden;

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
    let tray = TrayManager::new(tx.clone(), &initial_state).ok();

    // ---- Engine message loop (background thread) ----
    let cfg_for_ui = engine.get_config();
    {
        let mut eng = engine;
        let state = ui_state.clone();
        let cfg_arc = cfg_for_ui.clone();
        std::thread::spawn(move || {
            while let Ok(msg) = rx.recv() {
                match msg {
                    UiMessage::Quit => {
                        eng.stop();
                        std::process::exit(0);
                    }
                    UiMessage::SetDpi(dpi) => {
                        eng.set_dpi(dpi);
                        if let Ok(mut s) = state.lock() { s.dpi = dpi; }
                    }
                    UiMessage::SetSmartShift(mode) => {
                        eng.set_smart_shift(&mode);
                        if let Ok(mut s) = state.lock() { s.smart_shift_mode = mode; }
                    }
                    UiMessage::SetMapping { profile, button, action_id } => {
                        if let Ok(mut c) = cfg_arc.lock() {
                            config::set_mapping(&mut c, &button, &action_id, &profile).ok();
                        }
                        eng.reload_mappings();
                    }
                    UiMessage::SwitchProfile(name) => {
                        if let Ok(mut c) = cfg_arc.lock() {
                            c.active_profile = name.clone();
                            config::save_config(&c).ok();
                        }
                        eng.reload_mappings();
                        if let Ok(mut s) = state.lock() { s.current_profile = name; }
                    }
                    UiMessage::CreateProfile { name, label, copy_from } => {
                        if let Ok(mut c) = cfg_arc.lock() {
                            config::create_profile(&mut c, &name, &label, copy_from.as_deref()).ok();
                        }
                    }
                    UiMessage::DeleteProfile(name) => {
                        if let Ok(mut c) = cfg_arc.lock() {
                            config::delete_profile(&mut c, &name).ok();
                        }
                        eng.reload_mappings();
                    }
                    UiMessage::SetStartAtLogin(v) => {
                        startup::set_login_item(v).ok();
                        if let Ok(mut c) = cfg_arc.lock() {
                            c.settings.start_at_login = v;
                            config::save_config(&c).ok();
                        }
                    }
                    UiMessage::SetStartMinimized(v) => {
                        if let Ok(mut c) = cfg_arc.lock() {
                            c.settings.start_minimized = v;
                            config::save_config(&c).ok();
                        }
                    }
                    UiMessage::SetInvertHScroll(v) => {
                        if let Ok(mut c) = cfg_arc.lock() {
                            c.settings.invert_hscroll = v;
                            config::save_config(&c).ok();
                        }
                        eng.reload_mappings();
                    }
                    UiMessage::SetInvertVScroll(v) => {
                        if let Ok(mut c) = cfg_arc.lock() {
                            c.settings.invert_vscroll = v;
                            config::save_config(&c).ok();
                        }
                    }
                    UiMessage::SetHScrollThreshold(v) => {
                        if let Ok(mut c) = cfg_arc.lock() {
                            c.settings.hscroll_threshold = v;
                            config::save_config(&c).ok();
                        }
                        eng.reload_mappings();
                    }
                    UiMessage::SetLanguage(lang) => {
                        if let Ok(mut c) = cfg_arc.lock() {
                            c.settings.language = lang;
                            config::save_config(&c).ok();
                        }
                    }
                    UiMessage::SetDebugMode(v) => {
                        eng.set_debug_enabled(v);
                        if let Ok(mut c) = cfg_arc.lock() {
                            c.settings.debug_mode = v;
                            config::save_config(&c).ok();
                        }
                    }
                    UiMessage::ShowSettings | UiMessage::HideSettings => {}
                }
            }
        });
    }

    log::info!("Mouser ready (start_minimized={})", start_minimized);

    // ---- macOS: set as accessory app (no dock icon, tray only) ----
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn objc_getClass(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
            fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
        }
        // objc_msgSend with different signatures
        extern "C" {
            #[link_name = "objc_msgSend"]
            fn msg_send_void(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
            #[link_name = "objc_msgSend"]
            fn msg_send_i64(obj: *mut std::ffi::c_void, sel: *mut std::ffi::c_void, val: i64) -> i64;
        }
        unsafe {
            let cls = objc_getClass(b"NSApplication\0".as_ptr() as *const _);
            let shared_app = sel_registerName(b"sharedApplication\0".as_ptr() as *const _);
            let app = msg_send_void(cls, shared_app);
            // NSApplicationActivationPolicyAccessory = 1 (no dock icon)
            let set_policy = sel_registerName(b"setActivationPolicy:\0".as_ptr() as *const _);
            msg_send_i64(app, set_policy, 1);
        }
    }

    // ---- Main thread: lightweight tray-only event loop ----
    //
    // No eframe runs here.  Memory: ~7 MB.  eframe is only launched
    // when "Settings" is clicked in the tray, and fully destroyed when
    // the window is closed (all GPU/WebKit resources freed).

    loop {
        if let Some(ref tray) = tray {
            tray.poll_events();
            if let Ok(state) = ui_state.lock() {
                tray.update(&state);
            }
            if tray.show_settings_flag.swap(false, std::sync::atomic::Ordering::Relaxed) {
                open_settings(tx.clone(), ui_state.clone(), cfg_for_ui.clone());
            }
            if tray.quit_flag.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
        }

        // Pump the native event loop for tray menu delivery.
        #[cfg(target_os = "macos")]
        {
            extern "C" {
                fn CFRunLoopRunInMode(mode: *const std::ffi::c_void, seconds: f64, returnAfterSourceHandled: u8) -> i32;
                static kCFRunLoopDefaultMode: *const std::ffi::c_void;
            }
            unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.1, 0); }
        }

        #[cfg(not(target_os = "macos"))]
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// Open the settings window (blocks until closed, then all GPU resources are freed).
fn open_settings(
    tx: std::sync::mpsc::Sender<UiMessage>,
    ui_state: Arc<Mutex<UiState>>,
    config: Arc<Mutex<config::Config>>,
) {
    ui::run_settings_window(tx, ui_state, config);
}
