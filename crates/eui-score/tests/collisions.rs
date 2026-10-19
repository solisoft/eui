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

/// A rest of one voice clears the notes the other voice sounds with it — in
/// Bach's C major prelude, the left hand's sixteenth rest drew on its middle C.
#[test]
fn a_rest_of_two_voices_clears_the_other_voices_notes() {
    let src = r"{ \clef bass \time 4/4 << { r16 e'8. ~ e'4 r16 e'8. ~ e'4 } \\ { c'2 c'2 } >> }";
    let eng = engrave(src, 120.0, &Layout::default()).unwrap();
    let items = &eng.staves[0][0].items;
    let c_top = items.iter().filter(|e| e.voice == Some(2) && !e.heads.is_empty()).map(|e| 2.0 - e.heads.last().unwrap().pos as f64 / 2.0 - 0.5).fold(f64::MAX, f64::min);
    let rests: Vec<_> = items.iter().filter(|e| e.voice == Some(1) && e.heads.is_empty()).collect();
    assert_eq!(rests.len(), 2);
    for rest in rests {
        // A rest's ink is 1 to 3 spaces under the top line, moved by its lift.
        let bottom = 3.0 + rest.rest_y.unwrap_or(-2.0);
        assert!(bottom <= c_top - 0.5, "the rest reaches {bottom}, the C's head is at {c_top}");
    }
}

/// An ending's number hangs under its bracket: a high note at the ending's
/// start was drawn through "1." (Ode to Joy, an a'' under the bracket).
#[test]
fn a_volta_number_clears_the_high_note_under_it() {
    let src = r"{ \key d \major \repeat volta 2 { d''1 } \alternative { { a''4 g'' fis'' e'' } { d''1 } } }";
    let eng = engrave(src, 120.0, &Layout::default()).unwrap();
    let mut seen = 0;
    for sys in &eng.systems {
        for line in &sys.lines {
            for vb in line.voltas.iter().filter(|v| v.first) {
                for &b in &vb.bars {
                    for ev in eng.staves[0][b].items.iter().filter(|e| !e.heads.is_empty() && e.x <= vb.x0 + 2.5) {
                        let head_top = 2.0 - ev.heads.last().unwrap().pos as f64 / 2.0 - 0.5;
                        // The number's baseline is 1.5 under the line.
                        assert!(vb.y + 1.5 + 0.2 <= head_top + 1e-9, "\"{}\" at {} runs into the head at {}", vb.label, vb.y, head_top);
                        seen += 1;
                    }
                }
            }
        }
    }
    assert!(seen >= 2, "both endings' first notes were laid out");
}
