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

get("/health", "home#health")


# What the manifest asks the client for; the person still has to allow it.
eui_capabilities("clipboard.read")
