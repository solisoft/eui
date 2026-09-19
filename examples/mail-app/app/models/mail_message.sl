# One stored message.
#
# There is no validation here and that is deliberate: every field comes
# from `mail_summary`, which has already replaced a missing subject, a
# missing sender and an unreadable body with something a person can read.
# A model that refused a mail for being malformed would lose the one copy
# of it this application has.
class MailMessage < Model
end
