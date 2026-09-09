# Components

> Every function on this page exists. The library is
> `examples/counter-app/app/controllers/eui_builders.sl` — 107 functions, all of
> them plain Soli, none of them native — and the server that reads what they
> return is `lang/src/serve/eui/tree.rs`. The vocabulary tables below are that
> file's own match arms, not a wish list.

A component is data. A view returns a hash, the server turns it into nodes,
diffs it against the tree that session last received, and sends the patch. So
a "component" here is nothing but a Soli function that returns a hash — you
write one the same way the library wrote its hundred and seven.

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

A hundred and seven functions, every one of them a function over the primitives.
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
| `spacer()` | Empty space with `grow: 1` |
| `divider()` | A hairline rule |
| `scroll(style, children)` | A clipping viewport, laid out as a column |
| `list(style, item_height, children)` | A virtualised list: the client lays out only the visible rows |
| `list_window(style, item_height, count, heights, children, on_window)` | A **windowed** list — see below |
| `input(value, on_change)` | A bordered single-line field, `change` wired to `on_change` |
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

## Buttons

| Signature | What it returns |
|---|---|
| `button_variant(label, on_click, bg, fg)` | The engine: a keyed box with `click`, and local `pointer_enter/leave/down/up` that repoint it at declared styles |
| `secondary_button(label, on_click)` | `surface.sunken` on `text.default` |
| `danger_button(label, on_click)` | `danger.base` on `danger.on` |
| `ghost_button(label, on_click)` | No fill, accent text |
| `icon_button(label, on_click, props)` | A 28×28 square, `props` travelling with the click |
| `loading_button(label, on_click, key)` | Reveals a spinner and changes the label **locally** on press, then sends the event |
| `local_button(label, program, after)` | A primary button whose click runs `program` locally, then sends `after` |
| `theme_toggle()` | Light/dark, entirely on the client (`theme.toggle()`), no round trip and nothing told to the server |

Feedback that costs no network is the point of the variant engine: hover and
press switch between style records the session already holds.

## Input and forms

| Signature | Notes |
|---|---|
| `checkbox(label, checked, on_toggle, props)` | The box's fill says its state; `props` come back as `params["props"]` |
| `switch(label, on, on_toggle, props)` | A track and a knob, placed by `justify` |
| `field(label, value, on_change)` | A muted label over an input |
| `form(children, submit_label, on_submit)` | The children, then a right-aligned submit |
| `sized_input(value, on_change, width)` | An input of a fixed width |
| `select(options, value, open, on_toggle, on_pick)` | Closed, it is its anchor; open, a dropdown. **The server owns `open`** |
| `select_option(label, selected, on_pick)` | One row of that dropdown, carrying `{"value": label}` |
| `dropdown(anchor, content, open)` | A panel positioned under its anchor; returns the anchor alone when closed |
| `slider(value, min, max, on_set)` | A 240 px track. Press, drag, or click; arrows nudge once focused. `on_set` receives `params["kind"]` (`"click"`, `"pointer_down"`, `"pointer_move"`, `"pointer_up"`, `"key_down"`) |

None of them keeps state. The handler does; the widget draws what it is told
and carries the identity the handler will need.

## Calendar and pickers

One engine, three pickers. `month` is `"YYYY-MM"`, days are ISO
`"YYYY-MM-DD"` strings — which compare correctly as strings, so no date
arithmetic reaches the view.

| Signature | Notes |
|---|---|
| `calendar(month, selected, range_start, range_end, on_pick, on_nav)` | Navigation, weekday header, a seven-column grid. `selected` is a list of ISO days; a range shades between its ends |
| `date_picker(month, value, on_pick, on_nav)` | One day |
| `datetime_picker(month, date, time, hour_open, min_open, on_pick, on_nav, on_hour_toggle, on_min_toggle, on_hour, on_min)` | A day plus hour and minute selects (`HH:MM`, nothing else) |
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
| `tabs(names, active, on_select)` | A row of labels, the active one underlined; each carries `{"tab": name}` |
| `dialog(title, body_children, actions)` | An overlay: dimmed ground, centred panel, actions right |
| `sheet(side, children)` | A 320 px panel at the left or right edge, over a dimmed ground |
| `drawer(children)` | `sheet("left", …)` |
| `popover(anchor, content, open)` | A panel over its anchor; the anchor alone when closed |
| `toolbar(children)` | A raised strip with a bottom rule |
| `accordion(sections, open_id, on_toggle)` | Sections of `{id, title, body}`; the open one shows its body |
| `stepper(steps, current)` | Numbered dots — done ones filled, the current one ringed |
| `menu(items, on_pick)` | A raised column; each item carries `{"item": it}` |
| `tooltip(content)` | Inverted text on the default ink |
| `segmented(options, selected, on_select)` | One sunken row, the selected option raised |

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
| `chart_donut(id, parts, labels, w, h)` | One arc per part in the four `base` roles, the total in the hole, a legend under it |
| `chart_points(values, w, h)` | The series scaled into `w × h` as `[x, y]` pairs |
| `chart_grid(w, h)` | Four hairlines to read a series against |
| `chart_max(values)` | The top of the scale, never 0 |
| `chart_spans(centres, w)` | The width of each hover band, from where the marks are |
| `flatten_points(points)` | `[[x, y], …]` → `[x, y, …]` |

No chart library, no SVG, no client change: a chart is arithmetic in Soli and
a list of numbers on the wire.

### Answering the pointer

The client hit-tests boxes, not paths — a `canvas` is one node — so a chart
that answers the pointer is a `stack` of three layers: a row of wash columns
behind the drawing, the drawing, and a row of invisible bands in front. Each
band holds a value chip and two local handlers (spec 07 §1), which repoint two
nodes at styles the handler declared: its wash, and its own chip.

```soli
"pointer_enter": {
  "local": "cw_line_3.style = @lit; ct_line_3.style = @shown",
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
