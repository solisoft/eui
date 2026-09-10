//! The counter, as an EUI server. What `lang/src/eui/` will do from a
//! `.eui.sl` view, written by hand so the client has something to talk to.
//!
//! Insecure `ws://` on loopback only; the client accepts that in a debug
//! build with `EUI_ALLOW_INSECURE_LOOPBACK=1`. A real deployment sits behind
//! TLS, and the client refuses anything else.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(clippy::arithmetic_side_effects)]

use eui_proto::*;
use eui_theme_roles::*;
use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;

/// Role ids, copied from `spec/05-theme.md` so this example depends on the
/// wire crate alone.
mod eui_theme_roles {
    pub(crate) const SURFACE_BASE: u16 = 1;
    pub(crate) const TEXT_MUTED: u16 = 6;
    pub(crate) const ACCENT_BASE: u16 = 9;
    pub(crate) const ACCENT_ON: u16 = 12;
}

const ATOM_INC: u32 = 1;
const ATOM_DEC: u32 = 2;
const ATOM_TITLE: u32 = 3;
const ATOM_PLUS: u32 = 4;
const ATOM_MINUS: u32 = 5;
const ATOM_ATTACH: u32 = 6;
const ATOM_EXPORT: u32 = 7;
const ATOM_PICK: u32 = 8;
const ATOM_SAVE: u32 = 9;
const ATOM_ATTACH_LABEL: u32 = 10;
const ATOM_EXPORT_LABEL: u32 = 11;

const NODE_VALUE: u32 = 2;
const NODE_PLUS: u32 = 4;
const NODE_MINUS: u32 = 6;
const NODE_ATTACH: u32 = 11;
const NODE_EXPORT: u32 = 13;
const NODE_NOTE: u32 = 15;

/// The counter's state for one session.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counter {
    /// The value.
    pub count: i64,
    /// The next batch sequence number.
    pub seq: u64,
    /// The upload the client said it was starting, and how much of it has
    /// arrived: spec 01 §6 says the metadata comes as an event and the
    /// bytes as `Upload` frames, so the two are joined here.
    pub incoming: Vec<Incoming>,
    /// What the last completed attachment was called, and how big it was.
    pub note: Option<String>,
}

/// A file the client announced and is sending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incoming {
    /// The id the client minted.
    pub id: u32,
    /// What it is called, without a path.
    pub name: String,
    /// What the client said it weighs.
    pub size: u64,
    /// What has arrived.
    pub got: u64,
    /// The next chunk expected.
    pub seq: u32,
}

impl Counter {
    /// The session's definitions. Sent once: tables persist for the whole
    /// session, so a re-mount after `Resync` must not repeat them.
    pub fn definitions(&self) -> Vec<Op> {
        let page = StyleRecord { display: Display::Column, padding: [6; 4], gap: 4, align_items: AlignItems::Start, bg: ColorRef::role(SURFACE_BASE), ..Default::default() };
        let title = StyleRecord { font_size: 4, font_weight: FontWeight::Semibold, ..Default::default() };
        let value = StyleRecord { font_size: 7, font_weight: FontWeight::Bold, ..Default::default() };
        let row = StyleRecord { display: Display::Row, gap: 2, ..Default::default() };
        let button = StyleRecord {
            display: Display::Row,
            justify: Justify::Center,
            align_items: AlignItems::Center,
            padding: [2, 4, 2, 4],
            min_width: Dim::Px(44),
            bg: ColorRef::role(ACCENT_BASE),
            fg: ColorRef::role(ACCENT_ON),
            radius: 2,
            cursor: Cursor::Pointer,
            ..Default::default()
        };
        let hint = StyleRecord { fg: ColorRef::role(TEXT_MUTED), font_size: 1, ..Default::default() };

        vec![
            Op::DefAtom { id: ATOM_INC, value: "increment".into() },
            Op::DefAtom { id: ATOM_DEC, value: "decrement".into() },
            Op::DefAtom { id: ATOM_TITLE, value: "Counter".into() },
            Op::DefAtom { id: ATOM_PLUS, value: "+".into() },
            Op::DefAtom { id: ATOM_MINUS, value: "−".into() },
            Op::DefAtom { id: ATOM_ATTACH, value: "attach".into() },
            Op::DefAtom { id: ATOM_EXPORT, value: "export".into() },
            Op::DefAtom { id: ATOM_PICK, value: "pick".into() },
            Op::DefAtom { id: ATOM_SAVE, value: "save".into() },
            Op::DefAtom { id: ATOM_ATTACH_LABEL, value: "Attach a file…".into() },
            Op::DefAtom { id: ATOM_EXPORT_LABEL, value: "Save the count…".into() },
            Op::DefStyle { id: 1, record: page },
            Op::DefStyle { id: 2, record: title },
            Op::DefStyle { id: 3, record: value },
            Op::DefStyle { id: 4, record: row },
            Op::DefStyle { id: 5, record: button },
            Op::DefStyle { id: 6, record: hint },
        ]
    }

