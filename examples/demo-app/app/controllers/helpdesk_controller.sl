# Shiftwise Support: a help desk for a small SaaS company, drawn the way
# Tailwind UI draws an application — a white sidebar, a top bar with the
# search and the person signed in, content on gray-50 — from `tw()` classes,
# the catalogue's builders and theme roles, with no colour written down.
#
# The data and every state change are `app/services/helpdesk_desk.sl`, so
# they are tested without a server (`tests/helpdesk_spec.sl`). This file is
# the view and nothing else, plus the two lines `router_eui` calls.
#
# Every width that text wraps or truncates against is worked out here from
# the viewport, because a text node without a pixel width is measured as if
# it will not wrap (`max_width` does not narrow it). `hdv_lay` is where the
# numbers come from and nothing below invents its own.
#
# Locals carry a prefix per function: Soli's scope is flat across calls, and
# a local that shares a builder's name (`row`, `text`, `card`) rebinds it.

def helpdesk(event_data)
  hd_reduce(event_data["state"] ?? {}, event_data["event"], event_data["params"] ?? {})
end

# ---- Measures ------------------------------------------------------------------

HDV_RAIL_PX = 256
HDV_PANEL_PX = 320

def hdv_lay(ly_state)
  ly_view = ly_state["viewport"] ?? {}
  ly_w = ly_view["width"] ?? 1280
  ly_rail = ly_w >= 1024
  ly_main = ly_rail ? ly_w - HDV_RAIL_PX : ly_w
  ly_pad = 16
  ly_pad = 24 if ly_w >= 640
  ly_pad = 32 if ly_w >= 1024
  ly_inner = ly_main - 2 * ly_pad
  ly_inner = 1216 if ly_inner > 1216
  {
    "w": ly_w,
    "rail": ly_rail,
    "roomy": ly_w >= 640,
    "main": ly_main,
    "pad": ly_pad,
    "inner": ly_inner,
    "table": ly_inner >= 880,
    "split": ly_inner >= 900
  }
end

# ---- Small parts ---------------------------------------------------------------

# Tailwind UI's initials avatar: a filled circle, white letters, the text a
# step smaller than the name beside it.
#
# Two kinds, and the kind is the legend: the team in the accent, customers in
# grey. A role and its own ink (`accent.base` under `accent.on`), because the
# theme guarantees that pair in both modes — white letters on `series.N` are
# white in dark mode too, on a fill that has gone light.
def hdv_avatar(av_letters, av_team, av_px)
  av_size = 0
  av_size = 1 if av_px >= 40
  av_bg = av_team == true ? "accent.base" : "surface.sunken"
  av_ink = av_team == true ? "accent.on" : "text.muted"
  {
    "k": "box",
    "s": {"display": "row", "width": av_px, "height": av_px, "radius": 4, "bg": av_bg, "justify": "center", "align": "center", "shrink": 0, "border": av_team == true ? 0 : 1, "border_color": "border.subtle"},
    "c": [text(av_letters, {"fg": av_ink, "size": av_size, "weight": "semibold"})]
  }
end

def hdv_status_tone(st_status)
  return "info" if st_status == "Open"
  return "warning" if st_status == "Pending"

  "success"
end

def hdv_status(ss_status)
  badge(ss_status, hdv_status_tone(ss_status))
end

# Priority is a dot and a word, not a third badge beside the status: only
# Urgent and High are coloured, so the eye finds the two that matter.
def hdv_prio(pi_prio)
  pi_dot = "border.default"
  pi_dot = "danger.base" if pi_prio == "Urgent"
  pi_dot = "warning.base" if pi_prio == "High"
  pi_ink = pi_prio == "Urgent" ? "text.default" : "text.muted"
  row({"tw": "items-center gap-2 shrink-0"}, [
    {"k": "box", "s": {"width": 8, "height": 8, "radius": 4, "bg": pi_dot, "shrink": 0}},
    text(pi_prio, {"size": 1, "fg": pi_ink, "weight": pi_prio == "Urgent" ? "medium" : "regular"})
  ])
end

def hdv_plan(pl_plan)
  return badge(pl_plan, "success") if pl_plan == "Scale"
  return badge(pl_plan, "info") if pl_plan == "Growth"

  badge(pl_plan, "neutral")
end

# A button with an event, props and a tone — the catalogue's `button` takes
# neither props nor a disabled state, and pagination needs both.
def hdv_button(bt_label, bt_event, bt_props, bt_tone, bt_off = false)
  control({
    "key": "hdb:" + bt_event + ":" + bt_label,
    "tone": bt_tone,
    "size": "md",
    "shape": {"min_height": 36},
    "on": {"click": bt_event},
    "props": bt_props,
    "disabled": bt_off,
    "a11y": {"role": "button", "label": bt_label},
    "c": [text(bt_label, {"weight": "semibold", "size": 1})]
  })
end

def hdv_clickable(ck_node, ck_event, ck_props)
  ck_node["on"] = (ck_node["on"] ?? {}).merge({"click": ck_event})
  ck_node["p"] = ck_props
  ck_node
end

def hdv_text_w(tx_content, tx_classes, tx_px)
  tx_s = tw_style(tx_classes)
  tx_s["width"] = tx_px
  text(tx_content, tx_s)
end

# The heading every page opens with: a title, one line under it, and
# whatever acts on the page on the right.
def hdv_header(hh_lay, hh_title, hh_sub, hh_actions)
  hh_words = column({"tw": "flex-col gap-1 grow min-w-0"}, [
    text(hh_title, tw_style("text-2xl font-semibold text-gray-900")),
    text(hh_sub, tw_style("text-sm text-gray-500"))
  ])
  return column({"tw": "flex-col gap-4 w-full"}, [hh_words].concat(hh_actions)) unless hh_lay["roomy"]

  row({"tw": "items-end gap-4 w-full"}, [hh_words].concat(hh_actions))
end

# ---- The shell -----------------------------------------------------------------

def hdv_nav_items(ni_state)
  ni_open = hd_count(ni_state["tickets"], "Open")
  ni_mine = ni_state["tickets"].filter(fn(t) { t["agent"] == hd_me() && t["status"] != "Solved" }).length()
  [
    {"id": "inbox", "label": "Inbox", "icon": "box", "count": ni_open},
    {"id": "mine", "label": "Assigned to me", "icon": "star", "count": ni_mine},
    {"id": "customers", "label": "Customers", "icon": "users", "count": 0},
    {"id": "reports", "label": "Reports", "icon": "chart", "count": 0}
  ]
end

