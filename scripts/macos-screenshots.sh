#!/usr/bin/env bash
# Takes the README's screenshots: the debug Iris on the demo's sample
# accounts, in dark and in light, into docs/screenshots/iris-dark.png and
# docs/screenshots/iris-light.png.
#
#   scripts/macos-screenshots.sh              build, then shoot
#   scripts/macos-screenshots.sh --no-build   shoot the build already there
#
# Each run opens a second Iris beside any copy already running, on a
# throwaway store, settings and data folder, so the person's own mail and
# preferences are never read or touched. The window comes to the front
# for a few seconds per shot. The Mac must be unlocked: a locked screen
# hands out no window pictures.
set -euo pipefail

cd "$(dirname "$0")/.."
root=$PWD
out="$root/docs/screenshots"
app="$root/macos/build/Build/Products/Debug/Iris.app"
# The conversation each shot opens, as `<account id>:<thread id>`. The
# demo adds its accounts in a fixed order, so the ids do not move.
conversation="2:t-roadmap"

if [ "${1:-}" != "--no-build" ]; then
    (
        cd macos
        command -v xcodegen >/dev/null || { echo "xcodegen is missing: brew install xcodegen" >&2; exit 1; }
        xcodegen generate -q
        xcodebuild -project Iris.xcodeproj -scheme Iris -configuration Debug -derivedDataPath build build -quiet
    )
fi
[ -d "$app" ] || { echo "macos-screenshots.sh: no debug build at $app" >&2; exit 1; }

work=$(mktemp -d -t iris-screenshots)
pid=
cleanup() {
    [ -n "$pid" ] && kill "$pid" 2>/dev/null || true
    rm -rf "$work"
}
trap cleanup EXIT

# A small helper that answers what the shell cannot: whether the screen
# is locked, and which window a process has on screen.
cat >"$work/window.swift" <<'SWIFT'
import CoreGraphics
import Foundation

let arguments = CommandLine.arguments
if arguments[1] == "locked" {
    let session = CGSessionCopyCurrentDictionary() as? [String: Any] ?? [:]
    exit((session["CGSSessionScreenIsLocked"] as? Bool ?? false) ? 0 : 1)
}
let pid = Int32(arguments[2])!
let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] ?? []
for window in windows
where window[kCGWindowOwnerPID as String] as? Int32 == pid && window[kCGWindowLayer as String] as? Int == 0 {
    let bounds = window[kCGWindowBounds as String] as? [String: Any] ?? [:]
    // A window still opening is a few points wide; wait for a real one.
    if (bounds["Width"] as? Double ?? 0) > 400 {
        print(window[kCGWindowNumber as String]!)
        exit(0)
    }
}
exit(1)
SWIFT
swiftc -O -o "$work/window" "$work/window.swift"

if "$work/window" locked; then
    echo "macos-screenshots.sh: the screen is locked; unlock the Mac and run it again" >&2
    exit 1
fi

# Shoots one appearance, `light` or `dark`, into docs/screenshots/iris-<it>.png.
shoot() {
    local look=$1 dir="$work/$1" window= tries=0
    mkdir -p "$dir"
    # The run's own Settings: the appearance, and American English
    # whatever language this Mac speaks, so the README reads as its
    # source strings do.
    printf 'color_scheme = "%s"\nlanguage = "en_US"\n' "$look" >"$dir/settings.toml"
    # The arguments after the binary go into the app's argument domain,
    # which wins over its saved defaults for this run alone: mail rather
    # than the calendar, an orange accent as PRODUCT.md asks of
    # screenshots, no restored windows and no Sparkle check.
    IRIS_DEMO=1 \
        IRIS_STORE="$dir/mailrs.db" \
        IRIS_OPEN="$conversation" \
        MAILRS_CONFIG="$dir/config.toml" \
        MAILRS_SETTINGS="$dir/settings.toml" \
        MAILRS_DATA_DIR="$dir" \
        MAILRS_DEMO_FULL_CONSENT=1 \
        XDG_CONFIG_HOME="$dir/config" \
        XDG_DATA_HOME="$dir/data" \
        XDG_CACHE_HOME="$dir/cache" \
        "$app/Contents/MacOS/Iris" \
        -space mail \
        -sidebar.folded "" \
        -AppleAccentColor 1 \
        -ApplePersistenceIgnoreState YES \
        -SUEnableAutomaticChecks NO \
        >"$dir/iris.log" 2>&1 &
    pid=$!
    until window=$("$work/window" window "$pid"); do
        tries=$((tries + 1))
        if [ "$tries" -gt 60 ] || ! kill -0 "$pid" 2>/dev/null; then
            echo "macos-screenshots.sh: the $look window never opened; see its log:" >&2
            cat "$dir/iris.log" >&2
            exit 1
        fi
        sleep 0.5
    done
    # Lets the first sync settle and the conversation's page draw.
    sleep 6
    screencapture -x -l "$window" "$out/iris-$look.png"
    kill "$pid"
    wait "$pid" 2>/dev/null || true
    pid=
    echo "Wrote docs/screenshots/iris-$look.png"
}

mkdir -p "$out"
shoot dark
shoot light
