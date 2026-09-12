//! Session tests: every op, every quota, poison and recovery.
//!
//! Tests may panic; that is how they fail. The strict lint set exists for the
//! apply path, not for the harness.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_proto::*;
use eui_tree::{dirty, ApplyError as E, Limits, Session, Table};

// ---------------------------------------------------------------- builders

fn flat(kind: NodeKind, id: u32, style: u32, children: u32) -> FlatNode {
    FlatNode { kind, id, style, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: children }
}

fn text_node(id: u32, text: TextRef) -> FlatNode {
    FlatNode { kind: NodeKind::Text, id, style: 0, key: 0, text: Some(text), props: (0, 0), handlers: (0, 0), child_count: 0 }
}

fn leaf(id: u32) -> Subtree {
    Subtree { nodes: vec![flat(NodeKind::Box, id, 0, 0)], ..Default::default() }
}

/// box(1) [ box(2) [ text(3) "Hi" ], text(4) "atom 1" ]
fn sample() -> Subtree {
    Subtree { nodes: vec![flat(NodeKind::Box, 1, 1, 2), flat(NodeKind::Box, 2, 0, 1), text_node(3, TextRef::Inline("Hi".into())), text_node(4, TextRef::Atom(1))], ..Default::default() }
}

fn defs() -> Vec<Op> {
    vec![
        Op::DefAtom { id: 1, value: "hello".into() },
        Op::DefAtom { id: 2, value: "save".into() },
        Op::DefStyle { id: 1, record: StyleRecord::default() },
        Op::DefColor { id: 1, rgba: 0x11223344 },
        Op::DefChunk { id: 1, hash: [9; 32] },
    ]
}

fn mounted() -> Session {
    mounted_with(Limits::default())
}

fn mounted_with(limits: Limits) -> Session {
    let mut s = Session::with_limits(limits);
    let mut ops = defs();
    ops.push(Op::Mount(sample()));
    s.apply(&Batch { seq: 1, ops }).unwrap();
    s
}

fn ids_under(s: &Session, id: u32) -> Vec<u32> {
    let ix = s.lookup(id).unwrap();
    s.children(ix).iter().map(|c| s.node(*c).unwrap().id).collect()
}

fn one(s: &mut Session, seq: u64, op: Op) -> core::result::Result<(), E> {
    s.apply(&Batch { seq, ops: vec![op] })
}

// ------------------------------------------------------------------ mount

#[test]
fn mount_builds_the_tree_and_resolves_text() {
    let s = mounted();
    let root = s.root().unwrap();
    assert_eq!(s.node(root).unwrap().id, 1);
    assert_eq!(ids_under(&s, 1), vec![2, 4]);
    assert_eq!(ids_under(&s, 2), vec![3]);
    assert_eq!(s.text_of(s.lookup(3).unwrap()), Some("Hi"));
    assert_eq!(s.text_of(s.lookup(4).unwrap()), Some("hello"));
    assert_eq!(s.live_nodes(), 4);
    assert_eq!(s.depth(s.lookup(3).unwrap()), Some(3));
    assert_eq!(s.table_sizes(), [2, 1, 1, 1]);
    assert_eq!(s.last_seq(), Some(1));
}

#[test]
fn definitions_survive_a_remount_and_the_old_tree_is_freed() {
    let mut s = mounted();
    one(&mut s, 2, Op::Mount(leaf(50))).unwrap();
    assert_eq!(s.live_nodes(), 1);
    assert!(s.lookup(1).is_none());
    assert_eq!(s.atom(1), Some("hello"));
    // Ids from the old tree are free again.
    one(&mut s, 3, Op::InsertChild { parent: 50, index: 0, subtree: leaf(1) }).unwrap();
    assert_eq!(s.live_nodes(), 2);
}

#[test]
fn ops_before_a_mount_are_rejected() {
    let mut s = Session::new();
    assert_eq!(one(&mut s, 1, Op::Focus { node: 1 }), Err(E::NoTree));
}

#[test]
fn preorder_walks_the_whole_tree_in_order() {
    let s = mounted();
    let ids: Vec<u32> = s.preorder(s.root().unwrap()).map(|ix| s.node(ix).unwrap().id).collect();
    assert_eq!(ids, vec![1, 2, 3, 4]);
}

// ----------------------------------------------------------------- tables

