#[allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;

use egui::{ComboBox, Slider, Ui};

use crate::actions::{ACTIONS, CATEGORY_NAVIGATION, CATEGORY_BROWSER, CATEGORY_EDITING, CATEGORY_MEDIA, CATEGORY_OTHER};
use crate::config::{Config, profile_button_names};
use crate::locale::AVAILABLE_LANGUAGES;

use super::{UiMessage, UiState};

/// The active tab in the settings window.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    MouseProfiles,
    PointerScroll,
}

/// The egui application for the settings window.
pub struct SettingsApp {
    pub tx: Sender<UiMessage>,
    pub state: Arc<Mutex<UiState>>,
    pub config: Arc<Mutex<Config>>,

    // UI state
    active_tab: Tab,

    // Mouse & Profiles tab
    selected_profile: String,
    new_profile_name: String,
    new_profile_label: String,
    show_new_profile_dialog: bool,

    // Per-button action selections (button_name -> action_id)
    button_actions: HashMap<String, String>,

    // Pointer & Scroll tab
    dpi: u32,
    smart_shift_mode: String,
    invert_hscroll: bool,
    invert_vscroll: bool,
    hscroll_threshold: f64,
    gesture_threshold: u32,
    gesture_deadzone: u32,
    gesture_timeout_ms: u32,

    // Settings
    language: String,
    start_at_login: bool,
    start_minimized: bool,
    debug_mode: bool,
}

/// Human-readable labels for each button key.
fn button_label(key: &str) -> String {
    match key {
        "middle"         => "Middle Button".to_string(),
        "gesture"        => "Gesture Button".to_string(),
        "xbutton1"       => "Back Button".to_string(),
        "xbutton2"       => "Forward Button".to_string(),
        "gesture_left"   => "Gesture Left".to_string(),
        "gesture_right"  => "Gesture Right".to_string(),
        "gesture_up"     => "Gesture Up".to_string(),
        "gesture_down"   => "Gesture Down".to_string(),
        "mode_shift"     => "Mode Shift".to_string(),
        "hscroll_left"   => "H-Scroll Left".to_string(),
        "hscroll_right"  => "H-Scroll Right".to_string(),
        _                => key.to_string(),
    }
}

/// Returns a human-readable label for an action id.
fn action_label(action_id: &str) -> String {
    if let Some(action) = ACTIONS.iter().find(|a| a.id == action_id) {
        return action.label.to_string();
    }
    if action_id.starts_with("custom:") {
        return crate::actions::custom_action_label(action_id);
    }
    action_id.to_string()
}

impl SettingsApp {
    /// Create a new SettingsApp.
    pub fn new(
        tx: Sender<UiMessage>,
        state: Arc<Mutex<UiState>>,
        config: Arc<Mutex<Config>>,
    ) -> Self {
        let (selected_profile, button_actions, dpi, smart_shift_mode,
             invert_hscroll, invert_vscroll, hscroll_threshold,
             gesture_threshold, gesture_deadzone, gesture_timeout_ms,
             language, start_at_login, start_minimized, debug_mode) = {
            let cfg = config.lock().unwrap();
            let profile = cfg.active_profile.clone();
            let actions = cfg.profiles
                .get(&profile)
                .map(|p| p.mappings.clone())
                .unwrap_or_default();
            let s = &cfg.settings;
            (
                profile,
                actions,
                s.dpi,
                s.smart_shift_mode.clone(),
                s.invert_hscroll,
                s.invert_vscroll,
                s.hscroll_threshold,
                s.gesture_threshold,
                s.gesture_deadzone,
                s.gesture_timeout_ms,
                s.language.clone(),
                s.start_at_login,
                s.start_minimized,
                s.debug_mode,
            )
        };

        Self {
            tx,
            state,
            config,
            active_tab: Tab::MouseProfiles,
            selected_profile,
            new_profile_name: String::new(),
            new_profile_label: String::new(),
            show_new_profile_dialog: false,
            button_actions,
            dpi,
            smart_shift_mode,
            invert_hscroll,
            invert_vscroll,
            hscroll_threshold,
            gesture_threshold,
            gesture_deadzone,
            gesture_timeout_ms,
            language,
            start_at_login,
            start_minimized,
            debug_mode,
        }
    }

    /// Reload button action mappings from config for the selected profile.
    fn reload_button_actions(&mut self) {
        let cfg = self.config.lock().unwrap();
        if let Some(profile) = cfg.profiles.get(&self.selected_profile) {
            self.button_actions = profile.mappings.clone();
        }
    }

    fn send(&self, msg: UiMessage) {
        let _ = self.tx.send(msg);
    }

