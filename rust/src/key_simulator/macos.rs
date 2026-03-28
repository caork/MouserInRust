//! macOS key simulation backend.
//!
//! Standard key events use `CGEventCreateKeyboardEvent` via the `core-graphics`
//! crate.  Media keys are sent through the `NSEvent otherEventWithType:` /
//! `CGEvent` pathway (NX_SYSDEFINED subtype 8), matching the Python source.
//! Mission Control and Spaces use `CGSGetSymbolicHotKeyValue` /
//! `CGSSetSymbolicHotKeyEnabled` from ApplicationServices.

#![allow(dead_code)]
#![allow(deprecated)] // cocoa 0.26 APIs are deprecated in favour of objc2-app-kit; the project pins cocoa 0.26

use std::ffi::CString;
use std::thread::sleep;
use std::time::Duration;

use anyhow::{bail, Result};
use cocoa::appkit::{
    NSEvent, NSEventModifierFlags, NSEventSubtype, NSEventType,
};
use cocoa::base::{id, nil};
use cocoa::foundation::{NSInteger, NSPoint, NSTimeInterval};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGScrollEventUnit};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

// ---------------------------------------------------------------------------
// macOS virtual-key codes (CGKeyCode)
// ---------------------------------------------------------------------------

pub type CGKeyCode = u16;

// Modifier keys
pub const KVK_COMMAND:  CGKeyCode = 0x37;
pub const KVK_SHIFT:    CGKeyCode = 0x38;
pub const KVK_OPTION:   CGKeyCode = 0x3A; // Alt
pub const KVK_CONTROL:  CGKeyCode = 0x3B;

// Common keys
pub const KVK_TAB:           CGKeyCode = 0x30;
pub const KVK_SPACE:         CGKeyCode = 0x31;
pub const KVK_RETURN:        CGKeyCode = 0x24;
pub const KVK_DELETE:        CGKeyCode = 0x33; // Backspace
pub const KVK_FORWARD_DELETE: CGKeyCode = 0x75;
pub const KVK_ESCAPE:        CGKeyCode = 0x35;

// Arrow keys
pub const KVK_LEFT_ARROW:  CGKeyCode = 0x7B;
pub const KVK_RIGHT_ARROW: CGKeyCode = 0x7C;
pub const KVK_DOWN_ARROW:  CGKeyCode = 0x7D;
pub const KVK_UP_ARROW:    CGKeyCode = 0x7E;

// Navigation cluster
pub const KVK_HOME:      CGKeyCode = 0x73;
pub const KVK_END:       CGKeyCode = 0x77;
pub const KVK_PAGE_UP:   CGKeyCode = 0x74;
pub const KVK_PAGE_DOWN: CGKeyCode = 0x79;

// ANSI letter keys (physical position, layout-independent)
pub const KVK_ANSI_A: CGKeyCode = 0x00;
pub const KVK_ANSI_B: CGKeyCode = 0x0B;
pub const KVK_ANSI_C: CGKeyCode = 0x08;
pub const KVK_ANSI_D: CGKeyCode = 0x02;
pub const KVK_ANSI_E: CGKeyCode = 0x0E;
pub const KVK_ANSI_F: CGKeyCode = 0x03;
pub const KVK_ANSI_G: CGKeyCode = 0x05;
pub const KVK_ANSI_H: CGKeyCode = 0x04;
pub const KVK_ANSI_I: CGKeyCode = 0x22;
pub const KVK_ANSI_J: CGKeyCode = 0x26;
pub const KVK_ANSI_K: CGKeyCode = 0x28;
pub const KVK_ANSI_L: CGKeyCode = 0x25;
pub const KVK_ANSI_M: CGKeyCode = 0x2E;
pub const KVK_ANSI_N: CGKeyCode = 0x2D;
pub const KVK_ANSI_O: CGKeyCode = 0x1F;
pub const KVK_ANSI_P: CGKeyCode = 0x23;
pub const KVK_ANSI_Q: CGKeyCode = 0x0C;
pub const KVK_ANSI_R: CGKeyCode = 0x0F;
pub const KVK_ANSI_S: CGKeyCode = 0x01;
pub const KVK_ANSI_T: CGKeyCode = 0x11;
pub const KVK_ANSI_U: CGKeyCode = 0x20;
pub const KVK_ANSI_V: CGKeyCode = 0x09;
pub const KVK_ANSI_W: CGKeyCode = 0x0D;
pub const KVK_ANSI_X: CGKeyCode = 0x07;
pub const KVK_ANSI_Y: CGKeyCode = 0x10;
pub const KVK_ANSI_Z: CGKeyCode = 0x06;

