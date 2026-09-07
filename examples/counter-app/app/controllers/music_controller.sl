# Needle: a small player that finds a record and plays it. A field at the
# top, a rail of what the search found, one pane for the thing you picked,
# a bar at the bottom. It is not shaped like anybody's product: it takes
# its colours from the theme (05 §1), so it follows the viewer into dark
# mode without the server ever knowing which one they chose, and every
# sleeve is *drawn* from the record's own name — five arcs and a label on
# a canvas — so the catalogue needs no image asset and no picture fetch.
#
# The catalogue is Spotify's when SPOTIFY_CLIENT_ID and
# SPOTIFY_CLIENT_SECRET are in the app's `.env` (client credentials:
# search, artist, album), and a small generated one otherwise, so the
# sample runs with nothing configured at all.
#
# Playing: EUI itself can make a sound — 03 §7 gives the tree an `audio`
# node whose `src` is an asset, decoded in the confined worker and mixed
# by the window — but neither of this app's catalogues hands it any bytes
# to play. Spotify's Web API returns metadata and never audio, and the
# generated catalogue is names and durations. So `play` asks Spotify
# Connect to start the track on a device you already have open, when an
# account is linked; otherwise the bar keeps its own time, and says which
# of the two happened rather than pretending.
#
# What moves, and why so little: 03 §5 animates `bg`, `fg`,
# `border_color` and `opacity` on a style change and nothing else — layout
# never runs per frame. So the pieces that persist across screens keep
# their keys and change only colour: the band behind a record morphs from
# one record's hue to the next over `motion.slow`, a row's highlight fades
# across the rail, and the disc in the bar turns (`animation: spin`) for
# exactly as long as something is playing.

NEEDLE_ARTISTS = [
  "Nova Reyes",
  "The Low Tide",
  "Ines Marlow",
  "Kappa Sun",
  "Orbital Fields",
  "June & the Motors",
  "Halden",
  "Coral Static",
  "Mira Voss",
  "Pale Harbour"
]
NEEDLE_TITLES = [
  "Night Drive",
  "Salt",
  "Everything After",
  "Slow Light",
  "Copper Sky",
  "Undertow",
  "Northern Rooms",
  "Glass Hours",
  "Afterglow",
  "Small Hours",
  "Second Summer",
  "Static Bloom"
]
NEEDLE_WORDS = [
  "Intro",
  "Runaway",
  "Signal",
  "Marble",
  "Weightless",
  "Fever",
  "Skyline",
  "Tides",
  "Drift",
  "Fold",
  "Ember",
  "Echoes",
  "Velvet",
  "Meridian",
  "Circles",
  "Wander"
]

NEEDLE_API = "https://api.spotify.com/v1"
NEEDLE_TOKEN_URL = "https://accounts.spotify.com/api/token"

# ------------------------------------------------------- a name's colour

# One stable number per name. The sleeve's hues, a sample record's year
# and its track lengths all come from here, so a name always draws the
# same record — the generated catalogue and Spotify's ids alike.
def needle_hash(name)
  b = name.bytes()
  int(range(0, b.length()).map(fn(i) { b[i] * (i * 7 + 13) }).sum()) % 100003
end

def needle_hue(name)
  needle_hash(name) % 360
end

def needle_hex2(n)
  digits = "0123456789abcdef"
  v = n < 0 ? 0 : (n > 255 ? 255 : n)
  digits[v / 16] + digits[v % 16]
end

def needle_channel(p, q, t)
  u = t < 0 ? t + 360 : (t >= 360 ? t - 360 : t)
  return p + (q - p) * u / 60.0 if u < 60
  return q if u < 180
  return p + (q - p) * (240.0 - u) / 60.0 if u < 240

  p
end

# HSL — hue 0–359, saturation and lightness 0–100 — as `#rrggbb`. The one
# place this app spends a literal colour is a record's own hue; everything
# else is a role.
def needle_hsl(h, s, l)
  ll = l / 100.0
  ss = s / 100.0
  q = ll < 0.5 ? ll * (1.0 + ss) : ll + ss - ll * ss
  p = 2.0 * ll - q
  r = needle_channel(p, q, h + 120)
  g = needle_channel(p, q, h)
  b = needle_channel(p, q, h - 120)
  "#" + needle_hex2(int(r * 255)) + needle_hex2(int(g * 255)) + needle_hex2(int(b * 255))
end

# The band behind a record: deep enough that white text sits on it in
# either mode, and far enough from its neighbour that the morph reads.
def needle_tint(name)
  needle_hsl(needle_hue(name), 44, 21)
end

def needle_glow(name)
  needle_hsl(needle_hue(name), 68, 60)
end

# ------------------------------------------------------------- a sleeve

# A record's sleeve, drawn: grooves struck at angles the name chooses, a
# label, a spindle hole. No asset, no fetch, and the same name always
# draws the same sleeve — which is what makes a rail of them legible.
def needle_sleeve(name, edge, radius)
  h = needle_hash(name)
  hue = h % 360
  cx = edge / 2.0
  cy = edge / 2.0
  # A canvas paints its paths inside its border box but the box's radius
  # does not clip them (03 §1.1), so a round sleeve is a drawn disc.
  square = [1, "surface.sunken", 0, 0, edge, edge, 0]
  base = radius == 4 ? [
    3,
    "surface.sunken",
    cx,
    cy,
    edge / 2.0
  ] : square
  grooves = range(0, 5).map(fn(i) {
    r = edge * (17 + i * 8) / 100.0
    span = 1.1 + ((h / (i + 2)) % 42) / 10.0
    start = ((h * (i + 3)) % 360) * 3.14159 / 180.0;
    [4, needle_hsl((hue + i * 13) % 360, 62, 40 + i * 6), edge / 44.0 + 1.0, cx, cy, r, start, start + span]
  })
  label = [3, needle_hsl(hue, 70, 56), cx, cy, edge / 10.0]
  hole = [3, "surface.raised", cx, cy, edge / 46.0]
  sleeve = canvas(edge, edge, [base].concat(grooves).concat([label, hole]))
  sleeve["s"]["radius"] = radius
  sleeve["s"]["shrink"] = 0
  sleeve
end

# The mark: a tonearm over a record, the app's whole identity in 22 px.
def needle_mark
  canvas(22, 22, [
    [3, "accent.base", 11, 11, 9],
    [3, "surface.raised", 11, 11, 3],
    [0, "surface.base", 2.0, 4, 4, 13, 13],
    [3, "surface.base", 4, 4, 2]
  ])
