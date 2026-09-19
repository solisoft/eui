# Gmail, read over IMAP, drawn as an EUI tree.
#
# The design brief is HEY: a mail client that refuses to look like a mail
# client. No three-pane split, no toolbar, no ruled table of rows. One
# column down the middle of the window, set at a measure you can read,
# with whitespace doing the work borders usually do. A message is not a
# preview in a pane -- it takes the whole window, the way a letter takes
# the whole desk.
#
# And it is driven from the keyboard. Every action in this application has
# a key: j and k move, Enter opens, u comes back, r refreshes, ? lists
# them. The pointer works, but nothing here needs it.
#
# ---------------------------------------------------------------------
# Four things from the protocol shape everything below. None is a
# workaround; each is the grain of the thing.
#
# **A text node is one string with one style** (02 §3). There are no
# styled runs, so HEY's "**Sender** subject" is two text nodes in a row,
# and every word this file wants in another weight costs a node.
#
# **`max_width` does not constrain measurement.** A node with no explicit
# `width` is measured against a loosened constraint, so text that will
# wrap is measured as one line and then overlaps what follows. Every
# paragraph here is given a pixel width computed from the viewport --
# `lay["measure"]` -- and never a percentage.
#
# **There is no global key handler** (06 §5). A `key_down` handler hears a
# key only when it, or something below it, has focus. So the root box
# holds the handler and carries `autofocus`, and dispatch -- nearest
# handler on the path, no bubbling -- brings every key up to it, including
# from a row that took focus with Tab. The login screen deliberately has
# no such handler: there, the keyboard belongs to the two fields.
#
# **The server never learns the scroll position** (08 §8), but it may set
# one: `scroll_to` is the single name in `p` that never reaches the client
# as a prop, becoming `Op::ScrollTo` instead. That is what lets j and k
# carry the cursor past the fold. It also means this file has to do the
# arithmetic itself, which is why the masthead and every row have an
# explicit pixel height -- `MAIL_HEAD_H` and `MAIL_ROW_H` -- rather than
# whatever their contents came to.
#
# Nothing here asks the client for a capability. The IMAP connection is
# this process's, out to imap.gmail.com; the window sees boxes and text.

MAIL_HOST = "imap.gmail.com"
MAIL_BOX = "INBOX"

# The trash, when the server will not say which mailbox it is. Only a
# fallback: `mail_trash_name` asks, because this name is English-only.
#
# Not `delete` + `expunge`, which is what the IMAP verbs are called and
# the wrong thing on Gmail: there, removing a message from INBOX only
# removes the label, so it quietly survives in All Mail and the person
# who pressed delete has no idea. Moving it to the trash mailbox is what
# Gmail's own clients do, it is what the web interface calls deleting,
# and it is undoable for thirty days -- which matters for a key with no
# confirmation behind it.
MAIL_TRASH = "[Gmail]/Trash"


# How many are fetched per tick of the loader, and how many more the foot
# of the list asks for.
#
# The list used to arrive all at once, after twenty full `BODY.PEEK[]`
# round trips, behind a full-screen "Fetching your mail…" -- which for a
# real inbox is a window that sits still for the better part of a minute
# and shows nothing. So the fetch is done a batch at a time: each `wake`
# takes the next `MAIL_BATCH`, appends them, and leaves `busy` set while
# there are more, so you are reading a list that is filling in rather than
# watching a spinner that is not.
#
# Five made sense when five meant five round trips. One command answers a
# whole run now, so the batch is the number that fills the list in two
# ticks -- and reaching the foot brings fifty more rather than five, which
# is the difference between paging and pretending to.
MAIL_BATCH = 50

# How often the loader asks for the next batch. The floor a client must
# impose is 100 ms (06 §1.1) and there is no point being near it: a batch
# takes about a second, and every tick that arrives while one is running
# is a round trip that can only answer "still going".
MAIL_TICK = 250

# How long a stored mailbox counts as current.
#
# Opening the window arms a refresh, and a refresh begins with a ~1200 ms
# handshake held under the session's frame lock -- so every reconnect,
# however recent the last one, spent a second and a bit frozen to ask a
# question that almost always answers "nothing new". A window reopened
# thirty seconds after it closed does not need to ask again. `r` always
# does, whatever this says.
MAIL_FRESH = 120

# How often an open window asks whether anything arrived, in seconds.
#
# The clock is the `wake` prop and nothing else (06 §1.1), so an
# application that wants to look again has to carry one -- and this one
# carried it only while it was busy, which meant a window left open never
# learned about new mail at all. You pressed `r`, or you closed and
# reopened it.
#
# Ninety seconds is a round trip a minute and a half on a connection that
# is already open: a `SELECT` and a `UID SEARCH` above the highest UID
# held, about 200 ms, and nothing fetched when there is nothing new. The
# wake itself costs the client one frame per period, which is the idle
# budget spec 10 §1 asks for.
# Thirty seconds rather than ninety.
#
# Ninety was the number of a poll that cost a handshake, a `SELECT` and a
# search -- a second and a half of held frame lock, which is a thing you
# want rarely. What a poll costs now is one `UID SEARCH` on a connection
# that is already open and already in the right mailbox: 135 ms, measured,
# and nothing on screen. At thirty seconds that is four tenths of a
# percent of one worker, and the worst case between a mail arriving at the
# server and the window saying so falls from a minute and a half to half a
# minute.
#
# The thing that would make this number irrelevant is `IDLE` -- the server
# telling us, instead of us asking -- and `IDLE` is a blocking read, so it
# cannot live inside a session event any more than a slow fetch can. It
# wants the job queue.
# How long a folder's counts are believed before they are asked for
# again. Ten minutes: the number beside a folder you are not in is not
# the sort of thing anybody watches change, and asking is a round trip
# per folder.
MAIL_COUNTS_FRESH = 600

MAIL_POLL = 30

# How long the keyboard has to have been still before the poll will run,
# in seconds.
MAIL_QUIET = 5

# Where a fetched picture is put, and the ceilings on doing it.
#
# An EUI asset has to live under `public/` or `app/assets` -- the server
# refuses to serve one from anywhere else, because an asset is handed to
# anyone holding its hash and the rule is what keeps that set to files the
# application meant to publish. So `public/mail-img`, which `.gitignore`
# covers: these are someone else's bytes and they are not ours to commit.
#
# The name is the SHA-256 of the address, which does two things: the same
# picture in twenty newsletters is fetched once, and a mail cannot choose
# where its bytes are written.
MAIL_IMG_DIR = "public/mail-img"
MAIL_IMG_MAX = 12

# The largest a picture is ever drawn in the reader, and therefore the
# largest worth keeping: the measure at its widest, and `MAIL_IMG_TALL`.
MAIL_IMG_WIDE = 760

# A body is kept only up to this, because the whole of it lives in session
# state and state is diffed on every event.
MAIL_BODY_CAP = 40000

# One text node carries at most 4096 **bytes** -- `MAX_INLINE_STR`, and the
# server refuses the frame above it with "a text of N bytes; the client
# accepts at most 4096 -- split it into nodes". A real mail is routinely
# past that on its own, so a body is not one text node: it is a column of
# them, cut on line boundaries and stacked with no gap, which reads as one
# block because a text node's line height is the same inside a chunk as
# between two.
#
# The budget is bytes, not characters, and `bytesize` is what answers that
# -- a body with accents in it is longer on the wire than `length` says,
# which is exactly the kind of message that would get through in testing
# and fail on a French inbox.
MAIL_CHUNK = 3000

# An HTML part is bigger than a text one -- it carries the stylesheet and
# the table scaffolding -- so it gets its own, larger ceiling.
MAIL_HTML_CAP = 200000

# A single line with no break in it -- a base64 blob in a text part, a
# signed URL -- cannot be cut on a line boundary, so it is cut by hand.
# 1000 characters is at most 4000 bytes even if every one of them is a
# four-byte codepoint, which keeps the guarantee without measuring.
MAIL_WRAP = 1000

# The geometry the scroll arithmetic depends on. Both are set as explicit
# heights on the nodes themselves, so they are true by construction rather
# than by a measurement this process cannot make.
#
# A row is `space.5` above and below (16), two lines of `text.base` (22
# each) and `space.1` between them: 16 + 22 + 2 + 22 + 16 = 78.
# How wide the folder rail is. Wide enough for "All Mail" and a folder
# somebody named after a project, narrow enough that the measure beside it
# is still a measure.
MAIL_RAIL_W = 200

MAIL_ROW_H = 78
MAIL_HEAD_H = 68
# Where row zero starts inside the scroller. A windowed `list` puts row
# `i` at `i * item_height` in its content box and this application gives
# it no padding, so it is zero -- it was eight when the rows sat in a
# padded column.
MAIL_LIST_TOP = 0

# How many rows to draw before the client has said which it wants. Two
# screenfuls at the tallest window anybody has: the first frame after a
# mount is drawn from this, and the `window` event replaces it.
MAIL_WINDOW = 30

# The box a row grows on its left when anything is marked, and it is the
# whole of what marking costs the layout: the card is still exactly the
# measure wide, so the scroll arithmetic above is still true. What gives
# the gutter its room is the text beside it (`mail_row`).
MAIL_PICK_W = 20

# ------------------------------------------------------------- the elements
#
# `node`, `column`, `row` and `text` were four defs here until the composer
# needed `markdown_editor`, which needs the rest of the catalogue. They are
# now the catalogue's own, vendored beside this file by
# `scripts/sync-catalogue.sh` and byte for byte the demo application's --
# which is where they are edited, and the only place.

def set_key(h, k, v)
  out = h.merge({})
  out[k] = v
  out
end

def clamp(n, lo, hi)
  return lo if n < lo
  return hi if n > hi

  n
end

# The measure, and the gutter that centres it. HEY's column does not grow
# with the window; it stops being readable long before the window stops
# being wide.
def mail_layout(state)
  view = state["viewport"] ?? {}
  w = view["width"] ?? 1280
  h = view["height"] ?? 900
  roomy = w >= 720
  measure = w - (roomy ? 96 : 40)
  measure = 700 if measure > 700
  measure = 260 if measure < 260
  {"w": w, "h": h, "roomy": roomy, "measure": measure, "pad": roomy ? 8 : 5}
end

# ------------------------------------------------------------- the controls

# A control that reads as a word rather than a widget, with the key that
# does the same thing set beside it in mono. HEY labels its buttons; this
# one labels the keyboard, because the keyboard is the point.
def mail_action(label, key, event)
  {
    "k": "box",
    "s": {
      "display": "row", "align": "center", "gap": 2,
      "pad": [1, 2, 1, 2], "radius": 2, "cursor": "pointer", "shrink": 0,
      "transition": "fast"
    },
    "on": {
      "click": event,
      "pointer_enter": {"local": "self.style = @lit", "styles": {"lit": mail_action_style(true)}},
      "pointer_leave": {"local": "self.style = @rest", "styles": {"rest": mail_action_style(false)}}
    },
    "p": {"role": "button", "label": label},
    "c": [
      text(label, {"size": 1, "weight": "semibold", "fg": "accent.base"}),
      text(key, {"font": "mono", "size": 0, "fg": "text.muted"})
    ]
  }
end

def mail_action_style(lit)
  {
    "display": "row", "align": "center", "gap": 2,
    "pad": [1, 2, 1, 2], "radius": 2, "cursor": "pointer", "shrink": 0,
    "bg": lit == true ? "surface.raised" : "none"
  }
end

# The one real button in the application, on the one screen that wants a
# target rather than a word.
def mail_submit(label, event)
  {
    "k": "box",
    "s": {
      "display": "row", "align": "center", "justify": "center",
      "pad": [3, 6, 3, 6], "radius": 3, "cursor": "pointer",
      "bg": "accent.base", "shrink": 0, "transition": "fast"
    },
    "on": {"click": event},
    "p": {"role": "button", "label": label},
    "c": [text(label, {"size": 2, "weight": "bold", "fg": "text.inverted"})]
  }
end

# A field with the label above it and a single rule under it -- the only
# borders in the application, and they are here because a place to type
# has to look like one.
def mail_field(label, value, event, width, secret, first)
  props = {"label": label}
  props["secret"] = true if secret == true
  props["autofocus"] = true if first == true
  column({"gap": 2, "width": width}, [
    text(label, {"size": 0, "weight": "semibold", "fg": "text.muted", "width": width}),
    {
      "k": "input",
      "t": value,
      "s": {
        "width": width, "pad": [3, 1, 3, 1], "size": 2,
        "bg": "none", "fg": "text.default",
        "border": [0, 0, 2, 0], "border_color": "border.default"
      },
      "p": props,
      "on": {"change": event, "submit": "link"}
    }
  ])
end

# ------------------------------------------------------------------ the mail
#
# One connection per action, opened and logged out inside the event that
# needed it. A live IMAP socket held across events would have to survive a
# server restart, a session that wandered off and a Gmail idle timeout,
# and none of those is worth carrying to read twenty messages.
def mail_pull_range(address, app_password, skip, want, box = "")
  held = mail_box_info(address, app_password, box)
  return {"error": mail_refused(address)} if held.nil?

  box = held["box"]
  exists = held["info"]["exists"] ?? 0
  return {"msgs": [], "total": 0} if exists <= 0

  # Newest first, so the batch already fetched is the `skip` newest and
  # this one starts just below it. Sequence numbers, not UIDs: `select`
  # hands back how many there are, and the newest is always that number.
  top = exists - skip
  return {"msgs": [], "total": exists} if top < 1

  floor = top - want + 1
  floor = 1 if floor < 1
  # Headers, in one command, newest last -- then reversed.
  #
  # This used to be `fetch(seq)` in a loop: twenty whole messages, every
  # attachment included, one command and one wait apiece. A mailbox of
  # twenty cost about a megabyte and the window sat still for all of it.
  # A list only ever draws a sender, a subject and a date, so that is all
  # that is asked for, and `lo:hi` asks once.
  got = box.fetch_headers_range(floor, top) rescue []
  out = []
  i = got.length() - 1
  while i >= 0
    out = out.concat([mail_summary(got[i], 0, false)])
    i = i - 1
  end
  {"msgs": out, "total": exists}
end

# ------------------------------------------------------- the connection
#
# One live IMAP connection per address, kept between events.
#
# Measured, because it was not what it looked like: signing in and saying
# hello costs about 1200 ms to Gmail, and the work afterwards -- selecting
# the mailbox and fetching five messages' headers -- costs about nothing.
# Every action here used to open a connection, do microseconds of work and
# hang up, so a delete, a mark-as-read, a body and each batch of the walk
# each paid a full second of TLS and `LOGIN` for nothing.
#
# That second is also exactly the freeze. A session's events are
# serialised behind its own frame lock (`serve/eui/mod.rs`), by design, so
# that two frames of one session cannot race -- which means the handshake
# is not merely slow, it is a second in which that window answers no key.
# More workers do not touch this: they parallelise *across* sessions, not
# within one.
#
# So the connection is kept. `SELECT` is re-issued each time, which is one
# cheap round trip and also the liveness check: Gmail drops an idle IMAP
# connection after a while, and a dead one fails there rather than
# somewhere less obvious. On any failure the entry is dropped and a fresh
# connection made, so a stale socket costs one retry and never a wedged
# application.
#
# Each worker interpreter keeps its own, which is the right granularity:
# two workers get two connections, and neither ever interleaves commands
# on the other's.
MAIL_OPEN = {}
MAIL_INFO = {}
# Which mailbox each kept connection is currently looking at. A connection
# has exactly one selected mailbox, and asking for a different folder
# means a `SELECT` -- so this is what tells a caller whether it is already
# where it wants to be.
MAIL_SEL = {}

def mail_box(address, app_password, want = "")
  box = want == "" ? MAIL_BOX : want
  kept = MAIL_OPEN[address]
  unless kept.nil?
    return {"box": kept, "info": MAIL_INFO[address] ?? {}} if MAIL_SEL[address] == box

    # The right connection, the wrong folder: one `SELECT` and it is the
    # right one. A failed re-select means the socket is gone, which the
    # caller's retry handles by opening a fresh one.
    info = kept.select(box) rescue nil
    if info.nil?
      mail_box_drop(address)
      return nil
    end
    MAIL_SEL[address] = box
    MAIL_INFO[address] = info
    return {"box": kept, "info": info}
  end

  # Where this address lives, as the account layer learned it
  # (`mail_learn`). Gmail's defaults when nobody said otherwise, which is
  # what keeps every caller below this line address-only.
  where = MAIL_WHERE[address] ?? {}
  fresh = Imap.new(
    where["host"] ?? MAIL_HOST, address, app_password,
    {"port": where["port"] ?? 993}
  ) rescue nil
  return nil if fresh.nil?

  info = fresh.select(box) rescue nil
  if info.nil?
    MAIL_OPEN[address] = nil
    return nil
  end

  MAIL_OPEN[address] = fresh
  MAIL_INFO[address] = info
  MAIL_SEL[address] = box
  {"box": fresh, "info": info}
end

# The same, with the mailbox's counts refreshed. Only the two callers that
# need `exists` or `uidvalidity` pay for it -- a `SELECT` is a round trip
# (~200 ms to Gmail) and a delete has no use for one.
def mail_box_info(address, app_password, want = "")
  held = mail_box(address, app_password, want)
  return nil if held.nil?

  info = held["box"].select(want == "" ? MAIL_BOX : want) rescue nil
  if info.nil?
    mail_box_drop(address)
    again = mail_box(address, app_password)
    return nil if again.nil?

    return again
  end
  MAIL_INFO[address] = info
  {"box": held["box"], "info": info}
end

def mail_box_drop(address)
  MAIL_OPEN[address] = nil
  MAIL_INFO[address] = nil
  MAIL_SEL[address] = nil
end

# Every folder the server has, once per address per process.
#
# `LIST` answers with the names and their SPECIAL-USE attributes, which is
# what tells Sent from Drafts from a folder somebody made -- and it is the
# same round trip `mail_trash_name` was already making for the trash
# alone. `\Noselect` folders are containers, not mailboxes: Gmail's
# `[Gmail]` is one, and selecting it is an error.
MAIL_FOLDERS_OF = {}

def mail_folders(address, app_password)
  kept = MAIL_FOLDERS_OF[address]
  return kept unless kept.nil?
  return MAIL_DEMO_FOLDERS if mail_demo?()

  held = mail_box(address, app_password)
  return [] if held.nil?

  said = held["box"].mailboxes() rescue []
  out = []
  for m in said
    flags = m["flags"] ?? []
    next if flags.filter(fn(f) { f == "\\Noselect" }).length() > 0

    name = m["name"] ?? ""
    next if name == ""

    # `label` is the same name decoded out of modified UTF-7 (RFC 3501
    # §5.1.3): a French mailbox arrives as `Messages envoy&AOk-s` and is
    # read as `Messages envoyés`. `name` stays exactly as the server sent
    # it, because that is what a `SELECT` must be given.
    out.push({"name": name, "label": m["label"] ?? name, "kind": mail_folder_kind(name, flags)})
  end
  out = mail_folder_order(out)
  MAIL_FOLDERS_OF[address] = out if out.length() > 0
  out
end

# What a folder is *for*, from its SPECIAL-USE attribute -- and from its
# name for INBOX, which is the one mailbox every server has and the one
# that carries no attribute.
def mail_folder_kind(name, flags)
  return "inbox" if name.upcase() == "INBOX"

  said = ""
  for f in flags
    said = "sent" if f == "\\Sent"
    said = "drafts" if f == "\\Drafts"
    said = "trash" if f == "\\Trash"
    said = "junk" if f == "\\Junk"
    said = "archive" if f == "\\All" || f == "\\Archive"
    said = "flagged" if f == "\\Flagged"
  end
  said
end

# The order a person expects: the inbox, then the folders the server gave
# a meaning to, then everything else as it came. Not alphabetical --
# "Archive, Drafts, Inbox, Sent, Trash" is alphabetical and nobody thinks
# of their mail that way.
MAIL_FOLDER_RANK = ["inbox", "archive", "sent", "drafts", "flagged", "junk", "trash"]

def mail_folder_order(folders)
  out = []
  for want in MAIL_FOLDER_RANK
    for one in folders
      out.push(one) if (one["kind"] ?? "") == want
    end
  end
  for one in folders
    out.push(one) if (one["kind"] ?? "") == ""
  end
  out
end

# What a folder is called on screen. Gmail puts everything under
# `[Gmail]/`, which is an implementation detail of Gmail's namespace and
# not a thing to read twelve times down the side of a window.
def mail_folder_name(one)
  said = one["label"] ?? ""
  said = one["name"] ?? "" if said == ""
  at = said.index_of("/") rescue 0 - 1
  said = said.substring(at + 1, said.length()) if at >= 0 && said.starts_with("[")
  said == "INBOX" ? "Inbox" : said
end

# The demo has no server to ask, and a rail with nothing in it would say
# the wrong thing about what this screen is.
MAIL_DEMO_FOLDERS = [
  {"name": "INBOX", "label": "INBOX", "kind": "inbox"},
  {"name": "[Gmail]/All Mail", "label": "[Gmail]/All Mail", "kind": "archive"},
  {"name": "[Gmail]/Sent Mail", "label": "[Gmail]/Sent Mail", "kind": "sent"},
  {"name": "[Gmail]/Drafts", "label": "[Gmail]/Drafts", "kind": "drafts"},
  {"name": "[Gmail]/Spam", "label": "[Gmail]/Spam", "kind": "junk"},
  {"name": "[Gmail]/Trash", "label": "[Gmail]/Trash", "kind": "trash"}
]

# One command on the kept connection, retried once on a fresh one.
#
# Reusing a connection without re-selecting saves a round trip on every
# action, at the cost of not noticing that Gmail closed it -- it drops an
# idle IMAP connection after about half an hour, and this application
# sits idle by design. So the command speaks first and asks questions
# afterwards: if it fails, the connection is thrown away and the whole
# thing tried once on a new one. The normal case costs one turn; the
# stale case costs a reconnect rather than an error on the screen.
def mail_do(address, app_password, work, box = "")
  held = mail_box(address, app_password, box)
  return nil if held.nil?

  got = work(held["box"]) rescue nil
  return got unless got.nil?

  mail_box_drop(address)
  again = mail_box(address, app_password, box)
  return nil if again.nil?

  work(again["box"]) rescue nil
end

# Drop the connection on the way out, so signing out does not leave a
# mailbox open on a credential that has just been deleted.
def mail_box_close(address)
  kept = MAIL_OPEN[address]
  return if kept.nil?

  kept.logout() rescue nil
  mail_box_drop(address)
end

# Only what is not here yet.
#
# The point of keeping a mailbox is not to have to ask for it again. A
# refresh with twenty messages already on disk should cost one round trip
# that usually answers "nothing new", not twenty `BODY.PEEK[]` downloads
# of mail you are already looking at.
#
# UIDs are what make that possible and sequence numbers are not: a UID is
# stable for the life of a mailbox, so "everything above the highest one I
# hold" is a question the server can answer. `UID n:*` is the IMAP way to
# ask it -- with the wrinkle that the range always matches at least the
# last message even when nothing is new, so the answer is filtered rather
# than trusted.
#
# `uidvalidity` is the escape hatch. If it changes, every UID this
# application stored means something else now, and the only correct thing
# is to throw the copy away and walk the mailbox again.
# Is there anything new? One command, and no `SELECT`.
#
# A refresh asks three questions -- how many messages there are, which
# UIDs are above the highest held, and which of the held ones are still
# there -- and a poll only needs the middle one. The mailbox is already
# selected on the kept connection, and a `UID SEARCH` is evaluated
# against the mailbox as it is now, so the count and the validity are
# only wanted once the answer is "yes". Measured on this account: the
# `SELECT` this skips is 182 ms of the 346 ms a poll used to hold the
# frame lock for, and the spinner it skips is the one that flashed every
# ninety seconds to say "nothing".
# How much is in a mailbox, without opening it.
#
# Through `mail_do`, so a connection Gmail dropped while we were reading
# is one retry rather than a folder that never gets a number.
def mail_status(address, app_password, name)
  mail_do(address, app_password, fn(box) { box.status(name) rescue nil })
end

def mail_peek(address, app_password, high, box = "")
  held = mail_box(address, app_password, box)
  return nil if held.nil?

  found = held["box"].uid_search("UID " + str(high + 1) + ":*") rescue nil
  return nil if found.nil?

  found.filter(fn(u) { u > high })
end

def mail_pull_new(address, app_password, high, low, was_valid, want, had = 0, box = "")
  t0 = mail_ms()
  held = mail_box_info(address, app_password, box)
  t1 = mail_ms()
  return {"error": mail_refused(address)} if held.nil?

  box = held["box"]
  info = held["info"]
  valid = info["uidvalidity"] ?? 0
  return {"reset": true, "uidvalidity": valid} if was_valid > 0 && valid > 0 && valid != was_valid

  exists = info["exists"] ?? 0
  found = box.uid_search("UID " + str(high + 1) + ":*") rescue []
  t2 = mail_ms()
  fresh = found.filter(fn(u) { u > high })

  # And which of the ones already held are still in the mailbox.
  #
  # The top-up only ever *added*: it asks for UIDs above the highest one
  # stored, so a message deleted from Gmail's own interface -- or from
  # another client, or moved to a label -- stayed in the list for ever as
  # a row that could not be opened, because the letter behind it was
  # gone. `UID lo:hi` answers with the ones that are still there; anything
  # held and not named has left, and is dropped.
  #
  # An empty answer is treated as "could not ask" rather than "everything
  # is gone": a failed search must never be allowed to wipe the mailbox.
  #
  # And it is only asked when something could have changed. `SELECT` has
  # just said how many messages the mailbox holds: if that is the number
  # the store already had, and the search above the highest UID found
  # nothing, then nothing arrived and nothing left -- one of those would
  # have moved the count. Skipping it is a third of the round trips of a
  # poll that finds nothing, which is what nearly every poll finds, and
  # every one of those round trips is held under the session's frame
  # lock.
  quiet = fresh.length() == 0 && had > 0 && exists == had
  alive = []
  alive = box.uid_search("UID " + str(low) + ":" + str(high)) rescue [] if quiet != true
  # One command for all of them.
  #
  # This was `fetch_headers_uid` in a loop: one round trip per new
  # message, all of them inside the one event that asked -- so twenty
  # messages arriving while the window was closed meant twenty commands
  # under the session's frame lock, which is the freeze. `UID FETCH` takes
  # a set, and the newest `want` of them is a set.
  take = fresh
  take = fresh.slice(fresh.length() - want, fresh.length()) if fresh.length() > want
  t3 = mail_ms()
  got = mail_headers_of(box, take)
  t4 = mail_ms()
  mail_say("refresh: select " + str(t1 - t0) + " ms, search " + str(t2 - t1) + " ms, alive " + str(t3 - t2) + " ms, headers(" + str(take.length()) + ") " + str(t4 - t3) + " ms")
  out = []
  i = got.length() - 1
  while i >= 0
    out.push(mail_summary(got[i], 0, false))
    i = i - 1
  end
  {"msgs": out, "total": exists, "uidvalidity": valid, "alive": alive}
end

# Move one message to the trash, addressed by UID.
#
# The addressing is the delicate part. Every mutating verb in this Imap
# takes a **sequence number**, and a sequence number is a position: it
# shifts whenever anything before it is removed, so a stored one is a
# loaded gun -- act on a stale position and you delete a message the
# person never chose. UIDs do not move, so the UID is turned into a
# position immediately before it is used: `SEARCH UID n` answers with the
# sequence number that UID has *right now*, and that is what is moved.
def mail_trash_uid(address, app_password, uid, here = "")
  where = mail_trash_name(address, app_password)

  # `UID MOVE`, not `SEARCH` then `MOVE`.
  #
  # Every mutating verb in IMAP has a sequence-number form and a UID form.
  # A sequence number is a position and moves whenever anything before it
  # is removed, so a client holding a UID -- which is what one that stores
  # messages actually has -- had to convert first. That conversion is a
  # whole round trip, ~200 ms against Gmail, and every one of those is
  # time the window answers no key, because a session's events are
  # serialised behind its frame lock. Addressing by the handle we already
  # hold makes deleting one turn instead of three.
  mail_say("trash mailbox is '" + str(where) + "'")
  moved = mail_do(address, app_password, fn(box) { box.uid_move(uid, where) }, here)
  return "" unless moved.nil?

  # Some servers have no MOVE. Flagging is the older way to say it, and on
  # Gmail it archives rather than trashes -- still recoverable, from All
  # Mail. `EXPUNGE` is deliberately not sent: it would remove *every*
  # flagged message in the mailbox, not the one asked about.
  flagged = mail_do(address, app_password, fn(box) { box.uid_delete(uid) }, here)
  return "Gmail would not move that message to " + where + "." if flagged.nil?

  ""
end

# Which mailbox is the trash, asked rather than assumed.
#
# `[Gmail]/Trash` is only its name on an English account. Gmail localises
# the special folders, so a French one has `[Gmail]/Corbeille` and the
# hardcoded name simply does not exist -- every delete failed with
# "would not move to the trash", which named the symptom and hid the
# cause. Worse, that account also had a *user* folder called `Trash`,
# so a looser guess would have quietly filed mail in the wrong place.
#
# The server knows. `LIST` returns SPECIAL-USE attributes, and the trash
# is the mailbox flagged `\\Trash` whatever it is called. One round trip,
# once per address per process, then remembered.
MAIL_TRASH_OF = {}

# `MAIL_DEBUG=1` makes the application say what it is doing to the server
# log. Off by default and silent; on, it is the difference between "it
# does not work" and a line naming the mailbox, the UID and the answer.
def mail_trash_name(address, app_password)
  kept = MAIL_TRASH_OF[address]
  return kept unless kept.nil?

  held = mail_box(address, app_password)
  return MAIL_TRASH if held.nil?

  boxes = held["box"].mailboxes() rescue []
  found = ""
  for m in boxes
    flags = m["flags"] ?? []
    marked = flags.filter(fn(f) { f == "\\Trash" }).length() > 0
    found = m["name"] if found == "" && marked
  end
  name = found == "" ? MAIL_TRASH : found
  MAIL_TRASH_OF[address] = name
  name
end

# Searching the whole mailbox, which is the only kind worth having here.
#
# Filtering the twenty messages on hand is instant and nearly useless: the
# inbox behind them holds ten thousand. IMAP can answer the real question
# -- `SEARCH TEXT "..."` is matched by the server, across headers and
# bodies, without any of it crossing the wire -- and it answers with UIDs,
# which is exactly the handle the batch loader already takes.
#
# The query is quoted into an IMAP string, so the two characters that can
# end one early are removed rather than escaped. A search is a convenience;
# losing a backslash out of it costs nothing, and getting the quoting
# wrong costs a malformed command.
def mail_search_uids(address, app_password, query, want, box = "")
  held = mail_box(address, app_password, box)
  return {"error": "Could not reach Gmail to search."} if held.nil?

  found = held["box"].uid_search("TEXT \"" + mail_quotable(query) + "\"") rescue []

  # Newest first, and no more than the loader will ever ask for.
  out = []
  i = found.length() - 1
  while i >= 0 && out.length() < want
    out = out.concat([found[i]])
    i = i - 1
  end
  {"uids": out, "total": found.length()}
end

def mail_quotable(query)
  query.replace("\\", " ").replace("\"", " ").trim()
end

# The batch loader's third mode: fetch the next few of a list of UIDs the
# search already found.
# The headers of a set of UIDs, in one command, newest last.
#
# The list is sorted by the server, so what comes back is in UID order
# whatever order it was asked in; the caller reverses it.
def mail_headers_of(box, uids)
  return [] if uids.length() == 0

  set = uids.map(fn(u) { str(u) }).join(",")
  box.fetch_headers_set(set) rescue []
end

def mail_pull_uids(address, app_password, uids, skip, want)
  held = mail_box(address, app_password)
  return {"error": mail_refused(address)} if held.nil?

  box = held["box"]
  stop = skip + want
  stop = uids.length() if stop > uids.length()
  return {"msgs": []} if skip >= stop

  got = mail_headers_of(box, uids.slice(skip, stop))
  # The order asked for is the order wanted -- newest first, as the search
  # returned them -- and a `UID FETCH` answers in UID order, so the answer
  # is put back the way it was asked.
  wanted = uids.slice(skip, stop)
  out = []
  for u in wanted
    found = got.filter(fn(m) { (m["uid"] ?? 0) == u })
    out.push(mail_summary(found[0], 0, false)) if found.length() > 0
  end
  {"msgs": out}
end

# Set \\Seen on one message, addressed by UID for the same reason `d` is:
# `mark_seen` takes a sequence number, and a sequence number is a position
# that moves.
def mail_mark_seen(address, app_password, uid, here = "")
  mail_do(address, app_password, fn(box) { box.uid_mark_seen(uid) }, here)
end

# One whole message, for reading. The only place `BODY.PEEK[]` is still
# asked for -- everywhere else takes headers.
def mail_pull_body(address, app_password, uid, here = "")
  # Same retry as `mail_pull_parts`, and for the same reason: opening a
  # letter after an idle spell used to fail once and say the message
  # would not come down.
  # `fetch_uid_text`, not `fetch_uid`: the shape of the message first
  # (`BODYSTRUCTURE`, which the server sends with every header anyway),
  # then only the parts that are the letter's faces. A mail with
  # seventeen photographs on it is two megabytes of which the text is
  # four thousand bytes -- and it was fetched whole to show those four
  # thousand, and fetched whole *again* when `p` wanted the photographs.
  # Now neither of them brings down the other's bytes.
  #
  # The attachments still arrive listed -- name, type, size and part
  # number -- because the structure says all of that without a byte of
  # them being downloaded. `mail_pull_parts` asks for the bytes, by
  # number, when somebody presses `p`.
  got = mail_do(address, app_password, fn(box) { box.fetch_uid_text(uid) rescue nil }, here)
  return nil if got.nil?

  mail_summary(got, 0, true)
end

