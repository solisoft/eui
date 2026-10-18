//! What protocol 11 reads beyond the Soli engraver (03 §1.4), each held to
//! what LilyPond 2.25 does with the same source.
//!
//! `tests/lilypond/*.ly` are the pieces that were set beside LilyPond's own
//! engraving while this was written — variables and voices, a pickup in
//! English note names, repeats written out, clefs and their keys, marks,
//! grace notes, chord names, drums and a tablature: each must engrave on a
//! screen and on A4, and play. The tests after them pin what the pictures
//! showed: the pitches, the bars, the names, the frets, the keys.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use std::fs;
use std::path::PathBuf;

use eui_score::measure::{clef_mid, measures};
use eui_score::parse::{parse, Clef, Kind, Parsed, StaffKind};
use eui_score::{engrave, engrave_a4, midi, paths, protocol, Layout, MidiOptions};

fn dir(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join(sub)
}

fn piece(name: &str) -> String {
    fs::read_to_string(dir("lilypond").join(format!("{name}.ly"))).unwrap()
}

fn read(src: &str) -> Parsed {
    parse(src).unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

/// The pitches of a staff's notes, in order: `(d, alter)` per head.
fn pitches(p: &Parsed, staff: usize) -> Vec<Vec<(i64, i64)>> {
    p.staves[staff].events.iter().filter(|e| e.k == Kind::Note).map(|e| e.pitches.iter().map(|x| (x.d, x.alter)).collect()).collect()
}

#[test]
fn every_piece_engraves_plays_and_needs_protocol_11() {
    let mut names: Vec<String> = fs::read_dir(dir("lilypond")).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".ly")).collect();
    names.sort();
    assert!(names.len() >= 9, "the pieces are missing");
    for name in names {
        let src = fs::read_to_string(dir("lilypond").join(&name)).unwrap();
        let eng = engrave(&src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        let (drawn, _) = paths(&eng, 8.0, 0).unwrap();
        assert!(drawn.len() > 20, "{name}: {} paths", drawn.len());
        engrave_a4(&src).unwrap_or_else(|e| panic!("{name} on A4: {e}"));
        let bytes = midi(&src, &MidiOptions::default()).unwrap_or_else(|e| panic!("{name} as MIDI: {e}"));
        assert_eq!(&bytes[..4], b"MThd", "{name}");
        assert_eq!(protocol(&src), Ok(11), "{name}");
    }
}

#[test]
fn what_the_soli_engraver_reads_needs_protocol_9() {
    for entry in fs::read_dir(dir("corpus")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "ly") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let src = fs::read_to_string(&path).unwrap();
        // Three pieces sing voices under `\relative`, which this reads as
        // LilyPond does and the Soli engraver did not (`tests/golden.rs`).
        let want = if ["voices", "piano_voices", "voices_spacers"].contains(&name.as_str()) { 11 } else { 9 };
        assert_eq!(protocol(&src), Ok(want), "{name}");
    }
}

#[test]
fn under_relative_a_voice_starts_from_the_last_pitch_of_the_one_before() {
    // LilyPond's `music_list_to_relative`: `g,` after the first voice's e''
    // is g', and the c1 after `>>` follows the second voice's c''.
    let p = read(&piece("voices_relative"));
    let notes = pitches(&p, 0);
    assert_eq!(notes[2], vec![(13, 0)], "the first voice's b'");
    assert_eq!(notes[6], vec![(11, 0)], "the second voice's g'");
    assert_eq!(notes[10], vec![(14, 0)], "c'' after the voices");
    let corpus = read(&fs::read_to_string(dir("corpus").join("voices.ly")).unwrap());
    let second = corpus.staves[0].events.iter().skip_while(|e| e.k != Kind::VoiceNext).find(|e| e.k == Kind::Note).unwrap();
    assert_eq!(second.pitches[0].d, 14, "c'' under g'', as LilyPond draws voices.ly");
}

#[test]
fn variables_a_global_two_voices_and_lyrics_sung_to_one_by_name() {
    let p = read(&piece("vars"));
    assert_eq!(p.staves.len(), 1);
    let staff = &p.staves[0];
    assert_eq!(staff.voices, vec![("soprano".to_string(), 1), ("alto".to_string(), 2)]);
    assert_eq!(staff.lyrics_voice, 1);
    assert_eq!(staff.lyrics.as_ref().unwrap().len(), 8, "Glo-ry to God in the high-est");
    let bars = measures(&staff.events).unwrap();
    assert_eq!((bars[0].fifths, bars[0].time), (1, (3, 4)), "the \\global's key and metre, set in the first voice");
    assert!(bars.iter().all(|b| b.poly));
}

