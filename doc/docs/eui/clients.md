# Serving EUI from Ruby, Python, PHP or Node

Soli is the reference server, not the only one. `clients/` holds the
protocol's server half in four more languages, each its own repository, each
with no runtime dependencies to speak of:

| | repository | install | what it needs |
|---|---|---|---|
| Ruby | [solisoft/eui-ruby](https://github.com/solisoft/eui-ruby) | gem `eui-ruby` | nothing; Ruby ≥ 3.2 |
| Python | [solisoft/eui-python](https://github.com/solisoft/eui-python) | `eui-python` | nothing; Python ≥ 3.11 |
| PHP | [solisoft/eui-php](https://github.com/solisoft/eui-php) | `solisoft/eui-php` | `ext-openssl`; PHP ≥ 8.2 |
| Node | [solisoft/eui-node](https://github.com/solisoft/eui-node) | `eui-node` | nothing; Node ≥ 20 or Bun ≥ 1.1 |

They exist for the obvious reason. An application whose data already lives in
a Rails codebase, a Django one, a Laravel one or an Express one should not
have to move to another language to get a window; it should be able to serve
one from where its models already are.

```ruby
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

The contract is the one Soli uses, because it is the protocol's: a view is a
hash and a pure function of the state, a handler changes state and returns,
and what reaches the client is the difference the change made. Pressing that
button sends one `SetText` — nine bytes. The same program in Python, PHP and
JavaScript is the same program: `mount`, a handler named by the view, and a
`render` that returns the same tree.

## What they implement

The whole of [the wire format](/docs/wire-format) — every frame, all nineteen
ops, the 64-byte style record, flat subtrees, values and handlers — checked in
each language against the byte vectors that document pins, including the §8
worked example at exactly 150 bytes.

The session of [the transport](/docs/transport): Hello and Welcome with the
version negotiated, batches with their sequence numbers, events, acks, pings,
a resync answered with a fresh `Mount` that does not repeat the tables, the
manifest at `/.well-known/eui` signed with an Ed25519 publisher key, and
assets at `/_eui/asset/<blake3-hex>`.

BLAKE3 is written out in each of them, because an asset is named by the hash
of its content and none of those four standard libraries ships one; each is
checked against the official vectors, the multi-chunk ones included. Ed25519
comes from the runtime where there is one (Ruby's OpenSSL, PHP's, Node's
`crypto`) and is written out in Python, where there is not — and that one was
compared byte for byte against OpenSSL's signatures.

**The publisher key is one file for all four**: a PKCS#8 PEM. An application
that changes language keeps its identity, and nobody's pin breaks.

The view layer interns exactly as Soli's does — the same style vocabulary,
the same enumeration names, the same append-only tables — and diffs against
the tree the client holds. Keyed children reconcile through a Fenwick tree, so
a reordered table is *n* moves and not a scan per row.

**Not yet, in any of them:** local handlers (the bytecode a client runs for
itself), file transfers, session resume, and the windowed list's `window`
event. A socket that breaks gets a fresh session and a fresh `Mount`, which
is a conforming answer and a worse one.

## What they cost

The same application written five times — the same node hash, the same keys,
the same strings — an invoice table with a tick that changes one number and a
sort that reverses every row. One session at a time, over loopback, each
phase measured until the server stops talking.

**500 rows — 2 511 nodes, which is what an application looks like**

| | Soli | Ruby | Python | PHP | Node |
|---|---:|---:|---:|---:|---:|
| Mount | 62 ms | 107 ms | 72 ms | 117 ms | 63 ms |
| Tick — one number | 12.0 ms | 23.7 ms | 25.3 ms | 28.0 ms | 9.4 ms |
| Sort — 500 reversed | 18 ms | 29 ms | 27 ms | 32 ms | 10 ms |
| Memory, idle → after | 59 → 71 MB | 23 → 33 MB | 22 → 26 MB | 30 → 49 MB | 61 → 127 MB |
| CPU, 40 events | 0.57 s | 1.10 s | 1.06 s | 1.27 s | 0.63 s |

**10 000 rows — 50 011 nodes**

| | Soli | Ruby | Python | PHP | Node |
|---|---:|---:|---:|---:|---:|
| Mount | 1 471 ms | 1 742 ms | 1 420 ms | 2 055 ms | 1 075 ms |
| Tick | 327 ms | 577 ms | 650 ms | 601 ms | 125 ms |
| Sort — 10 000 reversed | 385 ms | 688 ms | 709 ms | 717 ms | 191 ms |
| Memory, after | 177 MB | 125 MB | 91 MB | 151 MB | 248 MB |
| CPU, 20 events | 7.16 s | 12.94 s | 13.95 s | 13.79 s | 4.41 s |

The JavaScript one runs unchanged on **Bun**, where the same 98 tests pass
and the client cannot tell the difference. Bun is slower on this work by a
fifth to a third — 178 ms against Node's 152 for a tick at ten thousand rows
— and holds about half the memory, 34 MB idle against 60.

**On the wire, all five are identical**: 9 bytes for the tick, 2.8 KB for the
500-row sort, 58.5 KB and ten thousand `MoveChild` for the 10 000-row one.
The one difference at the mount — 885 KB against 846 KB — is Soli interning a
row's cell strings the other four carry inline.

Three things are worth reading out of that.

**The bytes are the protocol's, not the implementation's.** Five servers that
share nothing but a specification produce the same frames to the byte. That
is the claim this whole project rests on, and it is the one thing here that
is not a matter of degree.

**The spread between languages is about 3×, and it ranks by JIT.** Node is
quickest and V8 is why; Soli's interpreter is next; CRuby, CPython and PHP
land within a fifth of one another. None of it is Rust against the rest —
Soli renders its view through an interpreter too.

**The memory column is not a language ranking.** Soli's idle is a whole
application server — worker pool, HTTP stack, database driver, LiveView
registry, a compiler for local handlers — and Node's is V8. Compare what the
processes do before comparing what they hold.

And nothing here says anything about **concurrency**: one session at a time is
one session at a time. A hundred at once would rank Soli's worker pool,
Node's event loop, PHP's process per connection and Ruby's and Python's
threads under a global lock quite differently, and that is unmeasured.

The first run of that benchmark reported the Ruby sort at 17 690 ms. That was
not Ruby: it was a quadratic keyed reconciliation, and a style record compiled
once per node instead of once per distinct style. A Fenwick tree and a cache
took it to 789 ms, and the three implementations written afterwards had both
from the start. A benchmark between languages measures the implementations
first.

## Where it lives

`clients/` in this repository holds all four, each its own git repository —
cloned and pushed on its own, and this tree only ever has a copy sitting
there. `clients/eui-ruby/bench` holds the five applications and the one
driver that measures them, and each library's own tests run in its own way:

```
rake test                                        # Ruby, 97
python3 -m unittest discover -s tests -t tests   # Python, 103
php tests/run.php                                # PHP, 96
npm test                                         # Node, 99
```

Among them, in each language, a client of forty lines that applies the
server's ops and compares the tree it ends up holding against the server's
own — over every permutation of five keyed rows and four hundred and eighty
random edits. That is the test a diff is actually worth: a `MoveChild` off by
one encodes, decodes and applies without complaint.
