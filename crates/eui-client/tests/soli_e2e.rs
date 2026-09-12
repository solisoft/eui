//! The counter, served by Soli itself: `soli serve examples/demo-app` on a
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

/// A server already running, when `EUI_SOLI_PORT` names one. Lets a test be
/// pointed at the very server that is misbehaving instead of a fresh one
/// started the test's own way — the flags differ, and so can the answer.
fn existing_port() -> Option<u16> {
    std::env::var("EUI_SOLI_PORT").ok().and_then(|p| p.parse().ok())
}

/// Atrium's rooms, once per test process.
///
/// The chat suite reads a seeded room — one of its tests asserts the room is
/// deeper than a page — and a database that has never been seeded answers
/// every read with `CollectionNotFound`. That used to reach the tests as a
/// thirty-second timeout with the cause thirty lines away, on any machine
/// whose SoliDB was not the one someone had already set up by hand: a fresh
/// checkout, a build server's throwaway instance, CI.
///
/// Both steps are idempotent and are what `db/seeds.sl` documents. A fresh
/// database costs about nine seconds here, an already-seeded one about one,
/// and an operator who pointed the suite at their own server with
/// `EUI_SOLI_PORT` owns its data and is left alone.
fn seed_once(bin: &str, app: &str) {
    static SEEDED: std::sync::Once = std::sync::Once::new();
    if existing_port().is_some() {
        return;
    }
    SEEDED.call_once(|| {
        // `db:migrate` wants admin rights the demo application's credentials
        // do not have on a database nobody has provisioned, and it fails when
        // a collection already exists. Neither is fatal: what matters is that
        // the seed goes in, and writing a message makes its own collection.
        // `db:seed` is idempotent — a room that has its four thousand is left
        // alone — so this is nine seconds once and one second thereafter.
        for args in [["db:migrate", "up"], ["db:seed", ""]] {
            let name = args.join(" ");
            let name = name.trim();
            let out = std::env::temp_dir().join(format!("eui-e2e-{}.log", args[0].replace(':', "-")));
            let Ok(file) = std::fs::File::create(&out) else { continue };
            let Ok(errs) = file.try_clone() else { continue };
            let mut cmd = Command::new(bin);
            cmd.arg(args[0]).current_dir(app).stdout(Stdio::from(file)).stderr(Stdio::from(errs));
            if !args[1].is_empty() {
                cmd.arg(args[1]);
            }
            let Ok(mut child) = cmd.spawn() else {
                eprintln!("note: could not run `soli {name}`");
                continue;
            };
            // Bounded, because an unprovisioned database makes this crawl
            // rather than fail, and a suite that hangs in its first test is
            // worse than one that says what is missing. Nine seconds is the
            // seeded case; the cap is for the one that will not work anyway.
            let deadline = Instant::now() + Duration::from_secs(120);
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(200)),
                    _ => {
                        let _ = child.kill();
                        let _ = child.wait();
                        eprintln!("note: `soli {name}` outran its budget — the chat tests want a seeded room");
                        break;
                    }
                }
            }
            // Both exit 0 whatever happened, so the status says nothing and
            // the output is the only witness. Silence on success; one line
            // when it did not work, because the alternative is five chat
            // tests timing out thirty seconds apart with the reason nowhere
            // on screen.
            if let Ok(said) = std::fs::read_to_string(&out) {
                if let Some(line) = said.lines().find(|l| l.contains("Error")) {
                    eprintln!("note: `soli {name}` did not go through — {}", line.trim());
                }
            }
            let _ = std::fs::remove_file(&out);
        }
    });
}

fn start_soli(bin: &str) -> (Server, u16) {
    let app = std::env::var("EUI_SOLI_APP").unwrap_or_else(|_| format!("{}/../../examples/demo-app", env!("CARGO_MANIFEST_DIR")));
    seed_once(bin, &app);
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
        // `examples/demo-app/.env`.
        // One realtime worker, which is what this application asks for and
        // what its own `app.infos` starts it with: Atrium's room and the
        // feed's card cache are module globals, and a module global belongs
        // to the thread it is on. With several, consecutive events of one
        // session land on different threads and read different copies of it
        // — intermittently, which is the worst way for a test to fail.
        .env("SOLI_WS_WORKERS", "1")
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
                            Frame::Batch(b) => {
                                eprintln!("TRACE server -> batch seq={} ops={:?}", b.seq, b.ops.iter().map(|o| format!("{o:?}").chars().take(60).collect::<String>()).collect::<Vec<_>>())
                            }
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
    let conn = connect(&url, driver.hello().encode(), None, false, move || {
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
    open_with(port, component, w, h, 0)
}

/// [`open`] with capabilities granted, for a component that asks for one.
/// Without the grant the client refuses the dialog and says so — which is
/// the right behaviour and a confusing test failure.
fn open_with(port: u16, component: &str, w: f32, h: f32, caps: u32) -> (Driver, eui_client::Connection, mpsc::Receiver<()>) {
    let url = format!("ws://127.0.0.1:{port}/_eui/session/{component}");
    let mut driver = Driver::new(w, h, 1.0, caps);
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let conn = connect(&url, driver.hello().encode(), None, false, move || {
        let _ = wake_tx.send(());
    })
    .expect("connect to soli");
    pump(&mut driver, &conn, &wake_rx, |d| d.session().root().is_some());
    (driver, conn, wake_rx)
}

/// Pump with the client's clock running, until `until` holds.
///
/// Needle does its slow work on a wake and not in the handler that asked for
/// it (06 §1.1): the boot — a catalogue to generate, a speaker to find — and
/// every search. So what a freshly opened session shows is "Warming up", and
/// what a click on a suggestion shows is "Looking for …", each with a node
/// asking for a 100 ms clock. Only `tick` advances that clock; a driver that
/// merely paints never fires it, so a test that clicked and asserted read the
/// waiting screen and called it a failure.
fn settle(d: &mut Driver, conn: &eui_client::Connection, wake: &mpsc::Receiver<()>, w: u32, h: u32, until: impl Fn(&Driver) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut clock = Instant::now();
    while !until(d) {
        assert!(Instant::now() < deadline, "it never settled: {:?}", texts(d, root(d)));
        clock += Duration::from_millis(120);
        d.tick(clock);
        let _ = d.paint(w, h);
        for f in d.take_pending() {
            conn.tx.send(f.encode()).unwrap();
        }
        let _ = wake.recv_timeout(Duration::from_millis(20));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    let frame = Frame::decode(&bytes).expect("soli sent a well-formed frame");
                    if let Frame::Error { code, message } = &frame {
                        panic!("soli sent error {code}: {message}");
                    }
                    for out in d.handle_frame(frame) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("connection closed: {e}"),
                Incoming::Asset(hash, Ok(bytes)) => d.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => d.asset_failed(hash, why),
            }
        }
    }
    let _ = d.paint(w, h);
}

