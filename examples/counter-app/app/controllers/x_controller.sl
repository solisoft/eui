# Feedx, connected to a real timeline.
#
# The feed sample is forty thousand invented posts, and that is what it is
# for: the client has to lay out and paint only the rows it shows. This
# file is the other half — the same cards, filled from x.com's API, so the
# demo can be pointed at an account and scrolled.
#
# What X asks for, and why the shape is the one Needle already uses:
#
#   * The home timeline (`/2/users/:id/timelines/reverse_chronological`) is
#     a *user context* endpoint: an app-only bearer cannot read it, so
#     there is a browser step, a refresh token, and an access token minted
#     from it before each fetch. Needle does the same for Spotify Connect.
#   * It is not on the free tier. Basic reads ten thousand posts a month,
#     five requests per fifteen minutes; the app fetches on connect and
#     when asked, never on a scroll.
#   * The client fetches assets from its own origin by hash (01 §2.2) and
#     decodes PNG only, so pictures are downloaded here, re-encoded, and
#     named like any other file in `public`. X never hears from the window.
#
# Set these before starting: X_CLIENT_ID, X_CLIENT_SECRET, then visit
# /x/login once and put the X_REFRESH_TOKEN it hands you in the launcher.

X_API = "https://api.x.com/2"
X_AUTHORIZE = "https://x.com/i/oauth2/authorize"
X_TOKEN_URL = "https://api.x.com/2/oauth2/token"

# Read the timeline, know who "me" is, and come back with a refresh token.
X_SCOPE = "tweet.read users.read offline.access"

def x_configured
  id = getenv("X_CLIENT_ID") ?? ""
  secret = getenv("X_CLIENT_SECRET") ?? ""
  id != "" && secret != ""
end

def x_linked
  x_configured() && (getenv("X_REFRESH_TOKEN") ?? "") != ""
end

# Where X sends the browser back. It must match the callback registered on
# the developer portal character for character.
def x_redirect
  getenv("X_REDIRECT_URI") ?? "http://127.0.0.1:5092/x/callback"
end

# `state` proves the callback belongs to a login this app started, and the
# PKCE verifier proves the code is being spent by whoever asked for it.
# Both would normally be nonces kept between two requests; there is no
# database here and no durable file (the desktop build's application
# folder dies with the process), so both are derived from the client
# secret instead. An attacker who cannot read the secret can forge
# neither, which is what the two parameters are for.
def x_login_state
  Crypto.sha256(getenv("X_CLIENT_SECRET") + ":feedx-login").substring(0, 32)
end

def x_verifier
  Crypto.sha256(getenv("X_CLIENT_SECRET") + ":feedx-pkce")
end

# S256: the challenge is the verifier's SHA-256, base64url, unpadded.
# `Crypto.sha256` speaks hex, `Hex.decode` turns it back into the bytes
# `Base64.encode` wants.
def x_challenge
  raw = Base64.encode(Hex.decode(Crypto.sha256(x_verifier())))
  raw.replace("+", "-").replace("/", "_").replace("=", "")
end

def x_authorize_url
  query = [
    "response_type=code",
    "client_id=" + url_encode(getenv("X_CLIENT_ID")),
    "redirect_uri=" + url_encode(x_redirect()),
    "scope=" + url_encode(X_SCOPE),
    "state=" + x_login_state(),
    "code_challenge=" + x_challenge(),
    "code_challenge_method=S256"
  ]
  X_AUTHORIZE + "?" + query.join("&")
end

# A confidential client authenticates at the token endpoint with Basic,
# not with the client_id in the body.
def x_basic
  Base64.encode(getenv("X_CLIENT_ID") + ":" + getenv("X_CLIENT_SECRET"))
end

def x_token_post(form)
  opts = {"headers": {
    "Content-Type": "application/x-www-form-urlencoded",
    "Authorization": "Basic " + x_basic()
  }, "timeout": 10}
  resp = HTTP.request("POST", X_TOKEN_URL, opts, form) rescue nil
  return nil if resp.nil?
  return nil if resp["status"] != 200

  json_parse(resp["body"]) rescue nil
end

def x_exchange(code)
  form = [
    "grant_type=authorization_code",
    "code=" + url_encode(code),
    "redirect_uri=" + url_encode(x_redirect()),
    "code_verifier=" + x_verifier(),
    "client_id=" + url_encode(getenv("X_CLIENT_ID"))
  ]
  x_token_post(form.join("&"))
