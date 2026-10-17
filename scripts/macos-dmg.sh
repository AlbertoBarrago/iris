#!/usr/bin/env bash
# Builds Iris-<version>.dmg: a release Iris.app from Xcode, signed, in a
# disk image with a link to Applications, for people to drag across. It
# lands in target/dmg/.
#
#   scripts/macos-dmg.sh
#
# Xcode signs the app with the Apple Development identity project.yml
# names, which carries a Team ID, so the Keychain's "Always Allow" holds
# across updates. It is not a Developer ID, so a new install opens once
# through System Settings > Privacy & Security > Open Anyway. The OAuth
# clients come from packaging/secrets.env, as for macos-install.sh.
#
# The app carries Sparkle, which checks the appcast Info.plist names. The
# script adds the public half of the EdDSA key Sparkle's generate_keys
# keeps in the login Keychain, signs the app again, and writes
# target/dmg/appcast.xml for every DMG in target/dmg, each pointing at
# IRIS_DOWNLOAD_URL, by default this version's GitHub release.
# scripts/macos-publish.sh puts them there. The release workflow has no
# login Keychain to read the key from, so it passes the public half in
# IRIS_SPARKLE_PUBLIC_KEY and the private half in
# IRIS_SPARKLE_PRIVATE_KEY. Sparkle offers a DMG only when
# its CFBundleVersion, the version in Cargo.toml, is newer than the
# running one.
set -euo pipefail

cd "$(dirname "$0")/.."

if [ -f packaging/secrets.env ]; then
    set -a
    # shellcheck disable=SC1091
    . packaging/secrets.env
    set +a
fi
for tool in hdiutil codesign plutil xcodebuild; do
    command -v "$tool" >/dev/null || { echo "$tool is missing" >&2; exit 1; }
done

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
appcast_url="${IRIS_APPCAST_URL:-https://albz.it/iris/appcast.xml}"
download_url="${IRIS_DOWNLOAD_URL:-https://github.com/AlbertoBarrago/iris/releases/download/v$version/}"

# Sparkle, fetched once into target/ and checked against its known digest.
sparkle_version=2.10.0
sparkle_sha256=c2bf58aa8387266ac179357b1415d6f2635f044da8be41042af32425dae6da0c
sparkle="target/sparkle-$sparkle_version"
if [ ! -d "$sparkle/Sparkle.framework" ] || [ ! -x "$sparkle/bin/generate_appcast" ]; then
    rm -rf "$sparkle"
    mkdir -p "$sparkle"
    curl -sSfL -o "$sparkle/Sparkle.tar.xz" \
        "https://github.com/sparkle-project/Sparkle/releases/download/$sparkle_version/Sparkle-$sparkle_version.tar.xz"
    echo "$sparkle_sha256  $sparkle/Sparkle.tar.xz" | shasum -a 256 -c - >/dev/null ||
        { echo "Sparkle's download does not match its digest" >&2; rm -rf "$sparkle"; exit 1; }
    tar -xJf "$sparkle/Sparkle.tar.xz" -C "$sparkle"
fi
# The public half of the signing key; the private half stays in the Keychain.
public_key="${IRIS_SPARKLE_PUBLIC_KEY:-}"
if [ -z "$public_key" ]; then
    public_key="$("$sparkle/bin/generate_keys" -p 2>/dev/null)" ||
        { echo "No Sparkle key in the Keychain; run $sparkle/bin/generate_keys once" >&2; exit 1; }
fi

command -v xcodegen >/dev/null || { echo "xcodegen is missing: brew install xcodegen" >&2; exit 1; }
# xcodegen lists the files in macos/Iris when it runs, and the Swift
# bindings land there only when the core is built. A fresh checkout has
# none yet, so the core is built first; Xcode's own run of the script
# then finds nothing to do.
scripts/build-ffi.sh release
(cd macos && xcodegen generate -q &&
    xcodebuild -project Iris.xcodeproj -scheme Iris -configuration Release -derivedDataPath build build -quiet)
built="macos/build/Build/Products/Release/Iris.app"

out=target/dmg
staging="$out/staging"
rm -rf "$staging"
mkdir -p "$staging"
ditto "$built" "$staging/Iris.app"
app="$staging/Iris.app"

# The version people see and the one Sparkle compares, both Cargo.toml's.
plist="$app/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$version" "$plist"
plutil -replace CFBundleVersion -string "$version" "$plist"
plutil -replace SUFeedURL -string "$appcast_url" "$plist"
plutil -replace SUPublicEDKey -string "$public_key" "$plist"
minimum="$(plutil -extract LSMinimumSystemVersion raw "$plist")"

# The Info.plist changed under Xcode's signature, so the app is signed
# again, with the identity Xcode used.
identity="${IRIS_SIGN_IDENTITY:-$(codesign -dvv "$built" 2>&1 | sed -n 's/^Authority=//p' | head -1)}"
codesign --force --sign "$identity" --preserve-metadata=entitlements,flags \
    --identifier io.github.AlbertoBarrago.Iris "$app" >&2
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
appcast=("$sparkle/bin/generate_appcast" --download-url-prefix "$download_url"
    --maximum-versions 1 --maximum-deltas 3)
if [ -n "${IRIS_SPARKLE_PRIVATE_KEY:-}" ]; then
    # Read from standard input, so the key never lands in a file.
    printf '%s' "$IRIS_SPARKLE_PRIVATE_KEY" | "${appcast[@]}" --ed-key-file - "$out" >&2
else
    "${appcast[@]}" "$out" >&2
fi
echo "$dmg (macOS $minimum or later, Apple silicon)"
echo "Publish it with: scripts/macos-publish.sh"
