# 09 — Conformance

Status: **normative**. `cargo run -p xtask -- conform` runs everything below
and fails on the first deviation.

## 1. What conformance means

An implementation of EUI/1 is **conforming** when it passes the vectors in
this document's harness. The vectors are executable, live beside the code
they pin, and are the tie-breaker when prose and behaviour disagree: a
vector that contradicts the prose is a bug in the prose until the vector is
changed, and changing a vector is a protocol change.

Two profiles:

- A **client** MUST pass §1.1 (transport), §2 (wire), §3 (tree),
  §4 (layout), §5 (theme), §6 (bytecode) and §7 (events); §8 (painting)
  applies when it draws, and §11 (scenes) when it draws those.
- A **server** MUST pass §2 and §6 for what it emits, and §9 (diff) if it
  patches rather than re-mounts.

The harness also builds the seven portable crates — `eui-proto`, `eui-tree`,
`eui-theme`, `eui-layout`, `eui-text`, `eui-vm` and `eui-shader` — for
`aarch64-linux-android`, `aarch64-apple-ios` and `wasm32-unknown-unknown`
where those targets' standard libraries are installed, and skips each with a
note where one is not. Three, because they fail differently: the first
catches a `cfg` written for Unix that Android happens to satisfy, the second
one written for Apple that a Mac satisfies and a device does not, and the
third one written for anything with an operating system at all. It then
builds `eui-client` twice more, in the two shapes no ordinary build reaches:
`--target wasm32-unknown-unknown --no-default-features`, where four
capabilities are off and a different transport module is compiled, and
`--cfg no_subprocess`, where a body of code a desktop never sees is turned
on. A `cfg` nobody compiles is a `cfg` nobody checks.

None of that is a vector and nothing conforms by passing it. It is there
because those seven crates carry no `cfg` for any platform and the claim is
worth keeping true: a client for a phone, or for a page, or for anything
else, starts by taking them unchanged.

### 1.1 Transport — `crates/eui-client/tests/driver.rs`, `crates/eui-client/tests/dial.rs`

01 §1 and §2.3, which had no vectors at all until the rule they state was
found to be two rules that contradicted each other.

1. **`wss://` always, `ws://` only to loopback, and only when asked.**
   `plain_text_reaches_loopback_and_only_when_it_is_asked_for` in
   `driver.rs` walks every case: the three addresses §1 names, each with and
   without a port; each of them refused when nobody has asked; each allowed
   when somebody has, **in whatever build this is**; and the exception
   buying nothing for any other origin.
2. **Loopback is a host, not a prefix.** `dial.rs`'s unit tests refuse
   `ws://localhost.evil.example/`, `ws://127.0.0.1.evil.example/` and
   `ws://localhost@evil.example/` — all three of which pass a
   `starts_with`, and one of which is a request to somebody else's server.
   A client that decides by prefix has handed the exception to whoever
   registers the name.
3. **An embedded host vouches for its own session only.** The trust is a
   parameter and not a process flag, so one embedded session cannot turn
   `ws://` on for a network session opened beside it.
4. **An HTTP status is an answer, not a dropped socket.** `dial.rs` pins
   that a refused upgrade ends the tab with the server's reason rather than
   climbing a retry ladder, and that the reason reads as a sentence.
5. **Every frame kind is classified for lazy dialling.** All fourteen of 01 §3,
   written out rather than sampled, plus a line that fails the day a
   fifteenth is added — because the failure modes either way are silent: a
   kind wrongly dialling opens a session when a window is dragged, and one
   wrongly not dialling drops a click into nothing.

### 1.2 Assets — `crates/eui-client/tests/assets.rs`

01 §2.2's budget, and 10's number for it.

1. **A response past the room left is abandoned on its declared length.**
   `a_fetch_past_the_room_left_is_abandoned`: one byte over the room is
   `TooLarge`, exactly the room is the body.
2. **A still is held once, and everything held is counted.**
   `the_store_counts_what_it_holds_and_keeps_one_copy_of_a_still`: a PNG's
   file goes when its pixels are held, a WebP's stays, and `held` is the sum.
3. **Past the budget, what nothing names goes, oldest first; what is named
   stays.** `the_store_lets_go_of_what_nothing_names_least_recently_used_first`,
   including that the natural size outlives the pixels and that a hash let
   go is fetched again when it is named again;
   `the_driver_lets_go_of_a_picture_the_page_stopped_showing` is the same
   through the driver, with the room measured against the named picture.
4. **A failed fetch is tried again.** `a_failed_fetch_is_asked_for_again`:
   the retry is not due at once, is due after its wait, and asks.
5. **A silent server is given up on.** `a_server_that_stops_talking_is_given_up_on`:
   a server that accepts and says nothing is a `Timeout` on the idle limit.
6. **A page of pictures is a few connections.**
   `a_page_of_pictures_is_fetched_by_a_few_workers`: sixteen fetches at
   once are all answered, with never more than `FETCHES_PER_ORIGIN`
   connections open to the origin.
7. **A picture is decoded off the thread that paints.**
   `a_picture_is_decoded_off_the_painting_thread`: handed over, it is not
   held yet; the driver asks to be woken while it is in flight; it lands on
   a tick; the words beside it are not measured again; and the driver is at
   rest once it has landed. `crates/eui-client/tests/worker.rs`'s
   `a_picture_is_decoded_in_the_confined_worker_and_lands_later` is the
   same across the process boundary, where the decode threads run under
   seccomp.
8. **The worker closes its door on threads that are running.**
   `crates/eui-client/tests/worker.rs`'s
   `every_thread_has_started_before_the_sandbox_closes`: 128 confined
   workers lock down at once, and none is killed for a call one of its own
   threads made on the way in (08 §10). Nor on the way out:
   `a_decode_thread_gives_memory_back_without_being_killed` has a decode
   thread grow its heap and free it, which glibc follows with a trim.

### 1.3 A session over a pipe — `crates/eui-client/tests/pipe.rs`, `crates/eui-proto/tests/frames.rs`

01 §7. Every name below is a test that exists and passes.

1. **Frames end where their `len` says.** `frames_on_a_pipe_are_cut_by_their_own_length`:
   three frames written back to back, split at every byte boundary across
   reads and then fed a byte at a time, come out as the same three frames
   and then the clean end.
2. **A declared length is checked before it is reserved.**
   `a_pipe_frame_past_the_limit_is_refused_before_it_is_read`: a header
   declaring `MAX_FRAME_BYTES + 1` with no payload behind it is refused on
   the header — a reader that tried to read the payload would report the
   pipe ending instead.
3. **A pipe that ends mid-frame is a truncated frame**, not a clean end:
   `a_pipe_cut_mid_frame_ends_with_a_reason`, at every cut, and through a
   connection as a `TransportError::Pipe`.
4. **The client speaks first, at version 8, with nothing to resume.**
   `a_pipe_hello_offers_eight_and_no_resume`: the driver's `Hello`, and the
   first bytes the application reads off the pipe are exactly it.
5. **An asset is fetched in the session and verified.**
   `an_asset_arrives_over_the_pipe_in_chunks_and_is_hashed`: one asset over
   three chunks lands; one whose bytes do not hash to its name is discarded;
   one asked for twice while on its way is asked for once, and once answered
   `aborted` may be asked for again.
6. **An asset nobody asked for, or past its `cap`, ends the session.**
   `an_unasked_or_oversized_asset_ends_the_pipe_session`.
7. **`Fetch` and `Asset` do not exist on a socket.** In `eui-proto`,
   `fetch_and_asset_are_refused_on_a_socket`: both decode with
   `decode_pipe` and are rejected as unknown kinds by `decode`;
   `fetch_and_asset_have_the_bytes_01_7_3_gives_them` writes the bytes out,
   and `an_asset_chunk_is_bounded_like_a_transfer` holds a chunk to §6's
   sizes.
