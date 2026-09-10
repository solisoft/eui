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
    images: ImageAtlas,
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
    Fx { session, layout, theme, text, atlas: Atlas::new(), images: ImageAtlas::new() }
}

fn node(kind: NodeKind, id: u32, style: u32, children: u32) -> FlatNode {
    FlatNode { kind, id, style, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: children }
}

fn text(id: u32, style: u32, s: &str) -> FlatNode {
    FlatNode { kind: NodeKind::Text, id, style, key: 0, text: Some(TextRef::Inline(s.to_owned())), props: (0, 0), handlers: (0, 0), child_count: 0 }
}

fn draw(fx: &mut Fx, w: u32, h: u32, scale: f32) -> DrawList {
    paint(&mut Scene {
        session: &fx.session,
        layout: &fx.layout,
        theme: &fx.theme,
        text: &mut fx.text,
        atlas: &mut fx.atlas,
        images: &fx.images,
        scale,
        size: (w, h),
        focus: None,
        anims: &[],
        glides: &[],
        cache: &mut PaintCache::new(),
        editing: None,
        now: 0.0,
        scrollbar_hot: None,
    })
}

/// One device for every pixel test in this file, taken in turn.
///
/// A `Renderer` is a GPU device, and twenty of them at once -- one per
/// test, on the threads the harness runs them from -- is a great deal to
/// ask of a software adapter: the Windows runner answered by taking the
/// test process down with an access violation, about one run in four.
/// Production has one device per process, so the tests may too, and the
/// lock makes them queue for it rather than race.
fn gpu() -> Option<std::sync::MutexGuard<'static, Renderer>> {
    static SHARED: std::sync::OnceLock<Option<std::sync::Mutex<Renderer>>> = std::sync::OnceLock::new();
    let shared = SHARED.get_or_init(|| match Renderer::new_headless() {
        Ok(r) => Some(std::sync::Mutex::new(r)),
        Err(e) => {
            eprintln!("no GPU adapter ({e}); skipping the pixel tests");
            None
        }
    });
    // A test that fails while holding it poisons the lock; the next one
    // still wants the device, and a fresh session's textures are its own.
    shared.as_ref().map(|m| m.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
}

fn pixel(px: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2], px[i + 3]]
}

fn rgba_of(c: u32) -> [u8; 4] {
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
    assert_eq!(list.runs, vec![Run { clip: 0, chain: 0, first: 0, count: 3 }]);
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
    // Only the rows overlapping the 50 px viewport survive: rows at y 0, 20,
    // 40 — plus the scrollbar thumb, painted last, outside the scissor run.
    assert_eq!(list.quads.len(), 4, "{list:#?}");
    let thumb = list.quads.last().unwrap();
    assert_eq!(thumb.rect[2], SCROLLBAR_WIDTH - 2.0);
    assert_eq!(thumb.rect[3], 24.0, "200 px of content in 50 px: the 24 px minimum");
    assert!(list.runs.iter().take(list.runs.len() - 1).all(|r| r.clip == 1));
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
    let mut st = r.session();
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let mut fx = fixture(vec![col], vec![node(NodeKind::Box, 1, 1, 0)], vec![], &[], 64.0, 64.0);
    let list = draw(&mut fx, 64, 64, 1.0);
    let target = r.offscreen(64, 64);
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    assert_eq!(px.len(), 64 * 64 * 4);
    let want = rgba_of(fx.theme.color(Role::SurfaceBase));
    assert!(close(pixel(&px, 64, 0, 0), want, 1), "{:?} vs {:?}", pixel(&px, 64, 0, 0), want);
    assert!(close(pixel(&px, 64, 63, 63), want, 1));
}

#[test]
fn a_filled_box_lands_where_layout_put_it_with_its_role_colour() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, padding: [3; 4], ..Default::default() }; // 8 px
    let bg = StyleRecord { bg: ColorRef::role(Role::AccentBase.id()), width: Dim::Px(40), height: Dim::Px(20), ..Default::default() };
    let mut fx = fixture(vec![col, bg], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)], vec![], &[], 100.0, 100.0);
    let list = draw(&mut fx, 100, 100, 1.0);
    let target = r.offscreen(100, 100);
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    let accent = rgba_of(fx.theme.color(Role::AccentBase));
    let surface = rgba_of(fx.theme.color(Role::SurfaceBase));
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
    let mut st = r.session();
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
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    let accent = rgba_of(fx.theme.color(Role::AccentBase));
    let surface = rgba_of(fx.theme.color(Role::SurfaceBase));
    let stroke = rgba_of(fx.theme.color(Role::BorderStrong));
    assert!(close(pixel(&px, 80, 30, 30), accent, 2), "centre is fill");
    assert!(close(pixel(&px, 80, 0, 0), surface, 2), "the corner is cut away");
    assert!(close(pixel(&px, 80, 30, 0), stroke, 8), "top edge is border: {:?}", pixel(&px, 80, 30, 0));
    assert!(close(pixel(&px, 80, 0, 30), stroke, 8), "left edge is border");
}

#[test]
fn text_puts_ink_inside_its_rect_and_nowhere_else() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, padding: [4; 4], ..Default::default() }; // 12 px
    let big = StyleRecord { font_size: 7, ..Default::default() }; // 38 px
    let mut fx = fixture(vec![col, big], vec![node(NodeKind::Box, 1, 1, 1), text(2, 2, "HHH")], vec![], &[], 200.0, 100.0);
    let list = draw(&mut fx, 200, 100, 1.0);
    assert!(list.quads.iter().all(|q| q.params[2] as u32 == TEXTURED), "only glyph quads");
    let target = r.offscreen(200, 100);
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    let surface = rgba_of(fx.theme.color(Role::SurfaceBase));
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
    let text_col = rgba_of(fx.theme.color(Role::TextDefault));
    let darkest = (0..100u32).flat_map(|y| (0..200u32).map(move |x| (x, y))).map(|(x, y)| pixel(&px, 200, x, y)).min_by_key(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])).unwrap();
    assert!(close(darkest, text_col, 12), "darkest {darkest:?} vs {text_col:?}");
}

#[test]
fn an_icon_puts_ink_in_its_own_colour_inside_its_own_rect() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, padding: [4; 4], ..Default::default() };
    // `fg` on the icon, not inherited, so the test pins the colour it takes.
    let ic = StyleRecord { width: Dim::Px(40), height: Dim::Px(40), fg: ColorRef::role(Role::DangerBase.id()), ..Default::default() };
    let mut nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Icon, 2, 2, 0)];
    nodes[1].props = (0, 1);
    let mut fx = fixture(vec![col, ic], nodes, vec![(1, Value::Str("close".into()))], &["name"], 100.0, 100.0);
    let list = draw(&mut fx, 100, 100, 1.0);
    assert!(!list.quads.is_empty(), "the icon drew something");
    let target = r.offscreen(100, 100);
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    let surface = rgba_of(fx.theme.color(Role::SurfaceBase));
    let rect = fx.layout.rect(fx.session.lookup(2).unwrap()).unwrap();
    let (mut inside, mut outside) = (0, 0);
    for y in 0..100u32 {
        for x in 0..100u32 {
            if close(pixel(&px, 100, x, y), surface, 24) {
                continue;
            }
            let in_rect = x as f32 >= rect.x && (x as f32) < rect.x + rect.w && y as f32 >= rect.y && (y as f32) < rect.y + rect.h;
            if in_rect {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert!(inside > 60, "a 40 px cross leaves ink: {inside}");
    assert_eq!(outside, 0, "and none of it outside the icon's rect");
    // The stroke is the icon's own `fg`, not the text colour it would have
    // inherited as a glyph.
    let want = rgba_of(fx.theme.color(Role::DangerBase));
    let mut best = [0u8; 4];
    let mut score = u32::MAX;
    for y in 0..100u32 {
        for x in 0..100u32 {
            let p = pixel(&px, 100, x, y);
            let d = p.iter().zip(want.iter()).map(|(a, b)| u32::from(a.abs_diff(*b))).sum::<u32>();
            if d < score {
                score = d;
                best = p;
            }
        }
    }
    assert!(close(best, want, 12), "stroke {best:?} vs danger.base {want:?}");
}

#[test]
fn an_unknown_icon_name_draws_nothing_and_keeps_its_box() {
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let ic = StyleRecord { width: Dim::Px(40), height: Dim::Px(40), ..Default::default() };
    let mut nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Icon, 2, 2, 0)];
    nodes[1].props = (0, 1);
    let mut fx = fixture(vec![col, ic], nodes, vec![(1, Value::Str("no_such_icon".into()))], &["name"], 100.0, 100.0);
    let list = draw(&mut fx, 100, 100, 1.0);
    assert!(list.quads.is_empty(), "nothing is drawn for a name this client does not know");
    let rect = fx.layout.rect(fx.session.lookup(2).unwrap()).unwrap();
    assert_eq!((rect.w, rect.h), (40.0, 40.0), "and the space it asked for is still there");
}