end

def needle_glass(tone)
  canvas(16, 16, [ [4, tone, 1.7, 7, 7, 5, 0, 6.28], [
    0,
    tone,
    1.7,
    11,
    11,
    15,
    15
  ]])
end

# --------------------------------------------------------- the catalogue

def needle_clock(seconds)
  m = seconds / 60
  s = seconds % 60
  str(m) + ":" + (s < 10 ? "0" + str(s) : str(s))
end

def needle_market
  getenv("SPOTIFY_MARKET") ?? "FR"
end

def needle_configured
  id = getenv("SPOTIFY_CLIENT_ID")
  secret = getenv("SPOTIFY_CLIENT_SECRET")
  return false if id.nil? || secret.nil?

  id != "" && secret != ""
end

# The client-credentials token, kept in the process cache for its hour.
# Every failure here returns nil and the caller falls back to the sample
# catalogue: a missing key is a state of the app, not a crash.
# The three grants Needle uses all post a form to the same endpoint and
# all answer with the same hash, so they share one door. `Cache` is a
# best-effort here and nothing more: it is backed by SoliKV, and the
# desktop build starts no database, so every one of these calls is
# written to work with the cache missing.
def needle_credentials
  id = "client_id=" + url_encode(getenv("SPOTIFY_CLIENT_ID"))
  secret = "client_secret=" + url_encode(getenv("SPOTIFY_CLIENT_SECRET"));
  [id, secret]
end

def needle_token_post(form)
  kind = "application/x-www-form-urlencoded"
  opts = {"headers": {"Content-Type": kind}, "timeout": 10}
  resp = HTTP.request("POST", NEEDLE_TOKEN_URL, opts, form) rescue nil
  return nil if resp.nil?
  return nil if resp["status"] != 200

  json_parse(resp["body"]) rescue nil
end

# The catalogue's own token: no person, no consent, no callback.
def needle_token
  hit = Cache.get("needle:token") rescue nil
  return hit unless hit.nil?

  grant = ["grant_type=client_credentials"].concat(needle_credentials())
  data = needle_token_post(grant.join("&"))
  return nil if data.nil?

  token = data["access_token"]
  Cache.set("needle:token", token, 3000) rescue nil
  token
end

# ------------------------------------------------- the person's account

# Where Spotify sends the browser back. It has to match the redirect URI
# registered on the dashboard character for character; since April 2025
# Spotify takes only HTTPS or the loopback IP — `localhost` is refused,
# `127.0.0.1` is not — so the desktop app pins its port to 8888.
def needle_redirect
  getenv("SPOTIFY_REDIRECT_URI") ?? "http://127.0.0.1:8888/spotify/callback"
end

def needle_login_url
  needle_redirect().replace("/spotify/callback", "/spotify/login")
end

# `state` proves the callback belongs to a login this app started. The
# usual nonce would have to survive between two requests, and with no
# database there is nowhere to keep one — so it is derived from the
# client secret instead: an attacker who cannot read the secret cannot
# forge it, which is what the parameter is for.
def needle_login_state
  Crypto.sha256(getenv("SPOTIFY_CLIENT_SECRET") + ":needle-login").substring(0, 32)
end

def needle_authorize_url
  scope = "user-modify-playback-state user-read-playback-state"
  query = [
    "response_type=code",
    "client_id=" + url_encode(getenv("SPOTIFY_CLIENT_ID")),
    "scope=" + url_encode(scope),
    "redirect_uri=" + url_encode(needle_redirect()),
    "state=" + needle_login_state()
  ]
  "https://accounts.spotify.com/authorize?" + query.join("&")
end

# The code from the callback, traded for a pair of tokens.
def needle_exchange(code)
  grant = ["grant_type=authorization_code", "code=" + url_encode(code), "redirect_uri="
  + url_encode(needle_redirect())].concat(needle_credentials())
  needle_token_post(grant.join("&"))
end

# A refresh token is good for months; the access token it mints lasts an
# hour. With no cache to hold one, Needle simply mints a fresh one before
# a play — one extra call, and nothing to expire in a drawer.
def needle_refresh(refresh)
  grant = ["grant_type=refresh_token", "refresh_token=" + url_encode(refresh)].concat(needle_credentials())
  data = needle_token_post(grant.join("&"))
  return nil if data.nil?

  token = data["access_token"]
  Cache.set("needle:user", token, 3000) rescue nil
  token
end

def needle_user_token
  hit = Cache.get("needle:user") rescue nil
  return hit unless hit.nil?

  refresh = getenv("SPOTIFY_REFRESH_TOKEN") ?? ""
  return needle_refresh(refresh) if refresh != ""

  given = getenv("SPOTIFY_USER_TOKEN") ?? ""
  given == "" ? nil : given
end

# Whether an account is linked at all — read from the environment only,
# because the view asks on every frame and must never reach the network.
def needle_linked
  return false unless needle_configured();

  (getenv("SPOTIFY_REFRESH_TOKEN") ?? "") != "" || (getenv("SPOTIFY_USER_TOKEN") ?? "") != ""
end

def needle_api(path)
  token = needle_token()
  return nil if token.nil?

  opts = {"headers": {"Authorization": "Bearer " + token}, "timeout": 10}
  resp = HTTP.request("GET", NEEDLE_API + path, opts) rescue nil
  return nil if resp.nil?
  return nil if resp["status"] != 200

  json_parse(resp["body"]) rescue nil
end

def needle_year(date)
  date.nil? ? "" : date.substring(0, 4)
end

def needle_album_stub(a)
  artists = a["artists"] ?? []
  name = artists.length() > 0 ? artists[0]["name"] : "Unknown"
  aid = artists.length() > 0 ? artists[0]["id"] : ""
  {
    "id": a["id"],
    "title": a["name"],
    "artist": name,
    "artist_id": aid,
    "year": needle_year(a["release_date"]),
    "uri": a["uri"] ?? ""
  }
end

# ------------------------------------------------- the sample catalogue

def needle_sample_album(i)
  title = NEEDLE_TITLES[i % NEEDLE_TITLES.length()]
  artist = NEEDLE_ARTISTS[(i * 5) % NEEDLE_ARTISTS.length()]
  h = needle_hash(title + artist)
  tracks = range(0, 8 + h % 3).map(fn(t) {
    {
      "n": t + 1,
      "title": NEEDLE_WORDS[(h + t * 3) % NEEDLE_WORDS.length()] + (t % 5 == 4 ? " (Reprise)" : ""),
      "seconds": 138 + ((h + t * 53) % 200),
      "uri": ""
    }
  })
  {
    "id": "sample:" + str(i),
    "title": title,
    "artist": artist,
    "artist_id": "artist:" + artist,
    "year": str(2005 + h % 20),
    "uri": "",
    "tracks": tracks
  }
