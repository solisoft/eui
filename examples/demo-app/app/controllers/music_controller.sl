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

# A record's face: Spotify's picture when it was fetched, the drawn
# sleeve when there is none — the sample catalogue, or a fetch that
# failed. Same shape, same corner, so a rail of them stays even.
def needle_art(picture, name, edge, radius)
  return needle_sleeve(name, edge, radius) if picture.nil? || picture == ""

  {
    "k": "image",
    "p": {"src": picture},
    "s": {
      "width": edge,
      "height": edge,
      "radius": radius,
      "shrink": 0
    }
  }
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

# Spotify hands out picture URLs, largest first, and the client fetches
# assets only from its own origin by hash (01 §2.2) — a third party never
# hears from it. So the server fetches instead, once, into `public`, and
# the node names the file like any other picture. `HTTP.download` is what
# makes this possible at all: every other HTTP builtin decodes its body
# as UTF-8, which a JPEG is not.
def needle_pick_art(images, small)
  return "" if images.nil? || images.length() == 0

  # [640, 300, 64] in that order: the last is the rail's thumbnail, the
  # middle one is what a band or a tile shows.
  wanted = small ? images.length() - 1 : (images.length() > 1 ? 1 : 0)
  images[wanted]["url"] ?? ""
end

def needle_cache_art(id, url, tag)
  return "" if url == "" || id == ""

  # The client decodes PNG and nothing else (`eui-client/src/assets.rs`
  # looks for the PNG magic and stores anything else undecoded), and
  # Spotify serves JPEG. So the picture is re-encoded once, here, on its
  # way into the cache, and the JPEG is dropped.
  png = "public/covers/" + tag + "-" + id + ".png"
  return png if File.exists(png)

  raw = "public/covers/" + tag + "-" + id + ".jpg"
  written = HTTP.download(url, raw) rescue 0
  return "" if written == 0

  needle_square(raw, png, tag == "s" ? 160 : 320) rescue nil
  File.delete(raw) rescue nil
  File.exists(png) ? png : ""
end

# A rail of faces reads as a rail only if they are all the same shape, and
# Spotify's pictures are not all square — an artist's portrait comes back
# 270×320 as often as not. So the middle square is taken and scaled to the
# one size that node will draw, which also keeps the cache small: a 64 px
# thumbnail is a few kilobytes, not forty.
def needle_square(source, target, edge)
  picture = Image.new(source)
  wide = picture.width()
  tall = picture.height()
  side = wide < tall ? wide : tall
  picture.crop((wide - side) / 2, (tall - side) / 2, side, side).resize(edge, edge).format("png").to_file(target)
end

def needle_year(date)
  date.nil? ? "" : date.substring(0, 4)
end

def needle_album_stub(a)
  artists = a["artists"] ?? []
  name = artists.length() > 0 ? artists[0]["name"] : "Unknown"
  aid = artists.length() > 0 ? artists[0]["id"] : ""
  images = a["images"] ?? []
  {
    "id": a["id"],
    "title": a["name"],
    "artist": name,
    "artist_id": aid,
    "year": needle_year(a["release_date"]),
    "uri": a["uri"] ?? "",
    "art": needle_cache_art(a["id"], needle_pick_art(images, true), "s"),
    "art_big": needle_cache_art(a["id"], needle_pick_art(images, false), "m")
  }
end

# ----------------------------------------------- records on this machine

# What the client can decode: `eui-audio` reads WAV, FLAC, MP3 and Ogg
# Vorbis, and nothing else — no C library under it, which is the whole
# argument for the short list.
def needle_is_sound(path)
  low = path.downcase()
  low.ends_with?(".wav") || low.ends_with?(".mp3") || low.ends_with?(".flac") || low.ends_with?(".ogg")
end

# `File.glob` may answer with the resolved absolute path; a node's `src`
# is a path inside the application, so keep it from `public/` on.
def needle_relative(path)
  cut = path.index_of("public/")
  cut < 0 ? path : path.substring(cut, path.length())
end

def needle_track_name(path)
  name = path.split("/").last()
  dot = name.index_of(".")
  dot < 0 ? name : name.substring(0, dot)
end

def needle_local_files
  found = File.glob("public/music/*") rescue []
  found.filter(fn(f) { needle_is_sound(f) })
end

