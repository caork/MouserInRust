#![allow(dead_code)]
// startup.rs — Cross-platform login-item / autostart management.
//
// Windows:  HKCU\Software\Microsoft\Windows\CurrentVersion\Run  ("Mouser")
// macOS:    ~/Library/LaunchAgents/io.github.tombadash.mouser.plist
//           (bootstrapped with `launchctl bootstrap gui/<uid>`)
// Linux:    ~/.config/autostart/mouser.desktop  (XDG autostart)

use anyhow::{Context, Result};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// macOS / Linux constants
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = "io.github.tombadash.mouser";

#[cfg(target_os = "linux")]
const DESKTOP_FILE_NAME: &str = "mouser.desktop";

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod windows_impl {
    use anyhow::{Context, Result};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_SZ,
    };
    use windows::core::PCWSTR;

    const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "Mouser";

    fn to_wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn current_exe_command() -> String {
        std::env::current_exe()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    pub fn is_login_item_enabled() -> bool {
        use windows::Win32::System::Registry::{RegGetValueW, RRF_RT_REG_SZ, HKEY_CURRENT_USER};
        let subkey = to_wide_null(RUN_SUBKEY);
        let value = to_wide_null(VALUE_NAME);
        let mut buf = [0u16; 512];
        let mut buf_size = (buf.len() * 2) as u32;
        let result = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut buf_size),
            )
        };
        result.is_ok()
    }

    pub fn set_login_item(enabled: bool) -> Result<()> {
        let subkey = to_wide_null(RUN_SUBKEY);
        let value_name = to_wide_null(VALUE_NAME);

        let mut hkey = windows::Win32::System::Registry::HKEY::default();
        unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                0,
                KEY_SET_VALUE,
                &mut hkey,
            )
            .context("RegOpenKeyExW failed")?;
        }

        let result: Result<()> = (|| {
            if enabled {
                let cmd = current_exe_command();
                let cmd_wide = to_wide_null(&cmd);
                // REG_SZ value is a null-terminated UTF-16 string; size in bytes
                let byte_len = (cmd_wide.len() * 2) as u32;
                unsafe {
                    RegSetValueExW(
                        hkey,
                        PCWSTR(value_name.as_ptr()),
                        0,
                        REG_SZ,
                        Some(cmd_wide.as_ptr() as *const u8),
                        byte_len,
                    )
                    .context("RegSetValueExW failed")?;
                }
            } else {
                unsafe {
                    // Ignore "not found" errors when deleting
                    let _ = RegDeleteValueW(hkey, PCWSTR(value_name.as_ptr()));
                }
            }
            Ok(())
        })();

        unsafe { let _ = RegCloseKey(hkey); }
        result
    }
}

// ---------------------------------------------------------------------------
// macOS implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod macos_impl {
    use super::LAUNCH_AGENT_LABEL;
    use anyhow::{Context, Result};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    fn plist_path() -> std::path::PathBuf {
        dirs::home_dir()
            .unwrap_or_default()
            .join("Library/LaunchAgents")
            .join(format!("{}.plist", LAUNCH_AGENT_LABEL))
    }

    fn current_exe_path() -> String {
        std::env::current_exe()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    fn uid() -> u32 {
        extern "C" { fn getuid() -> u32; }
        unsafe { getuid() }
    }

    fn launchctl(args: &[&str]) {
        let _ = Command::new("launchctl").args(args).output();
    }

    fn domain() -> String {
        format!("gui/{}", uid())
    }

    pub fn is_login_item_enabled() -> bool {
        plist_path().exists()
    }

    pub fn set_login_item(enabled: bool) -> Result<()> {
        let path = plist_path();
        let domain = domain();
        let path_str = path.to_string_lossy();

        if enabled {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).context("create LaunchAgents dir")?;
            }

            // Unload any old version first
            if path.exists() {
                launchctl(&["bootout", &domain, &path_str]);
            }

            let exe = current_exe_path();
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
                label = LAUNCH_AGENT_LABEL,
                exe = exe,
            );

            fs::write(&path, plist.as_bytes()).context("write plist")?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
                .context("chmod plist")?;

            let out = Command::new("launchctl")
                .args(["bootstrap", &domain, &path_str])
                .output()
                .context("launchctl bootstrap")?;

            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                log::warn!("[startup] launchctl bootstrap failed: {}", stderr.trim());
            }
        } else {
            if path.exists() {
                launchctl(&["bootout", &domain, &path_str]);
                let _ = fs::remove_file(&path);
            } else {
                launchctl(&["bootout", &domain, LAUNCH_AGENT_LABEL]);
            }
        }
        Ok(())
    }
}

// (libc is pulled in as a macOS platform dependency and used inside macos_impl)

// ---------------------------------------------------------------------------
// Linux implementation (XDG autostart)
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::DESKTOP_FILE_NAME;
    use anyhow::{Context, Result};
    use std::fs;

    fn autostart_dir() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_default()
                    .join(".config")
            })
            .join("autostart")
    }

    fn desktop_path() -> std::path::PathBuf {
        autostart_dir().join(DESKTOP_FILE_NAME)
    }

    fn current_exe_path() -> String {
        std::env::current_exe()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    pub fn is_login_item_enabled() -> bool {
        desktop_path().exists()
    }

    pub fn set_login_item(enabled: bool) -> Result<()> {
        let path = desktop_path();

        if enabled {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).context("create autostart dir")?;
            }
            let exe = current_exe_path();
            let content = format!(
                "[Desktop Entry]\n\
                 Type=Application\n\
                 Name=Mouser\n\
                 Exec={exe}\n\
                 Hidden=false\n\
                 NoDisplay=false\n\
                 X-GNOME-Autostart-enabled=true\n",
                exe = exe
            );
            fs::write(&path, content.as_bytes()).context("write .desktop file")?;
        } else {
            if path.exists() {
                fs::remove_file(&path).context("remove .desktop file")?;
            }
        }
        Ok(())
    }

    use std::path::PathBuf;
}

// ---------------------------------------------------------------------------
// Public API — thin wrappers that dispatch to the platform module
// ---------------------------------------------------------------------------

/// Returns `true` if the Mouser login-item / autostart entry currently exists.
pub fn is_login_item_enabled() -> bool {
    #[cfg(target_os = "windows")]
    { windows_impl::is_login_item_enabled() }

    #[cfg(target_os = "macos")]
    { macos_impl::is_login_item_enabled() }

    #[cfg(target_os = "linux")]
    { linux_impl::is_login_item_enabled() }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    { false }
}

/// Enable or disable the Mouser login item / autostart entry.
pub fn set_login_item(enabled: bool) -> Result<()> {
    #[cfg(target_os = "windows")]
    { windows_impl::set_login_item(enabled) }

    #[cfg(target_os = "macos")]
    { macos_impl::set_login_item(enabled) }

    #[cfg(target_os = "linux")]
    { linux_impl::set_login_item(enabled) }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    { Ok(()) }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_login_item_enabled_does_not_panic() {
        let _ = is_login_item_enabled();
    }

    // We intentionally do NOT call set_login_item in tests to avoid
    // mutating the user's system during a test run.
}
