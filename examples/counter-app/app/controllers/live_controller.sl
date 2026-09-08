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
  column(
    {"gap": 2},
    [
      audio("public/sounds/chime.wav", {"playing": playing, "volume": 80}, {"ended": "sound_ended"}),
      row(
        {"gap": 3, "align": "center"},
        [
          button(playing ? "Stop" : "Play a chime", "sound"),
          muted(playing ? "playing…" : "1.6 s, decoded and mixed by the client")
        ]
      )
    ]
  )
end

# The gallery's moving picture: a loop the client decodes and plays. The
# node says what it should be doing; the client owns the clock.
def gallery_video(state)
  playing = state["video"] ?? false
  column(
    {"gap": 2},
    [
      video(
        "public/video/pulse.gif",
        {"playing": playing, "loop": false, "position": state["video_seek"] ?? 0},
        {"width": 320, "height": 180, "radius": 3},
        {"time_update": "video_time", "ended": "video_done"}
      ),
      row(
        {"gap": 3, "align": "center"},
        [
          media_button(playing, "video", {}),
          media_scrubber(200, state["video_at"] ?? 0, 1440, "video_scrub", {"w": 200}),
          muted(media_clock(state["video_at"] ?? 0) + " / 0:01")
        ]
      )
    ]
  )
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
    "cal_month": "2026-09",
    "cal_date": "",
    "dt_month": "2026-09",
    "dt_date": "2026-09-06",
    "dt_time": "09:30",
    "range_month": "2026-09",
    "range_start": "",
    "range_end": "",
    "sound": false,
    "video": false,
    "video_at": 0,
    "video_seek": 0
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
    "sound" => set_key(state, "sound", !(state["sound"] ?? false)),
    "video" => set_key(set_key(state, "video", !(state["video"] ?? false)), "video_at", state["video"] ?? false ? state["video_at"] : 0),
    "video_time" => set_key(state, "video_at", params["payload"][0]),
    "video_done" => set_key(set_key(state, "video", false), "video_at", 1440),
    "video_scrub" => set_key(set_key(state, "video_seek", int(params["payload"][0] * 1440 / 200)), "video_at", int(params["payload"][0] * 1440 / 200)),
    "sound_ended" => set_key(state, "sound", false),
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
      labelled("Sound", gallery_sound(state)),
      labelled("Video", gallery_video(state)),
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
  count = state["count"] ?? 10
  window = state["window"] ?? [0, 0]
  sound = state["sound"] ?? -1
  moving = state["moving"] ?? -1
  at = state["at"] ?? 0
  duration = state["duration"] ?? 0
  seek = state["seek"] ?? 0
  keep = {"liked": liked, "count": count, "window": window, "sound": sound, "moving": moving, "at": at, "duration": duration, "seek": seek}
  match event {
    "like" => keep.merge({"liked": toggle_id(liked, params["props"]["id"])}),
    "more" => keep.merge({"count": count + 5000}),
    "window" => keep.merge({"window": params["payload"]}),
    "play" => keep.merge({"sound": sound == params["props"]["id"] ? -1 : params["props"]["id"]}),
    "sound_ended" => keep.merge({"sound": -1}),
    # A picture plays only when asked, and only one at a time.
    "video_play" => keep.merge({"moving": moving == params["props"]["id"] ? -1 : params["props"]["id"], "at": 0, "seek": 0}),
    "video_time" => keep.merge({"at": params["payload"][0], "duration": params["payload"][1]}),
    "video_ended" => keep.merge({"moving": -1, "at": duration}),
    "video_seek" => video_seek(keep, params),
    _ => keep,
  }
end

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
    if i >= first - 100 && i <= last + 100
      keep[key] = FEED_CARDS[key]
    end
  end
  FEED_CARDS = keep
end

def feed_view(state)
  liked = state["liked"] ?? []
  count = state["count"] ?? 10
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
    feed_card(i, liked.includes?(i), {
      "sound": i == sound,
      "video": i == moving,
      "at": i == moving ? at : 0,
      "duration": i == moving ? duration : 0,
      "seek": i == moving ? seek : 0
    })
  })
  header = row(
    {
      "gap": 3,
      "align": "center",
      "pad": [3, 4, 3, 4],
      "bg": "surface.raised",
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    [h1("Feed"), badge(str(count) + " posts", "info"), spacer(), loading_button("Load 5 000 more", "more", "more")]
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
      [header, list_window({"grow": 1}, 128, count, feed_heights(count), cards, "window")]
    )]
  )
end
