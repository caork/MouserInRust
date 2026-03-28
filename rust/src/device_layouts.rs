//! Device-layout registry for Mouser's interactive mouse view.
//!
//! The goal is to keep device-specific visual layout data out of QML so adding a
//! new Logitech family becomes a data change instead of a UI rewrite.

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A clickable region on the device image that maps to a button or feature.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hotspot {
    pub button_key: &'static str,
    pub label: &'static str,
    pub summary_type: &'static str,
    pub is_hscroll: bool,
    pub norm_x: f64,
    pub norm_y: f64,
    pub label_side: &'static str,
    pub label_off_x: i32,
    pub label_off_y: i32,
}

/// Full visual-layout descriptor for one device family.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceLayout {
    pub key: &'static str,
    pub label: &'static str,
    pub image_asset: &'static str,
    pub image_width: u32,
    pub image_height: u32,
    pub interactive: bool,
    pub manual_selectable: bool,
    pub note: &'static str,
    pub hotspots: &'static [Hotspot],
}

/// A key/label pair returned by [`get_manual_layout_choices`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutChoice {
    pub key: &'static str,
    pub label: &'static str,
}

// ---------------------------------------------------------------------------
// Hotspot data
// ---------------------------------------------------------------------------

static MX_MASTER_HOTSPOTS: &[Hotspot] = &[
    Hotspot {
        button_key: "middle",
        label: "Middle button",
        summary_type: "mapping",
        is_hscroll: false,
        norm_x: 0.33,
        norm_y: 0.45,
        label_side: "left",
        label_off_x: -80,
        label_off_y: -100,
    },
    Hotspot {
        button_key: "gesture",
        label: "Gesture button",
        summary_type: "gesture",
        is_hscroll: false,
        norm_x: 0.70,
        norm_y: 0.63,
        label_side: "left",
        label_off_x: -200,
        label_off_y: 60,
    },
    Hotspot {
        button_key: "xbutton2",
        label: "Forward button",
        summary_type: "mapping",
        is_hscroll: false,
        norm_x: 0.60,
        norm_y: 0.48,
        label_side: "left",
        label_off_x: -300,
        label_off_y: 0,
    },
    Hotspot {
        button_key: "xbutton1",
        label: "Back button",
        summary_type: "mapping",
        is_hscroll: false,
        norm_x: 0.65,
        norm_y: 0.40,
        label_side: "right",
        label_off_x: 200,
        label_off_y: 50,
    },
    Hotspot {
        button_key: "hscroll_left",
        label: "Horizontal scroll",
        summary_type: "hscroll",
        is_hscroll: true,
        norm_x: 0.60,
        norm_y: 0.375,
        label_side: "right",
        label_off_x: 200,
        label_off_y: -50,
    },
    Hotspot {
        button_key: "mode_shift",
        label: "Mode shift button",
        summary_type: "mapping",
        is_hscroll: false,
        norm_x: 0.43,
        norm_y: 0.25,
        label_side: "right",
        label_off_x: 150,
        label_off_y: -80,
    },
];

// ---------------------------------------------------------------------------
// Layout definitions
// ---------------------------------------------------------------------------

pub static MX_MASTER_LAYOUT: DeviceLayout = DeviceLayout {
    key: "mx_master",
    label: "MX Master family",
    image_asset: "mouse.png",
    image_width: 460,
    image_height: 360,
    interactive: true,
    manual_selectable: true,
    note: "",
    hotspots: MX_MASTER_HOTSPOTS,
};

pub static GENERIC_MOUSE_LAYOUT: DeviceLayout = DeviceLayout {
    key: "generic_mouse",
    label: "Generic mouse",
    image_asset: "icons/mouse-simple.svg",
    image_width: 220,
    image_height: 220,
    interactive: false,
    manual_selectable: false,
    note: "This device is detected and the backend can still probe HID++ features, \
           but Mouser does not have a dedicated visual overlay for it yet.",
    hotspots: &[],
};

