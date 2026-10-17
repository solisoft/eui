# Northwind Outfitters' campaign book: the data and the reducer, with no view.
#
# A service for the reason `helpdesk_desk.sl` gives: `soli test` preloads
# `app/services` and never `app/controllers`, so this is the half
# `tests/campaigns_spec.sl` can ask anything without a server.
#
# The thirty campaigns are a function, not state. They are generated from a
# seed on every call — thirty rows of integers is nothing to build — so the
# session holds only what the person changed: the search, the sort, the page,
# what is ticked, what they paused, which columns they hid. The view rebuilds
# the whole tree on every event and the state is paid for on every keystroke.
#
# Every figure is an integer in a fixed unit — dollars, cents, tenths of a
# percent, tenths of an x — and is formatted here, so a total never drifts by
# a float's rounding and a sort compares what is printed.
#
# Every local is prefixed. Soli's scope is flat across calls, and a local
# named after a catalogue builder (`row`, `text`) rebinds it for the process.

CP_TARGET_ROAS = 43
CP_PER_PAGE = [10, 20, 30]

# ---- Seed data ---------------------------------------------------------------

def cp_products
  ["Glacier Gloves", "Ember Stove", "Meadow Hammock", "Harbor Duffel", "Coastline Shell", "Alpine Fleece", "Trailhead Jacket", "Summit Pack"]
end

def cp_flights
  ["Price Drop", "Creator Cuts", "Evergreen", "Launch", "Brand Exact", "Holiday Preview"]
end

# The channel is the row's mark: a letter in its series colour, so a column of
# them reads as a legend. Invented names, because a real network's logo is a
# trademark this demo has no business drawing.
def cp_channels
  ["Search", "Feed", "Video", "Audio", "Retail"]
end

def cp_markets
  ["US", "GB", "FR", "DE", "BR", "MX"]
end

def cp_audiences
  ["Prospecting", "Retargeting", "Winback", "Brand Search"]
end

# The columns a person may hide. The rest — name, status, spend — are what a
# row *is*, and a table without them is a list of numbers about nothing.
def cp_optional_columns
  ["conv", "trend", "cpa", "roas", "nb"]
end

# A Park–Miller step: the next value from the last, in 31 bits, so the book is
# the same on every machine and every boot.
def cp_next(nx_seed)
  (nx_seed * 48271) % 2147483647
end

def cp_campaign(cg_i, cg_r)
  cg_flight = cp_flights()[(cg_i * 3 + cg_i / 8) % 6]
  cg_budget = 26000 + (cg_r[0] % 56) * 1000
  cg_pace = 74 + cg_r[1] % 26
  cg_spend = cg_budget * cg_pace / 100
  cg_spend = cg_spend - cg_spend % 100
  cg_cpa = 800 + (cg_r[2] % 60) * (cg_r[3] % 4 == 0 ? 300 : 70)
  cg_conv = cg_spend * 100 / cg_cpa
  cg_trend = range(0, 16).map(fn(cg_k) { 30 + (cg_r[4 + cg_k] % 60) + (cg_k * (cg_r[2] % 3)) })
  cg_status = "Live"
  cg_status = "Limited" if cg_r[1] % 9 == 0
  cg_status = "Paused" if cg_r[1] % 11 == 0
  {
    "id": "CMP-" + str(1001 + cg_i),
    "name": cp_products()[cg_i % 8] + " " + cg_flight,
    "channel": cg_r[3] % 5,
    "market": cp_markets()[cg_r[5] % 6],
    "audience": cg_flight == "Brand Exact" ? "Brand Search" : cp_audiences()[cg_r[6] % 3],
    "status": cg_status,
    "budget": cg_budget,
    "spend": cg_spend,
    "imps": cg_spend * (40 + cg_r[7] % 50),
    "clicks": cg_spend * (40 + cg_r[7] % 50) * (12 + cg_r[8] % 25) / 1000,
    "conv": cg_conv,
    "conv_d": (cg_r[9] % 900) - 420,
    "cpa_c": cg_spend * 100 / (cg_conv < 1 ? 1 : cg_conv),
    "cpa_d": (cg_r[10] % 800) - 380,
    "roas_t": 4 + cg_r[11] % 130,
    "nb": 8 + cg_r[12] % 84,
    "trend": cg_trend,
    "trend_d": cg_trend[15] - cg_trend[0]
  }
end

