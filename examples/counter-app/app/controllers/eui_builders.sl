# EUI view builders. Each returns a plain hash; nothing here is native.
# Lives in app/controllers/ so it loads with the handlers (one namespace).
#
#   {"k": kind, "s": style, "t": text, "c": children, "on": handlers, "key": key, "p": props}
#
# Style keys are the spec's vocabulary: display, gap, pad, margin, bg, fg,
# border, border_color, radius, size, weight, width, height, align, justify,
# wrap, grow, cursor… Colours are role names ("accent.base") or "#RRGGBB".

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

def stack(style, children)
  style["display"] = "stack"
  node("box", style, children)
end

def text(content, style)
  {"k": "text", "t": content, "s": style}
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
  {"k": "list", "s": style, "c": children, "p": {"item_height": item_height}}
end

def input(value, on_change)
  {"k": "input", "t": value, "s": {"pad": [2, 3, 2, 3], "border": 1, "border_color": "border.default", "radius": 2}, "on": {"change": on_change}}
end

# A button is a box with a click handler. Variants are roles, so it follows
# the viewer into dark mode without the server knowing.
def button(label, on_click)
  {
    "k": "box",
    "s": {"display": "row", "justify": "center", "align": "center", "pad": [2, 4, 2, 4], "min_width": 44,
          "bg": "accent.base", "fg": "accent.on", "radius": 2, "cursor": "pointer"},
    "on": {"click": on_click},
    "c": [text(label, {"weight": "semibold"})]
  }
end

def keyed(key, n)
  n["key"] = key
  n
end
