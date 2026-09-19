# One account's mailbox, as two numbers.
#
# `total` is what the server last said the box holds and `uidvalidity` is
# what says whether the UIDs stored beside it still mean anything: when a
# server changes it, every UID we hold is meaningless and the cache is
# thrown away rather than repaired.
class MailBox < Model
end
