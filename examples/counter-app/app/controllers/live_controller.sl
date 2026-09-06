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
  with_state({"count": count}, column(
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
      keyed("value", text(
        count.to_s,
        {"size": 7, "weight": "bold"}
      )),
      row(
        {"gap": 2},
        [button("−", "decrement"), local_button("+", "state.count += 1; value.text = str(state.count)", "increment")]
      ),
      text(
        "− is a round trip; + updates locally, then tells Soli.",
        {"fg": "text.muted", "size": 1}
      )
    ]
  ))
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
  if draft.blank?
    return {
      "items": items,
      "draft": draft,
      "next_id": next_id
    }
  end

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
      row(
        {"gap": 3, "align": "center"},
        [avatar("public/images/avatar.png", 32), h1("Todo")]
      ),
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

# ---------------------------------------------------------------- gallery
# One page with every catalogue widget, driven by a handful of state keys, so
# the whole catalogue is exercised by one mount and a few clicks.

def gallery(event_data)
  event = event_data["event"]
  params = event_data["params"]
  state = event_data["state"]
  tab = state["tab"] ?? "Overview"
  open = state["open"] ?? "a"
  page = state["page"] ?? 1
  seg = state["seg"] ?? "Day"
  sheet_open = state["sheet"] ?? false
  tree_open = state["tree_open"] ?? ["root"]

  match event {
    "tab" => {
      "tab": params["props"]["tab"],
      "open": open,
      "page": page,
      "seg": seg,
      "sheet": sheet_open,
      "tree_open": tree_open
    },
    "toggle" => {
      "tab": tab,
      "open": params["props"]["id"] == open ? "" : params["props"]["id"],
      "page": page,
      "seg": seg,
      "sheet": sheet_open,
      "tree_open": tree_open
    },
    "page" => {
      "tab": tab,
      "open": open,
      "page": params["props"]["page"],
      "seg": seg,
      "sheet": sheet_open,
      "tree_open": tree_open
    },
    "seg" => {
      "tab": tab,
      "open": open,
      "page": page,
      "seg": params["props"]["option"],
      "sheet": sheet_open,
      "tree_open": tree_open
    },
    "sheet" => {
      "tab": tab,
      "open": open,
      "page": page,
      "seg": seg,
      "sheet": !sheet_open,
      "tree_open": tree_open
    },
    "tree" => {
      "tab": tab,
      "open": open,
      "page": page,
      "seg": seg,
      "sheet": sheet_open,
      "tree_open": toggle_id(tree_open, params["props"]["id"])
    },
    _ => {
      "tab": tab,
      "open": open,
      "page": page,
      "seg": seg,
      "sheet": sheet_open,
      "tree_open": tree_open
    },
  }
end

def toggle_id(ids, id)
  return ids.filter(fn(x) { x != id }) if ids.includes?(id)

  ids.concat([id])
end

def gallery_view(state)
  tab = state["tab"] ?? "Overview"
  open = state["open"] ?? "a"
  page = state["page"] ?? 1
  seg = state["seg"] ?? "Day"
  sheet_open = state["sheet"] ?? false
  tree_open = state["tree_open"] ?? ["root"]
  sections = [
    {
      "id": "a",
      "title": "What is EUI?",
      "body": "A protocol for interfaces without a document engine."
    },
    {
      "id": "b",
      "title": "Why no CSS?",
      "body": "Styles are resolved on the server; the client looks them up."
    },
    {
      "id": "c",
      "title": "Is it secure?",
      "body": "Deny by default, no code from the network, quotas everywhere."
    }
  ]
  tree = [{
    "id": "root",
    "label": "app",
    "children": [
      {
        "id": "ctl",
        "label": "controllers",
        "children": [{
          "id": "live",
          "label": "live_controller.sl",
          "children": []
        }]
      },
      {
        "id": "views",
        "label": "views",
        "children": []
      }
    ]
  }]
  page_content = column(
    {"gap": 5, "pad": 6},
    [
      navbar("EUI", ["Overview", "Inputs", "Data"], tab, "tab"),
      tabs(["Overview", "Inputs", "Data"], tab, "tab"),
      row(
        {"gap": 3, "wrap": "wrap"},
        [stat("Nodes", "14", "primitives"), stat("Roles", "28", "colours"), stat("Tests", "181", "and counting")]
      ),
      banner("This gallery is served by Soli and drawn by EUI.", "info", "Open sheet", "sheet"),
      row(
        {"gap": 2, "align": "center"},
        [
          progress(0.62),
          muted("62 %"),
          spinner(),
          badge("beta", "warning"),
          chip("keyed", "", {}),
          chip("removable", "noop", {"id": 1})
        ]
      ),
      row(
        {"gap": 3, "align": "center"},
        [
          segmented(["Day", "Week", "Month"], seg, "seg"),
          pagination(page, 9, "page"),
          breadcrumb([{"label": "app", "path": "/"}, {
            "label": "gallery",
            "path": "/gallery"
          }], "noop")
        ]
      ),
      stepper(["Spec", "Client", "Soli", "Ship"], 2),
      row(
        {"gap": 4, "align": "start"},
        [
          column(
            {"gap": 3, "grow": 1},
            [
              accordion(sections, open, "toggle"),
              code_block("router_eui(gallery, live#gallery, live#gallery_view)  # config/routes.sl")
            ]
          ),
          column(
            {"gap": 3, "width": 240},
            [
              card({"gap": 2}, [h2("Tree"), tree_view(tree, tree_open, "tree", 0)]),
              menu(["Rename", "Duplicate", "Delete"], "noop"),
              tooltip("A tooltip")
            ]
          )
        ]
      ),
      empty_state(
        "Nothing here yet",
        "Widgets that need canvas paths — charts — wait for the renderer.",
        "Noted",
        "noop"
      ),
      form([field("Name", "", "noop"), field("Email", "", "noop")], "Save", "noop"),
      skeleton(320, 12)
    ]
  )
  layers = sheet_open ? [
    page_content,
    sheet("right", [h2("A sheet"), text("Slides in over the page.", {"fg": "text.muted"}), button("Close", "sheet")])
  ] : [page_content]
  stack({"gap": 0}, layers)
end
