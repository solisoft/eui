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
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false })).is_empty());
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
fn a_pointer_move_handler_follows_the_press_off_the_node() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 3), child_count: 0 });
    tree.handlers.push((EventKind::PointerMove, Handler::Server(ATOM_INC)));
    tree.handlers.push((EventKind::PointerDown, Handler::Server(ATOM_INC)));
    tree.handlers.push((EventKind::PointerUp, Handler::Server(ATOM_INC)));
    d.handle_frame(Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: ATOM_INC, value: "slider".into() },
            Op::DefStyle { id: 1, record: StyleRecord { width: Dim::Px(100), height: Dim::Px(20), ..Default::default() } },
            Op::Mount(tree),
        ],
    }));
    let _ = d.paint(400, 300);
    let r = d.layout().rect(d.session().lookup(1).unwrap()).unwrap();
    let out = d.input(Input::PointerMove(r.x + 10.0, r.y + r.h / 2.0));
    assert!(out.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::PointerMove && e.node == 1)));
    let _ = d.input(Input::PointerDown(0));
    assert!(d.input(Input::PointerMove(r.x + r.w + 40.0, r.y + r.h / 2.0)).is_empty(), "the input holds it for the frame");
    // 06 §1: coalesced, at most one a frame carrying the latest value. The
    // frame is where the server hears it -- still named for the node that
    // was pressed, and still in that node's coordinates, though the
    // pointer has left the box.
    let _ = d.paint(400, 300);
    let sent = d.take_pending();
    let Some(Frame::Event(e)) = sent.iter().find(|f| matches!(f, Frame::Event(ev) if ev.event == EventKind::PointerMove)) else {
        panic!("expected the move at the frame that followed it, got {sent:?}");
    };
    assert_eq!(e.node, 1);
    let Value::List(p) = &e.payload else { panic!("{:?}", e.payload) };
    let Value::Float(x) = &p[0] else { panic!("{:?}", p[0]) };
    assert!(*x > 100.0, "local x past the box: {x}");
    let out = d.input(Input::PointerUp(0));
    assert!(out.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::PointerUp && e.node == 1)), "{out:?}");
}

#[test]
fn a_captured_pointer_move_is_emitted_while_layout_is_owed() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 2), child_count: 0 });
    tree.handlers.push((EventKind::PointerMove, Handler::Server(ATOM_INC)));
    tree.handlers.push((EventKind::PointerDown, Handler::Server(ATOM_INC)));
    d.handle_frame(Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: ATOM_INC, value: "slider".into() },
            Op::DefStyle { id: 1, record: StyleRecord { width: Dim::Px(100), height: Dim::Px(20), ..Default::default() } },
            Op::Mount(tree),
        ],
    }));
    let _ = d.paint(400, 300);
    let r = d.layout().rect(d.session().lookup(1).unwrap()).unwrap();
    d.input(Input::PointerMove(r.x + 10.0, r.y + r.h / 2.0));
    let _ = d.input(Input::PointerDown(0));
    d.handle_frame(Frame::Batch(Batch {
        seq: 2,
        ops: vec![Op::DefStyle { id: 2, record: StyleRecord { width: Dim::Px(100), height: Dim::Px(20), padding: [1, 0, 0, 0], ..Default::default() } }, Op::SetStyle { node: 1, style: 2 }],
    }));
    assert!(d.input(Input::PointerMove(r.x + 40.0, r.y + r.h / 2.0)).is_empty(), "captured moves stay local");
    let out = d.input(Input::PointerUp(0));
    assert!(out.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::PointerMove && e.node == 1)), "last move on release while layout owed: {out:?}");
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
    let out = d.handle_frame(Frame::Welcome(Welcome { version: 9, session: [0; 16], resumed: false }));
    assert!(matches!(out.as_slice(), [Frame::Error { code: 100, .. }]));
    assert_eq!(d.closed(), Some(&Close::Version(9)));
}

#[test]
fn resize_and_mode_changes_report_the_viewport_and_relayout() {
    let mut d = welcomed();
    assert!(d.input(Input::Resized(800.0, 600.0, 2.0)).is_empty(), "viewport waits until the resize settles");
    assert!(d.needs_redraw());
    d.tick(std::time::Instant::now() + std::time::Duration::from_millis(50));
    let _ = d.paint(1600, 1200);
    let out = d.take_pending();
    let Some(Frame::Viewport(v)) = out.iter().find(|f| matches!(f, Frame::Viewport(_))) else { panic!("{out:?}") };
    assert_eq!((v.width, v.height, v.scale), (800, 600, 200));
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
fn wheel_over_a_fitted_list_scrolls_the_page() {
    // A list that fits its rows must not eat the wheel: the page underneath
    // is the scroller that can still move. This is the data-grid-in-a-gallery
    // case — eight rows in a 256 px list, inside a page `scroll`.
    let mut d = Driver::new(200.0, 80.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    let page = StyleRecord { display: Display::Column, height: Dim::Px(80), ..Default::default() };
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let list = StyleRecord { display: Display::Column, height: Dim::Px(40), ..Default::default() };
    let pad = StyleRecord { height: Dim::Px(200), ..Default::default() };
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 1 });
    tree.handlers.push((EventKind::Scroll, Handler::Server(1)));
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 2, style: 2, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
    tree.nodes.push(FlatNode { kind: NodeKind::List, id: 3, style: 3, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 2 });
    tree.props.push((ATOM_ITEM_H, Value::Int(20)));
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 4, style: 0, key: 0, text: Some(TextRef::Inline("a".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 5, style: 0, key: 0, text: Some(TextRef::Inline("b".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 6, style: 4, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
    d.handle_frame(Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "scrolled".into() },
            Op::DefAtom { id: ATOM_ITEM_H, value: "item_height".into() },
            Op::DefStyle { id: 1, record: page },
            Op::DefStyle { id: 2, record: col },
            Op::DefStyle { id: 3, record: list },
            Op::DefStyle { id: 4, record: pad },
            Op::Mount(tree),
        ],
    }));
    let _ = d.paint(200, 80);
    let r = d.layout().rect(d.session().lookup(3).unwrap()).unwrap();
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    let out = d.input(Input::Wheel(0.0, 40.0));
    let [Frame::Event(e)] = out.as_slice() else { panic!("expected the page to scroll, got {out:?}") };
    assert_eq!((e.node, e.event), (1, EventKind::Scroll), "the fitted list must not swallow the wheel");
}

#[test]
fn wheel_over_an_open_select_scrolls_its_options_not_the_page() {
    // 04 §5: an open select is a popover, and one with more options than the
    // window has room for holds them in a `scroll`. The wheel over it has to
    // find that scroller — it used to find the page's, so the options went by
    // underneath and the ones off the bottom could not be reached at all.
    let mut d = Driver::new(200.0, 100.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    let page = StyleRecord { display: Display::Column, ..Default::default() };
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let stack = StyleRecord { display: Display::Stack, ..Default::default() };
    let anchor = StyleRecord { height: Dim::Px(20), ..Default::default() };
    let panel = StyleRecord { display: Display::Column, position: Position::Absolute, ..Default::default() };
    let option = StyleRecord { width: Dim::Px(120), height: Dim::Px(20), ..Default::default() };
    let pad = StyleRecord { height: Dim::Px(400), ..Default::default() };
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 1 });
    tree.handlers.push((EventKind::Scroll, Handler::Server(1)));
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 2, style: 2, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 3, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 4, style: 4, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
    tree.nodes.push(FlatNode { kind: NodeKind::Overlay, id: 5, style: 5, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: 6, style: 2, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 20 });
    for i in 0..20 {
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 10 + i, style: 6, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
    }
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 7, style: 7, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
    d.handle_frame(Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "scrolled".into() },
            Op::DefStyle { id: 1, record: page },
            Op::DefStyle { id: 2, record: col },
            Op::DefStyle { id: 3, record: stack },
            Op::DefStyle { id: 4, record: anchor },
            Op::DefStyle { id: 5, record: panel },
            Op::DefStyle { id: 6, record: option },
            Op::DefStyle { id: 7, record: pad },
            Op::Mount(tree),
        ],
    }));
    let _ = d.paint(200, 100);
    // Twenty 20 px options are 400 px of list; the panel is the window's 100.
    let over = d.session().lookup(5).unwrap();
    let list = d.session().lookup(6).unwrap();
    let r = d.layout().rect(over).unwrap();
    assert!(r.h <= 100.0 && r.y + r.h <= 100.01, "the panel is inside the window: {r:?}");
    assert_eq!(d.layout().content_size(list).map(|c| c.h), Some(400.0), "with every option in it");
    d.input(Input::PointerMove(r.x + r.w / 2.0, r.y + r.h / 2.0));
    d.input(Input::Wheel(0.0, 60.0));
    assert_eq!(d.session().node(list).unwrap().scroll.1, 60, "the options moved");
    assert_eq!(d.session().node(d.session().root().unwrap()).unwrap().scroll.1, 0, "the page behind did not");
}

#[test]
fn insecure_urls_are_refused_outside_debug_loopback() {
    assert!(check_url("wss://app.example/_eui/session", false).is_ok());
    assert!(check_url("ws://app.example/_eui/session", false).is_err());
    assert!(check_url("http://127.0.0.1/_eui/session", false).is_err());
    // Loopback over ws:// needs both a debug build and the explicit opt-in.
    std::env::remove_var("EUI_ALLOW_INSECURE_LOOPBACK");
    assert!(check_url("ws://127.0.0.1:1/_eui/session", false).is_err());
    // An embedded host vouches for its own session only. The trust is a
    // parameter, so it cannot spill onto a network session opened beside it.
    assert!(check_url("ws://127.0.0.1:1/_eui/session", true).is_ok());
    assert!(check_url("ws://127.0.0.1:1/_eui/session", false).is_err());
    assert!(check_url("ws://app.example/_eui/session", true).is_err());
}

