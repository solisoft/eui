# Components

> Every function on this page exists. The library is
> `examples/demo-app/app/controllers/eui_builders*.sl` — 531 functions over
> six files, all of them plain Soli, none of them native, and what
> `soli new <app> --eui` writes into a new application — and the server that reads what they
> return is `lang/src/serve/eui/tree.rs`. The vocabulary tables below are that
> file's own match arms, not a wish list.

A component is data. A view returns a hash, the server turns it into nodes,
diffs it against the tree that session last received, and sends the patch. So
a "component" here is nothing but a Soli function that returns a hash — you
write one the same way the library wrote its two hundred and sixty.

## A component is two functions

```soli
# config/routes.sl
router_eui("counter", "live#counter", "live#counter_view")
```

`router_eui(component, handler, view)` names three things: the component, the
handler that owns its state, and the view that draws it.

**The handler** takes one hash and returns the new state. It is exactly a
LiveView handler — same registry, same worker pool.

```soli
def counter(event_data)
  event = event_data["event"]
  state = event_data["state"]
  count = state["count"] ?? 0

  if event == "increment"
    {"count": count + 1}
  elsif event == "decrement"
    {"count": count - 1}
  else
    {"count": count}
  end
end
```

| Key in `event_data` | What it carries |
|---|---|
| `event` | The event name: `"connect"`, `"viewport"`, your own names |
| `state` | The state this instance last returned |
| `params` | What came with the event, below |

`params` for an event on a node:

| Key | What it carries |
|---|---|
| `params["kind"]` | The event kind — `"click"`, `"key_down"`, `"change"`, … |
| `params["payload"]` | The event's own value: the key name, the text, the local point |
| `params["props"]` | **The `p` hash of the node that fired.** This is how one handler serves ten thousand rows |

`params` for `connect` and `viewport`:

```soli
params["viewport"]  # {"width": 1280, "height": 800, "scale": 1.0,
                    #  "mode": "dark", "density": "cozy", "font_scale": 1.0}
```

The viewport arrives at `connect` and again whenever it changes, so a view can
be responsive without the client shipping a media-query engine.

**The view** takes the state and returns a node hash.

```soli
def counter_view(state)
  count = state["count"] ?? 0
  with_state({"count": count}, column({"pad": 6, "gap": 4, "bg": "surface.base"}, [
    text("Counter", {"size": 4, "weight": "semibold"}),
    keyed("value", text(count.to_s, {"size": 7, "weight": "bold"})),
    row({"gap": 2}, [
      button("−", "decrement"),
      local_button("+", "state.count += 1; value.text = str(state.count)", "increment")
    ])
  ]))
end
```

### It is a function, not a file

`router_eui`'s third argument is resolved as `controller#action`, and if that
misses, as a top-level Soli function of that name
(`lang/src/serve/eui/mod.rs:83`). There is no `.eui.sl` view file and no
template engine in the path: the view is called with the state and must return
a hash. Put it wherever the handlers load from — `app/controllers/` keeps the
two together in one namespace, which is why the library lives there too.

## The node

Eight keys, all optional but `k`:

| Key | Type | Meaning |
|---|---|---|
| `k` | string | The kind. Defaults to `"box"` |
| `s` | hash | Style — the vocabulary below. Resolved once per session, then referenced by id |
| `c` | array | Children |
| `t` | string | Text content. `text` and `input` nodes |
| `p` | hash | Props. Sent to the client, and handed back to the handler as `params["props"]` |
| `on` | hash | Handlers, by event name |
| `key` | string | Identity for the diff. Keyed children are matched by key and **moved**, not rebuilt |
| `intern` | bool | Put this text in the atom table instead of inline. Only for short strings that repeat — see *Interning* |

### The seventeen kinds

Fourteen primitives, the two the media work added, and `scene`. The set is
closed: adding one is a protocol version bump, which is the deliberately high
price that keeps the catalogue below a library rather than a client release.

| `k` | What it is |
|---|---|
| `box` | A styled rectangle that arranges children |
| `text` | A run of text. Leaf |
| `image` | A raster image, referenced by content hash |
| `icon` | A vector glyph. Leaf |
| `input` | A single-line editable field |
| `textarea` | A multi-line editable field |
| `scroll` | A clipping viewport |
| `list` | A virtualised child list — only the visible window is laid out |
| `canvas` | A retained path list in `paths`: polylines, rectangles, areas, circles, arcs |
| `spacer` | Flexible empty space. Leaf |
| `divider` | A hairline rule. Leaf |
| `overlay` | A layer above the flow: menus, tooltips, dialogs |
| `slot` | A named insertion point |
| `sizer` | An invisible box that only imposes constraints |
| `audio` | A sound. Draws nothing, plays |
| `video` | A moving picture. GIF and animated WebP, decoded in the sandboxed worker |
| `scene` | A 3D pass into a target of its own, drawn by a WGSL module the server names. Leaf, and only where the `scene` capability was granted (03 §1.2, 11) |

## Style

There is no CSS, no cascade and no selector. A style is a flat hash, resolved
by the server into a 64-byte record that the session interns once. **A key it
does not know is an error, not a shrug** — the render fails with
`EUI: unknown style key '…'`, and the same holds for an unknown value or an
unknown colour role.

| Key | Values |
|---|---|
| `display` | `row` · `column` · `stack` · `grid` · `none` |
| `wrap` | `nowrap` · `wrap` · `wrap_reverse` |
| `justify` | `start` · `center` · `end` · `between` · `around` · `evenly` |
| `align` | `start` · `center` · `end` · `stretch` · `baseline` |
| `self` | as `align`, plus `auto` — this node's own cross-axis placement |
| `grow`, `shrink` | 0–255 |
| `gap` | A space index, 0–255 |
| `width`, `height`, `min_width`, `min_height`, `max_width`, `max_height`, `basis` | A length, below |
| `pad`, `margin`, `border` | Edges, below |
| `bg`, `fg`, `border_color` | A colour, below |
| `radius`, `shadow`, `opacity`, `z` | 0–255 |
| `blur` | 0–255 — the node shows what is behind it through a Gaussian this wide, in px, and `bg` tints the result |
| `font` | `sans` · `mono` · the name of a font the application declared with `eui_font` |
| `size` | A text-scale index, 0–255 |
| `weight` | `regular` · `medium` · `semibold` · `bold` |
| `text_align` | `start` · `center` · `end` · `justify` |
| `clamp` | Maximum lines, then ellipsis |
| `underline`, `strike` | `true` / `false` — a rule per line of the shaped run, in the glyphs' own colour (03 §2) |

**There is no italic.** The style record has a family, a size and a weight
and no slant at all (02 §3), and its last reserved byte went to `motion_kind`,
so there will not be one. A real italic is a *face* an application ships with
`eui_font(name, paths)`; `markdown`'s `em_font` names it. Without one, `*mark*`
is drawn a weight heavier — which says emphasis, where the old `text.muted`
said the opposite.

| `overflow` | `visible` · `clip` · `scroll` |
| `transition` | `none` · `fast` · `base` · `slow` · `slower` · `slowest` |
| `animation` | A list: `spin` · `enter` · `exit`, or `none` |
| `position` | `flow` · `absolute` |
| `cursor` | `default` · `pointer` · `text` · `grab` · `grabbing` · `resize_h` · `resize_v` · `wait` · `not_allowed` |
| `blur` | A backdrop radius, 0–255 — what is *behind* the node, not the node |
| `motion` | `fade` · `leading` · `trailing` · `top` · `bottom` · `scale` · `paired` |

**Arriving and leaving.** `animation` is a *set*, not one word: `["enter",
"exit"]` is the ordinary spelling of a page, and `motion` says which way the
entrance comes from — the one leaving takes the mirror without being told, so
a push and a pop are one direction written once. A `motion` with neither an
entrance nor an exit is refused on the wire, in every client and all six
SDKs. `transition` is the *duration* of all of it, from the theme's five-step
`motion` scale, and never a delay.

**`paired` is a shared element** (03 §5.3) — one thing on two pages rather
than a direction. Put it, with the **same** `key`, on the node that is
leaving and on the node that is taking its place, and the arriving one flies
out of the box its partner had instead of going the way its page goes: a
row's avatar becoming a header's avatar, a thumbnail becoming a hero. Both
ends are boxes the client already laid out, so nothing is laid out again and
the whole flight is one interpolation. The catalogue spells it
`shared_element(name, node)`, and the customers section of the ERP demo is
one.

A name that resolves to nothing is the ordinary case and not an error — a
panel is built and torn down as it opens — which is also the one way to get
this wrong silently: a key spelt two ways is a page where nothing moves and
nothing complains. `EUI_TRACE=1` prints a line per pair, resolved or not, and
says why.

**Lengths.** `120` is pixels (0–65535), `"auto"`, `"50%"`, `"1fr"`, or
`"sp:4"` for a space-scale index.

**Edges.** `2` for all four, `[y, x]` for two, `[t, r, b, l]` for four. The
numbers are **space indices**, not pixels: the viewer's density setting scales
them.

**Colours.** A role name, a literal `"#RRGGBB"` / `"#RRGGBBAA"`, or `"none"`.
Thirty-three roles exist, and the client resolves them against the viewer's
mode — which is why the same view is right in dark mode without the server
learning the viewer went there.

```
surface.base   surface.raised  surface.sunken  surface.overlay
text.default   text.muted      text.inverted   text.disabled
accent.base    accent.hover    accent.active   accent.on
success.base   success.subtle  success.on
warning.base   warning.subtle  warning.on
danger.base    danger.subtle   danger.on
info.base      info.subtle     info.on
border.subtle  border.default  border.strong   focus.ring
series.1       series.2        series.3        series.4        series.5
```

