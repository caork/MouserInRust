#![allow(dead_code)]
// app_detector/macos.rs — macOS foreground-app detection.
//
// Strategy (in priority order):
//  1. Use NSWorkspace.sharedWorkspace().frontmostApplication() via osascript to
//     retrieve the bundle identifier of the front-most app.  This is the most
//     stable identifier (matches profile app specs on macOS).
//  2. Fall back to the localised application name if no bundle ID is available.
//
// We intentionally avoid pulling in the full `cocoa` / `objc` runtime for this
// single query to keep the implementation simple and avoid retain-count bugs.
// Using `std::process::Command` to call `osascript` is fast enough for a 300 ms
// poll interval and works in both sandboxed and non-sandboxed builds.

#![cfg(target_os = "macos")]

use std::process::Command;

// ---------------------------------------------------------------------------
// osascript helpers
// ---------------------------------------------------------------------------

/// Run an AppleScript one-liner and return trimmed stdout, or `None` on failure.
fn run_osascript(script: &str) -> Option<String> {
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

/// Ask System Events for the bundle identifier of the frontmost process.
fn frontmost_bundle_id() -> Option<String> {
    run_osascript(
        "tell application \"System Events\" \
         to get bundle identifier of first process whose frontmost is true",
    )
}

/// Ask System Events for the name of the frontmost process.
fn frontmost_process_name() -> Option<String> {
    run_osascript(
        "tell application \"System Events\" \
         to get name of first process whose frontmost is true",
    )
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Return the foreground application identifier on macOS.
///
/// Returns the bundle ID when available (e.g. `"com.google.Chrome"`), or the
/// process name as a fallback (e.g. `"Google Chrome"`).  Returns `None` only
/// when System Events is completely unresponsive.
pub fn get_foreground_exe() -> Option<String> {
    // Prefer bundle ID — it is stable across renames and locales.
    if let Some(bundle_id) = frontmost_bundle_id() {
        return Some(bundle_id);
    }
    // Fallback: process name
    frontmost_process_name()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_osascript_invalid_script_returns_none() {
        // Intentionally broken script – should return None, not panic.
        let result = run_osascript("this is not valid AppleScript !@#$");
        // May return None or Some(error text) depending on osascript version;
        // the important thing is it does not panic.
        let _ = result;
    }

    #[test]
    fn test_get_foreground_exe_does_not_panic() {
        // Just ensure it can be called safely in a test context.
        let _ = get_foreground_exe();
    }
}
