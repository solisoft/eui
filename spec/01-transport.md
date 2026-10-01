# 01 — Transport

Status: draft

EUI runs over HTTPS. There is no EUI-specific port, no new TLS profile, and no
new certificate story: an EUI application is served from an ordinary origin,
behind ordinary proxies and CDNs.

The one exception is an application on the person's own machine, which
starts the client itself and speaks to it over a pair of pipes (§7). Nothing
in that case is served, and nothing in this document about origins,
manifests or certificates applies to it.

## 1. Requirements

- TLS 1.3 is REQUIRED, and a client MUST offer no earlier version: a server
  that answers `wss://` with TLS 1.2 is refused, not accommodated. A client
  MUST refuse `ws://` and `http://` origins, with **one** exception, which is
  narrow and is stated here rather than left to each implementation:
  a **loopback address** — `127.0.0.1`, `localhost`, `[::1]` and nothing
  else — MAY be reached in plain text when the person running the client has
  asked for it, and a **release** client that allows this MUST say so on its
  diagnostic channel (a debug build is already somebody's own). A client
  MUST NOT offer the exception for any other address, and MUST NOT infer it
  from the address alone: it is granted, never assumed. "Loopback" is a
  judgement about the **host**, not about how the address is spelled:
  `ws://localhost.example.com/` is not loopback, and a client that decides
  by prefix has given the exception away to whoever registers the name.
  See 08 §1 for how the reference client is asked — an environment variable
  on a desktop, and on the web the browser's own answer to "was this page
  itself served from loopback?", which nothing the page contains can forge.

  *This rule used to read "debug builds only; a release client MUST NOT
  honour it", which no implementation has ever obeyed and which nothing
  could obey and still be testable: a release binary talking to a
  `soli serve` on the same machine is how this project measures everything
  it claims. A requirement that the reference client violates on every run
  is not a requirement, it is a comment. What actually bounds the risk is
  the address and the asking, and those are now what the rule says.*
- A client verifies the certificate chain. The public web's roots are the
  baseline, but an application on a private network or behind a development
  proxy is signed by a root no public list carries, so a client SHOULD also
  honour **the roots the machine it runs on already trusts** — a client that
  refused what the browser beside it accepts would be read as broken — and
  SHOULD accept further roots named out of band. The reference client takes
  all three: `webpki-roots`, the platform trust store (`EUI_CA_SYSTEM=0`
  opts out), and `EUI_CA_FILE` (one PEM bundle, or several separated by
  `:`). A client MUST NOT offer a way to skip verification — an option that
  accepts any certificate makes every other requirement here decorative.
- A client MUST NOT follow a redirect that changes origin during discovery.
- A client MUST send no user-agent string and no client identifier. The only
  request headers it sends are those required by HTTP itself plus `Accept`.

## 2. Endpoints

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/.well-known/eui` | Application manifest |
| `GET` | `/_eui/asset/<blake3-hex>` | Content-addressed asset |
| — | `wss://<host>/_eui/session` | Interactive session |
| `GET` | `/_eui/view/<component>` | One-shot render, cacheable (§2.4) |

### 2.1 Manifest

`GET /.well-known/eui` returns `application/vnd.eui.manifest` — an EUI-encoded
record (see [`02-wire-format.md`](02-wire-format.md) §7) carrying:

- `app_id`, `name`, `version`
- `protocol_min`, `protocol_max` — supported EUI versions
- `publisher_key` — Ed25519 public key, 32 bytes
- `signature` — Ed25519 over the manifest body excluding the signature field
- `capabilities` — the set requested; the client grants none of them implicitly
- `theme` — blake3 of the default theme asset
- `icon` — blake3 of a PNG, fetched as an asset (§2.2): the icon a
  launcher entry made for this application wears. OPTIONAL, and signed
  like everything else here, because a tile in a dock claiming to be an
  application is exactly the thing an intermediary should not be able to
  change.
- `entry` — session path, defaults to `/_eui/session`. A client given an
  address with no path MUST take the origin's manifest and connect to
  `entry` on it, so that `wss://host` is an address and not half of one:
  the protocol's own prefix is the part nobody should have to type. An
  address that names a path is already whole and `entry` does not touch it.
  `entry` is inside the signed body and MUST be an absolute path, so it
  names nothing the origin could not have served anyway.
- `pin` — OPTIONAL SPKI pin set for the origin

The record's keys, in the order they MUST appear (02 §7 leaves the table to
each document):

