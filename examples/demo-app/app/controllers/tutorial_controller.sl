# The tutorial's application: a shopping list, one idea per step.
#
# `doc/docs/eui/tutorial.md` shows each step beside a live session of it, so
# every step is a component of its own — `pantry_1` … `pantry_6` in
# config/routes.sl — and each is the one before it plus one thing. The last
# is the whole application.
#
# Every name here starts with `pantry`. A top-level `def` is global across
# app/, and a local assigned without `let` under a global's name replaces
# that global for everyone; a prefix nobody else uses is the cheap defence.

# ------------------------------------------------------- 1. A component
# A handler (event and state in, state out) and a view (state in, node tree
# out). Nothing happens yet, so the handler hands the state straight back.

def pantry_1(event_data)
  event_data["state"] ?? {}
end

def pantry_1_view(state)
  column(
    {
      "pad": 8,
      "gap": 2,
      "bg": "surface.base"
    },
    [text(
      "Pantry",
      {"size": 6, "weight": "bold"}
    ), text(
      "Nothing to buy yet.",
      {"size": 1, "fg": "text.muted"}
    )]
  )
end

# ---------------------------------------------------- 2. Roles, not colours
# The same tree with a list in it. Every colour is a role, resolved by the
# client against the viewer's theme: the page is right in dark mode without
# the server ever learning which one they use.

def pantry_seed
  [
    {
      "id": 1,
      "name": "Oat milk",
      "qty": 2,
      "done": false
    },
    {
      "id": 2,
      "name": "Lemons",
      "qty": 6,
      "done": false
    },
    {
      "id": 3,
      "name": "Sourdough",
      "qty": 1,
      "done": false
    }
  ]
end

def pantry_2(event_data)
  event_data["state"] ?? {}
end

def pantry_2_view(state)
  pantry_page(
    {"pad": 8, "gap": 6},
    [
      pantry_heading("Three things to pick up."),
      card(
        {
          "pad": 0,
          "gap": 0,
          "tw": "divide-y divide-gray-200"
        },
        pantry_seed.map { |it| pantry_row_2(it) }
      )
    ]
  )
end

def pantry_heading(line)
  column({"gap": 1}, [text(
    "Pantry",
    {"size": 6, "weight": "bold"}
  ), text(
    line,
    {"size": 1, "fg": "text.muted"}
  )])
end

# A window is as tall as the reader made it, and a page can be taller. In
# EUI that is not a property of the page but a node: the root fills the
# window, a `scroll` takes the room it has, and the page goes inside it, as
# wide as the window and as tall as it needs to be. Every step from here on
# is built on this.
def pantry_page(style, children)
  column(
    {"height": "100%", "bg": "surface.base"},
    [scroll(
      {"grow": 1, "min_height": 0},
      [column(style.merge({"width": "100%"}), children)]
    )]
  )
end

def pantry_row_2(it)
  row(
    {
      "gap": 3,
      "align": "center",
      "pad": [4, 6, 4, 6]
    },
    [text(
      it["name"],
      {
        "size": 1,
        "weight": "medium",
        "grow": 1
      }
    ), badge("× " + str(it["qty"]), "neutral")]
  )
end

# ------------------------------------------------------ 3. Events and state
# A click names an event; the handler returns the next state; the view runs
# again and only what changed goes back. The row's id rides on the button's
# props, so one handler serves every row.

def pantry_3(event_data)
  goods = (event_data["state"] ?? {})["goods"] ?? pantry_seed
  id = event_data.dig("params", "props", "id")
  match event_data["event"] {
    "more" => {"goods": pantry_bump(goods, id, 1)},
    "less" => {"goods": pantry_bump(goods, id, -1)},
    _ => {"goods": goods},
  }
end

def pantry_bump(goods, id, by)
  goods.map { |it| it["id"] == id ? it.merge({"qty": pantry_at_least_one(it["qty"] + by)}) : it }
end

def pantry_at_least_one(n)
  n < 1 ? 1 : n
end