#[test]
fn a_local_handler_updates_the_tree_without_a_round_trip() {
    use eui_vm::Asm;
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    d.handle_frame(Frame::Batch(form_batch()));
    let ring_around = |list: &eui_render::DrawList, r: eui_layout::Rect| list.quads.iter().any(|q| q.params[1] == 2.0 && q.rect == [r.x - 2.0, r.y - 2.0, r.w + 4.0, r.h + 4.0]);
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
    let quad = |list: &eui_render::DrawList| *list.quads.iter().find(|q| q.params[2] as u32 & (eui_render::TEXTURED | eui_render::TEXTURED_RGBA) == 0 && q.fill[3] > 0.0).expect("a box");
    // Restyle the button: danger background, `base` motion (180 ms).
    let danger =
        StyleRecord { display: Display::Row, padding: [3; 4], bg: ColorRef::role(Role::DangerBase.id()), fg: ColorRef::role(Role::AccentOn.id()), radius: 2, transition: 2, ..Default::default() };
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::DefStyle { id: 3, record: danger }, Op::SetStyle { node: 3, style: 3 }] }));
    assert!(d.animating());
    assert_eq!(d.next_frame_at(), Some(t0), "a frame is due at once");
    // The quad carries both ends and the clock: the vertex stage moves
    // between them, so this list is the frame for the whole transition.
    let start_list = d.paint(400, 300);
    let q = quad(&start_list);
    let danger = eui_render::linear(d.theme_color(Role::DangerBase));
    assert_eq!(q.fill, danger, "where it is going");
    assert_eq!(eui_render::unpack4([q.from[0], q.from[1], q.from[2], q.from[3]]), eui_render::unpack4(eui_render::pack4(accent)), "where it came from");
    assert!(q.params[2] as u32 & eui_render::ANIMATED != 0, "marked for the vertex stage");
    assert_eq!(q.spin[2], 0.0, "it began at this paint");
    assert!((q.spin[3] - 0.18).abs() < 1e-6, "and takes the base motion");
    assert!(start_list.gpu_only, "so the window draws this list again");
    assert_eq!(start_list.repeat_until_ms, 180, "until the transition ends");
    assert_eq!(d.next_frame_at(), Some(t0 + Duration::from_millis(16)));
    // Halfway: the same list, not painted again; the clock is the GPU's.
    assert!(!d.tick(t0 + Duration::from_millis(5)), "not due yet");
    assert!(d.tick(t0 + Duration::from_millis(90)));
    let mid_list = d.paint(400, 300);
    assert!(std::sync::Arc::ptr_eq(&start_list, &mid_list), "the same list halfway");
    assert_eq!(d.spin_repeats(), 1);
    assert_eq!(d.next_frame_at(), Some(t0 + Duration::from_millis(106)), "the next frame at sixty");
    // Past the end: exactly the new colour, no transition on the quad, and
    // the driver is at rest again.
    d.tick(t0 + Duration::from_millis(200));
    let end = quad(&d.paint(400, 300));
    assert_eq!(end.fill, danger);
    assert!(end.params[2] as u32 & eui_render::ANIMATED == 0, "settled");
    assert!(!d.animating());
    assert_eq!(d.next_frame_at(), None);
    assert!(!d.tick(t0 + Duration::from_millis(300)));
}

/// Spec 03 §5 `enter`: a node grafted wearing it arrives from nothing —
/// transparent and unblurred — rather than appearing already there.
#[test]
fn a_node_that_asks_to_enter_fades_and_frosts_in() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    let t0 = Instant::now();
    d.tick(t0);

    let scrim = StyleRecord {
        display: Display::Stack,
        width: Dim::Px(80),
        height: Dim::Px(40),
        bg: ColorRef::role(Role::SurfaceOverlay.id()),
        blur: 16,
        animation: eui_proto::ANIMATION_ENTER,
        transition: 2, // `base`, 180 ms
        ..Default::default()
    };
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Overlay, id: 9, style: 3, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::DefStyle { id: 3, record: scrim }, Op::InsertChild { parent: 1, index: 0, subtree: tree }] }));
    assert!(d.animating(), "the graft started an entrance");

    let pane = |list: &eui_render::DrawList| *list.quads.iter().find(|q| q.rect[2] == 80.0 && q.rect[3] == 40.0).expect("the scrim");

    // At the start it is not there at all: no opacity, and no blur, so the
    // frame has not been asked for a backdrop either.
    let start = d.paint(400, 300);
    assert_eq!(pane(&start).params[3], 0.0, "arrives transparent");
    assert_eq!(start.backdrop, None, "and unblurred, so no extra pass yet");

    // Halfway, both are partway there and the backdrop is being built.
    d.tick(t0 + Duration::from_millis(90));
    let mid = d.paint(400, 300);
    let m = pane(&mid);
    assert!(m.params[3] > 0.0 && m.params[3] < 1.0, "opacity {:?}", m.params[3]);
    assert!(m.extra[2] > 0.0 && m.extra[2] < 16.0, "sigma {:?}", m.extra[2]);
    assert!(mid.backdrop.is_some(), "a partial frost still needs its backdrop");

    // And at the end it is exactly its own record, with the driver at rest.
    d.tick(t0 + Duration::from_millis(200));
    let end = d.paint(400, 300);
    assert_eq!(pane(&end).params[3], 1.0);
    assert_eq!(pane(&end).extra[2], 16.0);
    assert!(!d.animating());
    assert_eq!(d.next_frame_at(), None);
}

/// 03 §5: an entrance dims everything painted for the node, not just the
/// node's own quads — otherwise a dialog's text is at full strength before
/// the card under it has arrived.
#[test]
fn an_entrance_dims_what_is_painted_inside_it() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    let t0 = Instant::now();
    d.tick(t0);
    let panel = StyleRecord {
        display: Display::Column,
        width: Dim::Px(80),
        height: Dim::Px(40),
        bg: ColorRef::role(Role::SurfaceRaised.id()),
        fg: ColorRef::role(Role::TextDefault.id()),
        animation: eui_proto::ANIMATION_ENTER,
        transition: 2,
        ..Default::default()
    };
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Overlay, id: 9, style: 3, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 10, style: 0, key: 0, text: Some(TextRef::Inline("hello".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::DefStyle { id: 3, record: panel }, Op::InsertChild { parent: 1, index: 0, subtree: tree }] }));

    d.tick(t0 + Duration::from_millis(90));
    let list = d.paint(400, 300);
    let pane = list.quads.iter().find(|q| q.rect[2] == 80.0 && q.rect[3] == 40.0).expect("the panel");
    // The panel carries the entrance: from nothing to its own opacity,
    // decelerating, on a clock that began ninety milliseconds ago.
    assert_eq!((pane.extra[3], pane.params[3]), (0.0, 1.0), "panel from {} to {}", pane.extra[3], pane.params[3]);
    assert!(pane.params[2] as u32 & (eui_render::ANIMATED | eui_render::DECELERATE) == eui_render::ANIMATED | eui_render::DECELERATE);
    assert!((pane.spin[2] + 0.09).abs() < 1e-3 && (pane.spin[3] - 0.18).abs() < 1e-6, "when: {:?}", &pane.spin[2..]);
    assert!(list.gpu_only, "and the list is the frame for the whole entrance");
    // Only the glyphs inside the panel — the counter behind it has its own,
    // and those must stay at full strength, which is half of what this
    // checks: the fade descends, and it stops where the node does.
    let inside = |q: &eui_render::Quad, p: &eui_render::Quad| q.rect[0] >= p.rect[0] && q.rect[0] < p.rect[0] + p.rect[2] && q.rect[1] >= p.rect[1] && q.rect[1] < p.rect[1] + p.rect[3];
    let glyphs: Vec<&eui_render::Quad> = list.quads.iter().filter(|q| q.params[2] as u32 & eui_render::TEXTURED != 0).collect();
    let (within, without): (Vec<&eui_render::Quad>, Vec<&eui_render::Quad>) = glyphs.into_iter().partition(|q| inside(q, pane));
    assert!(!within.is_empty(), "the panel's text was painted");
    assert!(!without.is_empty(), "the counter's text is still there to compare against");
    for g in &within {
        assert_eq!((g.extra[3], g.params[3]), (pane.extra[3], pane.params[3]), "a glyph on its own way while its panel is on another: {g:?}");
        assert_eq!((g.spin[2], g.spin[3]), (pane.spin[2], pane.spin[3]), "and on the panel's clock");
        assert!(g.params[2] as u32 & eui_render::DECELERATE != 0);
    }
    for g in &without {
        assert!(g.params[2] as u32 & eui_render::ANIMATED == 0 && g.params[3] == 1.0, "a glyph outside the entrance was dimmed by it");
    }

    // And once it has arrived, everything is back to full strength.
    d.tick(t0 + Duration::from_millis(200));
    let done = d.paint(400, 300);
    assert!(done.quads.iter().filter(|q| q.params[2] as u32 & eui_render::TEXTURED != 0).all(|q| q.params[3] == 1.0 && q.params[2] as u32 & eui_render::ANIMATED == 0));
}

/// The exception is opt-in: the same node without the byte is simply there.
#[test]
fn a_node_without_enter_is_mounted_at_once() {
    let mut d = welcomed();
    let scrim = StyleRecord { display: Display::Stack, width: Dim::Px(80), height: Dim::Px(40), bg: ColorRef::role(Role::SurfaceOverlay.id()), transition: 2, ..Default::default() };
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Overlay, id: 9, style: 3, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::DefStyle { id: 3, record: scrim }, Op::InsertChild { parent: 1, index: 0, subtree: tree }] }));
    assert!(!d.animating(), "a transition alone does not animate a mount");
    let list = d.paint(400, 300);
    assert_eq!(list.quads.iter().find(|q| q.rect[2] == 80.0).expect("the scrim").params[3], 1.0);
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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

