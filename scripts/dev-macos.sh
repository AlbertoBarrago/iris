#!/usr/bin/env bash
# Builds Iris on macOS and runs it, replacing any copy of the debug build
# already running. Two copies of the app share one store, so a stale one is quit
# first rather than left to refuse the new one.
#
#   scripts/dev-macos.sh            run with your accounts
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

export CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0
cargo +1.98 build -p mailrs

binary="$PWD/target/debug/iris"
# A copy started by hand as ./target/debug/iris counts too.
running="target/debug/iris( |$)"
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

exec "$binary" "$@"
