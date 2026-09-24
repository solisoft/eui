# Widget catalogue

> The primitives are specified (`spec/03`) and painted. Five hundred and
> thirty-one plain Soli functions build the catalogue below, in
> `examples/demo-app/app/controllers/eui_builders.sl` and its `_forms`,
> `_charts`, `_feed`, `_markdown` and `_tw` companions, and a `gallery`
> component shows them together. It is drawn the way Tailwind UI draws an
> application, and [`tw()`](/docs/tailwind) writes a style in Tailwind's
> classes. `soli new <app> --eui` starts an application with those files
> already in it. Every signature is in
> [Components](/docs/components); this page is the shape of the catalogue,
> not its reference.

There are two tiers, and the split is what lets the catalogue grow without ever
shipping a new client.

## Tier 0 — primitives

Implemented in the client. **Seventeen kinds, and the set is closed.** This is
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
| `scene` | A 3D picture, drawn by a program the server names. Leaf, and behind a capability |

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

**Input.** `text_field`, `password_field` (the client paints marks; `secret` is a prop, not a kind), a `placeholder` on any field (the client draws it muted while the field is empty), `otp_field` (six boxes, paste fills them, Backspace walks back), `textarea`, `select`, `combobox` (type to narrow a fixed list), `multi_select`,
`tag_field` (free text with suggestions), `checkbox`, `radio_group`, `switch`, `slider`, `date_picker`,
`datetime_picker`, `date_range_picker` (one calendar engine, two selections),
`time_picker`, `calendar`, `color_picker`, `rating`, `file_drop`, `form` with
error display

**Structure.** `card`, `panel`, `sheet`, `dialog`, `drawer`, `popover`,
`tooltip`, `tabs`, `accordion`, `split_pane` (draggable dividers on both axes,
nested, keyboard-movable), `resizable`, `stepper`

**Navigation.** `navbar`, `sidebar`, `breadcrumb`, `pagination`, `tree_view`

**Data.** `table` (sortable, virtualised), `data_grid` (editable cells, header outside the scroll), `multi_select_list`
and `multi_select_window` (tickable rows, select-all across the whole count rather than the window), `list_item`, `chart`
(line, bar, area, donut, drawn on `canvas`), `stat`, `code_block`

**Markdown.** `markdown` and `markdown_file` (a document as a tree of nodes),
`md_doc_rows` (the same as rows for a windowed list), `markdown_editor` and
`markdown_editor_step` (a block editor that writes one, with pictures and
files), and the pure `md_edit_*` model the two of them move

**Feedback.** `toast`, `banner`, `progress`, `spinner`, `skeleton`,
`empty_state`, `avatar`, `badge`, `chip`

Every one of them is themeable through roles: two hardcoded colours in the whole
file, both deliberate scrims, and everything else a role — which is why the same
widget follows a viewer into dark mode, and into their desktop's palette, with
the server never seeing a colour.

One consequence of the protocol shows through the catalogue here: `click`
carries `[x, y]` and no modifier bits (06 §1), and `double_click` and
`long_press` are in the enum but no client emits them. So a multi-selection is
one row at a time, or all of them; a range is expressible from the keyboard,
where `key_down` does carry the modifiers, and not from the pointer.

The other two claims this page used to make were not true, and are being made
true rather than restated. Every interactive widget is now built on `control`,
which gives it hover, press and a disabled state, a size, and the props with
which it declares what it is. Four have been moved onto it — `checkbox`,
`switch`, `tabs`, `icon_button` — and twenty still answer the pointer with a
cursor and nothing else. Keyboard navigation beyond `Tab` needs client work that
has not been done: there is no roving focus and no type-ahead. See
[what is not there yet](/docs/status).

## The 3D scene, and what it cost

`scene` is the seventeenth kind, and the only one whose picture the client
does not work out from the tree. The server names a WGSL module and a mesh;
the client renders them into a target of its own and draws that target as a
single quad in the node's box.

Compositing rather than drawing into the frame is what makes it cheap to own.
The corner radius, the opacity, the scissor of a scroller, the slide of a page
transition — the quad carries all of them, and the module knows about none.
The 2D pipeline keeps no depth buffer, so an application with no scene pays
nothing for the ones that do.

It is the most expensive thing on this page, and not in frames. A shader is
code from the network, which the [rationale](/docs/overview) refuses; it is
admitted only through a verifier of its own, which proves before compiling
that the module terminates in a bounded number of steps and can reach nothing
but the uniform block the client hands it. What that verifier cannot do is
make the *driver's* shader compiler safe — it runs in the window process, on
input the server chose, and no amount of checking moves it somewhere else.
That trade is written down rather than glossed over, in `spec/11-shaders.md`
§6, and it is why `scene` is a capability a person grants rather than
something every application has.

Three things a scene deliberately cannot do: its target is not readable, so
nothing on screen goes back to the server; a press on it reports a position
and never an object, because picking is readback under another name; and a
scene that is not playing draws once and wakes nothing.

## Sound and moving pictures

`audio` and `video` were the fifteenth and sixteenth kinds, and they cost a
protocol version, exactly as this page said they would. `scene` was the
seventeenth and cost the second.

They are in because the decode stays small and stays confined. `eui-audio`
decodes with `symphonia` and mixes up to eight sources; `eui-video` decodes
GIF and animated WebP in Rust. Both run in the **sandboxed worker** — the
process that may not open a file, a socket or a device — and hand frames and
samples to the window process, which owns the audio device and the GPU. No
codec library is downloaded, and nothing is handed to a system decoder.

Compressed video (H.264 and friends) is still out. Taking it would mean
shipping a codec or calling the platform's decoder, and neither fits in the
worker as it stands.

## Files: two props, and a person's hand

A server cannot reach a file on someone's machine, and cannot put one there.
Two props say what a node would like to do about that, and the client decides
whether anything happens.

| Prop | Value | Means |
|---|---|---|
| `pick` | `"csv,pdf"`, or `[accept, flags, max]` | activating this node opens the platform's open dialog |
| `save` | `"export.csv"` | activating this node opens the platform's save dialog |

A dialog opens only when **all** of this holds: the person *activated* the
node — a click, or `Enter`/`Space` on it; the node also declares a **server**
handler for the event that answers (`file_pick`, `file_save`); and the
capability behind it — `fs.pick` or `fs.save` — was granted. A tree that
merely arrives opens nothing, and neither does a batch, a timer or a local
handler. There is no frame that opens a dialog, because a frame is not a
person.

What comes back is ordinary: one `file_pick` event per file, naming it and
its size, with the bytes following as `Upload` frames; or one `file_save`
naming what the person called it, which *is* the request for the bytes — the
server answers it with `Blob` frames, and the client writes them where the
person said. Nothing is written until the first chunk arrives, so a save the
server never answers leaves nothing behind.

An export therefore costs a round trip rather than riding on an asset, and
that is the point: the bytes are generated when someone asks for them, they
are nobody else's, and no one off this session can fetch them.

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
