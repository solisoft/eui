# 06 — Events

Status: **normative** for the kinds and payloads; implemented by `eui-proto`
(kinds) and `eui-client` (emission).

An event frame is a node, a kind, the atom naming the server-side handler,
and a payload. Nothing in it is trusted until the server has validated it
against the node's schema.

```
Event := node:varint  event:u8  name:varint  payload:Value
```

## 1. Kinds and payloads

| id | Kind | Payload | Coalesced |
|---:|---|---|:-:|
| `0x01` | `click` | `List[Float x, Float y]`, local to the node the event names — the one holding the handler, not the leaf under the pointer | |
| `0x02` | `double_click` | as `click` | |
| `0x03` | `pointer_down` | `List[Float x, Float y, Int button]` | |
| `0x04` | `pointer_up` | as `pointer_down` | |
| `0x05` | `pointer_move` | `List[Float x, Float y]` | per frame |
| `0x06` | `pointer_enter` | `Null` | |
| `0x07` | `pointer_leave` | `Null` | |
| `0x08` | `key_down` | `List[Str key, Int modifiers]` | |
| `0x09` | `key_up` | as `key_down` | |
| `0x0A` | `text_input` | `Str`, the committed text | |
| `0x0B` | `focus` | `Null` | |
| `0x0C` | `blur` | `Null` | |
| `0x0D` | `change` | `Str`, the editable node's whole value | |
| `0x0E` | `submit` | `Null` | |
| `0x0F` | `scroll` | `List[Int x, Int y]`, the new offsets | per frame |
| `0x10` | `resize` | `List[Float w, Float h]` | per frame |
| `0x11` | `context_menu` | as `click` | |
| `0x12` | `drag_start` | as `pointer_down` (§6) | once a gesture |
| `0x13` | `drag_over` | `List[Float x, Float y, Int slot]` (§6) | per frame, **and only when the target or the slot changes** |
| `0x14` | `drop` | `List[Float x, Float y, Int slot]`, `slot = -1` for a cancel (§6) | once a gesture |
| `0x15` | `long_press` | as `click` | on a held contact, §5.1 |
| `0x16` | `window` | `List[Int first, Int last]`, the rows a windowed `list` needs (spec 04 §7.1), inclusive | when the range changes, once a scroll has landed |
| `0x17` | `ended` | `Null`, a sound or a picture reached its end (spec 03 §7, §8) | |
| `0x18` | `time_update` | `List[Int position_ms, Int duration_ms]` | at most 10/s |
| `0x19` | `wake` | `Null`, the node's `wake` interval elapsed (§1.1) | every `wake` ms, at most 10/s |
| `0x1A` | `file_pick` | `List[Int upload, Str name, Int size]`, one per file the person chose (spec 03 §3.2); the bytes follow as `Upload` frames | |
| `0x1B` | `file_save` | `Str`, the name the person chose; the bytes are owed as `Blob` frames (spec 03 §3.2) | |
| `0x1C` | `location` | `List[Float latitude, Float longitude, Float accuracy_m]`, coarse (§1.2) | every `locate` ms, at most 1/s |
| `0x1D` | `nfc_tag` | `List[Str uid, List[List[Str kind, Str payload]]]`, one tag for a scan the person started (spec 03 §3.3) | |
| `0x1E` | `file_drag` | `List[Bool over]`, a file is over a node carrying `drop`, or has left it (spec 03 §3.2) | **only when the node under the file changes** |

Coordinates are logical pixels relative to the node's border box. `button` is
`0` primary, `1` secondary, `2` middle. `modifiers` is a bit set: `1` shift,
`2` control, `4` alt, `8` super. `key` is the key's name as in the W3C UI
Events `KeyboardEvent.key` value (`"Enter"`, `"a"`, `"ArrowLeft"`).

### 1.1 Being woken

Every other event is something that happened. `wake` is the one an
application asks for: a node carrying a **`wake` prop** — an integer of
milliseconds — **and a `wake` handler** is sent one `wake` event every
that many milliseconds, for as long as it carries both. Nothing else
starts or stops it: remove the prop, the handler or the node and the
clock is gone.

It exists because an application that watches something the client cannot
see — a player on another device, a job on a server, a countdown — has no
other way to be given time. Without it such an application can only
refresh when the person touches it.

A client MUST bound this, because it is the one event a server can ask
for without anyone doing anything:

- a period below **100 ms** is raised to 100 ms;
- at most **four** nodes wake at once, in tree order; the rest are
  ignored;
- a window that was not painted for a while owes **one** event, not the
  ones it missed: the next is due a period after the one that fires, not
  after the one that was scheduled;
- a clock already running keeps its phase when the tree is re-rendered.
  Only a node that was not waking, or whose period changed, starts one.

