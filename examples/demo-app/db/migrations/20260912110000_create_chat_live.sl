# What is true for a few seconds, written down so a second worker knows it.
#
# Presence, typing and the seat that decides who a window is were the three
# things Atrium kept in the memory of the process serving a session — and
# the whole reason it needed exactly one realtime worker. Two workers meant
# two windows that could not see each other type and could be handed the
# same identity: not degraded, wrong.
#
# They are here now, which is what lets `SOLI_WS_WORKERS` be more than 1.
# One row per fact, keyed:
#
#   seat          the number of windows that have ever arrived
#   p:<who>       when that person was last seen
#   t:<room>:<who>  when they last typed in that room
#
# A row is overwritten, never appended, so the collection is bounded by the
# cast and the rooms — a few dozen rows, which is why reading all of it once
# per event is cheaper than asking a question per person.
# `chat_lives`, plural. A model derives its collection from its class name
# through the inflector — `ChatLive` becomes `chat_lives`, as `ChatMessage`
# becomes `chat_messages` — and a migration that names the singular creates
# a collection nobody reads while the model quietly makes its own on first
# write. The rows appear and work; the **index does not exist**, which is
# the half that only shows up under concurrency, which is the whole point
# of this table.
fn up(db: Any) -> Any {
  db.create_collection("chat_lives")
  db.create_index("chat_lives", "idx_key", ["key"], { "unique": true })
}

fn down(db: Any) -> Any {
  db.drop_collection("chat_lives")
}
