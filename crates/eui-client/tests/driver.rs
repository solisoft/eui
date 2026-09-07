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
    // The point is local to the button — the node the event names — not to
    // the glyph under the pointer.
    let button = d.layout().rect(d.session().lookup(3).unwrap()).unwrap();
    assert_eq!(e.payload, Value::List(vec![Value::Float(f64::from(x - button.x)), Value::Float(f64::from(y - button.y))]));
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
    // Blurring commits too — but the server already has this value, so
    // nothing is repeated; and typing with nothing focused goes nowhere.
    assert!(d.input(Input::Unfocused).iter().all(|f| !matches!(f, Frame::Event(e) if e.event == EventKind::Change)));
    assert!(d.input(Input::Text("z".into())).is_empty());
    // Edit again, then blur: now it is a change.
    let (x, y) = centre(&mut d, 2);
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    d.input(Input::Text("!".into()));
    assert!(d.input(Input::Unfocused).iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::Change && e.payload == Value::Str("abdé!".into()))));
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

/// A form: an input, a button, a second input — the shape spec 03 §3 is about.
fn form_batch() -> Batch {
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 3 });
    tree.nodes.push(FlatNode { kind: NodeKind::Input, id: 2, style: 0, key: 0, text: Some(TextRef::Inline("a".into())), props: (0, 0), handlers: (0, 1), child_count: 0 });
    tree.handlers.push((EventKind::Change, Handler::Server(1)));
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 0, key: 0, text: None, props: (0, 0), handlers: (1, 1), child_count: 1 });
    tree.handlers.push((EventKind::Click, Handler::Server(2)));
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 4, style: 0, key: 0, text: Some(TextRef::Inline("Save".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    tree.nodes.push(FlatNode { kind: NodeKind::Input, id: 5, style: 0, key: 0, text: Some(TextRef::Inline("".into())), props: (0, 0), handlers: (2, 1), child_count: 0 });
    tree.handlers.push((EventKind::Focus, Handler::Server(3)));
    Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "changed".into() },
            Op::DefAtom { id: 2, value: "save".into() },
            Op::DefAtom { id: 3, value: "focused".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, padding: [6; 4], gap: 4, ..Default::default() } },
            Op::Mount(tree),
        ],
    }
}

fn events(out: &[Frame]) -> Vec<(EventKind, u32, u32)> {
    out.iter().filter_map(|f| if let Frame::Event(e) = f { Some((e.event, e.node, e.name)) } else { None }).collect()
}

fn tab(d: &mut Driver, shift: bool) -> Vec<Frame> {
    let mut out = d.input(Input::Key { key: "Tab".into(), modifiers: u32::from(shift), down: true });
    out.extend(d.input(Input::Key { key: "Tab".into(), modifiers: u32::from(shift), down: false }));
    out
}

#[test]
fn tab_walks_editable_and_activatable_nodes_in_document_order_and_wraps() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(form_batch()));
    // Nothing focused: Tab lands on the first focusable, and is never reported.
    assert!(tab(&mut d, false).is_empty());
    assert_eq!(d.focused(), d.session().lookup(2));
    assert!(tab(&mut d, false).is_empty(), "leaving an unedited field is not a change");
    assert_eq!(d.focused(), d.session().lookup(3), "the button is activatable, the text inside it is not");
    // The third node holds a focus handler: the server hears about it.
    assert_eq!(events(&tab(&mut d, false)), vec![(EventKind::Focus, 5, 3)]);
    assert_eq!(d.focused(), d.session().lookup(5));
    // Wraps, both ways.
    assert!(events(&tab(&mut d, false)).is_empty());
    assert_eq!(d.focused(), d.session().lookup(2));
    tab(&mut d, true);
    assert_eq!(d.focused(), d.session().lookup(5));
    // Escape blurs; nothing is reported for the key itself.
    assert!(events(&d.input(Input::Key { key: "Escape".into(), modifiers: 0, down: true })).is_empty());
    assert_eq!(d.focused(), None);
}

