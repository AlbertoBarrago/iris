#!/usr/bin/env bash
# Draws Iris's app icon as an Icon Composer document,
# macos/Iris/AppIcon.icon: the winged envelope that scripts/iris-art.py
# writes is its one layer, and macOS draws the squircle under it in a fill
# that is violet by default and near black when icons are dark. Clear and
# tinted icons come from the same document. Run it again after changing
# the art.
#
# The layer is a PNG rather than the SVG: Icon Composer's SVG renderer
# washes out the wing's gradient.
#
#   scripts/macos-icon.sh
set -euo pipefail

cd "$(dirname "$0")/.."
command -v rsvg-convert >/dev/null || { echo "rsvg-convert is missing: brew install librsvg" >&2; exit 1; }
doc=macos/Iris/AppIcon.icon
mkdir -p "$doc/Assets"
rsvg-convert -w 1024 -h 1024 data/icons/app-layer.svg -o "$doc/Assets/envelope.png"

# #5b3fd1 to #1b1340, the squircle iris-art.py draws for the plain icon,
# and #2b2b30 to #050508 for dark icons, both corner to corner.
cat > "$doc/icon.json" <<'JSON'
{
  "fill-specializations" : [
    {
      "value" : {
        "linear-gradient" : [
          "srgb:0.35686,0.24706,0.81961,1.00000",
          "srgb:0.10588,0.07451,0.25098,1.00000"
        ],
        "orientation" : {
          "start" : { "x" : 0, "y" : 0 },
          "stop" : { "x" : 1, "y" : 1 }
        }
      }
    },
    {
      "appearance" : "dark",
      "value" : {
        "linear-gradient" : [
          "srgb:0.16863,0.16863,0.18824,1.00000",
          "srgb:0.01961,0.01961,0.03137,1.00000"
        ],
        "orientation" : {
          "start" : { "x" : 0, "y" : 0 },
          "stop" : { "x" : 1, "y" : 1 }
        }
      }
    }
  ],
  "groups" : [
    {
      "layers" : [
        {
          "glass" : false,
          "image-name" : "envelope.png",
          "name" : "envelope"
        }
      ],
      "shadow" : {
        "kind" : "neutral",
        "opacity" : 0.5
      },
      "translucency" : {
        "enabled" : false,
        "value" : 0.5
      }
    }
  ],
  "supported-platforms" : {
    "squares" : [
      "macOS"
    ]
  }
}
JSON
echo "Wrote $doc"
