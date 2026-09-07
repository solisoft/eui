# Soundly: a music player in the shape of the one everybody knows — a black
# shell, a rounded library panel, a home of covers, an album page with a
# tinted header and a track table, a now-playing bar. No network and no
# catalogue behind it: the library is generated and the covers are the
# feed's pictures. It exists to show the catalogue doing a real layout,
# every hover a local handler and every click a server round trip that
# patches a few nodes. The colours are literal on purpose: this app is
# always dark, like its model, and spec 05 §6 allows a brand its colours.

MUSIC_ARTISTS = ["Nova Reyes", "The Low Tide", "Ines Marlow", "Kappa Sun", "Orbital Fields", "June & the Motors", "Halden", "Coral Static", "Mira Voss", "Pale Harbour", "Dune Cartel", "Silent Meridian"]
MUSIC_TITLES = ["Night Drive", "Salt", "Everything After", "Slow Light", "Copper Sky", "Undertow", "Northern Rooms", "Glass Hours", "Afterglow", "Small Hours", "Second Summer", "Static Bloom", "Harbour Lights", "Wide Awake", "Half Moon", "Paper Planes", "Golden Ratio", "Lighthouse", "Open Water", "Late Bloomer", "Quiet Engines", "Sun Chaser", "Blue Room", "Last Train"]
MUSIC_TRACK_WORDS = ["Intro", "Runaway", "Signal", "Marble", "Weightless", "Fever", "Skyline", "Tides", "Drift", "Fold", "Ember", "Echoes", "Velvet", "Meridian", "Circles", "Wander"]
MUSIC_PLAYLISTS = ["Liked Songs", "Morning Focus", "Late Night Drive", "Coffee & Rain", "Running 160", "Sunday Slow", "Deep Work", "Road Trip 2025"]
# A nested array literal at top level reads as a string in Soli 2.0.7, so
# the genre table is built from two flat lists.
MUSIC_GENRE_NAMES = ["Podcasts", "Live Events", "Made For You", "New Releases", "Pop", "Hip-Hop", "Rock", "Latin", "Dance/Electronic", "Indie", "Mood", "Workout", "Chill", "Sleep", "Jazz", "Classical"]
MUSIC_GENRE_COLOURS = ["#006450", "#8400e7", "#1e3264", "#e8115b", "#148a08", "#bc5900", "#e91429", "#e1118c", "#d84000", "#608108", "#e0c200", "#777777", "#d84000", "#1e3264", "#7d4b32", "#8d67ab"]

def music_genres
  range(0, MUSIC_GENRE_NAMES.length()).map(fn(i) { [MUSIC_GENRE_NAMES[i], MUSIC_GENRE_COLOURS[i]] })
end
# A dark tint per cover, for an album page's header: what the real client
# derives from the picture, chosen here by hand for the eight covers.
MUSIC_TINTS = ["#1f3a5f", "#1f4d34", "#6b3a12", "#4a2a63", "#2e3b4d", "#6b1f3f", "#1a4a4d", "#5a4a12"]

BLACK = "#000000"
PANEL = "#121212"
CARD = "#181818"
CARD_HOVER = "#282828"
PICK = "#2a2a2a"
PICK_HOVER = "#3a3a3a"
WHITE = "#ffffff"
GREY = "#b3b3b3"
DIM = "#4d4d4d"
GREEN = "#1db954"
GREEN_HOVER = "#1ed760"

def music_album(a)
  tracks = range(0, 8).map(fn(t) {
    {
      "n": t + 1,
      "title": MUSIC_TRACK_WORDS[(a * 5 + t * 3) % MUSIC_TRACK_WORDS.length()] + (t % 4 == 3 ? " (Reprise)" : ""),
      "seconds": 150 + ((a * 37 + t * 53) % 190)
    }
  })
  {
    "id": a,
    "title": MUSIC_TITLES[a % MUSIC_TITLES.length()],
    "artist": MUSIC_ARTISTS[(a * 5) % MUSIC_ARTISTS.length()],
    "year": 2008 + ((a * 7) % 17),
    "cover": "public/images/feed/" + str(a % 8) + ".png",
    "tint": MUSIC_TINTS[a % 8],
    "tracks": tracks
  }
end

MUSIC_ALBUMS = range(0, 24).map(fn(a) { music_album(a) })

def music_initial(name)
  name.substring(0, 1)
end

def music_clock(seconds)
  m = seconds / 60
  s = seconds % 60
  str(m) + ":" + (s < 10 ? "0" + str(s) : str(s))
end

def music_long(seconds)
  str(seconds / 60) + " min " + str(seconds % 60) + " sec"
end

def music_defaults(state)
  state["view"] = state["view"] ?? "home"
  state["album"] = state["album"] ?? 0
  state["track"] = state["track"] ?? 0
  state["playing"] = state["playing"] ?? false
  state["position"] = state["position"] ?? 42
  state["volume"] = state["volume"] ?? 70
  state["query"] = state["query"] ?? ""
  state["liked"] = state["liked"] ?? []
  state["open"] = state["open"] ?? 0
  state["shuffle"] = state["shuffle"] ?? false
  state["repeat"] = state["repeat"] ?? false
  state["history"] = state["history"] ?? []
  state["artist"] = state["artist"] ?? MUSIC_ARTISTS[0]
  state["genre"] = state["genre"] ?? "Pop"
  state["section"] = state["section"] ?? "Made For You"
  state["following"] = state["following"] ?? []
  state["autoplay"] = state["autoplay"] ?? true
  state["normalize"] = state["normalize"] ?? true
  state["quality"] = state["quality"] ?? "Very high"
  state["viewport"] = state["viewport"] ?? {"width": 1200, "height": 800}
  state
end

# Three layouts by width: full (sidebar with names), compact under 900 px
# (a rail of icons and covers), narrow under 640 px (no sidebar, a short
# now-playing bar). The client reports its viewport at connect and on
# every resize; the handler keeps it.
def music_layout(state)
  w = state["viewport"]["width"] ?? 1200
  {"compact": w < 900, "narrow": w < 640, "width": w}
end

# Every navigation remembers where it came from, so the top bar's chevron
# goes back through it like a browser's.
def music_nav(state, view)
  return state if state["view"] == view

  state["history"] = state["history"].concat([state["view"]])
  state["view"] = view
  state
end

