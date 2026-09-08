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

# Spec 06 §1.1: the smallest thing that needs time to pass.
#
# The node carries a `wake` of 100 ms and a `wake` handler, so the client
# sends one event every 100 ms for as long as both are there — nobody
# clicks anything. It is what a progress bar for a player on another
# machine, or a job somewhere, is made of.
def clock(event_data)
  state = event_data["state"]
  ticks = state["ticks"] ?? 0
  event_data["event"] == "tick" ? {"ticks": ticks + 1} : {"ticks": ticks}
end

def clock_view(state)
  ticks = state["ticks"] ?? 0
  face = column(
    {
      "pad": 6,
      "gap": 2,
      "bg": "surface.base"
    },
    [
      text(
        "Ticks",
        {"size": 1, "fg": "text.muted"}
      ),
      text(
        str(ticks),
        {
          "size": 6,
          "weight": "bold",
          "fg": "text.default"
        }
      )
    ]
  )
  face["p"] = {"wake": 100}
  face["on"] = {"wake": "tick"}
  with_state({"ticks": ticks}, face)
end

# Spec 01 §4, on purpose: a view that cannot be encoded.
#
# `nope` is not an event kind the protocol has, so the server cannot turn
# this tree into ops. It exists so the end-to-end suite can check what
# happens next — the session ends with the reason and the window says so,
# rather than freezing on the last frame it was given.
def broken(event_data)
  event_data["state"] ?? {}
end

def broken_view(state)
  face = text("This view names an event that does not exist", {"size": 2})
  face["on"] = {"nope": "never"}
  column({"pad": 6}, [face])
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

# The gallery's sound: one chime, played by a button. The node carries
# what it should be doing; the client owns the clock and says when it
# ended (EUI spec 03 §7).
def gallery_sound(state)
  playing = state["sound"] ?? false
  column({"gap": 2}, [
    audio("public/sounds/chime.wav", {
      "playing": playing,
      "volume": 80
    }, {"ended": "sound_ended"}),
    row(
      {"gap": 3, "align": "center"},
      [
        button(playing ? "Stop" : "Play a chime", "sound"),
        muted(playing ? "playing…" : "1.6 s, decoded and mixed by the client")
      ]
    )
  ])
end

# The gallery's moving picture: a loop the client decodes and plays. The
# node says what it should be doing; the client owns the clock.
def gallery_video(state)
  playing = state["video"] ?? false
  column({"gap": 2}, [
    video("public/video/pulse.gif", {
      "playing": playing,
      "loop": false,
      "position": state["video_seek"] ?? 0
    }, {
      "width": 320,
      "height": 180,
      "radius": 3
    }, {"time_update": "video_time", "ended": "video_done"}),
    row(
      {"gap": 3, "align": "center"},
      [
        media_button(playing, "video", {}),
        media_scrubber(200, state["video_at"] ?? 0, 1440, "video_scrub", {"w": 200}),
        muted(media_clock(state["video_at"] ?? 0) + " / 0:01")
      ]
    )
  ])
end

def gallery_invoices
  [
    {
      "id": "FA-1001",
      "ref": "FA-1001",
      "client": "Ada SARL",
      "status": "Open",
      "amount": "137 €"
    },
    {
      "id": "FA-1002",
      "ref": "FA-1002",
      "client": "Grace Ltd",
      "status": "Paid",
      "amount": "412 €"
    },
    {
      "id": "FA-1003",
      "ref": "FA-1003",
      "client": "Linus GmbH",
      "status": "Open",
      "amount": "850 €"
    },
    {
      "id": "FA-1004",
      "ref": "FA-1004",
      "client": "Margaret SA",
      "status": "Paid",
      "amount": "112 €"
    },
    {
      "id": "FA-1005",
      "ref": "FA-1005",
      "client": "Dennis BV",
      "status": "Open",
      "amount": "920 €"
    },
    {
      "id": "FA-1006",
      "ref": "FA-1006",
      "client": "Barbara LLC",
      "status": "Paid",
      "amount": "256 €"
    },
    {
      "id": "FA-1007",
      "ref": "FA-1007",
      "client": "Ken SAS",
      "status": "Open",
      "amount": "640 €"
    },
    {
      "id": "FA-1008",
      "ref": "FA-1008",
      "client": "Radia Inc",
      "status": "Paid",
      "amount": "318 €"
    }
  ]
