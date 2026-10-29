//! LilyPond 2.24's ways of writing marks, sections, endings and harmonics
//! (score #433): each used to refuse the whole source, and now draws what
//! LilyPond draws.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::measure::measures;
use eui_score::parse::{parse, Kind, Parsed};
use eui_score::{engrave, engrave_a4, Layout};

fn read(src: &str) -> Parsed {
    parse(src).unwrap_or_else(|e| panic!("{src:?}: {e}"))
}

fn engraves(src: &str) {
    engrave(src, 60.0, &Layout::default()).unwrap_or_else(|e| panic!("{src:?}: {e}"));
    engrave_a4(src).unwrap_or_else(|e| panic!("{src:?} on A4: {e}"));
}

fn marks(p: &Parsed) -> Vec<&eui_score::parse::Ev> {
    p.staves[0].events.iter().filter(|e| e.k == Kind::Mark).collect()
}

#[test]
fn a_segno_or_coda_mark_takes_a_number_or_default_and_draws_the_same_sign() {
    for src in [r"{ c'1 \segnoMark 2 d'1 \codaMark 1 e'1 }", r"{ c'1 \segnoMark \default d'1 \codaMark #1 e'1 }", r"{ c'1 \segnoMark d'1 \codaMark e'1 }"] {
        let p = read(src);
        let signs: Vec<&str> = marks(&p).iter().map(|m| m.texts[0].runs[0].text.as_str()).collect();
        assert_eq!(signs, ["segno", "coda"], "{src}");
        assert!(marks(&p).iter().all(|m| m.texts[0].runs[0].glyph), "{src}: drawn as signs");
        let notes = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).count();
        assert_eq!(notes, 3, "{src}: the number is not read as a note");
        engraves(src);
    }
    assert!(parse(r"{ c'1 \segnoMark #x d'1 }").is_err(), "only a number or \\default goes after it");
}

#[test]
fn a_segno_or_coda_over_a_note_is_its_sign() {
    for src in [r"{ c'4\segno d' e' f'\coda }", r"{ c'4 \segno d' e' f' \varcoda }"] {
        let p = read(src);
        let notes: Vec<_> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).collect();
        assert_eq!(notes[0].scripts[0].name, "segno", "{src}");
        assert!(["coda", "varcoda"].contains(&notes[3].scripts[0].name.as_str()), "{src}");
        engraves(src);
    }
    assert!(parse(r"{ \segno c'4 }").is_err(), "a sign over a note goes after it");
}

#[test]
fn a_section_is_a_double_bar_line_and_its_label_bold_words_over_it() {
    let src = r#"{ c'1 \section \sectionLabel "Trio" d'1 }"#;
    let p = read(src);
    let bars: Vec<_> = p.staves[0].events.iter().filter(|e| e.k == Kind::Bar).collect();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].v, "||");
    let label = &marks(&p)[0].texts[0];
    assert_eq!((label.runs[0].text.as_str(), label.dir), ("Trio", -1));
    assert_eq!(label.runs[0].weight, eui_score::glyphs::Weight::Bold);
    let measured = measures(&p.staves[0].events).unwrap();
    assert_eq!(measured[1].marks.len(), 1, "the label starts the second bar");
    engraves(src);
    assert!(parse(r"{ c'1 \sectionLabel d'1 }").is_err(), "a label wants its words");
}

#[test]
fn a_text_mark_stands_over_its_bar_line_and_a_text_end_mark_ends_the_bar() {
    let src = r#"{ c'1 \textMark "To Coda" d'1 \textEndMark \markup \italic "D.C." }"#;
    let p = read(src);
    let m = marks(&p);
    assert_eq!(m.len(), 2);
    assert_eq!((m[0].texts[0].runs[0].text.as_str(), m[0].texts[0].dir), ("To Coda", -1));
    assert_eq!(m[1].texts[0].dir, 1, "at the end of its bar, as a jump");
    let measured = measures(&p.staves[0].events).unwrap();
    assert_eq!(measured[1].marks.last().unwrap().0, measured[1].length, "D.C. ends the last bar");
    engraves(src);
    assert!(parse(r"{ c'1 \textMark 3 }").is_err());
}

#[test]
fn volta_numbers_label_the_endings_they_name() {
    let src = r"{ \repeat volta 3 { c'1 } \alternative { \volta 1,2 { d'1 } \volta 3 { e'1 } } f'1 }";
    let p = read(src);
    let labels: Vec<&str> = p.staves[0].events.iter().filter(|e| e.k == Kind::VoltaStart).map(|e| e.v.as_str()).collect();
    assert_eq!(labels, ["1., 2.", "3."]);
    let bars = measures(&p.staves[0].events).unwrap();
    assert_eq!(bars[1].volta_in.as_deref(), Some("1., 2."));
    assert_eq!(bars[2].volta_in.as_deref(), Some("3."));
    engraves(src);
    // Without \volta, the endings are numbered in turn, as before.
    let plain = read(r"{ \repeat volta 2 { c'1 } \alternative { { d'1 } { e'1 } } }");
    let labels: Vec<&str> = plain.staves[0].events.iter().filter(|e| e.k == Kind::VoltaStart).map(|e| e.v.as_str()).collect();
    assert_eq!(labels, ["1.", "2."]);
    assert!(parse(r"{ \repeat volta 2 { c'1 } \alternative { \volta { d'1 } { e'1 } } }").is_err(), "\\volta names its passes");
    assert!(parse(r"{ \repeat volta 2 { c'1 } \alternative { \volta 1 d'1 } }").is_err(), "and is followed by its ending");
}

#[test]
fn an_unfolded_repeat_plays_the_ending_each_pass_names() {
    let src = r"{ \repeat unfold 3 { c'1 } \alternative { \volta 1,3 { d'1 } \volta 2 { e'1 } } }";
    let p = read(src);
    let steps: Vec<i64> = p.staves[0].events.iter().filter(|e| e.k == Kind::Note).map(|e| e.pitches[0].d).collect();
    let reference = read(r"{ c'1 d'1 e'1 }");
    let written: Vec<i64> = reference.staves[0].events.iter().filter(|e| e.k == Kind::Note).map(|e| e.pitches[0].d).collect();
    let (c, d, e) = (written[0], written[1], written[2]);
    assert_eq!(steps, [c, d, c, e, c, d], "pass 1: d, pass 2: e, pass 3: d");
}

#[test]
fn a_harmonic_head_is_a_diamond() {
    let src = r"{ c''4\harmonic d'' <e' a'\harmonic>2 }";
    let p = read(src);
    let bars = measures(&p.staves[0].events).unwrap();
    let styles: Vec<Vec<&str>> = bars[0].items.iter().map(|e| e.heads.iter().map(|h| h.style).collect()).collect();
    assert_eq!(styles, [vec!["Harmonic"], vec![""], vec!["", "Harmonic"]], "only the heads written \\harmonic");
    engraves(src);
    assert!(parse(r"{ \harmonic c'4 }").is_err(), "a harmonic goes after its note");
}
