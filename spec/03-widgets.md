# 03 — Primitives and the catalogue

Status: **normative** for §1–§3 (what the client implements); §4 is the
catalogue contract for servers.

## 1. The seventeen primitives

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
| `scene` | A 3D picture, drawn by a program the server names (§1.2) | — | — | yes |

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
4. If `s.text_decoration` is non-zero, `text`, `input` and `textarea` draw a
   rule in the same colour the glyphs took, **one per line of the shaped
   run** and no wider than the glyphs on that line — so a decoration on text
   that wrapped underlines each line to its own end rather than drawing one
   bar the width of the box. Bit 0 puts it below the baseline, bit 1 through
   the middle of the x-height, and both together draw both. It is one device
   pixel at 1× and grows with the scale, exactly as the caret does. The
   offsets are the client's: the shaper hands back a baseline and an advance,
   not an underline position, and a server that wanted a rule somewhere
   precise would be asking for a layout it cannot see.
5. Children paint in order; `stack` children in ascending `z`. An `overlay`
   paints in the **top layer**: after every other node in the tree, overlays
   among themselves in tree order, clipped by the window and by no ancestor —
   so a dialog inside a card and a popover inside a scroller are both whole.
   Hit-testing asks the top layer first, and in the same order.

   **A press outside an open overlay dismisses it**, and the client does
   that, because the client owns the hand. An overlay carrying a `blur`
   handler hears `blur` when a press lands outside it; one carrying none
   hears nothing and is not dismissible, so an application opts in per panel
   and says for itself what closing means. Nothing is added to the wire:
   `blur` already means "this stopped being the thing being used".

   **Outside means outside the overlay's parent**, not outside the overlay.
   A panel and the control that raised it are siblings under one box — which
   is what a `stack` with an absolute overlay in it is — so a press on the
   control is a press on the widget. Were the rule the overlay alone, a
   select would shut on the press and its own click would open it again, and
   no select could ever be closed by clicking it.
6. `scroll` and `list` clip their children to their border box, and one
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

### 5.1 Leaving

A record's `animation` bit `4` is **exit**, the mirror of `enter`: a node
released while wearing it — by `RemoveChild`, `Replace`, `Mount`, or a
parent's release carrying it down — **keeps its painting** for its
`transition` duration, or `motion.base` when it names none, along the
**accelerate** curve of 05 §2 rather than the theme's standard one. Then it
is let go.

`animation` is a bit set and not an enumeration for exactly this reason. A
node has to say how it will leave while it is still there to say it: the op
that removes a node is the only op there is, and a `SetStyle` aimed at one on
its way out would be a style change on something already gone. So a page
names its arrival and its departure in the one record it is grafted with, and
`enter | exit` is the ordinary spelling of a page.

What is kept is the **painting and not the tree**. The node is gone: it
cannot be named by an op, cannot be reached by an event, cannot hold focus,
cannot be a `wake`r or a locator, is not laid out again, is not hit-tested —
a click during a page's departure lands on whatever is arriving underneath,
which is what a person expects — and is not in the accessibility tree. None
of that is a rule a client has to enforce at each door; it is what is left
when only the quads are kept.

Three bounds hold it down. A client MUST keep at most **one** released
painting per session, and a second supersedes the first, which is let go at
once — the same shape as one contact at a time (06 §5) and one drag at a
time (06 §6), and what makes the memory a constant rather than a function of
how fast a person can tap. A client MUST let a painting go rather than draw
it wrong when what it was made against has moved: the glyph atlas it sampled,
the size of the frame, the viewer's palette. And a subtree that painted an
overlay (§1) painted part of itself in the top layer, outside the span it
otherwise occupies; a client MAY refuse to keep such a page at all, and then
it simply goes, which is the same licence the paragraph below gives.

The server is never told — not that a departure began, and not that it
ended. There is no op, no event and no acknowledgement, exactly as for the
transition above, and a client MAY skip the whole thing (reduced motion)
with no difference on the wire.

### 5.2 Which way

A record's `motion_kind` byte (02 §3, offset 63) says which way its entrance
arrives and its exit leaves:

| value | Name | The geometry |
|---:|---|---|
| 0 | `fade` | no movement: the entrance as §5 has always described it |
| 1 | `leading` | from, or to, beyond the leading edge of the parent's content box |
| 2 | `trailing` | likewise the trailing edge |
| 3 | `top` | |
| 4 | `bottom` | |
| 5 | `scale` | from, or to, 92 % about the node's own centre |
| 6 | `paired` | it flies between its own box and that of the node carrying the same `key` on the other side of the change (§5.3) |

