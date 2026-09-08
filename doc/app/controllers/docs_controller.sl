# Serves the EUI documentation.
#
# Pages are Markdown on disk, rendered per request. The `:page` route segment
# is a *key into a whitelist*, never a path fragment: an unknown slug 404s
# before anything touches the filesystem, so `../../.env` is not a case to
# defend against — it is a lookup miss.

class DocsController < Controller
  def index
    redirect("/docs/overview")
  end

  def show
    slug = params["page"].to_s
    entry = this._pages()[slug]

    if entry.nil?
      return {
        "status": 404,
        "headers": {"Content-Type": "text/html; charset=utf-8"},
        "body": "<!doctype html><meta charset=utf-8><title>No such page</title>"
        + "<p>No documentation page named <code>"
        + html_escape(slug)
        + "</code>. "
        + "<a href=\"/docs\">Start from the overview</a>."
      }
    end

    @slug = slug
    @title = entry["title"]
    @lead = entry["lead"]
    @sections = this._sections()
    @html = Markdown.to_safe_html(File.read("docs/eui/" + entry["file"]))

    render("docs/show", {"layout": "layouts/eui_docs"})
  end

  # ---------------------------------------------------------------- private

  # slug -> file, title, one-line lead. Adding a page means adding a row here;
  # nothing is discovered by scanning a directory, so a stray file in
  # `docs/eui/` is never reachable.
  def _pages
    pages = {}
    for section in this._sections()
      for item in section["items"]
        pages[item["slug"]] = item
      end
    end
    return pages
  end

  def _sections
    return [
      {"title": "Start here", "items": [
        {
          "slug": "overview",
          "file": "overview.md",
          "title": "What EUI is",
          "lead": "Why an application UI does not need a document engine."
        },
        {
          "slug": "status",
          "file": "status.md",
          "title": "What works today",
          "lead": "Built, specified, and not started — kept honest."
        }
      ]},
      {"title": "The protocol", "items": [
        {
          "slug": "wire-format",
          "file": "wire-format.md",
          "title": "Wire format",
          "lead": "Atoms, computed styles, flat subtrees, tree patches."
        },
        {
          "slug": "transport",
          "file": "transport.md",
          "title": "Transport",
          "lead": "Discovery, the signed manifest, and the session over HTTPS."
        },
        {
          "slug": "events",
          "file": "events.md",
          "title": "Events",
          "lead": "What the client sends back, and what the server may believe."
        }
      ]},
      {"title": "Building", "items": [
        {
          "slug": "views",
          "file": "views.md",
          "title": "Writing views",
          "lead": "A view is a Soli function: state in, node tree out."
        },
        {
          "slug": "components",
          "file": "components.md",
          "title": "Components",
          "lead": "The node, the style vocabulary, and all 107 builders."
        },
        {
          "slug": "widgets",
          "file": "widgets.md",
          "title": "Widget catalogue",
          "lead": "Fourteen primitives, and the catalogue composed from them."
        },
        {
          "slug": "theming",
          "file": "theming.md",
          "title": "Theming",
          "lead": "Roles, scales, and why the client resolves them."
        }
      ]},
      {"title": "Guarantees", "items": [
        {
          "slug": "security",
          "file": "security.md",
          "title": "Security model",
          "lead": "Deny by default, no downloaded code, quotas everywhere."
        },
        {
          "slug": "budgets",
          "file": "budgets.md",
          "title": "Budgets",
          "lead": "The numbers, and the tests that produced them."
        }
      ]}
    ]
  end
end
