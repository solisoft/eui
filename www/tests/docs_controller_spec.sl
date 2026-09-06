# End-to-end spec for DocsController.
#
# The slug is a whitelist key, never a path fragment, so the interesting cases
# are: every listed page renders, an unlisted slug is a 404 rather than a file
# read, and a traversal attempt is just another unlisted slug.

describe("DocsController") do
  describe("GET /docs") do
    test("redirects to the overview") do
      response = get("/docs")
      assert_eq(res_status(response), 302)
    end
  end

  describe("GET /docs/:page") do
    test("renders overview") do
      response = get("/docs/overview")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("renders status") do
      response = get("/docs/status")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("renders wire-format") do
      response = get("/docs/wire-format")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("renders transport") do
      response = get("/docs/transport")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("renders events") do
      response = get("/docs/events")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("renders views") do
      response = get("/docs/views")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("renders widgets") do
      response = get("/docs/widgets")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("renders theming") do
      response = get("/docs/theming")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("renders security") do
      response = get("/docs/security")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("renders budgets") do
      response = get("/docs/budgets")
      assert_eq(res_status(response), 200)
      assert_eq(view_path(), "docs/show.html")
    end

    test("marks the current page in the rail") do
      response = get("/docs/theming")
      assert_contains(res_body(response), "href=\"/docs/theming\" aria-current=\"page\"")
    end

    test("404s an unlisted slug without touching the filesystem") do
      response = get("/docs/no-such-page")
      assert_eq(res_status(response), 404)
    end

    test("treats a traversal attempt as an unlisted slug") do
      response = get("/docs/..%2F..%2F.env")
      assert_eq(res_status(response), 404)
    end
  end
end

describe("HomeController") do
  test("renders the home page with the EUI layout") do
    response = get("/")
    assert_eq(res_status(response), 200)
    assert_contains(res_body(response), "A user interface, in 150 bytes.")
  end
end
