# What a link to the site unfurls into, and what a search engine is told.
#
# None of it is visible on the page, so none of it is noticed when it
# breaks: a card that names a picture the site no longer serves still
# renders, as a grey box, in every chat the link is pasted into. So each tag
# is asserted where it is made, and every file a tag names is fetched.
describe("Share tags", fn() {
  test("every page carries the full set", fn() {
    for path in ["/", "/samples", "/docs/packaging", "/spec/01-transport"]
      let body = res_body(get(path))
      for tag in [
        "property=\"og:type\"",
        "property=\"og:site_name\" content=\"EUI\"",
        "property=\"og:title\"",
        "property=\"og:description\"",
        "property=\"og:url\" content=\"https://eui.solisoft.net" + path + "\"",
        "property=\"og:image\" content=\"https://eui.solisoft.net/images/share/eui.png",
        "property=\"og:image:secure_url\"",
        "property=\"og:image:width\" content=\"1200\"",
        "property=\"og:image:height\" content=\"630\"",
        "property=\"og:image:alt\" content=\"The Meridian demo dashboard",
        "name=\"twitter:card\" content=\"summary_large_image\"",
        "name=\"twitter:image:alt\"",
        "rel=\"canonical\" href=\"https://eui.solisoft.net" + path + "\"",
        "rel=\"apple-touch-icon\"",
        "rel=\"manifest\"",
        "type=\"application/ld+json\""
      ]
        assert_contains(body, tag)
      end
    end
  })

  test("a page of the docs is an article named after itself and the site", fn() {
    let body = res_body(get("/docs/packaging"))
    assert_contains(body, "<title>Phone packages — EUI</title>")
    assert_contains(body, "property=\"og:type\" content=\"article\"")
    assert_contains(body, "\"@type\":\"TechArticle\"")
    assert_contains(body, "\"headline\":\"Phone packages\"")
    # The site's own pages are not.
    let home = res_body(get("/"))
    assert_contains(home, "property=\"og:type\" content=\"website\"")
    assert_contains(home, "\"@type\":\"WebSite\"")
    assert_contains(home, "<title>EUI — an interface is not a document</title>")
  })

  test("a docs page that does not exist asks not to be indexed, and one that does does not", fn() {
    assert_contains(res_body(get("/docs/nope")), "name=\"robots\" content=\"noindex\"")
    assert_not(res_body(get("/docs/packaging")).includes?("noindex"))
  })

  test("the files the tags name are served", fn() {
    for path in [
      "/images/share/eui.png",
      "/images/icons/eui-180.png",
      "/images/icons/eui-192.png",
      "/images/icons/eui-512.png",
      "/manifest.json"
    ]
      assert_eq(res_status(get(path)), 200)
    end
  })
})
