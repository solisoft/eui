//! The manifest check: signature, protocol range, trust on first use, key
//! rotation. Keys are generated here with ring; the pin store is a temp dir.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use eui_client::manifest::{verify, ManifestError};
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
    let got = verify(&signed(&m, &k), &pins).unwrap();
    assert_eq!(got, m);
    let pin = std::fs::read_dir(&pins).unwrap().next().unwrap().unwrap().path();
    assert_eq!(std::fs::read(&pin).unwrap(), public(&k).to_vec(), "the pin is the raw public key");
    // The same key again: fine. A tampered field: the signature fails.
    assert!(verify(&signed(&m, &k), &pins).is_ok());
    let mut bytes = signed(&m, &k);
    bytes[10] ^= 1;
    assert_eq!(verify(&bytes, &pins).unwrap_err(), ManifestError::BadSignature);
    // A stranger with a valid manifest of their own for the same app_id: refused against the pin.
    let other = keypair();
    let stranger = signed(&Manifest { publisher_key: public(&other), capabilities: caps::CAMERA, ..m.clone() }, &other);
    assert_eq!(verify(&stranger, &pins).unwrap_err(), ManifestError::KeyChanged);
}

#[test]
fn a_changed_key_is_refused_unless_the_old_key_signed_the_rotation() {
    let pins = tmp("rotate");
    let old = keypair();
    let new = keypair();
    let first = Manifest { app_id: "demo".into(), publisher_key: public(&old), ..speakable() };
    verify(&signed(&first, &old), &pins).unwrap();
    let moved = Manifest { publisher_key: public(&new), ..first.clone() };
    assert_eq!(verify(&signed(&moved, &new), &pins).unwrap_err(), ManifestError::KeyChanged);
    // A rotation signed by the wrong key does not help.
    let mut bad_sig = [0u8; 64];
    bad_sig.copy_from_slice(new.sign(&public(&new)).as_ref());
    let bad = Manifest { rotation: Some(Rotation { previous_key: public(&old), signature: bad_sig }), ..moved.clone() };
    assert_eq!(verify(&signed(&bad, &new), &pins).unwrap_err(), ManifestError::KeyChanged);
    // The old key vouching for the new one: accepted, and the pin moves.
    let mut sig = [0u8; 64];
    sig.copy_from_slice(old.sign(&public(&new)).as_ref());
    let good = Manifest { rotation: Some(Rotation { previous_key: public(&old), signature: sig }), ..moved.clone() };
    assert!(verify(&signed(&good, &new), &pins).is_ok());
    assert!(verify(&signed(&moved, &new), &pins).is_ok(), "pinned to the new key now");
    assert_eq!(verify(&signed(&first, &old), &pins).unwrap_err(), ManifestError::KeyChanged, "and the old one is refused");
}

#[test]
fn a_server_outside_this_clients_protocol_is_refused() {
    let pins = tmp("proto");
    let k = keypair();
    // A range that starts past what this client speaks, whatever it speaks.
    let (min, max) = (PROTOCOL_VERSION + 1, PROTOCOL_VERSION + 2);
    let m = Manifest { app_id: "demo".into(), publisher_key: public(&k), protocol_min: min, protocol_max: max, ..Manifest::default() };
    assert_eq!(verify(&signed(&m, &k), &pins).unwrap_err(), ManifestError::Protocol { min, max });
    assert!(!pins.exists(), "nothing pinned for a server we cannot talk to");
}

#[test]
fn garbage_is_not_a_manifest() {
    let pins = tmp("garbage");
    assert!(matches!(verify(b"<!doctype html>", &pins).unwrap_err(), ManifestError::Decode(_)));
}
