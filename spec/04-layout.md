# 04 — Layout

Status: **normative** for what it defines; §9 lists what version 1 does not.
Implemented by `crates/eui-layout`.

Layout is the one expensive thing the client still does, because it depends on
facts the server cannot know: the viewport, the viewer's font scale and
density, and the shaped width of text in the client's fonts. It is therefore
specified as **one algorithm** rather than a family of behaviours, so that
every conforming client places every node at the same pixel.

There are no floats, no CSS positioning, no `calc()`, no percentage margins,
no `auto` margins, no writing modes, no `z-index` across containers. Each of
those is a way for two implementations to disagree, and none of them is
needed to build an application.

## 1. Terms

- **Container**: a node whose `display` is `row`, `column`, `stack` or `grid`.
- **Main axis**: horizontal for `row`, vertical for `column`. **Cross axis**:
  the other.
- **Available space**: for each axis, either a definite length in px or
  *indefinite* (unbounded).
- **Box model**: `width` and `height` size the **border box**. Padding and
  border lie inside it; margin lies outside and is never collapsed. The
  **content box** is the border box minus border minus padding.
- **Content size**: the size a node takes with indefinite available space on
  the axis in question.

All lengths are `f32` device-independent pixels. Results are not snapped to
whole pixels by layout; the renderer snaps at paint.

## 2. Resolving a `Dim`

Against an axis whose parent content-box length is `P`:

| `Dim` | Resolves to |
|---|---|
| `Auto` | *unresolved* — the algorithm decides |
| `Px(n)` | `n` |
| `Percent(n)` | `P × n / 10000` if `P` is definite, else *unresolved* |
| `Space(i)` | the resolved `space` scale entry `i` ([`05-theme.md`](05-theme.md) §5) |
| `Fr(n)` | *unresolved* outside a grid track |

After `width`/`height` resolve, `min_*`/`max_*` clamp the result, with
`min` winning over `max` when they conflict. Padding, margin and gap are
`space` indices and always resolve.

## 3. Sizing a node

`size(node, avail_w, avail_h) → (w, h, baseline)` is defined per kind.

**`text`** — shaped by the client's font engine at the resolved `text` scale
entry, in the `font_family` and `font_weight` roles. With definite `avail_w`
the run wraps at that width; `line_clamp > 0` truncates to that many lines
with an ellipsis. `w` is the widest line, `h` is `lines × line height`, and
`baseline` is the first line's ascent. A run of only whitespace has zero
width and one line height. `icon` and `image` size to their intrinsic size
unless `width`/`height` say otherwise; `divider` is `1 px` on its parent's
cross axis; `spacer` has zero content size.

**A container** — §4–§6, with `w`/`h` from `width`/`height` when resolved,
otherwise from content. A container's `baseline` is that of its first in-flow
child if it has one, else its border-box height.

**`sizer`**, **`slot`**, **`overlay`** behave as a `column` box. **`scroll`**
and **`list`** are §7.

`display: none` removes the node and its subtree from layout entirely; they
have no size and take no space. A `position: absolute` child is skipped by the
flow of any container other than `stack`.

## 4. Flow: `row` and `column`

This is CSS flexbox restricted to what applications use. Item order is child
order; there is no `order` property.

### 4.1 Hypothetical main size

For each in-flow child: `basis` if resolved; else `width`/`height` on the main
axis if resolved; else the child's content size on the main axis, sized with
the container's cross-axis available space. Clamp by `min`/`max`.

### 4.2 Lines

With `wrap: nowrap`, one line holds every child. Otherwise children are placed
in order onto the current line until adding the next child's hypothetical main
size plus the `gap` would exceed the container's inner main size, at which
point a new line begins. A child larger than the line stands alone on it.
`wrap-reverse` lays lines in reverse cross-axis order.

### 4.3 Distributing free space

Per line, `free = inner main − Σ hypothetical − gap × (n − 1)`.

If `free > 0`, each child with `grow > 0` receives `free × grow / Σ grow`.
If `free < 0`, each child with `shrink > 0` loses
`−free × (shrink × hypothetical) / Σ (shrink × hypothetical)`.

