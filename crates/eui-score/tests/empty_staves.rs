//! Empty staves hidden (score #404): `\RemoveEmptyStaves` in a `\layout`'s
//! `\context { \Staff }` or a staff's `\with`, `\RemoveAllEmptyStaves`, and
//! `VerticalAxisGroup.remove-empty` / `remove-first` as overrides — a staff
//! whose bars on a system are all rests is not drawn there, the first system
//! excepted, its group's brace or bracket shrinking with it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::engrave::Engraved;
use eui_score::{bands, engrave, midi, parse::parse, paths, Layout, MidiOptions};

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// Which staves each system draws.
fn shown(e: &Engraved) -> Vec<Vec<bool>> {
    e.systems.iter().map(|s| s.lines.iter().map(|l| !l.hidden).collect()).collect()
}

/// Two staves over three systems: the second rests through the second
/// system, the first through the third.
fn two(setting_layout: &str, with_one: &str, with_two: &str) -> String {
    format!(
        "\\score {{ << \\new Staff \\with {{ instrumentName = \"Flute\" shortInstrumentName = \"Fl.\" {with_one} }} {{ c''1 c'' \\break c'' c'' \\break R1*2 }} \\new Staff \\with {{ instrumentName = \"Oboe\" shortInstrumentName = \"Ob.\" {with_two} }} {{ R1*2 \\break R1*2 \\break e''1 e'' }} >> {setting_layout} }}"
    )
}

#[test]
fn an_empty_staff_is_left_out_of_its_system_but_never_the_first() {
    let plain = eng(&two("", "", ""));
    assert_eq!(shown(&plain), vec![vec![true, true]; 3]);
    let layout = "\\layout { \\context { \\Staff \\RemoveEmptyStaves } }";
    let e = eng(&two(layout, "", ""));
    assert!(parse(&two(layout, "", "")).unwrap().warnings.is_empty());
    assert_eq!(shown(&e), vec![vec![true, true], vec![true, false], vec![false, true]], "the first system keeps both");
    // The staff left out takes no room: the next stands where it stood.
    assert_eq!(e.systems[1].offsets, vec![0.0, 0.0]);
    assert!(e.systems[0].offsets[1] > 8.0);
    assert!(e.systems[1].reach.unwrap().1 < plain.systems[1].reach.unwrap().1 - 5.0);
    // Its names: the short ones after the first system, the left-out staff's not drawn.
    assert_eq!(e.systems[1].names, vec!["Fl.".to_owned(), "Ob.".to_owned()]);
    let with_names = paths(&e, 8.0, 0).unwrap().0.len();
    assert!(with_names < paths(&plain, 8.0, 0).unwrap().0.len());
    // \RemoveAllEmptyStaves: the first system too.
    let all = eng(&two("\\layout { \\context { \\Staff \\RemoveAllEmptyStaves } }", "", ""));
    assert_eq!(shown(&all), vec![vec![true, false], vec![true, false], vec![false, true]]);
    // In one staff's \with: that staff only.
    let one = eng(&two("", "", "\\RemoveEmptyStaves"));
    assert_eq!(shown(&one), vec![vec![true, true], vec![true, false], vec![true, true]], "the flute keeps its rests");
    // As overrides, in the \with or in the music.
    let over = eng(&two("", "", "\\override VerticalAxisGroup.remove-empty = ##t \\override VerticalAxisGroup.remove-first = ##t"));
    assert_eq!(shown(&over), vec![vec![true, false], vec![true, false], vec![true, true]]);
    let off = eng(&two("\\layout { \\context { \\Staff \\RemoveEmptyStaves \\override VerticalAxisGroup.remove-empty = ##f } }", "", ""));
    assert_eq!(shown(&off), vec![vec![true, true]; 3]);
}

#[test]
fn spacers_count_as_empty_and_one_note_keeps_a_staff() {
    let src = "\\layout { \\context { \\Staff \\RemoveEmptyStaves } } << \\new Staff { c''1 \\break c'' \\break c'' } \\new Staff { c'1 \\break s1 \\break r2 c'2 } >>";
    assert_eq!(shown(&eng(src)), vec![vec![true, true], vec![true, false], vec![true, true]]);
    // Never every staff: a system all of rests keeps them.
    let src = "\\layout { \\context { \\Staff \\RemoveAllEmptyStaves } } << \\new Staff { R1 } \\new Staff { R1 } >>";
    assert_eq!(shown(&eng(src)), vec![vec![true, true]]);
    // Chord names are not a staff of notes: never left out.
    let src = "\\layout { \\context { \\Staff \\RemoveAllEmptyStaves } } << \\chords { c1 c } \\new Staff { R1 c'1 } \\new Staff { c'1 c' } >>";
    let e = eng(src);
    assert!(e.systems.iter().all(|s| !s.lines[0].hidden));
}

