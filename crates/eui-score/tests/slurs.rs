//! Slurs chosen as LilyPond's slur scoring chooses them, reduced (score
//! #1019): clear of the stems, the accidentals and the articulations they go
//! round, measured on the curve as it is drawn; and the ties left as they
//! were.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use eui_score::glyphs::glyph;
use eui_score::marks::{CurveKind, CurvePlace};
use eui_score::parse::Ev;
use eui_score::{engrave, Engraved, Layout, RULE};

fn set(src: &str) -> Engraved {
    engrave(src, 120.0, &Layout::default()).unwrap()
}

fn notes(eng: &Engraved) -> Vec<&Ev> {
    eng.staves[0].iter().flat_map(|b| &b.items).filter(|e| !e.heads.is_empty()).collect()
}

fn curves(eng: &Engraved, kind: CurveKind) -> Vec<&CurvePlace> {
    eng.systems.iter().flat_map(|s| &s.lines).flat_map(|l| &l.curve_places).filter(|c| c.kind == kind).collect()
}

/// The curve's middle line, 400 points of it.
fn along(c: &CurvePlace) -> Vec<(f64, f64)> {
    let p = &c.points;
    (0..=400)
        .map(|k| {
            let t = f64::from(k) / 400.0;
            let u = 1.0 - t;
            let (a, b, d, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            (a * p[0].0 + b * p[1].0 + d * p[2].0 + e * p[3].0, a * p[0].1 + b * p[1].1 + d * p[2].1 + e * p[3].1)
        })
        .collect()
}

/// How far the curve's ink is from a box `[left, top, right, bottom]`: its
/// middle line's distance, less half its thickness.
fn gap(c: &CurvePlace, b: [f64; 4]) -> f64 {
    along(c)
        .iter()
        .map(|&(x, y)| {
            let dx = (b[0] - x).max(x - b[2]).max(0.0);
            let dy = (b[1] - y).max(y - b[3]).max(0.0);
            (dx * dx + dy * dy).sqrt()
        })
        .fold(f64::MAX, f64::min)
        - RULE.curve_mid / 2.0
}

fn y_of(pos: i64) -> f64 {
    2.0 - pos as f64 / 2.0
}

/// A note's stem, as a box.
fn stem(ev: &Ev) -> [f64; 4] {
    let x = if ev.up { ev.x + ev.head_w.unwrap() } else { ev.x };
    let head = y_of(if ev.up { ev.heads[0].pos } else { ev.heads.last().unwrap().pos });
    [x - RULE.stem, head.min(ev.stem_end), x + RULE.stem, head.max(ev.stem_end)]
}

/// A head, as a box.
fn head(ev: &Ev, k: usize) -> [f64; 4] {
    let y = y_of(ev.heads[k].pos);
    [ev.x, y - 0.5, ev.x + ev.head_w.unwrap(), y + 0.5]
}

/// An accent under a note inside a slur under the stems: the slur goes
/// round it, and round the heads, by a third of a space at least.
#[test]
fn a_slur_goes_round_an_accent_next_to_its_note() {
    let eng = set(r"{ c''8( \stemUp g'-> a' c'') }");
    let slurs = curves(&eng, CurveKind::Slur);
    assert_eq!(slurs.len(), 1);
    let slur = slurs[0];
    assert!(!slur.up, "under the stems");
    let line = &eng.systems[0].lines[0];
    assert_eq!(line.scripts.len(), 1);
    let accent = line.scripts[0].bbox();
    assert!(gap(slur, accent) >= 0.3, "the slur is {} from the accent", gap(slur, accent));
    // The accent is where it was without the slur: next to its note.
    let alone = set(r"{ c''8 \stemUp g'-> a' c'' }");
    assert_eq!(alone.systems[0].lines[0].scripts[0].bbox(), accent);
    let ns = notes(&eng);
    for inner in &ns[1..3] {
        assert!(gap(slur, head(inner, 0)) >= 0.3, "the slur is {} from a head", gap(slur, head(inner, 0)));
    }
    for ev in &ns {
        assert!(gap(slur, stem(ev)) >= 0.3, "the slur is {} from a stem", gap(slur, stem(ev)));
    }
}

/// A staccato over a note two octaves up, inside a slur over it.
#[test]
fn a_slur_goes_round_a_staccato_and_the_stem_under_it() {
    let eng = set(r"{ c'4( c'''-. c') }");
    let slur = curves(&eng, CurveKind::Slur)[0];
    assert!(slur.up);
    let line = &eng.systems[0].lines[0];
    let dot = line.scripts[0].bbox();
    let ns = notes(&eng);
    assert!(dot[3] <= y_of(ns[1].heads[0].pos) - 0.5, "the staccato is over its head");
    assert!(gap(slur, dot) >= 0.3, "the slur is {} from the staccato", gap(slur, dot));
    assert!(gap(slur, head(ns[1], 0)) >= 0.3);
    for ev in &ns {
        assert!(gap(slur, stem(ev)) >= 0.3, "the slur is {} from a stem", gap(slur, stem(ev)));
    }
    // The arch is over the staccato, not through it: its lowest point there is above the dot.
    let over = along(slur).iter().filter(|p| p.0 >= dot[0] && p.0 <= dot[2]).map(|p| p.1).fold(f64::MIN, f64::max);
    assert!(over < dot[1], "the slur passes over the staccato");
}

/// A sharp before a note inside a slur: the slur clears the sign as it
/// clears the head.
#[test]
fn a_slur_goes_round_an_accidental() {
    let eng = set(r"{ e'8( gis''! e') }");
    let slur = curves(&eng, CurveKind::Slur)[0];
    let ns = notes(&eng);
    let gis = ns[1];
    let hd = &gis.heads[0];
    assert!(hd.acc.is_some(), "the sharp is printed");
    let bb = glyph("accidentalSharp").bbox;
    let x = gis.x + hd.acc_x.unwrap();
    let sharp = [x + bb[0], y_of(hd.pos) + bb[1], x + bb[2], y_of(hd.pos) + bb[3]];
    assert!(gap(slur, sharp) >= 0.3, "the slur is {} from the sharp", gap(slur, sharp));
    assert!(gap(slur, head(gis, 0)) >= 0.3);
    for ev in &ns {
        assert!(gap(slur, stem(ev)) >= 0.3, "the slur is {} from a stem", gap(slur, stem(ev)));
    }
}

/// Between two heads, a slur follows the notes: an end half a space off
/// each head, as LilyPond's, and no steeper than they are.
#[test]
fn a_slur_starts_and_ends_half_a_space_off_its_heads() {
    let eng = set(r"{ c'4( e') g'( g') }");
    let slurs = curves(&eng, CurveKind::Slur);
    let ns = notes(&eng);
    assert_eq!(slurs.len(), 2);
    // Stems up: under the heads.
    for (slur, pair) in slurs.iter().zip(ns.chunks(2)) {
        assert!(!slur.up);
        for (end, ev) in [(slur.points[0], pair[0]), (slur.points[3], pair[1])] {
            let under = end.1 - (y_of(ev.heads[0].pos) + 0.5);
            assert!((0.45..=0.7).contains(&under), "an end is {under} under its head");
            assert!((end.0 - (ev.x + ev.head_w.unwrap() / 2.0)).abs() < 0.01, "an end is under its head's middle");
        }
    }
    // Up a third: the slur rises with it; a unison: level.
    let rise = slurs[0].points[3].1 - slurs[0].points[0].1;
    assert!((-1.2..=-0.8).contains(&rise), "the slur rises {rise}");
    assert_eq!(slurs[1].points[0].1, slurs[1].points[3].1);
}

/// What stands outside a slur — a fermata, a fingering — is set past it.
#[test]
fn a_fermata_and_a_fingering_stand_outside_the_slur() {
    let eng = set(r"{ \slurUp c''4( d''\fermata e''-3 f'') }");
    let slur = curves(&eng, CurveKind::Slur)[0];
    let line = &eng.systems[0].lines[0];
    assert_eq!(line.scripts.len(), 2);
    for sc in &line.scripts {
        let b = sc.bbox();
        let top = along(slur).iter().filter(|p| p.0 >= b[0] && p.0 <= b[2]).map(|p| p.1).fold(f64::MAX, f64::min);
        assert!(b[3] <= top - 0.1, "a mark at {} reaches into the slur at {top}", b[3]);
    }
}

/// A phrasing slur over a slur goes round it.
#[test]
fn a_phrasing_slur_goes_round_the_slur_under_it() {
    let eng = set(r"{ \stemDown c''4\( d''( e'') f''\) }");
    let slur = curves(&eng, CurveKind::Slur)[0];
    let phrase = curves(&eng, CurveKind::Phrase)[0];
    let mut least = f64::MAX;
    for &(x, y) in &along(slur) {
        let over = along(phrase).iter().filter(|p| (p.0 - x).abs() < 0.05).map(|p| p.1).fold(f64::MIN, f64::max);
        if over > f64::MIN {
            least = least.min(y - over);
        }
    }
    assert!(least >= 0.3, "the phrasing slur is {least} over the slur");
}

/// The ties are what they were: a third of a space off their heads, from
/// just after one to just before the next, and each says the head it holds.
#[test]
fn ties_are_left_as_they_were() {
    let eng = set(r"{ c''4~ c'' <c' e' g'>2~ | <c' e' g'>1 }");
    let ties = curves(&eng, CurveKind::Tie);
    let ns = notes(&eng);
    assert_eq!(ties.len(), 4);
    let (a, b) = (ns[0], ns[1]);
    let tie = ties[0];
    assert_eq!((tie.item, tie.at, tie.head), (a.i, a.at, Some(a.heads[0].d)));
    assert!((tie.points[0].0 - (a.x + a.head_w.unwrap() + 0.15)).abs() < 1e-9);
    assert!((tie.points[3].0 - (b.x - 0.15)).abs() < 1e-9);
    // Stem down: over the head.
    assert!(tie.up);
    assert!((tie.points[0].1 - (y_of(a.heads[0].pos) - 0.35)).abs() < 1e-9);
    assert_eq!(tie.points[0].1, tie.points[3].1);
    // The chord's: one a head, the lower under and the upper over.
    let heads: Vec<Option<i64>> = ties[1..].iter().map(|t| t.head).collect();
    assert_eq!(heads, ns[2].heads.iter().map(|h| Some(h.d)).collect::<Vec<_>>());
    assert_eq!(ties[1..].iter().map(|t| t.up).collect::<Vec<_>>(), [false, true, true]);
}

/// Each curve says where it comes from: the note it starts on, by its
/// number and its place in the source, and the piece a line break makes.
#[test]
fn a_curve_says_the_note_it_starts_on() {
    let src = r"{ c'4( d') e'\( f'\) }";
    let eng = set(src);
    let ns = notes(&eng);
    let line = &eng.systems[0].lines[0];
    assert_eq!(line.curves.len(), line.curve_places.len());
    let slur = curves(&eng, CurveKind::Slur)[0];
    assert_eq!((slur.item, slur.at, slur.piece, slur.head), (ns[0].i, ns[0].at, 0, None));
    assert_eq!(src.chars().nth(slur.at), Some('c'));
    let phrase = curves(&eng, CurveKind::Phrase)[0];
    assert_eq!((phrase.item, phrase.at), (ns[2].i, ns[2].at));
    // Over a line break: two pieces, the second's saying so.
    let broken = set(r"{ c'1( \break d') }");
    let pieces = curves(&broken, CurveKind::Slur);
    assert_eq!(pieces.iter().map(|p| p.piece).collect::<Vec<_>>(), [0, 1]);
    assert!(pieces.iter().all(|p| p.item == pieces[0].item));
}