| key | field | value |
|---:|---|---|
| 0 | `app_id` | `Str`, non-empty |
| 1 | `name` | `Str` |
| 2 | `version` | `Str` |
| 3 | `protocol_min` | `Int` ≥ 1 |
| 4 | `protocol_max` | `Int` ≥ `protocol_min` |
| 5 | `publisher_key` | `Str`, 64 hex digits |
| 6 | `capabilities` | `Int`, the bitset of the capability names below |
| 7 | `theme` | `Str`, 64 hex digits, or `Null` |
| 8 | `entry` | `Str`, an absolute path |
| 9 | `rotation` | `List[Str previous key, Str signature]` or `Null` |
| 10 | `icon` | `Str`, 64 hex digits — **record version 2 only** |
| 10 / 11 | `signature` | `Str`, 128 hex digits; always last |

The signature is always the record's last field, so its key is the count of
the fields before it: **10** in a version 1 record, **11** in a version 2
one, where slot 10 is the icon. The version byte is what says which, and a
decoder MUST NOT guess from the field count.

A manifest with no icon MUST be written as version 1, and `icon` MUST NOT
be `Null` in a version 2 record. Both rules exist so there is exactly one
encoding of any manifest: a record published before icons existed is still
byte-for-byte what its publisher signed, and a decoder can always rebuild
the signed bytes exactly. A client that does not know version 2 rejects
such a manifest, which is the same answer it gives any record it cannot
verify — publishing an icon is opting into that.

Strings are at most 256 bytes. The signature is Ed25519 over the record
encoded with the signed fields only (`field_count` 10, or 11 with an icon),
which a decoder can rebuild exactly because the order is fixed. Capability names and bits: `camera` 1,
`microphone` 2, `clipboard.read` 4, `clipboard.write` 8, `notifications`
16, `location` 32, `fs.pick` 64, `fs.save` 128, `nfc` 256, `scene` 512,
`net.open` 1024.

`scene` is named for what the person agrees to and not for the hardware it
uses: that this application may run **a graphics program of its own** on their
machine. Both halves of that are worth telling them — it spends their graphics
card and can slow the machine down, and it still cannot read what is on their
screen (03 §1.2, 08 §8).

Requesting it and being granted it are separate acts. A `scene` that names no
shader draws either way: it is the client's own program over the client's own
shape, and there is nothing third-party in it to allow. The **request** is
what raises the floor below, because an older client cannot decode the kind
however the scene is drawn; the **grant** is about running somebody's code. An application that asks for it MUST
advertise `protocol_min` of at least 2, because a client that cannot decode
`0x11` cannot be shown it at all; the refusal then happens at the handshake,
with a reason, rather than mid-session on a batch.

`notifications` is a capability with no node behind it. Everything else in
this list is asked for by something in the tree — a `camera` node, a `pick`
prop, a node carrying `open` — and can therefore be refused by refusing what
asked. A notification is asked for by an op ([`02-wire-format.md`](02-wire-format.md)
§5.2), which arrives whether or not anybody is looking at the window, so the
grant is the only thing between an application and the person's attention. A
client that was not granted it decodes the op, counts it, and shows nothing;
the session goes on, because a notification is not part of the document.

`net.open` is the other one worth a sentence, for the opposite reason:
it is the only capability that hands something to the world outside this
client. What it buys is a person clicking an `https:` address and their own
browser opening it (03 §3.5); what it costs is that the address may carry a
token, and the browser then arrives at that server with the person's cookies
beside it. No allowlist closes that — a server is always allowed to link to
itself — so the grant is the whole of the defence, and 08 §8.1 says so at
length.

The set grows from the top and a bit outside it is a decode error
([`08-security.md`](08-security.md) §3). That is the point rather than a
limitation: a client that does not know what a bit means must not agree to
it, and an application that asks for one the client has never heard of
finds out at the manifest rather than at the call.

A client MUST verify the signature before acting on any other field, and
MUST refuse a server whose protocol range excludes its own version. On first
run it pins `publisher_key` for the pair of the **origin** the manifest was
fetched from and its `app_id` (trust on first use). On later runs a
different key for that pair MUST be rejected unless `rotation` names the
pinned key and carries that key's Ed25519 signature over the new
`publisher_key`; the pin then moves.

The origin is part of the key because the record does not carry one. A
manifest is public, and its signature says who *wrote* it, not who is
*serving* it: the same bytes copied onto another host verify there exactly
as well. Pinned by `app_id` alone, such a copy would match the real
publisher's pin — and show as trusted under its name — and whichever origin
reached an unpinned `app_id` first would lock the real publisher out of it.
The origin is scheme, host and port, spelled one way (lower case, a default
port dropped). A client that kept pins by `app_id` alone before this rule
MUST NOT carry them over, since which origin each came from is what they
did not record: every application is trusted on first use once more. A manifest that fails any of this ends the connection before a
session is opened. The one exception is the debug loopback of 08 §1: over
`ws://` on `127.0.0.1` a client MAY proceed without a manifest, and MUST say
so on its diagnostics.

