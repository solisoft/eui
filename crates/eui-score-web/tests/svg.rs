//! The SVG a page gets is the engraving a `score` node draws (03 §1.4).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_score_web::{render, Options};

const FRERE: &str = include_str!("../../eui-score/tests/corpus/frere_jacques.ly");
const BEETHOVEN: &str = include_str!("../../../examples/demo-app/public/scores/beethoven-symphony-5-1.ly");

fn count(svg: &str, tag: &str) -> usize {
    svg.matches(&format!("<{tag} ")).count()
}

#[test]
fn a_page_is_the_engravers_paths_as_svg() {
    let o = Options { width: 600.0, ..Options::default() };
    let drawn = render(FRERE, &o);
    assert!(drawn.refusal.is_empty(), "{}", drawn.refusal);
    let eng = eui_score::engrave(FRERE, 600.0 / 8.0, &eui_score::Layout::default()).unwrap();
    let (paths, _) = eui_score::paths(&eng, 8.0, 0).unwrap();
    let rects = paths.iter().filter(|p| matches!(p, eui_score::Path::Rect(_))).count();
    assert_eq!(count(&drawn.svg, "rect"), rects, "one <rect> a rectangle");
    assert_eq!(count(&drawn.svg, "path"), paths.len() - rects, "one <path> a fill");
    assert!(drawn.svg.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 "#), "{}", &drawn.svg[..120]);
    assert!(drawn.svg.contains(r#"fill="currentColor""#), "the page's colour is the ink");
    assert_eq!(drawn.height, (eng.height * 8.0).ceil(), "one page as tall as the music");
    assert_eq!(drawn.pages, 1);
}

#[test]
fn a_source_it_cannot_set_draws_nothing_and_says_why() {
    let drawn = render("{ c'4 \\nonsense }", &Options::default());
    assert!(drawn.svg.is_empty());
    assert!(!drawn.refusal.is_empty());
    assert!(!render("{ c'4 }", &Options { width: 0.0, ..Options::default() }).refusal.is_empty(), "no width, no music");
}

#[test]
fn pages_are_counted_and_one_past_the_last_draws_the_last() {
    let o = Options { width: 700.0, page_height: 700.0, ..Options::default() };
    let first = render(BEETHOVEN, &o);
    assert!(first.refusal.is_empty(), "{}", first.refusal);
    assert!(first.pages > 1, "Beethoven's exposition takes pages: {}", first.pages);
    let past = render(BEETHOVEN, &Options { page: 999, ..o });
    assert_eq!(past.page, first.pages - 1);
    let last = render(BEETHOVEN, &Options { page: first.pages - 1, ..o });
    assert_eq!(past.svg, last.svg);
    eprintln!("Beethoven, page 1 of {}: {} bytes of SVG", first.pages, first.svg.len());
}

#[test]
fn on_a4_the_sheet_is_the_page() {
    let drawn = render(FRERE, &Options { width: 595.0, a4: true, ..Options::default() });
    assert!(drawn.refusal.is_empty(), "{}", drawn.refusal);
    assert!((drawn.height - (595.0_f64 * 297.0 / 210.0).floor()).abs() <= 1.0, "an A4 sheet: {}", drawn.height);
}
