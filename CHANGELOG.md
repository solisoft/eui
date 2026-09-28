# Changelog

## Versions

A release is tagged `vMAJOR.MINOR.PATCH`, and **the minor number is the
protocol version it speaks**: `v0.7.x` speaks EUI protocol 7
(`eui_proto::PROTOCOL_VERSION`). A change that raises the protocol raises
the minor number with it, and anything else that ships raises the patch.
`the_minor_version_is_the_protocol_version` (`crates/eui-client/tests/version.rs`)
holds the two together, so a tag cannot publish a client that names the wrong
one.

The major number stays 0 until the wire format is declared stable.

`eui --version` prints the version, the protocol and the commit a build was
made from; `scripts/install.sh` prints the one it replaces and the one it
installs. Between tags, the `rolling` prerelease is the tip of `main`.

The Soli server that speaks a protocol is released separately (soli_lang);
its `Cargo.toml` pins `eui-proto` and `eui-client` at the commit of an eui
release. A wire change lands here first, is released, and then the pin moves.

## [Unreleased]

## [0.7.1] - 2026-09-28

### Changed

- A page follows a window being resized. The client used to tell the server
  the new size only once the resize had been still for 50 ms, so a view laid
  out from the width stayed at its old layout for the whole drag and jumped
  at the end. It now sends a single resize (a window snapped or maximised)
  at once, and during a drag a size every 32 ms, never more than one waiting
  on the server (spec 01 §3). A server render of the landing page is 4–7 ms.

### Fixed

- Text with a `max_width` and no `width` was measured as if it did not wrap,
  then drawn wrapped over whatever followed it. `max_width` now narrows the
  width text is measured at (spec 04 §2).
- A row stretched across a column was measured with its width only bounded,
  so a `flex-1` child in it was measured at 0 px — a word a line — and the
  row took that height: a hero section drew thousands of pixels of empty
  band. A stretched child of a column of definite width is now measured at
  that width, as in CSS (spec 04 §4.1).

  Together these let a view say `w-full` and `flex-1` and have the client
  reflow it on every frame of a resize, as a browser does, instead of
  computing pixel widths on the server that follow a window only one round
  trip at a time. The demo's landing page (`site`) is written that way now,
  and needs a client with these fixes: on 0.7.0 its wide layout draws the
  empty band.

## [0.7.0] - 2026-09-27

The first versioned release. Everything before it was the `rolling` build of
`main`; this tag names the same client, and says which protocol it speaks.

### Protocol 7

- An absolute child names the edge it sits against across its parent:
  `position` 3 `absolute_start`, 4 `absolute_center`, 5 `absolute_end`
  (spec 04 §5). A stack placed every child horizontally by its own
  `justify`, so a badge at a card's top right beside children packed from
  the left had no spelling. A server sends a session below 7 plain
  `absolute`, which it draws as before.

### What a client at 7 speaks, by the version that added it

- **6** — gradients (`DefGradient`, a `bg` naming one), the `pulse` and
  `bounce` animations, and Tailwind's half steps `1.5`, `2.5`, `3.5`, `20`
  and `32` on the space scale (indices 13–17). A session below 6 is sent a
  gradient's first stop and the nearest older step.
- **5** — adopting a tree fetched over `GET /_eui/view/<component>`
  (`Offer::Adopt`, `Start::Adopted`), so a page drawn before its socket opens
  is not sent again.
- **4** — an application's own fonts (`DefFont`, font roles from 2).
- **3** — `level` events.
- **2** — the `scene` node kind and capability.

### Also in this release

- The shader verifier refuses expression nesting past 32 levels, counted on
  the source before the parser sees it (spec 11 §2.1). Three thousand `(`
  used to overflow the verifying thread's stack — the worker's, in the
  client — and end the session instead of refusing the module.
- `eui --version`, and install scripts that say which build they replace and
  which they installed, read from each binary without running it.
- `tw()`, the catalogue's Tailwind translator, takes auto margins, reversed
  rows, one side's border colour, offsets in a stack including `right-*`,
  aspect ratios and a set of aliases (`doc/docs/eui/tailwind.md`).

### Known limitations

- The six servers in `clients/` speak protocol 4.
- The phone builds are packaged by CI and have not been run on a device.
- `doc/docs/eui/status.md` is the full account, crate by crate.
