//! `snapshot <out-dir>` — the counter, rendered off-screen at 2× in light and
//! dark, before and after clicks, as raw RGBA files plus a manifest line each.
//!
//! `snapshot <out-dir> --soli <session-url> <name> <w> <h>` — connect to a
//! running Soli, mount `<name>`'s component, fetch its assets, and render it
//! in light and dark. `EUI_ALLOW_INSECURE_LOOPBACK=1` for a `ws://` URL.
//! `SNAPSHOT_CLICK`, `SNAPSHOT_SCROLL`, `SNAPSHOT_HOVER`, `SNAPSHOT_KEYS`,
//! `SNAPSHOT_TEXT` and `SNAPSHOT_COVERED` drive it first, so a pane two
//! clicks in, a card below the fold, a state that only exists under the
//! pointer, a field that has been typed into, or a page with a phone's
//! keyboard standing on it can be looked at without a screen.

#![allow(clippy::arithmetic_side_effects, clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use eui_client::driver::SceneAsset;
use eui_client::{Driver, Input};
use eui_proto::{Frame, ThemeMode, Welcome};
use eui_render::{Renderer, SessionTextures};

/// Hand the renderer whatever a scene is waiting for.
///
/// A real session does this in the window process, because that is where the
/// GPU is and the driver is somewhere else (`app.rs`). This tool has no
/// window and no worker: the driver is right here. Both paths call the same
/// two renderer methods, which is why they are methods and not something the
/// worker boundary does privately -- a scene that could only be uploaded
/// through a pipe could not be looked at without a screen, and this is the
/// only pixel harness in the repository.
fn load_scene_assets(driver: &mut Driver, renderer: &mut Renderer, textures: &mut SessionTextures) {
    for (hash, asset) in driver.take_scene_assets() {
        match asset {
            SceneAsset::Mesh(m) => renderer.load_mesh(textures, hash, &m.vertices, &m.indices),
            SceneAsset::Shader(src) => {
                if let Err(e) = renderer.load_shader(hash, &src) {
                    eprintln!("snapshot: {e}");
                }
            }
        }
    }
}

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
    let mut textures = renderer.session();
    eprintln!("adapter: {}", renderer.adapter_name());

    for (name, mode, clicks) in [("light-0", ThemeMode::Light, 0), ("light-3", ThemeMode::Light, 3), ("dark-3", ThemeMode::Dark, 3)] {
        let mut driver = Driver::new(w, h, scale, 0);
        let mut counter = counter_server::Counter::default();
        driver.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
        load_scene_assets(&mut driver, &mut renderer, &mut textures);
        let (atlas, images) = driver.atlases_mut();
        renderer.render_offscreen(&mut textures, &target, 0.0, &list, atlas, images);
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

    // SNAPSHOT_COOKIE=soli_desktop=… — a desktop artifact run with
    // SOLI_DESKTOP_NO_WINDOW=1 prints its session URL and the cookie its
    // loopback gate wants; without it the upgrade is refused, so a bundled
    // app could not be looked at at all.
    let cookie = std::env::var("SNAPSHOT_COOKIE").ok();
    let (dw, dh) = ((w * scale) as u32, (h * scale) as u32);
    let mut renderer = Renderer::new_headless().expect("a GPU adapter");
    let mut textures = renderer.session();
    for (mode_name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        let mut driver = Driver::new(w, h, scale, 0);
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let conn = connect(url, driver.hello().encode(), cookie.clone(), false, move || {
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
        // SNAPSHOT_CLICK="Some label;Another" — click each in turn before
        // rendering, so a pane that is two clicks in can be looked at.
        //
        // This is the only place the variable is read. A second loop further
        // down used to read it again, so every `x,y` step was clicked twice —
        // the second one landing wherever the first one's answer had moved
        // things to. A panel a click opened and the repeat then shut
        // photographed as a panel that never opened at all.
        // The label is matched on a node's text; the click goes to the
        // nearest ancestor that has a handler for it, the way an event does.
        //
        // An entry that reads "x,y" is clicked at that point in logical px
        // instead. A field carries its *value* as its text, not a label, and
        // what it answers a press with — focus, a caret, a style a local
        // handler put there — is exactly what a picture is wanted for; there
        // is no label to name it by, and its value is a poor one.
        if let Ok(labels) = std::env::var("SNAPSHOT_CLICK") {
            for label in labels.split(';').map(str::trim).filter(|l| !l.is_empty()) {
                let _ = driver.paint(dw, dh);
                // An entry beginning with `+` is typed into whatever holds
                // focus rather than clicked, so one variable can drive a
                // sequence that alternates — click a field, type enough to
                // narrow a panel, click what the panel then offers.
                if let Some(typed) = label.strip_prefix('+') {
                    for f in driver.input(Input::Text(typed.to_owned())) {
                        conn.tx.send(f.encode()).unwrap();
                    }
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while Instant::now() < deadline {
                        let _ = wake_rx.recv_timeout(Duration::from_millis(50));
                        let mut answered = false;
                        while let Ok(Incoming::Message(b)) = conn.rx.try_recv() {
                            answered = true;
                            for f in driver.handle_frame(Frame::decode(&b).expect("frame")) {
                                conn.tx.send(f.encode()).unwrap();
                            }
                        }
                        // 06 §2's `change` is owed 300 ms after the last
                        // keystroke and fires on a *tick*; a loop that only
                        // painted would wait for an answer that was never
                        // going to be asked for.
                        driver.tick(Instant::now());
                        let _ = driver.paint(dw, dh);
                        // And a `change` a paint produced leaves through the
                        // pending queue, not through `input`'s return.
                        for f in driver.take_pending() {
                            conn.tx.send(f.encode()).unwrap();
                        }
                        if answered {
                            break;
                        }
                    }
                    continue;
                }
                // An entry reading `s<dy>` wheels the view down first: what
                // is worth photographing is often below the fold, and a
                // click is aimed at where a thing is *on screen*.
                if let Some(dy) = label.strip_prefix('s').and_then(|d| d.trim().parse::<f32>().ok()) {
                    driver.input(Input::PointerMove(w / 2.0, h / 2.0));
                    driver.input(Input::Wheel(0.0, dy));
                    let mut clock = Instant::now();
                    for _ in 0..40 {
                        clock += Duration::from_millis(16);
                        driver.tick(clock);
                        let _ = driver.paint(dw, dh);
                    }
                    continue;
                }
                let point = label.split_once(',').and_then(|(a, b)| Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?)));
                let at = match point {
                    Some(at) => Some(at),
                    None => {
                        let found = driver.session().root().and_then(|root| driver.session().preorder(root).find(|ix| driver.session().text_of(*ix) == Some(label)));
                        let Some(mut ix) = found else {
                            eprintln!("snapshot: nothing reads {label:?}");
                            continue;
                        };
                        while driver.session().handler(ix, eui_proto::EventKind::Click).is_none() {
                            let Some(up) = driver.session().node(ix).map(|n| n.parent) else {
                                break;
                            };
                            if up == ix {
                                break;
                            }
                            ix = up;
                        }
                        driver.layout().rect(ix).map(|r| (r.x + r.w / 2.0, r.y + r.h / 2.0))
                    }
                };
                let Some((px, py)) = at else {
                    continue;
                };
                // Every one of the three, not just the last. A press is
                // what moves focus, so the `blur` of the field being left
                // and the `focus` of the thing being pressed both leave on
                // `PointerDown` — and a tool that threw those away made a
                // field that opens its panel on focus look like a field
                // that does not.
                for i in [Input::PointerMove(px, py), Input::PointerDown(0), Input::PointerUp(0)] {
                    for f in driver.input(i) {
                        conn.tx.send(f.encode()).unwrap();
                    }
                }
                // The answer, and whatever pictures it names.
                let deadline = Instant::now() + Duration::from_secs(20);
                let mut outstanding: usize = 0;
                let mut answered = false;
                loop {
                    if Instant::now() > deadline {
                        eprintln!("snapshot: no answer to {label:?}");
                        break;
                    }
                    let _ = wake_rx.recv_timeout(Duration::from_millis(50));
                    while let Ok(msg) = conn.rx.try_recv() {
                        match msg {
                            Incoming::Message(b) => {
                                answered = true;
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
                    driver.tick(Instant::now());
                    let _ = driver.paint(dw, dh);
                    // Everything a paint decided to say: an idle `change`,
                    // and the `focus` of a field the answering batch asked
                    // for with `autofocus`. Both leave through the queue and
                    // not through `input`, and a tool that never drained it
                    // stopped one round trip short of what a window does.
                    for f in driver.take_pending() {
                        conn.tx.send(f.encode()).unwrap();
                        answered = false;
                    }
                    for hash in driver.pending_assets() {
                        conn.request_asset(hash);
                        outstanding += 1;
                    }
                    if answered && outstanding == 0 {
                        let _ = wake_rx.recv_timeout(Duration::from_millis(150));
                        if conn.rx.try_recv().is_err() {
                            break;
                        }
                    }
                }
            }
        }
        // SNAPSHOT_NAV="Orders" — click a label that changes the page, then
        // time every frame of the transition it starts: the one that applies
        // the server's batch and lays the new page out, and the ones after
        // it, which should cost nothing because the vertex stage is carrying
        // both pages (03 §5). A number that is not near zero on the frames
        // after the first is a transition the CPU is drawing.
        if let Ok(label) = std::env::var("SNAPSHOT_NAV") {
            // Rects come from a paint; without one the layout has nothing.
            let _ = driver.paint(dw, dh);
            // The first one with a box: a label also appears in the drawer,
            // which is not laid out while it is closed.
            let root = driver.session().root().expect("a tree");
            let hits: Vec<_> = driver.session().preorder(root).filter(|ix| driver.session().text_of(*ix) == Some(label.as_str())).collect();
            let mut r = None;
            for hit in hits {
                let mut ix = hit;
                while driver.session().handler(ix, eui_proto::EventKind::Click).is_none() {
                    let Some(up) = driver.session().node(ix).map(|n| n.parent) else { break };
                    if up == ix {
                        break;
                    }
                    ix = up;
                }
                r = driver.layout().rect(ix).filter(|r| r.w > 0.0 && r.h > 0.0);
                if r.is_some() {
                    break;
                }
            }
            let r = r.expect("a box");
            driver.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
            driver.input(Input::PointerDown(0));
            for f in driver.input(Input::PointerUp(0)) {
                conn.tx.send(f.encode()).unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(20);
            let mut batches = 0u32;
            while Instant::now() < deadline {
                let _ = wake_rx.recv_timeout(Duration::from_millis(50));
                let mut answered = false;
                while let Ok(Incoming::Message(b)) = conn.rx.try_recv() {
                    answered = true;
                    batches += 1;
                    for f in driver.handle_frame(Frame::decode(&b).expect("frame")) {
                        conn.tx.send(f.encode()).unwrap();
                    }
                }
                if answered {
                    break;
                }
            }
            let laid = driver.relayouts();
            let target = renderer.offscreen(dw, dh);
            for i in 0..14 {
                driver.tick(Instant::now());
                let t = Instant::now();
                let list = driver.paint(dw, dh);
                let painted = t.elapsed();
                load_scene_assets(&mut driver, &mut renderer, &mut textures);
                let (atlas, images) = driver.atlases_mut();
                renderer.render_offscreen(&mut textures, &target, 0.0, &list, atlas, images);
                let drawn = t.elapsed() - painted;
                println!(
                    "nav {label:?} frame {i}: layout+paint {:.3} ms, render {:.3} ms, {} quads, {} relayouts, {} batches",
                    painted.as_secs_f64() * 1e3,
                    drawn.as_secs_f64() * 1e3,
                    list.quads.len(),
                    driver.relayouts() - laid,
                    batches
                );
                std::thread::sleep(Duration::from_millis(16).saturating_sub(t.elapsed()));
            }
        }
        // SNAPSHOT_SCROLL=<px> — wheel the page down before the last paint,
        // so a card below the fold can be looked at at all.
        if let Some(dy) = std::env::var("SNAPSHOT_SCROLL").ok().and_then(|v| v.trim().parse::<f32>().ok()) {
            let _ = driver.paint(dw, dh);
            driver.input(Input::PointerMove(w / 2.0, h / 2.0));
            driver.input(Input::Wheel(0.0, dy));
            // A scroll glides: it lands on the clock, not on the wheel
            // event, so the frames it wants are run here rather than
            // painting the page half way there.
            let mut clock = Instant::now();
            for _ in 0..60 {
                clock += Duration::from_millis(16);
                driver.tick(clock);
                let _ = driver.paint(dw, dh);
            }
        }
        // SNAPSHOT_SETTLE=<ms> — let time pass before the last paint, ticking
        // the driver's clock and applying whatever arrives.
        //
        // Without this the tool paints but never ticks, so anything the clock
        // drives is invisible to it: a `wake` never fires, a transition never
        // eases, a glide never lands. An application that shows a loader until
        // its first wake would be photographed mid-load forever.
        if let Some(ms) = std::env::var("SNAPSHOT_SETTLE").ok().and_then(|v| v.trim().parse::<u64>().ok()) {
            let until = Instant::now() + Duration::from_millis(ms);
            while Instant::now() < until {
                let _ = wake_rx.recv_timeout(Duration::from_millis(16));
                while let Ok(msg) = conn.rx.try_recv() {
                    match msg {
                        Incoming::Message(b) => {
                            let frame = Frame::decode(&b).expect("frame");
                            for f in driver.handle_frame(frame) {
                                conn.tx.send(f.encode()).unwrap();
                            }
                        }
                        Incoming::Asset(hash, Ok(bytes)) => driver.asset_ready(hash, bytes),
                        Incoming::Asset(hash, Err(e)) => driver.asset_failed(hash, e),
                        Incoming::Closed(e) => panic!("closed: {e}"),
                    }
                }
                driver.tick(Instant::now());
                let _ = driver.paint(dw, dh);
                // Painting is what raises a wake, a time update or a viewport
                // frame; `take_pending` is what lets them leave. A tool that
                // paints and never drains generates them and sends none.
                for f in driver.take_pending() {
                    conn.tx.send(f.encode()).unwrap();
                }
                for hash in driver.pending_assets() {
                    conn.request_asset(hash);
                }
            }
        }
        // SNAPSHOT_KEYS="Tab;Escape" — keys pressed in order before the last
        // paint, so a focus ring, a trapped Tab or a surface that closes on
        // Escape can be looked at without a keyboard. Each press waits for
        // whatever the server sends back, the way a click does.
        if let Ok(keys) = std::env::var("SNAPSHOT_KEYS") {
            for key in keys.split(';').map(str::trim).filter(|k| !k.is_empty()) {
                let _ = driver.paint(dw, dh);
                for f in driver.input(Input::Key { key: key.to_owned(), modifiers: 0, down: true }) {
                    conn.tx.send(f.encode()).unwrap();
                }
                for f in driver.input(Input::Key { key: key.to_owned(), modifiers: 0, down: false }) {
                    conn.tx.send(f.encode()).unwrap();
                }
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if Instant::now() > deadline {
                        break;
                    }
                    let _ = wake_rx.recv_timeout(Duration::from_millis(50));
                    let mut answered = false;
                    while let Ok(msg) = conn.rx.try_recv() {
                        if let Incoming::Message(b) = msg {
                            answered = true;
                            let frame = Frame::decode(&b).expect("frame");
                            for f in driver.handle_frame(frame) {
                                conn.tx.send(f.encode()).unwrap();
                            }
                        }
                    }
                    let _ = driver.paint(dw, dh);
                    if answered {
                        break;
                    }
                }
            }
        }
        // SNAPSHOT_COVERED=<px> — a soft keyboard standing on the bottom of
        // the window, which is what a phone does the moment a field takes
        // focus and what no desktop can be made to do. Sent after the keys,
        // so `SNAPSHOT_KEYS=Tab SNAPSHOT_COVERED=340` is "the field a Tab
        // landed on, with a keyboard over it" — the case being looked at.
        if let Some(px) = std::env::var("SNAPSHOT_COVERED").ok().and_then(|v| v.trim().parse::<f32>().ok()) {
            for f in driver.input(Input::Covered(px)) {
                conn.tx.send(f.encode()).unwrap();
            }
            let _ = driver.paint(dw, dh);
        }
        // SNAPSHOT_TEXT="1;2;3" — committed text, typed one entry at a time
        // into whatever holds focus, each waiting for the server's answer.
        // A field that only looks right once something has been typed into
        // it cannot be photographed any other way.
        if let Ok(runs) = std::env::var("SNAPSHOT_TEXT") {
            for run in runs.split(';').filter(|r| !r.is_empty()) {
                let _ = driver.paint(dw, dh);
                for f in driver.input(Input::Text(run.to_owned())) {
                    conn.tx.send(f.encode()).unwrap();
                }
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if Instant::now() > deadline {
                        break;
                    }
                    let _ = wake_rx.recv_timeout(Duration::from_millis(50));
                    let mut answered = false;
                    while let Ok(msg) = conn.rx.try_recv() {
                        if let Incoming::Message(b) = msg {
                            answered = true;
                            let frame = Frame::decode(&b).expect("frame");
                            for f in driver.handle_frame(frame) {
                                conn.tx.send(f.encode()).unwrap();
                            }
                        }
                    }
                    driver.tick(Instant::now());
                    let _ = driver.paint(dw, dh);
                    for f in driver.take_pending() {
                        conn.tx.send(f.encode()).unwrap();
                    }
                    if answered {
                        break;
                    }
                }
            }
        }
        // SNAPSHOT_HOVER="x,y" — logical px, where the pointer is left
        // standing before the last paint, so a state that only exists under
        // it (a chart's band, a button) can be looked at. Hover settles at
        // paint, and a transition needs a clock: the tick after the wait is
        // what the fade would have had on a screen.
        if let Some((x, y)) = std::env::var("SNAPSHOT_HOVER").ok().and_then(|at| {
            let (x, y) = at.split_once(',')?;
            Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))
        }) {
            let _ = driver.paint(dw, dh);
            for f in driver.input(Input::PointerMove(x, y)) {
                conn.tx.send(f.encode()).unwrap();
            }
            let _ = driver.paint(dw, dh);
            std::thread::sleep(Duration::from_millis(200));
            driver.tick(Instant::now());
        }
        let list = driver.paint(dw, dh);
        let target = renderer.offscreen(dw, dh);
        // SNAPSHOT_AGE=<ms> — draw the list as it will look this long after
        // the paint that produced it. Everything the vertex stage animates
        // from the clock (03 §5) is then visible off-screen: a transition
        // part way, a page mid-slide, a spinner at an angle.
        let age = std::env::var("SNAPSHOT_AGE").ok().and_then(|v| v.trim().parse::<f32>().ok()).map_or(0.0, |ms| ms / 1000.0);
        load_scene_assets(&mut driver, &mut renderer, &mut textures);
        let (atlas, images) = driver.atlases_mut();
        renderer.render_offscreen_at(&mut textures, &target, 0.0, age, &list, atlas, images);
        // SNAPSHOT_TIMING=1: how long a scrolled frame takes, five times.
        if std::env::var_os("SNAPSHOT_TIMING").is_some() {
            if let Some(root) = driver.session().root() {
                let scroller = driver.session().preorder(root).find(|ix| matches!(driver.session().node(*ix).map(|n| n.kind), Some(eui_proto::NodeKind::Scroll | eui_proto::NodeKind::List)));
                if let Some(sc) = scroller {
                    let r = driver.layout().rect(sc).unwrap_or_default();
                    driver.input(Input::PointerMove(r.x + 10.0, r.y + 10.0));
                }
                for i in 0..5 {
                    let t = Instant::now();
                    let _ = driver.input(Input::Wheel(0.0, 20.0));
                    let list = driver.paint(dw, dh);
                    let painted = t.elapsed();
                    load_scene_assets(&mut driver, &mut renderer, &mut textures);
                    let (atlas, images) = driver.atlases_mut();
                    renderer.render_offscreen(&mut textures, &target, 0.0, &list, atlas, images);
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
        // SNAPSHOT_A11Y=1 — the tree a screen reader is handed, as text.
        // A screen reader is the only honest test of what an application
        // says about itself; this is the one that can run without one.
        if std::env::var_os("SNAPSHOT_A11Y").is_some() {
            for n in &driver.access_snapshot().nodes {
                let st = &n.state;
                let mut says: Vec<String> = Vec::new();
                if let Some(c) = st.checked {
                    says.push(format!("checked={c:?}"));
                }
                if let Some(v) = st.expanded {
                    says.push(format!("expanded={v}"));
                }
                if let Some(v) = st.selected {
                    says.push(format!("selected={v}"));
                }
                for (on, word) in [(st.disabled, "disabled"), (st.read_only, "read_only"), (st.required, "required"), (st.invalid, "invalid"), (st.busy, "busy"), (st.modal, "modal")] {
                    if on {
                        says.push((*word).to_owned());
                    }
                }
                if let Some(v) = st.value_now {
                    says.push(format!("value={v}"));
                }
                if st.set_size > 0 {
                    says.push(format!("{} of {}", st.pos_in_set, st.set_size));
                }
                if st.orientation > 0 {
                    says.push(format!("orientation={}", st.orientation));
                }
                if st.live > 0 {
                    says.push(format!("live={}", st.live));
                }
                println!("a11y {:?} {:?} {}", n.role, n.label, says.join(" "));
            }
        }
    }
}

/// `SNAPSHOT_DUMP=1`: one line per node, indented by depth — kind, id, the
/// style it is pointed at, its rect and its text, for reading a layout
/// without a screen.
///
/// The style id is there because a local handler's whole effect is to
/// change it (07 §1): a hover that did not light or a focus that did not
/// take is a node still on the style it started on, and nothing else in a
/// picture says which style that was.
fn dump(driver: &Driver, ix: eui_tree::NodeIx, depth: usize) {
    let s = driver.session();
    let Some(node) = s.node(ix) else { return };
    let rect = driver.layout().rect(ix).map_or("absent".to_owned(), |r| format!("{:.0},{:.0} {:.0}x{:.0}", r.x, r.y, r.w, r.h));
    let text = s.text_of(ix).map_or(String::new(), |t| format!(" {t:?}"));
    // Which events the node answers, and whether locally: a widget that
    // "does nothing" is usually one the view never gave the handler to.
    let kinds = [
        (eui_proto::EventKind::Click, "click"),
        (eui_proto::EventKind::PointerEnter, "enter"),
        (eui_proto::EventKind::PointerLeave, "leave"),
        (eui_proto::EventKind::Focus, "focus"),
        (eui_proto::EventKind::Blur, "blur"),
        (eui_proto::EventKind::Change, "change"),
    ];
    let on: Vec<String> = kinds
        .iter()
        .filter_map(|(k, name)| {
            s.handler(ix, *k).map(|h| match h {
                eui_proto::Handler::Server(_) => (*name).to_owned(),
                _ => format!("{name}*"),
            })
        })
        .collect();
    let on = if on.is_empty() { String::new() } else { format!(" on[{}]", on.join(",")) };
    println!("{:indent$}{:?}#{} s{}{on} {rect}{text}", "", node.kind, node.id, node.style, indent = depth * 2);
    for c in s.children(ix) {
        dump(driver, *c, depth + 1);
    }
}