#[test]
fn tables_define_once() {
    let mut s = Session::new();
    s.apply(&Batch { seq: 1, ops: defs() }).unwrap();
    assert_eq!(one(&mut s, 2, Op::DefAtom { id: 1, value: "x".into() }), Err(E::Redefined(Table::Atom, 1)));
    let mut s = Session::new();
    s.apply(&Batch { seq: 1, ops: defs() }).unwrap();
    assert_eq!(one(&mut s, 2, Op::DefStyle { id: 1, record: StyleRecord::default() }), Err(E::Redefined(Table::Style, 1)));
}

#[test]
fn table_ids_respect_the_ceiling() {
    let mut s = Session::with_limits(Limits { max_atoms: 2, ..Default::default() });
    one(&mut s, 1, Op::DefAtom { id: 2, value: "ok".into() }).unwrap();
    assert_eq!(one(&mut s, 2, Op::DefAtom { id: 3, value: "no".into() }), Err(E::IdOutOfRange(Table::Atom, 3)));
}

#[test]
fn atom_budget_is_enforced_before_storing() {
    let mut s = Session::with_limits(Limits { max_atom_total_bytes: 8, ..Default::default() });
    one(&mut s, 1, Op::DefAtom { id: 1, value: "12345".into() }).unwrap();
    assert_eq!(one(&mut s, 2, Op::DefAtom { id: 2, value: "6789".into() }), Err(E::AtomBudget));
    assert_eq!(s.atom_bytes(), 5);
    assert!(s.atom(2).is_none());
}

#[test]
fn forward_references_are_rejected() {
    let mut s = Session::new();
    // Text references atom 7, which nothing defined.
    let tree = Subtree { nodes: vec![text_node(1, TextRef::Atom(7))], ..Default::default() };
    assert_eq!(one(&mut s, 1, Op::Mount(tree)), Err(E::Undefined(Table::Atom, 7)));

    let mut s = Session::new();
    let tree = Subtree { nodes: vec![flat(NodeKind::Box, 1, 9, 0)], ..Default::default() };
    assert_eq!(one(&mut s, 1, Op::Mount(tree)), Err(E::Undefined(Table::Style, 9)));

    let mut s = Session::new();
    let record = StyleRecord { bg: ColorRef::literal(3), ..Default::default() };
    assert_eq!(one(&mut s, 1, Op::DefStyle { id: 1, record }), Err(E::Undefined(Table::Color, 3)));

    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::SetHandler { node: 1, event: EventKind::Click, handler: Handler::Local(4) }), Err(E::Undefined(Table::Chunk, 4)));
}

#[test]
fn nothing_is_placed_when_a_deep_node_is_invalid() {
    let mut s = Session::new();
    s.apply(&Batch { seq: 1, ops: defs() }).unwrap();
    let mut tree = sample();
    tree.nodes[3].style = 77; // last node references an undefined style
    assert_eq!(one(&mut s, 2, Op::Mount(tree)), Err(E::Undefined(Table::Style, 77)));
    assert_eq!(s.live_nodes(), 0);
    assert!(s.lookup(1).is_none());
}

// --------------------------------------------------------------- tree ops

#[test]
fn set_style_text_prop_and_handlers() {
    let mut s = mounted();
    let n3 = s.lookup(3).unwrap();
    s.clear_all_dirty();

    one(&mut s, 2, Op::SetText { node: 3, text: TextRef::Atom(1) }).unwrap();
    assert_eq!(s.text_of(n3), Some("hello"));
    assert_eq!(s.node(n3).unwrap().dirty & dirty::SELF, dirty::SELF);
    // Ancestors know something below them changed.
    assert_eq!(s.node(s.lookup(2).unwrap()).unwrap().dirty, dirty::DESCENDANT);
    assert_eq!(s.node(s.lookup(1).unwrap()).unwrap().dirty, dirty::DESCENDANT);
    // A sibling subtree is untouched.
    assert_eq!(s.node(s.lookup(4).unwrap()).unwrap().dirty, 0);

    one(&mut s, 3, Op::SetStyle { node: 3, style: 1 }).unwrap();
    assert_eq!(s.node(n3).unwrap().style, 1);
    assert_eq!(s.style_of(n3), StyleRecord::default());

    one(&mut s, 4, Op::SetProp { node: 1, prop: 2, value: Value::Int(5) }).unwrap();
    one(&mut s, 5, Op::SetProp { node: 1, prop: 2, value: Value::Int(6) }).unwrap();
    let root = s.root().unwrap();
    assert_eq!(s.node(root).unwrap().props, vec![(2, Value::Int(6))]);

    one(&mut s, 6, Op::SetHandler { node: 1, event: EventKind::Click, handler: Handler::Server(2) }).unwrap();
    one(&mut s, 7, Op::SetHandler { node: 1, event: EventKind::Click, handler: Handler::Local(1) }).unwrap();
    assert_eq!(s.handler(root, EventKind::Click), Some(Handler::Local(1)));
    one(&mut s, 8, Op::ClearHandler { node: 1, event: EventKind::Click }).unwrap();
    assert_eq!(s.handler(root, EventKind::Click), None);
    // Clearing what is not there is a harmless no-op.
    one(&mut s, 9, Op::ClearHandler { node: 1, event: EventKind::Blur }).unwrap();
}

