# Tailwind classes

> `tw("...")` writes an EUI style in Tailwind's classes. It is plain Soli in
> the catalogue — `examples/demo-app/app/controllers/eui_builders_tw.sl`, and
> the copy `soli new <app> --eui` writes — and it adds nothing to the wire:
> every class becomes a style key the encoder already takes, colours become the
> theme's roles, and spacing becomes an index the client scales by the viewer's
> density.

```soli
row({"tw": "items-center gap-4 w-full px-4 py-4 border-b border-gray-200 hover:bg-gray-50"}, [
  initial_avatar("L", "accent.base", 40),
  column({"tw": "flex-col gap-1 grow min-w-0"}, [
    text("Leslie Alexander", tw_style("text-sm font-semibold text-gray-900")),
    text("leslie@meridian.test", tw_style("text-xs text-gray-500 truncate"))
  ])
])
```

The gallery's Catalogue section opens with a card written in nothing else,
and the demo application's `helpdesk` component is a whole Tailwind UI app
shell written the same way — sidebar, top bar, tables, a thread, forms —
in `examples/demo-app/app/controllers/helpdesk_controller.sl`, with
breakpoints, `divide-y`, `space-y-*`, `mx-auto` and the half steps where
Tailwind UI writes them.

## What it answers

`tw(classes, width = nil)` returns eight hashes:

| Key | What it is |
|---|---|
| `s` | The resting style — an ordinary style hash |
| `hover`, `press`, `focus`, `disabled` | What `hover:`, `active:`, `focus:` and `disabled:` classes change, as **deltas** over `s` |
| `props` | What Tailwind says in a class and EUI says in a prop: `grid-cols-N` is `{"columns": N}` |
| `gaps`, `divide` | What a class says about the children rather than the box — `space-y-4`, `divide-y`. `tw()` settles `gaps` into `s` when the classes say which way the box runs and raises otherwise, and raises on `divide`, having no children to lay it on; a node settles both itself (below). So from `tw()` both are always empty |

