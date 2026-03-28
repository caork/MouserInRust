#!/usr/bin/env bash
# Build Mouser Rust binary for Linux and create an AppImage-like portable package.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Building Mouser (Rust) for Linux..."
cargo build --release

BINARY="target/release/mouser"
strip "$BINARY"

DIST="dist-rust"
mkdir -p "$DIST"
cp "$BINARY" "$DIST/mouser"
chmod +x "$DIST/mouser"

echo "Build complete: $DIST/mouser"
echo "Binary size: $(du -sh "$DIST/mouser" | cut -f1)"

# Create a simple .desktop file for system integration
cat > "$DIST/mouser.desktop" << 'DESKTOP'
[Desktop Entry]
Name=Mouser
Comment=Logitech mouse button remapper
Exec=mouser
Icon=input-mouse
Type=Application
Categories=Utility;
StartupNotify=false
X-GNOME-AutostartEnabled=false
DESKTOP

echo "Package ready in $DIST/"
