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

pub mod app;
pub mod driver;
pub mod transport;

pub use driver::{Close, Driver, Input};
pub use transport::{check_url, connect, Connection, Incoming, TransportError};