A direction and never a duration. The duration is `transition`, and keeping
the two apart is what stops a page from carrying a timing of its own — 05 §2
having already said that motion specified per node is motion nobody gets
right twice.

**A node that is leaving is not told where to go.** Its direction is the
*mirror* of the direction the node arriving beside it came from, under
`leading ↔ trailing`, `top ↔ bottom`, and `fade`, `scale` and `paired` each
their own. Two nodes are a pair when one is released and one is grafted under
the same parent with no paint between them; a release with no partner uses
its own record's direction. So a push and a pop are the same sentence read in
the two directions, a server interns two records for a page rather than four,
and the question "which way is back" is never asked on the wire at all.

The one leaving travels **a third** of the distance the one arriving does,
and **fades to nothing while it goes**. Both are prose here and not fields.
A third, because the page underneath is not being replaced, it is being
uncovered, and something sliding out as fast as the thing covering it reads
as two slides rather than as a stack with a depth to it. Fading, because a
page leaving at full opacity is a second page competing with the one
arriving — and on the `accelerate` curve §5 already gives an exit it stays
nearly solid for the first half of the move and only then lets go, which
reads as a departure rather than as a dissolve. A client that moved both the
same distance, or that slid a page out without fading it, would be
conforming and wrong.

As with an entrance, the movement descends to **everything painted for the
node**. That is the second place anything descends, and the justification
already given for opacity carries over without change: a page whose header
slid while its rows did not would read as a tear rather than as a page.

### 5.3 Pairing

`motion_kind` `6` `paired`, on a node carrying a `key`, on **both** sides of a
change: the node leaving and the node arriving fly between their two boxes
rather than each going the way its page goes.

Nothing is laid out per frame to do it. The arriving node was laid out for
this frame, and the leaving node's box is frozen by §5.1 — both rectangles are
known before the first frame of the movement, so the pair is one
interpolation resolved once, which is the same bargain §5 strikes for a
colour and 04 §7 strikes for a scroll.

A `paired` node whose partner is missing is **not an error and MUST NOT
refuse the batch**: it falls back to the motion of the page it is on. A
panel is built and torn down as it opens, so a name that does not resolve is
the ordinary case and not a broken one.

A client MUST bound the number of pairs it resolves in one change, and a
client that cannot resolve a pair paints the node where the layout put it —
the same answer it gives when it runs out of room for a scroll in flight.

A record's `animation` bit `1` is **spin**: a
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

### 1.2 `scene`

A `scene` is the one node whose picture the client does not compute from the
tree. The server names a WGSL module and a mesh, both as assets; the client
renders them **into a target of its own**, with a depth buffer of its own, and
draws that target as a single textured quad in the node's box.

Everything a node ordinarily gets, a scene gets from that quad: the corner
radius of its style, its opacity, the scissor of any `scroll` it sits in, and
the transform of any page transition it is on (§5). Its module knows about
none of them, and is not trusted to apply any of them.

| Prop | Value | Meaning |
|---|---|---|
| `shader` | asset | The WGSL module, in its `"EUIS"` container (11 §1). Absent is the client's own |
| `mesh` | asset | The geometry, in its `"EUIM"` container. Absent is the client's own |
| `uniforms` | list of ≤ 8 numbers | The author's half of the uniform block (11 §3), zero-padded |
| `playing` | bool | The clock runs. Absent is `false` — the same word `audio` and `video` use, and the same meaning |
| `fps` | int | Frames a second while it animates, 1 to 60. A server may slow a scene down, never speed the client up |
| `msaa` | int | `4` asks for four samples, `1` for one. Absent is `4`: a scene is a picture of edges, and the author who has to ask for them is the author who ships without them |

A `scene` has **no intrinsic size**: the server sizes it, as it does a
`spacer`. It names two assets, and neither of them is a width.

A client that cannot multisample this format MUST draw the scene once rather
than refuse it. A slightly harder silhouette is better than no picture, and
the alternative is a validation error on a machine the author never had.

**The `scene` capability guards the module, not the kind.** A scene that
names a `shader` MUST NOT be drawn unless the capability was granted (08 §3):
without it the client does not fetch the module, and the node paints its own
background like any node with no content. A scene that names none draws — it
is the client's own program over the client's own shape, so there is nothing
third-party to consent to, and its `mesh` is fetched either way because
vertices are data and not a program.

An application that uses a `scene` at all SHOULD still *request* the
capability, whether or not it names a module: the request is what raises the
manifest's `protocol_min` (01 §2.1), and an older client cannot decode the
kind however the scene is drawn. Requesting and granting are separate acts,
and only the second is about running somebody's code.

