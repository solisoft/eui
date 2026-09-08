//! The counter, served by Soli itself: `soli serve examples/counter-app` on a
//! binary built with `--features eui`, driven by the real transport and the
//! real driver. Skipped unless `EUI_SOLI_BIN` points at such a binary.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eui_client::{connect, Driver, Incoming, Input};
use eui_proto::{EventKind, Frame};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn start_soli(bin: &str) -> (Server, u16) {
    let app = std::env::var("EUI_SOLI_APP").unwrap_or_else(|_| format!("{}/../../examples/counter-app", env!("CARGO_MANIFEST_DIR")));
    let port = free_port();
    // EUI_SOLI_LOG=path captures the server's stderr for a post-mortem.
    let stderr = match std::env::var("EUI_SOLI_LOG") {
        Ok(path) => Stdio::from(std::fs::File::create(path).expect("log file")),
        Err(_) => Stdio::inherit(),
    };
    let child = Command::new(bin)
        .args(["serve", &app, "--port", &port.to_string()])
        // The player's sample catalogue, not whatever catalogue the
        // machine happens to be configured for. Soli's `.env` loader only
        // fills a variable that is not already set, so setting these to
        // empty is how a test says "no account" over a developer's own
        // `examples/counter-app/.env`.
        .env("SPOTIFY_CLIENT_ID", "")
        .env("SPOTIFY_CLIENT_SECRET", "")
        .env("SPOTIFY_REFRESH_TOKEN", "")
        .env("SPOTIFY_USER_TOKEN", "")
        .stdout(Stdio::null())
        .stderr(stderr)
        .spawn()
        .expect("spawn soli");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(mut s) = TcpStream::connect(("127.0.0.1", port)) {
            let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = s.write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
            let mut buf = String::new();
            let _ = s.read_to_string(&mut buf);
            if buf.starts_with("HTTP/1.1 200") {
                break;
            }
        }
        assert!(Instant::now() < deadline, "soli did not come up on {port}");
        std::thread::sleep(Duration::from_millis(200));
    }
    (Server(child), port)
}

fn pump(driver: &mut Driver, conn: &eui_client::Connection, wake: &mpsc::Receiver<()>, until: impl Fn(&Driver) -> bool) {
    // A debug interpreter builds five thousand cards in seconds, more on a
    // busy machine.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !until(driver) {
        assert!(Instant::now() < deadline, "timed out waiting on soli");
        let _ = wake.recv_timeout(Duration::from_millis(50));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    let frame = Frame::decode(&bytes).expect("soli sent a well-formed frame");
                    if std::env::var("EUI_SOLI_TRACE").is_ok() {
                        match &frame {
                            Frame::Batch(b) => eprintln!("TRACE server -> batch seq={} ops={:?}", b.seq, b.ops.iter().map(|o| format!("{o:?}").chars().take(60).collect::<String>()).collect::<Vec<_>>()),
                            other => eprintln!("TRACE server -> {other:?}"),
                        }
                    }
                    if let Frame::Error { code, message } = &frame {
                        panic!("soli sent error {code}: {message}");
                    }
                    for out in driver.handle_frame(frame) {
                        if std::env::var("EUI_SOLI_TRACE").is_ok() {
                            eprintln!("TRACE client -> {:?}", out);
                        }
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("connection closed: {e}"),
                Incoming::Asset(hash, Ok(bytes)) => driver.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => driver.asset_failed(hash, why),
            }
        }
    }
}

/// The value text is the second child of the root column.
fn value(d: &Driver) -> Option<String> {
    let root = d.session().root()?;
    let ix = *d.session().children(root).get(1)?;
    d.session().text_of(ix).map(str::to_owned)
}

/// The "+" button is the second child of the row that is the root's third child.
fn plus(d: &Driver) -> Option<eui_tree::NodeIx> {
    let root = d.session().root()?;
    let row = *d.session().children(root).get(2)?;
    d.session().children(row).get(1).copied()
}

#[test]
fn the_counter_runs_end_to_end_against_soli() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else {
        eprintln!("EUI_SOLI_BIN not set; skipping the Soli end-to-end test");
        return;
    };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let url = format!("ws://127.0.0.1:{port}/_eui/session/counter");

    let mut driver = Driver::new(420.0, 260.0, 1.0, 0);
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let conn = connect(&url, driver.hello().encode(), move || {
        let _ = wake_tx.send(());
    })
    .expect("connect to soli");

    pump(&mut driver, &conn, &wake_rx, |d| value(d).is_some());
    assert_eq!(value(&driver).as_deref(), Some("0"), "the view rendered by Soli shows 0");
    assert_eq!(driver.session().live_nodes(), 9, "column, title, value, row, 2 buttons × (box + text), hint");

    let _ = driver.paint(420, 260);
    let plus_ix = plus(&driver).unwrap();
    let r = driver.layout().rect(plus_ix).unwrap();
    // Hover and press are local handlers Soli compiled from `self.style = @hover`:
    // the button's style id changes with the pointer, and no frame leaves.
    let base_style = driver.session().node(plus_ix).unwrap().style;
    assert!(driver.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0)).is_empty());
    let hover_style = driver.session().node(plus_ix).unwrap().style;
    assert_ne!(hover_style, base_style, "pointer_enter repointed the button at its hover style");
    assert!(driver.input(Input::PointerDown(0)).is_empty());
    let active_style = driver.session().node(plus_ix).unwrap().style;
    assert!(active_style != hover_style && active_style != base_style, "pointer_down: active style");
    let out = driver.input(Input::PointerUp(0));
    assert_eq!(driver.session().node(plus_ix).unwrap().style, hover_style, "pointer_up: back to hover");
    assert!(matches!(out.as_slice(), [Frame::Event(e)] if e.event == EventKind::Click), "{out:?}");
    conn.tx.send(out[0].encode()).unwrap();

    // "+" is local-first: the text already reads "1" before any answer. What
    // to wait for is the server's confirmation — one batch per click.
    assert_eq!(value(&driver).as_deref(), Some("1"), "the local handler ran before the round trip");
    pump(&mut driver, &conn, &wake_rx, |d| d.session().last_seq() >= Some(2));
    assert_eq!(value(&driver).as_deref(), Some("1"), "the server agrees");
    // The update was a diff, not a re-mount: same node ids, same count.
    assert_eq!(driver.session().live_nodes(), 9);
    assert_eq!(plus(&driver), Some(plus_ix));

    // Twice more, then wait for both confirmations.
    for _ in 0..2 {
        driver.input(Input::PointerDown(0));
        for f in driver.input(Input::PointerUp(0)) {
            conn.tx.send(f.encode()).unwrap();
        }
    }
    // Leaving the button restores its base style, locally — settled by the
    // frame that follows the move, as a window paints one.
    driver.input(Input::PointerMove(1.0, 1.0));
    let _ = driver.paint(800, 600);
    assert_eq!(driver.session().node(plus_ix).unwrap().style, base_style, "pointer_leave: base style");
    assert_eq!(value(&driver).as_deref(), Some("3"), "the local copy is ahead");
    pump(&mut driver, &conn, &wake_rx, |d| d.session().last_seq() >= Some(4));
    assert_eq!(value(&driver).as_deref(), Some("3"), "the server caught up");

    // Resync: Soli re-sends the tree with its own state, and no definitions.
    conn.tx.send(Frame::Resync.encode()).unwrap();
    pump(&mut driver, &conn, &wake_rx, |d| d.session().last_seq() >= Some(5));
    assert_eq!(value(&driver).as_deref(), Some("3"), "the re-sent tree carries Soli's count");
    assert!(!driver.session().is_poisoned());
}

