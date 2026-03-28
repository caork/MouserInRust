//! Known Logitech device metadata used to scale Mouser beyond a single mouse model.
//!
//! This module intentionally keeps the catalog lightweight: enough structure to
//! identify common HID++ mice, surface the right model name in the UI, and hang
//! future per-device capabilities off a single place.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

pub const DEFAULT_GESTURE_CIDS: &[u16] = &[0x00C3, 0x00D7];
pub const DEFAULT_DPI_MIN: u16 = 200;
pub const DEFAULT_DPI_MAX: u16 = 8000;

pub const DEFAULT_BUTTON_LAYOUT: &[&str] = &[
    "middle",
    "gesture",
    "gesture_left",
    "gesture_right",
    "gesture_up",
    "gesture_down",
    "xbutton1",
    "xbutton2",
    "hscroll_left",
    "hscroll_right",
    "mode_shift",
];

// ---------------------------------------------------------------------------
// Device spec (compile-time catalog entry)
// ---------------------------------------------------------------------------

/// A known Logitech device specification.
///
/// All fields use `&'static str` / `&'static [..]` so the entire catalog can
/// live in read-only data with zero heap allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogiDeviceSpec {
    pub key: &'static str,
    pub display_name: &'static str,
    pub product_ids: &'static [u16],
    pub aliases: &'static [&'static str],
    pub gesture_cids: &'static [u16],
    pub ui_layout: &'static str,
    pub image_asset: &'static str,
    pub supported_buttons: &'static [&'static str],
    pub dpi_min: u16,
    pub dpi_max: u16,
}

impl LogiDeviceSpec {
    /// Check whether this spec matches a given product ID or product name.
    pub fn matches(&self, product_id: Option<u16>, product_name: Option<&str>) -> bool {
        if let Some(pid) = product_id {
            if self.product_ids.contains(&pid) {
                return true;
            }
        }
        let normalized = match product_name {
            Some(n) => normalize_name(n),
            None => return false,
        };
        if normalized.is_empty() {
            return false;
        }
        let candidates: Vec<&str> = std::iter::once(self.display_name)
            .chain(std::iter::once(self.key))
            .chain(self.aliases.iter().copied())
            .collect();
        candidates
            .iter()
            .any(|c| normalize_name(c) == normalized)
    }
}

// ---------------------------------------------------------------------------
// Connected device info (runtime, heap-allocated strings)
// ---------------------------------------------------------------------------

/// Runtime information about a connected Logitech device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectedDeviceInfo {
    pub key: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub ui_layout: String,
    pub image_asset: String,
    pub supported_buttons: Vec<String>,
    pub gesture_cids: Vec<u16>,
    pub dpi_min: u16,
    pub dpi_max: u16,
}

// ---------------------------------------------------------------------------
// Known device catalog
// ---------------------------------------------------------------------------

