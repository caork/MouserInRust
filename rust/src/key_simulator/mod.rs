//! Cross-platform keyboard / scroll simulator.
//!
//! # Public API
//!
//! - [`execute_action`] — dispatch a built-in action id (e.g. `"copy"`) or a
//!   `"custom:ctrl+shift+a"` combo.
//! - [`inject_scroll`] — inject a vertical or horizontal scroll event.
//!
//! All heavy lifting is delegated to the platform-specific sub-modules.
//! The `custom:` parsing goes through [`crate::actions::parse_custom_combo`],
//! which validates key names against the shared `VALID_KEY_NAMES` list, then
//! each platform translates those names to its native key codes.

#![allow(dead_code)]

// ---------------------------------------------------------------------------
// Platform sub-modules
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

// ---------------------------------------------------------------------------
// Shared helpers (re-exported from actions.rs)
// ---------------------------------------------------------------------------

use crate::actions::{custom_action_label as _custom_action_label, parse_custom_combo};

/// Default key-hold duration in milliseconds.
const DEFAULT_HOLD_MS: u64 = 50;

// ---------------------------------------------------------------------------
// execute_action
// ---------------------------------------------------------------------------

/// Dispatch `action_id` to the correct platform implementation.
///
/// Handles:
/// - `"none"` — no-op, returns immediately.
/// - `"custom:ctrl+shift+a"` — parsed via [`parse_custom_combo`] then
///   forwarded to the platform key-combo sender.
/// - Any of the 33+ predefined action ids in [`crate::actions::ACTIONS`].
///
/// Unknown ids are logged as warnings and silently ignored (returns `Ok(())`).
pub fn execute_action(action_id: &str) -> anyhow::Result<()> {
    execute_action_with_hold(action_id, DEFAULT_HOLD_MS)
}

/// Like [`execute_action`] but with an explicit hold duration.
pub fn execute_action_with_hold(action_id: &str, hold_ms: u64) -> anyhow::Result<()> {
    if action_id == "none" {
        return Ok(());
    }

    if action_id.starts_with("custom:") {
        return dispatch_custom(action_id, hold_ms);
    }

    dispatch_builtin(action_id, hold_ms)
}

// ---------------------------------------------------------------------------
// inject_scroll
// ---------------------------------------------------------------------------

/// Inject a scroll event.
///
/// - `horizontal = false` → vertical scroll (positive = up, negative = down)
/// - `horizontal = true`  → horizontal scroll (positive = right, negative = left)
///
/// `delta` uses the Windows WHEEL_DELTA convention: ±120 per detent.
/// On Linux the value is converted to evdev detents internally.
pub fn inject_scroll(horizontal: bool, delta: i32) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    return windows::inject_scroll(horizontal, delta);

    #[cfg(target_os = "macos")]
    return macos::inject_scroll(horizontal, delta);

    #[cfg(target_os = "linux")]
    return linux::inject_scroll(horizontal, delta);

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        log::warn!("[key_simulator] inject_scroll: unsupported platform");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Internal dispatch helpers
// ---------------------------------------------------------------------------

fn dispatch_builtin(action_id: &str, hold_ms: u64) -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    return windows::execute_action(action_id, hold_ms);

    #[cfg(target_os = "macos")]
    return macos::execute_action(action_id, hold_ms);

    #[cfg(target_os = "linux")]
    return linux::execute_action(action_id, hold_ms);

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        log::warn!(
            "[key_simulator] execute_action '{}': unsupported platform",
            action_id
        );
        Ok(())
    }
}

