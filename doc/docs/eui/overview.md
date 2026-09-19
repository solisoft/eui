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

## What it looks like

A view is a Soli function: state in, a tree out, as plain data. This is the
counter from `examples/demo-app`, with both buttons taking the round trip;
its real `+` answers locally first, which [writing views](/docs/views) shows.

```soli
def counter_view(state)
  count = state["count"] ?? 0
  column({"pad": 6, "gap": 4, "bg": "surface.base"}, [
    text("Counter", {"size": 4, "weight": "semibold"}),
    keyed("value", text(count.to_s, {"size": 7, "weight": "bold"})),
    row({"gap": 2}, [
      button("−", "decrement"),
      button("+", "increment")
    ])
  ])
end
```

`column`, `text`, `button` are ordinary functions returning hashes — the whole
catalogue is in [Components](/docs/components). When the count changes, the
server does not re-send that tree. It sends the difference:

```
03  14  02  01  23  8d 01  01  0d  31 20 39 38 34 2c 20 34 32 20 e2 82 ac
```

Twenty-two bytes: a batch frame, its length, the sequence number, one op,
`set_text`, node 141, an inline string of thirteen bytes, and the thirteen
bytes a person actually reads. No row, no table, no page.

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

- **An open element vocabulary.** Seventeen node kinds, and the set is closed.
  Everything a person would call a widget is composed on the server.
- **The cascade.** No selectors, no specificity, no `!important`.
- **Code from the network.** No native code, no JIT, no `eval`.
- **Ambient authority.** A capability the manifest never requested has no code
  path in the client.
- **Error recovery.** A malformed frame ends the session.
- **Layout escape hatches.** No floats, no CSS positioning, no `calc()`.
- **A fingerprinting surface.** The client sends no user-agent, enumerates no
  fonts, reads back no canvas and carries no device identifier — and in the
  standalone client that is the whole surface, so it is the whole story. The
  WebAssembly build below inherits the page's: the *client* still asks for
  none of it, but the tab around it is a browser tab, with everything that
  implies. Read documentation in it; do not bank in it.

## Getting it

One line, on anything with a shell. It reads `uname`, picks the build that
matches the machine, unpacks it, and leaves the binary in `~/.local/bin` —
or wherever `EUI_DEST` says:

```sh
curl -fsSL https://raw.githubusercontent.com/solisoft/eui/main/scripts/install.sh | sh
```

It takes a tag, so `| sh -s v0.4.0` installs that release rather than the
rolling one. And it refuses rather than guesses where there is no build: an
Intel Mac and a Linux on ARM both `uname` perfectly well and have nothing to
download.

On macOS that installs the bare binary, which is the thing a `eui wss://…`
command line wants. For **EUI.app** in `/Applications`:

```sh
curl -fsSL https://raw.githubusercontent.com/solisoft/eui/main/scripts/install-macos.sh | bash
```

`curl` rather than a browser, and that is the whole point of the second
script. The build is ad-hoc signed and not notarised, so Gatekeeper holds
the first launch of anything wearing `com.apple.quarantine` — which is
written by whatever downloaded it, afresh on every new download, which is
why stripping it by hand never stays stripped. `curl` writes none.

Or take the archive and unpack it yourself:

| | file |
|---|---|
| Linux | [`eui-x86_64-linux.tar.gz`](https://github.com/solisoft/eui/releases/download/rolling/eui-x86_64-linux.tar.gz) |
| macOS | [`EUI-aarch64-macos.dmg`](https://github.com/solisoft/eui/releases/download/rolling/EUI-aarch64-macos.dmg) |
| Windows | [`eui-x86_64-windows.zip`](https://github.com/solisoft/eui/releases/download/rolling/eui-x86_64-windows.zip) |
| Android | [`eui.apk`](https://github.com/solisoft/eui/releases/download/rolling/eui.apk) |

`rolling` is rebuilt on every push to `main`; a version tag in place of it
gets that release instead.

### And then the applications

Once the client is installed, an application can be too — `eui --install
<address>` writes a launcher entry that opens it in its own window: a
`.desktop` file on Linux, a bundle in `~/Applications` on macOS, a Start
menu shortcut on Windows. The arrow beside the padlock in the address bar
does the same thing, and becomes a tick once it is there.

Nothing is packaged and nothing is downloaded by that: the entry runs the
client with the application's address, so updating the client updates every
installed application at once. The icon is the one the publisher signed into
their manifest ([Transport](/docs/transport) §2.1) — an application that
publishes none cannot be installed, because the alternative is a launcher
full of identical pictures. `--installed` lists what is there and says
where; `--uninstall` takes an address, or an `app_id` to remove every entry
an application has.

## Four ways to open an application, and one of them is a courier

- `eui <wss://host/_eui/session/app> [--allow cap,cap]` — the standalone
  client, twelve megabytes, no browser.
- `soli eui <url>` — the same window from a Soli built with
  `--features eui-desktop`.
- `soli desktop build --eui <component>` — one executable that carries the
  app, its database and the window; the server runs on a thread behind a
  loopback gate only the embedded client can pass.
- **A page, on a `<canvas>`** — the same client compiled to WebAssembly, so
  that a documentation page can put a running application beside its source
  instead of a picture of one:
  [eui-data.solisoft.net/live](https://eui-data.solisoft.net/live/counter).
  It is served by the application it embeds, and it has to be: Soli refuses a
  WebSocket upgrade whose origin is not its own, so the page that opens a
  session must come from the server that answers it.

That fourth entry is a **viewing vehicle, not the model**, and the difference
is worth being plain about. The browser supplies three things — a GPU
surface, a socket and a pointer — and nothing else: there is no DOM below
that line, no HTML, no CSS, no cascade, and still no code from the network in
the sense [Security](/docs/security) means, because the module *is* the
client and the server sends only data. What it costs is a multi-megabyte
download the native client does not, which is why nothing fetches it until
you press a button.

It also gives up three things the standalone client does not, and they are
listed where they belong rather than here: the publisher key is the
browser's business and not ours ([Transport](/docs/transport)), the decoder
does not get a process of its own ([Security](/docs/security) §10), and the
`scene` capability is not offered at all, because a browser cannot tell you
whether a shader compiled until after it has run.

If you want the real thing, the first line of this list is twelve megabytes
and opens in a window.

## Reading order

Start with [the wire format](/docs/wire-format) if you want the mechanism, or
[writing views](/docs/views) if you want to know what the code looks like.
[What works today](/docs/status) says plainly which parts exist.
