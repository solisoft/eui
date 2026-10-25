//! Layout settings honoured (score #403): `\override`, `\set`, `\tweak`,
//! `\omit` and `\hide` with `\once`, `\revert` and `\undo`, in the music, in
//! a `\with` and in `\layout { \context { … } }` — each against the same
//! music without it, and what is still passed over still warned about.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects, clippy::float_cmp)]

use eui_score::engrave::{Engraved, Line};
use eui_score::look::Val;
use eui_score::parse::{parse, Kind, Parsed};
use eui_score::{engrave, paths, Layout};

fn read(src: &str) -> Parsed {
    parse(src).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn eng(src: &str) -> Engraved {
    engrave(src, 900.0 / 8.0, &Layout::default()).unwrap_or_else(|e| panic!("{src}: {e}"))
}

fn line(e: &Engraved) -> &Line {
    &e.systems[0].lines[0]
}

/// The source's warnings, as words.
fn warned(src: &str) -> Vec<String> {
    read(src).warnings.into_iter().map(|w| w.message).collect()
}

/// The notes and rests of staff `s`, in order.
fn notes(p: &Parsed, s: usize) -> Vec<&eui_score::parse::Ev> {
    p.staves[s].events.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest)).collect()
}

/// How many paths the first page draws.
fn drawn(src: &str) -> usize {
    paths(&eng(src), 8.0, 0).unwrap().0.len()
}

#[test]
fn honoured_settings_say_nothing_and_the_others_still_warn() {
    let honoured = [
        "{ \\override Slur.direction = #UP c'4( d') }",
        "{ \\once \\override TextScript.extra-offset = #'(0 . 2) c'4^\"a\" }",
        "{ \\set Score.currentBarNumber = #5 c'1 }",
        "{ \\omit Staff.TimeSignature \\hide Stem c'4 \\undo \\omit Staff.TimeSignature }",
        "{ \\override NoteHead.style = #'diamond c'4 \\revert NoteHead.style }",
        "\\layout { \\context { \\Score \\override BarNumber.break-visibility = ##(#f #t #t) } } { c'4 }",
        "\\new Staff \\with { \\RemoveEmptyStaves fontSize = #-2 \\override StaffSymbol.color = #red } { c'4 }",
        "{ \\slurUp \\tieDown \\dynamicUp \\tupletDown \\phrasingSlurNeutral \\numericTimeSignature \\xNotesOn c'4 }",
        "{ \\set Timing.beatStructure = 2,2,3 \\set Timing.beamExceptions = #'() \\time 7/8 c'8 d' e' f' g' a' b' }",
        "{ c'4-\\tweak extra-offset #'(1 . 0) -\"x\" \\tweak NoteHead.style #'cross d' }",
    ];
    for src in honoured {
        let w = warned(src);
        let rest: Vec<&String> = w.iter().filter(|m| !m.contains("StaffSymbol.color")).collect();
        assert!(rest.is_empty(), "{src}: {w:?}");
        eng(src);
    }
    // What the engraver has no hook for is read and passed over, as before.
    let passed = [
        ("{ \\override NoteHead.color = #red c'4 }", "\\override NoteHead.color is not drawn"),
        ("{ \\once \\override Stem.length = #9 c'4 }", "\\override Stem.length is not drawn"),
        ("{ \\override Slur.positions = #'(1 . 2) c'4( d') }", "\\override Slur.positions is not drawn"),
        ("{ \\override TextScript.direction = #'up c'4-\"x\" }", "\\override TextScript.direction is not drawn"),
        ("{ \\override Staff.StaffSymbol.staff-space = #(magstep -3) c'4 }", "\\override Staff.StaffSymbol.staff-space is not drawn"),
        ("{ \\tweak color #red c'4 }", "\\tweak color is not drawn"),
        ("{ \\omit Accidental cis'4 }", "\\omit Accidental is not drawn"),
        ("{ \\override Score.SpacingSpanner.common-shortest-duration = #(ly:make-moment 1/16) c'4 }", "\\override Score.SpacingSpanner.common-shortest-duration is not drawn"),
    ];
    for (src, said) in passed {
        assert_eq!(warned(src), vec![said.to_owned()], "{src}");
    }
    let w = warned("\\new Staff \\with { \\override StaffSymbol.color = #red } { c'4 }");
    assert_eq!(w, vec!["\\override StaffSymbol.color in \\with is not drawn".to_owned()]);
    // A setting with no value is still refused.
    assert!(parse("{ \\override TextScript.extra-offset c'4 }").is_err());
}

