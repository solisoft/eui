//! `eui package android`: the published client package, made into one
//! application.
//!
//! The client APK the release carries is a whole application already — the
//! shared object, the C++ runtime beside it, a manifest, a resource table
//! and an icon. What makes it somebody's application is five things, and
//! none of them needs a compiler:
//!
//! 1. the manifest's package name, label and version ([`crate::axml`]);
//! 2. the launcher icon, the five `res/mipmap-*/ic_launcher.png` the
//!    resource table already points at ([`crate::icon`]);
//! 3. `assets/eui.url`, the address the client opens instead of the shell;
//! 4. the alignment `zipalign` would give it ([`crate::zip`]);
//! 5. a signature over the result ([`crate::sign`]).
//!
//! So the package is made from the release's bytes, and an application
//! needs neither the SDK, the NDK nor this repository to have one.

use crate::axml;
use crate::config::Config;
use crate::icon::Icon;
use crate::sign::{self, Key};
use crate::zip::{self, Entry};

/// Where the address goes, inside the package. The client reads it with
/// `AAssetManager_open("eui.url")` — `assets/` is the asset manager's root.
pub const URL_ASSET: &str = "assets/eui.url";

/// The densities a launcher icon comes in, by the directory name aapt gives
/// each one, and its size in px. Longest name first, so that `xxxhdpi` is
/// not read as `xhdpi`.
const DENSITIES: [(&str, u32); 5] = [("xxxhdpi", 192), ("xxhdpi", 144), ("xhdpi", 96), ("hdpi", 72), ("mdpi", 48)];

/// What came out, for the caller to report.
pub struct Packaged {
    /// The signed APK.
    pub apk: Vec<u8>,
    /// The package name it was made with.
    pub package: String,
}

/// Make the APK for `cfg` from the client package `template`, signed with `key`.
pub fn package(template: &[u8], cfg: &Config, key: &Key) -> Result<Packaged, String> {
    let package = cfg.android.package.clone().ok_or("eui.toml: [android] package is required: the application id, com.example.app")?;
    let icon = cfg.icon.as_deref().map(Icon::load).transpose()?;
    let entries = zip::read(template).map_err(|e| format!("the template: {e}"))?;

    let manifest = entries.iter().find(|e| e.name == "AndroidManifest.xml").ok_or("the template has no AndroidManifest.xml; is it an APK?")?;
    let lib = entries.iter().find(|e| e.name.starts_with("lib/") && e.name.ends_with("/libeui.so")).ok_or("the template has no lib/*/libeui.so; is it the EUI client?")?;
    // A client from before `eui package` does not look for the address and
    // would open the shell. Better to say so here than on somebody's phone.
    let so = lib.contents().map_err(|e| format!("the template: {e}"))?;
    if !so.windows(b"eui.url".len()).any(|w| w == b"eui.url") {
        return Err(
            "the template's client does not read eui.url — it is older than `eui package`, and would open the shell instead of the application. Use a client package from 0.8.0 or later".into()
        );
    }

    let patch = axml::Patch { package: Some(package.clone()), label: Some(cfg.label.clone()), version_name: cfg.version.clone(), version_code: cfg.android.version_code };
    let manifest = axml::patch(&manifest.contents().map_err(|e| format!("the template: {e}"))?, &patch).map_err(|e| format!("the template's manifest: {e}"))?;

    let mut out: Vec<Entry> = vec![Entry::stored("AndroidManifest.xml", manifest, 0)];
    let mut icons = 0;
    for e in entries {
        // The template's own signature covered the template, and the
        // address it carried, if any, was somebody else's.
        if e.name == "AndroidManifest.xml" || e.name.starts_with("META-INF/") || e.name == URL_ASSET {
            continue;
        }
        if let (Some(icon), Some(px)) = (&icon, launcher_size(&e.name)) {
            out.push(Entry::stored(&e.name, icon.png(px, None)?, 0));
            icons += 1;
            continue;
        }
        out.push(e);
    }
    if icon.is_some() && icons == 0 {
        return Err("the template has no res/mipmap-*/ic_launcher.png to replace".into());
    }
    out.push(Entry::stored(URL_ASSET, format!("{}\n", cfg.url).into_bytes(), 0));

    let written = zip::write(&out, |e| if e.name.ends_with(".so") { 4096 } else { 4 });
    let apk = sign::sign(&written.bytes, written.cd_offset, key)?;
    Ok(Packaged { apk, package })
}

