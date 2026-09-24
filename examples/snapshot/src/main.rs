//! `snapshot <out-dir>` — the counter, rendered off-screen at 2× in light and
//! dark, before and after clicks, as raw RGBA files plus a manifest line each.
//!
//! `snapshot <out-dir> --soli <session-url> <name> <w> <h>` — connect to a
//! running Soli, mount `<name>`'s component, fetch its assets, and render it
//! in light and dark. `EUI_ALLOW_INSECURE_LOOPBACK=1` for a `ws://` URL.
//! `SNAPSHOT_CLICK`, `SNAPSHOT_SCROLL`, `SNAPSHOT_HOVER`, `SNAPSHOT_KEYS`,
//! `SNAPSHOT_THEN` (keys after the text), `SNAPSHOT_DRIVE` (keys and text
//! interleaved),
//! `SNAPSHOT_TEXT` and `SNAPSHOT_COVERED` drive it first, so a pane two
//! clicks in, a card below the fold, a state that only exists under the
//! pointer, a field that has been typed into, or a page with a phone's
//! keyboard standing on it can be looked at without a screen.

#![allow(clippy::arithmetic_side_effects, clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use eui_client::driver::SceneAsset;
use eui_client::{Driver, Input};
use eui_proto::{Frame, Start, ThemeMode, Welcome};
use eui_render::{Renderer, SessionTextures};
use std::time::{Duration, Instant};

/// The longest a snapshot waits for transitions to finish. `motion.slowest`
/// is a second (05 §2); anything still running past twice that is a clock
/// that never stops, and the picture is taken anyway.
const LAND_LIMIT: Duration = Duration::from_secs(2);

/// Run the driver's clock until no transition, entrance or glide is in
/// flight, so the list painted next is where every change *ends*.
///
/// The tool paints but only ticks when asked, and the renderer draws a list
/// at the age it is given — 0 unless `SNAPSHOT_AGE` says otherwise. A style
/// change with a `transition` is a quad that carries both colours and eases
/// between them on the vertex stage's clock, so at age 0 it is exactly the
/// colour it is leaving. A row restyled by `j` came out in last render's
/// colours on every run, `SNAPSHOT_SETTLE` could not help while it ran
/// before the keys, and "a style-only change is not delivered" stood as a
/// fact about the protocol for a week. A spin is not waited for: it never
/// ends, and `animating` does not count it.
fn let_transitions_land(driver: &mut Driver, dw: u32, dh: u32) {
    let until = Instant::now() + LAND_LIMIT;
    let _ = driver.paint(dw, dh);
    while driver.animating() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(16));
        driver.tick(Instant::now());
        let _ = driver.paint(dw, dh);
    }
}

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

/// `SNAPSHOT_ALLOW=scene,clipboard.read` — what to grant the session.
///
/// Nothing by default, which is the right default for a tool that renders
/// somebody else's application. But a capability-gated node draws its own
/// background and nothing else without the grant (08 §3), so the only pixel
/// harness in the repository could not look at a `scene` at all until this
/// existed. An unknown name is a mistake worth stopping for, not worth
/// guessing at.
fn granted() -> u32 {
    let Ok(list) = std::env::var("SNAPSHOT_ALLOW") else { return 0 };
    list.split(',').map(str::trim).filter(|n| !n.is_empty()).map(|n| eui_proto::caps::from_name(n).unwrap_or_else(|| panic!("SNAPSHOT_ALLOW: no capability called {n:?}"))).fold(0, |a, b| a | b)
}

static ASKED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static GOT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or_else(|| ".".into());
    // `snapshot <out> --view <url> <component> <w> <h>` — the same page,
    // fetched over HTTPS with no session at all (01 §2.4). The point of
    // having it here is the comparison: run it beside `--soli` against the
    // same component and the two `.rgba` files should be identical, which is
    // what "a page drawn without a socket is the same page" means when it is
    // checked rather than asserted.
    if args.get(2).map(String::as_str) == Some("--view") {
        let url = args.get(3).expect("url");
        let component = args.get(4).expect("component");
        let w: f32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(1000.0);
        let h: f32 = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(900.0);
        let scale: f32 = args.get(7).and_then(|s| s.parse().ok()).unwrap_or(2.0);
        snapshot_view(&out, url, component, w, h, scale);
        return;
    }
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
        let mut driver = Driver::new(w, h, scale, granted());
        let mut counter = counter_server::Counter::default();
        driver.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
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
        write_image(&out, name, &px, dw, dh);
        println!("{name} {dw} {dh} quads={} mount_bytes={wire}", list.quads.len());
    }
}

