# What works today

This page is the honest one. It is kept current by hand, and if it disagrees
with the repository, the repository is right.

Everything below is produced by a command you can run:

```
cargo test --workspace                                     # the client and protocol tests
cargo test -p eui-proto --test size_budget -- --nocapture  # the wire numbers
cargo run -p xtask -- conform                              # every vector of spec/09
cargo run --release -p xtask -- bench                      # the budgets; exits 1 on a miss
```

## Built and tested

**`eui-proto` — the wire format.** Encodes and decodes every construct in
[the wire format specification](/docs/wire-format): frames, batches, all nineteen
ops, the 64-byte style record, flat subtrees, values, handlers.

- No dependencies. Everything this crate touches came off a network socket, so a
  dependency here would be attack surface we did not write and cannot fuzz on
  our own schedule.
- `#![forbid(unsafe_code)]`.
- 93 tests: 10 round-trip, 7 byte-level vectors, 4 size budgets, 4 manifest,
  4 frame-walking,
  **64 rejection cases**, plus two bulk tests that throw 40 000 mutated and random buffers at
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
- 35 tests, including a random op stream that must keep the arena's live
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
- 23 tests.

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
- 36 golden tests.

**`eui-text` — shaping and rasterisation.** Over `cosmic-text`, the one
third-party dependency on the CPU side of the client: shaping is the part of
text that must not be reinvented.

- Four faces embedded — Inter regular and bold, JetBrains Mono, Noto Sans Symbols for the hearts and arrows a text face lacks — all OFL. The font
  database is built by hand from those four files and **never touches the
  system's fonts** — a test asserts the count is exactly four. The first
  version of this code used the convenience constructor and loaded 779
  faces; the same text would have shaped differently on every machine, and
  the installed font list would have been visible to a server.
- **Font roles.** An application may supply faces of its own: `DefFont`
  binds a role — the `font_family` byte — to faces named by asset hash, and
  the client fetches them from its own origin, checks the bytes against
  their own name, reads the family out of the face's own tables and shapes
  the role in it. Roles 0 and 1 are sans and mono and may be replaced for
  the session; 2–9 are the application's. A role nothing bound, a face that
  has not arrived, and a face that will not parse all draw in sans, so a
  font is never what stops a page being drawn. Nothing about this widens
  what the client connects to: a face from a font service is fetched by the
  *server*, once, and re-served from the application's origin.
- A bounded shaping cache keyed by `(text, font, width, clamp)`; layout
  measures a run under several constraints per frame and shapes it once.
  The key holds the *role*, so rebinding one drops what was shaped under it.
- Glyph rasterisation at a device scale behind an opaque key, for the
  renderer's atlas.
- The pointer's shape is held while a hover is owed. It comes from the node's
  style, and a `local` hover handler is what puts a beam there — so every
  batch, which restores the style the server last sent before diffing against
  it, wiped the preview. The hover re-runs at the next paint, so between the
  two the node wore the server's style and the window drew the arrow: a beam
  flickering under a pointer that never moved, on every update of a page that
  updates. The pointer has not moved, so the shape has not changed; while the
  hover is unsettled the answer is the last one worked out. Focus already had
  this repair on the same path (`hover_relight` beside it), and the cursor did
  not.
- And the hover is anchored on the node's **name** rather than on its slot,
  which is the other half of the same symptom. Holding the shape while a
  hover is owed repaired the frame the client knew was unsettled; it left the
  one it did not know about. `pointer.over` was an arena index, and `replace`
  frees the subtree it replaces before grafting the new one back over the
  slots it freed — LIFO against a pre-order walk, so a row rebuilt in place
  hands its first child's old slot to the new row and moves every id along
  one. The pointer was then resting on a freed slot, where the walk up the
  tree found nothing and answered the arrow, or on a **sibling**, where it
  answered for a node the pointer was never on. Nothing re-settled either
  until the hand moved. The anchor is now 01 §2.7's owner and the server's
  id, re-found once per batch on the page's path and on an island's: found,
  the index is re-pointed and **nothing is emitted**, because the same node
  under a pointer that did not move owes no events — a `hover()` here would
  put a `pointer_move` on the wire per batch for any page that handles one,
  and a server that answers it with a batch is a loop with no bottom; gone,
  the hover is left owed and the last shape stands until the paint settles
  it. It took a wire-visible bug with it: the hover settled at the next paint
  used to send `pointer_leave` naming whichever node had taken the slot.
- Line clamping appends the face's own `…`, shaped at the text's size, and
  drops trailing glyphs of the last line so the mark fits inside the width it
  was given — otherwise the one line that says "there is more" would be the
  one line that overflows (04 §3).
- 24 tests, one of which lays out real glyphs through `eui-layout`.

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
- A `scene` renders into a target of its own, with a depth buffer of its own,
  and the list carries one textured quad that samples it. So the pipeline
  above keeps `depth_stencil: None` whatever an application draws, and the
  scene inherits the corner radius, the opacity, the scissor and the page
  transition from a quad its shader knows nothing about. An inlined 3D pass
  would have had to reimplement every one of those inside each shader a
  server wrote.
- Off-screen targets can be read back, so the renderer is **tested by its
  pixels** on a machine with no display: clear colour, box placement, corner
  radius and border, text ink confined to its rect in the text role's colour,
  scroll clipping at the pixel, stack z order.
- 68 tests. Shadows, images and canvas paths each have one now; what is
  still not covered is a scene's pixels, deliberately — `spec/09-conformance.md`
  §11 pins the verifier's verdicts and the frame's structure, and says in as
  many words that a scene's pixels are not a conformance surface.

**`eui-client` — the client.** Three parts kept apart so two can be tested
without the third.

- The **driver**: session, layout, input dispatch and painting, with no
  window and no socket. Dispatch is spec 06's one walk — the nearest handler
  on the path from the hit node to the root, no bubbling. Clicks are a press
  and a release resolving to the same handler; typing edits an `input`
  locally and commits on `Enter` or blur; the wheel scrolls the nearest
  `scroll` or `list` and clamps; dark mode re-resolves the theme with no round
  trip. 113 tests.
