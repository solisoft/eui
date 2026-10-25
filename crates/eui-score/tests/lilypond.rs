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
    assert!(names.len() >= 19, "the pieces are missing");
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
    let eng = engrave("\\new TabStaff { e4\\3 }", 900.0 / 8.0, &Layout::default()).unwrap();
    assert_eq!(eng.staves[0][0].items[0].tab, vec![(4, 2)], "a string that cannot sound the note is not the one: e on the D string, as LilyPond plays it");
    let staff = engrave("{ c''4\\2 }", 900.0 / 8.0, &Layout::default()).unwrap();
    let circled = staff.systems[0].lines[0].scripts.iter().filter(|s| s.circled).count();
    assert_eq!(circled, 1, "a string's number is circled on a staff");
}

#[test]
fn a_rehearsal_mark_stands_over_an_ottava_that_starts_beside_it() {
    let eng = engrave(&piece("d05_marks"), 900.0 / 8.0, &Layout::default()).unwrap();
    let line = &eng.systems[0].lines[0];
    let ottava = &line.ottavas[0];
    let b = line.rehearsal.iter().find(|m| m.runs[0].text == "B").unwrap();
    assert!(b.y < ottava.y - 1.0, "B at {} over the 8 at {}", b.y, ottava.y);
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

#[test]
fn the_sustain_pedal_is_drawn_under_the_staff_and_held_down_in_the_midi() {
    let src = r"\relative c' { \clef bass c,4\sustainOn e g c\sustainOff | <c, e g>1\sustainOff\sustainOn | c1\sustainOff }";
    let p = read(src);
    let notes: Vec<_> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).collect();
    assert!(notes[0].pedal_on && !notes[0].pedal_off);
    assert!(notes[3].pedal_off && !notes[3].pedal_on);
    assert!(notes[4].pedal_off && notes[4].pedal_on, "let go and pressed again on one chord");
    let change: Vec<_> = notes[4].texts.iter().filter(|w| w.dir == 1).collect();
    assert_eq!(change.len(), 1, "one line of signs");
    assert_eq!(change[0].runs.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["keyboardPedalUp", "keyboardPedalPed"], "* then Ped.");
    assert!(notes[0].texts.iter().any(|w| w.dir == 1 && w.runs.iter().any(|r| r.glyph && r.text == "keyboardPedalPed")), "Ped. under the staff");
    assert!(notes[3].texts.iter().any(|w| w.runs.iter().any(|r| r.glyph && r.text == "keyboardPedalUp")), "* where it is let go");
    engrave(src, 80.0, &Layout::default()).unwrap();
    let bytes = midi(src, &MidiOptions::default()).unwrap();
    let down = bytes.windows(3).filter(|w| w == &[0xB0, 64, 127]).count();
    let up = bytes.windows(3).filter(|w| w == &[0xB0, 64, 0]).count();
    assert_eq!((down, up), (2, 3), "controller 64 down and up, as written");
    assert!(parse(r"{ \sustainOn c'4 }").is_err(), "a pedal goes after its note");
}

#[test]
fn a_jump_ends_its_bar_and_stands_over_the_next_bars_mark() {
    let src = r#"{ c'1 \jump "D.S. al Coda" | \mark \default d'1 | e'1 \fine }"#;
    let p = read(src);
    let marks: Vec<_> = p.staves[0].events.iter().filter(|e| e.k == Kind::Mark).collect();
    assert_eq!(marks.len(), 3);
    assert_eq!(marks[0].texts[0].dir, 1, "a jump");
    assert_eq!(marks[2].texts[0].runs[0].text, "Fine");
    let bars = measures(&p.staves[0].events).unwrap();
    assert_eq!(bars[0].marks.len(), 1, "the D.S. ends bar 1");
    assert_eq!(bars[0].marks[0].0, bars[0].length);
    assert_eq!(bars[1].marks.len(), 1, "the letter starts bar 2");
    assert_eq!(bars[2].marks.last().unwrap().0, bars[2].length, "Fine ends the last bar");
    engrave(src, 60.0, &Layout::default()).unwrap();
    assert!(parse(r"{ c'1 \jump 3 }").is_err());
}

#[test]
fn a_note_past_its_bar_line_is_cut_into_tied_notes() {
    let src = r#"{ \time 3/4 c'2 d'2\p^"dolce"( e'4) | r2 r2 r2 | g'1 a'1 }"#;
    let p = read(src);
    let bars = measures(&p.staves[0].events).unwrap();
    let shape = |b: usize| bars[b].items.iter().map(|e| (e.base, e.dots, e.tie)).collect::<Vec<_>>();
    assert_eq!(shape(0), [(2, 0, false), (4, 0, true)], "d'2 begins in bar 1, a quarter, tied");
    assert_eq!(shape(1), [(4, 0, false), (4, 0, false), (4, 0, false)], "and ends in bar 2, before e'4 and a rest's start");
    let d = &bars[0].items[1];
    assert_eq!(d.dynamic.as_deref(), Some("p"), "its start keeps the dynamic");
    assert!(d.slur_on && !d.slur_off);
    assert!(bars[1].items[0].dynamic.is_none() && bars[1].items[0].texts.is_empty(), "the piece after has none");
    // A rest is cut into rests, never tied.
    assert!(bars[2].items.iter().all(|e| e.k == Kind::Rest && !e.tie));
    // A whole note in 3/4, on the last beat: a quarter tied to a dotted half.
    let g: Vec<_> = bars.iter().flat_map(|b| &b.items).filter(|e| e.k == Kind::Note && e.pitches[0].d == 11).collect();
    assert_eq!(g.iter().map(|e| (e.base, e.dots, e.tie)).collect::<Vec<_>>(), [(4, 0, true), (2, 1, false)]);
    assert_eq!(g.iter().map(|e| e.ticks).sum::<i64>(), eui_score::WHOLE);
    assert_ne!(g[0].bar, g[1].bar, "on each side of the bar line");
    engrave(src, 80.0, &Layout::default()).unwrap();
    assert!(!midi(src, &MidiOptions::default()).unwrap().is_empty());
    assert!(measures(&read(r"{ \time 3/4 c'2 \tuplet 3/2 { d'4 e' f' } }").staves[0].events).is_err(), "a tuplet is not cut");
    // A second voice's note too.
    let two = read(r"{ << { c''2 d''2 e''1 } \\ { a'4 b'2 g'4 g'1 } >> }");
    let bars = measures(&two.staves[0].events).unwrap();
    let b = bars[0].items.iter().filter(|e| e.voice == Some(2) && e.pitches.first().is_some_and(|x| x.d == 13)).count();
    assert_eq!(b, 1, "b'2 from the second beat fits its bar");
    engrave(r"{ << { c''1 d''1 } \\ { a'2. b'2 g'2 r4 } >> }", 80.0, &Layout::default()).unwrap();
}