end

def gallery_grid_columns
  [
    {
      "id": "ref",
      "label": "Ref",
      "width": 80
    },
    {
      "id": "client",
      "label": "Client",
      "width": 120
    },
    {
      "id": "status",
      "label": "Status",
      "width": 88,
      "align": "center",
      "options": ["Open", "Paid"]
    },
    {
      "id": "amount",
      "label": "Amount",
      "width": 80,
      "align": "end"
    }
  ]
end

def gallery_grid_col_editable(id)
  for col in gallery_grid_columns()
    return grid_col_editable(col) if col["id"] == id
  end

  true
end

def gallery_grid_col_options(id)
  for col in gallery_grid_columns()
    return col["options"] ?? [] if col["id"] == id
  end

  []
end

def gallery_grid_visible_columns(width)
  cols = gallery_grid_columns()
  return cols.filter(fn(c) { ["ref", "amount"].includes?(c["id"]) }) unless bp_min(width, "sm")
  return cols.filter(fn(c) { c["id"] != "client" }) unless bp_min(width, "md")

  cols
end

def gallery_grid_write(state, row_id, col, value)
  for record in state["grid_rows"]
    record[col] = value if record["id"] == row_id
  end
  state
end

def gallery_grid_caption(state)
  rows = state["grid_rows"]
  first = rows.length() > 0 ? rows[0]["id"] : ""
  if state["grid_row"].present?
    line = state["grid_row"] + " · " + state["grid_col"]
    return state["grid_edit"] == true ? line + " · editing" : line
  end

  first + " first"
end

def gallery_grid_select(state, params)
  props = params["props"] ?? {}
  kind = params["kind"]
  if props["value"].present? && props["col"].present?
    gallery_grid_write(state, props["row"], props["col"], props["value"])
    state["grid_edit"] = false
    state["grid_menu"] = false
    return state
  end

  if kind == "blur" || kind == "submit"
    state["grid_edit"] = false
    state["grid_menu"] = false
    return state
  end

  row_id = props["row"]
  col = props["col"]
  same = state["grid_row"] == row_id && state["grid_col"] == col
  choices = gallery_grid_col_options(col)
  state["grid_row"] = row_id
  state["grid_col"] = col
  if same && gallery_grid_col_editable(col)
    if choices.length() > 0
      if state["grid_edit"] == true
        state["grid_menu"] = state["grid_menu"] == true ? false : true
      else
        state["grid_edit"] = true
        state["grid_menu"] = true
      end
    else
      state["grid_edit"] = true
      state["grid_menu"] = false
    end
  else
    state["grid_edit"] = false
    state["grid_menu"] = false
  end
  state
end

def gallery_grid_change(state, params)
  value = params["payload"]
  return state if value.nil?

  props = params["props"] ?? {}
  row_id = props["row"]
  col = props["col"]
  for record in state["grid_rows"]
    record[col] = value if record["id"] == row_id
  end
  # Stay in edit: Enter sends change then submit, and validate runs
  # against the tree *after* each handler. Leaving here would replace
  # the input before submit arrives (error 300).
  state
end

def gallery_grid_sort(state, col)
  dir = "asc"
  dir = "desc" if state["grid_sort"] == col && state["grid_dir"] == "asc"
  state["grid_sort"] = col
  state["grid_dir"] = dir
  state["grid_rows"] = grid_sort_rows(state["grid_rows"], col, dir)
  state
end

def gallery_grid_key(state, params)
  key = params["payload"][0]
  return state if state["grid_row"].blank?

  if key == "Enter" || key == "F2"
    state["grid_edit"] = gallery_grid_col_editable(state["grid_col"])
    state["grid_menu"] = gallery_grid_col_options(state["grid_col"]).length() > 0
    return state
  end

  delta = 0
  delta = 1 if key == "ArrowRight"
  delta = -1 if key == "ArrowLeft"
  return state if delta == 0

  col_ids = ["ref", "client", "status", "amount"]
  at = 0
  i = 0
  for cid in col_ids
    at = i if cid == state["grid_col"]
    i = i + 1
  end
  at = at + delta
  at = 0 if at < 0
  at = 3 if at > 3
  state["grid_col"] = col_ids[at]
  state["grid_edit"] = false
  state["grid_menu"] = false
  state
