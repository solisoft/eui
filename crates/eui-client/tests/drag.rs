//! Dragging, spec 06 §6: a list of rows that can be picked up and put down,
//! driven through the driver with no window and no socket.
//!
//! The gesture is the client's whole contribution. What the server hears is
//! three events — one grab, one per boundary crossed, one drop — and the point
//! of most of what follows is that it hears *no more than that*.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_client::{Driver, Input};
use eui_proto::*;

const A_DRAG: u32 = 1;
const A_ACCEPTS: u32 = 2;
const A_HANDLE: u32 = 3;
const A_GRAB: u32 = 4;
const A_OVER: u32 = 5;
const A_DROP: u32 = 6;
const A_CLICK: u32 = 7;
const A_TASK: u32 = 8;

const ROW_H: f32 = 40.0;

/// A column that accepts `task`s, holding `rows` draggable rows keyed `1..=n`,
/// each with a grip inside it. The rows also carry a `click` handler, because
/// a row that is both activatable and movable is the case that goes wrong.
///
/// ids: 1 the column, then per row `10*i` the row and `10*i+1` the grip.
fn board(rows: u32, handles: bool) -> Batch {
    let col = StyleRecord { display: Display::Column, ..Default::default() };
    let row = StyleRecord { display: Display::Row, height: Dim::Px(ROW_H as u16), ..Default::default() };
    let grip = StyleRecord { width: Dim::Px(20), height: Dim::Px(20), ..Default::default() };

    let mut tree = Subtree::default();
    tree.props.push((A_ACCEPTS, Value::Str("task".into())));
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 1), handlers: (0, 2), child_count: rows });
    tree.handlers.push((EventKind::DragOver, Handler::Server(A_OVER)));
    tree.handlers.push((EventKind::Drop, Handler::Server(A_DROP)));

    for i in 1..=rows {
        let (p0, h0) = (tree.props.len() as u32, tree.handlers.len() as u32);
        tree.props.push((A_DRAG, Value::Str("task".into())));
        tree.handlers.push((EventKind::DragStart, Handler::Server(A_GRAB)));
        tree.handlers.push((EventKind::Click, Handler::Server(A_CLICK)));
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 10 * i, style: 2, key: A_TASK + i, text: None, props: (p0, 1), handlers: (h0, 2), child_count: 1 });
        let gp = tree.props.len() as u32;
        if handles {
            tree.props.push((A_HANDLE, Value::Bool(true)));
        }
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 10 * i + 1, style: 3, key: 0, text: None, props: (gp, u32::from(handles)), handlers: (0, 0), child_count: 0 });
    }

    let mut ops = vec![
        Op::DefAtom { id: A_DRAG, value: "drag".into() },
        Op::DefAtom { id: A_ACCEPTS, value: "accepts".into() },
        Op::DefAtom { id: A_HANDLE, value: "drag_handle".into() },
        Op::DefAtom { id: A_GRAB, value: "grabbed".into() },
        Op::DefAtom { id: A_OVER, value: "moved".into() },
        Op::DefAtom { id: A_DROP, value: "dropped".into() },
        Op::DefAtom { id: A_CLICK, value: "opened".into() },
        Op::DefStyle { id: 1, record: col },
        Op::DefStyle { id: 2, record: row },
        Op::DefStyle { id: 3, record: grip },
    ];
    for i in 1..=rows {
        ops.push(Op::DefAtom { id: A_TASK + i, value: format!("task-{i}") });
    }
    ops.push(Op::Mount(tree));
    Batch { seq: 1, ops }
}

fn open(rows: u32, handles: bool) -> Driver {
    let mut d = Driver::new(400.0, 600.0, 1.0, 0);
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false })).is_empty());
    assert_eq!(d.handle_frame(Frame::Batch(board(rows, handles))), vec![Frame::Ack { seq: 1 }]);
    let _ = d.paint(400, 600);
    d
}

