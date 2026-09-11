# Writing views

> Built, and running: `examples/demo-app` is served by a `soli` compiled
> with `--features eui`, and `crates/eui-client/tests/soli_e2e.rs` clicks its
> button through the real socket.

An EUI component is a LiveView component with a different render. Same
registration, same handler contract, same registry and worker pool; only the
view — a function of state returning a **node tree as plain data** — and the
wire differ.

```soli
# config/routes.sl
router_eui("counter", "live#counter", "live#counter_view")
```

```soli
# app/controllers/live_controller.sl

# The handler: exactly a LiveView handler.
def counter(event_data)
  event = event_data["event"]
  count = event_data["state"]["count"] ?? 0
  if event == "increment"
    {"count": count + 1}
  elsif event == "decrement"
    {"count": count - 1}
  else
    {"count": count}
  end
end

# The view: state in, tree out. Nothing here is native.
def counter_view(state)
  count = state["count"] ?? 0
  column({"pad": 6, "gap": 4, "align": "start", "bg": "surface.base"}, [
    text("Counter", {"size": 4, "weight": "semibold"}),
    text(count.to_s, {"size": 7, "weight": "bold"}),
    row({"gap": 2}, [
      button("−", "decrement"),
      button("+", "increment")
    ]),
    text("Every click is a round trip.", {"fg": "text.muted", "size": 1})
  ])
end
```

`column`, `row`, `text`, `button` and the rest are ordinary Soli functions in
`app/controllers/eui_builders.sl` — all 150 of them are listed, with
their signatures, in [Components](/docs/components). Each returns a hash:

```
{"k": "box", "s": {style}, "t": text, "c": [children], "on": {"click": "increment"}, "key": ..., "p": {props}}
```

The server turns that into nodes, interns every atom and every distinct
style once per session, diffs against the tree it last sent, and encodes the
patch. A view author never sees a `StyleRecord` and never sees a byte.

## There is no CSS

Style is a hash of the spec's own vocabulary — `gap`, `pad`, `bg`, `size`,
`weight`, `radius`, `width`, `align` — and colours are **roles**, not
literals: `"accent.base"`, `"text.muted"`. The client resolves those against
the viewer's mode, so the same view is correct in dark mode without the
server knowing. A literal `"#RRGGBB"` is allowed for a brand mark or a data
series, and wrong for a surface.

You cannot write a selector, because there is nothing to select against. If
two places should look alike, they call the same function.

## Where a handler runs

`"on": {"click": "increment"}` names a **server** event: a round trip, the
handler runs, the view re-renders, the diff comes back.

A handler can also run **locally first**. The counter's `+`:

```soli
local_button("+", "state.count += 1; value.text = str(state.count)", "increment")
```

That string is a small statement language — assignments to `state.x`, to a
keyed node's `.text` or `.style`, arithmetic and comparisons, `if … else`,
`emit("name")`, and `self` for the node carrying the handler. Soli compiles
it to a chunk of the bytecode in `spec/07`, delivers it once per session,
and the client verifies it before its first run and executes it with a fuel
budget. It reads the root node's props (`with_state({"count": count}, ...)`
puts them there), rewrites the node keyed `"value"`, and only *then* sends
`increment` to the server — whose next batch confirms or corrects.

Every button in the catalogue uses the same mechanism for its states:

```soli
"pointer_enter": {"local": "self.style = @hover", "styles": {"hover": hover}}
```

The `styles` map declares the records a handler may point at; the client
switches the node between session styles it already holds, so hover and
press cost no network and no allocation. An assembly-list form of `local`
also exists for tooling.

The rule stays: a local handler's effect is advisory, and **authorisation is
never local**. A `local { … }` source syntax over this is not built yet; the
assembly list is the interface today.

## Images

```soli
avatar("public/images/avatar.png", 32)
image("public/images/chart.png", 320, 200)
```

The path is a file in the application. The server hashes it and sends the
hash; the client fetches it once from `/_eui/asset/<hash>`, verifies the
bytes against the name, and caches it for good. Without an explicit size the
image takes its own. PNG, JPEG or WebP, told apart by their first bytes.

## Lists

Give repeated children a key and re-sorting becomes moves rather than
rebuilds:

```soli
list({"height": 400}, 20, invoices.map(fn(inv) {
  keyed(inv["id"], row({"gap": 2}, [
    text(inv["reference"], {}),
    text(inv["client"], {}),
    text(inv["total"].to_s, {})
  ]))
}))
```

A `list` with an item height is virtualised: the client lays out only the
rows it can see, so ten thousand rows cost about what fifty do.

## What the diff does

Matched nodes keep their ids, so the client edits its arena in place. Keyed
children are matched by key and reordered with `MoveChild`; positional
children are matched by index. A node whose kind changed is replaced whole.
The end-to-end test asserts that three clicks leave the tree at the same nine
nodes with the same ids.

A render that changed nothing sends nothing: no ops, no batch, no frame. That
matters more than it sounds, because applying a batch — **any** batch, empty
included — puts back every style a local handler had previewed and relights
whatever the pointer is over. A page that sent an empty batch on a timer used
to restyle itself once a tick forever, which reads as a flicker and is one.

