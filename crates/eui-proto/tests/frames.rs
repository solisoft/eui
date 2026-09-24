//! Spec 01 §2.4: a body that is a run of frames, walked without a socket.
//!
//! The framing is self-delimiting, so a socket's message boundaries were
//! never what carried it. These say the walk is possible without softening
//! what `decode` refuses.
#![allow(clippy::unwrap_used, clippy::panic, clippy::indexing_slicing)]

use eui_proto::*;

fn body() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let welcome = Frame::Welcome(Welcome { version: 4, session: [0u8; 16], start: Start::Fresh }).encode();
    let one = Frame::Batch(Batch { seq: 1, ops: vec![Op::DefColor { id: 1, rgba: 0x1122_3344 }] }).encode();
    let two = Frame::Batch(Batch { seq: 2, ops: vec![Op::DefColor { id: 2, rgba: 0x5566_7788 }] }).encode();
    (welcome, one, two)
}

#[test]
fn a_run_of_frames_walks_one_at_a_time() {
    let (w, a, b) = body();
    let mut all = w;
    all.extend_from_slice(&a);
    all.extend_from_slice(&b);

    let mut at = 0;
    let mut seen = Vec::new();
    while at < all.len() {
        let (frame, used) = Frame::decode_prefix(&all[at..]).unwrap();
        assert_eq!(used, Frame::framed_len(&all[at..]).unwrap(), "the two must agree on where the next frame starts");
        seen.push(frame);
        at += used;
    }
    assert_eq!(at, all.len(), "the walk lands exactly on the end");
    assert_eq!(seen.len(), 3);
    assert!(matches!(seen[0], Frame::Welcome(_)));
    assert!(matches!(seen[1], Frame::Batch(ref b) if b.seq == 1));
    assert!(matches!(seen[2], Frame::Batch(ref b) if b.seq == 2));
}

#[test]
fn decode_is_still_strict_about_what_follows() {
    // The walk exists so that `decode` does not have to soften: a
    // concatenation handed to it is still trailing bytes, which is what
    // keeps one implementation's frame from being another's smuggling
    // channel.
    let (w, a, _) = body();
    let mut two = w.clone();
    two.extend_from_slice(&a);
    assert_eq!(Frame::decode(&two), Err(DecodeError::TrailingBytes));
    assert!(Frame::decode(&w).is_ok());
}

#[test]
fn a_length_past_the_ceiling_is_refused_before_the_payload_is_read() {
    // `framed_len` must not be a way to have an enormous length believed. It
    // checks the ceiling `decode` checks, without touching a byte of body —
    // which is the whole point of it being cheap.
    let mut w = Writer::new();
    w.varint(u64::try_from(limits::MAX_FRAME_BYTES).unwrap() + 1);
    let mut bad = vec![0x03u8];
    bad.extend_from_slice(&w.into_vec());
    assert_eq!(Frame::framed_len(&bad), Err(DecodeError::LimitExceeded("frame length")));
}

#[test]
fn a_truncated_run_does_not_claim_a_frame() {
    // A body cut short mid-frame must fail rather than hand back a short
    // one: half a batch applied is a tree the server never sent.
    let (w, a, _) = body();
    let mut all = w;
    all.extend_from_slice(&a[..a.len() - 1]);
    let (_, used) = Frame::decode_prefix(&all).unwrap();
    assert!(Frame::decode_prefix(&all[used..]).is_err());
}

// One frame was bounded and a count never was, because nothing handed a
// client a concatenation before this endpoint did. Checked when the crate
// builds rather than when a test runs: these are constants, so a runtime
// assertion could only ever fail after the binary was already wrong.
const _: () = {
    assert!(limits::MAX_VIEW_FRAMES > 0);
    assert!(limits::MAX_VIEW_BYTES <= limits::MAX_FRAME_BYTES);
};

#[test]
fn the_header_written_in_place_is_the_header_written_in_front() {
    // `encode` writes the body after room for the longest header and fills
    // that room in afterwards. Every width of length varint must come out
    // byte for byte as `kind, varint(len), body` — the lengths either side
    // of each varint boundary are where an off-by-one would live.
    for len in [0usize, 1, 120, 125, 126, 127, 128, 16_380, 16_383, 16_384, 70_000, 2_097_152] {
        let message = "x".repeat(len);
        let mut body = Writer::new();
        body.varint32(7).str(&message);
        let mut want = Writer::new();
        want.u8(0x08).varint(body.len() as u64).raw(body.as_slice());
        let got = Frame::Error { code: 7, message }.encode();
        assert_eq!(got, want.into_vec(), "message of {len} bytes");
    }
    let resync = Frame::Resync.encode();
    assert_eq!(resync, vec![0x09, 0x00]);
    assert_eq!(Frame::decode(&resync).unwrap(), Frame::Resync);
}
