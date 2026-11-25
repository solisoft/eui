//! A tempo mark stays on its line (score #1060). A mark takes no room in its
//! bar, as LilyPond's takes none, so one over the last bar of a line ran
//! past the staff's end — into the margin on paper, off the picture on a
//! screen. The line breaks leave it its room; where none can, it starts
//! farther left and ends at the line's end.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::marks::{mark_width, TempoMark};
use eui_score::parse::Ev;
use eui_score::{engrave, engrave_paper, Engraved, Layout};

fn set(src: &str, width: f64) -> Engraved {
    engrave(src, width, &Layout::default()).unwrap_or_else(|e| panic!("{e}"))
}

/// Where the lines end: the right of the music.
fn edge(eng: &Engraved) -> f64 {
    eng.left + eng.width
}

/// The tempo mark of bar `bar`, the note it is written on, and its system.
fn marked(eng: &Engraved, bar: usize) -> (&TempoMark, &Ev, usize) {
    let s = eng.systems.iter().position(|sys| sys.lines[0].bars.contains(&bar)).expect("the bar is on a system");
    let ev = eng.staves[0][bar].items.iter().find(|ev| ev.tempo.is_some()).expect("the bar has a tempo mark");
    let mark = eng.systems[s].lines[0].marks.iter().find(|m| Some(&m.mark) == ev.tempo.as_ref()).expect("the mark is drawn on its system");
    (mark, ev, s)
}

const BAR: &str = "c'4 d' e' f'";
/// Some 44 spaces wide: more than two of these bars.
const LONG: &str = "\"Poco più mosso, ma non troppo\" 4 = 66";

/// `before` bars, a bar under the long mark, `after` bars more.
fn piece(before: usize, after: usize) -> String {
    format!("{{ {} | \\tempo {LONG} {} }}", vec![BAR; before].join(" | "), vec![BAR; after + 1].join(" | "))
}

/// The same for a piano: the mark in the right hand.
fn piano(before: usize, after: usize) -> String {
    let left = vec!["c8 g e g c g e g"; before + after + 1].join(" | ");
    format!("\\new PianoStaff << \\new Staff {} \\new Staff {{ \\clef bass {left} }} >>", piece(before, after))
}

/// The mark ends on its line, and stands where it is written: over its
/// note, or after the clef when its note starts the line — it was not moved
/// back to fit.
fn in_its_place(eng: &Engraved, bar: usize, what: &str) {
    let (mark, ev, s) = marked(eng, bar);
    let end = mark.x + mark_width(&mark.mark);
    assert!(end <= edge(eng) + 1e-6, "{what}: the mark ends at {end}, the lines at {}", edge(eng));
    let opens = eng.systems[s].lines[0].bars.start == bar;
    assert!(mark.x >= ev.x - if opens { 1.5 } else { 1e-6 }, "{what}: the mark starts at {}, its note stands at {}", mark.x, ev.x);
    assert!(mark.x <= ev.x + 1e-6, "{what}: the mark starts at {}, after its note at {}", mark.x, ev.x);
}

/// At every width the lines are cut so the mark has its room: it is never
/// the last thing of a line it would run out of. Before, at one width in
/// three its bar closed a line and the words ran on past the staff.
#[test]
fn the_line_breaks_leave_a_tempo_mark_its_room() {
    let src = piece(9, 9);
    assert!(mark_width(&marked(&set(&src, 100.0), 9).0.mark) > 40.0, "a long mark");
    for w in (60..=160).step_by(2) {
        let eng = set(&src, f64::from(w));
        in_its_place(&eng, 9, &format!("{w} spaces wide"));
    }
}

/// … for a piano's two staves as for one, the lines cut for the system.
#[test]
fn a_piano_s_lines_leave_the_mark_its_room_too() {
    for w in (70..=160).step_by(3) {
        let eng = set(&piano(9, 9), f64::from(w));
        assert_eq!(eng.systems[0].lines.len(), 2);
        in_its_place(&eng, 9, &format!("a piano, {w} spaces wide"));
    }
}

/// … and on paper, wherever in the piece the mark is written.
#[test]
fn a_page_s_lines_leave_the_mark_its_room() {
    for before in 1..=14 {
        let (eng, _) = engrave_paper(&piece(before, 14)).unwrap_or_else(|e| panic!("{e}"));
        assert!(eng.systems.len() > 1, "more than one line");
        in_its_place(&eng, before, &format!("on paper, after {before} bars"));
    }
}

/// The cuts are the same as ever where no mark is in the way: a short mark
/// that fits where it stands moves no line.
#[test]
fn a_mark_that_fits_moves_no_line() {
    let plain = format!("{{ {} }}", vec![BAR; 19].join(" | "));
    let short = plain.replacen(BAR, &format!("{BAR} | \\tempo \"Vivo\" {BAR}"), 1).replacen(&format!(" | {BAR} }}"), " }", 1);
    for w in (60..=160).step_by(10) {
        let lines = |eng: &Engraved| -> Vec<usize> { eng.systems.iter().map(|s| s.lines[0].bars.len()).collect() };
        let (a, b) = (set(&plain, f64::from(w)), set(&short, f64::from(w)));
        assert_eq!(a.staves[0].len(), b.staves[0].len());
        assert_eq!(lines(&a), lines(&b), "{w} spaces wide");
    }
}

/// A `\break` puts the mark's bar at the end of its line all the same: no
/// cut can give it room, so it starts farther left and ends where the line
/// ends, rather than in the margin.
#[test]
fn a_mark_no_cut_can_fit_ends_at_the_line_s_end() {
    let src = format!("{{ {BAR} | {BAR} | \\tempo {LONG} {BAR} \\break {BAR} | {BAR} | {BAR} }}");
    let eng = set(&src, 70.0);
    assert_eq!(eng.systems[0].lines[0].bars, 0..3, "the break is kept");
    let (mark, ev, s) = marked(&eng, 2);
    assert_eq!(s, 0);
    let end = mark.x + mark_width(&mark.mark);
    assert!((end - edge(&eng)).abs() < 1e-6, "the mark ends at {end}, the line at {}", edge(&eng));
    assert!(mark.x < ev.x - 5.0, "it starts before its note: {} against {}", mark.x, ev.x);
    assert!(mark.x >= eng.systems[0].lines[0].left, "and not before its line");
}

/// A score of one short line is not filled to the margin: its mark may run
/// past the staff's end, as it always has, while it ends on the page.
#[test]
fn a_mark_longer_than_a_short_line_keeps_its_place() {
    let eng = set(&format!("{{ \\tempo {LONG} c'4 }}"), 120.0);
    let line = &eng.systems[0].lines[0];
    let (mark, ev, _) = marked(&eng, 0);
    let end = mark.x + mark_width(&mark.mark);
    assert!(line.right < 30.0, "a short line: {}", line.right);
    assert!(end > line.right && end <= edge(&eng), "the mark ends at {end}");
    assert!(mark.x > line.left + 3.0 && mark.x <= ev.x, "after the clef: {}", mark.x);
}
