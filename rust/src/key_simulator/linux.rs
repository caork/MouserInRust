//! Linux key simulation backend.
//!
//! Uses the `evdev` crate to create a `UInput` virtual keyboard / relative
//! pointer device.  The device is created lazily on first use and kept alive
//! for the lifetime of the process.
//!
//! Workspace switching detects GNOME vs KDE/Plasma (or other) via
//! `$XDG_CURRENT_DESKTOP` and chooses the appropriate key combo.

#![allow(dead_code)]

use std::sync::{Mutex, OnceLock};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use evdev::{
    uinput::{VirtualDevice, VirtualDeviceBuilder},
    AttributeSet, InputEvent, Key, RelativeAxisType,
};

// ---------------------------------------------------------------------------
// Linux input event key codes
// (numeric values from linux/input-event-codes.h — stable ABI)
// ---------------------------------------------------------------------------

// Modifier keys
pub const KEY_LEFTCTRL:  Key = Key::KEY_LEFTCTRL;
pub const KEY_LEFTSHIFT: Key = Key::KEY_LEFTSHIFT;
pub const KEY_LEFTALT:   Key = Key::KEY_LEFTALT;
pub const KEY_LEFTMETA:  Key = Key::KEY_LEFTMETA;

// Common keys
pub const KEY_TAB:       Key = Key::KEY_TAB;
pub const KEY_SPACE:     Key = Key::KEY_SPACE;
pub const KEY_ENTER:     Key = Key::KEY_ENTER;
pub const KEY_BACKSPACE: Key = Key::KEY_BACKSPACE;
pub const KEY_DELETE:    Key = Key::KEY_DELETE;
pub const KEY_ESC:       Key = Key::KEY_ESC;

// Arrow / navigation keys
pub const KEY_LEFT:      Key = Key::KEY_LEFT;
pub const KEY_UP:        Key = Key::KEY_UP;
pub const KEY_RIGHT:     Key = Key::KEY_RIGHT;
pub const KEY_DOWN:      Key = Key::KEY_DOWN;
pub const KEY_PAGEUP:    Key = Key::KEY_PAGEUP;
pub const KEY_PAGEDOWN:  Key = Key::KEY_PAGEDOWN;
pub const KEY_HOME:      Key = Key::KEY_HOME;
pub const KEY_END:       Key = Key::KEY_END;

// Browser navigation
pub const KEY_BACK:      Key = Key::KEY_BACK;
pub const KEY_FORWARD:   Key = Key::KEY_FORWARD;

// Media keys
pub const KEY_VOLUMEUP:      Key = Key::KEY_VOLUMEUP;
pub const KEY_VOLUMEDOWN:    Key = Key::KEY_VOLUMEDOWN;
pub const KEY_MUTE:          Key = Key::KEY_MUTE;
pub const KEY_PLAYPAUSE:     Key = Key::KEY_PLAYPAUSE;
pub const KEY_NEXTSONG:      Key = Key::KEY_NEXTSONG;
pub const KEY_PREVIOUSSONG:  Key = Key::KEY_PREVIOUSSONG;

// Letter keys (A–Z)
pub const KEY_A: Key = Key::KEY_A;
pub const KEY_B: Key = Key::KEY_B;
pub const KEY_C: Key = Key::KEY_C;
pub const KEY_D: Key = Key::KEY_D;
pub const KEY_E: Key = Key::KEY_E;
pub const KEY_F: Key = Key::KEY_F;
pub const KEY_G: Key = Key::KEY_G;
pub const KEY_H: Key = Key::KEY_H;
pub const KEY_I: Key = Key::KEY_I;
pub const KEY_J: Key = Key::KEY_J;
pub const KEY_K: Key = Key::KEY_K;
pub const KEY_L: Key = Key::KEY_L;
pub const KEY_M: Key = Key::KEY_M;
pub const KEY_N: Key = Key::KEY_N;
pub const KEY_O: Key = Key::KEY_O;
pub const KEY_P: Key = Key::KEY_P;
pub const KEY_Q: Key = Key::KEY_Q;
pub const KEY_R: Key = Key::KEY_R;
pub const KEY_S: Key = Key::KEY_S;
pub const KEY_T: Key = Key::KEY_T;
pub const KEY_U: Key = Key::KEY_U;
pub const KEY_V: Key = Key::KEY_V;
pub const KEY_W: Key = Key::KEY_W;
pub const KEY_X: Key = Key::KEY_X;
pub const KEY_Y: Key = Key::KEY_Y;
pub const KEY_Z: Key = Key::KEY_Z;