# The lowest UID on hand, which is where the "are these still there"
# question starts.
def mail_low(msgs)
  low = 0
  for one in msgs
    uid = one["uid"] ?? 0
    low = uid if uid > 0 && (low == 0 || uid < low)
  end
  low
end

# The highest UID on hand, which is where the next fetch starts.
def mail_high(msgs)
  high = 0
  for one in msgs
    uid = one["uid"] ?? 0
    high = uid if uid > high
  end
  high
end

# Everything the screens need from one message, and nothing else. The raw
# hash also carries `raw` -- the whole RFC822 source, attachments and all
# -- and putting that in session state would diff it on every keystroke.
def mail_summary(msg, seq, full = false)
  who = msg["from"] ?? {}
  subject = msg["subject"] ?? ""
  address = who["address"] ?? ""
  name = who["name"] ?? ""
  flags = msg["flags"] ?? []
  {
    "uid": msg["uid"] ?? seq,
    "subject": subject == "" ? "(no subject)" : mail_cap(subject, 400),
    "name": name == "" ? (address == "" ? "unknown sender" : mail_cap(address, 200)) : mail_cap(name, 200),
    "address": mail_cap(address, 200),
    "date": mail_when(msg["date"] ?? ""),
    "seen": flags.filter(fn(f) { f == "\\Seen" }).length() > 0,
    "body": mail_body(msg),
    # Whether the letter itself is here, or only the four header lines a
    # list draws. Opening a message that is not loaded fetches it.
    #
    # Told, not inferred. Guessing from "is there a body string" looked
    # sound and was wrong: a `HEADER.FIELDS` response parses to an
    # `html_body` of twenty-six characters of residue, so every header
    # row claimed to be a whole message, nothing was ever fetched on
    # open, and the reader drew that residue -- an empty letter.
    "loaded": full == true,
    # The HTML part is kept beside the text one rather than instead of
    # it: `h` switches between them and a mail that has only one of them
    # still has that one.
    "html": mail_html_part(msg),
    # And the third face, when a message carries one. `text_body` and
    # `html_body` answer "the plain one" and "the HTML one" and neither
    # will ever return a `text/markdown` part, so it is picked out of the
    # parts list -- which exists for this.
    "md": mail_md_part(msg),
    # What came attached. The parser has always listed them -- name, type
    # and size -- and this application has always thrown the list away, so
    # a mail with a PDF on it looked exactly like a mail without one.
    # Kept as three fields per attachment and nothing else: the bytes are
    # not fetched, and nothing here pretends they are.
    "atts": mail_atts_of(msg),
    # What a row shows without opening anything: how big the message is
    # and how many paper clips it has. Both ride along with the header
    # fetch -- `RFC822.SIZE` and `BODYSTRUCTURE` in the same command -- so
    # a list costs what it always cost, plus a few bytes a row.
    "bytes": msg["bytes"] ?? 0,
    "clips": msg["clips"] ?? 0
  }
end

# Raw text, which is what was asked for. A message with no plain-text part
# is flattened out of its HTML rather than shown as markup, and one with
# neither says so.
def mail_body(msg)
  said = msg["text_body"] ?? ""
  if said == ""
    html = msg["html_body"] ?? ""
    said = strip_html(html) rescue "" unless html == ""
  end
  return "" if said == ""
  return said.substring(0, MAIL_BODY_CAP) + "\n\n[...truncated]" if said.length() > MAIL_BODY_CAP

  said
end

# A body as a list of text-node-sized pieces, cut on line boundaries so
# no chunk ever splits a line down the middle.
def mail_chunks(body)
  flat = body.replace("\r", "")
  lines = []
  for line in flat.split("\n")
    lines = lines.concat(mail_long_line(line))
  end
  out = []
  held = ""
  n = 0
  for piece in lines
    room = held.bytesize() + piece.bytesize() + 1
    if n == 0
      held = piece
    elsif room > MAIL_CHUNK
      out = out.concat([held])
      held = piece
    else
      held = held + "\n" + piece
    end
    n = n + 1
  end
  out = out.concat([held]) if n > 0
  out
end

def mail_long_line(line)
  return [line] if line.bytesize() <= MAIL_CHUNK

  out = []
  rest = line
  while mail_len(rest) > MAIL_WRAP
    out = out.concat([rest.substring(0, MAIL_WRAP)])
    rest = rest.substring(MAIL_WRAP, mail_len(rest))
  end
  out = out.concat([rest]) if mail_len(rest) > 0
  out
end

# The HTML part, capped for the same reason the text one is: all of it
# lives in session state and state is diffed on every event.
def mail_atts_of(msg)
  out = []
  for one in (msg["attachments"] ?? [])
    name = one["name"] ?? ""
    name = "(sans nom)" if name == ""
    out.push({
      "name": mail_cap(name, 120),
      "type": one["content_type"] ?? "",
      "size": one["size"] ?? 0,
      # Which part of the message it is, so `p` can ask for it by number
      # instead of asking for the message again. Empty for anything
      # fetched by an older build, and `mail_pull_parts` falls back to
      # the whole message when it is.
      "part": one["part"] ?? ""
    })
  end
  out
end

# The source a message was written in, if it says it carries one.
def mail_md_part(msg)
  found = ""
  for part in (msg["parts"] ?? [])
    said = (part["content_type"] ?? "").downcase()
    if said.starts_with("text/markdown") || said.starts_with("text/x-markdown")
      found = part["body"] ?? "" if found == ""
    end
  end
  mail_cap(found, MAIL_HTML_CAP)
end

def mail_html_part(msg)
  said = msg["html_body"] ?? ""
  return "" if said == ""
  return said.substring(0, MAIL_HTML_CAP) if said.length() > MAIL_HTML_CAP

  said
end

# The date arrives as RFC 3339. Neither screen wants the offset.
def mail_when(raw)
  return "" if raw == ""

  raw.substring(0, 16).replace("T", " ")
end

# A row is a fixed height, so a subject that would wrap is cut instead --
# the arithmetic that carries the cursor past the fold depends on every
# row being `MAIL_ROW_H` and nothing else.
# A header line is one text node too, and a malformed message can carry a
# subject far past 4096 bytes. Cut every one of them at the source rather
# than trusting a remote sender to be reasonable.
def mail_fit(said, width, size_px)
  room = width / size_px
  return said if mail_len(said) <= room
  return "" if room < 2

  said.substring(0, room - 1) + "…"
end


# --------------------------------------------------------------- the demo
#
# `MAIL_DEMO=1 soli serve examples/mail-app` opens straight into the list
# with the sample below instead of the sign-in screen. It exists because
# the two screens worth looking at are the two you cannot reach without an
# account, and a design nobody can see is a design nobody reviewed. It
# touches nothing on the real path: without the variable this function is
# never called.
def mail_sample
  [
    ["Ana Vieira", "ana@fieldnotes.co", "Re: the pricing page copy — second pass", "2026-09-17 09:12", false,
     "I took another run at the middle section. The thing that was bothering me is that we lead with the tiers, and nobody knows what a tier is worth until after they know what the thing does.\n\nSo: swap them. What it does, then what it costs. I have a draft in the doc, third heading down.\n\nOne open question — do we keep the annual discount on the card, or move it to the checkout? I lean toward the card. People decide before they click.\n\nAna"],
    ["Postmaster", "mailer-daemon@googlemail.com", "Delivery Status Notification (Failure)", "2026-09-17 08:47", false,
     "Address not found.\n\nYour message wasn't delivered to hello@exmaple.com because the domain exmaple.com couldn't be found. Check for typos or unnecessary spaces and try again.\n\nThe response was:\nDNS Error: 21024024 DNS type 'mx' lookup of exmaple.com responded with code NXDOMAIN"],
    ["Devon Park", "devon@park.dev", "that IMAP thing you were asking about", "2026-09-16 22:03", false,
     "Short version: Gmail will not take your account password on port 993 any more, and it has not for a while. You need an app password, which means 2FA on the account first.\n\nLonger version: XOAUTH2 also works and is what the official clients use, but it wants a Cloud project and a consent screen, which for a client only you will ever run is a lot of ceremony for the same sixteen characters.\n\nd"],
    ["Rita Osei", "rita@osei.studio", "invoice 2026-114", "2026-09-16 16:30", true,
     "Attached, and also below in case the attachment gives you trouble.\n\nInvoice 2026-114\nSeptember retainer — 4 days\nDue 30 September\n\nNo rush. Rita"],
    ["GitHub", "notifications@github.com", "[soli/eui] Run failed: build — main (d9306db)", "2026-09-16 14:18", true,
     "build: macOS wrap step failed\n\n  scripts/wrap-macos-app.sh: line 44: codesign: command not found\n\nView the run: https://github.com/soli/eui/actions/runs/000000000"],
    ["Marguerite Delacroix-Fontaine", "m.delacroix@conservatoire-national-superieur.fr", "Programme for the October recital, and a question about the hall", "2026-09-16 11:52", true,
     "Dear all,\n\nThe programme is settled: Ravel first, then the Debussy, and the Fauré after the interval.\n\nThe question is the hall. The Salle Cortot is free on the 14th but not the 15th, and the piano there is the one we complained about last year.\n\nMarguerite"],
    ["Tom", "tom@basecamp.example", "no subject really", "2026-09-15 19:40", true,
     "just checking this renders. it does."],
    ["Solaris Billing", "billing@solisoft.net", "Your receipt — September", "2026-09-15 09:00", true,
     "Thanks. Nothing is owed until October.\n\nPlan: Workshop\nPeriod: 1–30 September 2026\nCharged: 0.00 EUR (annual, paid)"]
  ]
end

# A body past 4096 bytes, with accents in it, because the budget the
# client enforces is bytes and a French paragraph spends more of them than
# `length` suggests. This is the message that proves `mail_chunks` works.
def mail_long_sample
  para = "Nous avons relu le dossier entièrement, et la conclusion n'a pas changé : le problème n'était pas la mesure elle-même mais l'unité dans laquelle on la publiait. Chaque chiffre était juste ; aucun n'était comparable à celui d'à côté."
  out = "Chère lectrice, cher lecteur,\n\n"
  i = 0
  while i < 60
    out = out + para + "\n\n"
    i = i + 1
  end
  # And one line with no break in it at all, which is what a signed URL or
  # a base64 part looks like: `mail_chunks` cannot cut it on a newline, so
  # `mail_long_line` cuts it by hand.
  blob = ""
  j = 0
  while j < 90
    blob = blob + "ZmFrZS1iYXNlNjQtcGF5bG9hZC1ub3QtcmVhbGx5LWJ1dC1sb25nLWVub3VnaA=="
    j = j + 1
  end
  out = out + blob + "\n\n"
  out + "Bien à vous,\nLa rédaction"
end

def mail_seed(state)
  said = mail_sample()
  msgs = range(0, said.length()).map(fn(n) {
    one = said[n]
    {
      "uid": 9000 + n,
      "name": one[0],
      "address": one[1],
      "subject": one[2],
      "date": one[3],
      "seen": one[4],
      "body": one[5],
      "html": "",
      # The sample carries its letters already. Without this the demo
      # asks for a body it has no account to fetch, and every message
      # opens on "Fetching the letter…" that never resolves.
      "loaded": true
    }
  })
  msgs = msgs.concat([{
    "uid": 9901,
    "name": "Bien’ici",
    "address": "alertes@bienici.com",
    "subject": "1 nouvelle annonce correspond à votre alerte",
    "date": "2026-09-14 06:30",
    "seen": true,
    "loaded": true,
    "body": "Bien’ici [https://mail-sender.bienici.com/static/emails/logo-bienici.png]\n[https://www.bienici.com/?at_canal=CRM&at_medium=alertes]  [https://mail-sender.bienici.com/static/emails/transparent.png]\nBonne nouvelle, 1 nouvelle annonce correspond à votre alerte !",
    "html": "<html><head><style>.a{color:red}</style></head><body><table><tr><td><img src=\"https://mail-sender.bienici.com/static/emails/logo-bienici.png\" alt=\"Bien ici\" width=\"160\" height=\"40\"></td></tr><tr><td><p>Bonne nouvelle, <b>1 nouvelle annonce</b> correspond &agrave; votre alerte !</p></td></tr><tr><td><a href=\"https://www.bienici.com/recherche\">Voir l&rsquo;annonce</a></td></tr><tr><td><img src=\"https://mail-sender.bienici.com/static/emails/transparent.png\" alt=\"\" width=\"1\" height=\"1\"></td></tr><tr><td><p>Acheter - Maison, appartement, loft - 300k &euro; max</p></td></tr></table></body></html>"
  }, {
    "uid": 9902,
    "name": "Spitogatos",
    "address": "no-reply@spitogatos.gr",
    "subject": "Βρήκαμε 1 αγγελία που σου ταιριάζει!",
    "date": "2026-09-14 05:12",
    "seen": true,
    "loaded": true,
    "body": "Βρήκαμε 1 αγγελία που σου ταιριάζει!\n\nHouses to rent (search on map).\nhttps://www.spitogatos.gr/enoikiaseis-katoikies/anazitisi-xarti/radius-10950?utm_source=saved_search\n\n1 νέα αγγελία:\n\n&#8364; 600\nΚέντρο (Αντίκυρα)\nΔες την αγγελία: https://www.spitogatos.gr/aggelia/2120859497?utm_source=saved_search\n\nSpitogatos",
    "html": ""
  }, {
    "uid": 9900,
    "name": "Le Télégraphe",
    "address": "lettre@letelegraphe.org",
    "subject": "La lettre de septembre — et pourquoi elle est si longue",
    "date": "2026-09-14 07:00",
    "seen": true,
    "html": "",
    "loaded": true,
    "body": mail_long_sample()
  }])
  out = set_key(state, "msgs", msgs)
  out = set_key(out, "total", msgs.length())
  out = set_key(out, "linked", true)
  out = set_key(out, "cursor", 0)
  out = set_key(out, "scroll", 0)
  # The demo has no server to ask for a folder list, and a rail is part of
  # what this screen *is* -- so it gets the one `mail_folders` would have
  # answered with, and the folders other than the inbox are empty because
  # a demo has nothing to put in them.
  out = set_key(out, "box", MAIL_BOX)
  out = set_key(out, "folders", MAIL_DEMO_FOLDERS)
  out = set_key(out, "asked_folders", true)
  # The demo has its one picture already, so opening that message shows a
  # picture rather than a placeholder -- it is the feature being
  # demonstrated, and `i` on an account that does not exist would fetch
  # nothing.
  shot = mail_get_image("https://mail-sender.bienici.com/static/emails/logo-bienici.png")
  if !shot.nil?
    out = set_key(out, "shots", {"https://mail-sender.bienici.com/static/emails/logo-bienici.png": shot})
    out = set_key(out, "images", true)
  end
  out = set_key(out, "address", "you@gmail.com")
  # Two accounts, because one account is a case that hides every question
  # the switcher has to answer: what the bar says, what the numbers mean,
  # and what happens to a list that belongs to somebody else. Neither is
  # written to disk -- `mail_accounts_save` refuses in the demo.
  out = set_key(out, "accounts", [
    mail_account("you@gmail.com", "demo-not-a-real-app-password", "", 0, "", 0),
    mail_account("you@fastmail.com", "demo-not-a-real-app-password", "imap.fastmail.com", 993, "", 0)
  ])
  out = set_key(out, "account", (out["accounts"] ?? [])[0])
  # A credential that cannot work, so pressing `r` in the demo walks the
  # whole arm -> loader -> fetch -> failure path against the real server.
  set_key(out, "secret", "demo-not-a-real-app-password")
end

# ------------------------------------------------------------- the accounts
#
# More than one, and not all of them Gmail.
#
# An account is a hash: where to read, where to send, and the one
# credential both use. Everything under this layer -- the connection
# pool, the fetches, the store -- is keyed on the **address** and knows
# nothing else, which is why adding accounts changed almost none of it:
# the host a connection should dial is looked up from the address
# (`MAIL_WHERE`) instead of being threaded down through twenty call
# sites.
#
# Where the credential is kept, and what that costs: `File` is jailed to
# this application's own folder under `soli serve` (SEC-006), so there is
# no writing to ~/.config from here -- the accounts go in `config/`,
# beside the publisher key, and `.gitignore` keeps both out of the
# repository. They are sealed with `Crypto.encrypt` when
# `SOLI_ENCRYPTION_KEY` is set and written plainly when it is not, and
# the sign-in screen says which of those happened rather than implying a
# safety it does not have.
MAIL_ACCOUNTS = "config/accounts.json"

# The one-account file this replaced. Read once, folded in, deleted.
MAIL_ACCOUNT = "config/account.json"

# address -> account. The pool dials by address (`mail_box`), and this is
# where it learns what that address means.
MAIL_WHERE = {}

def mail_sealed?
  key = getenv("SOLI_ENCRYPTION_KEY") ?? ""
  key != ""
end

# What to say when a server will not have us. Naming the host is the
# whole point: "Gmail would not accept that" in front of somebody adding
# a Fastmail account is an error message that sends them looking in the
# wrong place.
# Put the account that was just refused back into the fields, so the
# screen it lands on is the one it came from.
#
# Without this an IMAP account that failed came back as the Gmail form:
# the chip reset, the host and the port gone, and no way to correct the
# one thing that was probably wrong without typing all of it again.
def mail_refill(state)
  one = state["account"]
  return state if one.nil?

  where = one["host"] ?? MAIL_HOST
  out = set_key(state, "kind", where == MAIL_HOST ? "gmail" : "imap")
  out = set_key(out, "host", where == MAIL_HOST ? "" : where)
  out = set_key(out, "port", str(one["port"] ?? 993))
  set_key(out, "smtp", one["smtp"] ?? "")
end

def mail_refused(address)
  where = (MAIL_WHERE[address] ?? {})["host"] ?? MAIL_HOST
  return where + " would not accept that. It takes an app password here -- sixteen characters from myaccount.google.com/apppasswords -- and never your account password." if where == MAIL_HOST

  where + " would not accept that address and password. Check the server, the port and the password -- some providers want an app-specific one here rather than the one you log in with."
end

def mail_learn(one)
  MAIL_WHERE[one["address"] ?? ""] = one
  one
end

# The sending host a receiving host implies: `imap.` becomes `smtp.`,
# which is right for Gmail, Fastmail, iCloud, Proton's bridge and most of
# the rest. A server that does not follow it is the reason the field is
# there to be filled in.
def mail_smtp_of(host)
  return "smtp" + host.substring(4, mail_len(host)) if host.starts_with("imap")

  host
end

# One shape, whether it came from the sign-in screen or from the file,
# with the Gmail defaults filled in around whatever was given.
def mail_account(address, secret, host, port, smtp, smtp_port)
  where = host.trim().downcase()
  where = MAIL_HOST if where == ""
  out = {
    "address": address.trim(),
    "secret": secret,
    "host": where,
    "port": port < 1 ? 993 : port,
    "smtp": smtp.trim().downcase() == "" ? mail_smtp_of(where) : smtp.trim().downcase(),
    "smtp_port": smtp_port < 1 ? MAIL_SMTP_PORT : smtp_port
  }
  mail_learn(out)
end

def mail_accounts_load
  return [] if mail_demo?()

  raw = File.read(MAIL_ACCOUNTS) rescue ""
  return mail_accounts_fold() if raw == ""

  data = json_parse(raw) rescue nil
  return [] if data.nil?

  out = []
  for one in (data["accounts"] ?? [])
    said = one["secret"] ?? ""
    if one["sealed"] == true
      said = Crypto.decrypt(said) rescue ""
    end
    address = one["address"] ?? ""
    if said != "" && address != ""
      out = out.concat([mail_account(
        address, said, one["host"] ?? "", one["port"] ?? 0,
        one["smtp"] ?? "", one["smtp_port"] ?? 0
      )])
    end
  end
  out
end

def mail_accounts_save(list)
  return if mail_demo?()

  kept = []
  for one in list
    said = one["secret"] ?? ""
    sealed = false
    if mail_sealed?()
      out = Crypto.encrypt(said) rescue ""
      if out != ""
        said = out
        sealed = true
      end
    end
    kept = kept.concat([{
      "address": one["address"] ?? "", "secret": said, "sealed": sealed,
      "host": one["host"] ?? MAIL_HOST, "port": one["port"] ?? 993,
      "smtp": one["smtp"] ?? "", "smtp_port": one["smtp_port"] ?? MAIL_SMTP_PORT
    }])
  end
  blob = json_stringify({"accounts": kept, "at": mail_now()}) rescue ""
  return if blob == ""

  File.write(MAIL_ACCOUNTS, blob) rescue nil
end

# The one account an older build wrote, folded into the list and its file
# removed. It runs once, on the first start after the upgrade, and never
# again -- and if there was nothing there it costs one failed read.
def mail_accounts_fold
  raw = File.read(MAIL_ACCOUNT) rescue ""
  return [] if raw == ""

  data = json_parse(raw) rescue nil
  return [] if data.nil?

  said = data["secret"] ?? ""
  if data["sealed"] == true
    said = Crypto.decrypt(said) rescue ""
  end
  address = data["address"] ?? ""
  return [] if said == "" || address == ""

  list = [mail_account(address, said, "", 0, "", 0)]
  mail_accounts_save(list)
  File.delete(MAIL_ACCOUNT) rescue nil
  mail_say("folded the old single account in: " + address)
  list
end

# Add or replace, by address. Signing in again with the same address
# fixes a changed password rather than listing it twice.
def mail_accounts_add(list, one)
  rest = list.filter(fn(a) { (a["address"] ?? "") != (one["address"] ?? "") })
  out = rest.concat([one])
  mail_accounts_save(out)
  out
end

def mail_accounts_drop(list, address)
  out = list.filter(fn(a) { (a["address"] ?? "") != address })
  mail_accounts_save(out)
  out
end

def mail_accounts_find(list, address)
  found = list.filter(fn(a) { (a["address"] ?? "") == address })
  found[0]
end



# ---------------------------------------------------------------- the events

# Every event, timed.
#
# A handler holds the session's frame lock for as long as it runs, so the
# only honest measure of "the window hung" is how long one of these took.
# Anything over `MAIL_SLOW` says so in the log, with its name.
MAIL_SLOW = 50

# The margin left between two ticks: enough of the session's frame lock
# for whatever a hand pressed while the last one ran.
MAIL_ROOM = 150

# And the slowest the clock may go, so a pathological round trip does not
# turn the fetch walk into a standstill.
MAIL_TICK_MAX = 2000

def mail(event_data)
  began = mail_ms()
  out = mail_event(event_data)
  spent = mail_ms() - began
  # The clock time as well as the duration: two ticks that each take 353
  # ms are fine if they are 500 ms apart and a growing queue if they are
  # 250. Only the gap between the lines says which, and the gap is only
  # visible if the line carries when it happened.
  mail_say("SLOW " + str(event_data["event"]) + " took " + str(spent) + " ms at " + str(began) + " next in " + str(mail_pace(out.is_a?("hash") ? set_key(out, "paced", spent) : {}))) if spent >= MAIL_SLOW
  # What the last tick cost, so the next one is not asked for sooner than
  # that. See `mail_pace`.
  return set_key(out, "paced", spent) if event_data["event"] == "pull" && out.is_a?("hash")

  out
end

# How long to wait before the next `wake`, given what the last one cost.
#
# This is the whole of a six-second key press, and it is arithmetic.
# `wake` is a period, not a heartbeat: the client asks again every
# `MAIL_TICK` whether or not the last answer has come back. IMAP inside a
# session event costs about 350 ms against Gmail -- so a 250 ms period
# asks for a new round trip 100 ms before the previous one has finished,
# and the queue grows by 100 ms every tick. A minute of refreshing is six
# seconds of backlog, and the next thing pressed -- `v`, or any key --
# waits behind all of it. The log said so plainly once it was asked:
# thirty consecutive `SLOW pull took 353 ms`, then `v: open` and
# `v: built` one millisecond apart.
#
# So the period follows the work: never sooner than the last tick took,
# plus a margin for the person. Nothing is cancelled and nothing is
# cleverer; the clock simply stops outrunning the thing it drives.
def mail_pace(state)
  clamp((state["paced"] ?? 0) + MAIL_ROOM, MAIL_TICK, MAIL_TICK_MAX)
end

def mail_event(event_data)
  state = event_data["state"] ?? {}
  params = event_data["params"] ?? {}
  event = event_data["event"]
  props = params["props"] ?? {}

  # Where this account lives, taught to *this* worker, before anything
  # dials.
  #
  # `MAIL_WHERE` is a module global and a module global is per worker:
  # each one warms its own handlers and its own globals (`soli serve`
  # prints a line per worker saying so). The event that signs an account
  # in and the tick that fetches for it are two events and need not land
  # on the same one -- so the host learned while signing in was missing
  # when the fetch dialled, the lookup fell back to Gmail's, and adding a
  # Fastmail account answered "Gmail would not accept that".
  #
  # Session state is the thing that travels with the session, so state is
  # what the cache is filled from, on every event, before any of them can
  # reach `mail_box`.
  # `focus_to` is an op, not a state: left in the tree it would take the
  # caret back on every frame. So the flag `o` sets lasts exactly until
  # the next event, whatever that is.
  state = set_key(state, "browse", false) if (state["browse"] ?? false) == true && event != "key"
  # The rail's `focus_to` is the same kind of one-shot, and it ends at the
  # next event of any kind -- including the key that moves along the rail,
  # which sets it again.
  state = set_key(state, "rail_to", false) if (state["rail_to"] ?? false) == true

  here = state["account"]
  mail_learn(here) unless here.nil?

  # A door in the menu is a door: whatever it opens, the menu closes
  # behind it. Listed rather than inferred, because "every event closes
  # it" would close it on the tick that arrives while it is open.
  state = set_key(state, "sheet_menu", false) if (state["sheet_menu"] ?? false) == true && mail_menu_door?(event)

  # The viewport arrives on its own, once at connect and again on resize.
  if event == "connect"
    seen = set_key(state, "viewport", params["viewport"] ?? state["viewport"])
    linked = state["linked"] ?? false
    return mail_seed(seen) if mail_demo?() && linked != true
    return mail_resume(seen) if linked != true

    return seen
  end
  return set_key(state, "viewport", params["viewport"] ?? state["viewport"]) if event == "viewport"

  # The two fields. `payload` is the whole settled value of an editable
  # node (06 §2), so neither of these accumulates anything.
  return set_key(set_key(state, "address", params["payload"] ?? ""), "error", "") if event == "address"
  return set_key(set_key(state, "secret", params["payload"] ?? ""), "error", "") if event == "secret"

  return mail_arm(state, "Connecting to Gmail…") if event == "link"
  return mail_arm(state, "Fetching your mail…") if event == "refresh"
  return mail_reload(state) if event == "reload"
  return mail_fetch(state) if event == "pull"
  return mail_key(state, params["payload"] ?? ["", 0]) if event == "key"
  return mail_open(state, props["uid"] ?? 0) if event == "open"
  return mail_spot(state, props["uid"] ?? 0) if event == "spot"
  return mail_delete(state) if event == "trash"
  # The cursor follows the box that was pressed, because a mark made with
  # the pointer is still a choice about *this* row: leaving the cursor
  # three rows up would have left `d` pointing somewhere else the moment
  # the marks were cleared.
  return mail_pick(mail_spot(state, props["uid"] ?? 0), props["uid"] ?? 0) if event == "pick"
  return mail_pick_none(state) if event == "pick_none"
  # `payload` is `[x, y]`; only the vertical offset matters here.
  return set_key(state, "scroll", (params["payload"] ?? [0, 0])[1] ?? 0) if event == "scrolled"
  # `[first, last]`, inclusive (04 §7.1): the rows the client is about to
  # need. Answering with those and letting the others go is the whole of
  # what makes a long list cost what a short one costs.
  return set_key(state, "window", params["payload"] ?? [0, MAIL_WINDOW]) if event == "windowed"
  return mail_box_pick(state, props["box"] ?? "") if event == "box_pick"
  return mail_rail_enter(state) if event == "folders_open"
  return mail_screen(state, "sheet_menu") if event == "menu_open"
  return set_key(state, "rail_focus", props["box"] ?? "") if event == "box_focus"
  return mail_find_open(state) if event == "find"
  return mail_query(state, params["payload"] ?? "") if event == "query"
  return mail_find_all(state) if event == "find_all"
  return mail_find_close(state) if event == "find_close"
  return mail_scroll(set_key(state, "reading", false)) if event == "back"
  return ((state["sheet"] ?? false) == true ? mail_screens_off(state) : mail_screen(state, "sheet")) if event == "sheet"
  return set_key(state, "raw", !(state["raw"] ?? false)) if event == "plain"
  return mail_face(state) if event == "face"
  return mail_shoot(state) if event == "shoot"
  return mail_clip_open(state) if event == "clips"
  return mail_view_open(state, props["at"] ?? 0) if event == "show"
  return mail_view_step(state, -1) if event == "view_prev"
  return mail_view_step(state, 1) if event == "view_next"
  return set_key(state, "viewing", nil) if event == "view_close"
  return set_key(state, "copying", !(state["copying"] ?? false)) if event == "select"
  return mail_out(state) if event == "signout"
  return set_key(state, "note", "") if event == "note_done"
  # The accounts, and the sign-in screen when it is used to add one.
  return mail_accounts_open(state) if event == "accounts"
  return mail_switch(state, props["address"] ?? "") if event == "switch"
  return mail_add_open(state) if event == "add_open"
  return mail_add_close(state) if event == "add_cancel"
  # Back to Gmail forgets the server that was typed, so a host left over
  # from a change of mind cannot be dialled for an account that does not
  # live there.
  return mail_kind_gmail(state) if event == "kind_gmail"
  return set_key(state, "kind", "imap") if event == "kind_imap"
  return set_key(state, "host", params["payload"] ?? "") if event == "host"
  return set_key(state, "port", params["payload"] ?? "") if event == "port"
  return set_key(state, "smtp", params["payload"] ?? "") if event == "smtp_host"
  return mail_write(state, "new") if event == "write_new"
  return mail_write(state, "reply") if event == "write_reply"
  return mail_write(state, "forward") if event == "write_forward"
  return mail_write_set(state, "to", params["payload"] ?? "") if event == "w_to"
  return mail_write_set(state, "cc", params["payload"] ?? "") if event == "w_cc"
  return mail_write_set(state, "subject", params["payload"] ?? "") if event == "w_subject"
  # Every gesture the letter's editor makes, in one line: it names its own
  # handlers from the prefix and takes them apart again, so nothing here has
  # to list them.
  return mail_write_doc(state, md_edit_what("w_body", event), params) if md_edit_mine?("w_body", event)
  return mail_write_attach(state, params) if event == "file_upload"
  return mail_arm_send(state) if event == "send"
  return mail_write_close(state) if event == "write_close"

  state
end

