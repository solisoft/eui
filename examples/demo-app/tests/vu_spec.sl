# The level meter, tested without a client.
#
# A meter is two decisions and a lot of arithmetic: which of the three
# colours a segment belongs to, and how many of them the reading lights.
# Both are wrong in ways that only show on the edges — the reading of
# exactly zero that must light nothing, the reading of a hundred that must
# light everything including the last red one, and the peak-hold marker
# that sits above the bar rather than inside it.
#
# Run with a soli that can see the catalogue:
#
#   soli tests/vu_spec.sl
#
# The definitions are copied in by `tools/sync_split_spec.py` rather than
# imported, because `app/controllers` is loaded by the server and not by a
# bare script.

# ---- copied from the catalogue, do not edit ----

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

def text(content, style)
  {
    "k": "text",
    "t": content,
    "s": style
  }
end

def vu_zone(i, n)
  at = n > 1 ? (i * 100) / (n - 1) : 0
  return "danger.base" if at >= 88
  return "warning.base" if at >= 70
  "success.base"
end

def vu_segment(i, n, lit, axis)
  thin = axis == "v" ? 4 : 3
  long = axis == "v" ? 3 : 4
  {
    "k": "box",
    "s": {
      "width": axis == "v" ? 18 : long,
      "height": axis == "v" ? long : 10,
      "radius": 1,
      "bg": vu_zone(i, n),
      "opacity": lit == true ? 255 : 38,
      "transition": "fast",
      "shrink": 0,
      # Both axes now: a segment shares the length the strip was given, the
      # way the horizontal one always has. Vertically it used to be three
      # pixels and no growth, so the bar came out the height of its contents
      # — 58 px — while the legend beside it stretched to 96 and no mark
      # stood against the segment it names.
      "grow": 1,
      "min_width": axis == "v" ? 0 : thin,
      "min_height": axis == "v" ? long : 0
    }
  }
end

def vu_strip(level, o = {})
  axis = o["axis"] ?? "h"
  n = o["segments"] ?? 12
  now = (level ?? 0).clamp(0, 100)
  hold = (o["peak"] ?? -1).clamp(-1, 100)
  lit_to = (now * n) / 100
  hold_at = hold >= 0 ? (hold * n) / 100 : -1
  cells = range(0, n).map(fn(i) {
    vu_segment(i, n, i < lit_to || i == hold_at, axis)
  })
  cells = cells.reverse() if axis == "v"
  # The vertical strip needs a length to share out, exactly as the
  # horizontal one takes the full width. `height` names it; the legend is
  # given the same one, which is the whole of making the two line up.
  tall = o["height"] ?? 96
  strip = axis == "v"
    ? column({"gap": 0, "align": "center", "shrink": 0, "height": tall}, cells)
    : row({"gap": 0, "align": "center", "width": "100%"}, cells)
  strip["s"]["gap"] = 1
  strip["p"] = {
    "role": "progress",
    "label": o["label"] ?? "Level",
    "value_now": now,
    "value_min": 0,
    "value_max": 100
  }
  strip
end

def vu_scale(o = {})
  axis = o["axis"] ?? "h"
  marks = (o["marks"] ?? ["-20", "-10", "-6", "-3", "0", "+3"]).map(fn(m) {
    text(m, {"size": 0, "fg": "text.muted", "font": "mono"})
  })
  # The mirror of the horizontal case, and it was not one. Laid out at its
  # natural height, six marks of text stand about twice as tall as twelve
  # three-pixel segments, so the legend ran past the strip and no mark stood
  # beside the segment it names. `between` is what the row already does
  # across its width.
  #
  # And no `height`: the parent row is `align: "stretch"`, so this column is
  # already the height of the strips beside it. Asking for `100%` on top of
  # that resolved against an ancestor instead and laid the meter out 6 232
  # pixels tall — measured, after writing it.
  if axis == "v"
    tall = o["height"] ?? 96
    return column({"gap": 0, "justify": "between", "align": "end", "shrink": 0, "height": tall}, marks.reverse())
  end


  row({"gap": 0, "justify": "between", "width": "100%"}, marks)
end

