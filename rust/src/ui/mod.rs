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
// eframe drives the native event loop (required on macOS for tray-icon).
// The window hides on close instead of exiting; clicking "Settings" in
// the tray re-shows it.  When hidden, rendering is skipped entirely to
// minimise CPU and GPU usage.
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
            // Slow repaint when hidden — just enough to poll tray events.
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
            return;
        }

        // Intercept the close button: hide instead of quit.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.window_visible = false;
            return;
        }

        // Draw the settings UI only when the window is visible.
        self.settings.update_ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.settings.tx.send(UiMessage::Quit);
    }
}
