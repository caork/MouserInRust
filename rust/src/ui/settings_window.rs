#[allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;

use egui::{ComboBox, Slider, Ui, Color32, RichText, Rounding, Stroke, Vec2, Frame as EFrame, Margin};

use crate::actions::{ACTIONS, CATEGORY_NAVIGATION, CATEGORY_BROWSER, CATEGORY_EDITING, CATEGORY_MEDIA, CATEGORY_OTHER};
use crate::config::{Config, profile_button_names};
use crate::locale::AVAILABLE_LANGUAGES;

use super::{UiMessage, UiState};

// ---------------------------------------------------------------------------
// Theme colours — matches the Python QML dark palette
// ---------------------------------------------------------------------------
mod theme {
    use egui::Color32;

    pub const BG: Color32            = Color32::from_rgb(0x11, 0x18, 0x27);
    pub const BG_CARD: Color32       = Color32::from_rgb(0x16, 0x21, 0x3e);
    pub const BG_SIDEBAR: Color32    = Color32::from_rgb(0x0b, 0x12, 0x20);
    pub const BG_SUBTLE: Color32     = Color32::from_rgb(0x0f, 0x15, 0x25);
    pub const BG_INPUT: Color32      = Color32::from_rgb(0x11, 0x18, 0x27);

    pub const ACCENT: Color32        = Color32::from_rgb(0x00, 0xd4, 0xaa);
    pub const ACCENT_HOVER: Color32  = Color32::from_rgb(0x00, 0xff, 0xc8);
    pub const ACCENT_DIM: Color32    = Color32::from_rgb(0x0d, 0x2e, 0x26);

    pub const TEXT_PRIMARY: Color32  = Color32::from_rgb(0xed, 0xf2, 0xf7);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0x9a, 0xa4, 0xb6);
    pub const TEXT_DIM: Color32      = Color32::from_rgb(0x6f, 0x7b, 0x90);

    pub const BORDER: Color32        = Color32::from_rgb(0x26, 0x32, 0x46);

    pub const DANGER: Color32        = Color32::from_rgb(0xff, 0x6b, 0x6b);

    pub const CARD_ROUNDING: f32     = 12.0;
    pub const BTN_ROUNDING: f32      = 10.0;
}

// ---------------------------------------------------------------------------
// Tab enum
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    MouseProfiles,
    PointerScroll,
}

// ---------------------------------------------------------------------------
// Main struct
// ---------------------------------------------------------------------------
pub struct SettingsApp {
    pub tx: Sender<UiMessage>,
    pub state: Arc<Mutex<UiState>>,
    pub config: Arc<Mutex<Config>>,

    active_tab: Tab,

    // Mouse & Profiles tab
    selected_profile: String,
    new_profile_name: String,
    new_profile_label: String,
    show_new_profile_dialog: bool,
    selected_button: Option<String>,

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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn button_label(key: &str) -> &str {
    match key {
        "middle"         => "Middle Button",
        "gesture"        => "Gesture Button",
        "xbutton1"       => "Back Button",
        "xbutton2"       => "Forward Button",
        "gesture_left"   => "Gesture Left",
        "gesture_right"  => "Gesture Right",
        "gesture_up"     => "Gesture Up",
        "gesture_down"   => "Gesture Down",
        "mode_shift"     => "Mode Shift",
        "hscroll_left"   => "H-Scroll Left",
        "hscroll_right"  => "H-Scroll Right",
        _                => key,
    }
}

fn action_label(action_id: &str) -> String {
    if action_id == "none" {
        return "None".to_string();
    }
    if let Some(action) = ACTIONS.iter().find(|a| a.id == action_id) {
        return action.label.to_string();
    }
    if action_id.starts_with("custom:") {
        return crate::actions::custom_action_label(action_id);
    }
    action_id.to_string()
}

/// Apply the dark theme to egui visuals.
pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();

    visuals.panel_fill = theme::BG;
    visuals.window_fill = theme::BG_CARD;
    visuals.faint_bg_color = theme::BG_SUBTLE;
    visuals.extreme_bg_color = theme::BG_INPUT;

    visuals.override_text_color = Some(theme::TEXT_PRIMARY);