fn centre(d: &mut Driver, id: u32) -> (f32, f32) {
    let _ = d.paint(400, 600);
    let r = d.layout().rect(d.session().lookup(id).unwrap()).unwrap();
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Every event a burst of input produced, as `(kind, node id, payload)`.
fn events(frames: &[Frame]) -> Vec<(EventKind, u32, Value)> {
    frames
        .iter()
        .filter_map(|f| match f {
            Frame::Event(e) => Some((e.event, e.node, e.payload.clone())),
            _ => None,
        })
        .collect()
}

fn kinds(frames: &[Frame]) -> Vec<EventKind> {
    events(frames).into_iter().map(|(k, _, _)| k).collect()
}

/// The slot an event carries — the third number of a `drag_over` or a `drop`.
fn slot(payload: &Value) -> i64 {
    let Value::List(items) = payload else { panic!("payload is a list: {payload:?}") };
    let Some(Value::Int(n)) = items.get(2) else { panic!("third field is the slot: {payload:?}") };
    *n
}

/// Press at a point, then move to another, in one burst.
fn press_and_move(d: &mut Driver, from: (f32, f32), to: (f32, f32)) -> Vec<Frame> {
    let mut out = d.input(Input::PointerMove(from.0, from.1));
    out.extend(d.input(Input::PointerDown(0)));
    out.extend(d.input(Input::PointerMove(to.0, to.1)));
    out
}

// --------------------------------------------------------------- the grab

/// §6.1 step 1: arming is invisible. A press that wobbles inside the slop and
/// lifts is a click, and nothing about it may betray that a drag was possible.
#[test]
fn a_press_that_stays_inside_the_slop_is_still_a_click() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let mut out = press_and_move(&mut d, at, (at.0 + 3.0, at.1 + 2.0));
    out.extend(d.input(Input::PointerUp(0)));
    let seen = kinds(&out);
    assert!(seen.contains(&EventKind::Click), "a wobble is still a press: {seen:?}");
    assert!(!seen.contains(&EventKind::DragStart), "and nothing was grabbed: {seen:?}");
    assert!(!seen.contains(&EventKind::Drop), "{seen:?}");
}

/// §6.1 step 2 and §6.2: past the slop the press becomes a drag, the grab is
/// reported once and names the **source** — the row carrying `drag`, not the
/// grip the hand actually landed on.
#[test]
fn the_slop_turns_the_press_into_a_drag_and_names_the_row() {
    let mut d = open(4, false);
    let at = centre(&mut d, 11);
    let out = press_and_move(&mut d, at, (at.0, at.1 + 30.0));
    let grabs: Vec<_> = events(&out).into_iter().filter(|(k, ..)| *k == EventKind::DragStart).collect();
    assert_eq!(grabs.len(), 1, "grabbed once: {:?}", kinds(&out));
    assert_eq!(grabs[0].1, 10, "the row is the source, not the grip inside it");
}

/// §2: the lift is the drop. A gesture that became a drag reports no
/// `pointer_up` and no `click` — otherwise putting a card down also opens it.
#[test]
fn a_drag_reports_no_click_and_no_release() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let mut out = press_and_move(&mut d, at, (at.0, at.1 + ROW_H * 2.0));
    out.extend(d.input(Input::PointerUp(0)));
    let seen = kinds(&out);
    assert!(seen.contains(&EventKind::Drop), "it was dropped: {seen:?}");
    assert!(!seen.contains(&EventKind::Click), "and never opened: {seen:?}");
    assert!(!seen.contains(&EventKind::PointerUp), "and never released: {seen:?}");
}

/// §3.4: a handle grabs at once, with no slop to cross. This is the route that
/// lets a finger drag a row out of a list it could otherwise only scroll.
#[test]
fn a_handle_grabs_without_waiting_for_the_slop() {
    let mut d = open(4, true);
    let at = centre(&mut d, 11);
    let out = press_and_move(&mut d, at, (at.0 + 1.0, at.1 + 1.0));
    assert!(kinds(&out).contains(&EventKind::DragStart), "one pixel was enough: {:?}", kinds(&out));
}