#[test]
fn inert_kinds_refuse_content() {
    let mut s = mounted();
    one(&mut s, 2, Op::InsertChild { parent: 1, index: 0, subtree: Subtree { nodes: vec![flat(NodeKind::Spacer, 10, 0, 0)], ..Default::default() } }).unwrap();
    assert_eq!(one(&mut s, 3, Op::SetText { node: 10, text: TextRef::Atom(1) }), Err(E::InertNode(10)));
}

#[test]
fn insert_remove_and_move_children() {
    let mut s = mounted();
    one(&mut s, 2, Op::InsertChild { parent: 1, index: 1, subtree: leaf(20) }).unwrap();
    assert_eq!(ids_under(&s, 1), vec![2, 20, 4]);
    one(&mut s, 3, Op::InsertChild { parent: 1, index: 3, subtree: leaf(21) }).unwrap();
    assert_eq!(ids_under(&s, 1), vec![2, 20, 4, 21]);

    // [2, 20, 4, 21] move 0 -> 2 gives [20, 4, 2, 21]: `to` indexes after removal.
    one(&mut s, 4, Op::MoveChild { parent: 1, from: 0, to: 2 }).unwrap();
    assert_eq!(ids_under(&s, 1), vec![20, 4, 2, 21]);
    one(&mut s, 5, Op::MoveChild { parent: 1, from: 3, to: 0 }).unwrap();
    assert_eq!(ids_under(&s, 1), vec![21, 20, 4, 2]);
    one(&mut s, 6, Op::MoveChild { parent: 1, from: 1, to: 1 }).unwrap();
    assert_eq!(ids_under(&s, 1), vec![21, 20, 4, 2]);

    // Removing node 2 takes its child 3 with it.
    one(&mut s, 7, Op::RemoveChild { parent: 1, index: 3, count: 1 }).unwrap();
    assert_eq!(ids_under(&s, 1), vec![21, 20, 4]);
    assert!(s.lookup(2).is_none());
    assert!(s.lookup(3).is_none());
    assert_eq!(s.live_nodes(), 4);

    one(&mut s, 8, Op::RemoveChild { parent: 1, index: 0, count: 0 }).unwrap();
    one(&mut s, 9, Op::RemoveChild { parent: 1, index: 0, count: 3 }).unwrap();
    assert_eq!(ids_under(&s, 1), Vec::<u32>::new());
    assert_eq!(s.live_nodes(), 1);
}

#[test]
fn reversing_fifty_keyed_rows_is_moves() {
    let mut s = Session::new();
    let mut nodes = vec![flat(NodeKind::Box, 1, 0, 50)];
    for i in 0..50u32 {
        let mut n = flat(NodeKind::Box, 100 + i, 0, 0);
        n.key = i + 1;
        nodes.push(n);
    }
    one(&mut s, 1, Op::Mount(Subtree { nodes, ..Default::default() })).unwrap();
    // Always take the last row and put it at position i.
    let ops = (0..49u32).map(|i| Op::MoveChild { parent: 1, from: 49, to: i }).collect();
    s.apply(&Batch { seq: 2, ops }).unwrap();
    let keys: Vec<u32> = ids_under(&s, 1).iter().map(|id| s.node(s.lookup(*id).unwrap()).unwrap().key).collect();
    assert_eq!(keys, (1..=50).rev().collect::<Vec<_>>());
}

