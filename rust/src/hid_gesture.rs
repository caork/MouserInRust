//! hid_gesture.rs — Logitech HID++ gesture / feature listener.
//!
//! Ports `core/hid_gesture.py` to Rust. Opens the Logitech vendor HID
//! interface (usage-page 0xFF00), discovers HID++ features via IRoot
//! (0x0000), diverts the best gesture-button CID, and dispatches events
//! back to the caller via `HidCallbacks`.
//!
//! The background thread communicates with the caller through
//! `std::sync::mpsc` channels (commands in, responses out).

#![allow(dead_code)]

#[cfg(target_os = "macos")]
#[path = "mac_hid.rs"]
mod mac_hid;

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice};
use log::{debug, info, warn};

// ── Protocol constants ────────────────────────────────────────────────────────

pub const LOGITECH_VID: u16 = 0x046D;

/// PIDs of Bolt / Unifying / Nano wireless receivers.
pub const WIRELESS_RECEIVER_PIDS: &[u16] = &[
    0xC52B, // Unifying receiver
    0xC53D, // Nano receiver
    0xC539, // Nano receiver (alt)
    0xC534, // Nano receiver 2
    0xC52D, // Nano receiver 3
    0xC545, // Bolt receiver
    0xC547, // Bolt receiver (alt)
    0xC548, // Bolt receiver (alt2)
];

const SHORT_REPORT_ID: u8 = 0x10;
const LONG_REPORT_ID: u8 = 0x11;
const SHORT_LEN: usize = 7;
const LONG_LEN: usize = 20;

/// Device index for direct BT / BLE connections.
const BT_DEV_IDX: u8 = 0xFF;

/// Software-ID embedded in all our outgoing requests (lower nibble of byte 3).
const MY_SW: u8 = 0x0A;

/// HID++ feature IDs used by Mouser.
const FEAT_IROOT: u16 = 0x0000;
const FEAT_REPROG_V4: u16 = 0x1B04;
const FEAT_ADJ_DPI: u16 = 0x2201;
const FEAT_SMART_SHIFT: u16 = 0x2110;
const FEAT_UNIFIED_BATT: u16 = 0x1004;
const FEAT_BATTERY_STATUS: u16 = 0x1000;

/// Default gesture CIDs tried in priority order.
const DEFAULT_GESTURE_CIDS: &[u16] = &[0x00C3, 0x00D7];

const SMART_SHIFT_FREESPIN: u8 = 0x01;
const SMART_SHIFT_RATCHET: u8 = 0x02;

/// HID++ vendor usage-page that every HID++ collection sits under.
const HIDPP_USAGE_PAGE: u16 = 0xFF00;

// ── HID++ error code names (for logging) ─────────────────────────────────────

fn hidpp_error_name(code: u8) -> &'static str {
    match code {
        0x01 => "UNKNOWN",
        0x02 => "INVALID_ARGUMENT",
        0x03 => "OUT_OF_RANGE",
        0x04 => "HARDWARE_ERROR",
        0x05 => "LOGITECH_ERROR",
        0x06 => "INVALID_FEATURE_INDEX",
        0x07 => "INVALID_FUNCTION",
        0x08 => "BUSY",
        0x09 => "UNSUPPORTED",
        _ => "?",
    }
}

// ── Key / mapping flag helpers (for debug logging) ────────────────────────────

fn format_key_flags(v: u16) -> String {
    const BITS: &[(u16, &str)] = &[
        (0x0001, "mse"),
        (0x0002, "fn"),
        (0x0004, "nonstandard"),
        (0x0008, "fn_sensitive"),
        (0x0010, "reprogrammable"),
        (0x0020, "divertable"),
        (0x0040, "persist_divertable"),
        (0x0080, "virtual"),
        (0x0100, "raw_xy"),
        (0x0200, "force_raw_xy"),
        (0x0400, "analytics"),
        (0x0800, "raw_wheel"),
    ];
    let names: Vec<&str> = BITS.iter().filter(|(b, _)| v & b != 0).map(|(_, n)| *n).collect();
    if names.is_empty() { "none".into() } else { names.join(",") }
}

fn format_mapping_flags(v: u16) -> String {
    const BITS: &[(u16, &str)] = &[
        (0x0001, "diverted"),
        (0x0004, "persist_diverted"),
        (0x0010, "raw_xy_diverted"),
        (0x0040, "force_raw_xy_diverted"),
        (0x0100, "analytics_reporting"),
        (0x0400, "raw_wheel"),
    ];
    let names: Vec<&str> = BITS.iter().filter(|(b, _)| v & b != 0).map(|(_, n)| *n).collect();
    if names.is_empty() { "none".into() } else { names.join(",") }
}

fn format_cid(cid: u16) -> String {
    match cid {
        0x00C3 => format!("0x{cid:04X} (Mouse Gesture Button)"),
        0x00C4 => format!("0x{cid:04X} (Smart Shift)"),
        0x00D7 => format!("0x{cid:04X} (Virtual Gesture Button)"),
        _ => format!("0x{cid:04X}"),
    }
}

// ── Parsed HID++ message ──────────────────────────────────────────────────────

/// Parsed fields of one incoming HID++ report.
#[derive(Debug, Clone)]
struct HidppMsg {
    dev_idx: u8,
    feat_idx: u8,
    func: u8,
    sw: u8,
    /// Bytes starting at offset 4 (after the fixed header).
    params: Vec<u8>,
}

/// Parse a raw HID read buffer into an `HidppMsg`.
///
/// The hidapi C backend strips the report-ID byte on Windows; on other
/// platforms the report-ID is kept.  We detect by checking whether byte 0
/// is a valid HID++ report-ID.
fn parse_report(raw: &[u8]) -> Option<HidppMsg> {
    if raw.len() < 4 {
        return None;
    }
    let off = if raw[0] == SHORT_REPORT_ID || raw[0] == LONG_REPORT_ID {
        1usize
    } else {
        0usize
    };
    if off + 3 > raw.len() {
        return None;
    }
    let dev_idx = raw[off];
    let feat_idx = raw[off + 1];
    let fsw = raw[off + 2];
    let func = (fsw >> 4) & 0x0F;
    let sw = fsw & 0x0F;
    let params = raw[off + 3..].to_vec();
    Some(HidppMsg { dev_idx, feat_idx, func, sw, params })
}

/// Decode a big-endian signed 16-bit value from two bytes.
#[inline]
fn decode_s16(hi: u8, lo: u8) -> i16 {
    let v = (hi as u16) << 8 | lo as u16;
    v as i16
}

// ── Device info ───────────────────────────────────────────────────────────────

/// Minimal metadata about one candidate Logitech HID interface.
#[derive(Debug, Clone)]
pub struct HidDeviceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub path: Vec<u8>,
    pub product_string: String,
}