end

def needle_sample_albums
  range(0, 12).map(fn(i) { needle_sample_album(i) })
end

def needle_stub_of(album)
  {
    "id": album["id"],
    "title": album["title"],
    "artist": album["artist"],
    "artist_id": album["artist_id"],
    "year": album["year"],
    "uri": album["uri"]
  }
end

def needle_sample_search(q)
  ql = q.downcase()
  albums = needle_sample_albums()
  hits = albums.filter(fn(a) { a["title"].downcase().includes?(ql) || a["artist"].downcase().includes?(ql) })
  found = hits.length() > 0 ? hits : albums.slice(0, 6)
  names = NEEDLE_ARTISTS.filter(fn(n) { n.downcase().includes?(ql) })
  picked = names.length() > 0 ? names : NEEDLE_ARTISTS.slice(0, 3)
  {
    "artists": picked.slice(0, 6).map(fn(n) { {
      "id": "artist:" + n,
      "name": n
    } }),
    "albums": found.slice(0, 8).map(fn(a) { needle_stub_of(a) }),
    "error": ""
  }
end

def needle_sample_artist(id)
  name = id.replace("artist:", "")
  mine = needle_sample_albums().filter(fn(a) { a["artist"] == name })
  albums = mine.length() > 0 ? mine : [needle_sample_album(needle_hash(name) % 12)]
  first = albums[0]
  top = first["tracks"].slice(0, 5).map(fn(t) {
    {
      "n": 0,
      "title": t["title"],
      "seconds": t["seconds"],
      "uri": "",
      "album": first["title"],
      "album_id": first["id"]
    }
  })
  {
    "id": id,
    "name": name,
    "genres": "from the sample catalogue",
    "albums": albums.map(fn(a) { needle_stub_of(a) }),
    "top": top
  }
end

# ----------------------------------------------- one catalogue, two backs

def needle_search(q)
  return needle_sample_search(q) unless needle_configured()

  data = needle_api("/search?type=artist,album&limit=8&q=" + url_encode(q))
  if data.nil?
    return {
      "artists": [],
      "albums": [],
      "error": "Spotify did not answer."
    }
  end

  artists = ((data["artists"] ?? {})["items"] ?? []).map(fn(a) { {
    "id": a["id"],
    "name": a["name"]
  } })
  albums = ((data["albums"] ?? {})["items"] ?? []).map(fn(a) { needle_album_stub(a) })
  {
    "artists": artists,
    "albums": albums,
    "error": ""
  }
end

def needle_album(id)
  return needle_sample_album(id.replace("sample:", "").to_i()) if id.starts_with?("sample:")

  data = needle_api("/albums/" + id)
  return nil if data.nil?

  items = ((data["tracks"] ?? {})["items"] ?? [])
  tracks = items.map(fn(t) {
    {
      "n": t["track_number"],
      "title": t["name"],
      "seconds": t["duration_ms"] / 1000,
      "uri": t["uri"] ?? ""
    }
  })
  stub = needle_album_stub(data)
  stub["tracks"] = tracks
  stub
end

def needle_artist(id)
  return needle_sample_artist(id) if id.starts_with?("artist:")

  info = needle_api("/artists/" + id)
  return nil if info.nil?

  listing = needle_api("/artists/" + id + "/albums?include_groups=album,single&limit=10")
  # An app registered after Spotify's 2025 restrictions is refused this
  # one — 403, whatever the market — along with related-artists and
  # recommendations. An older app still gets it, so the call stays and the
  # page simply has no Popular section when it comes back empty.
  tops = needle_api("/artists/" + id + "/top-tracks?market=" + needle_market())
  albums = listing.nil? ? [] : (listing["items"] ?? []).map(fn(a) { needle_album_stub(a) })
  top = tops.nil? ? [] : (tops["tracks"] ?? []).slice(0, 5).map(fn(t) {
    album = t["album"] ?? {}
    {
      "n": 0,
      "title": t["name"],
      "seconds": t["duration_ms"] / 1000,
      "uri": t["uri"] ?? "",
      "album": album["name"] ?? "",
      "album_id": album["id"] ?? ""
    }
  })
  {
    "id": id,
    "name": info["name"],
    "genres": (info["genres"] ?? []).slice(0, 3).join(" · "),
    "albums": albums,
    "top": top
  }
end

# --------------------------------------------------------- playing a thing

# Nothing here makes a sound, for want of bytes rather than for want of a
# widget (03 §7 has one). With a linked account it asks Spotify Connect to
# start on a device that is already open, and the answer is what the bar
# reports. "local" means the bar is keeping time by itself.
def needle_remote(body)
  token = needle_user_token()
  return "local" if token.nil? || token == ""

  head = {
    "Authorization": "Bearer " + token,
    "Content-Type": "application/json"
  }
  opts = {"headers": head, "timeout": 10}
  url = NEEDLE_API + "/me/player/play"
  resp = HTTP.request("PUT", url, opts, json_stringify(body)) rescue nil
  return "offline" if resp.nil?
  return "remote" if resp["status"] == 204 || resp["status"] == 202
  return "no device" if resp["status"] == 404
  return "needs Premium" if resp["status"] == 403
  return "the link expired" if resp["status"] == 401

  "refused"
end

def needle_note(mode)
  return "the Spotify link expired — connect again" if mode == "the link expired"
  return "playing on your Spotify device" if mode == "remote"
  return "no Spotify device is open" if mode == "no device"
  return "Spotify Connect needs Premium" if mode == "needs Premium"
  return "Spotify Connect refused the call" if mode == "refused"
  return "Spotify could not be reached" if mode == "offline"

  "keeping time only — this catalogue carries no sound"
end

# The pages the login routes answer with. No attribute in this markup is
# quoted, because a `\"` inside a Soli string is a lex error and HTML5
# needs no quotes around an unspaced value.
def needle_page(title, body)
  css = "body{font-family:system-ui,sans-serif;background:#0f1113;color:#e8eaec;margin:0;display:flex;min-height:100vh;align-items:center;justify-content:center}main{max-width:36rem;padding:2rem}h1{font-size:1.35rem;margin:0 0 .75rem}p{line-height:1.6;color:#a6adb4;margin:.6rem 0}code{display:block;background:#191c1f;color:#79e3b4;padding:.9rem 1rem;border-radius:.5rem;margin:1.1rem 0;word-break:break-all;font-size:.82rem}b{color:#e8eaec;font-weight:600}"
  head = "<!doctype html><meta charset=utf-8><title>" + title + "</title>"
  head + "<style>" + css + "</style><main><h1>" + title + "</h1>" + body + "</main>"
