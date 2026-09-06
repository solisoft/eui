# Wire format

The normative text is `spec/02-wire-format.md`. This page is the same material
with the reasoning left in.

Everything below is implemented by `eui-proto` and pinned by byte-level test
vectors, so a second implementation written from the spec alone can check itself
against the same numbers.

## Primitives

| Notation | Encoding |
|---|---|
| `u8`, `u16`, `u32` | fixed width, little-endian |
| `varint` | LEB128 unsigned |
| `svarint` | LEB128 over zigzag |
| `bytes` | length-prefixed |
| `str` | `bytes`, required to be valid UTF-8 |

A varint must be **minimally encoded**. A decoder rejects a multi-byte encoding
whose final byte carries no bits, because two spellings of one number is one
spelling too many for anything that gets hashed, signed, or compared. That is
the classic route to a signature bypass, and it costs one comparison to close.

## Session tables

A session owns four append-only tables, all with dense ids assigned by the
server starting at 1. Id 0 always means "none" and can never be defined.

| Table | Defined by | Ceiling |
|---|---|---|
| Atoms | `DefAtom` | 65 535 |
| Styles | `DefStyle` | 65 535 |
| Literal colours | `DefColor` | 4 095 |
| Bytecode chunks | `DefChunk` | 4 095 |

Redefining an id is an error. Referencing an undefined id is an error. Tables
clear only on `Mount`.

## The style record

A style record is exactly **64 bytes**, fixed layout, no optional fields. It
holds *computed* style: every value is a literal number or an index into a theme
scale, and nothing in it needs resolving against a parent.

```
 0  display        8  basis  (3B)   37  bg            (2B)
 1  wrap          11  width  (3B)   39  fg            (2B)
 2  justify       14  height (3B)   41  border_color  (2B)
 3  align_items   17  min_width     43  border_width  (4B)
 4  align_self    20  min_height    47  radius
 5  grow          23  max_width     48  shadow
 6  shrink        26  max_height    49  opacity
 7  gap           29  padding (4B)  50  font_family
                  33  margin  (4B)  51  font_size
                                    52  font_weight
                                    53  text_align
                                    54  line_clamp
                                    55  text_decoration
                                    56  overflow
                                    57  position
                                    58  z
                                    59  cursor
                                    60  reserved (4B, must be zero)
```

Enumerated fields **reject** unknown values rather than clamping them. Clamping
is how two implementations quietly disagree about a layout for a year. The
reserved tail must be zero for the same reason: accepting garbage there today
would make tomorrow's field unusable, because some deployed server would already
be putting something else in it.

## Nodes

```
Node := kind:u8  flags:u8  id:varint  style:varint
        [ key      ]  -- flags & 0x01
        [ text     ]  -- flags & 0x02
        [ props    ]  -- flags & 0x04
        [ handlers ]  -- flags & 0x08
        child_count:varint
        child_count × Node
```

There are fourteen kinds and the set is closed: `box`, `text`, `image`, `icon`,
`input`, `textarea`, `scroll`, `list`, `canvas`, `spacer`, `divider`, `overlay`,
`slot`, `sizer`. Adding one is a protocol version bump, because it means
shipping a new client. See [the widget catalogue](/docs/widgets) for how a
button gets built out of these.

A subtree decodes into **pre-order arrays**, not a tree of boxes. That is not a
micro-optimisation: it is what lets the decoder run iteratively with an explicit
work stack, so a ten-thousand-deep hostile tree costs a bounds check instead of
the call stack.

## Patches

```
DefAtom  DefStyle  DefColor  DefChunk       definitions
Mount(subtree)                              replace everything
Replace(node, subtree)
SetStyle(node, style)
SetText(node, text)
SetProp(node, prop, value)
InsertChild(parent, index, subtree)
RemoveChild(parent, index, count)
MoveChild(parent, from, to)
SetHandler(node, event, handler)
ClearHandler(node, event)
Focus(node)   ScrollTo(node, x, y)
```

`MoveChild` is the one worth pointing at. Soli's current LiveView diff works on
*lines of an HTML string*, so re-sorting a table has no cheap spelling — the
lines all change. Here, a keyed child list that is a permutation of the previous
one becomes *n* moves: reversing fifty rows costs 201 bytes rather than 4 619.

## Limits

Every one of these is checked *before* the memory it bounds is allocated. A
hostile server can be annoying; it cannot make the client exhaust itself.

| Limit | Value |
|---|---|
| Frame | 8 MiB |
| Tree depth | 256 |
| Nodes | 250 000 |
| Atoms | 65 535, 64 KiB each, 8 MiB total |
| Styles | 65 535 |
| Children per node | 65 535 |
| Props per node | 64 |
| Handlers per node | 16 |
| Ops per batch | 65 535 |
| Inline string | 4 KiB |
| Value nesting | 4 |

## No error recovery

A malformed frame ends the session. There is no quirks mode, no partial
application, and no "ignore what you don't understand" — that last one is how
one implementation's frame becomes another's smuggling channel. If a batch fails
to apply, the client discards its tree, asks for a resync, and rebuilds.

This is deliberately unforgiving. A patch stream that has drifted is a bug on
one side or an attack from the other, and both are better served by a clean
rebuild than by a heuristic.
