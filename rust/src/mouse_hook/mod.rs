//! Cross-platform mouse hook module.
//!
//! Intercepts mouse button presses and horizontal scroll events so they can be
//! remapped before reaching applications.  Platform implementations are in the
//! sibling modules; call [`create_hook`] to get a boxed trait object for the
//! current OS.
#![allow(dead_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Public surface types
// ---------------------------------------------------------------------------

/// A captured mouse event, analogous to `MouseEvent` in the Python source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseEvent {
    XButton1Down,
    XButton1Up,
    XButton2Down,
    XButton2Up,
    MiddleDown,
    MiddleUp,
    /// Gesture button held down (from HidGestureListener).
    GestureDown,
    /// Gesture button released (from HidGestureListener).
    GestureUp,
    /// Tap on the gesture button without swipe.
    GestureClick,
    GestureSwipeLeft,
    GestureSwipeRight,
    GestureSwipeUp,
    GestureSwipeDown,
    HScrollLeft,
    HScrollRight,
    ModeShiftDown,
    ModeShiftUp,
}

/// Which event type to register a callback or block/unblock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseEventType {
    XButton1Down,
    XButton1Up,
    XButton2Down,
    XButton2Up,
    MiddleDown,
    MiddleUp,
    GestureDown,
    GestureUp,
    GestureClick,
    GestureSwipeLeft,
    GestureSwipeRight,
    GestureSwipeUp,
    GestureSwipeDown,
    HScrollLeft,
    HScrollRight,
    ModeShiftDown,
    ModeShiftUp,
}

impl From<MouseEvent> for MouseEventType {
    fn from(e: MouseEvent) -> Self {
        match e {
            MouseEvent::XButton1Down => MouseEventType::XButton1Down,
            MouseEvent::XButton1Up => MouseEventType::XButton1Up,
            MouseEvent::XButton2Down => MouseEventType::XButton2Down,
            MouseEvent::XButton2Up => MouseEventType::XButton2Up,
            MouseEvent::MiddleDown => MouseEventType::MiddleDown,
            MouseEvent::MiddleUp => MouseEventType::MiddleUp,
            MouseEvent::GestureDown => MouseEventType::GestureDown,
            MouseEvent::GestureUp => MouseEventType::GestureUp,
            MouseEvent::GestureClick => MouseEventType::GestureClick,
            MouseEvent::GestureSwipeLeft => MouseEventType::GestureSwipeLeft,
            MouseEvent::GestureSwipeRight => MouseEventType::GestureSwipeRight,
            MouseEvent::GestureSwipeUp => MouseEventType::GestureSwipeUp,
            MouseEvent::GestureSwipeDown => MouseEventType::GestureSwipeDown,
            MouseEvent::HScrollLeft => MouseEventType::HScrollLeft,
            MouseEvent::HScrollRight => MouseEventType::HScrollRight,
            MouseEvent::ModeShiftDown => MouseEventType::ModeShiftDown,
            MouseEvent::ModeShiftUp => MouseEventType::ModeShiftUp,
        }
    }
}

/// Optional payload attached to some events (e.g. scroll delta, gesture deltas).
#[derive(Debug, Clone)]
pub struct MouseEventData {
    pub delta_x: f64,
    pub delta_y: f64,
    pub scroll_delta: f64,
    pub source: Option<String>,
}

impl Default for MouseEventData {
    fn default() -> Self {
        Self {
            delta_x: 0.0,
            delta_y: 0.0,
            scroll_delta: 0.0,
            source: None,
        }
    }
}

// ---------------------------------------------------------------------------
// GestureConfig
// ---------------------------------------------------------------------------

/// Tuning knobs for the shared [`GestureDetector`].
///
/// * `threshold`   – minimum dominant-axis displacement (px) to fire a swipe.
/// * `deadzone`    – minimum cross-axis displacement tolerated before the gesture
///                   is declared ambiguous and ignored.
/// * `timeout_ms`  – if no movement arrives for this many ms the accumulator resets.
/// * `cooldown_ms` – after a gesture fires, ignore new input for this long.
#[derive(Debug, Clone, Copy)]
pub struct GestureConfig {
    pub threshold: u32,
    pub deadzone: u32,
    pub timeout_ms: u32,
    pub cooldown_ms: u32,
    /// How long (ms) the candidate direction must remain stable before firing.
    /// Set to 0 in tests for instant swipe detection.
    pub confirm_ms: u64,
}