#[test]
fn enter_and_space_click_the_focused_button_at_its_centre() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(form_batch()));
    tab(&mut d, false);
    tab(&mut d, false);
    assert_eq!(d.focused(), d.session().lookup(3));
    let out = d.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true });
    let click = out.iter().find_map(|f| if let Frame::Event(e) = f { (e.event == EventKind::Click).then_some(e) } else { None }).expect("a click");
    assert_eq!((click.node, click.name), (3, 2));
    let r = d.layout().rect(d.session().lookup(3).unwrap()).unwrap();
    assert_eq!(click.payload, Value::List(vec![Value::Float(f64::from(r.w / 2.0)), Value::Float(f64::from(r.h / 2.0))]));
    // Space too; the key itself is reported only where a handler listens (none here).
    let out = d.input(Input::Key { key: " ".into(), modifiers: 0, down: true });
    assert_eq!(events(&out), vec![(EventKind::Click, 3, 2)]);
    // Enter in a field is a submit, not a click.
    tab(&mut d, false);
    assert!(events(&d.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true })).iter().all(|e| e.0 != EventKind::Click));
}

#[test]
fn the_focus_ring_is_painted_for_keyboard_and_server_focus_only() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(form_batch()));
    let ring_around = |list: &eui_render::DrawList, r: eui_layout::Rect| {
        list.quads.iter().any(|q| q.params[1] == 2.0 && q.rect == [r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0])
    };
    let (x, y) = centre(&mut d, 2);
    let field = d.layout().rect(d.session().lookup(2).unwrap()).unwrap();
    // Pointer focus: no ring.
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    assert_eq!(d.focused(), d.session().lookup(2));
    assert!(!ring_around(&d.paint(400, 300), field));
    // Keyboard focus: ring on the button, none on the field.
    tab(&mut d, false);
    let button = d.layout().rect(d.session().lookup(3).unwrap()).unwrap();
    let list = d.paint(400, 300);
    assert!(ring_around(&list, button));
    assert!(!ring_around(&list, field));
    // The server may focus a node; that shows the ring as the keyboard would.
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::Focus { node: 5 }] }));
    assert_eq!(d.focused(), d.session().lookup(5));
    let other = d.layout().rect(d.session().lookup(5).unwrap()).unwrap();
    assert!(ring_around(&d.paint(400, 300), other));
    // A pointer click elsewhere takes it away.
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    assert!(!ring_around(&d.paint(400, 300), other));
}

/// The button's box: the first quad that is not a glyph.
fn box_fill(list: &eui_render::DrawList) -> [f32; 4] {
    list.quads.iter().find(|q| q.params[2] == 0.0).expect("a box").fill
}

#[test]
fn a_style_change_with_a_transition_fades_over_the_motion_scale() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    let t0 = Instant::now();
    d.tick(t0);
    let accent = box_fill(&d.paint(400, 300));
    // Restyle the button: danger background, `base` motion (180 ms).
    let danger = StyleRecord { display: Display::Row, padding: [3; 4], bg: ColorRef::role(Role::DangerBase.id()), fg: ColorRef::role(Role::AccentOn.id()), radius: 2, transition: 2, ..Default::default() };
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::DefStyle { id: 3, record: danger }, Op::SetStyle { node: 3, style: 3 }] }));
    assert!(d.animating());
    assert_eq!(d.next_frame_at(), Some(t0), "a frame is due at once");
    // At t0 the button still wears its old colour.
    let at_start = box_fill(&d.paint(400, 300));
    assert_eq!(at_start, accent);
    assert_eq!(d.next_frame_at(), Some(t0 + Duration::from_millis(16)));
    // Halfway: somewhere between, and a frame is due when asked at that time.
    assert!(!d.tick(t0 + Duration::from_millis(5)), "not due yet");
    assert!(d.tick(t0 + Duration::from_millis(90)));
    let mid = box_fill(&d.paint(400, 300));
    assert!(mid != accent && mid[0] != 0.0, "{mid:?}");
    // Past the end: exactly the new colour, and the driver is at rest again.
    d.tick(t0 + Duration::from_millis(200));
    let end = box_fill(&d.paint(400, 300));
    assert_eq!(end, eui_render::linear(d.theme_color(Role::DangerBase)));
    assert!(!d.animating());
    assert_eq!(d.next_frame_at(), None);
    assert!(!d.tick(t0 + Duration::from_millis(300)));
}