8. **The end of the pipe after `Error` shows the reason; without one, the
   window closes.** `a_pipe_that_ends_after_an_error_shows_it` and
   `a_pipe_that_ends_cleanly_closes_the_window`, through `pipe::ending` —
   the decision `Tab::pump` acts on.
9. **Standard output carries frames only.** `nothing_but_frames_on_stdout`
   runs the `eui` binary with `--pipe`, `EUI_TRACE=1`, an empty pipe for
   input and no display, and decodes everything it wrote: what it checks is
   the start-up and its refusal, every word of which goes to standard error.
   A session with a window is not run here — a test machine has no screen.
10. **No island opens from a pipe session.**
    `an_island_in_a_pipe_session_keeps_its_children`: `dial::island_url`
    has no address for a pipe session, and a tree naming an island keeps the
    child it was rendered with.

The Ruby server's half is `clients/eui-ruby/test/pipe_test.rb`: the bytes of
both frames, a session over two pipes that mounts and answers a click, an
asset in three 256 KiB-bounded chunks that hashes to its name, the two
`aborted` answers, a length past the limit, and `run_pipe` starting a
stand-in client with the arguments it was given.

## 2. Wire format — `crates/eui-proto/tests`

| File | Pins |
|---|---|
| `vectors.rs` | Byte-exact encodings: a 64-byte style record, varints, the counter's Mount batch, a `DefFont`, a `DefGradient` and the `ColorRef` naming it (`def_gradient_bytes`), every op |
| `roundtrip.rs` | Every frame, op, value and record survives encode → decode unchanged |
| `reject.rs` | 64 malformed inputs, each refused with the named error and no allocation past the limit: truncation, non-minimal varints, trailing bytes, unknown enums, undefined `animation` bits, a `motion_kind` with nothing going that way, oversized lists, depth, a font role past the last, a role bound to no face, too many faces on one |
| `size_budget.rs` | The counter's Mount fits 576 B and a click 25 B |
| `alloc.rs` | 08 §5's reservation rule, watched at the allocator: a value list whose header claims `MAX_VALUE_LIST` items and carries none is refused as truncated having reserved under 4 KiB, not the 40 MB its count asked for |
| `reject.rs`, again | **06 §4's payload shapes**, which are not decode failures: every payload there is a well-formed `Value` in a valid frame, and the question is whether it means what its kind says. A `click` carrying a string, a null, one coordinate, three, or a thousand; a fractional button, slot or scroll offset; a payload on a kind that declares `Null`; an NFC tag whose records are not pairs. Plus the two properties the rest rests on: an integer stands in for a float and never the reverse, and **no kind accepts a payload of nonsense** — the vector that stops a catch-all arm turning the whole check into a no-op |
| `manifest.rs` | The manifest record round-trips, its signed bytes are rebuilt exactly, malformed records are refused, and an `icon` makes it a version 2 record while a manifest without one stays byte-for-byte version 1 |

`roundtrip.rs` and `vectors.rs` cover 01 §4.1 and §6 as well: a `Hello` that
offers a session back, a `Welcome` that resumed one, and a transfer in each
direction with each of its three flags, byte for byte. `reject.rs` refuses an
unknown transfer flag, an oversized chunk, an oversized abort reason, an
unknown resume tag, an unknown `resumed` byte, and a `Hello` that ends
before its resume flag.

05 §2's version-6 `space` steps are a record a server rewrites per session:
`a_space_step_an_older_session_lacks_falls_back` (`roundtrip.rs`) holds that
`StyleRecord::for_protocol` leaves a record alone at 6, replaces 13–17 with
2, 3, 4, 11, 12 below it on `gap`, `padding`, `margin` and every
`Dim::Space`, touches no pixel count or other scale, and never answers an
index a version-5 scale lacks.

02 §5.3's gradients and 03 §5's `pulse` and `bounce`, the rest of version 6:
`a_gradient_definition_round_trips` (`roundtrip.rs`) sends every angle,
`0..=363`, with two and three stops; `a_gradient_is_refused_outside_its_rules`
(`reject.rs`) refuses `0x4000` as a `bg`, a gradient in `fg`, in
`border_color` and in a `Value` colour, an angle past 363, one stop and four,
a stop that is none or a gradient, stops that go backwards, id 0, and a
definition cut short; `an_animation_bit_this_revision_does_not_define_is_rejected`
takes 8 and 16 and their combination with `spin`, and refuses 32 up; and
`pulse_and_bounce_and_a_gradient_are_version_6` holds that `for_protocol`
leaves all three alone at 6 and below it takes off the two bits and any
gradient `bg`, and nothing else.

### 2.1 Font roles — `crates/eui-text/tests/shape.rs`, `crates/eui-client/tests/assets.rs`, `crates/eui-tree/tests/apply.rs`

02 §5.1. `font_family` is the one style byte with an open half, so `2` is a
*role* rather than an unknown tag and `reject.rs` pins that it decodes. The
rest is the behaviour the section promises and the client owes:

1. **A face is read and names itself** — `add_font` answers the family out of
   the face's own tables, and refuses bytes that are not a face at all.
2. **A role draws in the face bound to it**, and rebinding a role throws away
   what was shaped under the old binding. Rebinding a role to the family it
   already holds costs the cache nothing.
3. **Sans and mono are roles**: an application may replace either, and when
   the session ends the client's own faces come back.
4. **A missing face never costs a page.** A role no `DefFont` bound, a role
   whose asset has not arrived, and a role whose bytes would not parse all
   draw in `sans`, and the session stands in all three.
5. **The faces are asked for from the session**, not from the tree: a role's
   hashes are wanted because a `DefFont` bound them, and asked for once.

And the embedded side of it: the client carries a face for each of the four
`font_weight` values, so `each_weight_draws_in_a_face_of_its_own` shapes one
word at each weight and finds four faces and four widths, each wider than the
last — `medium` and `semibold` are not `regular` and `bold` under other
names.

## 3. Tree — `crates/eui-tree/tests`

Tables never cleared — including by a `Mount`; a batch that fails leaves the
session poisoned until the next `Mount`; every reference (atom, style, chunk,
node, key) validated before anything is placed; `MoveChild` indexes after
removal. Font roles are the one table that is not define-once: a role holds
the faces bound to it, may be bound again, and one past the last is refused
in the session as well as in the decoder.

`apply.rs` also pins the quotas that are session state (02 §6):
`chunk_bytes_have_one_budget_across_the_page_and_its_islands` refuses the
inline chunk that would pass `MAX_CHUNK_TOTAL_BYTES`, counting an island's
bytes against the page's; `atom_budget_is_enforced_before_storing` does the
same for atoms. `a_gradient_is_defined_once_and_named_only_after` holds 02
§5.3's table to the same rules as the colours: a record naming a gradient
before it is defined is refused, so is a literal stop whose colour is not,
an id is defined once and past `max_gradients` not at all — and an island's
gradient 1 is its own, beside the page's. And that a refused graft is refused **whole**:
`a_refused_graft_leaves_nothing_behind_and_the_resync_lands` sends a subtree
with an id used twice, one past the depth limit, and a bad `InsertChild`,
and requires that none leaves a node live — so the resync's `Mount`, which
carries the same ids, lands rather than being refused as a duplicate.

## 4. Layout — `crates/eui-layout/tests/layout.rs`

Goldens against the fixed-pitch measurer (9 px per character, 22 px lines):
flow, grow, shrink with the automatic minimum, wrap, justify, align, baseline,
percent, stack (stretch on both axes, absolute layers), grid, scroll clamping,
virtualised lists, display none, depth 255 within a 1 MiB stack.

