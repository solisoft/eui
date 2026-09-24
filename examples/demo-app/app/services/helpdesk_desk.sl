# Shiftwise's help desk: the data and the reducer, with no view in sight.
#
# A service and not part of `helpdesk_controller.sl` for the reason
# `chat_measure.sl` gives: `soli test` preloads `app/services` and never
# `app/controllers`, so this is the half `tests/helpdesk_spec.sl` can ask
# anything without a server behind it. Everything is an argument or a
# literal. No database: the queue is seeded into the session's state on
# connect and lives there, which is why the deploy needs no table for it.
#
# The state is kept small on purpose. A ticket row is seven short fields;
# the conversation a ticket opened with is a function of its id
# (`hd_seed_thread`) and only what this session *added* is stored
# (`extra`). The view rebuilds the whole tree on every event, so a state
# that carried every seeded message would be paid for on every keystroke.
#
# Every local is prefixed. Soli's scope is flat across calls — a callee's
# bare assignment writes its caller's variable of the same name — and a
# local named after a catalogue builder (`row`, `text`, `field`) rebinds the
# builder for the whole process.

HD_PAGE_SIZE = 8

# ---- Seed data ---------------------------------------------------------------

def hd_me
  "Maya Chen"
end

def hd_agents
  ["Maya Chen", "Tomás Ortega", "Priya Nair"]
end

def hd_statuses
  ["Open", "Pending", "Solved"]
end

def hd_priorities
  ["Urgent", "High", "Normal", "Low"]
end

def hd_customers
  [
    {"id": "harbor", "name": "Harbor Street Bakery", "contact": "Dana Whitfield", "email": "dana@harborstreetbakery.com", "plan": "Starter", "mrr": 49, "since": "Mar 2024"},
    {"id": "northgate", "name": "Northgate Physio", "contact": "Amir Haddad", "email": "amir@northgatephysio.co.uk", "plan": "Growth", "mrr": 190, "since": "Nov 2023"},
    {"id": "luma", "name": "Luma Dental Group", "contact": "Sofia Brandt", "email": "s.brandt@lumadental.de", "plan": "Scale", "mrr": 1240, "since": "Jun 2022"},
    {"id": "kestrel", "name": "Kestrel Logistics", "contact": "Owen Mercer", "email": "owen.mercer@kestrel-logistics.com", "plan": "Scale", "mrr": 2180, "since": "Jan 2022"},
    {"id": "pine", "name": "Pine & Pour", "contact": "Hana Sato", "email": "hana@pineandpour.co", "plan": "Starter", "mrr": 49, "since": "Aug 2025"},
    {"id": "orchard", "name": "Orchard Vets", "contact": "Grace Okafor", "email": "grace@orchardvets.ie", "plan": "Growth", "mrr": 290, "since": "Feb 2024"},
    {"id": "cobalt", "name": "Cobalt Fitness", "contact": "Luca Romano", "email": "luca@cobaltfitness.it", "plan": "Growth", "mrr": 390, "since": "Sep 2023"},
    {"id": "saltmarsh", "name": "Saltmarsh Hotel", "contact": "Ingrid Holm", "email": "ingrid.holm@saltmarsh.no", "plan": "Scale", "mrr": 980, "since": "Apr 2023"},
    {"id": "juniper", "name": "Juniper Care Homes", "contact": "Ruth Adeyemi", "email": "r.adeyemi@junipercare.org", "plan": "Scale", "mrr": 3450, "since": "Oct 2021"},
    {"id": "bramble", "name": "Bramble Café", "contact": "Tom Fairley", "email": "tom@bramblecafe.com", "plan": "Starter", "mrr": 29, "since": "Jul 2025"}
  ]
end

def hd_customer(hc_id)
  hc_found = hd_customers().filter(fn(c) { c["id"] == hc_id })
  return {"id": hc_id, "name": hc_id, "contact": hc_id, "email": "", "plan": "Starter", "mrr": 0, "since": ""} if hc_found.length() == 0

  hc_found[0]
