//! Layout goldens against the fixed-pitch measurer: 9 px per character at the
//! base size, 22 px lines. Numbers are computed by hand from spec/04.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_layout::*;
use eui_proto::*;
use eui_theme::{Theme, Viewer};
use eui_tree::Session;

// ---------------------------------------------------------------- builder

#[derive(Default)]
struct B {
    styles: Vec<StyleRecord>,
    atoms: Vec<String>,
    nodes: Vec<FlatNode>,
    props: Vec<(u32, Value)>,
    next: u32,
}

impl B {
    fn style(&mut self, r: StyleRecord) -> u32 {
        self.styles.push(r);
        self.styles.len() as u32
    }
    fn atom(&mut self, s: &str) -> u32 {
        if let Some(i) = self.atoms.iter().position(|a| a == s) {
            return i as u32 + 1;
        }
        self.atoms.push(s.to_owned());
        self.atoms.len() as u32
    }
    fn push(&mut self, kind: NodeKind, style: u32, children: u32) -> u32 {
        self.next += 1;
        let id = self.next;
        self.nodes.push(FlatNode { kind, id, style, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: children });
        id
    }
    fn text(&mut self, style: u32, s: &str) -> u32 {
        let id = self.push(NodeKind::Text, style, 0);
        self.nodes.last_mut().unwrap().text = Some(TextRef::Inline(s.to_owned()));
        id
    }
    fn prop(&mut self, name: &str, value: Value) {
        let atom = self.atom(name);
        let start = self.props.len() as u32;
        self.props.push((atom, value));
        let last = self.nodes.last_mut().unwrap();
        if last.props.1 == 0 {
            last.props = (start, 1);
        } else {
            last.props.1 += 1;
        }
    }
    fn session(self) -> Session {
        let mut ops: Vec<Op> = self.atoms.iter().enumerate().map(|(i, a)| Op::DefAtom { id: i as u32 + 1, value: a.clone() }).collect();
        ops.extend(self.styles.iter().enumerate().map(|(i, r)| Op::DefStyle { id: i as u32 + 1, record: *r }));
        ops.push(Op::Mount(Subtree { nodes: self.nodes, props: self.props, handlers: Vec::new() }));
        let mut s = Session::new();
        s.apply(&Batch { seq: 1, ops }).unwrap();
        s
    }
}

fn st() -> StyleRecord {
    StyleRecord::default()
}
fn col() -> StyleRecord {
    StyleRecord { display: Display::Column, ..st() }
}
fn row() -> StyleRecord {
    StyleRecord { display: Display::Row, ..st() }
}
fn px(n: u16) -> Dim {
    Dim::Px(n)
}

fn lay(s: &Session, w: f32, h: f32) -> (Layout, Monospace) {
    let theme = Theme::default().resolve(Viewer::default());
    let mut m = Monospace::default();
    let mut l = Layout::new();
    l.compute(&mut Env { session: s, theme: &theme, text: &mut m }, Size::new(w, h));
    (l, m)
}

fn r(l: &Layout, s: &Session, id: u32) -> Rect {
    l.rect(s.lookup(id).unwrap()).unwrap_or_else(|| panic!("node {id} was not laid out"))
}

fn assert_rect(l: &Layout, s: &Session, id: u32, x: f32, y: f32, w: f32, h: f32) {
    let got = r(l, s, id);
    let ok = |a: f32, b: f32| (a - b).abs() < 0.01;
    assert!(ok(got.x, x) && ok(got.y, y) && ok(got.w, w) && ok(got.h, h), "node {id}: got {got:?}, want ({x}, {y}, {w}, {h})");
}

// --------------------------------------------------------------- basics

#[test]
fn root_fills_the_viewport_and_a_column_stretches_its_child() {
    let mut b = B::default();
    let c = b.style(col());
    let t = b.style(st());
    b.push(NodeKind::Box, c, 1);
    let hello = b.text(t, "Hello");
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 600.0);
    assert_rect(&l, &s, 1, 0.0, 0.0, 800.0, 600.0);
    // 5 × 9 = 45 wide as content, stretched to the column's width; one line.
    assert_rect(&l, &s, hello, 0.0, 0.0, 800.0, 22.0);
    assert!((l.baseline(s.lookup(hello).unwrap()).unwrap() - 12.0).abs() < 0.01);
}