/// Connect, wait for the first mount, and hand back the pieces.
fn open(port: u16, component: &str, w: f32, h: f32) -> (Driver, eui_client::Connection, mpsc::Receiver<()>) {
    let url = format!("ws://127.0.0.1:{port}/_eui/session/{component}");
    let mut driver = Driver::new(w, h, 1.0, 0);
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let conn = connect(&url, driver.hello().encode(), move || {
        let _ = wake_tx.send(());
    })
    .expect("connect to soli");
    pump(&mut driver, &conn, &wake_rx, |d| d.session().root().is_some());
    (driver, conn, wake_rx)
}

fn click(driver: &mut Driver, conn: &eui_client::Connection, ix: eui_tree::NodeIx) {
    let _ = driver.paint(800, 600);
    let r = driver.layout().rect(ix).expect("target laid out");
    driver.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    driver.input(Input::PointerDown(0));
    for f in driver.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
}

/// All text under a node, depth first.
fn texts(d: &Driver, ix: eui_tree::NodeIx) -> Vec<String> {
    d.session().preorder(ix).filter_map(|n| d.session().text_of(n).map(str::to_owned)).collect()
}

fn root(d: &Driver) -> eui_tree::NodeIx {
    d.session().root().unwrap()
}

#[test]
fn todo_toggles_by_prop_and_keeps_keyed_rows() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "todo", 800.0, 600.0);
    assert!(texts(&d, root(&d)).iter().any(|t| t == "Ship the counter through Soli"));
    assert!(texts(&d, root(&d)).iter().any(|t| t == "1 left"));

    // The header's avatar arrived as a hash. Fetch it from Soli's asset
    // endpoint over plain HTTP (loopback), verify, deliver, and the image
    // gets its size and its pixels.
    let pending = d.pending_assets();
    assert_eq!(pending.len(), 1, "one image, asked for once");
    let hash = pending[0];
    let bytes = eui_client::assets::fetch(&conn.origin, &hash).expect("soli serves the asset");
    assert!(bytes.starts_with(b"\x89PNG"));
    assert_eq!(bytes.len(), 164, "the avatar file, byte for byte");
    d.asset_ready(hash, bytes);
    let list = d.paint(800, 600);
    assert_eq!(list.quads.iter().filter(|q| q.params[2] as u32 == eui_render::TEXTURED_RGBA).count(), 1);
    let header = d.session().children(root(&d))[0];
    let avatar = d.session().children(header)[0];
    let r = d.layout().rect(avatar).unwrap();
    assert_eq!((r.w, r.h), (32.0, 32.0));
    // A wrong hash is refused by the server with a 404, not served.
    assert!(eui_client::assets::fetch(&conn.origin, &[0u8; 32]).is_err());

    // The third row's checkbox: the rows column is the root's third child.
    let rows = d.session().children(root(&d))[2];
    let third = d.session().children(rows)[2];
    let third_id = d.session().node(third).unwrap().id;
    let checkbox = d.session().children(third)[0];
    click(&mut d, &conn, checkbox);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t == "0 left"));
    // Toggling patched the row in place: same row node id.
    let rows = d.session().children(root(&d))[2];
    assert_eq!(d.session().node(d.session().children(rows)[2]).unwrap().id, third_id);

    // Type into the field and submit with Enter: a new keyed row appears.
    let field = d.session().children(d.session().children(root(&d))[1])[0];
    click(&mut d, &conn, field);
    d.input(Input::Text("Fuzz the decoder".into()));
    for f in d.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true }) {
        conn.tx.send(f.encode()).unwrap();
    }
    // Wait for the *row*, not the text: the field shows the draft locally
    // before the server has answered.
    pump(&mut d, &conn, &wake, |d| d.session().children(d.session().children(root(d))[2]).len() == 4);
    let rows = d.session().children(root(&d))[2];
    assert!(texts(&d, rows).iter().any(|t| t == "Fuzz the decoder"));
    assert!(texts(&d, root(&d)).iter().any(|t| t == "1 left"));

    // Clear done removes three keyed rows and leaves the new one, id intact.
    let new_row_id = d.session().node(d.session().children(rows)[3]).unwrap().id;
    let footer = d.session().children(root(&d))[3];
    let clear = d.session().children(footer)[2];
    click(&mut d, &conn, clear);
    pump(&mut d, &conn, &wake, |d| d.session().children(d.session().children(root(d))[2]).len() == 1);
    let rows = d.session().children(root(&d))[2];
    assert_eq!(d.session().node(d.session().children(rows)[0]).unwrap().id, new_row_id);
}

#[test]
fn ten_thousand_rows_mount_within_budget_and_sort_by_moves() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let start = Instant::now();
    let (mut d, conn, wake) = open(port, "table", 800.0, 600.0);
    let mounted = start.elapsed();
    // header row, column header, list → 10 000 rows × 5 nodes.
    assert!(d.session().live_nodes() > 50_000, "{} nodes", d.session().live_nodes());
    let list = d.session().children(root(&d))[2];
    assert_eq!(d.session().children(list).len(), 10_000);
    let first_row_id = d.session().node(d.session().children(list)[0]).unwrap().id;

    // Layout and paint of 10 000 rows is virtualised: quick, and few quads.
    let t = Instant::now();
    let list_draw = d.paint(800, 600);
    let painted = t.elapsed();
    assert!(list_draw.quads.len() < 600, "{} quads for a 400 px list — a non-virtualised paint would be ~200 000", list_draw.quads.len());
    eprintln!("table-10k: mounted in {mounted:?}, painted in {painted:?}, {} quads", list_draw.quads.len());
    assert!(painted < Duration::from_millis(250), "paint took {painted:?}");

    // Sort: the server answers with moves; the row that was first is now
    // last, with the same id, and nothing was rebuilt.
    let header = d.session().children(root(&d))[0];
    let sort = *d.session().children(header).last().unwrap();
    click(&mut d, &conn, sort);
    pump(&mut d, &conn, &wake, |d| {
        let list = d.session().children(root(d))[2];
        d.session().node(*d.session().children(list).last().unwrap()).unwrap().id == first_row_id
    });
    let list = d.session().children(root(&d))[2];
    assert_eq!(d.session().children(list).len(), 10_000);
    let _ = d.paint(800, 600);
}

/// Diagnostic: dump every frame a component's session sends until it closes,
/// and say whether the server is still alive afterwards. Run by hand:
/// `EUI_SOLI_PROBE=counter cargo test -p eui-client --test soli_e2e probe -- --nocapture`
#[test]
fn probe_session_frames() {
    let (Ok(bin), Ok(component)) = (std::env::var("EUI_SOLI_BIN"), std::env::var("EUI_SOLI_PROBE")) else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (mut server, port) = start_soli(&bin);
    let url = format!("ws://127.0.0.1:{port}/_eui/session/{component}");
    let driver = Driver::new(420.0, 260.0, 1.0, 0);
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let conn = connect(&url, driver.hello().encode(), move || {
        let _ = wake_tx.send(());
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let _ = wake_rx.recv_timeout(Duration::from_millis(100));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(b) => match Frame::decode(&b) {
                    Ok(Frame::Batch(batch)) => {
                        eprintln!("PROBE batch seq={} ops={} bytes={}", batch.seq, batch.ops.len(), b.len());
                        for op in &batch.ops {
                            let name = format!("{op:?}");
                            eprintln!("PROBE   {}", &name[..name.len().min(100)]);
                        }
                        // Behave like the client: acknowledge it.
                        let ack = Frame::Ack { seq: batch.seq }.encode();
                        eprintln!("PROBE sending Ack {:02x?}", ack);
                        match conn.tx.send(ack) {
                            Ok(()) => eprintln!("PROBE ack queued"),
                            Err(_) => eprintln!("PROBE ack: transport already gone"),
                        }
                    }
                    Ok(f) => eprintln!("PROBE frame {:?}", f),
                    Err(e) => eprintln!("PROBE undecodable ({e}): {} bytes, head {:02x?}", b.len(), &b[..b.len().min(24)]),
                },
                Incoming::Closed(e) => {
                    eprintln!("PROBE closed: {e}; server alive: {}", server.0.try_wait().ok().flatten().is_none());
                    return;
                }
                Incoming::Asset(..) => {}
            }
        }
    }
    eprintln!("PROBE still open after 3 s; server alive: {}", server.0.try_wait().ok().flatten().is_none());
}

