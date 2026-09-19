# The markdown editor's document model, checked without a client.
#
# Everything here is pure: markdown in, blocks out, blocks in, markdown out,
# and one function per gesture in between. That is deliberate — the widget
# draws what it is told and holds nothing, so the part that can be wrong in
# a way nobody notices until a document comes back mangled is exactly the
# part a script can check.
#
# The gestures are the ones 03 §3.1 rule 3 makes reportable: `Enter` at the
# end of a block, `Backspace` at its head, an arrow off either end. What
# they do to the list is here; that the client sends them at all is the
# client's own vectors (`crates/eui-client/tests/driver.rs`).
#
# Run with a soli that can see the catalogue:
#
#   rbuild soli eui/examples/demo-app test tests/markdown_editor_spec.sl
#
# The definitions are copied in by `tools/sync_split_spec.py` rather than
# imported, because `app/controllers` is loaded by the server and not by a
# bare script.

# ---- copied from the catalogue, do not edit ----

def md_find(s, mark, at)
  size = s.length()
  msize = mark.length()
  i = at
  while i + msize <= size
    return i if s.substring(i, i + msize) == mark
    i = i + 1
  end
  -1
end

def md_starts(s, prefix)
  return false if s.length() < prefix.length()

  s.substring(0, prefix.length()) == prefix
end

def md_ends(s, suffix)
  return false if s.length() < suffix.length()

  s.substring(s.length() - suffix.length(), s.length()) == suffix
end

def md_int(said)
  mn_at = 0
  mn_out = 0
  while mn_at < said.length() && md_find("0123456789", said.substring(mn_at, mn_at + 1), 0) >= 0
    mn_out = mn_out * 10 + md_find("0123456789", said.substring(mn_at, mn_at + 1), 0)
    mn_at = mn_at + 1
  end
  mn_out
end

def md_src(src)
  return {"asset": src.substring(10, src.length())} if md_starts(src, "eui-asset:")

  src
end

def md_target(inside)
  mt_said = inside.strip()
  mt_note = ""
  mt_cut = md_find(mt_said, " \"", 0)
  if mt_cut >= 0
    mt_note = mt_said.substring(mt_cut + 2, mt_said.length())
    mt_note = mt_note.substring(0, mt_note.length() - 1) if md_ends(mt_note, "\"")
    mt_said = mt_said.substring(0, mt_cut)
  end
  mt_by = md_find(mt_note, "x", 0)
  mt_w = mt_by > 0 ? md_int(mt_note) : 0
  mt_h = mt_by > 0 ? md_int(mt_note.substring(mt_by + 1, mt_note.length())) : 0
  {"src": mt_said, "note": mt_note, "w": mt_w, "h": mt_h}
end

def md_fit(w, h, width)
  mf_w = w
  mf_h = h
  if mf_w < 1 || mf_h < 1
    mf_w = 320
    mf_h = 200
  end
  if mf_w > width
    mf_h = int(mf_h * width / mf_w)
    mf_w = width
  end
  if mf_h > 520
    mf_w = int(mf_w * 520 / mf_h)
    mf_h = 520
  end
  [mf_w < 1 ? 1 : mf_w, mf_h < 1 ? 1 : mf_h]
end

def md_image_of(line)
  return {} unless md_starts(line, "![")

  mi_shut = md_find(line, "](", 2)
  return {} if mi_shut < 0

  mi_fin = md_find(line, ")", mi_shut + 2)
  return {} unless mi_fin == line.length() - 1

  {"t": line.substring(2, mi_shut), "k": "image"}.merge(md_target(line.substring(mi_shut + 2, mi_fin)))
end

def md_file_src?(src)
  return true if md_starts(src, "eui-asset:")
  return false if md_find(src, "://", 0) >= 0
  return false if md_starts(src, "mailto:")
  return false if md_starts(src, "#")

  src != ""
end

def md_file_of(line)
  return {} unless md_starts(line, "[")

  mo_shut = md_find(line, "](", 1)
  return {} if mo_shut < 0

  mo_fin = md_find(line, ")", mo_shut + 2)
  return {} unless mo_fin == line.length() - 1

  mo_aim = md_target(line.substring(mo_shut + 2, mo_fin))
  return {} unless md_file_src?((mo_aim["src"] ?? "").to_s)

  {"t": line.substring(1, mo_shut)}.merge(mo_aim)
end

def md_media?(line)
  return true unless md_image_of(line)["src"].nil?
  return true unless md_file_of(line)["src"].nil?

  false
end

def md_ordered_marker(line)
  i = 0
  size = line.length()
  while i < size && "0123456789".includes?(line.substring(i, i + 1))
    i = i + 1
  end
  return 0 if i == 0
  return 0 unless i + 2 <= line.length() && line.substring(i, i + 2) == ". "

  i + 2
end

def md_edit_mark(kind)
  return "# " if kind == "h1"
  return "## " if kind == "h2"
  return "### " if kind == "h3"
  return "> " if kind == "quote"
  return "- " if kind == "bullet"

  ""
end

def md_edit_kind_of(said)
  return "h3" if md_starts(said, "### ")
  return "h2" if md_starts(said, "## ")
  return "h1" if md_starts(said, "# ")
  # A quote line with nothing on it is `>` and not `> `, which is what a
  # quoted mail is mostly made of: every blank line of the letter being
  # answered comes back as one. Reading it as a paragraph puts a visible
  # ">" in the middle of the quote, once per blank line.
  return "quote" if md_starts(said, "> ") || said == ">"
  # Before the bullet, because a task *is* a bullet with a box on it and
  # reading it as one would leave the box in the text.
  return "task" if md_starts(said, "- [ ] ") || md_starts(said, "- [x] ") || md_starts(said, "- [X] ")
  return "bullet" if md_starts(said, "- ") || md_starts(said, "* ")
  return "number" if md_ordered_marker(said) > 0

  "p"
end

def md_edit_bare(said)
  mb_kind = md_edit_kind_of(said)
  return said.substring(6, said.length()) if mb_kind == "task"
  return "" if said == ">"
  return said.substring(md_ordered_marker(said), said.length()) if mb_kind == "number"
  return said.substring(md_edit_mark(mb_kind).length(), said.length()) unless mb_kind == "p"

  said
end

def md_edit_text?(kind)
  kind != "image" && kind != "file" && kind != "rule" && kind != "table"
end

def md_edit_verbatim?(kind)
  kind == "code" || kind == "image" || kind == "file"
end

