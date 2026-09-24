# 02 — Wire format

Status: **normative**. Implemented by `crates/eui-proto`, verified by the test
vectors in [`09-conformance.md`](09-conformance.md).

## 1. Primitives

| Notation | Encoding |
|---|---|
| `u8`, `u16`, `u32`, `u64` | fixed width, little-endian |
| `f32`, `f64` | IEEE-754, little-endian |
| `varint` | LEB128 unsigned, at most 5 bytes for a `u32`, 10 for a `u64` |
| `svarint` | LEB128 over zigzag: `(n << 1) ^ (n >> 63)` |
| `bytes` | `varint` length followed by that many bytes |
| `str` | `bytes`, REQUIRED to be well-formed UTF-8 |

A `varint` MUST be minimally encoded: a decoder MUST reject a multi-byte
encoding whose final byte is `0x00`, and MUST reject a `varint` that would
overflow its target width. Non-minimal encodings are the classic route to
signature-bypass and cache-poisoning bugs, so they are an error here rather
than a normalisation step.

## 2. Session tables

A session owns four append-only tables. Ids are dense and assigned by the
server, starting at 1. **Id 0 is always "none"** and MUST NOT be defined.

| Table | Defined by | Max entries |
|---|---|---|
| Atoms | `DefAtom` | 65 535 |
| Styles | `DefStyle` | 65 535 |
| Colors (literal) | `DefColor` | 4 095 |
| Chunks (bytecode) | `DefChunk` | 4 095 |

Redefining an existing id is an error. A reference to an undefined id is an
error. Tables are **never cleared**: they are session-scoped and append-only,
and a session that needs a clean slate is a new session. This is what lets the
definitions in a batch precede the `Mount` that uses them.

### 2.1 Atoms

```
DefAtom := id:varint  value:str
```

An atom's value MUST NOT exceed 64 KiB, and the sum of all atom values in a
session MUST NOT exceed 8 MiB.

An atom is the unit of string deduplication. A server SHOULD intern any string
it will send more than once: property names, list keys, repeated labels,
enumerated values.

## 3. Style records

```
DefStyle := id:varint  record:StyleRecord
```

A `StyleRecord` is exactly **64 bytes**, fixed layout, no padding negotiation,
no optional fields. It holds *computed* style: every value is either a literal
number or a reference into the theme's token scales, and nothing in it needs
resolution against a parent.

| Off | Size | Field | Notes |
|---:|---:|---|---|
| 0 | 1 | `display` | 0 `row`, 1 `column`, 2 `stack`, 3 `grid`, 4 `none` |
| 1 | 1 | `wrap` | 0 `nowrap`, 1 `wrap`, 2 `wrap-reverse` |
| 2 | 1 | `justify` | 0 `start`, 1 `center`, 2 `end`, 3 `between`, 4 `around`, 5 `evenly` |
| 3 | 1 | `align_items` | 0 `start`, 1 `center`, 2 `end`, 3 `stretch`, 4 `baseline` |
| 4 | 1 | `align_self` | as `align_items`, plus 5 `auto` (inherit from parent) |
| 5 | 1 | `grow` | integer factor, 0–255 |
| 6 | 1 | `shrink` | integer factor, 0–255 |
| 7 | 1 | `gap` | `space` scale index |
| 8 | 3 | `basis` | `Dim` |
| 11 | 3 | `width` | `Dim` |
| 14 | 3 | `height` | `Dim` |
| 17 | 3 | `min_width` | `Dim` |
| 20 | 3 | `min_height` | `Dim` |
| 23 | 3 | `max_width` | `Dim` |
| 26 | 3 | `max_height` | `Dim` |
| 29 | 4 | `padding` | top, right, bottom, left — `space` scale indices |
| 33 | 4 | `margin` | top, right, bottom, left — `space` scale indices |
| 37 | 2 | `bg` | `ColorRef` |
| 39 | 2 | `fg` | `ColorRef` |
| 41 | 2 | `border_color` | `ColorRef` |
| 43 | 4 | `border_width` | top, right, bottom, left — device-independent px |
| 47 | 1 | `radius` | `radius` scale index |
| 48 | 1 | `shadow` | `shadow` scale index |
| 49 | 1 | `opacity` | 0–255, 255 = opaque |
| 50 | 1 | `font_family` | 0 `sans`, 1 `mono`, 2..9 an application font role (§5, `DefFont`) |
| 51 | 1 | `font_size` | `text` scale index; the default record carries `2`, `base` |
| 52 | 1 | `font_weight` | 0 `regular`, 1 `medium`, 2 `semibold`, 3 `bold` |
| 53 | 1 | `text_align` | 0 `start`, 1 `center`, 2 `end`, 3 `justify` |
| 54 | 1 | `line_clamp` | 0 = unlimited |
| 55 | 1 | `text_decoration` | bitfield: 1 underline, 2 strikethrough |
| 56 | 1 | `overflow` | 0 `visible`, 1 `clip`, 2 `scroll` |
| 57 | 1 | `position` | 0 `flow`, 1 `absolute`, 2 `pointer` (meaningful inside `stack`; see [`04-layout.md`](04-layout.md) §5) |
| 58 | 1 | `z` | stacking order within the parent |
| 59 | 1 | `cursor` | 0 `default`, 1 `pointer`, 2 `text`, 3 `grab`, … |
| 60 | 1 | `transition` | 0 none, else `motion` scale index + 1, at most 5 (05 §2): colours and opacity animate into this record (03 §5) |
| 61 | 1 | `animation` | bit set: 1 `spin` (the node turns about its centre while on screen), 2 `enter` (it arrives when mounted), 4 `exit` (its painting is kept while it leaves) — 03 §5 |
| 62 | 1 | `blur` | backdrop blur, as the standard deviation of a Gaussian in device-independent px; 0 none (03 §2) |
| 63 | 1 | `motion_kind` | 0 `fade`, 1 `leading`, 2 `trailing`, 3 `top`, 4 `bottom`, 5 `scale`, 6 `paired`: which way an `enter` arrives and an `exit` leaves (03 §5.2) |

