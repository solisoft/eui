# The query the river makes, and the index it was missing.
#
# A room is read as "the messages of this room, from row `n` on". `idx_room_n`
# cannot serve it: a composite index answers an equality on *every* field it
# holds, and `n >= 3946` is a range, not an equality — so the planner fell
# back to a full scan. 32 058 documents read to return 101 of them, about
# 150 ms, every time someone walked into a channel.
#
# A single-field persistent index on `room` is what it can use: the equality
# narrows to that room's four thousand, and the range and the sort run over
# those. `idx_room_n` stays as it is — it is what makes `n` unique within a
# room, which is what stops two sends landing on the same row.

fn up(db: Any) -> Any {
  db.create_index("chat_messages", "idx_room", ["room"], { "type": "persistent" })
}

fn down(db: Any) -> Any {
  db.drop_index("chat_messages", "idx_room")
}
