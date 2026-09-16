# EUI, presented in EUI.
#
# A landing page is the one kind of document a protocol like this is never
# asked to draw, which is the reason to draw one: everything here is the
# same eight-key node hash and the same flat style record an ERP row is
# made of, and the whole page is one component, one handler and one view.
#
# The design brief came out of a constraint rather than around it. The text
# scale tops out at index 7 -- 38 px at font scale 1.0 (spec 05 §2) -- and
# it is not themeable, so there is no seventy-two-pixel hero to be had. A
# scale the viewer can resize is worth more than a hero, so this page is set
# the way a type specimen is: a hard measure, real rules, four display lines
# in the whole document, and the protocol's own bytes as the opening image.
#
# Every colour here is a *role*. Nothing says `#RRGGBB`, so the page is
# right in the viewer's dark mode without this server ever learning they
# went there -- which is one of the things the page is claiming.

# ---------------------------------------------------------------- the faces
#
# `eui_font` is idempotent: it finds the family it already bound and keeps
# its role, so calling it per render costs a lookup and never a re-upload.
# The declaration that matters is the one in `config/routes.sl`, at boot,
# before the manifest is signed.
#
# Both answer `"sans"` if the faces are not on disk, so a checkout without
# them draws the page in the client's own Inter rather than naming a font
# nothing declared.

def site_display()
  eui_font("Playfair Display", [
    "public/fonts/playfair-display-400.ttf",
    "public/fonts/playfair-display-700.ttf"
  ]) rescue "sans"
end

def site_body()
  eui_font("Space Grotesk", [
    "public/fonts/space-grotesk-400.ttf",
    "public/fonts/space-grotesk-700.ttf"
  ]) rescue "sans"
end

# ------------------------------------------------------------- the elements
#
# Nine of them, and the page is built from nothing else. A landing page does
# not need a widget catalogue; it needs a voice, and a voice is a small set
# of elements used consistently.

def node(kind, style, children)
  {"k": kind, "s": style, "c": children}
end

def column(style, children)
  style["display"] = "column"
  node("box", style, children)
end

def row(style, children)
  style["display"] = "row"
  node("box", style, children)
end

def text(content, style)
  {"k": "text", "t": content, "s": style}
end

# A thesis. Four of these in the document, and they carry it.
def display(content)
  text(content, {
    "font": site_display(),
    "weight": "bold",
    "size": 7,
    "fg": "text.default",
    "width": "100%"
  })
end

# The line under a thesis: larger than prose, quieter than a heading.
def lede(content)
  text(content, {"font": site_body(), "size": 3, "fg": "text.muted", "width": "100%"})
end

def prose(content)
  text(content, {"font": site_body(), "size": 2, "fg": "text.default", "width": "100%"})
end

# Anything the protocol itself would write: a byte, a count, a field name.
# Mono here is notation and not decoration -- these are the subject's own
# characters, and setting them in the body face would be a translation.
def figure(content, tone)
  text(content, {"font": "mono", "size": 1, "fg": tone ?? "text.muted"})
end

# An address you can click, now that there is something for a click to do.
#
# `open` is a prop, not an op: the server cannot open anything: it can only
# put an address on a node and wait. The client checks the scheme, checks
# the host, tells the person where they are going, and hands it to the
# platform -- and then tells this server nothing at all, not that it opened
# and not that it failed. So the underline is honest now: it promises a
# browser, and a browser is what the person gets.
#
# What it costs is in 08 §8 and worth knowing: a server can put a token in
# the address, and the browser then arrives carrying cookies and an address
# of its own. That links a session to a web identity, and no allowlist
# closes it, because a server is always allowed to link to itself.
def site_link_style(lit)
  {
    "font": "mono", "size": 1, "fg": "accent.base",
    "underline": lit, "cursor": "pointer", "transition": "fast"
  }
end

def site_address(label)
  {
    "k": "text",
    "t": label,
    "s": site_link_style(false),
    "p": {"open": "https://" + label, "role": "link", "label": "Open " + label},
    "on": {
      "pointer_enter": {"local": "self.style = @lit", "styles": {"lit": site_link_style(true)}},
      "pointer_leave": {"local": "self.style = @rest", "styles": {"rest": site_link_style(false)}}
    }
  }
end

