//! iOS: where the process starts, where its files go, and what UIKit does
//! for the client that no other platform does.
//!
//! iOS is the platform that asks least of this crate, because winit's UIKit
//! backend already speaks the two dialects that mattered elsewhere:
//!
//! - **Touch.** `touchesBegan/Moved/Ended/Cancelled` arrive as
//!   `WindowEvent::Touch` with all four phases, so spec 06 §5 — one contact
//!   resolved into the pointer — runs here unchanged. It was written for
//!   Android and needed nothing for this.
//! - **The soft keyboard.** `set_ime_allowed` is `becomeFirstResponder` on
//!   this platform, which *is* how a keyboard is raised. The focus change
//!   that welcomes a desktop input method raises and dismisses the iOS one
//!   through the same call.
//! - **The lifecycle.** `applicationDidBecomeActive` is `Resumed` and
//!   `applicationWillResignActive` is `Suspended`, so the surface dropped on
//!   the way out and made again on the way back is already right.
//!
//! Raising the keyboard is where winit stops, though. It never says how
//! *big* the thing it raised is, and on a platform that puts it in front of
//! a window which keeps its size, that is the difference between a field
//! somebody can see and one they cannot. [`covered`] is the one question
//! this file puts to UIKit, and it is a getter — no Objective-C class of
//! our own, no notification observer, nothing to unregister.
//!
//! What is left is this file: where the process is entered from, where it
//! may write, and how much of the window the keyboard is on.
//!
//! What iOS does *not* get, and cannot:
//!
//! - **No worker.** There is no `fork` and no `exec` on iOS at all — not
//!   restricted, absent. The driver runs on a thread of this process (see
//!   [`crate::worker::Backend::open`]). The application sandbox confines the
//!   process; nothing holds the frame decoder apart from the renderer beside
//!   it, which is what spec 08 §10 is for. Android at least has an
//!   `isolatedProcess` service to argue about; here there is nothing.
//! - **No file dialogs, clipboard or accessibility adapter.** The three
//!   crates behind them have no UIKit backend; `build.rs` turns their `has_*`
//!   cfgs off for this target.

use std::path::PathBuf;

/// Where this application may write.
///
/// iOS sets `HOME` to the application's own container — the sandbox, private
/// to this app and gone when it is deleted — so the path is reachable
/// without a line of Objective-C. `Library/Application Support` is where a
/// file the person never sees belongs: `Documents` is theirs and shows up in
/// the Files app, and a pin store is not something anyone should be invited
/// to edit.
///
/// The pin store and the recent list come here. Without a pin store there is
/// no trust on first use and so no session at all, which is why this is not
/// a nicety.
pub fn data_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library").join("Application Support").join("eui"))
}

/// The address this build was made for, baked in at build time or passed at runtime.
///
/// An application is one application, not a browser: there is no command
/// line on a phone to take a session URL from, and no address bar worth
/// typing into. So the packaging sets `EUI_IOS_URL` and the address is in
/// the binary. Without one the client checks the runtime `EUI_IOS_URL` env var,
/// and if that is also absent, falls back to the shell, which is what
/// a development build wants — somewhere to type an address while the
/// packaging is still being worked out.
pub fn url() -> Option<String> {
    if let Some(u) = option_env!("EUI_IOS_URL").filter(|u| !u.is_empty()) {
        return Some(u.to_owned());
    }
    std::env::var("EUI_IOS_URL").ok().filter(|u| !u.is_empty())
}

/// Everything after UIKit has started: open the address this build was made
/// for, or the shell if it was made without one, and run until the
/// application ends.
///
/// Must be called from the main thread, after `UIApplicationMain` — which is
/// what the static library's entry point guarantees, and why there is one.
pub fn run() -> Result<(), String> {
    match url() {
        Some(u) => crate::app::run(u, 0),
        None => crate::app::shell(),
    }
}

/// How much of the window's bottom edge the soft keyboard is standing on,
/// for [`crate::driver::Input::Covered`].
///
/// iOS is the platform that never answers this by itself. Android can be
/// asked to shorten the window for the keyboard and the client hears an
/// ordinary resize; UIKit puts the keyboard *in front of* a window that
/// keeps its size, and says how big it is through a notification or not at
/// all.
///
/// The asking is [`eui_uikit::covered`], one crate over, because a message
/// to an Objective-C object needs an `unsafe` block and this crate forbids
/// them. What is left here is the safe half, which is all of the winit: the
/// `UIView` this window was made from, out of its own handle.
///
/// Points, which are the logical px the driver counts in — no scale.
pub fn covered(window: &winit::window::Window) -> f32 {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let Ok(handle) = window.window_handle() else { return 0.0 };
    let RawWindowHandle::UiKit(ui) = handle.as_raw() else { return 0.0 };
    // The pointer is winit's own `UIView`, live for as long as the window,
    // and this runs on the thread the event loop runs on — which is what
    // `eui_uikit::covered` asks of its caller.
    eui_uikit::covered(ui.ui_view.as_ptr())
}
