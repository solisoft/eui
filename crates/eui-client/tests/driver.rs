//! The driver, end to end without a window or a socket: a counter arrives as
//! frames, a click on its button leaves as an event, a text update repaints.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_client::{check_url, Close, Driver, Input};
use eui_proto::*;
use eui_theme::Role;

const ATOM_INC: u32 = 1;
const ATOM_LABEL: u32 = 2;
const ATOM_ITEM_H: u32 = 3;

/// column(1) [ text(2) "0", button box(3) [ text(4) "+" ] ]  — the button
/// carries a server click handler named by atom 1.
fn counter_batch() -> Batch {
    let col = StyleRecord { display: Display::Column, padding: [6; 4], gap: 4, align_items: AlignItems::Start, ..Default::default() };
    let button = StyleRecord { display: Display::Row, padding: [3; 4], bg: ColorRef::role(Role::AccentBase.id()), fg: ColorRef::role(Role::AccentOn.id()), radius: 2, ..Default::default() };
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 0, key: 0, text: Some(TextRef::Inline("0".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 2, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 1 });
    tree.handlers.push((EventKind::Click, Handler::Server(ATOM_INC)));
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 4, style: 0, key: 0, text: Some(TextRef::Atom(ATOM_LABEL)), props: (0, 0), handlers: (0, 0), child_count: 0 });
    Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: ATOM_INC, value: "increment".into() },
            Op::DefAtom { id: ATOM_LABEL, value: "+".into() },
            Op::DefAtom { id: ATOM_ITEM_H, value: "item_height".into() },
            Op::DefStyle { id: 1, record: col },
            Op::DefStyle { id: 2, record: button },
            Op::Mount(tree),
        ],
    }
}

fn welcomed() -> Driver {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] })).is_empty());
    let out = d.handle_frame(Frame::Batch(counter_batch()));
    assert_eq!(out, vec![Frame::Ack { seq: 1 }]);
    d
}

fn centre(d: &mut Driver, id: u32) -> (f32, f32) {
    let _ = d.paint(400, 300);
    let r = d.layout().rect(d.session().lookup(id).unwrap()).unwrap();
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

#[test]
fn hello_carries_the_viewport_and_granted_capabilities() {
    let d = Driver::new(1280.0, 800.0, 2.0, caps::CLIPBOARD_WRITE | 0x8000_0000);
    let Frame::Hello(h) = d.hello() else { panic!() };
    assert_eq!(h.version, PROTOCOL_VERSION);
    assert_eq!((h.viewport.width, h.viewport.height, h.viewport.scale), (1280, 800, 200));
    assert_eq!(h.granted, caps::CLIPBOARD_WRITE, "unknown bits are dropped");
}

#[test]
fn a_click_on_the_button_reaches_the_buttons_handler_through_its_text() {
    let mut d = welcomed();
    // The "+" glyph sits inside the button; a click there lands on the text
    // node and walks up to the button's handler — no bubbling, one walk.
    let (x, y) = centre(&mut d, 4);
    assert!(d.input(Input::PointerMove(x, y)).is_empty(), "no handlers for move/enter");
    assert!(d.input(Input::PointerDown(0)).is_empty(), "no pointer_down handler");
    let out = d.input(Input::PointerUp(0));
    assert_eq!(out.len(), 1);
    let Frame::Event(e) = &out[0] else { panic!("{out:?}") };
    assert_eq!(e.node, 3, "the button, not the text");
    assert_eq!(e.event, EventKind::Click);
    assert_eq!(e.name, ATOM_INC);
    let Value::List(p) = &e.payload else { panic!() };
    assert_eq!(p.len(), 2, "click carries a local point");
}

#[test]
fn a_click_elsewhere_emits_nothing() {
    let mut d = welcomed();
    d.input(Input::PointerMove(390.0, 290.0));
    d.input(Input::PointerDown(0));
    assert!(d.input(Input::PointerUp(0)).is_empty());
}

#[test]
fn press_and_release_on_different_targets_is_not_a_click() {
    let mut d = welcomed();
    let (x, y) = centre(&mut d, 3);
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerMove(390.0, 290.0));
    assert!(d.input(Input::PointerUp(0)).is_empty());
}

#[test]
fn a_server_update_repaints_with_the_new_text() {
    let mut d = welcomed();
    let before = d.paint(400, 300);
    assert!(!d.needs_redraw());
    let out = d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 2, text: TextRef::Inline("1234".into()) }] }));
    assert_eq!(out, vec![Frame::Ack { seq: 2 }]);
    assert!(d.needs_redraw());
    let after = d.paint(400, 300);
    // "0" is one glyph, "1234" four: three more textured quads.
    assert_eq!(after.quads.len(), before.quads.len() + 3);
}

