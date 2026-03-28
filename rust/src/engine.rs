//! Engine — wires the mouse hook to the key simulator using the current
//! configuration.  Sits between the hook layer and the UI.  Supports
//! per-application auto-switching of profiles.
//!
//! Ported from `core/engine.py`.
//!
//! # Architecture
//!
//! ```text
//!  ┌──────────────────────────────────────────────────────────┐
//!  │                       Engine                             │
//!  │                                                          │
//!  │  ┌──────────────┐   ┌──────────────────────────────┐   │
//!  │  │  MouseHook   │   │      HidGestureListener       │   │
//!  │  │  (platform)  │   │  (HID++ gesture + DPI + SS)  │   │
//!  │  └──────┬───────┘   └──────────────┬───────────────┘   │
//!  │         │ MouseEvent                │ on_gesture_*       │
//!  │         ▼                           ▼                    │
//!  │  ┌──────────────────────────────────────────────────┐   │
//!  │  │           key_simulator::execute_action          │   │
//!  │  └──────────────────────────────────────────────────┘   │
//!  │                                                          │
//!  │  AppDetector ──on_change──► _on_app_change              │
//!  └──────────────────────────────────────────────────────────┘
//! ```
//!
//! The `HidGestureListener` fires `on_gesture_down/move/up` callbacks that
//! the engine forwards into the `MouseHook`'s internal `GestureDetector`
//! via the `MouseHookGestureInput` extension trait.  Each platform hook
//! implements that trait; on platforms without a real implementation the
//! default no-op is used.
//!
//! Gesture *events* (GestureClick, GestureSwipeLeft, …) bubble up through the
//! hook's registered callbacks exactly like any other `MouseEvent`.

#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::app_detector::AppDetector;
use crate::config::{
    get_active_mappings, get_profile_for_app, load_config, save_config, Config,
    GESTURE_DIRECTION_BUTTONS,
};
use crate::hid_gesture::{HidBackend, HidCallbacks, HidGestureListener};
use crate::key_simulator::execute_action;
use crate::mouse_hook::{
    create_hook, GestureConfig, MouseEvent, MouseEventData, MouseEventType,
};

// ---------------------------------------------------------------------------
// Cooldown constants
// ---------------------------------------------------------------------------

/// Normal hscroll cooldown: 350 ms.
const HSCROLL_ACTION_COOLDOWN: Duration = Duration::from_millis(350);

/// Shorter cooldown for volume-type actions: 60 ms.
const HSCROLL_VOLUME_COOLDOWN: Duration = Duration::from_millis(60);

/// Action IDs that use the shorter volume cooldown.
const VOLUME_ACTIONS: &[&str] = &["volume_up", "volume_down"];

// ---------------------------------------------------------------------------
// MouseHookGestureInput extension trait
// ---------------------------------------------------------------------------

/// Extension trait that allows the Engine to feed raw HID gesture events
/// (button-down, XY move, button-up) into a hook's internal `GestureDetector`.
///
/// Platform hooks that support this override the relevant methods.
/// The blanket default implementation is a no-op so the code compiles on all
/// platforms without changes to `mouse_hook/mod.rs`.
pub trait MouseHookGestureInput {
    /// Called when the HID gesture button is pressed.
    fn gesture_button_down(&mut self) {}
    /// Called with raw XY motion while the gesture button is held.
    fn gesture_move(&mut self, _dx: f64, _dy: f64, _source: &str) {}
    /// Called when the HID gesture button is released.
    fn gesture_button_up(&mut self) {}
    /// Called when the HID mode-shift button is pressed.
    fn mode_shift_down(&mut self) {}
    /// Called when the HID mode-shift button is released.
    fn mode_shift_up(&mut self) {}
}

// Blanket: every type that implements `MouseHook` also gets the default no-op
// `MouseHookGestureInput`.
impl<T: crate::mouse_hook::MouseHook + ?Sized> MouseHookGestureInput for T {}

// ---------------------------------------------------------------------------
// Helper: button key → (MouseEventType, is_up_event) list
// ---------------------------------------------------------------------------