/// The player, past its boot: the one wait every music test begins with.
fn warm_up(d: &mut Driver, conn: &eui_client::Connection, wake: &mpsc::Receiver<()>, w: u32, h: u32) {
    settle(d, conn, wake, w, h, |d| texts(d, root(d)).iter().any(|t| t == "Find something to play"));
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
    let bytes = eui_client::assets::fetch(&conn.origin, &hash, None).expect("soli serves the asset");
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
    assert!(eui_client::assets::fetch(&conn.origin, &[0u8; 32], None).is_err());

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
    //
    // The best of three, because this suite runs its tests in parallel and
    // each of them starts a server: a budget is a claim about the work, and
    // the slowest of several runs is a measurement of the scheduler. On an
    // idle machine the three are within a few milliseconds of each other, and
    // when they are not it is the machine that differs and not the paint.
    let mut painted = Duration::from_secs(1);
    let mut list_draw = d.paint(800, 600);
    for _ in 0..3 {
        // A resize to the same size is the cheapest honest way to make the
        // next paint do the whole of the work again from a test.
        d.input(Input::Resized(800.0, 600.0, 1.0));
        let t = Instant::now();
        list_draw = d.paint(800, 600);
        painted = painted.min(t.elapsed());
    }
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
    let conn = connect(&url, driver.hello().encode(), None, false, move || {
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

/// The demo application, section by section. It is an ERP now — a rail, six
/// sections, and a parts distributor's figures in them — so the widgets are
/// where an application would have put them rather than all on one page, and
/// this walks to each in turn.
#[test]
fn the_gallery_mounts_and_its_widgets_respond() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 900.0);
    // Tall, so that what a section holds is inside the viewport instead of
    // scrolled away: a click is hit-tested against the page's scroller. The
    // number is a floor and not a measurement — the dashboard grows as the
    // catalogue does, and it has already outrun 2 600 once.
    for f in d.input(Input::Resized(1000.0, 3600.0, 1.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    let has = |d: &Driver, t: &str| texts(d, root(d)).iter().any(|x| x == t);

    // ---- Dashboard: where it lands.
    let all = texts(&d, root(&d));
    for expected in ["Meridian", "Revenue, month to date", "412 380 €", "62 %", "Quarter target", "Picked", "Before rebates"] {
        assert!(all.iter().any(|t| t == expected), "the dashboard shows {expected:?}");
    }
    let nodes_before = d.session().live_nodes();
    assert!(nodes_before > 120, "{nodes_before} nodes");
    let _ = d.paint(1000, 900);

    // The period control sits in the top bar, on every section. "Day", not
    // "Week": "Week" is what the section opens on, so clicking it asked the
    // server for the page it had already sent and the answer was the same
    // tree. That used to travel as a batch with no ops in it, which moved
    // the sequence and let this wait finish having tested nothing; the
    // server now says nothing when it has nothing to say.
    let day = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Day")).unwrap();
    click(&mut d, &conn, day);
    let seq = d.session().last_seq().unwrap();
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));

    // The banner opens the queue of what is late, as a sheet, and closes it.
    let open_sheet = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Open the queue")).unwrap();
    click(&mut d, &conn, open_sheet);
    pump(&mut d, &conn, &wake, |d| has(d, "Past due"));
    let close = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Close")).unwrap();
    click(&mut d, &conn, close);
    pump(&mut d, &conn, &wake, |d| !has(d, "Past due"));

    // Twelve charts — the four of "This week", the three of "Six weeks", the
    // candlestick, the Gantt, and the ranked, diverging and dumbbell rows of
    // "Where the work is" — and the spinner beside the quarter target. The
    // heatmap is not among them: its cells are boxes, so it draws no canvas.
    let paths = d.session().atom_id("paths").expect("the paths atom");
    let canvases: Vec<_> = d.session().preorder(root(&d)).filter(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Canvas)).collect();
    assert_eq!(canvases.len(), 14, "thirteen charts and the spinner");
    for c in &canvases {
        match d.session().node(*c).unwrap().prop(paths) {
            Some(eui_proto::Value::List(p)) => {
                assert!(!p.is_empty());
                assert!(p.iter().all(|path| matches!(path, eui_proto::Value::List(items) if matches!(items.get(1), Some(eui_proto::Value::Color(_))))), "colours resolved server-side");
            }
            other => panic!("{other:?}"),
        }
    }
    let list = d.paint(1000, 3600);
    assert!(list.quads.iter().any(|q| q.extra[0] != 0.0), "a segment is a rotated capsule");

    // ---- Orders: filters, a table of sixty-three, and the invoice grid.
    goto(&mut d, &conn, &wake, "Orders", "Delivery window");
    assert!(has(&d, "1 / 9"), "nine pages of seven");
    assert!(has(&d, "63 matching"));

    // The status select: closed it is its anchor, open it lists its options
    // in an overlay, and a pick closes it. Every option is also a status in
    // the table below, so "open" is a question about the overlay and not
    // about whether the word is anywhere on the page.
    assert!(!in_overlay(&d, "Draft"), "closed: the options are not in the tree");
    let target = within(&d, "Status", "Any status");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| in_overlay(d, "Draft"));
    let target = overlay_text(&d, "Invoiced");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| !in_overlay(d, "Draft"));

    // Invoiced *and* unpaid is nothing at all, and the empty state says so.
    let unpaid = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Only unpaid")).unwrap();
    click(&mut d, &conn, unpaid);
    pump(&mut d, &conn, &wake, |d| has(d, "Nothing matches"));
    let clear = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Clear filters")).unwrap();
    click(&mut d, &conn, clear);
    pump(&mut d, &conn, &wake, |d| has(d, "1 / 9"));

    // An order row opens its lines.
    let row = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("SO-24001")).unwrap();
    click(&mut d, &conn, row);
    pump(&mut d, &conn, &wake, |d| has(d, "SKU"));

    // The grid: a header click sorts by moving keyed rows; a second click on
    // an editable cell turns it into an input, and a commit patches it.
    assert!(has(&d, "FA-1001 first"));
    let target = within(&d, "Invoices", "Amount");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "FA-1004 first"));
    let target = within(&d, "Invoices", "Ada SARL");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "FA-1001 · client"));
    let target = within(&d, "Invoices", "Ada SARL");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "FA-1001 · client · editing"));
    let field = within(&d, "Invoices", "Ada SARL");
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

    // A date field opens its calendar in an overlay rather than in the page.
    let anchor = within(&d, "Delivery window", "Any day");
    click(&mut d, &conn, anchor);
    pump(&mut d, &conn, &wake, |d| has(d, "September 2026"));
    let target = within(&d, "Delivery window", "20");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "2026-09-20 →"));
    let target = within(&d, "Delivery window", "10");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "2026-09-10 → 2026-09-20"));

    // ---- Customers: a split, a tree, and an accordion.
    goto(&mut d, &conn, &wake, "Customers", "Open balance");
    assert!(has(&d, "Ada SARL"));
    let why = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Addresses")).unwrap();
    click(&mut d, &conn, why);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t.starts_with("Invoices to")));
    assert!(!texts(&d, root(&d)).iter().any(|t| t.starts_with("Camille Roy owns")), "the other section closed");
    let grace = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Grace Ltd")).unwrap();
    click(&mut d, &conn, grace);
    pump(&mut d, &conn, &wake, |d| has(d, "41 200 €"));

    // ---- Reports: the inline pickers, still the same three.
    goto(&mut d, &conn, &wake, "Reports", "Close date");
    let target = within(&d, "Close date", "15");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "2026-09-15"));
    let target = named(&d, "Close date", "Next month");
    click(&mut d, &conn, target);
    pump(&mut d, &conn, &wake, |d| has(d, "October 2026"));
    assert!(has(&d, "2026-09-15"), "the pick survives turning the month");

    // ---- Settings: the typed fields, and the slider the client captions.
    goto(&mut d, &conn, &wake, "Settings", "Legal name");
    assert!(!has(&d, "A tooltip"), "the tooltip belongs to the dashboard");
    let value = within(&d, "Low stock threshold", "Value 40");
    let track = slider_track(&d, value);
    let _ = d.paint(1000, 3600);
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

    // A field keeps what was typed across server round trips the app does
    // not care about, and a later click lands the caret in that text.
    let name_label = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Legal name")).unwrap();
    let field = d.session().children(d.session().node(name_label).unwrap().parent)[1];
    click(&mut d, &conn, field);
    // The company's name is already in it, so select it before typing: what
    // is being tested is that a local edit survives a render, not what the
    // caret does to a word it landed in the middle of.
    for f in d.input(Input::Key { key: "a".into(), modifiers: 2, down: true }) {
        conn.tx.send(f.encode()).unwrap();
    }
    for f in d.input(Input::Text("azd".into())) {
        conn.tx.send(f.encode()).unwrap();
    }
    // Somewhere else, and somewhere that moves: the period is on "Day" by
    // now, and a click on the segment already chosen asks for the tree the
    // client is holding, which the server answers with silence.
    let elsewhere = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Month")).unwrap();
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, elsewhere);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    assert_eq!(d.session().text_of(field), Some("azd"), "the server's re-render did not wipe the field");
    let _ = d.paint(1000, 3600);
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

/// The node showing `text` in the region `title` names — the smallest
/// ancestor of the title that contains it. The dashboard's cards put their
/// title in a header row beside a badge, so the title's own parent is
/// usually just that row; climbing means a caller says which *card* it
/// means without knowing how the card was built.
fn within(d: &Driver, title: &str, text: &str) -> eui_tree::NodeIx {
    let heading = d.session().preorder(root(d)).find(|ix| d.session().text_of(*ix) == Some(title)).unwrap_or_else(|| panic!("no region titled {title:?}"));
    let mut scope = d.session().node(heading).unwrap().parent;
    for _ in 0..6 {
        if let Some(hit) = d.session().preorder(scope).find(|ix| d.session().text_of(*ix) == Some(text)) {
            return hit;
        }
        let up = d.session().node(scope).unwrap().parent;
        if up == scope {
            break;
        }
        scope = up;
    }
    panic!("no {text:?} under {title:?}")
}

/// A control that draws an icon rather than a word, found by the name it
/// declares for a screen reader (03 §4) — the only thing a calendar's
/// "Next month" button says once its glyph became a vector.
fn named(d: &Driver, title: &str, name: &str) -> eui_tree::NodeIx {
    let label = d.session().atom_id("label").expect("the label atom");
    let heading = d.session().preorder(root(d)).find(|ix| d.session().text_of(*ix) == Some(title)).unwrap_or_else(|| panic!("no region titled {title:?}"));
    let mut scope = d.session().node(heading).unwrap().parent;
    for _ in 0..6 {
        let hit = d.session().preorder(scope).find(|ix| matches!(d.session().node(*ix).and_then(|n| n.prop(label)), Some(eui_proto::Value::Str(s)) if s == name));
        if let Some(hit) = hit {
            return hit;
        }
        let up = d.session().node(scope).unwrap().parent;
        if up == scope {
            break;
        }
        scope = up;
    }
    panic!("nothing named {name:?} under {title:?}")
}

/// Whether an open overlay — a select's list, a date field's calendar —
/// carries this text. The page underneath is not asked.
fn in_overlay(d: &Driver, text: &str) -> bool {
    d.session()
        .preorder(root(d))
        .filter(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Overlay))
        .any(|o| d.session().preorder(o).any(|ix| d.session().text_of(ix) == Some(text)))
}

/// That text, in the overlay that holds it.
fn overlay_text(d: &Driver, text: &str) -> eui_tree::NodeIx {
    d.session()
        .preorder(root(d))
        .filter(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::Overlay))
        .find_map(|o| d.session().preorder(o).find(|ix| d.session().text_of(*ix) == Some(text)))
        .unwrap_or_else(|| panic!("no {text:?} in an overlay"))
}

/// Click a section in the rail and wait for it to arrive. The ERP keeps the
/// section server-side, so this is one round trip and the landmark is the
/// proof it landed.
fn goto(d: &mut Driver, conn: &eui_client::Connection, wake: &mpsc::Receiver<()>, section: &str, landmark: &str) {
    let link = d.session().preorder(root(d)).find(|ix| d.session().text_of(*ix) == Some(section)).unwrap_or_else(|| panic!("no way to {section:?}"));
    click(d, conn, link);
    pump(d, conn, wake, |d| texts(d, root(d)).iter().any(|t| t == landmark));
}

#[test]
fn soli_serves_a_signed_manifest_the_client_pins() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    let (_server, port) = start_soli(&bin);
    let origin = format!("http://127.0.0.1:{port}");
    let pins = std::env::temp_dir().join(format!("eui-e2e-pins-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&pins);
    let m = eui_client::manifest::check(&origin, &pins, None).expect("a signed manifest");
    assert_eq!(m.app_id, "demo-app", "the application folder's name");
    assert_eq!((m.protocol_min, m.protocol_max), (1, 1));
    assert_eq!(m.entry, "/_eui/session");
    // `fs.pick` joined it when the messenger learned to take an attachment,
    // and the other three when it learned to use a phone. The manifest is
    // the whole list an application ever asks for and the client pins it,
    // so a capability added to a route shows up here — which is the point:
    // widening what an application may ask for should never be quiet.
    assert_eq!(eui_proto::caps::names(m.capabilities), vec!["camera", "microphone", "clipboard.read", "location", "fs.pick", "nfc"], "what config/routes.sl asked for");
    // Pinned: the same server is accepted again; a stranger's key is not.
    assert!(eui_client::manifest::check(&origin, &pins, None).is_ok());
    let pin = std::fs::read_dir(&pins).unwrap().next().unwrap().unwrap().path();
    std::fs::write(&pin, [9u8; 32]).unwrap();
    assert_eq!(eui_client::manifest::check(&origin, &pins, None).unwrap_err(), eui_client::manifest::ManifestError::KeyChanged);
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
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let mut d = Driver::new(1000.0, 900.0, 1.0, 0);
        let conn = eui_client::transport::connect(&url, d.hello().encode(), None, false, move || {
            let _ = wake_tx.send(());
        })
        .unwrap();
        let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pump(&mut d, &conn, &wake_rx, |d| d.session().root().is_some());
        }));
        assert!(refused.is_err(), "the gate let a cookie-less client in");
        // With it, the gallery mounts. The cookie belongs to this
        // connection, not to the process — which is what lets two sessions
        // share one process without presenting each other's.
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let mut d = Driver::new(1000.0, 900.0, 1.0, 0);
        let conn = eui_client::transport::connect(&url, d.hello().encode(), Some(cookie), false, move || {
            let _ = wake_tx.send(());
        })
        .unwrap();
        pump(&mut d, &conn, &wake_rx, |d| d.session().root().is_some());
        assert!(texts(&d, root(&d)).iter().any(|t| t == "Nodes"));
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
    warm_up(&mut d, &conn, &wake, 1100, 760);
    let _ = d.paint(1100, 760);
    let all = texts(&d, root(&d));
    assert!(all.iter().any(|t| t == "Find something to play") && all.iter().any(|t| t == "Nothing playing"), "{all:?}");
    // A suggestion runs a search: the rail fills and the pane lays the find out.
    let tile = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Nova Reyes")).unwrap();
    let seq = d.session().last_seq().unwrap();
    let tile_box = d.session().node(tile).unwrap().parent;
    click(&mut d, &conn, tile_box);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    settle(&mut d, &conn, &wake, 1100, 760, |d| texts(d, root(d)).iter().any(|t| t == "RECORDS"));
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