#[cfg(has_a11y)]
#[test]
fn the_accessibility_tree_names_buttons_fields_and_labels_and_follows_focus() {
    use eui_client::a11y::AccessRole as Role;
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    d.handle_frame(Frame::Batch(form_batch()));
    let _ = d.paint(400, 300);
    let tree = d.access_snapshot();
    let by_role = |r: Role| tree.nodes.iter().filter(|n| n.role == r).count();
    assert_eq!(by_role(Role::Window), 1);
    assert_eq!(by_role(Role::TextInput), 2);
    assert_eq!(by_role(Role::Button), 1, "the box with the click handler");
    assert_eq!(by_role(Role::Label), 0, "the button's text is its name, not a child");
    let button = tree.nodes.iter().find(|n| n.role == Role::Button).unwrap();
    assert_eq!(button.label, "Save");
    assert!(button.click);
    let field = tree.nodes.iter().find(|n| n.role == Role::TextInput).unwrap();
    assert_eq!(field.value, "a");
    assert_eq!(tree.focus, 0, "nothing focused: the window");
    // Focus follows Tab, and an assistive technology's click is a keyboard press.
    tab(&mut d, false);
    tab(&mut d, false);
    let focused = d.access_snapshot().focus;
    assert_eq!(d.node_for_accessibility(focused), d.session().lookup(3));
    let ix = d.node_for_accessibility(button.id).unwrap();
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
        d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
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
    // The glide starts on the driver's clock, which the input takes from
    // the wall, so every tick below is measured from the notch and not
    // from `t0` -- a loaded machine can spend a hundred milliseconds
    // between the two, and a tick that lands before the start reads as a
    // glide that has not moved.
    let notch = Instant::now();
    assert!(d.input(Input::WheelStep(0.0, 1.0)).is_empty());
    assert!(d.animating());
    assert_eq!(d.session().node(scroll).unwrap().scroll, (0, 0));
    // Mid-way: the tree holds the landing, 100, and the layout knows the
    // content stands somewhere short of it -- between 0 and 100 px below
    // where it was put -- which is where the vertex stage draws it.
    d.tick(notch + Duration::from_millis(60));
    let _ = d.paint(400, 300);
    assert_eq!(d.session().node(scroll).unwrap().scroll, (0, 100), "laid out once, at the landing");
    let dy = d.layout().glide(scroll).expect("gliding").delta.1;
    assert!(dy > 0.0 && dy < 100.0, "{dy}");
    assert!(d.take_pending().is_empty(), "not landed yet");
    // Zero-valued pixel events between notches (a Magic Mouse) change nothing.
    assert!(d.input(Input::Wheel(0.0, 0.0)).is_empty());
    assert!(d.animating(), "a zero delta does not cancel the motion");
    // A second notch mid-flight retargets to 120 (the end) from where the view is.
    let second = Instant::now();
    d.input(Input::WheelStep(0.0, 1.0));
    d.tick(second + Duration::from_millis(400));
    let _ = d.paint(400, 300);
    assert_eq!(d.session().node(scroll).unwrap().scroll, (0, 120));
    assert!(!d.animating());
    let landed = d.take_pending();
    assert_eq!(landed.len(), 1, "one scroll event when it lands: {landed:?}");
    assert!(matches!(&landed[0], Frame::Event(e) if e.event == EventKind::Scroll && e.payload == Value::List(vec![Value::Int(0), Value::Int(120)])));
    assert_eq!(d.next_frame_at(), None);
}

/// 04 §7: a glide is one layout, at the landing, and then the same list
/// drawn again with the vertex stage sliding the content into place. The
/// list names the scroller and its thumb, and every quad in it that moves.
#[test]
fn a_glide_frame_does_not_lay_out() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    let mut d = welcomed();
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
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops }));
    let t0 = Instant::now();
    d.tick(t0);
    let _ = d.paint(400, 300);
    d.input(Input::PointerMove(50.0, 50.0));
    d.input(Input::WheelStep(0.0, 1.0));
    // The paint after the notch lays out once, at the landing.
    let laid = d.relayouts();
    let list = d.paint(400, 300);
    assert_eq!(d.relayouts(), laid + 1, "one layout, at the landing");
    assert_eq!(list.scrollers.len(), 2, "the content and its thumb");
    let content = list.scrollers[0];
    assert_eq!((content.from, content.to), ([0.0, 100.0], [0.0, 0.0]), "the content starts a notch below where it was put and slides up to it");
    assert!((content.dur - 0.1).abs() < 1e-6 && content.t0 == 0.0 && content.curve == 0, "{content:?}");
    let thumb = list.scrollers[1];
    assert!(thumb.from[1] < 0.0 && thumb.to == [0.0, 0.0], "the thumb travels the other way: {thumb:?}");
    let carried = |q: &eui_render::Quad| (q.params[2] as u32 & eui_render::SCROLLER_MASK) >> eui_render::SCROLLER_SHIFT;
    let rows = list.quads.iter().filter(|q| q.params[2] as u32 & eui_render::TEXTURED != 0).count();
    assert!(rows > 0);
    assert!(list.quads.iter().filter(|q| q.params[2] as u32 & eui_render::TEXTURED != 0).all(|q| carried(q) == 1), "every glyph of the content is carried by slot one");
    assert!(list.quads.iter().any(|q| carried(q) == 2), "and the thumb by slot two");
    assert!(list.gpu_only, "so the frames of the glide are this list again");
    assert_eq!(list.repeat_until_ms, 100);
    // The frames between: the same list, no layout.
    for i in 1..=5 {
        d.tick(t0 + Duration::from_millis(16 * i));
        let again = d.paint(400, 300);
        assert!(Arc::ptr_eq(&list, &again), "frame {i} is the same list");
    }
    assert_eq!(d.relayouts(), laid + 1, "nothing was laid out for them");
    assert_eq!(d.spin_repeats(), 5);
    // The landing: a real paint, nothing gliding, the scroll reported.
    // Well past the glide's end -- the notch stamped the driver's clock
    // with the wall's, which a busy machine may have moved on since t0.
    d.tick(t0 + Duration::from_millis(500));
    let landed = d.paint(400, 300);
    assert!(landed.scrollers.is_empty());
    assert!(!d.animating());
    assert_eq!(d.relayouts(), laid + 1, "and no layout for the landing either: it was done at the start");
    let out = d.take_pending();
    assert!(matches!(&out[..], [Frame::Event(e)] if e.event == EventKind::Scroll && e.payload == Value::List(vec![Value::Int(0), Value::Int(100)])), "{out:?}");
}

/// Mid-glide the pointer is over what is drawn, not over what the layout
/// put where: the row on screen under the pointer is the hovered one.
#[test]
fn a_hit_during_a_glide_finds_the_moved_row() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: 2, style: 11, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 10 });
    for i in 0..10 {
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 10 + i, style: 0, key: 0, text: Some(TextRef::Inline(format!("row {i}"))), props: (0, 0), handlers: (0, 0), child_count: 0 });
    }
    let ops = vec![
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), ..Default::default() } },
        Op::Mount(tree),
    ];
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops }));
    let t0 = Instant::now();
    d.tick(t0);
    let _ = d.paint(400, 300);
    d.input(Input::PointerMove(50.0, 10.0));
    let _ = d.paint(400, 300);
    assert_eq!(d.hovered().map(|ix| d.session().node(ix).unwrap().id), Some(10), "row 0 at rest");
    // A page down from rest rather than a wheel notch: it glides over
    // motion.slow, 320 ms, and a hover settles on the input that carries
    // it -- against the driver's clock, which an input takes from the
    // wall. So the wall has to be part way through the glide when the
    // pointer moves, and a hundred milliseconds is a window a loaded
    // machine can miss.
    press(&mut d, "PageDown");
    let _ = d.paint(400, 300);
    assert!(d.animating(), "the page down glides");
    std::thread::sleep(Duration::from_millis(120));
    d.input(Input::PointerMove(50.0, 11.0));
    // The delta the layout holds now is the one that hit test just used:
    // the hover refreshed it from the clock before asking.
    let scroll = d.session().lookup(2).unwrap();
    let dy = d.layout().glide(scroll).expect("still gliding").delta.1;
    let shown = 100.0 - dy; // the offset on screen
    assert!(shown > 11.0 && shown < 100.0, "part way, and past the first row: {shown}");
    // Which is neither where it started (row 0 under the pointer) nor
    // where it lands (row 5): the hit follows what is drawn.
    let expected = 10 + ((11.0 + shown) / 22.0).floor() as u32;
    assert_eq!(d.hovered().map(|ix| d.session().node(ix).unwrap().id), Some(expected), "the row drawn under the pointer, {shown} px in");
}

/// The client lays a slider's three parts out itself while one is being
/// dragged, so the thumb does not wait for the server's next tree. Three
/// children under a `pointer_move` handler is not enough to know a slider
/// from anything else, though: a split pane is a panel, a divider and a
/// panel under exactly such a handler, and laid out as a track, a thumb
/// and the rest it lands somewhere it never asked to be. The node says
/// what it is.
#[test]
fn only_a_node_that_says_it_is_a_slider_is_laid_out_as_one() {
    const ROLE: u32 = 40;
    let three = |d: &mut Driver, role: Option<&str>| {
        let mut tree = Subtree::default();
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
        let props = if role.is_some() { (0, 1) } else { (0, 0) };
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 2, style: 11, key: 0, text: None, props, handlers: (0, 1), child_count: 3 });
        tree.handlers.push((EventKind::PointerMove, Handler::Server(ATOM_INC)));
        if let Some(r) = role {
            tree.props.push((ROLE, Value::Str(r.into())));
        }
        for id in 3..6u32 {
            tree.nodes.push(FlatNode { kind: NodeKind::Box, id, style: 12, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
        }
        let ops = vec![
            Op::DefAtom { id: ROLE, value: "role".into() },
            Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
            Op::DefStyle { id: 11, record: StyleRecord { display: Display::Row, width: Dim::Px(240), height: Dim::Px(24), ..Default::default() } },
            Op::DefStyle { id: 12, record: StyleRecord { width: Dim::Px(80), height: Dim::Px(24), ..Default::default() } },
            Op::Mount(tree),
        ];
        d.handle_frame(Frame::Batch(Batch { seq: 2, ops }));
        let _ = d.paint(400, 300);
    };
    // The middle child's box before and after a drag that stays inside it.
    let middle_after_a_drag = |role: Option<&str>| -> (eui_layout::Rect, eui_layout::Rect) {
        let mut d = welcomed();
        three(&mut d, role);
        let mid = d.session().lookup(4).unwrap();
        let before = d.layout().rect(mid).expect("laid out");
        d.input(Input::PointerMove(before.x + 4.0, before.y + 4.0));
        d.input(Input::PointerDown(0));
        d.input(Input::PointerMove(before.x + 60.0, before.y + 4.0));
        let _ = d.paint(400, 300);
        (before, d.layout().rect(mid).expect("still laid out"))
    };
    let (before, after) = middle_after_a_drag(Some("slider"));
    assert!((after.x - before.x).abs() > 8.0, "a slider's thumb follows the hand at once: {before:?} -> {after:?}");
    let (before, after) = middle_after_a_drag(None);
    assert_eq!(after, before, "anything else keeps the box the layout gave it");
    let (before, after) = middle_after_a_drag(Some("separator"));
    assert_eq!(after, before, "including a node that says it is something else");
}