#[test]
fn a_group_shrinks_with_its_staves_and_loses_its_brace_with_one_left() {
    let piano = |extra: &str| {
        format!(
            "\\score {{ << \\new Staff {{ c''1 \\break c'' }} \\new PianoStaff << \\new Staff {{ c''1 \\break c'' }} \\new Staff {{ \\clef bass c1 \\break R1 }} >> >> \\layout {{ \\context {{ \\Staff \\RemoveEmptyStaves }} {extra} }} }}"
        )
    };
    let e = eng(&piano(""));
    assert_eq!(shown(&e), vec![vec![true, true, true], vec![true, true, false]]);
    let count = |e: &Engraved, page: usize| paths(e, 8.0, page).unwrap().0.len();
    let kept = eng(&piano("\\context { \\PianoStaff \\override SystemStartBrace.collapse-height = #4 }"));
    assert_eq!(count(&kept, 0), count(&e, 0) + 1, "the brace kept over one staff when its collapse-height lets it");
    // The bar lines of the second system join its two staves, the brace's down to no one.
    let sys = &e.systems[1];
    assert_eq!(sys.lines[1].connect_down, 0.0, "its group's other staff is gone");
    assert_eq!(sys.lines[2].connect_down, 0.0);
    // A bracket left with one staff: not drawn (LilyPond's collapse-height 5).
    let group = "\\score { \\new StaffGroup << \\new Staff { c''1 \\break c'' } \\new Staff { c'1 \\break R1 } >> \\layout { \\context { \\Staff \\RemoveEmptyStaves } } }";
    let e = eng(group);
    let full = eng("\\score { \\new StaffGroup << \\new Staff { c''1 \\break c'' } \\new Staff { c'1 \\break c'1 } >> }");
    let lines_and_ink = |e: &Engraved| e.systems[1].lines.iter().filter(|l| !l.hidden).count();
    assert_eq!(lines_and_ink(&e), 1);
    assert!(count(&e, 0) < count(&full, 0) - 8, "a staff, its bar lines and the bracket fewer");
    // Every band of a page is still cut, a hidden staff's empty.
    let b = bands(&e, 8.0, 0, 2000).unwrap();
    assert!(b.len() >= 3);
}

#[test]
fn midi_plays_every_staff_whatever_is_hidden() {
    let a = two("", "", "");
    let b = two("\\layout { \\context { \\Staff \\RemoveAllEmptyStaves } }", "", "");
    assert_eq!(midi(&a, &MidiOptions::default()).unwrap(), midi(&b, &MidiOptions::default()).unwrap());
}

#[test]
fn what_stands_over_the_score_goes_over_the_top_staff_drawn() {
    let src = "\\layout { \\context { \\Staff \\RemoveEmptyStaves } } << \\new Staff { c''1 c'' \\break \\mark \\default \\tempo 4 = 90 \\repeat volta 2 { R1 } \\alternative { { R1 } { R1 } } \\break c''1 c'' } \\new Staff { c'1 c' \\break c' c' c' \\break c' c' } >>";
    let e = eng(src);
    assert_eq!(shown(&e), vec![vec![true, true], vec![false, true], vec![true, true]]);
    let sys = &e.systems[1];
    assert_eq!(sys.lines[1].staff_index, Some(0), "the second staff is the system's top");
    assert_eq!(sys.lines[1].rehearsal.len(), 1, "the mark over it");
    assert_eq!(sys.lines[1].marks.len(), 1, "the tempo mark over it");
    assert_eq!(sys.lines[1].voltas.len(), 2, "the two endings over it");
    // Its bar number, 3, at the start of the line, in the drawn paths.
    let (drawn, _) = paths(&e, 8.0, 0).unwrap();
    let plain = eng("\\layout { \\context { \\Staff \\RemoveEmptyStaves } } << \\new Staff { c''1 c'' \\break R1 R1 \\break c''1 c'' } \\new Staff { c'1 c' \\break c' c' \\break c' c' } >>");
    let (with_hidden, _) = paths(&plain, 8.0, 0).unwrap();
    let unnumbered = eng("\\layout { \\context { \\Staff \\RemoveEmptyStaves } \\context { \\Score \\omit BarNumber } } << \\new Staff { c''1 c'' \\break R1 R1 \\break c''1 c'' } \\new Staff { c'1 c' \\break c' c' \\break c' c' } >>");
    let (without, _) = paths(&unnumbered, 8.0, 0).unwrap();
    assert_eq!(with_hidden.len(), without.len() + 2, "\"3\" over the second system, \"5\" over the third");
    assert!(sys.reach.unwrap().0 < -2.5, "room for the number and the mark over the drawn staff");
    assert!(drawn.len() > with_hidden.len());
}

#[test]
fn a_hidden_staff_leaves_the_beams_across_a_piano_staff_between_the_staves_drawn() {
    let src = "\\layout { \\context { \\Staff \\RemoveEmptyStaves } } << \\new Staff { c''1 c'' \\break R1 R1 } \\new PianoStaff << \\new Staff = \"up\" { c''8 g' e' c' \\change Staff = \"down\" g e \\change Staff = \"up\" c'' e'' | c''1 | c''8 g' e' c' \\change Staff = \"down\" g e \\change Staff = \"up\" c'' e'' | c''1 } \\new Staff = \"down\" { \\clef bass c1 c s1 c } >> >>";
    let e = eng(src);
    assert_eq!(shown(&e), vec![vec![true, true, true], vec![false, true, true]]);
    let (one, two) = (&e.systems[0], &e.systems[1]);
    assert!(!one.kneed.is_empty() && !two.kneed.is_empty(), "a kneed beam on each system");
    // The second system: the piano's staves at the top, the beam between them.
    assert_eq!(two.offsets[1], 0.0);
    assert_eq!(two.lines[1].staff_index, Some(0));
    let gap = two.offsets[2] - two.offsets[1];
    assert!((gap - (one.offsets[2] - one.offsets[1])).abs() < 3.0, "the piano's staves as far apart as on the first system");
    // Its height counts only the staves drawn.
    assert!(two.reach.unwrap().1 < one.reach.unwrap().1 - 8.0, "a staff's room fewer");
    assert!(two.reach.unwrap().0 <= -3.0, "and room for its bar number over the top staff drawn");
    paths(&e, 8.0, 0).unwrap();
    bands(&e, 8.0, 0, 3000).unwrap();
}