def md_edit_read(lines, at)
  mp2_said = lines[at].strip()
  if md_starts(mp2_said, "```")
    mp2_lang = mp2_said.substring(3, mp2_said.length()).strip()
    mp2_body = []
    mp2_to = at + 1
    while mp2_to < lines.length() && !md_starts(lines[mp2_to].strip(), "```")
      mp2_body = mp2_body.concat([lines[mp2_to]])
      mp2_to = mp2_to + 1
    end
    return [{"kind": "code", "t": mp2_body.join("\n"), "lang": mp2_lang}, mp2_to + 1]
  end

  mp2_shot = md_image_of(mp2_said)
  unless mp2_shot["src"].nil?
    return [{
      "kind": "image", "t": mp2_shot["t"] ?? "", "src": mp2_shot["src"],
      "w": mp2_shot["w"] ?? 0, "h": mp2_shot["h"] ?? 0
    }, at + 1]
  end

  mp2_kept = md_file_of(mp2_said)
  unless mp2_kept["src"].nil?
    return [{
      "kind": "file", "t": mp2_kept["t"] ?? "", "src": mp2_kept["src"],
      "note": mp2_kept["note"] ?? ""
    }, at + 1]
  end

  return [{"kind": "rule", "t": ""}, at + 1] if mp2_said == "---" || mp2_said == "***" || mp2_said == "___"

  # A run of lines beginning with a pipe is one block, not one a line: a
  # table is the only thing here whose shape is two-dimensional, and a row
  # of it on its own means nothing.
  if md_starts(mp2_said, "|")
    mp2_rows = []
    mp2_to = at
    while mp2_to < lines.length() && md_starts(lines[mp2_to].strip(), "|")
      mp2_cells = md_table_cells(lines[mp2_to])
      mp2_rows = mp2_rows.concat([mp2_cells]) unless md_edit_ruler?(mp2_cells)
      mp2_to = mp2_to + 1
    end
    return [{"kind": "table", "t": "", "rows": mp2_rows}, mp2_to]
  end

  mp2_kind = md_edit_kind_of(mp2_said)
  mp2_out = {"kind": mp2_kind, "t": md_edit_bare(mp2_said)}
  mp2_out["done"] = md_edit_done?(mp2_said) if mp2_kind == "task"
  [mp2_out, at + 1]
end

def md_edit_parse(source)
  mk_out = []
  mk_lines = source.split("\n")
  mk_at = 0
  while mk_at < mk_lines.length()
    if mk_lines[mk_at].strip() == ""
      mk_at = mk_at + 1
    else
      mk_got = md_edit_read(mk_lines, mk_at)
      mk_out = mk_out.concat([{"id": mk_out.length() + 1}.merge(mk_got[0])])
      mk_at = mk_got[1]
    end
  end
  mk_out.length() == 0 ? [{"id": 1, "kind": "p", "t": ""}] : mk_out
end

def md_edit_title(one)
  mq_w = one["w"] ?? 0
  mq_h = one["h"] ?? 0
  return " \"" + str(mq_w) + "x" + str(mq_h) + "\"" if mq_w > 0 && mq_h > 0

  mq_note = (one["note"] ?? "").to_s
  return " \"" + mq_note + "\"" unless mq_note == ""

  ""
end

def md_edit_line(one, n)
  ml_kind = (one["kind"] ?? "p").to_s
  ml_said = (one["t"] ?? "").to_s
  return "---" if ml_kind == "rule"
  return "```" + (one["lang"] ?? "").to_s + "\n" + ml_said + "\n```" if ml_kind == "code"
  return "![" + ml_said + "](" + (one["src"] ?? "").to_s + md_edit_title(one) + ")" if ml_kind == "image"
  return "[" + ml_said + "](" + (one["src"] ?? "").to_s + md_edit_title(one) + ")" if ml_kind == "file"
  return md_edit_grid(one) if ml_kind == "table"
  return "- [" + (one["done"] == true ? "x" : " ") + "] " + ml_said if ml_kind == "task"
  return str(n) + ". " + ml_said if ml_kind == "number"
  # `">"` and not `"> "`: an empty quote line is what a quoted letter is
  # mostly made of, and a trailing space on every one of them is what a
  # reader shows as a ragged right edge.
  return ">" if ml_kind == "quote" && ml_said == ""

  md_edit_mark(ml_kind) + ml_said
end

def md_edit_source(blocks)
  ms_out = []
  ms_at = 0
  ms_n = 0
  while ms_at < blocks.length()
    ms_kind = (blocks[ms_at]["kind"] ?? "p").to_s
    ms_n = ms_kind == "number" ? ms_n + 1 : 0
    ms_out = ms_out.concat([md_edit_line(blocks[ms_at], ms_n)])
    ms_after = ms_at + 1 < blocks.length() ? (blocks[ms_at + 1]["kind"] ?? "p").to_s : ""
    ms_run = ms_after == ms_kind && (ms_kind == "bullet" || ms_kind == "number" || ms_kind == "task")
    ms_out = ms_out.concat([""]) unless ms_run
    ms_at = ms_at + 1
  end
  ms_out.join("\n").strip()
end

def md_edit_index(blocks, id)
  mz_at = 0
  while mz_at < blocks.length()
    return mz_at if blocks[mz_at]["id"] == id

    mz_at = mz_at + 1
  end
  -1
end

def md_edit_at(blocks, id)
  mz2 = md_edit_index(blocks, id)
  mz2 < 0 ? {} : blocks[mz2]
end

def md_edit_fresh(blocks)
  mj_top = 0
  for mj_one in blocks
    mj_top = mj_one["id"] ?? 0 if (mj_one["id"] ?? 0) > mj_top
  end
  mj_top + 1
end

def md_edit_step(blocks, id, delta)
  mv_at = md_edit_index(blocks, id)
  return id if mv_at < 0

  mv_to = mv_at + delta
  return id if mv_to < 0 || mv_to >= blocks.length()

  blocks[mv_to]["id"]
end

def md_edit_splice(blocks, at, drop, put)
  blocks.slice(0, at).concat(put).concat(blocks.slice(at + drop, blocks.length()))
end

def md_edit_retype(one, id, said)
  mr_was = (one["kind"] ?? "p").to_s
  mr_out = {"id": id, "kind": mr_was, "t": said}
  mr_out["lang"] = one["lang"] unless one["lang"].nil?
  mr_out["src"] = one["src"] unless one["src"].nil?
  mr_out["rows"] = one["rows"] unless one["rows"].nil?
  mr_out["done"] = one["done"] unless one["done"].nil?
  return mr_out unless md_edit_text?(mr_was)

  if md_starts(said, "```")
    mr_out["kind"] = "code"
    mr_out["lang"] = said.substring(3, said.length()).strip()
    mr_out["t"] = ""
    return mr_out
  end

  mr_kind = md_edit_kind_of(said)
  if mr_kind != "p" && mr_kind != mr_was
    mr_out["kind"] = mr_kind
    mr_out["t"] = md_edit_bare(said)
    mr_out["done"] = md_edit_done?(said) if mr_kind == "task"
  end
  mr_out
end

def md_edit_set(blocks, id, said, next_id)
  mw_at = md_edit_index(blocks, id)
  return blocks if mw_at < 0

  mw_one = blocks[mw_at]
  if md_edit_verbatim?((mw_one["kind"] ?? "p").to_s)
    mw_kept = mw_one.merge({"t": said})
    return md_edit_splice(blocks, mw_at, 1, [mw_kept])
  end

  mw_lines = said.split("\n")
  mw_made = []
  mw_n = 0
  while mw_n < mw_lines.length()
    mw_made = mw_made.concat([md_edit_retype(mw_one, mw_n == 0 ? id : next_id + mw_n - 1, mw_lines[mw_n])])
    mw_n = mw_n + 1
  end
  md_edit_splice(blocks, mw_at, 1, mw_made)
end

def md_edit_split(blocks, id, next_id)
  mu_at = md_edit_index(blocks, id)
  return blocks if mu_at < 0

  mu_one = blocks[mu_at]
  mu_kind = (mu_one["kind"] ?? "p").to_s
  mu_list = mu_kind == "bullet" || mu_kind == "number"
  if mu_list && (mu_one["t"] ?? "").to_s == ""
    return md_edit_splice(blocks, mu_at, 1, [mu_one.merge({"kind": "p"})])
  end

  md_edit_splice(blocks, mu_at + 1, 0, [{"id": next_id, "kind": mu_list ? mu_kind : "p", "t": ""}])
end