Plus **what a node's kind says about its layout**, which 03 §1's table states
and nothing checked: a `slot` lays out as a column where a `box` with the same
style lays out as a row, and a `slot` that is `display: none` is still gone —
the kind decides the direction and must not put back a node the author took
out of the flow.

**Measured at the width it is drawn at** (04 §2, §4.1):
`max_width_is_the_width_text_wraps_at_and_is_measured_at` — a paragraph with
only a `max_width` wraps at it and its sibling starts below;
`a_stretched_child_of_a_column_wraps_at_the_column` — text with no width
follows its column's; `a_flex_1_column_in_a_stretched_row_is_as_tall_as_its_share_makes_it`
— a `grow`, `basis: 0` child of a stretched row is measured at its share,
and the row is as tall as its tallest child, not as a word-a-line column.
The first and the last fail on the 0.7.0 engine.

And **what is hit is what is painted** (03 §2 item 7):
`overflow_clip_trims_the_hit_as_it_trims_the_paint` refuses a point on text
an `overflow: clip` box cut away, and `what_is_culled_from_the_paint_is_not_hit`
refuses a child that spills out of a box scrolled wholly out of view — the
painter drops that box and everything under it, and so must a hit.

An absolute child that names its edge (04 §5, version 7):
`an_absolute_child_may_name_its_own_edge_across_a_stack` places one against
the end of a stack that packs from the start, less its right margin, one
centred, and one against the start of a stack that packs from the end, while
a plain `absolute` sibling still follows the stack's `justify`.
`an_absolute_child_naming_its_edge_is_version_7` (`roundtrip.rs`) round-trips
3–5, refuses 6, and holds that `for_protocol` sends a session below 7 plain
`absolute` with nothing else changed; lang's
`an_absolute_child_naming_its_edge_is_plain_absolute_below_7` holds the
server to it.

A scroll is not a new layout (04 §7): `a_scroll_alone_translates_to_what_a_full_layout_places`
moves the boxes under scrollers that scrolled — nested, clamped past the end,
a stack out of z order among them — and compares every box and every hit with
a full layout of the scrolled tree; `a_scroll_with_anything_else_is_left_to_a_full_layout`
and `a_virtualised_list_scrolled_recomputes_its_window` pin when it must not.

## 5. Theme — `crates/eui-theme/tests`

Every default pair in 05 §4.3 meets its contrast; a seed produces a ramp whose
`on` colour passes against `base`; scale indices out of range are refused;
the easing curve is monotone and pinned at both ends.

05 §2 and §3's Tailwind values are pinned rather than described:
`the_scales_are_tailwinds` holds the text scale and every shadow layer,
`radius_scale_derives_from_radius_md` the default `4 8 16`, and
`the_default_palette_is_tailwinds_gray_and_indigo` compares each light role
with the Tailwind colour it stands in for, in OKLab ΔE with a tolerance per
role (1 for the surfaces, `border.subtle` and `accent.base`; 6 for the
contrast-bound `border.strong`). The light hover, which lightens, is held to
3:1 under `accent.on` for every seed `contrast_holds_for_hostile_seeds`
draws.

`the_space_scale_appends_tailwinds_half_steps` holds the eighteen `space`
entries, their count against `space_steps(PROTOCOL_VERSION)`, density on the
appended ones, and derives each fallback from the pixels — nearest older
step, ties down — so the table in `eui-proto` cannot drift from the scale it
stands in for. `check_style_rejects_indices_past_a_scale` accepts 13–17 and
refuses 18.

## 6. Bytecode — `crates/eui-vm/tests`

The verifier refuses bad jumps, stack underflow and overflow past 64,
unknown opcodes and oversized strings; fuel exhaustion aborts without effect;
every opcode's stack effect is exercised.

For the floats (07 §3), one vector each for the property that makes them
safe to let near a uniform: that a float and an integer are different types
and `to_float` is the only bridge, that a division by zero, the root of a
negative and an overflow each abort the run with nothing left half-done,
that a non-finite literal cannot be smuggled in, and that a float read back
out of local state is the float that was put there — which is what an
integrator between frames rests on. `set_uniform` refuses an index of 8 and
a value that is not a float.

## 7. Events — `crates/eui-client/tests/driver.rs`

Click resolution to the nearest handler, payloads local to the handler's
node, press and release on different targets, focus by pointer and by `Tab`
with wrapping, `Enter` and `Space` as clicks, `Escape`, the ring for keyboard
and server focus only, editing and commit, wheel and scroll offsets, local
handlers with and without a following server event, resync on a bad batch,
transitions on a style change, a back reaching the mounted root and a back
reaching nobody when the root holds no handler for one.

`Viewport` during a resize (01 §3): a single step goes at the next paint,
and the window then sleeps (`a_resize_that_settles_while_nothing_moves_leaves_the_window_at_rest`);
a 300 ms drag the server answers is followed during the drag, a size about
every 32 ms and never faster, each the width the window had then
(`a_drag_the_server_answers_is_followed_during_the_drag`); one it does not
answer sends one size, then only the size it settled at once the answer wait
runs out (`a_drag_the_server_does_not_answer_sends_one_size_then_the_last`);
and through a real worker process the size arrives
(`a_resize_after_the_mount_reaches_the_server_through_the_worker`, `worker.rs`).

And the two kinds that had payloads, decoders and a server mapping the word
while no client had ever produced one (§2): **`double_click`** — the first
click is a click alone, the second is a click *and* a double in that order,
a third starts a new pair rather than reporting a second double, and a pair
600 ms apart is two clicks; and **`resize`** — a node's first layout says
nothing, a node pushed sideways by an inserted sibling says nothing, a node
whose style changes its width reports the new box once, and painting it
again reports nothing.

**03 §3's pointer shape, across a rebuild.** Three, because the shape is
worked out from the node under the pointer and the pointer is the one thing a
batch does not move. A beam previewed by a local hover handler survives the
batch that puts the server's style back, because the hover it is owed has not
been settled yet and the pointer has not moved. A hand survives the row under
it being **replaced**, where no local style is involved at all and the only
thing that changed is which arena slot the node lives in — and `hovered()`
still names the node rather than whichever sibling was rotated into its
place. And a node that genuinely left in a batch holds the last shape until
the paint that settles the hover, which is where that settle's `leave` and
`enter` belong; between the two, nothing at all goes on the wire, which is
06 §3's clause about a rebuild not being a gesture.

**03 §5.3, pairing**, which had no entry at all while `motion_kind` 6 was a
byte no client acted on: a node arriving under the key one that just left
was wearing starts at its partner's box and its partner's width — the scale
is what proves it, since no other motion produces one — ends where the
layout put it, and never fades, because the element arrives rather than
appears. A `paired` node whose key resolves to nothing is **accepted**, as
§5.3 requires in as many words, and drawn where the layout put it carrying
no transform of its own.

Two more, because the first of those vectors put both of its nodes at the
same origin and so could not tell a **centre** from a **corner**: a pair
whose two boxes differ in origin *and* in size starts with its centre on its
partner's centre, which is not where a corner delta would have put it — off
by half the difference in their sizes, and the two agree only when there is
no difference, which is when there is no pair. The composition itself is
pinned a layer down, in `crates/eui-render/tests/render.rs`: an offset and a
scale taken about the same pivot, the only vector in that file that scales by
anything but one, and the reason the arithmetic above has a right answer to
be wrong about.

And pairing across a page, which is what the section is for: a key that left
inside a released subtree pairs, not only one that was the released node
itself. A server's diff names the page and a release notes its root, so a
client that read departures off that list alone paired a shared element that
*was* the page and nothing else — passing every vector above while the
feature did not work.

