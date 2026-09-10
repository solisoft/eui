//! # EUI client
//!
//! Three parts, kept apart so that two of them can be tested without the
//! third:
//!
//! - [`driver`] — the session, layout, input dispatch and painting. Frames
//!   in, frames out; input in, events out. No window, no socket.
//! - [`transport`] — a WebSocket over TLS on its own thread, binary frames
//!   only, refusing `ws://` outside a debug build's loopback.
//! - [`app`] — the winit window in `ControlFlow::Wait`, a wgpu surface, and
//!   the glue. There is no render loop; the window redraws only when a frame
//!   arrived, the viewer acted, or the OS asked.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// Pixel and coordinate arithmetic on values the window handed us; see
// eui-render's lib.rs for why this is the right scope for the lint.
#![allow(clippy::arithmetic_side_effects)]

// The tree an assistive technology sees is plain data the worker builds
// and the window forwards, so it is compiled whether or not the platform
// adapter is: only `a11y::to_update`, which speaks AccessKit, is behind
// the feature. `--no-default-features` drops the adapter, not the wire.
pub mod a11y;
pub mod app;
pub mod assets;
pub mod audio;
pub mod chrome;

/// Which build this is: the short commit it was made from, `+` when the
/// tree it was made from had uncommitted changes, `unknown` when there was
/// no repository to ask. Shown in the window so a demo from the wrong run
/// can be told from the right one at a glance.
pub const BUILD: &str = env!("EUI_BUILD");
pub mod desktop_theme;
pub mod driver;
pub mod manifest;
pub mod sandbox;
pub mod transport;
pub mod worker;

pub use assets::{AssetError, AssetStore, Image};
pub use driver::{Close, Driver, Input};
pub use transport::{check_url, connect, Connection, Incoming, TransportError};
pub use worker::Backend;
