# Meridian: a product landing page for an invented deploy-preview service,
# written in `tw()` classes and nothing else -- no colour, no pixel of
# spacing written down outside a class string, except the text widths a
# wrapping paragraph needs (a text node is measured against its own width).
#
# Every local carries its function's prefix: Soli's scope is flat across
# calls, and a local named like a builder rebinds it.

def site(event_data)
  sr_state = event_data["state"] ?? {}
  sr_event = event_data["event"]
  sr_p = event_data["params"] ?? {}
  sr_props = sr_p["props"] ?? {}
  sr_out = sr_state.merge({})
  sr_out["viewport"] = sr_p["viewport"] ?? sr_state["viewport"] if sr_event == "connect" || sr_event == "viewport"
  sr_out["tab"] = sr_props["tab"] ?? "Previews" if sr_event == "tab"
  sr_out["yearly"] = !(sr_state["yearly"] == true) if sr_event == "billing"
  if sr_event == "faq"
    sr_q = sr_props["q"] ?? -1
    sr_out["faq"] = (sr_state["faq"] ?? 0) == sr_q ? -1 : sr_q
  end
  sr_out["email"] = (sr_p["payload"] ?? "").to_s if sr_event == "email"
  sr_out["joined"] = (sr_out["email"] ?? "").index_of("@") > 0 if sr_event == "subscribe"
  sr_out
end

# ---- What the encoder speaks -----------------------------------------------------

# The deploy runs a released Soli, which may predate protocol 6 (gradients,
# pulse) or 7 (an absolute child naming its edge), and whose encoder refuses
# a style it does not know -- ending the session. So the page asks the
# encoder it runs on, once per process, through `eui_render` (the same
# `style_record` a session's frames go through), as the gallery's Tailwind
# card does, and draws the older way when the answer is no.
SV_PROBE = {}

def sv_speaks(sp_level)
  sp_key = "v" + str(sp_level)
  return SV_PROBE[sp_key] unless SV_PROBE[sp_key].nil?

  sp_style = {"position": "absolute_end"}
  sp_style = tw_style("bg-gradient-to-r from-indigo-600 to-[#ec4899] animate-pulse") if sp_level == 6
  sp_frame = eui_render({"k": "box", "s": sp_style, "c": []}) rescue nil
  SV_PROBE[sp_key] = !sp_frame.nil?
  SV_PROBE[sp_key]
end

# Protocol-6 classes, or what stands in for them on an older encoder.
def sv_v6(v6_classes, v6_plain)
  sv_speaks(6) ? v6_classes : v6_plain
end

# A badge in a card's top right corner. With protocol 7 the badge names its
# edge (`right-6`); before it, the stack packs every child from the end and
# the badge keeps a right margin, which is the same picture because the card
# fills the stack.
def sv_corner(co_w, co_card, co_classes, co_label)
  co_badge_text = text(co_label, tw_style("text-xs font-semibold text-white"))
  return stack({"width": co_w}, [co_card, node("box", {"tw": "absolute top-6 right-6 " + co_classes}, [co_badge_text])]) if sv_speaks(7)

  stack({"tw": "justify-end", "width": co_w}, [co_card, node("box", {"tw": "absolute top-6 mr-6 " + co_classes}, [co_badge_text])])
end

# ---- Measures ------------------------------------------------------------------

def sv_lay(ly_state)
  ly_w = (ly_state["viewport"] ?? {})["width"] ?? 1280
  ly_pad = 16
  ly_pad = 24 if ly_w >= 640
  ly_pad = 32 if ly_w >= 1024
  ly_inner = ly_w - 2 * ly_pad
  ly_inner = 1152 if ly_inner > 1152
  {"w": ly_w, "inner": ly_inner, "md": ly_w >= 768, "lg": ly_w >= 1024}
end

# A paragraph that wraps, at a width in px.
def sv_para(pa_text, pa_classes, pa_px)
  pa_s = tw_style(pa_classes)
  pa_s["width"] = pa_px
  text(pa_text, pa_s)
end

def sv_button(bt_label, bt_event, bt_props, bt_tone, bt_key)
  control({
    "key": "sb:" + bt_key,
    "tone": bt_tone,
    "size": "md",
    "tw": "px-4 shadow-sm",
    "shape": {"min_height": 36},
    "on": {"click": bt_event},
    "props": bt_props,
    "a11y": {"role": "button", "label": bt_label},
    "c": [text(bt_label, tw_style("text-sm font-semibold"))]
  })