end

# `age` is minutes since the ticket last moved; the list is sorted on it, so
# whatever was touched last is at the top.
def hd_ticket(tk_id, tk_subject, tk_cust, tk_prio, tk_status, tk_agent, tk_age, tk_topic)
  {"id": tk_id, "subject": tk_subject, "cust": tk_cust, "prio": tk_prio, "status": tk_status, "agent": tk_agent, "age": tk_age, "topic": tk_topic}
end

def hd_seed_tickets
  [
    hd_ticket(1051, "Shift swap requests aren't reaching managers", "northgate", "High", "Open", "Maya Chen", 12, "Notifications"),
    hd_ticket(1050, "Charged twice for September on the card ending 4417", "harbor", "Urgent", "Open", "", 26, "Billing"),
    hd_ticket(1049, "Google Workspace SSO fails for staff added this week", "luma", "Urgent", "Open", "Tomás Ortega", 48, "SSO"),
    hd_ticket(1048, "CSV export drops employees with accents in their names", "kestrel", "High", "Open", "Maya Chen", 95, "Exports"),
    hd_ticket(1047, "Mobile app shows yesterday's rota until it is force-closed", "cobalt", "Normal", "Open", "Priya Nair", 140, "Mobile"),
    hd_ticket(1046, "Different overtime rules for our Bergen and Oslo sites", "saltmarsh", "Normal", "Open", "Maya Chen", 210, "Payroll"),
    hd_ticket(1045, "Our VAT number is missing from the invoice", "orchard", "Low", "Open", "", 320, "Billing"),
    hd_ticket(1044, "API returns 429 on /v2/shifts during the nightly sync", "kestrel", "High", "Open", "Tomás Ortega", 400, "API"),
    hd_ticket(1043, "Timesheets stuck in Awaiting approval after a manager left", "juniper", "High", "Open", "Maya Chen", 530, "Timesheets"),
    hd_ticket(1042, "How do I copy last week's rota to the next four weeks?", "pine", "Normal", "Open", "", 610, "How-to"),
    hd_ticket(1041, "Clock-in geofence too tight at the Dock Road warehouse", "kestrel", "Normal", "Pending", "Priya Nair", 700, "Time clock"),
    hd_ticket(1040, "Cancel our trial before it converts on the 30th", "bramble", "Normal", "Pending", "Maya Chen", 820, "Billing"),
    hd_ticket(1039, "Holiday allowance resets on the wrong date", "juniper", "High", "Pending", "Tomás Ortega", 1000, "Leave"),
    hd_ticket(1038, "Add a second admin without giving them payroll access", "luma", "Low", "Pending", "Maya Chen", 1300, "Permissions"),
    hd_ticket(1037, "Printed rota cuts off the Sunday column", "harbor", "Low", "Pending", "Priya Nair", 1700, "Printing"),
    hd_ticket(1036, "Staff can't see open shifts on Android 15", "northgate", "Normal", "Solved", "Maya Chen", 2200, "Mobile"),
    hd_ticket(1035, "How is an upgrade from Growth to Scale prorated?", "cobalt", "Low", "Solved", "Tomás Ortega", 2900, "Billing"),
    hd_ticket(1034, "Break times not deducted from exported hours", "saltmarsh", "High", "Solved", "Maya Chen", 3600, "Exports"),
    hd_ticket(1033, "Reset two-factor for a locked-out manager", "orchard", "Urgent", "Solved", "Priya Nair", 4300, "Security"),
    hd_ticket(1032, "Import from Deputy skipped part-time contracts", "luma", "Normal", "Solved", "Maya Chen", 5200, "Imports"),
    hd_ticket(1031, "Start the week on Sunday instead of Monday", "pine", "Low", "Solved", "Tomás Ortega", 7100, "Settings")
  ]
end