# Tailwind UI's sidebar row: `rounded-md p-2 text-sm font-semibold gap-x-3`,
# the current one on a grey wash in the accent, a count pill on the right.
def hdv_nav_item(nv_item, nv_here, nv_where)
  nv_kids = [
    icon(nv_item["icon"], {"width": 20, "height": 20, "shrink": 0, "fg": nv_here == true ? "accent.base" : "text.disabled"}),
    text(nv_item["label"], {"size": 1, "weight": "semibold", "grow": 1})
  ]
  if nv_item["count"] > 0
    nv_kids = nv_kids.concat([row({"tw": "items-center px-2 py-0.5 rounded-full bg-white ring-1 ring-gray-200 shrink-0"}, [
      text(str(nv_item["count"]), tw_style("text-xs font-medium text-gray-600"))
    ])])
  end
  control({
    "key": "hdnav:" + nv_where + ":" + nv_item["id"] + (nv_here == true ? ":on" : ":off"),
    "tone": "quiet",
    "selected": nv_here,
    "shape": {"justify": "start", "gap": 4, "pad": [3, 3, 3, 3], "border": 0, "min_width": 0, "width": "100%", "fg": nv_here == true ? "accent.base" : "text.default"},
    "on": {"click": "nav"},
    "props": {"path": nv_item["id"], "label": nv_item["label"], "current": nv_here},
    "a11y": {"role": "link", "label": nv_item["label"], "selected": nv_here},
    "c": nv_kids
  })
end

def hdv_logo
  row({"tw": "items-center gap-3 h-16 shrink-0 px-2"}, [
    {"k": "box", "s": {"display": "row", "width": 32, "height": 32, "radius": 2, "bg": "accent.base", "justify": "center", "align": "center", "shrink": 0}, "c": [
      icon("calendar", {"width": 18, "height": 18, "fg": "accent.on"})
    ]},
    column({"tw": "flex-col gap-0"}, [
      text("Shiftwise", tw_style("text-sm font-semibold text-gray-900")),
      text("Support", tw_style("text-xs text-gray-500"))
    ])
  ])
end

def hdv_nav(nn_state, nn_where)
  nn_section = nn_state["section"]
  nn_rows = hdv_nav_items(nn_state).map(fn(it) { hdv_nav_item(it, it["id"] == nn_section, nn_where) })
  nn_settings = hdv_nav_item({"id": "settings", "label": "Settings", "icon": "settings", "count": 0}, nn_section == "settings", nn_where)
  column({"tw": "flex-col gap-1 w-full grow"}, nn_rows.concat([spacer(), nn_settings]))
end

def hdv_sidebar(sb_state)
  column({"tw": "flex-col gap-2 w-64 h-full bg-white border-r border-gray-200 px-4 pb-4 shrink-0"}, [hdv_logo(), hdv_nav(sb_state, "rail")])
end

# EUI has no placeholder, so one is drawn: a muted line under a transparent
# field, in a stack. Focus puts it out locally (and lights the frame the
# field sits in, Tailwind's focus ring); the next render drops it for good
# once something was typed. `ph_frame` is the box whose border lights up.
def hdv_hint(ph_key, ph_words, ph_pad)
  ph_style = {"size": 1, "fg": "text.disabled", "pad": ph_pad}
  ph_node = text(ph_words, ph_style)
  ph_node["key"] = ph_key
  ph_node
end

def hdv_hint_on(hn_hint_key, hn_frame_key, hn_frame_style, hn_shown)
  hn_ring = hn_frame_style.merge({"border_color": "accent.base"})
  hn_focus = hn_frame_key + ".style = @ring"
  hn_focus = hn_hint_key + ".style = @gone; " + hn_focus if hn_shown
  {
    "focus": {"local": hn_focus, "styles": {"ring": hn_ring, "gone": {"size": 1, "fg": "text.disabled", "opacity": 0}}},
    "blur": {"local": hn_frame_key + ".style = @rest", "styles": {"rest": hn_frame_style}}
  }
end

def hdv_search_box(sx_value, sx_event, sx_px, sx_label)
  sx_frame = "hd_frame_" + sx_event
  sx_hint = "hd_hint_" + sx_event
  sx_empty = (sx_value ?? "") == ""
  sx_box = row({"tw": "items-center gap-2 px-3 bg-white border border-gray-300 rounded-md shadow-sm transition"}, [])
  sx_field = input(sx_value, sx_event, {
    "key": "hdq:" + sx_event,
    "style": {"border": 0, "bg": "none", "shadow": 0, "pad": [2, 0, 2, 0], "width": sx_px - 46},
    "props": {"label": sx_label},
    "on": hdv_hint_on(sx_hint, sx_frame, sx_box["s"].merge({"width": sx_px}), sx_empty)
  })
  sx_well = sx_empty ? stack({"width": sx_px - 46, "align": "center"}, [hdv_hint(sx_hint, sx_label, [2, 0, 2, 0]), sx_field]) : sx_field
  sx_box["c"] = [icon("search", {"width": 16, "height": 16, "fg": "text.disabled", "shrink": 0}), sx_well]
  sx_box["s"]["width"] = sx_px
  sx_box["key"] = sx_frame
  sx_box
end

def hdv_user(us_state, us_lay)
  us_kids = [hdv_avatar("MC", true, 32)]
  us_kids = us_kids.concat([text(hd_me(), tw_style("text-sm font-semibold text-gray-900"))]) if us_lay["roomy"]
  us_kids = us_kids.concat([icon("chevron_down", {"width": 16, "height": 16, "fg": "text.disabled"})])
  us_open = us_state["menu"] == "user"
  us_anchor = control({
    "key": "hd_user",
    "tone": "quiet",
    "shape": {"gap": 3, "pad": [1, 2, 1, 1], "border": 0, "min_width": 0},
    "on": {"click": "menu"},
    "props": {"which": "user"},
    "a11y": {"role": "button", "label": "Account", "expanded": us_open},
    "c": us_kids
  })
  # The menu is its own panel; the popover's frame around it would be a
  # second one, so the popover keeps only its position.
  us_list = menu(["Your profile", "Settings"], "user_pick")
  us_list["s"] = us_list["s"].merge({"border": 0, "shadow": 0, "bg": "none", "pad": 0})
  us_pop = popover(us_anchor, [us_list], us_open)
  us_pop["c"][1]["s"]["pad"] = [1, 0, 1, 0] if us_open
  us_pop
end