# Everything the keyboard does, in one place. The names are the client's:
# a named key by its name, and any other key by the character it produced
# -- so Shift+G arrives as "G" and Shift+/ as "?", and neither needs the
# modifier word that came with it.
def mail_key(state, said)
  key = said[0] ?? ""
  state = set_key(state, "touched", mail_now())
  # A key the root was given means the person is driving the list, not the
  # folder rail -- so the caret's claim on `Enter` ends here.
  #
  # Nothing reports focus *leaving* a node, so without this the frame went
  # on lending `Enter` to the rail after the folder had been chosen, and
  # `Enter` on a message opened nothing at all for the rest of the
  # session. The one key that does not clear it is the one that put it
  # there, and Tab is never claimed, so it never reaches this line.
  state = set_key(state, "rail_focus", "") if (state["rail_focus"] ?? "") != "" && mail_rail_key?(key) != true
  # The sign-in screen claims one key when it is being used to add an
  # account, and this is it.
  if (state["adding"] ?? false) == true && (state["linked"] ?? false) != true
    return mail_add_close(state) if key == "Escape"

    return state
  end
  # While the bar is open, the root holds the keys that move and the keys
  # that leave; everything else -- every letter of the query -- belongs to
  # the field. The arrows are safe to take because the query is one line:
  # up and down do nothing in a single-line field, and left and right,
  # which do, are never claimed.
  #
  # A filtered list is a list. It was one you could look at and not one
  # you could use: no way to walk it, no way to open anything in it, and
  # `Escape` threw the filter away rather than giving it back.
  #
  # `reading != true` is what makes the last of those work. Opening a
  # letter from here leaves `finding` standing on purpose, so this branch
  # would go on answering for the letter's screen -- and `Escape`, which
  # there means "put the letter away", would have thrown the filter away
  # instead and left the letter open. While a letter is up the letter's
  # keys are the ones the frame claims, and these are not asked for.
  if (state["finding"] ?? false) == true && (state["reading"] ?? false) != true
    return mail_find_step(state, 1) if key == "ArrowDown"
    return mail_find_step(state, -1) if key == "ArrowUp"
    return mail_find_step(state, mail_page(state)) if key == "PageDown"
    return mail_find_step(state, 0 - mail_page(state)) if key == "PageUp"
    # `Enter` opens what is under the cursor. With nothing under it --
    # a query that matches none of the messages on hand -- it means what
    # it used to mean and asks the server for the rest of the mailbox.
    if key == "Enter"
      rows = mail_shown(state)
      one = rows[clamp(state["cursor"] ?? 0, 0, rows.length() - 1)]
      return mail_find_all(state) if one.nil?

      # `finding` is left standing, which is the whole point: the reader
      # is checked before the bar in `mail_view`, so the letter is what
      # you see -- and `u` or `Escape` puts it away and gives you back the
      # bar, the query and the filtered list you opened it from.
      return mail_open(state, one["uid"] ?? 0)
    end
    return mail_find_close(state) if key == "Escape"

    return state
  end
  # The viewer, while it is open, owns the keyboard: the arrows walk the
  # pictures rather than scrolling the letter behind them, and `o` is the
  # browser. Anything else puts it away, like the help screen.
  if (state["viewing"] ?? nil).nil? != true
    return mail_view_step(state, -1) if key == "ArrowLeft" || key == "k"
    return mail_view_step(state, 1) if key == "ArrowRight" || key == "j"
    return mail_view_step(state, -1) if key == "PageUp"
    return mail_view_step(state, 1) if key == "PageDown"
    return state if key == "o"

    return set_key(state, "viewing", nil)
  end
  # A draft claims `Escape` and nothing else (see `mail_composer`), so
  # this is the whole of the keyboard while one is open.
  writing = state["writing"]
  if writing.nil? != true
    return mail_write_close(state) if key == "Escape"

    return state
  end
  # The accounts screen is a screen like the keys are: its own numbers,
  # and any other key puts it away.
  if (state["sheet_accounts"] ?? false) == true
    n = MAIL_DIGITS.index_of(key)
    return mail_switch_at(state, n) if n >= 0
    # Everything has a key, and "add one" is the only thing on this
    # screen that was a button and nothing else.
    return mail_add_open(state) if key == "+"

    return set_key(state, "sheet_accounts", false)
  end
  sheet = state["sheet"] ?? false
  reading = state["reading"] ?? false
  msgs = mail_shown(state)
  last = msgs.length() - 1

  # The sheet is a screen, and every key that is not one of its own puts
  # it away, because a help screen you have to find the exit of is a bad
  # help screen.
  if sheet == true
    return state if key == "j" || key == "k"
    return set_key(state, "sheet", false)
  end
  # A selection is a depth, and `Escape` comes out of the nearest one --
  # which on this screen is the marks, before the search that may be
  # behind them and before anything else. The same layering the reader
  # already had for `c`, at the depth above it.
  return mail_pick_none(state) if key == "Escape" && reading != true && (state["picked"] ?? []).length() > 0
  return mail_find_close(state) if key == "Escape" && (state["mode"] ?? "") == "hits" && reading != true
  return mail_screen(state, "sheet") if key == "?"
  # `@` is the chip beside it, and on a narrow window the chip is the menu:
  # the accounts are one press further in, where there is room to show
  # them. Wide, the masthead has room for both and `@` goes straight to
  # the accounts.
  return mail_accounts_open(state) if key == "@"
  # `M` is the menu, and only a narrow window has one: wide, every door it
  # holds is already in the masthead.
  return mail_screen(state, "sheet_menu") if key == "M" && mail_layout(state)["roomy"] != true
  # A number switches account straight from the list, without the screen
  # -- the screen is where you learn which number is which.
  if MAIL_DIGITS.contains(key)
    return mail_switch_at(state, MAIL_DIGITS.index_of(key))
  end
  # `F` for the folders.
  #
  # Not Shift+Tab, which is what a hand reaches for and what this cannot
  # have: `Tab` order is the client's (03 §3) and the rail sits in front
  # of a hundred activatable message rows, so walking back to it means
  # walking through all of them. A key the application owns gets there in
  # one press -- and from there the arrows walk the folders, because while
  # the caret is on the rail the rail is what the arrows are for.
  # The folders screen is a screen, so `Escape` closes it before anything
  # else looks at the key.
  # The folders screen is the rail at another width, so it takes the same
  # keys: the arrows walk it, `Enter` opens what they are on, `Escape`
  # and `F` put it away.
  if (state["sheet_folders"] ?? false) == true
    return mail_screens_off(state) if key == "Escape" || key == "F"
    return mail_rail_step(state, 1) if key == "ArrowDown" || key == "j"
    return mail_rail_step(state, -1) if key == "ArrowUp" || key == "k"
    return mail_box_pick(state, state["rail_focus"] ?? "") if key == "Enter"

    return state
  end
  if (state["sheet_menu"] ?? false) == true
    return mail_screens_off(state) if key == "Escape" || key == "M"
  end
  return mail_rail_enter(state) if key == "F"
  if (state["rail_focus"] ?? "") != ""
    return mail_rail_step(state, 1) if key == "ArrowDown" || key == "j"
    return mail_rail_step(state, -1) if key == "ArrowUp" || key == "k"
    return set_key(state, "rail_focus", "") if key == "Escape"
  end
  return mail_find_open(state) if key == "/"
  # A new message needs no message to start from; the other two do.
  # `Insert` is the same key as `n`, the way `Delete` is the same key as
  # `d`: the hand that reaches for the block on the right of the keyboard
  # gets the two things it expects to find there.
  return mail_write(state, "new") if key == "n" || key == "Insert"
  return state if last < 0

  at = clamp(state["cursor"] ?? 0, 0, last)

  # `Space` marks the row under the cursor, and a mark is what makes `d`
  # mean more than one message.
  #
  # It does not move afterwards, which is the one decision in it worth
  # naming. A key that both marked and advanced would be two gestures
  # wearing one key -- and pressing it twice, which is how anybody
  # unmarks something they did not mean, would have marked the row below
  # instead of undoing the row above. So `Space` marks, `j` moves, and
  # marking three in a row is six presses that each mean one thing.
  return mail_pick(state, (msgs[at] ?? {})["uid"] ?? 0) if key == " " && reading != true

  # j and k mean the same thing on both screens -- move to the next
  # message -- which is the whole of why the reader is worth opening with
  # the keyboard at all.
  return mail_move(state, clamp(at + 1, 0, last), reading) if key == "j" || key == "ArrowDown"
  return mail_move(state, clamp(at - 1, 0, last), reading) if key == "k" || key == "ArrowUp"
  # A page moves the cursor by a screenful rather than the scroller,
  # which on a list is the same thing said better: the selection is what
  # you are moving, and the view follows it. A letter claims neither of
  # these, so there they stay the client's and scroll.
  return mail_move(state, clamp(at + mail_page(state), 0, last), reading) if key == "PageDown"
  return mail_move(state, clamp(at - mail_page(state), 0, last), reading) if key == "PageUp"
  return mail_move(state, 0, reading) if key == "g"
  return mail_move(state, last, reading) if key == "G"

  # The three doors read the same on the list and in a letter, because
  # both have exactly one message in hand: the one under the cursor.
  return mail_write(state, "reply") if key == "a"
  return mail_write(state, "all") if key == "A"
  return mail_write(state, "forward") if key == "f"

  if reading == true
    # Escape backs out of selecting first, and only then out of the
    # letter -- one key, two depths, which is what it means everywhere.
    return set_key(state, "copying", false) if key == "Escape" && (state["copying"] ?? false) == true
    return mail_scroll(set_key(set_key(state, "reading", false), "copying", false)) if key == "u" || key == "Escape"
    # `c` puts the caret in the letter so the whole of it can be taken
    # with Ctrl+A, Ctrl+C without reaching for the pointer. From there the
    # keyboard is the field's -- which is the point -- until Escape.
    return set_key(state, "copying", true) if key == "c"
    # `d` reads the same here as on the list: the letter fades, the bar
    # spins, and when the move lands the cursor has not moved -- which is
    # now the *next* message, so it opens straight into it.
    if key == "d" || key == "Delete"
      mail_say("key d in the reader")
      return mail_delete(state)
    end
    # Pictures are fetched for the message you are reading and no other,
    # and never on open. `t` is the way back to what actually arrived.
    return mail_shoot(state) if key == "i"
    return mail_clip_open(state) if key == "p"
    # `v` for the pictures, from the keyboard: the viewer opens on the
    # first of them, and the arrows walk the rest.
    return mail_view_open(state, 0) if key == "v"
    return set_key(state, "raw", !(state["raw"] ?? false)) if key == "t"
    return mail_face(state) if key == "h"
    # `o`: the caret on to the link, because a key cannot open a browser.
    # `net.open` is a prop and it is spent when *you* activate the node
    # that carries it (03 §3.5) -- so this focuses it and `Enter` opens
    # it. Two presses, and the second one is the person's, which is the
    # whole of what that capability is careful about.
    return set_key(state, "browse", true) if key == "o"
    return state
  end

  return mail_open(state, (msgs[at] ?? {})["uid"] ?? 0) if key == "Enter" || key == "o"
  # `Delete` is the same key as `d`, for the hand that reaches for the
  # block on the right of the keyboard rather than for the letter.
  #
  # Safe to claim only because of *where* it is claimed. An ancestor's
  # claim beats the field under the caret (03 §3.1) -- a root that held
  # `Delete` everywhere would take it out of every draft, where it is the
  # character in front of the caret. It is in `MAIL_KEYS` and
  # `MAIL_READ_KEYS` and in neither of the two lists a screen with a field
  # uses: a draft claims `Escape` alone and the search bar `Escape` and
  # `Enter`.
  if key == "d" || key == "Delete"
    mail_say("key d in the list")
    return mail_delete(state)
  end
  return mail_arm(state, "Fetching your mail…") if key == "r"
  return mail_reload(state) if key == "R"
  return mail_out(state) if key == "q"

  state
end

# How many rows a screenful is. The masthead is a fixed height and so is
# every row, which is what makes this arithmetic rather than a guess --
# the same two constants the scroll offset is computed from.
def mail_page(state)
  lay = mail_layout(state)
  n = (lay["h"] - MAIL_HEAD_H) / MAIL_ROW_H
  n = 1 if n < 1

  n
end

# Move the cursor, and -- when the list is what is on screen -- carry the
# scroller with it. On the reader there is nothing to scroll to: moving
# the cursor there changes which letter is open.
# Whatever is open now needs its letter, and counts as read.
#
# `mail_open` asks for both, but it is not the only way a different
# message ends up on screen: `j` and `k` inside the reader move between
# them, and deleting one brings the next up under the cursor. Neither
# goes through `mail_open`, so neither asked -- and the reader sat on
# "Fetching the letter…" for a message nothing had fetched, for ever.
#
# This is that request, made from wherever the open message changes.
def mail_showing(state)
  return state if (state["reading"] ?? false) != true

  msgs = mail_shown(state)
  at = clamp(state["cursor"] ?? 0, 0, msgs.length() - 1)
  one = msgs[at] ?? {}
  uid = one["uid"] ?? 0
  return state if uid < 1

  out = state
  out = set_key(out, "want_body", uid) if one["loaded"] != true
  return out if one["seen"] == true

  mark = fn(m) { (m["uid"] ?? 0) == uid ? set_key(m, "seen", true) : m }
  marked = (out["msgs"] ?? []).map(mark)
  kept = (out["all"] ?? []).map(mark)
  mail_store_save(state["address"] ?? "", kept, state["total"] ?? kept.length(), state["uidvalidity"] ?? 0, mail_here(state))
  out = set_key(out, "msgs", marked)
  out = set_key(out, "all", kept)
  set_key(out, "want_seen", uid)
end

def mail_move(state, to, reading)
  moved = set_key(set_key(state, "cursor", to), "note", "")
  # Reaching the last row is the request for more. No button, no
  # "load more" — the end of the list *is* the gesture, and five is
  # small enough that it arrives before you have finished reading the
  # one you are on.
  moved = mail_more(moved) if to >= mail_shown(moved).length() - 1
  return mail_showing(moved) if reading == true

  mail_scroll(moved)
end

# Ask for the next few older messages, if there are any and nothing else
# is in flight.
#
# One batch per arrival at the end, not a loop: `busy` is cleared when the
# batch lands, so walking off the bottom again asks for the next five.
# A runaway here would quietly download ten thousand messages.
def mail_more(state)
  return state if (state["busy"] ?? "") != ""
  return state if (state["mode"] ?? "") == "hits"
  return state if (state["query"] ?? "") != ""

  have = (state["msgs"] ?? []).length()
  total = state["total"] ?? 0
  return state if have < 1 || have >= total

  out = set_key(state, "mode", "older")
  # Never rewind the walk: `have` counts rows, and after a batch that
  # overlapped what was already held there are fewer rows than mailbox
  # walked. Taking the smaller of the two would ask for the overlapping
  # stretch again on every trip to the bottom.
  done = state["fetched"] ?? 0
  out = set_key(out, "fetched", done > have ? done : have)
  set_key(out, "busy", "Fetching")
end

# Keep the cursor row wholly in view.
#
# `state["scroll"]` is heard, not assumed: the list subscribes to `scroll`
# (06 §8), so a wheel, a dragged thumb and the arrow keys this application
# deliberately leaves to the client (03 §3) all report where they left the
# view. Believing the last offset we asked for instead was wrong exactly
# when it mattered -- after someone wheeled down, `j` measured the cursor
# against a window it was no longer in and pushed the selection off screen.
def mail_scroll(state)
  lay = mail_layout(state)
  msgs = state["msgs"] ?? []
  at = clamp(state["cursor"] ?? 0, 0, msgs.length())
  top = MAIL_LIST_TOP + at * MAIL_ROW_H
  bottom = top + MAIL_ROW_H
  view = lay["h"] - MAIL_HEAD_H
  y = state["scroll"] ?? 0
  y = top if top < y
  floor = bottom - view
  y = floor if floor > y
  y = 0 if y < 0
  set_key(state, "scroll", y)
end

# `d`. There is no confirmation and no undo inside this application,
# which is a deliberate trade and the reason it trashes rather than
# deletes: Gmail keeps a trashed message for thirty days and the web
# interface will put it back. A key that emptied something irreversibly
# would need a dialog; this one does not.
# ------------------------------------------------------------ searching
#
# Two searches in one bar. Typing filters what is already here, which
# costs nothing and answers immediately; `Enter` asks the server, which
# costs a round trip and answers about the whole mailbox. The first is
# what you want while you are still deciding what you are looking for.
# The marks go when the bar opens and when it closes, and that is not
# tidiness: `d` acts on what is marked, a filter hides rows, and a filter
# that hid four of five marked messages would leave one key meaning
# "delete some things you cannot see".
def mail_find_open(state)
  out = set_key(state, "finding", true)
  out = mail_pick_none(out)
  set_key(out, "query", state["query"] ?? "")
end

# The cursor, moved inside whatever the filter is showing.
def mail_find_step(state, by)
  rows = mail_shown(state)
  return state if rows.length() == 0

  mail_move(state, clamp((state["cursor"] ?? 0) + by, 0, rows.length() - 1), false)
end

def mail_find_close(state)
  out = mail_pick_none(state)
  out = set_key(out, "finding", false)
  out = set_key(out, "query", "")
  out = set_key(out, "hits", [])
  out = set_key(out, "mode", "new")
  out = set_key(out, "cursor", 0)
  out = set_key(out, "scroll", 0)
  kept = state["all"] ?? (state["msgs"] ?? [])
  # `fetched` was counting its way down the search results; the inbox is
  # back now, so the walk cursor belongs to the inbox again. Left alone it
  # outranks the row count, and the walk -- which only ever moves forward
  # (`mail_more`) -- would resume below the messages on screen and leave a
  # hole in the middle of the list.
  out = set_key(out, "fetched", kept.length())
  mail_scroll(set_key(out, "msgs", kept))
end

# Typing narrows the list under the cursor, so the cursor has to come
# back with it -- otherwise it is left pointing past the end of what is
# now on screen, and the highlight lands on nothing or on the wrong row.
def mail_query(state, said)
  out = set_key(state, "query", said)
  out = set_key(out, "cursor", 0)
  set_key(out, "scroll", 0)
end

# `Enter`: ask the server. The matches are fetched by the ordinary batch
# loader, so the first few appear about as fast as the inbox does.
def mail_find_all(state)
  query = (state["query"] ?? "").trim()
  return state if query == ""

  got = mail_search_uids(
    state["address"] ?? "", state["secret"] ?? "", query, MAIL_LIMIT, mail_here(state)
  )
  return mail_blame(state, got["error"]) unless got["error"].nil?

  uids = got["uids"] ?? []
  out = set_key(state, "hits", uids)
  out = set_key(out, "found", got["total"] ?? uids.length())
  out = set_key(out, "msgs", [])
  out = set_key(out, "fetched", 0)
  out = set_key(out, "cursor", 0)
  out = set_key(out, "scroll", 0)
  out = set_key(out, "finding", false)
  out = set_key(out, "mode", "hits")
  return set_key(out, "busy", "") if uids.length() == 0

  set_key(out, "busy", "Fetching")
end

# The list the screens actually work on. Every place that counts rows,
# moves a cursor, opens a message or deletes one goes through here, so
# the cursor always indexes what is on the screen and never the inbox
# behind a filter.
# Deduplicated here as well as at every merge, because this is the list
# that becomes keyed rows: whatever else goes wrong upstream, the tree
# handed to the client is the one thing that must never carry one UID
# twice -- the server refuses the whole frame, and a refused frame is a
# window that stops updating.
def mail_shown(state)
  return mail_unique(state["msgs"] ?? []) if (state["mode"] ?? "") == "hits"

  mail_unique(mail_filter(state["msgs"] ?? [], state["query"] ?? ""))
end

# What the list shows: everything, or only what matches what is typed.
def mail_filter(msgs, query)
  said = query.trim().downcase()
  return msgs if said == ""

  msgs.filter(fn(one) { mail_hit?(one, said) })
end

def mail_hit?(one, said)
  hay = (one["name"] ?? "") + " " + (one["address"] ?? "") + " " + (one["subject"] ?? "")
  return true if hay.downcase().contains(said)

  (one["body"] ?? "").downcase().contains(said)
end

# A row took focus. Move the cursor there and nothing else -- no scroll,
# because the client already brought the focused node into view, and a
# `scroll_to` answering that would fight it.
def mail_spot(state, uid)
  at = mail_index(mail_shown(state), uid)
  return state if at < 0

  set_key(state, "cursor", at)
end

def mail_index(msgs, uid)
  found = -1
  i = 0
  while i < msgs.length()
    one = msgs[i] ?? {}
    found = i if one["uid"] == uid
    i = i + 1
  end
  found
end

# The letter as a page, written beside the attachments, and the address
# it answers at.
#
# There is no op that opens an address and no event that reports one:
# `net.open` is a **prop**, spent when the person activates the node that
# carries it (03 §3.5). So a key cannot open a browser, and this does not
# pretend to -- it writes the file and hands back a URL for a node to
# carry. `o` puts the caret on that node; `Enter` is the activation.
#
# The sender's HTML is written out as it stands, which is the whole point
# of opening it in a browser -- and which is also why the page carries a
# content policy that forbids scripts outright. A mail is bytes a
# stranger chose; this application draws them as boxes and text and a
# browser would *run* them, on this origin, beside every other message
# written here. `default-src 'none'` is the difference.
def mail_page_file(one)
  uid = one["uid"] ?? 0
  return "" if uid < 1

  # Once per message, not once per frame. This is called from the view --
  # the one place that knows the letter is on screen -- and the view runs
  # on every tick, so writing unconditionally is a few kilobytes of disk
  # every quarter of a second for a page whose content cannot change.
  path = MAIL_PAGE_DIR + "/" + str(uid) + ".html"
  return path if (File.exists(path) rescue false) == true

  html = one["html"] ?? ""
  said = html == "" ? "<pre>" + mail_escape(one["body"] ?? "") + "</pre>" : html
  head = "<!doctype html>\n<html><head><meta charset=\"utf-8\">" +
    "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src https: http: data: cid:; style-src 'unsafe-inline'\">" +
    "<meta name=\"referrer\" content=\"no-referrer\">" +
    "<title>" + mail_escape(one["subject"] ?? "") + "</title>" +
    "<style>body{font:16px/1.5 system-ui,sans-serif;margin:2rem auto;max-width:44rem;padding:0 1rem}" +
    "header{border-bottom:1px solid #ddd;padding-bottom:1rem;margin-bottom:1rem;color:#555}" +
    "h1{font-size:1.4rem;margin:0 0 .3rem}img{max-width:100%;height:auto}pre{white-space:pre-wrap}</style></head><body>" +
    "<header><h1>" + mail_escape(one["subject"] ?? "") + "</h1>" +
    mail_escape((one["name"] ?? "") + " · " + (one["address"] ?? "")) + " — " + mail_escape(one["date"] ?? "") +
    "</header>"
  wrote = File.write(path, head + said + "</body></html>") rescue nil
  return "" if wrote.nil?

  path
end

def mail_escape(said)
  said.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")
end

def mail_open(state, uid)
  found = mail_index(mail_shown(state), uid)
  at = found < 0 ? (state["cursor"] ?? 0) : found
  opened = set_key(set_key(state, "cursor", at), "open", uid)
  fresh = set_key(set_key(opened, "copying", false), "images", false)
  fresh = set_key(fresh, "face", "")
  out = set_key(set_key(fresh, "raw", false), "reading", true)
  one = mail_shown(state)[at] ?? {}

  # Opening a message shows the message. The letter and the read mark are
  # asked for on the next tick, not here.
  #
  # Doing them here is a few hundred milliseconds under the session's
  # frame lock, which is a few hundred milliseconds in which the window
  # answers nothing -- and it is spent *after* the key, so what you see is
  # a press that does nothing and then a page. Asking on the tick lets the
  # reader draw first, with a spinner where the letter will be.
  #
  # Deferring is safe for exactly these two and not for `d`. If the
  # session ends before the tick, an unfetched letter costs nothing --
  # reopening asks again -- and an unmarked message is still unread, which
  # it was. A delete that never happened is a message the person believes
  # they threw away, so that one stays inline whatever it costs.
  mail_say("open uid=" + str(uid) + " loaded=" + str(one["loaded"] ?? false) + " -> asking for the letter=" + str(one["loaded"] != true))
  out = set_key(out, "want_body", uid) if one["loaded"] != true

  # The read mark is optimistic on screen: waiting for Gmail to confirm
  # before un-bolding a row you are already looking at would be a round
  # trip spent on nothing.
  return out if one["seen"] == true

  mark = fn(m) { (m["uid"] ?? 0) == uid ? set_key(m, "seen", true) : m }
  marked = (out["msgs"] ?? []).map(mark)
  kept = (out["all"] ?? []).map(mark)
  mail_store_save(state["address"] ?? "", kept, state["total"] ?? kept.length(), state["uidvalidity"] ?? 0, mail_here(state))
  out = set_key(out, "msgs", marked)
  out = set_key(out, "all", kept)
  set_key(out, "want_seen", uid)
end

# The tick's errands: the letter someone is looking at, then the flag.
# Both are idempotent, and both clear their own marker, which is what
# stops the clock.
def mail_errand(state)
  uid = state["want_body"] ?? 0
  mail_say("tick: errand body=" + str(uid) + " seen=" + str(state["want_seen"] ?? 0))
  return mail_load_body(state, uid) if uid > 0

  seen = state["want_seen"] ?? 0
  out = set_key(state, "want_seen", 0)
  return out if seen < 1 || mail_demo?()

  mail_mark_seen(state["address"] ?? "", state["secret"] ?? "", seen, mail_here(state))
  out
end

def mail_load_body(state, uid)
  out = set_key(state, "want_body", 0)
  return out if mail_demo?()

  full = mail_pull_body(state["address"] ?? "", state["secret"] ?? "", uid, mail_here(state))
  mail_say("letter uid=" + str(uid) + " -> " + (full.nil? ? "FAILED" : str(full["body"].length()) + " bytes"))
  return set_key(out, "error", "That message would not come down.") if full.nil?

  # Everything the whole message knows that the four header fields did
  # not, not just its text.
  #
  # This copied `body`, `html` and `loaded` and dropped the rest on the
  # floor -- so a message with a PDF on it was parsed, its attachments
  # listed, and then the list thrown away the moment the letter was
  # stored. The reader showed no paper clips for exactly the messages
  # whose paper clips it had just read.
  swap = fn(m) {
    return m if (m["uid"] ?? 0) != uid

    one = set_key(set_key(m, "body", full["body"]), "html", full["html"])
    one = set_key(one, "md", full["md"] ?? "")
    one = set_key(one, "atts", full["atts"] ?? [])
    one = set_key(one, "clips", (full["atts"] ?? []).length())
    set_key(one, "loaded", true)
  }
  msgs = (out["msgs"] ?? []).map(swap)
  kept = (out["all"] ?? []).map(swap)
  mail_store_save(state["address"] ?? "", kept, state["total"] ?? kept.length(), state["uidvalidity"] ?? 0, mail_here(state))
  out = set_key(out, "msgs", msgs)
  set_key(out, "all", kept)
end

# `d`: the row catches, and the mailbox is told a quarter of a second
# later.
#
# This used to move the message inside the press. With a kept connection
# that is about 110 ms, which is under a frame -- but the connection is
# not always kept, and a `UID MOVE` down a cold one is a second and a
# half of a window that does not answer. The press is the one moment the
# application must not stop, because it is the moment someone is looking
# straight at it.
#
# So the press marks the row and returns, the next tick moves the
# message, and the row leaves when the mailbox agrees that it has. Which
# is also why the row is not taken out of `all` here: if this session
# ends before the tick runs, nothing has been lost and nothing has been
# lied about -- the message is still in the inbox and comes back on the
# next refresh. The old queued delete dropped the row from the store
# first, and a session that ended in between left a message that the
# application had forgotten and the mailbox still had.
# -- marking ----------------------------------------------------------------
#
# A mark is a UID in a list, like a delete on its way out is, and for the
# same reason: the row it belongs to is found again by UID whatever the
# list does underneath it -- a fetch that puts four newer messages on top
# moves every row down and marks none of them.
#
# Nothing else in the application has to know about it. `d` asks whether
# there are marks and, if there are, arms all of them instead of the one
# under the cursor; every other action goes on taking the message the
# cursor is on. That is what keeps this a hundred lines rather than a
# second mode with a second copy of each door in it.
def mail_pick(state, uid)
  return state if uid < 1

  picked = state["picked"] ?? []
  kept = picked.filter(fn(u) { u != uid })
  # Filtered first and added only if the filter took nothing: one pass
  # says both whether it was marked and what the list is without it.
  kept = kept.concat([uid]) if kept.length() == picked.length()
  set_key(set_key(state, "picked", kept), "note", "")
end

def mail_pick_none(state)
  set_key(state, "picked", [])
end

# `d` with rows marked is `d` on every one of them, in one press.
#
# There is no second machinery for it: `burning` has been a list since
# `d d d` needed to put three UIDs in it, and the tick has always moved
# one message per tick and taken the row out when the mailbox agreed. So
# a group delete is the same animation, the same undo window and the same
# recovery on the same rows -- five of them at once instead of one.
def mail_delete_picked(state)
  msgs = mail_shown(state)
  here = {}
  for one in msgs
    here[str(one["uid"] ?? 0)] = true
  end
  # Only what is still on the screen, and only what is not already on its
  # way out. A mark that outlived its message -- another client deleted
  # it, a refresh dropped it -- would otherwise ask the server to move a
  # UID that is not there, and the answer to that is a red line in the
  # bar about a message nobody can see.
  add = (state["picked"] ?? []).filter(fn(u) { here[str(u)] == true && mail_burning?(state, u) != true })
  out = mail_pick_none(state)
  return out if add.length() == 0

  mail_say("delete " + str(add.length()) + " marked as=" + str(state["address"] ?? "(none)"))
  out = set_key(out, "burning", (state["burning"] ?? []).concat(add))
  out = set_key(out, "error", "")
  out = set_key(out, "copying", false)
  out = set_key(out, "reading", false)
  # Where the eye is, moved off whatever is now leaving -- the same walk a
  # single delete does, from wherever the cursor happened to be.
  mail_delete_next(out, clamp(state["cursor"] ?? 0, 0, msgs.length() - 1))
end

def mail_delete(state)
  # On the list the marks outrank the cursor, and nothing else about `d`
  # changes: same key, same row treatment, same quarter-second before the
  # mailbox is told.
  #
  # Inside a letter they do not, and that is the same rule the search bar
  # is held to: a letter shows no gutter and no count, so the marks are
  # not on screen -- and a key that deleted things you cannot see is the
  # one thing a delete must never be. In a letter there is exactly one
  # message in hand, and `d` takes it.
  return mail_delete_picked(state) if (state["picked"] ?? []).length() > 0 && (state["reading"] ?? false) != true

  msgs = mail_shown(state)
  at = clamp(state["cursor"] ?? 0, 0, msgs.length() - 1)
  one = msgs[at]
  return state if one.nil?

  uid = one["uid"] ?? 0
  return state if uid < 1

  going = state["burning"] ?? []
  # A second `d` on a row already on its way is not a second delete.
  return mail_delete_next(state, at) if mail_burning?(state, uid)

  mail_say("delete uid=" + str(uid) + " armed as=" + str(state["address"] ?? "(none)"))
  out = set_key(state, "burning", going.concat([uid]))
  out = set_key(out, "error", "")
  out = set_key(out, "copying", false)
  # Reading it and burning it at once would be a letter on screen that is
  # not there any more; the list is where this is watched.
  out = set_key(out, "reading", false)
  mail_delete_next(out, at)
end

# The cursor lands on the next row that is not already leaving, so `d d d`
# burns three of them rather than arguing with the first.
def mail_delete_next(state, at)
  msgs = mail_shown(state)
  to = at
  while to < msgs.length() && mail_burning?(state, (msgs[to] ?? {})["uid"] ?? 0)
    to = to + 1
  end
  mail_scroll(set_key(state, "cursor", clamp(to, 0, msgs.length() - 1)))
end

def mail_burning?(state, uid)
  found = false
  for u in (state["burning"] ?? [])
    found = true if u == uid
  end
  found
end

# One message per tick, and the row goes when the mailbox says it has.
def mail_burn_step(state)
  going = state["burning"] ?? []
  return state if going.length() == 0

  uid = going[0]
  left = going.slice(1, going.length())
  # The demo has no account to move anything in. It drops the row and
  # stops there, which is the whole of what it can honestly do.
  unless mail_demo?()
    said = mail_trash_uid(state["address"] ?? "", state["secret"] ?? "", uid, mail_here(state))
    mail_say("delete uid=" + str(uid) + " -> " + (said == "" ? "moved" : "FAILED: " + said))
    if said != ""
      out = set_key(state, "burning", left)
      return set_key(out, "error", said)
    end
  end

  # Where it was, so the cursor can be put back where the eye is. The
  # press moved it past the row it had just lit; every row below that one
  # comes up by one when it finally goes, and a cursor left where it was
  # would be pointing at the message *after* the one it looked like.
  was = mail_index(mail_shown(state), uid)
  rest = (state["msgs"] ?? []).filter(fn(m) { (m["uid"] ?? 0) != uid })
  kept = (state["all"] ?? []).filter(fn(m) { (m["uid"] ?? 0) != uid })
  # Out of the cache as well as out of the list: `mail_store_save` writes
  # what it is given and a row nobody mentions any more would otherwise
  # sit there until the account was signed out.
  mail_store_drop(state["address"] ?? "", uid, mail_here(state))
  total = state["total"] ?? rest.length()
  total = total - 1 if total > 0
  mail_store_save(state["address"] ?? "", kept, total, state["uidvalidity"] ?? 0, mail_here(state))

  out = set_key(state, "msgs", rest)
  out = set_key(out, "all", kept)
  out = set_key(out, "total", total)
  out = set_key(out, "fetched", rest.length())
  out = set_key(out, "burning", left)
  out = set_key(out, "error", "")
  # An animation that lasts 320 ms is not an answer to "did that work?".
  # The note is: it names what happened and where the message went, and
  # stays until the next thing you do.
  out = set_key(out, "note", "Moved to " + mail_trash_name(state["address"] ?? "", state["secret"] ?? ""))
  cur = state["cursor"] ?? 0
  cur = cur - 1 if was >= 0 && cur > was
  return set_key(out, "reading", false) if mail_shown(out).length() == 0

  mail_scroll(set_key(out, "cursor", clamp(cur, 0, mail_shown(out).length() - 1)))
end

# -- writing -----------------------------------------------------------------
#
# Three doors, one screen. `n` starts a new message, `a` answers the one
# under the cursor, `A` answers everyone on it and `f` forwards it. What
# differs between them is only what the draft opens with, so there is one
# composer and one send.
#
# Sending is SMTP with the same address and app password IMAP signed in
# with -- Gmail takes an app password on both -- and the letter goes out
# as plain text. A mail client that could only read would be half a mail
# client, and this is the other half at its smallest: no attachments, no
# signatures, no drafts folder.
# Gmail by default, and named in the environment for anyone whose mail
# is not Gmail -- or for a test that wants a server it can read.
MAIL_SMTP = getenv("MAIL_SMTP_HOST") ?? "smtp.gmail.com"
MAIL_SMTP_PORT = (getenv("MAIL_SMTP_PORT") ?? "587").to_i()
MAIL_SMTP_TLS = getenv("MAIL_SMTP_TLS") ?? "starttls"

# How much of an answered letter is quoted back, in **bytes**.
#
# Not a matter of taste: one text node holds `MAX_INLINE_STR` = 4096
# bytes (02 §4), and a `textarea`'s value is one text node. A letter is
# split across nodes when it is read, and a draft cannot be -- there is
# one field and it holds one string -- so the quote is cut to fit, with
# the budget left for the attribution line and what you are about to
# write above it.
MAIL_QUOTE_CAP = 3400

# The whole source of one message, headers and all.
#
# A list row keeps four header fields, which is all a row draws and all
# the store is asked to hold. An answer needs more than that -- the
# address to reply to, everyone else who was on it, and the two headers
# that make an answer thread -- so it fetches the message again rather
# than widening what every row carries.
def mail_source(address, app_password, uid)
  held = mail_box(address, app_password)
  return nil if held.nil?

  held["box"].fetch_uid(uid) rescue nil
end

# One header out of a raw message, unfolded onto a single line.
def mail_header_of(raw, name)
  want = name.downcase() + ":"
  n = want.length()
  found = ""
  taking = false
  for line in raw.lines()
    said = line.replace("\r", "")
    # The header block ends at the first empty line, and so does this.
    return found if said == ""

    if taking == true
      return found unless said.starts_with(" ") || said.starts_with("\t")

      found = found + " " + said.trim()
    end
    if taking != true && said.downcase().starts_with(want)
      found = said.substring(n, said.length()).trim()
      taking = true
    end
  end
  found
end

# `Name <address>`, or the bare address when there is no name.
def mail_one_address(who)
  address = who["address"] ?? ""
  name = who["name"] ?? ""
  return address if name == ""

  name + " <" + address + ">"
end

