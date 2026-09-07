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
  the pinned key accompanies it.
- Every asset is named by BLAKE3 and verified on arrival; a mismatch is
  discarded. A CDN or proxy cannot substitute content.

*Status: specified; the manifest and asset paths are not yet implemented.*

*Enforced: `eui-client::manifest` — the signature is checked with the manifest's own key, the key pinned under the `app_id` in the pin store, a changed key refused without a rotation the pinned key signed; `lang/src/serve/eui/manifest.rs` signs with a key generated on first use and kept in `config/eui_publisher.pkcs8`.*

## 3. Capabilities

- Deny by default. A capability the manifest never requested, or the user
  never granted, has **no code path** in the client: the check is not at the
  call site, the call site is absent.
- Unknown capability bits in a frame are a decode error. *Enforced:
  `eui-proto::Frame::decode`.*

## 4. Code from the network

- No native code, no JIT, no `eval`. The only executable content is a chunk
  that passed the verifier in [`07-bytecode.md`](07-bytecode.md) §4.
  *Enforced: `eui-vm::Chunk::verify`.*
- A chunk's reachable surface is the six methods of the host trait: local
  state, node text and props by key, and an event queue. *Enforced by the
  type: `eui-vm::Host`.*
- A run is fuel-metered and string-bounded; an abort has no further effect
  and sends nothing. *Enforced: `eui-vm::run`, `Driver::emit`.*

## 5. Memory safety and hostile input

- `#![forbid(unsafe_code)]` on every crate but the GPU boundary.
- The decode path cannot panic: bounds-checked reads, minimal varints,
  rejected unknown discriminants, rejected trailing bytes, every limit
  checked before allocation, iterative tree decoding. *Enforced: `eui-proto`,
  under `clippy::indexing_slicing`, `panic`, `unwrap_used`, `expect_used`,
  `arithmetic_side_effects` as errors; 46 rejection tests; 40 000 hostile
  buffers per `cargo test`; four `cargo fuzz` targets.*
- A batch that is well-formed but incoherent — undefined atom, duplicate
  node, index past the end — is refused before anything is placed, and the
  session is poisoned until the next `Mount`. *Enforced: `eui-tree::Session::apply`.*

## 6. Quotas

Enforced by the client, before the memory they bound is allocated: nodes,
atoms and atom bytes, styles, colours, chunks, children per node, props and
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

## 8. Privacy

No user-agent, no font enumeration (the client shapes with embedded faces
only — *enforced: `eui-text::TextEngine::new`*), no canvas readback, no
device identifier, no third-party connection. The fingerprinting surface is
the viewport frame, and that is a testable claim.

## 9. Server side

Every client event is validated against the tree the server last sent: the
node exists, it carries a handler of that kind naming that atom, the payload
has the declared shape. A local handler's effect is advisory until the server
re-derives it. *Enforced: `lang/src/serve/eui/session.rs::validate`.*

## 10. Not yet

Process isolation — decoding and the VM in a child under seccomp and
Landlock on Linux, `sandbox_init` on macOS, AppContainer on Windows — is
stage three. Until then the decoder's own discipline is the boundary.
