# The documentation and the specification, served here rather than handed
# to a file browser on github.com.
#
# What is worth asserting is not that Markdown renders — it does — but the
# three things that would quietly stop being true: that every page named on
# the rail actually resolves, that a slug is a key and not a path, and that
# the site no longer sends a reader to GitHub to read its own spec.
describe("Docs", fn() {
  test("the two entry points land on a page", fn() {
    assert_eq(res_status(get("/docs")), 302)
    assert_eq(res_status(get("/spec")), 302)
    assert_eq(res_status(get("/docs/overview")), 200)
    assert_eq(res_status(get("/spec/00-rationale")), 200)
  })

  test("every page on the rail resolves", fn() {
    # The rail is built from the same list the lookup uses, so a row added
    # to one and not the other is the failure this catches — a link in the
    # sidebar that 404s.
    let body = res_body(get("/docs/overview"))
    let slugs = [
      "docs/status",
      "docs/wire-format",
      "docs/transport",
      "docs/events",
      "docs/views",
      "docs/components",
      "docs/widgets",
      "docs/theming",
      "docs/tailwind",
      "docs/clients",
      "docs/security",
      "docs/budgets",
      "spec/01-transport",
      "spec/02-wire-format",
      "spec/03-widgets",
      "spec/04-layout",
      "spec/05-theme",
      "spec/06-events",
      "spec/07-bytecode",
      "spec/08-security",
      "spec/09-conformance",
      "spec/10-budgets",
      "spec/11-shaders"
    ]
    for slug in slugs
      assert_contains(body, "href=\"/" + slug + "\"")
      assert_eq(res_status(get("/" + slug)), 200)
    end
  })

  test("the pages carry their own text, not a directory listing", fn() {
    # One line from each side, chosen because it is normative and would not
    # survive the page being served from the wrong file.
    assert_contains(res_body(get("/spec/01-transport")), "/.well-known/eui")
    assert_contains(res_body(get("/spec/05-theme")), "space")
    assert_contains(res_body(get("/docs/overview")), "EUI")
    # Rendered, not escaped: a page showing its own Markdown source would
    # pass every assertion above.
    assert_contains(res_body(get("/spec/01-transport")), "<table>")
  })

  test("a slug is a key, not a path", fn() {
    # An unknown name is a lookup miss and 404s before anything reaches the
    # filesystem, so traversal is not a case to defend against.
    assert_eq(res_status(get("/docs/nope")), 404)
    assert_eq(res_status(get("/spec/99-nothing")), 404)
    # The Soli docs vendored under www/docs/ are not on the rail and are
    # therefore unreachable, which is the whole point of the whitelist.
    assert_eq(res_status(get("/docs/routing")), 404)
  })

  test("the masthead has one entry for the docs and the spec, and marks it in both", fn() {
    # The specification is a section of the documentation's contents, so the
    # masthead stopped carrying an entry of its own for it.
    let marked = "href=\"/docs\" aria-current=\"page\">Docs</a>"
    assert_contains(res_body(get("/docs/overview")), marked)
    assert_contains(res_body(get("/spec/01-transport")), marked)
    assert_not(res_body(get("/docs/overview")).includes?(">Spec</a>"))
  })

  test("a page the contents do not have is answered inside the docs", fn() {
    let response = get("/docs/wire")
    assert_eq(res_status(response), 404)
    let body = res_body(response)
    # The rail, which lists every page there is, and the pages whose address
    # shares a word with the one asked for.
    assert_contains(body, "class=\"docs-rail\"")
    assert_contains(body, "No such page")
    assert_contains(body, "href=\"/docs/wire-format\"")
    assert_contains(body, "href=\"/spec/02-wire-format\"")
    # The name asked for is shown as it was asked for, and never as markup:
    # the segment arrives undecoded and is escaped on its way out besides.
    let hostile = res_body(get("/docs/%3Cq9%3E"))
    assert_not(hostile.includes?("<q9>"))
    assert_contains(hostile, "<code>%3Cq9%3E</code>")
  })

  test("an address the site does not route gets the site's own 404", fn() {
    let response = get("/an/address/nobody/serves")
    assert_eq(res_status(response), 404)
    let body = res_body(response)
    assert_contains(body, "Nothing is served at this address.")
    assert_contains(body, "href=\"/docs/tutorial\"")
  })

  test("the site no longer sends a reader to GitHub for its own documentation", fn() {
    # Every page carries the masthead, so one page proves it for all of them.
    let body = res_body(get("/"))
    assert_contains(body, "href=\"/docs\"")
    assert_contains(body, "href=\"/spec\"")
    assert_not(body.includes?("github.com/solisoft/eui/tree/main/spec"))
    assert_not(body.includes?("github.com/solisoft/eui/tree/main/doc/docs/eui"))
  })
})
