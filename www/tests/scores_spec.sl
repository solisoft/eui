# The scores page draws everything with a script, after the HTML has rendered
# 200 — so what is worth asserting is that the script it names is stamped
# with a real build, that every file the element fetches is served, and that
# the music is in the page for it to read.
describe("Scores", fn() {
  test("serves the page", fn() { assert_eq(res_status(get("/scores")), 200) })

  test("holds the music the element engraves", fn() {
    let body = res_body(get("/scores"))
    assert_contains(body, "<eui-score>")
    assert_contains(body, "type=\"text/x-lilypond\"")
    assert_contains(body, "src=\"/eui-score/beethoven.ly\"")
  })

  # The module is served `immutable`; "none" is what the controller answers
  # when nothing has been built, which is a page that cannot draw.
  test("stamps the build of the element it will load", fn() {
    let body = res_body(get("/scores"))
    assert_contains(body, "/eui-score/eui-score.js?v=")
    assert_eq(body.index_of("eui-score.js?v=none"), -1)
  })

  # Every one of these is fetched by the element, after the page has rendered.
  test("the element it names is actually served", fn() {
    for f in ["eui-score.js", "eui_score_web.js", "eui_score_web_bg.wasm", "beethoven.ly", "manifest.json"]
      assert_eq(res_status(get("/eui-score/" + f)), 200)
    end
  })

  test("is reachable from every other page", fn() { assert_contains(res_body(get("/")), "href=\"/scores\"") })
})
