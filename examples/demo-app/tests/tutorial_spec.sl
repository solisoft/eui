# The tutorial's excerpts against the component they excerpt.
#
#   soli test tests/tutorial_spec.sl --no-coverage
#
# `doc/docs/eui/tutorial.md` shows this application's `tutorial_controller.sl`
# a step at a time, and a reader copies the excerpts in order. An excerpt that
# calls a helper the page never shows is code that does not run: step 2 called
# `pantry_heading` for as long as the page existed, and nothing said so. So
# every `pantry_*` the page calls must be defined on the page, and every one it
# defines must exist here, under the same name, so that the two cannot part.

TUTORIAL_DOC = "../../doc/docs/eui/tutorial.md"
TUTORIAL_SOURCE = "app/controllers/tutorial_controller.sl"

def tutorial_names(pattern, text, prefix)
  Regex.find_all(pattern, text).map do |it|
    it["match"].replace(prefix, "").replace("(", "")
  end
end

describe("The tutorial page", fn() {
  test("defines every pantry helper its excerpts call", fn() {
    let page = File.read(TUTORIAL_DOC)
    let shown = tutorial_names("def pantry_[a-z0-9_]+", page, "def ")
    let called = tutorial_names("pantry_[a-z0-9_]+\\(", page, "")
    let missing = called.filter(&{ |it|
      !shown.includes?(it)
    })
    assert_eq(missing, [])
  })

  test("shows only helpers the component has", fn() {
    let page = File.read(TUTORIAL_DOC)
    let source = File.read(TUTORIAL_SOURCE)
    let shown = tutorial_names("def pantry_[a-z0-9_]+", page, "def ")
    let defined = tutorial_names("def pantry_[a-z0-9_]+", source, "def ")
    let invented = shown.filter(&{ |it|
      !defined.includes?(it)
    })
    assert_eq(invented, [])
    assert(shown.length() > 20)
  })
})
