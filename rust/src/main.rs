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
use ui::{MainApp, TrayManager, UiMessage, UiState};

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

    // ---- Logging ----
    if let Err(e) = logging::setup_logging(debug) {
        eprintln!("Failed to setup logging: {e}");
    }
    log::info!("Mouser v{} starting", env!("CARGO_PKG_VERSION"));

    // ---- macOS accessibility check ----
    #[cfg(target_os = "macos")]
    {
        if !accessibility::is_process_trusted() {
            log::warn!("Accessibility permission not granted — requesting...");
            accessibility::request_accessibility();
        }
    }

    // ---- Load config ----
    let cfg = match config::load_config() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to load config: {e}");
            config::Config::default()
        }
    };

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
    let engine_cfg = EngineConfig {
        hid_backend,
        debug,
    };
    let mut engine = Engine::new(cfg, engine_cfg);

    // Wire debug callbacks
    {
        engine.set_debug_callback(Box::new(move |msg| {
            log::debug!("[event] {msg}");
        }));

        let state2 = ui_state.clone();
        engine.set_gesture_callback(Box::new(move |evt, btn, action| {
            log::debug!("[gesture] {evt} {btn} → {action}");
            let _ = (state2.lock(), evt, btn, action);
        }));
    }

    // Start engine background threads
    if let Err(e) = engine.start() {
        log::error!("Engine failed to start: {e}");
    }

    // ---- Tray ----
    let initial_state = ui_state.lock().unwrap().clone();
    let tray = TrayManager::new(tx.clone(), &initial_state).ok();
    if tray.is_none() {
        log::warn!("System tray failed to initialise — running without tray");
    }

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
                        if let Ok(mut s) = state.lock() {
                            s.dpi = dpi;
                        }
                    }
                    UiMessage::SetSmartShift(mode) => {
                        eng.set_smart_shift(&mode);
                        if let Ok(mut s) = state.lock() {
                            s.smart_shift_mode = mode;
                        }
                    }
                    UiMessage::SetMapping { profile, button, action_id } => {
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            config::set_mapping(&mut cfg, &button, &action_id, &profile).ok();
                        }
                        eng.reload_mappings();
                    }
                    UiMessage::SwitchProfile(name) => {
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            cfg.active_profile = name.clone();
                            config::save_config(&cfg).ok();
                        }
                        eng.reload_mappings();
                        if let Ok(mut s) = state.lock() {
                            s.current_profile = name;
                        }
                    }
                    UiMessage::CreateProfile { name, label, copy_from } => {
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            config::create_profile(
                                &mut cfg,
                                &name,
                                &label,
                                copy_from.as_deref(),
                            ).ok();
                        }
                    }
                    UiMessage::DeleteProfile(name) => {
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            config::delete_profile(&mut cfg, &name).ok();
                        }
                        eng.reload_mappings();
                    }
                    UiMessage::SetStartAtLogin(v) => {
                        startup::set_login_item(v).ok();
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            cfg.settings.start_at_login = v;
                            config::save_config(&cfg).ok();
                        }
                    }
                    UiMessage::SetStartMinimized(v) => {
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            cfg.settings.start_minimized = v;
                            config::save_config(&cfg).ok();
                        }
                    }
                    UiMessage::SetInvertHScroll(v) => {
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            cfg.settings.invert_hscroll = v;
                            config::save_config(&cfg).ok();
                        }
                        eng.reload_mappings();
                    }
                    UiMessage::SetInvertVScroll(v) => {
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            cfg.settings.invert_vscroll = v;
                            config::save_config(&cfg).ok();
                        }
                    }
                    UiMessage::SetHScrollThreshold(v) => {
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            cfg.settings.hscroll_threshold = v;
                            config::save_config(&cfg).ok();
                        }
                        eng.reload_mappings();
                    }
                    UiMessage::SetLanguage(lang) => {
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            cfg.settings.language = lang;
                            config::save_config(&cfg).ok();
                        }
                    }
                    UiMessage::SetDebugMode(v) => {
                        eng.set_debug_enabled(v);
                        if let Ok(mut cfg) = cfg_arc.lock() {
                            cfg.settings.debug_mode = v;
                            config::save_config(&cfg).ok();
                        }
                    }
                    UiMessage::ShowSettings | UiMessage::HideSettings => {
                        // handled in main thread via eframe/tray
                    }
                }
            }
        });
    }

    // ---- Main thread: eframe drives the native event loop ----
    // This is required on macOS for tray-icon to receive menu events.
    log::info!("Mouser ready (start_minimized={})", start_minimized);

    let app = MainApp::new(
        tx,
        ui_state,
        cfg_for_ui,
        tray,
        !start_minimized,
    );

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mouser Settings")
            .with_inner_size([900.0, 620.0])
            .with_min_inner_size([780.0, 520.0])
            .with_visible(!start_minimized),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Mouser",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    ) {
        log::error!("UI error: {e}");
    }
}
