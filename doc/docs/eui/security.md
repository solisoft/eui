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
`fs.save`, `nfc`, `scene` — and the user grants per application, revocably.

`scene` is the odd one and worth its own sentence: it is not a device, it is
permission to run a graphics program the application wrote. What the person
is told is both halves of that — it spends their graphics card and can slow
the machine down, and it still cannot read what is on their screen.

And it guards the program, not the feature. A `scene` that names no shader
draws without the grant: it is the client's own program over the client's own
shape, and asking someone to allow that would be a toll on nothing — which is
how people learn to grant without reading.

An application asks for them in its routes, and asking grants nothing:

```soli
# config/routes.sl
eui_capabilities("clipboard.read", "notifications")
```

The important part is not the prompt. It is that **a capability that was not
granted has no code path in the client**: the check is not "is this allowed?" at
the call site, because the call site is absent. Unknown capability bits in a
frame are rejected outright.

## No code from the network

No native code. No JIT. No `eval`. Exactly two kinds of executable content
reach the client, and each has a verifier of its own before the first
instruction runs.

**Bytecode** (`spec/07-bytecode.md`): an opcode allowlist, stack-effect
validation, constant-pool bounds, jump-target checks, a maximum chunk size,
and fuel that bounds a run while it is running.

**A shader** (`spec/11-shaders.md`), and only where the `scene` capability was
granted. A fragment stage cannot be stopped once it is running — it is already
on the GPU when it misbehaves — so its bound has to be proved from its shape
*before* it is compiled: every loop's trip count read off constants, a total
step budget, and no storage, no atomics, no images, nothing bound to it but
the uniform block the client hands it.

This structurally removes the entire browser-JIT exploit class. There is no
speculative compiler to confuse, because there is no compiler.

What it does *not* remove, and the specification says so rather than implying
otherwise: a shader still has to be compiled, and a shader compiler belongs to
the graphics driver, which lives in the window process because that is where
the GPU is. Verification makes the input to that compiler small and dull. It
does not make the compiler safe, and no arrangement of processes can, short of
not granting `scene` at all — which is exactly why it is a capability.

## Memory safety — **built**

- `#![forbid(unsafe_code)]` on every decoding crate. `unsafe` will be confined
  to the wgpu FFI boundary in `eui-render` and nowhere else.
- `eui-proto` has **no dependencies at all**. Everything it touches came off a
  network socket, so a dependency there is attack surface we did not write and
  cannot fuzz on our own schedule.
- The decode path is clean under `clippy` with `indexing_slicing`, `panic`,
  `unwrap_used`, `expect_used` and `arithmetic_side_effects` denied. It cannot
  panic by construction, not merely by inspection.
- 55 rejection cases, plus bulk tests that push 40 000 random and
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

## The filesystem

The client reads a file only when a person picked it in the platform's own
dialog, and writes one only where a person just said. Everything else follows
from those two sentences.

A dialog opens on an **activation and nothing else** — a click, or
`Enter`/`Space` on a focused node carrying `pick` or `save`, with a server
handler for the answer and the capability granted. No frame opens one,
because a frame is not a person. `fs.pick` and `fs.save` are separate, and
neither implies the other: reading what someone chose and writing where they
said are different powers.

A `Blob` frame for a node with no open save **ends the session**. There is no
benign reading of a server sending bytes for a file nobody offered it. The
file itself is created on the first chunk rather than when the path is
chosen, and an aborted transfer removes what it had written — half an export
is worse than none, because it looks like a whole one until it is opened.

No path ever reaches a server: a `file_pick` carries a name and a size, a
`file_save` a name. A dismissed dialog is reported to nobody. And the bytes
never enter the worker in the direction that would matter — the window
reads and writes files, because the worker may not open one.

## Privacy by default

No user-agent string, no font enumeration, no canvas readback, no device
identifier, and no third-party connection — the manifest's host allowlist is
enforced. The fingerprinting surface is close to nil, and that is a **testable**
goal, not a slogan.

An application may supply its own typeface without moving any of that. A face
is an asset: named by the BLAKE3 of its bytes, fetched from the session's own
origin, checked against its own name, and parsed inside the confined worker.
The client never resolves a face by name or by URL and never asks the machine
what it has installed, so a face from a font service is one the *server*
downloaded, once, and re-served — the viewer's address never reaches the third
party. Bytes that are not a face the client reads are discarded, and the role
falls back to sans.

That is a claim about the **client**, and in the standalone client the client
is the whole surface, so it is also a claim about what a person running it
gives away. In the WebAssembly build it is only the first half: the client
still asks for none of those things, but the tab around it is a browser tab,
and the `Origin` and `User-Agent` on the upgrade are the browser's to send.
A page cannot suppress them and this one does not pretend to.

## Server side

Every event is validated against the schema of the node it names. A client
cannot invent an event on a node it never received, nor a value outside the
declared domain. A local handler's effect is advisory and is re-derived
server-side before anything is trusted.

## Process isolation

The decoder, the tree, layout, text shaping, picture decoding and the VM run
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

**In a browser there is no second process, and the trade is not all one
way.** The WebAssembly build runs the driver in the window — the same path
the phones take, for the same reason: there is no second binary to start.
What the engine gives instead is real and is not nothing: a decoder that
cannot corrupt the host's memory, cannot reach the GPU, the socket or the
filesystem except through the imports the module declares, and runs inside
the browser's own site-isolated renderer. What it does not give is the
*availability* half. A panic in the decoder takes the whole module, canvas
and all, where a native worker's death leaves the window standing with a
reason on it. That is a regression against §10 and it is the price of the
demo; the honest fix is a Web Worker speaking the same request/reply wire
the two processes already use, which is written down and not yet built.