Sound reports on one clock and says nothing about the machine: `level` and
`time_update` are emitted on the same tick and neither twice in it; a
`level` reaches only a node that holds a handler for one; a meter that has
not moved sends nothing and a sound that stopped sends one last zero. The
property that carries 08 §8 is pinned a layer down, in
`crates/eui-audio/tests/audio.rs` — the reported peak does not change when
the viewer's master gain does, including when it is zero. So is 10's
ceiling on one decoded sound: a sound one sample past the budget it was
decoded under is refused, and a caller cannot raise the budget past
`MAX_BYTES` (`a_sound_past_its_decoded_budget_is_refused`); the same
for a moving picture in `crates/eui-video/tests/video.rs`
(`a_picture_is_refused_past_the_room_it_was_given`). In `driver.rs`, the
session's side of both: a decoded sound and a decoded picture are gone the
frame no node names them (`decoded_media_goes_when_no_node_names_it`), and
the second of each is refused when only one fits the room the session has
left (`decoded_media_past_the_sessions_room_is_refused`).

03 §3.5's address is pinned in the same file, and the three conditions are
pinned apart: a tree that merely arrives opens nothing; an activation with
the `net.open` grant yields the address exactly once, and a second read of it
yields nothing; an activation without the grant yields nothing at all; and a
scheme that is not literally `https://` — `file:`, `ms-msdt:`, plain `http:`,
or an authority carrying credentials — never reaches the platform. The same
cases are pinned a layer down on `https_host` itself, where whitespace,
quotes and control characters are refused with them.

02 §5.2's notification is pinned there too, and the same way: a batch with
the `notifications` grant yields its lines exactly once, with control
characters gone and an untitled one dropped; the same batch without the
grant yields nothing and still mounts the tree it carried, because a
notification is not part of the document. The per-batch ceiling is pinned a
layer down, in `crates/eui-proto/tests/roundtrip.rs`, where a fifth
notification in one batch is a decode error and four are not.

### 7.1 Keyboard — `crates/eui-client/tests/keyboard.rs`

§3.1's tab clause: a focused node carrying `typing` receiving `Tab` and
`Shift+Tab` as keys with focus left where it was, a node without the prop
still having both moved out from under it, and `Ctrl+Shift+Tab` moving focus
backwards past a node that carries `typing` and claims every key it can —
the one chord a surface may not take.

§3.1's paste clause: a focused `typing` node hearing nothing from typed text
and **everything from a paste** — reported as `text_input`, newlines and all,
with the client inserting nothing and focus unmoved.

§3.1: `Escape` reaching a handler on the path and leaving focus alone, and
still only blurring when nothing asked for it; `modal` keeping `Tab` inside a
subtree, nesting so the innermost wins, and leaving an unmodal page alone;
`autofocus` placing focus when a surface arrives and *not* reclaiming it on a
later batch; and `keys` letting a node take the arrows while `Enter` stays the
press it stands for, sending it no key it did not name, and leaving a node
without the prop hearing everything.

§3.1 inside a field, which is where the withholding clause earns its three
parts: naming a key cannot make a field undeletable, and the same key with
nothing left to delete reaches the node that asked for it; an arrow with
somewhere to go is the client's and one at the end of the line is not; a
claimed `Enter` reporting the value *before* the key and withholding only the
`submit`, and an unclaimed one still submitting; a node with a bare handler not
swallowing the `Enter` that presses a button inside it; and a printable
character landing in the field whatever the prop says, because text never
arrives as a key. And (protocol 9) a handler above a field that named no
`keys` hearing nothing typed into it — `Backspace` with nothing to delete, the
arrows, `Ctrl+Z` — while the same handler hears everything once focus is its
own (`shortcuts_above_a_field_hear_nothing_typed_into_it`).

### 7.2 Accessibility — `crates/eui-client/tests/a11y.rs`

§6.1's declared half: a `role` prop beating the kind it would have been
inferred as; `checked` in all three of its states; a `label` overriding the
text inside; a disabled node keeping its role and accepting neither action;
a container role keeping the children a leaf role would swallow; `set_size`
carrying a count virtualisation left out of the tree; a slider's value and
range, including a present zero; an unknown role name falling back rather
than failing; a live region's urgency; and every role discriminant surviving
the `to_u8`/`from_u8` round trip the worker boundary makes of it.

Rule 4: a field naming an option that is its *sibling* and not its child —
which is where a combo box's panel is, and is the whole reason a relation
exists here at all — and a name pointing at nothing being dropped without
refusing the batch, because a panel that has shut leaves exactly that.

The kind-mapping default of §6 is pinned separately, in `driver.rs`, so that
a tree declaring nothing is provably exposed as it was before §6.1 existed.

### 7.3 Manifest — `crates/eui-client/tests/manifest.rs`

Signature, protocol range, trust on first use, refusal of a changed key,
acceptance of a rotation the pinned key signed, and garbage. Pins and
grants are per origin and `app_id` (01 §2.1, 08 §2):
`the_same_manifest_at_another_origin_is_pinned_afresh` verifies one
manifest's bytes at two origins and has the second pinned on first use of
its own, a squatter at one origin not lock the publisher out at another,
and a pin kept by `app_id` alone ignored;
`grants::a_grant_given_at_one_origin_is_not_given_at_another` and
`grants::an_answer_kept_by_app_id_alone_is_not_read` do the same for the
remembered consent.

### 7.4 Files — `crates/eui-client/tests/files.rs`

Spec 03 §3.2 and 01 §6, twenty-one vectors. A click on a node carrying `pick`
asking the window for a dialog with the accept list, multiplicity and
ceiling the tree declared; **nothing opening** without the capability, and
nothing opening for a tree that merely arrived; what was picked becoming one
`file_pick` naming the file and not its path, then chunks that fit a frame;
a file past the ceiling announced and aborted, and accepting no bytes after
it; a dismissed dialog reported to nobody and its token spent; a save
becoming a `file_save` and the server's `Blob` frames becoming writes for
the window; a `Blob` for a save nobody asked for **ending the session**; and
a `Blob` out of order losing the save without ending it. The last vector
runs the two gestures against the reference server over a real socket: the
bytes of a picked file reach it, and the bytes it owes a save come back.

The memory bound of 10 §5 is two halves. The window takes an upload chunk
only while the connection's unwritten backlog is under eight chunks, and
`e2e.rs`'s `the_backlog_counts_what_the_socket_has_not_written_and_wakes_when_it_drains`
has a server stop reading, the backlog hold what the kernel would not, and —
once it reads again — the backlog drain to nothing and the window be woken
as it falls under the low mark. What is held while there is no socket at all
(01 §4.1) is `app.rs`'s: `a_clock_is_not_held_for_a_socket_that_is_down`,
`a_state_report_is_folded_into_the_last_one_and_not_past_a_click` and
`the_offline_queue_is_bounded`.

### 7.5 Resuming — `crates/eui-client/tests/resume.rs`

Spec 01 §4.1, six vectors. A first `Hello` offering nothing and a later one
offering the session and the last batch applied; a `Welcome` that did not
resume taking the tree, the tables and the acked sequence with it; a
`resumed` naming a session the client never offered ending it; a batch
already applied being acked again and **not applied again**. The last drops
a real socket mid-session and opens another: the tree is still standing, the
count is where it was, and the session goes on.

**The server's half** — `lang/src/serve/eui/mod.rs`, five vectors, since
2026-09-20. A resumed session is owed the batches after the one it
acknowledged and not the tree. A handle alone opens nothing: the same bytes
with another cookie, or naming another component, or a handle nobody minted,
all resume nothing — §4.1 makes the handle a bearer, and a bearer that needs
no other evidence is sixteen bytes standing between a reader and somebody
else's data. A session whose socket is still attached is not picked up, or
two live sockets would both be told they own its tree. A client that fell
further behind than the replay reaches is **refused rather than half-filled**:
told it was resumed and handed a tree with a hole in it, nothing would say so.
And an `Ack` drops what it names, or the buffer would hold the last
sixty-four batches of every session that ever ran.

