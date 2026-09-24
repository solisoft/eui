# The pure halves of the newer widgets, tested without a client.
#
#   soli tests/catalogue_spec.sl
#
# A filter builder is the one widget here whose correctness is not visible in
# a screenshot: it draws a tree, and every control it draws sends the dotted
# path of the item it belongs to. A path that walks wrong edits the wrong
# condition, and the picture of that is a picture of a filter builder working.
#
# The definitions are copied in by `tools/sync_split_spec.py`.

# ---- copied from the catalogue, do not edit ----

def filter_at(tree, path)
  return tree if path.to_s == ""

  here = tree
  steps = path.to_s.split(".")
  i = 0
  while i < steps.length()
    items = here["items"] ?? []
    at = int(steps[i])
    return null if at < 0 || at >= items.length()

    here = items[at]
    i = i + 1
  end
  here
end

def filter_edit(tree, path, f)
  return f(tree) if path.to_s == ""

  steps = path.to_s.split(".")
  at = int(steps[0])
  rest = steps.slice(1, steps.length()).join(".")
  items = tree["items"] ?? []
  return tree if at < 0 || at >= items.length()

  out = range(0, items.length()).map(fn(i) { i == at ? filter_edit(items[i], rest, f) : items[i] })
  tree.merge({"items": out})
end

def filter_drop(tree, path)
  return tree if path.to_s == ""

  steps = path.to_s.split(".")
  return filter_edit(tree, steps.slice(0, steps.length() - 1).join("."), fn(g) {
    at = int(steps[steps.length() - 1])
    kept = range(0, (g["items"] ?? []).length()).filter(fn(i) { i != at }).map(fn(i) { g["items"][i] })
    g.merge({"items": kept})
  }) if steps.length() > 1

  at = int(steps[0])
  kept = range(0, (tree["items"] ?? []).length()).filter(fn(i) { i != at }).map(fn(i) { tree["items"][i] })
  tree.merge({"items": kept})
end

def filter_blank(fields, cmps)
  {"field": (fields[0] ?? "").to_s, "cmp": (cmps[0] ?? "is").to_s, "value": ""}
end

def filter_group_blank(fields, cmps)
  {"op": "and", "items": [filter_blank(fields, cmps)]}
end

def filter_group?(it)
  !(it["items"]).nil?
end

def track_part(part, style)
  {"k": "box", "s": style, "p": {"track_part": part}, "c": []}
end

def track_thumb(label, at, floor_v, ceil_v)
  {
    "k": "box",
    "s": {
      "width": 14, "height": 14, "radius": 4, "shrink": 0,
      "bg": "accent.base", "border": 2, "border_color": "surface.base"
    },
    "p": {
      "track_part": "thumb",
      "role": "slider",
      "label": label,
      "value_now": at,
      "value_min": floor_v,
      "value_max": ceil_v,
      "orientation": "horizontal"
    },
    "c": []
  }
end

def slider(value, min, max, on_set, o = {})
  {
    "k": "box",
    "s": {"display": "row", "align": "center", "width": o["width"] ?? 240, "height": 24, "cursor": "grab"},
    "p": {
      "track": "x",
      "track_min": min,
      "track_max": max,
      "track_step": o["step"] ?? 1,
      "track_value": value,
      "role": "slider",
      "label": o["label"] ?? "Value",
      "value_now": value,
      "value_min": min,
      "value_max": max,
      "orientation": "horizontal"
    },
    "on": {"change": on_set},
    "c": [
      track_part("groove", {"height": 4, "bg": "surface.sunken", "radius": 4}),
      track_part("fill", {"height": 4, "bg": "accent.base", "radius": 4}),
      {
        "k": "box",
        "s": {
          "width": 16, "height": 16, "radius": 4, "shrink": 0,
          "bg": "accent.base", "border": 2, "border_color": "surface.base"
        },
        "p": {"track_part": "thumb"},
        "c": []
      }
    ]
  }
end

def range_slider(low, high, min, max, on_set, o = {})
  {
    "k": "box",
    "key": o["key"] ?? "range",
    "s": {"display": "row", "align": "center", "width": o["width"] ?? 240, "height": 24, "cursor": "grab"},
    "p": {
      "track": "x",
      "track_min": min,
      "track_max": max,
      "track_step": o["step"] ?? 1,
      "track_value": [low, high],
      "role": "group",
      "label": o["label"] ?? "Range"
    },
    "on": {"change": on_set},
    "c": [
      track_part("groove", {"height": 4, "bg": "surface.sunken", "radius": 4}),
      track_thumb(o["low_label"] ?? "From", low, min, high),
      track_part("fill", {"height": 4, "bg": "accent.base", "radius": 4}),
      track_thumb(o["high_label"] ?? "To", high, low, max)
    ]
  }
end

def diff_tally(lines)
  added = lines.filter(fn(l) { (l["kind"] ?? "").to_s == "add" }).length()
  removed = lines.filter(fn(l) { (l["kind"] ?? "").to_s == "del" }).length()
  {"added": added, "removed": removed}
end

