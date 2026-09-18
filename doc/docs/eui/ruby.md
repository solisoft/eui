# Serving EUI from Ruby

Soli is the reference server, not the only one. `clients/eui-ruby` is the
protocol's server half as a Ruby gem: the wire format, the session, the view
encoder and its diff, the content-addressed asset store and the signed
manifest — about 3 600 lines, no runtime dependencies, and the reference
client cannot tell which one it is talking to.

It exists for the obvious reason. An application whose data already lives in
a Rails codebase should not have to move to another language to get a window;
it should be able to serve one from where its models already are.

```ruby
require "eui"

class Counter < EUI::Component
  def mount(params)
    super          # the window, as it is right now
    @count = 0
  end

  on("increment") { @count += 1 }

  def render
    column(gap: 5, align: "center", justify: "center",
           width: "100%", height: "100%", bg: "surface.base") do
      [text(@count.to_s, size: "4xl", weight: "bold"),
       button("Increment", "increment")]
    end
  end
end

app = EUI::App.new(name: "Counter", app_id: "counter.example")
app.mount("counter", Counter)
app.run(port: 5099)
```

```
EUI_ALLOW_INSECURE_LOOPBACK=1 eui ws://127.0.0.1:5099/_eui/session/counter
```

The contract is the one Soli uses, because it is the protocol's: a view is a
hash and a pure function of the state, a handler changes state and returns,
and what reaches the client is the difference the change made. Pressing that
button sends one `SetText` — nine bytes.

## What it implements

The whole of [the wire format](/docs/wire-format) — every frame, all
nineteen ops, the 64-byte style record, flat subtrees, values and handlers —
checked against the byte vectors that document pins, including the §8 worked
example at exactly 150 bytes.

The session of [the transport](/docs/transport): Hello and Welcome with the
version negotiated, batches with their sequence numbers, events, acks, pings,
a resync answered with a fresh `Mount` that does not repeat the tables, the
manifest at `/.well-known/eui` signed with an Ed25519 publisher key, and
assets at `/_eui/asset/<blake3-hex>`. BLAKE3 is in the gem in Ruby, because
an asset is named by the hash of its content and nothing in the standard
library ships one; it is checked against the official vectors, the
multi-chunk ones included.

The view layer interns exactly as Soli's does — the same style vocabulary,
the same enumeration names, the same append-only tables — and diffs against
the tree the client holds. Keyed children reconcile through `MoveChild`, so a
reordered table is *n* moves and not a rebuild.

**Not yet:** local handlers (the [bytecode](/docs/wire-format) a client runs
for itself), file transfers, session resume, and the windowed list's `window`
event. A socket that breaks gets a fresh session and a fresh `Mount`, which
is a conforming answer and a worse one.

## What it costs against Soli

The same application written twice — the same node hash, the same keys, the
same strings — an invoice table with a tick that changes one number and a
sort that reverses every row. One session at a time, over loopback, each
phase measured until the server stops talking, because Soli grafts a large
tree on across batches and the gem sends one `Mount`.

**500 rows — 2 511 nodes, which is what an application looks like**

| | Soli | eui-ruby | on the wire |
|---|---:|---:|---|
| Mount, fresh session | 45 ms | 79 ms | 39.6 KB |
| Tick — one number | 12.2 ms | 22.7 ms | **9 B** |
| Sort — 500 rows reversed | 15 ms | 29 ms | **2.8 KB**, 500 ops |
| Resident memory, idle → after | 88.5 → 86.8 MB | 23.5 → 32.7 MB | |
| CPU for 40 events | 0.51 s | 1.04 s | |

**10 000 rows — 50 011 nodes**

| | Soli | eui-ruby | + YJIT | on the wire |
|---|---:|---:|---:|---|
| Mount | 1 227 ms | 1 691 ms | 1 544 ms | 885 / 846 KB |
| Tick | 279 ms | 612 ms | 409 ms | **9 B** |
| Sort — 10 000 reversed | 250 ms | 789 ms | 457 ms | **58.5 KB**, 10 000 ops |
| Resident memory, idle | 96.5 MB | 23.5 MB | 24.2 MB | |
| CPU for 25 events | 7.1 s | 16.8 s | 10.9 s | |

Three things are worth reading out of that.

**The bytes are identical.** Nine bytes for a changed number and 2.8 KB to
reorder five hundred rows, byte for byte, out of two implementations that
share nothing but a specification. That is the protocol's claim and it holds
whoever writes the server.

**Ruby costs about twice the CPU and about a third of the memory.** Twice is
a language difference — the same work through a different interpreter, with
YJIT closing about half of it. The memory is not: Soli's 88 MB is an
application server with a worker pool, an HTTP stack, a database driver, a
LiveView registry and a compiler for local handlers, and the gem is a library
that does one thing. Compare what they do before comparing what they hold.

**Nothing here says anything about concurrency.** One session at a time is
one session at a time; a hundred at once is Soli's worker pool against one
Ruby thread per connection under a GVL, which is a different argument and an
unmeasured one.

The first run of that benchmark reported the Ruby sort at 17 690 ms. That was
not Ruby: it was a quadratic keyed reconciliation in the gem, and a style
record compiled once per node instead of once per distinct style. A Fenwick
tree and a cache took it to 789 ms and 612 ms. A benchmark between two
languages measures two implementations first.

## Where it lives

`clients/` holds the protocol's language bindings, each its own repository;
`clients/eui-ruby` is the first. `bench/` inside it has both halves of the
comparison above and the driver that produced the numbers, and `rake test`
runs 97 tests — among them a forty-line client that applies the server's ops
and compares the resulting tree to the server's own, over every permutation
of five keyed rows and four hundred and eighty random edits.