    /// Draw the device status bar at top.
    fn draw_status_bar(&self, ui: &mut Ui) {
        let state = self.state.lock().unwrap();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(&state.device_name).strong());
            ui.separator();
            if let Some(pct) = state.battery_pct {
                ui.label(format!("Battery: {}%", pct));
                ui.separator();
            }
            ui.label(format!("DPI: {}", state.dpi));
            ui.separator();
            let status = if state.enabled { "Enabled" } else { "Disabled" };
            ui.label(status);
        });
    }

    /// Draw the Mouse & Profiles tab.
    fn draw_mouse_profiles_tab(&mut self, ui: &mut Ui) {
        // Profile selector
        ui.horizontal(|ui| {
            ui.label("Profile:");
            let profile_names: Vec<String> = {
                let cfg = self.config.lock().unwrap();
                let mut names: Vec<String> = cfg.profiles.keys().cloned().collect();
                names.sort();
                names
            };

            let current = self.selected_profile.clone();
            ComboBox::from_id_salt("profile_selector")
                .selected_text(&current)
                .show_ui(ui, |ui| {
                    for name in &profile_names {
                        let label = {
                            let cfg = self.config.lock().unwrap();
                            cfg.profiles.get(name)
                                .map(|p| if p.label.is_empty() { name.clone() } else { p.label.clone() })
                                .unwrap_or_else(|| name.clone())
                        };
                        if ui.selectable_value(&mut self.selected_profile, name.clone(), label).clicked() {
                            self.send(UiMessage::SwitchProfile(name.clone()));
                            self.reload_button_actions();
                        }
                    }
                });

            if ui.button("New Profile").clicked() {
                self.show_new_profile_dialog = true;
                self.new_profile_name = String::new();
                self.new_profile_label = String::new();
            }

            let can_delete = self.selected_profile != "default";
            if ui.add_enabled(can_delete, egui::Button::new("Delete Profile")).clicked() {
                let name = self.selected_profile.clone();
                self.send(UiMessage::DeleteProfile(name));
                self.selected_profile = "default".to_string();
                self.reload_button_actions();
            }
        });

        // New profile dialog
        if self.show_new_profile_dialog {
            egui::Window::new("New Profile")
                .collapsible(false)
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Name (id):");
                        ui.text_edit_singleline(&mut self.new_profile_name);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Label:");
                        ui.text_edit_singleline(&mut self.new_profile_label);
                    });
                    ui.horizontal(|ui| {
                        if ui.button("Create").clicked() {
                            let name = self.new_profile_name.trim().to_string();
                            let label = self.new_profile_label.trim().to_string();
                            if !name.is_empty() {
                                let copy_from = Some(self.selected_profile.clone());
                                self.send(UiMessage::CreateProfile { name: name.clone(), label, copy_from });
                                self.selected_profile = name;
                                self.reload_button_actions();
                            }
                            self.show_new_profile_dialog = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_new_profile_dialog = false;
                        }
                    });
                });
        }

        ui.separator();

        // Per-button action assignments
        ui.heading("Button Mappings");

        let all_buttons = profile_button_names();
        let categories = [
            CATEGORY_NAVIGATION,
            CATEGORY_BROWSER,
            CATEGORY_EDITING,
            CATEGORY_MEDIA,
            CATEGORY_OTHER,
        ];

        egui::Grid::new("button_grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for btn in &all_buttons {
                    let label = button_label(btn);
                    ui.label(label);

                    let current_action = self.button_actions
                        .get(*btn)
                        .cloned()
                        .unwrap_or_else(|| "none".to_string());
                    let display = action_label(&current_action);

                    let mut selected = current_action.clone();
                    ComboBox::from_id_salt(format!("action_{}", btn))
                        .selected_text(&display)
                        .show_ui(ui, |ui| {
                            for &cat in &categories {
                                ui.separator();
                                ui.label(egui::RichText::new(cat).italics().small());
                                for action in ACTIONS.iter().filter(|a| a.category == cat) {
                                    if ui.selectable_value(&mut selected, action.id.to_string(), action.label).clicked() {
                                        // value updated by selectable_value
                                    }
                                }
                            }
                        });

                    if selected != current_action {
                        self.button_actions.insert(btn.to_string(), selected.clone());
                        self.send(UiMessage::SetMapping {
                            profile: self.selected_profile.clone(),
                            button: btn.to_string(),
                            action_id: selected,
                        });
                    }

                    ui.end_row();
                }
            });
    }

    /// Draw the Pointer & Scroll tab.
    fn draw_pointer_scroll_tab(&mut self, ui: &mut Ui) {
        egui::Grid::new("scroll_grid")
            .num_columns(2)
            .striped(false)
            .show(ui, |ui| {
                // DPI
                ui.label("DPI:");
                let old_dpi = self.dpi;
                ui.add(Slider::new(&mut self.dpi, 200..=8000).step_by(50.0).text("DPI"));
                ui.end_row();

                if self.dpi != old_dpi {
                    self.send(UiMessage::SetDpi(self.dpi));
                }

                // Smart Shift
                ui.label("Scroll Wheel Mode:");
                ui.horizontal(|ui| {
                    let old_mode = self.smart_shift_mode.clone();
                    if ui.radio_value(&mut self.smart_shift_mode, "ratchet".to_string(), "Ratchet").clicked()
                        && self.smart_shift_mode != old_mode
                    {
                        self.send(UiMessage::SetSmartShift(self.smart_shift_mode.clone()));
                    }
                    if ui.radio_value(&mut self.smart_shift_mode, "freespin".to_string(), "Free Spin").clicked()
                        && self.smart_shift_mode != old_mode
                    {
                        self.send(UiMessage::SetSmartShift(self.smart_shift_mode.clone()));
                    }
                });
                ui.end_row();

                // Invert H-Scroll
                ui.label("Invert H-Scroll:");
                let old = self.invert_hscroll;
                ui.checkbox(&mut self.invert_hscroll, "");
                ui.end_row();
                if self.invert_hscroll != old {
                    self.send(UiMessage::SetInvertHScroll(self.invert_hscroll));
                }

                // Invert V-Scroll
                ui.label("Invert V-Scroll:");
                let old = self.invert_vscroll;
                ui.checkbox(&mut self.invert_vscroll, "");
                ui.end_row();
                if self.invert_vscroll != old {
                    self.send(UiMessage::SetInvertVScroll(self.invert_vscroll));
                }

                // H-Scroll threshold
                ui.label("H-Scroll Threshold:");
                let old = self.hscroll_threshold;
                ui.add(Slider::new(&mut self.hscroll_threshold, 0.1..=5.0).step_by(0.1));
                ui.end_row();
                if (self.hscroll_threshold - old).abs() > 0.001 {
                    self.send(UiMessage::SetHScrollThreshold(self.hscroll_threshold));
                }

                // Gesture threshold
                ui.label("Gesture Threshold:");
                ui.add(Slider::new(&mut self.gesture_threshold, 5..=200).text("px"));
                ui.end_row();

                // Gesture deadzone
                ui.label("Gesture Deadzone:");
                ui.add(Slider::new(&mut self.gesture_deadzone, 5..=100).text("px"));
                ui.end_row();

                // Gesture timeout
                ui.label("Gesture Timeout:");
                ui.add(Slider::new(&mut self.gesture_timeout_ms, 100..=5000).text("ms"));
                ui.end_row();
            });
    }

    /// Draw the settings section (language, startup, debug).
    fn draw_settings_section(&mut self, ui: &mut Ui) {
        ui.separator();
        ui.heading("Settings");

        egui::Grid::new("settings_grid")
            .num_columns(2)
            .striped(false)
            .show(ui, |ui| {
                // Language
                ui.label("Language:");
                let current_lang = self.language.clone();
                ComboBox::from_id_salt("lang_selector")
                    .selected_text(&current_lang)
                    .show_ui(ui, |ui| {
                        for &(code, name) in AVAILABLE_LANGUAGES {
                            if ui.selectable_value(&mut self.language, code.to_string(), name).clicked() {
                                // selected
                            }
                        }
                    });
                ui.end_row();
                if self.language != current_lang {
                    self.send(UiMessage::SetLanguage(self.language.clone()));
                }

                // Start at login
                ui.label("Start at login:");
                let old = self.start_at_login;
                ui.checkbox(&mut self.start_at_login, "");
                ui.end_row();
                if self.start_at_login != old {
                    self.send(UiMessage::SetStartAtLogin(self.start_at_login));
                }

                // Start minimized
                ui.label("Start minimized:");
                let old = self.start_minimized;
                ui.checkbox(&mut self.start_minimized, "");
                ui.end_row();
                if self.start_minimized != old {
                    self.send(UiMessage::SetStartMinimized(self.start_minimized));
                }

                // Debug mode
                ui.label("Debug mode:");
                let old = self.debug_mode;
                ui.checkbox(&mut self.debug_mode, "");
                ui.end_row();
                if self.debug_mode != old {
                    self.send(UiMessage::SetDebugMode(self.debug_mode));
                }
            });
    }
}

impl SettingsApp {
    /// Draw the full settings UI.  Called by `MainApp::update()` and by the
    /// standalone `eframe::App` impl.
    pub fn update_ui(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("status_bar").show(ctx, |ui| {
            self.draw_status_bar(ui);
        });

        egui::TopBottomPanel::top("tab_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(
                    self.active_tab == Tab::MouseProfiles,
                    "Mouse & Profiles",
                ).clicked() {
                    self.active_tab = Tab::MouseProfiles;
                }
                if ui.selectable_label(
                    self.active_tab == Tab::PointerScroll,
                    "Pointer & Scroll",
                ).clicked() {
                    self.active_tab = Tab::PointerScroll;
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                match self.active_tab {
                    Tab::MouseProfiles => {
                        self.draw_mouse_profiles_tab(ui);
                    }
                    Tab::PointerScroll => {
                        self.draw_pointer_scroll_tab(ui);
                    }
                }
                self.draw_settings_section(ui);
            });
        });
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.tx.send(UiMessage::HideSettings);
    }
}
