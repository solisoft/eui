# EUI view builders. Each returns a plain hash; nothing here is native.
# Lives in app/controllers/ so it loads with the handlers (one namespace).
#
#   {"k": kind, "s": style, "t": text, "c": children, "on": handlers, "key": key, "p": props}
#
# Style keys are the spec's vocabulary: display, gap, pad, margin, bg, fg,
# border, border_color, radius, size, weight, width, height, align, justify,
# wrap, grow, cursor… Colours are role names ("accent.base") or "#RRGGBB".

def node(kind, style, children)
  {
    "k": kind,
    "s": style,
    "c": children
  }
end

def column(style, children)
  style["display"] = "column"
  node("box", style, children)
end

def row(style, children)
  style["display"] = "row"
  node("box", style, children)
end

def stack(style, children)
  style["display"] = "stack"
  node("box", style, children)
end

def text(content, style)
  {
    "k": "text",
    "t": content,
    "s": style
  }
end

def spacer
  {"k": "spacer", "s": {"grow": 1}}
end

def divider
  {"k": "divider"}
end

def scroll(style, children)
  style["display"] = "column"
  node("scroll", style, children)
end

# A virtualised list: `item_height` lets the client skip rows it cannot see.
def list(style, item_height, children)
  style["display"] = "column"
  {
    "k": "list",
    "s": style,
    "c": children,
    "p": {"item_height": item_height}
  }
end

def input(value, on_change)
  {
    "k": "input",
    "t": value,
    "s": {
      "pad": [2, 3, 2, 3],
      "border": 1,
      "border_color": "border.default",
      "radius": 2
    },
    "on": {"change": on_change}
  }
end

# The primary button: accent roles, so it follows the viewer into dark mode
# without the server knowing.
def button(label, on_click)
  button_variant(label, on_click, "accent.base", "accent.on")
end

def keyed(key, n)
  n["key"] = key
  n
end

# ---------------------------------------------------------------- catalogue
# Composed widgets. Every one is a plain function over the primitives above;
# state and events belong to the handler, so a checkbox carries the id of what
# it toggles as a prop, and the server reads it back from params["props"].

def h1(content)
  text(
    content,
    {"size": 5, "weight": "bold"}
  )
end

def h2(content)
  text(
    content,
    {"size": 4, "weight": "semibold"}
  )
end

def muted(content)
  text(
    content,
    {"fg": "text.muted", "size": 1}
  )
end

# A button: a box with a click handler, and hover/pressed states that run
# locally — the client repoints the node at a declared style on pointer
# enter/down/up/leave, so feedback never waits for the network. The node is
# keyed so the local handler can name it (`self`).
def button_variant(label, on_click, bg, fg)
  base = {
    "display": "row",
    "justify": "center",
    "align": "center",
    "pad": [2, 4, 2, 4],
    "min_width": 44,
    "bg": bg,
    "fg": fg,
    "radius": 2,
    "cursor": "pointer"
  }
  hover = base.merge({
    "bg": bg == "accent.base" ? "accent.hover" : bg,
    "border": 1,
    "border_color": "border.strong"
  })
  active = base.merge({"bg": bg == "accent.base" ? "accent.active" : "surface.sunken"})
  {
    "k": "box",
    "key": "btn:" + on_click + ":" + label,
    "s": base,
    "on": {
      "click": on_click,
      "pointer_enter": {"local": "self.style = @hover", "styles": {"hover": hover}},
      "pointer_leave": {"local": "self.style = @base", "styles": {"base": base}},
      "pointer_down": {"local": "self.style = @active", "styles": {"active": active}},
      "pointer_up": {"local": "self.style = @hover", "styles": {"hover": hover}}
    },
    "c": [text(label, {"weight": "semibold"})]
  }
end

def secondary_button(label, on_click)
  button_variant(label, on_click, "surface.sunken", "text.default")
end

def danger_button(label, on_click)
  button_variant(label, on_click, "danger.base", "danger.on")
end

def ghost_button(label, on_click)
  button_variant(label, on_click, "none", "accent.base")
end

