# `tw()`, tested without a client.
#
#   soli test tests/tw_spec.sl --no-coverage
#
# Three things are worth pinning, and none of them shows in a screenshot
# until it is wrong on someone else's page: that a class becomes the style
# key and the scale index it names, that a class with no equivalent is
# refused *by name* rather than dropped, and that everything tw() does emit
# is something the encoder takes. The last is checked against the real
# encoder: `eui_render` is the same `style_record` a session goes through,
# and it raises on a key or a value it does not know.
#
# The definitions are copied in by `tools/sync_split_spec.py` rather than
# imported, because `app/controllers` is loaded by the server and not by a
# bare script. The memo is a module constant, which the copy does not carry,
# so it is declared here.

TW_MEMO = {}
TW_MEMO_CAP = 1024

# ---- copied from the catalogue, do not edit ----

def node(kind, style, children)
  # Not `n`: a bare assignment here would write the caller's `n`, and half
  # the catalogue calls this with one of its own in hand.
  nd_made = {
    "k": kind,
    "s": style,
    "c": children
  }
  nd_made = tw_node(nd_made) unless style.nil? || style["tw"].nil?
  return tw_auto_place(nd_made) if tw_autos?(nd_made["c"])

  nd_made
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
  return tw_text(content, style) unless style.nil? || style["tw_case"].nil?

  {
    "k": "text",
    "t": content,
    "s": style
  }
end

def tw_split(classes)
  return [] if classes.nil?

  sp_list = classes.class == "array" ? classes : classes.to_s.replace("\n", " ").replace("\t", " ").split(" ")
  sp_list.map(fn(w) { w.to_s.strip() }).filter(fn(w) { w != "" })
end

def tw(classes, width = nil)
  tw_t = tw_raw(classes, width)
  tw_dv = tw_t["divide"]
  if tw_dv.keys().length() > 0
    throw tw_no(tw_dv["c"], "a divider borders the children, so it is written where they are: node(), row() or column() with \"tw\"")
  end
  unless tw_t["s"]["tw_reverse"].nil?
    throw tw_no(tw_t["s"]["tw_reverse"], "a reversed box is its children in the other order, so it is written where they are: node() with \"tw\"")
  end
  if tw_t["gaps"].keys().length() > 0
    tw_dir = tw_t["s"]["display"]
    if tw_dir.nil?
      tw_first = tw_t["gaps"][tw_t["gaps"].keys()[0]]["c"]
      throw tw_no(tw_first, "spacing children depends on which way they run: write flex or flex-col with it, or put it on row() or column()")
    end
    tw_t["s"] = tw_gap_settle(tw_t["s"], tw_t["gaps"], tw_dir)
    tw_t["gaps"] = {}
  end
  tw_t
end

def tw_raw(classes, width)
  twr_key = classes.class == "array" ? classes.join(" ") : classes.to_s
  twr_rank = tw_screen(width)
  twr_key = twr_key + " @" + str(twr_rank) if tw_responsive?(twr_key)
  twr_hit = TW_MEMO[twr_key]
  return tw_copy(twr_hit) unless twr_hit.nil?

  twr_fresh = tw_parse(classes, twr_rank)
  TW_MEMO[twr_key] = twr_fresh if TW_MEMO.keys().length() < TW_MEMO_CAP
  tw_copy(twr_fresh)
end

def tw_copy(t)
  {
    "s": t["s"].merge({}),
    "hover": t["hover"].merge({}),
    "press": t["press"].merge({}),
    "focus": t["focus"].merge({}),
    "disabled": t["disabled"].merge({}),
    "props": t["props"].merge({}),
    "gaps": t["gaps"].merge({}),
    "divide": t["divide"].merge({})
  }
end

def tw_blank()
  {"s": {}, "hover": {}, "press": {}, "focus": {}, "disabled": {}, "props": {}, "gaps": {}, "divide": {}}
end

def tw_screen(width)
  return -1 if width.nil?
  return 5 if width >= 1536
  return 4 if width >= 1280
  return 3 if width >= 1024
  return 2 if width >= 768
  return 1 if width >= 640

  0
end

def tw_screen_rank(prefix)
  return 1 if prefix == "sm"
  return 2 if prefix == "md"
  return 3 if prefix == "lg"
  return 4 if prefix == "xl"
  return 5 if prefix == "2xl"

  0
end

def tw_screen_px(rank)
  return 640 if rank == 1
  return 768 if rank == 2
  return 1024 if rank == 3
  return 1280 if rank == 4

  1536
end

def tw_responsive?(key)
  key.index_of("sm:") >= 0 || key.index_of("md:") >= 0 || key.index_of("lg:") >= 0 || key.index_of("xl:") >= 0
end

def tw_word(word)
  twwd_bits = word.split(":")
  twwd_bp = 0
  twwd_state = "s"
  twwd_said = ""
  for twwd_i in range(0, twwd_bits.length() - 1)
    twwd_pre = twwd_bits[twwd_i]
    twwd_rank = tw_screen_rank(twwd_pre)
    if twwd_rank > 0
      throw tw_no(word, "one breakpoint per class; the larger one alone says the same") if twwd_bp > 0
      twwd_bp = twwd_rank
    else
      throw tw_no(word, "one state per class; hover:focus: is two states at once") if twwd_state != "s"
      twwd_state = tw_variant(twwd_pre, word)
      twwd_said = twwd_pre
    end
  end
  twwd_name = twwd_bits[twwd_bits.length() - 1]
  {"bp": twwd_bp, "state": twwd_state, "name": twwd_name, "whole": word, "noop": tw_focus_ring?(twwd_said, twwd_name)}
end

def tw_focus_ring?(said, name)
  return true if name == "outline-none" || name == "outline-0"
  return true if (said == "focus" || said == "focus-visible") && name.starts_with?("outline")
  return true if said == "focus-visible" && name.starts_with?("ring")

  false
end

def tw_needs_width(whole, rank)
  "tw: '" + whole + "' needs the viewport width — a breakpoint class applies from " + str(tw_screen_px(rank)) + " px up, and tw() was not given one: write tw(classes, width), tw_style(classes, false, width), or \"vw\": width beside \"tw\" on a node"
end

def tw_parse(classes, rank)
  twp_words = tw_split(classes).map(fn(w) { tw_word(w) })
  if rank < 0
    for twp_w in twp_words
      throw tw_needs_width(twp_w["whole"], twp_w["bp"]) if twp_w["bp"] > 0
    end
  end
  twp_out = tw_blank()
  for twp_pass in ["rest", "state"]
    for twp_level in range(0, 6)
      # Which rule of the stylesheet a class is, for the one place its order
      # decides and the order it was written in does not: a side's border
      # colour over the whole box's (tw_bc_settle).
      twp_rule = (twp_pass == "state" ? 10 : 0) + twp_level
      for twp_w in twp_words
        twp_stated = twp_w["state"] != "s"
        if twp_w["noop"] != true && twp_w["bp"] == twp_level && twp_stated == (twp_pass == "state")
          if twp_level <= rank || twp_level == 0
            twp_out = tw_take(twp_out, twp_w["state"], twp_w["name"], twp_w["whole"], twp_rule)
          else
            tw_take(tw_blank(), twp_w["state"], twp_w["name"], twp_w["whole"], twp_rule)
          end
        end
      end
    end
  end
  tw_place_settle(tw_bc_settle(tw_grad_settle(twp_out)))
end

def tw_style(classes, disabled = false, width = nil)
  ts_all = tw(classes, width)
  return ts_all["s"].merge(ts_all["disabled"]) if disabled == true

  ts_all["s"]
end

def tw_stateful?(t)
  st_n = t["hover"].keys().length() + t["press"].keys().length() + t["focus"].keys().length()
  st_n > 0
end

def tw_node(n)
  tn_style = n["s"]
  tn_t = tw_raw(tn_style["tw"], tn_style["vw"])
  tn_rest = {}
  for tn_key in tn_style.keys()
    tn_rest[tn_key] = tn_style[tn_key] unless tn_key == "tw" || tn_key == "vw"
  end
  tn_base = tn_t["s"].merge(tn_rest)
  tn_case = tn_base["tw_case"]
  unless tn_case.nil?
    throw tw_no(tw_case_class(tn_case), "a text transform changes a string, and a box has none: put it on the text, text(s, tw_style(\"" + tw_case_class(tn_case) + "\"))")
  end
  tn_reversed = !tn_base["tw_reverse"].nil?
  tn_base = tw_reverse_style(tn_base) if tn_reversed
  tn_base = tw_gap_settle(tn_base, tn_t["gaps"], tn_base["display"] ?? "row")
  n["s"] = tn_base
  n["p"] = (n["p"] ?? {}).merge(tn_t["props"]) if tn_t["props"].keys().length() > 0
  n["c"] = tw_divide(n["c"] ?? [], tn_t["divide"]) if tn_t["divide"].keys().length() > 0
  # After the rules, which Tailwind lays on in the order the children were
  # given, as `> * ~ *` does; the eye then sees them the other way round.
  n["c"] = (n["c"] ?? []).reverse() if tn_reversed
  n["on"] = tw_wire(tn_base, tn_t, n["on"] ?? {}) if tw_stateful?(tn_t)
  n
end

def tw_wire(base, t, on)
  tww_hover = base.merge(t["hover"] ?? {})
  tww_press = tww_hover.merge(t["press"] ?? {})
  tww_focus = base.merge(t["focus"] ?? {})
  tww_out = on.merge({})
  if (t["hover"] ?? {}).keys().length() > 0 || (t["press"] ?? {}).keys().length() > 0
    tww_out = tww_out.merge({
      "pointer_enter": {"local": "self.style = @hover", "styles": {"hover": tww_hover}},
      "pointer_leave": {"local": "self.style = @base", "styles": {"base": base}},
      "pointer_down": {"local": "self.style = @active", "styles": {"active": tww_press}},
      "pointer_up": {"local": "self.style = @hover", "styles": {"hover": tww_hover}}
    })
  end
  if (t["focus"] ?? {}).keys().length() > 0
    tww_out = tww_out.merge({
      "focus": {"local": "self.style = @focus", "styles": {"focus": tww_focus}},
      "blur": {"local": "self.style = @base", "styles": {"base": base}}
    })
  end
  tww_out
end

def tw_reverse_style(style)
  rv_out = tw_without(style, "tw_reverse")
  rv_just = rv_out["justify"] ?? "start"
  rv_out["justify"] = "end" if rv_just == "start"
  rv_out["justify"] = "start" if rv_just == "end"
  rv_out
end

def tw_autos?(kids)
  return false if kids.nil? || kids.class != "array"

  for au_kid in kids
    if !au_kid.nil? && au_kid.class == "hash"
      au_s = au_kid["s"]
      return true if !au_s.nil? && au_s.class == "hash" && !au_s["tw_auto"].nil?
    end
  end
  false
end

def tw_auto_place(n)
  ap_disp = (n["s"] ?? {})["display"] ?? "row"
  ap_out = []
  for ap_kid in n["c"]
    ap_s = (ap_kid.nil? || ap_kid.class != "hash" || ap_kid["s"].class != "hash") ? nil : ap_kid["s"]
    ap_auto = ap_s.nil? ? nil : ap_s["tw_auto"]
    if ap_auto.nil?
      ap_out = ap_out.concat([ap_kid])
    else
      ap_c = ap_auto["c"]
      if n["k"] != "box" || (ap_disp != "row" && ap_disp != "column")
        throw tw_no(ap_c, "an auto margin takes the free space along a row or a column, and a " + (n["k"] == "box" ? ap_disp : n["k"]) + " lays its children out another way")
      end
      ap_row = ap_disp == "row"
      ap_before = ap_row ? ap_auto["l"] : ap_auto["t"]
      ap_after = ap_row ? ap_auto["r"] : ap_auto["b"]
      ap_near = ap_row ? ap_auto["t"] : ap_auto["l"]
      ap_far = ap_row ? ap_auto["b"] : ap_auto["r"]
      ap_self = nil
      ap_self = "center" if ap_near == true && ap_far == true
      ap_self = "end" if ap_near == true && ap_far != true
      ap_self = "start" if ap_near != true && ap_far == true
      ap_placed = tw_restyle_kid(ap_kid, fn(st) { tw_auto_style(st, ap_self) })
      ap_out = ap_out.concat([{"k": "spacer", "s": {"grow": 1}}]) if ap_before == true
      ap_out = ap_out.concat([ap_placed])
      ap_out = ap_out.concat([{"k": "spacer", "s": {"grow": 1}}]) if ap_after == true
    end
  end
  n["c"] = ap_out
  n
end

def tw_auto_style(st, self_to)
  as_out = tw_without(st, "tw_auto")
  as_out["self"] = self_to unless self_to.nil?
  as_out
end

def tw_no(whole, why)
  "tw: '" + whole + "' has no EUI equivalent — " + why
end

def tw_unknown(whole)
  "tw: unknown class '" + whole + "' — not one tw() knows; the table is doc/docs/eui/tailwind.md"
end

def tw_variant(prefix, whole)
  return "hover" if prefix == "hover"
  return "press" if prefix == "active"
  return "focus" if prefix == "focus"
  return "disabled" if prefix == "disabled"
  # The client draws the keyboard ring itself; what else a focus-visible:
  # class changes is laid on whenever the node has focus, as focus: is.
  return "focus" if prefix == "focus-visible"

  tv_why = "a variant tw() does not know; hover:, active:, focus:, focus-visible: and disabled: are the states, sm: to 2xl: the breakpoints"
  tv_why = "breakpoints are mobile first: write the narrow classes bare and the wider ones under sm:, md:, lg:, xl: or 2xl:" if prefix.starts_with?("max-") || prefix.starts_with?("min-")
  tv_why = "colours are roles and already follow the viewer's theme; drop the dark: classes" if prefix == "dark"
  tv_why = "a local handler restyles the node it is on, not an ancestor; focus: on the field, and a key on the box if it must change too" if prefix == "focus-within"
  tv_why = "there are no group or peer states: a local handler restyles one node, by key" if prefix.starts_with?("group") || prefix.starts_with?("peer")
  tv_why = "there are no structural selectors: style the first or last child where it is built" if ["first", "last", "odd", "even", "only"].includes?(prefix)
  tv_why = "there are no pseudo-elements: build the node" if ["before", "after", "placeholder", "file", "marker", "selection"].includes?(prefix)
  throw tw_no(whole, tv_why)
end