### 7.6 Touch — `crates/eui-client/tests/touch.rs`

06 §5: a tap reaching what it landed on, and still reaching it after five
pixels of tremor; a stroke past the slop scrolling the view and never
clicking the row it began on; the press given back before the view moves; a
node that asked for moves taking the whole stroke while the view under it
stays put; a second contact ignored while the first is live; a cancelled
gesture releasing without a click and forgetting the contact; a finger still
moving as it leaves carrying the view on, and a finger that stopped first
not doing so; and a window that loses the input forgetting the finger that
was on it, so the next one is a gesture and not a second contact.

The tests are the driver's, not the window's — a contact becomes the pointer
before any platform is involved — so they run everywhere and want no touch
screen.

06 §5.1 is here too: a contact held past the timer on a `drag` becoming a drag
and never a tap, one held on a `long_press` handler reporting it and giving the
press back without a click, one that wandered first being a scroll with no
timer left to fire, and a held contact on a row carrying neither doing nothing
at all. The hold is a clock the test names, not a sleep.

### 7.7 Dragging — `crates/eui-client/tests/drag.rs`

06 §6: an arming press that becomes a click being indistinguishable from one
that never armed; the slop turning the press into a drag and taking the click
back; a handle grabbing without it; `drag_over` arriving once a boundary
crossed and not once a sample; the slot being the box the pointer is in, so a
row moved under the hand does not oscillate; a `MoveChild` arriving mid-drag
and the gesture still naming the same row; a target inside the source
resolving nothing; `Escape`, a lost window and a vanished source each ending
it with `slot = -1`; and a drag at the foot of a list scrolling it and
reporting one `scroll` rather than one a frame.

### 7.8 Arriving and leaving — `crates/eui-client/tests/driver.rs`

03 §5's two lifecycles, which are exactly the kind of thing two clients would
diverge on: `animation` read as a bit set, so `enter | exit` still enters; an
entrance that names a direction arriving from it rather than fading; a
released page painting on after the tree has let it go, with the way it goes
being the mirror of the way its replacement came, along the accelerate curve;
one kept painting at a time; and the frame after it is over holding one page,
with the driver at rest and no frame owed.

06 §5's edge clause — `crates/eui-client/tests/touch.rs`: a stroke inwards
from the leading edge carrying the page and reporting a `back` when it is let
go past halfway; the press given back with no `click`; a stroke let go early
springing back and reporting nothing at all; a stroke along the edge still
scrolling; and a tap inside the slop still being the tap it was aimed at.

### 7.9 Adopting a tree — `crates/eui-client/tests/adopt.rs`

Spec 01 §2.6, seven vectors. A client holding a tree that came over §2.4
offers **the tree** and not the sixteen zero bytes that body's `Welcome`
carried; one holding no tree offers nothing. `Adopted` keeps the tree, sends
nothing back, and leaves a real session behind — so the socket after it
resumes rather than offering the tree twice. `Fresh` tears the tree down
**before** the mount that follows, which is the vector that matters: a fresh
mount numbers from 1 again, so a client that kept its acked sequence would
drop it as one already applied and show an empty window with nothing in the
log. `Adopted` to a client that offered nothing is a protocol error. And the
two tag bytes are checked for what they are — extension points on frames that
cannot grow a field — by giving `start` a fourth value and requiring
`UnknownTag`.

### 7.10 Islands — `crates/eui-tree/tests/apply.rs`, `crates/eui-client/tests/islands.rs`

Spec 01 §2.7, in two halves. **The tree's half is written**, in
`eui-tree/tests/apply.rs`, and it is the half the whole design turns on: an
island has its own node ids *and* its own four interned tables, so a page and
an island may both define atom 1, style 1 and node 1 and both live — which a
shared table could not do, since a `DefineOnce` answers a second write to a
live id with `Redefined`. An island naming a node of the page reaches
nothing, because the id index is keyed by owner and there is no such node in
its space: §2.7's "no id it sends can name a node it did not create",
enforced by the shape of the map rather than by a check that could be
forgotten. An island's `Mount` replaces that island's content and leaves the
page's root, the page's nodes and every other island alone; the node's own
children show until it speaks. A ninth island opens nothing, and a batch for
an owner nobody handed out is refused rather than resolved against the page's
tables — which is the failure that would otherwise be silent, since the
island's ops would land on the page. The ceiling is on islands open at
once: `a_closed_islands_slot_is_taken_by_the_next` opens and closes twenty in
one session, `closing_an_island_keeps_the_pages_own_children` keeps the
cached render a silent island never replaced, and
`a_page_mount_that_frees_the_host_prunes_the_island` has a page `Mount` that
releases the host take the island's slot with it, and refuse the frame that
arrives for it afterwards rather than graft it under whatever took the
host's index.

**The client's half is written too**, in `eui-client/tests/islands.rs`, and
the ones that matter are the refusals. An `island` naming another origin is
refused — `https://`, `wss://`, a bare relative path, a backslash UNC, and
`//host/path`, which is the one worth naming: a protocol-relative URL is a
different origin *and* starts with a slash, so the obvious spelling of the
check lets it through. Refused in the listing **and** in the opening, so a
caller that asks directly is refused too.

A ninth island on one page opens nothing and leaves the
node as it was rendered (10 §1). An island whose session cannot be opened, or
which ends, leaves the page standing and its own children showing — the vector
that says a live part can never take a still page with it. Two islands naming
one path share one session; two naming one component with different queries do
not. An island that ended is not redialled while its node stands; one whose
node a page `Mount` released is reported to the window, whose socket goes,
and the new page's node is asked for afresh
(`a_page_mount_that_frees_the_host_ends_the_island`); twenty islands opened
one after another over one session all open
(`a_long_session_opens_islands_past_the_ceiling_one_at_a_time`); a node that
stops naming a path closes that island, content and socket
(`an_island_the_tree_stops_asking_for_is_closed`); and a session that starts
over ends them all (`a_session_that_starts_over_ends_its_islands`). And an event raised inside an island carries that island's node ids and
goes to that island's socket, never the page's — the boundary being the node
carrying the prop, which belongs to the page while everything below it does
not. And a page with an island still draws, lays out and answers a pointer as
**one** tree: the layout engine and the painter never learn islands exist.

The vector that found a real bug is the event one. An island's click must not
join the page's outbound, and the failure if it does is quiet: an island's
node 1 and the page's node 1 are different nodes, so the page's server gets a
well-formed event naming a node it created for something else — which its own
06 §4 validation may accept, since the id exists and may even carry a handler
of that kind. Writing it turned up that the *layout's* style cache was keyed
by style id alone, so an island's style 1 was served the page's: a box asking
for 80×20 came out 400×0, and nothing said a word. Both caches are keyed by
`(owner, id)` now, and `Layout::style_for` takes the owner.

The worker's pipe carries islands too, and `replies_round_trip` in
`worker.rs` pins it: a reply's `island_outbound` survives the encode with its
owners, in a field of its own rather than tagged inside `outbound` — a
separate field has to be read to be sent, and so cannot become the page's by
being forgotten.

And §2.7's `MAY`: an island nine hundred pixels down a three-hundred-pixel
window is not offered at all, and is offered the moment the page above it
shrinks — the same thing a scroll does to where a node sits.

### 7.11 Installing — `crates/eui-client/tests/install.rs`

A signed manifest with an `icon` is verified, its icon fetched by hash and
written at the size the platform's format declares, and a launcher entry
written and then removed exactly. `install.rs`'s own unit tests pin what an
`app_id` may become as a file name, and that a path the record names but
this client never writes to is left alone by an uninstall.

