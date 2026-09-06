# EUI documentation site.

get("/", "home#index")
get("/health", "home#health")

# Documentation. `:page` is looked up in a whitelist inside the controller —
# it is never joined onto a filesystem path, so there is no traversal to guard.
get("/docs", "docs#index")
get("/docs/:page", "docs#show")
