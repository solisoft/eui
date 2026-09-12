# Meridian — the Files section: its view.
#
# The builders only. State, queries and the reducer are in
# app/services/erp_files_model.sl, which is where the spec can reach them.
#
# Two moments, not one (03 §3.2). `file_pick` is the person having chosen —
# a name and a weight, never a path — and it is what puts the placeholder on
# the screen. `file_upload` is the bytes having landed in the session spool.

# ---- the view -------------------------------------------------------------

def erp_files_section(state, lay)
  files = erp_files_visible(state)
  body = files.length() == 0 ? erp_files_empty(state) : erp_files_grid(state, files, lay)
  parts = [erp_files_bar(state, lay, files.length())]
  parts = parts.concat([erp_files_note(state)]) unless (state["file_trouble"] ?? "").blank?
  parts = parts.concat([
    lay["wide"]
      ? row({"gap": 4, "width": "100%", "align": "start"}, [erp_files_rail(state), body])
      : column({"gap": 3, "width": "100%"}, [erp_files_rail(state), body])
  ])
  column({"gap": lay["wide"] ? 5 : 3, "width": "100%"}, parts)
end

# The bar: where you are, what you are looking at, and the one control that
# can add something.
def erp_files_bar(state, lay, shown)
  crumbs = erp_files_crumbs(state)
  left = column({"gap": 1}, [
    breadcrumb(crumbs, "file_nav"),
    muted(str(shown) + (shown == 1 ? " file" : " files"))
  ])
  column({"gap": 2, "width": "100%"}, [
    row({"gap": 3, "align": "center", "width": "100%"},
        [left, spacer(), erp_files_add(state)]),
    erp_files_drop(state, lay)
  ])
end

# The picker, and the whole of what makes it one: `pick` names what the
# dialog accepts and how many, and the node carries a *server* handler for
# `file_pick`. Neither alone opens anything, and a tree that merely arrives
# opens nothing either — the person has to activate it (03 §3.2).
def erp_files_add(state)
  busy = !(state["attaching"] ?? "").blank?
  resting = {
    "display": "row", "gap": 2, "align": "center", "justify": "center",
    "pad": [2, 4, 2, 4], "radius": 2, "cursor": busy ? "default" : "pointer",
    "bg": "accent.base", "fg": "accent.on", "transition": "fast"
  }
  built = {
    "k": "box",
    "s": resting,
    "p": {"pick": [ERP_FILES_ACCEPT, ERP_FILES_PICK_FLAGS, ERP_FILES_MAX]},
    "c": [text(busy ? "Adding " + state["attaching"] + "…" : "Add files", {"weight": "semibold"})]
  }
  built["on"] = stateful(resting, {
    "hover": {"bg": "accent.hover"},
    "press": {"bg": "accent.active"}
  }, {"file_pick": "file_pick"})
  keyed("erp_files_add", built)
end

# The drop target. `drop` is the same prop as `pick` and answers the same
# `file_pick`, so a file arrives identically whether it was chosen in the
# dialog or let go over this box (03 §3.2). `pick` rides along so that
# clicking it opens the dialog too — one box, both gestures.
#
# `file_drag` is only a report: it says a file is over the box, so the box
# can show it would take it. It arrives when the node under the file
# changes, not once a frame.
def erp_files_drop(state, lay)
  over = state["file_over"] ?? false
  resting = {
    "display": "column", "gap": 1, "align": "center", "justify": "center",
    "width": "100%", "pad": 5, "radius": 3,
    "border": 1,
    "border_color": over ? "accent.base" : "border.subtle",
    "bg": over ? "accent.hover" : "surface.sunken",
    "transition": "fast"
  }
  built = {
    "k": "box",
    "s": resting,
    "p": {
      "drop": [ERP_FILES_ACCEPT, ERP_FILES_PICK_FLAGS, ERP_FILES_MAX],
      "pick": [ERP_FILES_ACCEPT, ERP_FILES_PICK_FLAGS, ERP_FILES_MAX]
    },
    "c": [
      text(over ? "Let go to add them" : "Drop files here", {"weight": "semibold"}),
      muted(lay["wide"] ? "or use Add files · " + erp_files_size(ERP_FILES_MAX) + " each" : "or use Add files")
    ]
  }
  built["on"] = {"file_pick": "file_pick", "file_drag": "file_drag"}
  keyed("erp_files_drop", built)