/// The editor: a buffer in the session's state, a keyboard on one box, and
/// highlighting the server computes. Typing changes the line under the
/// cursor and the bar says the file is no longer what is on disk.
#[test]
fn the_editor_types_into_its_own_source() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "editor", 1000.0, 800.0);
    let _ = d.paint(1000, 800);
    // It opened its own source, and says so.
    let all = texts(&d, root(&d));
    assert!(all.iter().any(|t| t == "app/controllers/editor_controller.sl"), "{all:?}");
    assert!(all.iter().any(|t| t == "as on disk"), "the file matches the disk: {all:?}");
    // The first line is numbered 1 and the line count is the file's.
    assert!(all.iter().any(|t| t == "1"), "a gutter number");
    assert!(all.iter().any(|t| t.ends_with(" lines")), "a line count: {all:?}");

    // One box takes the keyboard; a click anywhere in the buffer focuses it.
    let keyed = d.session().preorder(root(&d)).find(|ix| d.session().handler(*ix, eui_proto::EventKind::KeyDown).is_some()).expect("the buffer takes the keyboard");
    let r = d.layout().rect(keyed).expect("laid out");
    let seq = d.session().last_seq().unwrap();
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + 8.0));
    d.input(Input::PointerDown(0));
    for f in d.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    assert_eq!(d.focused(), Some(keyed), "clicking the buffer focuses it (03 §3)");

    // Type. The line under the cursor gains the character, and the header
    // stops claiming the file is as on disk.
    for key in ["Z", "Z", "Z"] {
        for f in d.input(Input::Key { key: key.into(), modifiers: 0, down: true }) {
            conn.tx.send(f.encode()).unwrap();
        }
    }
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t.contains("ZZZ")));
    assert!(texts(&d, root(&d)).iter().any(|t| t == "modified"), "the bar says the buffer changed");

    // Reload throws it away: the file on disk was never touched.
    let reload = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Reload")).expect("the Reload button");
    let button = d.session().node(reload).unwrap().parent;
    click(&mut d, &conn, button);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t == "as on disk"));
    assert!(!texts(&d, root(&d)).iter().any(|t| t.contains("ZZZ")), "what was typed is gone");
}

/// The tracker: what is typed is what is mixed. The pattern takes the
/// keyboard, Play walks it into eight-bit PCM on the server, and the
/// window plays the file it is handed — so a note typed here is a sample
/// out of the client's mixer three round trips later.
#[test]
fn the_tracker_types_a_note_and_plays_what_it_typed() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "tracker", 1000.0, 800.0);
    let _ = d.paint(1000, 800);
    // The demo song is there, in a pattern that shows its row numbers in hex.
    let all = texts(&d, root(&d));
    for expected in ["Play", "Instruments", "bass square", "C-2", "00"] {
        assert!(all.iter().any(|t| t == expected), "the tracker shows {expected:?}");
    }
    // The grid is one node: clicking it puts the cursor where the pointer
    // is and takes the keyboard.
    let grid = d.session().preorder(root(&d)).find(|ix| d.session().handler(*ix, eui_proto::EventKind::KeyDown).is_some()).expect("the pattern takes the keyboard");
    // Just past the row-number gutter, on the first line under the channel
    // headings: the note column of the first channel.
    let _ = d.paint(1000, 800);
    let r = d.layout().rect(grid).expect("the pattern is laid out");
    d.input(Input::PointerMove(r.x + 40.0, r.y + 20.0));
    d.input(Input::PointerDown(0));
    for f in d.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    // Focus is the client's own doing, so this waits on the focus and not on
    // the sequence: the server may well answer a click on a grid with the
    // tree it already sent, and a tree that did not change is not sent.
    pump(&mut d, &conn, &wake, |d| d.focused() == Some(grid));
    assert_eq!(d.focused(), Some(grid), "clicking a grid that wants keys focuses it (03 §3)");

    // `y` is A in FT2's upper key row, so at the default octave the cell
    // reads A-5 — a note the demo song does not have anywhere.
    let key = |d: &mut Driver, conn: &eui_client::Connection, k: &str| {
        for f in d.input(Input::Key { key: k.into(), modifiers: 0, down: true }) {
            conn.tx.send(f.encode()).unwrap();
        }
    };
    key(&mut d, &conn, "y");
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t == "A-5"));

    // The arrows move the cursor. They are also the client's scrolling keys
    // (03 §3), so this is the check that the pattern gets them at all.
    // The panel prints the cursor's row in hex beside the word "Row".
    let row_now = |d: &Driver| {
        let label = d.session().preorder(root(d)).find(|ix| d.session().text_of(*ix) == Some("Row")).expect("the Row field");
        let holder = d.session().node(label).unwrap().parent;
        d.session().preorder(holder).filter_map(|ix| d.session().text_of(ix)).find(|t| *t != "Row").map(str::to_owned)
    };
    let before = row_now(&d);
    key(&mut d, &conn, "ArrowDown");
    pump(&mut d, &conn, &wake, |d| row_now(d) != before);
    key(&mut d, &conn, "ArrowRight");
    key(&mut d, &conn, "ArrowUp");
    pump(&mut d, &conn, &wake, |d| row_now(d) == before);

    // Play: the server mixes the pattern and the tree gains a sound.
    let play = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Play")).expect("the Play button");
    let button = d.session().node(play).unwrap().parent;
    click(&mut d, &conn, button);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t.starts_with("Playing")));
    let _ = d.paint(1000, 800);
    for hash in d.pending_assets() {
        conn.request_asset(hash);
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    while !d.audio_playing() {
        assert!(Instant::now() < deadline, "the mix never arrived");
        let _ = wake.recv_timeout(Duration::from_millis(20));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    for out in d.handle_frame(Frame::decode(&bytes).unwrap()) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("{e}"),
                Incoming::Asset(hash, Ok(bytes)) => {
                    assert!(bytes.starts_with(b"RIFF"), "the server mixed a wav");
                    // The pattern is 7.68 s at 22 kHz, sixteen bits: a third
                    // of a megabyte. The placeholder the node carries before
                    // the first render is a 70 kB chime, and a client that
                    // kept playing it would look exactly like a working one.
                    assert!(bytes.len() > 200_000, "the mix, not the chime: {} bytes", bytes.len());
                    d.asset_ready(hash, bytes);
                }
                Incoming::Asset(hash, Err(why)) => panic!("asset {hash:?}: {why}"),
            }
        }
        let _ = d.paint(1000, 800);
    }
    // Whatever is already queued was reported before the mixer moved — the
    // source's own start, a millisecond in. Drop it, or the tracker is told
    // the music is where it was when it began.
    let _ = d.take_pending();
    let mut out = vec![0.0f32; 11_025];
    let _ = d.fill_audio(&mut out, 1, 11_025);
    assert!(out.iter().any(|s| s.abs() > 0.05), "the pattern is audible: {:?}", &out[..4]);
    // Filling the buffer moves the mixer; the client reports where it got
    // to as it paints, four times a second at most (03 §7), so the next
    // report has to be waited for. That report is the tracker's only clock.
    std::thread::sleep(Duration::from_millis(300));
    // The driver's clock is the window's: it moves on input and on `tick`,
    // never on its own, so a test that only paints stays inside the rate
    // limit for ever.
    d.tick(Instant::now());
    let _ = d.paint(1000, 800);
    let reports = d.take_pending();
    assert!(reports.iter().any(|f| matches!(f, Frame::Event(e) if e.event == eui_proto::EventKind::TimeUpdate)), "a second of sound reports its position: {reports:?}");
    for f in reports {
        conn.tx.send(f.encode()).unwrap();
    }

    // A second of it has gone past, and the window says so: `time_update`
    // carries the mixer's own position back, and the playhead is a row of
    // its own — "Head" beside "Row". A row is 120 ms at the demo's tempo,
    // so a second in is row 8.
    let field = |d: &Driver, name: &str| {
        let label = d.session().preorder(root(d)).find(|ix| d.session().text_of(*ix) == Some(name))?;
        let holder = d.session().node(label).unwrap().parent;
        d.session().preorder(holder).filter_map(|ix| d.session().text_of(ix)).find(|t| *t != name).map(str::to_owned)
    };
    let cursor = field(&d, "Row");
    pump(&mut d, &conn, &wake, |d| field(d, "Head").is_some_and(|h| h != "--" && h != "00"));
    assert_eq!(field(&d, "Row"), cursor, "the music moved, the cursor stayed where it was typing");

    // And it is still a tracker while it plays. Pressing Play took the
    // keyboard with it, so the pattern gets it back the way anything does,
    // with a click — which is also how a typist picks the cell to work in
    // while the song runs.
    let _ = d.paint(1000, 800);
    let r = d.layout().rect(grid).expect("the pattern is laid out");
    let seq = d.session().last_seq().unwrap();
    d.input(Input::PointerMove(r.x + 40.0, r.y + 20.0));
    d.input(Input::PointerDown(0));
    for f in d.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    // `x` is D in FT2's lower key row, and at octave 4 the cell reads D-4 —
    // a note the demo song has nowhere.
    key(&mut d, &conn, "x");
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t == "D-4"));
    assert!(
        texts(&d, root(&d)).iter().any(|t| t.starts_with("Edited")),
        "the mix in the air is older than the edit, and the status says so: {:?}",
        d.session().preorder(root(&d)).filter_map(|ix| d.session().text_of(ix)).last()
    );
}