def tw_refused(name)
  rf_why = {
    "tracking-": "EUI has no letter-spacing; the 64-byte style record has no byte for it",
    "leading-": "line height comes with the text size; there is no independent line-height",
    "ring-offset": "a ring is written as a border, and a border has no offset",
    "rounded-t": "radius is one byte for all four corners",
    "rounded-b": "radius is one byte for all four corners",
    "rounded-l": "radius is one byte for all four corners",
    "rounded-r": "radius is one byte for all four corners",
    "rounded-s": "radius is one byte for all four corners",
    "rounded-e": "radius is one byte for all four corners",
    "translate-": "EUI has no transforms",
    "rotate-": "EUI has no transforms",
    "scale-": "EUI has no transforms",
    "skew-": "EUI has no transforms",
    "origin-": "EUI has no transforms",
    "transform": "EUI has no transforms",
    "outline": "the client draws its own focus ring; there is no outline",
    "ring-inset-": "a ring is written as a border",
    "blur": "there are no filters; backdrop-blur is the one blur",
    "drop-shadow": "there are no filters; shadow-sm, shadow-md and shadow-lg are the shadows",
    "brightness-": "there are no filters",
    "contrast-": "there are no filters",
    "grayscale": "there are no filters",
    "invert": "there are no filters",
    "saturate-": "there are no filters",
    "sepia": "there are no filters",
    "hue-rotate-": "there are no filters",
    "ease-": "the client has one easing curve; transition and duration-N are what there is",
    "delay-": "a transition is a duration and never a delay; stagger by grafting nodes over time",
    "order-": "children are drawn in the order they are given",
    "col-": "a grid places its children in order; there are no spans",
    "row-": "a grid places its children in order; there are no spans",
    "grid-rows-": "a grid has columns and rows follow; write grid-cols-N",
    "grid-flow-": "a grid fills by row",
    "auto-cols-": "a grid's columns are equal shares",
    "auto-rows-": "a grid's rows are as tall as their content",
    "place-": "write items-* and justify-* on the box",
    "content-": "a wrapped row packs its lines from the start",
    "justify-items-": "write items-* on the box",
    "justify-self-": "write self-* on the child",
    "whitespace-": "text wraps at its box's width; truncate keeps it to one line with an ellipsis, or give the box the width",
    "break-": "text wraps at the box edge, and clamp decides how far",
    "object-": "an image fills the box it is given",
    "pointer-events-": "the topmost node takes the pointer, handler or not, and an event walks up from it, never through it to a sibling below",
    "select-": "only editable nodes select text; select-none is what every other node already is",
    "appearance-": "there is no native appearance to reset",
    "resize": "a textarea grows with its content",
    "overflow-x-": "overflow is one value for both axes; scrolling is a scroll() node",
    "overflow-y-": "overflow is one value for both axes; scrolling is a scroll() node",
    "decoration-": "an underline takes the text's colour and weight",
    "underline-offset-": "an underline sits where the client draws it",
    "list-": "there are no list markers; build the bullet",
    "fill-": "an icon takes fg; write text-*",
    "stroke-": "an icon takes fg; write text-*",
    "sr-only": "there is no visually hidden text; put the label in props",
    "not-sr-only": "there is no visually hidden text; put the label in props",
    "backdrop-": "backdrop-blur is the one backdrop filter",
    "animate-": "the animations are animate-spin, animate-pulse and animate-bounce; an entrance is the enter animation",
    "font-": "the weights are font-normal, font-medium, font-semibold and font-bold, and the faces font-sans and font-mono",
    "shadow-": "shadow-sm, shadow, shadow-md, shadow-lg and shadow-xl are the shadows, and a shadow takes no colour",
    "rounded-": "rounded-none, -sm, rounded, -md, -lg, -xl, -2xl, -3xl and -full are the radii",
    "cursor-": "the cursors are pointer, default, text, wait, not-allowed, grab, grabbing, col-resize and row-resize",
    "overflow-": "overflow-hidden, overflow-clip and overflow-visible; scrolling is a scroll() node",
    "transition-": "transition, transition-colors, transition-all, transition-opacity, transition-shadow and transition-none"
  }
  for rf_prefix in rf_why.keys()
    return rf_why[rf_prefix] if name.starts_with?(rf_prefix)
  end
  ""
end

def tw_refused_exact(name)
  return "there are no positioning schemes; absolute is the one there is, inside a stack" if name == "fixed" || name == "sticky"
  return "there is no inline flow; a box is flex, flex-col, block, grid or hidden, and text wraps inside its own node" if ["inline", "inline-block", "table", "contents", "flow-root"].includes?(name)
  return "the client ships no italic face" if name == "italic" || name == "not-italic"
  return "Inter's figures are drawn proportional and the client selects no OpenType feature; font-mono sets figures that line up" if name == "tabular-nums" || name == "proportional-nums" || name == "lining-nums" || name == "oldstyle-nums"
  return "the view is given the viewport's size; use it" if ["w-screen", "h-screen", "min-h-screen", "min-w-screen", "max-w-screen", "max-h-screen", "h-dvh", "h-svh", "h-lvh", "min-h-dvh", "min-h-svh", "min-h-lvh", "w-dvw", "w-svw", "w-lvw"].includes?(name)
  return "a line that does not wrap runs out of its box; truncate keeps it to one line and ends it with an ellipsis" if name == "text-nowrap" || name == "text-clip"
  return "a line breaks where the next word does not fit, and the client balances nothing" if name == "text-balance" || name == "text-pretty"
  return "a bare ring is three pixels; write ring-1 or ring-2, which become a border" if name == "ring"
  return "there is no container query and no typography plugin; give the box a width" if name == "container" || name == "prose"
  return "a shadow is cast, never inset" if name == "shadow-inner"
  return "a border is always solid; border-0 removes it" if ["border-dashed", "border-dotted", "border-double", "border-solid", "border-none"].includes?(name)

  ""
end

def tw_space_steps()
  {"0": 0, "0.5": 1, "1": 2, "1.5": 13, "2": 3, "2.5": 14, "3": 4, "3.5": 15, "4": 5, "5": 6, "6": 7, "8": 8, "10": 9, "12": 10, "16": 11, "20": 16, "24": 12, "32": 17}
end

def tw_text_sizes()
  {"xs": 0, "sm": 1, "base": 2, "lg": 3, "xl": 4, "2xl": 5, "3xl": 6, "4xl": 7}
end

def tw_max_widths()
  {"xs": 320, "sm": 384, "md": 448, "lg": 512, "xl": 576, "2xl": 672, "3xl": 768, "4xl": 896, "5xl": 1024, "6xl": 1152, "7xl": 1280}
end

def tw_space(value, whole)
  sc_steps = tw_space_steps()
  sc_ix = sc_steps[value]
  return sc_ix unless sc_ix.nil?

  throw tw_no(whole, "'" + value + "' is not on the space scale; " + tw_space_near(value) + "the steps are " + sc_steps.keys().join(", "))
end

def tw_space_near(value)
  return "" unless tw_numeric?(value)

  sn_want = float(value)
  sn_below = ""
  sn_above = ""
  for sn_step in tw_space_steps().keys()
    sn_at = float(sn_step)
    sn_below = sn_step if sn_at < sn_want
    sn_above = sn_step if sn_at > sn_want && sn_above == ""
  end
  return "" if sn_below == "" && sn_above == ""
  return "the nearest is " + sn_below + " (" + str(int(float(sn_below) * 4.0)) + " px); " if sn_above == ""
  return "the nearest is " + sn_above + " (" + str(int(float(sn_above) * 4.0)) + " px); " if sn_below == ""

  "the nearest are " + sn_below + " (" + str(int(float(sn_below) * 4.0)) + " px) and " + sn_above + " (" + str(int(float(sn_above) * 4.0)) + " px); "
end

def tw_numeric?(value)
  nm_chars = value.chars()
  return false if nm_chars.length() == 0

  nm_dots = 0
  for nm_c in nm_chars
    if nm_c == "."
      nm_dots = nm_dots + 1
    elsif !("0123456789".index_of(nm_c) >= 0)
      return false
    end
  end
  nm_dots <= 1 && value != "."
end

def tw_bracket(value, whole)
  br_inner = value.substring(1, value.length() - 1)
  if br_inner.ends_with?("px")
    br_n = br_inner.substring(0, br_inner.length() - 2)
    return int(float(br_n).round()) if tw_numeric?(br_n)
  end
  if br_inner.ends_with?("%")
    br_p = br_inner.substring(0, br_inner.length() - 1)
    return br_p + "%" if tw_numeric?(br_p)
  end
  throw tw_no(whole, "an arbitrary length is [Npx] or [N%]")
end

def tw_length(value, whole)
  return "100%" if value == "full"
  return "auto" if value == "auto"
  return 1 if value == "px"
  return tw_bracket(value, whole) if value.starts_with?("[") && value.ends_with?("]")

  if value.index_of("/") > 0
    ln_parts = value.split("/")
    if ln_parts.length() == 2 && tw_numeric?(ln_parts[0]) && tw_numeric?(ln_parts[1]) && float(ln_parts[1]) > 0
      ln_pct = float(ln_parts[0]) * 100.0 / float(ln_parts[1])
      return str(int(ln_pct.round())) + "%"
    end
  end
  if tw_numeric?(value)
    ln_px = int((float(value) * 4.0).round())
    return ln_px if ln_px <= 65535
  end
  throw tw_no(whole, "a length is N (N x 4 px), a fraction, full, auto, px or [Npx]")
end

def tw_roles()
  [
    "surface.base", "surface.raised", "surface.sunken", "surface.overlay",
    "text.default", "text.muted", "text.inverted", "text.disabled",
    "accent.base", "accent.hover", "accent.active", "accent.on",
    "success.base", "success.subtle", "success.on",
    "warning.base", "warning.subtle", "warning.on",
    "danger.base", "danger.subtle", "danger.on",
    "info.base", "info.subtle", "info.on",
    "border.subtle", "border.default", "border.strong",
    "focus.ring",
    "series.1", "series.2", "series.3", "series.4", "series.5"
  ]
end

def tw_role_alias(prefix, value)
  ra_dotted = value.replace("-", ".")
  return ra_dotted if tw_roles().includes?(ra_dotted)
  return value + ".base" if ["accent", "danger", "success", "warning", "info"].includes?(value)
  return "text." + value if ["muted", "disabled", "inverted"].includes?(value) && prefix == "text"
  return "surface." + value if ["raised", "sunken", "overlay"].includes?(value)
  return "surface.base" if value == "surface"
  return "text.default" if value == "default" && prefix == "text"
  return "border." + value if ["subtle", "default", "strong"].includes?(value) && (prefix == "border" || prefix == "ring")

  ""
end

def tw_neutral?(family)
  ["gray", "slate", "zinc", "neutral", "stone"].includes?(family)
end

def tw_status(family)
  return "danger" if family == "red" || family == "rose"
  return "success" if family == "green" || family == "emerald"
  return "warning" if family == "yellow" || family == "amber"
  return "info" if family == "blue" || family == "sky"

  ""
end

def tw_nearest(family)
  return "accent (indigo-600), or series-1 to series-5 for data" if ["purple", "violet", "fuchsia", "pink"].includes?(family)
  return "warning (yellow-600)" if family == "orange"
  return "success (green-600) or info (blue-600)" if ["teal", "cyan", "lime"].includes?(family)

  "a role: accent, danger, success, warning, info, or the grays"
end

def tw_hex2(n)
  hx_digits = "0123456789ABCDEF"
  hx_n = n
  hx_n = 0 if hx_n < 0
  hx_n = 255 if hx_n > 255
  hx_digits[hx_n / 16] + hx_digits[hx_n % 16]
end

def tw_hex?(value)
  return false unless value.length() == 6 || value.length() == 8

  for hx_c in value.chars()
    return false unless "0123456789abcdefABCDEF".index_of(hx_c) >= 0
  end
  true
end

def tw_alpha(prefix, value, whole)
  al_parts = value.split("/")
  al_base = al_parts[0]
  al_pct = al_parts.length() == 2 && tw_numeric?(al_parts[1]) ? float(al_parts[1]) : -1.0
  if al_pct >= 0.0 && al_pct <= 100.0
    al_byte = int((al_pct * 255.0 / 100.0).round())
    al_dash = al_base.index_of("-")
    al_family = al_dash > 0 ? al_base.substring(0, al_dash) : al_base
    if prefix == "border" || prefix == "ring"
      return "border.subtle" if (al_base == "black" || tw_neutral?(al_family)) && al_pct <= 20.0
      return tw_status(al_family) + ".subtle" if tw_status(al_family) != "" && al_pct <= 30.0
    end
    if prefix == "bg"
      return "#000000" + tw_hex2(al_byte) if al_base == "black"
      return "#FFFFFF" + tw_hex2(al_byte) if al_base == "white"
      return "#6B7280" + tw_hex2(al_byte) if al_base == "gray-500"
    end
  end
  throw tw_no(whole, "roles are opaque; an opacity is taken only on a ring (ring-gray-900/5 is border.subtle) and on a scrim (bg-black/50, bg-gray-500/75)")
end

def tw_colour(prefix, value, whole)
  return "none" if value == "transparent"
  if value.starts_with?("[#") && value.ends_with?("]")
    co_hex = value.substring(2, value.length() - 1)
    return "#" + co_hex if tw_hex?(co_hex)

    throw tw_no(whole, "a literal colour is [#RRGGBB] or [#RRGGBBAA]")
  end
  return tw_alpha(prefix, value, whole) if value.index_of("/") > 0

  co_role = tw_role_alias(prefix, value)
  return co_role if co_role != ""

  if value == "white"
    return "accent.on" if prefix == "text"

    return "surface.raised"
  end
  return "text.default" if value == "black" && prefix == "text"

  co_dash = value.index_of("-")
  if co_dash <= 0
    throw tw_no(whole, "'" + value + "' is not a colour tw() knows; write a role (bg-accent, text-muted) or a palette shade") if value != "black"

    throw tw_no(whole, "black is a literal, and a surface is a role; write bg-gray-900, or bg-black/50 for a scrim")
  end
  co_family = value.substring(0, co_dash)
  co_shade = value.substring(co_dash + 1, value.length())
  co_found = ""
  if tw_neutral?(co_family)
    co_found = tw_gray(prefix, co_shade)
  elsif co_family == "indigo"
    co_found = "accent.base" if co_shade == "600"
    co_found = "accent.hover" if co_shade == "500"
    co_found = "accent.active" if co_shade == "700"
    if co_found == ""
      throw tw_no(whole, "the accent has base, hover and active (indigo-600, -500, -700) and no tint; bg-info-subtle is the nearest wash")
    end
  elsif tw_status(co_family) != ""
    co_status = tw_status(co_family)
    co_found = co_status + ".subtle" if co_shade == "50" || co_shade == "100"
    co_found = co_status + ".base" if ["500", "600", "700", "800"].includes?(co_shade)
    if co_found == ""
      throw tw_no(whole, co_family + " is the " + co_status + " role, which has a subtle tint (-50, -100) and a base (-500 to -800)")
    end
  else
    throw tw_no(whole, "colours are theme roles and " + co_family + " is not one; nearest is " + tw_nearest(co_family) + ", or [#RRGGBB] for a literal")
  end
  if co_found == ""
    throw tw_no(whole, "the grays are roles: bg white/50/100 are the surfaces, 200/300/400 the borders; text 900-700, 600-500 and 400 are default, muted and disabled")
  end
  co_found
