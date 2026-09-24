# 08 — Security

Status: **normative**. Each requirement names where it is enforced.

The threat model: the server is honest but its author makes mistakes; the
network is hostile; a third party may run a server that lies. The client must
survive all three without a crash, a leak, or an action its user did not
grant.

## 1. Transport

- TLS 1.3 only, no downgrade. `http://` and `ws://` are refused in release
  builds without exception; a debug build MAY accept loopback with an
  explicit environment opt-in. *Enforced: `eui-client::transport::check_url`.*
- A manifest MAY declare SPKI pins for its origin; a client that supports
  pinning MUST honour them.
- No user-agent string and no client identifier is ever sent. The `Hello`
  frame carries the protocol version, the viewport, and the granted
  capability bits — nothing else. *Enforced: `Driver::hello`.*

The reference client honours the `EUI_ALLOW_INSECURE_LOOPBACK=1` variable in every build, loopback only, and says so on stderr in a release build. The loopback exception has one other legitimate user: a host that embeds the
client and the server in one process — a desktop artifact — where the
socket never leaves the machine and the host arms its own gate. The client
library exposes this as an explicit call; the `eui` binary has no flag for
it.

## 2. Provenance and integrity

- The manifest is signed with Ed25519; the client pins the publisher key on
  first use and refuses a different key unless a rotation record signed by
  the pinned key accompanies it. The pin, and any remembered grant, is kept
  per **origin and `app_id`** (01 §2.1): the signed record names no origin,
  so a copy of a manifest served from another host is another application,
  with no inherited trust and nothing granted.
- Every asset is named by BLAKE3 and verified on arrival; a mismatch is
  discarded. A CDN or proxy cannot substitute content.


*Enforced: `eui-client::manifest` — the signature is checked with the manifest's own key, the key pinned under the origin and `app_id` in the pin store (`manifest::store_path`), the grants remembered under the same pair, a changed key refused without a rotation the pinned key signed; `lang/src/serve/eui/manifest.rs` signs with a key generated on first use and kept in `config/eui_publisher.pkcs8`.*

## 3. Capabilities

- Deny by default. A capability the manifest never requested, or the user
  never granted, has **no code path** in the client: the check is not at the
  call site, the call site is absent.
- Unknown capability bits in a frame are a decode error. *Enforced:
  `eui-proto::Frame::decode`.*

## 4. Code from the network

- No native code, no JIT, no `eval`. Exactly two kinds of executable content
  reach the client: a chunk that passed the verifier in
  [`07-bytecode.md`](07-bytecode.md) §4, and — only where the `scene`
  capability was granted — a WGSL module that passed the verifier in
  [`11-shaders.md`](11-shaders.md) §2.
- The grant is on the **module**, not on the node kind. A `scene` that names
  no shader runs the client's own program and is drawn without it: what a
  person is asked to allow is somebody else's code, and a picture the client
  computed for itself is not that. Charging for it would have been a toll on
  nothing, and a toll on nothing teaches people to grant without reading.
  *Enforced: `eui-vm::Chunk::verify`, `eui-shader::verify`.*
- Both are **total by construction**, and differently. A chunk's steps are
  bounded at run time by fuel; a shader's are bounded *before it is compiled*,
  because a fragment stage is already on the GPU when it misbehaves and there
  is nothing left to stop it with. That is why 11 §2.4 refuses every loop
  whose trip count cannot be read off constants, including one bounded by a
  uniform — a uniform is a number the server sends after verification.
- A module reaches nothing: no storage, no atomics, no barriers, no images,
  no subgroup or ray-query operations. The only thing bound to it is the
  client's own uniform block, at the one group and binding the client offers.
- A chunk's reachable surface is the ten methods of the host trait: local
  state, node text, props and style by key, the viewer's palette, an event
  queue, a request to go back, and one float of a scene's uniform block.
  The tenth is the narrowest of them: it refuses a node that is not a scene,
  an index outside the eight the block holds, and a value that is not a
  finite number — a typed door can refuse what a general one cannot, which
  is why it is not a `set_prop` of a list. *Enforced by the type: `eui-vm::Host`.*
  `go_back` carries no data in either direction and reaches no state: it asks,
  and the tree that results is the server's (06 §1.3).
- A run is fuel-metered and string-bounded; an abort has no further effect
  and sends nothing. *Enforced: `eui-vm::run`, `Driver::emit`.*

## 5. Memory safety and hostile input

