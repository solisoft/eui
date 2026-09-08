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
})
