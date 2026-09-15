//! `cargo run -p xtask-web` — the EUI client, compiled for a page.
//!
//! Its own crate rather than a subcommand of `xtask`, because `xtask` links
//! `eui-client` so that `bench` can measure it. That made building a
//! WebAssembly module depend on ALSA, xkbcommon and Wayland, which is a
//! confusing wall to hit when the target is `wasm32-unknown-unknown` and
//! nothing in the module wants sound — and every deploy that ships the
//! client paid for it. This crate has no dependencies at all.
//!
//! It runs three programs and copies four files. `cargo` builds the
//! `cdylib`, `wasm-bindgen` turns it into a module a bare
//! `<script type="module">` can import, and `wasm-opt` shrinks it where
//! binaryen is installed. Then the pair, the bootstrap script and a
//! manifest naming the build go beside every site that serves a session.

// A build tool, held to a build tool's standards rather than the decode
// path's. The workspace denies `expect` because `eui-proto` meets bytes a
// server chose and must not panic on them; nothing here meets anything but
// this repository, and a missing `cargo` is worth a panic with a name on it
// rather than three layers of `Result`. Same allowance as `xtask` next door.
#![allow(clippy::expect_used)]

/// The page's target, named once.
const TARGET: &str = "wasm32-unknown-unknown";

/// Whether this machine can compile for `target` at all.
///
/// The same courtesy `xtask conform` extends to the two phone targets: a
/// missing standard library is something to say plainly and act on, not a
/// wall of linker errors.
fn std_installed(target: &str) -> bool {
    let Ok(out) = std::process::Command::new("rustc").args(["--print", "target-libdir", "--target", target]).output() else {
        return false;
    };
    out.status.success() && std::path::Path::new(String::from_utf8_lossy(&out.stdout).trim()).is_dir()
}

