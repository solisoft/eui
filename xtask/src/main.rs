//! `cargo run --release -p xtask -- bench`
//!
//! Measures what `spec/10-budgets.md` promises and exits non-zero when a
//! budget is missed. Numbers are printed as a Markdown table so they can be
//! pasted into the spec verbatim — a budget that was never measured is a
//! slogan.

#![allow(clippy::arithmetic_side_effects, clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::time::{Duration, Instant};

use eui_client::worker::Backend;
use eui_client::{Driver, Input};
use eui_layout::{Env, Layout, Monospace, Size};
use eui_proto::*;
use eui_render::PaintCache;
use eui_theme::{Theme, Viewer};
use eui_tree::Session;

struct Row {
    what: &'static str,
    value: String,
    budget: &'static str,
    ok: bool,
}

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok()).map(|pages| pages * 4).unwrap_or(0)
}

/// The 10 000-row table as a batch, keyed rows, item_height prop.
fn table_batch(rows: u32) -> Batch {
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let list = StyleRecord { display: Display::Column, height: Dim::Px(400), ..Default::default() };
    let row = StyleRecord { display: Display::Row, gap: 4, padding: [1, 3, 1, 3], ..Default::default() };
    let cell = StyleRecord { width: Dim::Px(120), ..Default::default() };
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    t.nodes.push(FlatNode { kind: NodeKind::List, id: 2, style: 2, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: rows });
    t.props.push((1, Value::Int(22)));
    let mut id = 3;
    for i in 0..rows {
        t.nodes.push(FlatNode { kind: NodeKind::Box, id, style: 3, key: i + 1, text: None, props: (0, 0), handlers: (0, 0), child_count: 4 });
        id += 1;
        for c in 0..4 {
            let text = match c {
                0 => format!("FA-{i:05}"),
                1 => format!("Client {} SARL", i % 37),
                2 => {
                    if i % 3 == 0 {
                        "Paid".into()
                    } else {
                        "Open".into()
                    }
                }
                _ => format!("{} €", 100 + i * 37),
            };
            t.nodes.push(FlatNode { kind: NodeKind::Text, id, style: 4, key: 0, text: Some(TextRef::Inline(text)), props: (0, 0), handlers: (0, 0), child_count: 0 });
            id += 1;
        }
    }
    Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "item_height".into() },
            Op::DefStyle { id: 1, record: col },
            Op::DefStyle { id: 2, record: list },
            Op::DefStyle { id: 3, record: row },
            Op::DefStyle { id: 4, record: cell },
            Op::Mount(t),
        ],
    }
}

fn median(mut xs: Vec<Duration>) -> Duration {
    xs.sort();
    xs[xs.len() / 2]
}