- The **transport**: a WebSocket over TLS on its own thread, binary frames
  only. `ws://` is refused unless the **host** is `127.0.0.1`, `localhost`
  or `[::1]` *and* somebody asked for it — `EUI_ALLOW_INSECURE_LOOPBACK=1`
  on a desktop, or, in a page, the browser's own answer to whether the
  document itself came from loopback, which nothing the document contains
  can forge. A host and not a prefix: `ws://localhost.evil.example/` passes
  a `starts_with` and is somebody else's server, which is what the check
  used to be and what `dial.rs` now refuses. 01 §1 used to add "in a debug
  build" and no client ever obeyed it — every measurement in this
  repository is a release binary on loopback — so the rule now says what
  bounds the risk instead.
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
- `examples/demo-app` holds three components as a Soli app — the counter,
  a todo list (keyed rows, a text field, checkboxes that report which item
  they belong to through the node's props), and a 10 000-row table sorted by
  `MoveChild` — plus `eui_builders.sl`, a catalogue of widgets composed from the
  primitives: buttons in four variants, checkbox, switch, badge, card, tabs,
  spinner, toast, dialog, field, form, table header and rows. Nothing native.
- Measured against a **debug** `soli`: ten thousand rows mount in about 2 s
  and paint in about 50 ms as 423 quads; a non-virtualised paint would be
  around 200 000.
- Forty end-to-end tests start the real `soli serve` and drive it through
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

`cargo deny check` passes with four tracked exceptions — unmaintained
`ttf-parser`, `rustybuzz`, `paste` and `instant`, under cosmic-text, wgpu
and notify — each with its reason and the condition that ends it in
`deny.toml`. It is a CI job now, and the first run of it earned its place:
a live advisory against `rustls` 0.23.43 (RUSTSEC-2026-0285, TLS 1.3
handshake messages accepted across encryption-level boundaries) had landed
in the stack that spec 01 §1 makes the whole client rest on, and the file
that would have said so was being run by whoever remembered. Fixed by the
bump it asks for.

Five `cargo fuzz` targets exist — frame decoding, session apply, theme
documents, layout, and the shader verifier — and want a nightly toolchain
to run. They had never run in CI either; `fuzz.yml` now gives each ten
minutes a night and twenty seconds on a pull request that touches the
crate under it, and carries the corpus between runs. The in-tree
hostile-input tests still run on every `cargo test`.

**`eui-shader` — what a scene's module must be.** The second verifier, and
the second exception to "no code from the network".

- It proves three things before the window compiles anything, and claims no
  more: the module terminates in at most 4 096 steps per invocation — the
  same number as the VM's fuel, deliberately — it reaches nothing but the
  uniform block the client hands it, and it has the shape the client
  compiles.
- Every loop's trip count is read off its own constants. A bound taken from
  a uniform is refused, because a uniform is a number the server sends
  *after* verification.
- Around forty vectors and a fuzz target, all of which decide without a GPU.
  That is the point: a scene's pixels are not a conformance surface, and its
  verdicts are.
- What it does not prove is written down in `spec/11-shaders.md` §6 rather
  than left implied — chiefly that the driver's own shader compiler runs in
  the window process on input the server chose, and no arrangement of
  processes moves it.

**`eui-vm` — local handlers.** The only code a client runs that it did not
ship with, per `spec/07-bytecode.md`, now normative.

- A chunk is a tiny stack machine: integers, booleans, strings, the root
  node's props as local state, `set_text` and `set_prop` on nodes, `emit` to
  queue a server event. No I/O, no clock, no allocation beyond its operand
  stack. The `Host` trait has ten methods and nothing else is reachable:
  `atom`, `load`, `store`, `set_text`, `set_prop`, `set_style`, `emit`,
  `set_mode`, `go_back` and `set_scene_uniform`.
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
- 16 VM tests; the driver runs a local handler with no round trip and stays
  silent when a chunk fails verification; the Soli end-to-end counter uses
  one.

**A render with no session, and what a session costs.** `GET
/_eui/view/<component>` on the Soli side answers
`application/vnd.eui.frames` — the exact frames a fresh socket would have
sent, `Welcome` and the batches through the first `Mount`, with a strong
`ETag` over the body and the `Cache-Control` the component declared. A
component opts in with `router_eui(..., {"static": "public, max-age=60"})`,
which is refused alongside `{"session": "required"}` because a static view is
rendered for nobody.

The numbers behind it, measured against `examples/eui-site` on 2026-09-19:
a socket costs **50–60 kB of resident memory per reader who is doing
nothing**, linear to 400 sessions, and does not come back promptly when the
socket closes. The page is 4 799 B on the wire, so the server holds about
twelve times the page per reader — the previous tree plus the four interned
tables. Six hundred *distinct* one-shot renders of the same page, each at a
different viewport so nothing could be reused, moved resident memory by
**4 kB in total**. The bytes are the socket's own: compared frame by frame,
the batches are identical and only the `Welcome`'s session handle differs,
which is sixteen zeroes here because this body is everyone's.

**A session that adopts the tree instead of re-sending it.** 01 §2.6, and the
reason it exists: without it the first interaction on a fetched page
re-mounts it, and `start_over` takes the tree, the layout, focus, every
scroll offset and anything half-typed with it — a reader partway down a page
clicks something and is returned to the top. The client offers the BLAKE3 of
the batches it holds as a third `Hello` resume tag; Soli renders `connect` as
it would have anyway, compares, and either sends a `Welcome` and nothing else
or sends the frames it just rendered. One render either way. Verified against
a live server: 4 799 B fetched over HTTPS, the socket answers
`Welcome.start = Adopted` and no `Mount` follows. `PROTOCOL_VERSION` is 5;
the six language SDKs need no change, because they negotiate `min(client,
ours)` and can only receive the offer if they serve §2.4, which none do.

**And the shell now says when there is no session.** A typed `https://host`
is normalised to `wss://host` at one door, because somebody copying their
browser's address bar writes the first and it names the same origin; the
manifest's `entry` then completes the path. So a tab served entirely over
`GET /_eui/view/<component>` — no socket, nothing held for the reader —
showed `wss://eui-site.solisoft.test/_eui/session/site` in the bar and
nothing to say it was not connected. `Link::Static` existed and
`link_word` deliberately returned `None` for it, on the reasoning that there
was no fault to report: true, and not the question. It is `page` now, beside
where `reconnecting` and `offline` go, and it goes when a socket is dialled.
The rule moved out of the method into `link_word_of` so that all five states
are one vector rather than something only a standing tab can be asked.

**Session resume on the Soli side** (01 §4.1) is built, as of 2026-09-20.
`lang/src/serve/eui/` answered `Start::Fresh` on every reconnect, so a dropped
wifi hop was a fresh mount and the reader went back to the top with nothing
focused. It now mints a handle per EUI session, keeps the last sixty-four
unacknowledged batches, and answers `Resumed` with only what that client
missed — no `connect`, no resync, no tree.

Three things in it are worth keeping written down. The handle used to be a
digest of the cookie session, which names the **person**: two tabs of one
reader share a cookie, so nothing could tell them apart and there was nothing
to look up. It is minted per session now. The handle is also a bearer, so it
is not the whole of the check — a resume requires the same cookie and the same
component, or sixteen bytes would be enough to be handed somebody else's tree.
And the encoder is no longer dropped when the socket goes: that single line is
what made every reconnect a fresh mount, since the encoder holds the interned
tables and the tree the next batch diffs against. A sweep frees it when the
two-minute grace runs out. This paragraph said the work was blocked on the
pinned protocol revision long after that stopped being true.

**Islands** (01 §2.7) are specified and half built. A node carrying `island`
takes its content from a session of its own, so a page can be a cached
render with one corner that is not; the node's own children show until that
session speaks, which makes an older client, a failed session and a stale
cache all degrade the same way — to *out of date* rather than to a hole.
Nothing is added to the wire. Soli's half is done: a session address may
carry a query (`?for=1042`) and it arrives as `connect` params.

**The tree's half is done too, as of 2026-09-20**, and it is the half the
design turns on. `Node` carries an `owner`; `Session` holds one set of
interned tables per island beside the page's; `text_of`, `style_of` and every
reference checked while a batch is applied resolve through it; and the arena's
id index is keyed by `(owner, id)`. That last one is not a detail — it is the
enforcement. An island's encoder starts at 1 like any other, so a page and an
island both define atom 1, style 1 and node 1, and a shared table would answer
the second with `Redefined`. Keyed by owner, both live, and an island naming
one of the page's nodes finds nothing at all: 01 §2.7's "no id it sends can
name a node it did not create", held by the shape of a map rather than by a
check somebody has to remember. `apply_region(owner, batch)` grafts under the
node carrying the prop and touches nothing else, and puts `applying` back on
every road out, including the error ones — leaving it set would have the whole
session resolve the page's ops against an island's tables. Eight islands a
page (`MAX_ISLANDS`), and a batch for an owner nobody handed out is refused.
One arena, one layout, one paint: neither the layout engine nor the painter
learns that islands exist.

**And the client's half, the same day.** `Driver` finds the nodes carrying the
prop, refuses any path that is not an absolute one on this origin, opens one
session per *distinct path* — query and all, because the query is what tells
the application which island it is rendering — stops at eight, applies an
island's batches under its owner, and answers `owner_of` so that an event
raised inside an island goes to that island's socket rather than naming a node
the page's server never created. `island_ended` releases nothing: 01 §2.7's
"leaves the page alone" is the vector the whole feature rests on, since a live
part that could take a still page with it would make every island a liability.

`crates/eui-client/tests/islands.rs` exists now — it was the one file `spec/09`
named that did not — with eight vectors, five more in
`eui-tree/tests/apply.rs`. The refusal worth naming is `//host/path`: a
protocol-relative URL is a different origin *and* starts with a slash, which
is exactly how the obvious spelling of that check lets it through.

An island's events go to their own queue rather than the page's, tagged with
the owner that owes them — `take_island_pending()` beside `take_pending()`,
kept apart rather than tagged inside it so that an island's event cannot reach
the page's server by being forgotten about. Writing the vector for it found a
real bug three crates away: the **layout's** style cache was keyed by style id
alone, so an island's style `1` was served the page's, and a box asking for
80×20 came out 400×0 with nothing anywhere saying so. Both style caches take
the owner now.

**And the sockets, which is the rest of it.** A `Tab` holds an
`IslandSocket` per island beside the page's connection — which the page may
not even have, since this is exactly the case where it has none. `pump`
drains each one into `apply_region`, dials the ones the tree asks for that
are not open, shares a socket between two islands naming one path, and hands
`take_island_outbound()` back to the connection that owes it. The address is
built from the page's own origin plus the path, here rather than taken from
the tree: §2.7 allows a path and nothing else, so nothing the tree says can
decide where a socket connects.

It reaches the worker, so the protocol grew four requests — `IslandsWanted`,
`OpenIsland`, `IslandFrame`, `IslandEnded` — two payloads and one status
field. `island_outbound` is a field of its own rather than a tag inside
`outbound`, for the same reason the driver keeps two queues: a separate field
has to be read to be sent, and so cannot become the page's by being
forgotten. `Backend::take_island_outbound()` is one drain rather than a
return value on each call, because an island's event can come out of an
`Input`, a `Paint` or its own frame, and a path that must be remembered at
three call sites will be forgotten at one of them.

§2.7's one `MAY` is taken: an island is not opened until its node has been
laid out **and is on the glass**, so a comment thread below the fold on a page
nobody scrolls costs what the rest of the page costs, which is nothing. Not
laid out is not the same as not there — a node the layout has not reached has
no rect at all, and opening a session for it would be guessing — so both wait,
and both are asked again on the next pump, because a scroll that brings one
into view is a frame and a frame is a pump.

What is left is an end-to-end run against a Soli server serving one, which
`soli_e2e` is the place for and which no CI has a server to do.

Interaction over plain verbs — a `POST` carrying one event — was specified in
an earlier draft and is deliberately **not** being built: it answers the same
question as islands from the other end, and two ways to do one thing is
worse than either.

**Assets and images.** `GET /_eui/asset/<blake3>` on the Soli side, from a
process-wide content-addressed store: an image in a view is a file path,
hashed and served immutable, so a client caches it forever and nothing on
the path can substitute it. On the client, a deliberately small HTTP/1.1
reader over TLS — status 200, one `Content-Length` body, no chunked
encoding — verifies the hash before anything is decoded, decodes PNG, JPEG
and WebP, and
packs images into an RGBA atlas beside the glyph atlas. An image with no
explicit size takes its intrinsic size the moment it arrives. Chunks defined
by hash go through the same path. The todo's header carries an avatar that
the end-to-end test fetches from the real server.

**The catalogue, second half.** `eui_builders.sl` now composes 491 builders
widgets from the primitives: buttons in four variants with local states,
checkbox, switch, badge, chip, card, stat, tabs, segmented control,
accordion, stepper, breadcrumb, pagination, progress, skeleton, spinner,
toast, banner, empty state, dialog, sheet and drawer, popover, tooltip,
menu, toolbar, navbar, sidebar, tree view, code block, field and form,
table header and rows, avatar and image. A `gallery` component shows them
all on one page; its end-to-end test mounts it through Soli, moves the
segmented control, swaps accordion sections and opens and closes the sheet.

**A spinner that spins, and a button that shows it is working.** The
style record's next byte is `animation`: `spin` turns everything painted
for a node about its centre, one turn per 1.2 s, and the window wakes for
frames only while such a node is on screen; the catalogue's `spinner` is a
three-quarter arc on a canvas that spins. The feed's `Load 5 000 more` is a
`loading_button`: a local handler reveals the spinner and changes the label
the instant it is pressed — and the effects of a local-then-server handler
are **provisional** now (spec 07 §6): the client puts the old values back
the moment the server's answer arrives, before applying it, so a server that
confirms sends the change and one that does not sends nothing, and the
client agrees either way. The scrollbar's thumb fills its strip under the
pointer and while dragged.

**Memory, measured on the feed** (release client, the machine's 1.5×
window of 1230 × 1390): 131 MB resident, 104 MB proportional, at rest with
five thousand cards on the tree; the empty counter costs 85 MB resident,
32 MB proportional — Mesa, LLVM and the Vulkan driver are most of it, and
shared. The client's own data: 33 MB for the 96 000 nodes of five thousand
cards (about 345 bytes a node: 136 in the arena, the rest in the vectors
and strings each node owns), 5 MB more after the first paint, 3 MB after
scrolling through all of it, 35 MB more for the next five thousand. The
node is where the next frugality work goes.

**What fifteen thousand cards taught.** Loading the feed past 250 000
nodes made the client refuse the batch, ask for a fresh tree, refuse that
too, and show nothing. Three things changed: the node limit is a million
(136 bytes a node in the arena — a hostile server can cost a desktop about
140 MB, not more), a refused *resync* now ends the session with an `Error`
naming the reason instead of looping, and Soli says on its console when a
view exceeds what a client accepts. A big update also streams now: the
server sends slices of a thousand ops and the client paints between them.
Measured at the server, the seconds of "Load 5 000 more" are Soli itself:
building ten thousand cards in the interpreter (cached now, per card),
converting the tree and diffing it; the client applies a thousand cards in
30 ms.

**Why a like cost half a second.** Three things in the Soli side of a
render were proportional to the whole tree, not to what changed: the view's
value was serialised to JSON before conversion, the previous tree was
cloned before the diff, and the keyed diff searched the sibling list for
every child — quadratic in five thousand cards. Now the value is converted
directly, the previous tree is diffed in place, keys go through a map, and
a keyed child whose view value is *the same hash object* as last render
(the feed keeps its cards in a cache) is kept behind an `Arc` — neither
walked nor compared, its subtree size cached so even counting the tree
costs only the fresh part. A like on 5 000 cards: convert-and-diff 520 ms
→ 21 ms in a debug build, 2.2 s → 44 ms on 10 000; the round trip is now
mostly the view (130 ms debug, about a tenth of that in release). The
contract, documented with `router_eui`: a keyed node hash returned
unchanged is assumed unchanged.

**A feed, for the performance check.** `examples/demo-app`'s `feed`
component: ten posts to start with (`Load 5 000 more` adds five thousand), a
third with a picture, in a virtualised list whose rows now carry their own
`item_height` — two card heights, one addition per row per layout, only the
visible cards measured. 96 000 nodes; a scrolled frame is half a
millisecond of layout and paint in a debug build. Scrollers overflowing
their box wear a real scrollbar now: a thumb to drag, a track that pages.
Along a row, a paragraph's automatic minimum is its longest word, so it
wraps before it squeezes an avatar; a `width: 100%` obeys its `max_width`.

**Seen on glass.** On 2026-09-07 the gallery opened in a real window for
the first time — Hyprland on Wayland, an AMD GPU at 1.5× — and every widget
answered: segmented control, select, slider, pickers, accordion, typing
into fields, scrolling. Two things only a screen could show: the renderer
asked wgpu for downlevel limits, whose 2048 px texture cap a high-DPI
window exceeds on its first frame (now the adapter's own limits); and a
page taller than the window needs to say so — the gallery's page is a
`scroll` now, the way a browser's viewport would be.

**One artefact, no browser.** `soli desktop build --eui <component>` — with
a soli built with `--features eui-desktop`, which links the client crates
into the runtime — produces the usual desktop executable, but at launch the
server runs on a thread and the embedded client opens the component in its
own window. The loopback gate is armed with a session only that client
holds, presented as a cookie on every request; the publisher key is
generated per install and never bundled. The window was written here
without a display: it is the same `App` the `eui` binary runs, under test
headlessly, and the desktop path runs headless with
`SOLI_DESKTOP_NO_WINDOW=1`. A defeat to record: the plan said such an
artifact would fall from 80 MB to about 15. Measured, the `demo-app`
artifact is 96 MB (80 with a runtime built without Soli's default
features); the window costs 12 MB and the rest is the Soli runtime and
its database. `soli desktop build --no-db` (or `--db-url` for a database
elsewhere) drops the database from the artifact and from the launch: the
feed as a desktop app is 76 MB and starts no database process. Getting to
15 MB means a runtime built for an offline app, which is open work — and
now a measured one. `cargo bloat --release --features eui-desktop --bin
soli --crates` on 2026-09-08 puts 45 MB of `.text` in the 78 MB binary,
and the shape of it says what a slim build could and could not do:
**`solilang` itself is 10.2 MB** of that, the interpreter and its
builtins, before any dependency. Then 4.4 MB of `std`, 2.0 MB of
`openssl-sys` (pulled in by SSH), 3.5 MB of window (naga, wgpu, winit,
rustybuzz, skrifa, the client crates), 1.3 MB of accessibility (zbus and
its AT-SPI adapter), and a tail of 388 crates worth 9 MB — spreadsheets,
PDF, images, the language server, the terminal UI, S3, mail, three SQL
drivers.

That build now exists. Six of those — `soli deploy` and its vendored
OpenSSL, the spreadsheet class, the PDF class and its signatures, S3, the
mailer, the language server — became cargo features in Soli on
2026-09-08, all on by default, and a runtime built without them plus the
database clients and the code-graph grammars is **46 MB** where it was 61
(78 with the default set). Fifteen megabytes for six features, and the
remainder is not more of the same: the interpreter is 10 MB of machine
code before any dependency, the application's own server needs its HTTP
stack, and the window needs wgpu, a shader translator, text shaping and
the accessibility bridge. **15 MB is not reachable by subtraction**, and
this page will not keep printing it as a target. The artifact that
follows from it was built and booted the same day: the feed as a desktop
application, `--no-db`, is **52.6 MB** where the same thing with the
default runtime was 76.

**The manifest, signed and pinned.** `GET /.well-known/eui` is a record
(spec 01 §2.1, keys fixed now) signed by the publisher's Ed25519 key. The
client verifies it before opening a session, refuses a protocol it cannot
speak, pins the key on first use and refuses a changed key without a
rotation the old key signed. Soli signs with a key it generates into
`config/eui_publisher.pkcs8`; `eui_capabilities(...)` in `routes.sl` says
what to ask for and `eui <url> --allow ...` what the person grants. A
capability prompt in the window asks in the person's words, once, with a
padlock in the address bar that opens the answer again afterwards.

The eleventh capability is `net.open`, and it is the only one that hands
something *outside* the client: a node carrying an `https:` address opens it
in the person's own browser when they click it (03 §3.5). Their act and
never the application's — no op opens an address, no event reports one, the
host is named before the opener is called, and exactly one scheme is
accepted, because a platform opener is a URI dispatcher and `file:` is a
scheme. What it costs is written down rather than discovered: a token in the
address links this session to the identity the person browses the web with,
and no allowlist closes that (08 §8.1).

**Notifications.** `eui_notify("Nouveau message", "Ana : on déjeune ?",
{ "tag": "thread-7" })` in a Soli handler raises a real notification on the
machine the window is on — `notify-send` on Linux, `osascript` on macOS, and
a line on the error output anywhere else. It is a new op (`0x2C`, 02 §5.2),
the only one that names no node, and the `notifications` capability is the
whole of the gate: nothing in the tree asks for a notification, so there is
nothing else to refuse. Four to a batch, refused past that. Nothing comes
back — not shown, not clicked, not dismissed — and clicking one brings the
window forward and tells the server nothing. A notification goes to the
session whose handler called it; `eui_wake` is how the other windows are
given the chance to notify themselves.

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
index — five of them now, `fast 100` to `slowest 1000`, the top two for a
thing arriving over a distance rather than a control changing state — and a
node whose style changes to such a record eases its colours and opacity from
the old ones. The driver keeps a clock, the window switches to
`WaitUntil` for the next frame only while something animates, and the
catalogue's buttons fade between their hover and pressed states.

**Charts.** `canvas` paths are in (spec 03 §1.1): five kinds — polyline,
rectangle, area, circle, arc — that the renderer draws with the one quad
pipeline it already has, a rotation added to the vertex stage so a segment
is a capsule and an arc a fan of them. Soli resolves the colours before
encoding. The catalogue's `chart_line`, `chart_area`, `chart_bar`,
`chart_donut`, `chart_candle` and `chart_gantt` build the paths server-side;
the gallery shows all six. Each one answers the pointer without a round trip:
boxes over the drawing carry local handlers that light the column under it and
fade in a value chip, and the donut's legend writes the reading into the hole.
The candlestick scales to the extent of its lows and highs rather than to a
top, and the Gantt runs its bands across the rows rather than down the
columns; neither needed a byte of client.

**Keyboard focus, and the widgets that needed it.** The client walks `Tab`
order itself — editable fields and anything with a `click` handler, in
document order, with the one exception 03 §3.1 now carves out: a focused
node carrying `typing` is *sent* `Tab` and `Shift+Tab` instead, because a
terminal or a code editor means the tab character by them, and
`Ctrl+Shift+Tab` is the one chord nothing may claim so there is always a way
out. It draws the focus ring for keyboard and server focus only,
turns `Enter` and `Space` on a focused button into the `click` they stand
for, and drops focus on `Escape`. On top of that the catalogue gained
`select` (a dropdown the server opens and closes), `slider` (drag or click to
set, arrows to nudge once focused), and one calendar engine behind `date_picker`,
`datetime_picker` and `date_range_picker`. The gallery's end-to-end test
picks an option, drags the slider and nudges it from the keyboard, picks a day,
turns a month and selects a range.

**A data grid, not a printout.** `table_header` and `table_row` still draw
the 10 000-row invoice list. `data_grid` is the same columns as a tool: the
header sits outside the virtualised list (sticky without sticky), a click
selects a cell, a second click on an editable one puts an `input` in it, a
header click sorts by `MoveChild`. The gallery shows eight invoices; its
end-to-end test sorts by amount and edits a client. A list that fits its
rows no longer swallows the page's wheel — the scroller under the pointer
is the one that can still move in that direction (03 §3). A sticky header
inside the scroll is still out — layout §9 has no sticky. Column resize was
recorded here as out for the same reason, and that was wrong: what a local
handler cannot do is set a width from `pointer_move`, which is a limit on
*latency*, not on capability. A server-driven drag has always been available —
`split_pane` uses one — and the slider used one until `track` (03 §3.4) gave
it the other answer: a declared prop the client resolves itself, drawing every
frame from its own value and reporting one `change` per step rather than one
round trip per mouse position.
What column resize still needs is the re-emitting of every row's styles that a
width change touches, since a table is rows with per-cell widths rather than a
grid with tracks.

**Needle, a player that is nobody's copy.** `examples/demo-app`'s
`music` component searches a catalogue, opens an artist or a record and
plays a track: a field at the top, a rail of what the search found, one
detail pane, a bar at the bottom, and on a window under 720 px the rail
and the pane take turns. It is drawn out of roles, not brand colours —
`surface`, `text`, `accent`, `border` — so the viewer's own mode is the
app's, and the one literal colour anywhere is the record's own hue: a
number taken from its name gives the band behind it, and the same number
draws its sleeve on a `canvas`, five arcs and a label, for every record
that has no picture of its own. One that does gets it: the server fetches
the catalogue's artwork with Soli's new `HTTP.download`, crops the middle
square, re-encodes it — the picture is cut down to the size the node draws,
which is a re-encode either way — and names the file like any other picture, so the client still
speaks to no one but its own origin (01 §2.2). The welcome page opens on
four records asked for by name, sleeves and all, when a catalogue is
configured: `/browse/new-releases` answers 403 to an application
registered since November 2024, and search's `tag:new` — which does
answer — returns the world's fortnight, whose Hebrew and Korean titles
this client has no face for and paints as tofu. Three things move,
and only three, because 03 §5 animates colour and opacity and nothing
else: the band keeps its key across records, so switching one morphs its
hue over `motion.slow`; a row's highlight fades under the pointer from a
local handler, no round trip; and the disc in the bar wears `spin` for
exactly as long as something is playing.

The catalogue is Spotify's when `SPOTIFY_CLIENT_ID` and
`SPOTIFY_CLIENT_SECRET` are in the app's `.env`, and a generated one
otherwise, so the sample runs configured with nothing. Those two are the
client-credentials grant, which has no browser step and no callback at
all. Playing on a device the person already has open is Spotify Connect,
which is *their* account rather than the app's, so the top bar's
**connect account** opens the consent screen — the server spawns the
browser, because this is the server's own flow to finish; a client can be
handed an address now (`net.open`, 03 §3.5), but only one the person clicks
themselves — and the two ordinary routes in `spotify_controller.sl` finish
the authorization code exchange. The `state` parameter is derived from the
client secret rather than kept between the two requests, and the refresh
token is handed to the person to paste into their launcher rather than
saved, because `Cache` is SoliKV and `File` is jailed to an application
folder that, in a desktop build, is a directory under `/dev/shm` that
dies with the process: there is nowhere durable for that process to put
a secret, and pretending otherwise would have been the wrong lesson.

The tree can carry a sound — 03 §7's `audio` node — and a record in
`public/music` really plays: the file is an asset like a picture, the
client decodes it in the worker, and `time_update` four times a second
is the only clock this application has, so that bar advances because the
sound does. `level` (03 §7) rides that same tick and carries how loud the
sound has been since the last one, which is what a `vu_meter` draws. It is
measured on the source before the viewer's own volume is applied, and that
is not an implementation detail: measured one line later it would be a
readout of the viewer's volume knob, and a zero would report that they had
muted. `peak_is_before_the_viewers_own_gain` is the test that says so. Spotify's own catalogue never plays here, because the Web
API returns metadata and never audio to anyone. What it offers instead
is a device to play *on*, so Needle lists them, marks the one it is
asking, and can start one of its own on this machine — `librespot`, 41
MB and no window, rather than the several hundred megabytes of the
official client. Closing the window stops that speaker: the session
posts a `disconnect` to the component before it tears down, and what an
application started on the way in it can stop on the way out.

Every pane was rendered off-screen while it was written; three
end-to-end tests search and open a record, play a file from the machine
and watch the bar follow it, and light a card under the pointer to check
it goes out again — including when a frame arrives while it is lit,
which is what used to leave a wall of them on.

**What the window weighs, by mapping.** Measured on the standalone
client with the feed open, release, PSS (what the process really costs,
shared pages divided among their sharers): 63 MB. Of it, 32 MB is
`libLLVM` — loaded by this machine's AMD Vulkan driver (Mesa's RADV
links it), touched through its relocations, ours only in the sense that
we opened a GPU device; 8 MB is the binary's own text, 10 MB heap and
6 MB anonymous are the client (atlases, session, transport), 5 MB the
Vulkan driver itself. The GL backend used to be initialised alongside
Vulkan and brought Mesa's gallium and a second LLVM mapping; the window
now asks wgpu for the primary backends only. The worker beside it is
17 MB. RSS reads higher (97 MB) because it counts every shared page
whole.

**Windowed lists (04 §7.1).** A `list` with a `count` has rows the tree
does not hold: the client lays out `count` rows from a `heights` prop
(one integer per row, `item_height` where absent), places the children it
has by their `row`, paints nothing for the rest, and asks — `window
[first, last]`, once a scroll has landed, once per range — for the rows in
view plus a viewport of margin. The feed does this now: five thousand
more posts arrive as five thousand integers and a badge, the server holds
one window of cards (and prunes its card cache to a few windows), and the
tree stays at a few hundred nodes whatever the count. The client asks
for two viewports of margin, once the scroll has been still for 120 ms
(a request a frame was a server render a frame), and paints the rows it
does not have yet as placeholders, so a fast scroll shows where the cards
are rather than a blank. Forty thousand
posts had cost 1.4 GB across the two processes — 30 KB a card on the Soli
side, mostly the interpreter's hashes, 6.5 KB on the client; they now
cost the client 160 KB of heights and the server a window.

**The desktop's palette, followed (05 §5).** The theme is resolved on the
client from roles, so a desktop that publishes its colours can be followed
exactly. Omarchy is the first: the window reads the current theme's
`colors.toml` (`~/.local/state/omarchy/current/theme`), maps background,
foreground, accent, the surfaces and the four status colours onto the
roles — hover and active accents derived in OKLCH, `on` colours by
contrast — and hands them to the driver as overrides on top of the
application's theme, the palette's own light or dark as the mode — in
that mode only: the app's own light/dark switch takes the viewer to the
theme's colours for the other mode, and back to the desktop's. The
directory is watched with inotify, so switching the theme from Omarchy's
menu recolours every open EUI window at once, no wakeups otherwise. The
server never sees a colour; it sees the mode, as the next viewport.
`EUI_DESKTOP_THEME=0` leaves the application's theme alone. macOS and
Windows accent colours are not read yet.

**Installing an application (01 §2.1).** An EUI application is an address,
and an address is something you have to have somewhere to type. A manifest
that publishes an `icon` can be put in the desktop's own launcher instead:
`eui --install <address>`, or the arrow beside the padlock in the address
bar, writes a `.desktop` file and a PNG on Linux, a bundle in
`~/Applications` on macOS, and a Start menu shortcut with an `.ico` on
Windows. `--uninstall <app id>` takes it out again, `--installed` lists
what is there, and the tick the arrow becomes does the same from the
address bar.

Nothing is packaged and nothing is downloaded: the entry runs the client
with the application's address, so an installed application is the session
it always was and updating the client updates every one of them at once.
The icon is fetched as a content-addressed asset and checked against the
hash the publisher signed — which matters more here than anywhere else,
because everything else a session draws is inside a window that says whose
it is, and this is a tile in a dock with nothing around it. An application
without an icon is not installable rather than being given the client's
own: a launcher full of identical pictures is worse than an absent button.

The `icon` field made the manifest a version 2 record. A manifest without
one is still written as version 1, byte for byte what it always was, so
every record signed before this still verifies; a decoder is told which it
is reading by the version byte and never guesses from the field count. On
the Soli side the whole of publishing one is a PNG at `public/icon.png`,
or `eui_icon("...")` for a file kept somewhere else.

**Light or dark, before the first frame (05 §5).** Which of the two the
machine is in is asked once, before any driver exists, and every driver
is built in it — so the first `Hello` carries the right mode and a server
never renders a light page for somebody sitting in the dark. It used to be
asked after the tabs were open: a driver is born light, the correction
reached only the tab that happened to be active, and every tab opened
afterwards — a typed address, a link, a reload — started light again with
nothing to tell it otherwise. Linux hid that, because the Omarchy palette
is handed to each new tab and carries a mode with it; on macOS the shell
came up dark, you opened an application, and it was light from then on.
A change now reaches every open tab rather than the front one, which is
what a background session's server was missing.

Where the platform will not answer, the desktop is asked. winit reports a
theme on macOS, on Windows and in a page, and on no Linux backend at all —
its Wayland answer is the decoration theme the process itself asked for,
its X11 one is `None` — so a GNOME or KDE desktop in the dark got a light
client and no way to say otherwise. The client now reads `color-scheme`
from the XDG desktop portal (`org.freedesktop.appearance`, the setting a
browser answers `prefers-color-scheme` from) and listens for the portal's
`SettingChanged`, so those desktops follow live as Omarchy's do. "No
preference" is not light: it is no answer, and the application's own
theme decides as before.

**Keys, cursor, theme switch (03 §3, 07).** `ArrowDown`/`ArrowUp` land a
list on its next/previous row — the layout keeps each virtualised list's
row tops, so the feed's cards of two heights snap exactly — `PageDown`/
`PageUp` move a viewport, `Home`/`End` the whole way, all eased like a
wheel notch and chaining onto a scroll in flight. The pointer takes the
shape of what it is over: a `cursor` style, a beam on a field, a hand on
anything clickable — and where it *is* now comes from the move that put it
there rather than from whichever code path is asking. It used to be an
argument, and each call site passed the truth about itself: the chrome's
event path said "over the chrome" and the paint path said "over the page",
so a page that repaints on a clock took the shape back ten times a second
while the pointer stood still on a tab. A page that never repaints never
showed it. And a local handler can switch the viewer's palette —
`theme.toggle()`, `theme.mode = "dark"`, bytecode `set_mode` — so an app
can carry its own light/dark switch at no round trip (`theme_toggle()` in
the example builders; the feed does not show it, since the client follows
the desktop's theme).

**The process boundary (08 §10).** Everything that reads bytes a server
chose — frame decoding, the tree, layout, text shaping, PNG decoding, the
VM: the whole `Driver` — now runs in a worker process; the window keeps
winit, wgpu, TLS, the pin store, the clipboard and the accessibility
adapter, and never decodes a frame. Two pipes carry a private
request/reply wire: raw frames and inputs go in, outbound frames come back
with the driver's state (redraw owed, IME area, clipboard, next frame
due), and a paint reply carries the draw list plus each atlas bitmap when
it changed. On Linux the worker locks itself down before its first byte:
Landlock deny-all on files and sockets, a seccomp allowlist of 35 system
calls that kills on anything else, and not dumpable, so a kill leaves no
core of the session on disk. The self-tests show a file read, a TCP
connect and an exec each end the worker with `SIGSYS`; the counter runs
end to end through a confined worker and paints the same quads as an
in-process driver. A dead worker ends the session with a reason and the
window stands.

**What the sandbox costs, measured.** On the 10 000-row table of spec 10
§1, a scroll frame is about 180 µs with the driver in this process and
355–500 µs through a worker: two round trips over the pipe, 80–100 µs
each, and 47 KB of draw list coming back — 96 bytes a quad, in the shape
the renderer uploads, against 27 bytes going out. Both are inside the
2 ms budget. Only the rows of an atlas that changed cross, not the
atlas. What the boundary
does not do yet: confine the worker on macOS (`sandbox_init`) or Windows
(AppContainer), where it is its own process but not a sandboxed one, and
fold an input into the paint that follows it, which would make it one
round trip a frame rather than two.

That measurement is what found the real cost of a scrolled frame, and it
was not the pipe. A virtualised list added its rows' heights up on every
frame — ten thousand additions and a prop read per row, to move a window
by a few pixels. The tops are now kept until something under the list
changes, which a scroll is not: `set_scroll` marks the node
`dirty::SCROLL` rather than `dirty::SELF`, and the layout keeps its row
tops across the frame. A scroll step went from 1.75 ms to about 180 µs in
this process, and from 2.03 ms — over budget — to under 500 µs through
the worker; the first paint of the table, which used to add the same
heights up twice, from 9.98 ms to 5.9 ms.

**A base under the catalogue, and what the count of it was.** The catalogue
was audited rather than remembered, and it did not come out well: of a hundred
and thirty functions, six answered `pointer_enter`, two answered `key_down`,
`focus.ring` was used once, and the word `disabled` did not appear at all.
Twenty-four widgets set `cursor: pointer` and gave no other feedback. What was
good was the theming — two hardcoded colours in 2 617 lines, both deliberate
scrims, everything else a role.

`control` is the base every interactive widget is built on now: one options hash
carrying a tone, a size, the caller's own shape, the handler map, the props, and
the semantics. Five tones, each a resting colour set plus a hover and a press
**delta** — deltas, so whatever geometry the caller ended up with survives into
all three states, which is the invariant `restyle` used to repair afterwards.
Disabling deletes the handler map, and that one act is right three times over —
no click reaches the server, the node leaves the Tab order for free, and the
cursor and colours come from one patch — and wrong a fourth time, because with
no click handler the accessibility mapping has no button to infer and the
control decays into an unnamed group. So `disabled` is a prop as well: it has to
be something a widget says, not something it stops doing.

Four are moved onto it — `checkbox`, `switch`, `tabs`, `icon_button` — each
keeping its positional arguments behind a trailing `o = {}`, so nothing that
called them changed, and the gallery renders to the same 762 quads over 902
nodes it did before. Twenty are still to move. The widgets emit their
accessibility props today and the client does not read them yet; teaching it to
prefer a declared role over an inferred one is a change to `a11y.rs` and one
paragraph of 03 §6, not to the protocol.

One thing the palette settled rather than the design: only `accent` carries a
hover and an active offset. The status roles have neither, so there is no
`danger.active` to reach for, and a pressed danger button goes to a sunken
surface and keeps its own colour in the label.

**Split panes, and the difference between blocked and unwritten.** Draggable
dividers on both axes, nested, with a minimum for each panel. A press on the
divider captures the pointer; `pointer_move` and `pointer_up` are handled on the
*container*, and since a pointer payload is measured against the node whose
handler catches it, the number reaching the server is already the divider's
position inside its container. The divider holds `key_down`, so it is in the Tab
order and the arrow keys move it.

The fraction is per mille, and both conversions round rather than truncate —
which is what makes the round trip exact, checked across every pixel of travel
rather than at a few samples. Truncating at both ends lost a pixel.

Panels are built by functions of their own width, so content can answer the
panel rather than the window. `bp` turned out to be useless for that: its rungs
are Tailwind's and they are a *window's*, so a 309 px panel and a 505 px one are
both `xs` and a view branching on it never branches. `pane_bp` is the same idea
at 200, 320, 480, 720.

Twenty-one assertions cover the geometry and the drag, in
`examples/demo-app/tests/`, run without a client because the functions are
pure. The one that matters most is that a `pointer_move` with no press does
nothing — without it, selecting text inside a panel would move the divider.
What is still missing is the latency: every frame of a drag is a round trip,
because a chunk cannot read the event that triggered it.

**The icon kind draws.** `icon` had been a defined kind since the first
version, laid out like a picture and painted by nothing: the painter's match
on node kind had no arm for it, so it fell through and drew a hole of the
right size. The catalogue worked around it with characters — a chevron was
`▾`, a tick was `✓` — which land on the 227 KB fallback symbols face and are
a poor icon three ways over: they cannot be sized against the control they sit
in, cannot take a colour apart from their label, and reach a screen reader as
themselves.

There is an arm now, and a table of thirty-three icons behind it, under
forty-seven names — fourteen of those names are aliases that reach for a shape
another one already draws, so a view can say `sort_asc` or `inventory` and get
`arrow_up` or `box`. Every icon is
polylines on a 24-unit grid with a 2-unit stroke, and every segment is the
same rounded capsule a chart's line is made of — so this cost no new pipeline,
no font, no asset fetch and no protocol version. A run of one point is a dot,
since a capsule of no length is a circle of the stroke's own radius. An icon
with no size takes a square from the font size in force, so one beside a label
needs no measurement from the server; an unknown name draws nothing and keeps
its space, which is what lets the set grow without a client release.

`spec/03` §2 says all of that now, including the two MUSTs an unknown name
carries. Two pixel tests hold it: a 40 px `close` leaves ink inside its rect
and none outside it, in `danger.base` rather than the text colour it would
have inherited as a glyph; and an unknown name draws no quads while keeping
its 40 px box.

Nothing in the catalogue draws an icon as a character any more: the select's
chevron, the accordion's and the tree's disclosures, the calendar's month
arrows, the chip's remove and the checkbox's tick and dash are icons.
`icon_button` still *takes* a character beside its `icon` option, so an
application that has not moved yet keeps working, and where both are given the
character is never drawn. Pagination gained something else on the
way: its ends are `disabled` now, so the page cannot be walked past either
end, which the character version had never stopped.

**A widget says what it is.** The accessibility mapping was kind and handler
alone: anything holding a `click` handler was a button named by the text
inside it. So a checkbox, a switch, a tab, a menu item and a slider all
reached AT-SPI, UIA and AX as "Button", carrying no state at all — a screen
reader user could not tell a switch from a link, or a ticked box from an
unticked one.

A node declares itself now, in props the client reads: a role from
forty names, a `label` that overrides the text gathered from inside,
and the states — `checked` with its third value, `expanded`, `selected`,
`disabled`, `read_only`, `required`, `invalid`, `busy`, `modal` — plus
`value_now` with its range, `pos_in_set`/`set_size`, `level`, `orientation`
and `live`. Props already travel, so **this cost nothing on the wire**: it is
`a11y.rs`, a new `spec/03` §6.1, and the worker's snapshot codec, which grew
a bitfield and three numbers.

Three rules came out of it and are normative. Leafness follows the *role*,
not the presence of a handler — a `tab_list`, a `menu` or a `grid` keeps the
children the old rule swallowed. A disabled node keeps its role, because
disabling a control means dropping its handlers and there would otherwise be
no button left to infer. And a present zero is not an absence: `value_now: 0`
is a slider at the bottom of its range, which is why the numbers travel
behind presence bits rather than as sentinels.

Sixteen vectors in `crates/eui-client/tests/a11y.rs`, named in `spec/09`
§7.1, including every role discriminant surviving the `to_u8`/`from_u8` round
trip that crossing the worker's process boundary makes of it. The
kind-mapping default is pinned separately in `driver.rs`, so a tree that
declares nothing is provably exposed as it was before §6.1 existed.

The catalogue is declaring its way through: checkbox, switch, tabs, segmented,
icon buttons, pagination, the calendar arrows, the chip's remove, slider,
progress, toast and the split divider say what they are. A run of the gallery
answers with six `TabList`s, sixteen `Tab`s, four `Separator`s, two `Slider`s
and two `Progress`es where each of those used to be a button or a group —
against three hundred and thirty-eight nodes that are still plain buttons:
menus, the select, the tree, the data grid, dialogs and the day cells. `SNAPSHOT_A11Y=1` on
the off-screen renderer prints the tree an assistive technology is handed,
which is the check that can run without a screen reader — the todo's rows
now read `CheckBox "Write the spec" checked=Yes` and
`Button "Remove Write the spec"`, where the second used to be `Button "×"`.

**What a server cannot do for itself.** It does not own `Tab` — save by
declaring `typing`, which is a node saying it is the sort of surface that
means the character. It was never told about `Escape` — the client handled the key and returned before anything
was reported. And it cannot know which node the client will treat as pressed.
So no dialog in the catalogue could trap focus, open with focus inside it, or
close on the key every dialog closes on, and no amount of server-side work
would have fixed any of the three.

Three props settle it, read by the client and specified in a new `spec/03`
§3.1. `modal` makes its own subtree the whole of the Tab order while it is
laid out — innermost first, so a dialog opened over a dialog traps inside the
second. `autofocus` puts focus inside a surface when it arrives, and
deliberately does *not* reclaim it on a later batch: a batch landing while
someone tabs through an open dialog must not pull them back to its first
field. `keys` names the keys a node wants.

That third one turned out to matter more than it looked. A `key_down` handler
used to receive **every** key, which is why a dialog could not simply listen
for `Escape` — it would hear each letter typed into the field inside it, and a
handler that closes on a key press closes on all of them. A node carrying
`keys` is now sent only what it named, and only those are withheld from the
client's own meaning: a tab can take `ArrowLeft` and `ArrowRight` and still be
activated by `Enter`, which was not expressible before. A node with a handler
and no `keys` prop hears everything, so nothing that worked stops.

`Escape` follows from it: it reaches a handler on the path that asked for it
and leaves focus alone, so the surface can put it back; with nothing listening
it drops focus, as always. `dialog`, `alert`, `confirm`, `sheet` and `drawer`
declare all of it now, and `spec/06` §3's "no global key capture" gained the
clause `keys` adds to it.

Seventeen vectors in `crates/eui-client/tests/keyboard.rs`, named in `spec/09` §7.1.
And the off-screen renderer grew `SNAPSHOT_KEYS`, so a focus ring, a trapped
`Tab` or a surface that closes on a key can be looked at without a keyboard:
driven through a real server, the gallery's sheet opens at 940 nodes carrying
`Dialog "A sheet" modal`, and one `Escape` later it is 934 nodes with no
dialog at all.

A printable key is now named by **what was typed**, not by what the layout
calls the key: `key_down` carries `event.text` when no control, alt or super
is held, and winit's `logical_key` otherwise. 06 §1's `key` is the W3C key
value, and for a printable that value is the character produced — `A` for
Shift+a, `é` for a dead key and an e. The backends do not agree about
`logical_key`, so an application that reads key names (a terminal, an
editor — anything `typing` exists for, and 03 §3.1 tells it in the same
breath that it will never receive `text_input`) saw every capital arrive
lowercase and every composed character not arrive at all.

On Wayland the character is no longer winit's word either, because winit's
word can be late. It fills `text` from an xkb state it moves only on
`wl_keyboard.modifiers`, and under Hyprland that event arrives *after* the
key it applies to often enough to matter: a fast `Shift`+`1` went out with
the shift bit set (the window keeps its own `held` bits since that was
found) and the name `1` — right bit, wrong name, and a terminal writes the
name. So the window now drives a keyboard state of its own. `eui-wayland`
asks the seat for a second `wl_keyboard` on winit's connection, the way it
asks for the data device, and takes from it only the **keymap**, the locks
and the layout group; every key winit reports is then fed to an
`xkb_state` this window owns (`xkbcommon-dl`, winit's own dlopen'd binding
— nothing new is linked), and what that state says the key types is the
name. A modifier key is a key to xkbcommon, so Shift is in force from the
moment the Shift press was seen, whatever the compositor says about its
modifiers afterwards. Dead keys go through the locale's compose table on the
same path. Seven tests in `crates/eui-wayland/src/xkb.rs` run this against a
five-key, two-layout keymap compiled from a string, so they need no
compositor and no `xkeyboard-config`: Shift in force the moment its key went
down, a repeat moving nothing, Caps Lock as a key and Shift undoing it, a
French row the other way round with AltGr as a third level, a `modifiers`
event that lies about what is pressed being disbelieved, and losing the
keyboard releasing what was held while keeping the locks. `EUI_TRACE=1`
prints the answer beside winit's on every key (`xkb !`), and
`EUI_WAYLAND_KEYS=0` turns it off, which is how a fault elsewhere is told
apart from this.

Which chords the window keeps is now the platform's answer, not one answer
everywhere: **`⌘T` / `⌘W` / `⌘V` on macOS, `Ctrl+Shift+T` / `Ctrl+Shift+W` /
`Ctrl+Shift+V` elsewhere** — the terminal emulator's convention, and the
reason for it is the same on both: plain `Ctrl+T`, `Ctrl+W` and `Ctrl+V` are
transpose, delete-word and quoted-insert, and they belong to the page. The
zooms keep the shell's modifier without Shift (so `Ctrl+_`, readline's undo,
reaches the application), and going back is `⌘←` rather than `Alt+←`
(`SHELL_MOD` / `BACK_MOD` in `eui-client/src/app.rs`, and 06 §1.3's sentence
about the back chord). It is the platform's convention and it is also what
makes an embedded terminal usable: `Ctrl+W`, `Ctrl+T` and `Ctrl+V` are a
word, a transposition and a quoted insert to every shell there is, and a
window that ate them on a Mac left the application unable to receive the keys
its subject is defined by.

