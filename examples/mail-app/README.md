# Mail

Your Gmail, read over IMAP, drawn as an EUI application. Raw text, for now.

```bash
soli serve examples/mail-app --port 5013
eui ws://127.0.0.1:5013/_eui/session/mail
```

Sign in with your address and a password. **Gmail** is the default and
wants an app password, not your account password — Google stopped accepting that on port 993, and the only two
credentials it still takes are an app password and XOAUTH2, of which
Soli's `Imap` builtin implements the first.

To get one: turn on 2-Step Verification, then generate sixteen characters
at <https://myaccount.google.com/apppasswords>. If that page answers *"the
setting you are looking for is not available for your account"*, it is one
of these: 2-Step Verification is not on yet; the account is Workspace and
an administrator has switched app passwords off; or the account is under
Advanced Protection, which has no app passwords at all.

**Another IMAP server** is the other chip on that screen: an address, a
password, the incoming host and — if it is not 993 — a port. The sending
host is guessed from the reading one (`imap.` becomes `smtp.`), which is
right for Fastmail, iCloud, Proton's bridge and most of the rest; the
field beside the port is for the servers that do not follow it.

## Accounts

More than one, of either kind. `@` opens the list, the number beside an
account switches to it — from the list itself too, so `2` is one key —
and `+` adds one. `q` signs out of the account on screen and leaves the
others alone.

Everything under the account layer is keyed on the **address** and knows
nothing else, which is why this changed almost nothing below it: the host
to dial is looked up from the address (`MAIL_WHERE`) rather than threaded
down through twenty call sites. Each account keeps its own
`config/messages-<address>.json`, so the mailbox you are not looking at
is not rewritten every time a message is read in the one you are; the
credentials share `config/accounts.json`, sealed the same way the single
account was. A `config/account.json` from an older build is folded in
once, on the first start, and removed.

Switching drops the list, the cursor and the open letter, because they
belong to a different mailbox — the *connection* is kept, since the pool
is keyed by address, so switching back costs no handshake. The two
inboxes are not merged into one stream: that would need per-message
account attribution, and it is not what "which account am I in" wants.

## The keyboard

Everything here has a key. The pointer works; nothing needs it.

| key | |
|---|---|
| `j` `k` | next / previous message — on the list *and* inside a letter |
| `↓` `↑` | move the selection — **on the list**; inside a letter they scroll |
| `PgDn` `PgUp` | move the selection a screenful — on the list; inside a letter they scroll |
| `g` `G` | first / last — and reaching the end fetches fifty older |
| `Enter` `o` | open — and inside a letter, `o` puts the caret on **Browser**, `Enter` opens it |
| `u` `Escape` | back to the list |
| `Space` | mark the row — marked rows are what the next `d` takes |
| `d` / `Delete` | move to the trash — everything marked, or the row under the cursor; recoverable there for thirty days |
| `n` / `Insert` | write a new message |
| `a` `A` | answer the sender / answer everyone on it |
| `f` | forward it |
| `F` | the folders — `↑` `↓` walk them, `Enter` opens, `Escape` comes back |
| `/` | search: typing filters what is loaded, `↑` `↓` walk it, `Enter` opens |
| `c` | put the letter in fields so it can be selected; `Escape` leaves |
| `i` | fetch this message's pictures |
| `t` | the whole letter in one block, selectable across lines, no links |
| `h` | the HTML face of a message — the one with the pictures — and back |
| `p` | fetch what is attached — pictures are shown, the rest gets a link |
| `v` | the pictures one at a time, full size — `←` `→` walk them, `Escape` closes |
| `R` | read every row again — for fields an older build did not store |
| `r` | fetch again |
| `@` | accounts — the number switches, `+` adds one |
| `1`…`9` | switch to that account |
| `q` | sign out of this account; the others stay |
| `?` | the list of keys — works on the sign-in screen too |
| `Tab` | step through the rows; the cursor follows |

Which keys the application claims **changes per screen**, and that is the
mechanism rather than a special case: `keys` is a prop on a node and the
tree is rebuilt every render. On the list an arrow means "the next
message", because that is the only thing there is to move between; inside
a letter it means "more of this letter", so the letter claims neither
arrow and the client scrolls with them for free. While the search bar is
open the root claims the keys that move and the keys that leave — the
vertical arrows, PageUp, PageDown, `Enter`, `Escape` — and every other
key is the field's. The vertical arrows are safe to take because a query
is one line and up and down do nothing in a single-line field; the
horizontal two, which do, are never claimed.

**A filtered list is a list.** It used to be something you could look at
and not something you could use: no way to walk it, no way to open
anything in it, and `Escape` threw the filter away rather than handing it
back. Now the arrows walk what the filter is showing, `Enter` opens the
row under the cursor, and `finding` is deliberately left standing while
the letter is open — the reader is checked before the bar in `mail_view`,
so what you see is the letter, and `u` or `Escape` gives you back the bar,
the query and the filtered list you opened it from. `Enter` still asks the
server for the rest of the mailbox when the filter matches nothing here,
and that door is in the bar as well, because a key that means two things
should say so somewhere.

The root box holds one `key_down` handler and carries `autofocus`, because
there is no global key handler in EUI (06 §5) — a key reaches a handler
only when it, or something under it, has focus. The sign-in screen has no
such handler on purpose: a claim on `Enter` there would withhold the
`submit` from the field under the caret (03 §3.1), and `Enter` is how you
sign in.

## Writing

`n` starts a new message, `a` answers the one under the cursor, `A`
answers everyone on it and `f` forwards it — from the list and from
inside a letter alike, because both have exactly one message in hand.
An answer opens with the caret above the quote; a new message and a
forward open at "To", which is the one thing neither of them knows.

