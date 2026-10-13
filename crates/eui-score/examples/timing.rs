//! How long a source takes to engrave and draw, the measurement
//! `doc/docs/eui/status.md` prints:
//!
//! ```text
//! cargo run --release -p eui-score --example timing -- piece.ly [width]
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::time::Instant;

use eui_score::{engrave, engrave_a4, midi, paths, pdf, Layout, MidiOptions};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = std::fs::read_to_string(args.get(1).expect("a .ly file")).expect("readable");
    let width: f64 = args.get(2).and_then(|w| w.parse().ok()).unwrap_or(900.0);
    let runs = 10;
    let time = |what: &str, f: &dyn Fn() -> usize| {
        let mut best = f64::MAX;
        let mut size = 0;
        for _ in 0..runs {
            let t = Instant::now();
            size = f();
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        }
        println!("{what:<28} {best:>9.2} ms   ({size})");
    };
    time("engrave, screen", &|| engrave(&src, width / 8.0, &Layout::default()).unwrap().systems.len());
    let eng = engrave(&src, width / 8.0, &Layout::default()).unwrap();
    time("paths, screen", &|| paths(&eng, 8.0, 0).unwrap().0.len());
    time("engrave, A4", &|| engrave_a4(&src).unwrap().pages.len());
    time("midi", &|| midi(&src, &MidiOptions::default()).unwrap().len());
    time("pdf", &|| pdf(&src, "").unwrap().len());
}
