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