**A draft claims one key, and it has to be one nobody types.** A claim by
an ancestor beats the field under the caret, and the root is every
field's ancestor — so a root claiming `s` for send would take the `s` out
of every word typed into the letter, and `Enter` belongs to the textarea,
where it is a new line. `Escape` is neither. Send is therefore a button,
and the keyboard reaches it the way it reaches any button: `Tab` from the
letter, then `Enter`, which the client turns into the click it stands for
(03 §3).

**Each field is held twice.** `seed` is what the composer draws; the
values beside it are what you have typed and what will be sent. Writing
the typed value back into the node it came from emits a `SetText` on
every settled `change`, the client reseeds the field from it (07 §6), and
the caret lands at the end of your own sentence. Drawing a seed that
never changes while the composer is open means no `SetText` is ever sent
for a field someone is typing in.

**A draft's text is capped at 3400 bytes of quote.** One text node holds
4096 bytes (02 §4) and a `textarea`'s value is one text node — a letter
is split across nodes when it is *read*, and a draft cannot be, because
there is one field and it holds one string. So the quote is cut to whole
lines inside a byte budget and says so where it stops. The letter box
grows with what is typed into it, because a `textarea` does not scroll
to its own caret: its follow is horizontal only, so text below a fixed
box is text nobody can see.

**Send is armed by the press and done on the tick**, for the reason
opening a letter is: SMTP is about a second under the session's frame
lock, and a second in which the window answers nothing is a second in
which the press looked ignored. The press draws the spinner; the next
`wake` sends. It is the one errand that is not repeatable, so it runs
before the others.

**An answer is threaded, not merely prefixed.** A reader threads on
`In-Reply-To` and `References`, so `Re:` alone lands as a new message.
Neither header is among the four fields a list row keeps, so the reply
fetches the message again and reads them out of its source —
`mail_header_of`, which unfolds one header and stops at the blank line.
Carrying them across needed a `headers` key on the mail hash, which
`Mailer` did not have; it does now, guarded against CR and LF on both
halves of the line, because this is the one place an application supplies
both.

**What you write is markdown, and it goes out three ways.** The composer
has one format, not a mode it can be in. Soli renders the other two from
it — `Markdown.to_text` for a reader that draws nothing, `Markdown.to_html`
for one that draws — and the source rides along as `text/markdown`, so a
reader that prefers it has it and the copy in your sent folder is the one
you typed. `multipart/alternative`, ordered least rich to most (RFC 2046
§5.1.4: the last part a reader understands is the one it shows): plain,
markdown, HTML.

`to_text` had to be written for this. `strip_html` on the generated HTML
loses a list's bullets and, worse, **every link's address** — the words of
a link and no way to reach it. The plain face keeps `- `, `1. `, `> `, and
turns a link into `texte <url>`.

`alternatives` had to be added to the mailer for it too: `text_body` and
`html_body` are the only two names mail-builder has, and a third part
means assembling the structure by hand — attachments included, because a
custom body makes the built-in ones inert.

**Reading prefers markdown, then text, then HTML.** That order is the
application's, not the sender's: a message that carries its markdown
source is shown as that source (rendered through `Markdown.to_html`,
which is the direction that exists, and then through the same block
parser); otherwise the text part, which is what a person wrote rather
than what their mail client built around it; and HTML last.

`h` reaches the HTML face whenever there is one, and the bar says which
face you are on — **that is where the pictures are**, so a newsletter is
one key from being a newsletter. Opening a message always starts on the
preferred face.

Seeing a `text/markdown` part at all needed `parts` in the mail parser:
`text_body` and `html_body` answer "the plain one" and "the HTML one",
and neither will ever return a third face. The block parser learned
`h1`–`h4`, `li` and `blockquote` at the same time, because a document
rendered through a parser that only knew paragraphs arrived as a wall of
them.

**The other direction does not exist.** Soli's `Markdown` class and EUI's
markdown builders both go *from* markdown; nothing turns HTML into it. So
reading a mail is still an HTML parser, and routing display through
markdown would mean writing that converter (the parse `mail_html.sl`
already does) and then parsing its output again to get nodes back — two
parsers for one result.

Sending is SMTP with the same address and app password IMAP signed in
with — Gmail takes an app password on both. `MAIL_SMTP_HOST`,
`MAIL_SMTP_PORT` and `MAIL_SMTP_TLS` name a different server, which is
how the send path is tested against something that can be read back.
Plain text only: no attachments, no signature, no drafts folder.

## Attachments

A row says how many paper clips it carries and how big the message is,
and neither costs a round trip: `RFC822.SIZE` and `BODYSTRUCTURE` ride
along in the same `FETCH` as the headers. The clip count is a count of
`"attachment"` in the structure rather than a parse of it — every
attached part carries that disposition, so the number of times the word
appears is the answer, and a filename containing it would add one to a
hint that nothing depends on.

`p` fetches what is attached. The bytes arrive with the message — it was
fetched whole to be read — but they are not kept in session state or in
the store: a hundred messages with their attachments inlined would be a
hundred megabytes of JSON rewritten on every read mark. They are written
once to `public/mail-att/`, and what state keeps is the path. A picture
is then shown as a picture; anything else keeps its name and size and
carries a link, because there is no node that can display a PDF.

The link needs `MAIL_BASE_URL` — where this application answers — because
a session does not tell the server its own address. Without it the list
and the pictures still work and the rest is name and size only. Sending
the bytes over the session instead would be `file_save` and `Blob`
frames (03 §3.2): the event kind is understood, but nothing in Soli's
EUI server emits a `Blob` yet.

