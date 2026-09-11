# Atrium — a team messenger, the shape of application EUI has not had yet.
#
# Three columns and a river of messages is the hardest thing to draw over a
# protocol that sends resolved trees: the river is virtualised (04 §7.1), the
# panels are dragged (04 §6), and — unlike every other component in this app
# — what is on the screen changes because *someone else* did something.
#
# Two windows are kept in step by two different things, and it is worth
# knowing which does what. What *happened* is pushed: `chat_told` calls
# `eui_wake("chat")` where the counter moves, and every other session renders
# at once — the socket is bidirectional and a `Batch` is S→C (01 §3), so
# nothing ever required a window to ask. What quietly *stopped being true* is
# still found by asking: the view carries a `wake` (06 §1.1), the client sends
# a tick, and the handler compares what it has seen against what the workspace
# holds — because nobody writes a counter when a presence goes stale.
#
#   * The workspace is a module global, so it is one workspace for the whole
#     server and two windows on /chat are two people in the same room.
#   * History is *derived*, not stored: message 8 of "general" is a function
#     of 8, the way `feed_post` is. Ten thousand messages therefore cost the
#     store nothing and the window only the rows it shows.
#   * What people actually do — send, react, type, attach — is the only
#     thing the store keeps, appended after the derived history.
#
# Run two windows against one server to see the point:
#   SOLI_WS_WORKERS=1 soli serve examples/demo-app --port 5011
#   eui ws://127.0.0.1:5011/_eui/session/chat --allow fs.pick
#
# `SOLI_WS_WORKERS=1` is not decoration. A module global belongs to the
# interpreter of the thread it is on, and the server hashes each session onto
# one of its realtime workers (`lv_sender_for`). With two of those, two
# windows have an even chance of landing on different threads — and then they
# hold *different* workspaces and never see each other, intermittently, which
# is the worst kind of not working. One realtime worker is what makes the
# global one room. An application that wanted this properly would keep the
# room in `Cache` (SoliKV) instead, and pay a round trip per event for it.

# The cast, the rooms and the sample conversation live in
# `app/services/chat_sample.sl`: `soli db:seed` loads services and models and
# not controllers, so the generator has to be somewhere a seed can reach.
# Services are auto-loaded before controllers, so every name it defines —
# CHAT_PEOPLE, CHAT_ROOMS, chat_past, chat_now — is in scope here.

# ---- the arithmetic begins -----------------------------------------------
# Everything between this and "the arithmetic ends" is pure: values in,
# numbers and strings out, no room and no database. It is the half that the
# screen cannot show you is wrong — a height that disagrees with the row it
# measures slides the whole river under the person's hands — so it is the
# half `tests/chat_spec.sl` holds a copy of.
#
# A copy, because a bare script's imports are contained to its own directory
# and `soli test` preloads models, services and helpers but never
# controllers. So the copying is a command and CI can check it changed
# nothing:
#
#     python3 tools/sync_chat_spec.py --check
#
# ------------------------------------------------------------------ links
# A URL in a message becomes a card under it. The unfurl is entirely the
# server's — it is the half of a messenger a client cannot do, and here it
# costs the window nothing but the rows it already draws.


# What a card says about a link. A real fetch belongs behind a cache and a
# timeout — `x_controller.sl` shows the shape — and it is deliberately not
# done on a scroll: the host and the path are what a preview is mostly made
# of anyway, and they cost no round trip at all.

# ---- the arithmetic ends -------------------------------------------------

# ---------------------------------------------------------------- links
#
# An unfurl is a request to someone else's server, so it is resolved **once**
# — when the message is written — and read from `chat_links` for ever after.
# A view that resolved links would fetch a page per card on every scroll.

CHAT_LINKS = {}

def chat_link_of(url)
  chat_kept = CHAT_LINKS[url]
  return chat_kept unless chat_kept.nil?

  chat_found = ChatLink.find_by("url", url) rescue nil
  return nil if chat_found.nil?

  # Not `chat_seen`: that is the name of the function that records presence,
  # and a bare assignment to it replaces the function for the whole process.
  # The next event died with "Cannot call non-function value" pointing at
  # `chat_seen(state["me"])`, which is not where the mistake is.
  chat_card = {
    "url": url,
    "host": chat_found.host ?? chat_link_host(url),
    "title": chat_found.title ?? "",
    "description": chat_found.description ?? "",
    "image": chat_found.image ?? ""
  }
  CHAT_LINKS[url] = chat_card
  chat_card
end

# Ask for it to be resolved — and do not wait.
#
# The fetch itself is `ChatLinkJob`, off this thread, because a link points
# at a server nobody here controls and a server that has gone away does not
# refuse, it hangs. Doing it here would hold the send handler for the whole
# HTTP timeout, and with one realtime worker that holds every session on the
# server: one unreachable link, and the application stops for everyone.
#
# The message is written and drawn at once with the plain card. The picture
# and the title arrive later, the job moves the counter, and the next tick
# fills the card in under the message.
def chat_resolve_link(url)
  return if url.blank?
  return unless chat_link_of(url).nil?

  Job.enqueue("ChatLinkJob", { "url": url }) rescue nil
end

# What the card draws: what was fetched, or — for a link nobody has resolved
# yet, or a page that would not be read — the host and the path, which is
# what this showed before any of it existed.
def chat_card_for(url)
  chat_link_of(url) ?? chat_unfurl(url)
end

# ----------------------------------------------------------- the workspace
#
# The rooms live in SoliDB (`db/migrations`, `db/seeds.sl`). What is here is
# a **read-through cache** of them, and the distinction matters in both
# directions: a message is not said until it is written, and a room is not
# read twice if nothing has moved.
#
# The cache exists because of the one thing the river needs that a query
# cannot cheaply give: a height for every row the list holds, not just the
# ones in view. The client is handed one number per row so the scroll bar is
# honest, so those rows have to be in hand all at once — but they are the
# window the room was opened on, a hundred of them, and not the four thousand
# the room holds. What is above the window is one click and one query away.
#
# `chat_seq` is the whole of the synchronisation, and it is one small
# document: every write moves it, and a session whose tick sees the same
# number has nothing to read and nothing to redraw. That is what makes an
# idle window cost one tiny query a second instead of a room.

CHAT_CACHE = {}

# What stays in this process: presence and typing, and only those. Both are
# timestamps with a life measured in seconds, so they have no business in a
# database that would outlive them by months. The counters they used to sit
# beside have moved into `chat_meta`, because a background job moves them.
CHAT_LOCAL = {
  "presence": {},
  "typing": {}
}

# The counter, as the database has it — read **once** per event and
# remembered for the rest of it.
#
# The memo is not an optimisation, it is the difference between working and
# not. `chat_rows` checks the counter on every access, and measuring a room
# asks for five rows per row: without the memo, opening a room of four
# thousand messages made twenty thousand round trips and took 3.7 seconds.
# With it, two.
CHAT_SEQ_MEMO = -1
CHAT_GEN_MEMO = -1

def chat_forget_seq
  CHAT_SEQ_MEMO = -1
  CHAT_GEN_MEMO = -1
end

# Both counters, from the one document, in one query.
#
# `gen` lives beside `seq` in the database and not in this process because a
# background job moves it: a link resolved off-thread makes a card taller,
# and a height table keyed on a number only this process knew would never
# hear about it.
def chat_read_meta
  chat_meta = ChatMeta.find_by("key", "seq") rescue nil
  CHAT_SEQ_MEMO = chat_meta.nil? ? 0 : (chat_meta.value ?? 0)
  CHAT_GEN_MEMO = chat_meta.nil? ? 0 : (chat_meta.gen ?? 0)
  CHAT_SEQ_MEMO
end

def chat_seq
  chat_read_meta() if CHAT_SEQ_MEMO == -1
  CHAT_SEQ_MEMO
end

def chat_gen
  chat_read_meta() if CHAT_GEN_MEMO == -1
  CHAT_GEN_MEMO
end

def chat_bump
  chat_meta = ChatMeta.find_by("key", "seq") rescue nil
  if chat_meta.nil?
    ChatMeta.create({ "key": "seq", "value": 1 }) rescue nil
    return 1
  end

  # `chat_meta.value = …`, not `chat_meta["value"] = …`: `find_by` hands
  # back a model instance and an instance is not a hash. The index form
  # parses and then fails at run time with "invalid assignment target",
  # which — because every caller of this was inside a `rescue` — meant the
  # counter never moved and nothing ever propagated between sessions.
  chat_next = (chat_meta.value ?? 0) + 1
  chat_meta.value = chat_next
  chat_meta.save()
  CHAT_SEQ_MEMO = chat_next
  chat_told()
  chat_next
end

# The counter moved, so every other window is now a screen behind. Tell them,
# rather than leaving them to find out on their own clock.
#
# The counters move in three places — the two functions below, and
# `ChatLinkJob` when an unfurl lands — and every one of them says so, so there
# is no way to write something and leave the other windows to find out. The
# session that did the writing is left out by `eui_wake` itself: it is already
# rendering, and its own batch is on the way.
#
# The clock stays (`wake: 700` in `chat_view`) and it is not redundant: what
# it watches is the half of this screen that goes stale without anyone doing
# anything — presence that decays after twelve seconds, "is typing" after
# four. Nobody writes a counter when a timestamp quietly stops being true, so
# nobody can be woken for it.
def chat_told
  eui_wake("chat")
end

# Something changed that an existing row is *drawn* from — a reaction, a
# reply, a link that finished resolving. The height table and the row cache
# are keyed on this and not on the sequence, because a sequence that every
# keystroke moves is a cache that never hits.
def chat_reshape
  chat_meta = ChatMeta.find_by("key", "seq") rescue nil
  return chat_bump() if chat_meta.nil?

  chat_meta.gen = (chat_meta.gen ?? 0) + 1
  chat_meta.value = (chat_meta.value ?? 0) + 1
  chat_meta.save()
  CHAT_GEN_MEMO = chat_meta.gen
  CHAT_SEQ_MEMO = chat_meta.value
  chat_told()
  CHAT_SEQ_MEMO
end

# ---- a room, in hand ----------------------------------------------------

# The part of a room that is held in memory: `from` is the index of
# `rows[0]`, and the rows run from there to the end of the room.
#
# A window, and not the room. A room opens on its last hundred and unrolls a
# hundred at a time (`CHAT_OPEN`), so the messages above the window are built
# by nothing, measured by nothing and on the wire nowhere — and reading them
# anyway is what walking into a channel cost: 190 ms of database and 200 ms
# of measuring four thousand rows to put a hundred on the screen.
#
# The window grows at the head when someone unrolls, and at the tail when the
# sequence moves — "what is there after the row I have", which is the query a
# tick makes and is almost always empty.
def chat_reach(room_id, want)
  chat_want = want < 0 ? 0 : want
  # One row earlier than asked for, when there is one: a row is grouped with
  # the one before it and the day rule is drawn against it, so the first row
  # of a window cannot be measured without the row above it. Fetching it with
  # the rest is one query; asking for it afterwards is two.
  chat_floor = chat_want > 0 ? chat_want - 1 : 0
  chat_kept = CHAT_CACHE[room_id]
  return chat_hold(room_id, chat_floor, chat_read_tail(room_id, chat_floor)) if chat_kept.nil?
  return chat_kept if chat_kept["seq"] == chat_seq() && chat_floor >= chat_kept["from"]

  chat_at = chat_kept["from"]
  chat_have = chat_kept["rows"]
  # Someone unrolled: the head grows backwards, by what is missing and no
  # more.
  if chat_floor < chat_at
    chat_have = chat_read_span(room_id, chat_floor, chat_at).concat(chat_have)
    chat_at = chat_floor
  end
  # Something was written: the tail may have grown.
  unless chat_kept["seq"] == chat_seq()
    chat_have = chat_have.concat(chat_read_tail(room_id, chat_at + chat_have.length()))
  end
  chat_hold(room_id, chat_at, chat_have)
end

def chat_hold(room_id, chat_low, held)
  CHAT_CACHE[room_id] = { "seq": chat_seq(), "from": chat_low, "rows": held }
  CHAT_CACHE[room_id]
end

# Rows read raw, between two indices.
#
# `ChatMessage.in_room(room).all()` is the same query and took **3.6
# seconds** for four thousand rows: the cost is not the database, it is
# building four thousand model instances. `@sdbql` returns plain documents
# and does it in a fraction of that. Validation and callbacks are what a
# model is for, and reading a room back wants neither.
def chat_read_span(room_id, chat_low, chat_high)
  chat_docs = @sdbql{
    FOR m IN chat_messages
    FILTER m.room == #{room_id} && m.n >= #{chat_low} && m.n < #{chat_high}
    SORT m.n ASC
    RETURN { n: m.n, who: m.who, at: m.at, text: m.text, shape: m.shape, file: m.file, room: m.room, live: m.live }
  } rescue []
  chat_docs.map(fn(d) { chat_doc(d) })
end

