# The help desk's reducer.
#
#   soli test tests/helpdesk_spec.sl --no-coverage
#
# Everything checked here lives in `app/services/helpdesk_desk.sl`, which
# `soli test` preloads — so nothing is copied in and nothing can drift. The
# view is not tested here; what is, is the part a screenshot cannot show you
# is wrong: a count that disagrees with its list, a reply that does not move
# its ticket, a filter that quietly keeps a row it should drop.

def hd_spec_open(sp_state, sp_id)
  hd_reduce(sp_state, "open", {"props": {"id": sp_id}})
end

describe("The queue") do
  test("the inbox opens on Open tickets, newest first") do
    st = hd_state({})
    seen = hd_visible(st)
    assert_eq(seen.length(), 10)
    assert_eq(seen[0]["id"], 1051)
    assert_eq(seen.filter(fn(t) { t["status"] != "Open" }).length(), 0)
  end

  test("each tab shows only its status, and the counts agree with the lists") do
    st = hd_reduce({}, "filter", {"props": {"tab": "Pending"}})
    assert_eq(hd_visible(st).length(), hd_count(hd_scope(st), "Pending"))
    assert_eq(hd_visible(st).length(), 5)
    solved = hd_reduce(st, "filter", {"props": {"tab": "Solved"}})
    assert_eq(hd_visible(solved).length(), 6)
  end

  test("Assigned to me keeps only my tickets") do
    st = hd_reduce({}, "nav", {"props": {"path": "mine"}})
    mine = hd_visible(st)
    assert_eq(mine.length(), 4)
    assert_eq(mine.filter(fn(t) { t["agent"] != "Maya Chen" }).length(), 0)
  end

  test("pages hold eight and the last page holds the rest") do
    st = hd_state({})
    first = hd_page_of(st)
    assert_eq(first["pages"], 2)
    assert_eq(first["rows"].length(), 8)
    second = hd_page_of(hd_reduce(st, "page", {"props": {"page": 2}}))
    assert_eq(second["rows"].length(), 2)
    assert_eq(second["from"], 8)
  end

  test("a page past the end is clamped rather than empty") do
    st = hd_reduce({}, "page", {"props": {"page": 9}})
    assert_eq(hd_page_of(st)["page"], 2)
  end
end

describe("Search") do
  test("matches subject, customer, contact and ticket number, ignoring case") do
    assert_eq(hd_visible(hd_reduce({}, "search", {"payload": "kestrel"})).length(), 2)
    assert_eq(hd_visible(hd_reduce({}, "search", {"payload": "SSO"})).length(), 1)
    assert_eq(hd_visible(hd_reduce({}, "search", {"payload": "sofia"})).length(), 1)
    assert_eq(hd_visible(hd_reduce({}, "search", {"payload": "#1045"}))[0]["id"], 1045)
  end

  test("a search sent from another section lands in the inbox, on page one") do
    st = hd_reduce(hd_reduce({}, "nav", {"props": {"path": "reports"}}), "search", {"payload": "vat"})
    assert_eq(st["section"], "inbox")
    assert_eq(st["page"], 1)
    assert_eq(hd_visible(st).length(), 1)
  end

  test("clearing it brings the whole queue back") do
    st = hd_reduce(hd_reduce({}, "search", {"payload": "vat"}), "clear_search", {})
    assert_eq(hd_visible(st).length(), 10)
  end

  test("customers filter by name, contact or plan") do
    assert_eq(hd_customer_rows(hd_reduce({}, "cust_search", {"payload": "scale"})).length(), 4)
    assert_eq(hd_customer_rows(hd_reduce({}, "cust_search", {"payload": "okafor"}))[0]["id"], "orchard")
    assert_eq(hd_customer_rows(hd_state({})).length(), 10)
  end

  test("opening a customer searches the inbox for them") do
    st = hd_reduce({}, "cust_open", {"props": {"id": "kestrel"}})
    assert_eq(st["q"], "Kestrel Logistics")
    assert_eq(hd_visible(st).length(), 2)
  end
end

