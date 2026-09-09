# Markdown as a node tree, for showing documentation inside an application.
#
# `doc/docs/eui/widgets.md` has listed `markdown` in the Tier-1 catalogue
# since the beginning without anything behind it; this is that entry.
#
# Soli's own `Markdown.to_html` is no use here — it returns HTML, and
# turning HTML back into nodes is a worse parser than reading the markdown
# directly. So this reads the source.
#
# The one structural constraint worth knowing: a `text` node carries a
# single font family, size and weight for its whole run (02 §3), so a
# sentence with a bold word in it cannot be one node. Inline runs therefore
# become several nodes in a wrapping row.

# ---------------------------------------------------------------- inline

# The next occurrence of `mark` in `s` at or after `at`, or -1.
def md_find(s, mark, at)
  size = s.length()
  msize = mark.length()
  i = at
  while i + msize <= size
    return i if s.substring(i, i + msize) == mark
    i = i + 1
  end
  -1
end

# Does `s` begin with `prefix`? Bounds-safe, which `substring` is not: a
# real document has lines shorter than the prefix being tested for — a bare
# "}" inside a fence is one character, and asking it for its first three
# is an index error, not a false.
def md_starts(s, prefix)
  return false if s.length() < prefix.length()

  s.substring(0, prefix.length()) == prefix
end

# One inline run: the text, and what it is.
def md_run(t, kind)
  {"t": t, "k": kind}
end

# Cut a line into runs: code, strong, emphasis, links and the plain text
# between them. Unclosed markers are left as literal text rather than
# swallowing the rest of the line.
def md_runs(line)
  runs = []
  plain = ""
  i = 0
  size = line.length()
  while i < size
    c = line.substring(i, i + 1)
    two = i + 2 <= size ? line.substring(i, i + 2) : ""

    if two == "**"
      close = md_find(line, "**", i + 2)
      if close >= 0
        runs = runs.concat([md_run(plain, "plain")]) unless plain == ""
        plain = ""
        runs = runs.concat([md_run(line.substring(i + 2, close), "strong")])
        i = close + 2
        next
      end
    end

    if c == "`"
      close = md_find(line, "`", i + 1)
      if close >= 0
        runs = runs.concat([md_run(plain, "plain")]) unless plain == ""
        plain = ""
        runs = runs.concat([md_run(line.substring(i + 1, close), "code")])
        i = close + 1
        next
      end
    end

    if c == "*"
      close = md_find(line, "*", i + 1)
      if close >= 0
        runs = runs.concat([md_run(plain, "plain")]) unless plain == ""
        plain = ""
        runs = runs.concat([md_run(line.substring(i + 1, close), "em")])
        i = close + 1
        next
      end
    end

    if c == "["
      shut = md_find(line, "](", i + 1)
      if shut >= 0
        fin = md_find(line, ")", shut + 2)
        if fin >= 0
          runs = runs.concat([md_run(plain, "plain")]) unless plain == ""
          plain = ""
          runs = runs.concat([md_run(line.substring(i + 1, shut), "link")])
          i = fin + 1
          next
        end
      end
    end

    plain = plain + c
    i = i + 1
  end
  runs = runs.concat([md_run(plain, "plain")]) unless plain == ""
  runs
end

# A run as a node. `code` gets the mono face on a sunken ground; a link is
# accent-coloured but goes nowhere yet — opening a URL is a capability
# (08 §7), not a builder's decision.
def md_run_node(run, size)
  kind = run["k"]
  return text(run["t"], {"font": "mono", "size": size - 1, "bg": "surface.sunken", "fg": "accent.base", "pad": [0, 1, 0, 1], "radius": 1}) if kind == "code"
  return text(run["t"], {"size": size, "weight": "bold"}) if kind == "strong"
  return text(run["t"], {"size": size, "fg": "text.muted"}) if kind == "em"
  return text(run["t"], {"size": size, "fg": "accent.base"}) if kind == "link"

  text(run["t"], {"size": size})
end

# A line of inline markdown as a wrapping row of nodes.
#
# A plain run is split at spaces and each word given its own node. One node
# per run would be tidier, but a row wraps between its children, so a whole
# sentence would drop to the next line the moment it did not fit beside a
# bold word. Words wrap the way prose is supposed to.
def md_line(line, size)
  nodes = []
  for r in md_runs(line)
    if r["k"] == "plain"
      # A run following code or bold begins with the space that separated
      # them, and splitting on " " turns that into an empty first element.
      # Dropping it as empty is what glued "`code`" to the word after it.
      lead = md_starts(r["t"], " ") ? " " : ""
      for w in r["t"].split(" ")
        nodes = nodes.concat([text(lead + w + " ", {"size": size})]) unless w == ""
        lead = "" unless w == ""
      end
    else
      nodes = nodes.concat([md_run_node(r, size)])
    end
  end
  row({"wrap": "wrap", "align": "baseline", "gap": 0}, nodes)