# Who an answer goes to: `Reply-To` when the sender named one -- a
# mailing list, a no-reply that is not -- and the sender otherwise.
def mail_answer_to(msg)
  said = mail_header_of(msg["raw"] ?? "", "reply-to")
  return said if said != ""

  mail_one_address(msg["from"] ?? {})
end

# Everyone else who was on it, for `A`. Yourself removed: answering all
# should not put a copy of your answer in your own inbox.
def mail_answer_all(msg, me)
  crowd = (msg["to"] ?? []).map(fn(w) { mail_one_address(w) })
  cc = mail_header_of(msg["raw"] ?? "", "cc")
  crowd = crowd.concat(cc.split(",").map(fn(s) { s.trim() })) if cc != ""
  kept = crowd.filter(fn(a) { a != "" && mail_is_me?(a, me) != true })
  kept.join(", ")
end

def mail_is_me?(address, me)
  return false if me == ""

  address.downcase().contains(me.downcase())
end

def mail_re(subject)
  said = subject == "(no subject)" ? "" : subject
  return said if said.downcase().starts_with("re:")

  "Re: " + said
end

def mail_fw(subject)
  said = subject == "(no subject)" ? "" : subject
  return said if said.downcase().starts_with("fwd:")

  "Fwd: " + said
end

# As many whole lines as fit in `budget` bytes, and a plain word about
# the ones that did not.
#
# Whole lines, and counted in bytes, because both halves matter: cutting
# mid-character corrupts it, and cutting mid-line of a quote reads as
# though the sender stopped there. `length` is bytes here, which is what
# the limit is measured in.
def mail_room(body, budget)
  kept = []
  spent = 0
  short = false
  for line in body.replace("\r", "").lines()
    if short != true
      cost = line.length() + 1
      if spent + cost > budget
        short = true
      end
      if short != true
        kept.push(line)
        spent = spent + cost
      end
    end
  end
  said = kept.join("\n")
  return said if short != true

  said + "\n[…the rest of this message is not quoted]"
end

# The attribution line and the letter under it, every line marked. The
# convention is old and worth keeping: it survives being quoted again.
def mail_quote(one, body)
  lead = "On " + (one["date"] ?? "") + ", " + (one["name"] ?? "") + " wrote:"
  # Marked first, measured after: the two bytes a mark adds to every line
  # are bytes the budget has to know about.
  marked = body.replace("\r", "").lines().map(fn(l) { "> " + l }).join("\n")
  "\n\n" + lead + "\n" + mail_room(marked, MAIL_QUOTE_CAP)
end

def mail_forwarded(one, body)
  head = [
    "---------- Forwarded message ----------",
    "From: " + mail_one_address(one),
    "Date: " + (one["date"] ?? ""),
    "Subject: " + (one["subject"] ?? "")
  ]
  "\n\n" + head.join("\n") + "\n\n" + mail_room(body, MAIL_QUOTE_CAP)
end

# A draft holds each header field twice.
#
# `seed` is what the composer *draws*; the three beside it are what you
# have typed and what will be sent. They are separate because a view that
# writes the typed value back into the node it came from sends a
# `SetText` for every settled change -- the client reseeds the field from
# it (07 §6) and the caret lands at the end of your own sentence. Drawing
# the seed, which never changes while the composer is open, means no
# `SetText` is ever emitted for a field someone is typing in.
#
# The letter is not one of them. It is a document -- `markdown_editor`'s
# list of blocks -- and it needs no seed, because the server stores what
# was typed verbatim and a `SetText` is only emitted when the server's own
# value changed (07 §6). The two times it does change are the two times it
# should: a marker at the head of a block becoming a heading, and two
# blocks joining.
def mail_draft(kind, to, cc, subject, body, ref, refs)
  blocks = md_edit_parse(body)
  # An answer and a forward both open above what they carry, never inside
  # it. `md_edit_parse` drops blank lines -- a document is blocks, and a
  # blank line between two of them is punctuation rather than a block -- so
  # the empty paragraph to write in is put there rather than written into
  # the source and hoped for.
  quoting = kind == "reply" || kind == "all" || kind == "forward"
  blocks = md_edit_splice(blocks, 0, 0, [{"id": md_edit_fresh(blocks), "kind": "p", "t": ""}]) if quoting
  # And the caret goes there, for the two that know who they are going to.
  # A new message and a forward open at "who to", because that is the one
  # thing neither of them knows.
  answering = kind == "reply" || kind == "all"
  {
    "kind": kind, "to": to, "cc": cc, "subject": subject,
    "doc": {
      "blocks": blocks,
      "focus": answering ? blocks[0]["id"] : 0,
      "take": answering
    },
    "ref": ref, "refs": refs,
    "seed": {"to": to, "cc": cc, "subject": subject}
  }
end

# The letter as markdown, which is what goes out and what is quoted back.
def mail_draft_body(draft)
  md_edit_source((draft["doc"] ?? {})["blocks"] ?? [])
end

def mail_write(state, kind)
  return state if (state["linked"] ?? false) != true

  return mail_writing(state, mail_draft("new", "", "", "", "", "", "")) if kind == "new"

  msgs = mail_shown(state)
  at = clamp(state["cursor"] ?? 0, 0, msgs.length() - 1)
  one = msgs[at]
  return state if one.nil?

  uid = one["uid"] ?? 0
  full = nil
  full = mail_source(state["address"] ?? "", state["secret"] ?? "", uid) if mail_demo?() != true && uid > 0
  here = full.nil? != true

  body = one["body"] ?? ""
  body = mail_body(full) if here == true
  raw = ""
  raw = full["raw"] ?? "" if here == true
  ref = raw == "" ? "" : mail_header_of(raw, "message-id")
  refs = raw == "" ? "" : mail_header_of(raw, "references")
  refs = ref if refs == ""
  refs = refs + " " + ref if refs != ref && ref != ""

  return mail_writing(state, mail_draft(
    "forward", "", "", mail_fw(one["subject"] ?? ""), mail_forwarded(one, body), "", ""
  )) if kind == "forward"

  to = mail_one_address(one)
  to = mail_answer_to(full) if here == true
  cc = ""
  cc = mail_answer_all(full, state["address"] ?? "") if here == true && kind == "all"
  mail_writing(state, mail_draft(
    kind, to, cc, mail_re(one["subject"] ?? ""), mail_quote(one, body), ref, refs
  ))
end

# The composer is a screen, not a panel: it replaces what was there and
# puts everything else away, so nothing underneath can take a key it was
# not offered.
def mail_writing(state, draft)
  out = set_key(state, "writing", draft)
  out = set_key(out, "reading", false)
  out = set_key(out, "finding", false)
  out = set_key(out, "sheet", false)
  out = set_key(out, "copying", false)
  out = set_key(out, "note", "")
  set_key(out, "error", "")
end

def mail_write_set(state, field, said)
  draft = state["writing"]
  return state if draft.nil?

  set_key(state, "writing", set_key(draft, field, said))
end

# Every gesture the letter's editor makes, through the one function the
# catalogue exposes for it.
def mail_write_doc(state, what, params)
  draft = state["writing"]
  return state if draft.nil?

  doc = draft["doc"] ?? {"blocks": md_edit_parse("")}
  set_key(state, "writing", set_key(draft, "doc", markdown_editor_step(doc, what, params)))
end

# A file the person attached, kept where a received attachment is kept.
#
# `file_upload` is the server's own event and names no node (03 §3.2), so
# it is only this composer's if the transfer id matches the `file_pick`
# the editor wrote down -- which is what `md_edit_wants?` answers. This
# application has no other picker today; it is written this way because
# the day it grows one, the bug would be a photograph landing in the wrong
# place with nothing on screen to say so.
#
# The bytes go under `public/mail-att`, not into the asset store: an
# `eui-asset:` address is resolvable by a client talking to this server and
# by nothing else, and a letter leaves. `mail_send_source` turns these
# paths into real MIME parts on the way out.
def mail_write_attach(state, params)
  draft = state["writing"]
  return state if draft.nil?

  doc = draft["doc"] ?? {"blocks": md_edit_parse("")}
  return state unless md_edit_wants?(doc, params)

  said = params["payload"] ?? {}
  name = (said["name"] ?? "piece").to_s
  why = (said["error"] ?? "").to_s
  return mail_write_doc_set(state, doc.merge({"trouble": name + " did not arrive: " + why})) unless why == ""

  spool = (said["path"] ?? "").to_s
  kept = mail_write_keep(spool, name, said["size"] ?? 0)
  return mail_write_doc_set(state, doc.merge({"trouble": name + ": it could not be kept."})) if kept == ""

  shot = Image.new(kept) rescue null
  one = {"id": md_edit_fresh(doc["blocks"] ?? []), "kind": "file", "t": name, "src": kept, "note": md_edit_weight(said["size"] ?? 0)}
  unless shot.nil?
    one = {
      "id": one["id"], "kind": "image", "t": name, "src": kept,
      "w": shot.width() rescue 0,
      "h": shot.height() rescue 0
    }
  end
  mail_write_doc_set(state, doc.merge({
    "blocks": md_edit_put(doc["blocks"] ?? [], doc["focus"] ?? 0, one),
    "focus": one["id"],
    "take": true,
    "over": false,
    "trouble": "",
    "awaiting": null
  }))
end

def mail_write_doc_set(state, doc)
  draft = state["writing"]
  return state if draft.nil?

  set_key(state, "writing", set_key(draft, "doc", doc))
end

# Move the spool file beside the received attachments, under a name that
# cannot be a path. Answers where it landed, or "".
#
# `rename` and not read-then-write: the spool is under this application's
# root and so is `public/mail-att`, so the two are one filesystem and a
# rename is a directory entry. Reading it first would mean an array of one
# boxed integer per byte -- a twenty-megabyte attachment several times
# over, for the length of one event, under the frame lock.
#
# It also takes the file *out* of the spool, which is right: the spool
# dies with the socket (01 §6) and a draft should survive a reconnection.
#
# The uid is 0 because nothing sent has one yet; the counter is the epoch
# second, so two pictures of the same name attached to one draft are two
# files.
def mail_write_keep(spool, name, size)
  return "" if spool == ""
  return "" if size > MAIL_ATT_MAX

  mkdir_p(MAIL_ATT_DIR) rescue nil
  leaf = MAIL_ATT_DIR + "/" + mail_att_name(0, DateTime.now().to_unix(), name)
  moved = rename(spool, leaf) rescue null
  moved.nil? ? "" : leaf
end

def mail_write_close(state)
  out = set_key(state, "writing", nil)
  out = set_key(out, "sending", "")
  mail_scroll(set_key(out, "error", ""))
end

# Send is two steps, for the reason opening a letter is: the work is a
# second of SMTP under the session's frame lock, and a second in which
# the window answers nothing is a second in which the press looked
# ignored. The press arms it and draws the spinner; the tick sends.
def mail_arm_send(state)
  draft = state["writing"]
  return state if draft.nil?

  return set_key(state, "error", "Say who it goes to.") if (draft["to"] ?? "").trim() == ""

  out = set_key(state, "error", "")
  set_key(out, "sending", "Sending…")
end

# What a letter's own pictures and files cost it on the way out.
#
# A document written here names them by where they were kept --
# `public/mail-att/...`, this application's folder and nobody else's. A
# mail leaves, so each one has to become two things: a real MIME part, and
# -- where `MAIL_BASE_URL` says where this application answers -- an
# absolute address in the markdown, so that a reader which can fetch it
# draws the picture where it was written rather than at the bottom.
#
# Without `MAIL_BASE_URL` the address is left alone and the part is still
# attached: the picture arrives, in the place every mail client has always
# put one.
#
# Answers `{"source": <markdown>, "files": [<attachments>]}`.
def mail_send_source(source)
  lines = []
  files = []
  for line in source.split("\n")
    said = line.strip()
    aim = mail_send_aim(said)
    if aim == ""
      lines = lines.concat([line])
    else
      files = files.concat([mail_send_file(aim)])
      lines = lines.concat([mail_send_line(said, aim)])
    end
  end
  {"source": lines.join("\n"), "files": files.filter(fn(f) { f["filename"].nil? != true })}
end

# The path a line attaches, or "" -- a picture or a file of this
# application's own, never a link to somewhere else.
def mail_send_aim(said)
  shot = md_image_of(said)
  kept = md_file_of(said)
  at = (shot["src"] ?? kept["src"] ?? "").to_s
  at.starts_with(MAIL_ATT_DIR) ? at : ""
end

# The same line with an absolute address in it, when there is one to give.
def mail_send_line(said, aim)
  return said if MAIL_BASE_URL == ""

  said.replace(aim, MAIL_BASE_URL + "/" + aim.replace("public/", ""))
end

# One attachment. A file that has gone since it was attached is answered
# as `{}` and dropped: a draft can outlive its own folder, and a send that
# died on it would lose the letter as well as the picture.
def mail_send_file(aim)
  bytes = slurp(aim, "binary") rescue null
  return {} if bytes.nil?

  {
    "filename": mail_send_leaf(aim),
    "content_type": mail_send_type(aim),
    "base64": Base64.encode(bytes)
  }
end

# The name a reader shows. The stored name carries the counter that keeps
# two attachments of the same name apart here; nobody else needs it.
def mail_send_leaf(aim)
  cut = aim.length() - 1
  while cut > 0
    return aim.substring(cut + 1, aim.length()) if aim.substring(cut, cut + 1) == "/"

    cut = cut - 1
  end
  aim
end

# What it is, from what it is called. A guess, and the honest default for
# everything else is the one that says so.
def mail_send_type(aim)
  low = aim.downcase()
  return "image/png" if low.ends_with(".png")
  return "image/jpeg" if low.ends_with(".jpg") || low.ends_with(".jpeg")
  return "image/webp" if low.ends_with(".webp")
  return "image/gif" if low.ends_with(".gif")
  return "application/pdf" if low.ends_with(".pdf")
  return "text/plain" if low.ends_with(".txt")
  return "text/markdown" if low.ends_with(".md")
  return "text/csv" if low.ends_with(".csv")
  return "application/zip" if low.ends_with(".zip")

  "application/octet-stream"
end

def mail_send(state)
  out = set_key(state, "sending", "")
  draft = state["writing"]
  return out if draft.nil?


  if mail_demo?()
    return set_key(mail_write_close(out), "note", "The demo has nothing to send from.")
  end

  address = state["address"] ?? ""
  subject = (draft["subject"] ?? "").trim()
  body = mail_draft_body(draft)
  letter = {
    "from": address,
    "to": (draft["to"] ?? "").trim(),
    "subject": subject == "" ? "(no subject)" : subject,
    "text": body
  }

  # Three faces of one source.
  #
  # What is typed is markdown -- that is the format of the composer, not a
  # mode it can be in -- and Soli renders the other two from it:
  # `to_text` for a reader that draws nothing, `to_html` for one that
  # draws. The markdown itself rides along as `text/markdown`, so a reader
  # that prefers the source has it, and so does the person, in their sent
  # folder, in the form they wrote it.
  #
  # `alternatives` is the mailer's third part: `text_body` and `html_body`
  # are the two mail-builder names, and anything else needs the structure
  # assembled by hand. Ordered least rich to most (RFC 2046 §5.1.4), which
  # is plain, markdown, HTML.
  # The pictures and the files the letter carries, before the three faces
  # are rendered: `carried["source"]` is the same markdown with absolute
  # addresses where this application has one to give, which is what makes
  # a picture appear in the letter rather than only under it.
  carried = mail_send_source(body)
  body = carried["source"]
  letter["text"] = Markdown.to_text(body)
  letter["html"] = Markdown.to_html(body)
  letter["alternatives"] = [{"content_type": "text/markdown", "body": body}]
  letter["attachments"] = carried["files"] unless carried["files"].length() == 0
  letter["cc"] = (draft["cc"] ?? "").trim() if (draft["cc"] ?? "").trim() != ""
  # What makes an answer an answer. A reader threads on these two and not
  # on the subject, which is why a `Re:` alone lands as a new message.
  threading = {}
  threading["In-Reply-To"] = draft["ref"] if (draft["ref"] ?? "") != ""
  threading["References"] = draft["refs"] if (draft["refs"] ?? "") != ""
  letter["headers"] = threading if threading.keys().length() > 0

  # The account's own server, set immediately before the delivery that
  # reads it. `Mailer`'s configuration is process-wide, so this is the
  # one part of the application where two accounts could tread on each
  # other -- and they cannot here, because a session's events are
  # serialised and the configure and the deliver are one event. Nothing
  # else in this file has a global to be careful about.
  one = state["account"] ?? {}
  Mailer.configure({
    "delivery_method": "smtp",
    "host": one["smtp"] ?? MAIL_SMTP,
    "port": one["smtp_port"] ?? MAIL_SMTP_PORT,
    "tls": MAIL_SMTP_TLS,
    "user": address, "pass": state["secret"] ?? "", "from": address
  })
  said = ""
  try
    Mailer.deliver(letter)
  rescue e
    said = str(e)
  end
  mail_say("send to=" + (letter["to"] ?? "") + " -> " + (said == "" ? "gone" : "FAILED: " + said))
  return set_key(out, "error", "It would not go out. " + mail_cap(said, 200)) if said != ""

  gone = mail_write_close(out)
  set_key(gone, "note", "Sent to " + mail_cap(letter["to"] ?? "", 60))
end

# A saved account signs you in without asking. The fetch does not happen
# here -- this only arms the loader, and the loader asks for it.
def mail_resume(state)
  list = mail_accounts_load()
  out = set_key(state, "accounts", list)
  return out if list.length() == 0

  return mail_into(out, list[0])
end

# Sign in as one of them: the account becomes the current one and its
# stored mailbox goes on screen before any packet leaves.
#
# `address` and `secret` stay flat in state because everything below the
# account layer reads them there, and there are sixty such reads. The
# account itself is kept beside them for the two things the address does
# not answer: which host to send through, and what to draw in the bar.
# Another folder, which is very nearly another account: a different list,
# a different store, a different set of UIDs -- and the same account, the
# same connection and the same credentials.
#
# What it is *not* is a fetch. The folder's own store goes on screen and
# the tick asks the server what is in it, exactly as `mail_into` does for
# an account, and for the same reason: a `SELECT` on a folder never opened
# is a round trip, and a round trip under the frame lock is a window that
# does not answer.
# The folder this session is looking at. Empty means the inbox, which is
# what every call meant before there were folders -- so an old call site
# that has not been taught yet still reads the inbox rather than whatever
# was selected last.
# Whether this session still owes itself a folder list.
def mail_wants_folders?(state)
  return false if (state["linked"] ?? false) != true
  return false if (state["asked_folders"] ?? false) == true

  # Asked once a session even when the rail is already full: `LIST` is how
  # a folder made this morning arrives. What the stored list buys is that
  # the rail is *never empty* while that round trip happens, not that it
  # is never made.
  true
end

# One `LIST`, and the rail has something to draw.
#
# `asked_folders` is set either way: a server that answers with one
# mailbox, or with none, has answered -- and asking it again every tick
# for the life of the session would be a round trip a quarter of a second
# for ever.
# Whether any folder still owes this session its counts.
def mail_wants_counts?(state)
  return false if (state["linked"] ?? false) != true
  return false if mail_demo?()

  mail_counts_next(state) != ""
end

# The next folder whose counts this *session* has not asked for. Stored
# counts fill the rail; they do not count as asked, so every session
# refreshes them once -- one folder a tick, behind whatever else is
# happening.

# The next folder without counts, or "" when they all have them.
def mail_counts_next(state)
  asked = state["counted"] ?? {}
  counts = state["counts"] ?? {}
  now = mail_now()
  found = ""
  for one in (state["folders"] ?? [])
    name = one["name"] ?? ""
    # Asked already this session, or answered recently enough that asking
    # again would be a round trip for a number that has not moved. The
    # stored answer carries when it was true, so a reconnect costs
    # nothing -- which is what this is for: every reconnect used to redo
    # the whole walk, twenty-three round trips, and a window that had just
    # come back looked like a window that fetches all the time.
    next unless asked[name].nil?

    kept = counts[name] ?? {}
    next if now - (kept["at"] ?? 0) < MAIL_COUNTS_FRESH

    found = name if found == "" && name != ""
  end
  found
end

# One `STATUS`, one folder, one tick.
def mail_counts_step(state)
  name = mail_counts_next(state)
  return state if name == ""

  said = mail_status(state["address"] ?? "", state["secret"] ?? "", name)
  counts = state["counts"] ?? {}
  # A folder the server would not answer for is recorded as answered with
  # nothing: asking again every tick for the life of the session would be
  # a round trip a quarter of a second for ever.
  # When it was true travels with what was true: `mail_counts_next` reads
  # it to decide whether to ask again, and it is stored with the folder.
  counts[name] = set_key(said, "at", mail_now()) if said.nil? != true
  mail_counts_save(state["address"] ?? "", name, counts[name]) if said.nil? != true
  asked = state["counted"] ?? {}
  asked[name] = true
  out = set_key(state, "counted", asked)
  set_key(out, "counts", counts)
end

def mail_folders_step(state)
  address = state["address"] ?? ""
  said = mail_folders(address, state["secret"] ?? "")
  out = set_key(state, "asked_folders", true)
  mail_say("folders: " + str(said.length()) + " from the server")
  # A server that answered with nothing has not answered: keep whatever
  # was stored rather than emptying the rail on a failed `LIST`.
  return out if said.length() == 0

  mail_folders_save(address, said)
  set_key(out, "folders", said)
end

def mail_here(state)
  state["box"] ?? MAIL_BOX
end

# The caret on to the rail, at the folder you are in.
# The keys the rail answers to while it has the caret. Every other key
# hands the keyboard back to the list, which is what makes this a mode you
# leave by using the application rather than by remembering to.
def mail_rail_key?(key)
  # `Enter` is in the list because of the folders *screen*. On the rail it
  # never reaches this function at all -- the frame lends it to the
  # focused row, which activates itself -- but on the screen it arrives
  # here, and a key that is about to act on the caret must not be the key
  # that clears it first.
  key == "ArrowDown" || key == "ArrowUp" || key == "j" || key == "k" ||
    key == "F" || key == "Enter"
end

def mail_rail_enter(state)
  folders = state["folders"] ?? []
  return state if folders.length() == 0

  # On a narrow window there is no rail to put a caret on -- the folders
  # are hidden, because a folder list that takes a third of a phone is a
  # folder list nobody reads a message next to. So `F` opens them as a
  # screen, the way `@` opens the accounts: the same list, taking the room
  # it needs, and a press puts it away again.
  unless mail_layout(state)["roomy"]
    out = mail_screen(state, "sheet_folders")
    # The caret starts where you are, so the first `↓` is the folder
    # after this one rather than the first in the list.
    return set_key(out, "rail_focus", mail_here(state))
  end

  out = set_key(state, "rail_focus", mail_here(state))
  set_key(out, "rail_to", true)
end

# The menu a narrow window puts everything behind.
#
# Not a new set of actions -- the same ones the wide masthead shows, at
# the size a thumb needs, each with the key that does the same thing so
# the two ways of driving this application stay one thing to learn.
def mail_menu_door?(event)
  event == "folders_open" || event == "write_new" || event == "find" ||
    event == "refresh" || event == "accounts" || event == "sheet" ||
    event == "signout" || event == "box_pick"
end

def mail_menu_sheet(lay, state)
  m = lay["measure"]
  here = state["address"] ?? ""
  doors = [
    ["Folders", "F", "folders_open", (state["folders"] ?? []).length() > 1],
    ["Write a message", "n", "write_new", true],
    ["Search", "/", "find", true],
    ["Refresh", "r", "refresh", true],
    ["Accounts", "@", "accounts", true],
    ["Keys", "?", "sheet", true],
    ["Sign out of " + here, "q", "signout", here != ""]
  ]
  rows = doors.filter(fn(d) { d[3] == true }).map(fn(d) { mail_menu_card(d[0], d[1], d[2], m) })
  {"k": "scroll", "s": {"grow": 1, "width": "100%"}, "c": [
    column({"width": "100%", "align": "center", "pad": [7, lay["pad"], 10, lay["pad"]]}, [
      column({"width": m, "gap": 6}, [
        column({"gap": 2, "width": m}, [
          text("Menu", {"size": 6, "weight": "bold", "fg": "accent.base", "width": m}),
          text(mail_folder_name({"name": mail_here(state)}) + " · Escape closes this.", {
            "size": 1, "fg": "text.muted", "width": m
          })
        ]),
        column({"width": m, "gap": 3}, rows)
      ])
    ])
  ]}
end

def mail_menu_card(label, key, event, m)
  {
    "k": "box",
    "s": {
      "display": "row", "align": "center", "gap": 4, "width": m,
      "pad": [4, 5, 4, 5], "radius": 3, "cursor": "pointer", "bg": "surface.raised"
    },
    "on": {"click": event},
    "p": {"role": "button", "label": label},
    "c": [
      text(mail_fit(label, m - 80, 6), {"size": 3, "fg": "text.default", "grow": 1, "shrink": 1}),
      text(key, {"font": "mono", "size": 1, "fg": "text.muted", "shrink": 0})
    ]
  }
end

# Every folder, as a screen. What the rail is on a window with room for
# it.
def mail_folders_sheet(lay, state)
  m = lay["measure"]
  here = mail_here(state)
  list = state["folders"] ?? []
  caret = state["rail_focus"] ?? ""
  rows = list.map(fn(one) {
    mail_folder_card(one, (one["name"] ?? "") == here, m, (one["name"] ?? "") == caret)
  })
  {"k": "scroll", "s": {"grow": 1, "width": "100%"}, "c": [
    column({
      "width": "100%", "align": "center",
      "pad": [lay["roomy"] ? 10 : 7, lay["pad"], 10, lay["pad"]]
    }, [
      column({"width": m, "gap": 6}, [
        column({"gap": 2, "width": m}, [
          text("Folders", {"size": 6, "weight": "bold", "fg": "accent.base", "width": m}),
          text(str(list.length()) + " in this account · ↑ ↓ then Enter · Escape closes this.", {
            "size": 1, "fg": "text.muted", "width": m
          })
        ]),
        column({"width": m, "gap": 3}, rows)
      ])
    ])
  ]}
end

# One folder on that screen: the whole card is the press, and the one you
# are in is filled rather than ticked -- the same distinction the rail
# makes, at the size a thumb needs.
def mail_folder_card(one, here, m, caret = false)
  name = mail_folder_name(one)
  kind = one["kind"] ?? ""
  {
    "k": "box",
    "s": {
      "display": "row", "align": "center", "gap": 4, "width": m,
      "pad": [4, 5, 4, 5], "radius": 3, "cursor": "pointer",
      "bg": here == true ? "accent.base" : "surface.raised",
      # Where the arrows are, outlined; where you are, filled. The same
      # two things the rail distinguishes, at the size a thumb needs.
      "border": caret == true ? 2 : 0,
      "border_color": "focus.ring"
    },
    "on": {"click": "box_pick"},
    "p": {"box": one["name"] ?? "", "role": "button", "label": name},
    "c": [
      text(mail_fit(name, m - 140, 6), {
        "size": 3, "weight": here == true ? "semibold" : "regular",
        "fg": here == true ? "text.inverted" : "text.default",
        "grow": 1, "shrink": 1
      }),
      kind == "" ? text("", {"size": 0}) : text(kind, {
        "font": "mono", "size": 0, "shrink": 0,
        "fg": here == true ? "text.inverted" : "text.muted"
      })
    ]
  }
end

# And along it. The client's focus follows, because `Enter` is activated
# by whatever the *client* thinks is focused -- the rail's claim on that
# key is a loan (`mail_view`), not an interception.
def mail_rail_step(state, by)
  folders = state["folders"] ?? []
  return state if folders.length() == 0

  here = state["rail_focus"] ?? ""
  at = 0
  i = 0
  for one in folders
    at = i if (one["name"] ?? "") == here
    i = i + 1
  end
  to = clamp(at + by, 0, folders.length() - 1)
  out = set_key(state, "rail_focus", (folders[to] ?? {})["name"] ?? "")
  set_key(out, "rail_to", true)
end

def mail_box_pick(state, name)
  mail_say("box_pick: " + str(name) + " (here " + mail_here(state) + ")")
  # The folder is chosen; the keyboard belongs to the list again --
  # *before* the two answers below, because choosing the folder you are
  # already in is still choosing it. Without this, `Tab` then `Enter` on
  # the current folder left the caret's claim on `Enter` standing and no
  # message would open for the rest of the session.
  state = set_key(state, "rail_focus", "")
  state = set_key(state, "sheet_folders", false)
  return state if name == ""
  return state if name == (state["box"] ?? MAIL_BOX)

  address = state["address"] ?? ""
  out = set_key(state, "box", name)
  # Going there is reading it: the count the poll kept is spent.
  out = set_key(out, "inbox_new", 0) if name == MAIL_BOX
  # Another folder is another list, and a mark made in this one means
  # nothing there.
  out = set_key(out, "picked", [])
  out = set_key(out, "msgs", [])
  out = set_key(out, "all", [])
  out = set_key(out, "total", 0)
  out = set_key(out, "fetched", 0)
  out = set_key(out, "cursor", 0)
  out = set_key(out, "scroll", 0)
  out = set_key(out, "reading", false)
  out = set_key(out, "finding", false)
  out = set_key(out, "query", "")
  out = set_key(out, "error", "")
  out = set_key(out, "note", "")
  out = set_key(out, "mode", "all")

  kept = mail_store_load(address, name)
  unless kept.nil?
    out = set_key(out, "msgs", kept["msgs"])
    out = set_key(out, "all", kept["msgs"])
    out = set_key(out, "total", kept["total"])
    out = set_key(out, "uidvalidity", kept["uidvalidity"])
    out = set_key(out, "mode", "new")
  end
  # The demo has no server to open a folder on: the rail moves, the list
  # empties, and nothing is asked of a mailbox that does not exist.
  return out if mail_demo?()

  mail_arm(out, "Opening " + mail_folder_name({"name": name}) + "…")
end

def mail_into(state, one, quiet = false)
  address = one["address"] ?? ""
  mail_learn(one)
  out = set_key(state, "account", one)
  out = set_key(out, "address", address)
  out = set_key(out, "secret", one["secret"] ?? "")
  out = set_key(out, "fetched", 0)
  out = set_key(out, "msgs", [])
  out = set_key(out, "all", [])
  out = set_key(out, "total", 0)
  out = set_key(out, "cursor", 0)
  out = set_key(out, "scroll", 0)
  out = set_key(out, "reading", false)
  out = set_key(out, "linked", false)
  out = set_key(out, "error", "")
  out = set_key(out, "query", "")
  out = set_key(out, "finding", false)
  # Marks are UIDs in *this* mailbox, and the next account's UID 4 is a
  # different message entirely.
  out = set_key(out, "picked", [])
  # Another account's folders are not this one's, and the rail must not
  # show the last account's while the new one is being fetched.
  out = set_key(out, "box", MAIL_BOX)
  # Going there is reading it: whatever the poll counted for this
  # account while you were in another one is spent.
  news = state["other_new"] ?? {}
  news[address] = 0
  out = set_key(out, "other_new", news)
  # From the cache when this worker has already asked, and from the demo's
  # fixed list when there is no server to ask. Empty otherwise, and the
  # first tick fills it (`mail_folders_step`).
  # From the demo's fixed list, from this worker's cache, or from the
  # database -- in that order, and the database is the one that makes the
  # rail survive a reconnect. The tick asks the server anyway, once, and
  # writes back whatever has changed.
  said = mail_demo?() ? MAIL_DEMO_FOLDERS : (MAIL_FOLDERS_OF[address] ?? [])
  # Ordered again on the way out of the database: rows come back in
  # whatever order the store finds them, and the rail's order -- inbox
  # first, then what the server gave a meaning to -- is the application's
  # to impose, not the store's to remember.
  said = mail_folder_order(mail_folders_load(address)) if said.length() == 0
  MAIL_FOLDERS_OF[address] = said if said.length() > 0 && mail_demo?() != true
  out = set_key(out, "folders", said)
  out = set_key(out, "asked_folders", false)
  # What was known about them last time, so the rail has its numbers in
  # the first frame and the `STATUS` walk only refreshes them.
  known = {}
  for one in said
    kept = one["counts"] ?? {}
    known[one["name"] ?? ""] = kept if kept.keys().length() > 0
  end
  out = set_key(out, "counts", known)

  # The mode matters as much as the list: with a mailbox on hand the
  # refresh asks only for UIDs above the highest one held, and without
  # one it walks the newest `MAIL_LIMIT` by sequence number.
  kept = mail_store_load(address, mail_here(state))
  out = set_key(out, "mode", "all")
  fresh = false
  if !kept.nil?
    out = set_key(out, "msgs", kept["msgs"])
    out = set_key(out, "all", kept["msgs"])
    out = set_key(out, "total", kept["total"])
    out = set_key(out, "uidvalidity", kept["uidvalidity"])
    out = set_key(out, "linked", true)
    out = set_key(out, "mode", "new")
    fresh = mail_now() - (kept["at"] ?? 0) < MAIL_FRESH
    # "Fewer than a page" used to mean stale. At twenty that was a rare
    # mailbox; at a hundred it is nearly every one -- so a window reopened
    # a second after it closed spent a 1200 ms handshake, under the frame
    # lock, to ask a question it had just asked. A page's worth *or the
    # whole mailbox* is a full hand: there is nothing more to fetch.
    have = kept["msgs"].length()
    fresh = false if have < MAIL_LIMIT && have < (kept["total"] ?? have)
  end
  # A mailbox stored a moment ago is shown and left alone. Arming the
  # refresh here is what made every reopen freeze: the first thing a
  # refresh does is shake hands with Gmail, and that happens under the
  # frame lock.
  mail_say("into " + address + ": store is " + (fresh ? "fresh, not refreshing" : "stale, refreshing"))
  return out if fresh
  # Switching is a render, not a fetch.
  #
  # Every account's store goes stale in two minutes, so switching used to
  # arm a refresh every time -- and a refresh begins with a handshake,
  # under the session's frame lock, which is about a second in which the
  # window answers nothing. The list was on screen immediately and the
  # next key was not.
  #
  # There is somewhere to put that work now: the poll (`MAIL_POLL`) comes
  # round on its own and asks the same question. So a switch shows what is
  # stored and stops there, and `r` is the way to insist.
  return out if quiet == true && (out["msgs"] ?? []).length() > 0
  # The demo has two accounts and no network. The first one is re-seeded
  # so switching away and back is the round trip it would be with a store
  # on disk; the second shows what an account with nothing stored looks
  # like, which is the honest answer and not a server dialled on a
  # credential that was never real.
  if mail_demo?()
    return mail_seed(out) if address == "you@gmail.com"

    return set_key(out, "linked", true)
  end

  set_key(out, "busy", "Fetching your mail…")
