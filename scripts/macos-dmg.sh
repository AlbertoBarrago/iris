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
#
# The bundle carries Sparkle, which checks IRIS_APPCAST_URL for updates. The
# script also writes target/dmg/appcast.xml for every DMG in target/dmg,
# signed with the EdDSA key Sparkle's generate_keys keeps in the login
# Keychain, each pointing at IRIS_DOWNLOAD_URL, by default this version's
# GitHub release. scripts/macos-publish.sh puts them there. Sparkle offers a
# DMG only when its CFBundleVersion, the version in Cargo.toml, is newer
# than the running one.
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
appcast_url="${IRIS_APPCAST_URL:-https://albz.it/iris/appcast.xml}"
download_url="${IRIS_DOWNLOAD_URL:-https://github.com/AlbertoBarrago/iris/releases/download/v$version/}"

# Sparkle, fetched once into target/ and checked against its known digest.
sparkle_version=2.10.0
sparkle_sha256=c2bf58aa8387266ac179357b1415d6f2635f044da8be41042af32425dae6da0c
sparkle="target/sparkle-$sparkle_version"
if [ ! -d "$sparkle/Sparkle.framework" ]; then
    rm -rf "$sparkle"
    mkdir -p "$sparkle"
    curl -sSfL -o "$sparkle/Sparkle.tar.xz" \
        "https://github.com/sparkle-project/Sparkle/releases/download/$sparkle_version/Sparkle-$sparkle_version.tar.xz"
    echo "$sparkle_sha256  $sparkle/Sparkle.tar.xz" | shasum -a 256 -c - >/dev/null ||
        { echo "Sparkle's download does not match its digest" >&2; rm -rf "$sparkle"; exit 1; }
    tar -xJf "$sparkle/Sparkle.tar.xz" -C "$sparkle"
fi
# The public half of the signing key; the private half stays in the Keychain.
public_key="$("$sparkle/bin/generate_keys" -p 2>/dev/null)" ||
    { echo "No Sparkle key in the Keychain; run $sparkle/bin/generate_keys once" >&2; exit 1; }

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

# Sparkle goes in beside GTK. Iris is not sandboxed, so Sparkle's XPC
# services, which only a sandboxed app needs, stay out.
ditto "$sparkle/Sparkle.framework" "$app/Contents/Frameworks/Sparkle.framework"
rm -rf "$app/Contents/Frameworks/Sparkle.framework/Versions/B/XPCServices"
# The framework's top-level link to them would be left pointing at nothing,
# which xattr -cr trips on.
rm -f "$app/Contents/Frameworks/Sparkle.framework/XPCServices"
plist="$app/Contents/Info.plist"
plutil -replace SUFeedURL -string "$appcast_url" "$plist"
plutil -replace SUPublicEDKey -string "$public_key" "$plist"
plutil -replace SUEnableAutomaticChecks -bool true "$plist"

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
framework="$app/Contents/Frameworks/Sparkle.framework"
codesign --force --sign "$identity" "$framework/Versions/B/Autoupdate" >&2
codesign --force --sign "$identity" "$framework/Versions/B/Updater.app" >&2
codesign --force --sign "$identity" "$framework" >&2
codesign --force --sign "$identity" --identifier io.github.AlbertoBarrago.Iris "$app" >&2
codesign --verify --strict --deep "$app"

ln -s /Applications "$staging/Applications"
dmg="$out/Iris-$version.dmg"
rm -f "$dmg"
hdiutil create -volname "Iris $version" -srcfolder "$staging" -fs HFS+ -format UDZO -ov "$dmg" >/dev/null
rm -rf "$staging"
# The testers' installer goes up beside the DMGs.
cp packaging/macos/install.sh "$out/install.sh"
# The feed offers the newest build alone, with deltas from the three
# before it, all on this version's release, and testers download a few MB.
"$sparkle/bin/generate_appcast" --download-url-prefix "$download_url" \
    --maximum-versions 1 --maximum-deltas 3 "$out" >&2
echo "$dmg (macOS $minimum or later, Apple silicon)"
echo "Publish it with: scripts/macos-publish.sh"
