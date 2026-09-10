//! The process boundary of spec 08 §10, end to end: a real worker process
//! (this crate's `eui` binary), the counter server on a loopback socket,
//! the real transport. A click leaves as an event frame, the new value
//! comes back as a batch, and the draw list that crosses the pipe is the
//! one an in-process driver paints. On Linux the worker is confined and
//! the self-tests show what that refuses.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eui_client::worker::Backend;
use eui_client::{connect, Driver, Incoming, Input};

fn eui_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_eui"))
}

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

/// Pump incoming messages through the backend until `until` holds.
fn pump(backend: &mut Backend, conn: &eui_client::Connection, wake: &mpsc::Receiver<()>, mut until: impl FnMut(&mut Backend) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !until(backend) {
        assert!(Instant::now() < deadline, "timed out waiting on the server; closed={:?} quads={}", backend.closed(), backend.paint(320, 240).0.quads.len());
        let _ = wake.recv_timeout(Duration::from_millis(50));
        while let Ok(msg) = conn.rx.try_recv() {
            match msg {
                Incoming::Message(bytes) => {
                    for out in backend.frame(bytes) {
                        conn.tx.send(out).unwrap();
                    }
                }
                Incoming::Closed(e) => panic!("connection closed: {e}"),
                Incoming::Asset(hash, Ok(bytes)) => backend.asset_ready(hash, bytes),
                Incoming::Asset(hash, Err(why)) => backend.asset_failed(hash, why),
            }
        }
    }
}

fn open(backend: &mut Backend, url: &str) -> (eui_client::Connection, mpsc::Receiver<()>) {
    let (wake_tx, wake_rx) = mpsc::channel::<()>();
    let hello = backend.hello();
    let conn = connect(url, hello, None, false, move || {
        let _ = wake_tx.send(());
    })
    .expect("connect");
    (conn, wake_rx)
}

/// The value text is painted as glyph quads; the draw list changes when
/// the value does (the digits share a width, so it is the atlas
/// coordinates that differ). Two paints that differ mean a batch landed.
fn quads(backend: &mut Backend) -> Vec<eui_render::Quad> {
    backend.paint(320, 240).0.quads
}