end

def tw_gray(prefix, shade)
  if prefix == "bg"
    return "surface.base" if shade == "50"
    return "surface.sunken" if shade == "100"
    return "border.subtle" if shade == "200"
    return "border.default" if shade == "300"
    return "border.strong" if shade == "400"
    return "text.default" if ["800", "900", "950"].includes?(shade)
  elsif prefix == "text"
    return "text.default" if ["700", "800", "900", "950"].includes?(shade)
    return "text.muted" if shade == "500" || shade == "600"
    return "text.disabled" if shade == "300" || shade == "400"
  else
    return "border.subtle" if shade == "100" || shade == "200"
    return "border.default" if shade == "300"
    return "border.strong" if shade == "400" || shade == "500"
  end
  ""
end

def tw_exact()
  {
    "flex": {"display": "row"},
    "inline-flex": {"display": "row"},
    "flex-row": {"display": "row"},
    "flex-col": {"display": "column"},
    "grid": {"display": "grid"},
    "hidden": {"display": "none"},
    "flex-wrap": {"wrap": "wrap"},
    "flex-nowrap": {"wrap": "nowrap"},
    "flex-wrap-reverse": {"wrap": "wrap_reverse"},
    "items-start": {"align": "start"},
    "items-center": {"align": "center"},
    "items-end": {"align": "end"},
    "items-stretch": {"align": "stretch"},
    "items-baseline": {"align": "baseline"},
    "justify-start": {"justify": "start"},
    "justify-center": {"justify": "center"},
    "justify-end": {"justify": "end"},
    "justify-between": {"justify": "between"},
    "justify-around": {"justify": "around"},
    "justify-evenly": {"justify": "evenly"},
    "self-auto": {"self": "auto"},
    "self-start": {"self": "start"},
    "self-center": {"self": "center"},
    "self-end": {"self": "end"},
    "self-stretch": {"self": "stretch"},
    "self-baseline": {"self": "baseline"},
    "grow": {"grow": 1},
    "grow-0": {"grow": 0},
    "shrink": {"shrink": 1},
    "shrink-0": {"shrink": 0},
    "flex-1": {"grow": 1, "shrink": 1, "basis": 0},
    "flex-auto": {"grow": 1, "shrink": 1, "basis": "auto"},
    "flex-initial": {"grow": 0, "shrink": 1},
    "flex-none": {"grow": 0, "shrink": 0},
    # Tailwind 2's names, still in Tailwind UI's markup.
    "flex-grow": {"grow": 1},
    "flex-grow-0": {"grow": 0},
    "flex-shrink": {"shrink": 1},
    "flex-shrink-0": {"shrink": 0},
    # Reversed by node(), which has the children; see tw_reverse_style.
    "flex-row-reverse": {"display": "row", "tw_reverse": "flex-row-reverse"},
    "flex-col-reverse": {"display": "column", "tw_reverse": "flex-col-reverse"},
    # In a flexbox, normal and stretch pack the line from its start.
    "justify-normal": {"justify": "start"},
    "justify-stretch": {"justify": "start"},
    "z-auto": {"z": 0},
    "max-w-none": {"max_width": "auto"},
    "max-h-none": {"max_height": "auto"},
    "text-xs": {"size": 0},
    "text-sm": {"size": 1},
    "text-base": {"size": 2},
    "text-lg": {"size": 3},
    "text-xl": {"size": 4},
    "text-2xl": {"size": 5},
    "text-3xl": {"size": 6},
    "text-4xl": {"size": 7},
    "text-left": {"text_align": "start"},
    "text-start": {"text_align": "start"},
    "text-center": {"text_align": "center"},
    "text-right": {"text_align": "end"},
    "text-end": {"text_align": "end"},
    "text-justify": {"text_align": "justify"},
    "font-normal": {"weight": "regular"},
    "font-medium": {"weight": "medium"},
    "font-semibold": {"weight": "semibold"},
    "font-bold": {"weight": "bold"},
    # The nearest of the four faces; see Approximations.
    "font-thin": {"weight": "regular"},
    "font-extralight": {"weight": "regular"},
    "font-light": {"weight": "regular"},
    "font-extrabold": {"weight": "bold"},
    "font-black": {"weight": "bold"},
    "font-sans": {"font": "sans"},
    "font-mono": {"font": "mono"},
    "underline": {"underline": true},
    "no-underline": {"underline": false},
    "line-through": {"strike": true},
    "truncate": {"clamp": 1},
    "line-clamp-none": {"clamp": 0},
    "rounded-none": {"radius": 0},
    "rounded-sm": {"radius": 1},
    "rounded": {"radius": 2},
    "rounded-md": {"radius": 2},
    "rounded-lg": {"radius": 2},
    "rounded-xl": {"radius": 3},
    "rounded-2xl": {"radius": 3},
    "rounded-3xl": {"radius": 3},
    "rounded-full": {"radius": 4},
    "shadow-none": {"shadow": 0},
    "shadow-sm": {"shadow": 1},
    "shadow": {"shadow": 2},
    "shadow-md": {"shadow": 2},
    "shadow-lg": {"shadow": 3},
    "shadow-xl": {"shadow": 3},
    "shadow-2xl": {"shadow": 3},
    "cursor-auto": {"cursor": "default"},
    "cursor-default": {"cursor": "default"},
    "cursor-pointer": {"cursor": "pointer"},
    "cursor-text": {"cursor": "text"},
    "cursor-wait": {"cursor": "wait"},
    "cursor-not-allowed": {"cursor": "not_allowed"},
    "cursor-grab": {"cursor": "grab"},
    "cursor-grabbing": {"cursor": "grabbing"},
    "cursor-col-resize": {"cursor": "resize_h"},
    "cursor-ew-resize": {"cursor": "resize_h"},
    "cursor-row-resize": {"cursor": "resize_v"},
    "cursor-ns-resize": {"cursor": "resize_v"},
    "overflow-hidden": {"overflow": "clip"},
    "overflow-clip": {"overflow": "clip"},
    "overflow-visible": {"overflow": "visible"},
    "transition": {"transition": "fast"},
    "transition-colors": {"transition": "fast"},
    "transition-all": {"transition": "fast"},
    "transition-opacity": {"transition": "fast"},
    "transition-shadow": {"transition": "fast"},
    "transition-none": {"transition": "none"},
    "absolute": {"position": "absolute"},
    "animate-spin": {"animation": "spin"},
    "animate-pulse": {"animation": "pulse"},
    "animate-bounce": {"animation": "bounce"},
    "animate-none": {"animation": "none"},
    "backdrop-blur-none": {"blur": 0},
    "backdrop-blur-sm": {"blur": 4},
    "backdrop-blur": {"blur": 8},
    "backdrop-blur-md": {"blur": 12},
    "backdrop-blur-lg": {"blur": 16},
    "backdrop-blur-xl": {"blur": 24},
    "backdrop-blur-2xl": {"blur": 40},
    "backdrop-blur-3xl": {"blur": 64},
    # A ring is drawn inside the box, and so is an EUI border; see `ring-1`.
    "ring-inset": {},
    # A block's children stack down it at its full width, which is a column
    # stretching them; its margins never collapse, as no EUI margin does.
    "block": {"display": "column"},
    # In flow, placed by its parent. What `relative` adds in a browser -- a
    # box an absolute child is placed against -- is what a `stack` is (04 §5).
    "relative": {"position": "flow"},
    "static": {"position": "flow"},
    # Every box already paints its children above itself and z orders only
    # siblings (04: no z-index across containers), so there is no stacking
    # context to isolate.
    "isolate": {},
    # Only editable nodes select text (06 §3).
    "select-none": {},
    # What every node already is: no native look to reset, no list marker,
    # no resize handle (a textarea grows), the pointer taken as usual, text
    # that wraps at its box, an ellipsis wherever a line is cut, and glyphs
    # drawn by the client's own rasteriser.
    "appearance-none": {},
    "list-none": {},
    "resize-none": {},
    "pointer-events-auto": {},
    "whitespace-normal": {},
    "text-wrap": {},
    "text-ellipsis": {},
    "overflow-ellipsis": {},
    "antialiased": {},
    "visible": {},
    "aspect-auto": {"tw_aspect": "auto"},
    # An auto margin on the cross axis centres the node on it: mx-auto in a
    # column, which is where Tailwind writes it (a centred container in block
    # flow), and my-auto in a row. On the main axis it is a spacer instead.
    "mx-auto": {"self": "center"},
    "my-auto": {"self": "center"},
    # Text transforms, which text() applies to its string; see tw_text.
    "uppercase": {"tw_case": "upper"},
    "lowercase": {"tw_case": "lower"},
    "capitalize": {"tw_case": "capital"},
    "normal-case": {"tw_case": "none"}
  }
end

def tw_sides(which)
  return [true, false, false, false] if which == "t"
  return [false, true, false, false] if which == "r"
  return [false, false, true, false] if which == "b"
  return [false, false, false, true] if which == "l"
  return [false, true, false, true] if which == "x"
  return [true, false, true, false] if which == "y"

  [true, true, true, true]
end

def tw_edge(key, which, v)
  {"edge": key, "sides": tw_sides(which), "v": v}
end

def tw_edges(start, sides, v)
  ed_now = [0, 0, 0, 0]
  if start.nil?
    ed_now = [0, 0, 0, 0]
  elsif start.class == "array" && start.length() == 4
    ed_now = [start[0], start[1], start[2], start[3]]
  elsif start.class == "array" && start.length() == 2
    ed_now = [start[0], start[1], start[0], start[1]]
  else
    ed_now = [start, start, start, start]
  end
  ed_out = range(0, 4).map(fn(i) { sides[i] == true ? v : ed_now[i] })
  return ed_out[0] if ed_out[0] == ed_out[1] && ed_out[1] == ed_out[2] && ed_out[2] == ed_out[3]

  ed_out
end

def tw_border_width(value, whole)
  return 1 if value == ""
  return int(value) if ["0", "1", "2", "4", "8"].includes?(value)

  throw tw_no(whole, "a border is border, border-0, -2, -4 or -8")
end

def tw_class(name, whole)
  if name.index_of(":") >= 0
    throw tw_no(whole, "one variant per class; hover:focus: is two states at once")
  end
  if name.starts_with?("-")
    throw tw_no(whole, "margins are unsigned, a byte per side; there is no negative margin")
  end
  cl_fixed = tw_exact()[name]
  return {"set": cl_fixed} unless cl_fixed.nil?

  cl_exact = tw_refused_exact(name)
  throw tw_no(whole, cl_exact) if cl_exact != ""

  return tw_edge("border", "", 1) if name == "border"

  cl_family = tw_family(name, whole)
  return cl_family unless cl_family.nil?

  cl_why = tw_refused(name)
  throw tw_no(whole, cl_why) if cl_why != ""

  throw tw_unknown(whole)
end