/// Build the page's client and lay it beside the site that serves it.
///
/// `wasm-bindgen` directly, and neither `wasm-pack` nor `trunk`: the sites
/// are Soli applications with no build step, no npm and their own layouts
/// (`www/README.md` says so in as many words), and what this needs is the
/// one thing `wasm-bindgen` does — turn a `cdylib` into a `.wasm` and an ES
/// module that a bare `<script type="module">` can import. `wasm-pack`
/// would wrap that in an npm package for nobody, and `trunk` wants to own
/// an `index.html` that Soli already owns.
fn main() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    if !std_installed(TARGET) {
        eprintln!("web: no {TARGET} standard library — `rustup target add {TARGET}`");
        std::process::exit(1);
    }
    // The generator and the macro must be the same version or the module
    // loads and then fails on its first call, which is a long way from the
    // cause. Checked rather than hoped for.
    let want = wasm_bindgen_version();
    match std::process::Command::new("wasm-bindgen").arg("--version").output() {
        Ok(out) => {
            let have = String::from_utf8_lossy(&out.stdout).split_whitespace().nth(1).unwrap_or_default().to_owned();
            if !want.is_empty() && have != want {
                eprintln!("web: wasm-bindgen CLI is {have}, the lock file says {want} — `cargo install wasm-bindgen-cli --version {want} --locked`");
                std::process::exit(1);
            }
        }
        Err(_) => {
            eprintln!("web: no wasm-bindgen on PATH — `cargo install wasm-bindgen-cli --version {want} --locked`");
            std::process::exit(1);
        }
    }

    // Its own target directory, for the reason every other step here has
    // one: these flags are part of a build's fingerprint, and sharing would
    // have each run evict what the other left.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../target/web");
    let status = std::process::Command::new(&cargo).args(["build", "--release", "--target", TARGET, "-p", "eui-web"]).env("CARGO_TARGET_DIR", &dir).status().expect("cargo runs");
    if !status.success() {
        eprintln!("web: FAILED at cargo build");
        std::process::exit(1);
    }

    let module = dir.join(TARGET).join("release/eui_web.wasm");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut served = false;
    // Every tree that serves an embed, because none should have to reach
    // into another's for it. `examples/demo-app` is on the list and is not
    // a site: it is the one server that can answer a session for the page
    // it also serves, which is what SEC-046's same-origin rule leaves.
    for site in ["www", "doc", "examples/demo-app"] {
        let out = root.join(site).join("public/eui");
        let status = std::process::Command::new("wasm-bindgen").args(["--target", "web", "--no-typescript", "--out-dir"]).arg(&out).arg(&module).status().expect("wasm-bindgen runs");
        if !status.success() {
            eprintln!("web: FAILED at wasm-bindgen for {site}");
            std::process::exit(1);
        }
        // `wasm-opt` where there is one, and a note where there is not:
        // the module works either way and the difference is bytes on a
        // documentation page, not correctness.
        let wasm = out.join("eui_web_bg.wasm");
        match std::process::Command::new("wasm-opt").args(["-Oz", "--enable-bulk-memory", "-o"]).arg(&wasm).arg(&wasm).status() {
            Ok(s) if s.success() => {}
            Ok(_) => eprintln!("note: wasm-opt failed; shipping the unoptimised module"),
            Err(_) => eprintln!("note: no wasm-opt on PATH — the module is some 30% larger than it needs to be (`binaryen`)"),
        }
        // The bootstrap script, from the one copy of it there is.
        //
        // It used to live in each site's `public/`, which meant two of them
        // and a third the day a third site wanted one. It is generated
        // output like the module beside it, so it is written like the module
        // beside it.
        let embed_src = root.join("assets/web/eui-embed.js");
        if let Err(e) = std::fs::copy(&embed_src, out.join("eui-embed.js")) {
            eprintln!("web: cannot copy {}: {e}", embed_src.display());
            std::process::exit(1);
        }

        // What version this is, in a file small enough to fetch uncached.
        //
        // The module is served `immutable` for a year — right for bytes that
        // never change, fatal for bytes that do, and a deploy that replaced
        // the client would never be fetched again. So the embed asks this
        // first, with `cache: "no-store"`, and then loads the module at a
        // URL carrying the answer. One request of forty bytes buys a client
        // that is never a year stale.
        // The commit, and the module's own mtime beside it.
        //
        // The commit alone is not enough and the reason is the ordinary
        // working day: a tree with uncommitted changes reports the same sha
        // for every build, so an hour of iterating would be served from the
        // cache as one. The mtime moves whenever the bytes do, which is the
        // only property this needs.
        let stamp = std::fs::metadata(&wasm).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
        let version = format!("{}-{stamp}", build_id());
        let manifest = out.join("manifest.json");
        if let Err(e) = std::fs::write(&manifest, format!("{{\"version\":\"{version}\"}}\n")) {
            eprintln!("web: cannot write {}: {e}", manifest.display());
            std::process::exit(1);
        }
        let bytes = std::fs::metadata(&wasm).map_or(0, |m| m.len());
        println!("{}: {:.2} MB, version {version}", wasm.display(), bytes as f64 / (1024.0 * 1024.0));
        served = true;
    }
    if served {
        // Soli reads its JS into memory once, at boot, and serves the module
        // `immutable` for a year. A server left running across a build hands
        // out the old glue beside the new module, and what that looks like is
        // a `LinkError` about a function import that is not callable —
        // twenty minutes from its cause. The version above is what saves the
        // *reader*; this line is what saves whoever built it.
        println!("note: restart `soli serve` to pick this up — it caches its JS at boot");
    }
}

/// The short commit this module was built from, with `+` when the tree it
/// was built from had uncommitted changes — the same shape `EUI_BUILD` uses,
/// so a module and the window title it reports agree.
fn build_id() -> String {
    let out = std::process::Command::new("git").args(["rev-parse", "--short", "HEAD"]).output();
    let Ok(out) = out else { return "unknown".to_owned() };
    if !out.status.success() {
        return "unknown".to_owned();
    }
    let sha = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let dirty = std::process::Command::new("git").args(["status", "--porcelain"]).output().map(|o| !o.stdout.is_empty()).unwrap_or(false);
    if dirty {
        format!("{sha}+")
    } else {
        sha
    }
}

/// What the lock file resolved `wasm-bindgen` to, so the CLI can be held to
/// it. Empty when it cannot be read, which turns the check into a warning
/// rather than a wall.
fn wasm_bindgen_version() -> String {
    let lock = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.lock");
    let Ok(text) = std::fs::read_to_string(lock) else { return String::new() };
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "name = \"wasm-bindgen\"" {
            return lines.next().unwrap_or_default().trim().trim_start_matches("version = ").trim_matches('"').to_owned();
        }
    }
    String::new()
}