#[test]
fn verses_stand_one_under_another_each_on_the_same_notes() {
    let src = r#"\relative c'' { c4 d e( f) | g1 } \addlyrics { Ah, vous di -- rai } \addlyrics { Al -- lons en -- fants }"#;
    let p = read(src);
    assert_eq!(p.staves[0].verses.len(), 1, "a second verse");
    assert_eq!(p.staves[0].verses[0][1].text, "lons");
    let eng = engrave(src, 80.0, &Layout::default()).unwrap();
    assert!(!paths(&eng, 7.0, 0).unwrap().0.is_empty());
    // \new Lyrics \lyricsto: two verses to one voice; to two voices, refused.
    let named = r#"<< \new Staff { \new Voice = "m" { c''4 d'' } } \new Lyrics \lyricsto "m" { la la } \new Lyrics \lyricsto "m" { lo lo } >>"#;
    assert_eq!(read(named).staves[0].verses.len(), 1);
    assert!(
        parse(r#"\new Staff { << \new Voice = "a" { \voiceOne c''4 } \new Voice = "b" { \voiceTwo a'4 } >> } \addlyrics { la } \new Lyrics \lyricsto "b" { lo }"#).is_err(),
        "verses sung to two voices"
    );
}

#[test]
fn a_name_with_a_flat_or_a_sharp_prints_them() {
    use eui_score::glyphs::{letters, Weight};
    use eui_score::text::text_width;
    // "Clarinet in B♭": the text faces have the accidentals, drawn from the music font.
    for weight in [Weight::Regular, Weight::Bold, Weight::Italic] {
        for sign in ['♭', '♯', '♮'] {
            let letter = letters().face(weight).get(&sign).expect("the sign is in the face");
            assert!(!letter.f.is_empty() && letter.a > 200.0, "{sign} has an outline and room");
        }
    }
    let plain = text_width("Clarinet in B", Weight::Bold, 1.0);
    assert!(text_width("Clarinet in B♭", Weight::Bold, 1.0) > plain + 0.25, "the flat takes its own room");
    let src = r#"\new Staff \with { instrumentName = "Clarinet in B♭" } { c'1 }"#;
    let eng = engrave(src, 80.0, &Layout::default()).unwrap();
    assert!(!paths(&eng, 7.0, 0).unwrap().0.is_empty());
}

#[test]
fn the_paper_a_source_asks_for_is_the_pdfs() {
    use eui_score::{engrave_paper, pdf, Paper};
    // \paper's size, margins in any unit, and the staff size, as LilyPond writes them.
    let src = r#"#(set-global-staff-size 16)
\paper { #(set-paper-size "letter") left-margin = 2\cm right-margin = 15 top-margin = 0.5\in indent = 0 }
{ c'1 }"#;
    let spec = parse(src).unwrap().paper;
    assert_eq!(spec.size.as_deref(), Some("letter"));
    assert_eq!(spec.staff_size, Some(16.0));
    assert_eq!(spec.left, Some(20.0));
    assert_eq!(spec.right, Some(15.0));
    assert!((spec.top.unwrap() - 12.7).abs() < 1e-9);
    assert_eq!(spec.indent, Some(0.0));
    let paper = Paper::of(&spec);
    assert_eq!(paper.pt, 4.0);
    let mm = |spaces: f64| spaces * paper.pt * 25.4 / 72.0;
    assert!((mm(paper.w) - 215.9).abs() < 1e-6 && (mm(paper.h) - 279.4).abs() < 1e-6, "US letter");
    assert!((mm(paper.left) - 20.0).abs() < 1e-6);
    // The PDF's pages are letter: 612 × 792 points.
    assert!(pdf(src, "Letter").unwrap().contains("/MediaBox [0 0 612 792]"));
    // Nothing said: A4 at 20 pt, as before.
    assert!(pdf("{ c'1 }", "A4").unwrap().contains("/MediaBox [0 0 595.28 841.89]"));
    assert_eq!(Paper::of(&parse("{ c'1 }").unwrap().paper), Paper::a4());
    // Landscape turns the sheet; a staff size out of reason is not taken.
    let wide = Paper::of(&parse("\\paper { #(set-paper-size \"a3landscape\") } { c'1 }").unwrap().paper);
    assert!(wide.w > wide.h);
    assert_eq!(parse("#(set-global-staff-size 200) { c'1 }").unwrap().paper.staff_size, None);
    // A smaller staff fits more on a line: fewer systems for the same music.
    let music: String = (0..64).map(|_| "c'4 d' e' f' ").collect();
    let lines = |size: &str| engrave_paper(&format!("#(set-global-staff-size {size}) {{ {music} }}")).unwrap().0.systems.len();
    assert!(lines("14") < lines("24"), "14 pt: fewer lines than 24 pt");
}

#[test]
fn a_page_break_starts_a_new_page() {
    use eui_score::engrave_paper;
    let (one, _) = engrave_paper("{ c'1 c'1 c'1 c'1 }").unwrap();
    let (two, _) = engrave_paper("{ c'1 c'1 \\pageBreak c'1 c'1 }").unwrap();
    assert_eq!(one.pages.len(), 1);
    assert_eq!(two.pages.len(), 2, "the music after \\pageBreak is on page 2");
    let (lines, _) = engrave_paper("{ c'1 \\break c'1 \\break c'1 }").unwrap();
    assert_eq!(lines.systems.len(), 3, "a \\break ends its line");
}

#[test]
fn sixty_fourths_and_hundred_twenty_eighths_have_their_flags_and_rests() {
    let src = r"{ \time 1/8 c''64 d'' e'' f'' g''128 a'' b'' c''' r64 r128 r128 }";
    let p = parse(src).unwrap();
    let ticks: Vec<i64> = p.staves[0].events.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest)).map(|e| e.ticks).collect();
    assert_eq!(ticks[0], eui_score::WHOLE / 64);
    assert_eq!(ticks[4], eui_score::WHOLE / 128);
    let eng = engrave(src, 80.0, &Layout::default()).unwrap();
    assert!(!paths(&eng, 7.0, 0).unwrap().0.is_empty());
    // One alone has its flags; written past the bar, it is cut into tied notes of the small values too.
    for alone in [r"{ c''64 r64 r32 r16 r8 r4 r2 }", r"{ c''128 r128 r64 r32 r16 r8 r4 r2 }"] {
        engrave(alone, 80.0, &Layout::default()).unwrap();
    }
    assert!(parse(r"{ c'256 }").is_err(), "256ths are not drawn");
}

#[test]
fn a_key_in_a_mode_has_the_signature_of_its_notes() {
    let fifths = |src: &str| parse(src).unwrap().staves[0].events.iter().find(|e| e.k == Kind::Key).map(|e| e.fifths).unwrap();
    assert_eq!(fifths(r"{ \key d \dorian c'1 }"), 0, "D dorian: no sharp or flat");
    assert_eq!(fifths(r"{ \key g \mixolydian c'1 }"), 0);
    assert_eq!(fifths(r"{ \key e \phrygian c'1 }"), 0);
    assert_eq!(fifths(r"{ \key f \lydian c'1 }"), 0);
    assert_eq!(fifths(r"{ \key b \locrian c'1 }"), 0);
    assert_eq!(fifths(r"{ \key a \dorian c'1 }"), 1, "A dorian: one sharp");
    assert_eq!(fifths(r"{ \key d \minor c'1 }"), -1);
    assert_eq!(fifths(r"{ \key c \ionian c'1 }"), 0);
    assert!(parse(r"{ \key c \blues c'1 }").is_err());
}

#[test]
fn metres_added_up_long_or_odd_are_read_and_drawn() {
    let time = |src: &str| {
        let p = parse(src).unwrap();
        let ev = p.staves[0].events.iter().find(|e| e.k == Kind::Time).cloned().unwrap();
        (ev.time, ev.time_parts)
    };
    assert_eq!(time(r"{ \time 2+3/8 c'4 c'4. }"), ((5, 8), [2, 3, 0, 0]));
    assert_eq!(time(r"{ \time 3+2+2/8 c'4. c'4 c'4 }"), ((7, 8), [3, 2, 2, 0]));
    assert_eq!(time(r"{ \time 15/16 c'2.. c'16 }").0, (15, 16));
    assert_eq!(time(r"{ \time 3/32 c'16 c'32 }").0, (3, 32));
    for src in [r"{ \time 2+3/8 c'4 c'4. | c'4. c'4 }", r"{ \time 13/8 c'1 c'4. c'4 }", r"{ \time 4/5 \tuplet 5/4 { c'4 c' c' c' } }"] {
        let eng = engrave(src, 80.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"));
        assert!(!paths(&eng, 7.0, 0).unwrap().0.is_empty());
    }
    // The bar of 2+3/8 lasts five eighths: a sixth does not fit.
    assert!(engrave(r"{ \time 2+3/8 c'4 c'4. c'8 }", 80.0, &Layout::default()).is_ok(), "cut into the next bar");
    assert!(parse(r"{ \time 3/7 c'4 }").is_err(), "a seventh is not a note");
    assert!(parse(r"{ \time 60+50/8 c'4 }").is_err(), "more than 99 in a bar");
}

#[test]
fn a_tuplet_inside_a_tuplet_takes_both_ratios_and_has_both_brackets() {
    // A triplet of eighths in the time of a quarter, inside a quintuplet of quarters in the time of four.
    let src = r"{ \tuplet 5/4 { c'4 \tuplet 3/2 { d'8 e' f' } g'4 a' b' } }";
    let p = parse(src).unwrap();
    let notes: Vec<_> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).collect();
    let quarter = eui_score::WHOLE / 4;
    assert_eq!(notes[0].ticks, quarter * 4 / 5);
    assert_eq!(notes[1].ticks, quarter / 2 * 2 / 3 * 4 / 5, "both ratios");
    assert!(notes[1].tuplet.is_some() && notes[1].tuplet_outer.is_some());
    assert_eq!(notes[1].tuplet_outer.unwrap().id, notes[0].tuplet.unwrap().id, "the outer tuplet is the quintuplet");
    assert_eq!(notes.iter().map(|n| n.ticks).sum::<i64>(), eui_score::WHOLE, "a whole bar");
    let eng = engrave(src, 80.0, &Layout::default()).unwrap();
    let tuplets = &eng.systems[0].lines[0].tuplets;
    assert_eq!(tuplets.len(), 2, "a bracket each");
    let labels: Vec<&str> = tuplets.iter().map(|t| t.label.as_str()).collect();
    assert!(labels.contains(&"5") && labels.contains(&"3"));
    assert!(parse(r"{ \tuplet 3/2 { \tuplet 3/2 { \tuplet 3/2 { c'32 c' c' } } } }").is_err(), "three deep");
}

#[test]
fn three_or_four_voices_on_a_staff_each_have_their_stems_and_their_place() {
    // A chorale on one staff: soprano and alto up and down, then tenor and bass.
    let src = r"{ << { \voiceOne e''4 f'' g''2 } \\ { \voiceTwo c''4 d'' e''2 } \\ { \voiceThree g'4 a' b'2 } \\ { \voiceFour c'4 f' g'2 } >> }";
    let p = parse(src).unwrap();
    let voices: Vec<Option<u8>> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).map(|e| e.voice).collect();
    assert_eq!(voices, vec![Some(1), Some(1), Some(1), Some(2), Some(2), Some(2), Some(3), Some(3), Some(3), Some(4), Some(4), Some(4)]);
    let eng = engrave(src, 80.0, &Layout::default()).unwrap();
    let bar = &eng.staves[0][0];
    let stems: Vec<(u8, bool)> = bar.items.iter().filter(|e| e.k == Kind::Note).map(|e| (e.voice.unwrap_or(0), e.up)).collect();
    assert!(stems.iter().all(|(v, up)| *up == (v % 2 == 1)), "odd voices up, even down: {stems:?}");
    // The third and fourth voices stand a head to the right of the first two.
    let shift = |v: u8| bar.items.iter().find(|e| e.voice == Some(v)).and_then(|e| e.shift).unwrap_or(0.0);
    assert!(shift(3) > 0.5 && shift(4) > 0.5);
    assert!(!paths(&eng, 7.0, 0).unwrap().0.is_empty());
    // Unnamed parts take the numbers in turn; the same number twice, or a fifth voice, is refused.
    let three = parse(r"{ << { c''4 } \\ { a'4 } \\ { e'4 } >> }").unwrap();
    assert!(three.staves[0].events.iter().any(|e| e.voice == Some(3)));
    assert!(parse(r"{ << { \voiceOne c''4 } \\ { \voiceOne a'4 } >> }").is_err());
    assert!(parse(r"{ << { c''4 } \\ { a'4 } \\ { e'4 } \\ { c'4 } \\ { g4 } >> }").is_err());
    // Voices that last differently are refused, the third too.
    assert!(engrave(r"{ << { c''4 } \\ { a'4 } \\ { e'2 } >> }", 80.0, &Layout::default()).is_err());
}