# A record made of the files sitting in `public/music`. Their length is
# not read from the files — nothing here parses a container — it arrives
# from the client's first `time_update`, which carries the duration the
# decoder found (03 §7).
def needle_local_record
  files = needle_local_files()
  tracks = range(0, files.length()).map(fn(i) {
    {
      "n": i + 1,
      "title": needle_track_name(files[i]),
      "seconds": 0,
      "uri": "",
      "file": needle_relative(files[i])
    }
  })
  {
    "id": "machine",
    "title": "On this machine",
    "artist": str(tracks.length()) + " files",
    "artist_id": "",
    "year": "",
    "uri": "",
    "art": "",
    "art_big": "",
    "tracks": tracks
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

# The four records the welcome page offers, with their own sleeves.
#
# They are named rather than discovered: `/browse/new-releases` returns
# 403 to an application registered after November 2024, and search's
# `tag:new` answers with the world's fortnight — Hebrew, Arabic, Korean
# titles this client has no face for, which paint as tofu. A sample page
# is not the place to discover that; four records asked for by name are
# four sleeves that arrive.
#
# One call each, the first time, plus its picture into `public/covers`;
# afterwards the files are there. A record that cannot be found is simply
# not offered, and an unconfigured catalogue offers none, so the welcome
# draws its sleeves instead.
NEEDLE_PICKS = [
  ["Radiohead", "OK Computer"],
  ["Nina Simone", "I Put A Spell On You"],
  ["Aphex Twin", "Selected Ambient Works 85-92"],
  ["Fela Kuti", "Zombie"]
]

def needle_pick(who, what)
  data = needle_api("/search?type=album&limit=1&market=" + needle_market() + "&q="
  + url_encode("artist:" + who + " album:" + what))
  return nil if data.nil?

  items = ((data["albums"] ?? {})["items"] ?? [])
  return nil if items.length() == 0

  needle_album_stub(items[0])
end

def needle_fresh
  return [] unless needle_configured()

  NEEDLE_PICKS.map(fn(pair) { needle_pick(pair[0], pair[1]) }).filter(fn(a) { !a.nil? })
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

  artists = ((data["artists"] ?? {})["items"] ?? []).map(fn(a) {
    pictures = a["images"] ?? []
    {
      "id": a["id"],
      "name": a["name"],
      "art": needle_cache_art(a["id"], needle_pick_art(pictures, true), "s"),
      "art_big": needle_cache_art(a["id"], needle_pick_art(pictures, false), "m")
    }
  })
  albums = ((data["albums"] ?? {})["items"] ?? []).map(fn(a) { needle_album_stub(a) })
  {
    "artists": artists,
    "albums": albums,
    "error": ""
  }
end

def needle_album(id)
  return needle_local_record() if id == "machine"
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

# Everything the artist has, in pages. Ten at a time is not a choice:
# an app registered after Spotify's 2025 restrictions is answered
# "Invalid limit" above ten on this endpoint, whatever the documentation
# says, and it stops serving past the first handful of pages. Spotify
# also returns the same record under several editions, so the list is
# thinned by name.
def needle_artist_albums(id)
  path = "/artists/" + id + "/albums?include_groups=album,single&limit=10&offset="
  items = []
  page = 0
  while page < 6
    data = needle_api(path + str(page * 10))
    got = data.nil? ? [] : (data["items"] ?? [])
    items = items.concat(got)
    page = got.length() < 10 ? 99 : page + 1
  end
  # `uniq_by` takes a field name, not a block: the whole point is that it
  # never leaves Rust.
  items.uniq_by("name")
end

def needle_artist(id)
  return needle_sample_artist(id) if id.starts_with?("artist:")

  info = needle_api("/artists/" + id)
  return nil if info.nil?

  listing = needle_artist_albums(id)
  # An app registered after Spotify's 2025 restrictions is refused this
  # one — 403, whatever the market — along with related-artists and
  # recommendations. An older app still gets it, so the call stays and the
  # page simply has no Popular section when it comes back empty.
  tops = needle_api("/artists/" + id + "/top-tracks?market=" + needle_market())
  albums = listing.map(fn(a) { needle_album_stub(a) })
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
  pictures = info["images"] ?? []
  {
    "id": id,
    "name": info["name"],
    "genres": (info["genres"] ?? []).slice(0, 3).join(" · "),
    "albums": albums,
    "top": top,
    "art": needle_cache_art(id, needle_pick_art(pictures, true), "s"),
    "art_big": needle_cache_art(id, needle_pick_art(pictures, false), "m")
  }
end

# --------------------------------------------------------- playing a thing

# ------------------------------------- the Spotify client on this machine

# The installed client speaks MPRIS on the session bus, and that is a
# better way to reach it than the Web API: no token, no round trip to
# Stockholm, and it works on the machine Needle is running on rather than
# on whichever device Connect feels like. `OpenUri` takes a `spotify:`
# URI and plays it; the rest is transport. The window it opens can be
# parked out of sight by a window rule — it does not need to be seen to
# be driven.
NEEDLE_MPRIS = "org.mpris.MediaPlayer2.spotify"

def needle_dbus(method, args)
  call = [
    "gdbus",
    "call",
    "--session",
    "--dest",
    NEEDLE_MPRIS,
    "--object-path",
    "/org/mpris/MediaPlayer2",
    "--method",
    method
  ]
  out = System.run_sync(call.concat(args)) rescue nil
  return nil if out.nil?
  return nil if (out["exit_code"] ?? 1) != 0

  out["stdout"] ?? ""
end

def needle_player_get(name)
  needle_dbus("org.freedesktop.DBus.Properties.Get", ["org.mpris.MediaPlayer2.Player", name])
end

# Whether the client is running and listening. One cheap property read.
def needle_here_ready
  !needle_player_get("PlaybackStatus").nil?
end

def needle_here_play(uri)
  needle_dbus("org.mpris.MediaPlayer2.Player.OpenUri", [uri]).nil? ? "refused" : "here"
end

def needle_here_toggle
  needle_dbus("org.mpris.MediaPlayer2.Player.PlayPause", [])
end

def needle_here_step(forward)
  needle_dbus(forward ? "org.mpris.MediaPlayer2.Player.Next" : "org.mpris.MediaPlayer2.Player.Previous", [])
end

# A read on the person's own account rather than the application's: the
# device list, and what is playing on it.
def needle_user_api(path)
  token = needle_user_token()
  return nil if token.nil?

  opts = {"headers": {"Authorization": "Bearer " + token}, "timeout": 10}
  resp = HTTP.request("GET", NEEDLE_API + path, opts) rescue nil
  return nil if resp.nil?
  return nil if resp["status"] != 200

  json_parse(resp["body"]) rescue nil
end

# Everything Spotify will play on, this machine included when its own
# client is open. A device is where the sound comes out; Needle only says
# which one, and asks.
def needle_devices
  data = needle_user_api("/me/player/devices")
  return [] if data.nil?;

  (data["devices"] ?? []).map(fn(d) {
    {
      "id": d["id"] ?? "",
      "name": d["name"] ?? "",
      "kind": d["type"] ?? "",
      "active": d["is_active"] ?? false
    }
  })
end

# The one to play on: the one already active, else the one the person
# picked, else the first that answered.
def needle_device_id(state, devices)
  chosen = state["device"] ?? ""
  return chosen if chosen != "" && devices.filter(fn(d) { d["id"] == chosen }).length() > 0

  # The speaker Needle starts carries Needle's name, and it is the one on
  # this machine: prefer it over whatever else the account has awake.
  mine = devices.filter(fn(d) { d["name"] == "Needle" })
  return mine[0]["id"] if mine.length() > 0

  live = devices.filter(fn(d) { d["active"] })
  return live[0]["id"] if live.length() > 0
  return devices[0]["id"] if devices.length() > 0

  ""
end

def needle_device_name(devices, id)
  found = devices.filter(fn(d) { d["id"] == id })
  found.length() > 0 ? found[0]["name"] : ""
end

# Nothing here makes a sound, for want of bytes rather than for want of a
# widget (03 §7 has one). With a linked account it asks Spotify Connect to
# start on a device that is already open, and the answer is what the bar
# reports. "local" means the bar is keeping time by itself.
def needle_remote(body, device)
  token = needle_user_token()
  return "local" if token.nil? || token == ""

  head = {
    "Authorization": "Bearer " + token,
    "Content-Type": "application/json"
  }
  opts = {"headers": head, "timeout": 10}
  where = device == "" ? "" : "?device_id=" + device
  url = NEEDLE_API + "/me/player/play" + where
  resp = HTTP.request("PUT", url, opts, json_stringify(body)) rescue nil
  return "offline" if resp.nil?
  return "remote" if resp["status"] == 204 || resp["status"] == 202
  return "no device" if resp["status"] == 404
  return "needs Premium" if resp["status"] == 403
  return "the link expired" if resp["status"] == 401

  "refused"
end

# Transport on a Connect device. `play` with no body resumes what is
# already loaded, which is what a pause button's other half means.
def needle_player_command(method, path, device)
  token = needle_user_token()
  return nil if token.nil?

  where = device == "" ? "" : "?device_id=" + device
  opts = {"headers": {"Authorization": "Bearer " + token}, "timeout": 10}
  resp = HTTP.request(method, NEEDLE_API + path + where, opts, "") rescue nil
  return nil if resp.nil?

  resp["status"]
end

# What the sound is doing when it is not coming out of this window.
#
# A local sound has `time_update` (03 §7) and a remote one has nothing, so
# the bar used to freeze where the last click left it. The bar asks to be
# woken once a second (06 §1.1) while something plays elsewhere, and this
# is what it does with the second: one call, the position, and whether it
# is still playing at all — the person may have paused it from their
# phone.
def needle_poll(state)
  token = needle_user_token()
  return state if token.nil?

  opts = {"headers": {"Authorization": "Bearer " + token}, "timeout": 10}
  resp = HTTP.request("GET", NEEDLE_API + "/me/player", opts) rescue nil
  return state if resp.nil?
  # 204: nothing is playing anywhere. The bar keeps the track it shows and
  # stops pretending it moves.
  return needle_set(state, "playing", false) if resp["status"] == 204
  return state if resp["status"] != 200

  data = json_parse(resp["body"]) rescue nil
  return state if data.nil?

  state["playing"] = data["is_playing"] ?? false
  state["position"] = (data["progress_ms"] ?? 0) / 1000
  item = data["item"] ?? {}
  length = item["duration_ms"] ?? 0
  state["now"]["seconds"] = length / 1000 if length > 0
  state
end

def needle_remote_toggle(state)
  path = state["playing"] ? "/me/player/pause" : "/me/player/play"
  needle_player_command("PUT", path, state["device"] ?? "")
end

def needle_remote_step(state, forward)
  needle_player_command("POST", forward ? "/me/player/next" : "/me/player/previous", state["device"] ?? "")
end

def needle_note(mode, where)
  return "playing here" if mode == "file"
  return "playing on Spotify, on this machine" if mode == "here"
  return "the Spotify link expired — connect again" if mode == "the link expired"
  return "playing on " + (where == "" ? "your Spotify device" : where) if mode == "remote"
  return "no Spotify device is open — start one below" if mode == "no device"
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
  # The millisecond the client should seek to. It is sent on every frame
  # and the client acts only when the number changes (03 §7), so it must
  # not be recomputed from the position — that would stutter the sound.
  state["seek"] = state["seek"] ?? 0
  # What the welcome page shows: real records, fetched once when the
  # window opens (`needle_fresh`), empty when there is no catalogue to
  # ask.
  state["fresh"] = state["fresh"] ?? []
  state["devices"] = state["devices"] ?? []
  state["device"] = state["device"] ?? ""
  state["device_name"] = state["device_name"] ?? ""
  state["here"] = state["here"] ?? false
  # Whether this window started a speaker of its own, which is what it
  # stops on the way out.
  state["spawned"] = state["spawned"] ?? false
  state
end

# The window opened: take its size, and ask once for the records the
# welcome page shows. One call and, the first time, eight pictures; the
# view itself never reaches the network.
def needle_open(state, viewport)
  state["viewport"] = viewport ?? state["viewport"]
  state["fresh"] = needle_fresh()
  needle_ensure_speaker(state)
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
def needle_play(state, title, artist, album, seconds, uri, context, offset, picture, file)
  state["now"] = {
    "title": title,
    "artist": artist,
    "album": album,
    "seconds": seconds,
    "uri": uri,
    "art": picture,
    "file": file
  }
  state["seek"] = 0
  state["playing"] = true
  state["position"] = 0
  return needle_set(state, "mode", "file") if file != ""

  return needle_set(state, "mode", "local") if uri == "" && context == ""

  body = context == "" ? {"uris": [uri]} : {
    "context_uri": context,
    "offset": {"position": offset}
  }
  # The client on this machine first: it is the only one whose sound
  # comes out of these speakers, and reaching it is a D-Bus call rather
  # than a round trip to Stockholm. Connect is the fallback, for
  # everything that is somewhere else.
  if state["device"] == "here" && needle_here_ready()
    state["device_name"] = "this machine"
    state["mode"] = needle_here_play(uri == "" ? context : uri)
    return state
  end

  devices = needle_devices()
  target = needle_device_id(state, devices)
  state["devices"] = devices
  state["device"] = target
  state["device_name"] = needle_device_name(devices, target)
  state["mode"] = needle_remote(body, target)
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
    i,
    album["art"] ?? "",
    track["file"] ?? ""
  )