fn bench() -> Vec<Row> {
    let mut rows = Vec::new();

    // --- wire: the counter mount, a click, an update ---------------------
    let mut counter = counter_server::Counter::default();
    let mount = Frame::Batch(counter.first()).encode().len();
    let click = Frame::Event(EventFrame { node: 4, event: EventKind::Click, name: 1, payload: Value::List(vec![Value::Float(21.0), Value::Float(9.0)]) }).encode().len();
    let update = Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 2, text: TextRef::Inline("1".into()) }] }).encode().len();
    rows.push(Row { what: "counter: mount batch", value: format!("{mount} B"), budget: "—", ok: true });
    rows.push(Row { what: "counter: one click", value: format!("{click} B"), budget: "< 40 B", ok: click < 40 });
    rows.push(Row { what: "counter: server answer", value: format!("{update} B"), budget: "< 40 B", ok: update < 40 });

    // --- wire: 10 000-row table -------------------------------------------
    let batch = table_batch(10_000);
    let bytes = Frame::Batch(batch.clone()).encode();
    // ~9 B of structure per node (spec/10 §2) plus the cell text itself,
    // which averages 6–7 B per node here and is the data, not overhead.
    rows.push(Row {
        what: "table-10k: mount batch, text included",
        value: format!("{:.1} KB ({:.1} B/node)", bytes.len() as f64 / 1024.0, bytes.len() as f64 / 50_002.0),
        budget: "< 18 B/node",
        ok: bytes.len() < 18 * 50_002,
    });

    // --- decoder: a ~4 KB batch, and the 10k one --------------------------
    let small = Frame::Batch(table_batch(60)).encode();
    let t: Vec<Duration> = (0..200)
        .map(|_| {
            let s = Instant::now();
            let _ = Frame::decode(&small).unwrap();
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: format!("decode {:.1} KB batch (median)", small.len() as f64 / 1024.0).leak(), value: format!("{d:?}"), budget: "< 50 µs", ok: d < Duration::from_micros(50) });
    let t: Vec<Duration> = (0..5)
        .map(|_| {
            let s = Instant::now();
            let _ = Frame::decode(&bytes).unwrap();
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "decode table-10k batch (median)", value: format!("{d:?}"), budget: "< 20 ms", ok: d < Duration::from_millis(20) });

    // --- session apply + layout + paint of 10k rows ------------------------
    let before = rss_kb();
    let mut session = Session::new();
    let s = Instant::now();
    session.apply(batch).unwrap();
    let applied = s.elapsed();
    rows.push(Row { what: "apply table-10k (50 002 nodes)", value: format!("{applied:?}"), budget: "< 30 ms", ok: applied < Duration::from_millis(30) });

    let theme = Theme::default().resolve(Viewer::default());
    let mut mono = Monospace::default();
    let mut layout = Layout::new();
    let t: Vec<Duration> = (0..5)
        .map(|_| {
            let s = Instant::now();
            layout.compute(&mut Env { session: &session, theme: &theme, text: &mut mono }, Size::new(800.0, 600.0));
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "layout table-10k, virtualised (median)", value: format!("{d:?}"), budget: "< 5 ms", ok: d < Duration::from_millis(5) });
    let t: Vec<Duration> = (0..5)
        .map(|_| {
            let s = Instant::now();
            session.clear_all_dirty();
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "clear_all_dirty, 50 002 nodes (median)", value: format!("{d:?}"), budget: "info", ok: true });
    let mut atlas = eui_render::Atlas::new();
    // One cache across the frames, and the bits a driver clears after
    // each paint cleared, so the row is a frame as a frame is painted:
    // what did not change is what it was.
    let mut cache = PaintCache::new();
    session.clear_all_dirty();
    let mut text = eui_text::TextEngine::new();
    layout.compute(&mut Env { session: &session, theme: &theme, text: &mut text }, Size::new(800.0, 600.0));
    let t: Vec<Duration> = (0..5)
        .map(|_| {
            let s = Instant::now();
            let images = eui_render::ImageAtlas::new();
            let _ = eui_render::paint(&mut eui_render::Scene {
                session: &session,
                layout: &layout,
                theme: &theme,
                text: &mut text,
                atlas: &mut atlas,
                images: &images,
                scale: 1.0,
                size: (800, 600),
                focus: None,
                anims: &[],
                movers: &[],
                glides: &[],
                cache: &mut cache,
                editing: None,
                now: 0.0,
                scrollbar_hot: None,
                scrollbars: &[],
                scenes_allowed: true,
            });
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "paint only, table-10k, layout done (median)", value: format!("{d:?}"), budget: "info", ok: true });
    let t: Vec<Duration> = (0..5)
        .map(|_| {
            let s = Instant::now();
            layout.compute(&mut Env { session: &session, theme: &theme, text: &mut text }, Size::new(800.0, 600.0));
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "layout only, real text engine, cached (median)", value: format!("{d:?}"), budget: "info", ok: true });
    let st = layout.stats();
    rows.push(Row {
        what: "layout work per frame",
        value: format!("{} measures, {} memo hits, {} list placements, {} rows measured, {} virtual", st.measures, st.memo_hits, st.list_placements, st.rows_measured, st.rows_virtual),
        budget: "info",
        ok: true,
    });
    let after = rss_kb();
    rows.push(Row {
        what: "RSS growth: session + layout, 10k rows",
        value: format!("{:.1} MB", (after.saturating_sub(before)) as f64 / 1024.0),
        budget: "< 45 MB",
        ok: after.saturating_sub(before) < 45 * 1024,
    });

    // --- the full client driver: real text engine, paint ------------------
    let before = rss_kb();
    let mut driver = Driver::new(800.0, 600.0, 1.0, 0);
    driver.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
    driver.handle_frame(Frame::Batch(table_batch(10_000)));
    let s = Instant::now();
    let list = driver.paint(800, 600);
    let first = s.elapsed();
    let t: Vec<Duration> = (0..10)
        .map(|_| {
            driver.input(Input::Wheel(0.0, 22.0));
            let s = Instant::now();
            let _ = driver.paint(800, 600);
            s.elapsed()
        })
        .collect();
    let scroll = median(t);
    let after = rss_kb();
    rows.push(Row { what: "driver: first paint of table-10k (shaping)", value: format!("{first:?}, {} quads", list.quads.len()), budget: "< 80 ms", ok: first < Duration::from_millis(80) });
    rows.push(Row { what: "driver: scroll step, layout + paint (median)", value: format!("{scroll:?}"), budget: "< 2 ms", ok: scroll < Duration::from_millis(2) });
    rows.extend(motion_rows(&mut driver));
    rows.extend(prose_rows());
    rows.push(Row {
        what: "driver RSS growth, table-10k with real text",
        value: format!("{:.1} MB", after.saturating_sub(before) as f64 / 1024.0),
        budget: "< 45 MB",
        ok: after.saturating_sub(before) < 45 * 1024,
    });
    rows.push(Row { what: "process RSS at the end", value: format!("{:.1} MB", rss_kb() as f64 / 1024.0), budget: "info", ok: true });

    rows.extend(through_a_worker(scroll));
    rows.extend(scene_rows());

    rows
}