end

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
    "slider_drag": false,
    "media_tab": "Sound",
    "cal_month": "2026-09",
    "cal_date": "",
    "dt_month": "2026-09",
    "dt_date": "2026-09-06",
    "dt_time": "09:30",
    "dt_hour_open": false,
    "dt_min_open": false,
    "range_month": "2026-09",
    "range_start": "",
    "range_end": "",
    "sound": false,
    "video": false,
    "video_at": 0,
    "video_seek": 0,
    "grid_rows": gallery_invoices(),
    "grid_row": "",
    "grid_col": "",
    "grid_edit": false,
    "grid_sort": "",
    "grid_dir": "asc",
    "grid_menu": false,
    "viewport": {
      "width": 1280,
      "height": 800,
      "scale": 1.0,
      "mode": "light",
      "density": "cozy",
      "font_scale": 1.0
    }
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
  kind = params["kind"]
  props = params["props"] ?? {}
  sw = props["width"] ?? 240
  dragging = state["slider_drag"] ?? false
  from_pointer = kind == "click" || kind == "pointer_down" || kind == "pointer_up"
  from_pointer = true if kind == "pointer_move" && dragging
  if from_pointer
    x = payload[0]
    x = 0 if x < 0
    x = sw if x > sw
    state["slider"] = int(x * 100 / sw)
    state["slider_drag"] = kind == "pointer_down" || kind == "pointer_move"
  elsif payload[0] == "ArrowRight"
    state["slider"] = state["slider"] + 5
  elsif payload[0] == "ArrowLeft"
    state["slider"] = state["slider"] - 5
  end
  state["slider"] = 0 if state["slider"] < 0
  state["slider"] = 100 if state["slider"] > 100
  state
end

def set_dt_part(state, which, value)
  bits = (state["dt_time"] ?? "00:00").split(":")
  h = bits[0]
  m = "00"
  m = bits[1] if bits.length() > 1
  h = value if which == "hour"
  m = value if which == "min"
  state["dt_time"] = h + ":" + m
  state["dt_hour_open"] = false
  state["dt_min_open"] = false
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
    "sound" => set_key(state, "sound", !(state["sound"] ?? false)),
    "video" => set_key(
      set_key(state, "video", !(state["video"] ?? false)),
      "video_at",
      state["video"] ?? false ? state["video_at"] : 0
    ),
    "video_time" => set_key(state, "video_at", params["payload"][0]),
    "video_done" => set_key(set_key(state, "video", false), "video_at", 1440),
    "video_scrub" => set_key(
      set_key(state, "video_seek", int(params["payload"][0] * 1440 / 200)),
      "video_at",
      int(params["payload"][0] * 1440 / 200)
    ),
    "sound_ended" => set_key(state, "sound", false),
    "cal_nav" => set_key(state, "cal_month", month_shift(state["cal_month"], props["delta"])),
    "cal_pick" => set_key(state, "cal_date", props["date"]),
    "dt_nav" => set_key(state, "dt_month", month_shift(state["dt_month"], props["delta"])),
    "dt_pick" => set_key(state, "dt_date", props["date"]),
    "dt_hour_toggle" => set_key(
      set_key(state, "dt_hour_open", !(state["dt_hour_open"] ?? false)),
      "dt_min_open",
      false
    ),
    "dt_min_toggle" => set_key(set_key(state, "dt_min_open", !(state["dt_min_open"] ?? false)), "dt_hour_open", false),
    "dt_hour" => set_dt_part(state, "hour", props["value"]),
    "dt_min" => set_dt_part(state, "min", props["value"]),
    "range_nav" => set_key(state, "range_month", month_shift(state["range_month"], props["delta"])),
    "range_pick" => pick_range(state, props["date"]),
    "media_tab" => set_key(set_key(set_key(state, "media_tab", props["option"]), "sound", false), "video", false),
    "grid_select" => gallery_grid_select(state, params),
    "grid_change" => gallery_grid_change(state, params),
    "grid_sort" => gallery_grid_sort(state, props["col"]),
    "grid_key" => gallery_grid_key(state, params),
    "connect" => set_key(state, "viewport", params["viewport"] ?? state["viewport"]),
    "viewport" => set_key(state, "viewport", params["viewport"] ?? state["viewport"]),
    _ => state,
  }
end