#[test]
fn a_style_change_without_a_transition_is_immediate() {
    let mut d = welcomed();
    let danger = StyleRecord { display: Display::Row, padding: [3; 4], bg: ColorRef::role(Role::DangerBase.id()), ..Default::default() };
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::DefStyle { id: 3, record: danger }, Op::SetStyle { node: 3, style: 3 }] }));
    assert!(!d.animating());
    assert_eq!(box_fill(&d.paint(400, 300)), eui_render::linear(d.theme_color(Role::DangerBase)));
}

#[test]
fn an_ime_composition_shows_in_the_field_and_reports_only_on_commit() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(form_batch()));
    assert_eq!(d.ime_area(), None, "nothing focused: no input method");
    tab(&mut d, false);
    let field = d.session().lookup(2).unwrap();
    assert_eq!(d.ime_area(), d.layout().rect(field));
    // Composing: the field shows it, nothing is sent.
    assert!(d.input(Input::ImePreedit("か".into())).is_empty());
    assert_eq!(d.session().text_of(field), Some("aか"));
    assert!(d.input(Input::ImePreedit("かん".into())).is_empty());
    assert_eq!(d.session().text_of(field), Some("aかん"));
    // Committing inserts once; there is no text_input handler, so still nothing on the wire.
    assert!(d.input(Input::ImeCommit("感".into())).is_empty());
    assert_eq!(d.session().text_of(field), Some("a感"));
    // A composition abandoned by leaving the field is dropped; the commit is what changed.
    d.input(Input::ImePreedit("x".into()));
    assert_eq!(d.session().text_of(field), Some("a感x"));
    let out = d.input(Input::Unfocused);
    assert_eq!(d.session().text_of(field), Some("a感"));
    assert_eq!(events(&out), vec![(EventKind::Change, 2, 1)]);
    assert_eq!(d.ime_area(), None);
    // Focus on a button: no input method either.
    tab(&mut d, false);
    tab(&mut d, false);
    assert_eq!(d.focused(), d.session().lookup(3));
    assert_eq!(d.ime_area(), None);
}

#[cfg(feature = "a11y")]
#[test]
fn the_accessibility_tree_names_buttons_fields_and_labels_and_follows_focus() {
    use accesskit::{Action, NodeId, Role};
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(form_batch()));
    let _ = d.paint(400, 300);
    let tree = d.accessibility_tree();
    let by_role = |r: Role| tree.nodes.iter().filter(|(_, n)| n.role() == r).count();
    assert_eq!(by_role(Role::Window), 1);
    assert_eq!(by_role(Role::TextInput), 2);
    assert_eq!(by_role(Role::Button), 1, "the box with the click handler");
    assert_eq!(by_role(Role::Label), 0, "the button's text is its name, not a child");
    let button = tree.nodes.iter().find(|(_, n)| n.role() == Role::Button).unwrap();
    assert_eq!(button.1.label(), Some("Save"));
    assert!(button.1.supports_action(Action::Click));
    let field = tree.nodes.iter().find(|(_, n)| n.role() == Role::TextInput).unwrap();
    assert_eq!(field.1.value(), Some("a"));
    assert_eq!(tree.focus, NodeId(0), "nothing focused: the window");
    // Focus follows Tab, and an assistive technology's click is a keyboard press.
    tab(&mut d, false);
    tab(&mut d, false);
    let focused = d.accessibility_tree().focus;
    assert_eq!(d.node_for_accessibility(focused), d.session().lookup(3));
    let ix = d.node_for_accessibility(button.0).unwrap();
    let out = d.activate_node(ix);
    assert_eq!(events(&out), vec![(EventKind::Click, 3, 2)]);
}

fn key(d: &mut Driver, k: &str, modifiers: u32) -> Vec<Frame> {
    d.input(Input::Key { key: k.into(), modifiers, down: true })
}