#[test]
fn the_gallery_mounts_and_its_widgets_respond() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 900.0);
    // The page is a scroll container now: grow the window so every widget
    // the test clicks is inside the viewport instead of scrolled away.
    for f in d.input(Input::Resized(1000.0, 2600.0, 1.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    let all = texts(&d, root(&d));
    for expected in ["Overview", "Nodes", "62 %", "Spec", "What is EUI?", "Rename", "A tooltip", "Nothing here yet", "1 / 9", "FA-1001 first"] {
        assert!(all.iter().any(|t| t == expected), "gallery shows {expected:?}");
    }
    let nodes_before = d.session().live_nodes();
    assert!(nodes_before > 120, "{nodes_before} nodes");
    let _ = d.paint(1000, 900);

    // Segmented control: click "Week" (its prop names it) — the selection moves.
    let week = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Week")).unwrap();
    click(&mut d, &conn, week);
    let seq = d.session().last_seq().unwrap();
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    // Accordion: open "b"; "a" closes. The body text of b appears.
    let why = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Why no CSS?")).unwrap();
    click(&mut d, &conn, why);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t.starts_with("Styles are resolved")));
    assert!(!texts(&d, root(&d)).iter().any(|t| t.starts_with("A protocol for interfaces")), "the other section closed");
    // The sheet: opens as an overlay, closes again.
    let open_sheet = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Open sheet")).unwrap();
    click(&mut d, &conn, open_sheet);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t == "A sheet"));
    let close = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Close")).unwrap();
    click(&mut d, &conn, close);
    pump(&mut d, &conn, &wake, |d| !texts(d, root(d)).iter().any(|t| t == "A sheet"));
    let _ = d.paint(1000, 900);

    // Select: opens under its anchor, an option picks and closes it.
    let has = |d: &Driver, t: &str| texts(d, root(d)).iter().any(|x| x == t);
    assert!(!has(&d, "Small"), "closed: options are not in the tree");
    let target = within(&d, "Select", "Medium");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "Small"));
    let target = within(&d, "Select", "Large");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| !has(d, "Small"));
    assert!(has(&d, "Large") && !has(&d, "Medium"), "the anchor shows the pick");

    // Slider: a click three quarters along the track sets 75; once focused
    // from the keyboard, ArrowRight nudges by the server's step of 5.
    let value = within(&d, "Slider", "Value 40");
    let track = slider_track(&d, value);
    let _ = d.paint(1000, 900);
    let r = d.layout().rect(track).unwrap();
    d.input(Input::PointerMove(r.x + r.w * 0.75, r.y + r.h / 2.0));
    d.input(Input::PointerDown(0));
    for f in d.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| has(d, "Value 75"));
    for _ in 0..400 {
        if d.focused() == Some(track) {
            break;
        }
        for f in d.input(Input::Key { key: "Tab".into(), modifiers: 0, down: true }) {
            conn.tx.send(f.encode()).unwrap();
        }
    }
    assert_eq!(d.focused(), Some(track), "Tab reaches the slider: it has a click handler");
    for f in d.input(Input::Key { key: "ArrowRight".into(), modifiers: 0, down: true }) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| has(d, "Value 80"));

    // Drag: press, move along the track, the value follows.
    let value = within(&d, "Slider", "Value 80");
    let track = slider_track(&d, value);
    let _ = d.paint(1000, 900);
    let r = d.layout().rect(track).unwrap();
    d.input(Input::PointerMove(r.x + r.w * 0.8, r.y + r.h / 2.0));
    for f in d.input(Input::PointerDown(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    let seq = d.session().last_seq().unwrap();
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let label = texts(&d, root(&d)).into_iter().find(|t| t.starts_with("Value ")).expect("slider value");
    let value = within(&d, "Slider", &label);
    let track = slider_track(&d, value);
    let _ = d.paint(1000, 900);
    let r = d.layout().rect(track).unwrap();
    d.input(Input::PointerMove(r.x + r.w * 0.2, r.y + r.h / 2.0));
    for f in d.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| has(d, "Value 20"));

    // Date picker: pick the 15th, then turn the month. Three calendars in
    // a grid (4 / 2 / 1 columns by viewport); clicks are scoped by title.
    let target = within(&d, "Date", "15");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "2026-09-15"));
    let target = within(&d, "Date", "›");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "October 2026"));
    assert!(has(&d, "2026-09-15"), "the pick survives turning the month");

    // Range: two clicks — the second earlier than the first.
    let target = within(&d, "Range", "20");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "2026-09-20 → …"));
    let target = within(&d, "Range", "10");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "2026-09-10 → 2026-09-20"));

    // Data grid: a header click sorts by moving keyed rows; a second click
    // on an editable cell turns it into an input, and a commit patches it.
    assert!(has(&d, "FA-1001 first"));
    let target = within(&d, "Grid", "Amount");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "FA-1004 first"));
    let target = within(&d, "Grid", "Ada SARL");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "FA-1001 · client"));
    let target = within(&d, "Grid", "Ada SARL");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "FA-1001 · client · editing"));
    let field = within(&d, "Grid", "Ada SARL");
    click(&mut d, &conn, field);
    for f in d.input(Input::Key { key: "a".into(), modifiers: 2, down: true }) {
        conn.tx.send(f.encode()).unwrap();
    }
    for f in d.input(Input::Text("Ada & Co".into())) {
        conn.tx.send(f.encode()).unwrap();
    }
    for f in d.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true }) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| has(d, "Ada & Co"));

    // Charts: four canvases in a grid (4 / 3 / 2 / 1 columns by viewport)
    // plus the spinner; a line segment paints as a rotated quad.
    let paths = d.session().atom_id("paths").expect("the paths atom");
    let canvases: Vec<_> = d.session().preorder(root(&d)).filter(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Canvas)).collect();
    assert_eq!(canvases.len(), 5, "four charts and the spinner");
    for c in &canvases {
        match d.session().node(*c).unwrap().prop(paths) {
            Some(eui_proto::Value::List(p)) => {
                assert!(!p.is_empty());
                assert!(p.iter().all(|path| matches!(path, eui_proto::Value::List(items) if matches!(items.get(1), Some(eui_proto::Value::Color(_))))), "colours resolved server-side");
            }
            other => panic!("{other:?}"),
        }
    }
    let list = d.paint(1000, 2600);
    assert!(list.quads.iter().any(|q| q.extra[0] != 0.0), "a segment is a rotated capsule");

    // A field keeps what was typed across server round trips the app does
    // not care about, and a later click lands the caret in that text.
    let name_label = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Name")).unwrap();
    let field = d.session().children(d.session().node(name_label).unwrap().parent)[1];
    click(&mut d, &conn, field);
    for f in d.input(Input::Text("azd".into())) {
        conn.tx.send(f.encode()).unwrap();
    }
    let month = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Month")).unwrap();
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, month);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    assert_eq!(d.session().text_of(field), Some("azd"), "the server's re-render did not wipe the field");
    let _ = d.paint(1000, 2600);
    let r = d.layout().rect(field).unwrap();
    d.input(Input::PointerMove(r.x + r.w - 2.0, r.y + r.h / 2.0));
    d.input(Input::PointerDown(0));
    for f in d.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    d.input(Input::Text("!".into()));
    assert_eq!(d.session().text_of(field), Some("azd!"), "the click put the caret at the end of the kept text");
}

