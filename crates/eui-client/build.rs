//! What build this is, baked in at compile time.
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
    let id = std::env::var("GITHUB_SHA")
        .ok()
        .map(|sha| sha.chars().take(7).collect::<String>())
        .or_else(git_short_sha)
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=EUI_BUILD={id}");
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
