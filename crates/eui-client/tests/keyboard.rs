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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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

// -------------------------------------------------- keys inside a field
//
// 03 §3.1's "and only those are withheld from the client's own meaning" was
// written for a tab strip taking the arrows. Inside a field it needs three
// tiers, because the naive reading — a named key is the server's — makes the
// field undeletable, and the useful reading is that the client keeps a key it
// has a use for and yields one it does not.

/// A wrapper that may claim keys, holding a field that reports everything a
/// field can report, and a button beside it. `page` cannot serve here: its
/// input carries no handlers, so a `change` or a `submit` would be emitted to
/// nobody and the test would pass on an empty list either way.
///
/// ids: 1 the page, 2 the wrapper, 3 the field, 4 the button.
fn field_page(keys: Option<Vec<&str>>) -> Batch {
    const A_KEYS: u32 = 1;
    const A_SAID: u32 = 2;
    const A_PRESSED: u32 = 3;
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });

    let named = keys.is_some();
    if let Some(list) = keys {
        tree.props.push((A_KEYS, Value::List(list.into_iter().map(|k| Value::Str(k.to_owned())).collect())));
    }
    tree.handlers.push((EventKind::KeyDown, Handler::Server(A_SAID)));
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 2, style: 0, key: 0, text: None, props: (0, u32::from(named)), handlers: (0, 1), child_count: 1 });

    tree.handlers.push((EventKind::Change, Handler::Server(A_SAID)));
    tree.handlers.push((EventKind::Submit, Handler::Server(A_SAID)));
    tree.nodes.push(FlatNode { kind: NodeKind::Input, id: 3, style: 0, key: 0, text: Some(TextRef::Inline(String::new())), props: (0, 0), handlers: (1, 2), child_count: 0 });

    tree.handlers.push((EventKind::Click, Handler::Server(A_PRESSED)));
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 4, style: 0, key: 0, text: None, props: (0, 0), handlers: (3, 1), child_count: 0 });

    Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: A_KEYS, value: "keys".into() },
            Op::DefAtom { id: A_SAID, value: "said".into() },
            Op::DefAtom { id: A_PRESSED, value: "pressed".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, padding: [4; 4], ..Default::default() } },
            Op::Mount(tree),
        ],
    }
}

/// Tab until focus lands on `id`. Counting presses instead would depend on
/// which of the other nodes happens to be focusable in a given tree, and the
/// keyed node in these tests is exactly the one that varies.
fn focus_on(d: &mut Driver, id: u32) {
    for _ in 0..8 {
        if d.focused().and_then(|ix| d.session().node(ix)).map(|n| n.id) == Some(id) {
            return;
        }
        tab(d);
    }
    panic!("nothing focusable with id {id}");
}

fn in_the_field(d: &mut Driver) {
    focus_on(d, 3);
}

fn typed(d: &Driver) -> String {
    d.session().lookup(3).and_then(|ix| d.session().text_of(ix)).unwrap_or("").to_owned()
}

fn type_it(d: &mut Driver, s: &str) {
    d.input(Input::Text(s.into()));
}

/// Tier 1, and the whole reason the tiers exist: naming a key cannot take
/// editing away. A field that cannot be deleted from is not a field.
#[test]
fn naming_a_key_cannot_make_a_field_undeletable() {
    let mut d = driver(field_page(Some(vec!["Backspace"])));
    in_the_field(&mut d);
    type_it(&mut d, "ab");
    assert_eq!(typed(&d), "ab");
    let out = press(&mut d, "Backspace");
    assert_eq!(typed(&d), "a", "the character went, claim or no claim");
    assert!(events(&out).is_empty(), "and the client kept a key it had a use for: {:?}", events(&out));
}

/// Tier 3, and the gesture a tag field is built on. The same key, the same
/// claim, an empty field: now the client has nothing to delete, so it is not
/// the one to answer.
#[test]
fn backspace_with_nothing_to_delete_is_the_servers() {
    let mut d = driver(field_page(Some(vec!["Backspace"])));
    in_the_field(&mut d);
    let out = press(&mut d, "Backspace");
    assert_eq!(events(&out), vec![(EventKind::KeyDown, 2)], "the node that asked for it hears it: {:?}", events(&out));
}

/// The same rule with a caret rather than a value: an arrow that would move
/// nowhere is not the client's either.
#[test]
fn an_arrow_with_nowhere_to_go_is_the_servers() {
    let mut d = driver(field_page(Some(vec!["ArrowLeft"])));
    in_the_field(&mut d);
    type_it(&mut d, "ab");
    // The caret is at the end, so there is somewhere to go.
    let out = press(&mut d, "ArrowLeft");
    assert!(events(&out).is_empty(), "the client moved the caret: {:?}", events(&out));
    // Twice more and it is at 0, where the key means nothing.
    press(&mut d, "ArrowLeft");
    let out = press(&mut d, "ArrowLeft");
    assert_eq!(events(&out), vec![(EventKind::KeyDown, 2)], "and now it is the server's");
}

/// Tier 2. A claim on `Enter` withholds the `submit` — the thing the key
/// *stands for* — and never the `change`, because a server that claimed it to
/// take a highlighted suggestion still needs to know what was typed, and needs
/// it before the key that acts on it.
#[test]
fn enter_in_a_claimed_field_reports_the_value_and_withholds_the_submit() {
    let mut d = driver(field_page(Some(vec!["Enter"])));
    in_the_field(&mut d);
    type_it(&mut d, "ab");
    let out = press(&mut d, "Enter");
    let seen = events(&out);
    assert_eq!(seen.first().map(|(k, _)| *k), Some(EventKind::Change), "the value first: {seen:?}");
    assert!(seen.iter().any(|(k, n)| *k == EventKind::KeyDown && *n == 2), "then the key: {seen:?}");
    assert!(!seen.iter().any(|(k, _)| *k == EventKind::Submit), "and no submit: {seen:?}");
}

/// The negative, so the claim is visibly what does it.
#[test]
fn enter_in_a_field_nobody_claimed_still_submits() {
    let mut d = driver(field_page(None));
    in_the_field(&mut d);
    type_it(&mut d, "ab");
    let out = press(&mut d, "Enter");
    let seen = events(&out);
    assert!(seen.iter().any(|(k, _)| *k == EventKind::Submit), "{seen:?}");
}

/// 03 §3.1's narrowing, and the regression it guards. A node carrying a bare
/// `key_down` claims every key **for itself** — but not on behalf of what is
/// inside it, or a dialog that merely listens would swallow `Enter` from every
/// button in it.
#[test]
fn a_dialog_that_asked_for_keys_does_not_swallow_its_buttons_enter() {
    let mut d = driver(field_page(None));
    focus_on(&mut d, 4);
    let out = press(&mut d, "Enter");
    let seen = events(&out);
    assert!(seen.iter().any(|(k, n)| *k == EventKind::Click && *n == 4), "the button was still pressed: {seen:?}");
}

/// A printable character is never withheld, and cannot be: the text does not
/// arrive as a key at all. This is why comma cannot be a delimiter, and it is
/// worth a vector so nobody tries again.
#[test]
fn a_printable_character_is_never_withheld() {
    let mut d = driver(field_page(Some(vec![","])));
    in_the_field(&mut d);
    type_it(&mut d, ",");
    assert_eq!(typed(&d), ",", "the comma landed in the field regardless");
}
