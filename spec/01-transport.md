# 01 — Transport

Status: draft

EUI runs over HTTPS. There is no EUI-specific port, no new TLS profile, and no
new certificate story: an EUI application is served from an ordinary origin,
behind ordinary proxies and CDNs.

## 1. Requirements

- TLS 1.3 is REQUIRED. A client MUST refuse `http://` origins with no exception,
  including loopback in release builds. (`EUI_ALLOW_INSECURE_LOOPBACK=1` MAY
  relax this in debug builds only; a release client MUST NOT honour it.)
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
16, `location` 32, `fs.pick` 64.

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
| `0x01` | `Hello` | C→S | protocol version, viewport, theme mode, density, font scale, granted capabilities |
| `0x02` | `Welcome` | S→C | negotiated version, session id, initial quotas |
| `0x03` | `Batch` | S→C | a sequence of ops (§02 wire format) |
| `0x04` | `Event` | C→S | node id, event kind, payload |
| `0x05` | `Ack` | C→S | last applied batch sequence number |
| `0x06` | `Ping` | both | 8-byte opaque |
| `0x07` | `Pong` | both | echo of the ping payload |
| `0x08` | `Error` | both | code:varint, message: atom or inline UTF-8 |
| `0x09` | `Resync` | C→S | client state is unrecoverable; send a full `Mount` |
| `0x0A` | `Viewport` | C→S | size, scale factor, theme mode, density, font scale changed |

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

## 5. Idle behaviour

`Ping` is sent by whichever side has been silent for 30 s. A client MUST NOT
poll, MUST NOT keep a timer that wakes it when nothing has changed, and MUST NOT
redraw unless a frame, an input event, or a window event asked it to. The
zero-wakeup idle budget in [`10-budgets.md`](10-budgets.md) is a property of
this rule.