# The same, open at the end: a row, and everything the room has after it.
def chat_read_tail(room_id, chat_low)
  chat_docs = @sdbql{
    FOR m IN chat_messages
    FILTER m.room == #{room_id} && m.n >= #{chat_low}
    SORT m.n ASC
    RETURN { n: m.n, who: m.who, at: m.at, text: m.text, shape: m.shape, file: m.file, room: m.room, live: m.live }
  } rescue []
  chat_docs.map(fn(d) { chat_doc(d) })
end

# A document as the view wants it. The database keeps what was said; `id` is
# how a reaction and a thread name it, and it is derived rather than stored
# so that nothing has to be rewritten if the room is ever renamed.
def chat_doc(d)
  {
    "id": d["room"] + ":" + str(d["n"]),
    "n": d["n"],
    "who": d["who"],
    "at": d["at"],
    "text": d["text"] ?? "",
    "shape": d["shape"] ?? "line",
    "file": d["file"],
    "live": d["live"] == true
  }
end

# How many messages a room holds — **without** reading the room.
#
# The channel column shows an unread count for every room, and a count that
# went through the rows read four thousand documents to find out. Eight rooms
# is thirty-two thousand, and that is where a second of delay after sending a
# message came from: not the room you are in, the seven you are not.
#
# One aggregate answers for all of them, and it is cached against the
# sequence like everything else.
CHAT_COUNTS = {}

def chat_counts
  chat_kept = CHAT_COUNTS["at"]
  return CHAT_COUNTS["by_room"] if !chat_kept.nil? && chat_kept == chat_seq()

  chat_rows_of = @sdbql{
    FOR m IN chat_messages
    COLLECT room = m.room WITH COUNT INTO n
    RETURN { room: room, n: n }
  } rescue []
  chat_by_room = {}
  for chat_one in chat_rows_of
    chat_by_room[chat_one["room"]] = chat_one["n"]
  end
  CHAT_COUNTS = { "at": chat_seq(), "by_room": chat_by_room }
  chat_by_room
end

def chat_count(room_id)
  chat_counts()[room_id] ?? 0
end

# One row, by its index into the room. The window is asked to reach it rather
# than told where it is: a row under the window — a thread opened on a
# message the room has since been unrolled past — grows the window instead of
# coming back empty.
def chat_at_row(room_id, i)
  chat_held = chat_reach(room_id, i)
  chat_one_row = chat_held["rows"][i - chat_held["from"]]
  return chat_one_row unless chat_one_row.nil?

  chat_held["rows"][chat_held["rows"].length() - 1]
end

# The next free row number, asked of the **database** and not of the cache.
#
# `n` is unique per room, so a cache that is one message behind makes the
# next send collide with a row that already exists — and a collision that is
# rescued is a message that vanishes without anyone being told. Which is
# exactly what happened: two sends, one row.
def chat_next_n(room_id)
  chat_top = @sdbql{
    FOR m IN chat_messages
    FILTER m.room == #{room_id}
    SORT m.n DESC
    LIMIT 1
    RETURN m.n
  } rescue []
  chat_top.length() == 0 ? 0 : chat_top[0] + 1
end

def chat_say(room_id, who, body, upload)
  chat_n = chat_next_n(room_id)
  chat_written = {
    "room": room_id,
    "n": chat_n,
    "who": who,
    "at": DateTime.now().to_unix(),
    "text": body,
    "shape": chat_shape_of(body, upload),
    "file": upload,
    "live": true
  }
  # No `rescue nil` here, and that is the point. Every other call in this
  # file may fail into a room that simply looks empty; this one is someone's
  # message, and losing it quietly is the worst thing the file can do. A
  # write that did not take says so, and `chat_send` puts it on the screen.
  chat_resolve_link(chat_link_in(body)) if chat_written["shape"] == "link"
  chat_saved = ChatMessage.create(chat_written)
  chat_wrong = (chat_saved["_errors"] ?? {}).keys().join(", ") rescue ""
  return { "error": chat_wrong } unless chat_wrong.blank?

  # `chat_bump`, not `chat_reshape`: a send appends a row and changes no
  # existing one, so the height table only has to grow. Treating it as a
  # change of shape threw the table away and remeasured four thousand rows
  # on every message — about 85 ms, which is what a send used to feel like.
  chat_bump()
  chat_message_of = chat_doc(chat_written)
  # Appended by number, not by trust: if the window was behind, the row we
  # were given is not the row after the one it holds — so the window goes,
  # and the next render reads back the one it wants.
  chat_kept = CHAT_CACHE[room_id]
  if !chat_kept.nil? && chat_n == chat_kept["from"] + chat_kept["rows"].length()
    chat_hold(room_id, chat_kept["from"], chat_kept["rows"].concat([chat_message_of]))
  else
    CHAT_CACHE[room_id] = nil
  end
  chat_message_of
end

# ---- who is here, and who is mid-sentence -------------------------------
#
# These two stay in memory, and that is not an oversight. Both are timestamps
# that stop being true on their own a few seconds after the last refresh, so
# there is nothing to expire and no way to leave a ghost in a room — and a
# fact with a four-second life has no business in a database that would
# outlive it by months.
#
# It does mean presence is per process: a server with more than one realtime
# worker would have two of these, and two windows that landed on different
# ones would not see each other type. Three things are process-local and
# they are the whole list — this table, `CHAT_SEATS` (which decides who a
# window is), and nothing else. Messages, reactions, replies, read marks,
# links and attachments are all in the database and cross a worker without
# noticing.
#
# So `app.infos` asks the proxy for eight workers and pins the realtime one:
#
#   start_script = "SOLI_WS_WORKERS=1 soli serve . --port $PORT --workers $WORKERS"
#   workers = 8
#
# HTTP scales; the room does not need to. Raising the realtime count is a
# real piece of work and a bounded one — move these three into `chat_meta`
# beside the counter, refreshed on a timer rather than on every event, since
# a fact with a four-second life must not cost a write per tick.

CHAT_PRESENCE_FOR = 12
CHAT_TYPING_FOR = 4

def chat_seen(who)
  CHAT_LOCAL["presence"][str(who)] = DateTime.now().to_unix()
end

def chat_here?(who)
  last = CHAT_LOCAL["presence"][str(who)] ?? 0
  DateTime.now().to_unix() - last < CHAT_PRESENCE_FOR
end

# Presence is only ever claimed for people a window is actually standing in
# for. Everyone else is invented, deterministically, so the rail is not a
# column of grey dots on a server nobody else is connected to.
def chat_online?(who)
  return true if chat_here?(who)

  (who * 5 + 3) % 7 < 3
end

def chat_typing_now(room_id, who)
  CHAT_LOCAL["typing"][room_id + ":" + str(who)] = DateTime.now().to_unix()
  chat_bump()
end

def chat_typists(room_id, not_who)
  now = DateTime.now().to_unix()
  CHAT_LOCAL["typing"].keys().filter(fn(k) {
    k.starts_with?(room_id + ":") && CHAT_LOCAL["typing"][k] > now - CHAT_TYPING_FOR
  }).map(fn(k) {
    int(k.split(":")[1])
  }).filter(fn(w) { w != not_who })
end

# ---- reactions ----------------------------------------------------------
#
# A reaction belongs to the room and not to a session — the point of the demo
# is that the other window sees it — so it is written, not remembered.
#
# One document per message holding every glyph on it, and a per-room cache
# beside the messages: a row asks "what is on this message" on every render
# and there are sixty rows on screen, so sixty queries a frame is not a
# thing that can be allowed to happen.

CHAT_GLYPHS = ["♥", "✓", "★", "↑"]

CHAT_MARKS = {}

def chat_marks(room_id)
  chat_kept = CHAT_MARKS[room_id]
  return chat_kept["by_id"] if !chat_kept.nil? && chat_kept["gen"] == chat_gen()

  chat_marks_of = {}
  chat_docs = ChatReaction.where({ "room": room_id }).all() rescue []
  for chat_one in chat_docs
    chat_marks_of[chat_one["message_id"]] = chat_one["glyphs"] ?? {}
  end
  CHAT_MARKS[room_id] = { "gen": chat_gen(), "by_id": chat_marks_of }
  chat_marks_of
end

def chat_room_of(message_id)
  message_id.split(":")[0]
end

def chat_reactions(message_id)
  chat_marks(chat_room_of(message_id))[message_id] ?? {}
end

def chat_react(message_id, glyph, who)
  chat_all = chat_reactions(message_id)
  chat_mine = chat_all[glyph] ?? []
  chat_all[glyph] = chat_mine.includes?(who) ? chat_mine.filter(fn(w) { w != who }) : chat_mine.concat([who])
  ChatReaction.upsert("message_id", {
    "message_id": message_id,
    "room": chat_room_of(message_id),
    "glyphs": chat_all
  }) rescue nil
  chat_reshape()
  chat_repitch(message_id)
end

def chat_reaction_list(message_id)
  chat_all = chat_reactions(message_id)
  chat_all.keys().filter(fn(g) { (chat_all[g] ?? []).length() > 0 })
end

# ---- threads ------------------------------------------------------------
#
# The same bargain: written down, cached per room by how many each message
# has, because the river draws a reply count on every row and the panel is
# the only thing that wants the replies themselves.

CHAT_THREADS = {}

def chat_reply_counts(room_id)
  chat_kept = CHAT_THREADS[room_id]
  return chat_kept["by_id"] if !chat_kept.nil? && chat_kept["gen"] == chat_gen()

  chat_marks_of = {}
  chat_docs = ChatReply.where({ "room": room_id }).all() rescue []
  for chat_one in chat_docs
    chat_key = chat_one["parent"]
    chat_marks_of[chat_key] = (chat_marks_of[chat_key] ?? 0) + 1
  end
  CHAT_THREADS[room_id] = { "gen": chat_gen(), "by_id": chat_marks_of }
  chat_marks_of
end

def chat_replies(message_id)
  chat_docs = ChatReply.under(message_id).all() rescue []
  chat_docs.map(fn(d) {
    {
      "id": message_id + "/" + str(d["n"]),
      "who": d["who"],
      "at": d["at"],
      "text": d["text"] ?? "",
      "shape": "line",
      "live": true
    }
  })
end

def chat_reply(message_id, who, body)
  chat_have = chat_reply_counts(chat_room_of(message_id))[message_id] ?? 0
  ChatReply.create({
    "parent": message_id,
    "room": chat_room_of(message_id),
    "n": chat_have,
    "who": who,
    "at": DateTime.now().to_unix(),
    "text": body
  }) rescue nil
  chat_reshape()
  # The parent gains a reply count, which is a row that got taller.
  chat_repitch(message_id)
end

def chat_reply_count(message)
  chat_reply_counts(chat_room_of(message["id"]))[message["id"]] ?? 0
end

# ------------------------------------------------------------ measurements
# A windowed list is only honest if the server can say how tall row `i` is
# without building it (04 §7.1): the scroll extent and every row top come
# from those numbers. So every height below is arithmetic over the message,
# and the body is `clamp`ed to exactly the number of lines that arithmetic
# assumed — a row that wrapped one line further than the server guessed
# would slide every row under it.


# Inter at text size 1, in the cozy density the client defaults to: a little
# over seven pixels of advance per character on average prose. It is an
# average and not a promise, which is what the clamp is for.


# Two messages are one block when the same person said them close together.
# This is the single thing that most makes a messenger look like a messenger
# rather than a list of cards, and it is a pure function of two rows.

# Prefixed locals, not decoration: the scope is flat, so a bare `here` or
# `before` in a function this deep lands in the variable of whoever called
# it, three frames up.
# Two messages, not a room and an index: the rule is about the pair and
# nothing else, and written this way it can be checked without a database
# behind it. `chat_grouped?` below is the same question asked of a room.

def chat_grouped?(room_id, i)
  return false if i == 0

  chat_pair_grouped?(chat_at_row(room_id, i), chat_at_row(room_id, i - 1))
end

# Which day a moment falls on, as a number. The height pass asks this four
# times per row, and a room has four thousand of them: a `DateTime.from_unix`
# and a `format` each would be sixteen thousand date conversions to measure
# one room. A division is exact enough to tell two days apart, which is all
# a separator needs; the *label* still uses a real date, but only for the
# sixty rows that are actually built.


def chat_day_break?(room_id, i)
  return true if i == 0

  chat_pair_day_break?(chat_at_row(room_id, i), chat_at_row(room_id, i - 1))
end

# What the row is made of, above and below the body.

# How tall a row is, from the row and what is on it. Pure: a message, the one
# before it, the width, whether anything is stuck to it. This is the number
# the client is given for every row *and* the number the row is built with,
# so the two cannot be allowed to drift — which is why they come from here
# and nowhere else, and why this is the function the spec checks.

