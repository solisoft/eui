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

- **Protocol 9: MIDI sounds** (spec 03 §7). An `audio` node's `src` may be
  a Standard MIDI File, which the client synthesizes — with the SoundFont 2
  bank its `soundfont` prop names (a list of assets, the parts of a bank
  larger than one asset) or, without one, voices of its own for General
  MIDI's sixteen families and a drum kit. `notes` plays a list of MIDI
  commands at once, again each time `notes_seq` changes: a note played as it
  is written, with no sound file sent. lang passes the negotiated version
  to `connect` as `protocol`.
- **Protocol 9: MIDI instruments** (spec 06 §1.4, 01 §2.1). A node holding a
  `midi` handler hears the person's MIDI keyboard — every channel message,
  in order, stamped with the node's clock (a playing sound's position, for
  recording against it) — while the new `midi` capability is granted and
  the window is in front. System messages, SysEx included, never leave the
  client. The window opens the platform's inputs (ALSA, CoreMIDI, Windows
  MM) only while a node asks.
- The score editor plays in the window: Play sends the score as a MIDI file
  and each note written goes as MIDI commands, with GeneralUser GS
  (`tools/fetch_soundfont.sh`) or the client's own voices; the server mixes
  nothing. A fermata holds its note twice its length, on every staff, in
  the MIDI file and the playhead alike.
- **`eui-score`**, the catalogue's score engraver in Rust: LilyPond input
  laid out and drawn as canvas paths, played as MIDI and printed as PDF,
  path for path and byte for byte what the Soli engraver makes (its golden
  tests hold the two together) and about two hundred times faster. lang
  calls it as `eui_score_layout`, `eui_score_bands`, `eui_score_midi` and
  `eui_score_pdf`; `score()` and the score editor use them when the server
  has them.
- A dragged note follows the hand: a server behind on its events handles a
  run of moves once, with the last position (06 §2) — the note no longer
  goes on moving for a second after the release.
- The score editor has an **Insert** mode beside the pointer (N, Escape to
  leave it): a click on a note adds the pitch to it as a chord, a click on
  a rest turns it into a note of its length, and a click between notes
  writes a new one there, in the value the palette shows — which in that
  mode is the next note's, never the selected one's — while the pointer
  goes on picking notes up to move them.
- In Insert mode a value other than the note's writes over it: an eighth
  on a quarter takes its first half and leaves an eighth rest, a half on a
  quarter covers the next one too; nothing after moves.
- A start repeat (`[`) on the first bar is written (`\bar ".|:"` before the
  first note) and drawn after the clef, key and time, as LilyPond draws it;
  `score()` and `eui-score` read that bar line.
- A key or metre change "From the selected bar" works from any track,
  the full score included: it is set on the first track at that bar.
- The score's playhead and selection are laid over the music instead of
  recolouring it: lighting a note sends its few paths, not its system's
  (267 KB for a system of the full score), so the picture keeps up with the
  sound and Stop stops at once. Screen pages are no longer numbered in the
  music; A4 pages and the PDF are.
- The score editor's **Examples** menu loads Frère Jacques or the exposition
  of Beethoven's Fifth (Mutopia's public-domain engraving, reduced to what
  the editor reads by `tools/mutopia_beethoven5.py`), a whole score at once.
- The score editor shows a long score a page at a time — only the page on
  screen is drawn and sent — with the page following the selection and the
  playhead. **Full score** heads the track strip and shows every staff, at
  65 % size, instead of the edited one; a part's own staff gathers its bars
  of rest into one with its count, as `\compressEmptyMeasures` prints a part.
- The score editor: ↑/↓ step along the key's scale and Shift+↑/↓ by
  semitones; a note can be dragged up and down its staff, redrawn and heard
  at each line; a deleted note leaves a rest of its length, and a last bar
  emptied that way goes; S arms a slur and S again closes it; notes stay
  within five ledger lines of the staff and a piano's keys. A keyboard's
  step input (a key, a note; keys together, a chord) is written and waits
  for EUI 9 on the server side.
- The score editor's playhead lights every note that began since the last
  report — a run of eighths lights rather than being skipped — and moves on
  its own every 100 ms between the window's four reports a second;
  crescendo and diminuendo are set as a slur is, once on the first note and
  once on the last (`<` and `>`).

- **Canvas fills** (spec 03 §1.1, path kind `5`): a filled outline of moves,
  lines, quadratic and cubic Béziers and closes, filled by the non-zero
  rule and anti-aliased. The client rasterises it into the glyph atlas;
  a client from before it skips the path, as it skips any unknown kind.
  It is what a notehead, a clef or a slur is drawn with.
- **`score(src, opts)`** in the catalogue: a piece written in LilyPond's
  own input — `\relative`, clefs, keys, metres, notes, rests, chords, ties,
  slurs, beams — engraved into one canvas with LilyPond's spacing, stem,
  beam and slur rules and line weights, from the outlines of Emmentaler,
  LilyPond's own music font. What it cannot draw is refused with its
  position. `xtask smufl` regenerates the outlines from the font.
- `score()` prints a title page and tempo marks: `\header { title = …
  composer = … }` set as LilyPond sets it, `\tempo "Allegro" 4 = 120` over
  the staff, in the letters LilyPond uses. A key or a metre may change at any
  bar, with the old key cancelled and the change announced at the end of the
  line before; lines carry their bar numbers; and `{"page": "a4"}` lays the
  music out on A4 pages with LilyPond's margins and spacing.
