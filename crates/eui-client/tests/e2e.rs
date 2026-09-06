//! End to end without a window: the counter server on a loopback socket, the
//! real transport thread, the real driver. A click leaves as an event frame
//! and the new value comes back as a batch. This is the milestone the plan
//! called "the counter runs end to end", minus the pixels on glass — which
//! `eui-render`'s tests cover separately.
//!
//! Its own binary because it sets `EUI_ALLOW_INSECURE_LOOPBACK`, which the
//! driver tests deliberately leave unset.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use eui_client::{connect, Driver, Incoming, Input};
use eui_proto::{EventKind, Frame};

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

/// Pump incoming messages into the driver until `until` holds or time runs out.
fn pump(driver: &mut Driver, conn: &eui_client::Connection, wake: &mpsc::Receiver<()>, until: impl Fn(&Driver) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !until(driver) {
        assert!(Instant::now() < deadline, "timed out waiting on the server");
        let _ = wake.recv_timeout(Duration::from_millis(50));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    let frame = Frame::decode(&bytes).expect("server sent a well-formed frame");
                    for out in driver.handle_frame(frame) {
                        conn.tx.send(out.encode()).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("connection closed: {e}"),
            }
        }
    }
}

fn value(d: &Driver) -> Option<String> {
    let ix = d.session().lookup(2)?;
    d.session().text_of(ix).map(str::to_owned)
}

#[test]
fn the_counter_runs_end_to_end_over_a_real_socket() {
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let url = start_server();
    let mut driver = Driver::new(400.0, 300.0, 1.0, 0);
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let wake_tx = Arc::new(wake_tx);
    let conn = connect(&url, driver.hello().encode(), move || {
        let _ = wake_tx.send(());
    })
    .expect("connect");

    // Welcome and the mount arrive; the tree shows 0.
    pump(&mut driver, &conn, &wake_rx, |d| value(d).is_some());
    assert_eq!(value(&driver).as_deref(), Some("0"));

    // Find the "+" button by laying out, click it, ship the event.
    let _ = driver.paint(400, 300);
    let plus = driver.session().lookup(4).unwrap();
    let r = driver.layout().rect(plus).unwrap();
    let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    driver.input(Input::PointerMove(x, y));
    driver.input(Input::PointerDown(0));
    let out = driver.input(Input::PointerUp(0));
    assert!(matches!(out.as_slice(), [Frame::Event(e)] if e.event == EventKind::Click && e.node == 4), "{out:?}");
    let click = out[0].encode();
    assert!(click.len() < 40, "a click is {} bytes on the wire", click.len());
    conn.tx.send(click).unwrap();

    // The server answers with a SetText; the driver applies it and wants a redraw.
    pump(&mut driver, &conn, &wake_rx, |d| value(d).as_deref() == Some("1"));
    assert!(driver.needs_redraw());
    let list = driver.paint(400, 300);
    assert!(!list.quads.is_empty());

    // Twice more, then the other button once: 2.
    for _ in 0..2 {
        driver.input(Input::PointerDown(0));
        for f in driver.input(Input::PointerUp(0)) {
            conn.tx.send(f.encode()).unwrap();
        }
    }
    pump(&mut driver, &conn, &wake_rx, |d| value(d).as_deref() == Some("3"));
    let minus = driver.session().lookup(6).unwrap();
    let r = driver.layout().rect(minus).unwrap();
    driver.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    driver.input(Input::PointerDown(0));
    for f in driver.input(Input::PointerUp(0)) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut driver, &conn, &wake_rx, |d| value(d).as_deref() == Some("2"));

    // A resync brings the tree back with the server's value, not 0.
    conn.tx.send(Frame::Resync.encode()).unwrap();
    let seq_before = driver.session().last_seq().unwrap();
    pump(&mut driver, &conn, &wake_rx, |d| d.session().last_seq() > Some(seq_before));
    assert_eq!(value(&driver).as_deref(), Some("2"));
}

#[test]
fn a_forged_event_ends_the_session() {
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let url = start_server();
    let mut driver = Driver::new(400.0, 300.0, 1.0, 0);
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let conn = connect(&url, driver.hello().encode(), move || {
        let _ = wake_tx.send(());
    })
    .unwrap();
    pump(&mut driver, &conn, &wake_rx, |d| value(d).is_some());

    // An event on the value text, which has no handler: the server refuses it.
    let forged = Frame::Event(eui_proto::EventFrame { node: 2, event: EventKind::Click, name: 1, payload: eui_proto::Value::Null });
    conn.tx.send(forged.encode()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline);
        let _ = wake_rx.recv_timeout(Duration::from_millis(50));
        match conn.rx.try_recv() {
            Ok(Incoming::Message(b)) => {
                if let Ok(Frame::Error { code, .. }) = Frame::decode(&b) {
                    assert_eq!(code, 300);
                    break;
                }
            }
            Ok(Incoming::Closed(_)) => panic!("closed before the error frame"),
            Err(_) => {}
        }
    }
}