end

def sv_clickable(ck_node, ck_event, ck_props)
  ck_node["on"] = (ck_node["on"] ?? {}).merge({"click": ck_event})
  ck_node["p"] = ck_props
  ck_node
end

def sv_section(se_lay, se_classes, se_kids)
  column({"tw": "w-full items-center " + se_classes, "vw": se_lay["w"]}, [
    column({"tw": "flex-col", "width": se_lay["inner"]}, se_kids)
  ])
end

def sv_eyebrow(ey_text)
  text(ey_text, tw_style("text-sm font-semibold text-indigo-600 uppercase"))
end

# ---- Top bar -------------------------------------------------------------------

def sv_logo()
  row({"tw": "items-center gap-2"}, [
    node("box", {"tw": "size-8 rounded-lg " + sv_v6("bg-gradient-to-br from-indigo-600 to-[#ec4899]", "bg-indigo-600")}, []),
    text("Meridian", tw_style("text-lg font-bold text-gray-900"))
  ])
end

def sv_nav(nv_lay)
  nv_links = []
  if nv_lay["md"]
    nv_links = ["Product", "Pricing", "Docs", "Changelog"].map(fn(l) {
      node("box", {"tw": "px-3 py-2 rounded-md cursor-pointer hover:bg-gray-100 transition-colors"}, [text(l, tw_style("text-sm font-medium text-gray-700"))])
    })
  end
  nv_right = nv_lay["md"] ? [text("Sign in", tw_style("text-sm font-semibold text-gray-700")), sv_button("Start free", "noop", {}, "accent", "nav-start")] : [icon("menu", tw_style("size-6 text-gray-700"))]
  sv_section(nv_lay, "bg-white border-b border-gray-200 py-3", [
    row({"tw": "items-center gap-6"}, [
      sv_logo(),
      row({"tw": "items-center gap-1"}, nv_links),
      row({"tw": "ml-auto items-center gap-4"}, nv_right)
    ])
  ])
end

# ---- Hero ----------------------------------------------------------------------

def sv_pill()
  row({"tw": "self-start items-center gap-2 px-3 py-1 rounded-full bg-white ring-1 ring-gray-900/5 shadow-sm"}, [
    node("box", {"tw": "size-2 rounded-full bg-green-500" + sv_v6(" animate-pulse", "")}, []),
    text("Meridian 3 is live", tw_style("text-xs font-semibold text-gray-900")),
    text("Read the notes", tw_style("text-xs font-medium text-indigo-600"))
  ])
end

# A deploy chart: builds per hour, the last one still running.
def sv_chart(ch_w, ch_h)
  ch_vals = [12, 18, 15, 22, 30, 26, 34, 41, 38, 47, 44, 52]
  ch_n = ch_vals.length()
  ch_step = float(ch_w) / float(ch_n)
  ch_bar = ch_step * 0.62
  ch_paths = [ [1, "surface.sunken", 0, ch_h - 1, ch_w, 1, 0] ]
  for ch_i in range(0, ch_n)
    ch_bh = float(ch_vals[ch_i]) / 56.0 * float(ch_h - 8)
    ch_col = ch_i == ch_n - 1 ? "series.2" : "accent.base"
    ch_paths = ch_paths.concat([ [1, ch_col, ch_i * ch_step + (ch_step - ch_bar) / 2.0, float(ch_h) - ch_bh, ch_bar, ch_bh, 3] ])
  end
  canvas(ch_w, ch_h, ch_paths)
end

def sv_preview_card(pc_w)
  pc_rows = [
    ["feat/checkout-v2", "Ready", "success", "42 s"],
    ["fix/cart-rounding", "Building", "warning", "…"],
    ["chore/deps-sept", "Ready", "success", "37 s"]
  ]
  pc_list = column({"tw": "divide-y divide-gray-100"}, pc_rows.map(fn(r) {
    row({"tw": "items-center gap-3 py-2.5"}, [
      node("box", {"tw": "size-2 rounded-full " + (r[2] == "success" ? "bg-green-500" : "bg-yellow-500" + sv_v6(" animate-pulse", ""))}, []),
      text(r[0], tw_style("text-sm font-mono text-gray-900 truncate")),
      row({"tw": "ml-auto items-center gap-3"}, [
        text(r[3], tw_style("text-xs text-gray-500")),
        badge(r[1], r[2])
      ])
    ])
  }))
  sv_corner(pc_w,
    column({"tw": "flex-col gap-4 p-6 rounded-2xl bg-white shadow-xl ring-1 ring-gray-900/5"}, [
      text("Deploys today", tw_style("text-sm font-semibold text-gray-900")),
      row({"tw": "items-end gap-3"}, [
        text("412", tw_style("text-4xl font-bold text-gray-900")),
        text("+18% on last week", tw_style("pb-1 text-sm font-semibold text-green-600"))
      ]),
      sv_chart(pc_w - 48, 96),
      pc_list
    ]), "px-2 py-0.5 rounded-md bg-indigo-600", "LIVE")
