#![allow(dead_code)]
// single_instance.rs — Ensure only one Mouser process runs at a time.
//
// Unix:    Advisory file lock (flock / fcntl) on a lock file in the config dir.
//          The OS releases the lock automatically when the file descriptor is
//          closed (i.e. when the process exits or `SingleInstance` is dropped).
//
// Windows: A named kernel mutex.  The mutex is released when the handle is
//          closed (i.e. when `SingleInstance` is dropped or the process exits).

use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Lock-file path helper (shared by all platforms, used on Unix)
// ---------------------------------------------------------------------------

fn lock_file_path() -> PathBuf {
    // Prefer the Mouser config dir; fall back to the OS temp dir.
    crate::config::config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("Mouser"))
        .join("mouser.lock")
}

// ---------------------------------------------------------------------------
// Unix implementation (Linux + macOS)
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod unix_impl {
    use super::lock_file_path;
    use std::fs::{self, OpenOptions};
    use std::os::unix::io::IntoRawFd;
    use std::path::PathBuf;

    pub struct Inner {
        pub fd: std::os::unix::io::RawFd,
        pub path: PathBuf,
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            // Close the fd – this automatically releases the advisory lock.
            extern "C" { fn close(fd: i32) -> i32; }
            unsafe { close(self.fd) };
        }
    }

    // Inline the two POSIX symbols we need to avoid a hard libc dependency.
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
        fn close(fd: i32) -> i32;
    }

    // flock operation flags (POSIX / BSDs / Linux all agree on these values)
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;

    pub fn try_acquire() -> Option<Inner> {
        let path = lock_file_path();

        // Ensure the parent directory exists.
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .ok()?;

        let fd = file.into_raw_fd();

        // Try a non-blocking exclusive flock.
        let ret = unsafe { flock(fd, LOCK_EX | LOCK_NB) };
        if ret != 0 {
            // Another instance holds the lock.
            unsafe { close(fd) };
            return None;
        }

        Some(Inner { fd, path })
    }
}

// libc is available as a platform dependency on macOS and Linux

// ---------------------------------------------------------------------------
// Windows implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod windows_impl {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{CreateMutexW, MUTEX_ALL_ACCESS};
    use windows::core::PCWSTR;

    const MUTEX_NAME: &str = "MouserSingleInstance";

    pub struct Inner {
        pub handle: HANDLE,
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            unsafe { let _ = CloseHandle(self.handle); }
        }
    }

    pub fn try_acquire() -> Option<Inner> {
        let name: Vec<u16> = MUTEX_NAME.encode_utf16().chain(std::iter::once(0)).collect();

        let handle = unsafe {
            CreateMutexW(None, true, PCWSTR(name.as_ptr())).ok()?
        };

        // If the mutex already existed and was owned, GetLastError() returns
        // ERROR_ALREADY_EXISTS (183).  We detect this via the last OS error.
        let last_err = unsafe { windows::Win32::Foundation::GetLastError() };
        if last_err == windows::Win32::Foundation::ERROR_ALREADY_EXISTS {
            unsafe { let _ = CloseHandle(handle); }
            return None;
        }

        Some(Inner { handle })
    }
}

// ---------------------------------------------------------------------------
// Public type
// ---------------------------------------------------------------------------

/// A guard that represents exclusive ownership of the "single instance" lock.
///
/// The lock is released when this value is dropped.
///
/// # Usage
/// ```ignore
/// match SingleInstance::try_acquire() {
///     Some(_guard) => { /* run the application */ }
///     None         => { eprintln!("Mouser is already running."); return; }
/// }
/// ```
pub struct SingleInstance {
    #[cfg(unix)]
    _inner: unix_impl::Inner,
    #[cfg(target_os = "windows")]
    _inner: windows_impl::Inner,
    // Fallback: unit on unsupported platforms
    #[cfg(not(any(unix, target_os = "windows")))]
    _phantom: (),
}

impl SingleInstance {
    /// Attempt to acquire the single-instance lock.
    ///
    /// Returns `Some(SingleInstance)` if this process is the first (or only)
    /// running instance, or `None` if another instance is already running.
    pub fn try_acquire() -> Option<Self> {
        #[cfg(unix)]
        {
            unix_impl::try_acquire().map(|inner| Self { _inner: inner })
        }

        #[cfg(target_os = "windows")]
        {
            windows_impl::try_acquire().map(|inner| Self { _inner: inner })
        }

        #[cfg(not(any(unix, target_os = "windows")))]
        {
            Some(Self { _phantom: () })
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acquire_once() {
        let guard = SingleInstance::try_acquire();
        // Should succeed on a clean test run.
        assert!(guard.is_some(), "Failed to acquire SingleInstance lock");
    }

    #[test]
    fn test_lock_file_path_non_empty() {
        let p = lock_file_path();
        assert!(!p.as_os_str().is_empty());
        assert!(p.file_name().unwrap().to_string_lossy().contains("mouser"));
    }
}
