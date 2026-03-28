# Mouser

Mouser is a cross-platform (Windows/macOS/Linux) Logitech HID++ mouse button remapper. It captures low-level mouse events, remaps them to keyboard shortcuts/actions, and provides a UI for configuration. Fully local, zero telemetry.

## Project Structure

This repo contains two independent implementations:

### Python (original, at repo root)
```
main_qml.py          # Entry point (PySide6/QML UI)
core/                 # Core logic
  engine.py           # Event loop, profile switching, action dispatch
  mouse_hook.py       # Platform mouse hooks (Win WH_MOUSE_LL, macOS CGEventTap, Linux evdev)
  key_simulator.py    # Platform key simulation (Win SendInput, macOS CGEvent, Linux UInput)
  hid_gesture.py      # Logitech HID++ protocol (gesture, DPI, battery, SmartShift)
  config.py           # JSON config management with migration (v1-v6)
  app_detector.py     # Foreground app detection (per-app profiles)
  app_catalog.py      # Application catalog & discovery
  logi_devices.py     # Device specs, DPI ranges, product IDs
  device_layouts.py   # UI layout definitions for mouse diagrams
  accessibility.py    # macOS accessibility trust check
  startup.py          # Login startup (Windows registry, macOS LaunchAgent)
  log_setup.py        # Rotating file logging
ui/
  backend.py          # QML backend bridge (Python <-> QML)
  locale_manager.py   # Multi-language (en, zh_CN, zh_TW)
  qml/                # Qt Quick/QML UI files
tests/                # Python unittest suite
```

### Rust (new, in rust/)
```
rust/
  Cargo.toml
  build.rs            # Platform link flags (macOS frameworks)
  src/
    main.rs           # Entry point
    config.rs         # JSON config (serde, same schema as Python)
    devices.rs        # Device specs
    device_layouts.rs # UI layout definitions
    actions.rs        # Action registry (45+ predefined actions)
    engine.rs         # Event loop & orchestrator
    hid_gesture.rs    # HID++ protocol
    locale.rs         # Multi-language strings
    logging.rs        # tracing + rolling file
    single_instance.rs
    accessibility.rs  # macOS AXIsProcessTrusted
    startup.rs        # Login startup
    app_catalog.rs    # App discovery
    mouse_hook/       # Platform mouse hooks (mod.rs + windows/macos/linux.rs)
    key_simulator/    # Platform key sim (mod.rs + windows/macos/linux.rs)
    app_detector/     # Foreground app detection (mod.rs + windows/macos/linux.rs)
    ui/               # egui UI + system tray
```

## Build & Run

### Python
```bash
pip install -r requirements.txt
python main_qml.py
python main_qml.py --start-hidden
python main_qml.py --hid-backend=iokit   # macOS only
```

### Rust
```bash
cd rust
cargo build
cargo test
cargo run
cargo build --release   # optimized build
```

### Python Tests
```bash
python -m unittest discover -s tests -p "test_*.py"
```

### Rust Tests
```bash
cd rust && cargo test
```

## Architecture

### Event Flow
1. `MouseHook` captures platform mouse events (low-level hooks)
2. `Engine` maps events to actions via active profile
3. `KeySimulator` simulates keyboard shortcuts via platform APIs
4. `HidGestureListener` handles HID++ protocol (gesture detection, DPI, battery)
5. `AppDetector` polls foreground app for profile auto-switching

### Config
- Location: `~/.config/Mouser/config.json` (Linux), `~/Library/Application Support/Mouser/config.json` (macOS), `%APPDATA%\Mouser\config.json` (Windows)
- Schema version: 6 (with migration from v1-v5)
- Both Python and Rust read/write the same config.json format

### Threading Model
- Main thread: UI event loop
- Hook thread: mouse event capture (platform-specific)
- HID thread: HID++ device communication (blocking I/O)
- App detector: foreground app polling (300ms interval)

### Platform Modules
Mouse hook, key simulator, and app detector each have platform-specific implementations selected at compile time:
- Windows: `windows` crate (Win32 API)
- macOS: `core-graphics` + `cocoa` crates (Quartz, AppKit)
- Linux: `evdev` crate (input subsystem)

## Conventions

- Platform code uses `#[cfg(target_os = "...")]` (Rust) or `if sys.platform == "..."` (Python)
- Config changes are atomic (tempfile + rename)
- All button names, action IDs, and device keys are lowercase_snake_case strings
- Custom shortcuts use the format `custom:ctrl+shift+a`
- The Rust port shares the same JSON config schema for interoperability
- UI is the lowest priority in the Rust port; core daemon functionality comes first

## Key Dependencies

### Python
- PySide6 (Qt/QML UI), hidapi (HID++), pyobjc (macOS), evdev (Linux)

### Rust
- serde/serde_json (config), hidapi (HID++), windows (Win32), core-graphics/cocoa (macOS), evdev (Linux), eframe/egui (UI), tray-icon (system tray), tracing (logging), dirs (platform paths)