/// What a scene costs the half of the client that is not the GPU.
///
/// The GPU's own share is not measured here and 10 §1 says so: it wants an
/// adapter, and this runs where there may be none. What *can* be measured is
/// the part the budget actually leans on — that verifying a module and
/// decoding a mesh are cheap enough to do in the worker on the frame they
/// arrive, and that a scene adds nothing per frame to the paint, because
/// everything that moves in it moves on the GPU from a clock.
fn scene_rows() -> Vec<Row> {
    let mut rows = Vec::new();

    // A module at the shape a real one has: the client's own contract, with
    // a bounded loop in it so the trip-count analysis is doing its work.
    let module = "
struct Scene { mvp: mat4x4<f32>, time: vec4<f32>, size: vec4<f32>, params: vec4<f32>, tint: vec4<f32> }
@group(0) @binding(0) var<uniform> u: Scene;
struct VIn { @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32> }
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) normal: vec3<f32> }
@vertex fn vs_main(in: VIn) -> VOut {
    var o: VOut;
    o.pos = u.mvp * vec4<f32>(in.pos, 1.0);
    o.normal = in.normal;
    return o;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    var acc: f32 = 0.0;
    for (var i: i32 = 0; i < 8; i = i + 1) { acc = acc + 0.1; }
    let light = normalize(vec3<f32>(0.35, 0.75, 0.55));
    let shade = 0.25 + 0.75 * max(dot(normalize(in.normal), light), 0.0);
    return vec4<f32>(u.tint.rgb * shade * acc, u.tint.a);
}
";
    let t: Vec<Duration> = (0..20)
        .map(|_| {
            let s = Instant::now();
            let _ = eui_shader::verify(module);
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "verify a scene shader (median)", value: format!("{d:?}"), budget: "< 2 ms", ok: d < Duration::from_millis(2) });

    // A mesh at the cap, checked index by index — which is the check no
    // driver makes, so its cost is the price of the promise.
    let verts = vec![eui_render::scene::Vertex { pos: [0.5, -0.5, 0.25], normal: [0.0, 0.0, 1.0], uv: [0.5, 0.5] }; 60_000];
    let idx: Vec<u32> = (0..180_000u32).map(|i| i % 60_000).collect();
    let bytes = eui_client::mesh::encode(&verts, &idx);
    let t: Vec<Duration> = (0..5)
        .map(|_| {
            let s = Instant::now();
            let _ = eui_client::mesh::decode(&bytes);
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "decode and check a 60k-vertex mesh (median)", value: format!("{d:?}, {:.1} KB", bytes.len() as f64 / 1024.0), budget: "< 20 ms", ok: d < Duration::from_millis(20) });

    // The line the whole design rests on: a scene on screen costs the paint
    // nothing per frame. The comparison is against the same tree with the
    // scene node taken out, so what is measured is the scene and not the
    // page around it.
    let paint_of = |with_scene: bool| -> Duration {
        let mut driver = Driver::new(800.0, 600.0, 1.0, eui_proto::caps::SCENE);
        driver.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
        let mut tree = Subtree::default();
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: u32::from(with_scene) });
        if with_scene {
            tree.nodes.push(FlatNode { kind: NodeKind::Scene, id: 2, style: 2, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 0 });
            tree.props.push((1, Value::Bool(true)));
        }
        driver.handle_frame(Frame::Batch(Batch {
            seq: 1,
            ops: vec![
                Op::DefAtom { id: 1, value: "playing".into() },
                Op::DefStyle { id: 1, record: StyleRecord { width: Dim::Px(800), height: Dim::Px(600), ..Default::default() } },
                Op::DefStyle { id: 2, record: StyleRecord { width: Dim::Px(400), height: Dim::Px(300), ..Default::default() } },
                Op::Mount(tree),
            ],
        }));
        let _ = driver.paint(800, 600);
        let t: Vec<Duration> = (0..20)
            .map(|_| {
                let s = Instant::now();
                let _ = driver.paint(800, 600);
                s.elapsed()
            })
            .collect();
        median(t)
    };
    let bare = paint_of(false);
    let scened = paint_of(true);
    let delta = scened.saturating_sub(bare);
    rows.push(Row {
        what: "paint a frame with a playing scene, over one without",
        value: format!("{delta:?} ({scened:?} against {bare:?})"),
        budget: "< 100 µs",
        ok: delta < Duration::from_micros(100),
    });
    rows
}