`series.1`–`series.5` are the chart palette, in fixed order and never cycled —
which is why `chart_role(i)` numbers them rather than hashing a name: a sixth
series is a chart that wanted to be two.

A literal is right for a brand mark or a chart series, and wrong for a
surface.

**Frosted glass.** `blur` makes a node show its backdrop — everything painted
under it — through a Gaussian, with `bg` composited over the result, so one
node is both the frost and the tint:

```
{"k": "overlay", "s": {"display": "stack", "justify": "center",
                       "align": "center", "blur": 16, "bg": "#00000073"}}
```

Every blurred node in a frame shares one backdrop: the frame as it stood
when the first of them was painted. Two frosted panels that overlap both
show what is under the pair, not one through the other.

## Events

`"on": {"click": "increment"}` names a **server** event: the client sends it,
the handler runs, the view re-renders, the diff comes back.

Twenty-four names, and an unknown one is refused:

```
click          double_click   long_press     context_menu
pointer_down   pointer_up     pointer_move   pointer_enter
pointer_leave  drag_start     drag_over      drop
key_down       key_up         text_input     change
focus          blur           submit         scroll
resize         window         ended          time_update
```

`window` is the windowed list's report of what it can see; `ended` and
`time_update` come from `audio` and `video`.

A handler can instead be a hash, and then it runs **locally first**:

```soli
"on": {"click": {
  "local": "state.count += 1; value.text = str(state.count)",
  "styles": {"hover": hover_record},
  "then": "increment"
}}
```

| Key | Meaning |
|---|---|
| `local` | A source string, or an assembly list (`spec/07`), compiled to a verified chunk |
| `styles` | The style records this chunk may point a node at, by name — `self.style = @hover` |
| `then` | A server event to send after the local part runs |

The local language assigns to `state.x`, to a keyed node's `.text` or
`.style`, does arithmetic and comparison, `if … else`, `emit("name")`, and
`self` for the node that carries the handler. The client verifies the chunk
before its first run and executes it on a fuel budget. Its effect is
**advisory**: authorisation is never local.

---

# The library

Two hundred and sixty functions, every one of them a function over the primitives.
Copy the file into your application and change it — that is the intended use.
A widget is not a protocol feature.

## Primitives and wrappers

| Signature | What it returns |
|---|---|
| `node(kind, style, children)` | The bare hash. Everything else is built on it. A `"tw"` key in `style` is read as Tailwind classes — see [Tailwind classes](/docs/tailwind) |
| `column(style, children)` | A box with `display: column` |
| `row(style, children)` | A box with `display: row` |
| `stack(style, children)` | A box with `display: stack` — children superimposed |
| `text(content, style)` | A text node |
| `icon(name, style)` | A named vector icon, stroked by the client in `fg`. The name is a **prop**, not text — so it is never shaped, never falls back to a symbols face, and never reaches a screen reader as the character it resembles. A name the client does not know draws nothing and keeps its space |
| `spacer()` | Empty space with `grow: 1` |
| `divider()` | A hairline rule |
| `scroll(style, children)` | A clipping viewport, laid out as a column |
| `list(style, item_height, children)` | A virtualised list: the client lays out only the visible rows |
| `list_window(style, item_height, count, heights, children, on_window)` | A **windowed** list — see below |
| `input(value, on_change, o = {})` | A bordered single-line field, `change` wired to `on_change`. `o` carries `style`, `props`, `key`, further `on` handlers, and `placeholder` — the hint the client draws in `text.muted` while the field is empty (03 §3), never the value |
| `button(label, on_click)` | The primary button: accent roles, local hover and press |
| `image(src, width, height)` | A picture from a file in the application |
| `avatar(src, size)` | A round picture |
| `canvas(width, height, paths)` | A drawing surface, `paths` as in *Charts* |
| `audio(src, props, on)` | A sound. Draws nothing |
| `video(src, props, style, on)` | A moving picture |
| `keyed(key, n)` | Sets `n["key"]`, and returns `n` |
| `with_state(state, root)` | Puts the state in the root node's props, where local handlers read it |
| `bp(width)` | Breakpoint name for a viewport width: `xs` · `sm` · `md` · `lg` · `xl` · `2xl` (Tailwind rungs) |
| `bp_min(width, name)` | `true` when `width` is at least that breakpoint |
| `bp_px(name)` | The pixel floor: `bp_px("md")` is `768` |

## Text

| Signature | What it returns |
|---|---|
| `h1(content)` | Size 6 (30 px), bold — `text-3xl font-bold` |
| `h2(content)` | Size 4 (20 px), semibold |
| `muted(content)` | Size 1 (14 px), `text.muted` |
| `text_interned(content, style)` | A text marked `intern` — for short strings that repeat across many nodes |

## Controls and states

Every interactive widget below is `control` plus a body. It exists because
twenty-four widgets in this library once answered the pointer with a cursor and
nothing else, and because not one of them could be disabled — the word did not
appear in the file.

| Signature | What it returns |
|---|---|
| `control(o)` | A keyed node: the size applied, the tone's resting colours, the caller's shape on top, the four pointer handlers wired to declared styles, the semantics in its props — and, when it is disabled or loading, no handler map at all |
| `stateful(base, tone, on)` | The four pointer handlers alone, merged onto an existing handler map. `tone` may also be a class string or a `tw()` result, whose `focus:` classes add a focus and a blur |
| `tone_resting(tone, lit)` | A tone's resting colours, with its selected patch folded in |
| `a11y_props(o)` | What a widget declares about itself, merged with the props its handler reads back |
| `control_metrics(size)` | `pad`, `gap` and `min_width` for `sm`, `md` or `lg` |
| `control_text_size(size)` | The text-scale index a label takes at that size |
| `control_px(size, density)` | A control height in px, for the places where a fixed box is the point |
| `icon_box_px(size)` / `checkbox_box_px(size)` | The square an icon button occupies, and the mark of a checkbox |
| `size_spec(size)` | The whole row of the size table |

`TONES` names seven — `accent`, `neutral`, `ghost`, `danger`, `quiet`, and the
underlined tab's `tab` and `tab_on`. Each is a
resting colour set plus a **hover and a press delta**. Deltas, not whole styles:
merged over whatever base the caller ended up with, they keep every geometry
choice inside all three states, so a size, a selection or a patch applied from
outside cannot go missing under the pointer. That is the invariant `restyle`
used to repair afterwards, made structural instead — and `restyle` stays, for
narrowing a widget from outside.