def hdv_topbar(tb_state, tb_lay)
  # What is left once the menu button, the account button and the gaps
  # between them are taken out; the account button is 190 with a name, 76
  # without.
  tb_search_px = tb_lay["main"] - 2 * tb_lay["pad"] - (tb_lay["roomy"] == true ? 190 : 76) - 16
  tb_search_px = tb_search_px - 52 unless tb_lay["rail"]
  tb_search_px = 440 if tb_search_px > 440
  tb_kids = []
  tb_kids = [icon_button("≡", "nav_toggle", {}, {"icon": "menu", "name": "Open navigation", "key": "hd_nav_btn"})] unless tb_lay["rail"]
  tb_kids = tb_kids.concat([hdv_search_box(tb_state["q"], "search", tb_search_px, "Search tickets"), spacer(), hdv_user(tb_state, tb_lay)])
  row({"tw": "items-center gap-4 h-16 w-full bg-white border-b border-gray-200 shrink-0", "pad": [0, tb_lay["pad"] == 32 ? 8 : (tb_lay["pad"] == 24 ? 7 : 5), 0, tb_lay["pad"] == 32 ? 8 : (tb_lay["pad"] == 24 ? 7 : 5)]}, tb_kids)
end

# ---- Inbox ---------------------------------------------------------------------

# Underlined tabs with a count pill, Tailwind UI's "tabs with badges".
def hdv_tab(tb_name, tb_count, tb_on)
  control({
    "key": "hdtab:" + tb_name + (tb_on == true ? ":on" : ":off"),
    "tone": tb_on == true ? "tab_on" : "tab",
    "shape": {"pad": [3, 1, 3, 1], "gap": 3, "min_width": 0, "radius": 0, "border": [0, 0, 2, 0]},
    "on": {"click": "filter"},
    "props": {"tab": tb_name},
    "a11y": {"role": "tab", "selected": tb_on, "label": tb_name},
    "c": [
      text(tb_name, {"size": 1, "weight": "medium"}),
      row({"pad": [0, 3, 0, 3], "radius": 4, "bg": tb_on == true ? "info.subtle" : "surface.sunken", "shrink": 0}, [
        text(str(tb_count), {"size": 0, "weight": "medium", "fg": tb_on == true ? "accent.base" : "text.muted"})
      ])
    ]
  })
end

def hdv_tabs(ts_state)
  ts_scope = hd_scope(ts_state)
  ts_cells = hd_statuses().map(fn(s) { hdv_tab(s, hd_count(ts_scope, s), s == ts_state["filter"]) })
  ts_strip = row({"tw": "items-end gap-8 w-full border-b border-gray-200"}, ts_cells)
  ts_strip["p"] = {"role": "tab_list", "orientation": "horizontal"}
  ts_strip
end

# Column widths for the wide table: subject takes what the others leave.
def hdv_cols(cq_inner)
  cq_fixed = [172, 84, 84, 124, 72]
  cq_gaps = 5 * 16
  cq_subject = cq_inner - 2 - 32 - (172 + 84 + 84 + 124 + 72) - cq_gaps
  [cq_subject].concat(cq_fixed)
end

def hdv_assignee(as_name, as_px)
  return row({"tw": "items-center gap-2 shrink-0"}, [
    {"k": "box", "s": {"width": 24, "height": 24, "radius": 4, "border": 1, "border_color": "border.default", "shrink": 0}},
    hdv_text_w("Unassigned", "text-sm text-gray-400 truncate", as_px - 32)
  ]) if (as_name ?? "") == ""

  row({"tw": "items-center gap-2 shrink-0"}, [
    hdv_avatar(hd_initials(as_name), true, 24),
    hdv_text_w(hd_first_name(as_name), "text-sm text-gray-700 truncate", as_px - 32)
  ])
end

def hdv_ticket_row(tr_t, tr_cols)
  tr_c = hd_customer(tr_t["cust"])
  tr_line = row({"tw": "items-center gap-4 w-full px-4 py-3 border-b border-gray-200 bg-white hover:bg-gray-50 cursor-pointer transition"}, [
    column({"tw": "flex-col gap-0.5 shrink-0"}, [
      hdv_text_w(tr_t["subject"], "text-sm font-medium text-gray-900 truncate", tr_cols[0]),
      hdv_text_w("#" + str(tr_t["id"]) + " in " + tr_t["topic"], "text-xs text-gray-500 truncate", tr_cols[0])
    ]),
    row({"tw": "items-center gap-2 shrink-0"}, [
      hdv_avatar(hd_initials(tr_c["name"]), false, 24),
      hdv_text_w(tr_c["name"], "text-sm text-gray-700 truncate", tr_cols[1] - 32)
    ]),
    row({"width": tr_cols[2], "shrink": 0}, [hdv_prio(tr_t["prio"])]),
    row({"width": tr_cols[3], "shrink": 0}, [hdv_status(tr_t["status"])]),
    hdv_assignee(tr_t["agent"], tr_cols[4]),
    hdv_text_w(hd_age_label(tr_t["age"]), "text-sm text-gray-500 text-right", tr_cols[5])
  ])
  keyed("tk:" + str(tr_t["id"]), hdv_clickable(tr_line, "open", {"id": tr_t["id"], "label": tr_t["subject"]}))
end

def hdv_table_head(th_cols)
  th_names = ["Subject", "Customer", "Priority", "Status", "Assignee", "Updated"]
  row({"tw": "items-center gap-4 w-full px-4 py-3 border-b border-gray-300 bg-white"}, range(0, 6).map(fn(i) {
    hdv_text_w(th_names[i], i == 5 ? "text-sm font-semibold text-gray-900 text-right" : "text-sm font-semibold text-gray-900", th_cols[i])
  }))
end

# Narrow: one ticket is three short lines instead of six columns.
def hdv_ticket_item(ti_t, ti_w)
  ti_c = hd_customer(ti_t["cust"])
  ti_line = column({"tw": "flex-col gap-2 w-full px-4 py-4 border-b border-gray-200 bg-white hover:bg-gray-50 cursor-pointer transition"}, [
    row({"tw": "items-center gap-3 w-full"}, [
      hdv_text_w(ti_t["subject"], "text-sm font-semibold text-gray-900 truncate", ti_w - 32 - 56 - 12),
      spacer(),
      text(hd_age_label(ti_t["age"]), tw_style("text-xs text-gray-500"))
    ]),
    hdv_text_w(ti_c["name"] + ", #" + str(ti_t["id"]), "text-sm text-gray-500 truncate", ti_w - 32),
    row({"tw": "items-center gap-3 w-full"}, [hdv_status(ti_t["status"]), hdv_prio(ti_t["prio"]), spacer(), text(hd_first_name(ti_t["agent"]), tw_style("text-xs text-gray-500"))])
  ])
  keyed("tk:" + str(ti_t["id"]), hdv_clickable(ti_line, "open", {"id": ti_t["id"], "label": ti_t["subject"]}))
