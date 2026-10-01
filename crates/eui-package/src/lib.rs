//! `eui package android|ios`: a phone package for one application, made
//! from the published client and the application's `eui.toml`.
//!
//! The client on a phone is the same client everywhere: one shared object
//! on Android, one executable on iOS, and neither knows which application
//! it is for until it is told. So a package is not compiled, it is
//! *assembled* — the release's client package, given a name, an identifier,
//! an icon and the address to open — and the person making it needs neither
//! the Android SDK, the NDK, Xcode nor this repository. On iOS the one
//! exception is the signature, which only a Mac can apply.
//!
//! ```text
//! cd mail-app            # a Soli application, with an eui.toml beside soli.toml
//! eui package android    # → dist/com.example.mail.apk
//! eui package ios --sim  # → dist/Mail.app
//! ```
//!
//! The address is a file in the package — `assets/eui.url` in an APK,
//! `eui.url` beside the executable in a bundle — which the client reads
//! before anything else and opens instead of the shell.

// Offsets in two binary formats: every sum here is of lengths the code has
// just measured, in archives a few megabytes long, and the reads are all
// `get`. The lint would turn each into a `checked_add` that cannot fail.
#![allow(clippy::arithmetic_side_effects)]

pub mod android;
pub mod axml;
pub mod config;
pub mod icon;
pub mod ios;
pub mod sign;
pub mod zip;

use std::path::{Path, PathBuf};

/// The version of the client the release's templates are of: this one.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the releases are.
const RELEASES: &str = "https://github.com/solisoft/eui/releases/download";

/// Which phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    Android,
    IosDevice,
    IosSim,
}

impl Target {
    /// The release asset this target is assembled from.
    fn asset(self) -> &'static str {
        match self {
            Self::Android => "eui.apk",
            Self::IosDevice => "EUI-ios-device-unsigned.zip",
            Self::IosSim => "EUI-ios-simulator.zip",
        }
    }
}

const USAGE: &str = "usage: eui package android [dir] [--from <eui.apk>] [--release <tag>] [--out <dir>]
       eui package ios [dir] [--sim] [--from <bundle.zip>] [--release <tag>] [--out <dir>]

  dir        the application, where eui.toml is (default: here)
  --from     the client package to start from, instead of the release's
  --release  the release to take it from (default: v{VERSION}; `rolling` is main)
  --out      where the package goes (default: <dir>/dist)
  --sim      the iOS simulator rather than a device";

/// `eui package …`, with `args` the words after `package`. `config_dir` is
/// the client's own corner of the person's configuration: the signing key
/// and the downloaded templates live there. Returns the exit status.
pub fn cli(args: &[String], config_dir: Option<PathBuf>) -> i32 {
    match run(args, config_dir) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("eui package: {e}");
            1
        }
    }
}

