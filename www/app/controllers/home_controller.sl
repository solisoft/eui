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

  # GET /demo
  #
  # The gallery, running. Its session is opened at `/_eui/session/gallery` on
  # *this* host, not on the demo server's — Soli refuses a WebSocket upgrade
  # whose `Origin` is not its own, so the page and the session have to share
  # an origin. The proxy makes that true by routing `/_eui/*` here to the
  # demo application; see `deploy/README.md`.
  def demo
    render(
      "home/demo",
      {"title": "EUI demo — the gallery, running in this page", "eui_build": this._eui_build()}
    )
  end

  # Which build of the browser client is on disk, for the page to stamp on
  # the script URL. `xtask-web` writes the manifest beside the module; a tree
  # with no manifest has no client built yet, and says so rather than
  # guessing a version.
  def _eui_build
    raw = slurp("public/eui/manifest.json") rescue null
    return "none" if raw.nil?

    parsed = JSON.parse(raw) rescue null
    parsed.nil? ? "none" : (parsed["version"] ?? "none")
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
