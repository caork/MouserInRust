#![allow(dead_code)]
// app_detector/windows.rs — Windows foreground-app detection.
//
// Strategy:
//  1. GetForegroundWindow() → GetWindowThreadProcessId → QueryFullProcessImageNameW
//  2. If result is ApplicationFrameHost.exe (UWP host): enumerate child windows
//     to find the real hosted process.
//  3. If result is explorer.exe with an unexpected window class: try the same
//     UWP child-enumeration, then fall back to a global window scan.

#![cfg(target_os = "windows")]

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;

use windows::core::PWSTR;
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, TRUE};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, EnumWindows, GetClassNameW, GetForegroundWindow,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};

// Window classes that belong to genuine File Explorer / shell windows.
const EXPLORER_CLASSES: &[&str] = &[
    "CabinetWClass",           // File Explorer windows
    "Shell_TrayWnd",           // Taskbar
    "Shell_SecondaryTrayWnd",  // Taskbar on secondary monitors
    "Progman",                 // Desktop
    "WorkerW",                 // Desktop worker
];

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Retrieve the full executable path for a PID, or `None` on failure.
fn path_from_pid(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    unsafe {
        let hproc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let pwstr = PWSTR(buf.as_mut_ptr());

        let ok = QueryFullProcessImageNameW(hproc, PROCESS_NAME_WIN32, pwstr, &mut size);
        let _ = windows::Win32::Foundation::CloseHandle(hproc);

        if ok.is_ok() && size > 0 {
            let os = OsString::from_wide(&buf[..size as usize]);
            Some(os.to_string_lossy().into_owned())
        } else {
            None
        }
    }
}

/// Return the window class name for `hwnd`.
fn get_window_class(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    unsafe {
        GetClassNameW(hwnd, &mut buf);
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    OsString::from_wide(&buf[..end]).to_string_lossy().into_owned()
}

/// Return the window title for `hwnd`.
fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len == 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let copied = GetWindowTextW(hwnd, &mut buf);
        if copied == 0 {
            return String::new();
        }
        OsString::from_wide(&buf[..copied as usize])
            .to_string_lossy()
            .into_owned()
    }
}

// ---------------------------------------------------------------------------
// UWP resolution: enumerate child windows of the ApplicationFrameHost window
// to find the real hosted process.
// ---------------------------------------------------------------------------

struct UwpChildState {
    host_pid: u32,
    result: Option<String>,
}

unsafe extern "system" fn enum_child_callback(child_hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam.0 as *mut UwpChildState);

    let mut child_pid: u32 = 0;
    GetWindowThreadProcessId(child_hwnd, Some(&mut child_pid));

    if child_pid != 0 && child_pid != state.host_pid {
        if let Some(path) = path_from_pid(child_pid) {
            let base = Path::new(&path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if base != "applicationframehost.exe" {
                state.result = Some(path);
                return BOOL(0); // stop enumeration
            }
        }
    }
    TRUE
}

/// Enumerate child windows of `hwnd` (an ApplicationFrameHost window) to find
/// the real hosted process path.
fn resolve_uwp_child(hwnd: HWND) -> Option<String> {
    let mut host_pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut host_pid)) };

    let mut state = UwpChildState {
        host_pid,
        result: None,
    };
    unsafe {
        let _ = EnumChildWindows(
            hwnd,
            Some(enum_child_callback),
            LPARAM(&mut state as *mut _ as isize),
        );
    }
    state.result
}

// ---------------------------------------------------------------------------
// Global UWP scan: walk all top-level windows looking for an
// ApplicationFrameHost window that has a hosted child.
// ---------------------------------------------------------------------------

struct GlobalUwpState {
    result: Option<String>,
}

unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam.0 as *mut GlobalUwpState);

    if IsWindowVisible(hwnd).as_bool() {
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid != 0 {
            if let Some(path) = path_from_pid(pid) {
                let base = Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase();
                if base == "applicationframehost.exe" {
                    if let Some(real) = resolve_uwp_child(hwnd) {
                        state.result = Some(real);
                        return BOOL(0); // stop
                    }
                }
            }
        }
    }
    TRUE
}

fn find_uwp_app_global() -> Option<String> {
    let mut state = GlobalUwpState { result: None };
    unsafe {
        let _ = EnumWindows(
            Some(enum_windows_callback),
            LPARAM(&mut state as *mut _ as isize),
        );
    }
    state.result
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Return the foreground application executable path (full path), or `None`.
pub fn get_foreground_exe() -> Option<String> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null() {
        return None;
    }

    let mut pid: u32 = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }

    let exe_path = path_from_pid(pid)?;
    let base_lower = Path::new(&exe_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    // UWP host – find the real process
    if base_lower == "applicationframehost.exe" {
        return resolve_uwp_child(hwnd);
    }

    // Explorer.exe – only return it if the window class is a known shell class
    if base_lower == "explorer.exe" {
        let wc = get_window_class(hwnd);
        if !EXPLORER_CLASSES.contains(&wc.as_str()) {
            log::debug!(
                "[AppDetect] FG: explorer.exe class={} title='{}'",
                wc,
                get_window_title(hwnd)
            );
            let real = resolve_uwp_child(hwnd);
            if real.is_some() {
                return real;
            }
            return find_uwp_app_global();
        }
    }

    Some(exe_path)
}