A decoder MUST reject any enumerated field whose value is outside the range
defined above, any `animation` bit this revision does not define, and a
`motion_kind` on a record carrying neither `enter` nor `exit` — a direction
with nothing going that way. Rejecting unknown values rather than clamping
them is what keeps two implementations from silently diverging.

Offset 63 was this record's last reserved byte, and `motion_kind` is what it
was kept for. §3 fixes the record at 64 bytes, so there was never more than
one further field in it; the next one is a wider record and a version bump,
and that was always the price.

### 3.1 `Dim`

```
Dim := tag:u8  value:u16
```

| tag | Meaning |
|---:|---|
| 0 | `auto` — `value` MUST be 0 |
| 1 | device-independent pixels |
| 2 | percent of the parent's content box, in hundredths (`5000` = 50 %) |
| 3 | flex fraction (`fr`), in hundredths |
| 4 | `space` scale index — `value` MUST be ≤ 255 |

### 3.2 `ColorRef`

A `u16`:

- `0` — none / inherit from the parent
- `1..=0x7FFF` — a theme colour role id (see [`05-theme.md`](05-theme.md))
- `0x8000..=0xFFFF` — `value & 0x7FFF` indexes the session's literal colour table

```
DefColor := id:varint  rgba:u32       -- 0xRRGGBBAA, sRGB
```

Literal colours exist for brand marks and data visualisation. A server SHOULD
prefer roles: only roles follow the user's mode, contrast and density settings.

## 4. Nodes

```
Node := kind:u8  flags:u8  id:varint  style:varint
        [ key:varint      ]  -- if flags & 0x01
        [ text:TextRef    ]  -- if flags & 0x02
        [ props:PropList  ]  -- if flags & 0x04
        [ handlers:HList  ]  -- if flags & 0x08
        child_count:varint
        child_count × Node
```

Node ids are assigned by the server, MUST be non-zero, and MUST be unique within
a session for as long as the node exists. An id MAY be reused after the node has
been removed.

### 4.1 Kinds

| kind | Name |
|---:|---|
| `0x01` | `box` |
| `0x02` | `text` |
| `0x03` | `image` |
| `0x04` | `icon` |
| `0x05` | `input` |
| `0x06` | `textarea` |
| `0x07` | `scroll` |
| `0x08` | `list` |
| `0x09` | `canvas` |
| `0x0A` | `spacer` |
| `0x0B` | `divider` |
| `0x0C` | `overlay` |
| `0x0D` | `slot` |
| `0x0E` | `sizer` |
| `0x0F` | `audio` |
| `0x10` | `video` |
| `0x11` | `scene` |