/// The node showing `text` inside the titled card `title` — the card is the
/// title's parent, so two calendars showing "15" never collide.
/// The slider's row, found from its value label: it is the sibling just
/// above it. The column carries a heading too, so the track is not simply
/// the first child.
fn slider_track(d: &Driver, value: eui_tree::NodeIx) -> eui_tree::NodeIx {
    let parent = d.session().node(value).unwrap().parent;
    let kids = d.session().children(parent).to_vec();
    let at = kids.iter().position(|c| *c == value).expect("a label is one of its parent's children");
    assert!(at > 0, "the value label follows the track it belongs to");
    kids[at - 1]
}

fn within(d: &Driver, title: &str, text: &str) -> eui_tree::NodeIx {
    let heading = d.session().preorder(root(d)).find(|ix| d.session().text_of(*ix) == Some(title)).unwrap_or_else(|| panic!("no card titled {title:?}"));
    let card = d.session().node(heading).unwrap().parent;
    d.session().preorder(card).find(|ix| d.session().text_of(*ix) == Some(text)).unwrap_or_else(|| panic!("no {text:?} under {title:?}"))
}

#[test]
fn soli_serves_a_signed_manifest_the_client_pins() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    let (_server, port) = start_soli(&bin);
    let origin = format!("http://127.0.0.1:{port}");
    let pins = std::env::temp_dir().join(format!("eui-e2e-pins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pins);
    let m = eui_client::manifest::check(&origin, &pins).expect("a signed manifest");
    assert_eq!(m.app_id, "counter-app", "the application folder's name");
    assert_eq!((m.protocol_min, m.protocol_max), (1, 1));
    assert_eq!(m.entry, "/_eui/session");
    assert_eq!(eui_proto::caps::names(m.capabilities), vec!["clipboard.read"], "what config/routes.sl asked for");
    // Pinned: the same server is accepted again; a stranger's key is not.
    assert!(eui_client::manifest::check(&origin, &pins).is_ok());
    let pin = std::fs::read_dir(&pins).unwrap().next().unwrap().unwrap().path();
    std::fs::write(&pin, [9u8; 32]).unwrap();
    assert_eq!(eui_client::manifest::check(&origin, &pins).unwrap_err(), eui_client::manifest::ManifestError::KeyChanged);
}

/// A desktop artifact built with `soli desktop build --eui gallery`, run
/// headless: its loopback gate admits the embedded client's cookie and
/// nothing else. Needs EUI_DESKTOP_ARTIFACT (the executable) and
/// SOLI_BUNDLE_KEY (the key it was built with).
#[test]
fn a_desktop_artifact_serves_its_component_behind_a_cookie_gate() {
    let Ok(artifact) = std::env::var("EUI_DESKTOP_ARTIFACT") else { return };
    let port = free_port();
    let home = std::env::temp_dir().join(format!("eui-desktop-home-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let mut child = std::process::Command::new(&artifact)
        .args(["--port", &port.to_string()])
        .env("SOLI_DESKTOP_NO_WINDOW", "1")
        .env("HOME", &home)
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("the artifact starts");
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let (mut url, mut cookie) = (None, None);
    while (url.is_none() || cookie.is_none()) && Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                let t = line.trim();
                if t.starts_with("ws://") {
                    url = Some(t.to_string());
                } else if t.starts_with("soli_desktop=") {
                    cookie = Some(t.to_string());
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => break,
        }
    }
    let result = std::panic::catch_unwind(|| {
        let url = url.expect("the artifact printed its session URL");
        let cookie = cookie.expect("the artifact printed its cookie");
        assert!(url.ends_with("/_eui/session/gallery"), "{url}");
        // Without the cookie the gate refuses the upgrade.
        std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
        eui_client::transport::set_session_cookie(None);
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let mut d = Driver::new(1000.0, 900.0, 1.0, 0);
        let conn = eui_client::transport::connect(&url, d.hello().encode(), move || {
            let _ = wake_tx.send(());
        })
        .unwrap();
        let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pump(&mut d, &conn, &wake_rx, |d| d.session().root().is_some());
        }));
        assert!(refused.is_err(), "the gate let a cookie-less client in");
        // With it, the gallery mounts.
        eui_client::transport::set_session_cookie(Some(cookie));
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let mut d = Driver::new(1000.0, 900.0, 1.0, 0);
        let conn = eui_client::transport::connect(&url, d.hello().encode(), move || {
            let _ = wake_tx.send(());
        })
        .unwrap();
        pump(&mut d, &conn, &wake_rx, |d| d.session().root().is_some());
        assert!(texts(&d, root(&d)).iter().any(|t| t == "Nodes"));
        eui_client::transport::set_session_cookie(None);
    });
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&home);
    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn the_feeds_loading_button_spins_locally_and_settles_with_the_answer() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "feed", 700.0, 900.0);
    let _ = d.paint(700, 900);
    let label = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Load 5 000 more")).unwrap();
    let button = d.session().node(label).unwrap().parent;
    let spin = d.session().children(button)[0];
    let hidden = d.session().node(spin).unwrap().style;
    assert_eq!(d.session().style_of(spin).display, eui_proto::Display::None, "the spinner starts hidden");
    // Press: the spinner shows and the label changes before any byte comes back.
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, button);
    assert_eq!(d.session().text_of(label), Some("Loading…"));
    let showing = d.session().node(spin).unwrap().style;
    assert_ne!(showing, hidden);
    assert_eq!(d.session().style_of(spin).animation, 1, "the spinner spins");
    let list = d.paint(700, 900);
    assert!(list.wants_frame, "a spinning node keeps frames coming");
    assert!(d.next_frame_at().is_some());
    // The server's answer streams in; the provisional changes are gone and
    // the cards are there.
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    assert_eq!(d.session().node(spin).unwrap().style, hidden, "reverted on the first batch");
    assert_eq!(d.session().text_of(label), Some("Load 5 000 more"));
    // The feed is windowed (04 §7.1): five thousand more posts arrive as
    // five thousand row heights and a badge, not as cards — the tree stays
    // one window's worth. The scroll extent is the whole feed's, though.
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().text_of(ix) == Some("5010 posts")));
    assert!(d.session().live_nodes() < 3000, "{} nodes for a windowed feed", d.session().live_nodes());
    let list = d.paint(700, 900);
    assert!(!list.wants_frame, "nothing spins once the answer landed");
    let feed = d.session().preorder(root(&d)).find(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::List)).unwrap();
    let content = d.layout().content_size(feed).unwrap();
    assert!(content.h > 5000.0 * 128.0, "the extent is the whole feed: {}", content.h);
    // To the end: the landing asks for the last rows, and they come.
    d.input(Input::Key { key: "End".into(), modifiers: 0, down: true });
    d.input(Input::Key { key: "End".into(), modifiers: 0, down: false });
    let mut clock = Instant::now();
    for _ in 0..5 {
        clock += Duration::from_millis(400);
        d.tick(clock);
        let _ = d.paint(700, 900);
    }
    for f in d.take_pending() {
        conn.tx.send(f.encode()).unwrap();
    }
    let row = d.session().atom_id("row").unwrap();
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().node(ix).and_then(|n| n.prop(row)) == Some(&eui_proto::Value::Int(5009))));
    assert!(d.session().live_nodes() < 3000, "{} nodes after the last window", d.session().live_nodes());
}

