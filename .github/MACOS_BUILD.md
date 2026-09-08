# Building EUI Demo for macOS

This document describes how to build EUI Demo applications for macOS, both locally and via CI/CD.

## Overview

Two different apps come out of this repo, built two different ways:

- **EUI Demo** — the `eui-client` binary. A native window that connects to an
  EUI server over a WebSocket. Built by `cargo`, needs nothing outside this
  repo, and needs a server to point at.
- **Vitrine** — the widget gallery (`examples/counter-app`, component
  `gallery`) as a self-contained desktop artifact: its own Soli server on a
  thread, no database, the EUI window with the decoder in a confined worker.
  Double-click and it runs; there is nothing to point it at.

Supporting pieces: **counter-app** (the Soli app itself) and
**counter-server** (a minimal hand-written Rust server, for testing the
client without Soli).

## Local Build

### Prerequisites

- Rust toolchain (stable): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- macOS 11+ on Apple Silicon. Intel macOS is not a supported target.
- `rustup target add aarch64-apple-darwin`

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
# Build release app bundle
./scripts/build-macos-app.sh release

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

1. **Builds** eui-client for `aarch64-apple-darwin` (Apple Silicon)

2. **Creates** a macOS app bundle with proper structure:
   - `Info.plist` configuration
   - Executable entry point
   - Resource directories

3. **Packages** release artifacts:
   - DMG installers (disk image)
   - Tar archives (compressed binary)
   - Zip archives (compressed app bundle)

4. **Tests** on macOS:
   - Runs the full test suite
   - Runs `clippy -D warnings`. This catches lints that never fire on Linux,
     because the Linux-only code (Wayland/X11, seccomp, Landlock) is `cfg`'d
     out on macOS and anything only reachable from it becomes dead. These
     cannot be reproduced locally from Linux: cross-checking the Darwin target
     dies in `ring`'s build script, which wants a real macOS C toolchain.

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

`eui-macos-aarch64`:
- `eui-aarch64-macos.tar.gz` — the bare binary
- `EUI-Demo-aarch64-macos.dmg` — installer
- `EUI-Demo-aarch64-macos.zip` — app bundle

`vitrine-macos-aarch64` (only when `SOLI_BUNDLE_KEY` is set):
- `Vitrine-aarch64-macos.dmg` — installer
- `Vitrine-aarch64-macos.zip` — app bundle

Neither is signed or notarized, so the first launch of a downloaded build
needs right-click → Open, or `xattr -dr com.apple.quarantine <app>`.

### Creating a Release

Push a semantic version tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

GitHub Actions will:
1. Build the Apple Silicon app
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

## Vitrine

Vitrine is packaged by `soli desktop build`, which lives in
[`solisoft/soli_lang`](https://github.com/solisoft/soli_lang), not here. The
CI job therefore checks out **two** repositories: this one for the app source
(`examples/counter-app`), and the language repo for the tool that packages it.

```bash
soli desktop build examples/counter-app \
  --app-id com.soli.vitrine \
  --name Vitrine \
  --eui gallery \
  --no-db \
  --output dist/vitrine-desktop
```

That emits **one executable**, which the shared `scripts/wrap-macos-app.sh`
then wraps in a `.app` (macOS needs a bundle for a display name, an icon, and
a URL scheme) with a DMG beside it.

### It requires a bundle key

A desktop artifact ships your application **encrypted** — there is no
unencrypted desktop build — so it cannot be produced without a key, and there
is no offline fallback. CI reads `SOLI_BUNDLE_KEY` from repository secrets:

```bash
gh secret set SOLI_BUNDLE_KEY -R solisoft/eui
```

The job checks for it in its first seconds and fails with that command in the
message, rather than discovering the problem after a ten-minute build of the
Soli toolchain.

**Do not bake a key into a published artifact.** If the download carries its
own key, the encryption and the ability to revoke an installation both stop
meaning anything. The key belongs in a secret, or behind
`SOLI_BUNDLE_AUTH_URL`.

If you are rebuilding to replace an existing local install, reuse that
install's key rather than minting a new one — a fresh key leaves the old
launcher unable to unlock:

```bash
grep -o 'SOLI_BUNDLE_KEY=[0-9a-f]*' ~/.local/bin/vitrine | cut -d= -f2
```

### Build cost

The Soli toolchain is built with `--no-default-features --features
eui-desktop`. That is deliberate: the default features roughly double the
build, and none of them are needed to package a desktop artifact.

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
rustup target add aarch64-apple-darwin
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
