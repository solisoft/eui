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

A `stack` with indefinite size takes the largest child border box plus that
child's margins, per axis.

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

A `list` node is a `scroll` whose children are laid out at their content size
in a column, with one addition: when the list carries an `item_height` prop
(integer px), a child outside the visible range plus one viewport of margin on
either side is **not measured** — it is assigned `item_height` and skipped. A
child carrying its own `item_height` prop takes that height instead, so a
feed of cards of a few known heights virtualises like a table: the client
sums the heights once per layout, one addition per row, and measures only
the rows in view.
This is the only place the algorithm is allowed to use an estimate, and it is
what makes a ten-thousand-row table cost what a fifty-row one costs.

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