/// A press on a `pointer_move` handler captures the pointer, and the move
/// it coalesces is sent at the next paint -- not one per OS sample, and
/// not one saved up for the release. Held to the release, anything whose
/// shape only the server knows cannot follow the hand: a slider hides
/// that by moving its own thumb locally, a split pane has nothing to hide
/// it with and sits where it started.
///
/// One is in flight at a time. The gallery re-renders nine hundred nodes
/// for each move, which takes longer than a frame on a busy machine, and
/// a move a frame regardless of that is a queue -- felt as letting go and
/// watching the thing carry on.
#[test]
fn a_drag_keeps_one_move_in_flight_rather_than_a_queue() {
    let mut d = welcomed();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    // Both, as a split declares them: the move to follow the hand and the
    // release to end the drag.
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 2, style: 11, key: 0, text: None, props: (0, 0), handlers: (0, 2), child_count: 0 });
    tree.handlers.push((EventKind::PointerMove, Handler::Server(ATOM_INC)));
    tree.handlers.push((EventKind::PointerUp, Handler::Server(ATOM_INC)));
    let ops = vec![
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { width: Dim::Px(200), height: Dim::Px(100), ..Default::default() } },
        Op::Mount(tree),
    ];
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops }));
    let _ = d.paint(400, 300);
    let moves = |fs: &[Frame]| fs.iter().filter(|f| matches!(f, Frame::Event(e) if e.event == EventKind::PointerMove)).count();

    d.input(Input::PointerMove(50.0, 50.0));
    let _ = d.paint(400, 300);
    let _ = d.take_pending();
    d.input(Input::PointerDown(0));
    let _ = d.take_pending();

    // Three OS samples inside one frame: the drag coalesces them and the
    // input itself sends nothing.
    for x in [60.0, 70.0, 80.0] {
        assert_eq!(moves(&d.input(Input::PointerMove(x, 50.0))), 0, "the input holds it for the frame");
    }
    let _ = d.paint(400, 300);
    let sent = d.take_pending();
    assert_eq!(moves(&sent), 1, "one move a frame, at the last position: {sent:?}");
    let Some(Frame::Event(e)) = sent.iter().find(|f| matches!(f, Frame::Event(e) if e.event == EventKind::PointerMove)) else { panic!("{sent:?}") };
    let Value::List(at) = &e.payload else { panic!("{:?}", e.payload) };
    assert_eq!(at.first(), Some(&Value::Float(80.0)), "the position it reached, not the one it left");

    // One at a time, though: until that move is answered the next waits,
    // however many frames pass. A drag that outruns the server otherwise
    // builds a queue, and the queue is what a hand feels when it stops and
    // the thing it was dragging goes on moving.
    d.input(Input::PointerMove(120.0, 50.0));
    let _ = d.paint(400, 300);
    assert_eq!(moves(&d.take_pending()), 0, "the last one is still in flight");
    // The server answers; the newest position goes next.
    d.handle_frame(Frame::Batch(Batch { seq: 3, ops: vec![] }));
    d.input(Input::PointerMove(130.0, 50.0));
    let _ = d.paint(400, 300);
    let sent = d.take_pending();
    assert_eq!(moves(&sent), 1, "answered, so the next goes: {sent:?}");
    let Some(Frame::Event(e)) = sent.iter().find(|f| matches!(f, Frame::Event(e) if e.event == EventKind::PointerMove)) else { panic!("{sent:?}") };
    let Value::List(at) = &e.payload else { panic!() };
    assert_eq!(at.first(), Some(&Value::Float(130.0)), "the latest position, not the one that waited");

    // The release still carries the last move and the up.
    d.input(Input::PointerMove(150.0, 50.0));
    let out = d.input(Input::PointerUp(0));
    assert_eq!(moves(&out), 1, "the move the frame did not get to");
    assert!(out.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::PointerUp)), "{out:?}");
}

#[test]
fn trackpad_fractions_add_up_instead_of_being_swallowed() {
    let mut d = welcomed();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: 2, style: 11, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 10 });
    for i in 0..10 {
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 10 + i, style: 0, key: 0, text: Some(TextRef::Inline(format!("row {i}"))), props: (0, 0), handlers: (0, 0), child_count: 0 });
    }
    let ops = vec![
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), ..Default::default() } },
        Op::Mount(tree),
    ];
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops }));
    let _ = d.paint(400, 300);
    let scroll = d.session().lookup(2).unwrap();
    d.input(Input::PointerMove(50.0, 50.0));
    for _ in 0..10 {
        d.input(Input::Wheel(0.0, 0.7));
    }
    assert_eq!(d.session().node(scroll).unwrap().scroll, (0, 7), "ten events of 0.7 px scroll 7 px");
}

#[test]
fn the_scrollbar_thumb_drags_and_its_track_pages() {
    let mut d = welcomed();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: 2, style: 11, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 20 });
    for i in 0..20 {
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 10 + i, style: 0, key: 0, text: Some(TextRef::Inline(format!("row {i}"))), props: (0, 0), handlers: (0, 0), child_count: 0 });
    }
    let ops = vec![
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 2, ops })), vec![Frame::Ack { seq: 2 }]);
    let list = d.paint(400, 300);
    let scroll = d.session().lookup(2).unwrap();
    let r = d.layout().rect(scroll).unwrap();
    // The thumb is painted at the right edge, 100/440 of the track, min 24 px.
    let thumb = eui_render::scrollbar_thumb(d.session(), d.layout(), scroll, r).expect("content overflows");
    assert!(thumb.x >= r.x + r.w - eui_render::SCROLLBAR_WIDTH);
    assert_eq!(thumb.h, 24.0);
    // At rest the page wears no bar. 03 §2's thumb is feedback about a
    // movement and nothing here has moved, so it is not painted until
    // something scrolls — or until the pointer is on the strip, where it
    // is there to be grabbed.
    let drawn = |list: &eui_render::DrawList, thumb: &eui_layout::Rect| list.quads.iter().any(|q| q.rect[3] == thumb.h && q.rect[0] >= r.x + r.w - eui_render::SCROLLBAR_WIDTH - 2.0);
    assert!(!drawn(&list, &thumb), "no bar until something moves");
    d.input(Input::PointerMove(thumb.x + 2.0, thumb.y + 5.0));
    let list = d.paint(400, 300);
    assert!(drawn(&list, &thumb), "the thumb is drawn under the pointer");
    // A press on the track below the thumb pages down by the view height.
    d.input(Input::PointerMove(thumb.x + 2.0, r.y + r.h - 5.0));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    assert_eq!(d.session().node(scroll).unwrap().scroll, (0, 100));
    // Grab the thumb and drag it to the bottom of the track: the end.
    let _ = d.paint(400, 300);
    let thumb = eui_render::scrollbar_thumb(d.session(), d.layout(), scroll, r).unwrap();
    d.input(Input::PointerMove(thumb.x + 2.0, thumb.y + 5.0));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerMove(thumb.x + 2.0, r.y + r.h + 50.0));
    d.input(Input::PointerUp(0));
    assert_eq!(d.session().node(scroll).unwrap().scroll, (0, 340), "content 440 − view 100");
    // Dragging back to the top.
    let _ = d.paint(400, 300);
    let thumb = eui_render::scrollbar_thumb(d.session(), d.layout(), scroll, r).unwrap();
    d.input(Input::PointerMove(thumb.x + 2.0, thumb.y + 5.0));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerMove(thumb.x + 2.0, r.y - 50.0));
    d.input(Input::PointerUp(0));
    assert_eq!(d.session().node(scroll).unwrap().scroll, (0, 0));
}

#[test]
fn pointer_moves_while_a_frame_is_owed_do_not_lay_out_and_hover_settles_at_paint() {
    let mut d = welcomed();
    let _ = d.paint(400, 300);
    let (x, y) = centre(&mut d, 4);
    // Hovering the button on a valid layout: enter reaches the local handler
    // path at once (no handler here: no frames, but `hovered()` moves).
    d.input(Input::PointerMove(x, y));
    assert_eq!(d.hovered(), d.session().lookup(4));
    // Invalidate the layout the way a scroll does, then move: no layout, no
    // hover change yet.
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 2, text: TextRef::Inline("7".into()) }] }));
    let measures_before = d.layout().stats().measures;
    d.input(Input::PointerMove(5.0, 5.0));
    assert_eq!(d.layout().stats().measures, measures_before, "a move did not lay out");
    assert_eq!(d.hovered(), d.session().lookup(4), "hover waits for the frame");
    // The paint lays out and settles hover on the root.
    let _ = d.paint(400, 300);
    assert_eq!(d.hovered(), d.session().lookup(1));
}

#[test]
fn a_refused_resync_ends_the_session_instead_of_looping() {
    let mut d = welcomed();
    // A bad batch: resync. The fresh tree is bad too: an Error, and closed.
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 99, text: TextRef::Atom(1) }] })), vec![Frame::Resync]);
    let out = d.handle_frame(Frame::Batch(Batch { seq: 3, ops: vec![Op::SetText { node: 99, text: TextRef::Atom(1) }] }));
    assert!(matches!(&out[..], [Frame::Error { code: 102, .. }]), "{out:?}");
    assert!(d.closed().is_some());
    // Whereas a good fresh tree after one refusal is simply accepted.
    let mut d = welcomed();
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::SetText { node: 99, text: TextRef::Atom(1) }] }));
    let mut fresh = counter_batch();
    fresh.seq = 3;
    fresh.ops.retain(|op| !matches!(op, Op::DefAtom { .. } | Op::DefStyle { .. }));
    assert_eq!(d.handle_frame(Frame::Batch(fresh)), vec![Frame::Ack { seq: 3 }]);
    // And a later bad batch resyncs again: the guard is per resync, not per session.
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 4, ops: vec![Op::SetText { node: 99, text: TextRef::Atom(1) }] })), vec![Frame::Resync]);
}