end

def needle_reply(title, body)
  {
    "status": 200,
    "headers": {"Content-Type": "text/html; charset=utf-8"},
    "body": needle_page(title, "<p>" + body + "</p>")
  }
end

def needle_reply_token(refresh)
  told = "<p>Needle is linked to your account. One step is left, and it is manual on purpose: this app has nowhere durable to keep a secret, so the token goes where the other two live.</p>"
  told = told + "<p>Put this line in <b>~/.local/bin/needle</b>, just before <b>exec</b>, and start the player again.</p>"
  told = told + "<code>SPOTIFY_REFRESH_TOKEN=" + refresh + "</code>"
  told = told + "<p>It is good for months, and it is what the player trades for the hour-long tokens it needs. You can close this tab.</p>"
  {
    "status": 200,
    "headers": {"Content-Type": "text/html; charset=utf-8"},
    "body": needle_page("Linked", told)
  }
end

# ------------------------------------------------------------------ state

def needle_defaults(state)
  state["typed"] = state["typed"] ?? ""
  state["query"] = state["query"] ?? ""
  state["artists"] = state["artists"] ?? []
  state["albums"] = state["albums"] ?? []
  state["pane"] = state["pane"] ?? "welcome"
  state["album"] = state["album"] ?? {}
  state["artist"] = state["artist"] ?? {}
  state["now"] = state["now"] ?? {}
  state["playing"] = state["playing"] ?? false
  state["position"] = state["position"] ?? 0
  state["mode"] = state["mode"] ?? "local"
  state["error"] = state["error"] ?? ""
  state["notice"] = state["notice"] ?? ""
  state["viewport"] = state["viewport"] ?? {
    "width": 1200,
    "height": 800
  }
  state
end

def needle_set(state, key, value)
  state[key] = value
  state
end

def needle_layout(state)
  w = state["viewport"]["width"] ?? 1200
  {
    "single": w < 720,
    "tight": w < 1000,
    "width": w
  }
end

def needle_find(state)
  q = state["typed"]
  return needle_set(state, "error", "") if q == ""

  found = needle_search(q)
  state["query"] = q
  state["artists"] = found["artists"]
  state["albums"] = found["albums"]
  state["error"] = found["error"] ?? ""
  state["notice"] = ""
  empty = found["artists"].length() + found["albums"].length() == 0
  state["pane"] = empty ? "welcome" : "results"
  state
end

def needle_open_album(state, id)
  album = needle_album(id)
  return needle_set(state, "error", "That record would not load.") if album.nil?

  state["album"] = album
  state["pane"] = "album"
  state["error"] = ""
  state
end

def needle_open_artist(state, id)
  artist = needle_artist(id)
  return needle_set(state, "error", "That artist would not load.") if artist.nil?

  state["artist"] = artist
  state["pane"] = "artist"
  state["error"] = ""
  state
end

# What the bar shows, and what Connect was asked for. A track carries its
# own uri when it came from Spotify; a sample track has none, so the bar
# keeps its own time.
def needle_play(state, title, artist, album, seconds, uri, context, offset)
  state["now"] = {
    "title": title,
    "artist": artist,
    "album": album,
    "seconds": seconds,
    "uri": uri
  }
  state["playing"] = true
  state["position"] = 0
  body = context == "" ? {"uris": [uri]} : {
    "context_uri": context,
    "offset": {"position": offset}
  }
  state["mode"] = uri == "" && context == "" ? "local" : needle_remote(body)
  state
end

def needle_play_track(state, album, index)
  tracks = album["tracks"] ?? []
  return state if tracks.length() == 0

  i = index < tracks.length() ? index : 0
  track = tracks[i]
  needle_play(
    state,
    track["title"],
    album["artist"],
    album["title"],
    track["seconds"],
    track["uri"] ?? "",
    album["uri"] ?? "",
    i
  )
end

def needle_step(state, delta)
  album = state["album"]
  tracks = album["tracks"] ?? []
  return state if tracks.length() == 0

  now = state["now"]["title"] ?? ""
  at = 0
  found = range(0, tracks.length()).filter(fn(i) { tracks[i]["title"] == now })
  at = found[0] if found.length() > 0
  next_at = (at + delta + tracks.length()) % tracks.length()
  needle_play_track(state, album, next_at)
end

def needle_seek(state, params)
  return state if params["kind"] != "click"

  duration = state["now"]["seconds"] ?? 0
  return state if duration == 0

  width = needle_layout(state)["tight"] ? 200 : 360
  x = params["payload"][0]
  state["position"] = int(x * duration / width)
  state
end

# The consent screen is a web page and the client cannot open one — no
# capability in 01 §2.1 opens a URL. The server can, though: it runs on
# the same machine as the person clicking, which is the whole point of a
# desktop build.
def needle_open_login(state)
  System.run(["xdg-open", needle_login_url()]) rescue nil
  state["notice"] = "Approve Needle in the browser that just opened, then follow the one line it gives you."
  state["error"] = ""
  state
end

def music(event_data)
  event = event_data["event"]
  params = event_data["params"]
  props = params["props"] ?? {}
  state = needle_defaults(event_data["state"] ?? {})
  match event {
    "connect" => needle_set(state, "viewport", params["viewport"] ?? state["viewport"]),
    "viewport" => needle_set(state, "viewport", params["viewport"]),
    "typed" => needle_set(state, "typed", params["payload"]),
    "find" => needle_find(state),
    "suggest" => needle_find(needle_set(state, "typed", props["q"])),
    "open_album" => needle_open_album(state, props["id"]),
    "open_artist" => needle_open_artist(state, props["id"]),
    "results" => needle_set(state, "pane", state["query"] == "" ? "welcome" : "results"),
    "play_album" => needle_play_track(state, state["album"], 0),
    "play_track" => needle_play_track(state, state["album"], props["i"]),
    "play_top" => needle_play(
      state,
      props["title"],
      state["artist"]["name"],
      props["album"],
      props["seconds"],
      props["uri"],
      "",
      0
    ),
    "login" => needle_open_login(state),
    "toggle" => needle_set(state, "playing", !state["playing"]),
    "next" => needle_step(state, 1),
    "prev" => needle_step(state, -1),
    "seek" => needle_seek(state, params),
    _ => state,
  }
