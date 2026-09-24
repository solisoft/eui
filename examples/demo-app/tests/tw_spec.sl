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
  return nd_made if style.nil? || style["tw"].nil?

  tw_node(nd_made)
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

def tw_split(classes)
  return [] if classes.nil?

  sp_list = classes.class == "array" ? classes : classes.to_s.replace("\n", " ").replace("\t", " ").split(" ")
  sp_list.map(fn(w) { w.to_s.strip() }).filter(fn(w) { w != "" })
end

def tw(classes)
  tw_key = classes.class == "array" ? classes.join(" ") : classes.to_s
  tw_hit = TW_MEMO[tw_key]
  return tw_copy(tw_hit) unless tw_hit.nil?

  tw_fresh = tw_parse(classes)
  TW_MEMO[tw_key] = tw_fresh if TW_MEMO.keys().length() < TW_MEMO_CAP
  tw_copy(tw_fresh)
end

def tw_copy(t)
  {
    "s": t["s"].merge({}),
    "hover": t["hover"].merge({}),
    "press": t["press"].merge({}),
    "focus": t["focus"].merge({}),
    "disabled": t["disabled"].merge({}),
    "props": t["props"].merge({})
  }
end

def tw_parse(classes)
  tw_words = tw_split(classes)
  tw_out = {"s": {}, "hover": {}, "press": {}, "focus": {}, "disabled": {}, "props": {}}
  # Resting classes first, whatever order they were written in, so that a
  # state which patches one side (`hover:pt-4`) starts from the resting sides.
  for tw_word in tw_words
    tw_out = tw_take(tw_out, "s", tw_word, tw_word) if tw_word.index_of(":") < 0
  end
  for tw_word in tw_words
    tw_colon = tw_word.index_of(":")
    if tw_colon >= 0
      tw_state = tw_variant(tw_word.substring(0, tw_colon), tw_word)
      tw_out = tw_take(tw_out, tw_state, tw_word.substring(tw_colon + 1, tw_word.length()), tw_word)
    end
  end
  tw_out
end

def tw_style(classes, disabled = false)
  ts_all = tw(classes)
  return ts_all["s"].merge(ts_all["disabled"]) if disabled == true

  ts_all["s"]
end

def tw_stateful?(t)
  st_n = t["hover"].keys().length() + t["press"].keys().length() + t["focus"].keys().length()
  st_n > 0
end

def tw_node(n)
  tn_style = n["s"]
  tn_t = tw(tn_style["tw"])
  tn_rest = {}
  for tn_key in tn_style.keys()
    tn_rest[tn_key] = tn_style[tn_key] unless tn_key == "tw"
  end
  tn_base = tn_t["s"].merge(tn_rest)
  n["s"] = tn_base
  n["p"] = (n["p"] ?? {}).merge(tn_t["props"]) if tn_t["props"].keys().length() > 0
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

  tv_why = "a variant tw() does not know; hover:, active:, focus: and disabled: are the four states"
  tv_why = "there are no media queries: branch on bp(width) with the viewport the view is given" if ["sm", "md", "lg", "xl", "2xl"].includes?(prefix)
  tv_why = "colours are roles and already follow the viewer's theme; drop the dark: classes" if prefix == "dark"
  tv_why = "the client draws its own ring for keyboard focus; write focus: for the rest" if prefix == "focus-visible" || prefix == "focus-within"
  tv_why = "there are no group or peer states: a local handler restyles one node, by key" if prefix.starts_with?("group") || prefix.starts_with?("peer")
  tv_why = "there are no structural selectors: style the first or last child where it is built" if ["first", "last", "odd", "even", "only"].includes?(prefix)
  tv_why = "there are no pseudo-elements: build the node" if ["before", "after", "placeholder", "file", "marker", "selection"].includes?(prefix)
  throw tw_no(whole, tv_why)
end