/// The size an entry is, if it is one of the launcher icons.
fn launcher_size(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("res/mipmap-")?;
    let (dir, file) = rest.split_once('/')?;
    if file != "ic_launcher.png" {
        return None;
    }
    DENSITIES.iter().find(|(d, _)| dir.starts_with(d)).map(|(_, px)| *px)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn the_icons_are_found_by_density() {
        assert_eq!(launcher_size("res/mipmap-xxxhdpi-v4/ic_launcher.png"), Some(192));
        assert_eq!(launcher_size("res/mipmap-xhdpi-v4/ic_launcher.png"), Some(96));
        assert_eq!(launcher_size("res/mipmap-mdpi/ic_launcher.png"), Some(48));
        assert_eq!(launcher_size("res/mipmap-hdpi-v4/other.png"), None);
        assert_eq!(launcher_size("res/drawable/ic_launcher.png"), None);
    }

    /// A template as small as one can be and still be the client's shape:
    /// the real manifest, one icon, and a "shared object" that mentions the
    /// asset — then everything the package step promises, checked.
    #[test]
    fn a_template_becomes_the_application() {
        let tpl = zip::write(
            &[
                Entry::stored("AndroidManifest.xml", include_bytes!("../tests/AndroidManifest-0.7.1.bin").to_vec(), 0),
                Entry::stored("res/mipmap-hdpi-v4/ic_launcher.png", vec![0u8; 10], 0),
                Entry::stored("resources.arsc", vec![1u8; 10], 0),
                Entry::stored("lib/arm64-v8a/libeui.so", b"....eui.url....".to_vec(), 0o755),
                Entry::stored("META-INF/ANDROIDD.SF", vec![2u8; 10], 0),
            ],
            |_| 4,
        );
        let dir = std::env::temp_dir().join(format!("eui-package-android-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut icon = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut icon, 256, 256);
            enc.set_color(png::ColorType::Rgb);
            enc.write_header().unwrap().write_image_data(&[9u8; 256 * 256 * 3]).unwrap();
        }
        std::fs::write(dir.join("icon.png"), icon).unwrap();
        let cfg = crate::config::parse(
            "[app]\nurl = \"https://mail.test\"\ncomponent = \"inbox\"\nlabel = \"Mail\"\nicon = \"icon.png\"\nversion = \"2.0\"\n[android]\npackage = \"com.example.mail\"\nversion_code = 7\n",
            &dir,
        )
        .unwrap();
        let (key, _) = Key::load_or_create(&dir.join("keys")).unwrap();
        let out = package(&tpl.bytes, &cfg, &key).unwrap();

        let entries = zip::read(&out.apk).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["AndroidManifest.xml", "res/mipmap-hdpi-v4/ic_launcher.png", "resources.arsc", "lib/arm64-v8a/libeui.so", "assets/eui.url"]);
        let m = axml::read(&entries[0].contents().unwrap()).unwrap();
        assert_eq!(m.package.as_deref(), Some("com.example.mail"));
        assert_eq!(m.label.as_deref(), Some("Mail"));
        assert_eq!(m.version_name.as_deref(), Some("2.0"));
        assert_eq!(m.version_code, Some(7));
        assert_eq!(Icon::decode(&entries[1].contents().unwrap()).unwrap().size, 72);
        assert_eq!(entries[4].contents().unwrap(), b"https://mail.test/_eui/session/inbox\n");

        // A client that does not read the asset is refused, not packaged.
        let old = zip::write(
            &[Entry::stored("AndroidManifest.xml", include_bytes!("../tests/AndroidManifest-0.7.1.bin").to_vec(), 0), Entry::stored("lib/arm64-v8a/libeui.so", b"nothing".to_vec(), 0)],
            |_| 4,
        );
        assert!(package(&old.bytes, &cfg, &key).err().unwrap().contains("older than `eui package`"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