#[test]
fn the_player_plays_a_file_from_the_machine_and_the_bar_follows_it() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "music", 1000.0, 900.0);
    warm_up(&mut d, &conn, &wake, 1000, 900);
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
    // Right away: placeholders painted, and -- the view having outrun
    // every row it holds -- the rows around it asked for at once rather
    // than when it stops (04 §7.1), so a drag sees cards, not placeholders.
    d.tick(clock);
    let list = d.paint(700, 900);
    let asked = d.take_pending();
    assert!(asked.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::Window)), "outran its rows: {asked:?}");
    let sunken = eui_render::linear(d.theme_color(eui_theme::Role::SurfaceSunken));
    assert!(list.quads.iter().any(|q| q.fill == sunken && q.rect[3] > 50.0), "placeholders where the cards will be");
    for f in asked {
        conn.tx.send(f.encode()).unwrap();
    }
    // Settled: nothing new to ask -- the window it wants is the one it
    // asked for -- and the cards come and are laid out.
    clock += Duration::from_millis(200);
    d.tick(clock);
    let _ = d.paint(700, 900);
    let again = d.take_pending();
    assert!(again.iter().all(|f| !matches!(f, Frame::Event(e) if e.event == EventKind::Window)), "asked once, not again on settling: {again:?}");
    let row = d.session().atom_id("row").unwrap();
    let (first, last) = d.layout().row_window(feed, sy as f32).unwrap();
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().node(ix).and_then(|n| n.prop(row)) == Some(&eui_proto::Value::Int(i64::from(first + 5)))));
    let _ = d.paint(700, 900);
    let placed = d.layout().placed_rows(feed).unwrap().to_vec();
    assert!(placed.len() > 10 && placed.iter().all(|r| *r >= first && *r <= last), "rows placed: {placed:?} for window {first}..={last}");
    let card = d.session().preorder(root(&d)).find(|ix| d.session().node(*ix).and_then(|n| n.prop(row)) == Some(&eui_proto::Value::Int(i64::from(first + 5)))).unwrap();
    assert!(d.layout().rect(card).is_some(), "the card has a rect");
}

/// Multi-selection over ten thousand rows the server never holds.
///
/// The first half is ordinary: tick a row, read the count. The second half is
/// the reason the widget exists — a selection keyed by row *id*, and a
/// select-all kept as one flag rather than ten thousand strings, answer for
/// rows that have not been built. So the window can move out from under the
/// selection and come back with its ticks intact, and a row the server has
/// never sent arrives already ticked.
#[test]
fn multi_selection_is_by_id_and_survives_the_window_it_scrolled_past() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 900.0);
    for f in d.input(Input::Resized(1000.0, 3600.0, 1.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    let has = |d: &Driver, t: &str| texts(d, root(d)).iter().any(|x| x == t);
    goto(&mut d, &conn, &wake, "Inventory", "Stock ledger");
    assert!(has(&d, "None selected"), "nothing is chosen to begin with");

    let id = d.session().atom_id("id").expect("the id atom");
    let role = d.session().atom_id("role").expect("the role atom");
    let selected = d.session().atom_id("selected").expect("the selected atom");
    let pos_in_set = d.session().atom_id("pos_in_set").expect("the pos_in_set atom");
    let set_size = d.session().atom_id("set_size").expect("the set_size atom");
    let row_atom = d.session().atom_id("row").expect("the row atom");
    let count_atom = d.session().atom_id("count").expect("the count atom");
    let sku_of = |d: &Driver, ix: eui_tree::NodeIx| match d.session().node(ix).and_then(|n| n.prop(id)) {
        Some(eui_proto::Value::Str(s)) => Some(s.clone()),
        _ => None,
    };
    let find_row = |d: &Driver, sku: &str| d.session().preorder(root(d)).find(|ix| sku_of(d, *ix).as_deref() == Some(sku));
    let is_on = |d: &Driver, ix: eui_tree::NodeIx| d.session().node(ix).and_then(|n| n.prop(selected)) == Some(&eui_proto::Value::Bool(true));

    // ---- One row, chosen by the id it carries, not by where it sits.
    let third = find_row(&d, "AX-0003").expect("the third part is in the first window");
    click(&mut d, &conn, third);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t == "1 of 10 000 selected"));

    let third = find_row(&d, "AX-0003").expect("and is still there afterwards");
    let node = d.session().node(third).unwrap();
    assert_eq!(node.prop(role), Some(&eui_proto::Value::Str("option".into())), "a row is an option, not a check box");
    assert_eq!(node.prop(selected), Some(&eui_proto::Value::Bool(true)));
    assert_eq!(node.prop(pos_in_set), Some(&eui_proto::Value::Int(3)), "one-based, or row zero loses the prop");
    // 03 §6.1: the size of the set *including what virtualisation left out*.
    assert_eq!(node.prop(set_size), Some(&eui_proto::Value::Int(10_000)), "the ledger, not the window");
    // An unchosen row says so rather than saying nothing: absent means "not
    // selectable", false means "selectable, not selected".
    let fourth = find_row(&d, "AX-0004").expect("its neighbour");
    assert_eq!(d.session().node(fourth).unwrap().prop(selected), Some(&eui_proto::Value::Bool(false)));

    // ---- Every row there is, as one flag.
    let all = named(&d, "Stock ledger", "Select every part in the ledger");
    click(&mut d, &conn, all);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t == "All 10 000 selected"));

    // ---- And the flag read backwards: everything except this one.
    let third = find_row(&d, "AX-0003").unwrap();
    click(&mut d, &conn, third);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t == "9 999 of 10 000 selected"));
    assert!(!is_on(&d, find_row(&d, "AX-0003").unwrap()), "the one exception");
    assert!(is_on(&d, find_row(&d, "AX-0004").unwrap()), "and nothing else");

    // ---- Now scroll past every row the server has sent.
    let list = d.session().preorder(root(&d)).find(|ix| d.session().node(*ix).and_then(|n| n.prop(count_atom)) == Some(&eui_proto::Value::Int(10_000))).expect("the ledger's windowed list");
    let r = d.layout().rect(list).expect("it is laid out");
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    for _ in 0..30 {
        d.input(Input::Wheel(0.0, 1000.0));
    }
    let mut clock = Instant::now();
    d.tick(clock);
    let _ = d.paint(1000, 900);
    for f in d.take_pending() {
        conn.tx.send(f.encode()).unwrap();
    }
    clock += Duration::from_millis(200);
    d.tick(clock);
    let _ = d.paint(1000, 900);
    for f in d.take_pending() {
        conn.tx.send(f.encode()).unwrap();
    }
    // Rows the server had never built when the flag was set. They arrive
    // ticked, because the selection answers for a row rather than storing one.
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| matches!(d.session().node(ix).and_then(|n| n.prop(row_atom)), Some(eui_proto::Value::Int(n)) if *n > 200)));
    let far: Vec<eui_tree::NodeIx> = d.session().preorder(list).filter(|ix| matches!(d.session().node(*ix).and_then(|n| n.prop(row_atom)), Some(eui_proto::Value::Int(n)) if *n > 200)).collect();
    assert!(far.len() > 5, "a window's worth of new rows, got {}", far.len());
    for ix in &far {
        assert!(is_on(&d, *ix), "row {:?} came back unticked", sku_of(&d, *ix));
    }

    // ---- And back. The exception is still the only one.
    for _ in 0..40 {
        d.input(Input::Wheel(0.0, -1000.0));
    }
    clock += Duration::from_millis(200);
    d.tick(clock);
    let _ = d.paint(1000, 900);
    for f in d.take_pending() {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| sku_of(d, ix).as_deref() == Some("AX-0003")));
    assert!(!is_on(&d, find_row(&d, "AX-0003").unwrap()), "the exception survived the round trip");
    assert!(is_on(&d, find_row(&d, "AX-0004").unwrap()));
    assert!(has(&d, "9 999 of 10 000 selected"));

    // ---- The dropdown: chips in the anchor, and a panel that stays open.
    assert!(!in_overlay(&d, "Katowice"), "the panel is shut");
    let anchor = named(&d, "Raise a purchase order", "Other warehouses");
    click(&mut d, &conn, anchor);
    pump(&mut d, &conn, &wake, |d| in_overlay(d, "Katowice"));
    let lyon = overlay_text(&d, "Lyon");
    click(&mut d, &conn, lyon);
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().filter(|t| *t == "Lyon").count() > 1);
    // Picking does not shut it -- the one behavioural difference from
    // `select`, and it lives in the handler rather than in the widget.
    assert!(in_overlay(&d, "Katowice"), "still open after a pick");
    let katowice = overlay_text(&d, "Katowice");
    click(&mut d, &conn, katowice);
    pump(&mut d, &conn, &wake, |d| {
        d.session().preorder(root(d)).any(|ix| matches!(d.session().node(ix).and_then(|n| n.prop(d.session().atom_id("label").unwrap())), Some(eui_proto::Value::Str(s)) if s == "Remove Katowice"))
    });
    assert!(in_overlay(&d, "Katowice"), "and still open after the second");
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

/// Spec 03 §3, on the real page: the gallery's content is a single column
/// inside one `scroll`, so its only "row top" is the top of the document.
/// Landing on it made `ArrowUp` a `Home` key from anywhere on the page.
#[test]
fn an_arrow_up_on_the_gallery_steps_back_rather_than_going_home() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, _conn, _wake) = open(port, "gallery", 1000.0, 900.0);
    let mut clock = Instant::now();
    // A keyed scroll eases over `motion.slow`; these are the frames it takes.
    let settle = |d: &mut Driver, clock: &mut Instant| {
        for _ in 0..40 {
            *clock += Duration::from_millis(30);
            d.tick(*clock);
            let _ = d.paint(1000, 900);
        }
    };
    let page = d.session().preorder(root(&d)).find(|ix| matches!(d.session().node(*ix).map(|n| n.kind), Some(eui_proto::NodeKind::Scroll))).expect("the page scroller");
    for _ in 0..5 {
        d.input(Input::Key { key: "ArrowDown".into(), modifiers: 0, down: true });
        settle(&mut d, &mut clock);
    }
    assert_eq!(d.session().node(page).unwrap().scroll.1, 200, "five 40 px steps down");
    d.input(Input::Key { key: "ArrowUp".into(), modifiers: 0, down: true });
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(page).unwrap().scroll.1, 160, "one step back up, not the top");
    d.input(Input::Key { key: "Home".into(), modifiers: 0, down: true });
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(page).unwrap().scroll.1, 0, "Home is still Home");
}