impl Default for GestureConfig {
    fn default() -> Self {
        Self {
            threshold: 50,
            deadzone: 40,
            timeout_ms: 3000,
            cooldown_ms: 500,
            confirm_ms: 80,
        }
    }
}

// ---------------------------------------------------------------------------
// GestureDetector
// ---------------------------------------------------------------------------

/// Shared, lock-protected gesture state machine extracted from the platform
/// implementations.
///
/// Handles:
/// * tap vs. swipe detection (gesture_click vs. gesture_swipe_*)
/// * delta accumulation with timeout-based segment reset
/// * per-source locking (hid_rawxy beats evdev/event_tap)
/// * cooldown window after a gesture fires
pub struct GestureDetector {
    pub config: GestureConfig,
    pub enabled: bool,

    active: bool,
    tracking: bool,
    triggered: bool,

    delta_x: f64,
    delta_y: f64,
    input_source: Option<String>,

    started_at: Option<Instant>,
    last_move_at: Option<Instant>,
    cooldown_until: Option<Instant>,

    /// Direction confirmation: once the distance threshold is crossed,
    /// we record a candidate direction and the time.  The swipe only
    /// fires after `CONFIRM_MS` if the direction hasn't changed.
    candidate_dir: Option<MouseEvent>,
    candidate_since: Option<Instant>,
}

impl GestureDetector {
    pub fn new(config: GestureConfig) -> Self {
        Self {
            config,
            enabled: false,
            active: false,
            tracking: false,
            triggered: false,
            delta_x: 0.0,
            delta_y: 0.0,
            input_source: None,
            started_at: None,
            last_move_at: None,
            cooldown_until: None,
            candidate_dir: None,
            candidate_since: None,
        }
    }