This set is closed. Adding a kind is a protocol version bump, because it means
shipping a new client. `0x11` took this protocol to version 2: a client
that knows only version 1 meets it as a decode error and ends the session,
which is the deliberate price and the reason an application that asked for
`scene` raises the floor its manifest advertises (01 §2.1). An event kind is
the same bargain read from the other end — a client that does not know
`0x20` `level` (03 §7, 06 §1) fails the same way on the handler that names
it — and that took the protocol to **version 3**. An op is the same bargain
again: `0x15` `DefFont` and the font roles it binds (§5.1) took it to
**version 4**. The three are paid for differently, though: a kind is refused
at the handshake through the manifest's floor, while an event is left out at
encode time, because a capability is declared before anything renders and a
handler is a key in a view that has not run yet. A font role is both — an
application that declares one at boot raises its floor, and one that
declares a face mid-session falls back to `sans` for the sessions already
open rather than ending them. A scale step is the fourth way: the `space`
entries 13–17 (05 §2) took the protocol to **version 6**, and are paid for
like an event — a session below 6 is sent the older step each one falls
back to, so nothing is refused and nothing raises a floor. Everything users would call a widget — button, dialog,
table, date picker — is composed from these on the server; see
[`03-widgets.md`](03-widgets.md).

Kinds `0x02` `text`, `0x04` `icon`, `0x0F` `audio`, `0x10` `video` and
`0x11` `scene` MUST have no children. Kind `0x0A`
`spacer` and `0x0B` `divider` are *inert*: they MUST have no children, no text,
no props and no handlers.

### 4.2 `flags`

| bit | Meaning |
|---:|---|
| `0x01` | a `key` follows — an atom id: identity for reconciliation, and the name a local handler uses for the node |
| `0x02` | a `TextRef` follows |
| `0x04` | a `PropList` follows |
| `0x08` | a handler list follows |
| `0x10`–`0x80` | reserved, MUST be zero |

### 4.3 `TextRef`

```
TextRef := 0x00 atom:varint      -- interned
         | 0x01 inline:str       -- one-off, ≤ 4 KiB
```

### 4.4 `PropList`

```
PropList := count:varint  count × ( prop:varint  value:Value )
```

`prop` is an atom id naming the property. `count` MUST NOT exceed 64.

```
Value := 0x00                       -- null
       | 0x01 b:u8                  -- bool, MUST be 0 or 1
       | 0x02 n:svarint             -- integer
       | 0x03 f:f64                 -- float, MUST NOT be NaN or infinite
       | 0x04 atom:varint           -- interned string
       | 0x05 s:str                 -- inline string, ≤ 4 KiB
       | 0x06 hash:32×u8            -- asset reference (BLAKE3)
       | 0x07 c:ColorRef
       | 0x08 count:varint × Value  -- list, depth ≤ 4, count ≤ 1024
```

### 4.5 Handlers

```
HList   := count:varint  count × ( event:u8  Handler )
Handler := 0x00 name:varint                 -- server round trip, atom names the event
         | 0x01 chunk:varint                -- local bytecode chunk
         | 0x02 chunk:varint  name:varint   -- local first, then notify the server
```

`count` MUST NOT exceed 16. Event codes are defined in
[`06-events.md`](06-events.md).

A `0x01` handler runs entirely on the client and produces no network traffic.
A server MUST NOT place authorisation-relevant logic in a local handler: the
effect of every local handler is advisory and MUST be revalidated server-side
before it is trusted. See [`07-bytecode.md`](07-bytecode.md) §5.

## 5. Ops

A `Batch` frame is:

```
Batch := seq:varint  op_count:varint  op_count × Op
Op    := opcode:u8  payload
```

| opcode | Op | Payload |
|---:|---|---|
| `0x10` | `DefAtom` | `id:varint value:str` |
| `0x11` | `DefStyle` | `id:varint record:64×u8` |
| `0x12` | `DefColor` | `id:varint rgba:u32` |
| `0x13` | `DefChunk` | `id:varint hash:32×u8` — fetched as an asset |
| `0x14` | `DefChunkBytes` | `id:varint bytes:bytes` — inline, ≤ 64 KiB ([`07-bytecode.md`](07-bytecode.md)) |
| `0x15` | `DefFont` | `role:u8 count:varint count × 32×u8` — bind a font role to its faces, fetched as assets |
| `0x20` | `Mount` | `root:Node` — replaces the whole tree; tables persist |
| `0x21` | `Replace` | `node:varint subtree:Node` |
| `0x22` | `SetStyle` | `node:varint style:varint` |
| `0x23` | `SetText` | `node:varint text:TextRef` |
| `0x24` | `SetProp` | `node:varint prop:varint value:Value` |
| `0x25` | `InsertChild` | `parent:varint index:varint subtree:Node` |
| `0x26` | `RemoveChild` | `parent:varint index:varint count:varint` — the root cannot be removed |
| `0x27` | `MoveChild` | `parent:varint from:varint to:varint` |
| `0x28` | `SetHandler` | `node:varint event:u8 handler:Handler` |
| `0x29` | `ClearHandler` | `node:varint event:u8` |
| `0x2A` | `Focus` | `node:varint` |
| `0x2B` | `ScrollTo` | `node:varint x:svarint y:svarint` |
| `0x2C` | `Notify` | `title:str body:str tag:str` — §5.2 |