`SNAPSHOT_KEYS` always runs before `SNAPSHOT_TEXT` and `SNAPSHOT_THEN` always
after it, which is the right order for a form that is filled and then
submitted and the wrong one for everything else: a second field reachable only
by a key pressed in the first could not be typed into at all. `SNAPSHOT_DRIVE`
interleaves them — `key:<k>` (the `$^~#` modifier prefixes as everywhere, and
the `key:` optional), `text:<what>`, `wait:<ms>` — each step waiting for the
server's answer the way a click does. A two-field sheet is now photographable
from the outside: `^b;t;n;text:A title;Enter;text:A description;$Tab;Enter`
opens it, fills both fields and submits, and the record it wrote can be read
back from the application's own store.

What this does not reach: roving focus and type-ahead, which a widget must
still do over the wire at a round trip per arrow, and accelerators, which need
the global key capture the specification still refuses.

Nor does it reach a modifier-click. `key_down` carries the modifier bits;
`click` carries `[x, y]` and nothing else (06 §1), and `double_click` and
`long_press` are decodable but no client emits either. So the catalogue's
multi-selection ticks one row at a time and offers select-all, and there is no
shift-click range. The fix is small and is a protocol change rather than a
catalogue one: a third element on `click`'s payload, mirroring the mouse
button the pointer events already carry.

