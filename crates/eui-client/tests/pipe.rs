//! Spec 01 §7: a session over a pipe, and the vectors 09 §1.3 names.
//!
//! The pipe is exercised through a Unix socket pair, which is a pipe in both
//! directions with nothing in between: what a test writes on one end is
//! exactly what `eui --pipe` would have read on its standard input.
#![cfg(unix)]
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use eui_client::pipe::{self, Ending};
use eui_client::{Driver, Incoming, TransportError};
use eui_proto::*;

/// A reader that hands over its bytes in the pieces it was given, however
/// small, so a frame can be split across reads at any boundary.
struct Pieces(Vec<Vec<u8>>);

impl Read for Pieces {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // An empty piece is no read at all: `Ok(0)` is the end of the pipe.
        self.0.retain(|p| !p.is_empty());
        let Some(first) = self.0.first_mut() else { return Ok(0) };
        let n = first.len().min(buf.len());
        buf[..n].copy_from_slice(&first[..n]);
        first.drain(..n);
        if first.is_empty() {
            self.0.remove(0);
        }
        Ok(n)
    }
}

fn three() -> Vec<Frame> {
    vec![
        Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [9u8; 16], start: Start::Fresh }),
        Frame::Batch(Batch { seq: 1, ops: vec![Op::DefColor { id: 1, rgba: 0x1122_3344 }] }),
        Frame::Error { code: 7, message: "x".repeat(200) },
    ]
}

/// Vector 1: Frames end where their `len` says, however the bytes arrive.
#[test]
fn frames_on_a_pipe_are_cut_by_their_own_length() {
    let frames = three();
    let all: Vec<u8> = frames.iter().flat_map(Frame::encode).collect();
    for split in 0..=all.len() {
        let mut input = Pieces(vec![all[..split].to_vec(), all[split..].to_vec()]);
        for want in &frames {
            let got = pipe::read_frame(&mut input).unwrap().expect("a frame");
            assert_eq!(&Frame::decode(&got).unwrap(), want, "split at {split}");
        }
        assert_eq!(pipe::read_frame(&mut input).unwrap(), None, "and then the clean end");
    }
    // And a byte at a time, which is every boundary at once.
    let mut input = Pieces(all.iter().map(|b| vec![*b]).collect());
    for want in &frames {
        assert_eq!(&Frame::decode(&pipe::read_frame(&mut input).unwrap().unwrap()).unwrap(), want);
    }
}

/// Vector 2: A declared length past the ceiling is refused on the header alone:
/// nothing is reserved for it and nothing after it is read.
#[test]
fn a_pipe_frame_past_the_limit_is_refused_before_it_is_read() {
    let mut head = vec![0x03];
    let mut len = (eui_proto::limits::MAX_FRAME_BYTES + 1) as u64;
    loop {
        let b = (len & 0x7F) as u8;
        len >>= 7;
        if len == 0 {
            head.push(b);
            break;
        }
        head.push(b | 0x80);
    }
    // No payload follows: a reader that tried to read one would report the
    // pipe ending mid-frame instead of the refusal.
    let err = pipe::read_frame(&mut Pieces(vec![head])).unwrap_err();
    assert!(err.contains("refused"), "{err}");
}

/// Vector 3: The end of the pipe inside a frame is a truncated frame, not the end
/// of a session — and the end between frames is.
#[test]
fn a_pipe_cut_mid_frame_ends_with_a_reason() {
    let whole = Frame::Batch(Batch { seq: 1, ops: vec![Op::DefColor { id: 1, rgba: 1 }] }).encode();
    for cut in 1..whole.len() {
        let err = pipe::read_frame(&mut Pieces(vec![whole[..cut].to_vec()])).unwrap_err();
        assert!(err.contains("middle of a frame"), "cut at {cut}: {err}");
    }
    assert_eq!(pipe::read_frame(&mut Pieces(Vec::new())).unwrap(), None);

    // Through a connection, the same cut is a `Pipe` error the window shows.
    let (client, mut app) = UnixStream::pair().unwrap();
    let conn = pipe::connect(client.try_clone().unwrap(), client, Vec::new(), || {}).unwrap();
    app.write_all(&whole[..whole.len() - 1]).unwrap();
    app.shutdown(std::net::Shutdown::Write).unwrap();
    match conn.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        Incoming::Closed(TransportError::Pipe(why)) => assert!(why.contains("middle of a frame"), "{why}"),
        other => panic!("{other:?}"),
    }
}

/// Vector 4: The client speaks first, at version 8 or later, offering nothing to
/// resume — and the `Hello` is the first thing on the pipe.
#[test]
fn a_pipe_hello_offers_eight_and_no_resume() {
    let driver = Driver::new(640.0, 480.0, 1.0, 0);
    let Frame::Hello(hello) = driver.hello() else { panic!("not a Hello") };
    assert!(hello.version >= 8, "a pipe session is version 8 or later (01 §7.1)");
    assert_eq!(hello.resume, None, "and resumes nothing");

    let (client, mut app) = UnixStream::pair().unwrap();
    let first = driver.hello().encode();
    let _conn = pipe::connect(client.try_clone().unwrap(), client, first.clone(), || {}).unwrap();
    let got = pipe::read_frame(&mut app).unwrap().unwrap();
    assert_eq!(got, first, "the Hello, before anything else");
}