#[test]
fn the_counter_runs_through_a_worker_process() {
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let url = start_server();
    let (mut backend, how) = Backend::open_with(eui_binary(), 320.0, 240.0, 1.0, 0);
    eprintln!("{how}");
    assert!(matches!(backend, Backend::Remote { .. }), "a worker started: {how}");
    #[cfg(target_os = "linux")]
    assert!(how.contains("landlock") && how.contains("seccomp"), "confined on Linux: {how}");
    let (conn, wake) = open(&mut backend, &url);
    // The mount arrives and paints.
    pump(&mut backend, &conn, &wake, |b| quads(b).len() > 5);
    let before = quads(&mut backend);
    assert!(backend.closed().is_none());
    // Click "+": the button is the second child of the row that is the
    // root's third child; an in-process driver tells us where it is.
    let mut probe = Driver::new(320.0, 240.0, 1.0, 0);
    let (probe_conn, probe_wake) = {
        let (tx, rx) = mpsc::channel::<()>();
        let c = connect(&url, probe.hello().encode(), None, false, move || {
            let _ = tx.send(());
        })
        .unwrap();
        (c, rx)
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while probe.session().root().is_none() {
        assert!(Instant::now() < deadline);
        let _ = probe_wake.recv_timeout(Duration::from_millis(50));
        while let Ok(Incoming::Message(b)) = probe_conn.rx.try_recv() {
            probe.handle_frame(eui_proto::Frame::decode(&b).unwrap());
        }
    }
    let _ = probe.paint(320, 240);
    let root = probe.session().root().unwrap();
    let row = probe.session().children(root)[2];
    let plus = probe.session().children(row)[1];
    let r = probe.layout().rect(plus).unwrap();
    backend.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    backend.input(Input::PointerDown(0));
    let out = backend.input(Input::PointerUp(0));
    assert_eq!(out.len(), 1, "the click left as one event frame");
    assert!(matches!(eui_proto::Frame::decode(&out[0]), Ok(eui_proto::Frame::Event(_))));
    for f in out {
        conn.tx.send(f).unwrap();
    }
    pump(&mut backend, &conn, &wake, |b| quads(b) != before);
    let after = quads(&mut backend);
    // The same session in this process paints the same list.
    let (mut local, _) = Backend::open_with(PathBuf::from("/nonexistent/eui"), 320.0, 240.0, 1.0, 0);
    assert!(matches!(local, Backend::Local(_)));
    let (lconn, lwake) = open(&mut local, &url);
    pump(&mut local, &lconn, &lwake, |b| quads(b).len() > 5);
    assert_eq!(quads(&mut local), before, "in-process and worker paint the same mount");
    local.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    local.input(Input::PointerDown(0));
    for f in local.input(Input::PointerUp(0)) {
        lconn.tx.send(f).unwrap();
    }
    pump(&mut local, &lconn, &lwake, |b| quads(b) == after);
    // The accessibility tree crosses too.
    let tree = backend.access_tree();
    assert!(tree.nodes.iter().any(|n| n.role == eui_client::a11y::AccessRole::Button && n.label == "+"), "{tree:?}");
}

#[test]
fn a_hostile_frame_ends_the_session_and_the_window_stands() {
    let (mut backend, _) = Backend::open_with(eui_binary(), 100.0, 100.0, 1.0, 0);
    assert!(matches!(backend, Backend::Remote { .. }));
    let _ = backend.hello();
    assert!(backend.closed().is_none());
    // Garbage where a frame should be: the driver refuses it, the session
    // is closed, and the worker is still there to say so.
    let out = backend.frame(vec![0xff; 40]);
    assert!(out.is_empty());
    let why = backend.closed().expect("closed");
    assert!(why.contains("bad frame"), "{why}");
    // 01 §4: a window that stopped talking says so — the driver replaces
    // the tree with its own notice, so the worker still has something to
    // paint and the pipe still carries a list.
    let (list, _) = backend.paint(100, 100);
    assert!(!list.quads.is_empty(), "the notice is painted, and nothing panicked");
    let tree = backend.access_tree();
    assert!(tree.nodes.iter().any(|n| n.label == "The application stopped"), "{tree:?}");
}

#[test]
fn a_dead_worker_is_reported_not_fatal() {
    let (mut backend, _) = Backend::open_with(eui_binary(), 100.0, 100.0, 1.0, 0);
    let Backend::Remote { worker, .. } = &mut backend else { panic!("a worker") };
    // Take the worker down from outside, as a crash would: the pipe closes
    // when the last handle to it goes.
    if let Ok(mut w) = worker.lock() {
        w.kill_for_test();
    }
    let _ = backend.hello();
    let (list, _) = backend.paint(100, 100);
    assert!(list.quads.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn the_sandbox_refuses_files_sockets_and_processes() {
    use std::process::Command;
    let run = |what: &str| Command::new(eui_binary()).arg(eui_client::worker::SELFTEST_ARG).arg(what).output().unwrap();
    let none = run("none");
    let report = String::from_utf8_lossy(&none.stderr);
    assert!(none.status.success(), "{report}");
    // Landlock reports what the *kernel* would take: a build asking for a
    // newer ABI than the kernel implements gets "partly enforced", which
    // is an honest answer and not a failure. The behavioural half below is
    // the real check either way; seccomp is not negotiable.
    assert!(report.contains("landlock: files and sockets denied") || report.contains("landlock: partly enforced"), "{report}");
    assert!(report.contains("seccomp"), "{report}");
    for what in ["fs", "net", "exec"] {
        let out = run(what);
        let err = String::from_utf8_lossy(&out.stderr);
        // Refused by Landlock (exit 0, "refused") or killed by seccomp (a
        // signal): either way it did not go through.
        assert_ne!(out.status.code(), Some(3), "{what} went through: {err}");
        assert!(!err.contains("went through"), "{what}: {err}");
        eprintln!("{what}: {:?} {}", out.status, err.trim());
    }
}

/// One application dying takes its own worker with it and nothing else.
///
/// This is the property the shell rests on: several tabs in one window,
/// each with its own confined worker, and a crash in one of them — a
/// seccomp kill, a panic, an OOM — that the others never learn about. They
/// share a process, so it has to be *shown* that they share no fate.
#[test]
fn a_crashed_worker_leaves_its_neighbours_untouched() {
    let (mut doomed, _) = Backend::open_with(eui_binary(), 100.0, 100.0, 1.0, 0);
    let (mut bystander, _) = Backend::open_with(eui_binary(), 100.0, 100.0, 1.0, 0);
    let (Backend::Remote { .. }, Backend::Remote { .. }) = (&doomed, &bystander) else {
        eprintln!("no worker on this platform; skipping");
        return;
    };
    let _ = doomed.hello();
    let _ = bystander.hello();
    assert!(doomed.closed().is_none() && bystander.closed().is_none());

    // Take one worker down from outside, as a crash would.
    let Backend::Remote { worker, .. } = &mut doomed else { panic!("a worker") };
    if let Ok(mut w) = worker.lock() {
        w.kill_for_test();
    }
    let (dead_list, _) = doomed.paint(100, 100);
    assert!(dead_list.quads.is_empty(), "the dead one paints nothing");

    // The other one is asked for everything a live tab is asked for. None
    // of it goes through the dead one's pipe, and none of it panics.
    assert!(bystander.closed().is_none(), "the survivor was not closed by its neighbour's death");
    let _ = bystander.input(Input::PointerMove(10.0, 10.0));
    let (list, _) = bystander.paint(100, 100);
    let _ = list;
    let _ = bystander.access_tree();
    assert!(bystander.worker_status().is_some(), "the survivor's worker is still answering");
    assert!(bystander.closed().is_none(), "and it still has a session");
}
