//! Spec 01 §2.7: a page that is mostly still, with parts that are not.
//!
//! An island is a node whose *content* comes from a session of its own,
//! while the page around it stays a cached render nobody holds a session
//! for. Nothing was added to the wire for it: it is an ordinary session
//! addressed by an ordinary prop, which is why an application can adopt one
//! without its clients being rebuilt.
//!
//! The vectors that matter are the **refusals**. An island is the one thing
//! on a page that opens a socket at the tree's request, and the tree came
//! from the network.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_client::{Driver, Input};
use eui_proto::*;

const A_ISLAND: u32 = 1;
const A_PATH: u32 = 2;

/// A page whose node 2 is a `slot` carrying `island`, with one child that
/// stands for what the cached render put there.
fn page(path: &str) -> Batch {
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
    t.nodes.push(FlatNode { kind: NodeKind::Slot, id: 2, style: 0, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 1 });
    t.props.push((A_ISLAND, Value::Str(path.to_owned())));
    t.nodes.push(FlatNode { kind: NodeKind::Text, id: 3, style: 0, key: 0, text: Some(TextRef::Inline("142 comments, as rendered".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    t.nodes.push(FlatNode { kind: NodeKind::Text, id: 4, style: 0, key: 0, text: Some(TextRef::Atom(A_PATH)), props: (0, 0), handlers: (0, 0), child_count: 0 });
    Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: A_ISLAND, value: "island".into() },
            Op::DefAtom { id: A_PATH, value: "the page".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, ..Default::default() } },
            Op::Mount(t),
        ],
    }
}

fn welcomed(path: &str) -> Driver {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: PROTOCOL_VERSION, session: [0u8; 16], start: Start::Fresh }));
    assert_eq!(d.handle_frame(Frame::Batch(page(path))), vec![Frame::Ack { seq: 1 }]);
    d
}

/// The content of an island: its own atom 1, its own style 1, its own node 1
/// — every one of which the page has already defined.
fn island_content(text: &str) -> Batch {
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    t.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 0, key: 0, text: Some(TextRef::Atom(1)), props: (0, 0), handlers: (0, 0), child_count: 0 });
    Batch { seq: 1, ops: vec![Op::DefAtom { id: 1, value: text.to_owned() }, Op::DefStyle { id: 1, record: StyleRecord { display: Display::Row, ..Default::default() } }, Op::Mount(t)] }
}

#[test]
fn a_page_asks_for_the_island_its_tree_names() {
    let mut d = welcomed("/_eui/session/comments?for=1042");
    let wanted = d.islands_wanted();
    assert_eq!(wanted.len(), 1);
    assert_eq!(wanted[0].1, "/_eui/session/comments?for=1042");
    assert_eq!(d.session().node(wanted[0].0).map(|n| n.id), Some(2), "the node carrying the prop");
}

/// §2.7: "a client MUST refuse one that names another origin: a tree that
/// could open a socket elsewhere would make every page a way to reach any
/// host the reader can reach."
///
/// `//host/path` is the one worth writing out: it is a protocol-relative
/// URL, so it is a different origin *and* it starts with a slash, which is
/// how the obvious spelling of this check lets it through.
#[test]
fn an_island_naming_another_origin_is_refused() {
    for elsewhere in ["https://evil.example/_eui/session/x", "//evil.example/_eui/session/x", "http://127.0.0.1:1/x", "wss://evil.example/x", "\\\\evil.example\\x", "_eui/session/x"] {
        let mut d = welcomed(elsewhere);
        assert!(d.islands_wanted().is_empty(), "{elsewhere} was offered as an island");
        // And it cannot be opened by asking directly either: the refusal is
        // in the opening and not only in the listing.
        let at = d.session().lookup(2).unwrap();
        assert_eq!(d.open_island(at, elsewhere), None, "{elsewhere} was opened");
    }
}

