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
//!
//! The loop itself is kept here too, as a proxy into it. winit builds one
//! event loop per page and refuses a second — "EventLoop can't be
//! recreated" was what the second embed on a page got — so every session
//! after the first is handed to the loop that is already running
//! ([`swap`]), and opens in the window and canvas it already has.

use std::cell::RefCell;

use winit::event_loop::EventLoopProxy;

use crate::app::Wake;

thread_local! {
    static CANVAS: RefCell<Option<web_sys::HtmlCanvasElement>> = const { RefCell::new(None) };
    static LOOP: RefCell<Option<EventLoopProxy<Wake>>> = const { RefCell::new(None) };
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

/// Keep a way into the loop once it exists. Called by `run_loop` on this
/// target, and by nothing else.
pub(crate) fn hold(proxy: EventLoopProxy<Wake>) {
    LOOP.with(|l| *l.borrow_mut() = Some(proxy));
}

/// Open `url` in place of whatever session this page is running, if it is
/// running one. `false` means there is no loop yet, and the caller starts
/// one; `true` means the loop that is there took it, and nothing else is to
/// be done — least of all building a second loop.
pub fn swap(url: String, allowed: u32) -> bool {
    LOOP.with(|l| l.borrow().as_ref().is_some_and(|p| p.send_event(Wake::Swap(url, allowed)).is_ok()))
}