## 8. Painting — `crates/eui-render/tests/render.rs`

One quad per box and per glyph; scissor runs for scroll containers; device
snapping at 2×; shadows, one quad per layer grown by its spread and its blur,
with the empty second layer of `shadow.sm` drawing nothing; canvas paths; the backdrop a `blur` asks for — that
a frame without one asks for none, that the region is the blurred rects
grown by three standard deviations and no more, and that two radii are two
chains; and, where a GPU adapter exists, pixels read back from an off-screen
target for the clear colour, a filled box, a clipped scroll, a canvas line,
a canvas fill (03 §1.1, kind `5`) covering by the non-zero rule — a square
inside a square wound the same way leaves no hole, wound the other way
leaves one (`a_canvas_fill_covers_by_the_non_zero_rule`) — and a cubic
staying inside its hull (`a_canvas_cubic_stays_inside_its_hull`),
a `shadow.lg` whose negative spreads leave the row above its box the bare
surface and darken the row below (`a_negative_spread_keeps_the_shadow_under_the_box`),
and a frosted pane carrying each half of a seam into the other — both when
it covers the whole frame and when it covers a part of it, which is what
pins the backdrop's origin.

A fill's painting is pinned without a GPU as well: one textured quad per
tile from the glyph atlas, one region for an outline repeated at the same
phase, a path with an unknown or short command skipped alone, more than
`MAX_FILL_COMMANDS` skipped whole
(`a_canvas_fill_is_one_textured_quad_per_tile_from_the_glyph_atlas`), and
the mask cut to the canvas's clip and into `FILL_TILE` tiles
(`a_canvas_fill_is_cut_to_its_clip_and_tiled`). A rectangle thinner than a
device pixel keeps one (`a_canvas_hairline_keeps_a_device_pixel`).

A MIDI `src` (03 §7) is pinned by `crates/eui-audio/tests/midi.rs`: a
Standard MIDI File synthesizes to as long as its tempo map says
(`a_midi_file_lasts_what_its_tempo_says`), its notes sound where they are
(`a_midi_note_sounds_at_its_time_and_not_before`), on the client's own
voices without a bank and on a SoundFont's with one
(`a_soundfont_plays_its_own_instruments`), panned by controller 10
(`a_pan_moves_the_sound_to_one_side`); a `notes` list becomes the same
file (`notes_are_a_midi_file_of_their_own`) and is held to its ceiling
(`a_notes_list_past_its_ceiling_is_refused`). In the driver
(`crates/eui-client/tests/driver.rs`): `notes` play when `notes_seq` moves
and not on a render that leaves it alone
(`notes_play_when_their_seq_changes_and_not_on_a_re_render`); a MIDI `src`
asks for its `soundfont` parts and plays on the client's voices when they
are no bank (`a_midi_src_asks_for_its_soundfont_parts_and_plays_without_a_bank`),
and a real bank in two parts plays its own instruments
(`a_soundfont_in_parts_plays_its_own_instruments`, given
`EUI_TEST_SOUNDFONT`).

MIDI instruments (06 §1.4) are pinned in the driver: nothing is heard
without the `midi` grant or with the window behind, and granted, in front
and asked for, a chord arrives as one event in order with the clock byte
gone and the payload the shape §1 declares
(`midi_is_heard_only_granted_in_front_and_asked`); a burst past
`MAX_MIDI_PER_EVENT` is several events with none dropped
(`a_burst_of_midi_is_split_and_nothing_is_dropped`). The window keeps
channel messages and nothing else — clock, active sensing, a SysEx identity
reply — (`only_what_was_played_is_kept`, `crates/eui-client/src/midi_in.rs`),
and `Input::Midi` crosses the worker's pipe intact
(`requests_round_trip`, `worker.rs`).

The catalogue's `score()` (03 §4) is pinned by
`examples/demo-app/tests/score_spec.sl` (`soli test`): `\relative` octaves,
alterations and durations; every refusal named with its position; the
accidentals a key and a bar imply; the spacing's increment; stem
directions; beams grouped by metre, held to a space of slope and quanted;
LilyPond's slur height; every path a kind `1` or `5` the encoder takes
(`eui_render`); systems inside the width; a `\header` read and set as
LilyPond's title page sets it, two spaces clear of the margin above and
of the music below, `\tempo` marks over the staff, the
naturals a key change cancels with, and A4 pages that break and stay inside
their margins; the PDF's footer, the title at the left and the page out of
how many at the right, a title too long for it cut with an ellipsis clear of
the number; repeat signs and endings, a start repeat moved past a
line break, bars of rest counted in the bar numbers and gathered when
asked; tuplets' durations, their beams by the beat, their brackets and
refusals; `\break` and `\pageBreak` ending a line and a page; dynamics,
hairpins, articulations and words read onto their notes, dynamics clear of
the stems on one line where they touch, a staccato opposite its stem and in
a space; the cache recolouring a copy of a lit note's paths, and a lit
note laid over its band with the band's own paths unchanged; one band
canvas per system adding up to the page, an edited note changing one
band, and the line search not run again for the same widths; a MIDI file's
header, division and notes, a tie held as one note and dynamics as its
loudness; staves and their groups, names and instruments read, their notes
sharing columns, nine spaces apart or more, bar lines run through a
brace's staves, and one format-1 track per staff with its program; a PDF
with a page per A4 page, its cross-reference where `startxref` says and its
title in UTF-16; and the
score editor's model —
a note across a bar cut and tied, the keys its help lists, undo, slurs,
the source read back to the same model with its titles, tempo and changes,
a key or metre from the selected bar written where that bar starts (and
moved to the next bar line when the bar moves under it), a click resolved
to a note and a pitch on screen and on an A4 page, and in insert mode
(N) to the pitch pressed — added to a note as a chord, in another value
on a note of the first voice written into the second voice from that
note's start (the note kept), filling a rest, written over rests of another
value with a rest for what is left, or a new note between two — nothing
picked up and, but for the last, nothing moved, a repeat opened on the first bar written and drawn after the clef, repeats set and taken
off at the selected bar, bars of rest in a row written as one and read back
as bars, triplets and sextuplets made with Ctrl+3 and Ctrl+6 and broken by
a deletion, breaks set after the selected bar, a dynamic, a hairpin,
articulations and words written after their note and read back — a run
of typing words one step to undo, a mark or Clear a step of its own, on
the side chosen — the tempo mark shown as it prints, and the
tune mixed into a `.wav`; and its tracks — added and edited one at a time
with Alt+↑/↓ between them, written as a staff group with the shorter
filled, a change from a bar the first track's and every staff's — chosen
on any track, the full score's included — staves
read back as tracks with their names and MIDI settings, a click on the full
score editing that staff's note — the insert tool and the voices,
which only a single staff has, neither offered nor reachable by their keys
there — and each track's sounding note lit there while it plays; an undo step
that copies the track edited and not the others, undone and redone on that
track whichever is shown, and a track-level edit copying them all; and the
frame: the toolbar and the score each exactly the height under the top bar
that holds the titles, the score as wide as the window less the toolbar.

