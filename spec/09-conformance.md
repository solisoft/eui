# 09 — Conformance

Status: **normative**. `cargo run -p xtask -- conform` runs everything below
and fails on the first deviation.

## 1. What conformance means

An implementation of EUI/1 is **conforming** when it passes the vectors in
this document's harness. The vectors are executable, live beside the code
they pin, and are the tie-breaker when prose and behaviour disagree: a
vector that contradicts the prose is a bug in the prose until the vector is
changed, and changing a vector is a protocol change.

Two profiles:

- A **client** MUST pass §2 (wire), §3 (tree), §4 (layout), §5 (theme),
  §6 (bytecode) and §7 (events); §8 (painting) applies when it draws.
- A **server** MUST pass §2 and §6 for what it emits, and §9 (diff) if it
  patches rather than re-mounts.

The harness also builds `eui-proto`, `eui-tree`, `eui-theme`, `eui-layout`,
`eui-text` and `eui-vm` for `aarch64-linux-android` where that target's
standard library is installed, and skips it with a note where it is not.
This is not a vector and nothing conforms by passing it. It is there because
those six crates carry no `cfg` for any platform and the claim is worth
keeping true: a client for a phone, or for anything else, starts by taking
them unchanged.

## 2. Wire format — `crates/eui-proto/tests`

| File | Pins |
|---|---|
| `vectors.rs` | Byte-exact encodings: a 64-byte style record, varints, the counter's Mount batch, every op |
| `roundtrip.rs` | Every frame, op, value and record survives encode → decode unchanged |
| `reject.rs` | 55 malformed inputs, each refused with the named error and no allocation past the limit: truncation, non-minimal varints, trailing bytes, unknown enums, reserved bits, oversized lists, depth |
| `size_budget.rs` | The counter's Mount fits 576 B and a click 25 B |
| `manifest.rs` | The manifest record round-trips, its signed bytes are rebuilt exactly, malformed records are refused |

`roundtrip.rs` and `vectors.rs` cover 01 §4.1 and §6 as well: a `Hello` that
offers a session back, a `Welcome` that resumed one, and a transfer in each
direction with each of its three flags, byte for byte. `reject.rs` refuses an
unknown transfer flag, an oversized chunk, an oversized abort reason, an
unknown resume tag, an unknown `resumed` byte, and a `Hello` that ends
before its resume flag.

## 3. Tree — `crates/eui-tree/tests`

Tables never cleared; a batch that fails leaves the session poisoned until
the next `Mount`; every reference (atom, style, chunk, node, key) validated
before anything is placed; `MoveChild` indexes after removal.

## 4. Layout — `crates/eui-layout/tests/layout.rs`

Goldens against the fixed-pitch measurer (9 px per character, 22 px lines):
flow, grow, shrink with the automatic minimum, wrap, justify, align, baseline,
percent, stack (stretch on both axes, absolute layers), grid, scroll clamping,
virtualised lists, display none, depth 255 within a 1 MiB stack.

## 5. Theme — `crates/eui-theme/tests`

Every default pair in 05 §6 meets its contrast; a seed produces a ramp whose
`on` colour passes against `base`; scale indices out of range are refused;
the easing curve is monotone and pinned at both ends.

## 6. Bytecode — `crates/eui-vm/tests`

The verifier refuses bad jumps, stack underflow and overflow past 64,
unknown opcodes and oversized strings; fuel exhaustion aborts without effect;
every opcode's stack effect is exercised.

## 7. Events — `crates/eui-client/tests/driver.rs`

Click resolution to the nearest handler, payloads local to the handler's
node, press and release on different targets, focus by pointer and by `Tab`
with wrapping, `Enter` and `Space` as clicks, `Escape`, the ring for keyboard
and server focus only, editing and commit, wheel and scroll offsets, local
handlers with and without a following server event, resync on a bad batch,
transitions on a style change.

### 7.1 Keyboard — `crates/eui-client/tests/keyboard.rs`

§3.1: `Escape` reaching a handler on the path and leaving focus alone, and
still only blurring when nothing asked for it; `modal` keeping `Tab` inside a
subtree, nesting so the innermost wins, and leaving an unmodal page alone;
`autofocus` placing focus when a surface arrives and *not* reclaiming it on a
later batch; and `keys` letting a node take the arrows while `Enter` stays the
press it stands for, sending it no key it did not name, and leaving a node
without the prop hearing everything.

