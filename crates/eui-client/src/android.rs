//! Android: where the process starts, where its files go, and how the soft
//! keyboard is asked for.
//!
//! An Android application has no `main`. The platform loads a shared object
//! and calls `android_main` in it with an [`AndroidApp`] — the handle to the
//! activity, its looper and its directories — and everything the client
//! wants from the platform that winit does not carry comes through that one
//! object. So it is kept here, once, and the three places that need it ask.
//!
//! What is *not* here is as much of the story:
//!
//! - **No worker.** Android will not `exec` a second binary out of an
//!   application's own storage, so the driver runs on a thread of this
//!   process (see [`crate::worker::Backend::open`]). The application
//!   sandbox and SELinux confine the process; nothing confines the decoder
//!   from the renderer beside it, which spec 08 §10 asks for and this
//!   platform does not yet get.
//! - **No file dialogs, clipboard or accessibility.** The three crates
//!   behind them have no Android backend; `build.rs` turns their `has_*`
//!   cfgs off for this target and the code that calls them is not compiled.

use std::path::PathBuf;
use std::sync::OnceLock;

use android_activity::AndroidApp;

/// The handle the platform gave `android_main`, kept for the life of the
/// process. It is cheap to clone and every clone names the same activity.
static APP: OnceLock<AndroidApp> = OnceLock::new();

/// Take the handle. Called once, first thing, by the shared object's
/// `android_main`; a second call is ignored rather than replacing a handle
/// something may already be holding.
pub fn start(app: AndroidApp) {
    let _ = APP.set(app);
}

/// The handle, for whatever needs the platform directly.
pub fn app() -> Option<AndroidApp> {
    APP.get().cloned()
}

/// The directory the platform gave this application for its own files.
///
/// There is no `$HOME` on Android and no XDG anything, so the pin store and
/// the recent list — which look for those on a desktop — are told to come
/// here instead. It is inside the application's sandbox: no other
/// application can read it, and it goes when the application is uninstalled.
pub fn data_dir() -> Option<PathBuf> {
    app()?.internal_data_path()
}

/// Ask for the soft keyboard, or dismiss it.
///
/// On a desktop `set_ime_allowed` is the whole of this: the input method is
/// a service that is either welcome or not. Android has no such thing — the
/// keyboard is a window the application asks the system to raise, and
/// nothing raises it on its own because a field took focus. So the same
/// change of focus that tells a desktop input method it is welcome tells
/// Android to put the keyboard up, and the same one that ends it takes the
/// keyboard away.
///
/// Both calls take a flag and the two flags do not mean the same thing.
/// `show_soft_input(true)` is *implicit*: the keyboard is going up because
/// a field took focus, not because the person asked for it by name — which
/// is exactly what a focus change is. `hide_soft_input(true)` is
/// *implicit-only*: take it away if it went up that way, and leave it alone
/// if the person raised it themselves.
pub fn soft_input(show: bool) {
    let Some(app) = app() else { return };
    if show {
        app.show_soft_input(true);
    } else {
        app.hide_soft_input(true);
    }
}

/// The address this package was built for, baked in at build time.
///
/// An APK is one application, not a browser: there is no command line to
/// take a session URL from and no address bar worth typing into on a phone.
/// So the packaging sets `EUI_ANDROID_URL` and the address is in the binary.
/// Without one the client falls back to the shell, which is what a
/// development build wants — somewhere to type an address while the
/// packaging is still being worked out.
pub fn url() -> Option<&'static str> {
    option_env!("EUI_ANDROID_URL").filter(|u| !u.is_empty())
}

/// Everything after `android_main` has handed the platform over: open the
/// address this package was built for, or the shell if it was built without
/// one, and run until the activity ends.
pub fn run() -> Result<(), String> {
    match url() {
        Some(u) => crate::app::run(u.to_owned(), 0),
        None => crate::app::shell(),
    }
}
