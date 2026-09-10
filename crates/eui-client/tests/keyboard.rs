//! Spec 03 §3, the parts a server cannot own: what `Escape` means, where
//! `Tab` may go while a modal is up, where focus starts when one arrives, and
//! which keys a widget may claim without losing `Enter`.
//!
//! All four exist because a server *cannot* do them. It does not own Tab, it
//! is never told about Escape, and it cannot know which node the client will
//! treat as pressed.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_client::{Driver, Input};
use eui_proto::*;

/// A page with a button, a modal holding a field and a button, and a button
/// after it. Preorder with child counts, so the modal really contains two.
///
/// `props` are attached by node id, which is what the tests vary.
fn page(props: Vec<(u32, Vec<(u32, Value)>)>, atoms: &[&str], keyed: Option<u32>) -> Batch {
    let mut tree = Subtree::default();
    // Prop atoms take 1..=n; the two event names a handler needs follow them,
    // because a handler naming an atom the batch never defined is refused.
    let click_atom = u32::try_from(atoms.len()).unwrap() + 1;
    let key_atom = click_atom + 1;
    let mut flat: Vec<(NodeKind, u32, u32, bool)> =
        vec![(NodeKind::Box, 1, 3, false), (NodeKind::Box, 2, 0, true), (NodeKind::Box, 3, 2, false), (NodeKind::Input, 4, 0, false), (NodeKind::Box, 5, 0, true), (NodeKind::Box, 6, 0, true)];
    for (kind, id, children, click) in flat.drain(..) {
        let own: Vec<(u32, Value)> = props.iter().find(|(n, _)| *n == id).map(|(_, p)| p.clone()).unwrap_or_default();
        let pstart = u32::try_from(tree.props.len()).unwrap();
        let plen = u32::try_from(own.len()).unwrap();
        tree.props.extend(own);
        let hstart = u32::try_from(tree.handlers.len()).unwrap();
        if click {
            tree.handlers.push((EventKind::Click, Handler::Server(click_atom)));
        }
        if keyed == Some(id) {
            tree.handlers.push((EventKind::KeyDown, Handler::Server(key_atom)));
        }
        let hlen = u32::try_from(tree.handlers.len()).unwrap() - hstart;
        tree.nodes.push(FlatNode {
            kind,
            id,
            style: if id == 1 { 1 } else { 0 },
            key: 0,
            text: if kind == NodeKind::Input { Some(TextRef::Inline(String::new())) } else { None },
            props: (pstart, plen),
            handlers: (hstart, hlen),
            child_count: children,
        });
    }
    let mut ops: Vec<Op> = atoms.iter().enumerate().map(|(i, a)| Op::DefAtom { id: u32::try_from(i).unwrap() + 1, value: (*a).to_owned() }).collect();
    ops.push(Op::DefAtom { id: click_atom, value: "pressed".into() });
    ops.push(Op::DefAtom { id: key_atom, value: "keyed".into() });
    ops.push(Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, padding: [4; 4], ..Default::default() } });
    ops.push(Op::Mount(tree));
    Batch { seq: 1, ops }
}

fn driver(batch: Batch) -> Driver {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(batch));
    let _ = d.paint(400, 300);
    d
}

fn tab(d: &mut Driver) {
    d.input(Input::Key { key: "Tab".into(), modifiers: 0, down: true });
    d.input(Input::Key { key: "Tab".into(), modifiers: 0, down: false });
}

fn press(d: &mut Driver, key: &str) -> Vec<Frame> {
    d.input(Input::Key { key: key.into(), modifiers: 0, down: true })
}

fn events(out: &[Frame]) -> Vec<(EventKind, u32)> {
    out.iter().filter_map(|f| if let Frame::Event(e) = f { Some((e.event, e.node)) } else { None }).collect()
}

#[test]
fn escape_reaches_a_handler_on_the_path_and_leaves_focus_alone() {
    // The modal asks for keys; the button inside it has focus.
    let mut d = driver(page(vec![(3, vec![(1, Value::Bool(true))])], &["modal"], Some(3)));
    tab(&mut d);
    let focused = d.focused();
    assert!(focused.is_some(), "something inside the modal has focus");
    let out = press(&mut d, "Escape");
    assert_eq!(events(&out), vec![(EventKind::KeyDown, 3)], "the modal hears it");
    assert_eq!(d.focused(), focused, "and focus stays where it was, for the server to move");
}

#[test]
fn escape_with_nothing_listening_still_only_blurs() {
    let mut d = driver(page(vec![], &[], None));
    tab(&mut d);
    assert!(d.focused().is_some());
    let out = press(&mut d, "Escape");
    assert!(events(&out).is_empty(), "no handler, so nothing is reported");
    assert_eq!(d.focused(), None, "the old meaning is kept");
}