end

def hdv_empty(em_state)
  em_filter = em_state["filter"].downcase()
  em_q = em_state["q"] ?? ""
  em_title = "No " + em_filter + " tickets"
  em_body = em_filter == "open" ? "Every conversation is waiting on a customer or solved." : "Tickets move here when their status changes."
  em_body = "Nothing " + em_filter + " matches “" + em_q + "”." unless em_q == ""
  em_kids = [
    {"k": "box", "s": {"display": "row", "width": 48, "height": 48, "radius": 4, "bg": "surface.sunken", "justify": "center", "align": "center"}, "c": [
      icon(em_q == "" ? "check" : "search", {"width": 24, "height": 24, "fg": "text.muted"})
    ]},
    text(em_title, tw_style("text-sm font-semibold text-gray-900")),
    text(em_body, tw_style("text-sm text-gray-500 text-center"))
  ]
  column({"tw": "flex-col items-center gap-3 w-full py-12 px-6 bg-white"}, em_kids)
end

def hdv_pager(pg_info)
  pg_says = "Showing " + str(pg_info["from"] + 1) + " to " + str(pg_info["to"]) + " of " + str(pg_info["total"])
  pg_kids = [text(pg_says, tw_style("text-sm text-gray-700")), spacer()]
  if pg_info["pages"] > 1
    pg_kids = pg_kids.concat([
      hdv_button("Previous", "page", {"page": pg_info["page"] - 1}, "neutral", pg_info["page"] <= 1),
      hdv_button("Next", "page", {"page": pg_info["page"] + 1}, "neutral", pg_info["page"] >= pg_info["pages"])
    ])
  end
  row({"tw": "items-center gap-3 w-full px-4 py-3 bg-white"}, pg_kids)
end

def hdv_inbox(ib_state, ib_lay)
  ib_mine = ib_state["section"] == "mine"
  ib_scope = hd_scope(ib_state)
  ib_open = hd_count(ib_scope, "Open")
  ib_urgent = ib_scope.filter(fn(t) { t["status"] == "Open" && t["prio"] == "Urgent" }).length()
  ib_title = ib_mine == true ? "Assigned to me" : "Inbox"
  ib_sub = str(ib_open) + " open, " + (ib_urgent == 0 ? "none" : str(ib_urgent)) + " urgent"
  ib_sub = str(ib_open) + " open tickets are yours" if ib_mine
  ib_q = ib_state["q"] ?? ""
  ib_sub = str(ib_scope.length()) + (ib_scope.length() == 1 ? " ticket matches “" : " tickets match “") + ib_q + "”" unless ib_q == ""
  ib_actions = []
  ib_actions = [hdv_button("Clear search", "clear_search", {}, "neutral")] unless ib_q == ""
  ib_info = hd_page_of(ib_state)
  ib_rows = []
  if ib_lay["table"]
    ib_cols = hdv_cols(ib_lay["inner"])
    ib_rows = [hdv_table_head(ib_cols)].concat(ib_info["rows"].map(fn(t) { hdv_ticket_row(t, ib_cols) }))
  else
    ib_rows = ib_info["rows"].map(fn(t) { hdv_ticket_item(t, ib_lay["inner"]) })
  end
  ib_rows = [hdv_empty(ib_state)] if ib_info["total"] == 0
  ib_rows = ib_rows.concat([hdv_pager(ib_info)]) if ib_info["total"] > 0
  ib_card = column({"tw": "flex-col w-full bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden"}, ib_rows)
  ib_head = [hdv_header(ib_lay, ib_title, ib_sub, ib_actions)]
  column({"tw": "flex-col gap-6 w-full"}, ib_head.concat([hdv_tabs(ib_state), ib_card]))
end

# ---- A ticket ------------------------------------------------------------------

def hdv_message(ms_m, ms_w, ms_cust)
  ms_kind = ms_m["kind"]
  if ms_kind == "event"
    return row({"tw": "items-center gap-3 w-full"}, [
      row({"tw": "justify-center w-8 shrink-0"}, [{"k": "box", "s": {"width": 6, "height": 6, "radius": 4, "bg": "border.strong"}}]),
      hdv_text_w(ms_m["by"] + " " + ms_m["body"], "text-xs text-gray-500", ms_w - 32 - 12 - 72 - 12),
      spacer(),
      text(ms_m["at"], tw_style("text-xs text-gray-500"))
    ])
  end
  ms_card_w = ms_w - 32 - 12
  ms_body_w = ms_card_w - 2 - 32
  ms_head = [text(ms_m["by"], tw_style("text-sm font-semibold text-gray-900"))]
  ms_head = ms_head.concat([text(ms_cust["name"], tw_style("text-sm text-gray-500"))]) if ms_kind == "customer" && ms_card_w >= 480
  ms_head = ms_head.concat([badge("Shiftwise", "neutral")]) if ms_kind == "agent"
  if ms_kind == "note"
    ms_head = ms_head.concat([row({"tw": "items-center gap-1"}, [
      icon("lock", {"width": 14, "height": 14, "fg": "warning.base"}),
      text("Internal note", tw_style("text-xs font-medium text-yellow-700"))
    ])])
  end
  ms_head = ms_head.concat([spacer(), text(ms_m["at"], tw_style("text-xs text-gray-500"))])
  ms_card_tw = "flex-col gap-2 p-4 rounded-lg border border-gray-200 bg-white shadow-sm"
  ms_card_tw = "flex-col gap-2 p-4 rounded-lg border border-yellow-100 bg-yellow-50" if ms_kind == "note"
  ms_card = column({"tw": ms_card_tw, "width": ms_card_w}, [
    row({"tw": "items-center gap-2 w-full"}, ms_head),
    hdv_text_w(ms_m["body"], "text-sm text-gray-700", ms_body_w)
  ])
  row({"tw": "items-start gap-3 w-full"}, [hdv_avatar(hd_initials(ms_m["by"]), ms_kind != "customer", 32), ms_card])
end

