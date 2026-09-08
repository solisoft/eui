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

# A windowed list (spec 04 §7.1): `count` rows of which only `children`
# are present, each carrying its `row`; `heights` gives every row's height
# so the scroll extent is exact. `on_window` receives `[first, last]`
# when the rows in view change.
def list_window(style, item_height, count, heights, children, on_window)
  style["display"] = "column"
  {
    "k": "list",
    "s": style,
    "c": children,
    "p": {
      "item_height": item_height,
      "count": count,
      "heights": heights
    },
    "on": {"window": on_window}
  }
end

# A sound (EUI spec 03 §7). It draws nothing; it plays. `src` is a file in
# the application, hashed and served like a picture. `props` may carry
# "playing", "volume" (0..100), "loop" and "position" (ms — the client
# seeks when the number changes), and `on` may carry "ended" and
# "time_update" handlers.
def audio(src, props, on)
  # `sound`, not `node`: a bare assignment to a builder's name rebinds it.
  sound = {"k": "audio", "p": props.merge({"src": src})}
  sound["on"] = on unless on.nil?
  sound
end

# A moving picture (EUI spec 03 §8). It sizes itself to its frames unless
# a style says otherwise. `props` may carry "playing", "loop" and
# "position" (ms), and `on` may carry "ended". GIF and animated WebP: the
# client decodes them in Rust, in its sandboxed worker.
def video(src, props, style, on)
  picture = {
    "k": "video",
    "s": style ?? {},
    "p": props.merge({"src": src})
  }
  picture["on"] = on unless on.nil?
  picture
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

# Viewport breakpoints, same rungs as Tailwind: the view branches on these
# because the client has no media-query engine. `width` is
# `state["viewport"]["width"]`, sent on connect and every resize.
BP = {
  "xs": 0,
  "sm": 640,
  "md": 768,
  "lg": 1024,
  "xl": 1280,
  "2xl": 1536
}

# The spacing scale, 05 §2, in px at cozy density — the same numbers the
# client resolves `pad`, `gap` and `margin` against. A view needs them
# when it has to predict a box's width instead of being told: a canvas is
# drawn at a size the server chooses, so it can only fill its parent if
# the server can work out what the parent will give it.
SPACE = [0, 2, 4, 8, 12, 16, 20, 24, 32, 40, 48, 64, 96]

def space_px(ix, density)
  factor = 1.0
  factor = 0.8 if density == "compact"
  factor = 1.25 if density == "comfortable"
  int((SPACE[ix] * factor).round())
end

def bp_px(name)
  BP[name] ?? 0
end

def bp(width)
  return "2xl" if width >= BP["2xl"]
  return "xl" if width >= BP["xl"]
  return "lg" if width >= BP["lg"]
  return "md" if width >= BP["md"]
  return "sm" if width >= BP["sm"]

  "xs"
end

def bp_min(width, name)
  width >= bp_px(name)
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
      "bg": tone + ".subtle",
      "shrink": 0
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
# learns of it only as the next viewport. Not shown in the feed: a client
# that follows the desktop's theme has no use for it.
def theme_toggle
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

# ---- Data grid -------------------------------------------------------------
# An editable, sortable grid. The header sits outside the list — sticky
# without sticky — and rows are keyed so a sort is MoveChild and a cell
# edit is set_text. `columns` are `{id, label, width, editable, options,
# align}`; `rows` are hashes keyed by those ids plus `id`. A column is
# editable unless `editable` is `false`. A non-empty `options` list makes
# the editor a compact select. `align` is `start`, `center` or `end`
# (default `start`). `selected` / `editing` / `sort` are `{row, col}`,
# `{row, col, open}`, `{col, dir}` — empty hashes when none. Rows are an
# equal-column `grid` (`1fr` each), so two columns are 50 %, three 33 %,
# filling the parent. The handler owns all three.

def grid_sort_rows(rows, col, dir)
  sorted = rows.sort_by(col)
  dir == "desc" ? sorted.reverse() : sorted
end

def grid_col_editable(col)
  return false if col["editable"] == false

  true
end

def grid_col_align(col)
  a = col["align"] ?? "start"
  return a if a == "center" || a == "end"

  "start"
end

