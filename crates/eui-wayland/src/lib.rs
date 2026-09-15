//! That a file is over the window, and which files were let go — asked of
//! Wayland, because winit will not say.
//!
//! winit reports a dropped file on X11, on Windows and on macOS. Its
//! Wayland backend has no data device in it at all: `HoveredFile`,
//! `HoveredFileCancelled` and `DroppedFile` are never emitted there, so on
//! a Wayland desktop a drop zone is a rectangle that does nothing and says
//! nothing. This crate is the missing half, and the client keeps the rest
//! (`eui-client/src/wayland.rs`).
//!
//! It exists as its own crate for two lines of `unsafe`, and for where
//! those are allowed to be. `eui-client` is `#![forbid(unsafe_code)]` — a
//! promise about the process that draws pages from the network, and not
//! one to give up for a file drop. So the pointers are turned into objects
//! here, in a crate small enough to read in a sitting, and the client does
//! the safe half: pulling the `wl_display` and the `wl_surface` out of its
//! window handle and handing them over. It knows nothing of winit.
//!
//! **Why not a second connection.** The obvious shape — open our own
//! `wl_display`, bind a data device, listen — cannot work, and it is worth
//! saying so before somebody tries it. `wl_data_device.enter` is not
//! broadcast: the compositor finds the surface under the drag, finds the
//! client that owns it, and sends the event to *that client's* device. A
//! second `wl_display_connect()` is a different `wl_client` — its own
//! socket, its own object ids, its own resources — and it owns no
//! surfaces, so nothing of its is ever under a pointer and no `enter` is
//! ever addressed to it. Even if one were, the `surface` argument is an
//! object id on the connection that received it, and an id from one
//! connection says nothing about an object on another.
//!
//! So the requirement is not "speak Wayland", it is "be the same client".
//! That is what [`wayland_backend::sys::client::Backend::from_foreign_display`]
//! buys: a second `wl_event_queue` on winit's own `wl_display`, hence the
//! same socket, the same client and the same id space — and hence a
//! `surface` argument that can be compared, by identity, against ours.
//!
//! **What it costs.** This crate shares a fate with the window. There is no
//! per-object failure on Wayland: a protocol error on a `wl_data_offer` —
//! a `finish` outside its window, a stale offer touched after `leave`, an
//! action mask with two bits in it — is `wl_display.error`, and
//! libwayland's answer to that is the whole connection, which is winit's
//! connection, which is the window. It would not look like a broken file
//! drop; it would look like the application vanishing when somebody drags
//! a file near it. Everything below that reads like excessive bookkeeping
//! is there for that one reason.

#![allow(unsafe_code)]

pub mod uris;

use std::path::PathBuf;

/// What the compositor said about a drag over the one window this was
/// started for.
///
/// Positions are surface-local and in the window's own logical px. There is
/// no scale to take out of them: winit's Wayland pointer takes the same
/// numbers and multiplies by the scale factor, and the client divides it
/// straight back out, so what arrives here is already what the client
/// counts in.
#[derive(Debug, Clone, PartialEq)]
pub enum Drag {
    /// A drag is over the surface at this point, or — `None` — has left it,
    /// ended, or gone somewhere else.
    Over(Option<(f32, f32)>),
    /// Files were let go, here. One event for however many were held: the
    /// caller makes one arrival of each.
    Dropped {
        /// Where the hand let go, surface-local.
        at: (f32, f32),
        /// The local files it was holding, in the order the sender listed.
        paths: Vec<PathBuf>,
    },
}

#[cfg(target_os = "linux")]
mod dnd;

#[cfg(target_os = "linux")]
pub use dnd::Dnd;

/// Where there is no Wayland to ask.
#[cfg(not(target_os = "linux"))]
#[derive(Debug)]
pub struct Dnd(());

#[cfg(not(target_os = "linux"))]
impl Dnd {
    /// Nothing to watch: this platform has no Wayland, and winit reports
    /// its own drops there.
    ///
    /// # Safety
    ///
    /// Nothing is read, so nothing is required. The signature matches the
    /// Linux one so the caller has no `cfg` of its own.
    #[must_use]
    pub fn start(_display: *mut core::ffi::c_void, _surface: *mut core::ffi::c_void, _wake: Box<dyn Fn() + Send>) -> Option<(Self, std::sync::mpsc::Receiver<Drag>)> {
        None
    }
}