    visuals.widgets.noninteractive.bg_fill = theme::BG_CARD;
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, theme::TEXT_SECONDARY);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, theme::BORDER);
    visuals.widgets.noninteractive.rounding = Rounding::same(theme::BTN_ROUNDING);

    visuals.widgets.inactive.bg_fill = theme::BG_SUBTLE;
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, theme::TEXT_PRIMARY);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, theme::BORDER);
    visuals.widgets.inactive.rounding = Rounding::same(theme::BTN_ROUNDING);

    visuals.widgets.hovered.bg_fill = Color32::from_rgba_premultiplied(0x1f, 0x34, 0x60, 255);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, theme::TEXT_PRIMARY);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, theme::ACCENT);
    visuals.widgets.hovered.rounding = Rounding::same(theme::BTN_ROUNDING);

    visuals.widgets.active.bg_fill = theme::ACCENT_DIM;
    visuals.widgets.active.fg_stroke = Stroke::new(2.0, theme::ACCENT);
    visuals.widgets.active.bg_stroke = Stroke::new(2.0, theme::ACCENT);
    visuals.widgets.active.rounding = Rounding::same(theme::BTN_ROUNDING);

    visuals.widgets.open.bg_fill = theme::BG_CARD;
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, theme::TEXT_PRIMARY);
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, theme::ACCENT);

    visuals.selection.bg_fill = theme::ACCENT_DIM;
    visuals.selection.stroke = Stroke::new(1.0, theme::ACCENT);

    visuals.window_rounding = Rounding::same(theme::CARD_ROUNDING);
    visuals.window_stroke = Stroke::new(1.0, theme::BORDER);

    visuals.striped = true;

    ctx.set_visuals(visuals);
}

// ---------------------------------------------------------------------------
// Card frame helper
// ---------------------------------------------------------------------------

fn card_frame() -> EFrame {
    EFrame::none()
        .fill(theme::BG_CARD)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .rounding(Rounding::same(theme::CARD_ROUNDING))
        .inner_margin(Margin::same(20.0))
}

fn card_title(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).size(16.0).strong().color(theme::TEXT_PRIMARY));
    ui.add_space(4.0);
}

fn card_desc(ui: &mut Ui, desc: &str) {
    ui.label(RichText::new(desc).size(12.0).color(theme::TEXT_SECONDARY));
    ui.add_space(10.0);
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl SettingsApp {
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
                profile, actions,
                s.dpi, s.smart_shift_mode.clone(),
                s.invert_hscroll, s.invert_vscroll, s.hscroll_threshold,
                s.gesture_threshold, s.gesture_deadzone, s.gesture_timeout_ms,
                s.language.clone(), s.start_at_login, s.start_minimized, s.debug_mode,
            )
        };

        Self {
            tx, state, config,
            active_tab: Tab::MouseProfiles,
            selected_profile,
            new_profile_name: String::new(),
            new_profile_label: String::new(),
            show_new_profile_dialog: false,
            selected_button: None,
            button_actions,
            dpi, smart_shift_mode,
            invert_hscroll, invert_vscroll, hscroll_threshold,
            gesture_threshold, gesture_deadzone, gesture_timeout_ms,
            language, start_at_login, start_minimized, debug_mode,
        }
    }

    fn reload_button_actions(&mut self) {
        let cfg = self.config.lock().unwrap();
        if let Some(profile) = cfg.profiles.get(&self.selected_profile) {
            self.button_actions = profile.mappings.clone();
        }
    }

    fn send(&self, msg: UiMessage) {
        let _ = self.tx.send(msg);
    }
}

// ---------------------------------------------------------------------------
// Sidebar
// ---------------------------------------------------------------------------