// Bracket keys used in tab-switching combos
pub const KVK_ANSI_LEFT_BRACKET:  CGKeyCode = 0x21;
pub const KVK_ANSI_RIGHT_BRACKET: CGKeyCode = 0x1E;

// Function keys
pub const KVK_F1:  CGKeyCode = 0x7A;
pub const KVK_F2:  CGKeyCode = 0x78;
pub const KVK_F3:  CGKeyCode = 0x63;
pub const KVK_F4:  CGKeyCode = 0x76;
pub const KVK_F5:  CGKeyCode = 0x60;
pub const KVK_F6:  CGKeyCode = 0x61;
pub const KVK_F7:  CGKeyCode = 0x62;
pub const KVK_F8:  CGKeyCode = 0x64;
pub const KVK_F9:  CGKeyCode = 0x65;
pub const KVK_F10: CGKeyCode = 0x6D;
pub const KVK_F11: CGKeyCode = 0x67;
pub const KVK_F12: CGKeyCode = 0x6F;

// ---------------------------------------------------------------------------
// NX media key IDs  (IOKit/hidsystem/ev_keymap.h)
// ---------------------------------------------------------------------------

const NX_KEYTYPE_SOUND_UP:   i64 = 0;
const NX_KEYTYPE_SOUND_DOWN: i64 = 1;
const NX_KEYTYPE_MUTE:       i64 = 7;
const NX_KEYTYPE_PLAY:       i64 = 16;
const NX_KEYTYPE_NEXT:       i64 = 17;
const NX_KEYTYPE_PREVIOUS:   i64 = 18;

// ---------------------------------------------------------------------------
// CGS symbolic hot-key IDs for Mission Control / Spaces
// ---------------------------------------------------------------------------

const SYMBOLIC_HOTKEY_SPACE_LEFT:  u32 = 79;
const SYMBOLIC_HOTKEY_SPACE_RIGHT: u32 = 81;

// ---------------------------------------------------------------------------
// FFI bindings for ApplicationServices private APIs
// ---------------------------------------------------------------------------

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGSGetSymbolicHotKeyValue(
        hotkey: u32,
        key_equivalent: *mut u16,
        virtual_key: *mut u16,
        modifiers: *mut u32,
    ) -> i32;

    fn CGSIsSymbolicHotKeyEnabled(hotkey: u32) -> bool;
    fn CGSSetSymbolicHotKeyEnabled(hotkey: u32, enabled: bool) -> i32;
    fn CoreDockSendNotification(notification: *const std::ffi::c_void, flags: i32) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithCString(
        alloc: *const std::ffi::c_void,
        c_str: *const std::ffi::c_char,
        encoding: u32,
    ) -> *const std::ffi::c_void;

    fn CFRelease(cf: *const std::ffi::c_void);
}

// CFStringEncoding kCFStringEncodingUTF8
const KCF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

// ---------------------------------------------------------------------------
// Key name → CGKeyCode map  (matches parse_custom_combo from actions.rs)
// ---------------------------------------------------------------------------

