# A mail reader, as an EUI application.
#
#   soli serve examples/mail-app --port 5013
#   eui ws://127.0.0.1:5013/_eui/session/mail
#
# Needs a `soli` built with `--features eui`.
#
# Reading mail asks the client for almost nothing: no network of its own,
# no clipboard beyond what a text field already has. The IMAP connection
# is the *server's*, opened from this process to imap.gmail.com; the
# window only ever sees a tree of boxes and text.
# The one thing this application asks of the machine: hand an https
# address to your browser when *you* activate a link in a letter. There is
# no op that opens an address and no event that reports one (03 §3.5), so
# this buys the application nothing it can use on its own -- it cannot
# open anything, cannot learn whether anything opened, and cannot tell
# that you clicked. Without it a mail's links are dead text.
# And one line said out loud when mail arrives while you are looking at
# something else (02 §5.2). A notification is the one thing a server may
# do while nobody is watching its window, which is exactly why it is a
# capability: without the grant the client shows nothing and says so on
# stderr, and the application goes on working.
# And the one thing writing a letter needs that reading one does not:
# reaching a file on the machine, so a picture can be attached (03 §3.2).
# A picker is three things at once and a node with two of them opens
# nothing, silently (08 §3) -- the `pick` prop, a server handler for
# `file_pick`, and this. The composer's editor draws the prop and the
# handler; the person still answers for the third, and a refusal leaves
# the composer working with nothing to attach.
eui_capabilities("net.open", "notifications", "fs.pick")

router_eui("mail", "mail#mail", "mail#mail_view")