# All thirty, most spent first, as the book opens.
def cp_campaigns
  cb_seed = 20261012
  cb_out = []
  for cb_i in range(0, 30)
    cb_r = []
    for cb_k in range(0, 20)
      cb_seed = cp_next(cb_seed)
      cb_r = cb_r.concat([cb_seed / 7])
    end
    cb_out = cb_out.concat([cp_campaign(cb_i, cb_r)])
  end
  cb_out
end

# ---- Formatting --------------------------------------------------------------

# `n` in units of a tenth, with the tenth dropped when it is zero: 661 is
# "66.1", 500 is "50". A table of figures reads faster without ".0" on half
# of them.
def cp_tenths(tt_n)
  tt_whole = tt_n / 10
  tt_part = tt_n % 10
  return str(tt_whole) if tt_part == 0

  str(tt_whole) + "." + str(tt_part)
end

def cp_thousands(th_n)
  return str(th_n) if th_n < 1000

  th_rest = th_n % 1000
  th_pad = th_rest < 10 ? "00" : (th_rest < 100 ? "0" : "")
  cp_thousands(th_n / 1000) + "," + th_pad + str(th_rest)
end

# Dollars, shortened the way a dashboard does: $940, $66.1K, $1.2M.
def cp_money(mo_n)
  return "$" + str(mo_n) if mo_n < 1000
  return "$" + cp_tenths((mo_n + 50) / 100) + "K" if mo_n < 1000000

  "$" + cp_tenths((mo_n + 50000) / 100000) + "M"
end

# Cents, to the cent: $27.05, $190.70.
def cp_cents(ce_n)
  ce_c = ce_n % 100
  "$" + cp_thousands(ce_n / 100) + "." + (ce_c < 10 ? "0" : "") + str(ce_c)
end

def cp_count(ct_n)
  return cp_thousands(ct_n) if ct_n < 10000
  return cp_tenths((ct_n + 50) / 100) + "K" if ct_n < 1000000

  cp_tenths((ct_n + 50000) / 100000) + "M"
end

# A change in tenths of a percent, always signed: "+23.2%", "-6%".
def cp_delta(dl_n)
  dl_abs = dl_n < 0 ? 0 - dl_n : dl_n
  (dl_n < 0 ? "-" : "+") + cp_tenths(dl_abs) + "%"
end

def cp_roas(ro_t)
  str(ro_t / 10) + "." + str(ro_t % 10) + "x"
end

# ---- State -------------------------------------------------------------------

def cp_defaults
  {
    "viewport": {"width": 1280, "height": 800},
    "q": "",
    "sort": "spend",
    "dir": "desc",
    "page": 1,
    "per": 10,
    "sel": [],
    "paused": [],
    "resumed": [],
    "status": "All",
    "hidden": [],
    "dense": false,
    "menu": "",
    "toast": ""
  }
end

def cp_state(cs_raw)
  cs_base = cp_defaults()
  cs_in = cs_raw ?? {}
  for cs_key in cs_base.keys()
    cs_base[cs_key] = cs_in[cs_key] unless cs_in[cs_key].nil?
  end
  cs_base
end

def cp_set(st_state, st_key, st_value)
  st_state[st_key] = st_value
  st_state
end

# A campaign as this session sees it: paused or resumed by hand on top of the
# seeded status.
def cp_as_seen(sn_c, sn_state)
  sn_c["status"] = "Paused" if sn_state["paused"].includes?(sn_c["id"])
  sn_c["status"] = "Live" if sn_state["resumed"].includes?(sn_c["id"])
  sn_c
end

def cp_matches?(mt_c, mt_q)
  mt_said = (mt_q ?? "").strip().downcase()
  return true if mt_said == ""

  mt_hay = (mt_c["name"] + " " + mt_c["id"] + " " + mt_c["market"] + " " + mt_c["audience"] + " " + cp_channels()[mt_c["channel"]]).downcase()
  mt_hay.index_of(mt_said) >= 0
end

# What the search and the status filter leave, before sorting and paging.
# The figures across the top are of this, so they follow the table.
def cp_in_view(iv_state)
  iv_status = iv_state["status"]
  cp_campaigns().map(fn(c) { cp_as_seen(c, iv_state) }).filter(fn(c) {
    cp_matches?(c, iv_state["q"]) && (iv_status == "All" || c["status"] == iv_status)
  })
end

def cp_sorted(so_list, so_key, so_dir)
  so_out = so_list.sort_by(so_key)
  so_dir == "desc" ? so_out.reverse() : so_out