// --------------------------------------------------------------- the slot

/// §6.1 step 4: the drop names the slot the hand was over, counted among the
/// container's draggable items.
#[test]
fn the_drop_names_the_slot_it_landed_in() {
    let mut d = open(4, false);
    let from = centre(&mut d, 10);
    let onto = centre(&mut d, 40);
    let mut out = press_and_move(&mut d, from, onto);
    out.extend(d.input(Input::PointerUp(0)));
    let drops: Vec<_> = events(&out).into_iter().filter(|(k, ..)| *k == EventKind::Drop).collect();
    assert_eq!(drops.len(), 1, "dropped once: {:?}", kinds(&out));
    assert_eq!(drops[0].1, 1, "the column hears it, not the row");
    assert_eq!(slot(&drops[0].2), 3, "the fourth row's slot");
}

/// §6.2: every place in the list, and the last of them especially. There are
/// n + 1 positions in a list of n, and the end is the one a clamp to n − 1
/// quietly takes away — which reads, from the hand, as a card that always
/// lands on top.
#[test]
fn every_place_in_the_list_can_be_reached_including_the_end() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let mut out = press_and_move(&mut d, at, (at.0, at.1 + 6.0));
    // Down through every row, and then past the last of them.
    let mut seen = Vec::new();
    for step in 1..=5u8 {
        out.extend(d.input(Input::PointerMove(at.0, at.1 + ROW_H * f32::from(step))));
        if let Some((_, _, p)) = events(&out).into_iter().rfind(|(k, ..)| *k == EventKind::DragOver) {
            seen.push(slot(&p));
        }
    }
    seen.dedup();
    assert_eq!(seen, vec![1, 2, 3, 4], "every slot below it, ending past the last row: {seen:?}");
}

/// The regression that produced it: a column resolves the slot from which of
/// its own children can be picked up, so a card wrapped in something that
/// cannot is a column with no draggable children — and every drop lands at
/// the top. The prop has to be on the child the container holds.
#[test]
fn a_slot_is_counted_from_the_container_s_own_children() {
    let mut d = open(4, false);
    // Row 10 is the container's child and carries `drag`; its grip, 11, is a
    // descendant and does not. Pressing the grip still grabs the row, and the
    // slot is still resolved against the rows.
    let at = centre(&mut d, 11);
    let out = press_and_move(&mut d, at, (at.0, at.1 + ROW_H * 2.0));
    let last = events(&out).into_iter().rfind(|(k, ..)| *k == EventKind::DragOver);
    assert_eq!(last.map(|(_, _, p)| slot(&p)), Some(2), "the third row, not the top");
}

/// §2: `drag_over` is coalesced twice — per frame, and again against the last
/// one sent. Crossing three rows is three events, however many samples it took.
#[test]
fn a_drag_over_is_sent_once_a_row_crossed_not_once_a_sample() {
    let mut d = open(5, false);
    let at = centre(&mut d, 10);
    let mut out = press_and_move(&mut d, at, (at.0, at.1 + 6.0));
    // Sixty samples down three rows.
    for i in 1..=60u8 {
        let y = at.1 + 6.0 + (ROW_H * 3.0 - 6.0) * f32::from(i) / 60.0;
        out.extend(d.input(Input::PointerMove(at.0, y)));
    }
    let slots: Vec<i64> = events(&out).into_iter().filter(|(k, ..)| *k == EventKind::DragOver).map(|(_, _, p)| slot(&p)).collect();
    assert!(slots.len() <= 4, "one an row crossed, not one a sample: {} events, {slots:?}", slots.len());
    assert_eq!(slots.first(), Some(&0), "it began in its own slot");
    assert_eq!(slots.last(), Some(&3), "and ended three down");
    let mut sorted = slots.clone();
    sorted.dedup();
    assert_eq!(sorted, slots, "and never repeated itself");
}

