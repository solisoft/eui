# 10 — Budgets

Status: **measured** except where marked. `cargo run --release -p xtask --
bench` produces §2, §3 and §4 and exits non-zero when a budget is missed;
CI runs it.

Soli's own benchmark document publishes the rows it loses. This one does the
same: where a number comes from a run, it says so, and where it is still a goal,
it says that instead. A budget that was never measured is a slogan.

## 1. Runtime — targets, enforced by `cargo xtask bench` once the client lands

| Measure | Budget |
|---|---:|
| Launch → first pixel | < 80 ms |
| Idle, 200-node application | **0 % CPU, zero wakeups** |
| Idle RSS, 200-node application | < 25 MB |
| 10 000-row virtualised table, RSS | < 45 MB |
| 10 000-row virtualised table, scroll | 60 fps, < 2 ms CPU per frame |
| 10 000-row virtualised table, drag | 60 fps, < 2 ms CPU per frame |
| Client binary, stripped, 2 variable fonts included | < 12 MB |

A drag frame costs what a wheel notch costs: one hit test, one search for the
slot, at most one scroll step, and a relayout of the rows that have boxes — not
of the ten thousand that do not. The thing that can be slow is the **server's**
answer, and that is the honest limit here. A live reorder is a render and a diff
per boundary crossed, which the one-answer-at-a-time rule of
[`06-events.md`](06-events.md) §2 caps at a few dozen a second; past that the
client goes on following the hand, because nothing about the hand waits on the
server, and the rows arrive behind it. That degrades; it does not break.

Measured on 2026-09-08, `cargo build --release -p eui-client`, x86-64 Linux,
LTO, stripped: **15.64 MB** (15 644 096 bytes) with the default features and
**12.66 MB** with `--no-default-features`. Of the difference, 0.25 MB is the
two pictures beside PNG (03 §1) — JPEG 157 KB (`zune-jpeg`) and WebP 64 KB
(`image-webp`), each behind its own feature, both on by default. The rest is
the accessibility stack — AccessKit and, on Linux, the AT-SPI bus client it
needs (`zbus`), which brings its own async runtime beside tokio.

