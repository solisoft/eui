//! How much of the window a soft keyboard is standing on, asked of UIKit.
//!
//! This crate exists for one line of `unsafe`, and for where that line is
//! allowed to be. `eui-client` is `#![forbid(unsafe_code)]` — a promise
//! about the process that draws pages from the network, and not one to give
//! up for a getter. Sending a message to an Objective-C object cannot be
//! done without `unsafe`, so the message is sent here instead, in a crate
//! small enough to read in a sitting, and the client calls a safe function.
//!
//! It knows nothing of winit: the caller does the safe half — pulling the
//! `UIView` out of its window handle — and hands the pointer over.
//!
//! Why a getter and not an observer: UIKit announces its keyboard through
//! `UIKeyboardWillChangeFrameNotification`, which would mean a class of our
//! own, a block, and something to unregister. But `UIKeyboardLayoutGuide`
//! holds the same answer and can simply be read, and the client is already
//! asking the platform where the keyboard belongs once a pass of its loop.
//! Reading beats being told when the reader is already there.

#![allow(unsafe_code)]

use core::ffi::c_void;

/// How much of `ui_view`'s bottom edge the soft keyboard is over, in points
/// — which are the logical px the client counts in, so no scale.
///
/// Zero when the keyboard is down, when the pointer is null, and on iOS 14
/// and older where the guide does not exist: the selector is asked for
/// rather than assumed, because the packaging says `MinimumOSVersion 13.0`
/// and sending a message nobody implements is not a missing feature, it is
/// a crash.
///
/// # Safety
///
/// `ui_view` must be a live `UIView` — winit's, for the window being asked
/// about — and this must run on the main thread, the only one UIKit may be
/// touched from. Both hold at the one call site: the pointer comes straight
/// out of the window handle, and the caller is the event loop.
#[cfg(target_os = "ios")]
#[allow(clippy::cast_possible_truncation)]
#[must_use]
pub fn covered(ui_view: *mut c_void) -> f32 {
    use objc2::runtime::NSObjectProtocol;
    use objc2::sel;
    use objc2_ui_kit::UIView;

    if ui_view.is_null() {
        return 0.0;
    }
    // SAFETY: the caller's contract, above.
    let view: &UIView = unsafe { &*ui_view.cast::<UIView>() };
    if !view.respondsToSelector(sel!(keyboardLayoutGuide)) {
        return 0.0;
    }
    // SAFETY: the selector is there, and every call below is a getter but
    // the one setter, which is documented where it is made.
    unsafe {
        let guide = view.keyboardLayoutGuide();
        // Left alone, the guide falls back to the bottom safe area when the
        // keyboard is down — so a phone with a home indicator would report
        // its 34 points as covered for ever, and the page would be short by
        // that much with no keyboard in sight. What is wanted here is the
        // keyboard and nothing else. Set every time rather than once: the
        // window may have been rebuilt since, and the setter costs a
        // message.
        guide.setUsesBottomSafeArea(false);
        let frame = guide.layoutFrame();
        let bounds = view.bounds();
        let floor = bounds.origin.y + bounds.size.height;
        // A keyboard is a thing standing on the bottom edge, and only a
        // rectangle that reaches that edge is taken for one. A guide that
        // has not been through a layout pass yet reports an empty rect at
        // the origin, and read naively that is the whole window covered and
        // a page with no room left to be in.
        let reaches_the_floor = frame.size.height > 0.0 && frame.origin.y + frame.size.height >= floor - 1.0;
        if !reaches_the_floor {
            return 0.0;
        }
        let over = floor - frame.origin.y;
        if !over.is_finite() || over <= 0.0 {
            return 0.0;
        }
        over.min(bounds.size.height) as f32
    }
}

/// Nothing is standing on anything where there is no UIKit.
#[cfg(not(target_os = "ios"))]
#[must_use]
pub fn covered(_ui_view: *mut c_void) -> f32 {
    0.0
}