#[test]
fn scroll_clips_at_the_pixel_level() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let sc = StyleRecord { display: Display::Column, height: Dim::Px(30), ..Default::default() };
    let bg = StyleRecord { bg: ColorRef::role(Role::DangerBase.id()), height: Dim::Px(100), ..Default::default() };
    let mut fx = fixture(vec![col, sc, bg], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Scroll, 2, 2, 1), node(NodeKind::Box, 3, 3, 0)], vec![], &[], 50.0, 100.0);
    let list = draw(&mut fx, 50, 100, 1.0);
    let target = r.offscreen(50, 100);
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    let danger = rgba_of(fx.theme.color(Role::DangerBase));
    let surface = rgba_of(fx.theme.color(Role::SurfaceBase));
    assert!(close(pixel(&px, 50, 25, 10), danger, 2), "inside the scroll box");
    assert!(close(pixel(&px, 50, 25, 29), danger, 2), "last row inside");
    assert!(close(pixel(&px, 50, 25, 30), surface, 2), "first row outside is clipped");
    assert!(close(pixel(&px, 50, 25, 90), surface, 2));
}

#[test]
fn stack_paints_in_z_order() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let stack = StyleRecord { display: Display::Stack, ..Default::default() };
    let low = StyleRecord { bg: ColorRef::role(Role::DangerBase.id()), width: Dim::Px(40), height: Dim::Px(40), z: 0, ..Default::default() };
    let high = StyleRecord { bg: ColorRef::role(Role::SuccessBase.id()), width: Dim::Px(40), height: Dim::Px(40), z: 9, ..Default::default() };
    // The high-z child comes first in child order and must still paint on top.
    let mut fx = fixture(vec![stack, low, high], vec![node(NodeKind::Box, 1, 1, 2), node(NodeKind::Box, 2, 3, 0), node(NodeKind::Box, 3, 2, 0)], vec![], &[], 40.0, 40.0);
    let list = draw(&mut fx, 40, 40, 1.0);
    let target = r.offscreen(40, 40);
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    assert!(close(pixel(&px, 40, 20, 20), rgba_of(fx.theme.color(Role::SuccessBase)), 2));
}

#[test]
fn an_image_paints_its_pixels() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    const AVATAR: &[u8] = include_bytes!("../../../examples/counter-app/public/images/avatar.png");
    // Decode by hand here to keep eui-render free of the png crate: the
    // avatar's centre 8×8 is white and its corners are transparent, which is
    // all this test needs — so build the RGBA from that knowledge.
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let i = (y * 32 + x) * 4;
            let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
            if (12..20).contains(&x) && (12..20).contains(&y) {
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, 255]);
            } else if dx * dx + dy * dy <= 15.5 * 15.5 {
                rgba[i..i + 4].copy_from_slice(&[0x22, 0x29, 0xa8, 255]);
            }
        }
    }
    let _ = AVATAR.len();
    let hash = [7u8; 32];
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, padding: [4; 4], ..Default::default() }; // 12 px
    let img = StyleRecord { width: Dim::Px(32), height: Dim::Px(32), ..Default::default() };
    let mut nodes = vec![node(NodeKind::Box, 1, 1, 1)];
    nodes.push(FlatNode { kind: NodeKind::Image, id: 2, style: 2, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 0 });
    let mut fx = fixture(vec![col, img], nodes, vec![(1, Value::Asset(hash))], &["src"], 100.0, 100.0);
    fx.images.insert(hash, 32, 32, &rgba);
    let list = draw(&mut fx, 100, 100, 1.0);
    assert_eq!(list.quads.iter().filter(|q| q.params[2] as u32 == TEXTURED_RGBA).count(), 1);
    let target = r.offscreen(100, 100);
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    let surface = rgba_of(fx.theme.color(Role::SurfaceBase));
    // Centre of the image (12+16, 12+16): white. Corner (12, 12): the surface shows through.
    assert!(close(pixel(&px, 100, 28, 28), [255, 255, 255, 255], 2), "{:?}", pixel(&px, 100, 28, 28));
    assert!(close(pixel(&px, 100, 12, 12), surface, 2), "{:?}", pixel(&px, 100, 12, 12));
    // A point on the disc: ultramarine.
    assert!(close(pixel(&px, 100, 16, 28), [0x22, 0x29, 0xa8, 255], 3), "{:?}", pixel(&px, 100, 16, 28));
}

// ------------------------------------------------------------- canvas

fn canvas_fixture(paths: Value, w: f32, h: f32) -> Fx {
    // A 100 × 50 canvas at the top-left of a column (the root itself would
    // take the viewport).
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
    let cv = StyleRecord { width: Dim::Px(100), height: Dim::Px(50), ..Default::default() };
    let mut nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Canvas, 2, 2, 0)];
    nodes[1].props = (0, 1);
    fixture(vec![col, cv], nodes, vec![(1, paths)], &["paths"], w, h)
}

#[test]
fn canvas_paths_become_capsules_strips_circles_and_arcs() {
    let i = Value::Int;
    let paths = Value::List(vec![
        Value::List(vec![i(0), Value::Str("accent.base".into()), i(2), i(0), i(0), i(30), i(40)]),
        Value::List(vec![i(1), Value::Str("#ff0000".into()), i(10), i(10), i(20), i(5), i(0)]),
        Value::List(vec![i(2), Value::Color(ColorRef::role(Role::InfoSubtle.id())), i(50), i(0), i(50), i(10), i(40)]),
        Value::List(vec![i(3), Value::Str("info.base".into()), i(50), i(25), i(5)]),
        Value::List(vec![i(4), Value::Int(i64::from(Role::SuccessBase.id())), i(4), i(80), i(25), i(15), Value::Float(0.0), Value::Float(std::f64::consts::FRAC_PI_2)]),
        Value::List(vec![i(9), Value::Str("accent.base".into()), i(1)]),
        Value::List(vec![i(0), Value::Str("no.such.role".into()), i(1), i(0), i(0), i(9), i(9)]),
    ]);
    let mut fx = canvas_fixture(paths, 200.0, 100.0);
    let list = draw(&mut fx, 200, 100, 1.0);
    // The canvas clips to its box.
    assert_eq!(list.clips, vec![[0, 0, 200, 100], [0, 0, 100, 50]]);
    let rotated: Vec<&Quad> = list.quads.iter().filter(|q| q.extra[0] != 0.0).collect();
    // The diagonal: one capsule, length 50 plus the width, round caps.
    let seg = rotated.iter().find(|q| (q.rect[2] - 52.0).abs() < 0.01).expect("the segment");
    assert!((seg.extra[0] - 40f32.atan2(30.0)).abs() < 1e-5);
    assert_eq!(seg.params[0], 1.0, "radius is half the width");
    assert_eq!(seg.fill, linear(fx.theme.color(Role::AccentBase)));
    // The rectangle, in literal red.
    assert!(list.quads.iter().any(|q| q.rect == [10.0, 10.0, 20.0, 5.0] && q.fill == linear(0xFF00_00FF)));
    // The area: one strip per device column down to the base; the column
    // at x = 0 sits on the base line and is empty, so ten strips.
    let strips: Vec<&Quad> = list.quads.iter().filter(|q| q.rect[2] == 1.0 && q.fill == linear(fx.theme.color(Role::InfoSubtle))).collect();
    assert_eq!(strips.len(), 10);
    assert_eq!(strips[0].rect, [1.0, 49.0, 1.0, 1.0]);
    assert_eq!(strips[9].rect, [10.0, 40.0, 1.0, 10.0]);
    // The circle: a 10 × 10 quad with a 5 px radius.
    assert!(list.quads.iter().any(|q| q.rect == [45.0, 20.0, 10.0, 10.0] && q.params[0] == 5.0 && q.fill == linear(fx.theme.color(Role::InfoBase))));
    // The quarter arc at 6° pitch: fifteen capsules in success.base.
    let arc = rotated.iter().filter(|q| q.fill == linear(fx.theme.color(Role::SuccessBase))).count();
    assert_eq!(arc, 15);
    // The unknown kind and the unknown colour drew nothing.
    assert_eq!(rotated.len(), 16);
}

