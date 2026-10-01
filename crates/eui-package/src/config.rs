//! `eui.toml`: what an application says about itself as a phone package.
//!
//! ```toml
//! [app]
//! url       = "https://mail.example.com"   # the origin, or a whole session address
//! component = "inbox"                      # the name given to router_eui
//! label     = "Mail"                       # under the icon
//! icon      = "public/icon.png"            # a square PNG, 1024 px is plenty
//! version   = "1.2.0"                      # what the store page would say
//!
//! [android]
//! package      = "com.example.mail"
//! version_code = 3
//!
//! [ios]
//! bundle_id = "com.example.mail"
//! identity  = "Apple Development: you@example.com"
//! ```
//!
//! It is a file of its own rather than a section of `soli.toml`, because
//! `soli.toml`'s parser refuses a section it does not know — so a `[eui]`
//! there would be an application every soli before the one that learned it
//! cannot start.
//!
//! The parser is the subset of TOML that file needs — sections, strings,
//! integers, booleans, comments — and it is strict: a key it does not know
//! is an error with a line number, because a misspelt `bundle_ld` that was
//! quietly ignored would build a package with the wrong identity, and
//! nothing on the phone would say why.

use std::path::{Path, PathBuf};

/// The file's name, next to `soli.toml`.
pub const FILE: &str = "eui.toml";

/// Everything `eui.toml` says, checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The directory the file is in; relative paths in it are relative to this.
    pub dir: PathBuf,
    /// The session address the package opens, composed from `url` and
    /// `component`.
    pub url: String,
    /// The name under the icon.
    pub label: String,
    /// The launcher icon, if the application has one of its own.
    pub icon: Option<PathBuf>,
    /// The human version, `versionName` and `CFBundleShortVersionString`.
    pub version: Option<String>,
    /// `[android]`.
    pub android: Android,
    /// `[ios]`.
    pub ios: Ios,
}

/// `[android]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Android {
    /// The application id: `com.example.mail`. Required for `eui package android`.
    pub package: Option<String>,
    /// The integer Android compares to decide that one package upgrades another.
    pub version_code: Option<u32>,
    /// The client package to start from, instead of the release's.
    pub template: Option<PathBuf>,
    /// The directory holding `key.pk8` and `cert.der`, instead of the person's own.
    pub signing: Option<PathBuf>,
}

/// `[ios]`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ios {
    /// The bundle identifier: `com.example.mail`. Required for `eui package ios`.
    pub bundle_id: Option<String>,
    /// The `codesign` identity, for a device build.
    pub identity: Option<String>,
    /// The provisioning profile to embed, for a device build.
    pub profile: Option<PathBuf>,
    /// The client bundle to start from, instead of the release's.
    pub template: Option<PathBuf>,
}

/// One value, as the file wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Str(String),
    Int(i64),
    Bool(bool),
}

impl Value {
    fn kind(&self) -> &'static str {
        match self {
            Self::Str(_) => "a string",
            Self::Int(_) => "an integer",
            Self::Bool(_) => "a boolean",
        }
    }
}

/// Read `eui.toml` in `dir`.
pub fn load(dir: &Path) -> Result<Config, String> {
    let path = dir.join(FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text, dir).map_err(|e| format!("{}{e}", path.display()))
}

