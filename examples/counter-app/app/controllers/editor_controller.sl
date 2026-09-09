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

# ------------------------------------------------------------- languages
#
# A fence in a markdown document says what it holds, and this is what
# colours it. What the lexer needs to know about a language is small: the
# words it reserves, what opens a comment, what quotes a string, and what
# either of those does when it runs past the end of the line. Five answers
# is all colour needs (`ed_colour`), so five is all a language is here.
#
# Soli is Ruby-shaped — `def … end`, `#` to the end of the line, the same
# quotes — so `ruby` is this lexer with Ruby's own words added and nothing
# else changed. `sdbql` and `sql` are two dialects, not one: the first is
# SolidB's `FOR … IN … FILTER … RETURN`, which shares almost no vocabulary
# with the second.
#
# A record is {kw, line, quotes, blocks, fold}:
#   kw      the reserved words, lower case
#   line    what starts a comment that ends with the line ("" for none)
#   quotes  the characters that open and close a string
#   blocks  runs that may cross lines, flat triples: open, close, kind
#           (flat because `[["/*"` opens a Lua-style multiline string in
#           Soli, not a list of lists — the same reason `spans` is flat)
#   fold    true where the words are written in either case (SQL)
ED_RUBY_WORDS = [
  "do",
  "then",
  "yield",
  "self",
  "begin",
  "ensure",
  "raise",
  "case",
  "when",
  "require",
  "super",
  "lambda",
  "attr_accessor",
  "puts"
]

ED_C_WORDS = [
  "int", "long", "short", "char", "float", "double", "void", "bool", "auto",
  "const", "static", "struct", "class", "enum", "union", "typedef", "template",
  "namespace", "public", "private", "protected", "virtual", "override", "new",
  "delete", "if", "else", "for", "while", "do", "switch", "case", "default",
  "break", "continue", "return", "sizeof", "true", "false", "null", "nullptr",
  "this", "try", "catch", "throw", "import", "package", "final", "extends",
  "implements", "interface", "fun", "val", "var", "let", "func"
]

