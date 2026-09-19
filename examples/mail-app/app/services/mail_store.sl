# The mailbox, kept between runs -- in SoliDB when there is one, and in a
# file when there is not.
#
# Loaded as a service so a background job can write what it fetched
# without going through a session (`serve/background_jobs.rs` loads
# `app/services`, never `app/controllers`).
# Whether the database answered the last time anything asked it.
#
# Per worker, which is the right grain: a worker that has never spoken to
# SoliDB tries once, and one that has been refused stops trying for the
# life of the process rather than paying a failed connection on every
# save. `nil` means "not asked yet".
MAIL_DB_OK = {}

# And what has already been written, so a save that changes one message
# does not rewrite a hundred.
#
# Also per worker, and deliberately allowed to be wrong in the safe
# direction: a worker that does not know writes one message more than it
# had to, which costs a round trip. Forgetting is what must never happen,
# and nothing here forgets -- the key is the account and the UID, and the
# value is what was true about that message when it was written.
MAIL_WROTE = {}

def mail_db?()
  said = MAIL_DB_OK["ok"]
  return said if said.nil? != true

  # One question, once, and the answer is kept: `MailBox` is the smallest
  # collection, so this is the cheapest way to find out whether there is a
  # database at all.
  probe = MailBox.where("doc.account == @a", {"a": "-probe-"}).limit(1).all() rescue nil
  ok = probe.nil? != true
  MAIL_DB_OK["ok"] = ok
  mail_say("store: " + (ok ? "SoliDB" : "a file, the database did not answer"))
  ok
end

# What a stored message is fingerprinted by: the things that change about
# a message after it arrives. The subject and the sender do not, so they
# are not in it.
def mail_mark(one)
  str(one["uid"] ?? 0) + ":" + ((one["seen"] ?? false) == true ? "1" : "0") +
    ":" + ((one["loaded"] ?? false) == true ? "1" : "0") +
    ":" + str((one["body"] ?? "").length()) + ":" + str((one["atts"] ?? []).length())
end

# One message, written if it has changed since this worker last wrote it.
def mail_store_put(address, one, box = "")
  uid = one["uid"] ?? 0
  return if uid < 1

  here = box == "" ? MAIL_BOX : box
  key = address + "/" + here + "/" + str(uid)
  mark = mail_mark(one)
  return if MAIL_WROTE[key] == mark

  found = MailMessage.where("doc.account == @a AND doc.box == @b AND doc.uid == @u", {"a": address, "b": here, "u": uid}).first() rescue nil
  if found.nil?
    made = MailMessage.create({"account": address, "box": here, "uid": uid, "msg": one, "at": mail_now()}) rescue nil
    return if made.nil?
  else
    fixed = found.update({"msg": one, "at": mail_now()}) rescue nil
    return if fixed.nil?
  end
  MAIL_WROTE[key] = mark
end

# The two numbers that belong to the box rather than to a message.
def mail_store_box(address, total, valid, box = "")
  here = box == "" ? MAIL_BOX : box
  found = MailBox.where("doc.account == @a AND doc.box == @b", {"a": address, "b": here}).first() rescue nil
  return (found.update({"total": total, "uidvalidity": valid, "at": mail_now()}) rescue nil) if found.nil? != true

  MailBox.create({"account": address, "box": here, "total": total, "uidvalidity": valid, "at": mail_now()}) rescue nil
end

# The whole list, saved -- which in a database means "whatever of it has
# changed".
#
# The call sites hand over the entire mailbox because that is what the
# file store wanted: one write of everything. A hundred documents a save
# would be a hundred round trips inside the session's frame lock, so the
# fingerprint above decides which of them are actually written. In the
# common case -- a message marked read, a letter fetched -- that is one.
def mail_store_save(address, msgs, total, valid, box = "")
  return if mail_demo?()
  return mail_file_save(address, msgs, total, valid) if mail_db?() != true

  here = box == "" ? MAIL_BOX : box
  began = mail_ms()
  wrote = 0
  for one in msgs
    before = MAIL_WROTE[address + "/" + here + "/" + str(one["uid"] ?? 0)]
    mail_store_put(address, one, here)
    wrote = wrote + 1 unless before == mail_mark(one)
  end
  mail_store_box(address, total, valid, here)
  mail_say("store: wrote " + str(wrote) + " of " + str(msgs.length()) + " in " + str(mail_ms() - began) + " ms") if wrote > 0