def tw_family(name, whole)
  fa_dash = name.index_of("-")
  return nil if fa_dash <= 0

  fa_head = name.substring(0, fa_dash)
  fa_rest = name.substring(fa_dash + 1, name.length())

  # p-4, px-2, mt-1, gap-3
  if ["p", "px", "py", "pt", "pr", "pb", "pl"].includes?(fa_head)
    return tw_edge("pad", fa_head.substring(1, fa_head.length()), tw_space(fa_rest, whole))
  end
  if ["m", "mx", "my", "mt", "mr", "mb", "ml"].includes?(fa_head)
    # ml-auto, mt-auto, m-auto: placed by the box the node is put in
    # (tw_auto_place). mx-auto and my-auto are in the exact table.
    return {"auto": tw_sides(fa_head.substring(1, fa_head.length()))} if fa_rest == "auto"
    return tw_edge("margin", fa_head.substring(1, fa_head.length()), tw_space(fa_rest, whole))
  end
  return {"set": {"gap": tw_space(fa_rest, whole)}} if fa_head == "gap" && fa_rest.index_of("-") < 0

  # gap-x-4, space-y-2, divide-y, divide-gray-200: what a box says about the
  # space and the rules between its children. Settled by tw_gap_settle and
  # laid on by tw_divide, once the box's direction and children are known.
  if (fa_head == "gap" || fa_head == "space") && (fa_rest.starts_with?("x-") || fa_rest.starts_with?("y-"))
    fa_axis = fa_rest.substring(0, 1)
    fa_step = fa_rest.substring(2, fa_rest.length())
    # space-x-reverse: the margin on the other side of each child, for a
    # reversed box. EUI's gap is the same on both sides, so it is nothing.
    return {"set": {}} if fa_step == "reverse" && fa_head == "space"

    fa_kind = fa_head == "gap" ? fa_axis : "space_" + fa_axis
    return {"set": {}, "gap": {"k": fa_kind, "v": tw_space(fa_step, whole)}}
  end
  return tw_divider(fa_rest, whole) if fa_head == "divide"

  # w-64, h-10, size-8, basis-1/2, min-w-0, max-w-md
  return {"set": {"width": tw_length(fa_rest, whole)}} if fa_head == "w"
  return {"set": {"height": tw_length(fa_rest, whole)}} if fa_head == "h"
  if fa_head == "size"
    fa_side = tw_length(fa_rest, whole)
    return {"set": {"width": fa_side, "height": fa_side}}
  end
  return {"set": {"basis": tw_length(fa_rest, whole)}} if fa_head == "basis"
  if fa_head == "min" || fa_head == "max"
    fa_axis = fa_rest.substring(0, 2)
    fa_value = fa_rest.substring(2, fa_rest.length())
    if fa_axis == "w-" || fa_axis == "h-"
      fa_key = fa_head + (fa_axis == "w-" ? "_width" : "_height")
      fa_named = tw_max_widths()[fa_value]
      fa_set = {}
      if fa_key == "max_width" && !fa_named.nil?
        fa_set[fa_key] = fa_named
      else
        fa_set[fa_key] = tw_length(fa_value, whole)
      end
      return {"set": fa_set}
    end
    return nil
  end

  # bg-gradient-to-r, from-indigo-600, via-50%, to-pink-500: the pieces of a
  # gradient, settled into one `bg` by tw_grad_settle.
  return tw_grad_direction(fa_rest, whole) if fa_head == "bg" && fa_rest.starts_with?("gradient")
  return tw_grad_stop(fa_head, fa_rest, whole) if ["from", "via", "to"].includes?(fa_head)

  # bg-*, text-* (sizes and aligns are exact), border-*, ring-*
  return {"set": {"bg": tw_colour("bg", fa_rest, whole)}} if fa_head == "bg"
  if fa_head == "text"
    if tw_text_sizes()[fa_rest].nil? && ["5xl", "6xl", "7xl", "8xl", "9xl"].includes?(fa_rest)
      throw tw_no(whole, "the text scale stops at text-4xl, index 7")
    end
    if fa_rest.index_of("/") > 0 && !tw_text_sizes()[fa_rest.split("/")[0]].nil?
      throw tw_no(whole, "line height comes with the text size; write " + name.split("/")[0])
    end
    return {"set": {"fg": tw_colour("text", fa_rest, whole)}}
  end
  if fa_head == "border"
    return tw_edge("border", "", tw_border_width(fa_rest, whole)) if tw_numeric?(fa_rest)
    if ["x", "y", "t", "r", "b", "l"].includes?(fa_rest.substring(0, 1)) && (fa_rest.length() == 1 || fa_rest.substring(1, 2) == "-")
      fa_width = fa_rest.length() == 1 ? "" : fa_rest.substring(2, fa_rest.length())
      # border-b-indigo-600: one side's colour, which tw_bc_settle makes the
      # box's one border colour when the sides that are drawn agree.
      return tw_bc_side(fa_rest.substring(0, 1), fa_width, whole) if fa_width != "" && !tw_numeric?(fa_width)
      return tw_edge("border", fa_rest.substring(0, 1), tw_border_width(fa_width, whole))
    end
    return {"set": {"border_color": tw_colour("border", fa_rest, whole)}}
  end
  if fa_head == "ring"
    return tw_edge("border", "", int(fa_rest)) if fa_rest == "0" || fa_rest == "1" || fa_rest == "2"
    return nil if fa_rest.starts_with?("offset") || fa_rest.starts_with?("inset")
    if tw_numeric?(fa_rest)
      throw tw_no(whole, "a ring is written as a border, and ring-1 and ring-2 are the widths that read as one")
    end
    return {"set": {"border_color": tw_colour("ring", fa_rest, whole)}}
  end

  # inset-0, top-4, right-0: where an absolute node sits in its stack.
  return tw_inset(fa_head, fa_rest, whole) if ["inset", "top", "right", "bottom", "left"].includes?(fa_head)
  return tw_aspect(fa_rest, whole) if fa_head == "aspect"

  # opacity-50, z-10, line-clamp-2, duration-150, grid-cols-3
  if fa_head == "opacity"
    if tw_numeric?(fa_rest) && float(fa_rest) <= 100.0
      return {"set": {"opacity": int((float(fa_rest) * 255.0 / 100.0).round())}}
    end
    throw tw_no(whole, "opacity is 0 to 100")
  end
  if fa_head == "z"
    return {"set": {"z": int(fa_rest)}} if tw_numeric?(fa_rest) && fa_rest.index_of(".") < 0 && int(fa_rest) <= 255

    throw tw_no(whole, "z is 0 to 255")
  end
  if name.starts_with?("line-clamp-")
    fa_lines = name.substring(11, name.length())
    return {"set": {"clamp": int(fa_lines)}} if tw_numeric?(fa_lines) && fa_lines.index_of(".") < 0 && int(fa_lines) <= 255

    throw tw_no(whole, "line-clamp takes a count of lines")
  end
  if fa_head == "duration"
    if tw_numeric?(fa_rest)
      fa_ms = float(fa_rest)
      return {"set": {"transition": "fast"}} if fa_ms <= 100.0
      return {"set": {"transition": "base"}} if fa_ms <= 200.0
      return {"set": {"transition": "slow"}} if fa_ms <= 350.0
      return {"set": {"transition": "slower"}} if fa_ms <= 700.0

      return {"set": {"transition": "slowest"}}
    end
    throw tw_no(whole, "a duration is a number of milliseconds")
  end
  if name.starts_with?("grid-cols-")
    fa_cols = name.substring(10, name.length())
    return {"set": {}, "props": {"columns": int(fa_cols)}} if tw_numeric?(fa_cols) && fa_cols.index_of(".") < 0 && int(fa_cols) > 0

    throw tw_no(whole, "a grid's columns are equal shares; grid-cols-N takes a count")
  end
  nil
end

def tw_take(out, variant, name, whole, rule = 0)
  tk_patch = tw_class(name, whole)
  tk_style = out[variant]
  tk_set = tk_patch["set"] ?? {}
  tk_placed = !tk_patch["auto"].nil? || !tk_patch["inset"].nil? || !tk_set["tw_aspect"].nil? || !tk_set["tw_reverse"].nil?
  if tk_placed && variant != "s"
    throw tw_no(whole, "where a node sits is laid out once and has no states; write it without the state")
  end
  if !tk_patch["auto"].nil?
    tk_style = tk_style.merge({"tw_auto": tw_sided(tk_style["tw_auto"], tk_patch["auto"], true).merge({"c": whole})})
    # The side's number goes: `ml-4 ml-auto` is auto, not 16 px and auto.
    tk_style["margin"] = tw_edges(tk_style["margin"], tk_patch["auto"], 0) unless tk_style["margin"].nil?
  elsif !tk_patch["inset"].nil?
    tk_style = tk_style.merge({"tw_inset": tw_sided(tk_style["tw_inset"], tk_patch["inset"]["sides"], tk_patch["inset"]["v"]).merge({"c": whole})})
  elsif !tk_patch["side_c"].nil?
    tk_style = tk_style.merge({"tw_bc": tw_sided(tk_style["tw_bc"], tk_patch["side_c"]["sides"], [tk_patch["side_c"]["c"], rule, whole])})
  elsif tk_patch["edge"].nil?
    tk_style = tk_style.merge(tk_set)
    tk_style["tw_bc_lvl"] = rule unless tk_set["border_color"].nil?
    # `flex-row-reverse md:flex-row`: a direction written later is not reversed.
    tk_style = tw_without(tk_style, "tw_reverse") if !tk_set["display"].nil? && tk_set["tw_reverse"].nil?
  else
    tk_key = tk_patch["edge"]
    tk_from = tk_style[tk_key]
    tk_from = out["s"][tk_key] if tk_from.nil? && variant != "s"
    tk_style = tk_style.merge({})
    tk_style[tk_key] = tw_edges(tk_from, tk_patch["sides"], tk_patch["v"])
    # `ml-auto md:ml-4`: a number written later is not auto.
    tk_style["tw_auto"] = tw_sided(tk_style["tw_auto"], tk_patch["sides"], false) if tk_key == "margin" && !tk_style["tw_auto"].nil?
  end
  tk_gap = tk_patch["gap"]
  tk_rule = tk_patch["divide"]
  if !tk_gap.nil? || !tk_rule.nil?
    throw tw_no(whole, "the space and the rules between children are laid out once and have no states; write it without the state") if variant != "s"

    out["gaps"][tk_gap["k"]] = {"v": tk_gap["v"], "c": whole} unless tk_gap.nil?
    out["divide"] = out["divide"].merge(tk_rule).merge({"c": whole}) unless tk_rule.nil?
  end
  # animate-spin animate-pulse: the animation is a bit set (03 §5), so two
  # animate-* classes are both bits rather than the last one written.
  tk_anim = (tk_patch["set"] ?? {})["animation"]
  tk_had = out[variant]["animation"]
  if !tk_anim.nil? && tk_anim != "none" && !tk_had.nil? && tk_had != "none"
    tk_bits = tk_had.class == "array" ? tk_had.filter(fn(b) { b != tk_anim }) : [tk_had].filter(fn(b) { b != tk_anim })
    tk_style["animation"] = tk_bits.concat([tk_anim])
  end
  if variant != "s" && !(tk_patch["set"] ?? {})["tw_case"].nil?
    throw tw_no(whole, "a text transform changes the string, which a state cannot; write it without the state")
  end
  tk_props = tk_patch["props"] ?? {}
  if tk_props.keys().length() > 0
    throw tw_no(whole, "a prop has no states; write grid-cols-N without a variant") if variant != "s"

    out["props"] = out["props"].merge(tk_props)
  end
  out[variant] = tk_style
  out
end

def tw_sided(marker, sides, v)
  sd_out = (marker ?? {}).merge({})
  sd_names = ["t", "r", "b", "l"]
  for sd_i in range(0, 4)
    sd_out[sd_names[sd_i]] = v if sides[sd_i]
  end
  sd_out
end

def tw_without(style, key)
  wo_out = {}
  for wo_key in style.keys()
    wo_out[wo_key] = style[wo_key] unless wo_key == key
  end
  wo_out
end

def tw_grad_direction(rest, whole)
  gd_sides = {"t": "top", "tr": "top right", "r": "right", "br": "bottom right", "b": "bottom", "bl": "bottom left", "l": "left", "tl": "top left"}
  gd_side = rest.starts_with?("gradient-to-") ? gd_sides[rest.substring(12, rest.length())] : nil
  throw tw_no(whole, "a gradient goes to a side or a corner: bg-gradient-to-t, -tr, -r, -br, -b, -bl, -l or -tl") if gd_side.nil?

  {"set": {"tw_grad_dir": gd_side, "tw_grad_c": whole}}
end

def tw_grad_stop(head, rest, whole)
  gs_key = head == "to" ? "tw_grad_end" : "tw_grad_" + head
  if rest.ends_with?("%")
    gs_pct = rest.substring(0, rest.length() - 1)
    if tw_numeric?(gs_pct) && float(gs_pct) <= 100.0
      gs_at = {}
      gs_at[gs_key + "_at"] = int((float(gs_pct) * 255.0 / 100.0).round())
      gs_at["tw_grad_c"] = whole
      return {"set": gs_at}
    end
    throw tw_no(whole, "a stop's position is 0% to 100%")
  end
  gs_colour = tw_colour("bg", rest, whole)
  throw tw_no(whole, "a stop is a colour, and a role has no transparent version to fade to; give the gradient a to-* colour") if gs_colour == "none"

  gs_set = {}
  gs_set[gs_key] = gs_colour
  gs_set["tw_grad_c"] = whole
  {"set": gs_set}
end

def tw_grad_any?(style)
  style.keys().filter(fn(k) { k.starts_with?("tw_grad_") }).length() > 0
end

def tw_grad_pieces(style)
  gp_out = {}
  for gp_key in style.keys()
    gp_out[gp_key] = style[gp_key] if gp_key.starts_with?("tw_grad_")
  end
  gp_out
end

def tw_grad_build(style, p)
  gb_c = p["tw_grad_c"]
  throw tw_no(gb_c, "a colour stop belongs to a gradient, and bg-gradient-to-* is what starts one") if p["tw_grad_dir"].nil?
  throw tw_no(gb_c, "a gradient starts from a colour: write from-*") if p["tw_grad_from"].nil?
  throw tw_no(gb_c, "a gradient ends at a colour: write to-* (a role has no transparent version to fade to)") if p["tw_grad_end"].nil?
  throw tw_no(gb_c, "via-N% places the middle stop, and there is no via-* colour") if p["tw_grad_via"].nil? && !p["tw_grad_via_at"].nil?

  gb_from_at = p["tw_grad_from_at"] ?? 0
  gb_end_at = p["tw_grad_end_at"] ?? 255
  gb_stops = [ [p["tw_grad_from"], gb_from_at] ]
  unless p["tw_grad_via"].nil?
    gb_via_at = p["tw_grad_via_at"] ?? 128
    throw tw_no(gb_c, "the stops go forwards: from-N% <= via-N% <= to-N%") if gb_via_at < gb_from_at || gb_end_at < gb_via_at
    gb_stops = gb_stops.concat([ [p["tw_grad_via"], gb_via_at] ])
  end
  throw tw_no(gb_c, "the stops go forwards: from-N% <= to-N%") if gb_end_at < gb_from_at
  gb_stops = gb_stops.concat([ [p["tw_grad_end"], gb_end_at] ])
  gb_out = {}
  for gb_key in style.keys()
    gb_out[gb_key] = style[gb_key] unless gb_key.starts_with?("tw_grad_")
  end
  gb_out["bg"] = {"gradient": {"to": p["tw_grad_dir"], "stops": gb_stops}}
  gb_out
end

def tw_grad_settle(out)
  for gt_state in ["hover", "press", "focus", "disabled"]
    if tw_grad_any?(out[gt_state])
      out[gt_state] = tw_grad_build(out[gt_state], tw_grad_pieces(out["s"]).merge(tw_grad_pieces(out[gt_state])))
    end
  end
  out["s"] = tw_grad_build(out["s"], tw_grad_pieces(out["s"])) if tw_grad_any?(out["s"])
  out
end

def tw_bc_side(which, colour, whole)
  {"side_c": {"sides": tw_sides(which), "c": tw_colour("border", colour, whole)}}
end

def tw_bc_settle(out)
  bt_s = out["s"]
  bt_rest = bt_s["tw_bc"] ?? {}
  for bt_state in ["hover", "press", "focus", "disabled"]
    bt_st = out[bt_state]
    bt_touches = !bt_st["tw_bc"].nil? || !bt_st["border"].nil? || !bt_st["border_color"].nil?
    if bt_touches && (bt_rest.keys().length() > 0 || !bt_st["tw_bc"].nil?)
      bt_sides = bt_rest.merge(bt_st["tw_bc"] ?? {})
      bt_own = !bt_st["border_color"].nil?
      bt_general = bt_own ? bt_st["border_color"] : bt_s["border_color"]
      bt_rule = bt_own ? bt_st["tw_bc_lvl"] : (bt_s["tw_bc_lvl"] ?? -1)
      bt_c = tw_bc_state(bt_sides, bt_general, bt_rule, bt_st["border"] ?? bt_s["border"])
      bt_st = tw_bc_strip(bt_st)
      bt_st["border_color"] = bt_c unless bt_c.nil?
      out[bt_state] = bt_st
    else
      out[bt_state] = tw_bc_strip(bt_st)
    end
  end
  if bt_rest.keys().length() > 0
    bt_c = tw_bc_state(bt_rest, bt_s["border_color"], bt_s["tw_bc_lvl"] ?? -1, bt_s["border"])
    bt_s = tw_bc_strip(bt_s)
    bt_s["border_color"] = bt_c unless bt_c.nil?
    out["s"] = bt_s
  else
    out["s"] = tw_bc_strip(bt_s)
  end
  out