end

def sv_hero(he_lay)
  he_two = he_lay["lg"]
  he_card_w = he_two ? 440 : (he_lay["inner"] > 480 ? 480 : he_lay["inner"])
  he_text_w = he_two ? he_lay["inner"] - he_card_w - 64 : he_lay["inner"]
  he_copy = column({"tw": "flex-col gap-6", "width": he_text_w}, [
    sv_pill(),
    sv_para("Every pull request, a live preview your whole team can click.", "text-4xl font-bold text-gray-900", he_text_w),
    sv_para("Meridian builds each branch in under a minute, gives it a URL, and tears it down when the branch merges. Reviewers stop reading diffs of a checkout page and start using one.", "text-lg text-gray-600", he_text_w > 560 ? 560 : he_text_w),
    row({"tw": "items-center gap-3 flex-wrap"}, [
      sv_button("Start free for 14 days", "noop", {}, "accent", "hero-start"),
      sv_button("Book a demo", "noop", {}, "neutral", "hero-demo")
    ]),
    row({"tw": "items-center gap-2"}, [
      icon("check", tw_style("size-4 text-green-600")),
      text("No card needed · SOC 2 Type II", tw_style("text-sm text-gray-500"))
    ])
  ])
  he_body = he_two ? row({"tw": "items-center gap-16"}, [he_copy, sv_preview_card(he_card_w)]) : column({"tw": "flex-col gap-12"}, [he_copy, sv_preview_card(he_card_w)])
  column({"tw": "w-full items-center py-16 lg:py-20 " + sv_v6("bg-gradient-to-b from-blue-50 to-white", "bg-gray-50"), "vw": he_lay["w"]}, [
    column({"tw": "flex-col", "width": he_lay["inner"]}, [he_body])
  ])
end

# ---- Numbers -------------------------------------------------------------------

def sv_stats(st_lay)
  st_items = [ ["38 s", "median build"], ["99.98%", "preview uptime"], ["12,400", "teams"], ["4.9 / 5", "on G2"] ]
  st_cols = st_lay["md"] ? 4 : 2
  sv_section(st_lay, "bg-white py-12", [
    node("box", {"tw": "grid gap-8 grid-cols-" + str(st_cols)}, st_items.map(fn(s) {
      column({"tw": "flex-col gap-1 items-center"}, [
        text(s[0], tw_style("text-3xl font-bold text-gray-900")),
        text(s[1], tw_style("text-sm text-gray-500"))
      ])
    }))
  ])
end

# ---- Features, by tab ----------------------------------------------------------

def sv_features_of(fo_tab)
  if fo_tab == "Collaboration"
    return [ ["Comments on the page", "Pin a note to the button that is wrong, not to line 212.", "star"], ["Shared sessions", "Everyone sees the same cart, the same seed data, the same bug.", "circle"], ["Slack and Linear", "The preview link lands where the conversation already is.", "arrow_right"] ]
  end
  if fo_tab == "Security"
    return [ ["Private by default", "Previews sit behind your SSO; a link alone opens nothing.", "warning"], ["Secrets per branch", "Staging keys, never production, injected at build time.", "check"], ["Audit trail", "Who opened what, when, exportable to your SIEM.", "search"] ]
  end
  [ ["Build in seconds", "A warm cache per repository: most previews are ready in under a minute.", "arrow_right"], ["One URL per branch", "Stable, shareable, and gone when the branch merges.", "check"], ["Seeded databases", "Each preview gets a copy of fixtures, not your customers' data.", "calendar"] ]
end

