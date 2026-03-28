//! Platform-independent action registry.
//!
//! Defines every built-in action (id, human-readable label, category) and
//! helpers for parsing custom key-combo action IDs such as `"custom:ctrl+shift+a"`.
//!
//! This module intentionally contains **no** platform-specific key simulation
//! code -- only metadata and parsing.

// ---------------------------------------------------------------------------
// Category constants
// ---------------------------------------------------------------------------

pub const CATEGORY_NAVIGATION: &str = "Navigation";
pub const CATEGORY_BROWSER: &str = "Browser";
pub const CATEGORY_EDITING: &str = "Editing";
pub const CATEGORY_MEDIA: &str = "Media";
pub const CATEGORY_OTHER: &str = "Other";

// ---------------------------------------------------------------------------
// Action descriptor
// ---------------------------------------------------------------------------

/// A built-in action that can be bound to a mouse gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    /// Machine-readable identifier, e.g. `"copy"`, `"volume_up"`.
    pub id: &'static str,
    /// Human-readable label shown in the UI, e.g. `"Copy (Ctrl+C)"`.
    pub label: &'static str,
    /// One of the `CATEGORY_*` constants.
    pub category: &'static str,
}

// ---------------------------------------------------------------------------
// Static action table
// ---------------------------------------------------------------------------

/// Complete list of built-in actions (platform-independent metadata).
///
/// Labels follow the convention used across Windows / macOS / Linux in the
/// Python source; the platform layer is responsible for mapping each `id` to
/// the correct key codes at runtime.
pub static ACTIONS: &[Action] = &[
    // -- Navigation --
    Action { id: "alt_tab",       label: "Switch Windows",                 category: CATEGORY_NAVIGATION },
    Action { id: "alt_shift_tab", label: "Switch Windows Reverse",         category: CATEGORY_NAVIGATION },
    Action { id: "win_d",         label: "Show Desktop",                   category: CATEGORY_NAVIGATION },
    Action { id: "task_view",     label: "Task View / Mission Control",    category: CATEGORY_NAVIGATION },
    Action { id: "mission_control", label: "Mission Control",              category: CATEGORY_NAVIGATION },
    Action { id: "app_expose",    label: "App Expose",                     category: CATEGORY_NAVIGATION },
    Action { id: "show_desktop",  label: "Show Desktop",                   category: CATEGORY_NAVIGATION },
    Action { id: "launchpad",     label: "Launchpad",                      category: CATEGORY_NAVIGATION },
    Action { id: "space_left",    label: "Previous Desktop",               category: CATEGORY_NAVIGATION },
    Action { id: "space_right",   label: "Next Desktop",                   category: CATEGORY_NAVIGATION },
    Action { id: "page_up",       label: "Page Up",                        category: CATEGORY_NAVIGATION },
    Action { id: "page_down",     label: "Page Down",                      category: CATEGORY_NAVIGATION },
    Action { id: "home",          label: "Home",                           category: CATEGORY_NAVIGATION },
    Action { id: "end",           label: "End",                            category: CATEGORY_NAVIGATION },

    // -- Browser --
    Action { id: "browser_back",    label: "Browser Back",                 category: CATEGORY_BROWSER },
    Action { id: "browser_forward", label: "Browser Forward",              category: CATEGORY_BROWSER },
    Action { id: "next_tab",        label: "Next Tab",                     category: CATEGORY_BROWSER },
    Action { id: "prev_tab",        label: "Previous Tab",                 category: CATEGORY_BROWSER },
    Action { id: "close_tab",       label: "Close Tab",                    category: CATEGORY_BROWSER },
    Action { id: "new_tab",         label: "New Tab",                      category: CATEGORY_BROWSER },

    // -- Editing --
    Action { id: "copy",       label: "Copy",                              category: CATEGORY_EDITING },
    Action { id: "paste",      label: "Paste",                             category: CATEGORY_EDITING },
    Action { id: "cut",        label: "Cut",                               category: CATEGORY_EDITING },
    Action { id: "undo",       label: "Undo",                              category: CATEGORY_EDITING },
    Action { id: "select_all", label: "Select All",                        category: CATEGORY_EDITING },
    Action { id: "save",       label: "Save",                              category: CATEGORY_EDITING },
    Action { id: "find",       label: "Find",                              category: CATEGORY_EDITING },

    // -- Media --
    Action { id: "volume_up",   label: "Volume Up",                        category: CATEGORY_MEDIA },
    Action { id: "volume_down", label: "Volume Down",                      category: CATEGORY_MEDIA },
    Action { id: "volume_mute", label: "Volume Mute",                      category: CATEGORY_MEDIA },
    Action { id: "play_pause",  label: "Play / Pause",                     category: CATEGORY_MEDIA },
    Action { id: "next_track",  label: "Next Track",                       category: CATEGORY_MEDIA },
    Action { id: "prev_track",  label: "Previous Track",                   category: CATEGORY_MEDIA },

    // -- Other --
    Action { id: "none", label: "Do Nothing (Pass-through)",               category: CATEGORY_OTHER },
];

