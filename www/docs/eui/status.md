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

**`eui-tree` — session state.** The four define-once tables, the node arena
with a free list, and `apply` for every op in the wire format.

- Every reference — style, atom, colour, chunk, node id — is checked against
  the tables *before* anything is placed, so an invalid node deep in a subtree
  leaves nothing behind.
- Node, depth and atom-byte quotas are checked before the memory they bound is
  allocated. Subtree removal is an iterative walk, never a recursive drop.
- A failed op poisons the session until the next successful `Mount` — the
  transport's own recovery — so no per-batch snapshot is needed.
- 27 tests, including a random op stream that must keep the arena's live
  count equal to a fresh walk of the tree after every step.

**`eui-theme` — theme resolution.** Roles and scale indices to concrete
values, per `spec/05-theme.md`, which is now normative.

- OKLab/OKLCH conversions, gamut clipping by chroma reduction, WCAG contrast.
- Contrast is met **by construction**: after the per-mode lightness targets
  are assigned, each specified pair is nudged apart until it passes. A test
  drives 300 random seed pairs and the corner cases through all three modes
  and asserts every pair in the spec's table; none fail.
- The `EUIT` theme document, encoded and decoded; unknown keys are an error,
  because a content-addressed document has no version skew to be lenient
  about.
- `check_style` rejects a style record whose indices run off a scale, so a
  bad index is a rejected batch rather than a surprise at paint time.
- 17 tests.

## Specified, not yet written

Normative, in `spec/`, and stable enough to build against:

- **Wire format** (`spec/02`) — implemented by `eui-proto` and pinned by test
  vectors.
- **Theme** (`spec/05`) — implemented by `eui-theme`.
- **Transport** (`spec/01`) — discovery, the signed manifest, content-addressed
  assets, the session, framing.
- **Budgets** (`spec/10`) — the wire numbers measured, the runtime numbers as
  targets.

## Designed, not yet specified

Described on these pages in enough detail to argue with, but their normative
spec documents (`spec/03` to `spec/09`) are **not written yet**. Until they
are, the doc page is the design and there is nothing more precise to appeal to.

- **Layout** — flex, stack, grid, virtualised scroll, intrinsic sizing.
- **Events** — the twenty-one kinds exist in `eui-proto`; their payloads are
  not pinned down.
- **Bytecode** — the verified subset, the host surface, the metering.
- **Security** — the threat model. Its memory-safety half is built; the rest is
  design.
- **Widgets** — the fourteen primitives are in the wire format; the catalogue
  is a list.

## Not started

- `eui-layout`, `eui-text` — the parts that can be tested without a GPU, and
  where the CPU budgets are won or lost.
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
