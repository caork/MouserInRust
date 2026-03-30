//! Windows key simulation backend.
//!
//! Uses the Win32 `SendInput` API with virtual-key codes.  All I/O with the
//! kernel happens inside `unsafe` blocks; the public surface is entirely safe.

#![allow(dead_code)]

use std::thread::sleep;
use std::time::Duration;

use anyhow::{bail, Result};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL, MOUSEINPUT, VIRTUAL_KEY,
};

// ---------------------------------------------------------------------------
// Virtual-key code constants
// (those not directly re-exported by the `windows` crate at the version in
//  Cargo.toml are defined as plain u16 literals and wrapped in VIRTUAL_KEY).
// ---------------------------------------------------------------------------

const VK_LCONTROL: u16 = 0xA2;
const VK_LSHIFT:   u16 = 0xA0;
const VK_LMENU:    u16 = 0xA4; // Left Alt
const VK_LWIN:     u16 = 0x5B;

// Standard modifiers (non-side-specific, used in the Windows action table)
const VK_CONTROL:  u16 = 0x11;
const VK_SHIFT:    u16 = 0x10;
const VK_MENU:     u16 = 0x12; // Alt

const VK_BACK:     u16 = 0x08; // Backspace
const VK_TAB:      u16 = 0x09;
const VK_RETURN:   u16 = 0x0D;
const VK_ESCAPE:   u16 = 0x1B;
const VK_SPACE:    u16 = 0x20;
const VK_DELETE:   u16 = 0x2E;

const VK_PRIOR:    u16 = 0x21; // Page Up
const VK_NEXT:     u16 = 0x22; // Page Down
const VK_END:      u16 = 0x23;
const VK_HOME:     u16 = 0x24;
const VK_LEFT:     u16 = 0x25;
const VK_UP:       u16 = 0x26;
const VK_RIGHT:    u16 = 0x27;
const VK_DOWN:     u16 = 0x28;

const VK_VOLUME_MUTE:        u16 = 0xAD;
const VK_VOLUME_DOWN:        u16 = 0xAE;
const VK_VOLUME_UP:          u16 = 0xAF;
const VK_MEDIA_NEXT_TRACK:   u16 = 0xB0;
const VK_MEDIA_PREV_TRACK:   u16 = 0xB1;
const VK_MEDIA_STOP:         u16 = 0xB2;
const VK_MEDIA_PLAY_PAUSE:   u16 = 0xB3;

const VK_BROWSER_BACK:       u16 = 0xA6;
const VK_BROWSER_FORWARD:    u16 = 0xA7;
const VK_BROWSER_REFRESH:    u16 = 0xA8;
const VK_BROWSER_STOP:       u16 = 0xA9;
const VK_BROWSER_HOME:       u16 = 0xAC;

// Function keys
const VK_F1:  u16 = 0x70;
const VK_F2:  u16 = 0x71;
const VK_F3:  u16 = 0x72;
const VK_F4:  u16 = 0x73;
const VK_F5:  u16 = 0x74;
const VK_F6:  u16 = 0x75;
const VK_F7:  u16 = 0x76;
const VK_F8:  u16 = 0x77;
const VK_F9:  u16 = 0x78;
const VK_F10: u16 = 0x79;
const VK_F11: u16 = 0x7A;
const VK_F12: u16 = 0x7B;

// Letters (A=0x41 … Z=0x5A)
const VK_A: u16 = 0x41;
const VK_B: u16 = 0x42;
const VK_C: u16 = 0x43;
const VK_D: u16 = 0x44;
const VK_E: u16 = 0x45;
const VK_F: u16 = 0x46;
const VK_G: u16 = 0x47;
const VK_H: u16 = 0x48;
const VK_I: u16 = 0x49;
const VK_J: u16 = 0x4A;
const VK_K: u16 = 0x4B;
const VK_L: u16 = 0x4C;
const VK_M: u16 = 0x4D;
const VK_N: u16 = 0x4E;
const VK_O: u16 = 0x4F;
const VK_P: u16 = 0x50;
const VK_Q: u16 = 0x51;
const VK_R: u16 = 0x52;
const VK_S: u16 = 0x53;
const VK_T: u16 = 0x54;
const VK_U: u16 = 0x55;
const VK_V: u16 = 0x56;
const VK_W: u16 = 0x57;
const VK_X: u16 = 0x58;
const VK_Y: u16 = 0x59;
const VK_Z: u16 = 0x5A;

// ---------------------------------------------------------------------------
// Extended-key set
// Keys in this set require KEYEVENTF_EXTENDEDKEY when sent through SendInput.
// ---------------------------------------------------------------------------

fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        VK_BROWSER_BACK
            | VK_BROWSER_FORWARD
            | VK_BROWSER_REFRESH
            | VK_BROWSER_STOP
            | VK_BROWSER_HOME
            | VK_VOLUME_MUTE
            | VK_VOLUME_DOWN
            | VK_VOLUME_UP
            | VK_MEDIA_NEXT_TRACK
            | VK_MEDIA_PREV_TRACK
            | VK_MEDIA_STOP
            | VK_MEDIA_PLAY_PAUSE
            | VK_LEFT
            | VK_RIGHT
            | VK_UP
            | VK_DOWN
            | VK_DELETE
            | VK_RETURN
            | VK_TAB
            | VK_PRIOR
            | VK_NEXT
            | VK_HOME
            | VK_END
    )
}

// ---------------------------------------------------------------------------
// Key name → VK code map  (matches parse_custom_combo key names from actions.rs)
// ---------------------------------------------------------------------------

/// Resolve a lowercase key name (as returned by `parse_custom_combo`) into a
/// Windows virtual-key code.  Returns `None` for unknown names.
pub fn key_name_to_vk(name: &str) -> Option<u16> {
    Some(match name {
        "ctrl"       => VK_CONTROL,
        "shift"      => VK_SHIFT,
        "alt"        => VK_MENU,
        "super"      => VK_LWIN,
        "tab"        => VK_TAB,
        "space"      => VK_SPACE,
        "enter"      => VK_RETURN,
        "esc"        => VK_ESCAPE,
        "backspace"  => VK_BACK,
        "delete"     => VK_DELETE,
        "left"       => VK_LEFT,
        "right"      => VK_RIGHT,
        "up"         => VK_UP,
        "down"       => VK_DOWN,
        "pageup"     => VK_PRIOR,
        "pagedown"   => VK_NEXT,
        "home"       => VK_HOME,
        "end"        => VK_END,
        "a"  => VK_A,  "b" => VK_B,  "c" => VK_C,  "d" => VK_D,
        "e"  => VK_E,  "f" => VK_F,  "g" => VK_G,  "h" => VK_H,
        "i"  => VK_I,  "j" => VK_J,  "k" => VK_K,  "l" => VK_L,
        "m"  => VK_M,  "n" => VK_N,  "o" => VK_O,  "p" => VK_P,
        "q"  => VK_Q,  "r" => VK_R,  "s" => VK_S,  "t" => VK_T,
        "u"  => VK_U,  "v" => VK_V,  "w" => VK_W,  "x" => VK_X,
        "y"  => VK_Y,  "z" => VK_Z,
        "f1"  => VK_F1,  "f2"  => VK_F2,  "f3"  => VK_F3,
        "f4"  => VK_F4,  "f5"  => VK_F5,  "f6"  => VK_F6,
        "f7"  => VK_F7,  "f8"  => VK_F8,  "f9"  => VK_F9,
        "f10" => VK_F10, "f11" => VK_F11, "f12" => VK_F12,
        "volumeup"   => VK_VOLUME_UP,
        "volumedown" => VK_VOLUME_DOWN,
        "mute"       => VK_VOLUME_MUTE,
        "playpause"  => VK_MEDIA_PLAY_PAUSE,
        "nexttrack"  => VK_MEDIA_NEXT_TRACK,
        "prevtrack"  => VK_MEDIA_PREV_TRACK,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Low-level SendInput helpers
// ---------------------------------------------------------------------------

fn make_key_input(vk: u16, flags: windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Press and release a chord of virtual-key codes.
///
/// All keys are pressed in order, then held for `hold_ms` milliseconds,
/// then released in reverse order — matching the Python `send_key_combo`.
pub fn send_key_combo(keys: &[u16], hold_ms: u64) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }

    let mut inputs: Vec<INPUT> = Vec::with_capacity(keys.len() * 2);

    // Press phase
    for &vk in keys {
        let flags = if is_extended(vk) {
            KEYEVENTF_EXTENDEDKEY
        } else {
            windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0)
        };
        inputs.push(make_key_input(vk, flags));
    }

    // Release phase (reverse order)
    for &vk in keys.iter().rev() {
        let mut flags = KEYEVENTF_KEYUP;
        if is_extended(vk) {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        inputs.push(make_key_input(vk, flags));
    }

    let sent = unsafe {
        SendInput(
            &inputs,
            std::mem::size_of::<INPUT>() as i32,
        )
    };

    if sent != inputs.len() as u32 {
        bail!("SendInput sent {}/{} events", sent, inputs.len());
    }
    Ok(())
}

/// Phased Alt+Arrow for browser back/forward.
///
/// Some Chromium-based windows silently drop batched VK-only SendInput chords.
/// Sending modifier-down → key-tap → modifier-up with pauses is accepted reliably.
pub fn send_phased_alt_arrow(arrow_vk: u16, hold_ms: u64) -> Result<()> {
    let pause = Duration::from_millis(hold_ms.max(1));

    let ext = KEYEVENTF_EXTENDEDKEY;

    let alt_down = make_key_input(
        VK_LMENU,
        windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0),
    );
    let alt_up = make_key_input(VK_LMENU, KEYEVENTF_KEYUP);
    let arr_down = make_key_input(arrow_vk, ext);
    let mut arr_up_flags = KEYEVENTF_KEYUP;
    arr_up_flags |= ext;
    let arr_up = make_key_input(arrow_vk, arr_up_flags);

    let send_batch = |batch: &[INPUT]| -> Result<()> {
        let sent = unsafe {
            SendInput(batch, std::mem::size_of::<INPUT>() as i32)
        };
        if sent != batch.len() as u32 {
            bail!("SendInput sent {}/{} events", sent, batch.len());
        }
        Ok(())
    };

    send_batch(&[alt_down])?;
    sleep(pause);
    send_batch(&[arr_down, arr_up])?;
    sleep(pause);
    send_batch(&[alt_up])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Scroll injection
// ---------------------------------------------------------------------------

/// Inject a vertical (`horizontal = false`) or horizontal scroll event.
///
/// `delta` uses the Windows WHEEL_DELTA convention: 120 = one detent up/right,
/// -120 = one detent down/left.
pub fn inject_scroll(horizontal: bool, delta: i32) -> Result<()> {
    let flags = if horizontal {
        MOUSEEVENTF_HWHEEL
    } else {
        MOUSEEVENTF_WHEEL
    };

    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: delta as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let sent = unsafe {
        SendInput(&[input], std::mem::size_of::<INPUT>() as i32)
    };

    if sent != 1 {
        bail!("inject_scroll: SendInput failed");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Action dispatch
// ---------------------------------------------------------------------------

/// Execute a built-in action id on Windows.
///
/// Returns `Ok(())` for "none" / unknown ids (no-op with a log warning).
pub fn execute_action(action_id: &str, hold_ms: u64) -> Result<()> {
    // Custom key combos are resolved by the caller (mod.rs) and forwarded as
    // a VK slice, but we handle the full dispatch here for completeness.
    let keys: &[u16] = match action_id {
        "none"            => return Ok(()),

        // Navigation
        "alt_tab"         => &[VK_MENU, VK_TAB],
        "alt_shift_tab"   => &[VK_MENU, VK_SHIFT, VK_TAB],
        "win_d"           => &[VK_LWIN, VK_D],
        "task_view"       => &[VK_LWIN, VK_TAB],
        "space_left"      => &[VK_CONTROL, VK_LWIN, VK_LEFT],
        "space_right"     => &[VK_CONTROL, VK_LWIN, VK_RIGHT],
        "page_up"         => &[VK_PRIOR],
        "page_down"       => &[VK_NEXT],
        "home"            => &[VK_HOME],
        "end"             => &[VK_END],

        // Browser
        "browser_back"    => return send_phased_alt_arrow(VK_LEFT, hold_ms),
        "browser_forward" => return send_phased_alt_arrow(VK_RIGHT, hold_ms),
        "next_tab"        => &[VK_CONTROL, VK_TAB],
        "prev_tab"        => &[VK_CONTROL, VK_SHIFT, VK_TAB],
        "close_tab"       => &[VK_CONTROL, VK_W],
        "new_tab"         => &[VK_CONTROL, VK_T],

        // Editing
        "copy"            => &[VK_CONTROL, VK_C],
        "paste"           => &[VK_CONTROL, VK_V],
        "cut"             => &[VK_CONTROL, VK_X],
        "undo"            => &[VK_CONTROL, VK_Z],
        "select_all"      => &[VK_CONTROL, VK_A],
        "save"            => &[VK_CONTROL, VK_S],
        "find"            => &[VK_CONTROL, VK_F],

        // Media
        "volume_up"       => &[VK_VOLUME_UP],
        "volume_down"     => &[VK_VOLUME_DOWN],
        "volume_mute"     => &[VK_VOLUME_MUTE],
        "play_pause"      => &[VK_MEDIA_PLAY_PAUSE],
        "next_track"      => &[VK_MEDIA_NEXT_TRACK],
        "prev_track"      => &[VK_MEDIA_PREV_TRACK],

        // macOS-only actions — no-op on Windows
        "mission_control" | "app_expose" | "show_desktop" | "launchpad" => {
            log::warn!("[key_simulator] action '{}' is not available on Windows", action_id);
            return Ok(());
        }

        _ => {
            log::warn!("[key_simulator] unknown action '{}' on Windows", action_id);
            return Ok(());
        }
    };

    send_key_combo(keys, hold_ms)
}

/// Send an arbitrary VK combo resolved from a `custom:` action id.
pub fn execute_custom(vk_codes: &[u16], hold_ms: u64) -> Result<()> {
    send_key_combo(vk_codes, hold_ms)
}