The native engraver lang calls in its place (`eui-score`) is held to that
one rather than to a second description:
`the_corpus_draws_as_the_soli_engraver_drew_it`
(`crates/eui-score/tests/golden.rs`) engraves each piece of
`tests/corpus/` on a 900 px screen and on A4 and requires every path to be
the Soli engraver's to the hundredth of a pixel — the screen's bands too, a
staff each in a system of several, their tops and heights to the pixel —
and its MIDI file and its
PDF to be the same bytes; `what_the_soli_engraver_refuses_this_refuses_in_the_same_words`
requires its refusals word for word, character position included. Two
voices are among the pieces (`voices.ly`, `voices_spacers.ly`,
`piano_voices.ly`, and `voices_crossed.ly`: two eighths of the second voice
a spacer apart, not beamed, and the voices crossing, the first a head to
the right), and among the refusals; so are lyrics (`lyrics.ly`,
`lyrics_choir.ly`, and six refusals: a missing or unclosed block, an empty
one, `--` with nothing before it, a command inside, a second verse), and
`score_spec.sl` holds a syllable a note with ties and slurs holding one,
the room notes make for long syllables, the line under the staff and
under low notes, a choir's staves each with its own, and the editor's
lyrics — typed for a track, written after its music in braces, read back,
counted against the notes that take one, one step to undo a run of typing; `score_spec.sl` holds their
stems, shared columns and rests, and the editor's second voice — written
beside the first, chosen with Ctrl+Alt+2, an insert-mode press writing at
the moment pressed with the silence around it kept — between two heads of
the first voice, that much of the time between them rounded to the value
chosen, so two eighths go under one quarter — and the pointer picking a
note of either voice and going to that voice; a source's second voice
read back as the second voice, written and read again the same; and every
example in the menu loading as what it is there to show.

02 §5.3's **gradients**. `a_gradient_background_carries_its_stops_in_the_quad`
pins the instance a gradient `bg` becomes — the first stop as the fill, the
middle and last in `from`, the positions and the angle code in `uv`, a
literal stop from the session's own table, and the radius and border kept —
and that a gradient reference resolves to no colour anywhere a colour is
asked for. In pixels: `a_gradient_runs_from_its_first_stop_to_its_last_in_srgb`
(to right, to bottom and 45° on a square: the ends are the stops, half way
is the mean of their **sRGB** values, and a line across the gradient is one
colour); `a_corner_gradient_follows_the_box_and_not_45_degrees` (to top right
on a 200 × 40 banner is half way along the diagonal that misses the corner,
which 45° is not); and `a_gradient_keeps_its_corners_its_border_and_its_opacity`
(the rounded corner is surface, the border draws over the gradient, and half
opacity is the picture a solid fill at half opacity draws). That a change to
or from a gradient does not ease its colours (03 §5) is
`a_change_to_or_from_a_gradient_does_not_ease_its_colours` in
`crates/eui-client/tests/driver.rs`.

03 §5's **pulse** and **bounce**, which are spin's bargain again:
`a_pulsing_or_bouncing_node_paints_the_same_list_whatever_the_clock` holds
that every quad painted for the node — its box and its glyphs — carries the
flag or its height, that the list asks for the next frame, and that the
factor and the lift are Tailwind's numbers at the phases that pin them (1 and
0.5 at 0 s and 1 s, a quarter and nothing at 0 s and 0.5 s). In pixels,
`pulse_dims_to_half_and_back_on_the_clock` draws one list at 0, 1 and 2 s —
full, the picture a solid half-opacity bar draws, full again — and
`bounce_lifts_a_quarter_of_the_height_and_lands_at_the_half` draws a 40 px
bar 10 px up at 0 s and on its laid-out place at 0.5 s, with the layout
unmoved.

Plus 03 §1's one word about a kind that the painter has to obey: a `sizer`
carrying a background, a border and a shadow draws **none** of them, and its
child still draws. The same tree with a `box` in that place draws more. The
word in the table is the whole specification of the kind, and it was the one
thing this file did not do.

03 §3's **`placeholder`**, which is painted and nothing else, so its vectors
sit a layer apart. In this file,
`a_placeholder_is_drawn_muted_only_while_the_field_is_empty`: an empty
`input`, an `input` with no text at all and an empty `textarea` draw the
hint's glyphs in `text.muted`; a value hides it and draws in the node's own
colour; an empty field with no width of its own is measured as its hint and a
filled one by its value; a `secret` field draws the hint as text, not marks;
a `text` node carrying the prop draws nothing of it. And
`a_focused_empty_field_puts_its_caret_where_it_would_without_a_hint`: the
caret of a centred empty field stands in the middle whether a hint is shown
or not, in the node's colour rather than the hint's. What the prop is to the
tree is `a_placeholder_is_declared_by_fields_and_shown_only_while_empty` in
`crates/eui-tree/tests/apply.rs` — shown only while the text is empty, hidden
by a `SetText`, back when the field is emptied, declared by neither a `text`
node nor a value that is not a string. And that it is never the value is
`a_placeholder_is_never_the_value_and_is_announced_as_a_placeholder` in
`crates/eui-client/tests/driver.rs`: typing starts from nothing, emptying the
field brings the hint back, nothing the field sends names it, and an
assistive technology is handed it as the placeholder with an empty value.

**A picture of a restyle is a picture of where it lands.** Not a vector of
the protocol — the harness is `examples/snapshot`, and its tests are in
`src/main.rs` — but the reason a conformant server and client were once
recorded as losing every style-only change.
`a_restyle_with_a_transition_is_mid_flight_on_the_first_paint` pins what a
list at age 0 is: the restyled box a moving quad, still the colour it is
leaving. `the_picture_is_taken_after_the_transition_lands` pins that the tool
runs the clock until nothing is easing before it draws, so the box is in its
new colour and no quad is animated.

## 9. Diff — `lang/src/serve/eui/diff.rs` (feature `eui`)

Keyed rows move rather than rebuild; mixed keyed and unkeyed children match
by position and key; a changed cell is one `SetText`. And a row whose only
change is its style is one `SetStyle`, keyed or not, through the path a
Soli view's values take — `a_style_only_change_is_one_set_style_per_row_keyed_or_not`
in `lang/src/serve/eui/tree.rs`.

The same crate's encoder speaks the version a session negotiated (05 §2):
`tree.rs`'s `a_space_step_an_older_client_lacks_falls_back` renders one
style at 5, at 6 and with no handshake, and checks each `DefStyle`; and
`snapshot.rs`'s `a_one_shot_render_is_sent_the_space_steps_its_version_has`
does it through `render_once`, the path `eui_render` and `GET /_eui/view?v=`
share, decoding the body it answers. The rest of version 6 is held the same
two ways: `a_gradient_background_is_defined_once_per_session` (one
`DefGradient` for one gradient however often it is written, after the colour
its literal stop names and before the style that names it; evenly spread
positions; the corner codes; and each malformed `bg` refused with its reason),
`an_older_session_is_sent_a_gradients_first_stop_and_no_pulse_or_bounce`
(at 5: no `DefGradient`, the first stop as a solid `bg`, the `animation`
bits a version-5 client takes), and
`a_one_shot_render_below_6_carries_no_gradient_and_no_pulse` through
`render_once`.

## 10. End to end — `crates/eui-client/tests/soli_e2e.rs`

With `EUI_SOLI_BIN` set, the client drives a real Soli server: the counter's
local-first `+`, the todo's keyed rows and fetched avatar, ten thousand rows
sorted by moves, and the gallery's select, slider, pickers, data grid and
charts. The tag field is there too, because it is where §3.1's three tiers
meet: `Enter` arrives as a `submit` and the field empties itself, and the
same `Backspace` deletes a character on one press and a whole tag on the next.

And 03 §5.3's shared element, which needs a server to be worth anything: the
gallery's customers section is a stack narrow, the account's disc is the one
node on both of its pages, and pushing into a row gives that disc a transform
carrying its partner's width. Nothing else produces a scale, and the disc is
not the node the change named — the server swapped the page.

## 11. Scenes — `crates/eui-shader/tests`, `crates/eui-client/tests/mesh.rs`, `crates/eui-render/tests/render.rs`

**A scene's pixels are not a conformance surface**, and this section is short
because of it. Two conforming clients may draw the same scene differently:
`sin`, the filtering, the rasteriser's fill rule and the point of sRGB
conversion all belong to the adapter. A server that depends on a scene's exact
pixels depends on something this protocol does not promise. Saying that
plainly is better than widening §8 and hoping.

