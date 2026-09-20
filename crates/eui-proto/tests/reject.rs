// The workspace denies indexing, unwrapping and unchecked arithmetic because
// the *decode path* must not panic on hostile input. A test harness is the one
// place where a panic is the correct outcome — a test that panics is a test
// that failed — so the strict set is lifted here and nowhere else.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

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
    // The lowest bit above the set, whatever the set has grown to: this
    // vector named `0x100` outright and went quiet the day that became
    // `nfc` — it still passed, against a frame that was now perfectly
    // legal and merely truncated.
    p.varint32(caps::ALL | (caps::ALL + 1));
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
fn a_font_role_without_a_face_is_rejected() {
    // A role bound to nothing is a style that names a face no op ever
    // supplies: the session would hold a binding it can never honour.
    assert_eq!(op_err(&[0x15, 0x02, 0x00]), E::IllegalValue("font faces"));
}

#[test]
fn a_font_role_past_the_last_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x15).u8(MAX_FONT_ROLE + 1).varint32(1).raw(&[0u8; HASH_BYTES]);
    assert_eq!(op_err(w.as_slice()), E::LimitExceeded("font role"));
}

#[test]
fn too_many_faces_on_one_role_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x15).u8(2).varint32(MAX_FACES_PER_ROLE + 1);
    assert_eq!(op_err(w.as_slice()), E::LimitExceeded("font faces"));
}

#[test]
fn a_truncated_face_hash_is_rejected() {
    let mut w = Writer::new();
    w.u8(0x15).u8(2).varint32(2).raw(&[7u8; HASH_BYTES]).raw(&[7u8; 8]);
    assert_eq!(op_err(w.as_slice()), E::Truncated);
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
    // 0x11 is `scene`, and decodes. The next one along does not: the set is
    // closed, and adding to it is a protocol version, which is the price
    // this test exists to keep charging.
    assert_eq!(subtree_err(&leaf(0x12)), E::UnknownTag("node kind"));
    assert_eq!(subtree_err(&leaf(0xFF)), E::UnknownTag("node kind"));
}

#[test]
fn reserved_node_flags_are_rejected() {
    for bit in [0x10u8, 0x20, 0x40, 0x80] {
        assert_eq!(subtree_err(&[0x01, bit, 0x01, 0x01, 0x00]), E::IllegalValue("reserved node flags set"), "flag {bit:#04x}");
    }
}

#[test]
fn zero_node_id_in_a_subtree_is_rejected() {
    assert_eq!(subtree_err(&[0x01, 0x00, 0x00, 0x01, 0x00]), E::IllegalValue("node id must be non-zero"));
}

#[test]
fn leaf_kinds_may_not_have_children() {
    for kind in [0x02u8, 0x04, 0x0A, 0x0B] {
        assert_eq!(subtree_err(&[kind, 0x00, 0x01, 0x01, /*children*/ 0x01]), E::NotALeaf, "kind {kind:#04x}");
    }
}

#[test]
fn inert_kinds_may_not_carry_content() {
    // spacer with text
    assert_eq!(subtree_err(&[0x0A, 0x02, 0x01, 0x01, 0x00, 0x01, 0x00]), E::IllegalValue("inert node kind carries content"));
    // divider with a handler
    assert_eq!(subtree_err(&[0x0B, 0x08, 0x01, 0x01, 0x01, 0x01, 0x00, 0x01, 0x00]), E::IllegalValue("inert node kind carries content"));
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
    assert_eq!(subtree_err(&[0x02, 0x02, 0x01, 0x01, /*textref tag*/ 0x09]), E::UnknownTag("TextRef"));
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
    assert_eq!(style_with(52, &[4]), E::UnknownTag("font_weight"));
    assert_eq!(style_with(53, &[4]), E::UnknownTag("text_align"));
    assert_eq!(style_with(56, &[3]), E::UnknownTag("overflow"));
    assert_eq!(style_with(57, &[3]), E::UnknownTag("position"));
    assert_eq!(style_with(59, &[9]), E::UnknownTag("cursor"));
}

