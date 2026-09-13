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

  # What someone attached, and the small square the card draws of it.
  #
  # Two fields and not one. Every message that scrolls past needs its
  # thumbnail and almost none are ever opened, so the picture and the
  # postage stamp are separate blobs and the river reads only the stamps.
  # The uploader's own write-time transform cannot do this — it rewrites
  # the *stored original*, and the original is the thing worth keeping.
  #
  # `service` is the whole point of going through an uploader at all: it is
  # `"solidb"` here because Atrium's messages are already there, and it is
  # the one word to change for disk or S3. Nothing else in the application
  # knows where the bytes are.
  #
  # `content_types` matches what the pickers accept
  # (`chat_attach_button` and the two beside it) and it matches
  # **exactly** — `attach_upload` does a `contains` against the type the
  # extension implies, and for text that type carries its charset. A type
  # missing from this list is an upload refused after it arrived, which
  # reads to the person as the file not having arrived at all.
  uploader("file", {
    "multiple": false,
    "service": "solidb",
    "collection": "chat_files",
    "content_types": [
      "image/png", "image/jpeg", "image/gif", "image/webp",
      "application/pdf",
      "text/plain; charset=utf-8", "text/csv; charset=utf-8",
      "text/markdown; charset=utf-8",
      "audio/mp4", "audio/mpeg", "audio/wav", "video/ogg"
    ],
    # The same ceiling the camera and the recorder ask `pick` for. The
    # client refuses past it before a byte is sent; this refuses past it
    # if anything ever gets around the client.
    "max_size": 8388608
  })

  uploader("thumb", {
    "multiple": false,
    "service": "solidb",
    "collection": "chat_thumbs",
    "content_types": ["image/png"],
    "max_size": 262144
  })

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
