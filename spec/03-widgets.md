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
| `audio` | A sound, referenced by BLAKE3 hash in the `src` prop. Draws nothing | — | — | — |
| `video` | A moving picture, referenced by BLAKE3 hash in the `src` prop | — | — | — |

An inert kind MUST carry no text, props or handlers; a leaf kind MUST have
no children. Both are decoder errors ([`02-wire-format.md`](02-wire-format.md)
§4.1).

An `image`'s bytes are **PNG, JPEG or WebP**, told apart by their first bytes
and never by a name or a header the server sent. A client MAY be built without
the JPEG or the WebP decoder — they are counted in
[`10-budgets.md`](10-budgets.md) §1 — and then such an asset fails to decode,
with the format in the reason, and the node draws nothing. Anything else is a
decode failure in every build.

## 2. Painting

Everything paints as a rounded rectangle. For a node with style `s`:

1. If `s.bg` is not none, fill the border box with it, corners rounded by
   `s.radius`, at `s.opacity`. If `s.blur` is non-zero, the fill goes over
   the node's blurred **backdrop** (§2.1) rather than over what the target
   holds — so one node is both the frost and the tint — and the border box
   is filled even when `s.bg` *is* none: clear glass is still glass.
2. If any `s.border_width` is non-zero and `s.border_color` is not none,
   stroke the inside of the border box with it.
3. `text` paints its glyphs in `s.fg`, or the nearest ancestor's `fg`, or
   `text.default`; `image` paints its texture; `icon` paints the paths its
   `name` selects, in `fg`, as strokes of the same rounded capsule §1.1's
   polylines are made of, scaled to the largest square its content box holds
   and centred in it; `divider` paints a 1 px line in `bg` or
   `border.default`.
   A client MUST paint nothing for a `name` it does not know, and MUST NOT
   refuse the batch: an icon set grows without a protocol version, so an
   older client leaves a gap of the right size where a newer one draws.
   An `icon` with no width or height of its own takes a square from the
   font size in force, so an icon set beside a label needs no measurement
   from the server.
4. Children paint in order; `stack` children in ascending `z`. An `overlay`
   paints in the **top layer**: after every other node in the tree, overlays
   among themselves in tree order, clipped by the window and by no ancestor —
   so a dialog inside a card and a popover inside a scroller are both whole.
   Hit-testing asks the top layer first, and in the same order.
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

### 2.1 The backdrop

A node's **backdrop** is the frame as it stood when the first blurred node
of that frame was about to be painted — including that node's own shadow,
which is painted before its background. Every blurred node in a frame shares
that one backdrop, so two frosted panels that overlap both show what is
under the pair rather than one showing through the other. One snapshot is
taken however many nodes ask, and a frame in which none does is the single
pass it always was.

`s.blur` is the standard deviation of a Gaussian, in device-independent
pixels, as CSS `blur()` is. A client MAY approximate the Gaussian — as it
already approximates a shadow's (§2) — and a reference client does, by
convolving a reduced copy: the reduction is chosen so that the kernel's
width in samples stays about the same whatever radius was asked for, which
is what keeps a scrim over the whole window affordable. A radius beyond
about 85 device pixels MAY blur no further.

The backdrop is what the client itself painted. Nothing behind the window is
read, and nothing is reported back: a blur is no more a readback than a
shadow is (00 §"What EUI deliberately refuses").

## 5. Transitions

A style record's `transition` byte (02 §3, offset 60) names a `motion` scale
index plus one; `0` is none. When a node's style changes — by `SetStyle`, or
by a local handler's `set_style` — and the **new** record's `transition` is
non-zero, the client animates `bg`, `fg`, `border_color` and `opacity` from
the old record's resolved values to the new over that duration, along the
theme's easing curve, plus `blur` (§2.1) — nothing else, because layout
never runs per frame. The colours between are the renderer's: the quads a
transitioning node paints carry both ends and the clock, and the vertex
stage eases between them from the list's own age, so a frame owed to a
transition alone is the previous draw list drawn again, as a spin's is.
Only a transition of the blur is painted frame by frame, because the
backdrop is sized from it. A node that is mounted or replaced appears at once
unless it asks otherwise, which is `enter` below. The server is never
told; a transition is the client's rendering of a state change it already
knows about, and a client MAY skip it (reduced motion) without any
difference on the wire.

