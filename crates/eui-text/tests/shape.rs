//! Shaping against the embedded faces: real metrics, wrapping, clamping,
//! caching, rasterisation, and a run through the layout engine.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_layout::{Env, FontSpec, Layout, Size, TextMeasurer};
use eui_proto::*;
use eui_text::TextEngine;
use eui_theme::{Theme, Viewer};
use eui_tree::Session;

fn base() -> FontSpec {
    FontSpec { family: FontFamily::Sans, weight: FontWeight::Regular, size: 15.0, line_height: 22.0 }
}

#[test]
fn embedded_faces_are_loaded_and_nothing_else() {
    let e = TextEngine::new();
    assert_eq!(e.face_count(), 4, "Inter regular, Inter bold, JetBrains Mono, Noto Sans Symbols");
}

#[test]
fn a_word_measures_plausibly() {
    let mut e = TextEngine::new();
    let m = e.measure("Hello", base(), None, 0);
    assert!(m.width > 25.0 && m.width < 60.0, "width {}", m.width);
    assert_eq!(m.height, 22.0);
    assert_eq!(m.lines, 1);
    assert!(m.baseline > 8.0 && m.baseline < 20.0, "baseline {}", m.baseline);
    let s = e.shape("Hello", base(), None, 0);
    assert_eq!(s.glyphs.len(), 5);
    assert!(s.glyphs.windows(2).all(|w| w[1].x > w[0].x), "glyphs advance left to right");
}

#[test]
fn wrapping_and_clamping() {
    let mut e = TextEngine::new();
    let text = "the quick brown fox jumps over the lazy dog";
    let wide = e.measure(text, base(), None, 0);
    let narrow = e.measure(text, base(), Some(100.0), 0);
    assert_eq!(wide.lines, 1);
    assert!(narrow.lines >= 3, "lines {}", narrow.lines);
    assert!(narrow.width <= 100.0 + 0.5, "width {}", narrow.width);
    assert_eq!(narrow.height, narrow.lines as f32 * 22.0);
    let clamped = e.measure(text, base(), Some(100.0), 2);
    assert_eq!(clamped.lines, 2);
    assert_eq!(clamped.height, 44.0);
    let s = e.shape(text, base(), Some(100.0), 2);
    let shaped_full = e.shape(text, base(), Some(100.0), 0);
    assert!(s.glyphs.len() < shaped_full.glyphs.len(), "clamping drops glyphs of later lines");
}

#[test]
fn empty_and_whitespace_take_one_line() {
    let mut e = TextEngine::new();
    for t in ["", " ", "   "] {
        let m = e.measure(t, base(), Some(200.0), 0);
        assert_eq!(m.lines, 1, "{t:?}");
        assert_eq!(m.height, 22.0);
    }
    assert_eq!(e.measure("", base(), None, 0).width, 0.0);
}

#[test]
fn bold_is_wider_and_mono_is_fixed_pitch() {
    let mut e = TextEngine::new();
    let regular = e.measure("Measure", base(), None, 0).width;
    let bold = e.measure("Measure", FontSpec { weight: FontWeight::Bold, ..base() }, None, 0).width;
    assert!(bold > regular, "bold {bold} vs regular {regular}");

    let mono = FontSpec { family: FontFamily::Mono, ..base() };
    let thin = e.measure("iiiiii", mono, None, 0).width;
    let fat = e.measure("MMMMMM", mono, None, 0).width;
    assert!((thin - fat).abs() < 0.01, "mono: {thin} vs {fat}");
    let sans_thin = e.measure("iiiiii", base(), None, 0).width;
    let sans_fat = e.measure("MMMMMM", base(), None, 0).width;
    assert!(sans_fat > sans_thin * 1.5, "sans is proportional: {sans_thin} vs {sans_fat}");
}

#[test]
fn shaping_is_cached_and_deterministic() {
    let mut e = TextEngine::new();
    let a = e.shape("cache me", base(), Some(300.0), 0);
    let b = e.shape("cache me", base(), Some(300.0), 0);
    assert_eq!(e.stats().misses, 1);
    assert_eq!(e.stats().hits, 1);
    assert_eq!(*a, *b);
    // A different width is a different entry, even if the result is equal.
    let c = e.shape("cache me", base(), Some(400.0), 0);
    assert_eq!(e.stats().misses, 2);
    assert_eq!(a.metrics, c.metrics);
    // Two engines agree exactly: no system fonts are consulted.
    let mut f = TextEngine::new();
    assert_eq!(*f.shape("cache me", base(), Some(300.0), 0), *a);
}