end

# The newest messages this account has, and what the box said about
# itself.
#
# `MAIL_LIMIT` of them rather than all: the list shows a window and asks
# for more when the cursor reaches the end, and a cache that loaded ten
# thousand rows to show twenty would be slower than the fetch it saves.
def mail_store_load(address, box = "")
  return nil if mail_demo?()
  return nil if address == ""
  return (box == "" || box == MAIL_BOX ? mail_file_load(address) : nil) if mail_db?() != true

  here = box == "" ? MAIL_BOX : box
  rows = MailMessage.where("doc.account == @a AND doc.box == @b", {"a": address, "b": here}).order("uid", "DESC").limit(MAIL_LIMIT).all() rescue nil
  return mail_file_load(address) if rows.nil?
  # Nothing in the database for this account yet. If the file store this
  # replaced has something, that is this account's mail and it is moved
  # in rather than thrown away and refetched -- once, at the first connect
  # after the database arrives, and never again.
  if rows.length() == 0
    return nil if here != MAIL_BOX

    kept = mail_file_load(address)
    return nil if kept.nil?

    mail_say("store: moving " + str((kept["msgs"] ?? []).length()) + " messages out of the file and into the database")
    mail_store_save(address, kept["msgs"] ?? [], kept["total"] ?? 0, kept["uidvalidity"] ?? 0, here)
    return kept
  end

  msgs = mail_unique(rows.map(fn(r) { r["msg"] ?? {} }))
  return nil if msgs.length() == 0

  said = MailBox.where("doc.account == @a AND doc.box == @b", {"a": address, "b": here}).first() rescue nil
  {
    "msgs": msgs,
    "total": said.nil? ? msgs.length() : (said["total"] ?? msgs.length()),
    "uidvalidity": said.nil? ? 0 : (said["uidvalidity"] ?? 0),
    "at": said.nil? ? 0 : (said["at"] ?? 0)
  }
end

# The folders an account has, as the server last described them.
#
# Read at sign-in and at every account switch, which is why the rail is
# there in the first frame instead of appearing a tick later -- and why it
# stops vanishing on a reconnect, which is what a list held only in
# session state does.
def mail_folders_load(address)
  return [] if mail_demo?()
  return [] if address == ""
  return [] if mail_db?() != true

  rows = MailFolder.where("doc.account == @a", {"a": address}).all() rescue nil
  return [] if rows.nil?

  rows.map(fn(r) {
    {
      "name": r["name"] ?? "", "label": r["label"] ?? (r["name"] ?? ""),
      "kind": r["kind"] ?? "", "counts": r["counts"] ?? {}
    }
  })
end

# And written when `LIST` has answered.
#
# Whole, not incrementally: a folder list is a dozen rows and the thing
# that matters about it is that it is *the* list -- a folder deleted on
# the server must leave here too, and a merge would keep it for ever.
# One folder's counts, written where the folder is.
#
# The rail asks `STATUS` for every folder once a session, which is
# twenty-three round trips -- spread one per tick, so invisible, but
# twenty-three all the same. Kept here, the numbers are on screen in the
# first frame of the next session and the round trips only refresh them.
def mail_counts_save(address, name, counts)
  return if mail_demo?()
  return if mail_db?() != true
  return if name == ""

  found = MailFolder.where("doc.account == @a AND doc.name == @n", {"a": address, "n": name}).first() rescue nil
  return if found.nil?

  found.update({"counts": counts, "at": mail_now()}) rescue nil