end

# ------------------------------------------------------------ primitives

# A box that changes colour under the pointer: the client swaps the style
# itself, so hovering a rail of thirty rows costs no round trip.
def needle_hover(key, base, hover, children, on_click, props)
  n = {
    "k": "box",
    "key": key,
    "s": base,
    "c": children,
    "on": {"pointer_enter": {"local": "self.style = @hover", "styles": {"hover": hover}}, "pointer_leave": {
      "local": "self.style = @base",
      "styles": {"base": base}
    }}
  }
  n["on"]["click"] = on_click unless on_click.nil?
  n["p"] = props unless props.nil?
  n
end

def needle_label(content, size, weight, fg)
  text(
    content,
    {
      "size": size,
      "weight": weight,
      "fg": fg
    }
  )
end

def needle_caption(content)
  text(
    content.upcase(),
    {
      "size": 0,
      "weight": "bold",
      "fg": "text.muted"
    }
  )
end

# The one button that starts something: accent, and it warms under the
# pointer over `motion.fast`.
def needle_play_button(edge, on_click, props)
  base = {
    "display": "row",
    "align": "center",
    "justify": "center",
    "width": edge,
    "height": edge,
    "radius": 4,
    "bg": "accent.base",
    "cursor": "pointer",
    "shrink": 0,
    "transition": "fast",
    "shadow": 1
  }
  hover = base.merge({"bg": "accent.hover"})
  glyph = canvas(edge / 2, edge / 2, [ [
    0,
    "accent.on",
    edge / 7.0,
    edge / 6.0,
    edge / 8.0,
    edge / 6.0,
    edge * 3 / 8.0,
    edge / 6.0,
    edge / 8.0,
    edge / 2.4,
    edge / 4.0,
    edge / 6.0,
    edge * 3 / 8.0
  ]])
  needle_hover("play:" + on_click + str(edge), base, hover, [glyph], on_click, props)
end

def needle_icon_button(key, paths, size, on_click, props)
  base = {
    "display": "row",
    "align": "center",
    "justify": "center",
    "width": size,
    "height": size,
    "radius": 4,
    "bg": "none",
    "cursor": "pointer",
    "shrink": 0,
    "transition": "fast"
  }
  hover = base.merge({"bg": "surface.sunken"})
  needle_hover(key, base, hover, [canvas(size / 2, size / 2, paths)], on_click, props)
end

def needle_go_button
  base = {
    "display": "row",
    "align": "center",
    "justify": "center",
    "width": 28,
    "height": 28,
    "radius": 4,
    "bg": "accent.base",
    "cursor": "pointer",
    "shrink": 0,
    "transition": "fast"
  }
  hover = base.merge({"bg": "accent.hover"})
  arrow = canvas(16, 16, [ [
    0,
    "accent.on",
    2.0,
    4,
    8,
    12,
    8
  ], [0, "accent.on", 2.0, 8, 4, 12, 8, 8, 12]])
  needle_hover("go", base, hover, [arrow], "find", nil)
end

# ---------------------------------------------------------------- top bar

def needle_field(state, layout)
  entry = input(state["typed"], "typed")
  entry["s"] = {
    "grow": 1,
    "min_width": 0,
    "border": 0,
    "bg": "none",
    "fg": "text.default",
    "size": 2,
    "pad": [1, 0, 1, 0]
  }
  entry["on"]["submit"] = "find"
  row(
    {
      "gap": 2,
      "align": "center",
      "grow": 1,
      "max_width": layout["single"] ? 420 : 520,
      "bg": "surface.raised",
      "radius": 4,
      "pad": [1, 2, 1, 3],
      "border": 1,
      "border_color": "border.subtle"
    },
    [needle_glass("text.muted"), entry, needle_go_button()]
  )
end

# Linking an account is the one thing in this app that leaves the window,
# so it says which of the two states it is in and nothing more.
def needle_account_chip(state)
  if needle_linked()
    return {
      "k": "box",
      "s": {
        "pad": [0, 2, 0, 2],
        "radius": 4,
        "bg": "success.subtle",
        "shrink": 0
      },
      "c": [needle_label("account linked", 0, "medium", "success.base")]
    }
  end

  base = {
    "display": "row",
    "align": "center",
    "pad": [0, 2, 0, 2],
    "radius": 4,
    "bg": "surface.sunken",
    "cursor": "pointer",
    "transition": "fast",
    "shrink": 0
  }
  hover = base.merge({"bg": "accent.base"})
  needle_hover("link", base, hover, [needle_label("connect account", 0, "medium", "text.muted")], "login", nil)
end

def needle_topbar(state, layout)
  brand = row(
    {
      "gap": 2,
      "align": "center",
      "shrink": 0
    },
    [needle_mark(), needle_label("Needle", 3, "bold", "text.default")]
  )
  source = needle_configured() ? "Spotify" : "sample catalogue"
  badge = {
    "k": "box",
    "s": {
      "pad": [0, 2, 0, 2],
      "radius": 4,
      "bg": "surface.sunken",
      "shrink": 0
    },
    "c": [needle_label(source, 0, "medium", "text.muted")]
  }
  chips = needle_configured() ? [badge, needle_account_chip(state)] : [badge]
  head = layout["single"] ? [brand, spacer()] : [brand].concat(chips).concat([spacer()])
  row(
    {
      "gap": 3,
      "align": "center",
      "pad": [2, 3, 2, 3],
      "shrink": 0
    },
    head.concat([needle_field(state, layout), theme_toggle()])
  )
end

# ------------------------------------------------------------- the rail

def needle_rail_row(key, name, sub, sleeve, on_click, props, selected)
  base = {
    "display": "row",
    "gap": 3,
    "align": "center",
    "pad": [2, 2, 2, 2],
    "radius": 2,
    "cursor": "pointer",
    "transition": "fast",
    "bg": selected ? "surface.sunken" : "none"
  }
  hover = base.merge({"bg": "surface.sunken"})
  lines = sub == "" ? [needle_label(name, 2, "medium", "text.default")] : [
    needle_label(name, 2, "medium", "text.default"),
    text(
      sub,
      {
        "size": 0,
        "fg": "text.muted",
        "clamp": 1
      }
    )
  ]
  needle_hover(key, base, hover, [sleeve, column(
    {
      "gap": 0,
      "grow": 1,
      "min_width": 0
    },
    lines
  )], on_click, props)
