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
| 50 | 1 | `font_family` | 0 `sans`, 1 `mono`, 2+ granted font roles |
| 51 | 1 | `font_size` | `text` scale index; the default record carries `2`, `base` |
| 52 | 1 | `font_weight` | 0 `regular`, 1 `medium`, 2 `semibold`, 3 `bold` |
| 53 | 1 | `text_align` | 0 `start`, 1 `center`, 2 `end`, 3 `justify` |
| 54 | 1 | `line_clamp` | 0 = unlimited |
| 55 | 1 | `text_decoration` | bitfield: 1 underline, 2 strikethrough |
| 56 | 1 | `overflow` | 0 `visible`, 1 `clip`, 2 `scroll` |
| 57 | 1 | `position` | 0 `flow`, 1 `absolute` (meaningful inside `stack`) |
| 58 | 1 | `z` | stacking order within the parent |
| 59 | 1 | `cursor` | 0 `default`, 1 `pointer`, 2 `text`, 3 `grab`, … |
| 60 | 1 | `transition` | 0 none, else `motion` scale index + 1: colours and opacity animate into this record (03 §5) |
| 61 | 1 | `animation` | 0 none, 1 `spin`: the node turns about its centre while on screen (03 §5) |
| 62 | 2 | — | reserved, MUST be zero |

A decoder MUST reject a record whose reserved bytes are non-zero, and MUST
reject any enumerated field whose value is outside the range defined above.
Rejecting unknown enum values rather than clamping them is what keeps two
implementations from silently diverging.

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

This set is closed. Adding a kind is a protocol version bump, because it means
shipping a new client. Everything users would call a widget — button, dialog,
table, date picker — is composed from these on the server; see
[`03-widgets.md`](03-widgets.md).

Kinds `0x02` `text` and `0x04` `icon` MUST have no children. Kind `0x0A`
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

Definition ops (`0x1x`) within a batch MUST precede any op that references what
they define. A decoder MAY rely on this and MUST reject a forward reference.

`MoveChild` removes the child at `from` and re-inserts it at `to`, where `to`
indexes the list *after* the removal. Moving `[a, b, c]` with `from=0, to=2`
gives `[b, c, a]`.

`MoveChild` is what makes keyed list reconciliation cheap: reordering a
thousand-row table is *n* moves, not a rebuild. A server SHOULD emit
`MoveChild` whenever the keys of a child list are a permutation of the previous
keys.

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
| `MAX_CHILDREN` | 65 535 per node |
| `MAX_PROPS` | 64 per node |
| `MAX_HANDLERS` | 16 per node |
| `MAX_OPS_PER_BATCH` | 65 535 |
| `MAX_INLINE_STR` | 4 KiB |
| `MAX_VALUE_DEPTH` | 4 |
| `MAX_VALUE_LIST` | 1 000 000 elements (a windowed list's `heights`, 04 §7.1; bounded in bytes by the frame) |

`MAX_ATOM_TOTAL_BYTES`, and the rule that an id is defined once and referenced
only after, are session state and belong to the tree layer, not the decoder.
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
same numbers. 46 rejection cases live in `crates/eui-proto/tests/reject.rs`.
