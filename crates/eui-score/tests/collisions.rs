//! Marks that must not run into each other, each seen in a score and held
//! here: what LilyPond keeps apart, this keeps apart.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use eui_score::{engrave, Layout};

/// A repeat's ending bracket goes over the words above the staff — a chord
/// name over the ending's first note was drawn through "1." and "2.".
#[test]
fn a_volta_bracket_stands_over_the_words_under_it() {
    let src = r#"\relative c' { \key e \major
      \repeat volta 2 { e8^"E7" gis b cis d cis b gis | b8^"B7" dis fis gis a gis fis dis | }
      \alternative { { e,8^"E7" gis b cis d cis b gis | } { e8^"E7" gis b cis d cis b gis | } } }"#;
    let eng = engrave(src, 120.0, &Layout::default()).unwrap();
    let mut seen = 0;
    for sys in &eng.systems {
        for line in &sys.lines {
            for vb in &line.voltas {
                for tx in line.texts.iter().filter(|t| t.dir < 0 && t.x1 >= vb.x0 && t.x0 <= vb.x1) {
                    // The bracket's line, and its number hanging 1.8 under it, clear the words' top.
                    assert!(vb.y + 1.8 <= tx.top, "the bracket at {} reaches the words at {}", vb.y, tx.top);
                    seen += 1;
                }
            }
        }
    }
    assert!(seen >= 2, "the endings and their chord names were laid out");
}

/// Dynamics on notes side by side — `\fff \pp \p \fz` — stand apart instead
/// of reading as one word.
#[test]
fn dynamics_on_neighbouring_notes_do_not_overlap() {
    let eng = engrave(r"{ c''4\fff d''\pp e''\p f''\fz | g''1 }", 120.0, &Layout::default()).unwrap();
    let mut dyns: Vec<(f64, f64)> = eng.systems.iter().flat_map(|s| &s.lines).flat_map(|l| &l.dynamics).filter(|d| d.text.is_some()).map(|d| (d.x0, d.x1)).collect();
    dyns.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(dyns.len(), 4);
    for pair in dyns.windows(2) {
        assert!(pair[0].1 + 0.3 <= pair[1].0, "{:?} runs into {:?}", pair[0], pair[1]);
    }
}