#[test]
fn a_canvas_line_lands_on_its_pixels() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let i = Value::Int;
    let paths = Value::List(vec![Value::List(vec![i(0), Value::Str("accent.base".into()), i(6), i(0), i(0), i(40), i(40)])]);
    let mut fx = canvas_fixture(paths, 100.0, 50.0);
    let list = draw(&mut fx, 100, 50, 1.0);
    let target = r.offscreen(100, 50);
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    let accent = rgba_of(fx.theme.color(Role::AccentBase));
    let ground = rgba_of(fx.theme.color(Role::SurfaceBase));
    // On the diagonal: the line. Off it: the surface. The strip is one
    // strip: no gap between the two clipped halves of the capsule.
    assert!(close(pixel(&px, 100, 20, 20), accent, 2), "{:?}", pixel(&px, 100, 20, 20));
    assert!(close(pixel(&px, 100, 30, 30), accent, 2));
    assert!(close(pixel(&px, 100, 20, 35), ground, 2), "{:?}", pixel(&px, 100, 20, 35));
    assert!(close(pixel(&px, 100, 60, 10), ground, 2));
}

#[test]
fn a_shadow_is_a_grown_black_quad_painted_before_its_box() {
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, padding: [6; 4], ..Default::default() };
    let card = StyleRecord { bg: ColorRef::role(Role::SurfaceRaised.id()), width: Dim::Px(40), height: Dim::Px(20), shadow: 2, radius: 2, ..Default::default() };
    let mut fx = fixture(vec![col, card], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)], vec![], &[], 200.0, 100.0);
    let r = fx.layout.rect(fx.session.lookup(2).unwrap()).unwrap();
    let list = draw(&mut fx, 200, 100, 1.0);
    assert_eq!(list.quads.len(), 2, "{list:#?}");
    let (dy, blur, alpha) = fx.theme.shadow[2];
    let shadow = &list.quads[0];
    assert_eq!(shadow.rect, [r.x - blur, r.y + dy - blur, r.w + 2.0 * blur, r.h + 2.0 * blur]);
    assert_eq!(shadow.fill, [0.0, 0.0, 0.0, alpha]);
    assert_eq!(shadow.extra[1], blur, "the fragment stage fades across the blur");
    assert_eq!(shadow.params[0], fx.theme.radius(2).unwrap() + blur);
    assert_eq!(list.quads[1].rect, [r.x, r.y, r.w, r.h]);
}

// ------------------------------------------------------------------ blur

/// Two halves, one red one blue, under a pane that may or may not frost
/// them. `blur` of zero is the ordinary single-pass frame, which is what
/// the blurred one is judged against.
fn seam(blur: u8) -> (Fx, DrawList) {
    let root = StyleRecord { display: Display::Stack, width: Dim::Px(40), height: Dim::Px(20), ..Default::default() };
    let halves = StyleRecord { display: Display::Row, width: Dim::Px(40), height: Dim::Px(20), ..Default::default() };
    let half = |role: Role| StyleRecord { bg: ColorRef::role(role.id()), width: Dim::Px(20), height: Dim::Px(20), ..Default::default() };
    let pane = StyleRecord { blur, width: Dim::Px(40), height: Dim::Px(20), ..Default::default() };
    let mut fx = fixture(
        vec![root, halves, half(Role::DangerBase), half(Role::InfoBase), pane],
        vec![node(NodeKind::Box, 1, 1, 2), node(NodeKind::Box, 2, 2, 2), node(NodeKind::Box, 3, 3, 0), node(NodeKind::Box, 4, 4, 0), node(NodeKind::Box, 5, 5, 0)],
        vec![],
        &[],
        40.0,
        20.0,
    );
    let list = draw(&mut fx, 40, 20, 1.0);
    (fx, list)
}

#[test]
fn a_frame_with_no_blur_asks_for_no_backdrop() {
    let (_, list) = seam(0);
    assert_eq!(list.backdrop, None, "the ordinary frame stays one pass");
    assert!(list.quads.iter().all(|q| q.params[2] as u32 & BLURRED == 0));
}

#[test]
fn a_blur_names_the_backdrop_its_quad_will_sample() {
    let (_, list) = seam(4);
    let b = list.backdrop.expect("a blurred node asks for a backdrop");
    assert_eq!(b.sigmas, vec![4.0], "one radius, one chain");
    // Everything before the pane is the backdrop, and the pane is not
    // behind itself: the two halves are quads 0 and 1.
    assert_eq!(b.first, 2);
    // The region wanted the pane grown by three standard deviations and got
    // the framebuffer, which is all there was to give.
    assert_eq!(b.rect, [0, 0, 40, 20]);
    let pane = &list.quads[2];
    assert_eq!(pane.params[2] as u32 & BLURRED, BLURRED, "the pane samples it");
    assert_eq!(pane.extra[2], 4.0, "at the standard deviation it asked for");
    // One radius is chain 0, which is also what a run with nothing blurred
    // in it carries, so a frame like this never has to split a run: the
    // fragment reads the binding or ignores it, and the picture is the same.
    assert_eq!(list.runs.len(), 1);
    assert_eq!(list.runs[0].chain, 0);
}

/// Two radii are two chains, and a run binds one blur — so here the runs do
/// have to split, and each pane has to end up on its own.
#[test]
fn two_radii_are_two_chains_on_runs_of_their_own() {
    let root = StyleRecord { display: Display::Stack, width: Dim::Px(40), height: Dim::Px(20), ..Default::default() };
    let pane = |blur: u8| StyleRecord { blur, width: Dim::Px(40), height: Dim::Px(20), ..Default::default() };
    let mut fx = fixture(vec![root, pane(4), pane(9)], vec![node(NodeKind::Box, 1, 1, 2), node(NodeKind::Box, 2, 2, 0), node(NodeKind::Box, 3, 3, 0)], vec![], &[], 40.0, 20.0);
    let list = draw(&mut fx, 40, 20, 1.0);
    let b = list.backdrop.clone().expect("a backdrop");
    assert_eq!(b.sigmas, vec![4.0, 9.0], "one chain per distinct radius");
    // Both panes share the one backdrop: it was taken before the first of
    // them, so the second does not see the first (03 §2.1).
    assert_eq!(b.first, 0);
    let chains: Vec<u32> = list.runs.iter().map(|r| r.chain).collect();
    assert_eq!(chains, vec![0, 1], "the second pane's run binds the second chain");
    assert!(list.runs.iter().all(|r| r.count == 1));
}

