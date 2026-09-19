# Eleven EUI components. Needs a `soli` built with `--features eui`.
#
# router_eui(component, handler, view, options?):
#   handler — a LiveView handler: {event, params, state} -> state
#   view    — a function of state returning the node tree
#   options — {"session": "required"}
#
# **The first one is the default.** The EUI manifest carries a single `entry`
# (01 §2.1) and this server carries eleven components, so one of them has to
# be what `wss://host` means, and it is this one — which is why the gallery
# leads rather than the counter. A newer soli also takes `{"default": true}`
# to say so explicitly, and this file does not use it: the deploy installs a
# *released* soli, and a routes file that only loads under an unreleased one
# is a routes file that does not load.
# The application's own name would go here — `eui_name("Meridian")` — and does
# not yet. The builtin is on `soli_lang` main and in no release, the deploy
# pins an exact Soli, and `rescue` does not help: an unknown identifier is
# a *static* error and the routes never run. Re-add it with the pin, not
# before. Until then the manifest carries the directory's name, which is
# what a launcher entry is labelled with.

router_eui("gallery", "live#gallery", "live#gallery_view")
router_eui("counter", "live#counter", "live#counter_view")
router_eui("todo", "live#todo", "live#todo_view")
router_eui("table", "live#table", "live#table_view")
router_eui("feed", "live#feed", "live#feed_view")
router_eui("music", "music#music", "music#music_view")
# A tracker: the pattern, the instruments and the mixer are Soli; the
# window is handed the wav its Play button rendered.
router_eui("tracker", "tracker#tracker", "tracker#tracker_view")
# A code editor whose buffer is its own source, highlighted by the server.
router_eui("editor", "editor#editor", "editor#editor_view")
# Spec 06 §1.1, in ten lines: a node that asks to be woken.
router_eui("clock", "live#clock", "live#clock_view")
# Spec 01 §4: a view the server cannot encode, so the suite can watch a
# session end with its reason instead of freezing.
router_eui("broken", "live#broken", "live#broken_view")
# Atrium: a team messenger, and the only component here whose screen changes
# because someone at another window did something. Open it twice.
router_eui("chat", "chat#chat", "chat#chat_view")

# The one HTML page here: an EUI session in a browser, beside the Soli that
# produces it. It must be served by *this* application and not by the
# marketing site, because Soli refuses a WebSocket upgrade whose `Origin` is
# not its own — see `app/controllers/live_page_controller.sl`.
get("/live", "live_page#index")
get("/live/:component", "live_page#show")

get("/health", "home#health")

# Linking a Spotify account: the only two pages this app serves. The
# catalogue itself needs neither — client credentials has no browser step.
get("/spotify/login", "spotify#login")
get("/spotify/callback", "spotify#callback")

# The same for an X account, which the feed needs before it can show a
# real timeline: the home timeline is a user-context endpoint.
get("/x/login", "x#login")
get("/x/callback", "x#callback")

# What the manifest asks the client for; the person still has to allow it.
# `fs.pick` is Atrium's: without it the attach button opens no dialog at all,
# and there is no diagnostic the application can see (03 §3.2).
eui_capabilities("clipboard.read", "fs.pick", "camera", "microphone", "location", "nfc", "scene")

# Two faces from Google Fonts, fetched *here* — by the server, at boot, once
# per deploy — and served from this application's own origin by content hash.
# The window never talks to a font service: that is spec 08 §8's "no
# third-party connection", and it stays true because the third party is the
# server's business and not the viewer's (`app/services/google_fonts.sl`).
#
# Declaring a font raises what the manifest asks of a client to EUI 4, since
# `DefFont` and a font role are both decode errors below it — so this belongs
# at boot, before a manifest is signed, and not inside a view.
#
# `rescue nil` because `soli routes` evaluates this file with the builtins
# and *without* `app/services`, where `google_font` is defined — so the
# check that the routes parse would otherwise die on a font. At boot the
# services are loaded and the declaration happens; under `soli routes` it is
# skipped, which is right: that command asks whether the routes load, not
# whether a font service answered. The service already answers `"sans"` for
# every other way this can fail, and an application whose typography depends
# on a network that is down should still draw.
google_font("Playfair Display", [400, 700]) rescue nil
google_font("Space Grotesk", [400, 700]) rescue nil
