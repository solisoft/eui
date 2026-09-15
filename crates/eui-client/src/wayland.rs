//! Wayland: the one thing a compositor will tell the client that winit
//! will not — that a file is over the window, and which files were let go.
//!
//! winit answers spec 03 §3.2's drop half on X11, on Windows and on macOS.
//! Its Wayland backend has no data device in it at all, so
//! `WindowEvent::HoveredFile` and `WindowEvent::DroppedFile` are never
//! emitted there and a drop zone on a Wayland desktop is a rectangle that
//! does nothing, says nothing, and logs nothing. This file is the other
//! path: the same two calls into the driver, reached a different way.
//!
//! The `unsafe` is [`eui_wayland::Dnd::start`], one crate over, because
//! this crate is `#![forbid(unsafe_code)]`. What is left here is the safe
//! half, and it is all of the winit: the `wl_display` and the `wl_surface`
//! out of the window's own handles. `ios.rs` and `eui-uikit` are split on
//! the same line for the same reason.
//!
//! On X11 — and on XWayland, and under `WINIT_UNIX_BACKEND=x11` — the
//! handle is not a Wayland one, [`watch`] returns `None`, and winit's own
//! path is the only one running. There is no configuration in which both
//! fire.

use std::sync::Arc;

use crate::app::Wake;

/// Watch this window's data device for dragged and dropped files, on the
/// connection winit already has open.
///
/// `None` on X11, on a winit built without its Wayland backend, on a
/// compositor with no data device, and when `EUI_WAYLAND_DND=0` — which is
/// there for the same reason `EUI_A11Y=0` is: finding out whether a new
/// platform part is behind a misbehaviour, on a binary somebody already
/// has, without asking them to build one.
pub fn watch(window: &winit::window::Window, proxy: &Arc<winit::event_loop::EventLoopProxy<Wake>>) -> Option<Drops> {
    use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};

    if std::env::var_os("EUI_WAYLAND_DND").is_some_and(|v| v == "0") {
        eprintln!("eui: EUI_WAYLAND_DND=0; this window takes no dropped files");
        return None;
    }
    let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else { return None };
    let RawWindowHandle::Wayland(surface) = window.window_handle().ok()?.as_raw() else { return None };
    // One `Arc` clone, made once when the window opens. Never an
    // `EventLoopProxy::clone`, and never per frame: see the note on
    // [`crate::app::Proxy`] for what that cost the last time.
    let proxy = Arc::clone(proxy);
    let wake = Box::new(move || {
        let _ = proxy.send_event(Wake::Drop);
    });
    // SAFETY: both pointers come from this window's own handles and name
    // objects on the one connection winit is running; the `Drops` is
    // dropped in `Shell::close`, before the window they point into.
    let (dnd, rx) = eui_wayland::Dnd::start(display.display.as_ptr(), surface.surface.as_ptr(), wake)?;
    eprintln!("eui: watching the Wayland data device for dropped files");
    Some(Drops { dnd, rx })
}

/// The watching thread, and the queue it puts what it saw on.
///
/// Dropping this stops the thread and waits for it, which is why it is
/// dropped by name in `Shell::close` rather than left to the end of the
/// window: the thread holds the surface it is being told about.
pub struct Drops {
    /// Held for its `Drop`. The thread reads the window's pointers, so it
    /// has to be stopped before the window can go.
    #[allow(dead_code)]
    dnd: eui_wayland::Dnd,
    rx: std::sync::mpsc::Receiver<eui_wayland::Drag>,
}

impl std::fmt::Debug for Drops {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Drops").finish_non_exhaustive()
    }
}

impl Drops {
    /// Everything the thread has said since last time.
    ///
    /// A `Vec` and not an iterator, so the borrow on the window ends before
    /// the window is asked to act on any of it.
    pub fn take(&mut self) -> Vec<eui_wayland::Drag> {
        self.rx.try_iter().collect()
    }
}