def sv_tabs(tb_on)
  row({"tw": "gap-8 border-b border-gray-200"}, ["Previews", "Collaboration", "Security"].map(fn(t) {
    tb_cls = t == tb_on ? "py-3 border-b-2 border-b-indigo-600 cursor-pointer" : "py-3 border-b-2 border-transparent cursor-pointer hover:border-b-gray-300"
    tb_ink = t == tb_on ? "text-sm font-semibold text-indigo-600" : "text-sm font-medium text-gray-500"
    sv_clickable(node("box", {"tw": tb_cls}, [text(t, tw_style(tb_ink))]), "tab", {"tab": t})
  }))
end

def sv_features(fe_state, fe_lay)
  fe_tab = fe_state["tab"] ?? "Previews"
  fe_cols = fe_lay["md"] ? 3 : 1
  fe_gap = fe_lay["md"] ? 32 : 0
  fe_card_w = (fe_lay["inner"] - fe_gap * (fe_cols - 1)) / fe_cols
  fe_cards = sv_features_of(fe_tab).map(fn(f) {
    column({"tw": "flex-col gap-3 p-6 rounded-xl bg-white ring-1 ring-gray-200 shadow-sm hover:shadow-lg transition"}, [
      node("box", {"tw": "flex size-10 items-center justify-center rounded-lg bg-indigo-600"}, [icon(f[2], tw_style("size-5 text-white"))]),
      text(f[0], tw_style("text-base font-semibold text-gray-900")),
      sv_para(f[1], "text-sm text-gray-600", fe_card_w - 50)
    ])
  })
  sv_section(fe_lay, "bg-gray-50 py-20", [
    column({"tw": "flex-col gap-10"}, [
      column({"tw": "flex-col gap-3"}, [
        sv_eyebrow("Why teams switch"),
        sv_para("Review the product, not the patch", "text-3xl font-bold text-gray-900", fe_lay["inner"])
      ]),
      sv_tabs(fe_tab),
      node("box", {"tw": "grid gap-8 grid-cols-" + str(fe_cols)}, fe_cards)
    ])
  ])
end

# ---- Alternating rows (flex-row-reverse) -------------------------------------------

def sv_split(sp_lay, sp_flip, sp_title, sp_body, sp_visual)
  sp_half = sp_lay["md"] ? (sp_lay["inner"] - 64) / 2 : sp_lay["inner"]
  sp_copy = column({"tw": "flex-col gap-4", "width": sp_half}, [
    sv_para(sp_title, "text-2xl font-bold text-gray-900", sp_half),
    sv_para(sp_body, "text-base text-gray-600", sp_half)
  ])
  sp_cls = sp_lay["md"] ? (sp_flip ? "flex flex-row-reverse items-center gap-16" : "flex items-center gap-16") : "flex flex-col gap-8"
  node("box", {"tw": sp_cls}, [sp_copy, sp_visual(sp_half)])
end

def sv_ring_visual(rv_w)
  rv_h = 200
  rv_c = rv_w / 2
  column({"tw": "items-center justify-center rounded-2xl bg-white ring-1 ring-gray-200 py-6", "width": rv_w}, [
    canvas(220, rv_h - 40, [
      [4, "surface.sunken", 18, 110, 80, 60, 0, 6.283],
      [4, "accent.base", 18, 110, 80, 60, -1.571, 3.77],
      [3, "success.base", 110, 80, 8]
    ]),
    text("83% of previews ready under 60 s", tw_style("text-sm font-medium text-gray-700"))
  ])
end

def sv_list_visual(lv_w)
  column({"tw": "flex-col rounded-2xl bg-white ring-1 ring-gray-200 divide-y divide-gray-100", "width": lv_w}, [
    ["Maya approved", "checkout-v2 · 2 min ago", "success"],
    ["Tom commented", "\"The total jumps on hover\"", "info"],
    ["Preview rebuilt", "fix/cart-rounding · 38 s", "neutral"]
  ].map(fn(e) {
    row({"tw": "items-center gap-3 px-5 py-4"}, [
      badge(e[0], e[2]),
      text(e[1], tw_style("text-sm text-gray-500 truncate"))
    ])
  }))
end

def sv_story(so_lay)
  sv_section(so_lay, "bg-white py-20", [
    column({"tw": "flex-col gap-20"}, [
      sv_split(so_lay, false, "Fast enough to be the default", "A warm, per-repository cache and incremental builds mean the preview is usually ready before the CI checks finish. Nobody waits for it, so everybody uses it.", fn(w) { sv_ring_visual(w) }),
      sv_split(so_lay, true, "Feedback where the pixels are", "Reviewers comment on the running page. Every note carries the branch, the viewport and the account it was left from, so a bug report is reproducible by construction.", fn(w) { sv_list_visual(w) })
    ])
  ])
