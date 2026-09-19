# What a mailbox knows about itself, apart from its messages.
#
# Two numbers and the moment they were true: how many messages the server
# says are in the box, and the `UIDVALIDITY` that says whether the UIDs we
# hold still mean what they meant. They belong to the account rather than
# to any message, so they get a row of their own rather than riding on a
# message that might be deleted.
fn up(db: Any) -> Any {
  db.create_collection("mail_boxes")
  db.create_index("mail_boxes", "idx_account", ["account"], { "unique": true })
}

fn down(db: Any) -> Any {
  db.drop_collection("mail_boxes")
}