#[test]
fn a_local_then_server_chunks_effects_are_provisional_until_the_answer() {
    use eui_vm::Asm;
    const COUNT: u32 = 1;
    const INC: u32 = 2;
    const VALUE_KEY: u32 = 3;
    // The counter's local handler: count += 1, value.text = str(count).
    let chunk = Asm::new(2).load(COUNT).push_int(1).op(0x10).op(0x06).store(COUNT).op(0x1A).set_text(VALUE_KEY).ret();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 2 });
    tree.props.push((COUNT, Value::Int(41)));
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 0, key: VALUE_KEY, text: Some(TextRef::Inline("41".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 2, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 0 });
    tree.handlers.push((EventKind::Click, Handler::LocalThenServer { chunk: 1, name: INC }));
    let batch = Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: COUNT, value: "count".into() },
            Op::DefAtom { id: INC, value: "increment".into() },
            Op::DefAtom { id: VALUE_KEY, value: "value".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, ..Default::default() } },
            Op::DefStyle { id: 2, record: StyleRecord { width: Dim::Px(40), height: Dim::Px(20), ..Default::default() } },
            Op::DefChunkBytes { id: 1, bytes: chunk },
            Op::Mount(tree),
        ],
    };
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    assert_eq!(d.handle_frame(Frame::Batch(batch)), vec![Frame::Ack { seq: 1 }]);
    let (x, y) = centre(&mut d, 3);
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    let value = d.session().lookup(2).unwrap();
    assert_eq!(d.session().text_of(value), Some("42"), "shown at once");
    // A server batch that says nothing about the value: the provisional
    // change is undone — the client agrees with the server.
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::SetStyle { node: 3, style: 2 }] }));
    assert_eq!(d.session().text_of(value), Some("41"), "reverted");
    assert_eq!(d.session().root_prop(COUNT), Some(&Value::Int(41)));
    // A batch that confirms it: the old value goes back first, the op lands
    // on top, no flicker between.
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    assert_eq!(d.session().text_of(value), Some("42"));
    d.handle_frame(Frame::Batch(Batch { seq: 3, ops: vec![Op::SetText { node: 2, text: TextRef::Inline("42".into()) }, Op::SetProp { node: 1, prop: COUNT, value: Value::Int(42) }] }));
    assert_eq!(d.session().text_of(value), Some("42"));
    assert_eq!(d.session().root_prop(COUNT), Some(&Value::Int(42)));
}

#[test]
fn a_spinning_node_marks_its_quads_and_keeps_frames_coming() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    let spin = StyleRecord { width: Dim::Px(20), height: Dim::Px(20), bg: ColorRef::role(Role::AccentBase.id()), animation: 1, ..Default::default() };
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 2, style: 11, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
    let ops = vec![Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } }, Op::DefStyle { id: 11, record: spin }, Op::Mount(tree)];
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops }));
    let t0 = Instant::now();
    d.tick(t0 + Duration::from_millis(300));
    let list = d.paint(400, 300);
    assert!(list.wants_frame);
    assert!(d.next_frame_at().is_some(), "frames keep coming while it spins");
    // Nothing else is owed, so this frame may simply be drawn again.
    assert!(list.gpu_only, "a spin on its own is the window's to repeat");
    assert_eq!(list.repeat_until_ms, u32::MAX, "for as long as nothing reaches the driver");
    assert_ne!(list.serial, 0, "and the renderer can tell it is the same list");
    let q = list.quads.iter().find(|q| q.rect[2] == 20.0).expect("the spinning box");
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "params[2] is a small flag bitfield carried as a float")]
    let flags = q.params[2] as u32;
    assert!(flags & eui_render::SPINNING != 0, "marked for the vertex stage: {q:?}");
    assert_eq!(q.extra[0], 0.0, "the angle is the shader's, not the list's");
    // Its centre stayed put: the box turns about itself, so it is its own
    // spin centre and the offset the shader turns is zero.
    let r = d.layout().rect(d.session().lookup(2).unwrap()).unwrap();
    assert!((q.rect[0] + q.rect[2] / 2.0 - (r.x + r.w / 2.0)).abs() < 0.01);
    assert!(q.spin[0].abs() < 0.01 && q.spin[1].abs() < 0.01, "offset from its own centre is nothing: {:?}", q.spin);
    // The whole point: the clock does not reach the list, so the window can
    // draw this one again instead of asking for another.
    d.tick(t0 + Duration::from_millis(900));
    assert_eq!(d.paint(400, 300), list, "half a revolution later, the same list");
    // And the driver did not walk the tree to say so: a frame owed to a
    // spin alone, with nothing having reached the driver since, is the
    // last list handed back. Thirty of them a second is the cadence.
    assert_eq!(d.spin_repeats(), 1, "answered from the last list, not painted");
    let due = d.next_frame_at().expect("the next spin frame");
    let gap = due.saturating_duration_since(t0 + Duration::from_millis(900));
    assert!(gap >= Duration::from_millis(30) && gap <= Duration::from_millis(40), "thirty a second: {gap:?}");
    // Anything that reaches the driver ends the repeat, because it may
    // change what the tree paints: a pointer over the box lights nothing
    // here, but the driver cannot know that without looking.
    d.input(Input::PointerMove(10.0, 10.0));
    d.tick(t0 + Duration::from_millis(940));
    let _ = d.paint(400, 300);
    assert_eq!(d.spin_repeats(), 1, "a real paint after an input");
    d.tick(t0 + Duration::from_millis(980));
    let _ = d.paint(400, 300);
    assert_eq!(d.spin_repeats(), 2, "and the repeat resumes once nothing has happened");
}

/// A list with nothing moving in it is the frame until something reaches
/// the driver: an expose, a chrome repainted beside an animating
/// application, a window asked to draw for any reason of its own gets the
/// last list back and no tree walk.
#[test]
fn a_list_at_rest_is_the_same_list_until_something_reaches_the_driver() {
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    let t0 = Instant::now();
    d.tick(t0);
    let first = d.paint(400, 300);
    assert!(!first.wants_frame && first.gpu_only, "nothing moves, nothing is owed");
    assert_eq!(first.repeat_until_ms, u32::MAX, "so it holds until told otherwise");
    assert_eq!(d.next_frame_at(), None, "and no frame is due");
    let laid = d.relayouts();
    assert!(!d.tick(t0 + Duration::from_secs(5)));
    let again = d.paint(400, 300);
    assert!(Arc::ptr_eq(&first, &again), "the very same list, not a copy of it");
    assert_eq!(d.spin_repeats(), 1, "answered, not painted");
    assert_eq!(d.relayouts(), laid, "and not laid out either");
    assert_eq!(d.next_frame_at(), None, "still nothing due: a rest list has no cadence");
    // An input may change what the tree paints, so the next paint is real
    // -- and a new list, whatever it looks like.
    d.input(Input::PointerMove(1.0, 1.0));
    let after = d.paint(400, 300);
    assert!(!Arc::ptr_eq(&first, &after));
    assert_ne!(after.serial, first.serial, "the renderer is told it is another list");
    assert_eq!(d.spin_repeats(), 1);
}

/// A 100 px list of ten rows of two heights (22 and 40, alternating) with
/// `item_height` 22, so row tops are not multiples of anything simple.
fn list_of_rows(d: &mut Driver) -> eui_tree::NodeIx {
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::List, id: 2, style: 11, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 10 });
    tree.props.push((ATOM_ITEM_H, Value::Int(22)));
    for i in 0..10u32 {
        let tall = i % 2 == 1;
        tree.nodes.push(FlatNode {
            kind: NodeKind::Text,
            id: 10 + i,
            style: 0,
            key: 0,
            text: Some(TextRef::Inline(format!("row {i}"))),
            props: (tall as u32 * (tree.props.len() as u32), tall as u32),
            handlers: (0, 0),
            child_count: 0,
        });
        if tall {
            tree.props.push((ATOM_ITEM_H, Value::Int(40)));
        }
    }
    let ops = vec![
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 2, ops })), vec![Frame::Ack { seq: 2 }]);
    let _ = d.paint(400, 300);
    d.session().lookup(2).unwrap()
}

fn press(d: &mut Driver, k: &str) -> Vec<Frame> {
    let out = d.input(Input::Key { key: k.into(), modifiers: 0, down: true });
    d.input(Input::Key { key: k.into(), modifiers: 0, down: false });
    out
}

/// Let a scroll in flight land: the clock moves two seconds each call.
fn settle(d: &mut Driver, clock: &mut std::time::Instant) {
    *clock += std::time::Duration::from_secs(2);
    d.tick(*clock);
    let _ = d.paint(400, 300);
}

/// A page whose content is one tall column has a single "row", at the very
/// top, and honouring it made `ArrowUp` a `Home` key: the gallery jumped to
/// the top from wherever it was. A row further than a viewport away is not
/// the next row, it is a different part of the page.
#[test]
fn an_arrow_on_a_page_of_one_column_steps_instead_of_jumping_home() {
    use std::time::Instant;
    let mut d = welcomed();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: 2, style: 11, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 12, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
    let ops = vec![
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), overflow: Overflow::Scroll, ..Default::default() } },
        Op::DefStyle { id: 12, record: StyleRecord { display: Display::Column, height: Dim::Px(1000), ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 2, ops })), vec![Frame::Ack { seq: 2 }]);
    let t0 = Instant::now();
    let mut clock = t0;
    d.tick(t0);
    let _ = d.paint(400, 300);
    let page = d.session().lookup(2).unwrap();
    // Down a few steps, the way a reader would arrive part-way down.
    for _ in 0..3 {
        press(&mut d, "ArrowDown");
        settle(&mut d, &mut clock);
    }
    assert_eq!(d.session().node(page).unwrap().scroll, (0, 120), "three 40 px steps");
    // Up is one step back, not the whole way: the column's top is 120 px
    // away here, but it is the only row there is.
    press(&mut d, "ArrowUp");
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(page).unwrap().scroll, (0, 80), "one step up, not Home");
    // Home still goes home.
    press(&mut d, "Home");
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(page).unwrap().scroll, (0, 0));
}