# How tall this row's link card will draw, if it has one. A resolved link
# with a picture is taller; one that is only a host and a path is not.
def chat_link_tall(message)
  return CHAT_LINK_PX unless message["shape"] == "link"

  chat_url = chat_link_in(message["text"].to_s)
  return CHAT_LINK_PX if chat_url.nil?

  # The height has to agree with what will actually be drawn, and what is
  # drawn depends on the file still being there.
  chat_shot = chat_card_for(chat_url)["image"].to_s
  return CHAT_LINK_PX if chat_shot.blank? || !File.exists(chat_shot)

  CHAT_LINK_SHOT_PX
end

# The same question asked of a room, which is where the database comes in.
def chat_row_height(room_id, i, width, scale)
  chat_here = chat_at_row(room_id, i)
  chat_earlier = i == 0 ? nil : chat_at_row(room_id, i - 1)
  chat_height_at(
    chat_here,
    chat_earlier,
    width,
    chat_reaction_list(chat_here["id"]).length() > 0,
    chat_reply_count(chat_here),
    chat_link_tall(chat_here),
    scale
  )
end

# Every shown row's height, which is what the client needs for all of them.
# It is recomputed when the room, the width or the workspace changes, and
# kept otherwise: the arithmetic is cheap but a hundred of it on every wake
# is not, and four thousand of it — which is what this measured before the
# table started where the list does — was most of what walking into a channel
# cost.
CHAT_HEIGHTS = {}

# One height per row shown — so the table is grown, not rebuilt.
#
# Three things can change it. A **send** appends a row and leaves every
# earlier one alone, so the answer is the old array with the new rows
# measured onto the end. An **unroll** puts a hundred rows above the ones
# already measured, and those are measured and put in front. A **reaction or
# a reply** changes the height of one existing row, and that is what
# `chat_gen()` counts; there is no cheap way to know which row from here, so
# that one starts again. It is rare, and a send is not.
# One row got taller or shorter — a reaction placed, a reply posted — so
# that row is measured again and the table is kept.
#
# Without this a reaction remeasured everything on screen for a change to one
# row of it — four thousand rows and about 200 ms when the table held the
# whole room, and it was the difference between a heart that lights when you
# click it and one that thinks about it first.
# The row index is in the message id, which is why an id is `room:index`.
#
# Every local here is prefixed, and not out of habit: `chat_row_height` uses
# `tall` itself, and under a flat scope a callee's assignment lands in the
# caller's variable of the same name.
def chat_repitch(message_id)
  chat_parts = message_id.split(":")
  return if chat_parts.length() < 2

  chat_in_room = chat_parts[0]
  chat_index = int(chat_parts[1])
  for chat_key in CHAT_HEIGHTS.keys()
    chat_entry = CHAT_HEIGHTS[chat_key]
    if chat_key.starts_with?(chat_in_room + ":") && chat_index >= chat_entry["from"] && chat_index < chat_entry["n"]
      chat_width = int(chat_key.split(":")[1])
      chat_scale = float(chat_key.split(":")[2])
      chat_one = chat_row_height(chat_in_room, chat_index, chat_width, chat_scale)
      chat_old = chat_entry["tall"]
      chat_start = chat_entry["from"]
      chat_new = range(chat_start, chat_entry["n"]).map(fn(i) {
        i == chat_index ? chat_one : chat_old[i - chat_start]
      })
      CHAT_HEIGHTS[chat_key] = {"gen": chat_gen(), "from": chat_start, "n": chat_entry["n"], "tall": chat_new}
    end
  end
end

# The table starts at the row the list starts at, and not at the room's
# first message:
# the client is handed one height per row *shown*, and a row above the window
# is not shown. It is the same number `chat_base` gives the river, so the two
# cannot disagree about where the room begins.
def chat_heights(room_id, width, scale, chat_base_at)
  key = room_id + ":" + str(width) + ":" + str(scale)
  count = chat_count(room_id)
  kept = CHAT_HEIGHTS[key]
  unless kept.nil?
    if kept["gen"] == chat_gen() && kept["from"] == chat_base_at
      return kept["tall"] if kept["n"] == count
      if kept["n"] < count
        grown = kept["tall"].concat(range(kept["n"], count).map(fn(i) {
          chat_row_height(room_id, i, width, scale)
        }))
        CHAT_HEIGHTS[key] = {"gen": chat_gen(), "from": chat_base_at, "n": count, "tall": grown}
        return grown
      end
    end
    # An unroll moved the floor down: the hundred rows that appeared above it
    # are measured and put in front of the ones already measured, which have
    # not changed by being further down a list.
    if kept["gen"] == chat_gen() && kept["n"] == count && chat_base_at < kept["from"]
      chat_ahead = range(chat_base_at, kept["from"]).map(fn(i) { chat_row_height(room_id, i, width, scale) })
      CHAT_HEIGHTS[key] = {"gen": chat_gen(), "from": chat_base_at, "n": count, "tall": chat_ahead.concat(kept["tall"])}
      return CHAT_HEIGHTS[key]["tall"]
    end
  end

  # A few entries, so two windows of different widths do not take turns
  # remeasuring the room for each other.
  CHAT_HEIGHTS = {} if CHAT_HEIGHTS.size() > 4
  tall = range(chat_base_at, count).map(fn(i) { chat_row_height(room_id, i, width, scale) })
  CHAT_HEIGHTS[key] = {"gen": chat_gen(), "from": chat_base_at, "n": count, "tall": tall}
  tall
end

# Where the foot of the river is, which is the number `scroll_to` wants.
#
# `Op::ScrollTo` speaks absolute pixels and the server does not know the
# height the client gave the list — the chrome above and below it is laid
# out by the client, not measured here. So this answers with the whole
# content height, which is past the foot by one viewport, and the client
# brings it back to the last row: an offset is clamped to its content there
# the way a wheel notch already is. Guessing the viewport instead would land
# short by however much the guess was wrong, which is a river that stops a
# few messages above the newest one and looks like a bug.

# ------------------------------------------------------------ attachments
# EUI 01 §6: a file travels inside the session and is not an asset. What
# arrives is a path in the session's own spool, so a message that is to
# outlive the window that sent it has to be copied somewhere this
# application owns — which is this function, and it is an application's
# decision, not the protocol's.





# ---- the model ends ------------------------------------------------------

# Where a file that is to outlive its session goes. This is the only thing
# in the file that touches the disk, which is why it is on this side of the
# line: `File` and `mkdir_p` are the server's, and a bare script running the
# spec has neither.
CHAT_UPLOAD_DIR = "public/chat"

# Move a picked file out of the session's spool and into the application,
# where it becomes an asset: served by content hash, fetched once, cached by
# the client for ever. That promotion is the application's decision and not
# the protocol's — spec 01 §6 is explicit that a file is not an asset — and
# it is what makes a picture one window attached visible in the other.
#
# `File.copy` and not `File.write(File.read(...))`. The read/write pair goes
# through a string: it mangles anything that is not UTF-8 and raises on most
# of it, so every attachment came back "could not be kept" and the reason
# was swallowed by a `rescue`. A copy moves bytes.
def chat_keep(path, name)
  return { "error": "the upload arrived without a file" } if path.blank?
  return { "error": "the upload was gone before it could be kept" } unless File.exists(path)

  chat_dir = CHAT_UPLOAD_DIR
  mkdir_p(chat_dir)
  chat_leaf = str(DateTime.now().to_unix()) + "-" + name
  chat_kept = chat_dir + "/" + chat_leaf
  File.copy(path, chat_kept)
  return { "error": "the copy into " + chat_dir + " did not land" } unless File.exists(chat_kept)

  { "path": chat_kept, "thumb": chat_thumb(chat_kept, name) }
end

# A small square of a picture, for the card to draw.
#
# Without one the card drew the file itself at its natural size inside a
# 74 px box that clips — so a screenshot showed its **top-left corner,
# magnified**, which on a dark page is a black square. That is why "all my
# previews are black": they were previews of the wrong 74 pixels.
#
# It is also what the wire wants. A 891 KB screenshot fetched by every
# window that ever scrolls past the message, to fill a thumbnail, is most of
# a megabyte spent on something the size of a postage stamp.
CHAT_THUMB_PX = 160

def chat_thumb(kept, name)
  return "" unless chat_picture?(name)

  chat_small = kept + "-thumb.png"
  return chat_small if File.exists(chat_small)

  chat_shrink(kept, chat_small) rescue nil
  File.exists(chat_small) ? chat_small : ""
end

# Cropped to a square rather than letterboxed: the card's box is square, and
# a picture that keeps its shape inside it leaves bars of background that
# read as part of the picture.
def chat_shrink(source, target)
  chat_pic = Image.new(source)
  chat_w = chat_pic.width()
  chat_h = chat_pic.height()
  chat_edge = chat_w < chat_h ? chat_w : chat_h
  chat_scale = CHAT_THUMB_PX * 1.0 / chat_edge
  chat_pic.resize(int(chat_w * chat_scale), int(chat_h * chat_scale)).format("png").to_file(target)
end

# ----------------------------------------------------------------- state
# Everything the session carries, declared in one place and by section. A
# key that is not here is dropped on the next round trip — including, for a
# composer, on every keystroke — so this list is the contract and not a
# convenience.

def chat_defaults
  {
    "me": CHAT_ME_DEFAULT,
    "space": "atrium",
    "room": "general",
    "draft": "",
    "roomy_draft": false,
    "thread": "",
    "thread_draft": "",
    "panel": "",
    "split": 640,
    "split_drag": false,
    "window": [0, 0],
    "base": -1,
    "anchor": -1,
    "seen": -1,
    "at_foot": true,
    "foot": -1,
    "unread_from": {},
    "read_to": {},
    "search": "",
    "searching": false,
    "menu": "",
    "picker": "",
    "devbar": true,
    "attaching": "",
    "trouble": "",
    "collapsed": [],
    "viewport": {
      "width": 1280,
      "height": 800,
      "scale": 1.0,
      "mode": "light",
      "density": "cozy",
      "font_scale": 1.0
    }
  }
end

# The shapes this page takes. One function, so the view, the heights and the
# split handler cannot disagree about how wide anything is.
def chat_layout(state)
  view = state["viewport"] ?? {}
  w = view["width"] ?? 1280
  h = view["height"] ?? 800
  rail = bp_min(w, "lg")
  rooms = bp_min(w, "md")
  panel = state["panel"].to_s
  wide = bp_min(w, "lg") && panel != ""
  taken = (rail ? 64 : 0) + (rooms ? 240 : 0)
  river = wide ? (w - taken) * state["split"] / 1000 : w - taken
  {
    "w": w,
    "h": h,
    # What the reader set their text to. Every wrap estimate and every row
    # height is a function of it, so it travels with the widths rather than
    # being looked up again in four places.
    "scale": view["font_scale"] ?? 1.0,
    # Whether a row has room for its tools. Five 26 px buttons is 130 px
    # taken out of every row for ever, which on a phone is half the column
    # the message needed. Below `sm` they are not drawn, and the body gets
    # the width back.
    "tools": bp_min(w, "sm"),
    "rail": rail,
    "rooms": rooms,
    "panel": wide,
    "river": river < 320 ? 320 : river,
    "split_extent": w - taken
  }
end

# ------------------------------------------------------------- the handler
# One `match`, one branch per intention. Everything that changes the room
# goes through the store and bumps its sequence; everything that changes
# only this window stays in the state.

def chat(event_data)
  event = event_data["event"]
  params = event_data["params"] ?? {}
  state = chat_defaults().merge(event_data["state"] ?? {})
  props = params["props"] ?? {}
  chat_forget_seq()

  # The window is open, so whoever this is counts as here. It is refreshed
  # on every event including the tick, which is what makes presence decay
  # by itself when a window closes.
  chat_seen(state["me"])

  match event {
    "connect" => chat_arrive(state, params),
    "viewport" => chat_set(state, "viewport", params["viewport"] ?? state["viewport"]),
    "tick" => chat_tick(state),
    "window" => chat_window(state, params["payload"]),
    "go_room" => chat_go(state, props["id"].to_s),
    "earlier" => chat_unroll(state),
    "go_space" => chat_set(state, "space", props["id"].to_s),
    "fold" => chat_fold(state, props["id"].to_s),
    "draft" => chat_set(state, "draft", params["payload"].to_s),
    "typing" => chat_typing(state),
    "send" => chat_send(state, params),
    "roomy" => chat_set(state, "roomy_draft", !(state["roomy_draft"] == true)),
    "react" => chat_do_react(state, props),
    "open_thread" => chat_open_thread(state, props["id"].to_s),
    "thread_draft" => chat_set(state, "thread_draft", params["payload"].to_s),
    "thread_send" => chat_thread_send(state, params),
    "panel" => chat_panel(state, props["id"].to_s),
    "close_panel" => chat_set(state, "panel", ""),
    "split" => chat_split(state, params),
    "search" => chat_set(state, "search", params["payload"].to_s),
    "searching" => chat_set(state, "searching", !(state["searching"] == true)),
    "devbar" => chat_set(state, "devbar", !(state["devbar"] == true)),
    "menu" => chat_toggle(state, "menu", props["id"].to_s),
    "picker" => chat_toggle(state, "picker", props["id"].to_s),
    "file_pick" => chat_file_pick(state, params),
    "file_upload" => chat_file_upload(state, params),
    "jump" => chat_jump(state),
    "mark_read" => chat_mark_read(state),
    "dismiss" => chat_set(state, "trouble", ""),
    _ => state,
  }