#[test]
fn directions_of_slurs_ties_dynamics_words_scripts_fingers_and_tuplets() {
    // A slur over notes with their stems up goes under them; \slurUp puts it over.
    let curve_mid = |src: &str| {
        let e = eng(src);
        let c = &line(&e).curves[0];
        let mut ys = Vec::new();
        let mut k = 0;
        while k < c.len() {
            let args = match c[k] as i64 {
                0 | 1 => 2,
                2 => 4,
                3 => 6,
                _ => 0,
            };
            for j in 0..args / 2 {
                ys.push(c[k + 2 + 2 * j]);
            }
            k += 1 + args;
        }
        ys.iter().fold(f64::INFINITY, |a, b| a.min(*b))
    };
    let down = curve_mid("{ c'4( d' e' f') }");
    let up = curve_mid("{ \\slurUp c'4( d' e' f') }");
    assert!(up < down - 1.0, "over the notes: {up} against {down}");
    assert!(curve_mid("{ \\override Slur.direction = #UP c'4( d' e' f') }") < down - 1.0);
    assert_eq!(curve_mid("{ \\slurUp \\slurNeutral c'4( d' e' f') }"), down, "\\slurNeutral gives it back");
    assert!(curve_mid("{ \\phrasingSlurUp c'4\\( d' e' f'\\) }") < down - 1.0);
    // A tie on a note whose stem is down goes over it by default; \tieDown under.
    let tie_top = |src: &str| curve_mid(src);
    assert!(tie_top("{ \\tieDown c''2~ c'' }") > tie_top("{ c''2~ c'' }"));
    // \dynamicUp: the dynamic over the staff.
    let dyn_y = |src: &str| line(&eng(src)).dynamics[0].y;
    assert!(dyn_y("{ c'4\\f }") > 4.0);
    assert!(dyn_y("{ \\dynamicUp c'4\\f }") < 0.0);
    assert!(dyn_y("{ \\override DynamicLineSpanner.direction = #UP c'4\\< d'\\! }") < 0.0);
    // Words with `-` under the note; TextScript.direction over it.
    let text_dir = |src: &str| line(&eng(src)).texts[0].dir;
    assert_eq!(text_dir("{ c'4-\"x\" }"), 1);
    assert_eq!(text_dir("{ \\override TextScript.direction = #UP c'4-\"x\" }"), -1);
    assert_eq!(text_dir("{ \\override TextScript.direction = #UP c'4_\"x\" }"), 1, "written under stays under");
    // An accent written with `-` goes opposite the stem; Script.direction says where.
    let script_y = |src: &str| line(&eng(src)).scripts[0].y;
    assert!(script_y("{ c'4-> }") > 4.0, "stem up: the accent under the head");
    assert!(script_y("{ \\override Script.direction = #UP c'4-> }") < 0.0);
    assert!(script_y("{ \\override Script.direction = #DOWN c''4-> }") > 4.0);
    // A fingering above unless said.
    let finger_y = |src: &str| line(&eng(src)).scripts[0].y;
    assert!(finger_y("{ c''4-3 }") < 2.0);
    assert!(finger_y("{ \\override Fingering.direction = #DOWN c''4-3 }") > 4.0);
    // A tuplet's bracket on the side its notes give, or the one set.
    let tup_up = |src: &str| line(&eng(src)).tuplets[0].up;
    assert!(!tup_up("{ \\tuplet 3/2 { c''4 d'' e'' } }"));
    assert!(tup_up("{ \\tupletUp \\tuplet 3/2 { c''4 d'' e'' } }"));
    assert!(!tup_up("{ \\override TupletBracket.direction = #DOWN \\tuplet 3/2 { c'4 d' e' } }"));
    // \stemUp's kin as an override, and reverted.
    let ups = |src: &str| eng(src).staves[0][0].items.iter().map(|e| e.up).collect::<Vec<_>>();
    assert_eq!(ups("{ \\override Stem.direction = #DOWN c'4 \\revert Stem.direction c' c'' c'' }"), vec![false, true, false, false]);
    assert_eq!(ups("{ \\once \\override Stem.direction = #DOWN c'4 c' c' c' }"), vec![false, true, true, true], "\\once: the next note only");
}