# One message. `kind` is `customer`, `agent`, `note` (internal, the team
# only) or `event` (a property changed; drawn as a line, not a card).
def hd_msg(mg_kind, mg_by, mg_at, mg_body)
  {"kind": mg_kind, "by": mg_by, "at": mg_at, "body": mg_body}
end

# The conversation each ticket opened with. The five at the top of the
# queue have a history; the rest open with the customer's first message.
def hd_seed_thread(st_id)
  if st_id == 1051
    return [
      hd_msg("customer", "Amir Haddad", "Today, 08:52", "Since Monday, when one of our physios requests a swap, the manager on the other clinic never gets the notification. The swap just sits there and the physio ends up calling round. We have three swaps waiting right now."),
      hd_msg("note", "Maya Chen", "Today, 09:03", "Their managers were moved into the new Clinic Leads role last week. I think the role has no notification rule for swap approvals — checking with Tomás."),
      hd_msg("agent", "Maya Chen", "Today, 09:10", "Thanks Amir — I can see the three pending swaps. Could you confirm the managers still receive other notifications, such as leave requests? That tells me whether it is the role or the channel.")
    ]
  end
  if st_id == 1050
    return [
      hd_msg("customer", "Dana Whitfield", "Today, 08:30", "Our statement shows two charges of $49.00 for September, both on 1 September, on the card ending 4417. We only have the one location. Can you refund the duplicate?")
    ]
  end
  if st_id == 1049
    return [
      hd_msg("customer", "Sofia Brandt", "Today, 07:58", "Four dental nurses who started on Monday get 'Account not provisioned' when they sign in with Google. Everyone who joined before last week signs in fine."),
      hd_msg("agent", "Tomás Ortega", "Today, 08:20", "Hi Sofia, that message means the directory sync has not picked them up yet. Did they go into the same Google group as the rest of the team?"),
      hd_msg("customer", "Sofia Brandt", "Today, 08:41", "Yes, same group: clinic-staff@lumadental.de. I checked in the admin console this morning.")
    ]
  end
  if st_id == 1048
    return [
      hd_msg("customer", "Owen Mercer", "Today, 07:15", "The weekly hours export is missing rows. It looks like anyone with an accent in their name — Zoë Laurent, José Pires, Łukasz Nowak — is simply not in the CSV. Payroll runs on Thursday."),
      hd_msg("note", "Maya Chen", "Today, 07:40", "Reproduced on staging with a test user called Zoë. Filed as ENG-2291; the exporter drops rows that fail a Latin-1 encode.")
    ]
  end
  if st_id == 1043
    return [
      hd_msg("customer", "Ruth Adeyemi", "Yesterday, 16:20", "Our manager at the Ashford home left on Friday, and every timesheet she was the approver for is still Awaiting approval. Twelve carers are waiting to be paid."),
      hd_msg("agent", "Maya Chen", "Yesterday, 16:45", "I'm sorry for the wait, Ruth. Approvals follow the person, not the home, so they stayed with her account. I can move them to the new manager — who should that be?")
    ]
  end

  [hd_msg("customer", "", "Earlier", hd_opening(st_id))]
end

