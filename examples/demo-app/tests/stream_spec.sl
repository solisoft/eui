# The two decisions inside a live chart, tested without a client.
#
# Everything else `chart_stream` does is arithmetic a screenshot checks
# better than an assertion can. These two are not: one of them is only wrong
# while the data is moving, and the other is only wrong at its boundaries.
#
#   soli test tests/stream_spec.sl --no-coverage
#
# The definitions are copied in by `tools/sync_split_spec.py` rather than
# imported, because the catalogue is loaded by the server and a bare script
# cannot reach it.

# ---- copied from the catalogue, do not edit ----

def chart_stream_ceiling(sc_max, sc_mark)
  sc_want = sc_max > sc_mark ? sc_max : sc_mark + sc_mark / 8
  sc_step = 20
  sc_step = 50 if sc_want > 400
  sc_step = 5 if sc_want < 40
  sc_steps = int(sc_want / sc_step) + 1
  sc_steps * sc_step
end

def chart_band_role(sb_pct)
  return "danger.base" if sb_pct >= 90
  return "warning.base" if sb_pct >= 70
  return "success.base" if sb_pct >= 40

  # `info.base` and not `success.subtle`, which is what this had first. A
  # `.subtle` role is a tint meant to sit behind text, and a strip of them
  # beside three saturated bands does not read as "quiet" — it reads as a hole
  # in the drawing, as though those readings were missing. The quiet end of a
  # load ramp still has to be a colour. Blue also gives the ramp a fourth hue
  # rather than a fourth lightness, which is what a reader who cannot separate
  # the green from the amber has to go on.
  "info.base"
end

def check(label, got, want)
  assert_eq(got, want)
end

# --- a scale that does not breathe ------------------------------------

# The failure this exists to stop: a window whose largest reading wanders
# between 121 and 138 rescales the whole plot on every tick, and a line that
# did not move appears to. Every one of these tops out at the same number.
check("121 and 138 share a ceiling", chart_stream_ceiling(121, 96), chart_stream_ceiling(138, 96))
check("and so do 101 and 119", chart_stream_ceiling(101, 96), chart_stream_ceiling(119, 96))
check("the ceiling is the next step up", chart_stream_ceiling(121, 96), 140)

# It is a ceiling, so nothing in the window touches the top of the plot.
check("a reading on the step still gets room", chart_stream_ceiling(140, 96), 160)
check("and one just under it does not", chart_stream_ceiling(139, 96), 140)

# The target is drawn across the plot, so it has to be inside it even when
# every line is a long way below.
check("a quiet window still shows its target", chart_stream_ceiling(30, 96) > 96, true)

# The step follows the magnitude: five below forty, twenty in the middle,
# fifty past four hundred. A chart of single figures with a step of twenty
# would be one hairline and a lot of white.
check("small numbers step by five", chart_stream_ceiling(12, 0), 15)
check("ordinary ones by twenty", chart_stream_ceiling(121, 0), 140)
check("large ones by fifty", chart_stream_ceiling(920, 0), 950)

# --- the load ramp ----------------------------------------------------

# Four bands, and what matters is where each one starts. The middle of a
# band cannot be wrong without an end of it being wrong first.
check("nothing is idle", chart_band_role(0), "info.base")
check("and so is just under two fifths", chart_band_role(39), "info.base")
check("two fifths is working", chart_band_role(40), "success.base")
check("just under seven tenths still is", chart_band_role(69), "success.base")
check("seven tenths is busy", chart_band_role(70), "warning.base")
check("just under nine tenths still is", chart_band_role(89), "warning.base")
check("nine tenths is over", chart_band_role(90), "danger.base")
check("and so is everything past the ceiling", chart_band_role(140), "danger.base")
