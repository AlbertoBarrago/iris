#!/usr/bin/env bash
# Builds the debug Iris with Xcode, quits the copy already running, and
# opens the new one:
#
#   scripts/dev-macos.sh
set -euo pipefail

cd "$(dirname "$0")/../macos"
command -v xcodegen >/dev/null || { echo "xcodegen is missing: brew install xcodegen" >&2; exit 1; }
xcodegen generate -q
xcodebuild -project Iris.xcodeproj -scheme Iris -configuration Debug -derivedDataPath build build -quiet
app="build/Build/Products/Debug/Iris.app"
osascript -e 'tell application id "io.github.AlbertoBarrago.Iris" to quit' >/dev/null 2>&1 || true
sleep 1
open "$app"