The budget of `10-budgets.md` §1 — zero wakeups at rest — is about a
window nobody asked to wake. A node with a `wake` prop is a window asked
to wake, and it costs what it asked for.

### 1.2 Being placed

The second event an application asks for rather than receives. A node
carrying a **`locate` prop** — an integer of milliseconds — **and a
`location` handler** is told where the machine is on that interval, for as
long as it carries both. Dropping either takes the node out of the next
tree and the radio with it, exactly as a `wake` stops when its prop goes.

Four things must hold, and a client MUST check every one:

1. the `location` capability was granted
   ([`01-transport.md`](01-transport.md) §2.1). Without it the tree is not
   even read for `locate`, and a fix the platform offers is dropped rather
   than kept;
2. the window has the input. Nothing is reported while it does not — an
   application does not get to follow somebody because its window is open
   behind something else;
3. the platform has actually produced a fix. Asking is not knowing, and a
   client MUST NOT invent one;
4. the node's interval has elapsed. The floor is **one second**, whatever
   was asked for: no positioning hardware means anything faster, and the
   cost of asking is a radio rather than a timer.

**The answer is coarse and the client makes it so.** §2.1 grants the power
to read a *coarse* location and there is no second capability for a fine
one, so latitude and longitude are rounded to a thousandth of a degree —
about 110 m at the equator and less everywhere else — and the reported
accuracy is never better than 100 m, whatever the receiver claimed. The
rounding happens in the client, on the side a server cannot argue with. A
client MUST NOT offer a way to ask for more.

The count has a ceiling as `wake` does: a page wants one fix, and a server
that asks for a hundred gets two.

## 2. Emission rules

- A client emits an event only for a node that has a handler for that kind.
  There is no bubbling: the server composed the tree and attached handlers
  where it wanted them. A `click` on a `text` inside a button reaches the
  button because the button's handler is the nearest one on the path from the
  hit node to the root. That walk is the whole dispatch algorithm.
- `pointer_move`, `scroll`, `resize` and `drag_over` are coalesced: at most one
  per frame per node, carrying the latest value. `drag_over` is coalesced
  **twice**: per frame as the others are, and again against the last one sent,
  so a drag that crosses no boundary reports nothing at all. Six hundred
  samples down a list of forty is forty events (§6).
- A gesture that became a drag reports **no `pointer_up` and no `click`**: the
  lift is the `drop`. Without this, putting a card down also activates it.
- `change` fires when an editable node's value settles — on blur, on `Enter`
  in a single-line field, or after 300 ms of no input. `text_input` fires per
  committed insertion and exists for local handlers; a server that subscribes
  to it across a wide-area link has misread the design.
- A `Handler::Local` runs the chunk and emits nothing. A
  `Handler::LocalThenServer` runs the chunk, then emits.

## 3. What the client will not report

- Keystrokes outside a focused editable node, other than to a node that
  explicitly holds a `key_down` handler and has focus. There is no global key
  capture.
- An input method's composition in progress. The client shows the preedit
  in the focused field and reports nothing; the committed text arrives as
  one `text_input`, and a composition abandoned by a blur leaves no trace.
- `Tab`, `Shift+Tab` and `Escape`: they move or drop focus (spec 03 §3) and
  are consumed by the client. So are the scrolling keys — the arrows, page
  keys, `Home` and `End` — outside an editable node: they scroll, and only
  the resulting `scroll` is reported. `Enter` and `Space` on a focused activatable
  node arrive as the `click` they stand for, at the node's centre.
- Pointer position while the window is unfocused or the pointer is outside it.
- Clipboard contents without the `clipboard.read` capability.
- Any path on the filesystem. A `file_pick` reports the file's name and a
  `file_save` the name chosen for it; which directory either came from is
  the person's business, and the client keeps it. Nor is a dismissed dialog
  reported: an application learns that a person opened a dialog and thought
  better of it only if it was told, and it is not told.
- Where the machine is, unless a node asked (§1.2) — and then only as
  coarsely as §1.2 says, only while the window has the input, and never
  more than once a second. A client MUST NOT report a fix to a node that
  did not ask, and MUST NOT keep one when the capability is absent.
- That a scan found nothing. A tag that was read is an event; a person who
  held their phone up and changed their mind is not, for the reason a
  dismissed dialog is not.
- Anything else about the machine beyond the `Viewport` frame.

## 4. Server-side validation

The server validates every event against the node it names: the node exists
in the tree it last sent, it has a handler of that kind naming that atom, and
the payload has the shape in §1. A failure is a protocol error and ends the
session. A local handler's effect is advisory; the server re-derives state
from its own model before anything is trusted.

## 5. Touch

A touch screen reports contacts; this protocol has a pointer and no contact
of any kind. **No event kind in §1 is added for touch, and none is
reserved.** A server cannot tell a finger from a mouse, and must not try:
the same tree works on both because the client resolves the difference
before anything is emitted.