`width` is the viewport's width, which the view is given on `connect` and on
every resize and `tw()` is not. It is only needed for `sm:` … `2xl:`
([Breakpoints](#breakpoints)); `tw(classes)` with one argument is the whole
of the rest.

The deltas are the shape a `TONES` entry has, which is the point: a `tw()`
result is a tone, and everything that takes a tone takes one.

| Where | What it does with the classes |
|---|---|
| `node(kind, {"tw": "...", "vw": width}, children)` — so `column`, `row`, `stack` | The resting classes become the style, under any key written beside `"tw"` (a builder's own `display` still wins). `space-*` and `gap-x-*` are settled against that final direction, and `divide-*` is laid onto the children. States become local handlers on `self`: `pointer_enter`/`leave`/`down`/`up` for `hover:` and `active:`, `focus` and `blur` for `focus:`. No state, no handlers. `"vw"` is the viewport width, for breakpoints; it is not a style key and does not reach the style |
| `control({"tw": "...", ...})` | The resting classes go over the tone's resting colours and under `shape`; `hover:` and `active:` over the tone's deltas; `disabled:` over the disabled look |
| `stateful(base, "hover:bg-gray-50 ...", on)` | A class string, or a `tw()` result, as the tone |
| `tw_style(classes, disabled = false, width = nil)` | Only the resting style, for a `text` node, whose style is a plain hash |
| `text(content, tw_style("uppercase ..."))` | The string transformed on the server; see [Type](#type) |

`classes` is a string, or a list of strings when a view builds them
conditionally. Later classes win over earlier ones, whatever the stylesheet
order would have been in a browser; `border border-t-2` is a two-pixel top and
one pixel elsewhere.

A state needs no key: a local chunk names the node it runs on as `self`
(07 §1). A text colour set in `hover:` on a box reaches a text child only if
the child sets no `fg` of its own — `fg` is the one key that inherits.

Each distinct string is parsed once per process and copied out after that
(the first 1 024 strings; a string built from data costs a parse each time).
Copied, because builders write into the style they are given.

## The classes

### Layout

| Classes | EUI |
|---|---|
| `flex`, `inline-flex`, `flex-row` · `flex-col` · `grid` · `hidden` | `display: row` · `column` · `grid` · `none` |
| `grid-cols-N` | the `columns` prop |
| `flex-wrap`, `flex-nowrap`, `flex-wrap-reverse` | `wrap` |
| `items-start` … `items-baseline` | `align` |
| `justify-start` … `justify-evenly` | `justify` |
| `self-auto` … `self-baseline` | `self` |
| `grow`, `grow-0`, `shrink`, `shrink-0` | `grow`, `shrink` |
| `flex-1` · `flex-auto` · `flex-initial` · `flex-none` | `grow 1 shrink 1 basis 0` · `basis auto` · `grow 0 shrink 1` · `grow 0 shrink 0` |
| `block` | `display: column` — a block's children stack down it at its width, which is a column stretching them; see *Approximations* |
| `absolute`, `z-N` | `position: absolute`, `z` — inside a `stack` |
| `relative`, `static` | `position: flow`. In flow is what both mean for the box itself; the box an `absolute` child is placed against is the `stack` it sits in (04 §5), so the element a Tailwind page marks `relative` is written `stack()` |
| `isolate` | nothing: `z` orders siblings only, and no box's children paint across another's (04: no `z-index` across containers) |
| `overflow-hidden`, `overflow-clip` · `overflow-visible` | `overflow: clip` · `visible` |

### Spacing

`p`, `px`, `py`, `pt`, `pr`, `pb`, `pl`, the same for `m`, and `gap-N`. The
value is an index into the space scale (05 §2), which the client multiplies by
the viewer's density:

| Tailwind step | 0 | 0.5 | 1 | 1.5 | 2 | 2.5 | 3 | 3.5 | 4 | 5 | 6 | 8 | 10 | 12 | 16 | 20 | 24 | 32 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| px | 0 | 2 | 4 | 6 | 8 | 10 | 12 | 14 | 16 | 20 | 24 | 32 | 40 | 48 | 64 | 80 | 96 | 128 |
| index | 0 | 1 | 2 | 13 | 3 | 14 | 4 | 15 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 16 | 12 | 17 |

The pixels are the same on both sides. `1.5`, `2.5`, `3.5`, `20` and `32`
are the indices protocol version 6 appended, 13–17 — out of order because an
index cannot move once a client has it (05 §2). They were chosen by
measurement: over ten first-draft screens they were the refusals that came
back, `1.5` 34 times, `2.5` 14, `32` 10, `3.5` 8 and `20` twice (`40`, once,
was left out). A session whose client is older
than 6 is sent the nearest older step, ties down, by the encoder rather than
by `tw()` — `py-1.5` reaches it as `py-1` — so a view is written once.

A step the scale still does not have — `7`, `40` — is an error naming the two
nearest steps and listing the rest, not a guess: `py-7` says *the nearest are
6 (24 px) and 8 (32 px)*.

| Classes | EUI |
|---|---|
| `mx-auto` | `self: center`. An auto margin on the cross axis centres the node on it, and `mx-auto` is written for a box in a column — a centred container in block flow |
| `my-auto` | `self: center`, for a node in a row |
| `ml-auto`, `mr-auto`, `mt-auto`, `mb-auto`, `m-auto` | refused: along the parent's line an auto margin pushes the siblings apart, which is a `spacer()` before or after the node, or `justify-between` on the parent |

### Between children

Tailwind writes these on the parent and a child selector carries them to the
children. EUI has no selector, so `node()` does the carrying — it has the
children in hand — and a bare `tw()`, which does not, settles what it can and
refuses the rest.

| Classes | EUI |
|---|---|
| `space-x-N` on a row, `space-y-N` on a column | `gap` N. The margin on every child but the first *is* the gap, exactly, on a line that does not wrap |
| `gap-x-N` on a row, `gap-y-N` on a column | `gap` N, the gap along the line; `gap-x` overrides `gap-N` on its axis, as in the stylesheet |
| `gap-x-N gap-y-N`, the same N | `gap` N — on a wrapping row, a wrapping column or a grid, where both axes are spaced |
| `divide-y`, `divide-x`, `divide-y-2` … | every child but the first takes a top (`y`) or left (`x`) border of that width and none on the opposite side. Each axis on its own, so `divide-y md:divide-y-0 md:divide-x` turns with the box |
| `divide-gray-200` and every border colour | that child's `border_color`. With none, the child keeps its own, or has `border.subtle` — the gray-200 Tailwind's preflight gives every border — since an EUI border with no colour is not drawn |

Which way a box runs is decided last: `row({"tw": "space-x-4"}, …)` is a row
because `row()` says so. A bare `tw("space-y-4")` does not know, and raises
until the classes say `flex` or `flex-col`.

A divided child is copied, not written — the same hash may be a child
somewhere else — and keeps its key. A local handler on it that restyles the
node itself (`self.style = @hover`, which is what `tw()` and `stateful()`
write) has the rule laid onto every style it can switch to, so the rule does
not vanish on hover. A `nil` child is left where it is and not counted.

Refused, each with the reason: `space-y` on a row (a margin across the line,
not along it); `space-*` on a wrapping line (its next lines start indented and
unspaced) or on a grid; `space-*` beside a non-zero gap (the browser draws the
sum); `gap-x` and `gap-y` that differ where both axes are spaced; a cross-axis
gap on a line that does not wrap, unless it is `0` or the same (it would
space nothing, and writing it says the author expected it to show); anything
here under a state; `space-x-reverse`, `divide-y-reverse`; `divide-dashed`
and the other border styles.

### Sizes

| Classes | EUI |
|---|---|
| `w-N`, `h-N`, `size-N`, `basis-N`, `min-w-N`, `max-h-N` … | `N × 4` px |
| `w-full`, `w-1/2`, `w-2/3` … | `"100%"`, `"50%"`, `"67%"` |
| `w-auto`, `w-px`, `w-[320px]`, `h-[50%]` | `"auto"`, `1`, `320`, `"50%"` |
| `max-w-xs` … `max-w-7xl` | Tailwind's pixels: 320, 384, 448, 512, 576, 672, 768, 896, 1024, 1152, 1280 |

`max_width` does not narrow what text is measured against (see *Writing
views*): a paragraph that has to wrap wants a `w-[Npx]`, not a `max-w-*`.

### Type

| Classes | EUI |
|---|---|
| `text-xs` `sm` `base` `lg` `xl` `2xl` `3xl` `4xl` | `size` 0 – 7: 12, 14, 16, 18, 20, 24, 30, 36 px, Tailwind's line heights with them |
| `font-normal` `medium` `semibold` `bold` | `weight` — four real faces of Inter |
| `font-sans`, `font-mono` | `font` |
| `text-left` · `text-center` · `text-right` · `text-justify` | `text_align: start` · `center` · `end` · `justify` |
| `truncate`, `line-clamp-N`, `line-clamp-none` | `clamp` 1, N, 0 |
| `underline`, `no-underline`, `line-through` | `underline`, `strike` |
| `uppercase`, `lowercase`, `capitalize`, `normal-case` | the string, transformed by `text()` on the server |

A transform is not a style key, so it rides in `tw_style()`'s answer as
`tw_case` until `text()` takes it out and rewrites the string:
`text("Open tickets", tw_style("text-xs font-medium uppercase"))` sends
`OPEN TICKETS`. `capitalize` raises the first letter after each space and
leaves the rest as written, as CSS does. On a box it raises — there is no
string there to change, and CSS's inheritance of it down to every text in
the box has no carrier — and under a state it raises, since a state cannot
rewrite a string. Left on a style that never reaches `text()`, the encoder
refuses it by name.

### Colour

`bg-*`, `text-*`, `border-*` and `ring-*` take a colour, and a colour is a
**role** — the client resolves it against the viewer's theme, so the page is
right in dark mode without a `dark:` class. Which role a gray is depends on
what it paints:

| Shade | `bg-` | `text-` | `border-`, `ring-` |
|---|---|---|---|
| `white` | `surface.raised` | `accent.on` (white sits on a filled colour) | `surface.raised` |
| `gray-50` | `surface.base` | — | — |
| `gray-100` | `surface.sunken` | — | `border.subtle` |
| `gray-200` | `border.subtle` | — | `border.subtle` |
| `gray-300` | `border.default` | `text.disabled` | `border.default` |
| `gray-400` | `border.strong` | `text.disabled` | `border.strong` |
| `gray-500`, `gray-600` | — | `text.muted` | `border.strong` (500) |
| `gray-700` … `gray-950` | `text.default` (800 and up) | `text.default` | — |

`slate`, `zinc`, `neutral` and `stone` are read as `gray`.

| Palette | Role |
|---|---|
| `indigo-600` · `-500` · `-700` | `accent.base` · `accent.hover` · `accent.active` |
| `red` / `rose` | `danger` |
| `green` / `emerald` | `success` |
| `yellow` / `amber` | `warning` |
| `blue` / `sky` | `info` |

For the four status families, `-50` and `-100` are the `.subtle` tint and
`-500` to `-800` the `.base`.

The roles by name work too: `bg-accent`, `text-muted`, `border-subtle`,
`bg-danger-subtle`, `bg-surface-raised`, `text-series-2` — a dotted role with
dashes. `bg-transparent` is `none`. `bg-[#1E293B]` is a literal, which
03 §4 reserves for marks and data: a literal is the same colour in every
theme, so it is wrong in one of them.

An opacity suffix is taken in the two places Tailwind UI uses one: the faint
ring round a card (`ring-gray-900/5`, `ring-black/10`, up to `/20` —
`border.subtle`; a status colour's ring up to `/30` — its `.subtle`) and a
scrim (`bg-black/50`, `bg-white/80`, `bg-gray-500/75` — a literal with that
alpha).

### Borders, radius, shadow

| Classes | EUI |
|---|---|
| `border`, `border-0` `-2` `-4` `-8` | `border` width on all four sides |
| `border-x` `-y` `-t` `-r` `-b` `-l`, with an optional width | those sides |
| `ring-1`, `ring-2` | a **border** of that width — see below |
| `ring-inset` | nothing: an EUI border is already inside the box |
| `rounded-none` · `rounded-sm` · `rounded`, `-md`, `-lg` · `-xl`, `-2xl`, `-3xl` · `-full` | `radius` 0 · 1 · 2 · 3 · 4 |
| `shadow-none` · `shadow-sm` · `shadow`, `-md` · `-lg`, `-xl`, `-2xl` | `shadow` 0 · 1 · 2 · 3 |

### Everything else

| Classes | EUI |
|---|---|
| `opacity-N` | `opacity`, `round(N × 2.55)` — a byte |
| `cursor-pointer` `default` `text` `wait` `not-allowed` `grab` `grabbing` `col-resize` `row-resize` | `cursor` |
| `transition`, `transition-colors` `-all` `-opacity` `-shadow` · `transition-none` | `transition: fast` · `none` |
| `duration-N` | `fast` to 100 ms, `base` to 200, `slow` to 350, `slower` to 700, `slowest` beyond |
| `backdrop-blur-sm` … `-3xl` | `blur` 4, 8, 12, 16, 24, 40, 64 px |
| `animate-spin`, `animate-none` | `animation` |
| `select-none` | nothing: only editable nodes select text |

### States

`hover:`, `active:`, `focus:` and `disabled:` go in front of any class above.
One state per class.

`focus-visible:` is `focus:`. The ring it is used for is the client's: a
focused node draws 2 px of `focus.ring` outside its border box when focus
came from the keyboard (03 §3), so `focus-visible:outline*`,
`focus-visible:ring*`, `focus:outline*` and `outline-none` say nothing it does
not already do, and are taken as nothing. Anything else under
`focus-visible:` is laid on while the node has focus, however it came.
`focus:ring-2` is still a border, as `ring-2` is at rest.

A focus state that changes a border's *width* moves everything after it by
the difference; reserve the width at rest and change the colour —
`border border-gray-300 focus:border-indigo-600`, not `focus:ring-2`.

### Breakpoints

`sm:`, `md:`, `lg:`, `xl:` and `2xl:` apply from 640, 768, 1024, 1280 and
1536 px up — Tailwind's screens and the catalogue's `BP` — against the width
`tw()` is given:

```soli
# In a view, with the width the client sent on connect and on every resize.
vw = state["viewport"]["width"]
node("box", {"tw": "flex flex-col gap-4 sm:flex-row sm:items-end", "vw": vw}, [title, actions])
text(label, tw_style("text-sm md:text-base", false, vw))
tw("hidden lg:flex", vw)
```

Mobile first, the way the stylesheet orders them: the bare classes, then each
breakpoint the width has reached from the smallest up, then the states in the
same order — so `p-2 md:p-4` is `p-4` from 768 px whatever order it was
written in, and the larger breakpoint wins. A breakpoint goes before or after
a state (`md:hover:bg-gray-50`), one of each per class. A class under a
breakpoint the width has not reached is still parsed, so a refused class
raises on a phone too.

Without a width, a breakpoint class raises — *`md:flex` needs the viewport
width* — rather than being read as always or never. It is the server that
chooses; the client runs no media query, and a resize is a new render, which
is what it already was. `max-md:` and the other range variants are refused:
write the narrow classes bare and the wider ones prefixed.

The memo keeps a string once per breakpoint the width falls in, never per
width: a window dragged across three hundred widths is six entries.

## Approximations

Where the two scales differ, the class lands on the nearest step, and the
difference is written down here rather than discovered:

- **Radius.** `rounded` (4 px) and `rounded-md` (6 px) are drawn at 8 px with
  `rounded-lg`; `rounded-sm` is 4; `rounded-xl` (12) and `rounded-2xl` (16)
  are 16. The theme's radius scale is 4 / 8 / 16 / full, and the viewer may
  move it.
- **Shadow.** Three steps: `shadow` and `shadow-md` are one, `shadow-lg` to
  `shadow-2xl` another. No colour.
- **Ring.** A ring is a border: it takes layout space, a pixel on each side
  for `ring-1`, where Tailwind's is painted over the box. Write it at rest,
  not only under `focus:`.
- **Fractions** round to a whole percent (`w-1/3` is `33%`).
- **`transition`** is a duration and never a curve or a delay.
- **`text-white`** is `accent.on`, the ink the theme guarantees on a filled
  accent; on a status fill, write `text-danger-on` and its kin.
- **`block`** is a column, whose margins never collapse — no EUI margin does.
  Two siblings with `mb-4` and `mt-2` are 24 px apart, not 16.
- **`mx-auto`** is `self-center`, which centres across the parent's line. In
  a column — where Tailwind writes it — that is horizontal, as `mx-auto` is;
  in a row it would centre vertically, and a row wants a `spacer()` either
  side instead. `my-auto` is the same the other way round.
- **`focus-visible:`** styles are laid on for any focus, from a click as well
  as a key; the ring itself is drawn only for the keyboard, in `focus.ring`
  and not in the outline's colour.

## Refusals

Nothing is dropped. A class `tw()` cannot express raises, and the message
names the class and the reason:

```
tw: 'tracking-tight' has no EUI equivalent — EUI has no letter-spacing; the 64-byte style record has no byte for it
```

| Class | Why |
|---|---|
| `tracking-*`, `leading-*`, `text-sm/6` | no letter-spacing; line height comes with the size |
| `text-5xl` and up | the text scale stops at index 7 |
| `font-light`, `font-black` … | four weights |
| `italic` | no italic face |
| `tabular-nums` and the other figure variants | the client selects no OpenType feature; `font-mono` sets figures that line up |
| `bg-gradient-*`, `from-*`, `via-*`, `to-*` | no gradients |
| `rounded-t-*`, `rounded-tl-*` … | one radius for four corners |
| `space-*`, `gap-x-*`, `divide-*` where they are not the same thing | see [Between children](#between-children) |
| `translate-*`, `rotate-*`, `scale-*` | no transforms |
| `-m*` | margins are unsigned bytes |
| `p-7`, `gap-40` and every step off the space scale | the message names the two nearest steps; see [Spacing](#spacing) |
| `m-auto`, `ml-auto`, `mr-auto`, `mt-auto`, `mb-auto` | no auto margins along a line: `spacer()`, or `justify-between` on the parent |
| `fixed`, `sticky`, `inset-*`, `top-*` … | `absolute` inside a `stack` is the one positioning |
| `inline`, `inline-block`, `table`, `contents` | no inline flow; a box is `flex`, `flex-col`, `block`, `grid` or `hidden` |
| `whitespace-*` | text wraps at its box's width; `truncate` keeps it to one line |
| `pointer-events-none` | the topmost node takes the pointer, and an event walks up from it, never through to a sibling below |
| `w-screen`, `h-screen` | the view is given the viewport; use it |
| `overflow-auto`, `overflow-x-*` | scrolling is a `scroll()` node |
| `ring`, `ring-offset-*`, `outline-*` at rest | a ring is a border of 1 or 2 px; the client draws focus (under `focus:` and `focus-visible:` these are taken as nothing — see *States*) |
| `blur-*`, `drop-shadow-*` and the other filters | `backdrop-blur-*` is the one blur |
| `animate-pulse`, `animate-bounce` | `animate-spin` is the one animation |
| `ease-*`, `delay-*` | one curve, no delay |
| `sm:`, `md:` … without a width | the width is the view's to give — `tw(classes, width)`, `"vw"` on a node |
| `max-md:` and the range variants | write mobile first |
| `dark:` | roles already follow the theme |
| `focus-within:` | a local handler restyles the node it is on, not an ancestor |
| `group-hover:`, `peer-*:`, `first:`, `before:` … | no group, structural or pseudo-element state |
| a palette hue that is no role (`purple`, `orange` …) | the message names the nearest role |
| `indigo-50` | the accent has no tint; `bg-info-subtle` is the nearest wash |
| any other name | *unknown class*, with a pointer here |

`examples/demo-app/tests/tw_spec.sl` pins the table and the refusals, and
sends one of every accepted class — `tw_examples()`, 247 of them, at a width
past every breakpoint — through the server's encoder (`eui_render`), which
raises on a key or a value it does not know; a divided tree and a transformed
text node go through it whole. That is what 03 §4 asks of a translation like
this one.
