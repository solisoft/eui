//! An instrument changed in the middle of a staff (score #1023): `\set
//! Staff.instrumentName`, `shortInstrumentName` and `midiInstrument`, and
//! `\transposition`, hold from where they are written — the systems after
//! take the new names, the MIDI changes its program and sounds the new
//! instrument's pitch from there.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::parse;
use eui_score::{engrave, midi, warnings, Layout, MidiOptions};

/// A quarter, in a file's ticks.
const QUARTER: u32 = 6720;

/// A Standard MIDI File's channel messages: (tick, status, first byte).
fn messages(bytes: &[u8]) -> Vec<(u32, u8, u8)> {
    let mut out = Vec::new();
    let mut i = 14;
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]]) as usize;
        let track = &bytes[i + 8..i + 8 + len];
        i += 8 + len;
        let (mut at, mut k, mut status) = (0u32, 0usize, 0u8);
        while k < track.len() {
            let mut delta = 0u32;
            loop {
                let b = track[k];
                k += 1;
                delta = (delta << 7) | u32::from(b & 0x7f);
                if b & 0x80 == 0 {
                    break;
                }
            }
            at += delta;
            if track[k] & 0x80 != 0 {
                status = track[k];
                k += 1;
            }
            match status & 0xf0 {
                0x80 | 0x90 | 0xa0 | 0xb0 | 0xe0 => {
                    out.push((at, status, track[k]));
                    k += 2;
                }
                0xc0 | 0xd0 => {
                    out.push((at, status, track[k]));
                    k += 1;
                }
                _ => {
                    if status == 0xff {
                        k += 1;
                    }
                    let mut l = 0usize;
                    loop {
                        let b = track[k];
                        k += 1;
                        l = (l << 7) | usize::from(b & 0x7f);
                        if b & 0x80 == 0 {
                            break;
                        }
                    }
                    k += l;
                }
            }
        }
    }
    out
}

fn file(src: &str) -> Vec<(u32, u8, u8)> {
    messages(&midi(src, &MidiOptions::default()).unwrap_or_else(|e| panic!("{src}: {e}")))
}

/// The keys struck, each with the quarter it starts on.
fn struck(src: &str) -> Vec<(u32, u8)> {
    let mut on: Vec<(u32, u8)> = file(src).into_iter().filter(|m| m.1 & 0xf0 == 0x90).map(|m| (m.0 / QUARTER, m.2)).collect();
    on.sort_unstable();
    on
}

/// The programs set, each with the quarter it is set on.
fn programs(src: &str) -> Vec<(u32, u8)> {
    file(src).into_iter().filter(|m| m.1 & 0xf0 == 0xc0).map(|m| (m.0 / QUARTER, m.2)).collect()
}

