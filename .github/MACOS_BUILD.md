# Building EUI Demo for macOS

This document describes how to build EUI Demo applications for macOS, both locally and via CI/CD.

## Overview

What CI builds and releases is one thing:

- **EUI Demo** — the `eui-client` binary. A native window that connects to an
  EUI server over a WebSocket. Built by `cargo`, needs nothing outside this
  repo, and needs a server to point at.

There is a second thing this repo can produce by hand, and no longer does in
CI:

- **Vitrine** — the widget gallery (`examples/demo-app`, component
  `gallery`) as a self-contained desktop artifact: its own Soli server on a
  thread, no database, the EUI window with the decoder in a confined worker.
  Double-click and it runs; there is nothing to point it at. Packaged by a
  tool in another repository and shipped encrypted under a key CI had to
  hold, so it is [built by hand](#vitrine) now.

Supporting pieces: **demo-app** (the Soli app itself) and
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

#### What the DMG looks like

`scripts/wrap-macos-app.sh` does not hand over a folder with one app in it.
It stages the volume with a symlink to `/Applications` beside the app, draws
a backdrop for the window (`scripts/dmg-background.js`, at both 1x and 2x),
and then scripts Finder into laying the window out: 640x400, no toolbar,
128pt icons, the app on the left and Applications on the right with the
backdrop's chevrons running between them. Opening the image shows the drag
that installs it.

Only the layout is scripted through Finder, and only that part is allowed to
fail: a machine that withholds automation permission still gets a DMG, just
an undressed one. The `/Applications` symlink is staged before any of that,
so the drag target is there either way.

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

Neither is signed by a Developer ID nor notarized. The bundle is ad-hoc
signed — enough that Apple Silicon does not refuse it outright — but a
download still arrives wearing `com.apple.quarantine`, which the browser
writes, so Gatekeeper holds the first launch. Stripping the attribute by
hand does not stay stripped: the next download gets a fresh one.

Two ways to stop repeating it:

- **Fetch it with something that does not quarantine.** `curl`, `wget` and
  `gh release download` set no attribute at all; only apps that opt into
  `LSFileQuarantineEnabled` — browsers, Mail, AirDrop, Messages — do.
  `scripts/install-macos.sh` is that download plus the install:

  ```bash
  curl -fsSL https://raw.githubusercontent.com/solisoft/eui/main/scripts/install-macos.sh | bash
  ```

- **Notarize, and staple the ticket to the bundle.** The attribute is still
  written, and Gatekeeper reads the stapled ticket and lets it through
  anyway — for everyone who downloads it, not just whoever knows the
  incantation. This is the only fix that works for a person who clicked the
  link in the README. The workflow and the wrapper already do it; they are
  waiting on a paid Apple Developer account and the secrets under
  [Signing & Notarization](#signing--notarization) below, and take the
  ad-hoc path until those exist.

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

**Not built by CI, and not in any release.** It had a job of its own across
three platforms; it was removed, because a demo gallery is not something the
release exists to ship and it cost a full build of the Soli toolchain on
every push to produce a file nobody was asked to download. What follows is
how to make one by hand.

Vitrine is packaged by `soli desktop build`, which lives in
[`solisoft/soli_lang`](https://github.com/solisoft/soli_lang), not here — so
you need **both** repositories: this one for the app source
(`examples/demo-app`), and the language repo for the tool that packages it.

```bash
soli desktop build examples/demo-app \
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
is no offline fallback. Set `SOLI_BUNDLE_KEY` in the environment you build
in. Nothing in CI reads it any more; the repository secret can go whenever
you are sure nothing else wants it.

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

Build the Soli toolchain with `--no-default-features --features eui-desktop`.
The default features roughly double the build, and none of them are needed to
package a desktop artifact. This cost — ten minutes of toolchain on three
runners — is the reason the job is gone.

## Signing & Notarization

Wired up, and dormant until the secrets exist. `scripts/wrap-macos-app.sh`
reads the environment and takes one of two paths:

- **No `MACOS_SIGNING_IDENTITY`** — ad-hoc, no certificate. What a local
  build and a fork's CI get. The bundle runs, but a *download* of it wears
  `com.apple.quarantine` and Gatekeeper holds the first launch.
- **`MACOS_SIGNING_IDENTITY` set** — a Developer ID signature, with
  `--options runtime` and `--timestamp`, which notarization requires and
  which are not the default. With notary credentials beside it the app is
  submitted to Apple, the ticket is stapled into the bundle *before* the zip
  and the DMG are made from it, and the DMG is then signed, submitted and
  stapled in its own right. Quarantine is still written on download;
  Gatekeeper reads the staple and lets it through.

It fails loudly rather than quietly downgrading: an identity the keychain
does not have, or notary credentials with no identity to go with them, stops
the build instead of shipping something weaker than was asked for.

### The secrets

Set these on the repository (Settings → Secrets and variables → Actions).
The workflow's signing step returns immediately when `MACOS_CERTIFICATE` is
absent, so a repository with none of them keeps building exactly as it does
now.

| secret | what it is |
|---|---|
| `MACOS_CERTIFICATE` | the Developer ID Application certificate and key, exported from Keychain Access as a `.p12`, then `base64` |
| `MACOS_CERTIFICATE_PASSWORD` | the password set on that `.p12` export |
| `MACOS_SIGNING_IDENTITY` | the identity's full name, e.g. `Developer ID Application: Your Name (TEAMID)` — `security find-identity -v -p codesigning` prints it |

And one set of notary credentials. Prefer the App Store Connect API key: it
is scoped to notarization and can be revoked by itself, where an
app-specific password rides on the Apple ID that owns the account.

| secret | what it is |
|---|---|
| `APPLE_API_KEY_P8` | the `.p8` key file from App Store Connect → Users and Access → Integrations, `base64` |
| `APPLE_API_KEY_ID` | the key's ID, from the same page |
| `APPLE_API_ISSUER` | the issuer UUID, from the same page |

or, the older shape:

| secret | what it is |
|---|---|
| `APPLE_ID` | the Apple ID of the developer account |
| `APPLE_APP_PASSWORD` | an app-specific password made at appleid.apple.com, **not** the account password |
| `APPLE_TEAM_ID` | the ten-character team ID |

All of it needs a paid Apple Developer account; there is no free Developer
ID certificate.

### By hand

The same two paths, run locally:

```bash
# ad-hoc, which is what happens with nothing set
./scripts/build-macos-app.sh release

# signed and notarized
export MACOS_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_API_KEY=~/private_keys/AuthKey_XXXXXXXXXX.p8
export APPLE_API_KEY_ID=XXXXXXXXXX APPLE_API_ISSUER=....
./scripts/build-macos-app.sh release
```

Notarization adds a few minutes per submission — two of them, the app and
the DMG — and the wrapper prints the notary's report as it goes. On a
refusal it fetches the per-submission log, because the summary says only
"Invalid" while the log says which binary and why.

Check the result on a Mac that has never seen the build:

```bash
spctl --assess --type execute -vv /Applications/EUI.app   # expect: accepted, source=Notarized Developer ID
xcrun stapler validate /Applications/EUI.app
```

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
