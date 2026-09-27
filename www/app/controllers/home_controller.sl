# Home controller - handles the root routes

class HomeController < Controller
  # GET /
  def index
    render("home/index", {"title": "EUI — an interface is not a document"})
  end

  # GET /components
  def components
    render(
      "home/components",
      {
        "title": "EUI components",
        "description": "The component catalogue: Soli functions that return a hash, "
        + "composed from primitives. No widget runtime, no registry, no "
        + "client release behind any name."
      }
    )
  end

  # GET /controls
  def controls
    render(
      "home/controls",
      {
        "title": "EUI controls — the base every widget is built on",
        "description": "control: one options hash that carries a widget's states, "
        + "size and tone, so every interactive widget can be disabled, "
        + "pressed and focused the same way."
      }
    )
  end

  # GET /gaps
  def gaps
    render(
      "home/gaps",
      {
        "title": "EUI gaps — what is not there yet",
        "description": "What a desktop application expects and EUI does not do yet — "
        + "text shaping, input methods, accessibility, selection — each "
        + "with where it would be fixed."
      }
    )
  end

  # GET /samples
  def samples
    render(
      "home/samples",
      {
        "title": "EUI samples — eight applications, one client",
        "description": "Eight applications, one client: real renders taken off the "
        + "wire from a Soli server, laid out and painted on the GPU by "
        + "the Rust client. No mockups, no browser."
      }
    )
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
      {
        "title": "EUI demo — the gallery, running in this page",
        "description": "The EUI gallery running in your browser: the same Rust "
        + "client as the downloads, compiled to WebAssembly and drawing "
        + "on a canvas.",
        "eui_build": this._eui_build()
      }
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