end

def cp_pages(pg_count, pg_per)
  pg_n = (pg_count + pg_per - 1) / pg_per
  pg_n < 1 ? 1 : pg_n
end

def cp_page_of(po_state)
  po_all = cp_sorted(cp_in_view(po_state), po_state["sort"], po_state["dir"])
  po_per = po_state["per"]
  po_last = cp_pages(po_all.length(), po_per)
  po_page = po_state["page"]
  po_page = po_last if po_page > po_last
  po_page = 1 if po_page < 1
  po_from = (po_page - 1) * po_per
  po_to = po_from + po_per
  po_to = po_all.length() if po_to > po_all.length()
  {"rows": po_all.slice(po_from, po_to), "page": po_page, "pages": po_last, "from": po_from, "to": po_to, "total": po_all.length()}
end

# The six figures over the table, for what is in view.
def cp_totals(tl_list)
  tl_spend = 0
  tl_budget = 0
  tl_imps = 0
  tl_clicks = 0
  tl_conv = 0
  tl_prev = 0
  tl_revenue = 0
  tl_live = 0
  tl_below = 0
  for tl_c in tl_list
    tl_spend = tl_spend + tl_c["spend"]
    tl_budget = tl_budget + tl_c["budget"]
    tl_imps = tl_imps + tl_c["imps"]
    tl_clicks = tl_clicks + tl_c["clicks"]
    tl_conv = tl_conv + tl_c["conv"]
    tl_prev = tl_prev + tl_c["conv"] * 1000 / (1000 + tl_c["conv_d"])
    tl_revenue = tl_revenue + tl_c["spend"] * tl_c["roas_t"]
    tl_live = tl_live + 1 if tl_c["status"] == "Live"
    tl_below = tl_below + 1 if tl_c["roas_t"] < CP_TARGET_ROAS
  end
  tl_conv_safe = tl_conv < 1 ? 1 : tl_conv
  tl_prev_safe = tl_prev < 1 ? 1 : tl_prev
  tl_cpa = tl_spend * 100 / tl_conv_safe
  tl_cpa_prev = tl_spend * 100 / tl_prev_safe
  {
    "count": tl_list.length(),
    "live": tl_live,
    "spend": tl_spend,
    "budget": tl_budget,
    "paced": tl_budget < 1 ? 0 : tl_spend * 100 / tl_budget,
    "imps": tl_imps,
    "ctr_t": tl_imps < 1 ? 0 : tl_clicks * 1000 / tl_imps,
    "conv": tl_conv,
    "conv_d": (tl_conv - tl_prev) * 1000 / tl_prev_safe,
    "cpa_c": tl_cpa,
    "cpa_d": tl_cpa_prev < 1 ? 0 : (tl_cpa - tl_cpa_prev) * 1000 / tl_cpa_prev,
    "roas_t": tl_spend < 1 ? 0 : tl_revenue / tl_spend,
    "below": tl_below
  }
end

# ---- The reducer -------------------------------------------------------------

# A header pressed twice turns its sort round; a new header starts where a
# reader expects it to — names A to Z, figures largest first.
def cp_sort_by(sb_state, sb_key)
  if sb_state["sort"] == sb_key
    return cp_set(sb_state, "dir", sb_state["dir"] == "desc" ? "asc" : "desc")
  end

  sb_state = cp_set(sb_state, "sort", sb_key)
  cp_set(sb_state, "dir", sb_key == "name" || sb_key == "status" ? "asc" : "desc")
end

def cp_toggle_in(ti_list, ti_id)
  return ti_list.filter(fn(x) { x != ti_id }) if ti_list.includes?(ti_id)

  ti_list.concat([ti_id])
end

# The header's box ticks the page, or clears it when the page is already
# ticked. Rows ticked on other pages stay ticked either way.
def cp_tick_page(tp_state)
  tp_ids = cp_page_of(tp_state)["rows"].map(fn(c) { c["id"] })
  tp_sel = tp_state["sel"]
  tp_all = tp_ids.length() > 0 && tp_ids.filter(fn(x) { !tp_sel.includes?(x) }).length() == 0
  return cp_set(tp_state, "sel", tp_sel.filter(fn(x) { !tp_ids.includes?(x) })) if tp_all

  cp_set(tp_state, "sel", tp_sel.concat(tp_ids.filter(fn(x) { !tp_sel.includes?(x) })))
