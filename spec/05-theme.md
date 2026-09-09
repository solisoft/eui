# 05 — Theme

Status: **normative**. Implemented by `crates/eui-theme`.

The server never sends a colour. It sends a *role* — `surface.raised`,
`text.muted`, `accent.base` — and a *scale index* — `space.4`, `text.lg` — and
the client resolves them against the active theme and the viewer's own mode,
density and font scale. Switching to dark mode costs zero bytes, the viewer's
accessibility settings cannot be overridden because the server never sees them,
and the server learns nothing about the person.

Because the client resolves, the resolution algorithm is part of the protocol:
two conforming clients given the same theme document and the same viewer
settings MUST produce the same values, within the tolerance in §7.

## 1. Colour roles

A `ColorRef` in role space (`1..=0x7FFF`, see [`02-wire-format.md`](02-wire-format.md)
§3.2) names one of these. The numbering is stable; ids `29..=0x7FFF` are
reserved and MUST be rejected.

| id | Role | id | Role |
|---:|---|---:|---|
| 1 | `surface.base` | 15 | `success.on` |
| 2 | `surface.raised` | 16 | `warning.base` |
| 3 | `surface.sunken` | 17 | `warning.subtle` |
| 4 | `surface.overlay` | 18 | `warning.on` |
| 5 | `text.default` | 19 | `danger.base` |
| 6 | `text.muted` | 20 | `danger.subtle` |
| 7 | `text.inverted` | 21 | `danger.on` |
| 8 | `text.disabled` | 22 | `info.base` |
| 9 | `accent.base` | 23 | `info.subtle` |
| 10 | `accent.hover` | 24 | `info.on` |
| 11 | `accent.active` | 25 | `border.subtle` |
| 12 | `accent.on` | 26 | `border.default` |
| 13 | `success.base` | 27 | `border.strong` |
| 14 | `success.subtle` | 28 | `focus.ring` |

## 2. Scales

Indices are what a `StyleRecord` carries. Values are device-independent pixels
at `cozy` density and font scale 1.0; §5 says how the viewer's settings change
them.

**`space`** (13 entries, index 0–12): `0, 2, 4, 8, 12, 16, 20, 24, 32, 40, 48, 64, 96`

**`radius`** (index 0–4): `none 0`, `sm md/2`, `md`, `lg 2·md`, `full 9999`,
where `md` is the theme's `radius_md` (§3), `6` by default.

**`text`** (index 0–7), as `size / line-height`:

| index | name | size | line |
|---:|---|---:|---:|
| 0 | `xs` | 11 | 16 |
| 1 | `sm` | 13 | 18 |
| 2 | `base` | 15 | 22 |
| 3 | `lg` | 17 | 24 |
| 4 | `xl` | 20 | 28 |
| 5 | `2xl` | 24 | 32 |
| 6 | `3xl` | 30 | 38 |
| 7 | `4xl` | 38 | 46 |

The default `StyleRecord` carries `font_size = 2`, `base`.

**`shadow`** (index 0–3), as `y offset, blur, opacity` of black:
`none`, `sm 1 2 0.12`, `md 4 12 0.16`, `lg 12 32 0.24`

**`motion`** (index 0–2), milliseconds: `fast 100`, `base 180`, `slow 320`,
all with the easing curve `cubic-bezier(0.2, 0, 0, 1)`.

A curve is evaluated as CSS evaluates one: it gives `y` for an `x`, `x` is
the fraction of the duration elapsed, and solving `x` for the Bézier
parameter has no closed form — so a client iterates, and `0` and `1` are
exact. Beside the theme's own curve a client keeps four more, chosen by
what is moving rather than named on the wire, because motion that has to
be specified per node is motion nobody gets right twice:

| Curve | Control points | For |
|---|---|---|
| standard | `0.2, 0, 0, 1` | a style change (03 §5) |
| decelerate | `0, 0, 0.2, 1` | something arriving (03 §5 `enter`) |
| accelerate | `0.4, 0, 1, 1` | something leaving |
| smooth | `0.45, 0, 0.55, 1` | rest to rest — a keyboard scroll |
| linear | `0, 0, 1, 1` | a value that is not a movement |

Something arriving decelerates rather than easing: it was already moving
when it was first seen, which is what makes it read as having come from
somewhere instead of having been switched on.

**`control`** heights, used by composed widgets: `sm 28`, `md 36`, `lg 44`.

An index past the end of a scale is an error at style definition time; the
client rejects the `DefStyle`.

## 3. The theme document

