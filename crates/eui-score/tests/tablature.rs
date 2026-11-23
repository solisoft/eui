//! A tablature's details, as LilyPond draws them (score #1026): grace notes'
//! frets small before the note's, a dead note's cross on its string, a tied
//! note's fret hidden (in parentheses where a line starts with it), a
//! harmonic's fret between angle brackets, and string numbers past the
//! sixth on an instrument that has them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::engrave::tab_label;
use eui_score::parse::{parse, Ev, Kind, TabMark};
use eui_score::{engrave, engrave_paper, paths, Engraved, Layout};

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn notes(eng: &Engraved, staff: usize) -> Vec<Ev> {
    eng.staves[staff].iter().flat_map(|b| b.items.iter()).filter(|e| e.k == Kind::Note).cloned().collect()
}

/// How many paths each note of the first staff is drawn with: one a
/// character of its frets, its graces' included.
fn drawn(eng: &Engraved) -> Vec<usize> {
    let (_, ranges) = paths(eng, 8.0, 0).unwrap();
    notes(eng, 0).iter().map(|ev| ranges.iter().filter(|(i, _)| *i == ev.i).map(|(_, r)| r.len()).sum()).collect()
}

#[test]
fn grace_notes_have_their_small_frets_before_the_note() {
    let e = eng("\\new TabStaff { \\grace d8 e4 \\acciaccatura { f16 g } a4 }");
    let n = notes(&e, 0);
    // D3 open on the fourth string, before E3 at its second fret.
    assert_eq!(n[0].graces[0].tab, vec![(4, 0)]);
    assert_eq!(n[0].tab, vec![(4, 2)]);
    assert_eq!(n[1].graces.iter().map(|g| g.tab.clone()).collect::<Vec<_>>(), vec![vec![(4, 3)], vec![(3, 0)]]);
    // Left of the note, in order, and the note's room grows by them.
    assert!(n[0].graces[0].x < 0.0);
    assert!(n[1].graces[0].x < n[1].graces[1].x && n[1].graces[1].x < 0.0);
    let plain = notes(&eng("\\new TabStaff { e4 a4 }"), 0);
    assert!(n[0].left > plain[0].left + 0.5, "{} against {}", n[0].left, plain[0].left);
    // Drawn: one more fret for the first note, two for the second.
    let with = drawn(&e);
    let without = drawn(&eng("\\new TabStaff { e4 a4 }"));
    assert_eq!(with, vec![without[0] + 1, without[1] + 2]);
}

#[test]
fn a_dead_note_is_a_cross_on_its_string() {
    for src in ["\\new TabStaff { \\deadNote e4 e4 }", "\\new TabStaff { \\xNote e4 e4 }", "\\new TabStaff { \\deadNotesOn e4 \\deadNotesOff e4 }"] {
        let e = eng(src);
        let n = notes(&e, 0);
        assert_eq!((n[0].tab.clone(), n[0].tab_marks.clone()), (vec![(4, 2)], vec![TabMark::Dead]), "{src}");
        assert_eq!(n[1].tab_marks, vec![TabMark::Fret], "{src}");
        assert_eq!(tab_label(&n[0], 0, false), Some("X".to_string()));
        assert_eq!(tab_label(&n[1], 0, false), Some("2".to_string()));
        // A cross is one shape, a 2 another.
        assert_eq!(drawn(&e), vec![1, 1], "{src}");
    }
}

#[test]
fn a_tied_note_s_fret_is_hidden_unless_a_line_starts_with_it() {
    let e = eng("\\new TabStaff { e4~ e4 <e g>2~ | <e g>2 e2 }");
    let n = notes(&e, 0);
    assert_eq!(n[0].tab_marks, vec![TabMark::Fret]);
    assert_eq!(n[1].tab_marks, vec![TabMark::Tied]);
    assert_eq!(n[3].tab_marks, vec![TabMark::Tied, TabMark::Tied]);
    assert_eq!(n[4].tab_marks, vec![TabMark::Fret], "not tied");
    assert_eq!(tab_label(&n[1], 0, false), None);
    assert_eq!(tab_label(&n[1], 0, true), Some("(2)".to_string()));
    assert_eq!(drawn(&e), vec![1, 0, 2, 0, 1]);
    // A line that starts with a tied note shows it, in parentheses.
    let broken = eng("\\new TabStaff { e1~ \\break e1 }");
    assert_eq!(broken.systems.len(), 2);
    let (_, ranges) = paths(&broken, 8.0, 0).unwrap();
    let second = notes(&broken, 0)[1].i;
    let ink: usize = ranges.iter().filter(|(i, _)| *i == second).map(|(_, r)| r.len()).sum();
    assert_eq!(ink, 3, "(2), on the second line: three characters");
    // Only the head the tie reaches: a chord tied from one of its notes.
    let part = notes(&eng("\\new TabStaff { e4~ <e g>4 }"), 0);
    assert_eq!(part[1].tab.len(), 2);
    let tied: Vec<bool> = part[1].tab_marks.iter().map(|m| *m == TabMark::Tied).collect();
    assert_eq!(tied.iter().filter(|t| **t).count(), 1);
}

