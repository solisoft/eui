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
