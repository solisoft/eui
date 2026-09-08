# Budgets

Soli's benchmark document publishes the rows it loses. This one does the same:
where a number comes from a run it says so, and where it is a target it says
that instead. A budget that was never measured is a slogan.

## Wire — measured

From `crates/eui-proto/tests/size_budget.rs`. Run it yourself:

```
cargo test -p eui-proto --test size_budget -- --nocapture
```

The subject is a 50-row, four-column invoice table: 256 nodes, keyed rows, six
shared style records. The HTML side is the same table with the Tailwind classes
such a table really carries, unindented — the favourable case for HTML.

| | Bytes |
|---|---|
| Cell and header text, identical both ways | 2 337 |
| **EUI total** | **4 619** |
| EUI with the status column interned | 4 209 |
| EUI, of which style records, once per session | 396 |
| EUI structure only | 2 282 |
| **HTML total** | **14 362** |
| HTML structure only | 12 025 |

**Total 3.1×. Structure 5.3×. 8.9 bytes of structure per node.**

The text is the data — neither side can compress it away — so the honest
headline is the structural ratio. The regression budget is set at 4×, below the
measured 5.3×, so a real regression trips it and ordinary drift does not.

| Operation | Measured | Budget |
|---|---|---|
| Single-cell update | 22 B | under 40 B |
| Reversing 50 keyed rows | 201 B | under 300 B |
| The same reversal by re-mounting | 4 619 B | — |

That last pair is the argument for `MoveChild`.

## Runtime — targets

Not yet measurable: the client does not exist. These are what
`cargo xtask bench` will enforce.

| | Budget |
|---|---|
| Launch to first pixel | under 80 ms |
| Idle, 200 nodes | **0 % CPU, zero wakeups** |
| Idle RSS, 200 nodes | under 25 MB |
| 10 000-row virtualised table, RSS | under 45 MB |
| The same table, scrolling | 60 fps, under 2 ms CPU per frame |
| Client binary, stripped, two variable fonts included | under 12 MB |

Measured on 2026-09-06: **12.13 MB** with the default features, **9.81 MB**
without accessibility (`--no-default-features`). The default build misses the
budget by one per cent; the 2.3 MB is AccessKit and the AT-SPI bus client on
Linux, and it stays in — an accessible client is the one that ships. That is
where the next size work goes.

Zero wakeups is architectural, not a setting: `winit` runs in
`ControlFlow::Wait` and the client redraws only when something asked it to.
There is no render loop in the code to leave running by accident.

## Decoder — targets

| | Budget |
|---|---|
| Decode a 4 KB batch | under 50 µs |
| Allocations per batch | at most 3 |
| Peak decode memory | at most twice the frame size |

## What none of this claims

Bytes on the wire were never the main prize. A 3.1× reduction matters on a slow
link, but the reason EUI exists is what the client does *not* do with those
bytes: no tolerant parse, no selector matching, no cascade resolution, no reflow
of an untyped tree, no JIT. Those are the runtime numbers, and they are the ones
worth judging the project on once there is a client to measure.
