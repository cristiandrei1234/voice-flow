#!/bin/bash
# Builds "Voice Flow.app": the Rust engine plus the SwiftUI app, in one bundle.
#
#   ./scripts/build-app.sh            builds build/Voice Flow.app
#   ./scripts/build-app.sh --install  also copies it to ~/Applications and launches it
#
# Needs Rust (cargo) and Xcode or the Command Line Tools (swift). Liquid Glass needs Xcode 26+.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/build/Voice Flow.app"

echo "1/4 Rust engine..."
(cd "$ROOT" && cargo build --release)

echo "2/4 SwiftUI app..."
(cd "$ROOT/macos" && swift build -c release)

echo "3/4 App bundle..."
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/macos/.build/release/VoiceFlow" "$APP/Contents/MacOS/VoiceFlow"
cp "$ROOT/target/release/voice-flow" "$APP/Contents/Resources/voice-flow"
cp "$ROOT/macos/Info.plist" "$APP/Contents/Info.plist"
# The icon is generated from Logo.swift by scripts/make-icon.sh.
[[ -f "$ROOT/macos/Resources/AppIcon.icns" ]] && cp "$ROOT/macos/Resources/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"

echo "4/4 Local signing (stable certificate, see scripts/signing.sh)..."
source "$ROOT/scripts/signing.sh"
vf_ensure_identity
codesign --force --keychain "$VF_KEYCHAIN" --sign "$VF_IDENTITY" \
    --identifier com.automwise.voiceflow.engine "$APP/Contents/Resources/voice-flow"
codesign --force --keychain "$VF_KEYCHAIN" --sign "$VF_IDENTITY" "$APP"
codesign --verify --strict "$APP"

echo
echo "Done: $APP"

if [[ "${1:-}" == "--install" ]]; then
    mkdir -p "$HOME/Applications"
    pkill -x VoiceFlow 2>/dev/null || true
    rm -rf "$HOME/Applications/Voice Flow.app"
    cp -R "$APP" "$HOME/Applications/"
    open "$HOME/Applications/Voice Flow.app"
    echo "Installed in ~/Applications and launched. The icon is in the menu bar."
else
    echo "Launch it with:  open \"$APP\""
fi