**The letter in a browser, and why it is two presses.** A message is
written out as a page beside the attachments and the reader carries a
`Browser` node pointing at it. `o` inside a letter does not open
anything: `net.open` is a **prop**, spent when the person activates the
node that carries it (03 §3.5, 08 §8.1) — there is no op that opens an
address, so a key cannot. What `o` does is put the caret on that node,
with `focus_to`; `Enter` is the activation, and it is the person's. The
flag lasts exactly one event, because `focus_to` is an op and a node
carrying it every frame would hold the caret for ever.

And for that one frame the root **lets `Enter` go**. A focused node is
activated with `Enter` or `Space`, but an ancestor's claim beats the
focused node (03 §3.1) and this root claims `Enter` for "open the
message" — so `o` then `Enter` did nothing at all while `o` then `Space`
worked, which is a distinction nobody should have to learn. The key set
is a prop on a node and the tree is rebuilt every render, so dropping one
key for one frame costs nothing but saying so.

The page is the sender's HTML as it stands — that is the point of opening
it in a browser — so it carries `default-src 'none'`. This application
draws a mail as boxes and text; a browser would *run* it, on this origin,
beside every other message written here. The policy is the difference
between showing you a letter and executing one. It is written once per
message rather than once per frame: the view is the one place that knows
the letter is on screen, and the view runs on every tick.

**When a picture is slow, `EUI_TRACE=1` says where.** There was no trace
on the asset path at all, which is why a viewer that took six seconds
could be measured everywhere except where the time was going: the event,
the frame, the proxy and the server-side resize were all timed and all
fast. The client now prints, for every asset, the bytes and the
milliseconds of each of the three things that happen to it:

```
eui 0.181: asset fc2c94e4 fetched 38114 bytes in 0 ms
eui 0.182: asset fc2c94e4 38114 bytes -> 400x300: decoded in 1 ms, shrunk and packed in 0 ms
```

Fetch, decode, pack. Over loopback all three are noise; what it is for is
a real origin, where a fetch is a TLS connection of its own.

**`p` is two steps, so there is something to show.** It used to read the
whole message and write every file in the one event — a couple of
megabytes over IMAP and a couple of seconds of held frame lock with
nothing on screen to say why. The press now only records what is wanted;
the next tick reads the message, and the ticks after it write the files a
few at a time. So the header line spins and says *lecture du message…*
while there is no count to give, and then `3 / 17` as they land.

What that costs is the base64 of the attachments still to be written,
held in session state between ticks. It shrinks with every batch and is
gone when the last one lands, and `MAIL_ATT_MAX` bounds any single one.

**Pictures are shrunk to what is drawn, because the atlas is one
texture.** The client packs every picture it draws into a single
2048×2048 atlas, on shelves as tall as the tallest picture in them — so
six 800×600 photographs fill it and the seventh onwards are never drawn.
Not an error, not a failed fetch: seventeen were asked for and seventeen
arrived (the snapshot tool prints `assets=17/17` now), and six were
painted. Which six looked random, because it depends on the order they
land in.

So a picture is written at the size it will be shown: a 400-pixel
thumbnail for the contact sheet, and body pictures shrunk to the measure
before they are ever uploaded. `ouvrir` still opens the full file. The
underlying limit is the renderer's and this is the application living
inside it.

**A picture opens into a viewer, not into the browser.** A press on a
thumbnail used to hand the file straight to the browser, which is a lot
of ceremony for "what is this one?" — a window somewhere else, a tab to
close, and the other sixteen left behind in the mail. So a press opens a
darkroom instead: an `overlay` (03 §1, painted after everything else)
holding one picture, `1 / 17`, its name, and the browser a press further
on for when you do want the file itself. `←` `→` walk the set, `j`/`k`
do too, a press on the picture goes forward, a press on the scrim closes,
and so does `Escape`. `v` opens it on the first picture from the
keyboard.

While it is up it owns the keyboard — `MAIL_VIEW_KEYS` replaces
`MAIL_READ_KEYS` on the root, so the arrows page through photographs
rather than scrolling the letter underneath, and everything it does not
name closes it rather than doing two things at once. The thumbnail sends
its *position in the contact sheet* and nothing else: `mail_view_open`
filters the same rows the same way, so index `i` there is picture `i`
here and no identifier has to survive the round trip.

**The first press used to take five seconds, and none of it was here.**
The event and the frame are 17 ms together, the proxy answers in 5, and
shrinking a 4032×3024 photograph server-side is 430 ms. What took the time
was the *asset*: a viewer showing something sharper than the contact sheet
means, by definition, a file the client has never seen, so the first look
at each picture is a download. It really was re-loading the image from the
server — that is what it is for.

So the file it asks for is made as small as it can honestly be. A copy at
`MAIL_VIEW_PX` on the longest edge, as JPEG, written beside the original —
at download time for anything fetched since, on first look for anything
older, and read from disk on every look after that. 1024 is not a taste:
it is `assets::ATLAS_EDGE` in the client, which shrinks anything longer
than that *before* packing it, so a pixel beyond it is a pixel
downloaded, decoded and thrown away. A copy is made for a picture under
that size too when the file is over `MAIL_VIEW_BYTES`, because the
question is bytes on the wire and not only pixels on the sheet. `Ouvrir
dans le navigateur` still hands over the original.

**And the first thing on screen is a picture the client already has.** The
thumbnail is drawn underneath the sharp copy at the same size, so the
viewer opens in the frame the key was pressed in and sharpens when the
file lands — an image whose asset has not arrived paints nothing at all
(`paint.rs`), which is what makes a layer underneath work at all. The
*next* picture is named by a node of no width and no height, which is
enough to fetch it: one ahead, never a set, so `Suivante` costs nothing.