**Files, in and out (01 §6, 03 §3.2).** The protocol had no way to move a
file in either direction, and no amount of catalogue work could add one: an
asset is named by its content and is the same for everyone, which is exactly
wrong for the invoice one person attaches and the export another asks for.
So files travel in the session, as `Upload` (C→S) and `Blob` (S→C) frames —
one shape, an id, a chunk index, a flag and at most 256 KiB.

What opens either is a **prop and a person**. A node carrying `pick` or
`save`, declaring a server handler for the event that answers it, opens the
platform's dialog when someone activates it — a click, or `Enter`/`Space` on
it — and only with `fs.pick` or `fs.save` granted. A tree that merely
arrives opens nothing; neither does a batch, a `wake`, or a local chunk.
There is no frame that opens a dialog.

The dialogs are the platform's own, through `rfd` — the XDG portal on Linux
rather than GTK — on their own thread, so a modal panel never stops the
window drawing. The window does the filesystem, as it does the socket and
the GPU: the worker cannot open a file and must not be able to, so what
crosses the pipe is a name, a size, and opaque bytes. A file is read two
chunks ahead of the socket and no further, so a large attachment costs the
same memory whatever it weighs.

The sharp edge is on the way in: a `Blob` for a node with no open save ends
the session, because that is a server trying to write a file nobody offered
it. A save's file is created on the first chunk, not when the path is
chosen, and an aborted transfer takes the partial file with it. No path
reaches a server, a dismissed dialog reaches nobody, and the ceilings are
the client's — a node's `max` may only be lower.