def md_edit_merge(blocks, id)
  mh_at = md_edit_index(blocks, id)
  return blocks if mh_at < 0

  mh_one = blocks[mh_at]
  mh_kind = (mh_one["kind"] ?? "p").to_s
  return blocks unless md_edit_text?(mh_kind)

  if mh_kind != "p"
    return md_edit_splice(blocks, mh_at, 1, [mh_one.merge({"kind": "p"})])
  end
  return blocks if mh_at < 1

  mh_prev = blocks[mh_at - 1]
  return md_edit_splice(blocks, mh_at - 1, 1, []) unless md_edit_text?((mh_prev["kind"] ?? "p").to_s)

  mh_joined = mh_prev.merge({"t": (mh_prev["t"] ?? "").to_s + (mh_one["t"] ?? "").to_s})
  md_edit_splice(blocks, mh_at - 1, 2, [mh_joined])
end

def md_edit_kind(blocks, id, kind)
  mc_at = md_edit_index(blocks, id)
  return blocks if mc_at < 0

  mc_one = blocks[mc_at]
  return blocks unless md_edit_text?((mc_one["kind"] ?? "p").to_s)

  mc_to = (mc_one["kind"] ?? "p").to_s == kind ? "p" : kind
  md_edit_splice(blocks, mc_at, 1, [mc_one.merge({"kind": mc_to})])
end

def md_edit_wrap(blocks, id, mark)
  my_at = md_edit_index(blocks, id)
  return blocks if my_at < 0

  my_one = blocks[my_at]
  return blocks unless md_edit_text?((my_one["kind"] ?? "p").to_s)

  my_said = (my_one["t"] ?? "").to_s
  return blocks if my_said == ""

  my_wide = mark.length() * 2
  my_on = md_starts(my_said, mark) && md_ends(my_said, mark) && my_said.length() > my_wide
  my_to = my_on ? my_said.substring(mark.length(), my_said.length() - mark.length()) : mark + my_said + mark
  md_edit_splice(blocks, my_at, 1, [my_one.merge({"t": my_to})])
end

def md_edit_done?(said)
  md_starts(said, "- [x] ") || md_starts(said, "- [X] ")
end

def md_edit_grid?(kind)
  kind == "table"
end

def md_edit_ruler?(cells)
  return false if cells.length() == 0

  for mr2_cell in cells
    return false unless md_edit_dashes?(mr2_cell)
  end
  true
end

def md_edit_dashes?(said)
  mr3_at = 0
  return false if said.length() == 0

  while mr3_at < said.length()
    mr3_ch = said.substring(mr3_at, mr3_at + 1)
    return false unless mr3_ch == "-" || mr3_ch == ":" || mr3_ch == " "

    mr3_at = mr3_at + 1
  end
  md_find(said, "-", 0) >= 0
end

def md_table_cells(line)
  trimmed = line.strip()
  cells_wide = trimmed.chars().length()
  if md_starts(trimmed, "|")
    trimmed = trimmed.substring(1, cells_wide)
    cells_wide = cells_wide - 1
  end
  trimmed = trimmed.substring(0, cells_wide - 1) if cells_wide > 0 && trimmed.substring(cells_wide - 1, cells_wide) == "|"
  trimmed.split("|").map(fn(c) { c.strip() })
end

def md_edit_grid(one)
  mg2_rows = one["rows"] ?? []
  return "" if mg2_rows.length() == 0

  mg2_wide = 0
  for mg2_row in mg2_rows
    mg2_wide = mg2_row.length() if mg2_row.length() > mg2_wide
  end
  mg2_out = []
  mg2_at = 0
  while mg2_at < mg2_rows.length()
    mg2_out = mg2_out.concat([md_edit_grid_row(mg2_rows[mg2_at], mg2_wide)])
    mg2_out = mg2_out.concat([md_edit_grid_rule(mg2_wide)]) if mg2_at == 0
    mg2_at = mg2_at + 1
  end
  mg2_out.join("\n")
end

def md_edit_grid_row(cells, wide)
  mg3_out = "|"
  mg3_at = 0
  while mg3_at < wide
    mg3_out = mg3_out + " " + (mg3_at < cells.length() ? cells[mg3_at].to_s : "") + " |"
    mg3_at = mg3_at + 1
  end
  mg3_out
end

def md_edit_grid_rule(wide)
  mg4_out = "|"
  mg4_at = 0
  while mg4_at < wide
    mg4_out = mg4_out + " --- |"
    mg4_at = mg4_at + 1
  end
  mg4_out
end

def md_edit_check(blocks, id)
  mk2_at = md_edit_index(blocks, id)
  return blocks if mk2_at < 0

  mk2_one = blocks[mk2_at]
  return blocks unless (mk2_one["kind"] ?? "p").to_s == "task"

  md_edit_splice(blocks, mk2_at, 1, [mk2_one.merge({"done": mk2_one["done"] != true})])
end

def md_edit_cell(blocks, id, row, col, said)
  mc2_at = md_edit_index(blocks, id)
  return blocks if mc2_at < 0

  mc2_one = blocks[mc2_at]
  mc2_rows = mc2_one["rows"] ?? []
  return blocks if row < 0 || row >= mc2_rows.length()

  mc2_row = mc2_rows[row]
  while mc2_row.length() <= col
    mc2_row = md_edit_splice(mc2_row, mc2_row.length(), 0, [""])
  end
  md_edit_splice(blocks, mc2_at, 1, [mc2_one.merge({
    "rows": md_edit_splice(mc2_rows, row, 1, [md_edit_splice(mc2_row, col, 1, [said])])
  })])
end

def md_edit_cols(one)
  mw2_wide = 0
  for mw2_row in one["rows"] ?? []
    mw2_wide = mw2_row.length() if mw2_row.length() > mw2_wide
  end
  mw2_wide
end

def md_edit_row_add(blocks, id)
  mn2_at = md_edit_index(blocks, id)
  return blocks if mn2_at < 0

  mn2_one = blocks[mn2_at]
  mn2_rows = mn2_one["rows"] ?? []
  mn2_new = []
  mn2_col = 0
  mn2_wide = md_edit_cols(mn2_one)
  while mn2_col < mn2_wide
    mn2_new = mn2_new.concat([""])
    mn2_col = mn2_col + 1
  end
  md_edit_splice(blocks, mn2_at, 1, [mn2_one.merge({"rows": md_edit_splice(mn2_rows, mn2_rows.length(), 0, [mn2_new])})])
end

def md_edit_col_add(blocks, id)
  mo2_at = md_edit_index(blocks, id)
  return blocks if mo2_at < 0

  mo2_one = blocks[mo2_at]
  mo2_wide = md_edit_cols(mo2_one)
  mo2_rows = (mo2_one["rows"] ?? []).map(fn(r) {
    md_edit_splice(r, r.length(), 0, range(r.length(), mo2_wide + 1).map(fn(i) { "" }))
  })
  md_edit_splice(blocks, mo2_at, 1, [mo2_one.merge({"rows": mo2_rows})])
end

def md_edit_table(id)
  # The space after the outer bracket is load-bearing. `[["a", "b"], …]`
  # — an array of arrays whose first element is a *string* — lexes as a
  # string in Soli 2.3.7 and comes out as one, silently:
  #
  #   [[1, 2], [3, 4]]   -> array          [["x", "y"], ["z"]] -> string
  #   [ ["x"], ["y"] ]   -> array          [[], []]            -> string
  #
  # A space, a pair of parentheses or an intermediate variable all avoid it.
  {"id": id, "kind": "table", "t": "", "rows": [ ["", ""], ["", ""] ]}
end