// Function keys
pub const KEY_F1:  Key = Key::KEY_F1;
pub const KEY_F2:  Key = Key::KEY_F2;
pub const KEY_F3:  Key = Key::KEY_F3;
pub const KEY_F4:  Key = Key::KEY_F4;
pub const KEY_F5:  Key = Key::KEY_F5;
pub const KEY_F6:  Key = Key::KEY_F6;
pub const KEY_F7:  Key = Key::KEY_F7;
pub const KEY_F8:  Key = Key::KEY_F8;
pub const KEY_F9:  Key = Key::KEY_F9;
pub const KEY_F10: Key = Key::KEY_F10;
pub const KEY_F11: Key = Key::KEY_F11;
pub const KEY_F12: Key = Key::KEY_F12;

// ---------------------------------------------------------------------------
// Key name → evdev Key map  (matches parse_custom_combo from actions.rs)
// ---------------------------------------------------------------------------

/// Resolve a lowercase key name into an evdev `Key`.
pub fn key_name_to_key(name: &str) -> Option<Key> {
    Some(match name {
        "ctrl"       => KEY_LEFTCTRL,
        "shift"      => KEY_LEFTSHIFT,
        "alt"        => KEY_LEFTALT,
        "super"      => KEY_LEFTMETA,
        "tab"        => KEY_TAB,
        "space"      => KEY_SPACE,
        "enter"      => KEY_ENTER,
        "esc"        => KEY_ESC,
        "backspace"  => KEY_BACKSPACE,
        "delete"     => KEY_DELETE,
        "left"       => KEY_LEFT,
        "right"      => KEY_RIGHT,
        "up"         => KEY_UP,
        "down"       => KEY_DOWN,
        "pageup"     => KEY_PAGEUP,
        "pagedown"   => KEY_PAGEDOWN,
        "home"       => KEY_HOME,
        "end"        => KEY_END,
        "a" => KEY_A, "b" => KEY_B, "c" => KEY_C, "d" => KEY_D,
        "e" => KEY_E, "f" => KEY_F, "g" => KEY_G, "h" => KEY_H,
        "i" => KEY_I, "j" => KEY_J, "k" => KEY_K, "l" => KEY_L,
        "m" => KEY_M, "n" => KEY_N, "o" => KEY_O, "p" => KEY_P,
        "q" => KEY_Q, "r" => KEY_R, "s" => KEY_S, "t" => KEY_T,
        "u" => KEY_U, "v" => KEY_V, "w" => KEY_W, "x" => KEY_X,
        "y" => KEY_Y, "z" => KEY_Z,
        "f1"  => KEY_F1,  "f2"  => KEY_F2,  "f3"  => KEY_F3,
        "f4"  => KEY_F4,  "f5"  => KEY_F5,  "f6"  => KEY_F6,
        "f7"  => KEY_F7,  "f8"  => KEY_F8,  "f9"  => KEY_F9,
        "f10" => KEY_F10, "f11" => KEY_F11, "f12" => KEY_F12,
        "volumeup"   => KEY_VOLUMEUP,
        "volumedown" => KEY_VOLUMEDOWN,
        "mute"       => KEY_MUTE,
        "playpause"  => KEY_PLAYPAUSE,
        "nexttrack"  => KEY_NEXTSONG,
        "prevtrack"  => KEY_PREVIOUSSONG,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Virtual UInput device (lazy singleton)
// ---------------------------------------------------------------------------

/// All key codes that might be injected.  UInput requires the full set at
/// device-creation time.
static ALL_KEYS: &[Key] = &[
    KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_LEFTALT, KEY_LEFTMETA,
    KEY_TAB, KEY_SPACE, KEY_ENTER, KEY_BACKSPACE, KEY_DELETE, KEY_ESC,
    KEY_LEFT, KEY_UP, KEY_RIGHT, KEY_DOWN,
    KEY_PAGEUP, KEY_PAGEDOWN, KEY_HOME, KEY_END,
    KEY_BACK, KEY_FORWARD,
    KEY_VOLUMEUP, KEY_VOLUMEDOWN, KEY_MUTE,
    KEY_PLAYPAUSE, KEY_NEXTSONG, KEY_PREVIOUSSONG,
    KEY_A, KEY_B, KEY_C, KEY_D, KEY_E, KEY_F, KEY_G, KEY_H,
    KEY_I, KEY_J, KEY_K, KEY_L, KEY_M, KEY_N, KEY_O, KEY_P,
    KEY_Q, KEY_R, KEY_S, KEY_T, KEY_U, KEY_V, KEY_W, KEY_X,
    KEY_Y, KEY_Z,
    KEY_F1,  KEY_F2,  KEY_F3,  KEY_F4,  KEY_F5,  KEY_F6,
    KEY_F7,  KEY_F8,  KEY_F9,  KEY_F10, KEY_F11, KEY_F12,
];

static VIRTUAL_KBD: OnceLock<Mutex<Option<VirtualDevice>>> = OnceLock::new();

fn get_virtual_kbd() -> &'static Mutex<Option<VirtualDevice>> {
    VIRTUAL_KBD.get_or_init(|| {
        let device = build_virtual_device();
        Mutex::new(device)
    })
}

fn build_virtual_device() -> Option<VirtualDevice> {
    let mut keys = AttributeSet::<Key>::new();
    for &k in ALL_KEYS {
        keys.insert(k);
    }

    let mut rel_axes = AttributeSet::<RelativeAxisType>::new();
    rel_axes.insert(RelativeAxisType::REL_WHEEL);
    rel_axes.insert(RelativeAxisType::REL_HWHEEL);

    match VirtualDeviceBuilder::new()
        .context("VirtualDeviceBuilder::new")
        .and_then(|b| {
            b.name("Mouser Virtual Keyboard")
                .with_keys(&keys)
                .context("with_keys")?
                .with_relative_axes(&rel_axes)
                .context("with_relative_axes")
        })
        .and_then(|b| b.build().context("build"))
    {
        Ok(dev) => Some(dev),
        Err(e) => {
            log::error!(
                "[key_simulator] Failed to create UInput virtual device: {}. \
                 Make sure the user is in the 'input' group and /dev/uinput is accessible.",
                e
            );
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Workspace switching: detect GNOME vs KDE
// ---------------------------------------------------------------------------

fn linux_desktop() -> &'static str {
    use std::sync::OnceLock as OL;
    static DESKTOP: OL<String> = OL::new();
    DESKTOP.get_or_init(|| {
        std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_uppercase()
    })
}

fn workspace_keys(direction: &str) -> &'static [Key] {
    let desktop = linux_desktop();
    if desktop.contains("GNOME") {
        if direction == "left" {
            &[KEY_LEFTMETA, KEY_PAGEUP]
        } else {
            &[KEY_LEFTMETA, KEY_PAGEDOWN]
        }
    } else {
        // KDE / Plasma default, and pragmatic fallback for other desktops
        if direction == "left" {
            &[KEY_LEFTCTRL, KEY_LEFTMETA, KEY_LEFT]
        } else {
            &[KEY_LEFTCTRL, KEY_LEFTMETA, KEY_RIGHT]
        }
    }
}

// ---------------------------------------------------------------------------
// Key combo sender
// ---------------------------------------------------------------------------

/// Press and release a chord of evdev `Key` values.
///
/// Keys are pressed in order, held for `hold_ms` ms, released in reverse.
pub fn send_key_combo(keys: &[Key], hold_ms: u64) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }

    let lock = get_virtual_kbd();
    let mut guard = lock.lock().expect("virtual kbd mutex poisoned");
    let dev = guard.as_mut().ok_or_else(|| {
        anyhow::anyhow!(
            "UInput virtual device unavailable. \
             Add user to 'input' group and ensure /dev/uinput exists."
        )
    })?;

    // Press phase
    for &k in keys {
        dev.emit(&[InputEvent::new(
            evdev::EventType::KEY,
            k.code(),
            1, // value 1 = key-down
        )])
        .context("emit key-down")?;
    }

    if hold_ms > 0 {
        sleep(Duration::from_millis(hold_ms));
    }

    // Release phase (reverse order)
    for &k in keys.iter().rev() {
        dev.emit(&[InputEvent::new(
            evdev::EventType::KEY,
            k.code(),
            0, // value 0 = key-up
        )])
        .context("emit key-up")?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Scroll injection
// ---------------------------------------------------------------------------

/// Inject a relative scroll event via UInput.
///
/// `delta` is converted to detents (120 units per detent, matching Windows
/// WHEEL_DELTA convention so callers can use a uniform unit).
pub fn inject_scroll(horizontal: bool, delta: i32) -> Result<()> {
    let lock = get_virtual_kbd();
    let mut guard = lock.lock().expect("virtual kbd mutex poisoned");
    let dev = guard.as_mut().ok_or_else(|| {
        anyhow::anyhow!("UInput virtual device unavailable")
    })?;

    let detents = if delta.abs() >= 120 {
        delta / 120
    } else if delta > 0 {
        1
    } else {
        -1
    };

    let axis = if horizontal {
        RelativeAxisType::REL_HWHEEL
    } else {
        RelativeAxisType::REL_WHEEL
    };

    dev.emit(&[InputEvent::new(
        evdev::EventType::RELATIVE,
        axis.0,
        detents,
    )])
    .context("emit scroll")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Action dispatch
// ---------------------------------------------------------------------------

/// Execute a built-in action id on Linux.
pub fn execute_action(action_id: &str, hold_ms: u64) -> Result<()> {
    let keys: &[Key] = match action_id {
        "none" => return Ok(()),

        // Navigation
        "alt_tab"       => &[KEY_LEFTALT, KEY_TAB],
        "alt_shift_tab" => &[KEY_LEFTALT, KEY_LEFTSHIFT, KEY_TAB],
        "win_d"         => &[KEY_LEFTMETA, KEY_D],
        "task_view"     => &[KEY_LEFTMETA],
        "space_left"    => workspace_keys("left"),
        "space_right"   => workspace_keys("right"),
        "page_up"       => &[KEY_PAGEUP],
        "page_down"     => &[KEY_PAGEDOWN],
        "home"          => &[KEY_HOME],
        "end"           => &[KEY_END],

        // Browser
        "browser_back"    => &[KEY_BACK],
        "browser_forward" => &[KEY_FORWARD],
        "next_tab"        => &[KEY_LEFTCTRL, KEY_TAB],
        "prev_tab"        => &[KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_TAB],
        "close_tab"       => &[KEY_LEFTCTRL, KEY_W],
        "new_tab"         => &[KEY_LEFTCTRL, KEY_T],

        // Editing
        "copy"       => &[KEY_LEFTCTRL, KEY_C],
        "paste"      => &[KEY_LEFTCTRL, KEY_V],
        "cut"        => &[KEY_LEFTCTRL, KEY_X],
        "undo"       => &[KEY_LEFTCTRL, KEY_Z],
        "select_all" => &[KEY_LEFTCTRL, KEY_A],
        "save"       => &[KEY_LEFTCTRL, KEY_S],
        "find"       => &[KEY_LEFTCTRL, KEY_F],

        // Media
        "volume_up"   => &[KEY_VOLUMEUP],
        "volume_down" => &[KEY_VOLUMEDOWN],
        "volume_mute" => &[KEY_MUTE],
        "play_pause"  => &[KEY_PLAYPAUSE],
        "next_track"  => &[KEY_NEXTSONG],
        "prev_track"  => &[KEY_PREVIOUSSONG],

        // macOS-only actions — no-op on Linux
        "mission_control" | "app_expose" | "show_desktop" | "launchpad" => {
            log::warn!(
                "[key_simulator] action '{}' is not available on Linux",
                action_id
            );
            return Ok(());
        }

        _ => {
            log::warn!(
                "[key_simulator] unknown action '{}' on Linux",
                action_id
            );
            return Ok(());
        }
    };

    send_key_combo(keys, hold_ms)
}

/// Send an arbitrary key combo resolved from a `custom:` action id.
pub fn execute_custom(keys: &[Key], hold_ms: u64) -> Result<()> {
    send_key_combo(keys, hold_ms)
}
