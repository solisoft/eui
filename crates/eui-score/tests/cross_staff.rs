//! Cross-staff notation on a piano's two staves, as LilyPond draws it
//! (Notation Reference 2.2.1, "Changing staff manually", "Changing staff
//! automatically", "Staff-change lines", "Cross-staff stems"): a beam over
//! notes on both staves drawn once, between them, its stems reaching to it
//! from each side; slurs and ties drawn across; `\autoChange` splitting one
//! voice at middle C with rests where the other hand plays; the line
//! `\showStaffSwitch` draws; `\crossStaff`'s one stem over two staves; and
//! the music played once.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::{parse, Kind};
use eui_score::{engrave, midi, paths, Engraved, Layout, MidiOptions};

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{e}"))
}

fn piano(up: &str, down: &str) -> String {
    format!("\\new PianoStaff <<\n  \\new Staff = \"up\" {{ {up} }}\n  \\new Staff = \"down\" {{ \\clef bass {down} }}\n>>")
}

/// The notes a staff draws in a bar, in order.
fn notes(e: &Engraved, staff: usize, bar: usize) -> Vec<&eui_score::parse::Ev> {
    e.staves[staff][bar].items.iter().filter(|ev| ev.k == Kind::Note).collect()
}

/// Note-ons of a MIDI file, by channel.
fn note_ons(bytes: &[u8], channel: u8) -> usize {
    bytes.windows(3).filter(|w| w[0] == 0x90 + channel && w[2] > 0 && w[1] < 128).count()
}

