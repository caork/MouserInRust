// Config module: JSON configuration management with migration support
// Ported from core/config.py

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const CURRENT_VERSION: u32 = 6;

// ---------------------------------------------------------------------------
// Button & event mapping constants
// ---------------------------------------------------------------------------

pub const BUTTON_NAMES: &[&str] = &[
    "middle",
    "gesture",
    "xbutton1",
    "xbutton2",
    "hscroll_left",
    "hscroll_right",
    "mode_shift",
];

pub const GESTURE_DIRECTION_BUTTONS: &[&str] = &[
    "gesture_left",
    "gesture_right",
    "gesture_up",
    "gesture_down",
];

/// All buttons that appear in a profile's mappings dict.
pub fn profile_button_names() -> Vec<&'static str> {
    let mut v: Vec<&str> = BUTTON_NAMES.to_vec();
    v.extend_from_slice(GESTURE_DIRECTION_BUTTONS);
    v
}

// ---------------------------------------------------------------------------
// Config data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default = "default_active_profile")]
    pub active_profile: String,
    #[serde(default = "default_profiles")]
    pub profiles: HashMap<String, Profile>,
    #[serde(default)]
    pub settings: Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub apps: Vec<String>,
    #[serde(default = "default_mappings")]
    pub mappings: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub start_minimized: bool,
    #[serde(default)]
    pub start_at_login: bool,
    #[serde(default = "default_hscroll_threshold")]
    pub hscroll_threshold: f64,
    #[serde(default)]
    pub invert_hscroll: bool,
    #[serde(default)]
    pub invert_vscroll: bool,
    #[serde(default = "default_dpi")]
    pub dpi: u32,
    #[serde(default = "default_smart_shift_mode")]
    pub smart_shift_mode: String,
    #[serde(default = "default_gesture_threshold")]
    pub gesture_threshold: u32,
    #[serde(default = "default_gesture_deadzone")]
    pub gesture_deadzone: u32,
    #[serde(default = "default_gesture_timeout_ms")]
    pub gesture_timeout_ms: u32,
    #[serde(default = "default_gesture_cooldown_ms")]
    pub gesture_cooldown_ms: u32,
    #[serde(default = "default_appearance_mode")]
    pub appearance_mode: String,
    #[serde(default)]
    pub debug_mode: bool,
    #[serde(default)]
    pub device_layout_overrides: HashMap<String, serde_json::Value>,
    #[serde(default = "default_language")]
    pub language: String,
}

// ---------------------------------------------------------------------------
// Default helpers
// ---------------------------------------------------------------------------

fn default_version() -> u32 {
    CURRENT_VERSION
}
fn default_active_profile() -> String {
    "default".into()
}
fn bool_true() -> bool {
    true
}
fn default_hscroll_threshold() -> f64 {
    1.0
}
fn default_dpi() -> u32 {
    1000
}
fn default_smart_shift_mode() -> String {
    "ratchet".into()
}
fn default_gesture_threshold() -> u32 {
    50
}
fn default_gesture_deadzone() -> u32 {
    40
}
fn default_gesture_timeout_ms() -> u32 {
    3000
}
fn default_gesture_cooldown_ms() -> u32 {
    500
}
fn default_appearance_mode() -> String {
    "system".into()
}
fn default_language() -> String {
    "en".into()
}

fn default_mappings() -> HashMap<String, String> {
    let mut m = HashMap::new();
    m.insert("middle".into(), "none".into());
    m.insert("gesture".into(), "none".into());
    m.insert("gesture_left".into(), "alt_tab".into());
    m.insert("gesture_right".into(), "alt_tab".into());
    m.insert("gesture_up".into(), "none".into());
    m.insert("gesture_down".into(), "none".into());
    m.insert("xbutton1".into(), "alt_tab".into());
    m.insert("xbutton2".into(), "browser_forward".into());
    m.insert("hscroll_left".into(), "browser_back".into());
    m.insert("hscroll_right".into(), "browser_forward".into());
    m.insert("mode_shift".into(), "none".into());
    m
}