A theme is an `EUIT` record ([`02-wire-format.md`](02-wire-format.md) §7)
carrying **seeds**, not palettes. Every colour in §1 is derived from them by
§4, so a theme author chooses four hues and gets three consistent modes.

| key | Field | Value | Default |
|---:|---|---|---|
| 1 | `accent` | OKLCH seed `(L, C, h)` | `(0.55, 0.18, 264)` |
| 2 | `surface` | OKLCH seed; only `C` and `h` are used | `(0.98, 0.006, 250)` |
| 3 | `radius_md` | px | `6` |
| 4 | `density` | 0 compact, 1 cozy, 2 comfortable | `1` — a *preference*, the viewer's own setting wins |
| 5 | `font_sans` | asset hash, or absent for the client's built-in face | absent |
| 6 | `font_mono` | asset hash, or absent | absent |

`L` and `C` are clamped to `[0, 1]` and `[0, 0.4]`; `h` is taken modulo 360.
Status hues are fixed by the protocol, not the theme: `success 145`,
`warning 80`, `danger 25`, `info 250`. A theme cannot make danger green.

## 4. Resolution

All arithmetic is IEEE-754 binary64. Colour work happens in OKLab / OKLCH
(Ottosson 2020); the conversion constants are in §8.

### 4.1 Lightness targets

For each mode the algorithm assigns a target lightness `L` and chroma `C` to
every role. Hue comes from the relevant seed, or from the fixed status hues.
`sL`, `sC`, `sh` are the surface seed's values; `aL`, `aC`, `ah` the accent's.

| Role | light `L` | dark `L` | high-contrast `L` | `C` | hue |
|---|---:|---:|---:|---|---|
| `surface.base` | 0.985 | 0.19 | 0.00 | min(sC, 0.02) | sh |
| `surface.raised` | 1.000 | 0.24 | 0.05 | min(sC, 0.02) | sh |
| `surface.sunken` | 0.955 | 0.15 | 0.00 | min(sC, 0.02) | sh |
| `surface.overlay` | 1.000 | 0.27 | 0.08 | min(sC, 0.02) | sh |
| `text.default` | 0.18 | 0.93 | 1.00 | min(sC, 0.015) | sh |
| `text.muted` | 0.45 | 0.72 | 0.90 | min(sC, 0.015) | sh |
| `text.inverted` | 0.98 | 0.15 | 0.00 | min(sC, 0.015) | sh |
| `text.disabled` | 0.65 | 0.50 | 0.70 | min(sC, 0.015) | sh |
| `accent.base` | clamp(aL, 0.40, 0.60) | clamp(aL, 0.60, 0.80) | 0.80 | aC | ah |
| `accent.hover` | base − 0.06 | base + 0.06 | base + 0.06 | aC | ah |
| `accent.active` | base − 0.12 | base + 0.12 | base + 0.12 | aC | ah |
| `*.base` (status) | 0.52 | 0.72 | 0.80 | 0.14 (`warning` 0.12) | fixed |
| `*.subtle` (status) | 0.95 | 0.25 | 0.15 | 0.04 / 0.06 / 0.06 | fixed |
| `border.subtle` | 0.92 | 0.26 | 0.50 | min(sC, 0.02) | sh |
| `border.default` | 0.86 | 0.32 | 0.70 | min(sC, 0.02) | sh |
| `border.strong` | 0.70 | 0.45 | 0.90 | min(sC, 0.02) | sh |
| `focus.ring` | 0.55 | 0.75 | 0.85 | max(aC, 0.18) | ah |

### 4.2 The `on` roles

`accent.on` and each `*.on` are whichever of pure white `(1, 0, 0)` or pure
black `(0, 0, 0)` has the greater WCAG contrast ratio against the
corresponding `base`, computed after §4.3 has run on that base.

### 4.3 Contrast enforcement

Contrast is guaranteed by construction, not by a check. After the targets are
assigned, and before the `on` roles are chosen, the algorithm adjusts
lightness until every pair below meets its ratio, using WCAG 2.x relative
luminance on the gamut-clipped sRGB values:

| Foreground | Against | Minimum |
|---|---|---:|
| `text.default` | each of `surface.*` | 7.0 |
| `text.muted` | each of `surface.*` | 4.5 |
| `text.disabled` | `surface.base` | 3.0 |
| `accent.base`, each status `.base` | `surface.base` | 3.0 |
| `focus.ring` | `surface.base` | 3.0 |
| `border.default` | `surface.base` | 1.5 |
| `border.strong` | `surface.base` | 3.0 |

