//! Spec 06 §5: one finger, reported as the pointer.
//!
//! The window has contacts and the protocol has a pointer, and the whole
//! difference between a tap, a drag and a scroll is decided in the driver —
//! on the near side of the tree, because whether a stroke belongs to the
//! node under it depends on whether that node asked to hear moves.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::time::{Duration, Instant};

use eui_client::{Driver, Input};
use eui_proto::*;
use eui_theme::Role;

const ATOM: u32 = 1;

/// The scroller in every tree below, so a test can ask where the view is.
const VIEW: u32 = 2;

fn welcomed() -> Driver {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    d
}

/// A button with a server `click`, on its own: the tree a tap is aimed at.
fn button() -> Driver {
    let mut d = welcomed();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 0 });
    tree.handlers.push((EventKind::Click, Handler::Server(ATOM)));
    let ops = vec![
        Op::DefAtom { id: ATOM, value: "pressed".into() },
        Op::DefStyle { id: 1, record: StyleRecord { width: Dim::Px(200), height: Dim::Px(80), bg: ColorRef::role(Role::AccentBase.id()), ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 1, ops })), vec![Frame::Ack { seq: 1 }]);
    let _ = d.paint(400, 300);
    d
}

/// A 100 px scroller holding ten 22 px rows, each of them a button: every
/// thing a stroke can be confused for, in one tree.
fn scroller() -> Driver {
    let mut d = welcomed();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: VIEW, style: 11, key: 0, text: None, props: (0, 0), handlers: (0, 1), child_count: 10 });
    tree.handlers.push((EventKind::Scroll, Handler::Server(ATOM)));
    for i in 0..10u32 {
        // A scroll that starts on a button must not press it, which is what
        // the slop is for.
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 10 + i, style: 12, key: 0, text: None, props: (0, 0), handlers: (1 + i, 1), child_count: 0 });
        tree.handlers.push((EventKind::Click, Handler::Server(ATOM)));
    }
    let ops = vec![
        Op::DefAtom { id: ATOM, value: "acted".into() },
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), width: Dim::Px(200), ..Default::default() } },
        Op::DefStyle { id: 12, record: StyleRecord { height: Dim::Px(22), width: Dim::Px(200), ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 1, ops })), vec![Frame::Ack { seq: 1 }]);
    let _ = d.paint(400, 300);
    d
}

/// The same scroller, but the first row asked to hear `pointer_move`: a
/// slider, as far as the finger is concerned. It takes the stroke, and the
/// view it sits in never moves.
fn slider() -> Driver {
    let mut d = welcomed();
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: VIEW, style: 11, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 3, style: 12, key: 0, text: None, props: (0, 0), handlers: (0, 2), child_count: 0 });
    tree.handlers.push((EventKind::PointerMove, Handler::Server(ATOM)));
    tree.handlers.push((EventKind::PointerUp, Handler::Server(ATOM)));
    // Tall enough that the scroller has somewhere to go, so "the view did
    // not move" is a fact about the gesture and not about the layout.
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 4, style: 13, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 0 });
    let ops = vec![
        Op::DefAtom { id: ATOM, value: "slid".into() },
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), width: Dim::Px(200), ..Default::default() } },
        Op::DefStyle { id: 12, record: StyleRecord { height: Dim::Px(40), width: Dim::Px(200), ..Default::default() } },
        Op::DefStyle { id: 13, record: StyleRecord { height: Dim::Px(400), width: Dim::Px(200), ..Default::default() } },
        Op::Mount(tree),
    ];
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 1, ops })), vec![Frame::Ack { seq: 1 }]);
    let _ = d.paint(400, 300);
    d
}

fn events(frames: &[Frame], kind: EventKind) -> Vec<u32> {
    frames
        .iter()
        .filter_map(|f| match f {
            Frame::Event(e) if e.event == kind => Some(e.node),
            _ => None,
        })
        .collect()
}

/// Where the view stands, in logical px from the top.
fn offset(d: &Driver) -> i64 {
    d.session().node(d.session().lookup(VIEW).unwrap()).unwrap().scroll.1
}

#[test]
fn a_tap_is_a_press_and_a_release_on_what_it_landed_on() {
    let mut d = button();
    assert!(d.input(Input::TouchDown(1, 100.0, 40.0)).is_empty(), "no pointer_down handler to hear it");
    let out = d.input(Input::TouchUp(1, 100.0, 40.0));
    assert_eq!(events(&out, EventKind::Click), vec![1], "{out:?}");
}