/// Spec 04 §7.1, on the docs dialog: the page is a windowed list, so
/// opening it costs the client one window of blocks rather than the page,
/// and scrolling asks the server for the rows that come into view.
#[test]
fn the_docs_dialog_is_a_window_of_blocks_that_scrolling_extends() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 3_600.0);
    let _ = d.paint(1000, 3_600);
    goto(&mut d, &conn, &wake, "Reports", "Month end");
    let before = d.session().live_nodes();
    let label = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Open the handbook")).expect("the button");
    let button = d.session().node(label).unwrap().parent;
    let seq = d.session().last_seq().unwrap();
    let t = Instant::now();
    click(&mut d, &conn, button);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let round_trip = t.elapsed();
    let t = Instant::now();
    let _ = d.paint(1000, 3_600);
    let first_paint = t.elapsed();
    let added = d.session().live_nodes().saturating_sub(before);
    println!("docs: round trip {:.0} ms, first paint {:.0} ms, {added} nodes added", round_trip.as_secs_f64() * 1e3, first_paint.as_secs_f64() * 1e3);
    assert!(added < 1_500, "a window of blocks, not the page: {added} nodes");
    // The list knows the whole document — its count — and holds a window.
    let count = d.session().atom_id("count").unwrap();
    let row = d.session().atom_id("row").unwrap();
    let list = d
        .session()
        .preorder(root(&d))
        .find(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::List) && d.session().node(*ix).unwrap().prop(count).is_some())
        .expect("the docs list");
    let total = match d.session().node(list).unwrap().prop(count) {
        Some(eui_proto::Value::Int(n)) => *n,
        other => panic!("{other:?}"),
    };
    let rows_of = |d: &Driver| -> Vec<i64> {
        d.session()
            .children(list)
            .iter()
            .filter_map(|c| match d.session().node(*c).and_then(|n| n.prop(row)) {
                Some(eui_proto::Value::Int(r)) => Some(*r),
                _ => None,
            })
            .collect()
    };
    let held = rows_of(&d);
    assert!(total as usize > held.len(), "{total} blocks, {} held", held.len());
    assert_eq!(held.first(), Some(&0), "the window starts at the top");
    // Scroll the list a long way: once the scroll settles the client asks for
    // the rows now in view, and the server answers with those.
    let r = d.layout().rect(list).unwrap();
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    let seq = d.session().last_seq().unwrap();
    d.input(Input::Wheel(0.0, 4_000.0));
    let mut clock = Instant::now();
    for _ in 0..40 {
        clock += Duration::from_millis(30);
        d.tick(clock);
        let _ = d.paint(1000, 3_600);
        for f in d.take_pending() {
            conn.tx.send(f.encode()).unwrap();
        }
    }
    // Two requests may be in flight — the range the mount settled on, and
    // the one the glide outran to — so wait for the rows, not for a seq.
    let _ = seq;
    pump(&mut d, &conn, &wake, |d| rows_of(d).iter().any(|r| *r > 24));
    let _ = d.paint(1000, 3_600);
    let later = rows_of(&d);
    assert!(later.iter().any(|r| *r > 24), "rows past the first window arrived: {later:?}");
    assert!(later.len() < total as usize, "and still only a window is held");
}

/// The dev bar is the server's, and it costs a production session nothing.
/// `eui_stats()` answers only under `--dev`, and with no numbers the widget
/// draws a `display: none` box — so the gallery composes it unconditionally
/// and the tree that reaches a client outside dev carries no figures at all.
#[test]
fn the_dev_bar_is_absent_from_a_session_that_is_not_in_dev_mode() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 900.0);
    let _ = d.paint(1000, 900);
    // A second render, so a bar that had numbers to draw would have them.
    // "Day", not "Week": "Week" is what the section opens on, and the server
    // has nothing to send when it is asked for the tree already on screen.
    let day = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Day")).unwrap();
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, day);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1000, 900);
    let all = texts(&d, root(&d));
    for figure in ["view", "encode", "ops", "interned"] {
        assert!(!all.iter().any(|t| t == figure), "no dev bar outside --dev: {figure:?} is on screen");
    }
}

/// The gallery's editor card: the same component code the `editor` route
/// serves, with the gallery holding the buffer. A click puts the cursor on
/// the line clicked, a keystroke is a round trip, and what comes back is the
/// line that changed — there is no text widget anywhere in it.
#[test]
fn the_gallerys_editor_takes_a_keystroke_and_gives_back_the_line() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    // Tall enough that the card is in the viewport: a click is hit-tested
    // against the page's scroller, and what is below the fold is under
    // nothing at all.
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 3_600.0);
    let _ = d.paint(1000, 3_600);
    let has = |d: &Driver, t: &str| texts(d, root(d)).iter().any(|x| x == t);
    let reads = |d: &Driver, prefix: &str| texts(d, root(d)).iter().any(|x| x.starts_with(prefix));
    goto(&mut d, &conn, &wake, "Settings", "Pricing rule");
    assert!(has(&d, "sample.sl"), "the card is in the page");
    assert!(has(&d, "unchanged"), "and says the buffer is the sample it shipped with");
    // A line the cursor is not on is tokenised, one node per token, so the
    // last line of the sample is found by its string and clicked through the
    // row that carries the handler.
    let token = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("reprice")).expect("the sample's last line");
    let line_row = d.session().node(d.session().node(token).unwrap().parent).unwrap().parent;
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, line_row);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1000, 3_600);
    assert!(reads(&d, "Ln 11,"), "the cursor moved to the line that was clicked");
    // The line under the cursor is drawn plain, in three pieces, so that the
    // caret can invert the character it sits on: what is left of the cursor
    // is one node, and that is the whole line when the cursor is at its end.
    assert!(has(&d, "reprice(all)"), "the click landed the caret at the end of the line");
    // A keystroke: `key_down` on the box that holds the buffer, and the
    // server sends back the line it changed.
    let seq = d.session().last_seq().unwrap();
    for f in d.input(Input::Key { key: "x".into(), modifiers: 0, down: true }) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1000, 3_600);
    assert!(has(&d, "reprice(all)x"), "the character landed in the buffer at the cursor");
    assert!(has(&d, "modified"), "and the bar says the buffer is no longer the sample");
    assert!(reads(&d, "Ln 11, Col 14"), "with the cursor after it");
}

/// Spec 07 §1 and 03 §5: a chart answers the pointer on its own. The band
/// under it repoints two nodes at styles its handler declared — the column
/// behind the drawing and its own value chip — and the donut's legend writes
/// the reading into the hole. Nothing goes on the wire, and nothing moves.
#[test]
fn a_chart_shows_the_value_under_the_pointer_without_a_round_trip() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    // Tall enough to have the charts in view: hit-testing stops at the
    // page's scroller, and a band below the fold is under nothing.
    let (mut d, _conn, _wake) = open(port, "gallery", 1000.0, 3_600.0);
    let keyed = |d: &Driver, key: &str| d.session().atom_id(key).and_then(|a| d.session().lookup_key(a)).unwrap_or_else(|| panic!("a node keyed {key}"));
    // Hover settles on a valid layout, so a move is worth a paint either
    // side of it; what the chunk did must still leave the wire silent.
    let hover = |d: &mut Driver, ix: eui_tree::NodeIx| {
        let _ = d.paint(1000, 3_600);
        let r = d.layout().rect(ix).expect("laid out");
        let sent = d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
        let _ = d.paint(1000, 3_600);
        let mut frames = sent;
        frames.extend(d.take_pending());
        assert!(!frames.iter().any(|f| matches!(f, Frame::Event(_))), "a hover on a chart says nothing to the server: {frames:?}");
    };
    let _ = d.paint(1000, 3_600);
    // One chip a chart: a `tip_<id>` overlay with `position: pointer`, which
    // the band's handler writes into and shows. The client places it above
    // the hand (04 §5) — a local chunk has no access to the pointer, and
    // asking the server would be a round trip a mouse sample. Measured
    // against the window, it takes the width its reading asks for rather than
    // the thirty pixels of the band that reading came from.
    //
    // Down is `display: none` and not a transparency: the top layer is
    // hit-tested first and never asks about opacity, so a chip left in the
    // layout would eat the hovers meant for whatever sits under it. Laid out
    // or not is therefore the thing to assert, and the stronger one anyway.
    //
    // A band is reached through its wash: the two are the same column of the
    // plot, one behind the drawing and one in front, so a pointer at the
    // wash's centre lands on the band above it.
    let reading = |d: &Driver, key: &str| d.session().text_of(keyed(d, key)).map(str::to_owned);
    let up = |d: &Driver, chip: eui_tree::NodeIx| d.layout().rect(chip).is_some();
    let chip = keyed(&d, "tip_bars");
    let wash = keyed(&d, "cw_bars_1");
    assert!(!up(&d, chip), "a chip is not laid out until it is asked for");
    assert_eq!(d.session().style_of(wash).bg, eui_proto::ColorRef::NONE);
    hover(&mut d, wash);
    assert!(up(&d, chip), "the tooltip is up");
    assert_eq!(reading(&d, "tt_bars").as_deref(), Some("Tue · 7"), "carrying the day and the value under the pointer");
    assert_ne!(d.session().style_of(wash).bg, eui_proto::ColorRef::NONE, "and its column is washed");
    // Above the hand and centred on it. Anchored to the band instead — and a
    // band is the full height of the plot — it sat at the plot's foot wherever
    // in the column the pointer actually was.
    let at = d.layout().rect(chip).expect("laid out");
    let band = d.layout().rect(wash).expect("laid out");
    let (px, py) = (band.x + band.w / 2.0, band.y + band.h / 2.0);
    assert!(at.y + at.h <= py, "the chip clears the pointer: {at:?} over {py}");
    assert!((at.x + at.w / 2.0 - px).abs() < 1.0, "and is centred on it: {at:?} vs {px}");
    assert!(at.y + at.h < band.y + band.h, "not parked at the foot of the plot: {at:?} vs {band:?}");
    // The candlestick scales to the extent of its lows and highs rather than
    // to a top, and the Gantt's bands run across the rows rather than down the
    // columns; both hover through the same chunk as the bars.
    let session_wash = keyed(&d, "cw_candles_1");
    hover(&mut d, session_wash);
    assert!(up(&d, keyed(&d, "tip_candles")));
    assert_eq!(reading(&d, "tt_candles").as_deref(), Some("2 · 46 +2"), "the close, and what the session did to it");
    let task_wash = keyed(&d, "cw_plan_2");
    hover(&mut d, task_wash);
    assert!(up(&d, keyed(&d, "tip_plan")), "the row's chip is up");
    assert_eq!(reading(&d, "tt_plan").as_deref(), Some("5 → 10 · 5 d"), "when the task runs, worked out server-side");
    assert_ne!(d.session().style_of(task_wash).bg, eui_proto::ColorRef::NONE, "and its row is washed");

    // The donut has no bands — an arc is not a box — so its legend is what
    // the pointer finds, and what it changes is the text in the hole.
    let hole = keyed(&d, "dv_mix");
    let name = keyed(&d, "dl_mix");
    assert_eq!(d.session().text_of(hole), Some("11"), "the total, until a row says otherwise");
    assert_eq!(d.session().text_of(name), Some("Total"));
    let row = keyed(&d, "dr_mix_1");
    hover(&mut d, row);
    assert_eq!(d.session().text_of(hole), Some("3 · 27%"), "the row's share, worked out server-side");
    assert_eq!(d.session().text_of(name), Some("Search"));
    // Leaving the band put its pair back on the way here: a chart's hover
    // is not provisional, it is undone by the leave that follows it.
    assert!(!up(&d, chip), "the tooltip is down");
    assert_eq!(d.session().style_of(wash).bg, eui_proto::ColorRef::NONE);
    let _ = d.paint(1000, 3_600);
    d.input(Input::PointerOut);
    assert_eq!(d.session().text_of(hole), Some("11"), "and the hole reads the total again");
}

