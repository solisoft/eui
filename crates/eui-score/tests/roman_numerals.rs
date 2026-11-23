//! Roman numeral analysis (score #1026): LilyPond ships no command for it;
//! the form found in the wild is LSR snippet 710's `\markup \rN { … }`,
//! written as syllables of a `Lyrics` context under the bass, and plain
//! markup — `\markup { "V" \super "7" }` — in the same place. Both are
//! drawn: the numeral, an accidental before it, the quality and a single
//! figure raised, two figures stacked, `/` and the numeral of a secondary
//! function.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::{parse, Kind, Run, Syllable};
use eui_score::{engrave, engrave_paper, paths, pdf, warnings, Layout};

/// The analysis as LSR 710 writes it: the snippet's markup command (Scheme,
/// read and passed over), a bass line, and the numerals as lyrics to it.
const SNIPPET: &str = r#"\version "2.24.0"
#(define-markup-command (rN layout props symbols) (markup-list?)
  "Roman numeral analysis."
  (interpret-markup layout props (make-line-markup symbols)))

bass = \relative c { \clef bass \key g \major \time 3/4 g4 c d | g, b' c | a d d, | g2. \bar "||" }

analysis = \lyricmode {
  \set stanza = #"G:   "
  \markup \rN { I }4
  \markup \rN { IV }
  \markup \rN { V 6 5 }
  \markup \rN { I }
  \markup \rN { vii o 4 3 / IV }
  \markup \rN { bVII }
  \markup \rN { V 7 / V }
  \markup \rN { ii h 7 }
  \markup \rN { V }
  \markup \rN { I }2.
}

\score {
  \new Staff <<
    \new Voice = "bass" { \bass }
    \new Lyrics \lyricsto "bass" { \analysis }
  >>
}
"#;

fn syllables(src: &str) -> Vec<Syllable> {
    let p = parse(src).unwrap_or_else(|e| panic!("{e}"));
    p.staves.iter().find_map(|s| s.lyrics.clone()).expect("lyrics")
}

/// A numeral's runs as text, `^` before a raised one, `_` before a lowered
/// one, `|` before one stacked under the one before, `♭` for a flat's glyph.
fn shown(runs: &[Run]) -> String {
    runs.iter()
        .map(|r| {
            let text = if r.glyph {
                match r.text.as_str() {
                    "accidentalFlat" => "♭".to_string(),
                    "accidentalSharp" => "♯".to_string(),
                    "accidentalNatural" => "♮".to_string(),
                    other => other.to_string(),
                }
            } else {
                r.text.clone()
            };
            format!(
                "{}{}{}",
                if r.stack { "|" } else { "" },
                if r.sup {
                    "^"
                } else if r.sub {
                    "_"
                } else {
                    ""
                },
                text
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn the_snippet_s_numerals_are_read_as_it_draws_them() {
    let got: Vec<String> = syllables(SNIPPET).iter().map(|s| shown(&s.runs)).collect();
    assert_eq!(got, vec!["I", "IV", "V ^6 |_5", "I", "vii ^° ^4 |_3 / IV", "♭ VII", "V ^7 / V", "ii ^ø ^7", "V", "I"]);
    // Each syllable's text is its plain words, for an editor and for width.
    assert_eq!(syllables(SNIPPET)[2].text, "V65");
    // The Scheme definition and the stanza are read and passed over.
    let said: Vec<String> = warnings(SNIPPET).unwrap().into_iter().map(|w| w.message).collect();
    assert!(!said.iter().any(|w| w.contains("rN")), "{said:?}");
}

#[test]
fn accidentals_qualities_and_note_names() {
    let one = |rn: &str| shown(&syllables(&format!("{{ c'4 }} \\addlyrics {{ \\markup \\rN {{ {rn} }} }}"))[0].runs);
    assert_eq!(one("svi"), "♯ vi");
    assert_eq!(one("nIII"), "♮ III");
    assert_eq!(one("III +"), "III ^+");
    assert_eq!(one("I M7"), "I ^M7");
    assert_eq!(one("V 9"), "V ^9");
    assert_eq!(one("Fr + 6"), "Fr ^+ ^6");
    assert_eq!(one("It 6"), "It ^6");
    assert_eq!(one("Cs"), "C ♯");
    assert_eq!(one("Eb"), "E ♭");
    assert_eq!(one("I 6 4"), "I ^6 |_4");
    assert_eq!(one("V 4 2 / bVI"), "V ^4 |_2 / ♭ VI");
}

#[test]
fn plain_markup_under_the_notes_raises_with_super_and_lowers_with_sub() {
    let src = "{ c'4 g' } \\addlyrics { \\markup { \"V\" \\super \"7\" } \\markup \\concat { V \\sub 2 } }";
    let got: Vec<String> = syllables(src).iter().map(|s| shown(&s.runs)).collect();
    assert_eq!(got, vec!["V ^7", "V _2"]);
    // \concat: no space between.
    let s = syllables(src);
    assert!(s[0].runs[1].gap && !s[1].runs[1].gap);
    // A word is still a word.
    assert!(syllables("{ c'4 } \\addlyrics { la }")[0].runs.is_empty());
}

#[test]
fn the_analysis_is_drawn_under_its_notes() {
    let eng = engrave(SNIPPET, 900.0 / 8.0, &Layout::default()).unwrap();
    let (drawn, _) = paths(&eng, 8.0, 0).unwrap();
    let bare = SNIPPET.replace("    \\new Lyrics \\lyricsto \"bass\" { \\analysis }\n", "");
    let (without, _) = paths(&engrave(&bare, 900.0 / 8.0, &Layout::default()).unwrap(), 8.0, 0).unwrap();
    // One path a character or a glyph: I IV V65 I vii°43/IV ♭VII V7/V iiø7 V I.
    assert_eq!(drawn.len() - without.len(), 1 + 2 + 3 + 1 + 9 + 4 + 4 + 4 + 1 + 1);
    // Each centred on its note.
    let notes: Vec<_> = eng.staves[0].iter().flat_map(|b| b.items.iter()).filter(|e| e.k == Kind::Note).collect();
    assert!(notes[2].lyric.as_ref().is_some_and(|l| l.runs.len() == 3));
    // A stacked pair is as wide as its wider figure: V65 narrower than V6 5 side by side.
    let w = |src: &str| {
        let eng = engrave(src, 900.0 / 8.0, &Layout::default()).unwrap();
        let ev = eng.staves[0][0].items[0].clone();
        ev.right + ev.left
    };
    assert!(w("{ c'4 } \\addlyrics { \\markup \\rN { V 6 5 } }") + 0.3 < w("{ c'4 } \\addlyrics { \\markup { V \\super 6 \\super 5 } }"));
    engrave_paper(SNIPPET).unwrap();
    assert!(pdf(SNIPPET, "rn").unwrap().starts_with("%PDF"));
}
