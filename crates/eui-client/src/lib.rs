//! # EUI client
//!
//! Three parts, kept apart so that two of them can be tested without the
//! third:
//!
//! - [`driver`] — the session, layout, input dispatch and painting. Frames
//!   in, frames out; input in, events out. No window, no socket.
//! - [`transport`] — a WebSocket over TLS on its own thread, binary frames
//!   only, refusing `ws://` outside a debug build's loopback.
//! - [`app`] — the winit window in `ControlFlow::Wait`, a wgpu surface, and
//!   the glue. There is no render loop; the window redraws only when a frame
//!   arrived, the viewer acted, or the OS asked.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// Pixel and coordinate arithmetic on values the window handed us; see
// eui-render's lib.rs for why this is the right scope for the lint.
#![allow(clippy::arithmetic_side_effects)]

// The tree an assistive technology sees is plain data the worker builds
// and the window forwards, so it is compiled whether or not the platform
// adapter is: only `a11y::to_update`, which speaks AccessKit, is behind
// the feature. `--no-default-features` drops the adapter, not the wire.
/// `eprintln!` where there is no stderr.
///
/// This crate says what it is doing on stderr and by no other route — "no
/// GPU adapter", "refusing to connect", the sandbox line, the manifest
/// line. A page has no stderr: `wasm32-unknown-unknown` compiles the write
/// and drops it, so every one of those lines is written and nobody ever
/// sees it, which is a client that fails silently by construction.
///
/// A `macro_rules!` shadows the prelude macro for everything declared after
/// it, so this is the whole fix and it costs no edits at the call sites.
/// Declared before the modules for that reason — order is scope here.
#[cfg(target_arch = "wasm32")]
macro_rules! eprintln {
    () => { web_sys::console::error_1(&wasm_bindgen::JsValue::from_str("")) };
    ($($arg:tt)*) => {
        web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&format!($($arg)*)))
    };
}

/// The clock: the platform's on every target, and the page's on one.
///
/// `std::time::Instant::now()` on `wasm32-unknown-unknown` is a panic
/// waiting for the first frame — there is no monotonic clock in the
/// standard library there, and the browser's is `performance.now()`.
/// `web-time` is that clock behind `std`'s own names, and on every other
/// target it *is* `std`'s, re-exported: identical codegen, same `Debug`,
/// same arithmetic. It is also not a new dependency. winit takes a
/// `web_time::Instant` in `ControlFlow::WaitUntil` and wgpu uses the same
/// crate, so this is the client agreeing with the two libraries it is
/// already built on rather than converting at every boundary.
///
/// One caveat worth knowing and not worth working around: a browser
/// deliberately coarsens `performance.now()` to about 100 µs. Nothing here
/// samples finer than a frame.
pub mod time {
    #[cfg(not(target_arch = "wasm32"))]
    pub use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    #[cfg(target_arch = "wasm32")]
    pub use web_time::{Duration, Instant, SystemTime, UNIX_EPOCH};
}

pub mod a11y;
/// Where an Android process starts, and the three things the platform will
/// only tell an `AndroidApp`: its data directory, its soft keyboard, and
/// the looper the event loop has to be built on.
#[cfg(target_os = "android")]
pub mod android;
pub mod app;
pub mod assets;
/// Sound: the device this process opens and the thread that keeps it fed
/// (03 §7). A page has no such device — WebAudio is a graph the host owns
/// and pulls from — so the mixing stays (it is in the driver) and the
/// filler thread does not. See `build.rs`'s `platform_parts`.
#[cfg(has_audio)]
pub mod audio;
pub mod chrome;
/// Where an iOS process starts, and the one thing UIKit will only tell an
/// application about itself: the container it may write in.
#[cfg(target_os = "ios")]
pub mod ios;
pub mod recent;
/// Where a page's client starts: the canvas the embedding script handed
/// over, kept until the event loop is ready to take it.
#[cfg(target_arch = "wasm32")]
pub mod web;

/// Which build this is: the short commit it was made from, `+` when the
/// tree it was made from had uncommitted changes, `unknown` when there was
/// no repository to ask. Shown in the window so a demo from the wrong run
/// can be told from the right one at a glance.
pub const BUILD: &str = env!("EUI_BUILD");
/// Where the pointer is during a drag, which no winit backend says.
/// The safe half; the `unsafe` is in `eui-cursor`, one crate over.
pub mod cursor;
/// The desktop's own palette, read from disk and watched for edits
/// (05 §5). There is no such file in a browser, and the light/dark *mode*
/// is not this: winit reports that on every target, the web included.
#[cfg(has_desktop_theme)]
pub mod desktop_theme;
pub mod dial;
pub mod driver;
/// A launcher entry with the application's own icon, so an address is
/// something the desktop can start. Needs somewhere to write one, which
/// neither phone has; see `has_launchers` in `build.rs`.
#[cfg(has_launchers)]
pub mod install;
/// One process for every window: the socket a second `eui` hands its
/// launch to, so a launcher does not build a GPU stack beside the one
/// already running.
#[cfg(has_instance)]
pub mod instance;
/// The signed manifest of 01 §2.1 and the pin store behind it. Needs a
/// signature verifier and somewhere durable to keep a key, and a page has
/// neither; see `has_pins` in `build.rs`.
#[cfg(has_pins)]
pub mod manifest;
pub mod mesh;
pub mod nfc;
pub mod place;
pub mod sandbox;
#[cfg(has_native_net)]
pub mod transport;
/// The same socket over the one the host already opened. Selected here
/// rather than asked about at each site, so the re-export below and every
/// caller in `app.rs` name one module on every target.
#[cfg(not(has_native_net))]
#[path = "transport_web.rs"]
pub mod transport;
/// The drop half of spec 03 §3.2 on Wayland, which winit does not report.
/// The safe half of it; the `unsafe` is in `eui-wayland`, one crate over.
#[cfg(target_os = "linux")]
pub mod wayland;
pub mod worker;

pub use assets::{normalise_url, AssetError, AssetStore, Image};
pub use driver::{Close, Driver, FileAsk, FileWant, FileWrite, Fix, Input, NfcAsk, NfcRecord, PickSource};
pub use transport::{check_url, connect, Connection, Incoming, TransportError};
pub use worker::Backend;
