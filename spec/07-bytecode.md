# 07 — Bytecode

Status: **normative** for version 1 of the chunk format; implemented by
`crates/eui-vm`. The `local { }` source syntax on the Soli side is not yet
specified; today a chunk is written as an assembly list in the view.

A local handler runs on the client without a round trip: hover, toggling,
optimistic increments, validation. It is the only code a client ever
executes that it did not ship with, and so it is the most constrained thing in
the protocol.

## 1. What a chunk may do

Read and write the component's **local state** — the props of the mounted
root node — set a node's text or a prop, switch the viewer's palette mode,
and emit a server event. Nodes are
named by **key**, the atom carried in the node's `key` field, so a chunk is
independent of any one render's ids and is interned once per session. Nothing
else. There is no I/O, no clock, no randomness, no allocation beyond the
operand stack and the strings it holds, and no way to reach another
component's tree.

A chunk's effect is **advisory**: the server re-derives everything from its own
state on the next round trip, and its next batch overwrites whatever the
chunk touched. Authorisation is never local.

## 2. Delivery

```
DefChunkBytes := id:varint  bytes:bytes        -- op 0x14, ≤ 64 KiB
DefChunk      := id:varint  hash:32×u8         -- op 0x13, fetched as an asset
```

Version 1 clients MUST accept `DefChunkBytes`; `DefChunk` by hash is the
cacheable path for when the asset endpoint exists and MAY be rejected until
then. A chunk id is defined once per session like any table entry.

## 3. Format

```
chunk := magic:"EUIC"  version:u8=1  max_stack:u8  code:u8...
```

`max_stack` is the depth the verifier must confirm; a chunk claiming more than
64 is rejected. `code` is a sequence of instructions, each an opcode byte and
its operands. All varints are as in [`02-wire-format.md`](02-wire-format.md).

| op | Name | Operands | Stack | Effect |
|---:|---|---|---|---|
| `0x01` | `push_int` | `svarint` | `→ int` | |
| `0x02` | `push_str` | `atom:varint` | `→ str` | the atom's value |
| `0x03` | `push_bool` | `u8` (0/1) | `→ bool` | |
| `0x04` | `load` | `atom:varint` | `→ any` | root prop, `null` if absent |
| `0x05` | `store` | `atom:varint` | `any →` | set root prop |
| `0x06` | `dup` | | `a → a a` | |
| `0x07` | `pop` | | `a →` | |
| `0x10` | `add` | | `int int → int` | wrapping |
| `0x11` | `sub` | | `int int → int` | wrapping |
| `0x12` | `mul` | | `int int → int` | wrapping |
| `0x13` | `neg` | | `int → int` | |
| `0x14` | `not` | | `bool → bool` | |
| `0x15` | `eq` | | `a b → bool` | same type and value |
| `0x16` | `lt` | | `int int → bool` | |
| `0x17` | `gt` | | `int int → bool` | |
| `0x18` | `and` | | `bool bool → bool` | |
| `0x19` | `or` | | `bool bool → bool` | |
| `0x1A` | `to_str` | | `any → str` | ints in decimal, bools as `true`/`false`, `null` as `""` |
| `0x1B` | `concat` | | `str str → str` | |
| `0x20` | `jump` | `i16` | | relative to the next instruction |
| `0x21` | `jump_if_false` | `i16` | `bool →` | |
| `0x30` | `set_text` | `key:varint` | `str →` | the text of the first node whose key is atom `key`, locally |
| `0x31` | `set_prop` | `key:varint atom:varint` | `any →` | that node's prop, locally |
| `0x33` | `set_style` | `key:varint style:varint` | | point that node at a style table id, locally |
| `0x32` | `emit` | `atom:varint` | | queue a server event named by the atom, payload = the root props |
| `0x34` | `set_mode` | | `str →` | the viewer's palette mode: `light`, `dark`, `high_contrast`, or `toggle` (light ⇄ dark); any other string stops the run. The viewer's choice made through the app's own control — never provisional, and the server learns it as the next `Viewport` |
| `0x40` | `return` | | | stop |

Type errors at run time — `add` on a string, `set_text` with an int — abort
the chunk. An aborted chunk has no effect on what it had not yet touched and
the client SHOULD fall back to the server handler if the node has one.

## 4. Verification

Before a chunk runs for the first time, the client MUST:

- reject a chunk over 64 KiB, with a bad magic, or an unknown version;
- decode every instruction; an unknown opcode or a truncated operand rejects
  the chunk;
- confirm every jump lands on an instruction boundary inside `code`;
- compute the stack depth along every path with the fixed effects above,
  rejecting an underflow, a depth above `max_stack`, or two paths reaching one
  instruction at different depths;
- confirm `code` ends in `return` or a jump.

A chunk that fails verification is never run, and the session continues:
the handler is simply absent.

## 5. Metering

Each instruction costs one unit of **fuel**; a run gets 4 096. String
concatenation is bounded at 4 KiB per value. Exhausting either aborts the
chunk as in §3. There is no wall-clock deadline because there is nothing a
chunk can wait on.

## 6. Dispatch

`Handler::Local(chunk)` runs the chunk and sends nothing.
`Handler::LocalThenServer { chunk, name }` runs the chunk, then sends the
event named `name` exactly as `Handler::Server` would, with the same payload.
Any `emit` inside the chunk queues an additional event. All events are sent
after the chunk finishes, in order, and none is sent if the chunk aborted.

The effects of a `LocalThenServer` chunk — every `set_text`, `set_prop` and
`set_style` it performs — are **provisional**: the client shows them at
once and puts the previous values back the moment the next server batch
arrives, before applying it. The batch's own ops then land on the tree
the server actually has, so a server that confirms the change sends it
and a server that ignores it sends nothing, and the client agrees with
the server either way without a flicker. A `Local` chunk's effects are
not provisional: hover and pressed states are restored by their own
events.

