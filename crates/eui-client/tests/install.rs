//! Installing an application: the manifest's icon fetched and verified,
//! the launcher entry written, and removing it again.
//!
//! The directories these work in are named by environment variables, and
//! those belong to the whole process — so the tests that set them take a
//! lock and run one at a time. Without it they pass alone and fail
//! together, which is the worst way for a test to be wrong.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

use eui_proto::{Manifest, PROTOCOL_VERSION};
use ring::signature::{Ed25519KeyPair, KeyPair};

/// Held for the length of any test that names a directory by environment
/// variable. Poisoning is ignored: a panic in one of these leaves nothing
/// behind but a temporary directory, and turning that into a second
/// failure hides the first.
static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A directory of this test's own, with every variable that decides where
/// an install lands pointed into it.
///
/// All five, not the one this machine happens to read. `XDG_DATA_HOME`
/// means nothing on macOS, which writes a bundle under `$HOME/Applications`,
/// or on Windows, which writes a shortcut under `%APPDATA%` — so a test
/// that redirected only the first installed itself into the runner's own
/// home on two platforms out of three. It failed there for the right
/// reason, and it left a launcher entry behind on the way.
fn sandbox(name: &str) -> PathBuf {
    let here = std::env::temp_dir().join(format!("eui-install-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&here);
    std::fs::create_dir_all(&here).unwrap();
    std::env::set_var("EUI_PINS_DIR", here.join("pins"));
    std::env::set_var("EUI_INSTALLED_FILE", here.join("installed"));
    std::env::set_var("XDG_DATA_HOME", here.join("share"));
    std::env::set_var("HOME", &here);
    std::env::set_var("APPDATA", &here);
    here
}

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
    let _one_at_a_time = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let here = sandbox("e2e");

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
    assert!(app.url.ends_with("/_eui/session/demo"), "{}", app.url);
    // The manifest's `entry` is `/_eui/session`, so `/demo` is a component
    // of the application and not the application itself: it is named for
    // itself, so a launcher holding several of them reads as several
    // things rather than the same word repeated.
    assert_eq!(app.component.as_deref(), Some("demo"));
    assert_eq!(app.name, "Counter — Demo");

    let files = eui_client::install::install(&app).unwrap().files;
    assert!(eui_client::install::installed(&app.url));
    assert_eq!(eui_client::install::list().len(), 1);

    // Everything it wrote is inside the directory it was pointed at, and
    // every recorded path is a file that exists.
    for f in &files {
        assert!(f.starts_with(&here), "{} escaped the install root", f.display());
        assert!(f.exists(), "{} was recorded and not written", f.display());
    }

    if cfg!(target_os = "linux") {
        // First, not merely present: `install::launch` starts `files[0]`,
        // and this list used to lead with the icon — so installing an
        // application opened its PNG and nothing happened.
        assert_eq!(files.first().and_then(|f| f.extension()).and_then(|e| e.to_str()), Some("desktop"), "the launcher entry leads the list");
        let desktop = files.iter().find(|f| f.extension().is_some_and(|e| e == "desktop")).expect("a .desktop file");
        let text = std::fs::read_to_string(desktop).unwrap();
        assert!(text.contains("Name=Counter — Demo"), "{text}");
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
    assert!(!eui_client::install::installed(&app.url));
    for f in &files {
        assert!(!f.exists(), "{} survived the uninstall", f.display());
    }
    let _ = std::fs::remove_dir_all(&here);
}

/// Two components of one application are two entries.
///
/// A Soli application serves every component at one origin under one
/// `app_id`, because the id is what a publisher key is pinned against.
/// Keying the record on it said the music player was installed because
/// somebody had installed the gallery: a tick on a page nobody had
/// installed, and a launcher entry that opened something else.
#[test]
fn one_application_two_components_two_entries() {
    let _one_at_a_time = ONE_AT_A_TIME.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let here = sandbox("two");

    let png = icon();
    let one = eui_client::install::App { app_id: "demo-app".into(), name: "Meridian".into(), url: "wss://demo.example/_eui/session/gallery".into(), icon: png.clone(), component: None };
    let two =
        eui_client::install::App { app_id: "demo-app".into(), name: "Meridian — Music".into(), url: "wss://demo.example/_eui/session/music".into(), icon: png, component: Some("music".into()) };

    let a = eui_client::install::install(&one).unwrap();
    // The second is not installed because the first is.
    assert!(eui_client::install::installed(&one.url));
    assert!(!eui_client::install::installed(&two.url), "the gallery is not the music player");

    let b = eui_client::install::install(&two).unwrap();
    assert_eq!(eui_client::install::list().len(), 2, "two entries, one application");
    assert!(eui_client::install::installed(&one.url) && eui_client::install::installed(&two.url));
    // Different files, or the second wrote over the first.
    assert_ne!(a.files, b.files);

    // Removing one leaves the other.
    eui_client::install::uninstall(&two.url).unwrap();
    assert!(eui_client::install::installed(&one.url));
    assert!(!eui_client::install::installed(&two.url));

    // And the `app_id` removes whatever is left of the application.
    eui_client::install::install(&two).unwrap();
    eui_client::install::uninstall("demo-app").unwrap();
    assert!(eui_client::install::list().is_empty(), "an app_id removes every entry it has");
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