/// Write `px` — sRGB RGBA, row-major — as `<out>/<file>`.
///
/// Raw bytes by default, because that is what the pixel tests compare and a
/// PNG would only be a slower way to say the same thing. `SNAPSHOT_PNG=1`
/// asks for the other one: a poster for a documentation page, which wants a
/// picture a browser can show and not a buffer only this repository can read.
fn write_image(out: &str, file: &str, px: &[u8], w: u32, h: u32) {
    if std::env::var_os("SNAPSHOT_PNG").is_none() {
        std::fs::write(format!("{out}/{file}.rgba"), px).unwrap();
        return;
    }
    let path = format!("{out}/{file}.png");
    let writer = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
    let mut encoder = png::Encoder::new(writer, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().unwrap().write_image_data(px).unwrap();
}

/// Render a component served by a running Soli, in both modes.
/// One render, fetched rather than subscribed to.
///
/// No socket is opened at any point, which is the whole claim: if this draws
/// the page, then a reader who only reads costs the server one cached
/// response and nothing resident.
fn snapshot_view(out: &str, url: &str, component: &str, w: f32, h: f32, scale: f32) {
    use eui_client::transport::{fetch_view, Fetcher};
    use eui_client::Incoming;
    use std::sync::mpsc;

    let cookie = std::env::var("SNAPSHOT_COOKIE").ok();
    let (dw, dh) = ((w * scale) as u32, (h * scale) as u32);
    let mut renderer = Renderer::new_headless().expect("a GPU adapter");
    let mut textures = renderer.session();
    let view = fetch_view(url, component, eui_proto::PROTOCOL_VERSION, w as u32, cookie.as_deref()).expect("a page");
    let origin = eui_client::assets::origin_for(&eui_client::normalise_url(url)).expect("an origin");

    for (mode_name, mode) in [("light", ThemeMode::Light), ("dark", ThemeMode::Dark)] {
        let mut driver = Driver::new(w, h, scale, granted());
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let (fetch, rx) = Fetcher::alone(origin.clone(), cookie.clone(), move || {
            let _ = wake_tx.send(());
        });
        driver.input(Input::Mode(mode));
        // The frames as they arrived, through the ordinary path: the driver
        // cannot tell this from a socket that welcomed and mounted.
        for bytes in &view.frames {
            let frame = Frame::decode(bytes).expect("frame");
            driver.handle_frame(frame);
        }
        // Whatever the tree names, fetched with no session behind it.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut outstanding: usize = 0;
        let (mut asked, mut got) = (0usize, 0usize);
        loop {
            for hash in driver.pending_assets() {
                fetch.request_asset(hash);
                outstanding += 1;
                asked += 1;
            }
            if outstanding == 0 {
                break;
            }
            assert!(Instant::now() < deadline, "timed out fetching assets");
            let _ = wake_rx.recv_timeout(Duration::from_millis(50));
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Incoming::Asset(hash, Ok(bytes)) => {
                        driver.asset_ready(hash, bytes);
                        // No clock here to wake on a decode: wait for it, so the frame
                        // this run paints is the one with the picture in it.
                        driver.finish_decoding();
                        got += 1;
                        outstanding = outstanding.saturating_sub(1);
                    }
                    Incoming::Asset(hash, Err(e)) => {
                        eprintln!("snapshot: asset failed: {e}");
                        driver.asset_failed(hash, e);
                        outstanding = outstanding.saturating_sub(1);
                    }
                    other => panic!("nothing else can arrive without a socket: {other:?}"),
                }
            }
        }
        driver.tick(Instant::now());
        let list = driver.paint(dw, dh);
        let target = renderer.offscreen(dw, dh);
        load_scene_assets(&mut driver, &mut renderer, &mut textures);
        let (atlas, images) = driver.atlases_mut();
        renderer.render_offscreen(&mut textures, &target, 0.0, &list, atlas, images);
        let px = renderer.read_back(&target).expect("read back");
        let file = format!("{component}-{mode_name}");
        write_image(out, &file, &px, dw, dh);
        println!(
            "{file} {dw} {dh} quads={} nodes={} frames={} bytes={} assets={got}/{asked}",
            list.quads.len(),
            driver.session().live_nodes(),
            view.frames.len(),
            view.frames.iter().map(Vec::len).sum::<usize>()
        );
    }
}

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
        let mut driver = Driver::new(w, h, scale, granted());
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
                        // Counted here as well as in the click loops below:
                        // the mount loop is where most assets actually
                        // arrive, so a run that fetched everything it named
                        // reported `assets=0/2` and looked like one that had
                        // fetched nothing.
                        GOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        driver.asset_ready(hash, bytes);
                        // No clock here to wake on a decode: wait for it, so the frame
                        // this run paints is the one with the picture in it.
                        driver.finish_decoding();
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
                ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
                                // No clock here to wake on a decode: wait for it, so the frame
                                // this run paints is the one with the picture in it.
                                driver.finish_decoding();
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
            // A wheel reports where it left the view (06 §8, `scroll`), and a
            // server that subscribed to that is owed the report: dropping the
            // frames here left the application believing the offset it last
            // set, which is exactly the bug this option exists to photograph.
            for f in driver.input(Input::Wheel(0.0, dy)) {
                conn.tx.send(f.encode()).unwrap();
            }
            // A scroll glides: it lands on the clock, not on the wheel
            // event, so the frames it wants are run here rather than
            // painting the page half way there. The landing reports too.
            let mut clock = Instant::now();
            for _ in 0..60 {
                clock += Duration::from_millis(16);
                driver.tick(clock);
                let _ = driver.paint(dw, dh);
                for f in driver.take_pending() {
                    conn.tx.send(f.encode()).unwrap();
                }
                while let Ok(msg) = conn.rx.try_recv() {
                    match msg {
                        Incoming::Message(b) => {
                            let frame = Frame::decode(&b).expect("frame");
                            for f in driver.handle_frame(frame) {
                                conn.tx.send(f.encode()).unwrap();
                            }
                        }
                        Incoming::Asset(hash, Ok(bytes)) => {
                            GOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            driver.asset_ready(hash, bytes);
                            // No clock here to wake on a decode: wait for it, so the frame
                            // this run paints is the one with the picture in it.
                            driver.finish_decoding();
                        }
                        Incoming::Asset(hash, Err(e)) => driver.asset_failed(hash, e),
                        Incoming::Closed(e) => panic!("closed: {e}"),
                    }
                }
            }
        }
        // SNAPSHOT_KEYS="Tab;Escape;^z;~ArrowUp" — keys pressed in order
        // before the last paint, with `$` shift, `^` control, `~` alt and
        // `#` super on the front of one that wants a modifier.
        // paint, so a focus ring, a trapped Tab or a surface that closes on
        // Escape can be looked at without a keyboard. Each press waits for
        // whatever the server sends back, the way a click does.
        if let Ok(keys) = std::env::var("SNAPSHOT_KEYS") {
            for entry in keys.split(';').map(str::trim).filter(|k| !k.is_empty()) {
                // A leading `^`, `~`, `$` or `#` is control, alt, shift or
                // super (06 §1: 1 shift, 2 control, 4 alt, 8 super). Without
                // them the accelerators cannot be photographed at all — and
                // an accelerator is exactly the kind of key a node has to
                // *claim* to hear, so the path it takes is the one worth a
                // picture.
                let mut modifiers: u32 = 0;
                let mut key = entry;
                loop {
                    let bit = match key.as_bytes().first() {
                        Some(b'$') => 1,
                        Some(b'^') => 2,
                        Some(b'~') => 4,
                        Some(b'#') => 8,
                        _ => break,
                    };
                    modifiers |= bit;
                    key = &key[1..];
                }
                let key = key.to_owned();
                let _ = driver.paint(dw, dh);
                for f in driver.input(Input::Key { key: key.clone(), modifiers, down: true }) {
                    conn.tx.send(f.encode()).unwrap();
                }
                for f in driver.input(Input::Key { key: key.clone(), modifiers, down: false }) {
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
        // SNAPSHOT_SETTLE=<ms> — let time pass before the last paint, ticking
        // the driver's clock and applying whatever arrives.
        //
        // Without this the tool paints but never ticks, so anything the clock
        // drives is invisible to it: a `wake` never fires, a transition never
        // eases, a glide never lands. An application that shows a loader until
        // its first wake would be photographed mid-load forever.
        //
        // After the keys, because what a key sets going is the usual reason
        // to want time to pass: an application that answers a press by arming
        // work for its next `wake` -- a send, a fetch, a mark -- was
        // unphotographable while this ran first, and every such press looked
        // like a press that did nothing.
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
                        Incoming::Asset(hash, Ok(bytes)) => {
                            GOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            driver.asset_ready(hash, bytes);
                            // No clock here to wake on a decode: wait for it, so the frame
                            // this run paints is the one with the picture in it.
                            driver.finish_decoding();
                        }
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
                    ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
        // SNAPSHOT_THEN="Enter;^s" — keys pressed *after* the text, which
        // `SNAPSHOT_KEYS` cannot be: it runs before it. A form is filled and
        // then submitted, in that order, and a page that only exists on the
        // other side of a submit could not be photographed at all.
        if let Ok(after) = std::env::var("SNAPSHOT_THEN") {
            for entry in after.split(';').filter(|e| !e.is_empty()) {
                let mut modifiers: u32 = 0;
                let mut key = entry;
                loop {
                    let bit = match key.as_bytes().first() {
                        Some(b'$') => 1,
                        Some(b'^') => 2,
                        Some(b'~') => 4,
                        Some(b'#') => 8,
                        _ => break,
                    };
                    modifiers |= bit;
                    key = &key[1..];
                }
                let key = key.to_owned();
                let _ = driver.paint(dw, dh);
                for f in driver.input(Input::Key { key: key.clone(), modifiers, down: true }) {
                    conn.tx.send(f.encode()).unwrap();
                }
                for f in driver.input(Input::Key { key, modifiers, down: false }) {
                    conn.tx.send(f.encode()).unwrap();
                }
                // The answer to a submit is a whole new tree; wait for it the
                // way the text runs above wait for theirs.
                let until = Instant::now() + Duration::from_millis(1200);
                while Instant::now() < until {
                    let _ = wake_rx.recv_timeout(Duration::from_millis(50));
                    while let Ok(msg) = conn.rx.try_recv() {
                        if let Incoming::Message(b) = msg {
                            if let Ok(frame) = Frame::decode(&b) {
                                for out in driver.handle_frame(frame) {
                                    conn.tx.send(out.encode()).unwrap();
                                }
                            }
                        }
                    }
                    driver.tick(Instant::now());
                    let _ = driver.paint(dw, dh);
                }
            }
        }
        // SNAPSHOT_DRIVE="key:^b;key:n;text:A title;key:Enter;click:Save;wait:200;key:$Tab;key:Enter"
        // — keys and text *interleaved*, which none of the three above can
        // be: `SNAPSHOT_KEYS` always runs before `SNAPSHOT_TEXT` and
        // `SNAPSHOT_THEN` always after it, so a form whose second field is
        // only reachable by a key pressed in the first one could not be
        // filled in at all. A step is `key:<k>` (the `$^~#` modifier
        // prefixes work as everywhere else, and the `key:` is optional),
        // `text:<what>`, `paste:<what>`, `click:<label>` / `click:<x>,<y>`,
        // `drag:<x1>,<y1>><x2>,<y2>`, or `wait:<ms>`.
        // Each waits for the server's answer,
        // the way a click does.
        if let Ok(script) = std::env::var("SNAPSHOT_DRIVE") {
            for step in script.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                if let Some(ms) = step.strip_prefix("wait:").and_then(|v| v.trim().parse::<u64>().ok()) {
                    let until = Instant::now() + Duration::from_millis(ms);
                    while Instant::now() < until {
                        let _ = wake_rx.recv_timeout(Duration::from_millis(16));
                        while let Ok(msg) = conn.rx.try_recv() {
                            match msg {
                                Incoming::Message(b) => {
                                    if let Ok(frame) = Frame::decode(&b) {
                                        for out in driver.handle_frame(frame) {
                                            conn.tx.send(out.encode()).unwrap();
                                        }
                                    }
                                }
                                Incoming::Asset(hash, Ok(bytes)) => {
                                    GOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    driver.asset_ready(hash, bytes);
                                    // No clock here to wake on a decode: wait for it, so the frame
                                    // this run paints is the one with the picture in it.
                                    driver.finish_decoding();
                                }
                                Incoming::Asset(hash, Err(e)) => driver.asset_failed(hash, e),
                                Incoming::Closed(e) => panic!("closed: {e}"),
                            }
                        }
                        driver.tick(Instant::now());
                        let _ = driver.paint(dw, dh);
                        for f in driver.take_pending() {
                            conn.tx.send(f.encode()).unwrap();
                        }
                        // What the frames just applied name — a picture a
                        // page-change brought in, a face — fetched the way
                        // a window fetches it. A drive that never asked
                        // photographed every picture as an empty box and
                        // reported `assets=0/0`, which reads like a client
                        // that fetches nothing.
                        for hash in driver.pending_assets() {
                            conn.request_asset(hash);
                            ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    continue;
                }
                // `drag:<x1>,<y1>><x2>,<y2>` — a press, a few moves and a
                // lift, which is the only way to photograph anything that
                // reads a gesture rather than a press: a selection, a
                // marquee, a handle being dragged.
                if let Some(path) = step.strip_prefix("drag:") {
                    let mut ends = path.split('>');
                    let point = |t: Option<&str>| -> Option<(f32, f32)> {
                        let (a, b) = t?.split_once(',')?;
                        Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
                    };
                    let (Some(from), Some(to)) = (point(ends.next()), point(ends.next())) else {
                        eprintln!("snapshot: drag wants x1,y1>x2,y2");
                        continue;
                    };
                    let mut inputs = vec![Input::PointerMove(from.0, from.1), Input::PointerDown(0)];
                    for i in 1..=4 {
                        let t = i as f32 / 4.0;
                        inputs.push(Input::PointerMove(from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t));
                    }
                    inputs.push(Input::PointerUp(0));
                    for i in inputs {
                        for f in driver.input(i) {
                            conn.tx.send(f.encode()).unwrap();
                        }
                        let _ = driver.paint(dw, dh);
                        let deadline = Instant::now() + Duration::from_millis(400);
                        while Instant::now() < deadline {
                            let _ = wake_rx.recv_timeout(Duration::from_millis(20));
                            let mut answered = false;
                            while let Ok(msg) = conn.rx.try_recv() {
                                if let Incoming::Message(b) = msg {
                                    answered = true;
                                    if let Ok(frame) = Frame::decode(&b) {
                                        for f in driver.handle_frame(frame) {
                                            conn.tx.send(f.encode()).unwrap();
                                        }
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
                    continue;
                }
                // `click:<label>` or `click:<x>,<y>` — a press where the
                // three above cannot put one: after the keys, on a thing the
                // keys brought into being.
                let inputs = match step.strip_prefix("click:").map(str::trim) {
                    Some(what) => {
                        let point = what.split_once(',').and_then(|(a, b)| Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?)));
                        let at = match point {
                            Some(at) => Some(at),
                            None => {
                                let found = driver.session().root().and_then(|root| driver.session().preorder(root).find(|ix| driver.session().text_of(*ix) == Some(what)));
                                match found {
                                    Some(mut ix) => {
                                        while driver.session().handler(ix, eui_proto::EventKind::Click).is_none() {
                                            let Some(up) = driver.session().node(ix).map(|n| n.parent) else { break };
                                            if up == ix {
                                                break;
                                            }
                                            ix = up;
                                        }
                                        driver.layout().rect(ix).map(|r| (r.x + r.w / 2.0, r.y + r.h / 2.0))
                                    }
                                    None => {
                                        eprintln!("snapshot: nothing reads {what:?}");
                                        None
                                    }
                                }
                            }
                        };
                        let Some((px, py)) = at else { continue };
                        vec![Input::PointerMove(px, py), Input::PointerDown(0), Input::PointerUp(0)]
                    }
                    None => match step.strip_prefix("paste:") {
                        // What the window sends when somebody presses the
                        // platform's paste. It is not typing, and 03 §3.1
                        // treats it as its own thing, so a tool that can
                        // only type cannot photograph what a paste does.
                        Some(t) => vec![Input::Paste(t.to_owned())],
                        None => match step.strip_prefix("text:") {
                            Some(t) => vec![Input::Text(t.to_owned())],
                            None => {
                                let mut modifiers: u32 = 0;
                                let mut key = step.strip_prefix("key:").unwrap_or(step);
                                loop {
                                    let bit = match key.as_bytes().first() {
                                        Some(b'$') => 1,
                                        Some(b'^') => 2,
                                        Some(b'~') => 4,
                                        Some(b'#') => 8,
                                        _ => break,
                                    };
                                    modifiers |= bit;
                                    key = &key[1..];
                                }
                                let key = key.to_owned();
                                vec![Input::Key { key: key.clone(), modifiers, down: true }, Input::Key { key, modifiers, down: false }]
                            }
                        },
                    },
                };
                let _ = driver.paint(dw, dh);
                for input in inputs {
                    for f in driver.input(input) {
                        conn.tx.send(f.encode()).unwrap();
                    }
                }
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if Instant::now() > deadline {
                        break;
                    }
                    let _ = wake_rx.recv_timeout(Duration::from_millis(50));
                    let mut answered = false;
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
                                GOT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                driver.asset_ready(hash, bytes);
                                // No clock here to wake on a decode: wait for it, so the frame
                                // this run paints is the one with the picture in it.
                                driver.finish_decoding();
                            }
                            Incoming::Asset(hash, Err(e)) => driver.asset_failed(hash, e),
                            Incoming::Closed(e) => panic!("closed: {e}"),
                        }
                    }
                    driver.tick(Instant::now());
                    let _ = driver.paint(dw, dh);
                    for f in driver.take_pending() {
                        conn.tx.send(f.encode()).unwrap();
                    }
                    for hash in driver.pending_assets() {
                        conn.request_asset(hash);
                        ASKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
            // SNAPSHOT_CURSOR=<ms> — hold the pointer still for that long and
            // print the shape it takes on every frame. A cursor that flickers
            // on a live page is a node going out from under a pointer that
            // never moved, and there is no other way to see it happen: a
            // screenshot cannot hold a pointer and the shape is not in the
            // pixels.
            if let Some(ms) = std::env::var("SNAPSHOT_CURSOR").ok().and_then(|v| v.trim().parse::<u64>().ok()) {
                let until = Instant::now() + Duration::from_millis(ms);
                let mut was = None;
                let mut frames = 0u32;
                let mut changes = 0u32;
                let mut batches = 0u32;
                let mut bytes = 0usize;
                while Instant::now() < until {
                    while let Ok(msg) = conn.rx.try_recv() {
                        if let Incoming::Message(b) = msg {
                            batches += 1;
                            bytes += b.len();
                            if let Ok(frame) = Frame::decode(&b) {
                                for out in driver.handle_frame(frame) {
                                    conn.tx.send(out.encode()).unwrap();
                                }
                            }
                        }
                    }
                    driver.tick(Instant::now());
                    let _ = driver.paint(dw, dh);
                    // The `wake` a live page runs on leaves through here. A
                    // loop that only ticked and painted never sent one, so
                    // the server rebuilt nothing and the measurement said
                    // "the page is quiet" about a page that is not.
                    for f in driver.take_pending() {
                        conn.tx.send(f.encode()).unwrap();
                    }
                    let now = driver.cursor();
                    frames += 1;
                    if Some(now) != was {
                        changes += 1;
                        println!("cursor {:?} at frame {frames}", now);
                        let _ = (batches, bytes);
                        was = Some(now);
                    }
                    std::thread::sleep(Duration::from_millis(16));
                }
                println!("cursor: {changes} change(s) over {frames} frames with the pointer still");
                println!("server: {batches} batch(es), {bytes} bytes, while nothing was touched");
            }
        }
        // A picture of the page, not of the first frame of its last change.
        // Without `SNAPSHOT_AGE` the list is drawn at age 0, and a node whose
        // style carries a `transition` is then photographed in its *old*
        // colours — which is how a mail reader's cursor was recorded as a
        // `SetStyle` the server never sent (spec 03 §5; 09 §8 names the test).
        if std::env::var_os("SNAPSHOT_AGE").is_none() {
            let_transitions_land(&mut driver, dw, dh);
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
        write_image(out, &file, &px, dw, dh);
        println!(
            "{file} {dw} {dh} quads={} nodes={} assets={}/{}",
            list.quads.len(),
            driver.session().live_nodes(),
            GOT.load(std::sync::atomic::Ordering::Relaxed),
            ASKED.load(std::sync::atomic::Ordering::Relaxed)
        );
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
                if !n.value.is_empty() {
                    says.push(format!("value={:?}", n.value));
                }
                if !st.placeholder.is_empty() {
                    says.push(format!("placeholder={:?}", st.placeholder));
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
    // What a picture names: the one prop that decides whether anything is
    // fetched for it. An image drawn as an empty box is either a `src`
    // that is not an asset or an asset that never arrived, and the two
    // look the same on the screen.
    let props = if matches!(node.kind, eui_proto::NodeKind::Image | eui_proto::NodeKind::Audio | eui_proto::NodeKind::Video) {
        let said: Vec<String> = node
            .props
            .iter()
            .map(|(a, v)| match v {
                eui_proto::Value::Asset(h) => format!("{a}=asset:{}", h.iter().map(|b| format!("{b:02x}")).collect::<String>()),
                other => format!("{a}={other:?}").chars().take(60).collect(),
            })
            .collect();
        format!(" p[{}]", said.join(" "))
    } else {
        String::new()
    };
    println!("{:indent$}{:?}#{} s{}{on} {rect}{text}{props}", "", node.kind, node.id, node.style, indent = depth * 2);
    for c in s.children(ix) {
        dump(driver, *c, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eui_proto::{Batch, ColorRef, Op};

    fn animated(q: &eui_render::Quad) -> bool {
        q.params[2] as u32 & eui_render::ANIMATED != 0
    }

    /// The counter, with its `+` button restyled by the server to a
    /// different ground and a `base` transition — the shape of a list row
    /// whose selection moved.
    fn restyled() -> Driver {
        let mut d = Driver::new(420.0, 260.0, 1.0, 0);
        let mut counter = counter_server::Counter::default();
        d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
        d.handle_frame(Frame::Batch(counter.first()));
        let _ = d.paint(420, 260);
        let plus = d.session().lookup(4).expect("the counter's + button");
        let button = *d.session().style(d.session().node(plus).expect("a node").style).expect("its style");
        let lit = eui_proto::StyleRecord { bg: ColorRef::role(button.fg.index()), fg: button.bg, transition: 2, ..button };
        d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::DefStyle { id: 7, record: lit }, Op::SetStyle { node: 4, style: 7 }] }));
        d
    }

    /// The trap the tool used to be: the list painted straight after the
    /// batch holds the restyled box as a moving quad, and at age 0 a moving
    /// quad is the colour it is leaving.
    #[test]
    fn a_restyle_with_a_transition_is_mid_flight_on_the_first_paint() {
        let mut d = restyled();
        let list = d.paint(420, 260);
        let q = list.quads.iter().find(|q| animated(q)).expect("the restyled box eases");
        let from = eui_render::unpack4([q.from[0], q.from[1], q.from[2], q.from[3]]);
        assert_ne!(from, eui_render::unpack4(eui_render::pack4(q.fill)), "and at age 0 it shows where it came from");
    }

    /// What the picture is taken of now: the change landed, nothing moving,
    /// the box in its new colour.
    #[test]
    fn the_picture_is_taken_after_the_transition_lands() {
        let mut d = restyled();
        let target = d.paint(420, 260).quads.iter().find(|q| animated(q)).map(|q| (q.rect, q.fill)).expect("a transition in flight");
        let_transitions_land(&mut d, 420, 260);
        assert!(!d.animating(), "at rest");
        let list = d.paint(420, 260);
        assert!(!list.quads.iter().any(animated), "no quad still easing");
        let landed = list.quads.iter().find(|q| q.rect == target.0).expect("the same box");
        assert_eq!(landed.fill, target.1, "in the colour it was going to");
    }
}