def hdv_composer(cm_state, cm_w, cm_cust, cm_lay)
  cm_card_w = cm_w - 32 - 12
  cm_foot = []
  cm_foot = [text("Replying to " + cm_cust["contact"], tw_style("text-xs text-gray-500"))] if cm_card_w >= 460
  cm_foot = cm_foot.concat([spacer(), hdv_button("Add internal note", "note", {}, "neutral"), hdv_button("Send reply", "reply", {}, "accent")])
  cm_empty = (cm_state["draft"] ?? "") == ""
  cm_card = column({"tw": "flex-col rounded-lg border border-gray-300 bg-white shadow-sm overflow-hidden transition", "width": cm_card_w}, [])
  cm_field = textarea(cm_state["draft"], "draft", {
    "key": "hd_draft",
    "rows": 4,
    "style": {"border": 0, "bg": "none", "shadow": 0, "radius": 0, "pad": [4, 5, 4, 5], "width": cm_card_w - 2},
    "props": {"label": "Reply to " + cm_cust["contact"]},
    "on": hdv_hint_on("hd_hint_draft", "hd_frame_draft", cm_card["s"], cm_empty)
  })
  cm_well = cm_empty ? stack({"width": cm_card_w - 2}, [hdv_hint("hd_hint_draft", "Write a reply to " + hd_first_name(cm_cust["contact"]) + ", or a note only your team will see", [4, 5, 4, 5]), cm_field]) : cm_field
  cm_card["c"] = [cm_well, row({"tw": "items-center gap-3 w-full px-3 py-3 border-t border-gray-200 bg-gray-50"}, cm_foot)]
  cm_card["key"] = "hd_frame_draft"
  row({"tw": "items-start gap-3 w-full"}, [hdv_avatar("MC", true, 32), cm_card])
end

def hdv_dl(dl_label, dl_value)
  row({"tw": "items-center gap-3 w-full py-2"}, [
    text(dl_label, tw_style("text-sm text-gray-500 grow")),
    dl_value
  ])
end

def hdv_customer_card(cc_state, cc_t, cc_w)
  cc_c = hd_customer(cc_t["cust"])
  cc_in = cc_w - 2 - 40
  cc_recent = hd_sorted(cc_state["tickets"].filter(fn(t) { t["cust"] == cc_c["id"] && t["id"] != cc_t["id"] }))
  cc_recent = cc_recent.slice(0, cc_recent.length() > 3 ? 3 : cc_recent.length())
  cc_list = cc_recent.map(fn(t) {
    cc_line = column({"tw": "flex-col gap-1 w-full px-2 py-2 rounded-md hover:bg-gray-50 cursor-pointer transition"}, [
      hdv_text_w(t["subject"], "text-sm font-medium text-gray-900 truncate", cc_in - 16),
      row({"tw": "items-center gap-2"}, [text("#" + str(t["id"]), tw_style("text-xs text-gray-500")), hdv_status(t["status"])])
    ])
    keyed("rt:" + str(t["id"]), hdv_clickable(cc_line, "open", {"id": t["id"], "label": t["subject"]}))
  })
  cc_list = [text("No other tickets from " + cc_c["name"] + ".", tw_style("text-sm text-gray-500"))] if cc_list.length() == 0
  card({"gap": 4, "width": cc_w}, [
    row({"tw": "items-center gap-3 w-full"}, [
      hdv_avatar(hd_initials(cc_c["name"]), false, 40),
      column({"tw": "flex-col gap-0.5 min-w-0"}, [
        hdv_text_w(cc_c["name"], "text-sm font-semibold text-gray-900 truncate", cc_in - 52),
        hdv_text_w(cc_c["contact"], "text-sm text-gray-500 truncate", cc_in - 52)
      ])
    ]),
    hdv_text_w(cc_c["email"], "text-sm text-indigo-600 truncate", cc_in),
    column({"tw": "flex-col w-full border-t border-gray-200 pt-2"}, [
      hdv_dl("Plan", hdv_plan(cc_c["plan"])),
      hdv_dl("Monthly revenue", text(hd_money(cc_c["mrr"]), tw_style("text-sm font-medium text-gray-900"))),
      hdv_dl("Customer since", text(cc_c["since"], tw_style("text-sm text-gray-900"))),
      hdv_dl("Open tickets", text(str(hd_open_for(cc_state, cc_c["id"])), tw_style("text-sm text-gray-900")))
    ]),
    column({"tw": "flex-col gap-1 w-full border-t border-gray-200 pt-4"}, [text("Recent tickets", tw_style("text-sm font-semibold text-gray-900"))].concat(cc_list))
  ])
end

def hdv_property(pp_state, pp_label, pp_which, pp_options, pp_value, pp_event, pp_px)
  column({"tw": "flex-col gap-2 w-full"}, [
    field_label(pp_label),
    select_sized(pp_options, pp_value, pp_state["menu"] == pp_which, "menu", pp_event, pp_px, false, {"key": "hdsel:" + pp_which, "props": {"which": pp_which}})
  ])
end

def hdv_properties(pr_state, pr_t, pr_w)
  pr_px = pr_w - 2 - 40
  card({"gap": 5, "width": pr_w}, [
    text("Properties", tw_style("text-base font-semibold text-gray-900")),
    hdv_property(pr_state, "Status", "status", hd_statuses(), pr_t["status"], "set_status", pr_px),
    hdv_property(pr_state, "Priority", "prio", hd_priorities(), pr_t["prio"], "set_prio", pr_px),
    hdv_property(pr_state, "Assignee", "agent", ["Unassigned"].concat(hd_agents()), pr_t["agent"] == "" ? "Unassigned" : pr_t["agent"], "set_agent", pr_px)
  ])
end