end

# Pause means pause where the sound is: the file in this window, or the
# Spotify client this machine is running.
# The window is closing. A speaker this application started is this
# application's to stop: leaving it running would keep the music going
# with nothing on screen, and keep forty megabytes resident for nobody.
# A device somewhere else — a phone, a room — is not ours to touch.
def needle_leaving(state)
  played_ours = state["mode"] == "remote" && state["device_name"] == "Needle"
  return state unless played_ours || state["spawned"]

  needle_player_command("PUT", "/me/player/pause", state["device"] ?? "") if played_ours
  System.run(["pkill", "-f", "librespot -n Needle"]) rescue nil
  state
end

def needle_toggle(state)
  mode = state["mode"]
  needle_here_toggle() if mode == "here"
  needle_remote_toggle(state) if mode == "remote"
  needle_set(state, "playing", !state["playing"])
end

def needle_step(state, delta)
  needle_here_step(delta > 0) if state["mode"] == "here"
  needle_remote_step(state, delta > 0) if state["mode"] == "remote"
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

# `time_update` carries `[position_ms, duration_ms]`, four times a second
# while a sound plays. It is the only clock this application has: the
# protocol has no timer, and a server cannot push a frame into a session
# on its own — so a record playing here moves its own bar, and one
# playing on a Spotify device does not.
def needle_progress(state, params)
  payload = params["payload"] ?? []
  return state if payload.length() < 2

  state["position"] = payload[0] / 1000
  state["now"]["seconds"] = payload[1] / 1000
  state
