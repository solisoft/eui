# Theming

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
`shadow.sm/md/lg` · `motion.fast/base/slow` with their curves

## The three viewer axes

Applied on top of whatever the application shipped, always:

- **Mode** — light, dark, high contrast
- **Density** — compact, cozy, comfortable, which scales `space` and control
  heights together
- **Font scale** — an accessibility multiplier

Changing any of them re-resolves the style table and re-runs layout. Nothing
crosses the network.

## Authoring

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

The WCAG AA contrast of **every role pair that can meet on screen** is checked
at build time, in every mode. A theme that fails does not ship. Contrast is not
something to discover from a user's bug report.

## Literals, and when they are right

A literal colour is available:

```soli
box(bg: rgb(255, 87, 34))
```

It is right for a brand mark and for a data series that must stay identifiable
across themes. It is wrong for a surface, a border, or body text, because a
literal is frozen at whatever the designer's monitor showed and will not follow
the viewer anywhere. The linter says so.