Resolving it is not the window's job either. Whether a stroke belongs to the
node under it or to the view behind it turns on whether that node asked to
hear `pointer_move` — which is a fact about the tree — so the client does
this on the near side of the tree, next to hit-testing.

A conforming client MUST follow **one** contact at a time. A second contact
arriving while one is live is ignored until the first lifts: version 1 has
no gesture that wants two, and a resting palm must not move the view.

The first contact is followed like this:

1. **Down.** The pointer moves to the contact and presses button `0`:
   `pointer_move` then `pointer_down`, exactly as a mouse would.
2. The gesture is then **taken** or **undecided**. It is taken if the node
   the press landed on resolves a `pointer_move` handler, or carries
   `drag_handle` (spec 03 §3.4), or if the press took hold of a scrollbar
   thumb (spec 03 §2) — a slider, a split bar, a drag of any kind. Otherwise
   it is undecided.
3. **Taken**: every move is a `pointer_move` at the contact, coalesced by
   §2 like any other, and the lift is a `pointer_up`. The view does not
   scroll, and no fling follows.
4. **Undecided**: moves report nothing and the pointer stays where it
   landed, so a tap that wobbles still resolves to the node it was aimed
   at. The gesture is decided by whichever comes first:
   - the contact **lifts** — a tap: `pointer_up`, and `click` by the rule
     in §2, since press and release resolve to the same handler;
   - the contact passes the **slop**, a client-chosen distance from where
     it landed which SHOULD be about 8 logical px — a scroll.
5. **Scroll.** The press is given back before the view moves: the node that
   took it receives `pointer_up` and **no `click` follows**, because none
   was meant. The view then moves against the contact, by the whole
   displacement from where it landed — the slop is part of the stroke, not
   swallowed by it — and by each further delta after that, reported as
   `scroll` under §2's coalescing.
6. **Lift after a scroll.** A contact still moving when it leaves the glass
   carries the view on, over a client-chosen decay. A contact that has been
   still for a short time before it lifts does not: it was placed, moved
   and held, and throwing the view then is a bug a person feels as the page
   running away from them.
7. **Cancel.** A platform may take a gesture away — a system edge swipe, a
   call arriving, the application going to the background. Whatever was
   pressed receives `pointer_up`; no `click` follows and no fling.

### 5.1 The held contact

A gesture that is **undecided** is also being held, and the client runs a
timer of **500 ms** from where the contact landed. Whichever of these comes
first decides it:

- the contact passes the slop — a scroll, exactly as step 4 above, and the
  timer is forgotten;
- the contact lifts — a tap, and the timer is forgotten;
- the timer elapses with the contact still inside the slop, and then:
  - if the press resolves a node carrying `drag` (spec 03 §3.4), the gesture
    becomes a **drag**. `drag_start` is emitted, the press is **not** given
    back — no `pointer_up`, no `click` — every later move is a `drag_over`,
    and the lift is the `drop`. A client SHOULD ask the platform for a haptic;
  - else if the press resolves a `long_press` handler, `long_press` is
    emitted, and the press is then given back as a scroll gives it back:
    `pointer_up`, no `click`, because a long press that opened a menu must not
    also activate what it opened from. The gesture ends there;
  - else nothing happens and the contact stays undecided.

`long_press` is not emitted for a mouse. A held button is not a gesture, and
`context_menu` is already what the second button means.

**This adds no event kind and tells no finger from a mouse.** The server
writes `drag` and `accepts` and hears the same three events either way; it is
the *client* that picks the grab suiting the input it has — eight pixels of
travel for a mouse, a handle or half a second for a finger. That is this
section's own principle applied one level further in. A client with a mouse
runs no timer, because nothing is ever undecided for one.

What this costs, plainly: a slow drag begun by a slowly-moving finger on a row
with **no** handle is a scroll. There is no way around it that does not steal
strokes from the list, and the handle is the escape.

Nothing here is particular to one platform. Android delivers contacts
through `MotionEvent` and iOS through `touchesBegan`/`Moved`/`Ended`/
`Cancelled`; both arrive as the four phases above, and the rules that follow
are the client's, not the platform's.

At the end of every gesture the client MUST clear hover, emitting
`pointer_leave` where one is due. A finger leaves nothing under the pointer,
and a node lit on `pointer_enter` would otherwise stay lit with nothing left
to put it out.

`pointer_enter` and `pointer_leave` therefore bracket a tap rather than
describing a resting pointer, and `cursor` (spec 03 §4) means nothing on a
touch screen. An application whose only affordance is hover has no touch
behaviour, and this specification does not invent one for it.

## 6. Dragging