**No readback, and no picking.** A scene's target is not readable: 08 §8's
"no canvas readback" is kept by construction here, not by rule. It follows
that a press on a scene reports a position in the node's box and **never** an
object, a triangle or a depth — picking is readback under another name, and a
server that sent the geometry can do it for itself.

A scene that is not `playing` is a still picture: it is drawn once and wakes
nothing, and the zero-wakeup line of 10 §1 holds with one on screen. A scene
that is `playing` is a window that asked to be woken, exactly as a `wake`
prop is, and costs what it asked for. What a client MUST NOT do is wake the
part of itself that reads the server's bytes: an animating scene is the same
draw list redrawn with a later clock.

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
- A node carrying `drag` (§3.4) takes focus on `Tab`, so a thing that can be
  moved can be reached without a pointer. **`Space`** picks it up; the arrows
  then move it along the container's axis, `Space` puts it down, `Escape` puts
  it back. While the grab is live the client owns those keys, and **only** those
  and only on that node — not grabbed, nothing is claimed at all and the arrows
  are still the scroller's.

  `Space` is claimed only on a node with no `click` handler. A row that is both
  activatable and movable keeps `Enter`/`Space` for the thing it stands for and
  reaches the grab through a `drag_handle` child, which is its own focus stop.
  A `keys` prop (§3.1) naming `" "`, an arrow or `Escape` takes it back: the
  server wins when it asks explicitly. The grab reports what a pointer drag
  reports and nothing else — `drag_start`, `drag_over`, `drop`
  ([`06-events.md`](06-events.md) §6) — so a server needs no second path for the
  keyboard.
- The pointer takes the shape of what it is over: the nearest ancestor's
  `cursor` style when one names a shape, else a text beam over an editable
  node, else `grab` over a node resolving `drag` (§3.4), else a hand over
  anything with a `click` handler, else the arrow — and the arrow on a
  scrollbar. While a drag is live the shape is `grabbing`, over everything.
- An `input` taller than its line — stretched by a row, or given a
  control height — centres its line vertically, caret and selection with
  it; a `textarea` starts at the top. `text_align` (`start`, `center`,
  `end`) shifts the run inside the content box; a run that overflows
  still starts at the left and scrolls. `justify` is painted as `start`.
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
  caret in view. The caret blinks at 530 ms, up first: anything that moves
  it — a keystroke, a click, an arrow — starts the period again, so it is
  never absent under a hand that is typing. After ten seconds with nothing
  moving it the blink stops and the caret stays up, which is what keeps
  10 §1's idle budget true of a window left open on a form. The selection
  does not blink. A paste is the person's act on their own clipboard;
  `clipboard.read` (08 §7) governs reads the application would initiate.

- An `input` carrying **`secret: true`** is a password field. The client
  paints one mark per character (`•`), measures that string, and copies
  nothing from a selection — `Ctrl+C` is consumed and the clipboard is left
  alone; `Ctrl+X` deletes and still copies nothing. The value on the wire,
  and in the node's text, is what was typed. A `textarea` MUST ignore the
  prop: a secret that wraps is not a password. An assistive technology is
  handed a password field whose value is empty, never the text.

### 3.1 What a node may claim of the keyboard

Four props, read by the client, for the things a server cannot do because it
does not own them. §3.2 to §3.4 are the rest of that family.

| Prop | Value | Means |
|---|---|---|
| `modal` | boolean | while this node is laid out, `Tab` order is **its subtree alone** |
| `autofocus` | boolean | focus starts here when the surface holding it arrives |
| `keys` | list of key names | the keys this node wants; it is sent no others |
| `typing` | boolean | this node takes typed text, though it is not a field |

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
- **`typing`.** A node carrying `typing` takes typed text while it has focus,
  and a client MUST treat it as it treats a field **for the one purpose of
  making typing possible**: where a client would welcome an input method for
  an `input`, it welcomes one for this, and where raising a soft keyboard is
  what that amounts to — a phone, where there is no other keyboard — it
  raises it.

  Nothing else about the node changes. The client owns no caret here, no
  selection, and no buffer; it inserts nothing and reports no `text_input`.
  What the person types arrives as the `key_down` the node already asked for,
  which is the whole point: an editor, a pattern grid or a game wants the
  gutter, the highlighting and the selection to be one thing, and that thing
  is the application's (§3). Such a view is built from a box and a handler,
  and on a desktop it works because a keyboard is already there to be typed
  on.

  It is **opt-in and MUST NOT be inferred** from a node merely holding a
  `key_down` handler. A page that listens for one shortcut at its root would
  otherwise raise a phone's keyboard on any focus at all, over the view it
  was reading, with no way to decline; and a client that guessed would leave
  the application no way to say which of its boxes is the one being typed
  into.

  A client with no soft keyboard and no input method to welcome does nothing
  for this prop, which is correct: there, the keyboard was never in the way.

