# Writing views

> Built, and running: `examples/counter-app` is served by a `soli` compiled
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
`app/controllers/eui_builders.sl`. Each returns a hash:

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
handler runs, the view re-renders, the diff comes back. That is the whole
model today.

Local handlers — a `local { }` block compiled to bytecode and run on the
client for hover, typing, validation and optimistic updates — are specified
in outline and not built. `Handler::Local` exists on the wire; the client
currently treats it as inert. When it lands, the rule stays: **authorisation
is never local**, and a local handler's effect is advisory until the server
re-derives it.

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