#[test]
fn arrows_land_on_rows_and_page_keys_move_a_viewport() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    let list = list_of_rows(&mut d);
    let t0 = Instant::now();
    let mut clock = t0;
    d.tick(t0);
    let _ = d.paint(400, 300);
    // Row tops: 0, 22, 62, 84, 124, 146, 186, 208, 248, 270; content 310.
    assert_eq!(d.layout().row_tops(list).map(|t| t[..4].to_vec()), Some(vec![0.0, 22.0, 62.0, 84.0]));
    // Nothing focused, pointer anywhere: ArrowDown eases to the next row,
    // in and out over motion.slow, and nothing is reported until it lands.
    assert!(press(&mut d, "ArrowDown").is_empty());
    assert!(d.animating());
    d.tick(t0 + Duration::from_millis(100));
    let _ = d.paint(400, 300);
    // The tree holds the landing; the content is drawn on its way there.
    assert_eq!(d.session().node(list).unwrap().scroll, (0, 22), "laid out at the landing");
    let mid = 22.0 - d.layout().glide(list).expect("gliding").delta.1;
    assert!(mid > 0.0 && mid < 22.0, "mid-way at {mid}");
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(list).unwrap().scroll, (0, 22));
    assert!(!d.animating());
    // Presses chain: two more land on row 3 (top 84), not 22 + 2 steps of anything.
    press(&mut d, "ArrowDown");
    press(&mut d, "ArrowDown");
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(list).unwrap().scroll, (0, 84));
    // ArrowUp: the previous row.
    press(&mut d, "ArrowUp");
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(list).unwrap().scroll, (0, 62));
    // PageDown: one viewport (100 px), clamped at the end (310 - 100).
    press(&mut d, "PageDown");
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(list).unwrap().scroll, (0, 162));
    press(&mut d, "PageDown");
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(list).unwrap().scroll, (0, 210));
    press(&mut d, "Home");
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(list).unwrap().scroll, (0, 0));
    press(&mut d, "End");
    settle(&mut d, &mut clock);
    assert_eq!(d.session().node(list).unwrap().scroll, (0, 210));
    // Only the landings were reported, one scroll event each.
    let landed = d.take_pending();
    assert!(landed.iter().all(|f| matches!(f, Frame::Event(e) if e.event == EventKind::Scroll)), "{landed:?}");
    // A modifier makes it someone else's key.
    let out = d.input(Input::Key { key: "ArrowDown".into(), modifiers: 2, down: true });
    assert!(out.is_empty() && !d.animating());
}

#[test]
fn the_cursor_follows_what_the_pointer_is_over() {
    let mut d = welcomed();
    assert_eq!(d.cursor(), Cursor::Default);
    // Over the button's text: the click handler above it means a hand.
    let (x, y) = centre(&mut d, 4);
    d.input(Input::PointerMove(x, y));
    let _ = d.paint(400, 300);
    assert_eq!(d.cursor(), Cursor::Pointer);
    // Over the value text: nothing clickable up the tree, the arrow.
    let (x, y) = centre(&mut d, 2);
    d.input(Input::PointerMove(x, y));
    let _ = d.paint(400, 300);
    assert_eq!(d.cursor(), Cursor::Default);
    // A `cursor` style wins over the handler's hand.
    d.handle_frame(Frame::Batch(Batch {
        seq: 2,
        ops: vec![Op::DefStyle { id: 3, record: StyleRecord { cursor: Cursor::Grab, padding: [3; 4], ..Default::default() } }, Op::SetStyle { node: 3, style: 3 }],
    }));
    let (x, y) = centre(&mut d, 4);
    d.input(Input::PointerMove(x, y));
    let _ = d.paint(400, 300);
    assert_eq!(d.cursor(), Cursor::Grab);
    // An editable node is a beam.
    let mut f = Driver::new(400.0, 300.0, 1.0, 0);
    f.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    f.handle_frame(Frame::Batch(form_batch()));
    let (x, y) = centre(&mut f, 2);
    f.input(Input::PointerMove(x, y));
    let _ = f.paint(400, 300);
    assert_eq!(f.cursor(), Cursor::Text);
}

#[test]
fn a_local_handler_can_switch_the_viewers_palette() {
    use eui_vm::Asm;
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    const TOGGLE: u32 = 1;
    let chunk = Asm::new(1).push_str(TOGGLE).set_mode().ret();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 2, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 1 });
    tree.handlers.push((EventKind::Click, Handler::Local(1)));
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 4, style: 0, key: 0, text: Some(TextRef::Inline("☀/☾".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    let batch = Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: TOGGLE, value: "toggle".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, padding: [4; 4], ..Default::default() } },
            Op::DefStyle { id: 2, record: StyleRecord { padding: [3; 4], bg: ColorRef::role(Role::SurfaceSunken.id()), ..Default::default() } },
            Op::DefChunkBytes { id: 1, bytes: chunk },
            Op::Mount(tree),
        ],
    };
    assert_eq!(d.handle_frame(Frame::Batch(batch)), vec![Frame::Ack { seq: 1 }]);
    let light = d.theme_color(Role::SurfaceBase);
    let (x, y) = centre(&mut d, 4);
    d.input(Input::PointerMove(x, y));
    d.input(Input::PointerDown(0));
    assert!(d.input(Input::PointerUp(0)).is_empty(), "a local handler: no event");
    // The palette is dark now, and the server learns the viewport at the
    // next paint's pending frames — not as a provisional change.
    assert_ne!(d.theme_color(Role::SurfaceBase), light);
    let _ = d.paint(400, 300);
    let pending = d.take_pending();
    assert!(matches!(pending.as_slice(), [Frame::Viewport(v)] if v.mode == ThemeMode::Dark), "{pending:?}");
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![] }));
    assert_ne!(d.theme_color(Role::SurfaceBase), light, "a batch does not undo the viewer's choice");
    // Toggle back.
    d.input(Input::PointerDown(0));
    d.input(Input::PointerUp(0));
    assert_eq!(d.theme_color(Role::SurfaceBase), light);
}

#[test]
fn the_desktops_palette_overrides_roles_in_its_own_mode() {
    let mut d = welcomed();
    let theme_default = d.theme_color(Role::AccentBase);
    let out = d.set_desktop_theme(Some(ThemeMode::Dark), vec![(Role::AccentBase, 0xf7a96aff), (Role::SurfaceBase, 0x101a26ff)]);
    assert!(matches!(out.as_slice(), [Frame::Viewport(v)] if v.mode == ThemeMode::Dark));
    assert_eq!(d.theme_color(Role::AccentBase), 0xf7a96aff);
    assert_eq!(d.theme_color(Role::SurfaceBase), 0x101a26ff);
    // The button paints in the desktop's accent.
    let list = d.paint(400, 300);
    let accent = eui_render::linear(0xf7a96aff);
    assert!(list.quads.iter().any(|q| (q.fill[0] - accent[0]).abs() < 1e-3 && (q.fill[1] - accent[1]).abs() < 1e-3), "painted in the desktop accent");
    // The same palette again is a no-op. A switch to the other mode shows
    // the theme's own colours for it — the desktop published none — and
    // a switch back follows the desktop again.
    assert!(d.set_desktop_theme(Some(ThemeMode::Dark), vec![(Role::AccentBase, 0xf7a96aff), (Role::SurfaceBase, 0x101a26ff)]).is_empty());
    d.input(Input::Mode(ThemeMode::Light));
    assert_eq!(d.theme_color(Role::AccentBase), theme_default);
    d.input(Input::Mode(ThemeMode::Dark));
    assert_eq!(d.theme_color(Role::AccentBase), 0xf7a96aff);
    // None: the theme's own colours return, in whatever mode the viewer is.
    d.set_desktop_theme(None, Vec::new());
    d.input(Input::Mode(ThemeMode::Light));
    assert_eq!(d.theme_color(Role::AccentBase), theme_default);
}

/// Spec 04 §7.1: a windowed list asks for the rows in view, once per
/// range, once the view has landed.
#[test]
fn a_windowed_list_asks_for_its_rows_when_the_view_lands() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    const ATOM_COUNT: u32 = 20;
    const ATOM_ROW: u32 = 21;
    const ATOM_WINDOW: u32 = 22;
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::List, id: 2, style: 11, key: 0, text: None, props: (0, 2), handlers: (0, 1), child_count: 1 });
    tree.props.push((ATOM_ITEM_H, Value::Int(20)));
    tree.props.push((ATOM_COUNT, Value::Int(100)));
    tree.handlers.push((EventKind::Window, Handler::Server(ATOM_WINDOW)));
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 10, style: 0, key: 0, text: Some(TextRef::Inline("row 0".into())), props: (2, 1), handlers: (0, 0), child_count: 0 });
    tree.props.push((ATOM_ROW, Value::Int(0)));
    let ops = vec![
        Op::DefAtom { id: ATOM_COUNT, value: "count".into() },
        Op::DefAtom { id: ATOM_ROW, value: "row".into() },
        Op::DefAtom { id: ATOM_WINDOW, value: "window".into() },
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 2, ops })), vec![Frame::Ack { seq: 2 }]);
    let t0 = Instant::now();
    d.tick(t0);
    // The first paint asks for the rows within two viewports of margin: 0..=14.
    let _ = d.paint(400, 300);
    let asked = d.take_pending();
    assert_eq!(asked.len(), 1, "{asked:?}");
    let Frame::Event(e) = &asked[0] else { panic!() };
    assert_eq!((e.node, e.event, e.name), (2, EventKind::Window, ATOM_WINDOW));
    assert_eq!(e.payload, Value::List(vec![Value::Int(0), Value::Int(14)]));
    // The same range again: nothing.
    let _ = d.paint(400, 300);
    assert!(d.take_pending().is_empty());
    // The extent is the whole list, so it scrolls far: 1 000 px down lands
    // on rows around 40..=64.
    let list = d.session().lookup(2).unwrap();
    d.input(Input::PointerMove(50.0, 50.0));
    d.input(Input::Wheel(0.0, 1000.0));
    assert_eq!(d.session().node(list).unwrap().scroll, (0, 1000));
    // The view has outrun the rows it holds — it is at rows 50..=55 with
    // 0..=14 in hand — so it asks at once rather than showing placeholders
    // until it stops: the full window around where it is, 40..=64.
    let t1 = Instant::now();
    d.tick(t1);
    let _ = d.paint(400, 300);
    let windows = |frames: &[Frame]| -> Vec<Value> { frames.iter().filter_map(|f| if let Frame::Event(e) = f { (e.event == EventKind::Window).then_some(e.payload.clone()) } else { None }).collect() };
    assert_eq!(windows(&d.take_pending()), vec![Value::List(vec![Value::Int(40), Value::Int(64)])], "asked mid-scroll, once it outran what it held");
    // Not again the next frame: what it holds now covers what it sees, and
    // asking is throttled besides.
    d.tick(t1 + Duration::from_millis(16));
    let _ = d.paint(400, 300);
    assert!(windows(&d.take_pending()).is_empty(), "nothing asked while covered");
    assert!(d.next_frame_at().is_some(), "but a frame is due to settle");
    // Once still for a moment there is nothing new to ask: the settle finds
    // the window it already asked for.
    std::thread::sleep(Duration::from_millis(130));
    d.tick(Instant::now());
    let _ = d.paint(400, 300);
    assert!(windows(&d.take_pending()).is_empty(), "the settle repeats nothing");
    // A glide that stays within the rows it holds asks nothing until it
    // lands: a page down from 1 000 reaches 1 100, rows 55..=60, all held.
    d.input(Input::Key { key: "PageDown".into(), modifiers: 0, down: true });
    d.tick(t0 + Duration::from_millis(50));
    let _ = d.paint(400, 300);
    assert!(d.take_pending().iter().all(|f| !matches!(f, Frame::Event(e) if e.event == EventKind::Window)));
    d.tick(t0 + Duration::from_secs(2));
    let _ = d.paint(400, 300);
    std::thread::sleep(Duration::from_millis(130));
    d.tick(Instant::now() + Duration::from_secs(2));
    let _ = d.paint(400, 300);
    let asked = d.take_pending();
    assert!(asked.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::Window && e.payload == Value::List(vec![Value::Int(45), Value::Int(69)]))), "{asked:?}");
}

