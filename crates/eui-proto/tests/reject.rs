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

//! Rejection cases.
//!
//! EUI has no error recovery and no quirks mode: a malformed frame ends the
//! session. That promise is only worth what these tests are worth, so every
//! MUST in `spec/02-wire-format.md` that can be violated by bytes on the wire
//! gets a case here.
//!
//! Each case asserts the *kind* of rejection, not merely that something went
//! wrong — an off-by-one that turns a limit check into a truncation would
//! otherwise pass unnoticed.

use eui_proto::error::DecodeError as E;
use eui_proto::limits::*;
use eui_proto::*;

// ---------------------------------------------------------------- helpers

fn framed(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut w = Writer::new();
    w.u8(kind).varint(payload.len() as u64).raw(payload);
    w.into_vec()
}

fn subtree_err(bytes: &[u8]) -> E {
    Subtree::decode(&mut Reader::new(bytes)).expect_err("should have been rejected")
}

fn op_err(bytes: &[u8]) -> E {
    Op::decode(&mut Reader::new(bytes)).expect_err("should have been rejected")
}

fn value_err(bytes: &[u8]) -> E {
    Value::decode(&mut Reader::new(bytes)).expect_err("should have been rejected")
}

fn frame_err(bytes: &[u8]) -> E {
    Frame::decode(bytes).expect_err("should have been rejected")
}

/// A minimal well-formed leaf node: kind, flags=0, id=1, style=1, 0 children.
fn leaf(kind: u8) -> Vec<u8> {
    vec![kind, 0x00, 0x01, 0x01, 0x00]
}

fn style_bytes() -> Vec<u8> {
    let mut w = Writer::new();
    StyleRecord::default().encode(&mut w);
    w.into_vec()
}

fn style_with(offset: usize, bytes: &[u8]) -> E {
    let mut raw = style_bytes();
    raw[offset..offset + bytes.len()].copy_from_slice(bytes);
    StyleRecord::decode(&mut Reader::new(&raw)).expect_err("should have been rejected")
}

// ------------------------------------------------------------- primitives

#[test]
fn varint_non_minimal_is_rejected() {
    assert_eq!(Reader::new(&[0x80, 0x00]).varint().unwrap_err(), E::BadVarint);
    assert_eq!(Reader::new(&[0x81, 0x80, 0x00]).varint().unwrap_err(), E::BadVarint);
}

#[test]
fn varint_overflow_is_rejected() {
    let overlong = [0x80u8; 11];
    assert_eq!(Reader::new(&overlong).varint().unwrap_err(), E::BadVarint);
    // 2^64 would need a 65th payload bit.
    let too_big = [0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x02];
    assert_eq!(Reader::new(&too_big).varint().unwrap_err(), E::BadVarint);
}

#[test]
fn varint_truncated_is_rejected() {
    assert_eq!(Reader::new(&[0x80]).varint().unwrap_err(), E::Truncated);
    assert_eq!(Reader::new(&[]).varint().unwrap_err(), E::Truncated);
}

#[test]
fn varint32_rejects_values_above_u32() {
    let mut w = Writer::new();
    w.varint(u64::from(u32::MAX) + 1);
    assert_eq!(Reader::new(w.as_slice()).varint32().unwrap_err(), E::BadVarint);
}

#[test]
fn non_finite_floats_are_rejected() {
    assert_eq!(Reader::new(&f64::NAN.to_le_bytes()).f64().unwrap_err(), E::IllegalValue("float must be finite"));
    assert_eq!(Reader::new(&f64::INFINITY.to_le_bytes()).f64().unwrap_err(), E::IllegalValue("float must be finite"));
}

// ------------------------------------------------------------------ frames

#[test]
fn empty_message_is_rejected() {
    assert_eq!(frame_err(&[]), E::Truncated);
}

#[test]
fn unknown_frame_kind_is_rejected() {
    assert_eq!(frame_err(&framed(0xFF, &[])), E::UnknownTag("frame kind"));
    assert_eq!(frame_err(&framed(0x00, &[])), E::UnknownTag("frame kind"));
}

#[test]
fn trailing_bytes_after_a_frame_are_rejected() {
    let mut bytes = framed(0x09, &[]); // Resync
    bytes.push(0xAA);
    assert_eq!(frame_err(&bytes), E::TrailingBytes);
}

#[test]
fn trailing_bytes_inside_a_payload_are_rejected() {
    assert_eq!(frame_err(&framed(0x09, &[0x01])), E::TrailingBytes);
}