/// Parse the text of an `eui.toml` whose directory is `dir`. An error starts
/// with `:<line>: ` where there is a line to blame, so that a caller can put
/// the file's name in front of it.
pub fn parse(text: &str, dir: &Path) -> Result<Config, String> {
    let mut url: Option<String> = None;
    let mut component: Option<String> = None;
    let mut label: Option<String> = None;
    let mut icon: Option<PathBuf> = None;
    let mut version: Option<String> = None;
    let mut android = Android::default();
    let mut ios = Ios::default();
    let mut seen: Vec<(String, String)> = Vec::new();

    let mut section = String::new();
    for (n, raw) in text.lines().enumerate() {
        let line_no = n.saturating_add(1);
        let at = |e: String| format!(":{line_no}: {e}");
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('[') {
            let (name, tail) = rest.split_once(']').ok_or_else(|| at("a section header is `[name]`".into()))?;
            if !tail.trim().is_empty() && !tail.trim_start().starts_with('#') {
                return Err(at(format!("unexpected `{}` after the section header", tail.trim())));
            }
            let name = name.trim();
            if !matches!(name, "app" | "android" | "ios") {
                return Err(at(format!("unknown section [{name}]: eui.toml has [app], [android] and [ios]")));
            }
            section = name.to_owned();
            continue;
        }
        let (key, rest) = line.split_once('=').ok_or_else(|| at("expected `key = value`".into()))?;
        let key = key.trim();
        if section.is_empty() {
            return Err(at(format!("`{key}` is outside any section; it belongs under [app], [android] or [ios]")));
        }
        let value = parse_value(rest.trim()).map_err(at)?;
        if seen.iter().any(|(s, k)| *s == section && k == key) {
            return Err(at(format!("[{section}] {key} is given twice")));
        }
        seen.push((section.clone(), key.to_owned()));
        let want_str = |v: &Value| match v {
            Value::Str(s) => Ok(s.clone()),
            other => Err(at(format!("[{section}] {key} is a string, not {}", other.kind()))),
        };
        let want_path = |v: &Value| want_str(v).map(|s| dir.join(s));
        match (section.as_str(), key) {
            ("app", "url") => url = Some(want_str(&value)?),
            ("app", "component") => component = Some(want_str(&value)?),
            ("app", "label") => label = Some(want_str(&value)?),
            ("app", "icon") => icon = Some(want_path(&value)?),
            ("app", "version") => version = Some(want_str(&value)?),
            ("android", "package") => android.package = Some(want_str(&value)?),
            ("android", "version_code") => match value {
                Value::Int(i) => android.version_code = Some(u32::try_from(i).ok().filter(|v| *v >= 1 && *v <= 2_100_000_000).ok_or_else(|| at("[android] version_code is 1 to 2100000000".into()))?),
                other => return Err(at(format!("[android] version_code is an integer, not {}", other.kind()))),
            },
            ("android", "template") => android.template = Some(want_path(&value)?),
            ("android", "signing") => android.signing = Some(want_path(&value)?),
            ("ios", "bundle_id") => ios.bundle_id = Some(want_str(&value)?),
            ("ios", "identity") => ios.identity = Some(want_str(&value)?),
            ("ios", "profile") => ios.profile = Some(want_path(&value)?),
            ("ios", "template") => ios.template = Some(want_path(&value)?),
            (s, k) => return Err(at(format!("unknown key `{k}` in [{s}]"))),
        }
    }

    let url = url.ok_or(": [app] url is required: the address the package opens")?;
    let label = label.ok_or(": [app] label is required: the name under the icon")?;
    let url = session_url(&url, component.as_deref())?;
    if label.trim().is_empty() || label.chars().count() > 50 {
        return Err(": [app] label is 1 to 50 characters".into());
    }
    if let Some(p) = &android.package {
        check_android_package(p)?;
    }
    if let Some(b) = &ios.bundle_id {
        check_bundle_id(b)?;
    }
    Ok(Config { dir: dir.to_owned(), url, label, icon, version, android, ios })
}

/// The address the package opens: `url` itself when it already names a
/// session, else `url` with `/_eui/session/<component>` after it — the path
/// `router_eui` serves a component at.
fn session_url(url: &str, component: Option<&str>) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    let (scheme, rest) = url.split_once("://").ok_or(": [app] url needs a scheme: https://… (or wss://…)")?;
    if !matches!(scheme, "https" | "wss" | "http" | "ws") {
        return Err(format!(": [app] url is https://, wss://, http:// or ws://, not {scheme}://"));
    }
    if rest.split('/').next().unwrap_or("").is_empty() {
        return Err(": [app] url has no host".into());
    }
    match component {
        Some(c) => {
            if c.is_empty() || !c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
                return Err(format!(": [app] component {c:?} is a name, letters, digits, _ and -"));
            }
            if url.contains("/_eui/session/") {
                return Err(": [app] url already names a session; leave component out, or give the origin alone".into());
            }
            Ok(format!("{url}/_eui/session/{c}"))
        }
        None if url.contains("/_eui/session/") => Ok(url.to_owned()),
        None => Err(": [app] component is required unless url is a whole session address (…/_eui/session/<name>)".into()),
    }
}

/// An Android application id: two or more dot-separated segments, each a
/// letter followed by letters, digits and underscores.
pub fn check_android_package(p: &str) -> Result<(), String> {
    let segments: Vec<&str> = p.split('.').collect();
    let ok = segments.len() >= 2
        && segments.iter().all(|s| {
            let mut c = s.chars();
            c.next().is_some_and(|f| f.is_ascii_alphabetic()) && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        });
    if ok {
        Ok(())
    } else {
        Err(format!(": [android] package {p:?} is an application id: com.example.app — two or more parts, each starting with a letter"))
    }
}

/// An iOS bundle identifier: letters, digits, `-` and `.`, with a dot in it.
pub fn check_bundle_id(b: &str) -> Result<(), String> {
    if b.contains('.') && !b.starts_with('.') && !b.ends_with('.') && b.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.') {
        Ok(())
    } else {
        Err(format!(": [ios] bundle_id {b:?} is a reverse-DNS name: com.example.app"))
    }
}

