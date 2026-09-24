# 09 — Conformance

Status: **normative**. `cargo run -p xtask -- conform` runs everything below
and fails on the first deviation.

## 1. What conformance means

An implementation of EUI/1 is **conforming** when it passes the vectors in
this document's harness. The vectors are executable, live beside the code
they pin, and are the tie-breaker when prose and behaviour disagree: a
vector that contradicts the prose is a bug in the prose until the vector is
changed, and changing a vector is a protocol change.

Two profiles:

- A **client** MUST pass §1.1 (transport), §2 (wire), §3 (tree),
  §4 (layout), §5 (theme), §6 (bytecode) and §7 (events); §8 (painting)
  applies when it draws, and §11 (scenes) when it draws those.
- A **server** MUST pass §2 and §6 for what it emits, and §9 (diff) if it
  patches rather than re-mounts.

The harness also builds the seven portable crates — `eui-proto`, `eui-tree`,
`eui-theme`, `eui-layout`, `eui-text`, `eui-vm` and `eui-shader` — for
`aarch64-linux-android`, `aarch64-apple-ios` and `wasm32-unknown-unknown`
where those targets' standard libraries are installed, and skips each with a
note where one is not. Three, because they fail differently: the first
catches a `cfg` written for Unix that Android happens to satisfy, the second
one written for Apple that a Mac satisfies and a device does not, and the
third one written for anything with an operating system at all. It then
builds `eui-client` twice more, in the two shapes no ordinary build reaches:
`--target wasm32-unknown-unknown --no-default-features`, where four
capabilities are off and a different transport module is compiled, and
`--cfg no_subprocess`, where a body of code a desktop never sees is turned
on. A `cfg` nobody compiles is a `cfg` nobody checks.

None of that is a vector and nothing conforms by passing it. It is there
because those seven crates carry no `cfg` for any platform and the claim is
worth keeping true: a client for a phone, or for a page, or for anything
else, starts by taking them unchanged.

### 1.1 Transport — `crates/eui-client/tests/driver.rs`, `crates/eui-client/tests/dial.rs`

01 §1 and §2.3, which had no vectors at all until the rule they state was
found to be two rules that contradicted each other.

1. **`wss://` always, `ws://` only to loopback, and only when asked.**
   `plain_text_reaches_loopback_and_only_when_it_is_asked_for` in
   `driver.rs` walks every case: the three addresses §1 names, each with and
   without a port; each of them refused when nobody has asked; each allowed
   when somebody has, **in whatever build this is**; and the exception
   buying nothing for any other origin.
2. **Loopback is a host, not a prefix.** `dial.rs`'s unit tests refuse
   `ws://localhost.evil.example/`, `ws://127.0.0.1.evil.example/` and
   `ws://localhost@evil.example/` — all three of which pass a
   `starts_with`, and one of which is a request to somebody else's server.
   A client that decides by prefix has handed the exception to whoever
   registers the name.
3. **An embedded host vouches for its own session only.** The trust is a
   parameter and not a process flag, so one embedded session cannot turn
   `ws://` on for a network session opened beside it.
4. **An HTTP status is an answer, not a dropped socket.** `dial.rs` pins
   that a refused upgrade ends the tab with the server's reason rather than
   climbing a retry ladder, and that the reason reads as a sentence.
5. **Every frame kind is classified for lazy dialling.** All twelve of §7,
   written out rather than sampled, plus a line that fails the day a
   thirteenth is added — because the failure modes either way are silent: a
   kind wrongly dialling opens a session when a window is dragged, and one
   wrongly not dialling drops a click into nothing.

## 2. Wire format — `crates/eui-proto/tests`