/// A page of prose in a plain scroller: sixty paragraphs, laid out in full
/// rather than virtualised, which is what a document looks like.
fn prose_batch() -> Batch {
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let page = StyleRecord { display: Display::Column, height: Dim::Px(500), gap: 8, padding: [12; 4], ..Default::default() };
    let para = StyleRecord { ..Default::default() };
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    t.nodes.push(FlatNode { kind: NodeKind::Scroll, id: 2, style: 2, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 60 });
    let words = ["the", "layout", "of", "a", "page", "is", "a", "function", "of", "its", "words", "and", "their", "widths", "and", "nothing", "else", "moves"];
    for i in 0..60u32 {
        let text: Vec<&str> = (0..18).map(|k| words[(k + i as usize) % words.len()]).collect();
        t.nodes.push(FlatNode { kind: NodeKind::Text, id: 3 + i, style: 3, key: 0, text: Some(TextRef::Inline(text.join(" "))), props: (0, 0), handlers: (0, 0), child_count: 0 });
    }
    Batch { seq: 1, ops: vec![Op::DefStyle { id: 1, record: col }, Op::DefStyle { id: 2, record: page }, Op::DefStyle { id: 3, record: para }, Op::Mount(t)] }
}

/// A wheel step through the prose page: the layout of a scroll that is
/// not virtualised, and the paint of words that did not change.
fn prose_rows() -> Vec<Row> {
    let mut rows = Vec::new();
    let mut driver = Driver::new(800.0, 600.0, 1.0, 0);
    driver.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
    driver.handle_frame(Frame::Batch(prose_batch()));
    let s = Instant::now();
    let list = driver.paint(800, 600);
    let first = s.elapsed();
    rows.push(Row { what: "driver: first paint of the prose page", value: format!("{first:?}, {} quads", list.quads.len()), budget: "< 40 ms", ok: first < Duration::from_millis(40) });
    driver.input(Input::PointerMove(100.0, 100.0));
    let t: Vec<Duration> = (0..10)
        .map(|_| {
            driver.input(Input::Wheel(0.0, 40.0));
            let s = Instant::now();
            let _ = driver.paint(800, 600);
            s.elapsed()
        })
        .collect();
    let d = median(t);
    let st = driver.layout().stats();
    rows.push(Row { what: "driver: scroll step, prose page (median)", value: format!("{d:?}, {} measures on the last", st.measures), budget: "< 2 ms", ok: d < Duration::from_millis(2) });
    rows
}

