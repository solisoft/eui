# Campaign Performance: one dense, sortable table of ad campaigns, with the
# figures for what is in view across the top of it — the screen a marketing
# dashboard is built around, drawn from theme roles and the catalogue.
#
# The data and every state change are `app/services/campaigns_book.sl`, so
# they are tested without a server (`tests/campaigns_spec.sl`). This file is
# the view and nothing else, plus the two lines `router_eui` calls.
#
# Three kinds of cell carry this table, and each is the cheapest thing that
# says it:
#
# - **two lines of text**, a figure over what it means — "$66.1K" over "85%
#   of $77.5K" — so a row answers its own next question;
# - **a sparkline**, one `canvas` of three paths: an area in `info.subtle`, the
#   line over it in `series.1`, a dot on today. No axis, because sixteen days
#   in a 112 px cell is a shape, not a reading;
# - **a meter**, one `canvas` of three rectangles: a track, a fill, and for
#   ROAS a tick at the target, so a bar that crosses the tick has earned its
#   colour and one that stops short is grey.
#
# Every width a text truncates against is worked out in `cpv_lay` from the
# viewport. Columns leave in a fixed order as the window narrows, and below
# that the table becomes a list of cards.
#
# Locals carry a prefix per function: Soli's scope is flat across calls.

def campaigns(event_data)
  cp_reduce(event_data["state"] ?? {}, event_data["event"], event_data["params"] ?? {})
end

# ---- Measures ------------------------------------------------------------------

# `pad` and `gap` take a step of the spacing scale, widths take pixels, and
# the arithmetic below needs both: 5 is 16 px, 4 is 12 px.
CPV_PAD_X = 5
CPV_PAD_X_PX = 16
CPV_GAP = 4
CPV_GAP_PX = 12
CPV_CHECK = 24
CPV_ACT = 32
CPV_NAME_MIN = 248

def cpv_columns
  [
    {"id": "status", "label": "Status", "w": 84},
    {"id": "spend", "label": "Spend", "w": 112},
    {"id": "conv", "label": "Conversions", "w": 108},
    {"id": "trend", "label": "Trend", "w": 112},
    {"id": "cpa", "label": "CPA", "w": 92},
    {"id": "roas", "label": "ROAS", "w": 128},
    {"id": "nb", "label": "New Buyers", "w": 124}
  ]
end

# The order columns leave in when the window cannot hold them: the shape
# first, then the ratios, and the money last.
def cpv_drop_order
  ["trend", "nb", "roas", "cpa", "conv"]
end

def cpv_sort_key(sk_col)
  return "name" if sk_col == "name"
  return "conv" if sk_col == "conv"
  return "trend_d" if sk_col == "trend"
  return "cpa_c" if sk_col == "cpa"
  return "roas_t" if sk_col == "roas"

  sk_col
end

def cpv_lay(ly_state)
  ly_w = (ly_state["viewport"] ?? {})["width"] ?? 1280
  ly_pad = 16
  ly_pad = 24 if ly_w >= 640
  ly_pad = 32 if ly_w >= 1024
  ly_step = ly_pad == 16 ? 5 : (ly_pad == 24 ? 7 : 8)
  ly_card = ly_w - 2 * ly_pad
  ly_card = 1440 if ly_card > 1440
  ly_inner = ly_card - 2
  ly_hidden = ly_state["hidden"]
  ly_cols = cpv_columns().filter(fn(c) { !ly_hidden.includes?(c["id"]) })
  ly_drop = cpv_drop_order()
  ly_i = 0
  ly_name = cpv_name_w(ly_inner, ly_cols)
  while ly_name < CPV_NAME_MIN && ly_i < ly_drop.length()
    ly_gone = ly_drop[ly_i]
    ly_cols = ly_cols.filter(fn(c) { c["id"] != ly_gone })
    ly_name = cpv_name_w(ly_inner, ly_cols)
    ly_i = ly_i + 1
  end
  {
    "w": ly_w,
    "pad": ly_step,
    "card": ly_card,
    "inner": ly_inner,
    "cols": ly_cols,
    "name": ly_name,
    "table": ly_name >= CPV_NAME_MIN,
    "kpi_per_row": ly_card >= 1100 ? 6 : (ly_card >= 600 ? 3 : 2),
    "search": ly_card >= 760 ? 320 : (ly_card >= 520 ? 240 : ly_card - 2 - 2 * CPV_PAD_X_PX)
  }