/// A value and nothing after it but a comment.
fn parse_value(s: &str) -> Result<Value, String> {
    let (value, rest) = if let Some(body) = s.strip_prefix('"') {
        let (v, rest) = basic_string(body)?;
        (Value::Str(v), rest)
    } else if let Some(body) = s.strip_prefix('\'') {
        let end = body.find('\'').ok_or("a '…' string is not closed")?;
        (Value::Str(body.get(..end).unwrap_or("").to_owned()), body.get(end.saturating_add(1)..).unwrap_or(""))
    } else {
        let end = s.find('#').unwrap_or(s.len());
        let word = s.get(..end).unwrap_or("").trim();
        let v = match word {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            w => Value::Int(w.replace('_', "").parse::<i64>().map_err(|_| format!("`{w}` is not a value: a string is quoted"))?),
        };
        (v, s.get(end..).unwrap_or(""))
    };
    let rest = rest.trim();
    if rest.is_empty() || rest.starts_with('#') {
        Ok(value)
    } else {
        Err(format!("unexpected `{rest}` after the value"))
    }
}

/// The inside of a `"…"` string, and what follows its closing quote.
fn basic_string(body: &str) -> Result<(String, &str), String> {
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, body.get(i.saturating_add(1)..).unwrap_or(""))),
            '\\' => {
                let (_, e) = chars.next().ok_or("a string ends in a backslash")?;
                match e {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'u' | 'U' => {
                        let n = if e == 'u' { 4 } else { 8 };
                        let hex: String = (0..n).filter_map(|_| chars.next().map(|(_, h)| h)).collect();
                        let ch = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).ok_or_else(|| format!("\\{e}{hex} is not a character"))?;
                        out.push(ch);
                    }
                    other => return Err(format!("\\{other} is not an escape TOML has")),
                }
            }
            c => out.push(c),
        }
    }
    Err("a \"…\" string is not closed".into())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn cfg(text: &str) -> Result<Config, String> {
        parse(text, Path::new("/app"))
    }

    #[test]
    fn the_whole_file_reads() {
        let c = cfg(r#"
# a comment
[app]
url = "https://mail.example.com/"   # trailing slash is dropped
component = "inbox"
label = "Mail é"
icon = "public/icon.png"
version = '1.2.0'

[android]
package = "com.example.mail"
version_code = 3

[ios]
bundle_id = "com.example.mail"
"#)
        .unwrap();
        assert_eq!(c.url, "https://mail.example.com/_eui/session/inbox");
        assert_eq!(c.label, "Mail é");
        assert_eq!(c.icon, Some(PathBuf::from("/app/public/icon.png")));
        assert_eq!(c.version.as_deref(), Some("1.2.0"));
        assert_eq!(c.android.package.as_deref(), Some("com.example.mail"));
        assert_eq!(c.android.version_code, Some(3));
        assert_eq!(c.ios.bundle_id.as_deref(), Some("com.example.mail"));
    }

    #[test]
    fn a_session_address_needs_no_component() {
        let c = cfg("[app]\nurl = \"wss://h.test/_eui/session/x\"\nlabel = \"X\"\n").unwrap();
        assert_eq!(c.url, "wss://h.test/_eui/session/x");
    }

    #[test]
    fn a_misspelt_key_is_an_error_with_its_line() {
        let e = cfg("[app]\nurl = \"https://h.test\"\ncomponent = \"x\"\nlabel = \"X\"\n[ios]\nbundle_ld = \"a.b\"\n").unwrap_err();
        assert_eq!(e, ":6: unknown key `bundle_ld` in [ios]");
    }

    #[test]
    fn what_is_refused() {
        for (text, needle) in [
            ("[app]\nlabel = \"X\"\n", "url is required"),
            ("[app]\nurl = \"https://h.test\"\nlabel = \"X\"\n", "component is required"),
            ("[app]\nurl = \"ftp://h.test\"\ncomponent = \"x\"\nlabel = \"X\"\n", "not ftp://"),
            ("[app]\nurl = \"https://h.test\"\ncomponent = \"x\"\nlabel = \"X\"\n[android]\npackage = \"mail\"\n", "application id"),
            ("[app]\nurl = \"https://h.test\"\ncomponent = \"x\"\nlabel = \"X\"\n[android]\nversion_code = 0\n", "version_code is 1"),
            ("[app]\nurl = \"https://h.test\"\ncomponent = \"x\"\nlabel = \"X\"\nlabel = \"Y\"\n", "given twice"),
            ("[eui]\n", "unknown section"),
            ("url = \"x\"\n", "outside any section"),
            ("[app]\nurl = https://h.test\n", "is not a value"),
            ("[app]\nurl = \"open\n", "not closed"),
        ] {
            let e = cfg(text).unwrap_err();
            assert!(e.contains(needle), "{text:?} gave {e:?}, wanted {needle:?}");
        }
    }
}