end

def chat_set(state, key, value)
  state[key] = value
  state
end

def chat_toggle(state, key, value)
  state[key] = state[key].to_s == value ? "" : value
  state
end

# ---- arriving ------------------------------------------------------------
# A window takes an identity when it connects, so a second window on the
# same server is a second person and the room has two people in it. The
# server decides, not the client: the session id is what tells them apart.

CHAT_SEATS = {}

def chat_arrive(state, params)
  state["viewport"] = params["viewport"] ?? state["viewport"]
  taken = CHAT_SEATS.keys().length()
  state["me"] = taken % CHAT_PEOPLE.length()
  CHAT_SEATS[str(taken)] = true
  chat_seen(state["me"])
  # Everything already in the room has been read; only what arrives from
  # here on is new.
  state["seen"] = chat_seq()
  state["read_to"] = chat_read_all()
  state["base"] = chat_open_at(chat_count(state["room"].to_s))
  state["at_foot"] = true
  # A messenger opens at the present. The client mounts a list at its top,
  # so the foot has to be asked for — which is the whole reason `ScrollTo`
  # had to exist on the Soli side at all.
  state["foot"] = 0
  state
end

def chat_read_all
  marks = {}
  for room in CHAT_ROOMS
    marks[room["id"]] = chat_count(room["id"])
  end
  marks
end

# ---- the tick ------------------------------------------------------------
# Spec 06 §1.1 is the only clock a Soli EUI session has. The handler is
# called at every wake and does almost nothing: unless the workspace has
# moved, the state comes back unchanged and the diff is empty, so a quiet
# room costs one event and no ops at all.

def chat_tick(state)
  seq = chat_seq()
  return state if seq == state["seen"]

  state["seen"] = seq
  # The river follows a message that arrives only for a window that was
  # already at the foot of it. One that is reading history is left alone
  # and told there is something new instead.
  state["foot"] = 0 if state["at_foot"] == true
  state
end

# ---- moving about --------------------------------------------------------

def chat_go(state, room_id)
  return state if room_id.blank?

  # Leaving a room reads it.
  marks = state["read_to"] ?? {}
  marks[state["room"]] = chat_count(state["room"])
  state["read_to"] = marks
  state["room"] = room_id
  state["thread"] = ""
  state["panel"] = ""
  state["draft"] = ""
  state["window"] = [0, 0]
  state["base"] = chat_open_at(chat_count(room_id))
  state["anchor"] = -1
  state["at_foot"] = true
  state["foot"] = 0
  state
end

def chat_fold(state, section)
  folded = state["collapsed"] ?? []
  state["collapsed"] = folded.includes?(section) ? folded.filter(fn(s) { s != section }) : folded.concat([section])
  state
end

def chat_panel(state, which)
  state["panel"] = state["panel"].to_s == which ? "" : which
  state["thread"] = "" if which != "thread"
  state
end

def chat_split(state, params)
  lay = chat_layout(state)
  chat_split_event(state, params, lay["split_extent"])
end

def chat_split_event(state, params, extent)
  split_event(state, params, "split", "row", extent, 360, 280, 6)
end

# ---- the window ----------------------------------------------------------
# The client says which rows are in view once a scroll has settled (04
# §7.1). That answer is also how the server knows whether this window is
# reading the present or the past, which is what decides whether a message
# that arrives moves it.

def chat_window(state, payload)
  window = payload ?? [0, 0]
  state["window"] = window
  last = window[1] ?? 0
  count = chat_count(state["room"])
  state["at_foot"] = last >= count - 2
  # Having scrolled to the foot by hand is the same as having been carried
  # there: the room is read and the notice goes.
  state["read_to"] = chat_mark(state, count) if state["at_foot"] == true
  state["foot"] = -1
  state
end

def chat_mark(state, upto)
  marks = state["read_to"] ?? {}
  marks[state["room"]] = upto
  marks
end

def chat_mark_read(state)
  state["read_to"] = chat_mark(state, chat_count(state["room"]))
  state
end

# The one button that has to move the river without the person scrolling.
# It sets `foot` to a number the view turns into `scroll_to`; the view then
# clears it, so the same offset is never sent twice and a person who scrolls
# away is not dragged back.
def chat_jump(state)
  state["at_foot"] = true
  state["read_to"] = chat_mark(state, chat_count(state["room"]))
  state["foot"] = 0
  state
end

# ---- saying something ----------------------------------------------------

def chat_send(state, params)
  # `change` on an editable is not one per keystroke: the client sends it
  # when the field is left or Enter is pressed (03 §3). Enter in an `input`
  # commits the value *and then* submits, so by the time this runs the
  # payload or the draft is the real one.
  said = params["payload"].to_s
  said = state["draft"].to_s if said.blank?
  said = said.trim()
  return state if said.blank?

  chat_put = chat_say(state["room"], state["me"], said, nil)
  # A message that did not take is the one failure on this page that must
  # never be quiet: the draft stays in the box and the banner says why.
  unless (chat_put["error"] ?? "").blank?
    state["trouble"] = "That message was not saved: " + chat_put["error"]
    return state
  end

  state["draft"] = ""
  state["trouble"] = ""
  state["seen"] = chat_seq()
  state["at_foot"] = true
  state["read_to"] = chat_mark(state, chat_count(state["room"]))
  # Whoever sent it is carried to the foot, always: it is their own message
  # and they are entitled to see it land.
  state["foot"] = 0
  state
end

def chat_typing(state)
  chat_typing_now(state["room"], state["me"])
  # Typing is the room's business, not this window's, so the sequence is
  # bumped and the *other* window redraws. This one has nothing to redraw:
  # it is not shown its own typing.
  state["seen"] = chat_seq()
  state
end

def chat_do_react(state, props)
  id = props["id"].to_s
  glyph = props["glyph"].to_s
  return state if id.blank? || glyph.blank?

  chat_react(id, glyph, state["me"])
  state["seen"] = chat_seq()
  state["picker"] = ""
  state
end

# ---- threads -------------------------------------------------------------

def chat_open_thread(state, id)
  return state if id.blank?

  state["thread"] = id
  state["panel"] = "thread"
  state["thread_draft"] = ""
  state
end

def chat_thread_send(state, params)
  said = params["payload"].to_s
  said = state["thread_draft"].to_s if said.blank?
  said = said.trim()
  return state if said.blank? || state["thread"].to_s.blank?

  chat_reply(state["thread"], state["me"], said)
  state["thread_draft"] = ""
  state["seen"] = chat_seq()
  state
end

# ---- files ---------------------------------------------------------------
# Two events, because a file arrives in two moments. `file_pick` is the
# person having chosen — the name and the weight, never a path (03 §3.2) —
# and it is what puts a placeholder on the screen. `file_upload` is the
# bytes having landed, and it is the server's own event, not the client's.

def chat_file_pick(state, params)
  payload = params["payload"] ?? []
  name = (payload[1] ?? "").to_s
  state["attaching"] = name
  state["trouble"] = ""
  state
end

def chat_file_upload(state, params)
  payload = params["payload"] ?? {}
  state["attaching"] = ""
  trouble = payload["error"].to_s
  unless trouble.blank?
    state["trouble"] = payload["name"].to_s + " did not arrive: " + trouble
    return state
  end

  name = payload["name"].to_s
  # A file is not an asset (01 §6): it lives in the session's spool and dies
  # with the socket. Copying it under `public` is what makes it something
  # the other window can be shown, and that is this application's decision.
  chat_put = chat_keep(payload["path"].to_s, name)
  unless (chat_put["error"] ?? "").blank?
    state["trouble"] = name + ": " + chat_put["error"]
    return state
  end

  kept = chat_put["path"]

  chat_say(state["room"], state["me"], state["draft"].to_s.trim(), {
    "name": name,
    "size": payload["size"] ?? 0,
    "path": kept,
    # What the card draws. The file itself stays where it is; this is the
    # small square made from it.
    "thumb": chat_put["thumb"],
    "picture": chat_picture?(name)
  })
  state["draft"] = ""
  state["seen"] = chat_seq()
  state["at_foot"] = true
  state["read_to"] = chat_mark(state, chat_count(state["room"]))
  state["foot"] = 0
  state
end

# =========================================================================
# The view
#
# Nothing below holds state and nothing below decides anything: every
# function here is `state` in, nodes out. What makes it look like an
# application rather than a catalogue is three things and they are all
# composition — a fixed spatial rhythm, one surface per depth, and the
# grouping of consecutive messages.
#
# The palette is the protocol's. A Soli application cannot ship a theme
# today (the manifest sends none and the client fetches none), so there is
# no brand colour here, only roles — which is the constraint EUI argues for
# and a fair test of whether it is enough.
# =========================================================================

def chat_view(raw)
  chat_forget_seq()
  state = chat_defaults().merge(raw ?? {})
  lay = chat_layout(state)
  columns = []
  columns = columns.concat([chat_rail(state)]) if lay["rail"]
  columns = columns.concat([chat_rooms_column(state)]) if lay["rooms"]
  columns = columns.concat([chat_main(state, lay)])
  page = row(
    {
      "gap": 0,
      "width": "100%",
      "height": "100%",
      "bg": "surface.base",
      "align": "stretch"
    },
    columns
  )
  # The dev bar, over everything: what the *last* render cost, which is the
  # only honest thing it can report.
  #
  # Off until the `⋯` in the header raises it, and that is not shyness.
  #
  # Its figures change on every render, so with it up every tick carries four
  # or five ops and the window repaints twice a second — for ever. That is
  # the meter moving the needle by being read, and it is most visible exactly
  # where you notice it least kindly: a batch puts every locally previewed
  # style back before it diffs, so the row under the pointer is un-hovered
  # and re-hovered on every one of those ticks.
  #
  # With it down an idle tick sends nothing at all — no ops, no batch, no
  # frame — and a window sitting open costs the server nothing measurable.
  # Raise it when you want a number, put it away when you want the app.
  page = stack({"gap": 0, "width": "100%", "height": "100%"}, [
    page,
    dev_bar(eui_stats(), state["devbar"] == true)
  ])
  # The one waking node (06 §1.1). It is the page itself, so the clock stops
  # when the window closes and there is nothing else to tear down.
  #
  # No `with_state`: the session's state lives on the server and the handler
  # is handed it back on every event. `with_state` is for the other thing —
  # putting a copy in the *client's* props so a local chunk can read it —
  # and this page's chunks only ever repoint a style at one the session
  # already holds. Sending it anyway would fail outright, since a prop
  # cannot be a hash and half of this state is hashes.
  # `keys` narrows what the page hears to the one key it wants (03 §3.1), so
  # typing in the composer is not a stream of key events to the server.
  page["p"] = {"wake": 700, "keys": ["F2"]}
  page["on"] = {"wake": "tick", "key_down": "devbar"}
  page
end

# ---- the rail ------------------------------------------------------------
# Workspaces, 64 px, the deepest surface on the page. A rail is the one
# place a literal square of colour is the right answer: it is an identity,
# not a status, and there is nothing on it to read.

def chat_rail(state)
  tiles = CHAT_SPACES.map(fn(space) {
    chat_space_tile(space, space["id"] == state["space"].to_s)
  })
  column(
    {
      "gap": 2,
      "pad": [3, 2, 3, 2],
      "width": 64,
      "height": "100%",
      "align": "center",
      "bg": "surface.sunken",
      "border": [0, 1, 0, 0],
      "border_color": "border.subtle"
    },
    tiles.concat([spacer(), chat_rail_me(state)])
  )
end

def chat_space_tile(space, here)
  face = initial_avatar(space["name"][0], space["tone"], 40)
  face["s"]["radius"] = here ? 2 : 3
  face["s"]["transition"] = "base"
  face["p"] = {"id": space["id"]}
  face["on"] = {"click": "go_space"}
  # The active workspace is squared off rather than recoloured: the tile is
  # already the brightest thing in the column and a second signal in colour
  # would be one too many.
  keyed("space:" + space["id"], face)
end

# The foot of the rail: the palette switch, and nothing else.
#
# There used to be the viewer's avatar above it, and it was a duplicate — the
# channel column already ends with who you are signed in as, with a name
# beside it, which is the version that actually tells you something. Two
# portraits of the same person in the same corner is furniture.
def chat_rail_me(state)
  column({"gap": 2, "align": "center"}, [theme_toggle()])
end