#[test]
fn a_tap_that_wobbles_within_the_slop_still_clicks_what_it_was_aimed_at() {
    let mut d = button();
    d.input(Input::TouchDown(1, 100.0, 40.0));
    // Five pixels of hand tremor: under the eight-pixel slop, so the gesture
    // is still a press and the release is taken where it landed.
    d.input(Input::TouchMove(1, 103.0, 44.0));
    let out = d.input(Input::TouchUp(1, 103.0, 44.0));
    assert_eq!(events(&out, EventKind::Click), vec![1], "{out:?}");
}

#[test]
fn a_finger_leaves_no_hover_behind_it() {
    let mut d = button();
    d.input(Input::TouchDown(1, 100.0, 40.0));
    // Mid-gesture the pointer is on the button, and it says so: a hand
    // cursor is what a node with a `click` handler asks for.
    assert_eq!(d.cursor(), eui_proto::Cursor::Pointer);
    d.input(Input::TouchUp(1, 100.0, 40.0));
    // And once the finger is gone nothing is under the pointer. Without
    // this, a tile lit on `pointer_enter` stays lit — a highlight no
    // pointer will ever leave.
    assert_eq!(d.cursor(), eui_proto::Cursor::Default, "the pointer is nowhere");
}

#[test]
fn a_stroke_past_the_slop_scrolls_the_view_and_never_clicks_the_row() {
    let mut d = scroller();
    // Down on the first row, which is a button.
    d.input(Input::TouchDown(1, 100.0, 10.0));
    let mut clicks = Vec::new();
    let mut y = 10.0;
    // Up the glass by 60 px in six steps; the first crosses the slop.
    for _ in 0..6 {
        y -= 10.0;
        clicks.extend(events(&d.input(Input::TouchMove(1, 100.0, y)), EventKind::Click));
    }
    clicks.extend(events(&d.input(Input::TouchUp(1, 100.0, y)), EventKind::Click));
    assert!(clicks.is_empty(), "a scroll must not press the row it started on: {clicks:?}");
    // The view followed the finger, and the eight pixels of slop are part
    // of the stroke rather than swallowed by it — otherwise the view lags
    // the finger by the slop for the whole gesture.
    assert_eq!(offset(&d), 60, "60 px of stroke, 60 px of view");
}

#[test]
fn the_press_a_scroll_began_with_is_given_back_before_the_view_moves() {
    let mut d = slider();
    // The first row here hears `pointer_up`, so the giving-back is visible.
    // Down on it, then straight past the slop — but sideways, which the
    // slider does not take, so the gesture is a scroll.
    d.input(Input::TouchDown(1, 100.0, 20.0));
    assert!(d.input(Input::TouchMove(1, 100.0, 60.0)).is_empty(), "06 §2: the move is held for the frame");
    // It asked for moves, so it keeps the stroke: this is the drag case.
    let _ = d.paint(400, 300);
    assert_eq!(events(&d.take_pending(), EventKind::PointerMove), vec![3]);

    // Now the same stroke on a row that asked for nothing.
    let mut d = scroller();
    d.input(Input::TouchDown(1, 100.0, 10.0));
    let out = d.input(Input::TouchMove(1, 100.0, -10.0));
    assert!(events(&out, EventKind::Click).is_empty(), "no click: {out:?}");
    assert!(offset(&d) > 0, "and the view moved");
}

#[test]
fn a_node_that_asked_for_moves_takes_the_stroke_and_the_view_stays_put() {
    let mut d = slider();
    d.input(Input::TouchDown(1, 100.0, 20.0));
    // Straight down the glass, far past the slop. A finger here would
    // scroll anything else; the node that asked for moves gets it instead.
    // 06 §2: at most one `pointer_move` a frame, so the move is held until
    // the paint that follows it and reported there.
    assert!(d.input(Input::TouchMove(1, 100.0, 60.0)).is_empty());
    let _ = d.paint(400, 300);
    let sent = d.take_pending();
    assert_eq!(events(&sent, EventKind::PointerMove), vec![3], "the slider hears the move: {sent:?}");
    assert_eq!(offset(&d), 0, "and the view under it did not move");
    let out = d.input(Input::TouchUp(1, 100.0, 60.0));
    assert_eq!(events(&out, EventKind::PointerUp), vec![3], "{out:?}");
    assert_eq!(offset(&d), 0, "a drag never flings");
}

