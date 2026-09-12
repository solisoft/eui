# The selection model, tested without a client.
#
# A selection is `{"ids": [...], "all": Bool, "scope": Str}` and the flag
# reverses what the list means: chosen, or chosen-except. That reversal is
# invisible on screen — a ticked row looks the same either way — and wrong in
# both directions if `selection_toggle` or `selection_count` gets it backwards,
# so it is exactly the part worth checking directly.
#
# Run with a soli that can see the catalogue:
#
#   soli tests/selection_spec.sl
#
# The definitions are copied in by `tools/sync_split_spec.py` rather than
# imported, because `app/controllers` is loaded by the server and not by a
# bare script. When the catalogue becomes a package this file imports it
# instead and the copy goes away.

# ---- copied from app/controllers/eui_builders.sl, do not edit ----

def selection(ids = [], scope = "")
  {"ids": ids, "all": false, "scope": scope}
end

def selection_scope(sel)
  (sel ?? {})["scope"] ?? ""
end

def selection_all(sel)
  {"ids": [], "all": true, "scope": selection_scope(sel)}
end

def selection_none(sel)
  {"ids": [], "all": false, "scope": selection_scope(sel)}
end

def selection_scoped(sel, scope)
  return selection([], scope) if sel.nil? || selection_scope(sel) != scope

  sel
end

def selection_ids_of(sel)
  (sel ?? {})["ids"] ?? []
end

def selection_all?(sel)
  (sel ?? {})["all"] == true
end

def selection_has?(sel, id)
  inside = selection_ids_of(sel).includes?(id)
  return !inside if selection_all?(sel)

  inside
end

def selection_toggle(sel, id)
  ids = selection_ids_of(sel)
  kept = ids.filter(fn(x) { x != id })
  return {"ids": kept, "all": selection_all?(sel), "scope": selection_scope(sel)} if kept.length() < ids.length()

  {"ids": kept.concat([id]), "all": selection_all?(sel), "scope": selection_scope(sel)}
end

def selection_count(sel, total)
  held = selection_ids_of(sel).length()
  return held unless selection_all?(sel)
  return 0 if held > total

  total - held
end

def selection_empty?(sel, total)
  selection_count(sel, total) == 0
end

def selection_mark(sel, total)
  count = selection_count(sel, total)
  return "none" if count == 0
  return "all" if count >= total

  "some"
end

def selection_index(sel)
  index = {}
  for id in selection_ids_of(sel)
    index[id] = true
  end
  index
end

def selection_in?(index, sel, id)
  inside = index[id] == true
  return !inside if selection_all?(sel)

  inside
end

def selection_ids(sel, every)
  return selection_ids_of(sel) unless selection_all?(sel)

  every.filter(fn(id) { !selection_ids_of(sel).includes?(id) })
end

def check(label, got, want)
  if got == want
    print("ok   " + label)
  else
    print("FAIL " + label + " got " + got.to_s + " want " + want.to_s)
  end
end

LEDGER = 10000

# ---- the ordinary reading: `ids` are what is chosen -------------------------

empty = selection([], "unpaid")
check("a fresh selection holds nothing", selection_count(empty, LEDGER), 0)
check("and says so", selection_mark(empty, LEDGER), "none")
check("nothing is in it", selection_has?(empty, "AX-0003"), false)

one = selection_toggle(empty, "AX-0003")
check("toggling adds", selection_count(one, LEDGER), 1)
check("the added row is in it", selection_has?(one, "AX-0003"), true)
check("its neighbour is not", selection_has?(one, "AX-0004"), false)
check("one of ten thousand is mixed", selection_mark(one, LEDGER), "some")

check("toggling twice is the identity", selection_count(selection_toggle(one, "AX-0003"), LEDGER), 0)
check("and the scope rides along", selection_scope(selection_toggle(one, "AX-0003")), "unpaid")

three = selection_toggle(selection_toggle(one, "AX-0004"), "AX-0005")
check("three chosen", selection_count(three, LEDGER), 3)
check("all three of three is all", selection_mark(three, 3), "all")

# ---- the reversed reading: `ids` are the exceptions -------------------------

every = selection_all(three)
check("select all drops the list", selection_ids_of(every).length(), 0)
check("and counts the whole ledger", selection_count(every, LEDGER), LEDGER)
check("without holding ten thousand ids", selection_ids_of(every).length(), 0)
check("a row nobody has sent is chosen", selection_has?(every, "AX-9999"), true)
check("the mark is all", selection_mark(every, LEDGER), "all")

# The same call, the other meaning: under `all` a toggle pushes an exception.
short = selection_toggle(every, "AX-0042")
check("toggling under all removes", selection_has?(short, "AX-0042"), false)
check("its neighbour is untouched", selection_has?(short, "AX-0043"), true)
check("nine thousand nine hundred and ninety-nine", selection_count(short, LEDGER), 9999)
check("which is mixed", selection_mark(short, LEDGER), "some")
check("one exception, not ten thousand ids", selection_ids_of(short).length(), 1)

back = selection_toggle(short, "AX-0042")
check("toggling back restores it", selection_has?(back, "AX-0042"), true)
check("and the count with it", selection_count(back, LEDGER), LEDGER)

# Every row excepted is none chosen, however the flag reads.
gone = selection_toggle(selection_toggle(selection_all(empty), "a"), "b")
check("two of two excepted is none", selection_count(gone, 2), 0)
check("and the mark says none", selection_mark(gone, 2), "none")
check("a count cannot go below zero", selection_count(gone, 1), 0)

check("clearing forgets the flag", selection_all?(selection_none(every)), false)
check("and the list", selection_count(selection_none(short), LEDGER), 0)

# ---- the index, which must agree with the direct read -----------------------

index = selection_index(short)
check("the index agrees on an excepted row", selection_in?(index, short, "AX-0042"), false)
check("and on one that is not", selection_in?(index, short, "AX-0043"), true)
check("the index holds only exceptions", index.keys().length(), 1)

plain = selection_index(three)
check("and the other way round", selection_in?(plain, three, "AX-0003"), true)
check("for a row never mentioned", selection_in?(plain, three, "AX-9999"), false)

# A toggled-off row leaves the index rather than sitting in it as `false`.
# It reads the same either way; the difference is that `.keys().length()`
# counts it, so a count taken from the hash would start lying here.
off = selection_index(selection_toggle(one, "AX-0003"))
check("an unticked row leaves the index", off.keys().length(), 0)

# ---- materialising, for a list short enough to hold ------------------------

check("the chosen ids, spelled out", selection_ids(three, []).length(), 3)
check("under all it is the complement", selection_ids(short, ["AX-0042", "AX-0043"]), ["AX-0043"])

# ---- scope: `all` is only true of the query it was clicked in ---------------

check("the same query keeps the selection", selection_count(selection_scoped(short, "unpaid"), LEDGER), 9999)
check("a different query does not", selection_count(selection_scoped(short, "any"), LEDGER), 0)
check("and the new scope sticks", selection_scope(selection_scoped(short, "any")), "any")

# ---- nothing at all reads as nothing chosen --------------------------------

check("a selection that was never made is empty", selection_count(null, LEDGER), 0)
check("and holds nothing", selection_has?(null, "AX-0003"), false)
check("and is not all", selection_all?(null), false)
check("and says none", selection_mark(null, LEDGER), "none")