end

# `@`, and the digits. Switching is signing in as the other one: the
# connection to this account is left open -- the pool is keyed by address
# and a switch back costs nothing -- but its list, its cursor and its
# open letter are dropped, because they belong to a different mailbox.
def mail_switch(state, address)
  list = state["accounts"] ?? []
  one = mail_accounts_find(list, address)
  return set_key(state, "sheet_accounts", false) if one.nil?
  return set_key(state, "sheet_accounts", false) if address == (state["address"] ?? "")

  out = set_key(state, "sheet_accounts", false)
  mail_into(out, one, true)
end

def mail_kind_gmail(state)
  out = set_key(state, "kind", "gmail")
  out = set_key(out, "host", "")
  out = set_key(out, "port", "993")
  set_key(out, "smtp", "")
end

# Which of the four screens is up, if any: the keys, the accounts, the
# folders, the menu.
#
# They are four flags rather than one name because each was added on its
# own, and `mail_view` asks for them in a fixed order -- so a screen that
# raised its own flag without lowering the others simply did not appear,
# and the key that raised it looked dead. `@` from the folders screen was
# exactly that: the menu's flag went up behind a screen that is tested
# first, and nothing moved.
MAIL_SCREENS = ["sheet", "sheet_accounts", "sheet_folders", "sheet_menu"]

def mail_screen(state, want)
  out = state
  for name in MAIL_SCREENS
    out = set_key(out, name, name == want)
  end
  out
end

def mail_screens_off(state)
  mail_screen(state, "")
end

def mail_accounts_open(state)
  return mail_screens_off(state) if (state["sheet_accounts"] ?? false) == true

  mail_screen(state, "sheet_accounts")
end

# Adding one is the sign-in screen again, with the difference that there
# is somewhere to go back to. `linked` is what that screen is chosen by
# (`mail_view`), so adding turns it off and cancelling turns it back on
# -- the mailbox underneath is untouched either way, because nothing else
# in state was cleared.
def mail_add_open(state)
  out = set_key(state, "sheet_accounts", false)
  # Which account to come back to if this is cancelled.
  out = set_key(out, "was", state["address"] ?? "")
  out = set_key(out, "adding", true)
  out = set_key(out, "linked", false)
  out = set_key(out, "busy", "")
  out = set_key(out, "error", "")
  out = set_key(out, "kind", "gmail")
  out = set_key(out, "address", "")
  out = set_key(out, "host", "")
  out = set_key(out, "port", "993")
  out = set_key(out, "smtp", "")
  set_key(out, "secret", "")
end

def mail_add_close(state)
  list = state["accounts"] ?? []
  out = set_key(state, "adding", false)
  out = set_key(out, "error", "")
  return set_key(out, "linked", false) if list.length() == 0

  # Back to whatever was on screen, which is the account that was
  # current: its mailbox is still in state, so this is a render and not a
  # fetch.
  one = mail_accounts_find(list, state["was"] ?? "")
  return mail_into(out, list[0]) if one.nil?

  mail_into(out, one)
end

# The nth account, for the number keys.
def mail_switch_at(state, n)
  list = state["accounts"] ?? []
  return state if n < 0 || n >= list.length()

  mail_switch(state, (list[n] ?? {})["address"] ?? "")
end

# Pressing the button does not fetch anything. It sets `busy`, and that is
# the whole of the loader: the view answers a busy state with a spinner in
# the masthead carrying a `wake` prop, the client asks again a moment
# later (06 §1.1), and *that* event does the work.
#
# Two round trips instead of one, and the reason is that an event handler
# *is* the round trip. A fetch done here would hold the batch until the
# messages came back, so the window would show the sign-in screen, frozen,
# for as long as Gmail took -- there is no frame in between to put a
# spinner in. Splitting the work off the press is what buys that frame.
# `R`. Read every row again, rather than only what is new.
#
# `r` asks for UIDs above the highest one held, which is the cheap
# question and the right one nearly always -- but it means a row fetched
# by an older build keeps whatever that build knew. When the header fetch
# learns a new field (a message's size, its paper clips), every row
# already on disk is missing it and no amount of refreshing will fix
# that, because refreshing does not look at them.
#
# This walks the newest `MAIL_LIMIT` by sequence number again and
# overwrites them in place: the list stays on screen the whole time and
# each batch replaces its rows as it lands.
def mail_reload(state)
  return state if (state["linked"] ?? false) != true

  out = set_key(state, "fetched", 0)
  out = set_key(out, "mode", "all")
  out = set_key(out, "error", "")
  set_key(out, "busy", "Reading every row again…")
end

def mail_arm(state, said)
  address = state["address"] ?? ""
  app_password = state["secret"] ?? ""
  return set_key(state, "error", "Both lines, please.") if address == "" || app_password == ""

  # What the sign-in screen was holding becomes an account -- host and
  # all, which for Gmail is the default and for everything else is what
  # the two extra lines were for. It is not written to disk here: a
  # credential is remembered when the server accepts it, not when it is
  # typed.
  out = state
  if (state["linked"] ?? false) != true
    out = set_key(out, "account", mail_account(
      address, app_password,
      state["host"] ?? "", (state["port"] ?? "0").to_i(),
      state["smtp"] ?? "", (state["smtp_port"] ?? "0").to_i()
    ))
  end
  out = set_key(set_key(out, "error", ""), "busy", said)
  out = set_key(out, "fetched", 0)
  # A refresh with a mailbox on hand asks only for what is above the
  # highest UID it holds; with nothing, it walks the newest by sequence
  # number. Either way the old list stays on screen and is overwritten,
  # so refreshing never blanks the window.
  have = (state["msgs"] ?? []).length()
  set_key(out, "mode", have > 0 ? "new" : "all")
end

# Signing out is the one thing that deletes the saved account, and it
# takes the stored mailbox with it.
# `q`. One account, not all of them: its credential and its copy of its
# mail go, and whatever is left becomes the one on screen. With nothing
# left it is the sign-in screen, which is what it always was.
def mail_out(state)
  address = state["address"] ?? ""
  mail_box_close(address)
  list = mail_accounts_drop(state["accounts"] ?? [], address)
  mail_store_forget(address)
  MAIL_WHERE[address] = nil

  blank = {"viewport": state["viewport"], "accounts": list}
  return blank if list.length() == 0

  mail_into(set_key(blank, "note", "Signed out of " + address), list[0])
end

# The wake's one handler, and the whole of the background: everything this
# application does to Gmail after a key was pressed happens here, on a
# tick, rather than inside the event that asked for it.
# One message, once. Rows are keyed by UID, and two children of one box
# may not carry the same key -- the server refuses the frame with
# `key 'm:<uid>' is used by two children`, and the window keeps the last
# good frame, so the symptom is a list that stops updating.
#
# Every merge below is a `concat` of what is held and what just came
# back, and the two overlap whenever `fetched` -- a count of rows -- stops
# matching the stretch of the mailbox those rows cover. That happens for
# ordinary reasons: a batch that arrived with a gap in it, a message
# deleted from another client between two batches, the walk resuming with
# `fetched` reset to zero over a stored list. `mail_pull_range` skips by
# sequence number, so a mismatch of one is enough for the next batch to
# return a message already on screen.
#
# Keeping the first occurrence is what makes this safe: the lists are
# newest first, and the copy already held is the one the cursor, the open
# letter and the `seen` flag refer to.
def mail_fetch(state)
  # A draft on its way out first: it is the only errand that is not
  # repeatable, and the one somebody is waiting on.
  return mail_send(state) if (state["sending"] ?? "") != ""
  return mail_clip_step(state) if (state["clipping"] ?? nil).nil? != true
  # Then the rows on fire: somebody pressed `d` a quarter of a second ago
  # and is watching the row to see whether it meant it.
  return mail_burn_step(state) if (state["burning"] ?? []).length() > 0
  # The folder list, once, on a tick rather than in the press that needed
  # it: `LIST` is a round trip, and a round trip inside an event is a
  # window that does not answer. The rail appears a quarter of a second
  # after the list does, which nobody notices, instead of the sign-in
  # taking a second longer, which everybody does.
  return mail_folders_step(state) if mail_wants_folders?(state)
  # Then how much is in each of them, one folder a tick.
  #
  # `STATUS` asks about a mailbox the connection is *not* in and leaves
  # the selection alone -- which is the whole reason it exists, and the
  # reason this is not `SELECT`. Twenty-three folders is twenty-three
  # round trips, so they are spread one per tick: ten seconds in which
  # nothing is held for longer than one question, instead of three
  # seconds in which everything is.
  return mail_counts_step(state) if mail_wants_counts?(state)

  # A wake with nothing running is the poll: the window has been idle for
  # `MAIL_POLL`, so ask what arrived. `mail_arm` puts it in "new" mode,
  # which asks only for UIDs above the highest one held -- so the usual
  # answer is one round trip and no messages.
  mail_say("tick: busy=" + str((state["busy"] ?? "") != "") + " errands=" + str(mail_errands(state)))
  if (state["busy"] ?? "") == "" && mail_errands(state) == 0
    return state if (state["linked"] ?? false) != true
    return state if (state["mode"] ?? "") == "hits"
    # Not while a hand is on the keyboard.
    #
    # IMAP happens inside the event that asks for it, so it holds the
    # session's frame lock for as long as it takes -- and a poll that
    # lands between two keystrokes is felt as the application stopping.
    # It is the one piece of work here with no one waiting for it, so it
    # is also the one that can always wait: the next wake is ninety
    # seconds away and the mail will still be there.
    return state if mail_now() - (state["touched"] ?? 0) < MAIL_QUIET

    # Ask the cheap question first. Nothing new is the answer nearly every
    # time, and that answer costs one command and shows nothing: no
    # spinner, no fetch, no second round trip.
    address = state["address"] ?? ""
    secret = state["secret"] ?? ""
    here = mail_here(state)
    high = mail_high(state["msgs"] ?? [])
    fresh = mail_peek(address, secret, high, here)
    mail_say("poll " + here + " above " + str(high) + ": " + (fresh.nil? ? "no answer" : str(fresh.length()) + " new"))
    if fresh.nil? != true && fresh.length() > 0
      # Told when the mail is *there*, not when we learn it exists.
      #
      # A `UID SEARCH` answers in 130 ms and the headers take another
      # round trip, so announcing here put the bubble on screen a second
      # before the message was in the list -- and a notification about a
      # message you cannot see yet is a notification you cannot act on.
      # The count is remembered and spent by the fetch that lands it.
      armed = mail_arm(state, "Fetching " + str(fresh.length()) + (fresh.length() == 1 ? " new message…" : " new messages…"))
      return set_key(armed, "telling", fresh.length())
    end

    # One other account, in turn.
    #
    # Every account every cycle would be one IMAP round trip per account
    # inside the session's frame lock; one per cycle is a round trip, and
    # with two accounts each is asked about every minute. The answer is a
    # notification and a number beside the account -- never a fetch: mail
    # of an account you are not in does not belong in the list you are
    # looking at.
    out = mail_others_peek(state, address)
    return out if out != state

    # And the inbox, when you are somewhere else.
    #
    # Not every folder: twenty-three of them is twenty-three round trips
    # every ninety seconds, inside the session's frame lock, to answer a
    # question about mail nobody is looking at. The inbox is where new
    # mail lands, so it is the one other box worth a question -- and the
    # answer is a number in the rail, not a fetch: reading it is what
    # opening the folder is for.
    return state if here == MAIL_BOX

    return mail_inbox_peek(state, address, secret)
  end

  # Then the letter: it is the errand with a person watching a spinner
  # for it. Then the fetch walk.
  return mail_errand(state) if (state["want_body"] ?? 0) > 0 || (state["want_seen"] ?? 0) > 0
  return mail_shoot_next(state) if (state["shooting"] ?? []).length() > 0
  return state if (state["busy"] ?? "") == ""
  return mail_fetch_new(state) if (state["mode"] ?? "all") == "new"
  return mail_fetch_hits(state) if (state["mode"] ?? "all") == "hits"
  return mail_fetch_older(state) if (state["mode"] ?? "all") == "older"

  mail_fetch_all(state)
end

# The next account that is not this one, asked about in turn.
#
# Its own connection, its own credentials, its own high-water mark -- the
# connection cache is keyed by address (`MAIL_OPEN`), so this costs a
# handshake once per account per worker and a `UID SEARCH` after that.
def mail_others_peek(state, here)
  list = (state["accounts"] ?? []).filter(fn(a) { (a["address"] ?? "") != here })
  return state if list.length() == 0

  at = state["other_at"] ?? 0
  at = 0 if at >= list.length()
  one = list[at] ?? {}
  address = one["address"] ?? ""
  out = set_key(state, "other_at", at + 1)
  return out if address == ""

  marks = state["other_high"] ?? {}
  seen = marks[address] ?? 0
  if seen < 1
    # What that account's own store holds is the mark to measure against,
    # so a first pass learns rather than announcing a hundred messages
    # somebody has already read.
    kept = mail_store_load(address, MAIL_BOX)
    seen = mail_high(kept.nil? ? [] : (kept["msgs"] ?? []))
  end
  fresh = mail_peek(address, one["secret"] ?? "", seen, MAIL_BOX)
  return out if fresh.nil?

  high = seen
  for u in fresh
    high = u if u > high
  end
  marks[address] = high
  out = set_key(out, "other_high", marks)
  return out if fresh.length() == 0

  mail_say("other " + address + ": " + str(fresh.length()) + " new above " + str(seen))
  mail_tell(fresh.length(), "Inbox", address)
  counts = state["other_new"] ?? {}
  counts[address] = (counts[address] ?? 0) + fresh.length()
  set_key(out, "other_new", counts)
end

# How many messages the inbox has that this session has not seen, kept in
# state so the rail can say so.
#
# The count is the point rather than the mail: fetching another folder's
# messages into a list that is showing this one would be a list that
# lies, and a number beside `Inbox` is the whole of what somebody in
# another folder wants to know.
# One line said out loud, where something happened.
#
# `eui_notify` is a call in a handler rather than a value in a tree
# (02 §5.2): a notification is not part of the document, so there is no
# node to put it on. It goes to the session whose handler asked for it and
# to no other, which is why the poll is the right place for it -- that is
# the tick that found the mail.
#
# The tag is the account and the folder, so ten arrivals in one afternoon
# replace each other instead of stacking into a wall: a client showing one
# with the tag of one already on screen is required to replace it.
def mail_tell(n, folder, address)
  return if n < 1

  said = str(n) + (n == 1 ? " new message" : " new messages")
  # Logged either way: a notification that is queued and one that is
  # refused by a capability look exactly alike from here, and the whole
  # question the first time this did not appear was which of the two had
  # happened.
  mail_say("notify: " + said + " in " + folder)
  eui_notify(said + " in " + folder, address, "mail:" + address + ":" + folder) rescue nil
end

def mail_inbox_peek(state, address, secret)
  seen = state["inbox_high"] ?? 0
  if seen < 1
    # Nothing to compare against yet: what the store holds for the inbox
    # is the high-water mark, and if it holds nothing this pass only
    # learns one.
    kept = mail_store_load(address, MAIL_BOX)
    seen = mail_high(kept.nil? ? [] : (kept["msgs"] ?? []))
  end
  fresh = mail_peek(address, secret, seen, MAIL_BOX)
  return state if fresh.nil?

  high = seen
  for u in fresh
    high = u if u > high
  end
  out = set_key(state, "inbox_high", high)
  if fresh.length() > 0
    mail_say("inbox: " + str(fresh.length()) + " new above " + str(seen))
    mail_tell(fresh.length(), "Inbox", address)
  end
  set_key(out, "inbox_new", (state["inbox_new"] ?? 0) + fresh.length())
end

# The top-up: one round trip, and usually nothing to show for it, which is
# the desired outcome.
def mail_fetch_new(state)
  address = state["address"] ?? ""
  app_password = state["secret"] ?? ""
  have = state["msgs"] ?? []
  got = mail_pull_new(
    address, app_password, mail_high(have), mail_low(have),
    state["uidvalidity"] ?? 0, MAIL_LIMIT, state["total"] ?? 0
  )
  return mail_blame(state, got["error"]) unless got["error"].nil?

  # The mailbox was rebuilt under us: every stored UID is meaningless now.
  if got["reset"] == true
    out = set_key(state, "msgs", [])
    out = set_key(out, "all", [])
    out = set_key(out, "uidvalidity", got["uidvalidity"] ?? 0)
    out = set_key(out, "fetched", 0)
    return set_key(out, "mode", "all")
  end

  fresh = got["msgs"] ?? []
  total = got["total"] ?? (state["total"] ?? 0)

  # Drop what the mailbox no longer has.
  alive = got["alive"] ?? []
  kept_have = have
  if alive.length() > 0
    kept_have = have.filter(fn(m) {
      uid = m["uid"] ?? 0
      alive.filter(fn(a) { a == uid }).length() > 0
    })
  end
  all = mail_unique(fresh.concat(kept_have))
  all = all.slice(0, MAIL_LIMIT) if all.length() > MAIL_LIMIT

  mail_store_save(address, all, total, got["uidvalidity"] ?? 0, mail_here(state)) if fresh.length() > 0
  out = set_key(state, "msgs", all)
  out = set_key(out, "all", all)
  out = set_key(out, "total", total)
  out = set_key(out, "uidvalidity", got["uidvalidity"] ?? 0)
  out = set_key(out, "error", "")
  out = set_key(out, "linked", true)

  # A top-up that finds nothing new is not the end of the story if the
  # list itself is short. The store is written after every batch, so a
  # first run that was interrupted -- or simply a fresh one, which stores
  # five and then reconnects -- leaves a stub behind; resume finds it,
  # switches to asking only for *newer* mail, and there is never any, so
  # the list sat at five for ever. Filling up to `MAIL_LIMIT` is the rest
  # of the story, and it hands over to the same walk that pages.
  out = set_key(out, "fetched", all.length())
  # What the poll found, now that it is on screen. Only what a poll armed
  # is announced: the same code path fills a list at connect, and
  # "a hundred new messages" is not news, it is a mailbox.
  owed = state["telling"] ?? 0
  if owed > 0 && fresh.length() > 0
    mail_tell(owed, mail_folder_name({"name": mail_here(state)}), address)
    out = set_key(out, "telling", 0)
  end
  short = all.length() < MAIL_LIMIT && all.length() < total
  out = set_key(out, "mode", short ? "older" : "new")
  set_key(out, "busy", short ? "Fetching" : "")
end

# The search results, a batch at a time. Nothing is stored: a search is a
# view of the mailbox, not a copy of it, and writing it over the inbox
# would cost you the list you actually keep.
def mail_fetch_hits(state)
  uids = state["hits"] ?? []
  done = state["fetched"] ?? 0
  return set_key(state, "busy", "") if done >= uids.length()

  got = mail_pull_uids(
    state["address"] ?? "", state["secret"] ?? "", uids, done, MAIL_BATCH
  )
  return mail_blame(state, got["error"]) unless got["error"].nil?

  fresh = got["msgs"] ?? []
  all = mail_unique((state["msgs"] ?? []).concat(fresh))
  fetched = done + MAIL_BATCH
  out = set_key(state, "msgs", all)
  out = set_key(out, "fetched", fetched)
  set_key(out, "busy", fetched < uids.length() ? "Fetching" : "")
end

# Older mail, appended. The same ranged fetch the cold walk uses -- the
# messages already held are the newest, so "skip what I have" is exactly
# what asks for the next ones down.
def mail_fetch_older(state)
  address = state["address"] ?? ""
  have = state["msgs"] ?? []
  done = state["fetched"] ?? have.length()
  got = mail_pull_range(address, state["secret"] ?? "", done, MAIL_BATCH, mail_here(state))
  return mail_blame(state, got["error"]) unless got["error"].nil?

  fresh = got["msgs"] ?? []
  all = mail_unique(have.concat(fresh))
  total = got["total"] ?? (state["total"] ?? all.length())
  mail_store_save(address, all, total, state["uidvalidity"] ?? 0, mail_here(state))

  # The cursor counts the mailbox walked, not the rows kept. They used to
  # be the same number and mostly still are -- but a batch that comes back
  # holding a message already on screen adds nothing to the list, and a
  # cursor made of `all.length()` would then ask for that same stretch
  # again, for ever, one batch behind the bottom of the list. What was
  # asked for is what has been walked.
  walked = done + fresh.length()
  out = set_key(state, "msgs", all)
  out = set_key(out, "all", all)
  out = set_key(out, "total", total)
  out = set_key(out, "fetched", walked > all.length() ? walked : all.length())
  out = set_key(out, "error", "")
  # Two different appetites share this walk. Below `MAIL_LIMIT` it is
  # filling the list and keeps going by itself; at or above it, it is
  # paging, and stops after the batch so that walking off the end again
  # is what asks for the next five. Without the first half a short store
  # never grows; without the second, reaching the end of a ten-thousand
  # message inbox would quietly download all of it.
  short = all.length() < MAIL_LIMIT && all.length() < total && fresh.length() > 0
  set_key(out, "busy", short ? "Fetching" : "")
end

# The cold walk, a batch at a time, for a mailbox with nothing on hand.
def mail_fetch_all(state)
  address = state["address"] ?? ""
  app_password = state["secret"] ?? ""
  have = state["msgs"] ?? []
  linked = state["linked"] ?? false
  done = state["fetched"] ?? 0
  got = mail_pull_range(address, app_password, done, MAIL_BATCH, mail_here(state))

  unless got["error"].nil?
    return mail_blame(state, got["error"]) if linked == true

    # It was refused, so it is not an account: whatever was typed goes
    # no further, and an entry left from a password that has since
    # changed goes with it.
    dropped = mail_accounts_drop(state["accounts"] ?? [], address)
    out = set_key(mail_blame(state, got["error"]), "accounts", dropped)
    return set_key(mail_refill(out), "linked", false)
  end

  if linked != true
    one = state["account"] ?? mail_account(address, app_password, "", 0, "", 0)
    state = set_key(state, "accounts", mail_accounts_add(state["accounts"] ?? [], one))
    state = set_key(state, "adding", false)
  end

  fresh = got["msgs"] ?? []
  total = got["total"] ?? 0
  head = have.slice(0, done).concat(fresh)
  tail = have.length() > head.length() ? have.slice(head.length(), have.length()) : []
  all = mail_unique(head.concat(tail))
  fetched = done + fresh.length()

  valid = state["uidvalidity"] ?? 0
  mail_store_save(address, all, total, valid, mail_here(state))
  out = set_key(state, "msgs", all)
  out = set_key(out, "all", all)
  out = set_key(out, "total", total)
  out = set_key(out, "fetched", fetched)
  out = set_key(out, "error", "")
  out = set_key(out, "linked", true)

  ceiling = MAIL_LIMIT < total ? MAIL_LIMIT : total
  more = fetched < ceiling && fresh.length() > 0
  set_key(out, "busy", more ? "Fetching" : "")
end

# A failed fetch stops the clock and says why, and never costs you the
# mailbox you already had.
def mail_blame(state, said)
  set_key(set_key(state, "busy", ""), "error", said)
end

# Fetching the pictures of the message being read, and no other.
#
# This is the one place this application asks the network for something a
# sender chose, and it happens only because you pressed `i`. Each request
# goes out from this machine with your address attached in the sense that
# matters -- the sender learns the mail was opened -- which is why it is
# not done on open, and why a picture the HTML declares as one or two
# pixels is never fetched at all: that is not a picture, it is a receipt.
#
# The bytes land in `tmp/img` under the hash of their address. From there
# the EUI server treats them exactly as it treats any asset of this
# application: hashed, and served from `/_eui/asset` to a client that
# verifies the hash before anything decodes it (08 §3). The window never
# learns the address the picture came from, and opens no connection of its
# own -- which is the whole reason to fetch server-side rather than hand
# the client a URL, quite apart from the fact that it has no way to.
def mail_shoot(state)
  msgs = mail_shown(state)
  at = clamp(state["cursor"] ?? 0, 0, msgs.length() - 1)
  one = msgs[at] ?? {}
  html = one["html"] ?? ""
  return set_key(state, "images", true) if html == ""

  # `i` queues; it does not fetch.
  #
  # Fetching here was fifteen downloads inside the key handler, and a key
  # handler holds the session's frame lock for as long as it runs -- so
  # asking for a newsletter's pictures locked the window for several
  # seconds and looked like nothing happening. They come down a couple per
  # tick instead, and the letter fills in as they land.
  shots = state["shots"] ?? {}
  queue = []
  for b in mail_html_blocks(html)
    if b["kind"] == "image" && mail_worth_fetching?(b, shots)
      src = b["src"] ?? ""
      queue = queue.concat([src]) if queue.filter(fn(q) { q == src }).length() == 0
    end
  end
  queue = queue.slice(0, MAIL_IMG_MAX) if queue.length() > MAIL_IMG_MAX
  out = set_key(state, "images", true)
  set_key(out, "shooting", queue)
end

# The next few off the queue. Two at a time: enough that a letter fills
# quickly, few enough that no single tick holds the lock for long.
MAIL_IMG_BATCH = 2

def mail_shoot_next(state)
  queue = state["shooting"] ?? []
  return set_key(state, "shooting", []) if queue.length() == 0

  shots = state["shots"] ?? {}
  rest = queue
  n = 0
  while n < MAIL_IMG_BATCH && rest.length() > 0
    src = rest[0]
    rest = rest.slice(1, rest.length())
    got = mail_get_image(src)
    shots = set_key(shots, src, got) unless got.nil?
    n = n + 1
  end
  out = set_key(state, "shots", shots)
  set_key(out, "shooting", rest)
end

# A picture is worth the request if it is an https address this client
# would open, has not been fetched already, and is not a pixel.
def mail_worth_fetching?(b, shots)
  src = b["src"] ?? ""
  return false if !mail_openable?(src)
  return false if !shots[src].nil?

  w = b["w"] ?? 0
  h = b["h"] ?? 0
  return false if w > 0 && w < 3
  return false if h > 0 && h < 3

  true
end

def mail_get_image(src)
  name = MAIL_IMG_DIR + "/" + Crypto.sha256(src).substring(0, 40)
  got = HTTP.download(src, name) rescue nil
  return nil if got.nil?

  # The size is read from the file, not from the mail.
  #
  # `<img width>` is not a promise about the picture: these listings all
  # say `width="100%"` and no height at all, which is a *layout*
  # instruction to a browser and meaningless here. Taking it as pixels
  # made every photograph 100 wide with no height to go with it, so each
  # one came out as a tall narrow smear of itself. The bytes know their
  # own shape; ask them.
  img = Image.new(name) rescue nil
  return {"path": name, "w": 0, "h": 0} if img.nil?

  w = img.width() rescue 0
  h = img.height() rescue 0
  # No larger than it will ever be drawn.
  #
  # The client packs every picture into one 2048x2048 texture, on shelves
  # as tall as the tallest in them, and a picture that does not fit is
  # simply not drawn -- silently, and differently on each load. A
  # newsletter's two-thousand-pixel banner takes a shelf of its own for a
  # node that is never more than `MAIL_IMG_TALL` tall, so it is shrunk to
  # what the reader will actually use before it is ever uploaded.
  if w > MAIL_IMG_WIDE || h > MAIL_IMG_TALL
    small = name + ".fit.png"
    made = img.thumbnail(MAIL_IMG_WIDE).to_file(small) rescue nil
    unless made.nil?
      fit = Image.new(small) rescue nil
      return {"path": small, "w": fit.width() rescue 0, "h": fit.height() rescue 0} unless fit.nil?
    end
  end
  {"path": name, "w": w, "h": h}
end

# ----------------------------------------------------------------- the views

# Every key this application answers to, declared once. A node holding a
# `key_down` handler and carrying `keys` is sent only the keys it names,
# and -- the half that matters more -- only those are withheld from the
# client's own meaning (03 §3.1). The arrows, PageUp, PageDown, Home and
# End are absent on purpose: they stay the client's, and go on scrolling.
# `ArrowUp` and `ArrowDown` are in here, and that is a decision with a
# cost worth naming. The scrolling keys belong to the client only while
# nothing that wants them has focus (03 §3), so claiming the two arrows
# takes them away from the scroller: they move between messages now, and
# the page is scrolled with PageUp, PageDown, Home, End and the wheel,
# which are all still the client's. Worth it, because moving between
# messages is what someone reaching for an arrow in a mail client means.
MAIL_KEYS = [
  "j", "k", "ArrowDown", "ArrowUp", "PageDown", "PageUp",
  "g", "G", "o", "r", "R", "q", "u", "c", "i", "p", "t", "d", "h", "n", "a", "A", "f", "F", "M",
  "@", "+", "1", "2", "3", "4", "5", "6", "7", "8", "9",
  "?", "/", "Enter", "Escape", "Delete", "Insert"
]

# The list claims one more key than any other screen: `Space`, which marks
# the row under the cursor.
#
# Claimed *here and nowhere else*, and that is the whole of the care this
# key needs. On a screen made of doors -- the help sheet, the folders, the
# menu -- `Space` is how the focused one is pressed (03 §3.1), and a claim
# by the root beats the focused node: a root that held `Space` everywhere
# would take it out of every button in the application to mark a row
# nobody can see. The list is the only screen with rows to mark, so the
# list is the only screen that takes it.
MAIL_PICK_KEYS = MAIL_KEYS.concat([" "])

# What a letter claims, which is the list minus the two arrows.
#
# `keys` is per node and the tree is rebuilt every render, so which keys
# an application owns can differ from screen to screen -- and here it
# should. On the list an arrow means "the next message", because that is
# the only thing there is to move between. Inside a letter it means "more
# of this letter", and a client that was not asked for them scrolls with
# them for free (03 §3). `j` and `k` still move between messages on both,
# so the Vim hands lose nothing.
MAIL_READ_KEYS = [
  "j", "k",
  "g", "G", "o", "r", "R", "q", "u", "c", "i", "p", "v", "t", "d", "h", "n", "a", "A", "f",
  "@", "+", "1", "2", "3", "4", "5", "6", "7", "8", "9",
  "?", "/", "Enter", "Escape", "Delete", "Insert"
]

# What the sign-in screen claims, and why it is not the list above.
#
# That screen used to claim nothing at all, on the grounds that a claim on
# `Enter` withholds the `submit` from the field under the caret (03 §3.1)
# and `Enter` is how you sign in. True, but it threw out the help with it:
# `?` did nothing on the one screen where someone is most likely to press
# it. Claiming the two help keys and nothing else keeps `Enter` the
# field's, and a printable character is never withheld in any case -- text
# reaches an editable node as input with no key name on it -- so typing a
# `?` into the password still types a `?`.
MAIL_HELP_KEYS = ["?"]

# The same, plus the way out. Adding an account is the sign-in screen
# with somewhere to go back to, and "Cancel esc" has to be true: `Escape`
# is consumed by the client unless something claims it (06 §3), so the
# label would have been a promise nothing kept.
MAIL_ADD_KEYS = ["?", "Escape"]

# In order, so the nth is account n. `index_of` on a list of one-character
# strings rather than arithmetic on a key name: there is no `to_i` worth
# trusting on "@" and this cannot be off by one.
MAIL_DIGITS = ["1", "2", "3", "4", "5", "6", "7", "8", "9"]

# What the picture viewer claims: the ways to walk a set of photographs,
# and every way out of it. The arrows are in the list because inside a
# letter they are the client's to scroll with -- while a picture is up
# they are the viewer's to page with.
MAIL_VIEW_KEYS = [
  "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "PageUp", "PageDown",
  "j", "k", "o", "u", "q", "Enter", "Escape", "?"
]

# What the search bar claims: the ways to walk what it is showing, and the
# two ways out of it. Not a letter of the alphabet -- those are the
# query's -- and not the horizontal arrows, which are the caret's.
MAIL_FIND_KEYS = ["Escape", "Enter", "ArrowUp", "ArrowDown", "PageUp", "PageDown"]

# What a draft claims, and why it is one key.
#
# A claim by an ancestor beats the field under the caret, and the root is
# every field's ancestor. `s` for send would be taken out of every word
# typed into the letter; `Enter` is the textarea's new line. `Escape` is
# neither -- nobody types it -- so it is the only one a draft can hold.
MAIL_WRITE_KEYS = ["Escape"]