ED_LANGS = {
  "soli": {"kw": ED_KEYWORDS, "line": "#", "quotes": "\"'", "blocks": [], "fold": false},
  "ruby": {"kw": ED_KEYWORDS.concat(ED_RUBY_WORDS), "line": "#", "quotes": "\"'", "blocks": [], "fold": false},
  "javascript": {
    "kw": [
      "const", "let", "var", "function", "return", "if", "else", "for", "while",
      "do", "switch", "case", "break", "continue", "new", "class", "extends",
      "import", "from", "export", "default", "async", "await", "try", "catch",
      "finally", "throw", "typeof", "instanceof", "this", "null", "undefined",
      "true", "false", "yield", "delete", "in", "of", "static", "interface", "type"
    ],
    "line": "//",
    "quotes": "\"'",
    "blocks": ["/*", "*/", "comment", "`", "`", "string"],
    "fold": false
  },
  "python": {
    "kw": [
      "def", "class", "return", "if", "elif", "else", "for", "while", "in",
      "not", "and", "or", "import", "from", "as", "with", "try", "except",
      "finally", "raise", "lambda", "yield", "pass", "break", "continue",
      "global", "nonlocal", "assert", "del", "is", "async", "await", "self",
      "None", "True", "False"
    ],
    "line": "#",
    "quotes": "\"'",
    "blocks": ["\"\"\"", "\"\"\"", "string", "'''", "'''", "string"],
    "fold": false
  },
  "rust": {
    "kw": [
      "fn", "let", "mut", "const", "static", "struct", "enum", "impl", "trait",
      "for", "in", "if", "else", "match", "while", "loop", "break", "continue",
      "return", "use", "mod", "pub", "crate", "self", "super", "where", "as",
      "dyn", "ref", "move", "unsafe", "async", "await", "type", "true", "false",
      "Some", "None", "Ok", "Err"
    ],
    "line": "//",
    "quotes": "\"'",
    "blocks": ["/*", "*/", "comment"],
    "fold": false
  },
  "go": {
    "kw": [
      "func", "package", "import", "var", "const", "type", "struct", "interface",
      "map", "chan", "go", "defer", "if", "else", "for", "range", "switch",
      "case", "default", "break", "continue", "return", "select", "nil",
      "true", "false"
    ],
    "line": "//",
    "quotes": "\"'`",
    "blocks": ["/*", "*/", "comment"],
    "fold": false
  },
  "c": {"kw": ED_C_WORDS, "line": "//", "quotes": "\"'", "blocks": ["/*", "*/", "comment"], "fold": false},
  "shell": {
    "kw": [
      "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while",
      "case", "esac", "function", "return", "export", "local", "source",
      "echo", "cd", "set", "unset", "exit", "sudo"
    ],
    "line": "#",
    "quotes": "\"'",
    "blocks": [],
    "fold": false
  },
  "sdbql": {
    "kw": [
      "for", "in", "filter", "sort", "limit", "return", "let", "collect",
      "aggregate", "insert", "update", "replace", "remove", "upsert", "with",
      "into", "graph", "inbound", "outbound", "any", "distinct", "asc", "desc",
      "and", "or", "not", "null", "true", "false", "like", "options", "search"
    ],
    "line": "//",
    "quotes": "\"'",
    "blocks": ["/*", "*/", "comment"],
    "fold": true
  },
  "sql": {
    "kw": [
      "select", "from", "where", "insert", "into", "values", "update", "set",
      "delete", "join", "inner", "left", "right", "outer", "full", "on",
      "group", "by", "order", "asc", "desc", "limit", "offset", "having",
      "distinct", "as", "and", "or", "not", "null", "is", "in", "between",
      "like", "exists", "union", "all", "create", "table", "index", "view",
      "drop", "alter", "add", "primary", "key", "foreign", "references",
      "default", "case", "when", "then", "else", "end", "count", "sum", "avg",
      "min", "max", "with", "returning"
    ],
    "line": "--",
    "quotes": "\"'",
    "blocks": ["/*", "*/", "comment"],
    "fold": true
  },
  "json": {"kw": ["true", "false", "null"], "line": "", "quotes": "\"", "blocks": [], "fold": false},
  "yaml": {"kw": ["true", "false", "null", "yes", "no"], "line": "#", "quotes": "\"'", "blocks": [], "fold": false},
  "toml": {"kw": ["true", "false"], "line": "#", "quotes": "\"'", "blocks": [], "fold": false},
  "html": {"kw": [], "line": "", "quotes": "\"'", "blocks": ["<!--", "-->", "comment"], "fold": false},
  "css": {"kw": [], "line": "", "quotes": "\"'", "blocks": ["/*", "*/", "comment"], "fold": false},
  # What a fence gets when it names a language nobody here knows: no
  # keywords, because guessing them is worse than not colouring, but
  # strings and numbers, which nearly every language spells the same way.
  "other": {"kw": [], "line": "", "quotes": "\"", "blocks": [], "fold": false},
  # And what a fence gets when it says it is prose: nothing at all.
  "plain": {"kw": [], "line": "", "quotes": "", "blocks": [], "fold": false}
}

# What a fence may say for each of them. A name nobody claims falls to
# `other`: colouring Rust's words in Haskell is worse than not colouring,
# but a string is a string nearly everywhere.
ED_LANG_ALIAS = {
  "rb": "ruby",
  "js": "javascript",
  "jsx": "javascript",
  "mjs": "javascript",
  "ts": "javascript",
  "tsx": "javascript",
  "typescript": "javascript",
  "node": "javascript",
  "py": "python",
  "python3": "python",
  "rs": "rust",
  "golang": "go",
  "cpp": "c",
  "c++": "c",
  "cc": "c",
  "h": "c",
  "hpp": "c",
  "java": "c",
  "cs": "c",
  "csharp": "c",
  "kotlin": "c",
  "kt": "c",
  "swift": "c",
  "wgsl": "c",
  "glsl": "c",
  "sh": "shell",
  "bash": "shell",
  "zsh": "shell",
  "console": "shell",
  "aql": "sdbql",
  "postgres": "sql",
  "postgresql": "sql",
  "mysql": "sql",
  "sqlite": "sql",
  "yml": "yaml",
  "xml": "html",
  "svg": "html",
  "vue": "html",
  "slv": "html",
  "scss": "css",
  "sass": "css",
  "less": "css",
  "md": "plain",
  "markdown": "plain",
  "text": "plain",
  "txt": "plain",
  "diff": "plain"
}