The client grants the intersection of `capabilities` with what the person
allowed it — on the reference client, `--allow` on the command line — and
reports it in `Hello.granted`; nothing is granted by being asked for. A
client that remembers the person's answer MUST keep it under the same pair
of origin and `app_id` as the pin, for the same reason: a grant given to an
application at one origin is not a grant to another origin serving its
manifest. Answers kept by `app_id` alone are not carried over either, so
each application asks once more.

### 2.2 Assets

Assets — images, fonts, bytecode chunks, themes — are named by the BLAKE3 hash
of their content. A client MUST recompute the hash and MUST discard a mismatched
response. Because the name *is* the content, `Cache-Control: public, max-age=31536000,
immutable` is always correct and a hostile CDN cannot substitute content.

An asset response larger than the session's remaining asset budget
([`10-budgets.md`](10-budgets.md), *Assets*) MUST be abandoned mid-stream.
"Remaining" is the budget less what the assets the live tree names already
hold, since everything else may be let go to make room; a response whose
`Content-Length` exceeds it is abandoned before its body is read. Enforced
in `assets::fetch_within`, with the room measured by the driver
(`Driver::asset_room`).

The client's request carries no cookie and no session: an asset is
addressed by content, so the response is the same for everyone and a proxy
may serve it to anyone. A server MUST answer `404` for a hash it does not
hold, never a redirect.

### 2.3 Session

The session is a WebSocket over TLS carrying **binary** frames only. A client
MUST close the session on receiving a text frame.

Rationale for WebSocket in v1: it traverses every proxy in existence today and
Soli already speaks it (`lang/src/live/socket.rs`). The framing layer below is
specified independently of the transport so that HTTP/3 + WebTransport can be
substituted without changing a single message byte.

### 2.4 One-shot render

A session costs a server a resident session per reader: the interned tables,
the previous tree, and an instance to hang them on. For a page whose readers
mostly read — documentation, a catalogue, anything with a link to it — that is
the wrong shape, and the socket buys nothing, because nothing on such a page
changes without the reader.

`GET /_eui/view/<component>?v=<protocol>` answers
`application/vnd.eui.frames`: **exactly the frames a fresh socket would have
sent**, up to and including the first `Mount` — a `Welcome`, then one or more
`Batch`, each in the framing of §3. There is no second encoding, and a client
that can read a socket can read this by feeding the body through the same
decoder.

- The client's protocol version is a **query parameter and not a header**
  because it is part of the cache key: a client at 3 and a client at 5 are
  owed different bytes, and a shared cache must not hand one the other's. The
  `Welcome` names `min(client, server)`, as the socket's does. A server MUST
  answer `400` to a version it cannot read as one, rather than clamping: a
  `Welcome` naming version 0 ends the session it was meant to open.
- The `Welcome` MUST carry a session id of sixteen zero bytes. It names no
  session, because this body is everyone's — and a server that put a real
  handle there would give every response a different ETag and cache nothing
  while appearing to.
- The request carries **no cookie and no session**, exactly as an asset's does
  (§2.2). A component whose render depends on who is asking MUST NOT be served
  this way, and a server MUST refuse the endpoint for a component that
  requires a session. This is the whole of what makes a shared cache in front
  of it safe, and it is why no `Vary` is sent.
- A strong `ETag` is the BLAKE3 of the body. `If-None-Match` is read as a list
  and `*` matches. `Cache-Control` is the application's to choose.
- The render is given a **nominal viewport**, since there is nobody to ask.
  A view that branches on width may take one from `?w=`, which then joins the
  cache key; a server SHOULD snap it to a small declared set rather than
  render a variant per reader.
- A server MUST NOT offer this for a component whose first render is not the
  same for everyone, and an application declaring a component this way is
  promising that its `connect` is a **read**. `GET` is exempt from any
  same-origin check by construction — a resource a CDN is meant to hold cannot
  have one — so an `<img>` on any page anywhere reaches this endpoint.

**Determinism is normative here.** Two renders of the same component, at the
same version and viewport, MUST produce the same bytes, because the ETag is
the identity of those bytes and because anything that later has to recognise
the same tree arriving by another road does so by hashing them. What breaks
it is application code, not the protocol: a view that reads the clock, a
random, or a counter that moves per render. Such a view is still correct — it
simply caches nothing.

Note for anything that compares two of these: the **ETag covers the whole
body** and the `Welcome` differs between the two roads a tree can arrive by,
so the identity of a *tree* is the hash of its `Batch` frames alone.

### 2.6 Adopting a tree

A component that is fetched over §2.4 and later needs the server has a tree
already. Mounting it again would work and would be wrong: the client's
`Mount` handling discards the tree, the layout, focus, every scroll offset and
anything half-typed, so a reader halfway down a page who clicks something is
returned to the top with nothing focused. On the pages §2.4 exists for, that
is the most visible thing about it.

