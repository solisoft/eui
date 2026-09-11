# A message in a room.
#
# `n` is the row number, and it is the whole of how the river addresses a
# room: the client asks for rows `first..last` and the server answers with
# the messages whose `n` falls in that range. It is unique per room (the
# migration says so), because two messages in the same place is two things
# drawn on top of each other.
class ChatMessage < Model
  validates("room", { "presence": true })
  validates("who", { "presence": true })

  # The messages of a room, in order. One query, one index.
  scope("in_room", fn(room) {
    this.where({ "room": room }).order("n", "asc")
  })

  # What arrived after row `n` — what a session that has seen up to `n` is
  # missing. This is the query a tick makes, so it wants to stay small.
  scope("after", fn(room, n) {
    this.where("doc.room == @room && doc.n >= @n", { "room": room, "n": n }).order("n", "asc")
  })
end
