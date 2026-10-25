//! Staves drawn smaller (score #407), as LilyPond draws them: `\magnifyStaff`,
//! `\override StaffSymbol.staff-space = #(magstep -3)` with `fontSize = #-3`,
//! the ossia and cue staves written that way — lines, notes, clef, key,
//! beams, rests and the room the staff takes all at the staff's scale, the
//! notes in the system's columns; and cue notes (`\new CueVoice`) at cue size.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::{parse, Kind};
use eui_score::{engrave, paths, warnings, Engraved, Layout, Path};

const SP: f64 = 8.0;

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / SP, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"))
}

/// The scale each staff of the first system is drawn at.
fn scales(e: &Engraved) -> Vec<f64> {
    e.systems[0].lines.iter().map(|l| l.scale.unwrap_or(1.0)).collect()
}

/// The staff lines drawn: rectangles as wide as the line, by their y in px.
fn staff_lines(e: &Engraved) -> Vec<f64> {
    let line = &e.systems[0].lines[0];
    let w = (line.right - line.left) * SP;
    let mut ys: Vec<f64> = paths(e, SP, 0).unwrap().0.iter().filter_map(|p| if let Path::Rect([_, y, rw, _]) = p { ((rw - w).abs() < 1.0).then_some(*y) } else { None }).collect();
    ys.sort_by(f64::total_cmp);
    ys
}

/// A path's box in px: x0, y0, x1, y1.
fn bbox(p: &Path) -> [f64; 4] {
    match p {
        Path::Rect([x, y, w, h]) => [*x, *y, x + w, y + h],
        Path::Fill(cmds) => {
            let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
            let mut i = 0;
            while i < cmds.len() {
                let n = match cmds[i] as i64 {
                    0 | 1 => 2,
                    2 => 4,
                    3 => 6,
                    _ => 0,
                };
                for k in (0..n).step_by(2) {
                    let (x, y) = (cmds[i + 1 + k], cmds[i + 2 + k]);
                    b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
                }
                i += 1 + n;
            }
            b
        }
    }
}

const OSSIA: &str = r#"\new Staff = "main" \relative c'' {
  c4 b d c |
  << { c4 b d c }
     \new Staff \with {
       alignAboveContext = #"main"
       \remove Time_signature_engraver
       fontSize = #-3
       \override StaffSymbol.staff-space = #(magstep -3)
       \override StaffSymbol.thickness = #(magstep -3)
       firstClef = ##f
     } { e4 d f e } >>
  c4 b c2 |
}"#;