def rule()
  {"k": "divider", "s": {"height": 1, "bg": "border.subtle", "width": "100%"}}
end

def keyed(key, n)
  n["key"] = key
  n
end

def with_state(st, root)
  root["p"] = (root["p"] ?? {}).merge(st)
  root
end

# ------------------------------------------------------------ the layout
#
# Every width on this page is a number derived from the viewport, and the
# viewport arrives on its own: the session posts `connect` with it and a
# `viewport` event on every resize, so nothing has to ask.
#
# Numbers rather than percentages, and deliberately. `max_width` does not
# constrain measurement in this engine -- a node with no specified `width`
# is measured against a loosened cross constraint
# (`crates/eui-layout/src/engine.rs:617-621`), so text that is going to
# wrap is measured as though it will not and a two-line heading is laid
# out one line tall. A computed width is exact, so it both wraps correctly
# and follows the window. The flex main axis is sound, so anywhere two
# things share a row they share it with `grow` and no width at all.

SITE_BP = {"sm": 640, "md": 900, "lg": 1240}

def site_layout(state)
  view = state["viewport"] ?? {}
  w = view["width"] ?? 1280
  pad = w >= SITE_BP["md"] ? 8 : 5
  gutter = w >= SITE_BP["md"] ? 64 : 32
  content = w - gutter
  content = 1120 if content > 1120
  content = 280 if content < 280
  {
    "w": w,
    "pad": pad,
    "content": content,
    # The measure: never wider than is readable, never wider than there is.
    "measure": content > 680 ? 680 : content,
    "wide": w >= SITE_BP["md"],
    "roomy": w >= SITE_BP["sm"],
    "huge": w >= SITE_BP["lg"]
  }
end

# Two things side by side when there is room, stacked when there is not.
# `grow` rather than a width: the main axis of a flex row is the one this
# engine sizes correctly, so a split follows the window for free.
def split(lay, gap, left, right)
  return column({"gap": gap, "width": lay["content"]}, [left, right]) if !lay["wide"]
  row({"gap": gap, "width": lay["content"], "align": "start"}, [
    column({"grow": 1, "shrink": 1, "gap": 6}, [left]),
    column({"grow": 1, "shrink": 1, "gap": 6}, [right])
  ])
end

def measure(lay, children)
  column({"width": lay["measure"], "gap": 6}, children)
end

# Blocks arrive rather than appear.
#
# `enter` is a style byte, not a keyframe: a node grafted wearing it comes
# up from nothing over its `transition`, along the decelerate curve, and
# `motion` says which way. `bottom` starts it its *own height* below where
# it belongs -- measured, not named, so a short block barely moves and a
# tall one travels in proportion (03 §5.2).
#
# The whole effect is three style fields. There is no timeline, no library
# and no script: the client interpolates from the draw list's own age, so
# a frame owed to an entrance is the previous list drawn again.
def site_enter(style)
  style.merge({"animation": ["enter"], "motion": "bottom", "transition": "slowest"})
end

def section(lay, number, title, children)
  column(site_enter({"gap": 6, "width": lay["content"], "pad": [0, 0, lay["wide"] ? 8 : 6, 0]}), [
    row({"gap": 4, "align": "center"}, [
      figure(number, "accent.base"),
      text(title, {"font": site_body(), "size": 0, "weight": "bold", "fg": "text.muted"})
    ]),
    rule()
  ].concat(children))
end

# --------------------------------------------------------- the live frame
#
# The opening image, and the only honest way to make the argument: press
# the plus and the number changes *in the frame you pressed it*, because a
# chunk of verified bytecode ran on this machine and repainted the two text
# nodes it was allowed to touch. The server hears about it afterwards and
# confirms. Nothing here waits for a round trip, and nothing here is
# trusted either -- the authoritative quantity is the one that comes back.
#
# That is what the twenty-two bytes above the line are: one `set_text`, one
# node, one string. The counter under it adds them up as you press.

SITE_UNIT = 102

# hex, what it is, the role that says which kind it is, and how many bytes
# it takes -- the last one is what the ruler underneath draws to scale.
SITE_FRAME = [
  ["03", "frame: batch", "info.base", 1],
  ["14", "length 20", "warning.base", 1],
  ["02", "seq 2", "success.base", 1],
  ["01", "1 op", "warning.base", 1],
  ["23", "set_text", "danger.base", 1],
  ["8d 01", "node 141", "success.base", 2],
  ["01", "inline text", "danger.base", 1],
  ["0d", "13 bytes", "warning.base", 1],
  ["31 20 39 38 34 2c 20 34 32 20 e2 82 ac", "the new total", "text.default", 13]
]