def music_back(state)
  h = state["history"]
  return state if h.length() == 0

  state["view"] = h[h.length() - 1]
  state["history"] = h.slice(0, h.length() - 1)
  state
end

def music_toggle_follow(state, name)
  f = state["following"]
  state["following"] = f.includes?(name) ? f.filter(fn(x) { x != name }) : f.concat([name])
  state
end

def music_artist_albums(name)
  MUSIC_ALBUMS.filter(fn(album) { album["artist"] == name })
end

def music_liked_tracks(state)
  state["liked"].map(fn(id) {
    album = MUSIC_ALBUMS[(id / 100) % MUSIC_ALBUMS.length()]
    [album, album["tracks"][(id % 100) - 1]]
  })
end

def music_set(state, key, value)
  state[key] = value
  state
end

def music_current(state)
  MUSIC_ALBUMS[state["album"] % MUSIC_ALBUMS.length()]
end

def music_current_track(state)
  music_current(state)["tracks"][state["track"] % 8]
end

def music_step(state, delta)
  t = state["track"] + delta
  if t < 0
    state["album"] = (state["album"] + MUSIC_ALBUMS.length() - 1) % MUSIC_ALBUMS.length()
    state["track"] = 7
  elsif t > 7
    state["album"] = (state["album"] + 1) % MUSIC_ALBUMS.length()
    state["track"] = 0
  else
    state["track"] = t
  end
  state["position"] = 0
  state["playing"] = true
  state
end

def music_seek(state, params)
  payload = params["payload"]
  if params["kind"] == "click"
    duration = music_current_track(state)["seconds"]
    state["position"] = int(payload[0] * duration / 480)
  end
  state
end

def music_volume(state, params)
  payload = params["payload"]
  if params["kind"] == "click"
    state["volume"] = int(payload[0] * 100 / 93)
  end
  state
end

def music_toggle_like(state, id)
  liked = state["liked"]
  state["liked"] = liked.includes?(id) ? liked.filter(fn(x) { x != id }) : liked.concat([id])
  state
end

def music(event_data)
  event = event_data["event"]
  params = event_data["params"]
  props = params["props"] ?? {}
  state = music_defaults(event_data["state"] ?? {})
  match event {
    "connect" => music_set(state, "viewport", params["viewport"] ?? state["viewport"]),
    "viewport" => music_set(state, "viewport", params["viewport"]),
    "go" => music_nav(state, props["view"]),
    "back" => music_back(state),
    "open" => music_nav(music_set(state, "open", props["album"]), "album"),
    "artist" => music_nav(music_set(state, "artist", props["artist"]), "artist"),
    "genre" => music_nav(music_set(state, "genre", props["genre"]), "genre"),
    "section" => music_nav(music_set(state, "section", props["section"]), "section"),
    "follow" => music_toggle_follow(state, props["artist"]),
    "autoplay" => music_set(state, "autoplay", !state["autoplay"]),
    "normalize" => music_set(state, "normalize", !state["normalize"]),
    "quality" => music_set(state, "quality", props["quality"]),
    "play" => music_set(music_set(music_set(music_set(state, "album", props["album"]), "track", props["track"] ?? 0), "playing", true), "position", 0),
    "toggle" => music_set(state, "playing", !state["playing"]),
    "next" => music_step(state, 1),
    "prev" => music_step(state, -1),
    "shuffle" => music_set(state, "shuffle", !state["shuffle"]),
    "repeat" => music_set(state, "repeat", !state["repeat"]),
    "seek" => music_seek(state, params),
    "volume" => music_volume(state, params),
    "search" => music_nav(music_set(state, "query", params["payload"]), "search"),
    "like" => music_toggle_like(state, props["id"]),
    _ => state,
  }
end

# ------------------------------------------------------------ primitives

# The player's icons, drawn: a stroke is a canvas polyline, a loop an arc,
# so they need no symbol font and take the colour they are told.
def micon_shuffle(c)
  canvas(18, 16, [[0, c, 1.6, 1, 3, 5, 3, 12, 13, 17, 13], [0, c, 1.6, 1, 13, 5, 13, 12, 3, 17, 3], [0, c, 1.6, 14, 1, 17, 3, 14, 5], [0, c, 1.6, 14, 11, 17, 13, 14, 15]])
end

def micon_repeat(c)
  canvas(18, 16, [[4, c, 1.6, 9, 8, 6, 0.9, 6.0], [0, c, 1.6, 12, 1, 15, 3.5, 12, 6]])
end

def micon_queue(c)
  canvas(16, 16, [[0, c, 1.6, 2, 4, 14, 4], [0, c, 1.6, 2, 8, 14, 8], [0, c, 1.6, 2, 12, 9, 12]])
end

def micon_device(c)
  canvas(18, 16, [[0, c, 1.6, 2, 2, 16, 2, 16, 11, 2, 11, 2, 2], [0, c, 1.6, 6, 14, 12, 14]])
end

def micon_volume(c)
  canvas(18, 16, [[0, c, 1.6, 2, 6, 5, 6, 9, 2, 9, 14, 5, 10, 2, 10, 2, 6], [4, c, 1.6, 9, 8, 4, 5.4, 7.2], [4, c, 1.6, 9, 8, 7, 5.5, 7.1]])
end

def micon_expand(c)
  canvas(16, 16, [[0, c, 1.6, 2, 6, 2, 2, 6, 2], [0, c, 1.6, 10, 14, 14, 14, 14, 10]])
end

def micon_clock(c)
  canvas(16, 16, [[4, c, 1.4, 8, 8, 6.3, 0, 6.28], [0, c, 1.4, 8, 4.5, 8, 8, 11, 9.5]])
end

def micon_lyrics(c)
  canvas(16, 16, [[0, c, 1.6, 2, 3, 14, 3, 14, 11, 8, 11, 5, 14, 5, 11, 2, 11, 2, 3]])
end

def mtext(content, size, weight, fg)
  {"k": "text", "t": content, "s": {"size": size, "weight": weight, "fg": fg}}
end

def micon(glyph, size, fg)
  {"k": "text", "t": glyph, "s": {"size": size, "fg": fg}}
end

# An artist's name, a link to the artist's page.
def martist(name, size, weight, fg)
  {"k": "box", "s": {"cursor": "pointer"}, "p": {"artist": name}, "on": {"click": "artist"}, "c": [mtext(name, size, weight, fg)]}