Definition ops (`0x1x`) within a batch MUST precede any op that references what
they define. A decoder MAY rely on this and MUST reject a forward reference.

`MoveChild` removes the child at `from` and re-inserts it at `to`, where `to`
indexes the list *after* the removal. Moving `[a, b, c]` with `from=0, to=2`
gives `[b, c, a]`.

`MoveChild` is what makes keyed list reconciliation cheap: reordering a
thousand-row table is *n* moves, not a rebuild. A server SHOULD emit
`MoveChild` whenever the keys of a child list are a permutation of the previous
keys.

### 5.1 Font roles

`DefFont` binds a `font_family` role to the faces that draw it. `role` MUST be
at most `9`; `count` MUST be at least 1 and at most 8. Each face is the BLAKE3
hash of an asset, fetched and verified like any other
([`01-transport.md`](01-transport.md) §2.2); `font_weight` selects among the
faces of a role, and nothing else does.

Roles `0` and `1` are `sans` and `mono`, and the client MUST have faces for
them before any op arrives. A `DefFont` on `0` or `1` replaces the client's
own face **for that session only** — which is what a theme's `font_sans`
asset means ([`05-theme.md`](05-theme.md) §3). Roles `2..9` begin bound to
nothing.

Unlike the other definition tables a role MAY be bound again: a role is a
slot the protocol already names, not an id a server hands out, so rebinding
is a change of mind rather than a redefinition. A client MUST discard what it
shaped under the old binding.

A style MAY name a role no `DefFont` bound, and a client MUST NOT fail the
session for it: it draws the run in `sans` and carries on. The same applies
to a role whose faces have not arrived yet, or whose bytes the client could
not read as a face. **Text is never not drawn because a font is missing** —
the only thing a server can do by naming a face badly is choose the wrong
typography for its own application.

A client MUST NOT resolve a face by name, by URL, or from the machine it runs
on. The faces a session shapes with are exactly the embedded ones and the
assets its own origin served ([`08-security.md`](08-security.md) §8).

### 5.2 Saying something to the person

`Notify` is the one op that names no node. What it changes is not the
document but what somebody is *told*: a line raised by the machine's own
notifier, which outlives the batch, sits outside the window, and is read by
a person who may not be looking at the application at all.

| Field | |
|---|---|
| `title` | At most `MAX_NOTIFY_TITLE` bytes. Required in practice: a notification with no title is a blank rectangle, and a client SHOULD show nothing rather than that. |
| `body` | At most `MAX_NOTIFY_BODY` bytes; may be empty. |
| `tag` | At most `MAX_NOTIFY_TAG` bytes; may be empty. An identity, not prose: a notification carrying the tag of one still on screen **replaces** it rather than stacking beside it, so ten replies to one thread are one notification. |

A client MUST NOT show one without the `notifications` capability
([`01-transport.md`](01-transport.md) §2.1), and MUST NOT fail the session
for it either: the op is decoded, counted against the limit below, and
dropped. A batch carrying one is a batch like any other — applied whole,
acked once — and a batch whose tree is refused shows nothing, because the
op never applied.

At most `MAX_NOTIFY_PER_BATCH` of them may appear in one batch, and a batch
carrying more MUST be refused. This is the only limit in §6 that bounds
something other than the client's memory: what it bounds is how often one
batch may interrupt somebody, which nothing else in this protocol measures.
A client MAY drop the oldest of several it has not shown yet.

**Nothing is reported back.** There is no event for a notification shown,
clicked, replaced or dismissed, and none for a machine that has no notifier
at all. An application learns exactly as much by sending one as it does by
sending an address to `open` (03 §3.5), and for the same reason: an answer
would be a probe for what this machine is and who is at it, with a clock
beside it ([`08-security.md`](08-security.md) §8). What a client MAY do with
a click is bring its own window forward.

A client MUST NOT hand any part of a `Notify` to a shell, and a title
carrying control characters MUST be cleaned or refused before it reaches a
platform notifier — a notification's text is a server's string, and every
platform has an argument parser somewhere behind it.

## 6. Limits