- `#![forbid(unsafe_code)]` on every crate but the GPU boundary.
- The decode path cannot panic: bounds-checked reads, minimal varints,
  rejected unknown discriminants, rejected trailing bytes, every limit
  checked before allocation, no reservation larger than the bytes left in
  the frame could back (a count under its ceiling can still be a lie),
  iterative tree decoding. *Enforced: `eui-proto`,
  under `clippy::indexing_slicing`, `panic`, `unwrap_used`, `expect_used`,
  `arithmetic_side_effects` as errors — denied at the crate root of
  `eui-proto`, `eui-tree` and `eui-vm`, which run inside the application
  wherever there is no worker process; 64 rejection tests; 40 000 hostile
  buffers per `cargo test`; four `cargo fuzz` targets.*
- A batch that is well-formed but incoherent — undefined atom, duplicate
  node (including an id used twice within one subtree), a node past the
  depth limit, index past the end — is refused before anything is placed,
  and the session is poisoned until the next `Mount`. A refused subtree
  leaves no node behind, so the resync's `Mount` is never refused for ids
  the failed one left live. *Enforced: `eui-tree::Session::apply`.*

## 6. Quotas

Enforced by the client, before the memory they bound is allocated: nodes,
atoms and atom bytes, styles, colours, chunks and inline chunk bytes (one
total for a page and its islands), children per node, props and
handlers per node, ops per batch, chunk size, string length in a chunk,
fuel per run. A hostile server can be annoying; it cannot make the client
exhaust itself. *Enforced: `eui-proto::limits`, `eui-tree::Limits`, `eui-vm`.*

## 7. Input isolation

- An application never sees a keystroke outside a focused editable node or
  a node that holds focus and a `key_down` handler. There is no global key
  capture. *Enforced: `Driver::key`.*
- Clipboard reads require `clipboard.read`; pointer position is reported
  only inside the window. A person's own `Ctrl+V` into a field is not an
  application read: the window inserts the text and the application sees
  it as typing, exactly what a browser does. *Enforced: the driver has no
  clipboard access at all; the window reads it only on that key.*

### 7.1 The filesystem

The client reads a file only when a person picked it in the platform's own
dialog, and writes one only where a person just said. Everything else about
this feature follows from those two sentences.

- A dialog opens on an **activation and nothing else**: a primary click, or
  `Enter`/`Space` on a focused node, on a node carrying `pick` or `save`
  with a server handler for the answering event and the capability granted
  (spec 03 §3.2). A tree that merely arrives opens nothing; neither does a
  batch, a `wake`, or a local chunk. There is no frame that opens a dialog,
  because a frame is not a person. *Enforced: `Driver::offer_files`, called
  from the click and activate paths alone.*
- `fs.pick` and `fs.save` are separate capabilities, and neither is implied
  by the other: reading what someone chose and writing where they said are
  different powers. *Enforced: `Driver::offer_files`.*
- A **`Blob` for a node with no open save ends the session** — that is a
  server attempting to write a file nobody offered it, and there is no
  benign reading of it. A file is created on the first chunk, not when the
  path is chosen, and an aborted transfer removes what it had written.
  *Enforced: `Driver::blob`, `Shell::pump_writes`.*
- No path ever reaches a server: a `file_pick` carries a name and a size, a
  `file_save` a name (spec 06 §3). A dismissed dialog is reported to nobody.
  *Enforced: `Driver::picked`, `Driver::saving`, both through `basename`.*
- A file's bytes never enter the worker in the direction that would matter:
  the window reads and writes files, because the worker cannot open one and
  must not be able to (§10). What crosses the pipe is a name, a size, and
  opaque bytes.
- The ceilings are the client's, not the tree's: a node may ask for less
  than [`10-budgets.md`](10-budgets.md) §5 allows and never for more, and
  the driver enforces what it framed rather than trusting the size a
  dialog reported. *Enforced: `Driver::upload_chunk`, `Driver::blob`.*

### 7.2 The camera, the radio and the reader

- A camera sheet opens on an **activation and nothing else**, by the rule of
  §7.1 and the same code path: `pick` with bit 1 of its flags, a server
  handler for `file_pick`, and `camera` granted (spec 03 §3.2). A picture
  taken this way is a file and travels as one; nothing new carries it.
- The same for a recording: `pick` with bit 2, a server handler for
  `file_pick`, and `microphone` granted. What comes back is a finished
  recording — a file — and nothing here carries a live one.
