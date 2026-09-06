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

**`eui-layout` — the layout engine.** One algorithm, `spec/04-layout.md`,
now normative.

- Flow (`row`/`column`) with wrap, grow, shrink, gap, every `justify` and
  `align` value including baseline; a bounded freeze loop for min/max
  conflicts, so a pathological set of constraints costs eight passes rather
  than a hang.
- `stack` with z ordering, a one-shape `grid`, `scroll` with clamped offsets,
  and a `list` that **does not measure rows it cannot see** — a thousand-row
  list shapes about a dozen.
- Two phases: `measure` is memoised per frame and pure, `arrange` runs once
  per node. Hit-testing honours stacking order and scroll clipping.
- Text shaping is behind a trait; tests use a fixed-pitch stand-in so the
  goldens pin exact pixels.
- 18 golden tests.

**`eui-text` — shaping and rasterisation.** Over `cosmic-text`, the one
third-party dependency on the CPU side of the client: shaping is the part of
text that must not be reinvented.

- Two faces embedded, Titillium Web and JetBrains Mono, both OFL. The font
  database is built by hand from those three files and **never touches the
  system's fonts** — a test asserts the count is exactly three. The first
  version of this code used the convenience constructor and loaded 779
  faces; the same text would have shaped differently on every machine, and
  the installed font list would have been visible to a server.
- A bounded shaping cache keyed by `(text, font, width, clamp)`; layout
  measures a run under several constraints per frame and shapes it once.
- Glyph rasterisation at a device scale behind an opaque key, for the
  renderer's atlas.
- Line clamping truncates; it does not yet append an ellipsis.
- 8 tests, one of which lays out real glyphs through `eui-layout`.

**`eui-render` — the renderer.** One shape, one pipeline, one draw call per
scissor region.

- Everything on screen is a rounded rectangle: a box's fill and border, a
  divider, a glyph textured from the atlas. The fragment shader anti-aliases
  from the signed distance to the edge, so boxes, hairlines, borders and
  glyphs share one instanced pipeline over `wgpu`.
- Glyphs are rasterised on demand into a single R8 atlas, shelf-packed, grown
  once.
- Quads are snapped to device pixels at paint time; `scroll` and `list`
  become scissor rects; anything outside its clip is culled before it reaches
  the GPU; virtualised rows paint their box and shape no text.
- Off-screen targets can be read back, so the renderer is **tested by its
  pixels** on a machine with no display: clear colour, box placement, corner
  radius and border, text ink confined to its rect in the text role's colour,
  scroll clipping at the pixel, stack z order.
- 10 tests. Not yet: shadows, images, canvas paths.

**`eui-client` — the client.** Three parts kept apart so two can be tested
without the third.

- The **driver**: session, layout, input dispatch and painting, with no
  window and no socket. Dispatch is spec 06's one walk — the nearest handler
  on the path from the hit node to the root, no bubbling. Clicks are a press
  and a release resolving to the same handler; typing edits an `input`
  locally and commits on `Enter` or blur; the wheel scrolls the nearest
  `scroll` or `list` and clamps; dark mode re-resolves the theme with no round
  trip. 13 tests.
- The **transport**: a WebSocket over TLS on its own thread, binary frames
  only. `ws://` is refused except on loopback in a debug build with an
  explicit opt-in.
- The **window**: winit in `ControlFlow::Wait` over a wgpu surface. There is
  no render loop; a redraw happens when a frame arrived, the viewer acted, or
  the OS asked. Compiled, not yet exercised on a display.
- **End to end**, over a real socket against the example counter server:
  Welcome, Mount, a click leaving as an event of under 40 bytes, the new
  value coming back, a resync that restores the server's state, and a forged
  event that the server refuses. 2 tests.

The end-to-end run found a protocol mistake on both sides: a re-mount after
`Resync` was repeating definitions, and the client was answering a rejected
batch with an `Error` frame — which is fatal. Both fixed; `spec/01` §4 now
says so explicitly.

**The Soli integration.** `lang/` gained a cargo feature, `eui`, off by
default. On, it adds the `router_eui(component, handler, view)` builtin and
the `/_eui/session/<component>` socket; off, none of it is compiled and the
binary builds exactly as before.

- An EUI component *is* a LiveView component — same registry, same frame
  lock, same worker channel, same `{event, params, state}` handler. The only
  additions: a view action returning a tree as plain data, a converter that
  interns atoms and styles per session, a tree diff, and a binary socket that
  validates every client event against the tree it last sent before it
  becomes a handler call.
