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
  # An icon button is announced by its name, not by the character it draws:
  # "Remove Write the spec", rather than "×".
  remove = icon_button("×", "remove", {"id": it["id"]}, {
    "tone": "ghost",
    "icon": "close",
    "name": "Remove " + it["title"]
  })
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

def toggle_id(ids, id)
  return ids.filter(fn(x) { x != id }) if ids.includes?(id)

  ids.concat([id])
end

# One photograph, encoded three ways — the three formats a picture may
# arrive in (03 §1). The client tells them apart by their first bytes; the
# server only ever sends a hash. Showing the same image three times is the
# point: identical output, and the byte counts underneath are the whole
# argument for choosing between them.
def gallery_picture_card(label, src, weight, wide)
  column(
    {"gap": 1, "grow": 1, "basis": wide ? 200 : "100%", "min_width": 160},
    [
      {
        "k": "image",
        "p": {"src": src},
        "s": {"width": "100%", "height": 132, "radius": 2, "overflow": "clip"}
      },
      row({"gap": 2, "align": "center"}, [
        text(label, {"weight": "semibold", "size": 1, "grow": 1}),
        muted(weight)
      ])
    ]
  )
end

def gallery_pictures(wide)
  column({"gap": 2}, [
    row(
      {"gap": 3, "wrap": "wrap", "align": "start"},
      [
        gallery_picture_card("PNG", "public/images/formats/sample.png", "472 KB", wide),
        gallery_picture_card("JPEG", "public/images/formats/sample.jpg", "72 KB", wide),
        gallery_picture_card("WebP", "public/images/formats/sample.webp", "60 KB", wide)
      ]
    ),
    muted("The same photograph, decoded from three formats by the client.")
  ])
end

# The viewer, with the editor's tokeniser doing the colouring: one text
# node carrying a `spans` prop, so the code stays a single run and its
# lines stay level with the gutter counting them.
def gallery_code_viewer
  source = "# Revenue, as this company counts it: invoiced lines, less credit\n# notes, in the period the invoice was issued in — not the order.\ndef revenue(period)\n  lines = Invoice.issued_in(period).lines()\n  net = lines.sum(fn(l) { l.qty * l.unit })\n  net - credit_notes(period)\nend\n\ndef credit_notes(period)\n  CreditNote.issued_in(period).sum(fn(c) { c.total })\nend\n\nrevenue(\"2026-09\")"
  code_viewer(source, {"line_numbers": true, "spans": code_spans(source)})
end

# A page of documentation, rendered from markdown by `markdown_builders.sl`.
# The sample exercises what this project's own docs actually lean on: a lot
# of inline code, tables, and the occasional fence.
# The editor, in a card. It is the `editor` component's own code — the same
# buffer in the session's state, the same round trip per keystroke, the same
# tokeniser — told to name the three handlers the gallery answers to and to
# stop after twelve lines. What it opens is a sample, not a file: a gallery
# card that could rewrite the application it is in would be a different kind
# of demonstration.
def gallery_editor_lines
  sample = "# The rule every quote is priced by. Type in it: every keystroke is\n# a round trip — the client sends `key_down`, the server edits the\n# buffer and sends back the line that changed and the cursor that\n# moved. Nothing here is a text widget: the gutter, the colours and\n# the caret are the view's, and the highlighting happened on the server.\ndef price(quote)\n  base = quote.part.list_price * quote.qty\n  quote.tier == \"Gold\" ? base * 0.88 : base\nend\n\nreprice(all)"
  sample.split("\n")
end

def gallery_editor_state(state)
  seat = state["ed"] ?? {}
  # Seeded on the first look rather than in `gallery_defaults`, so the sample
  # is split once a session and not once an event. Without a buffer of its
  # own `ed_defaults` would go and read the editor's source file.
  seat = {
    "lines": gallery_editor_lines(),
    "name": "sample.sl",
    "message": "A sample, not a file. Nothing is written anywhere."
  } if seat["lines"].nil?
  seat["viewport"] = state["viewport"] ?? {}
  ed_defaults(seat)
end

def gallery_editor_reset(state)
  seat = gallery_editor_state(state)
  seat["lines"] = gallery_editor_lines()
  seat["row"] = 0
  seat["col"] = 0
  seat["dirty"] = false
  seat["message"] = "Back to the sample."
  seat
end

def gallery_editor(state)
  frame = {
    "k": "box",
    "s": {
      "display": "column",
      "width": "100%",
      "radius": 2,
      "overflow": "clip",
      "border": 1,
      "border_color": "border.subtle"
    },
    "c": [ed_panel(
      gallery_editor_state(state),
      {"key": "ed_key", "click": "ed_click", "reload": "ed_reload", "lines": 12, "clean": "unchanged"}
    )]
  }
  card(
    {"gap": 3, "width": "100%"},
    [
      row(
        {"gap": 3, "align": "center", "width": "100%"},
        [text("Pricing rule", {"weight": "bold", "grow": 1}), badge("runs on every quote", "info")]
      ),
      frame
    ]
  )
end

def gallery_markdown
  doc = "## Month end, in nine steps\n\nThe close is **the fourth working day**. Nothing below is automatic; the\nchecklist is the record that somebody did it.\n\n| Step | Owner | Cut-off |\n|---|---|---|\n| Post the last goods receipt | Warehouse | D-1, 18:00 |\n| Reconcile stock movements | Nightly job | D, 03:00 |\n| Issue outstanding invoices | Billing | D, 12:00 |\n\n- A period that is reopened is reported to the auditor, every time.\n- Rebates are accrued against the quarter, not the month they land in.\n\n> Anything unreconciled at D+2 goes to the controller, not into the close.\n\n```\nsoli jobs run close --period 2026-09\n```\n"
  card({"gap": 3}, [markdown(doc, {})])
end

# The document, in a dialog wide enough to read it in. Only ever built
# through `lazy`, so `markdown_file` — and the parse behind it — happens on
# the click that opens this and never before.
def gallery_doc(state)
  # The document as a windowed list, 04 §7.1: the client holds a height
  # for every block and lays out the ones in view, so opening a page costs
  # a window of blocks however long the page is. `doc_window` is what the
  # client last asked for; the rows it names are the only ones built.
  # The dialog takes the window it is in: as wide as 860 when there is
  # room, the window less a margin when there is not, and as tall as the
  # window leaves once the title and the Close row have had theirs. The
  # rows are guessed at the width they will be laid out in.
  doc_vw = (state["viewport"] ?? {})["width"] ?? 1280
  doc_vh = (state["viewport"] ?? {})["height"] ?? 800
  doc_width = doc_vw - 32 < 860 ? doc_vw - 32 : 860
  doc_width = 320 if doc_width < 320
  doc_height = doc_vh - 200
  doc_height = 240 if doc_height < 240
  # And no taller than a page one can read: a dialog that took a tall
  # window whole would be a wall of text with a Close button under it.
  doc_height = 720 if doc_height > 720
  doc = md_doc_rows("app/docs/components.md", doc_width - 60)
  doc_count = doc["rows"].length()
  doc_window = state["doc_window"] ?? [0, 24]
  doc_first = doc_window[0] ?? 0
  doc_last = doc_window[1] ?? 24
  doc_last = doc_count - 1 if doc_last > doc_count - 1
  # Each block rides in a box that names its row. A box, not the block:
  # a divider is an inert kind and may carry nothing, not even a prop, and
  # the cached block is shared by every window that shows it.
  doc_rows = doc_first > doc_last ? [] : range(doc_first, doc_last + 1).map(fn(i) {
    {"k": "box", "s": {"display": "column", "width": "100%"}, "p": {"row": i}, "c": [doc["rows"][i]]}
  })
  dialog(
    "EUI components",
    [list_window({"grow": 1, "max_height": doc_height, "gap": 3}, 24, doc_count, doc["heights"], doc_rows, "doc_window")],
    [{
      "k": "box",
      "s": {"display": "row", "justify": "center", "align": "center", "pad": [2, 4, 2, 4], "min_width": 44, "bg": "accent.base", "fg": "accent.on", "border": 1, "radius": 2, "cursor": "pointer"},
      "p": {"id": "doc"},
      "on": {"click": "lazy_close"},
      "c": [text("Close", {"weight": "semibold"})]
    }],
    {"width": doc_width}
  )
end

# A split whose panels answer their own width, not the window's. The outer
# divider is vertical; the left panel holds a second, horizontal one. Each
# panel is built by a function of its own extent in pixels, so `bp_min` here
# reads the panel and not the viewport — which is the whole point of handing
# the size back to the caller.
# The container's own width, wanted in two places: the view, which lays the
# split out, and the handler, which turns a pointer position into a fraction.
# One function, so a drag can never be measured against a width the view did
# not use.
def gallery_split_extent(state)
  w = state["viewport"]["width"] ?? 1000
  extent = w > 900 ? 820 : w - 80
  extent < 320 ? 320 : extent
end

