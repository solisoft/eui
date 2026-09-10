# EUI

A protocol for delivering application interfaces over HTTPS without HTML, CSS,
or JavaScript. The server sends an interface tree that is **already resolved**,
in a compact binary encoding; a native Rust client applies it, lays it out, and
draws it on the GPU.

The point is not the bytes — though a 50-row table is 4 619 B against 14 362 B
of HTML. The point is what the client no longer does: no tolerant parse, no
selector matching, no cascade resolution, no reflow of an untyped tree, no JIT.

## Layout

```
spec/        00–10, all normative; 09 says what conforming means, 10 has the numbers
crates/
  eui-proto  encode and decode. No dependencies, no unsafe            [built]
  eui-tree   session tables, node arena, patch application           [built]
  eui-theme  roles and scales to pixels; contrast by construction     [built]
  eui-layout flow, stack, grid, scroll, virtualised list             [built]
  eui-text   shaping and glyph rasterisation, embedded fonts only    [built]
  eui-render one instanced rounded-rect pipeline over wgpu, atlas    [built]
  eui-vm     local-handler bytecode: verifier and metered interpreter [built]
  eui-client driver, WSS transport, manifest check, assets, winit,
             keyboard focus, editing, IME, AccessKit, transitions,
             file dialogs, touch, a session that survives its socket [built]
  eui-android the shared object Android loads, and its packaging   [untested]
  eui-ios     the static library Xcode links, and its entry point  [untested]
examples/
  counter-server  the counter as a hand-written Rust server, on loopback
  demo-app        counter, todo, a 10 000-row table and a gallery as a Soli
                  app; the catalogue (buttons to date pickers to charts) is
                  app/controllers/eui_builders.sl
  snapshot        render the counter, or any live Soli component, off-screen
doc/         the documentation site, itself a Soli app
www/         the public site, itself a Soli app
xtask/       `bench` measures the budgets; `conform` runs every vector of spec/09
assets/      the icon: `icon/eui.svg` is the source, and every raster the three
             platforms want is rendered from it by `scripts/make-icons.py`
deny.toml    cargo-deny policy; crates/eui-proto/fuzz has four fuzz targets
rustfmt.toml one formatting, enforced; 200 columns, not rustfmt's default 100
```

`doc/docs/eui/status.md` says, crate by crate, what is built and tested,
what is specified, and what is not started — the worker sandbox on macOS
and Windows, and how far the phones have got.

## Get it

