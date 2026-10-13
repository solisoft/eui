# The campaign table's reducer and its figures.
#
#   soli test tests/campaigns_spec.sl --no-coverage
#
# Everything checked here lives in `app/services/campaigns_book.sl`, which
# `soli test` preloads — so nothing is copied in and nothing can drift. What a
# screenshot cannot show you is wrong is what is tested: a total that disagrees
# with its rows, a sort that is not one, a tick that leaks across pages.

def cp_spec_ids(sp_state)
  cp_page_of(sp_state)["rows"].map(fn(c) { c["id"] })
end

describe("The book") do
  test("thirty campaigns, the same thirty every time, ids unique") do
    first = cp_campaigns()
    again = cp_campaigns()
    assert_eq(first.length(), 30)
    assert_eq(first[7]["name"], again[7]["name"])
    assert_eq(first[7]["spend"], again[7]["spend"])
    names = first.map(fn(c) { c["name"] })
    assert_eq(names.filter(fn(n) { names.filter(fn(m) { m == n }).length() > 1 }).length(), 0)
  end

  test("a campaign's CPA is its spend over its conversions, in cents") do
    for c in cp_campaigns()
      assert_eq(c["cpa_c"], c["spend"] * 100 / c["conv"])
      assert(c["spend"] <= c["budget"])
    end
  end
end

describe("Formatting") do
  test("money is shortened, and a whole tenth drops its .0") do
    assert_eq(cp_money(940), "$940")
    assert_eq(cp_money(66100), "$66.1K")
    assert_eq(cp_money(50000), "$50K")
    assert_eq(cp_money(1499800), "$1.5M")
  end

  test("cents keep both digits, counts keep their commas") do
    assert_eq(cp_cents(2705), "$27.05")
    assert_eq(cp_cents(19070), "$190.70")
    assert_eq(cp_count(2443), "2,443")
    assert_eq(cp_count(43600000), "43.6M")
  end

  test("a change is always signed") do
    assert_eq(cp_delta(232), "+23.2%")
    assert_eq(cp_delta(-60), "-6%")
    assert_eq(cp_delta(0), "+0%")
    assert_eq(cp_roas(43), "4.3x")
  end
end

describe("Sorting and paging") do
  test("the book opens on spend, largest first, ten to a page") do
    st = cp_state({})
    rows = cp_page_of(st)["rows"]
    assert_eq(rows.length(), 10)
    for i in range(1, rows.length())
      assert(rows[i - 1]["spend"] >= rows[i]["spend"])
    end
  end

  test("a new header sorts names A to Z and figures largest first; again turns it round") do
    by_name = cp_reduce({}, "sort", {"props": {"col": "name"}})
    assert_eq(by_name["dir"], "asc")
    names = cp_page_of(by_name)["rows"].map(fn(c) { c["name"] })
    assert_eq(names[0], "Alpine Fleece Brand Exact")
    by_cpa = cp_reduce(by_name, "sort", {"props": {"col": "cpa_c"}})
    assert_eq(by_cpa["dir"], "desc")
    turned = cp_reduce(by_cpa, "sort", {"props": {"col": "cpa_c"}})
    assert_eq(turned["dir"], "asc")
    rows = cp_page_of(turned)["rows"]
    assert(rows[0]["cpa_c"] <= rows[1]["cpa_c"])
  end

  test("the last page holds the rest, and a page past the end is the last") do
    st = cp_reduce({"per": 20}, "page", {"props": {"page": 9}})
    info = cp_page_of(st)
    assert_eq(info["page"], 2)
    assert_eq(info["rows"].length(), 10)
    assert_eq(info["pages"], 2)
  end

  test("rows per page goes back to the first page") do
    st = cp_reduce({"page": 3}, "per", {"props": {"value": "30"}})
    assert_eq(st["per"], 30)
    assert_eq(st["page"], 1)
    assert_eq(cp_page_of(st)["rows"].length(), 30)
  end
end

describe("Search and filter") do
  test("search matches name, id, market and audience, and the figures follow") do
    st = cp_reduce({}, "search", {"payload": "glacier"})
    seen = cp_in_view(st)
    assert_eq(seen.filter(fn(c) { c["name"].index_of("Glacier") < 0 }).length(), 0)
    assert_eq(cp_totals(seen)["count"], seen.length())
    assert_eq(cp_in_view(cp_reduce({}, "search", {"payload": "CMP-1007"})).length(), 1)
    assert_eq(cp_in_view(cp_reduce({}, "search", {"payload": "nothing like this"})).length(), 0)
  end

  test("the status filter keeps only that status") do
    st = cp_reduce({}, "filter", {"props": {"value": "Limited"}})
    seen = cp_in_view(st)
    assert(seen.length() > 0)
    assert_eq(seen.filter(fn(c) { c["status"] != "Limited" }).length(), 0)
    assert_eq(st["menu"], "")
  end

  test("the totals are the sums of the rows") do
    rows = cp_campaigns()
    tot = cp_totals(rows)
    spend = 0
    below = 0
    for c in rows
      spend = spend + c["spend"]
      below = below + 1 if c["roas_t"] < CP_TARGET_ROAS
    end
    assert_eq(tot["spend"], spend)
    assert_eq(tot["below"], below)
    assert_eq(tot["live"], rows.filter(fn(c) { c["status"] == "Live" }).length())
  end
end

describe("Ticking and pausing") do
  test("the header box ticks the page, then clears it, and leaves other pages alone") do
    st = cp_reduce({}, "tick", {"props": {"id": "CMP-1030"}})
    page_one = cp_spec_ids(st)
    assert(!page_one.includes?("CMP-1030"))
    st = cp_reduce(st, "tick_page", {})
    assert_eq(st["sel"].length(), 11)
    st = cp_reduce(st, "tick_page", {})
    assert_eq(st["sel"], ["CMP-1030"])
  end

  test("pausing from a row's menu pauses it, and resuming brings it back") do
    live = cp_campaigns().filter(fn(c) { c["status"] == "Live" })[0]
    st = cp_reduce({}, "row_action", {"props": {"id": live["id"], "item": "Pause"}})
    assert_eq(cp_find(st, live["id"])["status"], "Paused")
    assert_eq(st["toast"], "Paused " + live["name"])
    assert_eq(cp_totals(cp_in_view(st))["live"], cp_totals(cp_campaigns())["live"] - 1)
    st = cp_reduce(st, "row_action", {"props": {"id": live["id"], "item": "Resume"}})
    assert_eq(cp_find(st, live["id"])["status"], "Live")
  end

  test("pausing what is ticked pauses each of them and clears the ticks") do
    st = cp_reduce({"sel": ["CMP-1002", "CMP-1003"]}, "pause_sel", {})
    assert_eq(st["sel"], [])
    assert_eq(cp_find(st, "CMP-1002")["status"], "Paused")
    assert_eq(cp_find(st, "CMP-1003")["status"], "Paused")
    assert_eq(st["toast"], "Paused 2 campaigns")
  end

  test("a hidden column comes back when it is toggled again") do
    st = cp_reduce({}, "column", {"props": {"col": "trend"}})
    assert_eq(st["hidden"], ["trend"])
    st = cp_reduce(st, "column", {"props": {"col": "trend"}})
    assert_eq(st["hidden"], [])
  end
end