/// §2.7: the two trees never share a node id space, and neither do their
/// interned tables.
#[test]
fn an_island_speaks_in_its_own_ids_and_the_page_keeps_its_own() {
    let mut d = welcomed("/_eui/session/comments");
    let at = d.session().lookup(2).unwrap();
    let owner = d.open_island(at, "/_eui/session/comments").expect("the first island");

    // The node keeps what the render put there until the island speaks.
    let before = d.session().children(at).to_vec();
    assert_eq!(before.len(), 1);
    assert_eq!(d.session().text_of(before[0]), Some("142 comments, as rendered"));

    d.apply_region(owner, &island_content("151 comments")).expect("its ids are its own");

    let after = d.session().children(at).to_vec();
    assert_eq!(after.len(), 1);
    let inner = d.session().children(after[0])[0];
    assert_eq!(d.session().text_of(inner), Some("151 comments"));
    // The page's atom 1 is still "island" and its node 4 still reads from
    // atom 2 — neither was overwritten by the island's own atom 1.
    assert_eq!(d.session().text_of(d.session().lookup(4).unwrap()), Some("the page"));
    assert_eq!(d.session().style_of(d.session().lookup(1).unwrap()).display, Display::Column);
}

/// §2.7: "two islands naming the same path share one session; two naming the
/// same component with different queries do not, because the query is what
/// tells the application which island it is rendering."
#[test]
fn one_session_per_distinct_path_and_the_query_is_part_of_it() {
    let mut d = welcomed("/_eui/session/comments?for=1042");
    let at = d.session().lookup(2).unwrap();
    let owner = d.open_island(at, "/_eui/session/comments?for=1042").unwrap();

    assert_eq!(d.island_for_path("/_eui/session/comments?for=1042"), Some(owner));
    assert_eq!(d.island_for_path("/_eui/session/comments?for=7"), None, "a different query is a different island");
    assert_eq!(d.island_for_path("/_eui/session/comments"), None);
}

/// §2.7 and 10 §1: at most `MAX_ISLANDS`. Past it a client opens no more and
/// leaves those nodes as they were rendered — a tree is data, and a view
/// that derived an island per row would otherwise open a socket per row.
#[test]
fn a_page_opens_no_more_than_the_ceiling_and_the_rest_stand() {
    let mut d = welcomed("/_eui/session/a");
    let at = d.session().lookup(2).unwrap();
    for n in 1..=8u16 {
        assert_eq!(d.open_island(at, &format!("/s/{n}")), Some(n));
    }
    assert_eq!(d.open_island(at, "/s/9"), None, "the ninth opens nothing");
    assert_eq!(d.islands_open(), 8);
    // And the page is exactly as it was: nothing was torn down to make room.
    assert_eq!(d.session().text_of(d.session().children(at)[0]), Some("142 comments, as rendered"));
}

/// §2.7: "an island whose session cannot be opened, or which ends, **leaves
/// the page alone**: the node keeps the children it had, and nothing else on
/// the page is torn down."
///
/// This is the vector the whole feature rests on. A live part that could take
/// a still page with it would make every island a liability, and a reader
/// would be better served by the stale render.
#[test]
fn an_island_that_ends_leaves_the_page_standing() {
    let mut d = welcomed("/_eui/session/comments");
    let at = d.session().lookup(2).unwrap();
    let owner = d.open_island(at, "/_eui/session/comments").unwrap();
    d.apply_region(owner, &island_content("151 comments")).unwrap();
    let root = d.session().root();

    d.island_ended(owner);

    assert_eq!(d.session().root(), root, "the page still has its root");
    assert!(d.session().lookup(1).is_some(), "and its nodes");
    assert_eq!(d.session().text_of(d.session().lookup(4).unwrap()), Some("the page"));
    assert_eq!(d.session().children(at).len(), 1, "the island's last content is still showing");
    assert!(d.closed().is_none(), "the page's session did not end with it");
    // The node is offered again, so a client may retry.
    assert_eq!(d.islands_wanted().len(), 1);
}

/// §2.7: an event raised inside an island carries that island's ids and goes
/// to **its own socket**. `owner_of` is how a client tells the two apart —
/// sending an island's event to the page's server would name a node that
/// server never created.
#[test]
fn a_node_says_which_socket_its_events_belong_to() {
    let mut d = welcomed("/_eui/session/comments");
    let at = d.session().lookup(2).unwrap();
    let owner = d.open_island(at, "/_eui/session/comments").unwrap();
    d.apply_region(owner, &island_content("151 comments")).unwrap();

    assert_eq!(d.owner_of(d.session().lookup(1).unwrap()), None, "the page's root is the page's");
    assert_eq!(d.owner_of(at), None, "and so is the node carrying the prop — the boundary is that node");
    let inside = d.session().children(at)[0];
    assert_eq!(d.owner_of(inside), Some(owner), "everything below it is the island's");
}

