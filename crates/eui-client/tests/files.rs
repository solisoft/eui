//! Spec 03 §3.2 and 01 §6: a file the person picks, and a file the server
//! is asked for.
//!
//! No window and no filesystem here — the driver's whole part is to decide
//! that a dialog may open, to frame what comes back, and to refuse what
//! nobody asked for. That is the part the security of this feature rests
//! on, and it is testable without either.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_client::{Driver, FileAsk, FileWant, Input};
use eui_proto::limits::MAX_TRANSFER_CHUNK_BYTES;
use eui_proto::*;

const ATOM_ATTACH: u32 = 1;
const ATOM_EXPORT: u32 = 2;
const ATOM_PICK: u32 = 3;
const ATOM_SAVE: u32 = 4;

const ATTACH: u32 = 2;
const EXPORT: u32 = 3;

/// box(1) [ attach(2) with `pick`, export(3) with `save` ].
fn tree() -> Batch {
    let col = StyleRecord { display: Display::Column, gap: 4, ..Default::default() };
    let button = StyleRecord { display: Display::Row, padding: [3; 4], min_width: Dim::Px(80), min_height: Dim::Px(24), ..Default::default() };
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: ATTACH, style: 2, key: 0, text: None, props: (0, 1), handlers: (0, 1), child_count: 0 });
    t.props.push((ATOM_PICK, Value::List(vec![Value::Str("csv,txt".into()), Value::Int(1), Value::Int(512 * 1024)])));
    t.handlers.push((EventKind::FilePick, Handler::Server(ATOM_ATTACH)));
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: EXPORT, style: 2, key: 0, text: None, props: (1, 1), handlers: (1, 1), child_count: 0 });
    t.props.push((ATOM_SAVE, Value::Str("export.csv".into())));
    t.handlers.push((EventKind::FileSave, Handler::Server(ATOM_EXPORT)));
    Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: ATOM_ATTACH, value: "attach".into() },
            Op::DefAtom { id: ATOM_EXPORT, value: "export".into() },
            Op::DefAtom { id: ATOM_PICK, value: "pick".into() },
            Op::DefAtom { id: ATOM_SAVE, value: "save".into() },
            Op::DefStyle { id: 1, record: col },
            Op::DefStyle { id: 2, record: button },
            Op::Mount(t),
        ],
    }
}

fn driver(granted: u32) -> Driver {
    let mut d = Driver::new(400.0, 300.0, 1.0, granted);
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [1; 16], resumed: false })).is_empty());
    assert_eq!(d.handle_frame(Frame::Batch(tree())), vec![Frame::Ack { seq: 1 }]);
    d
}

/// Click the middle of a node.
fn click(d: &mut Driver, id: u32) -> Vec<Frame> {
    let _ = d.paint(400, 300);
    let r = d.layout().rect(d.session().lookup(id).unwrap()).unwrap();
    let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let mut out = d.input(Input::PointerMove(x, y));
    out.extend(d.input(Input::PointerDown(0)));
    out.extend(d.input(Input::PointerUp(0)));
    out
}

fn one_ask(d: &mut Driver) -> FileAsk {
    let asks = d.take_file_asks();
    assert_eq!(asks.len(), 1, "{asks:?}");
    asks.into_iter().next().unwrap()
}

#[test]
fn a_click_on_a_node_carrying_pick_asks_the_window_for_a_dialog() {
    let mut d = driver(caps::FS_PICK);
    click(&mut d, ATTACH);
    let ask = one_ask(&mut d);
    assert_eq!(ask.node, ATTACH);
    assert_eq!(ask.want, FileWant::Open { accept: "csv,txt".into(), multiple: true, max: 512 * 1024 });
}

/// The capability is the whole of what stands between a tree and a dialog.
#[test]
fn without_the_capability_nothing_opens() {
    let mut d = driver(0);
    click(&mut d, ATTACH);
    assert!(d.take_file_asks().is_empty());
    click(&mut d, EXPORT);
    assert!(d.take_file_asks().is_empty());
}

/// And neither does anything else: a tree cannot open a dialog by being
/// sent, only by being activated.
#[test]
fn a_tree_that_arrives_opens_nothing() {
    let mut d = driver(caps::FS_PICK | caps::FS_SAVE);
    let _ = d.paint(400, 300);
    assert!(d.take_file_asks().is_empty());
}