/// The frames an animation costs the driver, on the table above (10 §1).
///
/// Each is measured as the window would ask for it: a real paint, then
/// paints at the animation's cadence with nothing else reaching the
/// driver. What the vertex stage animates from the clock is the last list
/// handed back; what is still interpolated here is a walk of the tree.
fn motion_rows(driver: &mut Driver) -> Vec<Row> {
    let mut rows = Vec::new();
    let t0 = Instant::now();
    driver.tick(t0);
    let _ = driver.paint(800, 600);
    // Nothing changed: an expose, or the chrome beside an animating tab.
    let t: Vec<Duration> = (1..=10)
        .map(|i| {
            driver.tick(t0 + Duration::from_millis(16 * i));
            let s = Instant::now();
            let _ = driver.paint(800, 600);
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "driver: repeated frame, nothing changed (median)", value: format!("{d:?}"), budget: "< 50 µs", ok: d < Duration::from_micros(50) });
    // A pointer over the table: the hit test through ten thousand rows,
    // and the hover's paint.
    let t: Vec<Duration> = (0..10)
        .map(|i| {
            let s = Instant::now();
            driver.input(Input::PointerMove(100.0, 100.0 + i as f32));
            let _ = driver.paint(800, 600);
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "driver: hover frame, table-10k (median)", value: format!("{d:?}"), budget: "< 0.3 ms", ok: d < Duration::from_micros(300) });
    // A colour transition on one row: the first paint starts it, the
    // frames after it are what the transition costs.
    let fade = StyleRecord { display: Display::Row, gap: 4, padding: [1, 3, 1, 3], bg: ColorRef::role(eui_theme::Role::AccentBase.id()), transition: 3, ..Default::default() };
    driver.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::DefStyle { id: 5, record: fade }, Op::SetStyle { node: 3, style: 5 }] }));
    let t1 = t0 + Duration::from_millis(200);
    driver.tick(t1);
    let _ = driver.paint(800, 600);
    let laid = driver.relayouts();
    let t: Vec<Duration> = (1..=10)
        .map(|i| {
            driver.tick(t1 + Duration::from_millis(16 * i));
            let s = Instant::now();
            let _ = driver.paint(800, 600);
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "driver: transition frame (median)", value: format!("{d:?}, {} relayouts", driver.relayouts() - laid), budget: "< 0.2 ms", ok: d < Duration::from_micros(200) });
    // A page change: one page released wearing `exit` and another grafted
    // wearing `enter`, which is what a keyed child list reports when the
    // page a navigator is on changes (03 §5.1). Two pages are on screen for
    // the whole of it, and neither is laid out again — the one arriving was
    // laid out once where it lands, and the one leaving is a picture.
    let page = StyleRecord {
        display: Display::Column,
        width: Dim::Percent(10000),
        height: Dim::Percent(10000),
        bg: ColorRef::role(eui_theme::Role::SurfaceBase.id()),
        animation: eui_proto::ANIMATION_ENTER | eui_proto::ANIMATION_EXIT,
        motion: eui_proto::Motion::Trailing,
        transition: 3,
        ..Default::default()
    };
    let page_tree = |id: u32| {
        let mut t = Subtree::default();
        t.nodes.push(FlatNode { kind: NodeKind::Box, id, style: 6, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
        t
    };
    let tp = Instant::now();
    driver.tick(tp);
    driver.handle_frame(Frame::Batch(Batch { seq: 3, ops: vec![Op::DefStyle { id: 6, record: page }, Op::InsertChild { parent: 1, index: 0, subtree: page_tree(900_001) }] }));
    driver.tick(tp + Duration::from_millis(400));
    let _ = driver.paint(800, 600);
    driver.handle_frame(Frame::Batch(Batch { seq: 4, ops: vec![Op::RemoveChild { parent: 1, index: 0, count: 1 }, Op::InsertChild { parent: 1, index: 0, subtree: page_tree(900_002) }] }));
    let tq = tp + Duration::from_millis(400);
    let _ = driver.paint(800, 600);
    let laid = driver.relayouts();
    let t: Vec<Duration> = (1..=5)
        .map(|i| {
            driver.tick(tq + Duration::from_millis(16 * i));
            let s = Instant::now();
            let _ = driver.paint(800, 600);
            s.elapsed()
        })
        .collect();
    let both = driver.paint(800, 600).quads.iter().filter(|q| q.rect[2] > 700.0).count();
    let d = median(t);
    let relaid = driver.relayouts() - laid;
    rows.push(Row {
        what: "driver: page transition frame (median of 5)",
        value: format!("{d:?}, {relaid} relayouts, {both} pages"),
        budget: "< 0.2 ms, no layout",
        ok: d < Duration::from_micros(200) && relaid == 0 && both == 2,
    });
    // A wheel notch glides for `motion[0]`, 100 ms: the frames of the
    // glide. An input stamps the driver's clock with the wall's, so the
    // ticks here are the wall's too.
    let t2 = Instant::now();
    driver.tick(t2);
    let _ = driver.paint(800, 600);
    driver.input(Input::PointerMove(100.0, 100.0));
    driver.input(Input::WheelStep(0.0, 3.0));
    let _ = driver.paint(800, 600);
    assert!(driver.animating(), "the notch glides");
    let laid = driver.relayouts();
    let t: Vec<Duration> = (1..=5)
        .map(|i| {
            driver.tick(t2 + Duration::from_millis(16 * i));
            let s = Instant::now();
            let _ = driver.paint(800, 600);
            s.elapsed()
        })
        .collect();
    assert!(driver.animating(), "and is still gliding after five frames");
    let d = median(t);
    // The layout was done once, at the landing; the frames of the glide
    // are the same list, slid by the vertex stage.
    let relaid = driver.relayouts() - laid;
    rows.push(Row { what: "driver: glide frame (median of 5)", value: format!("{d:?}, {relaid} relayouts"), budget: "< 0.2 ms, no layout", ok: d < Duration::from_micros(200) && relaid == 0 });
    // The chrome: a session like any other, painted every frame beside an
    // application that animates. At rest it must cost nothing.
    let mut chrome = eui_client::chrome::Chrome::new(800.0, 600.0, 1.0);
    let _ = chrome.paint(800, 600);
    let t: Vec<Duration> = (0..10)
        .map(|_| {
            let s = Instant::now();
            let _ = chrome.paint(800, 600);
            s.elapsed()
        })
        .collect();
    let d = median(t);
    rows.push(Row { what: "chrome: repeated paint, nothing changed (median)", value: format!("{d:?}"), budget: "< 50 µs", ok: d < Duration::from_micros(50) });
    rows
}

/// The `eui` binary beside this one: what a worker is started from.
fn worker_binary() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let candidate = exe.parent()?.join(if cfg!(windows) { "eui.exe" } else { "eui" });
    candidate.is_file().then_some(candidate)
}