impl SettingsApp {
    fn draw_sidebar(&mut self, ui: &mut Ui) {
        ui.set_min_width(72.0);
        ui.set_max_width(72.0);

        ui.vertical_centered(|ui| {
            ui.add_space(16.0);

            // Logo "M" button
            let (rect, _) = ui.allocate_exact_size(Vec2::new(44.0, 44.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, Rounding::same(14.0), theme::ACCENT);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "M",
                egui::FontId::proportional(20.0),
                theme::BG_SIDEBAR,
            );

            ui.add_space(24.0);

            // Navigation items
            let nav_items = [
                (Tab::MouseProfiles, "M", "Mouse"),
                (Tab::PointerScroll, "S", "Settings"),
            ];

            for (tab, icon, label) in nav_items {
                let is_active = self.active_tab == tab;
                let btn_size = Vec2::new(56.0, 48.0);
                let (rect, response) = ui.allocate_exact_size(btn_size, egui::Sense::click());

                // Active indicator bar
                if is_active {
                    let bar = egui::Rect::from_min_size(
                        rect.left_top() + Vec2::new(0.0, 12.0),
                        Vec2::new(3.0, 24.0),
                    );
                    ui.painter().rect_filled(bar, Rounding::same(2.0), theme::ACCENT);
                }

                // Background
                let bg = if is_active {
                    Color32::from_rgba_premultiplied(0x00, 0x60, 0x50, 40)
                } else if response.hovered() {
                    Color32::from_rgba_premultiplied(0xff, 0xff, 0xff, 15)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, Rounding::same(10.0), bg);

                // Icon
                let icon_color = if is_active { theme::ACCENT } else { theme::TEXT_DIM };
                ui.painter().text(
                    rect.center() - Vec2::new(0.0, 4.0),
                    egui::Align2::CENTER_CENTER,
                    icon,
                    egui::FontId::proportional(20.0),
                    icon_color,
                );

                // Label below icon
                ui.painter().text(
                    rect.center() + Vec2::new(0.0, 14.0),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(9.0),
                    if is_active { theme::ACCENT } else { theme::TEXT_DIM },
                );

                if response.clicked() {
                    self.active_tab = tab;
                }

                ui.add_space(4.0);
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

impl SettingsApp {
    fn draw_status_bar(&self, ui: &mut Ui) {
        let state = self.state.lock().unwrap();
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(RichText::new(&state.device_name).size(14.0).strong().color(theme::TEXT_PRIMARY));
            ui.add_space(12.0);

            // Connection badge
            let (dot_color, status_text) = if state.device_name == "No device" {
                (theme::DANGER, "Not Connected")
            } else {
                (theme::ACCENT, "Connected")
            };
            let badge_rect = ui.available_rect_before_wrap();
            let badge_pos = badge_rect.left_top() + Vec2::new(0.0, 4.0);
            ui.painter().circle_filled(badge_pos + Vec2::new(4.0, 6.0), 3.0, dot_color);
            ui.add_space(12.0);
            ui.label(RichText::new(status_text).size(11.0).color(dot_color));

            ui.add_space(16.0);

            // Battery badge
            if let Some(pct) = state.battery_pct {
                let batt_color = if pct <= 20 { theme::DANGER }
                    else if pct <= 40 { Color32::from_rgb(0xff, 0xb3, 0x47) }
                    else { theme::ACCENT };
                let badge_text = format!("{}%", pct);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(42.0, 22.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, Rounding::same(11.0), theme::ACCENT_DIM);
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, badge_text,
                    egui::FontId::proportional(11.0), batt_color);
            }

            // DPI badge
            let dpi_text = format!("DPI: {}", state.dpi);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(80.0, 22.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, Rounding::same(11.0), theme::ACCENT_DIM);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, dpi_text,
                egui::FontId::proportional(11.0), theme::ACCENT);
        });
    }
}

// ---------------------------------------------------------------------------
// Mouse & Profiles page
// ---------------------------------------------------------------------------

impl SettingsApp {
    fn draw_mouse_profiles_tab(&mut self, ui: &mut Ui) {
        ui.columns(2, |cols| {
            // Left: profile list
            self.draw_profile_list(&mut cols[0]);
            // Right: button mappings
            self.draw_button_mappings(&mut cols[1]);
        });

        // New profile dialog
        if self.show_new_profile_dialog {
            egui::Window::new("New Profile")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    ui.set_min_width(320.0);
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Name:").color(theme::TEXT_SECONDARY));
                        ui.text_edit_singleline(&mut self.new_profile_name);
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Label:").color(theme::TEXT_SECONDARY));
                        ui.text_edit_singleline(&mut self.new_profile_label);
                    });
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        let create_btn = ui.add(egui::Button::new(
                            RichText::new("Create").color(theme::BG_SIDEBAR)
                        ).fill(theme::ACCENT).rounding(Rounding::same(8.0)));
                        if create_btn.clicked() {
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
    }