The tones are Tailwind UI's. `accent` is the filled primary with a small
shadow and no visible edge, hovering to the lighter `accent.hover`; `neutral` is
the white secondary inside a `border.default` hairline (Tailwind's `ring-1
ring-inset ring-gray-300`), hovering to the page grey; `danger` lightens by a
tenth of its opacity under the pointer, because the theme has no `danger.hover`.
Every tone reserves the one-pixel edge whether it paints it or not, so a primary
and a secondary side by side are the same height. `o["tw"]` on `control` is the
same shape in classes: its resting classes go over the tone, its `hover:` and
`active:` over the tone's deltas.

```soli
control({
  "key": "cb:" + props["id"].to_s,
  "tone": "quiet",
  "size": "md",
  "shape": {"justify": "start", "border": 0, "min_width": 0},
  "on": {"click": on_toggle},
  "props": props,
  "disabled": disabled,
  "a11y": {"role": "check_box", "checked": checked, "label": label},
  "c": [mark, text(label, {"size": control_text_size("md")})]
})
```

Precedence is `disabled` > `loading` > `read_only` > `active` > `hover` >
`selected` > resting. The first two are terminal: they replace the resting style
and take the handlers with them, so hover and press cannot be reached. Selection
is folded into the **resting** style before the hover delta is derived, which is
what stops a hover on an already-selected row looking broken.

### Disabling deletes the handler map

That single act is right in three places at once. No click reaches the server,
because dispatch walks up from the hit node and finds nothing on the path. The
node leaves the Tab order for free, because focus order is exactly the nodes
holding a click, key or editable handler. And the cursor and colours come from
one patch — `not_allowed`, `text.disabled`, `border.subtle`.

It is wrong in a fourth. With no click handler the client's accessibility
mapping sees no button to infer, and a disabled control decays into an unnamed
group: a screen reader reads its label as loose text with no hint that it is a
control, let alone an unavailable one. Which is why `disabled` is also a prop —
it has to be something the widget *says*, not something it stops doing.

### Two rules

**Never style focus from a `focus` handler.** Focus fires for a pointer click
too, so a server-side focus style lights the ring exactly when the client is
taking care not to. What a widget owes focus is to stay reachable and to have a
radius the ring can trace.

**Size is the application's; density is the viewer's.** `pad`, `gap` and
`margin` are space *indices*, and the client multiplies each by the viewer's
density before it lands in a frame. Resolving them to pixels on the server would
scale them twice. A data-dense application asks for `size: "sm"`; it does not
get to choose a density.

### What a widget may claim of the keyboard

Three props the client reads, for the things a server cannot do because it
does not own them — it does not own `Tab`, and until now it was never told
about `Escape`.

| Prop | Value | Means |
|---|---|---|
| `modal` | boolean | while it is laid out, the Tab order is **its subtree alone**; innermost wins, so a dialog over a dialog traps in the second |
| `autofocus` | boolean | focus starts here when the surface arrives — and is not reclaimed by a later batch |
| `keys` | list of key names | the keys this node wants, and it is sent no others |

`keys` is the one that matters most. A `key_down` handler used to receive
*every* key, so a widget had to choose between taking the arrows and keeping
`Enter` as the press it stands for — and a dialog listening for `Escape`
would hear every letter typed into the field inside it. Naming what you want
settles both. A node with a handler and no `keys` prop still hears
everything.

## Buttons

| Signature | What it returns |
|---|---|
| `button_variant(label, on_click, bg, fg)` | A `control` whose tone the fill picks — `accent.base` is `accent`, `surface.sunken` or `surface.raised` is `neutral`, `danger.base` is `danger`, no fill in the accent is `ghost`, anything else is that pair over `quiet`. One control tall (36 px), 14 px semibold label |
| `secondary_button(label, on_click)` | White inside a `border.default` hairline, `text.default`, shadow 1 |
| `danger_button(label, on_click)` | `danger.base` on `danger.on` |
| `ghost_button(label, on_click)` | No fill, accent text |
| `icon_button(glyph, on_click, props, o = {})` | A square from the size scale. `o["icon"]` names a vector icon to draw instead of the glyph; `o["name"]` is what it is *called* — a control whose only text is `×` is announced as `×` |
| `loading_button(label, on_click, key)` | Reveals a spinner and changes the label **locally** on press, then sends the event |
| `local_button(label, program, after)` | A primary button whose click runs `program` locally, then sends `after` |
| `theme_toggle()` | Light/dark, entirely on the client (`theme.toggle()`), no round trip and nothing told to the server |
| `split_button(label, on_click, o = {})` | The default and the rest: a primary that clicks, a caret that opens `o["items"]` through `context_menu`. One decision, one tab stop |
| `popconfirm(anchor, open, o = {})` | The question asked beside the control that raised it. `o["props"]` ride on the confirming button, because the question is always about a particular thing |
| `toggle_group(options, chosen, on_toggle, o = {})` | `segmented` asks which one; this asks which ones. `chosen` is a list and the server owns it |

Feedback that costs no network is the point of the variant engine: hover and
press switch between style records the session already holds.

## Input and forms

| Signature | Notes |
|---|---|
| `checkbox(label, checked, on_toggle, props, o = {})` | The mark's fill says its state; `props` come back as `params["props"]`. `o["indeterminate"]` draws the third state |
| `switch(label, on, on_toggle, props, o = {})` | A track and a knob, placed by `justify` |
| `radio(label, selected, on_pick, props, o = {})` | A ring with a dot in it. It cannot be unticked: the group owns the value |
| `radio_group(options, value, on_pick, o = {})` | The buttons, the `radio_group` role, and keys namespaced by `o["name"]` so two groups of Yes/No cannot restyle each other. `o["direction"]` is `"column"` unless `"row"` is asked for |
| `field(label, value, on_change, o = {})` | A label over an input; `o` goes to the input, `placeholder` included |
| `field_label(label)` | The label every field takes: 14 px medium in `text.default`, Tailwind's `text-sm font-medium text-gray-900` |
| `textarea(value, on_change, o = {})` | The multi-line field. `o["rows"]` is a floor, not a ceiling — it grows with what is typed into it. `o["placeholder"]` as for `input` |
| `text_link(label, on_click, props = {})` | Text in the accent colour that declares the `link` role. Not `link`: `breadcrumb` keeps a local of that name |
| `form(children, submit_label, on_submit)` | The children, then a right-aligned submit |
| `sized_input(value, on_change, width)` | An input of a fixed width |
| `select(options, value, open, on_toggle, on_pick)` | Closed, it is its anchor; open, a dropdown. **The server owns `open`**. A list too long for the panel scrolls inside it |
| `combobox(options, value, o = {})` | A select you type into. Closed, the value and a chevron; open, a field at the top of the panel filters the options. `combo_filter` is the narrowing; `o` carries `query`, `open`, `at`, `on_toggle`, `on_change`, `on_pick`, `on_key`, `on_submit` |
| `combo_filter(options, query)` | What of a fixed list still belongs under the draft |
| `select_option(label, selected, on_pick)` | One row of that dropdown, carrying `{"value": label}` |
| `multi_select(options, sel, open, on_toggle, on_pick, o = {})` | Several of something. The anchor carries a chip per chosen option — each chip's × sends the same `on_pick`, because removing one *is* toggling it off — and the panel's rows are the listbox's. Picking does **not** shut it: that is the caller's handler, not the widget |
| `tag_field(label, tags, draft, o = {})` | A line of chips you type into, and a panel of what is still worth choosing — the `combo_box`. Unlike `multi_select` the words are not a fixed set: whatever is typed becomes a tag. `o["key"]` is required; `o["suggest"]` is the narrowed list, `o["at"]` where the arrows are, `o["open"]` whether the panel is up, `o["take_focus"]` asks for the caret back for one batch. The entry claims `Backspace`, the arrows and `Escape` — not `Enter`, which arrives as a `submit` (03 §3.1) |
| `tag_option(word, lit, pos, total, o)` | One row of that panel. `lit` is where the arrows have walked to, which is not a selection — nothing is chosen until Enter or a click |
| `dropdown(anchor, content, open, max_px = 0)` | A panel positioned under its anchor; returns the anchor alone when closed. The content scrolls: the panel is as tall as its content, the window, or `max_px`, whichever is least |
| `slider(value, min, max, on_set, o = {})` | A track the client draws. The node declares `track: "x"` with `track_min`, `track_max`, `track_step` and `track_value` (03 §3.4); the client owns the press, the hold and the thumb every frame, and sends one `change` carrying the value per quantised step. `on_set` hears `change` and nothing else — no `pointer_move`, so nothing to gate and no `drag_only`. `o`: `width`, `step`, `label` |
| `rating(value, on_set, o = {})` | Stars, one control each — a `radio_group`, not one node with five meanings |
| `range_slider(low, high, min, max, on_set, o = {})` | Two handles on one track; `track_value` carries the pair and the fill runs between them. Each handle is its own `Tab` stop and its own `slider` to a reader, bounded by the one beside it, so they meet without ever swapping. One `change` carries both ends. `o`: `width`, `step`, `key`, `low_label`, `high_label` |
| `currency_field(label, value, on_change, o = {})` | The unit inside the border, so it reads as part of the value. What is typed is what is sent — grouping a number under a live caret moves the caret |
| `kbd(key)` / `shortcut_sheet(groups, on_close, o = {})` | What the application claims of the keyboard (§3.1), on one screen |

## Files

A picker is three things at once and a node with two of them opens nothing,
silently (03 §3.2, and 08 §3 on why there is no diagnostic): the `pick` prop,
a **server** handler for `file_pick`, and a capability the person granted.
`file_field` writes the first two; the third is the component's own
`eui_capabilities("fs.pick", ...)`, and `camera` and `microphone` are separate
grants because taking a photograph, making a recording and reading a folder
are three different powers.

What `file_pick` receives is `[id, name, size]` — a name and a weight, never a
path. The bytes arrive afterwards, as the server's own `file_upload` event,
carrying `{upload, name, content_type, size, path, error}` where `path` is in
the session's spool and dies with the socket. **A file is not an asset**
(01 §6): keeping one is the application's decision, and `uploaded_file_at(path,
name)` turns that path into the hash Soli's uploaders take, so the bytes go
wherever `uploader("file", {"service": ...})` says — SoliDB, a disk, S3 —
rather than under `public/`. Reading one back is `read_upload(Model, field,
blob_id)`, and `eui_asset(bytes)` names those bytes for a `src`. Atrium
(`chat_controller.sl`) is the worked example.

| Signature | Notes |
|---|---|
| `file_field(label, accept, on_pick, o = {})` | A `control` that opens the platform's open dialog. `accept` is extensions without dots (`"png,jpg"`), empty for any file. `o["flags"]` takes `PICK_CAMERA` or `PICK_MICROPHONE`; `o["multiple"]` takes more than one file, which the capture flags ignore; `o["max"]` is the largest one file may be, in bytes, defaulting to the client's 16 MiB. `o["hover"]`/`o["press"]` override the tone's, for a glyph that lights rather than a surface that fills |
| `file_drop(label, accept, on_pick, o = {})` | A surface a file can be let go over. `drop` is the same prop as `pick`, so a drop is a `file_pick`. `o["on_drag"]` is `file_drag` (`[over]`); `o["over"]` lights the box. `o["pick"]` (default true) also opens the dialog on a click |
| `attachment_card(name, note, o = {})` | What was attached. `o["src"]` draws a small square of it — an asset from `eui_asset(bytes)`, or a path; `o["badge"]` draws a node there instead, usually the extension; neither draws just the name and the note, which is what a file whose bytes have gone gets. Resolve the bytes *before* choosing: a `src` naming nothing is a view that cannot be encoded, and that ends the session (01 §4) |
| `pick_prop(accept, o)` | The `pick` value itself, for a node built by hand — a string when nothing else is asked for, `[accept, flags, max]` otherwise |

None of them keeps state. The handler does; the widget draws what it is told
and carries the identity the handler will need.


## Typed fields

`field` is a label over an input, which is all a text field ever needed. A
*typed* field is the same three parts — a label, a control, and a line
underneath — with the type choosing the control and judging what ends up in
it.

The client has no types. An `input` is an `input`, and 03 §3 is not growing an
`email` kind so that a phone can pick a keyboard; the type lives on the
server, which is where the value was going anyway. What that costs is the
keyboard hint. What it buys is that "valid" means whatever this application
means by it, in Soli, beside the handler that stores the value — and that a
field can be told it is wrong by something no client-side type could know,
like a mailer that bounced.

A value is judged when it arrives, and `change` arrives on blur or on Enter
(06 §2), never per keystroke, so a field is never red while it is still being
typed into. An empty required field is not wrong yet either: `o["submitted"]`
is what says the person has had their turn.

| Signature | Notes |
|---|---|
| `text_field(label, value, on_change, o = {})` | A line of anything. It judges nothing on its own. `o["placeholder"]` is the example inside the empty box, `o["hint"]` the sentence under it — every `*_field` takes both |
| `password_field(label, value, on_change, o = {})` | The same, with `secret: true` so the client paints marks. `o["shown"]` reveals the text; `o["on_reveal"]` is Show/Hide. The value on the wire is still what was typed |
| `otp_field(label, value, o = {})` | Six boxes, one code. The value is a prefix; the live cell is the next empty one. `o["digits"]` (default 6), `o["numeric"]` (default true), `o["secret"]` for a PIN. `o["on_input"]` hears `text_input`, `change`, `Backspace` and a click that jumps. `otp_take` / `otp_pop` / `otp_apply` are the algebra |
| `email_field(label, value, on_change, o = {})` | One local part, one `@`, a domain with a dot in it, no spaces. Everything a field can honestly check — the only test of an address is a message sent to it |
| `number_field(label, value, on_change, o = {})` | `o["min"]`, `o["max"]` and `o["step"]` are the bounds and the stride; `o["on_step"]` adds − and + buttons that send the direction in `params["props"]["delta"]`. The handler does the arithmetic — `number_stepped` is it — because the value is the server's |
| `textarea_field(label, value, on_change, o = {})` | The multi-line one; `o["rows"]` is the floor the empty box keeps |
| `date_field(o)` | An anchor and a calendar in a `dropdown`. One options hash, not eleven arguments |
| `datetime_field(o)` | The same, plus `time_select` under the month |
| `date_range_field(o)` | Two ends on one calendar. It stays open until it has both |

Every one of them takes the same options: `hint`, `error` (the server's own
verdict, which wins), `required`, `submitted`, `invalid`, `complaint`,
`width`, `name`. `label`, `description`, `required` and `invalid` go out as
props, so a wrong value is *announced* as wrong and not only painted that way.

The floating three take their state in that hash: `value` (or `start` and
`finish`), `time`, `month`, `open`, and the handler names `on_toggle`,
`on_pick`, `on_nav` — and for a datetime `on_hour_toggle`, `on_min_toggle`,
`on_hour`, `on_min`. The server owns `open`, exactly as it owns a select's;
what a picked day does to it is the handler's business.

The panel is built by a thunk and only when it is down, so a closed picker
does not pay for a month of day cells on every render.

| Signature | Notes |
|---|---|
| `email_valid?(value)` / `number_valid?(value)` / `iso_day?(value)` | What the fields judge by, on their own |
| `number_within?(value, min, max)` | A number, and inside the bounds the caller gave |
| `number_stepped(value, delta, o)` | What − and + mean, clamped to `o["min"]`/`o["max"]` |
| `field_shell(label, control, o)` / `field_error(…)` / `field_style(…)` / `field_props(…)` | The parts, for a field of your own |

## Markdown

A document, both ways: markdown as a tree of nodes, and an editor that writes
one. It is the fifth catalogue file, `eui_builders_markdown.sl`.

Two rules of the protocol decide the shape of the editor, and no amount of
work gets around either. A `text` node carries one family, one size and one
weight for its whole run (02 §3), so a field cannot show a bold word inside a
sentence. And the client owns the caret and the selection and reports
neither (03 §3, 08 §7.1), so no button here can wrap "what is selected".
Between them they rule out a WYSIWYG of the kind a browser has.

What is left is a **block editor**, and 03 §3.1 rule 3 is what makes it
work: a key in a position where it does nothing — `Backspace` with nothing
before the caret, an arrow at the end of the text — is never the client's,
and is reported if `keys` asked for it.

| Gesture | Mechanism |
|---|---|
| `Enter` starts a block | `change` then `submit` on an `input` (03 §3.1 rule 2) |
| `Backspace` at the head joins | nothing for the client to do, so it is reported |
| The arrows change block | never a single-line field's, so they are reported |
| The caret lands where the server put it | the `focus_to` prop, which is a `Focus` op |

So a block is an `input` of an explicit pixel width — it wraps and grows,
because an editable node is measured by the same shaper a `text` is. What is
lost against one big `textarea` is that `Enter` cannot split a block at the
caret, because there is no caret to split at: it adds an empty block after
the one being typed in, which is where `Enter` is pressed nearly every time.
A code block is a `textarea`, where a newline is the whole point.

The marks stay visible while a block is being edited — `**bold**` reads as
`**bold**` — and become bold in the preview and everywhere the document is
read. That is the compromise, and it is structural rather than unfinished:
`B` and `I` mark the **block** and unmark it when pressed again, because a
button that promised anything else would be lying about what a server can
know.

| Signature | Notes |
|---|---|
| `markdown(source, opts)` | A whole document as one column. `opts["gap"]` is the space between blocks, `opts["width"]` the measure a picture is fitted to, `opts["em_font"]` a face for `*emphasis*` |
| `markdown_file(path, opts)` | The same, read from disk and parsed once — cached on the path *and* the measure, because between them they are the whole input |
| `md_blocks(source, width)` | The blocks on their own |
| `md_doc_rows(path, width)` | A long document as rows for a windowed `list` (04 §7.1), with a height guessed for each, so a handbook costs the client one window of blocks rather than the handbook |
| `markdown_editor(doc, o)` | The editor. `o`: `width` (required, in pixels), `key`, `placeholder`, `accept`, `max`, `files`, and one handler name each for `on_change`, `on_submit`, `on_key`, `on_focus`, `on_tool`, `on_pick`, `on_drag`, `on_drop`, `on_preview`. It holds no state |
| `markdown_editor_bar(doc, o)` | The toolbar on its own, for a caller that wants it **outside** the scroller. That is the only way a bar stays put while a long document moves: sticky positioning is not in version 1 (04 §9) and no scroll offset reaches the server to move one with (06 §8). Pass `bar: false` to `markdown_editor` and draw this above the `scroll` holding it; both halves read the same `doc` |
| `markdown_editor_step(doc, what, params)` | One handler for all of it. `what` is what happened — `change`, `submit`, `key`, `focus`, `tool`, `pick`, `upload`, `drag`, `drop`, `preview` — so an application may name its events anything and route them in a line each |
| `md_edit_parse(source)` / `md_edit_source(blocks)` | Markdown in, blocks out, and back. Thirteen kinds: `p`, `h1`–`h3`, `quote`, `bullet`, `number`, `task`, `code`, `rule`, `image`, `file` and `table`. One line is one block: a paragraph in an editor is a thing you put a caret in, not a run of lines the renderer will join. An empty document is still one empty paragraph, because an editor with nothing to put the caret in is one nobody can start typing in |
| `md_edit_set(blocks, id, said, next_id)` | What `change` does. A marker typed at the head of a block becomes that kind and comes off the text; a value with newlines can only be a paste, and becomes one block a line |
| `md_edit_split(blocks, id, next_id)` | `Enter`. A list carries on; an empty list item leaves the list instead of making another |
| `md_edit_merge(blocks, id)` | `Backspace` at the head. A marked block loses its marker first, then joins what is above; a picture above is removed rather than merged into, which is the only way one is deleted with the keyboard |
| `md_edit_kind(blocks, id, kind)` | The toolbar. Pressing the kind a block already is puts it back to a paragraph |
| `md_edit_wrap(blocks, id, mark)` | `B` and `I`, round the whole block, and off again. Any mark, not only the two the bar carries |
| `md_edit_put(blocks, id, one)` / `md_edit_drop(blocks, id)` | A block in after the one the caret was in, and out again. Dropping the last leaves an empty paragraph |
| `md_edit_step(blocks, id, delta)` / `md_edit_at` / `md_edit_index` / `md_edit_fresh` | Where the caret goes, what is there, and an id nothing is using |
| `md_edit_events(prefix)` / `md_edit_mine?(prefix, event)` / `md_edit_what(prefix, event)` | Seventeen gestures is seventeen handler names, and writing them out twice — once in the options, once in the reducer — is thirty-four places for a typo that fails silently (08 §3). They come from one prefix instead, and the reducer is one line: `step(doc, md_edit_what("note", event), params) if md_edit_mine?("note", event)` |
| `md_edit_check(blocks, id)` | Ticks a task and only a task. `- [ ]` / `- [x]` are markdown's own; the marker is a real `checkbox`, because a tick you can press is the difference between a note with a list in it and a note with a list you keep |
| `md_edit_table(id)` / `md_edit_cell(blocks, id, row, col, said)` / `md_edit_row_add` / `md_edit_col_add` | A table, and the four things done to one. Every cell is its own `input` carrying where it is, so one handler serves a table of any size and `Tab` walks them in reading order for free. A ragged table is not an error in markdown and is not one here |
| `md_edit_shift(blocks, id, delta)` / `md_edit_move_to(blocks, id, slot)` | One place up or down (`Alt` and an arrow), and anywhere (a drop). A `drop` reports the slot with the block still in the document (06 §6), so the correction lives in `md_edit_move_to` and not in every caller. A shift at either end is a no-op, never a wrap |
| `md_edit_remember(doc, why, id, cap)` / `md_edit_undo(doc)` / `md_edit_redo(doc)` | A stack of block lists. Typing is coalesced and has to be: `change` arrives when a field goes quiet (06 §2), so an uncoalesced stack would walk back through a sentence a breath at a time. `Ctrl+Z` reaches it because the block *claims* `z` — and a printable character cannot be withheld (03 §3.1 rule 1), so the key report arrives *and* the letter still types, which is why the modifiers are checked |
| `md_edit_tools()` / `md_edit_slash_hits(said)` | What the bar offers, and what is still worth offering under what was typed after a `/`. Typing `/` at the head of a block opens that list *where the caret is*, which is the gesture people reach for and the one a bar cannot be |
| `md_edit_attach(doc, payload)` | Keeping an upload the simple way: `slurp` then `eui_asset`, and the block names the bytes `eui-asset:<hex>` |
| `md_edit_wants?(doc, params)` | Whether this editor is the one that asked for this `file_upload` |
| `md_src(src)` / `md_fit(w, h, width)` | An address as a node's `src`, and a box of at most `width` that keeps the shape the picture has |

**A picture's size travels in markdown's own title field.** `![alt](src
"1600x1200")`, which every other reader shows as a tooltip and this one reads
as a measure. It has to come from somewhere: an `image` draws its texture
across whatever box it ends up with, so a photograph given the wrong box is
not cropped but squashed, and this process cannot decode a JPEG to ask —
the bytes are in the asset store and the picture is the client's to fetch
(01 §5). Whoever put the file there knew its size and writing it down costs
nothing; one that arrives without gets a modest box and sits in it.

**An `eui-asset:` address means something to a client talking to this server
and to nothing else.** An asset is addressed by its content and served from
`/_eui/asset/<hex>`; it is not a URL on the web, and a document that leaves
the application — a mail that is sent, a page that is published — has to turn
its assets into whatever that destination understands. `md_edit_attach` is
the default because it is the one thing that works with no infrastructure at
all, not because it is right everywhere. An application with somewhere better
to put the bytes handles `file_upload` itself and calls `md_edit_put` with
the block it built; the mail example does exactly that, writing each
attachment under `public/mail-att` and turning it into a real MIME part on
the way out.


## Calendar and pickers

One engine, three pickers. `month` is `"YYYY-MM"`, days are ISO
`"YYYY-MM-DD"` strings — which compare correctly as strings, so no date
arithmetic reaches the view.

| Signature | Notes |
|---|---|
| `calendar(month, selected, range_start, range_end, on_pick, on_nav)` | Navigation, weekday header, a seven-column grid. `selected` is a list of ISO days; a range shades between its ends |
| `date_picker(month, value, on_pick, on_nav)` | One day |
| `datetime_picker(month, date, time, hour_open, min_open, on_pick, on_nav, on_hour_toggle, on_min_toggle, on_hour, on_min)` | A day plus hour and minute selects (`HH:MM`, nothing else) |
| `time_select(time, hour_open, min_open, on_hour_toggle, on_min_toggle, on_hour, on_min)` | Those two selects on their own: twenty-four hours and sixty minutes, so there is no free text to parse and no "25:61" to reject |
| `date_range_picker(month, start, finish, on_pick, on_nav)` | Two ends on one calendar |
| `day_cell(iso, label, selected, in_range, on_pick)` | One day, carrying `{"date": iso}` |
| `day_blank()` | The gap before the first of the month |
| `weekday_header()` | Mo–Su |
| `month_label(month)` | `"September 2026"` |
| `month_shift(month, delta)` | The next or previous month |
| `weekday_index(day)` | Monday = 0 |
| `two_digits(n)` | `"07"` |
| `labelled(title, child)` | A titled card, so a picker reads as one thing |

## Structure and overlays

| Signature | Notes |
|---|---|
| `card(style, children)` | Raised surface, `border.subtle` hairline, radius 2 (8 px, `rounded-lg`), shadow 1, pad 6 — your `style` wins where it sets a key |
| `tabs(names, active, on_select, o = {})` | A row of labels, the active one in the accent over a 2 px accent rule; the others muted, drawing a grey rule under the pointer. Each carries `{"tab": name}` |
| `dialog(title, body_children, actions, opts = {})` | An overlay: dimmed ground, centred panel, actions right. Declares itself `modal` and `autofocus`, so the client traps `Tab` inside it and puts focus there when it opens, and claims `Escape` alone — `opts["on_close"]` is the event that key sends |
| `sheet(side, children, opts = {})` | A 384 px panel at the left or right edge, shadow 3, over a dimmed ground |
| `drawer(children, opts = {})` | `sheet("left", …)` |
| `popover(anchor, content, open)` | A panel over its anchor; the anchor alone when closed |
| `toolbar(children)` | A raised strip with a bottom rule |
| `accordion(sections, open_id, on_toggle)` | Sections of `{id, title, body}`, or `children` in place of `body`; the open one shows it |
| `stepper(steps, current)` | Numbered dots — done ones filled, the current one ringed |
| `menu(items, on_pick)` | A floating column — radius 3, shadow 3 — whose items wash grey under the pointer, locally; each carries `{"item": it}` |
| `context_menu(anchor, items, open, on_open, on_pick, o = {})` | Right-click opens `menu` over the anchor. The client already emits `context_menu` on button 1; `on_open` is that event. `o["on_close"]` is Escape |
| `command_palette(query, items, at, o = {})` | Overlay, a field, a list. `items` are `{id, label, hint, group}` or strings. `command_match` narrows them. Include it in the tree only while it is up |
| `command_match(items, query)` / `command_row(it)` | What of the palette still belongs under the draft, and the shape of one command |
| `tooltip(content)` | Inverted text on the default ink |
| `segmented(options, selected, on_select)` | One sunken row, the selected option raised |

## Split panes

| Signature | What it returns |
|---|---|
| `split_pane(o)` | Two panels and a divider that can be dragged. `dir` is `"row"` for a vertical divider with the panels side by side, `"column"` for a horizontal one with them stacked |
| `split_sizes(extent, fraction, min_a, min_b, bar)` | The two panel extents in px, clamped to both minimums |
| `split_at(extent, at, min_a, min_b, bar)` | A pointer position along the axis, as a fraction per mille |
| `tag_add(tags, text, o = {})` | Trim, reject empty, reject a word already there in any case, and stop at `o["max"]`. Copies before it grows, so the list handed in is left alone |
| `tag_remove(tags, at)` | Without the one at `at`; out of range removes nothing |
| `tag_suggest(all, tags, draft, limit)` | The words that match the draft anywhere, minus the ones already chosen, capped at `limit` |
| `tag_highlight(count, at, step)` | Where an arrow key lands, wrapping at both ends; `-1` in is "nowhere yet", and a highlight past a panel that shrank comes back inside |
| `split_span(extent, bar)` | The room the panels share, once the divider has taken its own |
| `split_event(state, params, name, dir, extent, min_a, min_b, bar)` | The four events a split sends, folded into a component's state |
| `pane_bp(px)` / `pane_min(px, name)` / `pane_px(name)` | Breakpoints at the scale a *panel* lives at |

```soli
split_pane({
  "key": "workspace",
  "dir": "row",
  "size": w, "cross": 400,
  "fraction": state["split"],
  "min_a": 160, "min_b": 240,
  "on_drag": "split",
  "dragging": state["split_drag"],
  "a": fn(px) { sidebar(px) },
  "b": fn(px) { detail(px) }
})
```

and one line in the handler:

```soli
"split" => split_event(state, params, "split", "row", w, 160, 240, 6)
```

**Where the handlers sit is the whole design.** `pointer_down` is on the
divider, so a drag can only start there. `pointer_move` and `pointer_up` are on
the **container** — and a pointer payload is measured against the node whose
handler catches it, so the number reaching the server is already the divider's
position inside its container, needing no arithmetic and no memory of where the
drag began. A press captures the pointer, so a move that leaves the divider
still arrives; moves are coalesced to one per frame rather than one per sample
the mouse sends.

The divider also holds `key_down`, which is what puts it in the Tab order: a
split can be moved with the arrow keys and recentred with `Home` by someone who
never touches a pointer.

`fraction` is per mille — an integer, so it survives state and props without
ever being a float. Both conversions round rather than truncate, which is what
makes the trip exact: a divider dropped on a pixel and rebuilt from its fraction
lands on that pixel, where truncating at both ends lost one on the way.

### Content that answers its panel

`a` and `b` are functions of one argument: the panel's own extent in pixels. A
panel is built knowing how much room it has, which is what lets its content
answer the panel instead of the window.

`bp()` is no use for this. Its rungs are Tailwind's and they are a *window's* —
a panel of 309 px and one of 505 px are both `xs`, so a view that branches on
`bp` inside a split never branches at all. `pane_bp` is the same idea at the
scale a panel actually lives at: 200, 320, 480, 720.

```soli
"b": fn(px) {
  cells = [stat("Rows", rows, ""), stat("Open", open, "")]
  pane_min(px, "lg") ? row({"gap": 3}, cells) : column({"gap": 3}, cells)
}
```

What this does not do yet is drag without a round trip. Every frame of a drag
reaches the server, because a local chunk cannot read the event that triggered
it. On loopback or a LAN that is invisible; over a long link it would not be.

## Navigation

| Signature | Notes |
|---|---|
| `navbar(brand, links, active, on_go)` | Brand and links; each carries `{"path": l}` |
| `sidebar(links, active, on_go)` | A 200 px rail, the active entry filled |
| `breadcrumb(crumbs, on_go)` | `{label, path}` crumbs; every one but the last is a link |
| `pagination(page, pages, on_page)` | ‹ and ›, each carrying the page it would go to |
| `tree_view(nodes, open_ids, on_toggle, depth)` | Recursive: `{id, label, children}`, indented by `depth` |

### Pages, and the stack they move through

A navigator is one page on screen, a stack in state, and the *client* owning
the movement between them. Nothing here names a duration or a direction to
come back by: the page says how it arrives, the client mirrors that for
whatever is leaving, and a pop is the same sentence read backwards.

| Signature | Notes |
|---|---|
| `nav_page(key, node, o)` | Restyles `node` into a page: keyed, `enter \| exit`, `o["motion"]` the way it comes in (default `trailing`), `"none"` for a cut |
| `navigator(state, pages, o)` | `pages` is a hash of **thunks**, so only what is on screen is built; two are rendered when there is one underneath, so an edge swipe reveals it |
| `nav_push(state, key)` · `nav_pop(state)` · `nav_top(state)` | The handler half: the stack, and which way it last moved |
| `back_button(after)` | `back()` locally and `after` on the server — the same event the platform's own gesture makes (06 §1.3) |
| `shared_element(name, node, o)` | One thing on two pages: give it to both, under the same name, and the arriving one flies out of the box the leaving one had (03 §5.3) |

## Data

| Signature | Notes |
|---|---|
| `table_header(labels, widths)` | A header row of fixed column widths, 14 px semibold in the default ink over a `border.default` rule |
| `table_row(key, values, widths, o = {})` | A keyed body row — keyed, so re-sorting **moves** rows. The first cell in the default ink, the rest muted, 12 px of room above and below and a local `surface.base` hover. `o["dense"]` is 4 px and no hover, for a virtualised list of thousands |
| `data_grid(columns, rows, selected, editing, sort, on_select, on_sort, on_change, on_key)` | Header outside the scroll, equal columns filling the width (50 % of two, 33 % of three), a selected cell, an `input` or select in the one being edited |
| `grid_header(columns, sort, on_sort)` | One labelled cell per column; the active sort is marked |
| `grid_row(record, columns, selected, editing, on_select, on_change, on_key)` | A keyed row of `grid_cell` |
| `grid_cell(row_id, col, value, selected, editing, open, on_select, on_change, on_key)` | A keyed box; the child is text, an `input`, or a compact select when the column has `options`. A column is editable unless `editable` is `false` |
| `grid_col_editable(col)` | `false` only when the column sets `editable: false` |
| `grid_col_align(col)` | `start`, `center` or `end` — default `start` |
| `grid_sort_rows(rows, col, dir)` | `sort_by` that column, reversed when `dir` is `"desc"` |
| `multi_select_list(items, sel, on_toggle, o = {})` | A list whose rows are tickable, with a tri-state select-all and a count. `items` are `{"id", "label"}`; `o["key"]` is required and prefixes every key inside; `o["row"]` is `fn(item, chosen)` for a row that is more than a label |
| `multi_select_window(make, count, window, sel, on_toggle, o = {})` | The same over a windowed list (04 §7.1). `make` is `fn(i)` for absolute row `i`; only the rows in `window` are built, `set_size` is the whole `count`, and `o["row_shape"]` pins the height `heights` promised |
| `selection_toggle(sel, id)` / `selection_count(sel, total)` / `selection_mark(sel, total)` | The model those two share — see **Multi-selection** below |
| `stat(label, value, hint)` | A card with one big number |
| `chip(label, on_remove, props)` | A grey `rounded-md` tag, with an optional × carrying `props` |
| `badge(label, tone)` | `tone` is a role family: `"info"` → `info.subtle` on `info.base`, 12 px medium, radius 1. `"neutral"` is the grey one |
| `progress(fraction)` | A bar; the fraction is clamped to 0–1 |
| `skeleton(width, height)` | A sunken placeholder |
| `code_block(code)` | Monospace on a sunken ground |

## Feedback

| Signature | Notes |
|---|---|
| `toast(message, tone)` | A white panel, radius 3 and shadow 3, the tone in an icon beside the words |
| `banner(message, tone, action_label, on_action)` | Full width on the tone's tint, a 4 px edge in its base, an action at the end |
| `spinner()` | An 18 px arc the **client** spins — `animation: "spin"`, no frames on the wire |
| `spinner_sized(size)` | The same, sized |
| `empty_state(title, body, action_label, on_action)` | Centred: a placeholder mark, a title, a line, one button |

## Picking things up

Spec 06 §6. The client owns the whole of the hand — how a press becomes a
grab, what is under it, which slot it is in, when a list should scroll because
the hand is at its edge — and tells the server three things: one `drag_start`,
one `drag_over` a boundary crossed, one `drop`. Everything that runs at the
speed of a pointer runs here; what comes back is a `MoveChild`, four bytes.

Two props declare it, and the split between them is the design: **the prop
says what a node *is*; the handler says who *hears*.** A card is draggable and
the column is what hears the drop, and those are two different nodes found by
two different walks.

| Signature | Notes |
|---|---|
| `draggable(key, group, style, children, opts)` | A thing that can be picked up. The key is **not** optional |
| `drag_grip(label)` | The grip: a press here grabs at once, with no slop and no hold |
| `drop_zone(group, style, on_over, on_drop, children, props)` | A thing that takes what others carry |
| `drag_slot(params)` | The slot an event carries; `-1` means the gesture was cancelled |
| `board_move(board, id, col, slot)` | Move an id to a column and a place, taking it out of wherever it was |
| `board_at(board, id)` | `[column, slot]`, so a cancelled drag goes back exactly where it was |

**There is no "reorder me" flag.** Reordering is the case where the item's own
parent is the target, so a container that `accepts` what its children `drag`
reorders itself *and* takes the same thing from elsewhere. One prop, two
behaviours, and no way for them to disagree.

**A draggable node must carry a key.** It is what the client holds it by: a
move between containers is a removal and an insertion, so the node is rebuilt
under the hand and its id changes. Only the key survives that. Keyed children
are also what make the server's answer a `MoveChild` rather than a rebuild, so
this costs nothing that was not already owed.

**The preview is the move itself.** On each `drag_over` the server puts the
item where the hand says and re-renders; the diff turns the permutation into
one `MoveChild`. There is no insertion line to invent, because the list *is*
the preview — and with nothing drawn over it, no hit test has to ignore one.

**A ghost is declared, not invented.** The thing under the cursor is an
`overlay` with `position: pointer`, which the client keeps under the hand with
no relayout, revealed by the `drag_start` handler's own local chunk so that it
is up before the server has answered. `erp_board_ghost` in the demo is the
whole of it — a style, a keyed label, and one line in the handler. Hidden is
`display: none` and not a transparency: the top layer is hit-tested first and
asks nothing about opacity, so a ghost left in the layout would answer every
`drag_over` meant for the column beneath it. `erp_board_event` in the demo is the whole
of a server's share — about thirty lines, and most of them are the cancel.

**Three ways in, and the client picks.** Eight pixels of travel for a mouse; a
grip or half a second of holding for a finger; `Space` then the arrows for a
keyboard. The server writes the same tree for
all of them and hears the same three events. A draggable node carries a *prop*
and not a `pointer_move` handler, which is exactly why a finger can still
scroll a list of them.


## Charts

A chart is a `canvas` with a `paths` prop. Each path is a list — kind, colour,
then numbers in logical pixels from the content box (`spec/03 §1.1`):

```
[0, colour, width, x0, y0, x1, y1, …]   polyline, round caps and joins
[1, colour, x, y, w, h, radius]         filled rectangle
[2, colour, base_y, x0, y0, x1, y1, …]  area between a polyline and base_y
[3, colour, cx, cy, r]                  filled circle
[4, colour, width, cx, cy, r, a0, a1]   arc, radians, clockwise from +x
```

| Signature | Notes |
|---|---|
| `chart_line(id, values, w, h)` | Grid, polyline, a dot per point |
| `chart_area(id, values, w, h)` | Grid, filled area, line on top |
| `chart_bar(id, values, w, h)` | Grid and bars, each an eighth of a slot apart |
| `chart_donut(id, parts, labels, w, h)` | One arc per part in the series roles, the total in the hole, a legend under it |
| `chart_candle(id, bars, w, h)` | A candlestick a session, `[open, high, low, close]`: a wick from high to low and a body from open to close, `success` up and `danger` down |
| `chart_gantt(id, tasks, w, h)` | A bar a task, `{"label", "start", "span"}`: the names in a gutter, the plot beside them, one band a row |
| `chart_points(values, w, h)` | The series scaled into `w × h` as `[x, y]` pairs |
| `chart_grid(w, h)` | Four hairlines to read a series against |
| `chart_max(values)` | The top of the scale, never 0 |
| `chart_spans(centres, w)` | The width of each hover band, from where the marks are |
| `chart_extent(bars)` | `[low, high]` over a candlestick series — a price scale is an extent, not a top |
| `chart_grid_v(w, h, divisions)` | Hairlines down rather than across, for an axis that is a time |
| `flatten_points(points)` | `[[x, y], …]` → `[x, y, …]` |
| `chart_role(i)` | The `i`-th series colour, in fixed order, never cycled |
| `chart_scale_ticks(low, high)` | The four readings the four hairlines stand for |
| `chart_y_axis(ticks, h, gutter)` | Those readings down the left of the plot |
| `chart_x_axis_bands(labels, spans)` | Categories centred under the bands they name |
| `chart_x_axis_points(labels, w)` | Categories spread between the ends, for marks that sit on the edges |
| `chart_value_axis(low, high, w, divisions)` | Readings along the bottom, for a chart whose rows are the categories |
| `chart_framed(ticks, x_axis, w, h, gutter, layers)` | The plot with both axes around it |

More than one series, where `sets` is a list of series and `names` names them:

| Signature | Notes |
|---|---|
| `chart_multi_line(id, sets, names, w, h)` | A line a series on one shared scale, markers ringed in the surface so crossings stay legible, legend under |
| `chart_grouped_bar(id, sets, names, labels, w, h)` | A bar a series within each category, 2 px of surface between neighbours |
| `chart_stacked_bar(id, sets, names, labels, w, h)` | Part-to-whole a category; the scale is the largest *total* |
| `chart_ranked_bar(id, items, w, h)` | `{"label", "value"}` sorted high to low, names in a gutter, one hue — the job is size, not identity |
| `chart_diverging_bar(id, items, w, h)` | `{"label", "value"}` signed about a zero rule, blue over and red under, the signed number beside every row |
| `chart_heatmap(id, grid, col_labels, row_labels, w, h)` | Boxes rather than a canvas: one role at varying opacity, so a cell hit-tests itself |
| `chart_dumbbell(id, items, w, h)` | `{"label", "from", "to"}` — two dots and the distance between them |
| `chart_sparkline(values, w, h)` | A bare line: no grid, no axis, no labels |
| `stat_spark(label, value, hint, values, w)` | `stat` with the shape of the last few periods under the number |
| `chart_legend(names)` | A swatch and a name a series, in the order the marks were drawn, centred under the plot |
| `chart_stream(sets, names, w, h, labels, mark, mark_name)` | The same shape as `chart_multi_line`, for a window that **moves**: no id and no hover bands, one marker a series rather than one a sample, whole-pixel coordinates, `mark` dashed across as a target, and the current reading in the legend |
| `chart_stream_ceiling(max, mark)` | The top of a live scale, rounded up to a step so the plot does not rescale on every tick |
| `chart_strip(values, ceiling, w, h)` | One measure as a row of cells, newest at the right: how often it was in the red, where a line chart answers what it was at 14:02 |
| `chart_band_role(pct)` | The four-step load ramp — idle, working, busy, over — as roles, because a hue computed from a reading would mint a colour a tick |

Every builder above takes `w` and `h` as the size of the **whole** chart —
axes included. Each works out its own gutter from how wide its readings print
and takes `chart_plot_h(h)` for the marks, so a caller sizes a cell and never
a plot. The ones whose x axis is a category (`chart_line`, `chart_area`,
`chart_bar`, `chart_multi_line`, `chart_candle`) take an optional list of
labels last; without it the marks are numbered.

`chart_stream` and `chart_strip` are the two that expect to be redrawn. They
are what Meridian's **Live** section is made of, and what makes them different
is not the drawing but what a redraw costs: every point in a moving window has
moved, so the whole `paths` prop goes again on each tick, and nothing in either
of them may derive a style or a colour from a reading — the session's tables
are define-once (02 §5), so a hue computed per value would mint a permanent
entry twice a second. Measured on that page with the dev bar, at one tick
every 500 ms: 7 511 B and 22 ops at tick 49, 10.2 ms of view and 0.7 ms of
encode, and `interned` reading **290/333/0/195 at tick 11 and 290/333/0/195 at
tick 49** — fifty redraws, not one new atom, style, colour or chunk. The
colour column is zero because every mark in both builders takes a theme role.


No chart library, no SVG, no client change: a chart is arithmetic in Soli and
a list of numbers on the wire.

### One series, and more than one

One series needs no legend: the title names it, and `chart_line`,
`chart_area`, `chart_bar`, `chart_ranked_bar` and `chart_sparkline` all draw
in `series.1` alone. Two or more is a different job — the reader has to tell
them apart — and every multi-series builder here ships a legend for that
reason, because identity is never allowed to rest on colour alone.

The scale is always shared. Two measures of different size go in two charts,
never in one with two y-axes: a second scale lets the author decide which line
looks higher, and that is not a decision a chart is allowed to make.

`chart_role(i)` hands out `series.1` … `series.5` in fixed order and does not
cycle. Past the fifth it returns the de-emphasis ink, because a sixth hue
generated to fill the gap is indistinguishable from one already in the set to
a reader with a colour vision deficiency — a tail belongs in a single "other",
in small multiples, or encoded as something that is not hue. The five are
`spec/05 §1.1`, and their separation is measured rather than judged.

A candlestick scales to the extent of its lows and highs rather than to a top,
because a price that moves three per cent about 400 is a flat smudge against a
zero baseline; a body that rounds to nothing is still drawn a pixel tall, so a
session that opened and closed at the same price is a line and not a gap. A
Gantt is the one chart here whose bands run across rather than down — a column
of washes and a column of bands, `chart_row_layers` rather than
`chart_layers` — and its names are ranged against the plot so a short name and
a long one both end where the bars begin.

### Answering the pointer

The client hit-tests boxes, not paths — a `canvas` is one node — so a chart
that answers the pointer is a `stack` of three layers: a row of wash columns
behind the drawing, the drawing, and a row of invisible bands in front. Each
band holds a value chip and two local handlers (spec 07 §1), which repoint two
nodes at styles the handler declared: its wash, and its own chip.

The chip is an **`overlay` with `position: pointer`**, and there is one a
chart rather than one a band — the band's handler writes the reading into it
and shows it. Two things make that work, and both are `04 §5`. An overlay
child of a `stack` is a popover, measured against the *window* and clipped by
no ancestor: a chip that was a plain box inside its band was measured against
the band, a band is about thirty pixels wide, and anything longer than a bare
number came back as four wrapped lines sitting on top of the marks it was
describing. And `pointer` says the **client** places it, above the cursor and
centred on it. It has to be the client: a local chunk has no access to the
pointer by design (`07 §1`), and asking the server for a position is a round
trip per mouse sample. Anchored to the band instead — and a band is the full
height of the plot — the chip sat at the foot of the chart wherever in the
column the hand actually was.

Down is `display: none`, not `opacity: 0`. The top layer is hit-tested first
and asks nothing about opacity, so a transparent chip left in the layout would
quietly swallow every hover that landed under it — the legend included. The
price is the fade, and it is worth paying.

```soli
"pointer_enter": {
  "local": "cw_line_3.style = @lit; tip_line.style = @shown; tt_line.text = \"Thu · 8\"",
  "styles": {"lit": …, "shown": …}
}
```

Nothing waits for the network, and nothing moves: what the new records change
is a colour and an opacity, which the client animates on its own (spec 03 §5)
while layout stays where it was. The `id` is what those keys are built from,
so it has to be an identifier and unique in the tree.

An arc is not a box, so the donut has no bands. Its legend rows are what the
pointer finds, and what they change is the reading in the hole — one
`set_text` for the value and its share, one for the name.

## The dev bar

There is no document to splice a dev bar into, so it is a widget the
application places itself — an `overlay`, pinned to the bottom of the window,
over every layer:

```soli
layers = layers.concat([dev_bar(eui_stats())])
stack({"gap": 0}, layers)
```

`eui_stats()` is a Soli builtin (feature `eui`). It answers with the
**previous** render's numbers — the work behind what is on screen — and with
an empty hash outside `--dev`, where `dev_bar` draws a `display: none` box.
A view can therefore compose it unconditionally and ship it.

| Figure | What it is |
|---|---|
| `event` | the event that caused the render |
| `view` | the Soli view function, in ms |
| `encode` | convert, diff and encode, in ms |
| `ops` | ops in the batch — green under 25, amber under 200, red above |
| `B` | bytes that went on the wire |
| `nodes` | nodes in the tree that was sent |
| `seq` / `renders` | the batch's sequence number, and renders this session |
| `interned` | atoms / styles / colours / chunks, interned once per session |

A click on a segmented control in the gallery reads `14 ops · 967 B` against a
928-node tree; the mount before it reads `509 ops · 31 872 B`. That ratio is
the protocol's whole argument, which is why these are the figures it shows.

Nothing the window knows is in there — frame time, quads, memory. Spec 08 says
the client reports nothing about the machine beyond its viewport, and a dev bar
is not a reason to change that; those numbers belong to a client-side overlay,
if one is ever wanted. The bar also takes the clicks that land on it: EUI has
no way to make a node transparent to the pointer, which is why it is one line
at the very bottom rather than a panel.

## Syntax highlighting

`code_spans(source, language)` turns source into the flat `[start, length,
colour]` triples a `text` node carries in its `spans` prop, so a whole file
stays one run and one node. The language is what a markdown fence says after
its backticks; `markdown` passes it through.

| Given | Coloured as |
|---|---|
| `soli`, `ruby`/`rb` | `def … end`, `#` comments, both quotes |
| `javascript`/`js`/`ts`/`jsx`/`tsx` | `//`, `/* … */`, template strings |
| `python`/`py` | `#`, `'…'`, `""" … """` docstrings |
| `rust`/`rs`, `go`, `c` (also C++, Java, C#, Kotlin, Swift, WGSL) | `//`, `/* … */` |
| `shell`/`sh`/`bash`/`zsh` | `#` |
| `sdbql`/`aql` | `FOR … IN … FILTER … RETURN`, `//` |
| `sql` (also `postgres`, `mysql`, `sqlite`) | `--`, keywords in either case |
| `json`, `yaml`/`yml`, `toml`, `html`/`xml`, `css`/`scss` | literals, `<!-- … -->`, `/* … */` |
| anything else | strings and numbers only |
| `text`, `md`, `diff` | nothing |

Soli is Ruby-shaped, so `ruby` is the Soli lexer with Ruby's own words added.
A comment or a string that runs past the end of its line carries to the next
one, which is the only state the lexer keeps.

The tokeniser is five answers wide — keyword, string, comment, number, the
rest — because that is what colour needs, and a lexer that small is worth
more than a parser that fails on the language it was not taught.

## Media

```soli
audio("public/sounds/chime.wav", {"playing": true, "volume": 80}, {"ended": "sound_ended"})

video(
  "public/video/pulse.gif",
  {"playing": on, "loop": false, "position": seek_ms},
  {"width": "100%", "max_width": 480, "height": 180, "radius": 2},
  {"time_update": "video_time", "ended": "video_ended"}
)
```

`src` is a path to a file in the application. The server hashes it; the client
fetches it once from `/_eui/asset/<hash>`, checks the bytes against the name,
and keeps it. Props: `playing`, `loop`, `volume` (0–100, audio), `position` in
milliseconds — the client **seeks when that number changes**, which is how a
scrubber works without a seek opcode. Events back: `time_update` and `ended`.

Nothing autoplays because nothing plays until `playing` is true, and a node
that is not in the tree is not a source: the feed removes the `audio` node
when the card stops, so the client holds a few sources, not a feed of them.

| Signature | Notes |
|---|---|
| `media_button(on, event, props)` | 28 px play/pause |
| `media_scrubber(width, at, duration, on_seek, props)` | A fixed-width bar; the click's x maps straight onto the position |
| `media_clock(ms)` | `"1:07"` |
| `vu_meter(level, o = {})` | A level meter the way a cassette deck had one: segments lit up to the reading, green until it gets loud, amber close, red past the line. `level` is one number or `[left, right]` — which is what a `level` event carries and what the front of a deck showed. `o`: `axis` (`"h"` default, `"v"`), `segments` (12), `peak` for the hold marker, `scale: false` to drop the legend, `label` |
| `vu_strip(level, o = {})` | One channel of it, if you are placing the strips yourself |
| `vu_scale(o = {})` | The dB legend a deck printed: `-20 -10 -6 -3 0 +3`. `0` is the line you are not supposed to cross, not the top of the scale |
| `vu_zone(i, n)` | Which of the three colours the `i`-th of `n` segments is. `success` / `warning` / `danger`, not `series.*`, which is categorical and must not be read as a ramp |

## Feed

The pieces that make a hundred thousand cards work. They are in the library
because virtualisation is a *composition* question, not a client feature.

| Signature | Notes |
|---|---|
| `post_card(post, liked, play, height)` | One card of a **fixed height** — which is what lets the list virtualise |
| `post_media(post, play)` | The card's picture, moving picture, or sound with controls |
| `post_action(glyph, count, on_click, props, active)` | One action under a post, carrying the post id |
| `initial_avatar(letter, tone, size)` | A letter in a coloured disc |

## Windowed lists

`list` virtualises what it has. `list_window` goes further: the server sends
only the rows in view, and the client knows the shape of the rest.

```soli
list_window({"grow": 1}, 128, count, feed_heights(count), cards, "window")
```

| Argument | Meaning |
|---|---|
| `count` | How many rows exist — thousands, of which `children` holds tens |
| `heights` | Every row's height, so the scroll extent is exact from the first frame |
| `children` | The rows in view. Each **must** carry its absolute index: `n["p"] = {"row": i}` |
| `on_window` | An event name; fires with `[first, last]` when what is in view changes |

The handler stores that window in its state, the view builds those rows, and
the client draws placeholders for rows it has not received. Two viewports of
margin and a settle delay keep a fast scroll from showing them.

## Multi-selection

A selection is one hash, and a flag reverses what its list means:

```soli
{"ids": ["AX-0003"], "all": false, "scope": "stock"}   # these are chosen
{"ids": ["AX-0003"], "all": true,  "scope": "stock"}   # all but these
```

The second reading is the whole point. Ten thousand rows selected is one
boolean, not ten thousand strings in the session state re-serialised on every
event — and because `selection_has?` *answers* for a row rather than storing
one, a row that scrolls into view an hour later arrives already ticked. An
application spends it the same way: `all` goes into the query as `NOT IN
(ids)`.

`selection_toggle` is one body with two meanings: adding to `ids` is choosing
under the first reading and excepting under the second. That symmetry is the
reason for the shape.

`scope` is what stops the flag lying. "All" is always relative to the query
that was on screen when it was clicked, so select every unpaid order, clear
the filter, and without a token "all" silently means every order there is.
`selection_scoped(sel, token)` hands back the selection, or an empty one when
the query has moved on.

Two things a windowed list must get right, and neither says so when it is
wrong. **Selection is keyed by row id, never by row index** — the `window`
event's payload is indices, which is exactly what makes the mistake look
natural, and a sort renumbers every row. And **a row must not change height
when it is ticked**: row tops come from `heights`, but a row that is present
is measured at its content size and drawn at that top, so a taller one creeps
over its neighbour and the scrollbar comes up short.

A row is one `control` with one click handler, and the tick inside it is
`check_mark` and not a `checkbox`. Its role is `option`, which 03 §6 rule 1
makes a leaf: a real checkbox in there would be dropped from the accessibility
tree while hit-testing still handed it the click. One handler per row is also
one Tab stop per row.

There is no shift-click. `click` carries `[x, y]` and no modifier bits (06 §1),
and `double_click` and `long_press` are in the enum but no client emits them —
so a range is expressible from the keyboard, where `key_down` carries the
modifiers, and not from the pointer.

## Patterns

**State lives in the handler.** No widget holds any. `select` is given `open`,
`accordion` is given `open_id`, `tabs` is given `active`. This is why they are
functions and not objects, and why any of them can be rendered twice on a page
without a name collision.

**Props carry identity.** A click arrives with `params["props"]`, so one
handler serves every row:

```soli
checkbox(item["title"], item["done"], "toggle", {"id": item["id"]})
```

**Keys are for the diff.** `keyed(id, node)` makes a child matched by key
instead of by position — the difference between `MoveChild` and rebuilding a
subtree. Reversing fifty keyed rows costs 201 bytes. `data_grid` is that
contract as a widget: a sort moves rows, a cell commit is `set_text`, and
the header sits outside the list because version 1 has no sticky.

**Interning is for repetition.** `text_interned` puts a short string in the
session's atom table so the wire carries it once. The table is append-only, so
a unique value would be a permanent entry — the server only interns strings of
24 bytes or fewer, and only when asked. Intern a glyph or a handle; never a
cell value.

**Identity is memoised.** Return the *same* hash for a keyed subtree that has
not changed and the server skips converting and diffing it entirely. That is
what the feed's card cache does, and why ten thousand cards diff in 44 ms.

**Responsive is a view branch.** There are no `sm:` style prefixes. Store
`params["viewport"]` on `connect` and `viewport`, then `bp_min(width, "md")`
chooses a different tree — a column instead of a row, fewer grid columns.
The client never runs a media query.

## Tailwind classes

`eui_builders_tw.sl`. A style written as a class string, in the vocabulary a
Tailwind page already uses; [Tailwind classes](/docs/tailwind) has the whole
table, the approximations and the refusals.

| Signature | What it returns |
|---|---|
| `tw(classes)` | `{"s", "hover", "press", "focus", "disabled", "props"}`: the resting style, the four states as deltas over it, and `grid-cols-N` as a prop. `classes` is a string or a list of them. A class with no equivalent raises, naming it |
| `tw_style(classes, disabled = false)` | Only the resting style — for a `text` node, whose style is a plain hash — with `disabled:` laid over it when asked |
| `tw_node(n)` | What `node()` does with a `"tw"` key: the classes under the keys written beside them, the props, and local handlers for the states |
| `tw_wire(base, t, on)` | Those handlers alone: pointer states when `hover:` or `active:` is present, a focus and a blur when `focus:` is |
| `tw_examples()` | One of every shape of class `tw()` accepts; the spec sends each through the encoder |

```soli
row({"tw": "items-center gap-4 px-4 py-4 border-b border-gray-200 hover:bg-gray-50"}, [
  text(person["name"], tw_style("text-sm font-semibold text-gray-900")),
  text(person["mail"], tw_style("text-xs text-gray-500 truncate"))
])
```

## Writing your own

There is nothing to register:

```soli
def price_tag(amount, currency, tone)
  row({"gap": 1, "align": "baseline"}, [
    text(amount, {"size": 4, "weight": "bold", "fg": tone + ".base"}),
    text(currency, {"size": 1, "fg": "text.muted"})
  ])
end
```

That is a component. It composes from primitives, so it needs no client
release and no protocol version; it takes its colours from roles, so it
follows the viewer into dark mode; and it holds no state, so the handler stays
the only place where anything is decided.

## The second catalogue

What the first pass had no room for. Each of these is in Meridian: the small
ones in "More catalogue" on the dashboard, the three charts under Reports.

| Signature | What it is for |
|---|---|
| `avatar_group(people, o = {})` | Roughly who, in the space of one and a half faces. `margin` is `[u8; 4]` in the protocol, so there is no negative margin to overlap with — every face but the last gets a slot narrower than it is, and spills over the next |
| `timeline(entries, o = {})` | What happened, in order. The rail is continuous and the dots are on it, so it reads as one thread; the last entry draws no line, because a thread that continues says more is coming |
| `expandable_row(key, values, widths, open, on_toggle, detail, o = {})` | A row that opens onto what is inside it, in the flow, under the row — not in a dialog that covers the rows you wanted to compare against |
| `tree_table(labels, widths, rows, open_ids, on_toggle, depth = 0, o = {})` | `tree_view` is a rail and `data_grid` is flat; this is the hierarchy that also has columns. Indentation lives in the first cell, so the numbers stay in theirs however deep it goes |
| `diff_view(lines, o = {})` / `diff_tally(lines)` | A patch as one column. Unified and not side by side: the removed line belongs directly above the line that replaced it. The sign carries the meaning and the tint repeats it |
| `filter_builder(tree, o = {})` | A question the author never wrote down. The condition is a tree the server owns; every control sends the dotted `path` of the item it belongs to, and `filter_at`, `filter_edit` and `filter_drop` walk it |
| `chart_funnel(id, stages, w, h)` | A ranked bar chart that gave up its baseline, so the eye reads the narrowing. The drop is put beside the lower stage, where it is wanted |
| `chart_waterfall(id, steps, w, h)` | Every bar starts where the last ended, so the gap between opening and closing is visible rather than asserted. A step marked `{"total": true}` stands on the floor |
| `chart_box(id, groups, w, h)` | Five numbers: the middle half as a box, the median as a line in it — not the mean, which one bad day drags — and the ends as whiskers |

A `select` now takes `o["key"]` and `o["props"]`. Without them its key is the
event name, so a second select answering the same event silently restyles the
first (§7), and neither the toggle nor the pick can say which row it came
from — which is exactly what a list of rows with a select in it has to say.
