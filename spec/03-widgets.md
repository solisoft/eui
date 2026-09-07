# 03 — Primitives and the catalogue

Status: **normative** for §1–§3 (what the client implements); §4 is the
catalogue contract for servers.

## 1. The fourteen primitives

The protocol can express exactly these node kinds. Every widget a person
would name — button, dialog, table, date picker — is composed from them on
the server. Adding a kind is a protocol version bump, because it means
shipping a new client, and that price is deliberate.

| kind | Semantics | Children | Text | Focusable |
|---:|---|:-:|:-:|:-:|
| `box` | A styled rectangle that arranges children by `display` | yes | — | — |
| `text` | A run of text, wrapped to its box, clamped by `line_clamp` | — | yes | — |
| `image` | A raster asset, referenced by BLAKE3 hash in the `src` prop | — | — | — |
| `icon` | A vector glyph from the client's icon set, named by the `name` prop | — | — | — |
| `input` | A single-line editable field; `text` is its value | — | yes | yes |
| `textarea` | A multi-line editable field | — | yes | yes |
| `scroll` | A clipping viewport with scroll offsets | yes | — | — |
| `list` | A `scroll` whose rows are virtualised by `item_height` | yes | — | — |
| `canvas` | A retained path list in the `paths` prop | — | — | — |
| `spacer` | Flexible empty space; inert | — | — | — |
| `divider` | A hairline rule; inert | — | — | — |
| `overlay` | A layer above the normal flow, painted after its siblings | yes | — | — |
| `slot` | A named insertion point; lays out as a `column` | yes | — | — |
| `sizer` | An invisible box that only imposes constraints | yes | — | — |

An inert kind MUST carry no text, props or handlers; a leaf kind MUST have
no children. Both are decoder errors ([`02-wire-format.md`](02-wire-format.md)
§4.1).

## 2. Painting

Everything paints as a rounded rectangle. For a node with style `s`:

1. If `s.bg` is not none, fill the border box with it, corners rounded by
   `s.radius`, at `s.opacity`.
2. If any `s.border_width` is non-zero and `s.border_color` is not none,
   stroke the inside of the border box with it.
3. `text` paints its glyphs in `s.fg`, or the nearest ancestor's `fg`, or
   `text.default`; `image` paints its texture; `icon` paints its glyph in
   `fg`; `divider` paints a 1 px line in `bg` or `border.default`.
4. Children paint in order; `stack` children in ascending `z`; `overlay`
   after every non-overlay sibling of the same parent.
5. `scroll` and `list` clip their children to their border box, and one
   whose content is taller than its box wears a vertical scrollbar along
   its right edge: a thumb in `text.muted` at 45 % opacity, as long as
   view ÷ content of the track and never under 24 px, painted after the
   children. The strip is the client's: pressing the thumb drags it,
   pressing the track pages by the view's height, and neither reaches the
   application except as the `scroll` event the resulting offset produces.
   While the pointer is on the strip, or the thumb is being dragged, the
   thumb fills the strip in `text.default` at 70 %.

Rectangles are snapped to device pixels before painting; glyph positions are
snapped horizontally to whole device pixels and vertically to the line's
baseline. Anti-aliasing is by signed distance to the edge, so the same
pipeline draws boxes, hairlines and glyphs.

A non-zero `s.shadow` paints, before the background, a black rounded
rectangle offset by the scale's `y`, grown by its `blur` on every side, with
coverage falling from opaque at the border box's edge to nothing at the
grown edge, at the scale's opacity. `canvas` paths (§1.1) are part of
version 1.

## 5. Transitions

A style record's `transition` byte (02 §3, offset 60) names a `motion` scale
index plus one; `0` is none. When a node's style changes — by `SetStyle`, or
by a local handler's `set_style` — and the **new** record's `transition` is
non-zero, the client animates `bg`, `fg`, `border_color` and `opacity` from
the old record's resolved values to the new over that duration, along the
theme's easing curve. Nothing else animates: layout never runs per frame,
and a node that is mounted or replaced appears at once. The server is never
told; a transition is the client's rendering of a state change it already
knows about, and a client MAY skip it (reduced motion) without any
difference on the wire.

A record's `animation` byte (02 §3, offset 61) is `0` or `1`, **spin**: a
node wearing it turns about its own centre, one revolution every 1.2 s,
for as long as it is on screen, and everything painted for it — its box,
its text, its canvas paths — turns with it. It is what a spinner is made
of: a `canvas` arc that spins. The client wakes for frames only while a
spinning node is painted; a client MAY hold it still (reduced motion).

### 1.1 `canvas` paths

A `canvas` carries its drawing in the `paths` prop: a `List` of paths, each
a `List` beginning with an `Int` kind and a colour, followed by numbers.
Coordinates are logical px from the node's content box; the drawing is
clipped to the border box. The colour is a `Color` (a server resolves role
names and `#RRGGBB[AA]` before encoding); a client MUST also accept an `Int`
role id and, for hand-written trees, a `Str` role name or hex literal.

