//! Reading a tag: the seam between the window and whatever the platform
//! has, and nothing else.
//!
//! The driver owns the three conditions of 03 §3.3 — the activation, the
//! server handler, the capability — and owns the token, the one-tag rule
//! and the silence when nothing is read. None of that is here. This module
//! is the one call a platform shell answers.
//!
//! It is compiled on every target, and deliberately so. The first version
//! of this was behind a `has_nfc` that only a phone set, which meant the
//! code calling it was never compiled on the machine it was written on: the
//! module did not exist, the desktop build was happy, and the two phone
//! builds fell over on a name. A seam that compiles identically everywhere
//! cannot do that.
//!
//! # What a platform has to provide
//!
//! - **Android.** `NfcAdapter.enableReaderMode` while the activity is in
//!   front, with a `ReaderCallback` — a Java class, in the application's
//!   own APK. The `android-activity` handle this crate keeps
//!   ([`crate::android::app`]) reaches the activity, not a classloader with
//!   a callback in it.
//! - **iOS.** `NFCNDEFReaderSession` with a delegate, which is an
//!   Objective-C class: either `objc2`'s `declare_class!` or a Swift shim
//!   in the Xcode project the static library is linked into. The session
//!   raises a system sheet and iOS requires a person to have asked for it,
//!   which is the rule 03 §3.3 already imposes on both platforms.
//!
//! Neither is a Rust-only piece of work, which is why neither is here: this
//! crate is linked *into* an application shell on both phones, and the
//! shell is where a class can be declared.

use crate::driver::{NfcAsk, NfcRecord};

/// Start a scan. `true` when a reader took it and will answer.
///
/// `false` means this build has no reader, and the caller ends the scan at
/// once — which is the same answer a person cancelling gives, and reports
/// nothing to the application (06 §3).
pub fn start(ask: &NfcAsk) -> bool {
    let _ = ask;
    false
}

/// What a shell hands back when a reader read something.
///
/// Here for the shape rather than the plumbing: a shell calls
/// [`crate::Backend::scanned`] with the token it was given and the records
/// below, and the driver does the rest. `kind` is `text`, `uri`, `mime:…`
/// or `raw`; `payload` is UTF-8 for the first three and lower-case hex for
/// the last. A client MUST NOT ship a general NDEF model (03 §3.3), so a
/// record a shell cannot classify is `raw` and the application decides.
pub fn record(kind: &str, payload: &str) -> NfcRecord {
    NfcRecord { kind: kind.to_owned(), payload: payload.to_owned() }
}