fn dispatch_custom(action_id: &str, hold_ms: u64) -> anyhow::Result<()> {
    // parse_custom_combo validates key names against VALID_KEY_NAMES and
    // returns lowercase name slices.  The platform layer converts each name
    // to its native key code.
    let names = match parse_custom_combo(action_id) {
        Some(n) if !n.is_empty() => n,
        Some(_) => {
            // Empty combo after parsing (e.g. "custom:") — silently ignore.
            return Ok(());
        }
        None => {
            log::warn!(
                "[key_simulator] custom action '{}' contains unknown key name(s)",
                action_id
            );
            return Ok(());
        }
    };

    #[cfg(target_os = "windows")]
    {
        let vk_codes: Vec<u16> = names
            .iter()
            .filter_map(|n| {
                let code = windows::key_name_to_vk(n);
                if code.is_none() {
                    log::warn!("[key_simulator] no Windows VK for key name '{}'", n);
                }
                code
            })
            .collect();
        return windows::execute_custom(&vk_codes, hold_ms);
    }

    #[cfg(target_os = "macos")]
    {
        let keycodes: Vec<macos::CGKeyCode> = names
            .iter()
            .filter_map(|n| {
                let code = macos::key_name_to_keycode(n);
                if code.is_none() {
                    log::warn!("[key_simulator] no macOS keycode for key name '{}'", n);
                }
                code
            })
            .collect();
        return macos::execute_custom(&keycodes, hold_ms);
    }

    #[cfg(target_os = "linux")]
    {
        let keys: Vec<evdev::Key> = names
            .iter()
            .filter_map(|n| {
                let k = linux::key_name_to_key(n);
                if k.is_none() {
                    log::warn!("[key_simulator] no evdev Key for key name '{}'", n);
                }
                k
            })
            .collect();
        return linux::execute_custom(&keys, hold_ms);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        log::warn!(
            "[key_simulator] dispatch_custom '{}': unsupported platform",
            action_id
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Re-export the label helper so callers don't need to import actions directly
// ---------------------------------------------------------------------------

/// Format a `custom:ctrl+shift+a` action id into `"Ctrl + Shift + A"`.
/// Non-custom ids are returned unchanged.
pub fn custom_action_label(action_id: &str) -> String {
    _custom_action_label(action_id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- custom_action_label (pure string logic, no platform I/O) -----------

    #[test]
    fn label_formats_correctly() {
        assert_eq!(
            custom_action_label("custom:ctrl+shift+a"),
            "Ctrl + Shift + A"
        );
    }

    #[test]
    fn label_single_key() {
        assert_eq!(custom_action_label("custom:f5"), "F5");
    }

    #[test]
    fn label_passthrough_for_non_custom() {
        assert_eq!(custom_action_label("copy"), "copy");
    }

    #[test]
    fn label_super_key() {
        assert_eq!(custom_action_label("custom:super+d"), "Super + D");
    }

    // -- execute_action with "none" (no platform I/O) ----------------------

    #[test]
    fn execute_none_is_ok() {
        assert!(execute_action("none").is_ok());
    }

    // -- custom combo parsing edge cases -----------------------------------

    #[test]
    fn empty_custom_combo_is_ok() {
        // "custom:" with nothing after the colon — should not panic
        assert!(execute_action("custom:").is_ok());
    }

    #[test]
    fn unknown_key_name_in_custom_returns_ok() {
        // Unknown key → parse_custom_combo returns None → warning + Ok(())
        assert!(execute_action("custom:ctrl+banana").is_ok());
    }

    // -- parse_custom_combo integration ------------------------------------

    #[test]
    fn parse_custom_combo_valid() {
        let keys = parse_custom_combo("custom:ctrl+c").unwrap();
        assert_eq!(keys, vec!["ctrl", "c"]);
    }

    #[test]
    fn parse_custom_combo_invalid_key() {
        assert!(parse_custom_combo("custom:ctrl+notakey").is_none());
    }

    #[test]
    fn parse_custom_combo_non_custom_id() {
        assert!(parse_custom_combo("copy").is_none());
    }

    // -- Platform key-name maps (compile-time reachability) ----------------
    // These tests verify that every key name in VALID_KEY_NAMES resolves to
    // a code on the current platform.  They are gated by the same cfg flags
    // as the platform modules so they compile and run on their respective OS.

    #[cfg(target_os = "windows")]
    #[test]
    fn all_valid_key_names_resolve_on_windows() {
        use crate::actions::VALID_KEY_NAMES;
        for name in VALID_KEY_NAMES {
            assert!(
                crate::key_simulator::windows::key_name_to_vk(name).is_some(),
                "Windows: no VK for '{}'",
                name
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn all_valid_key_names_resolve_on_macos() {
        use crate::actions::VALID_KEY_NAMES;
        // Media key names (volumeup, mute, etc.) do not map to a CGKeyCode
        // because they go through the NSEvent pathway — exclude them here.
        let media_names = &[
            "volumeup", "volumedown", "mute", "playpause", "nexttrack", "prevtrack",
        ];
        for name in VALID_KEY_NAMES {
            if media_names.contains(name) {
                continue;
            }
            assert!(
                crate::key_simulator::macos::key_name_to_keycode(name).is_some(),
                "macOS: no keycode for '{}'",
                name
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn all_valid_key_names_resolve_on_linux() {
        use crate::actions::VALID_KEY_NAMES;
        for name in VALID_KEY_NAMES {
            assert!(
                crate::key_simulator::linux::key_name_to_key(name).is_some(),
                "Linux: no evdev Key for '{}'",
                name
            );
        }
    }
}
