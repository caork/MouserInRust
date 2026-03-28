//! Locale Manager — provides i18n support for the UI.
//! Supports English (en), Simplified Chinese (zh_CN), and Traditional Chinese (zh_TW).

use std::sync::Mutex;

// ---------------------------------------------------------------------------
// Language enum
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    ZhCN,
    ZhTW,
}

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::ZhCN => "zh_CN",
            Language::ZhTW => "zh_TW",
        }
    }

    pub fn from_code(code: &str) -> Option<Language> {
        match code {
            "en" => Some(Language::En),
            "zh_CN" => Some(Language::ZhCN),
            "zh_TW" => Some(Language::ZhTW),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Available languages list
// ---------------------------------------------------------------------------

pub const AVAILABLE_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("zh_CN", "\u{7b80}\u{4f53}\u{4e2d}\u{6587}"),
    ("zh_TW", "\u{7e41}\u{9ad4}\u{4e2d}\u{6587}"),
];

// ---------------------------------------------------------------------------
// General UI translations (_TRANSLATIONS)
// ---------------------------------------------------------------------------

/// Each entry: (key, en, zh_CN, zh_TW)
static TRANSLATIONS: &[(&str, &str, &str, &str)] = &[
    // Navigation sidebar
    ("nav.mouse_profiles", "Mouse & Profiles", "\u{9f20}\u{6807}\u{4e0e}\u{914d}\u{7f6e}\u{6587}\u{4ef6}", "\u{6ed1}\u{9f20}\u{8207}\u{8a2d}\u{5b9a}\u{6a94}"),
    ("nav.point_scroll", "Point & Scroll", "\u{6307}\u{9488}\u{4e0e}\u{6eda}\u{8f6e}", "\u{6307}\u{6a19}\u{8207}\u{6372}\u{8ef8}"),

    // Mouse page — profile list
    ("mouse.profiles", "Profiles", "\u{914d}\u{7f6e}\u{6587}\u{4ef6}", "\u{8a2d}\u{5b9a}\u{6a94}"),
    ("mouse.all_applications", "All applications", "\u{6240}\u{6709}\u{5e94}\u{7528}\u{7a0b}\u{5e8f}", "\u{6240}\u{6709}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}"),
    ("mouse.add_app_profile", "Add App Profile", "\u{6dfb}\u{52a0}\u{5e94}\u{7528}\u{914d}\u{7f6e}", "\u{65b0}\u{589e}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{8a2d}\u{5b9a}\u{6a94}"),
    ("mouse.search_installed_apps", "Search installed apps or browse for one manually", "\u{641c}\u{7d22}\u{5df2}\u{5b89}\u{88c5}\u{7684}\u{5e94}\u{7528}\u{6216}\u{624b}\u{52a8}\u{6d4f}\u{89c8}", "\u{641c}\u{5c0b}\u{5df2}\u{5b89}\u{88dd}\u{7684}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{6216}\u{624b}\u{52d5}\u{700f}\u{89bd}"),
    ("mouse.delete_profile", "Delete Profile", "\u{5220}\u{9664}\u{914d}\u{7f6e}", "\u{522a}\u{9664}\u{8a2d}\u{5b9a}\u{6a94}"),

    // Mouse page — connection / status
    ("mouse.connected", "Connected", "\u{5df2}\u{8fde}\u{63a5}", "\u{5df2}\u{9023}\u{7dda}"),
    ("mouse.not_connected", "Not Connected", "\u{672a}\u{8fde}\u{63a5}", "\u{672a}\u{9023}\u{7dda}"),
    ("mouse.waiting_for_connection", "Waiting for connection", "\u{7b49}\u{5f85}\u{8fde}\u{63a5}", "\u{7b49}\u{5f85}\u{9023}\u{7dda}"),
    ("mouse.connect_mouse", "Connect your Logitech mouse", "\u{8fde}\u{63a5}\u{60a8}\u{7684}\u{7f57}\u{6280}\u{9f20}\u{6807}", "\u{9023}\u{63a5}\u{60a8}\u{7684}\u{7f85}\u{6280}\u{6ed1}\u{9f20}"),
    ("mouse.connect_mouse_desc", "Mouser will detect the active device, unlock button mapping, and enable the correct layout mode as soon as the mouse is available.", "Mouser \u{5c06}\u{68c0}\u{6d4b}\u{6d3b}\u{52a8}\u{8bbe}\u{5907}\u{ff0c}\u{89e3}\u{9501}\u{6309}\u{952e}\u{6620}\u{5c04}\u{ff0c}\u{5e76}\u{5728}\u{9f20}\u{6807}\u{53ef}\u{7528}\u{540e}\u{542f}\u{7528}\u{6b63}\u{786e}\u{7684}\u{5e03}\u{5c40}\u{6a21}\u{5f0f}\u{3002}", "Mouser \u{5c07}\u{5075}\u{6e2c}\u{6d3b}\u{52d5}\u{88dd}\u{7f6e}\u{ff0c}\u{89e3}\u{9396}\u{6309}\u{9375}\u{5c0d}\u{6620}\u{ff0c}\u{4e26}\u{5728}\u{6ed1}\u{9f20}\u{53ef}\u{7528}\u{5f8c}\u{555f}\u{7528}\u{6b63}\u{78ba}\u{7684}\u{7248}\u{9762}\u{914d}\u{7f6e}\u{6a21}\u{5f0f}\u{3002}"),
    ("mouse.layout_appears_auto", "Layout mode appears automatically", "\u{5e03}\u{5c40}\u{6a21}\u{5f0f}\u{81ea}\u{52a8}\u{663e}\u{793a}", "\u{7248}\u{9762}\u{914d}\u{7f6e}\u{6a21}\u{5f0f}\u{81ea}\u{52d5}\u{986f}\u{793a}"),
    ("mouse.per_device_settings", "Per-device settings stay separate", "\u{6bcf}\u{4e2a}\u{8bbe}\u{5907}\u{7684}\u{8bbe}\u{7f6e}\u{72ec}\u{7acb}\u{4fdd}\u{5b58}", "\u{6bcf}\u{500b}\u{88dd}\u{7f6e}\u{7684}\u{8a2d}\u{5b9a}\u{5206}\u{958b}\u{5132}\u{5b58}"),

    // Mouse page — header subtitles
    ("mouse.turn_on_mouse", "Turn on your Logitech mouse to start customizing buttons", "\u{6253}\u{5f00}\u{60a8}\u{7684}\u{7f57}\u{6280}\u{9f20}\u{6807}\u{4ee5}\u{5f00}\u{59cb}\u{81ea}\u{5b9a}\u{4e49}\u{6309}\u{952e}", "\u{958b}\u{555f}\u{60a8}\u{7684}\u{7f85}\u{6280}\u{6ed1}\u{9f20}\u{4ee5}\u{958b}\u{59cb}\u{81ea}\u{8a02}\u{6309}\u{9375}"),
    ("mouse.click_dot", "Click a dot to configure its action", "\u{70b9}\u{51fb}\u{5706}\u{70b9}\u{914d}\u{7f6e}\u{5176}\u{52a8}\u{4f5c}", "\u{9ede}\u{64ca}\u{5713}\u{9ede}\u{4ee5}\u{8a2d}\u{5b9a}\u{5176}\u{52d5}\u{4f5c}"),
    ("mouse.choose_layout", "Choose a layout mode below while we build a dedicated overlay", "\u{5728}\u{6211}\u{4eec}\u{6784}\u{5efa}\u{4e13}\u{5c5e}\u{8986}\u{76d6}\u{5c42}\u{7684}\u{540c}\u{65f6}\u{ff0c}\u{8bf7}\u{5728}\u{4e0b}\u{65b9}\u{9009}\u{62e9}\u{5e03}\u{5c40}\u{6a21}\u{5f0f}", "\u{5728}\u{6211}\u{5011}\u{5efa}\u{7acb}\u{5c08}\u{5c6c}\u{8986}\u{84cb}\u{5c64}\u{7684}\u{540c}\u{6642}\u{ff0c}\u{8acb}\u{5728}\u{4e0b}\u{65b9}\u{9078}\u{64c7}\u{7248}\u{9762}\u{914d}\u{7f6e}\u{6a21}\u{5f0f}"),

    // Mouse page — layout mode
    ("mouse.layout_mode", "Layout mode", "\u{5e03}\u{5c40}\u{6a21}\u{5f0f}", "\u{7248}\u{9762}\u{914d}\u{7f6e}\u{6a21}\u{5f0f}"),
    ("mouse.experimental_override_prefix", "Experimental override active: ", "\u{5b9e}\u{9a8c}\u{6027}\u{8986}\u{76d6}\u{5df2}\u{6fc0}\u{6d3b}\u{ff1a}", "\u{5be6}\u{9a57}\u{6027}\u{8986}\u{84cb}\u{5df2}\u{555f}\u{7528}\u{ff1a}"),
    ("mouse.experimental_override_suffix", ". Switch back to Auto-detect if the hotspot map does not line up.", "\u{3002}\u{5982}\u{679c}\u{70ed}\u{70b9}\u{56fe}\u{4e0d}\u{5bf9}\u{9f50}\u{ff0c}\u{8bf7}\u{5207}\u{6362}\u{56de}\u{81ea}\u{52a8}\u{68c0}\u{6d4b}\u{3002}", "\u{3002}\u{5982}\u{679c}\u{71b1}\u{9ede}\u{5716}\u{4e0d}\u{5c0d}\u{9f4a}\u{ff0c}\u{8acb}\u{5207}\u{63db}\u{56de}\u{81ea}\u{52d5}\u{5075}\u{6e2c}\u{3002}"),
    ("mouse.interactive_layout_coming", "Interactive layout coming later", "\u{4ea4}\u{4e92}\u{5f0f}\u{5e03}\u{5c40}\u{5373}\u{5c06}\u{63a8}\u{51fa}", "\u{4e92}\u{52d5}\u{5f0f}\u{7248}\u{9762}\u{914d}\u{7f6e}\u{5373}\u{5c07}\u{63a8}\u{51fa}"),
    ("mouse.auto_detect", "Auto-detect", "\u{81ea}\u{52a8}\u{68c0}\u{6d4b}", "\u{81ea}\u{52d5}\u{5075}\u{6e2c}"),

    // Mouse page — action / mapping helpers
    ("mouse.do_nothing", "Do Nothing", "\u{65e0}\u{64cd}\u{4f5c}", "\u{7121}\u{52d5}\u{4f5c}"),
    ("mouse.horizontal_scroll", "Horizontal Scroll", "\u{6c34}\u{5e73}\u{6eda}\u{52a8}", "\u{6c34}\u{5e73}\u{6372}\u{52d5}"),
    ("mouse.tap", "Tap: ", "\u{70b9}\u{51fb}\u{ff1a}", "\u{9ede}\u{64ca}\u{ff1a}"),
    ("mouse.swipes_configured", "Swipes configured", "\u{5df2}\u{914d}\u{7f6e}\u{6ed1}\u{52a8}", "\u{5df2}\u{8a2d}\u{5b9a}\u{6ed1}\u{52d5}"),
    ("mouse.installed_app", "Installed app", "\u{5df2}\u{5b89}\u{88c5}\u{7684}\u{5e94}\u{7528}", "\u{5df2}\u{5b89}\u{88dd}\u{7684}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}"),
    ("mouse.applications", "Applications", "\u{5e94}\u{7528}\u{7a0b}\u{5e8f}", "\u{61c9}\u{7528}\u{7a0b}\u{5f0f}"),
    ("mouse.system_applications", "System Applications", "\u{7cfb}\u{7edf}\u{5e94}\u{7528}\u{7a0b}\u{5e8f}", "\u{7cfb}\u{7d71}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}"),
    ("mouse.macos_coreservices", "macOS CoreServices", "macOS \u{6838}\u{5fc3}\u{670d}\u{52a1}", "macOS \u{6838}\u{5fc3}\u{670d}\u{52d9}"),

    // Mouse page — action picker
    ("mouse.choose_action_suffix", " \u{2014} Choose Action", " \u{2014} \u{9009}\u{62e9}\u{52a8}\u{4f5c}", " \u{2014} \u{9078}\u{64c7}\u{52d5}\u{4f5c}"),
    ("mouse.configure_scroll_actions", "Configure separate actions for scroll left and right", "\u{4e3a}\u{5411}\u{5de6}\u{548c}\u{5411}\u{53f3}\u{6eda}\u{52a8}\u{5206}\u{522b}\u{914d}\u{7f6e}\u{52a8}\u{4f5c}", "\u{5206}\u{5225}\u{8a2d}\u{5b9a}\u{5411}\u{5de6}\u{548c}\u{5411}\u{53f3}\u{6372}\u{52d5}\u{7684}\u{52d5}\u{4f5c}"),
    ("mouse.configure_gesture", "Configure tap behavior plus swipe actions for the gesture button", "\u{914d}\u{7f6e}\u{624b}\u{52bf}\u{6309}\u{9215}\u{7684}\u{70b9}\u{51fb}\u{884c}\u{4e3a}\u{548c}\u{6ed1}\u{52a8}\u{52a8}\u{4f5c}", "\u{8a2d}\u{5b9a}\u{624b}\u{52e2}\u{6309}\u{9215}\u{7684}\u{9ede}\u{64ca}\u{884c}\u{70ba}\u{548c}\u{6ed1}\u{52d5}\u{52d5}\u{4f5c}"),
    ("mouse.select_button_action", "Select what happens when you use this button", "\u{9009}\u{62e9}\u{4f7f}\u{7528}\u{6b64}\u{6309}\u{952e}\u{65f6}\u{89e6}\u{53d1}\u{7684}\u{52a8}\u{4f5c}", "\u{9078}\u{64c7}\u{4f7f}\u{7528}\u{6b64}\u{6309}\u{9375}\u{6642}\u{89f8}\u{767c}\u{7684}\u{52d5}\u{4f5c}"),
    ("mouse.scroll_left", "SCROLL LEFT", "\u{5411}\u{5de6}\u{6eda}\u{52a8}", "\u{5411}\u{5de6}\u{6372}\u{52d5}"),
    ("mouse.scroll_right", "SCROLL RIGHT", "\u{5411}\u{53f3}\u{6eda}\u{52a8}", "\u{5411}\u{53f3}\u{6372}\u{52d5}"),
    ("mouse.tap_action", "TAP ACTION", "\u{70b9}\u{51fb}\u{52a8}\u{4f5c}", "\u{9ede}\u{64ca}\u{52d5}\u{4f5c}"),
    ("mouse.swipe_actions", "SWIPE ACTIONS", "\u{6ed1}\u{52a8}\u{52a8}\u{4f5c}", "\u{6ed1}\u{52d5}\u{52d5}\u{4f5c}"),
    ("mouse.swipe_left", "Swipe left", "\u{5411}\u{5de6}\u{6ed1}\u{52a8}", "\u{5411}\u{5de6}\u{6ed1}\u{52d5}"),
    ("mouse.swipe_right", "Swipe right", "\u{5411}\u{53f3}\u{6ed1}\u{52a8}", "\u{5411}\u{53f3}\u{6ed1}\u{52d5}"),
    ("mouse.swipe_up", "Swipe up", "\u{5411}\u{4e0a}\u{6ed1}\u{52a8}", "\u{5411}\u{4e0a}\u{6ed1}\u{52d5}"),
    ("mouse.swipe_down", "Swipe down", "\u{5411}\u{4e0b}\u{6ed1}\u{52a8}", "\u{5411}\u{4e0b}\u{6ed1}\u{52d5}"),
    ("mouse.threshold", "Threshold", "\u{9608}\u{503c}", "\u{95be}\u{5024}"),

    // Mouse page — debug panel
    ("mouse.debug_events", "Debug Events", "\u{8c03}\u{8bd5}\u{4e8b}\u{4ef6}", "\u{9664}\u{932f}\u{4e8b}\u{4ef6}"),
    ("mouse.debug_events_desc", "Collects detected buttons, gestures, and mapped actions", "\u{6536}\u{96c6}\u{68c0}\u{6d4b}\u{5230}\u{7684}\u{6309}\u{952e}\u{3001}\u{624b}\u{52bf}\u{548c}\u{6620}\u{5c04}\u{52a8}\u{4f5c}", "\u{6536}\u{96c6}\u{5075}\u{6e2c}\u{5230}\u{7684}\u{6309}\u{9375}\u{3001}\u{624b}\u{52e2}\u{548c}\u{5c0d}\u{6620}\u{52d5}\u{4f5c}"),
    ("mouse.clear", "Clear", "\u{6e05}\u{9664}", "\u{6e05}\u{9664}"),
    ("mouse.clear_rec", "Clear Rec", "\u{6e05}\u{9664}\u{5f55}\u{5236}", "\u{6e05}\u{9664}\u{9304}\u{88fd}"),
    ("mouse.on", "On", "\u{5f00}", "\u{958b}"),
    ("mouse.off", "Off", "\u{5173}", "\u{95dc}"),
    ("mouse.rec", "Rec", "\u{5f55}\u{5236}\u{4e2d}", "\u{9304}\u{88fd}\u{4e2d}"),
    ("mouse.record", "Record", "\u{5f55}\u{5236}", "\u{9304}\u{88fd}"),
    ("mouse.live_gesture_monitor", "Live Gesture Monitor", "\u{5b9e}\u{65f6}\u{624b}\u{52bf}\u{76d1}\u{89c6}\u{5668}", "\u{5373}\u{6642}\u{624b}\u{52e2}\u{76e3}\u{8996}\u{5668}"),
    ("mouse.held", "Held", "\u{6309}\u{4f4f}", "\u{6309}\u{4f4f}"),
    ("mouse.idle", "Idle", "\u{7a7a}\u{95f2}", "\u{9592}\u{7f6e}"),
    ("mouse.move_seen", "Move Seen", "\u{68c0}\u{6d4b}\u{5230}\u{79fb}\u{52a8}", "\u{5075}\u{6e2c}\u{5230}\u{79fb}\u{52d5}"),
    ("mouse.no_move", "No Move", "\u{65e0}\u{79fb}\u{52a8}", "\u{7121}\u{79fb}\u{52d5}"),
    ("mouse.debug_placeholder", "Turn on debug mode, then press buttons or use the gesture button.", "\u{5f00}\u{542f}\u{8c03}\u{8bd5}\u{6a21}\u{5f0f}\u{ff0c}\u{7136}\u{540e}\u{6309}\u{4e0b}\u{6309}\u{952e}\u{6216}\u{4f7f}\u{7528}\u{624b}\u{52bf}\u{6309}\u{9215}\u{3002}", "\u{958b}\u{555f}\u{9664}\u{932f}\u{6a21}\u{5f0f}\u{ff0c}\u{7136}\u{5f8c}\u{6309}\u{4e0b}\u{6309}\u{9375}\u{6216}\u{4f7f}\u{7528}\u{624b}\u{52e2}\u{6309}\u{9215}\u{3002}"),
    ("mouse.gesture_placeholder", "Turn on Record and perform a few gesture attempts.", "\u{5f00}\u{542f}\u{5f55}\u{5236}\u{5e76}\u{8fdb}\u{884c}\u{51e0}\u{6b21}\u{624b}\u{52bf}\u{5c1d}\u{8bd5}\u{3002}", "\u{958b}\u{555f}\u{9304}\u{88fd}\u{4e26}\u{9032}\u{884c}\u{5e7e}\u{6b21}\u{624b}\u{52e2}\u{5617}\u{8a66}\u{3002}"),

    // Mouse page — add app dialog
    ("mouse.add_app_dialog.title", "Add App Profile", "\u{6dfb}\u{52a0}\u{5e94}\u{7528}\u{914d}\u{7f6e}", "\u{65b0}\u{589e}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{8a2d}\u{5b9a}\u{6a94}"),
    ("mouse.add_app_dialog.desc", "Choose an app. Mouser will switch to this profile when that app is focused.", "\u{9009}\u{62e9}\u{4e00}\u{4e2a}\u{5e94}\u{7528}\u{3002}\u{5f53}\u{8be5}\u{5e94}\u{7528}\u{5904}\u{4e8e}\u{7126}\u{70b9}\u{65f6}\u{ff0c}Mouser \u{5c06}\u{5207}\u{6362}\u{5230}\u{6b64}\u{914d}\u{7f6e}\u{3002}", "\u{9078}\u{64c7}\u{4e00}\u{500b}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{3002}\u{7576}\u{8a72}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{8655}\u{65bc}\u{7126}\u{9ede}\u{6642}\u{ff0c}Mouser \u{5c07}\u{5207}\u{63db}\u{5230}\u{6b64}\u{8a2d}\u{5b9a}\u{6a94}\u{3002}"),
    ("mouse.search_placeholder", "Search apps by name", "\u{6309}\u{540d}\u{79f0}\u{641c}\u{7d22}\u{5e94}\u{7528}", "\u{6309}\u{540d}\u{7a31}\u{641c}\u{5c0b}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}"),
    ("mouse.browse", "Browse", "\u{6d4f}\u{89c8}", "\u{700f}\u{89bd}"),
    ("mouse.search_results", "Search Results", "\u{641c}\u{7d22}\u{7ed3}\u{679c}", "\u{641c}\u{5c0b}\u{7d50}\u{679c}"),
    ("mouse.suggested_apps", "Suggested Apps", "\u{63a8}\u{8350}\u{5e94}\u{7528}", "\u{5efa}\u{8b70}\u{7684}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}"),
    ("mouse.no_matched", "No apps matched that search.", "\u{672a}\u{627e}\u{5230}\u{5339}\u{914d}\u{7684}\u{5e94}\u{7528}\u{3002}", "\u{672a}\u{627e}\u{5230}\u{7b26}\u{5408}\u{7684}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{3002}"),
    ("mouse.no_suggested", "No suggested apps available.", "\u{6682}\u{65e0}\u{63a8}\u{8350}\u{5e94}\u{7528}\u{3002}", "\u{66ab}\u{7121}\u{5efa}\u{8b70}\u{7684}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{3002}"),
    ("mouse.try_different", "Try a different name, or use Browse to choose the app directly.", "\u{8bf7}\u{5c1d}\u{8bd5}\u{5176}\u{4ed6}\u{540d}\u{79f0}\u{ff0c}\u{6216}\u{4f7f}\u{7528}\u{201c}\u{6d4f}\u{89c8}\u{201d}\u{76f4}\u{63a5}\u{9009}\u{62e9}\u{5e94}\u{7528}\u{3002}", "\u{8acb}\u{5617}\u{8a66}\u{5176}\u{4ed6}\u{540d}\u{7a31}\u{ff0c}\u{6216}\u{4f7f}\u{7528}\u{300c}\u{700f}\u{89bd}\u{300d}\u{76f4}\u{63a5}\u{9078}\u{64c7}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{3002}"),
    ("mouse.use_search", "Use the search box above, or browse to choose an app directly.", "\u{8bf7}\u{4f7f}\u{7528}\u{4e0a}\u{65b9}\u{7684}\u{641c}\u{7d22}\u{6846}\u{ff0c}\u{6216}\u{6d4f}\u{89c8}\u{4ee5}\u{76f4}\u{63a5}\u{9009}\u{62e9}\u{5e94}\u{7528}\u{3002}", "\u{8acb}\u{4f7f}\u{7528}\u{4e0a}\u{65b9}\u{7684}\u{641c}\u{5c0b}\u{6846}\u{ff0c}\u{6216}\u{700f}\u{89bd}\u{4ee5}\u{76f4}\u{63a5}\u{9078}\u{64c7}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{3002}"),
    ("mouse.create_profile", "Create Profile", "\u{521b}\u{5efa}\u{914d}\u{7f6e}", "\u{5efa}\u{7acb}\u{8a2d}\u{5b9a}\u{6a94}"),
    ("mouse.cancel", "Cancel", "\u{53d6}\u{6d88}", "\u{53d6}\u{6d88}"),

    // Mouse page — delete dialog
    ("mouse.delete_dialog.title", "Delete profile?", "\u{5220}\u{9664}\u{914d}\u{7f6e}\u{ff1f}", "\u{522a}\u{9664}\u{8a2d}\u{5b9a}\u{6a94}\u{ff1f}"),
    ("mouse.delete_dialog.confirm_prefix", "Delete the profile for ", "\u{5220}\u{9664} ", "\u{522a}\u{9664} "),
    ("mouse.delete_dialog.confirm_suffix", "?", " \u{7684}\u{914d}\u{7f6e}\u{ff1f}", " \u{7684}\u{8a2d}\u{5b9a}\u{6a94}\u{ff1f}"),
    ("mouse.delete_dialog.desc", "This removes its custom button mappings. The default profile will remain.", "\u{8fd9}\u{5c06}\u{5220}\u{9664}\u{5176}\u{81ea}\u{5b9a}\u{4e49}\u{6309}\u{952e}\u{6620}\u{5c04}\u{3002}\u{9ed8}\u{8ba4}\u{914d}\u{7f6e}\u{5c06}\u{4fdd}\u{7559}\u{3002}", "\u{9019}\u{5c07}\u{522a}\u{9664}\u{5176}\u{81ea}\u{8a02}\u{6309}\u{9375}\u{5c0d}\u{6620}\u{3002}\u{9810}\u{8a2d}\u{8a2d}\u{5b9a}\u{6a94}\u{5c07}\u{4fdd}\u{7559}\u{3002}"),

    // Scroll / Settings page
    ("scroll.title", "Point & Scroll", "\u{6307}\u{9488}\u{4e0e}\u{6eda}\u{8f6e}", "\u{6307}\u{6a19}\u{8207}\u{6372}\u{8ef8}"),
    ("scroll.subtitle", "Adjust pointer speed, appearance, and scroll behaviour", "\u{8c03}\u{6574}\u{6307}\u{9488}\u{901f}\u{5ea6}\u{3001}\u{5916}\u{89c2}\u{548c}\u{6eda}\u{52a8}\u{884c}\u{4e3a}", "\u{8abf}\u{6574}\u{6307}\u{6a19}\u{901f}\u{5ea6}\u{3001}\u{5916}\u{89c0}\u{548c}\u{6372}\u{52d5}\u{884c}\u{70ba}"),
    ("scroll.pointer_speed", "Pointer Speed (DPI)", "\u{6307}\u{9488}\u{901f}\u{5ea6} (DPI)", "\u{6307}\u{6a19}\u{901f}\u{5ea6} (DPI)"),
    ("scroll.pointer_speed_desc", "Adjust the tracking speed of the sensor. Higher = faster pointer.", "\u{8c03}\u{6574}\u{4f20}\u{611f}\u{5668}\u{7684}\u{8ddf}\u{8e2a}\u{901f}\u{5ea6}\u{3002}\u{503c}\u{8d8a}\u{9ad8}\u{ff0c}\u{6307}\u{9488}\u{79fb}\u{52a8}\u{8d8a}\u{5feb}\u{3002}", "\u{8abf}\u{6574}\u{611f}\u{6e2c}\u{5668}\u{7684}\u{8ffd}\u{8e64}\u{901f}\u{5ea6}\u{3002}\u{503c}\u{8d8a}\u{9ad8}\u{ff0c}\u{6307}\u{6a19}\u{79fb}\u{52d5}\u{8d8a}\u{5feb}\u{3002}"),
    ("scroll.pointer_speed_desc_range_prefix", "Adjust the tracking speed of the sensor. This device supports ", "\u{8c03}\u{6574}\u{4f20}\u{611f}\u{5668}\u{7684}\u{8ddf}\u{8e2a}\u{901f}\u{5ea6}\u{3002}\u{6b64}\u{8bbe}\u{5907}\u{652f}\u{6301} ", "\u{8abf}\u{6574}\u{611f}\u{6e2c}\u{5668}\u{7684}\u{8ffd}\u{8e64}\u{901f}\u{5ea6}\u{3002}\u{6b64}\u{88dd}\u{7f6e}\u{652f}\u{63f4} "),
    ("scroll.pointer_speed_desc_range_to", " to ", " \u{81f3} ", " \u{81f3} "),
    ("scroll.pointer_speed_desc_range_suffix", " DPI.", " DPI\u{3002}", " DPI\u{3002}"),
    ("scroll.presets", "Presets:", "\u{9884}\u{8bbe}\u{ff1a}", "\u{9810}\u{8a2d}\u{ff1a}"),
    ("scroll.wheel_mode", "Scroll Wheel Mode", "\u{6eda}\u{8f6e}\u{6a21}\u{5f0f}", "\u{6372}\u{8ef8}\u{6a21}\u{5f0f}"),
    ("scroll.wheel_mode_desc", "Switch between tactile ratchet scrolling and smooth free-spin.", "\u{5728}\u{89e6}\u{89c9}\u{68d8}\u{8f6e}\u{6eda}\u{52a8}\u{548c}\u{987a}\u{6ed1}\u{98de}\u{8f6e}\u{6eda}\u{52a8}\u{4e4b}\u{95f4}\u{5207}\u{6362}\u{3002}", "\u{5728}\u{89f8}\u{89ba}\u{68d8}\u{8f2a}\u{6372}\u{52d5}\u{548c}\u{9806}\u{6ed1}\u{98db}\u{8f2a}\u{6372}\u{52d5}\u{4e4b}\u{9593}\u{5207}\u{63db}\u{3002}"),
    ("scroll.ratchet", "Ratchet", "\u{68d8}\u{8f6e}", "\u{68d8}\u{8f2a}"),
    ("scroll.freespin", "Free Spin", "\u{98de}\u{8f6e}", "\u{98db}\u{8f2a}"),
    ("scroll.appearance", "Appearance", "\u{5916}\u{89c2}", "\u{5916}\u{89c0}"),
    ("scroll.appearance_desc", "Choose whether Mouser follows the system, stays light, or stays dark.", "\u{9009}\u{62e9} Mouser \u{662f}\u{8ddf}\u{968f}\u{7cfb}\u{7edf}\u{3001}\u{4fdd}\u{6301}\u{6d45}\u{8272}\u{8fd8}\u{662f}\u{4fdd}\u{6301}\u{6df1}\u{8272}\u{3002}", "\u{9078}\u{64c7} Mouser \u{662f}\u{8ddf}\u{96a8}\u{7cfb}\u{7d71}\u{3001}\u{4fdd}\u{6301}\u{6dfa}\u{8272}\u{9084}\u{662f}\u{4fdd}\u{6301}\u{6df1}\u{8272}\u{3002}"),
    ("scroll.system", "System", "\u{7cfb}\u{7edf}", "\u{7cfb}\u{7d71}"),
    ("scroll.light", "Light", "\u{6d45}\u{8272}", "\u{6dfa}\u{8272}"),
    ("scroll.dark", "Dark", "\u{6df1}\u{8272}", "\u{6df1}\u{8272}"),
    ("scroll.startup", "Startup", "\u{542f}\u{52a8}", "\u{555f}\u{52d5}"),
    ("scroll.startup_desc", "Start Mouser at login on Windows and macOS, and choose whether the settings window opens on launch or Mouser stays in the system tray.", "\u{5728} Windows \u{548c} macOS \u{4e0a}\u{968f}\u{767b}\u{5f55}\u{542f}\u{52a8} Mouser\u{ff0c}\u{5e76}\u{9009}\u{62e9}\u{542f}\u{52a8}\u{65f6}\u{662f}\u{5426}\u{6253}\u{5f00}\u{8bbe}\u{7f6e}\u{7a97}\u{53e3}\u{6216}\u{4ec5}\u{4fdd}\u{6301}\u{5728}\u{7cfb}\u{7edf}\u{6258}\u{76d8}\u{3002}", "\u{5728} Windows \u{548c} macOS \u{4e0a}\u{96a8}\u{767b}\u{5165}\u{555f}\u{52d5} Mouser\u{ff0c}\u{4e26}\u{9078}\u{64c7}\u{555f}\u{52d5}\u{6642}\u{662f}\u{5426}\u{958b}\u{555f}\u{8a2d}\u{5b9a}\u{8996}\u{7a97}\u{6216}\u{50c5}\u{4fdd}\u{6301}\u{5728}\u{7cfb}\u{7d71}\u{5217}\u{3002}"),
    ("scroll.start_at_login", "Start at login", "\u{5f00}\u{673a}\u{81ea}\u{542f}\u{52a8}", "\u{767b}\u{5165}\u{6642}\u{555f}\u{52d5}"),
    ("scroll.start_minimized", "Start minimized", "\u{542f}\u{52a8}\u{65f6}\u{6700}\u{5c0f}\u{5316}", "\u{555f}\u{52d5}\u{6642}\u{6700}\u{5c0f}\u{5316}"),
    ("scroll.scroll_speed", "Scroll Speed", "\u{6eda}\u{8f6e}\u{901f}\u{5ea6}", "\u{6eda}\u{8f2a}\u{901f}\u{5ea6}"),
    ("scroll.scroll_speed_desc", "Adjust how fast the page scrolls per wheel click. 1.0\u{00d7} is the system default.", "\u{8c03}\u{6574}\u{6bcf}\u{6b21}\u{6eda}\u{8f6e}\u{6eda}\u{52a8}\u{7684}\u{9875}\u{9762}\u{79fb}\u{52a8}\u{901f}\u{5ea6}\u{3002}1.0\u{00d7} \u{4e3a}\u{7cfb}\u{7edf}\u{9ed8}\u{8ba4}\u{3002}", "\u{8abf}\u{6574}\u{6bcf}\u{6b21}\u{6eda}\u{8f2a}\u{6eda}\u{52d5}\u{7684}\u{9801}\u{9762}\u{79fb}\u{52d5}\u{901f}\u{5ea6}\u{3002}1.0\u{00d7} \u{70ba}\u{7cfb}\u{7d71}\u{9810}\u{8a2d}\u{3002}"),
    ("scroll.scroll_speed_presets", "Presets:", "\u{9884}\u{8bbe}\u{ff1a}", "\u{9810}\u{8a2d}\u{ff1a}"),
    ("scroll.smooth_scroll", "Smooth Scrolling", "\u{5e73}\u{6ed1}\u{6eda}\u{52a8}", "\u{5e73}\u{6ed1}\u{6372}\u{52d5}"),
    ("scroll.smooth_scroll_desc", "Add inertia so the page coasts to a stop after each wheel tick, similar to Logi Options.", "\u{6eda}\u{8f6e}\u{6eda}\u{52a8}\u{540e}\u{6dfb}\u{52a0}\u{60ef}\u{6027}\u{6ed1}\u{884c}\u{6548}\u{679c}\u{ff0c}\u{7c7b}\u{4f3c} Logi Options \u{7684}\u{5e73}\u{6ed1}\u{6eda}\u{52a8}\u{4f53}\u{9a8c}\u{3002}", "\u{6eda}\u{8f2a}\u{6eda}\u{52d5}\u{5f8c}\u{6dfb}\u{52a0}\u{6163}\u{6027}\u{6ed1}\u{884c}\u{6548}\u{679c}\u{ff0c}\u{985e}\u{4f3c} Logi Options \u{7684}\u{5e73}\u{6ed1}\u{6372}\u{52d5}\u{9ad4}\u{9a57}\u{3002}"),
    ("scroll.scroll_direction", "Scroll Direction", "\u{6eda}\u{52a8}\u{65b9}\u{5411}", "\u{6372}\u{52d5}\u{65b9}\u{5411}"),
    ("scroll.scroll_direction_desc", "Invert the scroll direction (natural scrolling)", "\u{53cd}\u{8f6c}\u{6eda}\u{52a8}\u{65b9}\u{5411}\u{ff08}\u{81ea}\u{7136}\u{6eda}\u{52a8}\u{ff09}", "\u{53cd}\u{8f49}\u{6372}\u{52d5}\u{65b9}\u{5411}\u{ff08}\u{81ea}\u{7136}\u{6372}\u{52d5}\u{ff09}"),
    ("scroll.invert_vertical", "Invert vertical scroll", "\u{53cd}\u{8f6c}\u{5782}\u{76f4}\u{6eda}\u{52a8}", "\u{53cd}\u{8f49}\u{5782}\u{76f4}\u{6372}\u{52d5}"),
    ("scroll.invert_horizontal", "Invert horizontal scroll", "\u{53cd}\u{8f6c}\u{6c34}\u{5e73}\u{6eda}\u{52a8}", "\u{53cd}\u{8f49}\u{6c34}\u{5e73}\u{6372}\u{52d5}"),
    ("scroll.dpi_note", "DPI changes require HID++ communication with the device and will take effect after a short delay.", "DPI \u{66f4}\u{6539}\u{9700}\u{8981}\u{901a}\u{8fc7} HID++ \u{4e0e}\u{8bbe}\u{5907}\u{901a}\u{4fe1}\u{ff0c}\u{5c06}\u{5728}\u{77ed}\u{6682}\u{5ef6}\u{8fdf}\u{540e}\u{751f}\u{6548}\u{3002}", "DPI \u{66f4}\u{6539}\u{9700}\u{8981}\u{900f}\u{904e} HID++ \u{8207}\u{88dd}\u{7f6e}\u{901a}\u{8a0a}\u{ff0c}\u{5c07}\u{5728}\u{77ed}\u{66ab}\u{5ef6}\u{9072}\u{5f8c}\u{751f}\u{6548}\u{3002}"),
    ("scroll.language", "Language", "\u{8bed}\u{8a00}", "\u{8a9e}\u{8a00}"),
    ("scroll.language_desc", "Choose the display language for the application.", "\u{9009}\u{62e9}\u{5e94}\u{7528}\u{7a0b}\u{5e8f}\u{7684}\u{663e}\u{793a}\u{8bed}\u{8a00}\u{3002}", "\u{9078}\u{64c7}\u{61c9}\u{7528}\u{7a0b}\u{5f0f}\u{7684}\u{986f}\u{793a}\u{8a9e}\u{8a00}\u{3002}"),

    // Key-capture dialog
    ("key_capture.title", "Custom Shortcut", "\u{81ea}\u{5b9a}\u{4e49}\u{5feb}\u{6377}\u{952e}", "\u{81ea}\u{8a02}\u{5feb}\u{901f}\u{9375}"),
    ("key_capture.placeholder", "e.g. ctrl+shift+f5", "\u{4f8b}\u{5982}\u{ff1a}ctrl+shift+f5", "\u{4f8b}\u{5982}\u{ff1a}ctrl+shift+f5"),
    ("key_capture.valid_keys", "Valid keys: ctrl, shift, alt, super, a\u{2013}z, f1\u{2013}f12,\nspace, tab, enter, esc, left, right, up, down, delete, ...", "\u{6709}\u{6548}\u{6309}\u{952e}\u{ff1a}ctrl\u{3001}shift\u{3001}alt\u{3001}super\u{3001}a\u{2013}z\u{3001}f1\u{2013}f12\u{3001}\nspace\u{3001}tab\u{3001}enter\u{3001}esc\u{3001}left\u{3001}right\u{3001}up\u{3001}down\u{3001}delete\u{2026}\u{2026}", "\u{6709}\u{6548}\u{6309}\u{9375}\u{ff1a}ctrl\u{3001}shift\u{3001}alt\u{3001}super\u{3001}a\u{2013}z\u{3001}f1\u{2013}f12\u{3001}\nspace\u{3001}tab\u{3001}enter\u{3001}esc\u{3001}left\u{3001}right\u{3001}up\u{3001}down\u{3001}delete\u{2026}\u{2026}"),
    ("key_capture.cancel", "Cancel", "\u{53d6}\u{6d88}", "\u{53d6}\u{6d88}"),
    ("key_capture.confirm", "Confirm", "\u{786e}\u{8ba4}", "\u{78ba}\u{8a8d}"),

    // System tray
    ("tray.open_settings", "Open Settings", "\u{6253}\u{5f00}\u{8bbe}\u{7f6e}", "\u{958b}\u{555f}\u{8a2d}\u{5b9a}"),
    ("tray.disable_remapping", "Disable Remapping", "\u{7981}\u{7528}\u{6309}\u{952e}\u{91cd}\u{6620}\u{5c04}", "\u{505c}\u{7528}\u{6309}\u{9375}\u{91cd}\u{65b0}\u{5c0d}\u{6620}"),
    ("tray.enable_remapping", "Enable Remapping", "\u{542f}\u{7528}\u{6309}\u{952e}\u{91cd}\u{6620}\u{5c04}", "\u{555f}\u{7528}\u{6309}\u{9375}\u{91cd}\u{65b0}\u{5c0d}\u{6620}"),
    ("tray.enable_debug", "Enable Debug Mode", "\u{542f}\u{7528}\u{8c03}\u{8bd5}\u{6a21}\u{5f0f}", "\u{555f}\u{7528}\u{9664}\u{932f}\u{6a21}\u{5f0f}"),
    ("tray.disable_debug", "Disable Debug Mode", "\u{7981}\u{7528}\u{8c03}\u{8bd5}\u{6a21}\u{5f0f}", "\u{505c}\u{7528}\u{9664}\u{932f}\u{6a21}\u{5f0f}"),
    ("tray.quit", "Quit Mouser", "\u{9000}\u{51fa} Mouser", "\u{7d50}\u{675f} Mouser"),
    ("tray.tray_message", "Mouser is running in the system tray. Click the icon to open settings.", "Mouser \u{6b63}\u{5728}\u{7cfb}\u{7edf}\u{6258}\u{76d8}\u{4e2d}\u{8fd0}\u{884c}\u{3002}\u{70b9}\u{51fb}\u{56fe}\u{6807}\u{6253}\u{5f00}\u{8bbe}\u{7f6e}\u{3002}", "Mouser \u{6b63}\u{5728}\u{7cfb}\u{7d71}\u{5217}\u{4e2d}\u{57f7}\u{884c}\u{3002}\u{9ede}\u{64ca}\u{5716}\u{793a}\u{958b}\u{555f}\u{8a2d}\u{5b9a}\u{3002}"),

    // Accessibility dialog (macOS)
    ("accessibility.title", "Accessibility Permission Required", "\u{9700}\u{8981}\u{8f85}\u{52a9}\u{529f}\u{80fd}\u{6743}\u{9650}", "\u{9700}\u{8981}\u{8f14}\u{52a9}\u{4f7f}\u{7528}\u{6b0a}\u{9650}"),
    ("accessibility.text", "Mouser needs Accessibility permission to intercept mouse button events.\n\nmacOS should have opened the System Settings prompt.\nPlease grant permission, then restart Mouser.", "Mouser \u{9700}\u{8981}\u{8f85}\u{52a9}\u{529f}\u{80fd}\u{6743}\u{9650}\u{4ee5}\u{62e6}\u{622a}\u{9f20}\u{6807}\u{6309}\u{952e}\u{4e8b}\u{4ef6}\u{3002}\n\nmacOS \u{5e94}\u{5df2}\u{6253}\u{5f00}\u{7cfb}\u{7edf}\u{8bbe}\u{7f6e}\u{63d0}\u{793a}\u{3002}\n\u{8bf7}\u{6388}\u{4e88}\u{6743}\u{9650}\u{ff0c}\u{7136}\u{540e}\u{91cd}\u{65b0}\u{542f}\u{52a8} Mouser\u{3002}", "Mouser \u{9700}\u{8981}\u{8f14}\u{52a9}\u{4f7f}\u{7528}\u{6b0a}\u{9650}\u{4ee5}\u{6514}\u{622a}\u{6ed1}\u{9f20}\u{6309}\u{9375}\u{4e8b}\u{4ef6}\u{3002}\n\nmacOS \u{61c9}\u{5df2}\u{958b}\u{555f}\u{7cfb}\u{7d71}\u{8a2d}\u{5b9a}\u{63d0}\u{793a}\u{3002}\n\u{8acb}\u{6388}\u{4e88}\u{6b0a}\u{9650}\u{ff0c}\u{7136}\u{5f8c}\u{91cd}\u{65b0}\u{555f}\u{52d5} Mouser\u{3002}"),
    ("accessibility.info", "System Settings -> Privacy & Security -> Accessibility", "\u{7cfb}\u{7edf}\u{8bbe}\u{7f6e} -> \u{9690}\u{79c1}\u{4e0e}\u{5b89}\u{5168}\u{6027} -> \u{8f85}\u{52a9}\u{529f}\u{80fd}", "\u{7cfb}\u{7d71}\u{8a2d}\u{5b9a} -> \u{96b1}\u{79c1}\u{6b0a}\u{8207}\u{5b89}\u{5168}\u{6027} -> \u{8f14}\u{52a9}\u{4f7f}\u{7528}"),

    // Language names
    ("lang.en", "English", "English", "English"),
    ("lang.zh_CN", "\u{7b80}\u{4f53}\u{4e2d}\u{6587}", "\u{7b80}\u{4f53}\u{4e2d}\u{6587}", "\u{7b80}\u{4f53}\u{4e2d}\u{6587}"),
    ("lang.zh_TW", "\u{7e41}\u{9ad4}\u{4e2d}\u{6587}", "\u{7e41}\u{9ad4}\u{4e2d}\u{6587}", "\u{7e41}\u{9ad4}\u{4e2d}\u{6587}"),
];

// ---------------------------------------------------------------------------
// Button name translations (_BUTTON_TR)
// ---------------------------------------------------------------------------

/// Each entry: (english_name, zh_CN, zh_TW)
static BUTTON_TR: &[(&str, &str, &str)] = &[
    ("Middle button",           "\u{4e2d}\u{952e}",             "\u{4e2d}\u{9375}"),
    ("Gesture button",          "\u{624b}\u{52bf}\u{952e}",     "\u{624b}\u{52e2}\u{9375}"),
    ("Back button",             "\u{540e}\u{9000}\u{952e}",     "\u{5f8c}\u{9000}\u{9375}"),
    ("Forward button",          "\u{524d}\u{8fdb}\u{952e}",     "\u{524d}\u{9032}\u{9375}"),
    ("Horizontal scroll left",  "\u{6c34}\u{5e73}\u{5de6}\u{6eda}", "\u{6c34}\u{5e73}\u{5de6}\u{6372}"),
    ("Horizontal scroll right", "\u{6c34}\u{5e73}\u{53f3}\u{6eda}", "\u{6c34}\u{5e73}\u{53f3}\u{6372}"),
    ("Horizontal Scroll",       "\u{6c34}\u{5e73}\u{6eda}\u{52a8}", "\u{6c34}\u{5e73}\u{6372}\u{52d5}"),
    ("Mode shift button",       "\u{6a21}\u{5f0f}\u{5207}\u{6362}\u{952e}", "\u{6a21}\u{5f0f}\u{5207}\u{63db}\u{9375}"),
    ("Gesture swipe left",      "\u{624b}\u{52bf}\u{5de6}\u{6ed1}", "\u{624b}\u{52e2}\u{5de6}\u{6ed1}"),
    ("Gesture swipe right",     "\u{624b}\u{52bf}\u{53f3}\u{6ed1}", "\u{624b}\u{52e2}\u{53f3}\u{6ed1}"),
    ("Gesture swipe up",        "\u{624b}\u{52bf}\u{4e0a}\u{6ed1}", "\u{624b}\u{52e2}\u{4e0a}\u{6ed1}"),
    ("Gesture swipe down",      "\u{624b}\u{52bf}\u{4e0b}\u{6ed1}", "\u{624b}\u{52e2}\u{4e0b}\u{6ed1}"),
];

// ---------------------------------------------------------------------------
// Action category translations (_CATEGORY_TR)
// ---------------------------------------------------------------------------

/// Each entry: (english_category, zh_CN, zh_TW)
static CATEGORY_TR: &[(&str, &str, &str)] = &[
    ("Other",      "\u{5176}\u{4ed6}",         "\u{5176}\u{4ed6}"),
    ("Browser",    "\u{6d4f}\u{89c8}\u{5668}",   "\u{700f}\u{89bd}\u{5668}"),
    ("Editing",    "\u{7f16}\u{8f91}",           "\u{7de8}\u{8f2f}"),
    ("Media",      "\u{5a92}\u{4f53}",           "\u{5a92}\u{9ad4}"),
    ("Navigation", "\u{5bfc}\u{822a}",           "\u{5c0e}\u{822a}"),
    ("Custom",     "\u{81ea}\u{5b9a}\u{4e49}",   "\u{81ea}\u{8a02}"),
];

// ---------------------------------------------------------------------------
// Action label translations (_ACTION_TR)
// ---------------------------------------------------------------------------

/// Each entry: (english_label, zh_CN, zh_TW)
static ACTION_TR: &[(&str, &str, &str)] = &[
    // Other
    ("Do Nothing (Pass-through)", "\u{65e0}\u{64cd}\u{4f5c}\u{ff08}\u{76f4}\u{901a}\u{ff09}", "\u{7121}\u{64cd}\u{4f5c}\u{ff08}\u{76f4}\u{901a}\u{ff09}"),

    // Navigation (Windows)
    ("Alt + Tab (Switch Windows)", "Alt + Tab\u{ff08}\u{5207}\u{6362}\u{7a97}\u{53e3}\u{ff09}", "Alt + Tab\u{ff08}\u{5207}\u{63db}\u{8996}\u{7a97}\u{ff09}"),
    ("Alt + Shift + Tab (Switch Windows Reverse)", "Alt + Shift + Tab\u{ff08}\u{53cd}\u{5411}\u{5207}\u{6362}\u{ff09}", "Alt + Shift + Tab\u{ff08}\u{53cd}\u{5411}\u{5207}\u{63db}\u{ff09}"),
    ("Show Desktop (Win+D)", "\u{663e}\u{793a}\u{684c}\u{9762} (Win+D)", "\u{986f}\u{793a}\u{684c}\u{9762} (Win+D)"),
    ("Task View (Win+Tab)", "\u{4efb}\u{52a1}\u{89c6}\u{56fe} (Win+Tab)", "\u{5de5}\u{4f5c}\u{8996}\u{5716} (Win+Tab)"),
    ("Previous Desktop", "\u{4e0a}\u{4e00}\u{4e2a}\u{684c}\u{9762}", "\u{4e0a}\u{4e00}\u{500b}\u{684c}\u{9762}"),
    ("Next Desktop", "\u{4e0b}\u{4e00}\u{4e2a}\u{684c}\u{9762}", "\u{4e0b}\u{4e00}\u{500b}\u{684c}\u{9762}"),
    ("Page Up", "\u{5411}\u{4e0a}\u{7ffb}\u{9875}", "\u{5411}\u{4e0a}\u{7ffb}\u{9801}"),
    ("Page Down", "\u{5411}\u{4e0b}\u{7ffb}\u{9875}", "\u{5411}\u{4e0b}\u{7ffb}\u{9801}"),
    ("Home", "\u{884c}\u{9996} (Home)", "\u{884c}\u{9996} (Home)"),
    ("End", "\u{884c}\u{5c3e} (End)", "\u{884c}\u{5c3e} (End)"),

    // Navigation (macOS)
    ("Cmd + Tab (Switch Windows)", "Cmd + Tab\u{ff08}\u{5207}\u{6362}\u{7a97}\u{53e3}\u{ff09}", "Cmd + Tab\u{ff08}\u{5207}\u{63db}\u{8996}\u{7a97}\u{ff09}"),
    ("Cmd + Shift + Tab (Switch Windows Reverse)", "Cmd + Shift + Tab\u{ff08}\u{53cd}\u{5411}\u{5207}\u{6362}\u{ff09}", "Cmd + Shift + Tab\u{ff08}\u{53cd}\u{5411}\u{5207}\u{63db}\u{ff09}"),
    ("Mission Control (Ctrl+Up)", "\u{4efb}\u{52a1}\u{63a7}\u{5236} (Ctrl+\u{2191})", "\u{4efb}\u{52d9}\u{63a7}\u{5236} (Ctrl+\u{2191})"),
    ("Mission Control", "\u{4efb}\u{52a1}\u{63a7}\u{5236}", "\u{4efb}\u{52d9}\u{63a7}\u{5236}"),
    ("App Expose", "\u{5e94}\u{7528} Expos\u{00e9}", "\u{61c9}\u{7528}\u{7a0b}\u{5f0f} Expos\u{00e9}"),
    ("Show Desktop", "\u{663e}\u{793a}\u{684c}\u{9762}", "\u{986f}\u{793a}\u{684c}\u{9762}"),
    ("Launchpad", "\u{542f}\u{52a8}\u{53f0}", "\u{555f}\u{52d5}\u{53f0}"),

    // Navigation (Linux)
    ("Show Desktop (Super+D)", "\u{663e}\u{793a}\u{684c}\u{9762} (Super+D)", "\u{986f}\u{793a}\u{684c}\u{9762} (Super+D)"),
    ("Activities (Super)", "\u{6d3b}\u{52a8}\u{8868} (Super)", "\u{6d3b}\u{52d5}\u{8996}\u{5716} (Super)"),

    // Browser
    ("Browser Back", "\u{6d4f}\u{89c8}\u{5668}\u{540e}\u{9000}", "\u{700f}\u{89bd}\u{5668}\u{5f8c}\u{9000}"),
    ("Browser Forward", "\u{6d4f}\u{89c8}\u{5668}\u{524d}\u{8fdb}", "\u{700f}\u{89bd}\u{5668}\u{524d}\u{9032}"),
    ("Browser Back (Cmd+[)", "\u{6d4f}\u{89c8}\u{5668}\u{540e}\u{9000} (Cmd+[)", "\u{700f}\u{89bd}\u{5668}\u{5f8c}\u{9000} (Cmd+[)"),
    ("Browser Forward (Cmd+])", "\u{6d4f}\u{89c8}\u{5668}\u{524d}\u{8fdb} (Cmd+])", "\u{700f}\u{89bd}\u{5668}\u{524d}\u{9032} (Cmd+])"),
    ("Close Tab (Ctrl+W)", "\u{5173}\u{95ed}\u{6807}\u{7b7e}\u{9875} (Ctrl+W)", "\u{95dc}\u{9589}\u{6a19}\u{7c64}\u{9801} (Ctrl+W)"),
    ("Close Tab (Cmd+W)", "\u{5173}\u{95ed}\u{6807}\u{7b7e}\u{9875} (Cmd+W)", "\u{95dc}\u{9589}\u{6a19}\u{7c64}\u{9801} (Cmd+W)"),
    ("New Tab (Ctrl+T)", "\u{65b0}\u{5efa}\u{6807}\u{7b7e}\u{9875} (Ctrl+T)", "\u{65b0}\u{5efa}\u{6a19}\u{7c64}\u{9801} (Ctrl+T)"),
    ("New Tab (Cmd+T)", "\u{65b0}\u{5efa}\u{6807}\u{7b7e}\u{9875} (Cmd+T)", "\u{65b0}\u{5efa}\u{6a19}\u{7c64}\u{9801} (Cmd+T)"),
    ("Next Tab (Ctrl+Tab)", "\u{4e0b}\u{4e00}\u{4e2a}\u{6807}\u{7b7e}\u{9875} (Ctrl+Tab)", "\u{4e0b}\u{4e00}\u{500b}\u{6a19}\u{7c64}\u{9801} (Ctrl+Tab)"),
    ("Next Tab (Cmd+Shift+])", "\u{4e0b}\u{4e00}\u{4e2a}\u{6807}\u{7b7e}\u{9875} (Cmd+Shift+])", "\u{4e0b}\u{4e00}\u{500b}\u{6a19}\u{7c64}\u{9801} (Cmd+Shift+])"),
    ("Previous Tab (Ctrl+Shift+Tab)", "\u{4e0a}\u{4e00}\u{4e2a}\u{6807}\u{7b7e}\u{9875} (Ctrl+Shift+Tab)", "\u{4e0a}\u{4e00}\u{500b}\u{6a19}\u{7c64}\u{9801} (Ctrl+Shift+Tab)"),
    ("Previous Tab (Cmd+Shift+[)", "\u{4e0a}\u{4e00}\u{4e2a}\u{6807}\u{7b7e}\u{9875} (Cmd+Shift+[)", "\u{4e0a}\u{4e00}\u{500b}\u{6a19}\u{7c64}\u{9801} (Cmd+Shift+[)"),

    // Editing
    ("Copy (Ctrl+C)", "\u{590d}\u{5236} (Ctrl+C)", "\u{8907}\u{88fd} (Ctrl+C)"),
    ("Copy (Cmd+C)", "\u{590d}\u{5236} (Cmd+C)", "\u{8907}\u{88fd} (Cmd+C)"),
    ("Paste (Ctrl+V)", "\u{7c98}\u{8d34} (Ctrl+V)", "\u{8cbc}\u{4e0a} (Ctrl+V)"),
    ("Paste (Cmd+V)", "\u{7c98}\u{8d34} (Cmd+V)", "\u{8cbc}\u{4e0a} (Cmd+V)"),
    ("Cut (Ctrl+X)", "\u{526a}\u{5207} (Ctrl+X)", "\u{526a}\u{5207} (Ctrl+X)"),
    ("Cut (Cmd+X)", "\u{526a}\u{5207} (Cmd+X)", "\u{526a}\u{5207} (Cmd+X)"),
    ("Undo (Ctrl+Z)", "\u{64a4}\u{9500} (Ctrl+Z)", "\u{5fa9}\u{539f} (Ctrl+Z)"),
    ("Undo (Cmd+Z)", "\u{64a4}\u{9500} (Cmd+Z)", "\u{5fa9}\u{539f} (Cmd+Z)"),
    ("Select All (Ctrl+A)", "\u{5168}\u{9009} (Ctrl+A)", "\u{5168}\u{9078} (Ctrl+A)"),
    ("Select All (Cmd+A)", "\u{5168}\u{9009} (Cmd+A)", "\u{5168}\u{9078} (Cmd+A)"),
    ("Save (Ctrl+S)", "\u{4fdd}\u{5b58} (Ctrl+S)", "\u{5132}\u{5b58} (Ctrl+S)"),
    ("Save (Cmd+S)", "\u{4fdd}\u{5b58} (Cmd+S)", "\u{5132}\u{5b58} (Cmd+S)"),
    ("Find (Ctrl+F)", "\u{67e5}\u{627e} (Ctrl+F)", "\u{5c0b}\u{627e} (Ctrl+F)"),
    ("Find (Cmd+F)", "\u{67e5}\u{627e} (Cmd+F)", "\u{5c0b}\u{627e} (Cmd+F)"),

    // Media
    ("Volume Up", "\u{97f3}\u{91cf}\u{589e}\u{5927}", "\u{97f3}\u{91cf}\u{589e}\u{5927}"),
    ("Volume Down", "\u{97f3}\u{91cf}\u{51cf}\u{5c0f}", "\u{97f3}\u{91cf}\u{6e1b}\u{5c0f}"),
    ("Volume Mute", "\u{9759}\u{97f3}", "\u{975c}\u{97f3}"),
    ("Play / Pause", "\u{64ad}\u{653e}/\u{6682}\u{505c}", "\u{64ad}\u{653e}/\u{66ab}\u{505c}"),
    ("Next Track", "\u{4e0b}\u{4e00}\u{9996}", "\u{4e0b}\u{4e00}\u{9996}"),
    ("Previous Track", "\u{4e0a}\u{4e00}\u{9996}", "\u{4e0a}\u{4e00}\u{9996}"),

    // Custom
    ("Custom Shortcut\u{2026}", "\u{81ea}\u{5b9a}\u{4e49}\u{5feb}\u{6377}\u{952e}\u{2026}", "\u{81ea}\u{8a02}\u{5feb}\u{901f}\u{9375}\u{2026}"),
];

// ---------------------------------------------------------------------------
// Lookup helpers (linear scan over static slices — tables are small)
// ---------------------------------------------------------------------------

fn lookup_translation(key: &str, lang: Language) -> Option<&'static str> {
    for &(k, en, zh_cn, zh_tw) in TRANSLATIONS {
        if k == key {
            return Some(match lang {
                Language::En => en,
                Language::ZhCN => zh_cn,
                Language::ZhTW => zh_tw,
            });
        }
    }
    None
}