end

def needle_rail(state, layout)
  open_album = (state["album"]["id"] ?? "")
  open_artist = (state["artist"]["id"] ?? "")
  artists = state["artists"].map(fn(a) {
    needle_rail_row(
      "ar:" + a["id"],
      a["name"],
      "",
      needle_sleeve(a["name"], 40, 4),
      "open_artist",
      {"id": a["id"]},
      a["id"] == open_artist
    )
  })
  albums = state["albums"].map(fn(a) {
    sub = a["year"] == "" ? a["artist"] : a["artist"] + " · " + a["year"]
    needle_rail_row(
      "al:" + a["id"],
      a["title"],
      sub,
      needle_sleeve(a["title"] + a["artist"], 40, 1),
      "open_album",
      {"id": a["id"]},
      a["id"] == open_album
    )
  })
  groups = []
  if artists.length() > 0
    groups = groups.concat([column({"gap": 1}, [{
      "k": "box",
      "s": {"pad": [1, 2, 1, 2]},
      "c": [needle_caption("Artists")]
    }].concat(artists))])
  end
  if albums.length() > 0
    groups = groups.concat([column({"gap": 1}, [{
      "k": "box",
      "s": {"pad": [1, 2, 1, 2]},
      "c": [needle_caption("Records")]
    }].concat(albums))])
  end
  column(
    {
      "width": layout["tight"] ? 240 : 300,
      "shrink": 0,
      "gap": 0,
      "min_height": 0,
      "bg": "surface.raised",
      "radius": 3
    },
    [scroll(
      {
        "grow": 1,
        "min_height": 0,
        "gap": 3,
        "pad": [2, 2, 3, 2]
      },
      groups
    )]
  )
end

# ------------------------------------------------------------ the record

# The band is the one node that keeps its key across every record, so a
# style change is all the client sees and 03 §5 fades its colour from the
# last record's hue to this one over `motion.slow`.
def needle_band(name, kind, title, meta, extra, sleeve_edge, on_play, props, layout)
  sleeve = needle_sleeve(name, sleeve_edge, kind == "Artist" ? 4 : 2)
  sleeve["key"] = "band:sleeve"
  lines = [
    needle_label(kind.upcase(), 0, "bold", "#ffffffb0"),
    text(
      title,
      {
        "size": layout["tight"] ? 5 : 6,
        "weight": "bold",
        "fg": "#ffffff",
        "clamp": 2
      }
    ),
    text(
      meta,
      {"size": 2, "fg": "#ffffffcc"}
    )
  ]
  lines = lines.concat([extra]) unless extra.nil?
  {
    "k": "box",
    "key": "band",
    "s": {
      "display": "row",
      "gap": 4,
      "align": "end",
      "wrap": "wrap",
      "pad": [5, 4, 4, 4],
      "bg": needle_tint(name),
      "radius": 3,
      "transition": "slow"
    },
    "c": [
      sleeve,
      column(
        {
          "gap": 2,
          "grow": 1,
          "min_width": 200
        },
        lines.concat([row(
          {
            "gap": 3,
            "align": "center",
            "pad": [2, 0, 0, 0]
          },
          [needle_play_button(48, on_play, props)]
        )])
      )
    ]
  }
end

def needle_track_row(state, key, n, title, sub, seconds, on_click, props)
  now = state["now"]["title"] ?? ""
  current = now != "" && now == title
  base = {
    "display": "row",
    "gap": 3,
    "align": "center",
    "pad": current ? [
      2,
      3,
      2,
      2
    ] : [2, 3, 2, 3],
    "border": current ? [
      0,
      0,
      0,
      2
    ] : [0, 0, 0, 0],
    "border_color": "accent.base",
    "radius": 2,
    "cursor": "pointer",
    "transition": "fast",
    "bg": current ? "surface.sunken" : "none"
  }
  hover = base.merge({"bg": "surface.sunken"})
  index = {
    "k": "box",
    "s": {
      "width": 26,
      "shrink": 0,
      "display": "row",
      "justify": "center"
    },
    "c": [current && state["playing"] ? needle_sleeve(title, 14, 4) : text(
      n,
      {"size": 1, "fg": "text.muted"}
    )]
  }
  lines = sub == "" ? [text(
    title,
    {
      "size": 2,
      "weight": "medium",
      "fg": current ? "accent.base" : "text.default",
      "clamp": 1
    }
  )] : [
    text(
      title,
      {
        "size": 2,
        "weight": "medium",
        "fg": current ? "accent.base" : "text.default",
        "clamp": 1
      }
    ),
    text(
      sub,
      {
        "size": 0,
        "fg": "text.muted",
        "clamp": 1
      }
    )
  ]
  needle_hover(key, base, hover, [
    index,
    column(
      {
        "gap": 0,
        "grow": 1,
        "min_width": 0
      },
      lines
    ),
    text(
      needle_clock(seconds),
      {"size": 1, "fg": "text.muted"}
    )
  ], on_click, props)
end

def needle_album_pane(state, layout)
  album = state["album"]
  tracks = album["tracks"] ?? []
  total = int(tracks.map(fn(t) { t["seconds"] }).sum())
  name = album["title"] + album["artist"]
  year = album["year"] == "" ? "" : " · " + album["year"]
  count = str(tracks.length()) + " tracks"
  length = str(total / 60) + " min"
  meta = [album["artist"] + year, count, length].join(" · ")
  artist_link = {
    "k": "box",
    "s": {"pad": [1, 0, 1, 0], "cursor": "pointer"},
    "p": {"id": album["artist_id"]},
    "on": {"click": "open_artist"},
    "c": [needle_label("Go to " + album["artist"] + " →", 1, "medium", "#ffffffcc")]
  }
  rows = range(0, tracks.length()).map(fn(i) {
    t = tracks[i]
    needle_track_row(state, "tr:" + str(i), str(t["n"]), t["title"], "", t["seconds"], "play_track", {"i": i})
  })
  column(
    {"gap": 4, "pad": [
      0,
      0,
      4,
      0
    ]},
    [
      needle_band(
        name,
        "Record",
        album["title"],
        meta,
        artist_link,
        layout["tight"] ? 120 : 168,
        "play_album",
        nil,
        layout
      ),
      column(
        {"gap": 1, "pad": [
          0,
          2,
          0,
          2
        ]},
        rows
      )
    ]
  )
end

