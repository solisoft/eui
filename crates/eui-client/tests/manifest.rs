//! The manifest check: signature, protocol range, trust on first use, key
//! rotation. Keys are generated here with ring; the pin store is a temp dir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use eui_client::manifest::{verify, ManifestError};

/// The origin most of these tests fetch from; the pin is kept under it.
const O: &str = "https://app.example";
use eui_proto::{caps, Manifest, Rotation, PROTOCOL_VERSION};

/// A manifest for a server this client can actually talk to.
///
/// Written against `PROTOCOL_VERSION` rather than a literal, so that the
/// next kind added to the protocol moves these tests with it instead of
/// breaking them: what they are about is signatures and pins, not versions.
/// `Manifest::default()` stays at `1..=1` on purpose — an absent field on
/// the wire means a server that claims only the oldest version, and
/// claiming more on a server's behalf is the one mistake a default here
/// must not make.
fn speakable() -> Manifest {
    Manifest { protocol_min: 1, protocol_max: PROTOCOL_VERSION, ..Manifest::default() }
}
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};

fn keypair() -> Ed25519KeyPair {
    let doc = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    Ed25519KeyPair::from_pkcs8(doc.as_ref()).unwrap()
}

fn public(k: &Ed25519KeyPair) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(k.public_key().as_ref());
    out
}