    fn draw_profile_list(&mut self, ui: &mut Ui) {
        card_frame().show(ui, |ui| {
            ui.set_min_height(400.0);

            // Header
            ui.horizontal(|ui| {
                ui.label(RichText::new("Profiles").size(14.0).strong().color(theme::TEXT_PRIMARY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let add_btn = ui.add(egui::Button::new(
                        RichText::new("+").size(16.0).color(theme::ACCENT)
                    ).fill(theme::ACCENT_DIM).rounding(Rounding::same(8.0)).min_size(Vec2::new(30.0, 30.0)));
                    if add_btn.clicked() {
                        self.show_new_profile_dialog = true;
                        self.new_profile_name.clear();
                        self.new_profile_label.clear();
                    }
                });
            });

            ui.add_space(8.0);

            // Profile list
            let profile_data: Vec<(String, String, Vec<String>)> = {
                let cfg = self.config.lock().unwrap();
                let mut data: Vec<_> = cfg.profiles.iter().map(|(key, p)| {
                    let label = if p.label.is_empty() { key.clone() } else { p.label.clone() };
                    (key.clone(), label, p.apps.clone())
                }).collect();
                data.sort_by(|a, b| {
                    if a.0 == "default" { std::cmp::Ordering::Less }
                    else if b.0 == "default" { std::cmp::Ordering::Greater }
                    else { a.1.cmp(&b.1) }
                });
                data
            };

            for (key, label, apps) in &profile_data {
                let is_selected = *key == self.selected_profile;
                let item_height = 52.0;
                let (rect, response) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), item_height),
                    egui::Sense::click(),
                );

                // Background
                let bg = if is_selected {
                    Color32::from_rgba_premultiplied(0x00, 0x60, 0x50, 20)
                } else if response.hovered() {
                    Color32::from_rgba_premultiplied(0xff, 0xff, 0xff, 8)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(rect, Rounding::same(8.0), bg);

                // Active bar on left
                if is_selected {
                    let bar = egui::Rect::from_min_size(
                        rect.left_top() + Vec2::new(0.0, 12.0),
                        Vec2::new(3.0, 28.0),
                    );
                    ui.painter().rect_filled(bar, Rounding::same(2.0), theme::ACCENT);
                }

                // Profile name
                let name_color = if is_selected { theme::ACCENT } else { theme::TEXT_PRIMARY };
                ui.painter().text(
                    rect.left_top() + Vec2::new(16.0, 16.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::proportional(13.0),
                    name_color,
                );

                // Apps subtitle
                let apps_text = if apps.is_empty() {
                    "All applications"
                } else {
                    // Show first app name
                    apps.first().map(|s| s.as_str()).unwrap_or("")
                };
                ui.painter().text(
                    rect.left_top() + Vec2::new(16.0, 34.0),
                    egui::Align2::LEFT_TOP,
                    apps_text,
                    egui::FontId::proportional(10.0),
                    theme::TEXT_DIM,
                );

                if response.clicked() {
                    self.selected_profile = key.clone();
                    self.send(UiMessage::SwitchProfile(key.clone()));
                    self.reload_button_actions();
                }
            }

            // Delete button at bottom (for non-default)
            if self.selected_profile != "default" {
                ui.add_space(12.0);
                let del_btn = ui.add(egui::Button::new(
                    RichText::new("Delete Profile").size(11.0).color(theme::DANGER)
                ).fill(Color32::from_rgb(0x5b, 0x1f, 0x26)).rounding(Rounding::same(8.0)));
                if del_btn.clicked() {
                    let name = self.selected_profile.clone();
                    self.send(UiMessage::DeleteProfile(name));
                    self.selected_profile = "default".to_string();
                    self.reload_button_actions();
                }
            }
        });
    }