A record's `animation` byte of `2` is **enter**: a node grafted wearing it
— by `Mount`, `Replace` or `InsertChild` — arrives from nothing and
reaches its own record over its `transition` duration, or `motion.base`
when it names none, along the **decelerate** curve of 05 §2 rather than
the theme's standard one. From nothing means transparent and unblurred: `bg`,
`fg` and `border_color` fade up from no colour at all, `opacity` from `0`,
and `blur` from `0`, so a dialog's scrim darkens and widens its Gaussian
together and the page is handed out of play rather than shown already
gone.

An entrance dims **everything painted for the node**, as `spin` turns
everything painted for one: a panel arriving at a third of its opacity
shows its own text at a third too. `opacity` is otherwise a property of a
single node's own painting, and this is the one place it descends —
without it a dialog's words would be at full strength before the card
under them had arrived, which reads as text appearing on its own.

It has to be asked for, and that is the whole of why it is a separate
byte rather than an extension of `transition`: a button carrying a
transition for its hover would otherwise fade in every time a resync
rebuilt the tree, which is the flash the sentence above exists to
prevent. A client MAY skip an entrance for the same reason it may skip a
transition, and the server hears nothing either way.

A record's `animation` byte (02 §3, offset 61) is `0` or `1`, **spin**: a
node wearing it turns about its own centre, one revolution every 1.2 s,
for as long as it is on screen, and everything painted for it — its box,
its text, its canvas paths — turns with it. It is what a spinner is made
of: a `canvas` arc that spins. The client wakes for frames only while a
spinning node is painted, and those frames are cheap by construction: the
angle is the vertex stage's, from a clock the window hands it, so a frame
owed to a spin alone is the previous draw list drawn again — nothing is
laid out, nothing is painted, nothing is uploaded — at thirty a second,
which is 12° a frame. The same holds of a list with nothing moving in it
at all: it is the frame until something reaches the client, however often
a window asks to draw it. A client MAY hold a spin still (reduced motion).

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
  which is document order. A node that handles `key_down` or `key_up` takes
  focus on primary click too — the nearest one on the path, after any
  editable ancestor — so a grid, a canvas or a pattern editor is typed into
  after a click rather than after being found with `Tab`.
- A focused node draws a 2 px ring in `focus.ring`, outside its border box,
  when focus arrived from the keyboard; a client MAY suppress the ring after
  a pointer click.
- `Enter` in an `input` emits `submit`; `Escape` blurs. `Tab` and
  `Shift+Tab` move focus; the client, not the server, owns that order.
- A node with a `click` handler is activatable: it takes focus on `Tab` and
  `Enter` or `Space` emits `click` at its centre — unless that same node
  handles `key_down`, in which case both keys are reported and nothing is
  activated: it asked for the keyboard, and `Space` in a tracker starts the
  song. A node handling `key_down` or `key_up` takes focus on `Tab` too, so
  an editor is reachable without a pointer.
- The scrolling keys belong to the client while nothing that wants keys has
  focus — no editable node, and no node on the focused path handling
  `key_down` or `key_up`. An application that asked for the arrows gets them:
  a pattern editor, a grid, a game. Otherwise:
  `ArrowDown`/`ArrowUp` land the scroller on its next/previous row — a
  `list`'s rows, or the scroller's children. A plain 40 px step where there
  are none, where there is only **one** — a page whose content is a single
  column has one row top, at 0, and an arrow that honoured it would be `Home`
  — or where the nearest is more than a viewport away, so that an arrow never
  travels further than `PageUp` would. `PageDown`/`PageUp` move one viewport,
  `Home`/`End` the whole way. From rest the view eases in and out over `motion.slow`; a press
  that arrives while it is moving keeps the momentum and eases out to the
  new target over `motion.base`, like a wheel notch, so a held key is one
  glide rather than a series of departures. The scroller is the one under the
  pointer that can still move in that direction, else its ancestor that can,
  else the focused node's, else the first in document order that can. A nested
  `list` that does not overflow must not swallow the page's wheel. Nothing is
  reported but the `scroll` the landing produces.
- The pointer takes the shape of what it is over: the nearest ancestor's
  `cursor` style when one names a shape, else a text beam over an editable
  node, else a hand over anything with a `click` handler, else the arrow —
  and the arrow on a scrollbar.
- An `input` taller than its line — stretched by a row, or given a
  control height — centres its line vertically, caret and selection with
  it; a `textarea` starts at the top.
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