| File | Pins |
|---|---|
| `vectors.rs` | Byte-exact encodings: a 64-byte style record, varints, the counter's Mount batch, a `DefFont`, every op |
| `roundtrip.rs` | Every frame, op, value and record survives encode → decode unchanged |
| `reject.rs` | 64 malformed inputs, each refused with the named error and no allocation past the limit: truncation, non-minimal varints, trailing bytes, unknown enums, undefined `animation` bits, a `motion_kind` with nothing going that way, oversized lists, depth, a font role past the last, a role bound to no face, too many faces on one |
| `size_budget.rs` | The counter's Mount fits 576 B and a click 25 B |
| `alloc.rs` | 08 §5's reservation rule, watched at the allocator: a value list whose header claims `MAX_VALUE_LIST` items and carries none is refused as truncated having reserved under 4 KiB, not the 40 MB its count asked for |
| `reject.rs`, again | **06 §4's payload shapes**, which are not decode failures: every payload there is a well-formed `Value` in a valid frame, and the question is whether it means what its kind says. A `click` carrying a string, a null, one coordinate, three, or a thousand; a fractional button, slot or scroll offset; a payload on a kind that declares `Null`; an NFC tag whose records are not pairs. Plus the two properties the rest rests on: an integer stands in for a float and never the reverse, and **no kind accepts a payload of nonsense** — the vector that stops a catch-all arm turning the whole check into a no-op |
| `manifest.rs` | The manifest record round-trips, its signed bytes are rebuilt exactly, malformed records are refused, and an `icon` makes it a version 2 record while a manifest without one stays byte-for-byte version 1 |

`roundtrip.rs` and `vectors.rs` cover 01 §4.1 and §6 as well: a `Hello` that
offers a session back, a `Welcome` that resumed one, and a transfer in each
direction with each of its three flags, byte for byte. `reject.rs` refuses an
unknown transfer flag, an oversized chunk, an oversized abort reason, an
unknown resume tag, an unknown `resumed` byte, and a `Hello` that ends
before its resume flag.

### 2.1 Font roles — `crates/eui-text/tests/shape.rs`, `crates/eui-client/tests/assets.rs`, `crates/eui-tree/tests/apply.rs`

02 §5.1. `font_family` is the one style byte with an open half, so `2` is a
*role* rather than an unknown tag and `reject.rs` pins that it decodes. The
rest is the behaviour the section promises and the client owes:

1. **A face is read and names itself** — `add_font` answers the family out of
   the face's own tables, and refuses bytes that are not a face at all.
2. **A role draws in the face bound to it**, and rebinding a role throws away
   what was shaped under the old binding. Rebinding a role to the family it
   already holds costs the cache nothing.
3. **Sans and mono are roles**: an application may replace either, and when
   the session ends the client's own faces come back.
4. **A missing face never costs a page.** A role no `DefFont` bound, a role
   whose asset has not arrived, and a role whose bytes would not parse all
   draw in `sans`, and the session stands in all three.
5. **The faces are asked for from the session**, not from the tree: a role's
   hashes are wanted because a `DefFont` bound them, and asked for once.

## 3. Tree — `crates/eui-tree/tests`

Tables never cleared — including by a `Mount`; a batch that fails leaves the
session poisoned until the next `Mount`; every reference (atom, style, chunk,
node, key) validated before anything is placed; `MoveChild` indexes after
removal. Font roles are the one table that is not define-once: a role holds
the faces bound to it, may be bound again, and one past the last is refused
in the session as well as in the decoder.

`apply.rs` also pins the quotas that are session state (02 §6):
`chunk_bytes_have_one_budget_across_the_page_and_its_islands` refuses the
inline chunk that would pass `MAX_CHUNK_TOTAL_BYTES`, counting an island's
bytes against the page's; `atom_budget_is_enforced_before_storing` does the
same for atoms. And that a refused graft is refused **whole**:
`a_refused_graft_leaves_nothing_behind_and_the_resync_lands` sends a subtree
with an id used twice, one past the depth limit, and a bad `InsertChild`,
and requires that none leaves a node live — so the resync's `Mount`, which
carries the same ids, lands rather than being refused as a duplicate.

## 4. Layout — `crates/eui-layout/tests/layout.rs`

Goldens against the fixed-pitch measurer (9 px per character, 22 px lines):
flow, grow, shrink with the automatic minimum, wrap, justify, align, baseline,
percent, stack (stretch on both axes, absolute layers), grid, scroll clamping,
virtualised lists, display none, depth 255 within a 1 MiB stack.

