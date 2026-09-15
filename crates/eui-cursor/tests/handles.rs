//! What [`eui_cursor::position`] does with a handle it cannot use.
//!
//! The answer that matters is the same on every platform and is the whole
//! of what can be checked without a pointer on a screen: a handle that
//! names nothing is `None`, never a guess and never a crash. On a macOS or
//! Windows runner these reach the real `unsafe` bodies and prove their
//! guards; on Linux they reach the stubs and prove the stubs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use eui_cursor::{position, Handle};

#[test]
fn a_null_view_is_nobody_to_ask() {
    assert_eq!(position(Handle::AppKit(std::ptr::null_mut())), None);
}

#[test]
fn a_zero_window_is_nobody_to_ask() {
    // `HWND` is an integer and `0` is the null one. winit's handle is a
    // `NonZeroIsize`, so this cannot arrive from there — which is exactly
    // why it is worth pinning that it is refused rather than passed to
    // `ScreenToClient`.
    assert_eq!(position(Handle::Win32(0)), None);
}

/// A handle for the platform this is *not* running on: an `HWND` on macOS,
/// an `NSView *` on Windows, either on Linux. Nothing to ask, and
/// answering anything would mean a cast between two unrelated kinds of
/// thing.
#[test]
fn the_other_platforms_handle_is_nobody_to_ask() {
    #[cfg(not(target_os = "windows"))]
    assert_eq!(position(Handle::Win32(1)), None);
    #[cfg(not(target_os = "macos"))]
    {
        // Non-null and never dereferenced: the AppKit arm is not compiled
        // on the platforms this branch runs on.
        let mut byte = 0_u8;
        let not_a_view = std::ptr::addr_of_mut!(byte).cast();
        assert_eq!(position(Handle::AppKit(not_a_view)), None);
    }
}