def md_edit_move_to(blocks, id, slot)
  mv2_at = md_edit_index(blocks, id)
  return blocks if mv2_at < 0

  mv2_rest = md_edit_splice(blocks, mv2_at, 1, [])
  mv2_to = slot > mv2_at ? slot - 1 : slot
  mv2_to = 0 if mv2_to < 0
  mv2_to = mv2_rest.length() if mv2_to > mv2_rest.length()
  md_edit_splice(mv2_rest, mv2_to, 0, [blocks[mv2_at]])
end

def md_edit_shift(blocks, id, delta)
  ms2_at = md_edit_index(blocks, id)
  return blocks if ms2_at < 0

  ms2_to = ms2_at + delta
  return blocks if ms2_to < 0 || ms2_to >= blocks.length()

  md_edit_splice(md_edit_splice(blocks, ms2_at, 1, []), ms2_to, 0, [blocks[ms2_at]])
end

def md_edit_remember(doc, why, id, cap = 40)
  md_past = doc["past"] ?? []
  md_top = md_past.length() == 0 ? {} : md_past[md_past.length() - 1]
  md_same = why == "change" && (md_top["why"] ?? "") == "change" && (md_top["id"] ?? -1) == id
  return doc.merge({"future": []}) if md_same

  md_kept = md_edit_splice(md_past, md_past.length(), 0, [{
    "blocks": doc["blocks"] ?? [], "why": why, "id": id, "focus": doc["focus"] ?? 0
  }])
  md_kept = md_edit_splice(md_kept, 0, md_kept.length() - cap, []) if md_kept.length() > cap
  doc.merge({"past": md_kept, "future": []})
end

def md_edit_undo(doc)
  mu2_past = doc["past"] ?? []
  return doc if mu2_past.length() == 0

  mu2_step = mu2_past[mu2_past.length() - 1]
  doc.merge({
    "blocks": mu2_step["blocks"],
    "focus": mu2_step["focus"] ?? 0,
    "take": true,
    "slash": 0,
    "past": md_edit_splice(mu2_past, mu2_past.length() - 1, 1, []),
    "future": md_edit_splice(doc["future"] ?? [], 0, 0, [{
      "blocks": doc["blocks"] ?? [], "why": "undo", "id": 0, "focus": doc["focus"] ?? 0
    }])
  })
end

def md_edit_redo(doc)
  mr4_next = doc["future"] ?? []
  return doc if mr4_next.length() == 0

  mr4_step = mr4_next[0]
  doc.merge({
    "blocks": mr4_step["blocks"],
    "focus": mr4_step["focus"] ?? 0,
    "take": true,
    "slash": 0,
    "future": md_edit_splice(mr4_next, 0, 1, []),
    "past": md_edit_splice(doc["past"] ?? [], (doc["past"] ?? []).length(), 0, [{
      "blocks": doc["blocks"] ?? [], "why": "redo", "id": 0, "focus": doc["focus"] ?? 0
    }])
  })
end

def md_edit_accel?(mods)
  mods == 2 || mods == 8 || mods == 3 || mods == 9
end

def md_edit_tools()
  [
    {"tool": "h1", "glyph": "H1", "label": "Heading"},
    {"tool": "h2", "glyph": "H2", "label": "Subheading"},
    {"tool": "h3", "glyph": "H3", "label": "Sub-subheading"},
    {"tool": "bullet", "glyph": "•", "label": "Bullet list"},
    {"tool": "number", "glyph": "1.", "label": "Numbered list"},
    {"tool": "quote", "glyph": "“", "label": "Quote"},
    {"tool": "task", "icon": "check", "label": "Task list"},
    {"tool": "table", "icon": "grid", "label": "Table"},
    {"tool": "code", "glyph": "</>", "label": "Code"},
    {"tool": "strong", "glyph": "B", "label": "Bold this block"},
    {"tool": "em", "glyph": "I", "label": "Italic this block"},
    {"tool": "rule", "glyph": "—", "label": "Divider"},
    {"tool": "link", "glyph": "Link", "label": "Make this block a link"}
  ]
end

def md_edit_slash_hits(said)
  return [] unless md_starts(said, "/")

  es3_q = said.substring(1, said.length()).strip().downcase()
  return md_edit_tools() if es3_q == ""

  md_edit_tools().filter(fn(t) {
    md_find(t["label"].to_s.downcase(), es3_q, 0) >= 0 || md_find(t["tool"].to_s, es3_q, 0) >= 0
  })
end

def md_edit_walk(at, count, delta)
  return 0 if count <= 0

  ew2_to = at + delta
  return count - 1 if ew2_to < 0
  return 0 if ew2_to >= count

  ew2_to
end

def md_edit_unslash(blocks, id)
  eu2_one = md_edit_at(blocks, id)
  return blocks if eu2_one["id"].nil?
  return blocks unless md_starts((eu2_one["t"] ?? "").to_s, "/")

  md_edit_splice(blocks, md_edit_index(blocks, id), 1, [eu2_one.merge({"t": ""})])
end

def md_edit_pair_wrap(blocks, id, url)
  ew3_at = md_edit_index(blocks, id)
  return blocks if ew3_at < 0

  ew3_one = blocks[ew3_at]
  return blocks unless md_edit_text?((ew3_one["kind"] ?? "p").to_s)

  ew3_said = (ew3_one["t"] ?? "").to_s
  return blocks if ew3_said == ""

  md_edit_splice(blocks, ew3_at, 1, [ew3_one.merge({"t": "[" + ew3_said + "](" + url + ")"})])
end

def md_edit_step_tool(doc, tool, id, next_id)
  et2_blocks = doc["blocks"] ?? []
  et2_blocks = md_edit_unslash(et2_blocks, id)
  return doc.merge({"blocks": md_edit_wrap(et2_blocks, id, "**"), "take": true}) if tool == "strong"
  return doc.merge({"blocks": md_edit_wrap(et2_blocks, id, "*"), "take": true}) if tool == "em"
  if tool == "rule"
    return doc.merge({"blocks": md_edit_put(et2_blocks, id, {"id": next_id, "kind": "rule", "t": ""}), "take": false})
  end
  if tool == "table"
    return doc.merge({"blocks": md_edit_put(et2_blocks, id, md_edit_table(next_id)), "focus": next_id, "take": false})
  end
  if tool == "link"
    return doc.merge({"blocks": et2_blocks, "asking": {"block": id, "url": ""}})
  end

  doc.merge({"blocks": md_edit_kind(et2_blocks, id, tool), "take": true})
end

def md_edit_step_change(doc, params, id, next_id)
  ec2_said = (params["payload"] ?? "").to_s
  # Only a block the panel could act on. A picture's caption and a file's
  # name are fields too, and a slash typed into one of those is a slash.
  ec2_kind = (md_edit_at(doc["blocks"] ?? [], id)["kind"] ?? "p").to_s
  ec2_open = md_edit_text?(ec2_kind) && md_starts(ec2_said, "/") && md_edit_slash_hits(ec2_said).length() > 0
  doc.merge({
    "blocks": md_edit_set(doc["blocks"] ?? [], id, ec2_said, next_id),
    "take": false,
    "slash": ec2_open ? id : 0,
    "at": ec2_open ? 0 : 0
  })
end