### 3.1 What a node may claim of the keyboard

Four props, read by the client, for the things a server cannot do because it
does not own them.

| Prop | Value | Means |
|---|---|---|
| `modal` | boolean | while this node is laid out, `Tab` order is **its subtree alone** |
| `autofocus` | boolean | focus starts here when the surface holding it arrives |
| `keys` | list of key names | the keys this node wants; it is sent no others |

- **`modal`.** A client MUST restrict its focus order to the subtree of the
  innermost laid-out node carrying `modal`. Innermost, so a dialog opened
  over a dialog traps inside the second. Without this, `Tab` walks out of an
  open dialog into the page behind it, and no server can prevent it.
- **`autofocus`.** On applying a batch that did not carry an explicit `Focus`
  op, a client SHOULD focus the first laid-out node carrying `autofocus` —
  but only when focus is not already where it belongs: inside the modal if
  there is one, or anywhere at all if there is not. A batch arriving while
  someone is tabbing through an open dialog MUST NOT pull them back to its
  first field.
- **`keys`.** A node holding a `key_down` or `key_up` handler and carrying
  `keys` is sent **only** the keys it names, and only those are withheld from
  the client's own meaning. So a `tab` may take `ArrowLeft` and `ArrowRight`
  and still be activated by `Enter`, and a dialog may listen for `Escape`
  without hearing every letter typed into the field inside it. A node with a
  handler and no `keys` prop hears everything, as before.

`Escape` follows from the third: it reaches a `key_down` handler on the path
that asked for it, and focus is left alone so the surface can put it back. If
nothing on the path asked, `Escape` drops focus, which is what it has always
done.

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

The reference catalogue ships with `examples/demo-app` as
`app/controllers/eui_builders.sl`, and `soli new <app> --eui` writes that
same file into a new application beside a component that uses it. Its
families: actions (button variants,
segmented control, menu, toolbar), input (field, checkbox, switch, select,
slider, date and time, file drop, form), structure (card, panel, sheet,
dialog, drawer, popover, tooltip, tabs, accordion, split pane, stepper),
navigation (navbar, sidebar, breadcrumb, pagination, tree), data (table,
grid, list item, chart, stat, code block, markdown), feedback (toast,
banner, progress, spinner, skeleton, empty state, avatar, badge, chip).

## 7. Sound

An `audio` node is a sound the application put in the tree. It lays out as
a zero-sized leaf and paints nothing: what it does is play. Its props say
what, and what it should be doing.

| Prop | Value | Meaning |
|---|---|---|
| `src` | asset | The sound's BLAKE3 hash, fetched like an image's bytes |
| `playing` | bool | Play, or hold where it is. Absent is `false` |
| `volume` | int `0..=100` | The application's own gain. Absent is `100` |
| `loop` | bool | Start again at the end instead of stopping |
| `position` | int ms | Where to play from. The client seeks **when this value changes**, not on every render, so a server that re-sends the same number does not stutter the sound |

Decoding runs where every decoder runs — the sandboxed worker of
[`08-security.md`](08-security.md) §10 — and the mixed frames are handed
to the window process, which owns the audio device as it owns the GPU. A
sound that fails to decode is dropped with a message on the client's
console; the node stays, silent.

Two events go back, and only to a node that holds a handler for them:

- `ended` when a sound reaches its end. Not sent for a looping source,
  which has no end.
- `time_update`, `[position_ms, duration_ms]`, while a sound plays. The
  client decides how often and MUST NOT send more than ten a second; four
  is what the reference client sends. It is a progress bar's input, not a
  clock: a server that needs the exact position asks for it at the moment
  it matters.

A client MUST bound what a session may play: the reference client holds at
most eight sources at once and refuses a ninth, and counts decoded audio
against the session's asset quota. Playing a sound needs **no capability**:
it is output, like drawing. The microphone is another matter and is a
capability already ([`01-transport.md`](01-transport.md) §2.1).

The viewer's own volume is above all of this and the application cannot
read it, set it, or tell that it is muted.

## 8. Moving pictures

A `video` node is a moving picture the application put in the tree. It
sizes itself to its frames unless a style says otherwise and paints the
frame the clock makes due — to a painter it is a picture, because that is
exactly what it is at any instant.