    /// The whole tree, referencing the definitions.
    pub fn mount(&mut self) -> Batch {
        let mut t = Subtree::default();
        let leaf = |id: u32, style: u32, text: TextRef| FlatNode { kind: NodeKind::Text, id, style, key: 0, text: Some(text), props: (0, 0), handlers: (0, 0), child_count: 0 };
        t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 6 });
        t.nodes.push(leaf(7, 2, TextRef::Atom(ATOM_TITLE)));
        t.nodes.push(leaf(NODE_VALUE, 3, TextRef::Inline(self.count.to_string())));
        t.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 4, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
        t.nodes.push(FlatNode { kind: NodeKind::Box, id: NODE_MINUS, style: 5, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 1 });
        t.handlers.push((EventKind::Click, Handler::Server(ATOM_DEC)));
        t.nodes.push(leaf(8, 0, TextRef::Atom(ATOM_MINUS)));
        t.nodes.push(FlatNode { kind: NodeKind::Box, id: NODE_PLUS, style: 5, key: 0, text: None, props: (0, 0), handlers: (1, 1), child_count: 1 });
        t.handlers.push((EventKind::Click, Handler::Server(ATOM_INC)));
        t.nodes.push(leaf(9, 0, TextRef::Atom(ATOM_PLUS)));
        // Spec 03 §3.2: a node that carries `pick` and declares the
        // handler that answers it opens the platform's own dialog when the
        // person activates it — and only then, and only with `fs.pick`.
        // The button is an ordinary box; nothing else in the client knows
        // what a file is.
        t.nodes.push(FlatNode { kind: NodeKind::Box, id: NODE_ATTACH, style: 5, key: 0, text: None, props: (0, 1), handlers: (2, 1), child_count: 1 });
        t.props.push((ATOM_PICK, Value::List(vec![Value::Str("csv,txt,pdf,png".into()), Value::Int(0), Value::Int(4 * 1024 * 1024)])));
        t.handlers.push((EventKind::FilePick, Handler::Server(ATOM_ATTACH)));
        t.nodes.push(leaf(12, 0, TextRef::Atom(ATOM_ATTACH_LABEL)));

        t.nodes.push(FlatNode { kind: NodeKind::Box, id: NODE_EXPORT, style: 5, key: 0, text: None, props: (1, 1), handlers: (3, 1), child_count: 1 });
        t.props.push((ATOM_SAVE, Value::Str("counter.txt".into())));
        t.handlers.push((EventKind::FileSave, Handler::Server(ATOM_EXPORT)));
        t.nodes.push(leaf(14, 0, TextRef::Atom(ATOM_EXPORT_LABEL)));

        t.nodes.push(leaf(NODE_NOTE, 6, TextRef::Inline(self.note.clone().unwrap_or_else(|| "Every click is a round trip; the value comes back from the server.".into()))));

        self.seq += 1;
        Batch { seq: self.seq, ops: vec![Op::Mount(t)] }
    }

    /// The opening batch: definitions, then the tree.
    pub fn first(&mut self) -> Batch {
        let mut batch = self.mount();
        let mut ops = self.definitions();
        ops.append(&mut batch.ops);
        batch.ops = ops;
        batch
    }

    /// The text one save writes.
    fn export(&self) -> Vec<u8> {
        format!("counter\n{}\n", self.count).into_bytes()
    }

    /// Validate an event against what was sent (spec 06 §4) and answer it.
    /// `None` for an event that is not ours — which ends the session.
    pub fn handle(&mut self, e: &EventFrame) -> Option<Batch> {
        // A file the person picked: the metadata now, the bytes as
        // `Upload` frames after it (spec 01 §6).
        if let (NODE_ATTACH, EventKind::FilePick, ATOM_ATTACH) = (e.node, e.event, e.name) {
            let Value::List(p) = &e.payload else { return None };
            let (Some(Value::Int(id)), Some(Value::Str(name)), Some(Value::Int(size))) = (p.first(), p.get(1), p.get(2)) else {
                return None;
            };
            let id = u32::try_from(*id).ok()?;
            self.incoming.retain(|i| i.id != id);
            self.incoming.push(Incoming { id, name: name.clone(), size: (*size).max(0) as u64, got: 0, seq: 0 });
            return Some(self.say(format!("Receiving {name}…")));
        }
        // A place the person chose for what this node offers. The bytes
        // are owed now; `blobs` frames them.
        if let (NODE_EXPORT, EventKind::FileSave, ATOM_EXPORT) = (e.node, e.event, e.name) {
            let Value::Str(name) = &e.payload else { return None };
            return Some(self.say(format!("Saved as {name}")));
        }
        let delta = match (e.node, e.event, e.name) {
            (NODE_PLUS, EventKind::Click, ATOM_INC) => 1,
            (NODE_MINUS, EventKind::Click, ATOM_DEC) => -1,
            _ => return None,
        };
        if !matches!(&e.payload, Value::List(p) if p.len() == 2) {
            return None;
        }
        self.count += delta;
        self.seq += 1;
        Some(Batch { seq: self.seq, ops: vec![Op::SetText { node: NODE_VALUE, text: TextRef::Inline(self.count.to_string()) }] })
    }

    /// Put a line under the counter.
    fn say(&mut self, what: String) -> Batch {
        self.note = Some(what.clone());
        self.seq += 1;
        Batch { seq: self.seq, ops: vec![Op::SetText { node: NODE_NOTE, text: TextRef::Inline(what) }] }
    }

    /// A chunk of a file the person attached (spec 01 §6). `None` while
    /// there is nothing to say; a batch when the file is whole or gone.
    pub fn uploaded(&mut self, t: &Transfer) -> Option<Batch> {
        let Some(i) = self.incoming.iter_mut().find(|i| i.id == t.id) else {
            // Bytes for an upload this session never heard of. The client
            // does not do that; something in the middle did.
            return Some(self.say("An upload arrived that nobody announced".into()));
        };
        if t.seq != i.seq {
            let name = i.name.clone();
            self.incoming.retain(|i| i.id != t.id);
            return Some(self.say(format!("{name} arrived out of order")));
        }
        match t.flag {
            Chunked::Abort => {
                let (name, why) = (i.name.clone(), String::from_utf8_lossy(&t.bytes).into_owned());
                self.incoming.retain(|i| i.id != t.id);
                Some(self.say(format!("{name} did not arrive: {why}")))
            }
            Chunked::More => {
                i.seq += 1;
                i.got += t.bytes.len() as u64;
                None
            }
            Chunked::Last => {
                i.got += t.bytes.len() as u64;
                let (name, got) = (i.name.clone(), i.got);
                self.incoming.retain(|i| i.id != t.id);
                Some(self.say(format!("Attached {name} — {got} bytes")))
            }
        }
    }

    /// The frames that answer one `file_save`: what the person asked to
    /// keep, in chunks (spec 01 §6).
    pub fn blobs(&self) -> Vec<Frame> {
        let bytes = self.export();
        vec![Frame::Blob(Transfer { id: NODE_EXPORT, seq: 0, flag: Chunked::Last, bytes })]
    }
}