def toggle_id(ids, id)
  return ids.filter(fn(x) { x != id }) if ids.includes?(id)

  ids.concat([id])
end

def gallery_controls(state)
  card(
    {"gap": 3, "width": "100%"},
    [
      text("Controls", {"weight": "bold"}),
      row(
        {
          "gap": 6,
          "wrap": "wrap",
          "align": "start",
          "width": "100%"
        },
        [
          column({"gap": 2}, [
            text("Select", {"weight": "bold"}),
            select(["Small", "Medium", "Large"], state["select_value"], state["select_open"], "select_toggle", "select_pick")
          ]),
          column(
            {"gap": 2, "grow": 1},
            [
              text("Slider", {"weight": "bold"}),
              keyed("gallery_slider", slider(state["slider"], 0, 100, "slider")),
              keyed("gallery_slider_value", muted("Value " + str(state["slider"])))
            ]
          )
        ]
      )
    ]
  )
end

# The same 32-pixel avatar three times, as the three formats a picture may
# arrive in (03 §1). The client tells them apart by their first bytes; the
# server only ever sends a hash.
def gallery_pictures
  row(
    {
      "gap": 3,
      "align": "center",
      "wrap": "wrap"
    },
    [
      avatar("public/images/avatar.png", 32),
      avatar("public/images/avatar.jpg", 32),
      avatar("public/images/avatar.webp", 32),
      muted("PNG · JPEG · WebP")
    ]
  )
end

def gallery_media(state)
  kind = state["media_tab"] ?? "Sound"
  body = gallery_sound(state)
  body = gallery_video(state) if kind == "Video"
  card(
    {"gap": 3, "width": "100%"},
    [text("Media", {"weight": "bold"}), segmented(["Sound", "Video"], kind, "media_tab"), body, gallery_pictures()]
  )
end

def gallery_calendar(state)
  w = (state["viewport"] ?? {})["width"] ?? 1280
  cols = 1
  cols = 2 if bp_min(w, "sm")
  cols = 3 if bp_min(w, "md")
  pickers = [
    column(
      {
        "gap": 2,
        "width": "100%",
        "pad": [0, 4, 0, 4]
      },
      [text("Date", {"weight": "bold"}), date_picker(state["cal_month"], state["cal_date"], "cal_pick", "cal_nav")]
    ),
    column(
      {
        "gap": 2,
        "width": "100%",
        "pad": [0, 4, 0, 4]
      },
      [
        text("DateTime", {"weight": "bold"}),
        datetime_picker(
          state["dt_month"],
          state["dt_date"],
          state["dt_time"],
          state["dt_hour_open"] == true,
          state["dt_min_open"] == true,
          "dt_pick",
          "dt_nav",
          "dt_hour_toggle",
          "dt_min_toggle",
          "dt_hour",
          "dt_min"
        )
      ]
    ),
    column(
      {
        "gap": 2,
        "width": "100%",
        "pad": [0, 4, 0, 4]
      },
      [
        text("Range", {"weight": "bold"}),
        date_range_picker(state["range_month"], state["range_start"], state["range_end"], "range_pick", "range_nav")
      ]
    )
  ]
  card(
    {"gap": 3, "width": "100%"},
    [
      text("Calendar", {"weight": "bold"}),
      {
        "k": "box",
        "s": {
          "display": "grid",
          "gap": 5,
          "width": "100%"
        },
        "p": {"columns": cols},
        "c": pickers
      }
    ]
  )
end