#[test]
fn row_with_padding_and_gap() {
    let mut b = B::default();
    // padding index 4 = 12 px, gap index 2 = 4 px
    let c = b.style(StyleRecord { padding: [4; 4], gap: 2, ..row() });
    let t = b.style(st());
    b.push(NodeKind::Box, c, 2);
    let a = b.text(t, "ab");
    let d = b.text(t, "cde");
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 600.0);
    assert_rect(&l, &s, a, 12.0, 12.0, 18.0, 576.0);
    assert_rect(&l, &s, d, 34.0, 12.0, 27.0, 576.0);
}

#[test]
fn grow_shares_free_space_by_weight() {
    let mut b = B::default();
    let c = b.style(row());
    let g1 = b.style(StyleRecord { grow: 1, ..st() });
    let g2 = b.style(StyleRecord { grow: 2, ..st() });
    b.push(NodeKind::Box, c, 3);
    let a = b.push(NodeKind::Box, g1, 0);
    let bb = b.push(NodeKind::Box, g1, 0);
    let cc = b.push(NodeKind::Box, g2, 0);
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 100.0);
    assert_rect(&l, &s, a, 0.0, 0.0, 200.0, 100.0);
    assert_rect(&l, &s, bb, 200.0, 0.0, 200.0, 100.0);
    assert_rect(&l, &s, cc, 400.0, 0.0, 400.0, 100.0);
}

#[test]
fn shrink_respects_min_width_and_freezes() {
    let mut b = B::default();
    let c = b.style(row());
    let w200 = b.style(StyleRecord { width: px(200), ..st() });
    let w200_min = b.style(StyleRecord { width: px(200), min_width: px(200), ..st() });
    b.push(NodeKind::Box, c, 2);
    let a = b.push(NodeKind::Box, w200, 0);
    let bb = b.push(NodeKind::Box, w200_min, 0);
    let s = b.session();
    let (l, _) = lay(&s, 300.0, 50.0);
    // 400 into 300: the second is frozen at its minimum, the first absorbs it all.
    assert_rect(&l, &s, a, 0.0, 0.0, 100.0, 50.0);
    assert_rect(&l, &s, bb, 100.0, 0.0, 200.0, 50.0);
}

#[test]
fn justify_variants() {
    for (justify, x1, x2) in [
        (Justify::Start, 0.0, 100.0),
        (Justify::Center, 300.0, 400.0),
        (Justify::End, 600.0, 700.0),
        (Justify::Between, 0.0, 700.0),
        (Justify::Around, 150.0, 550.0),
        (Justify::Evenly, 200.0, 500.0),
    ] {
        let mut b = B::default();
        let c = b.style(StyleRecord { justify, ..row() });
        let w = b.style(StyleRecord { width: px(100), ..st() });
        b.push(NodeKind::Box, c, 2);
        let a = b.push(NodeKind::Box, w, 0);
        let bb = b.push(NodeKind::Box, w, 0);
        let s = b.session();
        let (l, _) = lay(&s, 800.0, 10.0);
        assert_rect(&l, &s, a, x1, 0.0, 100.0, 10.0);
        assert_rect(&l, &s, bb, x2, 0.0, 100.0, 10.0);
    }
}

#[test]
fn align_variants_and_stretch_honours_max() {
    for (align, y, h) in [(AlignItems::Start, 0.0, 20.0), (AlignItems::Center, 40.0, 20.0), (AlignItems::End, 80.0, 20.0)] {
        let mut b = B::default();
        let c = b.style(StyleRecord { align_items: align, ..row() });
        let w = b.style(StyleRecord { width: px(50), height: px(20), ..st() });
        b.push(NodeKind::Box, c, 1);
        let a = b.push(NodeKind::Box, w, 0);
        let s = b.session();
        let (l, _) = lay(&s, 800.0, 100.0);
        assert_rect(&l, &s, a, 0.0, y, 50.0, h);
    }
    // Stretch with a max on the cross axis stops at the max.
    let mut b = B::default();
    let c = b.style(col());
    let w = b.style(StyleRecord { max_width: px(50), height: px(20), ..st() });
    b.push(NodeKind::Box, c, 1);
    let a = b.push(NodeKind::Box, w, 0);
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 100.0);
    assert_rect(&l, &s, a, 0.0, 0.0, 50.0, 20.0);
    // align_self overrides align_items.
    let mut b = B::default();
    let c = b.style(StyleRecord { align_items: AlignItems::Start, ..row() });
    let w = b.style(StyleRecord { align_self: AlignSelf::End, width: px(50), height: px(20), ..st() });
    b.push(NodeKind::Box, c, 1);
    let a = b.push(NodeKind::Box, w, 0);
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 100.0);
    assert_rect(&l, &s, a, 0.0, 80.0, 50.0, 20.0);
}