def grid_cell(row_id, col, value, selected, editing, open, on_select, on_change, on_key)
  col_id = col["id"]
  props = {"row": row_id, "col": col_id}
  key = "cell:" + str(row_id) + ":" + col_id
  editable = grid_col_editable(col)
  align = grid_col_align(col)
  choices = col["options"] ?? []
  # The keyed node is always a box. Swapping it for an `input` is a kind
  # change, which replaces the node id — and the next event in the same
  # gesture (Enter's key_down, or change-then-submit) misses the tree.
  inner = text(
    value,
    {
      "size": 1,
      "clamp": 1,
      "text_align": align,
      "fg": selected == true ? "info.base" : "text.default"
    }
  )
  if choices.length() > 0 && !(editing == true)
    tone = value == "Paid" ? "success" : (value == "Open" ? "warning" : "info")
    inner = badge(value, tone)
  end
  if editing == true && editable == true && choices.length() > 0
    head = row(
      {
        "align": "center",
        "justify": align,
        "gap": 1,
        "grow": 1
      },
      [text(
        value,
        {
          "size": 1,
          "grow": 1,
          "clamp": 1
        }
      ), text(
        open == true ? "▴" : "▾",
        {"size": 0, "fg": "text.muted"}
      )]
    )
    picks = choices.map(fn(o) {
      {
        "k": "box",
        "s": {
          "pad": [1, 2, 1, 2],
          "radius": 1,
          "bg": o == value ? "surface.sunken" : "none",
          "cursor": "pointer"
        },
        "p": props.merge({"value": o}),
        "on": {"click": on_select},
        "c": [text(
          o,
          {"size": 1, "weight": o == value ? "bold" : "regular"}
        )]
      }
    })
    inner = open == true ? column(
      {"gap": 0, "grow": 1},
      [head].concat(picks)
    ) : head
  end
  if editing == true && editable == true && choices.length() == 0
    inner = {
      "k": "input",
      "t": value,
      "s": {
        "grow": 1,
        "pad": [1, 2, 1, 2],
        "border": 1,
        "border_color": "focus.ring",
        "radius": 1,
        "bg": "surface.raised",
        "text_align": align
      },
      "p": props,
      "on": {
        "change": on_change,
        "submit": on_select,
        "blur": on_select
      }
    }
  end
  cursor = "pointer"
  cursor = "text" if editable == true && choices.length() == 0
  base = {
    "display": "row",
    "justify": align,
    "align": "center",
    "pad": [1, 2, 1, 2],
    "radius": 1,
    "cursor": cursor,
    "bg": selected == true ? "info.subtle" : "none",
    "transition": "fast"
  }
  hover = base.merge({"bg": selected == true ? "info.subtle" : "surface.sunken"})
  {
    "k": "box",
    "key": key,
    "s": base,
    "p": props,
    "on": {
      "click": on_select,
      "key_down": on_key,
      "pointer_enter": {"local": "self.style = @hover", "styles": {"hover": hover}},
      "pointer_leave": {"local": "self.style = @base", "styles": {"base": base}}
    },
    "c": [inner]
  }
end

def grid_row(record, columns, selected, editing, on_select, on_change, on_key)
  row_id = record["id"]
  cells = columns.map(fn(col) {
    id = col["id"]
    is_edit = editing["row"] == row_id && editing["col"] == id
    grid_cell(
      row_id,
      col,
      str(record[id] ?? ""),
      selected["row"] == row_id && selected["col"] == id,
      is_edit,
      is_edit && editing["open"] == true,
      on_select,
      on_change,
      on_key
    )
  })
  n = columns.length()
  n = 1 if n < 1
  keyed(
    row_id,
    {
      "k": "box",
      "s": {
        "display": "grid",
        "width": "100%",
        "gap": 4,
        "align": "center",
        "border": [0, 0, 1, 0],
        "border_color": "border.subtle"
      },
      "p": {"columns": n},
      "c": cells
    }
  )
end

def grid_header(columns, sort, on_sort)
  cells = columns.map(fn(col) {
    id = col["id"]
    active = sort["col"] == id
    mark = ""
    mark = sort["dir"] == "desc" ? " ↓" : " ↑" if active
    align = grid_col_align(col)
    hs = {
      "display": "row",
      "justify": align,
      "align": "center",
      "pad": [2, 2, 2, 2],
      "cursor": "pointer"
    }
    {
      "k": "box",
      "key": "grid-h:" + id,
      "s": hs,
      "p": {"col": id},
      "on": {"click": on_sort},
      "c": [text(
        col["label"] + mark,
        {
          "weight": "semibold",
          "size": 1,
          "text_align": align,
          "fg": active == true ? "accent.base" : "text.muted"
        }
      )]
    }
  })
  n = columns.length()
  n = 1 if n < 1
  {
    "k": "box",
    "s": {
      "display": "grid",
      "width": "100%",
      "gap": 4,
      "pad": [1, 2, 1, 2],
      "align": "center",
      "border": [0, 0, 1, 0],
      "border_color": "border.default",
      "bg": "surface.sunken"
    },
    "p": {"columns": n},
    "c": cells
  }
