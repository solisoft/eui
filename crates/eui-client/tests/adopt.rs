//! Spec 01 §2.6: a session that starts from a tree the client already has.
//!
//! A page fetched over `GET /_eui/view/<component>` costs the server nothing
//! until the reader does something only the server can answer. The moment
//! they do, the naive thing is to mount the page again — and that discards
//! the tree, the layout, focus, every scroll offset and anything half-typed.
//! On the long pages this endpoint exists for, that snap back to the top is
//! the most visible thing about the whole feature.
//!
//! So the client offers what it has and the server may keep it.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_client::{Driver, Input};
use eui_proto::*;

fn batch(seq: u64) -> Batch {
    let col = StyleRecord { display: Display::Column, gap: 4, ..Default::default() };
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    t.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 0, key: 0, text: Some(TextRef::Inline("a page".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    Batch { seq, ops: vec![Op::DefStyle { id: 1, record: col }, Op::Mount(t)] }
}

/// A driver in the state a fetched page leaves it: welcomed by a body that
/// names no session, holding a tree, and knowing what that tree is.
fn fetched() -> (Driver, [u8; 32]) {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    // The `Welcome` of a one-shot render: sixteen zero bytes, because the
    // body is everyone's.
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [0u8; 16], start: Start::Fresh })).is_empty());
    let one = batch(1);
    let encoded = Frame::Batch(one.clone()).encode();
    assert_eq!(d.handle_frame(Frame::Batch(one)), vec![Frame::Ack { seq: 1 }]);
    // The tree's identity is the batches alone — never the body, whose
    // `Welcome` differs between the two roads a tree can travel.
    let tree = *blake3::hash(&encoded).as_bytes();
    d.fetched_tree(tree);
    (d, tree)
}

#[test]
fn a_fetched_page_offers_its_tree_and_not_a_session() {
    let (d, tree) = fetched();
    let Frame::Hello(h) = d.hello() else { panic!() };
    // Not a `Resume` of sixteen zero bytes, which is what offering the
    // session id of a one-shot render would amount to: claiming a session
    // that does not exist and never did.
    assert_eq!(h.resume, Some(Offer::Adopt(tree)));
}

#[test]
fn a_page_with_no_tree_offers_nothing() {
    // Nothing has been mounted, so there is nothing to keep.
    let d = Driver::new(400.0, 300.0, 1.0, 0);
    let Frame::Hello(h) = d.hello() else { panic!() };
    assert_eq!(h.resume, None);
}

#[test]
fn adopted_keeps_the_tree_and_sends_nothing_back() {
    let (mut d, _) = fetched();
    let root = d.session().root();
    assert!(root.is_some());

    // The socket's `Welcome`, with a real session id this time.
    let out = d.handle_frame(Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [9u8; 16], start: Start::Adopted }));
    assert!(out.is_empty(), "adoption is agreement, and needs no answer");
    assert_eq!(d.session().root(), root, "the tree is the one that was already there");

    // And the session is a real one now, so a later socket resumes rather
    // than offering the tree a second time.
    let Frame::Hello(h) = d.hello() else { panic!() };
    assert_eq!(h.resume, Some(Offer::Resume(Resume { session: [9u8; 16], acked: 1 })));
}

#[test]
fn a_refused_offer_tears_the_tree_down_so_the_mount_can_land() {
    let (mut d, _) = fetched();
    assert!(d.session().root().is_some());

    // The server rendered something else — the state moved, the view code
    // changed — so it sends the tree it has.
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [9u8; 16], start: Start::Fresh })).is_empty());
    assert!(d.session().root().is_none(), "the old tree is gone before the new one arrives");

    // This is the part that fails silently if `Fresh` stops calling
    // `start_over`: a fresh mount starts its sequence at 1 again, and a
    // client that kept `acked` would drop it as one already applied and show
    // an empty window with nothing in the log.
    let fresh = batch(1);
    assert_eq!(d.handle_frame(Frame::Batch(fresh)), vec![Frame::Ack { seq: 1 }]);
    assert!(d.session().root().is_some());
}

#[test]
fn a_tree_nobody_offered_cannot_be_adopted() {
    // A server claiming to have kept something this client never had is a
    // server talking about a different client's tree.
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    let out = d.handle_frame(Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [9u8; 16], start: Start::Adopted }));
    assert!(matches!(out.first(), Some(Frame::Error { code: 103, .. })), "{out:?}");
}

#[test]
fn what_the_reader_was_doing_survives_adoption() {
    // The whole point, stated as the thing a reader would notice.
    let (mut d, _) = fetched();
    d.input(Input::PointerMove(10.0, 10.0));
    let before = d.session().live_nodes();

    d.handle_frame(Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [3u8; 16], start: Start::Adopted }));

    assert_eq!(d.session().live_nodes(), before, "no node was rebuilt");
    assert!(d.session().root().is_some());
}

#[test]
fn the_offer_rides_a_tag_an_older_peer_refuses_rather_than_misreads() {
    // `Hello` cannot grow a field — its decoder ends in `finish()`, so an
    // appended one is trailing bytes to every peer built before it. The tag
    // byte is the extension point, and an old server meets `0x02` as an
    // unknown tag: a refusal, not a misreading.
    let (d, _) = fetched();
    let Frame::Hello(h) = d.hello() else { panic!() };
    let bytes = Frame::Hello(h).encode();
    assert_eq!(Frame::decode(&bytes).map(|f| matches!(f, Frame::Hello(_))), Ok(true));

    // The same for `Welcome`'s start byte: one byte, three meanings, and a
    // fourth is refused rather than taken for one of them.
    let mut welcome = Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [1u8; 16], start: Start::Adopted }).encode();
    let last = welcome.len() - 1;
    assert_eq!(welcome[last], 2);
    welcome[last] = 3;
    assert_eq!(Frame::decode(&welcome), Err(DecodeError::UnknownTag("start")));
}