**And a thumbnail is a JPEG.** The trace caught this: a 400×300 PNG of a
photograph is 255 KB — *more than the 800×600 JPEG it was made from*, and
seventeen of them are four megabytes, each on a TLS connection of its own
against a server with one worker. As JPEG they are 20–40 KB, and the whole
contact sheet is 500 KB that decodes and packs in about a millisecond
each. Lossless was never the right answer for a photograph; it was only
the default.

For that first layer to be worth looking at, the thumbnail had to grow —
and how far is the atlas's arithmetic, **including the two pixels of
padding it puts round every picture**. 400 square packs as 402, which goes
into 2048 five times; seventeen of them are four shelves and 1608 of 2048
rows. 512 looks like it divides 2048 four times and does not: 514 goes in
three times, so seventeen took six shelves, the sheet emptied and repacked
itself, and a third of the contact sheet came back blank — the exact fault
the thumbnail exists to cure, committed by the cure. Measured, twice, by
looking at the pixels.

**While the viewer is up, the contact sheet stops naming its pictures.**
Not to save a frame — it is behind a full-screen overlay either way. It is
the atlas again: the sheet's seventeen and the viewer's own sharp picture
do not fit together, so leaving them named means emptying and repacking on
every step. Unnamed, they are simply not packed while the viewer holds the
screen, and they come back when it closes — from the bytes the client
already has, not from the server (`driver.rs: repack_images` packs from
the asset store, and nothing there is ever evicted). The boxes that stand
in for them keep the exact size, so nothing moves underneath.

**Two parts of one message may share a name.** This mail carried
`Terms_of_Service_fr_fr.html` twice; the index is in the filename because
without it the second overwrote the first and both rows pointed at it.

**The whole message is kept when the letter lands.** `mail_load_body`
copied `body`, `html` and `loaded` and dropped the rest — so a message
with a PDF on it was parsed, its attachments listed, and the list thrown
away in the same breath. The reader showed no paper clips for exactly the
messages whose paper clips it had just read.

## The letter

A mail carries a text part and an HTML part, and for anything sent by a
machine the text part is a machine's idea of what the HTML said — every
image reduced to its URL, every link to a second URL beside it. So the
HTML part is read instead, into paragraphs, links and pictures; `t` falls
back to the text part, which is also what a human-written mail has.

Links are the `open` prop (03 §3.5) and `net.open` in `config/routes.sl`:
the client opens the address when **you** activate the node, exactly one
scheme is accepted (`https://`, lower case), and this application is never
told whether anything happened. Bare addresses in a text part are found
and made openable too. A link cannot sit inside a sentence — a text node
is one string with one style — so each one lands on its own line.

**Comments never reach the tokenizer, and the invisible entities are
decoded.** A marketing mail pads its preheader with hundreds of `&shy;`
and `&zwnj;` so a webmail's preview line stops after the first sentence,
and hides the rest of the preamble in an HTML comment. Undecoded, none of
that is invisible: it is nine hundred literal entities at the top of the
letter and a stray `!--` with the comment's prose under it. `mail_uncomment`
runs before the split on `<`, and the zero-width entities decode to
nothing.

**Pictures are not fetched when you open a mail.** Each one is a request
to a server the sender chose, made from your address, the moment you read
it; a good number are one pixel wide and exist only to report that you
did. So a picture draws as its alt text and its host, and `i` fetches
them for the message you are reading and no other.

## Seeing it without an account

```bash
MAIL_DEMO=1 soli serve examples/mail-app --port 5013
```

opens straight into the list with eight sample messages. It exists because
the two screens worth looking at are the two you cannot reach without an
account. Without the variable nothing in `mail_seed` is ever called.

## What it does, and what it costs

One pooled IMAP connection per account. `select()` gives the message count
and the newest `MAIL_LIMIT` (100) are fetched by sequence number, newest
first.

**A batch costs bytes, not round trips.** `fetch_headers_range` asks for a
sender, a subject and a date — all a row draws — for a whole run in one
command: twenty headers are about 4.5 KB and a hundred about 22 KB, both
one wait. It used to be `BODY.PEEK[]` per message, so twenty rows meant
twenty round trips and about a megabyte, which is the only reason the
limit was ever twenty. The letter itself is fetched when you open one, and
capped at 40 000 characters because the whole of session state is diffed
on every event.

A message with no plain-text part is flattened out of its HTML with
`strip_html` rather than shown as markup.

## What a big mailbox costs

Measured on a hundred messages, with a real clock — and the clock is the
first thing that had to be fixed: `DateTime.now()` built its instants as
`timestamp() * 1e9`, so `millisecond()` answered 0 for every one of them
and any duration measured in Soli came out as a whole number of seconds.
It keeps the subsecond now.

| | before | after |
|---|---|---|
| a keypress on the list | 20–28 ms | **2–3 ms** |
| opening an 86 KB HTML letter | 328 ms | **90 ms** |
| nodes in the tree | 733 | 278 |

Every one of those milliseconds is spent under the session's frame lock,
and the render happens **per event** — including each 250 ms tick while
anything is loading — so this is the difference between a list that
answers a key and one that thinks about it.

**A whole mailbox does not fit in session state.** Seven thousand seven
hundred headers held there cost 26–33 ms an event — the tree stays small
(303 nodes, because the list is windowed) but `mail_shown` walks the
whole mailbox twice a render. That is the case SoliDB is for: the
collection holds the mail, the session holds the window, and the cost
stops depending on how much mail there is. `db/migrations` and
`app/models/mail_message.sl` are written and `.env` is one password
short.

**The window is two bands, not one span.** The row under the cursor is
always drawn, whatever the client last asked for — but widening the
window *to* the cursor is what that must not mean: `G` on seven thousand
messages drew all seven thousand and the client laid out sixty-two
thousand nodes for them. A list's children each name their own row, so
they need not be contiguous: the window the client asked for, plus a
couple of rows around the cursor. Same jump, 303 nodes.

