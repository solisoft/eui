# Writing views

> Built, and running: `examples/demo-app` is served by a `soli` compiled
> with `--features eui`, and `crates/eui-client/tests/soli_e2e.rs` clicks its
> button through the real socket.

An EUI component is a LiveView component with a different render. Same
registration, same handler contract, same registry and worker pool; only the
view — a function of state returning a **node tree as plain data** — and the
wire differ.

```soli
# config/routes.sl
router_eui("counter", "live#counter", "live#counter_view")
```

```soli
# app/controllers/live_controller.sl

# The handler: exactly a LiveView handler.
def counter(event_data)
  event = event_data["event"]
  count = event_data["state"]["count"] ?? 0
  if event == "increment"
    {"count": count + 1}
  elsif event == "decrement"
    {"count": count - 1}
  else
    {"count": count}
  end
end

# The view: state in, tree out. Nothing here is native.
def counter_view(state)
  count = state["count"] ?? 0
  column({"pad": 6, "gap": 4, "align": "start", "bg": "surface.base"}, [
    text("Counter", {"size": 4, "weight": "semibold"}),
    text(count.to_s, {"size": 7, "weight": "bold"}),
    row({"gap": 2}, [
      button("−", "decrement"),
      button("+", "increment")
    ]),
    text("Every click is a round trip.", {"fg": "text.muted", "size": 1})
  ])
end
```

`column`, `row`, `text`, `button` and the rest are ordinary Soli functions in
`app/controllers/eui_builders.sl` and its four companions — all 491 of them
are listed, with their signatures, in [Components](/docs/components). Each returns a hash:

```
{"k": "box", "s": {style}, "t": text, "c": [children], "on": {"click": "increment"}, "key": ..., "p": {props}}
```

The server turns that into nodes, interns every atom and every distinct
style once per session, diffs against the tree it last sent, and encodes the
patch. A view author never sees a `StyleRecord` and never sees a byte.

## There is no CSS

Style is a hash of the spec's own vocabulary — `gap`, `pad`, `bg`, `size`,
`weight`, `radius`, `width`, `align` — and colours are **roles**, not
literals: `"accent.base"`, `"text.muted"`. The client resolves those against
the viewer's mode, so the same view is correct in dark mode without the
server knowing. A literal `"#RRGGBB"` is allowed for a brand mark or a data
series, and wrong for a surface.

You cannot write a selector, because there is nothing to select against. If
two places should look alike, they call the same function.

## Where a handler runs

`"on": {"click": "increment"}` names a **server** event: a round trip, the
handler runs, the view re-renders, the diff comes back.

A handler can also run **locally first**. The counter's `+`:

```soli
local_button("+", "state.count += 1; value.text = str(state.count)", "increment")
```

That string is a small statement language — assignments to `state.x`, to a
keyed node's `.text` or `.style`, arithmetic and comparisons, `if … else`,
`emit("name")`, and `self` for the node carrying the handler. Soli compiles
it to a chunk of the bytecode in `spec/07`, delivers it once per session,
and the client verifies it before its first run and executes it with a fuel
budget. It reads the root node's props (`with_state({"count": count}, ...)`
puts them there), rewrites the node keyed `"value"`, and only *then* sends
`increment` to the server — whose next batch confirms or corrects.

Every button in the catalogue uses the same mechanism for its states:

```soli
"pointer_enter": {"local": "self.style = @hover", "styles": {"hover": hover}}
```

The `styles` map declares the records a handler may point at; the client
switches the node between session styles it already holds, so hover and
press cost no network and no allocation. An assembly-list form of `local`
also exists for tooling.

The rule stays: a local handler's effect is advisory, and **authorisation is
never local**. A `local { … }` source syntax over this is not built yet; the
assembly list is the interface today.

## Images

```soli
avatar("public/images/avatar.png", 32)
image("public/images/chart.png", 320, 200)
```

The path is a file in the application. The server hashes it and sends the
hash; the client fetches it once from `/_eui/asset/<hash>`, verifies the
bytes against the name, and caches it for good. Without an explicit size the
image takes its own. PNG, JPEG or WebP, told apart by their first bytes.

## Lists

Give repeated children a key and re-sorting becomes moves rather than
rebuilds:

```soli
list({"height": 400}, 20, invoices.map(fn(inv) {
  keyed(inv["id"], row({"gap": 2}, [
    text(inv["reference"], {}),
    text(inv["client"], {}),
    text(inv["total"].to_s, {})
  ]))
}))
```

A `list` with an item height is virtualised: the client lays out only the
rows it can see, so ten thousand rows cost about what fifty do.

## What the diff does