end

def needle_seek(state, params)
  return state if params["kind"] != "click"

  duration = state["now"]["seconds"] ?? 0
  return state if duration == 0

  width = needle_layout(state)["tight"] ? 200 : 360
  x = params["payload"][0]
  state["position"] = int(x * duration / width)
  state["seek"] = state["position"] * 1000
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

def needle_refresh_devices(state)
  state["here"] = needle_here_ready()
  state["devices"] = needle_devices()
  state["notice"] = state["devices"].length() == 0 ? "No Spotify device answered. Start one on this machine and ask again." : ""
  state
end

# Play what is playing, somewhere else — or here, if here is what was
# picked. The track is sent again rather than transferred: a transfer
# needs something already playing, and Needle may be the one starting it.
def needle_use_device(state, id)
  state["device"] = id
  if id == "here"
    state["device_name"] = "this machine"
    state["here"] = true
    playing = state["now"]["uri"] ?? ""
    state["mode"] = playing == "" ? state["mode"] : needle_here_play(playing)
    state["notice"] = playing == "" ? "Spotify on this machine takes the next track you play." : ""
    return state
  end

  devices = needle_devices()
  state["devices"] = devices
  state["device_name"] = needle_device_name(devices, id)
  now = state["now"]
  uri = now["uri"] ?? ""
  if uri == ""
    return needle_set(
      state,
      "notice",
      "Picked " + state["device_name"] + ". Play something and it will come out there."
    )
  end

  state["mode"] = needle_remote({"uris": [uri]}, id)
  state["notice"] = ""
  state
