# Home controller - handles the root routes

class HomeController < Controller
  # GET /
  def index
    render("home/index", {"title": "EUI — an interface is not a document"})
  end

  # GET /health
  def health
    {
      "status": 200,
      "headers": {"Content-Type": "application/json"},
      "body": "{\"status\":\"ok\"}"
    }
  end
end