end

# A refresh token is good until it is used; each exchange hands back a new
# one. The app spends the one in the environment and keeps the access
# token it gets in the cache when there is one, so a scroll never mints.
def x_access_token
  hit = Cache.get("feedx:token") rescue nil
  return hit unless hit.nil?

  refresh = getenv("X_REFRESH_TOKEN") ?? ""
  return nil if refresh == "" || !x_configured()

  form = [
    "grant_type=refresh_token",
    "refresh_token=" + url_encode(refresh),
    "client_id=" + url_encode(getenv("X_CLIENT_ID"))
  ]
  data = x_token_post(form.join("&"))
  return nil if data.nil?

  token = data["access_token"]
  Cache.set("feedx:token", token, 6000) rescue nil
  token
end

def x_api(path)
  token = x_access_token()
  return nil if token.nil?

  opts = {"headers": {"Authorization": "Bearer " + token}, "timeout": 15}
  resp = HTTP.request("GET", X_API + path, opts) rescue nil
  return nil if resp.nil?
  return nil if resp["status"] != 200

  json_parse(resp["body"]) rescue nil
end

def x_me
  hit = Cache.get("feedx:me") rescue nil
  return hit unless hit.nil?

  data = x_api("/users/me")
  return nil if data.nil?

  id = (data["data"] ?? {})["id"]
  Cache.set("feedx:me", id, 86400) rescue nil
  id
end

# ------------------------------------------------------------- pictures
# X serves JPEG and the client decodes PNG (`eui-client/src/assets.rs`),
# so every picture is fetched once, re-encoded, and kept under a name that
# is its own cache key.

def x_png(tag, id, url, edge)
  return "" if url.nil? || url == "" || id == ""

  png = "public/x/" + tag + "-" + id + ".png"
  return png if File.exists(png)

  File.mkdir_p("public/x") rescue nil
  raw = "public/x/" + tag + "-" + id + ".src"
  written = HTTP.download(url, raw) rescue 0
  return "" if written == 0

  x_fit(raw, png, edge) rescue nil
  File.delete(raw) rescue nil
  File.exists(png) ? png : ""
end

# An avatar is a square; a post's picture keeps its shape and is only
# bounded, because the card gives it a fixed box to sit in.
def x_fit(source, target, edge)
  picture = Image.new(source)
  wide = picture.width()
  tall = picture.height()
  return picture.resize(edge, edge).format("png").to_file(target) if wide == tall

  scale = wide > tall ? edge * 1.0 / wide : edge * 1.0 / tall
  picture.resize(int(wide * scale), int(tall * scale)).format("png").to_file(target)
end

def x_avatar(user)
  # X hands out a 48 px "_normal" thumbnail; the card draws 40.
  url = (user["profile_image_url"] ?? "").replace("_normal", "_bigger")
  x_png("a", user["id"] ?? "", url, 96)
end

# ---------------------------------------------------------------- posts

def x_index(items, key)
  by_id = {}
  for item in items
    by_id[item[key]] = item
  end
  by_id
end

# "3h", "12m", "2d" — the card has room for two characters and a unit.
def x_when(created)
  return "" if created.nil? || created == ""

  seconds = int((DateTime.now().to_unix() - DateTime.parse(created).to_unix()))
  return str(seconds) + "s" if seconds < 60
  return str(seconds / 60) + "m" if seconds < 3600
  return str(seconds / 3600) + "h" if seconds < 86400

  str(seconds / 86400) + "d"
end

# What the card asks for, filled from what X answered. The keys are the
# sample feed's keys exactly, so `post_card` needs to know nothing about
# where a post came from.
def x_post(i, tweet, users, media)
  author = users[tweet["author_id"]] ?? {}
  name = author["name"] ?? "Someone"
  metrics = tweet["public_metrics"] ?? {}
  keys = (tweet["attachments"] ?? {})["media_keys"] ?? []
  picture = ""
  kind = ""
  for key in keys
    item = media[key] ?? {}
    # A photo has a url; a video or a gif has a still, which is what the
    # window can show — it decodes no streams.
    src = item["url"] ?? item["preview_image_url"] ?? ""
    if picture == "" && src != ""
      picture = x_png("m", key, src, 960)
      kind = picture == "" ? "" : "image"
    end
  end
  {
    "id": i,
    "name": name,
    "handle": "@" + (author["username"] ?? "someone"),
    "initial": name[0],
    "tone": FEED_TONES[i % FEED_TONES.length()],
    "when": x_when(tweet["created_at"]),
    "text": tweet["text"] ?? "",
    "replies": metrics["reply_count"] ?? 0,
    "reposts": metrics["retweet_count"] ?? 0,
    "likes": metrics["like_count"] ?? 0,
    "n": i + 1,
    "media": kind,
    "image": picture == "" ? nil : picture,
    "avatar": x_avatar(author)
  }
