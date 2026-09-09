# A code editor, in a window that has never heard of code.
#
# The buffer is an array of lines in the session's state. The keyboard
# arrives as `key_down` events on one box — the same arrangement the
# tracker uses for its pattern — and every keystroke is a round trip that
# comes back as a patch: a line that changed, a cursor that moved. Nothing
# here is a text widget. The client has `input` and `textarea` for a field
# whose caret belongs to the window (03 §3), and they are the wrong shape
# for an editor: an editor wants the gutter, the highlighting and the
# selection to be one thing, and that thing is the application's.
#
# The highlighting is a Soli function. `ed_tokens` cuts a line into
# keywords, strings, comments, numbers and the rest, and the view gives
# each token its own `text` node with a role for a colour — so the same
# buffer is olive on one desktop and blue on another, and the client is
# not asked to know what a keyword is. A text node can also carry a
# `spans` prop and colour itself per glyph; per-token nodes cost a few
# hundred more nodes for a window of code and need nothing but what the
# protocol already has.
#
# What it opens is its own source. Nothing is ever written back: the
# status line says so, and there is no route that could.

ED_FILE = "app/controllers/editor_controller.sl"
ED_ROW_H = 18
ED_GUTTER_W = 52
# How many lines the window shows. Like the tracker's pattern, the buffer
# is windowed around the cursor: the server sends the lines you can see.
ED_VISIBLE = 26

ED_KEYWORDS = [
  "def",
  "end",
  "if",
  "else",
  "elsif",
  "unless",
  "while",
  "for",
  "in",
  "return",
  "next",
  "break",
  "true",
  "false",
  "nil",
  "fn",
  "class",
  "module",
  "include",
  "match",
  "rescue",
  "print",
  "and",
  "or",
  "not"
]

ED_DIGITS = "0123456789"
# Enough spaces to carry any indentation this file has; Soli strings have
# no `repeat`, and a substring of a constant is cheaper than a loop.
ED_INDENT = "                                                                "
ED_WORD = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789?!"

# ----------------------------------------------------------------- the file

def ed_source
  source = File.read(ED_FILE) rescue ""
  return ["# The editor could not read its own source.", "# " + ED_FILE] if source == ""

  source.replace("\t", "  ").split("\n")
end

def ed_defaults(state)
  base = {
    "lines": ed_source(),
    "row": 0,
    "col": 0,
    "name": ED_FILE,
    "dirty": false,
    "message": "Opened its own source. Nothing is written back.",
    "viewport": {
      "width": 1280,
      "height": 800,
      "scale": 1.0,
      "mode": "dark",
      "density": "cozy",
      "font_scale": 1.0
    }
  }
  for key in base.keys()
    base[key] = state[key] unless state[key].nil?
  end
  base
end

def ed_line(state, at)
  state["lines"][at] ?? ""
end

# ------------------------------------------------------------- the tokeniser
# One line in, a list of {text, kind} out. Not a parser: a lexer with five
# answers, which is what colour needs.

def ed_kind_of_word(word)
  return "keyword" if ED_KEYWORDS.includes?(word)
  return "number" if ED_DIGITS.index_of(word[0]) >= 0

  "word"
end

def ed_tokens(line)
  tokens = []
  i = 0
  size = line.length()
  while i < size
    c = line[i]
    if c == "#"
      tokens = tokens.concat([{"t": line.substring(i, size), "k": "comment"}])
      i = size
      next
    end
    if c == "\""
      j = i + 1
      while j < size && line[j] != "\""
        j = j + 1
      end
      j = j + 1 if j < size
      tokens = tokens.concat([{"t": line.substring(i, j), "k": "string"}])
      i = j
      next
    end
    if ED_WORD.index_of(c) >= 0
      j = i
      while j < size && ED_WORD.index_of(line[j]) >= 0
        j = j + 1
      end
      word = line.substring(i, j)
      tokens = tokens.concat([{"t": word, "k": ed_kind_of_word(word)}])
      i = j
      next
    end
    j = i
    while j < size && ED_WORD.index_of(line[j]) < 0 && line[j] != "#" && line[j] != "\""
      j = j + 1
    end
    tokens = tokens.concat([{"t": line.substring(i, j), "k": "plain"}])
    i = j
  end
  tokens
end

def ed_colour(kind)
  return "accent.base" if kind == "keyword"
  return "success.base" if kind == "string"
  return "text.disabled" if kind == "comment"
  return "warning.base" if kind == "number"
  return "text.muted" if kind == "plain"

  "text.default"
end

# ------------------------------------------------------------------ editing

def ed_clamp(state)
  count = state["lines"].length()
  at = state["row"]
  at = 0 if at < 0
  at = count - 1 if at > count - 1
  state["row"] = at
  size = ed_line(state, at).length()
  col = state["col"]
  col = 0 if col < 0
  col = size if col > size
  state["col"] = col
  state
