#![allow(dead_code)]
// logging.rs — Logging initialisation for Mouser.
//
// Configures the `log` facade with:
//  • A rotating file appender  (5 MB max, 5 backup files)
//  • stderr output in debug mode
//
// The file rotation is implemented manually: before opening the log file we
// check its size; if it exceeds the threshold we shift the backup files and
// create a fresh one.  This avoids pulling in a dedicated rotating-file crate.
//
// Format: "[LEVEL YYYY-MM-DD HH:MM:SS] message"

use anyhow::{Context, Result};
use log::{Level, LevelFilter, Log, Metadata, Record};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::log_dir;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_BYTES: u64 = 5 * 1024 * 1024; // 5 MB
const BACKUP_COUNT: u32 = 5;

// ---------------------------------------------------------------------------
// Timestamp helper (no chrono dependency)
// ---------------------------------------------------------------------------

/// Format the current UTC time as "YYYY-MM-DD HH:MM:SS".
fn utc_timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Simple manual conversion (no chrono needed for this format)
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400; // days since 1970-01-01

    // Compute year / month / day from days
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        year, month, day, h, m, s
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    // Algorithm: http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ---------------------------------------------------------------------------
// File rotation
// ---------------------------------------------------------------------------

/// Shift backups: `mouser.log.5` is removed, `.4` → `.5`, …, `.1` → `.2`,
/// then `mouser.log` → `mouser.log.1`.
fn rotate_files(log_path: &Path) {
    // Remove the oldest backup
    let oldest = log_path.with_extension(format!("log.{}", BACKUP_COUNT));
    let _ = fs::remove_file(&oldest);

    // Shift backups down
    for i in (1..BACKUP_COUNT).rev() {
        let from = log_path.with_extension(format!("log.{}", i));
        let to = log_path.with_extension(format!("log.{}", i + 1));
        let _ = fs::rename(&from, &to);
    }

    // mouser.log → mouser.log.1
    let backup1 = log_path.with_extension("log.1");
    let _ = fs::rename(log_path, &backup1);
}

/// Open the log file, rotating first if it exceeds `MAX_BYTES`.
fn open_log_file(log_path: &Path) -> Result<File> {
    if log_path.exists() {
        let size = log_path.metadata().map(|m| m.len()).unwrap_or(0);
        if size >= MAX_BYTES {
            rotate_files(log_path);
        }
    }

    OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("Cannot open log file {}", log_path.display()))
}

// ---------------------------------------------------------------------------
// Logger implementation
// ---------------------------------------------------------------------------

struct MouserLogger {
    file: Mutex<Option<File>>,
    log_path: PathBuf,
    debug: bool,
}

impl Log for MouserLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= if self.debug { Level::Debug } else { Level::Info }
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let ts = utc_timestamp();
        let line = format!(
            "[{level} {ts}] {msg}\n",
            level = record.level(),
            ts = ts,
            msg = record.args()
        );

        // Write to file
        {
            let mut guard = self.file.lock().unwrap();
            if let Some(ref mut f) = *guard {
                let _ = f.write_all(line.as_bytes());
                // Check size; if over limit, rotate and re-open
                let size = f.metadata().map(|m| m.len()).unwrap_or(0);
                if size >= MAX_BYTES {
                    drop(guard); // release lock before rotating
                    self.rotate_and_reopen();
                    return;
                }
            }
        }

        // Write to stderr in debug mode
        if self.debug {
            let _ = std::io::stderr().write_all(line.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(ref mut f) = *guard {
                let _ = f.flush();
            }
        }
    }
}

impl MouserLogger {
    fn rotate_and_reopen(&self) {
        let mut guard = self.file.lock().unwrap();
        // Drop the current file handle so the rename can succeed on Windows
        *guard = None;
        rotate_files(&self.log_path);
        if let Ok(f) = open_log_file(&self.log_path) {
            *guard = Some(f);
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialise logging.
///
/// * `debug` – when `true`, log at DEBUG level and also write to stderr.
///
/// Safe to call multiple times; subsequent calls are no-ops because
/// `log::set_logger` returns an error if a logger is already installed.
pub fn setup_logging(debug: bool) -> Result<()> {
    let dir = log_dir().context("Cannot determine log directory")?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("Cannot create log directory {}", dir.display()))?;

    // Restrictive permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }

    let log_path = dir.join("mouser.log");
    let file = open_log_file(&log_path).ok(); // failure is non-fatal

    let logger = Box::new(MouserLogger {
        file: Mutex::new(file),
        log_path,
        debug,
    });

    let level = if debug { LevelFilter::Debug } else { LevelFilter::Info };

    log::set_boxed_logger(logger)
        .map(|()| log::set_max_level(level))
        .context("Failed to set logger (already initialised?)")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utc_timestamp_format() {
        let ts = utc_timestamp();
        // Format: "YYYY-MM-DD HH:MM:SS" — 19 chars
        assert_eq!(ts.len(), 19, "unexpected timestamp length: {}", ts);
        // Basic sanity on separators
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], " ");
    }

    #[test]
    fn test_days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn test_days_to_ymd_known_date() {
        // 2024-03-15 is day 19797 since epoch (verified independently)
        let (y, m, d) = days_to_ymd(19797);
        assert_eq!((y, m, d), (2024, 3, 15));
    }

    #[test]
    fn test_rotate_files_noop_when_absent() {
        // Should not panic when the files don't exist
        let tmp = std::env::temp_dir().join("mouser_test_rotate.log");
        rotate_files(&tmp);
    }
}
