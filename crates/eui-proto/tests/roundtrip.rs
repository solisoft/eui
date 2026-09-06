// The workspace denies indexing, unwrapping and unchecked arithmetic because
// the *decode path* must not panic on hostile input. A test harness is the one
// place where a panic is the correct outcome — a test that panics is a test
// that failed — so the strict set is lifted here and nowhere else.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

//! Encode → decode → compare, for every construct in the wire format.
//!
//! A round-trip test is weak on its own: two symmetric bugs cancel. So the
//! byte-level assertions in `vectors.rs` pin the actual encoding, and these
//! only check that the two halves agree.

use eui_proto::*;

fn node(kind: NodeKind, id: u32, children: u32) -> FlatNode {
    FlatNode {
        kind,
        id,
        style: 1,
        key: 0,
        text: None,
        props: (0, 0),
        handlers: (0, 0),
        child_count: children,
    }
}

fn roundtrip(frame: &Frame) {
    let bytes = frame.encode();
    let back = Frame::decode(&bytes).expect("decode");
    assert_eq!(&back, frame);
}

#[test]
fn every_frame_kind() {
    roundtrip(&Frame::Hello(Hello {
        version: PROTOCOL_VERSION,
        viewport: Viewport {
            width: 1280,
            height: 800,
            scale: 200,
            mode: ThemeMode::Dark,
            density: Density::Compact,
            font_scale: 125,
        },
        granted: caps::CLIPBOARD_WRITE | caps::NOTIFICATIONS,
    }));
    roundtrip(&Frame::Welcome(Welcome { version: 1, session: [7; 16] }));
    roundtrip(&Frame::Ack { seq: u64::MAX });
    roundtrip(&Frame::Ping([1, 2, 3, 4, 5, 6, 7, 8]));
    roundtrip(&Frame::Pong([0; 8]));
    roundtrip(&Frame::Error { code: 42, message: "nope".into() });
    roundtrip(&Frame::Resync);
    roundtrip(&Frame::Viewport(Viewport::default()));
    roundtrip(&Frame::Event(EventFrame {
        node: 9,
        event: EventKind::Click,
        name: 3,
        payload: Value::Null,
    }));
}

#[test]
fn every_op() {
    let mut tree = Subtree::default();
    tree.nodes.push(node(NodeKind::Text, 1, 0));

    let ops = vec![
        Op::DefAtom { id: 1, value: "compteur".into() },
        Op::DefStyle { id: 1, record: StyleRecord::default() },
        Op::DefColor { id: 1, rgba: 0xFF5722FF },
        Op::DefChunk { id: 1, hash: [3; 32] },
        Op::Mount(tree.clone()),
        Op::Replace { node: 1, subtree: tree.clone() },
        Op::SetStyle { node: 1, style: 4 },
        Op::SetText { node: 1, text: TextRef::Inline("42".into()) },
        Op::SetProp { node: 1, prop: 2, value: Value::Int(-7) },
        Op::InsertChild { parent: 1, index: 0, subtree: tree },
        Op::RemoveChild { parent: 1, index: 2, count: 3 },
        Op::MoveChild { parent: 1, from: 5, to: 0 },
        Op::SetHandler {
            node: 1,
            event: EventKind::Click,
            handler: Handler::LocalThenServer { chunk: 1, name: 2 },
        },
        Op::ClearHandler { node: 1, event: EventKind::Blur },
        Op::Focus { node: 1 },
        Op::ScrollTo { node: 1, x: -120, y: 4096 },
    ];
    roundtrip(&Frame::Batch(Batch { seq: 1, ops }));
}

#[test]
fn every_value_variant() {
    let values = vec![
        Value::Null,
        Value::Bool(true),
        Value::Bool(false),
        Value::Int(i64::MIN),
        Value::Int(i64::MAX),
        Value::Float(-0.5),
        Value::Atom(9),
        Value::Str("é→𝄞".into()),
        Value::Asset([0xAB; 32]),
        Value::Color(ColorRef::literal(3)),
        Value::List(vec![Value::Int(1), Value::List(vec![Value::Null])]),
    ];
    let ops = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| Op::SetProp { node: 1, prop: i as u32 + 1, value })
        .collect();
    roundtrip(&Frame::Batch(Batch { seq: 2, ops }));
}

#[test]
fn style_record_is_exactly_64_bytes() {
    let mut w = Writer::new();
    StyleRecord::default().encode(&mut w);
    assert_eq!(w.len(), limits::STYLE_RECORD_BYTES);
}