def mail_view(raw_state)
  state = raw_state ?? {}
  lay = mail_layout(state)
  linked = state["linked"] ?? false
  busy = state["busy"] ?? ""
  finding = (state["finding"] ?? false) == true
  errands = mail_errands(state)
  # The clock's period for this frame: never sooner than the last tick
  # took (`mail_pace`).
  pace = mail_pace(state)
  note = state["note"] ?? ""
  writing = state["writing"]
  # A draft outranks every other screen: it is the only one holding
  # something that is not written down anywhere else.
  return mail_frame(mail_composer(lay, state), MAIL_WRITE_KEYS, "", false, errands, note, false, nil, pace) if writing.nil? != true
  return mail_frame(mail_sheet(lay, linked), MAIL_KEYS, "", true, errands, note, linked, nil, pace) if (state["sheet"] ?? false) == true
  return mail_frame(mail_accounts_sheet(lay, state), MAIL_KEYS, busy, true, errands, note, linked, nil, pace) if (state["sheet_accounts"] ?? false) == true
  return mail_frame(mail_folders_sheet(lay, state), MAIL_KEYS, busy, true, errands, note, linked, nil, pace) if (state["sheet_folders"] ?? false) == true
  return mail_frame(mail_menu_sheet(lay, state), MAIL_KEYS, busy, true, errands, note, linked, nil, pace) if (state["sheet_menu"] ?? false) == true
  # Fetching is not a screen. The list is the screen, and the spinner
  # lives in the masthead beside the count -- so a resume shows the mail
  # you already have while the rest arrives behind it, and even the very
  # first sign-in shows the application rather than a held frame.
  # The sign-in screen leaves focus to its own first field, too.
  adding = (state["adding"] ?? false) == true
  return mail_frame(mail_login(lay, state), adding ? MAIL_ADD_KEYS : MAIL_HELP_KEYS, "", false, errands, note, false, nil, pace) if linked != true && busy == ""
  if (state["reading"] ?? false) == true
    watching = (state["viewing"] ?? nil).nil? != true
    # `o` put the caret on the `Browser` link, and a focused node is
    # activated with `Enter` or `Space` -- except that an ancestor's claim
    # beats the focused node (03 §3.1), and this root claims `Enter` for
    # "open the message". So `o` then `Enter` did nothing at all and `o`
    # then `Space` worked, which is a distinction nobody should have to
    # learn. For the one frame the caret is on the link, the root lets
    # `Enter` go.
    browsing = (state["browse"] ?? false) == true
    reads = browsing ? MAIL_READ_KEYS.filter(fn(k) { k != "Enter" }) : MAIL_READ_KEYS
    return mail_frame(
      mail_reader(lay, state),
      watching ? MAIL_VIEW_KEYS : reads,
      busy, watching != true && (state["copying"] ?? false) != true,
      errands, note, linked,
      watching ? mail_viewer(lay, state) : nil,
      pace
    )
  end

  return mail_frame(mail_list(lay, state), MAIL_FIND_KEYS, busy, false, errands, note, linked, nil, pace) if finding

  # While Tab has left the caret on a folder, `Enter` is that folder's.
  # The root takes it back the moment the caret moves on, because the
  # focus event that put it there is the same one that moves it away.
  on_rail = (state["rail_focus"] ?? "") != ""
  # While the caret is on a folder both of the keys that press a focused
  # node go back to the client: `Enter` for the reason above, and `Space`
  # because a rail row is a door and that is how a door is pressed.
  keys = on_rail ? MAIL_KEYS.filter(fn(k) { k != "Enter" }) : MAIL_PICK_KEYS
  mail_frame(mail_list(lay, state), keys, busy, on_rail != true, errands, note, linked, nil, pace)
end

# Everything the tick still owes: the letter someone is looking at, a
# read mark, the pictures being fetched, and a draft waiting to go out.
# The clock runs while this is more than nothing (see `mail_frame`).
def mail_errands(state)
  out = (state["want_body"] ?? 0) + (state["want_seen"] ?? 0) + (state["shooting"] ?? []).length()
  out = out + (state["burning"] ?? []).length()
  out = out + 1 if mail_wants_folders?(state)
  out = out + 1 if mail_wants_counts?(state)
  out = out + 1 if (state["sending"] ?? "") != ""
  return out + 1 if (state["clipping"] ?? nil).nil? != true

  out
end

# The root. It holds the keyboard for the whole application and takes
# focus on arrival, which is what makes a key work before anything has
# been clicked. Which keys depends on the screen -- the sign-in screen
# claims only `MAIL_HELP_KEYS`, so `Enter` stays the field's.
def mail_frame(body, keys, busy, hold_focus, errands, note = "", poll = false, viewer = nil, pace = MAIL_TICK)
  # A stack rather than a column, so a toast has a layer to float in.
  # With no toast up it is a stack of one, which lays out exactly as the
  # column did.
  kids = [body]
  kids = kids.concat([viewer]) if viewer.nil? != true
  kids = kids.concat([mail_toast(note)]) if note != ""
  shell = {
    "k": "box",
    "s": {"display": "stack", "width": "100%", "height": "100%", "bg": "surface.base"},
    "c": kids
  }
  on = {}
  props = {}
  wants_keys = keys.length() > 0
  if wants_keys
    on["key_down"] = "key"
    props["keys"] = keys
    # `autofocus` is not "focus me", it is "focus the **first** laid-out
    # node carrying this" (03 §3.1) -- and the root is always first. So
    # while the search field wants the caret, the root must not ask for
    # it, or the field never gets a single character and the bar looks
    # broken. Everywhere else the root holds focus, which is what makes a
    # key work before anything has been clicked.
    props["autofocus"] = true if hold_focus == true
  end
  # `wake` is the only event nobody caused (06 §1.1). It is sent every
  # period for exactly as long as the node carries both the prop and the
  # handler, so clearing `busy` stops the clock: nothing has to be
  # cancelled, because nothing was scheduled anywhere but here.
  # A clock runs for loud work and for quiet errands alike; only the
  # first of them puts a spinner anywhere.
  # Two speeds, and the slow one is the point.
  #
  # `wake` is sent for as long as the node carries both the prop and the
  # handler, so the fast tick lives exactly as long as there is work: 250
  # ms while a batch is being walked, gone the moment `busy` clears. What
  # is left when nothing is happening is the poll -- the same handler, a
  # minute and a half apart -- which is how an open window finds out that
  # something arrived.
  ticking = busy != "" || errands > 0
  if ticking
    on["wake"] = "pull"
    props["wake"] = pace
  elsif poll == true
    on["wake"] = "pull"
    props["wake"] = MAIL_POLL * 1000
  end
  ticking = ticking || poll == true
  shell["on"] = on if wants_keys || ticking
  shell["p"] = props if wants_keys || ticking
  shell
end

# The spinner. Three sides of a rounded square, turning. `animation` is a
# bit set and `spin` is the first bit (02 §3): the node's painting turns
# about its own centre, and the client owns the clock for it -- it costs
# no round trip and no wake.
# What just happened, in the corner, for as long as it takes to read.
#
# It used to live in the masthead, where it pushed the count and the
# actions along and stayed until the next thing you did. A toast is the
# shape this wants: over the page rather than in it, gone on its own.
#
# `wake` is the only clock in the protocol (06 §1.1) -- the node asks to
# be woken and the client obliges. A note that waited to be clicked would
# still be there tomorrow; clicking it early is the shortcut, not the
# mechanism.
def mail_toast(note)
  {
    "k": "overlay",
    "s": {"position": "absolute", "align": "end", "justify": "end", "pad": 6, "width": "100%"},
    "c": [{
      "k": "box",
      "s": {
        "display": "row", "align": "center", "gap": 3, "shrink": 0,
        "pad": [3, 4, 3, 4], "radius": 3, "cursor": "pointer",
        "bg": "surface.overlay", "border": 1, "border_color": "border.subtle",
        "shadow": 2, "animation": "enter", "motion": "bottom"
      },
      "p": {"wake": 3600, "role": "status", "label": note},
      "on": {"click": "note_done", "wake": "note_done"},
      "c": [text(mail_cap(note, 90), {"size": 1, "fg": "text.default", "shrink": 1})]
    }]
  }
end

# A round one: an arc on a canvas, turning.
#
# It was a box with three of its four borders and `radius: 4`, which is
# the right idea -- a ring with a gap in it -- and at thirteen pixels
# read as a square bracket rather than a circle, because the corners a
# border rounds are not an arc. `canvas` has an arc among its five path
# kinds (03 §1.1), which is the shape this actually wants, and `spin` is
# what a spinner is made of: one revolution every 1.2 s, and the client
# wakes for those frames only while it is on screen (03 §1).
#
# Three quarters of a circle, from -90° to 180°, so the gap says which
# way it is turning.
def mail_spinner(px, tone = "accent.base")
  r = (px - 3) / 2
  {
    "k": "canvas",
    "s": {"width": px, "height": px, "shrink": 0, "animation": "spin"},
    "p": {"paths": [[4, tone, 2, px / 2, px / 2, r, -1.57, 3.14]]}
  }
end

# -- signing in --------------------------------------------------------------
#
# Centred, four elements, and a sentence that says what the field wants
# instead of a tooltip that says it is required. Tab and Enter are all the
# keyboard this screen needs, and both are the client's already.
def mail_login(lay, state)
  field = lay["measure"] > 420 ? 420 : lay["measure"]
  kind = state["kind"] ?? "gmail"
  adding = (state["adding"] ?? false) == true
  lines = [
    mail_field("Your address", state["address"] ?? "", "address", field, false, true),
    mail_field(kind == "gmail" ? "App password" : "Password", state["secret"] ?? "", "secret", field, true, false)
  ]
  # The two lines a Gmail account never has to see. A server that is not
  # Gmail needs a host, and one that is not on 993 needs a port; the
  # sending host is guessed from the reading one (`imap.` -> `smtp.`) and
  # is here for the servers that do not follow the convention.
  if kind != "gmail"
    lines = lines.concat([
      mail_field("IMAP server", state["host"] ?? "", "host", field, false, false),
      row({"gap": 4, "width": field}, [
        mail_write_field("Port", state["port"] ?? "993", "port", (field - 4) / 2, false),
        mail_write_field("SMTP server", state["smtp"] ?? "", "smtp_host", (field - 4) / 2, false)
      ])
    ])
  end
  says = kind == "gmail" ? "Gmail will not take your account password over IMAP. Turn on 2-step verification, generate a sixteen-character app password at myaccount.google.com/apppasswords, and paste it above." : "Whatever your provider calls the incoming server — imap.fastmail.com, imap.mail.me.com, mail.yourdomain.tld. It is dialled over TLS on the port given, 993 unless you say otherwise."
  {"k": "scroll", "s": {"grow": 1, "width": "100%"}, "c": [
    column({
      "width": "100%", "align": "center", "justify": "center",
      "height": lay["h"] > 640 ? lay["h"] : 640,
      "pad": [8, lay["pad"], 8, lay["pad"]]
    }, [
      column({"width": field, "gap": 7}, [
        column({"gap": 3, "width": field}, [
          text(adding ? "Another account" : "Mail", {
            "size": 7, "weight": "bold", "fg": "accent.base", "width": field
          }),
          text(adding ? "It joins the ones you already have." : "Your mail, without the web page.", {
            "size": 3, "fg": "text.muted", "width": field
          })
        ]),
        row({"gap": 3, "align": "center", "width": field}, [
          mail_chip("Gmail", kind == "gmail", "kind_gmail"),
          mail_chip("Another IMAP server", kind != "gmail", "kind_imap")
        ]),
        column({"gap": 5, "width": field}, lines),
        text(says, {"size": 1, "fg": "text.muted", "width": field}),
        text(mail_sealed?() ? "It is kept in this application's own config folder, encrypted with SOLI_ENCRYPTION_KEY. Signing out deletes it." : "It is kept in this application's own config folder, in the clear — set SOLI_ENCRYPTION_KEY to have it sealed instead. Signing out deletes it.", {
          "size": 0, "fg": "text.muted", "width": field
        }),
        row({"gap": 4, "align": "center", "width": field}, [
          mail_submit(adding ? "Add it" : "Read my mail", "link"),
          adding ? mail_action("Cancel", "esc", "add_cancel") : text("", {"size": 0})
        ]),
        mail_trouble(state["error"] ?? "", field)
      ])
    ])
  ]}
end

# One of two, and which one it is is the whole of what it says.
def mail_chip(label, here, event)
  {
    "k": "box",
    "s": {
      "display": "row", "align": "center", "shrink": 0, "cursor": "pointer",
      "pad": [2, 4, 2, 4], "radius": 5, "transition": "fast",
      "bg": here == true ? "accent.base" : "surface.sunken"
    },
    "on": {"click": event},
    "p": {"role": "button", "label": label},
    "c": [text(label, {
      "size": 1, "weight": "semibold",
      "fg": here == true ? "text.inverted" : "text.muted"
    })]
  }
end

def mail_trouble(error, width)
  return column({"gap": 0, "width": width}, []) if error == ""

  column({
    "width": width, "pad": [3, 4, 3, 4], "radius": 2, "gap": 0,
    "bg": "danger.subtle"
  }, [text(error, {"size": 1, "fg": "danger.base", "width": width - 32})])
end

# -- the list ----------------------------------------------------------------
#
# No rules between rows, no grid, no columns of metadata. A sender, a
# subject, a date, and enough air that the eye finds the next sender
# without a line to help it.
# The list, windowed.
#
# A `list` carrying `count` and `item_height` has rows the tree does not
# hold (04 §7.1): the client lays out the whole extent from the count,
# paints placeholders where rows are missing, and asks -- through the
# `window` event -- for the range it actually needs. So a hundred
# messages cost the tree the dozen on screen rather than the hundred.
#
# Measured on a mailbox of a hundred, with a real clock: the view was
# 22 ms an event, which is 22 ms of the session's frame lock for every
# keypress and every tick, and it is 2 ms windowed. The number that
# matters is not the render, it is that the render is *per event*.
def mail_list(lay, state)
  msgs = mail_shown(state)
  m = lay["measure"]
  n = msgs.length()
  at = clamp(state["cursor"] ?? 0, 0, n)
  window = state["window"] ?? [0, MAIL_WINDOW]
  first = clamp(window[0] ?? 0, 0, n)
  last = clamp(window[1] ?? MAIL_WINDOW, first, n - 1)
  # Two bands, not one span.
  #
  # The row under the cursor is always drawn, whatever the client last
  # asked for: `G` on a long list scrolls somewhere the window has not
  # reached yet, and a placeholder under the cursor for the hundred
  # milliseconds that takes is a flicker with no cause. Widening the
  # window *to* the cursor is what that must not mean -- `G` on seven
  # thousand messages then drew all seven thousand, and the client laid
  # out sixty-two thousand nodes for them. A list's children each name
  # their own row (04 §7.1), so they need not be contiguous: the window
  # the client asked for, plus a couple of rows around the cursor.
  wanted = range(first, last + 1)
  if at < first || at > last
    # Deduplicated, and that is not a nicety: two children of one `list`
    # may not carry the same key -- the client refuses the frame, the
    # resync that follows finds the session poisoned, and the window dies.
    # The cursor's band is clamped into the list, so it can reach back
    # into the window it was meant to be outside of: a window of [0, 24]
    # with the cursor at 26 gives a band of [24, 28], and 24 is in both.
    seen = {}
    for i in wanted
      seen[str(i)] = true
    end
    for i in range(clamp(at - 2, 0, n - 1), clamp(at + 3, 0, n))
      unless seen[str(i)] == true
        seen[str(i)] = true
        wanted.push(i)
      end
    end
  end
  # Which of them are on their way out, so the row can say so in the
  # frame the key was pressed in -- the mailbox is told on the next tick.
  burning = {}
  for u in (state["burning"] ?? [])
    burning[str(u)] = true
  end
  # And which of them are marked. `picking` is a mode rather than a count:
  # the gutter is on every row or on none of them, so the rows do not
  # change width one at a time as marks come and go.
  marks = {}
  for u in (state["picked"] ?? [])
    marks[str(u)] = true
  end
  picking = (state["picked"] ?? []).length() > 0
  rows = wanted.map(fn(i) {
    at_uid = str((msgs[i] ?? {})["uid"] ?? 0)
    mail_row_slot(msgs[i], i, m, i == at, burning[at_uid] == true, picking, marks[at_uid] == true)
  })
  # The folders down the left, and the mailbox across the rest.
  #
  # A row rather than a column, and the masthead moves inside the right
  # half of it: the rail runs the full height of the window, which is what
  # makes it read as *where you are* rather than as a thing on top of the
  # list.
  body = column({"grow": 1, "height": "100%"}, [
    mail_masthead(lay, state),
    # Outside the list, because a `list` lays out only the children that
    # carry a `row` (04 §7.1) and an error is not a row -- so it gets the
    # centring the rows get from their slots.
    row({"width": "100%", "justify": "center", "shrink": 0}, [
      mail_trouble(state["error"] ?? "", m)
    ]),
    n == 0 ? mail_empty_page(state, lay, m) : {
      "k": "list",
      "s": {"grow": 1, "width": "100%"},
      "p": {
        "count": n, "item_height": MAIL_ROW_H,
        "scroll_to": [0, state["scroll"] ?? 0]
      },
      # `scroll` is where the view actually is, rather than where this
      # application last asked it to be -- a wheel is the client's own
      # (03 §3) and never asked anyone, so believing the last `scroll_to`
      # left `j` comparing the cursor against a window it was not in.
      #
      # `window` is the other half: the rows the client wants. It arrives
      # when the range changes and the scroll has settled, coalesced --
      # not once a frame, which would be a render a frame.
      "on": {"scroll": "scrolled", "window": "windowed"},
      "c": rows
    }
  ])
  return body unless mail_rail?(lay, state)

  row({"width": "100%", "height": "100%"}, [mail_rail(lay, state), body])
end

# Whether there is room for the rail, and anything to put in it.
#
# A narrow window keeps the list: a folder list that takes a third of a
# phone is a folder list nobody reads a message next to. `@` still reaches
# the accounts and `g`/`G` still walk the mailbox, so nothing is lost --
# only shown differently, which is what a breakpoint is for.
def mail_rail?(lay, state)
  return false if lay["roomy"] != true
  return false if (state["linked"] ?? false) != true

  (state["folders"] ?? []).length() > 1
end

# The folders, down the left.
def mail_rail(lay, state)
  here = state["box"] ?? MAIL_BOX
  caret = state["rail_focus"] ?? ""
  to = (state["rail_to"] ?? false) == true
  counts = state["counts"] ?? {}
  rows = (state["folders"] ?? []).map(fn(raw) {
    one = set_key(raw, "counts", counts[raw["name"] ?? ""] ?? {})
    mail_rail_row(
      one, (one["name"] ?? "") == here, (one["name"] ?? "") == caret, to,
      (one["name"] ?? "") == MAIL_BOX ? (state["inbox_new"] ?? 0) : 0
    )
  })
  # A `scroll`, because a Gmail account has as many folders as it has
  # labels and a column that overflowed would simply lose the last of
  # them. (`overflow: auto` is a CSS habit and not a value this protocol
  # has: 02 §3 gives `visible`, `clip` and `scroll`, and a scroller is a
  # node rather than a style.)
  {
    "k": "box",
    "s": {
      "display": "column", "width": MAIL_RAIL_W, "height": "100%", "shrink": 0,
      "overflow": "clip", "bg": "surface.sunken"
    },
    "c": [column({"width": "100%", "gap": 1, "pad": [2, 2, 4, 3]}, [
      text("Folders", {
        "size": 0, "font": "mono", "weight": "semibold",
        "fg": "text.muted", "pad": [2, 2, 2, 2]
      })
    ].concat(rows))]
  }
end

# One folder. The whole row is the target, and the one you are in carries
# the accent rather than a mark: there is only ever one, and a list where
# every row looks pressable and one looks *pressed* says it without a
# legend.
# The number beside a folder, and which number it is.
#
# `fresh` is what the poll found while you were elsewhere and outranks
# everything: it is news. Then the unread count, then the total -- and
# nothing at all for a folder the server has not been asked about yet, so
# the rail fills in over the ten seconds that takes rather than showing a
# column of zeroes that are not true.
def mail_rail_count(one, here, fresh)
  said = one["counts"] ?? {}
  unseen = said["unseen"] ?? 0
  total = said["messages"] ?? 0
  n = fresh > 0 ? fresh : (unseen > 0 ? unseen : total)
  return text("", {"size": 0}) if n < 1

  loud = fresh > 0 || unseen > 0
  text(str(n), {
    "font": "mono", "size": 0, "shrink": 0,
    "weight": loud ? "semibold" : "regular",
    "fg": here == true ? "text.inverted" : (loud ? "accent.base" : "text.muted")
  })
end

def mail_rail_row(one, here, caret = false, to = false, fresh = 0)
  name = mail_folder_name(one)
  # No `key`, for the same reason the message rows carry none: a keyed
  # node in this Soli is diffed differently, and the first version of this
  # rail was drawn correctly and answered no press at all.
  {
    "k": "box",
    "s": {
      "display": "row", "align": "center", "gap": 2, "width": "100%",
      "pad": [2, 3, 2, 3], "radius": 2, "cursor": "pointer",
      "bg": here == true ? "accent.base" : "none",
      # The one the caret is on, outlined. A row that is *selected* is
      # filled; a row that is merely where Tab has got to is outlined, and
      # the two are different things that Tab makes you tell apart.
      "border": caret == true ? 2 : 0,
      "border_color": "focus.ring"
    },
    # `focus` as well as `click`, and it is not decoration: Tab walks these
    # rows, and the root claims `Enter` for "open the message under the
    # cursor" -- an ancestor's claim beats the focused node (03 §3.1), so
    # `Enter` on a folder did nothing at all. Telling the server which
    # folder has the caret is what lets the frame let `Enter` go while one
    # of them does.
    "on": {"click": "box_pick", "focus": "box_focus"},
    # `focus_to` only for the frame a key moved the caret here: it is an
    # op, and a node that carried it every frame would hold the caret for
    # ever. Tab needs none of this -- the client moved the focus itself
    # and told us with `focus`.
    "p": caret == true && to == true
      ? {"box": one["name"] ?? "", "role": "button", "label": name, "focus_to": true}
      : {"box": one["name"] ?? "", "role": "button", "label": name},
    "c": [
      text(mail_fit(name, MAIL_RAIL_W - 44, 6), {
        "size": 1,
        "weight": here == true ? "semibold" : "regular",
        "fg": here == true ? "text.inverted" : "text.default",
        "grow": 1, "shrink": 1
      }),
      # What is in it: the unread count when there is one, and the
      # total when there is not. Unread is the number somebody scanning a
      # folder list is looking for; the total is what is worth knowing
      # about a folder that has none.
      mail_rail_count(one, here, fresh)
    ]
  }
end

# One row in its slot. The slot is the list's child and carries the `row`
# prop that says which row it is; the card inside it is what was always
# there, centred on the measure.
def mail_row_slot(one, i, measure, here, going, picking = false, marked = false)
  # The slot is the node that leaves the tree, so the slot is where the
  # exit has to be declared -- the row inside it goes with its parent and
  # is never removed on its own. It is declared only while the row is on
  # its way out: in a windowed list every scroll drops rows that were not
  # deleted at all, and an exit on all of them would animate the
  # scrolling.
  style = {
    "display": "row", "justify": "center", "align": "center",
    "width": "100%", "height": MAIL_ROW_H
  }
  style = style.merge({"animation": ["exit"], "motion": "scale", "transition": "fast"}) if going == true
  {
    "k": "box",
    "key": "m:" + str((one ?? {})["uid"] ?? i),
    "s": style,
    "p": {"row": i},
    "c": [mail_row(one ?? {}, measure, here, going, picking, marked)]
  }
end

def mail_empty_page(state, lay, m)
  {"k": "scroll", "s": {"grow": 1, "width": "100%"}, "c": [
    column({"width": "100%", "align": "center", "pad": [1, lay["pad"], 10, lay["pad"]]}, [
      column({"width": m, "gap": 4}, [mail_empty(state, m)])
    ])
  ]}
end

# An empty list means one of two quite different things, and saying the
# wrong one is how this read as broken: during the first fetch there is
# nothing on screen *yet*, and "Nothing in INBOX." claims the mailbox is
# empty when in fact 10 431 messages are sitting in it and five of them
# are seconds away. The spinner in the masthead is already saying the
# true thing; this says it in words.
def mail_empty(state, m)
  # The folder you are actually in, not the one this application used to
  # assume was the only one there was.
  said = (state["busy"] ?? "") != "" ? "Fetching your mail…" : "Nothing in " + mail_folder_name({"name": mail_here(state)}) + "."
  text(said, {"size": 2, "fg": "text.muted", "width": m})
end

# One message, as two lines and a date, in a box of exactly `MAIL_ROW_H`.
#
# Read and unread are told apart by weight and colour rather than by a dot
# or a bold everything: an unread sender is bold and at full contrast, a
# read one is regular and muted, and the subject sits a step down from
# either.
#
# The cursor is a bar in the left border and a raised ground. The border
# is on every row, always three pixels wide, and only its colour changes
# -- a border that appeared would move the text under it by three pixels,
# and the scroll arithmetic above believes these rows are all one height.
#
# These rows carry no `key`, which for a list is the unusual choice, and
# it is not a style preference. A keyed node in this Soli does not have
# its style diffed: in one batch, on these very rows, a changed `t` came
# through as `SetText` and a changed `s` produced no `SetStyle` at all, so
# the cursor drew on whichever row it had first landed on and stayed
# there for ever. Unkeyed, the same view is correct. What a key buys is
# `MoveChild` on a reorder (02 §5), and this list never reorders in place
# -- a refresh rebuilds it -- so the trade costs nothing here. Restore the
# key when the diff is fixed; twenty rows do not need it before then.
def mail_row(one, measure, here, going, picking = false, marked = false)
  seen = one["seen"] == true
  inner = measure - 44
  # The gutter takes its width out of the text and not out of the card:
  # every row is exactly `measure` wide whether anything is marked or not,
  # which is what the centring and the scroll arithmetic both assume.
  inner = inner - MAIL_PICK_W - 12 if picking == true
  rest = inner - 110
  rest = 80 if rest < 80
  # A message on its way to the trash says so in the one colour that
  # means it, and wears the line through it that means the same thing
  # everywhere else. It is still a row, still under the cursor if it was,
  # and still there to look at -- it has not gone yet, and if Gmail
  # refuses the move it is not going anywhere.
  tone = going == true ? "danger.base" : (seen ? "text.muted" : "text.default")
  {
    "k": "box",
    # Keyed on the slot above rather than here: see `mail_row_slot`.
    # Kept for the diff's sake, so a card and its slot agree.
    #
    # Unkeyed, the diff matches children by position: deleting the fourth
    # row rewrites the text of every row below it and drops the last
    # child. Nothing is wrong on screen afterwards -- but the node that
    # leaves is the last one, so an exit animation plays on the wrong row,
    # at the bottom, while the row you deleted silently becomes its
    # neighbour. A key is what makes the removal mean what it looks like.
    "key": "m:" + str(one["uid"]),
    "s": going == true ? mail_row_going(measure) : mail_row_style(measure),
    # No hover ground.
    #
    # A row used to light under the pointer, which is the web habit and
    # wrong here: the cursor is the selection, and a second highlight that
    # follows the mouse says "this one" about a row that is not the one
    # any key will act on. The pointer still opens a row; it just does not
    # claim to have chosen it.
    "on": {"click": "open", "focus": "spot"},
    "p": {"uid": one["uid"], "role": "button", "label": one["subject"]},
    "c": [mail_cursor(here)].concat(picking == true ? [mail_pick_box(one, marked)] : []).concat([
      column({"gap": 1, "grow": 1, "shrink": 1}, [
        row({"gap": 4, "align": "baseline", "width": inner}, [
          text(mail_fit(one["name"], rest, 8), {
            "size": 2, "grow": 1, "shrink": 1,
            "weight": seen || going == true ? "regular" : "bold",
            "fg": tone
          }),
          mail_row_marks(one, going),
          # The one genuinely moving thing on the row, and it costs
          # nothing: `animation: spin` turns the node about its own
          # centre on the client's own clock (02 §3) -- no wake, no round
          # trip, sixty frames a second while the mailbox is being told.
          going == true ? row({"gap": 2, "align": "center", "shrink": 0}, [
            mail_spinner(11, "danger.base"),
            text("deleting", {"font": "mono", "size": 0, "fg": "danger.base", "shrink": 0})
          ]) : text(one["date"], {"font": "mono", "size": 0, "fg": "text.muted", "shrink": 0})
        ]),
        text(mail_fit(one["subject"], inner, 8), {
          "size": 2, "width": inner,
          "weight": seen || going == true ? "regular" : "semibold",
          "strike": going == true,
          "fg": tone
        })
      ])
    ])
  }
end

# The box a marked row carries, and the one thing on a row that answers
# the pointer with something other than "open this".
#
# Marked and unmarked differ by a **child** rather than by a colour, for
# the same reason the cursor below is a node: a node whose only change
# between two renders is its style is sent no `SetStyle`, so a tick that
# arrives and leaves is the only mark that can be relied on to be where
# it says it is.
#
# `role` and `checked` are the other half of the same statement, said to
# an assistive technology rather than to the eye (03 §6). They cost two
# props and they are the difference between "a box" and "a checkbox that
# is ticked".
def mail_pick_box(one, marked)
  tick = marked == true ? [{
    "k": "box",
    "s": {"width": 10, "height": 10, "radius": 1, "bg": "accent.base"}
  }] : []
  {
    "k": "box",
    "s": {
      "display": "row", "justify": "center", "align": "center",
      "width": MAIL_PICK_W, "height": MAIL_PICK_W, "shrink": 0,
      "radius": 1, "border": 2, "border_color": "border.strong",
      "cursor": "pointer"
    },
    # Its own `click`, so the press that marks a row is not the press that
    # opens it: the nearest handler on the path is the one that is spent,
    # and this box is nearer than the row it sits in.
    "on": {"click": "pick"},
    "p": {
      "uid": one["uid"], "role": "checkbox",
      "checked": marked == true, "label": one["subject"]
    },
    "c": tick
  }
end

# The cursor, and the reason it is a node rather than a colour.
#
# The obvious way to mark the selected row is to give every row the same
# border and change only its colour, which costs no geometry and no node.
# It does not work here: a node whose **only** change between two renders
# is its style receives no `SetStyle` -- the row keeps the style it was
# mounted with, so the cursor draws on whichever row it first landed on
# and stays there for ever. Change that same node's text or its children
# in the same render and the style comes through with it. Measured, not
# guessed: the view was printing the right row the whole time.
#
# So the cursor is a child that arrives and leaves. `InsertChild` and
# `RemoveChild` (02 §5) are structural and always land, and the bar's own
# style never changes, so nothing here rides the broken path. The gutter
# box is on every row at the same width, so no row shifts when the cursor
# enters it.
#
# Put the border colour back when a style-only change is delivered again.
# The two marks a row can carry: paper clips, and how big it is.
#
# Muted and to the left of the date, so the eye that is scanning senders
# and subjects is not asked to read them -- they are there to be noticed
# when looked for.
def mail_row_marks(one, going)
  return text("", {"size": 0}) if going == true

  clips = one["clips"] ?? 0
  size = one["bytes"] ?? 0
  said = ""
  said = "PJ" + (clips > 1 ? str(clips) : "") if clips > 0
  said = said + "  " + mail_weight(size) if size > 0 && said != ""
  said = mail_weight(size) if size > 0 && said == ""
  return text("", {"size": 0}) if said == ""

  text(said, {"font": "mono", "size": 0, "fg": "text.muted", "shrink": 0})
end

# A size a person reads, not a number of bytes.
def mail_weight(n)
  return str(n / 1048576) + " MB" if n >= 1048576
  return str(n / 1024) + " KB" if n >= 1024

  str(n) + " B"
end

def mail_cursor(here)
  bar = here == true ? [{
    "k": "box",
    "s": {"width": 3, "height": 46, "radius": 4, "bg": "accent.base"}
  }] : []
  {"k": "box", "s": {"width": 3, "height": 46, "shrink": 0}, "c": bar}
end

def mail_row_style(measure)
  {
    "display": "row", "gap": 4, "align": "center",
    "width": measure, "height": MAIL_ROW_H,
    "pad": [5, 4, 5, 4], "radius": 3, "cursor": "pointer",
    "overflow": "clip", "bg": "none",
    # No `exit` here any more, and the reason is the windowing.
    #
    # A deleted row used to shrink and fade on its way out -- `animation:
    # exit` with `motion: scale`, because a row lives in a scroller and a
    # node sliding out sideways is clipped the moment it moves. That was
    # right while the tree held every row: leaving the tree meant being
    # deleted.
    #
    # In a windowed `list` (04 §7.1) rows leave the tree constantly: every
    # scroll drops the ones that went out of view. An exit animation would
    # play on all of them, three hundred milliseconds each, for rows that
    # were not deleted at all. What a delete looks like now is the row
    # gone, the list closed up and the toast in the corner saying where it
    # went -- which is the part that was actually informative.
    "transition": "fast"
  }
end

# Hover is the client's alone: a local handler swaps the record it already
# holds, in the frame the pointer moved, and the server is never told.
# A row on its way to the trash: still there, visibly leaving.
#
# Not `opacity`. That is the node's *own* painting (02 §3) and one of
# these boxes paints nothing -- no ground, no border -- so dimming it
# dims nothing and every child goes on at full strength. A sunken ground
# is a thing the box actually draws, and the text goes muted beside it.
# A row on its way out: an ember ground, and a leaving you can see.
#
# There is no fire to be had here and it is worth saying why: a style
# record has a colour, a radius and a motion, and nothing in the client
# draws particles. What there is: `danger.subtle` under it, the line
# through the subject, `deleting` where the date was, and -- when the
# mailbox agrees and the row is finally taken out of the tree -- an
# `exit` that slides it out trailing and fades it (03 §5). The client
# keeps painting the node while it leaves, which is what makes the
# removal something that happens rather than something that has
# happened.
def mail_row_going(measure)
  mail_row_style(measure).merge({
    "bg": "danger.subtle",
    # `scale` rather than a slide: these rows live in a scroller, and a
    # node moving sideways is clipped the moment it moves.
    "animation": ["exit"], "motion": "scale",
    "transition": "fast"
  })
end