#[test]
fn the_player_searches_opens_a_record_and_plays_a_track() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "music", 1100.0, 760.0);
    let _ = d.paint(1100, 760);
    let all = texts(&d, root(&d));
    assert!(all.iter().any(|t| t == "Find something to play") && all.iter().any(|t| t == "Nothing playing"), "{all:?}");
    // A suggestion runs a search: the rail fills and the pane lays the find out.
    let tile = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Nova Reyes")).unwrap();
    let seq = d.session().last_seq().unwrap();
    let tile_box = d.session().node(tile).unwrap().parent;
    click(&mut d, &conn, tile_box);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1100, 760);
    let all = texts(&d, root(&d));
    assert!(all.iter().any(|t| t == "RECORDS") && all.iter().any(|t| t.contains("for \u{201c}Nova Reyes\u{201d}")), "{all:?}");
    // Open the first record the rail found.
    let row = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Night Drive")).unwrap();
    let seq = d.session().last_seq().unwrap();
    let row_box = d.session().node(row).unwrap().parent;
    click(&mut d, &conn, row_box);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1100, 760);
    let all = texts(&d, root(&d));
    assert!(all.iter().any(|t| t == "RECORD") && all.iter().any(|t| t.contains(" tracks \u{b7} ")), "{all:?}");
    // Play the third track: the bar takes its name, and the disc spins.
    let third = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("3")).unwrap();
    let track_row = d.session().node(d.session().node(third).unwrap().parent).unwrap().parent;
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, track_row);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1100, 760);
    let all = texts(&d, root(&d));
    assert!(!all.iter().any(|t| t == "Nothing playing"), "the bar took the track: {all:?}");
    assert!(all.iter().any(|t| t.contains("keeping time only")), "and says it keeps its own time: {all:?}");
}

#[test]
fn the_player_plays_a_file_from_the_machine_and_the_bar_follows_it() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "music", 1000.0, 900.0);
    let _ = d.paint(1000, 900);
    // The welcome offers what sits in `public/music` beside the catalogue.
    let tile = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("On this machine")).expect("the machine tile");
    let tile_box = d.session().node(tile).unwrap().parent;
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, tile_box);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1000, 900);
    // Its tracks are the files. Play the first.
    let row_text = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("A chime")).expect("the file is a track");
    let row = d.session().node(d.session().node(row_text).unwrap().parent).unwrap().parent;
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, row);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1000, 900);
    // A sound is in the tree now, named by hash and told to play.
    let find_audio = |d: &Driver| d.session().preorder(root(d)).find(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Audio)).expect("an audio node");
    let playing = d.session().atom_id("playing").unwrap();
    assert_eq!(d.session().node(find_audio(&d)).and_then(|n| n.prop(playing)), Some(&eui_proto::Value::Bool(true)), "the server said play");
    // Fetch it like a picture, and it comes out of the mixer.
    for hash in d.pending_assets() {
        conn.request_asset(hash);
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while !d.audio_playing() {
        assert!(Instant::now() < deadline, "the file never arrived");
        let _ = wake.recv_timeout(Duration::from_millis(20));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    for out in d.handle_frame(Frame::decode(&bytes).unwrap()) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("{e}"),
                Incoming::Asset(hash, Ok(bytes)) => d.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => panic!("asset {hash:?}: {why}"),
            }
        }
        let _ = d.paint(1000, 900);
    }
    let mut out = vec![0.0f32; 22_050];
    let _ = d.fill_audio(&mut out, 1, 22_050);
    assert!(out.iter().any(|s| s.abs() > 0.05), "the file plays: {:?}", &out[..4]);
    // `time_update` is the only clock this application has, and it is what
    // fills in a length nothing here could read from the file itself. The
    // client emits it as it paints, at the rate 03 §7 allows.
    let _ = d.paint(1000, 900);
    let reports = d.take_pending();
    assert!(!reports.is_empty(), "a second of sound reports its position");
    for f in reports {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().text_of(ix).is_some_and(|t| t == "0:01")));
    let _ = d.paint(1000, 900);
    let all = texts(&d, root(&d));
    assert!(all.iter().any(|t| t.contains("playing here")), "and the bar says where: {all:?}");
}

/// A wheel scroll into rows the client does not have: placeholders first,
/// then the window is asked for once the scroll has settled, and the
/// cards arrive and are laid out where the placeholders were.
#[test]
fn a_wheel_scroll_into_unloaded_rows_asks_for_them_and_gets_cards() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "feed", 700.0, 900.0);
    let _ = d.paint(700, 900);
    let button = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Load 5 000 more")).unwrap();
    let button_box = d.session().node(button).unwrap().parent;
    click(&mut d, &conn, button_box);
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().text_of(ix) == Some("5010 posts")));
    let mut clock = Instant::now();
    d.tick(clock);
    let _ = d.paint(700, 900);
    for f in d.take_pending() {
        conn.tx.send(f.encode()).unwrap();
    }
    // Wheel 30 000 px down, pixel deltas as a trackpad sends them.
    d.input(Input::PointerMove(350.0, 450.0));
    for _ in 0..30 {
        d.input(Input::Wheel(0.0, 1000.0));
    }
    let feed = d.session().preorder(root(&d)).find(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::List)).unwrap();
    let sy = d.session().node(feed).unwrap().scroll.1;
    assert!(sy > 20_000, "scrolled to {sy}");
    // Right away: nothing asked, placeholders painted.
    d.tick(clock);
    let list = d.paint(700, 900);
    assert!(d.take_pending().iter().all(|f| !matches!(f, Frame::Event(e) if e.event == EventKind::Window)), "not while moving");
    let sunken = eui_render::linear(d.theme_color(eui_theme::Role::SurfaceSunken));
    assert!(list.quads.iter().any(|q| q.fill == sunken && q.rect[3] > 50.0), "placeholders where the cards will be");
    // Settled: the window is asked for, the cards come, and they are laid out.
    clock += Duration::from_millis(200);
    d.tick(clock);
    let _ = d.paint(700, 900);
    let asked = d.take_pending();
    assert!(asked.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::Window)), "{asked:?}");
    for f in asked {
        conn.tx.send(f.encode()).unwrap();
    }
    let row = d.session().atom_id("row").unwrap();
    let (first, last) = d.layout().row_window(feed, sy as f32).unwrap();
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().node(ix).and_then(|n| n.prop(row)) == Some(&eui_proto::Value::Int(i64::from(first + 5)))));
    let _ = d.paint(700, 900);
    let placed = d.layout().placed_rows(feed).unwrap().to_vec();
    assert!(placed.len() > 10 && placed.iter().all(|r| *r >= first && *r <= last), "rows placed: {placed:?} for window {first}..={last}");
    let card = d.session().preorder(root(&d)).find(|ix| d.session().node(*ix).and_then(|n| n.prop(row)) == Some(&eui_proto::Value::Int(i64::from(first + 5)))).unwrap();
    assert!(d.layout().rect(card).is_some(), "the card has a rect");
}