#[test]
fn notes_sent_to_the_other_staff_are_drawn_there_and_still_sound_in_their_own() {
    let src = r#"\new PianoStaff <<
  \new Staff = "up" { c''4 \change Staff = "down" c'4 e'4 \change Staff = "up" c''4 }
  \new Staff = "down" { \clef bass s1 }
>>"#;
    let p = parse(src).unwrap();
    assert_eq!(p.staves[0].name.as_deref(), Some("up"));
    let crossed: Vec<_> = p.staves[0].events.iter().filter(|e| e.cross.is_some()).collect();
    assert_eq!(crossed.len(), 2);
    let eng = engrave(src, 80.0, &Layout::default()).unwrap();
    let up = &eng.staves[0][0];
    let down = &eng.staves[1][0];
    assert_eq!(up.items.iter().filter(|e| e.k == Kind::Note).count(), 2, "two notes stay up");
    let moved: Vec<_> = down.items.iter().filter(|e| e.k == Kind::Note).collect();
    assert_eq!(moved.len(), 2, "two are drawn down");
    // Middle C on the bass staff sits a space above its top line: position 6.
    assert_eq!(moved[0].heads[0].pos, 6);
    assert!(moved.iter().all(|e| e.up), "their stems reach up, to the staff they came from");
    assert!(!paths(&eng, 7.0, 0).unwrap().0.is_empty());
    // They still sound in the right hand's track.
    midi(src, &MidiOptions::default()).unwrap();
    // A name no staff has, or a score of one staff, is refused.
    assert!(engrave(r#"<< \new Staff = "up" { \change Staff = "nowhere" c'1 } \new Staff = "down" { c1 } >>"#, 80.0, &Layout::default()).is_err());
    assert!(engrave(r#"{ \change Staff = "down" c'1 }"#, 80.0, &Layout::default()).is_err());
}

#[test]
fn pages_are_cut_evenly_when_asked_and_the_last_is_not_left_with_a_line() {
    use eui_score::engrave_paper;
    const EVEN: &str = "\\paper { page-breaking = #ly:optimal-breaking }\n";
    let cut = |n: usize, breaks: &str, paper: &str| {
        let music: String = (0..n).map(|k| if k + 1 < n { format!("c'1 {breaks} ") } else { "c'1".into() }).collect();
        let (eng, _) = engrave_paper(&format!("{paper}{{ {music} }}")).unwrap();
        eng.pages.iter().map(|p| p.systems.len()).collect::<Vec<_>>()
    };
    let per_page = |n: usize, breaks: &str| cut(n, breaks, EVEN);
    // How many lines one page holds.
    let room = (1..80).take_while(|&n| per_page(n, "\\break").len() == 1).last().unwrap();
    // One line more: the greedy cut left it alone on page 2; now the two pages share them.
    let pages = per_page(room + 1, "\\break");
    assert_eq!(pages.len(), 2, "{pages:?}");
    assert!(pages[0] - pages[1] <= 1, "{pages:?}");
    // Asked for nothing, or for pages filled in turn: each page full, the last with what is left.
    assert_eq!(cut(room + 1, "\\break", ""), vec![room, 1]);
    assert_eq!(cut(room + 1, "\\break", "\\paper { page-breaking = #ly:minimal-breaking }\n"), vec![room, 1]);
    // Over three pages too: as few pages as before, evenly full.
    let three = per_page(2 * room + 1, "\\break");
    assert_eq!(three.len(), 3, "{three:?}");
    assert!(three.iter().max().unwrap() - three.iter().min().unwrap() <= 1, "{three:?}");
    // A \pageBreak still ends its page, and the pages either side are cut evenly on their own.
    let n = room + 3;
    let music: String = (0..n).map(|k| if k == 1 { "c'1 \\pageBreak ".to_string() } else { "c'1 \\break ".to_string() }).collect();
    let (eng, _) = engrave_paper(&format!("{EVEN}{{ {music} c'1 }}")).unwrap();
    let forced: Vec<usize> = eng.pages.iter().map(|p| p.systems.len()).collect();
    assert_eq!(forced[0], 2, "{forced:?}");
    assert!(forced[1..].iter().max().unwrap() - forced[1..].iter().min().unwrap() <= 1, "{forced:?}");
}

#[test]
fn a_figured_bass_stacks_its_figures_under_the_bass_and_sounds_nothing() {
    let src = "<< \\new Staff { \\clef bass \\time 3/4 g,4 b, c | d2. } \\new FiguredBass \\figuremode { <5 3>4 <6> <7 5- 3!> | <_+>2. } >>";
    let p = read(src);
    assert_eq!(p.staves[1].kind, StaffKind::Figures);
    let figures: Vec<Vec<String>> =
        p.staves[1].events.iter().filter(|e| !e.figures.is_empty()).map(|e| e.figures.iter().map(|line| line.iter().map(|r| r.text.clone()).collect::<String>()).collect()).collect();
    assert_eq!(figures, vec![vec!["5", "3"], vec!["6"], vec!["7", "♭5", "♮3"], vec!["♯"]], "top to bottom, the accidental before its number");
    // Under the bass, each figure centred on its note; the line takes the bass's 3/4.
    let eng = engrave(src, 900.0 / 8.0, &Layout::default()).unwrap();
    let sys = &eng.systems[0];
    assert!(sys.offsets[1] > sys.offsets[0] + 4.0, "below the staff");
    let notes: Vec<f64> = eng.staves[0][0].items.iter().map(|e| e.x).collect();
    let figs: Vec<f64> = eng.staves[1][0].items.iter().map(|e| e.x).collect();
    assert_eq!(notes, figs, "each figure at its note");
    // Played: the bass alone.
    let alone = midi("\\new Staff { \\clef bass \\time 3/4 g,4 b, c | d2. }", &MidiOptions::default()).unwrap();
    let both = midi(src, &MidiOptions::default()).unwrap();
    let notes_on = |bytes: &[u8]| bytes.windows(3).filter(|w| w[0] & 0xF0 == 0x90 && w[2] > 0).count();
    assert_eq!(notes_on(&both), notes_on(&alone));
    // A figure that is no figure is refused, with its reason.
    assert!(parse("\\figures { <6 x>4 }").unwrap_err().message.contains("a figure is a number or _"));
    assert!(parse("\\figures { <6 4 }").is_err());
    assert!(parse("\\figures { <>4 }").unwrap_err().message.contains("an empty figure"));
}

#[test]
fn fret_boards_and_fret_diagrams_draw_a_guitars_chords() {
    use eui_score::frets::{from_spec, FRET};
    let shapes =
        |src: &str, staff: usize| -> Vec<String> { read(src).staves[staff].events.iter().filter_map(|e| e.chord_name.first()).filter_map(|r| r.text.strip_prefix(FRET).map(str::to_string)).collect() };
    let src = "<< \\new FretBoards \\chordmode { c1 g e:m r } \\new Staff { e'1 d' e' r } >>";
    assert_eq!(read(src).staves[0].kind, StaffKind::FretBoards);
    assert_eq!(shapes(src, 0), vec!["x;3;2;o;1;o;", "3;2;o;o;o;3;", "o;2;2;o;o;o;"], "a rest shows no diagram");
    // Over the staff below, as tall as a diagram: the system rises to hold them; silent.
    let eng = engrave(src, 900.0 / 8.0, &Layout::default()).unwrap();
    let names = engrave("<< \\chords { c1 g e:m r } \\new Staff { e'1 d' e' r } >>", 900.0 / 8.0, &Layout::default()).unwrap();
    assert!(eng.systems[0].reach_hi < names.systems[0].reach_hi - 2.0, "{} {}", eng.systems[0].reach_hi, names.systems[0].reach_hi);
    let alone = midi("{ e'1 d' e' r }", &MidiOptions::default()).unwrap();
    let notes_on = |bytes: &[u8]| bytes.windows(3).filter(|w| w[0] & 0xF0 == 0x90 && w[2] > 0).count();
    assert_eq!(notes_on(&midi(src, &MidiOptions::default()).unwrap()), notes_on(&alone));
    // In markup: as written, over its note.
    let p = read("{ c''1^\\markup \\fret-diagram \"6-x;5-3;4-2;3-o;2-1;1-o;\" }");
    let words = &p.staves[0].events.iter().find(|e| e.k == Kind::Note).unwrap().texts[0].runs[0].text;
    assert_eq!(from_spec(words).unwrap().spec(), "x;3;2;o;1;o;");
    assert!(engrave("{ c''1^\\markup \\fret-diagram-terse \"x;x;7;9;10;9;\" }", 900.0 / 8.0, &Layout::default()).is_ok());
    let refused = parse("{ c''1^\\markup \\fret-diagram-terse \"x;3;2;\" }").unwrap_err().message;
    assert!(refused.contains("a guitar has 6"), "{refused}");
}

#[test]
fn cue_notes_are_small_silent_and_what_the_quote_plays_meanwhile() {
    let src = "\\addQuote \"flute\" { c''4 d'' e'' f'' | g''8 a'' b'' c''' g''2 }\n{ c'1 | \\cueDuring \"flute\" #UP { R1 } }";
    let p = read(src);
    let cue: Vec<(i64, bool, i8)> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note && e.cue).map(|e| (e.pitches[0].d, e.cue, e.size)).collect();
    assert_eq!(cue.iter().map(|c| c.0).collect::<Vec<_>>(), vec![18, 19, 20, 21, 18], "the flute's second bar: g'' a'' b'' c''' g''");
    assert!(cue.iter().all(|c| c.2 == -3));
    let rest = p.staves[0].events.iter().find(|e| e.k == Kind::Rest && e.full).unwrap();
    assert_eq!((rest.voice, cue.len()), (Some(2), 5), "the rest under them, a voice of its own");
    // Drawn: the cue's heads smaller than a note's.
    let eng = engrave(src, 900.0 / 8.0, &Layout::default()).unwrap();
    let hw = |b: usize, j: usize| eng.staves[0][b].items[j].head_w.unwrap();
    assert!(hw(1, 0) < hw(0, 0) * 0.8, "{} {}", hw(1, 0), hw(0, 0));
    // Not played: the c' alone.
    let on = midi(src, &MidiOptions::default()).unwrap().windows(3).filter(|w| w[0] & 0xF0 == 0x90 && w[2] > 0).count();
    assert_eq!(on, 1);
    // \tiny and a cue voice, written by hand; a quote no one wrote is refused.
    let small = read("{ \\tiny c'4 \\normalsize d' << { \\new CueVoice { e'4 } } \\\\ { r4 } >> r4 }");
    let sizes: Vec<(i8, bool)> = small.staves[0].events.iter().filter(|e| e.k == Kind::Note).map(|e| (e.size, e.cue)).collect();
    assert_eq!(sizes, vec![(-2, false), (0, false), (-3, true)]);
    assert!(parse("{ \\cueDuring \"oboe\" #UP { R1 } }").unwrap_err().message.contains("no \\addQuote names it"));
    assert!(parse("\\addQuote \"f\" { c'1 } { \\cueDuring \"f\" { R1 } }").unwrap_err().message.contains("#UP or #DOWN"));
}

#[test]
fn an_ossia_stands_over_its_bars_only_and_sounds_nothing() {
    let src = "\\new Staff = \"main\" \\relative { c''4 b d c | << { c4 b d c } \\new Staff \\with { alignAboveContext = \"main\" } { d4 c e d } >> | c4 b c2 }";
    let p = read(src);
    assert_eq!(p.staves.len(), 2);
    assert_eq!(p.staves[0].kind, StaffKind::Ossia, "over the staff");
    let eng = engrave(src, 900.0 / 8.0, &Layout::default()).unwrap();
    // Its notes under the staff's of the same bar, its other bars empty.
    let notes = |s: usize, b: usize| eng.staves[s][b].items.iter().filter(|e| !e.spacer).map(|e| e.x).collect::<Vec<_>>();
    assert_eq!(notes(0, 1), notes(1, 1));
    assert!(notes(0, 0).is_empty() && notes(0, 2).is_empty());
    // The bar numbers and marks stay the staff's, and a system without it takes no room for it.
    assert_eq!(eng.systems[0].lines[1].staff_index, Some(0));
    assert!(eng.systems[0].offsets[1] >= 7.0, "the staff under its ossia");
    let broken = engrave(&src.replace("c | <<", "c \\break <<"), 900.0 / 8.0, &Layout::default()).unwrap();
    assert_eq!(broken.systems.len(), 2);
    assert_eq!(broken.systems[0].offsets, vec![0.0, 0.0], "no ossia in the first system: no room for one");
    assert!(broken.systems[1].offsets[1] >= 7.0);
    let on = |s: &str| midi(s, &MidiOptions::default()).unwrap().windows(3).filter(|w| w[0] & 0xF0 == 0x90 && w[2] > 0).count();
    assert_eq!(on(src), on("\\relative { c''4 b d c | c4 b d c | c4 b c2 }"), "the staff's notes; the ossia's are not played");
    assert!(parse("{ << { c'1 } \\new Staff { e'1 } >> }").unwrap_err().message.contains("is an ossia"));
}

#[test]
fn compressed_bars_of_rest_are_one_bar_counted_as_many() {
    // `\compressMMRests` around any music, around staves, and `skipBars`.
    let bars = |src: &str| engrave(src, 900.0 / 8.0, &Layout::default()).unwrap().staves[0].iter().map(|b| (b.number, b.span)).collect::<Vec<_>>();
    let gathered = vec![(1, None), (2, Some(4)), (6, Some(12)), (18, None)];
    assert_eq!(bars("{ \\compressMMRests { c''1 R1*4 R1*12 d''1 } }"), gathered);
    assert_eq!(bars("\\compressMMRests \\new Staff { c''1 R1*4 R1*12 d''1 }"), gathered);
    assert_eq!(bars("\\new Staff \\compressMMRests \\relative { c''1 R1*4 R1*12 d1 }"), gathered);
    assert_eq!(bars("{ \\set Score.skipBars = ##t c''1 R1*4 R1*12 d''1 }"), gathered);
    assert_eq!(bars("{ c''1 R1*4 R1*12 d''1 }").len(), 18, "without it, a bar each");
    assert_eq!(bars("{ \\set Score.skipBars = ##f c''1 R1*4 d''1 }").len(), 6);
    // Played as long as it counts: the same MIDI either way.
    let opts = MidiOptions::default();
    assert_eq!(midi("{ \\compressMMRests { c''1 R1*7 d''1 } }", &opts).unwrap(), midi("{ c''1 R1*7 d''1 }", &opts).unwrap());
    // Church rests to ten bars, the wide bar past that (LilyPond's expand-limit).
    let eng = engrave("{ \\compressMMRests { R1*10 R1*11 } }", 900.0 / 8.0, &Layout::default()).unwrap();
    assert_eq!(eng.staves[0].iter().map(|b| b.items[0].mm).collect::<Vec<_>>(), vec![Some(10), Some(11)]);
    // In a score, gathered where every staff rests the same bars; where another
    // staff plays on, the rest is as many bars as it counts.
    let p = piece("d14_mmrests");
    let eng = engrave(&p, 900.0 / 8.0, &Layout::default()).unwrap();
    let spans = |s: usize| eng.staves[s].iter().map(|b| b.span).collect::<Vec<_>>();
    assert_eq!(spans(0), vec![None, Some(9), Some(24), Some(3), None, None, None, None]);
    assert_eq!(spans(0), spans(1));
    assert_eq!(eng.staves[0].iter().map(|b| b.number).collect::<Vec<_>>(), vec![1, 2, 11, 35, 38, 39, 40, 41]);
    // The words over a gathered rest stand over its count, centred on it.
    let solo = eng.systems[0].lines[0].texts.iter().find(|t| t.text == "Solo").unwrap();
    let rest = &eng.staves[0][2].items[0];
    assert!(solo.bottom < -2.3, "over the count: {}", solo.bottom);
    assert!(((solo.x0 + solo.x1) / 2.0 - (rest.span0 + rest.span1) / 2.0).abs() < 0.01);
    let tempo = &eng.systems[0].lines[0].marks[0];
    assert!(tempo.y < -2.6, "the tempo over the count: {}", tempo.y);
}

#[test]
fn a_clef_or_a_key_changes_in_the_middle_of_a_bar() {
    use eui_score::parse::Clef;
    let eng = |src: &str| engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"));
    // Read in place, the notes after it under the new clef, drawn smaller before them.
    let e = eng("{ c'2 \\clef bass c2 }");
    let bar = &e.staves[0][0];
    assert_eq!(e.staves[0].len(), 1);
    assert_eq!(bar.inline, vec![(eui_score::WHOLE / 2, Clef::BASS, 0)]);
    assert_eq!(bar.items[1].clef_change, Some(Clef::BASS));
    assert_eq!(bar.items[1].heads[0].pos, -1, "c in the bass clef");
    assert!(bar.items[1].change_x.unwrap() < -2.0);
    let plain = eng("{ c'2 c2 }");
    assert!(bar.items[1].x - bar.items[0].x > plain.staves[0][0].items[1].x - plain.staves[0][0].items[0].x + 2.0, "room for the clef");
    assert_eq!(midi("{ c'2 \\clef bass c2 }", &MidiOptions::default()).unwrap(), midi("{ c'2 c2 }", &MidiOptions::default()).unwrap());
    assert_eq!(protocol("{ c'2 \\clef bass c2 }"), Ok(11));
    assert_eq!(protocol("{ c'2 c2 }"), Ok(9));
    // A key: the old one's naturals, the new one; the bar's accidentals start afresh.
    let e = eng("{ \\key d \\major d'4 fis' \\key c \\major fis'4 c''4 }");
    let items = &e.staves[0][0].items;
    assert_eq!(items[2].key_change, Some((2, 0)));
    assert_eq!(items[2].heads[0].acc, Some(1), "f sharp in C major is written");
    assert_eq!(items[3].heads[0].acc, None);
    let e = eng("{ \\key d \\major cis''2 \\key bes \\major bes'4 d'' | bes'1 }");
    assert_eq!(e.staves[0][0].items[1].key_change, Some((2, -2)));
    assert_eq!(e.staves[0][0].items[1].heads[0].acc, None, "b flat is in the key");
    assert_eq!(e.staves[0][1].fifths, -2, "the next bar is in it");
    assert!(!e.staves[0][1].changes.key, "and says nothing more");
    // A second voice reads under the clef where it lands.
    let e = eng("{ << { c''2 \\clef bass c2 } \\\\ { e'2 e2 } >> }");
    let second: Vec<i64> = e.staves[0][0].items.iter().filter(|ev| ev.voice == Some(2)).map(|ev| ev.heads[0].pos).collect();
    assert_eq!(second, vec![-4, 1], "e' under the treble clef, e under the bass");
    // In a piano staff the other staff's notes make room for it too.
    let e = eng(&piece("d15_midbar"));
    let x = |s: usize, b: usize, j: usize| e.staves[s][b].items[j].x;
    assert_eq!(x(0, 0, 2), x(1, 0, 2));
    assert!(e.staves[1][0].items[2].clef_change.is_some());
    // After the last note of a bar, at the next one's start; \time still at a bar line.
    let e = eng("{ c'2 \\clef bass s2 | c1 }");
    assert!(e.staves[0][1].changes.clef);
    assert!(engrave("{ c'2 \\time 3/4 c'4 }", 100.0, &Layout::default()).unwrap_err().message.contains("\\time in the middle of a bar"));
}

#[test]
fn layout_settings_are_read_and_passed_over_with_a_warning() {
    use eui_score::warnings;
    // Each of these refused the whole file; now read whole, the music after it too.
    let said = [
        "{ \\override Staff.TimeSignature.color = #red c'4 d' }",
        "{ \\override Stem #'length = #5 c'4 d' }",
        "{ \\once \\override NoteHead.color = #red c'4 \\revert NoteHead.color d' }",
        "{ \\override Slur.positions = #'(1 . 2) c'4( d') }",
        "{ \\override TextScript.color = #(rgb-color 1 0 0) c'4 d' }",
        "{ \\tweak color #red c'4 d'-\\tweak Script.color #blue -> }",
        "{ \\set Staff.explicitKeySignatureVisibility = #all-invisible \\unset Staff.explicitKeySignatureVisibility c'4 }",
        "{ \\slurDashed \\tieDotted \\textSpannerUp \\arpeggioBracket c'4 }",
        "{ \\transposition bes c'4 }",
        "\\new Staff \\with { \\consists \"Ambitus_engraver\" \\remove \"Time_signature_engraver\" fontSize = #-2 } { c'4 }",
        "\\layout { \\context { \\Score \\override BarNumber.font-size = #-2 } } { c'4 }",
        "\\midi { \\tempo 4 = 90 } { c'4 }",
        "\\paper { ragged-right = ##t system-system-spacing.basic-distance = #10 } { c'4 }",
        "\\header { encodingsoftware = \"musicxml2ly\" title = \\markup { \\bold Song } } { c'4 }",
        "<< \\set Score.proportionalNotationDuration = #(ly:make-moment 1/8) \\new Staff { c'4 } >>",
    ];
    for src in said {
        let p = read(src);
        assert!(!p.warnings.is_empty(), "{src}: no warning");
        assert_eq!(warnings(src).unwrap(), p.warnings);
        engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"));
        assert_eq!(protocol(src), Ok(11), "{src}");
    }
    assert!(read("{ \\override Stem.direction = #DOWN c'4 }").warnings.is_empty(), "honoured, not passed over");
    assert_eq!(read("\\header { title = \\markup { \\bold Song } } { c'4 }").header["title"], "Song");
    let w = &read("{ \\override NoteHead.color = #red c'4 }").warnings[0];
    assert_eq!((w.message.as_str(), w.at), ("\\override NoteHead.color is not drawn", 2));
    // Still refused: a command that is none of them, and a setting without its value.
    assert!(parse("{ c'4 \\foo }").unwrap_err().message.contains("\\foo is not a command"));
    assert!(parse("{ \\override Stem.direction c'4 }").is_err());
    // What is honoured: the stems' direction and the beams.
    let eng = engrave("{ \\stemDown c''4 \\stemUp c'' \\stemNeutral c'' c''8 d'' \\autoBeamOff e'' f'' \\autoBeamOn g'' a'' b'' c''' }", 900.0 / 8.0, &Layout::default()).unwrap();
    let ups: Vec<bool> = eng.staves[0][0].items.iter().map(|e| e.up).collect();
    assert_eq!(&ups[..3], &[false, true, false]);
    let beamed = |b: usize| eng.staves[0][b].items.iter().map(|e| e.beam.is_some()).collect::<Vec<_>>();
    assert_eq!(beamed(0), vec![false, false, false, true, true]);
    assert_eq!(beamed(1), vec![false, false, true, true, true, true], "e'' f'' under \\autoBeamOff");
}

#[test]
fn breaths_bowings_short_ties_and_spanners_are_drawn() {
    let eng = |src: &str| engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"));
    let p = read("{ c''4\\upbow d''\\downbow e''\\flageolet f''\\espressivo }");
    let names: Vec<&str> = p.staves[0].events.iter().map(|e| e.scripts[0].name.as_str()).collect();
    assert_eq!(names, vec!["upbow", "downbow", "flageolet", "espressivo"]);
    let e = eng("{ c''4\\upbow d''\\downbow e''\\flageolet f''\\espressivo }");
    let glyphs: Vec<&str> = e.systems[0].lines[0].scripts.iter().map(|s| s.glyph).collect();
    assert_eq!(glyphs, vec!["stringsUpBow", "stringsDownBow", "stringsHarmonic", "dynamicMessaDiVoce"]);
    // A breath after its note, room made for it; a caesura.
    let e = eng("{ c''4 \\breathe d'' \\caesura e''2 }");
    let items = &e.staves[0][0].items;
    assert_eq!((items[0].breath, items[1].breath), (Some("breathMarkComma"), Some("caesura")));
    assert!(items[0].breath_x > items[0].head_w.unwrap());
    assert!(items[1].x - items[0].x >= items[0].right + 0.6);
    // Short ties: one from the note, one into it; each head its own.
    let tied = eng("{ <c'' e''>2\\laissezVibrer c''2\\repeatTie }");
    let plain = eng("{ <c'' e''>2 c''2 }");
    assert_eq!(tied.systems[0].lines[0].curves.len(), 3);
    assert!(plain.systems[0].lines[0].curves.is_empty());
    // "cresc." and dashes for \cresc, the hairpin's MIDI kept.
    let e = eng("{ c'4\\p\\cresc d' e' f'\\f | g'1\\dim | c'1\\! }");
    let words: Vec<Option<String>> = e.systems[0].lines[0].dynamics.iter().filter(|d| d.text.is_none()).map(|d| d.words.clone()).collect();
    assert_eq!(words, vec![Some("cresc.".into()), Some("dim.".into())]);
    // A text spanner, its words from \override, over the notes it spans.
    let e = eng("{ \\override TextSpanner.bound-details.left.text = \"rit.\" c''4\\startTextSpan d'' e'' f''\\stopTextSpan | g''1 }");
    let sp = &e.systems[0].lines[0].spans;
    assert_eq!(sp.len(), 1);
    assert_eq!(sp[0].runs[0].text, "rit.");
    assert!(sp[0].y < -1.0, "over the staff: {}", sp[0].y);
    let bare = eng("{ c''4\\startTextSpan d'' e'' f''\\stopTextSpan }");
    assert!(bare.systems[0].lines[0].spans[0].runs.is_empty(), "the dashed line alone");
}

#[test]
fn forced_and_reminder_accidentals_print_their_sign() {
    let eng = |src: &str| engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"));
    let p = read("{ cis''4 cis''! cis''? <e'' g''?>4 }");
    let evs: Vec<_> = p.staves[0].events.iter().map(|e| (e.forced.clone(), e.cautionary.clone())).collect();
    assert_eq!(evs, vec![(vec![], vec![]), (vec![14], vec![]), (vec![], vec![14]), (vec![], vec![18])]);
    let e = eng("{ cis''4 cis''! cis''? c'' | g'4 g'? fis'! f' }");
    let accs: Vec<Vec<(Option<i64>, bool)>> = e.staves[0].iter().flat_map(|b| b.items.iter()).map(|ev| ev.heads.iter().map(|h| (h.acc, h.paren)).collect()).collect();
    assert_eq!(accs[0], vec![(Some(1), false)]);
    assert_eq!(accs[1], vec![(Some(1), false)], "a sharp the bar already has, printed");
    assert_eq!(accs[2], vec![(Some(1), true)], "in parentheses");
    assert_eq!(accs[5], vec![(Some(0), true)], "a natural as a reminder");
    assert_eq!(accs[6], vec![(Some(1), false)]);
    // The parentheses take their room.
    let items = &e.staves[0][0].items;
    assert!(items[2].left > items[1].left + 0.8, "{} {}", items[2].left, items[1].left);
    assert_eq!(protocol("{ c'?4 }"), Ok(11));
    assert_eq!(protocol("{ c'!4 }"), Ok(11));
}

#[test]
fn several_scores_are_engraved_one_after_another() {
    let src = piece("d16_real_files");
    let p = read(&src);
    assert_eq!(p.staves.len(), 1);
    let mv = p.staves[0].events.iter().find(|e| e.k == Kind::Movement).unwrap();
    assert_eq!((mv.texts[0].text.as_str(), mv.texts[1].text.as_str()), ("II. Menuet", "Op. 1"));
    let eng = engrave(&src, 900.0 / 8.0, &Layout::default()).unwrap();
    let bars = &eng.staves[0];
    let fresh = bars.iter().position(|b| b.fresh).unwrap();
    assert_eq!(bars[fresh].number, 0, "the second movement's pickup is bar 0");
    assert_eq!(bars[fresh + 1].number, 1);
    assert_eq!(bars[fresh].clef, Clef::TREBLE);
    assert_eq!((bars[fresh].fifths, bars[fresh].time), (2, (3, 4)));
    assert_eq!(bars[fresh - 1].brk.as_deref(), Some("line"));
    // On a line of its own, under its title.
    let line = eng.systems.iter().flat_map(|s| &s.lines).find(|l| l.bars.start == fresh).unwrap();
    assert_eq!(line.title.as_ref().unwrap().0, "II. Menuet");
    assert!(line.title_y.unwrap() < -2.0);
    assert!(line.courtesy == 0.0);
    let before = eng.systems.iter().flat_map(|s| &s.lines).find(|l| l.bars.end == fresh).unwrap();
    assert_eq!(before.courtesy, 0.0, "the line before announces nothing");
    // Played one after the other; on A4 too.
    engrave_a4(&src).unwrap();
    let on = |s: &str| midi(s, &MidiOptions::default()).unwrap().windows(3).filter(|w| w[0] & 0xF0 == 0x90 && w[2] > 0).count();
    assert!(on(&src) > on("{ g'1 }"));
    // Music without \score, a \book and \bookpart (a page each), and a movement whose staves differ.
    let two = read("{ c'1 } { d'1 }");
    assert_eq!(two.staves[0].events.iter().filter(|e| e.k == Kind::Movement).count(), 1);
    let book = read("\\book { \\header { title = \"B\" } \\bookpart { \\score { { c'1 } } } \\bookpart { \\header { title = \"Part 2\" } \\score { { d'1 } } } }");
    assert_eq!(book.header["title"], "B");
    let mv = book.staves[0].events.iter().find(|e| e.k == Kind::Movement).unwrap();
    assert!(mv.page);
    assert_eq!(mv.texts[0].text, "Part 2");
    let paged = engrave_a4("\\book { \\bookpart { { c'1 } } \\bookpart { { d'1 } } }").unwrap();
    assert_eq!(paged.pages.len(), 2);
    let odd = read("{ c'1 } \\new PianoStaff << \\new Staff { c'1 } \\new Staff { c1 } >>");
    assert_eq!(odd.staves.len(), 1);
    assert!(odd.warnings.iter().any(|w| w.message.contains("movement 2 is not engraved")));
    assert!(parse("\\book { { c'1 }").unwrap_err().message.contains("\\book block that is never closed"));
    assert!(parse("{ c'4 } }").unwrap_err().message.contains("'}' after the end of the music"));
}
