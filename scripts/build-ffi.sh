#!/usr/bin/env bash
# Builds the Rust core for the SwiftUI app: the static library in
# target/<profile>/, its C header and module map in target/ffi/headers/,
# and its Swift bindings in macos/Iris/Generated/. Xcode runs this before
# it compiles the app, and it runs by hand as well:
#
#   scripts/build-ffi.sh            a debug build
#   scripts/build-ffi.sh release    a release build
set -euo pipefail

cd "$(dirname "$0")/.."
profile="${1:-debug}"
case $profile in
debug) flag= ;;
release) flag=--release ;;
*) echo "build-ffi.sh: debug or release, not $profile" >&2; exit 2 ;;
esac

# The OAuth clients go into the build from packaging/secrets.env, as for
# the GTK app's scripts; without them Google and Microsoft accounts
# cannot connect.
if [ -f packaging/secrets.env ]; then
    set -a
    # shellcheck disable=SC1091
    . packaging/secrets.env
    set +a
fi
# Xcode runs scripts with a PATH of its own, without cargo.
export PATH="$HOME/.cargo/bin:$PATH"
cargo +1.98 build -p mailrs-ffi $flag
lib="target/$profile"
out=target/ffi
mkdir -p "$out/headers"
cargo +1.98 run -q -p mailrs-ffi --bin uniffi-bindgen $flag -- \
    generate --library "$lib/libmailrs_ffi.dylib" --language swift --out-dir "$out/generated"
cp "$out"/generated/*.h "$out/headers/"
# An XCFramework names its module map module.modulemap.
cp "$out"/generated/*.modulemap "$out/headers/module.modulemap"
mkdir -p macos/Iris/Generated
cp "$out"/generated/*.swift macos/Iris/Generated/
echo "The Rust core and macos/Iris/Generated are up to date ($profile)."
