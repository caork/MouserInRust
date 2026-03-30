#[allow(dead_code)]

mod tray;
mod settings_window;

pub use tray::TrayManager;
pub use settings_window::SettingsApp;

use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;

use crate::config::Config;

/// Snapshot of engine state for display in the UI.
#[derive(Clone)]
pub struct UiState {
    pub device_name: String,
    pub battery_pct: Option<u8>,
    pub current_profile: String,
    pub enabled: bool,
    pub dpi: u32,
    pub smart_shift_mode: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            device_name: "No device".to_string(),
            battery_pct: None,
            current_profile: "default".to_string(),
            enabled: true,
            dpi: 1000,
            smart_shift_mode: "ratchet".to_string(),
        }
    }
}

/// Messages sent from the UI to the engine.
#[allow(dead_code)]
pub enum UiMessage {
    SetMapping { profile: String, button: String, action_id: String },
    SetDpi(u32),
    SetSmartShift(String),
    CreateProfile { name: String, label: String, copy_from: Option<String> },
    DeleteProfile(String),
    SwitchProfile(String),
    SetStartAtLogin(bool),
    SetStartMinimized(bool),
    SetInvertHScroll(bool),
    SetInvertVScroll(bool),
    SetHScrollThreshold(f64),
    SetLanguage(String),
    SetDebugMode(bool),
    Quit,
    ShowSettings,
    HideSettings,
}

/// Launch the settings window.  Blocks until the user closes it.
/// All GPU / OpenGL / eframe resources are freed on return.
pub fn run_settings_window(
    tx: Sender<UiMessage>,
    ui_state: Arc<Mutex<UiState>>,
    config: Arc<Mutex<Config>>,
) {
    let app = SettingsApp::new(tx, ui_state, config);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Mouser Settings")
            .with_inner_size([900.0, 620.0])
            .with_min_inner_size([780.0, 520.0]),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Mouser",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    ) {
        log::error!("Settings UI error: {e}");
    }
}
