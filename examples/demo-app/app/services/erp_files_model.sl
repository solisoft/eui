# Meridian — the Files section: its model.
#
# The pure half — state, queries, formatting, the reducer, and the one
# function that touches the disk. It lives here and not in app/controllers
# because a spec loads app/services and app/models and not the controllers,
# and a model that cannot be tested is a model that drifts. The builders
# that draw it are in app/controllers/erp_files.sl.
#
# The one part of this application where something real happens. Everything
# else on these six screens is generated from an index and stored nowhere; a
# file manager that could not take a file would be a picture of one, so the
# picker here opens the platform's dialog and the bytes that come back are
# kept on disk and drawn from it.
#
# The seeded rows are not. They are there because an empty file manager
# photographs badly and this application exists to be photographed, and they
# answer an action the way the rest of Meridian does — with a toast saying
# the thing is not in a demonstration. A row with no bytes never pretends to
# have any.
#
# Two moments, not one (03 §3.2). `file_pick` is the person having chosen:
# a name and a weight, never a path, and it is what puts the placeholder on
# the screen. `file_upload` is the bytes having landed in the session spool,
# and it is the server's own event. A file that dies with the socket is no
# use to a file manager, so `erp_files_keep` copies it out of the spool.

# Where a kept file goes. The only line in this file that touches the disk.
ERP_FILES_DIR = "public/files"

# The edge of a thumbnail, in pixels. A card draws a square of about this
# size; sending the original instead would put a megabyte on the wire to
# fill a postage stamp, and — because the box clips — would show its
# top-left corner magnified rather than the picture.
ERP_FILES_THUMB_PX = 160

# What the dialog accepts, and the ceiling on one file. The ceiling is the
# protocol's own (10-budgets §5) unless a smaller one is asked for; naming
# it here is what lets the application say *why* a file was refused instead
# of leaving the person watching nothing happen.
ERP_FILES_ACCEPT = "png,jpg,jpeg,gif,webp,pdf,txt,md,csv,log,json,zip"

ERP_FILES_MAX = 20000000

# Bit 0 of `flags` asks for more than one file (03 §3.2). Bits 1 and 2 ask
# for the camera and a recording instead, and would need their own grants —
# this is a file manager, so neither.
ERP_FILES_PICK_FLAGS = 1

ERP_FILES_PICTURES = ["png", "jpg", "jpeg", "gif", "webp"]

# The folders. Fixed: a tree the person can also build is a second feature,
# and the point here is what a file does, not where it can be put.
ERP_FILES_FOLDERS = [
  {"id": "all", "name": "All files", "parent": ""},
  {"id": "drawings", "name": "Drawings", "parent": "all"},
  {"id": "invoices", "name": "Invoices", "parent": "all"},
  {"id": "contracts", "name": "Contracts", "parent": "all"}
]

def erp_files_defaults
  {
    "files": erp_files_seed(),
    "folder": "all",
    "folder_open": ["all"],
    "file_sel": "",
    "attaching": "",
    "file_trouble": "",
    "file_gone": ""
  }
end

# Five rows so the screen reads as a file manager at rest. `real` is false,
# and every action consults it.
def erp_files_seed
  [
    {"id": "f1", "name": "DWG-2231-flange-DN80.pdf", "size": 184320, "folder": "drawings",
     "at": "12 Sep", "real": false, "path": "", "thumb": ""},
    {"id": "f2", "name": "DWG-2240-bracket.pdf", "size": 96112, "folder": "drawings",
     "at": "11 Sep", "real": false, "path": "", "thumb": ""},
    {"id": "f3", "name": "FA-1003-linus-gmbh.pdf", "size": 41980, "folder": "invoices",
     "at": "10 Sep", "real": false, "path": "", "thumb": ""},
    {"id": "f4", "name": "supply-agreement-2026.pdf", "size": 302144, "folder": "contracts",
     "at": "03 Sep", "real": false, "path": "", "thumb": ""},
    {"id": "f5", "name": "warehouse-katowice.csv", "size": 8214, "folder": "all",
     "at": "02 Sep", "real": false, "path": "", "thumb": ""}
  ]
end

# ---- the model ------------------------------------------------------------

def erp_files_visible(state)
  folder = state["folder"] ?? "all"
  files = state["files"] ?? []
  return files if folder == "all"

  files.filter(fn(f) { f["folder"] == folder })
end

def erp_files_count(state, folder)
  return (state["files"] ?? []).length() if folder == "all"

  (state["files"] ?? []).filter(fn(f) { f["folder"] == folder }).length()
end

def erp_files_folder_name(id)
  found = ERP_FILES_FOLDERS.filter(fn(f) { f["id"] == id })
  found.length() > 0 ? found[0]["name"] : "All files"
end

def erp_files_crumbs(state)
  folder = state["folder"] ?? "all"
  return [{"id": "all", "label": "All files"}] if folder == "all"

  [{"id": "all", "label": "All files"}, {"id": folder, "label": erp_files_folder_name(folder)}]
end

# The rail's nodes: the folders, nested, each carrying its own count.
def erp_files_tree_nodes(state, parent)
  kids = ERP_FILES_FOLDERS.filter(fn(f) { f["parent"] == parent })
  kids.map(fn(f) {
    {
      "id": f["id"],
      "label": f["name"] + "  " + str(erp_files_count(state, f["id"])),
      "children": erp_files_tree_nodes(state, f["id"])
    }
  })
end

def erp_files_ext(name)
  parts = name.to_s.split(".")
  parts.length() < 2 ? "" : parts[parts.length() - 1].downcase()
end