end

# What is left for the campaign's name once the fixed columns, the checkbox,
# the actions and the gaps between them are paid for.
def cpv_name_w(nw_inner, nw_cols)
  nw_fixed = CPV_CHECK + CPV_ACT
  for nw_c in nw_cols
    nw_fixed = nw_fixed + nw_c["w"]
  end
  nw_gaps = (nw_cols.length() + 2) * CPV_GAP_PX
  nw_inner - 2 * CPV_PAD_X_PX - nw_fixed - nw_gaps
end

# ---- Small parts ---------------------------------------------------------------

def cpv_text_w(tx_content, tx_style, tx_px)
  tx_style["width"] = tx_px
  tx_style["clamp"] = 1
  text(tx_content, tx_style)
end

# The channel's mark: its letter in its series colour, on the page grey.
def cpv_channel_tile(ct_ch, ct_px)
  {
    "k": "box",
    "s": {"display": "row", "width": ct_px, "height": ct_px, "radius": 2, "bg": "surface.base", "border": 1, "border_color": "border.subtle", "justify": "center", "align": "center", "shrink": 0},
    "p": {"label": cp_channels()[ct_ch]},
    "c": [text(cp_channels()[ct_ch].substring(0, 1), {"size": 2, "weight": "bold", "fg": "series." + str(ct_ch + 1)})]
  }
end

# The market as a code in a small outlined chip, where a flag would be: a flag
# is a picture of a country, and a code is what a person filters by.
def cpv_market(mk_code)
  {
    "k": "box",
    "s": {"display": "row", "align": "center", "pad": [0, 2, 0, 2], "radius": 1, "border": 1, "border_color": "border.default", "shrink": 0},
    "c": [text(mk_code, {"size": 0, "weight": "semibold", "fg": "text.muted"})]
  }
end

def cpv_status(ss_status)
  return badge(ss_status, "success") if ss_status == "Live"
  return badge(ss_status, "warning") if ss_status == "Limited"

  badge(ss_status, "neutral")
end

# A change coloured by whether it is good news, which depends on the figure:
# more conversions is good, a dearer conversion is not.
def cpv_delta(dl_n, dl_up_is_good, dl_size = 0)
  dl_good = dl_up_is_good ? dl_n >= 0 : dl_n <= 0
  text(cp_delta(dl_n), {"size": dl_size, "weight": "medium", "fg": dl_good ? "success.base" : "danger.base"})
end

def cpv_figure(fg_main, fg_sub, fg_w)
  column({"gap": 1, "width": fg_w, "shrink": 0}, [
    cpv_text_w(fg_main, {"size": 1, "weight": "medium", "fg": "text.default"}, fg_w),
    fg_sub
  ])
end

def cpv_sparkline(sp_vals, sp_w, sp_h)
  sp_top = 0
  sp_low = 1000
  for sp_v in sp_vals
    sp_top = sp_v if sp_v > sp_top
    sp_low = sp_v if sp_v < sp_low
  end
  sp_span = sp_top - sp_low
  sp_span = 1 if sp_span < 1
  sp_n = sp_vals.length()
  sp_pts = []
  for sp_i in range(0, sp_n)
    sp_x = 3 + sp_i * (sp_w - 6) / (sp_n - 1)
    sp_y = 3 + (sp_h - 6) * (sp_top - sp_vals[sp_i]) / sp_span
    sp_pts = sp_pts.concat([sp_x, sp_y])
  end
  sp_last_x = sp_pts[sp_pts.length() - 2]
  sp_last_y = sp_pts[sp_pts.length() - 1]
  canvas(sp_w, sp_h, [
    [2, "info.subtle", sp_h].concat(sp_pts),
    [0, "series.1", 2].concat(sp_pts),
    [3, "series.1", sp_last_x, sp_last_y, 3]
  ])
end