## A page with a clock

A node carrying `wake` (`06 §1.1`) makes the client send an event on a timer,
and **the view runs on every one of them**. There is no way to tell the
server "nothing changed, skip the render": the handler runs, the view runs,
the tree is built, and only then does the diff find there was nothing to say.
So on a page that wakes, the cost of building the view is paid several times
a second for as long as the window is open, and three rules follow. (A render
the *server* asked for — `eui_wake`, below — is the same render and obeys the
same three rules. What it saves is the ones nobody needed.)

**Return the same object for a subtree that has not changed.** A *keyed* node
whose hash is the very same value the last render returned is kept by the
encoder and never converted again — not its props, not its children. That is
what makes an idle tick free. Cache built nodes in a module global and hand
the cached one back:

```soli
CHAT_RIVERS = {}

def chat_river(state, lay)
  key = [room, str(width), str(generation), str(state["me"])].join("|")
  kept = CHAT_RIVERS[key]
  return kept unless kept.nil?

  built = chat_build_river(state, lay)
  CHAT_RIVERS = {}          # a few windows, not the whole room
  CHAT_RIVERS[key] = built
  built
end
```

The node must be `keyed` for this to work at all, and the saving is largest
where the node is expensive: a virtualised list carries one height per row,
and four thousand of those converted to wire values twice a second is most of
a core for a screen where nothing is happening.

**Put everything the subtree is drawn from in the key** — the viewer
included. These caches are process globals and every session shares them, so
a row that renders one way for the person who reacted to it and another way
for everyone else needs `me` in its key, or two windows will be handed each
other's screens.

**Keep anything that changes every render behind a toggle.** A dev bar
reporting the last render's cost changes on every render by definition, so
leaving it up makes every tick a tick with ops in it — the very thing it
exists to help you see. Atrium puts it behind the `⋯` in its header, and on
`F2` as well. Two things about that bar are worth knowing before you go
looking for one that is not there: `eui_stats()` is **empty outside
`--dev`**, so `dev_bar` draws nothing on a production server; and a key
reaches nothing at all while nothing is focused (`driver.rs`, `fn key`
returns early on `self.focused`), so a shortcut wants an `autofocus`
somewhere or a control you can click.

## A local handler costs one chunk per key

A chunk's `self` is compiled to the **key of the node it is on**, and the
compile is cached on the triple (source, key, declared styles). So a keyed
node carrying a local handler interns one chunk *per key* even when every one
of them has byte-identical source. The table holds 4 095, and a session that
overflows it ends:

> this session has interned more than 4095 chunks; a key or a style derived
> from data grows the table with every new value

Which means: **do not put a local handler on the rows of a long list.** A
hover on four thousand messages is four thousand chunks and the window dies
about seven hundred rows into a scroll. Put the handler on something there is
a bounded number of — the list, the toolbar, the shell — or do without.

Two things soften it. An event dispatches to the nearest **ancestor** that
handles it, so one handler on a row's container answers for everything inside
the row; and `fg` is inherited by any child that sets none, so one style
change on a container can light or hide a whole group without naming any of
it. Between them, a great deal of per-row behaviour can be written once.

Split your counters, too. One that moves on *anything* is what a tick
compares to know whether to redraw; a cache keyed on it never hits. Keep a
second one that moves only when the drawn thing changes, and key the caches
on that.

## State shared between sessions

One window learns what another did in one of two ways, and an application
that shares state wants both.

**`eui_wake(component)`** renders every *other* live session of a component,
now — the session that calls it is left out, being already mid-render. It is a
Soli builtin (feature `eui`), it returns how many windows were told, and it
belongs where the change is written rather than on a timer:

```soli
def chat_bump
  # … move the counter …
  eui_wake("chat")
end
```

Nothing in the protocol ever forbade this: the session is a WebSocket, `Batch`
is S→C (spec 01 §3), and the client applies whatever arrives. What each woken
session then runs is the ordinary handler and view — the event is the one its
own `wake` handler already answers (`eui_wake("chat")` posts `tick`; pass a
second argument to post something else), so a view needs no special case for
having been woken from outside.

**A `wake` on the view** is still what watches the other half: the things that
stop being true without anyone doing anything — a presence that decays, an "is
typing" that expires, a countdown. Nobody moves a counter when a timestamp
goes stale, so nobody can be woken for it. Keep the clock for those, and let
the period suit what it is watching rather than the message you no longer wait
for.

The counter in a module global stays under both: it is what a woken handler
compares to know whether anything actually changed, and what makes a wake that
changed nothing cost an empty diff instead of a redraw.

The trap is that **a module global belongs to one thread.** The server hashes
each session onto one of its realtime workers, so with more than one of those
two windows can land on different threads and hold entirely different copies
of what you thought was shared state — intermittently, depending on the hash,
which is the worst way for something to not work. Serve an application that
shares state between sessions with one realtime worker:

```sh
SOLI_WS_WORKERS=1 soli serve myapp --port 5011
```

An application that needs this properly — several servers, or a pool that has
to stay wide — keeps the shared thing in `Cache` (SoliKV) and pays a round
trip per event for it. A global is the demo's bargain, and it is worth
knowing it is one.