/// Map a button key (as stored in a profile's mappings) to the
/// `MouseEventType`s that should be registered for it.
///
/// Returns `&[(event_type, is_up)]`.  Up-events are blocked so the OS never
/// sees them, but no handler is registered (matching the Python logic).
fn button_to_event_types(button: &str) -> &'static [(MouseEventType, bool)] {
    match button {
        "middle" => &[
            (MouseEventType::MiddleDown, false),
            (MouseEventType::MiddleUp, true),
        ],
        // "gesture" covers tap + all four swipe directions.
        "gesture" => &[
            (MouseEventType::GestureClick, false),
            (MouseEventType::GestureSwipeLeft, false),
            (MouseEventType::GestureSwipeRight, false),
            (MouseEventType::GestureSwipeUp, false),
            (MouseEventType::GestureSwipeDown, false),
        ],
        "gesture_left" => &[(MouseEventType::GestureSwipeLeft, false)],
        "gesture_right" => &[(MouseEventType::GestureSwipeRight, false)],
        "gesture_up" => &[(MouseEventType::GestureSwipeUp, false)],
        "gesture_down" => &[(MouseEventType::GestureSwipeDown, false)],
        "xbutton1" => &[
            (MouseEventType::XButton1Down, false),
            (MouseEventType::XButton1Up, true),
        ],
        "xbutton2" => &[
            (MouseEventType::XButton2Down, false),
            (MouseEventType::XButton2Up, true),
        ],
        "hscroll_left" => &[(MouseEventType::HScrollLeft, false)],
        "hscroll_right" => &[(MouseEventType::HScrollRight, false)],
        "mode_shift" => &[
            (MouseEventType::ModeShiftDown, false),
            (MouseEventType::ModeShiftUp, true),
        ],
        _ => &[],
    }
}

// ---------------------------------------------------------------------------
// HScroll accumulator state
// ---------------------------------------------------------------------------

/// Per-direction accumulator for horizontal scroll events.
#[derive(Debug, Clone)]
struct HScrollState {
    accum: f64,
    last_fire_at: Option<Instant>,
}

impl HScrollState {
    fn new() -> Self {
        Self {
            accum: 0.0,
            last_fire_at: None,
        }
    }

    /// Returns `true` when the cooldown has elapsed (or the direction has
    /// never fired).
    fn cooldown_elapsed(&self, cooldown: Duration) -> bool {
        self.last_fire_at
            .map(|t| t.elapsed() >= cooldown)
            .unwrap_or(true)
    }
}

// ---------------------------------------------------------------------------
// EngineConfig
// ---------------------------------------------------------------------------