def needle_artist_pane(state, layout)
  artist = state["artist"]
  albums = artist["albums"] ?? []
  top = artist["top"] ?? []
  meta = artist["genres"] == "" ? str(albums.length()) + " records" : artist["genres"]
  first = albums.length() > 0 ? albums[0] : nil
  props = first.nil? ? nil : {"id": first["id"]}
  event = first.nil? ? "results" : "open_album"
  band = needle_band(
    artist["name"],
    "Artist",
    artist["name"],
    meta,
    nil,
    layout["tight"] ? 120 : 168,
    event,
    props,
    layout
  )
  tops = range(0, top.length()).map(fn(i) {
    t = top[i]
    needle_track_row(state, "tp:" + str(i), str(i + 1), t["title"], t["album"], t["seconds"], "play_top", {
      "title": t["title"],
      "album": t["album"],
      "seconds": t["seconds"],
      "uri": t["uri"]
    })
  })
  cards = albums.map(fn(a) { needle_record_tile(a) })
  sections = [band]
  if tops.length() > 0
    sections = sections.concat([column(
      {"gap": 2, "pad": [
        0,
        3,
        0,
        3
      ]},
      [needle_caption("Popular")].concat(tops)
    )])
  end
  if cards.length() > 0
    sections = sections.concat([column(
      {"gap": 2, "pad": [
        0,
        3,
        0,
        3
      ]},
      [needle_caption("Records"), row(
        {"gap": 2, "wrap": "wrap"},
        cards
      )]
    )])
  end
  column(
    {"gap": 4, "pad": [
      0,
      0,
      4,
      0
    ]},
    sections
  )
end

# One tile: the sleeve, the name under it. A record's is square and opens
# the record; an artist's is round and opens the artist.
def needle_tile(key, name, sub, art, round, on_click, props)
  base = {
    "display": "column",
    "gap": 2,
    "width": 132,
    "pad": [2, 2, 3, 2],
    "radius": 2,
    "bg": "none",
    "cursor": "pointer",
    "transition": "fast",
    "shrink": 0
  }
  hover = base.merge({"bg": "surface.sunken"})
  lines = [
    needle_sleeve(art, 116, round ? 4 : 2),
    text(
      name,
      {
        "size": 1,
        "weight": "medium",
        "fg": "text.default",
        "clamp": 2
      }
    )
  ]
  if sub != ""
    lines = lines.concat([text(
      sub,
      {"size": 0, "fg": "text.muted"}
    )])
  end
  needle_hover(key, base, hover, lines, on_click, props)
end

def needle_record_tile(a)
  needle_tile("ca:" + a["id"], a["title"], a["year"], a["title"] + a["artist"], false, "open_album", {"id": a["id"]})
end

def needle_artist_tile(a)
  needle_tile("at:" + a["id"], a["name"], "", a["name"], true, "open_artist", {"id": a["id"]})
end

def needle_suggestion(name)
  base = {
    "display": "column",
    "gap": 2,
    "width": 128,
    "pad": [2, 2, 3, 2],
    "radius": 2,
    "bg": "none",
    "cursor": "pointer",
    "transition": "fast",
    "shrink": 0
  }
  hover = base.merge({"bg": "surface.sunken"})
  needle_hover(
    "sg:" + name,
    base,
    hover,
    [needle_sleeve(name, 112, 2), needle_label(name, 1, "medium", "text.default")],
    "suggest",
    {"q": name}
  )
end

def needle_welcome(state)
  names = needle_configured() ? [
    "Radiohead",
    "Nina Simone",
    "Aphex Twin",
    "Fela Kuti"
  ] : NEEDLE_ARTISTS.slice(0, 4)
  column(
    {
      "gap": 6,
      "pad": [8, 4, 8, 4],
      "align": "center",
      "justify": "center",
      "grow": 1
    },
    [
      column(
        {
          "gap": 2,
          "width": 560,
          "align": "center"
        },
        [
          text(
            "Find something to play",
            {
              "size": 6,
              "weight": "bold",
              "fg": "text.default",
              "text_align": "center"
            }
          ),
          text(
            "Type a name and press Enter. Every sleeve is drawn from the record's own name — no picture is fetched — and the ground behind it comes from your own theme.",
            {
              "size": 2,
              "fg": "text.muted",
              "text_align": "center"
            }
          )
        ]
      ),
      column(
        {"gap": 3, "align": "center"},
        [needle_caption("Try one"), row(
          {"gap": 2, "wrap": "wrap"},
          names.map(fn(n) { needle_suggestion(n) })
        )]
      )
    ]
  )
end

def needle_results_pane(state)
  count = state["artists"].length() + state["albums"].length()
  heading = str(count) + " for “" + state["query"] + "”"
  sections = [text(
    heading,
    {
      "size": 5,
      "weight": "bold",
      "fg": "text.default"
    }
  )]
  artists = state["artists"].map(fn(a) { needle_artist_tile(a) })
  records = state["albums"].map(fn(a) { needle_record_tile(a) })
  if artists.length() > 0
    sections = sections.concat([column({"gap": 2}, [needle_caption("Artists"), row(
      {"gap": 2, "wrap": "wrap"},
      artists
    )])])
  end
  if records.length() > 0
    sections = sections.concat([column({"gap": 2}, [needle_caption("Records"), row(
      {"gap": 2, "wrap": "wrap"},
      records
    )])])
  end
  column(
    {"gap": 5, "pad": [
      5,
      3,
      4,
      3
    ]},
    sections
  )
end

# On a window too narrow to hold both, the detail owns the width and this
# is the way back to what the search found.
def needle_back
  base = {
    "display": "row",
    "gap": 2,
    "align": "center",
    "pad": [1, 3, 1, 2],
    "margin": [2, 3, 0, 3],
    "radius": 4,
    "self": "start",
    "bg": "surface.sunken",
    "cursor": "pointer",
    "transition": "fast"
  }
  hover = base.merge({"bg": "border.subtle"})
  needle_hover("back", base, hover, [needle_label("‹  Results", 1, "medium", "text.default")], "results", nil)
end

