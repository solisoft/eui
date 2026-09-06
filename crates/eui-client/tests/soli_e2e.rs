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
    let child = Command::new(bin)
        .args(["serve", &app, "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
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
                    if let Frame::Error { code, message } = &frame {
                        panic!("soli sent error {code}: {message}");
                    }
                    for out in driver.handle_frame(frame) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("connection closed: {e}"),
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
    driver.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    driver.input(Input::PointerDown(0));
    let out = driver.input(Input::PointerUp(0));
    assert!(matches!(out.as_slice(), [Frame::Event(e)] if e.event == EventKind::Click), "{out:?}");
    conn.tx.send(out[0].encode()).unwrap();

    pump(&mut driver, &conn, &wake_rx, |d| value(d).as_deref() == Some("1"));
    // The update was a diff, not a re-mount: same node ids, same count.
    assert_eq!(driver.session().live_nodes(), 9);
    assert_eq!(plus(&driver), Some(plus_ix));

    for _ in 0..2 {
        driver.input(Input::PointerDown(0));
        for f in driver.input(Input::PointerUp(0)) {
            conn.tx.send(f.encode()).unwrap();
        }
    }
    pump(&mut driver, &conn, &wake_rx, |d| value(d).as_deref() == Some("3"));

    // Resync: Soli re-sends the tree with its own state, and no definitions.
    conn.tx.send(Frame::Resync.encode()).unwrap();
    let seq = driver.session().last_seq().unwrap();
    pump(&mut driver, &conn, &wake_rx, |d| d.session().last_seq() > Some(seq));
    assert_eq!(value(&driver).as_deref(), Some("3"));
    assert!(!driver.session().is_poisoned());
}