def hdv_detail(dt_state, dt_lay)
  dt_t = hd_find(dt_state["tickets"], dt_state["sel"])
  return hdv_inbox(hd_set(dt_state, "sel", 0), dt_lay) if dt_t.nil?

  dt_c = hd_customer(dt_t["cust"])
  dt_back = dt_state["section"] == "mine" ? "Assigned to me" : "Inbox"
  dt_crumbs = row({"tw": "items-center gap-2"}, [
    hdv_clickable(row({"tw": "items-center gap-1 cursor-pointer"}, [
      icon("chevron_left", {"width": 16, "height": 16, "fg": "text.muted"}),
      text(dt_back, tw_style("text-sm font-medium text-gray-500"))
    ]), "back", {"label": "Back to " + dt_back}),
    text("/", tw_style("text-sm text-gray-400")),
    text("#" + str(dt_t["id"]), tw_style("text-sm font-medium text-gray-500"))
  ])
  dt_meta = row({"tw": "items-center gap-4 flex-wrap"}, [
    hdv_status(dt_t["status"]),
    hdv_prio(dt_t["prio"]),
    text("Opened by " + dt_c["contact"] + " at " + dt_c["name"], tw_style("text-sm text-gray-500"))
  ])
  dt_head = column({"tw": "flex-col gap-3 w-full"}, [dt_crumbs, hdv_text_w(dt_t["subject"], "text-2xl font-semibold text-gray-900", dt_lay["inner"]), dt_meta])
  dt_thread_w = dt_lay["split"] == true ? dt_lay["inner"] - HDV_PANEL_PX - 32 : dt_lay["inner"]
  dt_panel_w = dt_lay["split"] == true ? HDV_PANEL_PX : dt_lay["inner"]
  dt_msgs = hd_thread(dt_state, dt_t["id"]).map(fn(m) { hdv_message(m, dt_thread_w, dt_c) })
  dt_thread = column({"tw": "flex-col gap-6", "width": dt_thread_w}, dt_msgs.concat([hdv_composer(dt_state, dt_thread_w, dt_c, dt_lay)]))
  dt_panel = column({"tw": "flex-col gap-6", "width": dt_panel_w}, [hdv_properties(dt_state, dt_t, dt_panel_w), hdv_customer_card(dt_state, dt_t, dt_panel_w)])
  # Side by side, the properties sit beside the thread; stacked, the
  # conversation comes first, because it is what the ticket is.
  dt_body = dt_lay["split"] == true ? row({"tw": "items-start gap-8 w-full"}, [dt_thread, dt_panel]) : column({"tw": "flex-col gap-8 w-full"}, [dt_thread, dt_panel])
  column({"tw": "flex-col gap-8 w-full"}, [dt_head, dt_body])
end

# ---- Customers -----------------------------------------------------------------

def hdv_customer_row(cw_c, cw_state, cw_inner)
  cw_fixed = 96 + 128 + 104 + 120 + 16
  cw_name_w = cw_inner - 2 - 32 - cw_fixed - 5 * 16 - 32 - 12
  cw_line = row({"tw": "items-center gap-4 w-full px-4 py-4 border-b border-gray-200 bg-white hover:bg-gray-50 cursor-pointer transition"}, [
    row({"tw": "items-center gap-3 shrink-0"}, [
      hdv_avatar(hd_initials(cw_c["name"]), false, 32),
      column({"tw": "flex-col gap-0.5"}, [
        hdv_text_w(cw_c["name"], "text-sm font-medium text-gray-900 truncate", cw_name_w),
        hdv_text_w(cw_c["contact"] + ", " + cw_c["email"], "text-xs text-gray-500 truncate", cw_name_w)
      ])
    ]),
    row({"width": 96, "shrink": 0}, [hdv_plan(cw_c["plan"])]),
    hdv_text_w(hd_money(cw_c["mrr"]), "text-sm text-gray-900 text-right", 128),
    hdv_text_w(str(hd_open_for(cw_state, cw_c["id"])), "text-sm text-gray-700 text-right", 104),
    hdv_text_w(cw_c["since"], "text-sm text-gray-500", 120),
    icon("chevron_right", {"width": 16, "height": 16, "fg": "text.disabled", "shrink": 0})
  ])
  keyed("cu:" + cw_c["id"], hdv_clickable(cw_line, "cust_open", {"id": cw_c["id"], "label": "Tickets from " + cw_c["name"]}))
end

def hdv_customer_head(ch_inner)
  ch_name_w = ch_inner - 2 - 32 - (96 + 128 + 104 + 120 + 16) - 5 * 16
  row({"tw": "items-center gap-4 w-full px-4 py-3 border-b border-gray-300 bg-white"}, [
    hdv_text_w("Company", "text-sm font-semibold text-gray-900", ch_name_w),
    hdv_text_w("Plan", "text-sm font-semibold text-gray-900", 96),
    hdv_text_w("Monthly revenue", "text-sm font-semibold text-gray-900 text-right", 128),
    hdv_text_w("Open tickets", "text-sm font-semibold text-gray-900 text-right", 104),
    hdv_text_w("Customer since", "text-sm font-semibold text-gray-900", 120),
    {"k": "box", "s": {"width": 16, "shrink": 0}}
  ])
end

def hdv_customer_item(ci_c, ci_w)
  ci_line = row({"tw": "items-center gap-3 w-full px-4 py-4 border-b border-gray-200 bg-white hover:bg-gray-50 cursor-pointer transition"}, [
    hdv_avatar(hd_initials(ci_c["name"]), false, 32),
    column({"tw": "flex-col gap-0.5"}, [
      hdv_text_w(ci_c["name"], "text-sm font-medium text-gray-900 truncate", ci_w - 2 - 32 - 44 - 80 - 12),
      hdv_text_w(hd_money(ci_c["mrr"]) + " a month, since " + ci_c["since"], "text-xs text-gray-500 truncate", ci_w - 2 - 32 - 44 - 80 - 12)
    ]),
    spacer(),
    hdv_plan(ci_c["plan"])
  ])
  keyed("cu:" + ci_c["id"], hdv_clickable(ci_line, "cust_open", {"id": ci_c["id"], "label": "Tickets from " + ci_c["name"]}))
end

def hdv_customers(cs_state, cs_lay)
  cs_all = hd_customers()
  cs_mrr = 0
  for cs_one in cs_all
    cs_mrr = cs_mrr + cs_one["mrr"]
  end
  cs_found = hd_customer_rows(cs_state)
  cs_search_px = cs_lay["roomy"] == true ? 280 : cs_lay["inner"]
  cs_head = hdv_header(cs_lay, "Customers", str(cs_all.length()) + " accounts, " + hd_money(cs_mrr) + " in monthly revenue", [hdv_search_box(cs_state["cq"], "cust_search", cs_search_px, "Search customers")])
  cs_rows = cs_lay["table"] == true ? [hdv_customer_head(cs_lay["inner"])].concat(cs_found.map(fn(c) { hdv_customer_row(c, cs_state, cs_lay["inner"]) })) : cs_found.map(fn(c) { hdv_customer_item(c, cs_lay["inner"]) })
  if cs_found.length() == 0
    cs_rows = [column({"tw": "flex-col items-center gap-3 w-full py-12 px-6 bg-white"}, [
      text("No customers match “" + cs_state["cq"] + "”", tw_style("text-sm font-semibold text-gray-900")),
      text("Search by company, contact, email or plan.", tw_style("text-sm text-gray-500")),
      hdv_button("Clear search", "cust_clear", {}, "neutral")
    ])]
  end
  cs_card = column({"tw": "flex-col w-full bg-white rounded-lg border border-gray-200 shadow-sm overflow-hidden"}, cs_rows)
  column({"tw": "flex-col gap-6 w-full"}, [cs_head, cs_card])
