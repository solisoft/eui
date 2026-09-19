# One mailbox of one account, as `LIST` described it.
#
# `name` is the wire form and the only one a command may use; `label` is
# the same name decoded for a person; `kind` is what the server's
# SPECIAL-USE attribute said it is for, which is how Sent is told from
# Drafts without matching on names in six languages.
class MailFolder < Model
end