Twenty-one vectors in `crates/eui-client/tests/files.rs`, named in `spec/09`
§7.4, the last of them running both gestures against the reference server
over a real socket: the bytes of a picked file reach it, and the bytes it
owes a save come back. `examples/counter-server` grew an "Attach a file…"
and a "Save the count…" button, which is what that vector drives.

**A socket that breaks is not an application that ended (01 §4.1).** Before
this, `Incoming::Closed` printed a line to stderr and dropped the
connection. A wifi hop, a VPN reconnect, a laptop lid or a proxy's idle
timeout left a window standing there, looking alive, answering nothing —
with a half-filled form in it.

A session belongs to the server; the socket under it does not. `Hello` now
offers the session back — the id the server named and the last batch the
client applied — and `Welcome` answers whether it was taken. Resumed, the
client keeps its tree, its tables, its focus and what was typed into it, and
the server sends what it missed. Not resumed, the client discards all of it
before the `Mount` that follows, and it believes that answer over its own
memory: a tree kept against a server that has forgotten the session would
answer clicks the server cannot place. Because a replay may repeat a batch
that did land, a sequence already applied is acked again and ignored — a
`SetText` survives being applied twice and an `InsertChild` does not.

The client retries by itself: 300 ms, doubling to 30 s, and it says so
meanwhile — a `reconnecting` chip beside the trust chip in the address bar,
and in the title of a chromeless window. A session that ended for a reason
another socket cannot fix — a refused manifest, an `Error` frame, a tree the
client would not take — is not retried at all.