/// Look up a built-in action by its `id`. Returns `None` for unknown ids and
/// custom actions (those starting with `"custom:"`).
pub fn get_action(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

// ---------------------------------------------------------------------------
// Valid key names for custom shortcuts
// ---------------------------------------------------------------------------

/// Sorted list of key names accepted in `custom:` action IDs.
///
/// This is the union of key names across all three platform key-maps in the
/// Python source. Names are always **lowercase**.
pub static VALID_KEY_NAMES: &[&str] = &[
    "a", "alt", "b", "backspace", "c", "ctrl", "d", "delete", "down",
    "e", "end", "enter", "esc",
    "f", "f1", "f10", "f11", "f12", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9",
    "g", "h", "home", "i", "j", "k", "l", "left",
    "m", "mute", "n", "nexttrack", "o",
    "p", "pagedown", "pageup", "playpause", "prevtrack",
    "q", "r", "right", "s", "shift", "space", "super",
    "t", "tab", "u", "up", "v", "volumedown", "volumeup",
    "w", "x", "y", "z",
];

/// Returns the sorted slice of valid custom key names.
pub fn valid_custom_key_names() -> &'static [&'static str] {
    VALID_KEY_NAMES
}

/// Returns `true` when `name` (lowercase) is an accepted key name.
pub fn is_valid_key_name(name: &str) -> bool {
    VALID_KEY_NAMES.binary_search(&name).is_ok()
}

// ---------------------------------------------------------------------------
// Custom action helpers
// ---------------------------------------------------------------------------

/// Format a custom action id into a human-readable label.
///
/// `"custom:ctrl+shift+a"` becomes `"Ctrl + Shift + A"`.
///
/// Non-custom ids are returned unchanged (as a new `String`).
pub fn custom_action_label(action_id: &str) -> String {
    const PREFIX: &str = "custom:";
    if !action_id.starts_with(PREFIX) {
        return action_id.to_owned();
    }
    let combo = &action_id[PREFIX.len()..];
    let parts: Vec<&str> = combo.split('+').collect();
    let formatted: Vec<String> = parts
        .iter()
        .map(|p| capitalize(p.trim()))
        .collect();
    formatted.join(" + ")
}

