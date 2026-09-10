# Home controller - handles the root routes

class HomeController < Controller
  # GET /
  def index
    render("home/index", {"title": "EUI — an interface is not a document"})
  end

  # GET /components
  def components
    render("home/components", {"title": "EUI components"})
  end

  # GET /controls
  def controls
    render("home/controls", {"title": "EUI controls — the base every widget is built on"})
  end

  # GET /gaps
  def gaps
    render("home/gaps", {"title": "EUI gaps — what is not there yet"})
  end

  # GET /samples
  def samples
    render("home/samples", {"title": "EUI samples — seven applications, one client"})
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
