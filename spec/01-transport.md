# 01 — Transport

Status: draft

EUI runs over HTTPS. There is no EUI-specific port, no new TLS profile, and no
new certificate story: an EUI application is served from an ordinary origin,
behind ordinary proxies and CDNs.

## 1. Requirements

- TLS 1.3 is REQUIRED, and a client MUST offer no earlier version: a server
  that answers `wss://` with TLS 1.2 is refused, not accommodated. A client
  MUST refuse `http://` origins with no exception, including loopback in
  release builds. (`EUI_ALLOW_INSECURE_LOOPBACK=1` MAY relax this in debug
  builds only; a release client MUST NOT honour it.)
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
| `POST` | `/_eui/rpc` | One-shot render, for non-interactive applications |

### 2.1 Manifest

`GET /.well-known/eui` returns `application/vnd.eui.manifest` — an EUI-encoded
record (see [`02-wire-format.md`](02-wire-format.md) §7) carrying:

- `app_id`, `name`, `version`
- `protocol_min`, `protocol_max` — supported EUI versions
- `publisher_key` — Ed25519 public key, 32 bytes
- `signature` — Ed25519 over the manifest body excluding the signature field
- `capabilities` — the set requested; the client grants none of them implicitly
- `theme` — blake3 of the default theme asset
- `entry` — session path, defaults to `/_eui/session`
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
| 10 | `signature` | `Str`, 128 hex digits; always last |

Strings are at most 256 bytes. The signature is Ed25519 over the record
encoded with fields 0–9 only (`field_count` 10), which a decoder can rebuild
exactly because the order is fixed. Capability names and bits: `camera` 1,
`microphone` 2, `clipboard.read` 4, `clipboard.write` 8, `notifications`
16, `location` 32, `fs.pick` 64, `fs.save` 128, `nfc` 256.

The set grows from the top and a bit outside it is a decode error
([`08-security.md`](08-security.md) §3). That is the point rather than a
limitation: a client that does not know what a bit means must not agree to
it, and an application that asks for one the client has never heard of
finds out at the manifest rather than at the call.

A client MUST verify the signature before acting on any other field, and
MUST refuse a server whose protocol range excludes its own version. On first
run it pins `publisher_key` for `app_id` (trust on first use). On later runs
a different key MUST be rejected unless `rotation` names the pinned key and
carries that key's Ed25519 signature over the new `publisher_key`; the pin
then moves. A manifest that fails any of this ends the connection before a
session is opened. The one exception is the debug loopback of 08 §1: over
`ws://` on `127.0.0.1` a client MAY proceed without a manifest, and MUST say
so on its diagnostics.

The client grants the intersection of `capabilities` with what the person
allowed it — on the reference client, `--allow` on the command line — and
reports it in `Hello.granted`; nothing is granted by being asked for.

### 2.2 Assets

Assets — images, fonts, bytecode chunks, themes — are named by the BLAKE3 hash
of their content. A client MUST recompute the hash and MUST discard a mismatched
response. Because the name *is* the content, `Cache-Control: public, max-age=31536000,
immutable` is always correct and a hostile CDN cannot substitute content.

An asset response larger than the session's remaining asset budget
([`10-budgets.md`](10-budgets.md)) MUST be abandoned mid-stream.

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

## 3. Framing

Each WebSocket binary message carries exactly one frame:

```
frame  := kind:u8  len:varint  payload:len×u8
```

`len` MUST NOT exceed `MAX_FRAME_BYTES` (8 MiB). A client MUST reject a frame
whose declared length does not match the remaining message bytes exactly —
trailing bytes are an error, not padding.

| kind | Name | Direction | Payload |
|---|---|---|---|
| `0x01` | `Hello` | C→S | protocol version, viewport, theme mode, density, font scale, granted capabilities, and a session offered back (§4.1) |
| `0x02` | `Welcome` | S→C | negotiated version, session id, whether the offered session was resumed (§4.1) |
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

Any other `kind` MUST be rejected. Unknown kinds are not reserved for
forward compatibility; version negotiation in `Hello`/`Welcome` is the only
extension mechanism, so that a client never has to guess at semantics.

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
```

`acked` is the highest batch sequence the client has applied. The server
answers in `Welcome`:

- **`resumed = 1`** — the session named is still here, nothing else is on
  it, and the server can still send everything after `acked`. The session
  id MUST be the one the client offered. The client keeps its tree, its
  tables, its focus and what was typed into it; the server then sends the
  batches after `acked`, in order, and the session goes on.
- **`resumed = 0`** — a session that starts empty, whether or not one was
  offered. A client that was holding a tree MUST discard it, along with its
  tables and everything keyed to them, before it applies the `Mount` that
  follows. This is also every first `Hello`'s answer.

A client MUST believe that answer over its own memory: a tree kept against a
server that has forgotten the session would answer clicks the server cannot
place. A `Welcome` with `resumed = 1` naming a session the client did not
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
