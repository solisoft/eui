# The pieces of this application that are not a screen.
#
# `app/services` is loaded by the web worker **and** by the background job
# workers (`serve/background_jobs.rs`), while `app/controllers` is loaded
# only by the web worker. Anything a job has to call therefore lives here
# rather than beside the views -- which is the whole reason this file
# exists, and the reason to resist putting anything screen-shaped in it.

def mail_say(said)
  return if (getenv("MAIL_DEBUG") ?? "") != "1"

  print("[mail] " + said)
end


def mail_now()
  DateTime.now().to_unix() rescue 0
end


def mail_ms()
  now = DateTime.utc()
  now.to_unix() * 1000 + now.millisecond()
end


def mail_demo?
  said = getenv("MAIL_DEMO") ?? ""
  said == "1"
end


def mail_unique(msgs)
  # A hash of what has been seen, not a scan per message.
  #
  # This was `mail_index` inside a filter -- a linear search of the list
  # for every message in it, so a hundred messages cost five thousand
  # comparisons. It is on the render path (`mail_shown`, twice an event,
  # because the masthead counts what the list draws) and it measured
  # **10 ms an event** on a mailbox of a hundred: the cost of drawing a
  # list that grew with the square of the mailbox and had nothing to do
  # with how much of it was on screen.
  seen = {}
  out = []
  for one in msgs
    uid = str((one ?? {})["uid"] ?? 0)
    if seen[uid].nil?
      seen[uid] = true
      out.push(one)
    end
  end
  out
end


def mail_cap(said, n)
  # `mail_len`, not `length`: the latter counts bytes and `substring`
  # counts characters, so a Greek subject cut at `length` lands in the
  # middle of a word. See the note at the top of mail_html.sl.
  mail_len(said) > n ? said.substring(0, n) + "…" : said
end


def mail_store_file(address)
  keep = ""
  for ch in address.downcase().chars()
    keep = keep + (MAIL_SAFE.contains(ch) ? ch : "-")
  end
  "config/messages-" + keep + ".json"
end

# How many of the newest messages the list holds.
#
# It was twenty, and for a reason that is no longer true: every row used
# to arrive as a whole message, because `Imap.fetch` had one shape,
# `BODY.PEEK[]`. A mailbox of twenty cost about a megabyte.
#
# A row draws a sender, a subject and a date. `fetch_headers_range` asks
# for exactly those, for a whole run, in **one command** -- so the size of
# a batch is bytes, not round trips: twenty headers are about 4.5 KB and a
# hundred about 22 KB, both one wait. The only thing the number still buys
# is a longer list, so it buys one.
MAIL_LIMIT = 100

# --------------------------------------------------------------- the store
#
# The mailbox, kept between runs.
#
# This wants to be SoliDB and is not yet: the collection, the model and the
# migration are written, and `.env` is one password short of running them.
# Rather than leave the window starting blank until then, the same two
# calls are backed by a file -- `mail_store_load` and `mail_store_save`
# are the whole interface, and moving them to `MailMessage` changes
# nothing above this line.
#
# It lives beside the account, under `config/`, because `File` is jailed
# to this application's folder (SEC-006) and `public/` is served. It is in
# `.gitignore` for the obvious reason: it is a copy of your mail.
#
# What it buys is the thing session state cannot: state belongs to a
# connection, so every reconnect started from nothing and refetched
# twenty messages you already had. Now the list is on screen before the
# first IMAP packet goes out.
# One file per account. A mailbox of a hundred headers with the letters
# you have opened folded into it is not small, and it is rewritten on
# every delete, every read mark and every letter fetched -- so the
# accounts you are not looking at are not rewritten with it.
#
# The name is the address with anything outside `MAIL_SAFE` replaced, so
# it cannot name a path: `File` is jailed to this folder (SEC-006), and a
# filename assembled from an address someone typed should not depend on
# that.
MAIL_SAFE = "abcdefghijklmnopqrstuvwxyz0123456789.@_-"

# What older builds wrote: one mailbox, one file, the address inside it.
# Read as a fallback until the account it belongs to saves once.
MAIL_STORE = "config/messages.json"
