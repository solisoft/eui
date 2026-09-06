//! The renderer, judged by its pixels. Headless: an adapter is requested
//! without a surface, so this runs on a machine with no display. Without any
//! adapter at all the GPU tests print a notice and pass vacuously; the paint
//! tests never need one.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_layout::{Env, Layout, Size};
use eui_proto::*;
use eui_render::*;
use eui_text::TextEngine;
use eui_theme::{Resolved, Role, Theme, Viewer};
use eui_tree::Session;

// ---------------------------------------------------------------- fixture

struct Fx {
    session: Session,
    layout: Layout,
    theme: Resolved,
    text: TextEngine,
    atlas: Atlas,
}

fn fixture(styles: Vec<StyleRecord>, nodes: Vec<FlatNode>, props: Vec<(u32, Value)>, atoms: &[&str], w: f32, h: f32) -> Fx {
    let mut ops: Vec<Op> = atoms.iter().enumerate().map(|(i, a)| Op::DefAtom { id: i as u32 + 1, value: (*a).to_owned() }).collect();
    ops.extend(styles.into_iter().enumerate().map(|(i, r)| Op::DefStyle { id: i as u32 + 1, record: r }));
    ops.push(Op::Mount(Subtree { nodes, props, handlers: Vec::new() }));
    let mut session = Session::new();
    session.apply(&Batch { seq: 1, ops }).unwrap();
    let theme = Theme::default().resolve(Viewer::default());
    let mut text = TextEngine::new();
    let mut layout = Layout::new();
    layout.compute(&mut Env { session: &session, theme: &theme, text: &mut text }, Size::new(w, h));
    Fx { session, layout, theme, text, atlas: Atlas::new() }
}

fn node(kind: NodeKind, id: u32, style: u32, children: u32) -> FlatNode {
    FlatNode { kind, id, style, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: children }
}

fn text(id: u32, style: u32, s: &str) -> FlatNode {
    FlatNode { kind: NodeKind::Text, id, style, key: 0, text: Some(TextRef::Inline(s.to_owned())), props: (0, 0), handlers: (0, 0), child_count: 0 }
}

fn draw(fx: &mut Fx, w: u32, h: u32, scale: f32) -> DrawList {
    paint(&mut Scene { session: &fx.session, layout: &fx.layout, theme: &fx.theme, text: &mut fx.text, atlas: &mut fx.atlas, scale, size: (w, h) })
}

fn gpu() -> Option<Renderer> {
    match Renderer::new_headless() {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("no GPU adapter ({e}); skipping pixel test");
            None
        }
    }
}

fn pixel(px: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

fn rgba(c: u32) -> [u8; 4] {
    [(c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, c as u8]
}

fn close(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| x.abs_diff(*y) <= tol)
}

// ------------------------------------------------------------- paint only

#[test]
fn paint_emits_one_quad_per_filled_box_and_one_per_glyph() {
    let bg = StyleRecord { bg: ColorRef::role(Role::AccentBase.id()), height: Dim::Px(10), ..Default::default() };
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let mut fx = fixture(vec![col, bg], vec![node(NodeKind::Box, 1, 1, 2), node(NodeKind::Box, 2, 2, 0), text(3, 0, "Hi")], vec![], &[], 200.0, 100.0);
    let list = draw(&mut fx, 200, 100, 1.0);
    // Root has no background: nothing. Box 2: one quad. "Hi": two glyphs.
    // (An empty auto-height box is 0 px tall and correctly draws nothing.)
    assert_eq!(list.quads.len(), 3, "{list:#?}");
    assert_eq!(list.quads[0].params[2] as u32, 0);
    assert_eq!(list.quads[1].params[2] as u32, TEXTURED);
    assert_eq!(list.quads[2].params[2] as u32, TEXTURED);
    assert_eq!(list.runs, vec![(0, 0, 3)]);
    assert_eq!(list.clips, vec![[0, 0, 200, 100]]);
    assert_eq!(list.clear, linear(fx.theme.color(Role::SurfaceBase)));
    assert_eq!(fx.atlas.len(), 2);
}

#[test]
fn scroll_containers_open_a_scissor_run_and_cull_what_is_outside() {
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let sc = StyleRecord { display: Display::Column, height: Dim::Px(50), ..Default::default() };
    let bg = StyleRecord { bg: ColorRef::role(Role::DangerBase.id()), height: Dim::Px(20), ..Default::default() };
    let mut nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Scroll, 2, 2, 10)];
    for i in 0..10 {
        nodes.push(node(NodeKind::Box, 10 + i, 3, 0));
    }
    let mut fx = fixture(vec![col, sc, bg], nodes, vec![], &[], 200.0, 400.0);
    let list = draw(&mut fx, 200, 400, 1.0);
    assert_eq!(list.clips.len(), 2);
    assert_eq!(list.clips[1], [0, 0, 200, 50]);
    // Only the rows overlapping the 50 px viewport survive: rows at y 0, 20, 40.
    assert_eq!(list.quads.len(), 3, "{list:#?}");
    assert!(list.runs.iter().all(|r| r.0 == 1));
}