/// Sessions this server still holds, and the batches they may owe.
///
/// Spec 01 §4.1: a session belongs to the server, not to the socket under
/// it. Keeping it for a couple of minutes after the socket goes is what
/// turns a closed lid into a pause rather than the end of an application.
#[derive(Clone, Default)]
pub struct Sessions(std::sync::Arc<std::sync::Mutex<std::collections::HashMap<[u8; 16], Held>>>);

/// One session, between sockets or under one.
pub struct Held {
    /// Its state.
    pub counter: Counter,
    /// Batches sent whose `Ack` has not come back, oldest first. This is
    /// what a resume replays, and it is why the client keeps its tree.
    pub sent: std::collections::VecDeque<(u64, Vec<u8>)>,
    /// When the last socket on it went away. `None` while one is on it.
    pub idle_since: Option<std::time::Instant>,
}

/// How long a session outlives its socket.
const KEEP: std::time::Duration = std::time::Duration::from_secs(120);
/// How many unacked batches are kept for a replay.
const REPLAY: usize = 64;

impl Sessions {
    /// A session id. `RandomState` is seeded by the OS, which is enough
    /// for an example on loopback; a real server takes 16 bytes from its
    /// own CSPRNG, because whoever holds this id can pick the session up.
    fn fresh_id() -> [u8; 16] {
        use std::hash::{BuildHasher, Hasher};
        let mut out = [0u8; 16];
        for half in 0..2 {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0));
            h.write_u64(half);
            let bytes = h.finish().to_le_bytes();
            if let Some(slot) = out.get_mut(half as usize * 8..half as usize * 8 + 8) {
                slot.copy_from_slice(&bytes);
            }
        }
        out
    }

    /// Drop what has been idle longer than [`KEEP`].
    fn reap(&self) {
        if let Ok(mut map) = self.0.lock() {
            map.retain(|_, h| h.idle_since.map_or(true, |t| t.elapsed() < KEEP));
        }
    }

    /// Take the session a `Hello` offered, if it is still here and nothing
    /// else is on it, and if everything after `acked` can still be replayed.
    /// `None` means the client is told to start again.
    fn take(&self, resume: Option<Resume>) -> Option<([u8; 16], Held, Vec<Vec<u8>>)> {
        let r = resume?;
        let mut map = self.0.lock().ok()?;
        let held = map.get(&r.session)?;
        // A session with a socket on it is not one to hand over: two
        // windows must not share one tree.
        held.idle_since?;
        let complete = held.sent.front().map_or(held.counter.seq == r.acked, |(first, _)| *first <= r.acked.saturating_add(1));
        if !complete {
            return None;
        }
        let mut held = map.remove(&r.session)?;
        held.idle_since = None;
        let replay = held.sent.iter().filter(|(seq, _)| *seq > r.acked).map(|(_, b)| b.clone()).collect();
        Some((r.session, held, replay))
    }

    /// Put a session back when its socket goes, for the next one to find.
    fn park(&self, id: [u8; 16], mut held: Held) {
        held.idle_since = Some(std::time::Instant::now());
        if let Ok(mut map) = self.0.lock() {
            map.insert(id, held);
        }
    }
}