- **`keys`.** A node holding a `key_down` or `key_up` handler and carrying
  `keys` is sent **only** the keys it names, and only those are withheld from
  the client's own meaning. So a `tab` may take `ArrowLeft` and `ArrowRight`
  and still be activated by `Enter`, and a dialog may listen for `Escape`
  without hearing every letter typed into the field inside it. A node with a
  handler and no `keys` prop hears everything — but only **for itself**: a
  dialog that merely listens must not swallow the `Enter` that presses a
  button inside it.

  **Inside an editable node**, "withheld from the client's own meaning" needs
  saying in three parts, because the obvious reading makes a field nothing can
  be typed into. The rule is that **a client keeps a key it has a use for and
  yields one it does not**:

  1. **Editing the text** — `Backspace`, `Delete`, the arrows, `Home`/`End`,
     `Ctrl+A`/`C`/`X`, `Enter` in a `textarea`, and every printable character —
     is **never** withheld. No prop may take it away. A key the client used
     this way is **not** reported, because a client that both acted on a key
     and passed it on leaves the application unable to tell that apart from a
     key it could not use, which is the same as not reporting it at all.
  2. **Acting on the field as a whole** — `Enter` in an `input` — is withheld
     on a claim, and what is withheld is the **`submit`**, never the `change`.
     A server that claimed `Enter` to take a highlighted suggestion instead of
     the text still needs to know what was typed, and needs it *before* the
     key that acts on it. This is the same rule as a claim on a button, which
     withholds the **click** the key stands for: the key is reported, and the
     client's *interpretation* of it is not applied.
  3. **A key in a position where it does nothing** — `Backspace` with nothing
     before the caret, `ArrowLeft` at 0, `Ctrl+C` with no selection — is never
     the client's, claim or no claim, and is reported if `keys` asked for it.
     There was no meaning to withhold. This is what lets a list of tokens above
     a field take `Backspace` as "remove the last" without the field losing the
     ability to delete a character.

  A **printable character is never withheld and cannot be**: text does not
  reach the client as a key at all, but as input with no key name on it, so
  there is nothing to hold back. A separator typed into a field is in the
  field before any handler could have been told, and a server that wants one
  takes it out of the value instead.

`Escape` follows from the third: it reaches a `key_down` handler on the path
that asked for it, and focus is left alone so the surface can put it back. If
nothing on the path asked, `Escape` drops focus, which is what it has always
done. The editing rules above do not touch it — a panel that closes on
`Escape` hears it with the caret still in the field below.

`Tab` is never any of this. It moves focus, always, and a node cannot ask for
it: the order is the client's (§3) and a surface that could take `Tab` could
strand someone in it.

### 3.2 What a node may claim of the filesystem

Two more props, for the two things a server cannot do at all: reach a file on
the person's machine, and put one there.

| Prop | Value | Means |
|---|---|---|
| `pick` | `Str accept`, or `List[Str accept, Int flags, Int max]` | activating this node opens the platform's open dialog |
| `drop` | as `pick` | a file let go over this node arrives as if it had been picked |
| `save` | `Str name` | activating this node opens the platform's save dialog |

`accept` is a comma-separated list of extensions without dots (`"csv,pdf"`),
empty for any file. `flags` bit 0 allows more than one file; **bit 1 asks
for the camera** and **bit 2 for a recording**, rather than for something
the person already has. `max` is
the largest one file may be, in bytes, capped by
[`10-budgets.md`](10-budgets.md) §5 and defaulting to it. `name` is the name
to suggest, and a client MUST reduce it to its last path segment.

Bits 1 and 2 change which capability the sheet needs and nothing else: a
picture taken now, or a recording just made, is a file like any other —
reported by the same `file_pick` and carried by the same `Upload` frames.
Bit 1 needs **`camera`**, bit 2 needs **`microphone`**, and a node asking
for either MUST NOT be opened on `fs.pick` alone: taking a photograph,
making a recording and reading a folder are three different powers, and
none implies another. Bits 1 and 2 together are a contradiction; a client
MUST resolve it as the camera, so that two clients resolve it alike.

The recording is a **finished** one, and that is the whole reason it fits
here: it has an end, so it is a file, and files already have a transport
([`01-transport.md`](01-transport.md) §6). Listening to a microphone as it
runs is a stream, has no transport in this protocol, and is not this
([`08-security.md`](08-security.md) §9.1).

A client with no capture on its platform opens nothing, exactly as it would
for a capability that was not granted. Bit 0 with either is one file:
neither phone photographs or records several things in one sheet, and a
`multiple` a client cannot honour is a promise to the server it would then
break.