/// Spec 03 §7: the gallery's chime — an `audio` node whose sound is an
/// asset Soli hashed, played by a button, ending on its own.
#[test]
fn the_gallerys_chime_is_fetched_played_and_reports_its_end() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 900.0);
    // The release card the sound sits in is near the foot of the dashboard,
    // and a click is hit-tested against the page's scroller: what is below
    // the fold is under nothing at all.
    for f in d.input(Input::Resized(1000.0, 3600.0, 1.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    let _ = d.paint(1000, 3600);
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
    // Tall enough to reach the release card the picture sits in: what is
    // below the fold is neither painted nor clickable.
    for f in d.input(Input::Resized(1000.0, 3600.0, 1.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
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
    // Both players are in the release card, silent, from the first frame:
    // a node that draws nothing until it is asked to costs a page nothing.
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
    let list = d.paint(1000, 3600);
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
    let numbers: Vec<u32> = d.session().preorder(root(&d)).filter_map(|ix| d.session().text_of(ix)).filter_map(|t| t.strip_prefix('#').and_then(|n| n.parse::<u32>().ok())).collect();
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
// Ignored, and not because of anything above it: what fails is the last
// assertion, that the server's clock follows the picture. Against a
// long-lived server it passes and the view logs `at=1300`; against a server
// this test started it does not, and the trace shows the five `video_time`
// events arriving all the same. So the report reaches the handler and does
// not reach the card, and the difference is a cold process rather than a
// race in the protocol or the client — which is where whoever picks this up
// should start. The client half is sound: the picture decodes, plays to
// 1 440 ms and stops on its own, and every assertion about it passes.
#[ignore = "the server-drawn clock does not follow a freshly started server's video; the client half passes"]
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
    for _ in 0..2 {
        turn(&mut d, &conn, &mut clock, 200);
    }
    assert!(d.video_position_ms(id).is_some_and(|ms| ms > 300), "it advanced: {:?}", d.video_position_ms(id));
    // The server draws the clock from what the client reported, so it follows
    // within a round trip — and only while the picture is running. The sample
    // is a second and a half long and the card goes back to its poster at the
    // end, clock and all, so this has to be caught in flight rather than
    // waited for afterwards.
    let clocks = |d: &Driver| -> Vec<String> { d.session().preorder(root(d)).filter_map(|ix| d.session().text_of(ix)).filter(|t| t.contains(" / ")).map(str::to_owned).collect() };
    let mut moved = false;
    while d.video_playing() && !moved {
        assert!(Instant::now() < deadline, "it never finished: {:?}", clocks(&d));
        turn(&mut d, &conn, &mut clock, 150);
        moved = clocks(&d).iter().any(|t| !t.starts_with("0:00 /"));
    }
    assert!(moved, "the clock never moved: {:?}", clocks(&d));
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
    let conn = connect(&url, d.hello().encode(), None, false, move || {
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
    let ticks = |d: &Driver| -> i64 { d.session().preorder(root(d)).filter_map(|ix| d.session().text_of(ix).and_then(|t| t.parse::<i64>().ok())).next().unwrap_or(-1) };
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
    warm_up(&mut d, &conn, &wake, 1000, 900);
    let _ = d.paint(1000, 900);
    // A suggestion lays out a wall of records.
    let tile = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Nova Reyes")).unwrap();
    let tile_box = d.session().node(tile).unwrap().parent;
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, tile_box);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    settle(&mut d, &conn, &wake, 1000, 900, |d| texts(d, root(d)).iter().any(|t| t == "Night Drive"));
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
    // Narrower, not just shorter: a render that changed nothing sends
    // nothing, and a window that only lost twenty pixels of height lays the
    // wall of records out exactly as it was — no diff, no batch, and nothing
    // for the pointer to survive.
    for f in d.input(Input::Resized(880.0, 880.0, 1.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    // `settle` and not `pump`: a `Viewport` frame is debounced on the
    // client's own clock, so a driver that only paints never gets round to
    // telling the server the window moved.
    settle(&mut d, &conn, &wake, 880, 880, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(880, 880);
    let card = d.session().preorder(root(&d)).find(|ix| d.session().text_of(*ix) == Some("Night Drive")).expect("still there");
    let mut card_box = d.session().node(card).unwrap().parent;
    while d.session().handler(card_box, eui_proto::EventKind::PointerEnter).is_none() {
        card_box = d.session().node(card_box).unwrap().parent;
    }
    d.input(Input::PointerMove(r.x + r.w / 2.0, 4.0));
    let _ = d.paint(880, 880);
    assert_eq!(d.session().style_of(card_box).bg, rest, "not lit once the pointer has gone");
}

/// 06 §1 end to end: dragging a split pane's divider moves it while the
/// hand is still down, not when the button comes up.
///
/// The bar's position is a `fraction` only the server knows, so the whole
/// chain has to work -- the press captures the pointer, each frame sends
/// the move it coalesced, the server folds it into the fraction and sends
/// the tree back. Held to the release instead, the bar sits at five
/// hundred per mille, in the middle, however far the hand has gone.
#[test]
fn a_split_pane_follows_the_hand_while_it_is_still_down() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 900.0);
    // The page is a scroll container: a viewport tall enough that the
    // split is laid out where the pointer can reach it.
    for f in d.input(Input::Resized(1000.0, 3600.0, 1.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    goto(&mut d, &conn, &wake, "Customers", "Open balance");
    let _ = d.paint(1000, 3600);

    // The dividers are the nodes that declare an orientation (03 §9).
    let orientation = d.session().atom_id("orientation").expect("the divider's a11y props");
    let bars = |d: &Driver| -> Vec<eui_tree::NodeIx> { d.session().preorder(root(d)).filter(|ix| d.session().node(*ix).is_some_and(|n| n.prop(orientation).is_some())).collect() };
    let vertical = *bars(&d).iter().find(|ix| d.layout().rect(**ix).is_some_and(|r| r.h > r.w)).expect("a vertical divider, dragged horizontally");
    let before = d.layout().rect(vertical).expect("laid out").x;

    // Press it and take the hand well to the left, a frame at a time.
    let r = d.layout().rect(vertical).unwrap();
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + 20.0));
    for f in d.input(Input::PointerDown(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    let _ = d.paint(1000, 3600);
    for f in d.take_pending() {
        conn.tx.send(f.encode()).unwrap();
    }
    for step in 1..=6u8 {
        d.input(Input::PointerMove(r.x - f32::from(step) * 20.0, r.y + 20.0));
        let _ = d.paint(1000, 3600);
        for f in d.take_pending() {
            conn.tx.send(f.encode()).unwrap();
        }
        std::thread::sleep(Duration::from_millis(20));
        while let Ok(Incoming::Message(bytes)) = conn.rx.try_recv() {
            for out in d.handle_frame(Frame::decode(&bytes).unwrap()) {
                conn.tx.send(out.encode()).unwrap();
            }
        }
    }
    // Still down: the bar has already moved. The client holds one move in
    // flight at a time, so on a busy machine — or against a heavier tree — the
    // last of them is still on the wire when the loop above ends. Waiting for
    // the answer rather than for a fixed number of milliseconds is what keeps
    // this about the drag and not about the machine.
    let moved = |d: &mut Driver| {
        let _ = d.paint(1000, 3600);
        d.layout().rect(vertical).is_some_and(|r| r.x < before - 20.0)
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !moved(&mut d) {
        assert!(Instant::now() < deadline, "the bar never followed the hand: {before} -> {}", d.layout().rect(vertical).map(|r| r.x).unwrap_or(f32::NAN));
        for f in d.take_pending() {
            conn.tx.send(f.encode()).unwrap();
        }
        let _ = wake.recv_timeout(Duration::from_millis(20));
        while let Ok(Incoming::Message(bytes)) = conn.rx.try_recv() {
            for out in d.handle_frame(Frame::decode(&bytes).unwrap()) {
                conn.tx.send(out.encode()).unwrap();
            }
        }
    }
    // And what tells a slider from a split, since the client lays a
    // slider's three parts out itself while one is dragged and must not do
    // it to anything else: the role, as a string, over the wire.
    let role = d.session().atom_id("role").expect("a11y roles");
    let is_slider = |d: &Driver, ix| d.session().node(ix).and_then(|n| n.prop(role)).is_some_and(|v| matches!(v, eui_proto::Value::Str(s) if s == "slider"));
    assert!(!is_slider(&d, d.session().node(vertical).unwrap().parent), "the split that holds this divider is not a slider");
    // And a real one, a section away, is.
    goto(&mut d, &conn, &wake, "Settings", "Low stock threshold");
    assert!(d.session().preorder(root(&d)).any(|ix| is_slider(&d, ix)), "the slider says so in its props");
}

/// Atrium takes a picture, keeps it, and shows it.
///
/// The whole path, with only the dialog and the disk played by the test:
/// the attach button asks the window for a picker, the client mints an
/// upload and streams the bytes, the server reassembles them into the
/// session's spool and posts `file_upload`, and the controller copies the
/// file under `public` — where it becomes an asset the window fetches back
/// by content hash.
///
/// It exists because this path failed three times for three different
/// reasons, each one hidden behind the last: a binary file destroyed by a
/// read/write round trip, an event whose fields sat somewhere no handler
/// looks, and a row number taken from a stale cache. None of them was
/// visible from either end alone.
#[test]
fn atrium_keeps_a_picture_someone_attached() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let kept: Option<(Server, u16)> = match existing_port() {
        Some(_) => None,
        None => Some(start_soli(&bin)),
    };
    let port = existing_port().unwrap_or_else(|| kept.as_ref().expect("started").1);
    let (mut d, conn, wake) = open_with(port, "chat", 1200.0, 900.0, eui_proto::caps::FS_PICK);
    let _ = d.paint(1200, 900);

    // One orange pixel, and a *whole* PNG — signature, header, data, end.
    //
    // It was a header and nothing else at first, which travelled and was
    // kept exactly right and then drew an empty box, because a picture that
    // cannot be decoded is not an error anywhere: the server hashes bytes
    // and the client fails to make an image of them. A test whose fixture
    // is not really a picture cannot tell that from success.
    //
    // Binary matters too: the bug this was first written for turned every
    // non-UTF-8 byte into a question mark.
    let png: Vec<u8> = vec![
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
        0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x38, 0x51, 0xa1, 0xf1, 0x1f, 0x00, 0x05, 0xdc, 0x02, 0x68, 0x57, 0xcc, 0x8a, 0xe1, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
        0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    // The paperclip: the one node that carries `pick`.
    let attach = d.session().preorder(root(&d)).find(|ix| d.session().handler(*ix, EventKind::FilePick).is_some()).expect("the composer offers a picker");
    click(&mut d, &conn, attach);
    let asks = d.take_file_asks();
    assert_eq!(asks.len(), 1, "a click on the paperclip asks for exactly one dialog: {asks:?}");

    // The person chose. The client mints an id, tells the server, and
    // streams; the test plays the disk.
    // A name this run alone will use. `public/chat` keeps what earlier runs
    // attached, and a glob that matched them picked an older, different file
    // and compared the wrong bytes.
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let name = format!("holiday-{stamp}.png");
    let (ids, out) = d.picked(asks[0].token, vec![(name.clone(), png.len() as u64)]);
    for f in out {
        conn.tx.send(f.encode()).unwrap();
    }
    for f in d.upload_chunk(ids[0], &png, true) {
        conn.tx.send(f.encode()).unwrap();
    }

    // The message lands in the room with the file's name on it. Anything
    // that went wrong instead says so in the banner, so a failure here
    // reads as the reason and not as a timeout.
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t.contains(&name)));
    let said = texts(&d, root(&d));
    assert!(!said.iter().any(|t| t.contains("did not arrive") || t.contains("could not be kept")), "the attachment was refused: {said:?}");

    // And the bytes are on disk, byte for byte, under the application —
    // which is what makes them an asset the other window can be shown.
    let app = std::env::var("EUI_SOLI_APP").unwrap_or_else(|_| format!("{}/../../examples/demo-app", env!("CARGO_MANIFEST_DIR")));
    let landed: Vec<std::path::PathBuf> = std::fs::read_dir(std::path::Path::new(&app).join("public/chat"))
        .expect("public/chat exists once something has been attached")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().ends_with(&name))
        .collect();
    assert!(!landed.is_empty(), "the picture was not kept under public/chat; the page says: {:?}", texts(&d, root(&d)));
    let on_disk = std::fs::read(&landed[0]).expect("read what was kept");
    assert_eq!(on_disk, png, "the bytes changed on the way through");

    // What is left behind is left on purpose. The message stays in the room
    // and its file stays beside it, because deleting the file and keeping
    // the message is precisely the state that used to end the session: an
    // `image` names a file by path, the server hashes it to put it on the
    // wire, and a path that is gone is a view that cannot be encoded. The
    // view tolerates it now, and a test that tidied up would be testing the
    // tidy case only.
}

/// Spec 04 §6: writing a line into a room of four thousand messages costs a
/// handful of ops, not a redraw of the room.
///
/// The river is a `list_window`, so only the rows inside the window are ever
/// built, and each one is cached under a key made of the room, its index,
/// the width and the generation — none of which a send moves. What changes
/// is the count, the foot, and the one new row. The ceiling here is loose on
/// purpose: what it guards against is the other shape, where every row in
/// the window comes back rewritten, which is a hundred ops and upwards.
#[test]
fn writing_a_line_does_not_redraw_the_room() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let kept: Option<(Server, u16)> = match existing_port() {
        Some(_) => None,
        None => Some(start_soli(&bin)),
    };
    let port = existing_port().unwrap_or_else(|| kept.as_ref().expect("started").1);
    let (mut d, conn, wake) = open(port, "chat", 1200.0, 900.0);
    let _ = d.paint(1200, 900);
    // The composer is the one field that reports what is typed into it, and
    // its arrival is what says the room is up.
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).len() > 20);
    let _ = d.paint(1200, 900);
    let hole = d.session().preorder(root(&d)).find(|ix| d.session().handler(*ix, EventKind::TextInput).is_some()).expect("the composer takes text");
    let r = d.layout().rect(hole).expect("the composer is laid out");
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    d.input(Input::PointerDown(0));
    for f in d.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }

    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let said = format!("ligne-{}", stamp % 100_000);
    for f in d.input(Input::Text(said.clone())) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| value(d).is_some_and(|v| v.contains(&said)) || texts(d, root(d)).iter().any(|t| t.contains(&said)));

    // Enter commits the field and submits it. Everything the server sends
    // back from here until the line is on screen is the cost of the send.
    for f in d.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true }) {
        conn.tx.send(f.encode()).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut widest = 0usize;
    let mut batches = 0usize;
    loop {
        // The line is in the river and the box has emptied behind it: the
        // draft node carries the same string until the send goes through,
        // so the text alone does not say it landed.
        let empty = d.session().text_of(hole).unwrap_or("").is_empty();
        if empty && texts(&d, root(&d)).iter().any(|t| t == &said) {
            break;
        }
        assert!(Instant::now() < deadline, "timed out waiting for the line to land");
        let _ = wake.recv_timeout(Duration::from_millis(50));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    let frame = Frame::decode(&bytes).expect("soli sent a well-formed frame");
                    if let Frame::Error { code, message } = &frame {
                        panic!("soli sent error {code}: {message}");
                    }
                    if let Frame::Batch(b) = &frame {
                        widest = widest.max(b.ops.len());
                        batches += 1;
                    }
                    for out in d.handle_frame(frame) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("connection closed: {e}"),
                Incoming::Asset(hash, Ok(bytes)) => d.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => d.asset_failed(hash, why),
            }
        }
        let _ = d.paint(1200, 900);
    }
    assert!(batches > 0, "the send was answered");
    assert!(widest <= 40, "a send rewrote the room: {widest} ops in one batch over {batches} batches");
}