describe("A ticket") do
  test("changing the status moves it to the other tab and writes it in the thread") do
    st = hd_spec_open({}, 1049)
    st = hd_reduce(st, "set_status", {"props": {"value": "Solved"}})
    assert_eq(hd_find(st["tickets"], 1049)["status"], "Solved")
    assert_eq(hd_visible(st).filter(fn(t) { t["id"] == 1049 }).length(), 0)
    thread = hd_thread(st, 1049)
    assert_eq(thread[thread.length() - 1]["kind"], "event")
    assert_eq(thread[thread.length() - 1]["body"], "changed the status to Solved")
    assert_eq(st["solved_here"], 1)
    assert_eq(st["toast"], "#1049 status set to Solved")
  end

  test("priority and assignee change, and Unassigned clears the agent") do
    st = hd_spec_open({}, 1050)
    st = hd_reduce(st, "set_prio", {"props": {"value": "Low"}})
    st = hd_reduce(st, "set_agent", {"props": {"value": "Priya Nair"}})
    assert_eq(hd_find(st["tickets"], 1050)["prio"], "Low")
    assert_eq(hd_find(st["tickets"], 1050)["agent"], "Priya Nair")
    st = hd_reduce(st, "set_agent", {"props": {"value": "Unassigned"}})
    assert_eq(hd_find(st["tickets"], 1050)["agent"], "")
  end

  test("setting a property to what it already is changes nothing") do
    st = hd_spec_open({}, 1051)
    before = hd_thread(st, 1051).length()
    st = hd_reduce(st, "set_status", {"props": {"value": "Open"}})
    assert_eq(hd_thread(st, 1051).length(), before)
    assert_eq(st["toast"], "")
  end

  test("a reply lands in the thread, empties the draft and waits on the customer") do
    st = hd_spec_open({}, 1045)
    st = hd_reduce(st, "draft", {"payload": "Added IE 3456789KH and resent September's invoice."})
    st = hd_reduce(st, "reply", {})
    thread = hd_thread(st, 1045)
    last = thread[thread.length() - 1]
    assert_eq(last["kind"], "agent")
    assert_eq(last["by"], "Maya Chen")
    assert_eq(st["draft"], "")
    assert_eq(hd_find(st["tickets"], 1045)["status"], "Pending")
    assert_eq(st["toast"], "Reply sent to Grace Okafor")
  end

  test("the ticket last touched sorts to the top of its tab") do
    st = hd_spec_open({}, 1045)
    st = hd_reduce(st, "draft", {"payload": "Done."})
    st = hd_reduce(hd_reduce(st, "reply", {}), "filter", {"props": {"tab": "Pending"}})
    assert_eq(hd_visible(st)[0]["id"], 1045)
  end

  test("an internal note keeps the status and says it is a note") do
    st = hd_spec_open({}, 1043)
    st = hd_reduce(st, "draft", {"payload": "Ruth confirmed: move approvals to Kemi Balogun."})
    st = hd_reduce(st, "note", {})
    thread = hd_thread(st, 1043)
    assert_eq(thread[thread.length() - 1]["kind"], "note")
    assert_eq(hd_find(st["tickets"], 1043)["status"], "Open")
    assert_eq(st["toast"], "Internal note added to #1043")
  end

  test("an empty reply is refused and nothing is added") do
    st = hd_spec_open({}, 1042)
    before = hd_thread(st, 1042).length()
    st = hd_reduce(st, "reply", {})
    assert_eq(hd_thread(st, 1042).length(), before)
    assert_eq(st["toast"], "Write a reply first")
  end

  test("a seeded opening message is signed by the customer's contact") do
    assert_eq(hd_thread(hd_state({}), 1042)[0]["by"], "Hana Sato")
  end

  test("replying to one ticket leaves the seeded list untouched") do
    st = hd_spec_open({}, 1045)
    st = hd_reduce(hd_reduce(st, "draft", {"payload": "Done."}), "reply", {})
    assert_eq(hd_find(hd_seed_tickets(), 1045)["status"], "Open")
    assert_eq(hd_thread(hd_state({}), 1045).length(), 1)
  end
end

describe("Settings and small things") do
  test("a notification switch toggles on and off") do
    st = hd_reduce({}, "notify", {"props": {"id": "digest"}})
    assert_eq(st["set_notify"].includes?("digest"), true)
    st = hd_reduce(st, "notify", {"props": {"id": "digest"}})
    assert_eq(st["set_notify"].includes?("digest"), false)
  end

  test("save says so") do
    st = hd_reduce({}, "save", {})
    assert_eq(st["toast"], "Changes saved")
    assert_eq(st["saved"], "Saved just now")
  end

  test("one menu open at a time, and a second press closes it") do
    st = hd_reduce({}, "menu", {"props": {"which": "status"}})
    assert_eq(st["menu"], "status")
    st = hd_reduce(st, "menu", {"props": {"which": "prio"}})
    assert_eq(st["menu"], "prio")
    st = hd_reduce(st, "menu", {"props": {"which": "prio"}})
    assert_eq(st["menu"], "")
  end

  test("money, ages and initials read the way the screen prints them") do
    assert_eq(hd_money(49), "$49")
    assert_eq(hd_money(3450), "$3,450")
    assert_eq(hd_money(2005), "$2,005")
    assert_eq(hd_age_label(0), "Just now")
    assert_eq(hd_age_label(48), "48m")
    assert_eq(hd_age_label(530), "8h")
    assert_eq(hd_age_label(2900), "2d")
    assert_eq(hd_initials("Tomás Ortega"), "TO")
  end
end
