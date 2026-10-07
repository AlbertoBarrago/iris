#!/usr/bin/env bash
# Removes what install.sh added, and what an install under an earlier name left.
# Your mail cache, config, and keyring entries stay.
set -euo pipefail

prefix="${PREFIX:-$HOME/.local}"
for id in io.github.AlbertoBarrago.Iris dev.penguinmail.PenguinMail dev.mailrs.Mailrs; do
    rm -f "$prefix/share/applications/$id.desktop" \
        "$prefix/share/icons/hicolor/scalable/apps/$id.svg" \
        "$prefix/share/icons/hicolor/symbolic/apps/$id-symbolic.svg" \
        "$prefix/share/icons/hicolor/16x16/apps/$id.svg" \
        "$HOME/.config/autostart/$id.desktop"
done
rm -f "$prefix/bin/iris" "$prefix/bin/iris-cli" \
    "$prefix/bin/mailrs" "$prefix/bin/mailrs-cli"
update-desktop-database "$prefix/share/applications" >/dev/null 2>&1 || true
echo "Removed Iris. Delete ~/.local/share/iris and ~/.config/iris too if you want your data gone."
