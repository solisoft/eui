//! Spec 01 §2.1: what the person allows, asked before anything is dialled.
//!
//! No window and no network here. The sheet is a tree the client mounts
//! itself, so the whole of it — what is offered, what a click does to it,
//! and what mask comes out the other end — is the driver's, and testable
//! without either.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_client::{Driver, Input};
use eui_proto::caps;

/// A driver with nothing in it, as one is before a session opens.
fn driver() -> Driver {
    Driver::new(500.0, 400.0, 1.0, 0)
}

/// Click the middle of a node by id.
fn click(d: &mut Driver, id: u32) {
    let _ = d.paint(500, 400);
    let ix = d.session().lookup(id).expect("the sheet has no such node");
    let r = d.layout().rect(ix).expect("the node was not laid out");
    let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let _ = d.input(Input::PointerMove(x, y));
    let _ = d.input(Input::PointerDown(0));
    let _ = d.input(Input::PointerUp(0));
}

/// The two answers, by the ids the sheet gives them.
const ALLOW: u32 = 10;
const DENY: u32 = 11;
/// The first row. They run upward from here in `caps::NAMES` order.
const ROW: u32 = 100;

#[test]
fn nothing_asked_is_answered_at_once_and_nothing_is_mounted() {
    // A session that wants no capability must not stop to ask about none
    // of them — that would be a window that shows a question with no rows
    // in it and will not open until it is dismissed.
    let mut d = driver();
    d.ask_consent(0, "Meridian");
    assert!(!d.asking_consent());
    assert_eq!(d.take_consent(), Some(0));
    assert_eq!(d.take_consent(), None, "and taken once");
}

#[test]
fn allowing_grants_everything_that_was_asked_for() {
    let mut d = driver();
    let asked = caps::FS_PICK | caps::CLIPBOARD_READ | caps::CAMERA;
    d.ask_consent(asked, "Meridian");
    assert!(d.asking_consent());
    assert_eq!(d.take_consent(), None, "nothing is decided by the sheet going up");
    click(&mut d, ALLOW);
    assert_eq!(d.take_consent(), Some(asked));
    assert!(!d.asking_consent(), "and the sheet is done with");
}

/// The same answer, on the window the machine actually has.
///
/// Every other test here runs at 500x400 and scale 1.0, where a pointer's
/// coordinates and the layout's are the same numbers — so a unit mismatch
/// anywhere between the two would be invisible in all of them. This one is a
/// desktop window on a 1.5x display, with the seven capabilities the demo
/// application asks for, because that is the configuration a person reported
/// being unable to answer.
#[test]
fn the_sheet_can_be_answered_on_a_hidpi_window() {
    let mut d = Driver::new(820.0, 927.0, 1.5, 0);
    let asked = caps::CAMERA | caps::MICROPHONE | caps::CLIPBOARD_READ | caps::LOCATION | caps::FS_PICK | caps::NFC | caps::SCENE;
    d.ask_consent(asked, "demo-app");
    assert!(d.asking_consent());

    let _ = d.paint(1230, 1390);
    let ix = d.session().lookup(ALLOW).expect("the sheet has no Allow");
    let r = d.layout().rect(ix).expect("Allow was not laid out");
    let (x, y) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    let _ = d.input(Input::PointerMove(x, y));
    let _ = d.input(Input::PointerDown(0));
    let _ = d.input(Input::PointerUp(0));

    assert_eq!(d.take_consent(), Some(asked), "Allow at {x},{y} did not answer the sheet");
    assert!(!d.asking_consent());
}