/// The same 10 000 rows, scrolled through a real worker process (08 §10).
///
/// This is the configuration that ships: every input and every paint is a
/// round trip over a pipe, and the draw list comes back whole. `in_process`
/// is the paint measured above, so the difference is what the boundary
/// costs rather than what the layout does.
fn through_a_worker(in_process: Duration) -> Vec<Row> {
    let mut rows = Vec::new();
    let Some(bin) = worker_binary() else {
        rows.push(Row { what: "worker: the eui binary is not beside this one", value: "skipped".into(), budget: "info", ok: true });
        return rows;
    };
    let (mut backend, how) = Backend::open_with(bin, 800.0, 600.0, 1.0, 0);
    rows.push(Row { what: "worker: how the driver runs", value: how.split(':').next_back().unwrap_or("?").trim().to_string(), budget: "info", ok: true });
    if backend.traffic().is_none() {
        rows.push(Row { what: "worker: no worker started, nothing to measure", value: "skipped".into(), budget: "info", ok: true });
        return rows;
    }
    backend.frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }).encode());
    backend.frame(Frame::Batch(table_batch(10_000)).encode());
    let s = Instant::now();
    let (list, _) = backend.paint(800, 600);
    let first = s.elapsed();
    rows.push(Row { what: "worker: first paint of table-10k, over the pipe", value: format!("{first:?}, {} quads", list.quads.len()), budget: "< 80 ms", ok: first < Duration::from_millis(80) });

    let before = backend.traffic().unwrap_or_default();
    let mut inputs = Vec::new();
    let mut paints = Vec::new();
    for _ in 0..10 {
        let s = Instant::now();
        backend.input(Input::Wheel(0.0, 22.0));
        inputs.push(s.elapsed());
        let s = Instant::now();
        let _ = backend.paint(800, 600);
        paints.push(s.elapsed());
    }
    let after = backend.traffic().unwrap_or_default();
    let input = median(inputs);
    let paint = median(paints);
    let step = paint + input;
    rows.push(Row { what: "worker: scroll step, input + paint over the pipe (median)", value: format!("{step:?}"), budget: "< 2 ms", ok: step < Duration::from_millis(2) });
    rows.push(Row { what: "worker: of which the input round trip (median)", value: format!("{input:?}"), budget: "info", ok: true });
    rows.push(Row { what: "worker: what the boundary adds to a paint", value: format!("{:?}", paint.saturating_sub(in_process)), budget: "info", ok: true });
    rows.push(Row {
        what: "worker: bytes over the pipe per scroll step",
        value: format!("{:.0} B out, {:.1} KB back", (after.0 - before.0) as f64 / 10.0, (after.1 - before.1) as f64 / 10.0 / 1024.0),
        budget: "info",
        ok: true,
    });
    // At rest, a paint the window asks for again is answered on this side
    // of the pipe.
    let _ = backend.paint(800, 600);
    let before = backend.traffic().unwrap_or_default();
    let mut paints = Vec::new();
    for _ in 0..10 {
        let s = Instant::now();
        let _ = backend.paint(800, 600);
        paints.push(s.elapsed());
    }
    let after = backend.traffic().unwrap_or_default();
    let repeat = median(paints);
    rows.push(Row {
        what: "worker: repeated frame, nothing changed",
        value: format!("{repeat:?}, {} B over the pipe", (after.0 - before.0 + after.1 - before.1) / 10),
        budget: "< 50 µs, 0 B",
        ok: repeat < Duration::from_micros(50) && after == before,
    });
    rows
}