# A track and a fill, and a tick at `tick` (a fraction in hundredths) when
# there is one. The fill is the accent's series colour when `lit` and grey when
# not, so the eye finds the rows that are doing their job.
def cpv_meter(mt_pct, mt_tick, mt_lit, mt_w)
  mt_h = 14
  mt_pct = 100 if mt_pct > 100
  mt_fill = (mt_w * mt_pct) / 100
  mt_fill = 3 if mt_fill < 3
  mt_paths = [
    [1, "surface.sunken", 0, 4, mt_w, 6, 3],
    [1, mt_lit ? "series.1" : "border.strong", 0, 4, mt_fill, 6, 3]
  ]
  mt_paths = mt_paths.concat([[1, "text.default", (mt_w * mt_tick) / 100 - 1, 0, 2, mt_h, 1]]) if mt_tick > 0
  canvas(mt_w, mt_h, mt_paths)
end

def cpv_button(bt_label, bt_icon, bt_event, bt_props, bt_tone, bt_key, bt_open = false)
  bt_kids = []
  bt_kids = [icon(bt_icon, {"width": 16, "height": 16, "shrink": 0})] unless bt_icon == ""
  bt_kids = bt_kids.concat([text(bt_label, {"weight": "semibold", "size": 1})]) unless bt_label == ""
  control({
    "key": bt_key,
    "tone": bt_tone,
    "size": "md",
    "shape": {"gap": 2, "min_height": 36},
    "on": {"click": bt_event},
    "props": bt_props,
    "a11y": {"role": "button", "label": bt_label == "" ? bt_icon : bt_label, "expanded": bt_open},
    "c": bt_kids
  })
end

# A list of choices with the current one ticked, hung under its button.
def cpv_choices(ch_state, ch_which, ch_anchor, ch_options, ch_value, ch_event, ch_w)
  ch_open = ch_state["menu"] == ch_which
  dropdown(ch_anchor, ch_options.map(fn(o) { select_option(o, o == ch_value, ch_event, ch_w, {}) }), ch_open, DROPDOWN_MAX_PX, "menu_close")
end

# ---- The header and the figures ------------------------------------------------

def cpv_header(hd_state, hd_lay, hd_tot)
  hd_bits = [str(hd_tot["count"]) + (hd_tot["count"] == 1 ? " campaign" : " campaigns"), str(hd_tot["live"]) + " live in view", "Sep 6 to Oct 5"]
  hd_actions = row({"gap": 3, "align": "center", "shrink": 0}, [
    cpv_button(hd_lay["card"] >= 560 ? "Export" : "", "arrow_down", "export", {}, "neutral", "cp_export"),
    cpv_button(hd_lay["card"] >= 560 ? "New Campaign" : "", "plus", "new", {}, "accent", "cp_new")
  ])
  row({"gap": 4, "align": "center", "width": "100%", "pad": [5, 5, 5, 5]}, [
    column({"gap": 1, "grow": 1, "min_width": 0}, [
      text("Campaign Performance", {"size": 2, "weight": "semibold", "fg": "text.default"}),
      text(hd_bits.join("  ·  "), {"size": 1, "fg": "text.muted", "clamp": 1})
    ]),
    hd_actions
  ])
end

def cpv_kpi(kp_label, kp_value, kp_sub, kp_last)
  {
    "k": "box",
    "s": {
      "display": "column", "gap": 1, "grow": 1, "basis": 0, "min_width": 0,
      "pad": [5, 5, 5, 5],
      "border": [0, kp_last ? 0 : 1, 0, 0], "border_color": "border.subtle"
    },
    "c": [
      text(kp_label, {"size": 1, "fg": "text.muted", "clamp": 1}),
      text(kp_value, {"size": 5, "weight": "semibold", "fg": "text.default"}),
      kp_sub
    ]
  }
end

