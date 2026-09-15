# The one HTML page this application serves: an EUI session, in a browser,
# beside the four lines of Soli that produce it.
#
# It is here and not on the marketing site for one reason, and it is not a
# preference. Soli refuses a WebSocket upgrade whose `Origin` is not its own
# (SEC-046, cross-site WebSocket hijacking), so the page that opens a session
# must be served by the server that answers it. The alternative was to vendor
# the eight thousand lines of the widget catalogue into `www/` so that it
# could serve sessions of its own; this costs nothing and duplicates nothing.
#
# `:component` is a *key into a whitelist*, never a path fragment and never
# passed to `router_eui` — an unknown slug 404s before anything else happens,
# so there is no way to ask this page for a session that does not exist.
class LivePageController < Controller
  def index
    redirect("/live/counter")
  end

  def show
    slug = params["component"].to_s
    entry = this._components()[slug]

    if entry.nil?
      return {
        "status": 404,
        "headers": {"Content-Type": "text/html; charset=utf-8"},
        "body": "<!doctype html><meta charset=utf-8><title>No such component</title>"
        + "<p>No component named <code>" + html_escape(slug) + "</code>. "
        + "<a href=\"/live/counter\">Start with the counter</a>."
      }
    end

    @slug = slug
    @title = entry["title"]
    @lead = entry["lead"]
    @source = entry["source"]
    @components = this._components()
    @build = this._build()

    render("live/show", {"layout": "layouts/live"})
  end

  # ---------------------------------------------------------------- private

  # Which build of the browser client is on disk, for the page to stamp on
  # the script URL.
  #
  # The module is served `immutable` for a year, so without this a deploy
  # that replaced the client would never be fetched again — the reader would
  # go on running a year-old one with nothing to say so. `xtask web` writes
  # the manifest beside the module; a tree with no manifest has no client
  # built yet, and says so rather than guessing a version.
  def _build
    raw = slurp("public/eui/manifest.json") rescue null
    return "none" if raw.nil?
    parsed = JSON.parse(raw) rescue null
    parsed.nil? ? "none" : (parsed["version"] ?? "none")
  end

  # slug -> title, one line about it, and the file its handler and view live
  # in. Every slug here is also a `router_eui` in `config/routes.sl`; one
  # without the other is a page that cannot open, so they are checked against
  # each other by `tests/live_page_spec.sl`.
  def _components
    {
      "counter": {
        "title": "Counter",
        "lead": "The smallest round trip there is: a click becomes an event, the server answers with a patch, the client draws it.",
        "source": "app/controllers/live_controller.sl"
      },
      "todo": {
        "title": "Todo",
        "lead": "Keyed rows, a text field and checkboxes — the reconciliation primitive of the wire format, doing its job.",
        "source": "app/controllers/live_controller.sl"
      },
      "table": {
        "title": "Ten thousand rows",
        "lead": "A virtualised list. The mount is 770 KB and the client lays out only the rows that have boxes.",
        "source": "app/controllers/live_controller.sl"
      },
      "gallery": {
        "title": "Gallery",
        "lead": "Content-addressed pictures, fetched by hash and verified before anything is decoded.",
        "source": "app/controllers/live_controller.sl"
      },
      "feed": {
        "title": "Feed",
        "lead": "A timeline built from the same seventeen primitives as everything else.",
        "source": "app/controllers/eui_builders_feed.sl"
      }
    }
  end
end
