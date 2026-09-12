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

fn r_(l: &Layout, s: &Session, id: u32) -> Rect {
    r(l, s, id)
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
    for (justify, x1, x2) in
        [(Justify::Start, 0.0, 100.0), (Justify::Center, 300.0, 400.0), (Justify::End, 600.0, 700.0), (Justify::Between, 0.0, 700.0), (Justify::Around, 150.0, 550.0), (Justify::Evenly, 200.0, 500.0)]
    {
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
fn an_overflowing_column_does_not_squash_text_below_its_content_size() {
    // Three 22 px lines in a 40 px column: CSS `min-height: auto` — the texts
    // keep their height and overflow the box, they are not stacked on each other.
    let mut b = B::default();
    let c = b.style(col());
    let t = b.style(st());
    b.push(NodeKind::Box, c, 3);
    let a = b.text(t, "one");
    let bb = b.text(t, "two");
    let cc = b.text(t, "three");
    let s = b.session();
    let (l, _) = lay(&s, 300.0, 40.0);
    assert_rect(&l, &s, a, 0.0, 0.0, 300.0, 22.0);
    assert_rect(&l, &s, bb, 0.0, 22.0, 300.0, 22.0);
    assert_rect(&l, &s, cc, 0.0, 44.0, 300.0, 22.0);
}

#[test]
fn a_paragraph_wraps_before_it_squeezes_its_sibling() {
    // A 40 px badge (shrink 0, as every avatar is) beside a long text in a
    // 300 px row: the text's automatic minimum is its longest word, not its
    // one-line width, so the text wraps inside the 260 px left to it.
    let mut b = B::default();
    let r = b.style(StyleRecord { display: Display::Row, gap: 0, align_items: AlignItems::Start, ..st() });
    let badge = b.style(StyleRecord { width: px(40), height: px(40), shrink: 0, ..st() });
    let grow = b.style(StyleRecord { display: Display::Column, grow: 1, ..st() });
    let t = b.style(st());
    b.push(NodeKind::Box, r, 2);
    let bd = b.push(NodeKind::Box, badge, 1);
    b.text(t, "A");
    let col = b.push(NodeKind::Box, grow, 1);
    let txt = b.text(t, "twenty two words wrap around inside this narrow column of text");
    let s = b.session();
    let (l, _) = lay(&s, 300.0, 200.0);
    assert_rect(&l, &s, bd, 0.0, 0.0, 40.0, 40.0);
    let c = r_(&l, &s, col);
    assert_eq!((c.x, c.w), (40.0, 260.0));
    let tr = r_(&l, &s, txt);
    assert!(tr.h > 22.0, "the text wrapped: {tr:?}");
}

/// §5: a popover — an absolute `overlay` in a `stack` — hangs under the
/// stack's first in-flow child, flips over it when the window has no room
/// below, and never crosses an edge.
fn popover_at(before_h: u16, window_h: f32) -> (Rect, Rect) {
    let mut b = B::default();
    let col = b.style(col());
    let stack = b.style(StyleRecord { display: Display::Stack, ..st() });
    let filler = b.style(StyleRecord { height: px(before_h), ..st() });
    let box_h = b.style(StyleRecord { height: px(32), ..st() });
    let panel = b.style(StyleRecord {
        position: Position::Absolute,
        width: px(160),
        height: px(84),
        // The scale's index 2 is four pixels: the gap it keeps.
        margin: [2, 0, 0, 0],
        ..st()
    });
    b.push(NodeKind::Box, col, 2);
    b.push(NodeKind::Box, filler, 0);
    b.push(NodeKind::Box, stack, 2);
    let control = b.push(NodeKind::Box, box_h, 0);
    let list = b.push(NodeKind::Overlay, panel, 0);
    let s = b.session();
    let (l, _) = lay(&s, 400.0, window_h);
    (r(&l, &s, control), r(&l, &s, list))
}

#[test]
fn a_popover_hangs_under_its_anchor_and_flips_when_the_window_is_short() {
    // Room below: under the control, with the margin between them.
    let (control, list) = popover_at(20, 400.0);
    assert!((list.y - (control.y + control.h + 4.0)).abs() < 0.01, "under: {control:?} {list:?}");
    assert!((list.x - control.x).abs() < 0.01, "left edges line up");
    // Near the floor, with more room above than below: over it instead,
    // the same distance away.
    let (control, list) = popover_at(100, 200.0);
    assert!((list.y + list.h + 4.0 - control.y).abs() < 0.01, "over: {control:?} {list:?}");
    assert!(list.y >= 0.0, "and inside the window");
}

/// §5: a popover is measured against the window, not against the box it
/// hangs off — so a `scroll` of sixty options in one is as tall as the
/// window at most, and the options past that scroll inside the panel
/// instead of sitting below the bottom edge where nothing can reach them.
#[test]
fn a_popover_taller_than_the_window_fits_inside_it_and_scrolls() {
    let mut b = B::default();
    let column = b.style(col());
    let stack = b.style(StyleRecord { display: Display::Stack, ..st() });
    let box_h = b.style(StyleRecord { height: px(32), ..st() });
    let panel = b.style(StyleRecord { position: Position::Absolute, margin: [2, 0, 0, 0], ..st() });
    let list = b.style(col());
    let t = b.style(st());
    b.push(NodeKind::Box, column, 1);
    b.push(NodeKind::Box, stack, 2);
    let control = b.push(NodeKind::Box, box_h, 0);
    let over = b.push(NodeKind::Overlay, panel, 1);
    let options = b.push(NodeKind::Scroll, list, 20);
    for _ in 0..20 {
        b.text(t, "option");
    }
    let s = b.session();
    // Twenty 22 px options are 440 px of content in a 300 px window.
    let (l, _) = lay(&s, 400.0, 300.0);
    let control = r(&l, &s, control);
    let over = r(&l, &s, over);
    let options = s.lookup(options).unwrap();
    assert_eq!(l.content_size(options), Some(Size::new(54.0, 440.0)), "the options are all there to scroll to");
    // The window less the 4 px gap the panel keeps from its anchor: the
    // room the viewer has, which is what the scroller takes. Measured
    // against the stack instead, the panel was its anchor's 32 px.
    assert!((over.h - 296.0).abs() < 0.01, "the panel takes the window's room: {over:?}");
    assert!(over.y >= 0.0 && over.y + over.h <= 300.01, "and sits inside it: {over:?}");
    assert!(control.h < over.h, "not the height of the control it hangs off: {control:?}");
    assert!((r(&l, &s, options.raw()).h - 296.0).abs() < 0.01, "the scroller is the panel's height, not its content's");
}

#[test]
fn a_stack_stretches_auto_sized_children_on_both_axes() {
    // A page column layered under a sheet fills the window, not its content
    // width. §5: only an in-flow child stretches — an absolute one takes
    // its content size on the axis it does not fix, so a popover is as
    // tall as its options and not as tall as the box it hangs off.
    let mut b = B::default();
    let stack = b.style(StyleRecord { display: Display::Stack, ..st() });
    let t = b.style(st());
    let layer = b.style(StyleRecord { position: Position::Absolute, width: px(100), ..st() });
    b.push(NodeKind::Box, stack, 2);
    let page = b.push(NodeKind::Box, t, 1);
    b.text(t, "hi");
    let sheet = b.push(NodeKind::Box, layer, 1);
    b.text(t, "hi");
    let s = b.session();
    let (l, _) = lay(&s, 400.0, 300.0);
    assert_rect(&l, &s, page, 0.0, 0.0, 400.0, 300.0);
    assert_rect(&l, &s, sheet, 0.0, 0.0, 100.0, 22.0);
}

#[test]
fn a_scroll_child_still_shrinks_to_fit_its_column() {
    // A `scroll` has no automatic minimum: with a 22 px sibling in a 100 px
    // column it takes the remaining 78 px and scrolls the rest.
    let mut b = B::default();
    let c = b.style(col());
    let t = b.style(st());
    b.push(NodeKind::Box, c, 2);
    let head = b.text(t, "head");
    let scroll = b.push(NodeKind::Scroll, c, 10);
    let rows: Vec<u32> = (0..10).map(|i| b.text(t, &format!("row {i}"))).collect();
    let s = b.session();
    let (l, _) = lay(&s, 300.0, 100.0);
    assert_rect(&l, &s, head, 0.0, 0.0, 300.0, 22.0);
    assert_rect(&l, &s, scroll, 0.0, 22.0, 300.0, 78.0);
    assert_rect(&l, &s, rows[0], 0.0, 22.0, 300.0, 22.0);
    assert_eq!(l.content_size(s.lookup(scroll).unwrap()), Some(Size::new(300.0, 220.0)));
}

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

/// §7 again, across frames: the tops of a list's rows are the rows' own
/// heights added up, so a scroll — which moves the window and nothing else
/// — reuses them, and a change to a row does not.
#[test]
fn a_scroll_keeps_the_row_tops_it_already_added_up() {
    let mut b = B::default();
    let c = b.style(col());
    let ls = b.style(StyleRecord { height: px(100), ..col() });
    let t = b.style(st());
    b.push(NodeKind::Box, c, 1);
    let list_id = b.push(NodeKind::List, ls, 1000);
    b.prop("item_height", Value::Int(20));
    let rows: Vec<u32> = (0..1000).map(|i| b.text(t, &format!("row {i}"))).collect();
    let mut s = b.session();
    let theme = Theme::default().resolve(Viewer::default());
    let mut m = Monospace::default();
    let mut l = Layout::new();
    let mut frame = |l: &mut Layout, s: &Session| l.compute(&mut Env { session: s, theme: &theme, text: &mut m }, Size::new(800.0, 600.0));

    frame(&mut l, &s);
    // Measure and arrange each ask once; both are served by one addition.
    assert_eq!(l.stats().rows_added_up, 1, "the first frame adds them up");
    let list = s.lookup(list_id).unwrap();
    assert_eq!(l.row_tops(list).map(<[f32]>::len), Some(1001));

    // A scroll: the window moves, the tops do not.
    s.apply(&Batch { seq: 2, ops: vec![Op::ScrollTo { node: list_id, x: 0, y: 4_000 }] }).unwrap();
    s.clear_all_dirty();
    frame(&mut l, &s);
    assert_eq!(l.stats().rows_added_up, 0, "a scrolled frame adds nothing up");
    assert_rect(&l, &s, rows[200], 0.0, 0.0, 800.0, 22.0);

    // A row that grows is a change: the tops below it move.
    s.apply(&Batch { seq: 3, ops: vec![Op::SetProp { node: rows[100], prop: 1, value: Value::Int(60) }] }).unwrap();
    frame(&mut l, &s);
    assert_eq!(l.stats().rows_added_up, 1, "a taller row is added up again");
    assert_rect(&l, &s, rows[200], 0.0, 40.0, 800.0, 22.0);
}

/// §7: a scroller mid-glide is laid out once, at the landing. A
/// virtualised list then holds the rows at both ends of the travel, so the
/// renderer has rows to slide past; and a hit asks where the content is
/// drawn, not where it was put.
#[test]
fn a_glide_widens_the_virtual_window_and_moves_the_hit() {
    let mut b = B::default();
    let c = b.style(col());
    let ls = b.style(StyleRecord { height: px(100), ..col() });
    let t = b.style(st());
    b.push(NodeKind::Box, c, 1);
    let list_id = b.push(NodeKind::List, ls, 1000);
    b.prop("item_height", Value::Int(20));
    let rows: Vec<u32> = (0..1000).map(|i| b.text(t, &format!("row {i}"))).collect();
    let mut s = b.session();
    let theme = Theme::default().resolve(Viewer::default());
    let mut m = Monospace::default();
    let mut l = Layout::new();
    let mut frame = |l: &mut Layout, s: &Session| l.compute(&mut Env { session: s, theme: &theme, text: &mut m }, Size::new(800.0, 600.0));
    let list = s.lookup(list_id).unwrap();

    // Landing at 4 000, gliding from 3 800: rows around both are placed.
    s.apply(&Batch { seq: 2, ops: vec![Op::ScrollTo { node: list_id, x: 0, y: 4_000 }] }).unwrap();
    l.set_glide(list, 3_800.0, 4_000.0, (0.0, 200.0));
    frame(&mut l, &s);
    assert!(l.rect(s.lookup(rows[186]).unwrap()).is_some(), "a row a viewport above the start of the travel");
    assert!(l.rect(s.lookup(rows[200]).unwrap()).is_some(), "the landing row");
    assert!(l.rect(s.lookup(rows[190]).unwrap()).is_some(), "a row on the way");
    assert!(l.rect(s.lookup(rows[150]).unwrap()).is_none(), "not the whole list");
    // The content is drawn 200 px below where it was put: the point 10 px
    // into the view is over the row put at −190, row 190 -- not row 200.
    assert_eq!(l.hit(&s, 10.0, 10.0), s.lookup(rows[190]));
    l.set_glide_delta(list, (0.0, 100.0));
    assert_eq!(l.hit(&s, 10.0, 10.0), s.lookup(rows[195]));
    l.clear_glide(list);
    assert_eq!(l.hit(&s, 10.0, 10.0), s.lookup(rows[200]), "landed: what the layout put there");
    // Without the glide the same layout holds only the rows around 4 000.
    s.clear_all_dirty();
    s.apply(&Batch { seq: 3, ops: vec![Op::ScrollTo { node: list_id, x: 0, y: 4_000 }] }).unwrap();
    frame(&mut l, &s);
    assert!(l.rect(s.lookup(rows[186]).unwrap()).is_none(), "a viewport above the start of the travel is out of the window at rest");
}

/// §7: an offset is not a size. A scrolled frame keeps every measure --
/// the scroller's and its ancestors' too -- and measures only what enters
/// the window; on a page laid out in full, nothing at all.
#[test]
fn a_scroll_keeps_the_measures_of_what_did_not_move() {
    let mut b = B::default();
    let c = b.style(col());
    let sc = b.style(StyleRecord { height: px(100), ..col() });
    let t = b.style(st());
    b.push(NodeKind::Box, c, 1);
    let scroll = b.push(NodeKind::Scroll, sc, 30);
    for i in 0..30 {
        b.text(t, &format!("row {i}"));
    }
    let mut s = b.session();
    let theme = Theme::default().resolve(Viewer::default());
    let mut m = Monospace::default();
    let mut l = Layout::new();
    let mut frame = |l: &mut Layout, s: &Session| l.compute(&mut Env { session: s, theme: &theme, text: &mut m }, Size::new(800.0, 600.0));
    frame(&mut l, &s);
    assert!(l.stats().measures > 30, "the first frame measures everything: {}", l.stats().measures);
    s.clear_all_dirty();
    s.apply(&Batch { seq: 2, ops: vec![Op::ScrollTo { node: scroll, x: 0, y: 40 }] }).unwrap();
    let root = s.lookup(1).unwrap();
    assert_eq!(s.node(root).unwrap().dirty, eui_tree::dirty::BELOW_UNMEASURED, "the root is told something below scrolled, and no more");
    frame(&mut l, &s);
    assert_eq!(l.stats().measures, 0, "a scroll measures nothing");
    assert!(l.stats().memo_hits >= 3, "the page, the scroller and its rows are what they were: {} hits", l.stats().memo_hits);
    assert_eq!(r(&l, &s, 3).y, -40.0, "and the rows moved");
}

// ----------------------------------------------------------------- misc

#[test]
fn a_percent_width_is_capped_by_max_width_and_then_aligned() {
    // A picture at `width: 100%`, `max_width: 480` in a 600 px column is 480
    // wide; centred when the column says so.
    let mut b = B::default();
    let centre = b.style(StyleRecord { display: Display::Column, align_items: AlignItems::Center, ..st() });
    let pic = b.style(StyleRecord { width: Dim::Percent(10_000), max_width: px(480), height: px(50), ..st() });
    b.push(NodeKind::Box, centre, 1);
    let p = b.push(NodeKind::Box, pic, 0);
    let s = b.session();
    let (l, _) = lay(&s, 600.0, 200.0);
    assert_rect(&l, &s, p, 60.0, 0.0, 480.0, 50.0);
}

#[test]
fn a_virtualised_list_honours_a_rows_own_height() {
    // Ten rows of 20 px, every third one 60 px tall by its own prop.
    let mut b = B::default();
    let c = b.style(col());
    let ls = b.style(StyleRecord { display: Display::Column, height: Dim::Px(100), ..st() });
    let t = b.style(st());
    b.push(NodeKind::Box, c, 1);
    let list = b.push(NodeKind::List, ls, 10);
    b.prop("item_height", Value::Int(20));
    let mut rows = Vec::new();
    for i in 0..10 {
        rows.push(b.text(t, &format!("row {i}")));
        if i % 3 == 0 {
            b.prop("item_height", Value::Int(60));
        }
    }
    let s = b.session();
    let (l, _) = lay(&s, 300.0, 400.0);
    // Tops: 0, 60, 80, 100, 160, 180, 200, 260, 280, 300; content 360.
    assert_rect(&l, &s, rows[0], 0.0, 0.0, 300.0, 22.0);
    assert_rect(&l, &s, rows[1], 0.0, 60.0, 300.0, 22.0);
    assert_rect(&l, &s, rows[3], 0.0, 100.0, 300.0, 22.0);
    assert_eq!(l.content_size(s.lookup(list).unwrap()), Some(Size::new(300.0, 360.0)));
    // Rows past one viewport of margin are not laid out: row 9 at 300 is beyond 200.
    assert!(l.rect(s.lookup(rows[9]).unwrap()).is_none());
    assert!(l.rect(s.lookup(rows[4]).unwrap()).is_some());
}

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
    // Layout recurses per level. Measured for depth 255: under 512 KiB in
    // release, ~2.3 MiB unoptimised — more than the 2 MiB a test thread gets.
    // The thread below has the smallest main-thread stack among targets
    // (1 MiB, Windows) in release, so the guarantee is what is tested.
    let run = || {
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
    };
    let stack = if cfg!(debug_assertions) { 8 << 20 } else { 1 << 20 };
    std::thread::Builder::new().stack_size(stack).spawn(run).unwrap().join().unwrap();
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

// ------------------------------------------------------- windowed lists

/// §7.1: a list of 1 000 rows the tree does not hold, heights 20 except
/// every third row at 50, holding only rows 4, 5 and 998.
#[test]
fn a_windowed_list_places_its_rows_by_index_and_sizes_the_rest_from_heights() {
    let mut b = B::default();
    let c = b.style(col());
    let ls = b.style(StyleRecord { height: px(100), ..col() });
    let t = b.style(StyleRecord { height: px(10), ..st() });
    b.push(NodeKind::Box, c, 1);
    let list = b.push(NodeKind::List, ls, 3);
    b.prop("item_height", Value::Int(20));
    b.prop("count", Value::Int(1000));
    b.prop("heights", Value::List((0..1000).map(|i| Value::Int(if i % 3 == 0 { 50 } else { 20 })).collect()));
    let mut ids = Vec::new();
    for row in [4i64, 5, 998] {
        ids.push(b.push(NodeKind::Box, t, 0));
        b.prop("row", Value::Int(row));
    }
    let s = b.session();
    let (l, _) = lay(&s, 300.0, 100.0);
    let list = s.lookup(list).unwrap();
    // Tops: 0, 50, 70, 90, 140, 160, 180, 230, …; 334 tall rows and 666 short: 30 020.
    let tops = l.row_tops(list).unwrap();
    assert_eq!(&tops[..6], &[0.0, 50.0, 70.0, 90.0, 140.0, 160.0]);
    assert_eq!(tops.len(), 1001);
    assert_eq!(l.content_size(list).unwrap().h, 30_020.0);
    // Rows 4 and 5 are in the window and placed at their tops; 998 is not.
    assert_eq!(r(&l, &s, ids[0]).y, 140.0);
    assert_eq!(r(&l, &s, ids[1]).y, 160.0);
    assert!(l.rect(s.lookup(ids[2]).unwrap()).is_none());
    assert_eq!(l.windowed_lists(), &[list]);
    assert_eq!(l.placed_rows(list), Some(&[4u32, 5][..]), "the rows that had a child, in the window");
    // The window at the top: rows within two viewports of margin.
    assert_eq!(l.row_window(list, 0.0), Some((0, 9)));
    // Scrolled to 15 000 px: the rows around it.
    let (a, z) = l.row_window(list, 15_000.0).unwrap();
    assert!(tops[a as usize] <= 14_800.0 && tops[(a + 1) as usize] > 14_800.0, "first {a}");
    assert!(tops[z as usize] < 15_300.0 && tops[(z + 1) as usize] >= 15_300.0, "last {z}");
}

// ------------------------------------------------- sizing a code viewer

/// A `max_height` on a scroll is a ceiling, not a height: content shorter
/// than it leaves the box at the content's size. Reading the ceiling as a
/// target is what put a field of empty background under a code viewer's
/// last line, with no gutter beside it.
#[test]
fn a_scroll_shrinks_to_its_content_under_its_ceiling() {
    let mut b = B::default();
    let outer = b.style(StyleRecord { max_height: px(300), ..col() });
    let line = b.style(col());
    let page = b.style(StyleRecord { height: px(600), ..col() });
    b.push(NodeKind::Box, page, 1);
    let sc = b.push(NodeKind::Scroll, outer, 1);
    let inner = b.text(line, "one\ntwo");
    let s = b.session();
    let (l, _) = lay(&s, 400.0, 600.0);
    // Two lines at 22 px is 44, and the ceiling is not reached.
    assert_eq!(r(&l, &s, inner).h, 44.0);
    assert_eq!(r(&l, &s, sc).h, 44.0, "the ceiling is a limit, not a size");
}

/// The ceiling still binds when the content is taller than it — that is
/// what makes it a viewport at all — and an `Exact` height still wins,
/// because a scroll told what size to be is that size.
#[test]
fn a_scroll_stops_at_its_ceiling_and_obeys_a_height() {
    for (h, max, want) in [(None, Some(120u16), 120.0f32), (Some(200u16), None, 200.0), (Some(80u16), Some(300u16), 80.0)] {
        let mut b = B::default();
        let outer = b.style(StyleRecord { height: h.map_or(Dim::Auto, px), max_height: max.map_or(Dim::Auto, px), ..col() });
        let line = b.style(col());
        let page = b.style(StyleRecord { height: px(600), ..col() });
        b.push(NodeKind::Box, page, 1);
        let sc = b.push(NodeKind::Scroll, outer, 1);
        // Twenty lines at 22 px is 440, taller than any ceiling here.
        b.text(line, &vec!["x"; 20].join("\n"));
        let s = b.session();
        let (l, _) = lay(&s, 400.0, 600.0);
        assert_eq!(r(&l, &s, sc).h, want, "height {h:?}, max {max:?}");
    }
}

/// A plain `box` in a column sizes to its content, which is what a viewer
/// that shows a whole file wants: it ends just below the last line, with no
/// strip of background under it. `code_viewer` uses one for exactly this,
/// and keeps the scroll for the case where a caller does name a ceiling.
#[test]
fn a_box_ends_at_its_last_line() {
    let mut b = B::default();
    let frame = b.style(StyleRecord { overflow: Overflow::Clip, ..col() });
    let line = b.style(col());
    let page = b.style(StyleRecord { height: px(600), ..col() });
    b.push(NodeKind::Box, page, 1);
    let fr = b.push(NodeKind::Box, frame, 1);
    let inner = b.text(line, "one\ntwo");
    let s = b.session();
    let (l, _) = lay(&s, 400.0, 600.0);
    assert_eq!(r(&l, &s, inner).h, 44.0);
    assert_eq!(r(&l, &s, fr).h, 44.0, "the frame is exactly its content, in a column with room to spare");
}

/// §5: a `position: pointer` panel is placed against the hand rather than
/// against the box it hangs off — above the cursor, centred on it — and it
/// keeps up with the hand without a second layout.
#[test]
fn a_panel_that_follows_the_pointer_sits_above_it_and_keeps_up() {
    let mut b = B::default();
    let column = b.style(col());
    let stack = b.style(StyleRecord { display: Display::Stack, ..st() });
    let box_h = b.style(StyleRecord { height: px(200), ..st() });
    let chip = b.style(StyleRecord { position: Position::Pointer, margin: [2, 0, 0, 0], ..st() });
    let t = b.style(st());
    b.push(NodeKind::Box, column, 1);
    b.push(NodeKind::Box, stack, 2);
    let plot = b.push(NodeKind::Box, box_h, 0);
    let tip = b.push(NodeKind::Overlay, chip, 1);
    b.text(t, "nine");
    let s = b.session();

    let theme = Theme::default().resolve(Viewer::default());
    let mut m = Monospace::default();
    let mut l = Layout::new();
    l.set_pointer(Some((180.0, 120.0)));
    l.compute(&mut Env { session: &s, theme: &theme, text: &mut m }, Size::new(400.0, 300.0));
    let plot = r(&l, &s, plot);
    let at = r(&l, &s, tip);
    // Above the pointer by the panel's own top margin (4 px), centred on it:
    // anchored to the plot instead, it sat at the plot's bottom-left corner
    // wherever in the plot the pointer actually was.
    assert!((at.y + at.h + 4.0 - 120.0).abs() < 0.01, "its bottom is 4 px over the cursor: {at:?}");
    assert!((at.x + at.w / 2.0 - 180.0).abs() < 0.01, "and its centre is on the cursor: {at:?}");
    assert!(at.y < plot.y + plot.h, "not parked at the foot of the plot: {at:?} vs {plot:?}");

    // Moving the hand moves the chip, and moves nothing else.
    assert!(l.track_pointer(&s, 60.0, 240.0), "the panel moved");
    let then = r(&l, &s, tip);
    assert!((then.y + then.h + 4.0 - 240.0).abs() < 0.01, "it followed: {then:?}");
    assert!((then.x + then.w / 2.0 - 60.0).abs() < 0.01, "on both axes: {then:?}");
    assert_eq!(r(&l, &s, 3), plot, "and the tree under it did not move");
    assert!(!l.track_pointer(&s, 60.0, 240.0), "and standing still costs no repaint");

    // No room above: it drops under the cursor rather than off the window.
    l.track_pointer(&s, 200.0, 2.0);
    let low = r(&l, &s, tip);
    assert!(low.y >= 2.0, "under the cursor when there is no room over it: {low:?}");
}

/// §5: a panel placed at the pointer sits under the hand by construction, so
/// asking it first would put it between the pointer and the thing it is
/// describing — a tooltip would answer the hover that shows it, and flicker.
/// It is painted and never pointed at.
#[test]
fn a_panel_at_the_pointer_is_painted_but_never_pointed_at() {
    let mut b = B::default();
    let column = b.style(col());
    let stack = b.style(StyleRecord { display: Display::Stack, ..st() });
    let plot = b.style(StyleRecord { height: px(200), ..st() });
    let chip = b.style(StyleRecord { position: Position::Pointer, margin: [2, 0, 0, 0], ..st() });
    let t = b.style(st());
    b.push(NodeKind::Box, column, 1);
    b.push(NodeKind::Box, stack, 2);
    let under = b.push(NodeKind::Box, plot, 0);
    let tip = b.push(NodeKind::Overlay, chip, 1);
    b.text(t, "nine");
    let s = b.session();

    let theme = Theme::default().resolve(Viewer::default());
    let mut m = Monospace::default();
    let mut l = Layout::new();
    l.set_pointer(Some((120.0, 140.0)));
    l.compute(&mut Env { session: &s, theme: &theme, text: &mut m }, Size::new(400.0, 300.0));

    let at = r(&l, &s, tip);
    assert!(at.w > 0.0 && at.h > 0.0, "the chip is laid out and painted: {at:?}");
    // The point the chip covers still finds what is under it.
    let inside = (at.x + at.w / 2.0, at.y + at.h / 2.0);
    let hit = l.hit(&s, inside.0, inside.1).expect("something is under the chip");
    assert_ne!(hit, s.lookup(tip).unwrap(), "the chip did not answer");
    assert_eq!(hit, s.lookup(under).unwrap(), "the plot it describes did");
}

/// §6.2: a drag asks what is under the hand while ignoring what is *in* it.
/// Without the exclusion the deepest node under the pointer is always the
/// thing being carried, and a folder could be dropped into itself.
#[test]
fn a_drag_does_not_find_a_target_inside_what_it_is_carrying() {
    let mut b = B::default();
    let column = b.style(col());
    let row_st = b.style(StyleRecord { height: px(40), ..st() });
    let inner = b.style(StyleRecord { height: px(20), width: px(120), ..st() });
    b.push(NodeKind::Box, column, 2);
    let carried = b.push(NodeKind::Box, row_st, 1);
    let within = b.push(NodeKind::Box, inner, 0);
    let other = b.push(NodeKind::Box, row_st, 0);
    let s = b.session();
    let (l, _) = lay(&s, 400.0, 300.0);

    let (carried, within, other) = (s.lookup(carried).unwrap(), s.lookup(within).unwrap(), s.lookup(other).unwrap());
    let at = r_(&l, &s, 3);
    let (x, y) = (at.x + at.w / 2.0, at.y + at.h / 2.0);
    assert_eq!(l.hit(&s, x, y), Some(within), "unfiltered, the deepest node wins");
    let through = l.hit_skipping(&s, x, y, Some(carried)).expect("the point still lands on something");
    assert_ne!(through, within, "the whole subtree goes, not just its root");
    assert_ne!(through, carried, "and the root of it with them");

    // What is outside the carried subtree is still found.
    let below = r_(&l, &s, 4);
    assert_eq!(l.hit_skipping(&s, below.x + 1.0, below.y + below.h / 2.0, Some(carried)), Some(other));
}
