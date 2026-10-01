//! `eui package ios`: the published client bundle, made into one
//! application.
//!
//! An iOS `.app` is a directory: the executable, an `Info.plist` and the
//! icons beside it. The release carries one for the simulator and one for a
//! device (unsigned, because a signature is somebody's), and what makes
//! either one an application is the plist's name and identifier, the icons,
//! and `eui.url` next to the executable — the address the client opens
//! instead of the shell.
//!
//! The bundle is written as a directory, because that is what `codesign`
//! signs and what `xcrun simctl install` takes. Signing it is the one step
//! that needs a Mac; everything up to it is done the same way anywhere.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::icon::Icon;
use crate::zip;

/// The file next to the executable that holds the address.
pub const URL_FILE: &str = "eui.url";

/// The icons the bundle names in `CFBundleIconFiles`, and their size in px.
const ICONS: [(&str, u32); 5] = [("AppIcon60x60@2x.png", 120), ("AppIcon60x60@3x.png", 180), ("AppIcon76x76@2x.png", 152), ("AppIcon83.5x83.5@2x.png", 167), ("AppIcon1024.png", 1024)];

/// Write the bundle for `cfg` from the client bundle zip `template` into
/// `out_dir`, and return its path.
pub fn package(template: &[u8], cfg: &Config, out_dir: &Path) -> Result<PathBuf, String> {
    let bundle_id = cfg.ios.bundle_id.clone().ok_or("eui.toml: [ios] bundle_id is required: the bundle identifier, com.example.app")?;
    let icon = cfg.icon.as_deref().map(Icon::load).transpose()?;
    let entries = zip::read(template).map_err(|e| format!("the template: {e}"))?;
    let prefix = entries.iter().find_map(|e| e.name.find(".app/").map(|at| e.name.get(..at + 5).unwrap_or("").to_owned())).ok_or("the template has no .app inside it; is it the EUI client bundle?")?;

    let plist_entry = entries.iter().find(|e| e.name == format!("{prefix}Info.plist")).ok_or("the template's bundle has no Info.plist")?;
    let plist = String::from_utf8(plist_entry.contents()?).map_err(|_| "the template's Info.plist is not XML text")?;
    let executable = get_string(&plist, "CFBundleExecutable").ok_or("the template's Info.plist names no CFBundleExecutable")?;
    let exe = entries.iter().find(|e| e.name == format!("{prefix}{executable}")).ok_or("the template's bundle has no executable")?;
    if !exe.contents()?.windows(URL_FILE.len()).any(|w| w == URL_FILE.as_bytes()) {
        return Err("the template's client does not read eui.url — it is older than `eui package`, and would open the shell instead of the application. Use a client bundle from 0.8.0 or later".into());
    }

    let mut plist = plist;
    plist = set_string(&plist, "CFBundleIdentifier", &bundle_id)?;
    plist = set_string(&plist, "CFBundleName", &cfg.label)?;
    plist = set_string(&plist, "CFBundleDisplayName", &cfg.label)?;
    if let Some(v) = &cfg.version {
        plist = set_string(&plist, "CFBundleShortVersionString", v)?;
        plist = set_string(&plist, "CFBundleVersion", v)?;
    }

    let name: String = cfg.label.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '\0')).collect();
    let app = out_dir.join(format!("{}.app", if name.trim().is_empty() { "App" } else { name.trim() }));
    if app.exists() {
        std::fs::remove_dir_all(&app).map_err(|e| format!("{}: {e}", app.display()))?;
    }
    std::fs::create_dir_all(&app).map_err(|e| format!("{}: {e}", app.display()))?;
    for e in &entries {
        let Some(rel) = e.name.strip_prefix(&prefix) else { continue };
        if rel.is_empty() || rel.ends_with('/') || rel == URL_FILE || rel.starts_with("_CodeSignature/") || rel.split('/').any(|p| p == "..") {
            continue;
        }
        let path = app.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("{}: {err}", parent.display()))?;
        }
        let bytes = if rel == "Info.plist" {
            plist.clone().into_bytes()
        } else if let (Some(icon), Some((_, px))) = (&icon, ICONS.iter().find(|(n, _)| *n == rel)) {
            // iOS composites an icon's transparency onto black; white is
            // what anybody who left a corner clear meant.
            icon.png(*px, Some([255, 255, 255]))?
        } else {
            e.contents()?
        };
        std::fs::write(&path, bytes).map_err(|err| format!("{}: {err}", path.display()))?;
        set_mode(&path, if rel == executable { 0o755 } else { e.unix_mode().unwrap_or(0o644) })?;
    }
    let url = app.join(URL_FILE);
    std::fs::write(&url, format!("{}\n", cfg.url)).map_err(|e| format!("{}: {e}", url.display()))?;
    Ok(app)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_: &Path, _: u32) -> Result<(), String> {
    Ok(())
}

