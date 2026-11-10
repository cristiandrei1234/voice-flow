#!/bin/bash
# Generates the app icon (macos/Resources/AppIcon.icns) and the README logo (docs/logo.png)
# from the drawing in macos/Sources/VoiceFlow/Logo.swift. Run it after changing the logo.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

(cd "$ROOT/macos" && swift build -c release)
"$ROOT/macos/.build/release/VoiceFlow" --icon "$TMP/art"

SET="$TMP/AppIcon.iconset"
mkdir -p "$SET"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$TMP/art/icon-1024.png" --out "$SET/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$TMP/art/icon-1024.png" --out "$SET/icon_${size}x${size}@2x.png" >/dev/null
done

mkdir -p "$ROOT/macos/Resources" "$ROOT/docs"
iconutil -c icns "$SET" -o "$ROOT/macos/Resources/AppIcon.icns"
cp "$TMP/art/logo-512.png" "$ROOT/docs/logo.png"
echo "Done: macos/Resources/AppIcon.icns and docs/logo.png"