fn signed(m: &Manifest, k: &Ed25519KeyPair) -> Vec<u8> {
    let sig = k.sign(&m.signed_bytes());
    let mut s = [0u8; 64];
    s.copy_from_slice(sig.as_ref());
    m.encode(&s)
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("eui-pins-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn a_signed_manifest_verifies_and_pins_its_key_on_first_use() {
    let pins = tmp("first");
    let k = keypair();
    let m = Manifest { app_id: "demo".into(), name: "Demo".into(), publisher_key: public(&k), capabilities: caps::CLIPBOARD_READ, ..speakable() };
    let got = verify(&signed(&m, &k), O, &pins).unwrap();
    assert_eq!(got, m);
    let pin = eui_client::manifest::store_path(&pins, O, "demo");
    assert_eq!(std::fs::read(&pin).unwrap(), public(&k).to_vec(), "the pin is the raw public key");
    // The same key again: fine. A tampered field: the signature fails.
    assert!(verify(&signed(&m, &k), O, &pins).is_ok());
    let mut bytes = signed(&m, &k);
    bytes[10] ^= 1;
    assert_eq!(verify(&bytes, O, &pins).unwrap_err(), ManifestError::BadSignature);
    // A stranger with a valid manifest of their own for the same app_id: refused against the pin.
    let other = keypair();
    let stranger = signed(&Manifest { publisher_key: public(&other), capabilities: caps::CAMERA, ..m.clone() }, &other);
    assert_eq!(verify(&stranger, O, &pins).unwrap_err(), ManifestError::KeyChanged);
}

/// 08 §2: the same signed bytes served from a second origin. The manifest
/// names no host, so the signature verifies there as well as at home; what
/// must not follow is the first origin's pin. Pinned by `app_id` alone, the
/// copy matched and wore the padlock, and a hostile origin that got to an
/// `app_id` first had the real publisher refused with `KeyChanged`.
#[test]
fn the_same_manifest_at_another_origin_is_pinned_afresh() {
    let pins = tmp("origins");
    let k = keypair();
    let m = Manifest { app_id: "demo".into(), publisher_key: public(&k), ..speakable() };
    let bytes = signed(&m, &k);
    verify(&bytes, O, &pins).unwrap();
    let there = "https://copy.example";
    let pin = eui_client::manifest::store_path(&pins, there, "demo");
    assert!(!pin.exists(), "nothing is pinned for the second origin before it is seen");
    verify(&bytes, there, &pins).unwrap();
    assert_eq!(std::fs::read(&pin).unwrap(), public(&k).to_vec(), "it got a pin of its own, on first use");
    // And the other way round: whoever claims an app_id first at their own
    // origin does not lock the real publisher out of it at theirs.
    let squatter = keypair();
    let claimed = Manifest { app_id: "claimed".into(), publisher_key: public(&squatter), ..speakable() };
    verify(&signed(&claimed, &squatter), there, &pins).unwrap();
    let owner = keypair();
    let real = Manifest { publisher_key: public(&owner), ..claimed.clone() };
    assert!(verify(&signed(&real, &owner), O, &pins).is_ok(), "the real publisher is not KeyChanged by a squatter elsewhere");
    // A pin written before pins were kept per origin is not inherited.
    let legacy = pins.join("legacy".bytes().map(|b| format!("{b:02x}")).collect::<String>());
    std::fs::write(&legacy, public(&squatter)).unwrap();
    let fresh = Manifest { app_id: "legacy".into(), publisher_key: public(&owner), ..speakable() };
    assert!(verify(&signed(&fresh, &owner), O, &pins).is_ok(), "an app_id-only pin is ignored, not compared");
}

#[test]
fn a_changed_key_is_refused_unless_the_old_key_signed_the_rotation() {
    let pins = tmp("rotate");
    let old = keypair();
    let new = keypair();
    let first = Manifest { app_id: "demo".into(), publisher_key: public(&old), ..speakable() };
    verify(&signed(&first, &old), O, &pins).unwrap();
    let moved = Manifest { publisher_key: public(&new), ..first.clone() };
    assert_eq!(verify(&signed(&moved, &new), O, &pins).unwrap_err(), ManifestError::KeyChanged);
    // A rotation signed by the wrong key does not help.
    let mut bad_sig = [0u8; 64];
    bad_sig.copy_from_slice(new.sign(&public(&new)).as_ref());
    let bad = Manifest { rotation: Some(Rotation { previous_key: public(&old), signature: bad_sig }), ..moved.clone() };
    assert_eq!(verify(&signed(&bad, &new), O, &pins).unwrap_err(), ManifestError::KeyChanged);
    // The old key vouching for the new one: accepted, and the pin moves.
    let mut sig = [0u8; 64];
    sig.copy_from_slice(old.sign(&public(&new)).as_ref());
    let good = Manifest { rotation: Some(Rotation { previous_key: public(&old), signature: sig }), ..moved.clone() };
    assert!(verify(&signed(&good, &new), O, &pins).is_ok());
    assert!(verify(&signed(&moved, &new), O, &pins).is_ok(), "pinned to the new key now");
    assert_eq!(verify(&signed(&first, &old), O, &pins).unwrap_err(), ManifestError::KeyChanged, "and the old one is refused");
}

#[test]
fn a_server_outside_this_clients_protocol_is_refused() {
    let pins = tmp("proto");
    let k = keypair();
    // A range that starts past what this client speaks, whatever it speaks.
    let (min, max) = (PROTOCOL_VERSION + 1, PROTOCOL_VERSION + 2);
    let m = Manifest { app_id: "demo".into(), publisher_key: public(&k), protocol_min: min, protocol_max: max, ..Manifest::default() };
    assert_eq!(verify(&signed(&m, &k), O, &pins).unwrap_err(), ManifestError::Protocol { min, max });
    assert!(!pins.exists(), "nothing pinned for a server we cannot talk to");
}

/// A server older than this client is talked down to, not refused.
///
/// The session always could: a server settles at `min(client, its own)` and
/// the client accepts any `Welcome` at or below its version. Only the
/// manifest check stood in the way, and requiring an exact match meant that
/// the day a version was added, every client refused every server that had
/// not been upgraded yet — which is every server, on that day. It happened:
/// `manifest: the server speaks EUI 2-2, this client 3`, against production
/// and against a local `soli` alike.
#[test]
fn a_server_older_than_this_client_is_still_talked_to() {
    let pins = tmp("older");
    let k = keypair();
    // The oldest wire this client still speaks, offered on its own.
    let (min, max) = (2, 2);
    let m = Manifest { app_id: "demo".into(), publisher_key: public(&k), protocol_min: min, protocol_max: max, ..Manifest::default() };
    assert!(verify(&signed(&m, &k), O, &pins).is_ok(), "a {min}-{max} server was refused by a client speaking {PROTOCOL_VERSION}");
}

#[test]
fn garbage_is_not_a_manifest() {
    let pins = tmp("garbage");
    assert!(matches!(verify(b"<!doctype html>", O, &pins).unwrap_err(), ManifestError::Decode(_)));
}

// -------------------------------------------------------------- grants

/// Spec 01 §2.1: what the person said to the consent sheet, kept so they
/// are not asked it again.
mod grants {
    use eui_client::manifest::{remember_grant, remembered_grant, store_path, Answered};
    use eui_proto::caps;

    const O: &str = super::O;

    /// One grant store for the module, and an `app_id` per test.
    ///
    /// `EUI_GRANTS_DIR` is process-wide and these run on threads of one
    /// process, so a directory per test would have them setting the
    /// variable out from under each other. One directory, set to the same
    /// path by whoever gets there first, and nothing shared inside it.
    fn store() -> std::path::PathBuf {
        static ONCE: std::sync::Once = std::sync::Once::new();
        let dir = std::env::temp_dir().join(format!("eui-grants-{}", std::process::id()));
        ONCE.call_once(|| {
            let _ = std::fs::remove_dir_all(&dir);
            std::env::set_var("EUI_GRANTS_DIR", &dir);
        });
        dir
    }

    /// The file `app_id` is kept in.
    fn file_of(app_id: &str) -> std::path::PathBuf {
        store_path(&store(), O, app_id)
    }

    #[test]
    fn nobody_has_been_asked_yet() {
        store();
        assert_eq!(remembered_grant(O, "com.example.unasked"), None);
    }

    #[test]
    fn what_was_said_comes_back() {
        store();
        let said = Answered { asked: caps::FS_PICK | caps::CAMERA, granted: caps::FS_PICK };
        remember_grant(O, "com.example.roundtrip", said);
        assert_eq!(remembered_grant(O, "com.example.roundtrip"), Some(said));
    }

    /// The whole reason the store keeps two numbers. Refused and never
    /// asked are both "not granted", and telling them apart is what
    /// decides between nagging somebody about the thing they already said
    /// no to and never noticing that a new version wants something new.
    #[test]
    fn a_refusal_is_an_answer_and_not_a_silence() {
        store();
        remember_grant(O, "com.example.refused", Answered { asked: caps::CAMERA, granted: 0 });
        let back = remembered_grant(O, "com.example.refused").unwrap();
        assert_eq!(back.granted, 0, "they said no");
        assert_eq!(back.asked, caps::CAMERA, "and they were asked, which is not the same as not having been");
        assert_eq!(caps::CAMERA & !(back.granted | back.asked), 0, "so it is not asked again");
        assert_ne!(caps::FS_PICK & !(back.granted | back.asked), 0, "and something new still is");
    }

    #[test]
    fn everything_refused_is_still_an_answer() {
        store();
        let said = Answered { asked: caps::ALL, granted: 0 };
        remember_grant(O, "com.example.allrefused", said);
        assert_eq!(remembered_grant(O, "com.example.allrefused"), Some(said));
    }

    /// A file somebody edited, or one a different build wrote. The sheet
    /// going up again costs a question; trusting it costs a grant nobody
    /// gave.
    #[test]
    fn a_file_that_makes_no_sense_is_not_an_answer() {
        let path = file_of("com.example.nonsense");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for bad in ["", "   ", "not a number", "7", "7 x", &format!("0 {}", caps::ALL)] {
            std::fs::write(&path, bad).unwrap();
            assert_eq!(remembered_grant(O, "com.example.nonsense"), None, "{bad:?}");
        }
    }

    /// Bits outside 01 §2.1 are not capabilities, however they got there.
    #[test]
    fn nothing_outside_the_capabilities_survives_the_store() {
        store();
        remember_grant(O, "com.example.outside", Answered { asked: u32::MAX, granted: u32::MAX });
        let back = remembered_grant(O, "com.example.outside").unwrap();
        assert_eq!(back.asked, caps::ALL);
        assert_eq!(back.granted, caps::ALL);
    }

    /// An `app_id` is a string from a server. It names a file, so it must
    /// not be able to name one anywhere else.
    #[test]
    fn an_app_id_cannot_walk_out_of_the_store() {
        let dir = store();
        remember_grant(O, "../../../etc/passwd", Answered { asked: caps::FS_PICK, granted: caps::FS_PICK });
        remember_grant("../../../x", "../../../etc/passwd", Answered { asked: caps::FS_PICK, granted: caps::FS_PICK });
        assert!(file_of("../../../etc/passwd").starts_with(&dir), "the file it wrote is inside the store");
        assert!(store_path(&dir, "../../../x", "../../../etc/passwd").is_file(), "an origin cannot walk out either");
        for origin in std::fs::read_dir(dir.join("by-origin")).unwrap().filter_map(Result::ok) {
            let hexed = |e: &std::fs::DirEntry| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_hexdigit());
            assert!(hexed(&origin), "every origin in it is hex, so none of them is a path");
            assert!(std::fs::read_dir(origin.path()).unwrap().filter_map(Result::ok).all(|e| hexed(&e)), "and every app_id under one");
        }
    }

    /// 08 §2: a grant is given to an application *at an origin*. The
    /// manifest is public and names no host, so another origin serving the
    /// same `app_id` — the same bytes, even — is somebody else, and gets
    /// asked. Keyed by `app_id` alone, it inherited the camera.
    #[test]
    fn a_grant_given_at_one_origin_is_not_given_at_another() {
        store();
        let said = Answered { asked: caps::CAMERA | caps::FS_PICK, granted: caps::CAMERA };
        remember_grant(O, "com.example.copied", said);
        assert_eq!(remembered_grant(O, "com.example.copied"), Some(said));
        assert_eq!(remembered_grant("https://evil.example", "com.example.copied"), None, "a copy elsewhere starts unasked");
        assert_eq!(remembered_grant("https://app.example:8443", "com.example.copied"), None, "a port is another origin");
        assert_eq!(remembered_grant("http://app.example", "com.example.copied"), None, "and so is a scheme");
        // One origin however it is spelled.
        assert_eq!(remembered_grant("https://APP.example:443/", "com.example.copied"), Some(said));
    }

    /// Answers written before grants were kept per origin are one file per
    /// bare `app_id`. Which origin gave them is exactly what they did not
    /// record, so they are not carried over: the application asks once more.
    #[test]
    fn an_answer_kept_by_app_id_alone_is_not_read() {
        let dir = store();
        std::fs::create_dir_all(&dir).unwrap();
        let legacy = dir.join("com.example.legacy".bytes().map(|b| format!("{b:02x}")).collect::<String>());
        std::fs::write(legacy, format!("{} {}\n", caps::CAMERA, caps::CAMERA)).unwrap();
        assert_eq!(remembered_grant(O, "com.example.legacy"), None);
    }
}