end

# ---- Pricing -------------------------------------------------------------------

def sv_toggle(tg_yearly)
  tg_on = "px-3 py-1.5 rounded-md bg-white shadow-sm cursor-pointer"
  tg_off = "px-3 py-1.5 rounded-md cursor-pointer hover:bg-gray-200"
  sv_clickable(row({"tw": "items-center gap-1 p-1 rounded-lg bg-gray-100 self-center"}, [
    node("box", {"tw": tg_yearly ? tg_off : tg_on}, [text("Monthly", tw_style("text-sm font-semibold text-gray-900"))]),
    row({"tw": (tg_yearly ? tg_on : tg_off) + " items-center gap-2"}, [
      text("Yearly", tw_style("text-sm font-semibold text-gray-900")),
      badge("−20%", "success")
    ])
  ]), "billing", {})
end

def sv_plan(pl_name, pl_month, pl_yearly, pl_blurb, pl_points, pl_hot, pl_w)
  pl_price = pl_month == 0 ? "$0" : "$" + str(pl_yearly ? pl_month * 8 / 10 : pl_month)
  pl_ring = pl_hot ? "ring-2 ring-indigo-600 shadow-xl" : "ring-1 ring-gray-200 shadow-sm"
  pl_card = column({"tw": "flex-col gap-6 p-8 rounded-2xl bg-white " + pl_ring, "width": pl_w}, [
    text(pl_name, tw_style("text-lg font-semibold " + (pl_hot ? "text-indigo-600" : "text-gray-900"))),
    sv_para(pl_blurb, "text-sm text-gray-600", pl_w - 68),
    row({"tw": "items-end gap-1"}, [
      text(pl_price, tw_style("text-4xl font-bold text-gray-900")),
      text(pl_month == 0 ? "forever" : "/ seat / month", tw_style("text-sm text-gray-500 pb-1"))
    ]),
    sv_button(pl_month == 0 ? "Start free" : "Choose " + pl_name, "noop", {}, pl_hot ? "accent" : "neutral", "plan-" + pl_name),
    column({"tw": "flex-col gap-3"}, pl_points.map(fn(p) {
      row({"tw": "items-center gap-3"}, [icon("check", tw_style("size-4 text-indigo-600")), text(p, tw_style("text-sm text-gray-700"))])
    }))
  ])
  return pl_card unless pl_hot

  sv_corner(pl_w, pl_card, "px-2.5 py-1 rounded-full bg-indigo-600", "Most popular")
end

def sv_pricing(pr_state, pr_lay)
  pr_yearly = pr_state["yearly"] == true
  pr_cols = pr_lay["lg"] ? 3 : 1
  pr_w = pr_lay["lg"] ? (pr_lay["inner"] - 64) / 3 : (pr_lay["inner"] > 440 ? 440 : pr_lay["inner"])
  pr_plans = [
    sv_plan("Hobby", 0, pr_yearly, "For side projects and trying it out.", ["3 previews at a time", "Public repositories", "Community support"], false, pr_w),
    sv_plan("Team", 24, pr_yearly, "For product teams who review in the browser.", ["Unlimited previews", "SSO and private previews", "Comments on the page", "Seeded databases"], true, pr_w),
    sv_plan("Scale", 49, pr_yearly, "For many teams, many repos, one bill.", ["Everything in Team", "Audit trail and SIEM export", "99.99% preview SLA", "A named engineer"], false, pr_w)
  ]
  pr_grid = pr_cols == 3 ? row({"tw": "items-start gap-8"}, pr_plans) : column({"tw": "flex-col items-center gap-8"}, pr_plans)
  sv_section(pr_lay, "bg-gray-50 py-20", [
    column({"tw": "flex-col gap-10"}, [
      column({"tw": "flex-col items-center gap-3"}, [
        sv_eyebrow("Pricing"),
        text("Pay for seats, not for builds", tw_style("text-3xl font-bold text-gray-900 text-center")),
        sv_toggle(pr_yearly)
      ]),
      pr_grid
    ])
  ])
end

# ---- FAQ -----------------------------------------------------------------------