Then clamp each child by its `min`/`max`. A child whose main-axis `min` is
`auto` has an **automatic minimum**: along a row, its min-content width —
its size measured as if the available width were zero, so a paragraph's
longest word; down a column, its content height at the width on offer —
each capped by the child's specified size, unless it is a `scroll` or `list`
node or its `overflow` is `scroll`: such children shrink to zero. This is the
CSS `min-width: auto` rule — a column that overflows its box overflows, it does
not squash its children's text onto each other. A child whose clamp changed its size
is **frozen** at the clamped size, the free space is recomputed over the
remaining children, and the step repeats — at most eight times, after which
remaining children keep their last computed size. This is the CSS resolution
loop with a bound, so a pathological set of constraints costs eight passes
rather than a hang.

When the container's main size is indefinite, `free` is zero: children take
their hypothetical size and the container's main size is their sum plus gaps.

### 4.4 Justify

With any free space left after §4.3 (which can only happen when no child
grows), children are packed on the main axis per `justify`: `start`,
`center`, `end`, or spread with `between` (no space at the ends), `around`
(half a space at each end), `evenly` (equal spaces everywhere). With a single
child, `between` behaves as `start` and `around`/`evenly` as `center`.

### 4.5 Cross axis

A line's cross size is the largest cross size among its children, where a
child's cross size is `height`/`width` on that axis if resolved, else its
content cross size given its now-final main size. With one line and a
definite container cross size, the line's cross size is the container's inner
cross size.

Each child is then aligned within its line by `align_self`, or the container's
`align_items` when `align_self` is `auto`: `start`, `center`, `end`;
`stretch` sets the child's cross size to the line's if the child's cross
`Dim` was `Auto`; `baseline` aligns children's baselines to the largest
baseline on the line.

With multiple lines, lines are stacked on the cross axis with `gap` between
them; a container with indefinite cross size takes their sum.

## 5. `stack`

Every child, in-flow or absolute, is laid out against the container's content
box independently of its siblings. A child's size comes from its `width`/
`height` when resolved; otherwise an in-flow child under `stretch` (the
default) fills the container's definite inner size on that axis, and any other
child takes its content size given the container's inner size as available
space. It is then positioned by `align_self` on the vertical
axis and `justify` on the horizontal, with its margins as offsets from the
chosen edge. Children paint in ascending `z`, ties in child order.

A `stack` with indefinite size takes the largest **in-flow** child border box
plus that child's margins, per axis: an absolute child is placed on the stack,
never counted into it, and never stretched to it.

**Popovers.** An `overlay` child of a `stack` with `position: absolute` hangs
off the stack's first in-flow child — its *anchor* — instead of the stack's
own corner. It is measured against the **viewport** rather than against the
stack, on both axes, less its own margins: it floats, so what limits it is
the room the viewer has, and a `scroll` in it therefore stops at the window
edge and scrolls the rest. Its left edge starts on the anchor's; its top is
the anchor's bottom plus its own top margin. When that would put its bottom
outside the viewport and the anchor has more room above it than below, it
goes over the anchor instead: its bottom the anchor's top, less the same
margin. Either way the box is then clamped into the viewport on both axes,
so a panel taller or wider than the window still starts inside it. Nothing
is measured again: the panel and its subtree are moved.

**Following the pointer.** `position: pointer` is `absolute` in every respect
above — out of flow, sized to its content, measured against the viewport,
never counted into its stack — but the anchor gives it only its stack, not its
place. A client MUST put it **above the pointer** and **centred on it**: its
bottom the pointer's `y` less its own top margin, its centre the pointer's
`x`. When there is no room above — the top of the box would fall outside the
viewport — it goes under the pointer instead, its top the pointer's `y` plus
the same margin. It is then clamped into the viewport on both axes as any
other panel is.

A tooltip is the case this exists for, and it is a client's job for the same
reason a scrollbar is: a local chunk has no access to the pointer (see
[`07-bytecode.md`](07-bytecode.md) §1), and asking the server for a position
is a round trip per mouse sample. The client MUST keep such a panel under the
pointer as it moves, and SHOULD do so without laying out again — nothing about
the panel changes but its origin. A client that does not know where the
pointer is — it has left the window, or the input is not a pointer at all —
leaves the panel where it was.

## 6. `grid`

Version 1 supports one shape: `N` equal columns. `N` is the container's
`columns` prop, an integer `≥ 1`, defaulting to `1`. Children are placed in
order, row-major. Each column is `(inner width − gap × (N − 1)) / N` wide; each
row is as tall as its tallest child; rows are separated by `gap`. A child is
sized as in §4.5 with `stretch` on both axes when its `Dim`s are `Auto`.