def hd_opening(op_id)
  return "Staff at our Dock Road site are clocking in inside the building and being told they are outside the geofence. It started after the app update." if op_id == 1041
  return "We decided Shiftwise isn't right for a café our size. Please cancel the trial so we aren't charged on the 30th." if op_id == 1040
  return "Allowances reset on 1 January, but our leave year starts on 1 April. Carers are seeing the wrong number of days left." if op_id == 1039
  return "I'd like our office manager to edit rotas and approve leave, but not see pay rates or export payroll." if op_id == 1038
  return "When I print the weekly rota in landscape the Sunday column is cut off at the right edge." if op_id == 1037
  return "Since the rota was published at 05:00, the app still shows yesterday's until you force-close it. Pull to refresh does nothing." if op_id == 1047
  return "Bergen pays overtime after 37.5 hours and Oslo after 40. Can one account hold two overtime rules?" if op_id == 1046
  return "Our accountant needs the VAT number IE 3456789KH on each invoice. Can you add it and resend September's?" if op_id == 1045
  return "The nightly sync of shifts into our warehouse system has failed three nights running with HTTP 429 after roughly 600 requests." if op_id == 1044
  return "Our rota barely changes week to week. Is there a way to copy last week forward for a month instead of one week at a time?" if op_id == 1042

  return "Since the Android 15 update, staff at our Leeds clinic see an empty Open shifts tab. iPhones are fine." if op_id == 1036
  return "If we move from Growth to Scale on the 18th, what do we pay this month?" if op_id == 1035
  return "Exported hours include the 30-minute unpaid break, so payroll is overpaying everyone on a long shift." if op_id == 1034
  return "Our duty manager lost her phone and cannot get past two-factor. She needs to publish tomorrow's rota tonight." if op_id == 1033
  return "We imported from Deputy and only full-time contracts came across. Our twenty part-timers are missing." if op_id == 1032
  return "Our week runs Sunday to Saturday. Can the rota start on Sunday?" if op_id == 1031

  "Please see the subject line."
end

# ---- Derived -----------------------------------------------------------------

def hd_defaults
  {
    "viewport": {"width": 1280, "height": 800},
    "section": "inbox",
    "filter": "Open",
    "page": 1,
    "sel": 0,
    "q": "",
    "cq": "",
    "draft": "",
    "menu": "",
    "nav_open": false,
    "toast": "",
    "tickets": hd_seed_tickets(),
    "extra": {},
    "solved_here": 0,
    "set_name": "Maya Chen",
    "set_email": "maya@shiftwise.io",
    "set_sig": "Maya — Shiftwise Support",
    "set_notify": ["assigned", "replies", "breach"],
    "set_start": "09:00",
    "set_end": "17:30",
    "set_tz": "Europe/London",
    "saved": "Saved 3 days ago"
  }
end

# What the session holds, with anything missing filled from the defaults.
def hd_state(hs_raw)
  hs_base = hd_defaults()
  hs_in = hs_raw ?? {}
  for hs_key in hs_base.keys()
    hs_base[hs_key] = hs_in[hs_key] unless hs_in[hs_key].nil?
  end
  hs_base
end

def hd_find(fd_tickets, fd_id)
  fd_hit = fd_tickets.filter(fn(t) { t["id"] == fd_id })
  return nil if fd_hit.length() == 0

  fd_hit[0]
end

# Newest first. Insertion into a fresh list, because `.concat` grows its
# receiver and the receiver here is the state's own array.
def hd_sorted(so_list)
  so_out = []
  for so_t in so_list
    so_before = so_out.filter(fn(o) { o["age"] <= so_t["age"] })
    so_after = so_out.filter(fn(o) { o["age"] > so_t["age"] })
    so_out = so_before.concat([so_t]).concat(so_after)
  end
  so_out
end

def hd_matches?(mt_ticket, mt_q)
  mt_said = (mt_q ?? "").strip().downcase()
  return true if mt_said == ""

  mt_c = hd_customer(mt_ticket["cust"])
  mt_hay = (mt_ticket["subject"] + " " + mt_c["name"] + " " + mt_c["contact"] + " #" + str(mt_ticket["id"]) + " " + mt_ticket["topic"]).downcase()
  mt_hay.index_of(mt_said) >= 0
end

# The tickets a section shows before the status tab is applied: every one
# in the inbox, only mine under "Assigned to me", and the search on both.
def hd_scope(sc_state)
  sc_mine = sc_state["section"] == "mine"
  sc_q = sc_state["q"]
  sc_state["tickets"].filter(fn(t) { (!sc_mine || t["agent"] == hd_me()) && hd_matches?(t, sc_q) })
end

