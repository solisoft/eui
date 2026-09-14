# 11 — Shaders

Status: **normative** for version 1 of the shader contract; implemented by
`crates/eui-shader` and `crates/eui-render/src/scene.rs`.

A `scene` (03 §1.2) is drawn by a program the **server** chose. That is the
second exception to `00-rationale.md`'s refusal of code from the network, and
the only one besides the bytecode of 07. This document is what makes it an
exception rather than a hole: the rules a module must satisfy, where they are
checked, and — in §6 — what is given up even when every one of them holds.

The order matters. A shader cannot be stopped once it is running: a fragment
stage is already on the GPU when it misbehaves, and there is no equivalent of
07 §5's fuel to cut it off. So everything here is proved from the module's
**shape**, before it is compiled, or it is not proved at all.

## 1. The container

A module travels as an asset, in a container:

```
"EUIS" version:u8=1  source:bytes      -- UTF-8 WGSL
```

Recognised by its leading bytes and never by a name the server sent, which is
03 §1's rule for every asset. Bare WGSL is not a shader asset. A mesh's
container is `"EUIM"` (03 §1.2), a chunk's is `"EUIC"` (07 §3).

## 2. What a module must be

A conforming client MUST refuse a module that fails any of the following, and
MUST refuse it **before** handing it to a shader compiler.

### 2.1 Size

| Measure | Limit |
|---|---:|
| `source` | 64 KiB |
| Functions, entry points included | 64 |
| Statements, across every function | 4 096 |
| Expressions, across every function | 16 384 |
| Types | 256 |

64 KiB is `MAX_CHUNK_BYTES` (02), and the same number for the same reason: it
is the size at which a thing the network sends stops being a handler and
starts being a program.

### 2.2 Entry points

Exactly one fragment entry named `fs_main`, returning `@location(0) vec4<f32>`.
At most one vertex entry named `vs_main`. **No compute stage.** No early depth
test.

The names and the fragment result are fixed rather than declared, so that no
identifier and no `@location` a server chose ever reaches pipeline creation.

### 2.3 Bindings

The only global a module may bind is the client's uniform block, at **group 0,
binding 0**, and it MUST be a struct of exactly 128 bytes — the block of §3.
A module does not define that struct; it receives it.

Refused outright: storage buffers in any access mode, push constants,
workgroup memory, textures, samplers, storage textures, atomics, and
runtime-sized arrays. A module therefore has nowhere to put a result except
its own colour, and nothing to read except what the client handed it.

Version 1 admits no texture. That is a real limitation and it is deliberate:
it keeps the surface closed while the rest of the design is proved, and it can
be opened later without changing anything already written here.

### 2.4 Termination

WGSL forbids recursion and a conforming front end rejects call cycles, so the
only way a module can fail to finish is a loop.

A loop is admitted only when its trip count can be read off constants:

- the counter is a local with a constant initial value;
- the loop's step is a single assignment of that counter plus or minus a
  non-zero constant, in the continuing block;
- the loop opens with a conditional break comparing that counter against a
  constant;
- and nothing in the body assigns the counter.

Anything else — an unbounded `loop`, a bound read from the uniform block, a
`break if` in a continuing block, a counter the body also moves — MUST be
refused. **A bound that comes from a uniform is not a bound**: a uniform is a
number the server sends after the module was verified.

Nested loops **multiply**. The product, counted in statements, MUST NOT exceed
**4 096** steps for any one invocation. That is 07 §5's fuel, deliberately:
one budget and one argument for both kinds of code a client runs.

### 2.5 Dynamic indexing

An index that is not constant-evaluable MUST be refused.

This is not redundant with the host's bounds checking. On a GLES backend a
conforming implementation of WebGPU generates **unchecked** indexing, because
the safety is left to GLSL and GLSL's answer to an out-of-range index is
undefined behaviour. A client SHOULD additionally refuse the `scene`
capability on a GL backend, and a client that does not MUST say so.

