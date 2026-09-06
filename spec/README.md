# EUI/1 — Specification

EUI is a protocol for delivering application user interfaces over HTTPS without
HTML, CSS, or JavaScript. A server sends an **already-resolved interface tree**
in a compact binary encoding; a native client applies it, lays it out, and draws
it on the GPU.

The specification is written in English to match the rest of the Soli
documentation corpus and because a protocol is meant to be implemented by people
who did not write it.

## Documents

| File | Status | Contents |
|---|---|---|
| [`00-rationale.md`](00-rationale.md) | draft | Why this exists, what it deliberately refuses to do |
| [`01-transport.md`](01-transport.md) | draft | HTTPS discovery, manifest, `wss://` session, framing |
| [`02-wire-format.md`](02-wire-format.md) | **normative** | Atoms, styles, nodes, patches — byte level |
| [`03-widgets.md`](03-widgets.md) | planned | Primitive node kinds and their semantics |
| [`04-layout.md`](04-layout.md) | **normative** | The layout algorithm; §9 lists what v1 omits |
| [`05-theme.md`](05-theme.md) | **normative** | Roles, scales, modes, the resolution algorithm |
| [`06-events.md`](06-events.md) | **normative** | Event kinds, payloads, dispatch and emission rules |
| [`07-bytecode.md`](07-bytecode.md) | planned | Verified bytecode subset, host surface, metering |
| [`08-security.md`](08-security.md) | planned | Threat model and normative requirements |
| [`09-conformance.md`](09-conformance.md) | planned | Test vectors and how to run them |
| [`10-budgets.md`](10-budgets.md) | planned | Performance budgets, enforced in CI |

## Conventions

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, MAY, and OPTIONAL are to be interpreted as described in RFC 2119.

Byte order is **little-endian** for every fixed-width field. Variable-length
integers are LEB128 unsigned unless a field is explicitly declared signed, in
which case it is LEB128 over a zigzag encoding.

A **conforming client** MUST reject any frame that violates a MUST in this
specification, and MUST do so without panicking, aborting, allocating
unboundedly, or entering a non-terminating loop. Rejection means: close the
session with an `Error` frame and a diagnostic code. There is no error recovery
and no "quirks mode" — the tolerant parser is the thing EUI exists to delete.