def site_field_style(lit)
  {
    "display": "column", "gap": 2, "shrink": 0,
    "pad": [1, 2, 1, 2], "radius": 1,
    "bg": lit ? "surface.sunken" : "none",
    "transition": "fast"
  }
end

def site_field(field)
  {
    "k": "box",
    "s": site_field_style(false),
    "on": {
      "pointer_enter": {"local": "self.style = @lit", "styles": {"lit": site_field_style(true)}},
      "pointer_leave": {"local": "self.style = @rest", "styles": {"rest": site_field_style(false)}}
    },
    "c": [
      text(field[0], {"font": "mono", "size": 1, "weight": "bold", "fg": field[2]}),
      text(field[1], {"font": site_body(), "size": 0, "fg": "text.muted"})
    ]
  }
end

# The same twenty-two bytes, to scale.
#
# A canvas: a list of filled rectangles and nothing else, drawn by the one
# rounded-rectangle pipeline the client uses for every box on this page.
# It is here because the hex above says *what* each field is and this says
# *how much of the frame it is* -- which is the whole argument, and a
# sentence cannot make it. Thirteen of the twenty-two bytes are the text
# itself; the protocol is the other nine.
def site_ruler(w)
  cell = w / 22
  paths = []
  x = 0
  for f in SITE_FRAME
    n = f[3]
    span = n * cell - 2
    paths = paths.concat([[1, f[2], x, 0, span, 12, 2]])
    x = x + n * cell
  end
  column({"gap": 2, "width": w}, [
    {"k": "canvas", "s": {"width": w, "height": 12}, "p": {"paths": paths}},
    row({"gap": 4, "justify": "between", "width": "100%"}, [
      text("nine bytes of protocol", {"font": site_body(), "size": 0, "fg": "text.muted"}),
      text("thirteen of text", {"font": site_body(), "size": 0, "fg": "text.muted"})
    ])
  ])
end

# ---------------------------------------------------------------- hover
#
# A hover state costs no round trip and no JavaScript: `pointer_enter`
# carries a chunk that points the node at a style record the session
# already holds, and `pointer_leave` points it back. `styles` is what the
# chunk is allowed to name -- it cannot invent a record, only choose among
# the ones declared with it (07 §4). `transition` makes the change a glide
# rather than a jump, and that is the client's, not a keyframe anybody
# wrote.

def site_step_style(lit)
  {
    "display": "row", "align": "center", "justify": "center",
    "width": 34, "height": 34, "radius": 2, "shrink": 0,
    "bg": lit ? "accent.base" : "surface.sunken",
    "fg": lit ? "accent.on" : "text.default",
    "cursor": "pointer", "transition": "fast",
    "border": 1, "border_color": lit ? "accent.base" : "border.subtle"
  }
end

# A stepper whose whole point is that it does not wait. `local` is the
# bytecode; `then` is the server event that follows it.
def site_step(label, program, after)
  {
    "k": "box",
    "s": site_step_style(false),
    "p": {"role": "button", "label": label},
    "on": {
      "click": {"local": program, "then": after},
      "pointer_enter": {"local": "self.style = @lit", "styles": {"lit": site_step_style(true)}},
      "pointer_leave": {"local": "self.style = @rest", "styles": {"rest": site_step_style(false)}}
    },
    "c": [text(label, {"font": "mono", "size": 3, "fg": "text.default"})]
  }
end