fn field_text(d: &Driver) -> String {
    d.session().text_of(d.session().lookup(2).unwrap()).unwrap_or("").to_owned()
}

#[test]
fn the_caret_selection_and_clipboard_belong_to_the_client() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(form_batch()));
    tab(&mut d, false); // the field, value "a", caret at the end
    d.input(Input::Text("bc".into()));
    assert_eq!(field_text(&d), "abc");
    key(&mut d, "ArrowLeft", 0);
    key(&mut d, "ArrowLeft", 0);
    d.input(Input::Text("X".into()));
    assert_eq!(field_text(&d), "aXbc", "typing inserts at the caret");
    key(&mut d, "Home", 0);
    key(&mut d, "Delete", 0);
    assert_eq!(field_text(&d), "Xbc");
    key(&mut d, "End", 0);
    key(&mut d, "Backspace", 0);
    assert_eq!(field_text(&d), "Xb");
    // Shift+Arrow selects; Ctrl+C copies it; typing replaces it.
    key(&mut d, "ArrowLeft", 1);
    key(&mut d, "ArrowLeft", 1);
    assert!(key(&mut d, "c", 2).iter().all(|f| !matches!(f, Frame::Event(_))), "copying is local");
    assert_eq!(d.take_clipboard(), Some("Xb".into()));
    assert_eq!(d.take_clipboard(), None, "taken once");
    d.input(Input::Text("Y".into()));
    assert_eq!(field_text(&d), "Y");
    // Ctrl+A, Ctrl+X: everything to the clipboard, the field empty.
    key(&mut d, "a", 2);
    key(&mut d, "x", 2);
    assert_eq!(d.take_clipboard(), Some("Y".into()));
    assert_eq!(field_text(&d), "");
    // Paste is an insertion like any other; word motion; replacing a word.
    d.input(Input::Paste("hello world".into()));
    assert_eq!(field_text(&d), "hello world");
    key(&mut d, "ArrowLeft", 2); // ⌘ or Ctrl: by word
    key(&mut d, "ArrowRight", 1 | 2);
    d.input(Input::Paste("there".into()));
    assert_eq!(field_text(&d), "hello there");
    // An IME composition is shown at the caret, not at the end.
    key(&mut d, "Home", 0);
    key(&mut d, "ArrowRight", 0);
    d.input(Input::ImePreedit("か".into()));
    assert_eq!(field_text(&d), "hかello there");
    d.input(Input::ImeCommit("感".into()));
    assert_eq!(field_text(&d), "h感ello there");
    // Leaving the field reports the value once.
    let out = d.input(Input::Unfocused);
    assert!(out.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::Change && e.payload == Value::Str("h感ello there".into()))));
}

#[test]
fn a_click_places_the_caret_and_a_drag_selects() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(form_batch()));
    tab(&mut d, false);
    d.input(Input::Text("hello".into()));
    assert_eq!(field_text(&d), "ahello");
    let _ = d.paint(400, 300);
    let r = d.layout().rect(d.session().lookup(2).unwrap()).unwrap();
    // A click at the very left puts the caret before everything.
    d.input(Input::PointerMove(r.x + 0.5, r.y + r.h / 2.0));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    d.input(Input::Text("Z".into()));
    assert_eq!(field_text(&d), "Zahello");
    // Press at the left, drag far right: everything selected; typing replaces it.
    d.input(Input::PointerMove(r.x + 0.5, r.y + r.h / 2.0));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerMove(r.x + r.w + 50.0, r.y + r.h / 2.0));
    d.input(Input::PointerUp(0));
    d.input(Input::Text("!".into()));
    assert_eq!(field_text(&d), "!");
    // The painter is told where the caret is.
    let list = d.paint(400, 300);
    assert!(list.quads.iter().any(|q| q.params[2] == 0.0 && q.rect[2] == 1.0), "a one-px caret is drawn");
}

