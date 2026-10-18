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

  # The first example shows its markup and then draws it: the code a reader
  # copies is the source the element below it read, chords and all.
  test("the markup it shows is the markup it draws", fn() {
    let body = res_body(get("/scores"))
    let src = "{ \\clef treble \\time 4/4 <c' e' g'>2 <d' f' a'>2 | <e' g' b'>2 <c' e' g' c''>2 }"
    assert_contains(body, "<script type=\"text/x-lilypond\">" + src + "</script>")
    assert_contains(body, html_escape(src))
  })

  # Protocol 11's LilyPond, as a file is written: the page shows the
  # source and the element below reads the same.
  test("shows LilyPond as a file is written, and draws it", fn() {
    let body = res_body(get("/scores"))
    assert_contains(body, "\\new ChordNames \\harmony")
    assert_contains(body, "\\new Lyrics \\lyricsto \"tune\" \\words")
    assert_contains(body, "\\new TabStaff \\melody")
    assert_contains(body, "\\drums {")
  })

  # One example a construct protocol 11 reads, each drawn and its source
  # shown under it.
  test("shows everything it reads, each with its source", fn() {
    let body = res_body(get("/scores"))
    for id in ["voices", "pitch", "grace", "clefs", "marks", "ornaments", "repeats", "chords", "drums", "tab"]
      assert_contains(body, "<h3 id=\"" + id + "\">")
      assert_contains(body, "href=\"#" + id + "\"")
    end
    assert_contains(body, "\\acciaccatura e8 d4")
    assert_contains(body, html_escape("\\new ChordNames \\harmony \\new Staff \\harmony"))
    assert_eq(body.split("<details class=\"scores__source\">").length, 11)
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
