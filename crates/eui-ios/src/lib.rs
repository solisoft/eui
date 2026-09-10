//! The EUI client as an iOS application.
//!
//! iOS has no `main` of ours to run. UIKit creates the process, and
//! `UIApplicationMain` must have done its work before a window can exist at
//! all — so the entry point is [`eui_start`], a function Xcode's own `main`
//! calls once UIKit is up. Everything after that is the ordinary client: the
//! same driver, the same layout engine, the same renderer, the same session.
//!
//! The crate is this one file because that is all the difference amounts to.
//! `eui_client::ios` says what the platform does change, and the short of it
//! is: the driver runs in this process rather than a confined worker, and
//! there are no file dialogs, no clipboard and no accessibility adapter,
//! because iOS has nothing behind those three.
//!
//! # Linking it
//!
//! ```sh
//! cargo build --release -p eui-ios --target aarch64-apple-ios
//! ```
//!
//! Add `target/aarch64-apple-ios/release/libeui_ios.a` to an Xcode project,
//! declare the entry point, and call it from `main`:
//!
//! ```c
//! void eui_start(void);
//!
//! int main(int argc, char *argv[]) {
//!     eui_start();
//!     return 0;
//! }
//! ```
//!
//! `eui_start` returns when the application ends.
#![cfg(target_os = "ios")]

/// Where the client begins, called from Xcode's `main` after UIKit is up.
///
/// Must be the main thread: winit's UIKit backend requires it, and so does
/// every window this opens. It returns when the application ends — which on
/// iOS is usually never, because a suspended application is killed rather
/// than asked to leave.
///
/// An error here is a client that could not open at all. It goes to the log,
/// which on a device is the only place anyone would look.
#[no_mangle]
pub extern "C" fn eui_start() {
    if let Err(e) = eui_client::ios::run() {
        eprintln!("eui: {e}");
    }
}