/// A RIFF/WAVE file of 16-bit mono samples, so the test needs no fixture.
fn wav_bytes(samples: &[i16], rate: u32) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36u32 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// Spec 03 §7: an `audio` node names a sound, says what it should be
/// doing, and hears back when it ends.
#[test]
fn an_audio_node_asks_for_its_sound_plays_it_and_reports_its_end() {
    let mut d = welcomed();
    const A_SRC: u32 = 30;
    const A_PLAYING: u32 = 31;
    const A_VOLUME: u32 = 32;
    const A_POSITION: u32 = 33;
    const A_ENDED: u32 = 34;
    let hash: [u8; 32] = [7; 32];
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Audio, id: 2, style: 0, key: 0, text: None, props: (0, 3), handlers: (0, 1), child_count: 0 });
    tree.props.push((A_SRC, Value::Asset(hash)));
    tree.props.push((A_PLAYING, Value::Bool(false)));
    tree.props.push((A_VOLUME, Value::Int(100)));
    tree.handlers.push((EventKind::Ended, Handler::Server(A_ENDED)));
    let ops = vec![
        Op::DefAtom { id: A_SRC, value: "src".into() },
        Op::DefAtom { id: A_PLAYING, value: "playing".into() },
        Op::DefAtom { id: A_VOLUME, value: "volume".into() },
        Op::DefAtom { id: A_POSITION, value: "position".into() },
        Op::DefAtom { id: A_ENDED, value: "ended".into() },
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 2, ops })), vec![Frame::Ack { seq: 2 }]);
    // The sound is an asset like any other: the client asks for it.
    assert!(d.pending_assets().contains(&hash), "the hash is wanted");
    // Until it arrives, and while `playing` is false, the mix is silence.
    let mut out = [1.0f32; 64];
    assert!(d.fill_audio(&mut out, 1, 8_000).is_empty());
    assert!(out.iter().all(|s| *s == 0.0), "silence, and the buffer is overwritten");
    // A tone of a hundredth of a second at 8 kHz: eighty frames.
    d.asset_ready(hash, wav_bytes(&[8_000i16; 80], 8_000));
    let _ = d.paint(400, 300);
    assert!(d.audio_playing(), "the sound is loaded");
    // Still not playing: the node says so.
    d.fill_audio(&mut out, 1, 8_000);
    assert!(out.iter().all(|s| *s == 0.0), "loaded is not playing");
    // The server says play: the tone comes out, at its own level.
    d.handle_frame(Frame::Batch(Batch { seq: 3, ops: vec![Op::SetProp { node: 2, prop: A_PLAYING, value: Value::Bool(true) }] }));
    let _ = d.paint(400, 300);
    assert!(d.fill_audio(&mut out, 1, 8_000).is_empty());
    assert!(out.iter().all(|s| (*s - 0.244).abs() < 0.01), "the tone: {:?}", &out[..4]);
    // Half volume halves it. Sixteen frames of the sound are left, so
    // this buffer is half tone, half silence — and it is the fill that
    // runs out, so the node's handler hears `ended` here.
    d.handle_frame(Frame::Batch(Batch { seq: 4, ops: vec![Op::SetProp { node: 2, prop: A_VOLUME, value: Value::Int(50) }] }));
    let _ = d.paint(400, 300);
    let ended = d.fill_audio(&mut out, 1, 8_000);
    assert!(out[..16].iter().all(|s| (*s - 0.122).abs() < 0.01), "half: {:?}", &out[..4]);
    assert!(out[16..].iter().all(|s| *s == 0.0), "silence past the end");
    assert_eq!(ended.len(), 1, "{ended:?}");
    let Frame::Event(e) = &ended[0] else { panic!("{ended:?}") };
    assert_eq!((e.node, e.event, e.name), (2, EventKind::Ended, A_ENDED));
    assert!(d.fill_audio(&mut out, 1, 8_000).is_empty(), "and only once");
    // A new `position` seeks; the same one again does not.
    d.handle_frame(Frame::Batch(Batch { seq: 5, ops: vec![Op::SetProp { node: 2, prop: A_POSITION, value: Value::Int(5) }] }));
    let _ = d.paint(400, 300);
    d.fill_audio(&mut out, 1, 8_000);
    assert!(out.iter().take(20).any(|s| *s > 0.1), "playing again from 5 ms: {:?}", &out[..4]);
    // A node pointed at another asset plays the other asset. The mixer
    // is keyed by node, so a tracker rendering a new wav into the same
    // node used to go on playing the first sound it was ever given.
    let second: [u8; 32] = [9; 32];
    d.handle_frame(Frame::Batch(Batch { seq: 6, ops: vec![Op::SetProp { node: 2, prop: A_SRC, value: Value::Asset(second) }] }));
    let _ = d.paint(400, 300);
    assert!(d.pending_assets().contains(&second), "the new hash is wanted");
    // Twice the length, a quarter of the level: nothing of the first tone
    // could pass for it.
    d.asset_ready(second, wav_bytes(&[2_000i16; 160], 8_000));
    let _ = d.paint(400, 300);
    d.handle_frame(Frame::Batch(Batch { seq: 7, ops: vec![Op::SetProp { node: 2, prop: A_VOLUME, value: Value::Int(100) }] }));
    let _ = d.paint(400, 300);
    d.fill_audio(&mut out, 1, 8_000);
    assert!(out.iter().all(|s| (*s - 0.061).abs() < 0.01), "the second sound, from its start: {:?}", &out[..4]);

    // The tree owns the sound: drop the node and the mixer forgets it.
    d.handle_frame(Frame::Batch(Batch { seq: 8, ops: vec![Op::RemoveChild { parent: 1, index: 0, count: 1 }] }));
    let _ = d.paint(400, 300);
    assert!(!d.audio_playing(), "no node, no sound");
    d.fill_audio(&mut out, 1, 8_000);
    assert!(out.iter().all(|s| *s == 0.0));
}

/// Spec 01 §4: a session that ends says so on the glass. A window that
/// stopped talking to its application must not look like one that is
/// merely idle.
#[test]
fn a_session_that_ends_replaces_the_tree_with_the_reason() {
    let mut d = welcomed();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 0, key: 0, text: Some(TextRef::Inline("the application".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    let ops = vec![Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } }, Op::Mount(tree)];
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops }));
    let _ = d.paint(400, 300);
    let texts = |d: &Driver| -> Vec<String> { d.session().root().map_or_else(Vec::new, |root| d.session().preorder(root).filter_map(|ix| d.session().text_of(ix).map(str::to_owned)).collect()) };
    assert!(texts(&d).iter().any(|t| t == "the application"), "the server's tree is what is drawn");

    // The server gives up on a view it cannot encode.
    d.handle_frame(Frame::Error { code: 400, message: "EUI: unknown event 'wake'".into() });
    let list = d.paint(400, 300);
    let after = texts(&d);
    assert!(after.iter().any(|t| t == "The application stopped"), "{after:?}");
    assert!(after.iter().any(|t| t.contains("unknown event 'wake'")), "and why: {after:?}");
    assert!(!after.iter().any(|t| t == "the application"), "the tree it stopped on is gone: {after:?}");
    assert!(!list.quads.is_empty(), "and something is painted");
    // Mounted once, not on every frame after.
    let again = d.paint(400, 300);
    assert_eq!(again.quads.len(), list.quads.len());
}

/// Spec 06 §1.1: a node that asks to be woken is, on its own period and
/// nobody's action; the floor holds, the phase survives a re-render, and
/// dropping the prop stops the clock.
#[test]
fn a_node_that_asks_to_be_woken_is_woken_on_its_own_period() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    const A_WAKE: u32 = 40;
    const A_TICK: u32 = 41;
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 1), handlers: (0, 1), child_count: 0 });
    // 10 ms asked, 100 ms given: a clock is not a render loop.
    tree.props.push((A_WAKE, Value::Int(10)));
    tree.handlers.push((EventKind::Wake, Handler::Server(A_TICK)));
    let ops = vec![
        Op::DefAtom { id: A_WAKE, value: "wake".into() },
        Op::DefAtom { id: A_TICK, value: "tick".into() },
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::Mount(tree),
    ];
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops }));
    let woken = |d: &mut Driver| d.take_pending().iter().filter(|f| matches!(f, Frame::Event(e) if e.event == EventKind::Wake)).count();

    let t0 = Instant::now();
    d.tick(t0);
    let _ = d.paint(400, 300);
    assert_eq!(woken(&mut d), 0, "nothing is due yet");
    // Before the floor: still nothing, whatever the node asked for.
    d.tick(t0 + Duration::from_millis(50));
    let _ = d.paint(400, 300);
    assert_eq!(woken(&mut d), 0, "10 ms asked, 100 ms is the floor");
    d.tick(t0 + Duration::from_millis(120));
    let _ = d.paint(400, 300);
    assert_eq!(woken(&mut d), 1, "one wake, once the period passed");
    // A frame that changed nothing else does not owe a second one.
    let _ = d.paint(400, 300);
    assert_eq!(woken(&mut d), 0, "one event a period, not one a frame");
    // A batch that leaves the prop alone leaves the clock running: the
    // next event is due a period after the one that fired, not now.
    d.handle_frame(Frame::Batch(Batch { seq: 3, ops: vec![Op::SetText { node: 1, text: TextRef::Inline("hello".into()) }] }));
    d.tick(t0 + Duration::from_millis(180));
    let _ = d.paint(400, 300);
    assert_eq!(woken(&mut d), 0, "a re-render does not restart the clock");
    d.tick(t0 + Duration::from_millis(240));
    let _ = d.paint(400, 300);
    assert_eq!(woken(&mut d), 1, "and the next one lands on time");
    // Taking the prop away stops it.
    d.handle_frame(Frame::Batch(Batch { seq: 4, ops: vec![Op::SetProp { node: 1, prop: A_WAKE, value: Value::Null }] }));
    d.tick(t0 + Duration::from_millis(400));
    let _ = d.paint(400, 300);
    assert_eq!(woken(&mut d), 0, "no prop, no clock");
    assert!(d.next_frame_at().is_none(), "and nothing is owed");
}