/// The one that fails if any pass of the chain is skipped: with no blur the
/// seam is a step from one role to the other, and with it each side has to
/// have taken on some of the other.
#[test]
fn a_blurred_pane_carries_each_half_across_the_seam() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let shot = |r: &mut Renderer, st: &mut SessionTextures, blur: u8| {
        let (mut fx, list) = seam(blur);
        let target = r.offscreen(40, 20);
        r.render_offscreen(st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
        r.read_back(&target).unwrap()
    };
    let sharp = shot(&mut r, &mut st, 0);
    let frosted = shot(&mut r, &mut st, 4);
    let red = rgba_of(fx_role(Role::DangerBase));
    let blue = rgba_of(fx_role(Role::InfoBase));

    // The sharp frame is the step it always was.
    assert!(close(pixel(&sharp, 40, 17, 10), red, 2), "left of the seam is red");
    assert!(close(pixel(&sharp, 40, 22, 10), blue, 2), "right of the seam is blue");

    // The frosted one has carried each half into the other. Which channel
    // moved does not matter; that both sides moved towards the other does.
    let d = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).map(|(x, y)| i32::from(*x) - i32::from(y)).map(i32::abs).sum::<i32>();
    let left = pixel(&frosted, 40, 17, 10);
    let right = pixel(&frosted, 40, 22, 10);
    assert!(d(left, red) > 8, "left of the seam took on the blue: {left:?} was {red:?}");
    assert!(d(right, blue) > 8, "right of the seam took on the red: {right:?} was {blue:?}");
    assert!(d(left, right) < d(red, blue), "and the two are closer than the roles are");

    // Far from the seam the blur has nothing to mix in, so the colour
    // survives: a chain that smeared everything would fail here.
    assert!(close(pixel(&frosted, 40, 2, 10), red, 12), "the far left is still red: {:?}", pixel(&frosted, 40, 2, 10));
    assert!(close(pixel(&frosted, 40, 37, 10), blue, 12), "the far right is still blue");
}

/// The snapshot is a sub-rect of the frame, so the fragment stage has to map
/// its own position into that rect rather than into the framebuffer. Get the
/// origin wrong and the pane shows a piece of the page from somewhere else —
/// which the seam test above cannot catch, because there the region *is* the
/// whole frame.
#[test]
fn a_pane_smaller_than_the_frame_frosts_what_is_actually_behind_it() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let stack = StyleRecord { display: Display::Stack, width: Dim::Px(80), height: Dim::Px(20), ..Default::default() };
    let row = StyleRecord { display: Display::Row, width: Dim::Px(80), height: Dim::Px(20), ..Default::default() };
    let half = |role: Role| StyleRecord { bg: ColorRef::role(role.id()), width: Dim::Px(40), height: Dim::Px(20), ..Default::default() };
    let gap = StyleRecord { width: Dim::Px(30), height: Dim::Px(20), ..Default::default() };
    let pane = StyleRecord { width: Dim::Px(20), height: Dim::Px(20), blur: 3, ..Default::default() };
    let mut fx = fixture(
        vec![stack, row, half(Role::DangerBase), half(Role::InfoBase), row, gap, pane],
        vec![
            node(NodeKind::Box, 1, 1, 2),
            node(NodeKind::Box, 2, 2, 2),
            node(NodeKind::Box, 3, 3, 0),
            node(NodeKind::Box, 4, 4, 0),
            node(NodeKind::Box, 5, 5, 2),
            node(NodeKind::Box, 6, 6, 0),
            node(NodeKind::Box, 7, 7, 0),
        ],
        vec![],
        &[],
        80.0,
        20.0,
    );
    let list = draw(&mut fx, 80, 20, 1.0);
    let b = list.backdrop.as_ref().expect("a backdrop");
    // The pane sits at x 30..50; three standard deviations is nine px, and
    // the top and bottom clip to the frame.
    assert_eq!(b.rect, [21, 0, 38, 20], "a sub-rect, not the framebuffer");

    let target = r.offscreen(80, 20);
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let px = r.read_back(&target).unwrap();
    let red = rgba_of(fx_role(Role::DangerBase));
    let blue = rgba_of(fx_role(Role::InfoBase));
    let at = |x| pixel(&px, 80, x, 10);

    // Outside the pane the page is untouched: the blur is confined to the
    // node that asked for it.
    assert!(close(at(25), red, 2), "left of the pane {:?}", at(25));
    assert!(close(at(55), blue, 2), "right of the pane {:?}", at(55));
    // Inside it, and away from the seam, the colour still has to be the one
    // that is actually there — which is what an origin off by 21 px breaks.
    assert!(close(at(32), red, 5), "inside the pane, left of the seam {:?}", at(32));
    assert!(close(at(47), blue, 5), "inside the pane, right of the seam {:?}", at(47));
    // And on the seam, half of each.
    let mid = at(40);
    for c in 0..3 {
        let (lo, hi) = (red[c].min(blue[c]), red[c].max(blue[c]));
        assert!(mid[c] > lo && mid[c] < hi, "channel {c}: {mid:?} not between {red:?} and {blue:?}");
    }
}

fn fx_role(role: Role) -> u32 {
    Theme::default().resolve(Viewer::default()).color(role)
}

/// The snapshot is the region that asked for it, not the framebuffer. This
/// is the whole memory argument in `10-budgets.md`: a small frosted node in
/// a big window costs a small texture, and a 4K frame does not quietly
/// allocate 33 MB because someone frosted a tooltip.
#[test]
fn the_backdrop_is_only_as_big_as_the_blur_reaches() {
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
    // No `bg` at all: clear glass is still glass, and still worth a quad.
    let glass = StyleRecord { width: Dim::Px(40), height: Dim::Px(20), blur: 6, ..Default::default() };
    let mut fx = fixture(vec![col, glass], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)], vec![], &[], 400.0, 300.0);
    let r = fx.layout.rect(fx.session.lookup(2).unwrap()).unwrap();
    let list = draw(&mut fx, 400, 300, 1.0);
    assert_eq!(list.quads.len(), 1, "a blurred node is painted even with no bg: {list:#?}");
    let b = list.backdrop.as_ref().expect("a blur asks for a backdrop");
    // Three standard deviations on every side, past which a Gaussian stops
    // mattering; clipped to the frame, which is why the near edges are 0.
    assert_eq!(b.rect, [0, 0, (r.x + r.w + 18.0) as u32, (r.y + r.h + 18.0) as u32]);
    assert!(b.rect[2] < 400 && b.rect[3] < 300, "not the whole framebuffer: {:?}", b.rect);
}

/// 03 §5: a node mid-transition paints both ends and the clock, and the
/// list it is in is the same list for the whole of it. A transition of the
/// blur cannot be the vertex stage's -- the backdrop is sized from it -- so
/// that one is painted where it stands, as every transition once was.
#[test]
fn a_transition_paints_both_ends_and_the_clock() {
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let bg = StyleRecord { bg: ColorRef::role(Role::AccentBase.id()), height: Dim::Px(10), ..Default::default() };
    let mut fx = fixture(vec![col, bg], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)], vec![], &[], 200.0, 100.0);
    let ix = fx.session.lookup(2).unwrap();
    let accent = eui_render::linear(fx.theme.color(Role::AccentBase));
    let from = Colors { bg: Some(accent), fg: None, border: None, opacity: 1.0, blur: 0.0 };
    let to = Colors { bg: Some([0.5, 0.25, 0.125, 1.0]), fg: None, border: None, opacity: 0.5, blur: 0.0 };
    let at = Colors { bg: Some([0.6, 0.4, 0.3, 1.0]), fg: None, border: None, opacity: 0.75, blur: 0.0 };
    let mut anim = GpuAnim { from, to, at, t0: -0.04, dur: 0.18, decelerate: false, baked: false };
    let draw = |fx: &mut Fx, anims: &[(eui_tree::NodeIx, GpuAnim)]| {
        paint(&mut Scene {
            session: &fx.session,
            layout: &fx.layout,
            theme: &fx.theme,
            text: &mut fx.text,
            atlas: &mut fx.atlas,
            images: &fx.images,
            scale: 1.0,
            size: (200, 100),
            focus: None,
            anims,
            glides: &[],
            cache: &mut PaintCache::new(),
            editing: None,
            now: 0.0,
            scrollbar_hot: None,
        })
    };
    let list = draw(&mut fx, &[(ix, anim)]);
    let q = list.quads[0];
    assert_eq!(q.fill, [0.5, 0.25, 0.125, 1.0], "where it is going");
    assert_eq!(unpack4([q.from[0], q.from[1], q.from[2], q.from[3]]), pack_round(accent), "where it came from");
    assert_eq!(q.params[3], 0.5, "the opacity it reaches");
    assert_eq!(q.extra[3], 1.0, "and the one it left");
    assert_eq!((q.spin[2], q.spin[3]), (-0.04, 0.18), "when, relative to this paint");
    assert!(u32::try_from(q.params[2] as i64).is_ok_and(|f| f & ANIMATED != 0 && f & DECELERATE == 0), "marked for the vertex stage: {q:?}");
    assert!(!list.cpu_bound, "and the list is good for every frame of it");

    // The same, decelerating: an entrance's curve.
    anim.decelerate = true;
    let list = draw(&mut fx, &[(ix, anim)]);
    assert!(u32::try_from(list.quads[0].params[2] as i64).is_ok_and(|f| f & DECELERATE != 0));

    // Baked: painted at `at`, no ends, no clock.
    anim.baked = true;
    let list = draw(&mut fx, &[(ix, anim)]);
    let q = list.quads[0];
    assert_eq!(q.fill, [0.6, 0.4, 0.3, 1.0]);
    assert_eq!(q.params[3], 0.75);
    assert_eq!((q.extra[3], q.spin[2], q.spin[3], q.from), (0.0, 0.0, 0.0, [0; 8]));
    assert!(u32::try_from(q.params[2] as i64).is_ok_and(|f| f & ANIMATED == 0));
}