def cpv_kpis(kz_tot, kz_per_row)
  kz_sub = fn(s) { text(s, {"size": 1, "fg": "text.muted", "clamp": 1}) }
  kz_cells = [
    ["Spend", cp_money(kz_tot["spend"]), kz_sub(str(kz_tot["paced"]) + "% of " + cp_money(kz_tot["budget"]) + " budget")],
    ["Impressions", cp_count(kz_tot["imps"]), kz_sub(cp_tenths(kz_tot["ctr_t"]) + "% CTR")],
    ["Conversions", cp_count(kz_tot["conv"]), cpv_delta(kz_tot["conv_d"], true, 1)],
    ["Blended CPA", cp_cents(kz_tot["cpa_c"]), cpv_delta(kz_tot["cpa_d"], false, 1)],
    ["Blended ROAS", cp_roas(kz_tot["roas_t"]), kz_sub("Target " + cp_roas(CP_TARGET_ROAS))],
    ["Below Target", str(kz_tot["below"]), kz_sub("of " + str(kz_tot["count"]) + " campaigns")]
  ]
  kz_rows = []
  kz_i = 0
  while kz_i < kz_cells.length()
    kz_end = kz_i + kz_per_row
    kz_end = kz_cells.length() if kz_end > kz_cells.length()
    kz_line = range(kz_i, kz_end).map(fn(j) { cpv_kpi(kz_cells[j][0], kz_cells[j][1], kz_cells[j][2], j == kz_end - 1) })
    kz_rows = kz_rows.concat([row({"gap": 0, "width": "100%", "align": "stretch", "border": [1, 0, 0, 0], "border_color": "border.subtle"}, kz_line)])
    kz_i = kz_end
  end
  column({"gap": 0, "width": "100%"}, kz_rows)
end

# ---- The toolbar -----------------------------------------------------------------

def cpv_search(sx_value, sx_px)
  sx_field = input(sx_value, "search", {
    "key": "cp_q",
    "placeholder": "Search campaign, ID, market…",
    "style": {"bg": "none", "pad": [3, 4, 3, 9], "width": sx_px},
    "props": {"label": "Search campaigns"}
  })
  sx_glass = icon("search", {"width": 16, "height": 16, "fg": "text.disabled", "position": "absolute", "self": "center", "margin": [0, 0, 0, 4]})
  stack({"width": sx_px, "justify": "start", "bg": "surface.raised", "radius": 2, "shadow": 1, "shrink": 0}, [sx_glass, sx_field])
end

def cpv_columns_menu(cm_state)
  cm_open = cm_state["menu"] == "columns"
  cm_anchor = cpv_button("Columns", "grid", "menu", {"which": "columns"}, "neutral", "cp_columns", cm_open)
  cm_hidden = cm_state["hidden"]
  cm_items = cpv_columns().filter(fn(c) { cp_optional_columns().includes?(c["id"]) }).map(fn(c) {
    checkbox(c["label"], !cm_hidden.includes?(c["id"]), "column", {"col": c["id"]}, {"key": "cp_col:" + c["id"]})
  })
  dropdown(cm_anchor, [column({"gap": 0, "pad": 1, "min_width": 180}, cm_items)], cm_open, 0, "menu_close")
end

def cpv_toolbar(tb_state, tb_lay)
  tb_status = tb_state["status"]
  tb_filter_label = tb_status == "All" ? "Filter" : tb_status
  tb_filter = cpv_choices(tb_state, "filter",
    cpv_button(tb_lay["card"] >= 520 ? tb_filter_label : "", "filter", "menu", {"which": "filter"}, tb_status == "All" ? "neutral" : "accent", "cp_filter", tb_state["menu"] == "filter"),
    ["All", "Live", "Limited", "Paused"], tb_status, "filter", 160)
  tb_left = [cpv_search(tb_state["q"], tb_lay["search"]), tb_filter]
  tb_n = tb_state["sel"].length()
  if tb_n > 0
    tb_left = tb_left.concat([
      text(str(tb_n) + " selected", {"size": 1, "weight": "medium", "fg": "text.default", "shrink": 0}),
      cpv_button("Pause", "", "pause_sel", {}, "neutral", "cp_pause_sel"),
      cpv_button("Resume", "", "resume_sel", {}, "neutral", "cp_resume_sel"),
      cpv_button("Clear", "", "clear_sel", {}, "quiet", "cp_clear_sel")
    ])
  end
  tb_dense = tb_state["dense"] == true
  tb_settings = cpv_choices(tb_state, "settings",
    cpv_button(tb_lay["card"] >= 900 ? "Settings" : "", "sliders", "menu", {"which": "settings"}, "neutral", "cp_settings", tb_state["menu"] == "settings"),
    ["Comfortable", "Compact"], tb_dense ? "Compact" : "Comfortable", "density", 180)
  tb_right = [tb_settings]
  tb_right = [cpv_columns_menu(tb_state)].concat(tb_right) if tb_lay["table"]
  row({"gap": 3, "align": "center", "width": "100%", "wrap": "wrap", "pad": [4, CPV_PAD_X, 4, CPV_PAD_X], "border": [1, 0, 0, 0], "border_color": "border.subtle"},
    tb_left.concat([spacer()]).concat(tb_right))
