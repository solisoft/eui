//! `cargo run -p eui-score-web --example ly2svg -- a.ly b.ly …`: each source
//! engraved 900 px wide, printed as SVG after a `=== <path>` line, or the
//! engraver's refusal after `REFUSED`. How a change to the engraver is held
//! up against LilyPond's own picture of the same source.

use eui_score_web::{render, Options};

fn main() {
    for path in std::env::args().skip(1) {
        let src = std::fs::read_to_string(&path).unwrap_or_default();
        let drawn = render(&src, &Options { width: 900.0, ..Options::default() });
        println!("=== {path}");
        if drawn.refusal.is_empty() {
            println!("{}", drawn.svg);
        } else {
            println!("REFUSED {}", drawn.refusal);
        }
    }
}