#[test]
fn offsets_padding_alignment_and_sizes_of_words() {
    let text = |src: &str| line(&eng(src)).texts[0].clone();
    let plain = text("{ c'4^\"dolce\" }");
    let moved = text("{ \\override TextScript.extra-offset = #'(1 . 2) c'4^\"dolce\" }");
    assert!((moved.x0 - plain.x0 - 1.0).abs() < 1e-9);
    assert!((moved.y - plain.y + 2.0).abs() < 1e-9, "two spaces up");
    let tweaked = text("{ c'4-\\tweak extra-offset #'(1 . 2) ^\"dolce\" }");
    assert_eq!((tweaked.x0, tweaked.y), (moved.x0, moved.y), "\\tweak, on what follows it");
    let x_off = text("{ \\override TextScript.X-offset = #-2 c'4^\"dolce\" }");
    assert!((x_off.x0 - plain.x0 + 2.0).abs() < 1e-9);
    let y_off = text("{ \\override TextScript.Y-offset = #6 c'4^\"dolce\" }");
    assert!((y_off.y - (2.0 - 6.0)).abs() < 1e-9, "from the middle line");
    let padded = text("{ \\override TextScript.padding = #2 c'4^\"dolce\" }");
    assert!(padded.y < plain.y - 1.0);
    let staff_padded = text("{ \\override TextScript.staff-padding = #4 c'4^\"dolce\" }");
    assert!(staff_padded.y < plain.y - 2.0);
    // self-alignment-X: the words' left at the head's (LilyPond's), centred, right.
    let centred = text("{ \\override TextScript.self-alignment-X = #CENTER c'4^\"dolce\" }");
    let right = text("{ \\override TextScript.self-alignment-X = #RIGHT c'4^\"dolce\" }");
    let w = plain.x1 - plain.x0;
    assert!(centred.x0 < plain.x0 && right.x0 < centred.x0);
    assert!((plain.x0 - right.x0 - (w - 1.304)).abs() < 0.02, "its right at the head's right");
    // font-size: larger words.
    let big = text("{ \\override TextScript.font-size = #3 c'4^\"dolce\" }");
    assert!(big.x1 - big.x0 > w * 1.3 && big.em > plain.em);
    // Script and fingering moved; the metronome mark and the dynamic too.
    let script = |src: &str| line(&eng(src)).scripts[0].clone();
    let s0 = script("{ c''4-. }");
    let s1 = script("{ \\override Script.extra-offset = #'(0.5 . -1) c''4-. }");
    assert!((s1.x - s0.x - 0.5).abs() < 1e-9 && (s1.y - s0.y - 1.0).abs() < 1e-9);
    let f1 = script("{ \\override Fingering.X-offset = #2 c''4-1 }");
    assert!((f1.x - eng("{ c''4-1 }").staves[0][0].items[0].x - 2.0).abs() < 1e-9);
    let mm = |src: &str| line(&eng(src)).marks[0].y;
    assert!((mm("{ \\override Score.MetronomeMark.extra-offset = #'(0 . 2) \\tempo 4 = 90 c'4 }") - mm("{ \\tempo 4 = 90 c'4 }") + 2.0).abs() < 1e-9);
    let dy = |src: &str| line(&eng(src)).dynamics[0].clone();
    let d0 = dy("{ c'4\\f }");
    let d1 = dy("{ \\once \\override DynamicText.extra-offset = #'(1 . 1) c'4\\f }");
    assert!((d1.x0 - d0.x0 - 1.0).abs() < 1e-9 && (d1.y - d0.y + 1.0).abs() < 1e-9);
    let high = dy("{ c''4\\f }");
    assert!(dy("{ \\override DynamicLineSpanner.staff-padding = #4 c''4\\f }").y > high.y + 2.0);
    // The notes' size: font-size steps, \large and \huge, \magnifyMusic, a staff's fontSize.
    let sizes = |src: &str| notes(&read(src), 0).iter().map(|e| e.size).collect::<Vec<_>>();
    assert_eq!(sizes("{ \\override NoteHead.font-size = #-2 c'4 \\revert NoteHead.font-size c' \\large c' \\huge c' }"), vec![-2, 0, 1, 2]);
    assert_eq!(sizes("{ c'4 \\magnifyMusic 0.63 { c' c' } c' }"), vec![0, -4, -4, 0]);
    assert_eq!(sizes("\\new Staff \\with { fontSize = #-3 } { c'4 c' }"), vec![-3, -3]);
    assert_eq!(sizes("{ \\set Staff.fontSize = #-1 c'4 \\unset Staff.fontSize c' }"), vec![-1, 0]);
    assert!(parse("{ \\magnifyMusic c'4 }").is_err(), "a ratio, then music");
}