/// Enumerate all HID devices for Logitech VID with usage-page 0xFF00.
pub fn vendor_hid_infos() -> Vec<HidDeviceInfo> {
    let api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            warn!("[HidGesture] HidApi::new failed: {e}");
            return Vec::new();
        }
    };
    api.device_list()
        .filter(|d| d.vendor_id() == LOGITECH_VID && d.usage_page() >= HIDPP_USAGE_PAGE)
        .map(|d| HidDeviceInfo {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            path: d.path().to_bytes().to_vec(),
            product_string: d.product_string().unwrap_or("").to_string(),
        })
        .collect()
}

// ── Backend selector ──────────────────────────────────────────────────────────

/// Which HID backend to use.
///
/// On macOS, `Auto` tries the native IOKit backend first (which receives all
/// HID report types including HID++ 0x10/0x11), then falls back to `hidapi`.
/// `IOKit` forces the native backend; `Hidapi` forces the hidapi crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HidBackend {
    Auto,
    Hidapi,
    /// Native macOS IOKit backend (receives HID++ reports on BLE devices).
    IOKit,
}

impl Default for HidBackend {
    fn default() -> Self {
        HidBackend::Auto
    }
}

// ── HID device abstraction ────────────────────────────────────────────────────

/// Wrapper that abstracts over `hidapi::HidDevice` and the native macOS
/// `MacNativeHidDevice`.  The Worker uses this so the rest of the code is
/// backend-agnostic.
enum HidDeviceWrapper {
    Hidapi(HidDevice),
    #[cfg(target_os = "macos")]
    Native(mac_hid::MacNativeHidDevice),
    /// Hybrid: writes go through `hidapi` (UP=0xFF43, HID++ works) while
    /// reads come from the native IOKit callback reader (UP=0x0001, receives
    /// ALL report types including async HID++ notifications on BLE).
    #[cfg(target_os = "macos")]
    Hybrid {
        writer: HidDevice,
        reader: mac_hid::MacNativeHidDevice,
    },
}

impl HidDeviceWrapper {
    fn write(&self, data: &[u8]) -> Result<usize, String> {
        match self {
            Self::Hidapi(dev) => dev.write(data).map_err(|e| e.to_string()),
            #[cfg(target_os = "macos")]
            Self::Native(dev) => dev.write(data),
            #[cfg(target_os = "macos")]
            Self::Hybrid { writer, .. } => writer.write(data).map_err(|e| e.to_string()),
        }
    }

    fn read_timeout(&self, buf: &mut [u8], timeout_ms: i32) -> Result<usize, String> {
        match self {
            Self::Hidapi(dev) => dev.read_timeout(buf, timeout_ms).map_err(|e| e.to_string()),
            #[cfg(target_os = "macos")]
            Self::Native(dev) => dev.read_timeout(buf, timeout_ms),
            #[cfg(target_os = "macos")]
            Self::Hybrid { reader, .. } => reader.read_timeout(buf, timeout_ms),
        }
    }
}

// ── Commands sent to the background thread ────────────────────────────────────

#[derive(Debug)]
enum Cmd {
    SetDpi(u16),
    ReadDpi,
    SetSmartShift(SmartShiftMode),
    ReadSmartShift,
    ReadBattery,
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartShiftMode {
    Ratchet,
    FreeSpin,
}

impl SmartShiftMode {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "ratchet" => Some(Self::Ratchet),
            "freespin" => Some(Self::FreeSpin),
            _ => None,
        }
    }

    fn as_byte(self) -> u8 {
        match self {
            Self::FreeSpin => SMART_SHIFT_FREESPIN,
            Self::Ratchet => SMART_SHIFT_RATCHET,
        }
    }
}

// ── Event callbacks ───────────────────────────────────────────────────────────

/// All event callbacks the caller can register on `HidGestureListener`.
///
/// Every field is an `Option` so callers need only set the callbacks they care
/// about.  All closures must be `Send` so they can be moved into the background
/// thread.
pub struct HidCallbacks {
    /// Gesture button pressed.
    pub on_gesture_down: Option<Box<dyn Fn() + Send>>,
    /// Gesture button released.
    pub on_gesture_up: Option<Box<dyn Fn() + Send>>,
    /// Raw XY movement while gesture is held.
    pub on_gesture_move: Option<Box<dyn Fn(i16, i16) + Send>>,
    /// Device connected (carries a human-readable info string).
    pub on_device_connected: Option<Box<dyn Fn(String) + Send>>,
    /// Device disconnected.
    pub on_device_disconnected: Option<Box<dyn Fn() + Send>>,
    /// Battery percentage update (0-100).
    pub on_battery: Option<Box<dyn Fn(u8) + Send>>,
    /// DPI changed.
    pub on_dpi_changed: Option<Box<dyn Fn(u16) + Send>>,
    /// Mode-shift button pressed.
    pub on_mode_shift_down: Option<Box<dyn Fn() + Send>>,
    /// Mode-shift button released.
    pub on_mode_shift_up: Option<Box<dyn Fn() + Send>>,
}

impl Default for HidCallbacks {
    fn default() -> Self {
        Self {
            on_gesture_down: None,
            on_gesture_up: None,
            on_gesture_move: None,
            on_device_connected: None,
            on_device_disconnected: None,
            on_battery: None,
            on_dpi_changed: None,
            on_mode_shift_down: None,
            on_mode_shift_up: None,
        }
    }
}

// ── Per-control record discovered via REPROG_V4 ───────────────────────────────

#[derive(Debug, Clone)]
struct ControlInfo {
    index: u8,
    cid: u16,
    task: u16,
    key_flags: u16,
    pos: u8,
    group: u8,
    gmask: u8,
    mapping_flags: u16,
}

impl ControlInfo {
    fn raw_xy_capable(&self) -> bool {
        self.key_flags & 0x0100 != 0
            || self.key_flags & 0x0200 != 0
            || self.mapping_flags & 0x0010 != 0
            || self.mapping_flags & 0x0040 != 0
    }

    fn is_divertable(&self) -> bool {
        self.key_flags & 0x0020 != 0
    }
}

// ── Worker: the actual HID++ state machine running on the background thread ───

/// Internal state that lives on the background thread.
struct Worker {
    dev: Option<HidDeviceWrapper>,
    dev_idx: u8,
    feat_idx: Option<u8>,       // REPROG_V4
    dpi_idx: Option<u8>,        // ADJUST_DPI
    smart_shift_idx: Option<u8>, // SMART_SHIFT
    battery_idx: Option<u8>,
    battery_is_unified: bool,
    gesture_cid: u16,
    gesture_candidates: Vec<u16>,
    rawxy_enabled: bool,
    held: bool,                 // gesture button currently held
    mode_shift_held: bool,
    mode_shift_cid: Option<u16>,
    callbacks: HidCallbacks,
    cmd_rx: Receiver<Cmd>,
    backend: HidBackend,
}