/// The Magic Mouse path: notched wheel steps, a real clock, no manual
/// ticking — as the window loop drives it.
#[test]
fn a_notched_wheel_scroll_gets_its_cards_on_the_real_clock() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "feed", 700.0, 900.0);
    let _ = d.paint(700, 900);
    let button = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Load 5 000 more")).unwrap();
    let button_box = d.session().node(button).unwrap().parent;
    click(&mut d, &conn, button_box);
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().text_of(ix) == Some("5010 posts")));
    // Drive it exactly as the window does: tick, paint, send, pump.
    let turn = |d: &mut Driver, conn: &eui_client::Connection| {
        d.tick(Instant::now());
        let _ = d.paint(700, 900);
        for f in d.take_pending() {
            conn.tx.send(f.encode()).unwrap();
        }
        while let Ok(msg) = conn.rx.try_recv() {
            if let Incoming::Message(bytes) = msg {
                let frame = Frame::decode(&bytes).unwrap();
                if let Frame::Error { code, message } = &frame {
                    panic!("soli error {code}: {message}");
                }
                for out in d.handle_frame(frame) {
                    conn.tx.send(out.encode()).unwrap();
                }
            }
        }
    };
    turn(&mut d, &conn);
    d.input(Input::PointerMove(350.0, 450.0));
    // Twenty notches, a frame apart, as a wheel spun fast.
    for _ in 0..20 {
        d.input(Input::WheelStep(0.0, 3.0));
        std::thread::sleep(Duration::from_millis(16));
        turn(&mut d, &conn);
    }
    // Let it land and settle, then keep turning as the loop would.
    let feed = d.session().preorder(root(&d)).find(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::List)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let row = d.session().atom_id("row").unwrap();
    loop {
        std::thread::sleep(Duration::from_millis(16));
        turn(&mut d, &conn);
        let sy = d.session().node(feed).unwrap().scroll.1 as f32;
        let placed = d.layout().placed_rows(feed).map(<[u32]>::to_vec).unwrap_or_default();
        let window = d.layout().row_window(feed, sy);
        if let Some((first, last)) = window {
            // The layout places a narrower band than the window asked for
            // (one viewport of margin above, two below); a row a little
            // past the top of the view is in it.
            let want = first + (last - first) / 2;
            if placed.contains(&want) {
                break;
            }
            assert!(Instant::now() < deadline, "row {want} never came: scroll {sy}, window {first}..={last}, placed {placed:?}");
        }
    }
    // Every row on screen has a card: no placeholder is left behind once
    // the answer landed.
    let sy = d.session().node(feed).unwrap().scroll.1 as f32;
    let tops = d.layout().row_tops(feed).unwrap().to_vec();
    let view = d.layout().rect(feed).unwrap();
    let placed = d.layout().placed_rows(feed).unwrap().to_vec();
    let on_screen: Vec<u32> = (0..tops.len() - 1).filter(|i| tops[*i] + 316.0 > sy && tops[*i] < sy + view.h).map(|i| i as u32).collect();
    assert!(!on_screen.is_empty());
    for r in &on_screen {
        assert!(placed.contains(r), "row {r} is on screen with no card: placed {placed:?}");
        let card = d.session().preorder(root(&d)).find(|ix| d.session().node(*ix).and_then(|n| n.prop(row)) == Some(&eui_proto::Value::Int(i64::from(*r)))).unwrap();
        assert!(d.layout().rect(card).is_some(), "row {r}'s card is laid out");
    }
}

/// Spec 03 §7: the gallery's chime — an `audio` node whose sound is an
/// asset Soli hashed, played by a button, ending on its own.
#[test]
fn the_gallerys_chime_is_fetched_played_and_reports_its_end() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 900.0);
    let _ = d.paint(1000, 900);
    // The node is in the tree, silent, and its sound is an asset the
    // client fetched from the session's origin.
    let find_audio = |d: &Driver| d.session().preorder(root(d)).find(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Audio)).expect("an audio node");
    let audio = find_audio(&d);
    let src = d.session().atom_id("src").unwrap();
    let playing = d.session().atom_id("playing").unwrap();
    let is_playing = move |d: &Driver| d.session().node(find_audio(d)).and_then(|n| n.prop(playing)).cloned();
    assert!(matches!(d.session().node(audio).and_then(|n| n.prop(src)), Some(eui_proto::Value::Asset(_))), "the src is a hash");
    assert_eq!(d.session().node(audio).and_then(|n| n.prop(playing)), Some(&eui_proto::Value::Bool(false)));
    // The sound is fetched like a picture: the client names the hash, the
    // window asks the session's origin for it.
    for hash in d.pending_assets() {
        conn.request_asset(hash);
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while !d.audio_playing() {
        assert!(Instant::now() < deadline, "the chime never arrived");
        let _ = wake.recv_timeout(Duration::from_millis(20));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    for out in d.handle_frame(Frame::decode(&bytes).unwrap()) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("{e}"),
                Incoming::Asset(hash, Ok(bytes)) => d.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => panic!("asset {hash:?}: {why}"),
            }
        }
        let _ = d.paint(1000, 900);
    }
    let mut out = [1.0f32; 256];
    assert!(d.fill_audio(&mut out, 1, 22_050).is_empty());
    assert!(out.iter().all(|s| *s == 0.0), "loaded, not playing");
    // Press play: the server says so, and the chime comes out.
    let label = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Play a chime")).unwrap();
    let button = d.session().node(label).unwrap().parent;
    let seq = d.session().last_seq().unwrap();
    let _ = d.paint(1000, 900);
    let r = d.layout().rect(button).expect("the button is laid out");
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    d.input(Input::PointerDown(0));
    let sent = d.input(Input::PointerUp(0));
    for f in sent {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1000, 900);
    assert_eq!(is_playing(&d), Some(eui_proto::Value::Bool(true)), "the server said play");
    d.fill_audio(&mut out, 1, 22_050);
    assert!(out.iter().any(|s| s.abs() > 0.05), "the chime: {:?}", &out[..4]);
    // Play it out — 1.6 s at 22 050 — and the end goes back to the server,
    // which stops the button.
    let mut ended = Vec::new();
    let mut buf = vec![0.0f32; 22_050];
    for _ in 0..3 {
        ended.extend(d.fill_audio(&mut buf, 1, 22_050));
    }
    assert_eq!(ended.len(), 1, "one end, once: {ended:?}");
    for f in ended {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().text_of(ix) == Some("Play a chime")));
    let _ = d.paint(1000, 900);
    assert_eq!(is_playing(&d), Some(eui_proto::Value::Bool(false)), "the server heard it end");
}

