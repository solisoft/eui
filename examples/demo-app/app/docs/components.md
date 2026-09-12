# Components

> Every function on this page exists. The library is
> `examples/demo-app/app/controllers/eui_builders.sl` — 260 functions, all of
> them plain Soli, none of them native, and what `soli new <app> --eui`
> writes into a new application — and the server that reads what they
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

### The sixteen kinds

Fourteen primitives, plus the two the media work added. The set is closed:
adding one is a protocol version bump.

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
| `font` | `sans` · `mono` |
| `size` | A text-scale index, 0–255 |
| `weight` | `regular` · `medium` · `semibold` · `bold` |
| `text_align` | `start` · `center` · `end` · `justify` |
| `clamp` | Maximum lines, then ellipsis |
| `underline`, `strike` | `true` / `false` |
| `overflow` | `visible` · `clip` · `scroll` |
| `transition` | `none` · `fast` · `base` · `slow` |
| `animation` | `none` · `spin` |
| `position` | `flow` · `absolute` |
| `cursor` | `default` · `pointer` · `text` · `grab` · `grabbing` · `resize_h` · `resize_v` · `wait` · `not_allowed` |

**Lengths.** `120` is pixels (0–65535), `"auto"`, `"50%"`, `"1fr"`, or
`"sp:4"` for a space-scale index.

**Edges.** `2` for all four, `[y, x]` for two, `[t, r, b, l]` for four. The
numbers are **space indices**, not pixels: the viewer's density setting scales
them.

