#!/bin/bash
# Builds the installer image: build/Voice Flow <version>.dmg, with the app and a shortcut to
# Applications (drag the app onto it). Run scripts/build-app.sh first.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/build/Voice Flow.app"
[[ -d "$APP" ]] || { echo "Missing $APP: run scripts/build-app.sh" >&2; exit 1; }

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")"
DMG="$ROOT/build/Voice Flow $VERSION.dmg"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
# "Resource busy" sometimes happens on the first try (Spotlight indexing the folder): retry.
for attempt in 1 2 3; do
    if hdiutil create -volname "Voice Flow" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null 2>&1; then
        break
    fi
    [[ $attempt == 3 ]] && { echo "Could not create the installer image" >&2; exit 1; }
    sleep 2
done
echo "Done: $DMG"
