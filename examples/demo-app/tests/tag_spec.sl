# The tag algebra, tested without a client.
#
# A tag field is four decisions — what a typed word does to the list, what
# removing one leaves, which suggestions are still worth offering, and where
# the highlight lands. All four are pure, and all four are wrong in ways that
# only show up on the edges: the word that is already there in another case,
# the field that is full, the arrow that runs off the end of the panel.
#
# Run with a soli that can see the catalogue:
#
#   soli tests/tag_spec.sl
#
# The definitions are copied in by `tools/sync_split_spec.py` rather than
# imported, because `app/controllers` is loaded by the server and not by a
# bare script.

# ---- copied from app/controllers/eui_builders.sl, do not edit ----

def tag_add(tags, text, o = {})
  said = (text ?? "").strip()
  return tags if said == ""

  for t in tags
    return tags if t.downcase() == said.downcase()
  end
  cap = o["max"] ?? 0
  return tags if cap > 0 && tags.length() >= cap

  # `concat` grows the array it is called on, so copy before growing: the
  # caller still holds `tags`, and the state hash it came out of holds it too.
  tags.slice(0, tags.length()).concat([said])
end

def tag_remove(tags, at)
  return tags if at < 0 || at >= tags.length()

  tags.slice(0, at).concat(tags.slice(at + 1, tags.length()))
end

def tag_suggest(all, tags, draft, limit)
  said = (draft ?? "").strip().downcase()
  out = []
  for one in all
    if out.length() < limit
      taken = false
      for t in tags
        taken = true if t.downcase() == one.downcase()
      end
      fits = said == "" || one.downcase().index_of(said) >= 0
      out = out.concat([one]) if !taken && fits
    end
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
    # `[].to_s` raises, and every interesting comparison here is a list.
    a = got
    b = want
    a = "[" + got.join(", ") + "]" if got.class == "array"
    b = "[" + want.join(", ") + "]" if want.class == "array"
    print("FAIL " + label + " got " + a.to_s + " want " + b.to_s)
  end
end

# ---- what a typed word does ----

check("a word becomes a tag", tag_add([], "lyon"), ["lyon"])
check("surrounding space is not part of it", tag_add([], "  lyon  "), ["lyon"])
check("nothing typed adds nothing", tag_add(["a"], "   "), ["a"])
check("a missing draft adds nothing", tag_add(["a"], null), ["a"])
check("the same word twice is one tag", tag_add(["Lyon"], "Lyon"), ["Lyon"])
check("case does not make it a new tag", tag_add(["Lyon"], "lyon"), ["Lyon"])
check("the first spelling is the one kept", tag_add(["Lyon"], "LYON"), ["Lyon"])
check("a tag joins the end", tag_add(["a"], "b"), ["a", "b"])
check("a full list takes no more", tag_add(["a", "b"], "c", {"max": 2}), ["a", "b"])
check("room under the cap is still room", tag_add(["a"], "b", {"max": 2}), ["a", "b"])
check("no cap means no limit", tag_add(["a", "b"], "c"), ["a", "b", "c"])

# The caller's list is never edited underneath it — `concat` grows its
# receiver, so a builder that rejects a word must hand back what it was given
# and a builder that accepts one must not have already changed the state the
# server is about to compare against.
held = ["a"]
grown = tag_add(held, "b")
check("the list that came in is left alone", held, ["a"])
check("and the new one has both", grown, ["a", "b"])

# ---- what removing one leaves ----

check("the middle goes", tag_remove(["a", "b", "c"], 1), ["a", "c"])
check("the first goes", tag_remove(["a", "b", "c"], 0), ["b", "c"])
check("the last goes", tag_remove(["a", "b", "c"], 2), ["a", "b"])
check("the only one goes", tag_remove(["a"], 0), [])
check("past the end removes nothing", tag_remove(["a", "b"], 2), ["a", "b"])
check("before the start removes nothing", tag_remove(["a", "b"], -1), ["a", "b"])
check("an empty list survives a remove", tag_remove([], 0), [])

kept = ["a", "b"]
tag_remove(kept, 0)
check("removing does not edit the list it was given", kept, ["a", "b"])

# ---- which suggestions are worth offering ----

ALL = ["Lyon", "Lyon-Sud", "Analytics", "Billing"]

check("an empty draft offers everything", tag_suggest(ALL, [], "", 9), ALL)
check("a draft matches anywhere in the word", tag_suggest(ALL, [], "ly", 9), ["Lyon", "Lyon-Sud", "Analytics"])
check("matching ignores case", tag_suggest(ALL, [], "LY", 9), ["Lyon", "Lyon-Sud", "Analytics"])
check("a chosen tag is not offered again", tag_suggest(ALL, ["Lyon"], "ly", 9), ["Lyon-Sud", "Analytics"])
check("exclusion ignores case too", tag_suggest(ALL, ["lyon"], "ly", 9), ["Lyon-Sud", "Analytics"])
check("the limit cuts the tail", tag_suggest(ALL, [], "", 2), ["Lyon", "Lyon-Sud"])
check("a word nobody has offers nothing", tag_suggest(ALL, [], "zzz", 9), [])
check("space around the draft does not count", tag_suggest(ALL, [], "  bill  ", 9), ["Billing"])
check("a missing draft offers everything", tag_suggest(ALL, [], null, 9), ALL)
check("a limit of nothing offers nothing", tag_suggest(ALL, [], "", 0), [])
check("everything chosen leaves nothing to offer", tag_suggest(ALL, ALL, "", 9), [])

# The panel is what the arrow keys walk, so the limit has to hold before the
# exclusion as well as after it — four words, one taken, a limit of two is two
# suggestions and not one.
check("the limit counts what is offered, not what was scanned", tag_suggest(ALL, ["Lyon"], "", 2), ["Lyon-Sud", "Analytics"])

# ---- where the highlight lands ----

check("nothing to walk has no highlight", tag_highlight(0, -1, 1), -1)
check("down from nowhere takes the first", tag_highlight(3, -1, 1), 0)
check("up from nowhere takes the last", tag_highlight(3, -1, -1), 2)
check("down moves down", tag_highlight(3, 0, 1), 1)
check("up moves up", tag_highlight(3, 2, -1), 1)
check("down off the end wraps to the first", tag_highlight(3, 2, 1), 0)
check("up off the start wraps to the last", tag_highlight(3, 0, -1), 2)
check("one suggestion stays put going down", tag_highlight(1, 0, 1), 0)
check("one suggestion stays put going up", tag_highlight(1, 0, -1), 0)

# A panel that shrank under the highlight must not leave it pointing past the
# end — the server re-suggests on every keystroke, so this is the common case
# and not the odd one.
check("a highlight past a shrunken panel comes back", tag_highlight(2, 5, 1), 0)
check("and walking up off it lands inside", tag_highlight(2, 5, -1), 0)