Plus **what a node's kind says about its layout**, which 03 §1's table states
and nothing checked: a `slot` lays out as a column where a `box` with the same
style lays out as a row, and a `slot` that is `display: none` is still gone —
the kind decides the direction and must not put back a node the author took
out of the flow.

## 5. Theme — `crates/eui-theme/tests`

Every default pair in 05 §6 meets its contrast; a seed produces a ramp whose
`on` colour passes against `base`; scale indices out of range are refused;
the easing curve is monotone and pinned at both ends.

## 6. Bytecode — `crates/eui-vm/tests`

The verifier refuses bad jumps, stack underflow and overflow past 64,
unknown opcodes and oversized strings; fuel exhaustion aborts without effect;
every opcode's stack effect is exercised.

For the floats (07 §3), one vector each for the property that makes them
safe to let near a uniform: that a float and an integer are different types
and `to_float` is the only bridge, that a division by zero, the root of a
negative and an overflow each abort the run with nothing left half-done,
that a non-finite literal cannot be smuggled in, and that a float read back
out of local state is the float that was put there — which is what an
integrator between frames rests on. `set_uniform` refuses an index of 8 and
a value that is not a float.

## 7. Events — `crates/eui-client/tests/driver.rs`

Click resolution to the nearest handler, payloads local to the handler's
node, press and release on different targets, focus by pointer and by `Tab`
with wrapping, `Enter` and `Space` as clicks, `Escape`, the ring for keyboard
and server focus only, editing and commit, wheel and scroll offsets, local
handlers with and without a following server event, resync on a bad batch,
transitions on a style change, a back reaching the mounted root and a back
reaching nobody when the root holds no handler for one.

And the two kinds that had payloads, decoders and a server mapping the word
while no client had ever produced one (§2): **`double_click`** — the first
click is a click alone, the second is a click *and* a double in that order,
a third starts a new pair rather than reporting a second double, and a pair
600 ms apart is two clicks; and **`resize`** — a node's first layout says
nothing, a node pushed sideways by an inserted sibling says nothing, a node
whose style changes its width reports the new box once, and painting it
again reports nothing.

**03 §3's pointer shape, across a rebuild.** Three, because the shape is
worked out from the node under the pointer and the pointer is the one thing a
batch does not move. A beam previewed by a local hover handler survives the
batch that puts the server's style back, because the hover it is owed has not
been settled yet and the pointer has not moved. A hand survives the row under
it being **replaced**, where no local style is involved at all and the only
thing that changed is which arena slot the node lives in — and `hovered()`
still names the node rather than whichever sibling was rotated into its
place. And a node that genuinely left in a batch holds the last shape until
the paint that settles the hover, which is where that settle's `leave` and
`enter` belong; between the two, nothing at all goes on the wire, which is
06 §3's clause about a rebuild not being a gesture.

**03 §5.3, pairing**, which had no entry at all while `motion_kind` 6 was a
byte no client acted on: a node arriving under the key one that just left
was wearing starts at its partner's box and its partner's width — the scale
is what proves it, since no other motion produces one — ends where the
layout put it, and never fades, because the element arrives rather than
appears. A `paired` node whose key resolves to nothing is **accepted**, as
§5.3 requires in as many words, and drawn where the layout put it carrying
no transform of its own.

Two more, because the first of those vectors put both of its nodes at the
same origin and so could not tell a **centre** from a **corner**: a pair
whose two boxes differ in origin *and* in size starts with its centre on its
partner's centre, which is not where a corner delta would have put it — off
by half the difference in their sizes, and the two agree only when there is
no difference, which is when there is no pair. The composition itself is
pinned a layer down, in `crates/eui-render/tests/render.rs`: an offset and a
scale taken about the same pivot, the only vector in that file that scales by
anything but one, and the reason the arithmetic above has a right answer to
be wrong about.

And pairing across a page, which is what the section is for: a key that left
inside a released subtree pairs, not only one that was the released node
itself. A server's diff names the page and a release notes its root, so a
client that read departures off that list alone paired a shared element that
*was* the page and nothing else — passing every vector above while the
feature did not work.

