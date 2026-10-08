#!/usr/bin/env bash
# Wraps a built iris binary in Iris.app, so macOS shows Iris's name and icon
# in the Dock, the app switcher and the menu bar instead of a bare exec.
#
#   scripts/macos-bundle.sh [debug|release]     default: debug
#
# The bundle lands next to the binary, in target/<profile>/Iris.app. It
# still loads GTK from Homebrew, so it runs on this Mac only; copying the
# libraries in, signing and notarizing come later.
set -euo pipefail

cd "$(dirname "$0")/.."

profile="${1:-debug}"
binary="target/$profile/iris"
app="target/$profile/Iris.app"
svg="app/data/icons/scalable/apps/io.github.AlbertoBarrago.Iris.svg"

[ -x "$binary" ] || { echo "No $binary; build it first." >&2; exit 1; }
command -v rsvg-convert >/dev/null || { echo "rsvg-convert is missing: brew install librsvg" >&2; exit 1; }

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
# A copy, not a link: macOS takes the bundle a process belongs to from the
# executable's real path. The old file goes first, so a copy of Iris still
# running from it keeps its pages instead of being overwritten in place,
# which macOS answers by killing it.
rm -f "$app/Contents/MacOS/iris"
cp "$binary" "$app/Contents/MacOS/iris"

icns="$app/Contents/Resources/Iris.icns"
if [ ! -f "$icns" ] || [ "$svg" -nt "$icns" ]; then
    iconset="$(mktemp -d)/Iris.iconset"
    mkdir -p "$iconset"
    for size in 16 32 128 256 512; do
        rsvg-convert -w "$size" -h "$size" "$svg" -o "$iconset/icon_${size}x${size}.png"
        rsvg-convert -w $((size * 2)) -h $((size * 2)) "$svg" -o "$iconset/icon_${size}x${size}@2x.png"
    done
    iconutil -c icns "$iconset" -o "$icns"
    rm -r "$(dirname "$iconset")"
fi

# The translations, where language.rs looks inside a bundle. Compiled from
# po/ on every run, so a bundle never carries a stale language.
command -v msgfmt >/dev/null || { echo "msgfmt is missing: brew install gettext" >&2; exit 1; }
rm -rf "$app/Contents/Resources/locale"
for po in po/*.po; do
    lang="$(basename "$po" .po)"
    mkdir -p "$app/Contents/Resources/locale/$lang/LC_MESSAGES"
    msgfmt -o "$app/Contents/Resources/locale/$lang/LC_MESSAGES/iris.mo" "$po"
done

# Sign it, so the Keychain recognises each new build as the same app and
# keeps the "Always Allow" answers. IRIS_SIGN_IDENTITY names the identity;
# by default the self-signed "albz Code Signing", when it is there. No
# hardened runtime yet: it would refuse Homebrew's GTK libraries, which a
# distributable bundle will carry inside and sign instead.
identity="${IRIS_SIGN_IDENTITY:-albz Code Signing}"
if security find-identity -p codesigning | grep -qF "\"$identity\""; then
    codesign --force --sign "$identity" --identifier io.github.AlbertoBarrago.Iris "$app" >&2
else
    echo "No signing identity \"$identity\"; the bundle stays unsigned." >&2
fi

# Tell Launch Services the bundle changed, so the Dock picks up a new icon.
touch "$app"
echo "$app"