end

def mcover(album, edge, radius)
  {"k": "image", "p": {"src": album["cover"]}, "s": {"width": edge, "height": edge, "radius": radius, "shrink": 0}}
end

# A box that lightens under the pointer: the client swaps its style
# locally, no round trip. `key` names it for its own handlers.
def mhover(key, base, hover, children, on_click, props)
  hoverable = {
    "k": "box",
    "key": key,
    "s": base,
    "c": children,
    "on": {
      "pointer_enter": {"local": "self.style = @hover", "styles": {"hover": hover}},
      "pointer_leave": {"local": "self.style = @base", "styles": {"base": base}}
    }
  }
  hoverable["on"]["click"] = on_click unless on_click.nil?
  hoverable["p"] = props unless props.nil?
  hoverable
end

def mcircle(glyph, edge, bg, fg, on_click, props)
  base = {"display": "row", "align": "center", "justify": "center", "width": edge, "height": edge, "radius": 4, "bg": bg, "fg": fg, "cursor": "pointer", "shrink": 0}
  hover = base.merge({"bg": bg == GREEN ? GREEN_HOVER : bg})
  mhover("circle:" + on_click + ":" + glyph + str(edge), base, hover, [micon(glyph, edge > 40 ? 3 : 2, fg)], on_click, props)
end

# ------------------------------------------------------------- sidebar

def music_nav_item(label, glyph, view, active)
  fg = active ? WHITE : GREY
  base = {"display": "row", "gap": 4, "align": "center", "pad": [2, 3, 2, 3], "cursor": "pointer", "fg": fg, "transition": "fast"}
  mhover("nav:" + view, base, base.merge({"fg": WHITE}), [micon(glyph, 4, fg), mtext(label, 3, "bold", fg)], "go", {"view": view})
end

def music_library_item(name, sub, album, square)
  cover = album.nil? ? {"k": "box", "s": {"width": 48, "height": 48, "radius": square ? 1 : 5, "bg": "#3a3a3a", "shrink": 0, "display": "row", "align": "center", "justify": "center"}, "c": [micon("♥", 3, WHITE)]} : mcover(album, 48, square ? 1 : 5)
  base = {"display": "row", "gap": 3, "align": "center", "pad": [2, 2, 2, 2], "radius": 2, "cursor": "pointer"}
  props = album.nil? ? {"view": "playlist:" + name} : {"album": album["id"]}
  on = album.nil? ? "go" : "open"
  mhover(
    "lib:" + name,
    base,
    base.merge({"bg": "#1f1f1f"}),
    [cover, column({"gap": 0}, [mtext(name, 2, "medium", WHITE), mtext(sub, 1, "regular", GREY)])],
    on,
    props
  )
end

def music_chip(label, active)
  {
    "k": "box",
    "s": {"pad": [1, 3, 1, 3], "radius": 4, "bg": active ? WHITE : "#2a2a2a", "fg": active ? BLACK : WHITE},
    "c": [mtext(label, 1, "medium", active ? BLACK : WHITE)]
  }
end

def music_rail(state)
  view = state["view"]
  item = fn(glyph, target, active) {
    {"k": "box", "s": {"display": "row", "align": "center", "justify": "center", "width": 48, "height": 48, "radius": 2, "cursor": "pointer", "bg": active ? "#2a2a2a" : "none"}, "p": {"view": target}, "on": {"click": "go"}, "c": [micon(glyph, 4, active ? WHITE : GREY)]}
  }
  covers = MUSIC_ALBUMS.slice(0, 8).map(fn(album) {
    {"k": "box", "s": {"cursor": "pointer", "pad": [1, 0, 1, 0]}, "p": {"album": album["id"]}, "on": {"click": "open"}, "c": [mcover(album, 48, 1)]}
  })
  column(
    {"width": 72, "shrink": 0, "gap": 2, "min_height": 0},
    [
      column({"gap": 1, "pad": [2, 2, 2, 2], "radius": 3, "bg": PANEL, "align": "center", "shrink": 0}, [item("⌂", "home", view == "home"), item("⌕", "search", view == "search")]),
      column({"gap": 1, "pad": [2, 2, 2, 2], "radius": 3, "bg": PANEL, "align": "center", "grow": 1, "min_height": 0}, [item("≡", "library", view == "library"), scroll({"grow": 1, "min_height": 0}, [column({"gap": 0, "align": "center"}, covers)])])
    ]
  )
end

def music_sidebar(state)
  view = state["view"]
  top = column(
    {"gap": 1, "pad": [3, 3, 3, 3], "radius": 3, "bg": PANEL, "shrink": 0},
    [music_nav_item("Home", "⌂", "home", view == "home"), music_nav_item("Search", "⌕", "search", view == "search")]
  )
  playlists = MUSIC_PLAYLISTS.map(fn(name) { music_library_item(name, "Playlist • Soundly", nil, true) })
  albums = MUSIC_ALBUMS.slice(0, 5).map(fn(album) { music_library_item(album["title"], "Album • " + album["artist"], album, true) })
  library = column(
    {"gap": 2, "pad": [3, 2, 3, 2], "radius": 3, "bg": PANEL, "grow": 1, "min_height": 0},
    [
      row(
        {"gap": 3, "align": "center", "pad": [1, 2, 1, 2]},
        [micon_queue(GREY), mtext("Your Library", 3, "bold", GREY), spacer(), micon("+", 4, GREY), micon("›", 4, GREY)]
      ),
      row({"gap": 2, "pad": [1, 2, 2, 2]}, [music_chip("Playlists", false), music_chip("Artists", false), music_chip("Albums", false)]),
      scroll({"grow": 1, "min_height": 0, "gap": 1}, [column({"gap": 1}, playlists.concat(albums))])
    ]
  )
  column({"width": 300, "shrink": 0, "gap": 2, "min_height": 0}, [top, library])
end

# ---------------------------------------------------------------- home

