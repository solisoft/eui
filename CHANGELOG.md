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

### Added

- A tutorial in the documentation (`/docs/tutorial`): a shopping list built in
  six steps, each a component of the demo application (`pantry_1` …
  `pantry_6`) and each running in the page beside its code. A line reading
  `::: eui <component>` in any documentation page now becomes such a session.
- **Protocol 8: a session over a pipe** (spec 01 §7). `eui --pipe [--title
  <name>] [--allow …]` opens a window whose session is on the client's own
  standard input and output, so an application on the same machine can start
  the client itself — no port, no TLS, no manifest. Assets travel in the
  session as two new frame kinds, `Fetch` (`0x0D`) and `Asset` (`0x0E`),
  which exist only on a pipe and are refused on a socket. A pipe session is
  always its own process, ends when either side closes its end, and closes
  the window when the application finishes without an `Error`. The Ruby
  server speaks it (`App#run_pipe` in `clients/eui-ruby`).
- Documentation code blocks are coloured (Soli, Ruby and `sh`), on the server.
- **`eui package android` and `eui package ios`**: an application's own APK
  or iOS bundle, made from the client package the release publishes and an
  `eui.toml` in the application's directory (address, label, icon, version,
  package name or bundle id), with no SDK, NDK or Xcode. The APK is signed
  with APK Signature Scheme v2 and a key kept in `~/.config/eui`, so a later
  package upgrades the installed one. On a Mac, `[ios] identity` signs the
  bundle and makes an `.ipa`. See `/docs/packaging`.

### Changed

- On Android and iOS the client opens the address in `eui.url` — an asset of
  the APK, a file beside the iOS executable — before any compiled in with
  `EUI_ANDROID_URL` or `EUI_IOS_URL`, and opens the shell only when there is
  neither.
- The version is 0.8.0 and the protocol 8. Nothing on a socket changed: a
  server at 7 and this client meet exactly as before.
- The site's masthead has no `Spec` entry — the specification is a section
  of the documentation's own contents — and shows the GitHub mark where it
  said `Source`.
- A link to the site unfurls into the demo application: its Meridian
  dashboard in a client window beside the thesis, instead of a text card.
  Every page now carries its full title (`Phone packages — EUI`), `og:type`
  `article` for the documentation and the specification, an `og:image:alt`
  that describes the picture, JSON-LD, a touch icon and a web manifest; the
  documentation's own 404 asks not to be indexed.
- Documentation pages fit a phone: the contents fold into one line naming the
  current page, and the masthead wraps instead of running off the edge.

### Fixed

- The tutorial (`/docs/tutorial`) shows every helper its excerpts call. Ten
  were used and never defined on the page — `pantry_heading` from step 2 on,
  then `pantry_bump`, `pantry_defaults`, `pantry_add`, `pantry_toggle`,
  `pantry_counts`, `pantry_row_5`, `pantry_left`, `pantry_summary` and the
  step-5 handler — so code copied from it did not run. The demo application's
  `tests/tutorial_spec.sl` now fails when the page calls a helper it does not
  show, or shows one the component does not have.
- The browser client runs any number of embeds on one page. The second
  *Run it* used to fail with "EventLoop can't be recreated" — winit allows a
  page one event loop — and the first session was only hidden, never closed.
  The page now keeps one loop, one window and one canvas, and each embed
  replaces the session in it.
- A second embed on the same page (the tutorial's second *Run it*) drew into
  a corner of the canvas under WebGL and showed a black box under WebGPU, as
  on a Mac: the page resized the canvas for the new figure and the client's
  surface never followed. The client now sizes its surface to the canvas's
  box when a session is swapped in, and when the window opens — where the
  first session had been opening at 1x on a 2x screen.
- An embed on a page reached through Soli's prefetching navigation had a
  button that did nothing and a still in the wrong theme: the bootstrap ran
  once per document. It binds again after every page swap (`soli:load`).

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