/// §6.2: the slot is the box the pointer is in, not the nearest boundary.
/// Once the server previews the move by making it, the row in the hand is
/// under the pointer — a midpoint rule oscillates there, and this does not.
#[test]
fn the_slot_is_the_box_the_pointer_is_in() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let mut out = press_and_move(&mut d, at, (at.0, at.1 + 8.0));
    // Still inside row 0's box, well past its midpoint.
    let last = events(&out).into_iter().rfind(|(k, ..)| *k == EventKind::DragOver);
    assert_eq!(last.map(|(_, _, p)| slot(&p)), Some(0), "still its own slot");
    // A hair into the next row's box, and only now does it change.
    out.extend(d.input(Input::PointerMove(at.0, at.1 + ROW_H)));
    let last = events(&out).into_iter().rfind(|(k, ..)| *k == EventKind::DragOver);
    assert_eq!(last.map(|(_, _, p)| slot(&p)), Some(1));
}

// ------------------------------------------------------------- the ending

/// §6.1 step 5: `Escape` puts the row back. It is a `drop` with the sentinel
/// slot, so a server that handles `drop` and nothing else is complete — and it
/// is tested before focus, because a pointer drag leaves focus nowhere.
#[test]
fn escape_ends_the_drag_with_the_sentinel_slot() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let mut out = press_and_move(&mut d, at, (at.0, at.1 + ROW_H * 2.0));
    out.extend(d.input(Input::Key { key: "Escape".into(), modifiers: 0, down: true }));
    let drops: Vec<_> = events(&out).into_iter().filter(|(k, ..)| *k == EventKind::Drop).collect();
    assert_eq!(drops.len(), 1, "ended once: {:?}", kinds(&out));
    assert_eq!(slot(&drops[0].2), -1, "and said so");
    // The gesture is over: a later lift says nothing more.
    let after = d.input(Input::PointerUp(0));
    assert!(!kinds(&after).contains(&EventKind::Drop), "and does not end twice: {:?}", kinds(&after));
}

/// §6.1 step 5: a window that loses the input has lost the hand with it.
#[test]
fn losing_the_input_ends_the_drag_as_escape_does() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let mut out = press_and_move(&mut d, at, (at.0, at.1 + ROW_H * 2.0));
    out.extend(d.input(Input::Unfocused));
    let drops: Vec<_> = events(&out).into_iter().filter(|(k, ..)| *k == EventKind::Drop).collect();
    assert_eq!(drops.len(), 1, "{:?}", kinds(&out));
    assert_eq!(slot(&drops[0].2), -1);
}

/// The identity invariant, and the most important test here. A `MoveChild`
/// arriving mid-drag is the *normal* case — it is the server previewing the
/// move — and the gesture must survive its own preview. The client holds the
/// row by key, so it does.
#[test]
fn a_move_child_under_the_hand_does_not_lose_the_drag() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let out = press_and_move(&mut d, at, (at.0, at.1 + ROW_H * 2.0));
    assert!(kinds(&out).contains(&EventKind::DragStart));

    // The server answers the way a live reorder does: the row in the hand
    // moves to where the hand is.
    let moved = d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::MoveChild { parent: 1, from: 0, to: 2 }] }));
    assert_eq!(moved, vec![Frame::Ack { seq: 2 }]);
    let _ = d.paint(400, 600);

    // The drag is still live, still holding the same row, and says so.
    let on = d.input(Input::PointerMove(at.0, at.1 + ROW_H * 3.0));
    let over: Vec<_> = events(&on).into_iter().filter(|(k, ..)| *k == EventKind::DragOver).collect();
    assert!(!over.is_empty(), "the drag is still running: {:?}", kinds(&on));
    let done = d.input(Input::PointerUp(0));
    let drops: Vec<_> = events(&done).into_iter().filter(|(k, ..)| *k == EventKind::Drop).collect();
    assert_eq!(drops.len(), 1, "and ends as a drop, not a cancel: {:?}", kinds(&done));
    assert_ne!(slot(&drops[0].2), -1, "the source never went missing");
}

