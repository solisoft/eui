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

The gallery's Catalogue section opens with a card written in nothing else.

## What it answers

`tw(classes)` returns six hashes:

| Key | What it is |
|---|---|
| `s` | The resting style — an ordinary style hash |
| `hover`, `press`, `focus`, `disabled` | What `hover:`, `active:`, `focus:` and `disabled:` classes change, as **deltas** over `s` |
| `props` | What Tailwind says in a class and EUI says in a prop: `grid-cols-N` is `{"columns": N}` |

The deltas are the shape a `TONES` entry has, which is the point: a `tw()`
result is a tone, and everything that takes a tone takes one.

| Where | What it does with the classes |
|---|---|
| `node(kind, {"tw": "..."}, children)` — so `column`, `row`, `stack` | The resting classes become the style, under any key written beside `"tw"` (a builder's own `display` still wins). States become local handlers on `self`: `pointer_enter`/`leave`/`down`/`up` for `hover:` and `active:`, `focus` and `blur` for `focus:`. No state, no handlers |
| `control({"tw": "...", ...})` | The resting classes go over the tone's resting colours and under `shape`; `hover:` and `active:` over the tone's deltas; `disabled:` over the disabled look |
| `stateful(base, "hover:bg-gray-50 ...", on)` | A class string, or a `tw()` result, as the tone |
| `tw_style(classes, disabled = false)` | Only the resting style, for a `text` node, whose style is a plain hash |

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
| `absolute`, `z-N` | `position: absolute`, `z` — inside a `stack` |
| `overflow-hidden`, `overflow-clip` · `overflow-visible` | `overflow: clip` · `visible` |

### Spacing

`p`, `px`, `py`, `pt`, `pr`, `pb`, `pl`, the same for `m`, and `gap-N`. The
value is an index into the space scale (05 §2), which the client multiplies by
the viewer's density:

| Tailwind step | 0 | 0.5 | 1 | 2 | 3 | 4 | 5 | 6 | 8 | 10 | 12 | 16 | 24 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| px | 0 | 2 | 4 | 8 | 12 | 16 | 20 | 24 | 32 | 40 | 48 | 64 | 96 |
| index | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 |

The pixels are the same on both sides. A step the space scale does not have —
`1.5`, `7`, `20` — is an error listing the steps, not a guess.

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

### States

`hover:`, `active:`, `focus:` and `disabled:` go in front of any class above.
One variant per class.

A focus state that changes a border's *width* moves everything after it by
the difference; reserve the width at rest and change the colour —
`border border-gray-300 focus:border-indigo-600`, not `focus:ring-2`.

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
| `italic`, `uppercase` … | no italic face, no text transforms |
| `bg-gradient-*`, `from-*`, `via-*`, `to-*` | no gradients |
| `rounded-t-*`, `rounded-tl-*` … | one radius for four corners |
| `divide-*`, `space-x-*`, `space-y-*` | no child selectors — `border-b` on each row, `gap-N` on the parent |
| `gap-x-*`, `gap-y-*` | one gap for both axes |
| `translate-*`, `rotate-*`, `scale-*` | no transforms |
| `-m*`, `m-auto`, `mx-auto` | margins are unsigned bytes; centre with `justify-center`, `items-center`, `self-center` |
| `relative`, `fixed`, `sticky`, `inset-*`, `top-*` … | `absolute` inside a `stack` is the one positioning |
| `block`, `inline-block` | no flow layout; a box is `flex`, `flex-col`, `grid` or `hidden` |
| `w-screen`, `h-screen` | the view is given the viewport; use it |
| `overflow-auto`, `overflow-x-*` | scrolling is a `scroll()` node |
| `ring`, `ring-offset-*`, `outline-*` | a ring is a border of 1 or 2 px; the client draws focus |
| `blur-*`, `drop-shadow-*` and the other filters | `backdrop-blur-*` is the one blur |
| `animate-pulse`, `animate-bounce` | `animate-spin` is the one animation |
| `ease-*`, `delay-*` | one curve, no delay |
| `sm:`, `md:` … | no media queries — branch on `bp(width)` with the viewport the view is given |
| `dark:` | roles already follow the theme |
| `group-hover:`, `peer-*:`, `first:`, `before:` … | no group, structural or pseudo-element state |
| a palette hue that is no role (`purple`, `orange` …) | the message names the nearest role |
| `indigo-50` | the accent has no tint; `bg-info-subtle` is the nearest wash |
| any other name | *unknown class*, with a pointer here |

`examples/demo-app/tests/tw_spec.sl` pins the table and the refusals, and
sends one of every accepted class — `tw_examples()`, 213 of them — through the
server's encoder (`eui_render`), which raises on a key or a value it does not
know. That is what 03 §4 asks of a translation like this one.