def music_card(album, edge)
  base = {"display": "column", "gap": 2, "pad": [3, 3, 3, 3], "radius": 2, "width": edge, "bg": CARD, "cursor": "pointer", "transition": "fast", "shrink": 0}
  hover = base.merge({"bg": CARD_HOVER})
  play_key = "play:" + str(album["id"])
  # The green play circle sits at the cover's bottom-right corner, in a
  # layer over it that only shows under the pointer.
  play_hidden = {"display": "none"}
  play_shown = {"display": "row", "align": "end", "justify": "end", "width": edge - 24, "height": edge - 24, "pad": [0, 2, 2, 0]}
  play_btn = {
    "k": "box",
    "key": play_key,
    "s": play_hidden,
    "c": [
      {
        "k": "box",
        "s": {"display": "row", "align": "center", "justify": "center", "width": 48, "height": 48, "radius": 4, "bg": GREEN, "fg": BLACK, "shadow": 2, "cursor": "pointer"},
        "p": {"album": album["id"], "track": 0},
        "on": {"click": "play"},
        "c": [micon("▶", 3, BLACK)]
      }
    ]
  }
  cover = stack({"width": edge - 24, "height": edge - 24}, [mcover(album, edge - 24, 2), play_btn])
  card_node = mhover(
    "card:" + str(album["id"]),
    base,
    hover,
    [cover, mtext(album["title"], 2, "bold", WHITE), mtext(album["artist"], 1, "regular", GREY)],
    "open",
    {"album": album["id"]}
  )
  # A key with a colon is a string in the local language: `"play:6".style`.
  card_node["on"]["pointer_enter"] = {"local": "self.style = @hover; \"" + play_key + "\".style = @shown", "styles": {"hover": hover, "shown": play_shown}}
  card_node["on"]["pointer_leave"] = {"local": "self.style = @base; \"" + play_key + "\".style = @hidden", "styles": {"base": base, "hidden": play_hidden}}
  card_node
end

def music_pick(album)
  base = {"display": "row", "gap": 3, "align": "center", "radius": 1, "bg": PICK, "width": 280, "height": 64, "cursor": "pointer", "pad": [0, 2, 0, 0], "transition": "fast", "grow": 1, "max_width": 420}
  mhover(
    "pick:" + str(album["id"]),
    base,
    base.merge({"bg": PICK_HOVER}),
    [mcover(album, 64, 1), mtext(album["title"], 2, "bold", WHITE)],
    "play",
    {"album": album["id"], "track": 0}
  )
end

def music_section(title, cards)
  column(
    {"gap": 3},
    [
      row({"align": "end"}, [mtext(title, 5, "bold", WHITE), spacer(), {"k": "box", "s": {"cursor": "pointer"}, "p": {"section": title}, "on": {"click": "section"}, "c": [mtext("Show all", 1, "bold", GREY)]}]),
      row({"gap": 3, "wrap": "wrap"}, cards)
    ]
  )
end

def music_topbar(state)
  avatar = {"k": "box", "s": {"display": "row", "align": "center", "justify": "center", "width": 32, "height": 32, "radius": 4, "bg": "#535353", "cursor": "pointer"}, "p": {"view": "profile"}, "on": {"click": "go"}, "c": [mtext("O", 1, "bold", WHITE)]}
  can_back = state["history"].length() > 0
  back = {"k": "box", "s": {"display": "row", "align": "center", "justify": "center", "width": 32, "height": 32, "radius": 4, "bg": BLACK, "cursor": "pointer"}, "on": {"click": "back"}, "c": [micon("‹", 3, can_back ? WHITE : DIM)]}
  fwd = {"k": "box", "s": {"display": "row", "align": "center", "justify": "center", "width": 32, "height": 32, "radius": 4, "bg": BLACK}, "c": [micon("›", 3, DIM)]}
  gear = {"k": "box", "s": {"display": "row", "align": "center", "justify": "center", "width": 32, "height": 32, "radius": 4, "bg": BLACK, "cursor": "pointer"}, "p": {"view": "settings"}, "on": {"click": "go"}, "c": [micon("⚙", 3, GREY)]}
  row(
    {"gap": 2, "align": "center", "pad": [3, 4, 3, 4]},
    [back, fwd, spacer(), gear, avatar]
  )
end

def music_card_edge(state)
  music_layout(state)["compact"] ? 150 : 184
end

def music_home(state)
  edge = music_card_edge(state)
  picks = MUSIC_ALBUMS.slice(0, 6).map(fn(album) { music_pick(album) })
  column(
    {"gap": 6, "pad": [0, 4, 6, 4]},
    [
      mtext("Good afternoon", 6, "bold", WHITE),
      row({"gap": 2, "wrap": "wrap"}, picks),
      music_section("Made For You", MUSIC_ALBUMS.slice(6, 11).map(fn(album) { music_card(album, edge) })),
      music_section("Recently played", MUSIC_ALBUMS.slice(11, 16).map(fn(album) { music_card(album, edge) })),
      music_section("Your top mixes", MUSIC_ALBUMS.slice(16, 21).map(fn(album) { music_card(album, edge) }))
    ]
  )
end

# --------------------------------------------------------------- album

def music_track_row(state, album, track, show_artist)
  current = state["album"] == album["id"] && state["track"] == track["n"] - 1
  id = album["id"] * 100 + track["n"]
  liked = state["liked"].includes?(id)
  num = current && state["playing"] ? micon("▶", 1, GREEN) : mtext(str(track["n"]), 2, "regular", GREY)
  base = {"display": "row", "gap": 4, "align": "center", "pad": [2, 3, 2, 3], "radius": 1, "cursor": "pointer", "transition": "fast"}
  title_col = show_artist ? column({"gap": 0, "grow": 1}, [mtext(track["title"], 2, "medium", current ? GREEN : WHITE), martist(album["artist"], 1, "regular", GREY)]) : column({"gap": 0, "grow": 1}, [mtext(track["title"], 2, "medium", current ? GREEN : WHITE)])
  heart = {
    "k": "box",
    "s": {"pad": [0, 2, 0, 2], "cursor": "pointer"},
    "p": {"id": id},
    "on": {"click": "like"},
    "c": [micon("♥", 2, liked ? GREEN : DIM)]
  }
  numbox = {"k": "box", "s": {"width": 24, "display": "row", "justify": "center", "shrink": 0}, "c": [num]}
  mhover(
    "track:" + str(id),
    base,
    base.merge({"bg": "#2a2a2a"}),
    [numbox, title_col, heart, {"k": "box", "s": {"width": 48, "display": "row", "justify": "end", "shrink": 0}, "c": [mtext(music_clock(track["seconds"]), 1, "regular", GREY)]}],
    "play",
    {"album": album["id"], "track": track["n"] - 1}
  )