A client opens a dialog when **all** of this holds, and never otherwise:

1. the person **activated** the node — a primary click, or `Enter`/`Space`
   with focus on it. A tree that merely arrives opens nothing, and neither
   does a batch, a timer, or a local handler;
2. the node carries the prop **and** a **server** handler for the event that
   answers it — `file_pick` or `file_save`
   ([`06-events.md`](06-events.md) §1). A local chunk cannot be given a file
   or asked for one: both ends of a transfer are the server's;
3. the matching capability — `fs.pick` or `fs.save` — was granted
   ([`01-transport.md`](01-transport.md) §2.1). Without it there is no
   dialog and no diagnostic the application can see.

A node may carry a `click` handler as well; the click is dispatched as
usual, so an application may show that something is happening. At most one
dialog per node is open at a time.

**What is picked.** Each file chosen gives one `file_pick` event carrying an
upload id, the file's **name** — never its path — and its size; the bytes
follow as `Upload` frames against that id
([`01-transport.md`](01-transport.md) §6). A file past `max` is announced
and then aborted, so the application can say why rather than leave the
person watching nothing happen. A dismissed dialog is not an event: nothing
happened.

**What is dropped.** `drop` is the same prop as `pick` and the same
arrival: a file let go over the node gives one `file_pick` carrying an
upload id, the file's **name** — never its path — and its size, and the
bytes follow as `Upload` frames. Nothing downstream can tell a drop from a
pick, and nothing should: they are one act with two gestures, and an
application that handles the dialog handles the drop for free.

Three of the prop's parts do not survive the change of gesture. `accept`
filters what a dialog will offer and filters nothing at all here — no
platform lets a window refuse a file before it is let go — so a node that
cares must look at the name it is given and say why. `flags` bit 0 is moot:
a hand lets go of as many files as it is holding, and each is its own
`file_pick`. Bits 1 and 2 are meaningless — a drop is never a camera — and
a client MUST ignore them rather than treat the node as a capture. `max` is
enforced exactly as it is for a pick: the event arrives, an `Abort` follows
on that id, and the application says why.

A client opens nothing and reads nothing unless **all** of this holds:

1. a file was let go **over** the node — the position is the pointer's, and
   a platform that reports a drop without one reports it where the pointer
   last was;
2. the node carries `drop` **and** a **server** handler for `file_pick`, the
   same pair a dialog needs and for the same reason;
3. the **`fs.pick`** capability was granted. A drop is reading a file the
   person already has, which is what that grant is; it needs no second one.
   Without it there is no transfer and no diagnostic the application can
   see, exactly as for a dialog.

**Showing that it would.** A node carrying `drop` may also carry a handler
for `file_drag` ([`06-events.md`](06-events.md) §1), which reports `true`
when a file comes over it and `false` when the file leaves, the drag ends,
or it goes anywhere else. It is sent **only when the node under the file
changes**, not once a frame: a box that flickers under a held file is worse
than one that does not light at all. It is a report and nothing more —
nothing is read, and a client that never sends it is still conformant.

**What is saved.** The person choosing a place gives one `file_save` event
carrying the name they chose. That event *is* the request: the bytes are
owed, and arrive as `Blob` frames addressed to the node. Nothing is written
until the first chunk arrives, so a save the server never answers leaves
nothing behind, and an aborted one leaves nothing either — half an export is
worse than none, because it looks like a whole one until it is opened.

### 3.3 What a node may claim of a reader

One prop, for a thing that is neither a file nor a fact about the machine:
a tag somebody holds against it.

| Prop | Value | Means |
|---|---|---|
| `nfc` | `Str prompt` | activating this node starts a scan for one tag |

`prompt` is what to tell the person the scan is for. A platform that raises
a sheet of its own shows it; one that listens without a sheet has nowhere
to put it, and the application should say it in the tree as well.

The three conditions of §3.2 hold **word for word**: the person activated
the node, the node carries the prop *and* a **server** handler for
`nfc_tag`, and `nfc` was granted. A tree that merely arrives scans nothing,
and neither does a batch, a timer, or a local handler. This is not a
platform's rule reflected into the protocol — one of the two phones would
happily listen for as long as its screen is on — it is the protocol's rule
imposed on both, because a reader nobody started is the whole of what makes
one dangerous.

**What is read.** One tag gives one `nfc_tag` event
([`06-events.md`](06-events.md) §1) and ends the scan: a reader that
delivers twice is answered once. A scan that reads nothing before it ends
is not an event — the person held their phone up and thought better of it,
and an application learns that only if it is told, which it is not.