fn lookup_triple<'a>(table: &'a [(&'a str, &'a str, &'a str)], english: &str, lang: Language) -> Option<&'a str> {
    for &(en, zh_cn, zh_tw) in table {
        if en == english {
            return Some(match lang {
                Language::En => en,
                Language::ZhCN => zh_cn,
                Language::ZhTW => zh_tw,
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// LocaleManager
// ---------------------------------------------------------------------------

pub struct LocaleManager {
    language: Mutex<Language>,
}

impl LocaleManager {
    /// Create a new `LocaleManager` with the given language code.
    /// Falls back to English if the code is not recognized.
    pub fn new(language_code: &str) -> Self {
        let lang = Language::from_code(language_code).unwrap_or(Language::En);
        Self {
            language: Mutex::new(lang),
        }
    }

    /// Return the current language code (e.g. `"en"`, `"zh_CN"`, `"zh_TW"`).
    pub fn language(&self) -> &'static str {
        self.language.lock().unwrap().code()
    }

    /// Set the active language. Ignored if the code is not recognized.
    pub fn set_language(&self, lang_code: &str) {
        if let Some(lang) = Language::from_code(lang_code) {
            *self.language.lock().unwrap() = lang;
        }
    }

    /// Translate a UI string key (e.g. `"mouse.connected"`).
    /// Returns the key itself if no translation is found.
    pub fn tr<'a>(&self, key: &'a str) -> &'a str
    where
        'static: 'a,
    {
        let lang = *self.language.lock().unwrap();
        lookup_translation(key, lang).unwrap_or(key)
    }

    /// Translate a button name (e.g. `"Middle button"`).
    /// Returns the English name unchanged for `Language::En` or if not found.
    pub fn tr_button<'a>(&self, english_name: &'a str) -> &'a str
    where
        'static: 'a,
    {
        let lang = *self.language.lock().unwrap();
        if lang == Language::En {
            return english_name;
        }
        lookup_triple(BUTTON_TR, english_name, lang).unwrap_or(english_name)
    }

    /// Translate an action label (e.g. `"Copy (Cmd+C)"`).
    /// Returns the English label unchanged for `Language::En` or if not found.
    pub fn tr_action<'a>(&self, english_label: &'a str) -> &'a str
    where
        'static: 'a,
    {
        let lang = *self.language.lock().unwrap();
        if lang == Language::En {
            return english_label;
        }
        lookup_triple(ACTION_TR, english_label, lang).unwrap_or(english_label)
    }

    /// Translate an action category (e.g. `"Browser"`).
    /// Returns the English category unchanged for `Language::En` or if not found.
    pub fn tr_category<'a>(&self, english_cat: &'a str) -> &'a str
    where
        'static: 'a,
    {
        let lang = *self.language.lock().unwrap();
        if lang == Language::En {
            return english_cat;
        }
        lookup_triple(CATEGORY_TR, english_cat, lang).unwrap_or(english_cat)
    }

    /// Return the list of available languages as `(code, native_name)` pairs.
    pub fn available_languages() -> &'static [(&'static str, &'static str)] {
        AVAILABLE_LANGUAGES
    }
}

impl Default for LocaleManager {
    fn default() -> Self {
        Self::new("en")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_language_is_english() {
        let lm = LocaleManager::default();
        assert_eq!(lm.language(), "en");
    }

    #[test]
    fn test_set_language() {
        let lm = LocaleManager::new("en");
        lm.set_language("zh_CN");
        assert_eq!(lm.language(), "zh_CN");
        lm.set_language("zh_TW");
        assert_eq!(lm.language(), "zh_TW");
        lm.set_language("en");
        assert_eq!(lm.language(), "en");
    }

    #[test]
    fn test_set_language_invalid_ignored() {
        let lm = LocaleManager::new("zh_CN");
        lm.set_language("fr");
        assert_eq!(lm.language(), "zh_CN");
    }

    #[test]
    fn test_new_with_invalid_falls_back_to_en() {
        let lm = LocaleManager::new("invalid");
        assert_eq!(lm.language(), "en");
    }

    #[test]
    fn test_tr_english() {
        let lm = LocaleManager::new("en");
        assert_eq!(lm.tr("mouse.connected"), "Connected");
        assert_eq!(lm.tr("mouse.not_connected"), "Not Connected");
        assert_eq!(lm.tr("scroll.title"), "Point & Scroll");
    }

    #[test]
    fn test_tr_zh_cn() {
        let lm = LocaleManager::new("zh_CN");
        assert_eq!(lm.tr("mouse.connected"), "\u{5df2}\u{8fde}\u{63a5}");
        assert_eq!(lm.tr("scroll.title"), "\u{6307}\u{9488}\u{4e0e}\u{6eda}\u{8f6e}");
    }

    #[test]
    fn test_tr_zh_tw() {
        let lm = LocaleManager::new("zh_TW");
        assert_eq!(lm.tr("mouse.connected"), "\u{5df2}\u{9023}\u{7dda}");
        assert_eq!(lm.tr("scroll.title"), "\u{6307}\u{6a19}\u{8207}\u{6372}\u{8ef8}");
    }

    #[test]
    fn test_tr_missing_key_returns_key() {
        let lm = LocaleManager::new("en");
        assert_eq!(lm.tr("nonexistent.key"), "nonexistent.key");
    }

    #[test]
    fn test_tr_button_english_passthrough() {
        let lm = LocaleManager::new("en");
        assert_eq!(lm.tr_button("Middle button"), "Middle button");
        assert_eq!(lm.tr_button("Gesture button"), "Gesture button");
    }

    #[test]
    fn test_tr_button_zh_cn() {
        let lm = LocaleManager::new("zh_CN");
        assert_eq!(lm.tr_button("Middle button"), "\u{4e2d}\u{952e}");
        assert_eq!(lm.tr_button("Gesture button"), "\u{624b}\u{52bf}\u{952e}");
        assert_eq!(lm.tr_button("Back button"), "\u{540e}\u{9000}\u{952e}");
    }

    #[test]
    fn test_tr_button_zh_tw() {
        let lm = LocaleManager::new("zh_TW");
        assert_eq!(lm.tr_button("Middle button"), "\u{4e2d}\u{9375}");
        assert_eq!(lm.tr_button("Forward button"), "\u{524d}\u{9032}\u{9375}");
    }

    #[test]
    fn test_tr_button_unknown_returns_english() {
        let lm = LocaleManager::new("zh_CN");
        assert_eq!(lm.tr_button("Unknown button"), "Unknown button");
    }

    #[test]
    fn test_tr_action_english_passthrough() {
        let lm = LocaleManager::new("en");
        assert_eq!(lm.tr_action("Copy (Ctrl+C)"), "Copy (Ctrl+C)");
    }

    #[test]
    fn test_tr_action_zh_cn() {
        let lm = LocaleManager::new("zh_CN");
        assert_eq!(lm.tr_action("Copy (Ctrl+C)"), "\u{590d}\u{5236} (Ctrl+C)");
        assert_eq!(lm.tr_action("Volume Up"), "\u{97f3}\u{91cf}\u{589e}\u{5927}");
        assert_eq!(lm.tr_action("Browser Back"), "\u{6d4f}\u{89c8}\u{5668}\u{540e}\u{9000}");
    }

    #[test]
    fn test_tr_action_zh_tw() {
        let lm = LocaleManager::new("zh_TW");
        assert_eq!(lm.tr_action("Copy (Ctrl+C)"), "\u{8907}\u{88fd} (Ctrl+C)");
        assert_eq!(lm.tr_action("Paste (Cmd+V)"), "\u{8cbc}\u{4e0a} (Cmd+V)");
    }

    #[test]
    fn test_tr_action_unknown_returns_english() {
        let lm = LocaleManager::new("zh_CN");
        assert_eq!(lm.tr_action("Some Unknown Action"), "Some Unknown Action");
    }

    #[test]
    fn test_tr_category_english_passthrough() {
        let lm = LocaleManager::new("en");
        assert_eq!(lm.tr_category("Browser"), "Browser");
        assert_eq!(lm.tr_category("Media"), "Media");
    }

    #[test]
    fn test_tr_category_zh_cn() {
        let lm = LocaleManager::new("zh_CN");
        assert_eq!(lm.tr_category("Browser"), "\u{6d4f}\u{89c8}\u{5668}");
        assert_eq!(lm.tr_category("Media"), "\u{5a92}\u{4f53}");
        assert_eq!(lm.tr_category("Custom"), "\u{81ea}\u{5b9a}\u{4e49}");
    }

    #[test]
    fn test_tr_category_zh_tw() {
        let lm = LocaleManager::new("zh_TW");
        assert_eq!(lm.tr_category("Browser"), "\u{700f}\u{89bd}\u{5668}");
        assert_eq!(lm.tr_category("Editing"), "\u{7de8}\u{8f2f}");
        assert_eq!(lm.tr_category("Navigation"), "\u{5c0e}\u{822a}");
    }

    #[test]
    fn test_tr_category_unknown_returns_english() {
        let lm = LocaleManager::new("zh_TW");
        assert_eq!(lm.tr_category("Nonexistent"), "Nonexistent");
    }

    #[test]
    fn test_available_languages() {
        let langs = LocaleManager::available_languages();
        assert_eq!(langs.len(), 3);
        assert_eq!(langs[0].0, "en");
        assert_eq!(langs[1].0, "zh_CN");
        assert_eq!(langs[2].0, "zh_TW");
    }

    #[test]
    fn test_language_enum_from_code() {
        assert_eq!(Language::from_code("en"), Some(Language::En));
        assert_eq!(Language::from_code("zh_CN"), Some(Language::ZhCN));
        assert_eq!(Language::from_code("zh_TW"), Some(Language::ZhTW));
        assert_eq!(Language::from_code("fr"), None);
    }

    #[test]
    fn test_language_enum_code_roundtrip() {
        for lang in &[Language::En, Language::ZhCN, Language::ZhTW] {
            assert_eq!(Language::from_code(lang.code()), Some(*lang));
        }
    }

    #[test]
    fn test_all_translation_keys_present_in_all_languages() {
        // Verify that every entry has non-empty strings for all three languages
        for &(key, en, zh_cn, zh_tw) in TRANSLATIONS {
            assert!(!key.is_empty(), "empty key found in TRANSLATIONS");
            assert!(!en.is_empty(), "empty en value for key: {key}");
            assert!(!zh_cn.is_empty(), "empty zh_CN value for key: {key}");
            assert!(!zh_tw.is_empty(), "empty zh_TW value for key: {key}");
        }
    }

    #[test]
    fn test_all_button_translations_present() {
        for &(en, zh_cn, zh_tw) in BUTTON_TR {
            assert!(!en.is_empty());
            assert!(!zh_cn.is_empty());
            assert!(!zh_tw.is_empty());
        }
    }

    #[test]
    fn test_all_action_translations_present() {
        for &(en, zh_cn, zh_tw) in ACTION_TR {
            assert!(!en.is_empty());
            assert!(!zh_cn.is_empty());
            assert!(!zh_tw.is_empty());
        }
    }

    #[test]
    fn test_all_category_translations_present() {
        for &(en, zh_cn, zh_tw) in CATEGORY_TR {
            assert!(!en.is_empty());
            assert!(!zh_cn.is_empty());
            assert!(!zh_tw.is_empty());
        }
    }

    #[test]
    fn test_tray_translations() {
        let lm = LocaleManager::new("zh_CN");
        assert_eq!(lm.tr("tray.quit"), "\u{9000}\u{51fa} Mouser");
        let lm = LocaleManager::new("zh_TW");
        assert_eq!(lm.tr("tray.quit"), "\u{7d50}\u{675f} Mouser");
    }

    #[test]
    fn test_accessibility_translations() {
        let lm = LocaleManager::new("zh_CN");
        assert_eq!(lm.tr("accessibility.title"), "\u{9700}\u{8981}\u{8f85}\u{52a9}\u{529f}\u{80fd}\u{6743}\u{9650}");
        let lm = LocaleManager::new("zh_TW");
        assert_eq!(lm.tr("accessibility.title"), "\u{9700}\u{8981}\u{8f14}\u{52a9}\u{4f7f}\u{7528}\u{6b0a}\u{9650}");
    }

    #[test]
    fn test_custom_shortcut_action() {
        let lm = LocaleManager::new("zh_CN");
        assert_eq!(lm.tr_action("Custom Shortcut\u{2026}"), "\u{81ea}\u{5b9a}\u{4e49}\u{5feb}\u{6377}\u{952e}\u{2026}");
        let lm = LocaleManager::new("zh_TW");
        assert_eq!(lm.tr_action("Custom Shortcut\u{2026}"), "\u{81ea}\u{8a02}\u{5feb}\u{901f}\u{9375}\u{2026}");
    }

    #[test]
    fn test_language_switch_updates_translations() {
        let lm = LocaleManager::new("en");
        assert_eq!(lm.tr("mouse.connected"), "Connected");
        lm.set_language("zh_CN");
        assert_eq!(lm.tr("mouse.connected"), "\u{5df2}\u{8fde}\u{63a5}");
        lm.set_language("zh_TW");
        assert_eq!(lm.tr("mouse.connected"), "\u{5df2}\u{9023}\u{7dda}");
    }
}