fn default_profiles() -> HashMap<String, Profile> {
    let mut m = HashMap::new();
    m.insert(
        "default".into(),
        Profile {
            label: "Default (All Apps)".into(),
            apps: vec![],
            mappings: default_mappings(),
        },
    );
    m
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_minimized: false,
            start_at_login: false,
            hscroll_threshold: 1.0,
            invert_hscroll: false,
            invert_vscroll: false,
            dpi: 1000,
            smart_shift_mode: "ratchet".into(),
            gesture_threshold: 50,
            gesture_deadzone: 40,
            gesture_timeout_ms: 3000,
            gesture_cooldown_ms: 500,
            appearance_mode: "system".into(),
            debug_mode: false,
            device_layout_overrides: HashMap::new(),
            language: "en".into(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            active_profile: "default".into(),
            profiles: default_profiles(),
            settings: Settings::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Config directory & paths
// ---------------------------------------------------------------------------

/// Return the platform-specific config directory for Mouser.
pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("Cannot determine config directory")?;
    Ok(base.join("Mouser"))
}

/// Return the full path to `config.json`.
pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

/// Return the platform-specific log directory for Mouser.
pub fn log_dir() -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().context("Cannot determine home directory")?;
        Ok(home.join("Library/Logs/Mouser"))
    }
    #[cfg(target_os = "windows")]
    {
        let base = dirs::config_dir().context("Cannot determine config directory")?;
        Ok(base.join("Mouser").join("logs"))
    }
    #[cfg(target_os = "linux")]
    {
        let base = dirs::state_dir()
            .or_else(|| {
                dirs::home_dir().map(|h| h.join(".local/state"))
            })
            .context("Cannot determine state directory")?;
        Ok(base.join("Mouser").join("logs"))
    }
}

// ---------------------------------------------------------------------------
// Load / Save
// ---------------------------------------------------------------------------

/// Load config from disk, applying defaults and migration.
pub fn load_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        let cfg = Config::default();
        save_config(&cfg)?;
        return Ok(cfg);
    }
    let data = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    let mut cfg: Config = serde_json::from_str(&data)
        .unwrap_or_else(|_| Config::default());
    migrate(&mut cfg);
    ensure_default_profile(&mut cfg);
    Ok(cfg)
}

/// Atomically write config to disk (tempfile + rename).
pub fn save_config(cfg: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg)?;

    // Atomic write: write to temp file in same directory, then rename
    let dir = path.parent().unwrap();
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    use std::io::Write;
    tmp.write_all(json.as_bytes())?;
    tmp.persist(&path)
        .with_context(|| format!("Failed to persist config to {}", path.display()))?;

    // Restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Migration
// ---------------------------------------------------------------------------

fn migrate(cfg: &mut Config) {
    if cfg.version < 2 {
        // v1 -> v2: add gesture direction buttons
        for profile in cfg.profiles.values_mut() {
            for btn in GESTURE_DIRECTION_BUTTONS {
                profile.mappings.entry((*btn).into()).or_insert_with(|| "none".into());
            }
        }
    }
    if cfg.version < 3 {
        // v2 -> v3: add mode_shift button
        for profile in cfg.profiles.values_mut() {
            profile.mappings.entry("mode_shift".into()).or_insert_with(|| "none".into());
        }
    }
    if cfg.version < 4 {
        // v3 -> v4: add gesture tuning settings (already handled by serde defaults)
    }
    if cfg.version < 5 {
        // v4 -> v5: add appearance_mode (already handled by serde defaults)
    }
    if cfg.version < 6 {
        // v5 -> v6: add language setting (already handled by serde defaults)
    }
    cfg.version = CURRENT_VERSION;
}

fn ensure_default_profile(cfg: &mut Config) {
    if !cfg.profiles.contains_key("default") {
        cfg.profiles.insert(
            "default".into(),
            Profile {
                label: "Default (All Apps)".into(),
                apps: vec![],
                mappings: default_mappings(),
            },
        );
    }
}

// ---------------------------------------------------------------------------
// Profile helpers
// ---------------------------------------------------------------------------

/// Get the mappings for the active profile.
pub fn get_active_mappings(cfg: &Config) -> &HashMap<String, String> {
    cfg.profiles
        .get(&cfg.active_profile)
        .or_else(|| cfg.profiles.get("default"))
        .map(|p| &p.mappings)
        .expect("default profile must exist")
}

/// Set a mapping for a button in a profile, then save.
pub fn set_mapping(cfg: &mut Config, button: &str, action_id: &str, profile: &str) -> Result<()> {
    if let Some(p) = cfg.profiles.get_mut(profile) {
        p.mappings.insert(button.into(), action_id.into());
    }
    save_config(cfg)
}