def pantry_3_view(state)
  goods = state["goods"] ?? pantry_seed
  pantry_page(
    {"pad": 8, "gap": 6},
    [
      pantry_heading("How many of each?"),
      card(
        {
          "pad": 0,
          "gap": 0,
          "tw": "divide-y divide-gray-200"
        },
        goods.map { |it| pantry_row_3(it) }
      )
    ]
  )
end

def pantry_row_3(it)
  keyed(it["id"], row(
    {
      "gap": 3,
      "align": "center",
      "pad": [3, 6, 3, 6]
    },
    [text(
      it["name"],
      {
        "size": 1,
        "weight": "medium",
        "grow": 1
      }
    ), pantry_stepper(it)]
  ))
end

def pantry_stepper(it)
  row(
    {"gap": 1, "align": "center"},
    [
      icon_button("−", "less", {"id": it["id"]}, {
        "icon": "minus",
        "key": "less" + str(it["id"]),
        "name": "One fewer " + it["name"]
      }),
      text(
        str(it["qty"]),
        {
          "size": 1,
          "weight": "semibold",
          "width": 24,
          "text_align": "center"
        }
      ),
      icon_button("+", "more", {"id": it["id"]}, {
        "icon": "plus",
        "key": "more" + str(it["id"]),
        "name": "One more " + it["name"]
      })
    ]
  )
end

# ---------------------------------------------------------------- 4. Typing
# A field is an `input` whose every keystroke is a `change` event carrying
# the text, and whose Enter is a `submit`. The draft lives in the state like
# everything else, so the server always knows what is typed.

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
    _ => was,
  }
end

def pantry_defaults(saved)
  {
    "goods": saved["goods"] ?? pantry_seed,
    "draft": saved["draft"].to_s,
    "next_id": saved["next_id"] ?? 4
  }
end

def pantry_add(was)
  name = was["draft"].trim()
  return was if name.blank?

  item = {
    "id": was["next_id"],
    "name": name,
    "qty": 1,
    "done": false
  }
  was.merge({
    "goods": was["goods"].concat([item]),
    "draft": "",
    "next_id": was["next_id"] + 1
  })
end

def pantry_toggle(goods, id)
  goods.map { |it| it["id"] == id ? it.merge({"done": !it["done"]}) : it }
end

def pantry_4_view(state)
  was = pantry_defaults(state)
  pantry_page(
    {"pad": 8, "gap": 6},
    [
      pantry_heading(pantry_left(was["goods"])),
      pantry_entry(was["draft"]),
      pantry_list(was["goods"]),
      row({"align": "center"}, [spacer(), secondary_button("Clear bought", "clear")])
    ]
  )
end

def pantry_left(goods)
  left = goods.filter { |it| !it["done"] }.length()
  left == 0 ? "Nothing left to buy." : str(left) + " left to buy."
end

def pantry_entry(draft)
  row({"gap": 2}, [
    input(draft, "draft", {
      "placeholder": "Add something…",
      "on": {"submit": "add"},
      "style": {"grow": 1}
    }),
    button("Add", "add")
  ])
end

def pantry_list(goods)
  card(
    {
      "pad": 0,
      "gap": 0,
      "tw": "divide-y divide-gray-200"
    },
    goods.map { |it| pantry_row_4(it) }
  )
end

def pantry_row_4(it)
  keyed(it["id"], row(
    {
      "gap": 2,
      "align": "center",
      "pad": [2, 4, 2, 4]
    },
    [checkbox(it["name"], it["done"], "got", {"id": it["id"]}), spacer(), pantry_stepper(it)]
  ))
end

# ------------------------------------------------- 5. Without the round trip
# `+` now changes the number where it was pressed, before the server hears of
# it: a local chunk the client verified once and runs with a fuel budget. It
# reads the root's props (`with_state`), rewrites the node keyed `q<id>`, and
# only then sends `more`, whose answer confirms it. `−` stays a round trip,
# so the two can be felt side by side.

def pantry_5(event_data)
  pantry_4(event_data)
end

