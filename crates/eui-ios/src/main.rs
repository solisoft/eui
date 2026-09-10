//! The EUI client as a standalone iOS application.
//!
//! winit's UIKit backend calls `UIApplicationMain` from `run_app`, so an
//! ordinary `fn main` *is* an iOS application: there is no Xcode project
//! here, no `AppDelegate`, and no Objective-C. `scripts/make-ios-app.sh`
//! wraps the binary this produces in a `.app` bundle with an `Info.plist`
//! beside it, which is all iOS asks of one.
//!
//! The other way in is the static library beside this file, for embedding in
//! an Xcode project that already owns its `main`. Nothing here is needed for
//! that, and nothing there is needed for this.
//!
//! On every other platform this is an empty program, so that
//! `cargo build --workspace` on a desktop stays honest.

fn main() {
    #[cfg(target_os = "ios")]
    if let Err(e) = eui_client::ios::run() {
        eprintln!("eui: {e}");
    }
}
