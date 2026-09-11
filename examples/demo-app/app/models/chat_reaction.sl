# The reactions on one message: glyph -> the people who placed it.
#
# One document per message rather than one per reaction, because the river
# asks "what is on this message" far more often than anyone adds one, and a
# row is drawn from the whole set or not at all.
class ChatReaction < Model
  validates("message_id", { "presence": true })
end