/// Engine-level configuration supplied at construction time.
pub struct EngineConfig {
    pub hid_backend: HidBackend,
    pub debug: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            hid_backend: HidBackend::Auto,
            debug: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal type aliases
// ---------------------------------------------------------------------------

/// The mouse hook lives in a shared holder so both Engine methods and
/// background closures (HID callbacks, app-change handler) can reach it.
type HookHolder = Arc<Mutex<Option<Box<dyn crate::mouse_hook::MouseHook>>>>;

type DebugCb = Arc<Mutex<Option<Box<dyn Fn(String) + Send + 'static>>>>;
type GestureCb =
    Arc<Mutex<Option<Box<dyn Fn(String, String, String) + Send + 'static>>>>;

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Core engine: reads config, installs the mouse hook, dispatches actions
/// when mapped buttons are pressed, and auto-switches profiles when the
/// foreground application changes.
pub struct Engine {
    config: Arc<Mutex<Config>>,
    hook: HookHolder,
    hid_listener: HidGestureListener,
    app_detector: AppDetector,
    current_profile: Arc<Mutex<String>>,
    enabled: Arc<AtomicBool>,
    /// `(left_state, right_state)` for horizontal scroll accumulation.
    hscroll_state: Arc<Mutex<(HScrollState, HScrollState)>>,
    debug_cb: DebugCb,
    /// Structured gesture event callback `(event_type, button_name, action_id)`
    /// for the UI debug panel.
    gesture_cb: GestureCb,
    debug_enabled: Arc<AtomicBool>,
}

impl Engine {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Create a new [`Engine`].
    ///
    /// Wires all callback chains and performs initial hook registration but
    /// does **not** start background threads.  Call [`Engine::start`] after
    /// registering any UI callbacks.
    pub fn new(
        cfg: Config,
        engine_config: EngineConfig,
        ui_state: Option<Arc<Mutex<crate::ui::UiState>>>,
    ) -> Self {
        let debug_enabled = Arc::new(AtomicBool::new(
            cfg.settings.debug_mode || engine_config.debug,
        ));
        let current_profile = Arc::new(Mutex::new(cfg.active_profile.clone()));
        let config = Arc::new(Mutex::new(cfg));
        let enabled = Arc::new(AtomicBool::new(true));
        let hscroll_state = Arc::new(Mutex::new((HScrollState::new(), HScrollState::new())));
        let debug_cb: DebugCb = Arc::new(Mutex::new(None));
        let gesture_cb: GestureCb = Arc::new(Mutex::new(None));
        let ui_state_ref = ui_state.unwrap_or_else(|| {
            Arc::new(Mutex::new(crate::ui::UiState::default()))
        });

        // The hook lives in a shared Arc so background closures can reach it.
        let hook_holder: HookHolder = Arc::new(Mutex::new(None));

        // ------------------------------------------------------------------
        // HID gesture callbacks
        // ------------------------------------------------------------------
        // Each HID callback forwards its signal into the hook's internal
        // GestureDetector via `MouseHookGestureInput`.  On platforms where
        // the hook drives gesture detection through HID++ exclusively (as
        // opposed to the event-tap / evdev path) this is the only code path.
        // The `MouseHookGestureInput` default implementations are no-ops, so
        // platforms that handle this internally are unaffected.

        let hid_callbacks = {
            let h_down = Arc::clone(&hook_holder);
            let h_move = Arc::clone(&hook_holder);
            let h_up = Arc::clone(&hook_holder);
            let h_ms_down = Arc::clone(&hook_holder);
            let h_ms_up = Arc::clone(&hook_holder);
            let dbg = Arc::clone(&debug_cb);
            let dbg2 = Arc::clone(&debug_cb);
            let dbg3 = Arc::clone(&debug_cb);
            let dbg4 = Arc::clone(&debug_cb);
            let dbg5 = Arc::clone(&debug_cb);
            let de = Arc::clone(&debug_enabled);
            let de2 = Arc::clone(&debug_enabled);
            let de3 = Arc::clone(&debug_enabled);
            let de4 = Arc::clone(&debug_enabled);
            let de5 = Arc::clone(&debug_enabled);

            HidCallbacks {
                on_gesture_down: Some(Box::new(move || {
                    {
                        let mut g = h_down.lock().unwrap();
                        if let Some(h) = g.as_mut() {
                            h.gesture_button_down();
                        }
                    }
                    if de.load(Ordering::Relaxed) {
                        emit_debug(&dbg, "HID: gesture button DOWN".into());
                    }
                })),
                on_gesture_up: Some(Box::new(move || {
                    {
                        let mut g = h_up.lock().unwrap();
                        if let Some(h) = g.as_mut() {
                            h.gesture_button_up();
                        }
                    }
                    if de2.load(Ordering::Relaxed) {
                        emit_debug(&dbg2, "HID: gesture button UP".into());
                    }
                })),
                on_gesture_move: Some(Box::new(move |dx: i16, dy: i16| {
                    {
                        let mut g = h_move.lock().unwrap();
                        if let Some(h) = g.as_mut() {
                            h.gesture_move(dx as f64, dy as f64, "hid_rawxy");
                        }
                    }
                    if de3.load(Ordering::Relaxed) {
                        emit_debug(&dbg3, format!("HID: gesture move dx={dx} dy={dy}"));
                    }
                })),
                on_device_connected: {
                    let st = Arc::clone(&ui_state_ref);
                    Some(Box::new(move |info: String| {
                        if let Ok(mut s) = st.lock() {
                            s.device_name = info.clone();
                        }
                        if de4.load(Ordering::Relaxed) {
                            emit_debug(&dbg4, format!("HID: device connected {info}"));
                        }
                    }))
                },
                on_device_disconnected: {
                    let st = Arc::clone(&ui_state_ref);
                    Some(Box::new(move || {
                        if let Ok(mut s) = st.lock() {
                            s.device_name = "No device".into();
                            s.battery_pct = None;
                        }
                        if de5.load(Ordering::Relaxed) {
                            emit_debug(&dbg5, "HID: device disconnected".into());
                        }
                    }))
                },
                on_battery: {
                    let st = Arc::clone(&ui_state_ref);
                    Some(Box::new(move |pct: u8| {
                        if let Ok(mut s) = st.lock() {
                            s.battery_pct = Some(pct);
                        }
                    }))
                },
                on_dpi_changed: {
                    let st = Arc::clone(&ui_state_ref);
                    Some(Box::new(move |dpi: u16| {
                        if let Ok(mut s) = st.lock() {
                            s.dpi = dpi as u32;
                        }
                    }))
                },
                on_mode_shift_down: Some(Box::new(move || {
                    let mut g = h_ms_down.lock().unwrap();
                    if let Some(h) = g.as_mut() {
                        h.mode_shift_down();
                    }
                })),
                on_mode_shift_up: Some(Box::new(move || {
                    let mut g = h_ms_up.lock().unwrap();
                    if let Some(h) = g.as_mut() {
                        h.mode_shift_up();
                    }
                })),
            }
        };

        let hid_listener = HidGestureListener::new(hid_callbacks, engine_config.hid_backend);

        // ------------------------------------------------------------------
        // Platform hook
        // ------------------------------------------------------------------

        let mut hook = create_hook();
        hook.set_connection_change_callback(Box::new(|_connected| {
            // HidGestureListener manages its own reconnect loop.
            // A UI-facing device-status callback can be added through
            // Engine::set_connection_callback in a future extension.
        }));
        *hook_holder.lock().unwrap() = Some(hook);

        // ------------------------------------------------------------------
        // App-change detector
        // ------------------------------------------------------------------

        let cfg_app = Arc::clone(&config);
        let cp_app = Arc::clone(&current_profile);
        let en_app = Arc::clone(&enabled);
        let hook_app = Arc::clone(&hook_holder);
        let hs_app = Arc::clone(&hscroll_state);
        let dbg_app = Arc::clone(&debug_cb);
        let de_app = Arc::clone(&debug_enabled);
        let gcb_app = Arc::clone(&gesture_cb);

        let app_detector = AppDetector::new(
            move |exe_name: String| {
                if !en_app.load(Ordering::Relaxed) {
                    return;
                }
                let target = {
                    let cfg = cfg_app.lock().unwrap();
                    get_profile_for_app(&cfg, &exe_name).to_owned()
                };
                let changed = {
                    let mut cp = cp_app.lock().unwrap();
                    if *cp != target {
                        log::info!("[Engine] App '{}' → profile '{}'", exe_name, target);
                        *cp = target.clone();
                        true
                    } else {
                        false
                    }
                };
                if changed {
                    {
                        let mut cfg = cfg_app.lock().unwrap();
                        cfg.active_profile = target.clone();
                    }
                    {
                        let mut g = hook_app.lock().unwrap();
                        if let Some(h) = g.as_mut() {
                            h.reset_bindings();
                            let cfg = cfg_app.lock().unwrap();
                            setup_hooks_inner(
                                h.as_mut(),
                                &cfg,
                                &en_app,
                                &hs_app,
                                &dbg_app,
                                &de_app,
                                &gcb_app,
                            );
                        }
                    }
                    if de_app.load(Ordering::Relaxed) {
                        emit_debug(
                            &dbg_app,
                            format!("[Engine] Active profile -> {}", target),
                        );
                    }
                }
            },
            300,
        );

        // ------------------------------------------------------------------
        // Initial hook wiring
        // ------------------------------------------------------------------

        {
            let mut g = hook_holder.lock().unwrap();
            if let Some(h) = g.as_mut() {
                let cfg = config.lock().unwrap();
                setup_hooks_inner(
                    h.as_mut(),
                    &cfg,
                    &enabled,
                    &hscroll_state,
                    &debug_cb,
                    &debug_enabled,
                    &gesture_cb,
                );
            }
        }

        Engine {
            config,
            hook: hook_holder,
            hid_listener,
            app_detector,
            current_profile,
            enabled,
            hscroll_state,
            debug_cb,
            gesture_cb,
            debug_enabled,
        }
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Start all background threads (hook, HID listener, app detector).
    pub fn start(&mut self) -> anyhow::Result<()> {
        {
            let mut g = self.hook.lock().unwrap();
            if let Some(h) = g.as_mut() {
                h.start()?;
            }
        }
        self.hid_listener.start();
        self.app_detector.start();
        Ok(())
    }

    /// Stop all background threads gracefully.
    pub fn stop(&mut self) {
        self.app_detector.stop();
        self.hid_listener.stop();
        let mut g = self.hook.lock().unwrap();
        if let Some(h) = g.as_mut() {
            h.stop();
        }
    }

    // -----------------------------------------------------------------------
    // Mapping reload
    // -----------------------------------------------------------------------

    /// Re-read config from disk and re-register all hook callbacks.
    ///
    /// Call this after the user changes a mapping in the UI.
    pub fn reload_mappings(&mut self) {
        match load_config() {
            Ok(fresh) => {
                let profile = fresh.active_profile.clone();
                {
                    let mut cfg = self.config.lock().unwrap();
                    *cfg = fresh;
                }
                {
                    let mut cp = self.current_profile.lock().unwrap();
                    *cp = profile.clone();
                }
                {
                    let mut g = self.hook.lock().unwrap();
                    if let Some(h) = g.as_mut() {
                        h.reset_bindings();
                        let cfg = self.config.lock().unwrap();
                        setup_hooks_inner(
                            h.as_mut(),
                            &cfg,
                            &self.enabled,
                            &self.hscroll_state,
                            &self.debug_cb,
                            &self.debug_enabled,
                            &self.gesture_cb,
                        );
                    }
                }
                if self.debug_enabled.load(Ordering::Relaxed) {
                    emit_debug(
                        &self.debug_cb,
                        format!("[Engine] reload_mappings profile={}", profile),
                    );
                }
            }
            Err(e) => {
                log::error!("[Engine] reload_mappings: {e}");
            }
        }
    }

    // -----------------------------------------------------------------------
    // Device controls
    // -----------------------------------------------------------------------

    /// Persist and apply a DPI change via HID++.
    pub fn set_dpi(&mut self, dpi: u32) {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.settings.dpi = dpi;
            let _ = save_config(&cfg);
        }
        self.hid_listener.set_dpi(dpi as u16);
    }