def gallery_charts(state)
  view = state["viewport"] ?? {}
  w = view["width"] ?? 1280
  density = view["density"] ?? "cozy"
  cols = 1
  cols = 2 if bp_min(w, "sm")
  cols = 3 if bp_min(w, "md")
  cols = 4 if bp_min(w, "lg")
  # A canvas is drawn at the size the server picks, so the width has to be
  # the one the layout will hand it: the window, less the page's padding,
  # less the card's, less the gaps between the columns.
  gap = space_px(4, density)
  inner = w - 2 * space_px(bp_min(w, "md") ? 6 : 4, density) - 2 * space_px(5, density)
  cw = int((inner - (cols - 1) * gap) / cols)
  cw = 160 if cw < 160
  ch = 140
  series = [3, 5, 4, 8, 6, 9, 7]
  plots = [
    column({"gap": 2}, [text("Line", {"weight": "bold"}), chart_line(series, cw, ch)]),
    column({"gap": 2}, [text("Area", {"weight": "bold"}), chart_area([
      2,
      4,
      3,
      6,
      5,
      8,
      9
    ], cw, ch)]),
    column({"gap": 2}, [text("Bars", {"weight": "bold"}), chart_bar([
      4,
      7,
      3,
      8,
      5,
      6
    ], cw, ch)]),
    column({"gap": 2}, [text("Donut", {"weight": "bold"}), chart_donut([
      5,
      3,
      2,
      1
    ], ch, ch)])
  ]
  card(
    {"gap": 3, "width": "100%"},
    [
      text("Charts", {"weight": "bold"}),
      {
        "k": "box",
        "s": {
          "display": "grid",
          "gap": 4,
          "width": "100%"
        },
        "p": {"columns": cols},
        "c": plots
      }
    ]
  )
end

# What sits beside the progress bar: a percentage, a spinner and three
# labels. It refuses to shrink, so the row spends its narrowness on the
# bar; if the labels still do not fit on one line they wrap among
# themselves rather than breaking a word.
def progress_legend
  row(
    {
      "gap": 2,
      "align": "center",
      "wrap": "wrap",
      "shrink": 0
    },
    [muted("62 %"), spinner(), badge("beta", "warning"), chip("keyed", "", {}), chip("removable", "noop", {"id": 1})]
  )
end