end

# ---------------------------------------------------------------- blocks

def md_heading_level(line)
  return 0 unless md_starts(line, "#")

  n = 0
  while n < 6 && n < line.length() && line.substring(n, n + 1) == "#"
    n = n + 1
  end
  return 0 unless n < line.length() && line.substring(n, n + 1) == " "

  n
end

# A heading's size on the text scale, floored so `######` still outranks
# the prose around it.
def md_heading_size(level)
  size = 6 - level
  size < 2 ? 2 : size
end

def md_table_cells(line)
  trimmed = line.strip()
  trimmed = trimmed.substring(1, trimmed.length()) if md_starts(trimmed, "|")
  trimmed = trimmed.substring(0, trimmed.length() - 1) if trimmed.length() > 0 && trimmed.substring(trimmed.length() - 1, trimmed.length()) == "|"
  trimmed.split("|").map(fn(c) { c.strip() })
end

def md_table(rows)
  return column({"gap": 0}, []) if rows.length() == 0

  head = md_table_cells(rows[0])
  # An explicit share per column. `grow` alone sizes each cell to its own
  # content plus a share of what is left, so a header and its body land on
  # different edges; `basis: 0` would equalise them but makes the measure
  # pass size text against zero width — the word "Kind" then reports three
  # lines and every row is 144 px tall. A percentage does neither.
  pct = str(int(100 / head.length())) + "%"
  body = rows.length() > 2 ? range(2, rows.length()).map(fn(i) { md_table_cells(rows[i]) }) : []
  # A grid: every cell but the first in a row carries a hairline on its
  # left, every row one underneath, and the body is striped — one row in
  # two on the raised surface — so a wide table can be read across.
  header = row(
    {"gap": 0, "pad": [0, 0, 0, 0], "bg": "surface.sunken", "border": [0, 0, 1, 0], "border_color": "border.default"},
    range(0, head.length()).map(fn(i) {
      text(head[i], {"weight": "semibold", "size": 1, "width": pct, "pad": [1, 2, 1, 2], "border": [0, 0, 0, i == 0 ? 0 : 1], "border_color": "border.subtle"})
    })
  )
  # A cell wraps: it has a width now, the percentage above, so its words
  # can flow into it the way a paragraph's do. What they must not do is
  # shrink — a flex child's default — because a row that does not fit then
  # squeezes every word below its own width and each one breaks letter by
  # letter, which is what a cell full of "Ho / w ma / ny ro / ws" was. A
  # run wider than the cell overflows and is clipped instead.
  lines = range(0, body.length()).map(fn(r) {
    cells = body[r]
    row(
      {
        "gap": 0,
        "pad": [0, 0, 0, 0],
        "bg": r % 2 == 1 ? "surface.raised" : "none",
        "border": [0, 0, 1, 0],
        "border_color": "border.subtle"
      },
      range(0, cells.length()).map(fn(i) {
        cell = md_line(cells[i], 1)
        cell["s"] = cell["s"].merge({
          "width": pct,
          "overflow": "clip",
          "pad": [1, 2, 1, 2],
          "border": [0, 0, 0, i == 0 ? 0 : 1],
          "border_color": "border.subtle"
        })
        cell["c"] = cell["c"].map(fn(run) {
          run["s"] = run["s"].merge({"shrink": 0})
          run
        })
        cell
      })
    )
  })
  column({"gap": 0, "border": 1, "border_color": "border.default", "radius": 2, "overflow": "clip"}, [header].concat(lines))
end

def md_quote(lines)
  {
    "k": "box",
    "s": {
      "display": "column",
      "gap": 1,
      "pad": [2, 3, 2, 3],
      "bg": "surface.sunken",
      "border": [0, 0, 0, 3],
      "border_color": "accent.base",
      "radius": 1
    },
    "c": lines.map(fn(l) { md_line(l, 2) })
  }
end

def md_list(items, ordered)
  column({"gap": 1}, range(0, items.length()).map(fn(i) {
    marker = ordered ? str(i + 1) + "." : "•"
    row({"gap": 2, "align": "start"}, [
      text(marker, {"fg": "text.muted", "size": 2, "width": ordered ? 24 : 14}),
      {"k": "box", "s": {"display": "column", "grow": 1}, "c": [md_line(items[i], 2)]}
    ])
  }))
