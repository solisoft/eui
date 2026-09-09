#!/bin/bash
# Wrap a macOS executable in a .app bundle, and a .dmg beside it.
#
#   wrap-macos-app.sh <binary> <AppName> <bundle-id> <out-dir>
#
# The .app is the unit macOS actually understands: a bare executable cannot
# carry an icon, a display name, or a URL scheme. The DMG is only made when
# hdiutil is present, so this is still runnable on Linux for a smoke test.

set -euo pipefail

BINARY="${1:?binary path}"
APP_NAME="${2:?app name}"
BUNDLE_ID="${3:?bundle id}"
OUT_DIR="${4:?output dir}"
VERSION="${VERSION:-0.1.0}"

[ -f "$BINARY" ] || { echo "wrap-macos-app: no binary at $BINARY" >&2; exit 1; }

APP="$OUT_DIR/$APP_NAME.app"
CONTENTS="$APP/Contents"
rm -rf "$APP"
mkdir -p "$CONTENTS/MacOS" "$CONTENTS/Resources"

cp "$BINARY" "$CONTENTS/MacOS/$APP_NAME"
chmod +x "$CONTENTS/MacOS/$APP_NAME"

cat > "$CONTENTS/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleExecutable</key><string>$APP_NAME</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>$APP_NAME</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
</dict>
</plist>
PLIST

printf 'APPL????' > "$CONTENTS/PkgInfo"

# A desktop artifact carries its application encrypted — there is no
# unencrypted desktop build — so it needs its key at every launch, not only
# at build time. Without one it starts, fails to unlock, and exits: from the
# Finder that reads as an icon that flashes and vanishes, with the reason on
# a stderr nobody sees. A `.env` beside the executable is the documented
# place for it (standalone.rs resolves env_dir from current_exe).
#
# This does put the key inside the artifact. That is only acceptable because
# the caller generates a throwaway key per build for a demo, so it guards
# nothing. Never pass a key here that also unlocks a real installation.
if [ -n "${BUNDLE_KEY:-}" ]; then
  printf 'SOLI_BUNDLE_KEY=%s\n' "$BUNDLE_KEY" > "$CONTENTS/MacOS/.env"
  chmod 600 "$CONTENTS/MacOS/.env"
  echo "wrap-macos-app: wrote the launch key beside the executable"
fi

# Sign the bundle, ad-hoc, with no certificate. Assembling a .app by copying
# a binary and writing a plist leaves the bundle itself unsigned, and Apple
# Silicon refuses to run an unsigned bundle outright — reporting it to the
# user as "damaged", which sends them looking for a corrupt download. This
# is not notarisation and does not clear Gatekeeper's quarantine on a
# download; it turns a hard refusal into the ordinary unidentified-developer
# prompt, which right-click - Open can answer.
if command -v codesign >/dev/null 2>&1; then
  codesign --force --deep --sign - "$APP" 2>&1 | sed 's/^/wrap-macos-app: /' || {
    echo "wrap-macos-app: codesign failed" >&2; exit 1; }
  codesign --verify --deep --strict "$APP" || {
    echo "wrap-macos-app: signature did not verify" >&2; exit 1; }
  echo "wrap-macos-app: ad-hoc signed"
fi

echo "wrap-macos-app: built $APP"

if command -v hdiutil >/dev/null 2>&1; then
  STAGE="$OUT_DIR/.dmg-stage"
  rm -rf "$STAGE"; mkdir -p "$STAGE"
  cp -R "$APP" "$STAGE/"
  hdiutil create -volname "$APP_NAME" -srcfolder "$STAGE" -ov -format UDZO \
    "$OUT_DIR/$APP_NAME-aarch64-macos.dmg" >/dev/null
  rm -rf "$STAGE"
  echo "wrap-macos-app: built $OUT_DIR/$APP_NAME-aarch64-macos.dmg"
fi

( cd "$OUT_DIR" && zip -qr "$APP_NAME-aarch64-macos.zip" "$APP_NAME.app" )
echo "wrap-macos-app: built $OUT_DIR/$APP_NAME-aarch64-macos.zip"