end

# `chunk`, not `text`: a parameter that shadows a builder rebinds it for
# the whole worker, and `text()` is the one thing every view calls.
def ed_put(state, chunk)
  at = state["row"]
  line = ed_line(state, at)
  col = state["col"]
  state["lines"][at] = line.substring(0, col) + chunk + line.substring(col, line.length())
  state["col"] = col + chunk.length()
  state["dirty"] = true
  state
end

def ed_split(state)
  at = state["row"]
  line = ed_line(state, at)
  col = state["col"]
  # The new line keeps the old one's indentation: an editor that forgets
  # that is an editor nobody types in twice.
  head = line.substring(0, col)
  tail = line.substring(col, line.length())
  spaces = 0
  while spaces < head.length() && head[spaces] == " "
    spaces = spaces + 1
  end
  before = state["lines"].slice(0, at).concat([head])
  after = state["lines"].slice(at + 1, state["lines"].length())
  state["lines"] = before.concat([ED_INDENT.substring(0, spaces) + tail], after)
  state["row"] = at + 1
  state["col"] = spaces
  state["dirty"] = true
  state
end

def ed_backspace(state)
  at = state["row"]
  col = state["col"]
  line = ed_line(state, at)
  if col > 0
    state["lines"][at] = line.substring(0, col - 1) + line.substring(col, line.length())
    state["col"] = col - 1
    state["dirty"] = true
    return state
  end
  return state if at == 0

  above = ed_line(state, at - 1)
  before = state["lines"].slice(0, at - 1)
  after = state["lines"].slice(at + 1, state["lines"].length())
  state["lines"] = before.concat([above + line], after)
  state["row"] = at - 1
  state["col"] = above.length()
  state["dirty"] = true
  state
end

def ed_delete(state)
  at = state["row"]
  col = state["col"]
  line = ed_line(state, at)
  if col < line.length()
    state["lines"][at] = line.substring(0, col) + line.substring(col + 1, line.length())
    state["dirty"] = true
    return state
  end
  return state if at >= state["lines"].length() - 1

  below = ed_line(state, at + 1)
  before = state["lines"].slice(0, at)
  after = state["lines"].slice(at + 2, state["lines"].length())
  state["lines"] = before.concat([line + below], after)
  state["dirty"] = true
  state
end

def ed_move(state, drow, dcol)
  state["row"] = state["row"] + drow
  state["col"] = state["col"] + dcol
  ed_clamp(state)
end

# The keys an editor answers to. A printable key arrives as itself — one
# character — and everything longer is a name.
def ed_key(state, key, mods)
  return ed_move(state, -1, 0) if key == "ArrowUp"
  return ed_move(state, 1, 0) if key == "ArrowDown"
  return ed_move(state, 0, -1) if key == "ArrowLeft"
  return ed_move(state, 0, 1) if key == "ArrowRight"
  return ed_move(state, -ED_VISIBLE, 0) if key == "PageUp"
  return ed_move(state, ED_VISIBLE, 0) if key == "PageDown"
  return set_key(state, "col", 0) if key == "Home"
  return set_key(state, "col", ed_line(state, state["row"]).length()) if key == "End"
  return ed_split(state) if key == "Enter"
  return ed_backspace(state) if key == "Backspace"
  return ed_delete(state) if key == "Delete"
  return ed_put(state, "  ") if key == "Tab"
  return ed_put(state, key) if key.length() == 1

  state
end

# A click puts the cursor on the line that was clicked. The column comes
# from the pointer's x over the width of a character in the mono face,
# which the view knows because it asked for that size.
def ed_click(state, params)
  point = params["payload"] ?? [0, 0]
  props = params["props"] ?? {}
  at = props["row"] ?? state["row"]
  state["row"] = at
  state["col"] = int((point[0] - ED_GUTTER_W) / 8)
  ed_clamp(state)
end

def ed_reload(state)
  state["lines"] = ed_source()
  state["row"] = 0
  state["col"] = 0
  state["dirty"] = false
  state["message"] = "Read again from disk. Anything typed is gone."
  state
end

def editor(event_data)
  event = event_data["event"]
  params = event_data["params"]
  state = ed_defaults(event_data["state"] ?? {})
  match event {
    "connect" => set_key(state, "viewport", params["viewport"] ?? state["viewport"]),
    "viewport" => set_key(state, "viewport", params["viewport"] ?? state["viewport"]),
    "key" => ed_key(state, params["payload"][0], params["payload"][1]),
    "click" => ed_click(state, params),
    "reload" => ed_reload(state),
    _ => state,
  }
end

# --------------------------------------------------------------------- view

def ed_mono(content, colour)
  text(
    content,
    {
      "font": "mono",
      "size": 1,
      "fg": colour
    }
  )
end

def ed_visible(state)
  height = (state["viewport"] ?? {})["height"] ?? 800
  fits = int((height - 150) / ED_ROW_H)
  fits = 6 if fits < 6
  fits