def pantry_5_view(state)
  was = pantry_defaults(state)
  page = pantry_page(
    {"pad": 8, "gap": 6},
    [
      pantry_heading(pantry_left(was["goods"])),
      pantry_entry(was["draft"]),
      card(
        {
          "pad": 0,
          "gap": 0,
          "tw": "divide-y divide-gray-200"
        },
        was["goods"].map { |it| pantry_row_5(it) }
      ),
      row({"align": "center"}, [spacer(), secondary_button("Clear bought", "clear")])
    ]
  )
  with_state(pantry_counts(was["goods"]), page)
end

# The numbers the local chunks read, as the root's props: {"q1": 2, …}.
def pantry_counts(goods)
  counts = {}
  goods.each do |it|
    counts["q" + str(it["id"])] = it["qty"]
  end
  counts
end

def pantry_row_5(it)
  keyed(it["id"], row(
    {
      "gap": 2,
      "align": "center",
      "pad": [2, 4, 2, 4]
    },
    [checkbox(it["name"], it["done"], "got", {"id": it["id"]}), spacer(), pantry_local_stepper(it)]
  ))
end

def pantry_local_stepper(it)
  q = "q" + str(it["id"])
  more = icon_button("+", "more", {"id": it["id"]}, {
    "icon": "plus",
    "key": "more" + str(it["id"]),
    "name": "One more " + it["name"]
  })
  more["on"]["click"] = {
    "local": "state." + q + " += 1; " + q + ".text = str(state." + q + ")",
    "then": "more"
  }
  row(
    {"gap": 1, "align": "center"},
    [
      icon_button("−", "less", {"id": it["id"]}, {
        "icon": "minus",
        "key": "less" + str(it["id"]),
        "name": "One fewer " + it["name"]
      }),
      keyed(q, text(
        str(it["qty"]),
        {
          "size": 1,
          "weight": "semibold",
          "width": 24,
          "text_align": "center"
        }
      )),
      more
    ]
  )
end

# ----------------------------------------------------- 6. Follow the window
# The client says how wide it is on connect and after every resize; the
# handler keeps it, and the view branches on it — there is no media query,
# because there is no stylesheet. Wide: the list beside a summary. Narrow:
# the list, then the summary under it.

def pantry_6(event_data)
  was = pantry_5(event_data)
  seen = event_data.dig("params", "viewport") ?? (event_data["state"] ?? {})["viewport"]
  was.merge({"viewport": seen ?? {"width": 1024}})
end

def pantry_6_view(state)
  was = pantry_defaults(state)
  wide = ((state["viewport"] ?? {})["width"] ?? 1024) >= 640
  shelf = column(
    {"gap": 4, "grow": 1},
    [
      pantry_entry(was["draft"]),
      card(
        {
          "pad": 0,
          "gap": 0,
          "tw": "divide-y divide-gray-200"
        },
        was["goods"].map { |it| pantry_row_5(it) }
      )
    ]
  )
  side = pantry_summary(was["goods"], wide)
  body = wide ? row(
    {"gap": 6, "align": "start"},
    [shelf, side]
  ) : column({"gap": 6}, [shelf, side])
  page = pantry_page(
    {"pad": wide ? 8 : 5, "gap": 6},
    [pantry_heading(pantry_left(was["goods"])), body]
  )
  with_state(pantry_counts(was["goods"]), page)
end

def pantry_summary(goods, wide)
  todo_items = goods.filter { |it| !it["done"] }
  units = todo_items.reduce(fn(acc, it) { acc + it["qty"] }, 0)
  card(
    {"gap": 3, "width": wide ? 240 : "100%"},
    [
      text(
        "Summary",
        {"size": 1, "weight": "semibold"}
      ),
      pantry_fact("To buy", str(todo_items.length())),
      pantry_fact("Units", str(units)),
      pantry_fact("In the basket", str(goods.length() - todo_items.length())),
      secondary_button("Clear bought", "clear")
    ]
  )
end

def pantry_fact(label, value)
  row({"align": "center"}, [
    text(
      label,
      {"size": 1, "fg": "text.muted"}
    ),
    spacer(),
    text(
      value,
      {"size": 1, "weight": "semibold"}
    )
  ])
end