def erp_files_kind(name)
  ext = erp_files_ext(name)
  return "IMG" if ERP_FILES_PICTURES.includes?(ext)
  return "FILE" if ext.blank?

  ext.upcase()
end

def erp_files_picture?(name)
  ERP_FILES_PICTURES.includes?(erp_files_ext(name))
end

# Integer arithmetic only: kB to the nearest, MB with one decimal.
def erp_files_size(n)
  bytes = int(n) rescue 0
  return str(bytes) + " B" if bytes < 1000
  return str(int((bytes + 500) / 1000)) + " kB" if bytes < 1000000

  whole = int(bytes / 1000000)
  tenth = int(((bytes - whole * 1000000) + 50000) / 100000)
  if tenth >= 10
    whole = whole + 1
    tenth = 0
  end
  str(whole) + "." + str(tenth) + " MB"
end

# ---- the disk -------------------------------------------------------------

# A file in the spool dies with the socket (01 §6). Copying it out is what
# makes it a file the manager still has on the next event, and that is this
# application's decision rather than the protocol's.
def erp_files_keep(path, name)
  return {"error": "the upload arrived without a file"} if path.blank?
  return {"error": "the upload was gone before it could be kept"} unless File.exists(path)

  mkdir_p(ERP_FILES_DIR)
  leaf = str(DateTime.now().to_unix()) + "-" + name
  kept = ERP_FILES_DIR + "/" + leaf
  File.copy(path, kept)
  return {"error": "the copy into " + ERP_FILES_DIR + " did not land"} unless File.exists(kept)

  {"path": kept, "thumb": erp_files_thumb(kept, name)}
end

def erp_files_thumb(kept, name)
  return "" unless erp_files_picture?(name)

  small = kept + "-thumb.png"
  return small if File.exists(small)

  erp_files_shrink(kept, small) rescue nil
  File.exists(small) ? small : ""
end

def erp_files_shrink(source, target)
  pic = Image.new(source)
  w = pic.width()
  h = pic.height()
  edge = w < h ? w : h
  scale = ERP_FILES_THUMB_PX * 1.0 / edge
  pic.resize(int(w * scale), int(h * scale)).format("png").to_file(target)
end


# The toast, set here rather than through `erp_say` — that one lives in
# app/controllers, which a spec does not load. The two keys are the whole of
# it; if the shell ever reads a third, this is the line to follow.
# `set_key` is the shell's, and lives in app/controllers. Same two lines.
def erp_files_set(state, key, value)
  state[key] = value
  state
end

def erp_files_say(state, message)
  state["toast"] = message
  state["toast_tone"] = "success"
  state
end

# ---- the events -----------------------------------------------------------

ERP_FILES_EVENTS = [
  "file_pick", "file_upload", "file_drag", "file_nav", "folder_toggle",
  "file_select", "file_remove", "file_save", "file_nobytes",
  "file_pick_hint", "file_trouble_clear"
]

def erp_files_owns?(event)
  ERP_FILES_EVENTS.includes?(event)
end

def erp_files_event(state, event, params, props)
  match event {
    "file_pick" => erp_files_on_pick(state, params),
    "file_upload" => erp_files_on_upload(state, params),
    "file_drag" => erp_files_set(state, "file_over", (params["payload"] ?? [false])[0] == true),
    "file_nav" => erp_files_set(state, "folder", props["id"] ?? "all"),
    "folder_toggle" => erp_files_set(state, "folder", props["id"] ?? "all"),
    "file_select" => erp_files_set(state, "file_sel", props["id"] ?? ""),
    "file_remove" => erp_files_remove(state, props),
    "file_save" => erp_files_say(state, "Saved " + (params["payload"] ?? [""])[0].to_s),
    "file_nobytes" => erp_files_say(state, "That one is part of the demonstration — it has no bytes to give."),
    "file_pick_hint" => erp_files_say(state, "Use Add files above, or drop a file on the box."),
    "file_trouble_clear" => erp_files_set(state, "file_trouble", ""),
    _ => state
  }
end

def erp_files_on_pick(state, params)
  payload = params["payload"] ?? []
  state["attaching"] = (payload[1] ?? "").to_s
  state["file_trouble"] = ""
  state["file_over"] = false
  state
end

def erp_files_on_upload(state, params)
  payload = params["payload"] ?? {}
  state["attaching"] = ""
  trouble = payload["error"].to_s
  unless trouble.blank?
    state["file_trouble"] = payload["name"].to_s + " did not arrive: " + trouble
    return state
  end

  name = payload["name"].to_s
  kept = erp_files_keep(payload["path"].to_s, name)
  unless (kept["error"] ?? "").blank?
    state["file_trouble"] = name + " could not be kept: " + kept["error"]
    return state
  end

  folder = state["folder"] ?? "all"
  entry = {
    "id": "u" + str(DateTime.now().to_unix()) + "-" + str((state["files"] ?? []).length()),
    "name": name,
    "size": int(payload["size"] ?? 0),
    "folder": folder,
    "at": "just now",
    "real": true,
    "path": kept["path"],
    "thumb": kept["thumb"]
  }
  state["files"] = [entry].concat(state["files"] ?? [])
  erp_files_say(state, name + " added")
end

def erp_files_remove(state, props)
  id = props["id"] ?? ""
  return state if id.blank?

  kept = (state["files"] ?? []).filter(fn(f) { f["id"] == id })
  state["files"] = (state["files"] ?? []).filter(fn(f) { f["id"] != id })
  state["file_sel"] = "" if (state["file_sel"] ?? "") == id
  return erp_files_say(state, "Removed") if kept.length() == 0

  erp_files_say(state, kept[0]["name"].to_s + " removed")
end