def md_edit_step_key(doc, params, id, next_id)
  ek2_said = params["payload"] ?? [""]
  ek2_key = ek2_said[0].to_s
  ek2_mods = ek2_said.length() > 1 ? ek2_said[1] : 0
  ek2_blocks = doc["blocks"] ?? []
  ek2_open = (doc["slash"] ?? 0) == id

  if md_edit_accel?(ek2_mods)
    return md_edit_redo(doc) if ek2_key == "y" || ek2_key == "Z"
    return md_edit_undo(doc) if ek2_key == "z"

    return doc
  end
  # Undo is the only thing a bare letter is wanted for, so every other one
  # goes back where it came from.
  return doc if ek2_key == "z" || ek2_key == "y" || ek2_key == "Z"
  return doc.merge({"slash": 0}) if ek2_key == "Escape"

  if ek2_open
    ek2_hits = md_edit_slash_hits((md_edit_at(ek2_blocks, id)["t"] ?? "").to_s)
    return doc.merge({"at": md_edit_walk(doc["at"] ?? 0, ek2_hits.length(), -1)}) if ek2_key == "ArrowUp"
    return doc.merge({"at": md_edit_walk(doc["at"] ?? 0, ek2_hits.length(), 1)}) if ek2_key == "ArrowDown"
  end

  # Alt and an arrow moves the block instead of the caret.
  if ek2_mods == 4 && (ek2_key == "ArrowUp" || ek2_key == "ArrowDown")
    ek2_by = ek2_key == "ArrowUp" ? -1 : 1
    return md_edit_remember(doc, "key", id).merge({
      "blocks": md_edit_shift(ek2_blocks, id, ek2_by), "take": true
    })
  end

  if ek2_key == "Backspace"
    ek2_joined = md_edit_merge_focus(ek2_blocks, id)
    return md_edit_remember(doc, "key", id).merge({
      "blocks": md_edit_merge(ek2_blocks, id), "focus": ek2_joined, "take": true
    })
  end
  return doc.merge({"focus": md_edit_step(ek2_blocks, id, -1), "take": true}) if ek2_key == "ArrowUp"
  return doc.merge({"focus": md_edit_step(ek2_blocks, id, 1), "take": true}) if ek2_key == "ArrowDown"

  doc
end

def md_edit_step_pick(doc, id)
  ep2_hits = md_edit_slash_hits((md_edit_at(doc["blocks"] ?? [], id)["t"] ?? "").to_s)
  return doc.merge({"slash": 0}) if ep2_hits.length() == 0

  ep2_at = doc["at"] ?? 0
  ep2_at = 0 if ep2_at < 0 || ep2_at >= ep2_hits.length()
  md_edit_step_tool(doc.merge({"slash": 0}), ep2_hits[ep2_at]["tool"].to_s, id, md_edit_fresh(doc["blocks"] ?? []))
end

def md_edit_step_slash(doc, params, props, id)
  es4_tool = (props["tool"] ?? "").to_s
  return doc.merge({"slash": 0}) if es4_tool == ""

  md_edit_step_tool(md_edit_remember(doc.merge({"slash": 0}), "slash", id), es4_tool, id, md_edit_fresh(doc["blocks"] ?? []))
end

def md_edit_step_link(doc, params, props)
  el2_ask = doc["asking"] ?? {}
  el2_id = el2_ask["block"] ?? 0
  return doc.merge({"asking": {}}) unless props["go"] == true

  el2_url = (el2_ask["url"] ?? "").to_s.strip()
  return doc.merge({"asking": {}}) if el2_url == "" || el2_id == 0

  md_edit_remember(doc, "link", el2_id).merge({
    "blocks": md_edit_pair_wrap(doc["blocks"] ?? [], el2_id, el2_url),
    "asking": {},
    "take": true
  })
end

def md_edit_put(blocks, id, one)
  mj_at = md_edit_index(blocks, id)
  return md_edit_splice(blocks, blocks.length(), 0, [one]) if mj_at < 0

  md_edit_splice(blocks, mj_at + 1, 0, [one])
end

def md_edit_drop(blocks, id)
  mj2 = md_edit_index(blocks, id)
  return blocks if mj2 < 0
  return [{"id": md_edit_fresh(blocks), "kind": "p", "t": ""}] if blocks.length() == 1

  md_edit_splice(blocks, mj2, 1, [])
end

def md_edit_split_focus(blocks, id, next_id)
  ek_one = md_edit_at(blocks, id)
  ek_kind = (ek_one["kind"] ?? "p").to_s
  ek_list = ek_kind == "bullet" || ek_kind == "number"
  return id if ek_list && (ek_one["t"] ?? "").to_s == ""

  next_id
end

def md_edit_merge_focus(blocks, id)
  em_one = md_edit_at(blocks, id)
  em_kind = (em_one["kind"] ?? "p").to_s
  return id unless md_edit_text?(em_kind)
  return id if em_kind != "p"

  em_before = md_edit_step(blocks, id, -1)
  em_above = md_edit_at(blocks, em_before)
  # A picture above is removed rather than merged into, and the caret stays
  # where it was.
  return id unless md_edit_text?((em_above["kind"] ?? "p").to_s)

  em_before
end

def check(label, got, want)
  assert_eq(got, want)
end

# A document with one of everything, written the way `md_edit_source` writes
# one so that the round trip is an equality and not a resemblance.
SRC = [
  "# Meridian, in a document",
  "",
  "A paragraph with **bold** and a `code` word in it.",
  "",
  "## What it holds",
  "",
  "- one",
  "- two",
  "",
  "1. first",
  "2. second",
  "",
  "> Someone said this.",
  "",
  "```soli",
  "def one()",
  "  1",
  "end",
  "```",
  "",
  "![The plan](eui-asset:aaaa \"1600x1200\")",
  "",
  "[devis.pdf](eui-asset:bbbb \"39 KB\")",
  "",
  "---"
].join("\n")

DOC = md_edit_parse(SRC)

# ---- what a document is made of -------------------------------------------

check("every block is one block", DOC.length(), 12)
check("kinds, in order", DOC.map(fn(b) { b["kind"] }), [
  "h1", "p", "h2", "bullet", "bullet", "number", "number", "quote", "code", "image", "file", "rule"
])
check("ids are handed out in order", DOC.map(fn(b) { b["id"] }), [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])
check("a marker comes off the text it marked", DOC[0]["t"], "Meridian, in a document")
check("a numbered item loses its number", DOC[5]["t"], "first")
check("a fence keeps its language", DOC[8]["lang"], "soli")
# What a quoted mail is mostly made of: every blank line of the letter
# being answered comes back as a bare ">".
check("an empty quote line is a quote", md_edit_kind_of(">"), "quote")
check("and it is empty", md_edit_bare(">"), "")
check("and it survives the round trip", md_edit_source(md_edit_parse("> one\n>\n> two")), "> one\n\n>\n\n> two")
check("and its body, newlines and indentation included", DOC[8]["t"], "def one()\n  1\nend")
check("a picture keeps its address", DOC[9]["src"], "eui-asset:aaaa")
check("and the measure written in its title", [DOC[9]["w"], DOC[9]["h"]], [1600, 1200])
check("a file keeps its note", DOC[10]["note"], "39 KB")

# The one that matters: a document that goes through the editor and comes
# back must be the document that went in. Everything else here is a detail
# of that.
check("the round trip is an equality", md_edit_source(DOC), SRC)

# An editor with nothing to put the caret in is an editor nobody can start
# typing in.
check("an empty document is one empty paragraph", md_edit_parse(""), [{"id": 1, "kind": "p", "t": ""}])
check("and it serialises back to nothing", md_edit_source(md_edit_parse("")), "")

# ---- typing into a block --------------------------------------------------

PARA = [{"id": 1, "kind": "p", "t": "hello"}]