def gallery_split(state)
  extent = gallery_split_extent(state)

  column({"gap": 3}, [
    h2("Split panes"),
    muted("Drag either divider. Tab to one and use the arrow keys."),
    split_pane({
      "key": "gsplit",
      "dir": "row",
      "size": extent,
      "cross": 260,
      "fraction": state["split_x"],
      "min_a": 160,
      "min_b": 200,
      "on_drag": "split_x",
      "dragging": state["split_x_drag"],
      "label": "Resize the sidebar",
      "a": fn(px) {
        split_pane({
          "key": "gsplit_y",
          "dir": "column",
          "size": 260,
          "cross": px,
          "fraction": state["split_y"],
          "min_a": 70,
          "min_b": 70,
          "on_drag": "split_y",
          "dragging": state["split_y_drag"],
          "label": "Resize the filter list",
          "a": fn(ph) { gallery_split_pane("Filters", px, ph, "info") },
          "b": fn(ph) { gallery_split_pane("Saved", px, ph, "success") }
        })
      },
      "b": fn(px) { gallery_split_detail(px) }
    })
  ])
end

# The content genuinely changes shape with the panel: below 220 px it drops to
# a single stacked column and the badge goes away.
def gallery_split_pane(title, px, ph, tone)
  roomy = pane_min(px, "md")
  head = roomy ? row({"gap": 2, "align": "center"}, [
    text(title, {"weight": "semibold"}),
    spacer(),
    badge(pane_bp(px), tone)
  ]) : text(title, {"weight": "semibold"})

  column({"pad": [2, 3, 2, 3], "gap": 2, "bg": "surface.raised", "height": ph}, [
    head,
    muted(px.to_s + " x " + ph.to_s)
  ])
end

# The detail pane goes from one column to two as soon as it has the room for
# them, on its own breakpoint and nobody else's.
def gallery_split_detail(px)
  cells = [
    stat("Rows", "10,024", "invoices"),
    stat("Open", "312", "unpaid")
  ]
  body = pane_min(px, "lg") ? row({"gap": 3}, cells) : column({"gap": 3}, cells)
  column({"pad": [3, 3, 3, 3], "gap": 3, "bg": "surface.base", "height": 260}, [
    row({"gap": 2, "align": "center"}, [
      text("Detail", {"weight": "semibold"}),
      spacer(),
      muted(pane_bp(px) + " · " + px.to_s + " px")
    ]),
    divider(),
    body
  ])
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
    [
      muted("62 %"),
      spinner(),
      badge("on track", "success"),
      chip("Lyon", "", {}),
      chip("excl. rebates", "chip_drop", {"key": "rebates"})
    ]
  )
end

# ---- Meridian Industrial ---------------------------------------------------
#
# The gallery is an application now. Every widget the catalogue has is still
# on this page, but it is on it the way a working system would have it: six
# sections behind a rail, a parts distributor's figures inside them, and forms
# that judge what is typed into them. The data is `erp_data.sl`, and none of
# it is real.
#
# Which section is up is `state["section"]` and nothing else. Navigation that
# lived in the client would be navigation the server could not answer for —
# no deep link, no permission check, and a back button that lies. One `scroll`
# holds the active section; the rail and the top bar are outside it, so the
# page moves under a header that stays.

ERP_SECTIONS = ["Dashboard", "Orders", "Customers", "Inventory", "Reports", "Settings"]

ERP_PER_PAGE = 7

# The three shapes this page takes, decided once so that the view, the charts
# and the split handler cannot disagree about them.
def erp_layout(state)
  view = state["viewport"] ?? {}
  w = view["width"] ?? 1280
  {
    "w": w,
    "density": view["density"] ?? "cozy",
    "rail": bp_min(w, "lg"),
    "wide": bp_min(w, "md"),
    "roomy": bp_min(w, "sm")
  }
end

# The width the content actually gets. A canvas is painted at the size the
# *server* chose (04 §6), so every chart on this page is drawn against this
# number — and the rail comes out of it, or every chart is 200 px too wide
# the moment the window is big enough to show one.
def erp_content_px(lay)
  lay["rail"] ? lay["w"] - 201 : lay["w"]
end

# ---- The shell -------------------------------------------------------------

def erp_topbar(state, lay)
  section = state["section"] ?? "Dashboard"
  account = popover(
    icon_button("⋯", "acct_toggle", {}, {
      "icon": "more_v",
      "name": "Account",
      "key": "erp_acct",
      "expanded": state["acct_open"] == true
    }),
    [menu(["Profile", "Preferences", "Sign out"], "acct_pick")],
    state["acct_open"] == true
  )
  crumbs = breadcrumb([{"label": "Meridian", "path": "Dashboard"}, {
    "label": section,
    "path": section
  }], "nav")
  # The menu button only appears where it is the *only* way to the sections.
  # Above `sm` the navbar above already lists all six, and a hamburger next
  # to a visible menu is furniture.
  head = lay["roomy"] ? crumbs : row(
    {"gap": 2, "align": "center", "shrink": 0},
    [icon_button("≡", "nav_toggle", {}, {"icon": "menu", "name": "Sections", "key": "erp_nav"}), crumbs]
  )
  tail = [
    segmented(["Day", "Week", "Month"], state["seg"], "seg"),
    theme_toggle(),
    account
  ]
  tail = [sized_input(state["search"] ?? "", "search", 200)].concat(tail) if lay["wide"]
  toolbar([head, spacer()].concat(tail))
end

# The rail: where you are, what you can reach from here, and whose session
# this is. The profile belongs at the bottom of it — an application people
# sign into says who they are signed in as somewhere, and the top bar is
# already carrying the section, the search and the period.
#
# `sidebar` still draws the links; it is stripped of its own surface and
# border here because the rail around it now owns those, and a panel with
# two borders down one edge is a panel with a seam in it.
def erp_rail(state)
  links = restyle(
    sidebar(ERP_SECTIONS, state["section"], "nav"),
    {"width": "100%", "bg": "none", "border": 0, "pad": 2}
  )
  column(
    {
      "gap": 0,
      "width": 200,
      "height": "100%",
      "bg": "surface.raised",
      "border": [0, 1, 0, 0],
      "border_color": "border.subtle"
    },
    [
      row(
        {"gap": 2, "align": "center", "pad": [3, 3, 2, 3], "width": "100%"},
        [initial_avatar("M", "accent.base", 24), text("Meridian", {"weight": "bold"})]
      ),
      links,
      spacer(),
      divider(),
      erp_profile(state)
    ]
  )
end

# Who is signed in, and the one control that belongs beside them. The way
# out is an icon and not a word: it sits on the baseline of the name it
# belongs to, and a rail this narrow has no room for a second row of links.
def erp_profile(state)
  row(
    {"gap": 2, "align": "center", "width": "100%", "pad": [3, 3, 3, 3]},
    [
      initial_avatar("C", "info.base", 28),
      column(
        {"gap": 0, "grow": 1},
        [text("Camille Roy", {"weight": "semibold", "size": 1}), muted("Sales · Lyon")]
      ),
      icon_button("⇥", "sign_out", {}, {
        "icon": "logout",
        "name": "Sign out",
        "key": "erp_signout",
        "size": "sm"
      })
    ]
  )
end

# The rail, or — narrow — a navbar with the sections in a drawer behind it.
# The drawer is a layer, so it is composed by the view, not by the shell.
def erp_shell(state, lay, page)
  content = column({"gap": 0, "grow": 1, "height": "100%"}, [erp_topbar(state, lay), page])
  return row(
    {"gap": 0, "width": "100%", "height": "100%"},
    [erp_rail(state), content]
  ) if lay["rail"]

  # Below `sm` the six section names cannot share a line and start breaking
  # inside their own words, so the navbar goes and the menu button in the
  # top bar — which is only drawn at this size — is the way to them.
  return column({"gap": 0, "width": "100%", "height": "100%"}, [content]) unless lay["roomy"]

  column(
    {"gap": 0, "width": "100%", "height": "100%"},
    [navbar("Meridian", ERP_SECTIONS, state["section"], "nav"), content]
  )
end

# A card with a title, an optional aside, and a body — the shape nearly every
# card on this page has, written once.
def erp_card(title, aside, children)
  head = row(
    {"gap": 3, "align": "center", "width": "100%"},
    [text(title, {"weight": "bold", "grow": 1})].concat(aside)
  )
  card({"gap": 3, "width": "100%"}, [head].concat(children))
end

# ---- Dashboard -------------------------------------------------------------

def erp_dashboard(state, lay)
  panels = [erp_activity_card(state), erp_pipeline_card(state)]
  column(
    {"gap": lay["wide"] ? 5 : 3, "width": "100%"},
    [
      erp_kpis(state, lay),
      banner(
        "Three invoices are past due and six parts are under their reorder point.",
        "warning",
        "Open the queue",
        "sheet"
      ),
      erp_target(state, lay),
      erp_charts(state, lay),
      lay["wide"] ? row({"gap": 4, "align": "start", "width": "100%"}, panels) : column({"gap": 4, "width": "100%"}, panels),
      erp_forecast(state, lay),
      erp_release(state, lay)
    ]
  )