#[test]
fn wrap_breaks_lines_and_stacks_them() {
    let mut b = B::default();
    let c = b.style(StyleRecord { wrap: Wrap::Wrap, gap: 2, ..row() }); // gap 4 px
    let w = b.style(StyleRecord { width: px(100), height: px(10), ..st() });
    b.push(NodeKind::Box, c, 3);
    let a = b.push(NodeKind::Box, w, 0);
    let bb = b.push(NodeKind::Box, w, 0);
    let cc = b.push(NodeKind::Box, w, 0);
    let s = b.session();
    let (l, _) = lay(&s, 250.0, 100.0);
    assert_rect(&l, &s, a, 0.0, 0.0, 100.0, 10.0);
    assert_rect(&l, &s, bb, 104.0, 0.0, 100.0, 10.0);
    assert_rect(&l, &s, cc, 0.0, 14.0, 100.0, 10.0);
}

#[test]
fn auto_sized_containers_take_their_content() {
    let mut b = B::default();
    let root = b.style(StyleRecord { align_items: AlignItems::Start, ..col() });
    let inner = b.style(col());
    let h30 = b.style(StyleRecord { width: px(40), height: px(30), ..st() });
    let h50 = b.style(StyleRecord { width: px(70), height: px(50), ..st() });
    b.push(NodeKind::Box, root, 1);
    let i = b.push(NodeKind::Box, inner, 2);
    b.push(NodeKind::Box, h30, 0);
    b.push(NodeKind::Box, h50, 0);
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 600.0);
    // Not stretched (align start): width is the widest child, height the sum.
    assert_rect(&l, &s, i, 0.0, 0.0, 70.0, 80.0);
}

#[test]
fn percent_resolves_against_the_parent() {
    let mut b = B::default();
    let c = b.style(StyleRecord { padding: [0, 4, 0, 4], ..row() }); // 12 px each side → inner 776
    let half = b.style(StyleRecord { width: Dim::Percent(5000), ..st() });
    b.push(NodeKind::Box, c, 1);
    let a = b.push(NodeKind::Box, half, 0);
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 100.0);
    assert_rect(&l, &s, a, 12.0, 0.0, 388.0, 100.0);
}

#[test]
fn baseline_alignment_lines_up_text_of_different_sizes() {
    let mut b = B::default();
    let c = b.style(StyleRecord { align_items: AlignItems::Baseline, ..row() });
    let big = b.style(StyleRecord { font_size: 7, ..st() }); // 38 px, baseline 30.4
    let small = b.style(st()); // 15 px, baseline 12
    b.push(NodeKind::Box, c, 2);
    let a = b.text(big, "A");
    let bb = b.text(small, "b");
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 100.0);
    assert_rect(&l, &s, a, 0.0, 0.0, 22.8, 46.0);
    assert_rect(&l, &s, bb, 22.8, 18.4, 9.0, 22.0);
}

// ----------------------------------------------------------- stack, grid

#[test]
fn stack_positions_children_independently_and_orders_by_z() {
    let mut b = B::default();
    let c = b.style(StyleRecord { display: Display::Stack, justify: Justify::Center, ..st() });
    let back = b.style(StyleRecord { width: px(100), height: px(100), align_self: AlignSelf::Start, z: 5, ..st() });
    let front = b.style(StyleRecord { width: px(50), height: px(50), align_self: AlignSelf::Center, z: 0, ..st() });
    b.push(NodeKind::Box, c, 2);
    let a = b.push(NodeKind::Box, back, 0);
    let bb = b.push(NodeKind::Box, front, 0);
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 600.0);
    assert_rect(&l, &s, a, 350.0, 0.0, 100.0, 100.0);
    assert_rect(&l, &s, bb, 375.0, 275.0, 50.0, 50.0);
    // Both cover (400, 50); the higher z wins the hit even though it is first.
    assert_eq!(l.hit(&s, 400.0, 50.0), s.lookup(a));
    assert_eq!(l.hit(&s, 400.0, 300.0), s.lookup(bb));
    assert_eq!(l.hit(&s, 10.0, 590.0), s.lookup(1));
}