#[test]
fn oversized_frame_length_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x03).varint(MAX_FRAME_BYTES as u64 + 1);
    assert_eq!(frame_err(w.as_slice()), E::LimitExceeded("frame length"));
}

#[test]
fn short_payload_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x03).varint(64).raw(&[0u8; 8]);
    assert_eq!(frame_err(w.as_slice()), E::Truncated);
}

#[test]
fn unknown_capability_bit_is_rejected() {
    let mut p = Writer::new();
    p.varint32(1);
    Viewport::default().encode(&mut p);
    p.varint32(caps::ALL | 0x100);
    assert_eq!(frame_err(&framed(0x01, p.as_slice())), E::IllegalValue("unknown capability bit"));
}

#[test]
fn unknown_theme_mode_and_density_are_rejected() {
    // width, height, scale, mode, density, font_scale
    let bad_mode = [0x00, 0x00, 100, 0, /*mode*/ 9, /*density*/ 1, 100, 0];
    assert_eq!(frame_err(&framed(0x0A, &bad_mode)), E::UnknownTag("theme mode"));
    let bad_density = [0x00, 0x00, 100, 0, 1, /*density*/ 9, 100, 0];
    assert_eq!(frame_err(&framed(0x0A, &bad_density)), E::UnknownTag("density"));
}

#[test]
fn unknown_event_kind_is_rejected() {
    let payload = [/*node*/ 1, /*event*/ 0xEE, /*name*/ 1, /*value*/ 0x00];
    assert_eq!(frame_err(&framed(0x04, &payload)), E::UnknownTag("event kind"));
}

// --------------------------------------------------------------------- ops

#[test]
fn unknown_opcode_is_rejected() {
    assert_eq!(op_err(&[0x00]), E::UnknownTag("opcode"));
    assert_eq!(op_err(&[0x2F]), E::UnknownTag("opcode"));
    assert_eq!(op_err(&[0xFF]), E::UnknownTag("opcode"));
}

#[test]
fn zero_table_ids_are_rejected() {
    assert_eq!(op_err(&[0x10, 0x00]), E::IllegalValue("atom id"));
    assert_eq!(op_err(&[0x11, 0x00]), E::IllegalValue("style id"));
    assert_eq!(op_err(&[0x12, 0x00]), E::IllegalValue("color id"));
    assert_eq!(op_err(&[0x13, 0x00]), E::IllegalValue("chunk id"));
}

#[test]
fn zero_node_ids_are_rejected() {
    for opcode in [0x21u8, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x2B] {
        assert_eq!(op_err(&[opcode, 0x00]), E::IllegalValue("node id"), "opcode {opcode:#04x}");
    }
}

#[test]
fn oversized_chunk_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x14).varint32(1).varint((MAX_CHUNK_BYTES + 1) as u64);
    assert_eq!(op_err(w.as_slice()), E::LimitExceeded("chunk bytes"));
    assert_eq!(op_err(&[0x14, 0x00]), E::IllegalValue("chunk id"));
}

#[test]
fn oversized_atom_value_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x10).varint32(1).varint((MAX_ATOM_BYTES + 1) as u64);
    assert_eq!(op_err(w.as_slice()), E::LimitExceeded("atom value"));
}

#[test]
fn bad_utf8_in_an_atom_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x10).varint32(1).bytes(&[0xFF, 0xFE]);
    assert_eq!(op_err(w.as_slice()), E::BadUtf8);
}

#[test]
fn oversized_batch_is_rejected() {
    let mut p = Writer::new();
    p.varint(1).varint32(MAX_OPS_PER_BATCH + 1);
    assert_eq!(frame_err(&framed(0x03, p.as_slice())), E::LimitExceeded("ops per batch"));
}

// ------------------------------------------------------------------- nodes

#[test]
fn unknown_node_kinds_are_rejected() {
    assert_eq!(subtree_err(&leaf(0x00)), E::UnknownTag("node kind"));
    assert_eq!(subtree_err(&leaf(0x0F)), E::UnknownTag("node kind"));
    assert_eq!(subtree_err(&leaf(0xFF)), E::UnknownTag("node kind"));
}

#[test]
fn reserved_node_flags_are_rejected() {
    for bit in [0x10u8, 0x20, 0x40, 0x80] {
        assert_eq!(
            subtree_err(&[0x01, bit, 0x01, 0x01, 0x00]),
            E::IllegalValue("reserved node flags set"),
            "flag {bit:#04x}"
        );
    }
}

#[test]
fn zero_node_id_in_a_subtree_is_rejected() {
    assert_eq!(
        subtree_err(&[0x01, 0x00, 0x00, 0x01, 0x00]),
        E::IllegalValue("node id must be non-zero")
    );
}