def vu_meter(level, o = {})
  axis = o["axis"] ?? "h"
  pair = level.is_a?("array") == true ? level : [level]
  vals = pair.filter(fn(v) { v != null })
  # One strip per reading, whatever the readings are. Two is the pair a
  # `level` event carries and the shape a deck showed, so two is named left
  # and right; one is named nothing, because there is nothing to tell it
  # apart from. Anything else — bands of a spectrum, a channel per voice —
  # is the same drawing and only wants its own words, so `labels` supplies
  # them. A reader who cannot see the bars is who this is for: without a
  # name each strip announces itself as "Level" and the screen reader says
  # the same thing five times.
  sides = o["labels"].is_a?("array") == true
    ? o["labels"].map(fn(l) { " " + l.to_s })
    : (vals.length() == 2 ? [" left", " right"] : [""])
  # The hold marker has the same shape as the reading it follows: one
  # number for one strip, a pair for two. A single number shared by both
  # would put the louder channel's marker over the quieter one, which is
  # the one thing a peak-hold must not do.
  holds = (o["peak"] ?? -1).is_a?("array") == true ? o["peak"] : [o["peak"] ?? -1, o["peak"] ?? -1]
  strips = range(0, vals.length()).map(fn(i) {
    vu_strip(vals[i], o.merge({
      "peak": holds[i] ?? -1,
      "label": (o["label"] ?? "Level") + (sides[i] ?? "")
    }))
  })
  body = axis == "v"
    ? row({"gap": 1, "align": "end", "shrink": 0}, strips)
    : column({"gap": 1, "width": "100%"}, strips)
  return body if o["scale"] == false
  axis == "v"
    ? row({"gap": 2, "align": "stretch", "shrink": 0}, [body, vu_scale(o)])
    : column({"gap": 1, "width": "100%"}, [body, vu_scale(o)])
end

def check(label, got, want)
  assert_eq(got, want)
end

# How many segments a strip has lit, read back off the tree it returns.
def lit_of(strip)
  strip["c"].filter(fn(c) { c["s"]["opacity"] == 255 }).length()
end

def roles_of(strip)
  strip["c"].map(fn(c) { c["s"]["bg"] })
end

# --- the ramp ---------------------------------------------------------

# Twelve segments: the first eight are safe, two are warnings, two are
# not. The boundaries are the whole point of the function, so they are
# what gets checked rather than the middle of each band.
check("the bottom of the scale is green", vu_zone(0, 12), "success.base")
check("and stays green to just under seven tenths", vu_zone(7, 12), "success.base")
check("amber starts at seven tenths", vu_zone(8, 12), "warning.base")
check("red starts at just under nine", vu_zone(11, 12), "danger.base")

# One segment is a whole meter, and it is not a warning.
check("a single segment is green", vu_zone(0, 1), "success.base")

# --- how much is lit --------------------------------------------------

check("silence lights nothing", lit_of(vu_strip(0, {"segments": 12})), 0)
check("full scale lights every one", lit_of(vu_strip(100, {"segments": 12})), 12)
check("half lights half", lit_of(vu_strip(50, {"segments": 12})), 6)

# A reading the server got wrong does not make a meter longer than itself.
check("over a hundred is still a hundred", lit_of(vu_strip(150, {"segments": 12})), 12)
check("under zero is still zero", lit_of(vu_strip(-20, {"segments": 12})), 0)

# --- peak hold --------------------------------------------------------

# The marker sits above the bar, so it adds one to what is lit rather
# than being swallowed by it. That is the whole visual point: the bar
# falls away and the marker stays.
check("a peak above the bar is one more lit", lit_of(vu_strip(20, {"segments": 10, "peak": 80})), 3)
check("a peak inside the bar adds nothing", lit_of(vu_strip(80, {"segments": 10, "peak": 20})), 8)
check("no peak asked for, none drawn", lit_of(vu_strip(20, {"segments": 10})), 2)

# --- the two axes -----------------------------------------------------

# Vertical is the same segments the other way up: the loudest is at the
# top, so the list is reversed and the last one is the quiet green.
check("horizontal runs quiet to loud", roles_of(vu_strip(50, {"segments": 12}))[11], "danger.base")
check("vertical runs loud to quiet", roles_of(vu_strip(50, {"segments": 12, "axis": "v"}))[0], "danger.base")
check("a horizontal strip is a row", vu_strip(50, {})["s"]["display"], "row")
check("a vertical strip is a column", vu_strip(50, {"axis": "v"})["s"]["display"], "column")

# --- what a reader is told --------------------------------------------

# A meter is a reading, not a decoration, so it says so — and it says the
# reading it was given rather than the number of segments it happened to
# light.
check("a strip declares its role", vu_strip(37, {})["p"]["role"], "progress")
check("and the reading it was given", vu_strip(37, {})["p"]["value_now"], 37)
check("on the scale it was given", vu_strip(37, {})["p"]["value_max"], 100)

# --- one channel or two -----------------------------------------------

# A `level` event carries a pair, which is what the front of a deck
# showed. One number is one strip; two are two, and they say which is
# which rather than both calling themselves "Level".
check("one reading is one strip", vu_meter(50, {"scale": false})["c"].length(), 1)
check("a pair is two", vu_meter([50, 30], {"scale": false})["c"].length(), 2)
check("and they are named apart", vu_meter([50, 30], {"scale": false})["c"][0]["p"]["label"], "Level left")
check("both of them", vu_meter([50, 30], {"scale": false})["c"][1]["p"]["label"], "Level right")
check("a lone reading is not called left", vu_meter(50, {"scale": false})["c"][0]["p"]["label"], "Level")

# The scale comes with it unless it is turned off, and it is the legend a
# deck printed rather than a percentage.
check("the legend is dB", vu_scale({})["c"].length(), 6)
check("and zero is on it", vu_scale({})["c"][4]["t"], "0")