impl Worker {
    fn new(callbacks: HidCallbacks, cmd_rx: Receiver<Cmd>, backend: HidBackend) -> Self {
        Self {
            dev: None,
            dev_idx: BT_DEV_IDX,
            feat_idx: None,
            dpi_idx: None,
            smart_shift_idx: None,
            battery_idx: None,
            battery_is_unified: false,
            gesture_cid: DEFAULT_GESTURE_CIDS[0],
            gesture_candidates: DEFAULT_GESTURE_CIDS.to_vec(),
            rawxy_enabled: false,
            held: false,
            mode_shift_held: false,
            mode_shift_cid: Some(0x00C4), // Smart Shift CID doubles as mode-shift
            callbacks,
            cmd_rx,
            backend,
        }
    }

    // ── Low-level I/O ─────────────────────────────────────────────────────────

    /// Send a long HID++ output report (20 bytes, always LONG_ID for BLE compat).
    fn tx(&self, feat: u8, func: u8, params: &[u8]) -> bool {
        let dev = match self.dev.as_ref() {
            Some(d) => d,
            None => return false,
        };

        // On macOS (IOKit backend) the first byte of the write buffer is the
        // HID report ID.  BLE HID++ devices only accept report IDs 0x10
        // (short, 7 bytes) or 0x11 (long, 20 bytes).  Sending 0x00 causes
        // IOHIDDeviceSetReport to fail with 0xE00002F0.
        //
        // We try long report first (0x11); if the device rejects it we fall
        // back to a short report (0x10).
        let mut buf = [0u8; LONG_LEN]; // 20 bytes: reportId + devIdx + feat + funcSw + 16 params
        buf[0] = LONG_REPORT_ID;       // 0x11
        buf[1] = self.dev_idx;
        buf[2] = feat;
        buf[3] = ((func & 0x0F) << 4) | (MY_SW & 0x0F);
        for (i, &b) in params.iter().enumerate() {
            let pos = 4 + i;
            if pos < buf.len() {
                buf[pos] = b;
            }
        }
        match dev.write(&buf) {
            Ok(_) => true,
            Err(_) => {
                // Fallback: try short report (0x10, 7 bytes)
                let mut short = [0u8; SHORT_LEN]; // 7 bytes
                short[0] = SHORT_REPORT_ID; // 0x10
                short[1] = self.dev_idx;
                short[2] = feat;
                short[3] = ((func & 0x0F) << 4) | (MY_SW & 0x0F);
                for (i, &b) in params.iter().enumerate() {
                    let pos = 4 + i;
                    if pos < short.len() {
                        short[pos] = b;
                    }
                }
                match dev.write(&short) {
                    Ok(_) => true,
                    Err(e) => {
                        warn!("[HidGesture] write error: {e}");
                        false
                    }
                }
            }
        }
    }

    /// Read one HID++ input report with the given timeout.
    fn rx(&self, timeout_ms: i32) -> Option<Vec<u8>> {
        let dev = self.dev.as_ref()?;
        let mut buf = [0u8; 64];
        match dev.read_timeout(&mut buf, timeout_ms) {
            Ok(0) => None,
            Ok(n) => Some(buf[..n].to_vec()),
            Err(e) => {
                debug!("[HidGesture] read error: {}", e);
                None
            }
        }
    }

