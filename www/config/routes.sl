# Routes configuration
# Define your application routes here

# Home page
get("/", "home#index")

# The component reference
get("/components", "home#components")

# What people build with it: every sample application, rendered
get("/samples", "home#samples")

# Health check endpoint
get("/health", "home#health")

print("Routes loaded!")
