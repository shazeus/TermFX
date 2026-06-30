#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="$ROOT_DIR/dist/TermFX Studio.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"

cargo build --bin termfx-studio

mkdir -p "$MACOS_DIR"
cp "$ROOT_DIR/target/debug/termfx-studio" "$MACOS_DIR/termfx-studio"
cp "$ROOT_DIR/packaging/macos/Info.plist" "$CONTENTS_DIR/Info.plist"
chmod +x "$MACOS_DIR/termfx-studio"

echo "$APP_DIR"
