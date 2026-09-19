# The local copy of a mailbox.
#
# One collection. A message is identified by the account it came from and
# its IMAP UID — which is what UIDs are for: stable for the life of a
# mailbox, unlike the sequence numbers the list is fetched by. The pair is
# unique, so a refetch of something already here is an update and never a
# second row.
#
# The index is on that pair rather than on the date, because the two
# questions this application asks are "do I already have UID n" and "give
# me the newest ones", and a UID is monotonic per mailbox: the newest are
# simply the largest, so one index answers both.
fn up(db: Any) -> Any {
  db.create_collection("mail_messages")
  db.create_index("mail_messages", "idx_account_uid", ["account", "uid"], { "unique": true })
}

fn down(db: Any) -> Any {
  db.drop_collection("mail_messages")
}