*The reference client does both.* `eui-shader::verify` admits only a constant
index — which costs nothing, because §2.3 binds a module no array to walk —
and `Renderer::grants_scenes` is false on a GL adapter, so the capability is
never granted there and the module is never fetched. Two refusals for one
hazard, because the GLES translator is the least exercised path in the stack
and the shader is the attacker's.

## 3. The uniform block

128 bytes, and the only thing bound to a module:

| Field | Bytes | Whose |
|---|---:|---|
| `mvp: mat4x4<f32>` | 64 | the client's |
| `time: vec4<f32>` — `(time, age, 0, 0)` seconds | 16 | the client's |
| `size: vec4<f32>` — `(w, h, scale, aspect)` device px | 16 | the client's |
| `params: vec4<f32>` | 16 | the author's |
| `tint: vec4<f32>` | 16 | the author's |

The camera is the client's, always. **A server never sends a matrix**, and so
can never send a degenerate one; it sends a node's eight floats and a clock
bit, and the client builds the rest.

## 4. Where it is checked

Verification runs in the **worker** (08 §10): the parse that meets bytes a
server chose happens under the sandbox, beside the picture decoders and the
bytecode VM, and not beside TLS and the pin store.

The window compiles what passed, inside a validation scope, and MUST NOT
panic on a module the driver refuses — a panic there is the window process,
which holds the display for every session.

The window **verifies again**, with this same document's rules, on a module
the worker already approved. That duplication is required, not incidental: the
worker is the process that reads what a server sent, so a worker that has been
taken over MUST NOT be able to mark a module verified that is not. Re-parsing
in the host's own front end is not enough on its own — that buys memory safety
and knows nothing of bounded loops, of there being no compute stage, or of the
one binding a scene has. A mesh's indices are re-checked on the same crossing
and for the same reason.

## 5. Failure

A module that fails verification, fails compilation, or names a mesh that
fails to decode, draws nothing: its node paints its own background, exactly
as a node with no content does. This MUST NOT end the session.

**Version 1 tells the server nothing at all.** There is no `failed` event: a
scene that did not draw is indistinguishable, from the server's side, from one
that did — and so is a client that was never granted the capability, and one
whose adapter is a backend §2.5 refuses. That is the strong version of 08 §8's
promise rather than a gap in it, and it costs an author a way to know their
shader was rejected, which the log and the verifier's own message give them
instead.

Should a later version add such an event, its payload MUST be drawn from a
closed vocabulary — `unsupported`, `shader`, `mesh`, `texture`, `budget`. A
compiler's diagnostic MUST NOT be forwarded under any version: it names the
driver, and through it the machine.

## 6. What this does not prove

Stated here because the rest of this document reads like a guarantee and this
part is not one.

1. **A verified module can still be slow enough to trip a driver's watchdog.**
   The step bound is per invocation. The number of invocations is the size of
   the target, which the client chooses, and the cost of a step is the
   hardware's. The real cost is `steps × fragments × hardware`, and only the
   first factor is bounded here. A client MUST therefore treat a lost device
   as a session that ends with a reason (08 §10), and MAY refuse the
   capability to a publisher whose scene has already cost it one.

2. **The platform's shader compiler runs in the window process, on input the
   server chose.** Verification makes that input small and dull. It does not
   make the compiler safe, and it cannot: the compiler belongs to the driver,
   and the driver is where the GPU is. This is the largest attack surface the
   capability adds, and it is irreducible.

3. **A scene's pixels are not a conformance surface.** Two conforming clients
   may draw the same scene differently — `sin`, the filtering, the rasteriser's
   fill rule and the point of sRGB conversion are all the adapter's. A server
   that depends on a scene's exact pixels depends on something this protocol
   does not promise. What conformance pins is this document's verdicts, the
   structure of the frame, and that nothing about the scene reaches the server
   (09 §11).
