#[allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc::Sender};

use tray_icon::{
    TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuItem, PredefinedMenuItem, MenuEvent},
    Icon,
};

use super::{UiMessage, UiState};

pub struct TrayManager {
    #[allow(dead_code)]
    tray: TrayIcon,
    toggle_item: MenuItem,
    profile_item: MenuItem,
    settings_item: MenuItem,
    quit_item: MenuItem,
    tx: Sender<UiMessage>,
    pub show_settings_flag: Arc<AtomicBool>,
    pub quit_flag: Arc<AtomicBool>,
}

impl TrayManager {
    pub fn new(tx: Sender<UiMessage>, state: &UiState) -> anyhow::Result<Self> {
        let tray_menu = Menu::new();

        let title_item = MenuItem::new("Mouser", false, None);
        tray_menu.append(&title_item)?;
        tray_menu.append(&PredefinedMenuItem::separator())?;

        let toggle_label = if state.enabled { "Disable Remapping" } else { "Enable Remapping" };
        let toggle_item = MenuItem::new(toggle_label, true, None);
        tray_menu.append(&toggle_item)?;

        let profile_label = format!("Profile: {}", state.current_profile);
        let profile_item = MenuItem::new(&profile_label, false, None);
        tray_menu.append(&profile_item)?;

        tray_menu.append(&PredefinedMenuItem::separator())?;

        let settings_item = MenuItem::new("Settings...", true, None);
        tray_menu.append(&settings_item)?;

        tray_menu.append(&PredefinedMenuItem::separator())?;

        let quit_item = MenuItem::new("Quit", true, None);
        tray_menu.append(&quit_item)?;

        let icon = Self::make_icon();

        let builder = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Mouser — click to open settings");

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
            quit_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    fn make_icon() -> Option<Icon> {
        const SIZE: u32 = 16;
        let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
        let cx = (SIZE / 2) as i32;
        let cy = (SIZE / 2) as i32;
        let r = (SIZE / 2 - 1) as i32;
        for y in 0..SIZE as i32 {
            for x in 0..SIZE as i32 {
                let dx = x - cx;
                let dy = y - cy;
                let idx = ((y * SIZE as i32 + x) * 4) as usize;
                if dx * dx + dy * dy <= r * r {
                    rgba[idx] = 220;
                    rgba[idx + 1] = 220;
                    rgba[idx + 2] = 220;
                    rgba[idx + 3] = 255;
                }
            }
        }
        Icon::from_rgba(rgba, SIZE, SIZE).ok()
    }

    pub fn update(&self, state: &UiState) {
        let toggle_label = if state.enabled { "Disable Remapping" } else { "Enable Remapping" };
        self.toggle_item.set_text(toggle_label);
        self.profile_item.set_text(&format!("Profile: {}", state.current_profile));
        let tooltip = if let Some(batt) = state.battery_pct {
            format!("Mouser — {} — {}%", state.device_name, batt)
        } else {
            format!("Mouser — {}", state.device_name)
        };
        let _ = self.tray.set_tooltip(Some(tooltip));
    }

    pub fn poll_events(&self) {
        // Only double-click on tray icon opens settings
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                log::info!("[Tray] Double-click → open settings");
                self.show_settings_flag.store(true, Ordering::Relaxed);
            }
        }

        // Check menu events
        let settings_id = self.settings_item.id().clone();
        let quit_id = self.quit_item.id().clone();
        let toggle_id = self.toggle_item.id().clone();

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            log::info!("[Tray] Menu event: {:?}", event.id());
            let id = event.id().clone();
            if id == settings_id || id == toggle_id {
                self.show_settings_flag.store(true, Ordering::Relaxed);
            } else if id == quit_id {
                self.quit_flag.store(true, Ordering::Relaxed);
                let _ = self.tx.send(UiMessage::Quit);
            }
        }
    }
}