- `camera`, `microphone` and `fs.pick` are three separate grants and none
  implies another. A node asking for the camera on `fs.pick` alone opens
  nothing; so does one asking for a folder on `camera` alone.
- A scan starts on an activation and nothing else (spec 03 §3.3), and reads
  **one** tag. A reader that delivers twice is answered once, because the
  scan is over after the first.
- **Location is not asked for by activation** — a page that shows where you
  are would be unusable if every fix needed a click — so its limits are
  elsewhere and there are four of them: the capability, the window having
  the input, a fix actually existing, and a floor of one second
  (spec 06 §1.2). Without the capability the tree is not read for `locate`
  at all and an offered fix is discarded rather than stored.
- A location leaves the client **coarse**: a thousandth of a degree, and an
  accuracy never better than 100 m. The rounding is the client's, before
  anything is sent, because it is the only side of this that a server
  cannot argue with. There is no capability that buys more.
- A session that ended stops all three: no scan is in flight, no node is
  asking to be placed, and the fix that was held is dropped (spec 01 §4).

### 7.3 Resuming a session

A session id is a bearer credential: whoever holds it can pick the session
up. It MUST come from a CSPRNG, MUST NOT be logged, and MUST NOT be handed
to a second socket while a first is still on it — two windows on one tree
is a session hijack whichever way it happens. The client believes the
server's `resumed` answer over its own memory (01 §4.1), so a server that
has forgotten a session cannot be made to act on a tree it no longer has.

## 8. Privacy

No user-agent, no font enumeration, no canvas readback, no device
identifier, no third-party connection. The fingerprinting surface is the
viewport frame, and that is a testable claim.

An application MAY supply a face (02 §5.1, 05 §3), and none of the above
softens to let it:

- the face is an **asset**, named by its content and fetched from the
  session's own origin like every other byte a server chose — *enforced:
  `eui-client::assets::origin_for` derives the origin from the session URL
  and nothing else can name a host*. A face hosted by a third party is
  fetched **by the server**, once, and re-served from the application's
  origin; the viewer's address never reaches it;
- the client still never asks the machine what fonts it has — *enforced:
  `eui-text::TextEngine::new` builds its database by hand rather than
  through `FontSystem::new_with_fonts`*. A session's faces are exactly the
  embedded ones plus the assets it was sent, so the same text shapes the
  same way on every machine;
- the face is parsed in the confined worker, under the same seccomp and
  Landlock as the tree (§10). A font file is a table of offsets a server
  chose, and it meets the parser where a parser taken over reaches nothing;
- bytes that are not a face the client reads are discarded rather than
  displayed, and the role falls back to `sans`. **No font a server sends or
  fails to send can stop text being drawn.**

A `scene` (03 §1.2) does not widen it, and is built so that it cannot:

- its target carries no `COPY_SRC`, so reading it back is a validation error
  rather than a rule somebody has to remember;
- a press on it reports a position and never an object, a triangle or a
  depth — picking is readback under another name;
- the adapter's name, vendor, device and driver reach neither the tree nor an
  event payload, and a shader that fails to compile yields one word from a
  closed vocabulary, never the compiler's own diagnostic, which would name
  the driver and through it the machine;
- no timestamp query is enabled on a device that will run one.

**One channel is not closed, and saying so is the point of this paragraph.**
A server can already time a client: a `wake` arrives on the driver's clock
while a sound's `time_update` arrives on the audio thread's, and the
difference between them is a coarse measure — about a tenth of a second — of
how hard the window is working. That existed before scenes. What a shader adds
is the ability to *modulate* what is being timed. The client does not close
this; what it does is refuse to add a finer one.

`level` (03 §7) is the case that tests that refusal, and it is why the
event is shaped the way it is. A meter wants to be fast, and a fast meter
would be exactly the finer clock this paragraph refuses — so `level` is
sent on `time_update`'s existing tick and carries the peak since the last
one rather than a fresh sample. Nothing about the timing surface changes:
the same four messages a second the client already sent, one of them now
carrying a second number.

What the number itself says is bounded by where it is measured. The peak
is taken on the source's own samples with the application's own `volume`
applied and **the viewer's master gain deliberately excluded**, so it is a
property of bytes the server sent, at a gain the server set — a value the
server could have computed for itself from the file it uploaded. Measured
one line later, after the master gain, it would instead be a readout of
the viewer's volume knob, and a zero would report that they had muted.
That is the whole difference between a meter and a sensor, and it is one
multiplication. *Enforced: `eui_audio::Mixer::take_peak`, pinned by
`peak_is_before_the_viewers_own_gain`.*

