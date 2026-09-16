# The public page is static, so there is exactly one thing worth asserting:
# that it is served, and that the numbers on it are the ones this repository
# measures. A marketing page that drifts from its own test suite is worse
# than no page at all.
describe("Home", fn() {
  test("serves the page", fn() {
    let response = get("/")
    assert_eq(res_status(response), 200)
  })

  test("carries the measured numbers, not rounded ones", fn() {
    let body = res_body(get("/"))
    # crates/eui-proto/tests/size_budget.rs
    assert_contains(body, "4 619 B")
    assert_contains(body, "14 362 B")
    assert_contains(body, "22 bytes on the wire")
    assert_contains(body, "201 B")
    # The hero's bytes are the real encoding of that one-cell update.
    assert_contains(body, "8d 01")
    assert_contains(body, "31 20 39 38 34 2c 20 34 32 20 e2 82 ac")
  })

  test("says what is not built", fn() {
    let body = res_body(get("/"))
    assert_contains(body, "Not started")
  })

  test("links the component reference", fn() { assert_contains(res_body(get("/")), "href=\"/components\"") })
})

# The reference page is generated from the library it documents, so the test
# pins the counts: if a builder is added and the page is not, this fails.
describe("Components", fn() {
  test("serves the page", fn() {
    let response = get("/components")
    assert_eq(res_status(response), 200)
  })

  test("carries the whole vocabulary", fn() {
    let body = res_body(get("/components"))
    # examples/demo-app/app/controllers/eui_builders*.sl. The heading is the
    # sum of the per-section counts beside it, so the two move together or
    # the page contradicts itself; the meter builders took it to 229.
    assert_contains(body, "All 229")
    # The counted claims, checked against what they count rather than against
    # what they used to say: seventeen node kinds is `spec/03-widgets.md` §1
    # and `NodeKind` beside it, thirty-three colour roles is
    # `spec/05-theme.md`, and the style record's keys are `spec/02` §3. The
    # page had been swept and this file had not, so it asserted a vocabulary
    # three sweeps old and the site could not deploy.
    assert_contains(body, "The seventeen kinds")
    assert_contains(body, "Thirty-nine keys")
    assert_contains(body, "The thirty-three roles")
    assert_contains(body, "time_update")
    assert_contains(body, "list_window(style, item_height, count, heights, children, on_window)")
    assert_contains(body, "data_grid(columns, rows, selected, editing, sort, on_select, on_sort, on_change, on_key)")
    assert_contains(body, "bp(width)")
    assert_contains(body, "bp_min(w, \"md\")")
    # Every entry carries a call, and a call has to be a real one.
    assert_contains(body, "list_window({\"grow\": 1}, 128, count, heights, cards, \"window\")")
    # And a drawing of what it makes.
    assert_contains(body, "class=\"p\"")
    assert_contains(body, "class=\"p val\"")
  })
})