def editable(kind, value, on_change, o)
  base = {
    "pad": [2, 4, 2, 4],
    "border": 1,
    "border_color": "border.default",
    "radius": 2,
    "bg": "surface.raised",
    "size": 1,
    "shadow": 1,
    "transition": "fast"
  }
  style = base.merge(o["style"] ?? {})
  # A field drawn inside another box — the entry of a tag well, the number
  # in a currency field — says `bg: none` and lets the shell be the field. Its
  # shadow would then be cast by nothing, a grey slab inside the shell.
  style["shadow"] = 0 if style["bg"] == "none" && (o["style"] ?? {})["shadow"].nil?
  key = o["key"].to_s
  key = editable_key(on_change, o) if key.blank?
  n = {
    "k": kind,
    "t": value ?? "",
    "s": style,
    "on": {"change": on_change}.merge(o["on"] ?? {})
  }
  unless key.blank?
    n["key"] = key
    n["on"] = editable_states(style, key, n["on"])
  end
  props = o["props"] ?? {}
  hint = o["placeholder"] ?? ""
  props = props.merge({"placeholder": hint}) if hint != ""
  n["p"] = props if props.keys().length() > 0
  n
end

def editable_key(on_change, o)
  label = (o["props"] ?? {})["label"].to_s
  said = on_change.to_s
  return "" if said.blank? && label.blank?

  "ed:" + said + ":" + label
end

def editable_states(style, key, on)
  bad = style["border_color"] == "danger.base"
  # The ground does not change under the pointer or the caret: a web field
  # keeps its white, and says it is live with its edge. Focus paints the edge
  # in the accent as well as the client's ring, because the ring is drawn for
  # keyboard focus alone (03 §3) and a field clicked into had nothing else.
  hover = style.merge({})
  focus = style.merge({})
  hover["border_color"] = "border.strong" unless bad
  focus["border_color"] = "accent.base" unless bad
  styles = {"base": style, "hover": hover, "focus": focus}
  mine = "\"" + key + "\""
  held = "if state.field_focus == " + mine
  states = {
    "pointer_enter": {"local": held + " { self.style = @focus } else { self.style = @hover }", "styles": styles},
    "pointer_leave": {"local": held + " { self.style = @focus } else { self.style = @base }", "styles": styles},
    "focus": {"local": "state.field_focus = " + mine + "; self.style = @focus", "styles": styles},
    "blur": {"local": "state.field_focus = \"\"; self.style = @base", "styles": styles}
  }
  out = on.merge({})
  for name in states.keys()
    said = out[name]
    if said.nil?
      out[name] = states[name]
    elsif said.to_s == said
      out[name] = states[name].merge({"then": said})
    end
  end
  out
end

def input(value, on_change, o = {})
  editable("input", value, on_change, o)
end

def field_props(label, error, bad, o)
  props = {"label": o["name"] ?? label}
  note = error != "" ? error : (o["hint"] ?? "")
  props["description"] = note if note != ""
  props["invalid"] = true if bad
  props["required"] = true if o["required"] == true
  # The hint an empty field shows in its own box (03 §3). Beside `hint`, not
  # instead of it: that one is a sentence under the field that stays, this is
  # an example inside it that goes the moment anything is typed.
  props["placeholder"] = o["placeholder"] if (o["placeholder"] ?? "") != ""
  props
end

def check(label, got, want)
  assert_eq(got, want)
end

# ---- the tree the filter builder walks ----

FIELDS = ["Status", "Amount"]
CMPS = ["is", ">"]

TREE = {
  "op": "and",
  "items": [
    {"field": "Status", "cmp": "is", "value": "Late"},
    {"op": "or", "items": [
      {"field": "Amount", "cmp": ">", "value": "10000"},
      {"field": "Status", "cmp": "is", "value": "Unpaid"}
    ]}
  ]
}

check("the empty path is the root", filter_at(TREE, "")["op"], "and")
check("one step reaches a leaf", filter_at(TREE, "0")["value"], "Late")
check("one step reaches a group", filter_at(TREE, "1")["op"], "or")
check("two steps reach into it", filter_at(TREE, "1.1")["value"], "Unpaid")
check("a step past the end is nothing", filter_at(TREE, "9"), null)
check("a step past the end of a nested group is nothing", filter_at(TREE, "1.9"), null)

check("a leaf is not a group", filter_group?(filter_at(TREE, "0")), false)
check("a group is", filter_group?(filter_at(TREE, "1")), true)
check("an empty group is still a group", filter_group?({"op": "and", "items": []}), true)

EDITED = filter_edit(TREE, "1.0", fn(leaf) { leaf.merge({"value": "250"}) })
check("editing reaches the item named", filter_at(EDITED, "1.0")["value"], "250")
check("and leaves its sibling alone", filter_at(EDITED, "1.1")["value"], "Unpaid")
check("and leaves the other branch alone", filter_at(EDITED, "0")["value"], "Late")
check("the original is untouched", filter_at(TREE, "1.0")["value"], "10000")