end

# ---- The table -------------------------------------------------------------------

def cpv_sort_mark(sm_on, sm_dir)
  return icon(sm_dir == "desc" ? "arrow_down" : "arrow_up", {"width": 14, "height": 14, "fg": "text.default", "shrink": 0}) if sm_on

  column({"gap": 0, "shrink": 0}, [
    icon("chevron_up", {"width": 10, "height": 8, "fg": "text.disabled"}),
    icon("chevron_down", {"width": 10, "height": 8, "fg": "text.disabled"})
  ])
end

def cpv_head_cell(hc_state, hc_id, hc_label, hc_w)
  hc_on = hc_state["sort"] == cpv_sort_key(hc_id)
  hc_base = {"display": "row", "gap": 2, "align": "center", "width": hc_w, "shrink": 0, "cursor": "pointer", "fg": "text.default"}
  {
    "k": "box",
    "key": "cp_head:" + hc_id,
    "s": hc_base,
    "on": {"click": "sort"},
    "p": {"col": cpv_sort_key(hc_id), "role": "column_header", "label": "Sort by " + hc_label},
    "c": [
      text(hc_label, {"size": 1, "weight": "semibold", "fg": hc_on ? "text.default" : "text.muted"}),
      cpv_sort_mark(hc_on, hc_state["dir"])
    ]
  }
end

def cpv_table_head(th_state, th_lay, th_page)
  th_ids = th_page["rows"].map(fn(c) { c["id"] })
  th_sel = th_state["sel"]
  th_on = th_ids.filter(fn(x) { th_sel.includes?(x) }).length()
  th_all = th_on > 0 && th_on == th_ids.length()
  th_box = checkbox("", th_all, "tick_page", {}, {"key": "cp_tick_page", "indeterminate": th_on > 0 && !th_all, "name": "Select this page"})
  th_cells = [
    row({"width": CPV_CHECK, "shrink": 0}, [th_box]),
    cpv_head_cell(th_state, "name", "Campaign", th_lay["name"])
  ].concat(th_lay["cols"].map(fn(c) { cpv_head_cell(th_state, c["id"], c["label"], c["w"]) }))
  th_cells = th_cells.concat([{"k": "box", "s": {"width": CPV_ACT, "shrink": 0}}])
  row({"gap": CPV_GAP, "align": "center", "width": "100%", "pad": [3, CPV_PAD_X, 3, CPV_PAD_X], "bg": "surface.base", "border": [1, 0, 1, 0], "border_color": "border.subtle"}, th_cells)
end

def cpv_campaign_cell(cc_c, cc_w)
  cc_text_w = cc_w - 36 - 12
  row({"gap": 4, "align": "center", "width": cc_w, "shrink": 0}, [
    cpv_channel_tile(cc_c["channel"], 36),
    column({"gap": 1, "width": cc_text_w}, [
      cpv_text_w(cc_c["name"], {"size": 1, "weight": "medium", "fg": "text.default"}, cc_text_w),
      row({"gap": 2, "align": "center"}, [
        cpv_market(cc_c["market"]),
        text(cc_c["audience"], {"size": 0, "fg": "text.muted"})
      ])
    ])
  ])
end

