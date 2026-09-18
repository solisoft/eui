# Serving EUI from Ruby, Python, PHP, Node, Go or Rust

Soli is the reference server, not the only one. `clients/` holds the
protocol's server half in six more languages, each its own repository, each
with no runtime dependencies to speak of:

| | repository | install | what it needs |
|---|---|---|---|
| Ruby | [solisoft/eui-ruby](https://github.com/solisoft/eui-ruby) | gem `eui-ruby` | nothing; Ruby ≥ 3.2 |
| Python | [solisoft/eui-python](https://github.com/solisoft/eui-python) | `eui-python` | nothing; Python ≥ 3.11 |
| PHP | [solisoft/eui-php](https://github.com/solisoft/eui-php) | `solisoft/eui-php` | `ext-openssl`; PHP ≥ 8.2 |
| Node | [solisoft/eui-node](https://github.com/solisoft/eui-node) | `eui-node` | nothing; Node ≥ 20 or Bun ≥ 1.1 |
| Go | [solisoft/eui-go](https://github.com/solisoft/eui-go) | `github.com/solisoft/eui-go` | nothing; Go ≥ 1.23 |
| Rust | [solisoft/eui-rust](https://github.com/solisoft/eui-rust) | crate `eui-rust` | nothing; Rust ≥ 1.75 |

They exist for the obvious reason. An application whose data already lives in
a Rails codebase, a Django one, a Laravel one, an Express one or a Go service
should not have to move to another language to get a window; it should be
able to serve one from where its models already are.

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
button sends one `SetText` — nine bytes. The same program in Python, PHP,
JavaScript, Go and Rust is the same program: a mount, a handler named by the
view, and a render that returns the same tree.

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
of its content and none of those six standard libraries ships one; each is
checked against the official vectors, the multi-chunk ones included. Ed25519
comes from the runtime where there is one (Ruby's OpenSSL, PHP's, Node's
`crypto`, Go's `crypto/ed25519`) and is written out in Python and Rust, where
there is not — and both of those were compared byte for byte against
OpenSSL's signatures and against RFC 8032 §7.1. The Rust crate writes out
SHA-1 and base64 for the WebSocket handshake too, because it takes no
dependencies at all.

**The publisher key is one file for all six**: a PKCS#8 PEM. An application
that changes language keeps its identity, and nobody's pin breaks — checked,
not assumed: the same key file produces the same manifest bytes, signature
included, out of every one of them.

The view layer interns exactly as Soli's does — the same style vocabulary,
the same enumeration names, the same append-only tables — and diffs against
the tree the client holds. Keyed children reconcile through a Fenwick tree, so
a reordered table is *n* moves and not a scan per row.

**Not yet, in any of them:** local handlers (the bytecode a client runs for
itself), file transfers, session resume, and the windowed list's `window`
event. A socket that breaks gets a fresh session and a fresh `Mount`, which
is a conforming answer and a worse one. And one thing in one of them: the
Rust crate has no TLS, because Rust's standard library has none and the crate
takes no dependencies — it answers `ws://` and belongs behind a terminator.
The other five terminate TLS 1.3 themselves.

## What they cost

The same application written seven times — the same node hash, the same keys,
the same strings — an invoice table with a **tick** that changes one number
and a **sort** that reverses every row. A tick is the floor: the handler
adds 1, the view re-renders in full — fifty thousand nodes at ten thousand
rows — the diff finds one changed text node, and nine bytes go out. One
session at a time, over loopback; 20 ticks and 5 sorts, each phase measured
until the server has been silent for a second.

**500 rows — 2 511 nodes, which is what an application looks like**

| | Soli | Ruby | Python | PHP | Node | Go | Rust |
|---|---:|---:|---:|---:|---:|---:|---:|
| Mount | 44 ms | 72 ms | 79 ms | 92 ms | 81 ms | 42 ms | 68 ms |
| Tick — one number | 13.8 ms | 22.9 ms | 22.5 ms | 26.4 ms | 8.3 ms | 4.4 ms | 2.4 ms |
| Sort — 500 reversed | 17 ms | 36 ms | 27 ms | 30 ms | 9 ms | 9 ms | 5 ms |
| Memory, idle → after | 109 → 101 MB | 24 → 33 MB | 22 → 26 MB | 30 → 49 MB | 73 → 124 MB | 8 → 16 MB | 2.5 → 5 MB |
| CPU, 25 events | 0.35 s | 0.67 s | 0.65 s | 0.72 s | 0.45 s | 0.18 s | 0.06 s |

**10 000 rows — 50 011 nodes**

| | Soli | Ruby | Python | PHP | Node | Go | Rust |
|---|---:|---:|---:|---:|---:|---:|---:|
| Mount | 1 202 ms | 1 421 ms | 1 293 ms | 1 572 ms | 896 ms | 851 ms | 828 ms |
| Tick | 223 ms | 536 ms | 550 ms | 552 ms | 116 ms | 88 ms | 57 ms |
| Sort — 10 000 reversed | 243 ms | 587 ms | 666 ms | 723 ms | 179 ms | 168 ms | 114 ms |
| Memory, after | 229 MB | 126 MB | 94 MB | 154 MB | 268 MB | 94 MB | 57 MB |
| CPU, 25 events | 6.29 s | 14.38 s | 14.65 s | 15.36 s | 5.09 s | 3.72 s | 1.46 s |

The JavaScript one runs unchanged on **Bun**, where the same 98 tests pass
and the client cannot tell the difference. Bun is slower on this work by a
tenth to a half — 173 ms against Node's 116 for a tick at ten thousand rows —
and holds about half the memory, 33 MB idle against 84.

**On the wire, all seven are identical**: 9 bytes for the tick, 2.8 KB for the
500-row sort, 58.5 KB and ten thousand `MoveChild` for the 10 000-row one.
The one difference at the mount — 885 KB against 846 KB — is Soli interning a
row's cell strings the other six carry inline.

Three things are worth reading out of that.

**The bytes are the protocol's, not the implementation's.** Seven servers that
share nothing but a specification produce the same frames to the byte. That
is the claim this whole project rests on, and it is the one thing here that
is not a matter of degree.

**The spread is about 11× at the tick, and it ranks by runtime.** Rust and Go
compile to machine code and land where you would expect; V8 is within a
factor of two of Go; Soli's interpreter is next; CRuby, CPython and PHP land
within a sixth of one another. Every one of these servers is doing the same
work, which is exactly why the wire is identical.

Soli's own half of that is worth a line, because it is two halves: the view,
which its interpreter runs, and the convert-diff-encode, which is Rust.
`EUI_TRACE=1` prints the split — at 500 rows, 7.6 ms and 3.4 ms. Half of the
first is rebuilding style hashes that are all identical, and hoisting them out
of the loop takes the tick to 6.2 ms without moving a byte. The floor under
that is the interpreter building five thousand hashes, which no work on the
Rust side reaches.

**The memory column is not a language ranking.** Soli's idle is a whole
application server — worker pool, HTTP stack, database driver, LiveView
registry, a compiler for local handlers — and it swings by sixty megabytes
between runs as that pool warms, which is why its 500-row idle above reads
higher than its figure after the run. Node's is V8. Rust's 2.5 MB and Go's
8 MB are what a process is when nothing else is in it. Compare what the
processes do before comparing what they hold.

**And these are single runs on a machine that is not idle.** An earlier pass
put Ruby's 500-row tick at 36.9 ms against the 22.9 above and Soli's at 17.0
against 13.8, while Rust moved by six hundredths of a millisecond. The
compiled servers barely notice a busy machine; the allocating ones notice it a
great deal. Read the ranking, not the third digit.

And nothing here says anything about **concurrency**: one session at a time is
one session at a time. A hundred at once would rank Soli's worker pool, Go's
and Rust's threads, Node's event loop, PHP's process per connection and
Ruby's and Python's threads under a global lock quite differently, and that is
unmeasured.

The first run of that benchmark reported the Ruby sort at 17 690 ms. That was
not Ruby: it was a quadratic keyed reconciliation, and a style record compiled
once per node instead of once per distinct style. A Fenwick tree and a cache
took it to 789 ms, and the five implementations written afterwards had both
from the start. A benchmark between languages measures the implementations
first.

## Where it lives

`clients/` in this repository holds all six, each its own git repository —
cloned and pushed on its own, and this tree only ever has a copy sitting
there. `clients/eui-ruby/bench` holds the seven applications and the one
driver that measures them, and each library's own tests run in its own way:

```
rake test                                        # Ruby, 97
python3 -m unittest discover -s tests -t tests   # Python, 103
php tests/run.php                                # PHP, 96
npm test                                         # Node, 98
go test ./...                                    # Go, 98
cargo test                                       # Rust, 102
```

Among them, in each language, a client of forty lines that applies the
server's ops and compares the tree it ends up holding against the server's
own — over every permutation of five keyed rows and four hundred and eighty
random edits. That is the test a diff is actually worth: a `MoveChild` off by
one encodes, decodes and applies without complaint.
