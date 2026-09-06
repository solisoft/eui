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

# A button is a box with a click handler. Variants are roles, so it follows
# the viewer into dark mode without the server knowing.
def button(label, on_click)
  {
    "k": "box",
    "s": {
      "display": "row",
      "justify": "center",
      "align": "center",
      "pad": [2, 4, 2, 4],
      "min_width": 44,
      "bg": "accent.base",
      "fg": "accent.on",
      "radius": 2,
      "cursor": "pointer"
    },
    "on": {"click": on_click},
    "c": [text(label, {"weight": "semibold"})]
  }
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

def button_variant(label, on_click, bg, fg)
  {
    "k": "box",
    "s": {
      "display": "row",
      "justify": "center",
      "align": "center",
      "pad": [2, 4, 2, 4],
      "min_width": 44,
      "bg": bg,
      "fg": fg,
      "radius": 2,
      "cursor": "pointer"
    },
    "on": {"click": on_click},
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