# The masthead sits on the same column the list does. A header that ran to
# the window edges over a centred list is the commonest way a page of this
# shape stops looking composed: two left margins, neither agreeing with
# the other. `mail_bar` is what keeps every screen honest about it.
def mail_masthead(lay, state)
  return mail_findbar(lay, state) if (state["finding"] ?? false) == true
  return mail_pickbar(lay, state) if (state["picked"] ?? []).length() > 0

  total = state["total"] ?? 0
  shown = mail_shown(state).length()
  busy = state["busy"] ?? ""
  count = shown < total ? str(shown) + " of " + str(total) : str(total) + (total == 1 ? " message" : " messages")
  count = shown == 0 ? "" : count
  # A search is a different question, so it gets a different answer.
  count = str(state["found"] ?? shown) + " found · Esc" if (state["mode"] ?? "") == "hits"
  # A narrow window keeps the name of the mailbox and one door.
  #
  # Seven actions across the top of a phone is seven things that are
  # nearly too small to press and one that is: the mailbox you are in.
  # So that is what stays, and everything else moves behind `Menu` --
  # which is the same screen the keyboard reaches with `?`, and which a
  # thumb can actually hit.
  unless lay["roomy"]
    return mail_bar(lay, [
      row({"gap": 3, "align": "center", "grow": 1, "shrink": 1}, [
        text(mail_folder_name({"name": mail_here(state)}), {
          "size": 4, "weight": "bold", "fg": "accent.base", "shrink": 1
        }),
        busy == "" ? {"k": "box", "s": {"width": 0, "height": 0}} : mail_spinner(13),
        # How many are loaded *and* how many there are, which is the
        # number that says whether the list you are looking at is the
        # mailbox or the top of it. `11/1234` where the wide bar has room
        # for "11 of 1234"; the slash is what a narrow window can afford.
        text(shown < total ? str(shown) + "/" + str(total) : str(total), {
          "font": "mono", "size": 0, "fg": "text.muted", "shrink": 0
        })
      ]),
      # The account, not the word "Menu". A label that says which mailbox
      # you are signed into is worth the same space as a label that says
      # there is a menu -- which you can see, because it is the only thing
      # up there.
      # The account, and `@` means what it means everywhere else: the
      # accounts. A chip that said `@` and opened a menu was a small lie,
      # and the one key this application spends on accounts is not the
      # place to tell it.
      mail_action(mail_whoami(state, false), "@", "accounts"),
      # The rest of the doors behind one more, which is as much as a
      # narrow bar can hold: three dots, `M`, everything else.
      mail_action("···", "M", "menu_open")
    ])
  end
  mail_bar(lay, [
    row({"gap": 3, "align": "center", "grow": 1, "shrink": 1}, [
      text("Mail", {"size": 5, "weight": "bold", "fg": "accent.base", "shrink": 0}),
      busy == "" ? {"k": "box", "s": {"width": 0, "height": 0}} : mail_spinner(13),
      text(lay["roomy"] ? count : str(shown), {"font": "mono", "size": 0, "fg": "text.muted", "shrink": 1})
    ]),
    row({"gap": 3, "align": "center", "shrink": 0}, [
      mail_action(mail_whoami(state, lay["roomy"]), "@", "accounts"),
      # On a narrow window the rail is not there, so this is the only way
      # to the folders that does not need a keyboard -- and a phone has
      # no `F` to press.
      lay["roomy"] || (state["folders"] ?? []).length() < 2
        ? text("", {"size": 0})
        : mail_action("Folders", "F", "folders_open"),
      mail_action("Write", "n", "write_new"),
      mail_action("Search", "/", "find"),
      mail_action("Refresh", "r", "refresh"),
      lay["roomy"] ? mail_action("Keys", "?", "sheet") : mail_action("?", "", "sheet"),
      lay["roomy"] ? mail_action("Sign out", "q", "signout") : mail_action("Out", "q", "signout")
    ])
  ])
end

# What is on top while rows are marked: how many, and the two things that
# can be done with them.
#
# It replaces the masthead rather than sitting under it, for the reason
# the search bar does: a bar that arrived *above* the list would push
# every row down by its own height and take the one your eye is on with
# it -- at the exact moment you are looking straight at it.
#
# The actions are words with their keys beside them, like every other
# door in this application, so the bar teaches the keyboard rather than
# replacing it: whoever finds `Delete` with the pointer reads `d` next to
# it and does not need the bar again.
def mail_pickbar(lay, state)
  n = (state["picked"] ?? []).length()
  mail_bar(lay, [
    row({"gap": 3, "align": "baseline", "grow": 1, "shrink": 1}, [
      text(str(n), {"font": "mono", "size": 4, "weight": "bold", "fg": "accent.base", "shrink": 0}),
      text(n == 1 ? "message marked" : "messages marked", {"size": 2, "fg": "text.default", "shrink": 0}),
      # The hint is the first thing a narrow bar gives up: it is there to
      # be read once, and the two doors beside it are not.
      lay["roomy"] ? text("space marks another", {"size": 1, "fg": "text.muted", "shrink": 1}) : text("", {"size": 0})
    ]),
    row({"gap": 3, "align": "center", "shrink": 0}, [
      # The same event the key presses, because it is the same thing:
      # `mail_delete` is what looks at the marks and decides whether it
      # is deleting one message or five.
      mail_action("Delete", "d", "trash"),
      mail_action("Clear", "esc", "pick_none")
    ])
  ])
end

# The bar. It replaces the masthead rather than sitting under it, so the
# list does not move when it opens and the row under the cursor stays
# where your eye left it.
def mail_findbar(lay, state)
  query = state["query"] ?? ""
  hits = mail_filter(state["msgs"] ?? [], query).length()
  mail_bar(lay, [
    row({
      "gap": 3, "align": "center", "grow": 1, "shrink": 1,
      "pad": [1, 3, 1, 3], "radius": 2, "bg": "surface.sunken"
    }, [
      text("/", {"font": "mono", "size": 2, "weight": "bold", "fg": "accent.base", "shrink": 0}),
      {
        "k": "input",
        "t": query,
        "s": {
          "grow": 1, "shrink": 1, "size": 2, "pad": [2, 0, 2, 0],
          "bg": "none", "fg": "text.default", "border": 0
        },
        # `focus_to` asks for the caret outright (02 §5, `Op::Focus`).
        # `autofocus` cannot do it: it is applied "only when focus is not
        # already where it belongs" (03 §3.1), and when this bar opens
        # focus is on the root that was holding the shortcuts -- so the
        # field would never get it and you could not type. Both are given:
        # `autofocus` for the case where focus is nowhere, `focus_to` for
        # the case where it is somewhere else.
        "p": {"label": "Search", "autofocus": true, "focus_to": true},
        # `change` only, and never `text_input`.
        #
        # `text_input` carries **the insertion**, not the value (06 §2) --
        # and the spec says plainly that it "exists for local handlers; a
        # server that subscribes to it across a wide-area link has misread
        # the design". Subscribing to it here did exactly the damage that
        # warns about: every keystroke arrived as a one-character payload,
        # was stored as the whole query, and was written straight back
        # into the field's text, so the field could never hold more than
        # the last character typed.
        #
        # `change` is the whole value, and it settles on blur, on Enter,
        # or after 300 ms in which nothing moved -- which is a filter that
        # follows your typing without the server hearing every letter.
        "on": {"change": "query", "submit": "find_all"}
      },
      # There is no placeholder in this protocol -- a field is its text --
      # so the hint is a node beside it, and it leaves when you type.
      query == "" ? text("filters as you pause · ↑↓ to walk · Enter opens", {
        "size": 1, "fg": "text.muted", "shrink": 1
      }) : text(str(hits) + " here", {
        "font": "mono", "size": 0, "fg": "text.muted", "shrink": 0
      })
    ]),
    # `Enter` opens the row under the cursor now, so the whole-mailbox
    # search needs a door of its own. It keeps `Enter` as its shortcut all
    # the same, because that is what it does when the filter matches
    # nothing here.
    mail_action("All mail", "⏎", "find_all"),
    mail_action("Close", "esc", "find_close")
  ])
end

# The account in the bar: the local part is enough to tell two apart at a
# glance, and the whole of it is a key press away.
def mail_whoami(state, roomy)
  address = state["address"] ?? ""
  n = (state["accounts"] ?? []).length()
  return "Accounts" if address == ""

  at = mail_at(address, "@")
  said = at < 1 ? address : address.substring(0, at)
  said = address if roomy == true
  n > 1 ? said + " +" + str(n - 1) : said
end

def mail_bar(lay, kids)
  row({
    "width": "100%", "height": MAIL_HEAD_H, "shrink": 0,
    "justify": "center", "align": "center",
    "pad": [0, lay["pad"], 0, lay["pad"]]
  }, [row({"width": lay["measure"], "gap": 4, "align": "center", "justify": "between"}, kids)])
end

# -- the viewer --------------------------------------------------------------
#
# One picture at a time, over the letter, with the others a key away.
#
# An `overlay` paints in the top layer, after everything else (03 §1), and
# a press outside it is the client's to dismiss -- so the scrim is the
# overlay's own box and nothing underneath needs to know it is there. What
# the viewer claims of the keyboard it claims for as long as it is open:
# the arrows and `j`/`k` walk the pictures, `Escape` closes, `o` hands the
# file to the browser.
#
# The picture shown is the full file rather than the thumbnail. One at a
# time is what the sheet can hold -- and when it cannot, it empties itself
# and packs this one, which is the behaviour a viewer wants anyway.
def mail_viewer(lay, state)
  mail_say("v: built t=" + str(mail_ms()))
  box = state["viewing"]
  return {"k": "box", "s": {"width": 0, "height": 0}} if box.nil?

  shots = box["shots"] ?? []
  at = clamp(box["at"] ?? 0, 0, shots.length() - 1)
  one = shots[at]
  return {"k": "box", "s": {"width": 0, "height": 0}} if one.nil?

  wide = lay["w"] - 80
  tall = lay["h"] - 160
  full = one["path"] ?? ""
  pic = mail_big_file(full, one)
  path = pic["path"] ?? full
  w = pic["w"] ?? 0
  h = pic["h"] ?? 0
  if w < 1 || h < 1
    w = wide
    h = tall
  end
  # Inside the window, and its own shape kept.
  if w > wide
    h = h * wide / w
    w = wide
  end
  if h > tall
    w = w * tall / h
    h = tall
  end
  link = MAIL_BASE_URL == "" ? "" : MAIL_BASE_URL + "/" + full.replace("public/", "")
  {
    "k": "overlay",
    "s": {
      "position": "absolute", "width": "100%", "height": "100%",
      "display": "column", "align": "center", "justify": "center",
      # No `blur` here on purpose. A frosted scrim is a full-screen blur
      # pass on every frame of the fade, which is what made the fade
      # stutter; the scrim's own colour hides the letter just as well and
      # costs one quad.
      "gap": 4, "pad": 6, "bg": "surface.overlay",
      "animation": "enter", "motion": "fade"
    },
    "on": {"click": "view_close"},
    "c": [
      {
        # A press on the picture itself walks on to the next one; a press
        # on the scrim around it closes. Dispatch is the nearest handler
        # on the path and there is no bubbling (06 §2), so the two do not
        # fight, and neither reaches the buttons below.
        "k": "box",
        "s": {"display": "stack", "width": w, "height": h, "cursor": "pointer", "radius": 3, "shadow": 3},
        "on": {"click": "view_next"},
        # The thumbnail underneath, the picture over it.
        #
        # The viewer took five seconds to open, and none of it was here:
        # the event and the frame are sixty milliseconds together. What
        # took the time was the *asset* -- the full file is one the client
        # has never seen, because the contact sheet draws thumbnails, so
        # opening asked for a fresh picture and then waited for it.
        #
        # An image whose asset has not arrived paints **nothing** at all
        # (`paint.rs`: no region, no quad), rather than a placeholder over
        # it. So the thumbnail the client already holds goes underneath at
        # the same size: the viewer opens in the frame you pressed in,
        # soft, and sharpens the moment the real file lands. Nothing
        # flashes, because there is nothing to flash -- the sharp one
        # simply starts being painted.
        "c": mail_view_layers(one, path, w, h)
      },
      # The next one, already on its way.
      #
      # A picture is fetched when a node names it and not before, so the
      # first look at each one waits for a file. A node one pixel square
      # asks for it just the same -- so while this photograph is being
      # looked at, the one after it is crossing the wire, and `Suivante`
      # costs nothing. It is the only prefetch in the application, and it
      # is bounded: one picture ahead, never a set.
      mail_view_next_up(box, at),
      row({"gap": 5, "align": "center", "justify": "center", "width": "100%"}, [
        mail_action("Précédente", "←", "view_prev"),
        text(str(at + 1) + " / " + str(shots.length()) + "  ·  " + mail_cap(one["name"] ?? "", 60), {
          "font": "mono", "size": 0, "fg": "text.muted", "shrink": 1
        }),
        mail_action("Suivante", "→", "view_next"),
        link == "" ? text("", {"size": 0}) : mail_open_link(link, one["name"] ?? ""),
        mail_action("Fermer", "esc", "view_close")
      ])
    ]
  }
end

def mail_view_next_up(box, at)
  shots = box["shots"] ?? []
  nxt = shots[at + 1]
  return {"k": "box", "s": {"width": 0, "height": 0}} if nxt.nil?

  path = nxt["view"] ?? ""
  path = nxt["path"] ?? "" if path == ""
  return {"k": "box", "s": {"width": 0, "height": 0}} if path == ""

  # Nothing wide and nothing tall: a picture is wanted because a node
  # names it, not because it is drawn, so this costs a node and no pixels.
  {"k": "image", "s": {"width": 0, "height": 0}, "p": {"src": path}}
end

def mail_view_layers(one, path, w, h)
  small = one["thumb"] ?? ""
  sharp = {
    "k": "image",
    "s": {"width": w, "height": h, "radius": 3},
    "p": {"src": path, "role": "image", "label": one["name"] ?? "Image"}
  }
  return [sharp] if small == "" || small == path

  [
    {"k": "image", "s": {"width": w, "height": h, "radius": 3}, "p": {"src": small}},
    sharp
  ]
end

# The copy made for the screen, beside the file, and its shape.
#
# A photograph off a telephone is four thousand pixels across and the
# client packs every picture into one 2048x2048 sheet: the full file
# would not fit whatever the sheet held, and a picture that does not fit
# is simply not drawn. So the viewer shows a copy no bigger than a screen
# ever is -- made once, kept beside the original, and read from disk on
# every later look. `Ouvrir dans le navigateur` still hands over the
# original: that is what the link is for.
def mail_big_file(path, one = {})
  return {"path": "", "w": 0, "h": 0} if path == ""

  # Written down when the file was, so a frame costs no decode: stepping
  # through seventeen photographs used to re-read one from disk each
  # time, inside the frame lock.
  kept = one["vw"] ?? 0
  return {"path": one["view"] ?? path, "w": kept, "h": one["vh"] ?? 0} if kept > 0

  big = path + ".view.jpg"
  ready = Image.new(big) rescue nil
  return {"path": big, "w": ready.width() rescue 0, "h": ready.height() rescue 0} unless ready.nil?

  img = Image.new(path) rescue nil
  return {"path": path, "w": 0, "h": 0} if img.nil?

  w = img.width() rescue 0
  h = img.height() rescue 0
  return {"path": path, "w": w, "h": h} if w <= MAIL_VIEW_PX && h <= MAIL_VIEW_PX

  made = img.thumbnail(MAIL_VIEW_PX).to_file(big) rescue nil
  return {"path": path, "w": w, "h": h} if made.nil?

  fit = Image.new(big) rescue nil
  return {"path": path, "w": w, "h": h} if fit.nil?

  {"path": big, "w": fit.width() rescue 0, "h": fit.height() rescue 0}
end

# The letter, in the browser: written out on the spot and handed over.
#
# The page is written here, in the view, which is the one place that knows
# the letter is on screen -- it is a few kilobytes and it costs a file
# write per frame of the reader. Cheap, and always right: a message whose
# face changed with `h`, or whose letter has just landed, writes the page
# it is showing now.
def mail_browser(one, caret)
  path = MAIL_PAGE_DIR == "" ? "" : mail_page_file(one)
  link = path == "" || MAIL_BASE_URL == "" ? "" : MAIL_BASE_URL + "/" + path.replace("public/", "")
  return text("", {"size": 0}) if link == ""

  {
    "k": "box",
    "s": {
      "display": "row", "align": "center", "gap": 2, "shrink": 0,
      "pad": [1, 2, 1, 2], "radius": 2, "cursor": "pointer"
    },
    # `focus_to` only while `o` has just been pressed: it is an op, and a
    # node that carried it every frame would hold the caret for ever.
    "p": caret == true
      ? {"open": link, "role": "link", "label": "Open in browser", "focus_to": true}
      : {"open": link, "role": "link", "label": "Open in browser"},
    "c": [
      text("Browser", {"size": 1, "weight": "semibold", "fg": "accent.base", "shrink": 0}),
      text("o ⏎", {"font": "mono", "size": 0, "fg": "text.muted", "shrink": 0})
    ]
  }
end

# The one node that hands a URL to the person's own browser (08 §7).
def mail_open_link(link, name)
  {
    "k": "box",
    "s": {
      "display": "row", "align": "center", "gap": 2, "shrink": 0,
      "pad": [1, 3, 1, 3], "radius": 2, "cursor": "pointer",
      "bg": "accent.base"
    },
    "p": {"open": link, "role": "button", "label": name},
    "c": [text("Ouvrir dans le navigateur", {
      "size": 1, "weight": "semibold", "fg": "text.inverted"
    })]
  }
end

# `Enter` on a thumbnail, or a press: the pictures of this message, and
# which one was asked for.
def mail_view_open(state, at)
  # Timed because a viewer that took six seconds to open was measured
  # everywhere but here. Silent unless `MAIL_DEBUG=1`.
  mail_say("v: open at=" + str(at) + " t=" + str(mail_ms()))
  msgs = mail_shown(state)
  cur = clamp(state["cursor"] ?? 0, 0, msgs.length() - 1)
  one = msgs[cur]
  return state if one.nil?

  # The same rows `mail_clips` draws, filtered the same way -- so the
  # number the thumbnail sent is an index into this list.
  saved = (state["clipped"] ?? {})[str(one["uid"] ?? 0)] ?? []
  rows = saved.length() > 0 ? saved : (one["atts"] ?? [])
  shots = rows.filter(fn(a) { (a["path"] ?? "") != "" && mail_image_type?(a["type"] ?? "") })
  # Nothing to show is not nothing to say: a letter whose pictures have
  # not been fetched yet looks exactly like a letter with none, and `v`
  # answering with a blank screen would teach nobody the difference.
  if shots.length() == 0
    waiting = (one["atts"] ?? []).length() > 0
    return set_key(state, "note", waiting ? "Appuyez sur p pour télécharger les pièces jointes." : "Ce message n'a pas d'image.")
  end

  set_key(state, "viewing", mail_view_ready({"shots": shots, "at": clamp(at, 0, shots.length() - 1)}))
end

def mail_view_step(state, by)
  mail_say("v: step " + str(by) + " t=" + str(mail_ms()))
  box = state["viewing"]
  return state if box.nil?

  shots = box["shots"] ?? []
  return state if shots.length() == 0

  at = clamp((box["at"] ?? 0) + by, 0, shots.length() - 1)
  set_key(state, "viewing", mail_view_ready(set_key(box, "at", at)))
end

# The picture about to be shown, measured once.
#
# Anything fetched by a build that writes its shapes down arrives with
# them and this does nothing. Anything older is read from disk here --
# in the event, which happens once, rather than in `mail_viewer`, which
# happens on every frame the viewer is up.
def mail_view_ready(box)
  shots = box["shots"] ?? []
  at = clamp(box["at"] ?? 0, 0, shots.length() - 1)
  one = shots[at]
  return box if one.nil?
  return box if (one["vw"] ?? 0) > 0

  img = Image.new(one["path"] ?? "") rescue nil
  return box if img.nil?

  big = mail_view_file(one["path"] ?? "", img, one["size"] ?? 0)
  fixed = set_key(set_key(one, "view", big["path"] ?? ""), "vw", big["w"] ?? 0)
  set_key(box, "shots", mail_replace_at(shots, at, set_key(fixed, "vh", big["h"] ?? 0)))
end

def mail_replace_at(xs, at, one)
  i = -1
  xs.map(fn(x) {
    i = i + 1
    i == at ? one : x
  })
end

# -- the composer ------------------------------------------------------------
#
# Three fields and a letter, on a screen that claims one key.
#
# It claims `Escape` and nothing else on purpose. A claim by an ancestor
# beats the field under the caret, so a root that claimed `s` for send
# would eat the `s` out of every word you typed -- and `Enter` belongs to
# the textarea, where it is a new line. So Send is a button, and the way
# to it from the keyboard is Tab: the field order is To, Cc, Subject,
# the letter, Send.
def mail_composer(lay, state)
  draft = state["writing"] ?? {}
  seed = draft["seed"] ?? {}
  m = lay["measure"] > 720 ? 720 : lay["measure"]
  kind = draft["kind"] ?? "new"
  title = "New message"
  title = "Answer" if kind == "reply"
  title = "Answer all" if kind == "all"
  title = "Forward" if kind == "forward"
  sending = state["sending"] ?? ""
  column({"width": "100%", "height": "100%"}, [
    mail_bar(lay, [
      row({"gap": 3, "align": "center", "grow": 1, "shrink": 1}, [
        text(title, {"size": 5, "weight": "bold", "fg": "accent.base", "shrink": 0}),
        sending == "" ? {"k": "box", "s": {"width": 0, "height": 0}} : mail_spinner(13),
        text(sending, {"size": 0, "fg": "text.muted", "shrink": 1})
      ]),
      row({"gap": 3, "align": "center", "shrink": 0}, [
        text(state["address"] ?? "", {"font": "mono", "size": 0, "fg": "text.muted", "shrink": 1}),
        mail_action("Discard", "esc", "write_close")
      ])
    ]),
    mail_write_bar(lay, draft, m),
    {
      "k": "scroll",
      # Keyed on nothing but the composer: opening a second draft builds a
      # fresh subtree, so its fields arrive with the new seed rather than
      # being patched under a caret that was in the old one.
      "key": "write:" + kind + ":" + str((seed["subject"] ?? "").length()),
      "s": {"grow": 1, "width": "100%"},
      # `modal` keeps Tab inside the draft (03 §3.1), so the last field
      # leads to Send rather than out through the masthead.
      "p": {"scroll_to": [0, 0], "modal": true},
      "c": [column({"width": "100%", "align": "center", "pad": [2, lay["pad"], 10, lay["pad"]]}, [
        column({"width": m, "gap": 5}, [
          mail_write_field("To", seed["to"] ?? "", "w_to", m, kind != "reply" && kind != "all"),
          mail_write_field("Cc", seed["cc"] ?? "", "w_cc", m, false),
          mail_write_field("Subject", seed["subject"] ?? "", "w_subject", m, false),
          mail_write_body(draft, m),
          row({"gap": 4, "align": "center", "width": m}, [
            mail_submit(sending == "" ? "Send" : "Sending…", "send"),
            text("Tab reaches Send · Escape discards", {
              "size": 1, "fg": "text.muted", "shrink": 1
            })
          ]),
          mail_trouble(state["error"] ?? "", m)
        ])
      ])]
    }
  ])
end

# Like `mail_field`, without its `submit`: Enter in a header field of a
# draft must not send the draft.
def mail_write_field(label, value, event, width, first)
  props = {"label": label}
  props["autofocus"] = true if first == true
  props["focus_to"] = true if first == true
  column({"gap": 2, "width": width}, [
    text(label, {"size": 0, "weight": "semibold", "fg": "text.muted", "width": width}),
    {
      "k": "input",
      "t": value,
      "s": {
        "width": width, "pad": [3, 1, 3, 1], "size": 2,
        "bg": "none", "fg": "text.default",
        "border": [0, 0, 2, 0], "border_color": "border.default"
      },
      "p": props,
      "on": {"change": event}
    }
  ])
end

# The letter.
#
# `markdown_editor` (03 §4, the catalogue's markdown family), because what
# is typed here is markdown -- that is the format of the composer, not a
# mode it can be in -- and a document being written should look like the
# document it will be. A heading is a heading while it is being typed; a
# picture is the picture, not its address.
#
# What it is not is a WYSIWYG. A `text` node carries one weight for its
# whole run (02 §3) and the client reports no caret (08 §7.1), so `**bold**`
# stays written out in the block being edited and becomes bold in the
# preview and in what goes out. The widget's own header has the rest.
#
# Pictures and files do not go to the asset store here, which is the
# widget's default and is wrong for a letter: an `eui-asset:` address means
# something to a client talking to this server and to nothing else, and a
# mail leaves. They are written under `public/mail-att` instead --
# `mail_write_attach` -- exactly where a received attachment lands, and
# `mail_send_source` turns them into real parts on the way out.
# The letter's toolbar, and why it is here rather than inside the editor.
#
# A bar inside the scroller goes up with the document and nothing can hold
# it there: sticky positioning is not in version 1 (04 §9), and no scroll
# offset reaches the server to move one with (06 §8). What does hold it is
# being *outside* the scroller, in the column above it -- where this
# application's own masthead already is. So the editor is asked for its bar
# separately and it is put there.
def mail_write_bar(lay, draft, width)
  row({
    "width": "100%", "shrink": 0,
    "justify": "center",
    "pad": [0, lay["pad"], 1, lay["pad"]]
  }, [
    {
      "k": "box",
      "s": {"display": "column", "width": width},
      "c": [markdown_editor_bar(draft["doc"] ?? {}, mail_write_opts())]
    }
  ])
end

# One options hash, read by the bar above and by the document below: the two
# halves of one widget must not disagree about what its buttons are called.
def mail_write_opts()
  md_edit_events("w_body").merge({
    "key": "write",
    "accept": MAIL_WRITE_ACCEPT,
    "max": MAIL_ATT_MAX
  })
end

def mail_write_body(draft, width)
  doc = draft["doc"] ?? {"blocks": md_edit_parse("")}
  column({"gap": 2, "width": width}, [
    text("Message", {"size": 0, "weight": "semibold", "fg": "text.muted", "width": width}),
    markdown_editor(doc, mail_write_opts().merge({
      "width": width,
      "bar": false,
      "placeholder": "Markdown. \"## \" makes a heading, \"- \" a list; Enter starts a block."
    }))
  ])
end

# What the composer's picker takes. Narrower than what a mail may carry:
# these are the formats a client can draw (03 §1) plus the documents
# people attach, and a picker that offered everything would offer things
# this application has nothing to show for.
MAIL_WRITE_ACCEPT = "png,jpg,jpeg,webp,gif,pdf,txt,md,csv,zip,odt,docx,xlsx"

# -- one message -------------------------------------------------------------
#
# The letter takes the window. The subject is the headline, the sender is
# the byline, and then there is nothing between the reader and the text --
# which, for now, is the text exactly as it came off the wire.
#
# The scroller is keyed on the message, so moving to the next letter with
# j builds a fresh node rather than reusing the old one, and its
# `scroll_to` lands on a subtree the client has not seen: the new letter
# opens at its top instead of wherever the last one was left.
def mail_reader(lay, state)
  msgs = mail_shown(state)
  at = clamp(state["cursor"] ?? 0, 0, msgs.length() - 1)
  one = msgs[at]
  return mail_list(lay, state) if one.nil?

  m = lay["measure"]
  busy = state["busy"] ?? ""
  # `d` leaves the letter for the list, where the row is watched, so this
  # is normally false -- it is here for the frame in between.
  going = mail_burning?(state, one["uid"] ?? 0)
  # How many pictures this letter has that have not been fetched. `i` is
  # opt-in on purpose -- every one of them is a request to a server the
  # sender chose -- but opt-in with nothing on screen to opt into is just
  # a feature nobody finds.
  # Parsed once, used twice. The reader needs the blocks for the letter
  # and the count of pictures needs them too -- and parsing an 86 KB
  # marketing mail measured 150 ms, so doing it twice an event was 300 ms
  # of frame lock for a number in the corner.
  face = state["face"] ?? ""
  blocks = mail_blocks_of(one, state["raw"] ?? false, face)
  other = mail_other_face(one)
  shots = mail_unshot(blocks, state["shots"] ?? {})
  byline = one["address"] == "" ? one["name"] : one["name"] + "  ·  " + one["address"]
  column({"width": "100%", "height": "100%"}, [
    mail_bar(lay, [
      row({"gap": 3, "align": "center", "grow": 1, "shrink": 1}, [
        mail_action("Back", "u", "back"),
        # The masthead has one and the reader did not, so `d` inside a
        # letter looked like nothing happening at all for the second the
        # move takes.
        busy == "" && going != true ? {"k": "box", "s": {"width": 0, "height": 0}} : mail_spinner(13),
        text(going == true ? "Moving to the trash…" : busy, {
          "size": 0,
          "fg": going == true ? "danger.base" : "text.muted", "shrink": 1
        })
      ]),
      row({"gap": 3, "align": "center", "shrink": 0}, [
        text(str(at + 1) + " of " + str(msgs.length()), {
          "font": "mono", "size": 0, "fg": "text.muted", "shrink": 0
        }),
        shots == 0 ? text("", {"size": 0}) : mail_action(str(shots) + (shots == 1 ? " picture" : " pictures"), "i", "shoot"),
        other == "" ? text("", {"size": 0}) : mail_action(face == "html" ? "Source" : "HTML", "h", "face"),
        mail_action("Answer", "a", "write_reply"),
        lay["roomy"] ? mail_action("Forward", "f", "write_forward") : text("", {"size": 0}),
        lay["roomy"] ? mail_action("Trash", "d", "trash") : text("", {"size": 0}),
        mail_action((state["copying"] ?? false) == true ? "Reading" : "Select", "c", "select"),
        mail_action((state["raw"] ?? false) == true ? "Links" : "Plain", "t", "plain"),
        # The message as its sender built it, in the person's own browser.
        # A node rather than a key, because `net.open` is spent on an
        # activation and there is no op for it; `o` puts the caret here
        # and `Enter` does the rest.
        mail_browser(one, (state["browse"] ?? false) == true),
        mail_action("Keys", "?", "sheet")
      ])
    ]),
    {
      "k": "scroll",
      "key": "read:" + str(one["uid"]),
      "s": {"grow": 1, "width": "100%"},
      "p": {"scroll_to": [0, 0]},
      "c": [column({"width": "100%", "align": "center", "pad": [2, lay["pad"], 11, lay["pad"]]}, [
        column({"width": m, "gap": 6}, [
          column({"gap": 3, "width": m}, [
            text(one["subject"], {
              "size": lay["roomy"] ? 5 : 4, "weight": "bold",
              "fg": going == true ? "danger.base" : "text.default",
              "strike": going == true, "width": m
            }),
            row({"gap": 4, "align": "baseline", "width": m}, [
              text(byline, {"size": 1, "fg": "text.muted", "grow": 1, "shrink": 1}),
              text(one["date"], {"font": "mono", "size": 0, "fg": "text.muted", "shrink": 0})
            ])
          ]),
          # Raw text, in the face notation belongs in. A mail body is
          # wrapped by whoever sent it, so it is set in mono and left to
          # its own line breaks rather than reflowed into a paragraph.
          mail_clips(
            one, (state["clipped"] ?? {})[str(one["uid"] ?? 0)] ?? [], m,
            mail_clip_job(state, one["uid"] ?? 0),
            (state["viewing"] ?? nil).nil? != true
          ),
          (one["loaded"] ?? false) != true ? mail_waiting_letter(m) : mail_letter(
            m, one["body"], blocks,
            state["copying"] ?? false, state["shots"] ?? {},
            state["raw"] ?? false, state["shooting"] ?? []
          )
        ])
      ])]
    }
  ])
end

# The body, in nodes you can select.
#
# There is no server-driven copy to be had: `clipboard.write` is a
# capability the handshake negotiates and no op ever spends, so nothing a
# view returns can put text on the clipboard. What *does* work is the
# person's own act on an editable node -- in a `textarea` the client owns
# the caret and the selection, and `Ctrl+A` / `Ctrl+C` are its keys, not
# this application's (03 §3). So the letter is set in textareas.
#
# They are styled flat -- no border, no ground, the same mono as before --
# so the page reads as a letter and not as a form. Nothing is done with
# what comes back: no `change` handler, no state, so an accidental keypress
# is undone by the next render. It is a reading surface that happens to be
# selectable, which is the closest this protocol comes to copyable text.
#
# The height has to be given, because a node with no explicit height is
# measured against a loosened constraint and a wrapped body would be laid
# out one line tall. Mono makes that measurable rather than a guess: every
# glyph is the same width, so the wrapped line count is arithmetic.
MAIL_MONO_ADV = 9
MAIL_MONO_LINE = 22

# The letter as blocks, parsed once per render.
# Which face of a message to draw: markdown, then text, then HTML.
#
# The order is the application's preference, not the sender's. A mail
# that carries its markdown source is shown as that source rendered --
# through `Markdown.to_html`, because that is the direction that exists
# and the block parser is already there. Otherwise the text part, which
# is what a person wrote rather than what their mail client built around
# it. HTML is the last resort, and `h` reaches it whenever it is there:
# it is the face with the pictures in it, so a newsletter is one key away
# from being a newsletter.
# `h`. The HTML face of a message, and back.
def mail_face(state)
  set_key(state, "face", (state["face"] ?? "") == "html" ? "" : "html")
end

def mail_blocks_of(one, raw, face)
  return [] if (one["loaded"] ?? false) != true

  html = one["html"] ?? ""
  return mail_html_blocks(html) if face == "html" && html != ""

  md = one["md"] ?? ""
  return mail_html_blocks(Markdown.to_html(md)) if md != ""

  body = one["body"] ?? ""
  return mail_text_blocks(body) if body.trim() != ""
  return [] if raw == true

  mail_html_blocks(html)
end

# Is there another face to switch to?
def mail_other_face(one)
  html = one["html"] ?? ""
  return "" if html == ""
  return "html" if (one["md"] ?? "") != ""
  return "html" if (one["body"] ?? "").trim() != ""

  ""
end

# Pictures in this letter that are not here yet.
def mail_unshot(blocks, shots)
  n = 0
  for b in blocks
    n = n + 1 if b["kind"] == "image" && mail_worth_fetching?(b, shots)
  end
  n
end