#[test]
fn what_is_picked_becomes_an_event_and_then_chunks() {
    let mut d = driver(caps::FS_PICK);
    click(&mut d, ATTACH);
    let ask = one_ask(&mut d);
    let (ids, out) = d.picked(ask.token, vec![("/home/someone/books/rows.csv".into(), 6)]);
    assert_eq!(ids.len(), 1);
    let Frame::Event(e) = &out[0] else { panic!("{out:?}") };
    assert_eq!((e.node, e.event, e.name), (ATTACH, EventKind::FilePick, ATOM_ATTACH));
    // 06 §3: the name, never the path it came from.
    assert_eq!(e.payload, Value::List(vec![Value::Int(i64::from(ids[0])), Value::Str("rows.csv".into()), Value::Int(6)]));

    let out = d.upload_chunk(ids[0], b"a,b,c\n", true);
    assert_eq!(out, vec![Frame::Upload(Transfer { id: ids[0], seq: 0, flag: Chunked::Last, bytes: b"a,b,c\n".to_vec() })]);
    // The upload is over: a chunk after the last one is not framed.
    assert!(d.upload_chunk(ids[0], b"more", true).is_empty());
}

#[test]
fn a_long_file_is_cut_into_chunks_that_fit_a_frame() {
    let mut d = driver(caps::FS_PICK);
    click(&mut d, ATTACH);
    let ask = one_ask(&mut d);
    let (ids, _) = d.picked(ask.token, vec![("big.csv".into(), (MAX_TRANSFER_CHUNK_BYTES + 10) as u64)]);
    let bytes = vec![7u8; MAX_TRANSFER_CHUNK_BYTES + 10];
    let out = d.upload_chunk(ids[0], &bytes, true);
    assert_eq!(out.len(), 2);
    let (Frame::Upload(a), Frame::Upload(b)) = (&out[0], &out[1]) else { panic!() };
    assert_eq!((a.seq, a.flag, a.bytes.len()), (0, Chunked::More, MAX_TRANSFER_CHUNK_BYTES));
    assert_eq!((b.seq, b.flag, b.bytes.len()), (1, Chunked::Last, 10));
}

/// The node said half a megabyte. What the person chose is theirs to choose;
/// what leaves the machine is the tree's to bound.
#[test]
fn a_file_past_the_ceiling_is_announced_and_then_aborted() {
    let mut d = driver(caps::FS_PICK);
    click(&mut d, ATTACH);
    let ask = one_ask(&mut d);
    let (ids, out) = d.picked(ask.token, vec![("huge.csv".into(), 2_000_000)]);
    assert!(matches!(&out[0], Frame::Event(e) if e.event == EventKind::FilePick));
    let Frame::Upload(t) = &out[1] else { panic!("{out:?}") };
    assert_eq!((t.id, t.flag), (ids[0], Chunked::Abort));
    assert!(d.upload_chunk(ids[0], b"x", true).is_empty(), "and it accepts no bytes");
}

#[test]
fn a_dismissed_dialog_is_not_an_event() {
    let mut d = driver(caps::FS_PICK);
    click(&mut d, ATTACH);
    let ask = one_ask(&mut d);
    d.dialog_dismissed(ask.token);
    assert!(d.take_pending().is_empty());
    // And the token is spent: an answer to it now says nothing.
    let (ids, out) = d.picked(ask.token, vec![("late.csv".into(), 1)]);
    assert!(ids.is_empty() && out.is_empty());
}

#[test]
fn a_save_asks_the_server_for_the_bytes_and_they_arrive_as_writes() {
    let mut d = driver(caps::FS_SAVE);
    click(&mut d, EXPORT);
    let ask = one_ask(&mut d);
    assert_eq!(ask.want, FileWant::Save { name: "export.csv".into() });

    let out = d.saving(ask.token, "somewhere/else/rows.csv".into());
    let Frame::Event(e) = &out[0] else { panic!("{out:?}") };
    assert_eq!((e.node, e.event, e.name), (EXPORT, EventKind::FileSave, ATOM_EXPORT));
    assert_eq!(e.payload, Value::Str("rows.csv".into()), "the name, not the path");

    assert!(d.handle_frame(Frame::Blob(Transfer { id: EXPORT, seq: 0, flag: Chunked::More, bytes: b"a,b\n".to_vec() })).is_empty());
    assert!(d.handle_frame(Frame::Blob(Transfer { id: EXPORT, seq: 1, flag: Chunked::Last, bytes: b"1,2\n".to_vec() })).is_empty());
    let writes = d.take_writes();
    assert_eq!(writes.len(), 2);
    assert_eq!((writes[0].token, writes[0].flag, &writes[0].bytes[..]), (ask.token, Chunked::More, &b"a,b\n"[..]));
    assert_eq!(writes[1].flag, Chunked::Last);
}

