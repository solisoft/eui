# Combobox and command palette algebra, tested without a client.
#
#   soli tests/combo_spec.sl
#
# The definitions are copied in by `tools/sync_split_spec.py`.

# ---- copied from app/controllers/eui_builders.sl, do not edit ----

def combo_filter(options, query)
  said = (query ?? "").strip().downcase()
  return options if said == ""

  options.filter(fn(o) { o.to_s.downcase().index_of(said) >= 0 })
end

def command_row(it)
  return {"id": it.to_s, "label": it.to_s, "hint": "", "group": ""} unless it.class == "hash"

  {
    "id": (it["id"] ?? it["label"]).to_s,
    "label": (it["label"] ?? it["id"]).to_s,
    "hint": (it["hint"] ?? "").to_s,
    "group": (it["group"] ?? "").to_s
  }
end

def command_match(items, query)
  said = (query ?? "").strip().downcase()
  out = []
  for it in items
    row = command_row(it)
    hay = (row["label"] + " " + row["hint"] + " " + row["group"]).downcase()
    out = out.concat([row]) if said == "" || hay.index_of(said) >= 0
  end
  out
end

def tag_highlight(count, at, step)
  return -1 if count <= 0

  return step > 0 ? 0 : count - 1 if at < 0

  next_at = at + step
  return count - 1 if next_at < 0
  return 0 if next_at >= count

  next_at
end

def check(label, got, want)
  if got == want
    print("ok   " + label)
  else
    a = got
    b = want
    a = "[" + got.join(", ") + "]" if got.class == "array"
    b = "[" + want.join(", ") + "]" if want.class == "array"
    print("FAIL " + label + " got " + a.to_s + " want " + b.to_s)
  end
end

def ids_of(rows)
  rows.map(fn(r) { r["id"] })
end

# ---- combobox ----

STATUSES = ["Any status", "Draft", "Confirmed", "Picked", "Invoiced", "Late"]

check("an empty draft offers the whole list", combo_filter(STATUSES, ""), STATUSES)
check("a missing draft offers the whole list", combo_filter(STATUSES, null), STATUSES)
check("a draft matches anywhere", combo_filter(STATUSES, "inv"), ["Invoiced"])
check("matching ignores case", combo_filter(STATUSES, "DRAFT"), ["Draft"])
check("space around the draft does not count", combo_filter(STATUSES, "  late  "), ["Late"])
check("a word nobody has offers nothing", combo_filter(STATUSES, "zzz"), [])
check("Any matches Any status", combo_filter(STATUSES, "any"), ["Any status"])

# ---- commands ----

CMDS = [
  {"id": "nav:Orders", "label": "Orders", "group": "Go to", "hint": ""},
  {"id": "export", "label": "Export orders", "group": "Orders", "hint": "CSV"},
  "Theme"
]

check("a string is a label that is its own id", command_row("Theme")["id"], "Theme")
check("an empty query offers everything", ids_of(command_match(CMDS, "")), ["nav:Orders", "export", "Theme"])
check("a query matches a label", ids_of(command_match(CMDS, "export")), ["export"])
check("a query matches a hint", ids_of(command_match(CMDS, "csv")), ["export"])
check("a query matches a group", ids_of(command_match(CMDS, "go to")), ["nav:Orders"])
check("matching ignores case", ids_of(command_match(CMDS, "ORDERS")), ["nav:Orders", "export"])
check("nothing matching is empty", ids_of(command_match(CMDS, "zzz")), [])

check("down from nowhere takes the first", tag_highlight(3, -1, 1), 0)
check("up wraps to the last", tag_highlight(3, 0, -1), 2)