**The list is windowed.** A `list` carrying `count` and `item_height` has
rows the tree does not hold (04 §7.1): the client lays out the whole
extent from the count, paints placeholders where rows are missing, and
asks — through the `window` event — for the range it needs. A hundred
messages cost the dozen on screen. The row under the cursor is always
drawn whatever the client last asked for, so `G` does not flash a
placeholder on the way.

The price was the row exit animation, and it is back with a condition: a
row carries `exit` only while it is being deleted. In a windowed list rows
leave the tree on every scroll, and an unconditional exit would animate
the scrolling itself.

## The folders

The rail down the left is the account's mailboxes, from `LIST` — which
answers with the names *and* their SPECIAL-USE attributes, so Sent,
Drafts, Trash, Junk and Archive are known by what the server says they are
for rather than by matching their names. `\Noselect` entries are dropped:
Gmail's `[Gmail]` is a container, not a mailbox, and selecting it is an
error. The order is the inbox, then the folders the server gave a meaning
to, then everything else — not alphabetical, because "Archive, Drafts,
Inbox, Sent, Trash" is alphabetical and nobody thinks of their mail that
way. `[Gmail]/` is stripped from what is drawn: it is a fact about
Gmail's namespace, not a thing to read twelve times down the side of a
window.

**A mailbox name is not text until it is decoded.** IMAP names its
mailboxes in US-ASCII and shifts anything else into a modified BASE64 of
UTF-16 between `&` and `-` (RFC 3501 §5.1.3), so a French account's Sent
folder arrives as `Messages envoy&AOk-s` — which is exactly what the rail
showed until `Imap.mailboxes()` learned to decode it. Every mailbox now
carries both: `name` as the server sent it, because that is what a
`SELECT` must be given, and `label` for a person to read. Nine cases in
`imap.rs`, including the literal `&-`, the `,` that stands in for
BASE64's `/`, and a malformed shift, which is left as it came rather than
decoded into the wrong letter.

**`F` is the way to the folders, and `Shift+Tab` is not.** Tab order is
the client's (03 §3) and a node cannot ask for a place in it — and the
rail sits in front of a hundred activatable message rows, so walking
backwards to it means walking through all of them. A key the application
owns gets there in one press: `F` puts the caret on the folder you are
in, `↑` `↓` walk the rail, `Enter` opens, and every other key hands the
keyboard back to the list, which is what makes it a mode you leave by
using the application rather than by remembering to. The client's focus
follows the caret with a one-frame `focus_to`, because `Enter` is
activated by whatever the *client* thinks is focused — the rail's claim
on that key is a loan, not an interception.

**Tab walks the folders too, and `Enter` opens the one it is on.** That last
half did not work at first and the reason is the one this application
keeps meeting: an ancestor's claim beats the focused node (03 §3.1), and
the root claims `Enter` for "open the message under the cursor" — so
`Enter` on a folder went to the message list and the folder was never
activated. The rail rows carry a `focus` handler, so the server knows
which folder has the caret, and the frame drops `Enter` from its claim for
as long as one does. The focused row is outlined where the selected row is
filled: being where Tab has got to and being the folder you are in are
different things, and Tab is what makes you tell them apart.

**The folder list is stored, because a rail that disappears is not a
rail.** It used to live in a module global — which is per worker — and in
session state, so it was gone on every reconnect, gone on every restart,
and absent in any other worker: the folders vanished and came back a
moment later, which is not a thing a list of folders should do. They are
a collection now, unique on the account and the mailbox's wire name, read
at sign-in and at every account switch. The server is still asked once a
session — `LIST` is how a folder made this morning arrives — but the rail
is never empty while that round trip happens. A `LIST` that answers with
nothing is not an answer: whatever is stored stays.

**The list arrives on a tick, not in the press that needed it.** `LIST` is
a round trip, and a round trip inside an event is a window that does not
answer — so the rail appears a quarter of a second after the list does,
which nobody notices, instead of the sign-in taking a second longer,
which everybody does. It is asked once per address per worker, and once
per session even if the answer is empty.

**A UID is unique inside a mailbox, not inside an account.** Message 412
in the inbox and message 412 in Sent are two messages, so the store's
unique index became (account, box, uid) — a third migration — and every
IMAP call now carries the folder it means. A connection has exactly one
selected mailbox: `mail_box` re-selects when the folder asked for is not
the one it is looking at, which is one round trip rather than a new
connection.

**Switching is a render, not a fetch**, the same bargain account switching
makes: the folder's own store goes on screen and the tick asks the server
what is in it. A folder never opened shows nothing for as long as a
`SELECT` takes, and shows it without holding the frame lock.

**The four screens are exclusive, and that had to be said in one place.**
The keys sheet, the accounts, the folders and the menu were four flags,
each added on its own, and `mail_view` asks for them in a fixed order — so
a screen that raised its own flag without lowering the others simply did
not appear, and the key that raised it looked dead. `@` from the folders
screen was exactly that: the menu's flag went up behind a screen that is
tested first, and nothing moved. `mail_screen(state, which)` now raises
one and lowers the rest, and every door goes through it.

**A narrow window keeps the name of the mailbox and one door.** Seven
actions across the top of a phone are seven things that are nearly too
small to press, and one that is not: the folder you are in. So the
masthead becomes the folder's name, the count, and the account you are
signed in as — which is worth the same space as the word "Menu", because
you can already see that there is a menu: it is the only thing up there.
`@` opens it, and the accounts are one press further in, where there is
room to show them. Behind it
a screen with every door the wide masthead shows, each carrying the key
that does the same thing, so the two ways of driving this application
stay one thing to learn. Choosing any of them closes the menu; the list
of which events are doors is written out rather than inferred, because
"every event closes it" would close it on the tick that arrives while it
is open.

