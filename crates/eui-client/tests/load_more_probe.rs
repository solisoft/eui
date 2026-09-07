//! Throwaway: where do the seconds of "Load 5 000 more" go?
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]
use std::sync::mpsc;
use std::time::{Duration, Instant};
use eui_client::{Driver, Input};
use eui_client::transport::Incoming;
use eui_proto::Frame;

#[test]
fn probe() {
    let Ok(url) = std::env::var("EUI_PROBE_URL") else { return };
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let mut d = Driver::new(700.0, 900.0, 1.0, 0);
    let conn = eui_client::transport::connect(&url, d.hello().encode(), move || { let _ = wake_tx.send(()); }).unwrap();
    let t0 = Instant::now();
    let pump = |d: &mut Driver, until: &dyn Fn(&Driver) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !until(d) {
            assert!(Instant::now() < deadline, "timeout");
            let _ = wake_rx.recv_timeout(Duration::from_millis(20));
            while let Ok(msg) = conn.rx.try_recv() {
                match msg {
                    Incoming::Message(bytes) => {
                        let n = bytes.len();
                        let frame = Frame::decode(&bytes).unwrap();
                        let ops = if let Frame::Batch(b) = &frame { b.ops.len() } else { 0 };
                        let t = Instant::now();
                        for out in d.handle_frame(frame) { conn.tx.send(out.encode()).unwrap(); }
                        eprintln!("PROBE +{:.2}s batch {} bytes, {} ops, applied in {:.1} ms", t0.elapsed().as_secs_f64(), n, ops, t.elapsed().as_secs_f64() * 1e3);
                    }
                    Incoming::Closed(e) => panic!("{e}"),
                    Incoming::Asset(h, Ok(b)) => d.asset_ready(h, b),
                    Incoming::Asset(h, Err(e)) => d.asset_failed(h, e),
                }
            }
        }
    };
    pump(&mut d, &|d| d.session().root().is_some());
    eprintln!("PROBE mounted after {:.2}s, {} nodes", t0.elapsed().as_secs_f64(), d.session().live_nodes());
    let _ = d.paint(700, 900);
    let root = d.session().root().unwrap();
    let btn = d.session().preorder(root).find(|ix| d.session().text_of(*ix) == Some("Load 5 000 more")).unwrap();
    let r = d.layout().rect(btn).unwrap();
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    d.input(Input::PointerDown(0));
    let t1 = Instant::now();
    for f in d.input(Input::PointerUp(0)) { conn.tx.send(f.encode()).unwrap(); }
    let before = d.session().live_nodes();
    pump(&mut d, &|d| d.session().live_nodes() > before);
    eprintln!("PROBE load more: first batch after {:.2}s", t1.elapsed().as_secs_f64());
    pump(&mut d, &|d| d.session().live_nodes() >= 193_000);
    eprintln!("PROBE load more: all batches after {:.2}s, {} nodes", t1.elapsed().as_secs_f64(), d.session().live_nodes());
    // A second click: the cards are cached on the server now.
    let _ = d.paint(700, 900);
    let t2 = Instant::now();
    d.input(Input::PointerDown(0));
    for f in d.input(Input::PointerUp(0)) { conn.tx.send(f.encode()).unwrap(); }
    pump(&mut d, &|d| d.session().live_nodes() >= 289_000);
    eprintln!("PROBE third 5 000: all batches after {:.2}s", t2.elapsed().as_secs_f64());
}