end

def tw_bc_state(sides, general, general_rule, widths)
  bs_w = tw_four(widths)
  bs_drawn = bs_w[0] > 0 || bs_w[1] > 0 || bs_w[2] > 0 || bs_w[3] > 0
  bs_names = ["t", "r", "b", "l"]
  bs_said = ["top", "right", "bottom", "left"]
  bs_first = nil
  bs_first_side = ""
  bs_class = ""
  for bs_i in range(0, 4)
    bs_side = sides[bs_names[bs_i]]
    bs_counts = bs_drawn ? bs_w[bs_i] > 0 : !bs_side.nil?
    if bs_counts
      bs_own = !bs_side.nil? && bs_side[1] >= general_rule
      bs_c = bs_own ? bs_side[0] : (general ?? "border.subtle")
      bs_class = bs_side[2] if bs_own && bs_class == ""
      if bs_first.nil?
        bs_first = bs_c
        bs_first_side = bs_said[bs_i]
      elsif bs_c != bs_first
        bs_named = bs_class
        bs_named = sides[sides.keys()[0]][2] if bs_named == ""
        throw tw_no(bs_named, "a box has one border colour for its four sides, and this one would draw the " + bs_first_side + " " + bs_first + " and the " + bs_said[bs_i] + " " + bs_c + "; colour every side that is drawn the same, or draw only this one")
      end
    end
  end
  bs_first ?? general
end

def tw_bc_strip(style)
  bp_out = {}
  for bp_key in style.keys()
    bp_out[bp_key] = style[bp_key] unless bp_key == "tw_bc" || bp_key == "tw_bc_lvl"
  end
  bp_out
end

def tw_four(v)
  return [0, 0, 0, 0] if v.nil?
  return [v[0], v[1], v[2], v[3]] if v.class == "array" && v.length() == 4
  return [v[0], v[1], v[0], v[1]] if v.class == "array" && v.length() == 2

  [v, v, v, v]
end

def tw_inset(head, rest, whole)
  ti_which = {"top": "t", "right": "r", "bottom": "b", "left": "l"}[head] ?? ""
  ti_v = rest
  if head == "inset" && (rest.starts_with?("x-") || rest.starts_with?("y-"))
    ti_which = rest.substring(0, 1)
    ti_v = rest.substring(2, rest.length())
  end
  if ti_v == "auto" || ti_v == "full" || ti_v == "px" || ti_v.index_of("/") >= 0 || ti_v.starts_with?("[")
    throw tw_no(whole, "an offset is a margin from the stack's edge, so it is a step on the space scale: top-0, right-4, inset-x-0")
  end
  {"inset": {"sides": tw_sides(ti_which), "v": tw_space(ti_v, whole)}}
end

def tw_inset_across(style, inset, m)
  ia_l = inset["l"]
  ia_r = inset["r"]
  return style if ia_l.nil? && ia_r.nil?

  ia_out = style.merge({})
  if !ia_l.nil? && !ia_r.nil? && ia_out["width"].nil?
    throw tw_no(inset["c"], "an offset from both edges is the stack's width less the two, and EUI has no such length; write inset-x-0 for the whole of it, or one edge and a w-*") unless ia_l == 0 && ia_r == 0

    ia_out["width"] = "100%"
    ia_out["position"] = "absolute_start"
    return ia_out
  end
  ia_side = ia_l.nil? ? "r" : "l"
  ia_by = ia_l.nil? ? ia_r : ia_l
  ia_out["position"] = ia_l.nil? ? "absolute_end" : "absolute_start"
  if ia_by > 0
    throw tw_no(inset["c"], "an offset is laid on as a margin from the edge, and that side has a margin already") if m[ia_l.nil? ? 1 : 3] != 0

    ia_out["margin"] = tw_edges(ia_out["margin"], tw_sides(ia_side), ia_by)
  end
  ia_out
end

def tw_aspect(rest, whole)
  return {"set": {"tw_aspect": [1, 1, whole]}} if rest == "square"
  return {"set": {"tw_aspect": [16, 9, whole]}} if rest == "video"

  ta_r = rest.starts_with?("[") && rest.ends_with?("]") ? rest.substring(1, rest.length() - 1) : rest
  ta_parts = ta_r.split("/")
  if ta_parts.length() == 2 && tw_numeric?(ta_parts[0]) && tw_numeric?(ta_parts[1]) && float(ta_parts[0]) > 0.0 && float(ta_parts[1]) > 0.0
    return {"set": {"tw_aspect": [float(ta_parts[0]), float(ta_parts[1]), whole]}}
  end
  throw tw_no(whole, "an aspect ratio is aspect-square, aspect-video, aspect-W/H or aspect-[W/H]")
end

def tw_place_settle(out)
  pl_s = out["s"]
  # `ml-auto md:ml-0`: an auto margin every side of which was undone.
  pl_auto = pl_s["tw_auto"]
  if !pl_auto.nil? && pl_auto["t"] != true && pl_auto["r"] != true && pl_auto["b"] != true && pl_auto["l"] != true
    pl_s = tw_without(pl_s, "tw_auto")
    out["s"] = pl_s
  end
  pl_ratio = pl_s["tw_aspect"]
  pl_in = pl_s["tw_inset"]
  return out if pl_ratio.nil? && pl_in.nil?

  pl_out = tw_without(tw_without(pl_s, "tw_aspect"), "tw_inset")
  if !pl_ratio.nil? && pl_ratio != "auto"
    pl_w = pl_out["width"]
    pl_h = pl_out["height"]
    pl_w_px = !pl_w.nil? && pl_w.class == "int"
    pl_h_px = !pl_h.nil? && pl_h.class == "int"
    if pl_w_px && (pl_h.nil? || pl_h == "auto")
      pl_out["height"] = int((float(pl_w) * float(pl_ratio[1]) / float(pl_ratio[0])).round())
    elsif pl_h_px && (pl_w.nil? || pl_w == "auto")
      pl_out["width"] = int((float(pl_h) * float(pl_ratio[0]) / float(pl_ratio[1])).round())
    elsif !(pl_w_px && pl_h_px)
      throw tw_no(pl_ratio[2], "a ratio is the height worked out from a width in px, or the width from a height, and EUI lays out no ratio of its own; give the box w-N or h-N")
    end
  end
  unless pl_in.nil?
    throw tw_no(pl_in["c"], "an offset places an absolute node against its stack; write absolute with it, and put the node in a stack()") unless pl_out["position"] == "absolute"

    pl_m = tw_four(pl_out["margin"])
    pl_t = pl_in["t"]
    pl_b = pl_in["b"]
    if !pl_t.nil? && !pl_b.nil? && pl_out["height"].nil?
      throw tw_no(pl_in["c"], "an offset from both edges is the stack's height less the two, and EUI has no such length; write inset-y-0 for the whole of it, or one edge and an h-*") unless pl_t == 0 && pl_b == 0

      pl_out["height"] = "100%"
      pl_out["self"] = "start"
    elsif !pl_t.nil?
      # With a height, the top wins, as it does in CSS.
      pl_out["self"] = "start"
      throw tw_no(pl_in["c"], "an offset is laid on as a margin from the edge, and that side has a margin already") if pl_t > 0 && pl_m[0] != 0
      pl_out["margin"] = tw_edges(pl_out["margin"], tw_sides("t"), pl_t) if pl_t > 0
    elsif !pl_b.nil?
      pl_out["self"] = "end"
      throw tw_no(pl_in["c"], "an offset is laid on as a margin from the edge, and that side has a margin already") if pl_b > 0 && pl_m[2] != 0
      pl_out["margin"] = tw_edges(pl_out["margin"], tw_sides("b"), pl_b) if pl_b > 0
    end
    pl_out = tw_inset_across(pl_out, pl_in, pl_m)
  end
  out["s"] = pl_out
  out
end

def tw_divider(rest, whole)
  if rest == "x" || rest == "y"
    dr_one = {}
    dr_one[rest] = 1
    return {"set": {}, "divide": dr_one}
  end
  if rest.starts_with?("x-") || rest.starts_with?("y-")
    dr_width = rest.substring(2, rest.length())
    if dr_width == "reverse"
      # divide-y-reverse: the rule on the trailing edge, for a reversed box.
      dr_rev = {}
      dr_rev[rest.substring(0, 1) + "_rev"] = true
      return {"set": {}, "divide": dr_rev}
    end

    dr_rule = {}
    dr_rule[rest.substring(0, 1)] = tw_border_width(dr_width, whole)
    return {"set": {}, "divide": dr_rule}
  end
  if ["solid", "dashed", "dotted", "double", "none"].includes?(rest)
    throw tw_no(whole, "a border is always solid; divide-y-0 removes the rules")
  end
  {"set": {}, "divide": {"color": tw_colour("border", rest, whole)}}
end

def tw_gap_settle(style, gaps, display)
  return style if gaps.keys().length() == 0 || display == "none"

  gs_out = style.merge({})
  gs_wraps = style["wrap"] == "wrap" || style["wrap"] == "wrap_reverse"
  gs_flow = display == "row" || display == "column"
  gs_main = display == "column" ? "y" : "x"
  gs_cross = display == "column" ? "x" : "y"
  gs_gx = gaps["x"]
  gs_gy = gaps["y"]
  if !gs_gx.nil? || !gs_gy.nil?
    gs_some = (gs_gx ?? gs_gy)["c"]
    throw tw_no(gs_some, "a stack lays its children over each other and has no gap") if display == "stack"

    gs_col = gs_gx.nil? ? (style["gap"] ?? 0) : gs_gx["v"]
    gs_row = gs_gy.nil? ? (style["gap"] ?? 0) : gs_gy["v"]
    if gs_flow && !gs_wraps
      gs_along = gs_main == "x" ? gs_col : gs_row
      gs_other = gaps[gs_cross]
      if !gs_other.nil? && gs_other["v"] != 0 && gs_other["v"] != gs_along
        throw tw_no(gs_other["c"], "EUI has one gap, and this " + display + " runs along " + gs_main + ": gap-" + gs_cross + " spaces nothing on it that does not wrap; write gap-" + gs_main + "-N or gap-N")
      end
      gs_out["gap"] = gs_along
    else
      if gs_col != gs_row
        gs_what = gs_flow ? "a wrapping " + display + " spaces its lines as well as its children" : "a grid spaces its rows as well as its columns"
        throw tw_no(gs_some, "EUI has one gap for both axes, and " + gs_what + "; write gap-N, or gap-x and gap-y the same")
      end
      gs_out["gap"] = gs_col
    end
  end
  for gs_axis in ["x", "y"]
    gs_space = gaps["space_" + gs_axis]
    if !gs_space.nil? && gs_space["v"] != 0
      gs_cls = gs_space["c"]
      throw tw_no(gs_cls, "space-" + gs_axis + " is a margin on every child but the first, and a " + display + " does not lay its children in a line; write gap-N") unless gs_flow
      if gs_axis != gs_main
        throw tw_no(gs_cls, "on a " + display + ", space-" + gs_axis + " puts a margin across the line, not between the children along it; write space-" + gs_main + "-N, or turn the box with " + (gs_main == "x" ? "flex-col" : "flex"))
      end
      throw tw_no(gs_cls, "on a wrapping " + display + ", space-" + gs_axis + " leaves the wrapped lines unspaced and their first child indented; write gap-N") if gs_wraps
      if (gs_out["gap"] ?? 0) != 0
        throw tw_no(gs_cls, "a gap and a space add up, and EUI has one gap; write one of them")
      end
      gs_out["gap"] = gs_space["v"]
    end
  end
  gs_out
end

def tw_divide(kids, rule)
  dv_out = []
  dv_seen = false
  for dv_kid in kids
    if dv_kid.nil? || dv_seen == false
      dv_out = dv_out.concat([dv_kid])
      dv_seen = true unless dv_kid.nil?
    else
      dv_out = dv_out.concat([tw_restyle_kid(dv_kid, fn(st) { tw_divide_style(st, rule) })])
    end
  end
  dv_out
end

def tw_restyle_kid(kid, f)
  rk_new = kid.merge({})
  rk_new["s"] = f(kid["s"] ?? {})
  rk_on = kid["on"]
  return rk_new if rk_on.nil? || rk_on.class != "hash"

  rk_handlers = {}
  for rk_event in rk_on.keys()
    rk_handlers[rk_event] = tw_restyle_handler(rk_on[rk_event], f)
  end
  rk_new["on"] = rk_handlers
  rk_new
end

def tw_restyle_handler(h, f)
  return h unless h.class == "hash"
  return h unless h["local"].class == "string" && h["styles"].class == "hash"
  return h unless h["local"].starts_with?("self.style = @")

  rh_styles = {}
  for rh_name in h["styles"].keys()
    rh_styles[rh_name] = f(h["styles"][rh_name])
  end
  h.merge({"styles": rh_styles})
end

def tw_divide_style(st, rule)
  ds_out = st.merge({})
  ds_top = [true, false, false, false]
  ds_bottom = [false, false, true, false]
  ds_left = [false, false, false, true]
  ds_right = [false, true, false, false]
  unless rule["y"].nil?
    ds_rev = rule["y_rev"] == true
    ds_ruled = tw_edges(ds_out["border"], ds_rev ? ds_bottom : ds_top, rule["y"])
    ds_out["border"] = tw_edges(ds_ruled, ds_rev ? ds_top : ds_bottom, 0)
  end
  unless rule["x"].nil?
    ds_rev = rule["x_rev"] == true
    ds_ruled = tw_edges(ds_out["border"], ds_rev ? ds_right : ds_left, rule["x"])
    ds_out["border"] = tw_edges(ds_ruled, ds_rev ? ds_left : ds_right, 0)
  end
  ds_out["border_color"] = rule["color"] ?? (st["border_color"] ?? "border.subtle")
  ds_out
end

def tw_case_class(mode)
  return "uppercase" if mode == "upper"
  return "lowercase" if mode == "lower"
  return "capitalize" if mode == "capital"

  "normal-case"
end

def tw_case_apply(content, mode)
  return content.upcase() if mode == "upper"
  return content.downcase() if mode == "lower"
  return content unless mode == "capital"

  ca_out = ""
  ca_prev = " "
  for ca_ch in content.chars()
    ca_start = ca_prev == " " || ca_prev == "\n" || ca_prev == "\t"
    ca_out = ca_out + (ca_start ? ca_ch.upcase() : ca_ch)
    ca_prev = ca_ch
  end
  ca_out
end