The rail is hidden on a narrow window — a folder list that takes a third
of a phone is a folder list nobody reads a message next to — and there
`F` opens the folders as a **screen** instead, the way `@` opens the
accounts: the same list, taking the room it needs, a press to choose and
`Escape` to leave — and the same keys as the rail, because it is the rail
at another width. `↑` `↓` walk it, `Enter` opens what they are on, and the
row the arrows are on is outlined where the folder you are in is filled.
`Enter` is in the rail's key list for that reason alone: on the rail it
never arrives, because the frame lends it to the focused row, but on the
screen it does — and the key that is about to act on the caret must not be
the key that clears it first. The menu carries a `Folders` door too,
because a phone has no `F` to press.

## Being told

When the poll finds mail — in the folder you are looking at, or in the
inbox while you are somewhere else — the window says so out loud:
`eui_notify(title, body, tag)`, which is 02 §5.2's `Notify` op.

A notification is not part of the document, so it is not part of the view:
there is no node to put it on and no state that "is" one. It is something
an application *does*, once, where something happened — so it is a call in
a handler, and the poll is exactly the handler that found the mail. It
goes to the session whose handler asked for it and to no other.

The tag is the account and the folder. A client showing a notification
whose tag matches one already on screen **replaces** it, so ten arrivals
in an afternoon are one line rather than a wall.

**Told when the mail is there, not when it is known to exist.** A
`UID SEARCH` answers in 130 ms and the headers are another round trip, so
announcing at the moment of finding put the bubble on screen a second
before the message was in the list — and a notification about a message
you cannot see yet is one you cannot act on. The poll remembers what it
found and the fetch that lands it spends the count. Only what a poll
armed is announced: the same code path fills the list at connect, and "a
hundred new messages" is not news, it is a mailbox.

**Every account, one per cycle.** The poll asks about the folder you are
in, the inbox when you are elsewhere in the same account, and — in turn —
the inbox of one *other* account. Every account every cycle would be one
round trip per account inside the session's frame lock; one per cycle is
one round trip, and with two accounts each is asked about every other
minute. What comes back is a notification naming that account and a
number kept beside it, never a fetch: mail of an account you are not in
does not belong in the list you are looking at, and a list that showed it
would be a list that lies. Switching to the account spends the count.
Each account has its own connection, its own credentials and its own
high-water mark — the connection cache is keyed by address — and the
first pass on an account *learns* its mark from that account's store
rather than announcing a hundred messages somebody read last week.

`notifications` is in `eui_capabilities` beside `net.open`, and it is the
one thing a server may do while nobody is looking at its window — which is
exactly why it is a capability rather than a call. Without the grant the
client shows nothing, says so once on stderr, and the application goes on
working.

## The store

The mailbox is kept in SoliDB, one document per message, keyed by the
account and the IMAP UID — which is what UIDs are for: stable for the life
of a mailbox, unlike the sequence numbers a list is fetched by. Two
migrations: `mail_messages` and `mail_boxes`, the second for the two
numbers that belong to the box rather than to any message (`total`,
`uidvalidity`).

**A save writes what changed, not what it was given.** The call sites hand
over the whole mailbox, because that is what the file store wanted: one
write of everything. A hundred documents a save would be a hundred round
trips inside the session's frame lock — so each message carries a
fingerprint of the things that change about it after it arrives (read,
loaded, body length, attachment count), kept per worker, and only the ones
whose fingerprint moved are written. In the common case — a message marked
read, a letter fetched — that is one. Measured moving a hundred messages
out of the old file store: 91 ms for all hundred, under a millisecond
each.

**The file store is still there, one layer down.** `SOLIDB_PASSWORD`
empty, or a database that will not answer, and the same two calls write a
JSON file as before; the application says which it is using in the log,
once per worker. The first connect after the database appears moves the
file's contents in and never looks at it again. A cache is a cache: it may
be absent, it may be stale, and neither is an error.

**What is in `app/services` is there for a reason.** A background job
worker loads models, services, policies, mailers and jobs — and **not**
controllers (`serve/background_jobs.rs`). So anything a job will have to
call lives in `app/services/mail_base.sl` and `app/services/mail_store.sl`
rather than beside the views. That is the groundwork for the thing this
application still needs most: IMAP off the session thread, where a cold
`SELECT` costs 2.6 s of frame lock and reading a message with seventeen
photographs on it cost 8.8.

**De-duplication was quadratic.** `mail_unique` ran a linear search of the
list for every message in it — five thousand comparisons on a hundred —
and it is on the render path twice an event, because the masthead counts
what the list draws. A hash of what has been seen makes it linear: that
one change is most of the 20 ms above.

**A letter is parsed once per render.** The reader needs the blocks for
the letter and the count of pictures needs them too; parsing the HTML
twice was 150 ms of frame lock for a number in the corner. The entity
table is skipped for any block with no `&` in it, which is nearly all of
them.

**A full hand is a page's worth *or the whole mailbox*.** "Fewer than
`MAIL_LIMIT`" used to mean stale, which at twenty was a rare mailbox and
at a hundred is almost every one — so a window reopened a second after it
closed spent a 1200 ms handshake to ask a question it had just asked.

**Fetching is not a screen.** An event handler *is* the round trip, so a
handler that fetched twenty messages would hold the frame for as long as
Gmail took and there would be no frame left to draw a spinner in. So the
press only sets `busy`; the root then carries a `wake` prop, and each tick
fetches the next `MAIL_BATCH` (50) and appends them. The list is on screen
after about a second and fills in behind a spinner in the masthead —
including on a refresh, which overwrites the list from the top down rather
than emptying it first.

