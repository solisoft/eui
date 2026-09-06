# 10 — Budgets

Status: **measured** except where marked. `cargo run --release -p xtask --
bench` produces §2, §3 and §4 and exits non-zero when a budget is missed;
CI runs it.

Soli's own benchmark document publishes the rows it loses. This one does the
same: where a number comes from a run, it says so, and where it is still a goal,
it says that instead. A budget that was never measured is a slogan.

## 1. Runtime — targets, enforced by `cargo xtask bench` once the client lands

| Measure | Budget |
|---|---:|
| Launch → first pixel | < 80 ms |
| Idle, 200-node application | **0 % CPU, zero wakeups** |
| Idle RSS, 200-node application | < 25 MB |
| 10 000-row virtualised table, RSS | < 45 MB |
| 10 000-row virtualised table, scroll | 60 fps, < 2 ms CPU per frame |
| Client binary, stripped, 2 variable fonts included | < 12 MB |

Measured on 2026-09-06, `cargo build --release -p eui-client`, x86-64 Linux,
LTO, stripped: **12.13 MB** with the default features and **9.81 MB** with
`--no-default-features`. The difference is the accessibility stack —
AccessKit and, on Linux, the AT-SPI bus client it needs (`zbus`). The
default build misses the budget by one per cent, and the budget stays: an
accessible client is the one that ships, and the 2.3 MB is where the next
size work goes (the bus client brings its own async runtime beside tokio).

The zero-wakeup line is an architectural consequence, not a tuning parameter:
`winit` runs in `ControlFlow::Wait` and the client redraws only when a frame, an
input event, or a window event asked it to. There is no render loop in the code
to accidentally leave running.

## 2. Wire — measured

Source: `crates/eui-proto/tests/size_budget.rs`, run with `--nocapture`. The
subject is a 50-row × 4-column invoice table: 256 nodes, rows keyed, six shared
style records. The HTML side is the same table with the Tailwind classes such a
table really carries, unindented — the favourable case for HTML.

| | Bytes |
|---|---:|
| Cell and header text (identical both ways) | 2 337 |
| **EUI total** | **4 619** |
| EUI, with the status column interned | 4 209 |
| EUI, of which style records (once per session) | 396 |
| EUI structure only | 2 282 |
| **HTML total** | **14 362** |
| HTML structure only | 12 025 |

**Total 3.1×. Structure 5.3×. 8.9 bytes of structure per node.**

The text is the data; neither side can compress it away, so the honest headline
is the structural ratio. The regression budget is set at **4×**, below the
measured 5.3×, so a real regression trips it and ordinary drift does not.

| Operation | Measured | Budget |
|---|---:|---:|
| Single-cell update | 22 B | < 40 B |
| Reversing 50 keyed rows | 201 B | < 300 B |
| Same reversal by re-mounting | 4 619 B | — |

That last pair is the case for `MoveChild`. Soli's current LiveView diff works
on lines of an HTML string, so reordering a table has no cheap spelling at all;
here it is 49 moves and 201 bytes.

### What this does not claim

Bytes on the wire were never the main prize. A 3.1× reduction matters on a slow
link, but the reason EUI exists is what the *client* does not do with those
bytes: no tolerant parse, no selector matching, no cascade resolution, no reflow
of an untyped tree, no JIT. Those are the numbers in §1, and they are the ones
worth judging the project on once the client can be measured.

## 3. Decoder and session — measured

| Measure | Measured | Budget |
|---|---:|---:|
| Decode a 4.4 KB batch | 18 µs | < 50 µs |
| Decode the 10 000-row mount, 770 KB | 3.1 ms | < 20 ms |
| Apply that mount into a session, 50 002 nodes | 6.4 ms | < 30 ms |
| Layout of the table, virtualised | 0.14 ms | < 5 ms |

## 4. Wire — the table, text included

The §2 figure is structure; a real mount carries the cells too. The
10 000-row invoice table mounts in 769.5 KB, **15.8 bytes per node** with the
text, of which about 9 are structure and the rest the data itself. Budget:
18 B per node.
