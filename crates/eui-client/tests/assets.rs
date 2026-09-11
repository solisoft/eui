//! Assets: the strict HTTP reader, hash verification, PNG decoding, and the
//! driver's fetch-then-size cycle for images.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::io::{Read, Write};
use std::net::TcpListener;

use eui_client::assets::{self, AssetError};
use eui_client::{Driver, Input};
use eui_proto::*;
use eui_theme::Role;

const AVATAR: &[u8] = include_bytes!("../../../examples/demo-app/public/images/avatar.png");
/// The same 32×32 avatar as a JPEG (flattened onto its own blue, since a
/// JPEG has no alpha) and as a lossless WebP (which keeps it).
const AVATAR_JPEG: &[u8] = include_bytes!("../../../examples/demo-app/public/images/avatar.jpg");
const AVATAR_WEBP: &[u8] = include_bytes!("../../../examples/demo-app/public/images/avatar.webp");

fn hash_of(bytes: &[u8]) -> [u8; 32] {
    *blake3::hash(bytes).as_bytes()
}

/// A one-shot HTTP/1.1 server on loopback that answers every request with
/// `body` (and the status/headers given), then exits.
fn serve_once(status: &'static str, extra_headers: &'static str, body: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut req = [0u8; 2048];
        let _ = s.read(&mut req);
        let head = format!("HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len());
        let _ = s.write_all(head.as_bytes());
        let _ = s.write_all(&body);
    });
    format!("http://{addr}")
}

#[test]
fn origins_derive_from_session_urls() {
    assert_eq!(assets::origin_for("wss://app.example/_eui/session/x").unwrap(), "https://app.example");
    assert_eq!(assets::origin_for("wss://app.example:8443/_eui/session/x").unwrap(), "https://app.example:8443");
    assert_eq!(assets::origin_for("ws://127.0.0.1:5011/_eui/session/x").unwrap(), "http://127.0.0.1:5011");
    assert!(assets::origin_for("https://app.example/").is_err());
    assert!(assets::origin_for("nope").is_err());
}

#[test]
fn a_fetch_verifies_the_hash() {
    let body = b"hello, content addressing".to_vec();
    let origin = serve_once("200 OK", "", body.clone());
    let got = assets::fetch(&origin, &hash_of(&body), None).unwrap();
    assert_eq!(got, body);

    // The same bytes under a different name are refused, not displayed.
    let origin = serve_once("200 OK", "", body.clone());
    assert_eq!(assets::fetch(&origin, &hash_of(b"something else"), None), Err(AssetError::HashMismatch));
}

#[test]
fn the_reader_is_strict() {
    let body = b"x".to_vec();
    let origin = serve_once("404 Not Found", "", body.clone());
    assert!(matches!(assets::fetch(&origin, &hash_of(&body), None), Err(AssetError::Http(_))));
    let origin = serve_once("200 OK", "Transfer-Encoding: chunked\r\n", body.clone());
    assert!(matches!(assets::fetch(&origin, &hash_of(&body), None), Err(AssetError::Http(_))));
    assert!(matches!(assets::fetch("http://127.0.0.1:1", &hash_of(&body), None), Err(AssetError::Connect(_))));
}

#[test]
fn the_avatar_decodes() {
    let img = assets::decode_png(AVATAR).unwrap();
    assert_eq!((img.width, img.height), (32, 32));
    assert_eq!(img.rgba.len(), 32 * 32 * 4);
    // Centre pixel is the white square; a corner is transparent.
    let px = |x: usize, y: usize| &img.rgba[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
    assert_eq!(px(16, 16), [255, 255, 255, 255]);
    assert_eq!(px(0, 0)[3], 0);
    assert_eq!(&px(4, 16)[..3], [0x22, 0x29, 0xa8]);
    assert!(matches!(assets::decode_png(b"not a png"), Err(AssetError::Decode(_))));
}

#[test]
fn the_other_two_pictures_decode_too() {
    // A picture is told by its first bytes, not by a name: the store
    // decodes whichever of the three it was handed.
    let png = assets::decode_image(AVATAR).unwrap();
    let jpeg = assets::decode_image(AVATAR_JPEG).unwrap();
    let webp = assets::decode_image(AVATAR_WEBP).unwrap();
    for img in [&png, &jpeg, &webp] {
        assert_eq!((img.width, img.height), (32, 32));
        assert_eq!(img.rgba.len(), 32 * 32 * 4);
    }
    let px = |img: &assets::Image, x: usize, y: usize| img.rgba[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].to_vec();
    // The centre is white in all three; JPEG is lossy, so it is only nearly.
    assert_eq!(px(&png, 16, 16), [255, 255, 255, 255]);
    assert_eq!(px(&webp, 16, 16), [255, 255, 255, 255]);
    assert!(px(&jpeg, 16, 16).iter().take(3).all(|c| *c > 240), "{:?}", px(&jpeg, 16, 16));
    // Lossless WebP keeps the transparent corner; the JPEG has none to keep.
    assert_eq!(px(&webp, 0, 0)[3], 0);
    assert_eq!(px(&jpeg, 0, 0)[3], 255);
    assert!(matches!(assets::decode_image(b"not a picture at all"), Err(AssetError::Decode(_))));
}

#[test]
fn the_store_asks_once_and_remembers_failures() {
    let mut s = assets::AssetStore::default();
    let h = hash_of(AVATAR);
    s.want(h);
    s.want(h);
    assert_eq!(s.take_pending(), vec![h]);
    assert!(s.take_pending().is_empty(), "asked once");
    s.deliver(h, AVATAR.to_vec());
    assert_eq!(s.image(&h).unwrap().width, 32);
    s.want(h);
    assert!(s.take_pending().is_empty(), "held, so not asked again");
    let bad = hash_of(b"bad");
    s.fail(bad, "nope".into());
    s.want(bad);
    assert!(s.take_pending().is_empty(), "failed, so not asked again");
    assert_eq!(s.failure(&bad), Some("nope"));
}

#[test]
fn an_image_node_is_fetched_then_sized_then_painted() {
    let h = hash_of(AVATAR);
    let mut d = Driver::new(300.0, 200.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], resumed: false }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
    // No explicit size: the image takes its intrinsic size once fetched.
    tree.nodes.push(FlatNode { kind: NodeKind::Image, id: 2, style: 0, key: 0, text: None, props: (0, 1), handlers: (0, 0), child_count: 0 });
    tree.props.push((1, Value::Asset(h)));
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 3, style: 0, key: 0, text: Some(TextRef::Inline("after".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    d.handle_frame(Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "src".into() },
            Op::DefStyle { id: 1, record: StyleRecord { display: Display::Row, align_items: AlignItems::Start, gap: 2, ..Default::default() } },
            Op::Mount(tree),
        ],
    }));
    let before = d.paint(300, 200);
    assert!(!before.quads.iter().any(|q| q.params[2] as u32 == eui_render::TEXTURED_RGBA), "nothing to draw yet");
    let img = d.session().lookup(2).unwrap();
    assert_eq!(d.layout().rect(img).unwrap().w, 0.0, "no size before the fetch");
    assert_eq!(d.pending_assets(), vec![h]);
    assert!(d.pending_assets().is_empty(), "asked once");

    d.asset_ready(h, AVATAR.to_vec());
    assert!(d.needs_redraw());
    let after = d.paint(300, 200);
    let r = d.layout().rect(img).unwrap();
    assert_eq!((r.w, r.h), (32.0, 32.0), "intrinsic size");
    let text = d.session().lookup(3).unwrap();
    assert_eq!(d.layout().rect(text).unwrap().x, 32.0 + 4.0, "the text moved over");
    assert_eq!(after.quads.iter().filter(|q| q.params[2] as u32 == eui_render::TEXTURED_RGBA).count(), 1);
    let _ = Role::AccentBase;
    let _ = Input::Unfocused;
}

