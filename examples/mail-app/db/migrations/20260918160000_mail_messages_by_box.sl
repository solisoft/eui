# A UID is unique inside a mailbox, not inside an account.
#
# The first index said (account, uid), which is true of an application
# that only ever reads INBOX and false the moment there is a folder list:
# message 412 in the inbox and message 412 in Sent are two messages, and
# the old index would have refused the second one. So the pair becomes a
# triple, and the box is part of what identifies a stored message.
fn up(db: Any) -> Any {
  db.drop_index("mail_messages", "idx_account_uid")
  db.create_index("mail_messages", "idx_account_box_uid", ["account", "box", "uid"], { "unique": true })
}

fn down(db: Any) -> Any {
  db.drop_index("mail_messages", "idx_account_box_uid")
  db.create_index("mail_messages", "idx_account_uid", ["account", "uid"], { "unique": true })
}