/// Answering the sheet must leave a session an application can start in.
///
/// The sheet is a tree like any other and it is mounted into the session. It
/// used to stay there: the window dialled, the application's first batch
/// arrived on top of it, and the client refused its own session —
/// `the resynced tree was refused`, then `resync refused`, then closed. So
/// saying yes was what broke the session the question was asked for, and the
/// only way through was `--allow`, which works by never asking and so never
/// mounting anything.
#[test]
fn a_session_can_start_once_the_sheet_has_been_answered() {
    use eui_proto::{AlignItems, Batch, Display, FlatNode, Frame, NodeKind, Op, StyleRecord, Subtree, TextRef, Welcome};

    let mut d = driver();
    d.ask_consent(caps::CAMERA | caps::FS_PICK, "demo-app");
    click(&mut d, ALLOW);
    assert_eq!(d.take_consent(), Some(caps::CAMERA | caps::FS_PICK));

    // What the window does next: dial, and the server says hello.
    assert!(d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [7; 16], resumed: false })).is_empty());

    // An application's first batch, using the same low ids the sheet did.
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 0, key: 0, text: Some(TextRef::Inline("the application".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    let batch = Batch { seq: 1, ops: vec![Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, align_items: AlignItems::Start, ..Default::default() } }, Op::Mount(tree)] };
    let out = d.handle_frame(Frame::Batch(batch));

    assert_eq!(out, vec![Frame::Ack { seq: 1 }], "the first batch after a grant was refused: {out:?}");
    assert!(d.closed().is_none(), "the session closed: {:?}", d.closed());
}

#[test]
fn refusing_grants_nothing() {
    let mut d = driver();
    d.ask_consent(caps::FS_PICK | caps::CAMERA, "Meridian");
    click(&mut d, DENY);
    assert_eq!(d.take_consent(), Some(0));
}

/// The whole reason the rows are rows: an application that wants a camera
/// and a file picker must not be able to make somebody grant the camera in
/// order to drop a CSV on it.
#[test]
fn one_row_can_be_turned_off_without_the_others() {
    let mut d = driver();
    // In `caps::NAMES` order: camera, clipboard.read, fs.pick.
    let asked = caps::CAMERA | caps::CLIPBOARD_READ | caps::FS_PICK;
    d.ask_consent(asked, "Meridian");
    click(&mut d, ROW);
    click(&mut d, ALLOW);
    assert_eq!(d.take_consent(), Some(caps::CLIPBOARD_READ | caps::FS_PICK));
}

#[test]
fn a_row_turned_off_and_on_again_is_where_it_started() {
    let mut d = driver();
    let asked = caps::CAMERA | caps::FS_PICK;
    d.ask_consent(asked, "Meridian");
    click(&mut d, ROW);
    click(&mut d, ROW);
    click(&mut d, ALLOW);
    assert_eq!(d.take_consent(), Some(asked));
}

#[test]
fn every_row_off_is_the_same_as_refusing() {
    let mut d = driver();
    d.ask_consent(caps::CAMERA | caps::FS_PICK, "Meridian");
    click(&mut d, ROW);
    click(&mut d, ROW + 1);
    click(&mut d, ALLOW);
    assert_eq!(d.take_consent(), Some(0));
}

/// The rows stand for the bits that were asked about, in bit order — and
/// only those. A capability the manifest never named has no row and cannot
/// be granted by clicking one.
#[test]
fn there_is_a_row_for_each_capability_asked_about_and_no_others() {
    let mut d = driver();
    d.ask_consent(caps::CAMERA | caps::FS_PICK, "Meridian");
    let _ = d.paint(500, 400);
    assert!(d.session().lookup(ROW).is_some(), "camera");
    assert!(d.session().lookup(ROW + 1).is_some(), "fs.pick");
    assert!(d.session().lookup(ROW + 2).is_none(), "and nothing else");
}

/// A bit outside 01 §2.1 is not a capability, and a sheet that showed a row
/// for one would be offering something it could not name.
#[test]
fn a_bit_that_is_not_a_capability_is_not_asked_about() {
    let mut d = driver();
    d.ask_consent(!caps::ALL, "Meridian");
    assert!(!d.asking_consent());
    assert_eq!(d.take_consent(), Some(0));
}

#[test]
fn the_sheet_can_be_answered_from_the_keyboard() {
    // The one page a person may reach before they have decided to trust
    // the application at all, so it had better not need a mouse.
    let mut d = driver();
    d.ask_consent(caps::FS_PICK, "Meridian");
    let _ = d.paint(500, 400);
    // Tab past the one row to Allow, and press it.
    for _ in 0..2 {
        let _ = d.input(Input::Key { key: "Tab".into(), modifiers: 0, down: true });
        let _ = d.input(Input::Key { key: "Tab".into(), modifiers: 0, down: false });
    }
    let _ = d.input(Input::Key { key: "Enter".into(), modifiers: 0, down: true });
    let _ = d.input(Input::Key { key: "Enter".into(), modifiers: 0, down: false });
    assert_eq!(d.take_consent(), Some(caps::FS_PICK));
}