#[test]
fn a_second_finger_is_ignored_while_the_first_is_down() {
    let mut d = scroller();
    d.input(Input::TouchDown(1, 100.0, 10.0));
    // A palm, or a second thumb. Version 1 has no gesture that wants two,
    // and a stray contact must not move the view.
    assert!(d.input(Input::TouchDown(2, 50.0, 90.0)).is_empty());
    assert!(d.input(Input::TouchMove(2, 50.0, 10.0)).is_empty());
    assert_eq!(offset(&d), 0, "the second contact moved nothing");
    // The first finger still owns the gesture.
    d.input(Input::TouchMove(1, 100.0, -30.0));
    assert_eq!(offset(&d), 40);
}

#[test]
fn a_cancelled_gesture_releases_the_press_without_clicking() {
    let mut d = button();
    d.input(Input::TouchDown(1, 100.0, 40.0));
    let out = d.input(Input::TouchCancel(1));
    assert!(events(&out, EventKind::Click).is_empty(), "{out:?}");
    // And the contact is forgotten, so the finger that comes back starts a
    // gesture rather than finishing the one the system took away.
    d.input(Input::TouchDown(1, 100.0, 40.0));
    let out = d.input(Input::TouchUp(1, 100.0, 40.0));
    assert_eq!(events(&out, EventKind::Click), vec![1], "{out:?}");
}

#[test]
fn a_finger_that_leaves_the_glass_still_moving_carries_the_view_on() {
    let mut d = scroller();
    let t0 = Instant::now();
    d.input_at(Input::TouchDown(1, 100.0, 90.0), t0);
    // Six samples a frame apart, 12 px each: a logical pixel per
    // millisecond, upward. The instants are handed over rather than slept
    // through — a stroke is a shape in time, and sleeping for it makes the
    // test a race with the machine, which is a race it loses on a loaded
    // runner where the sleeps stretch past the pause that means the finger
    // was held rather than thrown.
    let mut y = 90.0;
    let mut at = t0;
    for _ in 0..6 {
        at += Duration::from_millis(12);
        y -= 12.0;
        d.input_at(Input::TouchMove(1, 100.0, y), at);
    }
    let at_lift = offset(&d);
    let lifted = at + Duration::from_millis(12);
    d.input_at(Input::TouchUp(1, 100.0, y), lifted);
    assert!(d.animating(), "the view is still moving after the finger left");
    // It arrives rather than stopping: part-way at a third of the glide,
    // and past where the finger let go by the end.
    d.tick(lifted + Duration::from_millis(110));
    let _ = d.paint(400, 300);
    let mid = offset(&d);
    d.tick(lifted + Duration::from_millis(400));
    let _ = d.paint(400, 300);
    let landed = offset(&d);
    assert!(landed > at_lift, "the fling carried it past {at_lift}: {landed}");
    assert!(mid <= landed, "and it got there in order: {mid} then {landed}");
    assert!(!d.animating(), "and it stopped");
}

#[test]
fn a_finger_that_stops_before_it_lifts_does_not_fling() {
    let mut d = scroller();
    let t0 = Instant::now();
    d.input_at(Input::TouchDown(1, 100.0, 90.0), t0);
    d.input_at(Input::TouchMove(1, 100.0, 50.0), t0 + Duration::from_millis(12));
    // Placed, moved, held, lifted. The last samples say it was moving, but
    // the pause is the whole story: nothing should be thrown.
    let held = t0 + Duration::from_millis(140);
    d.input_at(Input::TouchMove(1, 100.0, 50.0), held);
    let settled = offset(&d);
    d.input_at(Input::TouchUp(1, 100.0, 50.0), held + Duration::from_millis(4));
    assert!(!d.animating(), "a held finger throws nothing");
    assert_eq!(offset(&d), settled);
}

#[test]
fn a_window_that_loses_the_input_forgets_the_finger_on_it() {
    let mut d = button();
    d.input(Input::TouchDown(1, 100.0, 40.0));
    // The application went to the background, or the window lost focus.
    // The window never saw the contact's id and cannot name it, so this is
    // how it says the gesture is over.
    let out = d.input(Input::Unfocused);
    assert!(events(&out, EventKind::Click).is_empty(), "{out:?}");
    // The finger that comes back is a new gesture. Were the old contact
    // still remembered, this `TouchDown` would be taken for a second finger
    // and ignored — and the button would never work again.
    d.input(Input::TouchDown(1, 100.0, 40.0));
    let out = d.input(Input::TouchUp(1, 100.0, 40.0));
    assert_eq!(events(&out, EventKind::Click), vec![1], "{out:?}");
}

// ------------------------------------------------------- the held contact