# The record a fence's info string asks for. An empty fence is Soli: this
# is a Soli application, and its documentation is written in it.
def ed_lang(name)
  asked = (name ?? "").strip().downcase()
  return ED_LANGS["soli"] if asked == ""

  # A fence may carry more than a name — ```soli title="x" — and the name
  # is the first word of it.
  asked = asked.split(" ")[0]
  asked = ED_LANG_ALIAS[asked] ?? asked
  ED_LANGS[asked] ?? ED_LANGS["other"]
end

# ----------------------------------------------------------------- the file

def ed_source
  source = File.read(ED_FILE) rescue ""
  return ["# The editor could not read its own source.", "# " + ED_FILE] if source == ""

  source.replace("\t", "  ").split("\n")
end

def ed_defaults(state)
  base = {
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
  # Outside the loop, and only when it has to be: `ed_source` reads a file,
  # and a state that already has a buffer — the gallery's card, every render
  # after the first — must not pay for it.
  base_lines = state["lines"]
  base_lines = ed_source() if base_lines.nil?
  base["lines"] = base_lines
  base
end

def ed_line(state, at)
  state["lines"][at] ?? ""
end

# ------------------------------------------------------------- the tokeniser
# One line in, a list of {text, kind} out. Not a parser: a lexer with five
# answers, which is what colour needs — and now a language to answer for,
# since a markdown fence says what it holds.

def ed_kind_of_word(word, lang)
  words = lang["kw"]
  return "keyword" if words.includes?(word)
  return "keyword" if lang["fold"] == true && words.includes?(word.downcase())
  return "number" if ED_DIGITS.index_of(word[0]) >= 0

  "word"
end

# Where a run of punctuation has to stop: the next character that could
# begin something the lexer has an answer for.
# A bare assignment in a callee overwrites the caller's variable of that
# name, so everything here is named for this function alone: `ed_scan` is
# in the middle of a loop over `j` when it asks, and a `j` here would put
# that loop back where it started, for ever.
def ed_interesting(c, lang)
  return true if ED_WORD.index_of(c) >= 0
  return true if (lang["quotes"] ?? "").index_of(c) >= 0

  probe_mark = lang["line"] ?? ""
  return true if probe_mark != "" && c == probe_mark[0]

  probe_runs = lang["blocks"] ?? []
  probe_at = 0
  while probe_at < probe_runs.length()
    return true if c == probe_runs[probe_at][0]
    probe_at = probe_at + 3
  end
  false
end

# The block whose opener stands at the head of `rest`, as [open, close,
# kind], or nil. Longest first would matter if two openers shared a prefix;
# none of them do.

# Does `mark` stand exactly at `at`? Compared in place: asking `ed_find`
# and testing what it returned would search the rest of the line for every
# character of it, which is a lexer that takes the square of its input.
#
# Its locals are named for it alone: a bare assignment in a callee writes
# the caller's variable of that name, and `ed_find` below calls this from
# inside its own loop.
def ed_at(glyphs, mark, at)
  needle = mark.chars()
  wide = needle.length()
  return false if at + wide > glyphs.length()

  m = 0
  while m < wide
    return false if glyphs[at + m] != needle[m]
    m = m + 1
  end
  true
end

# Where `mark` next stands in `glyphs`, at or after `at`, counted in
# characters — `String.index_of` counts bytes, and this lexer counts
# characters, so it cannot use it.
def ed_find(glyphs, mark, at)
  find_at = at
  find_stop = glyphs.length() - mark.chars().length()
  while find_at <= find_stop
    return find_at if ed_at(glyphs, mark, find_at)
    find_at = find_at + 1
  end
  -1
end

# The block whose opener stands at `at`, as [open, close, kind], or nil.
def ed_block_at(blocks, glyphs, at)
  seek_at = 0
  while seek_at < blocks.length()
    seek_open = blocks[seek_at]
    return [seek_open, blocks[seek_at + 1], blocks[seek_at + 2]] if ed_at(glyphs, seek_open, at)
    seek_at = seek_at + 3
  end
  nil
end

# One line in, its tokens out — and whatever run is still open at the end
# of it, because a `/* … */` or a `""" … """` colours the lines under it
# too. `carry` is what the line before left open: {c: the marker that
# closes it, k: what it is}, or nil.
#
# One `if` chain rather than the `next` the editor's first tokeniser used,
# because `next` is rejected by the static checker outside a server (it is
# fine in a handler), and a lexer nobody can run from a script is a lexer
# nobody can test.
#
# Everything here is counted in **characters**. Soli's `length()` and
# `index_of` count bytes while `[]`, `chars()` and `substring()` count
# characters, and a lexer that mixed the two walked off the end of every
# line with an em dash in it. `code_spans` measures the tokens this hands
# back, in bytes, which is where bytes are actually wanted: the client
# colours glyphs by their byte offset.
def ed_scan(line, lang, carry)
  toks = []
  glyphs = line.chars()
  size = glyphs.length()
  i = 0

  unless carry.nil?
    close_at = ed_find(glyphs, carry["c"], 0)
    return {"toks": [{"t": line, "k": carry["k"]}], "carry": carry} if close_at < 0

    i = close_at + carry["c"].chars().length()
    toks = toks.concat([{"t": line.substring(0, i), "k": carry["k"]}])
    carry = nil
  end

  line_mark = lang["line"] ?? ""
  quotes = lang["quotes"] ?? ""
  blocks = lang["blocks"] ?? []
  while i < size
    c = glyphs[i]
    block = ed_block_at(blocks, glyphs, i)

    if line_mark != "" && ed_at(glyphs, line_mark, i)
      # A comment to the end of the line.
      toks = toks.concat([{"t": line.substring(i, size), "k": "comment"}])
      i = size
    elsif !block.nil?
      # A run that may cross lines: `/* … */`, a template string, a
      # docstring. Unclosed, it takes the rest of the line and says so.
      head = i + block[0].chars().length()
      close_at = ed_find(glyphs, block[1], head)
      if close_at < 0
        toks = toks.concat([{"t": line.substring(i, size), "k": block[2]}])
        return {"toks": toks, "carry": {"c": block[1], "k": block[2]}}
      end
      stop = close_at + block[1].chars().length()
      toks = toks.concat([{"t": line.substring(i, stop), "k": block[2]}])
      i = stop
    elsif quotes.index_of(c) >= 0
      # A string, to its closing quote or to the end of the line.
      j = i + 1
      while j < size && glyphs[j] != c
        j = j + 1
      end
      j = j + 1 if j < size
      toks = toks.concat([{"t": line.substring(i, j), "k": "string"}])
      i = j
    elsif ED_WORD.index_of(c) >= 0
      j = i
      while j < size && ED_WORD.index_of(glyphs[j]) >= 0
        j = j + 1
      end
      word = line.substring(i, j)
      toks = toks.concat([{"t": word, "k": ed_kind_of_word(word, lang)}])
      i = j
    else
      # Punctuation. The run starts at `i + 1`, not `i`: a `/` that opens
      # nothing in a language whose comment is `//` is interesting to
      # `ed_interesting`, and a run that ends where it starts never ends.
      j = i + 1
      while j < size && !ed_interesting(glyphs[j], lang)
        j = j + 1
      end
      toks = toks.concat([{"t": line.substring(i, j), "k": "plain"}])
      i = j
    end
  end
  {"toks": toks, "carry": nil}
end

# The editor's own buffer is Soli, and nothing in it crosses a line.
def ed_tokens(line)
  ed_scan(line, ed_lang("soli"), nil)["toks"]
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

def ed_top(state, visible)
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

def ed_line_row(state, at, on_click)
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
  line_row["on"] = {"click": on_click}
  line_row["p"] = {"row": at}
  keyed("l" + str(at), line_row)
end

def ed_bar(state, on_reload, clean_label)
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
      badge(state["dirty"] ? "modified" : clean_label, state["dirty"] ? "warning" : "success"),
      muted(str(state["lines"].length()) + " lines"),
      spacer(),
      muted(where),
      secondary_button("Reload", on_reload)
    ]
  )