end

# Why a file did not arrive. A refusal the person can read beats a dialog
# that closes and leaves nothing behind.
def erp_files_note(state)
  banner(state["file_trouble"], "danger", "Dismiss", "file_trouble_clear")
end

def erp_files_rail(state)
  nodes = erp_files_tree_nodes(state, "")
  restyle(
    card({"gap": 2, "width": 220, "shrink": 0}, [
      muted("FOLDERS"),
      tree_view(nodes, state["folder_open"] ?? ["all"], "folder_toggle", 0)
    ]),
    {"width": 220}
  )
end

def erp_files_grid(state, files, lay)
  per = lay["wide"] ? 4 : 2
  basis = str(int(100 / per)) + "%"
  column({"gap": 3, "grow": 1}, [
    row({"gap": 3, "wrap": "wrap", "width": "100%"},
        files.map(fn(f) { tile(basis, erp_files_card(state, f)) }))
  ])
end

def erp_files_card(state, f)
  selected = (state["file_sel"] ?? "") == f["id"]
  resting = {
    "display": "column", "gap": 0, "width": "100%", "radius": 3,
    "border": 1, "border_color": selected ? "accent.base" : "border.subtle",
    "bg": "surface.raised", "cursor": "pointer", "transition": "fast", "overflow": "clip"
  }
  built = {
    "k": "box",
    "s": resting,
    "p": {"id": f["id"]},
    "c": [
      erp_files_preview(f),
      column({"gap": 1, "pad": 3}, [
        text(f["name"], {"weight": "semibold", "clamp": 1}),
        row({"gap": 2, "align": "center"}, [
          badge(erp_files_kind(f["name"]), f["real"] ? "success" : "info"),
          muted(erp_files_size(f["size"])),
          spacer(),
          muted(f["at"])
        ])
      ]),
      erp_files_actions(f)
    ]
  }
  built["on"] = stateful(resting, {"hover": {"border_color": "accent.base"}}, {"click": "file_select"})
  keyed("erp_file_" + f["id"], built)
end

# A picture shows itself; anything else shows what it is. A seeded row has
# no bytes and therefore no thumbnail, which is the honest picture of it.
def erp_files_preview(f)
  thumb = f["thumb"].to_s
  inner = thumb.blank?
    ? column({"align": "center", "justify": "center", "grow": 1},
             [text(erp_files_kind(f["name"]), {"size": 2, "fg": "text.muted"})])
    : image(thumb, "100%", 120)
  node("box", {
    "display": "column", "width": "100%", "height": 120,
    "align": "center", "justify": "center",
    "bg": "surface.sunken", "overflow": "clip"
  }, [inner])
end

def erp_files_actions(f)
  row({"gap": 2, "pad": [0, 3, 3, 3], "align": "center"}, [
    erp_files_save(f),
    spacer(),
    ghost_button("Remove", "file_remove")
  ])
end

# `save` opens the platform's save dialog and the person choosing a place
# gives one `file_save` event. A seeded row has nothing to write, so it
# carries no prop and says so when pressed.
def erp_files_save(f)
  return ghost_button("Download", "file_nobytes") unless f["real"]

  resting = {
    "display": "row", "align": "center", "pad": [1, 3, 1, 3], "radius": 2,
    "cursor": "pointer", "fg": "accent.base", "transition": "fast"
  }
  built = {
    "k": "box",
    "s": resting,
    "p": {"save": f["name"], "id": f["id"]},
    "c": [text("Download", {"weight": "semibold"})]
  }
  built["on"] = stateful(resting, {"hover": {"bg": "surface.sunken"}}, {"file_save": "file_save"})
  keyed("erp_files_save_" + f["id"], built)
end

def erp_files_empty(state)
  restyle(
    card({"grow": 1}, [
      empty_state(
        "Nothing in " + erp_files_folder_name(state["folder"] ?? "all"),
        "Drop files onto the box above, or pick them from your machine.",
        "Add files",
        "file_pick_hint"
      )
    ]),
    {"grow": 1}
  )
end