#[test]
fn a_beam_over_both_staves_is_one_beam_between_them() {
    let src = piano(r#"c'8 \change Staff = "down" g e \change Staff = "up" c'' e''2"#, "s1");
    let e = eng(&src);
    let sys = &e.systems[0];
    assert_eq!(sys.kneed.len(), 1, "one kneed beam");
    let k = &sys.kneed[0];
    assert_eq!(k.stems.len(), 4, "over the four eighths");
    // Neither staff beams its share of it.
    assert!(e.staves[0][0].beams.is_empty() && e.staves[1][0].beams.is_empty(), "no half beams on either staff");
    assert!(notes(&e, 0, 0).iter().take(2).all(|n| n.xbeam) && notes(&e, 1, 0).iter().all(|n| n.xbeam));
    // The upper staff's notes stem down to it, the lower staff's up.
    let lower_top = sys.offsets[1];
    for &(staff, from, tip) in &k.stems {
        if staff == 0 {
            assert!(tip > from, "an upper note's stem goes down");
        } else {
            assert!(tip < from, "a lower note's stem goes up");
        }
        assert!((from - tip).abs() >= 2.4, "every stem at least two spaces and a half: {from} to {tip}");
    }
    // The beam lies between the staves.
    for y in [k.y0, k.y1] {
        assert!(y > 4.0 && y < lower_top, "between the staves: {y} (lower staff at {lower_top})");
    }
    // The upper note's flag is not drawn: the beam is its.
    assert!(!sys.cross.is_empty());
    assert!(!paths(&e, 8.0, 0).unwrap().0.is_empty());
}

#[test]
fn a_beam_that_stays_on_one_staff_is_that_staffs() {
    // Every note of the beam goes down: the lower staff beams them, kneed nowhere.
    let src = piano(r#"c''2 \change Staff = "down" c8 d e f"#, "s1");
    let e = eng(&src);
    assert!(e.systems[0].kneed.is_empty());
    assert_eq!(e.staves[1][0].beams.len(), 1, "the lower staff beams the four eighths");
    // Notes moved where the other hand plays keep clear of it, as a third voice.
    let src = piano(r#"\change Staff = "down" c'4 r2."#, "c4 r2.");
    let e = eng(&src);
    let moved = notes(&e, 1, 0).into_iter().find(|n| n.moved.is_some()).unwrap();
    assert!(!moved.free && moved.up, "stem up, over the left hand's note");
}

#[test]
fn a_voice_switches_staff_in_the_middle_of_a_beam_of_sixteenths() {
    let src = piano(r#"c'16 \change Staff = "down" g, a, b, \change Staff = "up" c' d' e' f' g'2"#, "s1");
    let e = eng(&src);
    let k = &e.systems[0].kneed;
    assert_eq!(k.len(), 1, "the first four sixteenths share one beam");
    assert_eq!(k[0].stems.len(), 4);
    // The second group stays on the upper staff, beamed there.
    assert_eq!(e.staves[0][0].beams.len(), 1);
    assert_eq!(e.staves[0][0].beams[0].n, 2, "two beams, sixteenths");
}

#[test]
fn slurs_phrasing_slurs_and_ties_cross_from_one_staff_to_the_other() {
    let src = piano(r#"c''8( \change Staff = "down" g4) \change Staff = "up" e'8 ~ \change Staff = "down" e'2 | \change Staff = "up" e'4\( d' \change Staff = "down" c' b,\)"#, "s1 s1");
    let e = eng(&src);
    // Three curves cross: the slur, the tie and the phrasing slur.
    assert_eq!(e.systems[0].cross.len(), 3, "a slur, a tie and a phrasing slur");
    // No staff draws a half of one: the upper staff has no curve left, the lower none either.
    for line in &e.systems[0].lines {
        assert!(line.curves.is_empty(), "a staff drew a piece of a curve that crosses");
    }
    // A tie between two notes of one staff is still that staff's.
    let e = eng(&piano(r#"c''2 ~ c''2"#, "s1"));
    assert_eq!(e.systems[0].lines[0].curves.len(), 1);
    assert!(e.systems[0].cross.is_empty());
}

#[test]
fn a_curve_that_crosses_over_a_line_break_is_cut_in_two() {
    let src = piano(r#"c''2.( \change Staff = "down" c4 \break c2) c2"#, "s1 s1");
    let e = eng(&src);
    assert_eq!(e.systems.len(), 2);
    assert_eq!(e.systems[0].cross.len(), 1, "to the end of the first line");
    assert_eq!(e.systems[1].cross.len(), 1, "from the start of the next");
}

#[test]
fn show_staff_switch_draws_a_line_and_only_where_the_voice_switches() {
    let shown = piano(r#"\showStaffSwitch c'4 d' \change Staff = "down" a, \change Staff = "up" c'"#, "s1");
    let e = eng(&shown);
    assert_eq!(e.systems[0].cross.len(), 2, "down, then back up");
    let hidden = piano(r#"\showStaffSwitch c'4 d' \hideStaffSwitch \change Staff = "down" a, \change Staff = "up" c'"#, "s1");
    assert!(eng(&hidden).systems[0].cross.is_empty());
    let none = piano(r#"c'4 d' \change Staff = "down" a, \change Staff = "up" c'"#, "s1");
    assert!(eng(&none).systems[0].cross.is_empty(), "no line unless asked");
    // A change to the staff the voice is already on is no switch.
    let same = piano(r#"\showStaffSwitch c'4 \change Staff = "up" d' e' f'"#, "s1");
    assert!(eng(&same).systems[0].cross.is_empty());
}

#[test]
fn cross_staff_joins_two_stems_into_one() {
    let src = "\\new PianoStaff <<\n  \\new Staff { <e' c'>4 r4 g'2 }\n  \\new Staff { \\clef bass \\stemUp \\crossStaff { <e g>4 } e,4 \\crossStaff { g2 } }\n>>";
    let e = eng(src);
    assert_eq!(e.systems[0].cross.len(), 2, "two joined stems");
    let lower = notes(&e, 1, 0);
    assert!(lower[0].span_stem && !lower[1].span_stem && lower[2].span_stem);
    let upper = notes(&e, 0, 0);
    assert!(upper[0].up, "the upper chord's stem points the joined one's way");
    // With no note on the next staff to join, the note keeps its own stem.
    let alone = "\\new PianoStaff <<\n  \\new Staff { r1 }\n  \\new Staff { \\clef bass \\stemUp \\crossStaff { e4 } r2. }\n>>";
    let e = eng(alone);
    assert!(e.systems[0].cross.is_empty());
    assert!(!notes(&e, 1, 0)[0].span_stem);
    // A single staff has none to cross to.
    assert!(eng("{ \\crossStaff { c'4 } }").systems[0].cross.is_empty());
}

#[test]
fn auto_change_splits_one_voice_at_middle_c_with_rests_where_the_other_hand_plays() {
    let src = r"\new PianoStaff \autoChange { g4 a b c' | d' c' b a | c'8 b a g a b c' d' | c'1 }";
    let p = parse(src).unwrap();
    assert_eq!(p.staves.len(), 2);
    assert_eq!(p.staves[0].name.as_deref(), Some("up"));
    assert_eq!(p.staves[1].name.as_deref(), Some("down"));
    assert_eq!(p.groups.len(), 1, "braced as a piano's staves");
    let e = eng(src);
    // Bar 1: g a b below middle C go down; c' stays up.
    let up: Vec<Kind> = e.staves[0][0].items.iter().filter(|ev| !ev.spacer).map(|ev| ev.k.clone()).collect();
    assert_eq!(up, vec![Kind::Rest, Kind::Rest, Kind::Rest, Kind::Note], "rests over the left hand's notes");
    let down: Vec<Kind> = e.staves[1][0].items.iter().filter(|ev| !ev.spacer).map(|ev| ev.k.clone()).collect();
    assert_eq!(down, vec![Kind::Note, Kind::Note, Kind::Note, Kind::Rest], "a rest under the right hand's c'");
    assert_eq!(e.staves[1][0].clef.sign, eui_score::parse::Clef::named("bass").unwrap().sign, "the lower staff in the bass clef");
    // The eighths that cross middle C share one beam between the staves.
    assert!(!e.systems[0].kneed.is_empty(), "a kneed beam where the voice crosses inside a beat group");
    // Played once, every note: the same notes as the voice on one staff.
    let one = midi(r"{ g4 a b c' | d' c' b a | c'8 b a g a b c' d' | c'1 }", &MidiOptions::default()).unwrap();
    let two = midi(src, &MidiOptions::default()).unwrap();
    assert_eq!(note_ons(&two, 0) + note_ons(&two, 1), note_ons(&one, 0));
    assert_eq!(note_ons(&two, 1), 0, "the lower staff sounds nothing of its own");
}

#[test]
fn auto_change_takes_a_reference_pitch_and_refuses_what_it_cannot_split() {
    let src = r"\new PianoStaff \autoChange d' { c'4 d' e' c' }";
    let e = eng(src);
    let up = e.staves[0][0].items.iter().filter(|ev| ev.k == Kind::Note).count();
    let down = notes(&e, 1, 0).len();
    assert_eq!((up, down), (2, 2), "d' and e' up, the c's under d' down");
    // A chord goes by its lowest note.
    let e = eng(r"\new PianoStaff \autoChange { <a c' e'>2 <c' e'>2 }");
    assert_eq!(notes(&e, 1, 0).len(), 1);
    // Without a PianoStaff: two staves, unbraced.
    let p = parse(r"\autoChange { c4 c' c c' }").unwrap();
    assert_eq!(p.staves.len(), 2);
    // Two voices in it, or a \change Staff of its own, are refused.
    assert!(parse(r"\new PianoStaff \autoChange { << { c'4 } \\ { c4 } >> }").is_err());
    assert!(engrave(r#"\new PianoStaff \autoChange { c'4 \change Staff = "down" c'4 }"#, 100.0, &Layout::default()).is_err());
    assert!(parse(r"\new PianoStaff \autoChange").is_err());
}

#[test]
fn the_music_sounds_once_whatever_staff_it_is_drawn_on() {
    let src = piano(r#"c'8 \change Staff = "down" g e \change Staff = "up" c'' e''2"#, "s1");
    let crossed = midi(&src, &MidiOptions::default()).unwrap();
    let plain = midi(&piano("c'8 g e c'' e''2", "s1"), &MidiOptions::default()).unwrap();
    assert_eq!(crossed, plain, "the same notes, the same track");
}