fn run(args: &[String], config_dir: Option<PathBuf>) -> Result<(), String> {
    let usage = || USAGE.replace("{VERSION}", VERSION);
    let mut it = args.iter();
    let platform = it.next().ok_or_else(usage)?;
    let mut dir: Option<PathBuf> = None;
    let mut from: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut release = format!("v{VERSION}");
    let mut sim = false;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--from" => from = Some(PathBuf::from(it.next().ok_or_else(usage)?)),
            "--out" => out = Some(PathBuf::from(it.next().ok_or_else(usage)?)),
            "--release" => release = it.next().ok_or_else(usage)?.clone(),
            "--sim" => sim = true,
            "-h" | "--help" => {
                println!("{}", usage());
                return Ok(());
            }
            s if s.starts_with('-') => return Err(format!("unknown option {s}\n{}", usage())),
            s if dir.is_none() => dir = Some(PathBuf::from(s)),
            s => return Err(format!("one application at a time; {s} is a second\n{}", usage())),
        }
    }
    let target = match (platform.as_str(), sim) {
        ("android", false) => Target::Android,
        ("android", true) => return Err("--sim is for ios; an APK installs on an emulator as it is".into()),
        ("ios", false) => Target::IosDevice,
        ("ios", true) => Target::IosSim,
        (p, _) => return Err(format!("{p} is not a platform: android or ios\n{}", usage())),
    };
    let dir = dir.unwrap_or_else(|| PathBuf::from("."));
    let cfg = config::load(&dir)?;
    let out = out.unwrap_or_else(|| dir.join("dist"));
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;

    let own = match target {
        Target::Android => cfg.android.template.clone(),
        Target::IosDevice | Target::IosSim => cfg.ios.template.clone(),
    };
    let template_path = match from.or(own) {
        Some(p) => p,
        None => fetch_template(target, &release, config_dir.as_deref())?,
    };
    let template = std::fs::read(&template_path).map_err(|e| format!("{}: {e}", template_path.display()))?;
    println!("eui package: {} from {}", cfg.label, template_path.display());
    println!("  opens {}", cfg.url);

    match target {
        Target::Android => {
            let signing = match &cfg.android.signing {
                Some(d) => d.clone(),
                None => config_dir.ok_or("no configuration directory for the signing key; set [android] signing in eui.toml")?.join("android-signing"),
            };
            let (key, made) = sign::Key::load_or_create(&signing)?;
            if made {
                println!("  made a signing key in {} — keep it: a package signed with another key will not upgrade this one", signing.display());
            }
            let packaged = android::package(&template, &cfg, &key)?;
            let path = out.join(format!("{}.apk", packaged.package));
            std::fs::write(&path, &packaged.apk).map_err(|e| format!("{}: {e}", path.display()))?;
            println!("  signed by {}", key.fingerprint());
            println!("{}", path.display());
            println!("  adb install -r '{}'", path.display());
        }
        Target::IosDevice | Target::IosSim => {
            let app = ios::package(&template, &cfg, &out)?;
            if target == Target::IosSim {
                println!("{}", app.display());
                println!("  xcrun simctl install booted '{}'", app.display());
                return Ok(());
            }
            match &cfg.ios.identity {
                Some(identity) => sign_ios(&app, identity, cfg.ios.profile.as_deref(), &out)?,
                None => {
                    println!("{}", app.display());
                    println!("  unsigned: set [ios] identity in eui.toml to sign it, or let Xcode sign it on the way to the device");
                }
            }
        }
    }
    Ok(())
}

/// Sign the bundle with `codesign` and wrap it in an `.ipa`. A Mac's job.
fn sign_ios(app: &Path, identity: &str, profile: Option<&Path>, out: &Path) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        println!("{}", app.display());
        return Err("[ios] identity is set, and signing an iOS bundle takes codesign, which is a Mac's; the bundle above is unsigned".into());
    }
    if let Some(p) = profile {
        std::fs::copy(p, app.join("embedded.mobileprovision")).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    let status = std::process::Command::new("codesign").args(["--force", "--sign", identity, "--timestamp=none"]).arg(app).status().map_err(|e| format!("codesign: {e}"))?;
    if !status.success() {
        return Err(format!("codesign failed ({status})"));
    }
    let name = app.file_stem().and_then(|s| s.to_str()).unwrap_or("App");
    let ipa = out.join(format!("{name}.ipa"));
    std::fs::write(&ipa, ios::ipa(app)?).map_err(|e| format!("{}: {e}", ipa.display()))?;
    println!("{}", app.display());
    println!("{}", ipa.display());
    Ok(())
}

/// The release's client package for `target`, downloaded once into the
/// configuration directory and reused after. `rolling` is fetched every
/// time, because that tag moves.
fn fetch_template(target: Target, release: &str, config_dir: Option<&Path>) -> Result<PathBuf, String> {
    let cache = config_dir.ok_or("no configuration directory to keep the client package in; pass --from")?.join("templates").join(release);
    let path = cache.join(target.asset());
    if path.exists() && release != "rolling" {
        return Ok(path);
    }
    std::fs::create_dir_all(&cache).map_err(|e| format!("{}: {e}", cache.display()))?;
    let url = format!("{RELEASES}/{release}/{}", target.asset());
    println!("eui package: fetching {url}");
    // GitHub answers a release download with a redirect to its storage, and
    // the client's own fetch follows none, on purpose. So this is curl's,
    // which every desktop the client runs on has.
    let part = cache.join(format!("{}.part", target.asset()));
    let status = std::process::Command::new("curl")
        .args(["-fL", "--retry", "2", "-o"])
        .arg(&part)
        .arg(&url)
        .status()
        .map_err(|e| format!("could not run curl ({e}); download {url} yourself and pass --from"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(format!("could not download {url}; if release {release} has not been published, pass --release rolling or --from <file>"));
    }
    std::fs::rename(&part, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}
