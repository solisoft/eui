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

## 9.1 Sound

Playing a sound needs no capability: it is output, like drawing, and a
window that can draw can already annoy. What it does need is a bound, and
the client imposes it — at most eight sources at once, decoded bytes
counted against the session's asset quota, and the viewer's own volume
above everything, unreadable by the application. Decoding runs in the
worker with every other decoder; the audio device belongs to the window
process. Recording is not output and is not this: the microphone is a
declared capability (01 §2.1) and is not implemented.

## 10. Process isolation

Everything that reads bytes a server chose — the frame decoder, the tree,
layout, text shaping, the PNG decoder, the bytecode VM — runs in a
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
applied to every thread.

`EUI_SANDBOX=0` runs the driver in the window process, for debugging; the
window prints which of the two it did and what the sandbox enforced. macOS
(`sandbox_init`) and Windows (AppContainer) are not done: there the worker
is still its own process — a crash is contained — but a compromised worker
is not confined, and the window says so.

*Enforced: `eui-client/src/sandbox.rs` (Landlock, seccomp, dumpable),
`eui-client/src/worker.rs` (the boundary and the wire). Tested:
`eui-client/tests/worker.rs` — the counter end to end through a confined
worker, a hostile frame, a dead worker, and self-tests that a file read, a
TCP connect and an exec are refused.*