def site_invoice(lay, state)
  qty = state["qty"] ?? 12
  sent = state["sent"] ?? 0
  with_state({"qty": qty, "sent": sent}, column({
    "gap": 5,
    "pad": lay["wide"] ? 7 : 5,
    "bg": "surface.raised",
    "radius": 2,
    "border": 1,
    "border_color": "border.subtle",
    "shadow": 1,
    "width": "100%"
  }, [
    row({"gap": 3, "justify": "between", "align": "center", "width": "100%"}, [
      text("FRAME · BATCH · ONE OP", {"font": "mono", "size": 0, "fg": "text.muted"}),
      text("22 BYTES", {"font": "mono", "size": 0, "fg": "accent.base"})
    ]),
    row({"gap": 4, "wrap": "wrap", "align": "start", "width": "100%"},
      SITE_FRAME.map(fn(f) { site_field(f) })),
    site_ruler(lay["wide"] ? 420 : 280),
    rule(),
    row({"gap": 5, "align": "center", "wrap": "wrap", "width": "100%"}, [
      column({"gap": 1, "shrink": 0}, [
        text("Quantity", {"font": site_body(), "size": 0, "fg": "text.muted"}),
        row({"gap": 3, "align": "center"}, [
          site_step("−", "state.qty = state.qty - 1; qty.text = str(state.qty); total.text = str(state.qty * 102); state.sent = state.sent + 22; sent.text = str(state.sent)", "step"),
          keyed("qty", text(str(qty), {"font": "mono", "size": 3, "fg": "text.default", "width": 44, "text_align": "center"})),
          site_step("+", "state.qty = state.qty + 1; qty.text = str(state.qty); total.text = str(state.qty * 102); state.sent = state.sent + 22; sent.text = str(state.sent)", "step")
        ])
      ]),
      column({"gap": 1, "grow": 1, "shrink": 1}, [
        text("Line total", {"font": site_body(), "size": 0, "fg": "text.muted"}),
        row({"gap": 2, "align": "baseline"}, [
          keyed("total", text(str(qty * SITE_UNIT), {"font": site_body(), "size": 5, "weight": "bold", "fg": "accent.base"})),
          text("€", {"font": site_body(), "size": 4, "weight": "bold", "fg": "accent.base"})
        ])
      ])
    ]),
    row({"gap": 3, "align": "baseline", "wrap": "wrap"}, [
      text("sent this session", {"font": site_body(), "size": 0, "fg": "text.muted"}),
      keyed("sent", text(str(sent), {"font": "mono", "size": 1, "fg": "text.default"})),
      text("bytes — the number above, once per press", {"font": site_body(), "size": 0, "fg": "text.muted"})
    ])
  ]))
end

# ------------------------------------------------------------------ figures

def site_bar(width, label, amount, fraction, tone)
  column({"gap": 2, "width": width}, [
    row({"gap": 4, "justify": "between", "width": "100%"}, [
      text(label, {"font": "mono", "size": 1, "fg": "text.default"}),
      text(amount, {"font": "mono", "size": 1, "fg": tone})
    ]),
    {"k": "box", "s": {
      "display": "row", "height": 10, "radius": 1,
      "bg": "surface.sunken", "width": "100%", "overflow": "clip"
    }, "c": [
      {"k": "box", "s": {"width": fraction, "height": 10, "radius": 1, "bg": tone}}
    ]}
  ])
end

def site_dial(width, value, what, note)
  column({"gap": 2, "width": width, "shrink": 0}, [
    text(value, {"font": site_display(), "weight": "bold", "size": 6, "fg": "text.default"}),
    text(what, {"font": site_body(), "size": 1, "fg": "text.default", "width": "100%"}),
    text(note, {"font": "mono", "size": 0, "fg": "text.muted", "width": "100%"})
  ])
end

def site_process(width, name, what)
  column({"gap": 3, "width": width, "shrink": 0}, [
    text(name, {"font": site_body(), "size": 2, "weight": "bold", "fg": "text.default"}),
    text(what, {"font": site_body(), "size": 1, "fg": "text.muted", "width": "100%"})
  ])
end

def site_refusal_style(width, lit)
  {
    "display": "column", "gap": 2, "width": width, "shrink": 0,
    "pad": [3, 3, 3, 3], "radius": 1,
    "bg": lit ? "surface.raised" : "none",
    "transition": "fast"
  }
end

def site_refusal(width, what, where)
  restyled_refusal(width, what, where)
end

def restyled_refusal(width, what, where)
  inner = [
    row({"gap": 4, "align": "start", "width": "100%"}, [
      text("—", {"font": "mono", "size": 1, "fg": "danger.base"}),
      text(what, {"font": site_body(), "size": 1, "fg": "text.default", "grow": 1, "shrink": 1})
    ]),
    row({"gap": 4}, [
      text("  ", {"font": "mono", "size": 1}),
      text(where, {"font": "mono", "size": 0, "fg": "text.muted"})
    ])
  ]
  {
    "k": "box",
    "s": site_refusal_style(width, false),
    "on": {
      "pointer_enter": {"local": "self.style = @lit", "styles": {"lit": site_refusal_style(width, true)}},
      "pointer_leave": {"local": "self.style = @rest", "styles": {"rest": site_refusal_style(width, false)}}
    },
    "c": inner
  }