Every push to `main` builds the client for all five platforms and replaces
[the rolling build](https://github.com/solisoft/eui/releases/tag/rolling)
with it, so these links always point at the newest one. Tagged versions are
under [releases](https://github.com/solisoft/eui/releases).

| | download | how it goes on |
|---|---|---|
| Linux | [`eui-x86_64-linux.tar.gz`](https://github.com/solisoft/eui/releases/download/rolling/eui-x86_64-linux.tar.gz) | unpack and run |
| macOS | [`EUI-aarch64-macos.dmg`](https://github.com/solisoft/eui/releases/download/rolling/EUI-aarch64-macos.dmg) | open, drag across |
| Windows | [`eui-x86_64-windows.zip`](https://github.com/solisoft/eui/releases/download/rolling/eui-x86_64-windows.zip) | unpack and run |
| Android | [`eui.apk`](https://github.com/solisoft/eui/releases/download/rolling/eui.apk) | `adb install -r eui.apk` |
| iOS, simulator | [`EUI-ios-simulator.zip`](https://github.com/solisoft/eui/releases/download/rolling/EUI-ios-simulator.zip) | `xcrun simctl install booted EUI.app` |
| iOS, device | [`EUI-ios-device-unsigned.zip`](https://github.com/solisoft/eui/releases/download/rolling/EUI-ios-device-unsigned.zip) | sign it first — see below |

The window carries the commit it was built from in its title, so a build
from the wrong run can be told from the right one at a glance.

The APK is signed with Android's own debug key — the conventional
`~/.android/debug.keystore`, reused where it exists and minted where it does
not — which is what makes it installable without an account or a store. A CI
runner has none to reuse, so each rolling APK is signed afresh: if a device
already has an older one, `adb uninstall org.eui.client` before installing. The iOS device build is **unsigned**, because
no CI runner has an identity to sign with: give it one with
`EUI_IOS_IDENTITY=... ./scripts/make-ios-app.sh device`, or let Xcode or
`ios-deploy` sign it on the way to the phone. The simulator build needs
none of that.

Neither phone build has been run on a device by anyone yet.

## Try it

```sh
cargo fmt --all                                          # not optional: CI and `conform` both check it
cargo test                                               # 210 tests; pixel tests need any GPU adapter
cargo test -p eui-proto --test size_budget -- --nocapture # the wire numbers
cargo clippy --all-targets                               # must be silent
cargo run --release -p xtask -- bench                    # the budgets in spec/10; exits 1 on a miss
cargo run -p xtask -- conform                            # spec/09: tests, clippy -D warnings, the Soli suite with EUI_SOLI_BIN
cargo deny check                                         # advisories and licences, exceptions in deny.toml
cd doc && soli serve . --dev                             # the docs, on :5011

# The counter, end to end, in a window. It attaches a file and saves one, so
# it wants the two capabilities; stop and restart the server under it and the
# window gets its session back by itself (spec 01 §4.1).
cargo run -p counter-server                              # ws://127.0.0.1:5090
EUI_ALLOW_INSECURE_LOOPBACK=1 cargo run -p eui-client -- ws://127.0.0.1:5090 --allow fs.pick,fs.save

# A Soli app (../lang built with --features eui), with a capability granted:
../lang/target/debug/soli serve examples/demo-app --port 5011
EUI_ALLOW_INSECURE_LOOPBACK=1 cargo run -p eui-client -- ws://127.0.0.1:5011/_eui/session/gallery --allow clipboard.read
```

### The phones

The half of the client with no platform in it builds for either phone with
no toolchain at all — no NDK, no Xcode, nothing but rustup — and `conform`
checks that it still does:

```sh
rustup target add aarch64-linux-android aarch64-apple-ios
cargo check --target aarch64-linux-android -p eui-proto -p eui-tree -p eui-theme -p eui-layout -p eui-text -p eui-vm
cargo check --target aarch64-apple-ios -p eui-proto -p eui-tree -p eui-theme -p eui-layout -p eui-text -p eui-vm
```

Above that they want a real toolchain, because `ring` and `blake3` compile
C. An application is one application and not a browser, so the address it
opens is baked in at build time; leave it out for a development build that
opens the shell and lets an address be typed.

```sh
# Android: wants the SDK and NDK ($ANDROID_HOME, $ANDROID_NDK_ROOT).
cargo install cargo-apk --locked
EUI_ANDROID_URL=wss://example.test/_eui/session/gallery ./scripts/make-android-apk.sh

# iOS: wants a Mac. `sim` needs no account, no profile and no device.
EUI_IOS_URL=wss://example.test/_eui/session/gallery ./scripts/make-ios-app.sh sim
EUI_IOS_URL=... EUI_IOS_IDENTITY="Apple Development: you@example.com" \
    ./scripts/make-ios-app.sh device
```

Either script leaves something installable in `dist/` and prints the command
that installs it. There is no Xcode project in any of this: winit calls
`UIApplicationMain` itself, so the iOS binary is a whole application and a
`.app` is that binary next to an `Info.plist`.

The static library is the other way in, for an Xcode project that already
exists and owns its `main`: link `libeui_ios.a`, declare
`void eui_start(void);` and call it.

Neither has run on a device, and everything past `cargo check` on iOS needs
a Mac. `doc/docs/eui/status.md` says what is done, what is not, and which of
the two matters — the driver runs in the window's process on both, because
Android will not `exec` a second binary out of an application's own storage
and iOS has no `exec` at all, so spec 08 §10's confined worker does not
exist there.

## Where the design is written down

| | |
|---|---|
| Why it exists, and what it refuses | [`spec/00-rationale.md`](spec/00-rationale.md) |
| HTTPS discovery, manifest, session, framing | [`spec/01-transport.md`](spec/01-transport.md) |
| **Atoms, styles, nodes, patches — byte level** | [`spec/02-wire-format.md`](spec/02-wire-format.md) |
| **Roles, scales, and the resolution algorithm** | [`spec/05-theme.md`](spec/05-theme.md) |
| **The layout algorithm** | [`spec/04-layout.md`](spec/04-layout.md) |
| The numbers, and the tests behind them | [`spec/10-budgets.md`](spec/10-budgets.md) |

## Relationship to Soli

EUI is Soli's native interface layer. An EUI component is a LiveView
component whose view returns a node tree as plain data instead of HTML:

```soli
router_eui("counter", "live#counter", "live#counter_view")
```

The integration lives in `lang/src/serve/eui/` behind the cargo feature
`eui`, one of Soli's defaults; with it off, none of that code is compiled.
To run the counter through Soli:

```sh
(cd ../lang && cargo build --features eui)
../lang/target/debug/soli serve examples/demo-app --port 5011
EUI_ALLOW_INSECURE_LOOPBACK=1 cargo run -p eui-client -- ws://127.0.0.1:5011/_eui/session/counter
# or, headless:
EUI_SOLI_BIN=../lang/target/debug/soli cargo test -p eui-client --test soli_e2e
```

A new application starts with the catalogue in it:

```sh
soli new my-app --eui     # + app/controllers/eui_builders.sl and a component
```

That copy comes from `examples/demo-app/app/controllers/eui_builders.sl`,
which is where the catalogue is edited; `scripts/sync-catalogue.sh` puts it
in the language repository, and `--check` says whether the two have drifted.