## 7. Scrolling and virtualisation

A `scroll` node lays out its content as a `column` with **indefinite**
available height (and indefinite width when `overflow` is `scroll` on that
axis), then clips to its own border box. Its scroll offset is clamped to
`[0, content − viewport]` per axis after every layout.

A scroll in flight — a wheel notch or a key press easing to its offset
(03 §5's motion) — is laid out **once, at the offset it lands on**; the
frames between draw that layout with the content slid, in the vertex
stage, from where it was to where it was put, along the motion's curve
from the list's own clock, so a frame owed to a glide alone is the
previous draw list drawn again. The reference client tells its layout how
far the content stands from where it was put meanwhile, so a hit mid-glide
finds what is drawn under the point rather than what will be. A `scroll`
event is emitted when it lands, as ever. A virtualised list holds the rows
at both ends of the travel for the glide's one layout (§7.1), which is why
a glide over more than two viewports of such a list moves its offset frame
by frame instead.

A `list` node is a `scroll` whose children are laid out at their content size
in a column, with one addition: when the list carries an `item_height` prop
(integer px), a child outside the visible range plus one viewport of margin on
either side is **not measured** — it is assigned `item_height` and skipped. A
child carrying its own `item_height` prop takes that height instead, so a
feed of cards of a few known heights virtualises like a table: the client
sums the heights once, one addition per row, and measures only the rows in
view. Once, not once per frame: the sum is a function of the rows'
heights and the list's gap, and a scroll changes neither — a client is
expected to keep it until something under the list changes, since
rebuilding it is the whole cost of a scrolled frame otherwise (the
reference client marks a scrolled node `dirty::SCROLL` rather than
`dirty::SELF` for this).
This is the only place the algorithm is allowed to use an estimate, and it is
what makes a ten-thousand-row table cost what a fifty-row one costs.

### 7.1 Windowed lists

A `list` carrying a `count` prop (integer, the number of rows) has rows the
tree does not hold. Row `i` is `heights[i]` px tall when the list carries a
`heights` prop (a list of integers, one per row; a missing entry is
`item_height`), else `item_height`. A child of such a list carries a `row`
prop naming the row it is; a child without one is not laid out. Rows with
no child are laid out as empty boxes of their height — the scroll extent,
the scrollbar and the row tops are exactly those of the full list — and
those in view are painted as placeholders: a block in `surface.sunken`,
inset by `space.2` and rounded `md`, so a scroll that outruns the server
shows where the rows are rather than nothing.

Mid-glide the window covers both ends of the travel — the rows in view
where the glide began and where it lands, each with its margins — so the
one layout a glide takes has rows to slide past, and the placeholders are
painted for every row the view passes over.

When the range of rows that intersects the viewport plus two viewports of
margin on each side changes, and the scroll has been still for a moment
(the reference client waits 120 ms — a request a frame would be a server
render a frame) — after a scroll lands, a mount, a resize, a change of
`count` — the list emits `window` (spec 06) with `[first, last]`, inclusive
row indices, if it holds a handler of that kind. While the view is still
moving, it also asks the moment the rows within half a viewport of what it
shows are not all among those it last asked for — a drag that has outrun
its rows would otherwise show placeholders until it stopped — at most once
every 50 ms in the reference client, and the settle that follows then finds
nothing new to ask. A server that
answers with those rows as children, and lets the others go, holds one
window of a feed in memory, not the feed: forty thousand posts cost the
client forty thousand integers and the server a few dozen cards.

## 8. Invalidation

A conforming client MAY relayout from the root on every change; it SHOULD
relayout only subtrees whose [`dirty`](../crates/eui-tree) bits are set and
whose size could affect an ancestor. Either way the result MUST equal a full
relayout. Text measurements SHOULD be cached by `(text, font, available
width)`, as shaping is the single most expensive operation in the pipeline.

## 9. Not in version 1

Stated so no one goes looking:

- Grid tracks other than `N` equal columns; spanning; named areas.
- `order`, `auto` margins, percentage margins or padding, `aspect-ratio`.
- Vertical writing modes and right-to-left layout. (Text shaping is
  bidirectional; layout mirroring is not.)
- Sticky positioning.
- Cross-container stacking: `z` orders siblings within one `stack` only.
- Sub-pixel snapping rules for the renderer, which belong in
  [`03-widgets.md`](03-widgets.md) once written.
