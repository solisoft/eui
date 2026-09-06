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

//! Byte-level vectors.
//!
//! Round-trip tests cannot catch a pair of symmetric bugs — an encoder and a
//! decoder that agree with each other and with nobody else. These pin the
//! actual bytes, so a second implementation written from `spec/02-wire-format.md`
//! alone can check itself against the same numbers.

use eui_proto::*;

/// The worked example from `spec/02-wire-format.md` §8: a column containing
/// the text "Hi".
#[test]
fn spec_section_8_worked_example() {
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode {
        kind: NodeKind::Box,
        id: 1,
        style: 1,
        key: 0,
        text: None,
        props: (0, 0),
        handlers: (0, 0),
        child_count: 1,
    });
    tree.nodes.push(FlatNode {
        kind: NodeKind::Text,
        id: 2,
        style: 2,
        key: 0,
        text: Some(TextRef::Atom(1)),
        props: (0, 0),
        handlers: (0, 0),
        child_count: 0,
    });

    let column = StyleRecord { display: Display::Column, padding: [4; 4], bg: ColorRef::role(1), ..Default::default() };
    let label = StyleRecord { font_size: 3, fg: ColorRef::role(8), ..Default::default() };

    let ops = vec![
        Op::DefAtom { id: 1, value: "Hi".into() },
        Op::DefStyle { id: 1, record: column },
        Op::DefStyle { id: 2, record: label },
        Op::Mount(tree),
    ];

    let mut w = Writer::new();
    for op in &ops {
        op.encode(&mut w);
    }
    let body = w.into_vec();

    // DefAtom 5 B + two DefStyle 66 B each + Mount 13 B.
    assert_eq!(body.len(), 150, "spec §8 body size");

    assert_eq!(&body[0..5], &[0x10, 0x01, 0x02, b'H', b'i'], "DefAtom");
    assert_eq!(&body[5..7], &[0x11, 0x01], "DefStyle 1 header");
    assert_eq!(&body[71..73], &[0x11, 0x02], "DefStyle 2 header");
    assert_eq!(
        &body[137..150],
        &[
            0x20, // Mount
            0x01, 0x00, 0x01, 0x01, 0x01, // box, flags 0, id 1, style 1, 1 child
            0x02, 0x02, 0x02, 0x02, 0x00, 0x01, 0x00, // text, has-text, id 2, style 2, atom 1, 0 children
        ],
        "Mount"
    );
}

/// The default style record, byte for byte. Every implementation must agree on
/// this or nothing else will line up.
#[test]
fn default_style_record_bytes() {
    let mut w = Writer::new();
    StyleRecord::default().encode(&mut w);
    let b = w.into_vec();

    #[rustfmt::skip]
    let expected: [u8; 64] = [
        0x00,                   // display: row
        0x00,                   // wrap: nowrap
        0x00,                   // justify: start
        0x03,                   // align_items: stretch
        0x05,                   // align_self: auto
        0x00,                   // grow
        0x01,                   // shrink
        0x00,                   // gap
        0x00, 0x00, 0x00,       // basis: auto
        0x00, 0x00, 0x00,       // width: auto
        0x00, 0x00, 0x00,       // height: auto
        0x00, 0x00, 0x00,       // min_width: auto
        0x00, 0x00, 0x00,       // min_height: auto
        0x00, 0x00, 0x00,       // max_width: auto
        0x00, 0x00, 0x00,       // max_height: auto
        0x00, 0x00, 0x00, 0x00, // padding
        0x00, 0x00, 0x00, 0x00, // margin
        0x00, 0x00,             // bg: none
        0x00, 0x00,             // fg: none
        0x00, 0x00,             // border_color: none
        0x00, 0x00, 0x00, 0x00, // border_width
        0x00,                   // radius
        0x00,                   // shadow
        0xFF,                   // opacity
        0x00,                   // font_family: sans
        0x00,                   // font_size
        0x00,                   // font_weight: regular
        0x00,                   // text_align: start
        0x00,                   // line_clamp
        0x00,                   // text_decoration
        0x00,                   // overflow: visible
        0x00,                   // position: flow
        0x00,                   // z
        0x00,                   // cursor: default
        0x00, 0x00, 0x00, 0x00, // reserved
    ];
    assert_eq!(b.as_slice(), expected.as_slice());
}

/// Varints, pinned. LEB128 is easy to get subtly wrong in a second
/// implementation, and a disagreement here corrupts everything downstream.
#[test]
fn varint_bytes() {
    let cases: &[(u64, &[u8])] = &[
        (0, &[0x00]),
        (1, &[0x01]),
        (127, &[0x7F]),
        (128, &[0x80, 0x01]),
        (300, &[0xAC, 0x02]),
        (16_383, &[0xFF, 0x7F]),
        (16_384, &[0x80, 0x80, 0x01]),
    ];
    for (value, bytes) in cases {
        let mut w = Writer::new();
        w.varint(*value);
        assert_eq!(w.as_slice(), *bytes, "varint {value}");
    }

    let signed: &[(i64, &[u8])] = &[
        (0, &[0x00]),
        (-1, &[0x01]),
        (1, &[0x02]),
        (-2, &[0x03]),
        (63, &[0x7E]),
        (-64, &[0x7F]),
    ];
    for (value, bytes) in signed {
        let mut w = Writer::new();
        w.svarint(*value);
        assert_eq!(w.as_slice(), *bytes, "svarint {value}");
    }
}

/// Frame envelopes, pinned.
#[test]
fn frame_envelope_bytes() {
    assert_eq!(Frame::Resync.encode(), vec![0x09, 0x00]);
    assert_eq!(Frame::Ack { seq: 300 }.encode(), vec![0x05, 0x02, 0xAC, 0x02]);
    assert_eq!(
        Frame::Ping([1, 2, 3, 4, 5, 6, 7, 8]).encode(),
        vec![0x06, 0x08, 1, 2, 3, 4, 5, 6, 7, 8]
    );
}

/// Colour references: the top bit picks role space or literal space.
#[test]
fn color_ref_encoding() {
    assert!(ColorRef::NONE.is_none());
    assert_eq!(ColorRef::role(12).0, 12);
    assert!(!ColorRef::role(12).is_literal());
    assert_eq!(ColorRef::literal(3).0, 0x8003);
    assert!(ColorRef::literal(3).is_literal());
    assert_eq!(ColorRef::literal(3).index(), 3);
}