/// A line written in one window reaches the other without that window
/// asking for anything.
///
/// The session is a WebSocket and `Batch` is S→C (01 §3): nothing ties one
/// to an `Event`, and the client applies whatever arrives. So a server that
/// knows something changed can say so, instead of leaving every window to
/// find out on its own clock — which is up to a whole `wake` period late
/// (06 §1.1) and costs a render per period per window to learn that nothing
/// happened. `eui_wake` is the trigger, and Atrium pulls it at the two
/// places that move its counter.
///
/// What makes this a test of the push and not of the clock: the listening
/// driver's clock is never advanced and not one frame it produces is sent.
/// It asks for nothing. Anything that arrives, the server sent because it
/// wanted to.
#[test]
fn a_line_reaches_the_other_window_without_it_asking() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let kept: Option<(Server, u16)> = match existing_port() {
        Some(_) => None,
        None => Some(start_soli(&bin)),
    };
    let port = existing_port().unwrap_or_else(|| kept.as_ref().expect("started").1);
    let (mut writer, w_conn, w_wake) = open(port, "chat", 1200.0, 900.0);
    let (mut reader, r_conn, r_wake) = open(port, "chat", 1200.0, 900.0);
    let _ = writer.paint(1200, 900);
    let _ = reader.paint(1200, 900);
    pump(&mut writer, &w_conn, &w_wake, |d| texts(d, root(d)).len() > 20);
    pump(&mut reader, &r_conn, &r_wake, |d| texts(d, root(d)).len() > 20);
    let _ = writer.paint(1200, 900);
    let _ = reader.paint(1200, 900);

    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let said = format!("poussee-{}", stamp % 100_000);
    let hole = writer.session().preorder(root(&writer)).find(|ix| writer.session().handler(*ix, EventKind::TextInput).is_some()).expect("the composer takes text");
    let r = writer.layout().rect(hole).expect("the composer is laid out");
    writer.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    writer.input(Input::PointerDown(0));
    for f in writer.input(Input::PointerUp(0)) {
        w_conn.tx.send(f.encode()).unwrap();
    }
    for f in writer.input(Input::Text(said.clone())) {
        w_conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut writer, &w_conn, &w_wake, |d| value(d).is_some_and(|v| v.contains(&said)) || texts(d, root(d)).iter().any(|t| t.contains(&said)));
    let sent = Instant::now();
    for f in writer.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true }) {
        w_conn.tx.send(f.encode()).unwrap();
    }

    // The reader is mute from here: no clock, no frames out, only frames in.
    let deadline = sent + Duration::from_secs(20);
    loop {
        if texts(&reader, root(&reader)).iter().any(|t| t == &said) {
            break;
        }
        assert!(Instant::now() < deadline, "the line never arrived: the other window was told nothing, and it asked for nothing");
        let _ = r_wake.recv_timeout(Duration::from_millis(10));
        while let Ok(msg) = r_conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    let frame = Frame::decode(&bytes).expect("soli sent a well-formed frame");
                    if let Frame::Error { code, message } = &frame {
                        panic!("soli sent error {code}: {message}");
                    }
                    let _ = reader.handle_frame(frame);
                }
                Incoming::Closed(e) => panic!("connection closed: {e}"),
                Incoming::Asset(hash, Ok(bytes)) => reader.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => reader.asset_failed(hash, why),
            }
        }
        // The writer keeps its own socket moving; what the reader gets is
        // not owed to anything the reader did.
        let _ = w_wake.recv_timeout(Duration::from_millis(1));
        while let Ok(msg) = w_conn.rx.try_recv() {
            if let Incoming::Message(bytes) = msg {
                if let Ok(frame) = Frame::decode(&bytes) {
                    for out in writer.handle_frame(frame) {
                        w_conn.tx.send(out.encode()).unwrap();
                    }
                }
            }
        }
    }
    eprintln!("the other window had it {} ms later", sent.elapsed().as_millis());
}

/// A room opens on its last hundred messages, and says what it is holding
/// back.
///
/// `count` is one wire number per message, sent with the list and read by
/// the client to size the scrollbar — so a room of four thousand costs four
/// thousand of them to put a hundred on screen, and gives a thumb too small
/// to grab. The rest of the room is one click away and still in the
/// database; it is simply not on the wire until it is asked for.
#[test]
fn a_room_opens_on_a_hundred_and_unrolls_on_request() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let kept: Option<(Server, u16)> = match existing_port() {
        Some(_) => None,
        None => Some(start_soli(&bin)),
    };
    let port = existing_port().unwrap_or_else(|| kept.as_ref().expect("started").1);
    let (mut d, conn, wake) = open(port, "chat", 1200.0, 900.0);
    let _ = d.paint(1200, 900);
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().node(ix).map(|n| n.kind) == Some(eui_proto::NodeKind::List)));

    let river = |d: &Driver| -> u32 {
        let count = d.session().atom_id("count").expect("the count atom");
        d.session()
            .preorder(root(d))
            .filter(|ix| d.session().node(*ix).map(|n| n.kind) == Some(eui_proto::NodeKind::List))
            .find_map(|ix| match d.session().node(ix)?.prop(count) {
                Some(eui_proto::Value::Int(n)) => Some(*n as u32),
                _ => None,
            })
            .expect("the river says how long it is")
    };
    // A page, not the room. Not an exact hundred: the suite's other tests
    // write into this same room, and a message that lands between the
    // window opening and this line is one more row the river honestly has.
    let opened = river(&d);
    assert!((100..200).contains(&opened), "a page, not the room, got {opened}");

    // The rest is offered, and the offer says how much there is.
    let said = texts(&d, root(&d));
    let offer = said.iter().find(|t| t.ends_with("earlier messages")).expect("the room says what it is holding back: {said:?}");
    let held: u32 = offer.split(' ').next().and_then(|n| n.parse().ok()).expect("a number of messages");
    assert!(held > 100, "the seeded room is deeper than one page: {offer}");

    let button = d
        .session()
        .preorder(root(&d))
        .find(|ix| d.session().text_of(*ix).is_some_and(|t| t.ends_with("earlier messages")))
        .map(|ix| d.session().node(ix).unwrap().parent)
        .expect("the offer is a button");
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, button);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1200, 900);
    let unrolled = river(&d);
    assert!(unrolled >= opened + 100, "one more page: {opened} then {unrolled}");
    assert!(unrolled < opened + 200, "one page, not the whole room: {opened} then {unrolled}");
    let after = texts(&d, root(&d)).iter().find(|t| t.ends_with("earlier messages")).cloned().expect("still holding some back");
    let left: u32 = after.split(' ').next().and_then(|n| n.parse().ok()).unwrap();
    assert!(left <= held - 100, "and the offer counted down by a page: {held} then {left}");
}