end

def site_row_style(lit)
  {
    "display": "row", "align": "center", "width": "100%",
    "pad": [3, 2, 3, 2], "radius": 1, "gap": 16,
    "bg": lit ? "surface.raised" : "none",
    "transition": "fast"
  }
end

def site_row(lay, piece, state, tone, evidence)
  cells = [
    text(piece, {"font": site_body(), "size": 1, "fg": "text.default", "grow": 1, "shrink": 1}),
    text(state, {"font": "mono", "size": 0, "fg": tone, "width": 130, "shrink": 0})
  ]
  cells = cells.concat([
    text(evidence, {"font": "mono", "size": 0, "fg": "text.muted", "width": 200, "shrink": 0})
  ]) if lay["wide"]
  {
    "k": "box",
    "s": site_row_style(false),
    "on": {
      "pointer_enter": {"local": "self.style = @lit", "styles": {"lit": site_row_style(true)}},
      "pointer_leave": {"local": "self.style = @rest", "styles": {"rest": site_row_style(false)}}
    },
    "c": cells
  }
end

# A grid that reflows: `per_row` columns of an equal share of the row,
# wrapping when they no longer fit.
#
# `gap_px` is the gap in pixels rather than a scale index, and on purpose:
# indexing a global array inside a `def` gives this interpreter a value it
# will not type, and the arithmetic that follows fails to compile. The two
# gaps this page uses are `space.6` and `space.8` -- 20 and 32 at cozy
# density (spec 05 §2) -- and they are passed as numbers.
def site_cols(lay, per_row, gap_px)
  span = (per_row - 1) * gap_px
  free = lay["content"] - span
  free / per_row
end

# ----------------------------------------------------------------- the page

def site(event_data)
  state = event_data["state"] ?? {}
  params = event_data["params"] ?? {}
  event = event_data["event"]
  # The viewport arrives on its own: once with `connect`, and again on
  # every resize. Nothing on the page has to ask for it, and nothing else
  # about the machine comes with it (06 §1).
  return set_key(state, "viewport", params["viewport"] ?? state["viewport"]) if event == "connect"
  return set_key(state, "viewport", params["viewport"] ?? state["viewport"]) if event == "viewport"
  # The stepper already moved the number on the client. This is the server
  # doing the same arithmetic and sending back what it believes, which is
  # the half that is authoritative -- a local handler is never trusted.
  if event == "step"
    props = params["props"] ?? {}
    qty = props["qty"] ?? state["qty"] ?? 12
    qty = 0 if qty < 0
    qty = 999 if qty > 999
    sent = props["sent"] ?? 0
    return set_key(set_key(state, "qty", qty), "sent", sent)
  end
  # 06 §1.1: the only event nobody caused. A node carrying a `wake` prop
  # and a `wake` handler is sent one every period, for exactly as long as
  # it carries both -- so the cascade stops itself by dropping the prop
  # when the last block is out, and the page costs nothing at rest.
  return set_key(state, "shown", (state["shown"] ?? 1) + 1) if event == "reveal"
  state
end

def set_key(h, k, v)
  out = h.merge({})
  out[k] = v
  out
end

def site_masthead(lay)
  row({
    "gap": 4, "align": "center", "justify": "between", "width": "100%",
    "pad": [5, lay["pad"], 5, lay["pad"]],
    "bg": "surface.base", "border": [0, 0, 1, 0], "border_color": "border.subtle",
    "shrink": 0
  }, [
    row({"gap": 4, "align": "baseline"}, [
      text("EUI", {"font": "mono", "size": 2, "weight": "bold", "fg": "text.default"}),
      text("an interface, delivered", {"font": site_body(), "size": 0, "fg": "text.muted"})
    ]),
    text(lay["roomy"] ? "A PROTOCOL · A CLIENT · A SPECIFICATION" : "PROTOCOL", {
      "font": "mono", "size": 0, "fg": "text.muted"
    })
  ])
