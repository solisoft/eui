//! The EUI client as an Android application.
//!
//! Android has no `main`. The platform creates the process, loads this
//! shared object, and calls [`android_main`] with the handle to the
//! activity; everything after that is the ordinary client — the same
//! driver, the same layout engine, the same renderer, the same session.
//!
//! The crate is this one file because that is all the difference amounts
//! to. What the platform *does* change is spelled out in
//! `eui_client::android`, and the short of it is: the driver runs in this
//! process rather than a confined worker, and there are no file dialogs,
//! no clipboard and no accessibility adapter, because Android has nothing
//! behind those three.
#![cfg(target_os = "android")]

use android_activity::AndroidApp;

/// Where the process begins.
///
/// The platform calls this on a thread of its own with the activity, its
/// looper and its directories in hand. It is kept before anything else
/// runs, because the event loop is built on that looper and the pin store
/// lives in those directories — neither can be reached any other way.
///
/// It returns when the activity ends. An error here is a client that could
/// not open at all: it goes to the log, which on Android is the only place
/// anyone would look.
#[no_mangle]
fn android_main(app: AndroidApp) {
    eui_client::android::start(app);
    if let Err(e) = eui_client::android::run() {
        eprintln!("eui: {e}");
    }
}