end

def data_grid(columns, rows, selected, editing, sort, on_select, on_sort, on_change, on_key)
  body = rows.map(fn(r) { grid_row(r, columns, selected, editing, on_select, on_change, on_key) })
  n = rows.length()
  h = n * 32
  # A list that fits its rows still eats the wheel. Only virtualise when
  # the body is taller than the cap; otherwise a column lets the page scroll.
  inner = h > 256 ? list({"height": 256, "width": "100%"}, 32, body) : column(
    {"gap": 0, "width": "100%"},
    body
  )
  column(
    {
      "gap": 0,
      "width": "100%",
      "border": 1,
      "border_color": "border.subtle",
      "radius": 2,
      "bg": "surface.raised"
    },
    [grid_header(columns, sort, on_sort), inner]
  )
end

# A button whose click runs a local chunk first, then a server event.
# `program` is the assembly list of spec/07; node targets are keys.
def local_button(label, program, after)
  b = button(label, after)
  b["on"]["click"] = {"local": program, "then": after}
  b
end

# The root node's props are the component's local state — merged into
# whatever props the root already carries, since a root may also ask the
# client for something (a `wake`, 06 §1.1).
def with_state(state, root)
  root["p"] = (root["p"] ?? {}).merge(state)
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
      "overflow": "clip",
      "grow": 1,
      "width": "100%"
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

# The × that drops a chip. A button carries its hover and press styles with
# it, so reusing one here and narrowing only the live style let the × jump
# back to a button's padding under the pointer. This one is a fixed box:
# what changes on hover is the colour, which 03 §5 animates without ever
# running layout again.
def chip_remove(on_remove, props)
  base = {
    "display": "row",
    "justify": "center",
    "align": "center",
    "width": 16,
    "height": 16,
    "radius": 4,
    "bg": "none",
    "fg": "text.muted",
    "cursor": "pointer",
    "transition": "fast"
  }
  hover = base.merge({"fg": "danger.base"})
  active = base.merge({"bg": "danger.subtle", "fg": "danger.base"})
  {
    "k": "box",
    "key": "chip-x:" + on_remove + ":" + str(props["id"] ?? ""),
    "s": base,
    "p": props,
    "on": {
      "click": on_remove,
      "pointer_enter": {"local": "self.style = @hover", "styles": {"hover": hover}},
      "pointer_leave": {"local": "self.style = @base", "styles": {"base": base}},
      "pointer_down": {"local": "self.style = @active", "styles": {"active": active}},
      "pointer_up": {"local": "self.style = @hover", "styles": {"hover": hover}}
    },
    "c": [text(
      "×",
      {"size": 1, "weight": "semibold"}
    )]
  }
end

def chip(label, on_remove, props)
  parts = [text(label, {"size": 1})]
  parts = parts.concat([chip_remove(on_remove, props)]) if on_remove.present?
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
      "border_color": "border.subtle",
      "shrink": 0
    },
    "c": parts
  }
end

# A card that asks a wrapping row for `basis` pixels and takes an equal
# share of whatever the line has left: three tiles across on a desktop,
# two on a tablet, one on a phone, decided by the width itself rather
# than by a breakpoint, and with no hole at the end of the last line.
def tile(basis, node)
  node["s"]["width"] = "auto"
  node["s"]["basis"] = basis
  node["s"]["grow"] = 1
  node["s"]["shrink"] = 1
  node
end

def stat(label, value, hint)
  card(
    {"gap": 1, "width": "100%"},
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
    {
      "gap": 2,
      "align": "center",
      "shrink": 0
    },
    parts
  )
end

