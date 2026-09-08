#!/bin/bash
# Build macOS app bundle and DMG for EUI Demo Client
# Usage: ./scripts/build-macos-app.sh [release|debug]
#
# Apple Silicon only: Intel macOS is not a supported target.

set -e

BUILD_TYPE="${1:-release}"
ARCH="aarch64"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Building EUI Demo macOS App Bundle${NC}"
echo "Build type: $BUILD_TYPE"
echo "Architecture: $ARCH"

# Build the binary
echo -e "${BLUE}Compiling eui-client...${NC}"
if [ "$BUILD_TYPE" = "release" ]; then
  CARGO_FLAGS="--release"
else
  CARGO_FLAGS=""
fi

rustup target add aarch64-apple-darwin 2>/dev/null || true
cargo build $CARGO_FLAGS -p eui-client --target aarch64-apple-darwin
BINARY_PATH="target/aarch64-apple-darwin/$BUILD_TYPE/eui"

# Create output directory
DIST_DIR="$PROJECT_DIR/dist"
mkdir -p "$DIST_DIR"

# Create app bundle structure
echo -e "${BLUE}Creating macOS app bundle...${NC}"
APP_NAME="EUI-Demo"
APP_BUNDLE="$DIST_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_BUNDLE/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

rm -rf "$APP_BUNDLE"
mkdir -p "$MACOS_DIR"
mkdir -p "$RESOURCES_DIR"

# Copy the binary
cp "$BINARY_PATH" "$MACOS_DIR/$APP_NAME"
chmod +x "$MACOS_DIR/$APP_NAME"

# Create Info.plist
cat > "$CONTENTS_DIR/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>EUI-Demo</string>
    <key>CFBundleIdentifier</key>
    <string>com.soli.eui-demo</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>EUI Demo</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
    <key>NSRequiresIPhoneOS</key>
    <false/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
</dict>
</plist>
EOF

# Create PkgInfo
echo -n "APPL????" > "$CONTENTS_DIR/PkgInfo"

echo -e "${GREEN}✓ App bundle created: $APP_BUNDLE${NC}"

# Create DMG if hdiutil is available
if command -v hdiutil &> /dev/null; then
  echo -e "${BLUE}Creating DMG installer...${NC}"

  DMG_TEMP="$DIST_DIR/dmg-temp"
  rm -rf "$DMG_TEMP"
  mkdir -p "$DMG_TEMP"
  cp -r "$APP_BUNDLE" "$DMG_TEMP/"

  DMG_NAME="$APP_NAME-$ARCH-macos.dmg"
  hdiutil create -volname "EUI Demo" \
                -srcfolder "$DMG_TEMP" \
                -ov -format UDZO \
                "$DIST_DIR/$DMG_NAME"

  rm -rf "$DMG_TEMP"
  echo -e "${GREEN}✓ DMG created: $DIST_DIR/$DMG_NAME${NC}"
fi

# Create archives
echo -e "${BLUE}Creating archives...${NC}"
cd "$DIST_DIR"
tar -czf "eui-demo-$ARCH-macos.tar.gz" "$APP_NAME.app"
zip -r -q "eui-demo-$ARCH-macos.zip" "$APP_NAME.app"
cd - > /dev/null

echo -e "${GREEN}✓ Archives created${NC}"
echo -e "${GREEN}✓ Build complete!${NC}"
echo ""
echo "Artifacts in: $DIST_DIR/"
echo "  - $APP_NAME.app (ready to run)"
[ -f "$DIST_DIR/$DMG_NAME" ] && echo "  - $DMG_NAME"
echo "  - eui-demo-$ARCH-macos.tar.gz"
echo "  - eui-demo-$ARCH-macos.zip"
echo ""
echo "To run: open $DIST_DIR/$APP_NAME.app"
echo "Or: $MACOS_DIR/$APP_NAME ws://127.0.0.1:5090"