# The attachments, named and sized, above the letter.
#
# Above rather than below, because a letter can be long and a paper clip
# you have to scroll to find is a paper clip you do not know about.
# What is attached: listed always, shown once `p` has fetched it.
#
# Listed from the headers the letter came with, so a paper clip is
# visible before anything is downloaded; shown from `clipped`, which is
# what `p` wrote to disk. An image becomes an image; everything else
# keeps its name and its size, and carries a link when this application
# knows its own address.
# Where the download says where it is: a spinner while the message is
# being read -- there is no count to give yet -- and then "3 / 17" as the
# files land, because by then there is.
# The download in flight, if it is this message's.
def mail_clip_job(state, uid)
  job = state["clipping"]
  return nil if job.nil?
  return nil if (job["uid"] ?? 0) != uid

  job
end

def mail_clip_progress(job, m)
  return text("", {"size": 0, "grow": 1}) if job.nil?

  total = job["total"] ?? 0
  done = (job["done"] ?? []).length()
  says = total == 0 ? "lecture du message…" : str(done) + " / " + str(total)
  row({"gap": 3, "align": "center", "grow": 1, "shrink": 1}, [
    mail_spinner(12),
    text(says, {"font": "mono", "size": 0, "fg": "text.muted", "shrink": 1})
  ])
end

def mail_clips(one, saved, m, job = nil, covered = false)
  atts = one["atts"] ?? []
  return {"k": "box", "s": {"width": 0, "height": 0}} if atts.length() == 0

  rows = atts
  rows = saved if saved.length() > 0
  # Pictures go together in a wrapping row of thumbnails; everything else
  # keeps a line of its own.
  #
  # Each one used to be drawn at the measure's full width, which for the
  # seventeen photographs of a club outing is seventeen screenfuls: they
  # were all there and only the first could be seen without scrolling
  # past the others. The body's pictures have been capped at
  # `MAIL_IMG_TALL` since the beginning; these are capped smaller,
  # because a set of them is a contact sheet and not an album.
  shown = rows.filter(fn(a) { (a["path"] ?? "") != "" && mail_image_type?(a["type"] ?? "") })
  rest = rows.filter(fn(a) { (a["path"] ?? "") == "" || mail_image_type?(a["type"] ?? "") != true })
  chips = rest.map(fn(a) { mail_clip(a, m) })
  chips = [mail_clip_sheet(shown, m, covered)].concat(chips) if shown.length() > 0
  head = row({"width": m, "gap": 3, "align": "center"}, [
    text(str(atts.length()) + (atts.length() == 1 ? " pièce jointe" : " pièces jointes"), {
      "size": 1, "weight": "semibold", "fg": "text.muted", "shrink": 0
    }),
    mail_clip_progress(job, m),
    saved.length() > 0 || job.nil? != true ? text("", {"size": 0}) : mail_action("Télécharger", "p", "clips")
  ])
  column({"width": m, "gap": 2, "pad": [0, 0, 3, 0]}, [head].concat(chips))
end

def mail_clip(one, m)
  path = one["path"] ?? ""
  kind = one["type"] ?? ""
  # A picture that has come down is a picture, not a line about one.
  return mail_clip_image(one, path, m) if path != "" && mail_image_type?(kind)

  mail_clip_row(one, path, m)
end

# How tall a thumbnail may be. Four to a row at the usual measure.
MAIL_THUMB = 150

# And how large the file behind it may be, in pixels on its longest edge.
#
# The client packs every picture it draws into **one 2048x2048 texture**,
# on shelves as tall as the tallest picture in them (`ImageAtlas`). A
# photograph of 800x600 therefore takes a 600-tall shelf and two fit
# across, so six of them fill the atlas -- and the seventh onwards are
# simply never drawn. Seventeen photographs from a club outing showed
# six, and which six looked random.
#
# So what is *shown* is a thumbnail written beside the file. Its size is
# arithmetic, not taste, and the arithmetic is the atlas's -- **including
# the two pixels of padding it puts round every picture**, which is where
# the first version of this number went wrong:
#
#   400 square -> 402 packed -> 5 to a shelf (2048 / 402 = 5)
#   17 of them -> 4 shelves -> 1608 px of a 2048 px sheet. They fit.
#
# 512 looks like it divides 2048 four times and does not: 514 goes into
# 2048 three times, so seventeen took six shelves of 386 -- 2316 -- and
# the sheet emptied and repacked itself with a third of the pictures
# missing. Which is exactly the fault this thumbnail exists to cure, made
# by the cure.
#
# A square photograph is the worst case and this holds for it. Being
# resident is the point: the picture the viewer shows first is the one the
# contact sheet already put in the sheet, so opening downloads nothing.
MAIL_THUMB_PX = 400

# And what the viewer shows: one picture, at the size the client would
# keep anyway.
#
# 1024 is not a taste: it is `assets::ATLAS_EDGE` in the client, which
# shrinks every picture whose longest edge is over it *before* packing --
# so a pixel beyond this one is a pixel downloaded, decoded and then
# thrown away. Under it, the original is sent as it is.
MAIL_VIEW_PX = 1024

# And the weight above which a copy is worth making even when the picture
# is already small enough to keep whole. A press on a thumbnail asks the
# client for a file it has never seen -- that is what a viewer showing
# something sharper than the contact sheet *means* -- and the difference
# between waiting a moment and waiting five seconds is how many bytes
# that is. A quarter of a megabyte is about where a photograph stops
# arriving inside one frame.
MAIL_VIEW_BYTES = 262144

def mail_clip_sheet(shown, m, covered = false)
  # Numbered by their place in the sheet, because that is the number the
  # viewer is opened on: `mail_view_open` filters the same list the same
  # way, so position `i` here is picture `i` there -- no identifier has to
  # survive the round trip.
  thumbs = range(0, shown.length()).map(fn(i) { mail_thumb(shown[i], m, i, covered) })
  # `align: start`, or a wrapped line stretches its shorter thumbnails to
  # the tallest in that line and the sheet goes ragged.
  {
    "k": "box",
    "s": {"display": "row", "wrap": "wrap", "align": "start", "gap": 2, "width": m},
    "c": thumbs
  }
end

# A smaller copy beside the file, for the atlas's sake. Returns its path,
# or "" when the picture could not be read -- in which case the full one
# is shown and the sheet holds fewer of them.
def mail_thumb_file(path, weight = 0)
  img = Image.new(path) rescue nil
  return {} if img.nil?

  # JPEG, like the viewer's copy and for the same reason: a 400x300
  # thumbnail of a photograph is 255 KB as PNG and about 30 as JPEG. The
  # contact sheet is seventeen of these, and every one of them is a TLS
  # connection of its own against a server with one worker -- four
  # megabytes of lossless photograph is the difference between a sheet
  # that fills at once and a queue the next thing you press waits behind.
  small = path + ".thumb.jpg"
  made = img.thumbnail(MAIL_THUMB_PX).to_file(small) rescue nil
  return {} if made.nil?

  fit = Image.new(small) rescue nil
  return {"thumb": small} if fit.nil?

  # And the copy the viewer shows, made in the same breath -- this is the
  # one tick in the whole application where an image is already open and
  # there is a progress line on screen saying so.
  big = mail_view_file(path, img, weight)
  {
    "thumb": small, "tw": fit.width() rescue 0, "th": fit.height() rescue 0,
    "view": big["path"] ?? path, "vw": big["w"] ?? 0, "vh": big["h"] ?? 0
  }
end

# The copy the viewer shows, written beside the file. Bigger than a
# thumbnail and smaller than a telephone's idea of a photograph, because
# the client packs what it draws into one 2048x2048 sheet.
def mail_view_file(path, img, weight = 0)
  w = img.width() rescue 0
  h = img.height() rescue 0
  # Small enough to keep whole, in both senses: no more pixels than the
  # client will keep, and few enough bytes to cross in a moment.
  fits = w <= MAIL_VIEW_PX && h <= MAIL_VIEW_PX
  return {"path": path, "w": w, "h": h} if fits && weight <= MAIL_VIEW_BYTES

  # JPEG, not PNG. These are photographs: the same picture is a tenth of
  # the bytes, and what crosses the wire is the whole of what the first
  # press waits for. Measured on a 4032x3024 photograph: 200 ms to decode,
  # 80 to shrink, 30 to write -- against seconds of it arriving whole.
  big = path + ".view.jpg"
  made = img.thumbnail(MAIL_VIEW_PX).to_file(big) rescue nil
  return {"path": path, "w": w, "h": h} if made.nil?

  fit = Image.new(big) rescue nil
  return {"path": path, "w": w, "h": h} if fit.nil?

  {"path": big, "w": fit.width() rescue 0, "h": fit.height() rescue 0}
end

# One thumbnail, its own shape kept, and a press opens the viewer.
# `covered` is the viewer being open over this sheet, and it takes the
# pictures out of the tree while it is.
#
# Not to save a frame -- the sheet is behind a full-screen overlay and is
# not drawn either way. It is the atlas: seventeen 512-pixel thumbnails
# fill 1920 of its 2048 rows, so the viewer's own sharp picture cannot be
# packed beside them and the sheet empties and repacks itself on every
# step. Unnamed, the seventeen are simply not packed while the viewer
# holds the screen, and they go back when it closes -- from the bytes the
# client already has, not from the server (`driver.rs: repack_images`).
# The box keeps the thumbnail's exact size, so nothing moves underneath.
def mail_thumb(one, m, at = 0, covered = false)
  path = one["path"] ?? ""
  # The small copy is what is drawn; the full one is what opens.
  shown = one["thumb"] ?? ""
  shown = path if shown == ""
  # Remembered when the file was written, if it was written by a build
  # that remembers; read from the file otherwise. Seventeen thumbnails
  # meant seventeen decodes on every single frame, inside the lock.
  w = one["tw"] ?? 0
  h = one["th"] ?? 0
  if w < 1 || h < 1
    img = Image.new(shown) rescue nil
    w = img.nil? ? 0 : (img.width() rescue 0)
    h = img.nil? ? 0 : (img.height() rescue 0)
  end
  if w < 1 || h < 1
    w = 200
    h = MAIL_THUMB
  end
  if h > MAIL_THUMB
    w = w * MAIL_THUMB / h
    h = MAIL_THUMB
  end
  # And a panorama is no wider than a portrait is tall, so a line of them
  # holds the same number whatever shape they are.
  if w > MAIL_THUMB * 2
    h = h * MAIL_THUMB * 2 / w
    w = MAIL_THUMB * 2
  end
  w = m if w > m
  # Not `node`: a bare assignment to that name rebinds the global
  # function every builder in this file calls, and the next `row(...)`
  # dies with "cannot call non-function value" a long way from here.
  pic = covered == true ? {
    "k": "box",
    "s": {"width": w, "height": h, "radius": 2, "bg": "surface.sunken"}
  } : {
    "k": "image",
    "s": {"width": w, "height": h, "radius": 2},
    "p": {"src": shown, "role": "image", "label": one["name"] ?? "Image"}
  }
  # A press opens the viewer rather than the browser: `open` is still a
  # press away, from inside it, where the picture is big enough to decide
  # whether you want the file.
  {
    "k": "box",
    "s": {"display": "row", "cursor": "pointer", "radius": 2},
    "on": {"click": "show"},
    "p": {"at": at, "role": "button", "label": one["name"] ?? ""},
    "c": [pic]
  }
end

def mail_clip_image(one, path, m)
  img = Image.new(path) rescue nil
  w = img.nil? ? 0 : (img.width() rescue 0)
  h = img.nil? ? 0 : (img.height() rescue 0)
  wide = w > 0 ? w : m
  tall = h > 0 ? h : 200
  # The measure is the ceiling, and the shape is the picture's own.
  if wide > m
    tall = tall * m / wide
    wide = m
  end
  column({"width": m, "gap": 1}, [
    {"k": "image", "s": {"width": wide, "height": tall, "radius": 2}, "p": {"src": path}},
    mail_clip_row(one, path, m)
  ])
end

def mail_clip_row(one, path, m)
  size = one["size"] ?? 0
  link = path == "" || MAIL_BASE_URL == "" ? "" : MAIL_BASE_URL + "/" + path.replace("public/", "")
  kids = [
    text("PJ", {"font": "mono", "size": 0, "weight": "bold", "fg": "text.muted", "shrink": 0}),
    text(mail_fit(one["name"] ?? "", m - 260, 8), {"size": 1, "fg": "text.default", "grow": 1, "shrink": 1}),
    text(mail_weight(size), {"font": "mono", "size": 0, "fg": "text.muted", "shrink": 0})
  ]
  return row({
    "width": m, "gap": 3, "align": "center",
    "pad": [2, 3, 2, 3], "radius": 2, "bg": "surface.sunken"
  }, kids) if link == ""

  # `open` is the one prop that hands a URL to the person's own browser,
  # and it is spent only on a press (08 §7). The file is this
  # application's own, under `public/`.
  {
    "k": "box",
    "s": {
      "display": "row", "width": m, "gap": 3, "align": "center",
      "cursor": "pointer", "pad": [2, 3, 2, 3], "radius": 2,
      "bg": "surface.sunken"
    },
    "p": {"open": link, "role": "button", "label": one["name"] ?? ""},
    "c": kids.concat([
      text("ouvrir", {"size": 0, "weight": "semibold", "fg": "accent.base", "shrink": 0})
    ])
  }
end

# What stands in for the letter until it has come down.
def mail_waiting_letter(m)
  row({"gap": 3, "align": "center", "width": m}, [
    mail_spinner(13),
    text("Fetching the letter…", {"size": 1, "fg": "text.muted", "shrink": 1})
  ])
end

def mail_letter(m, body, blocks, copying, shots, raw, waiting = [])
  cols = (m - 4) / MAIL_MONO_ADV
  cols = 20 if cols < 20
  # Raw is not just "the text part instead of the HTML one" -- it is the
  # letter in as few nodes as it will go.
  #
  # A selection cannot cross a node: the client owns the caret inside one
  # editable node and knows nothing of its neighbours (03 §3). So every
  # link the rich view lifts out is also a place a drag stops, and a mail
  # with four links cannot be selected in one go however careful you are.
  # There is no protocol answer to that -- one selection means one node,
  # and one node means no links inside it. So it is a mode instead: `t`
  # gives you the whole letter, split only where the 4096-byte ceiling
  # forces it, which for most mail is not at all.
  return mail_plain(m, body, cols, copying) if raw == true

  return mail_plain(m, body, cols, copying) if blocks.length() == 0

  kids = range(0, blocks.length()).map(fn(i) {
    mail_body_node(blocks[i], m, cols, shots, copying, copying == true && i == 0, waiting)
  })
  column({"width": m, "gap": 2}, kids)
end

def mail_plain(m, body, cols, copying)
  parts = mail_chunks(mail_tighten(mail_entities(body)))
  kids = range(0, parts.length()).map(fn(i) {
    mail_letter_part(parts[i], m, cols, copying, copying == true && i == 0)
  })
  column({"width": m, "gap": 0}, kids)
end

# Words are a `textarea`; a link and a picture are not.
#
# The words are editable nodes because that is the only way this protocol
# offers to select text: the client owns the caret and the selection
# inside an editable node, and `Ctrl+A` / `Ctrl+C` are its keys (03 §3).
# Nothing is done with what comes back -- no handler, no state -- so a
# stray keypress is undone by the next render. The cost is real and worth
# naming: while the caret is in the letter the letter has the keyboard,
# so `j` types a `j`. `Escape` still leaves, because the root claimed it
# and a claim on `Escape` leaves focus alone (03 §3.1).
def mail_body_node(b, m, cols, shots, live, take_focus, waiting = [])
  kind = b["kind"]
  return mail_link_node(b, m) if kind == "link"
  return mail_image_node(b, m, shots, waiting) if kind == "image"

  # What the block was in its source, when it was anything but a
  # paragraph. Only while reading: `c` turns the letter into fields to be
  # selected from, and a field that is a heading is a form control
  # pretending to be a document.
  mark = b["mark"] ?? ""
  if live != true
    return mail_head_node(b["text"], mark, m) if MAIL_HEADS.contains(mark)
    return mail_item_node(b["text"], m, cols, live) if mark == "li"
    return mail_quote_node(b["text"], m, cols, live) if mark == "quote"
  end

  # The one place every piece of body text passes through, and therefore
  # the only honest place to enforce the 4096-byte ceiling. Splitting at
  # the source did not hold: a block can arrive over the limit from a
  # paragraph in an HTML part, from a text part whose sender never
  # wrapped it, or from a single line with no break in it at all -- and
  # each of those was a separate 400 from the server. Chunking here
  # catches all three by construction, because nothing else makes a text
  # node out of a message.
  parts = mail_chunks(b["text"])
  return mail_letter_part(b["text"], m, cols, live, take_focus) if parts.length() < 2

  kids = range(0, parts.length()).map(fn(i) {
    mail_letter_part(parts[i], m, cols, live, take_focus == true && i == 0)
  })
  column({"width": m, "gap": 0}, kids)
end

# A heading, in the face a heading wants: the letter is mono because a
# sender wrapped it and their line breaks are theirs, but a heading is
# not a line of a letter, it is a title.
def mail_head_node(said, mark, m)
  size = 3
  size = 5 if mark == "h1"
  size = 4 if mark == "h2"
  column({"width": m, "gap": 0, "pad": [3, 0, 1, 0]}, [
    text(said, {"size": size, "weight": "bold", "fg": "text.default", "width": m})
  ])
end

# A list item keeps its bullet and its hanging indent.
def mail_item_node(said, m, cols, live)
  inner = m - 18
  row({"width": m, "gap": 3, "align": "start"}, [
    text("•", {"font": "mono", "size": 1, "fg": "text.muted", "shrink": 0}),
    mail_letter_part(said, inner, cols - 2, live, false)
  ])
end

# A quote keeps the rule down its left, which is what `> ` means.
def mail_quote_node(said, m, cols, live)
  inner = m - 18
  row({"width": m, "gap": 4, "align": "stretch"}, [
    {"k": "box", "s": {"width": 2, "radius": 4, "shrink": 0, "bg": "border.default"}},
    mail_letter_part(said, inner, cols - 2, live, false)
  ])
end

# One selectable run of the letter, flat: no border, no ground, the same
# mono as the rest, so the page reads as a letter and not as a form.
# One run of the letter -- and whether it is a node you can put a caret
# in depends on the mode, which is the whole of the fix for two separate
# complaints.
#
# A `textarea` is the only way this protocol offers to select text: the
# client owns the caret and the selection inside an editable node, and
# `Ctrl+A` / `Ctrl+C` are its keys (03 §3). But the same rule says an
# editable node with focus keeps the arrows, `Home`, `End` and every
# printable character for editing, and that **no prop may take them
# away** -- so a letter that was always a textarea ate `ArrowDown`,
# `PageDown`, `j` and `u` the moment focus landed in it, and the page
# stopped scrolling.
#
# There is no setting that gives both. So reading is plain text, which is
# not focusable and therefore cannot swallow anything, and `c` turns the
# letter into fields for as long as you want to select from it. `Escape`
# comes back out.
def mail_letter_part(part, m, cols, live, take_focus)
  return text(part, {
    "font": "mono", "size": 2, "fg": "text.default", "width": m
  }) if live != true

  props = {"label": "Message body"}
  props["autofocus"] = true if take_focus == true
  {
    "k": "textarea",
    "t": part,
    "s": {
      "width": m, "height": mail_letter_height(part, cols),
      "font": "mono", "size": 2, "fg": "text.default",
      "bg": "none", "border": 0, "pad": 0
    },
    "p": props
  }
end

def mail_letter_height(part, cols)
  rows = 0
  for line in part.split("\n")
    n = mail_len(line)
    rows = rows + (n <= cols ? 1 : (n + cols - 1) / cols)
  end
  rows = 1 if rows < 1
  rows * MAIL_MONO_LINE + 4
end

# -- attachments -------------------------------------------------------------
#
# What came attached, put where it can be looked at.
#
# The bytes arrive with the message -- it was fetched whole to be read --
# but they are not kept in session state or in the store: a mailbox of a
# hundred messages with their attachments inlined would be a hundred
# megabytes of JSON rewritten on every read mark. So `p` writes them to
# `public/mail-att/` once, on demand, and what state keeps is the path.
#
# `public/` because an EUI `image` node is served from the application's
# own folder, which is how the remote pictures already work. A file that
# is not an image has no node that can show it: it gets its name, its
# size, and -- when `MAIL_BASE_URL` says where this application answers --
# a link the client can open (08 §7, `net.open`).
# Where a letter is written out as a page, for the one thing this
# application cannot draw: the message as its sender built it, in a
# browser. Beside the attachments, and public for the same reason.
MAIL_PAGE_DIR = "public/mail-page"

MAIL_ATT_DIR = "public/mail-att"

# Nothing larger is written. A mail server will carry twenty-five
# megabytes; a folder that fills up with them is a folder nobody asked
# for.
MAIL_ATT_MAX = 26214400

MAIL_BASE_URL = getenv("MAIL_BASE_URL") ?? ""

# A filename that cannot be a path. `File` is jailed to this folder
# (SEC-006) and a name that came off the wire should not depend on that.
def mail_att_name(uid, n, name)
  keep = ""
  for ch in name.downcase().chars()
    keep = keep + (MAIL_SAFE.contains(ch) ? ch : "-")
  end
  keep = "piece" if keep == ""
  # The index is in the name because two parts of one message may share
  # one: this mail carried `Terms_of_Service_fr_fr.html` twice, and
  # without the index the second overwrote the first and both rows
  # pointed at it.
  str(uid) + "-" + str(n) + "-" + keep
end

def mail_image_type?(said)
  low = said.downcase()
  low.starts_with("image/")
end

# How many attachments are written per tick. Writing is fast; what makes
# this a batch is that a tick is the only place a count can be shown.
MAIL_ATT_BATCH = 3

# `p`. Ask for what is attached -- and do none of it here.
#
# This used to read the whole message and write every file in the one
# event, which for a mail of seventeen photographs is a couple of
# megabytes over IMAP and a couple of seconds of held frame lock with
# nothing on screen to say why. The press now only says what is wanted;
# the tick does the reading, and the tick after it does the writing, a
# few at a time -- so there is a spinner, and then a count that moves.
def mail_clip_open(state)
  msgs = mail_shown(state)
  at = clamp(state["cursor"] ?? 0, 0, msgs.length() - 1)
  one = msgs[at]
  return state if one.nil?
  return state if mail_demo?()

  uid = one["uid"] ?? 0
  return state if uid < 1

  out = set_key(state, "error", "")
  # The letter's own list of what is attached travels with the job: it
  # carries the part numbers, which is what turns `p` from "the whole
  # message again" into "these parts, please".
  set_key(out, "clipping", {
    "uid": uid, "step": "read", "left": [], "done": [], "total": 0,
    "atts": one["atts"] ?? []
  })
end

# The tick's half of it.
# The rows to write, when the bytes came back by part number: what the
# letter said about each attachment, with the bytes that answered for it.
def mail_clip_rows(want, got)
  bytes = {}
  for row in got
    bytes[str(row["part"] ?? "")] = row["base64"] ?? ""
  end
  out = []
  n = 0
  for a in want
    size = a["size"] ?? 0
    said = bytes[str(a["part"] ?? "")] ?? ""
    if size > 0 && size <= MAIL_ATT_MAX && said != ""
      name = a["name"] ?? ""
      name = "piece" if name == ""
      out.push({
        "name": mail_cap(name, 120), "type": a["type"] ?? "",
        "size": size, "at": n, "base64": said
      })
    end
    n = n + 1
  end
  out
end

# And the old way: the whole message came down and was parsed, so the
# bytes are on the attachments themselves.
def mail_clip_rows_whole(full)
  out = []
  n = 0
  for a in (full["attachments"] ?? [])
    size = a["size"] ?? 0
    if size > 0 && size <= MAIL_ATT_MAX
      name = a["name"] ?? ""
      name = "piece" if name == ""
      out.push({
        "name": mail_cap(name, 120), "type": a["content_type"] ?? "",
        "size": size, "at": n, "base64": a["base64"] ?? ""
      })
    end
    n = n + 1
  end
  out
end

def mail_clip_step(state)
  job = state["clipping"]
  return state if job.nil?

  uid = job["uid"] ?? 0
  if (job["step"] ?? "") == "read"
    # What the letter's fetch already learned: the freight, named and
    # measured, with a part number each. Only the parts worth writing are
    # asked for, and only their bytes come down.
    want = job["atts"] ?? []
    numbers = want.filter(fn(a) { (a["part"] ?? "") != "" }).map(fn(a) { a["part"] })
    full = mail_pull_parts(state["address"] ?? "", state["secret"] ?? "", uid, numbers, mail_here(state))
    if full.nil?
      out = set_key(state, "clipping", nil)
      return set_key(out, "error", "Those attachments would not come down.")
    end

    # By number when we asked by number, and the old way -- the whole
    # message, parsed -- when we could not.
    left = numbers.length() > 0 ? mail_clip_rows(want, full) : mail_clip_rows_whole(full)
    moved = set_key(set_key(job, "left", left), "step", "write")
    return set_key(state, "clipping", set_key(moved, "total", left.length()))
  end

  # Writing, a few at a time.
  left = job["left"] ?? []
  done = job["done"] ?? []
  n = 0
  while n < MAIL_ATT_BATCH && left.length() > 0
    a = left[0]
    left = left.slice(1, left.length())
    path = MAIL_ATT_DIR + "/" + mail_att_name(uid, a["at"] ?? n, a["name"] ?? "piece")
    small = ""
    # `file_write_base64`, not `File.write_base64` -- it is a bare global,
    # and a `rescue` swallowed the difference: the call raised "undefined
    # method", the rescue turned it into `nil`, and the reader reported
    # nothing written for an hour.
    wrote = file_write_base64(path, a["base64"] ?? "") rescue nil
    shapes = {}
    shapes = mail_thumb_file(path, a["size"] ?? 0) if wrote.nil? != true && mail_image_type?(a["type"] ?? "")
    done.push({
      "name": a["name"] ?? "", "type": a["type"] ?? "",
      "size": a["size"] ?? 0, "path": wrote.nil? ? "" : path,
      "thumb": shapes["thumb"] ?? "", "at": a["at"] ?? n,
      # The shapes, so no frame ever opens an image file to ask.
      "tw": shapes["tw"] ?? 0, "th": shapes["th"] ?? 0,
      "view": shapes["view"] ?? "", "vw": shapes["vw"] ?? 0, "vh": shapes["vh"] ?? 0
    })
    n = n + 1
  end
  return set_key(state, "clipping", set_key(set_key(job, "left", left), "done", done)) if left.length() > 0

  kept = state["clipped"] ?? {}
  mail_say("attachments for uid=" + str(uid) + ": " + str(done.length()) + " written")
  out = set_key(state, "clipped", set_key(kept, str(uid), done))
  set_key(out, "clipping", nil)
end

# The whole message again, for its parts. `fetch_uid` is the same command
# the letter came down with; nothing here is cached because the bytes are
# what this is for.
def mail_pull_parts(address, app_password, uid, numbers = [], here = "")
  # Through `mail_do`, which speaks first and asks questions afterwards:
  # a kept connection is the normal case and a dropped one is Gmail's
  # habit after half an hour idle. Without the retry the first press of
  # `p` after a quiet afternoon answered "those attachments would not
  # come down" -- the connection was gone, and nothing tried a new one.
  # By part number when the letter's fetch left us the numbers, which is
  # every message opened since `fetch_uid_text` existed: only the freight
  # crosses the wire, and the letter is not downloaded a second time.
  # Without them -- a message whose letter came down before this, or a
  # server whose structure would not parse -- the whole message, as
  # before.
  return mail_do(address, app_password, fn(box) { box.fetch_uid_parts(uid, numbers) rescue nil }, here) if numbers.length() > 0

  mail_do(address, app_password, fn(box) { box.fetch_uid(uid) rescue nil }, here)
end

# -- the accounts ------------------------------------------------------------
#
# A screen, like the keys are, and for the same reason: it is a list of
# things to choose between, and a menu that hangs off a corner would be a
# smaller version of the same list with somewhere to hide.
#
# The number beside each one is the key that switches to it, so the
# screen teaches its own shortcut. `@` opens and closes it.
def mail_accounts_sheet(lay, state)
  m = lay["measure"]
  here = state["address"] ?? ""
  list = state["accounts"] ?? []
  # Signed in with nothing on the list is not a state the application
  # reaches -- a credential is saved the moment the server accepts it --
  # but a screen that answers "which account am I in?" with nothing at
  # all would be worse than one row that is right.
  list = [state["account"] ?? mail_account(here, "", "", 0, "", 0)] if list.length() == 0 && here != ""
  rows = range(0, list.length()).map(fn(i) { mail_account_row(list[i] ?? {}, i, here, m) })
  {"k": "scroll", "s": {"grow": 1, "width": "100%"}, "c": [
    column({
      "width": "100%", "align": "center",
      "pad": [lay["roomy"] ? 10 : 7, lay["pad"], 10, lay["pad"]]
    }, [
      column({"width": m, "gap": 6}, [
        column({"gap": 2, "width": m}, [
          text("Accounts", {"size": 6, "weight": "bold", "fg": "accent.base", "width": m}),
          text(list.length() == 1 ? "One account. The number switches to it; @ closes this." : "The number switches, @ closes this.", {
            "size": 1, "fg": "text.muted", "width": m
          })
        ]),
        column({"width": m, "gap": 3}, rows),
        row({"gap": 4, "align": "center", "width": m}, [
          mail_submit("Add another account  +", "add_open"),
          here == "" ? text("", {"size": 0}) : mail_action("Sign out of " + here, "q", "signout")
        ])
      ])
    ])
  ]}
end

def mail_account_row(one, i, here, m)
  address = one["address"] ?? ""
  current = address == here
  {
    "k": "box",
    "key": "acc:" + address,
    "s": {
      "display": "row", "align": "center", "gap": 4, "width": m,
      "pad": [3, 4, 3, 4], "radius": 3, "cursor": "pointer",
      "bg": current == true ? "surface.raised" : "none"
    },
    "on": {"click": "switch"},
    "p": {"address": address, "role": "button", "label": address},
    "c": [
      text(str(i + 1), {
        "font": "mono", "size": 1, "weight": "semibold", "shrink": 0,
        "fg": current == true ? "accent.base" : "text.muted"
      }),
      column({"gap": 0, "grow": 1, "shrink": 1}, [
        text(mail_fit(address, m - 160, 8), {
          "size": 2, "weight": current == true ? "semibold" : "regular",
          "fg": "text.default"
        }),
        text((one["host"] ?? MAIL_HOST) + " · " + (one["smtp"] ?? ""), {
          "font": "mono", "size": 0, "fg": "text.muted"
        })
      ]),
      current == true ? text("on screen", {"size": 0, "fg": "accent.base", "shrink": 0}) : text("", {"size": 0})
    ]
  }
end

# -- the keys ----------------------------------------------------------------
#
# A screen rather than an overlay, and it closes on any key it does not
# use, because a help screen you have to find the exit of is a bad one.
MAIL_SHEET = [
  ["j  or  ↓", "next message — ↓ only on the list"],
  ["k  or  ↑", "previous message — ↑ only on the list"],
  ["g", "first message"],
  ["G", "last message — and the end of the list fetches fifty older"],
  ["Enter", "open — or o"],
  ["Space", "mark the row — d then moves every marked message, Escape clears"],
  ["u", "back to the list — or Escape"],
  ["c", "put the letter in fields so it can be selected — Escape leaves"],
  ["i", "fetch this message's pictures"],
  ["p", "fetch what is attached — pictures are shown, the rest gets a link"],
  ["F", "the folders — ↑ ↓ walk them, Enter opens, Escape comes back"],
  ["M", "the menu — on a narrow window, where the masthead cannot hold it"],
  ["v", "the pictures, one at a time and full size — ← → to walk them"],
  ["o", "the caret on to Browser — Enter then opens the letter in your own"],
  ["t", "the whole letter in one block — selectable across lines, no links"],
  ["h", "the HTML face of a message — the one with the pictures — and back"],
  ["d", "move to the trash — everything marked, or the row under the cursor"],
  ["n", "write a new message"],
  ["a", "answer the sender — A answers everyone on it"],
  ["f", "forward it"],
  ["r", "fetch again — only what is new"],
  ["R", "read every row again — for fields an older build did not store"],
  ["@", "accounts — the number beside one switches to it, + adds one"],
  ["1…9", "switch to that account"],
  ["q", "sign out of this account — the others stay"],
  ["/", "search — typing filters, Enter asks the server"],
  ["?", "this list"],
  ["PgUp PgDn", "move the selection a screenful — on the list"],
  ["Arrows", "scroll a letter — on the list they move the selection"],
  ["Tab", "step through the list — the cursor follows"],
  ["Tab", "in a draft: To, Cc, Subject, the letter, then Send"]
]

def mail_sheet(lay, linked)
  m = lay["measure"]
  rows = MAIL_SHEET.map(fn(pair) { mail_sheet_row(pair[0], pair[1], m) })
  rows = [mail_sheet_note("Sign in first — these are the keys you will have.", m)].concat(rows) if linked != true
  {"k": "scroll", "s": {"grow": 1, "width": "100%"}, "c": [
    column({
      "width": "100%", "align": "center",
      "pad": [lay["roomy"] ? 10 : 7, lay["pad"], 10, lay["pad"]]
    }, [
      column({"width": m, "gap": 6}, [
        column({"gap": 2, "width": m}, [
          text("Keys", {"size": 6, "weight": "bold", "fg": "accent.base", "width": m}),
          text("Any other key puts this away.", {"size": 1, "fg": "text.muted", "width": m})
        ]),
        column({"width": m, "gap": 3}, rows)
      ])
    ])
  ]}
end

def mail_sheet_note(says, m)
  text(says, {"size": 1, "fg": "text.muted", "width": m})
end

def mail_sheet_row(key, says, m)
  row({"gap": 5, "align": "baseline", "width": m}, [
    row({
      "width": 108, "justify": "start", "shrink": 0
    }, [text(key, {"font": "mono", "size": 1, "weight": "semibold", "fg": "text.default"})]),
    text(says, {"size": 2, "fg": "text.muted", "grow": 1, "shrink": 1})
  ])
end