To adjust a foreground against a background: move the foreground's `L` **away
from** the background's `L` in steps of `0.01`, re-clipping to gamut each
step, and stop at the first step that meets the ratio, or after 60 steps.
Moving away means towards `1.0` when the mean `L` of the backgrounds is below
`0.5`, else towards `0.0`. A foreground checked against several backgrounds is
adjusted until it meets the ratio against all of them.

The `accent.hover` and `accent.active` offsets are re-derived from the
adjusted `accent.base`.

### 4.4 Gamut clipping

An OKLCH colour outside sRGB is brought into gamut by reducing chroma alone:
binary-search `C` in `[0, C]` for 16 iterations, keeping the largest value
whose linear-sRGB components all lie in `[0, 1]`. Lightness and hue are never
changed by clipping.

### 4.5 Output

Each role resolves to 8-bit sRGB with alpha 255, packed `0xRRGGBBAA`.
Quantisation is round-half-up of `component × 255`.

## 5. The viewer's axes

Applied on top of the theme, always, and never reported back to the server
beyond the coarse fields in the `Viewport` frame.

- **Mode** selects the column in §4.1.
- **Density** multiplies every `space` entry and every `control` height:
  `compact 0.8`, `cozy 1.0`, `comfortable 1.25`. Results are rounded to the
  nearest whole pixel; `space.0` stays `0`.
- **Font scale** multiplies every `text` size and line height, rounded to the
  nearest whole pixel. It does not touch `space`: an enlarged font on an
  unchanged grid is the behaviour a reader who enlarged the font asked for.
- **The desktop's palette.** A client MAY replace the resolved colour of
  any role with one the viewer's desktop publishes — its background,
  foreground, accent and status colours — and take the palette's own
  light or dark as the mode. The overrides sit on top of the theme in the
  palette's own mode — a viewer who switches to the other mode gets the
  theme's own colours for it, and the desktop's again on returning, so an
  application's light/dark switch still switches something — and are
  applied after §4's resolution, so a
  theme's contrast guarantees do not extend to them: the desktop chose
  the colours, and the desktop answers for them. The server sees nothing
  of this but the mode. The reference client follows Omarchy on Linux
  (`~/.local/state/omarchy/current/theme/colors.toml`), live; the mapping
  is in `eui-client/src/desktop_theme.rs`.

## 6. Literal colours

A `DefColor` literal bypasses all of the above and is correct for a brand mark
or a data series. It is wrong for a surface, a border or body text, because it
will not follow the viewer anywhere. Nothing in the protocol prevents it; the
server-side linter warns.

## 7. Conformance tolerance

Two implementations MUST agree on every resolved colour to within **±1 per
8-bit channel**, and on every resolved length exactly. The tolerance exists
because `cbrt` and `powf` are not correctly rounded on every platform's libm;
it is not licence to deviate in the algorithm.

## 8. Colour constants

OKLab, from linear sRGB (Ottosson):

```
l = 0.4122214708 r + 0.5363325363 g + 0.0514459929 b
m = 0.2119034982 r + 0.6806995451 g + 0.1073969566 b
s = 0.0883024619 r + 0.2817188376 g + 0.6299787005 b
l' = cbrt(l)   m' = cbrt(m)   s' = cbrt(s)
L = 0.2104542553 l' + 0.7936177850 m' − 0.0040720468 s'
a = 1.9779984951 l' − 2.4285922050 m' + 0.4505937099 s'
b = 0.0259040371 l' + 0.7827717662 m' − 0.8086757660 s'
```

and back:

```
l' = L + 0.3963377774 a + 0.2158037573 b
m' = L − 0.1055613458 a − 0.0638541728 b
s' = L − 0.0894841775 a − 1.2914855480 b
l = l'³   m = m'³   s = s'³
r =  4.0767416621 l − 3.3077115913 m + 0.2309699292 s
g = −1.2684380046 l + 2.6097574011 m − 0.3413193965 s
b = −0.0041960863 l − 0.7034186147 m + 1.7076147010 s
```

OKLCH is `(L, C = √(a² + b²), h = atan2(b, a))` with `h` in degrees.

sRGB transfer: linear `x ≤ 0.0031308 → 12.92 x`, else
`1.055 x^(1/2.4) − 0.055`; inverse `x ≤ 0.04045 → x / 12.92`, else
`((x + 0.055) / 1.055)^2.4`.

WCAG relative luminance `Y = 0.2126 R + 0.7152 G + 0.0722 B` on linear values;
contrast ratio `(Y₁ + 0.05) / (Y₂ + 0.05)` with the lighter colour first.