end

def music_table_header
  column(
    {"gap": 0},
    [
      row(
        {"gap": 4, "align": "center", "pad": [1, 3, 1, 3]},
        [
          {"k": "box", "s": {"width": 24, "display": "row", "justify": "center"}, "c": [mtext("#", 1, "regular", GREY)]},
          {"k": "box", "s": {"grow": 1}, "c": [mtext("Title", 1, "regular", GREY)]},
          {"k": "box", "s": {"width": 24}},
          {"k": "box", "s": {"width": 48, "display": "row", "justify": "end"}, "c": [micon_clock(GREY)]}
        ]
      ),
      {"k": "box", "s": {"height": 1, "bg": "#2a2a2a", "margin": [0, 3, 2, 3]}}
    ]
  )
end

def music_header(kind, title, meta, tint, cover)
  row(
    {"gap": 5, "align": "end", "pad": [6, 4, 4, 4], "bg": tint, "wrap": "wrap"},
    [
      cover,
      column(
        {"gap": 2, "pad": [0, 0, 1, 0]},
        [
          mtext(kind, 1, "medium", WHITE),
          mtext(title, 7, "bold", WHITE),
          meta
        ]
      )
    ]
  )
end

def music_actions(album, liked_any)
  row(
    {"gap": 4, "align": "center", "pad": [4, 4, 2, 4]},
    [
      mcircle("▶", 56, GREEN, BLACK, "play", {"album": album["id"], "track": 0}),
      micon("♥", 5, liked_any ? GREEN : GREY),
      micon("•••", 3, GREY)
    ]
  )
end

def music_album_view(state)
  album = MUSIC_ALBUMS[state["open"] % MUSIC_ALBUMS.length()]
  total = int(album["tracks"].map(fn(t) { t["seconds"] }).sum())
  meta = row(
    {"gap": 2, "align": "center"},
    [
      {"k": "box", "s": {"width": 24, "height": 24, "radius": 4, "bg": "#535353", "display": "row", "align": "center", "justify": "center"}, "c": [mtext(music_initial(album["artist"]), 0, "bold", WHITE)]},
      martist(album["artist"], 2, "bold", WHITE),
      mtext("• " + str(album["year"]) + " • 8 songs, " + music_long(total), 2, "regular", GREY)
    ]
  )
  cover = {"k": "image", "p": {"src": album["cover"]}, "s": {"width": 232, "height": 232, "radius": 1, "shadow": 3, "shrink": 0}}
  liked_any = album["tracks"].filter(fn(t) { state["liked"].includes?(album["id"] * 100 + t["n"]) }).length() > 0
  column(
    {"gap": 0},
    [
      music_header("Album", album["title"], meta, album["tint"], cover),
      music_actions(album, liked_any),
      column({"gap": 0, "pad": [2, 4, 6, 4]}, [music_table_header()].concat(album["tracks"].map(fn(t) { music_track_row(state, album, t, false) })))
    ]
  )
end

# ------------------------------------------------------------ playlist

def music_playlist(state, name)
  picks = range(0, 12).map(fn(i) { MUSIC_ALBUMS[(i * 7 + name.length()) % MUSIC_ALBUMS.length()] })
  rows = range(0, 12).map(fn(i) { music_track_row(state, picks[i], picks[i]["tracks"][(i * 3) % 8], true) })
  tint = MUSIC_TINTS[name.length() % 8]
  cover = {"k": "box", "s": {"width": 232, "height": 232, "radius": 1, "shadow": 3, "shrink": 0, "bg": tint == MUSIC_TINTS[0] ? "#5038a0" : "#3a3a3a", "display": "row", "align": "center", "justify": "center"}, "c": [micon("♥", 7, WHITE)]}
  meta = mtext("Soundly • 12 songs, about 45 min", 2, "regular", GREY)
  column(
    {"gap": 0},
    [
      music_header("Playlist", name, meta, tint, cover),
      music_actions(picks[0], false),
      column({"gap": 0, "pad": [2, 4, 6, 4]}, [music_table_header()].concat(rows))
    ]
  )
end

# -------------------------------------------------------------- search

def music_genre_tile(genre)
  {
    "k": "box",
    "s": {"width": 184, "height": 100, "radius": 2, "bg": genre[1], "pad": [3, 3, 3, 3], "cursor": "pointer"},
    "p": {"genre": genre[0]},
    "on": {"click": "genre"},
    "c": [mtext(genre[0], 4, "bold", WHITE)]
  }
end

def music_search(state)
  q = state["query"].downcase()
  hits = MUSIC_ALBUMS.filter(fn(album) { q != "" && (album["title"].downcase().includes?(q) || album["artist"].downcase().includes?(q)) })
  field = input(state["query"], "search")
  field["s"] = {"width": 420, "radius": 4, "bg": "#242424", "fg": WHITE, "pad": [2, 4, 2, 4], "border": 0}
  results = q == "" ? column({"gap": 4}, [mtext("Browse all", 5, "bold", WHITE), row({"gap": 3, "wrap": "wrap"}, music_genres().map(fn(g) { music_genre_tile(g) }))]) : (hits.length() == 0 ? mtext("No results found for \"" + state["query"] + "\"", 3, "bold", WHITE) : music_section("Albums", hits.map(fn(album) { music_card(album, 184) })))
  column(
    {"gap": 5, "pad": [0, 4, 6, 4]},
    [row({"gap": 3, "align": "center"}, [micon("⌕", 4, GREY), field]), results]
  )
end

def music_library_view(state)
  column(
    {"gap": 5, "pad": [0, 4, 6, 4]},
    [mtext("Albums", 6, "bold", WHITE), row({"gap": 3, "wrap": "wrap"}, MUSIC_ALBUMS.map(fn(album) { music_card(album, 184) }))]
  )
end

# --------------------------------------------------------------- artist