A drag is the one gesture where what the person is doing and what the
application is being told are furthest apart. The hand moves continuously; the
model changes a handful of times. So the client resolves the whole of the hand
— how a press becomes a grab, what is under it, which slot it is in, when a
list should scroll because the hand is at its edge — and reports only the
changes, in the same way it resolves slop, fling, hover and a scrollbar and
reports only what they land on.

**The client owns the hand; the server owns the order.**

### 6.1 The gesture

One drag at a time, because there is one pointer and §5 allows one contact.

1. **Armed.** A `pointer_down` whose path reaches a node carrying `drag`
   (spec 03 §3.4) arms the gesture. Nothing is emitted and nothing is visible
   to the server: an arming press that turns out to be a click MUST be
   indistinguishable from one that never armed.
2. **Grabbed.** The gesture becomes a drag when the pointer leaves the slop —
   the same distance §5 uses, about 8 logical px — or at once if the press
   landed on a `drag_handle`, or after the hold in §5.1, or from the keyboard
   (spec 03 §3). `drag_start` is emitted **once**, to the nearest handler above
   the **source**: the node carrying `drag`.
3. **Over.** While the drag runs the client resolves, each frame, the node
   under the pointer and the slot within it, and emits `drag_over` to the
   nearest handler above that **target** — but only when the pair has changed
   since the last one sent. A drag that crosses no boundary is silent.
4. **Dropped.** The lift emits `drop` **once**, to the target, carrying the
   slot it landed in.
5. **Cancelled.** `Escape`, a cancelled contact (§5 step 7), the window losing
   the input, or the source leaving the tree all end the drag with a `drop`
   carrying **`slot = -1`**. A cancel is an ordinary drop with a sentinel, so a
   server that handles `drop` and nothing else is correct and complete. It is
   reported to the source; when the source is what went missing, to the
   container last reported over, which is still there and is the node that
   would have received the drop. When neither is left the drag ends and nothing
   is emitted — the server took the row away itself, and already knows.

Dispatch is §2's and unchanged — the nearest handler on the path, no bubbling.
What is new is that a *second*, independent walk finds the props. **The prop
says what a node is; the handler says who hears.** A row is draggable and the
list is what hears the drop, and a container with no handler is not a target
however it is marked.

### 6.2 The slot

`slot` is the position the thing in the hand would take, counted **among the
container's draggable items** and not among its children, so a header, a
footer or a divider is skipped and the number indexes the records the server
holds rather than the nodes it sent. For a windowed `list` (spec 04 §7.1) it is
a row index, which is the same quantity for a container whose children are its
rows.

The rule is **the slot whose box contains the pointer**, clamped to the ends —
not the nearest boundary. The difference matters as soon as the server previews
the move by making it: the thing in the hand is then under the pointer, so the
slot does not change again until the pointer genuinely leaves that box, and the
oscillation a midpoint rule produces cannot happen. It also needs no hysteresis
for rows of unequal height.

A client MUST NOT resolve a target inside the source's own subtree. Dropping a
folder into itself is not a move, and a panel that follows the pointer
(spec 04 §5) is never hit-tested at all.

### 6.3 What the client does not do

- **It does not move anything.** The tree changes when the server says so. A
  client that reordered optimistically would have to reconcile two orders, and
  a `MoveChild` (spec 02 §5) costs four bytes.
- **It does not invent a ghost.** What the hand carries is the thing itself:
  the server answers each `drag_over` by making the move, so the item travels
  and the list is its own preview. An application that wants something under
  the cursor as well declares it — an `overlay` with `position: pointer`
  (spec 04 §5), which the client keeps under the hand without laying anything
  out again, revealed by the `drag_start` handler's own local chunk so that it
  is up before the server has answered. A chunk cannot read the pointer and
  does not need to: it reveals, and the client places.
- **It does not announce.** Announcing is prose, prose is content, and content
  is the server's. What the client owes is that focus stays on the moved node,
  that `pos_in_set` and `set_size` (spec 03 §6.1) stay true, and that the node
  is brought into view.
- **It does not decide whether the drop is allowed.** Groups match so the
  client knows which containers to light and which shape to draw. §4 still
  holds: the server re-derives everything.

### 6.4 Scrolling under the hand

A drag held near the edge of a scroller scrolls it, because the alternative is
a list you cannot reach the bottom of without letting go. Within
`min(48 px, 0.2 × the viewport on that axis)` of an edge the client scrolls
towards it, from nothing at the band's inner edge to a client-chosen maximum at
the outer. The fraction matters: a short list must not scroll from its middle.

The scroller is the one §3's scrolling keys would have chosen — the one under
the pointer that can still move that way, else its ancestor that can. Rows move
under a pointer that is standing still, so the client re-resolves the slot
after each step, and §6.1's change rule keeps that to one event per boundary
crossed. One `scroll` event is emitted when the movement stops, as a glide's is
and for the same reason.