Matched nodes keep their ids, so the client edits its arena in place. Keyed
children are matched by key and reordered with `MoveChild`; positional
children are matched by index. A node whose kind changed is replaced whole.
The end-to-end test asserts that three clicks leave the tree at the same nine
nodes with the same ids.

A render that changed nothing sends nothing: no ops, no batch, no frame. That
matters more than it sounds, because applying a batch — **any** batch, empty
included — puts back every style a local handler had previewed and relights
whatever the pointer is over. A page that sent an empty batch on a timer used
to restyle itself once a tick forever, which reads as a flicker and is one.

## A page with a clock

A node carrying `wake` (`06 §1.1`) makes the client send an event on a timer,
and **the view runs on every one of them**. There is no way to tell the
server "nothing changed, skip the render": the handler runs, the view runs,
the tree is built, and only then does the diff find there was nothing to say.
So on a page that wakes, the cost of building the view is paid several times
a second for as long as the window is open, and three rules follow. (A render
the *server* asked for — `eui_wake`, below — is the same render and obeys the
same three rules. What it saves is the ones nobody needed.)

**Return the same object for a subtree that has not changed.** A *keyed* node
whose hash is the very same value the last render returned is kept by the
encoder and never converted again — not its props, not its children. That is
what makes an idle tick free. Cache built nodes in a module global and hand
the cached one back:

```soli
CHAT_RIVERS = {}

def chat_river(state, lay)
  key = [room, str(width), str(generation), str(state["me"])].join("|")
  kept = CHAT_RIVERS[key]
  return kept unless kept.nil?

  built = chat_build_river(state, lay)
  CHAT_RIVERS = {}          # a few windows, not the whole room
  CHAT_RIVERS[key] = built
  built
end
```

The node must be `keyed` for this to work at all, and the saving is largest
where the node is expensive: a virtualised list carries one height per row,
and four thousand of those converted to wire values twice a second is most of
a core for a screen where nothing is happening.

**Put everything the subtree is drawn from in the key** — the viewer
included. These caches are process globals and every session shares them, so
a row that renders one way for the person who reacted to it and another way
for everyone else needs `me` in its key, or two windows will be handed each
other's screens.

**Keep anything that changes every render behind a toggle.** A dev bar
reporting the last render's cost changes on every render by definition, so
leaving it up makes every tick a tick with ops in it — the very thing it
exists to help you see. Atrium puts it behind the `⋯` in its header, and on
`F2` as well. Two things about that bar are worth knowing before you go
looking for one that is not there: `eui_stats()` is **empty outside
`--dev`**, so `dev_bar` draws nothing on a production server; and a key
reaches nothing at all while nothing is focused (`driver.rs`, `fn key`
returns early on `self.focused`), so a shortcut wants an `autofocus`
somewhere or a control you can click.

**A view that owns its own caret must say `typing`, or a phone will not
raise a keyboard for it.** An editor, a pattern grid or a game is built from
a box and a `key_down` — spec 03 §3 asks for that, because a view wanting
the gutter, the highlighting and the selection to be one thing cannot get it
from `input`. On a desktop that works and nothing is missing. On a phone the
soft keyboard is raised by the same call that welcomes a desktop input
method, and that call is made only for the focused node that takes typing:
`input`, `textarea`, or a node carrying `typing` (03 §3.1). Without the prop
the view can be scrolled, read and tapped, and never written to — and
nothing anywhere reports it, because the keys the handler is waiting for are
simply never pressed. It is deliberately not inferred from holding a
`key_down`: a page listening for one shortcut at its root would otherwise
raise the keyboard over whatever the person was reading, with no way to
decline. Nothing else changes for such a node — the client owns no caret
here and sends no `text_input`; iOS delivers typing as ordinary key events,
so what arrives is the `key_down` the view already handles.

## A local handler costs one chunk per key

A chunk's `self` is compiled to the **key of the node it is on**, and the
compile is cached on the triple (source, key, declared styles). So a keyed
node carrying a local handler interns one chunk *per key* even when every one
of them has byte-identical source. The table holds 4 095, and a session that
overflows it ends:

> this session has interned more than 4095 chunks; a key or a style derived
> from data grows the table with every new value

Which means: **do not put a local handler on the rows of a long list.** A
hover on four thousand messages is four thousand chunks and the window dies
about seven hundred rows into a scroll. Put the handler on something there is
a bounded number of — the list, the toolbar, the shell — or do without.

Two things soften it. An event dispatches to the nearest **ancestor** that
handles it, so one handler on a row's container answers for everything inside
the row; and `fg` is inherited by any child that sets none, so one style
change on a container can light or hide a whole group without naming any of
it. Between them, a great deal of per-row behaviour can be written once.

Split your counters, too. One that moves on *anything* is what a tick
compares to know whether to redraw; a cache keyed on it never hits. Keep a
second one that moves only when the drawn thing changes, and key the caches
on that.

