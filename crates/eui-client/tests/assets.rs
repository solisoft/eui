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
    // A status the server really sent comes back as itself, so a caller can
    // tell `404` — which for a view means "not served that way, open a
    // socket" — from a reply that could not be read at all, without matching
    // on the text of a status line.
    let origin = serve_once("404 Not Found", "", body.clone());
    assert_eq!(assets::fetch(&origin, &hash_of(&body), None), Err(AssetError::Status(404)));
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
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
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

/// A scene's module and mesh are assets, and travel the same verified path a
/// picture does.
#[test]
fn a_scene_asks_for_its_shader_and_its_mesh() {
    let (shader, mesh) = ([5u8; 32], [6u8; 32]);
    let mut d = Driver::new(300.0, 200.0, 1.0, caps::SCENE);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scene, id: 2, style: 1, key: 0, text: None, props: (0, 2), handlers: (0, 0), child_count: 0 });
    tree.props.push((1, Value::Asset(shader)));
    tree.props.push((2, Value::Asset(mesh)));
    d.handle_frame(Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "shader".into() },
            Op::DefAtom { id: 2, value: "mesh".into() },
            Op::DefStyle { id: 1, record: StyleRecord { width: Dim::Px(60), height: Dim::Px(40), ..Default::default() } },
            Op::Mount(tree),
        ],
    }));
    let asked = d.pending_assets();
    assert!(asked.contains(&shader) && asked.contains(&mesh), "both, and by hash: {asked:?}");
}

/// 08 §3: a session that was never granted `scene` does not so much as ask
/// for the **module** — and still asks for the mesh.
///
/// The line the grant draws is not the node kind, it is a program the server
/// wrote. A mesh is vertices, checked in the worker like any other asset;
/// withholding it would have been a toll on nothing. The module is the case
/// the capability exists for, and refusing to *compile* it later would still
/// have fetched it and told the server the request went out. Here there is no
/// request, so there is nothing to learn — decidable on a machine with no
/// GPU, which is what makes it a vector rather than a hope.
#[test]
fn a_scene_asks_for_nothing_without_the_grant() {
    let (shader, mesh) = ([5u8; 32], [6u8; 32]);
    let mut d = Driver::new(300.0, 200.0, 1.0, 0);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scene, id: 2, style: 1, key: 0, text: None, props: (0, 2), handlers: (0, 0), child_count: 0 });
    tree.props.push((1, Value::Asset(shader)));
    tree.props.push((2, Value::Asset(mesh)));
    d.handle_frame(Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "shader".into() },
            Op::DefAtom { id: 2, value: "mesh".into() },
            Op::DefStyle { id: 1, record: StyleRecord { width: Dim::Px(60), height: Dim::Px(40), ..Default::default() } },
            Op::Mount(tree),
        ],
    }));
    let asked = d.pending_assets();
    assert!(!asked.contains(&shader), "the module is not fetched: {asked:?}");
    assert!(asked.contains(&mesh), "the mesh is, because it is data and not a program");
    // The node still draws nothing: it named a module, and that module is
    // the thing the person did not allow.
    let list = d.paint(300, 200);
    assert!(list.scenes.is_empty());
}

/// The whole chain, in one test: a module the server named, fetched by hash,
/// verified in the driver, handed to a real GPU, and looked at.
///
/// Every link has a vector of its own — the verifier's, the asset store's,
/// the pipe's, the renderer's. None of them proves they are joined, and this
/// is the join: what arrives as an opaque `Value::Asset` in a tree comes out
/// the far end as the colour the server's own shader chose, on hardware.
///
/// Skipped with a notice where there is no adapter, like the renderer's own
/// pixel tests.
#[test]
fn a_shader_the_server_named_travels_from_a_hash_to_the_screen() {
    let Ok(mut renderer) = eui_render::Renderer::new_headless() else {
        eprintln!("no GPU adapter; skipping the end-to-end scene");
        return;
    };
    // The server's module: flat green, ignoring the tint it is handed, so
    // that the pixel read back can only have come from *this* shader and not
    // from the client's own.
    let wgsl = "
struct Scene { mvp: mat4x4<f32>, time: vec4<f32>, size: vec4<f32>, params: vec4<f32>, tint: vec4<f32> }
@group(0) @binding(0) var<uniform> u: Scene;
struct VIn { @location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32> }
struct VOut { @builtin(position) pos: vec4<f32> }
@vertex fn vs_main(in: VIn) -> VOut {
    var o: VOut;
    o.pos = u.mvp * vec4<f32>(in.pos, 1.0);
    return o;
}
@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(0.0, 1.0, 0.0, 1.0); }
";
    let asset = eui_shader::wrap(wgsl);
    let hash = hash_of(&asset);

    let mut d = Driver::new(64.0, 64.0, 1.0, caps::SCENE);
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Scene, id: 2, style: 1, key: 0, text: None, props: (0, 2), handlers: (0, 0), child_count: 0 });
    tree.props.push((1, Value::Asset(hash)));
    // Red, which the module above pointedly does not use.
    tree.props.push((2, Value::List(vec![Value::Float(0.0); 4].into_iter().chain([Value::Float(1.0), Value::Float(0.0), Value::Float(0.0), Value::Float(1.0)]).collect())));
    d.handle_frame(Frame::Batch(Batch {
        seq: 1,
        ops: vec![
            Op::DefAtom { id: 1, value: "shader".into() },
            Op::DefAtom { id: 2, value: "uniforms".into() },
            Op::DefStyle { id: 1, record: StyleRecord { width: Dim::Px(64), height: Dim::Px(64), ..Default::default() } },
            Op::Mount(tree),
        ],
    }));

    // The tree asks for the module by hash, and by nothing else.
    assert_eq!(d.pending_assets(), vec![hash], "the module is wanted, by hash");
    d.asset_ready(hash, asset);

    // Verified on the way in; what comes out is what the GPU may have.
    let taken = d.take_scene_assets();
    assert_eq!(taken.len(), 1, "one module, checked");
    let (got, eui_client::driver::SceneAsset::Shader(src)) = &taken[0] else { panic!("a shader, not a mesh") };
    assert_eq!(*got, hash);
    renderer.load_shader(hash, src).expect("the module compiles");

    let mut textures = renderer.session();
    let list = d.paint(64, 64);
    assert_eq!(list.scenes.len(), 1);
    assert_eq!(list.scenes[0].shader, hash, "the draw names the module the server did");

    let target = renderer.offscreen(64, 64);
    let (atlas, images) = d.atlases_mut();
    renderer.render_offscreen(&mut textures, &target, 0.0, &list, atlas, images);
    let px = renderer.read_back(&target).expect("read back");
    let i = (32 * 64 + 32) * 4;
    let middle = [px[i], px[i + 1], px[i + 2]];
    assert!(middle[1] > 200 && middle[0] < 60, "the server's own shader is what drew: {middle:?}");
}