#[test]
fn beams_group_subdivide_and_sit_where_they_are_told() {
    let groups = |src: &str| eng(src).staves[0][0].beams.iter().map(|b| b.notes.len()).collect::<Vec<_>>();
    assert_eq!(groups("{ \\time 6/8 c'8 d' e' f' g' a' }"), vec![3, 3]);
    assert_eq!(groups("{ \\time 6/8 \\set Timing.beatStructure = 2,2,2 \\set Timing.beamExceptions = #'() c'8 d' e' f' g' a' }"), vec![2, 2, 2]);
    assert_eq!(groups("{ \\time 7/8 \\set Timing.beatStructure = #'(2 2 3) c'8 d' e' f' g' a' b' }"), vec![2, 2, 3]);
    assert_eq!(groups("{ \\time 4/4 \\set Timing.beamExceptions = #'((end . (((1 . 8) . (3 3 2))))) c'8 d' e' f' g' a' b' c'' }"), vec![3, 3, 2]);
    // In 4/4, a beat structure alone leaves LilyPond's eighths beamed by the half bar.
    assert_eq!(groups("{ \\set Timing.beatStructure = 1,1,1,1 c'8 d' e' f' g' a' b' c'' }"), vec![4, 4]);
    // Subdivided: the same beam, its inner beams broken at each eighth.
    let src = "{ c'32 d' e' f' g' a' b' c'' r4 r2 }";
    let sub = "{ \\set subdivideBeams = ##t \\set baseMoment = #(ly:make-moment 1/8) c'32 d' e' f' g' a' b' c'' r4 r2 }";
    assert_eq!(groups(src), groups(sub));
    assert_eq!(drawn(sub), drawn(src) + 2, "the second and third beams each in two");
    // Positions: its ends where they are set; damping: flat.
    let beam = |src: &str| eng(src).staves[0][0].beams[0].clone();
    let b = beam("{ \\override Beam.positions = #'(3 . 4) c'8 e' g' c'' }");
    assert!((b.y0 - (2.0 - 3.0 - 0.24)).abs() < 1e-9 && (b.y1 - (2.0 - 4.0 - 0.24)).abs() < 1e-9, "{b:?}");
    // Set over notes whose stems would go down: the stems go up to it.
    let over = beam("{ \\override Beam.positions = #'(6 . 6) c''8 e'' g'' c''' }");
    assert!(over.up && over.y0 < -3.0);
    let under = beam("{ c''8 e'' g'' c''' }");
    assert!(!under.up);
    let sloped = beam("{ c'8 e' g' c'' }");
    assert!(sloped.y0 != sloped.y1);
    let flat = beam("{ \\override Beam.damping = #+inf.0 c'8 e' g' c'' }");
    assert_eq!(flat.y0, flat.y1);
}