A client MUST NOT ship a general NDEF model. Records are reduced to
`(kind, payload)` pairs, where `kind` is `text`, `uri`, `mime:…` or `raw`,
and `payload` is UTF-8 for the first three and lower-case hex for the last.
Parsing a format a stranger wrote, in a process that on both phones has no
worker to be confined to ([`08-security.md`](08-security.md) §10), is
exactly the surface this protocol spends its effort avoiding.

This is why a save costs a round trip rather than riding on an asset: the
bytes are generated when the person asks for them, they are nobody else's,
and no one who is not on this session can fetch them.

### 3.4 What a node may claim of the pointer

Five props and a family, for the two gestures a server cannot resolve
because it does not own the hand: picking something up and putting it
somewhere else, and running a value along a line.

| Prop | Value | Means |
|---|---|---|
| `drag` | boolean, or `Str group` | this node can be picked up; the string names its group |
| `accepts` | boolean, `Str group`, or `List[Str]` | this node takes items of those groups |
| `drag_handle` | boolean | a press here grabs at once, without the slop or the hold |
| `drag_axis` | `"x"`, `"y"` or `"both"` | which way a slot moves, and which arrows move it |
| `drag_only` | boolean | this node hears `pointer_move` only while a button is down |
| `track` | `"x"` or `"y"` | this node is a value on a line, running that way |

**There is no "reorder me".** Reordering is the case where the item's own
parent is the target, so a container that `accepts` what its children `drag`
reorders itself and takes the same thing from elsewhere, and the server tells
the two apart because it has both ends and a model. One prop, two behaviours,
and no way for them to disagree.

**The prop says what a node *is*; the handler says who *hears*.** A row is
draggable and a list is what hears the drop, and those are two different nodes
resolved by two different walks: the props by the walk in
[`06-events.md`](06-events.md) §6, the handler by §2's nearest-handler rule as
always. Matching is the client's affordance and not authorisation — it decides
which containers light up and which shape the pointer takes, and §4 there still
requires the server to re-derive everything it is told.

`drag: true` is the empty group and matches `accepts: true` alone. A named
group matches a container naming it, or naming it among several.

**A draggable node MUST carry a `key`.** It is how the client holds on to what
is in the hand: a move between containers is a removal and an insertion
([`02-wire-format.md`](02-wire-format.md) §5), so the node's id changes under
the gesture and only the key survives it. Keyed children are also what make the
server's reorder a `MoveChild` rather than a rebuild, so this costs nothing that
was not already owed.

**`drag_only`** is the older of the five and belongs here rather than in §3.1,
though it is not the keyboard's. A split bar's container must hear
`pointer_move` while it is being dragged and must not hear it the rest of the
time, and the obvious answer — give it the handler only while the drag runs —
loses events: one already in flight names a handler the server has since
removed, and §4 of [`06-events.md`](06-events.md) ends the session for it. The
prop lets the handler stay and the moves stop.

The pointer's shape follows from `drag` without any style: **`grab` over a node
resolving `drag`**, and `grabbing` over everything while a drag is live. Both
already exist in the `cursor` scale, and an application that wants a different
shape still says so in the style, which wins as it does for everything else.

**The track.** A slider is the widget a server is told a hundred times what it
needed to be told five times. The hand moves continuously along a line; the
value moves in steps. So a node carrying `track` says which way its line runs,
and the client resolves the whole of the hand along it — which handle a press
takes, where it goes, which step the pointer is in — and reports only what a
person would call a change: a `change` event ([`06-events.md`](06-events.md)
§1) carrying the new value, and nothing at all for the samples in between. It
is §6 of [`06-events.md`](06-events.md)'s argument one widget further:
**the client owns the hand, the server owns the value.**

Four props say what the line means, read from the node carrying `track`:

| Prop | Value | Means |
|---|---|---|
| `track_min` | integer | the value at the start of the line; default `0` |
| `track_max` | integer | the value at its end; default `100` |
| `track_step` | integer above zero | the quantum the value lands on; default `1` |
| `track_value` | integer, or `List` of two | where the handle is, or both handles |

and one names the parts the client places, on any **descendant** of the track:

| `track_part` | Means |
|---|---|
| `"groove"` | the line itself, stretched along the whole track |
| `"fill"` | the part of the line the value covers |
| `"thumb"` | a handle: one, or two in document order |

A track with two `thumb` parts is a range: `track_value` then carries two
numbers, and the `fill` runs between the handles rather than from the start.
A track with no parts at all still resolves a value and draws nothing.

