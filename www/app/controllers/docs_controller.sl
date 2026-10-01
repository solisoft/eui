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

    return this._missing(kind, slug) if entry.nil?

    # Which of the two this page is. The masthead has one entry for both —
    # the specification is a section of the rail under `Docs` — and marks it
    # either way; the word is kept for whatever later tells the two apart.
    @here = kind
    @slug = kind + "/" + slug
    @title = entry["title"]
    @lead = entry["lead"]
    @sections = this._sections()
    @source = entry["source"]
    @html = this._highlight(this._live_demos(Markdown.to_safe_html(File.read("docs/" + entry["file"]))))
    # The embed's script is two megabytes behind a button, and a page with no
    # session on it does not load even the five kilobytes in front of them.
    @eui_build = @html.contains("data-eui") ? this._eui_build() : nil

    render("docs/show", {"layout": "layouts/application"})
  end

  # An address the contents do not have. It used to be one line of bare HTML
  # with no stylesheet and no way on but a link to the overview; it is a page
  # of the documentation now, still a 404, because the rail beside it lists
  # every page there is — the most useful answer a wrong address can get —
  # and above it, the pages whose address or title shares a word with the
  # one asked for, which is usually the page that was meant.
  def _missing(kind, slug)
    @here = kind
    @slug = ""
    @title = "No such page"
    @lead = nil
    @sections = this._sections()
    @source = nil
    @eui_build = nil
    near = this._near(slug)
    page = "<h1>No such page</h1><p>Nothing in the documentation or the specification is called <code>"
    + html_escape(slug)
    + "</code>. Every page there is is listed in the contents.</p>"
    unless near.length() == 0
      links = near.map do |it|
        "<li><a href=\"/" + it["slug"] + "\">" + html_escape(it["title"]) + "</a></li>"
      end
      page = page + "<p>Perhaps one of these:</p><ul>" + links.join("") + "</ul>"
    end
    @html = page
    render("docs/show", {"layout": "layouts/application"}, {"status": 404})
  end

  # The pages sharing a word of three letters or more with `slug`, in the
  # order of the contents, five at most.
  def _near(slug)
    words = slug.downcase.replace("_", "-").replace("/", "-").replace(".", "-").split("-").filter do |w|
      w.length >= 3
    end
    return [] if words.length() == 0

    this._pages().values().filter do |it|
      words.any? do |w|
        it["slug"].contains(w) || it["title"].downcase.contains(w)
      end
    end.take(5)
  end

  # A line reading `::: eui <component>` in a page's markdown becomes a live
  # session of that component beside the prose around it: the still it was
  # photographed as (`public/images/live/<component>-{light,dark}.png`), a
  # button, and a canvas the browser client draws on once asked
  # (`public/eui/eui-embed.js`). The safe renderer keeps the line as a
  # paragraph of text, so it is found as one; read anywhere else — GitHub, an
  # editor — it is what it looks like, a marker.
  #
  # The name lands in an attribute and in a URL, so it is held to a closed
  # alphabet; a line that fails it is left as the text it was.
  def _live_demos(html)
    pieces = html.split("<p>::: eui ")
    return html if pieces.length() == 1

    out = [pieces[0]]
    for piece in pieces.drop(1)
      parts = piece.split("</p>")
      words = parts[0].trim().split(" ")
      tall = words.length() > 1 ? words[1] : ""
      if parts.length() > 1 && words.length() <= 2 && this._demo_name_ok(words[0]) && this._demo_tall_ok(tall)
        out.push(this._demo_figure(words[0], tall) + parts.drop(1).join("</p>"))
      else
        out.push("<p>::: eui " + piece)
      end
    end
    out.join("")
  end

  # `::: eui <component> 380` asks for a frame at least 380 px tall, for a
  # component taller than the 3:2 the frame otherwise takes. It is a floor:
  # a wide column still gets its 3:2, and whatever is taller than either
  # scrolls inside the component, which is the component's business.
  def _demo_tall_ok(tall)
    return true if tall.blank?
    return false if tall.length > 4 || tall.chars().filter(fn(c) { "0123456789".index_of(c) < 0 }).length() > 0

    int(tall) >= 120 && int(tall) <= 1200
  end

  def _demo_name_ok(name)
    return false if name.blank? || name.length > 40

    allowed = "abcdefghijklmnopqrstuvwxyz0123456789_"
    name.chars().filter(fn(c) { allowed.index_of(c) < 0 }).length() == 0
  end

  # ------------------------------------------------------------ highlighting
  # Code blocks are coloured here, on the server, and not by a highlighter
  # in the page: the site's one script is the embed's, and a colour is not
  # worth a second. The markdown renderer marks a fenced block with its
  # language (`<pre><code class="language-soli">`) and has already escaped
  # what is inside; this unescapes it, cuts it into tokens and escapes each
  # one again inside a span. Soli and Ruby share a lexer, `sh` has a smaller
  # one, and a block in anything else — the grammars in `spec/` are untagged
  # — is left exactly as it was rendered.
  #
  # Cut with `split` rather than `index_of`: the code holds `×` and `−`, and
  # `index_of` counts bytes where `substring` counts characters.
  def _highlight(html)
    pieces = html.split("<pre><code class=\"language-")
    return html if pieces.length() == 1

    out = [pieces[0]]
    for piece in pieces.drop(1)
      head = piece.split("\">")
      parts = head.drop(1).join("\">").split("</code></pre>")
      if head.length() < 2 || parts.length() < 2
        out.push("<pre><code class=\"language-" + piece)
      else
        lang = head[0]
        out.push("<pre><code class=\"language-" + lang + "\">" + this._tokens(lang, html_unescape(parts[0])))
        out.push("</code></pre>" + parts.drop(1).join("</code></pre>"))
      end
    end
    out.join("")
  end

  def _tokens(lang, source)
    return html_escape(source) unless [
      "soli",
      "ruby",
      "sh"
    ].includes?(lang)

    shell = lang == "sh"
    cs = source.chars()
    out = []
    i = 0
    first = true
    while i < cs.length()
      cut = this._cut(cs, i, shell, first)
      text = html_escape(cs.slice(i, cut[0]).join(""))
      out.push(cut[1].blank? ? text : "<span class=\"tk-" + cut[1] + "\">" + text + "</span>")
      first = cs[i] == "\n" || (first && (cs[i] == " " || cs[i] == "\t"))
      i = cut[0]
    end
    out.join("")
  end

  # Where the token that starts at `i` ends, and what kind it is: `c`omment,
  # `s`tring, `n`umber, `k`eyword, `f`unction, `t`ype, `v`ariable, or "" for
  # a character that is none of them.
  def _cut(cs, i, shell, first)
    c = cs[i]
    spaced = i == 0 || cs[i - 1] == " " || cs[i - 1] == "\n"
    return [this._skip(cs, i + 1, "line"), "c"] if c == "#" && (!shell || spaced)
    return [this._past_string(cs, i), "s"] if c == "\"" || c == "'"
    return [this._skip(cs, i + 1, "number"), "n"] if this._digit(c) && (i == 0 || !this._word_char(cs[i - 1]))
    return [i + 1, ""] unless this._word_char(c) || c == "@" || (shell && c == "$")

    j = this._skip(cs, i + 1, "word");
    [j, this._word_kind(cs.slice(i, j).join(""), shell, first, j < cs.length() ? cs[j] : "")]
  end

  def _skip(cs, start_at, mode)
    j = start_at
    while j < cs.length() && this._keeps(mode, cs[j])
      j = j + 1
    end
    j
  end

  def _keeps(mode, c)
    return c != "\n" if mode == "line"
    return this._digit(c) || c == "." || c == "_" if mode == "number"

    this._word_char(c)
  end

  # The index just past a string that opens at `at`, or the end of its line:
  # an apostrophe in a shell line is more often prose than a quote, and a
  # string that ran on would colour the whole block.
  def _past_string(cs, at)
    quote = cs[at]
    j = at + 1
    while j < cs.length()
      if cs[j] == "\\"
        j = j + 2
      elsif cs[j] == quote
        return j + 1
      elsif cs[j] == "\n"
        return j
      else
        j = j + 1
      end
    end
    cs.length()
  end

  def _word_kind(word, shell, first, after)
    return "v" if word.starts_with?("@") || word.starts_with?("$")
    return first ? "f" : "" if shell
    return "k" if this._keywords().includes?(word)
    return "f" if after == "("
    return "t" if "ABCDEFGHIJKLMNOPQRSTUVWXYZ".index_of(word.chars()[0]) >= 0

    ""
  end

  def _keywords
    [
      "def",
      "end",
      "if",
      "elsif",
      "else",
      "unless",
      "while",
      "for",
      "in",
      "do",
      "return",
      "match",
      "fn",
      "class",
      "module",
      "nil",
      "null",
      "true",
      "false",
      "let",
      "const",
      "self",
      "this",
      "case",
      "when",
      "then",
      "begin",
      "rescue",
      "ensure",
      "yield",
      "next",
      "break",
      "and",
      "or",
      "not",
      "static",
      "private",
      "require",
      "import"
    ]
  end

  def _digit(c)
    "0123456789".index_of(c) >= 0
  end

  def _word_char(c)
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_".index_of(c) >= 0
  end

  # The markup `/demo` uses, at the size of the text rather than the window.
  def _demo_figure(name, tall)

    # Fingerprinted, because `public/` is served immutable: a still shot
    # again under the same name would otherwise never reach a reader who
    # had seen the old one.
    light = this._still("images/live/" + name + "-light.png")
    dark = this._still("images/live/" + name + "-dark.png")
    floor = tall.blank? ? "" : " style=\"min-height: " + tall + "px\""
    "<figure class=\"demo demo--inline\" data-eui data-component=\"" + name + "\" data-allow=\"\">"
    + "<div class=\"demo__stage\""
    + floor
    + ">"
    + "<img class=\"demo__poster\" src=\""
    + light
    + "\" data-light=\""
    + light
    + "\" data-dark=\""
    + dark
    + "\" width=\"1440\" height=\"960\" alt=\"The component "
    + name
    + " as the client draws it, before it is run.\" loading=\"lazy\" decoding=\"async\">"
    + "<canvas class=\"demo__canvas\" id=\"eui-"
    + name
    + "\" tabindex=\"0\" aria-label=\"A live EUI session: "
    + name
    + "\" hidden></canvas>"
    + "<button class=\"demo__run\" type=\"button\">Run it <small>&asymp;2 MB</small></button>"
    + "<p class=\"demo__note\" role=\"status\" hidden></p>"
    + "</div>"
    + "<figcaption>Live: the component <code>"
    + name
    + "</code>, running in this page.</figcaption>"
    + "</figure>"
  end

  # A file under `public/`, stamped with when it last changed. `public_path`
  # does this in a view and is not reachable from here; a still that is
  # missing goes out unstamped, and its figure says so when it fails to load.
  def _still(path)
    stamp = File.modified("public/" + path) rescue nil
    stamp.nil? ? "/" + path : "/" + path + "?v=" + str(stamp)
  end

  # Which build of the browser client is on disk, for the page to stamp on the
  # script URL. The same five lines as `HomeController#_eui_build`, which
  # serves `/demo`; change both.
  def _eui_build
    raw = slurp("public/eui/manifest.json") rescue nil
    return "none" if raw.nil?

    parsed = JSON.parse(raw) rescue nil
    parsed.nil? ? "none" : (parsed["version"] ?? "none")
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
          "slug": "docs/tutorial",
          "file": "eui/tutorial.md",
          "source": "doc/docs/eui/tutorial.md",
          "title": "Tutorial",
          "lead": "A shopping list in six steps, each one running in the page."
        },
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
          "slug": "docs/tailwind",
          "file": "eui/tailwind.md",
          "source": "doc/docs/eui/tailwind.md",
          "title": "Tailwind classes",
          "lead": "tw(): a style in Tailwind's classes, and what it refuses."
        },
        {
          "slug": "docs/clients",
          "file": "eui/clients.md",
          "source": "doc/docs/eui/clients.md",
          "title": "Servers in six languages",
          "lead": "Ruby, Python, PHP, Node, Go and Rust: what they implement."
        },
        {
          "slug": "docs/packaging",
          "file": "eui/packaging.md",
          "source": "doc/docs/eui/packaging.md",
          "title": "Phone packages",
          "lead": "eui package android|ios: an application's own APK or iOS bundle."
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
