//! When a page that opened no socket has to open one.
//!
//! One function, and it lives here rather than in either `transport`
//! module for a reason the module header of `transport_web.rs` states from
//! the other side. The two transports write their types out twice on
//! purpose: `TransportError` and `Incoming` are enums `app.rs` matches on
//! exhaustively, so a variant added to one and not the other is a compile
//! error at the match. That property is what pays for the duplication.
//!
//! A `const fn` over a `u8` has no such backstop. A kind added to one copy
//! and not the other is not a compile error — it is a desktop that dials
//! where a page does not, or the reverse, found by whoever notices that
//! their click did nothing. So this one is written once and both transports
//! re-export it.

/// Would this outgoing frame be pointless without a server?
///
/// The question a page fetched over HTTPS has to answer before it opens a
/// socket it may never need. An **allowlist**, and the difference matters:
/// "anything but `Ack` and `Pong`" reads as the safe rule and is not, because
/// the driver emits a `Viewport` on every resize and every palette change —
/// so dragging a window edge or turning on dark mode would open a session,
/// which is the exact cost this endpoint exists to avoid. That frame is
/// waste anyway: `Hello` rebuilds the viewport when a socket is finally
/// dialled, so it says nothing that is not about to be said again.
///
/// The three that genuinely cannot be answered here: an `Event` (including
/// the one an `emit(...)` inside a local handler produces), a `Resync`, and
/// an `Upload`.
pub const fn kind_needs_server(kind: u8) -> bool {
    match kind {
        0x04 | 0x09 | 0x0B => true,         // Event, Resync, Upload
        0x05 | 0x07 | 0x0A | 0x08 => false, // Ack, Pong, Viewport, Error
        // A `Hello` is sent by dialling rather than through this path, and a
        // kind this client does not know is not one this client produced.
        _ => false,
    }
}
