# What works today

This page is the honest one. It is kept current by hand, and if it disagrees
with the repository, the repository is right.

## Built and tested

**`eui-proto` — the wire format.** Encodes and decodes every construct in
[the wire format specification](/docs/wire-format): frames, batches, all sixteen
ops, the 64-byte style record, flat subtrees, values, handlers.

- No dependencies. Everything this crate touches came off a network socket, so a
  dependency here would be attack surface we did not write and cannot fuzz on
  our own schedule.
- `#![forbid(unsafe_code)]`.
- 64 tests: 9 round-trip, 5 byte-level vectors, 3 size budgets, **46 rejection
  cases**, plus two bulk tests that throw 40 000 mutated and random buffers at
  every entry point and require that none of them panic.
- Clean under `clippy` with `indexing_slicing`, `panic`, `unwrap_used`,
  `expect_used` and `arithmetic_side_effects` all denied — the decode path
  cannot panic by construction, not merely by inspection.

## Specified, not yet written

Normative, in `spec/`, and stable enough to build against:

- **Wire format** (`spec/02`) — implemented by `eui-proto` and pinned by test
  vectors.
- **Transport** (`spec/01`) — discovery, the signed manifest, content-addressed
  assets, the session, framing.
- **Budgets** (`spec/10`) — the wire numbers measured, the runtime numbers as
  targets.

## Designed, not yet specified

Described on these pages in enough detail to argue with, but their normative
spec documents (`spec/03` to `spec/09`) are **not written yet**. Until they
are, the doc page is the design and there is nothing more precise to appeal to.

- **Layout** — flex, stack, grid, virtualised scroll, intrinsic sizing.
- **Theming** — roles, scales, modes, and the rules for resolving them.
- **Events** — the twenty-one kinds exist in `eui-proto`; their payloads are
  not pinned down.
- **Bytecode** — the verified subset, the host surface, the metering.
- **Security** — the threat model. Its memory-safety half is built; the rest is
  design.
- **Widgets** — the fourteen primitives are in the wire format; the catalogue
  is a list.

## Not started

- `eui-tree` — session tables and patch application.
- `eui-layout`, `eui-text`, `eui-theme` — the parts that can be tested without a
  GPU, and where the CPU budgets are won or lost.
- `eui-render` — the wgpu renderer.
- `eui-vm` — the verifier and the metered interpreter.
- `eui-client` — the window, the session, the capability prompts.
- The Soli integration: `lang/src/eui/`, and `soli serve . --eui`.

## Scope, stated plainly

Writing a renderer means rewriting what a browser gives away for free: text
shaping, input methods, accessibility, selection. That is the real cost of this
project, and it is not hidden in a later milestone. The first stage delivers the
protocol, the layout engine, a renderer, a working client, and about fifteen
widgets. The full catalogue, real accessibility, and mobile come after.

## The Soli integration is additive

`lang/` is a production binary at 2.0.7, 276 000 lines. EUI enters it as a new
`src/eui/` module behind a cargo feature that is **off by default**. No line of
`src/live/`, `src/template/`, `src/serve/`, `src/vm/` or `src/interpreter/`
changes. With the feature off, the published binary is identical.