def hd_count(ct_list, ct_status)
  ct_list.filter(fn(t) { t["status"] == ct_status }).length()
end

def hd_visible(vs_state)
  vs_filter = vs_state["filter"]
  hd_sorted(hd_scope(vs_state).filter(fn(t) { t["status"] == vs_filter }))
end

def hd_pages(pg_count)
  pg_n = (pg_count + HD_PAGE_SIZE - 1) / HD_PAGE_SIZE
  pg_n < 1 ? 1 : pg_n
end

def hd_page_of(po_state)
  po_all = hd_visible(po_state)
  po_page = po_state["page"]
  po_last = hd_pages(po_all.length())
  po_page = po_last if po_page > po_last
  po_page = 1 if po_page < 1
  po_from = (po_page - 1) * HD_PAGE_SIZE
  po_to = po_from + HD_PAGE_SIZE
  po_to = po_all.length() if po_to > po_all.length()
  {"rows": po_all.slice(po_from, po_to), "page": po_page, "pages": po_last, "from": po_from, "to": po_to, "total": po_all.length()}
end

def hd_thread(th_state, th_id)
  th_added = th_state["extra"][str(th_id)] ?? []
  th_t = hd_find(th_state["tickets"], th_id)
  th_who = "Customer"
  th_who = hd_customer(th_t["cust"])["contact"] unless th_t.nil?
  hd_seed_thread(th_id).map(fn(m) { m["by"] == "" ? m.merge({"by": th_who}) : m }).concat(th_added)
end

def hd_customer_rows(cr_state)
  cr_said = (cr_state["cq"] ?? "").strip().downcase()
  hd_customers().filter(fn(c) { cr_said == "" || (c["name"] + " " + c["contact"] + " " + c["email"] + " " + c["plan"]).downcase().index_of(cr_said) >= 0 })
end

def hd_open_for(of_state, of_cust)
  of_state["tickets"].filter(fn(t) { t["cust"] == of_cust && t["status"] != "Solved" }).length()
end

def hd_age_label(al_min)
  return "Just now" if al_min < 1
  return str(al_min) + "m" if al_min < 60
  return str(al_min / 60) + "h" if al_min < 1440

  str(al_min / 1440) + "d"
end

def hd_money(mn_n)
  return "$" + str(mn_n) if mn_n < 1000

  mn_rest = mn_n % 1000
  mn_pad = mn_rest < 10 ? "00" : (mn_rest < 100 ? "0" : "")
  "$" + str(mn_n / 1000) + "," + mn_pad + str(mn_rest)
end

def hd_initials(in_name)
  in_parts = (in_name ?? "").split(" ").filter(fn(p) { p != "" })
  return "?" if in_parts.length() == 0
  return in_parts[0].substring(0, 1) if in_parts.length() == 1

  in_parts[0].substring(0, 1) + in_parts[in_parts.length() - 1].substring(0, 1)
end

def hd_first_name(fn_name)
  return "Unassigned" if (fn_name ?? "") == ""

  fn_name.split(" ")[0]
end

# ---- The reducer -------------------------------------------------------------

def hd_set(ks_state, ks_key, ks_value)
  ks_state[ks_key] = ks_value
  ks_state
end

def hd_touched(td_t, td_key, td_value)
  td_copy = td_t.merge({})
  td_copy[td_key] = td_value
  td_copy["age"] = 0
  td_copy
end

# A copy of the list with one ticket's field changed and its clock reset.
# Always a fresh list, so nothing else holding the old one sees it move.
def hd_touch(tc_tickets, tc_id, tc_key, tc_value)
  tc_tickets.map(fn(t) { t["id"] == tc_id ? hd_touched(t, tc_key, tc_value) : t })
end

# Every other ticket gets a minute older, so the one just touched sorts above
# the one touched before it rather than tying with it at zero.
def hd_tick(tk_list)
  tk_list.map(fn(t) { t.merge({"age": t["age"] + 1}) })