#[test]
fn child_index_bounds() {
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::InsertChild { parent: 1, index: 3, subtree: leaf(9) }), Err(E::ChildIndexOutOfRange { parent: 1, index: 3, len: 2 }));
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::RemoveChild { parent: 1, index: 1, count: 2 }), Err(E::ChildIndexOutOfRange { parent: 1, index: 3, len: 2 }));
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::MoveChild { parent: 1, from: 2, to: 0 }), Err(E::ChildIndexOutOfRange { parent: 1, index: 2, len: 2 }));
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::MoveChild { parent: 1, from: 0, to: 2 }), Err(E::ChildIndexOutOfRange { parent: 1, index: 2, len: 2 }));
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::RemoveChild { parent: 1, index: u32::MAX, count: 1 }), Err(E::ChildIndexOutOfRange { parent: 1, index: u32::MAX, len: 2 }));
}

#[test]
fn leaves_cannot_take_children_and_boxes_cannot_scroll() {
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::InsertChild { parent: 3, index: 0, subtree: leaf(9) }), Err(E::NotAContainer(3)));
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::ScrollTo { node: 1, x: 0, y: 10 }), Err(E::NotScrollable(1)));
    let mut s = mounted();
    let scroll = Subtree { nodes: vec![flat(NodeKind::Scroll, 30, 0, 0)], ..Default::default() };
    one(&mut s, 2, Op::InsertChild { parent: 1, index: 0, subtree: scroll }).unwrap();
    one(&mut s, 3, Op::ScrollTo { node: 30, x: -4, y: 4096 }).unwrap();
    assert_eq!(s.node(s.lookup(30).unwrap()).unwrap().scroll, (-4, 4096));
}

#[test]
fn replace_swaps_a_subtree_in_place_and_can_replace_the_root() {
    let mut s = mounted();
    let sub = Subtree { nodes: vec![flat(NodeKind::Box, 2, 0, 1), text_node(40, TextRef::Atom(1))], ..Default::default() };
    one(&mut s, 2, Op::Replace { node: 2, subtree: sub }).unwrap();
    assert_eq!(ids_under(&s, 1), vec![2, 4]);
    assert_eq!(ids_under(&s, 2), vec![40]);
    assert!(s.lookup(3).is_none());
    assert_eq!(s.live_nodes(), 4);

    one(&mut s, 3, Op::Replace { node: 1, subtree: leaf(99) }).unwrap();
    assert_eq!(s.node(s.root().unwrap()).unwrap().id, 99);
    assert_eq!(s.live_nodes(), 1);
}

#[test]
fn duplicate_ids_are_rejected() {
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::InsertChild { parent: 1, index: 0, subtree: leaf(4) }), Err(E::DuplicateNode(4)));
    let mut s = Session::new();
    let tree = Subtree { nodes: vec![flat(NodeKind::Box, 1, 0, 1), flat(NodeKind::Box, 1, 0, 0)], ..Default::default() };
    assert_eq!(one(&mut s, 1, Op::Mount(tree)), Err(E::DuplicateNode(1)));
}

#[test]
fn unknown_nodes_are_rejected() {
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::SetStyle { node: 77, style: 0 }), Err(E::UnknownNode(77)));
}

// ----------------------------------------------------------------- quotas

#[test]
fn node_quota_counts_the_whole_session() {
    let mut s = mounted_with(Limits { max_nodes: 5, ..Default::default() }); // 4 live
    one(&mut s, 2, Op::InsertChild { parent: 1, index: 0, subtree: leaf(10) }).unwrap(); // 5
    assert_eq!(one(&mut s, 3, Op::InsertChild { parent: 1, index: 0, subtree: leaf(11) }), Err(E::TooManyNodes));
    // A remount frees everything first.
    let mut s = mounted_with(Limits { max_nodes: 5, ..Default::default() });
    one(&mut s, 2, Op::RemoveChild { parent: 1, index: 0, count: 1 }).unwrap(); // frees 2 and 3
    one(&mut s, 3, Op::InsertChild { parent: 1, index: 0, subtree: leaf(10) }).unwrap();
    one(&mut s, 4, Op::InsertChild { parent: 1, index: 0, subtree: leaf(11) }).unwrap();
    one(&mut s, 5, Op::InsertChild { parent: 1, index: 0, subtree: leaf(12) }).unwrap();
    assert_eq!(s.live_nodes(), 5);
}