#[test]
fn a_bad_batch_asks_for_a_resync_and_the_next_mount_recovers() {
    let mut d = welcomed();
    let out = d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 99, text: TextRef::Atom(1) }] }));
    assert_eq!(out, vec![Frame::Resync], "a rejected batch is a resync, never an Error");
    assert!(d.session().is_poisoned());
    let mut fresh = counter_batch();
    fresh.seq = 3;
    fresh.ops.retain(|op| !matches!(op, Op::DefAtom { .. } | Op::DefStyle { .. }));
    assert_eq!(d.handle_frame(Frame::Batch(fresh)), vec![Frame::Ack { seq: 3 }]);
    assert!(!d.session().is_poisoned());
}

#[test]
fn pings_are_answered_and_errors_close() {
    let mut d = welcomed();
    assert_eq!(d.handle_frame(Frame::Ping([1; 8])), vec![Frame::Pong([1; 8])]);
    assert!(d.closed().is_none());
    d.handle_frame(Frame::Error { code: 7, message: "bye".into() });
    assert_eq!(d.closed(), Some(&Close::ServerError(7, "bye".into())));
}

#[test]
fn a_client_only_frame_from_the_server_is_a_protocol_error() {
    let mut d = welcomed();
    let out = d.handle_frame(Frame::Resync);
    assert!(matches!(out.as_slice(), [Frame::Error { code: 101, .. }]));
    assert!(matches!(d.closed(), Some(Close::Protocol(_))));
}

#[test]
fn an_unusable_version_is_refused() {
    let mut d = Driver::new(100.0, 100.0, 1.0, 0);
    let out = d.handle_frame(Frame::Welcome(Welcome { version: 9, session: [0; 16] }));
    assert!(matches!(out.as_slice(), [Frame::Error { code: 100, .. }]));
    assert_eq!(d.closed(), Some(&Close::Version(9)));
}

#[test]
fn resize_and_mode_changes_report_the_viewport_and_relayout() {
    let mut d = welcomed();
    let out = d.input(Input::Resized(800.0, 600.0, 2.0));
    let [Frame::Viewport(v)] = out.as_slice() else { panic!("{out:?}") };
    assert_eq!((v.width, v.height, v.scale), (800, 600, 200));
    assert!(d.needs_redraw());
    let light = d.paint(1600, 1200);
    let out = d.input(Input::Mode(ThemeMode::Dark));
    let [Frame::Viewport(v)] = out.as_slice() else { panic!() };
    assert_eq!(v.mode, ThemeMode::Dark);
    let dark = d.paint(1600, 1200);
    assert_ne!(light.clear, dark.clear, "dark mode changed the clear colour without a round trip");
    assert_eq!(light.quads.len(), dark.quads.len());
}

#[test]
fn typing_into_a_field_edits_locally_and_commits_on_enter() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Input, id: 2, style: 0, key: 0, text: Some(TextRef::Inline("ab".into())), props: (0, 0), handlers: (0, 2), child_count: 0 });
    tree.handlers.push((EventKind::Change, Handler::Server(1)));
    tree.handlers.push((EventKind::Submit, Handler::Server(2)));
    let batch = Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "changed".into() },
            Op::DefAtom { id: 2, value: "submitted".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, ..Default::default() } },
            Op::Mount(tree),
        ],
    };
    d.handle_frame(Frame::Batch(batch));
    let (x, y) = centre(&mut d, 2);
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    assert_eq!(d.focused(), d.session().lookup(2));
    assert!(d.input(Input::Text("c".into())).is_empty(), "no text_input handler");
    d.input(Input::Key { key: "Backspace".into(), modifiers: 0, down: true });
    d.input(Input::Text("dé".into()));
    let ix = d.session().lookup(2).unwrap();
    assert_eq!(d.session().text_of(ix), Some("abdé"), "the field shows the edit before the server sees it");
    let out = d.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true });
    let names: Vec<(EventKind, u32, Value)> = out.iter().filter_map(|f| if let Frame::Event(e) = f { Some((e.event, e.name, e.payload.clone())) } else { None }).collect();
    assert_eq!(names[0], (EventKind::Change, 1, Value::Str("abdé".into())));
    assert_eq!(names[1], (EventKind::Submit, 2, Value::Null));
    // Blurring commits too, and typing with nothing focused goes nowhere.
    assert!(d.input(Input::Unfocused).iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::Change)));
    assert!(d.input(Input::Text("z".into())).is_empty());
}