/// An `.ipa` of the bundle at `app`: a zip with the `.app` under `Payload/`.
pub fn ipa(app: &Path) -> Result<Vec<u8>, String> {
    let name = app.file_name().and_then(|n| n.to_str()).ok_or("the bundle has no name")?;
    let mut files = Vec::new();
    walk(app, &mut files)?;
    files.sort();
    let mut entries = Vec::with_capacity(files.len());
    for f in files {
        let rel = f.strip_prefix(app).map_err(|_| "a file outside the bundle")?;
        let rel = rel.to_string_lossy().replace('\\', "/");
        let data = std::fs::read(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        entries.push(zip::Entry::stored(&format!("Payload/{name}/{rel}"), data, mode_of(&f)));
    }
    Ok(zip::write(&entries, |_| 1).bytes)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = e.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).map(|m| 0o100_000 | (m.permissions().mode() & 0o7777)).unwrap_or(0)
}

#[cfg(not(unix))]
fn mode_of(_: &Path) -> u32 {
    0
}

/// The `<string>` after `<key>key</key>`.
fn get_string(plist: &str, key: &str) -> Option<String> {
    let (start, end) = string_span(plist, key)?;
    Some(plist.get(start..end)?.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&"))
}

/// Where the text of the `<string>` after `<key>key</key>` is.
fn string_span(plist: &str, key: &str) -> Option<(usize, usize)> {
    let tag = format!("<key>{key}</key>");
    let after = plist.find(&tag)? + tag.len();
    let rest = plist.get(after..)?;
    let open = rest.find("<string>")?;
    if !rest.get(..open)?.trim().is_empty() {
        return None;
    }
    let start = after + open + "<string>".len();
    let end = start + plist.get(start..)?.find("</string>")?;
    Some((start, end))
}

/// The plist with the `<string>` after `<key>key</key>` set to `value`.
fn set_string(plist: &str, key: &str, value: &str) -> Result<String, String> {
    let (start, end) = string_span(plist, key).ok_or_else(|| format!("the template's Info.plist has no {key} string to set"))?;
    let escaped = value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    Ok(format!("{}{escaped}{}", plist.get(..start).unwrap_or(""), plist.get(end..).unwrap_or("")))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;
    use crate::zip::Entry;

    const PLIST: &str = "<?xml version=\"1.0\"?>\n<plist version=\"1.0\">\n<dict>\n  <key>CFBundleName</key><string>EUI</string>\n  <key>CFBundleDisplayName</key><string>EUI</string>\n  <key>CFBundleIdentifier</key><string>com.soli.eui</string>\n  <key>CFBundleExecutable</key><string>EUI</string>\n  <key>CFBundleVersion</key><string>0.7.1</string>\n  <key>CFBundleShortVersionString</key><string>0.7.1</string>\n</dict>\n</plist>\n";

    #[test]
    fn a_template_becomes_the_application() {
        let tpl = zip::write(
            &[
                Entry::stored("EUI.app/", Vec::new(), 0o040_755),
                Entry::stored("EUI.app/Info.plist", PLIST.as_bytes().to_vec(), 0o100_644),
                Entry::stored("EUI.app/EUI", b"..eui.url..".to_vec(), 0o100_755),
                Entry::stored("EUI.app/AppIcon60x60@2x.png", vec![0u8; 4], 0o100_644),
            ],
            |_| 1,
        );
        let dir = std::env::temp_dir().join(format!("eui-package-ios-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut icon = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut icon, 256, 256);
            enc.set_color(png::ColorType::Rgba);
            enc.write_header().unwrap().write_image_data(&[0u8; 256 * 256 * 4]).unwrap();
        }
        std::fs::write(dir.join("icon.png"), icon).unwrap();
        let cfg = crate::config::parse(
            "[app]\nurl = \"https://mail.test\"\ncomponent = \"inbox\"\nlabel = \"Mail & co\"\nicon = \"icon.png\"\nversion = \"2.0\"\n[ios]\nbundle_id = \"com.example.mail\"\n",
            &dir,
        )
        .unwrap();
        let app = package(&tpl.bytes, &cfg, &dir.join("dist")).unwrap();
        assert_eq!(app.file_name().unwrap(), "Mail & co.app");
        let plist = std::fs::read_to_string(app.join("Info.plist")).unwrap();
        assert_eq!(get_string(&plist, "CFBundleIdentifier").as_deref(), Some("com.example.mail"));
        assert_eq!(get_string(&plist, "CFBundleDisplayName").as_deref(), Some("Mail & co"));
        assert!(plist.contains("<string>Mail &amp; co</string>"));
        assert_eq!(get_string(&plist, "CFBundleExecutable").as_deref(), Some("EUI"));
        assert_eq!(get_string(&plist, "CFBundleVersion").as_deref(), Some("2.0"));
        assert_eq!(std::fs::read_to_string(app.join("eui.url")).unwrap(), "https://mail.test/_eui/session/inbox\n");
        let png = std::fs::read(app.join("AppIcon60x60@2x.png")).unwrap();
        let back = Icon::decode(&png).unwrap();
        assert_eq!(back.size, 120);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(app.join("EUI")).unwrap().permissions().mode() & 0o777, 0o755);
        }
        let ipa = zip::read(&ipa(&app).unwrap()).unwrap();
        assert!(ipa.iter().any(|e| e.name == "Payload/Mail & co.app/eui.url"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
