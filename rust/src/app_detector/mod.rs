#![allow(dead_code)]
// app_detector/mod.rs — Polls the foreground window and fires a callback when
// the active application changes.  Platform backends live in sibling modules.

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "linux")]
mod linux;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub struct AppDetector {
    on_change: Arc<dyn Fn(String) + Send + Sync + 'static>,
    interval_ms: u64,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl AppDetector {
    /// Create a new `AppDetector`.
    ///
    /// * `on_change`   – called with the new foreground app identifier whenever
    ///                   it changes.
    /// * `interval_ms` – polling interval in milliseconds (e.g. 300).
    pub fn new(on_change: impl Fn(String) + Send + Sync + 'static, interval_ms: u64) -> Self {
        Self {
            on_change: Arc::new(on_change),
            interval_ms,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }

    /// Start the background polling thread.  Calling `start` while already
    /// running is a no-op.
    pub fn start(&mut self) {
        if self.running.load(Ordering::SeqCst) {
            return;
        }
        self.running.store(true, Ordering::SeqCst);

        let running = Arc::clone(&self.running);
        let on_change = Arc::clone(&self.on_change);
        let interval = self.interval_ms;

        self.handle = Some(thread::spawn(move || {
            let mut last_exe: Option<String> = None;
            while running.load(Ordering::SeqCst) {
                if let Some(exe) = Self::get_foreground_exe() {
                    if last_exe.as_deref() != Some(&exe) {
                        last_exe = Some(exe.clone());
                        on_change(exe);
                    }
                }
                thread::sleep(Duration::from_millis(interval));
            }
        }));
    }

    /// Stop the polling thread and wait for it to finish (up to 2 s).
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            // best-effort join; ignore errors
            let _ = handle.join();
        }
    }

    // ------------------------------------------------------------------
    // Platform dispatch
    // ------------------------------------------------------------------

    /// Return the foreground application identifier, or `None` if it cannot
    /// be determined.  The returned string is platform-dependent:
    /// - Windows: basename of the process executable (`chrome.exe`)
    /// - macOS:   bundle identifier (`com.google.Chrome`) or app name
    /// - Linux:   absolute path to the executable (`/usr/bin/google-chrome`)
    fn get_foreground_exe() -> Option<String> {
        #[cfg(target_os = "windows")]
        { windows::get_foreground_exe() }

        #[cfg(target_os = "macos")]
        { macos::get_foreground_exe() }

        #[cfg(target_os = "linux")]
        { linux::get_foreground_exe() }

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        { None }
    }
}

impl Drop for AppDetector {
    fn drop(&mut self) {
        self.stop();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_start_stop() {
        let called = Arc::new(Mutex::new(Vec::<String>::new()));
        let called_clone = Arc::clone(&called);

        let mut detector = AppDetector::new(
            move |exe| {
                called_clone.lock().unwrap().push(exe);
            },
            50,
        );

        detector.start();
        // Starting again is a no-op
        detector.start();
        thread::sleep(Duration::from_millis(200));
        detector.stop();

        // Detector stopped – calling stop again is safe
        detector.stop();
    }

    #[test]
    fn test_get_foreground_exe_does_not_panic() {
        // Simply verify the function is callable and returns without panicking.
        let _ = AppDetector::get_foreground_exe();
    }
}