### 7.6 Touch — `crates/eui-client/tests/touch.rs`

06 §5: a tap reaching what it landed on, and still reaching it after five
pixels of tremor; a stroke past the slop scrolling the view and never
clicking the row it began on; the press given back before the view moves; a
node that asked for moves taking the whole stroke while the view under it
stays put; a second contact ignored while the first is live; a cancelled
gesture releasing without a click and forgetting the contact; a finger still
moving as it leaves carrying the view on, and a finger that stopped first
not doing so; and a window that loses the input forgetting the finger that
was on it, so the next one is a gesture and not a second contact.

The tests are the driver's, not the window's — a contact becomes the pointer
before any platform is involved — so they run everywhere and want no touch
screen.

### 7.4 Files — `crates/eui-client/tests/files.rs`

Spec 03 §3.2 and 01 §6, eleven vectors. A click on a node carrying `pick`
asking the window for a dialog with the accept list, multiplicity and
ceiling the tree declared; **nothing opening** without the capability, and
nothing opening for a tree that merely arrived; what was picked becoming one
`file_pick` naming the file and not its path, then chunks that fit a frame;
a file past the ceiling announced and aborted, and accepting no bytes after
it; a dismissed dialog reported to nobody and its token spent; a save
becoming a `file_save` and the server's `Blob` frames becoming writes for
the window; a `Blob` for a save nobody asked for **ending the session**; and
a `Blob` out of order losing the save without ending it. The last vector
runs the two gestures against the reference server over a real socket: the
bytes of a picked file reach it, and the bytes it owes a save come back.

### 7.5 Resuming — `crates/eui-client/tests/resume.rs`

Spec 01 §4.1, six vectors. A first `Hello` offering nothing and a later one
offering the session and the last batch applied; a `Welcome` that did not
resume taking the tree, the tables and the acked sequence with it; a
`resumed` naming a session the client never offered ending it; a batch
already applied being acked again and **not applied again**. The last drops
a real socket mid-session and opens another: the tree is still standing, the
count is where it was, and the session goes on.

### 7.2 Accessibility — `crates/eui-client/tests/a11y.rs`

§6.1's declared half: a `role` prop beating the kind it would have been
inferred as; `checked` in all three of its states; a `label` overriding the
text inside; a disabled node keeping its role and accepting neither action;
a container role keeping the children a leaf role would swallow; `set_size`
carrying a count virtualisation left out of the tree; a slider's value and
range, including a present zero; an unknown role name falling back rather
than failing; a live region's urgency; and every role discriminant surviving
the `to_u8`/`from_u8` round trip the worker boundary makes of it.

The kind-mapping default of §6 is pinned separately, in `driver.rs`, so that
a tree declaring nothing is provably exposed as it was before §6.1 existed.

### 7.3 Manifest — `crates/eui-client/tests/manifest.rs`

Signature, protocol range, trust on first use, refusal of a changed key,
acceptance of a rotation the pinned key signed, and garbage.

## 8. Painting — `crates/eui-render/tests/render.rs`

One quad per box and per glyph; scissor runs for scroll containers; device
snapping at 2×; shadows; canvas paths; the backdrop a `blur` asks for — that
a frame without one asks for none, that the region is the blurred rects
grown by three standard deviations and no more, and that two radii are two
chains; and, where a GPU adapter exists, pixels read back from an off-screen
target for the clear colour, a filled box, a clipped scroll, a canvas line,
and a frosted pane carrying each half of a seam into the other — both when
it covers the whole frame and when it covers a part of it, which is what
pins the backdrop's origin.

## 9. Diff — `lang/src/serve/eui/diff.rs` (feature `eui`)

Keyed rows move rather than rebuild; mixed keyed and unkeyed children match
by position and key; a changed cell is one `SetText`.

## 10. End to end — `crates/eui-client/tests/soli_e2e.rs`

With `EUI_SOLI_BIN` set, the client drives a real Soli server: the counter's
local-first `+`, the todo's keyed rows and fetched avatar, ten thousand rows
sorted by moves, and the gallery's select, slider, pickers, data grid and charts.
