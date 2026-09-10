# Widget catalogue

> The primitives are specified (`spec/03`) and painted. A hundred and fifty plain
> Soli functions build the catalogue below in
> `examples/demo-app/app/controllers/eui_builders.sl`, and a `gallery`
> component shows them together. Every signature is in
> [Components](/docs/components); this page is the shape of the catalogue,
> not its reference.

There are two tiers, and the split is what lets the catalogue grow without ever
shipping a new client.

## Tier 0 — primitives

Implemented in the client. **Sixteen kinds, and the set is closed.** This is
the entire vocabulary the protocol can express.

| Kind | What it is |
|---|---|
| `box` | A styled rectangle that arranges children |
| `text` | A run of text. Leaf |
| `image` | A raster image, referenced by content hash |
| `icon` | A vector glyph from the icon set. Leaf |
| `input` | A single-line editable field |
| `textarea` | A multi-line editable field |
| `scroll` | A clipping viewport with offsets |
| `list` | A virtualised child list — only the visible window is laid out |
| `canvas` | A retained path list in the `paths` prop — polylines, rectangles, areas, circles, arcs — for charts and custom marks |
| `spacer` | Flexible empty space. Leaf |
| `divider` | A hairline rule. Leaf |
| `overlay` | A layer above the flow: menus, tooltips, dialogs |
| `slot` | A named insertion point for composed content |
| `sizer` | An invisible box that only imposes constraints |
| `audio` | A sound. Draws nothing, plays |
| `video` | A moving picture, decoded in the sandboxed worker |

Adding a kind is a protocol version bump. That is a deliberately high price,
and it is why the list has to be argued down to what genuinely cannot be
composed.

## Tier 1 — the catalogue

A **Soli library**, composed from the primitives. It lives on the server, so
adding a widget is a library release, not a client release, and an application
can ship its own without asking anyone.

**Actions.** `button` (primary, secondary, ghost, danger × three sizes × a
loading state), `link`, `icon_button`, `segmented_control`, `menu`,
`context_menu`, `command_palette`, `toolbar`

**Input.** `text_field`, `password_field`, `textarea`, `select`, `combobox`,
`checkbox`, `radio_group`, `switch`, `slider`, `date_picker`,
`datetime_picker`, `date_range_picker` (one calendar engine, two selections),
`time_picker`, `calendar`, `color_picker`, `rating`, `file_drop`, `form` with
error display

**Structure.** `card`, `panel`, `sheet`, `dialog`, `drawer`, `popover`,
`tooltip`, `tabs`, `accordion`, `split_pane` (draggable dividers on both axes,
nested, keyboard-movable), `resizable`, `stepper`

**Navigation.** `navbar`, `sidebar`, `breadcrumb`, `pagination`, `tree_view`

**Data.** `table` (sortable, virtualised), `data_grid` (editable cells, header outside the scroll), `list_item`, `chart`
(line, bar, area, donut, drawn on `canvas`), `stat`, `code_block`, `markdown`

**Feedback.** `toast`, `banner`, `progress`, `spinner`, `skeleton`,
`empty_state`, `avatar`, `badge`, `chip`

Every one of them is themeable through roles: two hardcoded colours in the whole
file, both deliberate scrims, and everything else a role — which is why the same
widget follows a viewer into dark mode, and into their desktop's palette, with
the server never seeing a colour.

The other two claims this page used to make were not true, and are being made
true rather than restated. Every interactive widget is now built on `control`,
which gives it hover, press and a disabled state, a size, and the props with
which it declares what it is. Four have been moved onto it — `checkbox`,
`switch`, `tabs`, `icon_button` — and twenty still answer the pointer with a
cursor and nothing else. Keyboard navigation beyond `Tab` needs client work that
has not been done: there is no roving focus, no type-ahead, and `Escape` never
reaches the server. See [what is not there yet](/gaps).

## Sound and moving pictures

`audio` and `video` were the fifteenth and sixteenth kinds, and they cost a
protocol version, exactly as this page said they would.

They are in because the decode stays small and stays confined. `eui-audio`
decodes with `symphonia` and mixes up to eight sources; `eui-video` decodes
GIF and animated WebP in Rust. Both run in the **sandboxed worker** — the
process that may not open a file, a socket or a device — and hand frames and
samples to the window process, which owns the audio device and the GPU. No
codec library is downloaded, and nothing is handed to a system decoder.

Compressed video (H.264 and friends) is still out. Taking it would mean
shipping a codec or calling the platform's decoder, and neither fits in the
worker as it stands.

## How a button is a box

```soli
def button_variant(label, on_click, bg, fg)
  base = {
    "display": "row", "justify": "center", "align": "center",
    "pad": [2, 4, 2, 4], "min_width": 44,
    "bg": bg, "fg": fg, "radius": 2,
    "cursor": "pointer", "transition": "fast"
  }
  hover = base.merge({"bg": bg == "accent.base" ? "accent.hover" : bg})
  {
    "k": "box",
    "key": "btn:" + on_click + ":" + label,
    "s": base,
    "on": {
      "click": on_click,
      "pointer_enter": {"local": "self.style = @hover", "styles": {"hover": hover}},
      "pointer_leave": {"local": "self.style = @base", "styles": {"base": base}}
    },
    "c": [text(label, {"weight": "semibold"})]
  }
end
```

No new node kind, no client change, no protocol version. The button is a
function that returns a hash of primitives, and its colours are **roles** —
which is why the same button follows the viewer into dark mode without the
server knowing they went there. Its hover runs on the client, against a style
record the session already holds, so it costs no network.
