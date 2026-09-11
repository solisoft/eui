# One row, one number: the counter every write moves and every tick reads.
#
# It is what lets a session ask "is there anything new" without reading a
# room to find out — the question is asked once per session per tick, and the
# answer is almost always no.
class ChatMeta < Model
  validates("key", { "presence": true })
end
