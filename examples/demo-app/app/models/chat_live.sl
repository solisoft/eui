# A fact with a short life: presence, typing, and the seat counter.
#
# Deliberately not in `chat_meta` beside the sequence counter. That row is
# read on every tick of every session and moved by every write; these are
# read together and written on their own rhythm, and putting them in the
# same collection would have each invalidate the other's cache.
class ChatLive < Model
  validates("key", { "presence": true })
end