Sound reports on one clock and says nothing about the machine: `level` and
`time_update` are emitted on the same tick and neither twice in it; a
`level` reaches only a node that holds a handler for one; a meter that has
not moved sends nothing and a sound that stopped sends one last zero. The
property that carries 08 §8 is pinned a layer down, in
`crates/eui-audio/tests/audio.rs` — the reported peak does not change when
the viewer's master gain does, including when it is zero.

03 §3.5's address is pinned in the same file, and the three conditions are
pinned apart: a tree that merely arrives opens nothing; an activation with
the `net.open` grant yields the address exactly once, and a second read of it
yields nothing; an activation without the grant yields nothing at all; and a
scheme that is not literally `https://` — `file:`, `ms-msdt:`, plain `http:`,
or an authority carrying credentials — never reaches the platform. The same
cases are pinned a layer down on `https_host` itself, where whitespace,
quotes and control characters are refused with them.

02 §5.2's notification is pinned there too, and the same way: a batch with
the `notifications` grant yields its lines exactly once, with control
characters gone and an untitled one dropped; the same batch without the
grant yields nothing and still mounts the tree it carried, because a
notification is not part of the document. The per-batch ceiling is pinned a
layer down, in `crates/eui-proto/tests/roundtrip.rs`, where a fifth
notification in one batch is a decode error and four are not.

### 7.1 Keyboard — `crates/eui-client/tests/keyboard.rs`

§3.1's tab clause: a focused node carrying `typing` receiving `Tab` and
`Shift+Tab` as keys with focus left where it was, a node without the prop
still having both moved out from under it, and `Ctrl+Shift+Tab` moving focus
backwards past a node that carries `typing` and claims every key it can —
the one chord a surface may not take.

§3.1's paste clause: a focused `typing` node hearing nothing from typed text
and **everything from a paste** — reported as `text_input`, newlines and all,
with the client inserting nothing and focus unmoved.

§3.1: `Escape` reaching a handler on the path and leaving focus alone, and
still only blurring when nothing asked for it; `modal` keeping `Tab` inside a
subtree, nesting so the innermost wins, and leaving an unmodal page alone;
`autofocus` placing focus when a surface arrives and *not* reclaiming it on a
later batch; and `keys` letting a node take the arrows while `Enter` stays the
press it stands for, sending it no key it did not name, and leaving a node
without the prop hearing everything.

§3.1 inside a field, which is where the withholding clause earns its three
parts: naming a key cannot make a field undeletable, and the same key with
nothing left to delete reaches the node that asked for it; an arrow with
somewhere to go is the client's and one at the end of the line is not; a
claimed `Enter` reporting the value *before* the key and withholding only the
`submit`, and an unclaimed one still submitting; a node with a bare handler not
swallowing the `Enter` that presses a button inside it; and a printable
character landing in the field whatever the prop says, because text never
arrives as a key.

### 7.2 Accessibility — `crates/eui-client/tests/a11y.rs`

§6.1's declared half: a `role` prop beating the kind it would have been
inferred as; `checked` in all three of its states; a `label` overriding the
text inside; a disabled node keeping its role and accepting neither action;
a container role keeping the children a leaf role would swallow; `set_size`
carrying a count virtualisation left out of the tree; a slider's value and
range, including a present zero; an unknown role name falling back rather
than failing; a live region's urgency; and every role discriminant surviving
the `to_u8`/`from_u8` round trip the worker boundary makes of it.

Rule 4: a field naming an option that is its *sibling* and not its child —
which is where a combo box's panel is, and is the whole reason a relation
exists here at all — and a name pointing at nothing being dropped without
refusing the batch, because a panel that has shut leaves exactly that.

The kind-mapping default of §6 is pinned separately, in `driver.rs`, so that
a tree declaring nothing is provably exposed as it was before §6.1 existed.

### 7.3 Manifest — `crates/eui-client/tests/manifest.rs`

Signature, protocol range, trust on first use, refusal of a changed key,
acceptance of a rotation the pinned key signed, and garbage.

### 7.4 Files — `crates/eui-client/tests/files.rs`

