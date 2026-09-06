# The counter: the same handler contract as any LiveView component.
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

# The view: state in, node tree out. Plain data; the server does the rest.
def counter_view(state)
  count = state["count"] ?? 0
  column(
    {
      "pad": 6,
      "gap": 4,
      "align": "start",
      "bg": "surface.base"
    },
    [
      text(
        "Counter",
        {"size": 4, "weight": "semibold"}
      ),
      text(
        count.to_s,
        {"size": 7, "weight": "bold"}
      ),
      row({"gap": 2}, [button("−", "decrement"), button("+", "increment")]),
      text(
        "Every click is a round trip; the value comes back from Soli.",
        {"fg": "text.muted", "size": 1}
      )
    ]
  )
end

# ------------------------------------------------------------------- todo
# A list with keyed rows, a text field, and checkboxes. A click on a row's
# checkbox arrives with params["props"]["id"], because the server keeps the
# tree it sent and attaches the clicked node's props.

def toggle_item(items, id)
  items.map(fn(it) { it["id"] == id ? {
    "id": it["id"],
    "title": it["title"],
    "done": !it["done"]
  } : it })
end

def without_item(items, id)
  items.filter(fn(it) { it["id"] != id })
end

def pending_items(items)
  items.filter(fn(it) { !it["done"] })
end

def todo(event_data)
  event = event_data["event"]
  params = event_data["params"]
  state = event_data["state"]
  items = state["items"] ?? [
    {
      "id": 1,
      "title": "Write the spec",
      "done": true
    },
    {
      "id": 2,
      "title": "Build the client",
      "done": true
    },
    {
      "id": 3,
      "title": "Ship the counter through Soli",
      "done": false
    }
  ]
  draft = state["draft"] ?? ""
  next_id = state["next_id"] ?? 4

  match event {
    "draft" => {
      "items": items,
      "draft": params["payload"],
      "next_id": next_id
    },
    "add" => add_item(items, draft, next_id),
    "toggle" => {
      "items": toggle_item(items, params["props"]["id"]),
      "draft": draft,
      "next_id": next_id
    },
    "remove" => {
      "items": without_item(items, params["props"]["id"]),
      "draft": draft,
      "next_id": next_id
    },
    "clear_done" => {
      "items": pending_items(items),
      "draft": draft,
      "next_id": next_id
    },
    _ => {
      "items": items,
      "draft": draft,
      "next_id": next_id
    },
  }
end

def add_item(items, draft, next_id)
  return {
    "items": items,
    "draft": draft,
    "next_id": next_id
  } if draft.blank?

  {
    "items": items.concat([{
      "id": next_id,
      "title": draft,
      "done": false
    }]),
    "draft": "",
    "next_id": next_id + 1
  }
end

# One todo row: checkbox, then a remove button; both carry the item's id.
def todo_row(it)
  remove = ghost_button("×", "remove")
  remove["p"] = {"id": it["id"]}
  keyed(it["id"], row(
    {
      "gap": 3,
      "align": "center",
      "pad": [1, 0, 1, 0]
    },
    [checkbox(it["title"], it["done"], "toggle", {"id": it["id"]}), spacer(), remove]
  ))
end

def todo_view(state)
  items = state["items"] ?? []
  draft = state["draft"] ?? ""
  remaining = items.filter(fn(it) { !it["done"] }).length()
  column(
    {
      "pad": 6,
      "gap": 4,
      "bg": "surface.base"
    },
    [
      h1("Todo"),
      row({"gap": 2}, [
        {
          "k": "input",
          "t": draft,
          "s": {
            "grow": 1,
            "pad": [2, 3, 2, 3],
            "border": 1,
            "border_color": "border.default",
            "radius": 2
          },
          "on": {"change": "draft", "submit": "add"}
        },
        button("Add", "add")
      ]),
      column({"gap": 0}, items.map(fn(it) { todo_row(it) })),
      row(
        {"gap": 3, "align": "center"},
        [muted(remaining.to_s + " left"), spacer(), secondary_button("Clear done", "clear_done")]
      )
    ]
  )
end

# ------------------------------------------------------------------ table
# Ten thousand keyed rows in a virtualised list; sorting is a permutation of
# keys, so the diff is moves.

def table(event_data)
  event = event_data["event"]
  state = event_data["state"]
  order = state["order"] ?? "asc"
  order = order == "asc" ? "desc" : "asc" if event == "sort"
  {"order": order}
end

def table_view(state)
  order = state["order"] ?? "asc"
  widths = [90, 160, 90, 90]
  ids = range(0, 10000)
  ids = ids.reverse() if order == "desc"
  rows = ids.map(fn(i) {
    table_row(i, [
      "FA-" + i.to_s,
      "Client " + (i % 37).to_s + " SARL",
      i % 3 == 0 ? "Paid" : "Open",
      (100 + i * 37).to_s + " €"
    ], widths)
  })
  column(
    {
      "pad": 6,
      "gap": 3,
      "bg": "surface.base"
    },
    [
      row(
        {"gap": 3, "align": "center"},
        [
          h1("Invoices"),
          badge("10 000 rows", "info"),
          spacer(),
          secondary_button(order == "asc" ? "Sort ↓" : "Sort ↑", "sort")
        ]
      ),
      table_header(["Reference", "Client", "Status", "Amount"], widths),
      list({"height": 400}, 22, rows)
    ]
  )
end