**The geometry is the client's and is fixed here**, because a widget whose
thumb sits in a different place on two clients is not one widget. Along the
axis the handles' centres travel the track's extent **less one thumb's**, so a
handle at either end is inside the line rather than half outside it.
`track_min` is at the left of an `"x"` track and at the **bottom** of a `"y"`
one. The value under a pointer is the value at the centre of the handle it
holds, quantised to the nearest whole `track_step` from `track_min` and
clamped to the ends; the same arithmetic run backwards places the handle, so
**the value read at a handle is the value that put it there**. The server
sends `track_value` and nothing about geometry — no fill width, no baked-in
track width — because the only width that was ever true is the one the client
laid out.

A press on a handle takes that handle and holds it at the offset it was
grabbed at, so nothing jumps to centre itself under the finger. A press
anywhere else takes the handle nearer the value pressed, moves it there at
once, and reports it. Two handles on the same value are told apart by which
side the press is on, and a press on neither side takes the second — so a pair
closed at the minimum can still be opened. **A gesture keeps the handle its
press chose**, however far it travels: a handle that changed identity under a
finger that never left it would be a different thing in the hand. A handle
dragged into the other stops against it; they may meet and they never swap.

A press that lands on a track does not also arm a drag
([`06-events.md`](06-events.md) §6.1): the track is the more specific claim,
and a card with a slider on it is moved by its handle or by its margin. A
`track_part` node's own descendants are **not** carried with it — a part draws
its own box, and a handle with a label inside it is outside what this version
places locally.

**No handler appears or disappears for the gesture.** A track declares
`change` once and keeps it, and declares no `pointer_move` at all — which is
what `drag_only` above exists to work around, and which this makes unnecessary
for the case `drag_only` was invented for. The warning above applies in full: a
track that gained a handler on its press would lose the events already in
flight when it lost it again.

**A track is reached without a pointer.** Each `thumb` is focusable (§3), so
`Tab` steps through a range's two handles and the arrows always mean the one
the ring is on — there is no modifier to say which, and no state remembering
which moved last. The arrows along the axis move the focused handle one
`track_step`, the page keys ten, `Home` and `End` go to the ends, and none of
those keys is reported: only the `change` they make
([`06-events.md`](06-events.md) §3). On a touch screen a contact landing on a
track takes the stroke, so the view beneath it does not scroll (§5 there) —
that is the one thing a `pointer_move` handler used to be declared for.

**Accessibility follows without being said twice.** `value_now`, `value_min`
and `value_max` (§6.1) fall back to `track_value`, `track_min` and `track_max`,
and while a hand is on the track the value exposed is the one the client is
drawing rather than the one the last batch carried. Five props on the track and
one per part, against the `min`, `max` and `width` they replace, leave
`MAX_PROPS` ([`02-wire-format.md`](02-wire-format.md) §6) further from its
ceiling than before.

### 3.5 What a node may claim of the browser

One prop, for the one thing the client will start another program to do.

| Prop | Value | Means |
|---|---|---|
| `open` | `Str` an `https:` address | activating this node hands it to the platform's opener |

The three conditions of §3.2, and for the same reason: the node carries the
prop, the `net.open` capability was granted, and **the person activated it**.
There is no op that opens an address and no event that reports one, so a tree
that merely arrives opens nothing, and an application learns nothing by
trying. A node carrying `open` is a focus stop, so the keyboard and an
assistive technology can follow a link the pointer can.

**The scheme is the whole of the danger, and the check is a comparison.** A
platform opener is a URI *dispatcher*, not a browser: handed `file:`, `smb:`
or a scheme some other application registered for itself, it runs that
instead, with a string the server chose. So exactly one scheme is accepted —
`https://`, lower case, literally — and an address carrying credentials,
control characters or whitespace is refused before it reaches the platform.
A client MUST tell the person which host it is about to open, and MUST NOT
report back whether it opened, when, or whether it failed: an answer is a
probe for whether there is a browser here at all, with a clock beside it
([`08-security.md`](08-security.md) §8).

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
`app/controllers/eui_builders.sl` and its `_forms`, `_charts`, `_feed` and
`_markdown` companions, and `soli new <app> --eui` writes those same files
into a new application beside a component that uses them. Its
families: actions (button variants,
split button, segmented control, toggle group, menu, context menu, command palette,
popconfirm, toolbar), input (field, password field, currency field, checkbox, switch, select,
combobox, multi select, slider, range slider, rating, date and time, file drop, form),
structure (card, panel, sheet,
dialog, drawer, popover, tooltip, tabs, accordion, split pane, stepper, shortcut sheet),
navigation (navbar, sidebar, breadcrumb, pagination, tree), data (table,
expandable row, tree table, grid, multi-select list, list item, chart, stat, code block,
diff, filter builder), markdown (document, document rows for a windowed
list, block editor and its model), feedback (toast,
banner, progress, spinner, skeleton, empty state, timeline, avatar, avatar group, badge, chip).

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