A conforming client MUST enforce all of these and MUST fail the session, not
truncate, when one is exceeded.

| Limit | Value |
|---|---:|
| `MAX_FRAME_BYTES` | 8 MiB |
| `MAX_TREE_DEPTH` | 256 |
| `MAX_NODES` | 1 000 000 |
| `MAX_ATOMS` | 65 535 |
| `MAX_ATOM_BYTES` | 64 KiB each |
| `MAX_ATOM_TOTAL_BYTES` | 8 MiB per session |
| `MAX_STYLES` | 65 535 |
| `MAX_COLORS` | 4 095 |
| `MAX_CHUNKS` | 4 095 |
| `MAX_CHUNK_TOTAL_BYTES` | 8 MiB per session, of inline chunks (`DefChunkBytes`), the page and its islands (01 §2.7) together |
| `MAX_FONT_ROLE` | 9 (roles `0`–`9`) |
| `MAX_FACES_PER_ROLE` | 8 |
| `MAX_CHILDREN` | 65 535 per node |
| `MAX_PROPS` | 64 per node |
| `MAX_HANDLERS` | 16 per node |
| `MAX_OPS_PER_BATCH` | 65 535 |
| `MAX_INLINE_STR` | 4 KiB |
| `MAX_VALUE_DEPTH` | 4 |
| `MAX_VALUE_LIST` | 1 000 000 elements (a windowed list's `heights`, 04 §7.1; bounded in bytes by the frame) |
| `MAX_NOTIFY_TITLE` | 256 bytes |
| `MAX_NOTIFY_BODY` | 1 KiB |
| `MAX_NOTIFY_TAG` | 64 bytes |
| `MAX_NOTIFY_PER_BATCH` | 4 (§5.2) |

`MAX_ATOM_TOTAL_BYTES`, `MAX_CHUNK_TOTAL_BYTES`, and the rule that an id is
defined once and referenced only after, are session state and belong to the
tree layer, not the decoder. *Enforced: `eui-tree::Session::apply`.*

The chunk total is one figure for the page and every island open on it, where
the atom total is per namespace: an island's `DefChunkBytes` is budgeted
against what the page already holds. Without it the ceiling was 64 KiB times
4 095 ids — 256 MiB a namespace, and a page with its islands has nine.
Every other limit is enforced by `eui-proto` alone.

`MAX_TREE_DEPTH` is enforced during decoding, before any recursion, so that a
hostile tree cannot exhaust the stack. An implementation SHOULD decode
iteratively with an explicit work stack; `eui-proto` does.

## 7. Records

The manifest (§01 2.1) and the theme document ([`05-theme.md`](05-theme.md)) use
one shared shape:

```
Record := magic:4×u8  version:u8  field_count:varint
          field_count × ( key:varint  value:Value )
```

Magic is `EUIM` for a manifest, `EUIT` for a theme. Keys are indices into a
fixed key table defined by each document type, not atoms — a record stands alone
and has no session to intern against.

## 8. Worked example

A two-node tree: a column containing the text "Hi".

```
op 0x10  DefAtom   id=1  "Hi"
op 0x11  DefStyle  id=1  ⟨display=column, padding=[4,4,4,4], bg=role 1⟩
op 0x11  DefStyle  id=2  ⟨font_size=3, fg=role 8⟩
op 0x20  Mount
         node kind=0x01 box    flags=0x00 id=1 style=1  children=1
           node kind=0x02 text flags=0x02 id=2 style=2 text=atom(1) children=0
```

Encoded:

```
10 01 02 48 69                    DefAtom  1 "Hi"
11 01 <64 bytes>                  DefStyle 1
11 02 <64 bytes>                  DefStyle 2
20 01 00 01 01 01                 Mount, box id=1 style=1, 1 child
      02 02 02 02 00 01 00        text id=2 style=2 text=atom 1, 0 children
```

**150 bytes**, of which 132 are the two style records. Those are sent once for
the whole session no matter how many nodes come to use them, which is the whole
point: the marginal cost of the *next* node is 5 to 9 bytes, not another copy
of its styling.

The measured comparison on a realistic table is in
[`10-budgets.md`](10-budgets.md) §2, and the test that produces it is
`crates/eui-proto/tests/size_budget.rs`.

Byte-level vectors — this example, the default `StyleRecord`, varints, frame
envelopes — are pinned in `crates/eui-proto/tests/vectors.rs`, so a second
implementation written from this document alone can check itself against the
same numbers. 64 rejection cases live in `crates/eui-proto/tests/reject.rs`.
