# What a link turned out to be.
#
# An unfurl is a fetch of someone else's page, so it is resolved **once**,
# when the message is written, and read from here for ever after. Doing it at
# render time would be a request per card on a scroll — thousands of them,
# against sites that never asked to be scraped by a chat window.
#
# `url` is the key: the same link posted in two rooms is one unfurl.

fn up(db: Any) -> Any {
  db.create_collection("chat_links")
  db.create_index("chat_links", "idx_url", ["url"], { "unique": true })
}

fn down(db: Any) -> Any {
  db.drop_collection("chat_links")
}