/// A picture of `w` × `h`, a diagonal ramp so that averaging it gives
/// something checkable, fully opaque.
fn ramp(w: u32, h: u32) -> assets::Image {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let v = ((x + y) % 256) as u8;
            rgba.extend_from_slice(&[v, 255 - v, 128, 255]);
        }
    }
    assets::Image { width: w, height: h, rgba }
}

/// A picture bigger than the atlas is drawn, not silently dropped.
///
/// `ImageAtlas::pack` refuses anything with an edge past the sheet's 2048
/// and remembers the refusal so it is never retried, so before
/// `fit_to_atlas` a photograph off a phone — over the line on both axes by
/// default — reached the painter as no region at all and was drawn as bare
/// background: a black box, with nothing in the log to say why.
#[test]
fn a_picture_too_big_for_the_sheet_is_shrunk_rather_than_lost() {
    let big = ramp(2493, 3401);

    let small = assets::fit_to_atlas(&big).expect("over the edge, so a copy");
    assert_eq!(small.height, assets::ATLAS_EDGE, "the long edge is the cap");
    assert_eq!(small.width, 751, "and the short one keeps the proportion");
    assert_eq!(small.rgba.len() as u32, small.width * small.height * 4);
    assert!(small.rgba.chunks(4).all(|p| p[3] == 255), "opaque throughout");
    assert!(small.rgba.chunks(4).any(|p| p[0] > 0), "and not a black square");

    // The natural size is what an unsized picture lays out at, so it has to
    // survive the shrink untouched.
    assert_eq!((big.width, big.height), (2493, 3401));

    let h = hash_of(b"big");
    let mut whole = eui_render::ImageAtlas::new();
    assert!(whole.insert(h, big.width, big.height, &big.rgba).is_none(), "the sheet refuses it whole");
    let mut shrunk = eui_render::ImageAtlas::new();
    assert!(shrunk.insert(h, small.width, small.height, &small.rgba).is_some(), "and takes it shrunk");
}

/// One that already fits is handed over as it is, with no copy made.
#[test]
fn a_picture_that_fits_is_not_copied() {
    assert!(assets::fit_to_atlas(&assets::decode_image(AVATAR).unwrap()).is_none());
}

/// Transparency does not drag colour into what is opaque: a red pixel next
/// to a transparent one averages to red, not to half of red.
#[test]
fn what_is_transparent_lends_no_colour_to_what_is_not() {
    let mut rgba = Vec::new();
    for y in 0..2100u32 {
        for x in 0..2100u32 {
            if (x + y) % 2 == 0 {
                rgba.extend_from_slice(&[255, 0, 0, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 255, 0]);
            }
        }
    }
    let small = assets::fit_to_atlas(&assets::Image { width: 2100, height: 2100, rgba }).unwrap();
    let mid = small.rgba.chunks(4).nth((small.height / 2 * small.width + small.width / 2) as usize).unwrap();
    assert_eq!((mid[0], mid[1], mid[2]), (255, 0, 0), "the blue was invisible and stays out");
    // Not exactly half: 2100 into 1024 gives boxes two or three pixels
    // wide, so a checkerboard does not fall evenly inside every one of
    // them. Around half is the claim.
    assert!((100..=160).contains(&mid[3]), "and about half the coverage survives as alpha, got {}", mid[3]);
}