#[test]
fn grid_lays_out_equal_columns_row_major() {
    let mut b = B::default();
    let g = b.style(StyleRecord { display: Display::Grid, ..st() });
    let cell = b.style(StyleRecord { height: px(20), ..st() });
    b.push(NodeKind::Box, g, 5);
    b.prop("columns", Value::Int(3));
    let ids: Vec<u32> = (0..5).map(|_| b.push(NodeKind::Box, cell, 0)).collect();
    let s = b.session();
    let (l, _) = lay(&s, 900.0, 600.0);
    assert_rect(&l, &s, ids[0], 0.0, 0.0, 300.0, 20.0);
    assert_rect(&l, &s, ids[1], 300.0, 0.0, 300.0, 20.0);
    assert_rect(&l, &s, ids[2], 600.0, 0.0, 300.0, 20.0);
    assert_rect(&l, &s, ids[3], 0.0, 20.0, 300.0, 20.0);
    assert_rect(&l, &s, ids[4], 300.0, 20.0, 300.0, 20.0);
}

// ------------------------------------------------------- scroll and list

#[test]
fn scroll_clips_offsets_and_reports_content_size() {
    let mut b = B::default();
    let c = b.style(col());
    let sc = b.style(StyleRecord { height: px(100), ..col() });
    let t = b.style(st());
    b.push(NodeKind::Box, c, 1);
    let scroll = b.push(NodeKind::Scroll, sc, 10);
    let rows: Vec<u32> = (0..10).map(|i| b.text(t, &format!("row {i}"))).collect();
    let mut s = b.session();
    s.apply(&Batch { seq: 2, ops: vec![Op::ScrollTo { node: scroll, x: 0, y: 50 }] }).unwrap();
    let (l, _) = lay(&s, 800.0, 600.0);
    let six = s.lookup(scroll).unwrap();
    assert_rect(&l, &s, scroll, 0.0, 0.0, 800.0, 100.0);
    assert_eq!(l.content_size(six), Some(Size::new(800.0, 220.0)));
    assert_rect(&l, &s, rows[0], 0.0, -50.0, 800.0, 22.0);
    assert_rect(&l, &s, rows[2], 0.0, -6.0, 800.0, 22.0);
    // Hit inside the viewport reaches the row under the point …
    assert_eq!(l.hit(&s, 10.0, 10.0), s.lookup(rows[2]));
    // … but a row scrolled out of the box is not hittable through the clip.
    assert_eq!(l.hit(&s, 10.0, 150.0), s.lookup(1));
    // An offset past the end is clamped: content 220 − viewport 100 = 120.
    s.apply(&Batch { seq: 3, ops: vec![Op::ScrollTo { node: scroll, x: 0, y: 9_999 }] }).unwrap();
    let (l, _) = lay(&s, 800.0, 600.0);
    assert_rect(&l, &s, rows[9], 0.0, 78.0, 800.0, 22.0);
}

#[test]
fn a_virtualised_list_does_not_measure_what_it_cannot_see() {
    let mut b = B::default();
    let c = b.style(col());
    let ls = b.style(StyleRecord { height: px(100), ..col() });
    let t = b.style(st());
    b.push(NodeKind::Box, c, 1);
    let list = b.push(NodeKind::List, ls, 1000);
    b.prop("item_height", Value::Int(20));
    let rows: Vec<u32> = (0..1000).map(|i| b.text(t, &format!("row {i}"))).collect();
    let s = b.session();
    let (l, m) = lay(&s, 800.0, 600.0);
    // Window is [−100, 200]: about 11 rows fall in it. Each is shaped once
    // per distinct constraint it is measured under (up to three), which a
    // real text engine caches by (text, font, width); what matters here is
    // that the other 989 rows were never shaped at all.
    assert!(m.calls <= 40, "shaped {} rows", m.calls);
    // Content height: ~16 measured rows at 22 px, the rest estimated at 20 px.
    let h = l.content_size(s.lookup(list).unwrap()).unwrap().h;
    assert!((20_000.0..=20_100.0).contains(&h), "content height {h}");
    // A row outside the window is not laid out at all: no rect, nothing to
    // hit or paint. Scrollbars come from the content size, not from rects.
    assert!(l.rect(s.lookup(rows[500]).unwrap()).is_none(), "row 500 has no rect");
    // Hypothetical size, cross size, then arrange: three placements, and only
    // the last materialises rows.
    assert_eq!(l.stats().list_placements, 3);
    assert!(l.stats().rows_measured < 40, "{} rows measured", l.stats().rows_measured);
    // The visible rows were really measured: a text row is 22 px, not the estimate.
    assert_eq!(r(&l, &s, rows[0]).h, 22.0);
}

