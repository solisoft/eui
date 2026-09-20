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

/// Is this a `ws://` address whose **host** is loopback?
///
/// Spec 01 §1 allows plain text to reach `127.0.0.1`, `localhost` and
/// `[::1]`, "and nothing else". Both transports used to ask this with
/// `url.starts_with("ws://localhost")`, which is not the same question and
/// is not a smaller version of it: `ws://localhost.evil.example/` starts
/// with `ws://localhost`, so an address an attacker controls answered yes.
/// A developer with `EUI_ALLOW_INSECURE_LOOPBACK=1` set — which is every
/// measurement this repository takes — would then have talked to it in
/// clear text, which is precisely the downgrade the whole section exists to
/// refuse.
///
/// So the host is cut out and compared, rather than the string sniffed:
/// the authority is what lies between `ws://` and the first `/`, `?` or
/// `#`; anything before an `@` is userinfo and is **not** the host
/// (`ws://localhost@evil.example/` is a request to `evil.example`); a
/// trailing `:port` is dropped, and for a bracketed IPv6 literal only the
/// colon after the `]`.
pub fn is_loopback_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("ws://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    // Userinfo is whatever precedes the last `@`, and the host is what is
    // left. `rsplit` rather than `split`: a password may itself contain one.
    let hostport = authority.rsplit('@').next().unwrap_or("");
    if let Some(after) = hostport.strip_prefix('[') {
        // `[::1]` or `[::1]:5030` — the literal ends at `]`, and only a
        // colon after it is a port. Anything else after it is another name
        // wearing the address as a prefix.
        return match after.split_once(']') {
            Some((inside, tail)) if tail.is_empty() || tail.starts_with(':') => inside == "::1",
            _ => false,
        };
    }
    // An unbracketed address has at most one colon, and it is the port. Two
    // means a bare IPv6 literal, which is malformed in a URL.
    let host = match hostport.split_once(':') {
        Some((h, port)) if !port.contains(':') => h,
        Some(_) => return false,
        None => hostport,
    };
    matches!(host, "127.0.0.1" | "localhost")
}

#[cfg(test)]
mod tests {
    use super::is_loopback_url;

    #[test]
    fn the_three_addresses_the_spec_names_with_and_without_a_port() {
        for url in [
            "ws://127.0.0.1/_eui/session",
            "ws://127.0.0.1:5030/_eui/session",
            "ws://localhost/x",
            "ws://localhost:5030/x",
            "ws://[::1]/x",
            "ws://[::1]:5030/x",
            "ws://localhost",
            "ws://localhost:5030",
        ] {
            assert!(is_loopback_url(url), "{url}");
        }
    }

    #[test]
    fn a_name_that_merely_begins_with_one_of_them_is_not_one_of_them() {
        // The bug this function was written for. Every one of these passes
        // a `starts_with` and none of them is loopback.
        for url in [
            "ws://127.0.0.1.evil.example/x",
            "ws://localhost.evil.example/x",
            "ws://localhost-evil.example/x",
            "ws://[::1]x.evil.example/x",
            // Userinfo, which looks like the host and is not it.
            "ws://localhost@evil.example/x",
            "ws://127.0.0.1@evil.example/x",
            "ws://user:localhost@evil.example/x",
        ] {
            assert!(!is_loopback_url(url), "{url}");
        }
    }

    #[test]
    fn only_ws_and_only_a_host_that_parses() {
        assert!(!is_loopback_url("wss://127.0.0.1/x"), "wss is allowed by the caller, not by this");
        assert!(!is_loopback_url("http://127.0.0.1/x"));
        assert!(!is_loopback_url("ws://[::1/x"), "an unclosed bracket is not a host");
        assert!(!is_loopback_url("ws://::1/x"), "a bare IPv6 literal is malformed in a URL");
        assert!(!is_loopback_url("ws://127.0.0.2/x"), "the spec names one v4 address, not the /8");
        assert!(!is_loopback_url(""));
    }
}