#[test]
fn marks_formatted_aligned_and_moved() {
    let mark = |src: &str| {
        let e = eng(src);
        let m = line(&e).rehearsal[0].clone();
        (m.runs.iter().map(|r| r.text.clone()).collect::<String>(), m.frame, m.x, m.y)
    };
    assert_eq!(mark("{ \\mark \\default c'1 }").0, "A");
    let boxed = mark("{ \\set Score.rehearsalMarkFormatter = #format-mark-box-letters \\mark \\default c'1 \\mark \\default c'1 }");
    assert_eq!((boxed.0.as_str(), boxed.1.as_str()), ("A", "box"));
    let e = eng("{ \\set Score.markFormatter = #format-mark-circle-numbers c'1 \\mark \\default c'1 \\mark \\default c'1 }");
    let rs: Vec<(String, String)> = line(&e).rehearsal.iter().map(|m| (m.runs[0].text.clone(), m.frame.clone())).collect();
    assert_eq!(rs, vec![("1".into(), "circle".into()), ("2".into(), "circle".into())]);
    // In a \layout, for the whole score.
    let n = mark("\\score { { c'1 \\mark \\default c'1 } \\layout { \\context { \\Score rehearsalMarkFormatter = #format-mark-numbers } } }");
    assert_eq!((n.0.as_str(), n.1.as_str()), ("1", ""));
    let bars = mark("{ \\set Score.rehearsalMarkFormatter = #format-mark-barnumbers c'1 c'1 \\mark \\default c'1 }");
    assert_eq!(bars.0, "3", "the bar it stands at");
    // A mark written as words is not counted, nor formatted.
    assert_eq!(mark("{ \\set Score.markFormatter = #format-mark-box-numbers c'1 \\mark \"Fine\" c'1 }").1, "");
    // Its alignment on the bar line: centred, or its left at it.
    let centred = mark("{ c'1 \\mark \\default c'1 }");
    let left = mark("{ c'1 \\once \\override Score.RehearsalMark.self-alignment-X = #LEFT \\mark \\default c'1 }");
    assert!(left.2 > centred.2);
    let moved = mark("{ c'1 \\override Score.RehearsalMark.extra-offset = #'(1 . 1) \\mark \\default c'1 }");
    assert!((moved.2 - centred.2 - 1.0).abs() < 1e-9 && (moved.3 - centred.3 + 1.0).abs() < 1e-9);
    let padded = mark("{ c'1 \\override Score.RehearsalMark.padding = #3 \\mark \\default c'1 }");
    assert!(padded.3 < centred.3 - 2.0);
}

