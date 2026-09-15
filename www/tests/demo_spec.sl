# The demo page is the one page on this site that runs the client rather than
# showing a picture of it, and it is also the one that can fail silently:
# every part of it is fetched by a script after the HTML has already rendered
# 200. So what is worth asserting is that each of those parts is actually
# there to be fetched, and that the still is there for when they are not.
describe("Demo", fn() {
  test("serves the page", fn() { assert_eq(res_status(get("/demo")), 200) })

  test("names the component it opens a session for", fn() {
    let body = res_body(get("/demo"))
    assert_contains(body, "data-eui")
    assert_contains(body, "data-component=\"gallery\"")
  })

  # The module is served `immutable` for a year, so a deploy that replaced it
  # would never be fetched again without this. "none" is what the controller
  # answers when no client has been built, which is a page that cannot run.
  test("stamps the build of the client it will load", fn() {
    let body = res_body(get("/demo"))
    assert_contains(body, "/eui/eui-embed.js?v=")
    assert_eq(body.index_of("eui-embed.js?v=none"), -1)
  })

  # Every one of these is fetched by the script, after the page has rendered.
  # A 404 here is a button that does nothing and says a socket failed.
  test("the client it names is actually served", fn() {
    for f in ["eui_web.js", "eui_web_bg.wasm", "eui-embed.js", "manifest.json"]
      assert_eq(res_status(get("/eui/" + f)), 200)
    end
  })

  # The still is what a reader looks at until the session draws, and what
  # they go on looking at if it never does.
  test("shows a real render until the session draws", fn() {
    let body = res_body(get("/demo"))
    assert_contains(body, "demo__poster")
    for mode in ["light", "dark"]
      assert_contains(body, "/images/demo/gallery-" + mode + ".png")
      assert_eq(res_status(get("/images/demo/gallery-" + mode + ".png")), 200)
    end
  })

  # It is in the nav, or nobody finds it.
  test("is reachable from every other page", fn() { assert_contains(res_body(get("/")), "href=\"/demo\"") })
})