end

# A speaker of our own. Spotify hands out no audio, so something on this
# machine has to be the device its servers stream to; the official client
# will do it and costs a few hundred megabytes of browser to do it, while
# `librespot` is forty and has no window at all. It signs in once by
# OAuth and keeps its credentials in the cache directory named here.
#
# It is worth being plain about what this is: librespot is a
# reimplementation of a protocol Spotify never published. It wants a
# Premium account, and Spotify's terms do not contemplate it. The button
# starts what is installed; installing it was a decision made elsewhere.
# The line that starts a speaker of our own, and the watchdog around it.
#
# `--device pipewire` on purpose: the Arch build has no PulseAudio
# backend, and the default one (rodio, through cpal and ALSA) opened
# `default`, then dropped its output — Spotify went on reporting a
# playing track while PipeWire had no stream at all. `-O` drops the
# zeroconf discovery nobody here uses.
#
# The `sh` around it is a watchdog, and it is there because closing a
# window should stop the music: a spawned process outlives its parent on
# Unix, so the speaker would have gone on playing to an empty screen.
# `$PPID` inside the shell is this application; when it goes, the shell
# kills the speaker and follows it.
def needle_speaker_line
  cache = getenv("HOME") + "/.cache/needle-librespot"
  player = "librespot -n Needle -c " + cache + " --backend alsa --device pipewire --initial-volume 80 -O"
  player + " & speaker=$!; app=$PPID; while kill -0 $app 2>/dev/null; do sleep 2; done; kill $speaker"
end

def needle_spawn_speaker
  System.run(["sh", "-c", needle_speaker_line()]) rescue nil
end

def needle_ours(devices)
  devices.filter(fn(d) { d["name"] == "Needle" })
end

# On the way in: if the account has no speaker of ours, start one. The
# first frame does not wait for it to sign in — that takes a second or
# two and there is nothing to play yet; it is in the list by the time
# anything is asked of it. A speaker we started is one we stop on the way
# out, played through or not.
def needle_ensure_speaker(state)
  return state unless needle_linked()

  devices = needle_devices() ?? []
  state["devices"] = devices
  ours = needle_ours(devices)
  if ours.length() > 0
    state["device"] = ours[0]["id"]
    state["device_name"] = ours[0]["name"]
    return state
  end

  needle_spawn_speaker()
  state["spawned"] = true
  state
end