/// `font_family` is the one style byte with an open half: `2..=255` are the
/// application's font roles (02 §5), so a byte the client does not recognise
/// is a role it has not been told about, not a malformed record. The tree
/// checks the binding; the text engine falls back to sans for a role that
/// has none.
#[test]
fn a_font_role_is_not_an_unknown_tag() {
    let mut raw = style_bytes();
    raw[50] = 2;
    let record = StyleRecord::decode(&mut Reader::new(&raw)).expect("a font role is a legal byte");
    assert_eq!(record.font_family, eui_proto::FontFamily::Role(2));
    raw[50] = 255;
    let record = StyleRecord::decode(&mut Reader::new(&raw)).expect("a font role is a legal byte");
    assert_eq!(record.font_family, eui_proto::FontFamily::Role(255));
}

#[test]
fn unknown_text_decoration_bits_are_rejected() {
    assert_eq!(style_with(55, &[0b100]), E::IllegalValue("text_decoration has unknown bits"));
}

#[test]
fn a_transition_past_the_motion_scale_is_rejected() {
    assert_eq!(style_with(60, &[6]), E::IllegalValue("transition is a motion index + 1, at most 5"));
}

#[test]
fn an_animation_bit_this_revision_does_not_define_is_rejected() {
    assert_eq!(style_with(61, &[8]), E::IllegalValue("animation is a bit set of 1 (spin), 2 (enter) and 4 (exit)"));
}

#[test]
fn enter_is_a_known_animation() {
    let mut raw = style_bytes();
    raw[61] = 2;
    assert_eq!(StyleRecord::decode(&mut Reader::new(&raw)).map(|s| s.animation), Ok(2));
}

/// A page says how it arrives and how it leaves in one record, because there
/// is no later op to say the second half in.
#[test]
fn an_entrance_and_an_exit_are_one_record() {
    let mut raw = style_bytes();
    raw[61] = 2 | 4;
    raw[63] = 2;
    let out = StyleRecord::decode(&mut Reader::new(&raw)).unwrap();
    assert_eq!((out.animation, out.motion), (6, Motion::Trailing));
    assert_eq!(out.motion.mirrored(), Motion::Leading, "the way out is the way in, reversed");
}

#[test]
fn an_unknown_motion_is_rejected() {
    assert_eq!(style_with(63, &[7]), E::UnknownTag("motion"));
}

/// Offset 63 was the record's last reserved byte and is now `motion`. It is
/// still refused when it is nothing but garbage: a direction is only a
/// direction if something is going that way.
#[test]
fn a_motion_with_neither_an_entrance_nor_an_exit_is_rejected() {
    let mut raw = style_bytes();
    raw[61] = 1; // spin: an animation, but not one that arrives or leaves
    raw[63] = 1;
    assert_eq!(StyleRecord::decode(&mut Reader::new(&raw)).unwrap_err(), E::IllegalValue("motion needs an entrance or an exit to belong to"));
}

