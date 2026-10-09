#!/usr/bin/env bash
# Publishes the newest macOS build from target/dmg, the way
# scripts/macos-dmg.sh left it:
#
#   1. a GitHub release, tag v<version> on main, holding the DMG and the
#      deltas the appcast names, with the version's CHANGELOG.md section as
#      its notes;
#   2. the download page on GitHub Pages: site/, the appcast and the
#      installer, pushed as the one commit of the gh-pages branch.
#
#   scripts/macos-publish.sh
#
# The account's user site carries the albz.it domain, so this repository's
# Pages are served at https://albz.it/iris/, where the apps already out
# there look for their updates.
set -euo pipefail

cd "$(dirname "$0")/.."
repo=AlbertoBarrago/iris
feed=target/dmg/appcast.xml
command -v gh >/dev/null || { echo "gh is missing" >&2; exit 1; }
[ -f "$feed" ] || { echo "No $feed; run scripts/macos-dmg.sh first" >&2; exit 1; }

# The files the feed points at, by name, and the version they make up.
files="$(grep -o 'url="[^"]*"' "$feed" | sed -E 's|.*/([^/"]+)"$|\1|' | sort -u)"
dmg="$(grep -o 'url="[^"]*\.dmg"' "$feed" | head -n 1 | sed -E 's|.*/([^/"]+)"$|\1|')"
[ -n "$dmg" ] || { echo "$feed names no DMG" >&2; exit 1; }
version="$(sed -E 's/^Iris-(.*)\.dmg$/\1/' <<< "$dmg")"
tag="v$version"
download="https://github.com/$repo/releases/download/$tag"
grep -q "url=\"$download/$dmg\"" "$feed" ||
    { echo "$feed does not point at $download; build with scripts/macos-dmg.sh" >&2; exit 1; }

# The release. Its notes are the version's section of the changelog.
notes="$(mktemp)"
work="$(mktemp -d)"
trap 'rm -rf "$notes" "$work"' EXIT
awk -v heading="## $version " '
    index($0, heading) == 1 { found = 1; next }
    found && /^## / { exit }
    found { print }
' CHANGELOG.md > "$notes"
assets=()
for file in $files; do
    assets+=("target/dmg/$file")
done
if gh release view "$tag" --repo "$repo" >/dev/null 2>&1; then
    gh release upload "$tag" "${assets[@]}" --repo "$repo" --clobber
else
    gh release create "$tag" "${assets[@]}" --repo "$repo" --target main \
        --title "Iris $version" --notes-file "$notes"
fi

# The page, built afresh: GitHub Pages serves the branch's one commit, and
# the binaries live on the release rather than in its history.
cp -R site/. "$work/"
cp "$feed" packaging/macos/install.sh "$work/"
sed -i '' -E "s|href=\"[^\"]*Iris-[0-9.]+\.dmg\">Download Iris [0-9.]+<|href=\"$download/$dmg\">Download Iris $version<|" "$work/index.html"
grep -q "href=\"$download/$dmg\"" "$work/index.html" ||
    { echo "The download button was not found in site/index.html" >&2; exit 1; }
git -C "$work" init -q -b gh-pages
git -C "$work" add -A
git -C "$work" -c user.name="$(git config user.name)" -c user.email="$(git config user.email)" \
    commit -q -m "chore: publish Iris $version"
git -C "$work" push -q --force "https://github.com/$repo.git" gh-pages

# Pages turn on once, the first time there is a branch to serve. GitHub
# may turn them on by itself for a gh-pages branch, and then answers the
# request with a conflict, which is what was asked for.
if ! gh api "repos/$repo/pages" >/dev/null 2>&1; then
    gh api -X POST "repos/$repo/pages" -f "source[branch]=gh-pages" -f "source[path]=/" >/dev/null 2>&1 ||
        gh api "repos/$repo/pages" >/dev/null
fi

echo "Published Iris $version: https://github.com/$repo/releases/tag/$tag"
echo "The page and the feed update at https://albz.it/iris/ within a minute or two."