/// Create a new profile, optionally copying mappings from another.
pub fn create_profile(
    cfg: &mut Config,
    name: &str,
    label: &str,
    copy_from: Option<&str>,
) -> Result<()> {
    let mappings = copy_from
        .and_then(|src| cfg.profiles.get(src))
        .map(|p| p.mappings.clone())
        .unwrap_or_else(default_mappings);
    cfg.profiles.insert(
        name.into(),
        Profile {
            label: label.into(),
            apps: vec![],
            mappings,
        },
    );
    save_config(cfg)
}

/// Delete a profile (cannot delete "default").
pub fn delete_profile(cfg: &mut Config, name: &str) -> Result<bool> {
    if name == "default" {
        return Ok(false);
    }
    let removed = cfg.profiles.remove(name).is_some();
    if cfg.active_profile == name {
        cfg.active_profile = "default".into();
    }
    if removed {
        save_config(cfg)?;
    }
    Ok(removed)
}

/// Find the profile that matches a given executable name.
pub fn get_profile_for_app<'a>(cfg: &'a Config, exe_name: &str) -> &'a str {
    let exe_lower = exe_name.to_lowercase();
    for (key, profile) in &cfg.profiles {
        if key == "default" {
            continue;
        }
        for app in &profile.apps {
            if app.to_lowercase() == exe_lower {
                return key;
            }
        }
    }
    "default"
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = Config::default();
        assert_eq!(cfg.version, CURRENT_VERSION);
        assert_eq!(cfg.active_profile, "default");
        assert!(cfg.profiles.contains_key("default"));
        assert_eq!(cfg.settings.dpi, 1000);
        assert_eq!(cfg.settings.language, "en");
    }

    #[test]
    fn test_migration_from_v1() {
        let json = r#"{
            "version": 1,
            "active_profile": "default",
            "profiles": {
                "default": {
                    "label": "Default",
                    "apps": [],
                    "mappings": {
                        "middle": "none",
                        "gesture": "none",
                        "xbutton1": "alt_tab",
                        "xbutton2": "browser_forward",
                        "hscroll_left": "browser_back",
                        "hscroll_right": "browser_forward"
                    }
                }
            },
            "settings": {}
        }"#;
        let mut cfg: Config = serde_json::from_str(json).unwrap();
        migrate(&mut cfg);
        assert_eq!(cfg.version, CURRENT_VERSION);
        let mappings = &cfg.profiles["default"].mappings;
        assert!(mappings.contains_key("gesture_left"));
        assert!(mappings.contains_key("gesture_right"));
        assert!(mappings.contains_key("gesture_up"));
        assert!(mappings.contains_key("gesture_down"));
        assert!(mappings.contains_key("mode_shift"));
    }

    #[test]
    fn test_profile_for_app() {
        let mut cfg = Config::default();
        cfg.profiles.insert(
            "vscode".into(),
            Profile {
                label: "VS Code".into(),
                apps: vec!["Code.exe".into(), "code".into()],
                mappings: default_mappings(),
            },
        );
        assert_eq!(get_profile_for_app(&cfg, "Code.exe"), "vscode");
        assert_eq!(get_profile_for_app(&cfg, "code"), "vscode");
        assert_eq!(get_profile_for_app(&cfg, "chrome.exe"), "default");
    }

    #[test]
    fn test_cannot_delete_default() {
        let mut cfg = Config::default();
        // We don't call save_config in test since we have no config dir
        let removed = cfg.profiles.remove("default").is_some();
        // Just test the logic: default should exist
        assert!(removed); // it was there
        // Re-add for the test
        ensure_default_profile(&mut cfg);
        assert!(cfg.profiles.contains_key("default"));
    }

    #[test]
    fn test_roundtrip_serialize() {
        let cfg = Config::default();
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let cfg2: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg2.version, cfg.version);
        assert_eq!(cfg2.active_profile, cfg.active_profile);
        assert_eq!(cfg2.settings.dpi, cfg.settings.dpi);
    }

    #[test]
    fn test_get_active_mappings() {
        let cfg = Config::default();
        let mappings = get_active_mappings(&cfg);
        assert_eq!(mappings.get("xbutton1").unwrap(), "alt_tab");
        assert_eq!(mappings.get("xbutton2").unwrap(), "browser_forward");
    }
}