#[test]
fn any_blur_radius_is_accepted() {
    let mut raw = style_bytes();
    raw[62] = 255;
    assert_eq!(StyleRecord::decode(&mut Reader::new(&raw)).map(|s| s.blur), Ok(255));
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
    assert_eq!(StyleRecord::decode(&mut Reader::new(&raw[..STYLE_RECORD_BYTES - 1])).unwrap_err(), E::Truncated);
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

// ------------------------------------------------- transfers and resume

/// 01 §6: three flags, and nothing else. A fourth would be a receiver
/// guessing at what a sender meant by bytes it has no rule for.
#[test]
fn an_unknown_transfer_flag_is_refused() {
    let mut w = Writer::new();
    w.varint32(1).varint32(0).u8(3).bytes(b"x");
    assert!(matches!(frame_err(&framed(0x0B, w.as_slice())), E::UnknownTag("chunk flag")));
}

/// A chunk past the ceiling is refused before its bytes are copied.
#[test]
fn an_oversized_transfer_chunk_is_refused() {
    let mut w = Writer::new();
    w.varint32(1).varint32(0).u8(0).bytes(&vec![0u8; MAX_TRANSFER_CHUNK_BYTES + 1]);
    assert!(matches!(frame_err(&framed(0x0C, w.as_slice())), E::LimitExceeded("transfer chunk")));
}

/// An abort carries a reason, not a payload: the ceiling is far lower.
#[test]
fn an_oversized_abort_reason_is_refused() {
    let mut w = Writer::new();
    w.varint32(1).varint32(0).u8(2).bytes(&vec![b'x'; MAX_ABORT_REASON + 1]);
    assert!(matches!(frame_err(&framed(0x0B, w.as_slice())), E::LimitExceeded("transfer chunk")));
}

/// A `Hello` offers nothing, a session (01 §4.1) or a tree (01 §2.6) — and a
/// fourth tag is refused rather than taken for one of the three.
///
/// Tag `2` was this test's unknown value until adoption gave it a meaning,
/// which is the whole point of extending on the tag byte: the byte that used
/// to be refused is the one that now carries the new thing, so an old peer
/// meets it as a refusal rather than misreading it.
#[test]
fn a_hello_with_an_unknown_resume_tag_is_refused() {
    let mut w = Writer::new();
    w.varint32(1);
    Viewport::default().encode(&mut w);
    w.varint32(0).u8(3);
    assert!(matches!(frame_err(&framed(0x01, w.as_slice())), E::UnknownTag("resume")));

    // And the two that do mean something decode.
    let mut ok = Writer::new();
    ok.varint32(1);
    Viewport::default().encode(&mut ok);
    ok.varint32(0).u8(2).raw(&[7u8; 32]);
    assert!(Frame::decode(&framed(0x01, ok.as_slice())).is_ok());
}

/// And a `Welcome` starts fresh, resumed, or adopted (01 §2.6) — a fourth
/// value is refused rather than taken for one of the three.
#[test]
fn a_welcome_with_an_unknown_start_byte_is_refused() {
    let mut w = Writer::new();
    w.varint32(1).raw(&[0u8; 16]).u8(3);
    assert!(matches!(frame_err(&framed(0x02, w.as_slice())), E::UnknownTag("start")));

    for start in 0..=2u8 {
        let mut ok = Writer::new();
        ok.varint32(1).raw(&[0u8; 16]).u8(start);
        assert!(Frame::decode(&framed(0x02, ok.as_slice())).is_ok(), "start {start}");
    }
}

/// A `Hello` that ends before its resume flag is truncated, not tolerated.
#[test]
fn a_hello_without_its_resume_flag_is_truncated() {
    let mut w = Writer::new();
    w.varint32(1);
    Viewport::default().encode(&mut w);
    w.varint32(0);
    assert!(matches!(frame_err(&framed(0x01, w.as_slice())), E::Truncated));
}

/// The same, but seeded with well-formed prefixes so the fuzzer-ish input gets
/// past the first tag byte and exercises the deeper paths.
#[test]
fn corrupted_valid_frames_never_panic() {
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 2, key: 0, text: Some(TextRef::Inline("hello".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    let good = Frame::Batch(Batch { seq: 1, ops: vec![Op::DefAtom { id: 1, value: "label".into() }, Op::DefStyle { id: 1, record: StyleRecord::default() }, Op::Mount(tree)] }).encode();

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

// ---------------------------------------------------- 06 §4, payload shapes

/// Spec 06 §4 asks a server to check three things about an event, and names
/// the third "the payload has the shape in §1". Nothing checked it: a `click`
/// carrying a string reached an application's handler exactly as a pair of
/// coordinates would.
///
/// These are not decode failures — every payload below is a well-formed
/// `Value` and the frame carrying it is a valid frame. They are the layer
/// above: bytes that parse and do not mean what the kind says they mean.
#[test]
fn a_payload_that_is_not_the_shape_its_kind_declares_is_refused() {
    use EventKind as K;
    let xy = || Value::List(vec![Value::Float(1.5), Value::Float(2.5)]);

    // The shapes §1 writes down.
    assert!(K::Click.payload_fits(&xy()));
    assert!(K::DoubleClick.payload_fits(&xy()));
    assert!(K::PointerDown.payload_fits(&Value::List(vec![Value::Float(1.0), Value::Float(2.0), Value::Int(0)])));
    assert!(K::KeyDown.payload_fits(&Value::List(vec![Value::Str("a".into()), Value::Int(1)])));
    assert!(K::TextInput.payload_fits(&Value::Str("hi".into())));
    assert!(K::Scroll.payload_fits(&Value::List(vec![Value::Int(0), Value::Int(40)])));
    assert!(K::Focus.payload_fits(&Value::Null));
    assert!(K::FileDrag.payload_fits(&Value::List(vec![Value::Bool(true)])));
    assert!(K::NfcTag.payload_fits(&Value::List(vec![Value::Str("04a2".into()), Value::List(vec![Value::List(vec![Value::Str("text".into()), Value::Str("hello".into())])])])));

    // An integer stands in for a float, and has to: a coordinate of exactly
    // zero is `Int(0)` to any encoder that writes the narrowest form of a
    // number, and refusing it would refuse the top-left corner of every node.
    assert!(K::Click.payload_fits(&Value::List(vec![Value::Int(0), Value::Int(0)])));
    // The reverse is not true. A fractional button, slot or row index is not
    // a narrower spelling of anything.
    assert!(!K::PointerDown.payload_fits(&Value::List(vec![Value::Float(1.0), Value::Float(2.0), Value::Float(0.5)])));
    assert!(!K::Scroll.payload_fits(&Value::List(vec![Value::Float(0.5), Value::Int(40)])));

    // Wrong type altogether.
    assert!(!K::Click.payload_fits(&Value::Str("wherever you like".into())));
    assert!(!K::Click.payload_fits(&Value::Null));
    assert!(!K::TextInput.payload_fits(&Value::Int(3)));

    // Right type, wrong arity — the case a length check alone would let by.
    assert!(!K::Click.payload_fits(&Value::List(vec![Value::Float(1.0)])));
    assert!(!K::Click.payload_fits(&Value::List(vec![Value::Float(1.0), Value::Float(2.0), Value::Float(3.0)])));
    assert!(!K::Click.payload_fits(&Value::List(vec![Value::Float(0.0); 1024])));

    // A payload on a kind that declares `Null` is a peer saying something
    // the protocol has no room for.
    assert!(!K::Focus.payload_fits(&xy()));
    assert!(!K::Back.payload_fits(&Value::Int(0)));

    // `change` is the one kind with three legitimate shapes (§1): a field's
    // whole value, a track's number, or a two-handle track's pair.
    assert!(K::Change.payload_fits(&Value::Str("typed".into())));
    assert!(K::Change.payload_fits(&Value::Int(40)));
    assert!(K::Change.payload_fits(&Value::List(vec![Value::Int(10), Value::Int(90)])));
    assert!(!K::Change.payload_fits(&Value::List(vec![Value::Int(10), Value::Int(90), Value::Int(100)])));
    assert!(!K::Change.payload_fits(&Value::Null));

    // A tag whose records are not `[kind, payload]` pairs. This one matters
    // more than the rest: 03 §3.3's records are handed to an application
    // whole, and "a list" is not the shape §1 wrote down.
    assert!(!K::NfcTag.payload_fits(&Value::List(vec![Value::Str("04a2".into()), Value::List(vec![Value::Str("not a record".into())])])));
    assert!(!K::NfcTag.payload_fits(&Value::List(vec![Value::Str("04a2".into()), Value::Str("not a list".into())])));
}

/// Every kind has an answer, and the answer is never "anything goes".
///
/// A `match` that grew a catch-all arm would make this whole function a
/// no-op silently, which is the way a check like this usually dies.
#[test]
fn no_kind_accepts_everything() {
    let absurd = Value::List(vec![Value::Asset([7u8; 32]), Value::Bool(false), Value::Null, Value::Color(ColorRef::NONE)]);
    for byte in 0x01..=0x20u8 {
        let Ok(kind) = EventKind::from_u8(byte) else { continue };
        assert!(!kind.payload_fits(&absurd), "{kind:?} accepted a payload of nonsense");
    }
}