#[test]
fn a_click_lands_the_caret_on_the_glyph_under_the_pointer() {
    for scale in [1.0f32, 1.5, 2.0] {
        let mut d = Driver::new(400.0, 300.0, scale, 0);
        d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
        d.handle_frame(Frame::Batch(form_batch()));
        tab(&mut d, false);
        key(&mut d, "a", 2);
        d.input(Input::Text("hello world".into()));
        assert_eq!(field_text(&d), "hello world");
        d.input(Input::Unfocused);
        let _ = d.paint(400, 300);
        let field = d.session().lookup(2).unwrap();
        let r = d.layout().rect(field).unwrap();
        // Where the painter would draw the glyphs: the same shaping the
        // driver uses, from the content box.
        let mut engine = eui_text::TextEngine::new();
        let style = eui_layout::Style::resolve(&d.session().style_of(field), &eui_theme::Theme::default().resolve(eui_theme::Viewer::default()));
        let shaped = engine.shape("hello world", style.font, Some(r.w - style.inset_h()), 0);
        let w = shaped.glyphs[6];
        let x = r.x + style.border.l + style.padding.l + w.x + w.w * 0.3;
        let y = r.y + r.h / 2.0;
        d.input(Input::PointerMove(x, y));
        d.input(Input::PointerDown(0));
        d.input(Input::PointerUp(0));
        d.input(Input::Text("|".into()));
        assert_eq!(field_text(&d), "hello |world", "scale {scale}");
        // The caret painted is exactly where the next glyph starts.
        let list = d.paint((400.0 * scale) as u32, (300.0 * scale) as u32);
        let caret = list.quads.iter().find(|q| q.params[2] == 0.0 && q.rect[2] == scale.max(1.0).round()).expect("caret");
        let shaped = engine.shape("hello |world", style.font, Some(r.w - style.inset_h()), 0);
        let expect = ((r.x + style.border.l + style.padding.l + shaped.glyphs[7].x) * scale).round();
        assert!((caret.rect[0] - expect).abs() <= 1.0, "scale {scale}: caret at {} expected {expect}", caret.rect[0]);
    }
}

#[test]
fn a_wheel_notch_scrolls_smoothly_and_reports_once_it_lands() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    // A scroll box of 100 px holding ten 22 px rows, with a scroll handler.
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: 2, style: 11, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 10 });
    tree.handlers.push((EventKind::Scroll, Handler::Server(ATOM_INC)));
    for i in 0..10 {
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 10 + i, style: 0, key: 0, text: Some(TextRef::Inline(format!("row {i}"))), props: (0, 0), handlers: (0, 0), child_count: 0 });
    }
    let ops = vec![
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 2, ops })), vec![Frame::Ack { seq: 2 }]);
    let t0 = Instant::now();
    d.tick(t0);
    let _ = d.paint(400, 300);
    let scroll = d.session().lookup(2).unwrap();
    d.input(Input::PointerMove(50.0, 50.0));
    // One notch: nothing moves yet, a frame is due, nothing is reported.
    assert!(d.input(Input::WheelStep(0.0, 1.0)).is_empty());
    assert!(d.animating());
    assert_eq!(d.session().node(scroll).unwrap().scroll, (0, 0));
    // Mid-way: the offset is somewhere between 0 and 48.
    d.tick(t0 + Duration::from_millis(60));
    let _ = d.paint(400, 300);
    let (_, y) = d.session().node(scroll).unwrap().scroll;
    assert!(y > 0 && y < 48, "{y}");
    assert!(d.take_pending().is_empty(), "not landed yet");
    // A second notch mid-flight retargets to 96 from where the view is.
    d.input(Input::WheelStep(0.0, 1.0));
    d.tick(t0 + Duration::from_millis(400));
    let _ = d.paint(400, 300);
    assert_eq!(d.session().node(scroll).unwrap().scroll, (0, 96));
    assert!(!d.animating());
    let landed = d.take_pending();
    assert_eq!(landed.len(), 1, "one scroll event when it lands: {landed:?}");
    assert!(matches!(&landed[0], Frame::Event(e) if e.event == EventKind::Scroll && e.payload == Value::List(vec![Value::Int(0), Value::Int(96)])));
    assert_eq!(d.next_frame_at(), None);
}
