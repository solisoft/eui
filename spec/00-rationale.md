# 00 — Rationale

Status: draft

## The problem

A Soli application today is fast on the server and expensive on the client.
The server side is measured and good: ~95 µs of CPU to render a 50-row HTML
template, 95 MB PSS at idle for the whole process against 643 MB for Django.
The client side is a web browser.

To display that 50-row table, the browser must:

1. parse HTML with an error-tolerant parser that cannot fail, only guess;
2. build an untyped, dynamically-shaped DOM;
3. match every CSS selector against every element, and resolve the cascade;
4. compute inherited and specified values into computed values, per element;
5. reflow a tree whose constraints are discovered rather than declared;
6. keep a JavaScript JIT resident, with its attack surface and its memory.

Steps 1, 3, 4 and 6 exist to support *documents authored by strangers over
thirty years*. An application UI does not need them. A counter costs 300 MB of
RAM and a busy core because of machinery it never asked for.

## The move

Stop sending a document to be interpreted. Send an **interface tree that is
already resolved**.

| Browser does at runtime | EUI does where |
|---|---|
| Parse markup | Server emits typed records; client validates, does not parse |
| Match selectors, resolve cascade | Server resolves; client does one array lookup |
| Compute inherited values | Server resolves |
| Reflow an untyped tree | Client lays out a typed tree with a fixed algorithm |
| JIT untrusted JavaScript | Client runs verified, metered bytecode — or nothing |
| Rasterise via a general engine | Client draws rounded rects and glyphs, one shader |

The client keeps only the parts that genuinely must be local: layout (it depends
on the viewport and the user's font scale), text shaping, painting, and the
handful of interactions that must not cost a round trip.

## Three encoding ideas

**Session atom table.** Every repeated string — property names, labels, list
keys — is sent once as `DefAtom(id, bytes)` and referenced by a varint
thereafter. HTML re-emits `<div class="…">` on every row of a table; EUI sends
the atom once for the whole session.

**Computed style table.** `DefStyle(id, record)` defines a fixed-layout 64-byte
record of *computed* style. A thousand table rows share three style ids. There
is no cascade on the client because there is nothing to cascade.

**Node arena.** The tree decodes into one contiguous `Vec<Node>` with `u32`
child/sibling indices. One allocation, cache-friendly traversal, no pointer
chasing, no per-node allocation.

## What EUI deliberately refuses

These are not gaps to be filled later. They are the reason the client is small.

- **No open-ended element vocabulary.** The protocol can express exactly the
  primitive node kinds in [`03-widgets.md`](03-widgets.md). Richer widgets are
  *composed on the server* from those primitives, which is why the widget
  catalogue can grow without shipping a new client.
- **No cascade, no selectors, no specificity, no `!important`.**
- **No arbitrary code from the network.** No native code, no JIT, no `eval`.
  Only bytecode that passes the verifier in [`07-bytecode.md`](07-bytecode.md).
- **No ambient authority.** A capability not granted in the manifest has no code
  path in the runtime, not merely a failing check.
- **No error recovery.** A malformed frame ends the session. Tolerance is how
  parsers become attack surface.
- **No layout escape hatches.** No floats, no CSS `position`, no
  `calc()`. Layout is one specified algorithm so that every implementation
  agrees on the pixel.
- **No fingerprinting surface.** No user-agent string, no font enumeration, no
  canvas readback, no device identifier.

## What "light and fast" means, concretely

Normative budgets live in [`10-budgets.md`](10-budgets.md) and are enforced in
CI. The headline ones:

- 0 % CPU and **zero wakeups** at idle — an architectural consequence of an
  event-driven redraw with no continuous render loop, not a tuning parameter;
- < 25 MB RSS for a 200-node application;
- 4.6 KB on the wire for a 50-row table against 14.4 KB of HTML — 3.1× overall,
  5.3× on structure alone, measured rather than hoped;
- 22 bytes for a single-cell update.

If a change cannot hold these, the change is wrong, not the budget.
