# Routes configuration
# Define your application routes here

# Home page
get("/", "home#index")

# The component reference
get("/components", "home#components")

# Health check endpoint
get("/health", "home#health")

print("Routes loaded!")