**An open window looks again every ninety seconds.** The clock in this
protocol is the `wake` prop and nothing else (06 §1.1), so the tree has
to carry one to be woken — and it used to carry one only while it was
busy. The consequence was quiet and easy to miss: a window left open
never learned about new mail at all. You pressed `r`, or you closed it
and opened it again.

There are two speeds on the same handler now. 250 ms while a batch is
being walked, which ends the moment `busy` clears; and `MAIL_POLL` — 90 s
— when nothing is happening, which arms the same refresh `r` does. In
"new" mode that is a `SELECT` and a `UID SEARCH` above the highest UID
held on a connection that is already open: about 200 ms, and nothing
fetched when there is nothing new. The masthead shows the spinner while
it looks, because it is work and hiding it would be a lie.

**The poll asks one command, and usually stops there.** A refresh asks
three questions — how many messages there are, which UIDs are above the
highest held, and which held ones are still there — and a poll needs only
the middle one: the mailbox is already selected on the kept connection,
and a `UID SEARCH` is evaluated against the mailbox as it is now. Measured
on a real account, the `SELECT` this skips is 182 ms of the 346 ms a poll
used to hold the frame lock for, and the spinner it skips is the one that
flashed every ninety seconds to say "nothing".

**New messages come down in one command.** `fetch_headers_uid` answers one
message and costs one round trip, which was fine for opening a letter and
wrong for catching up: twenty new messages meant twenty commands inside
one event. `UID FETCH` has taken a set since 1996; `fetch_headers_set` is
that, with the set validated rather than interpolated — digits, `,`, `:`
and `*` are the whole grammar, and an argument that could carry a space
could carry a second command.

**The poll waits for a quiet keyboard, and asks as little as it can.**
IMAP happens inside the event that asks for it, so it holds the session's
frame lock for as long as it takes — and a poll that lands between two
keystrokes is felt as the application stopping. It is the one piece of
work here with nobody waiting for it, so it is the one that can always
wait: five seconds of stillness, or the next wake.

A poll that finds nothing used to cost three round trips — `SELECT`, a
search above the highest UID held, and the reconciliation search that
notices deletions. The third is now skipped when `SELECT`'s count matches
the count the store already had and no new UIDs were found: one of those
two would have moved if anything had arrived or left. Nearly every poll
finds nothing, so nearly every poll is now two.

**Switching accounts is a render, not a fetch.** Every store goes stale
in two minutes, so switching used to arm a refresh every time, and a
refresh opens with a handshake under the frame lock: the list was on
screen immediately and the next key was not. The poll comes round on its
own now, so a switch shows what is stored and stops there. `r` insists.

Not every screen polls. A draft does not — a fetch under the session's
frame lock while someone is typing is a stutter — and neither does the
sign-in screen, which has nothing to fetch yet.

**The list survives a restart.** Session state belongs to a *connection*,
so every reconnect used to start from nothing and refetch twenty messages
you already had. The mailbox is therefore kept on disk, in
`config/messages.json`, keyed on the address so signing in as someone else
does not hand you the last person's inbox. A cold start draws the stored
list first and refreshes behind it.

That file is standing in for SoliDB, which is what it should be: the
collection, the model and the migration are written and `.env` is one
password short of running them. `mail_store_load` and `mail_store_save`
are the whole interface, so moving them onto `MailMessage` changes nothing
else.

## Deleting

`Delete` is the same key as `d` and `Insert` the same as `n`, for the hand
that reaches for the block on the right of the keyboard. They are safe to
claim only because of *where* they are claimed: an ancestor's claim beats
the field under the caret (03 §3.1), so a root that held `Delete`
everywhere would take it out of every draft, where it is the character in
front of the caret. They are in the list's and the reader's key sets, and
in neither of the sets a screen with a field uses — a draft claims
`Escape` alone, the search bar `Escape` and `Enter`.

`d` lights the row and returns; the mailbox is told a quarter of a second
later, on the next tick.

The press is the one moment the application must not stop, because it is
the moment someone is looking straight at it. A `UID MOVE` down a kept
connection is about 110 ms, which is under a frame — but the connection is
not always kept, and down a cold one it is a second and a half of a window
that does not answer.

So the row answers first: `danger.subtle` under it, the sender and subject
in `danger.base`, and where the date was, a spinner turning on the
client's own clock (`animation: spin` costs no wake and no round trip).
The tick moves the message, and only then does the row leave — with
`animation: exit` and `motion: scale`, declared on the row's *slot*, which
is the node that actually leaves the tree.

This is the third shape this has had, and the first two are the argument
for it. Deferring used to lose work: a queued errand only runs if the
session survives to be woken again, and **a session ends whenever the
window reconnects** — the row was dropped from the store first, so a
session that ended in between left a message the application had forgotten
and the mailbox still had. Doing it inline never lost anything and froze
the window instead. What makes the third one safe is that nothing is
thrown away until the mailbox agrees: the row is marked, not removed, and
if this session ends before the tick runs the message is still in the
inbox and comes back on the next refresh.

A second `d` on a row already leaving is not a second delete — it moves
the cursor on, so `d d d` burns three. When the row finally goes, the
cursor is pulled back by one if it was below it, so the eye stays on the
message that took its place.

There is no fire, and the vocabulary is the reason: a style record has a
colour, a radius, a transition and a motion, and nothing in the client
draws particles. Ember ground, red text, a turning spinner and a scale-out
is what this language can say, and it says it in the frame the key was
pressed in.

*(`strike` is set on the subject of a row on its way out, and nothing
draws it: `text_decoration` is decoded and validated by the client and
then ignored by the painter — neither underline nor strikethrough is
painted today. The style key is not wrong, the renderer is incomplete.)*

