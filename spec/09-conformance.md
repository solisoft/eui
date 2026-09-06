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

## 2. Wire format — `crates/eui-proto/tests`

| File | Pins |
|---|---|
| `vectors.rs` | Byte-exact encodings: a 64-byte style record, varints, the counter's Mount batch, every op |
| `roundtrip.rs` | Every frame, op, value and record survives encode → decode unchanged |
| `reject.rs` | 47 malformed inputs, each refused with the named error and no allocation past the limit: truncation, non-minimal varints, trailing bytes, unknown enums, reserved bits, oversized lists, depth |
| `size_budget.rs` | The counter's Mount fits 576 B and a click 25 B |

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

## 8. Painting — `crates/eui-render/tests/render.rs`

One quad per box and per glyph; scissor runs for scroll containers; device
snapping at 2×; shadows; canvas paths; and, where a GPU adapter exists,
pixels read back from an off-screen target for the clear colour, a filled
box, a clipped scroll and a canvas line.

## 9. Diff — `lang/src/serve/eui/diff.rs` (feature `eui`)

Keyed rows move rather than rebuild; mixed keyed and unkeyed children match
by position and key; a changed cell is one `SetText`.

## 10. End to end — `crates/eui-client/tests/soli_e2e.rs`

With `EUI_SOLI_BIN` set, the client drives a real Soli server: the counter's
local-first `+`, the todo's keyed rows and fetched avatar, ten thousand rows
sorted by moves, and the gallery's select, slider, pickers and charts.