So a client that holds a fetched tree offers it, in the `resume` field of its
`Hello` (§4.1), as a third tag:

```
resume := 0x00                       -- nothing
        | 0x01 session:16  acked:varint
        | 0x02 tree:32
```

`tree` is the BLAKE3 of the **`Batch` frames** of the body it holds, in order,
and **not of the body**. The two differ because a body's `Welcome` carries
sixteen zero bytes and a socket's carries a session handle, so a hash over the
whole body would name something no socket could ever agree to. A server
comparing the wrong one finds no match, ever, and the only symptom is that
adoption never happens — so it is said here rather than left to be discovered.

The offer is not a claim. A `Resume` names a session and is a bearer
credential for it; this names only a tree, there is no session yet, and the
worst a wrong hash can do is cost a `Mount` that would have been sent anyway.
A client MUST NOT send both, and a client whose tree came over a socket MUST
offer `0x01`.

The server renders what it would have rendered, hashes those frames, and
answers in `Welcome.start`:

```
start := 0x00 Fresh | 0x01 Resumed | 0x02 Adopted
```

- **`Adopted`** — the hash matched. **No `Mount` follows.** The session
  continues from the tree the client has, and its batch sequence continues
  from where the one-shot render left off (§2.4 numbers from 1). The client
  keeps its tree, its layout, its focus and its scroll precisely by not
  tearing them down.
- **`Fresh`** — it did not match, or nothing was offered. The client MUST
  discard what it holds, and the server sends the frames it just rendered
  **as they are**. There is no second render: the comparison is of bytes the
  server produced once.

**This is one render either way**, and a server that renders twice to answer
it has misread this section.

A `Welcome` naming `Adopted` to a client that offered no tree is a protocol
error, as `Resumed` naming an unoffered session is (§4.1).

A server MAY decline to adopt for any reason — it need not implement the
comparison at all — and `Fresh` is always a correct answer. `Adopted` is an
optimisation the protocol makes possible, not an obligation it imposes.

### 2.7 Islands

§2.4 makes a page cost nothing to serve and §2.6 makes the session that
follows it seamless, but both are all-or-nothing: the moment one corner of a
page needs to be live, the whole page is a session again, and every reader
pays a session's memory for a comment count that changes twice a day.

The word is the web's own, and it is the right one: a page that is mostly
still, with islands in it that are not. It also stays clear of **live region**,
which in this document and everywhere else means what `aria-live` means (§09
§7.3). That was not only a confusing name: `live` is **already a prop**
(03 §6.1, `"polite"` or `"assertive"`), so the two would have collided on the
same node.

An **island** is a node whose *content* comes from a session of its own.
The page around it stays a cached render that no session is held for.

```
{"k": "slot", "island": "/_eui/session/comments?for=1042", "c": [ … ]}
```

`island` is an absolute path on **the same origin**, and a client MUST refuse
one that names another: a tree that could open a socket elsewhere would make
every page a way to reach any host the reader can reach. It is an ordinary
prop (§02 §4.4), so it needs no protocol version of its own — and a client
that does not know it ignores it and draws the node as it stands, which is
the whole degradation story and it is free.

**The node's own children are what shows until the island speaks**, and they
came from the cached render. An application can therefore put something
useful there — the count as it was when the page was rendered, a skeleton,
last night's build status — and the live version replaces it if and when a
session opens. A page whose regions never connect is a page that is merely
out of date, not a page with holes in it.

**The two trees never share a node id space.** The page's ids come from the
render that produced it; the region's come from its own session's encoder,
which starts at 1 like any other. An island's `Mount` replaces that island's
content and nothing else; its ops address its own nodes; and an event raised
inside it carries its own ids and goes to **its own socket**. How a client
reconciles the two is the client's business and this document does not say —
only that a server may reason about its own session alone, and that no id it
sends can name a node it did not create.

The boundary is the node carrying the prop: that node belongs to the page,
everything below it belongs to the island.

- A client MUST open **one session per distinct `island` path**. Two islands
  naming the same path share one session; two naming the same component with
  different queries do not, because the query is what tells the application
  which island it is rendering.
- A client MAY defer opening until the node is first laid out, and SHOULD for
  one that is not visible — a comment thread below the fold on a page nobody
  scrolls should cost what the rest of the page costs, which is nothing.
- An island whose session cannot be opened, or which ends, **leaves the page
  alone**: the node keeps the children it had, the reason is the client's to
  report, and nothing else on the page is torn down. A live part failing must
  never be able to take a still page with it. A client SHOULD NOT redial it
  while that node stands: what it shows is the island's last content, and a
  session that fails on every attempt would otherwise be dialled forever.