#[test]
fn a_pickup_in_english_names_fixed_and_transposed_with_q() {
    let p = read(&piece("pickup"));
    let notes = pitches(&p, 0);
    // \transpose c d of \fixed c' { g8 a | <c' e' g'>4 q q | bf2 fs4 … }
    assert_eq!(notes[0], vec![(12, 0)], "g' up a tone is a'");
    assert_eq!(notes[2], vec![(15, 0), (17, 1), (19, 0)], "<c'' e'' g''> up a tone is <d'' fis'' a''>");
    assert_eq!(notes[3], notes[2], "q repeats the chord");
    assert_eq!(notes[5], vec![(14, 0)], "bf' up a tone is c''");
    assert_eq!(notes[6], vec![(11, 1)], "fs' up a tone is gis'");
    let bars = measures(&p.staves[0].events).unwrap();
    assert_eq!((bars[0].length, bars[0].offset), (eui_score::WHOLE / 4, eui_score::WHOLE / 2), "a quarter's pickup in 3/4");
    assert_eq!((bars[0].number, bars[1].number), (0, 1), "the pickup is bar 0");
}

#[test]
fn repeats_written_out_percent_bars_and_a_cadenza() {
    let p = read(&piece("unfold"));
    let bars = measures(&p.staves[0].events).unwrap();
    assert_eq!(bars[0].items.len(), 8, "\\repeat unfold 2 {{ c8 d e f }}");
    let numbers: Vec<usize> = bars[0].items.iter().map(|e| e.i).collect();
    assert_eq!(numbers, (0..8).collect::<Vec<_>>(), "each copy its own number");
    assert!(!bars[2].percent && bars[3].percent, "the second time is the percent sign");
    assert!(bars[4].cadenza && bars[4].length == eui_score::WHOLE && bars[4].bar == "|", "the cadenza is one bar, closed by \\bar");
    let eng = engrave(&piece("unfold"), 900.0 / 8.0, &Layout::default()).unwrap();
    assert!(eng.staves[0][4].beams.is_empty(), "no beams in a cadenza but the ones written");
}

#[test]
fn clefs_put_the_middle_line_where_lilypond_does() {
    let mid = |name: &str| clef_mid(Clef::named(name).unwrap());
    assert_eq!((mid("treble"), mid("bass"), mid("alto")), (13, 1, 7));
    assert_eq!(mid("treble_8"), 6, "an octave down: b on the middle line");
    assert_eq!(mid("bass^8"), 8);
    assert_eq!(mid("tenor"), 5);
    assert_eq!(mid("soprano"), 11);
    assert_eq!(mid("baritone"), 3);
    assert_eq!(mid("french"), 15);
    assert!(Clef::named("treble_7").is_none());
}

#[test]
fn grace_notes_take_no_time_and_no_number() {
    let p = read(&piece("grace"));
    let notes: Vec<_> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).collect();
    assert_eq!(notes[0].graces.len(), 1, "\\acciaccatura d8 c4");
    assert_eq!(notes[2].graces.len(), 2, "\\grace {{ c16 d }} e4");
    let numbers: Vec<usize> = notes.iter().map(|e| e.i).take(6).collect();
    assert_eq!(numbers, (0..6).collect::<Vec<_>>());
    let bars = measures(&p.staves[0].events).unwrap();
    assert_eq!(bars[0].items.len(), 4, "four quarters, the graces riding on them");
    let bytes = midi("{ \\acciaccatura d''8 c''4 }", &MidiOptions::default()).unwrap();
    let on: Vec<u8> = bytes.windows(3).filter(|w| w[0] == 0x90 && w[2] > 0).map(|w| w[1]).collect();
    assert_eq!(on, vec![74, 72], "the grace sounds first");
}