#[test]
fn glyphs_rasterise_to_coverage_bitmaps() {
    let mut e = TextEngine::new();
    let s = e.shape("H", base(), None, 0);
    let g = s.glyphs[0];
    let img = e.rasterize(g.key, 1.0).expect("an H has an image");
    assert!(!img.color);
    assert!(img.width >= 5 && img.height >= 8, "{}×{}", img.width, img.height);
    assert_eq!(img.data.len(), (img.width * img.height) as usize);
    assert!(img.data.iter().any(|&p| p > 200), "some pixel is nearly opaque");
    let hi = e.rasterize(g.key, 2.0).unwrap();
    assert!(hi.width > img.width && hi.height > img.height, "2× is bigger");
    // A space has no image.
    let sp = e.shape(" ", base(), None, 0);
    if let Some(space) = sp.glyphs.first() {
        let img = e.rasterize(space.key, 1.0);
        assert!(img.map_or(true, |i| i.width == 0 || i.data.iter().all(|&p| p == 0)));
    }
}

#[test]
fn layout_runs_on_real_glyphs() {
    // A column of three lines of text, laid out at 200 px: heights come from
    // real line counts, and the long line wraps.
    let mut ops = vec![Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, ..Default::default() } }];
    let mut nodes = vec![FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 3 }];
    for (i, t) in ["short", "a considerably longer line of text that will need to wrap", "end"].iter().enumerate() {
        nodes.push(FlatNode { kind: NodeKind::Text, id: 2 + i as u32, style: 0, key: 0, text: Some(TextRef::Inline((*t).to_owned())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    }
    ops.push(Op::Mount(Subtree { nodes, ..Default::default() }));
    let mut s = Session::new();
    s.apply(&Batch { seq: 1, ops }).unwrap();

    let theme = Theme::default().resolve(Viewer::default());
    let mut engine = TextEngine::new();
    let mut layout = Layout::new();
    layout.compute(&mut Env { session: &s, theme: &theme, text: &mut engine }, Size::new(200.0, 600.0));

    let short = layout.rect(s.lookup(2).unwrap()).unwrap();
    let long = layout.rect(s.lookup(3).unwrap()).unwrap();
    let end = layout.rect(s.lookup(4).unwrap()).unwrap();
    assert_eq!(short.h, 22.0);
    assert!(long.h >= 44.0, "long line wrapped: {long:?}");
    assert_eq!(end.y, short.h + long.h);
    assert_eq!(long.w, 200.0);
    // Layout measured the long line under a few constraints; shaping ran
    // once per distinct one and the rest were cache hits.
    let st = engine.stats();
    assert!(st.hits > 0, "{st:?}");
}

#[test]
fn glyphs_carry_their_bytes_so_a_caret_can_be_placed() {
    let mut t = TextEngine::new();
    let shaped = t.shape("ab cd", base(), None, 0);
    let starts: Vec<usize> = shaped.glyphs.iter().map(|g| g.start).collect();
    assert_eq!(starts, vec![0, 1, 2, 3, 4]);
    assert_eq!(shaped.caret(0).0, 0.0);
    assert_eq!(shaped.caret(2).0, shaped.glyphs[2].x);
    let end = shaped.caret(5);
    assert!((end.0 - shaped.metrics.width).abs() < 0.01, "{end:?} vs {}", shaped.metrics.width);
    assert_eq!(shaped.byte_at(-5.0, 0.0), 0);
    assert_eq!(shaped.byte_at(shaped.metrics.width + 10.0, 0.0), 5);
    let g3 = shaped.glyphs[3];
    assert_eq!(shaped.byte_at(g3.x + g3.w * 0.2, 0.0), 3);
    assert_eq!(shaped.byte_at(g3.x + g3.w * 0.8, 0.0), 4);
    // Multi-byte text: offsets are bytes, ends are char boundaries.
    let shaped = t.shape("é!", base(), None, 0);
    assert_eq!(shaped.glyphs.iter().map(|g| (g.start, g.end)).collect::<Vec<_>>(), vec![(0, 2), (2, 3)]);
}

#[test]
fn symbols_come_from_the_fallback_face() {
    let mut t = TextEngine::new();
    let a = t.shape("a", base(), None, 0).metrics.width;
    for c in ["♥", "★", "✓", "→", "⟳", "↻"] {
        let w = t.shape(c, base(), None, 0).metrics.width;
        assert!(w > a * 0.4, "{c} has a glyph: width {w}");
    }
}

/// A caret in an empty field sits where the first glyph's would: the
/// empty text carries the baseline of a real line in that font.
#[test]
fn an_empty_text_has_the_baseline_of_a_real_line() {
    let mut engine = TextEngine::new();
    let font = base();
    let empty = engine.shape("", font, Some(200.0), 0);
    let some = engine.shape("a", font, Some(200.0), 0);
    assert!(empty.glyphs.is_empty());
    assert_eq!(empty.caret(0), (0.0, empty.metrics.baseline));
    assert!((empty.metrics.baseline - some.metrics.baseline).abs() < 0.01, "empty {} vs text {}", empty.metrics.baseline, some.metrics.baseline);
    assert!((empty.metrics.baseline - some.glyphs[0].y).abs() < 0.01);
}
