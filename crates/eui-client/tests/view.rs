//! Spec 01 §2.4: a first render fetched over HTTPS, with no session.
//!
//! The point of the endpoint is that a reader who only reads costs the
//! server nothing, so what these check is that the body is taken apart
//! correctly and that everything which is *not* such a body is refused
//! rather than half-applied.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::io::{Read, Write};
use std::net::TcpListener;

use eui_client::transport::{fetch_view, ViewError, VIEW_MEDIA_TYPE};
use eui_proto::{Batch, Frame, Op, StyleRecord, Welcome};

/// The bytes a server answers with: a `Welcome` naming no session, then
/// batches.
fn frames() -> (Vec<u8>, usize) {
    let welcome = Frame::Welcome(Welcome { version: 4, session: [0u8; 16], resumed: false }).encode();
    let one = Frame::Batch(Batch { seq: 1, ops: vec![Op::DefStyle { id: 1, record: StyleRecord::default() }, Op::DefColor { id: 1, rgba: 0x1122_3344 }] }).encode();
    let two = Frame::Batch(Batch { seq: 2, ops: vec![Op::DefColor { id: 2, rgba: 0x5566_7788 }] }).encode();
    let mut body = welcome;
    body.extend_from_slice(&one);
    body.extend_from_slice(&two);
    (body, 3)
}

/// A server that answers one request with what it was given.
fn serve(status: &'static str, content_type: &'static str, body: Vec<u8>, etag: Option<&'static str>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(4) {
            let Ok(mut s) = stream else { continue };
            let mut req = [0u8; 2048];
            let _ = s.read(&mut req);
            let tag = etag.map_or_else(String::new, |t| format!("ETag: {t}\r\n"));
            let head = format!("HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{tag}Connection: close\r\n\r\n", body.len());
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(&body);
        }
    });
    // `ws://` so the loopback rule lets it through; `origin_for` maps it to
    // `http://` for the fetch, which is the pairing the endpoint relies on.
    format!("ws://{addr}/_eui/session/demo")
}

#[test]
fn a_body_of_frames_is_walked_into_its_frames() {
    let (body, count) = frames();
    let url = serve("200 OK", VIEW_MEDIA_TYPE, body, Some("\"abc\""));
    let view = fetch_view(&url, "demo", 4, 1000, None).expect("a well-formed body");

    assert_eq!(view.frames.len(), count);
    assert_eq!(view.frames[0][0], 0x02, "a Welcome first");
    assert!(view.frames[1..].iter().all(|f| f[0] == 0x03), "batches after it");
    // Verbatim, quotes and all: the only correct `If-None-Match` is the
    // bytes the server sent, never one rebuilt from a hash of the body.
    assert_eq!(view.etag.as_deref(), Some("\"abc\""));

    // The tree's identity is the batches and not the body, because the
    // `Welcome` differs between the two roads a tree can arrive by.
    let mut hasher = blake3::Hasher::new();
    for f in &view.frames[1..] {
        hasher.update(f);
    }
    assert_eq!(view.tree, *hasher.finalize().as_bytes());
}

#[test]
fn a_404_is_not_an_error_but_an_answer() {
    // Most components are not served this way, and the client's response is
    // to open a socket rather than to complain.
    let url = serve("404 Not Found", "text/plain", b"no such static view".to_vec(), None);
    assert_eq!(fetch_view(&url, "demo", 4, 1000, None), Err(ViewError::NotOffered));
}

#[test]
fn a_body_that_is_not_frames_is_refused() {
    // An origin that answers this path with a login page is the case worth
    // refusing: the media type is the only thing that says the bytes are
    // what was asked for.
    let url = serve("200 OK", "text/html", b"<!doctype html><title>hi</title>".to_vec(), None);
    let Err(ViewError::Refused(why)) = fetch_view(&url, "demo", 4, 1000, None) else {
        panic!("html is not a render");
    };
    assert!(why.contains("text/html"), "{why}");
}

#[test]
fn a_body_that_does_not_start_with_a_welcome_is_refused() {
    let (body, _) = frames();
    let welcome_len = Frame::framed_len(&body).unwrap();
    // Batches with no `Welcome` in front: a shape 01 §2.4 does not promise.
    let url = serve("200 OK", VIEW_MEDIA_TYPE, body[welcome_len..].to_vec(), None);
    assert!(matches!(fetch_view(&url, "demo", 4, 1000, None), Err(ViewError::Refused(_))));
}

#[test]
fn a_truncated_body_is_refused_whole_rather_than_applied_in_part() {
    // This is the one that matters. A body cut short cannot be recovered by
    // opening a socket afterwards, because by then the driver would have
    // mounted a piece of a tree the server never sent.
    let (body, _) = frames();
    let url = serve("200 OK", VIEW_MEDIA_TYPE, body[..body.len() - 3].to_vec(), None);
    assert!(matches!(fetch_view(&url, "demo", 4, 1000, None), Err(ViewError::Refused(_))));
}

#[test]
fn a_welcome_on_its_own_is_not_a_render() {
    // Nothing to mount. Better to dial than to show an empty window.
    let welcome = Frame::Welcome(Welcome { version: 4, session: [0u8; 16], resumed: false }).encode();
    let url = serve("200 OK", VIEW_MEDIA_TYPE, welcome, None);
    assert!(matches!(fetch_view(&url, "demo", 4, 1000, None), Err(ViewError::Refused(_))));
}