end

def hd_append(ap_state, ap_id, ap_msg)
  ap_key = str(ap_id)
  ap_extra = ap_state["extra"].merge({})
  ap_extra[ap_key] = (ap_extra[ap_key] ?? []).concat([]).concat([ap_msg])
  hd_set(ap_state, "extra", ap_extra)
end

def hd_say(sy_state, sy_message)
  hd_set(sy_state, "toast", sy_message)
end

# Change one property of the open ticket, write it into the thread as an
# event, and say so.
def hd_change(ch_state, ch_key, ch_value)
  ch_id = ch_state["sel"]
  ch_t = hd_find(ch_state["tickets"], ch_id)
  return hd_set(ch_state, "menu", "") if ch_t.nil?
  return hd_set(ch_state, "menu", "") if ch_t[ch_key] == ch_value

  ch_was = ch_t["status"]
  ch_state = hd_set(ch_state, "tickets", hd_touch(hd_tick(ch_state["tickets"]), ch_id, ch_key, ch_value))
  ch_state = hd_set(ch_state, "menu", "")
  ch_state = hd_set(ch_state, "solved_here", ch_state["solved_here"] + 1) if ch_key == "status" && ch_value == "Solved"
  ch_state = hd_set(ch_state, "solved_here", ch_state["solved_here"] - 1) if ch_key == "status" && ch_was == "Solved"
  ch_what = "status"
  ch_what = "priority" if ch_key == "prio"
  ch_what = "assignee" if ch_key == "agent"
  ch_shown = ch_value == "" ? "Unassigned" : ch_value
  ch_state = hd_append(ch_state, ch_id, hd_msg("event", hd_me(), "Just now", "changed the " + ch_what + " to " + ch_shown))
  hd_say(ch_state, "#" + str(ch_id) + " " + ch_what + " set to " + ch_shown)
end

# A reply goes to the customer, so the ticket now waits on them: Open becomes
# Pending, and it leaves the Open tab. A note changes nothing but the thread.
def hd_reply(rp_state, rp_kind)
  rp_id = rp_state["sel"]
  rp_body = (rp_state["draft"] ?? "").strip()
  return hd_say(rp_state, "Write a reply first") if rp_body == ""

  rp_t = hd_find(rp_state["tickets"], rp_id)
  return rp_state if rp_t.nil?

  rp_state = hd_append(rp_state, rp_id, hd_msg(rp_kind, hd_me(), "Just now", rp_body))
  rp_state = hd_set(rp_state, "draft", "")
  if rp_kind == "note"
    rp_state = hd_set(rp_state, "tickets", hd_touch(hd_tick(rp_state["tickets"]), rp_id, "status", rp_t["status"]))
    return hd_say(rp_state, "Internal note added to #" + str(rp_id))
  end

  rp_next = rp_t["status"] == "Open" ? "Pending" : rp_t["status"]
  rp_state = hd_set(rp_state, "tickets", hd_touch(hd_tick(rp_state["tickets"]), rp_id, "status", rp_next))
  hd_say(rp_state, "Reply sent to " + hd_customer(rp_t["cust"])["contact"])
end

def hd_go(go_state, go_section)
  go_state = hd_set(go_state, "section", go_section)
  go_state = hd_set(go_state, "sel", 0)
  go_state = hd_set(go_state, "page", 1)
  go_state = hd_set(go_state, "menu", "")
  hd_set(go_state, "nav_open", false)
end

def hd_toggle_notify(tn_state, tn_id)
  tn_on = tn_state["set_notify"]
  tn_next = tn_on.filter(fn(x) { x != tn_id })
  tn_next = tn_next.concat([tn_id]) if tn_next.length() == tn_on.length()
  hd_set(tn_state, "set_notify", tn_next)
end

def hd_menu(mu_state, mu_which)
  hd_set(mu_state, "menu", mu_state["menu"] == mu_which ? "" : mu_which)
