# Where an attachment's bytes live.
#
# Two collections and not one, because the card and the picture are asked for
# at different moments: every message that scrolled past needs its thumbnail,
# and almost none of them are ever opened. Keeping them apart means a room
# full of screenshots reads a few kilobytes each and not a few megabytes.
#
# `"blob"` is not decoration. `solidb_store_blob` posts to
# `/_api/blob/{db}/{collection}`, and the model layer's auto-create only ever
# makes *document* collections — it never fires for a blob. Without this
# migration the collections quietly do not exist and every attach fails at
# the store, which reads as "the upload did not arrive" and is really a
# missing table.
#
# The names are the `collection` of the two uploaders on `ChatMessage`. A
# blob collection is not derived from a class name the way `chat_messages` is,
# so these two strings and that declaration have to agree by hand.

fn up(db: Any) -> Any {
  db.create_collection("chat_files", "blob")
  db.create_collection("chat_thumbs", "blob")
}

fn down(db: Any) -> Any {
  db.drop_collection("chat_files")
  db.drop_collection("chat_thumbs")
}
