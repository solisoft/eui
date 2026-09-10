# Transport

EUI runs over HTTPS. There is no EUI port, no new TLS profile, and no new
certificate story: an EUI application is served from an ordinary origin, behind
ordinary proxies and CDNs.

## Requirements

- **TLS 1.3, no exceptions.** A client refuses `http://` origins, loopback
  included, in any release build — and offers no earlier version, so a server
  answering with TLS 1.2 is refused rather than accommodated.
- **The roots your machine trusts.** Verification is against the public web's
  roots *and* the platform trust store, so a development proxy under a
  `.test` name works with no configuration at all — `mkcert -install` put its
  CA where the browser beside you reads it, and the client reads the same
  place:

  ```bash
  eui wss://app.example.test/_eui/session/gallery
  ```

  `EUI_CA_SYSTEM=0` drops back to the public roots alone. A root that is in
  neither — one carried with a deployment rather than installed — goes in
  `EUI_CA_FILE`, one PEM bundle or several separated by `:`. All three sets
  cover the manifest, the assets and the session alike.

  There is no flag that turns verification off. A client that would accept
  any certificate on request is a client whose TLS means nothing.
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

### The manifest, checked

Before a session opens, the client fetches `/.well-known/eui`, verifies the
publisher's Ed25519 signature, checks the protocol range, and pins the key
under the application's `app_id` — trust on first use. A later manifest with
a different key is refused unless it carries a rotation signed by the pinned
key. Soli serves the manifest for any app with the `eui` feature, signing
with a key it generates on first use into `config/eui_publisher.pkcs8`; an
app asks for capabilities with `eui_capabilities("clipboard.read")` and the
person grants them with `eui <url> --allow clipboard.read`. Only the debug
loopback may connect without a manifest, and says so.

## Assets

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
| `0x0B` | `Upload` | client to server |
| `0x0C` | `Blob` | server to client |

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

## When the socket breaks

A session belongs to the server; the socket under it does not. A wifi hop, a
VPN reconnect, a laptop lid and a proxy's idle timeout all end a socket while
both ends are still willing, and an application that treated those as its own
end would lose a half-filled form to a change of network.

So a client whose socket closed without an `Error` opens another one — after
300 ms, doubling to 30 s and no further — and says so meanwhile rather than
leaving a window that looks alive and answers nothing. Its `Hello` offers the
session back: the id the server named, and the last batch it applied. The
server answers in `Welcome`:

- **resumed** — the session is still here and nothing else is on it. The
  client keeps its tree, its tables, its focus and what was typed into it,
  and the server sends the batches it missed.
- **not resumed** — a session that starts empty. A client holding a tree
  discards it before the `Mount` that follows.

The client believes that answer over its own memory: a tree kept against a
server that has forgotten the session would answer clicks the server cannot
place. Because a replay may repeat a batch the client already applied, a
batch whose sequence has been applied is acked again and otherwise ignored —
a `SetText` would survive being applied twice, an `InsertChild` would not.

The reference server keeps a session for two minutes and the last 64 unacked
batches, and refuses the resume rather than half-serving it when either runs
out.

## Files

A file is not an asset. An asset is named by its content, is the same for
everyone, and anything in between may serve it to anyone — exactly wrong for
the invoice one person attached and the export another asked for. So files
travel *in the session*: authenticated by it, scoped to it, gone with it, and
cacheable by nothing.

One shape, both directions: an id, a chunk index, a flag (more, last,
aborted), and at most 256 KiB of bytes. `Upload` carries a file the person
picked, against the id announced in the `file_pick` event that opened it.
`Blob` carries what a node's `save` offers, addressed to that node — and a
client refuses one for a node it has no open save for, because that is a
server trying to write a file nobody offered it.

What opens either is in [widgets](/docs/widgets): a `pick` or `save` prop, a
handler for the event that answers it, the capability, and a person actually
clicking.

## Idle

Whichever side has been silent for 30 seconds sends a ping. A client does not
poll, does not keep a timer that fires when nothing has changed, and does not
redraw unless a frame or an input event asked it to. The zero-wakeup idle budget
is a property of that rule, not of a setting.
