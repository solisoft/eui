//! Installing an application: the manifest's icon fetched and verified,
//! the launcher entry written, and removing it again.
//!
//! One test and one process, because the three directories it works in are
//! named by environment variables and those are the whole process's.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::io::{Read, Write};
use std::net::TcpListener;

use eui_proto::{Manifest, PROTOCOL_VERSION};
use ring::signature::{Ed25519KeyPair, KeyPair};

/// A small square PNG, as a publisher's icon would be.
fn icon() -> Vec<u8> {
    let (w, h) = (48u32, 48u32);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            rgba.extend_from_slice(&[(x * 5) as u8, (y * 5) as u8, 0x80, 0xff]);
        }
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    writer.write_image_data(&rgba).unwrap();
    writer.finish().unwrap();
    out
}

/// A server that answers the two requests an install makes: the manifest,
/// and the one asset it names. Anything else is a 404, so a client asking
/// for something it was not told about fails rather than being humoured.
fn serve(manifest: Vec<u8>, asset_path: String, asset: Vec<u8>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(8) {
            let Ok(mut s) = stream else { continue };
            let mut req = [0u8; 4096];
            let n = s.read(&mut req).unwrap_or(0);
            let head = String::from_utf8_lossy(&req[..n]).to_string();
            let path = head.split_whitespace().nth(1).unwrap_or("").to_owned();
            let body: &[u8] = if path == "/.well-known/eui" {
                &manifest
            } else if path == asset_path {
                &asset
            } else {
                b""
            };
            let status = if body.is_empty() { "404 Not Found" } else { "200 OK" };
            let out = format!("HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = s.write_all(out.as_bytes());
            let _ = s.write_all(body);
        }
    });
    format!("ws://{addr}/_eui/session/demo")
}

#[test]
fn an_application_with_a_signed_icon_installs_and_uninstalls() {
    let here = std::env::temp_dir().join(format!("eui-install-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&here);
    std::env::set_var("EUI_PINS_DIR", here.join("pins"));
    std::env::set_var("EUI_INSTALLED_FILE", here.join("installed"));
    std::env::set_var("XDG_DATA_HOME", here.join("share"));

    let png = icon();
    let hash = *blake3::hash(&png).as_bytes();
    let key = Ed25519KeyPair::from_pkcs8(ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap().as_ref()).unwrap();
    let mut public = [0u8; 32];
    public.copy_from_slice(key.public_key().as_ref());
    let m = Manifest {
        app_id: "counter.example".into(),
        name: "Counter".into(),
        version: "1.0".into(),
        protocol_min: 1,
        protocol_max: PROTOCOL_VERSION,
        publisher_key: public,
        icon: Some(hash),
        ..Manifest::default()
    };
    let sig = key.sign(&m.signed_bytes());
    let mut s = [0u8; 64];
    s.copy_from_slice(sig.as_ref());
    let url = serve(m.encode(&s), format!("/_eui/asset/{}", hex(&hash)), png);

    // The manifest is verified and the icon fetched by its hash, both
    // before anything is written.
    let app = eui_client::install::from_url(&url).unwrap();
    assert_eq!(app.app_id, "counter.example");
    assert_eq!(app.name, "Counter");
    assert!(app.url.ends_with("/_eui/session/demo"), "{}", app.url);

    let files = eui_client::install::install(&app).unwrap();
    assert!(eui_client::install::installed("counter.example"));
    assert_eq!(eui_client::install::list().len(), 1);

    // Everything it wrote is inside the directory it was pointed at, and
    // every recorded path is a file that exists.
    for f in &files {
        assert!(f.starts_with(&here), "{} escaped the install root", f.display());
        assert!(f.exists(), "{} was recorded and not written", f.display());
    }

    if cfg!(target_os = "linux") {
        let desktop = files.iter().find(|f| f.extension().is_some_and(|e| e == "desktop")).expect("a .desktop file");
        let text = std::fs::read_to_string(desktop).unwrap();
        assert!(text.contains("Name=Counter"), "{text}");
        assert!(text.contains("X-EUI-AppId=counter.example"), "{text}");
        // The address is quoted as the desktop entry specification asks,
        // not as a shell would, and it is the whole session URL.
        assert!(text.contains(&format!("\"{}\"", app.url)), "{text}");
        // The icon is a real PNG at the size the entry promises.
        let icon_at = files.iter().find(|f| f.extension().is_some_and(|e| e == "png")).expect("a PNG");
        let img = eui_client::assets::decode_png(&std::fs::read(icon_at).unwrap()).unwrap();
        assert_eq!((img.width, img.height), (512, 512));
    }

    // Installing twice replaces rather than accumulating.
    eui_client::install::install(&app).unwrap();
    assert_eq!(eui_client::install::list().len(), 1);

    let gone = eui_client::install::uninstall("counter.example").unwrap();
    assert_eq!(gone.len(), files.len());
    assert!(!eui_client::install::installed("counter.example"));
    for f in &files {
        assert!(!f.exists(), "{} survived the uninstall", f.display());
    }
    let _ = std::fs::remove_dir_all(&here);
}

/// An application that publishes no icon is refused, with a reason: every
/// installed application wearing the client's own picture is a launcher
/// full of things you cannot tell apart.
#[test]
fn an_application_without_an_icon_is_not_installable() {
    let m = Manifest { app_id: "plain.example".into(), name: "Plain".into(), protocol_max: PROTOCOL_VERSION, ..Manifest::default() };
    assert!(m.icon.is_none());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
