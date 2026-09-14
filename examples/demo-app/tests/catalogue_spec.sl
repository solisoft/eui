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

# ---- copied from app/controllers/eui_builders.sl, do not edit ----

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

def range_takes?(kind, dragging)
  said = (kind ?? "").to_s
  return true if said == "click" || said == "pointer_down" || said == "pointer_up"

  said == "pointer_move" && dragging == true
end

def range_holding?(kind)
  said = (kind ?? "").to_s
  said == "pointer_down" || said == "pointer_move"
end

def range_moved(props, x)
  # Not `min`/`max`: both are builtins, and a bare assignment rebinds the
  # global -- the static checker rejects the file outright, which is the one
  # mercy in this family of mistakes.
  floor_v = props["min"] ?? 0
  ceil_v = props["max"] ?? 100
  width = props["width"] ?? 240
  width = 1 if width <= 0
  at = floor_v + (ceil_v - floor_v) * x / width
  at = floor_v if at < floor_v
  at = ceil_v if at > ceil_v
  low = props["low"] ?? floor_v
  high = props["high"] ?? ceil_v
  near_low = (at - low) < 0 ? low - at : at - low
  near_high = (at - high) < 0 ? high - at : at - high
  return {"low": at, "high": high} if near_low <= near_high && at <= high

  return {"low": low, "high": at} if at >= low

  at <= low ? {"low": at, "high": high} : {"low": low, "high": at}
end

def diff_tally(lines)
  added = lines.filter(fn(l) { (l["kind"] ?? "").to_s == "add" }).length()
  removed = lines.filter(fn(l) { (l["kind"] ?? "").to_s == "del" }).length()
  {"added": added, "removed": removed}
end

def check(label, got, want)
  if got == want
    print("ok   " + label)
  else
    print("FAIL " + label + " got " + got.to_s + " want " + want.to_s)
  end
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

# ---- when a range moves at all ----
#
# The bug this pins: a track reports `pointer_move` whenever the pointer
# crosses it, so a handler that takes every move has handles that follow the
# pointer without anybody pressing anything.

check("a press takes", range_takes?("pointer_down", false), true)
check("a click takes", range_takes?("click", false), true)
check("a release takes", range_takes?("pointer_up", true), true)
check("a move with a hand down takes", range_takes?("pointer_move", true), true)
check("a move with no hand down does not", range_takes?("pointer_move", false), false)
check("a key is not a pointer", range_takes?("key_down", true), false)
check("nothing is not a pointer", range_takes?(null, true), false)

check("a press means a hand is on it", range_holding?("pointer_down"), true)
check("a gated move means it is still on it", range_holding?("pointer_move"), true)
check("a release means it is gone", range_holding?("pointer_up"), false)
check("a click is not a hand held down", range_holding?("click"), false)

# ---- which end of a range a pointer asks for ----

SPAN = {"min": 0, "max": 100, "width": 200, "low": 20, "high": 80}

check("a press near the low handle moves it", range_moved(SPAN, 50)["low"], 25)
check("and leaves the high one", range_moved(SPAN, 50)["high"], 80)
check("a press near the high handle moves it", range_moved(SPAN, 170)["high"], 85)
check("and leaves the low one", range_moved(SPAN, 170)["low"], 20)
check("past the right edge clamps to max", range_moved(SPAN, 400)["high"], 100)
check("past the left edge clamps to min", range_moved(SPAN, 0 - 50)["low"], 0)
check("the ends never cross", range_moved(SPAN, 0)["low"] <= range_moved(SPAN, 0)["high"], true)

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