    /// Send a long HID++ request and wait for the matching response.
    fn request(&self, feat: u8, func: u8, params: &[u8], timeout_ms: u64) -> Option<HidppMsg> {
        if !self.tx(feat, func, params) {
            return None;
        }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let slice = remaining.min(Duration::from_millis(500));
            let raw = match self.rx(slice.as_millis() as i32) {
                Some(r) => r,
                None => continue,
            };
            let msg = match parse_report(&raw) {
                Some(m) => m,
                None => continue,
            };

            // HID++ error (feature-index 0xFF)
            if msg.feat_idx == 0xFF {
                let code = msg.params.get(1).copied().unwrap_or(0);
                warn!(
                    "[HidGesture] HID++ error 0x{code:02X} ({}) feat=0x{feat:02X} func=0x{func:X}",
                    hidpp_error_name(code)
                );
                return None;
            }

            let expected = [func, (func.wrapping_add(1)) & 0x0F];
            if msg.feat_idx == feat && msg.sw == MY_SW && expected.contains(&msg.func) {
                return Some(msg);
            }
        }
        warn!("[HidGesture] request timeout feat=0x{feat:02X} func=0x{func:X} devIdx=0x{:02X}", self.dev_idx);
        None
    }

    // ── Feature discovery ─────────────────────────────────────────────────────

    /// Ask IRoot (feature 0) for the index of `feature_id`.
    fn find_feature(&self, feature_id: u16) -> Option<u8> {
        let hi = (feature_id >> 8) as u8;
        let lo = feature_id as u8;
        let resp = self.request(0x00, 0, &[hi, lo, 0x00], 2000)?;
        let idx = resp.params.first().copied()?;
        if idx != 0 { Some(idx) } else { None }
    }

    // ── REPROG_CONTROLS_V4 helpers ────────────────────────────────────────────

    fn get_cid_reporting(&self, cid: u16) -> Option<HidppMsg> {
        let fi = self.feat_idx?;
        let hi = (cid >> 8) as u8;
        let lo = cid as u8;
        self.request(fi, 2, &[hi, lo], 2000)
    }

    fn set_cid_reporting(&self, cid: u16, flags: u8) -> Option<HidppMsg> {
        let fi = self.feat_idx?;
        let hi = (cid >> 8) as u8;
        let lo = cid as u8;
        self.request(fi, 3, &[hi, lo, flags, 0x00, 0x00], 2000)
    }

    fn discover_reprog_controls(&self) -> Vec<ControlInfo> {
        let fi = match self.feat_idx {
            Some(f) => f,
            None => return Vec::new(),
        };
        let count_resp = match self.request(fi, 0, &[], 2000) {
            Some(r) => r,
            None => {
                warn!("[HidGesture] Failed to read REPROG_V4 control count");
                return Vec::new();
            }
        };
        let raw_count = count_resp.params.first().copied().unwrap_or(0);
        let count = raw_count.min(32);
        info!("[HidGesture] REPROG_V4 exposes {count} controls");

        let mut controls = Vec::with_capacity(count as usize);
        let mut consecutive_failures = 0u8;

        for index in 0..count {
            let key_resp = match self.request(fi, 1, &[index], 500) {
                Some(r) => r,
                None => {
                    consecutive_failures += 1;
                    warn!("[HidGesture] Failed to read control info for index {index}");
                    if consecutive_failures >= 3 {
                        warn!("[HidGesture] {consecutive_failures} consecutive failures, aborting");
                        break;
                    }
                    continue;
                }
            };
            consecutive_failures = 0;

            let p = &key_resp.params;
            if p.len() < 9 {
                warn!("[HidGesture] Short control info for index {index}");
                continue;
            }

            let cid = (p[0] as u16) << 8 | p[1] as u16;
            let task = (p[2] as u16) << 8 | p[3] as u16;
            let key_flags = p[4] as u16 | ((p[8] as u16) << 8);
            let pos = p[5];
            let group = p[6];
            let gmask = p[7];

            // Read current mapping flags
            let mapping_flags = self
                .get_cid_reporting(cid)
                .and_then(|mr| {
                    let mp = &mr.params;
                    if mp.len() >= 5 {
                        let mut mf = mp[2] as u16;
                        if mp.len() >= 6 {
                            mf |= (mp[5] as u16) << 8;
                        }
                        Some(mf)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);

            info!(
                "[HidGesture] Control idx={index} cid={} task=0x{task:04X} \
                 flags=0x{key_flags:04X}[{}] group={group} gmask=0x{gmask:02X} pos={pos} \
                 reporting=0x{mapping_flags:04X}[{}]",
                format_cid(cid),
                format_key_flags(key_flags),
                format_mapping_flags(mapping_flags),
            );

            controls.push(ControlInfo { index, cid, task, key_flags, pos, group, gmask, mapping_flags });
        }
        controls
    }

    fn choose_gesture_candidates(&self, controls: &[ControlInfo]) -> Vec<u16> {
        let present: std::collections::HashSet<u16> = controls.iter().map(|c| c.cid).collect();
        let mut ordered: Vec<u16> = Vec::new();

        let mut add = |cid: u16| {
            if present.contains(&cid) && !ordered.contains(&cid) {
                ordered.push(cid);
            }
        };

        // Preferred defaults first
        for &cid in DEFAULT_GESTURE_CIDS {
            add(cid);
        }

        // Then any divertable, raw-XY-capable, virtual or named-gesture control
        for ctrl in controls {
            let virtual_or_named = ctrl.key_flags & 0x0080 != 0
                || matches!(ctrl.cid, 0x00C3 | 0x00D7);
            if ctrl.raw_xy_capable() && virtual_or_named && ctrl.is_divertable() {
                add(ctrl.cid);
            }
        }

        if ordered.is_empty() {
            DEFAULT_GESTURE_CIDS.to_vec()
        } else {
            ordered
        }
    }

    fn divert(&mut self) -> bool {
        if self.feat_idx.is_none() {
            return false;
        }
        let candidates = self.gesture_candidates.clone();
        for cid in &candidates {
            let cid = *cid;
            // Try with RawXY first (flags=0x33: diverted|persist|raw_xy|force_raw_xy)
            if self.set_cid_reporting(cid, 0x33).is_some() {
                self.gesture_cid = cid;
                self.rawxy_enabled = true;
                info!("[HidGesture] Divert {} with RawXY: OK", format_cid(cid));
                return true;
            }
            // Fallback: divert-only (flags=0x03: diverted|persist)
            if self.set_cid_reporting(cid, 0x03).is_some() {
                self.gesture_cid = cid;
                self.rawxy_enabled = false;
                info!("[HidGesture] Divert {}: OK", format_cid(cid));
                return true;
            }
            warn!("[HidGesture] Divert {}: FAILED", format_cid(cid));
        }
        self.gesture_cid = DEFAULT_GESTURE_CIDS[0];
        false
    }

    fn divert_mode_shift(&mut self) {
        if let Some(cid) = self.mode_shift_cid {
            if self.set_cid_reporting(cid, 0x03).is_some() {
                info!("[HidGesture] Mode-shift divert {}: OK", format_cid(cid));
            } else {
                warn!("[HidGesture] Mode-shift divert {}: FAILED", format_cid(cid));
            }
        }
    }

    fn undivert(&self) {
        if self.feat_idx.is_none() || self.dev.is_none() {
            return;
        }
        let fi = self.feat_idx.unwrap();
        // Undivert mode-shift CID
        if let Some(cid) = self.mode_shift_cid {
            let hi = (cid >> 8) as u8;
            let lo = cid as u8;
            self.tx(fi, 3, &[hi, lo, 0x02, 0x00, 0x00]);
        }
        // Undivert gesture CID
        let cid = self.gesture_cid;
        let hi = (cid >> 8) as u8;
        let lo = cid as u8;
        let flags = if self.rawxy_enabled { 0x22u8 } else { 0x02u8 };
        self.tx(fi, 3, &[hi, lo, flags, 0x00, 0x00]);
    }

    // ── Incoming report dispatch ──────────────────────────────────────────────

    fn on_report(&mut self, raw: &[u8]) {
        let msg = match parse_report(raw) {
            Some(m) => m,
            None => {
                debug!("[HidGesture] on_report: parse_report returned None for {} bytes", raw.len());
                return;
            }
        };

        debug!(
            "[HidGesture] on_report: devIdx=0x{:02X} featIdx=0x{:02X} func={} (expected featIdx={:?})",
            msg.dev_idx, msg.feat_idx, msg.func, self.feat_idx
        );

        if Some(msg.feat_idx) != self.feat_idx {
            return;
        }

        match msg.func {
            // func=1: RawXY movement event
            1 => {
                if !self.rawxy_enabled || !self.held {
                    return;
                }
                let p = &msg.params;
                if p.len() < 4 {
                    return;
                }
                let dx = decode_s16(p[0], p[1]);
                let dy = decode_s16(p[2], p[3]);
                if dx != 0 || dy != 0 {
                    if let Some(cb) = &self.callbacks.on_gesture_move {
                        cb(dx, dy);
                    }
                }
            }

            // func=0: diverted button state update
            0 => {
                // Collect currently-pressed CIDs from sequential pairs
                let mut active_cids = std::collections::HashSet::<u16>::new();
                let p = &msg.params;
                let mut i = 0;
                while i + 1 < p.len() {
                    let c = (p[i] as u16) << 8 | p[i + 1] as u16;
                    if c == 0 {
                        break;
                    }
                    active_cids.insert(c);
                    i += 2;
                }

                // Gesture button
                let gesture_now = active_cids.contains(&self.gesture_cid);
                if gesture_now && !self.held {
                    self.held = true;
                    debug!("[HidGesture] Gesture DOWN");
                    if let Some(cb) = &self.callbacks.on_gesture_down {
                        cb();
                    }
                } else if !gesture_now && self.held {
                    self.held = false;
                    debug!("[HidGesture] Gesture UP");
                    if let Some(cb) = &self.callbacks.on_gesture_up {
                        cb();
                    }
                }

                // Mode-shift button
                if let Some(ms_cid) = self.mode_shift_cid {
                    let ms_now = active_cids.contains(&ms_cid);
                    if ms_now && !self.mode_shift_held {
                        self.mode_shift_held = true;
                        debug!("[HidGesture] Mode-shift DOWN");
                        if let Some(cb) = &self.callbacks.on_mode_shift_down {
                            cb();
                        }
                    } else if !ms_now && self.mode_shift_held {
                        self.mode_shift_held = false;
                        debug!("[HidGesture] Mode-shift UP");
                        if let Some(cb) = &self.callbacks.on_mode_shift_up {
                            cb();
                        }
                    }
                }
            }

            _ => {}
        }
    }

    // ── DPI helpers ───────────────────────────────────────────────────────────

    fn apply_set_dpi(&self, dpi: u16) -> bool {
        let fi = match self.dpi_idx {
            Some(f) => f,
            None => {
                warn!("[HidGesture] Cannot set DPI — not connected / feature absent");
                return false;
            }
        };
        let hi = (dpi >> 8) as u8;
        let lo = dpi as u8;
        // setSensorDpi: function 3, params [sensorIdx=0, dpi_hi, dpi_lo]
        match self.request(fi, 3, &[0x00, hi, lo], 2000) {
            Some(resp) => {
                let p = &resp.params;
                let actual = if p.len() >= 3 {
                    (p[1] as u16) << 8 | p[2] as u16
                } else {
                    dpi
                };
                info!("[HidGesture] DPI set to {actual}");
                if let Some(cb) = &self.callbacks.on_dpi_changed {
                    cb(actual);
                }
                true
            }
            None => {
                warn!("[HidGesture] DPI set FAILED");
                false
            }
        }
    }

    fn apply_read_dpi(&self) {
        let fi = match self.dpi_idx {
            Some(f) => f,
            None => return,
        };
        // getSensorDpi: function 2, params [sensorIdx=0]
        match self.request(fi, 2, &[0x00], 2000) {
            Some(resp) => {
                let p = &resp.params;
                if p.len() >= 3 {
                    let current = (p[1] as u16) << 8 | p[2] as u16;
                    info!("[HidGesture] Current DPI = {current}");
                    if let Some(cb) = &self.callbacks.on_dpi_changed {
                        cb(current);
                    }
                }
            }
            None => warn!("[HidGesture] DPI read FAILED"),
        }
    }

    // ── Smart Shift helpers ───────────────────────────────────────────────────

    fn apply_set_smart_shift(&self, mode: SmartShiftMode) {
        let fi = match self.smart_shift_idx {
            Some(f) => f,
            None => {
                warn!("[HidGesture] Cannot set Smart Shift — not connected / feature absent");
                return;
            }
        };
        // setRatchetControlMode: function 1, params [mode, autoDisengage=0, 0]
        match self.request(fi, 1, &[mode.as_byte(), 0x00, 0x00], 2000) {
            Some(_) => info!("[HidGesture] Smart Shift set to {mode:?}"),
            None => warn!("[HidGesture] Smart Shift set FAILED"),
        }
    }

    fn apply_read_smart_shift(&self) {
        let fi = match self.smart_shift_idx {
            Some(f) => f,
            None => return,
        };
        // getRatchetControlMode: function 0
        match self.request(fi, 0, &[], 2000) {
            Some(resp) => {
                let mode_byte = resp.params.first().copied().unwrap_or(0);
                let mode = if mode_byte == SMART_SHIFT_FREESPIN { "freespin" } else { "ratchet" };
                info!("[HidGesture] Smart Shift mode = {mode}");
            }
            None => warn!("[HidGesture] Smart Shift read FAILED"),
        }
    }

    // ── Battery helpers ───────────────────────────────────────────────────────

    fn apply_read_battery(&self) {
        let fi = match self.battery_idx {
            Some(f) => f,
            None => return,
        };
        let func = if self.battery_is_unified { 1u8 } else { 0u8 };
        match self.request(fi, func, &[], 2000) {
            Some(resp) => {
                if let Some(&level) = resp.params.first() {
                    if level <= 100 {
                        info!("[HidGesture] Battery: {level}%");
                        if let Some(cb) = &self.callbacks.on_battery {
                            cb(level);
                        }
                    }
                }
            }
            None => warn!("[HidGesture] Battery read FAILED"),
        }
    }

    // ── Connect ───────────────────────────────────────────────────────────────

    fn try_connect(&mut self) -> bool {
        #[cfg(target_os = "macos")]
        {
            if self.backend == HidBackend::Auto {
                // Hybrid approach: hidapi for writes (HID++ via UP=0xFF43) +
                // IOKit native for reads (ALL reports via UP=0x0001 BLE).
                // This solves the BLE issue where hidapi doesn't deliver async
                // HID++ notification reports.
                if self.try_connect_hybrid() {
                    return true;
                }
                info!("[HidGesture] Hybrid connect failed, trying pure native IOKit");
            }

            if self.backend != HidBackend::Hidapi {
                if self.try_connect_native() {
                    return true;
                }
                if self.backend == HidBackend::IOKit {
                    // User explicitly requested IOKit; don't fall back
                    return false;
                }
                info!("[HidGesture] Native IOKit backend failed, falling back to hidapi");
            }
        }

        self.try_connect_hidapi()
    }

    /// Hybrid connect: use `hidapi` for writes (HID++ via UP=0xFF43 interface)
    /// and IOKit native for reads (via UP=0x0001 BLE interface that receives
    /// ALL report types including async HID++ notifications).
    ///
    /// This solves the macOS BLE issue where `hidapi`'s `read_timeout` only
    /// delivers mouse reports (0x02) but not HID++ notifications (0x10/0x11),
    /// while IOKit native on UP=0x0001 receives everything but can't write
    /// HID++ commands (returns UNSUPPORTED).
    #[cfg(target_os = "macos")]
    fn try_connect_hybrid(&mut self) -> bool {
        // Step 1: Open hidapi writer (UP >= 0xFF00, the HID++ interface)
        let hidapi_infos = vendor_hid_infos();
        if hidapi_infos.is_empty() {
            debug!("[HidGesture] Hybrid: no hidapi HID++ interfaces found");
            return false;
        }

        let api = match HidApi::new() {
            Ok(a) => a,
            Err(e) => {
                warn!("[HidGesture] Hybrid: HidApi::new: {e}");
                return false;
            }
        };

        // Try each hidapi candidate as the writer
        for info in &hidapi_infos {
            info!(
                "[HidGesture] Hybrid: trying hidapi writer PID=0x{:04X} UP=0x{:04X} product={}",
                info.product_id, info.usage_page, info.product_string
            );

            let path = match std::ffi::CString::new(info.path.clone()) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let writer = match api.open_path(&path) {
                Ok(dev) => dev,
                Err(e) => {
                    warn!("[HidGesture] Hybrid: can't open hidapi writer: {e}");
                    continue;
                }
            };

            // Step 2: Find and open the IOKit native BLE reader (UP=0x0001)
            // for the same physical device (matching ProductID).
            let ble_infos = mac_hid::MacHidEnumerator::enumerate_ble(LOGITECH_VID);
            let reader_info = ble_infos.into_iter().find(|bi| {
                bi.product_id == info.product_id && bi.usage_page == 0x0001
            });

            let reader_info = match reader_info {
                Some(ri) => ri,
                None => {
                    debug!(
                        "[HidGesture] Hybrid: no BLE UP=0x0001 reader for PID=0x{:04X}, skipping hybrid",
                        info.product_id
                    );
                    // Not a BLE device or no matching reader; skip hybrid for this device
                    continue;
                }
            };

            info!(
                "[HidGesture] Hybrid: opening IOKit reader PID=0x{:04X} UP=0x{:04X} transport={:?}",
                reader_info.product_id, reader_info.usage_page, reader_info.transport
            );

            let reader = match mac_hid::MacHidEnumerator::open(&reader_info) {
                Ok(dev) => dev,
                Err(e) => {
                    warn!("[HidGesture] Hybrid: can't open IOKit reader: {e}");
                    continue;
                }
            };

            // Step 3: Combine into Hybrid wrapper and try setup
            self.reset_feature_state();
            self.dev = Some(HidDeviceWrapper::Hybrid { writer, reader });
            info!(
                "[HidGesture] Hybrid: opened writer(hidapi UP=0x{:04X}) + reader(IOKit UP=0x{:04X}) for PID=0x{:04X}",
                info.usage_page, reader_info.usage_page, info.product_id
            );

            if self.try_setup_device(info.product_id, &info.product_string) {
                info!("[HidGesture] Hybrid connect SUCCESS for PID=0x{:04X}", info.product_id);
                return true;
            }

            // Setup failed; drop and try next
            if let Some(dev) = self.dev.take() {
                drop(dev);
            }
        }

        false
    }

    /// Connect using the native macOS IOKit backend.
    ///
    /// Tries multiple strategies:
    /// 1. Standard enumerate (USB / Bolt receiver — usage_page >= 0xFF00)
    /// 2. BLE direct open via `open_ble()` which KEEPS the IOHIDManager alive.
    ///    On macOS BLE, the manager must stay alive for the device ref to work.
    #[cfg(target_os = "macos")]
    fn try_connect_native(&mut self) -> bool {
        // Strategy 1: standard enumerate (USB / Bolt receiver)
        let infos = mac_hid::MacHidEnumerator::enumerate(LOGITECH_VID, HIDPP_USAGE_PAGE);
        if !infos.is_empty() {
            info!("[HidGesture] Native IOKit candidates: {}", infos.len());
            for info in &infos {
                info!(
                    "[HidGesture] Native candidate PID=0x{:04X} UP=0x{:04X} usage=0x{:04X} product={}",
                    info.product_id, info.usage_page, info.usage, info.product_string
                );
                self.reset_feature_state();
                match mac_hid::MacHidEnumerator::open(info) {
                    Ok(dev) => {
                        self.dev = Some(HidDeviceWrapper::Native(dev));
                        info!("[HidGesture] Native opened PID=0x{:04X}", info.product_id);
                    }
                    Err(e) => {
                        warn!("[HidGesture] Native can't open PID=0x{:04X}: {e}", info.product_id);
                        continue;
                    }
                }
                if self.try_setup_device(info.product_id, &info.product_string) {
                    return true;
                }
                if let Some(dev) = self.dev.take() { drop(dev); }
            }
        }

        // Strategy 2: BLE direct open — uses open_ble() which creates a
        // FRESH IOHIDManager with VendorID+ProductID+Transport matching
        // and KEEPS the manager alive for the device lifetime.
        // This mirrors Python's _MacNativeHidDevice.open() approach.
        debug!("[HidGesture] Native: no USB/Bolt HID++ devices, trying BLE open_ble...");
        let ble_infos = mac_hid::MacHidEnumerator::enumerate_ble(LOGITECH_VID);
        for info in &ble_infos {
            info!(
                "[HidGesture] Native BLE candidate PID=0x{:04X} product={}",
                info.product_id, info.product_string
            );
            self.reset_feature_state();
            match mac_hid::MacNativeHidDevice::open_ble(LOGITECH_VID, info.product_id) {
                Ok(dev) => {
                    info!("[HidGesture] Native BLE opened PID=0x{:04X} (manager kept alive)", info.product_id);
                    self.dev = Some(HidDeviceWrapper::Native(dev));
                }
                Err(e) => {
                    warn!("[HidGesture] Native BLE can't open PID=0x{:04X}: {e}", info.product_id);
                    continue;
                }
            }
            if self.try_setup_device(info.product_id, &info.product_string) {
                return true;
            }
            if let Some(dev) = self.dev.take() {
                drop(dev);
            }
        }
        false
    }

    /// Connect using the hidapi crate backend.
    fn try_connect_hidapi(&mut self) -> bool {
        let infos = vendor_hid_infos();
        if infos.is_empty() {
            return false;
        }
        info!("[HidGesture] Candidate HID interfaces: {}", infos.len());

        let api = match HidApi::new() {
            Ok(a) => a,
            Err(e) => {
                warn!("[HidGesture] HidApi::new: {e}");
                return false;
            }
        };

        for info in &infos {
            info!(
                "[HidGesture] Candidate PID=0x{:04X} UP=0x{:04X} usage=0x{:04X} product={}",
                info.product_id, info.usage_page, info.usage, info.product_string
            );

            self.reset_feature_state();

            let path = match std::ffi::CString::new(info.path.clone()) {
                Ok(p) => p,
                Err(_) => {
                    warn!("[HidGesture] Invalid device path, skipping");
                    continue;
                }
            };

            match api.open_path(&path) {
                Ok(dev) => {
                    self.dev = Some(HidDeviceWrapper::Hidapi(dev));
                    info!("[HidGesture] Opened PID=0x{:04X}", info.product_id);
                }
                Err(e) => {
                    warn!("[HidGesture] Can't open PID=0x{:04X}: {e}", info.product_id);
                    continue;
                }
            }

            if self.try_setup_device(info.product_id, &info.product_string) {
                return true;
            }

            // This interface didn't work; close and try next
            if let Some(dev) = self.dev.take() {
                drop(dev);
            }
        }
        false
    }

    /// Reset all feature state before trying a new device.
    fn reset_feature_state(&mut self) {
        self.feat_idx = None;
        self.dpi_idx = None;
        self.smart_shift_idx = None;
        self.battery_idx = None;
        self.battery_is_unified = false;
        self.gesture_cid = DEFAULT_GESTURE_CIDS[0];
        self.gesture_candidates = DEFAULT_GESTURE_CIDS.to_vec();
        self.rawxy_enabled = false;
    }

    /// After opening a device, probe for HID++ features, divert the gesture
    /// button, and fire the connected callback.  Returns true on success.
    fn try_setup_device(&mut self, product_id: u16, product_string: &str) -> bool {
        // Try BT direct (0xFF) and then Bolt receiver slots 1-6
        let mut found_reprog = false;
        for &idx in &[0xFFu8, 1, 2, 3, 4, 5, 6] {
            self.dev_idx = idx;
            if let Some(fi) = self.find_feature(FEAT_REPROG_V4) {
                self.feat_idx = Some(fi);
                info!(
                    "[HidGesture] Found REPROG_V4 @0x{fi:02X} PID=0x{product_id:04X} devIdx=0x{idx:02X}",
                );

                let controls = self.discover_reprog_controls();
                self.gesture_candidates = self.choose_gesture_candidates(&controls);
                info!(
                    "[HidGesture] Gesture CID candidates: {}",
                    self.gesture_candidates.iter().map(|&c| format_cid(c)).collect::<Vec<_>>().join(", ")
                );

                // ADJUST_DPI
                if let Some(dpi_fi) = self.find_feature(FEAT_ADJ_DPI) {
                    self.dpi_idx = Some(dpi_fi);
                    info!("[HidGesture] Found ADJUST_DPI @0x{dpi_fi:02X}");
                }
                // SMART_SHIFT
                if let Some(ss_fi) = self.find_feature(FEAT_SMART_SHIFT) {
                    self.smart_shift_idx = Some(ss_fi);
                    info!("[HidGesture] Found SMART_SHIFT @0x{ss_fi:02X}");
                }
                // Battery: prefer UNIFIED_BATT, fall back to BATTERY_STATUS
                if let Some(bfi) = self.find_feature(FEAT_UNIFIED_BATT) {
                    self.battery_idx = Some(bfi);
                    self.battery_is_unified = true;
                    info!("[HidGesture] Found UNIFIED_BATT @0x{bfi:02X}");
                } else if let Some(bfi) = self.find_feature(FEAT_BATTERY_STATUS) {
                    self.battery_idx = Some(bfi);
                    self.battery_is_unified = false;
                    info!("[HidGesture] Found BATTERY_STATUS @0x{bfi:02X}");
                }

                if self.divert() {
                    self.divert_mode_shift();
                    found_reprog = true;
                    break;
                }
                // Right device but divert failed -- no point trying other slots
                break;
            }
        }

        if found_reprog {
            let desc = format!(
                "PID=0x{product_id:04X} product=\"{product_string}\" devIdx=0x{:02X}",
                self.dev_idx
            );
            info!("[HidGesture] Connected: {desc}");
            let display_name = if product_string.is_empty() {
                format!("Logitech (0x{product_id:04X})")
            } else {
                product_string.to_string()
            };
            if let Some(cb) = &self.callbacks.on_device_connected {
                cb(display_name);
            }

            // Read battery and DPI on connect
            self.apply_read_battery();
            self.apply_read_dpi();
            return true;
        }

        false
    }

    // ── Main loop ─────────────────────────────────────────────────────────────

    fn run(&mut self) {
        let mut retry_logged = false;

        loop {
            // Check for Stop before spending time connecting
            if let Ok(Cmd::Stop) = self.cmd_rx.try_recv() {
                break;
            }

            if !self.try_connect() {
                if !retry_logged {
                    info!("[HidGesture] No compatible device; retrying in 5 s…");
                    retry_logged = true;
                }
                // Sleep 5 s in 100 ms slices so Stop is noticed quickly
                for _ in 0..50 {
                    if let Ok(Cmd::Stop) = self.cmd_rx.try_recv() {
                        return;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                continue;
            }
            retry_logged = false;
            info!("[HidGesture] Listening for gesture events…");

            // Inner read loop
            let disconnect = loop {
                // Drain pending commands
                loop {
                    match self.cmd_rx.try_recv() {
                        Ok(Cmd::Stop) => return,
                        Ok(Cmd::SetDpi(dpi)) => { self.apply_set_dpi(dpi); }
                        Ok(Cmd::ReadDpi) => self.apply_read_dpi(),
                        Ok(Cmd::SetSmartShift(mode)) => self.apply_set_smart_shift(mode),
                        Ok(Cmd::ReadSmartShift) => self.apply_read_smart_shift(),
                        Ok(Cmd::ReadBattery) => self.apply_read_battery(),
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }

                let raw = match self.rx(1000) {
                    Some(r) => r,
                    None => {
                        // Uncomment for deep debugging:
                        // debug!("[HidGesture] rx(1000) returned None (timeout)");
                        continue;
                    }
                };

                // Detect disconnect: hidapi typically returns 0 bytes or an error
                if raw.is_empty() {
                    break true;
                }
                // Debug: log raw reports to help diagnose gesture issues
                if raw.len() >= 4 {
                    debug!(
                        "[HidGesture] RAW report ({} bytes): {:02X?}",
                        raw.len(),
                        &raw[..std::cmp::min(raw.len(), 20)]
                    );
                }
                self.on_report(&raw);
            };

            // Cleanup after disconnect
            self.undivert();
            self.dev.take(); // drop closes the HID device
            self.feat_idx = None;
            self.dpi_idx = None;
            self.smart_shift_idx = None;
            self.battery_idx = None;
            self.held = false;
            self.mode_shift_held = false;
            self.gesture_cid = DEFAULT_GESTURE_CIDS[0];
            self.gesture_candidates = DEFAULT_GESTURE_CIDS.to_vec();
            self.rawxy_enabled = false;

            if disconnect {
                if let Some(cb) = &self.callbacks.on_device_disconnected {
                    cb();
                }
            }

            // Brief pause before reconnect
            for _ in 0..20 {
                if let Ok(Cmd::Stop) = self.cmd_rx.try_recv() {
                    return;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

// ── Public facade ─────────────────────────────────────────────────────────────

/// Logitech HID++ gesture listener.
///
/// Call `start()` to spawn the background thread, then use `set_dpi()` etc.
/// from any thread.  Call `stop()` to shut down gracefully.
pub struct HidGestureListener {
    callbacks: Option<HidCallbacks>,
    backend: HidBackend,
    cmd_tx: Option<Sender<Cmd>>,
    thread: Option<JoinHandle<()>>,
}

impl HidGestureListener {
    /// Create a new listener.  Does not open any device yet; call `start()`.
    pub fn new(callbacks: HidCallbacks, backend: HidBackend) -> Self {
        Self {
            callbacks: Some(callbacks),
            backend,
            cmd_tx: None,
            thread: None,
        }
    }

    /// Start the background thread.
    pub fn start(&mut self) {
        if self.thread.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.cmd_tx = Some(tx);

        let callbacks = self.callbacks.take().unwrap_or_default();
        let backend = self.backend;

        let handle = thread::Builder::new()
            .name("HidGesture".into())
            .spawn(move || {
                let mut worker = Worker::new(callbacks, rx, backend);
                worker.run();
            })
            .expect("failed to spawn HidGesture thread");

        self.thread = Some(handle);
    }

    /// Stop the background thread gracefully (waits up to 3 s).
    pub fn stop(&mut self) {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Stop);
        }
        if let Some(handle) = self.thread.take() {
            // Give the thread up to 3 s to exit cleanly
            let _ = handle.join();
        }
    }

    /// Queue a DPI change (non-blocking from caller's side).
    pub fn set_dpi(&self, dpi: u16) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(Cmd::SetDpi(dpi));
        }
    }

    /// Queue a DPI read.
    pub fn read_dpi(&self) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(Cmd::ReadDpi);
        }
    }

    /// Queue a Smart Shift mode change.  `mode` must be `"ratchet"` or `"freespin"`.
    pub fn set_smart_shift(&self, mode: &str) {
        if let Some(m) = SmartShiftMode::from_str(mode) {
            if let Some(tx) = &self.cmd_tx {
                let _ = tx.send(Cmd::SetSmartShift(m));
            }
        } else {
            warn!("[HidGesture] set_smart_shift: unknown mode '{mode}'");
        }
    }

    /// Queue a Smart Shift read.
    pub fn read_smart_shift(&self) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(Cmd::ReadSmartShift);
        }
    }

    /// Queue a battery read.
    pub fn read_battery(&self) {
        if let Some(tx) = &self.cmd_tx {
            let _ = tx.send(Cmd::ReadBattery);
        }
    }
}

impl Drop for HidGestureListener {
    fn drop(&mut self) {
        self.stop();
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_report ─────────────────────────────────────────────────────────

    /// Short report WITH report-ID byte prepended (the typical non-Windows layout).
    #[test]
    fn parse_short_report_with_id() {
        // [report_id=0x10, dev=0x01, feat=0x05, func<<4|sw=0x4A, p0, p1, p2]
        let raw = [0x10u8, 0x01, 0x05, 0x4A, 0x11, 0x22, 0x33];
        let msg = parse_report(&raw).expect("should parse");
        assert_eq!(msg.dev_idx, 0x01);
        assert_eq!(msg.feat_idx, 0x05);
        assert_eq!(msg.func, 0x04);
        assert_eq!(msg.sw, 0x0A);
        assert_eq!(msg.params, vec![0x11, 0x22, 0x33]);
    }

    /// Short report WITHOUT report-ID byte (Windows hidapi C backend strips it).
    #[test]
    fn parse_short_report_without_id() {
        // [dev=0x01, feat=0x05, func<<4|sw=0x4A, p0, p1, p2, p3]
        let raw = [0x01u8, 0x05, 0x4A, 0x11, 0x22, 0x33, 0x44];
        let msg = parse_report(&raw).expect("should parse");
        assert_eq!(msg.dev_idx, 0x01);
        assert_eq!(msg.feat_idx, 0x05);
        assert_eq!(msg.func, 0x04);
        assert_eq!(msg.sw, 0x0A);
        assert_eq!(msg.params[0], 0x11);
    }

    /// Long report with report-ID (20 bytes total).
    #[test]
    fn parse_long_report_with_id() {
        let mut raw = [0u8; 20];
        raw[0] = LONG_REPORT_ID;
        raw[1] = 0xFF; // dev_idx BT
        raw[2] = 0x08; // feat_idx
        raw[3] = 0x1A; // func=1, sw=0xA
        raw[4] = 0xDE;
        raw[5] = 0xAD;
        let msg = parse_report(&raw).expect("should parse");
        assert_eq!(msg.dev_idx, 0xFF);
        assert_eq!(msg.feat_idx, 0x08);
        assert_eq!(msg.func, 0x01);
        assert_eq!(msg.sw, 0x0A);
        assert_eq!(msg.params[0], 0xDE);
        assert_eq!(msg.params[1], 0xAD);
    }

    /// Too-short buffer returns None.
    #[test]
    fn parse_too_short_returns_none() {
        assert!(parse_report(&[]).is_none());
        assert!(parse_report(&[0x10, 0x01, 0x00]).is_none());
    }

    // ── decode_s16 ───────────────────────────────────────────────────────────

    #[test]
    fn decode_positive_s16() {
        assert_eq!(decode_s16(0x00, 0x05), 5i16);
        assert_eq!(decode_s16(0x01, 0x00), 256i16);
    }

    #[test]
    fn decode_negative_s16() {
        // -1 is 0xFFFF
        assert_eq!(decode_s16(0xFF, 0xFF), -1i16);
        // -256 is 0xFF00
        assert_eq!(decode_s16(0xFF, 0x00), -256i16);
    }

    #[test]
    fn decode_zero_s16() {
        assert_eq!(decode_s16(0x00, 0x00), 0i16);
    }

    // ── format_key_flags ─────────────────────────────────────────────────────

    #[test]
    fn format_key_flags_empty() {
        assert_eq!(format_key_flags(0), "none");
    }

    #[test]
    fn format_key_flags_raw_xy() {
        // bit 0x0100 = raw_xy
        let s = format_key_flags(0x0100);
        assert!(s.contains("raw_xy"), "got: {s}");
    }

    #[test]
    fn format_key_flags_multiple() {
        // divertable (0x0020) + mse (0x0001)
        let s = format_key_flags(0x0021);
        assert!(s.contains("mse"), "got: {s}");
        assert!(s.contains("divertable"), "got: {s}");
    }

    // ── choose_gesture_candidates ────────────────────────────────────────────

    #[test]
    fn choose_defaults_when_no_controls() {
        let worker = Worker::new(HidCallbacks::default(), mpsc::channel().1, HidBackend::Auto);
        let result = worker.choose_gesture_candidates(&[]);
        assert_eq!(result, DEFAULT_GESTURE_CIDS);
    }

    #[test]
    fn choose_prefers_known_cids() {
        let worker = Worker::new(HidCallbacks::default(), mpsc::channel().1, HidBackend::Auto);
        let controls = vec![
            ControlInfo {
                index: 0, cid: 0x00C3, task: 0, key_flags: 0x01A0, // virtual+raw_xy+divertable
                pos: 0, group: 0, gmask: 0, mapping_flags: 0,
            },
            ControlInfo {
                index: 1, cid: 0x00D7, task: 0, key_flags: 0x01A0,
                pos: 0, group: 0, gmask: 0, mapping_flags: 0,
            },
        ];
        let result = worker.choose_gesture_candidates(&controls);
        // 0x00C3 must come before 0x00D7
        assert_eq!(result[0], 0x00C3);
        assert_eq!(result[1], 0x00D7);
    }

    // ── SmartShiftMode parsing ────────────────────────────────────────────────

    #[test]
    fn smart_shift_mode_from_str() {
        assert_eq!(SmartShiftMode::from_str("ratchet"), Some(SmartShiftMode::Ratchet));
        assert_eq!(SmartShiftMode::from_str("FREESPIN"), Some(SmartShiftMode::FreeSpin));
        assert_eq!(SmartShiftMode::from_str("bad"), None);
    }

    #[test]
    fn smart_shift_mode_as_byte() {
        assert_eq!(SmartShiftMode::Ratchet.as_byte(), SMART_SHIFT_RATCHET);
        assert_eq!(SmartShiftMode::FreeSpin.as_byte(), SMART_SHIFT_FREESPIN);
    }

    // ── HID++ error name ─────────────────────────────────────────────────────

    #[test]
    fn error_names_known() {
        assert_eq!(hidpp_error_name(0x01), "UNKNOWN");
        assert_eq!(hidpp_error_name(0x06), "INVALID_FEATURE_INDEX");
        assert_eq!(hidpp_error_name(0x09), "UNSUPPORTED");
    }

    #[test]
    fn error_names_unknown() {
        assert_eq!(hidpp_error_name(0xFF), "?");
    }
}
