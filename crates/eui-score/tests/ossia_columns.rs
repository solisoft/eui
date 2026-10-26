//! An ossia's notes in the main staff's columns (score #407), as the score
//! app draws them — `engrave` then `bands`, a band a staff: the same moment
//! at the same x, its bar lines on the main staff's, its eighths beamed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score::parse::Kind;
use eui_score::{bands, engrave, Layout, Path};

const SP: f64 = 6.0;

fn ossia(with: &str) -> String {
    format!(r#"\new Staff = "main" \relative c'' {{ c4 d e f | << {{ g4 f e d }} \new Staff \with {{ alignAboveContext = "main" {with} }} {{ g8 a g f e4 d }} >> | c1 }}"#)
}

#[test]
fn an_ossia_stands_in_the_main_staffs_columns() {
    for with in ["", r"\magnifyStaff #2/3"] {
        let src = ossia(with);
        let e = engrave(&src, 700.0 / SP, &Layout::default()).unwrap();
        // The same moment, the same x: every main note has the ossia's over it.
        let (o, m) = (&e.staves[0][1], &e.staves[1][1]);
        for n in m.items.iter().filter(|ev| ev.k == Kind::Note) {
            let over = o.items.iter().find(|ev| ev.start == n.start && ev.k == Kind::Note).unwrap();
            assert_eq!(over.x, n.x, "{with}: at {}", n.start);
        }
        // Its bar's bounds are the main staff's.
        assert_eq!((o.x0, o.x1), (m.x0, m.x1));
        // Its eighths beamed, four in the half bar, as 4/4 beams them.
        assert_eq!(o.beams.iter().map(|b| b.notes.len()).collect::<Vec<_>>(), vec![4], "{with}");
        // Drawn so: in the bands the score app draws, the ossia's bar lines
        // (a thin line 1.1 px wide at this size, a stem 1) stand on the main staff's.
        let total = (e.height * SP).ceil() as i64;
        let bars: Vec<Vec<f64>> = bands(&e, SP, 0, total)
            .unwrap()
            .iter()
            .skip(1)
            .map(|b| b.paths.iter().filter_map(|p| if let Path::Rect([x, _, w, h]) = p { (*w > 1.05 && *w < 2.0 && *h > 12.0).then_some(*x) } else { None }).collect())
            .collect();
        assert_eq!(bars[0].len(), 2, "{with}: the ossia's two bar lines {:?}", bars[0]);
        for x in &bars[0] {
            assert!(bars[1].iter().any(|m| (m - x).abs() < 0.05), "{with}: {x} not on {:?}", bars[1]);
        }
    }
}