/// A scroller of rows that can be picked up: the tree §5.1 exists for. Row 0
/// also carries `long_press`, so the two outcomes of a hold sit side by side.
fn draggable_rows() -> Driver {
    const A_DRAG: u32 = 20;
    const A_ACCEPTS: u32 = 21;
    const A_GRAB: u32 = 22;
    const A_LONG: u32 = 23;
    let mut d = welcomed();
    let mut tree = Subtree::default();
    // The scroller is wrapped, as in `scroller()`: a root node fills the
    // window, and a scroller as tall as the window has nothing to scroll.
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 10, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.props.push((A_ACCEPTS, Value::Str("row".into())));
    tree.nodes.push(FlatNode { kind: NodeKind::Scroll, id: VIEW, style: 11, key: 0, text: None, props: (0, 1), handlers: (0, 3), child_count: 10 });
    tree.handlers.push((EventKind::Scroll, Handler::Server(ATOM)));
    tree.handlers.push((EventKind::DragOver, Handler::Server(A_GRAB)));
    tree.handlers.push((EventKind::Drop, Handler::Server(A_GRAB)));
    for i in 0..10u32 {
        let p0 = tree.props.len() as u32;
        let h0 = tree.handlers.len() as u32;
        // The last row carries `long_press` and no `drag`: §5.1's other
        // outcome needs somewhere to happen.
        if i == 9 {
            tree.handlers.push((EventKind::LongPress, Handler::Server(A_LONG)));
            // …and a `pointer_up`, so that the press being given back is
            // something the test can see rather than infer.
            tree.handlers.push((EventKind::PointerUp, Handler::Server(A_LONG)));
            tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 10 + i, style: 12, key: 100 + i, text: None, props: (p0, 0), handlers: (h0, 2), child_count: 0 });
            continue;
        }
        tree.props.push((A_DRAG, Value::Str("row".into())));
        tree.handlers.push((EventKind::DragStart, Handler::Server(A_GRAB)));
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 10 + i, style: 12, key: 100 + i, text: None, props: (p0, 1), handlers: (h0, 1), child_count: 0 });
    }
    let mut ops = vec![
        Op::DefAtom { id: ATOM, value: "acted".into() },
        Op::DefAtom { id: A_DRAG, value: "drag".into() },
        Op::DefAtom { id: A_ACCEPTS, value: "accepts".into() },
        Op::DefAtom { id: A_GRAB, value: "grabbed".into() },
        Op::DefAtom { id: A_LONG, value: "held".into() },
        Op::DefStyle { id: 10, record: StyleRecord { display: Display::Column, ..Default::default() } },
        Op::DefStyle { id: 11, record: StyleRecord { display: Display::Column, height: Dim::Px(100), width: Dim::Px(200), ..Default::default() } },
        Op::DefStyle { id: 12, record: StyleRecord { height: Dim::Px(22), width: Dim::Px(200), ..Default::default() } },
    ];
    for i in 0..10u32 {
        ops.push(Op::DefAtom { id: 100 + i, value: format!("row-{i}") });
    }
    ops.push(Op::Mount(tree));
    assert_eq!(d.handle_frame(Frame::Batch(Batch { seq: 1, ops })), vec![Frame::Ack { seq: 1 }]);
    let _ = d.paint(400, 300);
    d
}

/// The baseline §5.1 must not break, and the reason a draggable node carries a
/// prop rather than a `pointer_move` handler: a stroke down a list of rows
/// that can all be picked up still scrolls the list.
#[test]
fn a_stroke_on_a_row_that_can_be_picked_up_still_scrolls_the_list() {
    let mut d = draggable_rows();
    let t = Instant::now();
    let mut out = d.input_at(Input::TouchDown(1, 100.0, 50.0), t);
    out.extend(d.input_at(Input::TouchMove(1, 100.0, 20.0), t + Duration::from_millis(30)));
    assert!(events(&out, EventKind::DragStart).is_empty(), "nothing was picked up");
    assert!(offset(&d) > 0, "and the view came with the finger");
}