- An island **closes** when its node is released — a page `Mount`, a
  `Replace` or a removal that takes the node — or when the node stops naming
  that path. The client MUST then drop its session, and MUST refuse a frame
  that still arrives for it rather than apply it: the node it hung under is
  gone, and whatever now stands in its place belongs to the page. An island
  that closed because its path changed takes its content with it; the page's
  own children under the node stay. A node that asks again — the page
  navigated back to — is a new island and is opened afresh.
- At most `MAX_ISLANDS` sessions may be open for one page **at once**
  ([`10-budgets.md`](10-budgets.md) §1); one that ended but still shows its
  content counts until it closes, and one that closed counts no more. Past it a client opens no more and
  leaves those islands as they were rendered. The ceiling exists because a
  tree is data: a view that derives an island per row would otherwise open a
  socket per row.
- An island's tree may itself carry `island`. Each is a session like any
  other, and the same ceiling counts them all.

Nothing here adds a frame, an op or a version. An island is an ordinary
session that happens to be addressed by a prop, which is why an application
can adopt one without its clients being rebuilt.

## 3. Framing

Each WebSocket binary message carries exactly one frame:

```
frame  := kind:u8  len:varint  payload:len×u8
```

`len` MUST NOT exceed `MAX_FRAME_BYTES` (8 MiB). A client MUST reject a frame
whose declared length does not match the remaining message bytes exactly —
trailing bytes are an error, not padding.

On a pipe (§7) there are no messages, and frames follow one another with
nothing between them: a frame's own `len` is what ends it. The frame is the
same bytes either way, which is the whole of what lets one decoder serve both.

| kind | Name | Direction | Payload |
|---|---|---|---|
| `0x01` | `Hello` | C→S | protocol version, viewport, theme mode, density, font scale, granted capabilities, and what the client brings — a session (§4.1) or a tree (§2.6) |
| `0x02` | `Welcome` | S→C | negotiated version, session id, and how the session starts — fresh, resumed (§4.1) or adopted (§2.6) |
| `0x03` | `Batch` | S→C | a sequence of ops (§02 wire format) |
| `0x04` | `Event` | C→S | node id, event kind, payload |
| `0x05` | `Ack` | C→S | last applied batch sequence number |
| `0x06` | `Ping` | both | 8-byte opaque |
| `0x07` | `Pong` | both | echo of the ping payload |
| `0x08` | `Error` | both | code:varint, message: atom or inline UTF-8 |
| `0x09` | `Resync` | C→S | client state is unrecoverable; send a full `Mount` |
| `0x0A` | `Viewport` | C→S | size, scale factor, theme mode, density, font scale changed |
| `0x0B` | `Upload` | C→S | a chunk of a file the person picked (§6) |
| `0x0C` | `Blob` | S→C | a chunk of what a node's `save` offers (§6) |
| `0x0D` | `Fetch` | C→S | an asset wanted, on a pipe only (§7.3) |
| `0x0E` | `Asset` | S→C | a chunk of an asset, on a pipe only (§7.3) |

`Fetch` and `Asset` arrived with version 8, and exist only on a pipe —
whatever version the session negotiated there (§7.1). Over a WebSocket an
asset is an HTTPS request (§2.2), and either kind arriving there MUST be
rejected as if it were unknown.

Any other `kind` MUST be rejected. Unknown kinds are not reserved for
forward compatibility; version negotiation in `Hello`/`Welcome` is the only
extension mechanism, so that a client never has to guess at semantics.

**`Viewport` during a resize.** A view that lays itself out from the width
is re-rendered by the server, so how often a client says the size is how
closely the page follows the window. A client SHOULD send the size a resize
reached **at once** when no size has gone in the last 32 ms — a window snapped
or maximised is one step, and waiting on it is all delay — and otherwise
32 ms after the last one went, a period a later step does not push back; and
it SHOULD NOT send another while the server has not yet answered the last
(a `Batch` arrived) unless 250 ms have passed, so that a view slower than the
period never builds a queue of widths nobody will see. The size the window
finally settles at MUST be sent. Waiting for the resize to stop instead, as
the reference client did until 2026-09-28, leaves the page at its old layout
for the whole drag and moves it only once the hand is still — a server render
of the landing page (`site`) is 4–7 ms, so the wait was the delay.

## 4. Ordering and recovery

`Batch` frames carry a monotonically increasing sequence number. A client
applies them in order and MUST NOT reorder or skip. If a batch fails to apply —
an op referencing an unknown node id, an atom id that was never defined — the
client MUST NOT attempt partial application: it discards its tree, sends
`Resync`, and waits for a fresh `Mount`.

This is deliberately unforgiving. A patch stream that has drifted is a bug on
one side or an attack from the other; both are better served by a clean rebuild
than by a heuristic.

`Resync` is not an error. The client MUST NOT send an `Error` frame for a
batch it could not apply — `Error` ends the session on both sides, which is the
opposite of what a resync is for. The server answers `Resync` with a `Mount`
that references the session's existing tables and MUST NOT repeat definitions
the session already holds (§02 §2: tables are never cleared).