end

def ed_top(state)
  visible = ed_visible(state)
  count = state["lines"].length()
  top = state["row"] - int(visible / 2)
  top = 0 if top < 0
  top = count - visible if top > count - visible
  top = 0 if top < 0
  top
end

# The line under the cursor is drawn plain, in three pieces, so the caret
# can invert the character it sits on. Every other line is coloured by the
# tokeniser.
def ed_cursor_line(line, col)
  at = col < line.length() ? line.substring(col, col + 1) : " "
  row(
    {"gap": 0, "align": "center"},
    [
      ed_mono(line.substring(0, col), "text.default"),
      {
        "k": "box",
        "s": {"bg": "accent.base"},
        "c": [ed_mono(at, "accent.on")]
      },
      ed_mono(line.substring(col + 1, line.length()), "text.default")
    ]
  )
end

def ed_line_row(state, at)
  here = state["row"] == at
  line = ed_line(state, at)
  body = here ? ed_cursor_line(line, state["col"]) : row(
    {"gap": 0, "align": "center"},
    ed_tokens(line).map(fn(token) { ed_mono(token["t"], ed_colour(token["k"])) })
  )
  line_row = row(
    {
      "gap": 0,
      "height": ED_ROW_H,
      "align": "center",
      "width": "100%",
      "bg": here ? "surface.raised" : "none",
      "cursor": "text"
    },
    [
      {
        "k": "box",
        "s": {
          "width": ED_GUTTER_W,
          "display": "row",
          "justify": "end",
          "pad": [0, 2, 0, 0],
          "shrink": 0
        },
        "c": [ed_mono(str(at + 1), here ? "text.default" : "text.disabled")]
      },
      body
    ]
  )
  line_row["on"] = {"click": "click"}
  line_row["p"] = {"row": at}
  keyed("l" + str(at), line_row)
end

def ed_bar(state)
  where = "Ln " + str(state["row"] + 1) + ", Col " + str(state["col"] + 1)
  row(
    {
      "gap": 3,
      "align": "center",
      "width": "100%",
      "pad": [2, 3, 2, 3],
      "bg": "surface.raised",
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    [
      text(state["name"], {"weight": "semibold"}),
      badge(state["dirty"] ? "modified" : "as on disk", state["dirty"] ? "warning" : "success"),
      muted(str(state["lines"].length()) + " lines"),
      spacer(),
      muted(where),
      secondary_button("Reload", "reload")
    ]
  )
end

def editor_view(raw_state)
  state = ed_defaults(raw_state ?? {})
  visible = ed_visible(state)
  top = ed_top(state)
  last = top + visible
  last = state["lines"].length() if last > state["lines"].length()
  lines = range(top, last).map(fn(at) { ed_line_row(state, at) })
  # One box takes the keyboard for the whole buffer. A click anywhere in it
  # focuses it (03 §3), which is why the lines carry the click that moves
  # the cursor and this box carries none.
  page = {
    "k": "box",
    "s": {
      "display": "column",
      "gap": 0,
      "width": "100%",
      "grow": 1,
      "bg": "surface.base",
      "pad": [1, 0, 1, 0]
    },
    "on": {"key_down": "key"},
    "c": lines
  }
  column(
    {
      "gap": 0,
      "width": "100%",
      "height": "100%",
      "bg": "surface.base"
    },
    [
      ed_bar(state),
      page,
      row(
        {
          "gap": 3,
          "align": "center",
          "width": "100%",
          "pad": [1, 3, 1, 3],
          "bg": "surface.raised",
          "border": [1, 0, 0, 0],
          "border_color": "border.subtle"
        },
        [
          muted(state["message"]),
          spacer(),
          muted("Soli · highlighted by the server · " + str(visible) + " of " + str(state["lines"].length())
          + " lines sent")
        ]
      )
    ]
  )
end

# ---------------------------------------------------------- spans for a viewer
# The editor gives every token its own `text` node. A read-only viewer wants
# the opposite — one node, so its lines stay locked to a gutter beside it —
# so the same tokeniser is rendered instead as a `spans` prop: flat triples
# of [start byte, length, colour] over the whole source, which the client
# reads to colour glyph by glyph.
#
# Offsets are counted in characters, which is the same as bytes only while
# the source is ASCII. Code that is not would need `bytesize` per token.
def code_spans(source)
  spans = []
  at = 0
  for line in source.split("\n")
    for tok in ed_tokens(line)
      size = tok["t"].length()
      # `plain` is what the node's own colour already is, so a span for it
      # would be several thousand triples that change nothing.
      spans = spans.concat([at, size, ed_colour(tok["k"])]) unless tok["k"] == "plain"
      at = at + size
    end
    # The newline `split` removed still occupies a byte in the source the
    # client shaped, so the next line starts one further on.
    at = at + 1
  end
  spans
end
