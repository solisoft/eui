# Transport

EUI runs over HTTPS. There is no EUI port, no new TLS profile, and no new
certificate story: an EUI application is served from an ordinary origin, behind
ordinary proxies and CDNs.

## Requirements

- **TLS 1.3, no exceptions.** A client refuses `http://` origins, loopback
  included, in any release build.
- No cross-origin redirect during discovery.
- No user-agent string and no client identifier, ever.

## Endpoints

| | |
|---|---|
| `GET /.well-known/eui` | The application manifest |
| `GET /_eui/asset/<blake3-hex>` | A content-addressed asset |
| `wss://host/_eui/session` | The interactive session |
| `POST /_eui/rpc` | One-shot render, for applications that are not interactive |

### The manifest

The manifest carries the application id, the protocol range it supports, an
Ed25519 public key, a signature over everything else, the capabilities it wants,
and the hash of its default theme.

A client verifies the signature before acting on any other field. On first run
it **pins** the publisher key for that application id. A different key on a later
run is refused unless the manifest also carries a rotation record signed by the
previously pinned key.

### Assets

An asset is named by the BLAKE3 hash of its content. The client recomputes the
hash and discards a mismatch. Because the name *is* the content,
`Cache-Control: immutable` is always correct, and a hostile CDN cannot
substitute anything.

### The session

A WebSocket over TLS, carrying binary frames only. A text frame closes the
session.

WebSocket rather than HTTP/3 for version 1 because it traverses every proxy in
existence today and Soli already speaks it. The framing layer is specified
independently of the transport, so HTTP/3 with WebTransport can be substituted
later without changing a single message byte.

## Framing

```
frame := kind:u8  len:varint  payload
```

| kind | Name | Direction |
|---|---|---|
| `0x01` | `Hello` | client to server |
| `0x02` | `Welcome` | server to client |
| `0x03` | `Batch` | server to client |
| `0x04` | `Event` | client to server |
| `0x05` | `Ack` | client to server |
| `0x06` / `0x07` | `Ping` / `Pong` | either |
| `0x08` | `Error` | either |
| `0x09` | `Resync` | client to server |
| `0x0A` | `Viewport` | client to server |

Any other kind is rejected. Unknown kinds are **not** reserved for forward
compatibility: version negotiation in `Hello` and `Welcome` is the only
extension mechanism, so a client never has to guess at semantics.

A frame's declared length must account for every byte of the message. Trailing
bytes are an error, not padding.

## Ordering

Batches carry a monotonically increasing sequence number and apply in order,
all or nothing. If one fails — an op naming a node that does not exist, an atom
id that was never defined — the client does not attempt partial application. It
discards the tree, sends `Resync`, and waits for a fresh `Mount`.

## Idle

Whichever side has been silent for 30 seconds sends a ping. A client does not
poll, does not keep a timer that fires when nothing has changed, and does not
redraw unless a frame or an input event asked it to. The zero-wakeup idle budget
is a property of that rule, not of a setting.