def cpv_cell(ce_c, ce_col)
  ce_id = ce_col["id"]
  ce_w = ce_col["w"]
  if ce_id == "status"
    return row({"width": ce_w, "shrink": 0}, [cpv_status(ce_c["status"])])
  end
  if ce_id == "spend"
    ce_pace = ce_c["spend"] * 100 / ce_c["budget"]
    return cpv_figure(cp_money(ce_c["spend"]), text(str(ce_pace) + "% of " + cp_money(ce_c["budget"]), {"size": 0, "fg": "text.muted"}), ce_w)
  end
  return cpv_figure(cp_count(ce_c["conv"]), cpv_delta(ce_c["conv_d"], true), ce_w) if ce_id == "conv"
  return cpv_figure(cp_cents(ce_c["cpa_c"]), cpv_delta(ce_c["cpa_d"], false), ce_w) if ce_id == "cpa"
  return row({"width": ce_w, "shrink": 0}, [cpv_sparkline(ce_c["trend"], ce_w, 32)]) if ce_id == "trend"
  if ce_id == "roas"
    # The scale runs to 15x, so the target's tick sits where 4.3x falls on it.
    return row({"gap": 3, "align": "center", "width": ce_w, "shrink": 0}, [
      cpv_text_w(cp_roas(ce_c["roas_t"]), {"size": 1, "weight": "medium", "fg": "text.default"}, 44),
      cpv_meter(ce_c["roas_t"] * 100 / 150, CP_TARGET_ROAS * 100 / 150, ce_c["roas_t"] >= CP_TARGET_ROAS, ce_w - 44 - 8)
    ])
  end
  row({"gap": 3, "align": "center", "width": ce_w, "shrink": 0}, [
    cpv_text_w(str(ce_c["nb"]) + "%", {"size": 1, "weight": "medium", "fg": "text.default"}, 40),
    cpv_meter(ce_c["nb"], 0, true, ce_w - 40 - 8)
  ])
end

def cpv_row_menu(rm_state, rm_c)
  rm_which = "row:" + rm_c["id"]
  rm_open = rm_state["menu"] == rm_which
  rm_anchor = control({
    "key": "cp_more:" + rm_c["id"],
    "tone": "quiet",
    "shape": {"width": CPV_ACT, "height": CPV_ACT, "pad": 0, "min_width": 0, "border": 0},
    "on": {"click": "menu"},
    "props": {"which": rm_which},
    "a11y": {"role": "button", "label": "Actions for " + rm_c["name"], "expanded": rm_open},
    "c": [icon("more_h", {"width": 18, "height": 18, "fg": "text.muted"})]
  })
  # The menu's items without the menu's own frame: the dropdown is the panel.
  rm_items = menu([rm_c["status"] == "Paused" ? "Resume" : "Pause"], "row_action")["c"].map(fn(it) {
    it["p"] = it["p"].merge({"id": rm_c["id"]})
    it
  })
  rm_drop = dropdown(rm_anchor, [column({"gap": 0, "min_width": 140}, rm_items)], rm_open, 0, "menu_close")
  row({"width": CPV_ACT, "shrink": 0, "justify": "center"}, [rm_drop])
end

def cpv_table_row(tr_state, tr_lay, tr_c)
  tr_ticked = tr_state["sel"].includes?(tr_c["id"])
  tr_dense = tr_state["dense"] == true
  tr_base = {
    "display": "row", "gap": CPV_GAP, "align": "center", "width": "100%",
    "pad": tr_dense ? [2, CPV_PAD_X, 2, CPV_PAD_X] : [4, CPV_PAD_X, 4, CPV_PAD_X],
    "border": [0, 0, 1, 0], "border_color": "border.subtle",
    "bg": tr_ticked ? "info.subtle" : "surface.raised",
    "transition": "fast"
  }
  tr_cells = [
    row({"width": CPV_CHECK, "shrink": 0}, [checkbox("", tr_ticked, "tick", {"id": tr_c["id"]}, {"key": "cp_tick:" + tr_c["id"], "name": "Select " + tr_c["name"]})]),
    cpv_campaign_cell(tr_c, tr_lay["name"])
  ].concat(tr_lay["cols"].map(fn(col) { cpv_cell(tr_c, col) })).concat([cpv_row_menu(tr_state, tr_c)])
  tr_row = {"k": "box", "s": tr_base, "c": tr_cells, "p": {"role": "row", "label": tr_c["name"]}}
  tr_row["on"] = stateful(tr_base, {"hover": {"bg": tr_ticked ? "info.subtle" : "surface.base"}}, {})
  keyed("cp_row:" + tr_c["id"], tr_row)
end