end

# {event, params, state} -> state. Everything the screen can do is here.
def hd_reduce(rd_raw, rd_event, rd_params)
  rd_state = hd_state(rd_raw)
  rd_p = rd_params ?? {}
  rd_props = rd_p["props"] ?? {}
  rd_said = rd_p["payload"]

  return hd_set(rd_state, "viewport", rd_p["viewport"] ?? rd_state["viewport"]) if rd_event == "connect" || rd_event == "viewport"
  return hd_go(rd_state, rd_props["path"] ?? "inbox") if rd_event == "nav"
  return hd_set(rd_state, "nav_open", !(rd_state["nav_open"] == true)) if rd_event == "nav_toggle"
  return hd_set(hd_set(rd_state, "filter", rd_props["tab"] ?? "Open"), "page", 1) if rd_event == "filter"
  return hd_set(rd_state, "page", rd_props["page"] ?? 1) if rd_event == "page"
  return hd_set(hd_set(hd_set(rd_state, "sel", rd_props["id"] ?? 0), "menu", ""), "draft", "") if rd_event == "open"
  return hd_set(hd_set(rd_state, "sel", 0), "menu", "") if rd_event == "back"
  if rd_event == "search"
    rd_state = hd_set(hd_set(hd_set(rd_state, "q", (rd_said ?? "").to_s), "page", 1), "sel", 0)
    return hd_set(rd_state, "section", "inbox") unless rd_state["section"] == "inbox" || rd_state["section"] == "mine"

    return rd_state
  end
  return hd_set(hd_set(rd_state, "q", ""), "page", 1) if rd_event == "clear_search"
  return hd_set(rd_state, "cq", (rd_said ?? "").to_s) if rd_event == "cust_search"
  return hd_set(rd_state, "cq", "") if rd_event == "cust_clear"
  if rd_event == "cust_open"
    rd_state = hd_go(rd_state, "inbox")
    return hd_set(rd_state, "q", hd_customer(rd_props["id"] ?? "")["name"])
  end
  return hd_set(rd_state, "draft", (rd_said ?? "").to_s) if rd_event == "draft"
  return hd_reply(rd_state, "agent") if rd_event == "reply"
  return hd_reply(rd_state, "note") if rd_event == "note"
  return hd_menu(rd_state, rd_props["which"] ?? "") if rd_event == "menu"
  return hd_change(rd_state, "status", rd_props["value"] ?? "Open") if rd_event == "set_status"
  return hd_change(rd_state, "prio", rd_props["value"] ?? "Normal") if rd_event == "set_prio"
  return hd_change(rd_state, "agent", (rd_props["value"] ?? "") == "Unassigned" ? "" : rd_props["value"]) if rd_event == "set_agent"
  if rd_event == "user_pick"
    return hd_go(rd_state, "settings")
  end
  return hd_set(rd_state, "set_name", (rd_said ?? "").to_s) if rd_event == "set_name"
  return hd_set(rd_state, "set_email", (rd_said ?? "").to_s) if rd_event == "set_email"
  return hd_set(rd_state, "set_sig", (rd_said ?? "").to_s) if rd_event == "set_sig"
  return hd_toggle_notify(rd_state, rd_props["id"] ?? "") if rd_event == "notify"
  return hd_set(hd_set(rd_state, "set_start", rd_props["value"] ?? "09:00"), "menu", "") if rd_event == "set_start"
  return hd_set(hd_set(rd_state, "set_end", rd_props["value"] ?? "17:30"), "menu", "") if rd_event == "set_end"
  return hd_set(hd_set(rd_state, "set_tz", rd_props["value"] ?? "Europe/London"), "menu", "") if rd_event == "set_tz"
  return hd_say(hd_set(rd_state, "saved", "Saved just now"), "Changes saved") if rd_event == "save"
  return hd_set(rd_state, "toast", "") if rd_event == "toast_done"

  rd_state
end
