#!/usr/bin/env bash
# Build a self-contained Mouser.app bundle from the Rust binary.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET="${1:-aarch64-apple-darwin}"  # default to Apple Silicon; pass x86_64-apple-darwin for Intel
APP_NAME="Mouser-Rust.app"
OUT_DIR="$SCRIPT_DIR/dist"
APP_PATH="$OUT_DIR/$APP_NAME"

echo "Building for target: $TARGET"
cd "$SCRIPT_DIR"
rustup target add "$TARGET" 2>/dev/null || true
cargo build --release --target "$TARGET"

BINARY="$SCRIPT_DIR/target/$TARGET/release/mouser"

echo "Creating app bundle at $APP_PATH"
rm -rf "$APP_PATH"
mkdir -p "$APP_PATH/Contents/MacOS"
mkdir -p "$APP_PATH/Contents/Resources"

# Copy binary
cp "$BINARY" "$APP_PATH/Contents/MacOS/mouser"
chmod +x "$APP_PATH/Contents/MacOS/mouser"

# Generate icon if logo exists
ICON_SRC="$SCRIPT_DIR/../images/logo_icon.png"
if [[ -f "$ICON_SRC" ]]; then
    ICONSET="$OUT_DIR/Mouser.iconset"
    mkdir -p "$ICONSET"
    for size in 16 32 64 128 256 512; do
        sips -z $size $size "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null 2>&1 || true
        double=$((size * 2))
        sips -z $double $double "$ICON_SRC" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null 2>&1 || true
    done
    iconutil -c icns "$ICONSET" -o "$APP_PATH/Contents/Resources/Mouser.icns" 2>/dev/null || true
    rm -rf "$ICONSET"
fi

# Write Info.plist
ICON_KEY=""
if [[ -f "$APP_PATH/Contents/Resources/Mouser.icns" ]]; then
    ICON_KEY="<key>CFBundleIconFile</key><string>Mouser</string>"
fi

cat > "$APP_PATH/Contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>mouser</string>
  <key>CFBundleIdentifier</key>
  <string>io.github.tombadash.mouser-rust</string>
  <key>CFBundleName</key>
  <string>Mouser</string>
  <key>CFBundleDisplayName</key>
  <string>Mouser</string>
  <key>CFBundleVersion</key>
  <string>$(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['packages'][0]['version'])" 2>/dev/null || echo "0.1.0")</string>
  <key>CFBundleShortVersionString</key>
  <string>$(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c "import sys,json; print(json.load(sys.stdin)['packages'][0]['version'])" 2>/dev/null || echo "0.1.0")</string>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>LSMinimumSystemVersion</key>
  <string>12.0</string>
  <key>NSAccessibilityUsageDescription</key>
  <string>Mouser needs Accessibility access to intercept and remap mouse buttons.</string>
  ${ICON_KEY}
</dict>
</plist>
PLIST

# Self-sign
codesign --force --deep --sign - "$APP_PATH"

echo "Built: $APP_PATH"
echo "Binary size: $(du -sh "$APP_PATH/Contents/MacOS/mouser" | cut -f1)"
