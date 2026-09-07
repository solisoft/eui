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
    for expected in ["Overview", "Nodes", "62 %", "Spec", "What is EUI?", "Rename", "A tooltip", "Nothing here yet", "1 / 9"] {
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
    let track = d.session().children(d.session().node(value).unwrap().parent)[0];
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

    // Date picker: pick the 15th, then turn the month.
    let target = within(&d, "Date", "15");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "2026-09-15"));
    let target = within(&d, "Date", "›");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "October 2026"));
    assert!(has(&d, "2026-09-15"), "the pick survives turning the month");

    // Range: two clicks, the second earlier than the first — the server orders them.
    let target = within(&d, "Range", "20");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "2026-09-20 → …"));
    let target = within(&d, "Range", "10");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "2026-09-10 → 2026-09-20"));

    // Charts: four canvases carrying resolved paths; a line segment paints
    // as a rotated quad.
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