/// Spec 03 §8: the gallery's moving picture — a `video` node whose frames
/// the client decodes, sizes itself by, and advances on its own clock.
#[test]
fn the_gallerys_animation_is_decoded_sized_and_advances_on_the_clock() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 900.0);
    let mut clock = Instant::now();
    let turn = |d: &mut Driver, conn: &eui_client::Connection, clock: &mut Instant, ms: u64| {
        *clock += Duration::from_millis(ms);
        d.tick(*clock);
        let _ = d.paint(1000, 900);
        for f in d.take_pending() {
            conn.tx.send(f.encode()).unwrap();
        }
        for hash in d.pending_assets() {
            conn.request_asset(hash);
        }
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    for out in d.handle_frame(Frame::decode(&bytes).unwrap()) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("{e}"),
                Incoming::Asset(hash, Ok(bytes)) => d.asset_ready(hash, bytes),
                Incoming::Asset(_, Err(why)) => panic!("asset: {why}"),
            }
        }
    };
    // One player at a time, switched by the segmented control in the Media card.
    let tab = within(&d, "Media", "Video");
    click(&mut d, &conn, tab);
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().node(ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Video)));
    let node = |d: &Driver| d.session().preorder(root(d)).find(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Video)).expect("a video node");
    let src = d.session().atom_id("src").unwrap();
    assert!(matches!(d.session().node(node(&d)).and_then(|n| n.prop(src)), Some(eui_proto::Value::Asset(_))), "the src is a hash");
    // Fetch and decode it. It shows its first frame and plays nothing.
    let deadline = Instant::now() + Duration::from_secs(30);
    let id = d.session().node(node(&d)).unwrap().id;
    while d.video_position_ms(id).is_none() {
        assert!(Instant::now() < deadline, "the picture never arrived");
        turn(&mut d, &conn, &mut clock, 100);
    }
    assert!(!d.video_playing(), "no autoplay");
    let rect = d.layout().rect(node(&d)).expect("laid out");
    assert!((rect.w - 320.0).abs() < 0.5 && (rect.h - 180.0).abs() < 0.5, "sized by its style: {rect:?}");
    let list = d.paint(1000, 900);
    assert!(list.quads.iter().any(|q| q.params[2] as u32 == eui_render::TEXTURED_RGBA), "its first frame is drawn");
    // Press play: it advances on the client's clock and asks to be woken
    // exactly when the next frame is due.
    let play = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("▶")).map(|ix| d.session().node(ix).unwrap().parent).expect("a play button");
    click(&mut d, &conn, play);
    while !d.video_playing() {
        assert!(Instant::now() < deadline, "it never started");
        turn(&mut d, &conn, &mut clock, 100);
    }
    let due = d.next_frame_at().expect("scheduled");
    assert!(due <= clock + Duration::from_millis(100), "within a frame's delay, not a poll");
    turn(&mut d, &conn, &mut clock, 300);
    let moved = d.video_position_ms(id).expect("a position");
    assert!(moved > 100, "it advanced: {moved}");
    // Pause: it holds where it is, and the clock the server drew followed.
    let pause = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("▮▮")).map(|ix| d.session().node(ix).unwrap().parent).expect("a pause button");
    click(&mut d, &conn, pause);
    while d.video_playing() {
        assert!(Instant::now() < deadline, "it never stopped");
        turn(&mut d, &conn, &mut clock, 100);
    }
    let id = d.session().node(node(&d)).unwrap().id;
    let held = d.video_position_ms(id).expect("a position");
    turn(&mut d, &conn, &mut clock, 1_000);
    assert_eq!(d.video_position_ms(id), Some(held), "a paused picture does not advance");
}

/// Every card carries its own number, and the numbers of the cards on
/// screen are exactly the rows the list placed — so a scroll through a
/// hundred thousand of them can be checked by eye and by test.
#[test]
fn feed_cards_are_numbered_and_carry_their_media() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "feed", 700.0, 2_400.0);
    let mut clock = Instant::now();
    let settle = |d: &mut Driver, conn: &eui_client::Connection, clock: &mut Instant| {
        for _ in 0..4 {
            *clock += Duration::from_millis(200);
            d.tick(*clock);
            let _ = d.paint(700, 2_400);
            for f in d.take_pending() {
                conn.tx.send(f.encode()).unwrap();
            }
        }
    };
    settle(&mut d, &conn, &mut clock);
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().text_of(ix) == Some("#3")));
    settle(&mut d, &conn, &mut clock);
    // The numbers on screen: #1, #2, #3 … in order, one per row placed.
    let feed = d.session().preorder(root(&d)).find(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::List)).unwrap();
    let numbers: Vec<u32> = d
        .session()
        .preorder(root(&d))
        .filter_map(|ix| d.session().text_of(ix))
        .filter_map(|t| t.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()))
        .collect();
    assert!(numbers.len() >= 5, "{numbers:?}");
    assert_eq!(numbers, (1..=numbers.len() as u32).collect::<Vec<_>>(), "consecutive, none missing, none twice");
    let rows = d.layout().placed_rows(feed).unwrap();
    assert_eq!(numbers.len(), rows.len(), "one number per row placed");
    assert_eq!(numbers.first().map(|n| n - 1), rows.first().copied(), "#N is row N-1");
    // Card #1 (row 0) carries the moving picture, #4 (row 3) a picture,
    // #8 (row 7) the sound: one of each is on screen.
    let kinds: Vec<eui_proto::NodeKind> = d.session().preorder(root(&d)).filter_map(|ix| d.session().node(ix)).map(|n| n.kind).collect();
    assert!(kinds.contains(&eui_proto::NodeKind::Video), "a moving picture");
    assert!(kinds.contains(&eui_proto::NodeKind::Image), "a picture");
    assert!(d.session().preorder(root(&d)).any(|ix| d.session().text_of(ix) == Some("▶  Play the chime")), "a sound to start");
    // Pressing it puts an audio node in that card, and only there.
    assert!(!kinds.contains(&eui_proto::NodeKind::Audio), "no sound loaded until asked");
    let label = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("▶  Play the chime")).unwrap();
    let button = d.session().node(label).unwrap().parent;
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, button);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(700, 2_400);
    let audio: Vec<eui_tree::NodeIx> = d.session().preorder(root(&d)).filter(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Audio)).collect();
    assert_eq!(audio.len(), 1, "one sound, on the card that asked");
}

/// A video in the feed waits to be asked, then plays with controls: a
/// button, a bar that follows it, and a clock the server draws from the
/// client's own `time_update`.
#[test]
fn a_feed_video_waits_to_be_asked_and_then_reports_where_it_is() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, _wake) = open(port, "feed", 700.0, 1_400.0);
    let mut clock = Instant::now();
    let turn = |d: &mut Driver, conn: &eui_client::Connection, clock: &mut Instant, ms: u64| {
        *clock += Duration::from_millis(ms);
        d.tick(*clock);
        let _ = d.paint(700, 1_400);
        for f in d.take_pending() {
            conn.tx.send(f.encode()).unwrap();
        }
        for hash in d.pending_assets() {
            conn.request_asset(hash);
        }
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    for out in d.handle_frame(Frame::decode(&bytes).unwrap()) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("{e}"),
                Incoming::Asset(hash, Ok(bytes)) => d.asset_ready(hash, bytes),
                Incoming::Asset(_, Err(why)) => panic!("asset: {why}"),
            }
        }
    };
    // Card #1 carries the picture. Wait for it to arrive and decode.
    let deadline = Instant::now() + Duration::from_secs(30);
    while !d.session().preorder(root(&d)).any(|ix| d.session().node(ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Video)) {
        assert!(Instant::now() < deadline, "no video card");
        turn(&mut d, &conn, &mut clock, 100);
    }
    let node = |d: &Driver| d.session().preorder(root(d)).find(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Video)).unwrap();
    let id = d.session().node(node(&d)).unwrap().id;
    // Wait for its bytes and its decode: the player exists then.
    while d.video_position_ms(id).is_none() {
        assert!(Instant::now() < deadline, "the picture never decoded");
        turn(&mut d, &conn, &mut clock, 100);
    }
    // It does not play by itself, and it is as wide as the card allows.
    assert!(!d.video_playing(), "no autoplay");
    assert_eq!(d.video_position_ms(id), Some(0), "on its first frame");
    let rect = d.layout().rect(node(&d)).expect("laid out");
    assert!(rect.w > 300.0, "full width, not a thumbnail: {rect:?}");
    assert!((rect.h - 180.0).abs() < 0.5, "{rect:?}");
    // The controls are there: a clock at zero over the picture's length.
    assert!(d.session().preorder(root(&d)).any(|ix| d.session().text_of(ix).is_some_and(|t| t.starts_with("0:00 / 0:0"))), "a clock");
    // Press play: it starts, and says where it is four times a second.
    let button = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("▶")).map(|ix| d.session().node(ix).unwrap().parent).expect("a play button");
    click(&mut d, &conn, button);
    // The answer arrives, and a paint is what applies it.
    while !d.video_playing() {
        assert!(Instant::now() < deadline, "it never started");
        turn(&mut d, &conn, &mut clock, 100);
    }
    for _ in 0..4 {
        turn(&mut d, &conn, &mut clock, 300);
    }
    assert!(d.video_position_ms(id).is_some_and(|ms| ms > 300), "it advanced: {:?}", d.video_position_ms(id));
    // The server draws the clock from what the client reported, so it
    // follows within a round trip.
    let clocks = |d: &Driver| -> Vec<String> {
        d.session().preorder(root(d)).filter_map(|ix| d.session().text_of(ix)).filter(|t| t.contains(" / ")).map(str::to_owned).collect()
    };
    while clocks(&d).iter().all(|t| t.starts_with("0:00 /")) {
        assert!(Instant::now() < deadline, "the clock never moved: {:?}", clocks(&d));
        turn(&mut d, &conn, &mut clock, 200);
    }
    // Pause, if it is still running: it then holds where it is.
    if let Some(pause) = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("▮▮")).map(|ix| d.session().node(ix).unwrap().parent) {
        click(&mut d, &conn, pause);
        while d.video_playing() {
            assert!(Instant::now() < deadline, "it never stopped");
            turn(&mut d, &conn, &mut clock, 100);
        }
    }
    let id = d.session().node(node(&d)).unwrap().id;
    let held = d.video_position_ms(id).expect("a position");
    turn(&mut d, &conn, &mut clock, 1_000);
    assert_eq!(d.video_position_ms(id), Some(held), "paused stays put");
}