/// Resolve a lowercase key name into a macOS CGKeyCode.
/// Returns `None` for names that are handled through other channels (e.g.
/// media keys which use the NSEvent pathway).
pub fn key_name_to_keycode(name: &str) -> Option<CGKeyCode> {
    Some(match name {
        "ctrl"       => KVK_CONTROL,
        "shift"      => KVK_SHIFT,
        "alt"        => KVK_OPTION,
        "super"      => KVK_COMMAND,
        "tab"        => KVK_TAB,
        "space"      => KVK_SPACE,
        "enter"      => KVK_RETURN,
        "esc"        => KVK_ESCAPE,
        "backspace"  => KVK_DELETE,
        "delete"     => KVK_FORWARD_DELETE,
        "left"       => KVK_LEFT_ARROW,
        "right"      => KVK_RIGHT_ARROW,
        "up"         => KVK_UP_ARROW,
        "down"       => KVK_DOWN_ARROW,
        "pageup"     => KVK_PAGE_UP,
        "pagedown"   => KVK_PAGE_DOWN,
        "home"       => KVK_HOME,
        "end"        => KVK_END,
        "a" => KVK_ANSI_A, "b" => KVK_ANSI_B, "c" => KVK_ANSI_C,
        "d" => KVK_ANSI_D, "e" => KVK_ANSI_E, "f" => KVK_ANSI_F,
        "g" => KVK_ANSI_G, "h" => KVK_ANSI_H, "i" => KVK_ANSI_I,
        "j" => KVK_ANSI_J, "k" => KVK_ANSI_K, "l" => KVK_ANSI_L,
        "m" => KVK_ANSI_M, "n" => KVK_ANSI_N, "o" => KVK_ANSI_O,
        "p" => KVK_ANSI_P, "q" => KVK_ANSI_Q, "r" => KVK_ANSI_R,
        "s" => KVK_ANSI_S, "t" => KVK_ANSI_T, "u" => KVK_ANSI_U,
        "v" => KVK_ANSI_V, "w" => KVK_ANSI_W, "x" => KVK_ANSI_X,
        "y" => KVK_ANSI_Y, "z" => KVK_ANSI_Z,
        "f1"  => KVK_F1,  "f2"  => KVK_F2,  "f3"  => KVK_F3,
        "f4"  => KVK_F4,  "f5"  => KVK_F5,  "f6"  => KVK_F6,
        "f7"  => KVK_F7,  "f8"  => KVK_F8,  "f9"  => KVK_F9,
        "f10" => KVK_F10, "f11" => KVK_F11, "f12" => KVK_F12,
        // Media-key names have no CGKeyCode; callers use send_media_key() directly.
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Modifier flag computation
// ---------------------------------------------------------------------------

fn modifier_flags_for_keys(keys: &[CGKeyCode]) -> CGEventFlags {
    let mut flags = CGEventFlags::CGEventFlagNull;
    for &k in keys {
        match k {
            KVK_COMMAND => flags |= CGEventFlags::CGEventFlagCommand,
            KVK_SHIFT   => flags |= CGEventFlags::CGEventFlagShift,
            KVK_OPTION  => flags |= CGEventFlags::CGEventFlagAlternate,
            KVK_CONTROL => flags |= CGEventFlags::CGEventFlagControl,
            _ => {}
        }
    }
    flags
}

// ---------------------------------------------------------------------------
// Standard key combo
// ---------------------------------------------------------------------------

/// Press and release a chord of CGKeyCodes.
///
/// Modifier flags are computed from the key list and applied to every event,
/// matching the Python `send_key_combo` logic.
pub fn send_key_combo(keys: &[CGKeyCode], hold_ms: u64) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow::anyhow!("CGEventSource::new failed"))?;

    let flags = modifier_flags_for_keys(keys);

    // Press all keys in order
    for &k in keys {
        let event = CGEvent::new_keyboard_event(source.clone(), k, true)
            .map_err(|_| anyhow::anyhow!("CGEvent::new_keyboard_event failed for key {}", k))?;
        if !flags.is_empty() {
            event.set_flags(flags);
        }
        event.post(CGEventTapLocation::HID);
    }

    if hold_ms > 0 {
        sleep(Duration::from_millis(hold_ms));
    }

    // Release in reverse order
    for &k in keys.iter().rev() {
        let event = CGEvent::new_keyboard_event(source.clone(), k, false)
            .map_err(|_| anyhow::anyhow!("CGEvent::new_keyboard_event failed for key {}", k))?;
        event.post(CGEventTapLocation::HID);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Media key events via NSEvent NX_SYSDEFINED (subtype 8)
// ---------------------------------------------------------------------------

/// Send a media key by its NX key ID using the AppKit `NSEvent` pathway.
///
/// This mirrors the Python `_send_media_key` implementation.
/// `key_id` is one of the `NX_KEYTYPE_*` constants defined above.
///
/// # Safety
/// Uses the Objective-C runtime (`objc` / `cocoa` crates).
pub fn send_media_key(key_id: i64) -> Result<()> {
    // data1 encodes key_id in the upper 16 bits plus event flags in a lower
    // byte.  0xa = key-down flag, 0xb = key-up flag (IOKit convention).
    let data1_down: NSInteger = (key_id << 16) | (0xa << 8);
    let data1_up:   NSInteger = (key_id << 16) | (0xb << 8);

    unsafe {
        // NSEventTypeSystemDefined = 14
        let ev_type = NSEventType::NSSystemDefined;

        // Modifier flags: 0xa00 = key-down state, 0xb00 = key-up state
        let mf_down = NSEventModifierFlags::from_bits_truncate(0xa00);
        let mf_up   = NSEventModifierFlags::from_bits_truncate(0xb00);

        // NSEventSubtype value 8 = NX_SYSDEFINED (not in the cocoa enum,
        // so we transmute the integer directly via the enum's repr)
        // NSScreenChangedEventType = 8 is the variant with value 8.

        let ev_down: id =
            NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2_(
                nil,
                ev_type,
                NSPoint::new(0.0, 0.0),
                mf_down,
                0.0_f64 as NSTimeInterval,
                0,
                nil,
                // NSScreenChangedEventType has discriminant value 8 = NX_SYSDEFINED
                NSEventSubtype::NSScreenChangedEventType,
                data1_down,
                -1,
            );
        let ev_up: id =
            NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2_(
                nil,
                ev_type,
                NSPoint::new(0.0, 0.0),
                mf_up,
                0.0_f64 as NSTimeInterval,
                0,
                nil,
                NSEventSubtype::NSScreenChangedEventType,
                data1_up,
                -1,
            );

        if ev_down.is_null() || ev_up.is_null() {
            bail!("send_media_key: NSEvent creation failed for key_id {}", key_id);
        }

        // Retrieve the wrapped CGEventRef from each NSEvent
        let cg_down: *mut std::ffi::c_void = NSEvent::CGEvent(ev_down);
        let cg_up:   *mut std::ffi::c_void = NSEvent::CGEvent(ev_up);

        if cg_down.is_null() || cg_up.is_null() {
            bail!("send_media_key: CGEvent() returned null for key_id {}", key_id);
        }

        // Post through the HID event tap using the raw CGEventPost FFI
        // (core-graphics exposes this via the CGEvent::post method but we
        // hold a raw *mut c_void here, so we call the C function directly).
        CGEventPostRaw(CGEventTapLocation::HID as u32, cg_down);
        CGEventPostRaw(CGEventTapLocation::HID as u32, cg_up);
    }

    Ok(())
}

// Thin FFI shim for CGEventPost accepting a raw CGEventRef pointer.
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    #[allow(non_snake_case)]
    fn CGEventPost(tapLocation: u32, event: *mut std::ffi::c_void);
}

#[inline]
unsafe fn CGEventPostRaw(tap: u32, event: *mut std::ffi::c_void) {
    CGEventPost(tap, event);
}

// ---------------------------------------------------------------------------
// Dock notification  (Mission Control / Launchpad / App Expose / Show Desktop)
// ---------------------------------------------------------------------------

fn dock_notification(name: &str) -> Result<bool> {
    let c_name = CString::new(name)?;

    let cf_string = unsafe {
        CFStringCreateWithCString(
            std::ptr::null(),
            c_name.as_ptr(),
            KCF_STRING_ENCODING_UTF8,
        )
    };

    if cf_string.is_null() {
        return Ok(false);
    }

    let result = unsafe { CoreDockSendNotification(cf_string, 0) };
    unsafe { CFRelease(cf_string) };

    Ok(result == 0)
}

// ---------------------------------------------------------------------------
// Symbolic hot-key dispatch  (space_left / space_right)
// ---------------------------------------------------------------------------

fn post_symbolic_hotkey(hotkey: u32) -> Result<bool> {
    let mut key_equivalent: u16 = 0;
    let mut virtual_key: u16 = 0;
    let mut modifiers: u32 = 0;

    let err = unsafe {
        CGSGetSymbolicHotKeyValue(
            hotkey,
            &mut key_equivalent,
            &mut virtual_key,
            &mut modifiers,
        )
    };

    if err != 0 {
        return Ok(false);
    }

    let was_enabled = unsafe { CGSIsSymbolicHotKeyEnabled(hotkey) };
    if !was_enabled {
        unsafe { CGSSetSymbolicHotKeyEnabled(hotkey, true) };
    }

    let result = (|| -> Result<bool> {
        let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
            .map_err(|_| anyhow::anyhow!("CGEventSource::new failed"))?;

        let flags = CGEventFlags::from_bits_truncate(modifiers as u64);

        let key_down = CGEvent::new_keyboard_event(source.clone(), virtual_key, true)
            .map_err(|_| anyhow::anyhow!("CGEvent key_down failed"))?;
        let key_up = CGEvent::new_keyboard_event(source, virtual_key, false)
            .map_err(|_| anyhow::anyhow!("CGEvent key_up failed"))?;

        key_down.set_flags(flags);
        key_up.set_flags(flags);

        key_down.post(CGEventTapLocation::Session);
        key_up.post(CGEventTapLocation::Session);

        sleep(Duration::from_millis(50));
        Ok(true)
    })();

    // Restore enabled state regardless of whether posting succeeded
    if !was_enabled {
        unsafe { CGSSetSymbolicHotKeyEnabled(hotkey, false) };
    }

    result
}

// ---------------------------------------------------------------------------
// Scroll injection
// ---------------------------------------------------------------------------

/// Inject a scroll event using `CGEventCreateScrollWheelEvent2`.
///
/// `delta` is in "lines" (positive = up / right, negative = down / left).
pub fn inject_scroll(horizontal: bool, delta: i32) -> Result<()> {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow::anyhow!("CGEventSource::new failed"))?;

    // CGScrollEventUnit::LINE == 1 (type alias for u32)
    let units: CGScrollEventUnit = 1; // LINE

    // new_scroll_event(source, units, wheel_count, wheel1, wheel2, wheel3)
    // axis1 = vertical, axis2 = horizontal
    let event = if horizontal {
        CGEvent::new_scroll_event(source, units, 2, 0, delta, 0)
    } else {
        CGEvent::new_scroll_event(source, units, 1, delta, 0, 0)
    };

    let event = event.map_err(|_| anyhow::anyhow!("CGEvent::new_scroll_event failed"))?;
    event.post(CGEventTapLocation::HID);
    Ok(())
}

// ---------------------------------------------------------------------------
// Action dispatch
// ---------------------------------------------------------------------------

/// Execute a built-in action id on macOS.
pub fn execute_action(action_id: &str, hold_ms: u64) -> Result<()> {
    match action_id {
        "none" => return Ok(()),

        // --- macOS native shortcuts (dock notifications) ---
        "mission_control" => {
            if dock_notification("com.apple.expose.awake")? {
                return Ok(());
            }
            return send_key_combo(&[KVK_CONTROL, KVK_UP_ARROW], hold_ms);
        }
        "app_expose" => {
            if dock_notification("com.apple.expose.front.awake")? {
                return Ok(());
            }
            return send_key_combo(&[KVK_CONTROL, KVK_DOWN_ARROW], hold_ms);
        }
        "show_desktop" => {
            if dock_notification("com.apple.showdesktop.awake")? {
                return Ok(());
            }
            return send_key_combo(&[KVK_F11], hold_ms);
        }
        "launchpad" => {
            if dock_notification("com.apple.launchpad.toggle")? {
                return Ok(());
            }
            return send_key_combo(&[KVK_F4], hold_ms);
        }

        // --- Spaces (symbolic hot-keys with CGEvent fallback) ---
        "space_left" => {
            if post_symbolic_hotkey(SYMBOLIC_HOTKEY_SPACE_LEFT)? {
                return Ok(());
            }
            return send_key_combo(&[KVK_CONTROL, KVK_LEFT_ARROW], hold_ms);
        }
        "space_right" => {
            if post_symbolic_hotkey(SYMBOLIC_HOTKEY_SPACE_RIGHT)? {
                return Ok(());
            }
            return send_key_combo(&[KVK_CONTROL, KVK_RIGHT_ARROW], hold_ms);
        }

        // --- Media keys (NX_SYSDEFINED pathway) ---
        "volume_up"   => return send_media_key(NX_KEYTYPE_SOUND_UP),
        "volume_down" => return send_media_key(NX_KEYTYPE_SOUND_DOWN),
        "volume_mute" => return send_media_key(NX_KEYTYPE_MUTE),
        "play_pause"  => return send_media_key(NX_KEYTYPE_PLAY),
        "next_track"  => return send_media_key(NX_KEYTYPE_NEXT),
        "prev_track"  => return send_media_key(NX_KEYTYPE_PREVIOUS),

        _ => {}
    }

    // --- Standard CGEvent key combos ---
    let keys: &[CGKeyCode] = match action_id {
        // Navigation
        "alt_tab"       => &[KVK_COMMAND, KVK_TAB],
        "alt_shift_tab" => &[KVK_COMMAND, KVK_SHIFT, KVK_TAB],
        "win_d"         => &[KVK_CONTROL, KVK_UP_ARROW], // maps to Mission Control
        "task_view"     => &[KVK_CONTROL, KVK_UP_ARROW],
        "page_up"       => &[KVK_PAGE_UP],
        "page_down"     => &[KVK_PAGE_DOWN],
        "home"          => &[KVK_HOME],
        "end"           => &[KVK_END],

        // Browser
        "browser_back"    => &[KVK_COMMAND, KVK_ANSI_LEFT_BRACKET],
        "browser_forward" => &[KVK_COMMAND, KVK_ANSI_RIGHT_BRACKET],
        "next_tab"        => &[KVK_COMMAND, KVK_SHIFT, KVK_ANSI_RIGHT_BRACKET],
        "prev_tab"        => &[KVK_COMMAND, KVK_SHIFT, KVK_ANSI_LEFT_BRACKET],
        "close_tab"       => &[KVK_COMMAND, KVK_ANSI_W],
        "new_tab"         => &[KVK_COMMAND, KVK_ANSI_T],

        // Editing
        "copy"       => &[KVK_COMMAND, KVK_ANSI_C],
        "paste"      => &[KVK_COMMAND, KVK_ANSI_V],
        "cut"        => &[KVK_COMMAND, KVK_ANSI_X],
        "undo"       => &[KVK_COMMAND, KVK_ANSI_Z],
        "select_all" => &[KVK_COMMAND, KVK_ANSI_A],
        "save"       => &[KVK_COMMAND, KVK_ANSI_S],
        "find"       => &[KVK_COMMAND, KVK_ANSI_F],

        _ => {
            log::warn!(
                "[key_simulator] unknown or unsupported action '{}' on macOS",
                action_id
            );
            return Ok(());
        }
    };

    send_key_combo(keys, hold_ms)
}

/// Send an arbitrary key combo resolved from a `custom:` action id.
pub fn execute_custom(keycodes: &[CGKeyCode], hold_ms: u64) -> Result<()> {
    send_key_combo(keycodes, hold_ms)
}