`examples/counter-server` is the reference for the server half: sessions
outlive their sockets by two minutes, the last 64 unacked batches are kept
for a replay, and a resume is refused outright rather than half-served when
either runs out. Six vectors in `crates/eui-client/tests/resume.rs`, named
in `spec/09` §7.5; the last drops a real socket mid-session and opens
another, and the tree is still standing with the count where it was.

*The Soli side of both has since been done*, and this paragraph used to say
otherwise long after it stopped being true. `src/serve/eui/session.rs` spools
an `Upload` to `tmp/eui-uploads/<session>/` and posts its own `file_upload`
event when the last chunk lands; `tree.rs` carries `pick` and `save`; and
`crates/eui-client/tests/soli_e2e.rs` drives the whole path against a live
server. What an application does with a spooled file is its own decision —
`uploaded_file_at` hands it to Soli's uploaders, and `eui_asset` puts the
bytes back on the wire without their ever having been a file (see
[components](components.md#files)).

## One thing that is worth knowing before it is rediscovered

`ControlFlow::WaitUntil` does not sleep. The window used to hand winit the
instant it wanted the next animation frame at, which is what that control
flow is for; measured, a window with a thirty-a-second animation passed
through `about_to_wait` **125 000 times a second** and cost an entire core.
Set unconditionally to a deadline a whole second out, it spun at the same
rate. `ControlFlow::Wait` sleeps on the instant.

Both platforms, which is what made it look like a Mac problem for a day —
`WaitUntil` is the piece of winit that Wayland and AppKit share, and macOS
runs the driver in the window's process, the configuration that spins twice
as fast.

So the loop parks on `Wait` and keeps its own deadline on a thread that
wakes it through the proxy (`Timer` in `app.rs`). An animating window costs
62 loop passes for 31 frames — one wake and one redraw each — and a window
with nothing animating never reaches `about_to_wait` at all.

`EUI_LOOP_STATS=1` prints one line a second — passes, frames, wakes by
source, window events by kind, mean sleep asked for — and is how this was
found. Reach for it before guessing: nothing outside a process can tell a
redraw loop from a chattering wake source from a cost on another thread.
Read the *mean* sleep, not the shortest; the shortest is a trap and cost two
wrong conclusions here.

## What the specification covers

Every document in `spec/` is normative now, and each names the code that
implements it and the vectors that pin it:

- **Wire format** (`02`) — `eui-proto`, byte-exact vectors, 64 rejection
  cases. The last of the style record's reserved bytes became `transition`.
- **Primitives, painting, focus, canvas paths, transitions** (`03`) —
  `eui-render` and `eui-client`.
- **Layout** (`04`) — `eui-layout`, goldens against a fixed-pitch measurer;
  §9 lists what version 1 leaves out.
- **Theme** (`05`) — `eui-theme`, contrast enforced by construction.
- **Events** (`06`) — `eui-client`, the driver tests. §5, touch, is new:
  no event kind was added for it, because a contact becomes the pointer
  before anything is emitted and a server cannot tell the two apart.
- **Bytecode** (`07`) — `eui-vm`, verifier and fuel; Soli compiles `local`
  handlers to it.
- **Security** (`08`) — each requirement naming where it is enforced.
- **Conformance** (`09`) — `cargo run -p xtask -- conform`.
- **Budgets** (`10`) — `cargo run --release -p xtask -- bench`.
- **Transport** (`01`) — the session, the one-shot render of §2.4, tree
  adoption (§2.6) and content-addressed assets are implemented, and so are
  the signed manifest and its pin store: `crates/eui-client/src/manifest.rs`
  verifies the Ed25519 signature, pins under `app_id` and refuses a changed
  key without a rotation, with vectors in `tests/manifest.rs` and
  `tests/install.rs`. This line said otherwise for some time after it
  stopped being true. Still specified and not built: the `pin` field of
  §2.1's manifest, which has no slot in the key table below it and no code
  in either repository.

## The browser: it paints, and what that cost

A fourth target, added on 2026-09-15 and described in
[Overview](/docs/overview): the same client compiled to
`wasm32-unknown-unknown`, drawing on a `<canvas>` so a page can put a running
application beside its source. It is a **viewing vehicle, not a platform**,
and the entries below are what that means in practice rather than in
principle.

**Measured, on the day it first drew:**

- The module is **7.02 MB** and **1.97 MB** over the wire with brotli, before
  `wasm-opt`, which is worth about another 30%. Built by
  `cargo run -p xtask-web`.
- **Eleven of the workspace's twelve crates cross-compile untouched**,
  `eui-render` and all of wgpu 23 among them. Only `eui-client`'s platform
  layer needed work, and the first check of it — the whole dependency graph,
  every capability `cfg` — left `app.rs` with exactly **one** error in three
  and a half thousand lines: a `pollster::block_on`. The window's logic was
  already portable; only its *asking* for a GPU was not, because a page may
  not block the one thread it has while a promise settles.
- **Idle costs nothing.** Zero WebGL draw calls in a second and a half with a
  session up, which is [Budgets](/docs/budgets) §1's zero-wakeup line holding
  on a target where the obvious implementation is a `requestAnimationFrame`
  loop. `ControlFlow::WaitUntil` on this backend *is* a `setTimeout`, so the
  frame timer thread is simply not built.
- The counter, the todo list, the gallery, the feed, the clock and the
  **10 000-row virtualised table** all render and lay out correctly, the
  table's 770 KB mount included.

**One trap worth naming, because every example gets it wrong.** Asking wgpu
for `BROWSER_WEBGPU | GL` in one instance looks right and fails silently on
the machines it was meant to help: a canvas has exactly one context for its
lifetime, so taking a `webgpu` one makes WebGL2 impossible afterwards — and a
browser that *exposes* `navigator.gpu` while handing out no adapter (a
privacy-minded one with its defences up; a Chrome with the flag off) then
draws nothing at all. WebGPU can be asked for an adapter with no surface,
which touches no canvas, so it is asked first and asked about nothing; the
canvas goes to whichever backend answered. This build draws through WebGL2 on
the machine it was developed on, and that is the path that would otherwise
have been lost.

**Cut, and cut deliberately:** the accessibility adapter (a canvas is opaque
to a screen reader, and pretending otherwise would be worse), the clipboard,
file pick and save, NFC, audio output, the tab strip, the signed manifest and
its pin store, and `scene` — the last for a technical reason rather than a
scope one: WebGPU reports a shader's validation errors asynchronously, so a
browser cannot say whether a module the *server* wrote compiled until after
it has been used, and a capability that cannot be checked is not offered.

**Driven end to end**, against a Soli server, in a browser, on the day
above: the counter's `+` — a local bytecode handler that then tells the
server — took it from 6 to 7; its `−`, a full round trip, took it back to 6;
a button lights under the pointer; and the 10 000-row table scrolls to
FA-1028 with its scrollbar, laying out only the rows that have boxes. The
canvas is 914×685 CSS and 1828×1370 device, which is the page's own box at
its own pixel ratio.

**Not cut, just not wired: a page always dials.** The one-shot render of
01 §2.4 — fetch the tree over HTTPS, draw it, open no socket until something
happens that only a server can answer — is `#[cfg(has_native_net)]` and so
native only. Nothing about it is unavailable here: the page's own `fetch`
already goes and gets assets, verifies a BLAKE3 before anything is decoded
and refuses a redirect, and the half above it that walks a body into frames
carries no platform at all. What is missing is `fetch_view` on this side and
the call that reaches for it. Until then the target that would gain most
from costing a server nothing is the one that always costs it a session.

**Honest about the rest:** TLS and the publisher key are the browser's, not
ours ([Transport](/docs/transport)), and the decoder does not get a process
of its own ([Security](/docs/security)). Both are named where they matter
rather than here.

## The phones: started, not finished

The portable half of the client is portable in fact and not only in
principle: `eui-proto`, `eui-tree`, `eui-theme`, `eui-layout`, `eui-text`,
`eui-vm` and `eui-shader` cross-compile clean for **`aarch64-linux-android`
and `aarch64-apple-ios`** today, with no `cfg` between them and the desktop.
That is the protocol, the session, the theme, the layout engine, the text
shaper, the bytecode VM and the shader verifier — and it needs neither an NDK
nor Xcode to check, because none of it touches C. The verifier belongs there
for the same reason the rest do: a phone that draws a scene has to decide
whether to compile its module, and that decision carries no platform. `conform` builds both and skips with a
note where a target's standard library is missing.

**Done, and tested where it can be tested here:**

- **Touch** (`06` §5). One contact, followed into the pointer: a tap, a
  drag when the node under the finger asked for moves, a scroll when it did
  not, and a fling when the finger was still moving as it left. The press a
  scroll began with is given back so no button fires under a thumb that was
  only scrolling. Twenty-one tests in `crates/eui-client/tests/touch.rs`; they run
  on any machine, because this is the driver and not the window.
- **Suspend and resume.** Android destroys the native window whenever the
  application goes to the background. The surface is dropped on the way out
  and made again on the way back — a surface still held at that point is a
  crash on return — and the finger on the glass is told the gesture ended.
- **The soft keyboard.** The focus change that tells a desktop input method
  it is welcome raises and dismisses the Android keyboard.
- **The entry points.** `crates/eui-android` is the shared object Android
  loads: `android_main` takes the activity, and the event loop is built on
  its looper. `cargo apk build -p eui-android` reads the packaging in its
  manifest. `crates/eui-ios` is the static library Xcode links: iOS has no
  `main` of ours, so `eui_start` is a function Xcode's own `main` calls once
  `UIApplicationMain` is up. Either way the session address is baked in at
  build time — `EUI_ANDROID_URL`, `EUI_IOS_URL` — because an application is
  one application and not a browser.
- **iOS asks least of the client**, because winit's UIKit backend already
  speaks both dialects that mattered on Android. `touchesBegan/Moved/Ended/
  Cancelled` arrive as `WindowEvent::Touch` with all four phases, so 06 §5
  runs there unchanged — it was written for Android and needed nothing for
  this. `set_ime_allowed` *is* `becomeFirstResponder`, so the focus change
  that welcomes a desktop input method raises the iOS keyboard through the
  same call, with no UIKit code of our own. And
  `applicationDidBecomeActive`/`WillResignActive` are `Resumed`/`Suspended`,
  which the surface already handles.
- **Somewhere to write.** No `$HOME` and no XDG on Android, so the pin
  store and the recent list use the directory the platform gave the
  application. Without a pin store there is no trust on first use and so no
  session at all.
- **The three parts that are a platform, not a feature.** There is no `rfd`
  behind the Android file picker, no `arboard` behind its clipboard, no
  AccessKit adapter behind TalkBack. Those crates are declared for every
  target but Android, and `eui-client/build.rs` turns the matching `has_*`
  cfg off there, so the code that calls them is not compiled rather than
  compiled and broken.

**Not done, and the first of them is the real one:**

- **The worker, on both.** Android will not `exec` a second binary out of
  an application's own storage; iOS has no `fork` and no `exec` at all —
  not restricted, absent. So the driver runs on a thread of the window's
  process, and `Backend::open` says so in as many words rather than looking
  like a worker that failed to start. The application sandbox confines the
  *process*; nothing holds the frame decoder apart from the renderer beside
  it, which is what 08 §10 is for. Seccomp on one thread would confine its
  system calls and not its memory, and the memory is the point — so it is
  not offered as a substitute. This is the piece that makes a phone a stage
  rather than a port, and iOS is the stricter of the two: Android at least
  has an `isolatedProcess` service to argue about.
- **Something to install.** `scripts/make-android-apk.sh` and
  `scripts/make-ios-app.sh` produce an APK and a `.app`, and CI builds both
  on every push to `main`, replacing one rolling prerelease so the README's
  download links never move, as well as attaching them to every `v*`
  release. The APK
  is debug-signed, so `adb install` works with no account; the iOS
  simulator build needs no signing at all; the iOS device build is unsigned
  because no runner has an identity, and wants `EUI_IOS_IDENTITY` or Xcode
  on the way to a phone. Both jobs are `continue-on-error`: a packaging
  break must not stop the client's own build from reporting, which is the
  thing people read.
- **A build anyone can check.** `ring` and `blake3` want a toolchain for
  the target, so nothing above `eui-text` has been compiled for either
  phone here, and no APK and no `.app` has been built or run. Every
  platform-only path *is* type-checked, by compiling it for the host with
  its `cfg` flipped — which is how an unreachable-code warning in the pin
  store was caught before it reached a device — but that is not the same as
  a build, and neither is a build the same as a device.
- **iOS needs a Mac for everything after `cargo check`.** Xcode project,
  Info.plist, code signing, a provisioning profile, a device. None of it
  can be done from Linux, and none of it has been.
- **App Store review is an open question, not a technical one.** Guideline
  2.5.2 forbids an application downloading and executing code, and EUI's
  local handlers are verified, metered bytecode that a server sends and the
  client runs. Games ship Lua interpreters and pass; EUI's premise — that
  the interface comes from the server — sits closer to the line. There is
  an argument to make (the VM reaches no file, no socket and no capability
  the manifest did not grant, and 07 is where that is enforced) but it is
  an argument, and it is better made before the packaging than after a
  rejection.
- Multi-touch: pinch, rotate, two-finger pan. §5 follows one contact and
  says so.
- The soft keyboard does not know what a field holds. The typed fields —
  `email_field`, `number_field`, `date_field` — should each raise a
  different keypad, and nothing in the tree says which.
- The shell (`chrome.rs`) is a tab strip and an address bar: the wrong
  shape for a phone. An APK built with `EUI_ANDROID_URL` is chromeless and
  right; one built without it opens the shell, which works and looks like a
  desktop.
- Safe-area insets around a notch, on either phone. winit's `WindowExtIOS`
  exposes what iOS needs — the home indicator, the status bar, and which
  screen edges defer system gestures — and none of it is called yet.

  The system back button **is** wired: winit maps Android's `KEYCODE_BACK` to
  `NamedKey::BrowserBack`, so it arrives as an ordinary key event and needs no
  JNI and no `android-activity` code of our own. The shell takes it before the
  application sees a keystroke (08 §7) and hands it to the page as `back`
  (06 §1.3); a page that holds no `back` handler does not get it, and the
  window closes instead — which matters more than it sounds, because winit
  reports every key but volume as *handled*, and a client that swallowed the
  gesture without somewhere to send it would leave a person inside an
  application with no way out.
- The platform trust store. `rustls-native-certs` finds nothing useful on
  Android, so the client falls back to the public roots and a development
  CA is not honoured — `EUI_CA_FILE` is the way in until it is.

## Six more servers: Ruby, Python, PHP, Node, Go and Rust

`clients/` holds the protocol's server half in six languages besides Soli,
each its own repository and each with no runtime dependencies to speak of:
the wire format, the session, the view encoder and its diff, assets, and the
signed manifest — three to five thousand lines apiece. The reference client
cannot tell which server it is talking to, and the bytes bear that out: nine
for a changed number, 2.8 KB to reverse five hundred keyed rows, byte for
byte across all seven. The counter renders to identical pixels out of every
one of them.

BLAKE3 is written out in each of them, because an asset is named by the hash
of its content and none of those standard libraries ships one. Ed25519 is the
runtime's where there is one and written out in Python and Rust where there
is not; the Rust crate, which takes no dependencies at all, also writes out
SHA-1 and base64 for the handshake. The publisher key is a PKCS#8 PEM all six
read — the same file produces the same manifest bytes, signature included,
out of every one of them, so an application that changes language keeps its
identity and nobody's pin breaks.

Their tests run in their own runners — 97 in Ruby, 103 in Python, 96 in PHP,
98 in Node, 98 in Go, 102 in Rust — and each carries the same two that matter:
the spec's §8 example at 150 bytes, and a client of forty lines that applies
the server's ops and compares the tree it ends up holding against the
server's own, over every permutation of five keyed rows and four hundred and
eighty random edits.

The one gap is TLS in Rust: the standard library has none, so that crate
answers `ws://` and belongs behind a terminator. The other five terminate
TLS 1.3 themselves.

None of them has local handlers, file transfers or session resume yet.
[Servers in six languages](/docs/clients) has the rest, including what each
costs against Soli on the same application.

## A catalogue that can write, not only show

The catalogue could show a document and could not compose one. The gap was
not the widgets — the picker, the drop zone and the attachment card have been
there since files worked — it was that there was nothing to put them in:
every long piece of text in this repository was a `textarea` with a line
under it saying which marks it understood.

`eui_builders_markdown.sl` is the fifth catalogue file, and the first half of
it is the renderer that was already here, promoted: `markdown`,
`markdown_file` and `md_doc_rows` were `markdown_builders.sl`, outside the
catalogue, while `doc/docs/eui/widgets.md` had listed `markdown` in Tier 1
since the beginning. They now travel with the other four, and
`scripts/sync-catalogue.sh` keeps every copy byte for byte. It learnt
pictures on the way: `![alt](src "1600x1200")`, where the title field carries
the measure because an `image` draws its texture across whatever box it ends
up with and this process cannot decode a JPEG to ask.

The second half is `markdown_editor`, and what is worth saying about it is
what it *cannot* be. A `text` node carries one weight for its whole run
(02 §3) and the client reports no caret (03 §3, 08 §7.1), so a field cannot
show a bold word inside a sentence and no button can wrap a selection. That
rules out a WYSIWYG of the kind a browser has, and no amount of work gets
around either rule.

What it leaves is better than it sounds, because 03 §3.1 rule 3 already
describes a block editor without naming one: a key in a position where it
does nothing is never the client's, and is reported if `keys` asked for it.
So `Enter` is the `submit` of an `input`, `Backspace` at offset 0 is reported
because there was nothing to delete, the arrows are reported because a
single-line field has no use for them, and `focus_to` puts the caret in the
block the server just made. A block is an `input` of an explicit pixel width,
which wraps and grows. The one thing lost is that `Enter` cannot split a
block at the caret — there is no caret to split at — so it adds an empty
block after the one being typed in.

The marks stay visible while a block is edited and become bold in the
preview: `B` and `I` mark the block, not a selection, because a selection is
not something a server can know. A hundred and sixty assertions in
`examples/demo-app/tests/markdown_editor_spec.sl` pin the model, the round
trip among them — a document that goes through the editor and comes back is
the document that went in.

Writing the toolbar meant fixing something older on the way. `text_decoration`
has been in the wire format since the beginning — offset 55, bit 0 underline
and bit 1 strikethrough — decoded, range-checked, and then ignored: the
painter drew nothing for either, so a style key the component documentation
offered did nothing at all. It draws them now, one rule per line of the shaped
run and no wider than the glyphs on that line, so a decoration on text that
wrapped underlines each line to its own end rather than drawing one bar the
width of the box. `spec/03-widgets.md` §2 says what it looks like; it said
nothing before, which is how a key stays inert for a year without anyone
calling it a bug.

The bar does not write either mark. It carries `B` and `I` and stops there:
`~~x~~` and `<u>x</u>` are read and drawn, because a document arrives from
somewhere else as often as it is written here, and a mark the editor mangles
is a word that changed meaning on the way through.

One thing the widget cannot do for itself: keep its toolbar on screen while
a long document scrolls. Sticky positioning is not in version 1 (04 §9) and
no scroll offset reaches the server to move a bar with (06 §8), so there is
nothing to follow the document with. What there is, is being *outside* the
scroller — which is what every masthead here already does — so
`markdown_editor_bar` draws the bar on its own and `bar: false` leaves it out
of the document. The mail composer puts it in the column above its scroller,
beside the Discard row.

Then six more, because the first version was an editor you could write a
paragraph in and not much else.

**Undo**, which is the one whose absence costs work. The document is a list
of small hashes, so the stack is a list of those lists and forty of them cost
less than the frame that drew them once. Typing is coalesced: `change` arrives
when a field goes quiet (06 §2), so an uncoalesced stack would walk back
through a sentence a breath at a time. `Ctrl+Z` reaches it because the block
**claims** `z` — and 03 §3.1 rule 1 says a printable character can never be
withheld, so the key report arrives *and* the letter still types. That is why
the modifiers are checked rather than assumed, and why the editor's frame
claims the same keys: a tick on a task leaves focus on a control, and dispatch
walks up from there.

**The `/` menu**, which is the gesture people reach for. Typing `/` at the head
of a block opens the list of kinds where the caret is, filtered by what follows
it, arrows to walk it and `Enter` to take one — and while it is open `Enter`
means the panel rather than a new block, because a key means what the thing in
front of you does with it.

**Moving a block**: `Alt` and an arrow, and a grip that drags. One drop zone for
the whole document rather than one per block, because a `drop` already reports
the slot it landed in (06 §6).

**Tasks**, **tables** and a **link** tool. A task is `- [ ]`, markdown's own,
and its marker is a real `checkbox`. A table is a grid of `input`s, each
carrying where it is, so one handler serves a table of any size and `Tab` walks
the cells for free. A link asks for its address in a `dialog`, because a bar
that grows a text box when you press one of its buttons is a bar that moves
under the hand.

Two things were learned the hard way and are written down where they bit.
**A tree that changes shape under a field loses what is in it**: the grip first
appeared on focus, which made the block a different node, so the client
replaced it rather than patching it and the first character typed after
clicking in went nowhere, silently. The grip is drawn on every block now and
only its contents change. And an array of arrays whose first element is a
string **lexes as a string** in Soli 2.3.7; a space after the outer bracket is
what makes it an array. The widget shipped a table whose rows were a sentence
until a spec assertion said so.

The gallery's Catalogue section has an entry to type in, and the mail example
composes with it: its letter is a document now rather than a `textarea`, its
pictures are written under `public/mail-att` rather than into the asset
store, and `mail_send_source` turns each one into a real MIME part with an
absolute address in the markdown on the way out. An `eui-asset:` address is
resolvable by a client talking to this server and by nothing else, which is
the whole reason the widget's own default is not what a mail uses.

## Not started

- The worker sandbox on macOS (`sandbox_init`) and Windows (AppContainer):
  the worker is its own process there, so a crash is contained, but it is
  not confined.

**Specified, and not built — found by auditing `spec/` against the tree on
2026-09-20, and written here rather than left to be rediscovered.** Each of
these is decodable, several are emitted by the Soli server, and none of them
does anything on a client.

Six came off this list the same day. **`slot` lays out as a column** (03 §1)
now: it fell to the default arm and a `StyleRecord`'s default `display` is
`Row`, so every `slot` laid out as the opposite of the one thing its row in
the table says. The fix is not in the style cache — that is keyed by style id,
and a `slot` and a `box` share id `0` constantly — but in a `by_kind` applied
after it. And **`sizer` is invisible**: it had no arm in `paint.rs` and drew
its background, border and shadow like any box; its children still draw, since
what is invisible is the node and not what is inside it. Vectors in
`eui-layout/tests/layout.rs` and `eui-render/tests/render.rs`.

**`double_click` and `resize`** (06 §1) are emitted now too. Both had a
payload, a coalescing rule, a decoder and a Soli server that maps the word,
and no client had ever produced one — so a view that attached a handler
waited for ever. §2 gained the two rules that were never written down: a
double is a second click on the same *handler* within 500 ms and is emitted
**as well as** that click, and a third click starts a new pair rather than
reporting a second double; a `resize` fires on a change of size and never on
a move, and a node's first layout reports nothing, since a size that was
never anything else has not changed. The watch list for `resize` is rebuilt
when the tree changes rather than walked per frame, so an application that
does not use it pays nothing.

**`paired` motion** (03 §5.3) is resolved now. A node arriving under the key
one that just left was wearing flies out of that node's box instead of coming
from a direction — a whole normative section, a style byte since the format
had one, and `lang`'s `tree.rs` mapping the word, against two sites in
`driver.rs` that both returned `None`. Nothing is laid out to do it: the
arriving node was laid out for this frame and the departing node's box was
recorded on the frame it was last seen, so the pair is one interpolation
resolved once, and the page animation of §5.1 is resolved at paint, where the
painting of the departing subtree still exists. §5.3's bound is 10 §"Going
somewhere"'s eight pairs a change; a ninth is painted where the layout put
it.

**And then it was finished**, because the paragraph above described something
no application could reach and that would have looked wrong if it could.
Three things, each of which the vectors of the day before passed straight
through.

The **offset was between the two corners** and the vertex stage scales about
the arriving node's own **centre** — 03 §5.2's `scale` is 92 % about that
centre, and the painter has one pivot. Composed, they put the element half
the difference in the two sizes away from the box it was supposed to fly out
of: always, since a shared element the same size on both sides is not one.
`paired` is the only motion that carries an offset *and* a scale, which is
how a corner delta stood there looking right — and the vector that pinned it
put both of its nodes at the same origin, where the two conventions agree by
accident. The new one differs in origin and in size, and there is a second, a
layer down in `eui-render`, on the composition itself: every other transform
vector in that file scales by one.

**Departures were read off the exit list**, which never had them. A release
notes only the **root** of what it let go — deliberately, since walking the
subtree would hand a client a hundred thousand ids to throw away — and a
server's diff matches keys among siblings, so a page swap names the page and
nothing inside it. The thumbnail a panel grows out of is never the node the
change names, so pairing fired for a shared element that *was* the page and
for nothing else. It is read from the box map now: a key whose recorded node
the tree no longer holds is a key that left, wherever it sat. That map is
swept at each paint rather than on a timer, which is also §5.2's "no paint
between them" made structural — a box is a partner's for one change and not
for the rest of the session. `Session::exits()` was added for the old route
and has gone with it; `take_exits` is what the page animation uses and is
untouched.

The map is keyed by `(owner, key)` now, not by the bare atom. 01 §2.7 gives
an island its own atom table, so atom 7 on the page and atom 7 in an island
are two different names wearing one number, and a bare key would have let a
page's shared element pair with an island's. (`Arena::by_key`, which a local
handler's `set_style` resolves through, is still bare — a wider fault than
this one and not fixed here.)

`MAX_PAIRS` was sixteen and is eight, which is what 10 §"Going somewhere" has
said all along. Sixteen was also undeliverable: a quad's transform slot is
four bits, `MAX_XFORMS` is fifteen, and the arriving and departing pages take
one each.

The catalogue can ask for one now — `shared_element(name, node)` beside
`nav_page`, restyling rather than wrapping for the reason `nav_page` gives —
and the ERP demo's customers section is one: narrow, the list and the detail
are two pages of a stack, and the account's disc flies from the row into the
header, 24 px to 36 px, across the whole page. Wide, the two are side by side
and nothing leaves, so no pair is made: two live nodes under one name is a
name that means two things.

Last, `EUI_TRACE=1` prints a line per pair — the two boxes when it resolves,
and which of the three ways it failed when it does not. §5.3 makes an
unresolved pair the ordinary case, which is exactly why a key spelt two ways
had been a page where nothing moved and nothing said so.

**06 §4's third check** exists now. The section asks a server to verify that
the node exists, that it carries a handler of that kind, and that the payload
has the shape §1 declares — and named `validate` as the place all three
happen. It did two. `EventKind::payload_fits` is the third, and it lives in
`eui-proto` rather than in a server because it is a fact about the wire
format and there are seven servers: six are separate ports of that crate and
the one in this workspace can call it. An integer is accepted where §1 asks
for a float — a coordinate of exactly zero is an integer to any encoder that
writes the narrowest form of a number — and never the reverse, since a
fractional button or row index is not a narrower spelling of anything.

§4 also said a failure "ends the session", which no implementation has ever
done and which `lang` argues against in twenty lines of comment. The spec now
says what the code does and why: the event is dropped, because ending a
session over one malformed frame costs a person their application for a mouse
movement, and makes a single bad frame an attack on every reader.

The rest are still open:

- **The manifest's `pin` field (01 §2.1).** Listed among the fields and
  absent from the normative key table three lines below it, so there is no
  wire encoding for it; `grep -ri spki crates/` is empty. 08 §1's "a client
  that supports pinning MUST honour them" is satisfied by no client
  supporting it.
- **`Ping` from the client (01 §7).** The rule is symmetric — whichever side
  has been silent for 30 s sends one — and only the server does. The client
  answers a `Ping` and never starts one.
- **`Blob` (01 §6), server to client.** `examples/counter-server` sends one;
  `lang/src/serve/eui/session.rs` refuses it and none of the six SDKs
  implements it. The client half is built and has nothing to talk to.
- Selecting text that is not in a field, and copying it. Selection and
  `Ctrl+C` belong to `input` and `textarea`; a table cell, a label or a
  `code_block` cannot be selected, which is a browser freebie people reach
  for without thinking. Related: there is no find-in-page.
- Printing, and anything that would produce a PDF.

## Scope, stated plainly

Writing a renderer means rewriting what a browser gives away for free: text
shaping, input methods, accessibility, selection. That is the real cost of
this project, and it is not hidden in a later milestone. The first stage
delivered the protocol, the layout engine, a renderer, a working client and
about fifteen widgets; the second added the rest of the catalogue, keyboard
focus, input methods, transitions, shadows and charts; the third put the
decoder in a confined process. Mobile is the fourth, and it is where the
third stops being free: the confined process it bought does not exist on
Android, and saying that plainly is worth more than a fifteenth widget.

## The Soli integration is additive

`lang/` is a production binary at 2.3.7, 294 339 lines. EUI enters it as
`src/serve/eui/` behind a cargo feature, `eui` — a child of `serve` rather
than the planned `src/eui/`, so it can reuse `serve`'s private helpers
instead of duplicating them. With the feature off, none of it is compiled.
It is **on** by default now, and was described here as off long after it
stopped being: it is in `lang/Cargo.toml`'s `default` list, which is why a
stale `rev =` pin on the eui crates takes the whole of `cargo test` down
with it rather than one optional job.
