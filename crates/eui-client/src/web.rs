//! Where a page's client starts, and the one thing a browser will only
//! tell the module once: which canvas it was given.
//!
//! The same stash `android.rs` keeps for its `AndroidApp`, for the same
//! reason — the event loop is built long after the entry point returns,
//! and there is no other route from one to the other. A `thread_local`
//! rather than a `OnceLock` because there is one thread and never will be
//! two, and because a page may open a second session in place of the
//! first: the canvas is *taken*, not read, so a stale one cannot be
//! handed to a window that was not asked for.

use std::cell::RefCell;

thread_local! {
    static CANVAS: RefCell<Option<web_sys::HtmlCanvasElement>> = const { RefCell::new(None) };
}

/// Keep the canvas the page named. Called by the entry point, before the
/// event loop exists, and by nothing else.
pub fn start(canvas: web_sys::HtmlCanvasElement) {
    CANVAS.with(|c| *c.borrow_mut() = Some(canvas));
}

/// The canvas, for `WindowAttributesExtWebSys::with_canvas`.
///
/// `None` would have winit make a canvas of its own and append it to the
/// body, which is not what an embed wants and is hard to see having
/// happened — so the window refuses to open instead.
pub fn canvas() -> Option<web_sys::HtmlCanvasElement> {
    CANVAS.with(|c| c.borrow_mut().take())
}
