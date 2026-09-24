// A test harness, so the strict set is lifted here as in the other test files.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

//! What the decoder *reserves* before it has read what the reservation is
//! for.
//!
//! A limit checked before allocating is only half the rule in the crate docs:
//! a count that is under its ceiling can still be a lie, and memory reserved
//! on the strength of a count the bytes cannot back is memory a hostile
//! sender got for free. These tests watch the allocator rather than the
//! result, because the result of a lie is an error either way — what differs
//! is how much the decoder asked for on its way to that error.
//!
//! One file of its own because the allocator is global to the test binary.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use eui_proto::error::DecodeError;
use eui_proto::{Reader, Value, Writer};

/// The system allocator, remembering the largest single request.
struct Largest;

static LARGEST: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every call is forwarded to `System` unchanged; the only addition is
// an atomic max, which neither allocates nor touches the returned memory.
unsafe impl GlobalAlloc for Largest {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LARGEST.fetch_max(new_size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Largest = Largest;

#[test]
fn a_list_count_the_bytes_cannot_back_reserves_nothing_for_it() {
    // A list header claiming the ceiling's worth of items, and not one item.
    // Before the reservation was capped by the bytes left, this asked for a
    // million `Value`s — 40 MB — from five bytes of input.
    let mut w = Writer::new();
    w.u8(0x08).varint32(eui_proto::limits::MAX_VALUE_LIST);
    let bytes = w.into_vec();

    LARGEST.store(0, Ordering::Relaxed);
    let out = Value::decode(&mut Reader::new(&bytes));
    let largest = LARGEST.load(Ordering::Relaxed);

    assert_eq!(out, Err(DecodeError::Truncated));
    assert!(largest < 4096, "decoding a 5-byte lie reserved {largest} bytes");
}
