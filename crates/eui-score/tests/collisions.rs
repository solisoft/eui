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

/// A tempo mark over grace notes high above the staff stands over them —
/// it was drawn through two acciaccaturas before a high G.
#[test]
fn a_tempo_mark_clears_the_grace_notes_under_it() {
    let src = r#"{ \tempo "Allegro assai" 4 = 116 \key d \major \acciaccatura { a''16 c'''16 } g''4\mf fis'' g'' a'' | a''4 g'' fis'' e'' }"#;
    let eng = engrave(src, 120.0, &Layout::default()).unwrap();
    let mark = eng.systems.iter().flat_map(|s| &s.lines).flat_map(|l| &l.marks).next().expect("the tempo mark");
    let first = eng.staves[0][0].items.iter().find(|e| !e.graces.is_empty()).expect("the note with grace notes");
    let mut seen = 0;
    for g in &first.graces {
        let head_top = 2.0 - g.heads.last().unwrap().pos as f64 / 2.0 - 0.4;
        // The mark's baseline stands clear of every grace note's head and stem.
        let reach = head_top.min(g.stem_end);
        assert!(mark.y + 0.3 <= reach, "the mark at {} runs into a grace note reaching {reach}", mark.y);
        seen += 1;
    }
    assert_eq!(seen, 2);
}

/// A whole note's tremolo strokes hang under its head (its stem would go
/// down): a dynamic under the note goes under them, not through them —
/// the "p" of `c''1:32\p\<` was drawn across the last stroke (score #982).
#[test]
fn a_dynamic_goes_under_a_whole_notes_tremolo() {
    let dyn_y = |src: &str| {
        let eng = engrave(src, 120.0, &Layout::default()).unwrap();
        let line = &eng.systems[0].lines[0];
        let d = line.dynamics.iter().find(|d| d.text.is_some()).expect("a dynamic");
        (d.y, d.y + d.top)
    };
    let (plain, _) = dyn_y(r"{ c''1\p }");
    let (y, ink_top) = dyn_y(r"{ c''1:32\p\< d''1\! }");
    assert!(y > plain, "lower with the strokes ({y}) than without ({plain})");
    // c'' is a space and a half under the top line; three strokes start
    // 1.4 under its middle, 0.75 apart, half a beam and half a rise thick.
    let c2 = 1.5;
    let strokes_bottom = c2 + 1.4 + 2.0 * 0.75 + (0.5 + 0.48) / 2.0;
    assert!(ink_top >= strokes_bottom, "the dynamic's top ({ink_top}) clears the strokes ({strokes_bottom})");
    // A low note's strokes go over its head: under it, nothing moves.
    assert_eq!(dyn_y(r"{ c'1:32\p }").0, dyn_y(r"{ c'1\p }").0);
}

/// Ink in bands an eighth of a space high: `(band, left, right)`.
type Bands = Vec<(i64, f64, f64)>;

/// A glyph's ink, band by band, from its outline, the bands counted down
/// from the glyph's origin.
fn ink_bands(name: &str) -> Bands {
    let fill = &eui_score::glyphs::glyph(name).fill;
    let mut bands: std::collections::BTreeMap<i64, (f64, f64)> = std::collections::BTreeMap::new();
    let mut seen = |x: f64, y: f64| {
        // A point on a band's edge is the edge of the ink, in neither band.
        if (y * 8.0 - (y * 8.0).round()).abs() < 1e-6 {
            return;
        }
        let e = bands.entry((y * 8.0).floor() as i64).or_insert((x, x));
        *e = (e.0.min(x), e.1.max(x));
    };
    let (mut i, mut at) = (0, (0.0, 0.0));
    while i < fill.len() {
        let op = fill[i] as i64;
        match op {
            0 => {
                at = (fill[i + 1], fill[i + 2]);
                i += 3;
            }
            1 => {
                let to = (fill[i + 1], fill[i + 2]);
                for s in 0..=200 {
                    let t = s as f64 / 200.0;
                    seen(at.0 + (to.0 - at.0) * t, at.1 + (to.1 - at.1) * t);
                }
                at = to;
                i += 3;
            }
            3 => {
                let p = [at, (fill[i + 1], fill[i + 2]), (fill[i + 3], fill[i + 4]), (fill[i + 5], fill[i + 6])];
                for s in 0..=200 {
                    let t = s as f64 / 200.0;
                    let u = 1.0 - t;
                    let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    seen(w.iter().zip(&p).map(|(w, p)| w * p.0).sum(), w.iter().zip(&p).map(|(w, p)| w * p.1).sum());
                }
                at = p[3];
                i += 7;
            }
            _ => i += 1,
        }
    }
    bands.into_iter().map(|(b, (l, r))| (b, l, r)).collect()
}

