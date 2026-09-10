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

# Health check endpoint
get("/health", "home#health")

print("Routes loaded!")
