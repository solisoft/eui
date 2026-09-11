//! What build this is, baked in at compile time — and, on Windows, the
//! executable's icon.
//!
//! A person looking at a window cannot tell one build from another, and a
//! demo downloaded from the wrong run looks exactly like the right one --
//! which cost an afternoon of chasing bugs that had already been fixed.
//! So the client carries its own short commit and says it.
//!
//! CI hands it over in `GITHUB_SHA`; a build from a working tree asks git;
//! anything else — a source tarball with no repository — says so rather
//! than inventing a number.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-changed=build.rs");
    let id = std::env::var("GITHUB_SHA").ok().map(|sha| sha.chars().take(7).collect::<String>()).or_else(git_short_sha).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=EUI_BUILD={id}");
    platform_parts();
    windows_icon();
}

/// Three of the client's parts are a feature *and* a platform: the system
/// clipboard, the platform file dialogs and the accessibility adapter each
/// rest on a crate with no phone behind it (`arboard`, `rfd`,
/// `accesskit_winit`). Cargo can leave the crates out for those targets —
/// they are declared under a `cfg(not(any(android, ios)))` table — but a
/// `#[cfg(feature = ...)]` in the source would go on compiling the code
/// that calls them.
///
/// So the source asks about `has_clipboard`, `has_files` and `has_a11y`
/// rather than about the features, and those are set here: the feature is
/// on and the target has something to put behind it. On a desktop they
/// follow the features exactly, which is why `--no-default-features` still
/// means what it always did.
///
/// `no_subprocess` is the other one. Neither phone will start a second
/// binary — Android refuses to `exec` one out of an application's own
/// storage, and iOS has no `fork` or `exec` at all — so the confined worker
/// of 08 §10 cannot exist there and the code that would try is not
/// compiled. Asking about the capability rather than naming the two
/// platforms at each site is what keeps the third one honest.
fn platform_parts() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let phone = os == "android" || os == "ios";
    for (feature, cfg) in [("A11Y", "has_a11y"), ("CLIPBOARD", "has_clipboard"), ("FILES", "has_files")] {
        println!("cargo:rustc-check-cfg=cfg({cfg})");
        if !phone && std::env::var_os(format!("CARGO_FEATURE_{feature}")).is_some() {
            println!("cargo:rustc-cfg={cfg}");
        }
    }
    // The other direction: three things only a phone has behind them. A
    // desktop has no tag reader and no positioning the client can reach,
    // and a camera there is not a sheet anybody raises — so the code that
    // would call them is not compiled for it, exactly as `has_files` is
    // not compiled for a phone.
    for cfg in ["has_nfc", "has_location", "has_camera"] {
        println!("cargo:rustc-check-cfg=cfg({cfg})");
        if phone {
            println!("cargo:rustc-cfg={cfg}");
        }
    }
    println!("cargo:rustc-check-cfg=cfg(no_subprocess)");
    if phone {
        println!("cargo:rustc-cfg=no_subprocess");
    }
}

/// The icon Windows draws for the file itself.
///
/// It reaches a `.exe` as a linked resource and by no other route: winit's
/// window icon dresses the running window, while Explorer, the desktop and
/// the taskbar's pinned list draw what the linker embedded. The usual route
/// is `rc.exe`, which exists only on Windows and only with the SDK on the
/// path; what `rc.exe` produces is a `.res`, and `link.exe` takes one as an
/// ordinary input. So `scripts/make-icons.py` writes the `.res` from the
/// same SVG as every other icon, it is committed, and this names it.
/// Nothing is compiled here and no compiler is looked for.
///
/// Only the MSVC toolchain links a `.res`: a `windows-gnu` build would need
/// it turned into an object by `windres` first, and is told so rather than
/// failed.
fn windows_icon() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let res = std::path::Path::new(&root).join("../../assets/icon/eui.res");
    println!("cargo:rerun-if-changed={}", res.display());
    if !res.is_file() {
        println!("cargo:warning=no {} — run scripts/make-icons.py; eui.exe keeps the default icon", res.display());
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        println!("cargo:warning=eui.exe keeps the default icon: only the MSVC toolchain links a .res");
        return;
    }
    println!("cargo:rustc-link-arg-bins={}", res.display());
}

/// The working tree's short commit, with a `+` when it has uncommitted
/// changes -- a build from a dirty tree is not the commit it names.
fn git_short_sha() -> Option<String> {
    let head = Command::new("git").args(["rev-parse", "--short", "HEAD"]).output().ok()?;
    if !head.status.success() {
        return None;
    }
    let sha = String::from_utf8(head.stdout).ok()?.trim().to_owned();
    if sha.is_empty() {
        return None;
    }
    let dirty = Command::new("git").args(["status", "--porcelain"]).output().ok().is_some_and(|o| !o.stdout.is_empty());
    Some(if dirty { format!("{sha}+") } else { sha })
}
