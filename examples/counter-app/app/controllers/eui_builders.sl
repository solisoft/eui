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
    "cursor": "pointer",
    "transition": "fast"
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
  style["shadow"] = style["shadow"] ?? 1
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

# A spinner: a three-quarter arc on a canvas that the client spins.
def spinner
  spinner_sized(18)
end

def spinner_sized(size)
  half = size / 2
  {
    "k": "canvas",
    "s": {
      "width": size,
      "height": size,
      "animation": "spin",
      "shrink": 0
    },
    "p": {"paths": [ [
      4,
      "accent.base",
      2,
      half,
      half,
      half - 2,
      0,
      4.71
    ]]}
  }
end

# A button that shows it is working the instant it is pressed: a local
# handler reveals the spinner and changes the label, the server answers,
# and the client puts the button back the moment that answer arrives.
# A light/dark switch: the viewer's choice, made on the client by a local
# handler (`theme.toggle()`), so it costs no round trip and the server
# learns of it only as the next viewport. The glyph is the same on both
# sides: it names the switch, not the state.
def theme_toggle()
  {
    "k": "box",
    "s": {
      "display": "row",
      "align": "center",
      "justify": "center",
      "pad": [2, 3, 2, 3],
      "min_width": 36,
      "bg": "surface.sunken",
      "fg": "text.default",
      "radius": 2,
      "cursor": "pointer",
      "transition": "fast"
    },
    "on": {"click": {"local": "theme.toggle()"}},
    "c": [text("☀/☾", {"weight": "semibold"})]
  }
end

def loading_button(label, on_click, key)
  spin = spinner_sized(14)
  spin["s"]["display"] = "none"
  spin["key"] = key + "_spin"
  caption = text(label, {"weight": "semibold"})
  caption["key"] = key + "_label"
  showing = spinner_sized(14)["s"]
  showing["display"] = "row"
  {
    "k": "box",
    "s": {
      "display": "row",
      "gap": 2,
      "align": "center",
      "justify": "center",
      "pad": [2, 4, 2, 4],
      "min_width": 44,
      "bg": "surface.sunken",
      "fg": "text.default",
      "radius": 2,
      "cursor": "pointer",
      "transition": "fast"
    },
    "on": {"click": {
      "local": key + "_spin.style = @showing; " + key + "_label.text = \"Loading…\"",
      "styles": {"showing": showing},
      "then": on_click
    }},
    "c": [spin, caption]
  }
end