    fn draw_button_mappings(&mut self, ui: &mut Ui) {
        // Header
        ui.horizontal(|ui| {
            let profile_label = {
                let cfg = self.config.lock().unwrap();
                cfg.profiles.get(&self.selected_profile)
                    .map(|p| if p.label.is_empty() { self.selected_profile.clone() } else { p.label.clone() })
                    .unwrap_or_else(|| self.selected_profile.clone())
            };
            ui.label(RichText::new(&profile_label).size(20.0).strong().color(theme::TEXT_PRIMARY));
        });

        ui.add_space(16.0);

        // Button rows
        let all_buttons = profile_button_names();
        let categories = [
            CATEGORY_NAVIGATION,
            CATEGORY_BROWSER,
            CATEGORY_EDITING,
            CATEGORY_MEDIA,
            CATEGORY_OTHER,
        ];

        for btn in &all_buttons {
            let label = button_label(btn);
            let current_action = self.button_actions
                .get(*btn)
                .cloned()
                .unwrap_or_else(|| "none".to_string());
            let display = action_label(&current_action);

            // Button row card
            let is_selected = self.selected_button.as_deref() == Some(*btn);
            let row_bg = if is_selected { theme::ACCENT_DIM } else { theme::BG_CARD };
            let row_border = if is_selected { theme::ACCENT } else { theme::BORDER };

            EFrame::none()
                .fill(row_bg)
                .stroke(Stroke::new(1.0, row_border))
                .rounding(Rounding::same(10.0))
                .inner_margin(Margin::symmetric(16.0, 10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Button name
                        ui.label(RichText::new(label).size(13.0).strong().color(theme::TEXT_PRIMARY));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // Action combo box
                            let mut selected = current_action.clone();
                            ComboBox::from_id_salt(format!("action_{}", btn))
                                .selected_text(RichText::new(&display).size(12.0).color(theme::ACCENT))
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    // "None" option
                                    ui.selectable_value(&mut selected, "none".to_string(),
                                        RichText::new("None").color(theme::TEXT_DIM));
                                    for &cat in &categories {
                                        ui.separator();
                                        ui.label(RichText::new(cat.to_uppercase()).size(10.0)
                                            .color(theme::TEXT_DIM));
                                        for action in ACTIONS.iter().filter(|a| a.category == cat) {
                                            ui.selectable_value(&mut selected, action.id.to_string(),
                                                action.label);
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
                        });
                    });
                });

            ui.add_space(4.0);
        }
    }
}

// ---------------------------------------------------------------------------
// Pointer & Scroll page
// ---------------------------------------------------------------------------