/// A colour through sixteen bits and back, to the precision that survives.
fn pack_round(c: [f32; 4]) -> [f32; 4] {
    unpack4(eui_render::pack4(c))
}

/// One channel of linear light as the sRGB target stores it.
fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// The vertex stage eases between the ends from the list's age: at the
/// start the colour it left, halfway the theme's curve says where, and past
/// the end the colour it reaches. The same list every time.
#[test]
fn a_transition_renders_its_midway_colour_at_half_time() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let (mut fx, mut list) = spinning_bar();
    // One plain quad, red to blue over a second, from the paint.
    let red = [1.0, 0.0, 0.0, 1.0];
    let blue = [0.0, 0.0, 1.0, 1.0];
    let flags = ANIMATED as f32;
    let from = [eui_render::pack4(red), [0; 4]].concat().try_into().unwrap();
    list.quads = vec![Quad { rect: [10.0, 10.0, 40.0, 40.0], params: [0.0, 0.0, flags, 1.0], fill: blue, extra: [0.0, 0.0, 0.0, 1.0], spin: [0.0, 0.0, 0.0, 1.0], from, ..Quad::default() }];
    list.runs = vec![eui_render::Run { clip: 0, chain: 0, first: 0, count: 1 }];
    list.clear = [0.0, 0.0, 0.0, 1.0];
    list.wants_frame = false;
    list.serial = 9;
    let target = r.offscreen(100, 100);
    let mut at = |r: &mut Renderer, st: &mut SessionTextures, age: f32| {
        r.render_offscreen_at(st, &target, 0.0, age, &list, &mut fx.atlas, &mut fx.images);
        let px = r.read_back(&target).unwrap();
        let i = (30 * 100 + 30) * 4;
        [px[i], px[i + 1], px[i + 2]]
    };
    let start = at(&mut r, &mut st, 0.0);
    let mid = at(&mut r, &mut st, 0.5);
    let end = at(&mut r, &mut st, 2.0);
    assert!(start[0] > 250 && start[2] < 5, "at the start, the colour it left: {start:?}");
    assert!(end[2] > 250 && end[0] < 5, "past the end, the colour it reaches: {end:?}");
    // Halfway along the theme's curve, in linear light, then to sRGB as the
    // target stores it; three levels of slack for the interpolation's
    // rounding.
    let k = eui_theme::Curve::STANDARD.at(0.5);
    let srgb = |v: f32| (linear_to_srgb(v) * 255.0).round() as i32;
    let want = [srgb(1.0 - k), 0, srgb(k)];
    for (got, want) in mid.iter().zip(want) {
        assert!((i32::from(*got) - want).abs() <= 3, "halfway: {mid:?}, wanted {want:?} at k = {k}");
    }
}

/// 04 §7: a quad carried by a scroller is drawn on its way from where the
/// glide began to where the layout put it, along the curve the record
/// names, from the list's age.
#[test]
fn a_glide_moves_a_quad_by_its_eased_offset() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let (mut fx, mut list) = spinning_bar();
    let white = [1.0, 1.0, 1.0, 1.0];
    let slot = (1u32 << SCROLLER_SHIFT) as f32;
    // A 10 px bar put at y = 40, gliding there from 40 px lower over a second.
    list.quads = vec![Quad { rect: [10.0, 40.0, 40.0, 10.0], params: [0.0, 0.0, slot, 1.0], fill: white, ..Quad::default() }];
    list.runs = vec![eui_render::Run { clip: 0, chain: 0, first: 0, count: 1 }];
    list.clips = vec![[0, 0, 100, 100]];
    list.clear = [0.0, 0.0, 0.0, 1.0];
    list.wants_frame = false;
    list.scrollers = vec![Scroller { from: [0.0, 40.0], to: [0.0, 0.0], t0: 0.0, dur: 1.0, curve: 0, pad: 0 }];
    let target = r.offscreen(100, 100);
    // The first row of ink in column 30.
    let mut top = |r: &mut Renderer, st: &mut SessionTextures, age: f32| -> Option<usize> {
        r.render_offscreen_at(st, &target, 0.0, age, &list, &mut fx.atlas, &mut fx.images);
        let px = r.read_back(&target).unwrap();
        (0..100).find(|y| px[(y * 100 + 30) * 4] > 128)
    };
    assert_eq!(top(&mut r, &mut st, 0.0), Some(80), "at the start, 40 px below where it was put");
    assert_eq!(top(&mut r, &mut st, 2.0), Some(40), "past the end, where it was put");
    let k = eui_theme::Curve::STANDARD.at(0.5);
    let want = (40.0 + 40.0 * (1.0 - k)).round() as usize;
    let mid = top(&mut r, &mut st, 0.5).expect("ink halfway");
    assert!(mid.abs_diff(want) <= 1, "halfway along the curve: at {mid}, wanted {want}");
}

/// Asked to, the renderer times its main pass on the GPU and reports the
/// frame before's figure -- read without waiting, so a frame never stalls
/// on it.
#[test]
fn a_timed_renderer_reports_the_previous_frames_gpu_time() {
    // Its own device, since the shared one asked for no timestamps -- but
    // taken while holding the shared lock, so the two never overlap.
    let _queue = gpu();
    let Ok(mut r) = Renderer::new_headless_timed(true) else { return };
    let mut st = r.session();
    let (mut fx, list) = spinning_bar();
    let target = r.offscreen(100, 100);
    let first = r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    assert_eq!(first.gpu_ms, None, "nothing to report on the first frame");
    // Wait for the GPU, as a test may and a frame must not, then ask again.
    let _ = r.read_back(&target);
    let mut reported = None;
    for _ in 0..5 {
        let st2 = r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
        if let Some(ms) = st2.gpu_ms {
            reported = Some(ms);
            break;
        }
        let _ = r.read_back(&target);
    }
    // An adapter without timestamps reports nothing, and that is allowed.
    if let Some(ms) = reported {
        assert!((0.0..1000.0).contains(&ms), "a frame's worth: {ms} ms");
    }
}