def sv_faq(fq_state, fq_lay)
  fq_open = fq_state["faq"] ?? 0
  fq_w = fq_lay["inner"] > 768 ? 768 : fq_lay["inner"]
  fq_items = [
    ["Does a preview use production data?", "Never. Each preview gets its own database, seeded from fixtures you commit. Production credentials are not available to a preview build."],
    ["How long does a preview live?", "Until its branch merges or is deleted, then it is torn down within a minute. You can pin one for a demo."],
    ["Which frameworks work?", "Anything that builds in a container and listens on a port: Rails, Next.js, Django, Phoenix, Soli."],
    ["Can I self-host?", "The Scale plan runs the build fleet in your own cloud account; the dashboard stays with us."]
  ]
  fq_rows = range(0, fq_items.length()).map(fn(i) {
    fq_is = i == fq_open
    fq_kids = [
      row({"tw": "items-center gap-4 py-5 cursor-pointer"}, [
        text(fq_items[i][0], tw_style("text-base font-semibold text-gray-900")),
        icon(fq_is ? "chevron_up" : "chevron_down", tw_style("ml-auto size-5 text-gray-500"))
      ])
    ]
    fq_kids = fq_kids.concat([sv_para(fq_items[i][1], "text-sm text-gray-600 pb-5", fq_w - 40)]) if fq_is
    sv_clickable(column({"tw": "flex-col"}, fq_kids), "faq", {"q": i})
  })
  sv_section(fq_lay, "bg-white py-20", [
    column({"tw": "flex-col gap-8 mx-auto", "width": fq_w}, [
      text("Questions", tw_style("text-3xl font-bold text-gray-900")),
      column({"tw": "flex-col divide-y divide-gray-200 border-t border-b border-gray-200"}, fq_rows)
    ])
  ])
end

# ---- Newsletter and footer ----------------------------------------------------------

def sv_signup(sg_state, sg_lay)
  sg_done = sg_state["joined"] == true
  sg_form = sg_done ? row({"tw": "items-center gap-2 px-4 py-2.5 rounded-lg bg-green-50"}, [
    icon("check", tw_style("size-5 text-green-600")),
    text("You're on the list: " + (sg_state["email"] ?? ""), tw_style("text-sm font-medium text-green-700"))
  ]) : row({"tw": "items-center gap-3"}, [
    input(sg_state["email"] ?? "", "email", {"key": "sg-email", "placeholder": "you@company.com", "style": {"width": 280}}),
    sv_button("Subscribe", "subscribe", {}, "accent", "subscribe")
  ])
  sg_inner = sg_lay["inner"] - 96
  sv_section(sg_lay, "bg-white pb-20", [
    column({"tw": "flex-col items-center gap-6 px-6 py-16 rounded-3xl shadow-lg " + sv_v6("bg-gradient-to-r from-indigo-600 via-info to-[#ec4899]", "bg-indigo-600")}, [
      text("A changelog worth reading", tw_style("text-3xl font-bold text-white text-center")),
      text("One email a month. What shipped, what broke, what we learned.", tw_style("text-base text-white text-center opacity-90")),
      column({"tw": "flex-col p-2 rounded-xl bg-white shadow-sm"}, [sg_form])
    ])
  ])
end

def sv_footer(ft_lay)
  sv_section(ft_lay, "bg-gray-50 border-t border-gray-200 py-10", [
    node("box", {"tw": ft_lay["md"] ? "flex items-center gap-8" : "flex flex-col gap-4"}, [
      sv_logo(),
      row({"tw": "items-center gap-6"}, ["Privacy", "Terms", "Status", "Security"].map(fn(l) { text(l, tw_style("text-sm text-gray-500")) })),
      text("© 2026 Meridian Labs · a demo, not a company", tw_style("ml-auto text-sm text-gray-400"))
    ])
  ])
end

def site_view(raw_state)
  vw_state = raw_state ?? {}
  vw_lay = sv_lay(vw_state)
  column({"tw": "w-full h-full bg-white"}, [
    sv_nav(vw_lay),
    scroll({"grow": 1, "min_height": 0, "width": "100%"}, [
      column({"tw": "w-full"}, [
        sv_hero(vw_lay),
        sv_stats(vw_lay),
        sv_features(vw_state, vw_lay),
        sv_story(vw_lay),
        sv_pricing(vw_state, vw_lay),
        sv_faq(vw_state, vw_lay),
        sv_signup(vw_state, vw_lay),
        sv_footer(vw_lay)
      ])
    ])
  ])
end