#[test]
fn wheel_over_a_list_scrolls_it_and_reports_the_offset() {
    let mut d = Driver::new(200.0, 100.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::List, id: 1, style: 1, key: 0, text: None, props: (0, 1), handlers: (0, 1), child_count: 100 });
    tree.props.push((ATOM_ITEM_H, Value::Int(20)));
    tree.handlers.push((EventKind::Scroll, Handler::Server(1)));
    for i in 0..100 {
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 10 + i, style: 0, key: 0, text: Some(TextRef::Inline("row".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    }
    let batch = Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "scrolled".into() },
            Op::DefAtom { id: 2, value: "-".into() },
            Op::DefAtom { id: ATOM_ITEM_H, value: "item_height".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, ..Default::default() } },
            Op::Mount(tree),
        ],
    };
    d.handle_frame(Frame::Batch(batch));
    d.input(Input::PointerMove(50.0, 50.0));
    let out = d.input(Input::Wheel(0.0, 120.0));
    let [Frame::Event(e)] = out.as_slice() else { panic!("{out:?}") };
    assert_eq!((e.node, e.event), (1, EventKind::Scroll));
    assert_eq!(e.payload, Value::List(vec![Value::Int(0), Value::Int(120)]));
    // Past the end clamps and, once clamped, stops reporting.
    d.input(Input::Wheel(0.0, 100_000.0));
    let root = d.session().root().unwrap();
    let (_, y) = d.session().node(root).unwrap().scroll;
    assert!(y > 1_500 && y <= 2_000, "clamped to content: {y}");
    assert!(d.input(Input::Wheel(0.0, 10.0)).is_empty());
    // The first row is now off screen and the last one visible.
    let _ = d.paint(200, 100);
    let last = d.layout().rect(d.session().lookup(109).unwrap()).unwrap();
    assert!(last.y < 100.0 && last.y >= 0.0, "{last:?}");
}

#[test]
fn insecure_urls_are_refused_outside_debug_loopback() {
    assert!(check_url("wss://app.example/_eui/session").is_ok());
    assert!(check_url("ws://app.example/_eui/session").is_err());
    assert!(check_url("http://127.0.0.1/_eui/session").is_err());
    // Loopback over ws:// needs both a debug build and the explicit opt-in.
    std::env::remove_var("EUI_ALLOW_INSECURE_LOOPBACK");
    assert!(check_url("ws://127.0.0.1:1/_eui/session").is_err());
}


#[test]
fn a_local_handler_updates_the_tree_without_a_round_trip() {
    use eui_vm::Asm;
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    // Root carries the state; node 2 shows it; node 3 is a local button.
    const COUNT: u32 = 1;
    const INC: u32 = 2;
    const VALUE_KEY: u32 = 3;
    // set_text names the node by its key atom, not by a render's id.
    let chunk = Asm::new(2).load(COUNT).push_int(1).op(0x10).op(0x06).store(COUNT).op(0x1A).set_text(VALUE_KEY).ret();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 2 });
    tree.props.push((COUNT, Value::Int(41)));
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 0, key: VALUE_KEY, text: Some(TextRef::Inline("41".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 2, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 1 });
    tree.handlers.push((EventKind::Click, Handler::LocalThenServer { chunk: 1, name: INC }));
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 4, style: 0, key: 0, text: Some(TextRef::Inline("+".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    let batch = Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: COUNT, value: "count".into() },
            Op::DefAtom { id: INC, value: "increment".into() },
            Op::DefAtom { id: VALUE_KEY, value: "value".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, padding: [4; 4], gap: 3, align_items: AlignItems::Start, ..Default::default() } },
            Op::DefStyle { id: 2, record: StyleRecord { padding: [3; 4], bg: ColorRef::role(Role::AccentBase.id()), ..Default::default() } },
            Op::DefChunkBytes { id: 1, bytes: chunk },
            Op::Mount(tree),
        ],
    };
    assert_eq!(d.handle_frame(Frame::Batch(batch)), vec![Frame::Ack { seq: 1 }]);
    let (x, y) = centre(&mut d, 4);
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    let out = d.input(Input::PointerUp(0));
    // The tree changed locally, before any frame went out …
    let value = d.session().lookup(2).unwrap();
    assert_eq!(d.session().text_of(value), Some("42"));
    assert_eq!(d.session().root_prop(COUNT), Some(&Value::Int(42)));
    assert!(d.needs_redraw());
    // … and exactly one server event follows, the LocalThenServer one.
    assert_eq!(out.len(), 1);
    let Frame::Event(e) = &out[0] else { panic!() };
    assert_eq!((e.node, e.event, e.name), (3, EventKind::Click, INC));
    // Twice more: the local copy keeps counting without the server.
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    assert_eq!(d.session().text_of(value), Some("44"));
    // The server's answer wins over the local copy.
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 2, text: TextRef::Inline("100".into()) }] }));
    assert_eq!(d.session().text_of(value), Some("100"));
}

#[test]
fn a_chunk_that_fails_verification_is_inert_and_sends_nothing() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 0, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 0 });
    tree.handlers.push((EventKind::Click, Handler::LocalThenServer { chunk: 1, name: 1 }));
    let batch = Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "x".into() },
            Op::DefChunkBytes { id: 1, bytes: b"EUIC\x01\x01\x10\x40".to_vec() }, // add on an empty stack
            Op::Mount(tree),
        ],
    };
    d.handle_frame(Frame::Batch(batch));
    let _ = d.paint(400, 300);
    d.input(Input::PointerMove(10.0, 10.0));
    d.input(Input::PointerDown(0));
    assert!(d.input(Input::PointerUp(0)).is_empty(), "an aborted local handler sends nothing");
}