/// A text node that did not change paints the quads it painted last
/// frame; one whose box moved by whole pixels -- a scroll -- paints them
/// moved; one that changed, or paints under another colour, is built
/// afresh. The lists come out the same either way.
#[test]
fn retained_glyph_quads_are_replayed_until_the_node_changes() {
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
    let sc = StyleRecord { display: Display::Column, height: Dim::Px(60), ..Default::default() };
    // The scroller's own style, recoloured: a restyle that lays out alike.
    let lit = StyleRecord { fg: ColorRef::role(Role::AccentBase.id()), ..sc };
    let nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Scroll, 2, 2, 3), text(3, 0, "first line"), text(4, 0, "second line"), text(5, 0, "third line")];
    let mut fx = fixture(vec![col, sc, lit], nodes, vec![], &[], 200.0, 100.0);
    let mut cache = PaintCache::new();
    let draw = |fx: &mut Fx, cache: &mut PaintCache| {
        paint(&mut Scene {
            session: &fx.session,
            layout: &fx.layout,
            theme: &fx.theme,
            text: &mut fx.text,
            atlas: &mut fx.atlas,
            images: &fx.images,
            scale: 1.0,
            size: (200, 100),
            focus: None,
            anims: &[],
            glides: &[],
            cache,
            editing: None,
            now: 0.0,
            scrollbar_hot: None,
        })
    };
    let relayout = |fx: &mut Fx| fx.layout.compute(&mut Env { session: &fx.session, theme: &fx.theme, text: &mut fx.text }, Size::new(200.0, 100.0));
    let first = draw(&mut fx, &mut cache);
    assert_eq!(cache.stats().rebuilt, 3, "three text nodes built");
    fx.session.clear_all_dirty();
    let again = draw(&mut fx, &mut cache);
    assert_eq!(again, first, "the same list");
    assert_eq!((cache.stats().hits, cache.stats().rebuilt), (3, 3), "from the cache, all three");
    // A scroll (of five: the content is 66 px in a 60 px box): the same
    // glyphs, moved.
    let scroll = fx.session.lookup(2).unwrap();
    fx.session.set_scroll(scroll, 0, 5);
    relayout(&mut fx);
    let scrolled = draw(&mut fx, &mut cache);
    assert_eq!(cache.stats().translated, 3, "moved, not rebuilt");
    let glyph_top = |l: &DrawList| l.quads.iter().filter(|q| q.params[2] as u32 & TEXTURED != 0).map(|q| q.rect[1]).fold(f32::MAX, f32::min);
    assert_eq!(glyph_top(&scrolled), glyph_top(&first) - 5.0, "five pixels up");
    fx.session.clear_all_dirty();
    let mut fresh = PaintCache::new();
    assert_eq!(draw(&mut fx, &mut fresh), scrolled, "and exactly what a cold paint gives");
    // A change to one node rebuilds that node and no other.
    let second = fx.session.lookup(4).unwrap();
    assert!(fx.session.set_text_local(second, "second, changed".into()));
    let changed = draw(&mut fx, &mut cache);
    assert_eq!((cache.stats().hits, cache.stats().rebuilt), (5, 4), "two as they were, one afresh");
    assert_ne!(changed, scrolled);
    fx.session.clear_all_dirty();
    // A colour from the parent is in the key: a restyle above rebuilds
    // without a dirty bit on the text.
    assert_eq!(fx.session.set_style_local(scroll, 3), Some(true));
    fx.session.clear_all_dirty();
    let _ = draw(&mut fx, &mut cache);
    assert_eq!(cache.stats().rebuilt, 7, "all three, under the new colour");
    // Nothing painted for a frame is dropped.
    assert_eq!(cache.len(), 3);
}

/// A picture arriving is not a reason to lose the words.
///
/// The image texture is made full size the first time a picture is packed,
/// and the two atlas textures share a bind group, so growing one remakes
/// the other. The glyph texture that comes back is blank: unless the glyph
/// atlas is told to send everything again, every glyph uploaded before that
/// moment is gone from the GPU while the client still believes it is there
/// -- and assets arrive over the network, long after the first frames, so
/// the page renders and then loses its text.
#[test]
fn a_picture_arriving_does_not_wipe_the_glyphs_already_uploaded() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
    let mut fx = fixture(vec![col], vec![node(NodeKind::Box, 1, 1, 1), text(2, 0, "words")], vec![], &[], 100.0, 100.0);
    let list = draw(&mut fx, 100, 100, 1.0);
    let target = r.offscreen(100, 100);
    // Ink is what is darker than the surface it sits on.
    let ink = |r: &mut Renderer| r.read_back(&target).unwrap().chunks(4).filter(|p| p[0] < 200).count();

    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    let before = ink(&mut r);
    assert!(before > 20, "the words were drawn: {before} inked texels");

    // A picture is packed, as one does when its bytes arrive: the image
    // texture grows from nothing to its full size, and the glyph texture
    // is remade with it.
    fx.images.insert([9; 32], 8, 8, &vec![255u8; 8 * 8 * 4]).expect("an 8x8 picture packs");
    r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    assert_eq!(ink(&mut r), before, "the same words, drawn again after the picture landed");
}

/// An entrance starts from no opacity: the quad is not there at age zero
/// and is on its way at half time, along the entrance's own curve.
#[test]
fn an_entering_quad_fades_its_opacity_from_nothing() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let (mut fx, mut list) = spinning_bar();
    let white = [1.0, 1.0, 1.0, 1.0];
    let flags = (ANIMATED | DECELERATE) as f32;
    list.quads =
        vec![Quad { rect: [10.0, 10.0, 40.0, 40.0], params: [0.0, 0.0, flags, 1.0], fill: white, extra: [0.0, 0.0, 0.0, 0.0], spin: [0.0, 0.0, 0.0, 1.0], from: [65535; 8], ..Quad::default() }];
    list.runs = vec![eui_render::Run { clip: 0, chain: 0, first: 0, count: 1 }];
    list.clear = [0.0, 0.0, 0.0, 1.0];
    list.wants_frame = false;
    let target = r.offscreen(100, 100);
    let mut at = |r: &mut Renderer, st: &mut SessionTextures, age: f32| {
        r.render_offscreen_at(st, &target, 0.0, age, &list, &mut fx.atlas, &mut fx.images);
        r.read_back(&target).unwrap()[(30 * 100 + 30) * 4]
    };
    let start = at(&mut r, &mut st, 0.0);
    let mid = at(&mut r, &mut st, 0.5);
    let end = at(&mut r, &mut st, 1.5);
    assert!(start < 3, "nothing there yet: {start}");
    assert_eq!(end, 255, "and then all there");
    let k = eui_theme::Curve::DECELERATE.at(0.5);
    let want = (linear_to_srgb(k) * 255.0).round() as i32;
    assert!((i32::from(mid) - want).abs() <= 3, "halfway along the entrance curve: {mid}, wanted {want} at k = {k}");
}

#[test]
fn an_edited_field_paints_its_selection_and_caret_and_clips_scrolled_text() {
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
    let field = StyleRecord { width: Dim::Px(60), padding: [2; 4], ..Default::default() };
    let mut nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Input, 2, 2, 0)];
    nodes[1].text = Some(TextRef::Inline("hello".into()));
    let mut fx = fixture(vec![col, field], nodes, vec![], &[], 200.0, 100.0);
    let ix = fx.session.lookup(2).unwrap();
    let editing = Some(Editing { node: ix, start: 1, end: 3, caret: 3, scroll_x: 4.0 });
    let list = paint(&mut Scene {
        session: &fx.session,
        layout: &fx.layout,
        theme: &fx.theme,
        text: &mut fx.text,
        atlas: &mut fx.atlas,
        images: &fx.images,
        scale: 1.0,
        size: (200, 100),
        focus: None,
        anims: &[],
        glides: &[],
        cache: &mut PaintCache::new(),
        editing,
        now: 0.0,
        scrollbar_hot: None,
    });
    let boxes: Vec<&Quad> = list.quads.iter().filter(|q| q.params[2] == 0.0).collect();
    assert_eq!(boxes.len(), 2, "one selection rect, one caret: {list:#?}");
    let mut accent = linear(fx.theme.color(Role::AccentBase));
    accent[3] *= 0.3;
    assert_eq!(boxes[0].fill, accent);
    assert_eq!(boxes[1].rect[2], 1.0, "the caret is one device px wide");
    assert!(boxes[1].rect[0] > boxes[0].rect[0], "the caret sits at the selection's end");
    // The field opened its own scissor run, and its glyphs shifted left by the scroll.
    assert_eq!(list.clips.len(), 2);
    let glyph_x = list.quads.iter().find(|q| q.params[2] as u32 == TEXTURED).unwrap().rect[0];
    let r = fx.layout.rect(ix).unwrap();
    assert!(glyph_x < r.x + 4.0 + 4.0, "{glyph_x} vs {}", r.x);
}