Three events go back, and only to a node that holds a handler for them:

- `ended` when a sound reaches its end. Not sent for a looping source,
  which has no end.
- `time_update`, `[position_ms, duration_ms]`, while a sound plays. The
  client decides how often and MUST NOT send more than ten a second; four
  is what the reference client sends. It is a progress bar's input, not a
  clock: a server that needs the exact position asks for it at the moment
  it matters.
- `level`, `[peak_left, peak_right]`, each `0..=100`: how loud the sound
  has been **since the last `level`**, so a transient between two of them
  is smoothed rather than missed. It is a peak-reading instrument and a
  meter is what it is for.

  `level` rides the clock of `time_update` and MUST NOT bring a finer one:
  a client that sends one sends both on the same tick, and the reference
  client sends four a second. It is sent when the value changes, so a
  silent sound costs nothing, and a sound that stops sends one last zero —
  a meter that stays lit over silence is worse than no meter.

  **The peak is measured before the viewer's own volume**, on the source's
  own samples with only the application's `volume` applied. This is
  normative and it is the whole reason the event can exist: a peak taken
  after the master gain could be divided by the volume the application
  asked for to recover the viewer's setting, and a zero would say they had
  muted. What a server learns from a `level` is a property of the bytes it
  sent, at the gain it chose, and nothing about the machine.

  A client that settled on a protocol below 3 has never heard of this
  event, and a handler naming it would be a decode error that ends the
  session. A server MUST therefore leave the handler out of the tree it
  sends such a client rather than send it and lose the session: the
  application runs, and its meter does not move. This is the difference
  between an event added later and a node kind added later — a kind is a
  capability the manifest can declare before anything renders, an event is
  a key in a view that has not run yet, so the floor is enforced at encode
  time and not at the handshake.

  A `level` describes sound that is **about to be heard**, not sound
  already heard: a client keeps a buffer queued ahead of the device — a
  fifth of a second in the reference client — so the meter leads the
  loudspeaker by that much. Closing that gap would mean indexing peaks by
  playback position, which is the finer clock
  [`08-security.md`](08-security.md) §8 refuses, so it stays open and is
  written down here instead.

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

A node carrying `drag` (§3.4) also offers **move-before** and **move-after**,
which do in one step what §3's grab does in three, and stand at the same
ceiling — a keyboard user reaches the same place by the longer road.
They are offered only where there is somewhere to go, and a platform that has
no vocabulary for them exposes them as the custom actions it does have: the
ones that exist — grabbed, dropeffect — were deprecated by the standard that
invented them, and a moved thing announces itself better than a held one
describes itself. The announcement is the server's, through a node with
`live` (§6.1), because it is prose and prose is content. The client's share is
that focus stays on the thing it moved, that `pos_in_set` and `set_size` stay
true, and that the thing is brought into view.

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
| `active_descendant` | a node's **key** | which node in a set this one is *on*, while keeping the keyboard itself |

The role names: `button`, `link`, `check_box`, `radio`, `radio_group`,
`switch`, `tab`, `tab_list`, `tab_panel`, `menu`, `menu_item`, `menu_bar`,
`combo_box`, `list_box`, `option`, `slider`, `spin_button`, `progress`,
`dialog`, `alert_dialog`, `alert`, `status`, `tooltip`, `tree`, `tree_item`,
`toolbar`, `navigation`, `table`, `row`, `cell`, `grid`, `grid_cell`,
`column_header`, `heading`, `separator`, `password`, `group`, `label`, `image`.

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
4. **`active_descendant` names a node, and a name pointing at nothing is
   dropped.** The value is a *key*, because a key is the only handle on a
   node a server has; the client resolves it and exposes the node it found.
   The node named need not be a descendant and usually is not — a combo box
   puts its options in an overlay beside the field, so that the field can
   keep the keyboard while the arrows walk them, and rule 1's "an editable
   node gets no children" does not stand in the way. A key that names
   nothing laid out MUST be dropped and MUST NOT refuse the batch: the
   panel is built and torn down as it opens and shuts, so a stale name is
   the ordinary case.

`active_descendant` is the one relation this vocabulary has, and it is here
because the alternative is worse. A `live` node can be told *3 of 8,
Consignment* as prose, but prose is the server's and structure is the
client's — §6 already draws that line for the drag announcement — and a
highlighted option is structure: it has a role, a position in its set, and
a selected state that an assistive technology reads in the reader's own
language. `controls` and the rest of the ARIA relation set are deliberately
left out; each would be another way for a name to dangle, and this one
earns its place by being the only way to express the widget at all.