#[test]
fn leaf_kinds_may_not_have_children() {
    for kind in [0x02u8, 0x04, 0x0A, 0x0B] {
        assert_eq!(
            subtree_err(&[kind, 0x00, 0x01, 0x01, /*children*/ 0x01]),
            E::NotALeaf,
            "kind {kind:#04x}"
        );
    }
}

#[test]
fn inert_kinds_may_not_carry_content() {
    // spacer with text
    assert_eq!(
        subtree_err(&[0x0A, 0x02, 0x01, 0x01, 0x00, 0x01, 0x00]),
        E::IllegalValue("inert node kind carries content")
    );
    // divider with a handler
    assert_eq!(
        subtree_err(&[0x0B, 0x08, 0x01, 0x01, 0x01, 0x01, 0x00, 0x01, 0x00]),
        E::IllegalValue("inert node kind carries content")
    );
}

#[test]
fn too_many_children_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x01).u8(0x00).varint32(1).varint32(1).varint32(MAX_CHILDREN + 1);
    assert_eq!(subtree_err(w.as_slice()), E::LimitExceeded("children per node"));
}

#[test]
fn too_many_props_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x01).u8(0x04).varint32(1).varint32(1).varint32(MAX_PROPS + 1);
    assert_eq!(subtree_err(w.as_slice()), E::LimitExceeded("props per node"));
}

#[test]
fn too_many_handlers_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x01).u8(0x08).varint32(1).varint32(1).varint32(MAX_HANDLERS + 1);
    assert_eq!(subtree_err(w.as_slice()), E::LimitExceeded("handlers per node"));
}

#[test]
fn too_deep_a_tree_is_rejected() {
    // 258 nodes is one push past MAX_TREE_DEPTH.
    let mut w = Writer::new();
    for i in 1..=258u32 {
        let children = u32::from(i < 258);
        w.u8(0x01).u8(0x00).varint32(i).varint32(1).varint32(children);
    }
    assert_eq!(subtree_err(w.as_slice()), E::LimitExceeded("tree depth"));
}

#[test]
fn a_promised_child_that_never_arrives_is_rejected() {
    // Root claims one child, then the buffer ends.
    assert_eq!(subtree_err(&[0x01, 0x00, 0x01, 0x01, 0x01]), E::Truncated);
}

#[test]
fn unknown_textref_tag_is_rejected() {
    assert_eq!(
        subtree_err(&[0x02, 0x02, 0x01, 0x01, /*textref tag*/ 0x09]),
        E::UnknownTag("TextRef")
    );
}

#[test]
fn oversized_inline_string_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x02).u8(0x02).varint32(1).varint32(1).u8(0x01).varint((MAX_INLINE_STR + 1) as u64);
    assert_eq!(subtree_err(w.as_slice()), E::LimitExceeded("inline string"));
}

#[test]
fn unknown_handler_tag_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x01).u8(0x08).varint32(1).varint32(1).varint32(1).u8(0x01).u8(0x09);
    assert_eq!(subtree_err(w.as_slice()), E::UnknownTag("Handler"));
}

#[test]
fn unknown_event_in_a_handler_list_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x01).u8(0x08).varint32(1).varint32(1).varint32(1).u8(0xEE);
    assert_eq!(subtree_err(w.as_slice()), E::UnknownTag("event kind"));
}

// ------------------------------------------------------------------ values

#[test]
fn unknown_value_tag_is_rejected() {
    assert_eq!(value_err(&[0x09]), E::UnknownTag("Value"));
    assert_eq!(value_err(&[0xFF]), E::UnknownTag("Value"));
}

#[test]
fn non_canonical_bool_is_rejected() {
    assert_eq!(value_err(&[0x01, 0x02]), E::IllegalValue("bool must be 0 or 1"));
}

#[test]
fn non_finite_float_value_is_rejected() {
    let mut bytes = vec![0x03];
    bytes.extend_from_slice(&f64::NAN.to_le_bytes());
    assert_eq!(value_err(&bytes), E::IllegalValue("float must be finite"));
}

#[test]
fn over_nested_value_is_rejected() {
    // Five nested one-element lists; the limit is four.
    let mut w = Writer::new();
    for _ in 0..5 {
        w.u8(0x08).varint32(1);
    }
    w.u8(0x00);
    assert_eq!(value_err(w.as_slice()), E::LimitExceeded("value nesting"));
}