/// Spec 04 §7.1: a windowed list's rows in view without a child are drawn
/// as placeholders in `surface.sunken`; rows with a child, and rows out
/// of view, are not.
#[test]
fn a_windowed_list_paints_placeholders_for_the_rows_it_does_not_have() {
    // Atoms 1..3: item_height, count, row. A 100 px list of 40 rows of
    // 20 px, holding row 1 only.
    let mut list = node(NodeKind::List, 2, 2, 1);
    list.props = (0, 2);
    let mut present = node(NodeKind::Box, 3, 3, 0);
    present.props = (2, 1);
    let mut fx = fixture(
        vec![
            StyleRecord { display: Display::Column, ..Default::default() },
            StyleRecord { display: Display::Column, height: Dim::Px(100), ..Default::default() },
            StyleRecord { height: Dim::Px(10), bg: ColorRef::role(Role::AccentBase.id()), ..Default::default() },
        ],
        vec![node(NodeKind::Box, 1, 1, 1), list, present],
        vec![(1, Value::Int(20)), (2, Value::Int(40)), (3, Value::Int(1))],
        &["item_height", "count", "row"],
        300.0,
        200.0,
    );
    let list = draw(&mut fx, 300, 200, 1.0);
    let sunken = linear(fx.theme.color(Role::SurfaceSunken));
    let placeholders: Vec<&Quad> = list.quads.iter().filter(|q| q.fill == sunken && q.rect[3] > 5.0).collect();
    // Five rows fit in 100 px; row 1 has a child, the other four are placeholders.
    assert_eq!(placeholders.len(), 4, "{placeholders:?}");
    assert!(placeholders.iter().all(|q| q.rect[1] >= 0.0 && q.rect[1] < 100.0), "in view only");
    assert!(!placeholders.iter().any(|q| (q.rect[1] - 24.0).abs() < 1.0), "row 1 is real, not a placeholder");
    assert!(list.quads.iter().any(|q| q.fill == linear(fx.theme.color(Role::AccentBase))), "the present row painted itself");
}

/// Spec 03 §3: a single-line field taller than its line centres its text
/// and caret; a field of exactly one line does not move them.
#[test]
fn a_tall_field_centres_its_text_and_caret() {
    let caret_y = |height: u16| {
        let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
        let field = StyleRecord { width: Dim::Px(120), height: Dim::Px(height), ..Default::default() };
        let mut nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Input, 2, 2, 0)];
        nodes[1].text = Some(TextRef::Inline("hello".into()));
        let mut fx = fixture(vec![col, field], nodes, vec![], &[], 200.0, 100.0);
        let ix = fx.session.lookup(2).unwrap();
        let editing = Some(Editing { node: ix, start: 0, end: 0, caret: 5, scroll_x: 0.0 });
        let list = paint(&mut Scene {
            session: &fx.session,
            layout: &fx.layout,
            theme: &fx.theme,
            text: &mut fx.text,
            atlas: &mut fx.atlas,
            images: &fx.images,
            scale: 1.0,
            size: (200, 100),
            focus: None,
            anims: &[],
            glides: &[],
            cache: &mut PaintCache::new(),
            editing,
            now: 0.0,
            scrollbar_hot: None,
        });
        let caret = list.quads.iter().find(|q| q.params[2] == 0.0 && q.rect[2] == 1.0).expect("caret");
        let glyph = list.quads.iter().find(|q| q.params[2] as u32 == TEXTURED).expect("glyph");
        (caret.rect[1], glyph.rect[1])
    };
    // The default text line is 22 px: a 22 px field has nothing to centre.
    let (c22, g22) = caret_y(22);
    let (c44, g44) = caret_y(44);
    assert_eq!(c44 - c22, 11.0, "the caret moved down by half the spare height");
    assert_eq!(g44 - g22, 11.0, "and so did the text");
}

// A window's surface format is whatever the platform offers, and on Metal
// that is BGRA — never the RGBA `FORMAT` the off-screen path uses. Before
// the renderer built a pipeline per format, the Mac aborted inside
// `Surface::configure` on the first frame: "Requested format
// Rgba8UnormSrgb is not in list of supported formats". This draws the same
// scene into a BGRA target and checks it comes out as the same picture with
// the channels swapped, which is the whole of what the platform asked for.
#[test]
fn a_bgra_target_draws_the_same_picture_with_its_channels_swapped() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, padding: [3; 4], ..Default::default() };
    let bg = StyleRecord { bg: ColorRef::role(Role::AccentBase.id()), width: Dim::Px(40), height: Dim::Px(20), ..Default::default() };
    let mut fx = fixture(vec![col, bg], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)], vec![], &[], 100.0, 100.0);
    let list = draw(&mut fx, 100, 100, 1.0);

    // The same texture the surface would hand over, in the order Metal wants.
    let tex = r.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("bgra"),
        size: wgpu::Extent3d { width: 100, height: 100, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    r.render(&mut st, Target::whole(&view, wgpu::TextureFormat::Bgra8UnormSrgb, (100, 100), 0.0), &list, &mut fx.atlas, &mut fx.images);

    let px = read_texture(&mut r, &tex, 100, 100);
    let accent = rgba_of(fx.theme.color(Role::AccentBase));
    let surface = rgba_of(fx.theme.color(Role::SurfaceBase));
    let bgra = |c: [u8; 4]| [c[2], c[1], c[0], c[3]];
    assert!(close(pixel(&px, 100, 28, 18), bgra(accent), 2), "centre {:?}", pixel(&px, 100, 28, 18));
    assert!(close(pixel(&px, 100, 2, 2), bgra(surface), 2), "outside {:?}", pixel(&px, 100, 2, 2));
}

/// `read_back` only knows the off-screen target; this reads any texture.
fn read_texture(r: &mut Renderer, tex: &wgpu::Texture, w: u32, h: u32) -> Vec<u8> {
    let bpr = (w * 4).div_ceil(256) * 256;
    let buffer = r.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(bpr) * u64::from(h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = r.device().create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::ImageCopyBuffer { buffer: &buffer, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    r.queue().submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    r.device().poll(wgpu::Maintain::Wait);
    let mapped = slice.get_mapped_range();
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for row in 0..h {
        let start = (row * bpr) as usize;
        out.extend_from_slice(&mapped[start..start + (w * 4) as usize]);
    }
    out
}

// 03 §5 `spin`. The angle is not in the draw list: the vertex stage takes
// it from the clock in the uniforms. That is what lets the window redraw a
// spinning frame without repainting — so the property worth pinning is that
// the list does not depend on the clock at all, and that the picture still
// does.
fn spinning_bar() -> (Fx, DrawList) {
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
    // A 40 x 10 bar, so a quarter turn about its own centre is unmistakable.
    let bar = StyleRecord { bg: ColorRef::role(Role::AccentBase.id()), width: Dim::Px(40), height: Dim::Px(10), animation: 1, ..Default::default() };
    let mut fx = fixture(vec![col, bar], vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)], vec![], &[], 100.0, 100.0);
    let list = draw(&mut fx, 100, 100, 1.0);
    (fx, list)
}

#[test]
fn a_spinning_node_paints_the_same_list_whatever_the_clock() {
    let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
    let bar = StyleRecord { bg: ColorRef::role(Role::AccentBase.id()), width: Dim::Px(40), height: Dim::Px(10), animation: 1, ..Default::default() };
    let nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 0)];
    let at = |now: f32| {
        let mut fx = fixture(vec![col, bar], nodes.clone(), vec![], &[], 100.0, 100.0);
        paint(&mut Scene {
            session: &fx.session,
            layout: &fx.layout,
            theme: &fx.theme,
            text: &mut fx.text,
            atlas: &mut fx.atlas,
            images: &fx.images,
            scale: 1.0,
            size: (100, 100),
            focus: None,
            anims: &[],
            glides: &[],
            cache: &mut PaintCache::new(),
            editing: None,
            now,
            scrollbar_hot: None,
        })
    };
    let a = at(0.0);
    let b = at(0.31);
    let c = at(97.5);
    assert_eq!(a, b, "the clock must not reach the draw list");
    assert_eq!(a, c, "not even most of two minutes later");
    assert!(a.wants_frame, "a spinning node still asks for the next frame");
    let bar_quad = a.quads.iter().find(|q| q.rect[2] == 40.0).expect("the bar");
    assert!(u32::try_from(bar_quad.params[2] as i64).is_ok_and(|f| f & SPINNING != 0), "and it is marked for the vertex stage");
}