On 2026-09-06 the same build was 12.13 MB, one per cent over. It is a
quarter over now, and the budget stays: what grew is what the client
learned to do — sound and moving pictures decoded in the worker (03 §7,
§8: symphonia's WAV, FLAC, MP3 and Vorbis, GIF and animated WebP), still
pictures in the two other formats the web uses (03 §1), a
symbols fallback face at 227 KB beside the two variable ones, the desktop
theme's watcher, and the worker boundary itself. Naming the miss is the
point of this document; the levers are known, measured, and none of them
is free: the accessibility adapter (2.72 MB, by building without it), OGG
and Vorbis (0.72 MB — symphonia reserves FFT twiddle tables up to 65 536
points, half a megabyte of zeroes in `.data` because they are `Lazy`
statics rather than `.bss`), the four embedded faces (1.33 MB of
`.rodata`), and the shader translator wgpu needs at runtime (naga, 876 KB
of `.text`).

The zero-wakeup line is an architectural consequence, not a tuning parameter:
`winit` runs in `ControlFlow::Wait` and the client redraws only when a frame, an
input event, or a window event asked it to. There is no render loop in the code
to accidentally leave running.

### What the process boundary costs — measured

The configuration that ships puts the driver in a confined worker process
(08 §10), so the scroll budget above is met, or not, *over a pipe*. `cargo
run --release -p xtask -- bench` measures both sides on the same 10 000-row
table, x86-64 Linux, 2026-09-08:

| Measure | In this process | Through a worker |
|---|---:|---:|
| First paint (shaping) | 5.9 ms | 5.8 ms |
| Scroll step (median of ten, four runs) | 180–200 µs | 355–500 µs |
| of which the input round trip | — | 77–104 µs |
| what the boundary adds to a paint | — | 96–218 µs |
| bytes over the pipe per scroll step | — | 27 B out, 47.1 KB back |

Two round trips a frame — the input, then the paint — are most of the
difference, and the draw list is 128 bytes a quad — 112 until the quad
took on both ends of a transition (03 §5) — sent in the shape the
renderer uploads (the gallery is about 760 quads, its documentation dialog
about 3 900). A frame owed to a spin, a transition or a glide alone is not
sent at all: the window draws the last list again, and answers the
driver's ticks itself meanwhile. `xtask bench` holds those frames to their
budget: a repeated frame, a transition frame and a glide frame each under
0.2 ms of driver time — they measure at tens of nanoseconds — and a glide
with no layout after its first. The rest is layout and paint, which the boundary does
not change; the in-process figure is steady and the worker's is not,
because it includes two process wake-ups. Both sides are inside the 2 ms
budget, and folding an input into the paint that follows it would make it
one wake-up a frame.

A scroll step was 1.75 ms in this process and 2.03 ms through a worker
until 2026-09-08, when the row tops of a virtualised list stopped being
rebuilt on every frame (04 §7): the tops are the rows' heights added up,
and a scroll changes neither. `set_scroll` marks a node `dirty::SCROLL`
rather than `dirty::SELF` for exactly that reason.

### What a blurred frame costs

A `blur` (03 §2.1) is the one thing in the client that is not a single
pass, so it is worth saying what it is allowed to cost. A frame with no
blurred node in it — every frame of the applications measured above —
takes none of this: no texture is allocated, no pipeline is built, and the
frame is the one pass and one draw per scissor run it always was. The rows
above are therefore unaffected, and are meant to stay that way.

### The shaped-run cache

The text engine keeps shaped runs in a cache of **16 384** entries, keyed by
the run, the face, the size and the width it was measured at. The size is a
budget of its own: a page whose working set does not fit is not merely a
cache miss, it is a *guaranteed* one, because an entry evicted before its
next use will be shaped again — and layout measures the same run at several
widths on the way to a line break.

A 630-line markdown document — 4 400 nodes, one per word, because a
paragraph that flows across lines cannot be one text node (03 §1) — asks for
23 324 shapes against a 4 096-entry cache and 7 785 against this one: the
difference is thrash, and it was 334 ms of layout against 137 ms in a
release build. Eviction is first-in, first-out; with a cache that holds a
page that is enough, and a page larger than this one would thrash again.

Two more things were found on the same page. A run that fits on one line is
the same run at every width that holds it, so the engine answers a bounded
request from the run's natural shape whenever that fits (`Stats::reused`):
7 785 shapes became 5 713. And a `text` leaf with no height of its own is as
tall as its lines whatever room it was offered, so layout memoises every
non-`Exact` height constraint as one: 53 477 uncached measures became
21 617. Together, release layout of that page went 334 → 108 ms; a debug
build, which shapes text at a twentieth of the speed, 6.9 s → 1.9 s.

What remains is the shape of the work, not its cost: a paragraph that flows
around a styled word is one node per word (03 §1), and a wrapping row
measures every word for its line, its minimum and its cross size, once per
pass of each ancestor. A page that long wants the protocol's own answer to
long content — a windowed `list` (04 §7.1) laying out only the rows in view —
rather than a faster full layout.

Measured 2026-09-09: `RSS growth: session + layout, 10k rows` 25.8 MB and
`driver RSS growth, table-10k with real text` 17.7 MB, both against the
45 MB line above — the cache only fills as runs are shaped, so an idle
application holds a few hundred entries, not sixteen thousand.

A frame that *does* carry one adds, per distinct radius, three passes over
a snapshot of the region that asked — the union of the blurred rects grown
by three standard deviations, clipped to the window — and one pass to take
that snapshot. The snapshot is a texture the size of that region, not of
the framebuffer: a 360 px dialog is a few hundred kilobytes where a 4K
window would be 33 MB. It is freed when a frame stops asking, so a closed
dialog costs nothing, which is what keeps the idle RSS line above honest.

The reduction is chosen so the kernel is about fifteen taps whatever radius
was asked for, so a wider blur buys a smaller texture rather than more
samples and the cost does not grow with the radius. What it does grow with
is the *area*, and a scrim covers the window: that is the case to measure
before this line can stop saying "goal".

## 2. Wire — measured

Source: `crates/eui-proto/tests/size_budget.rs`, run with `--nocapture`. The
subject is a 50-row × 4-column invoice table: 256 nodes, rows keyed, six shared
style records. The HTML side is the same table with the Tailwind classes such a
table really carries, unindented — the favourable case for HTML.

| | Bytes |
|---|---:|
| Cell and header text (identical both ways) | 2 337 |
| **EUI total** | **4 619** |
| EUI, with the status column interned | 4 209 |
| EUI, of which style records (once per session) | 396 |
| EUI structure only | 2 282 |
| **HTML total** | **14 362** |
| HTML structure only | 12 025 |

**Total 3.1×. Structure 5.3×. 8.9 bytes of structure per node.**

The text is the data; neither side can compress it away, so the honest headline
is the structural ratio. The regression budget is set at **4×**, below the
measured 5.3×, so a real regression trips it and ordinary drift does not.

| Operation | Measured | Budget |
|---|---:|---:|
| Single-cell update | 22 B | < 40 B |
| Reversing 50 keyed rows | 201 B | < 300 B |
| Same reversal by re-mounting | 4 619 B | — |

That last pair is the case for `MoveChild`. Soli's current LiveView diff works
on lines of an HTML string, so reordering a table has no cheap spelling at all;
here it is 49 moves and 201 bytes.

### What this does not claim

Bytes on the wire were never the main prize. A 3.1× reduction matters on a slow
link, but the reason EUI exists is what the *client* does not do with those
bytes: no tolerant parse, no selector matching, no cascade resolution, no reflow
of an untyped tree, no JIT. Those are the numbers in §1, and they are the ones
worth judging the project on once the client can be measured.

## 3. Decoder and session — measured

| Measure | Measured | Budget |
|---|---:|---:|
| Decode a 4.4 KB batch | 18 µs | < 50 µs |
| Decode the 10 000-row mount, 770 KB | 3.1 ms | < 20 ms |
| Apply that mount into a session, 50 002 nodes | 6.4 ms | < 30 ms |
| Layout of the table, virtualised | 0.14 ms | < 5 ms |

## 4. Wire — the table, text included

The §2 figure is structure; a real mount carries the cells too. The
10 000-row invoice table mounts in 769.5 KB, **15.8 bytes per node** with the
text, of which about 9 are structure and the rest the data itself. Budget:
18 B per node.

## 5. Files

| Measure | Budget |
|---|---|
| One transfer chunk | 256 KiB |
| One upload, ceiling | 64 MiB — a node's `pick` may ask for less |
| One upload, default | 16 MiB, when `pick` names no ceiling |
| One save | 256 MiB, whatever the server sends |
| Abort reason | 256 B |
| Memory held while a file uploads | two chunks, whatever the file weighs |
| Dialogs open per node | 1 |

The upload ceiling is the client's; the tree's is whatever `max` says, and
it may only be lower. The save ceiling exists because the person chose where
a file goes and not how much of their disk it may take.

Nothing here is a stream: a transfer belongs to its session and does not
survive it, and the memory it costs is fixed by the chunk size rather than
by the file — the disk runs two chunks ahead of the socket and no further.

## Where the machine is, and what it is held against

| Measure | Budget |
|---|---|
| Nodes asking to be placed at once | 2, the rest ignored |
| Fastest interval | 1 s, whatever `locate` asks for |
| Coordinate resolution reported | 0.001°, about 110 m |
| Accuracy reported, floor | 100 m |
| Scans open per node | 1 |
| Tags per scan | 1 |
| Records kept off one tag | 64 |

A fix costs a radio rather than a timer, which is battery on the only two
platforms that have one — so the floor is a second and not the hundred
milliseconds a clock gets, and the count is two and not four. Neither is
negotiable by the tree: a `locate` of 10 is a `locate` of 1000, silently,
because a server that could argue about it would.

The resolution and the accuracy floor are in this table rather than in the
client's judgement for the same reason every other number here is: a
budget a reader cannot check is a promise, and this one is a privacy
promise.

## Sound

| Measure | Budget |
|---|---|
| Sources playing at once | 8, a ninth refused |
| Frames buffered ahead of the device | 200 ms |
| CPU with nothing loaded | 0 — the device is closed |

Mixing is a multiply and an add per sample per source, with one linear
interpolation for the rate; the cost is in the decode, which happens once
per sound.

## Moving pictures

| Measure | Budget |
|---|---|
| Pixels a frame | 1920 × 1080 |
| Frames a picture | 3 600 |
| Decoded frames a picture | 96 MB |
| Uploads per frame shown | 1, and none while the frame does not change |
| CPU while paused | 0 — nothing is scheduled |