def tw_text(content, style)
  twt_mode = style["tw_case"]
  twt_style = {}
  for twt_key in style.keys()
    twt_style[twt_key] = style[twt_key] unless twt_key == "tw_case"
  end
  twt_said = content.class == "string" ? tw_case_apply(content, twt_mode) : content
  {
    "k": "text",
    "t": twt_said,
    "s": twt_style
  }
end

def tw_examples()
  [
    "p-0", "p-0.5", "p-1", "p-2", "p-3", "p-4", "p-5", "p-6", "p-8", "p-10", "p-12", "p-16", "p-24",
    "py-1.5", "px-2.5", "gap-3.5", "p-20", "py-32", "flex-col space-y-1.5",
    "px-4", "py-2", "pt-1", "pr-2", "pb-3", "pl-6", "m-2", "mx-4", "my-1", "mt-2", "mr-3", "mb-4", "ml-1", "gap-3",
    "w-64", "h-10", "w-1.5", "w-full", "w-auto", "w-px", "w-1/2", "w-2/3", "w-[320px]", "h-[50%]", "size-8",
    "basis-0", "basis-1/3", "min-w-0", "min-h-0", "min-w-full", "max-w-sm", "max-w-7xl", "max-w-full", "max-h-96",
    "flex", "inline-flex", "flex-row", "flex-col", "grid", "grid-cols-3", "hidden",
    "flex-wrap", "flex-nowrap", "flex-wrap-reverse",
    "items-start", "items-center", "items-end", "items-stretch", "items-baseline",
    "justify-start", "justify-center", "justify-end", "justify-between", "justify-around", "justify-evenly",
    "self-auto", "self-start", "self-center", "self-end", "self-stretch", "self-baseline",
    "grow", "grow-0", "shrink", "shrink-0", "flex-1", "flex-auto", "flex-initial", "flex-none",
    "text-xs", "text-sm", "text-base", "text-lg", "text-xl", "text-2xl", "text-3xl", "text-4xl",
    "text-left", "text-center", "text-right", "text-justify",
    "font-normal", "font-medium", "font-semibold", "font-bold", "font-sans", "font-mono",
    "underline", "no-underline", "line-through", "truncate", "line-clamp-2", "line-clamp-none",
    "bg-white", "bg-gray-50", "bg-gray-100", "bg-gray-200", "bg-gray-300", "bg-gray-400", "bg-gray-900", "bg-slate-50",
    "text-gray-900", "text-gray-700", "text-gray-600", "text-gray-500", "text-gray-400", "text-white",
    "border-gray-200", "border-gray-300", "border-gray-400", "ring-gray-300",
    "bg-indigo-600", "bg-indigo-500", "bg-indigo-700", "text-indigo-600",
    "bg-red-50", "text-red-600", "bg-green-50", "text-green-700", "bg-yellow-50", "text-yellow-800",
    "bg-blue-50", "text-blue-600", "ring-red-600/10", "ring-gray-900/5", "ring-black/5",
    "bg-accent", "bg-accent-hover", "text-muted", "text-default", "bg-danger-subtle", "bg-surface-raised",
    "bg-raised", "border-subtle", "border-strong", "text-series-2", "bg-focus-ring",
    "bg-transparent", "bg-[#1E293B]", "bg-[#00000080]", "bg-black/50", "bg-gray-500/75", "bg-white/80",
    "border", "border-0", "border-2", "border-4", "border-x", "border-y", "border-t", "border-b-2", "border-l-4",
    "ring-1", "ring-2", "ring-inset",
    "rounded-none", "rounded-sm", "rounded", "rounded-md", "rounded-lg", "rounded-xl", "rounded-2xl", "rounded-full",
    "shadow-none", "shadow-sm", "shadow", "shadow-md", "shadow-lg", "shadow-xl",
    "opacity-0", "opacity-50", "opacity-100",
    "cursor-pointer", "cursor-default", "cursor-text", "cursor-wait", "cursor-not-allowed", "cursor-grab",
    "cursor-grabbing", "cursor-col-resize", "cursor-row-resize",
    "overflow-hidden", "overflow-clip", "overflow-visible",
    "transition", "transition-colors", "transition-none", "duration-150", "duration-300", "duration-1000",
    "absolute", "z-10", "animate-spin", "animate-pulse", "animate-bounce", "animate-spin animate-pulse",
    "bg-gradient-to-r from-indigo-600 to-[#ff80b5]", "bg-gradient-to-br from-accent via-info to-[#ff80b5]",
    "bg-gradient-to-t from-gray-50 from-10% to-white to-90%", "bg-gradient-to-tl from-red-600 via-yellow-600 via-40% to-green-600",
    "bg-gradient-to-b from-blue-600 to-blue-50 hover:to-indigo-600", "backdrop-blur", "backdrop-blur-sm", "backdrop-blur-lg",
    "hover:bg-gray-50", "active:bg-gray-100", "focus:border-indigo-600", "disabled:opacity-50",
    "focus-visible:bg-gray-50", "focus-visible:outline-2", "focus-visible:ring-2", "focus:outline-none", "outline-none",
    "sm:flex", "md:flex-row", "lg:px-8", "xl:max-w-7xl", "2xl:text-lg", "md:hover:bg-gray-50",
    "flex space-x-4", "flex-col space-y-2", "block space-y-4", "flex gap-x-4", "flex-col gap-y-2", "grid gap-x-4 gap-y-4",
    "block", "relative", "static", "isolate", "select-none", "mx-auto", "my-auto",
    "uppercase", "lowercase", "capitalize", "normal-case",
    "border-b-2 border-b-indigo-600", "border-x border-x-gray-300", "border-t-2 border-transparent hover:border-t-indigo-600", "ring-0",
    "flex-grow", "flex-grow-0", "flex-shrink", "flex-shrink-0", "justify-normal", "justify-stretch", "z-auto", "max-w-none", "max-h-none",
    "font-thin", "font-extralight", "font-light", "font-extrabold", "font-black",
    "appearance-none", "list-none", "resize-none", "pointer-events-auto", "whitespace-normal", "text-wrap", "text-ellipsis",
    "overflow-ellipsis", "antialiased", "visible", "outline-0", "flex space-x-reverse",
    "absolute inset-0", "absolute top-0 left-0", "absolute top-0 right-0", "absolute bottom-4 left-2", "absolute bottom-1 right-1", "absolute inset-x-0 bottom-0", "absolute inset-y-0 right-4",
    "size-10 aspect-square", "w-64 aspect-video", "h-24 aspect-[4/3]", "w-40 aspect-4/3", "aspect-auto",
    "ml-auto", "mr-auto", "mt-auto", "mb-auto", "m-auto"
  ]
end

def check(label, got, want)
  assert_eq(got, want)
end

# The error a class string raises, or "" when it raises none.
def tw_refusal(classes)
  said = ""
  try
    tw(classes)
  catch e
    said = str(e)
  end
  said
end

# The same, at a viewport width.
def tw_refusal_at(classes, width)
  said = ""
  try
    tw(classes, width)
  catch e
    said = str(e)
  end
  said
end

# The error building a box with this style raises, or "".
def node_refusal(style, kids)
  said = ""
  try
    node("box", style, kids)
  catch e
    said = str(e)
  end
  said
end

# Whether the encoder takes a style: `eui_render` raises on anything
# `style_record` refuses, and a refused render is `nil` here.
def encodes?(style)
  enc_got = eui_render({"k": "box", "s": style, "c": []}) rescue nil
  !enc_got.nil?
end

# Whether it takes a whole tree.
def renders?(tree)
  rn_got = eui_render(tree) rescue nil
  !rn_got.nil?
end