#[test]
fn depth_quota_applies_to_grafts_not_just_mounts() {
    // sample(): node 2 sits at depth 2, its child 3 at depth 3.
    let mut s = mounted_with(Limits { max_depth: 3, ..Default::default() });
    // A leaf under node 2 lands at depth 3: the limit, allowed.
    one(&mut s, 2, Op::InsertChild { parent: 2, index: 0, subtree: leaf(10) }).unwrap();
    // Two levels under node 2 would reach depth 4.
    let two = Subtree { nodes: vec![flat(NodeKind::Box, 11, 0, 1), flat(NodeKind::Box, 12, 0, 0)], ..Default::default() };
    assert_eq!(one(&mut s, 3, Op::InsertChild { parent: 2, index: 0, subtree: two.clone() }), Err(E::TooDeep));

    let mut s = mounted_with(Limits { max_depth: 3, ..Default::default() });
    // The same two levels under the root reach depth 3: allowed.
    one(&mut s, 2, Op::InsertChild { parent: 1, index: 0, subtree: two }).unwrap();
    // Three levels under the root would reach depth 4.
    let three = Subtree { nodes: vec![flat(NodeKind::Box, 20, 0, 1), flat(NodeKind::Box, 21, 0, 1), flat(NodeKind::Box, 22, 0, 0)], ..Default::default() };
    assert_eq!(one(&mut s, 3, Op::InsertChild { parent: 1, index: 0, subtree: three }), Err(E::TooDeep));
}

#[test]
fn per_node_prop_and_handler_caps() {
    let mut s = Session::new();
    let mut ops: Vec<Op> = (1..=65u32).map(|i| Op::DefAtom { id: i, value: format!("p{i}") }).collect();
    ops.push(Op::Mount(leaf(1)));
    s.apply(&Batch { seq: 1, ops }).unwrap();
    let ops = (1..=64u32).map(|i| Op::SetProp { node: 1, prop: i, value: Value::Null }).collect();
    s.apply(&Batch { seq: 2, ops }).unwrap();
    assert_eq!(one(&mut s, 3, Op::SetProp { node: 1, prop: 65, value: Value::Null }), Err(E::TooManyProps(1)));
}

// ---------------------------------------------------------- poison, order

#[test]
fn a_failure_poisons_until_the_next_mount() {
    let mut s = mounted();
    assert_eq!(one(&mut s, 2, Op::Focus { node: 404 }), Err(E::UnknownNode(404)));
    assert!(s.is_poisoned());
    assert!(s.root().is_none(), "a poisoned tree is not offered for rendering");
    assert_eq!(one(&mut s, 3, Op::Focus { node: 1 }), Err(E::Poisoned));
    // Definitions still land, so the resync batch can carry them.
    one(&mut s, 4, Op::DefAtom { id: 3, value: "late".into() }).unwrap();
    one(&mut s, 5, Op::Mount(leaf(1))).unwrap();
    assert!(!s.is_poisoned());
    assert!(s.root().is_some());
    assert_eq!(s.atom(3), Some("late"));
}

#[test]
fn batches_must_arrive_in_increasing_order() {
    let mut s = mounted();
    assert_eq!(one(&mut s, 1, Op::Focus { node: 1 }), Err(E::OutOfOrder { last: 1, got: 1 }));
    assert!(s.is_poisoned());
}

#[test]
fn a_failing_op_leaves_seq_unadvanced() {
    let mut s = mounted();
    let ops = vec![Op::Focus { node: 1 }, Op::Focus { node: 404 }];
    assert!(s.apply(&Batch { seq: 2, ops }).is_err());
    assert_eq!(s.last_seq(), Some(1));
}

// ------------------------------------------------------------------ focus

#[test]
fn focus_follows_the_tree() {
    let mut s = mounted();
    one(&mut s, 2, Op::Focus { node: 3 }).unwrap();
    assert_eq!(s.focused(), s.lookup(3));
    // Removing an ancestor of the focused node clears focus.
    one(&mut s, 3, Op::RemoveChild { parent: 1, index: 0, count: 1 }).unwrap();
    assert_eq!(s.focused(), None);
    one(&mut s, 4, Op::Focus { node: 4 }).unwrap();
    one(&mut s, 5, Op::Replace { node: 4, subtree: leaf(4) }).unwrap();
    assert_eq!(s.focused(), None);
    one(&mut s, 6, Op::Focus { node: 4 }).unwrap();
    one(&mut s, 7, Op::Mount(leaf(1))).unwrap();
    assert_eq!(s.focused(), None);
}

