# Three EUI components. Needs a `soli` built with `--features eui`.
#
# router_eui(component, handler, view):
#   handler — a LiveView handler: {event, params, state} -> state
#   view    — a function of state returning the node tree
router_eui("counter", "live#counter", "live#counter_view")
router_eui("todo", "live#todo", "live#todo_view")
router_eui("table", "live#table", "live#table_view")

get("/health", "home#health")
