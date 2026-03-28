#[allow(dead_code)]

mod tray;
mod settings_window;

pub use tray::TrayManager;
pub use settings_window::SettingsApp;

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
