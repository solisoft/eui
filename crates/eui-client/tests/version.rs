//! Which build this is, as `eui --version` says it and as the install
//! script reads it out of a file it will not run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

#[test]
fn the_stamp_names_the_protocol_this_build_speaks() {
    // The build script reads the number out of eui-proto's source, because
    // it cannot see the constant; this is what keeps the two the same.
    let stamp = eui_client::STAMP;
    assert!(stamp.starts_with("eui-build: eui "), "{stamp}");
    assert!(stamp.ends_with(';'), "{stamp}");
    assert!(stamp.contains(&format!(", protocol {}, ", eui_proto::PROTOCOL_VERSION)), "{stamp}");
    assert!(stamp.contains(&format!("commit {};", eui_client::BUILD)), "{stamp}");
    assert_eq!(eui_client::version(), &stamp["eui-build: ".len()..stamp.len() - 1]);
}

#[test]
fn the_binary_says_its_version_and_carries_it_where_a_script_can_find_it() {
    let exe = env!("CARGO_BIN_EXE_eui");
    let out = std::process::Command::new(exe).arg("--version").output().expect("run eui --version");
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim_end(), eui_client::version());
    // `scripts/install.sh` does not run the binary it replaces: an older
    // one takes `--version` for an address. It finds these bytes instead.
    let bytes = std::fs::read(exe).unwrap();
    let needle = eui_client::STAMP.as_bytes();
    assert!(bytes.windows(needle.len()).any(|w| w == needle), "the stamp is not in the binary as one run of bytes");
}

#[test]
fn the_minor_version_is_the_protocol_version() {
    // CHANGELOG.md's rule: a release's minor number is the protocol it
    // speaks, so `v0.7.x` speaks 7. A protocol bump that forgets the
    // version -- or a version bump that forgets the protocol -- fails here,
    // before a tag can publish a release that names the wrong one.
    let version = env!("CARGO_PKG_VERSION");
    let minor: u32 = version.split('.').nth(1).and_then(|m| m.parse().ok()).expect("a semver version");
    assert_eq!(minor, eui_proto::PROTOCOL_VERSION, "eui {version} speaks protocol {}", eui_proto::PROTOCOL_VERSION);
}