/// An application's end of a pipe connection, for the asset vectors.
fn pair() -> (eui_client::Connection, UnixStream) {
    let (client, app) = UnixStream::pair().unwrap();
    let conn = pipe::connect(client.try_clone().unwrap(), client, Vec::new(), || {}).unwrap();
    app.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    (conn, app)
}

fn send(app: &mut UnixStream, frame: Frame) {
    app.write_all(&frame.encode()).unwrap();
}

fn read_fetch(app: &mut UnixStream) -> ([u8; 32], u64) {
    match Frame::decode_pipe(&pipe::read_frame(app).unwrap().unwrap()).unwrap() {
        Frame::Fetch { hash, cap } => (hash, cap),
        other => panic!("expected a Fetch, got {other:?}"),
    }
}

/// Vector 5: An asset is asked for in the session, arrives in chunks, and is
/// believed only once it hashes to its name; one asked for twice while on
/// its way is asked for once.
#[test]
fn an_asset_arrives_over_the_pipe_in_chunks_and_is_hashed() {
    let (conn, mut app) = pair();
    let picture: Vec<u8> = (0..700u32).map(|i| i as u8).collect();
    let name = *blake3::hash(&picture).as_bytes();
    conn.fetcher().request_asset_within(name, 4096);
    assert_eq!(read_fetch(&mut app), (name, 4096));
    for (seq, part) in picture.chunks(250).enumerate() {
        let flag = if seq == 2 { Chunked::Last } else { Chunked::More };
        send(&mut app, Frame::Asset(AssetChunk { hash: name, seq: seq as u32, flag, bytes: part.to_vec() }));
    }
    match conn.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        Incoming::Asset(hash, Ok(bytes)) => {
            assert_eq!(hash, name);
            assert_eq!(bytes, picture, "three chunks, one asset");
        }
        other => panic!("{other:?}"),
    }

    // Bytes that are not what the name says are discarded, not delivered.
    let liar = [0x5A; 32];
    conn.fetcher().request_asset_within(liar, 4096);
    assert_eq!(read_fetch(&mut app).0, liar);
    send(&mut app, Frame::Asset(AssetChunk { hash: liar, seq: 0, flag: Chunked::Last, bytes: b"not that".to_vec() }));
    match conn.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        Incoming::Asset(hash, Err(why)) => {
            assert_eq!(hash, liar);
            assert!(why.contains("did not hash"), "{why}");
        }
        other => panic!("{other:?}"),
    }

    // Asked twice while on its way: one `Fetch`. Answered `aborted`, the
    // reason reaches the window and the name may be asked for again.
    let missing = [0x33; 32];
    conn.fetcher().request_asset_within(missing, 100);
    conn.fetcher().request_asset_within(missing, 100);
    assert_eq!(read_fetch(&mut app).0, missing);
    app.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    let mut probe = [0u8; 1];
    assert!(app.read(&mut probe).is_err(), "no second Fetch for a name already on its way");
    app.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    send(&mut app, Frame::Asset(AssetChunk { hash: missing, seq: 0, flag: Chunked::Abort, bytes: b"not here".to_vec() }));
    match conn.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        Incoming::Asset(hash, Err(why)) => {
            assert_eq!(hash, missing);
            assert_eq!(why, "not here");
        }
        other => panic!("{other:?}"),
    }
    conn.fetcher().request_asset_within(missing, 100);
    assert_eq!(read_fetch(&mut app).0, missing, "an answered name is asked for afresh");
}

/// Vector 6: An asset nobody asked for, or one larger than its `cap`, ends the
/// session: the pipe is the only road, and a server that does either is not
/// speaking the protocol.
#[test]
fn an_unasked_or_oversized_asset_ends_the_pipe_session() {
    let (conn, mut app) = pair();
    send(&mut app, Frame::Asset(AssetChunk { hash: [1; 32], seq: 0, flag: Chunked::Last, bytes: b"surprise".to_vec() }));
    match conn.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        Incoming::Closed(TransportError::Pipe(why)) => assert!(why.contains("nobody asked"), "{why}"),
        other => panic!("{other:?}"),
    }

    let (conn, mut app) = pair();
    let name = [2; 32];
    conn.fetcher().request_asset_within(name, 4);
    assert_eq!(read_fetch(&mut app), (name, 4));
    send(&mut app, Frame::Asset(AssetChunk { hash: name, seq: 0, flag: Chunked::More, bytes: b"abc".to_vec() }));
    send(&mut app, Frame::Asset(AssetChunk { hash: name, seq: 1, flag: Chunked::Last, bytes: b"de".to_vec() }));
    match conn.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        Incoming::Closed(TransportError::Pipe(why)) => assert!(why.contains("passed"), "{why}"),
        other => panic!("{other:?}"),
    }
}

