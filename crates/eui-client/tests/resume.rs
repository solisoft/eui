//! Spec 01 §4.1: a socket that breaks is not an application that ended.
//!
//! Half of this is the driver's, and needs no network: what it offers a
//! server on the second socket, and what it does with either answer. The
//! other half is the whole of it — a real socket, dropped mid-session, and
//! the tree still standing when the next one opens.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use eui_client::{connect, Connection, Driver, Incoming, Input};
use eui_proto::*;

// ------------------------------------------------------------ the driver

fn tree(seq: u64) -> Batch {
    let col = StyleRecord { display: Display::Column, gap: 4, ..Default::default() };
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    t.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 0, key: 0, text: Some(TextRef::Inline("rows".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    Batch { seq, ops: vec![Op::DefStyle { id: 1, record: col }, Op::Mount(t)] }
}

fn mounted(session: [u8; 16]) -> Driver {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: 1, session, resumed: false })).is_empty());
    assert_eq!(d.handle_frame(Frame::Batch(tree(1))), vec![Frame::Ack { seq: 1 }]);
    d
}

#[test]
fn the_first_hello_offers_nothing() {
    let d = Driver::new(400.0, 300.0, 1.0, 0);
    let Frame::Hello(h) = d.hello() else { panic!() };
    assert_eq!(h.resume, None);
}

#[test]
fn a_later_hello_offers_the_session_and_the_last_batch_applied() {
    let d = mounted([5; 16]);
    let Frame::Hello(h) = d.hello() else { panic!() };
    assert_eq!(h.resume, Some(Resume { session: [5; 16], acked: 1 }));
}

/// The server is the one that decides. A client that kept its tree against
/// a server which has forgotten the session would answer clicks the server
/// cannot place.
#[test]
fn a_welcome_that_did_not_resume_takes_the_tree_with_it() {
    let mut d = mounted([5; 16]);
    assert!(d.session().lookup(2).is_some());
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [6; 16], resumed: false })).is_empty());
    assert!(d.session().lookup(2).is_none(), "the tree from the old session is gone");
    assert_eq!(d.acked(), 0, "and so is what it had applied");
    // The new session mounts from seq 1 again, which the old `acked`
    // would otherwise have swallowed.
    assert_eq!(d.handle_frame(Frame::Batch(tree(1))), vec![Frame::Ack { seq: 1 }]);
    assert!(d.session().lookup(2).is_some());
}

#[test]
fn a_resume_of_a_session_the_client_never_had_ends_it() {
    let mut d = mounted([5; 16]);
    let out = d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [9; 16], resumed: true }));
    assert!(matches!(&out[0], Frame::Error { code: 103, .. }), "{out:?}");
    assert!(d.closed().is_some());
}

/// A replay covers what the socket may have dropped, and may well cover
/// what it did not: applying an `InsertChild` twice would double a row.
#[test]
fn a_batch_already_applied_is_acked_and_not_applied_again() {
    let mut d = mounted([5; 16]);
    let mut sub = Subtree::default();
    sub.nodes.push(FlatNode { kind: NodeKind::Text, id: 3, style: 0, key: 0, text: Some(TextRef::Inline("one".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    let insert = Batch { seq: 2, ops: vec![Op::InsertChild { parent: 1, index: 0, subtree: sub }] };
    assert_eq!(d.handle_frame(Frame::Batch(insert.clone())), vec![Frame::Ack { seq: 2 }]);
    let before = d.session().children(d.session().lookup(1).unwrap()).len();
    assert_eq!(d.handle_frame(Frame::Batch(insert)), vec![Frame::Ack { seq: 2 }], "acked again");
    assert_eq!(d.session().children(d.session().lookup(1).unwrap()).len(), before, "and not applied again");
}

// -------------------------------------------------------- a real socket

fn start_server() -> String {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(listener.local_addr().unwrap()).unwrap();
            counter_server::serve(listener).await;
        });
    });
    format!("ws://{}", rx.recv().unwrap())
}

fn pump(driver: &mut Driver, conn: &Connection, wake: &mpsc::Receiver<()>, until: impl Fn(&Driver) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !until(driver) {
        assert!(Instant::now() < deadline, "timed out waiting on the server");
        let _ = wake.recv_timeout(Duration::from_millis(50));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    for out in driver.handle_frame(Frame::decode(&bytes).expect("well-formed")) {
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

fn value(d: &Driver) -> Option<String> {
    d.session().lookup(2).and_then(|ix| d.session().text_of(ix)).map(str::to_owned)
}

fn dial(url: &str, driver: &Driver) -> (Connection, mpsc::Receiver<()>) {
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let wake_tx = Arc::new(wake_tx);
    let conn = connect(url, driver.hello().encode(), None, false, move || {
        let _ = wake_tx.send(());
    })
    .expect("connect");
    (conn, wake_rx)
}

#[test]
fn a_session_survives_the_socket_it_was_opened_on() {
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let url = start_server();
    let mut driver = Driver::new(400.0, 300.0, 1.0, 0);
    let (conn, wake) = dial(&url, &driver);
    pump(&mut driver, &conn, &wake, |d| value(d).is_some());

    // Count to two, so the session has state a fresh one would not.
    for _ in 0..2 {
        let _ = driver.paint(400, 300);
        let r = driver.layout().rect(driver.session().lookup(4).unwrap()).unwrap();
        driver.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
        driver.input(Input::PointerDown(0));
        for f in driver.input(Input::PointerUp(0)) {
            conn.tx.send(f.encode()).unwrap();
        }
    }
    pump(&mut driver, &conn, &wake, |d| value(d).as_deref() == Some("2"));
    let session = driver.session_id().expect("the server named the session");

    // The network goes away. Nothing tells the driver, because nothing on
    // the wire says so: the socket is simply dropped.
    drop(conn);
    drop(wake);
    // The server parks the session when its socket closes; a client's
    // first attempt is 300 ms later, and this stands in for that wait.
    std::thread::sleep(Duration::from_millis(300));

    // A second socket, offering the session back.
    let Frame::Hello(h) = driver.hello() else { panic!() };
    assert_eq!(h.resume, Some(Resume { session, acked: driver.acked() }));
    let (conn, wake) = dial(&url, &driver);

    // The tree was never torn down, and the count is where it was: a
    // server that had forgotten the session would have sent a mount
    // showing 0.
    pump(&mut driver, &conn, &wake, |d| d.session_id() == Some(session));
    assert_eq!(value(&driver).as_deref(), Some("2"));

    // And it is the same session, still counting from two.
    let _ = driver.paint(400, 300);
    let r = driver.layout().rect(driver.session().lookup(4).unwrap()).unwrap();
    driver.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    driver.input(Input::PointerDown(0));
    for f in driver.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut driver, &conn, &wake, |d| value(d).as_deref() == Some("3"));
}