end

# The editor as a panel, so a page that is not the editor can hold one.
# `opts` says what its host answers to — the component's own view keeps the
# short names, the gallery's card says `ed_key`, `ed_click`, `ed_reload` —
# and how many lines to show. Given a line count the panel is that tall;
# given none it fills what it is in, which is what a whole window wants.
#
# Every local here is named for this function. A bare assignment inside a
# `def` writes the caller's variable of that name, and this one is called
# from the middle of other people's views.
def ed_panel(state, opts)
  panel_click = opts["click"] ?? "click"
  panel_lines = opts["lines"]
  panel_visible = panel_lines ?? ed_visible(state)
  panel_top = ed_top(state, panel_visible)
  panel_last = panel_top + panel_visible
  panel_last = state["lines"].length() if panel_last > state["lines"].length()
  panel_rows = range(panel_top, panel_last).map(fn(at) { ed_line_row(state, at, panel_click) })
  # One box takes the keyboard for the whole buffer. A click anywhere in it
  # focuses it (03 §3), which is why the lines carry the click that moves
  # the cursor and this box carries none.
  panel_style = {
    "display": "column",
    "gap": 0,
    "width": "100%",
    "grow": 1,
    "bg": "surface.base",
    "pad": [1, 0, 1, 0]
  }
  panel_style["height"] = panel_visible * ED_ROW_H + 4 unless panel_lines.nil?
  panel_body = {
    "k": "box",
    "s": panel_style,
    "on": {"key_down": opts["key"] ?? "key"},
    "c": panel_rows
  }
  panel_frame = {"gap": 0, "width": "100%", "bg": "surface.base"}
  panel_frame["height"] = "100%" if panel_lines.nil?
  column(
    panel_frame,
    [
      ed_bar(state, opts["reload"] ?? "reload", opts["clean"] ?? "as on disk"),
      panel_body,
      ed_status(state, panel_last - panel_top)
    ]
  )
end

# What the window is doing, under the buffer: the last thing that happened,
# and how much of the file the server actually sent.
def ed_status(state, sent)
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
      muted("Soli · highlighted by the server · " + str(sent) + " of " + str(state["lines"].length())
      + " lines sent")
    ]
  )
end

def editor_view(raw_state)
  ed_panel(ed_defaults(raw_state ?? {}), {})
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
def code_spans(source, language = "soli")
  lang = ed_lang(language)
  spans = []
  offset = 0
  carry = nil
  for line in source.split("\n")
    scanned = ed_scan(line, lang, carry)
    carry = scanned["carry"]
    line_toks = scanned["toks"]
    for tok in line_toks
      span_size = tok["t"].length()
      # `plain` is what the node's own colour already is, so a span for it
      # would be several thousand triples that change nothing.
      spans = spans.concat([offset, span_size, ed_colour(tok["k"])]) unless tok["k"] == "plain"
      offset = offset + span_size
    end
    # The newline `split` removed still occupies a byte in the source the
    # client shaped, so the next line starts one further on.
    offset = offset + 1
  end
  spans
end