#[test]
fn virtual_rows_paint_their_box_but_shape_no_text() {
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let ls = StyleRecord { display: Display::Column, height: Dim::Px(100), ..Default::default() };
    let mut nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::List, 2, 2, 500)];
    for i in 0..500 {
        nodes.push(text(10 + i, 0, "row of text"));
    }
    let props = vec![(1, Value::Int(20))];
    nodes[1].props = (0, 1);
    let mut fx = fixture(vec![col, ls], nodes, props, &["item_height"], 200.0, 400.0);
    let before = fx.text.stats().misses;
    let list = draw(&mut fx, 200, 400, 1.0);
    let shaped = fx.text.stats().misses - before;
    assert!(shaped <= 20, "painted {shaped} distinct runs for 500 rows");
    // ~5 visible rows × 11 glyphs, minus the space.
    assert!(list.quads.len() < 80, "{} quads", list.quads.len());
}

#[test]
fn a_two_x_display_snaps_to_device_pixels() {
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let bg = StyleRecord { bg: ColorRef::role(1), width: Dim::Px(33), height: Dim::Px(11), margin: [1, 0, 0, 1], ..Default::default() }; // 2 px margin
    let mut fx = fixture(vec![StyleRecord { align_items: AlignItems::Start, ..col }, bg], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)], vec![], &[], 100.0, 100.0);
    let list = draw(&mut fx, 200, 200, 2.0);
    assert_eq!(list.quads[0].rect, [4.0, 4.0, 66.0, 22.0]);
    assert!(list.quads[0].rect.iter().all(|v| v.fract() == 0.0));
}

// --------------------------------------------------------------- pixels

#[test]
fn clear_colour_is_the_surface_role() {
    let Some(mut r) = gpu() else { return };
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let mut fx = fixture(vec![col], vec![node(NodeKind::Box, 1, 1, 0)], vec![], &[], 64.0, 64.0);
    let list = draw(&mut fx, 64, 64, 1.0);
    let target = r.offscreen(64, 64);
    r.render_offscreen(&target, &list, &mut fx.atlas);
    let px = r.read_back(&target).unwrap();
    assert_eq!(px.len(), 64 * 64 * 4);
    let want = rgba(fx.theme.color(Role::SurfaceBase));
    assert!(close(pixel(&px, 64, 0, 0), want, 1), "{:?} vs {:?}", pixel(&px, 64, 0, 0), want);
    assert!(close(pixel(&px, 64, 63, 63), want, 1));
}

#[test]
fn a_filled_box_lands_where_layout_put_it_with_its_role_colour() {
    let Some(mut r) = gpu() else { return };
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, padding: [3; 4], ..Default::default() }; // 8 px
    let bg = StyleRecord { bg: ColorRef::role(Role::AccentBase.id()), width: Dim::Px(40), height: Dim::Px(20), ..Default::default() };
    let mut fx = fixture(vec![col, bg], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)], vec![], &[], 100.0, 100.0);
    let list = draw(&mut fx, 100, 100, 1.0);
    let target = r.offscreen(100, 100);
    r.render_offscreen(&target, &list, &mut fx.atlas);
    let px = r.read_back(&target).unwrap();
    let accent = rgba(fx.theme.color(Role::AccentBase));
    let surface = rgba(fx.theme.color(Role::SurfaceBase));
    // Inside the box: accent. Just outside every edge: surface.
    assert!(close(pixel(&px, 100, 28, 18), accent, 2), "centre {:?}", pixel(&px, 100, 28, 18));
    assert!(close(pixel(&px, 100, 9, 9), accent, 2), "top-left inside");
    assert!(close(pixel(&px, 100, 47, 27), accent, 2), "bottom-right inside");
    assert!(close(pixel(&px, 100, 7, 18), surface, 2), "left of box");
    assert!(close(pixel(&px, 100, 48, 18), surface, 2), "right of box");
    assert!(close(pixel(&px, 100, 28, 7), surface, 2), "above box");
    assert!(close(pixel(&px, 100, 28, 28), surface, 2), "below box");
}

#[test]
fn rounded_corners_and_borders_render() {
    let Some(mut r) = gpu() else { return };
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
    // radius index 3 = lg = 12 px; border 2 px in the strong border role.
    let bg = StyleRecord {
        bg: ColorRef::role(Role::AccentBase.id()),
        border_color: ColorRef::role(Role::BorderStrong.id()),
        border_width: [2; 4],
        radius: 3,
        width: Dim::Px(60),
        height: Dim::Px(60),
        ..Default::default()
    };
    let mut fx = fixture(vec![col, bg], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)], vec![], &[], 80.0, 80.0);
    let list = draw(&mut fx, 80, 80, 1.0);
    let target = r.offscreen(80, 80);
    r.render_offscreen(&target, &list, &mut fx.atlas);
    let px = r.read_back(&target).unwrap();
    let accent = rgba(fx.theme.color(Role::AccentBase));
    let surface = rgba(fx.theme.color(Role::SurfaceBase));
    let stroke = rgba(fx.theme.color(Role::BorderStrong));
    assert!(close(pixel(&px, 80, 30, 30), accent, 2), "centre is fill");
    assert!(close(pixel(&px, 80, 0, 0), surface, 2), "the corner is cut away");
    assert!(close(pixel(&px, 80, 30, 0), stroke, 8), "top edge is border: {:?}", pixel(&px, 80, 30, 0));
    assert!(close(pixel(&px, 80, 0, 30), stroke, 8), "left edge is border");
}