end

def erp_kpis(state, lay)
  row(
    {"gap": 3, "width": "100%", "wrap": "wrap"},
    [
      tile(200, stat("Revenue, month to date", "412 380 €", "+8.4 % on August")),
      tile(200, stat("Orders", "1 284", "63 open · 9 late")),
      tile(200, stat("Open invoices", "37", "128 940 € outstanding")),
      tile(200, erp_margin_card())
    ]
  )
end

# The one KPI that is not a `stat`: it carries a tooltip, and `stat` takes
# three strings rather than three nodes on purpose.
def erp_margin_card
  card(
    {"gap": 1, "width": "100%"},
    [
      row({"gap": 2, "align": "center"}, [muted("Gross margin"), tooltip("Before rebates")]),
      text("31.6 %", {"size": 6, "weight": "bold"}),
      text("target 30.0 %", {"fg": "text.muted", "size": 0})
    ]
  )
end

def erp_target(state, lay)
  bar = lay["wide"] ? row(
    {"gap": 3, "align": "center", "width": "100%"},
    [progress(0.62), progress_legend()]
  ) : column({"gap": 2, "width": "100%"}, [progress(0.62), progress_legend()])
  erp_card(
    "Quarter target",
    [muted("Q3 2026 · 2 480 000 €")],
    [bar, divider(), stepper(["Quote", "Confirmed", "Picked", "Invoiced"], 2)]
  )
end

def erp_activity_card(state)
  rows = erp_activity().map(fn(entry) {
    row(
      {"gap": 3, "align": "center", "width": "100%"},
      [
        initial_avatar(entry["initial"], entry["tone"], 28),
        column(
          {"gap": 0, "grow": 1},
          [text(entry["who"], {"weight": "semibold", "size": 1}), muted(entry["what"])]
        ),
        muted(entry["when"])
      ]
    )
  })
  card(
    {"gap": 3, "width": "100%", "grow": 1, "basis": 340},
    [row({"gap": 3, "align": "center"}, [
      text("Today", {"weight": "bold", "grow": 1}),
      badge("live", "success")
    ])].concat(rows)
  )
end

def erp_pipeline_card(state)
  widths = [150, 60, 110]
  stages = [
    ["Quotes", "18", "84 200 €"],
    ["Confirmed", "24", "301 450 €"],
    ["Picked", "12", "96 800 €"],
    ["Invoiced", "9", "42 110 €"]
  ]
  body = stages.map(fn(stage) { table_row("pl-" + stage[0], stage, widths) })
  card(
    {"gap": 3, "width": "100%", "grow": 1, "basis": 340},
    [
      text("Pipeline", {"weight": "bold"}),
      table_header(["Stage", "Orders", "Value"], widths)
    ].concat(body)
  )
end

# Nothing of the forecast exists until it is asked for: `lazy` does not call
# the thunk, so the skeletons cost three boxes and the figures cost nothing.
def erp_forecast(state, lay)
  shown = state["shown"] ?? []
  running = shown.includes?("forecast")
  placeholder = column(
    {"gap": 2, "width": "100%"},
    [skeleton(320, 12), skeleton(280, 12), skeleton(300, 12)]
  )
  erp_card(
    "Forecast",
    [secondary_button(running ? "Clear" : "Run forecast", "forecast_toggle")],
    [lazy("forecast", shown, placeholder, fn() { erp_forecast_body(state, lay) })]
  )
end

def erp_forecast_body(state, lay)
  cells = [
    stat("October", "438 000 €", "+6.2 %"),
    stat("November", "451 500 €", "+3.1 %"),
    stat("December", "402 900 €", "−10.8 %")
  ]
  column(
    {"gap": 3, "width": "100%"},
    [
      lay["roomy"] ? row({"gap": 3, "width": "100%"}, cells) : column({"gap": 3, "width": "100%"}, cells),
      muted("Seasonal model, fitted on 36 months. Nothing here is advice.")
    ]
  )
end

# The release note, and the two media nodes. They draw nothing and play
# nothing until they are asked to, which is why a page can carry them.
def erp_release(state, lay)
  media = [gallery_video(state), gallery_sound(state)]
  erp_card(
    "Release 24.3",
    [badge("new", "info")],
    [
      muted("Stock counts reconcile nightly. The walkthrough is ninety seconds."),
      lay["wide"] ? row({"gap": 5, "align": "start"}, media) : column({"gap": 4}, media)
    ]
  )
end

# ---- Orders ----------------------------------------------------------------

def erp_orders_section(state, lay)
  column(
    {"gap": lay["wide"] ? 5 : 3, "width": "100%"},
    [
      erp_filters(state, lay),
      erp_orders_card(state, lay),
      erp_order_detail(state, lay),
      erp_invoices_card(state, lay)
    ]
  )
end

# Which orders survive the filter bar. Pure, so the count under the table and
# the rows in it cannot disagree.
def erp_orders_rows(state)
  rows = erp_orders(63)
  needle = (state["search"] ?? "").strip().downcase()
  rows = rows.filter(fn(o) {
    o["ref"].downcase().includes?(needle) || o["customer"].downcase().includes?(needle)
  }) unless needle == ""
  status = state["select_value"] ?? "Any status"
  rows = rows.filter(fn(o) { o["status"] == status }) unless status == "Any status"
  rows = rows.filter(fn(o) { o["status"] != "Invoiced" }) if state["unpaid"] == true
  erp_sorted(rows, state["sort_by"] ?? "Due date")
end

def erp_sorted(rows, by)
  return grid_sort_rows(rows, "total", "desc") if by == "Amount"
  return grid_sort_rows(rows, "customer", "asc") if by == "Customer"

  grid_sort_rows(rows, "due", "asc")
end

def erp_page_of(rows, page)
  first = (page - 1) * ERP_PER_PAGE
  last = first + ERP_PER_PAGE
  last = rows.length() if last > rows.length()
  return [] if first >= last

  range(first, last).map(fn(i) { rows[i] })
end

def erp_pages(rows)
  pages = int((rows.length() + ERP_PER_PAGE - 1) / ERP_PER_PAGE)
  pages < 1 ? 1 : pages
end

# The filter bar. Every control here owns one key of the state and nothing
# else, which is why the chips underneath can be built from the state alone.
def erp_filters(state, lay)
  controls = [
    column(
      {"gap": 1},
      [
        muted("Status"),
        select(
          ["Any status", "Draft", "Confirmed", "Picked", "Invoiced", "Late"],
          state["select_value"],
          state["select_open"],
          "select_toggle",
          "select_pick"
        )
      ]
    ),
    column(
      {"gap": 1, "width": 240},
      [date_range_field({
        "label": "Delivery window",
        "start": state["range_start"],
        "finish": state["range_end"],
        "month": state["range_month"],
        "open": state["range_open"] == true,
        "on_toggle": "range_toggle",
        "on_pick": "range_pick",
        "on_nav": "range_nav",
        "placeholder": "Any day",
        "hint": "Two clicks: a start, then an end"
      })]
    ),
    column(
      {"gap": 1},
      [
        muted("Sort by"),
        radio_group(
          ["Due date", "Amount", "Customer"],
          state["sort_by"],
          "sort_pick",
          {"name": "ordersort", "direction": "row", "label": "Sort orders by"}
        )
      ]
    ),
    column(
      {"gap": 2},
      [
        checkbox("Only unpaid", state["unpaid"] == true, "unpaid", {"id": "unpaid"}),
        switch("Group by customer", state["group"] == true, "group", {"id": "group"})
      ]
    )
  ]
  card(
    {"gap": 3, "width": "100%"},
    [
      row({"gap": 5, "align": "start", "wrap": "wrap", "width": "100%"}, controls),
      divider(),
      row({"gap": 2, "align": "center", "wrap": "wrap"}, erp_filter_chips(state))
    ]
  )
end

# What is currently narrowing the list, as chips. The ones that can be
# dropped carry the key they would clear; "63 orders" cannot be dropped, so
# it has no ×.
def erp_filter_chips(state)
  chips = [chip(str(erp_orders_rows(state).length()) + " matching", "", {})]
  status = state["select_value"] ?? "Any status"
  chips = chips.concat([chip(status, "chip_drop", {"key": "select_value"})]) unless status == "Any status"
  chips = chips.concat([chip("unpaid only", "chip_drop", {"key": "unpaid"})]) if state["unpaid"] == true
  chips = chips.concat([chip("grouped", "chip_drop", {"key": "group"})]) if state["group"] == true
  chips = chips.concat([chip("“" + state["search"] + "”", "chip_drop", {"key": "search"})]) unless (state["search"] ?? "") == ""
  window = state["range_start"] ?? ""
  chips = chips.concat([chip(window + " →", "chip_drop", {"key": "range"})]) unless window == ""
  chips
end

def erp_order_widths(lay)
  return [110, 190, 110, 110, 100] if lay["rail"]
  return [110, 170, 110, 100] if lay["wide"]

  [110, 110]
