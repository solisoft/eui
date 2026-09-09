# The samples page is a gallery of real renders. What is worth asserting is
# that every application it claims to show is actually on it, that each
# picture it names is actually in `public/`, and that the numbers under the
# plates are the ones this repository measures. A gallery whose images 404
# is worse than no gallery.
describe("Samples", fn() {
  test("serves the page", fn() { assert_eq(res_status(get("/samples")), 200) })

  test("shows all seven applications", fn() {
    let body = res_body(get("/samples"))
    for name in ["Tracker", "Needle", "Catalogue", "Feed", "Table", "Todo", "Counter"]
      assert_contains(body, name)
    end
  })

  test("names a picture that exists for each one", fn() {
    let body = res_body(get("/samples"))
    for slug in ["tracker", "music", "gallery", "feed", "table", "todo", "counter"]
      assert_contains(body, "/images/samples/" + slug + ".png")
      assert_eq(res_status(get("/images/samples/" + slug + ".png")), 200)
    end
  })

  test("carries the measured numbers, not rounded ones", fn() {
    let body = res_body(get("/samples"))
    # examples/counter-app: one pattern at 22 kHz, 16-bit
    assert_contains(body, "169 344")
    # crates/eui-client/tests/soli_e2e.rs: ten thousand rows, five nodes each
    assert_contains(body, "50 014")
    # spec/10 §2
    assert_contains(body, "&lt; 2 ms")
    # the counter's click, measured in crates/eui-proto/tests/size_budget.rs
    assert_contains(body, "25 B")
  })

  test("is reachable from every page", fn() {
    for path in ["/", "/components", "/samples"]
      assert_contains(res_body(get(path)), "href=\"/samples\"")
    end
  })
})
