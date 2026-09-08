# What EUI is

EUI delivers application interfaces over HTTPS without HTML, CSS, or JavaScript.
The server does not send a document to be interpreted. It sends an interface
tree that is **already resolved**, in a compact binary encoding, and a native
client applies it, lays it out, and draws it on the GPU.

## The problem it answers

A Soli application is fast on the server and expensive on the client. The server
side is measured and good: about 95 µs of CPU to render a 50-row template, 95 MB
of PSS at idle for the whole process against 643 MB for Django. The client side
is a web browser.

To display that same 50-row table, a browser must parse markup with an
error-tolerant parser that cannot fail — only guess; build an untyped DOM; match
every CSS selector against every element and resolve the cascade; compute
inherited values into computed values, per element; reflow a tree whose
constraints are discovered rather than declared; and keep a JavaScript JIT
resident with its attack surface and its memory.

Most of that exists to support documents written by strangers over thirty years.
An application interface does not need it. A counter costs 300 MB of RAM and a
busy core because of machinery it never asked for.

## Where the work moved

| A browser does this at runtime | EUI does it where |
|---|---|
| Parse markup | The server emits typed records; the client validates, never parses |
| Match selectors, resolve the cascade | The server resolves; the client does one array lookup |
| Compute inherited values | The server resolves |
| Reflow an untyped tree | The client lays out a typed tree with one fixed algorithm |
| JIT untrusted JavaScript | The client runs verified, metered bytecode, or nothing at all |
| Rasterise through a general engine | The client draws rounded rectangles and glyphs, one shader |

The client keeps only what genuinely must be local: layout, because it depends on
the viewport and the viewer's font scale; text shaping; painting; and the handful
of interactions that must not cost a round trip.

## Three ideas carry the encoding

**A session atom table.** Every repeated string — property names, labels, list
keys — is sent once and referenced by a variable-length integer afterwards. HTML
re-emits `<div class="…">` on every row of a table. EUI sends the atom once for
the whole session.

**A computed style table.** A style record is 64 fixed bytes of *already
resolved* style. A thousand table rows share three style ids, and there is no
cascade on the client because nothing is left to cascade.

**A flat node arena.** A subtree decodes into one contiguous array with integer
child and sibling indices. One allocation, cache-friendly traversal, and — the
part that matters for safety — a decoder that runs iteratively, so a
ten-thousand-deep hostile tree costs a bounds check rather than the call stack.

## What it costs

Measured on a 50-row, four-column table with keyed rows, against the same table
as Tailwind-styled HTML:

| | Bytes |
|---|---|
| Cell and header text, identical both ways | 2 337 |
| **EUI total** | **4 619** |
| **HTML total** | **14 362** |
| EUI structure only | 2 282 |
| HTML structure only | 12 025 |

Total 3.1×. Structure 5.3×. A single-cell update is 22 bytes; reordering fifty
keyed rows is 201, because rows move rather than rebuild.

Bytes on the wire were never the main prize. The reason EUI exists is what the
client does *not* do with those bytes. Those numbers, and how to reproduce them,
are on [Budgets](/docs/budgets).

## What it deliberately refuses

Each refusal is why the client stays small enough to audit.

- **An open element vocabulary.** Fourteen primitive kinds, and the set is
  closed. Everything a person would call a widget is composed on the server.
- **The cascade.** No selectors, no specificity, no `!important`.
- **Code from the network.** No native code, no JIT, no `eval`.
- **Ambient authority.** A capability the manifest never requested has no code
  path in the client.
- **Error recovery.** A malformed frame ends the session.
- **Layout escape hatches.** No floats, no CSS positioning, no `calc()`.
- **A fingerprinting surface.** No user-agent, no font enumeration, no canvas
  readback, no device identifier.

## Three ways to open an application

- `eui <wss://host/_eui/session/app> [--allow cap,cap]` — the standalone
  client, twelve megabytes, no browser.
- `soli eui <url>` — the same window from a Soli built with
  `--features eui-desktop`.
- `soli desktop build --eui <component>` — one executable that carries the
  app, its database and the window; the server runs on a thread behind a
  loopback gate only the embedded client can pass.

## Reading order

Start with [the wire format](/docs/wire-format) if you want the mechanism, or
[writing views](/docs/views) if you want to know what the code looks like.
[What works today](/docs/status) says plainly which parts exist.