// ----------------------------------------------------------------- misc

#[test]
fn display_none_is_absent_and_absolute_children_leave_the_flow() {
    let mut b = B::default();
    let c = b.style(row());
    let hidden = b.style(StyleRecord { display: Display::None, width: px(100), ..st() });
    let abs = b.style(StyleRecord { position: Position::Absolute, width: px(30), height: px(30), margin: [2, 0, 0, 3], ..st() }); // top 4, left 8
    let w = b.style(StyleRecord { width: px(100), ..st() });
    b.push(NodeKind::Box, c, 3);
    let h = b.push(NodeKind::Box, hidden, 0);
    let a = b.push(NodeKind::Box, abs, 0);
    let flow = b.push(NodeKind::Box, w, 0);
    let s = b.session();
    let (l, _) = lay(&s, 800.0, 100.0);
    assert!(l.rect(s.lookup(h).unwrap()).is_none());
    assert_rect(&l, &s, flow, 0.0, 0.0, 100.0, 100.0);
    assert_rect(&l, &s, a, 8.0, 4.0, 30.0, 30.0);
}

#[test]
fn a_tree_at_the_depth_limit_lays_out_without_blowing_the_stack() {
    let mut b = B::default();
    let c = b.style(StyleRecord { padding: [1; 4], ..col() }); // 2 px each side
    for i in 0..255 {
        b.push(NodeKind::Box, c, u32::from(i < 254));
    }
    let s = b.session();
    let (l, _) = lay(&s, 2000.0, 2000.0);
    let innermost = r(&l, &s, 255);
    assert!((innermost.x - 508.0).abs() < 0.01, "{innermost:?}");
    assert!((innermost.w - 984.0).abs() < 0.01, "{innermost:?}");
}

#[test]
fn recompute_is_deterministic_and_memo_is_per_frame() {
    let mut b = B::default();
    let c = b.style(StyleRecord { wrap: Wrap::Wrap, gap: 3, padding: [2; 4], ..row() });
    let t = b.style(st());
    b.push(NodeKind::Box, c, 6);
    let ids: Vec<u32> = (0..6).map(|i| b.text(t, &format!("word number {i}"))).collect();
    let s = b.session();
    let (l1, _) = lay(&s, 300.0, 200.0);
    let (l2, _) = lay(&s, 300.0, 200.0);
    for id in ids {
        assert_eq!(r(&l1, &s, id), r(&l2, &s, id));
    }
    // A different viewport is a different answer, not a stale memo.
    let (l3, _) = lay(&s, 900.0, 200.0);
    assert_ne!(r(&l1, &s, 6), r(&l3, &s, 6));
}

#[test]
fn text_wraps_to_the_available_width_and_clamps() {
    let mut b = B::default();
    let c = b.style(col());
    let t = b.style(st());
    let clamped = b.style(StyleRecord { line_clamp: 1, ..st() });
    b.push(NodeKind::Box, c, 2);
    let long = b.text(t, "one two three four five"); // 23 chars = 207 px
    let one = b.text(clamped, "one two three four five");
    let s = b.session();
    let (l, _) = lay(&s, 100.0, 600.0);
    // 100 px fits 11 chars: "one two" | "three four" | "five" → 3 lines.
    assert_rect(&l, &s, long, 0.0, 0.0, 100.0, 66.0);
    assert_rect(&l, &s, one, 0.0, 66.0, 100.0, 22.0);
}