end

def erp_order_labels(lay)
  return ["Order", "Customer", "Status", "Due", "Amount"] if lay["rail"]
  return ["Order", "Customer", "Status", "Amount"] if lay["wide"]

  ["Order", "Amount"]
end

def erp_order_values(order, lay)
  return [order["ref"], order["customer"], order["status"], order["due"], order["amount"]] if lay["rail"]
  return [order["ref"], order["customer"], order["status"], order["amount"]] if lay["wide"]

  [order["ref"], order["amount"]]
end

# A table row is a plain row of text; making one answer the pointer is three
# keys on the node it already returns, and keeps the widths and the hairline
# the header agreed on.
def erp_order_row(order, state, lay)
  line = table_row(order["ref"], erp_order_values(order, lay), erp_order_widths(lay))
  line["on"] = {"click": "order_pick"}
  line["p"] = {"ref": order["ref"]}
  line["s"]["cursor"] = "pointer"
  line["s"]["bg"] = "info.subtle" if state["order_sel"] == order["ref"]
  line
end

def erp_orders_card(state, lay)
  rows = erp_orders_rows(state)
  pages = erp_pages(rows)
  page = state["page"] ?? 1
  page = pages if page > pages
  body = erp_page_of(rows, page).map(fn(order) { erp_order_row(order, state, lay) })
  table = rows.length() == 0 ? empty_state(
    "Nothing matches",
    "No order answers those filters. Clear them to see all sixty-three.",
    "Clear filters",
    "clear_filters"
  ) : column(
    {"gap": 0, "width": "100%"},
    [table_header(erp_order_labels(lay), erp_order_widths(lay))].concat(body)
  )
  erp_card(
    "Orders",
    [
      loading_button("Export", "export", "export"),
      button("New order", "order_new")
    ],
    [table, row({"gap": 3, "align": "center", "justify": "center"}, [pagination(page, pages, "page")])]
  )
end

# What one order is made of, and the menu that acts on it. The menu is a
# `popover` rather than a dialog because it is a list of verbs, not a
# question — and it is the server that says whether it is down.
def erp_order_detail(state, lay)
  ref = state["order_sel"] ?? ""
  return card({"gap": 2, "width": "100%"}, [
    muted("Pick an order above to see its lines.")
  ]) if ref == ""

  widths = lay["wide"] ? [110, 170, 70, 100, 110] : [110, 110]
  labels = lay["wide"] ? ["SKU", "Part", "Qty", "Unit", "Total"] : ["SKU", "Total"]
  lines = erp_lines(ref).map(fn(line) {
    values = lay["wide"] ? [line["sku"], line["part"], line["qty"], line["unit"], line["total"]] : [line["sku"], line["total"]]
    table_row(line["id"], values, widths)
  })
  actions = popover(
    icon_button("⋯", "row_menu", {"ref": ref}, {
      "icon": "more_h",
      "name": "Order actions",
      "key": "erp_row_menu",
      "expanded": state["order_menu"] == true
    }),
    [menu(["Duplicate", "Print", "Delete"], "row_action")],
    state["order_menu"] == true
  )
  erp_card(ref, [badge("Confirmed", "info"), actions], [table_header(labels, widths)].concat(lines))
end

# The invoices, in a grid that sorts by moving keyed rows and edits a cell
# in place. Eight of them: the point is the machinery, not the volume.
def erp_invoices_card(state, lay)
  erp_card(
    "Invoices",
    [muted(gallery_grid_caption(state))],
    [
      data_grid(
        gallery_grid_visible_columns(lay["w"]),
        state["grid_rows"],
        state["grid_row"].present? ? {"row": state["grid_row"], "col": state["grid_col"]} : {},
        state["grid_edit"] == true ? {
          "row": state["grid_row"],
          "col": state["grid_col"],
          "open": state["grid_menu"]
        } : {},
        state["grid_sort"].present? ? {"col": state["grid_sort"], "dir": state["grid_dir"]} : {},
        "grid_select",
        "grid_sort",
        "grid_change",
        "grid_key"
      ),
      muted("Click a cell to select it, again to edit it. " + bp(lay["w"]) + " viewport.")
    ]
  )
end

# The new order, in a sheet: every typed field the catalogue has, doing the
# job it was written for. Nothing is saved anywhere — "Create" raises a
# toast and closes, which is exactly as much as a demonstration should do.
def erp_new_order(state)
  tried = state["nf_tried"] == true
  fields = [
    text_field("Reference", state["nf_ref"], "nf_ref", {
      "required": true,
      "submitted": tried,
      "hint": "Yours, or leave it for the sequence"
    }),
    text_field("Customer", state["nf_customer"], "nf_customer", {"required": true, "submitted": tried}),
    email_field("Contact", state["nf_email"], "nf_email", {
      "required": true,
      "submitted": tried,
      "hint": "The confirmation goes here"
    }),
    number_field("Quantity", state["nf_qty"], "nf_qty", {
      "min": 1,
      "max": 999,
      "step": 5,
      "on_step": "nf_qty_step",
      "hint": "Pallets, not pieces"
    }),
    date_field({
      "label": "Delivery date",
      "value": state["nf_date"],
      "month": state["nf_date_month"],
      "open": state["nf_date_open"] == true,
      "on_toggle": "nf_date_toggle",
      "on_pick": "nf_date_pick",
      "on_nav": "nf_date_nav",
      "hint": "Working days only, in theory"
    }),
    datetime_field({
      "label": "Pickup slot",
      "value": state["nf_slot"],
      "time": state["nf_slot_time"],
      "month": state["nf_slot_month"],
      "open": state["nf_slot_open"] == true,
      "hour_open": state["nf_slot_hour_open"] == true,
      "min_open": state["nf_slot_min_open"] == true,
      "on_toggle": "nf_slot_toggle",
      "on_pick": "nf_slot_pick",
      "on_nav": "nf_slot_nav",
      "on_hour_toggle": "nf_slot_hour_toggle",
      "on_min_toggle": "nf_slot_min_toggle",
      "on_hour": "nf_slot_hour",
      "on_min": "nf_slot_min"
    }),
    textarea_field("Notes", state["nf_notes"], "nf_notes", {
      "rows": 3,
      "hint": "Anything the warehouse should read"
    })
  ]
  sheet(
    "right",
    [
      h2("New order"),
      muted("Nothing here is written anywhere."),
      form(fields, "Create", "order_create")
    ],
    {"label": "New order", "on_close": "order_close"}
  )
end

# ---- Customers -------------------------------------------------------------

def erp_customers_section(state, lay)
  return column(
    {"gap": lay["wide"] ? 5 : 3, "width": "100%"},
    [erp_customer_split(state, lay)]
  ) if lay["wide"]

  # Narrow: the list *or* the detail, because a split pane at 500 px is two
  # columns of nothing.
  column(
    {"gap": 3, "width": "100%"},
    [(state["cust_sel"] ?? "") == "" ? erp_customer_list(state, 0) : erp_customer_detail(state, lay, 0)]
  )
end

def erp_customer_split(state, lay)
  extent = gallery_split_extent(state)
  erp_card("Customers", [muted("14 accounts · 3 owners")], [split_pane({
    "key": "gsplit",
    "dir": "row",
    "size": extent,
    "cross": 560,
    "fraction": state["split_x"],
    "min_a": 220,
    "min_b": 260,
    "on_drag": "split_x",
    "dragging": state["split_x_drag"],
    "label": "Resize the account list",
    "a": fn(px) { erp_customer_list(state, px) },
    "b": fn(px) { erp_customer_detail(state, lay, px) }
  })])
end

def erp_customer_list(state, px)
  selected = state["cust_sel"] ?? ""
  rows = erp_customers().map(fn(one) {
    line = row(
      {
        "gap": 3,
        "align": "center",
        "width": "100%",
        "pad": [1, 3, 1, 3],
        "radius": 1,
        "cursor": "pointer",
        "bg": one["id"] == selected ? "info.subtle" : "none"
      },
      [
        initial_avatar(one["initial"], one["tone"], 24),
        column(
          {"gap": 0, "grow": 1},
          [text(one["name"], {"weight": "semibold", "size": 1}), muted(one["city"] + " · " + one["country"])]
        ),
        badge(one["tier"], one["tier"] == "Gold" ? "warning" : (one["tier"] == "Silver" ? "info" : "success"))
      ]
    )
    line["on"] = {"click": "cust_pick"}
    line["p"] = {"id": one["id"]}
    keyed("cu:" + one["id"], line)
  })
  column({"gap": 1, "width": "100%", "pad": [2, 0, 2, 0], "height": px > 0 ? 560 : "auto"}, [scroll({"grow": 1}, [column({"gap": 1, "width": "100%"}, rows)])])
end

