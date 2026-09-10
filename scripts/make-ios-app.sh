#!/bin/bash
# Build the iOS client and wrap it in a .app bundle you can install.
#
#   make-ios-app.sh [device|sim] [out-dir]
#
# An iOS .app is a directory with an executable and an Info.plist in it, and
# that is genuinely all: winit's `run_app` calls `UIApplicationMain` itself,
# so `crates/eui-ios`'s binary is a whole application and there is no Xcode
# project anywhere in this.
#
# **sim** builds for the simulator (`aarch64-apple-ios-sim`) and needs no
# signing, no account and no device — `xcrun simctl install booted` and it is
# there. That is the one to reach for first.
#
# **device** builds for `aarch64-apple-ios`. iOS will not run an unsigned
# binary, so the bundle is signed if `EUI_IOS_IDENTITY` names a signing
# identity (`security find-identity -v -p codesigning` lists them) and left
# unsigned otherwise — an unsigned bundle is still worth having, because it
# is what Xcode or `ios-deploy` will sign for you on the way to the device.
#
# `EUI_IOS_URL` bakes the session address into the binary. Without one the
# client opens the shell, which on a phone is a development convenience and
# not a design.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

KIND="${1:-sim}"
OUT_DIR="${2:-$ROOT/dist}"
APP_NAME="${APP_NAME:-EUI}"
BUNDLE_ID="${BUNDLE_ID:-com.soli.eui}"
VERSION="${VERSION:-0.1.0}"

case "$KIND" in
  sim)    TARGET="aarch64-apple-ios-sim"; PLATFORM="iPhoneSimulator" ;;
  device) TARGET="aarch64-apple-ios";     PLATFORM="iPhoneOS" ;;
  *) echo "make-ios-app: first argument is 'device' or 'sim', not '$KIND'" >&2; exit 2 ;;
esac

echo "make-ios-app: $KIND ($TARGET)"
rustup target add "$TARGET" >/dev/null 2>&1 || true
cargo build --release -p eui-ios --bin eui-ios --target "$TARGET"

BIN="$ROOT/target/$TARGET/release/eui-ios"
[ -f "$BIN" ] || { echo "make-ios-app: no binary at $BIN" >&2; exit 1; }

APP="$OUT_DIR/$APP_NAME.app"
rm -rf "$APP"
mkdir -p "$APP"
cp "$BIN" "$APP/$APP_NAME"
chmod +x "$APP/$APP_NAME"

# The icons, which iOS will not draw without. There is no asset catalogue
# here and none is needed for a home-screen icon: `CFBundleIconFiles` names
# the files by their point size and iOS picks the @2x or @3x beside them.
# They are full-bleed and opaque on purpose — see `ios_svg` in
# `scripts/make-icons.py` — because iOS masks an icon into its own squircle
# and composites what is left onto black.
ICONS="$ROOT/assets/icon/ios"
if [ -d "$ICONS" ]; then
  cp "$ICONS"/AppIcon*.png "$APP/"
  echo "make-ios-app: $(find "$ICONS" -name 'AppIcon*.png' | wc -l | tr -d ' ') icons"
else
  echo "make-ios-app: no icons at $ICONS — run scripts/make-icons.py" >&2
fi

# The minimum iOS will accept. `UILaunchScreen` is not decoration: without
# a launch screen iOS letterboxes the app to a phone-sized box in the middle
# of an iPad, and the window the client is handed is then the wrong size.
cat > "$APP/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key><string>$APP_NAME</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSRequiresIPhoneOS</key><true/>
  <key>MinimumOSVersion</key><string>13.0</string>
  <key>UILaunchScreen</key><dict/>
  <key>CFBundleIcons</key>
  <dict>
    <key>CFBundlePrimaryIcon</key>
    <dict>
      <key>CFBundleIconFiles</key>
      <array>
        <string>AppIcon60x60</string>
        <string>AppIcon76x76</string>
        <string>AppIcon83.5x83.5</string>
      </array>
      <key>UIPrerenderedIcon</key><false/>
    </dict>
  </dict>
  <key>CFBundleIcons~ipad</key>
  <dict>
    <key>CFBundlePrimaryIcon</key>
    <dict>
      <key>CFBundleIconFiles</key>
      <array>
        <string>AppIcon60x60</string>
        <string>AppIcon76x76</string>
        <string>AppIcon83.5x83.5</string>
      </array>
      <key>UIPrerenderedIcon</key><false/>
    </dict>
  </dict>
  <key>CFBundleSupportedPlatforms</key><array><string>$PLATFORM</string></array>
  <key>UISupportedInterfaceOrientations</key>
  <array>
    <string>UIInterfaceOrientationPortrait</string>
    <string>UIInterfaceOrientationLandscapeLeft</string>
    <string>UIInterfaceOrientationLandscapeRight</string>
  </array>
</dict>
</plist>
PLIST

if [ "$KIND" = device ]; then
  if [ -n "${EUI_IOS_IDENTITY:-}" ] && command -v codesign >/dev/null; then
    codesign --force --sign "$EUI_IOS_IDENTITY" --timestamp=none "$APP"
    echo "make-ios-app: signed with $EUI_IOS_IDENTITY"
    # An .ipa is a zip with the .app under Payload/. It is what every tool
    # that installs to a device takes, and it is nothing else.
    (cd "$OUT_DIR" && rm -rf Payload "$APP_NAME.ipa" && mkdir Payload && cp -R "$APP_NAME.app" Payload/ && zip -qry "$APP_NAME.ipa" Payload && rm -rf Payload)
    echo "make-ios-app: $OUT_DIR/$APP_NAME.ipa"
  else
    echo "make-ios-app: unsigned — set EUI_IOS_IDENTITY to sign it, or let Xcode sign it on the way to the device"
  fi
fi

echo "make-ios-app: $APP"
if [ "$KIND" = sim ]; then
  echo "  xcrun simctl boot 'iPhone 15' 2>/dev/null || true"
  echo "  xcrun simctl install booted '$APP'"
  echo "  xcrun simctl launch --console booted $BUNDLE_ID"
fi
