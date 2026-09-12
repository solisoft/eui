//! Where the machine is: the seam between the window and whatever the
//! platform has, and the one source that works everywhere.
//!
//! The driver owns the clock, the rounding and the four conditions of
//! 06 §1.2; none of that is here. This module answers one question — *has
//! the platform a newer fix than the last one?* — and the window asks it
//! only while [`Driver::wants_location`](crate::Driver::wants_location) is
//! true, so nothing here runs for a session that never asked.
//!
//! # What a platform has to provide
//!
//! Three calls, and a phone's implementation of them is the whole of the
//! work left in this feature:
//!
//! - **Android.** `LocationManager` through JNI, or `FusedLocationProvider`
//!   if Play services are a dependency the application is willing to have.
//!   Both want a callback on a `Looper`, which means a small Java class in
//!   the application's own APK: the `android-activity` handle this crate
//!   keeps ([`crate::android::app`]) reaches the activity, not a classloader
//!   with a listener in it.
//! - **iOS.** `CLLocationManager` with a delegate, which is an Objective-C
//!   class and so either `objc2`'s `declare_class!` or a Swift shim in the
//!   Xcode project the `.a` is linked into.
//!
//! Neither is a Rust-only piece of work, which is why neither is here: this
//! crate is linked *into* an application shell on both platforms (see
//! `crates/eui-android` and `crates/eui-ios`), and the shell is where a
//! class can be declared. What this module fixes is the shape of the seam,
//! so that a shell only has to push fixes in.

use std::sync::Mutex;

use crate::driver::Fix;

/// The newest fix nobody has taken yet.
///
/// A `Mutex` rather than a channel because a fix has no queue: the one
/// before last is of no interest to anybody, and a platform that delivers
/// ten a second while the window is not drawing must not grow a backlog.
static LATEST: Mutex<Option<Fix>> = Mutex::new(None);

/// Push a fix in. The entry point for a platform shell.
///
/// Safe to call from any thread and at any rate. The window picks it up on
/// its next pass and hands it to the driver, which decides who hears about
/// it and how coarsely.
pub fn offer(fix: Fix) {
    if let Ok(mut slot) = LATEST.lock() {
        *slot = Some(fix);
    }
}

/// Take whatever has arrived since the last call, if anything has.
pub fn take() -> Option<Fix> {
    LATEST.lock().ok().and_then(|mut slot| slot.take())
}

/// Ask the platform to start or stop positioning.
///
/// Called only when the answer changes, so an implementation may treat it
/// as an edge rather than a level. The development source below needs
/// neither, and a platform that has no positioning at all does nothing —
/// `wants_location` stays true, no fix ever arrives, and 06 §1.2's third
/// condition means nothing is reported. Asking is not knowing.
pub fn running(on: bool) {
    let _ = on;
}

/// A fix named in the environment, for working on this without a receiver.
///
/// `EUI_FIX=48.8584,2.2945,12` — latitude, longitude, accuracy in metres.
/// Read once, at the first ask, and offered as the platform would offer it:
/// through [`offer`], so it travels the same path, gets the same rounding
/// and obeys the same four conditions. It is a *source*, not a bypass.
///
/// Deliberately an environment variable and not a flag: it belongs to
/// whoever is running the client, it leaves a trace in the process that
/// started it, and there is no way to reach it from a tree.
pub fn dev_fix() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let Ok(said) = std::env::var("EUI_FIX") else { return };
        let parts: Vec<f64> = said.split(',').filter_map(|p| p.trim().parse::<f64>().ok()).collect();
        let (Some(&latitude), Some(&longitude)) = (parts.first(), parts.get(1)) else {
            eprintln!("eui: EUI_FIX wants latitude,longitude[,accuracy_m]");
            return;
        };
        let accuracy_m = parts.get(2).copied().unwrap_or(50.0);
        eprintln!("eui: EUI_FIX — standing in for a receiver at {latitude}, {longitude} (±{accuracy_m} m)");
        offer(Fix { latitude, longitude, accuracy_m });
    });
}