pub static MX_ANYWHERE_LAYOUT: DeviceLayout = DeviceLayout {
    key: "mx_anywhere",
    label: "MX Anywhere family",
    image_asset: GENERIC_MOUSE_LAYOUT.image_asset,
    image_width: GENERIC_MOUSE_LAYOUT.image_width,
    image_height: GENERIC_MOUSE_LAYOUT.image_height,
    interactive: GENERIC_MOUSE_LAYOUT.interactive,
    manual_selectable: GENERIC_MOUSE_LAYOUT.manual_selectable,
    note: "MX Anywhere support is wired for device detection and HID++ probing. \
           A dedicated overlay image and hotspot map still need to be added.",
    hotspots: GENERIC_MOUSE_LAYOUT.hotspots,
};

pub static MX_VERTICAL_LAYOUT: DeviceLayout = DeviceLayout {
    key: "mx_vertical",
    label: "MX Vertical family",
    image_asset: GENERIC_MOUSE_LAYOUT.image_asset,
    image_width: GENERIC_MOUSE_LAYOUT.image_width,
    image_height: GENERIC_MOUSE_LAYOUT.image_height,
    interactive: GENERIC_MOUSE_LAYOUT.interactive,
    manual_selectable: GENERIC_MOUSE_LAYOUT.manual_selectable,
    note: "MX Vertical uses a different physical shape, so Mouser falls back to a \
           generic device card until a dedicated overlay is added.",
    hotspots: GENERIC_MOUSE_LAYOUT.hotspots,
};

// ---------------------------------------------------------------------------
// Registry (ordered to match the Python DEVICE_LAYOUTS dict)
// ---------------------------------------------------------------------------

/// All known device layouts, keyed by their `key` field.
static DEVICE_LAYOUTS: &[&DeviceLayout] = &[
    &MX_MASTER_LAYOUT,
    &MX_ANYWHERE_LAYOUT,
    &MX_VERTICAL_LAYOUT,
    &GENERIC_MOUSE_LAYOUT,
];

// ---------------------------------------------------------------------------
// Public helpers
// ---------------------------------------------------------------------------

/// Return the layout for `layout_key`, falling back to [`GENERIC_MOUSE_LAYOUT`].
///
/// The returned value is a *clone* (matching the Python `deepcopy` semantics)
/// so callers may mutate it freely.
pub fn get_device_layout(layout_key: Option<&str>) -> DeviceLayout {
    let key = layout_key.unwrap_or("");
    DEVICE_LAYOUTS
        .iter()
        .find(|l| l.key == key)
        .copied()
        .cloned()
        .unwrap_or_else(|| GENERIC_MOUSE_LAYOUT.clone())
}