- `score()` draws repeats and bars of rest: `\repeat volta 2 { … }` with
  its `\alternative` endings in brackets, the `\bar` signs `.|:`, `:|.`
  and `:|.|:`, and `R1*4` — four bars, or one bar with its count under
  `\compressEmptyMeasures`. The score editor sets a repeat at the selected
  bar (`[` and `]`) and gathers bars of rest.
- `score()` draws tuplets — `\tuplet 3/2 { … }`, `\tuplet 6/4`, spans,
  `\times 2/3` — numbered and bracketed as LilyPond does them, and obeys
  `\break` and `\pageBreak`. The score editor makes triplets and
  sextuplets (Ctrl+3, Ctrl+6), sets line and page breaks, and keeps its
  settings in tabs under one writing toolbar.
- `score()` draws expression: dynamics (`\p`, `\mf`, `\sfz`…), hairpins
  (`\<` `\>` `\!`), articulations (`-.` `->` `--` `-^` `-!` `-_`,
  `\fermata`) and words (`^"dolce"`), placed as LilyPond places them. The
  score editor has an Expression tab for them, and keeps the last
  engravings of a score so that a click on a note redraws in milliseconds.
- `score()` is a column of canvases, one per system: editing a note
  redraws and resends only its system (≈ 36 KB instead of ≈ 420 KB for an
  A4 page), and an edit in a long piece takes about half the time it did.
- `score_midi(src)` writes a score as a MIDI file — notes, ties, tempos,
  dynamics and hairpins as loudness, articulations — matching LilyPond's
  own MIDI note for note; the score editor has a **Download MIDI** button.
- `score()` draws several staves: a piano's (`\new PianoStaff`, a brace,
  bar lines through), an orchestra's (`\new StaffGroup`, a bracket), a
  choir's, and plain `<< >>`, with instrument names; `score_midi` writes
  them as a multi-track file, one instrument per staff.
- `score_pdf(src)` writes a score's A4 pages as a vector PDF; the score
  editor has a **Download PDF** button beside **Download MIDI**.
- The score editor has tracks: add them, switch with a click or Alt+↑/↓,
  edit one at a time, name them and give each an instrument, a volume, a
  pan and a mute; the full score under the sheet shows every track and
  lights the notes as they play. The PDF and the MIDI hold every track.
- The snapshot tool's `SNAPSHOT_DRIVE` takes `save:<name>`: it answers the
  save dialog the last click opened and writes what the server sends; a
  `click:<label>` now stops at a node that opens a file dialog.
- `score`, a twenty-first demo component: a score editor. Type A–G and the
  MuseScore duration numbers, or click on the staff; a note across a bar
  line is cut and tied for you; the LilyPond source beside it can be edited
  or pasted; Play plays the tune, and each note written is heard as it
  lands. It has fields for the title and composer, a
  free tempo with printed words, every common metre, key, metre and tempo
  changes from the selected bar, and an A4 page view. The gallery's catalogue gains a Notation
  card.
- `campaigns`, a twentieth demo component: a campaign-performance table with
  summary figures, search, a status filter, sortable headers, sparklines and
  target meters drawn as `canvas` paths, ticking with bulk pause and resume,
  a column picker, compact rows and paging. Columns drop out as the window
  narrows, and on a phone it becomes a list of cards. `/live/campaigns` opens
  it in a browser.

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

- A key typed into a text field reaches a `key_down` handler above the
  field only if that handler names it in `keys` (spec 03 §3.1): a page's
  shortcuts on its root no longer fire while someone types — a `Backspace`
  at the start of a title deleted a note in the score editor. A widget
  wrapped around a field (a combobox, a tag list) already names its keys.

- The client draws through **wgpu 30** (was 23), and the shader verifier
  parses with naga 30. The renderer's pixel tests pass unchanged, the
  surface stays sRGB, and a suboptimal frame is still drawn. Building from
  source now needs Rust 1.87. The client binary grows by 0.79 MB (18.42 →
  19.21 MB, stripped), most of it naga. The verifier also refuses, by name,
  the task, mesh and ray-tracing stages naga 30 can parse (spec 11 §2.2).
- The site serves its own fonts (IBM Plex Mono, IBM Plex Sans, Instrument
  Serif) instead of loading them from Google Fonts, and turns them on once
  the page has painted. Lighthouse scores the landing page 100 in all four
  categories on mobile and desktop; mobile first paint went from 1.6 s to
  0.9 s.
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

- Stopping a long sound no longer kills the window's worker on Linux
  (`the worker exited: signal: 31 (SIGSYS)`): glibc, trimming the heap
  the sound had been decoded into, opened a file behind the sandbox. The
  worker now runs with heap trimming off.
- The score editor's playhead starts with the sound, not three quarters of
  a second before it while the window synthesizes the MIDI file.
- Resizing a window on Linux (Wayland) follows the hand again. The client
  presented frames faster than the compositor took them, and Vulkan's FIFO
  held it back by blocking for a swapchain image — 9.4 s of a 12.5 s drag —
  while the compositor's configures queued. It now waits for the
  compositor's frame callback instead: resized frames went from every
  27.5 ms (median; 199 ms at the 90th percentile) to every 17 ms (31 ms).
  `EUI_TRACE=1` times the resize and the image wait on each `frame:` line.
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