/// Spec 01 §2.1: "the protocol's own prefix is the part nobody should have
/// to type." A person pasting what their browser shows them is doing the
/// obvious thing, and it names the same origin.
#[test]
fn an_address_is_taken_the_way_a_person_writes_it() {
    use eui_client::normalise_url;

    assert_eq!(normalise_url("https://app.example/_eui/session/x"), "wss://app.example/_eui/session/x");
    assert_eq!(normalise_url("http://127.0.0.1:5190/_eui/session/x"), "ws://127.0.0.1:5190/_eui/session/x");
    // Already the protocol's own spelling: untouched.
    assert_eq!(normalise_url("wss://app.example/x"), "wss://app.example/x");
    assert_eq!(normalise_url("ws://127.0.0.1:9/x"), "ws://127.0.0.1:9/x");
    // A bare host can only be the public scheme.
    assert_eq!(normalise_url("app.example"), "wss://app.example");
    assert_eq!(normalise_url("  app.example/path  "), "wss://app.example/path");

    // It is a spelling, not a way past the rule: plain http off loopback is
    // still refused, exactly as the ws:// it stands for would be.
    assert!(eui_client::check_url(&normalise_url("http://app.example/x"), false).is_err());
    assert!(eui_client::check_url(&normalise_url("https://app.example/x"), false).is_ok());
}