end

def cp_find(fd_state, fd_id)
  fd_hit = cp_campaigns().filter(fn(c) { c["id"] == fd_id })
  return nil if fd_hit.length() == 0

  cp_as_seen(fd_hit[0], fd_state)
end

# Pause or resume, from the row's menu or for everything ticked.
def cp_set_running(sr_state, sr_ids, sr_run)
  for sr_id in sr_ids
    sr_state["paused"] = sr_state["paused"].filter(fn(x) { x != sr_id })
    sr_state["resumed"] = sr_state["resumed"].filter(fn(x) { x != sr_id })
    sr_c = cp_find(sr_state, sr_id)
    unless sr_c.nil?
      sr_state["paused"] = sr_state["paused"].concat([sr_id]) if !sr_run && sr_c["status"] != "Paused"
      sr_state["resumed"] = sr_state["resumed"].concat([sr_id]) if sr_run && sr_c["status"] == "Paused"
    end
  end
  sr_n = sr_ids.length()
  sr_what = sr_n == 1 ? (cp_find(sr_state, sr_ids[0]) ?? {"name": sr_ids[0]})["name"] : str(sr_n) + " campaigns"
  cp_set(cp_set(sr_state, "menu", ""), "toast", (sr_run ? "Resumed " : "Paused ") + sr_what)
end

def cp_menu(mu_state, mu_which)
  cp_set(mu_state, "menu", mu_state["menu"] == mu_which ? "" : mu_which)
end

# {event, params, state} -> state. Everything the screen can do is here.
def cp_reduce(rd_raw, rd_event, rd_params)
  rd_state = cp_state(rd_raw)
  rd_p = rd_params ?? {}
  rd_props = rd_p["props"] ?? {}
  rd_said = rd_p["payload"]

  return cp_set(rd_state, "viewport", rd_p["viewport"] ?? rd_state["viewport"]) if rd_event == "connect" || rd_event == "viewport"
  return cp_set(cp_set(rd_state, "q", (rd_said ?? "").to_s), "page", 1) if rd_event == "cp_search"
  return cp_set(cp_set(cp_sort_by(rd_state, rd_props["col"] ?? "spend"), "page", 1), "menu", "") if rd_event == "cp_sort"
  return cp_set(rd_state, "page", rd_props["page"] ?? 1) if rd_event == "cp_page"
  return cp_set(cp_set(cp_set(rd_state, "per", int(rd_props["value"] ?? "10")), "page", 1), "menu", "") if rd_event == "cp_per"
  return cp_set(rd_state, "sel", cp_toggle_in(rd_state["sel"], rd_props["id"] ?? "")) if rd_event == "cp_tick"
  return cp_tick_page(rd_state) if rd_event == "cp_tick_page"
  return cp_set(rd_state, "sel", []) if rd_event == "cp_clear_sel"
  return cp_menu(rd_state, rd_props["which"] ?? "") if rd_event == "cp_menu"
  return cp_set(rd_state, "menu", "") if rd_event == "cp_menu_close"
  return cp_set(cp_set(cp_set(rd_state, "status", rd_props["value"] ?? "All"), "page", 1), "menu", "") if rd_event == "cp_filter"
  return cp_set(rd_state, "hidden", cp_toggle_in(rd_state["hidden"], rd_props["col"] ?? "")) if rd_event == "cp_column"
  return cp_set(cp_set(rd_state, "dense", (rd_props["value"] ?? "") == "Compact"), "menu", "") if rd_event == "cp_density"
  if rd_event == "cp_row_action"
    rd_id = rd_props["id"] ?? ""
    return cp_set_running(rd_state, [rd_id], (rd_props["item"] ?? "") == "Resume")
  end
  if rd_event == "cp_pause_sel" || rd_event == "cp_resume_sel"
    # Read the ticks before clearing them: `cp_set` changes the hash it is
    # given, so clearing first would leave nothing to pause.
    rd_ticked = rd_state["sel"]
    return cp_set_running(cp_set(rd_state, "sel", []), rd_ticked, rd_event == "cp_resume_sel")
  end
  return cp_set(rd_state, "toast", "Export is not wired up in this demo") if rd_event == "cp_export"
  return cp_set(rd_state, "toast", "New campaigns are not part of this demo") if rd_event == "cp_new"
  return cp_set(rd_state, "toast", "") if rd_event == "toast_done"

  rd_state
end