/// The name left of the staff, a system each, in a score of two staves
/// (a staff alone has no name drawn).
fn names(staff: &str) -> Vec<String> {
    let src = &format!("<< {staff} \\new Staff {{ c'1 }} >>");
    let eng = engrave(src, 120.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"));
    eng.systems.iter().map(|s| s.names[0].clone()).collect()
}

const FLUTE: &str = "\\new Staff \\with { instrumentName = \"Flute\" shortInstrumentName = \"Fl.\" midiInstrument = \"flute\" }";
const TO_PICCOLO: &str = "\\set Staff.instrumentName = \"Piccolo\" \\set Staff.shortInstrumentName = \"Picc.\" \\set Staff.midiInstrument = \"piccolo\" \\transposition c''";
const TO_FLUTE: &str = "\\set Staff.instrumentName = \"Flute\" \\set Staff.shortInstrumentName = \"Fl.\" \\set Staff.midiInstrument = \"flute\" \\transposition c'";

#[test]
fn a_flute_takes_the_piccolo_in_the_middle_of_its_staff() {
    // Bar 1 the flute; the piccolo from bar 2's third beat; the flute again at bar 4.
    let src = format!("{FLUTE} {{ c''4 d'' e'' f'' | c''4 d'' {TO_PICCOLO} c''4 d'' | \\break c''1 | \\break {TO_FLUTE} c''1 }}");
    // The program changes where the piccolo is taken, and back.
    assert_eq!(programs(&src), vec![(0, 73), (6, 72), (12, 73)]);
    // The piccolo sounds an octave over what is written, from there and no sooner.
    assert_eq!(struck(&src), vec![(0, 72), (1, 74), (2, 76), (3, 77), (4, 72), (5, 74), (6, 84), (7, 86), (8, 84), (12, 72)]);
    // The first system names the flute; the one after the change, the
    // piccolo in short; the one a change starts, the new name in full.
    assert_eq!(names(&src), vec!["Flute", "Picc.", "Flute"]);
    assert!(warnings(&src).unwrap().is_empty(), "{:?}", warnings(&src));
    // The staff is still the flute's: what it says where it starts.
    let p = parse(&src).unwrap();
    assert_eq!(p.staves[0].props.instrument_name.as_deref(), Some("Flute"));
    assert_eq!(p.staves[0].props.midi_instrument.as_deref(), Some("flute"));
    assert_eq!(p.staves[0].props.transposition, 0);
    // Drawn as written: the piccolo's notes where the flute's were.
    let written: Vec<i64> = p.staves[0].events.iter().filter(|e| e.k == eui_score::parse::Kind::Note).map(|e| e.pitches[0].d).collect();
    assert_eq!(written[4..6], written[6..8]);
}

#[test]
fn a_clarinet_in_b_flat_takes_the_one_in_a() {
    let src = "\\new Staff \\with { instrumentName = \"Clarinet in B♭\" shortInstrumentName = \"Cl. (B♭)\" midiInstrument = \"clarinet\" } \
               { \\transposition bes c''4 d'' e'' f'' | \\break \\set Staff.instrumentName = \"Clarinet in A\" \\set Staff.shortInstrumentName = \"Cl. (A)\" \\transposition a c''4 d'' e'' f'' | \\break c''1 }";
    // A tone under what is written, then a minor third.
    assert_eq!(struck(src), vec![(0, 70), (1, 72), (2, 74), (3, 75), (4, 69), (5, 71), (6, 73), (7, 74), (8, 69)]);
    // One instrument all along: its program is set once.
    assert_eq!(programs(src), vec![(0, 71)]);
    assert_eq!(names(src), vec!["Clarinet in B♭", "Clarinet in A", "Cl. (A)"]);
    // What is written before the first note is the staff's own.
    assert_eq!(parse(src).unwrap().staves[0].props.transposition, -2);
}

#[test]
fn a_setting_holds_from_where_it_is_written_and_not_before() {
    // No instrument named at the start: the host's until one is.
    let opts = MidiOptions { program: 40, ..MidiOptions::default() };
    let late = "{ c'4 d' \\set Staff.midiInstrument = \"flute\" e' f' }";
    let set: Vec<(u32, u8)> = messages(&midi(late, &opts).unwrap()).into_iter().filter(|m| m.1 & 0xf0 == 0xc0).map(|m| (m.0 / QUARTER, m.2)).collect();
    assert_eq!(set, vec![(0, 40), (2, 73)]);
    // Before the first note, it is the whole staff's, as it was.
    let p = parse("{ \\set Staff.instrumentName = \"Oboe\" \\set Staff.shortInstrumentName = \"Ob.\" \\set Staff.midiInstrument = \"oboe\" c'1 }").unwrap();
    assert_eq!(
        (p.staves[0].props.instrument_name.as_deref(), p.staves[0].props.short_instrument_name.as_deref(), p.staves[0].props.midi_instrument.as_deref()),
        (Some("Oboe"), Some("Ob."), Some("oboe"))
    );
    assert_eq!(programs("{ \\set Staff.midiInstrument = \"oboe\" c'1 }"), vec![(0, 68)]);
    // A name written late is not the first system's.
    assert_eq!(names("\\new Staff { c'1 | \\set Staff.instrumentName = \"Oboe\" \\set Staff.shortInstrumentName = \"Ob.\" c'1 | \\break c'1 }"), vec!["", "Ob."]);
    // An instrument this does not know changes no program.
    assert_eq!(programs("{ c'4 \\set Staff.midiInstrument = \"kazoo\" c'4 }"), vec![(0, 0)]);
    // The same one named again says nothing more.
    assert_eq!(programs("{ \\set Staff.midiInstrument = \"flute\" c'4 \\set Staff.midiInstrument = \"flute\" c'4 }"), vec![(0, 73)]);
}

#[test]
fn a_change_in_one_voice_is_the_staffs_at_that_moment() {
    // The first voice takes the piccolo on its second note: the second
    // voice's first note, read after it, sounds before the change.
    let src = "\\new Staff << { c''4 \\transposition c'' \\set Staff.midiInstrument = \"piccolo\" c''4 } \\\\ { c'4 c'4 } >>";
    assert_eq!(struck(src), vec![(0, 60), (0, 72), (1, 72), (1, 84)]);
    assert_eq!(programs(src), vec![(0, 0), (1, 72)]);
    // Another staff keeps its own.
    let two = "<< \\new Staff { c''4 \\transposition c'' c''4 } \\new Staff { c'4 c'4 } >>";
    assert_eq!(struck(two), vec![(0, 60), (0, 72), (1, 60), (1, 84)]);
    // A drum staff is not transposed.
    assert_eq!(struck("\\new DrumStaff \\drummode { bd4 \\transposition c'' bd4 }"), vec![(0, 36), (1, 36)]);
}
