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
[the wire format specification](/docs/wire-format): frames, batches, all sixteen
ops, the 64-byte style record, flat subtrees, values, handlers.

- No dependencies. Everything this crate touches came off a network socket, so a
  dependency here would be attack surface we did not write and cannot fuzz on
  our own schedule.
- `#![forbid(unsafe_code)]`.
- 78 tests: 9 round-trip, 6 byte-level vectors, 3 size budgets, 3 manifest,
  **55 rejection cases**, plus two bulk tests that throw 40 000 mutated and random buffers at
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

- Four faces embedded — Inter regular and bold, JetBrains Mono, Noto Sans Symbols for the hearts and arrows a text face lacks — all OFL. The font
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
- `examples/demo-app` holds three components as a Soli app — the counter,
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
encoding — verifies the hash before anything is decoded, decodes PNG, JPEG
and WebP, and
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
capability prompt in the window is still to come.

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
`chart_donut` build the paths server-side; the gallery shows all four. Each
one answers the pointer without a round trip: boxes over the drawing carry
local handlers that light the column under it and fade in a value chip, and
the donut's legend writes the reading into the hole.

**Keyboard focus, and the widgets that needed it.** The client walks `Tab`
order itself — editable fields and anything with a `click` handler, in
document order — draws the focus ring for keyboard and server focus only,
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
the slider has used one from the start — and `split_pane` now does the same.
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
browser, since no capability in 01 §2.1 lets a client open a URL — and
the two ordinary routes in `spotify_controller.sl` finish the
authorization code exchange. The `state` parameter is derived from the
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
sound does. Spotify's own catalogue never plays here, because the Web
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

**Keys, cursor, theme switch (03 §3, 07).** `ArrowDown`/`ArrowUp` land a
list on its next/previous row — the layout keeps each virtualised list's
row tops, so the feed's cards of two heights snap exactly — `PageDown`/
`PageUp` move a viewport, `Home`/`End` the whole way, all eased like a
wheel notch and chaining onto a scroll in flight. The pointer takes the
shape of what it is over: a `cursor` style, a beam on a field, a hand on
anything clickable. And a local handler can switch the viewer's palette —
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

There is an arm now, and a table of nineteen icons behind it, under
twenty-two names — `dash`, `sort_asc` and `sort_desc` are the three that reach
for a shape another name already draws. Every icon is
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
thirty-five names, a `label` that overrides the text gathered from inside,
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

Thirteen vectors in `crates/eui-client/tests/a11y.rs`, named in `spec/09`
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

**What a server cannot do for itself.** It does not own `Tab`. It was never
told about `Escape` — the client handled the key and returned before anything
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

Ten vectors in `crates/eui-client/tests/keyboard.rs`, named in `spec/09` §7.1.
And the off-screen renderer grew `SNAPSHOT_KEYS`, so a focus ring, a trapped
`Tab` or a surface that closes on a key can be looked at without a keyboard:
driven through a real server, the gallery's sheet opens at 940 nodes carrying
`Dialog "A sheet" modal`, and one `Escape` later it is 934 nodes with no
dialog at all.

What this does not reach: roving focus and type-ahead, which a widget must
still do over the wire at a round trip per arrow, and accelerators, which need
the global key capture the specification still refuses.

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

Eleven vectors in `crates/eui-client/tests/files.rs`, named in `spec/09`
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

*The Soli side of both is not done.* `lang/` pins the protocol crate by
revision, so nothing there has changed or broken: `src/serve/eui/` still
speaks the older `Hello`/`Welcome`, has no `pick`/`save` in its tree
builder, and does nothing with `Upload`. Taking these into Soli means
bumping that rev, adding the two event names to `tree.rs`, routing upload
chunks to a handler, giving a `file_save` handler a way to answer with
bytes, and keeping a LiveView session alive across sockets.

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

- **Wire format** (`02`) — `eui-proto`, byte-exact vectors, 55 rejection
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
- **Transport** (`01`) — the session and content-addressed assets are
  implemented; the signed manifest and key pinning are specified, not yet
  checked by the client.

## Android: started, not finished

The portable half of the client is portable in fact and not only in
principle: `eui-proto`, `eui-tree`, `eui-theme`, `eui-layout`, `eui-text`
and `eui-vm` cross-compile clean for `aarch64-linux-android` today, with no
`cfg` between them and the desktop. That is the protocol, the session, the
theme, the layout engine, the text shaper and the bytecode VM — and it needs
no NDK to check, because none of it touches C.

**Done, and tested where it can be tested here:**

- **Touch** (`06` §5). One contact, followed into the pointer: a tap, a
  drag when the node under the finger asked for moves, a scroll when it did
  not, and a fling when the finger was still moving as it left. The press a
  scroll began with is given back so no button fires under a thumb that was
  only scrolling. Ten tests in `crates/eui-client/tests/touch.rs`; they run
  on any machine, because this is the driver and not the window.
- **Suspend and resume.** Android destroys the native window whenever the
  application goes to the background. The surface is dropped on the way out
  and made again on the way back — a surface still held at that point is a
  crash on return — and the finger on the glass is told the gesture ended.
- **The soft keyboard.** The focus change that tells a desktop input method
  it is welcome raises and dismisses the Android keyboard.
- **The entry point.** `crates/eui-android` is the shared object the
  platform loads: `android_main` takes the activity, and the event loop is
  built on its looper. `cargo apk build -p eui-android` reads the packaging
  in its manifest; the session address is baked in at build time from
  `EUI_ANDROID_URL`, because an APK is one application and not a browser.
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

- **The worker.** Android will not `exec` a second binary out of an
  application's own storage, so the driver runs on a thread of the window's
  process. `Backend::open` says so in as many words rather than looking
  like a worker that failed to start. The application sandbox and SELinux
  confine the *process*; nothing holds the frame decoder apart from the
  renderer beside it, which is what 08 §10 is for. Seccomp on one thread
  would confine its system calls and not its memory, and the memory is the
  point — so it is not offered as a substitute. This is the piece that
  makes Android a stage rather than a port.
- **A build anyone can check.** `ring` and `blake3` want the NDK's clang,
  so nothing above `eui-text` has been compiled for the target here, and no
  APK has been built or run. The Rust is written and the packaging is
  declared; neither has met a device.
- Multi-touch: pinch, rotate, two-finger pan. §5 follows one contact and
  says so.
- The soft keyboard does not know what a field holds. The typed fields —
  `email_field`, `number_field`, `date_field` — should each raise a
  different keypad, and nothing in the tree says which.
- The shell (`chrome.rs`) is a tab strip and an address bar: the wrong
  shape for a phone. An APK built with `EUI_ANDROID_URL` is chromeless and
  right; one built without it opens the shell, which works and looks like a
  desktop.
- The system back button, and safe-area insets around a notch.
- The platform trust store. `rustls-native-certs` finds nothing useful on
  Android, so the client falls back to the public roots and a development
  CA is not honoured — `EUI_CA_FILE` is the way in until it is.

## Not started

- iOS (stage 3).
- The worker sandbox on macOS (`sandbox_init`) and Windows (AppContainer):
  the worker is its own process there, so a crash is contained, but it is
  not confined.
- Files and session resume **on the Soli side**: the client and the
  reference server do both, `lang/src/serve/eui/` does neither, and it
  cannot until the pinned protocol revision moves.
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

`lang/` is a production binary at 2.0.7, 276 000 lines. EUI enters it as
`src/serve/eui/` behind a cargo feature that is **off by default** — a child
of `serve` rather than the planned `src/eui/`, so it can reuse `serve`'s
private helpers instead of duplicating them. With the feature off, none of it
is compiled.