/// Vector 8a: The application said why it stopped, then closed the pipe: the
/// window keeps the reason on screen.
#[test]
fn a_pipe_that_ends_after_an_error_shows_it() {
    let (conn, mut app) = pair();
    let mut driver = Driver::new(640.0, 480.0, 1.0, 0);
    send(&mut app, Frame::Error { code: 500, message: "the database is gone".into() });
    app.shutdown(std::net::Shutdown::Write).unwrap();
    let why = loop {
        match conn.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            Incoming::Message(bytes) => {
                driver.handle_frame(Frame::decode(&bytes).unwrap());
            }
            Incoming::Closed(e) => break e,
            Incoming::Asset(..) => {}
        }
    };
    assert_eq!(why, TransportError::Closed, "a clean end of the pipe");
    match pipe::ending(&why, driver.closed().map(ToString::to_string)) {
        Ending::Show(text) => assert!(text.contains("the database is gone"), "{text}"),
        Ending::Finished => panic!("an application that said why must not have its window closed on it"),
    }
}

/// Vector 8b: The application finished without a word: the window goes with it.
#[test]
fn a_pipe_that_ends_cleanly_closes_the_window() {
    let (conn, app) = pair();
    drop(app);
    let why = match conn.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
        Incoming::Closed(e) => e,
        other => panic!("{other:?}"),
    };
    assert_eq!(pipe::ending(&why, None), Ending::Finished);
    assert!(matches!(pipe::ending(&TransportError::Pipe("x".into()), None), Ending::Show(_)), "a broken pipe is shown, not closed on");
    // Nothing more arrives on a pipe that has ended.
    assert!(matches!(conn.rx.recv_timeout(Duration::from_millis(200)), Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected)));
}

/// Vector 9: Standard output carries frames and nothing else. The binary is run
/// with `EUI_TRACE=1` — the loudest it gets — an empty pipe for input and
/// no display, so what is checked is the start-up and the refusal that
/// follows it: every word of either goes to standard error, and what is on
/// standard output decodes as whole frames or is empty.
#[test]
fn nothing_but_frames_on_stdout() {
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_eui"))
        .arg("--pipe")
        .arg("--title")
        .arg("probe")
        .env("EUI_TRACE", "1")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while child.try_wait().unwrap().is_none() {
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("eui --pipe did not finish on an empty pipe");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut out = Vec::new();
    child.stdout.take().unwrap().read_to_end(&mut out).unwrap();
    let mut err = String::new();
    child.stderr.take().unwrap().read_to_string(&mut err).unwrap();
    let mut input = Pieces(vec![out.clone()]);
    while let Some(frame) = pipe::read_frame(&mut input).unwrap_or_else(|e| panic!("stdout is not frames ({e}): {:?}\nstderr: {err}", String::from_utf8_lossy(&out))) {
        Frame::decode(&frame).unwrap_or_else(|e| panic!("a frame on stdout does not decode: {e}"));
    }
    assert!(!err.is_empty(), "the start-up said something, and said it on stderr");
}

const A_ISLAND: u32 = 1;

/// A page whose node 2 carries `island`, with one child the render put there.
fn page_with_island() -> Batch {
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    t.nodes.push(FlatNode { kind: NodeKind::Slot, id: 2, style: 0, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 1 });
    t.props.push((A_ISLAND, Value::Str("/_eui/session/comments".to_owned())));
    t.nodes.push(FlatNode { kind: NodeKind::Text, id: 3, style: 0, key: 0, text: Some(TextRef::Inline("142 comments, as rendered".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    Batch { seq: 1, ops: vec![Op::DefAtom { id: A_ISLAND, value: "island".into() }, Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, ..Default::default() } }, Op::Mount(t)] }
}

/// Vector 10: A pipe session has no origin, so an island in its tree has no address
/// to be dialled on (01 §7.5): the window opens none, and the node keeps the
/// children it was rendered with.
#[test]
fn an_island_in_a_pipe_session_keeps_its_children() {
    use eui_client::dial::{island_url, PIPE_URL};
    assert_eq!(island_url(PIPE_URL, "/_eui/session/comments"), None, "nowhere to dial from a pipe");
    assert_eq!(island_url("wss://h.example/_eui/session/page", "/_eui/session/comments").as_deref(), Some("wss://h.example/_eui/session/comments"), "where a socket page dials it");
    assert_eq!(island_url("ws://127.0.0.1:5011/_eui/session/page", "/x").as_deref(), Some("ws://127.0.0.1:5011/x"));

    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [0u8; 16], start: Start::Fresh }));
    assert_eq!(d.handle_frame(Frame::Batch(page_with_island())), vec![Frame::Ack { seq: 1 }]);
    // What `dial_islands` does with no address: nothing. The tree still
    // asks, and the node is left exactly as the page drew it.
    let at = d.session().lookup(2).unwrap();
    assert_eq!(d.session().children(at).len(), 1);
    assert_eq!(d.session().text_of(d.session().lookup(3).unwrap()), Some("142 comments, as rendered"));
    assert_eq!(d.islands_open(), 0, "and no island is open");
}