check(
  "a marker typed at the head changes what the block is",
  md_edit_set(PARA, 1, "## Title", 2),
  [{"id": 1, "kind": "h2", "t": "Title"}]
)
check(
  "a fence too, and it takes the language with it",
  md_edit_set(PARA, 1, "```rust", 2),
  [{"id": 1, "kind": "code", "t": "", "lang": "rust"}]
)
check(
  "ordinary text is stored as it arrived",
  md_edit_set(PARA, 1, "hello there", 2),
  [{"id": 1, "kind": "p", "t": "hello there"}]
)
check(
  "a marker at the head of a block that is already that kind is text",
  md_edit_set([{"id": 1, "kind": "bullet", "t": "one"}], 1, "- two", 2),
  [{"id": 1, "kind": "bullet", "t": "- two"}]
)
# `Enter` is a `submit` here, so newlines in a value can only be a paste.
check(
  "a pasted document becomes one block a line",
  md_edit_set(PARA, 1, "one\n## two\n- three", 7).map(fn(b) { [b["id"], b["kind"], b["t"]] }),
  [[1, "p", "one"], [7, "h2", "two"], [8, "bullet", "three"]]
)
check(
  "a code block keeps its newlines",
  md_edit_set([{"id": 1, "kind": "code", "t": ""}], 1, "a\nb", 2),
  [{"id": 1, "kind": "code", "t": "a\nb"}]
)
check(
  "and a caption cannot be turned into a heading",
  md_edit_set([{"id": 1, "kind": "image", "t": "", "src": "eui-asset:a"}], 1, "# not a heading", 2),
  [{"id": 1, "kind": "image", "t": "# not a heading", "src": "eui-asset:a"}]
)

# ---- Enter ----------------------------------------------------------------

check(
  "Enter adds a paragraph after the block",
  md_edit_split(PARA, 1, 2),
  [{"id": 1, "kind": "p", "t": "hello"}, {"id": 2, "kind": "p", "t": ""}]
)
check(
  "Enter in a list carries the list on",
  md_edit_split([{"id": 1, "kind": "bullet", "t": "one"}], 1, 2).map(fn(b) { b["kind"] }),
  ["bullet", "bullet"]
)
check(
  "Enter after a heading starts prose, not another heading",
  md_edit_split([{"id": 1, "kind": "h1", "t": "Title"}], 1, 2).map(fn(b) { b["kind"] }),
  ["h1", "p"]
)
EMPTY_ITEM = [{"id": 1, "kind": "bullet", "t": "one"}, {"id": 2, "kind": "bullet", "t": ""}]
check(
  "Enter on an empty item leaves the list instead of making another",
  md_edit_split(EMPTY_ITEM, 2, 3).map(fn(b) { b["kind"] }),
  ["bullet", "p"]
)
check("and the caret stays where it was", md_edit_split_focus(EMPTY_ITEM, 2, 3), 2)
check("where a block was made, the caret goes into it", md_edit_split_focus(PARA, 1, 2), 2)

# ---- Backspace at the head of a block -------------------------------------

check(
  "a marked block loses its marker first",
  md_edit_merge([{"id": 1, "kind": "h2", "t": "Title"}], 1),
  [{"id": 1, "kind": "p", "t": "Title"}]
)
TWO = [{"id": 1, "kind": "p", "t": "one "}, {"id": 2, "kind": "p", "t": "two"}]
check("and then it joins what is above it", md_edit_merge(TWO, 2), [{"id": 1, "kind": "p", "t": "one two"}])
check("the caret follows the text", md_edit_merge_focus(TWO, 2), 1)
check("the first block has nothing to join", md_edit_merge(PARA, 1), PARA)

SHOT = [{"id": 1, "kind": "image", "t": "", "src": "eui-asset:a"}, {"id": 2, "kind": "p", "t": "under it"}]
check(
  "a picture above is removed rather than merged into",
  md_edit_merge(SHOT, 2),
  [{"id": 2, "kind": "p", "t": "under it"}]
)
check("and the caret does not move to a block that has gone", md_edit_merge_focus(SHOT, 2), 2)

# ---- the toolbar ----------------------------------------------------------

check("a tool sets a kind", md_edit_kind(PARA, 1, "h1")[0]["kind"], "h1")
check(
  "and pressing it again puts the block back",
  md_edit_kind([{"id": 1, "kind": "h1", "t": "x"}], 1, "h1")[0]["kind"],
  "p"
)
check("a picture is not a kind anything can set", md_edit_kind(SHOT, 1, "h1")[0]["kind"], "image")

check("B marks the block", md_edit_wrap(PARA, 1, "**")[0]["t"], "**hello**")
check(
  "and unmarks it when it is already marked",
  md_edit_wrap([{"id": 1, "kind": "p", "t": "**hello**"}], 1, "**")[0]["t"],
  "hello"
)
check("an empty block is left alone", md_edit_wrap([{"id": 1, "kind": "p", "t": ""}], 1, "**")[0]["t"], "")
check("any mark, not only the two on the bar", md_edit_wrap(PARA, 1, "~~")[0]["t"], "~~hello~~")
# `**` on its own is two marks with nothing between them, and taking a mark
# off it would leave less than nothing.
check("a bare pair of marks is wrapped, not unwrapped", md_edit_wrap([{"id": 1, "kind": "p", "t": "**"}], 1, "**")[0]["t"], "******")

# ---- moving about ---------------------------------------------------------

check("down goes down", md_edit_step(TWO, 1, 1), 2)
check("up goes up", md_edit_step(TWO, 2, -1), 1)
check("up from the first stays put", md_edit_step(TWO, 1, -1), 1)
check("down from the last stays put", md_edit_step(TWO, 2, 1), 2)
check("an id nothing has stays put", md_edit_step(TWO, 99, 1), 99)

# ---- putting things in and taking them out --------------------------------

SHOT_BLOCK = {"id": 9, "kind": "image", "t": "A plan", "src": "eui-asset:c", "w": 800, "h": 600}
check(
  "a picture lands after the block the caret was in",
  md_edit_put(TWO, 1, SHOT_BLOCK).map(fn(b) { b["id"] }),
  [1, 9, 2]
)
check("and after everything when the caret was nowhere", md_edit_put(TWO, 0, SHOT_BLOCK).map(fn(b) { b["id"] }), [1, 2, 9])
check("the × takes one out", md_edit_drop(TWO, 1).map(fn(b) { b["id"] }), [2])
check("and never leaves the document with nothing to type into", md_edit_drop(PARA, 1), [{"id": 2, "kind": "p", "t": ""}])
check("the next id is one past the highest, not the length", md_edit_fresh([{"id": 4}, {"id": 9}]), 10)

# `concat` writes into the array it is called on. Every mutator here goes
# through `md_edit_splice`, whose first call is on a slice, and this is what
# says so: a handler that built a new list and found the old one changed too
# would have edited the state it was deciding about.
KEPT = [{"id": 1, "kind": "p", "t": "one"}, {"id": 2, "kind": "p", "t": "two"}]
md_edit_put(KEPT, 0, {"id": 7, "kind": "p", "t": "three"})
md_edit_put(KEPT, 1, {"id": 8, "kind": "p", "t": "four"})
md_edit_drop(KEPT, 1)
md_edit_set(KEPT, 1, "changed", 9)
md_edit_split(KEPT, 1, 9)
md_edit_merge(KEPT, 2)
md_edit_wrap(KEPT, 1, "**")
md_edit_kind(KEPT, 1, "h1")
check("nothing here edits the list it was given", KEPT, [{"id": 1, "kind": "p", "t": "one"}, {"id": 2, "kind": "p", "t": "two"}])

