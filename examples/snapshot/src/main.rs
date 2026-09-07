//! `snapshot <out-dir>` — the counter, rendered off-screen at 2× in light and
//! dark, before and after clicks, as raw RGBA files plus a manifest line each.
//!
//! `snapshot <out-dir> --soli <session-url> <name> <w> <h>` — connect to a
//! running Soli, mount `<name>`'s component, fetch its assets, and render it
//! in light and dark. `EUI_ALLOW_INSECURE_LOOPBACK=1` for a `ws://` URL.

#![allow(clippy::arithmetic_side_effects, clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::time::Instant;

use eui_client::{Driver, Input};
use eui_proto::{Frame, ThemeMode, Welcome};
use eui_render::Renderer;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| ".".into());
    if args.get(2).map(String::as_str) == Some("--soli") {
        let url = args.get(3).expect("session url");
        let name = args.get(4).expect("name");
        let w: f32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(1000.0);
        let h: f32 = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(900.0);
        let scale: f32 = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(2.0);
        snapshot_soli(&out, url, name, w, h, scale);
        return;
    }
    let (w, h, scale) = (420.0f32, 260.0f32, 2.0f32);
    let (dw, dh) = ((w * scale) as u32, (h * scale) as u32);
    let mut renderer = Renderer::new_headless().expect("a GPU adapter");
    eprintln!("adapter: {}", renderer.adapter_name());

    for (name, mode, clicks) in [("light-0", ThemeMode::Light, 0), ("light-3", ThemeMode::Light, 3), ("dark-3", ThemeMode::Dark, 3)] {
        let mut driver = Driver::new(w, h, scale, 0);
        let mut counter = counter_server::Counter::default();
        driver.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
        let first = counter.first();
        let wire = Frame::Batch(first.clone()).encode().len();
        driver.handle_frame(Frame::Batch(first));
        driver.input(Input::Mode(mode));
        let _ = driver.paint(dw, dh);
        // Click "+" `clicks` times, the way the window would: press, release,
        // ship the event, apply the server's answer.
        let plus = driver.session().lookup(4).unwrap();
        let r = driver.layout().rect(plus).unwrap();
        driver.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
        for _ in 0..clicks {
            driver.input(Input::PointerDown(0));
            for f in driver.input(Input::PointerUp(0)) {
                if let Frame::Event(e) = f {
                    let reply = counter.handle(&e).expect("server accepts its own button");
                    driver.handle_frame(Frame::Batch(reply));
                }
            }
        }
        let list = driver.paint(dw, dh);
        let target = renderer.offscreen(dw, dh);
        let (atlas, images) = driver.atlases_mut();
        renderer.render_offscreen(&target, &list, atlas, images);
        let px = renderer.read_back(&target).expect("read back");
        std::fs::write(format!("{out}/{name}.rgba"), &px).unwrap();
        println!("{name} {dw} {dh} quads={} mount_bytes={wire}", list.quads.len());
    }
}


/// Render a component served by a running Soli, in both modes.
fn snapshot_soli(out: &str, url: &str, name: &str, w: f32, h: f32, scale: f32) {
    use eui_client::{connect, Incoming, Input};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let (dw, dh) = ((w * scale) as u32, (h * scale) as u32);
    let mut renderer = Renderer::new_headless().expect("a GPU adapter");
    for (mode_name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        let mut driver = Driver::new(w, h, scale, 0);
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let conn = connect(url, driver.hello().encode(), move || {
            let _ = wake_tx.send(());
        })
        .expect("connect");
        driver.input(Input::Mode(mode));
        // Pump until the tree is mounted and every asset it names has arrived.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut outstanding: usize = 0;
        loop {
            assert!(Instant::now() < deadline, "timed out");
            let _ = wake_rx.recv_timeout(Duration::from_millis(50));
            while let Ok(msg) = conn.rx.try_recv() {
                match msg {
                    Incoming::Message(b) => {
                        let frame = Frame::decode(&b).expect("frame");
                        for f in driver.handle_frame(frame) {
                            conn.tx.send(f.encode()).unwrap();
                        }
                    }
                    Incoming::Asset(hash, Ok(bytes)) => {
                        driver.asset_ready(hash, bytes);
                        outstanding = outstanding.saturating_sub(1);
                    }
                    Incoming::Asset(hash, Err(e)) => {
                        driver.asset_failed(hash, e);
                        outstanding = outstanding.saturating_sub(1);
                    }
                    Incoming::Closed(e) => panic!("closed: {e}"),
                }
            }
            for hash in driver.pending_assets() {
                conn.request_asset(hash);
                outstanding += 1;
            }
            if driver.session().root().is_some() && outstanding == 0 {
                // One more short wait for a straggling batch, then done.
                let _ = wake_rx.recv_timeout(Duration::from_millis(150));
                if conn.rx.try_recv().is_err() {
                    break;
                }
            }
        }
        let list = driver.paint(dw, dh);
        let target = renderer.offscreen(dw, dh);
        let (atlas, images) = driver.atlases_mut();
        renderer.render_offscreen(&target, &list, atlas, images);
        // SNAPSHOT_TIMING=1: how long a scrolled frame takes, five times.
        if std::env::var_os("SNAPSHOT_TIMING").is_some() {
            if let Some(root) = driver.session().root() {
                let scroller = driver.session().preorder(root).find(|ix| driver.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Scroll));
                if let Some(sc) = scroller {
                    let r = driver.layout().rect(sc).unwrap_or_default();
                    driver.input(Input::PointerMove(r.x + 10.0, r.y + 10.0));
                }
                for i in 0..5 {
                    let t = Instant::now();
                    let _ = driver.input(Input::Wheel(0.0, 20.0));
                    let list = driver.paint(dw, dh);
                    let painted = t.elapsed();
                    let (atlas, images) = driver.atlases_mut();
                    renderer.render_offscreen(&target, &list, atlas, images);
                    let _ = renderer.read_back(&target);
                    println!("frame {i}: layout+paint {:.2} ms, render+readback {:.2} ms, {} quads", painted.as_secs_f64() * 1e3, (t.elapsed() - painted).as_secs_f64() * 1e3, list.quads.len());
                }
            }
        }
        let px = renderer.read_back(&target).expect("read back");
        let file = format!("{name}-{mode_name}");
        std::fs::write(format!("{out}/{file}.rgba"), &px).unwrap();
        println!("{file} {dw} {dh} quads={} nodes={}", list.quads.len(), driver.session().live_nodes());
        if std::env::var_os("SNAPSHOT_DUMP").is_some() {
            if let Some(root) = driver.session().root() {
                dump(&driver, root, 0);
            }
        }
    }
}

/// `SNAPSHOT_DUMP=1`: one line per node, indented by depth — kind, id, rect
/// and the text, for reading a layout without a screen.
fn dump(driver: &Driver, ix: eui_tree::NodeIx, depth: usize) {
    let s = driver.session();
    let Some(node) = s.node(ix) else { return };
    let rect = driver.layout().rect(ix).map_or("absent".to_owned(), |r| format!("{:.0},{:.0} {:.0}x{:.0}", r.x, r.y, r.w, r.h));
    let text = s.text_of(ix).map_or(String::new(), |t| format!(" {t:?}"));
    println!("{:indent$}{:?}#{} {rect}{text}", "", node.kind, node.id, indent = depth * 2);
    for c in s.children(ix) {
        dump(driver, *c, depth + 1);
    }
}
