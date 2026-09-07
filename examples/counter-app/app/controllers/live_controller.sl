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

def gallery_defaults(state)
  base = {
    "tab": "Overview",
    "open": "a",
    "page": 1,
    "seg": "Day",
    "sheet": false,
    "tree_open": ["root"],
    "select_open": false,
    "select_value": "Medium",
    "slider": 40,
    "cal_month": "2026-09",
    "cal_date": "",
    "dt_month": "2026-09",
    "dt_date": "2026-09-06",
    "dt_time": "09:30",
    "range_month": "2026-09",
    "range_start": "",
    "range_end": ""
  }
  for key in base.keys()
    base[key] = state[key] unless state[key].nil?
  end
  base
end

def set_key(state, key, value)
  state[key] = value
  state
end

def set_slider(state, params)
  payload = params["payload"]
  if params["kind"] == "click"
    state["slider"] = int(payload[0] * 100 / 240)
  elsif payload[0] == "ArrowRight"
    state["slider"] = state["slider"] + 5
  elsif payload[0] == "ArrowLeft"
    state["slider"] = state["slider"] - 5
  end
  state["slider"] = 0 if state["slider"] < 0
  state["slider"] = 100 if state["slider"] > 100
  state
end

def pick_range(state, iso)
  if state["range_start"].blank? || state["range_end"].present?
    state["range_start"] = iso
    state["range_end"] = ""
  elsif iso < state["range_start"]
    state["range_end"] = state["range_start"]
    state["range_start"] = iso
  else
    state["range_end"] = iso
  end
  state
end

def gallery(event_data)
  event = event_data["event"]
  params = event_data["params"]
  props = params["props"] ?? {}
  state = gallery_defaults(event_data["state"] ?? {})
  match event {
    "tab" => set_key(state, "tab", props["tab"]),
    "toggle" => set_key(state, "open", props["id"] == state["open"] ? "" : props["id"]),
    "page" => set_key(state, "page", props["page"]),
    "seg" => set_key(state, "seg", props["option"]),
    "sheet" => set_key(state, "sheet", !state["sheet"]),
    "tree" => set_key(state, "tree_open", toggle_id(state["tree_open"], props["id"])),
    "select_toggle" => set_key(state, "select_open", !state["select_open"]),
    "select_pick" => set_key(set_key(state, "select_value", props["value"]), "select_open", false),
    "slider" => set_slider(state, params),
    "cal_nav" => set_key(state, "cal_month", month_shift(state["cal_month"], props["delta"])),
    "cal_pick" => set_key(state, "cal_date", props["date"]),
    "dt_nav" => set_key(state, "dt_month", month_shift(state["dt_month"], props["delta"])),
    "dt_pick" => set_key(state, "dt_date", props["date"]),
    "dt_time" => set_key(state, "dt_time", params["payload"]),
    "range_nav" => set_key(state, "range_month", month_shift(state["range_month"], props["delta"])),
    "range_pick" => pick_range(state, props["date"]),
    _ => state,
  }
end

def toggle_id(ids, id)
  return ids.filter(fn(x) { x != id }) if ids.includes?(id)

  ids.concat([id])
end

def gallery_view(raw_state)
  state = gallery_defaults(raw_state ?? {})
  tab = state["tab"]
  open = state["open"]
  page = state["page"]
  seg = state["seg"]
  sheet_open = state["sheet"]
  tree_open = state["tree_open"]
  pickers = row(
    {
      "gap": 4,
      "wrap": "wrap",
      "align": "start"
    },
    [
      labelled("Select", select([
        "Small",
        "Medium",
        "Large"
      ], state["select_value"], state["select_open"], "select_toggle", "select_pick")),
      labelled(
        "Slider",
        column({"gap": 2}, [slider(state["slider"], 0, 100, "slider"), muted("Value " + str(state["slider"]))])
      ),
      labelled("Date", date_picker(state["cal_month"], state["cal_date"], "cal_pick", "cal_nav")),
      labelled(
        "Date and time",
        datetime_picker(state["dt_month"], state["dt_date"], state["dt_time"], "dt_pick", "dt_nav", "dt_time")
      ),
      labelled(
        "Range",
        date_range_picker(state["range_month"], state["range_start"], state["range_end"], "range_pick", "range_nav")
      )
    ]
  )
  charts = row(
    {
      "gap": 4,
      "wrap": "wrap",
      "align": "start"
    },
    [
      labelled("Line", chart_line([
        3,
        5,
        4,
        8,
        6,
        9,
        7
      ], 240, 120)),
      labelled("Area", chart_area([
        2,
        4,
        3,
        6,
        5,
        8,
        9
      ], 240, 120)),
      labelled("Bars", chart_bar([
        4,
        7,
        3,
        8,
        5,
        6
      ], 240, 120)),
      labelled("Donut", chart_donut([
        5,
        3,
        2,
        1
      ], 120, 120))
    ]
  )
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
      pickers,
      charts,
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
        "Filters that match nothing land here. Clear them to see everything.",
        "Noted",
        "noop"
      ),
      form([field("Name", "", "noop"), field("Email", "", "noop")], "Save", "noop"),
      skeleton(320, 12)
    ]
  )
  # The page scrolls as a whole, like a browser's viewport would; the sheet
  # is a layer above it.
  page = scroll({}, [page_content])
  layers = sheet_open ? [
    page,
    sheet("right", [h2("A sheet"), text("Slides in over the page.", {"fg": "text.muted"}), button("Close", "sheet")])
  ] : [page]
  stack({"gap": 0}, layers)
