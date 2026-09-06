# Home controller — handles the root routes

class HomeController < Controller
  # GET /
  def index
    render(
      "home/index",
      {"title": "A user interface in 150 bytes", "layout": "layouts/eui"}
    )
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
