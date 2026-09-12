# Meridian's Files section.
#
#   soli test tests/files_spec.sl --no-coverage
#
# What is worth testing here is what a screenshot cannot show you is wrong.
# A seeded row that claims to be real would offer a download of nothing. A
# folder count that disagrees with the folder shows the right number of
# files under the wrong name. And the two-step arrival of a file — `file_pick`
# then `file_upload` — has a failure branch that only ever runs when the
# spool has already gone, which is precisely when nobody is watching.

describe("Meridian's Files section") do

  test("a size is bytes, then kB to the nearest, then MB with one decimal") do
    assert_eq(erp_files_size(0), "0 B")
    assert_eq(erp_files_size(812), "812 B")
    assert_eq(erp_files_size(34000), "34 kB")
    assert_eq(erp_files_size(2400000), "2.4 MB")
  end

  test("a kind is the extension, upper-cased, and pictures say IMG") do
    assert_eq(erp_files_kind("plan.pdf"), "PDF")
    assert_eq(erp_files_kind("notes.md"), "MD")
    assert_eq(erp_files_kind("shot.JPG"), "IMG")
    assert_eq(erp_files_kind("README"), "FILE")
  end

  test("only a picture is a picture") do
    assert_eq(erp_files_picture?("a.png"), true)
    assert_eq(erp_files_picture?("a.webp"), true)
    assert_eq(erp_files_picture?("a.pdf"), false)
    assert_eq(erp_files_picture?("a"), false)
  end

  test("every seeded row is marked as having no bytes") do
    # The download button consults this, and a row that lied about it would
    # open a save dialog over nothing.
    for f in erp_files_seed()
      assert_eq(f["real"], false)
      assert_eq(f["path"], "")
    end
  end

  test("All files shows everything, a folder shows its own") do
    state = erp_files_defaults()
    assert_eq(erp_files_visible(state).length(), 5)

    state["folder"] = "drawings"
    assert_eq(erp_files_visible(state).length(), 2)

    state["folder"] = "contracts"
    assert_eq(erp_files_visible(state).length(), 1)
  end

  test("a folder's count matches what the folder shows") do
    state = erp_files_defaults()
    for folder in ERP_FILES_FOLDERS
      state["folder"] = folder["id"]
      assert_eq(erp_files_count(state, folder["id"]), erp_files_visible(state).length())
    end
  end

  test("the crumbs are the root alone, or the root and where you are") do
    state = erp_files_defaults()
    assert_eq(erp_files_crumbs(state).length(), 1)

    state["folder"] = "invoices"
    crumbs = erp_files_crumbs(state)
    assert_eq(crumbs.length(), 2)
    assert_eq(crumbs[1]["label"], "Invoices")
  end

  test("picking announces a name and clears the last trouble") do
    state = erp_files_defaults()
    state["file_trouble"] = "something earlier went wrong"
    after = erp_files_event(state, "file_pick", {"payload": ["u1", "plan.pdf", 4096]}, {})
    assert_eq(after["attaching"], "plan.pdf")
    assert_eq(after["file_trouble"], "")
  end

  test("an upload that failed says so and adds nothing") do
    state = erp_files_defaults()
    before = state["files"].length()
    after = erp_files_event(
      state, "file_upload",
      {"payload": {"error": "too large", "name": "huge.zip"}}, {}
    )
    assert_eq(after["files"].length(), before)
    assert_eq(after["attaching"], "")
    assert_eq(after["file_trouble"].includes?("huge.zip"), true)
  end

  test("an upload with no file behind it is refused, not half-kept") do
    # `erp_files_keep` is the only thing that touches the disk; handed a
    # path that was never there it must answer with an error rather than
    # put a row on screen pointing at nothing.
    kept = erp_files_keep("", "ghost.png")
    assert_eq(kept["error"].blank?, false)

    kept_missing = erp_files_keep("/tmp/definitely-not-here-38271.png", "ghost.png")
    assert_eq(kept_missing["error"].blank?, false)
  end

  test("selecting, navigating and removing move the state and nothing else") do
    state = erp_files_defaults()

    picked = erp_files_event(state, "file_select", {}, {"id": "f3"})
    assert_eq(picked["file_sel"], "f3")

    moved = erp_files_event(picked, "file_nav", {}, {"id": "invoices"})
    assert_eq(moved["folder"], "invoices")

    gone = erp_files_event(moved, "file_remove", {}, {"id": "f3"})
    assert_eq(gone["files"].length(), 4)
    assert_eq(gone["file_sel"], "")
    assert_eq(gone["files"].filter(fn(f) { f["id"] == "f3" }).length(), 0)
  end

  test("a drag over the box is a flag, and letting go clears it") do
    state = erp_files_defaults()
    over = erp_files_event(state, "file_drag", {"payload": [true]}, {})
    assert_eq(over["file_over"], true)

    left = erp_files_event(over, "file_drag", {"payload": [false]}, {})
    assert_eq(left["file_over"], false)

    # A pick that follows a drag must not leave the box lit.
    dropped = erp_files_event(over, "file_pick", {"payload": ["u2", "a.png", 12]}, {})
    assert_eq(dropped["file_over"], false)
  end

  test("the section owns its events and claims no others") do
    assert_eq(erp_files_owns?("file_pick"), true)
    assert_eq(erp_files_owns?("file_upload"), true)
    assert_eq(erp_files_owns?("file_drag"), true)
    assert_eq(erp_files_owns?("nav"), false)
    assert_eq(erp_files_owns?("kan_drop"), false)
  end

  test("the tree is the folders, nested, each carrying its count") do
    state = erp_files_defaults()
    roots = erp_files_tree_nodes(state, "")
    assert_eq(roots.length(), 1)
    assert_eq(roots[0]["id"], "all")
    assert_eq(roots[0]["children"].length(), 3)
  end

  # The builders themselves — `erp_files_section` and the two nodes that
  # carry `pick` and `drop` — cannot be reached from here: they live in
  # app/controllers, which a spec does not load, and they call the catalogue
  # in eui_builders.sl, which does not either. `split_spec.sl` copies its
  # subject in to get around that; this file will not, because a copy drifts.
  # They are checked by rendering the component instead — see
  # `snapshot --soli`, which mounts it for real and draws it off-screen.
end