/// Spec 03 §7: `time_update` goes only to a node that asks for it, and at
/// most four a second.
#[test]
fn time_update_is_rate_limited_and_only_for_nodes_that_ask() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    const A_SRC: u32 = 30;
    const A_PLAYING: u32 = 31;
    const A_TIME: u32 = 35;
    let hash: [u8; 32] = [9; 32];
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Audio, id: 2, style: 0, key: 0, text: None, props: (0, 2), handlers: (0, 1), child_count: 0 });
    tree.props.push((A_SRC, Value::Asset(hash)));
    tree.props.push((A_PLAYING, Value::Bool(true)));
    tree.handlers.push((EventKind::TimeUpdate, Handler::Server(A_TIME)));
    let ops = vec![
        Op::DefAtom { id: A_SRC, value: "src".into() },
        Op::DefAtom { id: A_PLAYING, value: "playing".into() },
        Op::DefAtom { id: A_TIME, value: "time".into() },
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::Mount(tree),
    ];
    d.handle_frame(Frame::Batch(Batch { seq: 2, ops }));
    // Two seconds of sound, so it is still playing throughout.
    d.asset_ready(hash, wav_bytes(&[4_000i16; 16_000], 8_000));
    let t0 = Instant::now();
    d.tick(t0);
    let _ = d.paint(400, 300);
    let first = d.take_pending();
    assert!(first.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::TimeUpdate)), "{first:?}");
    // Again at once: nothing, the limit holds.
    let _ = d.paint(400, 300);
    assert!(d.take_pending().is_empty(), "four a second, not one a frame");
    // A quarter of a second later, one more, carrying position and length.
    d.tick(t0 + Duration::from_millis(260));
    let _ = d.paint(400, 300);
    let next = d.take_pending();
    let Some(Frame::Event(e)) = next.iter().find(|f| matches!(f, Frame::Event(e) if e.event == EventKind::TimeUpdate)) else { panic!("{next:?}") };
    let Value::List(payload) = &e.payload else { panic!("{:?}", e.payload) };
    assert_eq!(payload.len(), 2);
    assert_eq!(payload[1], Value::Int(2000), "the sound is two seconds long");
}

/// A two-frame GIF, written by the test.
fn gif_bytes(w: u16, h: u16, colours: &[[u8; 4]], delay: u16) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut out, w, h, &[]).unwrap();
        encoder.set_repeat(gif::Repeat::Infinite).unwrap();
        for colour in colours {
            let mut rgba: Vec<u8> = colour.iter().copied().cycle().take((w as usize) * (h as usize) * 4).collect();
            let mut frame = gif::Frame::from_rgba_speed(w, h, &mut rgba, 10);
            frame.delay = delay;
            encoder.write_frame(&frame).unwrap();
        }
    }
    out
}

/// Spec 03 §8: a `video` node sizes itself by its frames, advances on the
/// client's clock, schedules exactly the next frame, and stops when told.
#[test]
fn a_video_node_decodes_sizes_itself_and_advances_frame_by_frame() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    const A_SRC: u32 = 40;
    const A_PLAYING: u32 = 41;
    const A_LOOP: u32 = 42;
    const A_ENDED: u32 = 43;
    let hash: [u8; 32] = [3; 32];
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Video, id: 2, style: 0, key: 0, text: None, props: (0, 2), handlers: (0, 1), child_count: 0 });
    tree.props.push((A_SRC, Value::Asset(hash)));
    tree.props.push((A_PLAYING, Value::Bool(true)));
    tree.handlers.push((EventKind::Ended, Handler::Server(A_ENDED)));
    let ops = vec![
        Op::DefAtom { id: A_SRC, value: "src".into() },
        Op::DefAtom { id: A_PLAYING, value: "playing".into() },
        Op::DefAtom { id: A_LOOP, value: "loop".into() },
        Op::DefAtom { id: A_ENDED, value: "ended".into() },
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 2, ops })), vec![Frame::Ack { seq: 2 }]);
    assert!(d.pending_assets().contains(&hash), "the picture is wanted");
    // Before it arrives the node has no size and nothing plays.
    let _ = d.paint(400, 300);
    assert!(!d.video_playing());
    // Two frames of 8 × 6, a tenth of a second each.
    d.asset_ready(hash, gif_bytes(8, 6, &[[200, 30, 30, 255], [30, 30, 200, 255]], 10));
    let t0 = Instant::now();
    d.tick(t0);
    let list = d.paint(400, 300);
    // It measures itself by its frames …
    let node = d.session().lookup(2).unwrap();
    let rect = d.layout().rect(node).expect("laid out");
    assert_eq!((rect.w, rect.h), (8.0, 6.0), "the picture's own size");
    // … draws a textured quad …
    assert!(list.quads.iter().any(|q| q.params[2] as u32 == eui_render::TEXTURED_RGBA), "the frame is drawn");
    assert!(d.video_playing());
    // … and asks to be woken exactly when the next frame is due.
    let due = d.next_frame_at().expect("scheduled");
    assert!(due <= t0 + Duration::from_millis(101) && due > t0, "within the frame's own delay");
    assert_eq!(d.video_position_ms(2), Some(0));
    // The clock moves it on.
    d.tick(t0 + Duration::from_millis(120));
    let _ = d.paint(400, 300);
    assert_eq!(d.video_position_ms(2), Some(120));
    // Past the end without looping: it stops, and the node's handler hears.
    d.tick(t0 + Duration::from_millis(400));
    let _ = d.paint(400, 300);
    let out = d.take_pending();
    assert!(out.iter().any(|f| matches!(f, Frame::Event(e) if e.event == EventKind::Ended && e.node == 2)), "{out:?}");
    assert!(!d.video_playing(), "it stopped at the end");
    assert_eq!(d.video_position_ms(2), Some(200), "on the last frame");
    // Looping keeps it going and never ends.
    d.handle_frame(Frame::Batch(Batch { seq: 3, ops: vec![Op::SetProp { node: 2, prop: A_LOOP, value: Value::Bool(true) }, Op::SetProp { node: 2, prop: A_PLAYING, value: Value::Bool(true) }] }));
    d.tick(t0 + Duration::from_millis(500));
    let _ = d.paint(400, 300);
    d.tick(t0 + Duration::from_millis(1_100));
    let _ = d.paint(400, 300);
    assert!(d.video_playing(), "looping");
    assert!(d.take_pending().iter().all(|f| !matches!(f, Frame::Event(e) if e.event == EventKind::Ended)), "a loop has no end");
    // The tree owns the picture: drop the node and the player goes.
    d.handle_frame(Frame::Batch(Batch { seq: 4, ops: vec![Op::RemoveChild { parent: 1, index: 0, count: 1 }] }));
    let _ = d.paint(400, 300);
    assert!(!d.video_playing());
    assert_eq!(d.video_position_ms(2), None);
}

/// A resize holds the `Viewport` frame back for 50 ms so that dragging a
/// window's edge does not restyle the tree once per pixel. The frame that
/// falls due at the end of that wait is, by then, a frame in which nothing
/// has moved — so the paint takes the cached-list short cut. If that short
/// cut returns without clearing what it was woken for, the driver goes on
/// saying a frame is due, for ever, and the window redraws as fast as the
/// platform will let it.
#[test]
fn a_resize_that_settles_while_nothing_moves_leaves_the_window_at_rest() {
    use std::time::{Duration, Instant};
    let mut d = welcomed();
    d.tick(Instant::now());
    let _ = d.paint(400, 300);
    // The resize. Nothing is animating, so the only reason to wake is the
    // viewport that is owed in 50 ms.
    //
    // The clock below is anchored *after* the input, not before it: `input`
    // reads the wall clock itself, so the settle it arms is 50 ms from
    // whenever that call happened. Anchoring earlier makes this test a race
    // it loses on a loaded machine -- which is what `cargo test --workspace`
    // is, several suites at once -- and a flaky test in the suite that
    // guards against a busy loop is worse than no test at all.
    d.input(Input::Resized(380.0, 280.0, 1.0));
    let resized = Instant::now();
    d.tick(resized + Duration::from_millis(5));
    let _ = d.paint(380, 280);
    assert!(d.take_pending().iter().all(|f| !matches!(f, Frame::Viewport(_))), "held back for the settle");
    // The settle passes and the frame falls due.
    let after = resized + Duration::from_millis(60);
    assert!(d.tick(after), "a frame is due: the viewport the resize owes");
    let _ = d.paint(380, 280);
    let sent = d.take_pending();
    assert!(sent.iter().any(|f| matches!(f, Frame::Viewport(v) if v.width == 380)), "the viewport is sent: {sent:?}");
    // And now nothing is owed. This is the assertion that matters: a due
    // time left behind in the past is a redraw asked for on every pass of
    // the event loop, which is one core, for ever, on a window nobody is
    // touching.
    assert_eq!(d.next_frame_at(), None, "nothing is due once the viewport has gone");
    assert!(!d.tick(after + Duration::from_millis(1)), "the window sleeps");
}

/// A session the window refuses to open must say why *on the glass*.
///
/// It used to say it on stderr and leave an empty page. On a desktop that
/// sends somebody to a terminal; on a phone there is no terminal, and the
/// window is simply blank while the client knows exactly what is wrong —
/// which is how an afternoon goes on a simulator with an untrusted
/// certificate.
#[test]
fn a_refused_session_puts_its_reason_where_it_can_be_read() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.close("manifest: the signature does not verify".into());
    let _ = d.paint(400, 300);

    // The page the driver builds for a stopped session: a heading, and the
    // reason under it. Both are found by reading the tree back, because
    // what matters is that a person could read them, not that a field was
    // set somewhere.
    let session = d.session();
    let root = session.root().expect("the stopped page is mounted");
    let text: Vec<String> = session.preorder(root).filter_map(|ix| session.text_of(ix).map(ToOwned::to_owned)).collect();
    assert!(text.iter().any(|t| t == "The application stopped"), "no heading: {text:?}");
    assert!(text.iter().any(|t| t.contains("the signature does not verify")), "the reason is not on the page: {text:?}");
}