# The same, asked for by the button, which does wait: the person clicked
# and is owed an answer.
def needle_start_here(state)
  needle_spawn_speaker()
  state["spawned"] = true
  # It takes librespot a second or two to sign in and appear in the
  # account's device list, so the handler waits for it rather than
  # leaving a notice the person has to dismiss by clicking Devices
  # again. Five looks, half a second apart: either the speaker is there
  # and selected, or it is not and the notice says what to do.
  found = []
  tries = 0
  while tries < 5 && found.length() == 0
    sleep(0.5)
    found = needle_devices().filter(fn(d) { d["name"] == "Needle" })
    tries = tries + 1
  end
  if found.length() > 0
    state["devices"] = needle_devices()
    state["device"] = found[0]["id"]
    state["device_name"] = found[0]["name"]
    state["notice"] = ""
  else
    state["notice"] = "Starting a speaker on this machine — give it a moment, then ask for the devices again."
  end
  state
end

def music(event_data)
  event = event_data["event"]
  params = event_data["params"]
  props = params["props"] ?? {}
  state = needle_defaults(event_data["state"] ?? {})
  match event {
    "connect" => needle_open(state, params["viewport"]),
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
      0,
      state["artist"]["art"] ?? "",
      ""
    ),
    "machine" => needle_open_album(state, "machine"),
    "devices" => needle_refresh_devices(state),
    "device" => needle_use_device(state, props["id"]),
    "here_start" => needle_start_here(state),
    "disconnect" => needle_leaving(state),
    "progress" => needle_progress(state, params),
    "poll" => needle_poll(state),
    "ended" => needle_step(state, 1),
    "login" => needle_open_login(state),
    "toggle" => needle_toggle(state),
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
  source_badge = {
    "k": "box",
    "s": {
      "pad": [0, 2, 0, 2],
      "radius": 4,
      "bg": "surface.sunken",
      "shrink": 0
    },
    "c": [needle_label(source, 0, "medium", "text.muted")]
  }
  chips = needle_configured() ? [source_badge, needle_account_chip(state)] : [source_badge]
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
      needle_art(a["art"], a["name"], 40, 4),
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
      needle_art(a["art"], a["title"] + a["artist"], 40, 1),
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
def needle_band(name, picture, kind, title, meta, extra, sleeve_edge, on_play, props, layout)
  sleeve = needle_art(picture, name, sleeve_edge, kind == "Artist" ? 4 : 2)
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
    "c": [current && needle_sounding(state) ? needle_sleeve(title, 14, 4) : text(
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
  body = column(
    {"gap": 4, "pad": [
      0,
      0,
      4,
      0
    ]},
    [column(
      {"gap": 1, "pad": [
        2,
        2,
        4,
        2
      ]},
      rows
    )]
  )
  {"head": needle_band(
    name,
    album["art_big"] ?? "",
    "Record",
    album["title"],
    meta,
    artist_link,
    layout["tight"] ? 120 : 168,
    "play_album",
    nil,
    layout
  ), "body": body}
end

def needle_artist_pane(state, layout)
  artist = state["artist"]
  albums = artist["albums"] ?? []
  top = artist["top"] ?? []
  meta = artist["genres"] == "" ? str(albums.length()) + " records" : artist["genres"]
  first = albums.length() > 0 ? albums[0] : nil
  props = first.nil? ? nil : {"id": first["id"]}
  event = first.nil? ? "results" : "open_album"
  head = needle_band(
    artist["name"],
    artist["art_big"] ?? "",
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
  sections = []
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
  {"head": head, "body": column(
    {"gap": 4, "pad": [
      3,
      0,
      4,
      0
    ]},
    sections
  )}
end

# One tile: the sleeve, the name under it. A record's is square and opens
# the record; an artist's is round and opens the artist.
def needle_tile(key, name, sub, seed, picture, round, on_click, props)
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
    needle_art(picture, seed, 124, round ? 4 : 2),
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
  needle_tile(
    "ca:" + a["id"],
    a["title"],
    a["year"],
    a["title"] + a["artist"],
    a["art_big"],
    false,
    "open_album",
    {"id": a["id"]}
  )
end

def needle_artist_tile(a)
  needle_tile("at:" + a["id"], a["name"], "", a["name"], a["art_big"], true, "open_artist", {"id": a["id"]})
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
    [needle_sleeve(name, 120, 2), needle_label(name, 1, "medium", "text.default")],
    "suggest",
    {"q": name}
  )
end

# The one entry that plays in this window rather than on a device
# somewhere else: whatever sits in `public/music`.
def needle_machine_tile(count)
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
  face = {
    "k": "box",
    "s": {
      "width": 120,
      "height": 120,
      "radius": 2,
      "bg": "accent.base",
      "display": "row",
      "align": "center",
      "justify": "center",
      "shrink": 0
    },
    "c": [needle_label(str(count), 6, "bold", "accent.on")]
  }
  needle_hover(
    "machine",
    base,
    hover,
    [face, needle_label("On this machine", 1, "medium", "text.default")],
    "machine",
    nil
  )
end

def needle_welcome(state)
  local_count = needle_local_files().length()
  fresh = state["fresh"] ?? []
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
          "width": "100%",
          "max_width": 560,
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
            fresh.length() > 0 ? "Type a name and press Enter, or open one of these. Their sleeves are the catalogue's own: the server fetched each one, cropped it and re-encoded it, and the window sees an ordinary picture from its own origin." : "Type a name and press Enter. Every sleeve is drawn from the record's own name — no picture is fetched — and the ground behind it comes from your own theme.",
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
        [
          needle_caption(local_count > 0 ? "Try one, or your own" : "Try one"),
          row(
            {
              "gap": 2,
              "wrap": "wrap",
              "justify": "center",
              "max_width": 840
            },
            (fresh.length() > 0 ? fresh.map(fn(a) { needle_record_tile(a) }) : names.map(fn(n) {
              needle_suggestion(n)
            })).concat(local_count > 0 ? [needle_machine_tile(local_count)] : [])
          )
        ]
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
  parts = {"head": nil, "body": needle_welcome(state)}
  if pane == "album"
    parts = needle_album_pane(state, layout)
  elsif pane == "artist"
    parts = needle_artist_pane(state, layout)
  elsif pane == "results"
    parts = {"head": nil, "body": needle_results_pane(state)}
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
  head = head.concat(parts["head"].nil? ? [] : [parts["head"]])
  column(
    {
      "grow": 1,
      "min_height": 0,
      "min_width": 0,
      "gap": 0,
      "bg": "surface.raised",
      "radius": 3
    },
    head.concat([scroll(
      {
        "grow": 1,
        "min_height": 0,
        "min_width": 0,
        "gap": 0
      },
      [parts["body"]]
    )])
  )
end

# ---------------------------------------------------------- the bottom bar

# The disc turns for exactly as long as something is playing: `spin` is
# the one thing in 03 §5 that moves without a state change, and stopping
# it is a style change like any other.
# Whether something is actually coming out of a speaker — as opposed to
# the player merely wanting it to. Connect can refuse, and a record with
# no sound behind it never starts: neither should turn the disc.
def needle_sounding(state)
  return false unless state["playing"]

  mode = state["mode"]
  mode == "file" || mode == "here" || mode == "remote"
end

def needle_disc(state, size)
  now = state["now"]
  name = (now["title"] ?? "") + (now["artist"] ?? "")
  disc = needle_art(now["art"] ?? "", name == "" ? "Needle" : name, size, 4)
  disc["key"] = "disc"
  disc["s"]["animation"] = needle_sounding(state) ? "spin" : "none"
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

# The line under the transport. On a narrow window it is the width of the
# window rather than a fixed 200 px, so the filled part is a percentage
# and not a pixel count — the same bar, told in the unit that stretches.
def needle_seekbar(state, layout)
  now = state["now"]
  duration = now["seconds"] ?? 0
  position = state["position"] < duration ? state["position"] : duration
  wide = layout["single"]
  width = layout["tight"] ? 200 : 360
  share = duration == 0 ? 0 : int(100 * position / duration)
  filled = duration == 0 ? 0 : int(width * position / duration)
  # Stretched, the track takes what the row has left over after the two
  # clocks — grow, not a width, or it takes the row's whole width and
  # pushes the second clock off the end. Fixed, it is the width it always
  # was.
  track = {
    "display": "row",
    "align": "center",
    "height": 6,
    "radius": 4,
    "bg": "surface.sunken",
    "cursor": "pointer"
  }
  track = wide ? track.merge({"grow": 1, "shrink": 1}) : track.merge({
    "width": width,
    "shrink": 0
  })
  bar = {
    "k": "box",
    "key": "bar:seek",
    "s": track,
    "on": {"click": "seek"},
    "c": [{"k": "box", "s": {
      "width": wide ? str(share) + "%" : filled,
      "height": 6,
      "radius": 4,
      "bg": "accent.base",
      "transition": "fast"
    }}]
  }
  frame = {
    "gap": 2,
    "align": "center",
    "shrink": 0
  }
  # No width: the column above stretches it, and a percentage would be a
  # percentage of the padded box — which is what pushed the second clock
  # off the end.
  frame = frame.merge({"shrink": 1}) if wide
  clock_style = {
    "size": 0,
    "fg": "text.muted",
    "shrink": 0
  }
  row(frame, [text(needle_clock(position), clock_style), bar, text(needle_clock(duration), clock_style)])
end

# Where the sound comes out. The list is Spotify's answer for this
# account, and the accent marks the one Needle is asking. The window
# itself is never in it: EUI plays what the application gives it (03 §7)
# and the Web API gives no audio, so the way to hear Spotify on this
# machine is Spotify's own client, which is a device like any other once
# it runs.
def needle_chip(label, event, props, active)
  base = {
    "display": "row",
    "align": "center",
    "pad": [0, 2, 0, 2],
    "radius": 4,
    "bg": active ? "accent.base" : "surface.sunken",
    "cursor": "pointer",
    "transition": "fast",
    "shrink": 0,
    "max_width": 160
  }
  hover = base.merge({"bg": active ? "accent.hover" : "border.subtle"})
  face = needle_label(label, 0, "medium", active ? "accent.on" : "text.muted")
  face["s"]["clamp"] = 1
  needle_hover("chip:" + event + label, base, hover, [face], event, props)
end

# The right of the bar: which device is playing, and how to get one.
#
# A narrow window used to drop this whole region, which took with it the
# only control that starts a speaker — silently, at 999 px. It keeps the
# chips that lead somewhere and sheds the list instead: one device at a
# tight width, three otherwise.
def needle_devices_bar(state, layout)
  empty = {"k": "box", "s": {"grow": 1, "width": 0}}
  return empty unless needle_linked()

  devices = state["devices"] ?? []
  chips = devices.slice(0, layout["tight"] ? 1 : 3).map(fn(d) {
    needle_chip(d["name"], "device", {"id": d["id"]}, d["id"] == state["device"])
  })
  # The client on this machine is not in Spotify's list until it is
  # playing, and it is the one that matters here, so it leads.
  chips = [needle_chip("this machine", "device", {"id": "here"}, state["device"] == "here")].concat(chips) if state["here"]
  # The narrowest window keeps the same button under a shorter name
  # rather than losing it: it is the only way to get a speaker at all.
  chips = chips.concat([needle_chip(
    layout["single"] ? "Speaker here" : "Start a speaker here",
    "here_start",
    nil,
    false
  )]) if devices.length() == 0
  chips = chips.concat([needle_chip("Devices", "devices", nil, false)])
  row(
    {
      "gap": 2,
      "align": "center",
      "grow": 1,
      "width": 0,
      "justify": "end"
    },
    chips
  )
end

def needle_bar(state, layout)
  now = state["now"]
  title = now["title"] ?? "Nothing playing"
  note = needle_note(state["mode"], state["device_name"])
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
  right = needle_devices_bar(state, layout)
  # A zero-sized leaf that plays (03 §7). `position` is `state["seek"]`
  # and not the running position: the client seeks when that number
  # changes, so sending the clock would restart the sound four times a
  # second.
  file = now["file"] ?? ""
  sound = file == "" ? [] : [audio(file, {
    "playing": state["playing"],
    "volume": 90,
    "position": state["seek"]
  }, {"ended": "ended", "time_update": "progress"})]
  # 06 §1.1: while the sound is somewhere else, the bar asks to be woken
  # once a second — it is the only clock a remote player has. A local
  # sound needs none: its own `time_update` is the clock, and asking for
  # both would poll Spotify for a position the mixer already knows.
  # A narrow window puts the line under everything else instead of
  # squeezing three regions into one row: the title had been clipped
  # mid-word and the device chip down to its first letter. Same parts,
  # stacked — what is playing and the controls above, the line across the
  # whole width below.
  shell = {
    "gap": layout["single"] ? 2 : 4,
    "align": "center",
    "pad": [2, 3, 2, 3],
    "bg": "surface.raised",
    "radius": 3,
    "shrink": 0
  }
  bar = row(shell, [left, middle, right].concat(sound))
  if layout["single"]
    top = row(
      {"gap": 3, "align": "center"},
      [left, needle_transport(state, layout), right]
    )
    bar = column(shell.merge({"align": "stretch"}), [top, needle_seekbar(state, layout)].concat(sound))
  end
  if state["mode"] == "remote"
    # Once a second while it plays, every three when it does not: the
    # pause may have come from a phone, and a bar that stopped asking
    # would never learn that the music started again.
    bar["p"] = (bar["p"] ?? {}).merge({"wake": state["playing"] ? 1000 : 3000})
    bar["on"] = (bar["on"] ?? {}).merge({"wake": "poll"})
  end
  bar
end

# ------------------------------------------------------------------ shell

# The rail earns its place once a search has run; before that the welcome
# has the whole width, and on a narrow window the two take turns.
def needle_panes(state, layout)
  return [needle_detail(state, layout)] if state["pane"] == "welcome"
  return [needle_rail(state, layout)] if layout["single"] && state["pane"] == "results"
  return [needle_detail(state, layout)] if layout["single"]

  # A record opened from the welcome page has no search behind it, so the
  # rail would stand there empty — a column of nothing beside the thing
  # you asked for. It appears when it has something to hold.
  return [needle_detail(state, layout)] if state["artists"].length() + state["albums"].length() == 0;

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