impl SettingsApp {
    fn draw_pointer_scroll_tab(&mut self, ui: &mut Ui) {
        // Page header
        ui.add_space(8.0);
        ui.label(RichText::new("Pointer & Scroll").size(24.0).strong().color(theme::TEXT_PRIMARY));
        ui.label(RichText::new("Adjust pointer speed, scroll wheel, and gesture settings")
            .size(13.0).color(theme::TEXT_SECONDARY));
        ui.add_space(16.0);

        // ── Pointer Speed (DPI) ──
        card_frame().show(ui, |ui| {
            card_title(ui, "Pointer Speed");
            card_desc(ui, "Adjust the DPI (dots per inch) for pointer sensitivity.");

            ui.horizontal(|ui| {
                ui.label(RichText::new("200").size(11.0).color(theme::TEXT_DIM));
                let old_dpi = self.dpi;
                let mut dpi_f = self.dpi as f64;
                ui.add(Slider::new(&mut dpi_f, 200.0..=8000.0).step_by(50.0).show_value(false));
                self.dpi = dpi_f as u32;
                ui.label(RichText::new("8000").size(11.0).color(theme::TEXT_DIM));

                // DPI value badge
                let badge_text = format!("{}", self.dpi);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(80.0, 32.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, Rounding::same(10.0), theme::ACCENT_DIM);
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, &badge_text,
                    egui::FontId::proportional(14.0), theme::ACCENT);

                if self.dpi != old_dpi {
                    self.send(UiMessage::SetDpi(self.dpi));
                }
            });

            // DPI presets
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for preset in [400, 800, 1000, 1200, 1600, 2400, 3200, 4000] {
                    let is_current = self.dpi == preset;
                    let bg = if is_current { theme::ACCENT } else { theme::BG_SUBTLE };
                    let fg = if is_current { theme::BG_SIDEBAR } else { theme::TEXT_PRIMARY };
                    let btn = ui.add(egui::Button::new(
                        RichText::new(format!("{}", preset)).size(12.0).color(fg)
                    ).fill(bg).rounding(Rounding::same(8.0)).min_size(Vec2::new(48.0, 28.0))
                    .stroke(Stroke::new(1.0, theme::BORDER)));
                    if btn.clicked() {
                        self.dpi = preset;
                        self.send(UiMessage::SetDpi(self.dpi));
                    }
                }
            });
        });

        ui.add_space(16.0);

        // ── Scroll Wheel Mode ──
        card_frame().show(ui, |ui| {
            card_title(ui, "Scroll Wheel Mode");
            card_desc(ui, "Choose between ratcheted (click-by-click) or free-spinning scroll.");

            ui.horizontal(|ui| {
                let modes = [("ratchet", "Ratchet"), ("freespin", "Free Spin")];
                for (value, label) in modes {
                    let is_active = self.smart_shift_mode == value;
                    let bg = if is_active { theme::ACCENT_DIM } else { theme::BG_SUBTLE };
                    let border = if is_active { theme::ACCENT } else { theme::BORDER };
                    let fg = if is_active { theme::ACCENT } else { theme::TEXT_PRIMARY };
                    let btn = ui.add(egui::Button::new(
                        RichText::new(label).size(13.0).color(fg).strong()
                    ).fill(bg).rounding(Rounding::same(10.0)).min_size(Vec2::new(96.0, 36.0))
                    .stroke(Stroke::new(if is_active { 2.0 } else { 1.0 }, border)));
                    if btn.clicked() && !is_active {
                        self.smart_shift_mode = value.to_string();
                        self.send(UiMessage::SetSmartShift(self.smart_shift_mode.clone()));
                    }
                }
            });
        });

        ui.add_space(16.0);

        // ── Scroll Direction ──
        card_frame().show(ui, |ui| {
            card_title(ui, "Scroll Direction");
            card_desc(ui, "Invert the scroll direction for vertical or horizontal scrolling.");

            if Self::draw_toggle_row(ui, "Invert Vertical Scroll", &mut self.invert_vscroll) {
                self.send(UiMessage::SetInvertVScroll(self.invert_vscroll));
            }
            ui.add_space(6.0);
            if Self::draw_toggle_row(ui, "Invert Horizontal Scroll", &mut self.invert_hscroll) {
                self.send(UiMessage::SetInvertHScroll(self.invert_hscroll));
            }
        });

        ui.add_space(16.0);

        // ── Gesture Settings ──
        card_frame().show(ui, |ui| {
            card_title(ui, "Gesture Settings");
            card_desc(ui, "Fine-tune gesture detection sensitivity and timing.");

            ui.horizontal(|ui| {
                ui.label(RichText::new("H-Scroll Threshold").size(12.0).color(theme::TEXT_SECONDARY));
                let old = self.hscroll_threshold;
                ui.add(Slider::new(&mut self.hscroll_threshold, 0.1..=5.0).step_by(0.1));
                if (self.hscroll_threshold - old).abs() > 0.001 {
                    self.send(UiMessage::SetHScrollThreshold(self.hscroll_threshold));
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Gesture Threshold").size(12.0).color(theme::TEXT_SECONDARY));
                ui.add(Slider::new(&mut self.gesture_threshold, 5..=200).suffix(" px"));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Gesture Deadzone").size(12.0).color(theme::TEXT_SECONDARY));
                ui.add(Slider::new(&mut self.gesture_deadzone, 5..=100).suffix(" px"));
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Gesture Timeout").size(12.0).color(theme::TEXT_SECONDARY));
                ui.add(Slider::new(&mut self.gesture_timeout_ms, 100..=5000).suffix(" ms"));
            });
        });

        ui.add_space(16.0);

        // ── Settings ──
        self.draw_settings_card(ui);
    }

    fn draw_toggle_row(ui: &mut Ui, label: &str, value: &mut bool) -> bool {
        let old = *value;
        EFrame::none()
            .fill(theme::BG_SUBTLE)
            .rounding(Rounding::same(10.0))
            .inner_margin(Margin::symmetric(16.0, 12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(13.0).color(theme::TEXT_PRIMARY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(value, "");
                    });
                });
            });
        *value != old
    }
}