/// The one that matters: a server may not write a file nobody asked for.
#[test]
fn a_blob_for_a_save_nobody_asked_for_ends_the_session() {
    let mut d = driver(caps::FS_SAVE);
    let out = d.handle_frame(Frame::Blob(Transfer { id: EXPORT, seq: 0, flag: Chunked::Last, bytes: b"gotcha".to_vec() }));
    assert!(matches!(&out[0], Frame::Error { code: 104, .. }), "{out:?}");
    assert!(d.closed().is_some());
    assert!(d.take_writes().is_empty(), "and nothing is owed to a disk");
}

/// A gap in the chunks loses the save rather than writing a file with a
/// hole in it.
#[test]
fn a_blob_out_of_order_aborts_the_save() {
    let mut d = driver(caps::FS_SAVE);
    click(&mut d, EXPORT);
    let ask = one_ask(&mut d);
    let _ = d.saving(ask.token, "rows.csv".into());
    assert!(d.handle_frame(Frame::Blob(Transfer { id: EXPORT, seq: 3, flag: Chunked::More, bytes: b"x".to_vec() })).is_empty());
    let writes = d.take_writes();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].flag, Chunked::Abort);
    assert!(d.closed().is_none(), "the save is lost; the session is not");
}

// -------------------------------------------------------- a real socket
//
// The same two gestures against the reference server, over the real
// transport: the bytes of a file the person picked reach it, and the bytes
// it owes a save come back.

use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use eui_client::{connect, Connection, Incoming};

/// The counter server's file nodes (`examples/counter-server`).
const SERVER_ATTACH: u32 = 11;
const SERVER_EXPORT: u32 = 13;
const SERVER_NOTE: u32 = 15;

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

/// Drain whatever the socket has, then answer `until`. Draining first
/// matters: what is already in the channel is part of the answer.
fn pump(driver: &mut Driver, conn: &Connection, wake: &mpsc::Receiver<()>, mut until: impl FnMut(&mut Driver) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
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
        if until(driver) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting on the server");
        let _ = wake.recv_timeout(Duration::from_millis(20));
    }
}

fn note(d: &Driver) -> Option<String> {
    d.session().lookup(SERVER_NOTE).and_then(|ix| d.session().text_of(ix)).map(str::to_owned)
}

#[test]
fn a_file_reaches_the_server_and_what_it_owes_comes_back() {
    std::env::set_var("EUI_ALLOW_INSECURE_LOOPBACK", "1");
    let url = start_server();
    let mut d = Driver::new(500.0, 400.0, 1.0, caps::FS_PICK | caps::FS_SAVE);
    let (wake_tx, wake) = mpsc::channel::<()>();
    let wake_tx = Arc::new(wake_tx);
    let conn = connect(&url, d.hello().encode(), None, false, move || {
        let _ = wake_tx.send(());
    })
    .expect("connect");
    pump(&mut d, &conn, &wake, |d| note(d).is_some());

    // Attach. The window's part — the dialog and the disk — is played by
    // the test; everything else is the client's.
    click(&mut d, SERVER_ATTACH);
    let ask = one_ask(&mut d);
    let (ids, out) = d.picked(ask.token, vec![("/tmp/rows.csv".into(), 6)]);
    for f in out {
        conn.tx.send(f.encode()).unwrap();
    }
    for f in d.upload_chunk(ids[0], b"a,b,c\n", true) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| note(d).as_deref() == Some("Attached rows.csv — 6 bytes"));

    // Save. The server answers the event with the bytes themselves.
    click(&mut d, SERVER_EXPORT);
    let ask = one_ask(&mut d);
    for f in d.saving(ask.token, "counter.txt".into()) {
        conn.tx.send(f.encode()).unwrap();
    }
    pump(&mut d, &conn, &wake, |d| note(d).as_deref() == Some("Saved as counter.txt"));
    let mut written: Vec<eui_client::FileWrite> = Vec::new();
    pump(&mut d, &conn, &wake, |d| {
        written.extend(d.take_writes());
        written.iter().any(|w| w.flag == Chunked::Last)
    });
    let bytes: Vec<u8> = written.iter().flat_map(|w| w.bytes.clone()).collect();
    assert_eq!(String::from_utf8_lossy(&bytes), "counter\n0\n");
    assert!(written.iter().all(|w| w.token == ask.token));
}