end

# One request: the timeline, with its authors and its pictures attached,
# mapped onto the cards the sample already draws.
def x_timeline(limit)
  id = x_me()
  return [] if id.nil?

  query = [
    "max_results=" + str(limit),
    "tweet.fields=" + url_encode("created_at,public_metrics,attachments"),
    "expansions=" + url_encode("author_id,attachments.media_keys"),
    "user.fields=" + url_encode("name,username,profile_image_url"),
    "media.fields=" + url_encode("type,url,preview_image_url")
  ]
  data = x_api("/users/" + id + "/timelines/reverse_chronological?" + query.join("&"))
  return [] if data.nil?

  tweets = data["data"] ?? []
  includes = data["includes"] ?? {}
  users = x_index(includes["users"] ?? [], "id")
  media = x_index(includes["media"] ?? [], "media_key")
  range(0, tweets.length()).map(fn(i) { x_post(i, tweets[i], users, media) })
end

# ------------------------------------------------------------ the pages
# The same two routes Needle has, and the same warning: they answer under
# `soli serve`, not in a packaged desktop build, whose launcher gate
# refuses every browser request. Link the account once from the repo
# server; the packaged app only ever spends the token.

def x_page(title, body)
  head = "<!doctype html><meta charset=utf-8><title>" + title + "</title>"
  style = "<style>body{font:16px/1.6 system-ui;margin:6rem auto;max-width:34rem;padding:0 1.5rem}code{display:block;background:#f4f4f5;padding:1rem;border-radius:.5rem;overflow-wrap:anywhere}</style>"
  head + style + "<h1>" + title + "</h1>" + body
end

def x_reply(title, body)
  {
    "status": 200,
    "headers": {"Content-Type": "text/html; charset=utf-8"},
    "body": x_page(title, "<p>" + body + "</p>")
  }
end

class XController < Controller
  # GET /x/login — send the browser to X's consent screen.
  def login
    unless x_configured()
      return x_reply(
        "Not configured",
        "Set X_CLIENT_ID and X_CLIENT_SECRET first, and register " + x_redirect()
        + " as the app's callback URI."
      )
    end

    {
      "status": 302,
      "headers": {"Location": x_authorize_url()},
      "body": ""
    }
  end

  # GET /x/callback — X sends the browser back here with a code.
  def callback
    return x_reply("Not configured", "This app has no X client to exchange a code with.") unless x_configured()

    refused = params["error"].to_s
    return x_reply("X said no", "It answered “" + refused + "”. Nothing was saved.") if refused != ""

    if params["state"].to_s != x_login_state()
      return x_reply(
        "That did not start here",
        "The state parameter did not match, so the code was ignored. Start again from /x/login."
      )
    end

    code = params["code"].to_s
    return x_reply("No code came back", "X redirected without one. Start again from /x/login.") if code == ""

    tokens = x_exchange(code)
    if tokens.nil?
      return x_reply(
        "The exchange failed",
        "X would not trade that code for a token. Check that the callback URI on the portal reads exactly "
        + x_redirect()
      )
    end

    refresh = tokens["refresh_token"] ?? ""
    if refresh == ""
      return x_reply(
        "No refresh token",
        "X returned an access token but no refresh token — the offline.access scope is what asks for one."
      )
    end

    told = "<p>Feedx is linked to your account. One step is left, and it is manual on purpose: this app has nowhere durable to keep a secret, so the token goes where the other two live.</p>"
    told = told + "<p>Put this line in <b>~/.local/bin/feedx</b>, just before <b>exec</b>, and start the feed again.</p>"
    told = told + "<code>export X_REFRESH_TOKEN=" + refresh + "</code>"
    told = told + "<p>X hands back a new one every time this one is spent, so if the feed stops loading, come back here.</p>"
    {
      "status": 200,
      "headers": {"Content-Type": "text/html; charset=utf-8"},
      "body": x_page("Linked", told)
    }
  end
end