    pub fn configure(&mut self, config: GestureConfig) {
        self.config = GestureConfig {
            threshold: config.threshold.max(5),
            deadzone: config.deadzone,
            timeout_ms: config.timeout_ms.max(250),
            cooldown_ms: config.cooldown_ms,
            confirm_ms: config.confirm_ms,
        };
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.tracking = false;
            self.triggered = false;
            self.input_source = None;
        }
    }

    // -- gesture button events (from HID listener) -----------------------

    /// Call when the gesture button is pressed.
    pub fn on_button_down(&mut self) {
        if self.active {
            return;
        }
        self.active = true;
        self.triggered = false;
        log::debug!("[GestureDetector] button down");
        if self.enabled && !self.cooldown_active() {
            self.start_tracking();
        } else {
            self.tracking = false;
            self.triggered = false;
        }
    }

    /// Call when the gesture button is released.
    /// Returns `Some(MouseEvent::GestureClick)` when the release should fire a
    /// click (i.e. no swipe was triggered during the press).
    pub fn on_button_up(&mut self) -> Option<MouseEvent> {
        if !self.active {
            return None;
        }
        let should_click = !self.triggered;
        self.active = false;
        self.finish_tracking();
        self.triggered = false;
        log::debug!("[GestureDetector] button up click_candidate={}", should_click);
        if should_click {
            Some(MouseEvent::GestureClick)
        } else {
            None
        }
    }

    // -- movement accumulation -------------------------------------------

    /// Accumulate a relative motion delta from `source`.
    ///
    /// Returns `Some(event)` when a swipe threshold is crossed.
    pub fn accumulate(&mut self, dx: f64, dy: f64, source: &str) -> Option<MouseEvent> {
        if !self.enabled || !self.active || self.triggered {
            // Once a swipe has been triggered, ignore further movement
            // until the button is released.
            return None;
        }
        if self.cooldown_active() {
            log::debug!("[GestureDetector] cooldown active, ignoring dx={dx} dy={dy}");
            return None;
        }

        if !self.tracking {
            log::debug!("[GestureDetector] tracking started source={source}");
            self.start_tracking();
        }

        let now = Instant::now();
        // Timeout: reset accumulator if idle too long
        if let Some(lm) = self.last_move_at {
            let idle_ms = now.duration_since(lm).as_millis() as u32;
            if idle_ms > self.config.timeout_ms {
                log::debug!(
                    "[GestureDetector] segment reset timeout dx={} dy={}",
                    self.delta_x,
                    self.delta_y
                );
                self.start_tracking();
            }
        }

        // Source promotion: hid_rawxy beats lower-fidelity sources
        if source == "hid_rawxy" {
            if self.input_source.as_deref() == Some("event_tap")
                || self.input_source.as_deref() == Some("evdev")
            {
                log::debug!("[GestureDetector] source promoted to hid_rawxy");
                self.start_tracking();
            }
        }

        // Source lock
        if let Some(ref locked) = self.input_source.clone() {
            if locked != source {
                log::debug!("[GestureDetector] source locked to {locked}, ignoring {source}");
                return None;
            }
        }
        self.input_source = Some(source.to_owned());

        self.delta_x += dx;
        self.delta_y += dy;
        self.last_move_at = Some(now);

        // Check if we've crossed the distance threshold
        let dir = match self.classify_direction() {
            Some(d) => d,
            None => {
                // Below threshold — reset candidate
                self.candidate_dir = None;
                self.candidate_since = None;
                return None;
            }
        };

        // If confirm_ms is 0, fire immediately (used in tests and for instant response)
        if self.config.confirm_ms == 0 {
            self.triggered = true;
            log::debug!(
                "[GestureDetector] instant {:?} dx={:.0} dy={:.0}",
                dir, self.delta_x, self.delta_y
            );
            self.cooldown_until = Some(
                Instant::now() + Duration::from_millis(self.config.cooldown_ms as u64),
            );
            self.finish_tracking();
            return Some(dir);
        }

        // Direction confirmation: the candidate must stay stable for
        // confirm_ms before we fire.  If the direction changes, restart
        // the confirmation timer.
        match self.candidate_dir {
            Some(prev) if prev == dir => {
                // Same direction — check if confirmed
                if let Some(since) = self.candidate_since {
                    let held_ms = now.duration_since(since).as_millis() as u64;
                    if held_ms >= self.config.confirm_ms {
                        self.triggered = true;
                        log::debug!(
                            "[GestureDetector] confirmed {:?} dx={:.0} dy={:.0} after {}ms",
                            dir, self.delta_x, self.delta_y, held_ms
                        );
                        self.cooldown_until = Some(
                            Instant::now() + Duration::from_millis(self.config.cooldown_ms as u64),
                        );
                        self.finish_tracking();
                        return Some(dir);
                    }
                }
            }
            _ => {
                // New or changed direction — start confirmation timer
                self.candidate_dir = Some(dir);
                self.candidate_since = Some(now);
            }
        }
        None
    }

    // -- private helpers -------------------------------------------------

    fn cooldown_active(&self) -> bool {
        self.cooldown_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false)
    }

    fn start_tracking(&mut self) {
        self.tracking = self.enabled;
        let now = Instant::now();
        self.started_at = Some(now);
        self.last_move_at = Some(now);
        self.delta_x = 0.0;
        self.delta_y = 0.0;
        self.input_source = None;
    }

    fn finish_tracking(&mut self) {
        self.tracking = false;
        self.started_at = None;
        self.last_move_at = None;
        self.delta_x = 0.0;
        self.delta_y = 0.0;
        self.input_source = None;
        self.candidate_dir = None;
        self.candidate_since = None;
    }

    /// Classify the current accumulated delta into a cardinal direction using `atan2`.
    ///
    /// This is the standard algorithm used by Android, iOS, and game engines.
    /// The circle is divided into four 90° cones centered on each cardinal
    /// direction (±45° from axis). Every angle maps to exactly one direction
    /// with no ambiguous zones.
    ///
    /// ```text
    ///            Up (-90°)
    ///          /     \
    ///    Left ← ─ · ─ → Right (0°)
    ///          \     /
    ///           Down (90°)
    /// ```
    fn classify_direction(&self) -> Option<MouseEvent> {
        let distance = (self.delta_x * self.delta_x + self.delta_y * self.delta_y).sqrt();
        if distance < self.config.threshold as f64 {
            return None;
        }

        // atan2 returns radians: 0=right, π/2=down, ±π=left, -π/2=up
        let angle = self.delta_y.atan2(self.delta_x);

        // Each direction gets a 90° (π/2) cone:
        //   Right:  -π/4  to  π/4
        //   Down:    π/4  to  3π/4
        //   Left:   3π/4  to  π  and  -π  to -3π/4
        //   Up:    -3π/4  to -π/4
        let pi_4 = std::f64::consts::FRAC_PI_4;

        Some(if angle >= -pi_4 && angle < pi_4 {
            MouseEvent::GestureSwipeRight
        } else if angle >= pi_4 && angle < 3.0 * pi_4 {
            MouseEvent::GestureSwipeDown
        } else if angle >= -3.0 * pi_4 && angle < -pi_4 {
            MouseEvent::GestureSwipeUp
        } else {
            MouseEvent::GestureSwipeLeft
        })
    }
}