OPPED = filter_edit(TREE, "1", fn(g) { g.merge({"op": "and"}) })
check("a group's operator is editable by path", filter_at(OPPED, "1")["op"], "and")
check("the root's operator is editable", filter_edit(TREE, "", fn(g) { g.merge({"op": "or"}) })["op"], "or")

DROPPED = filter_drop(TREE, "1.0")
check("dropping takes the item named", filter_at(DROPPED, "1.0")["value"], "Unpaid")
check("and shortens the group it was in", (filter_at(DROPPED, "1")["items"]).length(), 1)
check("dropping a top-level item shortens the root", (filter_drop(TREE, "0")["items"]).length(), 1)
check("and the one that is left has moved up", filter_at(filter_drop(TREE, "0"), "0")["op"], "or")
check("the root cannot be dropped", filter_drop(TREE, "")["op"], "and")

check("a blank condition takes the first field", filter_blank(FIELDS, CMPS)["field"], "Status")
check("and the first comparator", filter_blank(FIELDS, CMPS)["cmp"], "is")
check("a blank group starts with one condition", (filter_group_blank(FIELDS, CMPS)["items"]).length(), 1)

# ---- what a track declares ----
#
# The arithmetic these used to check is the client's now: `range_takes?`
# gated a `pointer_move` the widget no longer declares, and `range_moved`
# inverted a pointer offset against a `width` baked into the props. Both are
# gone, and what is checked here is that the tree still says what a track is
# -- the client reads nothing else (03 §3.4).
#
# The behaviour they pinned did not go with them: it is in
# `crates/eui-client/tests/driver.rs`, where the hand that resolves it is.

RS = range_slider(2000, 8000, 0, 10000, "demo_range", {"step": 250})

check("a range declares its axis", RS["p"]["track"], "x")
check("and what it measures", RS["p"]["track_max"], 10000)
check("and how finely", RS["p"]["track_step"], 250)
check("both ends travel together", RS["p"]["track_value"], [2000, 8000])
check("it carries no width to invert against", RS["p"]["width"], null)
check("and no pointer handler to gate", RS["on"]["pointer_move"], null)
check("one handler, and it is the value", (RS["on"].keys()).length(), 1)
check("which is `change`", RS["on"]["change"], "demo_range")

RS_PARTS = RS["c"].map(fn(c) { (c["p"] ?? {})["track_part"] })

check("the parts are named, not counted", RS_PARTS, ["groove", "thumb", "fill", "thumb"])
check("two handles make a range", RS_PARTS.filter(fn(x) { x == "thumb" }).length(), 2)

# Each handle is its own stop and its own slider, bounded by the one beside
# it -- which is how two of them cannot cross without a line of code here.
RS_LOW = RS["c"][1]
RS_HIGH = RS["c"][3]

check("a handle is a slider to a reader", RS_LOW["p"]["role"], "slider")
check("the low end stops at the high one", RS_LOW["p"]["value_max"], 8000)
check("and the high end starts at the low one", RS_HIGH["p"]["value_min"], 2000)
check("each says which it is", RS_HIGH["p"]["label"], "To")

SL = slider(40, 0, 100, "slider", {"step": 5})

SL_THUMBS = SL["c"].filter(fn(c) { (c["p"] ?? {})["track_part"] == "thumb" })

check("a slider is the same thing with one handle", SL_THUMBS.length(), 1)
check("its value is a number, not a pair", SL["p"]["track_value"], 40)
check("and it too has one handler", (SL["on"].keys()).length(), 1)

# ---- what a patch adds and takes away ----

PATCH = [
  {"kind": "hunk", "text": "@@"},
  {"kind": "same", "text": "a"},
  {"kind": "add", "text": "b"},
  {"kind": "add", "text": "c"},
  {"kind": "del", "text": "d"}
]

check("added lines are counted", diff_tally(PATCH)["added"], 2)
check("removed lines are counted", diff_tally(PATCH)["removed"], 1)
check("an empty patch counts nothing", diff_tally([])["added"], 0)

# ---- what a field says while it is empty ----
#
# 03 §3's `placeholder` is a prop the client draws in `text.muted` while the
# value is empty. The builders pass it through and never make it the value.

ED = input("", "search", {"placeholder": "Search mail"})

check("an input carries its placeholder as a prop", ED["p"]["placeholder"], "Search mail")
check("and not as its text", ED["t"], "")

ED_PLAIN = input("", "search", {})

check("a field without one declares none", ED_PLAIN["p"].nil?, true)

ED_BOTH = input("x", "search", {"placeholder": "Search mail", "props": {"label": "Search"}})

check("beside the props it was given", ED_BOTH["p"]["label"], "Search")
check("with its value untouched", ED_BOTH["t"], "x")

FP = field_props("Legal name", "", false, {"placeholder": "Meridian Trading Ltd", "hint": "As registered"})

check("a form field hands it to the control", FP["placeholder"], "Meridian Trading Ltd")
check("beside the hint under it, which is not the same thing", FP["description"], "As registered")
check("and a field that names none says nothing", field_props("Legal name", "", false, {})["placeholder"].nil?, true)