// ------------------------------------------------------- end to end bytes

#[test]
fn a_decoded_frame_applies() {
    // The spec §8 example, through the real decoder.
    let mut tree = Subtree::default();
    tree.nodes.push(flat(NodeKind::Box, 1, 1, 1));
    tree.nodes.push(text_node(2, TextRef::Atom(1)));
    let frame = Frame::Batch(Batch {
        seq: 1,
        ops: vec![Op::DefAtom { id: 1, value: "Hi".into() }, Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, ..Default::default() } }, Op::Mount(tree)],
    });
    let bytes = frame.encode();
    let Frame::Batch(batch) = Frame::decode(&bytes).unwrap() else { panic!("not a batch") };
    let mut s = Session::new();
    s.apply(&batch).unwrap();
    let root = s.root().unwrap();
    assert_eq!(s.style_of(root).display, Display::Column);
    assert_eq!(s.text_of(s.children(root)[0]), Some("Hi"));
}

/// Random op streams against a small session must never panic, and must
/// leave live-node accounting consistent with a fresh walk of the tree.
#[test]
fn random_op_streams_keep_the_arena_consistent() {
    let mut state = 0x853C_49E6_748F_EA9Bu64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for round in 0..200 {
        let mut s = Session::with_limits(Limits { max_nodes: 64, max_depth: 8, ..Default::default() });
        let mut ops = defs();
        ops.push(Op::Mount(sample()));
        s.apply(&Batch { seq: 1, ops }).unwrap();
        let mut next_id = 100u32;
        for seq in 2..60u64 {
            let target = [1u32, 2, 3, 4, next_id.saturating_sub(1), 500][(next() % 6) as usize];
            let op = match next() % 9 {
                0 => {
                    next_id += 1;
                    Op::InsertChild { parent: target, index: (next() % 4) as u32, subtree: leaf(next_id) }
                }
                1 => Op::RemoveChild { parent: target, index: (next() % 3) as u32, count: (next() % 3) as u32 },
                2 => Op::MoveChild { parent: target, from: (next() % 4) as u32, to: (next() % 4) as u32 },
                3 => Op::SetText { node: target, text: TextRef::Atom(1) },
                4 => Op::SetStyle { node: target, style: (next() % 2) as u32 },
                5 => Op::Focus { node: target },
                6 => {
                    next_id += 1;
                    Op::Replace { node: target, subtree: leaf(next_id) }
                }
                7 => Op::SetHandler { node: target, event: EventKind::Click, handler: Handler::Server(2) },
                _ => Op::ScrollTo { node: target, x: 1, y: 1 },
            };
            let _ = s.apply(&Batch { seq, ops: vec![op] });
            if s.is_poisoned() {
                break;
            }
            let walked = s.preorder(s.root().unwrap()).count() as u32;
            assert_eq!(walked, s.live_nodes(), "round {round} seq {seq}");
        }
    }
}

/// A local restyle that only recolours -- a hover, mostly -- owes a
/// repaint and not a layout: the node is marked painted, not changed.
#[test]
fn a_colour_only_restyle_is_a_repaint_not_a_layout() {
    let mut s = mounted();
    let base = StyleRecord { width: Dim::Px(40), bg: ColorRef::role(1), ..Default::default() };
    let lit = StyleRecord { bg: ColorRef::role(2), ..base };
    let wider = StyleRecord { width: Dim::Px(60), ..base };
    one(&mut s, 2, Op::DefStyle { id: 7, record: base }).unwrap();
    one(&mut s, 3, Op::DefStyle { id: 8, record: lit }).unwrap();
    one(&mut s, 4, Op::DefStyle { id: 9, record: wider }).unwrap();
    one(&mut s, 5, Op::SetStyle { node: 2, style: 7 }).unwrap();
    s.clear_all_dirty();
    let ix = s.lookup(2).unwrap();
    assert_eq!(s.set_style_local(ix, 8), Some(true), "a lit node paints differently and measures the same");
    assert_eq!(s.node(ix).unwrap().dirty & (dirty::PAINT | dirty::SELF), dirty::PAINT);
    assert_eq!(s.set_style_local(ix, 9), Some(false), "a wider one lays out again");
    assert_ne!(s.node(ix).unwrap().dirty & dirty::SELF, 0);
    assert_eq!(s.set_style_local(ix, 99), None, "an unknown style is refused");
    assert!(eui_tree::same_layout(&base, &lit));
    assert!(!eui_tree::same_layout(&base, &wider));
}

