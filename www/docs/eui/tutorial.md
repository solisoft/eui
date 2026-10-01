# Build an application, step by step

> Built, and running: every step below is a component of
> `examples/demo-app`, and each figure is a live session of it — the same
> Rust client as the downloads, compiled to WebAssembly, drawing on a canvas
> in this page. Nothing is fetched until you press *Run it*, and one session
> runs at a time.

This page builds a shopping list called **Pantry** in six steps, each one the
step before it plus one idea. The whole of it is
[`tutorial_controller.sl`](https://github.com/solisoft/eui/blob/main/examples/demo-app/app/controllers/tutorial_controller.sl),
with no HTML, no CSS and no JavaScript anywhere in it. The excerpts below are
the same code set more tightly than `soli fmt` lays it out in that file.

You need a `soli` built with the `eui` feature, and an application to put
the code in:

```sh
soli new pantry --eui
cd pantry && soli serve . --dev --port 5011
```

`--eui` copies the builder catalogue (`column`, `text`, `button` and the rest,
in `app/controllers/eui_builders*.sl`) into the new application. Everything
below calls it.

## 1. A component

An EUI component is two Soli functions and one line of routing. The
**handler** takes an event and the current state and returns the next state;
the **view** takes the state and returns a tree of nodes, as plain hashes.

```soli
# config/routes.sl
router_eui("pantry_1", "tutorial#pantry_1", "tutorial#pantry_1_view")
```

```soli
# app/controllers/tutorial_controller.sl
def pantry_1(event_data)
  event_data["state"] ?? {}
end

def pantry_1_view(state)
  column({"pad": 8, "gap": 2, "bg": "surface.base"}, [
    text("Pantry", {"size": 6, "weight": "bold"}),
    text("Nothing to buy yet.", {"size": 1, "fg": "text.muted"})
  ])
end
```

Nothing happens yet, so the handler hands the state straight back. The view is
data: `column` returns `{"k": "box", "s": {…}, "c": [...]}`, and the server
turns that into nodes, interns each string and each distinct style once for
the session, and sends the result in a compact binary encoding. The numbers
are steps on a scale, not pixels — `pad: 8` is 32 px, `size: 6` is 30 px type
— so that every application spaces and sets type the same way.

::: eui pantry_1

## 2. Roles, not colours

A list, inside a card. Look at the colours: there are none. `surface.base`,
`text.muted` and the card's `surface.raised` are **roles**, and the client
resolves them against the viewer's theme. The same bytes draw the page light or
dark, and the server never learns which.

```soli
def pantry_seed
  [
    {"id": 1, "name": "Oat milk", "qty": 2, "done": false},
    {"id": 2, "name": "Lemons", "qty": 6, "done": false},
    {"id": 3, "name": "Sourdough", "qty": 1, "done": false}
  ]
end

def pantry_2_view(state)
  pantry_page({"pad": 8, "gap": 6}, [
    pantry_heading("Three things to pick up."),
    card({"pad": 0, "gap": 0, "tw": "divide-y divide-gray-200"}, pantry_seed.map { |it| pantry_row_2(it) })
  ])
end

def pantry_page(style, children)
  column({"height": "100%", "bg": "surface.base"}, [
    scroll({"grow": 1, "min_height": 0}, [column(style.merge({"width": "100%"}), children)])
  ])
end

def pantry_row_2(it)
  row({"gap": 3, "align": "center", "pad": [4, 6, 4, 6]}, [
    text(it["name"], {"size": 1, "weight": "medium", "grow": 1}),
    badge("× " + str(it["qty"]), "neutral")
  ])
end
```

`"tw": "divide-y divide-gray-200"` is the one Tailwind class string here: a
hairline between the rows, laid onto each child but the first. `tw()` turns
classes into the same style keys you would write by hand, and refuses the ones
EUI cannot draw by name ([Tailwind classes](/docs/tailwind)). `grow: 1` on the
name pushes the badge to the end of its row.

`pantry_page` is the other new thing, and every step from here on stands on
it. A window is as tall as the reader made it, and a list is as long as it
is; something has to give. In a browser that is the page, which scrolls
because it is a document. In EUI nothing scrolls unless a node says so: the
root fills the window, a `scroll` takes the room it is given, and the page
goes inside it, as wide as the window and as tall as it needs to be. Try it
in the figures below — the wheel scrolls the application, not this page.

::: eui pantry_2

## 3. Events and state

Now the numbers change. A button names an **event**; pressing it sends that
name to the server, the handler returns the next state, the view runs again,
and the server sends **only what changed** — here, one text node's new string.

```soli
def pantry_3(event_data)
  goods = (event_data["state"] ?? {})["goods"] ?? pantry_seed
  id = event_data.dig("params", "props", "id")
  match event_data["event"] {
    "more" => {"goods": pantry_bump(goods, id, 1)},
    "less" => {"goods": pantry_bump(goods, id, -1)},
    _ => {"goods": goods}
  }
end

def pantry_stepper(it)
  row({"gap": 1, "align": "center"}, [
    icon_button("−", "less", {"id": it["id"]}, {"icon": "minus", "key": "less" + str(it["id"]), "name": "One fewer " + it["name"]}),
    text(str(it["qty"]), {"size": 1, "weight": "semibold", "width": 24, "text_align": "center"}),
    icon_button("+", "more", {"id": it["id"]}, {"icon": "plus", "key": "more" + str(it["id"]), "name": "One more " + it["name"]})
  ])
end
```

Three things are worth seeing:

- **One handler serves every row.** The row's id rides on the button as a
  prop (`{"id": it["id"]}`), and the event arrives with the pressed node's
  props in `params["props"]`. Nothing is generated per row on the server.
- **Rows are keyed** (`keyed(it["id"], row(…))` in `pantry_row_3`), so the
  diff can tell a row that moved from a row that changed.
- **`name` is what an assistive technology says**: "One more Lemons", not
  "+".

::: eui pantry_3

## 4. Typing

A field is an `input` node. Every keystroke is a `change` event carrying the
text, and Enter is a `submit`. The draft is state like anything else, so the
server always knows what has been typed — and can clear it.

```soli
def pantry_4(event_data)
  was = pantry_defaults(event_data["state"] ?? {})
  id = event_data.dig("params", "props", "id")
  match event_data["event"] {
    "draft" => was.merge({"draft": event_data.dig("params", "payload").to_s}),
    "add" => pantry_add(was),
    "more" => was.merge({"goods": pantry_bump(was["goods"], id, 1)}),
    "less" => was.merge({"goods": pantry_bump(was["goods"], id, -1)}),
    "got" => was.merge({"goods": pantry_toggle(was["goods"], id)}),
    "clear" => was.merge({"goods": was["goods"].filter { |it| !it["done"] }}),
    _ => was
  }
end

def pantry_entry(draft)
  row({"gap": 2}, [
    input(draft, "draft", {"placeholder": "Add something…", "on": {"submit": "add"}, "style": {"grow": 1}}),
    button("Add", "add")
  ])
end
```

The placeholder is drawn by the client in `text.muted` while the field is
empty and is never sent back. Each row is now a `checkbox` naming the event
`got`, with the same `{"id": …}` props as the buttons beside it.

::: eui pantry_4 380

## 5. Without the round trip

Every press so far has gone to the server and back. On a fast network that is
invisible; on a train it is not. A handler can also run **in the client
first**:

```soli
def pantry_local_stepper(it)
  q = "q" + str(it["id"])
  more = icon_button("+", "more", {"id": it["id"]}, {"icon": "plus", "key": "more" + str(it["id"]), "name": "One more " + it["name"]})
  more["on"]["click"] = {"local": "state." + q + " += 1; " + q + ".text = str(state." + q + ")", "then": "more"}
  row({"gap": 1, "align": "center"}, [
    icon_button("−", "less", {"id": it["id"]}, {"icon": "minus", "key": "less" + str(it["id"]), "name": "One fewer " + it["name"]}),
    keyed(q, text(str(it["qty"]), {"size": 1, "weight": "semibold", "width": 24, "text_align": "center"})),
    more
  ])
end
```

For Lemons the `local` string reads `state.q2 += 1; q2.text = str(state.q2)`.
Soli compiles it to the bytecode of [spec 07](/spec/07-bytecode); the client
verifies it once, runs it with a fuel budget, changes the number where it was
pressed, and *then* sends `more`. The server's answer confirms it — or
corrects it, because a local effect is only ever a prediction, and nothing is
ever authorised locally.

`state.q2` is read from the root node's props, which the view sets with
`with_state(pantry_counts(goods), page)`. Here `+` is local and `−` is still a
round trip, so the two can be felt side by side.

One chunk is interned per key, so this belongs on a handful of rows, not on
ten thousand ([Writing views](/docs/views) says why).

::: eui pantry_5 380

## 6. Follow the window

There is no stylesheet, so there is no media query. Instead the client tells
the server how wide it is — on connect and after every resize — and the view
branches on the number:

```soli
def pantry_6(event_data)
  was = pantry_5(event_data)
  seen = event_data.dig("params", "viewport") ?? (event_data["state"] ?? {})["viewport"]
  was.merge({"viewport": seen ?? {"width": 1024}})
end

def pantry_6_view(state)
  was = pantry_defaults(state)
  wide = ((state["viewport"] ?? {})["width"] ?? 1024) >= 640
  shelf = column({"gap": 4, "grow": 1}, [
    pantry_entry(was["draft"]),
    card({"pad": 0, "gap": 0, "tw": "divide-y divide-gray-200"}, was["goods"].map { |it| pantry_row_5(it) })
  ])
  side = pantry_summary(was["goods"], wide)
  body = wide ? row({"gap": 6, "align": "start"}, [shelf, side]) : column({"gap": 6}, [shelf, side])
  page = pantry_page({"pad": wide ? 8 : 5, "gap": 6}, [pantry_heading(pantry_left(was["goods"])), body])
  with_state(pantry_counts(was["goods"]), page)
end
```

Wider than 640 px, the list sits beside a summary; narrower, the summary
follows it. Run it and resize the window: the page follows the drag rather than
waiting for it to stop, at about thirty widths a second.

::: eui pantry_6 510

## 7. Out of the browser

The figures above are a viewing vehicle. The application is the same one the
native client opens on Linux, macOS and Windows:

```sh
# From this site's demo server
eui wss://eui-data.solisoft.net/_eui/session/pantry_6

# From your own, while you develop it (plain ws:// is refused without this)
EUI_ALLOW_INSECURE_LOOPBACK=1 eui ws://127.0.0.1:5011/_eui/session/pantry_6
```

To give it to somebody as an application rather than an address,
`soli desktop build` packages the server and the client into one artifact that
opens its own window:

```sh
soli desktop build . --app-id net.example.pantry --eui pantry_6
```

That build still runs a server on a loopback port inside the process.

An application can also skip the server entirely: it starts the client itself
with `eui --pipe` and speaks to it over the client's standard input and
output, with no port, no TLS and no manifest ([01 §7](/spec/01-transport),
client 0.8 or later). Soli does not speak the pipe yet; the Ruby server does,
and Pantry's first step in Ruby is a complete program:

```ruby
require "eui"

class Pantry < EUI::Component
  def render
    column(pad: 8, gap: 2, bg: "surface.base") do
      [text("Pantry", size: "3xl", weight: "bold"),
       text("Nothing to buy yet.", size: "sm", fg: "text.muted")]
    end
  end
end

app = EUI::App.new(name: "Pantry")
app.mount("pantry", Pantry)
app.run_pipe     # opens the window; returns when it closes
```

`ruby pantry.rb` opens the window and nothing listens anywhere. The
window is the script's child: closing it ends the script, and the script
ending closes it ([Transport](/docs/transport) has the rest).

## Where to go next

- [Writing views](/docs/views): where a handler runs, lists, the diff, and a
  page with a clock in it.
- [Components](/docs/components) and the [Widget catalogue](/docs/widgets):
  every builder this page used, and the five hundred it did not.
- [Theming](/docs/theming): what a role is, and why the client resolves it.
