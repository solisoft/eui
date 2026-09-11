# Atrium's rooms.
#
# Three collections and a counter. The shape is chosen for the one query the
# river makes constantly — "the messages of this room, in order" — and for
# the one it makes on every tick: "has anything changed?".
#
#   chat_messages   one per message, `n` its position in the room
#   chat_reactions  one per message that has any, glyph -> who
#   chat_replies    one per reply, under the message it answers
#   chat_meta       a single counter every write moves, so a session can ask
#                   whether there is anything to redraw without reading a room

fn up(db: Any) -> Any {
  db.create_collection("chat_messages")
  # The river reads a room in `n` order and nothing else, so that is the
  # index. `n` is unique within a room: it *is* the row number, and two
  # messages sharing one would put two things in the same place.
  db.create_index("chat_messages", "idx_room_n", ["room", "n"], { "unique": true })

  db.create_collection("chat_reactions")
  db.create_index("chat_reactions", "idx_message", ["message_id"], { "unique": true })

  db.create_collection("chat_replies")
  db.create_index("chat_replies", "idx_parent_n", ["parent", "n"], { "unique": true })

  db.create_collection("chat_meta")
}

fn down(db: Any) -> Any {
  db.drop_collection("chat_meta")
  db.drop_collection("chat_replies")
  db.drop_collection("chat_reactions")
  db.drop_collection("chat_messages")
}
