#[allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc::Sender};

use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem, MenuEvent},
    Icon,
};

use super::{UiMessage, UiState};

/// Manages the system tray icon and its context menu.
pub struct TrayManager {
    #[allow(dead_code)]
    tray: TrayIcon,
    toggle_item: MenuItem,
    profile_item: MenuItem,
    settings_item: MenuItem,
    quit_item: MenuItem,
    tx: Sender<UiMessage>,
    /// Set to `true` when the user clicks "Settings" in the tray.
    /// `MainApp` reads and clears this flag every frame.
    pub show_settings_flag: Arc<AtomicBool>,
}

impl TrayManager {
    /// Create the tray icon and menu. Must be called on the main thread.
    pub fn new(tx: Sender<UiMessage>, state: &UiState) -> anyhow::Result<Self> {
        let tray_menu = Menu::new();

        // Title item (disabled)
        let title_item = MenuItem::new("Mouser", false, None);
        tray_menu.append(&title_item)?;
        tray_menu.append(&PredefinedMenuItem::separator())?;

        // Enable/Disable toggle
        let toggle_label = if state.enabled {
            "Disable Remapping"
        } else {
            "Enable Remapping"
        };
        let toggle_item = MenuItem::new(toggle_label, true, None);
        tray_menu.append(&toggle_item)?;

        // Current profile (display only, disabled)
        let profile_label = format!("Profile: {}", state.current_profile);
        let profile_item = MenuItem::new(&profile_label, false, None);
        tray_menu.append(&profile_item)?;

        tray_menu.append(&PredefinedMenuItem::separator())?;

        // Settings
        let settings_item = MenuItem::new("Settings...", true, None);
        tray_menu.append(&settings_item)?;

        tray_menu.append(&PredefinedMenuItem::separator())?;

        // Quit
        let quit_item = MenuItem::new("Quit", true, None);
        tray_menu.append(&quit_item)?;

        // Build a small programmatic 16x16 icon (gray square with a dot)
        let icon = Self::make_icon();

        let builder = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Mouser");

        let tray = if let Some(icon) = icon {
            builder.with_icon(icon).build()?
        } else {
            builder.build()?
        };

        Ok(Self {
            tray,
            toggle_item,
            profile_item,
            settings_item,
            quit_item,
            tx,
            show_settings_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Generate a minimal 16x16 RGBA icon programmatically.
    fn make_icon() -> Option<Icon> {
        const SIZE: u32 = 16;
        let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
        // Draw a simple filled circle in white
        let cx = (SIZE / 2) as i32;
        let cy = (SIZE / 2) as i32;
        let r = (SIZE / 2 - 1) as i32;
        for y in 0..SIZE as i32 {
            for x in 0..SIZE as i32 {
                let dx = x - cx;
                let dy = y - cy;
                let idx = ((y * SIZE as i32 + x) * 4) as usize;
                if dx * dx + dy * dy <= r * r {
                    rgba[idx] = 220;     // R
                    rgba[idx + 1] = 220; // G
                    rgba[idx + 2] = 220; // B
                    rgba[idx + 3] = 255; // A
                }
            }
        }
        Icon::from_rgba(rgba, SIZE, SIZE).ok()
    }

    /// Update menu text to reflect current state. Call this after engine state changes.
    pub fn update(&self, state: &UiState) {
        let toggle_label = if state.enabled {
            "Disable Remapping"
        } else {
            "Enable Remapping"
        };
        self.toggle_item.set_text(toggle_label);

        let profile_label = format!("Profile: {}", state.current_profile);
        self.profile_item.set_text(&profile_label);

        let tooltip = if let Some(batt) = state.battery_pct {
            format!("Mouser — {} — {}%", state.device_name, batt)
        } else {
            format!("Mouser — {}", state.device_name)
        };
        let _ = self.tray.set_tooltip(Some(tooltip));
    }

    /// Poll for menu events and dispatch UiMessages. Call this in your event loop.
    pub fn poll_events(&self) {
        let toggle_id = self.toggle_item.id().clone();
        let settings_id = self.settings_item.id().clone();
        let quit_id = self.quit_item.id().clone();

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id().clone();
            if id == toggle_id {
                // Open the settings window
                self.show_settings_flag.store(true, Ordering::Relaxed);
            } else if id == settings_id {
                self.show_settings_flag.store(true, Ordering::Relaxed);
            } else if id == quit_id {
                let _ = self.tx.send(UiMessage::Quit);
            }
        }
    }

    /// Send a ShowSettings message (e.g. when tray icon is left-clicked).
    pub fn send_show_settings(&self) {
        let _ = self.tx.send(UiMessage::ShowSettings);
    }
}
