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

fn parse_args() -> (HidBackend, bool, bool, bool) {
    let mut hid_backend = HidBackend::Auto;
    let mut start_hidden = false;
    let mut debug = false;
    let mut settings_only = false;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--start-hidden" => start_hidden = true,
            "--debug" => debug = true,
            "--settings" => settings_only = true,
            s if s.starts_with("--hid-backend=") => {
                hid_backend = match &s["--hid-backend=".len()..] {
                    "hidapi" => HidBackend::Hidapi,
                    "iokit" => HidBackend::IOKit,
                    _ => HidBackend::Auto,
                };
            }
            _ => {}
        }
    }
    (hid_backend, start_hidden, debug, settings_only)
}

fn main() {
    let (hid_backend, start_hidden, debug, settings_only) = parse_args();

    // ---- Settings-only mode: just open the GUI window, no daemon ----
    if settings_only {
        let cfg = config::load_config().unwrap_or_default();
        let ui_state = Arc::new(Mutex::new(UiState::default()));
        let config = Arc::new(Mutex::new(cfg));
        let (tx, _rx) = std::sync::mpsc::channel::<UiMessage>();
        ui::run_settings_window(tx, ui_state, config);
        return;
    }

    // ---- Daemon mode ----
    let _guard = match single_instance::SingleInstance::try_acquire() {
        Some(g) => g,
        None => { eprintln!("Mouser is already running."); std::process::exit(0); }
    };

    let cfg = config::load_config().unwrap_or_else(|e| {
        eprintln!("Config: {e}");
        config::Config::default()
    });

    let effective_debug = debug || cfg.settings.debug_mode;
    logging::setup_logging(effective_debug).ok();
    log::info!("Mouser v{} starting", env!("CARGO_PKG_VERSION"));

    #[cfg(target_os = "macos")]
    if !accessibility::is_process_trusted() {
        log::warn!("Accessibility not granted — requesting...");
        accessibility::request_accessibility();
    }

    let ui_state = Arc::new(Mutex::new({
        let mut s = UiState::default();
        s.current_profile = cfg.active_profile.clone();
        s.dpi = cfg.settings.dpi;
        s.smart_shift_mode = cfg.settings.smart_shift_mode.clone();
        s
    }));

    let (tx, rx) = std::sync::mpsc::channel::<UiMessage>();

    let mut engine = Engine::new(cfg, EngineConfig { hid_backend, debug }, Some(ui_state.clone()));
    engine.set_debug_callback(Box::new(|msg| log::debug!("[event] {msg}")));
    {
        let s2 = ui_state.clone();
        engine.set_gesture_callback(Box::new(move |evt, btn, action| {
            log::debug!("[gesture] {evt} {btn} → {action}");
            drop(s2.lock());
        }));
    }
    if let Err(e) = engine.start() { log::error!("Engine start: {e}"); }

    // Tray is created later inside the event loop (macOS needs NSApp running first)

    // Engine message loop (background thread)
    let cfg_for_ui = engine.get_config();
    {
        let mut eng = engine;
        let state = ui_state.clone();
        let ca = cfg_for_ui.clone();
        std::thread::spawn(move || {
            while let Ok(msg) = rx.recv() {
                handle_ui_message(msg, &mut eng, &state, &ca);
            }
        });
    }

    // Config file watcher: reload when settings subprocess changes config.json
    {
        let ca = cfg_for_ui.clone();
        let tx2 = tx.clone();
        std::thread::spawn(move || {
            let path = config::config_path().unwrap();
            let mut last_modified = std::fs::metadata(&path).ok()
                .and_then(|m| m.modified().ok());
            loop {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let current = std::fs::metadata(&path).ok()
                    .and_then(|m| m.modified().ok());
                if current != last_modified {
                    last_modified = current;
                    log::info!("Config file changed, reloading...");
                    if let Ok(new_cfg) = config::load_config() {
                        if let Ok(mut c) = ca.lock() {
                            *c = new_cfg;
                        }
                        // Signal engine to reload
                        let _ = tx2.send(UiMessage::ShowSettings); // reuse as reload signal
                    }
                }
            }
        });
    }

    log::info!("Mouser ready — click tray icon to open settings");

    // ---- Main event loop: winit (no window) + tray polling ----
    // winit properly drives the macOS Cocoa event loop which is required
    // for tray-icon menu events.  No window = no GPU = ~10 MB.
    run_event_loop(tx, ui_state, cfg_for_ui);
}