def music_artist_view(state)
  name = state["artist"]
  albums = music_artist_albums(name)
  albums = albums.length() == 0 ? [MUSIC_ALBUMS[name.length() % MUSIC_ALBUMS.length()]] : albums
  first = albums[0]
  popular = range(0, 5).map(fn(i) {
    album = albums[i % albums.length()]
    track = album["tracks"][(i * 3) % 8]
    plays = str(1 + (name.length() * 7 + i * 13) % 40) + "," + str(100 + (i * 271) % 900) + "," + str(100 + (i * 397) % 900)
    row_node = music_track_row(state, album, track, false)
    current = state["album"] == album["id"] && state["track"] == track["n"] - 1
    num = {"k": "box", "s": {"width": 24, "display": "row", "justify": "center", "shrink": 0}, "c": [current && state["playing"] ? micon("▶", 1, GREEN) : mtext(str(i + 1), 2, "regular", GREY)]}
    row_node["c"] = [num, mcover(album, 40, 1), row_node["c"][1], mtext(plays, 1, "regular", GREY), row_node["c"][2], row_node["c"][3]]
    row_node
  })
  following = state["following"].includes?(name)
  listeners = str(1 + name.length() % 9) + "," + str(100 + (name.length() * 37) % 900) + "," + str(100 + (name.length() * 59) % 900)
  avatar = {"k": "box", "s": {"width": 232, "height": 232, "radius": 4, "shrink": 0, "shadow": 3, "display": "row", "align": "center", "justify": "center", "bg": first["tint"]}, "c": [mtext(music_initial(name), 7, "bold", WHITE)]}
  meta = mtext(listeners + " monthly listeners", 2, "regular", WHITE)
  kind = row({"gap": 1, "align": "center"}, [micon("✓", 1, "#4cb3ff"), mtext("Verified Artist", 1, "medium", WHITE)])
  follow_btn = {"k": "box", "s": {"pad": [1, 3, 1, 3], "radius": 4, "border": 1, "border_color": following ? WHITE : GREY, "cursor": "pointer"}, "p": {"artist": name}, "on": {"click": "follow"}, "c": [mtext(following ? "Following" : "Follow", 1, "bold", WHITE)]}
  actions = row({"gap": 4, "align": "center", "pad": [4, 4, 2, 4]}, [mcircle("▶", 56, GREEN, BLACK, "play", {"album": first["id"], "track": 0}), follow_btn, micon("•••", 3, GREY)])
  fans = MUSIC_ARTISTS.filter(fn(a) { a != name }).slice(0, 5).map(fn(a) {
    tint = MUSIC_TINTS[a.length() % 8]
    base = {"display": "column", "gap": 2, "pad": [3, 3, 3, 3], "radius": 2, "width": 184, "bg": CARD, "cursor": "pointer", "align": "center", "transition": "fast", "shrink": 0}
    mhover("fan:" + a, base, base.merge({"bg": CARD_HOVER}), [{"k": "box", "s": {"width": 160, "height": 160, "radius": 4, "bg": tint, "display": "row", "align": "center", "justify": "center"}, "c": [mtext(music_initial(a), 7, "bold", WHITE)]}, mtext(a, 2, "bold", WHITE), mtext("Artist", 1, "regular", GREY)], "artist", {"artist": a})
  })
  about = {
    "k": "box",
    "s": {"display": "column", "gap": 2, "pad": [4, 4, 4, 4], "radius": 2, "bg": CARD, "width": 560},
    "c": [mtext(listeners + " monthly listeners", 2, "bold", WHITE), mtext(name + " writes the kind of songs that sound like the end of a long drive: wide, patient, and a little brighter than the road deserves. Four albums in, the band still records live in one room.", 2, "regular", GREY)]
  }
  column(
    {"gap": 0},
    [
      row({"gap": 5, "align": "end", "pad": [6, 4, 4, 4], "bg": first["tint"]}, [avatar, column({"gap": 2, "pad": [0, 0, 1, 0]}, [kind, mtext(name, 7, "bold", WHITE), meta])]),
      actions,
      column(
        {"gap": 6, "pad": [2, 4, 6, 4]},
        [
          column({"gap": 2}, [mtext("Popular", 5, "bold", WHITE), column({"gap": 0}, popular)]),
          music_section("Discography", albums.map(fn(album) { music_card(album, 184) })),
          column({"gap": 3}, [mtext("Fans also like", 5, "bold", WHITE), row({"gap": 3, "wrap": "wrap"}, fans)]),
          column({"gap": 3}, [mtext("About", 5, "bold", WHITE), about])
        ]
      )
    ]
  )
end

# ---------------------------------------------------------- liked songs

def music_liked_view(state)
  pairs = music_liked_tracks(state)
  rows = pairs.map(fn(p) { music_track_row(state, p[0], p[1], true) })
  cover = {"k": "box", "s": {"width": 232, "height": 232, "radius": 1, "shadow": 3, "shrink": 0, "bg": "#5038a0", "display": "row", "align": "center", "justify": "center"}, "c": [micon("♥", 7, WHITE)]}
  meta = row({"gap": 1, "align": "center"}, [mtext("Olivier", 2, "bold", WHITE), mtext("• " + str(pairs.length()) + " songs", 2, "regular", GREY)])
  body = pairs.length() == 0 ? column({"gap": 2, "pad": [6, 4, 6, 4], "align": "center"}, [mtext("Songs you like will appear here", 4, "bold", WHITE), mtext("Save songs by tapping the heart icon.", 2, "regular", GREY)]) : column({"gap": 0, "pad": [2, 4, 6, 4]}, [music_table_header()].concat(rows))
  column(
    {"gap": 0},
    [
      music_header("Playlist", "Liked Songs", meta, "#3a2a7a", cover),
      pairs.length() == 0 ? {"k": "box"} : music_actions(pairs[0][0], true),
      body
    ]
  )
end

# ---------------------------------------------------------------- queue

def music_queue_view(state)
  album = music_current(state)
  now = music_current_track(state)
  next_up = range(state["track"] + 1, 8).map(fn(t) { music_track_row(state, album, album["tracks"][t], true) })
  more = MUSIC_ALBUMS[(state["album"] + 1) % MUSIC_ALBUMS.length()]
  column(
    {"gap": 5, "pad": [0, 4, 6, 4]},
    [
      mtext("Queue", 6, "bold", WHITE),
      column({"gap": 2}, [mtext("Now playing", 3, "bold", WHITE), music_track_row(state, album, now, true)]),
      column({"gap": 2}, [mtext("Next from: " + album["title"], 3, "bold", WHITE), column({"gap": 0}, next_up)]),
      column({"gap": 2}, [mtext("Next up: " + more["title"], 3, "bold", GREY), column({"gap": 0}, more["tracks"].slice(0, 3).map(fn(t) { music_track_row(state, more, t, true) }))])
    ]
  )