/// Return the list of layouts the user may select manually.
///
/// The first entry is always `("", "Auto-detect")`.
pub fn get_manual_layout_choices() -> Vec<LayoutChoice> {
    let mut choices = vec![LayoutChoice {
        key: "",
        label: "Auto-detect",
    }];
    for layout in DEVICE_LAYOUTS.iter() {
        if layout.manual_selectable {
            choices.push(LayoutChoice {
                key: layout.key,
                label: layout.label,
            });
        }
    }
    choices
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mx_master_hotspot_count() {
        assert_eq!(MX_MASTER_LAYOUT.hotspots.len(), 6);
    }

    #[test]
    fn test_mx_master_is_interactive() {
        assert!(MX_MASTER_LAYOUT.interactive);
        assert!(MX_MASTER_LAYOUT.manual_selectable);
    }

    #[test]
    fn test_generic_mouse_has_no_hotspots() {
        assert!(GENERIC_MOUSE_LAYOUT.hotspots.is_empty());
        assert!(!GENERIC_MOUSE_LAYOUT.interactive);
    }

    #[test]
    fn test_mx_anywhere_inherits_generic_dimensions() {
        assert_eq!(MX_ANYWHERE_LAYOUT.image_width, GENERIC_MOUSE_LAYOUT.image_width);
        assert_eq!(MX_ANYWHERE_LAYOUT.image_height, GENERIC_MOUSE_LAYOUT.image_height);
        assert!(!MX_ANYWHERE_LAYOUT.interactive);
    }

    #[test]
    fn test_mx_vertical_inherits_generic_dimensions() {
        assert_eq!(MX_VERTICAL_LAYOUT.image_width, GENERIC_MOUSE_LAYOUT.image_width);
        assert_eq!(MX_VERTICAL_LAYOUT.image_height, GENERIC_MOUSE_LAYOUT.image_height);
        assert!(!MX_VERTICAL_LAYOUT.interactive);
    }

    #[test]
    fn test_get_device_layout_known_key() {
        let layout = get_device_layout(Some("mx_master"));
        assert_eq!(layout.key, "mx_master");
        assert_eq!(layout.hotspots.len(), 6);
    }

    #[test]
    fn test_get_device_layout_unknown_falls_back() {
        let layout = get_device_layout(Some("nonexistent"));
        assert_eq!(layout.key, "generic_mouse");
    }

    #[test]
    fn test_get_device_layout_none_falls_back() {
        let layout = get_device_layout(None);
        assert_eq!(layout.key, "generic_mouse");
    }

    #[test]
    fn test_get_device_layout_empty_falls_back() {
        let layout = get_device_layout(Some(""));
        assert_eq!(layout.key, "generic_mouse");
    }

    #[test]
    fn test_manual_layout_choices_starts_with_auto() {
        let choices = get_manual_layout_choices();
        assert!(!choices.is_empty());
        assert_eq!(choices[0].key, "");
        assert_eq!(choices[0].label, "Auto-detect");
    }

    #[test]
    fn test_manual_layout_choices_includes_mx_master() {
        let choices = get_manual_layout_choices();
        assert!(choices.iter().any(|c| c.key == "mx_master"));
    }

    #[test]
    fn test_manual_layout_choices_excludes_non_selectable() {
        let choices = get_manual_layout_choices();
        // generic_mouse, mx_anywhere, mx_vertical are not manual_selectable
        assert!(!choices.iter().any(|c| c.key == "generic_mouse"));
        assert!(!choices.iter().any(|c| c.key == "mx_anywhere"));
        assert!(!choices.iter().any(|c| c.key == "mx_vertical"));
    }

    #[test]
    fn test_hotspot_hscroll_flag() {
        let hscroll = MX_MASTER_LAYOUT
            .hotspots
            .iter()
            .find(|h| h.button_key == "hscroll_left")
            .expect("hscroll_left hotspot missing");
        assert!(hscroll.is_hscroll);

        // All other hotspots should have is_hscroll == false
        for h in MX_MASTER_LAYOUT.hotspots.iter() {
            if h.button_key != "hscroll_left" {
                assert!(!h.is_hscroll, "{} should not be hscroll", h.button_key);
            }
        }
    }

    #[test]
    fn test_device_layouts_registry_has_all_four() {
        assert_eq!(DEVICE_LAYOUTS.len(), 4);
        let keys: Vec<&str> = DEVICE_LAYOUTS.iter().map(|l| l.key).collect();
        assert!(keys.contains(&"mx_master"));
        assert!(keys.contains(&"mx_anywhere"));
        assert!(keys.contains(&"mx_vertical"));
        assert!(keys.contains(&"generic_mouse"));
    }

    #[test]
    fn test_hotspot_norm_coords_in_range() {
        for layout in DEVICE_LAYOUTS.iter() {
            for h in layout.hotspots.iter() {
                assert!(
                    (0.0..=1.0).contains(&h.norm_x),
                    "{}/{}: normX out of range",
                    layout.key,
                    h.button_key
                );
                assert!(
                    (0.0..=1.0).contains(&h.norm_y),
                    "{}/{}: normY out of range",
                    layout.key,
                    h.button_key
                );
            }
        }
    }
}