#[test]
fn text_puts_ink_inside_its_rect_and_nowhere_else() {
    let Some(mut r) = gpu() else { return };
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, padding: [4; 4], ..Default::default() }; // 12 px
    let big = StyleRecord { font_size: 7, ..Default::default() }; // 38 px
    let mut fx = fixture(vec![col, big], vec![node(NodeKind::Box, 1, 1, 1), text(2, 2, "HHH")], vec![], &[], 200.0, 100.0);
    let list = draw(&mut fx, 200, 100, 1.0);
    assert!(list.quads.iter().all(|q| q.params[2] as u32 == TEXTURED), "only glyph quads");
    let target = r.offscreen(200, 100);
    r.render_offscreen(&target, &list, &mut fx.atlas);
    let px = r.read_back(&target).unwrap();
    let surface = rgba(fx.theme.color(Role::SurfaceBase));
    let rect = fx.layout.rect(fx.session.lookup(2).unwrap()).unwrap();
    let mut ink_inside = 0;
    let mut ink_outside = 0;
    for y in 0..100u32 {
        for x in 0..200u32 {
            let p = pixel(&px, 200, x, y);
            let is_ink = !close(p, surface, 24);
            let inside = x as f32 >= rect.x && (x as f32) < rect.x + rect.w && y as f32 >= rect.y && (y as f32) < rect.y + rect.h;
            if is_ink && inside {
                ink_inside += 1;
            } else if is_ink {
                ink_outside += 1;
            }
        }
    }
    assert!(ink_inside > 100, "three 38 px H's leave ink: {ink_inside}");
    assert_eq!(ink_outside, 0);
    // The ink is dark: text.default on a light surface.
    let text_col = rgba(fx.theme.color(Role::TextDefault));
    let darkest = (0..100u32).flat_map(|y| (0..200u32).map(move |x| (x, y))).map(|(x, y)| pixel(&px, 200, x, y)).min_by_key(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])).unwrap();
    assert!(close(darkest, text_col, 12), "darkest {darkest:?} vs {text_col:?}");
}

#[test]
fn scroll_clips_at_the_pixel_level() {
    let Some(mut r) = gpu() else { return };
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let sc = StyleRecord { display: Display::Column, height: Dim::Px(30), ..Default::default() };
    let bg = StyleRecord { bg: ColorRef::role(Role::DangerBase.id()), height: Dim::Px(100), ..Default::default() };
    let mut fx = fixture(vec![col, sc, bg], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Scroll, 2, 2, 1), node(NodeKind::Box, 3, 3, 0)], vec![], &[], 50.0, 100.0);
    let list = draw(&mut fx, 50, 100, 1.0);
    let target = r.offscreen(50, 100);
    r.render_offscreen(&target, &list, &mut fx.atlas);
    let px = r.read_back(&target).unwrap();
    let danger = rgba(fx.theme.color(Role::DangerBase));
    let surface = rgba(fx.theme.color(Role::SurfaceBase));
    assert!(close(pixel(&px, 50, 25, 10), danger, 2), "inside the scroll box");
    assert!(close(pixel(&px, 50, 25, 29), danger, 2), "last row inside");
    assert!(close(pixel(&px, 50, 25, 30), surface, 2), "first row outside is clipped");
    assert!(close(pixel(&px, 50, 25, 90), surface, 2));
}

#[test]
fn stack_paints_in_z_order() {
    let Some(mut r) = gpu() else { return };
    let stack = StyleRecord { display: Display::Stack, ..Default::default() };
    let low = StyleRecord { bg: ColorRef::role(Role::DangerBase.id()), width: Dim::Px(40), height: Dim::Px(40), z: 0, ..Default::default() };
    let high = StyleRecord { bg: ColorRef::role(Role::SuccessBase.id()), width: Dim::Px(40), height: Dim::Px(40), z: 9, ..Default::default() };
    // The high-z child comes first in child order and must still paint on top.
    let mut fx = fixture(vec![stack, low, high], vec![node(NodeKind::Box, 1, 1, 2), node(NodeKind::Box, 2, 3, 0), node(NodeKind::Box, 3, 2, 0)], vec![], &[], 40.0, 40.0);
    let list = draw(&mut fx, 40, 40, 1.0);
    let target = r.offscreen(40, 40);
    r.render_offscreen(&target, &list, &mut fx.atlas);
    let px = r.read_back(&target).unwrap();
    assert!(close(pixel(&px, 40, 20, 20), rgba(fx.theme.color(Role::SuccessBase)), 2));
}
