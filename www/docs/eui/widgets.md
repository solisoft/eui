# Widget catalogue

> The primitives are specified; the catalogue is designed and **not yet built**.
> The first stage ships about fifteen of these.

There are two tiers, and the split is what lets the catalogue grow without ever
shipping a new client.

## Tier 0 — primitives

Implemented in the client. **Fourteen kinds, and the set is closed.** This is
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
| `canvas` | A retained path list, for charts and custom marks |
| `spacer` | Flexible empty space. Leaf |
| `divider` | A hairline rule. Leaf |
| `overlay` | A layer above the flow: menus, tooltips, dialogs |
| `slot` | A named insertion point for composed content |
| `sizer` | An invisible box that only imposes constraints |

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
`checkbox`, `radio_group`, `switch`, `slider`, `date_picker`, `time_picker`,
`calendar`, `color_picker`, `rating`, `file_drop`, `form` with error display

**Structure.** `card`, `panel`, `sheet`, `dialog`, `drawer`, `popover`,
`tooltip`, `tabs`, `accordion`, `split_pane`, `resizable`, `stepper`

**Navigation.** `navbar`, `sidebar`, `breadcrumb`, `pagination`, `tree_view`

**Data.** `table` (sortable, virtualised), `data_grid`, `list_item`, `chart`
(line, bar, area, donut, drawn on `canvas`), `stat`, `code_block`, `markdown`

**Feedback.** `toast`, `banner`, `progress`, `spinner`, `skeleton`,
`empty_state`, `avatar`, `badge`, `chip`

Every one of them is themeable through roles, keyboard-navigable, and carries
documented accessibility semantics.

## How a button is a box

```soli
def button(label, variant: :primary, size: :md, loading: false, on_click: nil)
  box(
    display:  :row,
    align:    :center,
    justify:  :center,
    gap:      @space.2,
    pad_x:    _pad_x(size),
    height:   _height(size),
    radius:   @radius.md,
    bg:       _bg(variant),
    fg:       _fg(variant),
    cursor:   :pointer,
    on_click: on_click
  ) do
    spinner(size: :sm) if loading
    text(label, style: _text(size), weight: :semibold)
  end
end
```

No new node kind, no client change, no protocol version. The button is a
function that returns primitives, and `_bg(variant)` returns a **role** — which
is why the same button follows the viewer into dark mode without the server
knowing they went there.
