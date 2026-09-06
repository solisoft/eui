# The counter, served over EUI. Needs a `soli` built with `--features eui`.
#
# router_eui(component, handler, view):
#   handler — a LiveView handler: {event, params, state} -> state
#   view    — a function of state returning the node tree (see stdlib/eui.sl)
router_eui("counter", "live#counter", "live#counter_view")

get("/health", "home#health")