end

# ---- Reports -------------------------------------------------------------------

def hdv_stat(sa_label, sa_value, sa_delta, sa_up, sa_good, sa_note, sa_w)
  sa_kids = [text(sa_value, tw_style("text-3xl font-semibold text-gray-900"))]
  unless sa_delta == ""
    sa_ink = sa_good == true ? "success.base" : "danger.base"
    sa_kids = sa_kids.concat([row({"tw": "items-center gap-1 pb-1"}, [
      icon(sa_up == true ? "arrow_up" : "arrow_down", {"width": 14, "height": 14, "fg": sa_ink}),
      text(sa_delta, {"size": 1, "weight": "semibold", "fg": sa_ink})
    ])])
  end
  card({"gap": 2, "width": sa_w}, [
    text(sa_label, tw_style("text-sm font-medium text-gray-500")),
    row({"tw": "items-end gap-3"}, sa_kids),
    text(sa_note, tw_style("text-xs text-gray-500"))
  ])
end

def hdv_grid(gd_items, gd_cols, gd_gap)
  gd_rows = []
  gd_i = 0
  while gd_i < gd_items.length()
    gd_end = gd_i + gd_cols
    gd_end = gd_items.length() if gd_end > gd_items.length()
    gd_rows = gd_rows.concat([row({"gap": gd_gap, "width": "100%", "align": "stretch"}, gd_items.slice(gd_i, gd_end))])
    gd_i = gd_end
  end
  column({"gap": gd_gap, "width": "100%"}, gd_rows)
end

def hdv_card_title(ct_title, ct_sub)
  column({"tw": "flex-col gap-1"}, [text(ct_title, tw_style("text-base font-semibold text-gray-900")), text(ct_sub, tw_style("text-sm text-gray-500"))])
end

def hdv_reports(rp_state, rp_lay)
  rp_in = rp_lay["inner"]
  rp_cols = 1
  rp_cols = 2 if rp_in >= 520
  rp_cols = 4 if rp_in >= 900
  rp_card_w = (rp_in - (rp_cols - 1) * 24) / rp_cols
  rp_open = hd_count(rp_state["tickets"], "Open")
  rp_urgent = rp_state["tickets"].filter(fn(t) { t["status"] == "Open" && t["prio"] == "Urgent" }).length()
  rp_solved = 31 + rp_state["solved_here"]
  rp_stats = [
    hdv_stat("First response time", "1h 12m", "18%", false, true, "Median, down from 1h 28m", rp_card_w),
    hdv_stat("Customer satisfaction", "94%", "2 pts", true, true, "212 ratings in the last 30 days", rp_card_w),
    hdv_stat("Open tickets", str(rp_open), "", true, true, str(rp_urgent) + " urgent, " + str(rp_state["tickets"].filter(fn(t) { t["status"] == "Open" && t["agent"] == "" }).length()) + " unassigned", rp_card_w),
    hdv_stat("Solved this week", str(rp_solved), str(rp_solved - 27), true, true, "27 last week", rp_card_w)
  ]
  rp_two = rp_in >= 900
  rp_chart_card = rp_two == true ? (rp_in - 24) / 2 : rp_in
  rp_chart_w = rp_chart_card - 2 - 40
  rp_weeks = ["Aug 4", "Aug 11", "Aug 18", "Aug 25", "Sep 1", "Sep 8", "Sep 15", "Sep 22"]
  # Eight dates do not fit under a phone-width chart; every other one does.
  rp_weeks = ["Aug 4", "", "Aug 18", "", "Sep 1", "", "Sep 15", ""] if rp_chart_w < 520
  rp_sets = [ [31, 36, 29, 44, 38, 47, 41, 45], [27, 33, 31, 38, 40, 42, 44, 42] ]
  rp_flow = card({"gap": 4, "width": rp_chart_card}, [
    hdv_card_title("Created and solved", "Tickets per week"),
    chart_multi_line("hd_flow", rp_sets, ["Created", "Solved"], rp_chart_w, 220, rp_weeks)
  ])
  rp_topics = [
    {"label": "Billing", "value": 58},
    {"label": "Mobile app", "value": 41},
    {"label": "Timesheets", "value": 36},
    {"label": "Exports", "value": 29},
    {"label": "Notifications", "value": 22},
    {"label": "SSO", "value": 17},
    {"label": "API", "value": 12}
  ]
  rp_mix = card({"gap": 4, "width": rp_chart_card}, [
    hdv_card_title("Tickets by topic", "Last 8 weeks"),
    chart_ranked_bar("hd_topics", rp_topics, rp_chart_w, 238)
  ])
  rp_charts = rp_two == true ? row({"tw": "items-start gap-6 w-full"}, [rp_flow, rp_mix]) : column({"tw": "flex-col gap-6 w-full"}, [rp_flow, rp_mix])
  column({"tw": "flex-col gap-6 w-full"}, [
    hdv_header(rp_lay, "Reports", "Last 8 weeks, every agent", []),
    hdv_grid(rp_stats, rp_cols, rp_cols == 1 ? 5 : 7),
    rp_charts
  ])
end

# ---- Settings ------------------------------------------------------------------

def hdv_section(se_lay, se_title, se_desc, se_body)
  se_side = se_lay["split"] == true ? 280 : se_lay["inner"]
  se_card_w = se_lay["split"] == true ? se_lay["inner"] - 280 - 32 : se_lay["inner"]
  se_words = column({"tw": "flex-col gap-1 shrink-0", "width": se_side}, [
    text(se_title, tw_style("text-base font-semibold text-gray-900")),
    hdv_text_w(se_desc, "text-sm text-gray-500", se_side)
  ])
  se_card = card({"gap": 6, "width": se_card_w}, se_body)
  return row({"tw": "items-start gap-8 w-full"}, [se_words, se_card]) if se_lay["split"]

  column({"tw": "flex-col gap-4 w-full"}, [se_words, se_card])
end

def hdv_notify_row(nr_state, nr_id, nr_label, nr_desc, nr_w)
  nr_on = nr_state["set_notify"].includes?(nr_id)
  row({"tw": "items-center gap-4 w-full"}, [
    column({"tw": "flex-col gap-0.5"}, [
      text(nr_label, tw_style("text-sm font-medium text-gray-900")),
      hdv_text_w(nr_desc, "text-sm text-gray-500", nr_w - 80)
    ]),
    spacer(),
    switch("", nr_on == true, "notify", {"id": nr_id}, {"name": nr_label, "key": "hdsw:" + nr_id + (nr_on == true ? ":on" : ":off")})
  ])