### 8.1 The address bar is a hole in §8, and this is what it costs

`net.open` (03 §3.5) is the one capability that hands something to the world
outside this client, and the thing it hands over cannot be sanitised: a
server may put a token in the address it offers. The browser then arrives at
that server carrying the person's cookies and their address, and the session
they were reading is linked to the identity they browse the web with.

No allowlist closes that — a server is always allowed to link to itself — and
nor does the scheme check, which is there for a different attack. What the
client does instead is make the link the *person's* act and keep it that way:
no op opens an address, no event reports one, the host is shown before the
opener is called, and the capability is refused by default. A person who
never clicks is never correlated, and a person who clicks has done the same
thing they do every day in a browser.

It is written here rather than discovered later because §8's claim is that
the fingerprinting surface is the viewport frame. With `net.open` granted,
that claim has an asterisk, and this paragraph is the asterisk.

### 8.2 A notification is the one thing a server may do while nobody looks

Everything else in this protocol happens inside a window somebody has open
in front of them. A notification (02 §5.2) does not: it is raised by the
machine, it outlives the batch that asked for it, and it reaches somebody
who had gone to do something else. That is what makes it worth having and
what makes it the loudest thing a server can reach for, so three things
hold it down.

The **capability** is the whole of the gate, because nothing in the tree
asks for a notification and there is therefore nothing else to refuse; a
client not granted it drops the op and says so once, to its own error
output and never to the server. The **count** is the second: four to a
batch, refused past that, which is the only limit in 02 §6 that bounds
attention rather than memory. And **nothing comes back** — not shown, not
clicked, not dismissed, not "this machine has no notifier". A server that
notifies learns exactly what a server that offers an address learns, which
is nothing, and for the reason §8.1 gives at length: an answer is a probe
for what this machine is and who is at it, with a clock beside it.

The text itself is a server's string on its way to a platform that parses
arguments, so it is cleaned before it is handed over — control characters
out of the title, the body and the tag, nothing passed through a shell, and
no notifier started with a string that could be read as a flag.
*Enforced: `eui-client/src/driver.rs::note` and `one_line`, pinned by
`a_notification_needs_the_capability_and_a_title`.*

## 9. Server side

Every client event is validated against the tree the server last sent: the
node exists, it carries a handler of that kind naming that atom, the payload
has the declared shape. A local handler's effect is advisory until the server
re-derives it. *Enforced: `lang/src/serve/eui/session.rs::validate`.*

## 9.1 Sound and moving pictures

Playing a sound needs no capability: it is output, like drawing, and a
window that can draw can already annoy. What it does need is a bound, and
the client imposes it — at most eight sources at once, decoded bytes
counted against the session's asset quota, and the viewer's own volume
above everything, unreadable by the application — including through the
`level` event, which is measured before that gain is applied precisely so
that it stays unreadable. Decoding runs in the
worker with every other decoder; the audio device belongs to the window
process. Recording is not output and is not this: the microphone is a
declared capability (01 §2.1) and is not implemented.

A moving picture is bounded the same way and decoded in the same place.
Its formats are chosen for the same reason: GIF and animated WebP decode
in pure Rust with no C library, no assembly and no patent licence, which
is worth more here than the compression a real codec would buy.

## 10. Process isolation

Everything that reads bytes a server chose — the frame decoder, the tree,
layout, text shaping, the picture decoders, the bytecode VM — runs in a
**worker** process. The **window** process keeps what needs the platform:
the display, the GPU driver, TLS, the pin store, the clipboard, the
accessibility adapter. Two pipes carry a private request/reply protocol
between them: the window forwards raw frames and inputs; the worker
answers with outbound frames and, on request, a draw list and the atlas
bitmaps behind it. The window never decodes a frame. A worker that dies —
a panic, a runaway allocation, a sandbox kill — ends the session with a
reason; the window stays standing.

On Linux the worker confines itself before it reads its first byte, with
two mechanisms each sufficient on its own:

- **Landlock** handles every filesystem and TCP right the running kernel
  knows, with no rule granting any: a deny-all, best effort on older
  kernels.