/// Parse a `"custom:ctrl+a"` action id into a `Vec` of lowercase key name
/// strings. Returns `None` when the id does not start with `"custom:"` or
/// contains an unrecognised key name.
pub fn parse_custom_combo(action_id: &str) -> Option<Vec<&str>> {
    const PREFIX: &str = "custom:";
    if !action_id.starts_with(PREFIX) {
        return None;
    }
    let combo = &action_id[PREFIX.len()..];
    let mut keys: Vec<&str> = Vec::new();
    for part in combo.split('+') {
        let name = part.trim();
        if name.is_empty() {
            continue;
        }
        // Key names are always lowercase in the map; the action id is also
        // expected to be lowercase, but we verify against the canonical list.
        if !is_valid_key_name(name) {
            eprintln!("[actions] Unknown key name: {}", name);
            return None;
        }
        keys.push(name);
    }
    Some(keys)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Capitalize the first character of a string, leaving the rest as-is.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let mut out = String::with_capacity(s.len());
            for upper in c.to_uppercase() {
                out.push(upper);
            }
            out.push_str(chars.as_str());
            out
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Action table tests ------------------------------------------------

    #[test]
    fn actions_are_non_empty() {
        assert!(!ACTIONS.is_empty());
    }

    #[test]
    fn all_actions_have_valid_categories() {
        let valid = [
            CATEGORY_NAVIGATION,
            CATEGORY_BROWSER,
            CATEGORY_EDITING,
            CATEGORY_MEDIA,
            CATEGORY_OTHER,
        ];
        for action in ACTIONS {
            assert!(
                valid.contains(&action.category),
                "Action '{}' has unknown category '{}'",
                action.id,
                action.category,
            );
        }
    }

    #[test]
    fn action_ids_are_unique() {
        let mut ids: Vec<&str> = ACTIONS.iter().map(|a| a.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "Duplicate action ids found");
    }

    #[test]
    fn get_action_finds_known_ids() {
        assert_eq!(get_action("copy").unwrap().label, "Copy");
        assert_eq!(get_action("none").unwrap().category, CATEGORY_OTHER);
    }

    #[test]
    fn get_action_returns_none_for_unknown() {
        assert!(get_action("does_not_exist").is_none());
        assert!(get_action("custom:ctrl+c").is_none());
    }

    // -- Key name validation -----------------------------------------------

    #[test]
    fn valid_key_names_is_sorted() {
        let mut sorted = VALID_KEY_NAMES.to_vec();
        sorted.sort();
        assert_eq!(
            VALID_KEY_NAMES.to_vec(),
            sorted,
            "VALID_KEY_NAMES must be sorted for binary_search",
        );
    }

    #[test]
    fn common_key_names_are_valid() {
        for name in &["ctrl", "shift", "alt", "super", "a", "z", "f1", "f12",
                       "enter", "space", "tab", "esc", "delete", "backspace",
                       "left", "right", "up", "down", "home", "end",
                       "pageup", "pagedown", "volumeup", "volumedown",
                       "mute", "playpause", "nexttrack", "prevtrack"] {
            assert!(is_valid_key_name(name), "'{}' should be valid", name);
        }
    }

    #[test]
    fn invalid_key_names_rejected() {
        assert!(!is_valid_key_name("Ctrl"));   // must be lowercase
        assert!(!is_valid_key_name("foo"));
        assert!(!is_valid_key_name(""));
    }

    // -- custom_action_label -----------------------------------------------

    #[test]
    fn label_for_custom_action() {
        assert_eq!(
            custom_action_label("custom:ctrl+shift+a"),
            "Ctrl + Shift + A",
        );
    }

    #[test]
    fn label_for_single_key() {
        assert_eq!(custom_action_label("custom:f5"), "F5");
    }

    #[test]
    fn label_passthrough_for_non_custom() {
        assert_eq!(custom_action_label("copy"), "copy");
    }

    // -- parse_custom_combo ------------------------------------------------

    #[test]
    fn parse_valid_combo() {
        let keys = parse_custom_combo("custom:ctrl+shift+a").unwrap();
        assert_eq!(keys, vec!["ctrl", "shift", "a"]);
    }

    #[test]
    fn parse_single_key() {
        let keys = parse_custom_combo("custom:space").unwrap();
        assert_eq!(keys, vec!["space"]);
    }

    #[test]
    fn parse_returns_none_for_non_custom() {
        assert!(parse_custom_combo("copy").is_none());
    }

    #[test]
    fn parse_returns_none_for_unknown_key() {
        assert!(parse_custom_combo("custom:ctrl+banana").is_none());
    }

    #[test]
    fn parse_skips_empty_segments() {
        // "custom:ctrl++a" has an empty part between the two '+' signs
        let keys = parse_custom_combo("custom:ctrl++a").unwrap();
        assert_eq!(keys, vec!["ctrl", "a"]);
    }
}