end

# How many blocks are out, and whether the clock is still running.
#
# A stagger cannot be a style: `transition` is a duration and there is no
# delay field anywhere in the record (02 §3). What there is instead is
# `wake` -- so the blocks are *grafted* one at a time, and each one plays
# the entrance it already carries as it arrives. The delay is the period
# between grafts, and the curve is `enter`'s decelerate.
#
# 100 ms is the floor a client must impose (06 §1.1); 220 is what reads as
# a cascade rather than a burst. Each block eases over `slow` (320 ms, the
# top of the motion scale), so at 220 they overlap by a third -- one is
# still settling as the next starts, which is what makes it flow instead
# of ticking. It costs one round trip a block, once, and then the prop
# goes and nothing wakes again.
SITE_BLOCKS = 7

def site_view(raw_state)
  state = raw_state ?? {}
  shown = state["shown"] ?? 1
  lay = site_layout(state)
  blocks = site_blocks(lay, state)
  out = blocks.slice(0, shown)
  root = column({"width": "100%", "height": "100%", "bg": "surface.base"}, [
    site_masthead(lay),
    {"k": "scroll", "s": {"grow": 1, "width": "100%"}, "c": [
      column({
        "gap": lay["wide"] ? 11 : 9,
        "pad": [lay["wide"] ? 10 : 7, lay["pad"], 11, lay["pad"]],
        "width": "100%", "align": "center"
      }, out)
    ]}
  ])
  # The clock runs only while there is something left to bring out. When
  # the prop goes, the client stops waking this node -- nothing has to be
  # cancelled, because nothing was scheduled anywhere but here.
  return root if shown >= SITE_BLOCKS
  root["p"] = {"wake": 220}
  root["on"] = {"wake": "reveal"}
  root
end

