# Building EUI Demo for macOS

This document describes how to build EUI Demo applications for macOS, both locally and via CI/CD.

## Overview

The EUI Demo consists of:
- **eui-client**: The native macOS app that connects to an EUI server via WebSocket
- **counter-app**: A Soli-based demo application (run via `../lang/target/debug/soli serve examples/counter-app`)
- **counter-server**: A minimal Rust server example for testing

## Local Build

### Prerequisites

- Rust toolchain (stable): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- macOS 11+ (the minimum supported OS version)
- For universal binaries: `rustup target add aarch64-apple-darwin x86_64-apple-darwin`

### Build Release Binary

```bash
# Single architecture (native)
cargo build --release -p eui-client

# Run with a server
cargo run -p counter-server &
EUI_ALLOW_INSECURE_LOOPBACK=1 cargo run -p eui-client -- ws://127.0.0.1:5090
```

### Build macOS App Bundle and DMG

Use the provided build script:

```bash
# Build release app bundle (native architecture)
./scripts/build-macos-app.sh release

# Build universal binary (Intel + Apple Silicon)
./scripts/build-macos-app.sh release universal

# Build debug version
./scripts/build-macos-app.sh debug

# Output in dist/
# - dist/EUI-Demo.app (ready to run)
# - dist/EUI-Demo-*.dmg (installer)
# - dist/eui-demo-*.tar.gz (archive)
# - dist/eui-demo-*.zip (archive)
```

### Run the App

```bash
# From Finder: double-click dist/EUI-Demo.app

# From terminal:
open dist/EUI-Demo.app

# With a WebSocket URL argument:
./dist/EUI-Demo.app/Contents/MacOS/EUI-Demo ws://127.0.0.1:5090
```

## CI/CD Pipeline

### GitHub Actions Workflow

The workflow `.github/workflows/build-macos-demo.yml` automatically:

1. **Builds** eui-client for both architectures:
   - `x86_64-apple-darwin` (Intel macOS)
   - `aarch64-apple-darwin` (Apple Silicon)

2. **Creates** macOS app bundles with proper structure:
   - `Info.plist` configuration
   - Executable entry point
   - Resource directories

3. **Packages** release artifacts:
   - DMG installers (disk image)
   - Tar archives (compressed binary)
   - Zip archives (compressed app bundle)

4. **Tests** on macOS (latest):
   - Runs full test suite
   - Clippy lints

5. **Publishes** releases:
   - Automatically creates GitHub releases for tags
   - Attaches all build artifacts
   - Triggered when pushing `v*` tags (e.g., `v0.1.0`)

### Triggering a Build

**Manual trigger** (via GitHub web UI or gh CLI):
```bash
gh workflow run build-macos-demo.yml -r main
```

**Automatic triggers:**
- Push to `main` branch → builds
- Push tag `v*` → builds + creates GitHub release
- Pull request to `main` → builds (no release)

### Build Artifacts

All builds produce:
- `eui-x86_64-macos.tar.gz` — Intel binary (standalone)
- `eui-aarch64-macos.tar.gz` — Apple Silicon binary (standalone)
- `EUI-Demo-x86_64-macos.dmg` — Intel app installer
- `EUI-Demo-aarch64-macos.dmg` — Apple Silicon app installer
- `EUI-Demo-x86_64-macos.zip` — Intel app bundle archive
- `EUI-Demo-aarch64-macos.zip` — Apple Silicon app bundle archive

### Creating a Release

Push a semantic version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions will:
1. Build both architectures
2. Create a GitHub Release
3. Attach all artifacts automatically

Users can then download from GitHub Releases.

## App Bundle Structure

The macOS app bundle is organized as:

```
EUI-Demo.app/
├── Contents/
│   ├── MacOS/
│   │   └── EUI-Demo (executable)
│   ├── Resources/
│   ├── Info.plist
│   └── PkgInfo
```

The `Info.plist` specifies:
- Bundle identifier: `com.soli.eui-demo`
- Minimum OS: macOS 11
- Supports automatic graphics switching (integrated + discrete GPUs)
- Supports high-resolution displays

## Signing & Notarization (Future)

For distribution outside GitHub Releases, macOS requires:

1. **Code signing**: Sign with developer certificate
   ```bash
   codesign -s "Developer ID Application" dist/EUI-Demo.app
   ```

2. **Notarization** (for Gatekeeper bypass): Submit DMG to Apple
   ```bash
   xcrun notarytool submit dist/EUI-Demo.dmg --keychain-profile notarize
   ```

These steps can be added to the GitHub Actions workflow with secrets for the developer certificate and Apple credentials.

## Troubleshooting

### Build fails: "unable to determine target architecture"
Ensure you've installed the target:
```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

### "Could not build Objective-C wrapper" on wgpu
This is expected for Vulkan validation layers on macOS. The build should succeed; wgpu will use Metal instead.

### App won't launch from Finder
Check `dist/EUI-Demo.app/Contents/MacOS/` permissions:
```bash
chmod +x dist/EUI-Demo.app/Contents/MacOS/EUI-Demo
```

### "Cannot connect to server" when running
The demo app expects a WebSocket server at `ws://127.0.0.1:5090`. Start one first:
```bash
cargo run -p counter-server &
```

## Next Steps

1. **Enable auto-signing** in GitHub Actions (requires Apple Developer account)
2. **Add notarization** step for App Store Distribution
3. **Create installer** with app shortcuts and documentation
4. **Set up auto-updates** via Sparkle or similar framework
5. **Support drag-and-drop** of server URLs onto the app