/// §6.1 step 5: a source the server takes away ends the gesture rather than
/// leaving it open over a row that no longer exists.
#[test]
fn a_row_that_leaves_the_tree_ends_the_drag() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let out = press_and_move(&mut d, at, (at.0, at.1 + ROW_H));
    assert!(kinds(&out).contains(&EventKind::DragStart));

    let gone = d.handle_frame(Frame::Batch(Batch { seq: 2, ops: vec![Op::RemoveChild { parent: 1, index: 0, count: 1 }] }));
    assert_eq!(gone, vec![Frame::Ack { seq: 2 }]);
    let _ = d.paint(400, 600);

    let on = d.input(Input::PointerMove(at.0, at.1 + ROW_H * 2.0));
    let drops: Vec<_> = events(&on).into_iter().filter(|(k, ..)| *k == EventKind::Drop).collect();
    assert_eq!(drops.len(), 1, "the gesture ended: {:?}", kinds(&on));
    assert_eq!(slot(&drops[0].2), -1, "as a cancel, because nothing was put anywhere");
}

/// §3: the shape follows from the prop, with no style spent on it.
#[test]
fn the_pointer_grabs_over_a_row_and_holds_while_it_is_carried() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let _ = d.input(Input::PointerMove(at.0, at.1));
    let _ = d.paint(400, 600);
    assert_eq!(d.cursor(), Cursor::Grab, "over a thing that can be picked up");
    let _ = d.input(Input::PointerDown(0));
    let _ = d.input(Input::PointerMove(at.0, at.1 + ROW_H));
    assert_eq!(d.cursor(), Cursor::Grabbing, "and while it is being carried");
}

/// 04 §7.1 and 06 §6.2: a windowed list's children are not its rows — they are
/// the window's, and each says which row it is. Counting ordinals there would
/// give a position within the window, which is a fact about the client's
/// scroll offset rather than about the records the server holds.
#[test]
fn a_windowed_list_reports_the_row_and_not_the_place_in_the_window() {
    const A_COUNT: u32 = 40;
    const A_ROW: u32 = 41;
    let list = StyleRecord { display: Display::Column, height: Dim::Px(200), ..Default::default() };
    let row = StyleRecord { display: Display::Row, height: Dim::Px(ROW_H as u16), ..Default::default() };

    let mut tree = Subtree::default();
    tree.props.push((A_ACCEPTS, Value::Str("task".into())));
    tree.props.push((A_COUNT, Value::Int(500)));
    tree.nodes.push(FlatNode { kind: NodeKind::List, id: 1, style: 1, key: 0, text: None, props: (0, 2), handlers: (0, 1), child_count: 3 });
    tree.handlers.push((EventKind::Drop, Handler::Server(A_DROP)));
    // The window holds rows 200, 201, 202 — nowhere near the start.
    for (n, r) in [(1u32, 200i64), (2, 201), (3, 202)] {
        let p0 = tree.props.len() as u32;
        tree.props.push((A_DRAG, Value::Str("task".into())));
        tree.props.push((A_ROW, Value::Int(r)));
        let h0 = tree.handlers.len() as u32;
        tree.handlers.push((EventKind::DragStart, Handler::Server(A_GRAB)));
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 10 * n, style: 2, key: A_TASK + n, text: None, props: (p0, 2), handlers: (h0, 1), child_count: 0 });
    }

    let mut d = Driver::new(400.0, 600.0, 1.0, 0);
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false })).is_empty());
    let ops = vec![
        Op::DefAtom { id: A_DRAG, value: "drag".into() },
        Op::DefAtom { id: A_ACCEPTS, value: "accepts".into() },
        Op::DefAtom { id: A_GRAB, value: "grabbed".into() },
        Op::DefAtom { id: A_DROP, value: "dropped".into() },
        Op::DefAtom { id: A_COUNT, value: "count".into() },
        Op::DefAtom { id: A_ROW, value: "row".into() },
        Op::DefAtom { id: A_TASK + 1, value: "t1".into() },
        Op::DefAtom { id: A_TASK + 2, value: "t2".into() },
        Op::DefAtom { id: A_TASK + 3, value: "t3".into() },
        Op::DefStyle { id: 1, record: list },
        Op::DefStyle { id: 2, record: row },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 1, ops })), vec![Frame::Ack { seq: 1 }]);
    let _ = d.paint(400, 600);

    let from = centre(&mut d, 10);
    let onto = centre(&mut d, 30);
    let mut out = press_and_move(&mut d, from, onto);
    out.extend(d.input(Input::PointerUp(0)));
    let drops: Vec<_> = events(&out).into_iter().filter(|(k, ..)| *k == EventKind::Drop).collect();
    assert_eq!(drops.len(), 1, "{:?}", kinds(&out));
    assert_eq!(slot(&drops[0].2), 202, "the row, not the third place in the window");
}

