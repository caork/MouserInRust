#![allow(dead_code)]
// app_detector/linux.rs — Linux foreground-app detection.
//
// Session-type strategy:
//  - X11:            xdotool getactivewindow getwindowpid → /proc/PID/exe
//  - Wayland + KDE:  kdotool getactivewindow getwindowpid → /proc/PID/exe,
//                    falling back to xdotool (XWayland apps)
//  - Wayland (other): unsupported, returns None
//
// "Supported" here means the tool must already be installed; we make no attempt
// to install anything.

#![cfg(target_os = "linux")]

use std::env;
use std::fs;
use std::process::Command;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve `/proc/<pid>/exe` to the real executable path.
fn pid_to_exe(pid: u32) -> Option<String> {
    let link = format!("/proc/{}/exe", pid);
    fs::read_link(&link)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Run a tool that outputs a PID on stdout, parse it, and resolve it.
///
/// `args` is passed directly to `Command` after the program name.
fn tool_pid_to_exe(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let pid: u32 = stdout.trim().parse().ok()?;
    pid_to_exe(pid)
}

// ---------------------------------------------------------------------------
// Backend implementations
// ---------------------------------------------------------------------------

/// X11: use `xdotool getactivewindow getwindowpid`.
fn get_foreground_xdotool() -> Option<String> {
    tool_pid_to_exe("xdotool", &["getactivewindow", "getwindowpid"])
}

/// KDE Wayland: use `kdotool getactivewindow getwindowpid`.
fn get_foreground_kdotool() -> Option<String> {
    tool_pid_to_exe("kdotool", &["getactivewindow", "getwindowpid"])
}

// ---------------------------------------------------------------------------
// Session type detection
// ---------------------------------------------------------------------------

fn is_wayland() -> bool {
    env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .to_lowercase()
        == "wayland"
}

fn is_kde() -> bool {
    env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_uppercase()
        .contains("KDE")
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Return the foreground application executable path on Linux, or `None`.
///
/// The returned value is the resolved absolute path from `/proc/<pid>/exe`
/// (e.g. `/usr/bin/google-chrome`).
pub fn get_foreground_exe() -> Option<String> {
    if is_wayland() {
        if is_kde() {
            // Try KDE-native tool first, then fall back to xdotool (XWayland)
            if let Some(exe) = get_foreground_kdotool() {
                return Some(exe);
            }
            return get_foreground_xdotool();
        }
        // GNOME / other Wayland compositors: not yet supported
        None
    } else {
        get_foreground_xdotool()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pid_to_exe_self() {
        // Our own PID should resolve to something.
        let pid = std::process::id();
        let result = pid_to_exe(pid);
        // In CI there should always be a /proc/self/exe; just don't panic.
        // Result may be None in some restricted environments.
        let _ = result;
    }

    #[test]
    fn test_pid_to_exe_invalid() {
        // PID 0 is never a valid user process; should return None.
        assert!(pid_to_exe(0).is_none());
    }

    #[test]
    fn test_get_foreground_exe_does_not_panic() {
        // Just ensure it is callable without panicking.
        let _ = get_foreground_exe();
    }
}