#[test]
fn full_style_record_survives() {
    let record = StyleRecord {
        display: Display::Grid,
        wrap: Wrap::WrapReverse,
        justify: Justify::Evenly,
        align_items: AlignItems::Baseline,
        align_self: AlignSelf::Center,
        grow: 3,
        shrink: 0,
        gap: 4,
        basis: Dim::Fr(150),
        width: Dim::Percent(5000),
        height: Dim::Px(48),
        min_width: Dim::Space(3),
        min_height: Dim::Auto,
        max_width: Dim::Px(u16::MAX),
        max_height: Dim::Auto,
        padding: [1, 2, 3, 4],
        margin: [4, 3, 2, 1],
        bg: ColorRef::role(12),
        fg: ColorRef::literal(1),
        border_color: ColorRef::NONE,
        border_width: [1, 1, 1, 1],
        radius: 2,
        shadow: 1,
        opacity: 128,
        font_family: FontFamily::Mono,
        font_size: 5,
        font_weight: FontWeight::Bold,
        text_align: TextAlign::Center,
        line_clamp: 2,
        text_decoration: 0b11,
        overflow: Overflow::Scroll,
        position: Position::Absolute,
        z: 9,
        cursor: Cursor::NotAllowed,
    };
    let mut w = Writer::new();
    record.encode(&mut w);
    let mut r = Reader::new(w.as_slice());
    assert_eq!(StyleRecord::decode(&mut r).unwrap(), record);
    r.finish().unwrap();
}

#[test]
fn nested_tree_shape_survives() {
    // box(1) [ box(2) [ text(3) ], text(4) ]
    let mut tree = Subtree::default();
    tree.nodes.push(node(NodeKind::Box, 1, 2));
    tree.nodes.push(node(NodeKind::Box, 2, 1));
    tree.nodes.push(node(NodeKind::Text, 3, 0));
    tree.nodes.push(node(NodeKind::Text, 4, 0));

    let mut w = Writer::new();
    tree.encode(&mut w);
    let mut r = Reader::new(w.as_slice());
    let back = Subtree::decode(&mut r).unwrap();
    r.finish().unwrap();
    assert_eq!(back, tree);
}

#[test]
fn props_and_handlers_keep_their_owners() {
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode {
        kind: NodeKind::Box,
        id: 1,
        style: 1,
        key: 0,
        text: None,
        props: (0, 1),
        handlers: (0, 0),
        child_count: 2,
    });
    tree.props.push((10, Value::Bool(true)));
    tree.nodes.push(FlatNode {
        kind: NodeKind::Text,
        id: 2,
        style: 2,
        key: 77,
        text: Some(TextRef::Atom(5)),
        props: (1, 0),
        handlers: (0, 1),
        child_count: 0,
    });
    tree.handlers.push((EventKind::Click, Handler::Local(1)));
    tree.nodes.push(FlatNode {
        kind: NodeKind::Text,
        id: 3,
        style: 2,
        key: 78,
        text: Some(TextRef::Inline("x".into())),
        props: (1, 2),
        handlers: (1, 0),
        child_count: 0,
    });
    tree.props.push((11, Value::Int(1)));
    tree.props.push((12, Value::Null));

    let mut w = Writer::new();
    tree.encode(&mut w);
    let back = Subtree::decode(&mut Reader::new(w.as_slice())).unwrap();

    assert_eq!(back.props_of(&back.nodes[0]), &[(10, Value::Bool(true))]);
    assert_eq!(back.handlers_of(&back.nodes[1]), &[(EventKind::Click, Handler::Local(1))]);
    assert_eq!(back.props_of(&back.nodes[2]).len(), 2);
    assert_eq!(back.nodes[2].key, 78);
}

#[test]
fn varints_are_minimal_and_reversible() {
    for v in [0u64, 1, 127, 128, 300, 16_383, 16_384, u32::MAX as u64, u64::MAX] {
        let mut w = Writer::new();
        w.varint(v);
        let mut r = Reader::new(w.as_slice());
        assert_eq!(r.varint().unwrap(), v, "value {v}");
        r.finish().unwrap();
    }
    for v in [0i64, -1, 1, i64::MIN, i64::MAX, -300, 300] {
        let mut w = Writer::new();
        w.svarint(v);
        assert_eq!(Reader::new(w.as_slice()).svarint().unwrap(), v, "value {v}");
    }
}

#[test]
fn a_deep_but_legal_tree_is_accepted() {
    // 257 nodes is 256 pushes onto the depth stack: the last legal shape.
    let mut w = Writer::new();
    for i in 1..=257u32 {
        let children = u32::from(i < 257);
        w.u8(NodeKind::Box.to_u8()).u8(0).varint32(i).varint32(1).varint32(children);
    }
    let tree = Subtree::decode(&mut Reader::new(w.as_slice())).unwrap();
    assert_eq!(tree.nodes.len(), 257);
}