## State shared between sessions

One window learns what another did in one of two ways, and an application
that shares state wants both.

**`eui_wake(component)`** renders every *other* live session of a component,
now — the session that calls it is left out, being already mid-render. It is a
Soli builtin (feature `eui`), it returns how many windows were told, and it
belongs where the change is written rather than on a timer:

```soli
def chat_bump
  # … move the counter …
  eui_wake("chat")
end
```

Nothing in the protocol ever forbade this: the session is a WebSocket, `Batch`
is S→C (spec 01 §3), and the client applies whatever arrives. What each woken
session then runs is the ordinary handler and view — the event is the one its
own `wake` handler already answers (`eui_wake("chat")` posts `tick`; pass a
second argument to post something else), so a view needs no special case for
having been woken from outside.

**A `wake` on the view** is still what watches the other half: the things that
stop being true without anyone doing anything — a presence that decays, an "is
typing" that expires, a countdown. Nobody moves a counter when a timestamp
goes stale, so nobody can be woken for it. Keep the clock for those, and let
the period suit what it is watching rather than the message you no longer wait
for.

The counter in a module global stays under both: it is what a woken handler
compares to know whether anything actually changed, and what makes a wake that
changed nothing cost an empty diff instead of a redraw.

The trap is that **a module global belongs to one thread.** The server hashes
each session onto one of its realtime workers, so with more than one of those
two windows can land on different threads and hold entirely different copies
of what you thought was shared state — intermittently, depending on the hash,
which is the worst way for something to not work. Serve an application that
shares state between sessions with one realtime worker:

```sh
SOLI_WS_WORKERS=1 soli serve myapp --port 5011
```

An application that needs this properly — several servers, or a pool that has
to stay wide — keeps the shared thing in `Cache` (SoliKV) and pays a round
trip per event for it. A global is the demo's bargain, and it is worth
knowing it is one.

## Telling somebody something when they are not looking

`eui_notify(title, body, opts)` raises a real notification on the machine the
window is on — the desktop's own, the one every other application uses. It is
the one thing an application can do that reaches somebody who has gone to do
something else.

```soli
def mail_arrived(event)
  msg = Mail.latest()
  eui_notify("Nouveau message", "#{msg.from} : #{msg.subject}", {
    "tag": "thread-#{msg.thread_id}"
  })
  { "state": { "unseen": Mail.unseen() } }
end
```

It is a call and not a node, because a notification is something an
application *does* once where something happened, not something a view *is*.
So it belongs in a handler; a view that calls it says the same thing again on
every render.

- **`title`** is required — a notification with no title is a blank rectangle,
  and an empty one raises rather than showing that.
- **`body`** is the line under it, and may be left out.
- **`opts["tag"]`** is an identity. A second notification carrying the tag of
  one still on screen *replaces* it, so ten replies to one thread are one
  notification rather than ten. Without a tag each one stands alone.
- Long strings are cut to the protocol's limits (256 bytes of title, 1 KiB of
  body) rather than refused: a mail subject is not an attack.

It returns whether it was **sent**, never whether it was seen. The client
shows it only if the person granted `notifications`, and the protocol carries
nothing back — not shown, not clicked, not dismissed, not "this machine has no
notifier". Ask for the capability in `config/routes.sl`, like any other:

```soli
eui_capabilities("notifications")
```

**It goes to the session whose handler is running, and to no other.** That is
the whole rule, and it is what makes the pairing with `eui_wake` the shape to
reach for: something happens in a job, a controller or another window, that
code wakes the component, and each woken session decides for itself whether
its own machine should say anything.

```soli
# in the job that took the mail delivery
eui_wake("mail", "arrived")     # every other window renders, and notifies itself
```

Called where no session is rendering — a controller, a cron job, `soli run` —
there is no window to say it to, and it returns `false` having done nothing.

At most four notifications ride in one batch; a handler that says more sends
them in several, which costs nothing but is worth knowing before writing a
loop that notifies per row.

## Ten things that cost a restart each

None of these is a missing feature; each is a place where a view author's
habit from the web is a fact of this protocol instead, and the failure is
quiet enough to spend an afternoon on. They were collected writing one
application that draws a terminal — a dense grid inside a composed frame —
which is the shape that meets most of them at once.

**There is no margin.** A node cannot push its neighbours away; spacing is
the container's, through `gap` and `pad`, and both are indices into the
`space` scale (`0 2 4 8 12 16 20 24 32 …`, 05 §2) rather than pixels. A
design is therefore laid out as a rhythm of containers — 8 between panels,
12 inside one, a row that is 28 tall whatever it holds — and not as a set of
offsets on the things themselves. Reaching for a margin and finding none is
the first surprise; planning the rhythm instead is the answer.