fn run_event_loop(
    tx: std::sync::mpsc::Sender<UiMessage>,
    ui_state: Arc<Mutex<UiState>>,
    _cfg: Arc<Mutex<config::Config>>,
) {
    use std::sync::atomic::Ordering;
    use winit::event_loop::EventLoop;

    let event_loop = EventLoop::new().expect("Failed to create event loop");

    let initial_state = ui_state.lock().unwrap().clone();
    let tray = TrayManager::new(tx.clone(), &initial_state).ok();
    if tray.is_some() {
        log::info!("Tray icon created");
    }

    // Track the settings subprocess so we don't spawn duplicates
    let mut settings_child: Option<std::process::Child> = None;

    event_loop.run(move |_event, target| {
        target.set_control_flow(winit::event_loop::ControlFlow::wait_duration(
            std::time::Duration::from_millis(200),
        ));

        // Check if settings subprocess has exited
        if let Some(ref mut child) = settings_child {
            match child.try_wait() {
                Ok(Some(_)) => {
                    log::info!("Settings subprocess exited");
                    settings_child = None;
                }
                _ => {}
            }
        }

        if let Some(ref tray) = tray {
            tray.poll_events();
            if let Ok(state) = ui_state.lock() {
                tray.update(&state);
            }
            if tray.show_settings_flag.swap(false, Ordering::Relaxed) {
                // Only spawn if not already running
                if settings_child.is_none() {
                    log::info!("Opening settings subprocess...");
                    if let Ok(exe) = std::env::current_exe() {
                        match std::process::Command::new(&exe).arg("--settings").spawn() {
                            Ok(child) => {
                                log::info!("Settings subprocess started (pid={})", child.id());
                                settings_child = Some(child);
                            }
                            Err(e) => log::error!("Failed to start settings: {e}"),
                        }
                    }
                } else {
                    log::debug!("Settings already open, ignoring click");
                }
            }
            if tray.quit_flag.load(Ordering::Relaxed) {
                // Kill settings subprocess if running
                if let Some(ref mut child) = settings_child {
                    let _ = child.kill();
                }
                std::process::exit(0);
            }
        }
    }).ok();
}

fn handle_ui_message(
    msg: UiMessage,
    eng: &mut Engine,
    state: &Arc<Mutex<UiState>>,
    ca: &Arc<Mutex<config::Config>>,
) {
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
            if let Ok(mut c) = ca.lock() { config::set_mapping(&mut c, &button, &action_id, &profile).ok(); }
            eng.reload_mappings();
        }
        UiMessage::SwitchProfile(name) => {
            if let Ok(mut c) = ca.lock() { c.active_profile = name.clone(); config::save_config(&c).ok(); }
            eng.reload_mappings();
            if let Ok(mut s) = state.lock() { s.current_profile = name; }
        }
        UiMessage::CreateProfile { name, label, copy_from } => {
            if let Ok(mut c) = ca.lock() { config::create_profile(&mut c, &name, &label, copy_from.as_deref()).ok(); }
        }
        UiMessage::DeleteProfile(name) => {
            if let Ok(mut c) = ca.lock() { config::delete_profile(&mut c, &name).ok(); }
            eng.reload_mappings();
        }
        UiMessage::SetStartAtLogin(v) => {
            startup::set_login_item(v).ok();
            if let Ok(mut c) = ca.lock() { c.settings.start_at_login = v; config::save_config(&c).ok(); }
        }
        UiMessage::SetStartMinimized(v) => {
            if let Ok(mut c) = ca.lock() { c.settings.start_minimized = v; config::save_config(&c).ok(); }
        }
        UiMessage::SetInvertHScroll(v) => {
            if let Ok(mut c) = ca.lock() { c.settings.invert_hscroll = v; config::save_config(&c).ok(); }
            eng.reload_mappings();
        }
        UiMessage::SetInvertVScroll(v) => {
            if let Ok(mut c) = ca.lock() { c.settings.invert_vscroll = v; config::save_config(&c).ok(); }
        }
        UiMessage::SetHScrollThreshold(v) => {
            if let Ok(mut c) = ca.lock() { c.settings.hscroll_threshold = v; config::save_config(&c).ok(); }
            eng.reload_mappings();
        }
        UiMessage::SetLanguage(lang) => {
            if let Ok(mut c) = ca.lock() { c.settings.language = lang; config::save_config(&c).ok(); }
        }
        UiMessage::SetDebugMode(v) => {
            eng.set_debug_enabled(v);
            if let Ok(mut c) = ca.lock() { c.settings.debug_mode = v; config::save_config(&c).ok(); }
        }
        UiMessage::ShowSettings => {
            // Used as config-reload signal from file watcher
            eng.reload_mappings();
        }
        UiMessage::HideSettings => {}
    }
}