#[test]
fn a_modal_keeps_tab_inside_itself() {
    let mut d = driver(page(vec![(3, vec![(1, Value::Bool(true))])], &["modal"], None));
    let inside: Vec<_> = [4u32, 5].iter().map(|id| d.session().lookup(*id)).collect();
    // Four presses around a two-stop cycle land back where they started, and
    // never on the buttons outside the modal.
    for _ in 0..4 {
        tab(&mut d);
        assert!(inside.contains(&d.focused()), "focus left the modal: {:?}", d.focused());
    }
}

#[test]
fn without_a_modal_tab_still_walks_the_whole_page() {
    let mut d = driver(page(vec![], &[], None));
    let mut seen = Vec::new();
    for _ in 0..4 {
        tab(&mut d);
        seen.push(d.focused());
    }
    let outside = d.session().lookup(2);
    assert!(seen.contains(&outside), "the button before the modal is still reachable");
}

#[test]
fn a_modal_inside_a_modal_traps_in_the_inner_one() {
    // Node 3 and node 5 both declare themselves modal; 5 is deeper in preorder.
    let mut d = driver(page(vec![(3, vec![(1, Value::Bool(true))]), (5, vec![(1, Value::Bool(true))])], &["modal"], None));
    tab(&mut d);
    assert_eq!(d.focused(), d.session().lookup(5), "the innermost one owns the keyboard");
    tab(&mut d);
    assert_eq!(d.focused(), d.session().lookup(5), "and it is the only stop");
}

#[test]
fn a_surface_that_asks_for_focus_gets_it_when_it_arrives() {
    let d = driver(page(vec![(3, vec![(1, Value::Bool(true))]), (5, vec![(2, Value::Bool(true))])], &["modal", "autofocus"], None));
    assert_eq!(d.focused(), d.session().lookup(5), "focus starts inside the modal, not behind it");
}

#[test]
fn autofocus_does_not_yank_focus_back_on_a_later_batch() {
    let batch = page(vec![(3, vec![(1, Value::Bool(true))]), (5, vec![(2, Value::Bool(true))])], &["modal", "autofocus"], None);
    let mut d = driver(batch);
    assert_eq!(d.focused(), d.session().lookup(5));
    // Move within the modal, then let an unrelated batch land.
    tab(&mut d);
    let moved = d.focused();
    assert_ne!(moved, d.session().lookup(5), "tab moved off the autofocus node");
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 4, text: TextRef::Inline("typed".into()) }] }));
    let _ = d.paint(400, 300);
    assert_eq!(d.focused(), moved, "a batch arriving mid-edit does not steal focus back");
}

#[test]
fn a_node_may_claim_the_arrows_and_keep_enter_as_a_press() {
    let props = vec![(5, vec![(1, Value::List(vec![Value::Str("ArrowLeft".into()), Value::Str("ArrowRight".into())]))])];
    let mut d = driver(page(props, &["keys"], Some(5)));
    // Focus the claimant.
    while d.focused() != d.session().lookup(5) {
        tab(&mut d);
    }
    let out = press(&mut d, "ArrowRight");
    assert_eq!(events(&out), vec![(EventKind::KeyDown, 5)], "the arrow is its own");
    let out = press(&mut d, "Enter");
    let kinds: Vec<EventKind> = events(&out).iter().map(|(k, _)| *k).collect();
    assert!(kinds.contains(&EventKind::Click), "Enter is still the press it stands for: {kinds:?}");
}

#[test]
fn a_key_the_node_did_not_ask_for_is_not_sent_at_all() {
    // The dialog case: a surface listening for Escape must not hear the
    // letters typed into the field inside it, or a handler that closes on a
    // key press would close on every one of them.
    let props = vec![(3, vec![(1, Value::Bool(true)), (2, Value::List(vec![Value::Str("Escape".into())]))])];
    let mut d = driver(page(props, &["modal", "keys"], Some(3)));
    tab(&mut d);
    assert!(d.focused().is_some());
    let out = press(&mut d, "a");
    assert!(events(&out).is_empty(), "a letter is not the modal's business");
    let out = press(&mut d, "Escape");
    assert_eq!(events(&out), vec![(EventKind::KeyDown, 3)], "the one key it asked for arrives");
}

#[test]
fn a_node_with_no_keys_prop_still_claims_everything() {
    let mut d = driver(page(vec![], &[], Some(5)));
    while d.focused() != d.session().lookup(5) {
        tab(&mut d);
    }
    let out = press(&mut d, "Enter");
    let kinds: Vec<EventKind> = events(&out).iter().map(|(k, _)| *k).collect();
    assert!(!kinds.contains(&EventKind::Click), "the old all-or-nothing rule stands: {kinds:?}");
    assert!(kinds.contains(&EventKind::KeyDown));
}