/// The accidentals of the first chord of `src`: each one's ink bands on the
/// staff (band, left, right — x from the note's, bands down from the top
/// line), then the room they take left of the note.
fn chord_accidentals(src: &str) -> (Vec<Bands>, f64) {
    let eng = engrave(src, 120.0, &Layout::default()).unwrap();
    let ev = eng.staves[0][0].items.iter().find(|e| e.heads.len() > 1).expect("a chord");
    let names = |a: i64| ["accidentalDoubleFlat", "accidentalFlat", "accidentalNatural", "accidentalSharp", "accidentalDoubleSharp"][(a + 2) as usize];
    let mut out = Vec::new();
    let mut width: f64 = 0.0;
    for hd in &ev.heads {
        let Some(acc) = hd.acc else { continue };
        let x = hd.acc_x.expect("a placed accidental");
        width = width.max(-x);
        out.push(ink_bands(names(acc)).into_iter().map(|(b, l, r)| (b + 4 * (4 - hd.pos), x + l, x + r)).collect());
    }
    (out, width)
}

/// No accidental of a chord in another's ink, nor in a head: band by band,
/// 0.1 space of white between them at least.
fn assert_accidentals_clear(src: &str) {
    let (accs, _) = chord_accidentals(src);
    assert!(accs.len() >= 2, "{src}: accidentals to hold apart");
    for (i, a) in accs.iter().enumerate() {
        for &(band, _, right) in a {
            assert!(right <= -0.1, "{src}: accidental {i} reaches {right} in band {band}, the heads start at 0");
        }
        for b in accs.iter().skip(i + 1) {
            for &(band, l0, r0) in a {
                for &(_, l1, r1) in b.iter().filter(|x| x.0 == band) {
                    assert!(r0 + 0.1 <= l1 || r1 + 0.1 <= l0, "{src}: in band {band} one accidental's ink is at {l0}..{r0}, another's at {l1}..{r1}");
                }
            }
        }
    }
}

/// A chord's accidentals as LilyPond places them (score #1019): the top
/// one nearest the heads, then the bottom one, then the second from the top
/// — and each against the ink already there, not against its box. Two flats
/// a sixth apart shared a column, the lower one's stem touching the upper
/// one's bowl.
#[test]
fn a_chords_accidentals_never_touch() {
    for src in [
        "{ <des' bes'>1 }",
        "{ <cis' cis''>1 }",
        "{ <des' fis' bes' dis''>1 }",
        "{ <fis' ais' cis'' eis''>1 }",
        "{ <cis' dis'' cis''>1 }",
        "{ <ees' ges' bes' des'' fes''>1 }",
        "{ <c' des' e' fis'>1 }",
        "{ \\key e \\major <c' d' f' g' c''>1 }",
        "{ <ceses' eis' gisis' bes'>1 }",
    ] {
        assert_accidentals_clear(src);
    }
}

/// … and they take no more room than they did in columns of whole boxes:
/// a flat goes under the sharp over it as far as its ink lets it.
#[test]
fn a_chords_accidentals_take_no_more_room_than_columns_did() {
    // The widths the columns took, before score #1019.
    for (src, before) in [("{ <cis' cis''>1 }", 1.3), ("{ <des' fis' bes' dis''>1 }", 3.6), ("{ <fis' ais' cis'' eis''>1 }", 3.9)] {
        let (_, width) = chord_accidentals(src);
        assert!(width <= before + 1e-9, "{src}: {width} wide, the columns took {before}");
    }
    let (_, tucked) = chord_accidentals("{ <des' fis' bes' dis''>1 }");
    assert!(tucked < 3.6 - 0.05, "the flats go under the sharps: {tucked}");
}

/// Notes an octave apart carry their accidentals in one column, whatever
/// stands between them.
#[test]
fn octaves_share_their_accidentals_column() {
    for src in ["{ <cis' cis''>1 }", "{ <cis' cis'' dis''>1 }", "{ <ees' g' bes' ees''>1 }", "{ <fis ais cis' fis' ais'>1 }"] {
        let eng = engrave(src, 120.0, &Layout::default()).unwrap();
        let ev = eng.staves[0][0].items.iter().find(|e| e.heads.len() > 1).expect("a chord");
        let accs: Vec<_> = ev.heads.iter().filter(|h| h.acc.is_some()).collect();
        for a in &accs {
            for b in accs.iter().filter(|b| (b.pos - a.pos) % 7 == 0) {
                assert_eq!(a.acc_x, b.acc_x, "{src}: the octave {} / {} in two columns", a.pos, b.pos);
            }
        }
    }
}