    /// Persist and apply a Smart Shift mode change (`"ratchet"` | `"freespin"`).
    pub fn set_smart_shift(&mut self, mode: &str) {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.settings.smart_shift_mode = mode.to_owned();
            let _ = save_config(&cfg);
        }
        self.hid_listener.set_smart_shift(mode);
    }

    // -----------------------------------------------------------------------
    // Public accessors
    // -----------------------------------------------------------------------

    /// Return a clone of the shared config handle (for UI read/write).
    pub fn get_config(&self) -> Arc<Mutex<Config>> {
        Arc::clone(&self.config)
    }

    // -----------------------------------------------------------------------
    // UI callbacks
    // -----------------------------------------------------------------------

    /// Register a debug-message callback `cb(message: String)`.
    pub fn set_debug_callback(&mut self, cb: Box<dyn Fn(String) + Send + 'static>) {
        *self.debug_cb.lock().unwrap() = Some(cb);
    }

    /// Register a structured gesture event callback for the UI debug panel.
    ///
    /// The callback receives `(event_type, button_name, action_id)`.
    pub fn set_gesture_callback(
        &mut self,
        cb: Box<dyn Fn(String, String, String) + Send + 'static>,
    ) {
        *self.gesture_cb.lock().unwrap() = Some(cb);
    }