/// Spec 03 §3.3 and 06 §1.2 against a real server: a tag read off a label
/// reaches the composer, and a page that asks where the machine is is told.
///
/// Neither has a platform behind it on a desktop, which is exactly why this
/// is worth having: the driver's half — the activation rule, the capability,
/// the coarsening, the clock — is the half the security of both rests on,
/// and it is the same half on a phone.
#[test]
fn a_tag_and_a_fix_reach_the_room_they_were_asked_for() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let kept: Option<(Server, u16)> = match existing_port() {
        Some(_) => None,
        None => Some(start_soli(&bin)),
    };
    let port = existing_port().unwrap_or_else(|| kept.as_ref().expect("started").1);
    let caps = eui_proto::caps::FS_PICK | eui_proto::caps::CAMERA | eui_proto::caps::LOCATION | eui_proto::caps::NFC;
    let (mut d, conn, wake) = open_with(port, "chat", 1200.0, 900.0, caps);
    let _ = d.paint(1200, 900);
    pump(&mut d, &conn, &wake, |d| d.session().preorder(root(d)).any(|ix| d.session().text_of(ix) == Some("≋")));

    // The composer's four tools, by the glyph each draws.
    let tool = |d: &Driver, glyph: &str, kind: EventKind| -> eui_tree::NodeIx {
        let mut ix = d.session().preorder(root(d)).find(|ix| d.session().text_of(*ix) == Some(glyph)).unwrap_or_else(|| panic!("no {glyph} in the composer"));
        while d.session().handler(ix, kind).is_none() {
            let up = d.session().node(ix).unwrap().parent;
            assert_ne!(up, ix, "no ancestor of {glyph} handles {kind:?}");
            ix = up;
        }
        ix
    };

    // ---- the reader. A scan starts on the activation and on nothing else.
    assert!(d.take_nfc_asks().is_empty(), "the tree arrived and started nothing");
    let reader = tool(&d, "≋", EventKind::NfcTag);
    click(&mut d, &conn, reader);
    let asks = d.take_nfc_asks();
    assert_eq!(asks.len(), 1, "one scan, from one activation: {asks:?}");
    assert_eq!(asks[0].prompt, "Hold your phone near the label");

    // The platform read one. On a desktop there is none, so the test is it.
    let label = "https://eui.example/crate/4471";
    for f in d.scanned(asks[0].token, "04:a2:1f:7b", &[eui_client::NfcRecord { kind: "uri".into(), payload: label.into() }]) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| texts(d, root(d)).iter().any(|t| t.contains(label)));

    // ---- the radio. Nothing asks until the page says so.
    assert!(!d.wants_location(), "no node has asked to be placed");
    let pin = tool(&d, "◎", EventKind::Click);
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, pin);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1200, 900);
    assert!(d.wants_location(), "the chip is in the tree and it carries `locate`");

    // A fix off a receiver, precise to four metres. What reaches the server
    // is three decimal places and an accuracy that admits as much.
    // Painted in the loop, not only waited on: a `location` is emitted from
    // a paint, as a `wake` is, so a wait that only reads frames waits for
    // something nothing is going to produce.
    d.located(eui_client::Fix { latitude: 48.858_372_1, longitude: 2.294_481_9, accuracy_m: 4.0 });
    let deadline = Instant::now() + Duration::from_secs(20);
    while !texts(&d, root(&d)).iter().any(|t| t.contains("48.858")) {
        assert!(Instant::now() < deadline, "the fix never reached the room");
        d.tick(Instant::now());
        let _ = d.paint(1200, 900);
        for f in d.take_pending() {
            conn.tx.send(f.encode()).unwrap();
        }
        let _ = wake.recv_timeout(Duration::from_millis(40));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    let frame = Frame::decode(&bytes).expect("soli sent a well-formed frame");
                    if let Frame::Error { code, message } = &frame {
                        panic!("soli sent error {code}: {message}");
                    }
                    for out in d.handle_frame(frame) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("connection closed: {e}"),
                Incoming::Asset(hash, Ok(bytes)) => d.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => d.asset_failed(hash, why),
            }
        }
    }
    let said = texts(&d, root(&d));
    assert!(said.iter().any(|t| t.contains("48.858") && t.contains("2.294")), "the room was told where it is: {said:?}");
    assert!(!said.iter().any(|t| t.contains("48.8583")), "and not more than that: {said:?}");

    // Turning it off takes the node out of the tree, and the radio with it.
    let seq = d.session().last_seq().unwrap();
    click(&mut d, &conn, pin);
    pump(&mut d, &conn, &wake, |d| d.session().last_seq() > Some(seq));
    let _ = d.paint(1200, 900);
    assert!(!d.wants_location(), "the clock stops when its prop goes (06 §1.2)");
}

/// Spec 06 §6 against the real server: a card is picked up out of one column
/// and put down in another, and the board comes back with it there.
///
/// The whole of what the server does is reorder a list of ids. Everything
/// about the hand — the slop, the slot under the pointer, which column is
/// under it — was resolved here before a single frame went out, and what went
/// out was three events.
#[test]
fn a_card_is_carried_from_one_column_to_another() {
    let Ok(bin) = std::env::var("EUI_SOLI_BIN") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (_server, port) = start_soli(&bin);
    let (mut d, conn, wake) = open(port, "gallery", 1000.0, 900.0);
    for f in d.input(Input::Resized(1000.0, 3600.0, 1.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    let has = |d: &Driver, t: &str| texts(d, root(d)).iter().any(|x| x == t);
    pump(&mut d, &conn, &wake, |d| has(d, "The week's work"));
    let _ = d.paint(1000, 3600);

    let keyed = |d: &Driver, key: &str| d.session().atom_id(key).and_then(|a| d.session().lookup_key(a)).unwrap_or_else(|| panic!("a node keyed {key}"));
    // Everything under a column, so "which column is this card in" is a
    // question the test can ask without knowing any ids.
    let column_of = |d: &Driver, card: &str| {
        // A poll runs between batches as well as after them, so a card may
        // legitimately be in neither column for an instant.
        let Some(ix) = d.session().atom_id(card).and_then(|a| d.session().lookup_key(a)) else {
            return "in the air".to_owned();
        };
        let mut cur = d.session().node(ix).map(|n| n.parent);
        while let Some(p) = cur.filter(|p| p.is_some()) {
            let names = texts(d, p);
            for want in ["Backlog", "In progress", "Done"] {
                if names.first().map(String::as_str) == Some(want) {
                    return want.to_owned();
                }
            }
            cur = d.session().node(p).map(|n| n.parent);
        }
        "nowhere".to_owned()
    };

    assert_eq!(column_of(&d, "t1"), "Backlog", "it starts where the board put it");
    let card = keyed(&d, "t1");
    let onto = keyed(&d, "t6");

    // Grab it, carry it over the card in the last column, and let go.
    let from = d.layout().rect(card).expect("laid out");
    let to = d.layout().rect(onto).expect("laid out");
    for f in d.input(Input::PointerMove(from.x + from.w / 2.0, from.y + from.h / 2.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    for f in d.input(Input::PointerDown(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    for f in d.input(Input::PointerMove(to.x + to.w / 2.0, to.y + to.h / 2.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    let _ = d.paint(1000, 3600);
    // The ghost is the one thing the hand carries that the board does not: an
    // overlay with `position: pointer`, which the client keeps under the
    // cursor without laying anything out again (04 §5). It is shown by the
    // grab's own local chunk, so it is up before the server has answered.
    let ghost = keyed(&d, "kan_ghost");
    let at = d.layout().rect(ghost).expect("the ghost is laid out while a card is held");
    assert!(at.w > 0.0 && at.h > 0.0, "and has a box: {at:?}");
    assert!((at.x + at.w / 2.0 - (to.x + to.w / 2.0)).abs() < 2.0, "centred on the hand: {at:?}");
    assert_eq!(d.session().text_of(keyed(&d, "kan_ghost_t")), Some("Reconcile October stock"), "carrying the card it holds");
    for f in d.take_pending() {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| column_of(d, "t1") == "Done");

    for f in d.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    // The announcement is the server's, through a live region — the client
    // does not announce, because announcing is prose and prose is content.
    pump(&mut d, &conn, &wake, |d| d.session().text_of(keyed(d, "kan_say")).is_some_and(|t| t.contains("moved to Done")));
    assert_eq!(column_of(&d, "t1"), "Done", "and it stayed there");
    let _ = d.paint(1000, 3600);
    assert!(d.layout().rect(keyed(&d, "kan_ghost")).is_none(), "the ghost went with the drop");

    // And back the other way. A drag is not a direction: the columns are a
    // row, so right to left is the same gesture over different boxes, and the
    // one that is easy to get wrong.
    let back = keyed(&d, "t4");
    assert_eq!(column_of(&d, "t4"), "In progress", "where it started");
    let onto = keyed(&d, "t2");
    let from = d.layout().rect(back).expect("laid out");
    let to = d.layout().rect(onto).expect("laid out");
    for f in d.input(Input::PointerMove(from.x + from.w / 2.0, from.y + from.h / 2.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    for f in d.input(Input::PointerDown(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    for f in d.input(Input::PointerMove(to.x + to.w / 2.0, to.y + to.h / 2.0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    let _ = d.paint(1000, 3600);
    for f in d.take_pending() {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| column_of(d, "t4") == "Backlog");
    for f in d.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| d.session().text_of(keyed(d, "kan_say")).is_some_and(|t| t.contains("moved to Backlog")));
    assert_eq!(column_of(&d, "t4"), "Backlog", "and it stayed there too");
}