def pagination(page, pages, on_page)
  prev = secondary_button("‹", on_page)
  prev["p"] = {"page": page - 1}
  nxt = secondary_button("›", on_page)
  nxt["p"] = {"page": page + 1}
  row(
    {
      "gap": 2,
      "align": "center",
      "shrink": 0
    },
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
      "bg": "surface.sunken",
      "shrink": 0
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
      "k": "overlay",
      "s": {
        "position": "absolute",
        "margin": [2, 0, 0, 0],
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
          "bg": l == active ? "surface.sunken" : "none",
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

# A read-only code viewer with line numbers and scrolling.
# Displays code in a monospace font with a pinned gutter (line numbers stay
# visible while scrolling horizontally). Gutter/code lines stay pixel-aligned
# because both use the same font_size, which determines line-height.
# `opts` may include {"language": "...", "line_numbers": true/false, "spans": [...]}.
# `spans` is an optional array of [start_byte, len_byte, color] triples for syntax highlighting.
def code_viewer(code, opts)
  opts = opts ?? {}
  show_numbers = opts["line_numbers"] ?? true

  # Split code by newlines to get line count; build gutter numbers.
  lines = code.split("\n")
  line_count = lines.length()
  digit_width = line_count.to_s.length()

  # Build line number strings: "   1", "   2", etc., right-aligned.
  # Include trailing newline to match code's line structure.
  numbers_text = range(0, line_count).map(fn(i) { (i + 1).to_s.rjust(digit_width) }).join("\n")

  # Code styling: monospace, compact size.
  code_style = {"font": "mono", "size": 1}
  gutter_style = {
    "font": "mono",
    "size": 1,
    "text_align": "end",
    "fg": "text.muted",
    "pad": [0, 1, 0, 1]
  }

  # Wrap code text if large (>2KB) to use interning. `if` is a statement
  # in Soli, not an expression, so the choice is a ternary.
  code_text_node = code.length() > 2000 ? text_interned(code, code_style) : text(code, code_style)

  # Apply syntax highlighting spans if provided.
  unless opts["spans"].nil?
    code_text_node["p"] = code_text_node["p"] ?? {}
    code_text_node["p"]["spans"] = opts["spans"]
  end

  gutter_box = {
    "k": "box",
    "s": {
      "display": "column",
      "width": (digit_width * 8) + 8,
      "bg": "surface.raised",
      "overflow": "clip",
      "shrink": 0
    },
    "c": [text(numbers_text, gutter_style)]
  }
  # ~8px per digit + padding

  # Inner scroll holds the code; overflow: "scroll" gives unbounded width
  # so the code Text node doesn't wrap.
  code_scroll = {
    "k": "scroll",
    "s": {
      "display": "column",
      "overflow": "scroll",
      "grow": 1
    },
    "c": [code_text_node]
  }

  # Content row: gutter + code, both inside.
  content_row = row(
    {"gap": 0, "align": "start"},
    show_numbers ? [gutter_box, code_scroll] : [code_scroll]
  )

  # Outer scroll allows vertical scrolling of the entire viewer.
  outer_scroll = scroll(
    {
      "radius": 2,
      "bg": "surface.sunken",
      "pad": 1,
      "grow": 1
    },
    [content_row]
  )

  outer_scroll
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
  select_sized(options, value, open, on_toggle, on_pick, 160, false)
end

def select_sized(options, value, open, on_toggle, on_pick, min_width, grow)
  s = {
    "display": "row",
    "align": "center",
    "gap": 2,
    "pad": [2, 3, 2, 3],
    "min_width": min_width,
    "border": 1,
    "border_color": "border.default",
    "radius": 2,
    "bg": "surface.raised",
    "cursor": "pointer"
  }
  s["grow"] = 1 if grow
  anchor = {
    "k": "box",
    "s": s,
    "on": {"click": on_toggle},
    "c": [text(value, {"grow": 1}), text(
      "▾",
      {"fg": "text.muted", "size": 0}
    )]
  }
  dropdown(anchor, options.map(fn(o) { select_option(o, o == value, on_pick, min_width) }), open)
end

def select_option(label, selected, on_pick, min_width)
  {
    "k": "box",
    "s": {
      "pad": [1, 3, 1, 3],
      "radius": 1,
      "min_width": min_width,
      "bg": selected ? "surface.sunken" : "none",
      "cursor": "pointer"
    },
    "p": {"value": label},
    "on": {"click": on_pick},
    "c": [text(label, {"weight": selected ? "bold" : "regular"})]
  }
end

# A popover that opens under its anchor rather than over it.
# An open list floats: it is an `overlay`, so it paints in the top layer
# and no card or scroller clips it, and it is `absolute` in a `stack`, so
# it neither grows the box it hangs off nor pushes the page open. Where it
# lands is the client's business (04 §5): under the anchor when the window
# has room, over it when it has not, and never past an edge. The top
# margin is the gap it keeps.
def dropdown(anchor, content, open)
  return anchor unless open

  stack({"gap": 0}, [
    anchor,
    {
      "k": "overlay",
      "s": {
        "position": "absolute",
        "margin": [2, 0, 0, 0],
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

# A 240 px track. A press sets the value from the pointer x and a drag
# follows it; once the track has focus (Tab reaches it through its click
# handler) the arrow keys nudge it. The server owns the value: `on_set`
# receives `params["kind"]` — "click", "pointer_down", "pointer_move",
# "pointer_up", or "key_down" — and `params["payload"]`.
def slider(value, min, max, on_set)
  width = 240
  span = max - min
  span = 1 if span == 0
  filled = (value - min) * width / span
  lead = filled > 8 ? filled - 8 : 0
  {
    "k": "box",
    "s": {
      "display": "row",
      "align": "center",
      "width": width,
      "height": 24,
      "cursor": "grab"
    },
    "p": {
      "min": min,
      "max": max,
      "width": width
    },
    "on": {
      "click": on_set,
      "key_down": on_set,
      "pointer_down": on_set,
      "pointer_move": on_set,
      "pointer_up": on_set
    },
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
  node("box", {"height": 32}, [])
end

def weekday_header
  cells = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].map(fn(w) {
    node("box", {"display": "row", "justify": "center"}, [muted(w)])
  })
  {
    "k": "box",
    "s": {
      "display": "grid",
      "gap": 0,
      "width": "100%"
    },
    "p": {"columns": 7},
    "c": cells
  }
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
      "width": "100%"
    },
    "p": {"columns": 7},
    "c": blanks.concat(cells)
  }
  # A month is a panel of its own: a surface a shade lighter than the card
  # it sits on, padded and rounded, so the days read as one block rather
  # than as text loose on the card.
  column(
    {
      "gap": 1,
      "width": "100%",
      "pad": 3,
      "radius": 2,
      "bg": "surface.overlay",
      "border": 1,
      "border_color": "border.subtle"
    },
    [header, weekday_header(), grid]
  )
end

# ---- Pickers ---------------------------------------------------------------

def date_picker(month, value, on_pick, on_nav)
  column(
    {"gap": 2, "width": "100%"},
    [
      calendar(month, value.present? ? [value] : [], "", "", on_pick, on_nav),
      muted(value.present? ? value : "Pick a day")
    ]
  )
end

# A date and a time: the calendar plus hour and minute selects, so the
# clock cannot hold anything but HH:MM.
def datetime_picker(
  month,
  date,
  time,
  hour_open,
  min_open,
  on_pick,
  on_nav,
  on_hour_toggle,
  on_min_toggle,
  on_hour,
  on_min
)
  bits = (time ?? "00:00").split(":")
  hour = bits[0]
  minute = "00"
  minute = bits[1] if bits.length() > 1
  hours = range(0, 24).map(fn(h) { two_digits(h) })
  minutes = range(0, 60).map(fn(m) { two_digits(m) })
  clock = column(
    {"gap": 1, "width": "100%"},
    [
      muted("Time"),
      row(
        {
          "gap": 2,
          "align": "center",
          "width": "100%"
        },
        [
          select_sized(hours, hour, hour_open, on_hour_toggle, on_hour, 64, true),
          text(":", {"weight": "bold"}),
          select_sized(minutes, minute, min_open, on_min_toggle, on_min, 64, true)
        ]
      )
    ]
  )
  column(
    {"gap": 2, "width": "100%"},
    [calendar(month, date.present? ? [date] : [], "", "", on_pick, on_nav), clock, muted(date + " " + time)]
  )
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
  column(
    {"gap": 2, "width": "100%"},
    [calendar(month, ends, start, finish, on_pick, on_nav), muted(caption)]
  )
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

# A face when there is one, a coloured initial when there is not: a real
# timeline brings pictures, the sample brings letters, and the card treats
# them the same.
def post_avatar(post, size)
  face = post["avatar"] ?? ""
  return initial_avatar(post["initial"], post["tone"], size) if face == ""

  built = avatar(face, size)
  built["s"]["shrink"] = 0
  built
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
# What a card carries under its text: a picture, a moving picture, or a
# sound with a button to start it. The sound node exists only while that
# card is the one playing — the client holds a few sources, not a feed of
# them.
# A bar showing where a picture or a sound is, clickable to seek. The
# width is fixed so the click's x maps straight onto the position.
def media_scrubber(width, at, duration, on_seek, props)
  filled = duration > 0 ? int(width * at / duration) : 0
  filled = width if filled > width
  {
    "k": "box",
    "s": {
      "display": "row",
      "align": "center",
      "width": width,
      "height": 6,
      "radius": 4,
      "bg": "surface.sunken",
      "cursor": "pointer"
    },
    "p": props,
    "on": {"click": on_seek},
    "c": [{"k": "box", "s": {
      "width": filled,
      "height": 6,
      "radius": 4,
      "bg": "accent.base"
    }}]
  }
end

def media_clock(ms)
  seconds = ms / 1000
  str(seconds / 60) + ":" + (seconds % 60 < 10 ? "0" : "") + str(seconds % 60)
end

# A play/pause button of the size these cards use.
def media_button(on, event, props)
  {
    "k": "box",
    "s": {
      "display": "row",
      "align": "center",
      "justify": "center",
      "width": 28,
      "height": 28,
      "radius": 4,
      "bg": on ? "accent.base" : "surface.sunken",
      "fg": on ? "accent.on" : "text.default",
      "cursor": "pointer",
      "shrink": 0
    },
    "p": props,
    "on": {"click": event},
    "c": [text(on ? "▮▮" : "▶", {"size": 1})]
  }
end

def post_media(post, play)
  playing = play["sound"] ?? false
  kind = post["media"]
  if kind == "image"
    return [{
      "k": "image",
      "p": {"src": post["image"]},
      "s": {
        "width": "100%",
        "max_width": 480,
        "height": 180,
        "radius": 2
      }
    }]
  end

  if kind == "video"
    on = play["video"] ?? false
    at = play["at"] ?? 0
    duration = play["duration"] ?? 0
    props = {"id": post["id"]}
    return [
      video("public/video/pulse.gif", {
        "playing": on,
        "loop": false,
        "position": play["seek"] ?? 0
      }, {
        "width": "100%",
        "max_width": 480,
        "height": 180,
        "radius": 2
      }, {"time_update": "video_time", "ended": "video_ended"}),
      row(
        {
          "gap": 3,
          "align": "center",
          "width": "100%",
          "max_width": 480
        },
        [
          media_button(on, "video_play", props),
          media_scrubber(300, at, duration > 0 ? duration : 1440, "video_seek", props.merge({"w": 300})),
          muted(media_clock(at) + " / " + media_clock(duration > 0 ? duration : 1440))
        ]
      )
    ]
  end
  # Full width, like a video on a timeline anywhere else.

  if kind == "audio"
    controls = row(
      {"gap": 3, "align": "center"},
      [
        {
          "k": "box",
          "s": {
            "display": "row",
            "gap": 2,
            "align": "center",
            "pad": [2, 3, 2, 3],
            "radius": 4,
            "bg": playing ? "accent.base" : "surface.sunken",
            "fg": playing ? "accent.on" : "text.default",
            "cursor": "pointer"
          },
          "p": {"id": post["id"]},
          "on": {"click": "play"},
          "c": [text(
            playing ? "▮▮  Playing" : "▶  Play the chime",
            {"size": 1, "weight": "semibold"}
          )]
        },
        muted("1.6 s")
      ]
    )
    parts = [controls]
    if playing
      parts = parts.concat([audio("public/sounds/chime.wav", {
        "playing": true,
        "volume": 80
      }, {"ended": "sound_ended"})])
    end
    return parts
  end

  []
end

def post_card(post, liked, play, height)
  header = row(
    {
      "gap": 2,
      "align": "center",
      "wrap": "wrap"
    },
    [
      text(
        "#" + str(post["n"]),
        {
          "fg": "text.muted",
          "size": 1,
          "font": "mono"
        }
      ),
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
  # The card's number, so a scroll through a hundred thousand of them
  # can be checked by eye: nothing skipped, nothing repeated.
  body = text(post["text"], {"clamp": 2})
  picture = post_media(post, play)
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
      post_avatar(post, 40),
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