def gallery_view(raw_state)
  state = gallery_defaults(raw_state ?? {})
  tab = state["tab"]
  open = state["open"]
  page = state["page"]
  seg = state["seg"]
  sheet_open = state["sheet"]
  tree_open = state["tree_open"]
  w = (state["viewport"] ?? {})["width"] ?? 1280
  wide = bp_min(w, "md")
  invoices = card(
    {"gap": 3, "width": "100%"},
    [
      text("Grid", {"weight": "bold"}),
      data_grid(
        gallery_grid_visible_columns(w),
        state["grid_rows"],
        state["grid_row"].present? ? {
          "row": state["grid_row"],
          "col": state["grid_col"]
        } : {},
        state["grid_edit"] == true ? {
          "row": state["grid_row"],
          "col": state["grid_col"],
          "open": state["grid_menu"]
        } : {},
        state["grid_sort"].present? ? {
          "col": state["grid_sort"],
          "dir": state["grid_dir"]
        } : {},
        "grid_select",
        "grid_sort",
        "grid_change",
        "grid_key"
      ),
      muted(gallery_grid_caption(state)),
      muted("bp " + bp(w))
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
  structure = [
    column(
      {"gap": 3, "grow": 1},
      [
        accordion(sections, open, "toggle"),
        code_block("router_eui(gallery, live#gallery, live#gallery_view)  # config/routes.sl"),
        h2("Code Viewer:"),
        code_viewer(
          "def hello(name)\n  puts(\"Hello, #{name}!\")\nend\n\ndef world\n  puts(\"World\")\nend\n\nhello(\"Alice\")\nhello(\"Bob\")\nworld()",
          {"line_numbers": true}
        )
      ]
    ),
    column(
      {"gap": 3, "width": wide ? 240 : "100%"},
      [
        card({"gap": 2}, [h2("Tree"), tree_view(tree, tree_open, "tree", 0)]),
        menu(["Rename", "Duplicate", "Delete"], "noop"),
        tooltip("A tooltip")
      ]
    )
  ]
  page_content = column(
    {"gap": wide ? 5 : 3, "pad": wide ? 6 : 4},
    [
      navbar("EUI", ["Overview", "Inputs", "Data"], tab, "tab"),
      tabs(["Overview", "Inputs", "Data"], tab, "tab"),
      row(
        {
          "gap": 3,
          "width": "100%",
          "wrap": "wrap"
        },
        [
          tile(200, stat("Nodes", "14", "primitives")),
          tile(200, stat("Roles", "28", "colours")),
          tile(200, stat("Tests", "181", "and counting"))
        ]
      ),
      banner("This gallery is served by Soli and drawn by EUI.", "info", "Open sheet", "sheet"),
      wide ? row(
        {
          "gap": 3,
          "align": "center",
          "width": "100%"
        },
        [progress(0.62), progress_legend()]
      ) : column(
        {"gap": 2, "width": "100%"},
        [progress(0.62), progress_legend()]
      ),
      row(
        {
          "gap": 3,
          "align": "center",
          "wrap": "wrap"
        },
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
      gallery_controls(state),
      gallery_media(state),
      gallery_calendar(state),
      invoices,
      gallery_charts(state),
      wide ? row(
        {"gap": 4, "align": "start"},
        structure
      ) : column({"gap": 4}, structure),
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
  # None of the three shrinks, so a narrow window moves them onto
  # lines of their own rather than breaking their words.

  # Three tiles that wrap on their own: a line holds as many as fit at
  # 200 px each and they share the remainder, so the last one never
  # leaves a hole behind it.

  # The bar takes the room the legend does not want: the legend never
  # shrinks, so "62 %" keeps its two words on one line, and below md
  # the bar goes above it instead of squeezing it.

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

# How many posts a real timeline asks X for. The endpoint takes 5 to 100;
# fifty is two screens of scrolling and a fifth of a Basic day's quota.
FEED_LIVE = 50

# A card's media, decided by its number alone so the row heights cost an
# arithmetic each and no card has to be built to know them.
def feed_media(i)
  return "video" if i % 11 == 0
  return "audio" if i % 7 == 0
  return "image" if i % 3 == 0

  ""
end

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
    "n": i + 1,
    "media": feed_media(i),
    "image": feed_media(i) == "image" ? "public/images/feed/" + str((i * 7) % 8) + ".png" : nil
  }
end

# Four heights: text only, a sound, a picture, a moving picture.
def feed_height_of(media)
  return 316 if media == "image"
  return 356 if media == "video"
  return 184 if media == "audio"

  128
end

def feed_card_height(post)
  feed_height_of(post["media"])
end

# The feed is windowed (spec 04 §7.1): the client says which rows are in
# view, the server builds those cards and no other. Forty thousand posts
# cost the client forty thousand row heights and the server one window.
def feed(event_data)
  event = event_data["event"]
  params = event_data["params"]
  state = event_data["state"] ?? {}
  liked = state["liked"] ?? []
  posts = state["posts"] ?? []
  trouble = state["trouble"] ?? ""
  source = state["source"] ?? "x.com"
  # A real timeline is fetched when the window opens and when it is asked
  # for, never on a scroll: X allows five reads a quarter of an hour, and
  # a scroll can ask for a hundred windows in that time. What comes back
  # says why when it is empty, so a feed that quietly shows the sample is
  # never a mystery.
  if x_linked() && (event == "connect" || event == "refresh")
    answer = x_timeline(FEED_LIVE)
    posts = answer["posts"]
    trouble = answer["error"]
    source = answer["source"] ?? "x.com"
  end
  count = posts.length() > 0 ? posts.length() : (state["count"] ?? 10)
  window = state["window"] ?? [0, 0]
  sound = state["sound"] ?? -1
  moving = state["moving"] ?? -1
  at = state["at"] ?? 0
  duration = state["duration"] ?? 0
  seek = state["seek"] ?? 0
  keep = {
    "liked": liked,
    "posts": posts,
    "trouble": trouble,
    "source": source,
    "count": count,
    "window": window,
    "sound": sound,
    "moving": moving,
    "at": at,
    "duration": duration,
    "seek": seek
  }
  match event {
    "like" => keep.merge({"liked": toggle_id(liked, params["props"]["id"])}),
    "more" => keep.merge({"count": posts.length() > 0 ? count : count + 5000}),
    "window" => keep.merge({"window": params["payload"]}),
    "play" => keep.merge({"sound": sound == params["props"]["id"] ? -1 : params["props"]["id"]}),
    "sound_ended" => keep.merge({"sound": -1}),
    "video_play" => keep.merge({
      "moving": moving == params["props"]["id"] ? -1 : params["props"]["id"],
      "at": 0,
      "seek": 0
    }),
    "video_time" => keep.merge({
      "at": params["payload"][0],
      "duration": params["payload"][1]
    }),
    "video_ended" => keep.merge({
      "moving": -1,
      "at": duration
    }),
    "video_seek" => video_seek(keep, params),
    _ => keep,
  }
end
# A picture plays only when asked, and only one at a time.

# The height of row `i`, without building the post: what the client needs
# for every row, so the scroll extent and the row tops are exact.
def feed_height(i)
  feed_height_of(feed_media(i))
end

FEED_HEIGHTS = {}

def feed_heights(count)
  cached = FEED_HEIGHTS[count]
  return cached unless cached.nil?

  heights = range(0, count).map(fn(i) { feed_height(i) })
  FEED_HEIGHTS[count] = heights
  heights
end

# Cards are pure functions of (id, liked): built once, kept while they are
# near the window. The view is still a function of state; this only spares
# the interpreter the work of rebuilding identical hashes on every event.
FEED_CARDS = {}

# A click on the bar: its x over its width, times the length. The bar
# says how wide it is, so the two never disagree.
def video_seek(keep, params)
  width = params["props"]["w"] ?? 300
  duration = keep["duration"] ?? 0
  ms = duration > 0 ? int(params["payload"][0] * duration / width) : 0
  keep.merge({"seek": ms, "at": ms})
end

# A card from a post X sent, rather than one invented from its number.
def feed_live_card(i, post, liked, play)
  built = keyed(i, post_card(post, liked, play, feed_card_height(post)))
  built["p"] = {"row": i}
  built
end

def feed_build(i, liked, play)
  post = feed_post(i)
  built = keyed(i, post_card(post, liked, play, feed_card_height(post)))
  built["p"] = {"row": i}
  built
end

# A card that is playing something changes four times a second; caching
# it would fill the cache with one entry per position. The others are
# pure functions of (id, liked) and are kept.
def feed_card(i, liked, play)
  return feed_build(i, liked, play) if (play["sound"] ?? false) || (play["video"] ?? false)

  key = str(i) + (liked ? ":liked" : "")
  cached = FEED_CARDS[key]
  return cached unless cached.nil?

  built = feed_build(i, liked, play)
  FEED_CARDS[key] = built
  built
end

# Cards far from the window are let go: the cache holds a few windows,
# not the feed.
def feed_prune(first, last)
  return if FEED_CARDS.size() < 400

  keep = {}
  for key in FEED_CARDS.keys()
    i = int(key.split(":")[0])
    keep[key] = FEED_CARDS[key] if i >= first - 100 && i <= last + 100
  end
  FEED_CARDS = keep
end

def feed_view(state)
  liked = state["liked"] ?? []
  posts = state["posts"] ?? []
  live = posts.length() > 0
  count = live ? posts.length() : (state["count"] ?? 10)
  window = state["window"] ?? [0, 0]
  first = window[0]
  last = window[1] < count ? window[1] : count - 1
  feed_prune(first, last)
  sound = state["sound"] ?? -1
  moving = state["moving"] ?? -1
  at = state["at"] ?? 0
  duration = state["duration"] ?? 0
  seek = state["seek"] ?? 0
  cards = last < first ? [] : range(first, last + 1).map(fn(i) {
    play = {
      "sound": i == sound,
      "video": i == moving,
      "at": i == moving ? at : 0,
      "duration": i == moving ? duration : 0,
      "seek": i == moving ? seek : 0
    }
    # Fifty real cards are cheap to build and change under the account;
    # forty thousand invented ones are what the cache is for.
    live ? feed_live_card(i, posts[i], liked.includes?(i), play) : feed_card(i, liked.includes?(i), play)
  })
  heights = live ? posts.map(fn(post) { feed_card_height(post) }) : feed_heights(count)
  # An account is linked, so a Refresh is what the header offers; the
  # sample instead offers more of itself.
  trouble = state["trouble"] ?? ""
  linked = live || trouble != ""
  tail = linked ? loading_button("Refresh", "refresh", "refresh") : loading_button("Load 5 000 more", "more", "more")
  # What the header says is what the feed is: an account's timeline, the
  # reason there is none, or the sample that needs no account at all.
  label = live ? str(count) + " posts from " + (state["source"] ?? "x.com") : str(count) + " posts"
  label = trouble if trouble != ""
  tone = live ? "success" : "info"
  tone = "danger" if trouble != ""
  header = row(
    {
      "gap": 3,
      "align": "center",
      "pad": [3, 4, 3, 4],
      "bg": "surface.raised",
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    [h1("Feed"), badge(label, tone), spacer(), tail]
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
      [header, list_window({"grow": 1}, 128, count, heights, cards, "window")]
    )]
  )
end