/// Spec 09: every conformance vector in the workspace, then — when
/// `EUI_SOLI_BIN` points at a Soli built with the `eui` feature — the end-
/// to-end suite against a real server.
/// Whether this machine has the standard library for `target`. Asked of
/// `rustc` rather than `rustup`, because the answer is a directory either
/// way and not everyone installs Rust through rustup.
/// The page's target, named once.
const TARGET: &str = "wasm32-unknown-unknown";

fn std_installed(target: &str) -> bool {
    let Ok(out) = std::process::Command::new("rustc").args(["--print", "target-libdir", "--target", target]).output() else {
        return false;
    };
    out.status.success() && std::path::Path::new(String::from_utf8_lossy(&out.stdout).trim()).is_dir()
}

fn conform() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    // Formatting first: it is the fastest check and the one most likely to
    // be the only thing wrong, so failing on it costs a second rather than
    // a full test run. `rustfmt.toml` at the root says what it means.
    let mut steps: Vec<Vec<&str>> = vec![vec!["fmt", "--all", "--check"], vec!["test", "--workspace"], vec!["clippy", "--all-targets", "--", "-D", "warnings"]];
    // The half of the client that has no platform in it must stay that way,
    // and the only way to know is to build it for one. A `cfg` that creeps
    // into the layout engine or the text shaper is caught here rather than
    // by whoever next picks up a phone.
    //
    // Both phones, because they fail differently: one would catch a `cfg`
    // written for Unix that Android happens to satisfy, the other one
    // written for Apple that a Mac satisfies and a device does not.
    //
    // Only the crates that touch no C: `ring` and `blake3` want a toolchain
    // for the target, which a machine may not have and CI does not. Skipped
    // rather than failed where the standard library is not installed — this
    // is a check that the code is portable, not a demand that everyone
    // carry two phone toolchains.
    // `eui-shader` belongs here for the same reason the rest do: a phone
    // that draws a scene has to verify its module, and the verifier carries
    // no platform of its own.
    const CORE: [&str; 14] = ["-p", "eui-proto", "-p", "eui-tree", "-p", "eui-theme", "-p", "eui-layout", "-p", "eui-text", "-p", "eui-vm", "-p", "eui-shader"];
    // The browser is the third of these, and the one that is not a machine.
    // It earns its place here for the same reason the phones do: the six
    // portable crates claim to carry no platform `cfg` at all, and a claim
    // checked on two targets is weaker than one checked on three.
    for target in ["aarch64-linux-android", "aarch64-apple-ios", TARGET] {
        if std_installed(target) {
            let mut args = vec!["check", "--target", target];
            args.extend_from_slice(&CORE);
            steps.push(args);
        } else {
            eprintln!("note: no {target} standard library — skipping that portable-core cross-check (`rustup target add {target}`)");
        }
    }
    if std::env::var_os("EUI_SOLI_BIN").is_some() {
        steps.push(vec!["test", "-p", "eui-client", "--test", "soli_e2e"]);
    } else {
        eprintln!("note: EUI_SOLI_BIN not set — skipping the Soli end-to-end suite (spec 09 §10)");
    }
    // Each step gets its own target directory, and it has to. `xtask`
    // depends on `eui-client`, so building *this* binary and then running a
    // workspace test from inside it are two resolutions of the same graph
    // sharing one `target/`: the second unifies features differently, and the
    // rlibs the first left behind are evicted under it. What that looks like
    // is a doctest handed `--extern rustls=…-caac25ec480b39c5.rlib` for a
    // file that no longer exists, and a gate that fails on a tree where
    // `cargo test --workspace` passes on its own. A directory of its own
    // costs a first build and buys a check that means what it says.
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/conform");
    for args in steps {
        eprintln!("conform: cargo {}", args.join(" "));
        let status = std::process::Command::new(&cargo).args(&args).env("CARGO_TARGET_DIR", &target).status().expect("cargo runs");
        if !status.success() {
            eprintln!("conform: FAILED at cargo {}", args.join(" "));
            std::process::exit(1);
        }
    }
    // The shape a phone builds in, on whatever machine this is.
    //
    // Neither phone has a subprocess, so `no_subprocess` turns on a body of
    // code that a desktop build never compiles — and a `cfg` nobody
    // compiles is a `cfg` nobody checks. This cost three red CI runs the
    // day a seam was written behind one: the module it called did not
    // exist, every desktop build was happy, and both phone builds fell over
    // on the name. A `check` here is seconds and says so at once.
    //
    // It is not a substitute for building for the targets — it has none of
    // their platform crates in it — it only closes the gap that a
    // conditional compiled nowhere leaves open.
    //
    // Its own target directory again, and a *different* one from the steps
    // above: these flags are part of a build's fingerprint, so sharing a
    // directory would have each run evict what the other left and turn a
    // seconds-long check into a full rebuild, twice.
    // The shape a page builds in.
    //
    // Not covered by the `no_subprocess` check below, and not covered by
    // the portable-core cross-check above: `eui-client` on `wasm32` turns
    // off four capabilities at once (`has_native_net`, `has_audio`,
    // `has_desktop_theme`, `has_pins`) and turns on a whole module —
    // `transport_web.rs` — that no other target compiles. A `cfg` nobody
    // compiles is a `cfg` nobody checks, which is the lesson the phone
    // check below was written down for; this is the same lesson on the one
    // target where the *replacement* is conditional too.
    //
    // Its own target directory, for the reason every step here has one.
    if std_installed(TARGET) {
        eprintln!("conform: cargo check -p eui-client --target {TARGET} --no-default-features");
        let page = std::process::Command::new(&cargo)
            .args(["check", "-p", "eui-client", "--target", TARGET, "--no-default-features"])
            .env("CARGO_TARGET_DIR", target.with_file_name("conform-page"))
            .status()
            .expect("cargo runs");
        if !page.success() {
            eprintln!("conform: FAILED at the page-shaped check");
            std::process::exit(1);
        }
    } else {
        eprintln!("note: no {TARGET} standard library — skipping the page-shaped check (`rustup target add {TARGET}`)");
    }

    eprintln!("conform: cargo check -p eui-client --all-targets (no_subprocess)");
    let phone = std::process::Command::new(&cargo)
        .args(["check", "-p", "eui-client", "--all-targets"])
        .env("CARGO_TARGET_DIR", target.with_file_name("conform-phone"))
        .env("RUSTFLAGS", "--cfg no_subprocess")
        .status()
        .expect("cargo runs");
    if !phone.success() {
        eprintln!("conform: FAILED at the phone-shaped check");
        std::process::exit(1);
    }
    println!("conform: every vector passed");
}

fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    if task == "conform" {
        conform();
        return;
    }
    if task != "bench" {
        // `bench` wants the release build of itself, because it measures
        // this crate's own work. `conform` only shells out, and this crate
        // links the whole native client for `bench` — so asking for
        // `--release` there costs a minute of LTO and buys nothing.
        //
        // The browser client is built by `xtask-web`, which is its own crate
        // precisely so that it does not link any of this: see the note at
        // the top of `xtask-web/src/main.rs`.
        eprintln!("usage: cargo run --release -p xtask -- bench");
        eprintln!("       cargo run -p xtask -- conform");
        eprintln!("       cargo run -p xtask-web            (the browser client)");
        std::process::exit(2);
    }
    if cfg!(debug_assertions) {
        eprintln!("note: debug build — numbers are not the budgets' numbers; use --release");
    }
    let rows = bench();
    println!("| Measure | Value | Budget | |");
    println!("|---|---:|---:|:-:|");
    let mut failed = 0;
    for r in &rows {
        println!("| {} | {} | {} | {} |", r.what, r.value, r.budget, if r.ok { "ok" } else { "**miss**" });
        if !r.ok {
            failed += 1;
        }
    }
    if failed > 0 && !cfg!(debug_assertions) {
        eprintln!("{failed} budget(s) missed");
        std::process::exit(1);
    }
}
