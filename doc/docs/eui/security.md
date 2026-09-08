# Security model

Each item below is a requirement of the design, and each will be a MUST in
`spec/08-security.md` when that document is written — it is not yet. Where
something is built, this page says so; everything else is design.

## Transport

TLS 1.3 only, with no downgrade path and no exception for loopback in a release
build. A manifest may declare an SPKI pin set for its origin.

## Provenance

The manifest and every asset are signed with Ed25519. The client pins the
publisher key on first run and refuses a different key later unless a rotation
record signed by the pinned key accompanies it.

Assets are addressed by BLAKE3 hash and verified on arrival, so neither a CDN
nor a proxy can substitute content. The name *is* the content.

## Capabilities

Deny by default. A manifest declares what it wants — `camera`, `microphone`,
`clipboard.read`, `clipboard.write`, `notifications`, `location`, `fs.pick`,
`fs.save` — and the user grants per application, revocably.

The important part is not the prompt. It is that **a capability that was not
granted has no code path in the client**: the check is not "is this allowed?" at
the call site, because the call site is absent. Unknown capability bits in a
frame are rejected outright.

## No code from the network

No native code. No JIT. No `eval`. Only bytecode that passes a verifier — to be
specified in `spec/07-bytecode.md` — with an opcode allowlist, stack-effect
validation, constant-pool bounds, jump-target checks, and a maximum chunk size,
all before the first instruction runs.

This structurally removes the entire browser-JIT exploit class. There is no
speculative compiler to confuse, because there is no compiler.

## Memory safety — **built**

- `#![forbid(unsafe_code)]` on every decoding crate. `unsafe` will be confined
  to the wgpu FFI boundary in `eui-render` and nowhere else.
- `eui-proto` has **no dependencies at all**. Everything it touches came off a
  network socket, so a dependency there is attack surface we did not write and
  cannot fuzz on our own schedule.
- The decode path is clean under `clippy` with `indexing_slicing`, `panic`,
  `unwrap_used`, `expect_used` and `arithmetic_side_effects` denied. It cannot
  panic by construction, not merely by inspection.
- 46 rejection cases, plus bulk tests that push 40 000 random and
  bit-flipped buffers through every entry point and require that none panic.
- `eui-tree` applies the same discipline one layer up: a batch whose ops are
  well-formed but incoherent — an undefined atom, a duplicate node id, a
  child index past the end — is refused before anything is placed.
  `cargo fuzz` targets on the frame decoder, tree decoder, chunk verifier and
  layout engine follow.

## Quotas

Enforced by the client, checked before the memory they bound is allocated: node
count, atom count and total atom bytes, style count, asset bytes, texture
memory, frame time, and chunk fuel. A hostile server can be annoying. It cannot
make the client exhaust itself.

## Input isolation

An application never sees a keystroke outside its own focused editable node.
There is no global key capture, no clipboard read without a capability, and no
pointer polling. The keylogger shape is not available, rather than being
forbidden by policy.

## Privacy by default

No user-agent string, no font enumeration, no canvas readback, no device
identifier, and no third-party connection — the manifest's host allowlist is
enforced. The fingerprinting surface is close to nil, and that is a **testable**
goal, not a slogan.

## Server side

Every event is validated against the schema of the node it names. A client
cannot invent an event on a node it never received, nor a value outside the
declared domain. A local handler's effect is advisory and is re-derived
server-side before anything is trusted.

## Process isolation

The decoder, the tree, layout, text shaping, PNG decoding and the VM run
in a worker process; the window keeps the display, the GPU, TLS, the pin
store, the clipboard and the accessibility adapter, and never decodes a
frame. On Linux the worker confines itself before its first byte: Landlock
denies every file and socket, a seccomp allowlist of the system calls its
loop needs kills on any other, and the process is not dumpable, so a kill
leaves no core of the session on disk. A dead worker ends the session
with a reason and the window stands. macOS (`sandbox_init`) and Windows
(AppContainer) are not done: the worker is its own process there, a crash
is contained, a compromised worker is not confined, and the window says
so. `EUI_SANDBOX=0` runs the driver in the window process for debugging.