Spec 03 §3.2 and 01 §6, twenty-one vectors. A click on a node carrying `pick`
asking the window for a dialog with the accept list, multiplicity and
ceiling the tree declared; **nothing opening** without the capability, and
nothing opening for a tree that merely arrived; what was picked becoming one
`file_pick` naming the file and not its path, then chunks that fit a frame;
a file past the ceiling announced and aborted, and accepting no bytes after
it; a dismissed dialog reported to nobody and its token spent; a save
becoming a `file_save` and the server's `Blob` frames becoming writes for
the window; a `Blob` for a save nobody asked for **ending the session**; and
a `Blob` out of order losing the save without ending it. The last vector
runs the two gestures against the reference server over a real socket: the
bytes of a picked file reach it, and the bytes it owes a save come back.

### 7.5 Resuming — `crates/eui-client/tests/resume.rs`

Spec 01 §4.1, six vectors. A first `Hello` offering nothing and a later one
offering the session and the last batch applied; a `Welcome` that did not
resume taking the tree, the tables and the acked sequence with it; a
`resumed` naming a session the client never offered ending it; a batch
already applied being acked again and **not applied again**. The last drops
a real socket mid-session and opens another: the tree is still standing, the
count is where it was, and the session goes on.

**The server's half** — `lang/src/serve/eui/mod.rs`, five vectors, since
2026-09-20. A resumed session is owed the batches after the one it
acknowledged and not the tree. A handle alone opens nothing: the same bytes
with another cookie, or naming another component, or a handle nobody minted,
all resume nothing — §4.1 makes the handle a bearer, and a bearer that needs
no other evidence is sixteen bytes standing between a reader and somebody
else's data. A session whose socket is still attached is not picked up, or
two live sockets would both be told they own its tree. A client that fell
further behind than the replay reaches is **refused rather than half-filled**:
told it was resumed and handed a tree with a hole in it, nothing would say so.
And an `Ack` drops what it names, or the buffer would hold the last
sixty-four batches of every session that ever ran.

### 7.6 Touch — `crates/eui-client/tests/touch.rs`

06 §5: a tap reaching what it landed on, and still reaching it after five
pixels of tremor; a stroke past the slop scrolling the view and never
clicking the row it began on; the press given back before the view moves; a
node that asked for moves taking the whole stroke while the view under it
stays put; a second contact ignored while the first is live; a cancelled
gesture releasing without a click and forgetting the contact; a finger still
moving as it leaves carrying the view on, and a finger that stopped first
not doing so; and a window that loses the input forgetting the finger that
was on it, so the next one is a gesture and not a second contact.

The tests are the driver's, not the window's — a contact becomes the pointer
before any platform is involved — so they run everywhere and want no touch
screen.

06 §5.1 is here too: a contact held past the timer on a `drag` becoming a drag
and never a tap, one held on a `long_press` handler reporting it and giving the
press back without a click, one that wandered first being a scroll with no
timer left to fire, and a held contact on a row carrying neither doing nothing
at all. The hold is a clock the test names, not a sleep.

### 7.7 Dragging — `crates/eui-client/tests/drag.rs`

06 §6: an arming press that becomes a click being indistinguishable from one
that never armed; the slop turning the press into a drag and taking the click
back; a handle grabbing without it; `drag_over` arriving once a boundary
crossed and not once a sample; the slot being the box the pointer is in, so a
row moved under the hand does not oscillate; a `MoveChild` arriving mid-drag
and the gesture still naming the same row; a target inside the source
resolving nothing; `Escape`, a lost window and a vanished source each ending
it with `slot = -1`; and a drag at the foot of a list scrolling it and
reporting one `scroll` rather than one a frame.

### 7.8 Arriving and leaving — `crates/eui-client/tests/driver.rs`

03 §5's two lifecycles, which are exactly the kind of thing two clients would
diverge on: `animation` read as a bit set, so `enter | exit` still enters; an
entrance that names a direction arriving from it rather than fading; a
released page painting on after the tree has let it go, with the way it goes
being the mirror of the way its replacement came, along the accelerate curve;
one kept painting at a time; and the frame after it is over holding one page,
with the driver at rest and no frame owed.

