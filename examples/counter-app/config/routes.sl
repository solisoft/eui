# Three EUI components. Needs a `soli` built with `--features eui`.
#
# router_eui(component, handler, view):
#   handler — a LiveView handler: {event, params, state} -> state
#   view    — a function of state returning the node tree
router_eui("counter", "live#counter", "live#counter_view")
router_eui("todo", "live#todo", "live#todo_view")
router_eui("table", "live#table", "live#table_view")
router_eui("gallery", "live#gallery", "live#gallery_view")
router_eui("feed", "live#feed", "live#feed_view")
router_eui("music", "music#music", "music#music_view")
# Spec 06 §1.1, in ten lines: a node that asks to be woken.
router_eui("clock", "live#clock", "live#clock_view")
# Spec 01 §4: a view the server cannot encode, so the suite can watch a
# session end with its reason instead of freezing.
router_eui("broken", "live#broken", "live#broken_view")

get("/health", "home#health")

# Linking a Spotify account: the only two pages this app serves. The
# catalogue itself needs neither — client credentials has no browser step.
get("/spotify/login", "spotify#login")
get("/spotify/callback", "spotify#callback")

# What the manifest asks the client for; the person still has to allow it.
eui_capabilities("clipboard.read")