# ---- tasks ----------------------------------------------------------------

check("a box makes a task, not a bullet", md_edit_kind_of("- [ ] buy milk"), "task")
check("and the box comes off the text", md_edit_bare("- [x] done"), "done")
check("a tick is read", md_edit_done?("- [x] done"), true)
check("either case", md_edit_done?("- [X] done"), true)
check("an empty box is not", md_edit_done?("- [ ] not yet"), false)
check("an ordinary bullet is still a bullet", md_edit_kind_of("- plain"), "bullet")

TASKS = md_edit_parse("- [ ] one\n- [x] two")
check("both are tasks", TASKS.map(fn(b) { b["kind"] }), ["task", "task"])
check("and only the second is ticked", TASKS.map(fn(b) { b["done"] }), [false, true])
check("they survive the round trip", md_edit_source(TASKS), "- [ ] one\n- [x] two")
check("the box is what the tick writes", md_edit_source(md_edit_check(TASKS, 1)), "- [x] one\n- [x] two")
check("and nothing else can be ticked", md_edit_check(PARA, 1), PARA)

# ---- tables ---------------------------------------------------------------

GRID = md_edit_parse("| Ref | Amount |\n| --- | --- |\n| FA-1 | 137 |\n| FA-2 | 412 |")

check("a run of pipes is one block", GRID.length(), 1)
check("and it is a table", GRID[0]["kind"], "table")
check("the ruler is read and dropped", GRID[0]["rows"], [ ["Ref", "Amount"], ["FA-1", "137"], ["FA-2", "412"] ])
check("a table has no one field to type into", md_edit_text?("table"), false)
check(
  "and it writes its ruler back",
  md_edit_source(GRID),
  "| Ref | Amount |\n| --- | --- |\n| FA-1 | 137 |\n| FA-2 | 412 |"
)
check("a dashes-and-colons row is a ruler", md_edit_ruler?([":---", "---:"]), true)
check("a row with a word in it is not", md_edit_ruler?(["---", "Ref"]), false)
check("nor is an empty one", md_edit_ruler?([""]), false)

check("a cell is written where it was asked for", md_edit_cell(GRID, 1, 1, 0, "FA-9")[0]["rows"][1][0], "FA-9")
check("a cell outside the table changes nothing", md_edit_cell(GRID, 1, 9, 0, "x"), GRID)
check("how wide it is, is its widest row", md_edit_cols(GRID[0]), 2)
check("a row lands on the end, the right width", md_edit_row_add(GRID, 1)[0]["rows"][3], ["", ""])
check("a column lands on every row", md_edit_col_add(GRID, 1)[0]["rows"].map(fn(r) { r.length() }), [3, 3, 3])
# `[["", ""], …]` lexes as a *string* in Soli 2.3.7, not an array of arrays
# — a space after the outer bracket is what makes it one. This is the check
# that says so: without it the widget shipped a table whose rows were a
# sentence, and nothing anywhere would have said a word.
check("a new table is a head and a row", md_edit_table(7)["rows"], [ ["", ""], ["", ""] ])
check("and its rows really are rows", md_edit_table(7)["rows"].class(), "array")
# A ragged table is not an error anywhere in markdown, so it is not one here.
RAGGED = [{"id": 1, "kind": "table", "rows": [ ["a", "b"], ["c"] ]}]
check("a short row is padded to where it was written", md_edit_cell(RAGGED, 1, 1, 1, "d")[0]["rows"][1], ["c", "d"])
check("and to the widest on the way out", md_edit_source(RAGGED), "| a | b |\n| --- | --- |\n| c |  |")

# ---- moving a block -------------------------------------------------------

FOUR = md_edit_parse("one\n\ntwo\n\nthree\n\nfour")

check("down by one", md_edit_shift(FOUR, 1, 1).map(fn(b) { b["t"] }), ["two", "one", "three", "four"])
check("up by one", md_edit_shift(FOUR, 3, -1).map(fn(b) { b["t"] }), ["one", "three", "two", "four"])
check("up from the top is a no-op, never a wrap", md_edit_shift(FOUR, 1, -1), FOUR)
check("down from the bottom likewise", md_edit_shift(FOUR, 4, 1), FOUR)

# A drop reports the slot in the document as it stands, with the block still
# in it — so moving one down by a place is a slot two further on, and the
# correction belongs here rather than in every caller.
check("dropped where it already is, nothing moves", md_edit_move_to(FOUR, 1, 1).map(fn(b) { b["t"] }), ["one", "two", "three", "four"])
check("dropped at the top", md_edit_move_to(FOUR, 4, 0).map(fn(b) { b["t"] }), ["four", "one", "two", "three"])
check("dropped at the end", md_edit_move_to(FOUR, 1, 4).map(fn(b) { b["t"] }), ["two", "three", "four", "one"])
check("dropped past the end lands at the end", md_edit_move_to(FOUR, 1, 99).map(fn(b) { b["t"] }), ["two", "three", "four", "one"])

# ---- undoing --------------------------------------------------------------

DOC0 = {"blocks": PARA, "focus": 1}
DOC1 = md_edit_remember(DOC0, "submit", 1)
DOC1 = DOC1.merge({"blocks": md_edit_split(PARA, 1, 2)})

check("the stack holds what was there before", DOC1["past"].length(), 1)
check("undoing gives it back", md_edit_undo(DOC1)["blocks"], PARA)
check("and empties the stack", md_edit_undo(DOC1)["past"], [])
check("redoing puts it back again", md_edit_redo(md_edit_undo(DOC1))["blocks"].length(), 2)
check("undoing nothing is nothing", md_edit_undo(DOC0), DOC0)
check("redoing nothing is nothing", md_edit_redo(DOC0), DOC0)

# Typing is coalesced: `change` arrives every time a field goes quiet, and a
# stack with one entry per pause would walk back through a sentence a breath
# at a time.
TYPED = md_edit_remember(md_edit_remember(md_edit_remember(DOC0, "change", 1), "change", 1), "change", 1)
check("three pauses in one block are one step", TYPED["past"].length(), 1)
check("but a pause in another block is its own", md_edit_remember(TYPED, "change", 2)["past"].length(), 2)
check("and so is anything that is not typing", md_edit_remember(TYPED, "tool", 1)["past"].length(), 2)

# The stack has a floor, and it is the oldest that goes.
DEEP = DOC0
for i in range(0, 45)
  DEEP = md_edit_remember(DEEP.merge({"blocks": [{"id": 1, "kind": "p", "t": str(i)}]}), "tool", 1)
end
check("the stack stops at its cap", DEEP["past"].length(), 40)
check("and it is the oldest that went", DEEP["past"][0]["blocks"][0]["t"], "5")

check("control is the accelerator", md_edit_accel?(2), true)
check("and so is super, because a server cannot tell the machines apart", md_edit_accel?(8), true)
check("shift with it is still it", md_edit_accel?(3), true)
check("a bare key is not", md_edit_accel?(0), false)
check("nor is alt", md_edit_accel?(4), false)

# ---- the gestures, through the reducer ------------------------------------
#
# Everything above is the model; this is what a press actually does to it.
# The reducer is a pure function of the document, what happened and the
# event, so it is checked the same way rather than through a window.

def said(payload, props)
  {"payload": payload, "props": props}
end

NOTE = {"blocks": md_edit_parse("one\n\ntwo"), "focus": 1}