06 §5's edge clause — `crates/eui-client/tests/touch.rs`: a stroke inwards
from the leading edge carrying the page and reporting a `back` when it is let
go past halfway; the press given back with no `click`; a stroke let go early
springing back and reporting nothing at all; a stroke along the edge still
scrolling; and a tap inside the slop still being the tap it was aimed at.

### 7.9 Adopting a tree — `crates/eui-client/tests/adopt.rs`

Spec 01 §2.6, seven vectors. A client holding a tree that came over §2.4
offers **the tree** and not the sixteen zero bytes that body's `Welcome`
carried; one holding no tree offers nothing. `Adopted` keeps the tree, sends
nothing back, and leaves a real session behind — so the socket after it
resumes rather than offering the tree twice. `Fresh` tears the tree down
**before** the mount that follows, which is the vector that matters: a fresh
mount numbers from 1 again, so a client that kept its acked sequence would
drop it as one already applied and show an empty window with nothing in the
log. `Adopted` to a client that offered nothing is a protocol error. And the
two tag bytes are checked for what they are — extension points on frames that
cannot grow a field — by giving `start` a fourth value and requiring
`UnknownTag`.

### 7.10 Islands — `crates/eui-tree/tests/apply.rs`, `crates/eui-client/tests/islands.rs`

Spec 01 §2.7, in two halves. **The tree's half is written**, in
`eui-tree/tests/apply.rs`, and it is the half the whole design turns on: an
island has its own node ids *and* its own four interned tables, so a page and
an island may both define atom 1, style 1 and node 1 and both live — which a
shared table could not do, since a `DefineOnce` answers a second write to a
live id with `Redefined`. An island naming a node of the page reaches
nothing, because the id index is keyed by owner and there is no such node in
its space: §2.7's "no id it sends can name a node it did not create",
enforced by the shape of the map rather than by a check that could be
forgotten. An island's `Mount` replaces that island's content and leaves the
page's root, the page's nodes and every other island alone; the node's own
children show until it speaks. A ninth island opens nothing, and a batch for
an owner nobody handed out is refused rather than resolved against the page's
tables — which is the failure that would otherwise be silent, since the
island's ops would land on the page.

**The client's half is written too**, in `eui-client/tests/islands.rs`, and
the ones that matter are the refusals. An `island` naming another origin is
refused — `https://`, `wss://`, a bare relative path, a backslash UNC, and
`//host/path`, which is the one worth naming: a protocol-relative URL is a
different origin *and* starts with a slash, so the obvious spelling of the
check lets it through. Refused in the listing **and** in the opening, so a
caller that asks directly is refused too.

A ninth island on one page opens nothing and leaves the
node as it was rendered (10 §1). An island whose session cannot be opened, or
which ends, leaves the page standing and its own children showing — the vector
that says a live part can never take a still page with it. Two islands naming
one path share one session; two naming one component with different queries do
not. And an event raised inside an island carries that island's node ids and
goes to that island's socket, never the page's — the boundary being the node
carrying the prop, which belongs to the page while everything below it does
not. And a page with an island still draws, lays out and answers a pointer as
**one** tree: the layout engine and the painter never learn islands exist.

The vector that found a real bug is the event one. An island's click must not
join the page's outbound, and the failure if it does is quiet: an island's
node 1 and the page's node 1 are different nodes, so the page's server gets a
well-formed event naming a node it created for something else — which its own
06 §4 validation may accept, since the id exists and may even carry a handler
of that kind. Writing it turned up that the *layout's* style cache was keyed
by style id alone, so an island's style 1 was served the page's: a box asking
for 80×20 came out 400×0, and nothing said a word. Both caches are keyed by
`(owner, id)` now, and `Layout::style_for` takes the owner.

The worker's pipe carries islands too, and `replies_round_trip` in
`worker.rs` pins it: a reply's `island_outbound` survives the encode with its
owners, in a field of its own rather than tagged inside `outbound` — a
separate field has to be read to be sent, and so cannot become the page's by
being forgotten.

And §2.7's `MAY`: an island nine hundred pixels down a three-hundred-pixel
window is not offered at all, and is offered the moment the page above it
shrinks — the same thing a scroll does to where a node sits.

### 7.11 Installing — `crates/eui-client/tests/install.rs`