end

# How many characters an ordered-list marker takes ("12. " -> 4), or 0.
def md_ordered_marker(line)
  i = 0
  size = line.length()
  while i < size && "0123456789".includes?(line.substring(i, i + 1))
    i = i + 1
  end
  return 0 if i == 0
  return 0 unless i + 2 <= line.length() && line.substring(i, i + 2) == ". "

  i + 2
end

# The block walk. Markdown is line-oriented at this level: a line either
# starts a block or continues the one before it.
def md_blocks(source)
  out = []
  lines = source.split("\n")
  n = lines.length()
  i = 0
  while i < n
    line = lines[i]
    trimmed = line.strip()

    if trimmed == ""
      i = i + 1
      next
    end

    # A fence runs to its closing fence, or to the end of the document if
    # the author forgot one. It goes through the code viewer, which now
    # sizes itself to the lines it was given.
    if md_starts(trimmed, "```")
      lang = trimmed.substring(3, trimmed.length()).strip()
      body = []
      i = i + 1
      while i < n && !md_starts(lines[i].strip(), "```")
        body = body.concat([lines[i]])
        i = i + 1
      end
      i = i + 1
      code = body.join("\n")
      out = out.concat([code_viewer(code, {
        "line_numbers": false,
        "spans": code_spans(code, lang),
        "language": lang
      })])
      next
    end

    level = md_heading_level(line)
    if level > 0
      size = md_heading_size(level)
      # `piece`, not `node`: a bare assignment to a name that is also a
      # top-level def rebinds that def for the whole process, and `node` is
      # the builder every `row` and `column` calls.
      heads = md_runs(line.substring(level + 1, line.length())).map(fn(r) {
        piece = md_run_node(r, size)
        piece["s"] = piece["s"].merge({"weight": "bold"})
        piece
      })
      out = out.concat([row({"wrap": "wrap", "gap": 0, "pad": [level == 1 ? 2 : 1, 0, 0, 0]}, heads)])
      i = i + 1
      next
    end

    if trimmed == "---" || trimmed == "***" || trimmed == "___"
      out = out.concat([divider()])
      i = i + 1
      next
    end

    if md_starts(trimmed, "> ")
      body = []
      while i < n && md_starts(lines[i].strip(), ">")
        body = body.concat([lines[i].strip().substring(1, lines[i].strip().length()).strip()])
        i = i + 1
      end
      out = out.concat([md_quote(body)])
      next
    end

    if md_starts(trimmed, "|")
      rows = []
      while i < n && md_starts(lines[i].strip(), "|")
        rows = rows.concat([lines[i]])
        i = i + 1
      end
      out = out.concat([md_table(rows)])
      next
    end

    if md_starts(trimmed, "- ") || md_starts(trimmed, "* ")
      items = []
      while i < n && (md_starts(lines[i].strip(), "- ") || md_starts(lines[i].strip(), "* "))
        items = items.concat([lines[i].strip().substring(2, lines[i].strip().length())])
        i = i + 1
      end
      out = out.concat([md_list(items, false)])
      next
    end

    if md_ordered_marker(trimmed) > 0
      items = []
      while i < n && md_ordered_marker(lines[i].strip()) > 0
        cut = md_ordered_marker(lines[i].strip())
        items = items.concat([lines[i].strip().substring(cut, lines[i].strip().length())])
        i = i + 1
      end
      out = out.concat([md_list(items, true)])
      next
    end

    # Anything else is a paragraph: consecutive non-blank lines that start
    # no other block, joined, because a hard wrap in the source is not a
    # line break in the prose.
    para = []
    while i < n && lines[i].strip() != "" && md_heading_level(lines[i]) == 0 && !md_starts(lines[i].strip(), ">") && !md_starts(lines[i].strip(), "|") && !md_starts(lines[i].strip(), "```") && !md_starts(lines[i].strip(), "- ")
      para = para.concat([lines[i].strip()])
      i = i + 1
    end
    out = out.concat([md_line(para.join(" "), 2)]) if para.length() > 0
    i = i + 1 if para.length() == 0
  end
  out
end

# A document read from disk, parsed once.
#
# The cache is keyed on the path because the path is the whole input: the
# same file gives the same nodes every time. This is the memoisation `lazy`
# deliberately does not do — `lazy` cannot know what a subtree reads, but a
# document knows it reads one file.
#
# `File.read` is jailed to the application root, so the path is relative to
# it and must live inside the app: `../..` is refused, and so is a symlink.
# A `.md` under the app is carried into a packaged artifact verbatim
# (`lang/src/bundle.rs`), and the artifact serves from its extraction dir,
# so this same path works packaged and unpackaged.
MD_DOCS = {}

