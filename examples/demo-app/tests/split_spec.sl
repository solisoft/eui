# The split-pane geometry and its drag, tested without a client.
#
# These functions are pure — a size, a fraction and two minimums in, two
# panel widths out — so they can be checked directly, and they are the part
# where an off-by-one is invisible on screen and wrong on every drag.
#
# Run with a soli that can see the catalogue:
#
#   soli tests/split_spec.sl
#
# The definitions are copied in by `tools/sync_split_spec.py` rather than
# imported, because `app/controllers` is loaded by the server and not by a
# bare script. When the catalogue becomes a package this file imports it
# instead and the copy goes away.

# ---- copied from app/controllers/eui_builders.sl, do not edit ----

def split_span(extent, bar)
  span = extent - bar
  span < 0 ? 0 : span
end

def split_sizes(extent, fraction, min_a, min_b, bar)
  span = split_span(extent, bar)
  return [0, 0] if span <= 0

  a = int((span * fraction / 1000.0).round())
  room = span - min_b
  a = room if a > room
  a = min_a if a < min_a
  a = 0 if a < 0
  a = span if a > span
  [a, span - a]
end

def split_at(extent, at, min_a, min_b, bar)
  span = split_span(extent, bar)
  return 500 if span <= 0

  a = int(at) - int(bar / 2)
  room = span - min_b
  a = room if a > room
  a = min_a if a < min_a
  a = 0 if a < 0
  a = span if a > span
  int((a * 1000.0 / span).round())
end

def check(label, got, want)
  if got == want
    print("ok   " + label)
  else
    print("FAIL " + label + " got " + got.to_s + " want " + want.to_s)
  end
end

check("half", split_sizes(806, 500, 80, 80, 6), [400, 400])
check("all left clamps to min_b", split_sizes(806, 1000, 80, 80, 6), [720, 80])
check("all right clamps to min_a", split_sizes(806, 0, 80, 80, 6), [80, 720])
check("degenerate width", split_sizes(4, 500, 80, 80, 6), [0, 0])
check("drag to centre", split_at(806, 403, 80, 80, 6), 500)
check("drag past the left floor", split_at(806, 10, 80, 80, 6), 100)
check("drag past the right floor", split_at(806, 900, 80, 80, 6), 900)
check("degenerate stays centred", split_at(4, 2, 80, 80, 6), 500)

# Every pixel a drag can land on rebuilds itself exactly.
worst = 0
for at in range(83, 724)
  f = split_at(806, at, 80, 80, 6)
  back = split_sizes(806, f, 80, 80, 6)[0]
  d = back - (at - 3)
  d = 0 - d if d < 0
  worst = d if d > worst
end
check("round trip is exact across the whole travel", worst, 0)

# A vertical split is the same arithmetic on the other axis.
check("column split", split_sizes(406, 250, 60, 60, 6), [100, 300])