def erp_customer_detail(state, lay, px)
  one = erp_customer(state["cust_sel"] ?? "C-101")
  tab = state["cust_tab"] ?? "Profile"
  body = erp_customer_profile(state, one, lay)
  body = erp_customer_invoices(state, one, lay) if tab == "Invoices"
  body = erp_customer_notes(state, one) if tab == "Notes"
  column(
    {"gap": 3, "width": "100%", "pad": [2, 3, 2, 3], "height": px > 0 ? 560 : "auto"},
    [
      row(
        {"gap": 3, "align": "center", "width": "100%"},
        [
          initial_avatar(one["initial"], one["tone"], 36),
          column(
            {"gap": 0, "grow": 1},
            [text(one["name"], {"weight": "bold", "size": 3}), muted(one["owner"] + " · since " + one["since"])]
          ),
          badge(one["terms"], "info")
        ]
      ),
      tabs(["Profile", "Invoices", "Notes"], tab, "cust_tab"),
      scroll({"grow": 1}, [body])
    ]
  )
end

def erp_customer_profile(state, one, lay)
  cells = [
    stat("Open balance", erp_money(one["open"]), "on " + one["terms"]),
    stat("Orders", str(4 + erp_customer_index(one["id"])), "this quarter"),
    stat("Tier", one["tier"], "since " + one["since"])
  ]
  column(
    {"gap": 4, "width": "100%"},
    [
      lay["roomy"] ? row({"gap": 3, "width": "100%"}, cells) : column({"gap": 3, "width": "100%"}, cells),
      accordion(
        [
          {
            "id": "a",
            "title": "Contacts",
            "body": one["owner"] + " owns this account. Billing goes to accounts@" + one["id"].downcase() + ".test."
          },
          {
            "id": "b",
            "title": "Addresses",
            "body": "Invoices to " + one["city"] + "; deliveries to whichever plant the line names."
          },
          {
            "id": "c",
            "title": "Payment terms",
            "body": one["terms"] + ", reviewed each January. Over the limit, orders need a release."
          }
        ],
        state["open"],
        "toggle"
      ),
      column({"gap": 2, "width": "100%"}, [
        text("Sites", {"weight": "semibold"}),
        tree_view(erp_sites(one), state["tree_open"], "tree", 0)
      ])
    ]
  )
end

def erp_customer_invoices(state, one, lay)
  widths = [110, 110, 110]
  rows = range(0, 5).map(fn(i) {
    table_row(
      one["id"] + "-i" + str(i),
      ["FA-" + str(1200 + erp_customer_index(one["id"]) * 5 + i), erp_day(i * 11), erp_money(400 + i * 317)],
      widths
    )
  })
  column(
    {"gap": 0, "width": "100%"},
    [table_header(["Invoice", "Issued", "Amount"], widths)].concat(rows)
  )
end

def erp_customer_notes(state, one)
  column(
    {"gap": 3, "width": "100%"},
    [
      textarea_field("Account notes", state["cust_notes"], "cust_notes", {
        "rows": 5,
        "hint": "Kept for this session and nowhere else"
      }),
      muted("Last edited by " + one["owner"] + ".")
    ]
  )
end

# ---- Inventory -------------------------------------------------------------

def erp_inventory_section(state, lay)
  column(
    {"gap": lay["wide"] ? 5 : 3, "width": "100%"},
    [erp_stock_card(state, lay), erp_reorder_card(state, lay), erp_shortages_card(state, lay)]
  )
end

# Ten thousand parts, of which the client is sent about thirty.
#
# A plain `list` would put all ten thousand in the tree — six nodes a row,
# sixty thousand ops — and a single batch may carry 65 535 (02 §6). So this
# is a *windowed* list (04 §7.1): the server says how many rows there are and
# how tall each is, sends the ones in view, and is told — as `inv_window` —
# when the view moves. Scrolling the ledger costs a window of rows however
# long the catalogue is, which is the whole argument for the kind.
ERP_STOCK_COUNT = 10000

ERP_ROW_PX = 22

# One height per row, built once: the list needs every row's height to know
# how far it scrolls, and ten thousand identical numbers is still ten
# thousand numbers to rebuild on each event.
ERP_STOCK_HEIGHTS = {}

def erp_stock_heights
  cached = ERP_STOCK_HEIGHTS["all"]
  return cached unless cached.nil?

  heights = range(0, ERP_STOCK_COUNT).map(fn(i) { ERP_ROW_PX })
  ERP_STOCK_HEIGHTS["all"] = heights
  heights
end

def erp_stock_card(state, lay)
  widths = lay["wide"] ? [110, 170, 110, 90, 90] : [110, 90]
  labels = lay["wide"] ? ["SKU", "Part", "Warehouse", "On hand", "Price"] : ["SKU", "On hand"]
  window = state["inv_window"] ?? [0, 24]
  first = window[0] ?? 0
  last = window[1] ?? 24
  last = ERP_STOCK_COUNT - 1 if last > ERP_STOCK_COUNT - 1
  rows = first > last ? [] : range(first, last + 1).map(fn(i) {
    part = erp_product(i)
    values = lay["wide"] ? [
      part["sku"],
      part["name"],
      part["warehouse"],
      str(part["on_hand"]),
      part["price"]
    ] : [part["sku"], str(part["on_hand"])]
    {
      "k": "box",
      "s": {"display": "column", "width": "100%"},
      "p": {"row": i},
      "c": [table_row(part["id"], values, widths)]
    }
  })
  erp_card(
    "Stock ledger",
    [badge("10 000 parts", "info")],
    [
      table_header(labels, widths),
      list_window(
        {"height": 400, "width": "100%"},
        ERP_ROW_PX,
        ERP_STOCK_COUNT,
        erp_stock_heights(),
        rows,
        "inv_window"
      )
    ]
  )
end

def erp_reorder_card(state, lay)
  fields = [
    text_field("SKU", state["inv_sku"], "inv_sku", {"hint": "AX-0001 … AX-9999", "width": 200}),
    number_field("Quantity", state["inv_qty"], "inv_qty", {
      "min": 0,
      "max": 999,
      "step": 25,
      "on_step": "inv_qty_step",
      "width": 220
    }),
    column(
      {"gap": 1},
      [
        muted("Warehouse"),
        select_sized(
          ERP_WAREHOUSES,
          state["inv_wh"],
          state["inv_wh_open"] == true,
          "inv_wh_toggle",
          "inv_wh_pick",
          160,
          false
        )
      ]
    ),
    column({"gap": 1, "width": 220}, [date_field({
      "label": "Wanted by",
      "value": state["inv_date"],
      "month": state["inv_date_month"],
      "open": state["inv_date_open"] == true,
      "on_toggle": "inv_date_toggle",
      "on_pick": "inv_date_pick",
      "on_nav": "inv_date_nav",
      "placeholder": "Next delivery"
    })])
  ]
  erp_card(
    "Raise a purchase order",
    [],
    [
      row({"gap": 4, "align": "start", "wrap": "wrap", "width": "100%"}, fields),
      row({"gap": 2, "justify": "end", "width": "100%"}, [button("Send to supplier", "inv_send")])
    ]
  )
end

# What is short, as a bar each: the figure that matters is how far under the
# line it is, and a number cannot show that at a glance.
def erp_shortages_card(state, lay)
  short = erp_products(60).filter(fn(part) { part["short"] })
  rows = range(0, short.length() > 6 ? 6 : short.length()).map(fn(i) {
    part = short[i]
    row(
      {"gap": 3, "align": "center", "width": "100%"},
      [
        column(
          {"gap": 0, "width": 170},
          [
            text(part["name"], {"size": 1, "weight": "semibold"}),
            row(
              {"gap": 2, "align": "center"},
              [text_link(part["sku"], "inv_pick", {"sku": part["sku"]}), muted(part["warehouse"])]
            )
          ]
        ),
        progress(part["on_hand"] * 1.0 / part["reorder"]),
        muted(str(part["on_hand"]) + " / " + str(part["reorder"])),
        badge(part["on_hand"] * 2 < part["reorder"] ? "critical" : "low", part["on_hand"] * 2 < part["reorder"] ? "danger" : "warning")
      ]
    )
  })
  erp_card(
    "Under the reorder point",
    [muted(str(rows.length()) + " of " + str(short.length()) + " · first sixty SKUs")],
    rows
  )
end

# ---- Reports ---------------------------------------------------------------

def erp_reports_section(state, lay)
  column(
    {"gap": lay["wide"] ? 5 : 3, "width": "100%"},
    [
      erp_month_end(state, lay),
      erp_schedule_card(state, lay),
      gallery_markdown(),
      erp_query_card(state)
    ]
  )
end

