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

// ---------------------------------------------------------------------------
// MainApp — wraps tray + settings into one eframe application.
//
// On macOS (and Windows), eframe drives the native event loop which is
// required for tray-icon to deliver menu events.  The window starts hidden
// when `start_minimized` is set; clicking "Settings" in the tray shows it.
// ---------------------------------------------------------------------------

pub struct MainApp {
    tray: Option<TrayManager>,
    settings: SettingsApp,
    window_visible: bool,
}

impl MainApp {
    pub fn new(
        tx: Sender<UiMessage>,
        ui_state: Arc<Mutex<UiState>>,
        config: Arc<Mutex<Config>>,
        tray: Option<TrayManager>,
        start_visible: bool,
    ) -> Self {
        let settings = SettingsApp::new(tx, ui_state, config);
        Self {
            tray,
            settings,
            window_visible: start_visible,
        }
    }
}

impl eframe::App for MainApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll tray menu events every frame
        if let Some(tray) = &self.tray {
            tray.poll_events();

            // Update tray text from current state
            if let Ok(state) = self.settings.state.lock() {
                tray.update(&state);
            }

            // Check if the tray's "Settings" button was clicked
            if tray.show_settings_flag.swap(false, std::sync::atomic::Ordering::Relaxed) {
                self.window_visible = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }

        // On the very first frame, hide the window if we started minimised.
        // (with_visible(false) on macOS is unreliable.)
        if !self.window_visible {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        // Always draw the settings UI — if the window happens to be
        // on-screen we must render content, otherwise egui shows a
        // black rectangle.
        self.settings.update_ui(ctx);

        // Low-power repaint when hidden so tray events keep getting polled.
        if !self.window_visible {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.settings.tx.send(UiMessage::Quit);
    }
}