#[test]
fn chord_names_as_lilypond_names_them() {
    let names = |src: &str| -> Vec<String> {
        let p = read(src);
        p.staves[0]
            .events
            .iter()
            .filter(|e| !e.chord_name.is_empty())
            .map(|e| e.chord_name.iter().map(|r| if r.glyph { r.text.replace("accidental", "") } else { r.text.clone() }).collect::<Vec<_>>().join(""))
            .collect()
    };
    assert_eq!(
        names("\\chords { c1 c:m c:7 c:maj7 c:m7 c:dim c:dim7 c:aug c:sus4 c:6 c:9 c:13 c:m7.5- c:7.9- c/e bes:7 fis:m7 r1 }"),
        vec!["C", "Cm", "C7", "C△", "Cm7", "C°", "C°7", "C+", "Csus4", "C6", "C9", "C9 13", "Cø", "C7Flat9", "C/E", "BFlat7", "FSharpm7", "N.C."]
    );
    let p = read("\\chordmode { c1 c:m7 c/e }");
    assert_eq!(pitches(&p, 0)[0], vec![(7, 0), (9, 0), (11, 0)], "a chord's root is c' in chord mode");
    assert_eq!(pitches(&p, 0)[1], vec![(7, 0), (9, -1), (11, 0), (13, -1)]);
    assert_eq!(pitches(&p, 0)[2][0], (2, 0), "/e puts the e under the root");
}

#[test]
fn drums_sit_where_lilypond_puts_them_and_play_on_the_tenth_channel() {
    let p = read("\\drums { hh4 sn bd cymc }");
    assert_eq!(p.staves[0].kind, StaffKind::Drums);
    let bars = measures(&p.staves[0].events).unwrap();
    let pos: Vec<i64> = bars[0].items.iter().map(|e| e.heads[0].pos).collect();
    assert_eq!(pos, vec![3, 1, -3, 5]);
    let styles: Vec<&str> = bars[0].items.iter().map(|e| e.heads[0].style).collect();
    assert_eq!(styles, vec!["X", "", "", "CircleX"]);
    let bytes = midi("\\drums { hh4 sn bd cymc }", &MidiOptions::default()).unwrap();
    let on: Vec<u8> = bytes.windows(3).filter(|w| w[0] == 0x99 && w[2] > 0).map(|w| w[1]).collect();
    assert_eq!(on, vec![42, 38, 36, 49]);
}

#[test]
fn a_tablature_puts_each_note_on_the_highest_string_that_reaches_it() {
    let eng = engrave("\\new TabStaff \\relative c { c4 e g c | <c, e g>1 | c'4\\5 r2. }", 900.0 / 8.0, &Layout::default()).unwrap();
    let bars = &eng.staves[0];
    let frets: Vec<Vec<(i64, i64)>> = bars[0].items.iter().map(|e| e.tab.clone()).collect();
    assert_eq!(frets, vec![vec![(5, 3)], vec![(4, 2)], vec![(3, 0)], vec![(2, 1)]], "LilyPond's 3 2 0 1");
    assert_eq!(bars[1].items[0].tab, vec![(3, 0), (4, 2), (5, 3)]);
    assert_eq!(bars[2].items[0].tab, vec![(5, 15)], "\\5 names the string");
}

#[test]
fn key_signatures_under_a_tenor_clef_start_low() {
    // LilyPond's alteration-positions: F sharp on the tenor clef's second
    // line, not over the staff.
    let eng = engrave("{ \\clef tenor \\key e \\major c'1 }", 900.0 / 8.0, &Layout::default()).unwrap();
    assert_eq!(eng.staves[0][0].clef, Clef::named("tenor").unwrap());
    let (drawn, _) = paths(&eng, 8.0, 0).unwrap();
    assert!(drawn.len() > 10);
}

#[test]
fn an_edit_leaves_what_it_cannot_rewrite() {
    use eui_score::edit::{item, NotEditable};
    assert_eq!(item("\\transpose c d { c'4 }", 0), Err(NotEditable::Transposed));
    assert_eq!(item("\\language \"english\" { cs'4 }", 0), Err(NotEditable::Language));
    assert_eq!(item("{ <c' e'>4 q }", 1), Ok(None), "q writes no pitch");
    assert_eq!(item("\\drums { hh4 }", 0), Ok(None));
    assert_eq!(item("\\chords { c1 }", 0), Ok(None));
    let it = item("{ \\ottava #1 c'''4 }", 0).unwrap().unwrap();
    assert_eq!(it.clef.octave, 1, "under \\ottava a line stands for a pitch an octave up");
    let it = item("melody = { c'4 d' }\n{ \\melody }", 1).unwrap().unwrap();
    assert_eq!(&"melody = { c'4 d' }"[it.spans[0].clone()], "d'", "a variable's note is rewritten where it is defined");
}