// ---------------------------------------------------------------------------
// Callback storage helper
// ---------------------------------------------------------------------------

/// Type-alias for a heap-allocated, Send callback.
pub type Callback = Box<dyn Fn(MouseEvent, Option<MouseEventData>) + Send + 'static>;

/// Stores multiple callbacks keyed by `MouseEventType`.
pub struct CallbackMap(HashMap<MouseEventType, Vec<Callback>>);

impl CallbackMap {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn register(&mut self, event_type: MouseEventType, cb: Callback) {
        self.0.entry(event_type).or_default().push(cb);
    }

    pub fn dispatch(&self, event: MouseEvent, data: Option<MouseEventData>) {
        if let Some(cbs) = self.0.get(&MouseEventType::from(event)) {
            for cb in cbs {
                cb(event, data.clone());
            }
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl Default for CallbackMap {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// MouseHook trait
// ---------------------------------------------------------------------------

/// Platform-neutral interface for the low-level mouse hook.
pub trait MouseHook: Send + crate::engine::MouseHookGestureInput {
    /// Register a callback for a specific event type.
    fn register(
        &mut self,
        event_type: MouseEventType,
        cb: Box<dyn Fn(MouseEvent, Option<MouseEventData>) + Send + 'static>,
    );

    /// Suppress OS-level delivery of an event type (return early / block).
    fn block(&mut self, event_type: MouseEventType);

    /// Un-suppress a previously blocked event type.
    fn unblock(&mut self, event_type: MouseEventType);

    /// Clear all registered callbacks and blocked events.
    fn reset_bindings(&mut self);

    /// Apply gesture-detection tuning.
    fn configure_gestures(&mut self, config: GestureConfig);

    /// Register a callback invoked when device connection state changes.
    fn set_connection_change_callback(&mut self, cb: Box<dyn Fn(bool) + Send + 'static>);

    /// Start the hook (installs OS-level hooks, spawns threads).
    fn start(&mut self) -> anyhow::Result<()>;

    /// Stop the hook and clean up all resources.
    fn stop(&mut self);
}

// ---------------------------------------------------------------------------
// Platform dispatch
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub mod linux;

// Compile-time stubs so the other modules are always present for IDEs / CI
// cross-compilation.  They contain stub types but are gated so only the real
// one is exported at runtime.
#[cfg(not(target_os = "windows"))]
pub mod windows;
#[cfg(not(target_os = "macos"))]
pub mod macos;
#[cfg(not(target_os = "linux"))]
pub mod linux;

/// Create a platform-specific [`MouseHook`] implementation.
pub fn create_hook() -> Box<dyn MouseHook> {
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsMouseHook::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosMouseHook::new())
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxMouseHook::new())
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    )))]
    {
        compile_error!("Unsupported platform — no MouseHook implementation available.");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn detector(threshold: u32, deadzone: u32) -> GestureDetector {
        let cfg = GestureConfig {
            threshold,
            deadzone,
            timeout_ms: 3000,
            cooldown_ms: 0, // no cooldown so tests can re-trigger
            confirm_ms: 0,  // instant confirmation for tests
        };
        let mut d = GestureDetector::new(cfg);
        d.set_enabled(true);
        d.on_button_down();
        d
    }

    // -- threshold ----------------------------------------------------------

    #[test]
    fn threshold_not_crossed_returns_none() {
        let mut d = detector(50, 10);
        // accumulate just below threshold on X axis
        let result = d.accumulate(49.0, 0.0, "test");
        assert_eq!(result, None);
    }

    #[test]
    fn threshold_crossed_right() {
        let mut d = detector(50, 10);
        let result = d.accumulate(55.0, 0.0, "test");
        assert_eq!(result, Some(MouseEvent::GestureSwipeRight));
    }

    #[test]
    fn threshold_crossed_left() {
        let mut d = detector(50, 10);
        let result = d.accumulate(-55.0, 0.0, "test");
        assert_eq!(result, Some(MouseEvent::GestureSwipeLeft));
    }

    #[test]
    fn threshold_crossed_down() {
        let mut d = detector(50, 10);
        let result = d.accumulate(0.0, 55.0, "test");
        assert_eq!(result, Some(MouseEvent::GestureSwipeDown));
    }

    #[test]
    fn threshold_crossed_up() {
        let mut d = detector(50, 10);
        let result = d.accumulate(0.0, -55.0, "test");
        assert_eq!(result, Some(MouseEvent::GestureSwipeUp));
    }

    // -- deadzone -----------------------------------------------------------

    #[test]
    fn near_diagonal_classified_by_atan2() {
        // With atan2, (55, 45) → angle ≈ 39° which falls in the Right cone (±45°)
        let mut d = detector(50, 40);
        let result = d.accumulate(55.0, 45.0, "test");
        assert_eq!(result, Some(MouseEvent::GestureSwipeRight), "39° should map to Right");
    }

    #[test]
    fn deadzone_allows_clean_horizontal() {
        // abs_y=5 which is well below cross_limit
        let mut d = detector(50, 40);
        let result = d.accumulate(55.0, 5.0, "test");
        assert_eq!(result, Some(MouseEvent::GestureSwipeRight));
    }

    #[test]
    fn deadzone_allows_clean_vertical() {
        let mut d = detector(50, 40);
        let result = d.accumulate(5.0, -55.0, "test");
        assert_eq!(result, Some(MouseEvent::GestureSwipeUp));
    }

    // -- accumulation over multiple calls -----------------------------------

    #[test]
    fn multi_step_accumulation_right() {
        let mut d = detector(50, 10);
        assert_eq!(d.accumulate(20.0, 0.0, "test"), None);
        assert_eq!(d.accumulate(20.0, 0.0, "test"), None);
        let result = d.accumulate(15.0, 0.0, "test");
        assert_eq!(result, Some(MouseEvent::GestureSwipeRight));
    }

    // -- disabled detector --------------------------------------------------

    #[test]
    fn disabled_detector_never_fires() {
        let cfg = GestureConfig {
            threshold: 10,
            deadzone: 0,
            timeout_ms: 3000,
            cooldown_ms: 0,
            confirm_ms: 0,
        };
        let mut d = GestureDetector::new(cfg);
        // deliberately NOT calling set_enabled(true)
        d.on_button_down();
        let result = d.accumulate(100.0, 0.0, "test");
        assert_eq!(result, None);
    }

    // -- click vs swipe -----------------------------------------------------

    #[test]
    fn no_movement_produces_click() {
        let cfg = GestureConfig {
            threshold: 50,
            deadzone: 10,
            timeout_ms: 3000,
            cooldown_ms: 0,
            confirm_ms: 0,
        };
        let mut d = GestureDetector::new(cfg);
        d.set_enabled(true);
        d.on_button_down();
        // no accumulate calls — button up should yield a click
        let result = d.on_button_up();
        assert_eq!(result, Some(MouseEvent::GestureClick));
    }

    #[test]
    fn swipe_suppresses_click() {
        let mut d = detector(50, 10);
        // trigger a swipe
        let swipe = d.accumulate(100.0, 0.0, "test");
        assert_eq!(swipe, Some(MouseEvent::GestureSwipeRight));
        // button up must NOT also produce a click
        let click = d.on_button_up();
        assert_eq!(click, None);
    }

    // -- direction detection correctness ------------------------------------

    #[test]
    fn direction_detection_all_axes() {
        let cases: &[(f64, f64, MouseEvent)] = &[
            (100.0, 0.0, MouseEvent::GestureSwipeRight),
            (-100.0, 0.0, MouseEvent::GestureSwipeLeft),
            (0.0, 100.0, MouseEvent::GestureSwipeDown),
            (0.0, -100.0, MouseEvent::GestureSwipeUp),
        ];
        for &(dx, dy, expected) in cases {
            let mut d = detector(50, 10);
            let result = d.accumulate(dx, dy, "test");
            assert_eq!(result, Some(expected), "dx={dx} dy={dy}");
        }
    }
}
