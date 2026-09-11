# A reply in a thread, under the message it answers.
class ChatReply < Model
  validates("parent", { "presence": true })

  scope("under", fn(parent) {
    this.where({ "parent": parent }).order("n", "asc")
  })
end
