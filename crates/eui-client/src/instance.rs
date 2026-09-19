//! One process for every window, instead of one process for every launch.
//!
//! A window process holds the wgpu instance, the adapter, the device, the
//! pipelines and the naga output — most of what makes a first pixel
//! expensive — and [`crate::app::Shared`] already keeps exactly one of each
//! *per process*, however many windows are in it. What did not exist was a
//! way for a second `eui` on the command line to reach the first: every
//! launch from a menu, a `.desktop` file or a terminal built its own GPU
//! stack beside the one already running. Measured on two ordinary
//! applications: 88 MB and 121 MB of window process, against 12 MB and
//! 46 MB of worker.
//!
//! So: a listening socket in the user's runtime directory. The first `eui`
//! binds it and runs the loop; every later one connects, hands over its
//! addresses and the capabilities the person granted, and exits. The
//! running instance opens a window for them — a window, not a tab, because
//! that is what a launch has always produced and this is meant to change
//! what it costs and not what it looks like.
//!
//! **What this does not change is the sandbox.** The worker is per tab and
//! stays per tab: the decoder, the tree, layout, text shaping and the VM go
//! on running in their own confined process, one per application (08 §10).
//! What is now shared is the window process, which decodes nothing.
//!
//! What it does change is fate: a GPU device lost, or a panic in the event
//! loop, now takes every window in the process. `--standalone` is the way
//! out, and the socket is skipped entirely when it is given.
//!
//! The socket is same-user only. It lives in `$XDG_RUNTIME_DIR`, which is
//! `0700`, and is itself `0600` — a launch carries capability bits, so
//! anything that can write to it could open a session with capabilities
//! granted; the boundary that has to hold is the one that already holds for
//! running the binary at all.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use winit::event_loop::EventLoopProxy;

use crate::app::{Launch, Opening, Wake};

/// The first line of the protocol: its version, and the build on both ends.
///
/// The build is in it because these two processes share a window, a GPU
/// device and a renderer, and there is no version negotiation for any of
/// that — the wire between a window and its worker is private and
/// unversioned for the same reason. An `eui` from another build gets a
/// window of its own rather than a guess at what it understands, which is
/// also what makes two builds side by side behave during development.
fn hello() -> String {
    format!("eui-instance 1 {}", crate::BUILD)
}

/// How long a handover may take before the launcher gives up and opens its
/// own window. A running instance answers in microseconds; one wedged in a
/// dialog, in a debugger or in a GPU reset answers never, and a launcher
/// that waits for it is a menu entry that does nothing.
const PATIENCE: Duration = Duration::from_millis(400);

/// Most addresses one message may carry, and the longest each may be.
/// Bounds on what a peer can make this process allocate; generous for
/// anything a person types and small enough to be uninteresting.
const MAX_URLS: usize = 32;
const MAX_LINE: u64 = 8 * 1024;

/// Where the socket lives.
///
/// The user's runtime directory first: it is `0700`, it is per-user without
/// anything here having to ask who the user is, and the session removes it
/// at logout. Failing that — macOS has none — a directory of our own under
/// the cache home, made `0700`. Never a shared `/tmp` path: this crate
/// forbids `unsafe`, so it cannot ask for a uid to key one with, and a
/// guessable path in a world-writable directory is not a thing to hand
/// capability bits to.
fn door() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return Some(PathBuf::from(dir).join("eui.sock"));
    }
    let home = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    let dir = home.join("eui");
    std::fs::create_dir_all(&dir).ok()?;
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    Some(dir.join("eui.sock"))
}

/// Ask the instance already running to open these, and say whether it did.
///
/// `false` means there was nobody to ask, the socket was stale, or whoever
/// is there did not answer in [`PATIENCE`] — in every one of which the
/// caller should go on and open its own window, because a launch that
/// vanishes is worse than a launch that costs a process.
pub fn hand_over(launches: &[Launch], chrome: bool, allowed: u32) -> bool {
    let Some(path) = door() else { return false };
    hand_over_at(&path, launches, chrome, allowed)
}

/// The same, at a named socket. Split out so the test can name a path of
/// its own: `door()` reads the environment, and this crate forbids the
/// `unsafe` that setting a variable now needs.
fn hand_over_at(path: &std::path::Path, launches: &[Launch], chrome: bool, allowed: u32) -> bool {
    let Ok(stream) = UnixStream::connect(path) else {
        // Nobody listening. A file left behind by an instance that died is
        // cleared here rather than at exit, because exiting is the one
        // moment a process cannot be relied on to do anything.
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        return false;
    };
    let _ = stream.set_read_timeout(Some(PATIENCE));
    let _ = stream.set_write_timeout(Some(PATIENCE));
    let mut out = String::with_capacity(256);
    out.push_str(&hello());
    out.push('\n');
    out.push_str(&format!("{allowed} {}\n", u8::from(chrome)));
    for l in launches.iter().take(MAX_URLS) {
        out.push_str(&l.url);
        out.push('\n');
    }
    out.push('\n');
    let mut w = &stream;
    if w.write_all(out.as_bytes()).is_err() || w.flush().is_err() {
        return false;
    }
    // The answer is the whole point: it says a window is being opened, and
    // it is what tells a launcher that exiting now loses nothing.
    let mut said = String::new();
    let mut r = BufReader::new(&stream).take(64);
    matches!(r.read_line(&mut said), Ok(n) if n > 0) && said.trim() == "ok"
}