# Narrow: a campaign is a card of three lines instead of nine columns.
def cpv_item(it_state, it_lay, it_c)
  it_ticked = it_state["sel"].includes?(it_c["id"])
  it_w = it_lay["inner"] - 2 * CPV_PAD_X_PX
  it_text_w = it_w - CPV_CHECK - 36 - 4 * 8 - 72
  it_fig = fn(label, value) {
    column({"gap": 0, "grow": 1, "basis": 0}, [
      text(label, {"size": 0, "fg": "text.muted"}),
      text(value, {"size": 1, "weight": "medium", "fg": "text.default"})
    ])
  }
  it_line = column({"gap": 3, "width": "100%", "pad": [4, CPV_PAD_X, 4, CPV_PAD_X], "border": [0, 0, 1, 0], "border_color": "border.subtle", "bg": it_ticked ? "info.subtle" : "surface.raised"}, [
    row({"gap": 3, "align": "center", "width": "100%"}, [
      checkbox("", it_ticked, "tick", {"id": it_c["id"]}, {"key": "cp_tick:" + it_c["id"], "name": "Select " + it_c["name"]}),
      cpv_channel_tile(it_c["channel"], 36),
      column({"gap": 1, "width": it_text_w}, [
        cpv_text_w(it_c["name"], {"size": 1, "weight": "medium", "fg": "text.default"}, it_text_w),
        row({"gap": 2, "align": "center"}, [cpv_market(it_c["market"]), text(it_c["audience"], {"size": 0, "fg": "text.muted"})])
      ]),
      spacer(),
      cpv_status(it_c["status"])
    ]),
    row({"gap": 3, "width": "100%"}, [
      {"k": "box", "s": {"width": CPV_CHECK, "shrink": 0}},
      it_fig("Spend", cp_money(it_c["spend"])),
      it_fig("Conv.", cp_count(it_c["conv"])),
      it_fig("CPA", cp_cents(it_c["cpa_c"])),
      it_fig("ROAS", cp_roas(it_c["roas_t"]))
    ])
  ])
  keyed("cp_item:" + it_c["id"], it_line)
end

def cpv_empty(em_state)
  em_q = em_state["q"] ?? ""
  em_body = em_q == "" ? "No campaign has that status." : "Nothing matches “" + em_q + "”."
  column({"gap": 3, "align": "center", "width": "100%", "pad": [10, 6, 10, 6]}, [
    {"k": "box", "s": {"display": "row", "width": 48, "height": 48, "radius": 4, "bg": "surface.sunken", "justify": "center", "align": "center"}, "c": [
      icon("search", {"width": 24, "height": 24, "fg": "text.muted"})
    ]},
    text("No campaigns", {"size": 1, "weight": "semibold", "fg": "text.default"}),
    text(em_body, {"size": 1, "fg": "text.muted"})
  ])
end

# ---- The footer ------------------------------------------------------------------

def cpv_page_button(pb_n, pb_on)
  control({
    "key": "cp_page:" + str(pb_n),
    "tone": "quiet",
    "selected": pb_on,
    "shape": {"width": 36, "height": 36, "pad": 0, "min_width": 0, "border": 0},
    "on": {"click": "page"},
    "props": {"page": pb_n},
    "a11y": {"role": "button", "label": "Page " + str(pb_n), "selected": pb_on},
    "c": [text(str(pb_n), {"size": 1, "weight": pb_on ? "semibold" : "regular", "fg": pb_on ? "text.default" : "text.muted"})]
  })
end

def cpv_step_button(sb_icon, sb_page, sb_off, sb_label)
  control({
    "key": "cp_step:" + sb_icon,
    "tone": "quiet",
    "disabled": sb_off,
    "shape": {"width": 36, "height": 36, "pad": 0, "min_width": 0, "border": 0},
    "on": {"click": "page"},
    "props": {"page": sb_page},
    "a11y": {"role": "button", "label": sb_label},
    "c": [icon(sb_icon, {"width": 18, "height": 18})]
  })
end