/// Serve one connection to completion, on a session table of its own: no
/// socket after it can resume what it had.
pub async fn serve_one(stream: TcpStream) {
    serve_session(stream, Sessions::default()).await;
}

/// Serve one connection against `sessions`, which the socket after it may
/// pick its session up from.
pub async fn serve_session(stream: TcpStream, sessions: Sessions) {
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else { return };
    let (mut sink, mut source) = ws.split();

    // Hello first, then Welcome, then either the tree or what the socket
    // that broke did not get to say.
    let hello = match source.next().await {
        Some(Ok(Message::Binary(b))) => match Frame::decode(&b) {
            Ok(Frame::Hello(h)) => h,
            _ => return,
        },
        _ => return,
    };
    sessions.reap();
    let (id, mut held, replay) = match sessions.take(hello.resume) {
        Some(picked) => picked,
        None => ([Sessions::fresh_id()][0], Held { counter: Counter::default(), sent: std::collections::VecDeque::new(), idle_since: None }, Vec::new()),
    };
    let resumed = !replay.is_empty() || held.counter.seq > 0;
    let welcome = Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: id, resumed });
    if sink.send(Message::Binary(welcome.encode())).await.is_err() {
        return;
    }
    let mut first: Vec<Vec<u8>> = if resumed {
        replay
    } else {
        let batch = Frame::Batch(held.counter.first()).encode();
        held.sent.push_back((held.counter.seq, batch.clone()));
        vec![batch]
    };
    for bytes in first.drain(..) {
        if sink.send(Message::Binary(bytes)).await.is_err() {
            sessions.park(id, held);
            return;
        }
    }

    while let Some(Ok(msg)) = source.next().await {
        let Message::Binary(bytes) = msg else { break };
        let mut replies: Vec<Frame> = Vec::new();
        let mut fatal = false;
        match Frame::decode(&bytes) {
            Ok(Frame::Event(e)) => {
                let save = matches!((e.node, e.event), (NODE_EXPORT, EventKind::FileSave));
                match held.counter.handle(&e) {
                    Some(batch) => replies.push(Frame::Batch(batch)),
                    None => {
                        replies.push(Frame::Error { code: 300, message: "event does not match the tree".into() });
                        fatal = true;
                    }
                }
                if save && !fatal {
                    replies.extend(held.counter.blobs());
                }
            }
            Ok(Frame::Upload(t)) => {
                if let Some(batch) = held.counter.uploaded(&t) {
                    replies.push(Frame::Batch(batch));
                }
            }
            Ok(Frame::Ping(n)) => replies.push(Frame::Pong(n)),
            Ok(Frame::Ack { seq }) => {
                // What the client has acked is what a resume no longer
                // has to replay.
                while held.sent.front().is_some_and(|(s, _)| *s <= seq) {
                    held.sent.pop_front();
                }
            }
            Ok(Frame::Pong(_) | Frame::Viewport(_)) => {}
            Ok(Frame::Resync) => replies.push(Frame::Batch(held.counter.mount())),
            Ok(_) | Err(_) => {
                replies.push(Frame::Error { code: 301, message: "unexpected frame".into() });
                fatal = true;
            }
        }
        for reply in replies {
            let encoded = reply.encode();
            if let Frame::Batch(b) = &reply {
                held.sent.push_back((b.seq, encoded.clone()));
                while held.sent.len() > REPLAY {
                    held.sent.pop_front();
                }
            }
            if sink.send(Message::Binary(encoded)).await.is_err() {
                sessions.park(id, held);
                return;
            }
        }
        if fatal {
            // A session that ended on an error is not one to come back to.
            return;
        }
    }
    // The socket went. The session waits for the next one (spec 01 §4.1).
    sessions.park(id, held);
}

/// Accept connections forever, on one session table: a client whose socket
/// breaks reconnects and finds its tree where it left it.
pub async fn serve(listener: TcpListener) {
    let sessions = Sessions::default();
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        tokio::spawn(serve_session(stream, sessions.clone()));
    }
}