# A checkbox is a small box whose fill says its state, plus a label. `props`
# travel back with the click so the handler knows which item it was.
def checkbox(label, checked, on_toggle, props)
  mark = {
    "k": "box",
    "s": {
      "width": 18,
      "height": 18,
      "radius": 1,
      "border": 2,
      "border_color": checked ? "accent.base" : "border.strong",
      "bg": checked ? "accent.base" : "none",
      "display": "row",
      "justify": "center",
      "align": "center"
    },
    "c": checked ? [text(
      "✓",
      {
        "fg": "accent.on",
        "size": 0,
        "weight": "bold"
      }
    )] : []
  }
  {
    "k": "box",
    "s": {
      "display": "row",
      "gap": 3,
      "align": "center",
      "cursor": "pointer"
    },
    "on": {"click": on_toggle},
    "p": props,
    "c": [mark, text(label, checked ? {"fg": "text.muted"} : {})]
  }
end

def switch(label, on, on_toggle, props)
  knob = {"k": "box", "s": {
    "width": 16,
    "height": 16,
    "radius": 4,
    "bg": on ? "accent.on" : "surface.raised",
    "self": on ? "end" : "start"
  }}
  track = {
    "k": "box",
    "s": {
      "display": "row",
      "width": 36,
      "height": 20,
      "radius": 4,
      "pad": 1,
      "bg": on ? "accent.base" : "border.strong",
      "justify": on ? "end" : "start",
      "align": "center"
    },
    "c": [knob]
  }
  {
    "k": "box",
    "s": {
      "display": "row",
      "gap": 3,
      "align": "center",
      "cursor": "pointer"
    },
    "on": {"click": on_toggle},
    "p": props,
    "c": [track, text(label, {})]
  }
end

def badge(label, tone)
  {
    "k": "box",
    "s": {
      "display": "row",
      "pad": [0, 2, 0, 2],
      "radius": 4,
      "bg": tone + ".subtle"
    },
    "c": [text(
      label,
      {
        "fg": tone + ".base",
        "size": 0,
        "weight": "semibold"
      }
    )]
  }
end

def card(style, children)
  style["bg"] = "surface.raised"
  style["radius"] = 3
  style["border"] = 1
  style["border_color"] = "border.subtle"
  style["pad"] = style["pad"] ?? 5
  style["display"] = "column"
  node("box", style, children)
end