// ---------------------------------------------------------------------------
// Settings card (language, startup, debug)
// ---------------------------------------------------------------------------

impl SettingsApp {
    fn draw_settings_card(&mut self, ui: &mut Ui) {
        card_frame().show(ui, |ui| {
            card_title(ui, "Application Settings");
            card_desc(ui, "Language, startup behaviour, and debug options.");

            // Language buttons
            ui.label(RichText::new("Language").size(12.0).color(theme::TEXT_SECONDARY));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for &(code, name) in AVAILABLE_LANGUAGES {
                    let is_active = self.language == code;
                    let bg = if is_active { theme::ACCENT_DIM } else { theme::BG_SUBTLE };
                    let border = if is_active { theme::ACCENT } else { theme::BORDER };
                    let fg = if is_active { theme::ACCENT } else { theme::TEXT_PRIMARY };
                    let btn = ui.add(egui::Button::new(
                        RichText::new(name).size(12.0).color(fg)
                    ).fill(bg).rounding(Rounding::same(10.0)).min_size(Vec2::new(108.0, 36.0))
                    .stroke(Stroke::new(if is_active { 2.0 } else { 1.0 }, border)));
                    if btn.clicked() && !is_active {
                        self.language = code.to_string();
                        self.send(UiMessage::SetLanguage(self.language.clone()));
                    }
                }
            });

            ui.add_space(12.0);

            // Toggle rows
            {
                let old = self.start_at_login;
                let mut val = self.start_at_login;
                self.draw_setting_toggle(ui, "Start at Login", &mut val);
                if val != old { self.start_at_login = val; self.send(UiMessage::SetStartAtLogin(val)); }
            }
            ui.add_space(6.0);
            {
                let old = self.start_minimized;
                let mut val = self.start_minimized;
                self.draw_setting_toggle(ui, "Start Minimized", &mut val);
                if val != old { self.start_minimized = val; self.send(UiMessage::SetStartMinimized(val)); }
            }
            ui.add_space(6.0);
            {
                let old = self.debug_mode;
                let mut val = self.debug_mode;
                self.draw_setting_toggle(ui, "Debug Mode", &mut val);
                if val != old { self.debug_mode = val; self.send(UiMessage::SetDebugMode(val)); }
            }
        });
    }

    fn draw_setting_toggle(&self, ui: &mut Ui, label: &str, value: &mut bool) {
        EFrame::none()
            .fill(theme::BG_SUBTLE)
            .rounding(Rounding::same(10.0))
            .inner_margin(Margin::symmetric(16.0, 12.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).size(13.0).color(theme::TEXT_PRIMARY));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(value, "");
                    });
                });
            });
    }
}

// ---------------------------------------------------------------------------
// update_ui — called by MainApp
// ---------------------------------------------------------------------------

impl SettingsApp {
    pub fn update_ui(&mut self, ctx: &egui::Context) {
        // Apply theme on every frame (cheap, ensures consistency)
        apply_theme(ctx);

        // Sidebar panel
        egui::SidePanel::left("sidebar")
            .exact_width(72.0)
            .frame(EFrame::none().fill(theme::BG_SIDEBAR))
            .show(ctx, |ui| {
                self.draw_sidebar(ui);
            });

        // Status bar
        egui::TopBottomPanel::top("status_bar")
            .frame(EFrame::none().fill(theme::BG).inner_margin(Margin::symmetric(12.0, 8.0)))
            .show(ctx, |ui| {
                self.draw_status_bar(ui);
            });

        // Main content
        egui::CentralPanel::default()
            .frame(EFrame::none().fill(theme::BG).inner_margin(Margin::same(24.0)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    match self.active_tab {
                        Tab::MouseProfiles => self.draw_mouse_profiles_tab(ui),
                        Tab::PointerScroll => self.draw_pointer_scroll_tab(ui),
                    }
                });
            });
    }
}

// ---------------------------------------------------------------------------
// eframe::App impl (standalone mode — used if SettingsApp runs without MainApp)
// ---------------------------------------------------------------------------

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = self.tx.send(UiMessage::HideSettings);
    }
}