- **seccomp** allows the system calls the worker loop needs — its pipes,
  memory, clocks, signals, exit — and **kills** the process on any other.
  A worker that reaches for `openat` or `socket` is not tolerated and
  asked again; it is gone.

The worker is also marked not dumpable: a kill leaves no core — the core
would be the session, every text on screen, written to disk — and no other
process of the user can attach a debugger to it. What initialises itself
lazily (the text engine's font loader starts a thread pool and asks for the
core count) is warmed before the door closes, and the confinement is
applied to every thread. The two decode threads that take pictures, sounds
and moving pictures off the paint (10, *Assets*) are among what is warmed:
after the door closes a thread cannot be created, so they are started by
the throwaway driver the worker builds first, and every decode runs on
them, confined like the rest
(`a_picture_is_decoded_in_the_confined_worker_and_lands_later` in
`crates/eui-client/tests/worker.rs`).

Warmed means **running**, not created. A thread that has been created but
has not yet started does its own first steps whenever the scheduler gets to
it: the runtime names it (`prctl(PR_SET_NAME)`), and its first allocation is
where the C library gives it a memory arena. Behind the filter either one is
a kill, of a worker that did nothing wrong, and a loaded machine makes it
likely: a parallel test run, or a window starting its GPU in the same
instant. So the worker MUST NOT close the door until every thread it started
has run code of its own and allocated: the decode pool returns only once
each of its threads has checked in, and the lock-down first has every thread
of the text engine's pool run a task that allocates.

An arena is not only a timing question. glibc, while no arena limit is set,
works one out the first time the process holds eight arenas, by counting the
processors: `__get_nprocs`, an `openat` of `/sys/devices/system/cpu/online`,
in whichever thread's allocation got there, whenever that is. A thread
that allocated nothing during the warm-up and does so later is killed for
it. The worker therefore runs under `GLIBC_TUNABLES=glibc.malloc.arena_max=4`:
with a limit set, glibc takes it and never counts. The window sets it on the
worker's environment after clearing it, so nothing inherited can remove it,
and a worker started without it runs itself again with it before doing
anything else. Four is an arena for the worker's loop and one for each
decode thread, and one over. The allowlist is not widened for either call:
neither is something the worker needs
(`every_thread_has_started_before_the_sandbox_closes` in
`crates/eui-client/tests/worker.rs`: 128 workers locking down at once,
none killed).

**The `scene` capability moves one thing across this line, and it is the
largest thing on either side of it.** A shader is verified in the worker —
that is where the parse meeting bytes a server chose belongs — but it must
then be compiled, and a shader compiler belongs to the graphics driver, which
is in the window because the GPU is. So a program the server wrote is
translated by the client's shader front end and then compiled by the vendor's,
inside the process that holds TLS and the pin store. No division of labour
removes this. What verification buys is that the input to that compiler is
small, dull, and already rejected if it is anything else; what it does not buy
is the compiler's own safety. A client that grants `scene` MUST wrap that
compilation in a validation scope, because the alternative — the host API's
default — is a panic in the window process, which is the one outcome this
section exists to prevent.

**One window process may hold several applications, and that is a choice
about cost rather than about this boundary.** A window process carries the
GPU instance, the adapter, the device and the compiled pipelines, which is
most of what a first pixel costs; a client MAY therefore open a second
application's window in a process that is already running rather than
starting another. What it MUST NOT do is share a worker: the confinement
above is per session, and two applications in one worker would be two
servers' bytes in one address space. What it accepts in exchange is shared
fate — a lost device or a panic in the event loop takes every window in
the process — so a client that does this MUST offer a way to launch one
alone. The reference client hands a launch to the running instance over a
`0600` socket in the user's runtime directory, keyed to the exact build on
both ends, and `--standalone` opts out.

`EUI_SANDBOX=0` runs the driver in the window process, for debugging; the
window prints which of the two it did and what the sandbox enforced. macOS
(`sandbox_init`) and Windows (AppContainer) are not done: there the worker
is still its own process — a crash is contained — but a compromised worker
is not confined, and the window says so.

*Enforced: `eui-client/src/sandbox.rs` (Landlock, seccomp, dumpable),
`eui-client/src/worker.rs` (the boundary and the wire),
`eui-client/src/instance.rs` (the shared window process and its socket).
Tested:
`eui-client/tests/worker.rs` — the counter end to end through a confined
worker, a hostile frame, a dead worker, and self-tests that a file read, a
TCP connect and an exec are refused.*