/// §2.7: "an event raised inside it carries its own ids and goes to **its
/// own socket**."
///
/// The failure this stops is not loud. An island's node 1 and the page's node
/// 1 are different nodes, so an island's click sent to the page's server is a
/// well-formed event naming a node that server created for something else —
/// which its own §4 validation may well accept, because the id exists and may
/// even carry a handler of that kind. The page would then do something the
/// reader did not ask for, and nothing anywhere would say so.
#[test]
fn an_islands_event_never_joins_the_pages_outbound() {
    let mut d = welcomed("/_eui/session/comments");
    let at = d.session().lookup(2).unwrap();
    let owner = d.open_island(at, "/_eui/session/comments").unwrap();

    // The island mounts a button. Its node 1 is not the page's node 1.
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 0 });
    t.handlers.push((EventKind::Click, Handler::Server(1)));
    let batch = Batch {
        seq: 1,
        ops: vec![Op::DefAtom { id: 1, value: "reply".into() }, Op::DefStyle { id: 1, record: StyleRecord { width: Dim::Px(80), height: Dim::Px(20), ..Default::default() } }, Op::Mount(t)],
    };
    d.apply_region(owner, &batch).unwrap();

    let inner = d.session().children(at)[0];
    let r = {
        let _ = d.paint(400, 300);
        d.layout().rect(inner).unwrap()
    };
    let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    let to_the_page = d.input(Input::PointerUp(0));

    assert!(!to_the_page.iter().any(|f| matches!(f, Frame::Event(_))), "the page's socket was offered an island's event: {to_the_page:?}");
    let owed = d.take_island_pending();
    assert_eq!(owed.len(), 1, "{owed:?}");
    assert_eq!(owed[0].0, owner, "tagged with the socket that owes it");
    let Frame::Event(e) = &owed[0].1 else { panic!("{owed:?}") };
    assert_eq!(e.node, 1, "and carrying the island's own id");
    assert_eq!(e.event, EventKind::Click);

    // Taken once.
    assert!(d.take_island_pending().is_empty());
}

/// And the page's own events are untouched by any of this.
#[test]
fn the_pages_own_events_still_go_to_the_page() {
    let mut d = welcomed("/_eui/session/comments");
    let at = d.session().lookup(2).unwrap();
    d.open_island(at, "/_eui/session/comments").unwrap();

    // Give the page's node 1 a handler and click it.
    let mut t = Subtree::default();
    t.nodes.push(FlatNode { kind: NodeKind::Box, id: 9, style: 2, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 0 });
    t.handlers.push((EventKind::Click, Handler::Server(A_PATH)));
    let sized = StyleRecord { width: Dim::Px(80), height: Dim::Px(20), ..Default::default() };
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::DefStyle { id: 2, record: sized }, Op::InsertChild { parent: 1, index: 0, subtree: t }] }));

    let ix = d.session().lookup(9).unwrap();
    let r = {
        let _ = d.paint(400, 300);
        d.layout().rect(ix).unwrap()
    };
    let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    let out = d.input(Input::PointerUp(0));

    assert!(out.iter().any(|f| matches!(f, Frame::Event(e) if e.node == 9)), "{out:?}");
    assert!(d.take_island_pending().is_empty(), "and nothing was diverted");
}

/// A page with an island still draws, still lays out and still handles a
/// pointer: one arena, one layout, one paint, and neither the layout engine
/// nor the painter knows islands exist.
#[test]
fn a_page_with_an_island_is_one_tree_to_everything_above_the_session() {
    let mut d = welcomed("/_eui/session/comments");
    let at = d.session().lookup(2).unwrap();
    let owner = d.open_island(at, "/_eui/session/comments").unwrap();
    d.apply_region(owner, &island_content("151 comments")).unwrap();

    let list = d.paint(400, 300);
    assert!(!list.quads.is_empty(), "the page drew");
    let inside = d.session().children(at)[0];
    assert!(d.layout().rect(inside).is_some(), "the island's content was laid out with the page");
    // And a pointer over it resolves in the one tree.
    let r = d.layout().rect(inside).unwrap();
    assert!(d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0)).is_empty(), "nothing handles it, and nothing panics");
}
