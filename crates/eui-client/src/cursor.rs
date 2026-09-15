//! Where the pointer is during a drag — the safe half.
//!
//! winit reports a file over the window and a file let go on it, and
//! carries no position with either (see [`eui_cursor`] for which line of
//! which backend throws it away). No `CursorMoved` arrives meanwhile, so
//! [`crate::app`]'s last-known pointer is from before the gesture started.
//! This asks the platform instead.
//!
//! All of the winit is here and none of the `unsafe`, the way `ios.rs`
//! keeps the safe half of `eui-uikit` and `wayland.rs` of `eui-wayland`.

/// Where the pointer is now, in this window's physical px, top-left
/// origin — the frame `CursorMoved` reports in, so the caller divides by
/// the same scale.
///
/// `None` on a platform with no answer here: Wayland, where
/// [`crate::wayland`] is told the position outright and has no need to
/// ask; X11, whose `XdndPosition` winit discards and which is still on the
/// last-pointer fallback; and the phones, which have no pointer and no
/// file drop.
pub fn position(window: &winit::window::Window) -> Option<(f32, f32)> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::AppKit(h) => eui_cursor::Handle::AppKit(h.ns_view.as_ptr()),
        RawWindowHandle::Win32(h) => eui_cursor::Handle::Win32(h.hwnd.get()),
        _ => return None,
    };
    eui_cursor::position(handle)
}