def markdown_file(path, opts)
  cached = MD_DOCS[path]
  return cached unless cached.nil?

  built = markdown(File.read(path), opts)
  MD_DOCS[path] = built
  built
end

# A markdown document as rows for a windowed `list` (04 §7.1), so a long
# page costs the client one window of blocks rather than the page: parsed
# once per path and cached, with a height guessed for every block. The
# guess sets the scroll extent and the placeholders until a row arrives; it
# errs tall, because a row taller than its slot is clipped and a row shorter
# than it leaves air, and air is the lesser fault.
MD_DOC_ROWS = {}

def md_doc_rows(path, width)
  cached = MD_DOC_ROWS[path]
  return cached unless cached.nil?

  blocks = md_blocks(File.read(path))
  built = {"rows": blocks, "heights": blocks.map(fn(b) { md_guess_height(b, width) })}
  MD_DOC_ROWS[path] = built
  built
end

# How tall a block will probably be at `width`, from its nodes alone: text
# is 8 px a character and a line of its size tall, a wrapping row of words
# is the characters it holds divided into lines, a code viewer is 18 px a
# line, and a column is its children stacked with their gap. Not a layout —
# a guess a layout replaces — and one that errs tall on purpose: a row
# taller than its slot overlaps the next, a row shorter leaves air.
#
# Its locals are named for it alone: a bare assignment in a callee writes
# the caller's variable of that name, and this recurses.
MD_LINE_PX = [16, 18, 22, 24, 28, 32, 38, 46]
MD_SPACE_PX = [0, 2, 4, 8, 12, 16, 20, 24, 32, 40, 48, 64, 96]

# A `pad` is one space index for every side or a list of four; either way,
# the px of side `side` (0 top, 2 bottom).
def md_pad_px(pad, side)
  return 0 if pad.nil?
  return MD_SPACE_PX[pad] ?? 0 if pad.class() == "int"

  MD_SPACE_PX[pad[side] ?? 0] ?? 0
end

def md_line_px(style)
  MD_LINE_PX[(style ?? {})["size"] ?? 2] ?? 22
end

def md_guess_height(node, width)
  guess_kind = node["k"] ?? "box"
  guess_style = node["s"] ?? {}
  return md_guess_text(node["t"] ?? "", guess_style, width) if guess_kind == "text"
  return 1 if guess_kind == "divider"

  guess_kids = node["c"] ?? []
  return 0 if guess_kids.length() == 0

  guess_pad = md_pad_px(guess_style["pad"], 0) + md_pad_px(guess_style["pad"], 2)
  if guess_style["display"] == "row" && guess_style["wrap"] == "wrap"
    guess_chars = 0
    guess_line = 22
    for kid in guess_kids
      guess_chars = guess_chars + (kid["t"] ?? "").length()
      guess_line = md_line_px(kid["s"]) if md_line_px(kid["s"]) > guess_line
    end
    return guess_pad + guess_line * (int(guess_chars * 8 / width) + 1)
  end
  # The children first, as a list, and only then a fold over it: this
  # recurses, and a recursive call writes the same names — an accumulator
  # kept across the calls would be reset by each of them (the table under
  # "Key in `event_data`" came out 14 px short that way).
  guess_each = guess_kids.map(fn(k) { md_guess_height(k, width) })
  if guess_style["display"] == "row"
    guess_tallest = 0
    for guess_h in guess_each
      guess_tallest = guess_h if guess_h > guess_tallest
    end
    return guess_pad + guess_tallest + 4
  end
  guess_gap = MD_SPACE_PX[guess_style["gap"] ?? 0] ?? 0
  guess_pad + int(guess_each.sum()) + guess_gap * (guess_kids.length() - 1) + 8
end

def md_guess_text(content, style, width)
  guess_rows = content.split("\n")
  guess_line = (style ?? {})["font"] == "mono" ? 18 : md_line_px(style)
  guess_lines = 0
  for guess_row in guess_rows
    guess_lines = guess_lines + int(guess_row.length() * 8 / width) + 1
  end
  guess_line * guess_lines
end

# A markdown document as a node. `opts` may carry {"gap": n}.
def markdown(source, opts)
  opts = opts ?? {}
  column({"gap": opts["gap"] ?? 3}, md_blocks(source))
end
