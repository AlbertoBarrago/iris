#!/usr/bin/env bash
# Puts the newest macOS build into the albz.it site's folder, ready to
# deploy: the DMG and the deltas target/dmg/appcast.xml names, the appcast
# and the installer. Older DMGs and deltas leave the folder, so the site
# carries one build, and the download button points at it.
#
#   scripts/macos-publish.sh ~/Projects/albertobarrago.github.io
#
# Then commit and push the site, and run its `npm run deploy`.
set -euo pipefail

cd "$(dirname "$0")/.."
site="${1:?the albz.it site folder}"
target="$site/static/iris"
feed=target/dmg/appcast.xml
[ -d "$target" ] || { echo "No $target" >&2; exit 1; }
[ -f "$feed" ] || { echo "No $feed; run scripts/macos-dmg.sh first" >&2; exit 1; }

# The files the feed points at, by name.
files="$(grep -o 'url="[^"]*"' "$feed" | sed -E 's|.*/([^/"]+)"$|\1|' | sort -u)"
dmg="$(grep -o 'url="[^"]*\.dmg"' "$feed" | head -n 1 | sed -E 's|.*/([^/"]+)"$|\1|')"
version="$(sed -E 's/^Iris-(.*)\.dmg$/\1/' <<< "$dmg")"
[ -n "$dmg" ] || { echo "$feed names no DMG" >&2; exit 1; }

find "$target" -maxdepth 1 \( -name 'Iris-*.dmg' -o -name 'Iris*.delta' \) -delete
for file in $files; do
    cp "target/dmg/$file" "$target/"
done
cp "$feed" packaging/macos/install.sh "$target/"

# The page's one download button names the build.
sed -i '' -E "s|href=\"Iris-[0-9.]+\.dmg\">Download Iris [0-9.]+<|href=\"$dmg\">Download Iris $version<|" "$target/index.html"
grep -q "href=\"$dmg\"" "$target/index.html" || { echo "The download button was not found in index.html" >&2; exit 1; }

echo "Staged Iris $version in $target:"
ls -lh "$target" | awk 'NR > 1 {print "  " $5 "  " $NF}'
