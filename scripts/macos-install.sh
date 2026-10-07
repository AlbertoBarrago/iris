#!/usr/bin/env bash
# Builds a release Iris, signs it and installs it as /Applications/Iris.app,
# where Launchpad, Spotlight and the Dock find it like any other app.
#
# It still loads GTK from Homebrew, so it runs on this Mac; a bundle that
# carries its own libraries comes later. Your accounts and mail are the
# same as the development build's: both read the same folders.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f packaging/secrets.env ]; then
    set -a
    # shellcheck disable=SC1091
    . packaging/secrets.env
    set +a
fi

CARGO_INCREMENTAL=0 cargo +1.98 build -p mailrs --release
bundle="$(scripts/macos-bundle.sh release)"

installed=/Applications/Iris.app
running="^$installed/Contents/MacOS/iris"
if pgrep -f "$running" >/dev/null; then
    osascript -e 'tell application id "io.github.AlbertoBarrago.Iris" to quit' >/dev/null 2>&1 || true
    for _ in $(seq 1 20); do
        pgrep -f "$running" >/dev/null || break
        sleep 0.3
    done
    pkill -f "$running" 2>/dev/null || true
fi

# ditto keeps the signature, extended attributes and symlinks intact.
rm -rf "$installed"
ditto "$bundle" "$installed"
codesign --verify --strict "$installed"
echo "Installed $installed"