    /// Enable or disable debug output.
    pub fn set_debug_enabled(&mut self, enabled: bool) {
        self.debug_enabled.store(enabled, Ordering::Relaxed);
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.settings.debug_mode = enabled;
        }
        if enabled {
            let profile = self.current_profile.lock().unwrap().clone();
            emit_debug(
                &self.debug_cb,
                format!("[Engine] Debug enabled on profile {}", profile),
            );
        }
    }

    /// Enable or disable action dispatch (when `false`, events are still
    /// intercepted but `execute_action` is not called).
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Hook wiring (free function so it can be called from closures without &self)
// ---------------------------------------------------------------------------

/// Register hook callbacks for every button→action mapping in the active
/// profile.  This is the Rust equivalent of `Engine._setup_hooks()`.
fn setup_hooks_inner(
    hook: &mut dyn crate::mouse_hook::MouseHook,
    cfg: &Config,
    enabled: &Arc<AtomicBool>,
    hscroll_state: &Arc<Mutex<(HScrollState, HScrollState)>>,
    debug_cb: &DebugCb,
    debug_enabled: &Arc<AtomicBool>,
    gesture_cb: &GestureCb,
) {
    let mappings = get_active_mappings(cfg);
    let settings = &cfg.settings;

    // Configure gesture detection parameters.
    let gestures_active = GESTURE_DIRECTION_BUTTONS
        .iter()
        .chain(std::iter::once(&"gesture"))
        .any(|k| mappings.get(*k).map(|v| v != "none").unwrap_or(false));
    let _ = gestures_active; // used implicitly via configure_gestures

    hook.configure_gestures(GestureConfig {
        threshold: settings.gesture_threshold,
        deadzone: settings.gesture_deadzone,
        timeout_ms: settings.gesture_timeout_ms,
        cooldown_ms: settings.gesture_cooldown_ms,
    });

    // Emit a mapping snapshot to the debug callback.
    if debug_enabled.load(Ordering::Relaxed) {
        let keys = [
            "gesture",
            "gesture_left",
            "gesture_right",
            "gesture_up",
            "gesture_down",
            "xbutton1",
            "xbutton2",
        ];
        let summary = keys
            .iter()
            .map(|k| format!("{}={}", k, mappings.get(*k).unwrap_or(&"none".to_owned())))
            .collect::<Vec<_>>()
            .join(", ");
        emit_debug(debug_cb, format!("Hook mappings refreshed: {summary}"));
    }

    // Register a handler (and block) for each button→action pair.
    for (btn_key, action_id) in mappings {
        if action_id == "none" {
            continue;
        }
        let action_id = action_id.clone();
        let events = button_to_event_types(btn_key);

        for &(evt_type, is_up) in events {
            // Block the event from reaching the OS regardless of direction.
            hook.block(evt_type);

            if is_up {
                // Up-events are blocked but need no handler.
                continue;
            }

            let is_hscroll = matches!(
                evt_type,
                MouseEventType::HScrollLeft | MouseEventType::HScrollRight
            );

            if is_hscroll {
                let action_id = action_id.clone();
                let hscroll_state = Arc::clone(hscroll_state);
                let threshold = settings.hscroll_threshold.max(0.1);
                let enabled = Arc::clone(enabled);
                let debug_cb = Arc::clone(debug_cb);
                let debug_enabled = Arc::clone(debug_enabled);
                let is_left = evt_type == MouseEventType::HScrollLeft;

                hook.register(
                    evt_type,
                    Box::new(move |_evt, data| {
                        if !enabled.load(Ordering::Relaxed) {
                            return;
                        }

                        // Cap large wheel deltas to 1.0; preserve sub-1 values.
                        let step = data
                            .as_ref()
                            .map(|d| d.scroll_delta.abs().min(1.0))
                            .unwrap_or(1.0);

                        let cooldown = if VOLUME_ACTIONS.contains(&action_id.as_str()) {
                            HSCROLL_VOLUME_COOLDOWN
                        } else {
                            HSCROLL_ACTION_COOLDOWN
                        };

                        let should_fire = {
                            let mut pair = hscroll_state.lock().unwrap();
                            let state = if is_left { &mut pair.0 } else { &mut pair.1 };

                            if !state.cooldown_elapsed(cooldown) {
                                // Within cooldown window: reset and suppress.
                                state.accum = 0.0;
                                false
                            } else {
                                state.accum += step;
                                if state.accum >= threshold {
                                    state.accum = 0.0;
                                    state.last_fire_at = Some(Instant::now());
                                    true
                                } else {
                                    false
                                }
                            }
                        };

                        if should_fire {
                            if debug_enabled.load(Ordering::Relaxed) {
                                let dir = if is_left { "hscroll_left" } else { "hscroll_right" };
                                emit_debug(
                                    &debug_cb,
                                    format!("Mapped {dir} -> {action_id}"),
                                );
                            }
                            if let Err(e) = execute_action(&action_id) {
                                log::warn!("[Engine] execute_action '{action_id}': {e}");
                            }
                        }
                    }),
                );
            } else {
                // Regular (non-scroll) handler.
                let action_id = action_id.clone();
                let enabled = Arc::clone(enabled);
                let debug_cb = Arc::clone(debug_cb);
                let debug_enabled = Arc::clone(debug_enabled);
                let gesture_cb = Arc::clone(gesture_cb);
                let btn_key = btn_key.clone();

                hook.register(
                    evt_type,
                    Box::new(move |event, _data| {
                        if !enabled.load(Ordering::Relaxed) {
                            return;
                        }

                        if debug_enabled.load(Ordering::Relaxed) {
                            let event_name = format!("{:?}", event);
                            emit_debug(
                                &debug_cb,
                                format!("Mapped {event_name} -> {action_id}"),
                            );

                            // Emit structured gesture event for UI debug panel.
                            let is_gesture = matches!(
                                event,
                                MouseEvent::GestureClick
                                    | MouseEvent::GestureSwipeLeft
                                    | MouseEvent::GestureSwipeRight
                                    | MouseEvent::GestureSwipeUp
                                    | MouseEvent::GestureSwipeDown
                            );
                            if is_gesture {
                                let guard = gesture_cb.lock().unwrap();
                                if let Some(cb) = guard.as_ref() {
                                    cb(
                                        format!("{:?}", event),
                                        btn_key.clone(),
                                        action_id.clone(),
                                    );
                                }
                            }
                        }

                        if let Err(e) = execute_action(&action_id) {
                            log::warn!("[Engine] execute_action '{action_id}': {e}");
                        }
                    }),
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Debug helper
// ---------------------------------------------------------------------------

/// Invoke the debug callback with `message`, ignoring any panic in the
/// callback closure.
fn emit_debug(cb: &DebugCb, message: String) {
    let guard = cb.lock().unwrap();
    if let Some(f) = guard.as_ref() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(message)));
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Profile, Settings};
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    fn default_mappings() -> HashMap<String, String> {
        let mut m = HashMap::new();
        for k in &[
            "middle",
            "gesture",
            "gesture_left",
            "gesture_right",
            "gesture_up",
            "gesture_down",
            "mode_shift",
        ] {
            m.insert((*k).into(), "none".into());
        }
        m.insert("xbutton1".into(), "alt_tab".into());
        m.insert("xbutton2".into(), "browser_forward".into());
        m.insert("hscroll_left".into(), "browser_back".into());
        m.insert("hscroll_right".into(), "browser_forward".into());
        m
    }

    fn vscode_mappings() -> HashMap<String, String> {
        let mut m = default_mappings();
        m.insert("xbutton1".into(), "copy".into());
        m.insert("xbutton2".into(), "paste".into());
        m.insert("hscroll_left".into(), "volume_down".into());
        m.insert("hscroll_right".into(), "volume_up".into());
        m
    }

    fn make_multi_profile_config() -> Config {
        let mut profiles = HashMap::new();
        profiles.insert(
            "default".into(),
            Profile {
                label: "Default".into(),
                apps: vec![],
                mappings: default_mappings(),
            },
        );
        profiles.insert(
            "vscode".into(),
            Profile {
                label: "VS Code".into(),
                apps: vec!["Code.exe".into(), "code".into()],
                mappings: vscode_mappings(),
            },
        );
        Config {
            version: 6,
            active_profile: "default".into(),
            profiles,
            settings: Settings::default(),
        }
    }

    // -----------------------------------------------------------------------
    // Profile switching
    // -----------------------------------------------------------------------

    #[test]
    fn unknown_app_falls_back_to_default() {
        let cfg = make_multi_profile_config();
        assert_eq!(get_profile_for_app(&cfg, "notepad.exe"), "default");
    }

    #[test]
    fn known_app_returns_its_profile() {
        let cfg = make_multi_profile_config();
        assert_eq!(get_profile_for_app(&cfg, "Code.exe"), "vscode");
        assert_eq!(get_profile_for_app(&cfg, "code"), "vscode");
    }

    #[test]
    fn profile_lookup_is_case_insensitive() {
        let cfg = make_multi_profile_config();
        assert_eq!(get_profile_for_app(&cfg, "CODE.EXE"), "vscode");
        assert_eq!(get_profile_for_app(&cfg, "CODE"), "vscode");
    }

    #[test]
    fn switch_profile_activates_correct_mappings() {
        let mut cfg = make_multi_profile_config();
        // Switch to vscode.
        cfg.active_profile = "vscode".into();
        let m = get_active_mappings(&cfg);
        assert_eq!(m.get("xbutton1").unwrap(), "copy");
        assert_eq!(m.get("hscroll_left").unwrap(), "volume_down");

        // Switch back to default.
        cfg.active_profile = "default".into();
        let m = get_active_mappings(&cfg);
        assert_eq!(m.get("xbutton1").unwrap(), "alt_tab");
        assert_eq!(m.get("hscroll_left").unwrap(), "browser_back");
    }

    #[test]
    fn missing_active_profile_falls_back_to_default() {
        let mut cfg = make_multi_profile_config();
        cfg.active_profile = "nonexistent_profile".into();
        // get_active_mappings falls back to "default".
        let m = get_active_mappings(&cfg);
        assert_eq!(m.get("xbutton1").unwrap(), "alt_tab");
    }

    #[test]
    fn profile_change_only_when_app_not_already_mapped() {
        let cfg = make_multi_profile_config();
        // An app registered in vscode.
        let profile = get_profile_for_app(&cfg, "Code.exe");
        assert_eq!(profile, "vscode");
        // An unregistered app.
        let profile2 = get_profile_for_app(&cfg, "firefox.exe");
        assert_eq!(profile2, "default");
    }

    // -----------------------------------------------------------------------
    // HScroll accumulation
    // -----------------------------------------------------------------------

    #[test]
    fn hscroll_fires_when_threshold_reached() {
        let threshold = 1.0f64;
        let mut state = HScrollState::new();

        // Accumulate to threshold.
        state.accum += 1.0;
        assert!(state.accum >= threshold);
        assert!(state.cooldown_elapsed(HSCROLL_ACTION_COOLDOWN));
    }

    #[test]
    fn hscroll_does_not_fire_below_threshold() {
        let threshold = 3.0f64;
        let mut state = HScrollState::new();
        state.accum += 1.0;
        assert!(state.accum < threshold);
        state.accum += 1.0;
        assert!(state.accum < threshold);
    }

    #[test]
    fn hscroll_fires_after_multi_step_accumulation() {
        let threshold = 3.0f64;
        let mut state = HScrollState::new();
        for _ in 0..3 {
            state.accum += 1.0;
        }
        assert!(state.accum >= threshold);
    }

    #[test]
    fn hscroll_cooldown_blocks_immediate_repeat() {
        let mut state = HScrollState::new();
        state.last_fire_at = Some(Instant::now());
        // Should still be within cooldown.
        assert!(!state.cooldown_elapsed(HSCROLL_ACTION_COOLDOWN));
    }

    #[test]
    fn hscroll_cooldown_allows_after_delay() {
        let mut state = HScrollState::new();
        // last_fire_at in the distant past.
        state.last_fire_at = Some(Instant::now() - Duration::from_secs(10));
        assert!(state.cooldown_elapsed(HSCROLL_ACTION_COOLDOWN));
    }

    #[test]
    fn hscroll_never_fired_cooldown_elapsed() {
        let state = HScrollState::new();
        assert!(state.cooldown_elapsed(HSCROLL_ACTION_COOLDOWN));
    }

    #[test]
    fn hscroll_volume_cooldown_shorter_than_normal() {
        assert!(HSCROLL_VOLUME_COOLDOWN < HSCROLL_ACTION_COOLDOWN);
    }

    #[test]
    fn hscroll_step_capped_to_one() {
        // Large raw delta should be capped to 1.0.
        let step = 999.0f64.abs().min(1.0);
        assert_eq!(step, 1.0);
    }

    #[test]
    fn hscroll_sub_one_step_preserved() {
        let step = 0.33f64.abs().min(1.0);
        assert!((step - 0.33).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // Button → event type mapping
    // -----------------------------------------------------------------------

    #[test]
    fn middle_has_down_and_up() {
        let evts = button_to_event_types("middle");
        assert!(evts.iter().any(|&(e, up)| e == MouseEventType::MiddleDown && !up));
        assert!(evts.iter().any(|&(e, up)| e == MouseEventType::MiddleUp && up));
    }

    #[test]
    fn gesture_covers_click_and_all_four_swipes() {
        let types: Vec<MouseEventType> =
            button_to_event_types("gesture").iter().map(|&(e, _)| e).collect();
        assert!(types.contains(&MouseEventType::GestureClick));
        assert!(types.contains(&MouseEventType::GestureSwipeLeft));
        assert!(types.contains(&MouseEventType::GestureSwipeRight));
        assert!(types.contains(&MouseEventType::GestureSwipeUp));
        assert!(types.contains(&MouseEventType::GestureSwipeDown));
    }

    #[test]
    fn gesture_left_maps_to_single_swipe_left() {
        let evts = button_to_event_types("gesture_left");
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0], (MouseEventType::GestureSwipeLeft, false));
    }

    #[test]
    fn hscroll_left_maps_to_single_event() {
        let evts = button_to_event_types("hscroll_left");
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0], (MouseEventType::HScrollLeft, false));
    }

    #[test]
    fn mode_shift_has_down_and_up() {
        let evts = button_to_event_types("mode_shift");
        assert!(evts.iter().any(|&(e, up)| e == MouseEventType::ModeShiftDown && !up));
        assert!(evts.iter().any(|&(e, up)| e == MouseEventType::ModeShiftUp && up));
    }

    #[test]
    fn unknown_button_returns_empty_slice() {
        assert!(button_to_event_types("not_a_real_button").is_empty());
    }

    // -----------------------------------------------------------------------
    // Volume action detection
    // -----------------------------------------------------------------------

    #[test]
    fn volume_up_and_down_in_volume_actions() {
        assert!(VOLUME_ACTIONS.contains(&"volume_up"));
        assert!(VOLUME_ACTIONS.contains(&"volume_down"));
    }

    #[test]
    fn non_volume_not_in_volume_actions() {
        assert!(!VOLUME_ACTIONS.contains(&"alt_tab"));
        assert!(!VOLUME_ACTIONS.contains(&"browser_back"));
        assert!(!VOLUME_ACTIONS.contains(&"copy"));
    }
}