#[test]
fn oversized_value_list_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x08).varint32(MAX_VALUE_LIST + 1);
    assert_eq!(value_err(w.as_slice()), E::LimitExceeded("value list length"));
}

// ------------------------------------------------------------------ styles

#[test]
fn unknown_style_enums_are_rejected() {
    assert_eq!(style_with(0, &[5]), E::UnknownTag("display"));
    assert_eq!(style_with(1, &[3]), E::UnknownTag("wrap"));
    assert_eq!(style_with(2, &[6]), E::UnknownTag("justify"));
    assert_eq!(style_with(3, &[5]), E::UnknownTag("align_items"));
    assert_eq!(style_with(4, &[6]), E::UnknownTag("align_self"));
    assert_eq!(style_with(50, &[2]), E::UnknownTag("font_family"));
    assert_eq!(style_with(52, &[4]), E::UnknownTag("font_weight"));
    assert_eq!(style_with(53, &[4]), E::UnknownTag("text_align"));
    assert_eq!(style_with(56, &[3]), E::UnknownTag("overflow"));
    assert_eq!(style_with(57, &[2]), E::UnknownTag("position"));
    assert_eq!(style_with(59, &[9]), E::UnknownTag("cursor"));
}

#[test]
fn unknown_text_decoration_bits_are_rejected() {
    assert_eq!(style_with(55, &[0b100]), E::IllegalValue("text_decoration has unknown bits"));
}

#[test]
fn a_transition_past_the_motion_scale_is_rejected() {
    assert_eq!(style_with(60, &[4]), E::IllegalValue("transition is a motion index + 1, at most 3"));
}

#[test]
fn non_zero_reserved_bytes_are_rejected() {
    for i in 0..3 {
        let mut raw = style_bytes();
        raw[61 + i] = 1;
        assert_eq!(
            StyleRecord::decode(&mut Reader::new(&raw)).unwrap_err(),
            E::IllegalValue("reserved bytes must be zero"),
            "reserved byte {i}"
        );
    }
}

#[test]
fn malformed_dims_are_rejected() {
    // `basis` starts at offset 8: tag, then a little-endian u16.
    assert_eq!(style_with(8, &[0, 1, 0]), E::IllegalValue("Dim::Auto carries a value"));
    assert_eq!(style_with(8, &[5, 0, 0]), E::UnknownTag("Dim"));
    // tag 4 is a space index, which must fit in a u8.
    assert_eq!(style_with(8, &[4, 0x2C, 0x01]), E::IllegalValue("space index above 255"));
}

#[test]
fn a_truncated_style_record_is_rejected() {
    let raw = style_bytes();
    assert_eq!(
        StyleRecord::decode(&mut Reader::new(&raw[..STYLE_RECORD_BYTES - 1])).unwrap_err(),
        E::Truncated
    );
}

// ------------------------------------------------------------- hostile bulk

/// Arbitrary bytes must never panic, hang, or allocate without bound.
///
/// This is a cheap stand-in that runs on every `cargo test`; the real coverage
/// is `cargo fuzz`, but a regression that makes the decoder panic on short
/// input should not need a fuzzing session to notice.
#[test]
fn arbitrary_bytes_never_panic() {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let mut buf = Vec::with_capacity(256);
    for _ in 0..20_000 {
        buf.clear();
        let len = (next() % 192) as usize;
        for _ in 0..len {
            buf.push((next() & 0xFF) as u8);
        }
        // Every entry point, on the same bytes.
        let _ = Frame::decode(&buf);
        let _ = Subtree::decode(&mut Reader::new(&buf));
        let _ = Op::decode(&mut Reader::new(&buf));
        let _ = Value::decode(&mut Reader::new(&buf));
        let _ = StyleRecord::decode(&mut Reader::new(&buf));
    }
}

/// The same, but seeded with well-formed prefixes so the fuzzer-ish input gets
/// past the first tag byte and exercises the deeper paths.
#[test]
fn corrupted_valid_frames_never_panic() {
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
        text: Some(TextRef::Inline("hello".into())),
        props: (0, 0),
        handlers: (0, 0),
        child_count: 0,
    });
    let good = Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "label".into() },
            Op::DefStyle { id: 1, record: StyleRecord::default() },
            Op::Mount(tree),
        ],
    })
    .encode();

    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    for _ in 0..20_000 {
        let mut buf = good.clone();
        let flips = 1 + (next() % 4);
        for _ in 0..flips {
            let i = (next() as usize) % buf.len();
            buf[i] ^= 1 << (next() % 8);
        }
        let _ = Frame::decode(&buf);
    }
}