/// §5.1 outcome 3, first case: held still on a row that can be picked up, it
/// is picked up. The press is not given back — the lift will be the drop.
#[test]
fn a_contact_held_on_a_row_picks_it_up() {
    let mut d = draggable_rows();
    let t = Instant::now();
    let mut out = d.input_at(Input::TouchDown(1, 100.0, 50.0), t);
    // Half a second of holding still, as a clock the test names rather than
    // a sleep: two frames either side of the deadline.
    d.tick(t + Duration::from_millis(400));
    let _ = d.paint(400, 300);
    out.extend(d.take_pending());
    assert!(events(&out, EventKind::DragStart).is_empty(), "not yet");
    d.tick(t + Duration::from_millis(520));
    let _ = d.paint(400, 300);
    out.extend(d.take_pending());
    assert_eq!(events(&out, EventKind::DragStart), vec![12], "the row under the finger was grabbed");
    assert!(events(&out, EventKind::PointerUp).is_empty(), "and the press was not given back");

    // From here the finger carries the row rather than the view.
    let was = offset(&d);
    let on = d.input_at(Input::TouchMove(1, 100.0, 20.0), t + Duration::from_millis(560));
    assert!(!events(&on, EventKind::DragOver).is_empty(), "it moves the row");
    assert_eq!(offset(&d), was, "and not the view");
    let up = d.input_at(Input::TouchUp(1, 100.0, 20.0), t + Duration::from_millis(600));
    assert_eq!(events(&up, EventKind::Drop).len(), 1, "and the lift is the drop");
    assert!(events(&up, EventKind::Click).is_empty(), "never a click");
}

/// §5.1 outcome 1: a contact that wandered first is a scroll, and the clock
/// it was carrying is forgotten rather than left to fire later.
#[test]
fn a_contact_that_wandered_is_a_scroll_with_no_hold_left_to_fire() {
    let mut d = draggable_rows();
    let t = Instant::now();
    let mut out = d.input_at(Input::TouchDown(1, 100.0, 50.0), t);
    out.extend(d.input_at(Input::TouchMove(1, 100.0, 20.0), t + Duration::from_millis(20)));
    d.tick(t + Duration::from_millis(700));
    let _ = d.paint(400, 300);
    out.extend(d.take_pending());
    assert!(events(&out, EventKind::DragStart).is_empty(), "the hold did not fire late");
    assert!(events(&out, EventKind::LongPress).is_empty());
}

/// §5.1 outcome 3, second case: the reserved kind finally gets its use, on a
/// node that cannot be picked up. The press *is* given back here — a long
/// press that opened a menu must not also activate what it opened from — so
/// this is the one route out of a hold that does.
#[test]
fn a_contact_held_on_a_long_press_handler_reports_it_and_lets_go() {
    let mut d = draggable_rows();
    let t = Instant::now();
    // The tenth row, at 22 px each: the one carrying `long_press` alone. It
    // is below the fold, so scroll to it first — with a raw delta, not a
    // notch, because a notch glides and a glide is hit-tested where it is
    // drawn rather than where it will land.
    let _ = d.input_at(Input::Wheel(0.0, 200.0), t);
    let _ = d.paint(400, 300);
    let at = d.layout().rect(d.session().lookup(19).unwrap()).unwrap();
    let _ = d.input_at(Input::TouchDown(1, at.x + 10.0, at.y + 10.0), t);
    d.tick(t + Duration::from_millis(520));
    let _ = d.paint(400, 300);
    let out = d.take_pending();
    assert_eq!(events(&out, EventKind::LongPress), vec![19], "the hold was reported");
    assert!(events(&out, EventKind::DragStart).is_empty(), "nothing was picked up");
    assert_eq!(events(&out, EventKind::PointerUp), vec![19], "and the press was given back");
    assert!(events(&out, EventKind::Click).is_empty(), "without a click");
}

/// §5.1's order: a node that can be picked up *and* holds a `long_press` is
/// picked up. Only one of the two can happen, and the drag is the one the
/// person was reaching for.
#[test]
fn a_row_that_can_be_picked_up_is_picked_up_rather_than_reported() {
    let mut d = draggable_rows();
    let t = Instant::now();
    let _ = d.input_at(Input::TouchDown(1, 100.0, 10.0), t);
    d.tick(t + Duration::from_millis(520));
    let _ = d.paint(400, 300);
    let out = d.take_pending();
    assert_eq!(events(&out, EventKind::DragStart), vec![10]);
    assert!(events(&out, EventKind::LongPress).is_empty(), "and the menu is not opened as well");
}

/// A mouse runs no clock: a held button is not a gesture, and `context_menu`
/// is already what the second button means.
#[test]
fn a_held_mouse_button_is_not_a_long_press() {
    let mut d = draggable_rows();
    let t = Instant::now();
    let mut out = d.input_at(Input::PointerMove(100.0, 50.0), t);
    out.extend(d.input_at(Input::PointerDown(0), t));
    d.tick(t + Duration::from_millis(900));
    let _ = d.paint(400, 300);
    out.extend(d.take_pending());
    assert!(events(&out, EventKind::DragStart).is_empty(), "nothing was grabbed by waiting");
    assert!(events(&out, EventKind::LongPress).is_empty());
}
