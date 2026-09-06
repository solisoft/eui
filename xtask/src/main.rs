//! `cargo run --release -p xtask -- bench`
//!
//! Measures what `spec/10-budgets.md` promises and exits non-zero when a
//! budget is missed. Numbers are printed as a Markdown table so they can be
//! pasted into the spec verbatim — a budget that was never measured is a
//! slogan.

#![allow(clippy::arithmetic_side_effects, clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::time::{Duration, Instant};

use eui_client::{Driver, Input};
use eui_layout::{Env, Layout, Monospace, Size};
use eui_proto::*;
use eui_theme::{Theme, Viewer};
use eui_tree::Session;

struct Row {
    what: &'static str,
    value: String,
    budget: &'static str,
    ok: bool,
}

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map(|pages| pages * 4)
        .unwrap_or(0)
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
                2 => if i % 3 == 0 { "Paid".into() } else { "Open".into() },
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
    rows.push(Row { what: "table-10k: mount batch, text included", value: format!("{:.1} KB ({:.1} B/node)", bytes.len() as f64 / 1024.0, bytes.len() as f64 / 50_002.0), budget: "< 18 B/node", ok: bytes.len() < 18 * 50_002 });

    // --- decoder: a ~4 KB batch, and the 10k one --------------------------
    let small = Frame::Batch(table_batch(60)).encode();
    let t: Vec<Duration> = (0..200).map(|_| { let s = Instant::now(); let _ = Frame::decode(&small).unwrap(); s.elapsed() }).collect();
    let d = median(t);
    rows.push(Row { what: format!("decode {:.1} KB batch (median)", small.len() as f64 / 1024.0).leak(), value: format!("{d:?}"), budget: "< 50 µs", ok: d < Duration::from_micros(50) });
    let t: Vec<Duration> = (0..5).map(|_| { let s = Instant::now(); let _ = Frame::decode(&bytes).unwrap(); s.elapsed() }).collect();
    let d = median(t);
    rows.push(Row { what: "decode table-10k batch (median)", value: format!("{d:?}"), budget: "< 20 ms", ok: d < Duration::from_millis(20) });

    // --- session apply + layout + paint of 10k rows ------------------------
    let before = rss_kb();
    let mut session = Session::new();
    let s = Instant::now();
    session.apply(&batch).unwrap();
    let applied = s.elapsed();
    rows.push(Row { what: "apply table-10k (50 002 nodes)", value: format!("{applied:?}"), budget: "< 30 ms", ok: applied < Duration::from_millis(30) });

    let theme = Theme::default().resolve(Viewer::default());
    let mut mono = Monospace::default();
    let mut layout = Layout::new();
    let t: Vec<Duration> = (0..5).map(|_| { let s = Instant::now(); layout.compute(&mut Env { session: &session, theme: &theme, text: &mut mono }, Size::new(800.0, 600.0)); s.elapsed() }).collect();
    let d = median(t);
    rows.push(Row { what: "layout table-10k, virtualised (median)", value: format!("{d:?}"), budget: "< 5 ms", ok: d < Duration::from_millis(5) });
    let t: Vec<Duration> = (0..5).map(|_| { let s = Instant::now(); session.clear_all_dirty(); s.elapsed() }).collect();
    let d = median(t);
    rows.push(Row { what: "clear_all_dirty, 50 002 nodes (median)", value: format!("{d:?}"), budget: "info", ok: true });
    let mut atlas = eui_render::Atlas::new();
    let mut text = eui_text::TextEngine::new();
    layout.compute(&mut Env { session: &session, theme: &theme, text: &mut text }, Size::new(800.0, 600.0));
    let t: Vec<Duration> = (0..5).map(|_| { let s = Instant::now(); let images = eui_render::ImageAtlas::new();
        let _ = eui_render::paint(&mut eui_render::Scene { session: &session, layout: &layout, theme: &theme, text: &mut text, atlas: &mut atlas, images: &images, scale: 1.0, size: (800, 600) }); s.elapsed() }).collect();
    let d = median(t);
    rows.push(Row { what: "paint only, table-10k, layout done (median)", value: format!("{d:?}"), budget: "info", ok: true });
    let t: Vec<Duration> = (0..5).map(|_| { let s = Instant::now(); layout.compute(&mut Env { session: &session, theme: &theme, text: &mut text }, Size::new(800.0, 600.0)); s.elapsed() }).collect();
    let d = median(t);
    rows.push(Row { what: "layout only, real text engine, cached (median)", value: format!("{d:?}"), budget: "info", ok: true });
    let st = layout.stats();
    rows.push(Row { what: "layout work per frame", value: format!("{} measures, {} memo hits, {} list placements, {} rows measured, {} virtual", st.measures, st.memo_hits, st.list_placements, st.rows_measured, st.rows_virtual), budget: "info", ok: true });
    let after = rss_kb();
    rows.push(Row { what: "RSS growth: session + layout, 10k rows", value: format!("{:.1} MB", (after.saturating_sub(before)) as f64 / 1024.0), budget: "< 45 MB", ok: after.saturating_sub(before) < 45 * 1024 });

    // --- the full client driver: real text engine, paint ------------------
    let before = rss_kb();
    let mut driver = Driver::new(800.0, 600.0, 1.0, 0);
    driver.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    driver.handle_frame(Frame::Batch(table_batch(10_000)));
    let s = Instant::now();
    let list = driver.paint(800, 600);
    let first = s.elapsed();
    let t: Vec<Duration> = (0..10).map(|_| { driver.input(Input::Wheel(0.0, 22.0)); let s = Instant::now(); let _ = driver.paint(800, 600); s.elapsed() }).collect();
    let scroll = median(t);
    let after = rss_kb();
    rows.push(Row { what: "driver: first paint of table-10k (shaping)", value: format!("{first:?}, {} quads", list.quads.len()), budget: "< 80 ms", ok: first < Duration::from_millis(80) });
    rows.push(Row { what: "driver: scroll step, layout + paint (median)", value: format!("{scroll:?}"), budget: "< 2 ms", ok: scroll < Duration::from_millis(2) });
    rows.push(Row { what: "driver RSS growth, table-10k with real text", value: format!("{:.1} MB", after.saturating_sub(before) as f64 / 1024.0), budget: "< 45 MB", ok: after.saturating_sub(before) < 45 * 1024 });
    rows.push(Row { what: "process RSS at the end", value: format!("{:.1} MB", rss_kb() as f64 / 1024.0), budget: "info", ok: true });

    rows
}

fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    if task != "bench" {
        eprintln!("usage: cargo run --release -p xtask -- bench");
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