describe("tw", fn() {
  test("a class becomes the key and the index it names", fn() {
    check("padding is a space index", tw("p-4")["s"], {"pad": 5})
    check("sides combine", tw("p-4 px-2")["s"], {"pad": [5, 3, 5, 3]})
    check("a gap", tw("gap-3")["s"], {"gap": 4})
    check("half a step", tw("mt-0.5")["s"], {"margin": [1, 0, 0, 0]})
    check("a width is four px a step", tw("w-64")["s"], {"width": 256})
    check("a fraction is a percent", tw("w-1/2")["s"], {"width": "50%"})
    check("full", tw("w-full h-full")["s"], {"width": "100%", "height": "100%"})
    check("an arbitrary length", tw("w-[320px]")["s"], {"width": 320})
    check("a named max width", tw("max-w-md")["s"], {"max_width": 448})
    check("flex", tw("flex flex-col items-center justify-between")["s"], {"display": "column", "align": "center", "justify": "between"})
    check("text sizes are the scale", tw("text-xs")["s"]["size"], 0)
    check("text-sm is one", tw("text-sm")["s"]["size"], 1)
    check("text-4xl is the top", tw("text-4xl")["s"]["size"], 7)
    check("a weight", tw("font-semibold")["s"], {"weight": "semibold"})
    check("text-right is end", tw("text-right")["s"], {"text_align": "end"})
    check("truncate is one line", tw("truncate")["s"], {"clamp": 1})
    check("rounded-lg is radius 2", tw("rounded-lg")["s"], {"radius": 2})
    check("rounded-full is radius 4", tw("rounded-full")["s"], {"radius": 4})
    check("shadow-sm is one", tw("shadow-sm")["s"], {"shadow": 1})
    check("shadow-lg is three", tw("shadow-lg")["s"], {"shadow": 3})
    check("opacity is a byte", tw("opacity-50")["s"], {"opacity": 128})
    check("a duration picks a step", tw("duration-300")["s"], {"transition": "slow"})
    check("grid columns are a prop", tw("grid grid-cols-3")["props"], {"columns": 3})
  })

  test("a colour becomes a role, and which one depends on what it paints", fn() {
    check("white is the raised surface", tw("bg-white")["s"], {"bg": "surface.raised"})
    check("gray-50 is the page", tw("bg-gray-50")["s"], {"bg": "surface.base"})
    check("gray-100 is sunken", tw("bg-gray-100")["s"], {"bg": "surface.sunken"})
    check("text-gray-900 is the ink", tw("text-gray-900")["s"], {"fg": "text.default"})
    check("text-gray-500 is muted", tw("text-gray-500")["s"], {"fg": "text.muted"})
    check("text-gray-400 is disabled", tw("text-gray-400")["s"], {"fg": "text.disabled"})
    check("border-gray-300 is the default edge", tw("border-gray-300")["s"], {"border_color": "border.default"})
    check("border-gray-400 is strong", tw("border-gray-400")["s"], {"border_color": "border.strong"})
    check("slate is a gray", tw("bg-slate-50")["s"], {"bg": "surface.base"})
    check("indigo-600 is the accent", tw("bg-indigo-600")["s"], {"bg": "accent.base"})
    check("indigo-500 is its hover", tw("bg-indigo-500")["s"], {"bg": "accent.hover"})
    check("red-600 is danger", tw("text-red-600")["s"], {"fg": "danger.base"})
    check("green-50 is the success tint", tw("bg-green-50")["s"], {"bg": "success.subtle"})
    check("a role by name", tw("bg-accent text-muted")["s"], {"bg": "accent.base", "fg": "text.muted"})
    check("a dotted role, dashed", tw("bg-danger-subtle")["s"], {"bg": "danger.subtle"})
    check("a literal", tw("bg-[#1E293B]")["s"], {"bg": "#1E293B"})
    check("a scrim", tw("bg-black/50")["s"], {"bg": "#00000080"})
    check("a card's ring is the subtle edge", tw("ring-1 ring-gray-900/5")["s"], {"border": 1, "border_color": "border.subtle"})
    check("transparent is none", tw("bg-transparent")["s"], {"bg": "none"})
  })

  test("borders write sides", fn() {
    check("border is one all round", tw("border")["s"], {"border": 1})
    check("border-b is the bottom", tw("border-b")["s"], {"border": [0, 0, 1, 0]})
    check("border-l-4", tw("border-l-4")["s"], {"border": [0, 0, 0, 4]})
    check("later sides add to earlier", tw("border border-t-2")["s"], {"border": [2, 1, 1, 1]})
  })

  test("hover, active, focus and disabled are deltas over the resting style", fn() {
    got = tw("bg-white px-4 hover:bg-gray-50 active:bg-gray-100 focus:border-indigo-600 disabled:opacity-50")
    check("resting", got["s"], {"bg": "surface.raised", "pad": [0, 5, 0, 5]})
    check("hover", got["hover"], {"bg": "surface.base"})
    check("press", got["press"], {"bg": "surface.sunken"})
    check("focus", got["focus"], {"border_color": "accent.base"})
    check("disabled", got["disabled"], {"opacity": 128})
    check("a state's sides start from the resting ones", tw("p-2 hover:pt-4")["hover"], {"pad": [5, 3, 3, 3]})
    check("tw_style lays disabled over", tw_style("bg-white disabled:opacity-50", true), {"bg": "surface.raised", "opacity": 128})
  })

  test("a node with states gets local handlers, and one without gets none", fn() {
    plain = node("box", {"tw": "flex gap-2 bg-white"}, [])
    check("the classes became the style", plain["s"], {"display": "row", "gap": 3, "bg": "surface.raised"})
    check("no handlers", plain["on"], null)
    live = node("box", {"tw": "bg-white hover:bg-gray-50", "grow": 1}, [])
    check("a key written beside tw wins", live["s"], {"bg": "surface.raised", "grow": 1})
    check("hover is local", live["on"]["pointer_enter"]["local"], "self.style = @hover")
    check("and carries the hover style", live["on"]["pointer_enter"]["styles"]["hover"], {"bg": "surface.base", "grow": 1})
    focused = node("box", {"tw": "border focus:border-indigo-600"}, [])
    check("focus wires a focus and a blur", focused["on"]["focus"]["styles"]["focus"]["border_color"], "accent.base")
    check("and no pointer states", focused["on"]["pointer_enter"], null)
    check("column still wins its display", column({"tw": "flex gap-1"}, [])["s"]["display"], "column")
  })

  test("a result is a copy, so writing into one does not reach the next", fn() {
    first = tw("gap-2")
    first["s"]["display"] = "column"
    check("the memo is untouched", tw("gap-2")["s"], {"gap": 3})
  })

  test("a breakpoint applies from its width up, mobile first", fn() {
    check("one argument still works", tw("flex")["s"], {"display": "row"})
    check("below md the bare class", tw("p-2 md:p-4", 767)["s"], {"pad": 3})
    check("from md the md one", tw("p-2 md:p-4", 768)["s"], {"pad": 5})
    check("the memo is per breakpoint, not per string", tw("p-2 md:p-4", 500)["s"], {"pad": 3})
    check("written order does not matter", tw("md:p-4 p-2", 800)["s"], {"pad": 5})
    check("the larger breakpoint wins", tw("lg:p-8 md:p-4 p-2", 1100)["s"], {"pad": 8})
    check("and not before its width", tw("lg:p-8 md:p-4 p-2", 900)["s"], {"pad": 5})
    check("sm", tw("sm:flex", 640)["s"], {"display": "row"})
    check("2xl", tw("text-sm 2xl:text-lg", 1536)["s"], {"size": 3})
    check("xl is not yet 2xl", tw("text-sm 2xl:text-lg", 1535)["s"], {"size": 1})
    check("hidden on a phone", tw("hidden md:flex", 500)["s"], {"display": "none"})
    check("shown from md", tw("hidden md:flex", 900)["s"], {"display": "row"})
    check("sides from a breakpoint add to the bare ones", tw("p-2 md:px-4", 800)["s"], {"pad": [3, 5, 3, 5]})
    check("a state under a breakpoint", tw("bg-white md:hover:bg-gray-50", 900)["hover"], {"bg": "surface.base"})
    check("which is not there below it", tw("bg-white md:hover:bg-gray-50", 500)["hover"], {})
    check("a state starts from the resting style at this width", tw("p-2 md:p-4 hover:pt-8", 900)["hover"], {"pad": [8, 5, 5, 5]})
    check("tw_style takes the width third", tw_style("text-sm md:text-base", false, 800), {"size": 2})
    wide = node("box", {"tw": "flex md:flex-col", "vw": 1000}, [])
    check("a node takes it as vw, and vw is no style key", wide["s"], {"display": "column"})
    check("a node without vw raises", node_refusal({"tw": "md:flex"}, []).index_of("needs the viewport width") >= 0, true)
    check("no width names the class and the width", tw_refusal("flex md:flex-col").starts_with?("tw: 'md:flex-col' needs the viewport width — a breakpoint class applies from 768 px up"), true)
    check("a refused class raises below its breakpoint too", tw_refusal_at("lg:tracking-tight", 320).index_of("letter-spacing") >= 0, true)
    check("two breakpoints on a class", tw_refusal_at("md:lg:flex", 1200).index_of("one breakpoint per class") >= 0, true)
    check("max- is not mobile first", tw_refusal_at("max-md:hidden", 1200).index_of("mobile first") >= 0, true)
  })

  test("space and gap on one axis become the one gap, where they are the same thing", fn() {
    check("space-x on a row", tw("flex space-x-4")["s"], {"display": "row", "gap": 5})
    check("space-y on a column", column({"tw": "space-y-2"}, [])["s"], {"display": "column", "gap": 3})
    check("block is a column", tw("block space-y-4")["s"], {"display": "column", "gap": 5})
    check("space-0 is nothing, across the line or not", tw("flex space-y-0 space-x-2")["s"], {"display": "row", "gap": 3})
    check("turned at md", tw("flex flex-col space-y-4 md:flex-row md:space-y-0 md:space-x-4", 900)["s"], {"display": "row", "gap": 5})
    check("and not before", tw("flex flex-col space-y-4 md:flex-row md:space-y-0 md:space-x-4", 500)["s"], {"display": "column", "gap": 5})
    check("gap-x on a row", tw("flex gap-x-4")["s"]["gap"], 5)
    check("gap-y on a column", tw("flex-col gap-y-2")["s"]["gap"], 3)
    check("both the same on a grid", tw("grid gap-x-4 gap-y-4")["s"]["gap"], 5)
    check("gap-x overrides gap-N along a row", tw("flex gap-4 gap-x-2")["s"]["gap"], 3)
    check("a zero across the line", tw("flex gap-x-4 gap-y-0")["s"]["gap"], 5)
    check("both equal on a wrapping row", tw("flex flex-wrap gap-x-2 gap-y-2")["s"]["gap"], 3)
    check("the node's own direction decides", row({"tw": "gap-x-3"}, [])["s"]["gap"], 4)
    check("space-y on a row", tw_refusal("flex space-y-4").index_of("across the line") >= 0, true)
    check("space and gap add up", tw_refusal("flex gap-2 space-x-4").index_of("add up") >= 0, true)
    check("a gap beside the classes adds up too", node_refusal({"tw": "flex space-x-4", "gap": 2}, []).index_of("add up") >= 0, true)
    check("space on a wrapping row", tw_refusal("flex flex-wrap space-x-2").index_of("wrapping") >= 0, true)
    check("space on a grid", tw_refusal("grid space-x-2").index_of("write gap-N") >= 0, true)
    check("no direction to settle against", tw_refusal("space-y-4").index_of("which way they run") >= 0, true)
    check("a second gap across a row", tw_refusal("flex gap-x-4 gap-y-2").index_of("EUI has one gap") >= 0, true)
    check("gap-x on a column spaces nothing", tw_refusal("flex-col gap-x-4").index_of("spaces nothing") >= 0, true)
    check("a wrapping row spaces its lines", tw_refusal("flex flex-wrap gap-x-4").index_of("both axes") >= 0, true)
    check("a grid spaces its rows", tw_refusal("grid gap-x-4").index_of("both axes") >= 0, true)
    check("no states", tw_refusal("flex hover:space-x-2").index_of("no states") >= 0, true)
    check("space-x-reverse is the same gap", tw("flex space-x-4 space-x-reverse")["s"], {"display": "row", "gap": 5})
  })

  test("a divider is laid onto every child but the first", fn() {
    shared = text("shared", {})
    live = node("box", {"tw": "hover:bg-gray-50"}, [])
    kid_b = row({"gap": 2}, [])
    kid_b["key"] = "b"
    list = column({"tw": "divide-y divide-gray-200"}, [nil, text("a", {}), kid_b, text("c", nil), shared, live])
    kids = list["c"]
    check("a nil child is left and not counted", kids[0], nil)
    check("the first child is untouched", kids[1]["s"], {})
    check("the next takes a top rule and no bottom", kids[2]["s"], {"display": "row", "gap": 2, "border": [1, 0, 0, 0], "border_color": "border.subtle"})
    check("its key goes with it", kids[2]["key"], "b")
    check("a text child with no style", kids[3]["s"], {"border": [1, 0, 0, 0], "border_color": "border.subtle"})
    check("a shared child is copied, not written", shared["s"], {})
    check("its hover style keeps the rule", kids[5]["on"]["pointer_enter"]["styles"]["hover"]["border"], [1, 0, 0, 0])
    check("and its resting one", kids[5]["on"]["pointer_leave"]["styles"]["base"]["border_color"], "border.subtle")
    check("the parent's style has no trace of it", list["s"], {"display": "column"})
    side = row({"tw": "divide-x-2 divide-gray-300"}, [text("a", {}), text("b", {"border": 1, "border_color": "danger.base"})])
    check("divide-x is the left edge, and the colour overrides", side["c"][1]["s"], {"border": [1, 0, 1, 2], "border_color": "border.default"})
    own = row({"tw": "divide-x"}, [text("a", {}), text("b", {"border_color": "danger.base"})])
    check("with no colour the child's own", own["c"][1]["s"]["border_color"], "danger.base")
    turned = node("box", {"tw": "flex flex-col divide-y md:flex-row md:divide-y-0 md:divide-x", "vw": 900}, [text("a", {}), text("b", {})])
    check("each axis on its own", turned["c"][1]["s"]["border"], [0, 0, 0, 1])
    both = node("box", {"tw": "flex divide-y divide-x"}, [text("a", {}), text("b", {})])
    check("and both at once", both["c"][1]["s"]["border"], [1, 0, 0, 1])
    check("the tree encodes", renders?(list), true)
    check("tw() alone has no children to divide", tw_refusal("divide-y").index_of("written where they are") >= 0, true)
    check("no dashes", tw_refusal("divide-dashed").index_of("always solid") >= 0, true)
    flipped = column({"tw": "divide-y divide-y-reverse"}, [text("a", {}), text("b", {})])
    check("divide-y-reverse rules the trailing edge", flipped["c"][1]["s"]["border"], [0, 0, 1, 0])
  })

  test("auto margins, positioning and flow, where EUI already behaves so", fn() {
    check("mx-auto centres across a column", tw("mx-auto")["s"], {"self": "center"})
    check("my-auto across a row", tw("my-auto")["s"], {"self": "center"})
    check("relative is in flow", tw("relative")["s"], {"position": "flow"})
    check("and undoes absolute from a breakpoint", tw("absolute md:relative", 900)["s"], {"position": "flow"})
    check("static", tw("static")["s"], {"position": "flow"})
    check("block is a column", tw("block")["s"], {"display": "column"})
    check("hidden until md", tw("hidden md:block", 800)["s"], {"display": "column"})
    check("isolate is what every box is", tw("isolate")["s"], {})
    check("select-none is what every box is", tw("select-none")["s"], {})
    check("inline flow is still refused", tw_refusal("inline-block").index_of("no inline flow") >= 0, true)
    check("fixed is still refused", tw_refusal("fixed").index_of("positioning") >= 0, true)
  })

  test("an auto margin is placed by the box the node is put in", fn() {
    spacer_node = {"k": "spacer", "s": {"grow": 1}}
    along = row({"gap": 2}, [text("a", {}), text("b", tw_style("ml-auto"))])
    check("ml-auto in a row is a spacer before it", along["c"], [text("a", {}), spacer_node, text("b", {})])
    after = row({}, [text("a", tw_style("mr-auto")), text("b", {})])
    check("mr-auto a spacer after it", after["c"], [text("a", {}), spacer_node, text("b", {})])
    down = column({}, [text("a", {}), text("b", tw_style("mt-auto"))])
    check("mt-auto in a column", down["c"], [text("a", {}), spacer_node, text("b", {})])
    across = column({}, [text("a", tw_style("ml-auto")), text("b", tw_style("mr-auto"))])
    check("across a column ml-auto is self end", across["c"][0]["s"], {"self": "end"})
    check("and mr-auto self start", across["c"][1]["s"], {"self": "start"})
    check("mt-auto across a row", row({}, [text("a", tw_style("mt-auto"))])["c"][0]["s"], {"self": "end"})
    both = row({}, [text("a", tw_style("m-auto"))])
    check("m-auto centres both ways", both["c"], [spacer_node, text("a", {"self": "center"}), spacer_node])
    check("the parent needs no classes of its own", node("box", {"tw": "flex items-center"}, [text("a", tw_style("ml-auto"))])["c"].length(), 2)
    live = node("box", {"tw": "mt-auto hover:bg-gray-50"}, [])
    placed = row({}, [live])["c"][0]
    check("the hover style keeps its place", placed["on"]["pointer_enter"]["styles"]["hover"], {"bg": "surface.base", "self": "end"})
    check("and the resting one has no marker", placed["on"]["pointer_leave"]["styles"]["base"], {"self": "end"})
    check("the number goes", tw("ml-4 ml-auto")["s"]["margin"], 0)
    check("a number written later undoes it", tw("ml-auto md:ml-4", 900)["s"], {"margin": [0, 0, 0, 5]})
    check("and before the breakpoint it is auto", tw("ml-auto md:ml-4", 500)["s"]["tw_auto"]["l"], true)
    check("the tree encodes", renders?(along), true)
    check("a marker no box placed is refused", encodes?(tw("ml-auto")["s"]), false)
    check("not in a grid", node_refusal({"display": "grid"}, [text("a", tw_style("ml-auto"))]).index_of("lays its children out another way") >= 0, true)
    check("not in a stack", node_refusal({"display": "stack"}, [text("a", tw_style("mt-auto"))]).index_of("stack") >= 0, true)
    check("no states", tw_refusal("hover:ml-auto").index_of("no states") >= 0, true)
    check("mx-auto is still self-center", tw("mx-auto")["s"], {"self": "center"})
  })

  test("a reversed box is its children the other way round", fn() {
    rv = node("box", {"tw": "flex flex-row-reverse"}, [text("1", {}), text("2", {}), text("3", {})])
    check("the children reversed", rv["c"].map(fn(c) { c["t"] }), ["3", "2", "1"])
    check("packed from the far end, as a reversed line is", rv["s"], {"display": "row", "justify": "end"})
    check("justify-end packs from the near one", node("box", {"tw": "flex-col-reverse justify-end"}, [])["s"], {"display": "column", "justify": "start"})
    check("between is symmetric", node("box", {"tw": "flex-row-reverse justify-between"}, [])["s"]["justify"], "between")
    ruled = node("box", {"tw": "flex-col-reverse divide-y"}, [text("1", {}), text("2", {})])
    check("rules laid on in the order given, as the browser does", ruled["c"].map(fn(c) { c["s"] }), [{"border": [1, 0, 0, 0], "border_color": "border.subtle"}, {}])
    check("a direction written later is not reversed", node("box", {"tw": "flex-col-reverse md:flex-row", "vw": 900}, [text("1", {}), text("2", {})])["c"][0]["t"], "1")
    check("the tree encodes", renders?(rv), true)
    check("tw() alone has no children", tw_refusal("flex-row-reverse").index_of("written where they are") >= 0, true)
    check("no states", tw_refusal("hover:flex-row-reverse").index_of("no states") >= 0, true)
  })

  test("one side's border colour is the box's, when the sides drawn agree", fn() {
    check("the one side drawn", tw("border-b-2 border-b-indigo-600")["s"], {"border": [0, 0, 2, 0], "border_color": "accent.base"})
    check("two sides", tw("border-x border-x-gray-300")["s"], {"border": [0, 1, 0, 1], "border_color": "border.default"})
    check("a side's colour over the box's, whatever the order", tw("border-b-2 border-b-indigo-600 border-gray-200")["s"]["border_color"], "accent.base")
    check("every side named", tw("border border-t-red-500 border-r-red-500 border-b-red-500 border-l-red-500")["s"]["border_color"], "danger.base")
    check("a state's side over the resting box", tw("border-b-2 border-transparent hover:border-b-indigo-600")["hover"], {"border_color": "accent.base"})
    check("a state's box colour over a resting side", tw("border-b-2 border-b-red-500 hover:border-gray-300")["hover"], {"border_color": "border.default"})
    check("a wider breakpoint's box colour over a side", tw("border-b border-b-red-500 md:border-gray-300", 900)["s"]["border_color"], "border.default")
    check("the tree encodes", encodes?(tw("border-b-2 border-b-indigo-600")["s"]), true)
    check("two colours drawn", tw_refusal("border border-b-indigo-600").index_of("one border colour") >= 0, true)
    check("and they are named", tw_refusal("border border-b-indigo-600").index_of("top border.subtle and the bottom accent.base") >= 0, true)
    check("ring-0 takes the ring off", tw("ring-1 ring-0")["s"], {"border": 0})
    check("a side's width is still a width", tw("border-l-4")["s"], {"border": [0, 0, 0, 4]})
  })

  test("an offset places an absolute node in its stack", fn() {
    # eui 04 §5: down the stack by the node's own `self`, across by the
    # edge its `position` names (protocol 7), the offset a margin from it.
    check("top-0 left-0", tw("absolute top-0 left-0")["s"], {"position": "absolute_start", "self": "start"})
    check("top-0 right-0", tw("absolute top-0 right-0")["s"], {"position": "absolute_end", "self": "start"})
    check("an offset is a margin from the edge", tw("absolute bottom-4 left-2")["s"], {"position": "absolute_start", "self": "end", "margin": [0, 0, 5, 3]})
    check("right-1 is a right margin", tw("absolute top-0 right-1")["s"], {"position": "absolute_end", "self": "start", "margin": [0, 2, 0, 0]})
    check("inset-0 fills it", tw("absolute inset-0")["s"], {"position": "absolute_start", "width": "100%", "height": "100%", "self": "start"})
    check("inset-x-0 fills across", tw("absolute inset-x-0 bottom-0")["s"], {"position": "absolute_start", "width": "100%", "self": "end"})
    check("left-0 right-0 is inset-x-0", tw("absolute left-0 right-0")["s"], {"position": "absolute_start", "width": "100%"})
    check("with a width left wins", tw("absolute left-2 right-4 w-10")["s"], {"position": "absolute_start", "width": 40, "margin": [0, 0, 0, 3]})
    check("with a height the top wins", tw("absolute top-0 bottom-0 h-10")["s"], {"position": "absolute", "height": 40, "self": "start"})
    check("no offset across leaves it to the stack", tw("absolute top-2")["s"], {"position": "absolute", "self": "start", "margin": [3, 0, 0, 0]})
    badge = node("box", {"display": "stack"}, [node("box", {"tw": "absolute top-0 right-0 size-3 rounded-full bg-red-500"}, [])])
    check("the tree encodes", renders?(badge), true)
    check("absolute is needed", tw_refusal("top-0").index_of("write absolute with it") >= 0, true)
    check("both edges, not zero", tw_refusal("absolute top-2 bottom-2").index_of("inset-y-0") >= 0, true)
    check("both edges across", tw_refusal("absolute left-2 right-2").index_of("inset-x-0") >= 0, true)
    check("a fraction", tw_refusal("absolute top-1/2").index_of("space scale") >= 0, true)
    check("an offset over a margin", tw_refusal("absolute top-2 mt-1").index_of("margin already") >= 0, true)
    check("and across", tw_refusal("absolute right-2 mr-1").index_of("margin already") >= 0, true)
    check("no states", tw_refusal("absolute hover:top-0").index_of("no states") >= 0, true)
  })

  test("a ratio is the length the box was not given", fn() {
    check("square", tw("size-10 aspect-square")["s"], {"width": 40, "height": 40})
    check("video", tw("w-64 aspect-video")["s"], {"width": 256, "height": 144})
    check("from the height", tw("h-24 aspect-[4/3]")["s"], {"height": 96, "width": 128})
    check("a bare ratio", tw("aspect-4/3 w-40")["s"], {"width": 160, "height": 120})
    check("aspect-auto undoes it", tw("aspect-square aspect-auto")["s"], {})
    check("no length to work from", tw_refusal("aspect-square").index_of("w-N or h-N") >= 0, true)
    check("a percentage is no length in px", tw_refusal("w-full aspect-video").index_of("w-N or h-N") >= 0, true)
    check("not a ratio", tw_refusal("aspect-wide").index_of("aspect-square") >= 0, true)
  })

  test("the aliases and the classes that are what EUI already does", fn() {
    check("Tailwind 2's grow and shrink", tw("flex-grow flex-shrink-0")["s"], {"grow": 1, "shrink": 0})
    check("justify-normal", tw("justify-normal")["s"], {"justify": "start"})
    check("z-auto", tw("z-auto")["s"], {"z": 0})
    check("max-w-none", tw("max-w-none")["s"], {"max_width": "auto"})
    check("font-light is the regular face", tw("font-light")["s"], {"weight": "regular"})
    check("font-black the bold", tw("font-black")["s"], {"weight": "bold"})
    check("nothing to reset", tw("appearance-none list-none resize-none pointer-events-auto antialiased visible outline-0")["s"], {})
    check("text wraps already", tw("text-wrap whitespace-normal text-ellipsis")["s"], {})
    check("the screen's height", tw_refusal("h-dvh").index_of("viewport") >= 0, true)
    check("no balancing", tw_refusal("text-balance").index_of("balances nothing") >= 0, true)
    check("no nowrap by another name", tw_refusal("text-nowrap").index_of("truncate") >= 0, true)
  })

  test("a text transform is applied to the string by text()", fn() {
    check("uppercase", text("Hello world", tw_style("uppercase text-xs")), {"k": "text", "t": "HELLO WORLD", "s": {"size": 0}})
    check("lowercase", text("Hello World", tw_style("lowercase"))["t"], "hello world")
    check("capitalize raises each word's first letter", text("hello big world", tw_style("capitalize"))["t"], "Hello Big World")
    check("and leaves the rest as written", text("hello iPhone", tw_style("capitalize"))["t"], "Hello IPhone")
    check("normal-case undoes it", text("Hello", tw_style("uppercase normal-case")), {"k": "text", "t": "Hello", "s": {}})
    check("from a breakpoint", text("ab", tw_style("md:uppercase", false, 900))["t"], "AB")
    check("the node encodes", renders?(text("ab", tw_style("uppercase font-medium"))), true)
    check("a box has no string", node_refusal({"tw": "flex uppercase"}, []).index_of("put it on the text") >= 0, true)
    check("a state cannot", tw_refusal("hover:uppercase").index_of("which a state cannot") >= 0, true)
    check("italic is still refused", tw_refusal("italic").index_of("italic face") >= 0, true)
  })

  test("focus-visible is focus, and the ring it styles is the client's", fn() {
    ring = tw("focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600")
    check("the outline classes say nothing", [ring["s"], ring["focus"]], [{}, {}])
    check("so no handlers", tw_stateful?(ring), false)
    check("a ring under focus-visible", tw("focus-visible:ring-2 focus-visible:ring-indigo-600")["focus"], {})
    check("outline-none at focus", tw("focus:outline-none")["focus"], {})
    check("outline-none at rest", tw("outline-none")["s"], {})
    check("anything else is a focus state", tw("focus-visible:bg-gray-50")["focus"], {"bg": "surface.base"})
    check("focus:ring-2 is still a border", tw("focus:ring-2")["focus"], {"border": 2})
    check("an outline at rest", tw_refusal("outline-2").index_of("focus ring") >= 0, true)
    check("focus-within", tw_refusal("focus-within:bg-gray-50").index_of("not an ancestor") >= 0, true)
  })

  test("the half steps are version 6's space indices 13-17", fn() {
    # 05 §2: appended after 12, so out of pixel order on purpose.
    check("py-1.5 is 6 px", tw("py-1.5")["s"], {"pad": [13, 0, 13, 0]})
    check("px-2.5 is 10 px", tw("px-2.5")["s"], {"pad": [0, 14, 0, 14]})
    check("gap-3.5 is 14 px", tw("gap-3.5")["s"], {"gap": 15})
    check("mt-20 is 80 px", tw("mt-20")["s"], {"margin": [16, 0, 0, 0]})
    check("p-32 is 128 px", tw("p-32")["s"], {"pad": 17})
    check("space-y-1.5 settles into a column's gap", column({"tw": "space-y-1.5"}, [])["s"], {"display": "column", "gap": 13})
    check("the tree encodes", renders?(column({"tw": "py-1.5 px-2.5 gap-3.5"}, [text("a", {})])), true)
  })

  test("a gradient is its classes in any order, and its stops sit where they say", fn() {
    # eui 02 §5.3: one `bg`, however the classes are ordered. Positions are
    # 255ths; a stop given none sits where Tailwind puts it, 0, 50% and 100%.
    two = {"gradient": {"to": "right", "stops": [ ["accent.base", 0], ["#ff80b5", 255] ]}}
    check("to the right", tw("bg-gradient-to-r from-indigo-600 to-[#ff80b5]")["s"], {"bg": two})
    check("in any order", tw("to-[#ff80b5] from-indigo-600 bg-gradient-to-r")["s"], {"bg": two})
    three = tw("bg-gradient-to-tr from-accent via-info via-40% to-danger to-90%")["s"]["bg"]["gradient"]
    check("a corner is the box's own", three["to"], "top right")
    check("three stops, placed", three["stops"], [ ["accent.base", 0], ["info.base", 102], ["danger.base", 230] ])
    check("a via with no position is half way", tw("bg-gradient-to-b from-white via-gray-50 to-gray-100")["s"]["bg"]["gradient"]["stops"][1], ["surface.base", 128])
    check("grays by what they paint behind", tw("bg-gradient-to-b from-gray-50 to-white")["s"]["bg"]["gradient"]["stops"], [ ["surface.base", 0], ["surface.raised", 255] ])
    # A state changes one stop of the gradient the box already has.
    hovered = tw("bg-gradient-to-r from-blue-600 to-blue-50 hover:to-indigo-600")
    check("the resting gradient", hovered["s"]["bg"]["gradient"]["stops"][1], ["info.subtle", 255])
    check("the hovered one", hovered["hover"]["bg"]["gradient"]["stops"], [ ["info.base", 0], ["accent.base", 255] ])
    check("nothing left behind", tw_grad_any?(hovered["s"]) || tw_grad_any?(hovered["hover"]), false)
    check("it encodes", encodes?(tw("bg-gradient-to-br from-accent via-info to-[#ff80b5] rounded-lg shadow-sm")["s"]), true)
  })

  test("pulse and bounce are animations, and animations combine", fn() {
    check("pulse", tw("animate-pulse")["s"], {"animation": "pulse"})
    check("bounce", tw("animate-bounce")["s"], {"animation": "bounce"})
    check("two are both", tw("animate-spin animate-pulse")["s"], {"animation": ["spin", "pulse"]})
    check("three, once each", tw("animate-bounce animate-pulse animate-bounce")["s"], {"animation": ["pulse", "bounce"]})
    check("none clears", tw("animate-spin animate-none")["s"], {"animation": "none"})
    check("and after none, one again", tw("animate-none animate-bounce")["s"], {"animation": "bounce"})
    check("the pair encodes", encodes?(tw("animate-pulse animate-bounce")["s"]), true)
  })

  test("a class with no equivalent is refused by name, with the reason", fn() {
    check("tracking", tw_refusal("tracking-tight").starts_with?("tw: 'tracking-tight' has no EUI equivalent"), true)
    check("leading", tw_refusal("leading-6").index_of("line-height") >= 0, true)
    check("a gradient with no stops", tw_refusal("bg-gradient-to-r").index_of("from-*") >= 0, true)
    check("a stop with no gradient", tw_refusal("from-indigo-600 to-white").index_of("bg-gradient-to-*") >= 0, true)
    check("a stop that is no role", tw_refusal("bg-gradient-to-r from-pink-500 to-white").index_of("accent (indigo-600)") >= 0, true)
    check("a gradient that fades to nothing", tw_refusal("bg-gradient-to-r from-indigo-600").index_of("to-*") >= 0, true)
    check("to transparent", tw_refusal("bg-gradient-to-r from-indigo-600 to-transparent").index_of("transparent version") >= 0, true)
    check("a side that is none", tw_refusal("bg-gradient-to-x").index_of("side or a corner") >= 0, true)
    check("stops backwards", tw_refusal("bg-gradient-to-r from-white from-60% to-gray-50 to-20%").index_of("forwards") >= 0, true)
    check("a ping", tw_refusal("animate-ping").index_of("animate-pulse") >= 0, true)
    check("a corner", tw_refusal("rounded-tl-lg").index_of("four corners") >= 0, true)
    check("translate", tw_refusal("translate-x-1").index_of("transforms") >= 0, true)
    check("ring-offset", tw_refusal("ring-offset-2").index_of("offset") >= 0, true)
    check("a step off the scale lists the steps", tw_refusal("p-7").index_of("0, 0.5, 1, 1.5, 2, 2.5") >= 0, true)
    check("and the two nearest", tw_refusal("py-7").index_of("the nearest are 6 (24 px) and 8 (32 px)") >= 0, true)
    check("between two appended steps", tw_refusal("px-28").index_of("the nearest are 24 (96 px) and 32 (128 px)") >= 0, true)
    check("past the top", tw_refusal("p-40").index_of("the nearest is 32 (128 px)") >= 0, true)
    check("a palette hue that is no role", tw_refusal("bg-purple-600").index_of("accent (indigo-600)") >= 0, true)
    check("the accent has no tint", tw_refusal("bg-indigo-50").index_of("info-subtle") >= 0, true)
    check("dark mode", tw_refusal("dark:bg-gray-900").index_of("roles") >= 0, true)
    check("a negative margin", tw_refusal("-mt-2").index_of("unsigned") >= 0, true)
    check("line height on a size", tw_refusal("text-sm/6").index_of("write text-sm") >= 0, true)
    check("past the text scale", tw_refusal("text-5xl").index_of("text-4xl") >= 0, true)
    check("no nowrap", tw_refusal("whitespace-nowrap").index_of("truncate") >= 0, true)
    check("no tabular figures", tw_refusal("tabular-nums").index_of("font-mono") >= 0, true)
    check("the pointer is taken", tw_refusal("pointer-events-none").index_of("topmost node") >= 0, true)
    check("an unknown class", tw_refusal("wobble").starts_with?("tw: unknown class 'wobble'"), true)
    check("one bad class spoils the string", tw_refusal("flex gap-2 tracking-wide") != "", true)
    check("a good string raises nothing", tw_refusal("flex gap-2 rounded-lg bg-white shadow-sm"), "")
  })

  test("everything tw() accepts, the encoder takes", fn() {
    # One of every shape of class, and each state laid over the resting
    # style the way the handlers declare it -- at a width past every
    # breakpoint, and through text() for a transform, which is not a key.
    for cls in tw_examples()
      got = tw(cls, 1600)
      whole = got["s"].merge(got["hover"]).merge(got["press"]).merge(got["focus"]).merge(got["disabled"])
      made = {"k": "box", "s": whole, "c": []}
      made = text("Ab", whole) unless whole["tw_case"].nil?
      made = row({}, [made]) unless whole["tw_auto"].nil?
      check(cls, renders?(made) ? cls : "refused: " + cls, cls)
    end
    check("and the encoder does refuse", encodes?({"bg": "accent.subtle"}), false)
    check("a marker left on a box too", encodes?(tw("uppercase")["s"]), false)
  })
})