# An avatar with a presence dot sitting on its corner, composed in a stack
# (04 §3) — the dot is ringed in the surface behind it so it reads against
# the avatar as well as against the column.
def chat_presence_avatar(who, size, here)
  face = initial_avatar(chat_initial(who), chat_tone(who), size)
  face["s"]["shrink"] = 0
  face["s"]["grow"] = 0
  dot = {
    "k": "box",
    "s": {
      "width": 11,
      "height": 11,
      "radius": 4,
      "bg": here ? "success.base" : "text.disabled",
      "border": 2,
      "border_color": "surface.raised"
    }
  }
  stack(
    {"width": size, "height": size, "shrink": 0, "justify": "end", "align": "end"},
    [face, dot]
  )
end

# ---- the channel column --------------------------------------------------

def chat_rooms_column(state)
  space = CHAT_SPACES.filter(fn(s) { s["id"] == state["space"].to_s })
  name = space.length() > 0 ? space[0]["name"] : "Atrium"
  column(
    {
      "gap": 0,
      "width": 240,
      "height": "100%",
      "bg": "surface.raised",
      "border": [0, 1, 0, 0],
      "border_color": "border.subtle"
    },
    [
      chat_space_head(name),
      scroll(
        {"grow": 1, "min_height": 0, "pad": [1, 2, 3, 2]},
        [
          chat_section(state, "Channels", chat_rooms_of("channel")),
          chat_section(state, "Direct messages", chat_rooms_of("dm"))
        ]
      ),
      divider(),
      chat_me_card(state)
    ]
  )
end

