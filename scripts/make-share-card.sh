#!/bin/bash
# Render the site's link-preview picture from its source.
#
#   scripts/make-share-card.sh            # www/share/card.html -> www/public/images/share/eui.png
#
# The card is HTML so that it is drawn in the site's own fonts and colours
# and can be edited like the rest of the site; this turns it into the one
# PNG every unfurler (Open Graph, X, LinkedIn, Slack, Discord) fetches.
# Needs a Chromium (or Chrome) and the network, for Google Fonts. Commit the
# source and the PNG together.

set -euo pipefail

here=$(cd "$(dirname "$0")/.." && pwd)
src="$here/www/share/card.html"
out="$here/www/public/images/share/eui.png"

chrome=""
for c in chromium chromium-browser google-chrome google-chrome-stable; do
  if command -v "$c" >/dev/null 2>&1; then chrome="$c"; break; fi
done
[ -n "$chrome" ] || { echo "make-share-card: no Chromium or Chrome on the PATH" >&2; exit 1; }

mkdir -p "$(dirname "$out")"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# The budget gives the web fonts time to arrive before the picture is taken;
# without it the card is photographed in the fallback faces.
"$chrome" --headless=new --disable-gpu --hide-scrollbars --no-first-run \
  --user-data-dir="$tmp/profile" --force-device-scale-factor=1 \
  --window-size=1200,630 --virtual-time-budget=5000 \
  --screenshot="$tmp/card.png" "file://$src" >/dev/null 2>&1

[ -s "$tmp/card.png" ] || { echo "make-share-card: $chrome wrote no picture" >&2; exit 1; }
mv "$tmp/card.png" "$out"
echo "make-share-card: wrote $out ($(stat -c %s "$out" 2>/dev/null || stat -f %z "$out") bytes)"
