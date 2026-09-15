//! Where the pointer is, asked of the platform, because winit will not say
//! it during a drag.
//!
//! Spec 03 §3.2 wants two things of a file crossing a window: the node
//! under it lights while it is held there, and the node under it takes it
//! when it is let go. Both are questions about a **position**, and winit
//! answers neither. `HoveredFile` and `DroppedFile` carry no coordinates on
//! any backend — the position is there in every one of them and is thrown
//! away:
//!
//! - Windows: `_pt: *const POINTL` in `DragEnter`, `DragOver` and `Drop`
//!   (`winit/src/platform_impl/windows/drop_handler.rs`);
//! - macOS: `draggingEntered:` and `performDragOperation:` never read
//!   `draggingLocation`, and `draggingUpdated:` is not implemented at all;
//! - X11: the `XdndPosition` coordinates are dropped, with a comment
//!   saying a future winit might carry them.
//!
//! And no `CursorMoved` arrives meanwhile — the drag source holds the
//! pointer — so the last one the window saw is from before the gesture
//! began. The client used to fall back to that, which put the drop
//! wherever the mouse had last happened to rest: usually not the drop
//! zone, and nowhere at all if the pointer had not been over the page
//! since the window opened.
//!
//! So the position is asked for directly, at the moment it is wanted. It is
//! one call per platform and both are `unsafe`, which is why this is a
//! crate: `eui-client` is `#![forbid(unsafe_code)]` and that is not a
//! promise to give up for a cursor position. Same split as `eui-uikit` and
//! `eui-wayland` — the caller does the safe half, pulling the handle out of
//! its window, and hands the pointer over. This crate knows nothing of
//! winit.
//!
//! Wayland is not here and does not need to be: `wl_data_device` reports
//! `enter` and `motion` with surface-local coordinates, so `eui-wayland`
//! already knows where the file is without asking anybody.

#![allow(unsafe_code)]

use core::ffi::c_void;

/// A window to ask about, in whatever the platform calls one.
///
/// Not a `*mut c_void` for both: an `HWND` is an integer and a `NSView *`
/// is a pointer, and a function that took one shape for both would be one
/// cast away from asking AppKit about a number.
#[derive(Debug, Clone, Copy)]
pub enum Handle {
    /// An `NSView *` — the window's content view, macOS.
    AppKit(*mut c_void),
    /// An `HWND`, Windows.
    Win32(isize),
}

/// Where the pointer is now, relative to `window`'s content area, in
/// **physical pixels** with the origin at the top left.
///
/// The same frame `WindowEvent::CursorMoved` reports in, so the caller
/// divides by the same scale factor it already divides that by — one rule
/// for both paths rather than two that have to be kept agreeing.
///
/// `None` where there is nothing to ask: a handle this build has no
/// platform for, a null pointer, a view with no window yet, or a call the
/// platform refused. The caller keeps its old answer in that case, which is
/// what it had before this existed.
///
/// The point may be **outside** the content area — negative, or past the
/// width — when the pointer is over another window or the desktop. That is
/// reported rather than hidden: it is what "the file is no longer over the
/// page" looks like, and the caller's own bounds check is what turns it
/// into that.
#[must_use]
pub fn position(window: Handle) -> Option<(f32, f32)> {
    match window {
        Handle::AppKit(view) => appkit(view),
        Handle::Win32(hwnd) => win32(hwnd),
    }
}

#[cfg(target_os = "macos")]
#[allow(clippy::cast_possible_truncation)]
fn appkit(view: *mut c_void) -> Option<(f32, f32)> {
    use objc2_app_kit::NSView;

    if view.is_null() {
        return None;
    }
    // SAFETY: the caller's contract on [`Handle::AppKit`] — a live
    // `NSView *` from this process's own window, on the main thread, which
    // is where the event loop calls this from.
    let view: &NSView = unsafe { &*view.cast::<NSView>() };
    let window = view.window()?;
    // Not `NSEvent::mouseLocation`, which is the screen and would then have
    // to be converted through two coordinate systems. This is the pointer
    // in the window's own, and it is readable at any time — the point of
    // "outside of the event stream" is that it does not need an event,
    // which is exactly the situation here: during a drag there are no mouse
    // events to read one from.
    //
    // SAFETY: a getter on a live window; it takes no arguments and keeps
    // nothing.
    let in_window = unsafe { window.mouseLocationOutsideOfEventStream() };
    // winit's content view overrides `isFlipped` to `true`, so this comes
    // back with the origin at the top left and there is no flip to do —
    // the same call and the same assumption as winit's own `mouse_motion`.
    let p = view.convertPoint_fromView(in_window, None);
    let scale = window.backingScaleFactor();
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    Some(((p.x * scale) as f32, (p.y * scale) as f32))
}

#[cfg(not(target_os = "macos"))]
fn appkit(_view: *mut c_void) -> Option<(f32, f32)> {
    None
}

#[cfg(target_os = "windows")]
#[allow(clippy::cast_precision_loss)]
fn win32(hwnd: isize) -> Option<(f32, f32)> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    if hwnd == 0 {
        return None;
    }
    let mut pt = POINT { x: 0, y: 0 };
    // SAFETY: `pt` is a live, correctly sized `POINT` this function owns.
    // The call fails rather than writing anything on a locked desktop.
    if unsafe { GetCursorPos(&mut pt) } == 0 {
        return None;
    }
    // Screen to client. The window is per-monitor DPI aware — winit asks
    // for that — so client coordinates are device pixels, which is what
    // this function promises.
    //
    // SAFETY: the caller's contract on [`Handle::Win32`] — a live `HWND`
    // belonging to this process — and `pt` as above.
    if unsafe { ScreenToClient(hwnd, &mut pt) } == 0 {
        return None;
    }
    Some((pt.x as f32, pt.y as f32))
}

#[cfg(not(target_os = "windows"))]
fn win32(_hwnd: isize) -> Option<(f32, f32)> {
    None
}
