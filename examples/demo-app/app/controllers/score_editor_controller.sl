# A score editor on `score()`: type notes, see them engraved as you go.
#
# The model is a list of notes and a selection, nothing more. Each event
# edits the list, the list is written out as LilyPond, and the catalogue's
# engraver draws it — so what the editor shows is exactly what its source
# says, and the source can be read, copied or pasted back beside it.
#
#   A–G          add a note after the selection (Shift adds it to the chord)
#   R or 0       add a rest
#   2…7          32nd … whole, as MuseScore numbers them
#   .            dotted          + / -  sharpen, flatten    =  natural
#
# Like MuseScore, it has two moods and never says so. Straight after a note
# is written, a duration or a dot is for the notes still to come — that is
# how a tune is typed. Once a note has been chosen, by a click or an arrow,
# they change that note.
#   ↑ / ↓        a step up or down the key's scale; with Shift, a
#                semitone, through the sharps and flats; with Ctrl, an octave
#   ← / →        select the previous or next note
#   T            tie to the next note
#   Ctrl+3 / 6   the note becomes a triplet / a sextuplet of shorter notes
#   S            start a slur here, then S on a later note ends it
#   [ / ]        a repeat starts at the selected bar / ends after it
#   Space        play from the start, or stop
#   Backspace    a note becomes a rest; Ctrl+Z / Ctrl+Y  undo, redo
#
# With the mouse: a click on a note selects it, a second click on it moves
# it to the line or space under the pointer, dragging it up or down moves it
# line by line — redrawn and heard at each one — and a click past the last
# note writes a new one there. The palette above the staff does everything the
# keys do, for whoever does not know them.
#
# A note may be written across a bar line: it is cut at the bar and tied,
# as an engraver would, so the person thinks in durations and not in bars.
# Bars of rest in a row are gathered into one, with their count over it,
# as LilyPond's \compressEmptyMeasures does in a part.
#
# It sounds, too, and the window is the synthesiser (03 §7, protocol 9):
# Play sends it the tune as a MIDI file — the one "Download MIDI" saves, a
# few hundred bytes — and each note written goes as MIDI commands, played
# at once, with the SoundFont the node names (GeneralUser GS, fetched by
# tools/fetch_soundfont.sh) or the client's own voices. The server mixes
# nothing. The playhead comes back as `time_update` and lights the note it
# is on, and a wrong note is heard before it is seen.

