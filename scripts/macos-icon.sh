#!/usr/bin/env bash
# Draws Iris Next's app icon, the winged envelope that scripts/iris-art.py
# writes, into macos/Iris/Assets.xcassets/AppIcon.appiconset at every size
# macOS asks for. Run it again after changing the art.
#
#   scripts/macos-icon.sh
set -euo pipefail

cd "$(dirname "$0")/.."
command -v rsvg-convert >/dev/null || { echo "rsvg-convert is missing: brew install librsvg" >&2; exit 1; }
svg=app/data/icons/scalable/apps/io.github.AlbertoBarrago.Iris.svg
set_dir=macos/Iris/Assets.xcassets/AppIcon.appiconset
mkdir -p "$set_dir"
images=""
for size in 16 32 128 256 512; do
    for scale in 1 2; do
        pixels=$((size * scale))
        name="icon_${size}x${size}@${scale}x.png"
        rsvg-convert -w "$pixels" -h "$pixels" "$svg" -o "$set_dir/$name"
        images="$images{\"idiom\":\"mac\",\"size\":\"${size}x${size}\",\"scale\":\"${scale}x\",\"filename\":\"$name\"},"
    done
done
printf '{"images":[%s],"info":{"author":"xcode","version":1}}\n' "${images%,}" > "$set_dir/Contents.json"
printf '{"info":{"author":"xcode","version":1}}\n' > macos/Iris/Assets.xcassets/Contents.json
echo "Wrote $set_dir"
