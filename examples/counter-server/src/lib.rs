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

const NODE_VALUE: u32 = 2;
const NODE_PLUS: u32 = 4;
const NODE_MINUS: u32 = 6;

/// The counter's state for one session.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counter {
    /// The value.
    pub count: i64,
    /// The next batch sequence number.
    pub seq: u64,
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
        t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 4 });
        t.nodes.push(leaf(7, 2, TextRef::Atom(ATOM_TITLE)));
        t.nodes.push(leaf(NODE_VALUE, 3, TextRef::Inline(self.count.to_string())));
        t.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 4, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
        t.nodes.push(FlatNode { kind: NodeKind::Box, id: NODE_MINUS, style: 5, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 1 });
        t.handlers.push((EventKind::Click, Handler::Server(ATOM_DEC)));
        t.nodes.push(leaf(8, 0, TextRef::Atom(ATOM_MINUS)));
        t.nodes.push(FlatNode { kind: NodeKind::Box, id: NODE_PLUS, style: 5, key: 0, text: None, props: (0, 0), handlers: (1, 1), child_count: 1 });
        t.handlers.push((EventKind::Click, Handler::Server(ATOM_INC)));
        t.nodes.push(leaf(9, 0, TextRef::Atom(ATOM_PLUS)));
        t.nodes.push(leaf(10, 6, TextRef::Inline("Every click is a round trip; the value comes back from the server.".into())));

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

    /// Validate an event against what was sent (spec 06 §4) and answer it.
    /// `None` for an event that is not ours — which ends the session.
    pub fn handle(&mut self, e: &EventFrame) -> Option<Batch> {
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
}

/// Serve one connection to completion.
pub async fn serve_one(stream: TcpStream) {
    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else { return };
    let (mut sink, mut source) = ws.split();
    let mut counter = Counter::default();

    // Hello first, then Welcome, then the tree.
    match source.next().await {
        Some(Ok(Message::Binary(b))) if matches!(Frame::decode(&b), Ok(Frame::Hello(_))) => {}
        _ => return,
    }
    let welcome = Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [7; 16] });
    if sink.send(Message::Binary(welcome.encode())).await.is_err() {
        return;
    }
    if sink.send(Message::Binary(Frame::Batch(counter.first()).encode())).await.is_err() {
        return;
    }

    while let Some(Ok(msg)) = source.next().await {
        let Message::Binary(bytes) = msg else { break };
        let reply = match Frame::decode(&bytes) {
            Ok(Frame::Event(e)) => match counter.handle(&e) {
                Some(batch) => Frame::Batch(batch),
                None => Frame::Error { code: 300, message: "event does not match the tree".into() },
            },
            Ok(Frame::Ping(n)) => Frame::Pong(n),
            Ok(Frame::Ack { .. } | Frame::Pong(_) | Frame::Viewport(_)) => continue,
            Ok(Frame::Resync) => Frame::Batch(counter.mount()),
            Ok(_) | Err(_) => Frame::Error { code: 301, message: "unexpected frame".into() },
        };
        let fatal = matches!(reply, Frame::Error { .. });
        if sink.send(Message::Binary(reply.encode())).await.is_err() || fatal {
            break;
        }
    }
}

/// Accept connections forever.
pub async fn serve(listener: TcpListener) {
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        tokio::spawn(serve_one(stream));
    }
}