end

# ------------------------------------------------------- section, genre

def music_section_view(state)
  name = state["section"]
  albums = name == "Made For You" ? MUSIC_ALBUMS.slice(6, 16) : (name == "Recently played" ? MUSIC_ALBUMS.slice(11, 24) : MUSIC_ALBUMS.slice(0, 24))
  column({"gap": 5, "pad": [0, 4, 6, 4]}, [mtext(name, 6, "bold", WHITE), row({"gap": 3, "wrap": "wrap"}, albums.map(fn(album) { music_card(album, 184) }))])
end

def music_genre_view(state)
  name = state["genre"]
  found = music_genres().filter(fn(g) { g[0] == name })
  colour = found.length() == 0 ? "#535353" : found[0][1]
  picks = MUSIC_ALBUMS.filter(fn(album) { (album["id"] + name.length()) % 3 != 0 })
  column(
    {"gap": 0},
    [
      {"k": "box", "s": {"display": "column", "justify": "end", "height": 200, "pad": [4, 4, 4, 4], "bg": colour}, "c": [mtext(name, 7, "bold", WHITE)]},
      column({"gap": 5, "pad": [4, 4, 6, 4]}, [music_section("Popular in " + name, picks.slice(0, 5).map(fn(album) { music_card(album, 184) })), music_section("New releases", picks.slice(5, 10).map(fn(album) { music_card(album, 184) })), music_section("Playlists", MUSIC_PLAYLISTS.slice(0, 4).map(fn(pl) {
        base = {"display": "column", "gap": 2, "pad": [3, 3, 3, 3], "radius": 2, "width": 184, "bg": CARD, "cursor": "pointer", "transition": "fast", "shrink": 0}
        mhover("gpl:" + pl, base, base.merge({"bg": CARD_HOVER}), [{"k": "box", "s": {"width": 160, "height": 160, "radius": 2, "bg": colour, "display": "row", "align": "center", "justify": "center"}, "c": [micon("♪", 7, WHITE)]}, mtext(pl, 2, "bold", WHITE), mtext("Playlist • Soundly", 1, "regular", GREY)], "go", {"view": "playlist:" + pl})
      }))])
    ]
  )
end

# ---------------------------------------------------- profile, settings

def music_profile_view(state)
  top = MUSIC_ARTISTS.slice(0, 5).map(fn(a) {
    base = {"display": "column", "gap": 2, "pad": [3, 3, 3, 3], "radius": 2, "width": 184, "bg": CARD, "cursor": "pointer", "align": "center", "transition": "fast", "shrink": 0}
    mhover("top:" + a, base, base.merge({"bg": CARD_HOVER}), [{"k": "box", "s": {"width": 160, "height": 160, "radius": 4, "bg": MUSIC_TINTS[a.length() % 8], "display": "row", "align": "center", "justify": "center"}, "c": [mtext(music_initial(a), 7, "bold", WHITE)]}, mtext(a, 2, "bold", WHITE), mtext("Artist", 1, "regular", GREY)], "artist", {"artist": a})
  })
  avatar = {"k": "box", "s": {"width": 232, "height": 232, "radius": 4, "shrink": 0, "shadow": 3, "display": "row", "align": "center", "justify": "center", "bg": "#535353"}, "c": [mtext("O", 7, "bold", WHITE)]}
  meta = mtext(str(MUSIC_PLAYLISTS.length()) + " Public Playlists • " + str(state["following"].length()) + " Following", 2, "regular", WHITE)
  column(
    {"gap": 0},
    [
      row({"gap": 5, "align": "end", "pad": [6, 4, 4, 4], "bg": "#2e3b4d"}, [avatar, column({"gap": 2, "pad": [0, 0, 1, 0]}, [mtext("Profile", 1, "medium", WHITE), mtext("Olivier", 7, "bold", WHITE), meta])]),
      column(
        {"gap": 6, "pad": [4, 4, 6, 4]},
        [
          column({"gap": 3}, [mtext("Top artists this month", 5, "bold", WHITE), mtext("Only visible to you", 1, "regular", GREY), row({"gap": 3, "wrap": "wrap"}, top)]),
          music_section("Public playlists", MUSIC_PLAYLISTS.slice(0, 5).map(fn(pl) {
            base = {"display": "column", "gap": 2, "pad": [3, 3, 3, 3], "radius": 2, "width": 184, "bg": CARD, "cursor": "pointer", "transition": "fast", "shrink": 0}
            mhover("ppl:" + pl, base, base.merge({"bg": CARD_HOVER}), [{"k": "box", "s": {"width": 160, "height": 160, "radius": 2, "bg": MUSIC_TINTS[pl.length() % 8], "display": "row", "align": "center", "justify": "center"}, "c": [micon("♪", 7, WHITE)]}, mtext(pl, 2, "bold", WHITE), mtext("By Olivier", 1, "regular", GREY)], "go", {"view": "playlist:" + pl})
          }))
        ]
      )
    ]
  )
end

def music_setting_switch(label, sub, on, event)
  toggle = {
    "k": "box",
    "s": {"display": "row", "align": "center", "justify": on ? "end" : "start", "width": 40, "height": 22, "radius": 4, "bg": on ? GREEN : "#535353", "cursor": "pointer", "pad": [0, 2, 0, 2], "shrink": 0},
    "on": {"click": event},
    "c": [{"k": "box", "s": {"width": 18, "height": 18, "radius": 4, "bg": WHITE}}]
  }
  row({"gap": 4, "align": "center", "pad": [3, 0, 3, 0]}, [column({"gap": 0, "grow": 1}, [mtext(label, 2, "medium", WHITE), mtext(sub, 1, "regular", GREY)]), toggle])
end

