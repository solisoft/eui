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
cd "$PROJECT_DIR"

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
BINARY_PATH="$PROJECT_DIR/target/aarch64-apple-darwin/$BUILD_TYPE/eui"

# Create output directory
DIST_DIR="$PROJECT_DIR/dist"
mkdir -p "$DIST_DIR"

"$SCRIPT_DIR/wrap-macos-app.sh" "$BINARY_PATH" EUI-Demo com.soli.eui-demo "$DIST_DIR"

( cd "$DIST_DIR" && tar -czf "eui-$ARCH-macos.tar.gz" "EUI-Demo.app" )

echo ""
echo "Artifacts in: $DIST_DIR/"
echo "To run: open $DIST_DIR/EUI-Demo.app"
echo "It needs a server: cargo run -p counter-server, then pass ws://127.0.0.1:5090"
