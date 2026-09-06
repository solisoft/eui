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
    let deadline = Instant::now() + Duration::from_secs(10);
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
    // Leaving the button restores its base style, locally.
    driver.input(Input::PointerMove(1.0, 1.0));
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
    let _ = d.paint(1000, 900);
}

/// The node showing `text` inside the titled card `title` — the card is the
/// title's parent, so two calendars showing "15" never collide.
fn within(d: &Driver, title: &str, text: &str) -> eui_tree::NodeIx {
    let heading = d.session().preorder(root(d)).find(|ix| d.session().text_of(*ix) == Some(title)).unwrap_or_else(|| panic!("no card titled {title:?}"));
    let card = d.session().node(heading).unwrap().parent;
    d.session().preorder(card).find(|ix| d.session().text_of(*ix) == Some(text)).unwrap_or_else(|| panic!("no {text:?} under {title:?}"))
}