pub const KNOWN_LOGI_DEVICES: &[LogiDeviceSpec] = &[
    LogiDeviceSpec {
        key: "mx_master_4",
        display_name: "MX Master 4",
        product_ids: &[0xB042],
        aliases: &[
            "Logitech MX Master 4",
            "MX Master 4 for Mac",
            "MX_Master_4",
            "MX Master 4 for Business",
        ],
        gesture_cids: DEFAULT_GESTURE_CIDS,
        ui_layout: "mx_master",
        image_asset: "mouse.png",
        supported_buttons: DEFAULT_BUTTON_LAYOUT,
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: DEFAULT_DPI_MAX,
    },
    LogiDeviceSpec {
        key: "mx_master_3s",
        display_name: "MX Master 3S",
        product_ids: &[0xB034],
        aliases: &["Logitech MX Master 3S", "MX Master 3S for Mac"],
        gesture_cids: DEFAULT_GESTURE_CIDS,
        ui_layout: "mx_master",
        image_asset: "mouse.png",
        supported_buttons: DEFAULT_BUTTON_LAYOUT,
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: DEFAULT_DPI_MAX,
    },
    LogiDeviceSpec {
        key: "mx_master_3",
        display_name: "MX Master 3",
        product_ids: &[0xB023],
        aliases: &[
            "Wireless Mouse MX Master 3",
            "MX Master 3 for Mac",
            "MX Master 3 Mac",
        ],
        gesture_cids: DEFAULT_GESTURE_CIDS,
        ui_layout: "mx_master",
        image_asset: "mouse.png",
        supported_buttons: DEFAULT_BUTTON_LAYOUT,
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: DEFAULT_DPI_MAX,
    },
    LogiDeviceSpec {
        key: "mx_master_2s",
        display_name: "MX Master 2S",
        product_ids: &[0xB019],
        aliases: &["Wireless Mouse MX Master 2S"],
        gesture_cids: DEFAULT_GESTURE_CIDS,
        ui_layout: "mx_master",
        image_asset: "mouse.png",
        supported_buttons: DEFAULT_BUTTON_LAYOUT,
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: 4000,
    },
    LogiDeviceSpec {
        key: "mx_master",
        display_name: "MX Master",
        product_ids: &[0xB012],
        aliases: &["Wireless Mouse MX Master"],
        gesture_cids: DEFAULT_GESTURE_CIDS,
        ui_layout: "mx_master",
        image_asset: "mouse.png",
        supported_buttons: DEFAULT_BUTTON_LAYOUT,
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: 4000,
    },
    LogiDeviceSpec {
        key: "mx_vertical",
        display_name: "MX Vertical",
        product_ids: &[0xB020],
        aliases: &[
            "MX Vertical Wireless Mouse",
            "MX Vertical Advanced Ergonomic Mouse",
        ],
        gesture_cids: DEFAULT_GESTURE_CIDS,
        ui_layout: "mx_vertical",
        image_asset: "mouse.png",
        supported_buttons: DEFAULT_BUTTON_LAYOUT,
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: 4000,
    },
    LogiDeviceSpec {
        key: "mx_anywhere_3s",
        display_name: "MX Anywhere 3S",
        product_ids: &[0xB037],
        aliases: &["MX Anywhere 3S for Mac"],
        gesture_cids: DEFAULT_GESTURE_CIDS,
        ui_layout: "mx_anywhere",
        image_asset: "mouse.png",
        supported_buttons: DEFAULT_BUTTON_LAYOUT,
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: 8000,
    },
    LogiDeviceSpec {
        key: "mx_anywhere_3",
        display_name: "MX Anywhere 3",
        product_ids: &[0xB025],
        aliases: &["MX Anywhere 3 for Mac"],
        gesture_cids: DEFAULT_GESTURE_CIDS,
        ui_layout: "mx_anywhere",
        image_asset: "mouse.png",
        supported_buttons: DEFAULT_BUTTON_LAYOUT,
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: 4000,
    },
    LogiDeviceSpec {
        key: "mx_anywhere_2s",
        display_name: "MX Anywhere 2S",
        product_ids: &[0xB01A],
        aliases: &["Wireless Mobile Mouse MX Anywhere 2S"],
        gesture_cids: DEFAULT_GESTURE_CIDS,
        ui_layout: "mx_anywhere",
        image_asset: "mouse.png",
        supported_buttons: DEFAULT_BUTTON_LAYOUT,
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: 4000,
    },
];

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Normalize a device name for case-/whitespace-/underscore-insensitive comparison.
fn normalize_name(value: &str) -> String {
    let s = value.trim().to_lowercase().replace('_', " ");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Iterate over all known device specs.
pub fn iter_known_devices() -> impl Iterator<Item = &'static LogiDeviceSpec> {
    KNOWN_LOGI_DEVICES.iter()
}

/// Clamp a DPI value to the valid range for the given device (or global defaults).
pub fn clamp_dpi(value: i32, device: Option<&LogiDeviceSpec>) -> u16 {
    let dpi_min = device.map_or(DEFAULT_DPI_MIN, |d| d.dpi_min);
    let dpi_max = device.map_or(DEFAULT_DPI_MAX, |d| d.dpi_max);
    (value.max(dpi_min as i32).min(dpi_max as i32)) as u16
}