# The settings, as the person sees them.
const SCORE_ED_CLEFS = ["Treble", "Bass", "Alto"]
const SCORE_ED_TIMES = ["2/2", "2/4", "3/4", "4/4", "5/4", "6/4", "3/8", "6/8", "9/8", "12/8"]
const SCORE_ED_SCOPES = ["Whole piece", "From the selected bar"]
# What a press on the staff does: pick a note up to move it, or write a new
# one where it lands (`score_ed_insert_at`). N switches, Escape goes back.
const SCORE_ED_TOOLS = ["Pointer", "Insert"]
# Which voice the keys and clicks write in (Ctrl+Alt+1, Ctrl+Alt+2).
const SCORE_ED_VOICES = ["Voice 1", "Voice 2"]
const SCORE_ED_VIEWS = ["Screen", "A4 page"]
# The height of a row of track chips, gap included, in px.
const SCORE_ED_CHIP_ROW = 28
# What the Examples menu loads.
const SCORE_ED_EXAMPLES = ["Frère Jacques", "Beethoven, Symphony No. 5 (exposition)"]
# The tabs under the palette.
const SCORE_ED_PANELS = ["Score", "Bars", "Expression", "Lyrics", "Tracks"]
# Where a track sits, left to right, and its MIDI pan for each.
const SCORE_ED_PANS = ["Left", "Half left", "Centre", "Half right", "Right"]
# The dynamics and the articulations the Expression tab offers.
const SCORE_ED_DYNAMICS = ["ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "sfz", "fp"]
const SCORE_ED_ARTICS = ["staccato", "accent", "tenuto", "marcato", "staccatissimo", "portato", "fermata"]
# The title-page fields the editor asks for, in LilyPond's names.
const SCORE_ED_HEADER = ["title", "subtitle", "composer", "arranger"]
# Major keys by the number of sharps (positive) or flats (negative).
const SCORE_ED_KEYS = ["C♭", "G♭", "D♭", "A♭", "E♭", "B♭", "F", "C", "G", "D", "A", "E", "B", "F♯", "C♯"]
const SCORE_ED_KEY_LY = ["ces", "ges", "des", "as", "es", "bes", "f", "c", "g", "d", "a", "e", "b", "fis", "cis"]
const SCORE_ED_UNDO = 60
# Beats a minute the tempo field takes.
const SCORE_ED_BPM = [30, 240]

# ---- The handler -----------------------------------------------------------

def score_editor(event_data)
  ed_event = event_data["event"]
  params = event_data["params"] ?? {}
  props = params["props"] ?? {}
  state = score_ed_defaults(event_data["state"] ?? {})
  state["viewport"] = params["viewport"] if params["viewport"]
  ed_answer = score_ed_answer(state, ed_event, params, props)
  # A file answered is sent as it is; a state shows the page its selection,
  # or the note playing, is on.
  return ed_answer if ed_answer.has_key("save")

  score_ed_follow(ed_answer)
end

def score_ed_answer(state, ed_event, params, props)
  match ed_event {
    "track" => score_ed_switch(score_ed_flag(state, "full_view", false), int(props["track"] ?? 0)),
    "full_view" => score_ed_full_on(state),
    "track_add" => score_ed_track_add(state),
    "track_remove" => score_ed_track_remove(state),
    "track_up" => score_ed_track_move(state, -1),
    "track_down" => score_ed_track_move(state, 1),
    "track_name" => score_ed_track_set(state, "name", params["payload"] ?? ""),
    "track_short" => score_ed_track_set(state, "short", params["payload"] ?? ""),
    "track_instrument" => score_ed_track_set(score_ed_flag(state, "inst_open", false), "instrument", props["value"]),
    "inst_open" => score_ed_flag(state, "inst_open", !(state["inst_open"] == true)),
    "track_volume" => score_ed_track_set(state, "volume", score_max(0, score_min(127, score_ed_int(params["payload"] ?? "")))),
    "track_pan" => SCORE_ED_PANS.includes?(props["option"]) ? score_ed_track_set(state, "pan", [-100, -50, 0, 50, 100][SCORE_ED_PANS.index_of(props["option"])]) : state,
    "track_mute" => score_ed_track_set(state, "mute", !(score_ed_tracks(state)[state["track"] ?? 0]["mute"] == true)),
    "group" => SCORE_ED_GROUPS.includes?(props["option"]) ? score_ed_flag(state, "group", props["option"]) : state,
    "conductor_pick" => score_ed_conductor_pick(state, params["payload"] ?? [0, 0]),
    "connect" => state,
    "viewport" => state,
    "key" => score_ed_key(state, params["payload"][0], params["payload"][1]),
    "midi" => score_ed_midi_in(state, params["payload"] ?? []),
    "pick" => score_ed_pick(state, params["payload"] ?? [0, 0], int(props["page"] ?? 0)),
    "press" => score_ed_press(state, params["payload"] ?? [0, 0], int(props["page"] ?? 0)),
    "drag" => score_ed_drag(state, params["payload"] ?? [0, 0]),
    "release" => score_ed_flag(state, "dragging", false),
    "dur" => score_ed_duration(state, int(props["base"])),
    "dot" => score_ed_dot(state),
    "rest" => score_ed_insert_rest(state),
    "note" => score_ed_insert(state, int(props["step"]), false),
    "acc" => score_ed_alter(state, int(props["by"]), false),
    "natural" => score_ed_alter(state, 0, true),
    "step" => score_ed_move_pitch(state, int(props["by"])),
    "semitone" => score_ed_semitone(state, int(props["by"])),
    "tie" => score_ed_tie(state),
    "delete" => score_ed_delete(state, true),
    "undo" => score_ed_undo(state),
    "redo" => score_ed_redo(state),
    "clef" => score_ed_settings(state, "clef", props["option"]),
    "time" => score_ed_settings(state, "time", props["option"]),
    "time_open" => score_ed_flag(state, "time_open", !(state["time_open"] == true)),
    "time_pick" => score_ed_settings(score_ed_flag(state, "time_open", false), "time", props["value"]),
    "scope" => SCORE_ED_SCOPES.includes?(props["option"]) ? score_ed_flag(state, "scope", props["option"]) : state,
    "tool" => SCORE_ED_TOOLS.includes?(props["option"]) && !score_ed_full_view(state) ? score_ed_flag(state, "tool", props["option"]) : state,
    "voice" => SCORE_ED_VOICES.includes?(props["option"]) && !score_ed_full_view(state) ? score_ed_set_voice(state, SCORE_ED_VOICES.index_of(props["option"]) + 1) : state,
    "view" => SCORE_ED_VIEWS.includes?(props["option"]) ? score_ed_flag(state, "view", props["option"]) : state,
    "panel" => SCORE_ED_PANELS.includes?(props["tab"]) ? score_ed_flag(state, "panel", (state["panel"] ?? SCORE_ED_PANELS[0]) == props["tab"] ? "none" : props["tab"]) : state,
    "bpm" => score_ed_tempo(state, score_ed_int(params["payload"] ?? ""), state["tempo_text"], state["mark_tempo"]),
    "tempo_text" => score_ed_tempo(state, state["tempo"], params["payload"] ?? "", state["mark_tempo"]),
    "mark_tempo" => score_ed_tempo(state, state["tempo"], state["tempo_text"], !(state["mark_tempo"] == true)),
    "hdr_title" => score_ed_header_set(state, "title", params["payload"] ?? ""),
    "hdr_subtitle" => score_ed_header_set(state, "subtitle", params["payload"] ?? ""),
    "hdr_composer" => score_ed_header_set(state, "composer", params["payload"] ?? ""),
    "hdr_arranger" => score_ed_header_set(state, "arranger", params["payload"] ?? ""),
    "key_open" => score_ed_flag(state, "key_open", !(state["key_open"] == true)),
    "key_pick" => score_ed_settings(score_ed_flag(state, "key_open", false), "key", (props["value"] ?? "").replace(" major", "")),
    "src" => score_ed_from_source(state, params["payload"] ?? ""),
    "clear" => score_ed_clear(state),
    "example" => score_ed_example(state, props["value"] ?? SCORE_ED_EXAMPLES[0]),
    "ex_open" => score_ed_flag(state, "ex_open", !(state["ex_open"] == true)),
    "help" => score_ed_flag(state, "help", !(state["help"] == true)),
    "slur" => score_ed_slur(state),
    "tuplet" => score_ed_tuplet(state, int(props["n"] ?? 3)),
    "repeat_start" => score_ed_repeat(state, "rep_start"),
    "repeat_end" => score_ed_repeat(state, "rep_end"),
    "line_break" => score_ed_break(state, "line"),
    "dyn" => score_ed_dynamic(state, props["dyn"]),
    "hairpin" => score_ed_hairpin(state, props["grow"]),
    "artic" => score_ed_artic(state, props["name"]),
    "midi_save" => {"state": state, "save": score_midi(score_ed_full_source(state), {"bpm": state["tempo"], "beat": state["tempo_base"] ?? 4})},
    "pdf_save" => {"state": state, "save": score_pdf(score_ed_full_source(state))},
    "words" => score_ed_words(state, params["payload"] ?? "", props["dir"] == "down" ? 1 : -1),
    "words_clear" => score_ed_words_pressed(state, ""),
    "words_mark" => score_ed_words_pressed(state, props["text"] ?? ""),
    "lyrics" => score_ed_lyrics_set(state, params["payload"] ?? ""),
    "words_dir" => ["Above", "Below"].includes?(props["option"]) ? score_ed_flag(state, "words_dir", props["option"]) : state,
    "page_break" => score_ed_break(state, "page"),
    "play" => score_ed_play(state),
    "playhead" => score_ed_playhead(state, params["payload"] ?? [0, 0]),
    "play_tick" => score_ed_play_tick(state),
    "src_open" => score_ed_flag(state, "src_open", !(state["src_open"] == true)),
    "page_to" => score_ed_flag(state, props["which"] == "cond_page" ? "cond_page" : "page", int(props["to"] ?? 0)),
    "ended" => score_ed_stop(state),
    "sound" => score_ed_flag(state, "sound", !(state["sound"] == true)),
    _ => state
  }
end

# The digits of what was typed, as a number; 0 when there are none.
def score_ed_int(typed)
  digits = typed.chars().filter { |ch| "0123456789".contains(ch) }.join("")
  digits == "" ? 0 : int(digits)
end

def score_ed_flag(state, key, value)
  state[key] = value
  state
end

def score_ed_defaults(state)
  return state if state.has_key("notes")

  state["clef"] = "Treble"
  state["key"] = "C"
  state["time"] = "4/4"
  state["dur"] = 4
  state["dot"] = false
  state["undo"] = []
  state["redo"] = []
  state["error"] = ""
  state["help"] = false
  state["key_open"] = false
  state["tempo"] = 100
  state["tempo_base"] = 4
  state["tempo_text"] = ""
  state["mark_tempo"] = true
  state["header"] = {}
  state["scope"] = SCORE_ED_SCOPES[0]
  state["view"] = SCORE_ED_VIEWS[0]
  state["time_open"] = false
  state["sound"] = true
  state["playing"] = false
  state["play_at"] = -1
  state["blips"] = 0
  score_ed_frere_jacques(state)
end

# The first thing anyone sees: a tune they can read, to edit rather than a
# blank staff to puzzle over.
def score_ed_frere_jacques(state)
  tune = [[0, 4], [1, 4], [2, 4], [0, 4], [0, 4], [1, 4], [2, 4], [0, 4], [2, 4], [3, 4], [4, 2], [2, 4], [3, 4], [4, 2]]
  state["notes"] = tune.map { |nt| score_ed_note(nt[0], 4, 0, nt[1], 0) }
  state["header"] = {"title": "Frère Jacques", "composer": "Traditional"}
  state["tempo_text"] = "Moderato"
  state["sel"] = 0
  state
end

def score_ed_note(step, octave, alter, base, dots)
  {"rest": false, "pitches": [ {"step": step, "oct": octave, "alter": alter} ], "base": base, "dots": dots, "tie": false}
end

# ---- Edits -----------------------------------------------------------------

# Every edit goes through here: what it is about to change is copied onto
# the undo stack first, and the redo stack is dropped, as every editor does.
# Only the track being edited is copied — its voices, clef and the
# settings — and which track it is: copying every track made each step of
# an orchestra's undo 390 KB, and the state, read and written whole at every
# event, grew by that much with each note moved.
def score_ed_remember(state)
  score_ed_push_undo(state, score_ed_snap(state, false))
end

# For an edit that changes other tracks than the one edited — a track
# added, removed or moved, a change a bar carries on the first track, a
# whole piece read — every track is copied.
def score_ed_remember_all(state)
  score_ed_push_undo(state, score_ed_snap(state, true))
end

def score_ed_snap(state, all)
  snap = {"notes": state["notes"], "notes2": state["notes2"] ?? [], "voice": state["voice"] ?? 1, "clef": state["clef"], "key": state["key"], "time": state["time"], "sel": state["sel"], "track": state["track"] ?? 0}
  # The edited track's lyrics are its settings' only words an edit changes.
  snap["lyrics"] = score_ed_tracks(state)[state["track"] ?? 0]["lyrics"] ?? ""
  snap["tracks"] = score_ed_tracks(state) if all
  score_ed_copy(snap)
end

def score_ed_push_undo(state, snap)
  # Any other edit ends a run of typing words.
  state["words_at"] = ""
  undo = state["undo"] ?? []
  undo.push(snap)
  if undo.length > SCORE_ED_UNDO
    undo = range(undo.length - SCORE_ED_UNDO, undo.length).map { |k| undo[k] }
  end
  state["undo"] = undo
  state["redo"] = []
  state["error"] = ""
  state
end

# The state is plain data and the stacks hold copies of it: an edit that
# changed a note in place would otherwise change it in the past as well.
def score_ed_copy(v)
  return v.map { |x| score_ed_copy(x) } if v.is_a?("array")
  if v.is_a?("hash")
    out = {}
    for k in v.keys
      out[k] = score_ed_copy(v[k])
    end
    return out
  end

  v
end

def score_ed_restore(state, snap)
  state["tracks"] = snap["tracks"] if snap.has_key("tracks")
  state["track"] = snap["track"] if snap.has_key("track")
  score_ed_tracks(state)[state["track"] ?? 0]["lyrics"] = snap["lyrics"] if snap.has_key("lyrics")
  state["notes"] = snap["notes"]
  state["notes2"] = snap["notes2"] if snap.has_key("notes2")
  state["voice"] = snap["voice"] if snap.has_key("voice")
  state["clef"] = snap["clef"]
  state["key"] = snap["key"]
  state["time"] = snap["time"]
  state["sel"] = snap["sel"]
  state
end

# A step that copied one track is undone on that track: the editor goes
# back to it first, so what was there is put back where it was, and the
# step to redo is that track as it is now.
def score_ed_undo(state)
  undo = state["undo"] ?? []
  return state if undo.length == 0

  snap = undo.pop()
  state["words_at"] = ""
  state = score_ed_onto_track(state, snap)
  redo = state["redo"] ?? []
  redo.push(score_ed_snap(state, snap.has_key("tracks")))
  state["undo"] = undo
  state["redo"] = redo
  score_ed_restore(state, snap)
end

def score_ed_onto_track(state, snap)
  return state if snap.has_key("tracks") || !snap.has_key("track") || snap["track"] == (state["track"] ?? 0)

  score_ed_switch(state, snap["track"])
end

def score_ed_redo(state)
  redo = state["redo"] ?? []
  return state if redo.length == 0

  snap = redo.pop()
  state["words_at"] = ""
  state = score_ed_onto_track(state, snap)
  undo = state["undo"] ?? []
  undo.push(score_ed_snap(state, snap.has_key("tracks")))
  state["undo"] = undo
  state["redo"] = redo
  score_ed_restore(state, snap)
end

def score_ed_selected(state)
  sel = state["sel"] ?? -1
  return nil if sel < 0 || sel >= state["notes"].length

  state["notes"][sel]
end

# The alteration the key gives a step: typing F in G major writes F sharp,
# because that is the F the person means.
def score_ed_key_alter(state, step)
  score_key_alters(score_ed_fifths_of(score_ed_key_at(state, state["sel"] ?? -1)))[step]
end

def score_ed_fifths(state)
  score_ed_fifths_of(state["key"])
end

def score_ed_fifths_of(key)
  at = SCORE_ED_KEYS.index_of(key)
  at < 0 ? 0 : at - 7
end

# The key in force at the note `idx` (the last note when nothing is
# chosen): the piece's, or the last change written at or before it.
def score_ed_key_at(state, idx)
  last = idx < 0 ? state["notes"].length - 1 : idx
  key = state["key"]
  for nt, k in state["notes"]
    break if k > last

    changes = nt["set"] ?? {}
    key = changes["key"] if changes.has_key("key")
  end
  key
end

# How far a note may go: five ledger lines off its staff, which is where a
# part is written an octave away (8va) instead, and within a piano's 88
# keys. In staff positions from the middle line, the staff being ±4.
def score_ed_reach()
  14
end

def score_ed_pos_of(state, pt)
  (pt["oct"] - 3) * 7 + pt["step"] - score_clef_mid(score_ed_clef(state))
end

def score_ed_fits(state, pt)
  midi = 12 * (pt["oct"] + 1) + [0, 2, 4, 5, 7, 9, 11][pt["step"]] + pt["alter"]
  score_ed_pos_of(state, pt).abs <= score_ed_reach() && midi >= 21 && midi <= 108
end

# A pitch brought back within reach by whole octaves.
def score_ed_within(state, pt)
  out = pt.merge({})
  while score_ed_pos_of(state, out) > score_ed_reach()
    out["oct"] = out["oct"] - 1
  end
  while score_ed_pos_of(state, out) < 0 - score_ed_reach()
    out["oct"] = out["oct"] + 1
  end
  out
end

# The octave a new note takes: the one that puts it nearest the note before
# it, as LilyPond's `\relative` does; a first note sits in the staff.
def score_ed_octave_near(state, step)
  prev = nil
  sel = state["sel"] ?? -1
  idx = sel >= 0 ? sel : state["notes"].length - 1
  while idx >= 0 && prev.nil?
    cand = state["notes"][idx]
    prev = cand["pitches"][0] unless cand["rest"]
    idx -= 1
  end
  if prev.nil?
    return {"Treble": 4, "Bass": 3, "Alto": 4}[state["clef"]] ?? 4
  end
  ref = prev["oct"] * 7 + prev["step"]
  best = prev["oct"]
  best_gap = 99
  for octave in [prev["oct"] - 1, prev["oct"], prev["oct"] + 1]
    gap = (octave * 7 + step - ref).abs
    if gap < best_gap
      best_gap = gap
      best = octave
    end
  end
  best
end

def score_ed_insert(state, step, chord)
  state = score_ed_remember(state)
  current = score_ed_selected(state)
  if chord && !current.nil? && !current["rest"]
    octave = current["pitches"][-1]["oct"]
    octave += 1 if step <= current["pitches"][-1]["step"]
    current["pitches"].push(score_ed_within(state, {"step": step, "oct": octave, "alter": score_ed_key_alter(state, step)}))
    current["pitches"] = current["pitches"].sort_by { |pt| pt["oct"] * 7 + pt["step"] }
    return state
  end
  octave = score_ed_within(state, {"step": step, "oct": score_ed_octave_near(state, step), "alter": 0})["oct"]
  fresh = score_ed_note(step, octave, score_ed_key_alter(state, step), state["dur"], state["dot"] ? 1 : 0)
  score_ed_place(state, fresh)
end

def score_ed_insert_rest(state)
  state = score_ed_remember(state)
  fresh = {"rest": true, "pitches": [], "base": state["dur"], "dots": state["dot"] ? 1 : 0, "tie": false}
  score_ed_place(state, fresh)
end

# A new note goes after the selection — at the end when nothing is
# selected — and becomes the selection, so typing a tune is typing letters.
def score_ed_place(state, fresh)
  notes = state["notes"]
  sel = state["sel"] ?? -1
  at = sel >= 0 ? sel + 1 : notes.length
  at = notes.length if at > notes.length
  before = notes.take(at)
  after = range(at, notes.length).map { |k| notes[k] }
  state["notes"] = before + [fresh] + after
  state["sel"] = at
  state["typing"] = true
  score_ed_blip(state, fresh)
end

# Deleting a note leaves a rest of its length, as an engraver would: the
# bar keeps its count and what follows stays where it was. Deleting a rest
# takes it out. A bar at the very end left with nothing but such rests goes
# too (`score_ed_trim`).
def score_ed_delete(state, back)
  current = score_ed_selected(state)
  return state if current.nil?

  state = score_ed_remember(state)
  notes = state["notes"]
  sel = state["sel"]
  if current["rest"]
    score_ed_untuplet(state, notes[sel])
    state["notes"] = range(0, notes.length).filter { |k| k != sel }.map { |k| notes[k] }
  else
    score_ed_unslur(notes, sel)
    notes[sel - 1]["tie"] = false if sel > 0
    notes[sel] = score_ed_silenced(notes[sel])
  end
  state = score_ed_trim(state)
  left = state["notes"].length
  next_sel = back ? sel - 1 : sel
  next_sel = left - 1 if next_sel >= left
  next_sel = 0 if next_sel < 0 && left > 0
  next_sel = -1 if left == 0
  state["sel"] = next_sel
  state
end

# A note as the rest it leaves: its length, its place in a tuplet and what
# its bar starts with, and a mark saying no one wrote it.
def score_ed_silenced(nt)
  out = {"rest": true, "auto": true, "pitches": [], "base": nt["base"], "dots": nt["dots"], "tie": false}
  for field in ["tup", "set", "rep_start", "rep_end", "brk", "hairpin", "hairpin_end"]
    out[field] = nt[field] unless nt[field].nil?
  end
  out
end

# A slur that starts or ends on note `at` goes with it, both ends.
def score_ed_unslur(notes, at)
  if notes[at]["slur_on"] == true
    done = false
    for k in range(at + 1, notes.length)
      if !done && notes[k]["slur_off"] == true
        notes[k]["slur_off"] = false
        done = true
      end
    end
  end
  if notes[at]["slur_off"] == true
    done = false
    for k in range(0, at).reverse()
      if !done && notes[k]["slur_on"] == true
        notes[k]["slur_on"] = false
        done = true
      end
    end
  end
  notes
end

# What is left after the last note is not part of the piece: a last bar
# holding nothing but rests a deletion left goes, and the one before it if
# it is the same. A rest written by hand (R, 0, a bar of rest) is music,
# and stays.
def score_ed_trim(state)
  while state["notes"].length > 0
    notes = state["notes"]
    start = score_ed_bar_start(state, notes.length - 1)
    tail = range(start, notes.length).map { |k| notes[k] }
    break unless tail.all? { |nt| nt["auto"] == true }

    state["notes"] = notes.take(start)
  end
  state
end

def score_ed_duration(state, base)
  return state unless [1, 2, 4, 8, 16, 32].includes?(base)

  state["dur"] = base
  state["dot"] = false if base == 32
  current = score_ed_selected(state)
  # In insert mode a value is the next note's, never the selected one's:
  # the selection is only where the last press wrote.
  return state if current.nil? || state["typing"] == true || score_ed_inserting(state)

  state = score_ed_remember(state)
  current = score_ed_selected(state)
  score_ed_untuplet(state, current)
  current["base"] = base
  current["dots"] = 0 if base == 32
  state
end

# A note of a tuplet whose length changes, or that goes, breaks it: its
# notes become plain notes again, rather than a group that no longer adds up.
def score_ed_untuplet(state, nt)
  tup = nt["tup"]
  return nil if tup.nil?

  for other in state["notes"]
    other["tup"] = nil if !other["tup"].nil? && other["tup"]["g"] == tup["g"]
  end
end

# Ctrl+3 and Ctrl+6, as in MuseScore: the selected note becomes three notes
# of the next shorter value in its time (a triplet), or six of the value
# after that (a sextuplet), each at its pitch, the first selected so the
# arrows and a second click set them one by one.
def score_ed_tuplet(state, n)
  current = score_ed_selected(state)
  return state if current.nil? || !current["tup"].nil? || current["dots"] > 0

  d = n == 3 ? 2 : 4
  base = current["base"] * d
  return state if base > 32

  state = score_ed_remember(state)
  sel = state["sel"]
  current = state["notes"][sel]
  state["tup_seq"] = (state["tup_seq"] ?? 0) + 1
  group = range(0, n).map { |k| current.merge({"base": base, "dots": 0, "tie": false, "slur_on": k == 0 && current["slur_on"] == true, "slur_off": k == n - 1 && current["slur_off"] == true, "set": k == 0 ? (current["set"] ?? {}) : {}, "rep_start": k == 0 && current["rep_start"] == true, "rep_end": k == n - 1 && current["rep_end"] == true, "pitches": current["pitches"].map { |pt| pt.merge({}) }, "tup": {"g": state["tup_seq"], "n": n, "d": d}}) }
  notes = state["notes"]
  state["notes"] = range(0, sel).map { |k| notes[k] } + group + range(sel + 1, notes.length).map { |k| notes[k] }
  state["typing"] = false
  state
end

def score_ed_dot(state)
  current = score_ed_selected(state)
  if current.nil? || state["typing"] == true || score_ed_inserting(state)
    state["dot"] = !state["dot"] && state["dur"] != 32
    return state
  end
  return state if current["base"] == 32

  state = score_ed_remember(state)
  current = score_ed_selected(state)
  current["dots"] = current["dots"] > 0 ? 0 : 1
  state["dot"] = current["dots"] > 0
  state
end

# `by` raises or lowers by a semitone; `absolute` sets the alteration
# outright (a natural). Every head of a chord moves together.
def score_ed_alter(state, by, absolute)
  current = score_ed_selected(state)
  return state if current.nil? || current["rest"]

  state = score_ed_remember(state)
  current = score_ed_selected(state)
  for pt in current["pitches"]
    want = absolute ? by : pt["alter"] + by
    pt["alter"] = want.clamp(-2, 2)
  end
  score_ed_blip(state, current)
end

# A semitone up or down, so every sharp and flat is on the way. It is
# spelled as a musician would: as the key has it when the key has it (A up
# in F major is B flat), else natural, else sharp going up and flat going
# down (F up in C is F sharp, G down is G flat).
def score_ed_semitone(state, by)
  current = score_ed_selected(state)
  return state if current.nil? || current["rest"]

  moved = current["pitches"].map { |pt| score_ed_spell(state, score_ed_midi_key(pt) + by, by) }.map { |sp| {"step": sp[0], "alter": sp[1], "oct": sp[2]} }
  # Out of reach: the note stays where it is.
  return state unless moved.all? { |pt| score_ed_fits(state, pt) }

  state = score_ed_remember(state)
  current = score_ed_selected(state)
  current["pitches"] = moved
  score_ed_blip(state, current)
end

# [step, alter, octave] for a MIDI key.
def score_ed_spell(state, midi, by)
  natural = [0, 2, 4, 5, 7, 9, 11]
  key = midi
  pc = key % 12
  spelled = nil
  in_key = range(0, 7).map { |st| score_ed_key_alter(state, st) }
  way = by > 0 ? 1 : -1
  toward = [way, way, way, way, way, way, way]
  against = [0 - way, 0 - way, 0 - way, 0 - way, 0 - way, 0 - way, 0 - way]
  for choice in [in_key, [0, 0, 0, 0, 0, 0, 0], toward, against]
    for st in range(0, 7)
      spelled = [st, choice[st]] if spelled.nil? && score_mod(natural[st] + choice[st] - pc, 12) == 0
    end
  end
  [spelled[0], spelled[1], (key - natural[spelled[0]] - spelled[1]) / 12 - 1]
end

# A step up or down the staff, keeping to the key: from E in G major up is
# F sharp. Seven steps is an octave.
def score_ed_move_pitch(state, by)
  current = score_ed_selected(state)
  return state if current.nil? || current["rest"]

  moved = []
  for pt in current["pitches"]
    d = pt["oct"] * 7 + pt["step"] + by
    step = d % 7
    moved.push({"step": step, "oct": d / 7, "alter": by == 7 || by == -7 ? pt["alter"] : score_ed_key_alter(state, step)})
  end
  return state unless moved.all? { |pt| score_ed_fits(state, pt) }

  state = score_ed_remember(state)
  current = score_ed_selected(state)
  current["pitches"] = moved
  score_ed_blip(state, current)
end

def score_ed_tie(state)
  current = score_ed_selected(state)
  return state if current.nil? || current["rest"]

  state = score_ed_remember(state)
  current = score_ed_selected(state)
  current["tie"] = !(current["tie"] == true)
  state
end

# S is a switch. The first press arms a slur on the selected note and
# lights the Slur button; the second ends it on the note selected then —
# typed since, or reached with the arrows — and puts the button out. A
# second press on the note it started from, on a rest or with no note,
# drops it; one on an earlier note slurs from there to the start. Nothing
# is written until both ends are known, so the source never holds a `(`
# without its `)`. With nothing armed, S on a note a slur starts from
# takes that slur away.
def score_ed_slur(state)
  current = score_ed_selected(state)
  start = state["slur_from"] ?? -1
  start = -1 if start >= state["notes"].length
  if start >= 0
    state["slur_from"] = -1
    return state if current.nil? || current["rest"] || state["sel"] == start

    state = score_ed_remember(state)
    notes = state["notes"]
    notes[score_min(start, state["sel"])]["slur_on"] = true
    notes[score_max(start, state["sel"])]["slur_off"] = true
    return state
  end
  return state if current.nil? || current["rest"]

  if current["slur_on"] == true
    state = score_ed_remember(state)
    score_ed_unslur(state["notes"], state["sel"])
    score_ed_selected(state)["slur_on"] = false
    return state
  end
  state["slur_from"] = state["sel"]
  state
end

# A repeat sign at the selected bar: `rep_start` on the note that opens it,
# `rep_end` on the note that closes it. Pressed again, it comes off. The
# signs are bar lines, so they follow the bar when notes before it change.
def score_ed_repeat(state, which)
  return score_ed_flag(state, "error", "Repeats and breaks are the first voice's: switch to Voice 1 (Ctrl+Alt+1).") if state["voice"] == 2
  return score_ed_flag(state, "error", "Repeats and breaks are set on the first track: they are the bars', and every staff has them.") if score_ed_tracks(state).length > 1 && (state["track"] ?? 0) != 0
  sel = state["sel"] ?? -1
  return state if sel < 0 || sel >= state["notes"].length

  state = score_ed_remember(state)
  bar_of = score_ed_bars_of(state)
  mates = range(0, state["notes"].length).filter { |k| bar_of[k] == bar_of[sel] }
  marked = mates.filter { |k| state["notes"][k][which] == true }
  if marked.length > 0
    for k in marked
      state["notes"][k][which] = false
    end
    return state
  end
  state["notes"][which == "rep_start" ? mates[0] : mates[-1]][which] = true
  state
end

# A dynamic on the selected note or rest; the same one again takes it off.
def score_ed_dynamic(state, dyn)
  current = score_ed_selected(state)
  return state if current.nil? || !SCORE_ED_DYNAMICS.includes?(dyn)

  state = score_ed_remember(state)
  current = score_ed_selected(state)
  current["dyn"] = current["dyn"] == dyn ? nil : dyn
  state
end

# A hairpin from the selected note, "<" or ">", running to the next note
# with a dynamic or an end; "!" ends one on the selected note. Each, again,
# takes itself off.
# A crescendo or a diminuendo is set as a slur is (`score_ed_slur`): the
# first press arms it on the selected note and lights its button, the
# second closes it on the note selected then — `\\<` on the first, `\\!` on
# the last. Pressed on the note it started from it is dropped; on an
# earlier note it runs from there; the other button while armed switches
# which it is. With nothing armed, a press on a note a hairpin starts from
# takes that hairpin away, end and all.
def score_ed_hairpin(state, grow)
  return state unless ["<", ">"].includes?(grow)

  current = score_ed_selected(state)
  start = state["pin_from"] ?? -1
  start = -1 if start >= state["notes"].length
  if start >= 0
    if !current.nil? && state["sel"] == start && state["pin_grow"] != grow
      state["pin_grow"] = grow
      return state
    end
    kind = state["pin_grow"] ?? grow
    state["pin_from"] = -1
    return state if current.nil? || state["sel"] == start

    state = score_ed_remember(state)
    notes = state["notes"]
    first = score_min(start, state["sel"])
    last = score_max(start, state["sel"])
    notes[first]["hairpin"] = kind
    notes[last]["hairpin_end"] = true
    return state
  end
  return state if current.nil?

  if !current["hairpin"].nil?
    state = score_ed_remember(state)
    score_ed_unpin(state["notes"], state["sel"])
    return state
  end
  state["pin_from"] = state["sel"]
  state["pin_grow"] = grow
  state
end

# A hairpin that starts on note `at` goes, its end with it.
def score_ed_unpin(notes, at)
  notes[at]["hairpin"] = nil
  done = false
  for k in range(at, notes.length)
    if !done && notes[k]["hairpin_end"] == true
      notes[k]["hairpin_end"] = false
      done = true
    end
    # Another hairpin or a dynamic ends this one where it stands.
    done = true if k > at && (!notes[k]["hairpin"].nil? || !notes[k]["dyn"].nil?)
  end
  notes
end

# An articulation on the selected note, on or off.
def score_ed_artic(state, name)
  current = score_ed_selected(state)
  return state if current.nil? || current["rest"] || !SCORE_ED_ARTICS.includes?(name)

  state = score_ed_remember(state)
  current = score_ed_selected(state)
  had = (current["scripts"] ?? []).filter { |sc| sc["name"] == name }.length > 0
  kept = (current["scripts"] ?? []).filter { |sc| sc["name"] != name }
  current["scripts"] = had ? kept : kept + [ {"name": name, "dir": 0} ]
  state
end

# Words over (-1) or under (1) the selected note; empty takes them off.
# The field sends what it holds at every keystroke, so a run of typing on
# one note's one side is one step to undo, not a step a letter.
def score_ed_words(state, words, dir)
  current = score_ed_selected(state)
  return state if current.nil?

  at = str(state["track"] ?? 0) + "|" + str(state["voice"] ?? 1) + "|" + str(state["sel"]) + "|" + str(dir)
  if state["words_at"] != at
    state = score_ed_remember(state)
    state["words_at"] = at
  end
  current = score_ed_selected(state)
  kept = (current["texts"] ?? []).filter { |tx| tx["dir"] != dir }
  current["texts"] = words.trim == "" ? kept : kept + [ {"text": words, "dir": dir} ]
  state
end

# What a note carries besides its pitch, in LilyPond's postfix: on its
# first piece the dynamic, the hairpin it starts, its articulations and
# its words; on its last, the end of a hairpin.
# A mark or Clear pressed: a step of its own, never folded into typing
# before or after it.
def score_ed_words_pressed(state, words)
  state["words_at"] = ""
  state = score_ed_words(state, words, score_ed_words_side(state))
  state["words_at"] = ""
  state
end

# The side the words field writes: -1 above, 1 below.
def score_ed_words_side(state)
  state["words_dir"] == "Below" ? 1 : -1
end

# The words a score most often carries, one press each: how to play
# (above, as LilyPond sets markup over the staff) and how the loudness goes
# (below, with the dynamics) — either written on the side chosen.
def score_ed_word_marks()
  ["dolce", "espress.", "cantabile", "legato", "rit.", "rall.", "a tempo", "cresc.", "dim.", "poco a poco", "sub.", "pizz.", "arco"]
end

def score_ed_expression_ly(nt, first_piece, last_piece)
  out = ""
  if first_piece
    out += "\\" + nt["dyn"] unless nt["dyn"].nil?
    for sc in nt["scripts"] ?? []
      side = {"-1": "^", "0": "-", "1": "_"}[str(sc["dir"] ?? 0)]
      out += sc["name"] == "fermata" ? side + "\\fermata" : side + {"staccato": ".", "accent": ">", "tenuto": "-", "marcato": "^", "staccatissimo": "!", "portato": "_"}[sc["name"]]
    end
    for tx in nt["texts"] ?? []
      out += (tx["dir"] < 0 ? "^" : "_") + "\"" + score_ed_clean(tx["text"]) + "\"" if score_ed_clean(tx["text"]) != ""
    end
    out += "\\" + nt["hairpin"] unless nt["hairpin"].nil?
  end
  out += "\\!" if last_piece && nt["hairpin_end"] == true
  out
end

# A line or a page that ends after the selected bar: `brk` on the note that
# closes it, written `\break` or `\pageBreak` after its bar line. Pressed
# again, it comes off; the other kind replaces it.
def score_ed_break(state, kind)
  return score_ed_flag(state, "error", "Repeats and breaks are set on the first track: they are the bars', and every staff has them.") if score_ed_tracks(state).length > 1 && (state["track"] ?? 0) != 0
  sel = state["sel"] ?? -1
  return state if sel < 0 || sel >= state["notes"].length

  state = score_ed_remember(state)
  bar_of = score_ed_bars_of(state)
  mates = range(0, state["notes"].length).filter { |k| bar_of[k] == bar_of[sel] }
  had = mates.filter { |k| state["notes"][k]["brk"] == kind }.length > 0
  for k in mates
    state["notes"][k]["brk"] = nil
  end
  state["notes"][mates[-1]]["brk"] = kind unless had
  state
end

# The bar each note starts in, counting from 0.
def score_ed_bars_of(state)
  measure = score_ed_bar_ticks(state)
  pos = 0
  bar = 0
  out = []
  for nt in state["notes"]
    changes = nt["set"] ?? {}
    measure = score_ed_measure_of(changes["time"]) if changes.has_key("time") && pos == 0
    out.push(bar)
    pos += score_ed_ticks(nt)
    bar += pos / measure
    pos = pos % measure
  end
  out
end

# ---- Tracks --------------------------------------------------------------
#
# A piece is one track or several: a staff each, with its name, its
# instrument and how loud and where it sits when it is played. One track
# is edited at a time — its notes and clef are the state's `notes` and
# `clef`, so every editing function works on it unchanged — and the others
# wait in `tracks`, the active one's entry holding only its settings. What
# belongs to the bar and not to a staff (a change of key, metre or tempo
# from a bar on, a repeat, a break) is set on the first track and written
# into every staff; a track shorter than the longest is filled with rests.

const SCORE_ED_GROUPS = ["Bracket", "Brace", "None"]
# The instruments the track settings offer, in LilyPond's names.
const SCORE_ED_INSTRUMENTS = ["acoustic grand", "electric piano 1", "harpsichord", "celesta", "vibraphone", "marimba", "church organ", "accordion", "acoustic guitar (nylon)", "acoustic bass", "violin", "viola", "cello", "contrabass", "string ensemble 1", "harp", "choir aahs", "trumpet", "trombone", "tuba", "french horn", "soprano sax", "alto sax", "oboe", "english horn", "bassoon", "clarinet", "piccolo", "flute", "recorder", "timpani"]

def score_ed_track_new(name)
  {"name": name, "short": "", "instrument": "acoustic grand", "volume": 100, "pan": 0, "mute": false}
end

# The tracks' settings; a state from before tracks has one, unnamed.
def score_ed_tracks(state)
  state["tracks"] = [score_ed_track_new("")] if (state["tracks"] ?? []).length == 0
  state["tracks"]
end

# Every track with its notes and clef, the active one's from the state.
def score_ed_all(state)
  active = state["track"] ?? 0
  mine = score_ed_v1(state)
  out = []
  for trk, k in score_ed_tracks(state)
    if k == active
      out.push(trk.merge({"notes": mine["notes"], "notes2": mine["notes2"] ?? [], "clef": state["clef"]}))
    else
      out.push(trk.merge({"notes": trk["notes"] ?? [], "notes2": trk["notes2"] ?? [], "clef": trk["clef"] ?? "Treble"}))
    end
  end
  out
end

# ---- Two voices -------------------------------------------------------------
#
# A track has a second voice beside its first (`notes2`), written as
# `<< { first } \\\\ { second } >>`: stems up and down, sounding together.
# The voice being edited is the one in `state["notes"]` — every key, click
# and value works on it as on a single voice — and the other waits in
# `state["notes2"]`; `state["voice"]` says which is which.

# The state as the first voice sees it: what the writers and the bars read.
def score_ed_v1(state)
  return state unless state["voice"] == 2

  state.merge({"notes": state["notes2"] ?? [], "notes2": state["notes"], "voice": 1})
end

# Edit voice `v`: the other one waits.
def score_ed_set_voice(state, v)
  current = state["voice"] ?? 1
  return state if v == current || (v != 1 && v != 2)

  other = state["notes2"] ?? []
  state["notes2"] = state["notes"]
  state["notes"] = other
  state["voice"] = v
  state["sel"] = -1
  state["typing"] = false
  state["slur_from"] = -1
  state["pin_from"] = -1
  state
end

# Whether a second voice has anything in it to write: a note, or a rest
# someone wrote.
def score_ed_voiced?(notes2)
  (notes2 ?? []).any? { |nt| !nt["rest"] || nt["spacer"] != true }
end

# Another track to edit: the active one's notes go back to its entry, the
# other's come out of theirs.
def score_ed_switch(state, k)
  tracks = score_ed_tracks(state)
  active = state["track"] ?? 0
  return state if k == active || k < 0 || k >= tracks.length

  state = score_ed_set_voice(state, 1)
  tracks[active]["notes2"] = state["notes2"] ?? []
  state["notes2"] = tracks[k]["notes2"] ?? []
  tracks[k]["notes2"] = nil
  tracks[active]["notes"] = state["notes"]
  tracks[active]["clef"] = state["clef"]
  state["notes"] = tracks[k]["notes"] ?? []
  state["clef"] = tracks[k]["clef"] ?? "Treble"
  tracks[k]["notes"] = nil
  tracks[k]["clef"] = nil
  state["track"] = k
  state["sel"] = state["notes"].length > 0 ? 0 : -1
  state["typing"] = false
  state["slur_from"] = -1
  state["pin_from"] = -1
  state
end

def score_ed_track_add(state)
  state = score_ed_remember_all(state)
  tracks = score_ed_tracks(state)
  if tracks.length == 1 && tracks[0]["name"] == ""
    tracks[0]["name"] = "Track 1"
  end
  fresh = score_ed_track_new("Track " + str(tracks.length + 1))
  fresh["notes"] = []
  fresh["clef"] = "Treble"
  tracks.push(fresh)
  score_ed_switch(state, tracks.length - 1)
end

def score_ed_track_remove(state)
  tracks = score_ed_tracks(state)
  return state if tracks.length < 2

  state = score_ed_remember_all(state)
  tracks = state["tracks"]
  gone = state["track"] ?? 0
  to = gone == 0 ? 1 : gone - 1
  state = score_ed_switch(state, to)
  state["tracks"] = range(0, tracks.length).filter { |k| k != gone }.map { |k| tracks[k] }
  state["track"] = to > gone ? to - 1 : to
  state
end

def score_ed_track_move(state, by)
  tracks = score_ed_tracks(state)
  at = state["track"] ?? 0
  to = at + by
  return state if to < 0 || to >= tracks.length

  state = score_ed_remember_all(state)
  tracks = state["tracks"]
  moving = tracks[at]
  tracks[at] = tracks[to]
  tracks[to] = moving
  state["track"] = to
  state
end

def score_ed_track_set(state, field, value)
  return state if value.nil?

  tracks = score_ed_tracks(state)
  tracks[state["track"] ?? 0][field] = value
  state
end

# What the first track says about each bar: changes of key, metre and
# tempo, repeats and breaks, by the bar's number from 0 — read the way the
# single-staff writer reads them (a change on a note that does not start a
# bar waits for the next bar line) — and where each bar starts, in ticks.
def score_ed_bar_marks(state)
  first = score_ed_all(state)[0]
  measure = score_ed_bar_ticks(state)
  marks = {}
  starts = [0]
  pos = 0
  tick = 0
  bar = 0
  pending = {}
  opening = false
  closing = false
  breaking = nil
  for nt in first["notes"]
    set = nt["set"] ?? {}
    if set.keys.length > 0
      if pos == 0
        score_ed_mark(marks, bar)["set"] = score_ed_mark(marks, bar)["set"].merge(set)
        measure = score_ed_measure_of(set["time"]) if set.has_key("time")
      else
        pending = pending.merge(set)
      end
    end
    if nt["rep_start"] == true
      if pos == 0
        score_ed_mark(marks, bar)["rep_start"] = true
      else
        opening = true
      end
    end
    left = score_ed_ticks(nt)
    while left > 0
      take = nt["tup"].nil? ? score_min(left, measure - pos) : left
      pos += take
      left -= take
      tick += take
      closing = true if nt["rep_end"] == true && left == 0
      breaking = nt["brk"] if !nt["brk"].nil? && left == 0
      if pos >= measure
        score_ed_mark(marks, bar)["rep_end"] = closing
        score_ed_mark(marks, bar)["brk"] = breaking
        closing = false
        breaking = nil
        bar += 1
        pos = 0
        starts.push(tick)
        if pending.keys.length > 0
          score_ed_mark(marks, bar)["set"] = pending
          measure = score_ed_measure_of(pending["time"]) if pending.has_key("time")
          pending = {}
        end
        if opening
          score_ed_mark(marks, bar)["rep_start"] = true
          opening = false
        end
      end
    end
  end
  {"marks": marks, "starts": starts, "measure": measure}
end

def score_ed_mark(marks, bar)
  key = str(bar)
  marks[key] = {"set": {}, "rep_start": false, "rep_end": false, "brk": nil} unless marks.has_key(key)
  marks[key]
end

# Where bar `b` starts, past the first track's end as well: its last bar's
# length on and on.
def score_ed_bar_tick(info, b)
  starts = info["starts"]
  return starts[b] if b < starts.length

  starts[-1] + (b - starts.length + 1) * info["measure"]
end

# The tick every track is filled to: the longest one's end, rounded up to
# its bar line.
def score_ed_ensemble_end(state, info)
  ends = 0
  for trk in score_ed_all(state)
    total = trk["notes"].reduce(fn(acc, nt) acc + score_ed_ticks(nt), 0)
    b = 0
    while score_ed_bar_tick(info, b) < total
      b += 1
    end
    ends = score_max(ends, score_ed_bar_tick(info, b))
  end
  ends
end

# A part's notes with its bars of rest gathered (`opts["gather"]`): what a
# player reads, as LilyPond's \\compressEmptyMeasures prints it. The full
# score keeps every bar, its staves lining up bar by bar. A run is whole-bar
# rests from a bar line on, the padding that fills a short part to the end
# split into bars, and it stops where a bar carries a mark — a change, a
# repeat, a break — so what is written there is still written. Every item
# keeps the index of the note it came from (`src`); a gathered one is
# `{"mm": bars}`, owned by its first rest.
def score_ed_gathered(notes, measure, marks)
  # A metre that changes makes a bar's length a question of where it is;
  # such a part is written bar by bar.
  return notes if marks.values.any? { |mk| (mk["set"] ?? {}).has_key("time") }

  bars = []
  tick = 0
  for nt, ni in notes
    left = nt["pad"] ?? score_ed_ticks(nt)
    if nt.has_key("pad")
      head = tick % measure == 0 ? 0 : score_min(left, measure - tick % measure)
      bars.push({"rest": true, "pitches": [], "base": 1, "dots": 0, "pad": head, "bar": tick / measure}) if head > 0
      left -= head
      tick += head
      while left >= measure
        bars.push({"rest": true, "pitches": [], "base": 1, "dots": 0, "pad": measure, "whole": true, "bar": tick / measure})
        left -= measure
        tick += measure
      end
      bars.push({"rest": true, "pitches": [], "base": 1, "dots": 0, "pad": left, "bar": tick / measure}) if left > 0
      tick += left
      next
    end
    plain = nt["rest"] && nt["tup"].nil? && nt["dyn"].nil? && nt["hairpin"].nil? && nt["hairpin_end"] != true && (nt["texts"] ?? []).length == 0 && (nt["scripts"] ?? []).length == 0
    bars.push(nt.merge({"src": ni, "whole": plain && tick % measure == 0 && left == measure, "bar": tick / measure}))
    tick += left
  end
  out = []
  k = 0
  while k < bars.length
    nt = bars[k]
    run = 0
    while nt["whole"] == true && k + run < bars.length && bars[k + run]["whole"] == true
      here = marks[str(nt["bar"] + run)] ?? {}
      break if run > 0 && (((here["set"] ?? {}).keys.length > 0) || here["rep_start"] == true)

      run += 1
      break if here["rep_end"] == true || !here["brk"].nil?
    end
    if run > 1
      out.push({"rest": true, "mm": run, "src": nt["src"]})
      k += run
      next
    end
    out.push(nt)
    k += 1
  end
  out
end

# One track written out, the bars' marks (from the first track) put on
# every staff and the track filled with rests to the ensemble's end. The
# header and the tempo only where `opts` asks: a staff of the full score
# has neither, the sheet one track is edited on has both.
def score_ed_layout_track(state, k, info, opts)
  trk = score_ed_all(state)[k]
  notes = trk["notes"]
  total = notes.reduce(fn(acc, nt) acc + score_ed_ticks(nt), 0)
  fill = score_ed_ensemble_end(state, info) - total
  notes = notes + [ {"rest": true, "pitches": [], "base": 1, "dots": 0, "pad": fill} ] if fill > 0
  measure = score_ed_bar_ticks(state)
  fifths = score_ed_fifths(state)
  words = []
  if opts["header"] == true
    titles = score_ed_header_ly(state["header"] ?? {})
    words.push(titles) if titles != ""
  end
  words.push("\\clef " + {"Treble": "treble", "Bass": "bass", "Alto": "alto"}[trk["clef"]] + " \\key " + SCORE_ED_KEY_LY[fifths + 7] + " \\major \\time " + state["time"])
  head = words.length
  if opts["tempo"] == true
    tempo_ly = score_ed_tempo_ly(state["tempo_text"] ?? "", state["tempo"], state["mark_tempo"] != false, state["tempo_base"] ?? 4)
    words.push(tempo_ly) if tempo_ly != ""
  end
  owners = []
  pos = 0
  bar = 0
  bar_at = -1
  marks = info["marks"]
  gathering = opts["gather"] == true && score_ed_chunks(measure).length == 1
  if gathering
    notes = score_ed_gathered(notes, measure, marks)
    words.push("\\compressEmptyMeasures") if notes.any? { |nt| !nt["mm"].nil? }
  end
  measure = score_ed_track_marks(words, marks, 0, measure, opts, -1)
  for nt, ni in notes
    # Bars of rest gathered: one word, its count over it.
    unless nt["mm"].nil?
      words.push("R" + score_ed_chunks(measure).map { |ch| str(ch[0]) + (ch[1] > 0 ? "." : "") }.join("") + "*" + str(nt["mm"]))
      owners.push(nt["src"] ?? -1)
      bar += nt["mm"] - 1
      here = marks[str(bar)] ?? {}
      words.push(here["rep_end"] == true ? "\\bar \":|.\"" : "|")
      bar_at = words.length - 1
      bar += 1
      more = ni + 1 < notes.length
      words.push(here["brk"] == "page" ? "\\pageBreak" : "\\break") if !here["brk"].nil? && more
      measure = score_ed_track_marks(words, marks, bar, measure, opts, bar_at) if more
      next
    end
    left = nt["pad"] ?? score_ed_ticks(nt)
    tup = nt["tup"]
    tup_opens = !tup.nil? && (ni == 0 || notes[ni - 1]["tup"].nil? || notes[ni - 1]["tup"]["g"] != tup["g"])
    tup_closes = !tup.nil? && (ni + 1 >= notes.length || notes[ni + 1]["tup"].nil? || notes[ni + 1]["tup"]["g"] != tup["g"])
    words.push("\\tuplet " + str(tup["n"]) + "/" + str(tup["d"]) + " {") if tup_opens
    full = left
    while left > 0
      take = tup.nil? ? score_min(left, measure - pos) : left
      chunks = tup.nil? ? score_ed_chunks(take) : [ [nt["base"], nt["dots"]] ]
      for ch, ci in chunks
        last_piece = left - take == 0 && ci == chunks.length - 1
        word = nt["rest"] ? "r" : score_ed_chord_ly(nt)
        word += str(ch[0]) + (ch[1] > 0 ? "." : "")
        word += " ~" if !nt["rest"] && (!last_piece || nt["tie"] == true)
        word += "(" if nt["slur_on"] == true && left == full && ci == 0
        word += ")" if nt["slur_off"] == true && last_piece
        word += score_ed_expression_ly(nt, left == full && ci == 0, last_piece) unless nt.has_key("pad")
        words.push(word)
        owners.push(nt.has_key("pad") ? -1 : (nt["src"] ?? ni))
      end
      words.push("}") if tup_closes
      pos += take
      left -= take
      if pos >= measure
        here = marks[str(bar)] ?? {}
        words.push(here["rep_end"] == true ? "\\bar \":|.\"" : "|")
        bar_at = words.length - 1
        pos = 0
        bar += 1
        more = left > 0 || ni + 1 < notes.length
        words.push(here["brk"] == "page" ? "\\pageBreak" : "\\break") if !here["brk"].nil? && more
        measure = score_ed_track_marks(words, marks, bar, measure, opts, bar_at) if more
      end
    end
  end
  if notes.length > 0 && pos == 0 && bar_at == words.length - 1
    words[bar_at] = "\\bar \"|.\"" if words[bar_at] == "|"
  elsif notes.length > 0
    words.push("\\bar \"|.\"")
  end
  {"words": words, "map": owners, "head": head}
end

# A bar's marks, written where it starts: its changes (the tempo only on a
# staff that carries tempos), and a repeat opening there turned into the
# bar line before. Answers the bar's length.
def score_ed_track_marks(words, marks, bar, measure, opts, bar_at)
  here = marks[str(bar)]
  return measure if here.nil?

  set = here["set"]
  if opts["tempo"] != true && set.has_key("tempo")
    kept = {}
    for key in set.keys
      kept[key] = set[key] unless key == "tempo"
    end
    set = kept
  end
  changes = score_ed_changes_ly(set)
  for w in changes["words"]
    words.push(w)
  end
  if here["rep_start"] == true && bar_at >= 0
    words[bar_at] = score_ed_bar_ly(words[bar_at], "|:")
  end
  changes["time"].nil? ? measure : score_ed_measure_of(changes["time"])
end

# The whole piece as LilyPond: one track as it always was; several as a
# staff group (or a piano staff, or plain staves), each with its name, its
# instrument and its loudness and place for MIDI.
# `muting`: a muted track is written at no volume, for playing — its notes
# keep their times, so the playhead still lights them.
def score_ed_full_source(state, muting = false)
  # A hundred milliseconds for an orchestra, asked for by several parts of
  # one render: written once per score, on a fingerprint that costs two.
  print_of = str([muting, state["notes"], state["tracks"], state["track"], state["clef"], state["header"], state["group"], state["key"], state["time"], state["tempo"], state["tempo_base"], state["tempo_text"], state["mark_tempo"]])
  known = score_memo_get("ed_full", print_of)
  return known unless known.nil?

  written = score_ed_write_full(state, muting)
  score_memo_put("ed_full", print_of, written, 4)
  written
end

def score_ed_write_full(state, muting)
  tracks = score_ed_all(state)
  return score_ed_layout(state)["src"] if tracks.length < 2

  info = score_ed_bar_marks(state)
  staves = []
  for trk, k in tracks
    music = score_ed_track_music(state, k, info, {"header": false, "tempo": k == 0})["words"].join(" ")
    heard = muting && trk["mute"] == true ? trk.merge({"volume": 0}) : trk
    lyrics = score_ed_lyrics_ly(trk["lyrics"] ?? "")
    staves.push("\\new Staff \\with { " + score_ed_track_with(heard) + "} { " + music + " }" + (lyrics == "" ? "" : " \\addlyrics { " + lyrics + " }"))
  end
  opener = {"Bracket": "\\new StaffGroup ", "Brace": "\\new PianoStaff ", "None": ""}[state["group"] ?? "Bracket"] ?? ""
  titles = score_ed_header_ly(state["header"] ?? {})
  (titles == "" ? "" : titles + " ") + opener + "<< " + staves.join(" ") + " >>"
end

# ---- Lyrics ----------------------------------------------------------------
#
# A track's words, as LilyPond's `\\addlyrics` takes them: a syllable a
# note, `--` between a word's syllables, `__` to hold one over the notes
# after it, `_` for a note that takes none. Kept with the track's name and
# instrument as typed, and written after its music.

# The words as they go into the source: one line, nothing that would end
# the block or start a command, a straight quote made a curly one.
def score_ed_lyrics_ly(text)
  out = text.replace("\n", " ").replace("\r", " ").replace("\t", " ").replace("{", "").replace("}", "").replace("\\", "").replace("\"", "”")
  out.split(" ").filter { |w| w != "" }.join(" ")
end

# A staff's syllables read back as the words to edit.
def score_ed_lyrics_text(sylls)
  return "" if sylls.nil?

  words = []
  for sy in sylls
    words.push(sy["text"] == "" ? "_" : sy["text"])
    words.push("--") if sy["hyphen"] == true
    words.push("__") if sy["extend"] == true
  end
  words.join(" ")
end

# The lyrics typed: a run of typing is one step to undo, as words on a note
# are.
def score_ed_lyrics_set(state, text)
  trk = score_ed_tracks(state)[state["track"] ?? 0]
  return state if (trk["lyrics"] ?? "") == text

  at = "lyrics|" + str(state["track"] ?? 0)
  if state["words_at"] != at
    state = score_ed_remember(state)
    state["words_at"] = at
  end
  score_ed_tracks(state)[state["track"] ?? 0]["lyrics"] = text
  state
end

# How many syllables the words have, and how many of the first voice's
# notes take one: a note tied from the one before, or under a slur after
# its first, holds the syllable before; a rest takes none.
def score_ed_lyrics_count(state)
  trk = score_ed_tracks(state)[state["track"] ?? 0]
  sylls = score_ed_lyrics_ly(trk["lyrics"] ?? "").split(" ").filter { |w| w != "" && w != "--" && w != "__" }.length
  notes = score_ed_v1(state)["notes"]
  takers = 0
  tied = false
  slurred = false
  for nt in notes
    next if nt["rest"]

    takers += 1 unless tied || slurred
    tied = nt["tie"] == true
    slurred = (slurred || nt["slur_on"] == true) && nt["slur_off"] != true
  end
  {"syllables": sylls, "notes": takers}
end

def score_ed_track_with(trk)
  out = ""
  out += "instrumentName = " + score_ed_ly_string(trk["name"]) + " " if trk["name"].trim != ""
  out += "shortInstrumentName = " + score_ed_ly_string(trk["short"]) + " " if (trk["short"] ?? "").trim != ""
  out += "midiInstrument = " + score_ed_ly_string(trk["instrument"] ?? "acoustic grand") + " "
  out += "midiMaximumVolume = #" + score_pdf_num((trk["volume"] ?? 100) / 127.0) + " " if (trk["volume"] ?? 100) != 100
  out += "midiPanPosition = #" + score_pdf_num((trk["pan"] ?? 0) / 100.0) + " " if (trk["pan"] ?? 0) != 0
  out
end

# A field's words as they go into the source: no space at either end, and
# a straight quote, which would close the string, made a curly one.
def score_ed_clean(words)
  words.trim.replace("\"", "”")
end

def score_ed_ly_string(words)
  "\"" + words.trim.replace("\"", "'") + "\""
end

def score_ed_select(state, at)
  count = state["notes"].length
  return state if count == 0

  state["sel"] = at.clamp(0, count - 1)
  state["typing"] = false
  state
end

# A clef, a key or a metre, for the whole piece — or, for a key or a metre
# with "From the selected bar" chosen, from the bar the selection is in
# onwards: written on the note that opens it, as `\key` or `\time` there.
def score_ed_settings(state, which, value)
  return state if value.nil?

  allowed = {"clef": SCORE_ED_CLEFS, "time": SCORE_ED_TIMES, "key": SCORE_ED_KEYS}[which]
  return state unless allowed.includes?(value)

  if state["voice"] == 2 && state["scope"] == SCORE_ED_SCOPES[1] && which != "clef"
    state["error"] = "A change from a bar is the first voice's: switch to Voice 1 (Ctrl+Alt+1)."
    return state
  end
  at = score_ed_scope_at(state, which)
  if at > 0 && score_ed_tracks(state).length > 1 && (state["track"] ?? 0) != 0
    return score_ed_settings_first(state, which, value, at)
  end
  if at <= 0
    return state if state[which] == value

    state = score_ed_remember(state)
    state[which] = value
    return state
  end
  state = score_ed_remember(state)
  opener = state["notes"][at]
  changes = opener["set"] ?? {}
  changes[which] = value
  opener["set"] = changes
  state
end

# A change from a bar chosen on another track — a note picked on the
# violins in the full score — is the first track's to carry, as every
# change of key or metre is: it lands on the first track's note that
# starts where the selected bar does, and every staff takes it.
def score_ed_settings_first(state, which, value, at)
  tick = 0
  for nt, k in state["notes"]
    break if k >= at

    tick += score_ed_ticks(nt)
  end
  first = score_ed_tracks(state)[0]["notes"] ?? []
  pos = 0
  hit = -1
  for nt, k in first
    if pos == tick
      hit = k
      break
    end
    break if pos > tick

    pos += score_ed_ticks(nt)
  end
  if hit < 0
    state["error"] = "The first track has no note starting at that bar to carry the change."
    return state
  end
  state = score_ed_remember_all(state)
  opener = first[hit]
  changes = opener["set"] ?? {}
  changes[which] = value
  opener["set"] = changes
  state["error"] = ""
  state
end

# Where a change made now lands: -1 for the whole piece, or the note that
# opens the selected bar.
def score_ed_scope_at(state, which)
  return -1 if which == "clef" || state["scope"] != SCORE_ED_SCOPES[1]

  sel = state["sel"] ?? -1
  return -1 if sel < 0

  score_ed_bar_start(state, sel)
end

# The last note at or before `idx` that starts a bar.
def score_ed_bar_start(state, idx)
  measure = score_ed_measure_of(state["time"])
  pos = 0
  best = 0
  for nt, k in state["notes"]
    changes = nt["set"] ?? {}
    measure = score_ed_measure_of(changes["time"]) if changes.has_key("time") && pos == 0
    best = k if pos == 0
    break if k >= idx

    pos = (pos + score_ed_ticks(nt)) % measure
  end
  best
end

def score_ed_measure_of(time)
  parts = time.split("/")
  int(parts[0]) * score_whole() / int(parts[1])
end

# What a field holds is kept as it was typed — a space before the next
# word included — and cleaned only where it is written out
# (`score_ed_clean`). Trimmed here, a pause before a capital sent "Frère "
# and the field came back as "Frère", glued to whatever came next.
def score_ed_header_set(state, field, value)
  return state if (state["header"] ?? {})[field] == value

  state = score_ed_remember(state)
  hdr = state["header"] ?? {}
  hdr[field] = value
  state["header"] = hdr
  state
end

def score_ed_clear(state)
  state = score_ed_remember(state)
  state["notes"] = []
  state["sel"] = -1
  state
end

# A whole piece replaces the score — every track, the titles, the tempo —
# read the way a pasted source is, so Ctrl+Z brings back what was there.
def score_ed_example(state, name)
  state["ex_open"] = false
  state["playing"] = false
  src = score_ed_example_source(name)
  return state if src.nil?

  score_ed_from_source(state, src)
end

# Frère Jacques, written here; the Beethoven exposition from the file
# tools/mutopia_beethoven5.py made of Mutopia's public-domain engraving.
def score_ed_example_source(name)
  if name == SCORE_ED_EXAMPLES[0]
    return "\\header { title = \"Frère Jacques\" composer = \"Traditional\" } { \\tempo \"Moderato\" 4 = 100 \\clef treble \\key c \\major \\time 4/4 c'4 d' e' c' c' d' e' c' e' f' g'2 e'4 f' g'2 }"
  end
  return nil unless name == SCORE_ED_EXAMPLES[1]

  File.read("public/scores/beethoven-symphony-5-1.ly") rescue nil
end

# ---- Sound -----------------------------------------------------------------

# The tempo: beats a minute, the words over the staff, and whether it is
# printed at all. "From the selected bar" writes it as a change there.
def score_ed_tempo(state, bpm, tempo_words, printed)
  return state if bpm < SCORE_ED_BPM[0] || bpm > SCORE_ED_BPM[1]

  state["playing"] = false
  state["play_at"] = -1
  at = state["scope"] == SCORE_ED_SCOPES[1] && (state["sel"] ?? -1) >= 0 ? score_ed_bar_start(state, state["sel"]) : 0
  if at > 0
    state = score_ed_remember(state)
    opener = state["notes"][at]
    changes = opener["set"] ?? {}
    changes["tempo"] = {"bpm": bpm, "text": tempo_words, "base": state["tempo_base"] ?? 4}
    opener["set"] = changes
    return state
  end
  state["tempo"] = bpm
  state["tempo_text"] = tempo_words
  state["mark_tempo"] = printed
  state
end

# Play from the start, or stop. The MIDI file is written again only when
# what it would sound like has changed since the last time.
def score_ed_play(state)
  return score_ed_stop(state) if state["playing"] == true
  return state if state["notes"].length == 0

  several = score_ed_tracks(state).length > 1
  print_of = (several ? score_ed_full_source(state) + str(score_ed_tracks(state).map { |trk| trk["mute"] == true }) : score_ed_layout(state)["src"]) + " @" + str(state["tempo"])
  if state["audio_print"] != print_of
    state["audio"] = eui_asset(score_midi(score_ed_full_source(state, true), score_ed_midi_opts(state)))
    state["starts_tracks"] = score_ed_timing(state)
    state["starts"] = state["starts_tracks"][state["track"] ?? 0]
    state["audio_print"] = print_of
  end
  state["playing"] = true
  state["plays"] = (state["plays"] ?? 0) + 1
  state["play_at"] = 0
  state["play_ms"] = 0
  # Not yet: the window has a MIDI file to fetch and synthesize before the
  # first note sounds — three quarters of a second for a symphony — and a
  # playhead started now ran that far ahead of the music. It starts at the
  # window's first report of where the sound is (`score_ed_playhead`).
  state["play_wall"] = nil
  state["play_shown_ms"] = -1
  score_ed_light_to(state, 0)
end

# Seconds, as a float, to the microsecond. Not `clock()`: an application's
# own `def clock` replaces the builtin everywhere in it, and the demo's live
# clock is a component handler of that name.
def score_ed_now()
  DateTime.microtime() / 1000000.0
end

def score_ed_stop(state)
  state["playing"] = false
  state["play_at"] = -1
  state
end

# The window says where the sound is; the note it is on is the last one to
# have started.
# The window says where the sound is, four times a second: too seldom for
# an eighth at a quick tempo to be lit by the one report that falls inside
# it. So the report is a fix and not the whole clock: it is stamped with
# the server's time, a `wake` every 100 ms carries the playhead on from it
# (`score_ed_play_tick`), and every note that began since the last step is
# lit with the one sounding now — a run of short notes lights together
# rather than not at all.
def score_ed_playhead(state, payload)
  return state unless state["playing"] == true

  state["play_ms"] = payload[0] ?? 0
  state["play_wall"] = score_ed_now()
  score_ed_light_to(state, state["play_ms"])
end

def score_ed_play_tick(state)
  return state unless state["playing"] == true && !state["play_wall"].nil?

  # Carried on from the last report, never more than 600 ms past it: a
  # window that stopped reporting has stopped playing too.
  ahead = score_min(600.0, (score_ed_now() - state["play_wall"]) * 1000.0)
  score_ed_light_to(state, state["play_ms"] + int(ahead))
end

# The notes of every track that began after the last step and by `at`,
# and the one sounding at `at`, lit.
def score_ed_light_to(state, at)
  shown_to = state["play_shown_ms"] ?? -1
  return state if at <= shown_to

  starts_tracks = state["starts_tracks"] ?? [state["starts"] ?? []]
  lit_tracks = []
  for starts, k in starts_tracks
    lit = []
    for start, j in starts
      lit.push(j) if start > shown_to && start <= at
    end
    now_on = score_ed_lit_at(starts_tracks, k, at)
    lit.push(now_on) if now_on >= 0 && !lit.includes?(now_on)
    lit_tracks.push(lit)
  end
  state["play_lit"] = lit_tracks
  state["play_shown_ms"] = at
  state["play_at"] = score_ed_lit_at(starts_tracks, state["track"] ?? 0, at)
  state
end

# The note of track `k` sounding at `at` ms, or -1 when it has ended.
def score_ed_lit_at(starts_tracks, k, at)
  return -1 if k >= starts_tracks.length

  idx = -1
  for start, j in starts_tracks[k]
    idx = j if start <= at
  end
  idx
end

# The first track's tempo, by tick: [tick, beats a minute, beat value].
def score_ed_tempo_map(notes, state)
  out = [ [0, state["tempo"], state["tempo_base"] ?? 4] ]
  tick = 0
  for nt in notes
    change = (nt["set"] ?? {})["tempo"]
    out.push([tick, change["bpm"], change["base"] ?? 4]) unless change.nil?
    tick += score_ed_ticks(nt)
  end
  out
end

def score_ed_tempo_at(tempos, tick)
  beat = [tempos[0][1], tempos[0][2]]
  for t in tempos
    beat = [t[1], t[2]] if t[0] <= tick
  end
  beat
end

# ---- Sound in the window ---------------------------------------------------

# The tempo for a source with no `\\tempo`, and the instrument for one
# staff, which carries no `\\with`.
def score_ed_midi_opts(state)
  first = score_ed_tracks(state)[0]
  {"bpm": state["tempo"], "beat": state["tempo_base"] ?? 4, "program": SCORE_MIDI_PROGRAMS[first["instrument"] ?? "acoustic grand"] ?? 0}
end

# When each note of each track starts, in ms: the playhead's map, without a
# sample mixed. The tempo is the first track's, change by change, as the
# MIDI file plays it.
def score_ed_timing(state)
  tracks = score_ed_all(state)
  tempos = score_ed_tempo_map(tracks[0]["notes"], state)
  holds = score_ed_holds(tracks)
  out = []
  for trk in tracks
    starts = []
    clock_ms = 0.0
    tick = 0
    next_hold = 0
    for nt in trk["notes"]
      # The fermatas ended by now hold every staff, as the MIDI file does.
      while next_hold < holds.length && holds[next_hold][0] <= tick
        beat = score_ed_tempo_at(tempos, holds[next_hold][0])
        clock_ms += score_ed_ticks_ms(beat[0], beat[1], holds[next_hold][1])
        next_hold += 1
      end
      starts.push(int(clock_ms))
      beat = score_ed_tempo_at(tempos, tick)
      clock_ms += score_ed_note_ms(beat[0], beat[1], nt)
      tick += score_ed_ticks(nt)
    end
    out.push(starts)
  end
  out
end

# The fermatas of every track as [tick the held note ends, ticks held]:
# `score_midi_holds`, on the editor's model.
def score_ed_holds(tracks)
  by_end = {}
  for trk in tracks
    tick = 0
    for nt in trk["notes"]
      length = score_ed_ticks(nt)
      held = !nt["rest"] && (nt["scripts"] ?? []).any? { |sc| sc["name"] == "fermata" }
      by_end[str(tick + length)] = score_max(by_end[str(tick + length)] ?? 0, length) if held
      tick += length
    end
  end
  by_end.keys.map { |k| [int(k), by_end[k]] }.sort_by { |h| h[0] }
end

def score_ed_ticks_ms(bpm, beat_base, ticks)
  quarter_ms = 60000.0 / bpm * beat_base / 4.0
  ticks * quarter_ms / (score_whole() / 4.0)
end

# A note written, played as it lands: MIDI commands, the track's
# instrument then each pitch for under half a second — short, so typing
# fast is not drowned. `notes_seq` moving is what plays it.
def score_ed_blip(state, nt)
  return state unless state["sound"] == true
  return state if nt["rest"]

  trk = score_ed_tracks(state)[state["track"] ?? 0]
  cmds = [ [0, 192, SCORE_MIDI_PROGRAMS[trk["instrument"] ?? "acoustic grand"] ?? 0, 0] ]
  for pt in nt["pitches"]
    key = score_ed_midi_key(pt)
    cmds.push([0, 144, key, 100])
    cmds.push([450, 128, key, 0])
  end
  state["blip_notes"] = cmds
  state["blips"] = (state["blips"] ?? 0) + 1
  state
end

# The bank the window plays with: GeneralUser GS, in the parts
# tools/fetch_soundfont.sh cut it into, each under the 16 MiB an asset may
# weigh. None fetched, none named: the window's own voices play.
def score_ed_soundfont()
  unless SCORE_MEMO.has_key("ed_soundfont")
    parts = []
    k = 0
    while File.exists("public/soundfonts/GeneralUser-GS.sf2." + str(k))
      parts.push("public/soundfonts/GeneralUser-GS.sf2." + str(k))
      k += 1
    end
    SCORE_MEMO["ed_soundfont"] = parts
  end
  SCORE_MEMO["ed_soundfont"]
end

# The tune while it plays, and one player for the notes written. That one
# stays in the tree, keyed alike, and plays when `notes_seq` moves: it holds
# the bank from the first render, so the window has it parsed before the
# first note.
def score_ed_sounds(state)
  bank = score_ed_soundfont()
  out = []
  if !state["audio"].nil? && state["playing"] == true
    out.push({"k": "audio", "key": "song:" + str(state["plays"] ?? 0), "p": {"src": state["audio"], "playing": true, "soundfont": bank, "wake": 100}, "on": {"time_update": "playhead", "ended": "ended", "wake": "play_tick"}})
  end
  player = {"soundfont": bank, "notes_seq": state["blips"] ?? 0}
  player["notes"] = state["blip_notes"] if !state["blip_notes"].nil? && state["sound"] == true
  out.push({"k": "audio", "key": "notes", "p": player})
  out
end

# ---- Pages -----------------------------------------------------------------
#
# A long score is shown a page at a time: only the page on screen is drawn
# and sent, so an orchestra's hundred and twenty bars cost what a page
# costs. On a screen a page is about a screenful of staff; in A4 it is the
# paper's. The page follows the selection, and the note playing.

# Logical px of staff a screen page holds.
def score_ed_screen_page()
  1100
end

# The page shown: the one asked for, within the pages there are.
def score_ed_page_shown(state, engraved, which)
  score_max(0, score_min(engraved["pages"].length - 1, state[which] ?? 0))
end

# How tall page `shown` is drawn: down to its last system's ink.
def score_ed_page_height(engraved, shown, sp)
  bottom = 0.0
  for sys in engraved["systems"]
    bottom = sys["bottom"] if (sys["page"] ?? 0) == shown && sys["bottom"] > bottom
  end
  int((bottom + 3.0) * sp)
end

# "◀ Previous  Page 2 of 9  Next ▶"; nothing for one page.
def score_ed_pager(state, which, count)
  return nil if count < 2

  shown = score_max(0, score_min(count - 1, state[which] ?? 0))
  row({"gap": 2, "align": "center"}, [
    score_ed_tool("◀ Previous", "page_to", {"which": which, "to": shown - 1}, shown == 0, nil),
    text("Page " + str(shown + 1) + " of " + str(count), {"size": 1, "weight": "medium"}),
    score_ed_tool("Next ▶", "page_to", {"which": which, "to": shown + 1}, shown + 1 >= count, nil)
  ])
end

# Whether the full score is what the sheet shows: the "Full score" entry of
# the track strip, chosen. It is not drawn otherwise — its engraving is most
# of what an edit to a big piece costs.
def score_ed_full_view(state)
  state["full_view"] == true && score_ed_tracks(state).length > 1
end

# The full score shown: a click there picks a staff and a note, so the
# pointer is the tool and the first voice the one edited — the full score
# is drawn from, and picked in, each staff's first voice.
def score_ed_full_on(state)
  state = score_ed_set_voice(state, 1)
  state["tool"] = SCORE_ED_TOOLS[0]
  score_ed_flag(state, "full_view", true)
end

# Staves times notes past what engraves in a blink.
def score_ed_conductor_big(state)
  count = 0
  for trk in score_ed_all(state)
    count += trk["notes"].length
  end
  count > 600
end

# The full score cut into screen pages.
def score_ed_conductor_engraved(state, full)
  sp = score_ed_full_space(state)
  score_layout(full["src"], float(score_ed_width(state)) / sp, {"page_h": score_ed_screen_page() / sp})
end

# The page note `owner` of the edited track is on, or nil: the page of the
# item drawn for it, or for the nearest note before it that has one — a
# rest gathered into a bar count is drawn by the run's first.
def score_ed_page_of(engraved, map, owner)
  best = nil
  best_owner = -1
  for sys in engraved["systems"]
    for bar in sys["bars"]
      for ev in bar["items"]
        k = ev["i"]
        next if k.nil? || k >= map.length

        here = map[k]
        return sys["page"] ?? 0 if here == owner
        if here >= 0 && here < owner && here > best_owner
          best_owner = here
          best = sys["page"] ?? 0
        end
      end
    end
  end
  best
end

# The note lit — the one playing, else the selection — decides the page,
# when it changes: paging by hand holds until the next note is chosen.
def score_ed_follow(state)
  lit = state["playing"] == true ? (state["play_at"] ?? -1) : (state["sel"] ?? -1)
  key = str(lit) + "|" + str(state["track"] ?? 0)
  return state if lit < 0 || key == state["page_lit"]

  state["page_lit"] = key
  drawn = score_ed_engrave(state)
  return state if drawn["engraved"].nil?

  on = score_ed_page_of(drawn["engraved"], drawn["map"], lit)
  state["page"] = on unless on.nil?
  return state unless score_ed_full_view(state)

  full = score_ed_conductor(state)
  try
    on = score_ed_cond_page_of(score_ed_conductor_engraved(state, full), full, state["track"] ?? 0, lit)
    state["cond_page"] = on unless on.nil?
  catch e
    return state
  end
  state
end

# The page of the full score note `lit` of track `active` is on, or nil.
# Items are numbered across the staves: staff k's item i is note
# `maps[k][i - firsts[k]]` of its track.
def score_ed_cond_page_of(cond, full, active, lit)
  first = full["firsts"][active]
  owners = full["maps"][active]
  for sys in cond["systems"]
    staves = sys["staves"] ?? []
    next if active >= staves.length

    for bar in staves[active]["bars"]
      for ev in bar["items"]
        j = ev["i"] - first
        return sys["page"] ?? 0 if j >= 0 && j < owners.length && owners[j] == lit
      end
    end
  end
  nil
end

# ---- The full score -------------------------------------------------------

# The conductor's source, and for each staff the notes of the track behind
# its items, and the number of its first item: items are numbered across
# the staves in order, so item `i` of staff `k` is `maps[k][i - firsts[k]]`.
def score_ed_conductor(state)
  print_of = str([state["notes"], state["tracks"], state["track"], state["clef"], state["header"], state["group"], state["key"], state["time"], state["tempo"], state["tempo_base"], state["tempo_text"], state["mark_tempo"]])
  known = score_memo_get("ed_cond", print_of)
  return known unless known.nil?

  laid = score_ed_lay_conductor(state)
  score_memo_put("ed_cond", print_of, laid, 4)
  laid
end

def score_ed_lay_conductor(state)
  info = score_ed_bar_marks(state)
  maps = []
  firsts = []
  count = 0
  for trk, k in score_ed_all(state)
    laid = score_ed_track_music(state, k, info, {"header": false, "tempo": k == 0})
    firsts.push(count)
    maps.push(laid["map"])
    count += laid["map"].length
  end
  {"src": score_ed_full_source(state), "maps": maps, "firsts": firsts}
end

# A click on the full score: the staff under it becomes the track being
# edited, and the note under it the selection.
def score_ed_conductor_pick(state, point)
  full = score_ed_conductor(state)
  sp = score_ed_full_space(state)
  engraved = nil
  try
    engraved = score_ed_conductor_engraved(state, full)
  catch e
    return state
  end
  shown = score_ed_page_shown(state, engraved, "cond_page")
  xs = float(point[0]) / sp
  ys = float(point[1]) / sp
  best = nil
  best_k = 0
  on_page = engraved["systems"].filter { |sy| (sy["page"] ?? 0) == shown }
  for sys in on_page
    for ss, k in sys["staves"] ?? []
      mid = sys["top"] + sys["offsets"][k] + 2.0
      if best.nil? || (ys - mid).abs < (ys - best["mid"]).abs
        best = {"mid": mid, "staff": ss}
        best_k = k
      end
    end
  end
  return state if best.nil?

  hit = nil
  for bar in best["staff"]["bars"]
    for ev in bar["items"]
      mid_x = ev["x"] + (ev["head_w"] ?? 1.18) / 2.0
      hit = ev if hit.nil? || (xs - mid_x).abs < (xs - hit["x"] - (hit["head_w"] ?? 1.18) / 2.0).abs
    end
  end
  state = score_ed_switch(state, best_k)
  return state if hit.nil?

  j = hit["i"] - full["firsts"][best_k]
  note = j >= 0 && j < full["maps"][best_k].length ? full["maps"][best_k][j] : -1
  state["sel"] = note if note >= 0
  state["typing"] = false
  state
end

# The full score's lit items: the note each track is sounding while it
# plays; otherwise the selection, on the track being edited.
def score_ed_conductor_marks(state, full)
  marks = {}
  playing = state["playing"] == true
  active = state["track"] ?? 0
  for owners, k in full["maps"]
    lit_set = playing ? ((state["play_lit"] ?? [])[k] ?? []) : (k == active && (state["sel"] ?? -1) >= 0 ? [state["sel"]] : [])
    next if lit_set.length == 0

    for owner, j in owners
      marks[str(full["firsts"][k] + j)] = "accent.base" if owner >= 0 && lit_set.includes?(owner)
    end
  end
  marks
end

# `bpm` beats of a `beat_base` note a minute: a quarter lasts 4 / base of
# a beat, so 8 = 160 plays a quarter at 80.
def score_ed_note_ms(bpm, beat_base, nt)
  quarter_ms = 60000.0 / bpm * beat_base / 4.0
  score_ed_ticks(nt) * quarter_ms / (score_whole() / 4.0)
end

def score_ed_midi_key(pt)
  score_max(0, score_min(127, 12 * (pt["oct"] + 1) + [0, 2, 4, 5, 7, 9, 11][pt["step"]] + pt["alter"]))
end

# ---- The keyboard ----------------------------------------------------------

def score_ed_key(state, key, mods)
  ctrl = mods % 4 >= 2 || mods % 16 >= 8
  lower = key.downcase()
  # A capital is a shifted letter whatever the modifiers say: some
  # keyboards report the shift only in the character.
  shift = mods % 2 == 1 || (key.length == 1 && key != lower)
  # Alt+↑ / Alt+↓: the track above or below.
  alt = (mods / 4) % 2 == 1
  return score_ed_switch(state, (state["track"] ?? 0) - 1) if alt && key == "ArrowUp"
  return score_ed_switch(state, (state["track"] ?? 0) + 1) if alt && key == "ArrowDown"

  if ctrl
    return score_ed_set_voice(state, 1) if alt && key == "1" && !score_ed_full_view(state)
    return score_ed_set_voice(state, 2) if alt && key == "2" && !score_ed_full_view(state)
    return score_ed_tuplet(state, 3) if key == "3"
    return score_ed_tuplet(state, 6) if key == "6"
    return score_ed_redo(state) if lower == "y" || (lower == "z" && shift)
    return score_ed_undo(state) if lower == "z"
    return score_ed_move_pitch(state, 7) if key == "ArrowUp"
    return score_ed_move_pitch(state, -7) if key == "ArrowDown"

    return state
  end
  return score_ed_flag(state, "tool", score_ed_inserting(state) ? SCORE_ED_TOOLS[0] : SCORE_ED_TOOLS[1]) if lower == "n" && !score_ed_full_view(state)
  step = "cdefgab".index_of(lower)
  return score_ed_insert(state, step, shift) if key.length == 1 && step >= 0
  return score_ed_insert_rest(state) if lower == "r" || key == "0"

  digit = "234567".index_of(key)
  return score_ed_duration(state, [32, 16, 8, 4, 2, 1][digit]) if key.length == 1 && digit >= 0
  return score_ed_dot(state) if key == "."
  return score_ed_alter(state, 1, false) if key == "+" || key == "#"
  return score_ed_alter(state, -1, false) if key == "-"
  return score_ed_alter(state, 0, true) if key == "="
  return score_ed_tie(state) if lower == "t"
  return score_ed_slur(state) if lower == "s"
  return score_ed_hairpin(state, key) if key == "<" || key == ">"
  return score_ed_repeat(state, "rep_start") if key == "["
  return score_ed_repeat(state, "rep_end") if key == "]"
  return score_ed_play(state) if key == " "
  return score_ed_semitone(state, 1) if shift && key == "ArrowUp"
  return score_ed_semitone(state, -1) if shift && key == "ArrowDown"
  return score_ed_move_pitch(state, 1) if key == "ArrowUp"
  return score_ed_move_pitch(state, -1) if key == "ArrowDown"
  return score_ed_select(state, (state["sel"] ?? 0) - 1) if key == "ArrowLeft"
  return score_ed_select(state, (state["sel"] ?? -1) + 1) if key == "ArrowRight"
  return score_ed_select(state, 0) if key == "Home"
  return score_ed_select(state, state["notes"].length - 1) if key == "End"
  return score_ed_delete(state, true) if key == "Backspace"
  return score_ed_delete(state, false) if key == "Delete"
  return score_ed_flag(state, "tool", SCORE_ED_TOOLS[0]) if key == "Escape" && score_ed_inserting(state)
  return score_ed_flag(score_ed_flag(state, "sel", -1), "typing", false) if key == "Escape"

  state
end

# ---- A MIDI keyboard --------------------------------------------------------
#
# Step input (EUI 06 §1.4): every key pressed on the instrument writes a note
# of the duration chosen in the palette, at the pitch played, spelled as the
# key of the piece spells it; keys pressed together — within 30 ms of each
# other — write one chord. Note offs, pedals and controllers write nothing:
# in this mode a note lasts what the palette says, not what the hand held.

# Note ons this close together are one chord.
def score_ed_chord_ms()
  30
end

def score_ed_midi_in(state, messages)
  chords = []
  last_t = -1000000
  for m in messages
    next unless m.is_a?("array") && m.length == 4

    status = m[1]
    on = status >= 144 && status < 160 && m[3] > 0
    next unless on

    if chords.length == 0 || m[0] - last_t > score_ed_chord_ms()
      chords.push([m[2]])
    else
      chords[-1].push(m[2])
    end
    last_t = m[0]
  end
  for keys in chords
    state = score_ed_play_chord(state, keys)
  end
  state
end

# One chord played: a new note after the selection, its pitches spelled in
# the key, the ones out of the staff's reach left out.
def score_ed_play_chord(state, keys)
  pitches = []
  for key in keys.sort()
    spelled = score_ed_spell(state, key, 1)
    pt = {"step": spelled[0], "alter": spelled[1], "oct": spelled[2]}
    pitches.push(pt) if score_ed_fits(state, pt) && !pitches.any? { |p| p["step"] == pt["step"] && p["oct"] == pt["oct"] }
  end
  return state if pitches.length == 0

  state = score_ed_remember(state)
  fresh = score_ed_note(pitches[0]["step"], pitches[0]["oct"], pitches[0]["alter"], state["dur"], state["dot"] ? 1 : 0)
  fresh["pitches"] = pitches
  score_ed_place(state, fresh)
end

# ---- The pointer -----------------------------------------------------------

# A click lands in the score's own box, in logical px. Find the system by
# its staff, the note by its head, and the pitch by the line or space.
# `page` is the sheet the click landed on, A4 being several; on a screen
# there is only the one.
def score_ed_pick(state, point, page)
  drawn = score_ed_engrave(state)
  return state if drawn["engraved"].nil?

  sp = drawn["sp"]
  xs = float(point[0]) / sp
  ys = float(point[1]) / sp
  every = drawn["engraved"]["systems"]
  systems = every.filter { |sy| (sy["page"] ?? 0) == page }
  return state if systems.length == 0

  best_sys = systems[0]
  for sys in systems
    best_sys = sys if (ys - sys["top"] - 2.0).abs < (ys - best_sys["top"] - 2.0).abs
  end
  state["drag_top"] = best_sys["top"]
  pitch = score_ed_pitch_at(state, score_ed_pos_at(ys, best_sys["top"]))
  hit = nil
  hit_gap = 99.0
  last_x = 0.0
  for bar in best_sys["bars"]
    for ev in bar["items"]
      mid = ev["x"] + (ev["head_w"] ?? 1.18) / 2.0
      last_x = ev["x"] if ev["x"] > last_x
      if (xs - mid).abs < hit_gap
        hit_gap = (xs - mid).abs
        hit = ev
      end
    end
  end
  is_last_sys = best_sys["bars"][-1]["number"] == every[-1]["bars"][-1]["number"]
  # Past the last note of the score: a new note at the pitch under the
  # pointer.
  if is_last_sys && (hit.nil? || xs > last_x + 2.5) && state["voice"] != 2
    state["sel"] = -1
    state = score_ed_insert(state, pitch["step"], false)
    current = score_ed_selected(state)
    current["pitches"] = [pitch]
    return state
  end
  return state if hit.nil? || hit_gap > 2.0

  model = drawn["map"][hit["i"]]
  if state["sel"] == model
    return score_ed_set_pitch(state, pitch)
  end
  state["sel"] = model
  state["typing"] = false
  state
end

# The line or space at `ys` staff spaces down, on a staff whose top line is
# at `top`: 0 the middle line, a step a line or a space.
def score_ed_pos_at(ys, top)
  int(((2.0 - (ys - top)) * 2.0).round(0))
end

# A press on the score is a click's work — select a note, move the one
# selected, add one past the end — and then holds the note: while the
# button stays down, the note follows the pointer up and down its staff.
def score_ed_press(state, point, page)
  return score_ed_insert_at(state, point, page) if score_ed_inserting(state)

  state = score_ed_pick(state, point, page)
  current = score_ed_selected(state)
  state["dragging"] = !current.nil?
  state["drag_saved"] = false
  state
end

def score_ed_inserting(state)
  state["tool"] == SCORE_ED_TOOLS[1]
end

# Insert mode: a press on the staff writes the pitch under the pointer
# where it lands. On a note — within a space of its head — the pitch joins
# it as a chord; on a rest, the rest becomes a note of its length; either
# way nothing moves in time. Between notes, a new one is written there, in
# the value chosen, before the first note to the right of the press (after
# the line's last when it is past them), and what follows moves on. The
# note written is selected; nothing is picked up: moving notes is the
# pointer's.
def score_ed_insert_at(state, point, page)
  state["dragging"] = false
  drawn = score_ed_engrave(state)
  return state if drawn["engraved"].nil?

  sp = drawn["sp"]
  xs = float(point[0]) / sp
  ys = float(point[1]) / sp
  systems = drawn["engraved"]["systems"].filter { |sy| (sy["page"] ?? 0) == page }
  return state if systems.length == 0

  best_sys = systems[0]
  for sys in systems
    best_sys = sys if (ys - sys["top"] - 2.0).abs < (ys - best_sys["top"] - 2.0).abs
  end
  pitch = score_ed_within(state, score_ed_pitch_at(state, score_ed_pos_at(ys, best_sys["top"])))
  map = drawn["map"]
  at = -1
  last = -1
  on = -1
  on_gap = 1.0
  for bar in best_sys["bars"]
    for ev in bar["items"]
      next if ev["i"] >= map.length

      owner = map[ev["i"]]
      next if owner < 0

      last = owner if owner > last
      mid = ev["x"] + (ev["head_w"] ?? 1.18) / 2.0
      at = owner if at < 0 && mid > xs
      if (xs - mid).abs <= on_gap
        on_gap = (xs - mid).abs
        on = owner
      end
    end
  end
  notes = state["notes"]
  return score_ed_insert_onto(state, on, pitch) if on >= 0 && on < notes.length
  return score_ed_write_at_tick(state, score_ed_tick_at(state, drawn, best_sys, xs), pitch) if state["voice"] == 2

  at = last + 1 if at < 0
  at = notes.length if at > notes.length
  state = score_ed_remember(state)
  fresh = score_ed_note(pitch["step"], pitch["oct"], pitch["alter"], state["dur"], state["dot"] ? 1 : 0)
  state["notes"] = notes.take(at) + [fresh] + range(at, notes.length).map { |k| notes[k] }
  state["sel"] = at
  state["typing"] = true
  score_ed_blip(state, fresh)
end

# The moment under a press, for the second voice: where it falls between
# two of the first voice's heads on that line, as that much of the time
# between them, rounded to the value chosen — so two eighths go under one
# quarter, the second a click past the middle of it. On a head, its note's
# start; past the line's last head, within its note.
def score_ed_tick_at(state, drawn, sys, xs)
  map1 = drawn["map1"] ?? drawn["map"]
  first = score_ed_v1(state)["notes"]
  starts = []
  t = 0
  for nt in first
    starts.push(t)
    t += score_ed_ticks(nt)
  end
  marks = []
  seen = {}
  for bar in sys["bars"]
    for ev in bar["items"]
      next if ev["i"] >= map1.length

      owner = map1[ev["i"]]
      next if owner < 0 || owner >= starts.length || seen.has_key(str(owner))

      seen[str(owner)] = true
      marks.push({"x": ev["x"] + (ev["head_w"] ?? 1.18) / 2.0, "tick": starts[owner], "owner": owner})
    end
  end
  return 0 if marks.length == 0

  k = 0
  for mk, j in marks
    k = j if mk["x"] <= xs
  end
  here = marks[k]
  return here["tick"] if xs <= here["x"]

  t0 = here["tick"]
  x0 = here["x"]
  grid = score_ed_ticks({"base": state["dur"], "dots": state["dot"] ? 1 : 0})
  t1 = t0
  x1 = x0
  top = t0
  if k + 1 < marks.length
    t1 = marks[k + 1]["tick"]
    x1 = marks[k + 1]["x"]
    top = t1
  else
    t1 = t0 + score_ed_ticks(first[here["owner"]])
    x1 = x0 + 4.0
    top = score_max(t0, t1 - grid)
  end
  return t0 if t1 <= t0 || x1 <= x0 || grid <= 0

  steps = int(Math.floor((xs - x0) / (x1 - x0) * (t1 - t0) / grid + 0.5))
  score_min(t0 + steps * grid, top)
end

# A note written into the second voice at `tick`, over what is there: the
# silence it lands in is cut to make room, a note it lands in is written
# over from its start. Past the voice's end, silence up to it first.
def score_ed_write_at_tick(state, tick, pitch)
  notes = state["notes"]
  start = 0
  k = 0
  while k < notes.length && start + score_ed_ticks(notes[k]) <= tick
    start += score_ed_ticks(notes[k])
    k += 1
  end
  value = {"base": state["dur"], "dots": state["dot"] ? 1 : 0}
  if k >= notes.length && tick > start
    state["notes"] = notes + score_ed_chunks(tick - start).map { |c| {"rest": true, "spacer": true, "pitches": [], "base": c[0], "dots": c[1], "tie": false} }
    k = state["notes"].length
  elsif k < notes.length && start < tick && notes[k]["rest"] && notes[k]["tup"].nil?
    was = notes[k]
    piece = fn(t) { score_ed_chunks(t).map { |c| {"rest": true, "spacer": was["spacer"] == true, "pitches": [], "base": c[0], "dots": c[1], "tie": false} } }
    before = piece(tick - start)
    after = piece(start + score_ed_ticks(was) - tick)
    state["notes"] = notes.take(k) + before + after + range(k + 1, notes.length).map { |j| notes[j] }
    k += before.length
  end
  score_ed_write_over(state, k, pitch, value)
end

# The pitch written onto note `k`. In the value the note has: a rest is
# filled, a note made a chord (a pitch it already has only selects it). In
# another value, a note of the first voice stays: the new one goes into the
# second voice from the moment the note starts, and editing carries on
# there — an eighth on a quarter sounds with it, stem down. Anything else
# in another value is written over, as an engraver writes over: an eighth
# on a quarter rest takes its first half and a rest the second; a half on
# a quarter takes it and the next quarter. What it covers goes, what it
# leaves of the last note it reaches is a rest, and nothing after moves in
# time.
def score_ed_insert_onto(state, k, pitch)
  target = state["notes"][k]
  value = {"base": state["dur"], "dots": state["dot"] ? 1 : 0}
  if score_ed_ticks(value) != score_ed_ticks(target)
    return score_ed_write_over(state, k, pitch, value) if target["rest"] || (state["voice"] ?? 1) == 2

    tick = state["notes"].take(k).reduce(fn(acc, nt) acc + score_ed_ticks(nt), 0)
    return score_ed_write_at_tick(score_ed_set_voice(state, 2), tick, pitch)
  end

  want = pitch["oct"] * 7 + pitch["step"]
  had = !target["rest"] && target["pitches"].any? { |pt| pt["oct"] * 7 + pt["step"] == want }
  state["sel"] = k
  state["typing"] = false
  return state if had

  state = score_ed_remember(state)
  target = state["notes"][k]
  if target["rest"]
    target["rest"] = false
    target["pitches"] = [pitch]
    # A rest the editor put there to fill a bar is the person's note now,
    # and no longer one a trim may take away.
    target.delete("auto") if target.has_key("auto")
  else
    target["pitches"] = (target["pitches"] + [pitch]).sort_by { |pt| pt["oct"] * 7 + pt["step"] }
  end
  score_ed_blip(state, target)
end

def score_ed_write_over(state, k, pitch, value)
  notes = state["notes"]
  need = score_ed_ticks(value)
  covered = 0
  stop = k
  while stop < notes.length && covered < need
    if !notes[stop]["tup"].nil?
      state["error"] = "A note written over a tuplet would break it: change the tuplet's notes one by one."
      return state
    end
    covered += score_ed_ticks(notes[stop])
    stop += 1
  end
  state = score_ed_remember(state)
  notes = state["notes"]
  # Past the end (a second voice written further than it went) there is
  # nothing under the note to carry over.
  first = k < notes.length ? notes[k] : {}
  last = stop > k ? notes[stop - 1] : {}
  fresh = score_ed_note(pitch["step"], pitch["oct"], pitch["alter"], value["base"], value["dots"])
  # What belongs to the moment the note starts stays with it: a change of
  # key or metre, a repeat, a dynamic, words.
  for field in ["set", "rep_start", "dyn", "texts"]
    fresh[field] = first[field] if first.has_key(field) && !first[field].nil?
  end
  quiet = range(k, stop).all? { |j| notes[j]["spacer"] == true }
  rests = score_ed_chunks(score_max(0, covered - need)).map { |c| {"rest": true, "spacer": quiet && stop > k, "pitches": [], "base": c[0], "dots": c[1], "tie": false} }
  written = [fresh] + rests
  # And what belongs to where the last one ended: a repeat's end, a break.
  for field in ["rep_end", "brk"]
    written[-1][field] = last[field] if last.has_key(field) && !last[field].nil?
  end
  state["notes"] = notes.take(k) + written + range(stop, notes.length).map { |j| notes[j] }
  state["sel"] = k
  state["typing"] = false
  score_ed_blip(state, fresh)
end

# The pointer moved with the button down: the held note goes to the line
# or space under it, on the staff it was picked up from. Only a new
# position changes anything — the engraving is redrawn and the note
# sounds once per line or space crossed, not once per sample. A drag is
# one step to undo, however far it went.
def score_ed_drag(state, point)
  return state unless state["dragging"] == true

  current = score_ed_selected(state)
  return state if current.nil? || state["drag_top"].nil?

  sp = score_ed_engrave(state)["sp"]
  pitch = score_ed_pitch_at(state, score_ed_pos_at(float(point[1]) / sp, state["drag_top"]))
  want = pitch["oct"] * 7 + pitch["step"]
  return state if !current["rest"] && current["pitches"].any? { |pt| pt["oct"] * 7 + pt["step"] == want }

  state = score_ed_remember(state) unless state["drag_saved"] == true
  state["drag_saved"] = true
  score_ed_place_pitch(state, pitch)
end

# The pitch a staff position stands for under the clef, altered as the key
# alters it.
def score_ed_pitch_at(state, pos)
  d = pos.clamp(0 - score_ed_reach(), score_ed_reach()) + score_clef_mid(score_ed_clef(state))
  step = score_mod(d, 7)
  octave = (d - step) / 7 + 3
  {"step": step, "oct": octave, "alter": score_ed_key_alter(state, step)}
end

# A second click on the selection moves it there; a rest becomes a note.
# A chord moves the head nearest the pointer.
def score_ed_set_pitch(state, pitch)
  score_ed_place_pitch(score_ed_remember(state), pitch)
end

def score_ed_place_pitch(state, pitch)
  current = score_ed_selected(state)
  if current["rest"]
    current["rest"] = false
    current["pitches"] = [pitch]
    return state
  end
  want = pitch["oct"] * 7 + pitch["step"]
  nearest = 0
  for pt, k in current["pitches"]
    here = pt["oct"] * 7 + pt["step"]
    there = current["pitches"][nearest]["oct"] * 7 + current["pitches"][nearest]["step"]
    nearest = k if (here - want).abs < (there - want).abs
  end
  current["pitches"][nearest] = pitch
  current["pitches"] = current["pitches"].sort_by { |pt| pt["oct"] * 7 + pt["step"] }
  score_ed_blip(state, current)
end

# ---- From the model to LilyPond, and back ----------------------------------

def score_ed_clef(state)
  {"Treble": "treble", "Bass": "bass", "Alto": "alto"}[state["clef"]] ?? "treble"
end

def score_ed_bar_ticks(state)
  score_ed_measure_of(state["time"])
end

# How long a note lasts, in the engraver's ticks: in a tuplet, d/n of it.
def score_ed_ticks(nt)
  t = score_whole() / nt["base"]
  t += nt["dots"] > 0 ? t / 2 : 0
  tup = nt["tup"]
  tup.nil? ? t : t * tup["d"] / tup["n"]
end

# A length in ticks as the fewest written values: the longest that fits,
# dotted or not, then the rest of it. 192 is a dotted half; 160 is a half
# tied to an eighth.
def score_ed_chunks(ticks)
  out = []
  left = ticks
  while left >= score_whole() / 32
    best = nil
    for base in [1, 2, 4, 8, 16, 32]
      t = score_whole() / base
      if best.nil? && base < 32 && t + t / 2 <= left
        best = [base, 1]
      end
      if best.nil? && t <= left
        best = [base, 0]
      end
    end
    out.push(best)
    t = score_whole() / best[0]
    left -= best[1] > 0 ? t + t / 2 : t
  end
  out
end

def score_ed_pitch_ly(pt)
  name = "cdefgab".chars()[pt["step"]]
  alter = pt["alter"]
  suffix = ""
  suffix = "is" if alter == 1
  suffix = "isis" if alter == 2
  if alter < 0
    suffix = alter == -1 ? "es" : "eses"
    suffix = alter == -1 ? "s" : "ses" if name == "e" || name == "a"
  end
  marks = pt["oct"] - 3
  out = name + suffix
  out += "'" * marks if marks > 0
  out += "," * -marks if marks < 0
  out
end

# The model written out: the source, and for every item the engraver will
# number, the note of the model it came from. A note longer than what is
# left of its bar is cut there and tied over.
# The source the sheet engraves: the piece, or with several tracks the one
# being edited, alone on its staff with the bars' marks of the first.
def score_ed_layout(state)
  base = score_ed_v1(state)
  voiced = score_ed_voiced?(base["notes2"])
  base = base.merge({"no_gather": true}) if voiced
  info = score_ed_bar_marks(base)
  laid = nil
  if score_ed_tracks(state).length < 2
    laid = score_ed_layout_one(base)
  else
    laid = score_ed_layout_track(base, state["track"] ?? 0, info, {"header": true, "tempo": true, "gather": !voiced})
  end
  both = score_ed_with_voice_two(base, laid, base["notes2"], info, score_ed_tracks(state).length > 1)
  # `map` is the voice being edited's — what a click picks — and `map1`
  # the first voice's, what the playhead lights.
  src = score_ed_with_lyrics(both["words"], score_ed_lyrics_ly(score_ed_tracks(state)[state["track"] ?? 0]["lyrics"] ?? ""))
  {"src": src, "map": state["voice"] == 2 ? both["map2"] : both["map"], "map1": both["map"]}
end

# A part with lyrics: its music in braces, the titles before them, and
# `\\addlyrics { … }` after — LilyPond reads lyrics only after a block.
def score_ed_with_lyrics(words, lyrics)
  return words.join(" ") if lyrics == ""

  titled = words.length > 0 && words[0].starts_with?("\\header")
  head = titled ? [words[0]] : []
  body = titled ? words.drop(1) : words
  (head + ["{"] + body + ["}", "\\addlyrics {", lyrics, "}"]).join(" ")
end

# A part's music with its second voice, when it has one: the head (titles,
# clef, key, time) and then `<< { first } \\\\ { second } >>`. `map` is
# the first voice's notes item by item and `map2` the second's, each -1
# where the other voice's are.
def score_ed_with_voice_two(base, laid, notes2, info, ensemble)
  map1 = laid["map"]
  unless score_ed_voiced?(notes2)
    return {"words": laid["words"], "map": map1, "map2": map1.map { |o| -1 }}
  end
  total = base["notes"].reduce(fn(acc, nt) acc + score_ed_ticks(nt), 0)
  total = score_max(total, score_ed_ensemble_end(base, info)) if ensemble
  two = score_ed_voice_two_ly(notes2, info, total)
  words = laid["words"]
  head = laid["head"]
  out = words.take(head) + ["<< {"] + range(head, words.length).map { |k| words[k] } + ["} \\\\ {"] + two["words"] + ["} >>"]
  {"words": out, "map": map1 + two["map"].map { |o| -1 }, "map2": map1.map { |o| -1 } + two["map"]}
end

# The second voice as LilyPond: cut at the first voice's bar lines and
# tied over them, its silences spacers (`s`), filled with them to the
# first voice's end and cut there. `map`: the note behind each item.
def score_ed_voice_two_ly(notes2, info, total)
  words = []
  owners = []
  have = notes2.reduce(fn(acc, nt) acc + score_ed_ticks(nt), 0)
  notes = notes2
  notes = notes + [ {"rest": true, "spacer": true, "pitches": [], "base": 1, "dots": 0, "pad": total - have} ] if have < total
  tick = 0
  bar = 0
  for nt, ni in notes
    break if tick >= total

    left = score_min(nt["pad"] ?? score_ed_ticks(nt), total - tick)
    full = left
    tup = nt["tup"]
    tup_opens = !tup.nil? && (ni == 0 || notes[ni - 1]["tup"].nil? || notes[ni - 1]["tup"]["g"] != tup["g"])
    tup_closes = !tup.nil? && (ni + 1 >= notes.length || notes[ni + 1]["tup"].nil? || notes[ni + 1]["tup"]["g"] != tup["g"])
    words.push("\\tuplet " + str(tup["n"]) + "/" + str(tup["d"]) + " {") if tup_opens
    while left > 0
      room = score_ed_bar_tick(info, bar + 1) - tick
      take = tup.nil? ? score_min(left, room) : left
      chunks = tup.nil? ? score_ed_chunks(take) : [ [nt["base"], nt["dots"]] ]
      for ch, ci in chunks
        last_piece = left - take == 0 && ci == chunks.length - 1
        word = nt["spacer"] == true ? "s" : (nt["rest"] ? "r" : score_ed_chord_ly(nt))
        word += str(ch[0]) + (ch[1] > 0 ? "." : "")
        word += " ~" if !nt["rest"] && (!last_piece || nt["tie"] == true)
        word += "(" if nt["slur_on"] == true && left == full && ci == 0
        word += ")" if nt["slur_off"] == true && last_piece
        word += score_ed_expression_ly(nt, left == full && ci == 0, last_piece) unless nt.has_key("pad") || nt["spacer"] == true
        words.push(word)
        owners.push(nt.has_key("pad") ? -1 : ni)
      end
      words.push("}") if tup_closes
      tick += take
      left -= take
      while tick >= score_ed_bar_tick(info, bar + 1)
        bar += 1
        words.push("|")
      end
    end
  end
  {"words": words, "map": owners}
end

# A track's music for the full score and the files, its second voice with it.
def score_ed_track_music(state, k, info, opts)
  base = score_ed_v1(state)
  trk = score_ed_all(base)[k]
  voiced = score_ed_voiced?(trk["notes2"])
  laid = score_ed_layout_track(base, k, info, voiced ? opts.merge({"gather": false}) : opts)
  both = score_ed_with_voice_two(base.merge({"notes": trk["notes"]}), laid, trk["notes2"], info, true)
  {"words": both["words"], "map": both["map"]}
end

def score_ed_layout_one(state)
  measure = score_ed_bar_ticks(state)
  fifths = score_ed_fifths(state)
  head_ly = "\\clef " + score_ed_clef(state) + " \\key " + SCORE_ED_KEY_LY[fifths + 7] + " \\major \\time " + state["time"]
  words = []
  titles = score_ed_header_ly(state["header"] ?? {})
  words.push(titles) if titles != ""
  words.push(head_ly)
  head_at = words.length
  tempo_ly = score_ed_tempo_ly(state["tempo_text"] ?? "", state["tempo"], state["mark_tempo"] != false, state["tempo_base"] ?? 4)
  words.push(tempo_ly) if tempo_ly != ""
  owners = []
  pos = 0
  # The bar line last written, so a start repeat on the next note can turn
  # it into one; and an end repeat waiting for the line its bar ends on.
  bar_at = -1
  closing = false
  breaking = nil
  opening = false
  gather_from = -1
  gathered = false
  # A change is written where its bar starts. One on a note that no longer
  # starts a bar — a note before it got longer — waits for the next bar
  # line rather than break the engraving.
  waiting = []
  waiting_time = nil
  for nt, ni in state["notes"]
    next if ni < gather_from

    changes = score_ed_changes_ly(nt["set"] ?? {})
    opening = true if nt["rep_start"] == true
    if opening && pos == 0 && bar_at >= 0 && bar_at == words.length - 1
      words[bar_at] = score_ed_bar_ly(words[bar_at], "|:")
      opening = false
    elsif opening && pos == 0 && owners.length == 0
      # A repeat from the very start: no bar line comes before it to carry
      # the sign, so it is written on its own, and drawn after the clef,
      # key and time as LilyPond draws it.
      words.push("\\bar \".|:\"")
      opening = false
    end
    if changes["words"].length > 0
      if pos == 0
        words = words + changes["words"]
        measure = score_ed_measure_of(changes["time"]) unless changes["time"].nil?
      else
        waiting = waiting + changes["words"]
        waiting_time = changes["time"] unless changes["time"].nil?
      end
    end
    # Bars of rest in a row, written as one: R1*4.
    run = score_ed_rest_run(state, ni, pos, measure)
    if run > 1
      words.push("R" + score_ed_chunks(measure).map { |ch| str(ch[0]) + (ch[1] > 0 ? "." : "") }.join("") + "*" + str(run) + score_ed_expression_ly(nt, true, true))
      owners.push(ni)
      gather_from = ni + run
      gathered = true
      words.push(state["notes"][ni + run - 1]["rep_end"] == true ? "\\bar \":|.\"" : "|")
      bar_at = words.length - 1
      brk = state["notes"][ni + run - 1]["brk"]
      words.push(brk == "page" ? "\\pageBreak" : "\\break") if !brk.nil? && ni + run < state["notes"].length
      closing = false
      next
    end
    left = score_ed_ticks(nt)
    # A tuplet's note is written whole, inside its `\tuplet n/d { … }`.
    tup = nt["tup"]
    notes_all = state["notes"]
    tup_opens = !tup.nil? && (ni == 0 || notes_all[ni - 1]["tup"].nil? || notes_all[ni - 1]["tup"]["g"] != tup["g"])
    tup_closes = !tup.nil? && (ni + 1 >= notes_all.length || notes_all[ni + 1]["tup"].nil? || notes_all[ni + 1]["tup"]["g"] != tup["g"])
    words.push("\\tuplet " + str(tup["n"]) + "/" + str(tup["d"]) + " {") if tup_opens
    while left > 0
      take = tup.nil? ? score_min(left, measure - pos) : left
      chunks = tup.nil? ? score_ed_chunks(take) : [ [nt["base"], nt["dots"]] ]
      for ch, ci in chunks
        last_piece = left - take == 0 && ci == chunks.length - 1
        word = nt["rest"] ? "r" : score_ed_chord_ly(nt)
        word += str(ch[0]) + (ch[1] > 0 ? "." : "")
        word += " ~" if !nt["rest"] && (!last_piece || nt["tie"] == true)
        word += "(" if nt["slur_on"] == true && left == score_ed_ticks(nt) && ci == 0
        word += ")" if nt["slur_off"] == true && last_piece
        word += score_ed_expression_ly(nt, left == score_ed_ticks(nt) && ci == 0, last_piece)
        words.push(word)
        owners.push(ni)
      end
      words.push("}") if tup_closes
      pos += take
      left -= take
      closing = true if nt["rep_end"] == true && left == 0
      breaking = nt["brk"] if !nt["brk"].nil? && left == 0
      if pos == measure
        words.push(closing ? "\\bar \":|.\"" : "|")
        bar_at = words.length - 1
        closing = false
        pos = 0
        if !breaking.nil? && ni + 1 < state["notes"].length
          words.push(breaking == "page" ? "\\pageBreak" : "\\break")
        end
        breaking = nil
        if waiting.length > 0
          words = words + waiting
          measure = score_ed_measure_of(waiting_time) unless waiting_time.nil?
          waiting = []
          waiting_time = nil
        end
      end
    end
  end
  if state["notes"].length > 0 && pos == 0 && bar_at == words.length - 1
    # The last bar ended on its own line: that line becomes the final one.
    words[bar_at] = "\\bar \"|.\"" if words[bar_at] == "|"
  elsif state["notes"].length > 0
    words.push(closing ? "\\bar \":|.\"" : "\\bar \"|.\"")
  end
  words = range(0, head_at).map { |k| words[k] } + ["\\compressEmptyMeasures"] + range(head_at, words.length).map { |k| words[k] } if gathered
  {"src": words.join(" "), "map": owners, "words": words, "head": head_at}
end

# A bar line with a repeat sign added to it: `|:` opens one after it.
def score_ed_bar_ly(word, sign)
  return "\\bar \":|.|:\"" if word == "\\bar \":|.\""

  "\\bar \".|:\""
end

# How many bars of rest start at note `ni`: whole-bar rests from the bar
# line, with nothing written on the ones after the first — no change, no
# repeat sign — so that one bar can stand for them all. One that LilyPond
# could not write as a single value (a bar of 5/4) is never gathered.
def score_ed_rest_run(state, ni, pos, measure)
  return 0 if pos != 0 || score_ed_chunks(measure).length != 1 || state["no_gather"] == true

  run = 0
  k = ni
  while k < state["notes"].length
    nt = state["notes"][k]
    break unless nt["rest"] && score_ed_ticks(nt) == measure
    break if k > ni && ((nt["set"] ?? {}).keys.length > 0 || nt["rep_start"] == true || !nt["dyn"].nil? || !nt["hairpin"].nil? || nt["hairpin_end"] == true || (nt["texts"] ?? []).length > 0)
    break if k > ni && state["notes"][k - 1]["rep_end"] == true

    run += 1
    k += 1
  end
  run
end

# `\header { title = "…" … }` from the fields that are filled in, in
# LilyPond's order; nothing when none is.
def score_ed_header_ly(hdr)
  fields = []
  for name in ["dedication", "title", "subtitle", "subsubtitle", "poet", "instrument", "composer", "meter", "arranger"]
    value = score_ed_clean(hdr[name] ?? "")
    fields.push(name + " = \"" + value + "\"") if value != ""
  end
  return "" if fields.length == 0

  "\\header { " + fields.join(" ") + " }"
end

# `\tempo "Allegro" 4 = 120`, or as much of it as there is to print.
def score_ed_tempo_ly(typed, bpm, printed, beat_base)
  tempo_words = score_ed_clean(typed ?? "")
  return "" if tempo_words == "" && !printed
  return "\\tempo \"" + tempo_words + "\"" if !printed

  beat = str(beat_base) + " = " + str(bpm)
  return "\\tempo " + beat if tempo_words == ""

  "\\tempo \"" + tempo_words + "\" " + beat
end

# A note's changes as LilyPond: `\key`, `\time`, `\tempo`.
def score_ed_changes_ly(changes)
  out = []
  time = nil
  if changes.has_key("key")
    out.push("\\key " + SCORE_ED_KEY_LY[score_ed_fifths_of(changes["key"]) + 7] + " \\major")
  end
  if changes.has_key("time")
    out.push("\\time " + changes["time"])
    time = changes["time"]
  end
  if changes.has_key("tempo")
    out.push(score_ed_tempo_ly(changes["tempo"]["text"], changes["tempo"]["bpm"], true, changes["tempo"]["base"] ?? 4))
  end
  {"words": out, "time": time}
end

def score_ed_chord_ly(nt)
  return score_ed_pitch_ly(nt["pitches"][0]) if nt["pitches"].length == 1

  "<" + nt["pitches"].map { |pt| score_ed_pitch_ly(pt) }.join(" ") + ">"
end

# Pasted or edited LilyPond, read back into the model. What the engraver
# refuses is shown and nothing changes.
def score_ed_from_source(state, src)
  current = score_ed_full_source(state)
  return state if src.trim == current.trim

  parsed = nil
  problem = ""
  try
    parsed = score_parse(src)
    score_measures(parsed["events"])
  catch e
    problem = str(e)
  end
  if problem == "" && parsed["staves"].length > 1
    try
      score_layout(src, 100.0, {})
    catch e
      problem = str(e)
    end
  end
  if problem != ""
    state["error"] = problem.replace("score: ", "")
    return state
  end
  state = score_ed_remember_all(state)
  notes = []
  hdr = {}
  for name in SCORE_ED_HEADER
    hdr[name] = parsed["header"][name] if parsed["header"].has_key(name)
  end
  state["header"] = hdr
  # Several staves are several tracks: each staff's notes, clef, name and
  # MIDI settings; what belongs to the bar is read from the first.
  if parsed["staves"].length > 1
    tracks = []
    for stf, k in parsed["staves"]
      got = score_ed_import(state, stf["events"], k == 0)
      props = stf["props"] ?? {}
      trk = score_ed_track_new(props["instrumentName"] ?? ("Track " + str(k + 1)))
      trk["short"] = props["shortInstrumentName"] ?? ""
      trk["instrument"] = props["midiInstrument"] ?? "acoustic grand"
      trk["volume"] = int((props["midiMaximumVolume"] ?? (100.0 / 127.0)) * 127.0 + 0.5)
      pan = props["midiPanPosition"] ?? 0.0
      trk["pan"] = int(pan * 100.0 + (pan < 0.0 ? -0.5 : 0.5))
      trk["notes"] = got["notes"]
      trk["clef"] = got["clef"]
      trk["lyrics"] = score_ed_lyrics_text(stf["lyrics"])
      tracks.push(trk)
    end
    kind = parsed["groups"].length > 0 ? parsed["groups"][0]["kind"] : "none"
    state["group"] = {"brace": "Brace", "bracket": "Bracket", "choir": "Bracket", "none": "None"}[kind] ?? "Bracket"
    state["tracks"] = tracks
    state["track"] = 0
    state["notes"] = tracks[0]["notes"]
    state["clef"] = tracks[0]["clef"]
    tracks[0]["notes"] = nil
    tracks[0]["clef"] = nil
    state["sel"] = state["notes"].length > 0 ? 0 : -1
    return state
  end
  got = score_ed_import(state, parsed["events"], true)
  state["tracks"] = [score_ed_track_new("")]
  state["tracks"][0]["lyrics"] = score_ed_lyrics_text(parsed["staves"][0]["lyrics"])
  state["track"] = 0
  state["clef"] = got["clef"]
  state["notes"] = got["notes"]
  state["sel"] = got["notes"].length > 0 ? 0 : -1
  state
end

# One staff's events as the model's notes, and its clef. The first staff
# also gives the piece its key, metre and tempo and the bars their changes,
# repeats and breaks; another's are the first's already, and are not kept.
def score_ed_import(state, events, first)
  notes = []
  clef = "Treble"
  # Settings before the first note are the piece's; after it, they are
  # changes, written on the note that follows them.
  pending = {}
  # Repeats come back as the two signs; the endings of an \alternative
  # come back as the notes they hold, without their brackets.
  rep_next = false
  for ev in events
    if first && (ev["k"] == "repeat_start" || (ev["k"] == "bar" && ev["v"].ends_with?("|:")))
      rep_next = true
    end
    if first && (ev["k"] == "repeat_end" || (ev["k"] == "bar" && ev["v"].starts_with?(":|"))) && notes.length > 0
      notes[-1]["rep_end"] = true
    end
    notes[-1]["brk"] = ev["page"] ? "page" : "line" if first && ev["k"] == "break" && notes.length > 0
    if ev["k"] == "clef"
      clef = {"treble": "Treble", "bass": "Bass", "alto": "Alto"}[ev["v"]] if notes.length == 0
    elsif !first && (ev["k"] == "key" || ev["k"] == "time" || ev["k"] == "tempo")
      next
    elsif ev["k"] == "key"
      key_name = SCORE_ED_KEYS[ev["fifths"] + 7]
      if notes.length == 0
        state["key"] = key_name
      else
        pending["key"] = key_name
      end
    elsif ev["k"] == "time"
      label = str(ev["n"]) + "/" + str(ev["d"])
      if SCORE_ED_TIMES.includes?(label)
        if notes.length == 0
          state["time"] = label
        else
          pending["time"] = label
        end
      end
    elsif ev["k"] == "tempo"
      bpm = ev["bpm"] > 0 ? ev["bpm"] : state["tempo"]
      beat_base = ev["base"] > 0 ? ev["base"] : 4
      if notes.length == 0
        state["tempo"] = bpm
        state["tempo_base"] = beat_base
        state["tempo_text"] = ev["text"]
        state["mark_tempo"] = ev["bpm"] > 0
      else
        pending["tempo"] = {"bpm": bpm, "text": ev["text"], "base": beat_base}
      end
    elsif ev["k"] == "note" || ev["k"] == "rest"
      if pending.keys.length > 0
        ev["set"] = pending
        pending = {}
      end
      fresh = {
        "rest": ev["k"] == "rest",
        "pitches": ev["pitches"].map { |pt| {"step": score_mod(pt["d"], 7), "oct": (pt["d"] - score_mod(pt["d"], 7)) / 7 + 3, "alter": pt["alter"]} },
        "base": ev["base"],
        "dots": score_min(ev["dots"], 1),
        "tie": ev["tie"] == true,
        "slur_on": ev["slur_on"] == true,
        "slur_off": ev["slur_off"] == true,
        "set": ev["set"] ?? {}
      }
      fresh["rep_start"] = true if rep_next
      fresh["dyn"] = ev["dyn"] if SCORE_ED_DYNAMICS.includes?(ev["dyn"] ?? "")
      fresh["hairpin"] = ev["hairpin"] unless ev["hairpin"].nil?
      fresh["hairpin_end"] = true if ev["hairpin_end"] == true
      fresh["scripts"] = (ev["scripts"] ?? []).map { |sc| {"name": sc["name"], "dir": sc["dir"]} } unless ev["scripts"].nil?
      fresh["texts"] = ev["texts"] unless ev["texts"].nil?
      unless ev["tuplet"].nil?
        fresh["tup"] = {"g": ev["tuplet"]["id"], "n": ev["tuplet"]["n"], "d": ev["tuplet"]["d"]}
        state["tup_seq"] = score_max(state["tup_seq"] ?? 0, ev["tuplet"]["id"])
      end
      rep_next = false
      notes.push(fresh)
      # R1*4 is four bars of rest in the model; the writer gathers them again.
      for _ in range(1, ev["count"] ?? 1)
        notes.push(fresh.merge({"set": {}, "rep_start": false}))
      end
    end
  end
  {"notes": notes, "clef": clef}
end

# ---- The view --------------------------------------------------------------

# The sheet's width: the window's, less the toolbar on the left, the main
# column's padding and a card's, and a scroll bar's room.
def score_ed_width(state)
  vw = (state["viewport"] ?? {})["width"] ?? 1100
  score_max(score_min(vw - score_ed_rail_w() - 1 - 2 * 24 - 2 * 16 - 2 - 14, 1180), 300)
end

# The toolbar down the left side, px wide.
def score_ed_rail_w()
  212
end

# The bar across the top — the editor's name, Play, Undo, and the title
# page's fields — px tall, its rule included.
def score_ed_top_h()
  132
end

# What is left of the window under the top bar: the toolbar and the score
# are each that tall and scroll on their own, so neither the bar nor the
# toolbar ever scrolls away.
def score_ed_body_h(state)
  vh = (state["viewport"] ?? {})["height"] ?? 860
  score_max(vh - score_ed_top_h(), 240)
end

# A staff space of 12 px on a desk: big enough to aim a click at a line or
# a space, which is what writing with the mouse is.
# The full score's staff space: 65 % of a part's, so a dozen staves read as
# a conductor's page rather than a scroll of them.
def score_ed_full_space(state)
  score_ed_space(state) * 0.65
end

def score_ed_space(state)
  width = score_ed_width(state)
  return 12 if width >= 900
  return 10 if width >= 560

  8
end

# On a screen: one tall sheet at the editor's staff size. On A4: LilyPond's
# page, as many as the music takes, scaled to the editor's width.
def score_ed_engrave(state)
  laid = score_ed_layout(state)
  a4 = state["view"] == SCORE_ED_VIEWS[1]
  sp = score_ed_space(state)
  engraved = nil
  problem = ""
  try
    if a4
      sheets = score_pages(laid["src"], {"width": score_ed_page_width(state)})
      engraved = sheets["engraved"]
      sp = sheets["sp"]
    else
      engraved = score_layout(laid["src"], float(score_ed_width(state)) / sp, {"page_h": score_ed_screen_page() / sp})
    end
  catch e
    problem = str(e)
  end
  {"engraved": engraved, "map": laid["map"], "map1": laid["map1"] ?? laid["map"], "src": laid["src"], "problem": problem, "sp": sp, "a4": a4}
end

def score_ed_page_width(state)
  score_min(score_ed_width(state), 820)
end

def score_editor_view(raw_state)
  state = score_ed_defaults(raw_state ?? {})
  drawn = score_ed_engrave(state)
  vw = (state["viewport"] ?? {})["width"] ?? 1100
  body_h = score_ed_body_h(state)
  inner = column({"pad": 7, "gap": 5, "width": "100%"}, [
    score_ed_panels(state),
    score_ed_doc_bar(state),
    score_ed_track_bar(state),
    score_ed_full_view(state) ? score_ed_conductor_card(state) : score_ed_sheet(state, drawn),
    score_ed_status(state),
    score_ed_source(state, drawn),
    score_ed_help(state)
  ] + score_ed_sounds(state))
  # Only the score scrolls: the bar over it and the toolbar beside it stay
  # where they are. EUI has no `position: fixed` — the frame is a column of
  # the bar and a row of exactly the height left, its two halves scrolling
  # each on its own.
  main = scroll({"height": body_h, "width": score_max(vw - score_ed_rail_w() - 1, 300), "bg": "surface.base"}, [inner])
  main["key"] = "score_page"
  body = row({"width": "100%", "height": body_h}, [
    score_ed_palette(state, body_h),
    {"k": "box", "s": {"width": 1, "height": body_h, "bg": "border.subtle"}},
    main
  ])
  page = column({"width": "100%", "height": "100%", "bg": "surface.base"}, [score_ed_header(state), body])
  # The keys reach the editor wherever focus is — on the staff, or on a
  # palette button just pressed — but never from inside a field, whose own
  # typing the client keeps (03 §3.1).
  # A `midi` handler joins these once soli speaks EUI 9 (06 §1.4): a
  # server that does not know the event refuses the whole view.
  page["on"] = {"key_down": "key"}
  page
end

# The bar across the top, which never scrolls: the editor's name, Play and
# Undo on one line, and under them the title page's fields.
def score_ed_header(state)
  undo_count = (state["undo"] ?? []).length
  redo_count = (state["redo"] ?? []).length
  bar = column({"gap": 3, "pad": [4, 7, 4, 7], "width": "100%", "height": score_ed_top_h() - 1, "bg": "surface.raised"}, [
    row({"gap": 3, "align": "center", "width": "100%"}, [
      text("Score editor", {"size": 4, "weight": "semibold"}),
      {"k": "box", "s": {"grow": 1}},
      score_ed_play_button(state),
      score_ed_tool(state["sound"] == true ? "Hear notes: on" : "Hear notes: off", "sound", {}, false, nil),
      score_ed_rule(),
      score_ed_tool("Undo", "undo", {}, undo_count == 0, "Ctrl+Z"),
      score_ed_tool("Redo", "redo", {}, redo_count == 0, "Ctrl+Y")
    ]),
    score_ed_titles(state)
  ])
  column({"width": "100%"}, [bar, {"k": "box", "s": {"width": "100%", "height": 1, "bg": "border.subtle"}}])
end

# Over the score: how it is shown, and the whole-piece actions.
def score_ed_doc_bar(state)
  row({"gap": 3, "align": "center", "width": "100%", "wrap": "wrap"}, [
    score_ed_labelled("View", segmented(SCORE_ED_VIEWS, state["view"], "view")),
    {"k": "box", "s": {"grow": 1}},
    score_ed_save_button("Download PDF", "pdf_save", score_ed_file_name(state, "pdf"), state["notes"].length == 0),
    score_ed_save_button("Download MIDI", "midi_save", score_ed_file_name(state, "mid"), state["notes"].length == 0),
    select_sized(SCORE_ED_EXAMPLES, "Examples…", state["ex_open"] == true, "ex_open", "example", 150, false, {"key": "score_examples"}),
    score_ed_tool("Clear", "clear", {}, state["notes"].length == 0, nil)
  ])
end

# A button that offers a file: pressing it opens the platform's save dialog
# (spec 03 §3.2), and the place the person chooses comes back as a
# `file_save` event, which the handler answers with the bytes.
def score_ed_save_button(label, on_save, name, disabled)
  button = score_ed_tool(label, on_save, {}, disabled, nil)
  kept = {}
  for kind in (button["on"] ?? {}).keys
    kept[kind] = button["on"][kind] unless kind == "click"
  end
  kept["file_save"] = on_save
  button["on"] = kept
  button["p"] = (button["p"] ?? {}).merge({"save": name}) unless disabled
  button
end

# The file's name: the title, without what a file system refuses (and its
# dots), spaces as dashes, or "score".
def score_ed_file_name(state, ext)
  title = ((state["header"] ?? {})["title"] ?? "").trim
  clean = title.chars().map { |ch| "/\\:*?\"<>|.".contains(ch) ? "" : (ch == " " ? "-" : ch) }.join("")
  (clean == "" ? "score" : clean) + "." + ext
end

# A thin upright rule between groups of controls.
def score_ed_rule()
  {"k": "box", "s": {"width": 1, "height": 24, "bg": "border.subtle"}}
end

# What is not written note by note, in tabs so the window stays readable as
# the editor grows: the piece itself (its titles, clef, key, metre and
# tempo), and its bars (repeats). The open tab pressed again folds them.
def score_ed_panels(state)
  # The active tab pressed again folds the panel away: "none".
  panel = state["panel"] ?? SCORE_ED_PANELS[0]
  body = []
  if panel == SCORE_ED_PANELS[0]
    body = [score_ed_settings_bar(state)]
  elsif panel == SCORE_ED_PANELS[1]
    body = [score_ed_bars_panel(state)]
  elsif panel == SCORE_ED_PANELS[2]
    body = [score_ed_expression_panel(state)]
  elsif panel == SCORE_ED_PANELS[3]
    body = [score_ed_lyrics_panel(state)]
  elsif panel == SCORE_ED_PANELS[4]
    body = [score_ed_tracks_panel(state)]
  end
  card({"gap": 4, "pad": [2, 5, body.length > 0 ? 5 : 2, 5], "width": "100%"}, [tabs(SCORE_ED_PANELS, panel, "panel", {"key": "score_panels", "size": "sm"})] + body)
end

# The expression tab: on the selected note, a dynamic, a hairpin, its
# articulations, and words over or under it. What the note has is lit.
# A hairpin's button: lit while one of its kind is armed, and pressable
# then with no note, to drop it.
def score_ed_pin_button(state, grow, label, no_note)
  armed = (state["pin_from"] ?? -1) >= 0 && state["pin_grow"] == grow
  return score_ed_tool(label + "  " + grow, "hairpin", {"grow": grow}, no_note, grow) unless armed

  control({
    "key": "tool:hairpin:" + label,
    "tone": "accent",
    "size": "sm",
    "on": {"click": "hairpin"},
    "props": {"grow": grow},
    "shape": {"gap": 2, "pad": [1, 3, 1, 3]},
    "a11y": {"role": "button", "label": label + ": armed — press on the last note to end it", "pressed": true},
    "c": [text(label + " ●", {"size": 1, "weight": "medium", "fg": "accent.on"}), kbd(grow)]
  })
end

def score_ed_expression_panel(state)
  current = score_ed_selected(state)
  none = current.nil?
  dyns = SCORE_ED_DYNAMICS.map { |dyn| score_ed_glyph_button("dyn:" + dyn, "dyn", {"dyn": dyn}, score_ed_dyn_icon(dyn), !none && current["dyn"] == dyn, "", dyn) }
  armed = (state["pin_from"] ?? -1) >= 0
  pins = [
    score_ed_pin_button(state, "<", "Crescendo", none),
    score_ed_pin_button(state, ">", "Diminuendo", none)
  ]
  names = {"staccato": "Staccato", "accent": "Accent", "tenuto": "Tenuto", "marcato": "Marcato", "staccatissimo": "Staccatissimo", "portato": "Portato", "fermata": "Fermata"}
  glyphs = {"staccato": "articStaccatoAbove", "accent": "articAccentAbove", "tenuto": "articTenutoAbove", "marcato": "articMarcatoAbove", "staccatissimo": "articStaccatissimoAbove", "portato": "articTenutoStaccatoAbove", "fermata": "fermataAbove"}
  artics = SCORE_ED_ARTICS.map { |name| score_ed_glyph_button("artic:" + name, "artic", {"name": name}, score_ed_centred_icon(glyphs[name]), !none && (current["scripts"] ?? []).any? { |sc| sc["name"] == name }, "", names[name]) }
  side = score_ed_words_side(state)
  side_name = side < 0 ? "Above" : "Below"
  said = ""
  for tx in none ? [] : (current["texts"] ?? [])
    said = tx["text"] if tx["dir"] == side
  end
  # Keyed by the note and the side: another note selected is another field,
  # holding that note's words, never the last one's typing.
  field_key = "score_words:" + str(state["track"] ?? 0) + ":" + str(state["voice"] ?? 1) + ":" + str(state["sel"] ?? -1) + ":" + side_name
  marks = score_ed_word_marks().map { |mark| score_ed_tool(mark, "words_mark", {"text": mark}, none, nil) }
  column({"gap": 3}, [
    row({"gap": 3, "align": "center", "wrap": "wrap"}, [row({"gap": 1, "align": "center"}, dyns), score_ed_rule()] + pins),
    row({"gap": 3, "align": "center", "wrap": "wrap"}, [row({"gap": 1, "align": "center"}, artics)]),
    row({"gap": 3, "align": "center", "wrap": "wrap"}, [
      score_ed_labelled("Text", segmented(["Above", "Below"], side_name, "words_dir")),
      input(said, "words", {"key": field_key, "placeholder": none ? "Select a note first" : (side < 0 ? "dolce" : "cresc."), "props": {"dir": side < 0 ? "up" : "down"}, "style": {"width": 200}}),
      score_ed_tool("Clear", "words_clear", {}, none || said == "", nil)
    ]),
    row({"gap": 1, "align": "center", "wrap": "wrap"}, marks),
    muted(none ? "Select a note to give it a dynamic, a hairpin, an articulation or words." : (armed ? "A hairpin is armed: select the note it ends on and press it again." : "On the selected note; pressed again, each comes off. A hairpin, like a slur: once here, once on the note it ends on."))
  ])
end

# A dynamic's letters as an icon, in the music font.
def score_ed_dyn_icon(dyn)
  sp = 6.0
  paths = []
  x = 0.3
  for name in score_dyn_letters(dyn)
    g = score_glyph(name)
    paths.push(score_fill("text.default", g["fill"], x, 3.3, sp))
    x += g["advance"]
  end
  canvas(int((x + 0.5) * sp) + 1, 29, paths)
end

# A glyph drawn about its own middle, as articulations are.
def score_ed_centred_icon(name)
  g = score_glyph(name)
  bb = g["bbox"]
  sp = 6.0
  canvas(int((bb[2] - bb[0] + 0.6) * sp) + 1, 29, [score_fill("text.default", g["fill"], 0.3 - bb[0], 2.4 - (bb[1] + bb[3]) / 2.0, sp)])
end

# The lyrics tab: the edited track's words, and how they meet its notes.
def score_ed_lyrics_panel(state)
  k = state["track"] ?? 0
  trk = score_ed_tracks(state)[k]
  count = score_ed_lyrics_count(state)
  tally = str(count["syllables"]) + " syllables for " + str(count["notes"]) + " notes that take one"
  tally += count["syllables"] > count["notes"] ? " — the last " + str(count["syllables"] - count["notes"]) + " are left out" : (count["syllables"] < count["notes"] && count["syllables"] > 0 ? " — the last " + str(count["notes"] - count["syllables"]) + " notes have none" : "")
  column({"gap": 2, "width": "100%"}, [
    textarea(trk["lyrics"] ?? "", "lyrics", {"key": "score_lyrics:" + str(k), "placeholder": "Frè -- re Jac -- ques, Frè -- re Jac -- ques", "rows": 3, "style": {"width": "100%"}}),
    muted(tally),
    muted("A syllable a note, in order, under the first voice: -- joins a word's syllables (Frè -- re), __ holds a syllable over the notes after it, _ skips a note. Tied and slurred notes hold the syllable before them.")
  ])
end

# The tracks, one button each — the one being edited lit — and a button
# to add one. Alt+↑ / Alt+↓ move between them from the keyboard.
def score_ed_track_bar(state)
  tracks = score_ed_tracks(state)
  active = state["track"] ?? 0
  chips = []
  full = score_ed_full_view(state)
  if tracks.length > 1
    chips.push(control({
      "key": "track:full",
      "tone": full ? "accent" : "neutral",
      "size": "sm",
      "on": {"click": "full_view"},
      "shape": {"pad": [1, 4, 1, 4]},
      "a11y": {"role": "tab", "label": "Full score", "selected": full},
      "c": [text("Full score", {"size": 1, "weight": "semibold", "fg": full ? "accent.on" : "text.default"})]
    }))
  end
  for trk, k in tracks
    label = trk["name"] == "" ? "Track " + str(k + 1) : trk["name"]
    label = label + "  (muted)" if trk["mute"] == true
    chips.push(control({
      "key": "track:" + str(k),
      "tone": k == active && !full ? "accent" : "neutral",
      "size": "sm",
      "on": {"click": "track"},
      "props": {"track": k},
      "shape": {"pad": [1, 4, 1, 4]},
      "a11y": {"role": "tab", "label": label, "selected": k == active && !full},
      "c": [text(label, {"size": 1, "weight": k == active && !full ? "semibold" : "medium", "fg": k == active && !full ? "accent.on" : "text.default"})]
    }))
  end
  # The chips wrap inside a strip two rows high that scrolls, so an
  # orchestra's twelve staves do not push the score down the page. The
  # row the edited track is on is scrolled to when it changes — lang sends
  # `scroll_to` only then, so the person's own scrolling stays put.
  strip_w = score_ed_width(state) - 200
  strip = scroll({"max_height": 2 * SCORE_ED_CHIP_ROW - 2, "width": strip_w, "grow": 1}, [
    row({"gap": 1, "wrap": "wrap", "width": strip_w - 16}, chips)
  ])
  strip["key"] = "score_track_strip"
  strip["p"] = {"scroll_to": [0, score_ed_chip_row(tracks, active, strip_w - 16) * SCORE_ED_CHIP_ROW]}
  column({"gap": 1, "width": "100%"}, [
    row({"gap": 2, "align": "start", "width": "100%"}, [
      text("Track", {"size": 1, "weight": "medium", "fg": "text.muted", "margin": [2, 0, 0, 0]}),
      strip,
      score_ed_tool("+ Add track", "track_add", {}, false, nil)
    ]),
    tracks.length > 1 ? muted("Alt+↑ / Alt+↓ to switch") : muted("Add a track for a second staff: a piano's left hand, another instrument.")
  ])
end

# The row of the strip track `k`'s chip lands on, the chips laid as the
# client will lay them: an estimate from each label's length, which is all
# the server has — off by a chip at worst, and the strip shows two rows.
def score_ed_chip_row(tracks, k, width)
  # "Full score" leads the strip when there are several tracks.
  x = tracks.length > 1 ? 10 * 7.6 + 32.0 : 0.0
  line = 0
  for trk, j in tracks
    label = trk["name"] == "" ? "Track " + str(j + 1) : trk["name"]
    label = label + "  (muted)" if trk["mute"] == true
    chip = label.length * 7.6 + 30.0
    if x > 0.0 && x + chip > width
      line += 1
      x = 0.0
    end
    return line if j == k

    x += chip + 2.0
  end
  line
end

# The tracks tab: the one being edited — its names, its instrument, its
# loudness and place in the mix, muted or not — its order, and how the
# full score groups the staves.
def score_ed_tracks_panel(state)
  tracks = score_ed_tracks(state)
  k = state["track"] ?? 0
  trk = tracks[k]
  pan_name = SCORE_ED_PANS[[-100, -50, 0, 50, 100].index_of(trk["pan"] ?? 0)] ?? "Centre"
  rows = [
    row({"gap": 4, "align": "center", "wrap": "wrap"}, [
      score_ed_labelled("Name", input(trk["name"] ?? "", "track_name", {"key": "score_track_name", "placeholder": "Flute", "style": {"width": 170}})),
      score_ed_labelled("Short", input(trk["short"] ?? "", "track_short", {"key": "score_track_short", "placeholder": "Fl.", "style": {"width": 80}})),
      score_ed_labelled("Instrument", select(SCORE_ED_INSTRUMENTS, trk["instrument"] ?? "acoustic grand", state["inst_open"] == true, "inst_open", "track_instrument", {"key": "score_track_inst"}))
    ]),
    row({"gap": 4, "align": "center", "wrap": "wrap"}, [
      score_ed_labelled("Volume", input(str(trk["volume"] ?? 100), "track_volume", {"key": "score_track_volume", "style": {"width": 60}})),
      score_ed_labelled("Pan", segmented(SCORE_ED_PANS, pan_name, "track_pan")),
      score_ed_tool(trk["mute"] == true ? "Muted" : "Mute", "track_mute", {}, false, nil)
    ]),
    row({"gap": 2, "align": "center", "wrap": "wrap"}, [
      score_ed_tool("Move up", "track_up", {}, k == 0, nil),
      score_ed_tool("Move down", "track_down", {}, k + 1 >= tracks.length, nil),
      score_ed_tool("Remove track", "track_remove", {}, tracks.length < 2, nil),
      score_ed_rule(),
      score_ed_labelled("Full score", segmented(SCORE_ED_GROUPS, state["group"] ?? "Bracket", "group"))
    ]),
    muted("Volume is MIDI's 0–127; the pan and the instrument go into the MIDI file. A muted track is silent in Play, and still in the score and the file. Changes from a bar, repeats and breaks are the first track's, and every staff takes them.")
  ]
  column({"gap": 3}, rows)
end

# The full score, under the sheet, once there are two tracks: every staff,
# the notes sounding lit while it plays, the selection lit otherwise. A
# click on a staff edits its track, on the note under the pointer.
def score_ed_conductor_card(state)
  return {"k": "box", "s": {"display": "column"}, "c": []} if score_ed_tracks(state).length < 2

  full = score_ed_conductor(state)
  width = score_ed_width(state)
  sp = score_ed_full_space(state)
  inner = nil
  pages = 1
  try
    engraved = score_ed_conductor_engraved(state, full)
    pages = engraved["pages"].length
    shown = score_ed_page_shown(state, engraved, "cond_page")
    inner = score_band_column(engraved, sp, {"colour": "text.default", "marks": score_ed_conductor_marks(state, full), "page_index": shown}, width, score_ed_page_height(engraved, shown, sp))
  catch e
    inner = text(str(e).replace("score: ", ""), {"fg": "danger.base", "size": 1})
  end
  heading = [text("Full score", {"size": 2, "weight": "semibold"}), muted("Every track, as the PDF and the MIDI have them. Click a note to edit it; a track's name above goes back to its staff.")]
  pager = score_ed_pager(state, "cond_page", pages)
  heading = heading + [{"k": "box", "s": {"grow": 1}}, pager] unless pager.nil?
  card({"gap": 3, "pad": 5, "width": "100%"}, [
    row({"gap": 3, "align": "center", "wrap": "wrap", "width": "100%"}, heading),
    {"k": "box", "s": {"display": "column", "width": width, "cursor": "pointer"}, "on": {"click": "conductor_pick"}, "p": {"role": "group", "label": "Full score"}, "c": [inner]}
  ])
end

# The bars tab: repeat signs at the selected bar.
def score_ed_bars_panel(state)
  current = score_ed_selected(state)
  column({"gap": 2}, [
    row({"gap": 2, "align": "center", "wrap": "wrap"}, [
      score_ed_tool("Repeat from here", "repeat_start", {}, current.nil?, "["),
      score_ed_tool("Repeat to here", "repeat_end", {}, current.nil?, "]"),
      score_ed_rule(),
      score_ed_tool("Line break after", "line_break", {}, current.nil?, nil),
      score_ed_tool("Page break after", "page_break", {}, current.nil?, nil)
    ]),
    muted("At the bar the selected note is in: a repeat starts there or ends after it, and a line or a page can be made to end after it. Press again to take it off.")
  ])
end

# What the key, the metre and the tempo are, and — the scope — whether a
# change to one is for the whole piece or from the selected bar on. The key
# and metre shown are the ones in force at the selection.
def score_ed_settings_bar(state)
  sel = state["sel"] ?? -1
  beat = {"1": "𝅝", "2": "𝅗𝅥", "4": "♩", "8": "♪", "16": "𝅘𝅥𝅯"}[str(state["tempo_base"] ?? 4)] ?? "♩"
  shown_key = score_ed_key_at(state, sel)
  shown_time = score_ed_time_at(state, sel)
  column({"gap": 3, "width": "100%"}, [
    row({"gap": 5, "align": "center", "wrap": "wrap"}, [
      score_ed_labelled("Clef", segmented(SCORE_ED_CLEFS, state["clef"], "clef")),
      score_ed_labelled("Key", select(SCORE_ED_KEYS.map { |k| k + " major" }, shown_key + " major", state["key_open"] == true, "key_open", "key_pick", {"key": "score_key"})),
      score_ed_labelled("Time", select_sized(SCORE_ED_TIMES, shown_time, state["time_open"] == true, "time_open", "time_pick", 90, false, {"key": "score_time"})),
      score_ed_labelled("Applies to", segmented(SCORE_ED_SCOPES, state["scope"], "scope"))
    ]),
    row({"gap": 3, "align": "center", "wrap": "wrap"}, [
      score_ed_labelled("Tempo mark", row({"gap": 2, "align": "center"}, [
        input(state["tempo_text"] ?? "", "tempo_text", {"key": "score_tempo_text", "placeholder": "Allegro", "style": {"width": 150}}),
        text(beat + " =", {"size": 1, "fg": "text.muted"}),
        input(str(state["tempo"]), "bpm", {"key": "score_bpm", "style": {"width": 64}})
      ])),
      score_ed_tool(state["mark_tempo"] != false ? "Printed" : "Not printed", "mark_tempo", {}, false, nil),
      muted(state["mark_tempo"] != false ? "Over the first bar: " + score_ed_tempo_shown(state, beat) : "The speed still sets Play and the MIDI file.")
    ])
  ])
end

# The tempo mark as the score prints it: its words, then the speed in
# brackets — "Moderato (♩ = 100)" — or the speed alone.
def score_ed_tempo_shown(state, beat)
  words = (state["tempo_text"] ?? "").trim
  speed = beat + " = " + str(state["tempo"])
  words == "" ? speed : words + " (" + speed + ")"
end

def score_ed_time_at(state, idx)
  last = idx < 0 ? state["notes"].length - 1 : idx
  time = state["time"]
  for nt, k in state["notes"]
    break if k > last

    changes = nt["set"] ?? {}
    time = changes["time"] if changes.has_key("time")
  end
  time
end

# The title page's fields: what `\header` will say.
def score_ed_titles(state)
  hdr = state["header"] ?? {}
  labels = {"title": "Title", "subtitle": "Subtitle", "composer": "Composer", "arranger": "Arranger"}
  row({"gap": 4, "width": "100%"}, SCORE_ED_HEADER.map { |name| column({"gap": 1, "grow": 1, "basis": 0}, [text(labels[name], {"size": 1, "weight": "medium", "fg": "text.muted"}), input(hdr[name] ?? "", "hdr_" + name, {"key": "score_hdr_" + name, "placeholder": name == "title" ? "Untitled" : "", "style": {"width": "100%"}})]) })
end

def score_ed_labelled(label, control_node)
  row({"gap": 2, "align": "center"}, [text(label, {"size": 1, "weight": "medium", "fg": "text.muted"}), control_node])
end

# The palette: writing notes, as MuseScore's note-input toolbar has it —
# values, dot, rest and accidentals, then what joins notes (tie, slur,
# tuplets), then the pitches for whoever does not type them. Each button
# shows its key. It runs down the left of the window, `body_h` tall, and
# scrolls on its own when the window is short. On the full score, where a
# click picks a note and writes none, the mode and the voice go: the
# insert tool and the second voice are the single staff's.
def score_ed_palette(state, body_h)
  current = score_ed_selected(state)
  writing = current.nil? || state["typing"] == true || score_ed_inserting(state)
  shown_base = writing ? state["dur"] : current["base"]
  shown_dot = writing ? state["dot"] : current["dots"] > 0
  values = [1, 2, 4, 8, 16, 32]
  durations = values.map { |b| score_ed_glyph_button("dur:" + str(b), "dur", {"base": b}, score_ed_note_icon(b), shown_base == b, str(7 - values.index_of(b)), score_ed_duration_name(b)) }
  pitches = range(0, 7).map { |k| score_ed_text_button("note:" + str(k), "note", {"step": k}, "CDEFGAB".chars()[k], ["do", "ré", "mi", "fa", "sol", "la", "si"][k]) }
  no_note = current.nil? || current["rest"]
  inner_w = score_ed_rail_w() - 2 * 12 - 10
  full = score_ed_full_view(state)
  groups = []
  if full
    groups.push(score_ed_rail_group("Full score", inner_w, [muted("A click picks a note on its staff. Writing by click, and the second voice, are on a single track: pick it above the score.")]))
  else
    groups.push(score_ed_rail_group("Mode", inner_w, [segmented(SCORE_ED_TOOLS, state["tool"] ?? SCORE_ED_TOOLS[0], "tool"), kbd("N")]))
    groups.push(score_ed_rail_group("Voice", inner_w, [segmented(SCORE_ED_VOICES, SCORE_ED_VOICES[(state["voice"] ?? 1) - 1], "voice"), kbd("Ctrl+Alt+1/2")]))
  end
  groups.push(score_ed_rail_group("Value", inner_w, durations + [
    score_ed_glyph_button("dot", "dot", {}, score_ed_dot_icon(), shown_dot, ".", "Dotted"),
    score_ed_glyph_button("rest", "rest", {}, score_ed_glyph_icon("restQuarter", 2.0), false, "R", "Rest")
  ]))
  groups.push(score_ed_rail_group("Accidental", inner_w, [
    score_ed_glyph_button("flat", "acc", {"by": -1}, score_ed_glyph_icon("accidentalFlat", 2.6), false, "−", "Flatten"),
    score_ed_glyph_button("natural", "natural", {}, score_ed_glyph_icon("accidentalNatural", 2.0), false, "=", "Natural"),
    score_ed_glyph_button("sharp", "acc", {"by": 1}, score_ed_glyph_icon("accidentalSharp", 2.0), false, "+", "Sharpen")
  ]))
  groups.push(score_ed_rail_group("Join", inner_w, [
    score_ed_tool("Tie", "tie", {}, no_note, "T"),
    score_ed_slur_button(state, no_note),
    score_ed_tool("Triplet", "tuplet", {"n": 3}, current.nil?, "Ctrl+3"),
    score_ed_tool("Sextuplet", "tuplet", {"n": 6}, current.nil?, "Ctrl+6")
  ]))
  groups.push(score_ed_rail_group("Add a note", inner_w, pitches))
  groups.push(score_ed_rail_group("Change the note", inner_w, [
    score_ed_tool("Step ↑", "step", {"by": 1}, no_note, "↑"),
    score_ed_tool("Step ↓", "step", {"by": -1}, no_note, "↓"),
    score_ed_tool("Semitone ↑", "semitone", {"by": 1}, no_note, "⇧↑"),
    score_ed_tool("Semitone ↓", "semitone", {"by": -1}, no_note, "⇧↓"),
    score_ed_tool("Delete", "delete", {}, current.nil?, "⌫")
  ]))
  rail = scroll({"width": score_ed_rail_w(), "height": body_h, "bg": "surface.raised"}, [
    column({"gap": 5, "pad": [5, 4, 5, 4], "width": score_ed_rail_w() - 10}, groups)
  ])
  rail["key"] = "score_rail"
  rail
end

# One of the toolbar's groups: its name, then its controls wrapping within
# the toolbar's width.
def score_ed_rail_group(label, width, kids)
  column({"gap": 2, "width": width}, [
    text(label, {"size": 0, "weight": "semibold", "fg": "text.muted"}),
    row({"gap": 1, "align": "center", "wrap": "wrap", "width": width}, kids)
  ])
end

def score_ed_duration_name(base)
  {"1": "Whole", "2": "Half", "4": "Quarter", "8": "Eighth", "16": "Sixteenth", "32": "32nd"}[str(base)]
end

def score_ed_play_button(state)
  playing = state["playing"] == true
  control({
    "key": "tool:play",
    "tone": "accent",
    "size": "sm",
    "on": {"click": "play"},
    "disabled": state["notes"].length == 0,
    "shape": {"gap": 2, "pad": [1, 4, 1, 4]},
    "a11y": {"role": "button", "label": playing ? "Stop" : "Play"},
    "c": [text(playing ? "■  Stop" : "▶  Play", {"size": 1, "weight": "semibold", "fg": "accent.on"}), kbd("Space")]
  })
end

# Lit while a slur is armed, and pressable then even with no note: a
# press puts it out.
def score_ed_slur_button(state, no_note)
  armed = (state["slur_from"] ?? -1) >= 0
  return score_ed_tool("Slur", "slur", {}, no_note, "S") unless armed

  control({
    "key": "tool:slur:Slur",
    "tone": "accent",
    "size": "sm",
    "on": {"click": "slur"},
    "shape": {"gap": 2, "pad": [1, 3, 1, 3]},
    "a11y": {"role": "button", "label": "Slur: on — S on the last note ends it", "pressed": true},
    "c": [text("Slur ●", {"size": 1, "weight": "medium", "fg": "accent.on"}), kbd("S")]
  })
end

def score_ed_tool(label, on_click, props, disabled, hint)
  kids = [text(label, {"size": 1, "weight": "medium"})]
  kids.push(kbd(hint)) unless hint.nil?
  control({
    "key": "tool:" + on_click + ":" + label,
    "tone": "neutral",
    "size": "sm",
    "on": {"click": on_click},
    "props": props,
    "disabled": disabled,
    "shape": {"gap": 2, "pad": [1, 3, 1, 3]},
    "a11y": {"role": "button", "label": label},
    "c": kids
  })
end

def score_ed_text_button(key, on_click, props, label, hint)
  control({
    "key": key,
    "tone": "neutral",
    "size": "sm",
    "on": {"click": on_click},
    "props": props,
    "shape": {"gap": 1, "pad": [1, 3, 1, 3], "min_width": 44},
    "a11y": {"role": "button", "label": "Add " + label},
    "c": [text(label, {"size": 1, "weight": "semibold"}), text(hint, {"size": 0, "fg": "text.muted"})]
  })
end

def score_ed_glyph_button(key, on_click, props, picture, lit, hint, name)
  control({
    "key": "glyph:" + key,
    "tone": "neutral",
    "size": "sm",
    "selected": lit,
    "on": {"click": on_click},
    "props": props,
    "shape": {"gap": 1, "pad": [1, 2, 1, 2], "min_width": 40, "min_height": 40},
    "a11y": {"role": "button", "label": name, "selected": lit},
    "c": [picture, text(hint, {"size": 0, "fg": "text.muted"})]
  })
end

# A note value drawn small, from the same outlines the score uses: a head,
# a stem, and the flags its value has.
def score_ed_note_icon(base)
  sp = 6.0
  ink = "text.default"
  head_name = score_head_name(base)
  head_glyph = score_glyph(head_name)
  paths = []
  oy = 4.0
  paths.push(score_fill(ink, head_glyph["fill"], 0.3, oy, sp))
  if base > 1
    sx = 0.3 + head_glyph["advance"] - 0.12
    root = oy + head_glyph["anchors"]["stemUpSE"][1]
    tip = oy - 3.5
    paths.push(score_rect(ink, sx, tip, 0.14, root - tip, sp))
    if base >= 8
      flag = score_flag_name(base, true)
      anchor = score_glyph(flag)["anchors"]["stemUpNW"]
      paths.push(score_fill(ink, score_glyph(flag)["fill"], sx - anchor[0], tip - anchor[1], sp))
    end
  end
  canvas(int(3.0 * sp), int(4.8 * sp), paths)
end

def score_ed_dot_icon()
  canvas(12, 29, [score_fill("text.default", score_glyph("augmentationDot")["fill"], 0.5, 3.0, 6.0)])
end

# A glyph centred in the same 29 px a note icon is tall; `mid` is where its
# origin sits, in staff spaces from the top.
def score_ed_glyph_icon(name, mid)
  g = score_glyph(name)
  sp = 6.0
  canvas(int((g["advance"] + 0.4) * sp) + 1, 29, [score_fill("text.default", g["fill"], 0.2, mid + 0.4, sp)])
end

# The staff itself: one box that takes the clicks and the keys, the canvas
# inside it. The selection is the accent colour.
def score_ed_sheet(state, drawn)
  sp = drawn["sp"]
  width = score_ed_width(state)
  inner = nil
  if drawn["engraved"].nil?
    inner = text(drawn["problem"], {"fg": "danger.base", "size": 1})
  else
    marks = {}
    playing = state["playing"] == true
    lit = playing ? -1 : (state["sel"] ?? -1)
    lit_set = playing ? ((state["play_lit"] ?? [])[state["track"] ?? 0] ?? []) : []
    armed = playing ? -1 : ((state["slur_from"] ?? -1) >= 0 ? state["slur_from"] : (state["pin_from"] ?? -1))
    for owner, k in drawn["map"]
      marks[str(k)] = "accent.base" if owner >= 0 && owner == lit
      marks[str(k)] = "info.base" if owner == armed && owner != lit
    end
    # What plays is lit on the first voice, whichever is being edited.
    for owner, k in drawn["map1"] ?? drawn["map"]
      marks[str(k)] = "accent.base" if owner >= 0 && lit_set.includes?(owner)
    end
    if drawn["a4"]
      return score_ed_sheets(state, drawn, marks)
    end
    shown = score_ed_page_shown(state, drawn["engraved"], "page")
    inner = score_band_column(drawn["engraved"], sp, {"colour": "text.default", "marks": marks, "page_index": shown}, width, score_ed_page_height(drawn["engraved"], shown, sp))
  end
  shown = drawn["engraved"].nil? ? 0 : score_ed_page_shown(state, drawn["engraved"], "page")
  staff_box = {
    "k": "box",
    "s": {"display": "column", "width": width, "cursor": score_ed_inserting(state) ? "text" : "pointer"},
    "on": {"pointer_down": "press", "pointer_move": "drag", "pointer_up": "release", "key_down": "key"},
    "p": {"page": shown, "typing": true, "autofocus": true, "drag_only": true, "role": "group", "label": "Score — type A to G to add notes"},
    "c": [inner]
  }
  kids = [staff_box]
  pager = drawn["engraved"].nil? ? nil : score_ed_pager(state, "page", drawn["engraved"]["pages"].length)
  kids = [pager, staff_box] unless pager.nil?
  card({"pad": 5, "gap": 3, "width": "100%", "bg": "surface.raised"}, kids)
end

# A4: each page a sheet of paper that takes the clicks for its own systems.
def score_ed_sheets(state, drawn, marks)
  engraved = drawn["engraved"]
  sp = drawn["sp"]
  page_w = score_ed_page_width(state)
  page_h = int(engraved["paper"] * sp)
  shown = score_ed_page_shown(state, engraved, "page")
  pager = score_ed_pager(state, "page", engraved["pages"].length)
  sheets = engraved["pages"].filter { |pg| pg["index"] == shown }.map { |pg|
    {
      "k": "box",
      "s": {"bg": "surface.raised", "shadow": 2, "border": 1, "border_color": "border.subtle", "width": page_w, "cursor": score_ed_inserting(state) ? "text" : "pointer"},
      "on": {"pointer_down": "press", "pointer_move": "drag", "pointer_up": "release", "key_down": "key"},
      "p": {"page": pg["index"], "typing": true, "autofocus": true, "drag_only": true, "role": "group", "label": "Page " + str(pg["index"] + 1)},
      "c": [score_band_column(engraved, sp, {"colour": "text.default", "marks": marks, "page_index": pg["index"]}, page_w, page_h)]
    }
  }
  column({"gap": 6, "width": "100%", "align": "center", "pad": 5, "bg": "surface.sunken", "radius": 2}, pager.nil? ? sheets : [pager] + sheets)
end

def score_ed_status(state)
  current = score_ed_selected(state)
  said = ""
  coming = score_ed_duration_name(state["dur"]).downcase() + (state["dot"] ? ", dotted" : "")
  if current.nil?
    said = "Nothing selected: new notes go at the end, as " + coming + "s. Click a note to select it."
  elsif state["typing"] == true
    what = current["rest"] ? "a rest" : current["pitches"].map { |pt| score_ed_pitch_name(pt) }.join(" + ")
    said = "Writing: " + what + " added. The next note is a " + coming + " — pick another value first to change it."
  else
    what = current["rest"] ? "Rest" : current["pitches"].map { |pt| score_ed_pitch_name(pt) }.join(" + ")
    value = score_ed_duration_name(current["base"]).downcase() + (current["dots"] > 0 ? ", dotted" : "")
    tied = current["tie"] == true ? ", tied to the next" : ""
    said = "Note " + str(state["sel"] + 1) + " of " + str(state["notes"].length) + ": " + what + " · " + value + tied + ". Drag it, or click it again, to move it; ↑ ↓ along the scale, with Shift by semitones."
  end
  row({"gap": 2, "align": "center", "wrap": "wrap"}, [badge(str(state["notes"].length) + " notes", "info"), text(said, {"size": 1, "fg": "text.muted"})])
end

def score_ed_pitch_name(pt)
  sign = {"-2": "𝄫", "-1": "♭", "0": "", "1": "♯", "2": "𝄪"}[str(pt["alter"])]
  "CDEFGAB".chars()[pt["step"]] + sign + str(pt["oct"])
end

# The source, editable while it fits in a field: a text on the wire is at
# most 4 096 bytes (10 §2), and a symphony's is five times that. Past it the
# card shows it read-only, a block a staff, each cut under the ceiling.
def score_ed_source(state, drawn)
  if score_ed_conductor_big(state) && state["src_open"] != true
    return card({"gap": 2, "pad": 5, "width": "100%"}, [
      row({"gap": 3, "align": "center", "width": "100%"}, [
        text("LilyPond source", {"size": 1, "weight": "semibold"}),
        muted("Folded: a piece this size is written out again at every edit while it shows."),
        {"k": "box", "s": {"grow": 1}},
        score_ed_tool("Show source", "src_open", {}, false, nil)
      ])
    ])
  end
  src = score_ed_tracks(state).length > 1 ? score_ed_full_source(state) : drawn["src"]
  if src.length > score_ed_field_bytes()
    kids = [
      text("LilyPond source", {"size": 1, "weight": "semibold"}),
      muted("This piece's source is " + str(src.length / 1024) + " KB, more than a field holds (4 KB): it is shown here to read, and edited from the palette above. Download MIDI or PDF to take it elsewhere.")
    ]
    width = score_ed_width(state) - 48
    for block in score_ed_source_blocks(src)
      kids.push(text(block, {"font": "mono", "size": 0, "fg": "text.muted", "width": width}))
    end
    kids.push(text(state["error"], {"size": 1, "fg": "danger.base"})) if (state["error"] ?? "") != ""
    return card({"gap": 2, "pad": 5, "width": "100%"}, kids)
  end
  kids = [
    text("LilyPond source", {"size": 1, "weight": "semibold"}),
    muted("Edit or paste LilyPond here: the score follows once it reads. What it cannot read is said below, and the score stays as it was."),
    textarea(src, "src", {"rows": 3, "key": "score_src", "style": {"font": "mono", "size": 1, "width": "100%"}})
  ]
  kids.push(text(state["error"], {"size": 1, "fg": "danger.base"})) if (state["error"] ?? "") != ""
  card({"gap": 2, "pad": 5, "width": "100%"}, kids)
end

# What a field may hold, under the 4 096 bytes a text may weigh.
def score_ed_field_bytes()
  4000
end

# A source in blocks for reading: one a staff, and a staff longer than a
# text may be cut at a space.
def score_ed_source_blocks(src)
  out = []
  for piece in src.split("\\new Staff")
    staff = out.length == 0 ? piece : "\\new Staff" + piece
    block = ""
    for word in staff.split(" ")
      if block.length + word.length + 1 > 3000
        out.push(block)
        block = ""
      end
      block = block == "" ? word : block + " " + word
    end
    out.push(block) if block.trim != ""
  end
  out
end

def score_ed_help(state)
  open = state["help"] == true
  rows = [
    [ ["A", "…", "G"], "Add a note after the selection" ],
    [ ["Shift", "A…G"], "Add a note to the selected chord" ],
    [ ["R"], "Add a rest" ],
    [ ["N"], "Insert mode: a click writes a note — onto a note as a chord (in another value, as a second voice), into a rest, or between notes (Esc: back to the pointer)" ],
    [ ["Ctrl", "Alt", "1 / 2"], "Write in the first voice (stems up) or the second (stems down)" ],
    [ ["2", "…", "7"], "32nd, 16th, 8th, quarter, half, whole" ],
    [ ["."], "Dot the note" ],
    [ ["+"], "Sharpen" ],
    [ ["−"], "Flatten" ],
    [ ["="], "Natural" ],
    [ ["↑", "↓"], "A step up or down the key's scale (Ctrl: an octave)" ],
    [ ["Shift", "↑"], "A semitone up or down, sharps and flats included" ],
    [ ["←", "→"], "Select the previous or next note" ],
    [ ["T"], "Tie to the next note" ],
    [ ["S"], "Slur: on here, then S on the last note turns it off" ],
    [ ["<", ">"], "Crescendo, diminuendo: here, then again on the last note" ],
    [ ["Ctrl", "3"], "Make the note a triplet (Ctrl+6: a sextuplet)" ],
    [ ["["], "A repeat starts at this bar" ],
    [ ["]"], "A repeat ends after this bar" ],
    [ ["⌫"], "Delete the note" ],
    [ ["Ctrl", "Z"], "Undo (Ctrl+Y redoes)" ]
  ]
  body = []
  if open
    body = [row({"gap": 4, "wrap": "wrap"}, rows.map { |r| row({"gap": 2, "align": "center", "width": 300}, [row({"gap": 1}, r[0].map { |k| kbd(k) }), text(r[1], {"size": 1})]) })]
  end
  column({"gap": 3, "width": "100%"}, [ghost_button(open ? "Hide the keyboard shortcuts" : "Show the keyboard shortcuts", "help")] + body)
end
