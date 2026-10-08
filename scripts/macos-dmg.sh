#!/usr/bin/env bash
# Builds Iris-<version>.dmg: a release Iris.app that carries its own GTK,
# signed, in a disk image with a link to Applications, for testers to drag
# across. It lands in target/dmg/.
#
#   scripts/macos-dmg.sh
#
# The libraries come from Homebrew, which builds them for the macOS it runs
# on, so the app needs that macOS or later on Apple silicon; Info.plist says
# so, and an older Mac refuses it with a message rather than a crash.
#
# Signing uses IRIS_SIGN_IDENTITY, by default the self-signed "albz Code
# Signing". That is not a Developer ID, so a tester opens Iris once through
# System Settings > Privacy & Security > Open Anyway. The OAuth clients come
# from packaging/secrets.env, as for macos-install.sh.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f packaging/secrets.env ]; then
    set -a
    # shellcheck disable=SC1091
    . packaging/secrets.env
    set +a
fi
for tool in hdiutil codesign install_name_tool gdk-pixbuf-query-loaders python3; do
    command -v "$tool" >/dev/null || { echo "$tool is missing" >&2; exit 1; }
done

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
CARGO_INCREMENTAL=0 cargo +1.98 build -p mailrs --release
built="$(scripts/macos-bundle.sh release)"

out=target/dmg
staging="$out/staging"
rm -rf "$staging"
mkdir -p "$staging"
# A copy, so the bundle macos-install.sh uses keeps loading Homebrew's GTK.
ditto "$built" "$staging/Iris.app"
app="$staging/Iris.app"

python3 scripts/macos-bundle-libs.py "$app"

# The oldest macOS the bundled libraries were built for.
minimum="$(otool -l "$app/Contents/Frameworks/libgtk-4.1.dylib" | awk '/minos/ {print $2; exit}')"
plutil -replace LSMinimumSystemVersion -string "$minimum" "$app/Contents/Info.plist"

# Every library changed under install_name_tool lost its signature, and
# Apple silicon runs no unsigned code, so each is signed again, inside out:
# the libraries and loaders first, the app last.
identity="${IRIS_SIGN_IDENTITY:-albz Code Signing}"
if ! security find-identity -p codesigning | grep -qF "\"$identity\""; then
    echo "No signing identity \"$identity\"; signing ad hoc." >&2
    identity=-
fi
find "$app/Contents/Frameworks" "$app/Contents/Resources/lib" -type f \( -name '*.dylib' -o -name '*.so' \) -print0 |
    xargs -0 codesign --force --sign "$identity" >&2
codesign --force --sign "$identity" --identifier io.github.AlbertoBarrago.Iris "$app" >&2
codesign --verify --strict --deep "$app"

ln -s /Applications "$staging/Applications"
dmg="$out/Iris-$version.dmg"
rm -f "$dmg"
hdiutil create -volname "Iris $version" -srcfolder "$staging" -fs HFS+ -format UDZO -ov "$dmg" >/dev/null
rm -rf "$staging"
echo "$dmg (macOS $minimum or later, Apple silicon)"