**Colours.** A role name, a literal `"#RRGGBB"` / `"#RRGGBBAA"`, or `"none"`.
Twenty-eight roles exist, and the client resolves them against the viewer's
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
```

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
| `node(kind, style, children)` | The bare hash. Everything else is built on it |
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
| `input(value, on_change, o = {})` | A bordered single-line field, `change` wired to `on_change`. `o` carries `style`, `props`, `key` and further `on` handlers |
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
| `h1(content)` | Size 5, bold |
| `h2(content)` | Size 4, semibold |
| `muted(content)` | Size 1, `text.muted` |
| `text_interned(content, style)` | A text marked `intern` — for short strings that repeat across many nodes |

## Controls and states

Every interactive widget below is `control` plus a body. It exists because
twenty-four widgets in this library once answered the pointer with a cursor and
nothing else, and because not one of them could be disabled — the word did not
appear in the file.

| Signature | What it returns |
|---|---|
| `control(o)` | A keyed node: the size applied, the tone's resting colours, the caller's shape on top, the four pointer handlers wired to declared styles, the semantics in its props — and, when it is disabled or loading, no handler map at all |
| `stateful(base, tone, on)` | The four pointer handlers alone, merged onto an existing handler map |
| `tone_resting(tone, lit)` | A tone's resting colours, with its selected patch folded in |
| `a11y_props(o)` | What a widget declares about itself, merged with the props its handler reads back |
| `control_metrics(size)` | `pad`, `gap` and `min_width` for `sm`, `md` or `lg` |
| `control_text_size(size)` | The text-scale index a label takes at that size |
| `control_px(size, density)` | A control height in px, for the places where a fixed box is the point |
| `icon_box_px(size)` / `checkbox_box_px(size)` | The square an icon button occupies, and the mark of a checkbox |
| `size_spec(size)` | The whole row of the size table |

### Hover belongs to the deepest node

The client hovers whatever is **deepest** under the pointer and sends
`pointer_leave` to what it was over before `pointer_enter` to what it is over
now. So moving from a row onto a button *inside that row* is a genuine
**leave of the row**, and a row whose `leave` hides something — a toolbar
revealed on hover, say — hides it exactly as you reach for it, then shows it
again the moment the pointer lands back on the row. That is a flicker, and no
amount of care in the row alone fixes it: the row really is being left.

The rule that works is that every hoverable node *inside* the row keeps the
row's state up itself. `leave` then `enter` are applied in that order, so
every crossing nets out right:

| Crossing | Events | Result |
|---|---|---|
| row body → a tool | row leave hides, tool enter shows | shown |
| tool → another tool | tool leave hides, tool enter shows | shown |
| tool → row body | tool leave hides, row enter shows | shown |
| tool → elsewhere | tool leave hides, nothing shows | hidden |
| row body → elsewhere | row leave hides, nothing shows | hidden |

A local chunk may name any **keyed** node, not only `self` — `self.style =
@hover; tools_7.style = @shown` — and chunk source is lexed as identifiers, so
a key that reaches one may hold no `:`, `/` or `-`.

Two more things worth knowing before you hide something on hover. `opacity`
is per node and **does not reach children**, so setting it on a container
leaves every glyph inside it visible; `fg` *is* inherited by any child that
sets none, which is why a toolbar hidden this way declines to set its own.
And a node that is hidden by colour rather than by `display` stays laid out —
which is what you want, since a node appearing on hover would reflow the text
beside it.

`TONES` names five — `accent`, `neutral`, `ghost`, `danger`, `quiet`. Each is a
resting colour set plus a **hover and a press delta**. Deltas, not whole styles:
merged over whatever base the caller ended up with, they keep every geometry
choice inside all three states, so a size, a selection or a patch applied from
outside cannot go missing under the pointer. That is the invariant `restyle`
used to repair afterwards, made structural instead — and `restyle` stays, for
narrowing a widget from outside.

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
| `button_variant(label, on_click, bg, fg)` | The engine: a keyed box with `click`, and local `pointer_enter/leave/down/up` that repoint it at declared styles |
| `secondary_button(label, on_click)` | `surface.sunken` on `text.default` |
| `danger_button(label, on_click)` | `danger.base` on `danger.on` |
| `ghost_button(label, on_click)` | No fill, accent text |
| `icon_button(glyph, on_click, props, o = {})` | A square from the size scale. `o["icon"]` names a vector icon to draw instead of the glyph; `o["name"]` is what it is *called* — a control whose only text is `×` is announced as `×` |
| `loading_button(label, on_click, key)` | Reveals a spinner and changes the label **locally** on press, then sends the event |
| `local_button(label, program, after)` | A primary button whose click runs `program` locally, then sends `after` |
| `theme_toggle()` | Light/dark, entirely on the client (`theme.toggle()`), no round trip and nothing told to the server |

Feedback that costs no network is the point of the variant engine: hover and
press switch between style records the session already holds.

## Input and forms

| Signature | Notes |
|---|---|
| `checkbox(label, checked, on_toggle, props, o = {})` | The mark's fill says its state; `props` come back as `params["props"]`. `o["indeterminate"]` draws the third state |
| `switch(label, on, on_toggle, props, o = {})` | A track and a knob, placed by `justify` |
| `radio(label, selected, on_pick, props, o = {})` | A ring with a dot in it. It cannot be unticked: the group owns the value |
| `radio_group(options, value, on_pick, o = {})` | The buttons, the `radio_group` role, and keys namespaced by `o["name"]` so two groups of Yes/No cannot restyle each other. `o["direction"]` is `"column"` unless `"row"` is asked for |
| `field(label, value, on_change)` | A muted label over an input |
| `textarea(value, on_change, o = {})` | The multi-line field. `o["rows"]` is a floor, not a ceiling — it grows with what is typed into it |
| `text_link(label, on_click, props = {})` | Text in the accent colour that declares the `link` role. Not `link`: `breadcrumb` keeps a local of that name |
| `form(children, submit_label, on_submit)` | The children, then a right-aligned submit |
| `sized_input(value, on_change, width)` | An input of a fixed width |
| `select(options, value, open, on_toggle, on_pick)` | Closed, it is its anchor; open, a dropdown. **The server owns `open`**. A list too long for the panel scrolls inside it |
| `select_option(label, selected, on_pick)` | One row of that dropdown, carrying `{"value": label}` |
| `multi_select(options, sel, open, on_toggle, on_pick, o = {})` | Several of something. The anchor carries a chip per chosen option — each chip's × sends the same `on_pick`, because removing one *is* toggling it off — and the panel's rows are the listbox's. Picking does **not** shut it: that is the caller's handler, not the widget |
| `dropdown(anchor, content, open, max_px = 0)` | A panel positioned under its anchor; returns the anchor alone when closed. The content scrolls: the panel is as tall as its content, the window, or `max_px`, whichever is least |
| `slider(value, min, max, on_set)` | A 240 px track. Press, drag, or click; arrows nudge once focused. `on_set` receives `params["kind"]` (`"click"`, `"pointer_down"`, `"pointer_move"`, `"pointer_up"`, `"key_down"`) |

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
| `text_field(label, value, on_change, o = {})` | A line of anything. It judges nothing on its own |
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
| `card(style, children)` | Raised surface, subtle border, radius 3, shadow 1 — your `style` wins where it sets a key |
| `tabs(names, active, on_select, o = {})` | A row of labels, the active one underlined; each carries `{"tab": name}` |
| `dialog(title, body_children, actions, opts = {})` | An overlay: dimmed ground, centred panel, actions right. Declares itself `modal` and `autofocus`, so the client traps `Tab` inside it and puts focus there when it opens, and claims `Escape` alone — `opts["on_close"]` is the event that key sends |
| `sheet(side, children, opts = {})` | A 320 px panel at the left or right edge, over a dimmed ground |
| `drawer(children, opts = {})` | `sheet("left", …)` |
| `popover(anchor, content, open)` | A panel over its anchor; the anchor alone when closed |
| `toolbar(children)` | A raised strip with a bottom rule |
| `accordion(sections, open_id, on_toggle)` | Sections of `{id, title, body}`; the open one shows its body |
| `stepper(steps, current)` | Numbered dots — done ones filled, the current one ringed |
| `menu(items, on_pick)` | A raised column; each item carries `{"item": it}` |
| `tooltip(content)` | Inverted text on the default ink |
| `segmented(options, selected, on_select)` | One sunken row, the selected option raised |

## Split panes

| Signature | What it returns |
|---|---|
| `split_pane(o)` | Two panels and a divider that can be dragged. `dir` is `"row"` for a vertical divider with the panels side by side, `"column"` for a horizontal one with them stacked |
| `split_sizes(extent, fraction, min_a, min_b, bar)` | The two panel extents in px, clamped to both minimums |
| `split_at(extent, at, min_a, min_b, bar)` | A pointer position along the axis, as a fraction per mille |
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

## Data

| Signature | Notes |
|---|---|
| `table_header(labels, widths)` | A header row of fixed column widths |
| `table_row(key, values, widths)` | A keyed body row — keyed, so re-sorting **moves** rows |
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
| `chip(label, on_remove, props)` | A pill, with an optional × carrying `props` |
| `badge(label, tone)` | `tone` is a role family: `"info"` → `info.subtle` on `info.base` |
| `progress(fraction)` | A bar; the fraction is clamped to 0–1 |
| `skeleton(width, height)` | A sunken placeholder |
| `code_block(code)` | Monospace on a sunken ground |

## Feedback

| Signature | Notes |
|---|---|
| `toast(message, tone)` | A raised, toned strip |
| `banner(message, tone, action_label, on_action)` | Full width, a 3-unit accent edge, an action at the end |
| `spinner()` | An 18 px arc the **client** spins — `animation: "spin"`, no frames on the wire |
| `spinner_sized(size)` | The same, sized |
| `empty_state(title, body, action_label, on_action)` | Centred: a placeholder mark, a title, a line, one button |

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

Every builder above takes `w` and `h` as the size of the **whole** chart —
axes included. Each works out its own gutter from how wide its readings print
and takes `chart_plot_h(h)` for the marks, so a caller sizes a cell and never
a plot. The ones whose x axis is a category (`chart_line`, `chart_area`,
`chart_bar`, `chart_multi_line`, `chart_candle`) take an optional list of
labels last; without it the marks are numbered.


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