# `/` at the head of a block opens the panel, and the block goes on being an
# ordinary block until something is picked out of it.
SLASHED = md_edit_step_change(NOTE, said("/", {}), 1, 9)
check("a slash opens the panel", SLASHED["slash"], 1)
check("and the block holds what was typed", SLASHED["blocks"][0]["t"], "/")
check("ordinary text does not", md_edit_step_change(NOTE, said("hello", {}), 1, 9)["slash"], 0)
check("nor does a slash in the middle", md_edit_step_change(NOTE, said("a/b", {}), 1, 9)["slash"], 0)

check("an empty query offers every tool", md_edit_slash_hits("/").length(), md_edit_tools().length())
check("and a query narrows it", md_edit_slash_hits("/tab").map(fn(t) { t["tool"] }), ["table"])
check("a query that matches nothing offers nothing", md_edit_slash_hits("/zzz"), [])
check("and something that is not a slash is not a query", md_edit_slash_hits("tab"), [])

check("the highlight walks down", md_edit_walk(0, 3, 1), 1)
check("and wraps at the bottom", md_edit_walk(2, 3, 1), 0)
check("and at the top", md_edit_walk(0, 3, -1), 2)
check("an empty panel has nowhere to walk", md_edit_walk(0, 0, 1), 0)

# Picking one takes the `/query` back out: it was the gesture, not the text.
PICKED = md_edit_step_slash(SLASHED, said(null, {"tool": "h2"}), {"tool": "h2"}, 1)
check("picking sets the kind", PICKED["blocks"][0]["kind"], "h2")
check("and clears the query", PICKED["blocks"][0]["t"], "")
check("and shuts the panel", PICKED["slash"], 0)
check("a row with no tool on it is the panel being dismissed", md_edit_step_slash(SLASHED, said(null, {}), {}, 1)["slash"], 0)
check("Enter takes whatever is highlighted", md_edit_step_pick(SLASHED.merge({"at": 0}), 1)["blocks"][0]["kind"], md_edit_tools()[0]["tool"])

# ---- the toolbar through the reducer --------------------------------------

check("a table lands after the block and takes the caret", md_edit_step_tool(NOTE, "table", 1, 9)["focus"], 9)
check("and it is a table", md_edit_at(md_edit_step_tool(NOTE, "table", 1, 9)["blocks"], 9)["kind"], "table")
check("a rule lands after it and does not", md_edit_step_tool(NOTE, "rule", 1, 9)["focus"], 1)
check("the link tool asks rather than acting", md_edit_step_tool(NOTE, "link", 1, 9)["asking"], {"block": 1, "url": ""})
check("a task is a kind like any other", md_edit_step_tool(NOTE, "task", 1, 9)["blocks"][0]["kind"], "task")

ASKING = NOTE.merge({"asking": {"block": 1, "url": "https://example.test"}})
check(
  "the dialog writes the link round the block",
  md_edit_step_link(ASKING, said(null, {"go": true}), {"go": true})["blocks"][0]["t"],
  "[one](https://example.test)"
)
check("and shuts", md_edit_step_link(ASKING, said(null, {"go": true}), {"go": true})["asking"], {})
check("cancelling changes nothing", md_edit_step_link(ASKING, said(null, {}), {})["blocks"][0]["t"], "one")
check("and neither does an empty address", md_edit_step_link(NOTE.merge({"asking": {"block": 1, "url": "  "}}), said(null, {"go": true}), {"go": true})["blocks"][0]["t"], "one")

# ---- the keys through the reducer -----------------------------------------

check("a bare z is a letter and nothing else", md_edit_step_key(NOTE, said(["z", 0], {}), 1, 9), NOTE)
check("down moves the caret", md_edit_step_key(NOTE, said(["ArrowDown", 0], {}), 1, 9)["focus"], 2)
check("Escape shuts the panel", md_edit_step_key(SLASHED, said(["Escape", 0], {}), 1, 9)["slash"], 0)
check(
  "the arrows walk the panel while it is open",
  md_edit_step_key(SLASHED.merge({"at": 0}), said(["ArrowDown", 0], {}), 1, 9)["at"],
  1
)
check(
  "Alt and an arrow moves the block instead",
  md_edit_step_key(NOTE, said(["ArrowUp", 4], {}), 2, 9)["blocks"].map(fn(b) { b["t"] }),
  ["two", "one"]
)
check(
  "Backspace at the head joins",
  md_edit_step_key(NOTE, said(["Backspace", 0], {}), 2, 9)["blocks"].map(fn(b) { b["t"] }),
  ["onetwo"]
)

# Undo reaches back through whatever the last step was, and only with the
# accelerator: a printable character cannot be withheld from a field
# (03 §3.1 rule 1), so a bare `z` has to be let through as a letter.
TICKED = md_edit_step_key(NOTE, said(["ArrowUp", 4], {}), 2, 9)
check("control and z undoes it", md_edit_step_key(TICKED, said(["z", 2], {}), 1, 9)["blocks"].map(fn(b) { b["t"] }), ["one", "two"])
check("and super does too", md_edit_step_key(TICKED, said(["z", 8], {}), 1, 9)["blocks"].map(fn(b) { b["t"] }), ["one", "two"])
UNDONE = md_edit_step_key(TICKED, said(["z", 2], {}), 1, 9)
check("and y puts it back", md_edit_step_key(UNDONE, said(["y", 2], {}), 1, 9)["blocks"].map(fn(b) { b["t"] }), ["two", "one"])
check("shift and z as well, which is the other spelling", md_edit_step_key(UNDONE, said(["Z", 3], {}), 1, 9)["blocks"].map(fn(b) { b["t"] }), ["two", "one"])

# ---- what a line is, before any of the above ------------------------------

# No tool on the bar writes either of these, and both still have to come
# back unharmed: a document arrives from somewhere else as often as it is
# written here, and a mark the editor mangles is a word that changed
# meaning on the way through.
check("a strike survives", md_edit_source(md_edit_parse("~~gone~~")), "~~gone~~")
check("an underline survives", md_edit_source(md_edit_parse("<u>kept</u>")), "<u>kept</u>")

check("a picture on its own line is a picture", md_image_of("![a](b)")["src"], "b")
check("a picture with prose after it is not", md_image_of("![a](b) and more"), {})
check("a link on its own line is not a file", md_file_of("[a](https://example.com)"), {})
check("a link to an asset is", md_file_of("[a](eui-asset:z)")["src"], "eui-asset:z")
check("and so is one to a file beside the application", md_file_of("[devis.pdf](public/att/a.pdf)")["src"], "public/att/a.pdf")
check("a mail address is not a file", md_file_of("[write](mailto:a@b.test)"), {})
check("an asset address becomes an asset", md_src("eui-asset:abcd"), {"asset": "abcd"})
check("and anything else stays a path", md_src("public/images/a.png"), "public/images/a.png")
check("a measure in a title is read", md_target("x \"640x480\""), {"src": "x", "note": "640x480", "w": 640, "h": 480})
check("a note in a title is kept", md_target("x \"39 KB\"")["note"], "39 KB")
check("and a title nobody wrote is nothing", md_target("x"), {"src": "x", "note": "", "w": 0, "h": 0})

# A picture is never wider than the measure and never taller than a
# screenful, and keeps the shape it actually has through both.
check("a wide picture is fitted to the measure", md_fit(1600, 900, 800), [800, 450])
check("a tall one is fitted to the ceiling", md_fit(600, 1200, 800), [260, 520])
check("and the ceiling applies after the measure does", md_fit(1600, 1200, 800), [693, 520])
check("a small one is left alone", md_fit(120, 90, 800), [120, 90])
check("and one that never said gets a modest box", md_fit(0, 0, 800), [320, 200])