**`height: "100%"` inside a row is 100% of the *parent*.** Not of the height
the row was allotted. One child asking for it makes the row as tall as the
window and pushes whatever follows off the bottom — a footer that is simply
not on screen, with no error anywhere. In a column whose last child is a
fixed-height bar, give the body an explicit pixel height computed from the
viewport rather than trusting `grow`.

**`opacity` is a byte.** `0.62` is refused with *expected 0–255*; `150` is
what was meant.

**`text_align` is `start`, `center`, `end`, `justify`.** `"right"` is
*unknown value 'right'* — the names are writing-direction-relative, which is
the point of them.

**There is no `accent.subtle`.** `success`, `warning`, `danger` and `info`
each have `.base`, `.subtle` and `.on`; accent has `base`, `hover`, `active`
and `on`. A selected row wanting a faint accent wash uses `info.subtle`.

**Two children of one box may not share a `key`.** Easy to hit without a
loop in sight: a sidebar that lists workspaces *and* the agents inside them
will name the same identifier twice. Prefix the key with the list it belongs
to, not just the thing it names.

**A text node measured a tenth of a pixel short wraps inside its own box.**
Give a mono run an explicit width from a cumulative column position —
`round(n × advance)` — and the rounding can land under what the glyphs need,
so they wrap to a second line *inside* the node, which reads as one word on
top of another. `clamp: 1` stops the wrap and truncates instead, which is
usually not what was wanted either. The fix is two nodes: a box holding the
exact column width, and inside it the text with its own width rounded *up*,
overflowing by the missing fraction — `overflow: visible` is the default and
paints it.

**A grid is the server's arithmetic, and the server does not know the font
scale.** The client multiplies every text size by the viewer's setting
(05 §5); a server that computed pixel widths from an assumed advance is
right only at scale 1.0. Nothing reports the scale, so a cell grid is
honest only as a local tool — worth knowing before designing one for
somebody else's machine.

**There is no absolute placement.** No `left`/`top`, and `position` does not
take coordinates: something that must sit *at* a point — a caret over a
character, a label on a plot — is either drawn inside the flow that produces
that point, or drawn in a `canvas`, whose paths do take coordinates.

**`Tab` is the client's, unless the focused node declares `typing`.** That
prop reads as "this node takes typed text without being a field", and 03
§3.1 now hangs the tab character on it: such a node is sent `Tab` and
`Shift+Tab` as keys and the focus does not move. Everything else keeps the
client's order. The way out of a surface holding both is `Ctrl+Shift+Tab`,
which nothing may claim — so a terminal can have its completion key without
anyone being stranded in it. A node that wants the *old* behaviour simply
does not carry `typing`, and a node that carries it for the soft keyboard
alone now also owns two keys it may not have wanted: check before adding it.

**A `click` handler makes every pixel under it a hand.** The pointer's shape
is the nearest ancestor that names a `cursor`, else a beam over an editable
node, else a hand over anything with a `click` handler (03 §3,
`driver.rs: fn cursor`). So a handler on the *root* — the usual way to close
a sheet by pressing outside it — quietly turns the whole window into a
button, and the moment anything under a stationary pointer shifts by a
pixel, the shape alternates between that hand and whatever the region
underneath says. It reads as a flickering cursor and looks like a client
bug. Put the handler on the scrim that wants it, and give each large region
its own `cursor` so nothing has to walk to the root to find out what it is
over.

**What the pointer is over is a node, not a place.** A rebuilt subtree keeps
the hover on whichever node came back under the same id, so a row that
redraws on a clock keeps its hand and a field keeps its beam, and nothing is
reported for the rebuild — it is not a gesture, and a `pointer_move` per
batch is a conversation with no end. A row that genuinely goes takes the
hover with it: the shape it had stands for one frame and the next settles on
whatever is actually there, with the `pointer_leave` and `pointer_enter` that
belong to it. What does *not* happen is a **reordered** list re-lighting under
a hand that never moved — hover follows the pointer, not the content, the way
a drag's slot does (06 §6.2), so a list that shuffles under a resting hand
does not oscillate. An author who wants the highlight to follow should move
the row rather than rebuild it under the pointer. So the flicker described
above is the one kind left, and it is the view's to fix: if a shape changes
under a pointer that is standing still, something in *this* tree is claiming
it, not the client losing track.

**A tree rebuilt at the `wake` floor is work nobody asked for.** Ten frames
a second of a large tree is ten conversions and ten diffs of everything,
most of it unchanged. Return the **identical object** for a subtree that has
not changed and the encoder keeps it without converting or diffing it — a
terminal that redraws two of its forty lines then costs two lines. It wants
a number that changes when the content does; inventing one on the server is
usually harder than having the thing that produced the content carry it.