/// The socket, while this process is the one holding it.
///
/// Dropping it stops the thread from mattering and takes the file away.
/// Nothing depends on that happening — a stale file is cleared by the next
/// launcher that finds nobody behind it — but a process that exits tidily
/// should leave nothing behind for the next one to clean up.
pub struct Door {
    path: PathBuf,
}

impl Drop for Door {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Take the socket and answer on it, or return `None` if somebody else has
/// it (which means this process should have handed over and did not — the
/// caller then simply runs alone, which is what every build before this one
/// did).
pub fn listen(proxy: Arc<EventLoopProxy<Wake>>) -> Option<Door> {
    let path = door()?;
    let sock = match UnixListener::bind(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            // Either a live instance won a race with us, or a file left by a
            // dead one. Asking is the only way to tell them apart.
            if UnixStream::connect(&path).is_ok() {
                return None;
            }
            let _ = std::fs::remove_file(&path);
            UnixListener::bind(&path).ok()?
        }
        Err(_) => return None,
    };
    // Before anything can connect: the runtime directory is already 0700,
    // and this says the same thing where it is not.
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    let kept = path.clone();
    serve(sock, move |open| proxy.send_event(Wake::Open(Box::new(open))).is_ok())?;
    Some(Door { path: kept })
}

/// Answer on `sock` until the socket closes or `take` says the loop it was
/// feeding is gone.
///
/// Split from [`listen`] so the wire can be tested without an event loop:
/// building one needs a display, and a test that needs a display is a test
/// that does not run.
fn serve(sock: UnixListener, take: impl Fn(Opening) -> bool + Send + 'static) -> Option<()> {
    std::thread::Builder::new()
        .name("eui-instance".to_owned())
        .spawn(move || {
            for stream in sock.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(PATIENCE));
                if let Some(open) = read_ask(&stream) {
                    // Told before the window exists, on purpose: the
                    // launcher is a process waiting to exit, and what it
                    // needs to know is that this one took the job, not that
                    // a surface has been created.
                    let mut w = &stream;
                    let _ = w.write_all(b"ok\n");
                    let _ = w.flush();
                    if !take(open) {
                        break;
                    }
                }
            }
        })
        .ok()?;
    Some(())
}

/// One message, or `None` if it is not one of ours.
fn read_ask(stream: &UnixStream) -> Option<Opening> {
    let mut r = BufReader::new(stream.try_clone().ok()?).take(MAX_LINE * (MAX_URLS as u64 + 3));
    let mut line = String::new();
    r.read_line(&mut line).ok()?;
    if line.trim_end() != hello() {
        return None;
    }
    line.clear();
    r.read_line(&mut line).ok()?;
    let mut head = line.split_whitespace();
    let allowed: u32 = head.next()?.parse().ok()?;
    let chrome = head.next().is_some_and(|c| c == "1");
    let mut launches = Vec::new();
    loop {
        line.clear();
        if r.read_line(&mut line).ok()? == 0 {
            break;
        }
        let url = line.trim_end_matches(['\r', '\n']);
        if url.is_empty() {
            break;
        }
        if launches.len() >= MAX_URLS {
            break;
        }
        launches.push(Launch::new(url.to_owned(), allowed));
    }
    Some(Opening { launches, chrome, allowed })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wire, both ways, at a socket of the test's own: a launch
    /// crosses with its addresses and its grant intact, and a file left by
    /// an instance that died is cleared rather than waited on.
    #[test]
    fn a_launch_crosses_the_socket_and_a_dead_instance_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join(format!("eui-instance-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("eui.sock");
        let _ = std::fs::remove_file(&path);

        let (tx, rx) = std::sync::mpsc::channel();
        let sock = UnixListener::bind(&path).unwrap();
        serve(sock, move |open| tx.send(open).is_ok()).unwrap();

        let asks = vec![Launch::new("wss://host/_eui/session/one".to_owned(), 0), Launch::new("wss://host/_eui/session/two".to_owned(), 0)];
        assert!(hand_over_at(&path, &asks, false, eui_proto::caps::NET_OPEN), "the instance took the launch");

        let got = rx.recv_timeout(Duration::from_secs(2)).expect("the launch arrived");
        assert_eq!(got.launches.len(), 2);
        assert_eq!(got.launches[0].url, "wss://host/_eui/session/one");
        assert_eq!(got.launches[1].url, "wss://host/_eui/session/two");
        // The grant is the one on *that* command line, not the one this
        // process was started with.
        assert_eq!(got.allowed, eui_proto::caps::NET_OPEN);
        assert_eq!(got.launches[0].allowed, eui_proto::caps::NET_OPEN);
        assert!(!got.chrome);

        // An instance that died leaves a file behind, and the next launcher
        // must not mistake it for somebody to hand a window to.
        drop(Door { path: path.clone() });
        assert!(!path.exists(), "the door takes the file with it");
        std::fs::write(&path, b"stale").unwrap();
        assert!(!hand_over_at(&path, &asks, false, 0), "nobody is behind a stale socket");
        assert!(!path.exists(), "and the launcher cleared it");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