/// §6.4: a drag held at the foot of a list moves it, because the alternative
/// is a list you cannot reach the bottom of without letting go — and it
/// reports **one** `scroll` when it stops, not one a frame.
#[test]
fn a_drag_at_the_foot_of_a_list_carries_the_view_and_reports_once() {
    const A_ITEM_H: u32 = 200;
    const ROWS: u32 = 60;
    // No `overflow: scroll` — a `list` scrolls by being one, and asking for
    // it as well makes the width indefinite, which leaves every virtualised
    // row nought pixels wide and nothing to point at.
    let list = StyleRecord { display: Display::Column, height: Dim::Px(160), ..Default::default() };
    let row = StyleRecord { display: Display::Row, height: Dim::Px(ROW_H as u16), ..Default::default() };

    let mut tree = Subtree::default();
    tree.props.push((A_ACCEPTS, Value::Str("task".into())));
    tree.props.push((A_ITEM_H, Value::Int(ROW_H as i64)));
    tree.nodes.push(FlatNode { kind: NodeKind::List, id: 1, style: 1, key: 0, text: None, props: (0, 2), handlers: (0, 2), child_count: ROWS });
    tree.handlers.push((EventKind::Drop, Handler::Server(A_DROP)));
    tree.handlers.push((EventKind::Scroll, Handler::Server(A_OVER)));
    for n in 1..=ROWS {
        let p0 = tree.props.len() as u32;
        tree.props.push((A_DRAG, Value::Str("task".into())));
        let h0 = tree.handlers.len() as u32;
        tree.handlers.push((EventKind::DragStart, Handler::Server(A_GRAB)));
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 10 * n, style: 2, key: A_TASK + n, text: None, props: (p0, 1), handlers: (h0, 1), child_count: 0 });
    }

    let mut d = Driver::new(400.0, 600.0, 1.0, 0);
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false })).is_empty());
    let mut ops = vec![
        Op::DefAtom { id: A_DRAG, value: "drag".into() },
        Op::DefAtom { id: A_ACCEPTS, value: "accepts".into() },
        Op::DefAtom { id: A_GRAB, value: "grabbed".into() },
        Op::DefAtom { id: A_OVER, value: "scrolled".into() },
        Op::DefAtom { id: A_DROP, value: "dropped".into() },
        Op::DefAtom { id: A_ITEM_H, value: "item_height".into() },
        Op::DefStyle { id: 1, record: list },
        Op::DefStyle { id: 2, record: row },
    ];
    for n in 1..=ROWS {
        ops.push(Op::DefAtom { id: A_TASK + n, value: format!("t{n}") });
    }
    ops.push(Op::Mount(tree));
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 1, ops })), vec![Frame::Ack { seq: 1 }]);
    let _ = d.paint(400, 600);

    let scroller = d.session().lookup(1).unwrap();
    assert_eq!(d.session().node(scroller).unwrap().scroll.1, 0, "it starts at the top");

    // Grab the first row and hold the hand at the very foot of the list.
    let at = centre(&mut d, 10);
    let view = d.layout().rect(scroller).unwrap();
    let foot = view.y + view.h - 2.0;
    let mut out = press_and_move(&mut d, at, (at.0, foot));
    assert!(kinds(&out).contains(&EventKind::DragStart));

    // Twenty frames of holding still, a frame every 16 ms.
    let mut t = std::time::Instant::now();
    for _ in 0..20 {
        t += std::time::Duration::from_millis(16);
        d.tick(t);
        let _ = d.paint(400, 600);
        out.extend(d.take_pending());
    }
    let moved = d.session().node(scroller).unwrap().scroll.1;
    assert!(moved > 0, "the view came with the hand: {moved}");

    // Nothing was said about it while it was moving.
    let during: Vec<_> = events(&out).into_iter().filter(|(k, ..)| *k == EventKind::Scroll).collect();
    assert!(during.is_empty(), "the movement is silent until it stops: {during:?}");

    // And exactly one `scroll` lands with the drop, saying where it ended up.
    out.extend(d.input(Input::PointerUp(0)));
    out.extend(d.take_pending());
    let all: Vec<_> = events(&out).into_iter().filter(|(k, ..)| *k == EventKind::Scroll).collect();
    assert_eq!(all.len(), 1, "one scroll for the whole gesture, not one a frame: {all:?}");
    assert_eq!(all[0].2, Value::List(vec![Value::Int(0), Value::Int(moved)]));
}