def chat_space_head(name)
  row(
    {
      "gap": 2,
      "align": "center",
      "pad": [3, 3, 3, 3],
      "width": "100%",
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    [text(name, {"weight": "bold", "size": 2}), spacer(), icon_button("▾", "noop", {}, {
      "icon": "chevron_down",
      "name": "Workspace menu",
      "key": "chat_space_menu",
      "size": "sm"
    })]
  )
end

def chat_section(state, title, rooms)
  folded = (state["collapsed"] ?? []).includes?(title)
  head = row(
    {
      "gap": 1,
      "align": "center",
      "pad": [2, 2, 1, 2],
      "cursor": "pointer",
      "width": "100%"
    },
    [
      text(folded ? "›" : "⌄", {"fg": "text.muted", "size": 1, "width": 12}),
      text(title, {"fg": "text.muted", "size": 0, "weight": "semibold"})
    ]
  )
  head["p"] = {"id": title}
  head["on"] = {"click": "fold"}
  rows = folded ? [] : rooms.map(fn(r) { chat_room_row(state, r) })
  column({"gap": 0, "width": "100%"}, [keyed("fold:" + title, head)].concat(rows))
end

# One row of the channel list. Unread is two signals at once — the name goes
# bold and a count appears — because either alone is missable in a column of
# twenty, and losing your place is the thing this row exists to prevent.
def chat_room_row(state, room)
  here = room["id"] == state["room"].to_s
  unread = chat_unread(state, room["id"])
  label = room["kind"] == "dm" ? room["name"] : "# " + room["name"]
  resting = {
    "display": "row",
    "gap": 2,
    "align": "center",
    "pad": [1, 2, 1, 2],
    "radius": 2,
    "cursor": "pointer",
    "width": "100%",
    "transition": "fast",
    "bg": here ? "accent.base" : "none"
  }
  fg = here ? "accent.on" : (unread > 0 ? "text.default" : "text.muted")
  parts = []
  parts = parts.concat([chat_dm_dot(room)]) if room["kind"] == "dm"
  parts = parts.concat([text(label, {
    "fg": fg,
    "weight": (unread > 0 && !here) ? "semibold" : "regular",
    "grow": 1,
    "shrink": 1,
    "min_width": 0,
    "clamp": 1
  })])
  parts = parts.concat([chat_unread_badge(unread, here)]) if unread > 0
  built = {
    "k": "box",
    "s": resting,
    "p": {"id": room["id"]},
    "c": parts
  }
  # Hover is a local chunk (07): the row repoints itself at a style the
  # session already holds, so running a pointer down a list of twenty rooms
  # sends nothing at all.
  built["on"] = here ? {"click": "go_room"} : stateful(resting, {
    "hover": {"bg": "surface.sunken"},
    "press": {"bg": "surface.sunken"}
  }, {"click": "go_room"})
  keyed("room:" + room["id"], built)
end

def chat_dm_dot(room)
  who = room["who"] ?? 1
  {
    "k": "box",
    "s": {
      "width": 8,
      "height": 8,
      "radius": 4,
      "bg": chat_online?(who) ? "success.base" : "none",
      "border": chat_online?(who) ? 0 : 1,
      "border_color": "border.strong"
    }
  }
end

def chat_unread_badge(n, here)
  {
    "k": "box",
    "s": {
      "display": "row",
      "justify": "center",
      "align": "center",
      "min_width": 20,
      "height": 18,
      "radius": 4,
      "pad": [0, 1, 0, 1],
      "bg": here ? "accent.on" : "accent.base"
    },
    "c": [text(n > 99 ? "99+" : str(n), {
      "size": 0,
      "weight": "bold",
      "fg": here ? "accent.base" : "accent.on"
    })]
  }
end

def chat_unread(state, room_id)
  read = (state["read_to"] ?? {})[room_id]
  return 0 if read.nil?

  count = chat_count(room_id) - read
  count < 0 ? 0 : count
end

def chat_me_card(state)
  me = state["me"]
  row(
    {"gap": 2, "align": "center", "pad": [2, 3, 2, 3], "width": "100%"},
    [
      chat_presence_avatar(me, 28, true),
      column(
        {"gap": 0, "grow": 1, "shrink": 1, "min_width": 0},
        [
          text(chat_person(me), {"weight": "semibold", "size": 1, "clamp": 1}),
          muted("Active")
        ]
      )
    ]
  )
end

# ---- the main column -----------------------------------------------------

def chat_main(state, lay)
  river = column(
    {"gap": 0, "grow": 1, "min_height": 0, "min_width": 0, "height": "100%"},
    [
      chat_head(state, lay),
      chat_notice(state),
      chat_backlog(state),
      chat_river(state, lay),
      chat_typing_line(state),
      chat_composer(state, lay)
    ]
  )
  return river unless lay["panel"]

  # The thread and the member list share one draggable divider (04 §6). The
  # panels are functions of their own width, so what is inside them can
  # branch on the space they actually got rather than on the window.
  split_pane({
    "key": "split",
    "dir": "row",
    "size": lay["split_extent"],
    "cross": "100%",
    "fraction": state["split"],
    "min_a": 360,
    "min_b": 280,
    "on_drag": "split",
    "dragging": state["split_drag"] == true,
    "a": fn(px) { river },
    "b": fn(px) { chat_panel_view(state, px) }
  })
end

def chat_head(state, lay)
  room = chat_room(state["room"].to_s)
  title = room["kind"] == "dm" ? room["name"] : "# " + room["name"]
  left = [
    text(title, {"weight": "bold", "size": 2, "shrink": 0}),
    chat_head_topic(room, lay)
  ]
  right = []
  right = right.concat([chat_search_field(state)]) if state["searching"] == true
  right = right.concat([
    icon_button("⌕", "searching", {}, {
      "icon": "search",
      "name": "Search this channel",
      "key": "chat_search",
      "size": "sm",
      "tone": state["searching"] == true ? "accent" : "quiet"
    }),
    chat_members_button(state, room),
    icon_button("⋯", "devbar", {}, {
      "icon": "more_v",
      "name": state["devbar"] == true ? "Hide what the last render cost" : "Show what the last render cost",
      "key": "chat_room_menu",
      "size": "sm",
      "tone": state["devbar"] == true ? "accent" : "quiet"
    })
  ])
  toolbar([row({"gap": 3, "align": "baseline", "shrink": 1, "min_width": 0}, left), spacer()].concat(right))
end

def chat_head_topic(room, lay)
  return spacer() unless bp_min(lay["river"], "md")
  return spacer() if room["topic"].to_s.blank?

  text(room["topic"], {"fg": "text.muted", "size": 1, "clamp": 1, "shrink": 1, "min_width": 0})
end

def chat_search_field(state)
  sized_input(state["search"].to_s, "search", 200)
end

# Who is in the channel, and how many. One control, not an icon with a
# number beside it: the count *is* the label, and a filled icon button next
# to a loose numeral reads as a stray blue square.
def chat_members_button(state, room)
  here = state["panel"].to_s == "members"
  resting = {
    "display": "row",
    "gap": 1,
    "align": "center",
    "pad": [0, 2, 0, 2],
    "height": 26,
    "radius": 2,
    "cursor": "pointer",
    "transition": "fast",
    "fg": here ? "accent.base" : "text.muted",
    "bg": here ? "surface.sunken" : "none"
  }
  built = {
    "k": "box",
    "s": resting,
    "p": {"id": "members"},
    "c": [
      text_interned("☻", {"size": 1}),
      text(str(room["members"] ?? 0), {"size": 1, "weight": "semibold"})
    ]
  }
  built["on"] = stateful(resting, {
    "hover": {"bg": "surface.sunken"},
    "press": {"bg": "surface.sunken"}
  }, {"click": "panel"})
  keyed("chat_members", built)
end

# A banner and nothing more clever: something went wrong with a file, or a
# room has moved on while this window was reading its past.
def chat_notice(state)
  trouble = state["trouble"].to_s
  # `dismiss`, not `mark_read`: the button said Dismiss and marked the room
  # read instead, so it did nothing anyone could see and left the banner
  # exactly where it was.
  return banner(trouble, "danger", "Dismiss", "dismiss") unless trouble.blank?

  unread = chat_unread(state, state["room"].to_s)
  return spacer_zero() if unread == 0 || state["at_foot"] == true

  banner(str(unread) + " new " + (unread == 1 ? "message" : "messages"), "info", "Jump to latest", "jump")
end

# What is above the window, offered rather than sent.
#
# In the column whatever the answer is, like the banner beside it: a fixed
# list of children is a diff that keeps a node, and a list that grows and
# shrinks is one that inserts and removes.
def chat_backlog(state)
  chat_room_id = state["room"].to_s
  chat_above = chat_base(state, chat_count(chat_room_id))
  return spacer_zero() if chat_above == 0

  row(
    {"justify": "center", "pad": [2, 0, 1, 0], "bg": "surface.base"},
    [button_variant(str(chat_above) + " earlier " + (chat_above == 1 ? "message" : "messages"), "earlier", "surface.raised", "text.muted")]
  )
end

# One more page, and the view stays where it is: `foot` of -1 is what tells
# the river not to send a `scroll_to`, so the message you were reading does
# not jump out from under you because a hundred older ones arrived above it.
def chat_unroll(state)
  chat_win = state["window"] ?? [0, 0]
  chat_was = chat_base(state, chat_count(state["room"].to_s))
  chat_now = chat_was - CHAT_OPEN
  chat_now = 0 if chat_now < 0
  chat_moved = chat_was - chat_now
  return state if chat_moved == 0

  state["base"] = chat_now
  # A hundred rows appear *above* the ones on screen, so every index into
  # what is shown moves down by a hundred — and the pixel the client is
  # standing at now points a hundred messages further back than the one
  # being read. The anchor is the row that was at the top; the river turns
  # it into the offset that puts it back there.
  state["window"] = [(chat_win[0] ?? 0) + chat_moved, (chat_win[1] ?? 0) + chat_moved]
  state["anchor"] = (chat_win[0] ?? 0) + chat_moved
  state["foot"] = -1
  state
end

# A zero-height nothing, so the column's children stay a fixed list and the
# diff has a node to keep rather than a child to insert and remove.
def spacer_zero
  {"k": "box", "s": {"height": 0}}
end

# How wide a message body actually gets, which is not how wide the river is.
#
# Out of the river come the row's padding, the avatar gutter, the gap after
# it, and — where there is room for them — the tools on the right. The wrap
# estimate and the `clamp` the body is drawn with both come from this number,
# and they must: a body clamped to fewer lines than it needs does not overflow
# or ellipsise, it silently loses its last words.
CHAT_TOOLS_PX = 130
CHAT_ROW_CHROME_PX = 88

def chat_body_width(lay)
  chat_room_px = lay["river"] - CHAT_ROW_CHROME_PX
  chat_room_px = chat_room_px - CHAT_TOOLS_PX if lay["tools"]
  chat_room_px < 160 ? 160 : chat_room_px
end

# ---- the river -----------------------------------------------------------
# The virtualised list (04 §7.1). The client holds `count` row heights and
# nothing else; the server builds the rows in view plus a runway either
# side, so a room of four thousand messages costs one window.

CHAT_RUNWAY = 24

# How much of a room a window opens on.
#
# A room holds four thousand messages and a person arriving wants the last
# page of them, not the year. Everything about the river is already paid per
# *visible* row — the window builds forty of them and `heights` is cached —
# but `count` is not: it is one wire number per message in the room, sent
# with the list and reconverted whenever the list node is rebuilt, and it is
# what the client sizes the scrollbar from. Four thousand of those to put a
# hundred on screen is the wrong bargain, and a scrollbar whose thumb is two
# pixels tall is not a control anyone can use.
#
# `back` raises it, a page at a time, and the room is the whole room again
# once it passes the count.
CHAT_OPEN = 100

# The first message a room shows, as an index into it. Everything below this
# is still in the database and one click away; it is simply not on the wire.
#
# It is **chosen when the room is opened** and moved only by unrolling —
# never derived from the count. Derived, it slides down by one with every
# message that arrives, and then every row's index into the list slides with
# it: a send stops being one row appended and becomes the whole window
# rewritten. Measured on a room of four thousand, that is the difference
# between 15 ops and 55.
def chat_base(state, count)
  # Below zero means "not chosen yet": the state declares every key it
  # carries, and a room that has not been walked into has no base.
  chat_from = state["base"] ?? -1
  return chat_open_at(count) if chat_from < 0
  return count if chat_from > count

  chat_from
end

# Where a room starts when you walk into it.
def chat_open_at(count)
  chat_from = count - CHAT_OPEN
  return 0 if chat_from < 0

  chat_from
end

# The whole river, by everything it is drawn from. The window moves when the
# person scrolls, which is exactly when rebuilding it is fair.
CHAT_RIVERS = {}

def chat_river(state, lay)
  room_id = state["room"].to_s
  key = [
    room_id,
    str(chat_body_width(lay)),
    str(chat_gen()),
    str(chat_count(room_id)),
    str(state["me"]),
    (state["window"] ?? [0, 0]).join(","),
    str((state["read_to"] ?? {})[room_id] ?? -1),
    str(state["foot"]),
    str(state["base"] ?? -1),
    str(state["anchor"] ?? -1)
  ].join("|")
  kept = CHAT_RIVERS[key]
  return kept unless kept.nil?

  # Room for a few, not for one. Every window on this server shares these
  # globals and every one of them has its own key — its own identity, its own
  # scroll — so a cache that kept a single entry would have two windows
  # evicting each other on every tick and hitting never. That is worse than
  # no cache at all: the miss costs the rebuild *and* the clear.
  CHAT_RIVERS = {} if CHAT_RIVERS.size() > 8
  built = chat_build_river(state, lay)
  CHAT_RIVERS[key] = built
  built
end

# Indices here come in two kinds and mixing them slides the whole river by a
# page, so they are named apart: `base` and anything `chat_` is an index into
# the *room*, and `first`, `last` and whatever the client reports in `window`
# are indices into what is *shown*. The list only ever hears the second kind,
# because as far as it is concerned the room starts at `base`.
def chat_build_river(state, lay)
  room_id = state["room"].to_s
  count = chat_count(room_id)
  base = chat_base(state, count)
  shown = count - base
  heights = chat_heights(room_id, chat_body_width(lay), lay["scale"], base)
  window = state["window"] ?? [0, 0]
  first = (window[0] ?? 0) - CHAT_RUNWAY
  first = 0 if first < 0
  last = (window[1] ?? 0) + CHAT_RUNWAY
  last = shown - 1 if last > shown - 1
  # A window that has not reported yet opens on the present, not the past:
  # the foot is where a messenger starts.
  if window[0] == 0 && window[1] == 0
    first = shown - 40
    first = 0 if first < 0
    last = shown - 1
  end
  read = (state["read_to"] ?? {})[room_id] ?? count
  chat_prune(base + first, base + last)
  rows = last < first ? [] : range(first, last + 1).map(fn(i) {
    chat_row(state, room_id, base + i, lay, read, base)
  })
  river = list_window({"grow": 1, "min_height": 0, "bg": "surface.base"}, 64, shown, heights, rows, "window")
  # `scroll_to` is an instruction and not a state: it goes out when the view
  # names an offset it did not name last time. `chat_foot` is a different
  # number every time a row is added, which is exactly what makes the river
  # follow a message instead of sitting still.
  unless state["foot"] == -1
    river["p"]["scroll_to"] = [0, chat_foot(heights)]
  end
  # Holding a place rather than following the foot. Left in the state on
  # purpose: `scroll_to` travels only when the number changes, so an anchor
  # that stays put is sent once and then costs nothing every tick after.
  chat_at = state["anchor"] ?? -1
  if state["foot"] == -1 && chat_at > 0
    river["p"]["scroll_to"] = [0, chat_upto(heights, chat_at)]
  end
  # Keyed, because a keyed node whose hash is *the same object* is kept by
  # the encoder and never converted again — and this node carries `heights`,
  # one number per message in the room. Four thousand of them turned into
  # wire values twice a second is what an idle window used to cost: 18% of a
  # core for a screen where nothing was happening. Cached, an idle tick
  # converts nothing at all.
  keyed("river:" + room_id, river)
end

# ---- one message ---------------------------------------------------------
# Four things decide what a row looks like, and all four are the same
# arithmetic `chat_row_height` did, in the same order — if they ever drift
# apart the list slides, so they are written to be read side by side.

# Rows already built, by everything they are drawn from.
#
# This is the whole reason an idle window is cheap. The view runs on every
# event — a tick every 700 ms included — and the framework has no way to be
# told "nothing changed", so the saving has to be in the view: a row that
# comes back as *the very same hash* is kept by the encoder and skipped by
# the diff (the memo `Kept` exists for this, and `FEED_CARDS` in
# live_controller.sl is the same bargain).
#
# The key is everything a row is a function of. `chat_gen()` covers the
# reactions and the replies without naming them; the width covers the wrap;
# the unread rule is its own flag because it moves between two rows without
# either of them changing.
CHAT_ROWS = {}

def chat_row_key(state, room_id, i, lay, rule, base)
  # `me` is in the key because the cache is a *global* and two windows are
  # two people: a reaction chip is drawn lit for whoever placed it, so a row
  # built for one of them is not the row the other should be handed.
  # `base` too: a row names its place in the *list*, and the list starts at
  # `base` — so the same message drawn after another hundred were unrolled
  # is a different row prop and cannot be the same cached node.
  room_id + "#" + str(i) + ":" + str(chat_body_width(lay)) + ":" + str(chat_gen())
    + ":" + str(state["me"]) + ":" + str(base) + (rule ? ":rule" : "")
end

# The cache holds a few windows, not the room.
def chat_prune(first, last)
  return if CHAT_ROWS.size() < 400

  keep = {}
  for key in CHAT_ROWS.keys()
    n = int(key.split("#")[1].split(":")[0])
    keep[key] = CHAT_ROWS[key] if n >= first - 100 && n <= last + 100
  end
  CHAT_ROWS = keep
end

def chat_row(state, room_id, i, lay, read, base)
  rule = i == read && read < chat_count(room_id) && read > 0
  key = chat_row_key(state, room_id, i, lay, rule, base)
  kept = CHAT_ROWS[key]
  return kept unless kept.nil?

  built = chat_build_row(state, room_id, i, lay, rule, base)
  CHAT_ROWS[key] = built
  built
end

def chat_build_row(state, room_id, i, lay, rule, base)
  message = chat_at_row(room_id, i)
  grouped = chat_grouped?(room_id, i)
  tall = chat_row_height(room_id, i, chat_body_width(lay), lay["scale"])
  parts = []
  parts = parts.concat([chat_day_rule(message["at"])]) if chat_day_break?(room_id, i)
  parts = parts.concat([chat_unread_rule()]) if rule
  parts = parts.concat([chat_message(state, message, grouped, lay)])
  built = {
    "k": "box",
    "s": {
      "display": "column",
      "gap": 0,
      "height": tall,
      "overflow": "clip",
      "width": "100%"
    },
    # The list counts from `base`, not from the start of the room: this is
    # the prop the layout matches against its window (04 §7), and a row that
    # names an index the list does not have is dropped without a word.
    "p": {"row": i - base, "item_height": tall},
    "c": parts
  }
  keyed(room_id + "#" + str(i), built)
end

# The day, on a hairline. Not sticky — spec 04 has no sticky position, and
# inventing one in the server would be a lie the first time someone
# scrolled.
def chat_day_rule(at)
  row(
    {"gap": 3, "align": "center", "height": CHAT_DAY_PX, "pad": [0, 4, 0, 4], "width": "100%"},
    [
      chat_hairline("border.subtle"),
      {
        "k": "box",
        "s": {
          "pad": [0, 3, 0, 3],
          "radius": 4,
          "border": 1,
          "border_color": "border.subtle",
          "bg": "surface.base",
          "shrink": 0
        },
        "c": [text(chat_day_label(at), {"size": 0, "weight": "semibold", "fg": "text.muted"})]
      },
      chat_hairline("border.subtle")
    ]
  )
end

def chat_day_label(at)
  when_day = DateTime.from_unix(at)
  today = DateTime.from_unix(chat_now())
  return "Today" if when_day.format("%Y-%m-%d") == today.format("%Y-%m-%d")

  when_day.format("%A %e %B")
end

# Where reading stopped. It is the one rule on the page in `danger.base`,
# because it is the one thing you are meant to be unable to scroll past
# without noticing.
def chat_unread_rule()
  row(
    {"gap": 3, "align": "center", "height": CHAT_DAY_PX, "pad": [0, 4, 0, 4], "width": "100%"},
    [
      chat_hairline("danger.base"),
      text("New", {"size": 0, "weight": "bold", "fg": "danger.base", "shrink": 0})
    ]
  )
end

def chat_hairline(tone)
  {"k": "box", "s": {"height": 1, "grow": 1, "bg": tone}}
end

# The message itself. A grouped message drops its avatar and its name and
# keeps only its body, indented to where the body above it was — that one
# rule is most of what separates a messenger from a list of cards, and it
# costs the row twenty pixels and nothing else.
def chat_message(state, message, grouped, lay)
  who = message["who"]
  lines = chat_wrap_lines_at(message["text"], chat_body_width(lay), lay["scale"])
  stack_of = []
  stack_of = stack_of.concat([chat_byline(who, message["at"])]) unless grouped
  stack_of = stack_of.concat([text(message["text"], {"clamp": lines, "fg": "text.default"})]) unless message["text"].to_s.blank?
  stack_of = stack_of.concat([chat_attachment(message)]) if message["shape"] == "file"
  stack_of = stack_of.concat([chat_link_card(message)]) if message["shape"] == "link"
  reacted = chat_reaction_list(message["id"])
  stack_of = stack_of.concat([chat_reaction_row(state, message, reacted)]) if reacted.length() > 0
  replies = chat_reply_count(message)
  stack_of = stack_of.concat([chat_replies_link(message, replies)]) if replies > 0

  resting = {
    "display": "row",
    "gap": 3,
    "align": "start",
    "pad": [1, 4, 1, 4],
    "grow": 1,
    "min_height": 0,
    "width": "100%",
    # What the row's tools inherit.
    #
    # `text.default`, and it took three tries to get here.
    #
    # These are click targets that are always on screen — there is no hover
    # reveal any more — so the quiet roles are the wrong instinct.
    # `surface.base` has no contrast floor at all when used as a foreground;
    # `text.disabled` has the weakest floor the palette offers and *means*
    # "you cannot press this"; and `text.muted`, which reads fine on the
    # protocol's own palette, is still a smudge once a desktop theme has
    # supplied its own colours (05 §6 — the overrides carry no contrast
    # guarantee). What is legible on every palette is the role text is
    # written in. The hover background is the affordance; the glyph is just
    # meant to be seen.
    #
    # Everything else in a row names its own colour, because anything that
    # did not would move with these.
    "fg": "text.default"
  }
  # No presence dot here. It is the only thing in a row that changes with
  # the clock, so it alone would keep every row out of the cache below —
  # and presence belongs where you go looking for someone, which is the
  # rail and the roster, not four thousand avatars in a river.
  gutter = grouped ? chat_gutter(message["at"], lay["river"]) : chat_avatar(who)
  built = {
    "k": "box",
    "s": resting,
    "p": {"id": message["id"]},
    "c": [
      gutter,
      column({"gap": 0, "grow": 1, "shrink": 1, "min_width": 0}, stack_of),
      chat_row_tools(state, message, lay)
    ]
  }
  # Hovering a message raises it, locally. Four thousand rows and a pointer
  # dragged down them sends nothing.
  # The hover, and the two things that make it cost nothing.
  #
  # `stateful` compiles to `self.style = @hover`, and `self` is atom 0 — the
  # node the handler is running on, resolved by the client. So the source is
  # the same for every row in every room and the whole river is **one**
  # interned chunk. (It was not always: `self` used to compile to the node's
  # key, which made a keyed row with a local handler one chunk per key, and
  # the table holds 4 095 — a scroll ended the session about seven hundred
  # rows in.)
  #
  # Nothing inside the row carries a pointer handler of its own, on purpose.
  # The client hovers the deepest node but dispatches to the nearest
  # *ancestor* that handles the event, so a pointer on a tool is still a
  # pointer on the row: the tools stay up while you reach for them instead of
  # flickering away as you cross into them.
  built["on"] = stateful(resting, {
    "hover": {"bg": "surface.raised", "fg": "text.default"},
    "press": {"bg": "surface.raised", "fg": "text.default"}
  }, {})
  keyed("msg:" + message["id"], built)
end

def chat_byline(who, at)
  row(
    {"gap": 2, "align": "baseline", "height": CHAT_HEADER_PX},
    [
      text_interned(chat_person(who), {"weight": "semibold", "size": 1, "fg": "text.default"}),
      text(chat_clock(at), {"fg": "text.muted", "size": 0})
    ]
  )
end

# Where a grouped message's avatar would have been. The time lives here and
# is drawn in the surface it sits on, so it is present for the pointer and
# absent for the eye — which is what Slack does and why a block of six
# messages reads as one paragraph.
# A round avatar that stays round.
#
# `shrink: 0`, and it is not optional. A flex child shrinks by default, so on
# a narrow column — a phone — the row squeezed the avatar horizontally and
# left it an oval, while the circle it is meant to be comes from `radius` on
# a *square*. Nothing about the square survives being compressed.
def chat_avatar(who)
  chat_face = initial_avatar(chat_initial(who), chat_tone(who), CHAT_AVATAR_PX)
  chat_face["s"]["shrink"] = 0
  chat_face["s"]["grow"] = 0
  chat_face
end

# Where a grouped message's avatar would have been, and what goes in it.
#
# The box is exactly as wide as an avatar so the text below lines up with the
# text above — that alignment is most of what makes a block of messages read
# as one paragraph. What it holds has to fit inside that width, and a
# timestamp does not always: text scales with the viewer's font setting and
# this box does not, so at a large scale the time ran out of its own gutter
# and pushed into the message beside it.
#
# So it is clipped rather than allowed to push, and on a narrow column it is
# not drawn at all. A time that is only there to be glanced at is not worth a
# ragged left edge on a phone.
def chat_gutter(at, room_px)
  chat_shown = bp_min(room_px, "sm") ? [text(chat_clock(at), {
    "fg": "text.disabled",
    "size": 0,
    "clamp": 1
  })] : []
  {
    "k": "box",
    "s": {
      "width": CHAT_AVATAR_PX,
      "shrink": 0,
      "display": "row",
      "justify": "end",
      "align": "center",
      "overflow": "clip",
      "pad": [0, 1, 0, 0]
    },
    "c": chat_shown
  }
end

def chat_clock(at)
  DateTime.from_unix(at).format("%H:%M")
end

# ---- what hangs off a message -------------------------------------------

def chat_reaction_row(state, message, glyphs)
  chips = glyphs.map(fn(g) {
    chat_reaction_chip(state, message["id"], g)
  })
  row({"gap": 1, "align": "center", "height": CHAT_REACTION_PX, "wrap": "wrap"}, chips)
end

def chat_reaction_chip(state, message_id, glyph)
  who = chat_reactions(message_id)[glyph] ?? []
  mine = who.includes?(state["me"])
  resting = {
    "display": "row",
    "gap": 1,
    "align": "center",
    "pad": [0, 2, 0, 2],
    "height": 22,
    "radius": 4,
    "cursor": "pointer",
    "transition": "fast",
    "border": 1,
    "border_color": mine ? "accent.base" : "border.subtle",
    "bg": mine ? "surface.sunken" : "surface.raised"
  }
  built = {
    "k": "box",
    "s": resting,
    "p": {"id": message_id, "glyph": glyph},
    "c": [
      text_interned(glyph, {"size": 0, "fg": mine ? "accent.base" : "text.muted"}),
      text(str(who.length()), {"size": 0, "weight": "semibold", "fg": mine ? "accent.base" : "text.muted"})
    ]
  }
  # No pointer handlers: this sits inside a row, and a handler here would
  # take `enter`/`leave` off the row and put the tools out with them.
  built["on"] = {"click": "react"}
  keyed("re:" + message_id + ":" + glyph, built)
end

def chat_replies_link(message, n)
  face = row(
    {"gap": 2, "align": "center", "height": CHAT_REPLIES_PX, "cursor": "pointer"},
    [
      text("↩", {"size": 0, "fg": "accent.base"}),
      text(str(n) + (n == 1 ? " reply" : " replies"), {"size": 0, "weight": "semibold", "fg": "accent.base"})
    ]
  )
  face["p"] = {"id": message["id"]}
  face["on"] = {"click": "open_thread"}
  face
end

# The tools for the row you are pointing at.
#
# They are always in the tree and always laid out — a node that arrived on
# hover would be a round trip, and one that was `display: none` would reflow
# the message beside it the moment the pointer touched the row. Only the
# opacity moves, and it moves in a local chunk, so a pointer dragged down
# four thousand rows sends nothing at all.
# Invisible at rest because it is the colour of what is behind it, and not
# because it is `opacity: 0` — opacity is per node and does not reach a
# child, while `fg` is inherited by any child that does not set its own.
# That is why the tools below decline to set theirs.
CHAT_TOOLS_RESTING = {
  "display": "row",
  "gap": 0,
  "align": "start",
  "shrink": 0,
  "fg": "surface.base",
  "transition": "fast"
}

def chat_row_tools(state, message, lay)
  return spacer_zero() unless lay["tools"]

  glyphs = CHAT_GLYPHS.map(fn(g) {
    chat_quiet_tool(g, "react", {"id": message["id"], "glyph": g})
  })
  reply = chat_quiet_tool("↩", "open_thread", {"id": message["id"]})
  {
    "k": "box",
    "s": CHAT_TOOLS_RESTING,
    "c": glyphs.concat([reply])
  }
end

# Hover inside a river row, which is not as simple as it looks.
#
# The client hovers the *deepest* node under the pointer and sends `leave` to
# what it was over before `enter` to what it is over now. So moving from the
# body of a row onto one of its own tools is a genuine `leave` of the row —
# and a row that hid its tools on `leave` hid them exactly as you reached for
# them, then showed them again as the pointer landed back. That was the
# flicker, and giving each tool its own chunk to keep the bar up was the
# first fix: correct, and fatal, because a chunk source that names a row's
# key is a new interned chunk per row and the table holds 4 095.
#
# The fix that costs nothing is to give the tools no handlers at all. An
# event dispatches to the nearest *ancestor* that handles it, so `leave` and
# `enter` both land on the row whichever of its children the pointer is over,
# and the row simply stays entered. One chunk for the whole river.
#
# A tool inside a row therefore sets no colour of its own — it inherits the
# row's, which is the ground at rest and `text.muted` under the pointer — and
# answers only a click.
def chat_quiet_tool(glyph, on_click, props)
  {
    "k": "box",
    "s": {
      "display": "row",
      "justify": "center",
      "align": "center",
      "width": 26,
      "height": 26,
      "radius": 1,
      "cursor": "pointer",
      "bg": "none"
    },
    "p": props,
    "on": {"click": on_click},
    "c": [text_interned(glyph, {"size": 1})]
  }
end

# A tool outside the river — the composer's — which owns its own colour and
# its own hover, because nothing above it is doing that for it.
def chat_tool(key, glyph, on_click, props)
  chat_tool_in(key, glyph, on_click, props, "text.muted")
end

def chat_tool_in(key, glyph, on_click, props, tone)
  resting = {
    "display": "row",
    "justify": "center",
    "align": "center",
    "width": 26,
    "height": 26,
    "radius": 1,
    "cursor": "pointer",
    "transition": "fast",
    "bg": "none"
  }
  resting["fg"] = tone unless tone == ""
  built = {
    "k": "box",
    "s": resting,
    "p": props,
    "c": [text_interned(glyph, {"size": 1})]
  }
  # Hover and press only set the background; a tool inside the river has no
  # colour of its own and must keep inheriting the one that hides it, or
  # pointing at a row would light every tool on it at once.
  built["on"] = stateful(resting, {
    "hover": {"bg": "surface.sunken"},
    "press": {"bg": "surface.sunken"}
  }, {"click": on_click})
  keyed("tool:" + key, built)
end

# ---- attachments and links ----------------------------------------------

# An attachment, drawn from what is still on disk.
#
# A picture is only drawn if the file is **there**. An `image` node names a
# file by path and the server hashes it to put it on the wire, so a path that
# no longer exists is not a blank square — it is a view that cannot be
# encoded, which ends the session (01 §4) and takes the whole room with it.
# One deleted file, one dead channel, for everybody.
#
# So a message whose file has gone keeps its card and loses its picture. That
# is the honest thing to show: the file was attached, and it is not here any
# more.
def chat_attachment(message)
  file = message["file"]
  return chat_sample_file(message) if file.nil?

  # The thumbnail where there is one, the file itself for anything attached
  # before thumbnails existed.
  chat_where = file["thumb"].to_s
  chat_where = file["path"].to_s if chat_where.blank?
  chat_there = !chat_where.blank? && File.exists(chat_where)
  return chat_picture_card(file) if file["picture"] == true && chat_there

  chat_file_card(file["name"], chat_attachment_note(file, chat_there))
end

# The stamp to draw for an attached picture.
#
# A message stored before thumbnails existed carries none, and handing the
# original over instead is 891 KB fetched by every window that ever scrolls
# past it, to fill a box the size of a stamp — and, until the client learned
# to shrink what will not fit its sheet, a picture off a phone drew as a
# black square. So one is made here, once: what this writes is what the next
# render finds, and `chat_thumb` hands back an existing file untouched.
def chat_thumb_of(file)
  chat_small = file["thumb"].to_s
  return chat_small if !chat_small.blank? && File.exists(chat_small)

  chat_full = file["path"].to_s
  return chat_full if chat_full.blank? || !File.exists(chat_full)

  chat_made = chat_thumb(chat_full, file["name"].to_s) rescue ""
  return chat_full if chat_made.blank?

  chat_made
end

def chat_attachment_note(file, there)
  return chat_weight(file["size"] ?? 0) if there

  "no longer on the server"
end

# The derived past has files in it too, so a scroll through history shows
# what an attachment looks like without anyone having to attach one.
def chat_sample_file(message)
  chat_file_card("run-" + str(message["n"]) + ".log", chat_weight(2048 + message["n"] % 90000))
end

# A picture, with its name beside it.
#
# The name is not decoration. A picture that the client cannot decode is not
# an error anywhere — the server hashes bytes and puts them on the wire, the
# client fails to make an image of them — so a card that was only a picture
# drew an empty box and said nothing at all about what was in it. With the
# name there, the worst case reads as "a file called holiday.png that will
# not display", which is a thing a person can act on.
def chat_picture_card(file)
  row(
    {
      "gap": 3,
      "align": "center",
      "height": CHAT_FILE_PX,
      "radius": 2,
      "overflow": "clip",
      "border": 1,
      "border_color": "border.subtle",
      "bg": "surface.raised",
      "margin": [1, 0, 0, 0]
    },
    [
      {
        "k": "box",
        "s": {
          "width": CHAT_FILE_PX - 2,
          "height": CHAT_FILE_PX - 2,
          "shrink": 0,
          "overflow": "clip",
          "bg": "surface.sunken",
          "display": "row",
          "justify": "center",
          "align": "center"
        },
        # The thumbnail, drawn to fill the square. `image` does not scale a
        # picture to its box: it draws at the size the style asks for, and
        # anything larger is clipped — which is why this is a small square
        # made on the way in rather than the file itself squeezed here.
        "c": [image(chat_thumb_of(file), CHAT_FILE_PX - 2, CHAT_FILE_PX - 2)]
      },
      column(
        {"gap": 0, "grow": 1, "shrink": 1, "min_width": 0, "pad": [0, 3, 0, 0]},
        [
          text(file["name"].to_s, {"weight": "semibold", "size": 1, "clamp": 1, "fg": "text.default"}),
          muted(chat_weight(file["size"] ?? 0))
        ]
      )
    ]
  )
end

def chat_file_card(name, weight)
  row(
    {
      "gap": 3,
      "align": "center",
      "height": CHAT_FILE_PX,
      "pad": [0, 3, 0, 3],
      "radius": 2,
      "border": 1,
      "border_color": "border.subtle",
      "bg": "surface.raised",
      "margin": [1, 0, 0, 0]
    },
    [
      {
        "k": "box",
        "s": {
          "width": 40,
          "height": 40,
          "radius": 1,
          "bg": "surface.sunken",
          "display": "row",
          "justify": "center",
          "align": "center",
          "shrink": 0
        },
        "c": [text(chat_extension(name).upcase(), {"size": 0, "weight": "bold", "fg": "text.muted"})]
      },
      column(
        {"gap": 0, "grow": 1, "shrink": 1, "min_width": 0},
        [text(name, {"weight": "semibold", "size": 1, "clamp": 1, "fg": "text.default"}), muted(weight)]
      )
    ]
  )
end

# The card a link draws: the picture the page offers, its title, and the
# sentence it describes itself with.
#
# The picture sits on the left at a fixed square, because a card whose height
# follows its image is a card whose *row* height the server cannot state
# without having fetched the image — and the river is measured before it is
# drawn. A fixed box also means one shape for every link, which is what makes
# a column of them read as a list rather than a scrapbook.
def chat_link_card(message)
  url = chat_link_in(message["text"].to_s)
  return spacer_zero() if url.nil?

  card_of = chat_card_for(url)
  card_title = card_of["title"].to_s
  card_title = card_of["note"].to_s if card_title.blank?
  card_desc = card_of["description"].to_s
  card_desc = card_of["note"].to_s if card_desc.blank?
  card_shot = card_of["image"].to_s
  card_body = column(
    {"gap": 0, "pad": [2, 3, 2, 3], "grow": 1, "shrink": 1, "min_width": 0, "justify": "center"},
    [
      muted(card_of["host"]),
      text(card_title, {"weight": "semibold", "size": 1, "clamp": 1, "fg": "text.default"}),
      text(card_desc, {"size": 0, "fg": "text.muted", "clamp": 2})
    ]
  )
  # Same rule as an attachment: a picture whose file has gone is a view the
  # server cannot encode, so it is only drawn when the file is there.
  card_has_shot = !card_shot.blank? && File.exists(card_shot)
  card_parts = card_has_shot ? [chat_link_shot(card_shot), card_body] : [card_body]
  row(
    {
      "gap": 0,
      "height": chat_link_height(card_of),
      "margin": [1, 0, 0, 0],
      "radius": 2,
      "overflow": "clip",
      "bg": "surface.raised",
      "border": [0, 0, 0, 3],
      "border_color": "accent.base",
      "align": "stretch"
    },
    card_parts
  )
end

# A square of the page's own picture. `overflow: clip` on the box and a
# picture sized to the box's height means a wide banner is cropped to a
# square rather than letterboxed into a strip of background.
def chat_link_shot(src)
  {
    "k": "box",
    "s": {
      "width": CHAT_LINK_SHOT_PX,
      "height": CHAT_LINK_SHOT_PX,
      "shrink": 0,
      "overflow": "clip",
      "bg": "surface.sunken",
      "display": "row",
      "justify": "center",
      "align": "center"
    },
    "c": [image(src, "auto", CHAT_LINK_SHOT_PX)]
  }
end

# ---- who is typing -------------------------------------------------------
# A fixed-height line under the river, always in the tree. Anything that
# appears and disappears between two rows shoves the river up and down by
# its own height, which is worse than a line of empty space.

def chat_typing_line(state)
  typists = chat_typists(state["room"].to_s, state["me"])
  said = chat_typing_words(typists)
  row(
    {"height": 22, "pad": [0, 4, 0, 4], "align": "center", "bg": "surface.base"},
    [text(said, {"size": 0, "fg": "text.muted", "clamp": 1})]
  )
end

def chat_typing_words(typists)
  return "" if typists.length() == 0
  return chat_person(typists[0]) + " is typing…" if typists.length() == 1
  return chat_person(typists[0]) + " and " + chat_person(typists[1]) + " are typing…" if typists.length() == 2

  "Several people are typing…"
end

# ---- the composer --------------------------------------------------------
# The one floating piece on the page: its own surface, its own border, and
# a gap of air beneath it.
#
# Enter sends, and the single-line field is what makes that correct rather
# than nearly correct. `change` is not one per keystroke (03 §3) — the
# client sends it when the field is left or Enter is pressed — and Enter in
# an `input` commits the value *and then* emits `submit`, so the server has
# the real text at the moment it is asked to send it. A `textarea` would
# not: Enter inserts a newline there and nothing is committed, so a send
# would post whatever the field last happened to blur with. The roomy box
# is therefore a `textarea` with a Send button, which blurs it first.

def chat_composer(state, lay)
  roomy = state["roomy_draft"] == true
  box = roomy ? chat_roomy_field(state) : chat_line_field(state)
  shell = column(
    {
      "gap": 1,
      "pad": 2,
      "radius": 2,
      "border": 1,
      "border_color": "border.default",
      "bg": "surface.raised",
      "width": "100%"
    },
    [box, chat_composer_tools(state, roomy)]
  )
  column(
    {"gap": 0, "pad": [1, 4, 3, 4], "width": "100%", "bg": "surface.base"},
    [shell, chat_composer_hint(state, roomy)]
  )
end

def chat_line_field(state)
  room = chat_room(state["room"].to_s)
  where = room["kind"] == "dm" ? room["name"] : "#" + room["name"]
  box = input(state["draft"].to_s, "draft", {
    "key": "chat_draft",
    "style": {
      "border": 0,
      "bg": "none",
      "pad": [1, 1, 1, 1],
      "width": "100%"
    },
    # `autofocus` (03 §3.1) because a messenger puts the caret where you are
    # going to type — and because a key event reaches nothing at all while
    # nothing is focused, so F2 would be a shortcut you had to click first
    # to earn.
    "props": {"label": "Message " + where, "autofocus": true},
    "on": {"submit": "send", "text_input": "typing"}
  })
  box
end

def chat_roomy_field(state)
  textarea(state["draft"].to_s, "draft", {
    "key": "chat_draft_roomy",
    "rows": 4,
    "style": {
      "border": 0,
      "bg": "none",
      "pad": [1, 1, 1, 1],
      "width": "100%"
    },
    "props": {"label": "Message"},
    "on": {"text_input": "typing"}
  })
end

def chat_composer_tools(state, roomy)
  attaching = state["attaching"].to_s
  left = [
    chat_attach_button(state),
    chat_tool("composer:roomy", roomy ? "⌄" : "⌃", "roomy", {}),
    chat_emoji_button(state)
  ]
  left = left.concat([chat_attaching_chip(attaching)]) unless attaching.blank?
  right = roomy ? [button("Send", "send")] : [muted("Enter to send")]
  row({"gap": 1, "align": "center", "width": "100%"}, left.concat([spacer()]).concat(right))
end

# The picker, and the whole of what makes it one: `pick` names what the
# dialog accepts, and the node carries a *server* handler for `file_pick`.
# Neither alone opens anything (03 §3.2), and neither does a tree that
# merely arrives — the person has to activate it.
def chat_attach_button(state)
  resting = {
    "display": "row",
    "justify": "center",
    "align": "center",
    "width": 28,
    "height": 28,
    "radius": 1,
    "cursor": "pointer",
    "transition": "fast",
    "fg": "text.muted"
  }
  built = {
    "k": "box",
    "s": resting,
    "p": {"pick": "png,jpg,jpeg,gif,webp,pdf,txt,log,csv,md"},
    "c": [text_interned("⊕", {"size": 2})]
  }
  built["on"] = stateful(resting, {
    "hover": {"bg": "surface.sunken", "fg": "text.default"},
    "press": {"bg": "surface.sunken"}
  }, {"file_pick": "file_pick"})
  keyed("chat_attach", built)
end

def chat_attaching_chip(name)
  row(
    {"gap": 2, "align": "center", "pad": [0, 2, 0, 2], "radius": 4, "bg": "surface.sunken"},
    # `spinner_sized` takes pixels, not a size name — a string here divided
    # by two inside the catalogue and ended the session on the render right
    # after the file was picked.
    [spinner_sized(14), text(name, {"size": 0, "clamp": 1, "fg": "text.default"})]
  )
end

def chat_emoji_button(state)
  popover(
    chat_tool("composer:emoji", "☺", "picker", {"id": "composer"}),
    [chat_glyph_menu(state)],
    state["picker"].to_s == "composer"
  )
end

# Four glyphs, and the reason there are four: no colour emoji font is
# embedded (the client ships Inter, JetBrains Mono and Noto Sans Symbols),
# so a reaction is a symbol drawn in the text colour. Handing the person a
# grid of tofu would be worse than handing them four things that draw.
def chat_glyph_menu(state)
  glyphs = CHAT_GLYPHS.map(fn(g) {
    chat_tool("pick:" + g, g, "react", {"id": chat_last_id(state), "glyph": g})
  })
  {
    "k": "box",
    "s": {
      "display": "row",
      "gap": 1,
      "pad": 2,
      "radius": 2,
      "bg": "surface.overlay",
      "border": 1,
      "border_color": "border.subtle",
      "shadow": 2
    },
    "c": glyphs
  }
end

def chat_last_id(state)
  room_id = state["room"].to_s
  room_id + ":" + str(chat_count(room_id) - 1)
end

# Under the composer: nothing, unless something went wrong.
#
# There used to be a line of advice here — what the paperclip does, where the
# dev bar lives — and advice under a composer is read once and then occupies
# the bottom of the window for ever. What belongs in that space is the one
# thing you need at the moment it happens: a message that did not send, a
# file that did not arrive.
def chat_composer_hint(state, roomy)
  trouble = state["trouble"].to_s
  return {"k": "box", "s": {"height": 0}} if trouble.blank?

  row({"height": 18, "pad": [0, 1, 0, 1]}, [text(trouble, {"size": 0, "fg": "danger.base", "clamp": 1})])
end

# ---- the right-hand panel ------------------------------------------------

def chat_panel_view(state, px)
  which = state["panel"].to_s
  body = which == "thread" ? chat_thread_panel(state, px) : chat_members_panel(state, px)
  column(
    {
      "gap": 0,
      "width": "100%",
      "height": "100%",
      "min_height": 0,
      "bg": "surface.raised",
      "border": [0, 0, 0, 1],
      "border_color": "border.subtle"
    },
    [chat_panel_head(which), body]
  )
end

def chat_panel_head(which)
  row(
    {
      "gap": 2,
      "align": "center",
      "pad": [2, 3, 2, 3],
      "width": "100%",
      "border": [0, 0, 1, 0],
      "border_color": "border.subtle"
    },
    [
      text(which == "thread" ? "Thread" : "Members", {"weight": "bold"}),
      spacer(),
      icon_button("✕", "close_panel", {}, {
        "icon": "close",
        "name": "Close the panel",
        "key": "chat_panel_close",
        "size": "sm"
      })
    ]
  )
end

def chat_thread_panel(state, px)
  id = state["thread"].to_s
  return empty_state("No thread open", "Pick a reply count in the river.", "", "") if id.blank?

  parent = chat_by_id(id)
  replies = chat_replies(id)
  rows = replies.map(fn(r) { chat_thread_row(r, px) })
  column(
    {"gap": 0, "grow": 1, "min_height": 0, "width": "100%"},
    [
      scroll(
        {"grow": 1, "min_height": 0, "pad": [2, 0, 2, 0]},
        [chat_thread_parent(parent, px), divider()].concat(rows)
      ),
      chat_thread_composer(state)
    ]
  )
end

def chat_by_id(id)
  parts = id.split(":")
  return nil if parts.length() < 2

  chat_at_row(parts[0], int(parts[1]))
end

def chat_thread_parent(message, px)
  return spacer_zero() if message.nil?

  chat_thread_row(message, px)
end

def chat_thread_row(message, px)
  row(
    {"gap": 3, "align": "start", "pad": [2, 3, 2, 3], "width": "100%"},
    [
      chat_presence_avatar(message["who"], 32, chat_online?(message["who"])),
      column(
        {"gap": 0, "grow": 1, "shrink": 1, "min_width": 0},
        [
          chat_byline(message["who"], message["at"]),
          text(message["text"], {"clamp": chat_wrap_lines(message["text"], px)})
        ]
      )
    ]
  )
end

def chat_thread_composer(state)
  column(
    {"gap": 0, "pad": [1, 3, 3, 3], "width": "100%"},
    [input(state["thread_draft"].to_s, "thread_draft", {
      "key": "chat_thread_draft",
      "props": {"label": "Reply"},
      "style": {"width": "100%"},
      "on": {"submit": "thread_send"}
    })]
  )
end

# ---- members -------------------------------------------------------------

def chat_members_panel(state, px)
  room = chat_room(state["room"].to_s)
  # A channel's roster is the cast, in the order the room decides, so two
  # channels do not show the same list.
  seats = range(0, CHAT_PEOPLE.length()).map(fn(i) {
    (i * 5 + room["name"].length()) % CHAT_PEOPLE.length()
  })
  here = seats.filter(fn(w) { chat_online?(w) })
  away = seats.filter(fn(w) { !chat_online?(w) })
  scroll(
    {"grow": 1, "min_height": 0, "pad": [2, 2, 3, 2], "width": "100%"},
    [chat_member_group("Online", here), chat_member_group("Away", away)]
  )
end

def chat_member_group(title, who)
  rows = who.map(fn(w) { chat_member_row(w) })
  column(
    {"gap": 0, "width": "100%"},
    [row({"pad": [2, 2, 1, 2]}, [text(title + " — " + str(who.length()), {
      "size": 0,
      "weight": "semibold",
      "fg": "text.muted"
    })])].concat(rows)
  )
end

def chat_member_row(who)
  keyed("member:" + str(who), row(
    {"gap": 2, "align": "center", "pad": [1, 2, 1, 2], "radius": 2, "width": "100%"},
    [
      chat_presence_avatar(who, 28, chat_online?(who)),
      text(chat_person(who), {"size": 1, "clamp": 1, "grow": 1, "shrink": 1, "min_width": 0})
    ]
  ))
end