# Every block of the page, in order, built whether or not it is out yet.
# Seven of them, which is `SITE_BLOCKS`.
def site_blocks(lay, state)
  half = site_cols(lay, 2, 32)
  third = site_cols(lay, lay["wide"] ? 3 : 1, 20)
  fifth = site_cols(lay, lay["huge"] ? 5 : (lay["wide"] ? 3 : 2), 20)
  [

        # -- the opening: the claim, and the thing itself, side by side ---
        column(site_enter({"width": lay["content"]}), [split(lay, 8,
          column({"gap": 6}, [
            display("An interface is not a document. Stop sending one."),
            lede("EUI serves applications over HTTPS with no HTML, no CSS, no JavaScript and no JIT. The server sends a tree that is already resolved; a native client applies it as binary patches and draws it on the GPU."),
            row({"gap": 4, "wrap": "wrap"}, [
              site_address("github.com/solisoft/eui"),
              text("— your browser, when you click it", {"font": site_body(), "size": 0, "fg": "text.muted"})
            ])
          ]),
          site_invoice(lay, state)
        )]),

        # -- what it costs ------------------------------------------------
        section(lay, "§02", "WIRE FORMAT", [
          split(lay, 8,
            column({"gap": 6}, [
              display("Three times less than the favourable case for HTML."),
              prose("Fifty rows, four columns, keyed, six shared style records — an invoice table. Against it, the same table as HTML carrying the classes such a table really carries, unindented.")
            ]),
            column({"gap": 6}, [
              site_bar("100%", "EUI", "4 619 B", "32%", "accent.base"),
              site_bar("100%", "HTML", "14 362 B", "100%", "text.muted"),
              prose("Both carry the same 2 337 bytes of text, which neither can compress away, so the honest headline is the structure: 2 282 against 12 025, or 5.3×. The regression budget is 4×, below what was measured.")
            ])
          )
        ]),

        # -- the budgets --------------------------------------------------
        section(lay, "§10", "BUDGETS", [
          measure(lay, [display("A budget nobody measured is a slogan.")]),
          row({"gap": 6, "wrap": "wrap", "width": lay["content"]}, [
            site_dial(fifth, "22 B", "a one-cell update", "measured · budget 40"),
            site_dial(fifth, "201 B", "reversing fifty keyed rows", "measured · forty-nine moves"),
            site_dial(fifth, "0 %", "CPU at rest, zero wakeups", "architecture, not a setting"),
            site_dial(fifth, "80 ms", "launch to first pixel", "target"),
            site_dial(fifth, "16.28 MB", "the stripped client", "target 12 MB · the miss is published")
          ]),
          measure(lay, [prose("The last one is over, by a third, and it is printed here for the same reason it is printed in the specification: a document that lists only the budgets it meets is an advertisement.")])
        ]),

        # -- the shape ----------------------------------------------------
        section(lay, "§01", "SESSION", [
          measure(lay, [display("Three processes, and what each of them may do.")]),
          row({"gap": 6, "wrap": "wrap", "width": lay["content"]}, [
            site_process(third, "Server", "Your application. It renders a tree, diffs it against the tree this session last received, and sends the patches."),
            site_process(third, "Window", "Owns the display, the GPU, TLS and the clipboard. It never decodes a frame. When it is asked to close, it closes."),
            site_process(third, "Worker", "Everything that reads bytes a server chose. Confined by Landlock and seccomp — 37 system calls, and any other one kills it.")
          ]),
          measure(lay, [prose("Assets are fetched by BLAKE3 and verified before anything decodes them; the publisher's Ed25519 key is pinned on first visit. A capability the manifest did not ask for has no code path at all.")])
        ]),

        # -- the refusals -------------------------------------------------
        section(lay, "§08", "WHAT IT REFUSES", [
          measure(lay, [
            display("Most of the safety here is subtraction."),
            prose("These are not defaults to be turned off. The client has no way to do them.")
          ]),
          row({"gap": 8, "wrap": "wrap", "width": lay["content"]}, [
            site_refusal(lay["wide"] ? half : lay["content"], "Run downloaded native code, or JIT anything. Two kinds of executable content reach the client and each has a verifier: bytecode metered by fuel, and — only where the scene capability was granted — a WGSL module whose loops must be countable.", "spec 07 · spec 11"),
            site_refusal(lay["wide"] ? half : lay["content"], "Resolve a cascade. Styles arrive already computed, as fixed 64-byte records; the client does one array lookup.", "spec 02"),
            site_refusal(lay["wide"] ? half : lay["content"], "Report a keystroke outside a focused field, enumerate fonts, or read back a canvas. The fingerprint surface is the viewport frame.", "spec 08"),
            site_refusal(lay["wide"] ? half : lay["content"], "Fall back to plain HTTP. TLS 1.3, or no session.", "spec 01")
          ])
        ]),

        # -- where it is --------------------------------------------------
        section(lay, "§09", "WHERE IT ACTUALLY IS", [
          measure(lay, [display("The protocol, the client and the server integration exist and are tested.")]),
          column({"gap": 0, "width": lay["content"]}, [
            site_row(lay, "Wire format, decoder, fuzzing", "BUILT", "success.base", "80 tests · 4 fuzz targets"),
            rule(),
            site_row(lay, "Layout, text, renderer", "BUILT", "success.base", "121 tests · golden pixels"),
            rule(),
            site_row(lay, "Client: session, input, IME, accessibility", "BUILT", "success.base", "297 tests"),
            rule(),
            site_row(lay, "Sound, moving pictures, 3D scenes", "BUILT", "success.base", "67 tests · 1 fuzz target"),
            rule(),
            site_row(lay, "Sandbox: Landlock, seccomp, no core dump", "BUILT", "success.base", "Linux only"),
            rule(),
            site_row(lay, "Sandbox on macOS and Windows", "NOT STARTED", "danger.base", "—"),
            rule(),
            site_row(lay, "Android, iOS", "BUILT, UNTESTED", "warning.base", "rolling APK and iOS builds")
          ]),
          measure(lay, [prose("619 tests in all. None of the figures on this page is a slogan: every one is a number a command gives back.")])
        ]),

        # -- the close ----------------------------------------------------
        column({"gap": 6, "width": lay["content"]}, [
          rule(),
          row({"gap": 6, "justify": "between", "wrap": "wrap", "align": "baseline", "width": "100%"}, [
            text("This page is an EUI application: one component, one view, and the window you are reading it in.", {
              "font": site_body(), "size": 1, "fg": "text.muted", "grow": 1, "shrink": 1
            }),
            text(str(lay["w"]) + " PX · NO HTML · NO CSS · NO JAVASCRIPT", {
              "font": "mono", "size": 0, "fg": "text.muted", "shrink": 0
            })
          ])
        ])
  ]
end