def erp_month_end(state, lay)
  erp_card(
    "Month end",
    [badge("4 of 9 done", "warning")],
    [
      muted("The checklist below is the same document the handbook holds; the handbook is six hundred lines and is not read until it is opened."),
      row(
        {"gap": 2, "wrap": "wrap"},
        [
          {
            "k": "box",
            "s": {
              "display": "row",
              "justify": "center",
              "align": "center",
              "pad": [2, 4, 2, 4],
              "bg": "surface.raised",
              "border": 1,
              "border_color": "border.default",
              "radius": 2,
              "cursor": "pointer"
            },
            "p": {"id": "doc"},
            "on": {"click": "lazy_toggle"},
            "c": [text("Open the handbook", {"weight": "semibold", "size": 1})]
          },
          secondary_button("Email the pack", "ask_alert"),
          danger_button("Reopen the period…", "ask_confirm")
        ]
      ),
      (state["dialog_said"] ?? "") == "" ? muted("No answer yet.") : muted("You " + state["dialog_said"] + " it.")
    ]
  )
end

# The inline pickers. The same three the fields on the other sections open in
# a panel — same state, same handlers, drawn in the page instead of over it,
# which is what a scheduling screen wants and a form does not.
def erp_schedule_card(state, lay)
  cols = 1
  cols = 2 if lay["roomy"]
  cols = 3 if lay["wide"]
  pickers = [
    column(
      {"gap": 2, "width": "100%", "pad": [0, 4, 0, 4]},
      [
        text("Close date", {"weight": "bold"}),
        date_picker(state["cal_month"], state["cal_date"], "cal_pick", "cal_nav")
      ]
    ),
    column(
      {"gap": 2, "width": "100%", "pad": [0, 4, 0, 4]},
      [
        text("Stock count", {"weight": "bold"}),
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
      {"gap": 2, "width": "100%", "pad": [0, 4, 0, 4]},
      [
        text("Reporting period", {"weight": "bold"}),
        date_range_picker(state["range_month"], state["range_start"], state["range_end"], "range_pick", "range_nav")
      ]
    )
  ]
  erp_card("Schedule", [], [{
    "k": "box",
    "s": {"display": "grid", "gap": 5, "width": "100%"},
    "p": {"columns": cols},
    "c": pickers
  }])
end

def erp_query_card(state)
  erp_card(
    "How the revenue figure is got",
    [muted("app/models/revenue.sl")],
    [
      code_block("soli jobs run close --period 2026-09    # what the nightly close calls"),
      gallery_code_viewer()
    ]
  )
end

# ---- Settings --------------------------------------------------------------

def erp_settings_section(state, lay)
  column(
    {"gap": lay["wide"] ? 5 : 3, "width": "100%"},
    [
      erp_company_card(state, lay),
      erp_preferences_card(state, lay),
      gallery_editor(state),
      erp_diagnostics_card(state, lay)
    ]
  )
end

# Every one-line field the catalogue has, in the one place an application
# would put them: the form nobody fills in twice.
def erp_company_card(state, lay)
  left = [
    text_field("Legal name", state["co_name"], "co_name", {"required": true}),
    email_field("Billing address", state["co_email"], "co_email", {
      "required": true,
      "hint": "Where invoices are sent"
    }),
    number_field("VAT rate", state["co_vat"], "co_vat", {
      "min": 0,
      "max": 30,
      "step": 1,
      "on_step": "co_vat_step",
      "hint": "Per cent, 0 to 30"
    })
  ]
  right = [
    date_field({
      "label": "Fiscal year starts",
      "value": state["co_start"],
      "month": state["co_start_month"],
      "open": state["co_start_open"] == true,
      "on_toggle": "co_start_toggle",
      "on_pick": "co_start_pick",
      "on_nav": "co_start_nav",
      "placeholder": "Pick a day"
    }),
    textarea_field("Invoice footer", state["co_footer"], "co_footer", {
      "rows": 4,
      "hint": "Printed under every total"
    })
  ]
  body = lay["wide"] ? row(
    {"gap": 5, "align": "start", "width": "100%"},
    [column({"gap": 4, "grow": 1, "basis": 280}, left), column({"gap": 4, "grow": 1, "basis": 280}, right)]
  ) : column({"gap": 4, "width": "100%"}, left.concat(right))
  erp_card(
    "Company",
    [muted("Meridian Industrial SAS")],
    [body, row({"gap": 2, "justify": "end", "width": "100%"}, [button("Save", "save")])]
  )
end

def erp_preferences_card(state, lay)
  left = column(
    {"gap": 3, "grow": 1, "basis": 260},
    [
      column(
        {"gap": 1},
        [
          muted("Currency"),
          radio_group(
            ["Euro", "Pound", "Dollar"],
            state["currency"],
            "currency_pick",
            {"name": "currency", "label": "Reporting currency"}
          )
        ]
      ),
      checkbox("Weekly digest", state["digest"] == true, "digest", {"id": "digest"}),
      switch("Email me on low stock", state["notify"] == true, "notify", {"id": "notify"})
    ]
  )
  # The slider's key is the one the client looks for: it updates the caption
  # locally, without a round trip, and the atom it looks up is this name.
  right = column(
    {"gap": 2, "grow": 1, "basis": 260},
    [
      muted("Low stock threshold"),
      keyed("gallery_slider", slider(state["slider"], 0, 100, "slider")),
      keyed("gallery_slider_value", muted("Value " + str(state["slider"])))
    ]
  )
  erp_card(
    "Preferences",
    [],
    [lay["wide"] ? row({"gap": 5, "align": "start", "width": "100%"}, [left, right]) : column({"gap": 4, "width": "100%"}, [left, right])]
  )
end

def erp_diagnostics_card(state, lay)
  erp_card(
    "Attachment formats",
    [muted("what the client decodes")],
    [
      muted("The same photograph, encoded three ways. The client tells them apart by their first bytes; the server only ever sends a hash."),
      gallery_pictures(lay["wide"])
    ]
  )
end

# ---- The week, in four charts ----------------------------------------------
#
# A canvas is drawn at the size the server picked, so the width here has to
# be the one the layout will hand it: the content width (the window less the
# rail), less the page's padding, less the card's, less the gaps between the
# columns. The four series live in `erp_data.sl`.
def erp_charts(state, lay)
  cols = 1
  cols = 2 if lay["roomy"]
  cols = 3 if lay["wide"]
  cols = 4 if lay["rail"]
  gap = space_px(4, lay["density"])
  inner = erp_content_px(lay) - 2 * space_px(lay["wide"] ? 6 : 4, lay["density"]) - 2 * space_px(5, lay["density"])
  cw = int((inner - (cols - 1) * gap) / cols)
  cw = 160 if cw < 160
  ch = 140
  series = erp_revenue()
  plots = [
    column({"gap": 2}, [text("Revenue", {"weight": "bold"}), chart_line("line", series["line"], cw, ch)]),
    column({"gap": 2}, [text("Cash", {"weight": "bold"}), chart_area("area", series["area"], cw, ch)]),
    column({"gap": 2}, [text("Orders", {"weight": "bold"}), chart_bar("bars", series["bars"], cw, ch)]),
    column(
      {"gap": 2},
      [text("By channel", {"weight": "bold"}), chart_donut("mix", series["mix"], erp_mix_labels(), ch, ch)]
    )
  ]
  erp_card("This week", [muted("hover a chart")], [{
    "k": "box",
    "s": {"display": "grid", "gap": 4, "width": "100%"},
    "p": {"columns": cols},
    "c": plots
  }])
end

# ---- What the handler answers ----------------------------------------------

# The date fields all answer the same three events, so one function answers
# them for every field: a new picker costs a line in the view and nothing
# here. `name` is the state key holding the value; `name + "_open"` and
# `name + "_month"` are the two beside it.
ERP_PICKERS = ["nf_date", "nf_slot", "inv_date", "co_start"]

def erp_picker_name(event)
  found = ""
  i = 0
  while i < ERP_PICKERS.length()
    name = ERP_PICKERS[i]
    found = name if event == name + "_toggle" || event == name + "_pick" || event == name + "_nav"
    i = i + 1
  end
  found
end

def erp_picker(state, name, event, props)
  return set_key(state, name + "_open", !(state[name + "_open"] ?? false)) if event == name + "_toggle"
  return set_key(state, name + "_month", month_shift(state[name + "_month"], props["delta"])) if event == name + "_nav"

  # A picked day closes the panel. A range does not — it has a second end to
  # collect — which is why `range_pick` is not one of these.
  set_key(set_key(state, name, props["date"]), name + "_open", false)
end

# A field's `change` carries the whole value, once, when the field is left.
def erp_field(state, key, params)
  set_key(state, key, params["payload"])
end

def erp_step(state, key, props, o)
  set_key(state, key, number_stepped(state[key], props["delta"], o))
end

def erp_say(state, message)
  set_key(set_key(state, "toast", message), "toast_tone", "success")
end

# The clock of a datetime field: two selects, so the value cannot be
# anything but HH:MM, and picking either closes both.
def erp_set_time(state, name, which, value)
  bits = (state[name + "_time"] ?? "00:00").split(":")
  h = bits[0]
  m = "00"
  m = bits[1] if bits.length() > 1
  h = value if which == "hour"
  m = value if which == "min"
  state[name + "_time"] = h + ":" + m
  state[name + "_hour_open"] = false
  state[name + "_min_open"] = false
  state
end

# Dropping one chip clears one filter, and the chip carries which.
def erp_drop_filter(state, key)
  return set_key(set_key(state, "range_start", ""), "range_end", "") if key == "range"
  return set_key(state, key, false) if key == "unpaid" || key == "group"
  return set_key(state, "select_value", "Any status") if key == "select_value"

  set_key(state, key, "")
end

def erp_clear_filters(state)
  state = set_key(state, "select_value", "Any status")
  state = set_key(state, "unpaid", false)
  state = set_key(state, "group", false)
  state = set_key(state, "search", "")
  state = set_key(state, "range_start", "")
  state = set_key(state, "range_end", "")
  set_key(state, "page", 1)
end

# ---- The state -------------------------------------------------------------
#
# Composed per section rather than as one long literal, because a key that is
# not declared here is dropped on the next round trip — including, for an
# editor, on every keystroke — and a list of eighty in one place is a list
# nobody checks against the section that owns them.

def erp_chrome_defaults
  {
    "section": "Dashboard",
    "nav_open": false,
    "acct_open": false,
    "search": "",
    "seg": "Week",
    "sheet": false,
    "toast": "",
    "toast_tone": "success",
    "dialog": "",
    "dialog_said": "",
    "shown": [],
    "doc_window": [0, 24],
    "devbar": true,
    "sound": false,
    "video": false,
    "video_at": 0,
    "video_seek": 0,
    "viewport": {
      "width": 1280,
      "height": 800,
      "scale": 1.0,
      "mode": "light",
      "density": "cozy",
      "font_scale": 1.0
    }
  }
end

def erp_orders_defaults
  {
    "page": 1,
    "select_open": false,
    "select_value": "Any status",
    "sort_by": "Due date",
    "unpaid": false,
    "group": false,
    "order_sel": "",
    "order_menu": false,
    "range_month": "2026-09",
    "range_start": "",
    "range_end": "",
    "range_open": false,
    "nf_open": false,
    "nf_tried": false,
    "nf_ref": "",
    "nf_customer": "",
    "nf_email": "",
    "nf_qty": "20",
    "nf_notes": "",
    "nf_date": "",
    "nf_date_open": false,
    "nf_date_month": "2026-09",
    "nf_slot": "2026-09-18",
    "nf_slot_time": "09:30",
    "nf_slot_open": false,
    "nf_slot_month": "2026-09",
    "nf_slot_hour_open": false,
    "nf_slot_min_open": false,
    "grid_rows": gallery_invoices(),
    "grid_row": "",
    "grid_col": "",
    "grid_edit": false,
    "grid_sort": "",
    "grid_dir": "asc",
    "grid_menu": false
  }
end

def erp_customers_defaults
  {
    "cust_sel": "C-101",
    "cust_tab": "Profile",
    "cust_notes": "",
    "open": "a",
    "tree_open": ["C-101"],
    "split_x": 380,
    "split_x_drag": false,
    "split_y": 500,
    "split_y_drag": false
  }
end

def erp_inventory_defaults
  {
    "inv_sku": "",
    "inv_qty": "100",
    "inv_wh": "Lyon",
    "inv_wh_open": false,
    "inv_date": "",
    "inv_date_open": false,
    "inv_date_month": "2026-09",
    "inv_window": [0, 24]
  }
end

def erp_reports_defaults
  {
    "cal_month": "2026-09",
    "cal_date": "",
    "dt_month": "2026-09",
    "dt_date": "2026-09-06",
    "dt_time": "09:30",
    "dt_hour_open": false,
    "dt_min_open": false
  }
end

def erp_settings_defaults
  {
    "co_name": "Meridian Industrial SAS",
    "co_email": "billing@meridian.test",
    "co_vat": "20",
    "co_footer": "Meridian Industrial SAS · 14 rue des Docks, Lyon · VAT FR-88-402-113",
    "co_start": "2026-01-01",
    "co_start_open": false,
    "co_start_month": "2026-01",
    "currency": "Euro",
    "digest": true,
    "notify": false,
    "slider": 40,
    "slider_drag": false,
    # The editor's buffer. Seeded on the first look rather than here, so the
    # sample is split once a session and not once an event.
    "ed": {}
  }
end

def gallery_defaults(state)
  base = erp_chrome_defaults()
    .merge(erp_orders_defaults())
    .merge(erp_customers_defaults())
    .merge(erp_inventory_defaults())
    .merge(erp_reports_defaults())
    .merge(erp_settings_defaults())
  for key in base.keys()
    base[key] = state[key] unless state[key].nil?
  end
  base
end

def gallery(event_data)
  event = event_data["event"]
  params = event_data["params"]
  props = params["props"] ?? {}
  state = gallery_defaults(event_data["state"] ?? {})
  picker = erp_picker_name(event)
  return erp_picker(state, picker, event, props) unless picker == ""

  match event {
    # The shell.
    "nav" => set_key(set_key(state, "section", props["path"]), "nav_open", false),
    "nav_toggle" => set_key(state, "nav_open", !(state["nav_open"] ?? false)),
    "acct_toggle" => set_key(state, "acct_open", !(state["acct_open"] ?? false)),
    "acct_pick" => erp_say(set_key(state, "acct_open", false), props["item"] + " — not in a demonstration"),
    "sign_out" => erp_say(set_key(state, "acct_open", false), "Signed out. Not really: there is nothing to sign out of."),
    "search" => set_key(erp_field(state, "search", params), "page", 1),
    "seg" => set_key(state, "seg", props["option"]),
    "sheet" => set_key(state, "sheet", !state["sheet"]),
    "toast_done" => set_key(state, "toast", ""),
    # The dashboard.
    "forecast_toggle" => set_key(state, "shown", toggle_id(state["shown"] ?? [], "forecast")),
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
    # Orders: the filter bar owns one key each, so the chips can be built
    # from the state alone and dropping one is a single assignment.
    "select_toggle" => set_key(state, "select_open", !state["select_open"]),
    "select_pick" => set_key(set_key(set_key(state, "select_value", props["value"]), "select_open", false), "page", 1),
    "sort_pick" => set_key(state, "sort_by", props["value"]),
    "unpaid" => set_key(set_key(state, "unpaid", !(state["unpaid"] ?? false)), "page", 1),
    "group" => set_key(state, "group", !(state["group"] ?? false)),
    "chip_drop" => set_key(erp_drop_filter(state, props["key"]), "page", 1),
    "clear_filters" => erp_clear_filters(state),
    "page" => set_key(state, "page", props["page"]),
    "order_pick" => set_key(set_key(state, "order_sel", props["ref"]), "order_menu", false),
    "row_menu" => set_key(state, "order_menu", !(state["order_menu"] ?? false)),
    "row_action" => erp_say(set_key(state, "order_menu", false), props["item"] + " " + (state["order_sel"] ?? "")),
    "export" => erp_say(state, "Sixty-three orders exported"),
    "range_toggle" => set_key(state, "range_open", !(state["range_open"] ?? false)),
    "range_nav" => set_key(state, "range_month", month_shift(state["range_month"], props["delta"])),
    "range_pick" => set_key(pick_range(state, props["date"]), "page", 1),
    # The new order.
    "order_new" => set_key(set_key(state, "nf_open", true), "nf_tried", false),
    "order_close" => set_key(state, "nf_open", false),
    "order_create" => erp_create_order(state),
    "nf_ref" => erp_field(state, "nf_ref", params),
    "nf_customer" => erp_field(state, "nf_customer", params),
    "nf_email" => erp_field(state, "nf_email", params),
    "nf_qty" => erp_field(state, "nf_qty", params),
    "nf_qty_step" => erp_step(state, "nf_qty", props, {"min": 1, "max": 999, "step": 5}),
    "nf_notes" => erp_field(state, "nf_notes", params),
    "nf_slot_hour_toggle" => set_key(
      set_key(state, "nf_slot_hour_open", !(state["nf_slot_hour_open"] ?? false)),
      "nf_slot_min_open",
      false
    ),
    "nf_slot_min_toggle" => set_key(
      set_key(state, "nf_slot_min_open", !(state["nf_slot_min_open"] ?? false)),
      "nf_slot_hour_open",
      false
    ),
    "nf_slot_hour" => erp_set_time(state, "nf_slot", "hour", props["value"]),
    "nf_slot_min" => erp_set_time(state, "nf_slot", "min", props["value"]),
    # Customers.
    "cust_pick" => set_key(state, "cust_sel", props["id"]),
    "cust_tab" => set_key(state, "cust_tab", props["tab"]),
    "cust_notes" => erp_field(state, "cust_notes", params),
    "toggle" => set_key(state, "open", props["id"] == state["open"] ? "" : props["id"]),
    "tree" => set_key(state, "tree_open", toggle_id(state["tree_open"], props["id"])),
    "split_x" => split_event(state, params, "split_x", "row", gallery_split_extent(state), 220, 260, 6),
    "split_y" => split_event(state, params, "split_y", "column", 260, 70, 70, 6),
    # Inventory.
    "inv_sku" => erp_field(state, "inv_sku", params),
    "inv_qty" => erp_field(state, "inv_qty", params),
    "inv_qty_step" => erp_step(state, "inv_qty", props, {"min": 0, "max": 999, "step": 25}),
    "inv_wh_toggle" => set_key(state, "inv_wh_open", !(state["inv_wh_open"] ?? false)),
    "inv_wh_pick" => set_key(set_key(state, "inv_wh", props["value"]), "inv_wh_open", false),
    "inv_send" => erp_say(state, "Purchase order sent to " + (state["inv_wh"] ?? "")),
    "inv_window" => set_key(state, "inv_window", params["payload"]),
    "inv_pick" => set_key(state, "inv_sku", props["sku"]),
    # Reports: the inline pickers, and the handbook.
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
    "dt_hour" => erp_set_time(state, "dt", "hour", props["value"]),
    "dt_min" => erp_set_time(state, "dt", "min", props["value"]),
    "lazy_toggle" => set_key(state, "shown", toggle_id(state["shown"] ?? [], props["id"])),
    "lazy_close" => set_key(state, "shown", (state["shown"] ?? []).filter(fn(x) { x != props["id"] })),
    "doc_window" => set_key(state, "doc_window", params["payload"]),
    # Settings.
    "co_name" => erp_field(state, "co_name", params),
    "co_email" => erp_field(state, "co_email", params),
    "co_vat" => erp_field(state, "co_vat", params),
    "co_vat_step" => erp_step(state, "co_vat", props, {"min": 0, "max": 30, "step": 1}),
    "co_footer" => erp_field(state, "co_footer", params),
    "currency_pick" => set_key(state, "currency", props["value"]),
    "digest" => set_key(state, "digest", !(state["digest"] ?? false)),
    "notify" => set_key(state, "notify", !(state["notify"] ?? false)),
    "slider" => set_slider(state, params),
    "save" => erp_say(state, "Company details saved"),
    # The grid, the editor, the dialogs and the dev bar.
    "grid_select" => gallery_grid_select(state, params),
    "grid_change" => gallery_grid_change(state, params),
    "grid_sort" => gallery_grid_sort(state, props["col"]),
    "grid_key" => gallery_grid_key(state, params),
    "ed_key" => set_key(state, "ed", ed_key(gallery_editor_state(state), params["payload"][0], params["payload"][1])),
    "ed_click" => set_key(state, "ed", ed_click(gallery_editor_state(state), params)),
    "ed_reload" => set_key(state, "ed", gallery_editor_reset(state)),
    "ask_alert" => set_key(state, "dialog", "alert"),
    "ask_confirm" => set_key(state, "dialog", "confirm"),
    "dialog_ok" => set_key(set_key(state, "dialog", ""), "dialog_said", "confirmed"),
    "dialog_cancel" => set_key(set_key(state, "dialog", ""), "dialog_said", "cancelled"),
    "dialog_close" => set_key(set_key(state, "dialog", ""), "dialog_said", "acknowledged"),
    "dev_bar_toggle" => set_key(state, "devbar", false),
    "connect" => set_key(state, "viewport", params["viewport"] ?? state["viewport"]),
    "viewport" => set_key(state, "viewport", params["viewport"] ?? state["viewport"]),
    _ => state,
  }
end

# ---- The page --------------------------------------------------------------
#
# One scroller holds the section that is up; everything above it — the rail,
# the top bar — is outside, so the page moves under a header that stays. The
# layers over it are in the order they have to be looked at: the drawer and
# the sheet, then the new order, then a question, then the handbook, then
# what the last click did, and last the dev bar.
def gallery_view(raw_state)
  state = gallery_defaults(raw_state ?? {})
  lay = erp_layout(state)
  section = state["section"] ?? "Dashboard"
  body = erp_dashboard(state, lay)
  body = erp_orders_section(state, lay) if section == "Orders"
  body = erp_customers_section(state, lay) if section == "Customers"
  body = erp_inventory_section(state, lay) if section == "Inventory"
  body = erp_reports_section(state, lay) if section == "Reports"
  body = erp_settings_section(state, lay) if section == "Settings"
  page = scroll(
    {"grow": 1},
    [column({"gap": lay["wide"] ? 5 : 3, "pad": lay["wide"] ? 6 : 4, "width": "100%"}, [body])]
  )
  layers = [erp_shell(state, lay, page)]
  layers = layers.concat([drawer(
    [
      h2("Meridian"),
      restyle(sidebar(ERP_SECTIONS, section, "nav"), {"width": "100%", "bg": "none", "border": 0, "pad": 0}),
      spacer(),
      erp_profile(state),
      button("Close", "nav_toggle")
    ],
    {"label": "Sections", "on_close": "nav_toggle"}
  )]) if state["nav_open"] == true && !lay["rail"]
  layers = layers.concat([erp_queue_sheet(state)]) if state["sheet"] == true
  layers = layers.concat([erp_new_order(state)]) if state["nf_open"] == true
  asking = state["dialog"] ?? ""
  layers = layers.concat([confirm(
    "Reopen September?",
    "The period is closed and its invoices are numbered. Nothing here is real, so nothing is lost.",
    "dialog_ok",
    "dialog_cancel",
    {"ok": "Reopen", "danger": true}
  )]) if asking == "confirm"
  # The handbook is neither read nor parsed while it is closed: `lazy` does
  # not call the thunk, so the page behind pays nothing for it.
  layers = layers.concat([lazy(
    "doc",
    state["shown"],
    {"k": "box", "s": {"display": "none"}},
    fn() { gallery_doc(state) }
  )])
  layers = layers.concat([alert(
    "Sent",
    "The pack is on its way to the four people who ask for it every month.",
    "dialog_close",
    {"ok": "Good"}
  )]) if asking == "alert"
  layers = layers.concat([erp_toast(state)]) unless (state["toast"] ?? "") == ""
  layers = layers.concat([dev_bar(eui_stats(), state["devbar"] ?? true)])
  stack({"gap": 0}, layers)
end

# What the last action did, in the top corner, until it is clicked away. A
# toast that dismissed itself would need a clock the server does not have.
#
# The overlay is as tall as the toast and no taller. An overlay given the
# window's height is a sheet of glass over the whole page: it is in the top
# layer, so every click lands on *it* and the application underneath stops
# answering. The dev bar has always been placed this way; this follows it.
# Creating asks the fields the same questions they ask themselves. Nothing
# stops a client sending `order_create` with an empty form — the sheet's
# buttons are a courtesy, not a gate — so the handler decides, and a form
# that does not pass comes back with `nf_tried` set and says why.
def erp_create_order(state)
  ok = (state["nf_ref"] ?? "").strip() != ""
  ok = false if (state["nf_customer"] ?? "").strip() == ""
  ok = false unless email_valid?(state["nf_email"])
  return set_key(state, "nf_tried", true) unless ok

  erp_say(
    set_key(set_key(state, "nf_open", false), "nf_tried", false),
    "Order raised for " + (state["nf_customer"] ?? "")
  )
end

def erp_toast(state)
  note = toast(state["toast"], state["toast_tone"] ?? "success")
  # It goes on its own. `wake` is 06 §1.1: the node asks to be woken in so
  # many milliseconds and the client obliges, which is the only clock in
  # this protocol — the server has none, and a toast that waited for a
  # click would still be sitting there tomorrow. Clicking it is the
  # shortcut, not the mechanism.
  note["p"] = (note["p"] ?? {}).merge({"wake": 3200})
  note["on"] = {"click": "toast_done", "wake": "toast_done"}
  note["s"]["cursor"] = "pointer"
  {
    "k": "overlay",
    "s": {
      "position": "absolute",
      "align": "start",
      "justify": "end",
      "pad": 6,
      "width": "100%"
    },
    "c": [note]
  }
end

# The queue the banner opens: what is late, and nothing else.
def erp_queue_sheet(state)
  widths = [110, 150, 110]
  late = erp_orders(63).filter(fn(order) { order["status"] == "Late" })
  rows = range(0, late.length() > 6 ? 6 : late.length()).map(fn(i) {
    table_row("late-" + late[i]["ref"], [late[i]["ref"], late[i]["customer"], late[i]["amount"]], widths)
  })
  sheet(
    "right",
    [
      h2("Past due"),
      text("Nine orders are past their promised date.", {"fg": "text.muted"}),
      table_header(["Order", "Customer", "Amount"], widths)
    ].concat(rows).concat([button("Close", "sheet")]),
    {"label": "Past due", "on_close": "sheet"}
  )
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
  # The client asks for the rows in view plus two viewports either side, and
  # only once the scroll has been still (04 §7.1). Answering with more than
  # it asked for is runway that costs nothing to hold: a fling that outruns
  # its own request lands on cards instead of the sunken placeholder. A few
  # dozen extra cards is the order §7.1 already budgets for.
  runway = 30
  first = window[0] - runway
  first = 0 if first < 0
  last = window[1] + runway
  last = count - 1 if last > count - 1
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