/// The media nodes and the wakers are kept as nodes come and go, so the
/// client's players and clocks are found without a walk.
#[test]
fn media_and_wakers_are_kept_as_nodes_come_and_go() {
    let mut s = mounted();
    let mut sub = Subtree::default();
    sub.nodes.push(flat(NodeKind::Box, 50, 1, 2));
    sub.nodes.push(flat(NodeKind::Audio, 51, 0, 0));
    sub.nodes.push(flat(NodeKind::Video, 52, 0, 0));
    sub.nodes[2].handlers = (0, 1);
    sub.handlers.push((EventKind::Wake, Handler::Server(1)));
    one(&mut s, 2, Op::InsertChild { parent: 1, index: 0, subtree: sub }).unwrap();
    let ids = |v: &[eui_tree::NodeIx], s: &Session| v.iter().map(|ix| s.node(*ix).unwrap().id).collect::<Vec<_>>();
    assert_eq!(ids(s.media(), &s), vec![51, 52]);
    assert_eq!(ids(s.wakers(), &s), vec![52]);
    one(&mut s, 3, Op::SetHandler { node: 51, event: EventKind::Wake, handler: Handler::Server(1) }).unwrap();
    assert_eq!(ids(s.wakers(), &s), vec![52, 51]);
    one(&mut s, 4, Op::ClearHandler { node: 52, event: EventKind::Wake }).unwrap();
    assert_eq!(ids(s.wakers(), &s), vec![51]);
    one(&mut s, 5, Op::RemoveChild { parent: 1, index: 0, count: 1 }).unwrap();
    assert!(s.media().is_empty() && s.wakers().is_empty(), "gone with the subtree");
    // A well-known atom is a field once defined.
    assert_eq!(s.atoms().spans, None);
    one(&mut s, 6, Op::DefAtom { id: 40, value: "spans".into() }).unwrap();
    assert_eq!(s.atoms().spans, Some(40));
}

/// Moving a child between parents is a removal and an insertion (02 §5), and
/// which of the two comes first depends on the order the parents sit in. The
/// key must survive **both** orders: with "first placed wins" the insert-first
/// case left the map pointing at the node about to die, the release took the
/// key away with it, and a live node carrying a key had no entry at all.
///
/// What that cost, before it was found: a drag between columns worked left to
/// right and died right to left, because the client holds what is in the hand
/// by key.
#[test]
fn a_key_survives_a_move_between_parents_in_either_order() {
    for insert_first in [false, true] {
        let mut s = Session::new();
        // root [ a [ x ], b [] ], where x is keyed.
        let mut tree = Subtree::default();
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 0, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 2, style: 0, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 0, key: 9, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 4, style: 0, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
        s.apply(&Batch { seq: 1, ops: vec![Op::DefAtom { id: 9, value: "x".into() }, Op::Mount(tree)] }).unwrap();
        assert!(s.lookup_key(9).is_some(), "it starts with an entry");

        let mut moved = Subtree::default();
        moved.nodes.push(FlatNode { kind: NodeKind::Box, id: 5, style: 0, key: 9, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
        let take = Op::RemoveChild { parent: 2, index: 0, count: 1 };
        let put = Op::InsertChild { parent: 4, index: 0, subtree: moved };
        let ops = if insert_first { vec![put, take] } else { vec![take, put] };
        s.apply(&Batch { seq: 2, ops }).unwrap();

        let found = s.lookup_key(9).unwrap_or_else(|| panic!("the key survived, insert_first={insert_first}"));
        assert_eq!(s.node(found).map(|n| n.id), Some(5), "and names the node that is still there");
        assert_eq!(s.node(found).map(|n| n.parent), s.lookup(4), "under its new parent");
    }
}
