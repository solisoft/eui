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

- `\afterGrace` (score #389): `\afterGrace c2 { d16 e }`, and LilyPond's
  `\afterGrace 7/8 c2 { … }`, where it was refused as an unknown command.
  The grace notes ride on their note (`Ev::after_graces`, `after_at`),
  taking no time and no number; they are drawn small, beamed as grace notes
  are, after the note and moved up to the next one as LilyPond sets them,
  with no slur (LilyPond leaves it to the source); in MIDI the note ends at
  its 3/4 point (or the fraction written) and the grace notes share the
  rest by their written values. A note cut at a bar line keeps them on its
  last piece. Protocol 11.

- The page around a score's music (score #388). `\paper`'s
  `oddHeaderMarkup`, `evenHeaderMarkup`, `oddFooterMarkup` and
  `evenFooterMarkup` are read and printed in the PDF — words, the header's
  fields (`\fromproperty #'header:title`), the page's number
  (`#'page:page-number-string`), spread by `\fill-line`, `\on-the-fly
  #print-page-number-check-first` and its kin naming the pages — in grey,
  two steps under the music's words; an even page takes the odd page's when
  it sets none, and a header moves the titles and the music down 3 staff
  spaces. A footer set replaces the title and "n / N". `systems-per-page`
  (and `max-systems-per-page`) puts at most so many systems on a page,
  filled or spread. The header's `copyright` is printed at the foot of the
  first page, centred over the footer, as LilyPond's first-page footer
  prints it; a © the text face lacks is drawn as its `c` in a ring.
- Every word of a score's PDF — titles, lyrics, marks, numbers, header and
  footer — is text a reader selects, copies and searches: written under its
  outline in the invisible rendering mode, in named (not embedded) Type 0
  fonts whose widths are the outlines' and whose ToUnicode map gives each
  character. The drawing is unchanged; `pdf_outlines` writes it alone, which
  is what the golden test holds to the Soli engraver's bytes.

- A score opens the LilyPond files people have (score #359) — Mutopia's,
  Frescobaldi's, musicxml2ly's — where one layout command refused the
  whole file ("\X is not a command this reads"). `\override`, `\revert`,
  `\once`, `\tweak`, `\set` and `\unset` of any property, the direction
  and style switches (`\slurUp`, `\tieDown`, `\dynamicUp` and their kin),
  `\transposition`, a context's `\with { \consists … \override … }`,
  `\layout`, `\midi` and `\paper` settings, top-level `\markup`, an
  `\include` and header fields no title prints are read whole — Scheme
  values `#red`, `##t`, `#'(1 . 2)`, `#(…)` included — and passed over,
  each with a warning the caller can read (`eui_score::warnings`,
  `Parsed::warnings`). `\stemUp` / `\stemDown` / `\stemNeutral` (and
  `\override Stem.direction`) and `\autoBeamOff` / `\autoBeamOn` (and
  `\set Staff.autoBeaming`) are honoured.
- Drawn: `\breathe` (a comma over the staff after its note) and `\caesura`
  (the railroad tracks), `\upbow`, `\downbow`, `\flageolet`,
  `\espressivo`, `\laissezVibrer` and `\repeatTie` (short ties), `\cresc`,
  `\dim` and `\decresc` as words and dashes, `\startTextSpan` …
  `\stopTextSpan` (a dashed line, after the words `\override
  TextSpanner.bound-details.left.text` gives it), and forced and reminder
  accidentals, `cis!` and `cis?` (in parentheses). Eight glyphs added to
  the table through `xtask smufl`.
- Several `\score` blocks — or pieces of music written one after another
  — inside `\book` and `\bookpart` or not, are engraved one after another
  on the same staves: each movement on a line of its own (a page for a
  book part), its bars counted from 1, its clef, key and metre LilyPond's
  defaults until it says its own, under its header's `piece` and `opus`
  as LilyPond sets them. A movement whose staves are not the first's is
  left out, with a warning.
- A clef or a key may change in the middle of a bar (score #360) — the
  left hand's `\clef treble` between two beats, a MusicXML import that
  writes one — where it was refused: the clef is drawn smaller before the
  note it falls on, as LilyPond draws a clef that does not open a line
  (Emmentaler's own `clefs.*_change`, three glyphs added through `xtask
  smufl`), the key with the old one's naturals; the notes after it, the
  other voices of the staff too, read under it, a new key starting the
  bar's accidentals afresh. Room is made for it on every staff of the
  system. `\time` still changes at a bar line.
- A score gathers bars of rest wherever LilyPond does (score #362):
  `\compressMMRests` around any music — a staff's, a `\new Staff`, or
  the `<< >>` of a whole score — and `\set Score.skipBars = ##t`. In a
  score of several staves a run is one bar only where every staff rests
  the same bars, as many bars as it counts where another plays on. Words
  over a gathered rest (`R1*24^"Solo"`) and a tempo mark at it stand over
  its count instead of through it.
- A note or a rest longer than what is left of its bar is cut at the bar
  lines into tied notes (rests), in either voice of a staff, as LilyPond's
  `Completion_heads_engraver` cuts it — `\time 3/4 c'2 d'2` was refused
  with "a note runs past the bar line".
- A score reads the sustain pedal, `\sustainOn` and `\sustainOff`: "Ped."
  and "*" under the staff (Emmentaler's own signs, two glyphs added to the
  table through `xtask smufl`), and the pedal held in MIDI (controller 64).
- A score reads LilyPond 2.24's jumps, `\jump "D.S. al Coda"` and `\fine`,
  set in italics against the bar line that ends their bar; a rehearsal
  mark at the same bar line stands over them instead of through them.

### Fixed

- A score's ending bracket ("1.", "2.") stands over the words above the
  staff under it — a chord name such as `^"E7"` on an ending's first note
  was drawn through the bracket's number — and over the scripts, tuplet
  numbers and slurs there, as LilyPond's outside-staff priorities put it.
- A rest of two voices on a staff clears the notes the other voice sounds
  with it, moved by whole spaces as LilyPond moves it: in Bach's C major
  prelude the left hand's sixteenth rest was drawn on its middle C.
- Dynamics on notes side by side (`c''4\fff d''\pp e''\p f''\fz`) no
  longer run into one word: the two notes stand far enough apart for both.
- An ending's number clears the high notes under it: an `a''` starting
  "1." in Ode to Joy was drawn through the number, the bracket having
  cleared the notes by its line alone.

## [0.11.0] - 2026-10-18

### Added

- Protocol 11: a `score` reads LilyPond as people write it (spec 03 §1.4) —
  variables and `\global`, `\new Voice` with `\voiceOne`/`\voiceTwo`,
  `\new Lyrics \lyricsto`, `\language`, `\fixed`, `\transpose`, `q`,
  `\partial`, `\cadenzaOn`, spacers anywhere, `\repeat unfold` and
  `percent`, grace notes, the tenor, soprano, baritone, `treble_8` and other
  clefs, fingerings, `\markup`, `\mark`, `\ottava`, phrasing slurs,
  `\trill` and other ornaments, `\arpeggio`, `\glissando`, tremolos, and
  `\new ChordNames`, `\new DrumStaff` and `\new TabStaff` — each drawn as
  LilyPond 2.25 draws it, in the client, the server's native engraver and
  `<eui-score>`. A server sends a session below 11 the paths of a source
  that needs it (`eui_score::protocol`). The site's /scores page shows
  each of these in an example, drawn, with its source.

### Fixed

- Two voices under `\relative`: the second is read from the first voice's
  last pitch, as LilyPond reads it, not from the pitch before `<<`.
- A system of several staves draws the metre on its first bar, where it
  kept the room for it and drew nothing.
- A bar line written in one staff is the system's, drawn on every staff and
  spaced as the widest, as LilyPond draws it: a final `|.` under chord names
  ran off the edge.

### Changed

- The demo at eui-data.solisoft.net runs on the server's own `soli`
  (`soli serve` in its `app.infos`), as the other sites do, instead of a
  version pinned in its workflow and installed beside it: it sat on 2.27.2
  after 2.28.0 shipped. The deploy's checks boot the latest Soli release
  and warn when the server's differs.
- The score editor moves its selected note in the window (protocol 10,
  Soli 2.28.1): dragged or with the arrows, drawn and heard at once, and
  sent to the server once it settles. A click selects a note and never
  moves it, the second click included; a drag moves it.
- On a chord, ↑/↓ move one note — the one last clicked or moved in it,
  else the top one — not the whole chord, and that note is the one drawn in
  the selection's colour, the rest of the chord in the ink. A drag with
  Shift held moves the note by semitones.
- The shell shows which client it is: `eui --version`'s line at the foot
  of a new tab's page, and the version beside the build in the window's
  title.

### Added

- `<eui-score>`: music in any web page, read only. `crates/eui-score-web`
  is the engraver compiled to WebAssembly (593 KB, 175 KB with brotli),
  drawing SVG in the page's text colour, laid out to the element's width a
  page at a time. `<script type="module" src="/eui-score/eui-score.js">`
  and `<eui-score src="piece.ly">`. The site shows it at
  [eui.solisoft.net/scores](https://eui.solisoft.net/scores): a tune, a
  full score with a pager, a sheet of A4.

### Fixed

- A note dragged in a score went where the hand went and then jumped: each
  frame measured the hand against the staff nearest it, which in a full
  score is soon the next one down. A drag is now measured from where the
  hand pressed, half a staff space at a time, whatever the music does
  under it.
- Editing a note above the clef moved the whole score: the engraver places
  a system by its highest ink. The client keeps the edited system where it
  was drawn; the score never jumps up under the hand.
- In the score editor, a click that selects a note no longer moves it: the
  hand trembling as it clicked was taken for a drag, and the note jumped to
  the line under the pointer. A note already selected, or one the click has
  just written past the end, is still dragged.

## [0.10.0] - 2026-10-17

### Added

- Protocol 10: a `score` the client edits (spec 03 §1.4). With
  `edit: ["pitch"]`, a `selected` note and a `score_edit` handler, the
  client moves that note's pitch itself — a drag carries it by the lines the
  hand travels, the arrows move it a step in the key, a semitone with Shift,
  an octave with Ctrl — rewrites the source, draws and plays it at once, and
  reports the pitches once they settle (`score_edit`, event `0x24`, 06 §1.5):
  after `edit_after` (2 s by default), on blur, and before any other event.
  A drag in a full score no longer waits on a server render per line.

## [0.9.0] - 2026-10-17

### Added

- Protocol 9 has a `score` node: music the client engraves from LilyPond
  source (spec 03 §1.4). The server sends the source and nothing else; the
  client lays it out with `eui-score`, the catalogue's engraver, and draws
  exactly the paths the catalogue would have sent. Two events carry back
  what only the client then knows: `score_pick` says where in the music a
  press, a move while pressed or a release landed — page, system, staff,
  line, the nearest note and how far, the notes either side — and
  `score_info` says how many pages the music took, where each starts, and
  why a source was refused. A client below 9 is sent the catalogue's
  canvases by the server, so nothing breaks for it.
- The score editor sends a protocol-9 client the source alone and answers
  its presses from `score_pick`, so the server no longer engraves for the
  screen; on Beethoven's exposition a note moved in the full score is about
  30 ms of server time, none of it engraving.

- The score editor's "Download PDF" prints the part of the track you are
  on when the piece has several — that staff alone, its name set as the
  part's instrument — and the full score from the full score. The button
  says "Download part PDF" then, and the file is named after the track.
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
- In the score editor's Insert mode, a note written in another value onto
  a note no longer replaces it: it goes into the second voice, from where
  that note starts, and both sound.
- The score editor's Examples menu has four more pieces: a song with
  lyrics, a piano minuet, a two-voice Ode to Joy and a three-voice round.
- A source with two voices on a staff is read back as two voices; it was
  read as one, the second voice's notes after the first's.
- With two voices, a click selects a note of either and switches to its
  voice; the first voice's notes can be picked again after writing in the
  second.
- A score of several staves is drawn a band per staff: a note moved in
  the full score resends one staff (17 KB on Beethoven's Fifth), not its
  whole system (up to 255 KB).
- Insert mode writes two eighths under one quarter: in the second voice a
  click falls at the moment under the pointer, rounded to the value
  chosen, not at the nearest note's start.
- A second voice's notes either side of a spacer are no longer beamed
  together, and crossed voices stand apart instead of running their stems
  through each other's heads.
- **Lyrics** in `score()` and `eui-score`: `\addlyrics { Frè -- re Jac -- ques }`
  after a staff's music sets a syllable under each note, with hyphens,
  extenders and melismata on ties and slurs, the notes spread to make room;
  the score editor has a Lyrics tab for each track.
- The score editor's words: the tempo's text is a "Tempo mark" with what
  it prints shown beside it; words on a note are one field with Above /
  Below, Clear and common marks (dolce, cresc., rit., pizz. …) a press
  each; typing them is one step to undo, not one per letter.
- Moving notes in a large score no longer slows down as you go: an undo
  step keeps only the track edited (28 KB on Beethoven's Fifth, not 390 KB),
  and is undone on that track.
- The score editor keeps its frame: Play, Undo and the title, subtitle,
  composer and arranger fields stay in a bar on top, the toolbar stays down
  the left, and only the score scrolls. On the full score the toolbar
  leaves out the insert tool and the voices.
- The score's PDF has a footer on every page — the title at the left, the
  page's number out of how many at the right — and the titles stand clear
  of the top margin and of the first system, on a screen and on paper.
- A dragged note follows the hand: a server behind on its events handles a
  run of moves once, with the last position (06 §2) — the note no longer
  goes on moving for a second after the release.
- The score editor has an **Insert** mode beside the pointer (N, Escape to
  leave it): a click on a note adds the pitch to it as a chord, a click on
  a rest turns it into a note of its length, and a click between notes
  writes a new one there, in the value the palette shows — which in that
  mode is the next note's, never the selected one's — while the pointer
  goes on picking notes up to move them.
- **Two voices on a staff**: `score()` and `eui-score` read LilyPond's
  `<< { … } \\ { … } >>` — stems up and down, shared columns, rests out
  of the way, spacers `s` — and the score editor writes them: Voice 1 /
  Voice 2 beside the tools (Ctrl+Alt+1, Ctrl+Alt+2), every key and click
  working on the voice chosen.
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
  it in a browser, and the gallery has it as its Campaigns section.

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

- After the score editor had run, other demo pages served by the same
  worker failed with "Cannot call non-function value" — the counter, the
  todo list, the gallery, the table, the tracker, Music and three tutorial
  steps. Four of the editor's locals were named after functions
  (`button`, `chip`, `list`, `music`) and rebound them for the worker.
- The demo at eui-data.solisoft.net runs on a Soli of its own, 2.27.2,
  installed beside the site by its deploy, instead of the server's 2.5.5:
  the score editor engraves natively there, and Beethoven's exposition opens
  in a fraction of a second instead of about thirty.
- The score editor's full score answers an edit about seven times faster:
  on Beethoven's exposition a note moved went from 258 ms of server time to
  37 ms. Each track's music is laid out once and remembered, instead of
  every track twice per edit.
- A window on macOS draws as soon as it opens, instead of staying empty
  until it was clicked or resized. Since wgpu 30 the Metal surface gives
  no texture while macOS reports the window occluded, which it still does
  for a moment after the window is shown, and that first frame was dropped
  with nothing asking for another. The client now redraws when the window
  is uncovered (also after a restore from the Dock), and asks again after
  reconfiguring a lost or outdated surface.
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