| Prop | Value | Meaning |
|---|---|---|
| `src` | asset | The picture's BLAKE3 hash, fetched like an image's bytes |
| `playing` | bool | Play, or hold on the frame it is on. Absent is `false` |
| `loop` | bool | Start again at the end instead of stopping |
| `position` | int ms | Where to play from. Seeks **when the value changes**, as `audio`'s does |

`ended` goes back when a picture reaches its end, to a node that holds a
handler for it; a looping picture has no end. `time_update`,
`[position_ms, duration_ms]`, goes back while it plays, under the same
rate limit as a sound's — it is what a progress bar is drawn from, and a
picture with no such handler reports nothing at all. The client owns the clock,
schedules exactly the moment the next frame is due — not a poll — and
uploads a frame only when the frame on screen must change. A paused
picture wakes nothing.

The formats are **GIF** and **animated WebP**, and the reason is the whole
argument of this project: both decode in pure Rust, both are patent-free,
and the decoder is the most attacked surface a browser has. H.264 needs a
patent licence; AV1 needs a large library, in C or in Rust, and a client
that promises a 12 MB binary does not link one casually. The node kind
says nothing about the codec, so a client that one day carries a real one
plays the same tree.

Several nodes may name the same picture — a feed of cards carrying one
animation. They share the decoded frames and the frame on screen; a
client is not required to give each node its own position, and the
reference client gives the first node in tree order the say. Decoding
runs in the sandboxed worker (08 §10) like every other decoder,
and the frames are bounded: 1920 × 1080 pixels a frame, 3 600 frames, 96
MB of decoded frames, and a frame that claims to last no time at all is
given 20 ms, because a picture must not be able to spin the client.

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

### 6.1 What a node may declare

The table above is the **default**. A node MAY carry accessibility semantics
in its props, and where it does they take precedence over the kind. A client
MUST ignore a prop it does not understand and MUST NOT refuse the batch for
one: the vocabulary grows without a protocol version, so props cost nothing
on the wire and an older client falls back to the mapping above.

| Prop | Value | Means |
|---|---|---|
| `role` | one of the names below | what this node is |
| `label` | string | the accessible name, **overriding** the text inside |
| `description` | string | read after the name |
| `checked` | `true`, `false`, `"mixed"` | a tick, including the third state |
| `expanded` | boolean | open or shut |
| `selected` | boolean | chosen within a set |
| `disabled` | boolean | present but unavailable |
| `read_only` | boolean | editable in principle, not now |
| `required` | boolean | must be filled in |
| `invalid` | boolean | filled in wrongly |
| `busy` | boolean | working |
| `modal` | boolean | owns the window while it is up |
| `value_now` / `value_min` / `value_max` | number | where a value sits, and its range |
| `pos_in_set` / `set_size` | integer ≥ 1 | place in a set, and the size of it **including what virtualisation left out** |
| `level` | integer ≥ 1 | depth, for a heading or a tree item |
| `orientation` | `"horizontal"`, `"vertical"` | which way a set runs |
| `live` | `"polite"`, `"assertive"` | how urgently a change should be read |

The role names: `button`, `link`, `check_box`, `radio`, `radio_group`,
`switch`, `tab`, `tab_list`, `tab_panel`, `menu`, `menu_item`, `menu_bar`,
`combo_box`, `list_box`, `option`, `slider`, `spin_button`, `progress`,
`dialog`, `alert_dialog`, `alert`, `status`, `tooltip`, `tree`, `tree_item`,
`toolbar`, `navigation`, `table`, `row`, `cell`, `grid`, `grid_cell`,
`column_header`, `heading`, `separator`, `group`, `label`, `image`.

Three rules follow from the table and are normative:

1. **Leafness is a property of the role, not of having a handler.** A node
   whose resolved role is `button`, `link`, `check_box`, `radio`, `switch`,
   `tab`, `menu_item`, `option`, `tree_item` or `column_header` is named by
   every text inside it and exposed without children. Any other role keeps
   its children — so a `tab_list`, a `menu` or a `grid` does not swallow
   what it holds, which the handler-based rule alone would have done.
2. **A disabled node keeps its role.** Disabling a control in a catalogue
   built on this protocol means removing its handlers, and without a
   declared role there would be no button left to infer. A node carrying
   `disabled: true` MUST keep its role and MUST NOT accept the *click* or
   *focus* action.
3. **A number that is present and zero is not an absence.** `value_now: 0`
   is a slider at the bottom of its range, not a slider without a value.