What is pinned instead:

1. **The shader verifier's verdicts**, one vector per rule of 11 §2 — an
   unbounded loop, a bound read from a uniform, a step of zero, a counter the
   body also moves, nested loops over the step budget, a compute stage, a
   task or mesh stage and a ray-tracing stage with their extensions enabled
   (`a_mesh_or_task_stage_is_refused_even_with_its_extension_enabled`), a
   storage buffer, a texture, a barrier, a binding outside group 0, a uniform
   block of the server's own shape, an entry point by another name, a fragment
   result that is not `@location(0) vec4<f32>`, a module over 64 KiB, and
   expression nesting past 32 — three thousand `(`, a run of `-`, `array<`
   inside `array<`, and the fuzzer's own find, each refused on a 256 KiB
   stack, while 32 levels of each inside 63 braces still reach the parser
   (`deep_nesting_is_refused_before_the_parser_meets_it`). These
   decide on a machine with no GPU, which is what makes them vectors and not
   hopes; a fuzz target sits beside them (08 §5).
2. **The mesh decoder's**, likewise: a container that is not one, an undefined
   flag, a count over the cap, indices that are not whole triangles, a body
   that is not the length the header claims, a coordinate that is not finite,
   and **an index past the end of the vertices** — the bound no driver checks.
3. **The structure of the frame.** A `scene` node produces exactly one
   textured quad, at its box, in a run that names its target; a scene that is
   not `playing` adds no pass to the frame after the one that drew it; a frame
   whose only change is the clock uploads no instances; the target carries no
   `COPY_SRC`; a session that was not granted `scene` fetches no module and
   builds no scene for a node that names one — and **does** draw one that
   names none, whose mesh it fetches either way; and past 10 §1's 32
   compiled modules the one drawn longest ago is dropped and, named again,
   compiled afresh from its kept source and drawn
   (`a_module_the_server_sent_is_what_draws`).
4. **Three tolerant pixel vectors, and three only**: a flat triangle from the
   client's own shader, checked to ±2/255; depth order, which is a boolean and
   so insensitive to precision; and the fallback a refused module draws. A
   fourth would be a promise this protocol does not make.

## 12. Catalogue styles — `examples/demo-app/tests/tw_spec.sl` (`soli test`)

03 §4's translation rule, for the reference catalogue's `tw()`. Every class it
accepts — one of each of `tw_examples()` — is sent through `eui_render`, the
encoder's own path, and must come back a frame rather than a refusal; so
nothing `tw()` emits is a key or value the wire does not have. The refusals
are pinned by name: a letter-spacing, a line-height, a per-corner
radius, a transform, a breakpoint and an off-palette hue each raise with the
class in the message rather than vanish, and the hue's message names the
nearest role. So is every gradient that is not one — a direction with no
stops, a stop with no direction, a gradient with no `to-*` (a role has no
transparent version to fade to), stops that go backwards — while
`bg-gradient-to-*` with `from-*`, `via-*`, `to-*` and their `-N%` positions
compose into one `bg` in any order, a state's stop lies over the resting
gradient, and `animate-pulse` and `animate-bounce` join `animate-spin` as
bits that combine (02 §5.3, 03 §5). The four state prefixes land as local styles on the node, so a
`hover:` needs no round trip.
`py-1.5`, `px-2.5`, `gap-3.5`, `mt-20`, `p-32` and `space-y-1.5` land on
`space` 13–17 (05 §2), and a step the scale still lacks (`p-7`, `p-40`) is
refused naming the two nearest.
What a node says about its place is settled on the server too: a child's
`ml-auto` and its kin become a spacer along its parent's line or a `self`
across it, in its hover styles as well; `flex-row-reverse` reverses the
children and turns `justify`; a side's border colour is the box's one
`border_color` only when every drawn side agrees, and raises naming both
colours otherwise; `top-*`, `bottom-*`, `left-*`, `right-*` and `inset-*`
on an absolute node are `self`, a `position` that names its edge (04 §5,
version 7) and a margin.

## 13. Scores — `crates/eui-client/tests/score.rs`, `crates/eui-proto/tests/frames.rs`, lang `src/serve/eui/score.rs`

03 §1.4 and 06 §1.5. The client engraves; the server sends the source.

- **The kind and the events are protocol 9.** `0x12` `score` round-trips
  through `NodeKind`, and `0x22` `score_pick` and `0x23` `score_info`
  through `EventKind`, with payloads of the declared shape and no other
  (`the_score_kind_and_its_events_round_trip`).
- **The client draws what the catalogue draws.** A `score` on the corpus
  pieces, at a width and a staff space, is painted from exactly the paths
  `eui_score::paths` gives for that source, width and page — the
  catalogue's own (`a_score_is_drawn_from_the_catalogues_paths`).
- **Its height is the page it draws**, at the width it is given, on a
  screen and on A4 (`a_scores_height_is_the_page_it_draws`).
- **Only what moves the music engraves it again.** A new `marks`, `lit`,
  `page_index` or `colour` redraws and engraves nothing; a new `src`,
  width, `staff_space`, `page` or `page_height` engraves once
  (`only_what_moves_the_music_engraves_it_again`).
- **A refusal draws nothing and says why**, in `score_info`, with the
  position; a source past 512 KiB is refused unread
  (`a_refused_score_draws_nothing_and_says_why`).
- **`score_info` is sent once an engraving**, with the page count
  (`score_info_is_sent_once_an_engraving`).
- **`score_pick` names the music under the pointer**: on a press over a
  note, that note's item as `near` with a `gap` under half a space, its
  staff and line as `staff` and `pos`;
  between two notes, `before`, `after` and `frac`; nothing at all from a
  score that refused its source (`score_pick_names_the_music_under_the_pointer`).
- **A server sends the source to a session of 9 and the paths to an older
  one**: the catalogue's `score()` under lang, rendered for protocol 8 and
  9 (`a_score_is_sent_as_source_from_protocol_9_and_as_paths_before`).
- **A score edits its own pitches from protocol 10.** `0x24` `score_edit`
  round-trips through `EventKind` with entries of five integers and no
  other shape (`the_score_edit_event_round_trips`).
- **A drag on the selected note moves it in the source and reports once.**
  A press on the selected item moves nothing; a move two lines up from
  where it pressed — a line above the head — rewrites that head's
  pitch in `src` and nothing else, engrave it, send no `score_pick`, no
  pointer event and no `score_info`, and after `edit_after` one `score_edit` with the head's
  last pitch (`a_selected_note_dragged_moves_in_the_source_and_reports_once`).
  A press on another item is reported as before
  (`a_press_off_the_selected_note_is_the_servers`).
- **The arrows move it in the key.** ↑ from E in G major is F sharp, Shift+↑
  from A in F major is B flat, Ctrl+↑ is the octave, a chord moves all its
  heads or none at the edge of the reach, and none of these keys is
  reported (`arrows_move_the_selected_note_in_the_key`).
- **Edits are reported before anything else.** An edit followed by a click
  elsewhere sends `score_edit` first, then the click; blur sends it; a head
  moved and moved back sends nothing
  (`edits_are_reported_before_any_other_event`).
- **What cannot be edited is not.** A source using `\relative`, a refused
  source, a rest and a node without `edit` report the press and the keys as
  in protocol 9 and change nothing (`a_score_it_cannot_edit_is_left_alone`).
- **A new `src` from the server wins**, and edits it overtook are reported
  at once (`the_servers_source_replaces_unreported_edits`).
- **A server applies what it can.** lang applies a `score_edit` entry by
  entry, ignoring one with an item, head or pitch out of range
  (`a_score_edit_is_applied_entry_by_entry`).
