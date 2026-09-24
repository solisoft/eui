# Theming

> Normative in `spec/05-theme.md` and implemented by `eui-theme`: the
> resolution algorithm below is what the client runs.

The server never sends a colour. It sends a **role**, and the client resolves it.

That one decision buys three things at once: switching to dark mode costs zero
bytes and zero round trips; the viewer's contrast, density and font-size
preferences are always honoured because the server cannot override what it never
sees; and the server learns nothing about the person using it.

## Roles

| Group | Roles |
|---|---|
| Surface | `surface.base`, `.raised`, `.sunken`, `.overlay` |
| Text | `text.default`, `.muted`, `.inverted`, `.disabled` |
| Accent | `accent.base`, `.hover`, `.active`, `.on` |
| Status | `success`, `warning`, `danger`, `info`, each with `.base`, `.subtle`, `.on` |
| Border | `border.subtle`, `.default`, `.strong` |
| Focus | `focus.ring` |

## Scales

`space.0`–`space.12` · `radius.none/sm/md/lg/full` · `text.xs`–`text.4xl`,
each pairing a size with a line height · `weight.regular/medium/semibold/bold` ·
`shadow.sm/md/lg` · `motion.fast/base/slow/slower/slowest` with their curves

The scales are Tailwind's, so a design written in Tailwind classes carries
over index for index:

| Scale | Values | Tailwind |
|---|---|---|
| `text` | `12/16 14/20 16/24 18/28 20/28 24/32 30/36 36/40` (size / line) | `text-xs` … `text-4xl` |
| `radius` | `0 4 8 16 full` with the default `radius_md` of 8 | `rounded-none`, `rounded`, `rounded-lg`, `rounded-2xl`, `rounded-full` |
| `shadow` | `sm` one layer, `md` and `lg` two | `shadow-sm`, `shadow-md`, `shadow-lg` |

A style's `shadow` index paints soft black rectangles under the box, one per
layer of the scale — `md` is `0 4 6 -1 / 10%` then `0 2 4 -2 / 10%`, written
as CSS writes a `box-shadow` — each offset, grown by its *spread*, blurred,
and drawn by the same quad pipeline as the box with a fade across the blur.
The spreads of `md` and `lg` are negative: they pull each layer in before it
is blurred, which is what keeps the shadow under the card, below it, rather
than a grey halo round it. A style's `transition` names a `motion` index:
when a node's style changes to it, the client eases the background, foreground,
border colour and opacity from the old values over that duration, along
`cubic-bezier(0.2, 0, 0, 1)`. The two slowest steps — 560 ms and a second —
are for a thing *arriving* over a distance, where the duration reads against
how far it travels; a control's hover still belongs at `fast`. Layout never animates, the server never hears of
it, and the window wakes only while a transition runs — at rest it sleeps.

## The three viewer axes

Applied on top of whatever the application shipped, always:

- **Mode** — light, dark, high contrast
- **Density** — compact, cozy, comfortable, which scales `space` and control
  heights together
- **Font scale** — an accessibility multiplier

Changing any of them re-resolves the style table and re-runs layout. Nothing
crosses the network.

## Authoring

With no theme at all, the client resolves **Tailwind UI's palette**: a
gray-50 page, white cards, gray-200 and gray-300 borders, gray-900 and
gray-500 text, and an indigo-600 accent whose hover lightens to about
indigo-500, as a Tailwind button does. The seeds are Tailwind's own OKLCH
values, and a test holds each resolved role to within a few ΔE of the
Tailwind colour it stands in for:

| Role | Light | Dark | Tailwind |
|---|---|---|---|
| `surface.base` | `#f8faff` | `#111418` | gray-50 `#f9fafb` |
| `surface.raised` | `#ffffff` | `#1d1f24` | white |
| `surface.sunken` | `#f1f4fb` | `#090b0f` | gray-100 `#f3f4f6` |
| `border.subtle` | `#e4e7ee` | `#222429` | gray-200 `#e5e7eb` |
| `border.default` | `#cbced5` | `#33353b` | gray-300 `#d1d5dc` |
| `border.strong` | `#8e9198` | `#606369` | gray-400 `#99a1af`, darkened to meet 3:1 |
| `text.default` | `#16181d` | `#e4e8ef` | gray-900 `#101828` |
| `text.muted` | `#6c6f75` | `#a1a5ab` | gray-500 `#6a7282` |
| `accent.base` | `#4f39f6` | `#6667ff` | indigo-600 `#4f39f6` |
| `accent.hover` | `#5d59ff` | `#7982ff` | indigo-500 `#615fff` |
| `accent.active` | `#431be1` | `#8e9aff` | indigo-700 `#432dd7` |

Dark mode is the same algorithm's other column, not a Tailwind palette.

A theme is generated from a seed colour in OKLCH, so the full ramp is
perceptually even rather than hand-tuned:

```soli
theme "brand" do
  accent  seed: oklch(0.62, 0.19, 264)
  surface seed: oklch(0.98, 0.01, 150)
  radius  md: 8
  density :cozy
end
```

Contrast is not something to discover from a user's bug report, and it is not
a check either. The resolver assigns each role a lightness target per mode,
then nudges every specified pair — body text on each surface at 7:1, muted
text at 4.5:1, accents and borders at 3:1 — apart until it passes. A theme
cannot fail contrast, because the algorithm does not have a failing path.

## Typefaces

A client ships two faces, `sans` and `mono`, and shapes with those and
nothing else. `sans` is Inter in all four weights the wire names — regular,
medium, semibold and bold are each a face of their own, so a `semibold`
heading is not a bold one. The client never asks the machine what fonts it has, and it never
fetches one from a font service. An application that wants its own typeface
*serves* it, like any other asset:

```soli
eui_font("Playfair Display", ["public/fonts/playfair-400.ttf",
                              "public/fonts/playfair-700.ttf"])

text("A heading", font: "Playfair Display", weight: :bold)
```

One face per weight — `weight` picks among the faces of a family — and at
most eight per family, nine families beside sans and mono. The names `sans`
and `mono` replace the client's own faces rather than taking a role of their
own.

The wire carries a role, one byte, and the faces travel as content-addressed
assets: the window fetches them from its own origin and checks the bytes
against their own name before the shaper sees them. So a face from Google
Fonts is one the **server** downloaded, once, and re-served — `HTTP.download`
behind a `File.exists` guard is the whole of it. The viewer's address never
reaches the third party, which is the difference between this and a
`@font-face` rule.

Until a face arrives the role draws in `sans`, and so does a role whose bytes
would not parse. A missing font is never why a page is blank.

## Literals, and when they are right

A literal colour is available:

```soli
box(bg: rgb(255, 87, 34))
```

It is right for a brand mark and for a data series that must stay identifiable
across themes. It is wrong for a surface, a border, or body text, because a
literal is frozen at whatever the designer's monitor showed and will not follow
the viewer anywhere. The linter says so.
