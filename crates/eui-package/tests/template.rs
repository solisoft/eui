//! `eui package android` on a real client package, for CI to hand to
//! Google's own tools.
//!
//! The unit tests check the package against this crate's reading of the
//! formats; this one makes a package that `apksigner verify` and `aapt2 dump
//! badging` then read, which is the reading that decides whether a phone
//! installs it. It needs a client APK, so it does nothing unless
//! `EUI_PACKAGE_TEMPLATE` names one — the `build-android` job sets it to
//! the APK it has just built, and checks what lands at `EUI_PACKAGE_OUT`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

#[test]
fn a_real_client_package_becomes_an_application() {
    let Some(template) = std::env::var_os("EUI_PACKAGE_TEMPLATE") else {
        eprintln!("EUI_PACKAGE_TEMPLATE is not set; nothing to package");
        return;
    };
    let out = PathBuf::from(std::env::var_os("EUI_PACKAGE_OUT").expect("EUI_PACKAGE_OUT, where the package goes"));
    let dir = std::env::temp_dir().join(format!("eui-package-template-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut icon = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut icon, 512, 512);
        enc.set_color(png::ColorType::Rgb);
        enc.write_header().unwrap().write_image_data(&[0x4f, 0x46, 0xe5].repeat(512 * 512)).unwrap();
    }
    std::fs::write(dir.join("icon.png"), icon).unwrap();
    let cfg = eui_package::config::parse(
        "[app]\nurl = \"https://example.test\"\ncomponent = \"check\"\nlabel = \"Check\"\nicon = \"icon.png\"\nversion = \"9.0\"\n[android]\npackage = \"net.example.check\"\nversion_code = 9\n",
        &dir,
    )
    .unwrap();
    let (key, _) = eui_package::sign::Key::load_or_create(&dir.join("keys")).unwrap();
    let packaged = eui_package::android::package(&std::fs::read(template).unwrap(), &cfg, &key).unwrap();
    std::fs::write(&out, packaged.apk).unwrap();
    eprintln!("{}", out.display());
    let _ = std::fs::remove_dir_all(&dir);
}
