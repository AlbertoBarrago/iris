#!/usr/bin/env bash
# Builds Iris on macOS and runs it, replacing any copy of the debug build
# already running. Two copies of the app share one store, so a stale one is quit
# first rather than left to refuse the new one.
#
#   scripts/dev-macos.sh            run with your accounts, from Iris.app
#   scripts/dev-macos.sh --demo     run on the sample accounts
#
# The OAuth client ids come from packaging/secrets.env when it exists.
# Debug info is kept to line tables, which keeps target/ a few GB smaller.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f packaging/secrets.env ]; then
    set -a
    # shellcheck disable=SC1091
    . packaging/secrets.env
    set +a
fi

# GLib looks for icons, GSettings schemas and the like in XDG_DATA_DIRS,
# and a value set by the terminal replaces its own default, which on
# Homebrew is /opt/homebrew/share. Without it GTK finds neither the Adwaita
# icons nor its schemas.
homebrew_share="$(brew --prefix 2>/dev/null || echo /opt/homebrew)/share"
export XDG_DATA_DIRS="$homebrew_share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"

export CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0
cargo +1.98 build -p mailrs

# Run from inside Iris.app, so the Dock and the app switcher show its name
# and icon rather than a bare exec.
bundle="$(scripts/macos-bundle.sh debug)"
# A copy started by hand as ./target/debug/iris counts too.
running="target/debug/(Iris\.app/Contents/MacOS/)?iris( |$)"
if pgrep -f "$running" >/dev/null; then
    pkill -f "$running"
    # Give it a moment to let go of the store before the new copy opens it.
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        pgrep -f "$running" >/dev/null || break
        sleep 0.3
    done
    if pgrep -f "$running" >/dev/null; then
        echo "The old copy did not quit; stop it and run this again." >&2
        exit 1
    fi
fi

# Launch Services starts it, as a double click would, so macOS treats it
# as the app in front: its menu bar shows when its window is chosen. A
# process started straight from the shell stays the terminal's. `open`
# hands the app none of this shell's environment, so what GLib needs goes
# along with --env, and the log comes back here through a file.
log="$PWD/target/debug/iris.log"
: > "$log"
env_args=(--env "XDG_DATA_DIRS=$XDG_DATA_DIRS")
[ -n "${RUST_LOG:-}" ] && env_args+=(--env "RUST_LOG=$RUST_LOG")
tail -n +1 -f "$log" &
tail_pid=$!
trap 'kill "$tail_pid" 2>/dev/null; pkill -f "$running" 2>/dev/null; exit 130' INT TERM
open -n -W --stdout "$log" --stderr "$log" "${env_args[@]}" "$bundle" --args "$@"
kill "$tail_pid" 2>/dev/null
