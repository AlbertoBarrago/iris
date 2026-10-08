#!/bin/sh
# Installs or updates Iris on a Mac, for testers:
#
#   curl -fsSL https://albz.it/iris/install.sh | sh
#
# It reads the newest DMG from Iris's update feed, copies Iris into
# /Applications and opens it. The test builds carry no Apple Developer ID
# yet, so macOS would stop a copy downloaded in a browser until the person
# chose Open Anyway; a file curl downloads carries no quarantine mark, and
# the script clears every extended attribute from the app anyway, so Iris
# opens at once.
set -eu

feed="${IRIS_APPCAST_URL:-https://albz.it/iris/appcast.xml}"
target=/Applications/Iris.app

say() { printf '%s\n' "$*"; }
fail() { printf 'Iris was not installed: %s\n' "$*" >&2; exit 1; }

[ "$(uname -s)" = Darwin ] || fail "this installer is for macOS."
[ "$(uname -m)" = arm64 ] || fail "Iris needs a Mac with Apple silicon."
major="$(sw_vers -productVersion | cut -d. -f1)"
[ "$major" -ge 26 ] || fail "Iris needs macOS 26 or later; this Mac runs $(sw_vers -productVersion)."
[ -w /Applications ] || fail "your account cannot write to /Applications."

# The feed lists the newest build first.
url="$(curl -fsSL "$feed" | grep -o 'url="[^"]*\.dmg"' | head -n 1 | cut -d '"' -f 2)"
[ -n "$url" ] || fail "the update feed at $feed lists no DMG."

work="$(mktemp -d)"
mount=""
cleanup() {
    [ -n "$mount" ] && hdiutil detach -quiet "$mount" 2>/dev/null || true
    rm -rf "$work"
}
trap cleanup EXIT

say "Downloading $(basename "$url")…"
curl -fSL --progress-bar -o "$work/Iris.dmg" "$url"
mount="$(hdiutil attach -nobrowse -readonly -noautoopen "$work/Iris.dmg" | awk -F '\t' '/\/Volumes\// {print $NF; exit}')"
[ -d "$mount/Iris.app" ] || fail "the DMG holds no Iris.app."

if pgrep -xq iris; then
    say "Quitting the running Iris…"
    osascript -e 'tell application id "io.github.AlbertoBarrago.Iris" to quit' >/dev/null 2>&1 || true
    sleep 2
fi

say "Installing into /Applications…"
rm -rf "$target"
ditto "$mount/Iris.app" "$target"
# xattr reports an error for an attribute macOS keeps, such as
# com.apple.provenance, or a link inside the bundle it cannot follow, yet
# clears the rest; what matters is that the quarantine mark is gone.
xattr -cr "$target" 2>/dev/null || true
if xattr -p com.apple.quarantine "$target" >/dev/null 2>&1; then
    fail "macOS kept its quarantine mark on Iris; run: xattr -cr $target"
fi

say "Done. Opening Iris."
open "$target"