A server whose view cannot be encoded — it names an event kind, a role or a
value the protocol does not have — MUST send `Error` (code `400`) with the
reason and end the session. Such a view fails identically on every later
render, and a server that only logs it leaves a window that looks alive and
answers nothing.

Whatever ends a session, the client SHOULD **show** the reason rather than
leave the last frame standing: a window that stopped talking to its
application must not look like one that is merely idle.

### 4.1 Resuming a session

A session belongs to the server. The socket under it does not: a wifi hop, a
VPN reconnect, a laptop lid and a proxy's idle timeout all end a socket
while both ends are still willing. A client that treated those as the end of
the application would lose a half-filled form to a change of network, which
is not a property anyone would choose.

So a client whose socket closed without an `Error` **SHOULD** open another
one, and it MUST NOT tear its tree down before it knows what the server
says. The reference client waits 300 ms, doubling to 30 s and no further,
and shows that it is doing so (§4).

`Hello` carries the offer:

```
resume := 0x00                       -- nothing; a first socket
        | 0x01 session:16  acked:varint
        | 0x02 tree:32                -- a tree fetched over §2.4, not a session
```

`acked` is the highest batch sequence the client has applied. The server
answers in `Welcome`:

(`0x02` is §2.6's; the rest of this section is about `0x01`.)

- **`start = Resumed`** — the session named is still here, nothing else is on
  it, and the server can still send everything after `acked`. The session
  id MUST be the one the client offered. The client keeps its tree, its
  tables, its focus and what was typed into it; the server then sends the
  batches after `acked`, in order, and the session goes on.
- **`start = Fresh`** — a session that starts empty, whether or not one was
  offered. A client that was holding a tree MUST discard it, along with its
  tables and everything keyed to them, before it applies the `Mount` that
  follows. This is also every first `Hello`'s answer.

A client MUST believe that answer over its own memory: a tree kept against a
server that has forgotten the session would answer clicks the server cannot
place. A `Welcome` with `start = Resumed` naming a session the client did not
offer is a protocol error.

A server decides for itself how long a session outlives its socket and how
many batches it can replay; both are quotas like any other. The reference
server keeps a session for two minutes and the last 64 unacked batches, and
refuses the resume — rather than half-serving it — when either runs out.
A session id is a bearer credential for the whole session: it MUST come
from a CSPRNG, and it MUST NOT be handed to a second socket while a first
is still on it.

Because a batch already applied may be replayed, a client MUST ignore a
batch whose sequence it has already applied, and ack it again. Ops are not
all idempotent — a `SetText` is, an `InsertChild` is not — so this is the
receiver's job, not the sender's.

What the person does while there is no socket MAY be held and sent once the
server has answered, and what a client holds it MUST bound. A client SHOULD
NOT hold what a clock raised — `wake`, `location`, `timeupdate`, `level` —
which says what was true at a moment that has passed and will be said again
once there is somebody to tell, and SHOULD fold a report of state (`change`,
`scroll`, `resize`, a pointer or file-drag position) into the one it holds
for the same node and event, but not across anything else the person did in
between. An upload SHOULD NOT be read while there is no socket to take it.
The reference client holds at most 256 frames and 1 MiB, and drops a frame
past either rather than an older one ([`10-budgets.md`](10-budgets.md) §5).

## 5. Idle behaviour

`Ping` is sent by whichever side has been silent for 30 s. A client MUST NOT
poll, MUST NOT keep a timer that wakes it when nothing has changed, and MUST NOT
redraw unless a frame, an input event, or a window event asked it to. The
zero-wakeup idle budget in [`10-budgets.md`](10-budgets.md) is a property of
this rule.

The one exception is a timer the application asked for in the tree: a node
carrying a `wake` prop and a `wake` handler ([`06-events.md`](06-events.md)
§1.1) is woken on its period, within the bounds that section sets. It is not a
poll the client invented; it is the only way a view can watch something the
client cannot see, and it stops with the prop.

## 6. Files

A file is not an asset. An asset is named by its content, is the same for
everyone, and may be served to anyone by anything in between (§2.2) — which
is exactly wrong for the invoice one person attached and the export another
asked for. Files travel **in the session**, so they are authenticated by it,
scoped to it, end with it, and are cacheable by nothing.

Both directions use one shape:

```
transfer := id:varint  seq:varint  flag:u8  bytes:len-prefixed
flag     := 0x00 more | 0x01 last | 0x02 aborted
```

`seq` counts from 0 and MUST be contiguous; a receiver MUST reject a gap. On
`aborted`, `bytes` is a UTF-8 reason of at most 256 bytes, not content, and
everything already received MUST be discarded. `bytes` is at most 256 KiB,
so a transfer interleaves with everything else the session is doing rather
than filling a frame with a file.

**`Upload` (C→S)** carries a file the person picked. `id` is the upload id
the client minted and named in the `file_pick` event that opened it
([`06-events.md`](06-events.md) §1); the metadata is in that event and never
repeated here. A server MUST NOT accept an `Upload` for an id it has not
seen announced, and a client MUST NOT send one for a `pick` the person did
not answer.

**`Blob` (S→C)** carries what a node's `save` offers. `id` is the **node**
whose `save` the person answered — there is exactly one such transfer per
node at a time. A client MUST refuse a `Blob` for a node it has no open
save for: that is a server trying to write a file nobody offered it, and it
ends the session ([`08-security.md`](08-security.md) §7.1).

The ceilings are in [`10-budgets.md`](10-budgets.md) §5. Nothing here is a
stream in the general sense: there is no seeking, no resumption of a
transfer across sockets, and a transfer whose session ends is gone.

## 7. A session over a pipe

Status: implemented in the reference client from 0.8.0 (`eui --pipe`) and in
the Ruby server (`clients/eui-ruby`, `EUI::Pipe`); no other server speaks it
yet.

Everything above assumes the application lives on a server and the client
reaches it across a network it does not trust. An application on the
person's own machine — a Ruby script, a Go binary, a tool somebody ran from
a terminal — has no origin, no certificate and nothing to serve, and making
it bind a port so that a client can dial it back costs a listening socket
any other process of the user can reach, a cookie to keep them out of it,
and an environment variable that says "insecure" about a connection that
never left the machine.

So such an application **starts the client itself** and speaks to it over
the client's standard input and output. The session is the same session:
the same frames, the same `Hello` and `Welcome`, the same batches, events
and files. What changes is who opens it, how assets travel, and what is
trusted.

### 7.1 Opening

The application spawns the client with its standard input and output
connected to two pipes the application holds. The reference client is
asked for this as

```
eui --pipe [--title <name>] [--allow <capability,…>]
```

and from then on:

- **The client's standard output carries frames and nothing else.** A
  diagnostic, a warning or a trace goes to standard error. One stray line
  on standard output is a corrupt frame on the other side, and the session
  ends with it.
- **The client speaks first,** with `Hello`, exactly as it does on a socket.
  A server MUST NOT write before it has read the `Hello`, and answers it
  with `Welcome`.
- **The version is negotiated as on a socket.** A client that speaks the
  pipe speaks at least version 8 and offers its own in `Hello`; the server
  answers with the lower of the two, as everywhere. `Fetch` and `Asset`
  (§7.3) belong to the *transport*, not to a version: a session on a pipe
  carries them whatever version it agreed, because a pipe has no other way
  to move an asset. So a server that only speaks version 4 can still serve
  a pipe — the Ruby server does — and nothing above 4 is sent to it.
- **`resume` MUST be `0x00`.** There is no session to come back to (§7.4) and
  no tree fetched over §2.4, and a server MUST answer anything else with
  `Error`.
- **One pipe is one session is one window.** An application that wants two
  windows starts two clients.

The client does not hand a pipe session to an already running window
process, as it does a launch (08 §10): the pipe belongs to the process that
was spawned, and passing it across the instance socket would need the
descriptors sent with it. The reference client runs every pipe session as
if `--standalone` had been given. A client MAY pass them instead; the first
pixel it saves is the only thing that changes.

*Enforced: `eui-client/src/pipe.rs` (the transport), `eui-client/src/main.rs`
for the flag, the standalone process and keeping standard output clean.*

### 7.2 Framing

Frames follow one another on each pipe with nothing between them, and a
frame's own `len` is what ends it (§3). Everything §3 requires of a frame
still holds: `len` MUST NOT exceed `MAX_FRAME_BYTES`, and a reader MUST
check it **before** reserving room for the payload, since on a pipe nothing
else bounds what a declared length asks for.

The end of a pipe in the middle of a frame is a truncated frame, and ends
the session as any malformed frame does.

*Enforced: `eui_client::pipe::read_frame`, which reads the header a byte at
a time and asks `Frame::framed_len` where the frame ends — the function that
refuses a length past the limit — before it reserves the payload. Ruby:
`EUI::Pipe::Socket#recv`.*

Both pipes MUST be read while the other is written. A pipe holds a few tens
of kilobytes, and two processes each blocked writing into a full one, each
waiting for the other to read, is the classic way a pipe protocol stops
without an error. The reference client reads and writes on two threads of
its own, so its input path never waits on the application; an application
SHOULD do the same, or at least never block writing a batch while an event
is waiting to be read.

### 7.3 Assets

There is no origin to `GET` an asset from, so on a pipe an asset travels in
the session, beside the batches:

```
Fetch := hash:32  cap:varint
Asset := hash:32  seq:varint  flag:u8  bytes:len-prefixed
flag  := 0x00 more | 0x01 last | 0x02 aborted
```

**`Fetch` (C→S)** asks for the asset named by `hash`, at most `cap` bytes
of it: the session's remaining asset budget, measured exactly as §2.2
measures it. A client MUST NOT ask again for a hash that is still in flight.

**`Asset` (S→C)** answers it in chunks, in the shape of §6's transfers with
the hash in place of an id: `seq` counts from 0 and MUST be contiguous,
`bytes` is at most 256 KiB, and on `aborted` it is a UTF-8 reason of at most
256 bytes rather than content. A server that does not hold the hash answers
with a single `aborted` chunk — the `404` of §2.2 — and one whose asset is
larger than `cap` answers the same way, without sending any of it.

- A client MUST refuse an `Asset` for a hash it has not asked for, and an
  `Asset` whose bytes pass `cap`; either is a protocol error and ends the
  session.
- A client MUST recompute the BLAKE3 of the whole asset on `last` and
  discard it on a mismatch, exactly as over HTTPS. The pipe is not hostile,
  but the check is what makes "the name is the content" true for every
  cache the client keeps, and a cache that holds one unverified entry holds
  no guarantee about the rest.
- A server SHOULD interleave batches between the chunks of an asset, rather
  than send a whole picture before the next `Batch`. A frame is applied in
  order, and an asset that fills the pipe for a second is a second the
  window does not move.

What arrives is held, counted and let go exactly as an asset fetched over
HTTPS is ([`10-budgets.md`](10-budgets.md), *Assets*).

*Enforced: `eui_proto::Frame::decode` refuses both kinds as unknown (it is
what the socket and the worker decode with), and `Frame::decode_pipe` reads
them; `eui_client::pipe::Assets` keeps what is in flight, refuses the
unasked, the out-of-order and the over-cap, and checks the hash on `last`.
Ruby: `Session#serve_asset`, which refuses a hash it does not hold, or one
larger than `cap`, with one `aborted` chunk, and sends the chunks of an
asset one after another — it does not yet interleave batches between them.*

### 7.4 Ending

A pipe session ends when either side closes its end, and it does not
resume: there is no other pipe to resume it on, and the process that held
the session is the one that went away.

- **The person closes the window:** the client closes its standard output
  and exits. The application reads the end of the pipe and SHOULD exit in
  turn; it has no window left.
- **The application ends:** the client reads the end of its standard input.
  If an `Error` came before it, the client shows the reason, as §4 asks of
  every ended session; if none did, the application has finished, and the
  client closes the window and exits with status 0. An application that
  wants its reason seen sends `Error` first.
- **The client is killed:** the application reads the end of the pipe, and
  a write to it fails rather than raising a signal the application did not
  ask for. A server SHOULD ignore `SIGPIPE`, or write in a way that reports
  the failure instead.

`Ping` keeps a proxy's idle timer from closing a socket, and a pipe has
neither: neither side needs to send one, and a closed pipe is seen at once.
A side that receives a `Ping` still answers it with `Pong`.

*Enforced: `eui_client::pipe::ending` decides between the reason and the
exit, and `Tab::pump` acts on it — never `lost()`, so a pipe is never
redialled. Ruby sends no `Ping` on a pipe (`Session#pump`).*

### 7.5 What does not apply

Several things a socket session depends on have nothing to stand on here,
and a client MUST NOT try:

- **No manifest, no pin, no remembered grant.** There is no origin to fetch
  one from and none to key a pin or a grant under (§2.1). The window's
  title is the one `--title` gave, or the client's own name.
- **No islands.** An `island` is a path on the page's origin (§2.7), and a
  pipe session has none: the node keeps the children it came with, which is
  the degradation §2.7 already describes for an island that cannot open.
- **No one-shot render and no adoption** (§2.4, §2.6): there is no URL to
  fetch a component from.

*Enforced: `Tab::open` returns before the manifest, the pin store and the
one-shot render are reached; `dial::island_url` has no address for a pipe
session, so `Tab::dial_islands` opens none.*

### 7.6 Trust

A capability defends the person against a server they do not control, on a
machine that is not theirs. The process at the other end of a pipe is
neither: it was started by the person, it runs as them, and it can already
open their camera, read their files and post their notifications without
asking anybody. A consent sheet in front of it would be theatre.

So on a pipe **the grant is what the command line says**, `--allow` on the
reference client, and the command line is written by the application. That
is not a hole: anything that can choose the client's arguments can choose
to do the thing itself.

What the pipe does **not** change is what is parsed. The frames still go to
the client's confined worker (08 §10), because an application that renders
what it read from the network — a message, a feed, a file somebody sent —
relays bytes nobody on the machine chose, and a bug in the decoder is the
same bug whoever handed it the bytes.
