# Routes configuration
# Define your application routes here

# Home page
get("/", "home#index")

# The component reference
get("/components", "home#components")

# The control base: states, tones, sizes, semantics
get("/controls", "home#controls")

# The honest half: what a desktop application expects and does not get
get("/gaps", "home#gaps")

# What people build with it: every sample application, rendered
get("/samples", "home#samples")

# The gallery, running in the page. Its session is proxied to the demo
# application under this host so that the origin matches (see the controller).
get("/demo", "home#demo")

# The documentation and the specification, served here rather than handed
# to a file browser on github.com. `:page` is a key into a whitelist in the
# controller, not a path fragment.
get("/docs", "docs#index")
get("/docs/:page", "docs#show")
get("/spec", "docs#spec_index")
get("/spec/:page", "docs#spec")

# Health check endpoint
get("/health", "home#health")

print("Routes loaded!")