| kind | Path | Meaning |
|---:|---|---|
| `0` | `[0, colour, width, x0, y0, x1, y1, …]` | A polyline stroked `width` px wide, round caps and joins |
| `1` | `[1, colour, x, y, w, h, radius]` | A filled rectangle |
| `2` | `[2, colour, base_y, x0, y0, x1, y1, …]` | The area between a polyline and the horizontal `base_y` |
| `3` | `[3, colour, cx, cy, r]` | A filled circle |
| `4` | `[4, colour, width, cx, cy, r, a0, a1]` | An arc of radius `r` from angle `a0` to `a1`, radians, `0` along +x, increasing clockwise on screen |

Paths paint in order. A path with an unknown kind, a colour that does not
resolve, or too few numbers is skipped, never an error: a chart with a bad
series still shows its grid. The reference client draws every kind with its
rounded-rectangle pipeline — a segment is a rotated capsule, an area a strip
per device column, an arc a fan of chords at most 6° apart — so a canvas adds
no shader, no tessellator and no allocation beyond its quads.

## 3. Focus and keyboard

- `input` and `textarea` take focus on primary click and on `Tab` order,
  which is document order.
- A focused node draws a 2 px ring in `focus.ring`, outside its border box,
  when focus arrived from the keyboard; a client MAY suppress the ring after
  a pointer click.
- `Enter` in an `input` emits `submit`; `Escape` blurs. `Tab` and
  `Shift+Tab` move focus; the client, not the server, owns that order.
- A node with a `click` handler is activatable: it takes focus on `Tab` and
  `Enter` or `Space` emits `click` at its centre.
- The scrolling keys belong to the client while no editable node has focus:
  `ArrowDown`/`ArrowUp` land the scroller on its next/previous row — a
  `list`'s rows, or the scroller's children; a plain 40 px step where there
  are none — `PageDown`/`PageUp` move one viewport, `Home`/`End` the whole
  way. From rest the view eases in and out over `motion.slow`; a press
  that arrives while it is moving keeps the momentum and eases out to the
  new target over `motion.base`, like a wheel notch, so a held key is one
  glide rather than a series of departures. The scroller is the one under the pointer, else the
  focused node's, else the first in document order. Nothing is reported but
  the `scroll` the landing produces.
- The pointer takes the shape of what it is over: the nearest ancestor's
  `cursor` style when one names a shape, else a text beam over an editable
  node, else a hand over anything with a `click` handler, else the arrow —
  and the arrow on a scrollbar.
- In an editable node the client owns the caret and the selection. A click
  places the caret at the nearest glyph edge and a drag selects;
  `ArrowLeft`/`ArrowRight` move by character, by word with `Ctrl` (`⌘` on
  macOS), extending the selection with `Shift`; `Home`/`End` reach the line's
  ends, the text's with `Ctrl`; `Backspace`/`Delete` remove the selection or
  one character; `Ctrl+A` selects all; `Ctrl+C`/`Ctrl+X` put the selection
  on the system clipboard; `Ctrl+V` inserts it; `Enter` in a `textarea`
  inserts a line. Typing, a paste and a committed composition all replace
  the selection, and each reaches the application as one `text_input`. The
  caret is drawn in the text colour one device pixel wide, the selection in
  `accent.base` at 30 % opacity, and a field scrolls its text to keep the
  caret in view. A paste is the person's act on their own clipboard;
  `clipboard.read` (08 §7) governs reads the application would initiate.

## 4. The catalogue contract

The catalogue is a server-side library; the client knows nothing of it. A
catalogue implementation MUST:

- compose only from §1;
- express every colour as a role, with literals only for marks and data
  series;
- give every activatable widget a `click` handler and every editable one a
  `change` handler, so that §3 applies;
- give every repeated child in a list a `key`;
- put the identity a handler needs in the node's `props`, never in the
  event name.

The reference catalogue ships with `examples/counter-app` as
`app/controllers/eui_builders.sl`. Its families: actions (button variants,
segmented control, menu, toolbar), input (field, checkbox, switch, select,
slider, date and time, file drop, form), structure (card, panel, sheet,
dialog, drawer, popover, tooltip, tabs, accordion, split pane, stepper),
navigation (navbar, sidebar, breadcrumb, pagination, tree), data (table,
grid, list item, chart, stat, code block, markdown), feedback (toast,
banner, progress, spinner, skeleton, empty state, avatar, badge, chip).

## 6. The accessibility tree

A client SHOULD expose its tree to the platform's assistive technology —
AT-SPI, UIA, AX — with this mapping, and MUST NOT tell the server whether
one is listening:

| Node | Exposed as |
|---|---|
| any node with a `click` handler | a button, named by every text inside it, a leaf |
| `input` / `textarea` | a text field whose value is the node's text |
| `text` | a label |
| `image` / `icon` | an image |
| `scroll` / `list` | a scrolling container; virtualised rows are absent, as they are from layout |
| anything else | a group |

Bounds are the layout rectangles. Focus is §3's. An assistive technology's
*focus* action focuses as `Tab` would, and its *click* action presses as
`Enter` would: nothing it can do exceeds what a keyboard user can do, so
the server needs no new validation and learns nothing new.