end

# ---------------------------------------------------------------- feed
# A social feed of thousands of fixed-height cards in a virtualised list —
# the performance check: the client lays out and paints only what it shows,
# and a like is one round trip that patches one card.

FEED_NAMES = [
  "Ada",
  "Grace",
  "Linus",
  "Margaret",
  "Dennis",
  "Barbara",
  "Ken",
  "Radia",
  "Bjarne",
  "Frances",
  "Guido",
  "Hedy",
  "Alan",
  "Sophie",
  "Tim",
  "Anita",
  "Yukihiro",
  "Leslie",
  "Rob",
  "Katherine"
]
FEED_TEXTS = [
  "Shipped the virtualised list today: ten thousand rows, one layout pass, nothing off screen touched.",
  "A protocol without a document engine is a client that does less. That is the whole idea.",
  "Hot take: the byte budget is the design review. If the frame is bigger, the design got worse.",
  "Rewrote the layout engine's shrink step. CSS had it right about min-size: auto. Who knew.",
  "Every widget in the catalogue is a plain function returning a hash. No classes, no native code.",
  "Dark mode with zero bytes on the wire: the roles resolve on the client, the server never learns.",
  "The counter's click is 25 bytes. The answer is 9. I keep checking, it keeps being true.",
  "Fonts embedded, no system enumeration, no fingerprint. Privacy is a testable claim now.",
  "Charts drawn by the same rounded-rectangle shader as everything else. A segment is a capsule.",
  "Keyboard focus walks document order and wraps. Enter and Space are clicks. Escape drops it.",
  "Signed manifest, pinned on first use, rotation only with the old key's blessing.",
  "One artefact, no browser: the server on a thread, the window on the main one, a cookie between them.",
  "Measured, not promised: 96 MB. The runtime is the weight, not the window.",
  "The gallery opened on glass for the first time and found two bugs in ten minutes. Screens are tests.",
  "A wheel notch is 100 px eased over 180 ms, and a Magic Mouse sends zeros in between. Now known."
]
FEED_TONES = ["accent.base", "info.base", "success.base", "warning.base", "danger.base"]

def feed_post(i)
  name = FEED_NAMES[(i * 7) % FEED_NAMES.length()]
  {
    "id": i,
    "name": name,
    "handle": "@" + name.downcase() + str(i % 97),
    "initial": name[0],
    "tone": FEED_TONES[(i * 13) % FEED_TONES.length()],
    "when": str(1 + (i * 31) % 23) + (i % 2 == 0 ? "h" : "m"),
    "text": FEED_TEXTS[(i * 11) % FEED_TEXTS.length()],
    "replies": (i * 17) % 41,
    "reposts": (i * 29) % 113,
    "likes": (i * 43) % 977,
    "image": i % 3 == 0 ? "public/images/feed/" + str((i * 7) % 8) + ".png" : nil
  }
end

# Cards come in two heights: text only, or with a picture.
def feed_card_height(post)
  post["image"].nil? ? 128 : 316
end

def feed(event_data)
  event = event_data["event"]
  params = event_data["params"]
  state = event_data["state"] ?? {}
  liked = state["liked"] ?? []
  count = state["count"] ?? 5000
  match event {
    "like" => {"liked": toggle_id(liked, params["props"]["id"]), "count": count},
    "more" => {
      "liked": liked,
      "count": count + 5000
    },
    _ => {"liked": liked, "count": count},
  }
end

# Cards are pure functions of (id, liked): built once, kept. The view is
# still a function of state; this only spares the interpreter the work of
# rebuilding ten thousand identical hashes on every event.
FEED_CARDS = {}

def feed_card(i, liked)
  key = str(i) + (liked ? ":liked" : "")
  cached = FEED_CARDS[key]
  return cached unless cached.nil?

  post = feed_post(i)
  card = keyed(i, post_card(post, liked, feed_card_height(post)))
  FEED_CARDS[key] = card
  card
end

def feed_view(state)
  liked = state["liked"] ?? []
  count = state["count"] ?? 5000
  cards = range(0, count).map(fn(i) { feed_card(i, liked.includes?(i)) })
  header = row(
    {
      "gap": 3,
      "align": "center",
      "pad": [3, 4, 3, 4],
      "bg": "surface.raised",
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    [h1("Feed"), badge(str(count) + " posts", "info"), spacer(), secondary_button("Load 5 000 more", "more")]
  )
  column(
    {
      "gap": 0,
      "align": "center",
      "bg": "surface.base"
    },
    [column(
      {
        "gap": 0,
        "width": "100%",
        "max_width": 680,
        "grow": 1,
        "bg": "surface.raised"
      },
      [header, list({"grow": 1}, 128, cards)]
    )]
  )
end
