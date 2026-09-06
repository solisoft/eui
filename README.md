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
spec/        02 wire, 04 layout, 05 theme, 06 events, 07 bytecode normative; 10 measured
crates/
  eui-proto  encode and decode. No dependencies, no unsafe            [built]
  eui-tree   session tables, node arena, patch application           [built]
  eui-theme  roles and scales to pixels; contrast by construction     [built]
  eui-layout flow, stack, grid, scroll, virtualised list             [built]
  eui-text   shaping and glyph rasterisation, embedded fonts only    [built]
  eui-render one instanced rounded-rect pipeline over wgpu, atlas    [built]
  eui-vm     local-handler bytecode: verifier and metered interpreter [built]
  eui-client driver, WSS transport, winit window                     [built]
examples/
  counter-server  the counter as a hand-written Rust server, on loopback
  counter-app     counter, todo and a 10 000-row table as a Soli app, with
                  the widget catalogue in app/controllers/eui_builders.sl
  snapshot        render the counter off-screen, for machines with no display
www/         the documentation site, itself a Soli app
xtask/       `bench`: measures the budgets and fails when one is missed
deny.toml    cargo-deny policy; crates/eui-proto/fuzz has four fuzz targets
```

Everything else named in `spec/README.md` — layout, text, theme, renderer, VM,
client — is specified and not yet written. `www/docs/eui/status.md` keeps that
list honest.

## Try it

```sh
cargo test                                               # 160 tests; pixel tests need any GPU adapter
cargo test -p eui-proto --test size_budget -- --nocapture # the wire numbers
cargo clippy --all-targets                               # must be silent
cargo run --release -p xtask -- bench                    # the budgets in spec/10; exits 1 on a miss
cargo deny check                                         # advisories and licences, exceptions in deny.toml
cd www && soli serve . --dev                             # the docs, on :5011

# The counter, end to end, in a window:
cargo run -p counter-server                              # ws://127.0.0.1:5090
EUI_ALLOW_INSECURE_LOOPBACK=1 cargo run -p eui-client -- ws://127.0.0.1:5090
```

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

The integration lives in `lang/src/serve/eui/` behind the cargo feature `eui`,
**off by default**; with it off, none of that code is compiled. To run the
counter through Soli:

```sh
(cd ../lang && cargo build --features eui)
../lang/target/debug/soli serve examples/counter-app --port 5011
EUI_ALLOW_INSECURE_LOOPBACK=1 cargo run -p eui-client -- ws://127.0.0.1:5011/_eui/session/counter
# or, headless:
EUI_SOLI_BIN=../lang/target/debug/soli cargo test -p eui-client --test soli_e2e
```