// ----------------------------------------------------------- the keyboard

/// 03 §3: a thing that can be moved can be reached without a pointer. Nothing
/// is claimed until it is grabbed — an arrow with nothing in the hand is still
/// the scroller's.
#[test]
fn a_row_that_can_be_moved_is_in_the_tab_order_and_claims_nothing_yet() {
    let mut d = open(4, false);
    let _ = d.input(Input::Key { key: "Tab".into(), modifiers: 0, down: true });
    let _ = d.paint(400, 600);
    assert_eq!(d.session().node(d.focused().expect("something took focus")).map(|n| n.id), Some(10), "the first row");
    let out = d.input(Input::Key { key: "ArrowDown".into(), modifiers: 0, down: true });
    assert!(!kinds(&out).contains(&EventKind::DragStart), "a bare arrow is not a move: {:?}", kinds(&out));
}

/// `Space` picks a row up, the arrows move it, `Space` puts it down — but only
/// on a row with no `click` to stand for. The rows in this tree are buttons as
/// well, so they keep the key, and reach the grab through their grip instead.
/// That conflict is the whole reason the grip is a separate focus stop.
#[test]
fn space_is_left_alone_on_a_row_that_is_also_a_button() {
    let mut d = open(4, false);
    let _ = d.input(Input::Key { key: "Tab".into(), modifiers: 0, down: true });
    let _ = d.paint(400, 600);
    let out = d.input(Input::Key { key: " ".into(), modifiers: 0, down: true });
    assert!(!kinds(&out).contains(&EventKind::DragStart), "the button keeps its key: {:?}", kinds(&out));
}

/// §6.1 step 5 from the keyboard: `Escape` puts a grabbed row back.
#[test]
fn escape_puts_a_grabbed_row_back() {
    let mut d = open(4, false);
    let at = centre(&mut d, 10);
    let mut out = press_and_move(&mut d, at, (at.0, at.1 + ROW_H * 2.0));
    out.extend(d.input(Input::Key { key: "Escape".into(), modifiers: 0, down: true }));
    let drops: Vec<_> = events(&out).into_iter().filter(|(k, ..)| *k == EventKind::Drop).collect();
    assert_eq!(slot(&drops[0].2), -1);
}