def tw_refused(name)
  rf_why = {
    "tracking-": "EUI has no letter-spacing; the 64-byte style record has no byte for it",
    "leading-": "line height comes with the text size; there is no independent line-height",
    "bg-gradient": "EUI has no gradients",
    "from-": "EUI has no gradients",
    "via-": "EUI has no gradients",
    "to-": "EUI has no gradients",
    "ring-offset": "a ring is written as a border, and a border has no offset",
    "rounded-t": "radius is one byte for all four corners",
    "rounded-b": "radius is one byte for all four corners",
    "rounded-l": "radius is one byte for all four corners",
    "rounded-r": "radius is one byte for all four corners",
    "rounded-s": "radius is one byte for all four corners",
    "rounded-e": "radius is one byte for all four corners",
    "divide-": "there are no child selectors; put border-b on each row",
    "space-x-": "there are no child selectors; write gap-N on the parent",
    "space-y-": "there are no child selectors; write gap-N on the parent",
    "gap-x-": "a box has one gap, for both axes; write gap-N",
    "gap-y-": "a box has one gap, for both axes; write gap-N",
    "translate-": "EUI has no transforms",
    "rotate-": "EUI has no transforms",
    "scale-": "EUI has no transforms",
    "skew-": "EUI has no transforms",
    "origin-": "EUI has no transforms",
    "transform": "EUI has no transforms",
    "inset-": "there are no offsets; an absolute node is placed by the stack it is in",
    "top-": "there are no offsets; an absolute node is placed by the stack it is in",
    "bottom-": "there are no offsets; an absolute node is placed by the stack it is in",
    "left-": "there are no offsets; an absolute node is placed by the stack it is in",
    "right-": "there are no offsets; an absolute node is placed by the stack it is in",
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
    "whitespace-": "text wraps at the box edge, and clamp decides how far",
    "break-": "text wraps at the box edge, and clamp decides how far",
    "aspect-": "there is no aspect ratio; give the box a width and a height",
    "object-": "an image fills the box it is given",
    "pointer-events-": "every node with a handler takes the pointer",
    "select-": "only editable nodes select text",
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
    "animate-": "the one animation is animate-spin; entrances are the enter animation",
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
  return "there are no auto margins; centre with justify-center, items-center or self-center" if ["m-auto", "mx-auto", "my-auto", "mt-auto", "mr-auto", "mb-auto", "ml-auto"].includes?(name)
  return "there are no positioning schemes; absolute is the one there is, inside a stack" if ["relative", "static", "fixed", "sticky"].includes?(name)
  return "there is no block or inline flow; a box is flex, flex-col, grid or hidden" if ["block", "inline", "inline-block", "table", "contents", "flow-root"].includes?(name)
  return "the client ships no italic face" if name == "italic" || name == "not-italic"
  return "there are no text transforms; change the string" if ["uppercase", "lowercase", "capitalize", "normal-case"].includes?(name)
  return "children are drawn in the order they are given; reverse the list" if name == "flex-row-reverse" || name == "flex-col-reverse"
  return "the view is given the viewport's size; use it" if ["w-screen", "h-screen", "min-h-screen", "min-w-screen", "max-w-screen"].includes?(name)
  return "a bare ring is three pixels; write ring-1 or ring-2, which become a border" if name == "ring"
  return "there is no container query and no typography plugin; give the box a width" if name == "container" || name == "prose"
  return "a shadow is cast, never inset" if name == "shadow-inner"
  return "a border is always solid; border-0 removes it" if ["border-dashed", "border-dotted", "border-double", "border-solid", "border-none"].includes?(name)

  ""
end

def tw_space_steps()
  {"0": 0, "0.5": 1, "1": 2, "2": 3, "3": 4, "4": 5, "5": 6, "6": 7, "8": 8, "10": 9, "12": 10, "16": 11, "24": 12}
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

  throw tw_no(whole, "'" + value + "' is not on the space scale; the steps are " + sc_steps.keys().join(", "))
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
    "ring-inset": {}
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
    return tw_edge("margin", fa_head.substring(1, fa_head.length()), tw_space(fa_rest, whole))
  end
  return {"set": {"gap": tw_space(fa_rest, whole)}} if fa_head == "gap" && fa_rest.index_of("-") < 0

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

  # bg-*, text-* (sizes and aligns are exact), border-*, ring-*
  return {"set": {"bg": tw_colour("bg", fa_rest, whole)}} if fa_head == "bg" && !fa_rest.starts_with?("gradient")
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
      return tw_edge("border", fa_rest.substring(0, 1), tw_border_width(fa_width, whole))
    end
    return {"set": {"border_color": tw_colour("border", fa_rest, whole)}}
  end
  if fa_head == "ring"
    return tw_edge("border", "", int(fa_rest)) if fa_rest == "1" || fa_rest == "2"
    return nil if fa_rest.starts_with?("offset") || fa_rest.starts_with?("inset")
    if tw_numeric?(fa_rest)
      throw tw_no(whole, "a ring is written as a border, and ring-1 and ring-2 are the widths that read as one")
    end
    return {"set": {"border_color": tw_colour("ring", fa_rest, whole)}}
  end

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

def tw_take(out, variant, name, whole)
  tk_patch = tw_class(name, whole)
  tk_style = out[variant]
  if tk_patch["edge"].nil?
    tk_style = tk_style.merge(tk_patch["set"] ?? {})
  else
    tk_key = tk_patch["edge"]
    tk_from = tk_style[tk_key]
    tk_from = out["s"][tk_key] if tk_from.nil? && variant != "s"
    tk_style = tk_style.merge({})
    tk_style[tk_key] = tw_edges(tk_from, tk_patch["sides"], tk_patch["v"])
  end
  tk_props = tk_patch["props"] ?? {}
  if tk_props.keys().length() > 0
    throw tw_no(whole, "a prop has no states; write grid-cols-N without a variant") if variant != "s"

    out["props"] = out["props"].merge(tk_props)
  end
  out[variant] = tk_style
  out
end

def tw_examples()
  [
    "p-0", "p-0.5", "p-1", "p-2", "p-3", "p-4", "p-5", "p-6", "p-8", "p-10", "p-12", "p-16", "p-24",
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
    "absolute", "z-10", "animate-spin", "backdrop-blur", "backdrop-blur-sm", "backdrop-blur-lg",
    "hover:bg-gray-50", "active:bg-gray-100", "focus:border-indigo-600", "disabled:opacity-50"
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

# Whether the encoder takes a style: `eui_render` raises on anything
# `style_record` refuses, and a refused render is `nil` here.
def encodes?(style)
  enc_got = eui_render({"k": "box", "s": style, "c": []}) rescue nil
  !enc_got.nil?
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

  test("a class with no equivalent is refused by name, with the reason", fn() {
    check("tracking", tw_refusal("tracking-tight").starts_with?("tw: 'tracking-tight' has no EUI equivalent"), true)
    check("leading", tw_refusal("leading-6").index_of("line-height") >= 0, true)
    check("a gradient", tw_refusal("bg-gradient-to-r").index_of("gradients") >= 0, true)
    check("a corner", tw_refusal("rounded-tl-lg").index_of("four corners") >= 0, true)
    check("divide", tw_refusal("divide-y").index_of("border-b on each row") >= 0, true)
    check("space-x", tw_refusal("space-x-4").index_of("gap-N") >= 0, true)
    check("translate", tw_refusal("translate-x-1").index_of("transforms") >= 0, true)
    check("ring-offset", tw_refusal("ring-offset-2").index_of("offset") >= 0, true)
    check("a step off the scale lists the steps", tw_refusal("p-1.5").index_of("0, 0.5, 1, 2") >= 0, true)
    check("a palette hue that is no role", tw_refusal("bg-purple-600").index_of("accent (indigo-600)") >= 0, true)
    check("the accent has no tint", tw_refusal("bg-indigo-50").index_of("info-subtle") >= 0, true)
    check("a breakpoint", tw_refusal("md:flex").index_of("bp(width)") >= 0, true)
    check("dark mode", tw_refusal("dark:bg-gray-900").index_of("roles") >= 0, true)
    check("a negative margin", tw_refusal("-mt-2").index_of("unsigned") >= 0, true)
    check("auto margins", tw_refusal("mx-auto").index_of("auto margins") >= 0, true)
    check("line height on a size", tw_refusal("text-sm/6").index_of("write text-sm") >= 0, true)
    check("past the text scale", tw_refusal("text-5xl").index_of("text-4xl") >= 0, true)
    check("an unknown class", tw_refusal("wobble").starts_with?("tw: unknown class 'wobble'"), true)
    check("one bad class spoils the string", tw_refusal("flex gap-2 tracking-wide") != "", true)
    check("a good string raises nothing", tw_refusal("flex gap-2 rounded-lg bg-white shadow-sm"), "")
  })

  test("everything tw() accepts, the encoder takes", fn() {
    # One of every shape of class, and each state laid over the resting
    # style the way the handlers declare it.
    for cls in tw_examples()
      got = tw(cls)
      whole = got["s"].merge(got["hover"]).merge(got["press"]).merge(got["focus"]).merge(got["disabled"])
      check(cls, encodes?(whole) ? cls : "refused: " + cls, cls)
    end
    check("and the encoder does refuse", encodes?({"bg": "accent.subtle"}), false)
  })
})