def toast(message, tone)
  {
    "k": "box",
    "s": {
      "display": "row",
      "gap": 3,
      "pad": [3, 4, 3, 4],
      "radius": 2,
      "shadow": 2,
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
      "shadow": 2,
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
        "shadow": 2,
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

# ---- Select ----------------------------------------------------------------

# A closed select is its anchor; open, a dropdown lists the options below it.
# The server owns `open`: the anchor toggles it, an option picks and closes.
# The anchor has a click handler, so Tab reaches it and Enter opens it.
def select(options, value, open, on_toggle, on_pick)
  anchor = {
    "k": "box",
    "s": {
      "display": "row",
      "align": "center",
      "gap": 2,
      "pad": [2, 3, 2, 3],
      "min_width": 160,
      "border": 1,
      "border_color": "border.default",
      "radius": 2,
      "bg": "surface.raised",
      "cursor": "pointer"
    },
    "on": {"click": on_toggle},
    "c": [text(value, {"grow": 1}), text(
      "▾",
      {"fg": "text.muted", "size": 0}
    )]
  }
  dropdown(anchor, options.map(fn(o) { select_option(o, o == value, on_pick) }), open)
end

def select_option(label, selected, on_pick)
  {
    "k": "box",
    "s": {
      "pad": [1, 3, 1, 3],
      "radius": 1,
      "min_width": 150,
      "bg": selected ? "surface.sunken" : "none",
      "cursor": "pointer"
    },
    "p": {"value": label},
    "on": {"click": on_pick},
    "c": [text(label, {"weight": selected ? "bold" : "regular"})]
  }
end

# A popover that opens under its anchor rather than over it.
def dropdown(anchor, content, open)
  return anchor unless open

  stack({"gap": 0}, [
    anchor,
    {
      "k": "box",
      "s": {
        "position": "absolute",
        "margin": [40, 0, 0, 0],
        "pad": 1,
        "radius": 2,
        "shadow": 2,
        "bg": "surface.overlay",
        "border": 1,
        "border_color": "border.subtle",
        "display": "column",
        "z": 5
      },
      "c": content
    }
  ])
end

# ---- Slider ----------------------------------------------------------------

# A 240 px track. A click sets the value from the pointer x; once the track
# has focus (Tab reaches it through its click handler) the arrow keys nudge
# it. The server owns the value: `on_set` receives `params["kind"]` — "click"
# or "key_down" — and `params["payload"]`, the local point or the key name.
def slider(value, min, max, on_set)
  filled = (value - min) * 240 / (max - min)
  lead = filled > 8 ? filled - 8 : 0
  {
    "k": "box",
    "s": {
      "display": "row",
      "align": "center",
      "width": 240,
      "height": 24,
      "cursor": "pointer"
    },
    "p": {"min": min, "max": max},
    "on": {"click": on_set, "key_down": on_set},
    "c": [
      node("box", {
        "width": lead,
        "height": 4,
        "bg": "accent.base",
        "radius": 4
      }, []),
      node("box", {
        "width": 16,
        "height": 16,
        "radius": 4,
        "bg": "accent.base",
        "border": 2,
        "border_color": "surface.base"
      }, []),
      node("box", {
        "grow": 1,
        "height": 4,
        "bg": "surface.sunken",
        "radius": 4
      }, [])
    ]
  }
end

# ---- Calendar engine -------------------------------------------------------

# One engine, three pickers. `month` is "YYYY-MM"; days are ISO "YYYY-MM-DD"
# strings, which compare correctly as strings.
def month_label(month)
  DateTime.parse(month + "-01").format("%B %Y")
end

def month_shift(month, delta)
  first_day = DateTime.parse(month + "-01")
  moved = delta > 0 ? first_day.end_of_month().add_days(1) : first_day.add_days(-1)
  moved.format("%Y-%m")
end

def weekday_index(day)
  {
    "Monday": 0,
    "Tuesday": 1,
    "Wednesday": 2,
    "Thursday": 3,
    "Friday": 4,
    "Saturday": 5,
    "Sunday": 6
  }[day.weekday()]
end

def two_digits(n)
  n < 10 ? "0" + str(n) : str(n)
end

def icon_button(label, on_click, props)
  {
    "k": "box",
    "s": {
      "width": 28,
      "height": 28,
      "radius": 1,
      "display": "row",
      "justify": "center",
      "align": "center",
      "cursor": "pointer"
    },
    "p": props,
    "on": {"click": on_click},
    "c": [text(label, {"weight": "bold"})]
  }
end

def day_cell(iso, label, selected, in_range, on_pick)
  {
    "k": "box",
    "s": {
      "width": 32,
      "height": 32,
      "radius": 1,
      "display": "row",
      "justify": "center",
      "align": "center",
      "bg": selected ? "accent.base" : (in_range ? "info.subtle" : "none"),
      "cursor": "pointer"
    },
    "p": {"date": iso},
    "on": {"click": on_pick},
    "c": [text(
      label,
      {"fg": selected ? "accent.on" : "text.default", "size": 1}
    )]
  }
end

def day_blank
  node("box", {"width": 32, "height": 32}, [])
end

def weekday_header
  cells = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].map(fn(w) {
    node("box", {
      "width": 32,
      "display": "row",
      "justify": "center"
    }, [muted(w)])
  })
  row({"gap": 0}, cells)
end

# The month grid: navigation, weekday header, seven columns of days.
# `selected` is a list of ISO days; `range_start`/`range_end` shade between.
def calendar(month, selected, range_start, range_end, on_pick, on_nav)
  first_day = DateTime.parse(month + "-01")
  blanks = range(0, weekday_index(first_day)).map(fn(i) { day_blank() })
  cells = range(1, first_day.end_of_month().day() + 1).map(fn(d) {
    iso = month + "-" + two_digits(d)
    shaded = range_start.present? && range_end.present? && iso >= range_start && iso <= range_end
    day_cell(iso, str(d), selected.includes?(iso), shaded, on_pick)
  })
  header = row(
    {"align": "center", "gap": 1},
    [
      icon_button("‹", on_nav, {"delta": -1}),
      text(
        month_label(month),
        {
          "weight": "semibold",
          "grow": 1,
          "text_align": "center"
        }
      ),
      icon_button("›", on_nav, {"delta": 1})
    ]
  )
  grid = {
    "k": "box",
    "s": {
      "display": "grid",
      "gap": 0,
      "width": 224
    },
    "p": {"columns": 7},
    "c": blanks.concat(cells)
  }
  column(
    {"gap": 1, "width": 224},
    [header, weekday_header(), grid]
  )
end

# ---- Pickers ---------------------------------------------------------------

def date_picker(month, value, on_pick, on_nav)
  column(
    {"gap": 2},
    [
      calendar(month, value.present? ? [value] : [], "", "", on_pick, on_nav),
      muted(value.present? ? value : "Pick a day")
    ]
  )
end

# A date and a time: the calendar plus an "HH:MM" field committed on change.
def datetime_picker(month, date, time, on_pick, on_nav, on_time)
  clock = row(
    {"gap": 2, "align": "center"},
    [muted("Time"), sized_input(time, on_time, 80)]
  )
  column({"gap": 2}, [
    calendar(month, date.present? ? [date] : [], "", "", on_pick, on_nav),
    clock,
    muted(date + " " + time)
  ])
end

def sized_input(value, on_change, width)
  box = input(value, on_change)
  box["s"]["width"] = width
  box
end

# Two selections on one calendar: the first click starts, the second ends,
# the third starts over. The server keeps the two ends ordered.
def date_range_picker(month, start, finish, on_pick, on_nav)
  ends = [start, finish].filter(fn(d) { d.present? })
  caption = finish.present? ? start + " → " + finish : (start.present? ? start + " → …" : "Pick a start day")
  column({"gap": 2}, [calendar(month, ends, start, finish, on_pick, on_nav), muted(caption)])
end

# A titled card, so a picker reads as one thing.
def labelled(title, child)
  card({"gap": 3}, [text(title, {"weight": "bold"}), child])
end

# ---- Charts ----------------------------------------------------------------

# A chart is a `canvas` with a `paths` prop, spec 03 §1.1: each path is a
# list — kind, colour, then numbers in logical px from the content box.
#   [0, colour, width, x0, y0, x1, y1, …]   polyline, round caps and joins
#   [1, colour, x, y, w, h, radius]          filled rectangle
#   [2, colour, base_y, x0, y0, x1, y1, …]   area between a polyline and base_y
#   [3, colour, cx, cy, r]                   filled circle
#   [4, colour, width, cx, cy, r, a0, a1]    arc, radians, clockwise from +x
def canvas(width, height, paths)
  {
    "k": "canvas",
    "s": {"width": width, "height": height},
    "p": {"paths": paths}
  }
end

def chart_max(values)
  top = 0
  for v in values
    top = v if v > top
  end
  top > 0 ? top : 1
end

# A series scaled into `w × h` with 4 px of breathing room, as [x, y] pairs.
def chart_points(values, w, h)
  top = chart_max(values)
  count = values.length()
  step = count > 1 ? (w - 8) / (count - 1) : 0
  range(0, count).map(fn(i) { [4 + i * step, 4 + (h - 8) * (top - values[i]) / top] })
end

# Four hairlines, so a series has something to be read against.
def chart_grid(w, h)
  range(0, 4).map(fn(i) { [
    0,
    "border.subtle",
    1,
    4,
    4 + (h - 8) * i / 3,
    w - 4,
    4 + (h - 8) * i / 3
  ] })
end

def flatten_points(points)
  flat = []
  for p in points
    flat = flat.concat(p)
  end
  flat
end

def chart_line(values, w, h)
  points = chart_points(values, w, h)
  line = [0, "accent.base", 2].concat(flatten_points(points))
  dots = points.map(fn(p) { [
    3,
    "accent.base",
    p[0],
    p[1],
    3
  ] })
  canvas(w, h, chart_grid(w, h).concat([line]).concat(dots))
end

def chart_area(values, w, h)
  flat = flatten_points(chart_points(values, w, h))
  area = [2, "info.subtle", h - 4].concat(flat)
  line = [0, "info.base", 2].concat(flat)
  canvas(w, h, chart_grid(w, h).concat([area, line]))
end

def chart_bar(values, w, h)
  top = chart_max(values)
  count = values.length()
  slot = (w - 8) / count
  bars = range(0, count).map(fn(i) {
    bar_h = (h - 8) * values[i] / top;
    [1, "accent.base", 4 + i * slot + slot / 8, h - 4 - bar_h, slot - slot / 4, bar_h, 1]
  })
  canvas(w, h, chart_grid(w, h).concat(bars))
end

# A donut: one arc per part, in the four "base" roles, a small gap between.
def chart_donut(parts, w, h)
  total = parts.sum()
  total = 1 if total == 0
  roles = ["accent.base", "info.base", "success.base", "warning.base"]
  radius = (w < h ? w : h) / 2 - 10
  arcs = []
  start = -1.5707963
  i = 0
  for p in parts
    sweep = 6.2831853 * p / total
    arcs = arcs.concat([ [
      4,
      roles[i % 4],
      14,
      w / 2,
      h / 2,
      radius,
      start,
      start + sweep - 0.04
    ]])
    start = start + sweep
    i = i + 1
  end
  canvas(w, h, arcs)
end

# ---- Feed ------------------------------------------------------------------

# A text whose content repeats across many nodes: interned as an atom, so
# the wire carries it once per session.
def text_interned(content, style)
  interned = text(content, style)
  interned["intern"] = true
  interned
end

# An initial in a coloured disc, in place of a fetched avatar.
def initial_avatar(letter, tone, size)
  {
    "k": "box",
    "s": {
      "width": size,
      "height": size,
      "radius": 4,
      "bg": tone,
      "display": "row",
      "justify": "center",
      "align": "center"
    },
    "c": [text_interned(
      letter,
      {
        "fg": "accent.on",
        "weight": "bold",
        "size": 3
      }
    )]
  }
end

# One action under a post: a glyph and a count, clickable, carrying the
# post id so one handler serves every post.
def post_action(glyph, count, on_click, props, active)
  {
    "k": "box",
    "s": {
      "display": "row",
      "gap": 1,
      "align": "center",
      "pad": [1, 2, 1, 2],
      "radius": 2,
      "cursor": "pointer",
      "fg": active ? "accent.base" : "text.muted"
    },
    "p": props,
    "on": {"click": on_click},
    "c": [text_interned(glyph, {"size": 1}), text(str(count), {"size": 1})]
  }
end

# A post card of fixed height, so a feed of thousands can be virtualised:
# the client lays out only the cards it can see.
def post_card(post, liked, height)
  header = row(
    {
      "gap": 2,
      "align": "center",
      "wrap": "wrap"
    },
    [
      text_interned(post["name"], {"weight": "semibold"}),
      text_interned(
        post["handle"],
        {"fg": "text.muted", "size": 1}
      ),
      text_interned(
        "· " + post["when"],
        {"fg": "text.muted", "size": 1}
      )
    ]
  )
  body = text(post["text"], {"clamp": 2})
  picture = post["image"].nil? ? [] : [{
    "k": "image",
    "p": {"src": post["image"]},
    "s": {
      "width": "100%",
      "max_width": 480,
      "height": 180,
      "radius": 2
    }
  }]
  actions = row(
    {"gap": 4, "align": "center"},
    [
      post_action("↩", post["replies"], "noop", {"id": post["id"]}, false),
      post_action("⟳", post["reposts"], "noop", {"id": post["id"]}, false),
      post_action("♥", post["likes"] + (liked ? 1 : 0), "like", {"id": post["id"]}, liked)
    ]
  )
  {
    "k": "box",
    "s": {
      "display": "row",
      "gap": 3,
      "pad": [3, 4, 3, 4],
      "height": height,
      "overflow": "clip",
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle",
      "align": "start"
    },
    "p": {"item_height": height},
    "c": [
      initial_avatar(post["initial"], post["tone"], 40),
      column(
        {
          "gap": 1,
          "grow": 1,
          "shrink": 1,
          "min_width": 0
        },
        [header, body].concat(picture).concat([actions])
      )
    ]
  }
end
