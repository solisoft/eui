# Atrium's arithmetic: values in, numbers and strings out.
#
# A service and not part of the controller, for a reason worth stating. This
# is the half the screen cannot show you is wrong — a height that disagrees
# with the row it measures slides the whole river under the person's hands,
# and a grouping rule off by one turns a block of six messages into six
# cards. So it is the half that is tested, and `soli test` preloads
# `app/services` and never `app/controllers`.
#
# Nothing here reads a room or a database. Everything it needs is an
# argument, which is what makes `tests/chat_spec.sl` able to ask it anything
# without a server behind it.

CHAT_LINE_PX = 21
CHAT_MAX_LINES = 8
CHAT_CHAR_PX = 7.4
CHAT_WORD_PACKING = 0.9
CHAT_GROUP_WITHIN = 300
CHAT_PAD_PX = 8
CHAT_HEADER_PX = 20
CHAT_DAY_PX = 40
CHAT_REACTION_PX = 28
CHAT_FILE_PX = 76
CHAT_LINK_PX = 92

# A link card with a picture is taller, and the square the picture draws is
# what makes it so. Both numbers live here because the row height and the row
# itself must come from the same arithmetic — a card drawn taller than the
# list was told slides every row beneath it.
CHAT_LINK_SHOT_PX = 96

# An avatar, and the gutter a grouped message leaves where one would have
# been. One number, because the two must match: the body of a continuation
# lines up with the body above it or the block stops reading as a block.
CHAT_AVATAR_PX = 36

def chat_link_height(card)
  (card["image"] ?? "").blank? ? CHAT_LINK_PX : CHAT_LINK_SHOT_PX
end
CHAT_REPLIES_PX = 26
CHAT_PICTURES = ["png", "jpg", "jpeg", "gif", "webp"]

def chat_link_in(said)
  at = said.index_of("https://")
  return nil if at.nil? || at < 0

  rest = said.substring(at, said.length())
  cut = rest.index_of(" ")
  url = (cut.nil? || cut < 0) ? rest : rest.substring(0, cut)
  url.length() < 12 ? nil : url
end

def chat_unfurl(url)
  without = url.replace("https://", "")
  cut = without.index_of("/")
  host = (cut.nil? || cut < 0) ? without : without.substring(0, cut)
  tail = (cut.nil? || cut < 0) ? "" : without.substring(cut + 1, without.length())
  title = tail == "" ? host : tail.replace("-", " ").replace("/", " · ")
  {
    "host": host,
    "url": url,
    "title": title,
    "note": "Link preview is resolved by the server; the window fetches nothing."
  }
end

# How many lines a body takes, at the width it is given *and the size the
# viewer reads at*.
#
# `scale` is the viewer's font scale, which the client reports and the server
# never assumes. Leaving it out was a real bug and not a rounding one: at a
# scale of 1.5 the characters are half again as wide, the guess said a line
# held forty-six of them when it held thirty, and the body was `clamp`ed to
# fewer lines than it needed — so the last words of a message were simply not
# drawn. A wrap estimate that ignores the reader's own setting is wrong for
# exactly the people who changed it.
def chat_wrap_lines_at(said, width, scale)
  # `width` is the body's own width — the caller has already taken out the
  # avatar, the padding and the tools — so nothing is subtracted here. It
  # used to shave a flat 96 px off, which double-counted the chrome for
  # callers that had already removed it and ignored the tools for those that
  # had not.
  room = width
  room = 160 if room < 160
  # Nine tenths, because text wraps at **words**. A line that would hold
  # forty-seven characters holds forty-two once it has to break where the
  # spaces are, and the error is not symmetric: a guess one line short clamps
  # the body and the last words are simply not drawn, while a guess one line
  # long leaves a few pixels of gap nobody notices. So it errs long.
  per = int(room * CHAT_WORD_PACKING / (CHAT_CHAR_PX * scale))
  per = 20 if per < 20
  lines = 1 + int((said.length() - 1) / per)
  lines = 1 if lines < 1
  return CHAT_MAX_LINES if lines > CHAT_MAX_LINES

  lines
end

# The same, for a reader who has not changed anything.
def chat_wrap_lines(said, width)
  chat_wrap_lines_at(said, width, 1.0)
end

def chat_pair_grouped?(here, prev)
  return false if prev.nil?
  return false if here["who"] != prev["who"]
  return false if here["at"] - prev["at"] > CHAT_GROUP_WITHIN
  return false if chat_day(here["at"]) != chat_day(prev["at"])

  true
end

def chat_day(at)
  at / 86400
end

def chat_pair_day_break?(here, prev)
  return true if prev.nil?

  chat_day(here["at"]) != chat_day(prev["at"])
end

def chat_height_of(here, prev, width, marked, replies, link_tall)
  chat_height_at(here, prev, width, marked, replies, link_tall, 1.0)
end

# Text grows with the reader's setting; a picture does not. So the parts of
# a row that are made of words are scaled and the parts that are made of
# pixels are not — and the row the client is told about is the row that gets
# drawn, which is the only promise this function makes.
def chat_height_at(here, prev, width, marked, replies, link_tall, scale)
  tall = int((CHAT_PAD_PX + chat_wrap_lines_at(here["text"], width, scale) * CHAT_LINE_PX) * scale)
  tall = tall + int(CHAT_HEADER_PX * scale) unless chat_pair_grouped?(here, prev)
  tall = tall + int(CHAT_DAY_PX * scale) if chat_pair_day_break?(here, prev)
  tall = tall + CHAT_FILE_PX if here["shape"] == "file"
  # `link_tall` and not a constant: a card with the page's picture on it is
  # taller than one without, and whether there is a picture is something only
  # the resolved link knows. The row height and the row are built from the
  # same number, which is the whole point of this function.
  tall = tall + link_tall if here["shape"] == "link"
  tall = tall + int(CHAT_REACTION_PX * scale) if marked
  tall = tall + int(CHAT_REPLIES_PX * scale) if replies > 0
  tall
end

# The pixels above row `n` — where the list has to stand for that row to be
# at the top of the viewport. `chat_foot` is this with `n` at the end.
def chat_upto(heights, n)
  total = 0
  i = 0
  for tall in heights
    total = total + tall if i < n
    i = i + 1
  end
  total
end

def chat_foot(heights)
  total = 0
  for tall in heights
    total = total + tall
  end
  total
end

def chat_extension(name)
  cut = name.split(".")
  cut.length() < 2 ? "" : cut[cut.length() - 1].downcase()
end

def chat_picture?(name)
  CHAT_PICTURES.includes?(chat_extension(name))
end

def chat_weight(bytes)
  return str(bytes) + " B" if bytes < 1024
  return str(bytes / 1024) + " KB" if bytes < 1048576

  str(bytes / 1048576) + " MB"
end

def chat_shape_of(body, upload)
  return "file" unless upload.nil?
  return "link" unless chat_link_in(body).nil?

  "line"
end