end

def hdv_hours_select(hs_state, hs_label, hs_which, hs_options, hs_value, hs_event, hs_px)
  column({"tw": "flex-col gap-2"}, [
    field_label(hs_label),
    select_sized(hs_options, hs_value, hs_state["menu"] == hs_which, "menu", hs_event, hs_px, false, {"key": "hdsel:" + hs_which, "props": {"which": hs_which}})
  ])
end

def hdv_settings(sg_state, sg_lay)
  sg_card_w = sg_lay["split"] == true ? sg_lay["inner"] - 280 - 32 : sg_lay["inner"]
  sg_in = sg_card_w - 2 - 40
  sg_sel_w = sg_in >= 560 ? 160 : sg_in
  sg_hours = [
    hdv_hours_select(sg_state, "Start", "start", ["07:00", "08:00", "08:30", "09:00", "09:30", "10:00"], sg_state["set_start"], "set_start", sg_sel_w),
    hdv_hours_select(sg_state, "End", "end", ["16:00", "16:30", "17:00", "17:30", "18:00", "19:00"], sg_state["set_end"], "set_end", sg_sel_w),
    hdv_hours_select(sg_state, "Time zone", "tz", ["Europe/London", "Europe/Paris", "Europe/Oslo", "America/New_York"], sg_state["set_tz"], "set_tz", sg_in >= 560 ? 200 : sg_in)
  ]
  sg_hours_box = sg_in >= 560 ? row({"tw": "items-start gap-4 w-full"}, sg_hours) : column({"tw": "flex-col gap-4 w-full"}, sg_hours)
  # Tailwind UI's `max-w-md`: a name does not need the width of the card.
  sg_field_px = sg_in > 448 ? 448 : sg_in
  sg_field_w = {"style": {"width": sg_field_px}}
  column({"tw": "flex-col gap-10 w-full"}, [
    hdv_header(sg_lay, "Settings", "Signed in as " + sg_state["set_email"], []),
    hdv_section(sg_lay, "Profile", "Customers see your name and signature on every reply you send.", [
      text_field("Full name", sg_state["set_name"], "set_name", sg_field_w),
      text_field("Email address", sg_state["set_email"], "set_email", sg_field_w),
      textarea_field("Reply signature", sg_state["set_sig"], "set_sig", {"rows": 2, "style": {"width": sg_field_px}})
    ]),
    hdv_section(sg_lay, "Notifications", "Email and desktop alerts. Urgent tickets always notify whoever is on shift.", [
      hdv_notify_row(sg_state, "assigned", "Assigned to me", "A ticket is assigned to you by a teammate or a rule.", sg_in),
      hdv_notify_row(sg_state, "replies", "Customer replies", "A customer answers one of your tickets.", sg_in),
      hdv_notify_row(sg_state, "breach", "Response target at risk", "A ticket has 30 minutes left before its first-response target.", sg_in),
      hdv_notify_row(sg_state, "digest", "Morning summary", "Your open tickets, by priority, at 08:00.", sg_in)
    ]),
    hdv_section(sg_lay, "Working hours", "Tickets are only assigned to you inside these hours.", [sg_hours_box]),
    row({"tw": "items-center justify-end gap-4 w-full"}, [
      text(sg_state["saved"], tw_style("text-sm text-gray-500")),
      hdv_button("Save changes", "save", {}, "accent")
    ])
  ])
end

# ---- The page ------------------------------------------------------------------

def hdv_toast(to_state)
  to_note = toast(to_state["toast"], to_state["toast"] == "Write a reply first" ? "warning" : "success")
  to_note["p"] = (to_note["p"] ?? {}).merge({"wake": 3200})
  to_note["on"] = {"click": "toast_done", "wake": "toast_done"}
  to_note["s"]["cursor"] = "pointer"
  {
    "k": "overlay",
    "s": {"position": "absolute", "align": "start", "justify": "end", "pad": 6, "width": "100%"},
    "c": [to_note]
  }
end

def hdv_page(pa_state, pa_lay)
  pa_section = pa_state["section"]
  return hdv_customers(pa_state, pa_lay) if pa_section == "customers"
  return hdv_reports(pa_state, pa_lay) if pa_section == "reports"
  return hdv_settings(pa_state, pa_lay) if pa_section == "settings"
  return hdv_detail(pa_state, pa_lay) if pa_state["sel"] != 0

  hdv_inbox(pa_state, pa_lay)
end

def helpdesk_view(raw_state)
  hv_state = hd_state(raw_state ?? {})
  hv_lay = hdv_lay(hv_state)
  hv_page_key = hv_state["section"] + ":" + str(hv_state["sel"])
  hv_body = keyed("hdpage:" + hv_page_key, column({"width": hv_lay["inner"], "gap": 0}, [hdv_page(hv_state, hv_lay)]))
  hv_scroll = scroll({"grow": 1, "min_height": 0, "width": "100%"}, [
    column({"width": "100%", "align": "center", "pad": [hv_lay["roomy"] == true ? 8 : 6, 0, 10, 0]}, [hv_body])
  ])
  hv_main = column({"grow": 1, "min_height": 0, "gap": 0, "bg": "surface.base"}, [hdv_topbar(hv_state, hv_lay), hv_scroll])
  hv_shell = hv_lay["rail"] == true ? row({"gap": 0, "width": "100%", "height": "100%"}, [hdv_sidebar(hv_state), hv_main]) : column({"gap": 0, "width": "100%", "height": "100%"}, [hv_main])
  hv_layers = [hv_shell]
  if hv_state["nav_open"] == true && !hv_lay["rail"]
    hv_drawer = drawer([hdv_logo(), hdv_nav(hv_state, "drawer")], {"label": "Navigation", "on_close": "nav_toggle", "key": "hd_drawer"})
    # Tailwind UI's mobile sidebar is `max-w-xs`, and the sheet's 384 would
    # leave a phone a sliver of scrim to tap out on.
    hv_drawer_px = hv_lay["w"] - 64
    hv_drawer_px = 320 if hv_drawer_px > 320
    hv_drawer["c"][0]["s"]["width"] = hv_drawer_px
    hv_drawer["c"][0]["s"]["pad"] = 5
    hv_layers = hv_layers.concat([hv_drawer])
  end
  hv_layers = hv_layers.concat([hdv_toast(hv_state)]) unless hv_state["toast"] == ""
  stack({"gap": 0, "width": "100%", "height": "100%", "bg": "surface.base"}, hv_layers)
end