#[test]
fn metres_heads_and_omitted_grobs() {
    let parts = |src: &str| read(src).staves[0].events.iter().find(|e| e.k == Kind::Time).map(|e| e.time_parts[3]);
    assert_eq!(parts("{ \\time 4/4 c'1 }"), Some(0));
    assert_eq!(parts("{ \\numericTimeSignature \\time 4/4 c'1 }"), Some(255));
    assert_eq!(parts("{ \\override Staff.TimeSignature.style = #'numbered c'1 }"), Some(255), "the 4/4 no \\time writes");
    assert_eq!(parts("{ \\override Staff.TimeSignature.style = #'single-digit \\time 3/4 c'2. }"), Some(254));
    assert_eq!(parts("{ \\omit Staff.TimeSignature \\time 3/4 c'2. }"), Some(253));
    assert_eq!(parts("\\new Staff \\with { \\omit TimeSignature } { c'1 }"), Some(253));
    assert_eq!(parts("{ \\numericTimeSignature \\defaultTimeSignature \\time 4/4 c'1 }"), Some(0));
    // C or 4 over 4; the numbered one is wider, the omitted one takes no room.
    let first_x = |src: &str| eng(src).staves[0][0].items[0].x;
    assert!(first_x("{ \\omit Staff.TimeSignature c'1 }") < first_x("{ c'1 }") - 1.0);
    assert!(drawn("{ \\numericTimeSignature c'1 }") == drawn("{ c'1 }") + 1, "two figures for one C");
    // Heads: their style, their width.
    let head_w = |src: &str| eng(src).staves[0][0].items[0].head_w.unwrap();
    assert!(head_w("{ \\override NoteHead.style = #'slash c'4 }") > head_w("{ c'4 }"));
    assert!(head_w("{ \\override NoteHead.style = #'diamond c'4 }") > head_w("{ c'4 }"));
    for style in ["cross", "xcircle", "diamond", "triangle", "slash", "harmonic", "harmonic-black", "harmonic-mixed"] {
        let src = format!("{{ \\override NoteHead.style = #'{style} c'1 c'2 c'4 }}");
        assert!(warned(&src).is_empty(), "{style}");
        let p = paths(&eng(&src), 8.0, 0).unwrap().0;
        assert_ne!(p, paths(&eng("{ c'1 c'2 c'4 }"), 8.0, 0).unwrap().0, "{style}");
    }
    assert!(warned("{ \\override NoteHead.style = #'baroque c'4 }")[0].contains("NoteHead.style"), "a style not drawn is passed over");
    // \omit and \hide: nothing drawn.
    let base = drawn("{ c'8 d' e'4 \\tuplet 3/2 { c'4 d' e' } }");
    assert_eq!(drawn("{ \\omit Stem c'8 d' e'4 \\tuplet 3/2 { c'4 d' e' } }"), base - 6, "six stems");
    assert_eq!(drawn("{ \\omit Beam c'8 d' e'4 \\tuplet 3/2 { c'4 d' e' } }"), base - 1);
    assert!(drawn("{ \\omit TupletNumber c'8 d' e'4 \\tuplet 3/2 { c'4 d' e' } }") < base);
    assert_eq!(drawn("{ \\omit Flag c'8 r d'4 }"), drawn("{ c'8 r d'4 }") - 1);
    assert_eq!(drawn("{ \\omit Staff.StaffSymbol c'1 }"), drawn("{ c'1 }") - 5);
    assert_eq!(drawn("{ \\omit Staff.Clef c'1 }"), drawn("{ c'1 }") - 1);
    assert_eq!(drawn("{ \\key d \\major \\omit Staff.KeySignature c'1 }"), drawn("{ \\key d \\major c'1 }") - 2);
    assert_eq!(drawn("{ \\hide Staff.BarLine c'1 c'1 }"), drawn("{ c'1 c'1 }") - 2);
    assert_eq!(drawn("{ c'4-. \\omit Script d'-. }"), drawn("{ c'4-. d' }"));
    assert_eq!(drawn("{ c'4^\"x\" \\omit TextScript d'^\"y\" }"), drawn("{ c'4^\"x\" d' }"));
    assert_eq!(drawn("{ c'4\\f \\omit DynamicText d'\\p }"), drawn("{ c'4\\f d' }"));
    assert_eq!(drawn("{ \\omit Stem c'4 \\undo \\omit Stem d' }"), drawn("{ c'4 d' }") - 1, "\\undo gives it back");
    assert_eq!(drawn("{ \\once \\omit Stem c'4 d' }"), drawn("{ c'4 d' }") - 1, "\\once: one note");
}

#[test]
fn bar_numbers_shown_where_they_are_said_and_counted_from_where_they_are_set() {
    let numbers = |src: &str| eng(src).staves[0].iter().map(|b| b.number).collect::<Vec<_>>();
    assert_eq!(numbers("{ \\set Score.currentBarNumber = #20 c'1 c' c' }"), vec![20, 21, 22]);
    assert_eq!(numbers("{ c'1 \\set Score.currentBarNumber = #10 c' c' }"), vec![1, 10, 11]);
    // Only at a line's start by default: none on one line.
    let one_line = "{ c'1 c' c' c' }";
    let all = "{ \\set Score.barNumberVisibility = #all-bar-numbers-visible \\override Score.BarNumber.break-visibility = #all-visible c'1 c' c' c' }";
    assert_eq!(drawn(all), drawn(one_line) + 3, "bars 2, 3 and 4");
    let every = "{ \\set Score.barNumberVisibility = #(every-nth-bar-number-visible 2) \\override Score.BarNumber.break-visibility = #end-of-line-invisible c'1 c' c' c' c' }";
    assert_eq!(drawn(every), drawn("{ c'1 c' c' c' c' }") + 2, "bars 2 and 4");
    let modulo = "{ \\set Score.barNumberVisibility = #(modulo-bar-number-visible 3 2) \\override Score.BarNumber.break-visibility = #all-visible c'1 c' c' c' c' c' }";
    assert_eq!(drawn(modulo), drawn("{ c'1 c' c' c' c' c' }") + 2, "bars 2 and 5");
    // Visibility alone keeps them at the lines' starts, as LilyPond's.
    assert_eq!(drawn("{ \\set Score.barNumberVisibility = #all-bar-numbers-visible c'1 c' c' c' }"), drawn(one_line));
    // Several lines: the line's first number, unless omitted.
    let long = "{ c'1 c' c' c' \\break c' c' c' c' }";
    let omitted = "{ \\omit Score.BarNumber c'1 c' c' c' \\break c' c' c' c' }";
    assert_eq!(drawn(omitted), drawn(long) - 1);
}