A signed manifest with an `icon` is verified, its icon fetched by hash and
written at the size the platform's format declares, and a launcher entry
written and then removed exactly. `install.rs`'s own unit tests pin what an
`app_id` may become as a file name, and that a path the record names but
this client never writes to is left alone by an uninstall.

## 8. Painting — `crates/eui-render/tests/render.rs`

One quad per box and per glyph; scissor runs for scroll containers; device
snapping at 2×; shadows; canvas paths; the backdrop a `blur` asks for — that
a frame without one asks for none, that the region is the blurred rects
grown by three standard deviations and no more, and that two radii are two
chains; and, where a GPU adapter exists, pixels read back from an off-screen
target for the clear colour, a filled box, a clipped scroll, a canvas line,
and a frosted pane carrying each half of a seam into the other — both when
it covers the whole frame and when it covers a part of it, which is what
pins the backdrop's origin.

Plus 03 §1's one word about a kind that the painter has to obey: a `sizer`
carrying a background, a border and a shadow draws **none** of them, and its
child still draws. The same tree with a `box` in that place draws more. The
word in the table is the whole specification of the kind, and it was the one
thing this file did not do.

## 9. Diff — `lang/src/serve/eui/diff.rs` (feature `eui`)

Keyed rows move rather than rebuild; mixed keyed and unkeyed children match
by position and key; a changed cell is one `SetText`.

## 10. End to end — `crates/eui-client/tests/soli_e2e.rs`

With `EUI_SOLI_BIN` set, the client drives a real Soli server: the counter's
local-first `+`, the todo's keyed rows and fetched avatar, ten thousand rows
sorted by moves, and the gallery's select, slider, pickers, data grid and
charts. The tag field is there too, because it is where §3.1's three tiers
meet: `Enter` arrives as a `submit` and the field empties itself, and the
same `Backspace` deletes a character on one press and a whole tag on the next.

And 03 §5.3's shared element, which needs a server to be worth anything: the
gallery's customers section is a stack narrow, the account's disc is the one
node on both of its pages, and pushing into a row gives that disc a transform
carrying its partner's width. Nothing else produces a scale, and the disc is
not the node the change named — the server swapped the page.

## 11. Scenes — `crates/eui-shader/tests`, `crates/eui-client/tests/mesh.rs`, `crates/eui-render/tests/render.rs`

**A scene's pixels are not a conformance surface**, and this section is short
because of it. Two conforming clients may draw the same scene differently:
`sin`, the filtering, the rasteriser's fill rule and the point of sRGB
conversion all belong to the adapter. A server that depends on a scene's exact
pixels depends on something this protocol does not promise. Saying that
plainly is better than widening §8 and hoping.

What is pinned instead:

1. **The shader verifier's verdicts**, one vector per rule of 11 §2 — an
   unbounded loop, a bound read from a uniform, a step of zero, a counter the
   body also moves, nested loops over the step budget, a compute stage, a
   storage buffer, a texture, a barrier, a binding outside group 0, a uniform
   block of the server's own shape, an entry point by another name, a fragment
   result that is not `@location(0) vec4<f32>`, a module over 64 KiB. These
   decide on a machine with no GPU, which is what makes them vectors and not
   hopes; a fuzz target sits beside them (08 §5).
2. **The mesh decoder's**, likewise: a container that is not one, an undefined
   flag, a count over the cap, indices that are not whole triangles, a body
   that is not the length the header claims, a coordinate that is not finite,
   and **an index past the end of the vertices** — the bound no driver checks.
3. **The structure of the frame.** A `scene` node produces exactly one
   textured quad, at its box, in a run that names its target; a scene that is
   not `playing` adds no pass to the frame after the one that drew it; a frame
   whose only change is the clock uploads no instances; the target carries no
   `COPY_SRC`; a session that was not granted `scene` fetches no module and
   builds no scene for a node that names one — and **does** draw one that
   names none, whose mesh it fetches either way.
4. **Three tolerant pixel vectors, and three only**: a flat triangle from the
   client's own shader, checked to ±2/255; depth order, which is a boolean and
   so insensitive to precision; and the fallback a refused module draws. A
   fourth would be a promise this protocol does not make.