def cpv_footer(ft_state, ft_lay, ft_page)
  ft_per = cpv_choices(ft_state, "per",
    control({
      "key": "cp_per", "tone": "neutral", "size": "md",
      "shape": {"gap": 3, "min_height": 36, "pad": [2, 4, 2, 4]},
      "on": {"click": "menu"}, "props": {"which": "per"},
      "a11y": {"role": "button", "label": "Rows per page", "expanded": ft_state["menu"] == "per"},
      "c": [text(str(ft_state["per"]), {"size": 1}), icon("chevron_down", {"width": 16, "height": 16, "fg": "text.muted"})]
    }),
    CP_PER_PAGE.map(fn(n) { str(n) }), str(ft_state["per"]), "per", 96)
  ft_from = ft_page["total"] == 0 ? 0 : ft_page["from"] + 1
  ft_says = str(ft_from) + " - " + str(ft_page["to"]) + " of " + str(ft_page["total"]) + " campaigns"
  ft_pages = range(1, ft_page["pages"] + 1).map(fn(n) { cpv_page_button(n, n == ft_page["page"]) })
  ft_left = ft_lay["card"] >= 640 ? [text("Rows per page", {"size": 1, "fg": "text.muted"}), ft_per] : [ft_per]
  ft_right = [text(ft_says, {"size": 1, "fg": "text.muted"})]
  ft_right = ft_right.concat([cpv_step_button("chevron_left", ft_page["page"] - 1, ft_page["page"] <= 1, "Previous page")])
  ft_right = ft_right.concat(ft_pages) if ft_lay["card"] >= 520
  ft_right = ft_right.concat([cpv_step_button("chevron_right", ft_page["page"] + 1, ft_page["page"] >= ft_page["pages"], "Next page")])
  row({"gap": 3, "align": "center", "width": "100%", "pad": [3, CPV_PAD_X, 3, CPV_PAD_X]},
    ft_left.concat([spacer()]).concat([row({"gap": 1, "align": "center"}, ft_right)]))
end

# ---- The page --------------------------------------------------------------------

def cpv_toast(to_state)
  to_note = toast(to_state["toast"], "success")
  to_note["p"] = (to_note["p"] ?? {}).merge({"wake": 3200})
  to_note["on"] = {"click": "toast_done", "wake": "toast_done"}
  to_note["s"]["cursor"] = "pointer"
  {
    "k": "overlay",
    "s": {"position": "absolute", "align": "start", "justify": "end", "pad": 6, "width": "100%"},
    "c": [to_note]
  }
end

def campaigns_view(raw_state)
  cv_state = cp_state(raw_state ?? {})
  cv_lay = cpv_lay(cv_state)
  cv_page = cp_page_of(cv_state)
  cv_tot = cp_totals(cp_in_view(cv_state))
  cv_body = []
  if cv_page["total"] == 0
    cv_body = [cpv_empty(cv_state)]
  elsif cv_lay["table"]
    cv_body = [cpv_table_head(cv_state, cv_lay, cv_page), column({"gap": 0, "width": "100%"}, cv_page["rows"].map(fn(c) { cpv_table_row(cv_state, cv_lay, c) }))]
  else
    cv_body = [column({"gap": 0, "width": "100%", "border": [1, 0, 0, 0], "border_color": "border.subtle"}, cv_page["rows"].map(fn(c) { cpv_item(cv_state, cv_lay, c) }))]
  end
  cv_card = column({
    "gap": 0, "width": cv_lay["card"], "bg": "surface.raised", "radius": 3,
    "border": 1, "border_color": "border.subtle", "shadow": 1, "overflow": "clip"
  }, [cpv_header(cv_state, cv_lay, cv_tot), cpv_kpis(cv_tot, cv_lay["kpi_per_row"]), cpv_toolbar(cv_state, cv_lay)].concat(cv_body).concat([cpv_footer(cv_state, cv_lay, cv_page)]))
  cv_scroll = scroll({"grow": 1, "min_height": 0, "width": "100%"}, [
    column({"gap": 0, "width": "100%", "align": "center", "pad": [cv_lay["pad"], 0, cv_lay["pad"], 0]}, [cv_card])
  ])
  cv_layers = [cv_scroll]
  cv_layers = cv_layers.concat([cpv_toast(cv_state)]) unless cv_state["toast"] == ""
  stack({"gap": 0, "width": "100%", "height": "100%", "bg": "surface.base"}, cv_layers)
end