/// A list drawn again is already in the instance buffer: the serial says
/// so, and the renderer writes nothing. A list without one -- built by
/// hand, as here and in the snapshot tool -- is uploaded every time.
#[test]
fn the_same_serial_is_not_uploaded_twice() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let (mut fx, mut list) = spinning_bar();
    let target = r.offscreen(100, 100);
    let unnumbered = r.render_offscreen(&mut st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
    assert!(!unnumbered.upload_skipped && unnumbered.instance_bytes > 0, "no serial: uploaded, {unnumbered:?}");
    let unnumbered = r.render_offscreen(&mut st, &target, 0.1, &list, &mut fx.atlas, &mut fx.images);
    assert!(!unnumbered.upload_skipped, "and uploaded again, since nothing says it is the same list");
    list.serial = 42;
    let first = r.render_offscreen(&mut st, &target, 0.2, &list, &mut fx.atlas, &mut fx.images);
    assert!(!first.upload_skipped && first.instance_bytes == list.quads.len() * std::mem::size_of::<eui_render::Quad>(), "{first:?}");
    let again = r.render_offscreen(&mut st, &target, 0.3, &list, &mut fx.atlas, &mut fx.images);
    assert!(again.upload_skipped && again.instance_bytes == 0, "the same list: the clock moved, the buffer did not, {again:?}");
    assert_eq!((again.quads, again.runs, again.passes, again.submits), (first.quads, first.runs, 1, 1), "but it is drawn");
    list.serial = 43;
    let next = r.render_offscreen(&mut st, &target, 0.4, &list, &mut fx.atlas, &mut fx.images);
    assert!(!next.upload_skipped, "another serial is another list, alike or not");
    // Another session's buffer knows nothing of this one.
    let mut other = r.session();
    let elsewhere = r.render_offscreen(&mut other, &target, 0.5, &list, &mut fx.atlas, &mut fx.images);
    assert!(!elsewhere.upload_skipped);
}

#[test]
fn the_clock_turns_a_spinning_node_a_quarter_of_the_way_round() {
    let Some(mut r) = gpu() else { return };
    let mut st = r.session();
    let (mut fx, list) = spinning_bar();
    // One revolution per 1.2 s, so 0.3 s is a right angle: the horizontal
    // bar stands up, about a centre that does not move.
    let shot = |r: &mut Renderer, st: &mut SessionTextures, fx: &mut Fx, now: f64| {
        let target = r.offscreen(100, 100);
        r.render_offscreen(st, &target, now, &list, &mut fx.atlas, &mut fx.images);
        r.read_back(&target).unwrap()
    };
    let flat = shot(&mut r, &mut st, &mut fx, 0.0);
    let upright = shot(&mut r, &mut st, &mut fx, 0.3);
    let accent = rgba_of(fx.theme.color(Role::AccentBase));
    let surface = rgba_of(fx.theme.color(Role::SurfaceBase));
    // The bar is 40 x 10 at the origin, so its centre is (20, 5).
    let (cx, cy) = (20, 5);
    // Lying down: far along x is bar, far along y is not.
    assert!(close(pixel(&flat, 100, cx + 15, cy), accent, 2), "flat, along x: {:?}", pixel(&flat, 100, cx + 15, cy));
    assert!(close(pixel(&flat, 100, cx, cy + 15), surface, 2), "flat, along y: {:?}", pixel(&flat, 100, cx, cy + 15));
    // Stood up: the other way about, and the same list drew both.
    assert!(close(pixel(&upright, 100, cx, cy + 15), accent, 2), "upright, along y: {:?}", pixel(&upright, 100, cx, cy + 15));
    assert!(close(pixel(&upright, 100, cx + 15, cy), surface, 2), "upright, along x: {:?}", pixel(&upright, 100, cx + 15, cy));
    // The centre is on either way: a spin turns a node, it does not move it.
    assert!(close(pixel(&flat, 100, cx, cy), accent, 2));
    assert!(close(pixel(&upright, 100, cx, cy), accent, 2));
}

// Two windows on one device: what the renderer shares between them and what
// it must not. The glyph texture is the "must not" — a session uploads only
// the atlas rows that changed since its own last frame, so if both windows
// wrote into one texture the second would overwrite the first's glyphs and
// the first would then draw whatever the second had left there. It has a
// security half too (a worker picks its own uv, so one texture is one app
// reading another's text), but this is the half a test can see.
#[test]
fn a_second_window_does_not_draw_over_the_first_windows_glyphs() {
    let Some(mut r) = gpu() else { return };
    let mut one = r.session();
    let mut two = r.session();
    let scene = |s: &str| {
        let col = StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() };
        fixture(vec![col], vec![node(NodeKind::Box, 1, 1, 1), text(2, 0, s)], vec![], &[], 120.0, 40.0)
    };
    let shot = |r: &mut Renderer, st: &mut SessionTextures, fx: &mut Fx| {
        let list = draw(fx, 120, 40, 1.0);
        let target = r.offscreen(120, 40);
        r.render_offscreen(st, &target, 0.0, &list, &mut fx.atlas, &mut fx.images);
        r.read_back(&target).unwrap()
    };

    let mut a = scene("Wig");
    let mut b = scene("Mox");
    let first = shot(&mut r, &mut one, &mut a);
    // The other window packs different glyphs at the same places in its own
    // atlas, and uploads them.
    let _ = shot(&mut r, &mut two, &mut b);
    // Now the first window draws again. Its atlas has not changed, so it
    // uploads nothing: everything it draws comes from the texture it left
    // behind, which is exactly what a shared one would no longer hold.
    let again = shot(&mut r, &mut one, &mut a);
    assert_eq!(first, again, "the first window's text must survive the second window's frame");
    assert!(first.chunks(4).any(|p| p[0] != first[0] || p[1] != first[1] || p[2] != first[2]), "the frame has to have drawn some text to be worth comparing");
}

/// `overflow: clip` is honoured by anything that asks for it, not only by
/// the two node kinds that scroll.
///
/// It was in the protocol and read nowhere: the whole repository consulted
/// `style.overflow` once, in the layout, and only to ask whether it was
/// `Scroll`. A box that asked to be trimmed kept the clip of whatever
/// contained it, and its content painted over what followed — in the
/// gallery, one split panel spilling onto the card below it.
#[test]
fn a_box_that_asks_to_be_clipped_is_clipped() {
    let root = StyleRecord { display: Display::Column, ..Default::default() };
    let clipper = StyleRecord { display: Display::Column, width: Dim::Px(100), height: Dim::Px(40), overflow: Overflow::Clip, ..Default::default() };
    let tall = StyleRecord { bg: ColorRef::role(Role::AccentBase.id()), width: Dim::Px(100), height: Dim::Px(400), ..Default::default() };
    let nodes = vec![node(NodeKind::Box, 1, 1, 1), node(NodeKind::Box, 2, 2, 1), node(NodeKind::Box, 3, 3, 0)];
    let mut fx = fixture(vec![root, clipper, tall], nodes, vec![], &[], 200.0, 200.0);
    let list = draw(&mut fx, 200, 200, 1.0);

    // Clipping is a scissor, not a layout: the child keeps its 400 px. What
    // has to be true is that the run carrying it is scissored to the 40 px
    // its parent asked for, and not to the window.
    let child = list.quads.iter().position(|q| q.rect[3] > 100.0).expect("the tall child");
    let run = list.runs.iter().find(|r| child as u32 >= r.first && (child as u32) < r.first + r.count).expect("a run carrying it");
    let clip = list.clips.get(run.clip as usize).copied().expect("its clip");
    assert!(clip[3] <= 40, "the child must be trimmed to its parent's 40 px, not left at {} px", clip[3]);
}
