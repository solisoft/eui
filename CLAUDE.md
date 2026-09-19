# EUI

A protocol for delivering application interfaces over HTTPS without HTML, CSS
or JavaScript. `README.md` has the layout; `spec/` is normative and `spec/09`
says what conforming means.

## Documentation is part of the change

**Every change ships its documentation in the same commit.** Not afterwards —
a document that describes a version nobody is running is worse than one that
says nothing, because a reader cannot tell which half is stale. If a change
genuinely needs none of the surfaces below, say so in the commit message
rather than leaving it to be guessed at.

The surfaces, and when each one is owed:

1. **`spec/*.md`** — normative, and written **before** the code. A new
   endpoint, frame kind, op, node prop, media type or refusal belongs here
   first; the implementation is then a thing that can be wrong about
   something, rather than the only description of it. Requirements say where
   they are enforced.
2. **`spec/09-conformance.md`** — a normative requirement names the test that
   proves it. A new section of `spec/` that adds none is a claim nothing
   checks.
3. **`doc/docs/eui/*.md`** — the prose documentation. `transport.md`,
   `overview.md` and their neighbours explain what the spec requires;
   **`status.md`** says, crate by crate, what is built, tested, specified or
   not started.
4. **`doc/docs/eui/status.md` again, for numbers.** It and the prose print
   *counted* claims — bytes on the wire, memory per session, how many crates
   are done. A change that moves one of those numbers and leaves it written
   down is the failure mode this list exists for. Measure, then write the
   measurement, not an estimate of it.
5. **`www/docs/`** — a **copy**. Written only by `scripts/sync-docs.sh`; never
   edit it by hand. Run the script and commit what it produces. CI runs
   `scripts/sync-docs.sh --check`, so a hand edit or a missed run fails the
   build.
6. **`README.md`** — the crate table says what each crate is and whether it is
   built. It goes stale quietly.

## It reaches a second repository

The server half of the protocol lives in `../lang` (`src/serve/eui/`, and the
`router_eui` / `eui_*` builtins). A wire change is not done until both sides
speak it, and `../lang/CLAUDE.md` has its own documentation policy — the docs
site there has a markdown source *and* a hand-maintained `.html.slv` page for
every topic, plus two changelogs.

`clients/eui-{ruby,python,php,node,go,rust}` each carry their own port of the
framing, with no shared code. A new frame kind or tag is the same change
written six more times.

`eui-proto` and `eui-client` are pinned by git rev in `../lang/Cargo.toml`, so
a protocol change lands **here first**, is published, and both revs move
together. The `[patch]` block in that file is how the two are developed
against each other in the meantime.

## Building

Never `cargo build` locally — it starves the machine. Use `rbuild`:

```
rbuild eui                  # build, and link into ~/Work/soli/bin
rbuild eui --install        # also install over ~/.local/bin
rbuild check eui            # cargo fmt --check + clippy -D warnings
rbuild cargo eui test -p …  # any cargo command, remotely
```

`rustfmt.toml` sets 200 columns, not the default 100. Run `rustfmt` on the
files you touched — **never on a crate root** (`lib.rs`, `main.rs`), because
rustfmt follows `mod` declarations from there and rewrites the whole crate.

## Looking at a view without a screen

`examples/snapshot` renders off-screen:

```
snapshot <out> --soli <session-url> <name> <w> <h> <scale>   # over a socket
snapshot <out> --view <url> <component> <w> <h> <scale>      # over HTTPS, no session
```

The two should agree pixel for pixel on the same component; that comparison is
how "a page drawn without a socket is the same page" is checked rather than
asserted. `SNAPSHOT_CLICK`, `SNAPSHOT_NAV` and `EUI_TRACE=1` drive and
instrument it.