def music_settings_view(state)
  qualities = ["Low", "Normal", "High", "Very high"]
  quality = row({"gap": 1}, qualities.map(fn(q) {
    on = q == state["quality"]
    {"k": "box", "s": {"pad": [1, 3, 1, 3], "radius": 4, "bg": on ? WHITE : "#2a2a2a", "cursor": "pointer"}, "p": {"quality": q}, "on": {"click": "quality"}, "c": [mtext(q, 1, "medium", on ? BLACK : WHITE)]}
  }))
  group = fn(title, children) {
    column({"gap": 1, "pad": [0, 0, 4, 0]}, [mtext(title, 4, "bold", WHITE)].concat(children))
  }
  column(
    {"gap": 3, "pad": [0, 4, 6, 4], "max_width": 720},
    [
      mtext("Settings", 6, "bold", WHITE),
      group("Audio quality", [row({"gap": 4, "align": "center", "pad": [3, 0, 3, 0]}, [column({"gap": 0, "grow": 1}, [mtext("Streaming quality", 2, "medium", WHITE), mtext("Very high uses about 150 MB per hour.", 1, "regular", GREY)]), quality]), music_setting_switch("Normalize volume", "Set the same volume level for all tracks and podcasts.", state["normalize"], "normalize")]),
      group("Playback", [music_setting_switch("Autoplay", "Keep listening to similar songs when your music ends.", state["autoplay"], "autoplay")]),
      group("About", [mtext("Soundly for EUI — a sample application of the counter-app.", 2, "regular", GREY), mtext("Every page here is a Soli function returning a tree; every click is a patch.", 1, "regular", GREY)])
    ]
  )
end

def music_main(state)
  view = state["view"]
  body = music_home(state)
  if view == "album"
    body = music_album_view(state)
  elsif view == "search"
    body = music_search(state)
  elsif view == "library"
    body = music_library_view(state)
  elsif view == "artist"
    body = music_artist_view(state)
  elsif view == "queue"
    body = music_queue_view(state)
  elsif view == "section"
    body = music_section_view(state)
  elsif view == "genre"
    body = music_genre_view(state)
  elsif view == "profile"
    body = music_profile_view(state)
  elsif view == "settings"
    body = music_settings_view(state)
  elsif view == "playlist:Liked Songs"
    body = music_liked_view(state)
  elsif view.starts_with?("playlist:")
    body = music_playlist(state, view.replace("playlist:", ""))
  end
  scroll({"grow": 1, "min_height": 0, "bg": PANEL, "radius": 3}, [column({"gap": 0}, [music_topbar(state), body])])
end

# --------------------------------------------------------- now playing

def music_bar(width, height, fraction, fill)
  filled = int(width * fraction)
  {
    "k": "box",
    "s": {"display": "row", "align": "center", "width": width, "height": height, "radius": 4, "bg": DIM, "cursor": "pointer"},
    "c": [{"k": "box", "s": {"width": filled, "height": height, "radius": 4, "bg": fill}}]
  }
end

def music_now_playing(state)
  layout = music_layout(state)
  album = music_current(state)
  track = music_current_track(state)
  duration = track["seconds"]
  position = state["position"] < duration ? state["position"] : duration
  id = album["id"] * 100 + track["n"]
  liked = state["liked"].includes?(id)
  seek = music_bar(layout["narrow"] ? 200 : (layout["compact"] ? 320 : 480), 4, position * 1.0 / duration, WHITE)
  seek["s"]["shrink"] = 0
  seek["on"] = {"click": "seek"}
  vol = music_bar(93, 4, state["volume"] / 100.0, WHITE)
  vol["on"] = {"click": "volume"}
  play_glyph = state["playing"] ? "▮▮" : "▶"
  row(
    {"gap": 4, "align": "center", "pad": [2, 3, 2, 3], "height": 80, "bg": BLACK, "shrink": 0},
    [
      row(
        {"gap": 3, "align": "center", "grow": 1, "width": 0, "min_width": layout["narrow"] ? 120 : 200},
        [
          mcover(album, 56, 1),
          column({"gap": 0}, [mtext(track["title"], 2, "medium", WHITE), martist(album["artist"], 0, "regular", GREY)]),
          {"k": "box", "s": {"pad": [0, 2, 0, 2], "cursor": "pointer"}, "p": {"id": id}, "on": {"click": "like"}, "c": [micon("♥", 2, liked ? GREEN : GREY)]}
        ]
      ),
      column(
        {"gap": 1, "align": "center", "shrink": 0},
        [
          row(
            {"gap": 4, "align": "center"},
            [
              {"k": "box", "s": {"cursor": "pointer"}, "on": {"click": "shuffle"}, "c": [micon_shuffle(state["shuffle"] ? GREEN : GREY)]},
              {"k": "box", "s": {"cursor": "pointer"}, "on": {"click": "prev"}, "c": [micon("◀◀", 2, GREY)]},
              mcircle(play_glyph, 32, WHITE, BLACK, "toggle", nil),
              {"k": "box", "s": {"cursor": "pointer"}, "on": {"click": "next"}, "c": [micon("▶▶", 2, GREY)]},
              {"k": "box", "s": {"cursor": "pointer"}, "on": {"click": "repeat"}, "c": [micon_repeat(state["repeat"] ? GREEN : GREY)]}
            ]
          ),
          row(
            {"gap": 2, "align": "center", "shrink": 0},
            [
              {"k": "box", "s": {"width": 40, "display": "row", "justify": "end", "shrink": 0}, "c": [mtext(music_clock(position), 0, "regular", GREY)]},
              seek,
              {"k": "box", "s": {"width": 40, "shrink": 0}, "c": [mtext(music_clock(duration), 0, "regular", GREY)]}
            ]
          )
        ]
      ),
      layout["compact"] ? {"k": "box", "s": {"grow": 1, "width": 0}} : row({"gap": 3, "align": "center", "grow": 1, "width": 0, "min_width": 200, "justify": "end"}, [micon_lyrics(GREY), {"k": "box", "s": {"cursor": "pointer"}, "p": {"view": "queue"}, "on": {"click": "go"}, "c": [micon_queue(state["view"] == "queue" ? GREEN : GREY)]}, micon_device(GREY), micon_volume(GREY), vol, micon_expand(GREY)])
    ]
  )
end

def music_view(raw_state)
  state = music_defaults(raw_state)
  column(
    {"gap": 2, "bg": BLACK, "pad": [2, 2, 0, 2]},
    [
      # `min_height: 0`: the row holds scrollers, and without it its
      # automatic minimum (04 §4.3) would be the whole page's height.
      row({"gap": 2, "grow": 1, "min_height": 0, "align": "stretch"}, music_layout(state)["narrow"] ? [music_main(state)] : [music_layout(state)["compact"] ? music_rail(state) : music_sidebar(state), music_main(state)]),
      music_now_playing(state)
    ]
  )
end