#[test]
fn a_harmonic_s_fret_stands_between_angle_brackets() {
    for src in ["\\new TabStaff { <e'\\harmonic>4 }", "\\new TabStaff { e'4\\harmonic }", "\\new TabStaff { \\harmonicNote e'4 }", "\\new TabStaff { \\harmonicsOn e'4 }"] {
        let n = notes(&eng(src), 0);
        assert_eq!(n[0].tab_marks, vec![TabMark::Harmonic], "{src}");
        assert_eq!(tab_label(&n[0], 0, false), Some("<0>".to_string()), "{src}");
    }
    // In a chord, the head written so.
    let n = notes(&eng("\\new TabStaff { <g b\\harmonic>4 }"), 0);
    assert_eq!(n[0].tab, vec![(2, 0), (3, 0)]);
    assert_eq!(n[0].tab_marks, vec![TabMark::Harmonic, TabMark::Fret]);
}

#[test]
fn string_numbers_past_six_name_the_strings_a_tuning_has() {
    // A seven-string guitar's low B, written on its seventh string.
    let seven = "\\new TabStaff \\with { stringTunings = #guitar-seven-string-tuning } { e,4\\7 b,,4 e,4 }";
    let n = notes(&eng(seven), 0);
    assert_eq!(n[0].tab, vec![(7, 5)]);
    assert_eq!(n[1].tab, vec![(7, 0)]);
    assert_eq!(n[2].tab, vec![(6, 0)]);
    // A guitar has no seventh: the note goes where it is played.
    assert_eq!(notes(&eng("\\new TabStaff { e,4\\7 }"), 0)[0].tab, vec![(6, 0)]);
    // On a staff, a circled 7.
    let staff = eng("{ c4\\7 }");
    assert_eq!(staff.systems[0].lines[0].scripts.iter().filter(|s| s.circled).count(), 1);
    assert!(parse("{ c4\\0 }").is_err());
    engrave_paper(seven).unwrap();
}

#[test]
fn fret_boards_and_diagrams_have_the_strings_their_tuning_has() {
    use eui_score::frets::{from_spec, terse, verbose, FRET};
    let shapes =
        |src: &str| -> Vec<String> { parse(src).unwrap().staves[0].events.iter().filter_map(|e| e.chord_name.first()).filter_map(|r| r.text.strip_prefix(FRET).map(str::to_string)).collect() };
    // A ukulele's C, G7, F and Am, as LilyPond's ukulele table has them:
    // the C string under the high G is the bass.
    let uke = "\\new FretBoards \\with { stringTunings = #ukulele-tuning } \\chordmode { c1 g:7 f a:m }";
    assert_eq!(shapes(uke), vec!["o;o;o;3;", "o;2;1;2;", "2;o;1;o;", "2;o;o;o;"]);
    // A guitar's, unless it says.
    assert_eq!(shapes("\\new FretBoards \\chordmode { c1 }"), vec!["x;3;2;o;1;o;"]);
    // Four strings drawn: narrower than six.
    let four = from_spec(&format!("{FRET}o;o;o;3;")).unwrap();
    assert_eq!(four.strings(), 4);
    let six = from_spec(&format!("{FRET}x;3;2;o;1;o;")).unwrap();
    let e = eui_score::text::em(0.0);
    assert!(eui_score::frets::size(&four, e).0 < eui_score::frets::size(&six, e).0 - 0.5);
    let ink = |d: &eui_score::frets::Diagram| eui_score::frets::shapes(d, 0.0, 0.0, e).len();
    assert_eq!(ink(&four), 4 + 5 + 4, "4 strings, 5 fret lines, 3 rings and a dot");
    // In markup: `w:4;` says how many, a terse spec by how many it names.
    assert_eq!(verbose("w:4;4-o;3-o;2-o;1-3;").unwrap(), four);
    assert_eq!(terse("o;o;o;3;").unwrap(), four);
    assert!(verbose("w:4;5-1;").unwrap_err().contains("names no string 1 to 4"));
    assert!(verbose("w:12;").is_err());
    // Engraved, on paper too.
    let src = "<< \\new FretBoards \\with { stringTunings = #ukulele-tuning } \\chordmode { c1 g:7 } \\new Staff { c'1 b } >>";
    eng(src);
    engrave_paper(src).unwrap();
    assert!(eng("{ c''1^\\markup \\fret-diagram \"w:4;4-o;3-o;2-o;1-3;\" }").systems.len() == 1);
}
