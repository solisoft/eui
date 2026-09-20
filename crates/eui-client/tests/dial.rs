//! What an address means, and what an answer to it means.
//!
//! Both of these were wrong in a way that is invisible from the outside: an
//! address that is not a session looked like one, and a server declining an
//! upgrade looked like a network that had dropped it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use eui_client::chrome::session_component;
use eui_client::{connect, Incoming, TransportError};

#[test]
fn only_a_session_address_names_a_component() {
    assert_eq!(session_component("wss://host/_eui/session/site"), Some("site"));
    assert_eq!(session_component("wss://host/_eui/session/site/"), Some("site"));
    assert_eq!(session_component("ws://127.0.0.1:5190/_eui/session/demo"), Some("demo"));

    // The one that mattered: the last segment of any path used to be taken
    // for a component, so this fetched and mounted `site`.
    assert_eq!(session_component("wss://host/blog/site"), None);
    assert_eq!(session_component("wss://host/docs/intro"), None);
    assert_eq!(session_component("wss://host/"), None);
    assert_eq!(session_component("wss://host"), None);
    assert_eq!(session_component("wss://host/_eui/session/"), None);
    assert_eq!(session_component("wss://host/_eui/session"), None);
    // A path *under* a component is not that component either.
    assert_eq!(session_component("wss://host/_eui/session/site/extra"), None);
}

/// A server that answers an upgrade with a status instead of switching.
fn refuses(status: &'static str, body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(4) {
            let Ok(mut s) = stream else { continue };
            let mut req = [0u8; 2048];
            let _ = s.read(&mut req);
            let head = format!("HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            let _ = s.write_all(head.as_bytes());
        }
    });
    format!("ws://{addr}/docs/intro")
}

fn closed_with(url: &str) -> TransportError {
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    // `host_loopback`: the same vouching an embedded host does, so the
    // loopback rule lets a plain `ws://` test server through (08 §1).
    let conn = connect(url, vec![0x01, 0x00], None, true, move || {
        let _ = tx.send(());
    })
    .expect("the dial itself is spawned; the refusal arrives on the channel");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "no answer");
        let _ = rx.recv_timeout(Duration::from_millis(50));
        if let Ok(Incoming::Closed(e)) = conn.rx.try_recv() {
            return e;
        }
    }
}

#[test]
fn a_status_is_an_answer_and_not_a_dropped_socket() {
    // `/docs/intro` is a page, not a session: Soli's upgrade branch answers
    // 404 and routing never runs. Read as a broken connection, this is a
    // backoff ladder against a server that is replying perfectly well — so
    // it has to arrive as something a caller can tell apart.
    let err = closed_with(&refuses("404 Not Found", "WebSocket endpoint not found"));
    let TransportError::Refused(code, why) = err else {
        panic!("a status must not arrive as Connect or Closed: {err:?}");
    };
    assert_eq!(code, 404);
    assert!(why.contains("WebSocket endpoint not found"), "{why}");
    assert!(err_text(404, &why).contains("404"));
}

#[test]
fn an_unauthorised_upgrade_says_which_and_why() {
    // `{"session": "required"}` with no cookie. Retrying cannot help either.
    let err = closed_with(&refuses("401 Unauthorized", "this component needs a session"));
    let TransportError::Refused(code, why) = err else { panic!("{err:?}") };
    assert_eq!(code, 401);
    assert_eq!(why, "this component needs a session");
}

#[test]
fn a_refusal_reads_as_one() {
    // It goes on the glass of a window that never drew anything, so it has
    // to be a sentence rather than a variant name.
    assert_eq!(TransportError::Refused(404, "WebSocket endpoint not found".into()).to_string(), "the server answered 404: WebSocket endpoint not found");
    assert_eq!(TransportError::Refused(503, String::new()).to_string(), "the server answered 503 rather than opening a session");
}

fn err_text(code: u16, why: &str) -> String {
    TransportError::Refused(code, why.to_owned()).to_string()
}

/// Every frame kind the protocol defines, classified on purpose.
///
/// The allowlist decides whether a page that opened no socket has to open
/// one, and its failure mode is quiet in both directions: a kind wrongly
/// `true` dials a session when somebody drags a window edge, and a kind
/// wrongly `false` drops a click into nothing. Neither says anything.
///
/// So the twelve of `spec/01-transport.md` §7 are written out here rather
/// than sampled, and `0x0D` is asserted not to be a kind yet — the line
/// that fails on the day a thirteenth is added, which is the day somebody
/// has to decide which side of this it falls.
#[test]
fn every_frame_kind_is_classified_on_purpose() {
    use eui_client::dial::kind_needs_server;

    // Only a server can answer these: an `Event` (including the one an
    // `emit(...)` in a local handler produces), a `Resync`, an `Upload`.
    for kind in [0x04, 0x09, 0x0B] {
        assert!(kind_needs_server(kind), "{kind:#04x} must dial");
    }

    // And these must not. `Viewport` is the one that matters: it fires on
    // every resize and every palette change, so a denylist would open a
    // session for a window being dragged.
    for kind in [0x01, 0x02, 0x03, 0x05, 0x06, 0x07, 0x08, 0x0A, 0x0C] {
        assert!(!kind_needs_server(kind), "{kind:#04x} must not dial");
    }

    // A kind this client does not know is not a kind this client produced.
    assert!(!kind_needs_server(0x0D));
    assert!(!kind_needs_server(0xFF));

    // The guard: `Frame::decode` knows twelve kinds. When it knows
    // thirteen, the loop above is missing one and this line says so.
    assert_eq!(eui_proto::Frame::decode(&[0x0D, 0x00]), Err(eui_proto::DecodeError::UnknownTag("frame kind")), "a thirteenth frame kind exists; classify it above");
}
