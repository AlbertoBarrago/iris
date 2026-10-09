#!/usr/bin/env bash
# Builds the Rust core for the SwiftUI app: the static library packed as
# macos/IrisCore.xcframework, and its Swift bindings in
# macos/Iris/Generated/. Xcode runs this before it compiles the app, and
# it runs by hand as well:
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

# Xcode runs scripts with a PATH of its own, without cargo.
export PATH="$HOME/.cargo/bin:$PATH"
cargo +1.98 build -p mailrs-ffi $flag
lib="target/$profile"
out=target/ffi
rm -rf "$out"
mkdir -p "$out/headers"
cargo +1.98 run -q -p mailrs-ffi --bin uniffi-bindgen $flag -- \
    generate --library "$lib/libmailrs_ffi.dylib" --language swift --out-dir "$out/generated"
cp "$out"/generated/*.h "$out/headers/"
# An XCFramework names its module map module.modulemap.
cp "$out"/generated/*.modulemap "$out/headers/module.modulemap"
rm -rf macos/IrisCore.xcframework
xcodebuild -create-xcframework -library "$lib/libmailrs_ffi.a" -headers "$out/headers" \
    -output macos/IrisCore.xcframework >/dev/null
mkdir -p macos/Iris/Generated
cp "$out"/generated/*.swift macos/Iris/Generated/
echo "macos/IrisCore.xcframework and macos/Iris/Generated are up to date ($profile)."
