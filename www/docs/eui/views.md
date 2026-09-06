# Writing views

> This page describes a design that is **neither specified nor built yet**. It is
> here so the shape of the thing is arguable before it is written.
> [What works today](/docs/status) says what does exist.

A view is a `.eui.sl` file. It is Soli, evaluated by the engine already in the
binary, and it produces a node tree rather than a string of HTML.

```soli
view "counter" do
  props count: Int

  column(gap: @space.4, pad: @space.6, align: :center) do
    text("Compteur", style: @text.xl, weight: :semibold)
    text(count.to_s, style: @text.4xl, color: @accent.base, key: "value")

    row(gap: @space.2) do
      button("−", variant: :secondary,
             on_click: local { state.count -= 1 })
      button("+", variant: :primary,
             on_click: local { state.count += 1 })
      button("Sauver", variant: :ghost, on_click: server("save"))
    end
  end
end
```

Server-side it is a component like a LiveView one: `router_eui("counter",
"eui#counter")` in `config/routes.sl`, with `mount` and `handle_event`.

## There is no CSS

Style is named arguments, and the values are **theme roles**, not literals:
`@space.4`, `@surface.base`, `@text.xl`. The server resolves those to a computed
style record and interns it; a thousand rows that pass the same arguments share
one style id.

You cannot write a selector, because there is nothing to select against. If two
places should look alike, they call the same function. That is the whole
mechanism.

## Where a handler runs

The one genuinely new idea in the DSL is that `on_click:` takes either kind of
handler and the difference is visible in the source.

```soli
on_click: local  { state.open = !state.open }   # no network traffic at all
on_click: server("save")                        # a round trip
on_click: local { state.saving = true }, then: server("save")
```

A `local { }` block is compiled to a bytecode chunk, published as a
content-addressed asset, and run by the client in a verified, metered VM. It can
read and write the component's own state, set text, style and props on nodes it
owns, and emit an event. It cannot open a file, a socket, or a process; it has
no clock finer than a coarse monotonic tick and no randomness unless the
manifest asked for it.

**The rule for choosing.** Local for hovering, focusing, typing, form
validation, toggling, animating, and optimistic updates — everything whose only
job is to make the interface feel immediate. Server for anything that touches
data, navigation, or permissions.

Authorisation is never local. A local handler's effect is advisory and is
re-derived server-side, so the worst a tampered client achieves is lying to
itself.

## Metering

A chunk runs with a fuel counter, an allocation budget, and a wall-clock
deadline. Exceeding any of them kills the chunk and falls the node back to a
server round trip. It is not a crash and it is not silent — the client reports
it. Soli's own `src/interpreter/limits.rs` already works this way for
server-side allocation; this is the same idea pointed at the client.

## Lists

Give repeated children a `key:` and re-sorting becomes moves rather than
rebuilds — 201 bytes to reverse fifty rows instead of 4 619.

```soli
for invoice in invoices
  row(key: invoice.id) do
    text(invoice.reference)
    text(invoice.client.name)
    text(invoice.total.to_s)
  end
end
```

A `list` node goes further: only the visible window is laid out at all, so a
ten-thousand-row table costs about what a fifty-row one costs.