**The trash is asked for, not assumed.** `[Gmail]/Trash` is its name only
on an English account — a French one has `[Gmail]/Corbeille`, and the
hardcoded name simply does not exist, so every delete failed with a
message that named the symptom and hid the cause. Worse, that account also
had a *user* folder called `Trash`, so matching on the name would have
quietly filed mail in the wrong place. `LIST` returns SPECIAL-USE
attributes; the trash is whichever mailbox is flagged `\Trash`, whatever
it is called. One round trip per address per process, then remembered.

A row on its way out carries `animation: exit` with `motion: scale` — a
slide would be clipped by the scroller it lives in — and only a row on its
way out carries it: in a windowed list rows leave the tree on every
scroll, and an exit on all of them would animate the scrolling. They are
keyed for it: unkeyed,
the diff matches children by position, so deleting the fourth row rewrites
the text of everything below and drops the *last* child — the animation
would play on the wrong row. `enter` is deliberately not paired with it:
it fires whenever a node is mounted and the whole list mounts on every
reconnect, so twenty rows slid in from the right every time you came back.

`MAIL_DEBUG=1` makes the application narrate what it does to the server
log — which key was seen, which mailbox it resolved, what Gmail answered.
Off and silent by default.

## Why more workers would not have helped

A session's events are serialised behind its own frame lock
(`serve/eui/mod.rs`), deliberately: two frames of one session must not
race on its state. Raising `SOLI_WORKERS` or `SOLI_WS_WORKERS`
parallelises work *across* sessions and does nothing for one window's
responsiveness. So whatever a handler does, the person watching waits for
it.

That made it worth measuring where the time actually went, and it was not
where it looked:

| | |
|---|---|
| connect + `LOGIN` to Gmail | **~1200 ms** |
| `SELECT` + fetch 5 messages' headers | **~210 ms** |

The work was free; the handshake was everything. Every action used to open
a connection, do microseconds of work and hang up — so a delete, a
mark-as-read, a body fetch and *each batch* of the opening walk each paid
a full second of TLS for nothing, and each of those seconds was a second
in which the window answered no key.

So the connection is kept, one per address, in `MAIL_OPEN`. Two further
things follow from the same measurement:

**`SELECT` is only re-issued where the counts are wanted.** It is a round
trip like any other, and a delete has no use for `exists`. Only the two
callers that need `exists` or `uidvalidity` take `mail_box_info`.

**Mutations address messages by UID, not by sequence number.** Every
mutating verb in IMAP has both forms, and Soli only had the positional
one — so deleting meant `SEARCH UID n` to convert, then `MOVE`: one extra
turn, and turns are all this costs. `uid_move`, `uid_delete`,
`uid_mark_seen`, `uid_mark_unseen` and `uid_copy` were added to the
builtin for this. Deleting went from three turns to one.

Not re-selecting means not noticing when Gmail closes an idle connection,
so `mail_do` speaks first and asks questions afterwards: on failure it
throws the connection away and tries once more on a fresh one. The normal
case costs one turn; the stale case costs a reconnect rather than an error
on the screen.

Measured end to end, one action went from **~1400 ms to ~110 ms**.

## Two things to know before changing it

**A note is a toast, and the pointer highlights nothing.** What just
happened floats in the bottom-right corner over the page and takes itself
away on a `wake` (06 §1.1) — the only clock in the protocol; it used to
sit in the masthead, pushing the count along and staying until the next
thing you did. Rows have no hover ground: the cursor *is* the selection,
and a second highlight following the mouse says "this one" about a row
that no key will act on.

**The cursor is a node, not a colour.** A row's selected state is a child
that arrives and leaves, not a border whose colour changes. In this Soli, a
node whose *only* change between two renders is its style receives no
`SetStyle` — the row keeps the style it mounted with, so the cursor draws
on whichever row it first landed on and stays there. The same node's text
or children change fine, and carry the style with them. `InsertChild` /
`RemoveChild` always land, so the cursor rides those instead. Put the
border colour back when a style-only change is delivered again.

**`length` counts bytes; `substring` counts characters.** They agree on
ASCII and disagree on everything else: `"αγγελία: X".length()` is 17 and
`index_of("X")` is 16, but the string is ten characters. Every index
derived from one and spent on the other lands mid-word — a Greek mail
rendered as `α&#ελία` before this was found. `mail_at` and `mail_len` in
`mail_html.sl` are character-based and nothing in the parser uses
`index_of`. Byte counts are kept only where bytes are meant: the
4096-byte ceiling on one text node.

**One text node holds 4096 bytes.** `mail_body_node` is the single place
a message's text becomes nodes, and it chunks there — deliberately, after
three separate 400s from splitting at the source and missing a case each
time (a long HTML paragraph, an unwrapped text part, a line with no break
in it at all).

**The scroll offset is heard, not assumed.** The server never learns
where a scroller sits on its own (08 §8) but it may set one — `scroll_to`
is the one name in `p` that becomes an op instead of a prop — and it may
subscribe to `scroll`, which reports the new offsets. `j` and `k` carry
the cursor past the fold by keeping that offset, and the list hears the
event so a wheel does not leave the figure stale. Which is why the
masthead and every row carry an explicit pixel height: the arithmetic is
this application's to do.

## Not here yet

- **SoliDB.** The mailbox persists to a file today (see above);
  `db/migrations` and `app/models/mail_message.sl` are ready, and `.env`
  needs the SoliDB password before `soli db:migrate up .` can run. With a
  real store, a refresh can fetch only UIDs above the highest one held
  instead of re-downloading the newest hundred every time.
- Other mailboxes, attachments (parsed and counted, never shown, and
  never sent), a signature, and a drafts folder — a draft lives in
  session state, so closing the window loses it.
