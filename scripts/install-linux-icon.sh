#!/bin/bash
# Put the EUI icon and its launcher entry where a Linux desktop looks.
#
#   scripts/install-linux-icon.sh [--system]
#
# Per user by default, under ~/.local/share; --system writes /usr/share and
# wants root. Both are the freedesktop hicolor layout, which is the only
# thing a taskbar, a launcher and an alt-tab switcher agree on.
#
# A Wayland window cannot hand its own icon over — the compositor matches
# the window's app id against a .desktop file and takes the icon from
# there — so on Wayland this script is not a nicety, it is the icon.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$SCRIPT_DIR")"
PNGS="$ROOT/assets/icon/png"
DESKTOP="$ROOT/assets/eui.desktop"

if [ "${1-}" = "--system" ]; then
  PREFIX=/usr/share
else
  PREFIX="${XDG_DATA_HOME:-$HOME/.local/share}"
fi

[ -d "$PNGS" ] || { echo "install-linux-icon: no rasters at $PNGS — run scripts/make-icons.py" >&2; exit 1; }

for png in "$PNGS"/eui-*.png; do
  size="${png##*eui-}"
  size="${size%.png}"
  dir="$PREFIX/icons/hicolor/${size}x${size}/apps"
  mkdir -p "$dir"
  cp "$png" "$dir/eui.png"
done

# The scalable slot as well: a display that wants 96 px scales the SVG
# rather than one of the rasters above.
mkdir -p "$PREFIX/icons/hicolor/scalable/apps"
cp "$ROOT/assets/icon/eui.svg" "$PREFIX/icons/hicolor/scalable/apps/eui.svg"

mkdir -p "$PREFIX/applications"
cp "$DESKTOP" "$PREFIX/applications/eui.desktop"

# The cache is what GTK reads; without this the new icon shows up only
# after the next login, which reads as the install not having worked.
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$PREFIX/icons/hicolor" >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$PREFIX/applications" >/dev/null 2>&1 || true
fi

echo "install-linux-icon: icons in $PREFIX/icons/hicolor, launcher in $PREFIX/applications/eui.desktop"