/// Look up a known device by product ID or product name.
pub fn resolve_device(
    product_id: Option<u16>,
    product_name: Option<&str>,
) -> Option<&'static LogiDeviceSpec> {
    KNOWN_LOGI_DEVICES
        .iter()
        .find(|d| d.matches(product_id, product_name))
}

/// Build a [`ConnectedDeviceInfo`] from whatever identifiers are available.
///
/// If the product matches a known spec the spec's metadata is used; otherwise
/// a generic fallback is constructed.
pub fn build_connected_device_info(
    product_id: Option<u16>,
    product_name: Option<&str>,
    transport: Option<&str>,
    source: Option<&str>,
    gesture_cids: Option<&[u16]>,
) -> ConnectedDeviceInfo {
    let spec = resolve_device(product_id, product_name);

    if let Some(spec) = spec {
        let cids = gesture_cids.unwrap_or(spec.gesture_cids);
        return ConnectedDeviceInfo {
            key: spec.key.to_owned(),
            display_name: spec.display_name.to_owned(),
            product_id,
            product_name: Some(
                product_name
                    .unwrap_or(spec.display_name)
                    .to_owned(),
            ),
            transport: transport.map(|s| s.to_owned()),
            source: source.map(|s| s.to_owned()),
            ui_layout: spec.ui_layout.to_owned(),
            image_asset: spec.image_asset.to_owned(),
            supported_buttons: spec
                .supported_buttons
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            gesture_cids: cids.to_vec(),
            dpi_min: spec.dpi_min,
            dpi_max: spec.dpi_max,
        };
    }

    // Fallback for unknown devices.
    let display_name = match (product_name, product_id) {
        (Some(name), _) => name.to_owned(),
        (None, Some(pid)) => format!("Logitech PID 0x{pid:04X}"),
        (None, None) => "Logitech mouse".to_owned(),
    };
    let key = {
        let norm = normalize_name(&display_name);
        if norm.is_empty() {
            "logitech_mouse".to_owned()
        } else {
            norm.replace(' ', "_")
        }
    };
    let cids = gesture_cids.unwrap_or(DEFAULT_GESTURE_CIDS);

    ConnectedDeviceInfo {
        key,
        display_name: display_name.clone(),
        product_id,
        product_name: Some(
            product_name
                .map(|s| s.to_owned())
                .unwrap_or_else(|| display_name.clone()),
        ),
        transport: transport.map(|s| s.to_owned()),
        source: source.map(|s| s.to_owned()),
        ui_layout: "generic_mouse".to_owned(),
        image_asset: "icons/mouse-simple.svg".to_owned(),
        supported_buttons: DEFAULT_BUTTON_LAYOUT
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        gesture_cids: cids.to_vec(),
        dpi_min: DEFAULT_DPI_MIN,
        dpi_max: DEFAULT_DPI_MAX,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_name() {
        assert_eq!(normalize_name("  MX_Master 3  "), "mx master 3");
        assert_eq!(normalize_name("MX  Master  3"), "mx master 3");
        assert_eq!(normalize_name(""), "");
        assert_eq!(normalize_name("   "), "");
    }

    #[test]
    fn test_resolve_device_by_product_id() {
        let dev = resolve_device(Some(0xB034), None);
        assert!(dev.is_some());
        assert_eq!(dev.unwrap().key, "mx_master_3s");
    }

    #[test]
    fn test_resolve_device_by_name() {
        let dev = resolve_device(None, Some("MX Master 3S"));
        assert!(dev.is_some());
        assert_eq!(dev.unwrap().key, "mx_master_3s");
    }

    #[test]
    fn test_resolve_device_by_alias() {
        let dev = resolve_device(None, Some("Logitech MX Master 3S"));
        assert!(dev.is_some());
        assert_eq!(dev.unwrap().key, "mx_master_3s");
    }

    #[test]
    fn test_resolve_device_case_insensitive() {
        let dev = resolve_device(None, Some("mx master 3s"));
        assert!(dev.is_some());
        assert_eq!(dev.unwrap().key, "mx_master_3s");
    }

    #[test]
    fn test_resolve_device_underscore_alias() {
        let dev = resolve_device(None, Some("MX_Master_4"));
        assert!(dev.is_some());
        assert_eq!(dev.unwrap().key, "mx_master_4");
    }

    #[test]
    fn test_resolve_device_unknown() {
        assert!(resolve_device(Some(0xFFFF), None).is_none());
        assert!(resolve_device(None, Some("Unknown Mouse")).is_none());
        assert!(resolve_device(None, None).is_none());
    }

    #[test]
    fn test_clamp_dpi_defaults() {
        assert_eq!(clamp_dpi(100, None), DEFAULT_DPI_MIN);
        assert_eq!(clamp_dpi(10000, None), DEFAULT_DPI_MAX);
        assert_eq!(clamp_dpi(1200, None), 1200);
    }

    #[test]
    fn test_clamp_dpi_with_device() {
        let dev = resolve_device(Some(0xB019), None).unwrap(); // MX Master 2S, max 4000
        assert_eq!(clamp_dpi(5000, Some(dev)), 4000);
        assert_eq!(clamp_dpi(100, Some(dev)), DEFAULT_DPI_MIN);
        assert_eq!(clamp_dpi(3000, Some(dev)), 3000);
    }

    #[test]
    fn test_iter_known_devices_count() {
        assert_eq!(iter_known_devices().count(), 9);
    }

    #[test]
    fn test_build_connected_device_info_known() {
        let info = build_connected_device_info(
            Some(0xB042),
            None,
            Some("usb"),
            Some("hidapi"),
            None,
        );
        assert_eq!(info.key, "mx_master_4");
        assert_eq!(info.display_name, "MX Master 4");
        assert_eq!(info.ui_layout, "mx_master");
        assert_eq!(info.transport.as_deref(), Some("usb"));
        assert_eq!(info.source.as_deref(), Some("hidapi"));
        assert_eq!(info.gesture_cids, DEFAULT_GESTURE_CIDS);
    }

    #[test]
    fn test_build_connected_device_info_unknown_with_pid() {
        let info = build_connected_device_info(Some(0x1234), None, None, None, None);
        assert_eq!(info.display_name, "Logitech PID 0x1234");
        assert_eq!(info.key, "logitech_pid_0x1234");
        assert_eq!(info.ui_layout, "generic_mouse");
        assert_eq!(info.image_asset, "icons/mouse-simple.svg");
    }

    #[test]
    fn test_build_connected_device_info_unknown_no_pid() {
        let info = build_connected_device_info(None, None, None, None, None);
        assert_eq!(info.display_name, "Logitech mouse");
        assert_eq!(info.key, "logitech_mouse");
    }

    #[test]
    fn test_build_connected_device_info_custom_gesture_cids() {
        let custom_cids: &[u16] = &[0x0001, 0x0002];
        let info = build_connected_device_info(Some(0xB042), None, None, None, Some(custom_cids));
        assert_eq!(info.gesture_cids, vec![0x0001, 0x0002]);
    }

    #[test]
    fn test_build_connected_device_info_with_product_name() {
        let info =
            build_connected_device_info(None, Some("My Custom Mouse"), None, None, None);
        assert_eq!(info.display_name, "My Custom Mouse");
        assert_eq!(info.key, "my_custom_mouse");
        assert_eq!(info.ui_layout, "generic_mouse");
    }

    #[test]
    fn test_connected_device_info_serialization() {
        let info = build_connected_device_info(Some(0xB042), None, None, None, None);
        let json = serde_json::to_string(&info).expect("serialize");
        let round: ConnectedDeviceInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(info, round);
    }

    #[test]
    fn test_all_specs_have_at_least_one_product_id() {
        for spec in iter_known_devices() {
            assert!(
                !spec.product_ids.is_empty(),
                "spec {} has no product IDs",
                spec.key
            );
        }
    }

    #[test]
    fn test_spec_matches_product_id_priority() {
        // Even if name doesn't match, product ID match should succeed
        let spec = &KNOWN_LOGI_DEVICES[0]; // mx_master_4
        assert!(spec.matches(Some(0xB042), Some("completely wrong name")));
    }

    #[test]
    fn test_spec_matches_none_none() {
        let spec = &KNOWN_LOGI_DEVICES[0];
        assert!(!spec.matches(None, None));
    }
}