# Tabs: a row of labels, the active one underlined in accent. `props` carry
# the tab name so one handler serves every tab.
def tabs(names, active, on_select)
  row(
    {
      "gap": 5,
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    names.map(fn(name) {
      is_active = name == active
      {
        "k": "box",
        "s": {
          "pad": [2, 1, 2, 1],
          "border": [0, 0, 2, 0],
          "border_color": is_active ? "accent.base" : "none",
          "cursor": "pointer"
        },
        "on": {"click": on_select},
        "p": {"tab": name},
        "c": [text(name, is_active ? {"weight": "semibold"} : {"fg": "text.muted"})]
      }
    })
  )
end

def spinner
  {"k": "box", "s": {
    "width": 18,
    "height": 18,
    "radius": 4,
    "border": 2,
    "border_color": "accent.base"
  }}
end

def toast(message, tone)
  {
    "k": "box",
    "s": {
      "display": "row",
      "gap": 3,
      "pad": [3, 4, 3, 4],
      "radius": 2,
      "bg": tone + ".subtle",
      "border": 1,
      "border_color": tone + ".base"
    },
    "c": [text(
      message,
      {
        "fg": tone + ".base",
        "weight": "semibold"
      }
    )]
  }
end

# A dialog is a stack: a dimming overlay, then the panel, centred.
def dialog(title, body_children, actions)
  panel = card(
    {
      "width": 360,
      "gap": 4,
      "self": "center"
    },
    [h2(title)].concat(body_children, [row(
      {"gap": 2, "justify": "end"},
      actions
    )])
  )
  {
    "k": "overlay",
    "s": {
      "display": "stack",
      "justify": "center",
      "align": "center",
      "bg": "#00000066"
    },
    "c": [panel]
  }
end

def field(label, value, on_change)
  column({"gap": 1}, [muted(label), input(value, on_change)])
end

def form(children, submit_label, on_submit)
  column({"gap": 4}, children.concat([row({"justify": "end"}, [button(submit_label, on_submit)])]))
end

# A table: header row plus keyed body rows; `columns` is a list of widths.
def table_header(labels, widths)
  cells = range(0, labels.length()).map(fn(i) {
    {
      "k": "text",
      "t": labels[i],
      "s": {
        "width": widths[i],
        "weight": "semibold",
        "size": 1,
        "fg": "text.muted"
      }
    }
  })
  row(
    {
      "gap": 4,
      "pad": [2, 3, 2, 3],
      "border": [0, 0, 1, 0],
      "border_color": "border.default"
    },
    cells
  )
end

def table_row(key, values, widths)
  cells = range(0, values.length()).map(fn(i) { {
    "k": "text",
    "t": values[i],
    "s": {"width": widths[i]}
  } })
  keyed(key, row(
    {
      "gap": 4,
      "pad": [1, 3, 1, 3],
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    cells
  ))
end

# A button whose click runs a local chunk first, then a server event.
# `program` is the assembly list of spec/07; node targets are keys.
def local_button(label, program, after)
  b = button(label, after)
  b["on"]["click"] = {"local": program, "then": after}
  b
end

# The root node's props are the component's local state.
def with_state(state, root)
  root["p"] = state
  root
end

# An image asset from the application, by path. The server hashes the file
# and the client fetches it once, by content, and caches it forever.
def image(src, width, height)
  {
    "k": "image",
    "p": {"src": src},
    "s": {"width": width, "height": height}
  }
end

def avatar(src, size)
  {
    "k": "image",
    "p": {"src": src},
    "s": {
      "width": size,
      "height": size,
      "radius": 4
    }
  }
end

# ------------------------------------------------------- catalogue, part 2
# Everything below composes from the same primitives. A widget that needs
# a state (open/closed, selected, page) keeps it in the handler; the widget
# only draws what it is told and carries the identity a handler needs in
# its props.

def progress(fraction)
  filled = (fraction * 100).to_i
  filled = 100 if filled > 100
  filled = 0 if filled < 0
  {
    "k": "box",
    "s": {
      "display": "row",
      "height": 6,
      "radius": 4,
      "bg": "surface.sunken",
      "overflow": "clip"
    },
    "c": [{"k": "box", "s": {
      "width": filled.to_s + "%",
      "bg": "accent.base",
      "radius": 4
    }}]
  }
end

def skeleton(width, height)
  {"k": "box", "s": {
    "width": width,
    "height": height,
    "radius": 2,
    "bg": "surface.sunken"
  }}
end

def chip(label, on_remove, props)
  parts = [text(label, {"size": 1})]
  if on_remove.present?
    x = ghost_button("×", on_remove)
    x["p"] = props
    x["s"]["pad"] = [0, 1, 0, 1]
    x["s"]["min_width"] = 0
    parts = parts.concat([x])
  end
  {
    "k": "box",
    "s": {
      "display": "row",
      "align": "center",
      "gap": 1,
      "pad": [0, 2, 0, 3],
      "radius": 4,
      "bg": "surface.sunken",
      "border": 1,
      "border_color": "border.subtle"
    },
    "c": parts
  }
end

def stat(label, value, hint)
  card(
    {"gap": 1, "min_width": 160},
    [muted(label), text(
      value,
      {"size": 6, "weight": "bold"}
    ), text(
      hint,
      {"fg": "text.muted", "size": 0}
    )]
  )
end

def empty_state(title, body, action_label, on_action)
  column(
    {
      "align": "center",
      "gap": 3,
      "pad": 8
    },
    [
      {"k": "box", "s": {
        "width": 48,
        "height": 48,
        "radius": 4,
        "bg": "surface.sunken"
      }},
      h2(title),
      text(
        body,
        {"fg": "text.muted", "text_align": "center"}
      ),
      button(action_label, on_action)
    ]
  )
end

def banner(message, tone, action_label, on_action)
  row(
    {
      "gap": 3,
      "align": "center",
      "pad": [2, 4, 2, 4],
      "radius": 2,
      "bg": tone + ".subtle",
      "border": [0, 0, 0, 3],
      "border_color": tone + ".base"
    },
    [text(message, {"fg": "text.default"}), spacer(), ghost_button(action_label, on_action)]
  )
end

# Breadcrumb: every crumb but the last is a link carrying its path.
def breadcrumb(crumbs, on_go)
  parts = []
  i = 0
  for crumb in crumbs
    parts = parts.concat([muted("/")]) if i > 0
    if i == crumbs.length() - 1
      parts = parts.concat([text(crumb["label"], {"weight": "semibold"})])
    else
      link = {
        "k": "text",
        "t": crumb["label"],
        "s": {"fg": "accent.base", "cursor": "pointer"},
        "on": {"click": on_go},
        "p": {"path": crumb["path"]}
      }
      parts = parts.concat([link])
    end
    i = i + 1
  end
  row(
    {"gap": 2, "align": "center"},
    parts
  )
end

def pagination(page, pages, on_page)
  prev = secondary_button("‹", on_page)
  prev["p"] = {"page": page - 1}
  nxt = secondary_button("›", on_page)
  nxt["p"] = {"page": page + 1}
  row(
    {"gap": 2, "align": "center"},
    [prev, muted(page.to_s + " / " + pages.to_s), nxt]
  )
end

# Segmented control: one row of options, the selected one raised.
def segmented(options, selected, on_select)
  cells = options.map(fn(opt) {
    is_sel = opt == selected
    {
      "k": "box",
      "s": {
        "pad": [1, 3, 1, 3],
        "radius": 1,
        "bg": is_sel ? "surface.raised" : "none",
        "cursor": "pointer",
        "border": is_sel ? 1 : 0,
        "border_color": "border.subtle"
      },
      "on": {"click": on_select},
      "p": {"option": opt},
      "c": [text(opt, is_sel ? {"weight": "semibold"} : {"fg": "text.muted"})]
    }
  })
  row(
    {
      "gap": 1,
      "pad": 1,
      "radius": 2,
      "bg": "surface.sunken"
    },
    cells
  )
end

# Accordion: sections with a header that toggles by id; the open one shows its body.
def accordion(sections, open_id, on_toggle)
  column(
    {
      "gap": 0,
      "border": 1,
      "border_color": "border.subtle",
      "radius": 2
    },
    sections.map(fn(sec) {
      is_open = sec["id"] == open_id
      header = {
        "k": "box",
        "s": {
          "display": "row",
          "align": "center",
          "gap": 2,
          "pad": [2, 3, 2, 3],
          "cursor": "pointer",
          "border": [0, 0, 1, 0],
          "border_color": "border.subtle"
        },
        "on": {"click": on_toggle},
        "p": {"id": sec["id"]},
        "c": [text(
          is_open ? "▾" : "▸",
          {"fg": "text.muted", "size": 0}
        ), text(sec["title"], {"weight": "semibold"})]
      }
      body = is_open ? [column({"pad": [
        2,
        3,
        3,
        3
      ]}, [text(sec["body"], {"fg": "text.muted"})])] : []
      column({"gap": 0}, [header].concat(body))
    })
  )
end

# Stepper: numbered steps, done ones filled, the current one ringed.
def stepper(steps, current)
  cells = []
  i = 0
  for label in steps
    tone = i < current ? "accent.base" : (i == current ? "surface.raised" : "surface.sunken")
    dot = {
      "k": "box",
      "s": {
        "width": 24,
        "height": 24,
        "radius": 4,
        "bg": tone,
        "display": "row",
        "justify": "center",
        "align": "center",
        "border": i == current ? 2 : 0,
        "border_color": "accent.base"
      },
      "c": [text(
        (i + 1).to_s,
        {
          "size": 0,
          "weight": "semibold",
          "fg": i < current ? "accent.on" : "text.default"
        }
      )]
    }
    cells = cells.concat([row(
      {"gap": 2, "align": "center"},
      [dot, text(label, i == current ? {"weight": "semibold"} : {"fg": "text.muted"})]
    )])
    if i < steps.length() - 1
      cells = cells.concat([{"k": "box", "s": {
        "width": 24,
        "height": 1,
        "bg": "border.default"
      }}])
    end
    i = i + 1
  end
  row(
    {"gap": 2, "align": "center"},
    cells
  )
end

# A menu is an overlay anchored by the caller: a raised column of items.
def menu(items, on_pick)
  column(
    {
      "gap": 0,
      "pad": 1,
      "radius": 2,
      "bg": "surface.overlay",
      "border": 1,
      "border_color": "border.subtle",
      "min_width": 160
    },
    items.map(fn(it) {
      {
        "k": "box",
        "s": {
          "pad": [1, 3, 1, 3],
          "radius": 1,
          "cursor": "pointer"
        },
        "on": {"click": on_pick},
        "p": {"item": it},
        "c": [text(it, {})]
      }
    })
  )
end

def tooltip(content)
  {
    "k": "box",
    "s": {
      "pad": [1, 2, 1, 2],
      "radius": 1,
      "bg": "text.default"
    },
    "c": [text(
      content,
      {"fg": "text.inverted", "size": 0}
    )]
  }
end

# A sheet slides from an edge over the page: overlay, dim, panel at the edge.
def sheet(side, children)
  panel = column(
    {
      "gap": 4,
      "pad": 5,
      "bg": "surface.raised",
      "width": 320,
      "self": "stretch"
    },
    children
  )
  {
    "k": "overlay",
    "s": {
      "display": "row",
      "justify": side == "left" ? "start" : "end",
      "align": "stretch",
      "bg": "#00000066"
    },
    "c": [panel]
  }
end

def drawer(children)
  sheet("left", children)
end

def popover(anchor, content, open)
  return anchor unless open

  stack({"gap": 0}, [
    anchor,
    {
      "k": "box",
      "s": {
        "position": "absolute",
        "margin": [0, 0, 0, 0],
        "pad": 3,
        "radius": 2,
        "bg": "surface.overlay",
        "border": 1,
        "border_color": "border.subtle",
        "z": 5
      },
      "c": content
    }
  ])
end

def toolbar(children)
  row(
    {
      "gap": 2,
      "align": "center",
      "pad": [1, 2, 1, 2],
      "bg": "surface.raised",
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    children
  )
end

def navbar(brand, links, active, on_go)
  items = links.map(fn(l) {
    {
      "k": "text",
      "t": l,
      "s": l == active ? {"weight": "semibold"} : {
        "fg": "text.muted",
        "cursor": "pointer"
      },
      "on": {"click": on_go},
      "p": {"path": l}
    }
  })
  row(
    {
      "gap": 5,
      "align": "center",
      "pad": [2, 4, 2, 4],
      "bg": "surface.raised",
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    [text(brand, {"weight": "bold"})].concat(items)
  )
end

def sidebar(links, active, on_go)
  column(
    {
      "gap": 1,
      "pad": 3,
      "width": 200,
      "bg": "surface.raised",
      "border": [0, 1, 0, 0],
      "border_color": "border.subtle"
    },
    links.map(fn(l) {
      {
        "k": "box",
        "s": {
          "pad": [1, 2, 1, 2],
          "radius": 1,
          "bg": l == active ? "accent.subtle" : "none",
          "cursor": "pointer"
        },
        "on": {"click": on_go},
        "p": {"path": l},
        "c": [text(l, l == active ? {
          "weight": "semibold",
          "fg": "accent.base"
        } : {})]
      }
    })
  )
end

def code_block(code)
  {
    "k": "box",
    "s": {
      "pad": 3,
      "radius": 2,
      "bg": "surface.sunken",
      "overflow": "clip"
    },
    "c": [text(
      code,
      {"font": "mono", "size": 1}
    )]
  }
end

def tree_view(nodes, open_ids, on_toggle, depth)
  column({"gap": 0}, nodes.map(fn(n) {
    is_open = open_ids.includes?(n["id"])
    has_kids = n["children"].length() > 0
    rowv = {
      "k": "box",
      "s": {
        "display": "row",
        "gap": 1,
        "align": "center",
        "pad": [0, 1, 0, 1],
        "margin": [0, 0, 0, depth * 3],
        "cursor": has_kids ? "pointer" : "default"
      },
      "on": has_kids ? {"click": on_toggle} : {},
      "p": {"id": n["id"]},
      "c": [text(
        has_kids ? (is_open ? "▾" : "▸") : "·",
        {"fg": "text.muted", "size": 0}
      ), text(n["label"], {})]
    }
    kids = is_open && has_kids ? [tree_view(n["children"], open_ids, on_toggle, depth + 1)] : []
    column({"gap": 0}, [rowv].concat(kids))
  }))
end
