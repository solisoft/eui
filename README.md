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
spec/        02 (wire), 04 (layout), 05 (theme) normative; 10 measured
crates/
  eui-proto  encode and decode. No dependencies, no unsafe            [built]
  eui-tree   session tables, node arena, patch application           [built]
  eui-theme  roles and scales to pixels; contrast by construction     [built]
  eui-layout flow, stack, grid, scroll, virtualised list             [built]
  eui-text   shaping and glyph rasterisation, embedded fonts only    [built]
  eui-render one instanced rounded-rect pipeline over wgpu, atlas    [built]
www/         the documentation site, itself a Soli app
```

Everything else named in `spec/README.md` — layout, text, theme, renderer, VM,
client — is specified and not yet written. `www/docs/eui/status.md` keeps that
list honest.

## Try it

```sh
cargo test                                               # 144 tests, pixel tests need any GPU adapter
cargo test -p eui-proto --test size_budget -- --nocapture # the wire numbers
cargo clippy --all-targets                               # must be silent
cd www && soli serve . --dev                             # the docs, on :5011
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

EUI is Soli's native interface layer. Views will be written in `.eui.sl` and
served by the Soli runtime, reusing routing, models, and the LiveView session
machinery.

That integration enters `lang/` as a new `src/eui/` module behind a cargo
feature that is **off by default**. No line of `src/live/`, `src/template/`,
`src/serve/`, `src/vm/` or `src/interpreter/` changes, and with the feature off
the published binary is unchanged.
