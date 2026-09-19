# The documentation and the specification, served from this site rather
# than handed to GitHub.
#
# Both used to be links out: `Docs` went to a directory listing on
# github.com and `Spec` to another. A reader who wanted to know what a
# `Viewport` frame carries left the site to find out, landed in a file
# browser, and read normative text in a typeface chosen by somebody else.
# The pages are here now, in the site's own shell, with the rail that says
# what else there is.
#
# Markdown on disk, rendered per request. The `:page` segment is a *key
# into a whitelist*, never a path fragment: an unknown slug 404s before
# anything touches the filesystem, so `../../.env` is not a case to defend
# against — it is a lookup miss.
#
# The files under `www/docs/eui/` and `www/docs/spec/` are copies, written
# only by `scripts/sync-docs.sh` from `doc/docs/eui/` and `spec/`. The
# deploy rsyncs `www/` and nothing else, and neither source should move to
# suit it; the script's header has the rest, and CI checks the copies have
# not drifted.

class DocsController < Controller
  def index
    redirect("/docs/overview")
  end

  def spec_index
    redirect("/spec/00-rationale")
  end

  def show
    return this._page("docs", params["page"].to_s)
  end

  def spec
    return this._page("spec", params["page"].to_s)
  end

  # ---------------------------------------------------------------- private

  # One renderer for both, because they differ in nothing but the
  # directory and the word in the heading.
  def _page(kind, slug)
    entry = this._pages()[kind + "/" + slug]

    if entry.nil?
      return {
        "status": 404,
        "headers": {"Content-Type": "text/html; charset=utf-8"},
        "body": "<!doctype html><meta charset=utf-8><title>No such page</title>" + "<p>No page named <code>"
        + html_escape(slug)
        + "</code>. <a href=\"/docs\">Start from the overview</a>."
      }
    end

    # Which of the two the masthead should mark: a specification page is
    # reached from `Spec`, not from `Docs`, and highlighting the wrong one
    # tells the reader they are somewhere they are not.
    @here = kind
    @slug = kind + "/" + slug
    @title = entry["title"]
    @lead = entry["lead"]
    @sections = this._sections()
    @source = entry["source"]
    @html = Markdown.to_safe_html(File.read("docs/" + entry["file"]))

    render("docs/show", {"layout": "layouts/application"})
  end

  # slug -> file, title, one-line lead. Adding a page means adding a row
  # here; nothing is discovered by scanning a directory, so a stray file
  # under `docs/` is never reachable.
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
          "slug": "docs/overview",
          "file": "eui/overview.md",
          "source": "doc/docs/eui/overview.md",
          "title": "What EUI is",
          "lead": "Why an application UI does not need a document engine."
        },
        {
          "slug": "docs/status",
          "file": "eui/status.md",
          "source": "doc/docs/eui/status.md",
          "title": "What works today",
          "lead": "Built, specified, and not started — kept honest."
        }
      ]},
      {"title": "The protocol", "items": [
        {
          "slug": "docs/wire-format",
          "file": "eui/wire-format.md",
          "source": "doc/docs/eui/wire-format.md",
          "title": "Wire format",
          "lead": "Atoms, computed styles, flat subtrees, tree patches."
        },
        {
          "slug": "docs/transport",
          "file": "eui/transport.md",
          "source": "doc/docs/eui/transport.md",
          "title": "Transport",
          "lead": "Discovery, the signed manifest, and the session over HTTPS."
        },
        {
          "slug": "docs/events",
          "file": "eui/events.md",
          "source": "doc/docs/eui/events.md",
          "title": "Events",
          "lead": "What the client sends back, and what the server may believe."
        }
      ]},
      {"title": "Building", "items": [
        {
          "slug": "docs/views",
          "file": "eui/views.md",
          "source": "doc/docs/eui/views.md",
          "title": "Writing views",
          "lead": "A view is a Soli function: state in, node tree out."
        },
        {
          "slug": "docs/components",
          "file": "eui/components.md",
          "source": "doc/docs/eui/components.md",
          "title": "Components",
          "lead": "The node, the style vocabulary, and the builders."
        },
        {
          "slug": "docs/widgets",
          "file": "eui/widgets.md",
          "source": "doc/docs/eui/widgets.md",
          "title": "Widget catalogue",
          "lead": "The primitives, and the catalogue composed from them."
        },
        {
          "slug": "docs/theming",
          "file": "eui/theming.md",
          "source": "doc/docs/eui/theming.md",
          "title": "Theming",
          "lead": "Roles, scales, and why the client resolves them."
        },
        {
          "slug": "docs/clients",
          "file": "eui/clients.md",
          "source": "doc/docs/eui/clients.md",
          "title": "Servers in six languages",
          "lead": "Ruby, Python, PHP, Node, Go and Rust: what they implement."
        }
      ]},
      {"title": "Guarantees", "items": [
        {
          "slug": "docs/security",
          "file": "eui/security.md",
          "source": "doc/docs/eui/security.md",
          "title": "Security model",
          "lead": "Deny by default, no downloaded code, quotas everywhere."
        },
        {
          "slug": "docs/budgets",
          "file": "eui/budgets.md",
          "source": "doc/docs/eui/budgets.md",
          "title": "Budgets",
          "lead": "The numbers, and the tests that produced them."
        }
      ]},
      {"title": "Specification", "items": [
        {
          "slug": "spec/00-rationale",
          "file": "spec/00-rationale.md",
          "source": "spec/00-rationale.md",
          "title": "00 — Rationale",
          "lead": "What the protocol is for, and what it refuses."
        },
        {
          "slug": "spec/01-transport",
          "file": "spec/01-transport.md",
          "source": "spec/01-transport.md",
          "title": "01 — Transport",
          "lead": "Endpoints, the signed manifest, framing, recovery."
        },
        {
          "slug": "spec/02-wire-format",
          "file": "spec/02-wire-format.md",
          "source": "spec/02-wire-format.md",
          "title": "02 — Wire format",
          "lead": "Values, records, ops, and the style record."
        },
        {
          "slug": "spec/03-widgets",
          "file": "spec/03-widgets.md",
          "source": "spec/03-widgets.md",
          "title": "03 — Widgets",
          "lead": "The node kinds, and what a client must draw."
        },
        {
          "slug": "spec/04-layout",
          "file": "spec/04-layout.md",
          "source": "spec/04-layout.md",
          "title": "04 — Layout",
          "lead": "The box model, the flex rules, and measurement."
        },
        {
          "slug": "spec/05-theme",
          "file": "spec/05-theme.md",
          "source": "spec/05-theme.md",
          "title": "05 — Theme",
          "lead": "Roles, scales, modes, and the viewer's own palette."
        },
        {
          "slug": "spec/06-events",
          "file": "spec/06-events.md",
          "source": "spec/06-events.md",
          "title": "06 — Events",
          "lead": "What the client reports, and what it never does."
        },
        {
          "slug": "spec/07-bytecode",
          "file": "spec/07-bytecode.md",
          "source": "spec/07-bytecode.md",
          "title": "07 — Bytecode",
          "lead": "The local handler machine, and its limits."
        },
        {
          "slug": "spec/08-security",
          "file": "spec/08-security.md",
          "source": "spec/08-security.md",
          "title": "08 — Security",
          "lead": "Capabilities, confinement, and what is denied."
        },
        {
          "slug": "spec/09-conformance",
          "file": "spec/09-conformance.md",
          "source": "spec/09-conformance.md",
          "title": "09 — Conformance",
          "lead": "What a client must pass, and the tests that pin it."
        },
        {
          "slug": "spec/10-budgets",
          "file": "spec/10-budgets.md",
          "source": "spec/10-budgets.md",
          "title": "10 — Budgets",
          "lead": "The numbers a conforming client stays inside."
        },
        {
          "slug": "spec/11-shaders",
          "file": "spec/11-shaders.md",
          "source": "spec/11-shaders.md",
          "title": "11 — Shaders",
          "lead": "The verified subset, and why it is a capability."
        }
      ]}
    ]
  end
end
# The normative text, in its own numbering. The prose above explains
# it and this is what an implementation is measured against, so the
# two are kept apart on the rail rather than interleaved.
