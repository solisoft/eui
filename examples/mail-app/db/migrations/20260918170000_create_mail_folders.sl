# The folders an account has, kept between runs.
#
# `LIST` is a round trip and the answer changes about once a year, so
# asking the server on every connect is a round trip spent on a question
# whose answer is already known. Worse than the cost: until it came back
# the rail was empty, so the folders *disappeared* on every reconnect and
# came back a moment later — which is not a thing a list of folders should
# do.
#
# Unique on the account and the mailbox's name as the server spells it,
# which is the name a `SELECT` takes. The label is the same name decoded
# for reading (RFC 3501 §5.1.3) and is stored rather than recomputed only
# because it costs nothing to.
fn up(db: Any) -> Any {
  db.create_collection("mail_folders")
  db.create_index("mail_folders", "idx_account_name", ["account", "name"], { "unique": true })
}

fn down(db: Any) -> Any {
  db.drop_collection("mail_folders")
}
