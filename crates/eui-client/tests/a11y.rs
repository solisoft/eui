//! Spec 03 §6, the declared half: a node says what it is in its props, and the
//! client prefers that to what it would have inferred from the node's kind.
//!
//! The kind-mapping baseline lives in `driver.rs` and is deliberately left
//! alone — a tree that declares nothing must still be exposed exactly as it
//! was before any of this existed, and that test is what says so.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]
#![cfg(feature = "a11y")]

use eui_client::a11y::{AccessRole as Role, AccessSnapshot, Checked};
use eui_client::{Driver, Input};
use eui_proto::*;

/// A root box holding one child, whose props are whatever the test declares.
/// The child is a `Box` with a click handler, so without any declaration it
/// would be a `Button` — which is what makes the overrides visible.
fn tree_with(props: Vec<(u32, Value)>, atoms: &[&str], clickable: bool, kind: NodeKind, text: Option<&str>) -> Batch {
    let mut tree = Subtree::default();
    let n = u32::try_from(props.len()).unwrap();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    // A box holds its words in a text child; only an editable node carries
    // text of its own. Giving it both would name it twice.
    let boxed = kind == NodeKind::Box && text.is_some();
    tree.nodes.push(FlatNode {
        kind,
        id: 2,
        style: 0,
        key: 0,
        text: if boxed { None } else { text.map(|t| TextRef::Inline(t.into())) },
        props: (0, n),
        handlers: (0, u32::from(clickable)),
        child_count: u32::from(boxed),
    });
    if clickable {
        tree.handlers.push((EventKind::Click, Handler::Server(1)));
    }
    if let (true, Some(t)) = (boxed, text) {
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 3, style: 0, key: 0, text: Some(TextRef::Inline(t.into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    }
    tree.props = props;
    let mut ops: Vec<Op> = atoms.iter().enumerate().map(|(i, a)| Op::DefAtom { id: u32::try_from(i).unwrap() + 1, value: (*a).to_owned() }).collect();
    ops.push(Op::DefStyle { id: 1, record: StyleRecord { display: Display::Column, padding: [6; 4], ..Default::default() } });
    ops.push(Op::Mount(tree));
    Batch { seq: 1, ops }
}

fn snap(batch: Batch) -> AccessSnapshot {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(batch));
    let _ = d.paint(400, 300);
    d.access_snapshot()
}

/// The one node under the root.
fn child(s: &AccessSnapshot) -> &eui_client::a11y::AccessNode {
    s.nodes.iter().find(|n| n.id == 2).expect("the declared node is in the tree")
}

#[test]
fn a_declared_role_wins_over_the_one_the_kind_would_infer() {
    let s = snap(tree_with(vec![(1, Value::Str("check_box".into())), (2, Value::Bool(true))], &["role", "checked"], true, NodeKind::Box, Some("Ship it")));
    let n = child(&s);
    assert_eq!(n.role, Role::CheckBox, "a clickable box would have been a Button");
    assert_eq!(n.state.checked, Some(Checked::Yes));
    assert_eq!(n.label, "Ship it", "still named by the text inside it");
}

#[test]
fn checked_carries_its_third_state() {
    let s = snap(tree_with(vec![(1, Value::Str("check_box".into())), (2, Value::Str("mixed".into()))], &["role", "checked"], true, NodeKind::Box, Some("Some")));
    assert_eq!(child(&s).state.checked, Some(Checked::Mixed));
}

#[test]
fn a_label_prop_overrides_the_text_inside() {
    let s = snap(tree_with(vec![(1, Value::Str("button".into())), (2, Value::Str("Close".into()))], &["role", "label"], true, NodeKind::Box, Some("\u{00d7}")));
    let n = child(&s);
    assert_eq!(n.label, "Close", "an icon button is not called by its glyph");
}

#[test]
fn a_disabled_control_keeps_its_role_and_accepts_nothing() {
    // No click handler at all: this is what disabling does in the catalogue.
    let s = snap(tree_with(vec![(1, Value::Str("button".into())), (2, Value::Bool(true))], &["role", "disabled"], false, NodeKind::Box, Some("Publish")));
    let n = child(&s);
    assert_eq!(n.role, Role::Button, "without the declaration it would be an unnamed group");
    assert_eq!(n.label, "Publish");
    assert!(n.state.disabled);
    assert!(!n.click, "and it is not activatable");
    assert!(!n.focus);
}

#[test]
fn a_container_role_keeps_its_children_where_a_leaf_would_swallow_them() {
    let s = snap(tree_with(vec![(1, Value::Str("tab_list".into())), (2, Value::Str("horizontal".into()))], &["role", "orientation"], true, NodeKind::Box, Some("Overview")));
    let n = child(&s);
    assert_eq!(n.role, Role::TabList);
    assert_eq!(n.children.len(), 1, "a tab list holds its tabs");
    assert_eq!(n.state.orientation, 1);
    // The old rule made anything clickable a childless Button.
    assert!(s.nodes.iter().any(|x| x.role == Role::Label), "the child is still exposed");
}

#[test]
fn a_set_says_how_big_it_is_even_when_virtualisation_left_rows_out() {
    let s = snap(tree_with(vec![(1, Value::Str("option".into())), (2, Value::Int(3)), (3, Value::Int(10_000))], &["role", "pos_in_set", "set_size"], true, NodeKind::Box, Some("Row")));
    let n = child(&s);
    assert_eq!(n.state.pos_in_set, 3);
    assert_eq!(n.state.set_size, 10_000, "the virtual count, not the rendered one");
}

#[test]
fn a_slider_carries_its_value_and_the_ends_of_its_range() {
    let s = snap(tree_with(
        vec![(1, Value::Str("slider".into())), (2, Value::Int(40)), (3, Value::Int(0)), (4, Value::Int(100))],
        &["role", "value_now", "value_min", "value_max"],
        true,
        NodeKind::Box,
        Some("Volume"),
    ));
    let n = child(&s);
    assert_eq!(n.role, Role::Slider);
    assert_eq!(n.state.value_now, Some(40.0));
    assert_eq!(n.state.value_min, Some(0.0));
    assert_eq!(n.state.value_max, Some(100.0));
}

#[test]
fn zero_is_a_value_a_slider_may_sit_at_and_not_an_absence() {
    let s = snap(tree_with(vec![(1, Value::Str("slider".into())), (2, Value::Int(0))], &["role", "value_now"], true, NodeKind::Box, Some("Volume")));
    assert_eq!(child(&s).state.value_now, Some(0.0), "present and zero, not absent");
}

#[test]
fn an_unknown_role_falls_back_to_the_kind() {
    let s = snap(tree_with(vec![(1, Value::Str("flux_capacitor".into()))], &["role"], true, NodeKind::Box, Some("Go")));
    let n = child(&s);
    assert_eq!(n.role, Role::Button, "a name this client has not learned is ignored, not fatal");
    assert_eq!(n.label, "Go");
}

#[test]
fn a_live_region_says_how_urgently_it_should_be_read() {
    let s = snap(tree_with(vec![(1, Value::Str("status".into())), (2, Value::Str("assertive".into()))], &["role", "live"], false, NodeKind::Box, Some("Saved")));
    let n = child(&s);
    assert_eq!(n.role, Role::Status);
    assert_eq!(n.state.live, 2);
}

#[test]
fn every_role_survives_the_worker_boundary() {
    // The snapshot crosses two pipes as bytes; a discriminant without a
    // `from_u8` arm would decode as an error and take the tree with it.
    for raw in 0..=u8::MAX {
        let Some(role) = Role::from_u8(raw) else { continue };
        assert_eq!(role as u8, raw, "role {raw} does not round-trip");
    }
    // And every name a server may write resolves to one of them.
    for name in [
        "button",
        "link",
        "check_box",
        "radio",
        "radio_group",
        "switch",
        "tab",
        "tab_list",
        "tab_panel",
        "menu",
        "menu_item",
        "menu_bar",
        "combo_box",
        "list_box",
        "option",
        "slider",
        "spin_button",
        "progress",
        "dialog",
        "alert_dialog",
        "alert",
        "status",
        "tooltip",
        "tree",
        "tree_item",
        "toolbar",
        "navigation",
        "table",
        "row",
        "cell",
        "grid",
        "grid_cell",
        "column_header",
        "heading",
        "separator",
        "group",
        "label",
        "image",
    ] {
        let role = Role::from_name(name).unwrap_or_else(|| panic!("{name} names no role"));
        assert_eq!(Role::from_u8(role as u8), Some(role), "{name} does not round-trip");
    }
}

#[test]
fn a_field_that_is_disabled_is_not_focusable() {
    let s = snap(tree_with(vec![(1, Value::Bool(true))], &["disabled"], false, NodeKind::Input, Some("a")));
    let n = child(&s);
    assert_eq!(n.role, Role::TextInput, "the kind still decides");
    assert!(n.state.disabled);
    assert!(!n.focus);
}

#[test]
fn an_assistive_technologys_click_still_reaches_the_handler() {
    let mut d = Driver::new(400.0, 300.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16] }));
    d.handle_frame(Frame::Batch(tree_with(vec![(1, Value::Str("check_box".into())), (2, Value::Bool(false))], &["role", "checked"], true, NodeKind::Box, Some("Ship it"))));
    let _ = d.paint(400, 300);
    let snapshot = d.access_snapshot();
    let id = child(&snapshot).id;
    let ix = d.node_for_accessibility(id).unwrap();
    let out = d.activate_node(ix);
    let events: Vec<EventKind> = out.iter().filter_map(|f| if let Frame::Event(e) = f { Some(e.event) } else { None }).collect();
    assert_eq!(events, vec![EventKind::Click], "declaring a role does not change what a press does");
    let _ = Input::Key { key: "a".into(), modifiers: 0, down: true };
}