/// Spec 01 §4 end to end: a view the server cannot encode ends the session
/// with the reason, and the window says so instead of freezing on the last
/// frame it was given.
#[test]
fn a_view_that_cannot_be_encoded_ends_the_session_and_says_why() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    // Not `open`: this component never produces a tree, which is the point.
    let url = format!("ws://127.0.0.1:{port}/_eui/session/broken");
    let mut d = Driver::new(600.0, 400.0, 1.0, 0);
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let conn = connect(&url, d.hello().encode(), move || {
        let _ = wake_tx.send(());
    })
    .expect("connect to soli");
    let deadline = Instant::now() + Duration::from_secs(20);
    while d.closed().is_none() {
        assert!(Instant::now() < deadline, "the session never ended");
        let _ = wake_rx.recv_timeout(Duration::from_millis(50));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    for out in d.handle_frame(Frame::decode(&bytes).unwrap()) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("{e}"),
                Incoming::Asset(hash, Ok(bytes)) => d.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => panic!("asset {hash:?}: {why}"),
            }
        }
    }
    let why = d.closed().map(ToString::to_string).unwrap_or_default();
    assert!(why.contains("unknown event 'nope'"), "the reason travels: {why}");
    let _ = d.paint(600, 400);
    let all = texts(&d, root(&d));
    assert!(all.iter().any(|t| t == "The application stopped"), "{all:?}");
    assert!(all.iter().any(|t| t.contains("nope")), "and the window says why: {all:?}");
}

/// Spec 06 §1.1 end to end: a node asks to be woken, the client wakes it
/// on its own period with nobody touching anything, and the server counts.
#[test]
fn a_node_that_asks_to_be_woken_is_woken_without_anyone_doing_anything() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "clock", 400.0, 300.0);
    let _ = d.paint(400, 300);
    let ticks = |d: &Driver| -> i64 {
        d.session()
            .preorder(root(d))
            .filter_map(|ix| d.session().text_of(ix).and_then(|t| t.parse::<i64>().ok()))
            .next()
            .unwrap_or(-1)
    };
    assert_eq!(ticks(&d), 0, "it starts at nothing");
    // Nothing is clicked, nothing is typed: only the clock runs. The
    // client owes a frame while a wake is pending, so painting when it
    // says so is all the window does.
    let deadline = Instant::now() + Duration::from_secs(10);
    while ticks(&d) < 3 {
        assert!(Instant::now() < deadline, "the clock never ticked: {:?}", ticks(&d));
        let _ = wake.recv_timeout(Duration::from_millis(20));
        d.tick(Instant::now());
        let _ = d.paint(400, 300);
        for f in d.take_pending() {
            conn.tx.send(f.encode()).unwrap();
        }
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    for out in d.handle_frame(Frame::decode(&bytes).unwrap()) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("{e}"),
                Incoming::Asset(hash, Ok(bytes)) => d.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => panic!("asset {hash:?}: {why}"),
            }
        }
    }
    // And the client says a frame is owed for as long as the clock runs.
    assert!(d.next_frame_at().is_some(), "a running clock owes a frame");
}

/// A tile lights under the pointer and goes out when it leaves — the
/// whole exchange local to the client, and the leave as reliable as the
/// enter.
#[test]
fn a_hover_that_lights_a_tile_puts_it_out_again() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "music", 1000.0, 900.0);
    let _ = d.paint(1000, 900);
    // A suggestion lays out a wall of records.
    let tile = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Nova Reyes")).unwrap();
    let tile_box = d.session().node(tile).unwrap().parent;
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, tile_box);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1000, 900);
    let card = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Night Drive")).expect("a record tile");
    // The label sits inside the hoverable box, sometimes a column deep:
    // walk up to whatever carries the handler, the way an event does.
    let mut card_box = d.session().node(card).unwrap().parent;
    while d.session().handler(card_box, eui_proto::EventKind::PointerEnter).is_none() {
        let up = d.session().node(card_box).unwrap().parent;
        assert_ne!(up, card_box, "no hoverable ancestor");
        card_box = up;
    }
    let rest = d.session().style_of(card_box).bg;
    let r = d.layout().rect(card_box).expect("laid out");
    // Over it.
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    let _ = d.paint(1000, 900);
    let lit = d.session().style_of(card_box).bg;
    assert_ne!(lit, rest, "the pointer lights the tile");
    // Away from it, inside the window.
    d.input(Input::PointerMove(r.x + r.w / 2.0, 4.0));
    let _ = d.paint(1000, 900);
    assert_eq!(d.session().style_of(card_box).bg, rest, "and leaving puts it out");
    // And away from the window altogether.
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    let _ = d.paint(1000, 900);
    assert_ne!(d.session().style_of(card_box).bg, rest, "lit again");
    d.input(Input::PointerOut);
    let _ = d.paint(1000, 900);
    assert_eq!(d.session().style_of(card_box).bg, rest, "the pointer leaving the window puts it out too");
    // And the case that left a wall of cards lit: the pointer is over a
    // card when a frame arrives. The server never heard about the local
    // style, so its diff cannot undo it — the batch has to, and the
    // pointer's `enter` runs again on the fresh tree.
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    let _ = d.paint(1000, 900);
    assert_ne!(d.session().style_of(card_box).bg, rest, "lit under the pointer");
    let seq = d.session().last_seq().unwrap();
    for f in d.input(Input::Resized(1000.0, 880.0, 1.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1000, 880);
    let card = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Night Drive")).expect("still there");
    let mut card_box = d.session().node(card).unwrap().parent;
    while d.session().handler(card_box, eui_proto::EventKind::PointerEnter).is_none() {
        card_box = d.session().node(card_box).unwrap().parent;
    }
    d.input(Input::PointerMove(r.x + r.w / 2.0, 4.0));
    let _ = d.paint(1000, 880);
    assert_eq!(d.session().style_of(card_box).bg, rest, "not lit once the pointer has gone");
}