// -------------------------------------------------------------- font roles
//
// A face is an asset like any other: named by its hash, fetched from the
// session's own origin, verified, and then — unlike a picture — handed to the
// shaper. 02 §5, 08 §8.

/// One of the client's own faces, read off disk so the test has real font
/// bytes without embedding a fifth copy of one.
const A_FACE: &[u8] = include_bytes!("../../eui-text/fonts/NotoSansSymbols-Regular.ttf");

/// A row holding one text node drawn in `role`, with that role bound to
/// `hash`. A row that starts its children rather than stretching them is
/// what makes the text's own measured width visible in its rect.
fn text_in_role(d: &mut Driver, role: u8, hash: Option<[u8; 32]>) {
    d.handle_frame(Frame::Welcome(Welcome { version: 1, session: [0; 16], start: Start::Fresh }));
    let mut tree = Subtree::default();
    tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 1 });
    tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 2, key: 0, text: Some(TextRef::Inline("Hamburgefonstiv".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
    let mut ops = vec![
        Op::DefStyle { id: 1, record: StyleRecord { display: Display::Row, align_items: AlignItems::Start, ..Default::default() } },
        Op::DefStyle { id: 2, record: StyleRecord { font_family: FontFamily::Role(role), ..Default::default() } },
    ];
    if let Some(hash) = hash {
        ops.insert(0, Op::DefFont { role, faces: vec![hash] });
    }
    ops.push(Op::Mount(tree));
    d.handle_frame(Frame::Batch(Batch { seq: 1, ops }));
}

#[test]
fn a_font_role_is_fetched_then_bound_then_reshaped() {
    let h = hash_of(A_FACE);
    let mut d = Driver::new(300.0, 200.0, 1.0, 0);
    text_in_role(&mut d, 2, Some(h));

    // A role's faces are wanted because the *session* bound them, not
    // because a node holds the hash in a prop: no node ever names a face.
    assert_eq!(d.pending_assets(), vec![h]);
    assert!(d.pending_assets().is_empty(), "asked once");

    let node = d.session().lookup(2).unwrap();
    d.paint(300, 200);
    let fallback = d.layout().rect(node).unwrap().w;
    assert!(fallback > 0.0, "an unbound role draws in sans rather than not at all");

    d.asset_ready(h, A_FACE.to_vec());
    assert!(d.needs_redraw());
    d.paint(300, 200);
    assert_ne!(d.layout().rect(node).unwrap().w, fallback, "the run was re-shaped in the face that arrived");
}

#[test]
fn a_face_that_is_not_a_face_leaves_the_role_in_sans() {
    let junk = b"GIF89a and not a font at all".to_vec();
    let h = hash_of(&junk);
    let mut d = Driver::new(300.0, 200.0, 1.0, 0);
    text_in_role(&mut d, 3, Some(h));
    assert_eq!(d.pending_assets(), vec![h]);

    let node = d.session().lookup(2).unwrap();
    d.paint(300, 200);
    let fallback = d.layout().rect(node).unwrap().w;

    d.asset_ready(h, junk);
    d.paint(300, 200);
    assert_eq!(d.layout().rect(node).unwrap().w, fallback, "the text is still drawn, in sans");
    assert!(d.assets().failure(&h).is_some(), "and the reason was kept");
}

#[test]
fn a_role_the_application_never_bound_draws_in_sans() {
    // Role 7, and no `DefFont` anywhere: a legal style naming a binding that
    // does not exist. The session stands and the text is drawn.
    let mut d = Driver::new(300.0, 200.0, 1.0, 0);
    text_in_role(&mut d, 7, None);
    assert!(d.pending_assets().is_empty(), "nothing to fetch");
    d.paint(300, 200);
    let node = d.session().lookup(2).unwrap();
    assert!(d.layout().rect(node).unwrap().w > 0.0, "drawn in sans");
}