#[test]
fn a_staff_magnified_is_drawn_at_its_scale() {
    let src = r#"<< \new Staff \with { \magnifyStaff #2/3 } { \key d \major c''8 d'' e'' f'' g''4 r } \new Staff { \clef bass c4 d e f } >>"#;
    assert!(warnings(src).unwrap().is_empty(), "{:?}", warnings(src));
    let e = eng(src);
    let s = scales(&e)[0];
    assert!((s - 2.0 / 3.0).abs() < 1e-9 && scales(&e)[1] == 1.0);
    // Its notes are of its own size: neither small on it, nor small twice.
    assert!(e.staves[0][0].items.iter().all(|ev| ev.size == 0), "{:?}", e.staves[0][0].items.iter().map(|ev| ev.size).collect::<Vec<_>>());
    // Five lines two thirds of a space apart, the other staff's a space.
    let ys = staff_lines(&e);
    assert_eq!(ys.len(), 10);
    let gaps: Vec<f64> = ys.windows(2).map(|w| ((w[1] - w[0]) / SP * 100.0).round() / 100.0).collect();
    assert_eq!(&gaps[..4], &[0.67, 0.67, 0.67, 0.67], "{gaps:?}");
    assert_eq!(&gaps[5..], &[1.0, 1.0, 1.0, 1.0], "{gaps:?}");
    // A note keeps its column and is drawn two thirds as large: its head
    // starts where the other staff's note does, and is as much smaller.
    let (paths, ranges) = eui_score::paths(&e, SP, 0).unwrap();
    let head_box = |staff: usize, j: usize| {
        let ev = &e.staves[staff][0].items[j];
        let range = ranges.iter().find(|r| r.0 == ev.i).unwrap().1.clone();
        // The head: the path whose box starts nearest the note's place.
        paths[range].iter().map(bbox).min_by(|a, b| (a[0] - ev.x * SP).abs().total_cmp(&(b[0] - ev.x * SP).abs())).unwrap()
    };
    let (small, big) = (head_box(0, 0), head_box(1, 0));
    assert!((small[0] - big[0]).abs() < 0.6, "the same column: {small:?} {big:?}");
    let ratio = (small[3] - small[1]) / (big[3] - big[1]);
    assert!((ratio - 2.0 / 3.0).abs() < 0.05, "{ratio}");
    // The staff under it comes up: it takes less room.
    let plain = eng(r#"<< \new Staff { \key d \major c''8 d'' e'' f'' g''4 r } \new Staff { \clef bass c4 d e f } >>"#);
    assert!(e.systems[0].offsets[1] < plain.systems[0].offsets[1], "{} {}", e.systems[0].offsets[1], plain.systems[0].offsets[1]);
    // Written in the music, the same.
    let e = eng(r"<< \new Staff { \magnifyStaff #2/3 c''4 } \new Staff { c'4 } >>");
    assert!((scales(&e)[0] - 2.0 / 3.0).abs() < 1e-9);
    // Alone on its page too.
    let e = eng(r"\new Staff \with { \magnifyStaff #1/2 } { c''4 d'' e'' f'' }");
    assert_eq!(scales(&e), vec![0.5]);
    let ys = staff_lines(&e);
    assert!(((ys[4] - ys[0]) / SP - 2.0).abs() < 0.05, "two spaces from top to bottom line: {ys:?}");
}

#[test]
fn the_cue_staff_idiom_staff_space_with_font_size() {
    let src = r#"<< \new Staff \with { fontSize = #-3 \override StaffSymbol.staff-space = #(magstep -3) \override StaffSymbol.thickness = #(magstep -3) \remove "Time_signature_engraver" } { r4 g'8 a' b'4 c'' }
                   \new StaffGroup << \new Staff { c'1 } \new Staff { c'1 } >> >>"#;
    assert!(warnings(src).unwrap().is_empty(), "{:?}", warnings(src));
    let e = eng(src);
    let s = scales(&e)[0];
    assert!((s - 2f64.powf(-0.5)).abs() < 1e-9, "magstep -3: {s}");
    assert!(e.staves[0][0].items.iter().all(|ev| ev.size == 0));
    // `\remove "Time_signature_engraver"`: no metre on it; the others keep theirs.
    assert_eq!(e.staves[0][0].time_parts[3], 253);
    assert_eq!(e.staves[1][0].time_parts[3], 0);
    // Staff-space alone, no fontSize: the lines close up, the notes keep
    // their size, as LilyPond leaves them.
    let e = eng(r"\new Staff \with { \override StaffSymbol.staff-space = #(magstep -3) } { c''4 }");
    assert!(e.staves[0][0].items[0].size > 0);
    // Out of range, or not a staff's: passed over with a warning.
    assert!(!warnings(r"\new Staff \with { \override StaffSymbol.staff-space = #9 } { c''4 }").unwrap().is_empty());
    assert!(eng(r"\new Staff \with { \override StaffSymbol.staff-space = #9 } { c''4 }").systems[0].lines[0].scale.is_none());
}

#[test]
fn an_ossia_written_small_is_drawn_small() {
    assert!(warnings(OSSIA).unwrap().is_empty(), "{:?}", warnings(OSSIA));
    let p = parse(OSSIA).unwrap();
    assert_eq!(p.staves[0].kind, eui_score::parse::StaffKind::Ossia);
    let e = eng(OSSIA);
    let s = scales(&e);
    assert!((s[0] - 2f64.powf(-0.5)).abs() < 1e-9 && s[1] == 1.0, "{s:?}");
    // Its notes stand over the main staff's, in the same columns.
    let ossia = e.staves[0][1].items.iter().filter(|ev| ev.k == Kind::Note).map(|ev| ev.x).collect::<Vec<_>>();
    let main = e.staves[1][1].items.iter().filter(|ev| ev.k == Kind::Note).map(|ev| ev.x).collect::<Vec<_>>();
    assert_eq!(ossia, main);
    // Closer over the staff than a full-size ossia.
    let plain = eng(&OSSIA.replace("fontSize = #-3", "").replace("\\override StaffSymbol.staff-space = #(magstep -3)", ""));
    assert!(plain.systems[0].lines[0].scale.is_none());
    assert!(e.systems[0].offsets[1] < plain.systems[0].offsets[1]);
}

#[test]
fn cue_notes_are_cue_sized_and_a_group_s_bar_lines_reach_across_a_small_staff() {
    let src = r"\new Staff { c'4 << { \new CueVoice { \voiceOne g''8 a'' b''4 } } \\ { \voiceTwo e'2 } >> c'4 }";
    let e = eng(src);
    let cue: Vec<(i8, bool)> = e.staves[0][0].items.iter().filter(|ev| ev.cue).map(|ev| (ev.size, ev.cue)).collect();
    assert_eq!(cue, vec![(-3, true); 3], "LilyPond's cue size, -3");
    // On a staff drawn at cue size, cue notes are its own size.
    let e = eng(r"\new Staff \with { \magnifyStaff #(magstep -3) } { \new CueVoice { c''4 d'' } }");
    assert!(e.staves[0][0].items.iter().all(|ev| ev.size == 0));
    // A bracket's bar lines run from the small staff's top to the next one's.
    let e = eng(r"\new StaffGroup << \new Staff \with { \magnifyStaff #2/3 } { c''1 } \new Staff { c'1 } >>");
    let sys = &e.systems[0];
    let line = &sys.lines[0];
    let s = line.scale.unwrap();
    assert!(((line.connect_down + 4.0) * s - (sys.offsets[1] - sys.offsets[0])).abs() < 1e-9, "{} {}", line.connect_down, sys.offsets[1]);
    // A staff of the usual size draws as it did.
    let plain = eng(r"\new StaffGroup << \new Staff { c''1 } \new Staff { c'1 } >>");
    assert_eq!(plain.systems[0].lines[0].connect_down + 4.0, plain.systems[0].offsets[1]);
}