end

def mail_folders_save(address, folders)
  return if mail_demo?()
  return if address == ""
  return if mail_db?() != true
  return if folders.length() == 0

  # What is already known about each folder, kept across the rewrite: the
  # list is replaced because a folder deleted on the server must leave,
  # but a folder that is still there keeps its counts rather than blanking
  # the rail until the next `STATUS` walk.
  before = {}
  for row in (MailFolder.where("doc.account == @a", {"a": address}).all() rescue [])
    before[row["name"] ?? ""] = row["counts"] ?? {}
  end
  MailFolder.where("doc.account == @a", {"a": address}).delete_all() rescue nil
  for one in folders
    name = one["name"] ?? ""
    MailFolder.create({
      "account": address,
      "name": name,
      "label": one["label"] ?? name,
      "kind": one["kind"] ?? "",
      "counts": one["counts"] ?? (before[name] ?? {}),
      "at": mail_now()
    }) rescue nil
  end
  mail_say("folders: " + str(folders.length()) + " saved")
end

# One message gone from the cache, because it is gone from the mailbox.
def mail_store_drop(address, uid, box = "")
  return if mail_demo?()
  return if mail_db?() != true

  here = box == "" ? MAIL_BOX : box
  MailMessage.where("doc.account == @a AND doc.box == @b AND doc.uid == @u", {"a": address, "b": here, "u": uid}).delete_all() rescue nil
  MAIL_WROTE[address + "/" + here + "/" + str(uid)] = nil
end

# One account's copy of its mail, gone.
def mail_store_forget(address)
  return if mail_demo?()
  return mail_file_forget(address) if mail_db?() != true

  MailMessage.where("doc.account == @a", {"a": address}).delete_all() rescue nil
  MailBox.where("doc.account == @a", {"a": address}).delete_all() rescue nil
  MailFolder.where("doc.account == @a", {"a": address}).delete_all() rescue nil
  mail_file_forget(address)
end

def mail_file_save(address, msgs, total, valid)
  return if mail_demo?()

  said = json_stringify({
    "address": address, "total": total, "uidvalidity": valid,
    "at": mail_now(), "msgs": msgs
  }) rescue ""
  return if said == ""

  File.write(mail_store_file(address), said) rescue nil
end

# Keyed on the address, so signing in as someone else does not hand you
# the last person's inbox out of the cache.
def mail_file_load(address)
  return nil if mail_demo?()
  return nil if address == ""

  raw = File.read(mail_store_file(address)) rescue ""
  # The one-file store an older build left behind, which is this
  # account's only if the address inside it says so.
  raw = File.read(MAIL_STORE) rescue "" if raw == ""
  return nil if raw == ""

  data = json_parse(raw) rescue nil
  return nil if data.nil?
  return nil if (data["address"] ?? "") != address

  # Deduplicated on the way in, not only on the way out. A store written
  # by an older build holds whatever overlap put it there, and the list
  # goes on screen at connect before any fetch can repair it -- which is
  # exactly when the frame was refused for a repeated key, every single
  # boot, with the window never getting a first frame at all.
  msgs = mail_unique(data["msgs"] ?? [])
  return nil if msgs.length() == 0

  {
    "msgs": msgs,
    "total": data["total"] ?? msgs.length(),
    "uidvalidity": data["uidvalidity"] ?? 0,
    "at": data["at"] ?? 0
  }
end

# One account's copy of its mail, gone. The legacy file goes with it only
# when it is this account's -- otherwise signing out of the second
# account would throw away the first one's list.
def mail_file_forget(address)
  return if mail_demo?()

  File.delete(mail_store_file(address)) rescue nil
  raw = File.read(MAIL_STORE) rescue ""
  return if raw == ""

  data = json_parse(raw) rescue nil
  return if data.nil?
  return if (data["address"] ?? "") != address

  File.delete(MAIL_STORE) rescue nil
end