- `examples/counter-app` holds three components as a Soli app — the counter,
  a todo list (keyed rows, a text field, checkboxes that report which item
  they belong to through the node's props), and a 10 000-row table sorted by
  `MoveChild` — plus `eui_builders.sl`, twenty-odd widgets composed from the
  primitives: buttons in four variants, checkbox, switch, badge, card, tabs,
  spinner, toast, dialog, field, form, table header and rows. Nothing native.
- Measured against a **debug** `soli`: ten thousand rows mount in about 2 s
  and paint in about 50 ms as 423 quads; a non-virtualised paint would be
  around 200 000.
- Three end-to-end tests start the real `soli serve` and drive it through
  the real client transport: the counter's clicks leave the same nine nodes
  with the same ids and a resync carries Soli's state; the todo toggles a row
  in place by prop, adds a keyed row from a typed field, and clears three
  rows leaving the fourth's id intact; the table mounts ten thousand keyed
  rows and re-sorts them by moves.
- Touched in `lang/`: `Cargo.toml`, three `#[cfg(feature = "eui")]`
  insertions in `src/serve/mod.rs`, one builtin in `router.rs`, and the new
  `src/serve/eui/` module. `src/live/`, `src/template/`, `src/vm/` and
  `src/interpreter/` are untouched.

**Budgets, enforced.** `cargo run --release -p xtask -- bench` measures
what `spec/10` promises and exits non-zero on a miss. It found two: a
per-node byte budget that had been set without counting the cell text, and a
scroll step over ten thousand rows at 4.7 ms against a 2 ms budget. The
second was real, and the fix was structural — a virtualised list is now
placed arithmetically, so a row outside the window costs nothing: no style,
no measure, no rect. The step went from 4.7 ms to about 0.3 ms, and a
full layout of the table from 1.6 ms to about 0.15 ms. Along the way:
`clear_all_dirty` walks only dirty paths (760 µs → 60 ns), the shaping cache
no longer allocates on a hit, and the per-frame layout reset no longer
memsets a per-node style cache.

`cargo deny check` passes with three tracked exceptions (unmaintained
`ttf-parser`, `rustybuzz` and `paste`, all under cosmic-text or wgpu), each
with its reason in `deny.toml`. Four `cargo fuzz` targets exist — frame
decoding, session apply, theme documents, layout — and want a nightly
toolchain to run; the in-tree hostile-input tests run on every `cargo test`.

**`eui-vm` — local handlers.** The only code a client runs that it did not
ship with, per `spec/07-bytecode.md`, now normative.

- A chunk is a tiny stack machine: integers, booleans, strings, the root
  node's props as local state, `set_text` and `set_prop` on nodes, `emit` to
  queue a server event. No I/O, no clock, no allocation beyond its operand
  stack. The `Host` trait has six methods and nothing else is reachable.
- The verifier decodes every instruction, checks that every jump lands on an
  instruction boundary, and proves the stack depth along every path before a
  chunk runs once. A run has 4 096 units of fuel; a type error or an
  exhausted budget aborts it and nothing is sent.
- Chunks arrive inline in the session (`DefChunkBytes`, ≤ 64 KiB), so no
  asset endpoint and no HTTP client were needed for this step.
- On the Soli side a local handler is a small statement language —
  `state.count += 1; value.text = str(state.count)`, `if … else`,
  `self.style = @hover`, `emit("…")` — compiled by the server to a chunk;
  the counter's `+` increments its local copy, rewrites the value, and *then*
  tells the server, which confirms or corrects on the next batch. Every
  catalogue button uses it for hover and pressed states, and the end-to-end
  test watches the style id change under the pointer with no frame sent.
- 7 VM tests; the driver runs a local handler with no round trip and stays
  silent when a chunk fails verification; the Soli end-to-end counter uses
  one.

**Assets and images.** `GET /_eui/asset/<blake3>` on the Soli side, from a
process-wide content-addressed store: an image in a view is a file path,
hashed and served immutable, so a client caches it forever and nothing on
the path can substitute it. On the client, a deliberately small HTTP/1.1
reader over TLS — status 200, one `Content-Length` body, no chunked
encoding — verifies the hash before anything is decoded, decodes PNG, and
packs images into an RGBA atlas beside the glyph atlas. An image with no
explicit size takes its intrinsic size the moment it arrives. Chunks defined
by hash go through the same path. The todo's header carries an avatar that
the end-to-end test fetches from the real server.

**The catalogue, second half.** `eui_builders.sl` now composes forty-odd
widgets from the primitives: buttons in four variants with local states,
checkbox, switch, badge, chip, card, stat, tabs, segmented control,
accordion, stepper, breadcrumb, pagination, progress, skeleton, spinner,
toast, banner, empty state, dialog, sheet and drawer, popover, tooltip,
menu, toolbar, navbar, sidebar, tree view, code block, field and form,
table header and rows, avatar and image. A `gallery` component shows them
all on one page; its end-to-end test mounts it through Soli, moves the
segmented control, swaps accordion sections and opens and closes the sheet.

**Accessibility.** The client exposes its tree through AccessKit — AT-SPI
on Linux, UIA on Windows, AX on macOS — with the mapping of spec 03 §6:
click handlers are buttons named by their text, editable nodes are text
fields, text is a label, the rest are groups. The tree is built only when
an assistive technology asks and refreshed after a paint while one listens;
its focus and click actions become the same inputs Tab and Enter produce.
`--no-default-features` builds without it.

**Editing.** Fields have a caret and a selection: click and drag, arrows
by character and by word, Home and End, Backspace and Delete, select all,
copy, cut and paste through the system clipboard (the `clipboard` feature,
on by default). Glyphs now carry their byte range, so the caret sits on a
real cluster edge and a click lands on the nearest one; a field scrolls
its text to keep the caret in view. A paste is the person's act and reaches
the application as typing; the driver itself never touches the clipboard.

**Input methods.** The window allows an IME exactly while a field has
focus and anchors the candidate window to it; a composition in progress is
shown in the field and reported nowhere; the commit arrives as one
`text_input`. A field's `change` fires only when its value differs from
what the server last had.

**Conformance.** `spec/09` now says what conforming means and where each
vector lives; `cargo run -p xtask -- conform` runs the workspace suite,
clippy with warnings denied, and the Soli end-to-end suite when
`EUI_SOLI_BIN` is set.

**Shadows and transitions.** A record's `shadow` paints a grown, offset,
black quad under the box, faded across the blur in the fragment stage;
`transition` (the last reserved byte of the 64, now spent) names a `motion`
index, and a node whose style changes to such a record eases its colours and
opacity from the old ones. The driver keeps a clock, the window switches to
`WaitUntil` for the next frame only while something animates, and the
catalogue's buttons fade between their hover and pressed states.

**Charts.** `canvas` paths are in (spec 03 §1.1): five kinds — polyline,
rectangle, area, circle, arc — that the renderer draws with the one quad
pipeline it already has, a rotation added to the vertex stage so a segment
is a capsule and an arc a fan of them. Soli resolves the colours before
encoding. The catalogue's `chart_line`, `chart_area`, `chart_bar` and
`chart_donut` build the paths server-side; the gallery shows all four.

**Keyboard focus, and the widgets that needed it.** The client walks `Tab`
order itself — editable fields and anything with a `click` handler, in
document order — draws the focus ring for keyboard and server focus only,
turns `Enter` and `Space` on a focused button into the `click` they stand
for, and drops focus on `Escape`. On top of that the catalogue gained
`select` (a dropdown the server opens and closes), `slider` (click to set,
arrows to nudge once focused), and one calendar engine behind `date_picker`,
`datetime_picker` and `date_range_picker`. The gallery's end-to-end test
picks an option, drags the slider by click and by keyboard, picks a day,
turns a month and selects a range.

## What the specification covers

Every document in `spec/` is normative now, and each names the code that
implements it and the vectors that pin it:

- **Wire format** (`02`) — `eui-proto`, byte-exact vectors, 47 rejection
  cases. The last of the style record's reserved bytes became `transition`.
- **Primitives, painting, focus, canvas paths, transitions** (`03`) —
  `eui-render` and `eui-client`.
- **Layout** (`04`) — `eui-layout`, goldens against a fixed-pitch measurer;
  §9 lists what version 1 leaves out.
- **Theme** (`05`) — `eui-theme`, contrast enforced by construction.
- **Events** (`06`) — `eui-client`, the driver tests.
- **Bytecode** (`07`) — `eui-vm`, verifier and fuel; Soli compiles `local`
  handlers to it.
- **Security** (`08`) — each requirement naming where it is enforced.
- **Conformance** (`09`) — `cargo run -p xtask -- conform`.
- **Budgets** (`10`) — `cargo run --release -p xtask -- bench`.
- **Transport** (`01`) — the session and content-addressed assets are
  implemented; the signed manifest and key pinning are specified, not yet
  checked by the client.

## Not started

- Capability prompts and the manifest check in the client.
- The multi-process sandbox, Android and iOS, `soli desktop build --eui`
  (stage 3).

## Scope, stated plainly

Writing a renderer means rewriting what a browser gives away for free: text
shaping, input methods, accessibility, selection. That is the real cost of
this project, and it is not hidden in a later milestone. The first stage
delivered the protocol, the layout engine, a renderer, a working client and
about fifteen widgets; the second added the rest of the catalogue, keyboard
focus, input methods, transitions, shadows and charts. Accessibility,
mobile and the sandbox come after.

## The Soli integration is additive

`lang/` is a production binary at 2.0.7, 276 000 lines. EUI enters it as
`src/serve/eui/` behind a cargo feature that is **off by default** — a child
of `serve` rather than the planned `src/eui/`, so it can reuse `serve`'s
private helpers instead of duplicating them. With the feature off, none of it
is compiled.
