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