#[test]
fn settings_in_with_layout_voices_and_tweaks_hold_where_lilypond_holds_them() {
    // \layout's \context: every staff of its kind; the \with: that staff.
    let src = "\\score { << \\new Staff { c'4( d') } \\new Staff \\with { \\override Slur.direction = #DOWN } { c''4( d'') } >> \\layout { \\context { \\Staff \\override Slur.direction = #UP } } }";
    let p = read(src);
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    assert_eq!(notes(&p, 0)[0].look.dir("Slur"), Some(1));
    assert_eq!(notes(&p, 1)[0].look.dir("Slur"), Some(-1), "the staff's own over the layout's");
    // A \layout block's \context for \Voice, and a group's \with for its staves.
    let p = read("\\score { \\new PianoStaff \\with { \\override TextScript.font-size = #2 } << \\new Staff { c'4 } \\new Staff { c4 } >> \\layout { \\context { \\Voice \\stemDown } } }");
    assert_eq!(notes(&p, 1)[0].look.num("TextScript.font-size"), Some(2.0));
    assert!(p.warnings.is_empty(), "{:?}", p.warnings);
    assert_eq!(notes(&p, 0)[0].stem_dir, -1, "\\stemDown for every voice");
    // A voice's settings stay its own; the staff's hold for every voice.
    let p = read("{ << { \\slurUp \\override Staff.TimeSignature.style = #'numbered c''4( d'') } \\\\ { c'4( d') } >> c'4( d') }");
    let ns = notes(&p, 0);
    assert_eq!(ns[0].look.dir("Slur"), Some(1));
    assert_eq!(ns[2].look.dir("Slur"), None, "the second voice");
    assert_eq!(ns[4].look.dir("Slur"), None, "after the voices");
    assert_eq!(ns[4].look.get("TimeSignature.style"), Some(&Val::Sym("numbered".into())));
    // \tweak: the note after it, or the sign after a note.
    let p = read("{ \\tweak NoteHead.style #'cross c'4 d' e'-\\tweak Script.direction #DOWN -> }");
    let ns = notes(&p, 0);
    assert_eq!(ns[0].look.sym("NoteHead.style"), Some("cross"));
    assert_eq!(ns[1].look.sym("NoteHead.style"), None);
    assert_eq!(ns[2].look.dir("Script"), Some(-1));
    // \once: the next note; \revert and \unset: back to the default; `#'` paths.
    let p = read("{ \\once \\override Beam #'damping = #2 c'8 d' \\override Beam.damping = #3 e' \\revert Beam #'damping f' }");
    let damping: Vec<Option<f64>> = notes(&p, 0).iter().map(|e| e.look.num("Beam.damping")).collect();
    assert_eq!(damping, vec![Some(2.0), None, Some(3.0), None]);
}

#[test]
fn lyrics_aligned_and_settings_among_them_passed_over() {
    let src = "<< \\new Staff { \\new Voice = \"v\" { c'4 d' } } \\new Lyrics \\lyricsto \"v\" { Hal -- lo } >>";
    let left = format!("\\score {{ {src} \\layout {{ \\context {{ \\Lyrics \\override LyricText.self-alignment-X = #LEFT }} }} }}");
    assert!(read(&left).warnings.is_empty());
    let a = paths(&eng(src), 8.0, 0).unwrap().0;
    let b = paths(&eng(&left), 8.0, 0).unwrap().0;
    assert_eq!(a.len(), b.len());
    assert_ne!(a, b, "the syllables' left at their heads'");
    // Written among the words, it was refused; now read, and said to be passed over.
    let among = "{ c'4 d' } \\addlyrics { \\override LyricText.self-alignment-X = #LEFT Hal -- lo }";
    let p = read(among);
    assert_eq!(p.staves[0].lyrics.as_ref().unwrap().len(), 2);
    assert!(p.warnings.iter().any(|w| w.message == "\\override inside lyrics is not drawn"));
}
