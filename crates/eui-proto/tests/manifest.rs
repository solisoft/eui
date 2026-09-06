//! The manifest record: canonical bytes, strict decoding.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use eui_proto::manifest::{key, MAGIC};
use eui_proto::{caps, DecodeError, Manifest, Rotation};

fn sample() -> Manifest {
    Manifest {
        app_id: "counter-app".into(),
        name: "Counter".into(),
        version: "1.2.0".into(),
        protocol_min: 1,
        protocol_max: 1,
        publisher_key: [7; 32],
        capabilities: caps::CLIPBOARD_READ | caps::NOTIFICATIONS,
        theme: Some([9; 32]),
        entry: "/_eui/session".into(),
        rotation: Some(Rotation { previous_key: [1; 32], signature: [2; 64] }),
    }
}

#[test]
fn a_manifest_round_trips_and_its_signed_bytes_are_the_record_minus_the_signature() {
    let m = sample();
    let sig = [3u8; 64];
    let bytes = m.encode(&sig);
    assert_eq!(&bytes[..4], &MAGIC);
    let (back, back_sig) = Manifest::decode(&bytes).unwrap();
    assert_eq!(back, m);
    assert_eq!(back_sig, sig);
    let signed = m.signed_bytes();
    assert!(bytes.starts_with(&signed[6..]) || bytes[6..].starts_with(&signed[6..]), "same fields, in order");
    assert_eq!(signed[5], 10, "ten signed fields");
    assert_eq!(bytes[5], 11, "eleven with the signature");
    assert_eq!(back.signed_bytes(), signed, "a decoder rebuilds exactly what was signed");
    // A minimal manifest: no theme, no rotation.
    let m = Manifest { app_id: "x".into(), ..Manifest::default() };
    let bytes = m.encode(&[0; 64]);
    assert_eq!(Manifest::decode(&bytes).unwrap().0, m);
    assert!(bytes.len() < 260, "{} bytes", bytes.len());
}

#[test]
fn malformed_manifests_are_refused() {
    let m = sample();
    let good = m.encode(&[3; 64]);
    let mut bad = good.clone();
    bad[0] = b'X';
    assert_eq!(Manifest::decode(&bad).unwrap_err(), DecodeError::UnknownTag("manifest magic"));
    let mut bad = good.clone();
    bad[4] = 2;
    assert_eq!(Manifest::decode(&bad).unwrap_err(), DecodeError::UnknownTag("manifest version"));
    // Trailing bytes.
    let mut bad = good.clone();
    bad.push(0);
    assert_eq!(Manifest::decode(&bad).unwrap_err(), DecodeError::TrailingBytes);
    // Truncated anywhere.
    for cut in [3, 6, 20, good.len() - 1] {
        assert!(Manifest::decode(&good[..cut]).is_err(), "cut at {cut}");
    }
    // The signature must be present: the signed bytes alone are not a manifest.
    assert!(Manifest::decode(&m.signed_bytes()).is_err());
    // Bad values.
    for (bad, what) in [
        (Manifest { app_id: String::new(), ..sample() }, "empty app_id"),
        (Manifest { protocol_min: 0, ..sample() }, "protocol_min 0"),
        (Manifest { protocol_max: 0, ..sample() }, "max below min"),
        (Manifest { entry: "relative".into(), ..sample() }, "relative entry"),
        (Manifest { name: "n".repeat(300), ..sample() }, "long string"),
    ] {
        assert!(Manifest::decode(&bad.encode(&[0; 64])).is_err(), "{what}");
    }
    let _ = key::SIGNATURE;
}

#[test]
fn capability_names_map_to_bits_and_back() {
    assert_eq!(caps::from_name("clipboard.read"), Some(caps::CLIPBOARD_READ));
    assert_eq!(caps::from_name("root"), None);
    assert_eq!(caps::names(caps::CAMERA | caps::FS_PICK), vec!["camera", "fs.pick"]);
}
