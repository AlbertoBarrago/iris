#!/usr/bin/env bash
# Builds a release Iris with Xcode and installs it as /Applications/Iris.app,
# where Launchpad, Spotlight and the Dock find it like any other app. Your
# accounts and mail are the same as the debug build's: both read the same
# folders.
set -euo pipefail

cd "$(dirname "$0")/.."
command -v xcodegen >/dev/null || { echo "xcodegen is missing: brew install xcodegen" >&2; exit 1; }
(cd macos && xcodegen generate -q &&
    xcodebuild -project Iris.xcodeproj -scheme Iris -configuration Release -derivedDataPath build build -quiet)
built="macos/build/Build/Products/Release/Iris.app"

installed=/Applications/Iris.app
if pgrep -x Iris >/dev/null; then
    osascript -e 'tell application id "io.github.AlbertoBarrago.Iris" to quit' >/dev/null 2>&1 || true
    for _ in $(seq 1 20); do
        pgrep -x Iris >/dev/null || break
        sleep 0.3
    done
    pkill -x Iris 2>/dev/null || true
fi

# ditto keeps the signature, extended attributes and symlinks intact.
rm -rf "$installed"
ditto "$built" "$installed"
codesign --verify --strict "$installed"
echo "Installed $installed"