def needle_detail(state, layout)
  pane = state["pane"]
  body = needle_welcome(state)
  if pane == "album"
    body = needle_album_pane(state, layout)
  elsif pane == "artist"
    body = needle_artist_pane(state, layout)
  elsif pane == "results"
    body = needle_results_pane(state)
  end
  head = layout["single"] && pane != "welcome" && pane != "results" ? [needle_back()] : []
  head = head.concat(state["error"] == "" ? [] : [{
    "k": "box",
    "s": {
      "pad": [2, 3, 2, 3],
      "margin": [2, 3, 0, 3],
      "radius": 2,
      "bg": "danger.subtle"
    },
    "c": [needle_label(state["error"], 1, "medium", "danger.base")]
  }])
  head = head.concat(state["notice"] == "" ? [] : [{
    "k": "box",
    "s": {
      "pad": [2, 3, 2, 3],
      "margin": [2, 3, 0, 3],
      "radius": 2,
      "bg": "info.subtle"
    },
    "c": [needle_label(state["notice"], 1, "medium", "info.base")]
  }])
  scroll(
    {
      "grow": 1,
      "min_height": 0,
      "min_width": 0,
      "gap": 0,
      "bg": "surface.raised",
      "radius": 3
    },
    [column(
      {
        "gap": 0,
        "grow": 1,
        "min_height": 0
      },
      head.concat([body])
    )]
  )
end

# ---------------------------------------------------------- the bottom bar

# The disc turns for exactly as long as something is playing: `spin` is
# the one thing in 03 §5 that moves without a state change, and stopping
# it is a style change like any other.
def needle_disc(state, size)
  now = state["now"]
  name = (now["title"] ?? "") + (now["artist"] ?? "")
  disc = needle_sleeve(name == "" ? "Needle" : name, size, 4)
  disc["key"] = "disc"
  disc["s"]["animation"] = state["playing"] ? "spin" : "none"
  disc
end

def needle_transport(state, layout)
  glyph = state["playing"] ? [ [
    1,
    "accent.on",
    5,
    3,
    4,
    14,
    1
  ], [1, "accent.on", 13, 3, 4, 14, 1]] : [ [
    0,
    "accent.on",
    3.0,
    7,
    4,
    7,
    18,
    7,
    4,
    17,
    11,
    7,
    18
  ]]
  toggle_base = {
    "display": "row",
    "align": "center",
    "justify": "center",
    "width": 40,
    "height": 40,
    "radius": 4,
    "bg": "accent.base",
    "cursor": "pointer",
    "shrink": 0,
    "transition": "fast"
  }
  toggle = needle_hover(
    "bar:toggle",
    toggle_base,
    toggle_base.merge({"bg": "accent.hover"}),
    [canvas(22, 22, glyph)],
    "toggle",
    nil
  )
  prev = needle_icon_button("bar:prev", [
    [0, "text.muted", 2.0, 12, 3, 5, 9, 12, 15],
    [0, "text.muted", 2.0, 17, 3, 10, 9, 17, 15]
  ], 36, "prev", nil)
  fwd = needle_icon_button("bar:next", [
    [0, "text.muted", 2.0, 6, 3, 13, 9, 6, 15],
    [0, "text.muted", 2.0, 11, 3, 18, 9, 11, 15]
  ], 36, "next", nil)
  row(
    {
      "gap": 1,
      "align": "center",
      "shrink": 0
    },
    layout["single"] ? [toggle] : [
      prev,
      toggle,
      fwd
    ]
  )
end

def needle_seekbar(state, layout)
  now = state["now"]
  duration = now["seconds"] ?? 0
  position = state["position"] < duration ? state["position"] : duration
  width = layout["tight"] ? 200 : 360
  filled = duration == 0 ? 0 : int(width * position / duration)
  bar = {
    "k": "box",
    "key": "bar:seek",
    "s": {
      "display": "row",
      "align": "center",
      "width": width,
      "height": 6,
      "radius": 4,
      "bg": "surface.sunken",
      "cursor": "pointer",
      "shrink": 0
    },
    "on": {"click": "seek"},
    "c": [{"k": "box", "s": {
      "width": filled,
      "height": 6,
      "radius": 4,
      "bg": "accent.base",
      "transition": "fast"
    }}]
  }
  row(
    {
      "gap": 2,
      "align": "center",
      "shrink": 0
    },
    [
      text(
        needle_clock(position),
        {"size": 0, "fg": "text.muted"}
      ),
      bar,
      text(
        needle_clock(duration),
        {"size": 0, "fg": "text.muted"}
      )
    ]
  )
end

def needle_bar(state, layout)
  now = state["now"]
  title = now["title"] ?? "Nothing playing"
  note = needle_note(state["mode"])
  who = now["artist"] ?? ""
  sub = who == "" ? note : who + " · " + note
  left = row(
    {
      "gap": 3,
      "align": "center",
      "grow": 1,
      "width": 0,
      "min_width": layout["single"] ? 100 : 200
    },
    [
      needle_disc(state, layout["single"] ? 36 : 44),
      column(
        {"gap": 0, "min_width": 0},
        [
          text(
            title,
            {
              "size": 2,
              "weight": "medium",
              "fg": "text.default",
              "clamp": 1
            }
          ),
          text(
            sub,
            {
              "size": 0,
              "fg": "text.muted",
              "clamp": 1
            }
          )
        ]
      )
    ]
  )
  middle = column(
    {
      "gap": 1,
      "align": "center",
      "shrink": 0
    },
    [needle_transport(state, layout), needle_seekbar(state, layout)]
  )
  right = {"k": "box", "s": {"grow": 1, "width": 0}}
  row(
    {
      "gap": 4,
      "align": "center",
      "pad": [2, 3, 2, 3],
      "bg": "surface.raised",
      "radius": 3,
      "shrink": 0
    },
    [left, middle, right]
  )
end

# ------------------------------------------------------------------ shell

# The rail earns its place once a search has run; before that the welcome
# has the whole width, and on a narrow window the two take turns.
def needle_panes(state, layout)
  return [needle_detail(state, layout)] if state["pane"] == "welcome"
  return [needle_rail(state, layout)] if layout["single"] && state["pane"] == "results"
  return [needle_detail(state, layout)] if layout["single"];

  [needle_rail(state, layout), needle_detail(state, layout)]
end

def music_view(raw_state)
  state = needle_defaults(raw_state)
  layout = needle_layout(state)
  panes = needle_panes(state, layout)
  column(
    {
      "gap": 2,
      "bg": "surface.base",
      "pad": [0, 2, 2, 2],
      "grow": 1,
      "min_height": 0
    },
    [
      needle_topbar(state, layout),
      row(
        {
          "gap": 2,
          "grow": 1,
          "min_height": 0,
          "align": "stretch"
        },
        panes
      ),
      needle_bar(state, layout)
    ]
  )
end
# `min_height: 0`: the row holds scrollers, and without it its
# automatic minimum (04 §4.3) would be the whole page's height.
