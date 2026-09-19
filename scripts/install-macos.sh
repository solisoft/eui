#!/bin/bash
# Install EUI.app into /Applications, without the quarantine dance.
#
#   ./scripts/install-macos.sh [tag]        # default: rolling
#   curl -fsSL https://raw.githubusercontent.com/solisoft/eui/main/scripts/install-macos.sh | bash
#
# com.apple.quarantine is not something the app carries. It is written by
# whatever downloaded it — a browser, Mail, AirDrop all set it, because they
# opt into it — and Gatekeeper then holds the first launch of anything
# wearing it. The build is ad-hoc signed, not notarised, so that hold is a
# refusal rather than a prompt, and it is written afresh onto every new
# download: that is why stripping it by hand never stays stripped.
#
# curl does not set it. Fetching the same zip this way leaves the attribute
# off from the start, so there is nothing to remove and nothing to remove
# again next time. The xattr call below is belt and braces, for a zip that
# arrived some other way.

set -euo pipefail

TAG="${1:-rolling}"
REPO="${EUI_REPO:-solisoft/eui}"
ASSET="EUI-aarch64-macos.zip"
URL="https://github.com/$REPO/releases/download/$TAG/$ASSET"
DEST="${EUI_DEST:-/Applications}"

[ "$(uname -s)" = "Darwin" ] || { echo "install-macos: this is for macOS" >&2; exit 1; }
# Only Apple Silicon is built. Rosetta would run an x86_64 binary and there
# is none; say so here rather than let the download succeed and the window
# never open.
[ "$(uname -m)" = "arm64" ] || { echo "install-macos: only Apple Silicon is built" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "install-macos: fetching $URL"
curl -fL --progress-bar -o "$TMP/$ASSET" "$URL"

# ditto, not unzip: a .app is a directory, and this is the unpacker that
# keeps its symlinks and its signature intact.
ditto -x -k "$TMP/$ASSET" "$TMP/unpacked"
APP="$TMP/unpacked/EUI.app"
[ -d "$APP" ] || { echo "install-macos: no EUI.app in $ASSET" >&2; exit 1; }

xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

# The signature has to still verify after the trip; a bundle that was
# rewritten in transit is the one case where "damaged" is the honest word.
codesign --verify --deep --strict "$APP" || {
  echo "install-macos: the signature did not verify" >&2; exit 1; }

# Replaced rather than written over: copying onto a live bundle leaves the
# old files that the new one no longer has.
if [ -d "$DEST/EUI.app" ]; then
  echo "install-macos: replacing $DEST/EUI.app"
  rm -rf "$DEST/EUI.app"
fi
ditto "$APP" "$DEST/EUI.app"

echo "install-macos: installed $DEST/EUI.app — open it from the Finder"
