//! Spec 01 §2.1 and 08 §2: fetch the application's manifest, verify the
//! publisher's signature, and pin the key on first use. Nothing else in the
//! session is trusted before this returns.

use std::path::{Path, PathBuf};

use eui_proto::{Manifest, PROTOCOL_VERSION};

/// The oldest protocol this client still speaks.
///
/// Every version since has added to the wire rather than changed it — a new
/// event kind, a new prop — so a newer client holds a perfectly good
/// conversation with an older server by simply never using what that server
/// has not heard of. What it must not do is refuse to have the conversation.
const SPEAKS_FROM: u32 = 2;
use ring::signature::{UnparsedPublicKey, ED25519};

use crate::assets::{self, AssetError};

/// Why a manifest was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    /// The server did not serve one.
    Fetch(AssetError),
    /// The bytes are not a manifest record.
    Decode(String),
    /// The signature does not verify under the manifest's own key.
    BadSignature,
    /// The pinned key for this origin and `app_id` differs and no valid rotation was offered.
    KeyChanged,
    /// The server speaks no protocol version this client does.
    Protocol {
        /// The server's lowest version.
        min: u32,
        /// The server's highest version.
        max: u32,
    },
    /// The pin store could not be read or written.
    Pins(String),
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fetch(e) => write!(f, "manifest: {e}"),
            Self::Decode(e) => write!(f, "manifest: not a manifest ({e})"),
            Self::BadSignature => write!(f, "manifest: the signature does not verify"),
            Self::KeyChanged => write!(f, "manifest: the publisher key changed since it was pinned, with no rotation signed by the old key"),
            Self::Protocol { min, max } => write!(f, "manifest: the server speaks EUI {min}–{max}, this client {PROTOCOL_VERSION}"),
            Self::Pins(e) => write!(f, "manifest: pin store: {e}"),
        }
    }
}

impl std::error::Error for ManifestError {}

/// Where pins live: `$EUI_PINS_DIR`, else `$XDG_CONFIG_HOME/eui/pins`, else
/// `~/.config/eui/pins` (`%APPDATA%\eui\pins` on Windows).
///
/// On Android none of those exist — there is no `$HOME` and no XDG — so it
/// is the directory the platform gave the application, inside its own
/// sandbox. Without a pin store there is no trust on first use and no
/// session at all, so this is not a nicety.
pub fn pins_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("EUI_PINS_DIR") {
        return Some(PathBuf::from(d));
    }
    config_dir().map(|d| d.join("pins"))
}

/// The corner of the person's configuration this client keeps things in.
///
/// Public because it is the client's one answer to "where do my things
/// go": the pins, the grants, the recents and the record of what is
/// installed are all the same question, and four answers to it would drift.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn config_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(d).join("eui"));
    }
    if let Some(d) = std::env::var_os("APPDATA") {
        return Some(PathBuf::from(d).join("eui"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("eui"))
}

/// The directory the platform gave this application, inside its own sandbox.
#[cfg(target_os = "android")]
pub fn config_dir() -> Option<PathBuf> {
    crate::android::data_dir()
}

/// The application's own container, under `Library/Application Support`.
#[cfg(target_os = "ios")]
pub fn config_dir() -> Option<PathBuf> {
    crate::ios::data_dir()
}

/// Fetch `/.well-known/eui` from `origin` and run [`verify`] against `pins`.
pub fn check(origin: &str, pins: &Path, cookie: Option<&str>) -> Result<Manifest, ManifestError> {
    let bytes = assets::get(origin, "/.well-known/eui", "application/vnd.eui.manifest", cookie).map_err(ManifestError::Fetch)?;
    verify(&bytes, origin, pins)
}

/// Decode, verify the signature, check the protocol range, and pin or
/// compare the publisher key (trust on first use; a different key needs a
/// rotation signed by the pinned one). Pure but for the pin file.
///
/// `origin` is where the bytes were fetched from, and the pin is kept under
/// it and the `app_id` together. The signed record names no origin — it
/// cannot, without a wire change — so the signature proves who *wrote* a
/// manifest and says nothing about who is *serving* it. A manifest is
/// public: anybody can copy the bytes of a well-known application's
/// `/.well-known/eui` onto their own host, and they verify there exactly
/// as well. Pinned by `app_id` alone, that copy matched the real
/// publisher's pin, wore its padlock and walked into its remembered
/// grants; and an origin that got to an unpinned `app_id` first could claim
/// it, so that the real publisher was the one refused with `KeyChanged`.
/// Keyed by both, the copy is simply a new application at a new origin: it
/// is trusted on first use like any other, and asks for everything.
pub fn verify(bytes: &[u8], origin: &str, pins: &Path) -> Result<Manifest, ManifestError> {
    let (manifest, signature) = Manifest::decode(bytes).map_err(|e| ManifestError::Decode(e.to_string()))?;
    let key = UnparsedPublicKey::new(&ED25519, manifest.publisher_key);
    key.verify(&manifest.signed_bytes(), &signature).map_err(|_| ManifestError::BadSignature)?;
    // The two ranges have to *meet*, not match.
    //
    // This used to require the client's own version to be inside the
    // server's range, which reads as caution and is the opposite: it means
    // every client refuses every server older than itself, so the day a
    // version is added nothing can connect until every server in the world
    // has been upgraded first. One line of that shipped this morning and
    // took out every deployed server at once — `manifest: the server speaks
    // EUI 2-2, this client 3`.
    //
    // The session already knew better. A server settles at
    // `hello.version.min(its own)`, and `handle_frame` accepts any `Welcome`
    // at or below `PROTOCOL_VERSION`, so talking down was always supported
    // — the door was locked in front of a room that was ready.
    //
    // `SPEAKS_FROM` is the floor: the oldest wire this client still
    // understands, which is where the additions since have been additive.
    // Below it there is nothing to negotiate and refusing is right.
    if manifest.protocol_min > PROTOCOL_VERSION || manifest.protocol_max < SPEAKS_FROM {
        return Err(ManifestError::Protocol { min: manifest.protocol_min, max: manifest.protocol_max });
    }
    let pin = store_path(pins, origin, &manifest.app_id);
    match std::fs::read(&pin) {
        Ok(pinned) if pinned == manifest.publisher_key => {}
        Ok(pinned) => {
            let Some(rot) = &manifest.rotation else { return Err(ManifestError::KeyChanged) };
            if rot.previous_key.as_slice() != pinned.as_slice() {
                return Err(ManifestError::KeyChanged);
            }
            UnparsedPublicKey::new(&ED25519, rot.previous_key).verify(&manifest.publisher_key, &rot.signature).map_err(|_| ManifestError::KeyChanged)?;
            write_pin(&pin, &manifest.publisher_key)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => write_pin(&pin, &manifest.publisher_key)?,
        Err(e) => return Err(ManifestError::Pins(e.to_string())),
    }
    Ok(manifest)
}

/// Where the answers to the consent sheet are kept: one file per origin and
/// `app_id`, beside the pins.
///
/// Asking again on every run would make the sheet a thing to click past
/// rather than a thing to read, which is how a permission prompt stops
/// working. Forgetting one is deleting its file; forgetting all of them is
/// deleting this directory.
pub fn grants_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("EUI_GRANTS_DIR") {
        return Some(PathBuf::from(d));
    }
    config_dir().map(|d| d.join("grants"))
}

/// What an application asked for last time, and what it was given.
///
/// Two numbers and not one, and the second is not the interesting half. A
/// store of grants alone cannot tell a capability that was **refused**
/// from one that was never **asked about** — both are simply absent — so
/// it would either nag on every run about the thing the person already
/// said no to, or never notice a new version asking for something new.
/// What was asked is what says which of those happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answered {
    /// The capabilities the sheet showed.
    pub asked: u32,
    /// The ones ticked when it was answered.
    pub granted: u32,
}

/// What this person last said about `app_id` served from `origin`, or `None`
/// if they have not been asked yet.
///
/// By origin as well as id for the reason [`verify`] gives: a grant given
/// to one host is not a grant to another that happens to serve the same
/// manifest.
///
/// A file that cannot be read, or that says something this build does not
/// understand, is a person who has not answered: the sheet goes up again,
/// which costs a question, where guessing would cost a grant nobody gave.
#[must_use]
pub fn remembered_grant(origin: &str, app_id: &str) -> Option<Answered> {
    let path = store_path(&grants_dir()?, origin, app_id);
    let text = std::fs::read_to_string(path).ok()?;
    let mut parts = text.split_whitespace();
    let asked = parts.next()?.parse::<u32>().ok()?;
    let granted = parts.next()?.parse::<u32>().ok()?;
    // A grant outside what was asked is a file somebody edited or a build
    // that wrote a wider mask; either way the sheet is the safe answer.
    let (asked, granted) = (asked & eui_proto::caps::ALL, granted & eui_proto::caps::ALL);
    (granted & !asked == 0).then_some(Answered { asked, granted })
}

/// Keep what they answered, so they are not asked it again.
///
/// Best effort, and deliberately so: a client that could not write here
/// would otherwise have to refuse a session over a file nobody knew about.
/// The cost of failing is one more question next time.
pub fn remember_grant(origin: &str, app_id: &str, answered: Answered) {
    let Some(dir) = grants_dir() else { return };
    let path = store_path(&dir, origin, app_id);
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let asked = answered.asked & eui_proto::caps::ALL;
    let granted = answered.granted & asked;
    let _ = std::fs::write(path, format!("{asked} {granted}\n"));
}

/// Where the pin or the grant for `app_id` at `origin` is kept under `root`:
/// `root/by-origin/<hex origin>/<hex app_id>`.
///
/// Hex, so that neither string — both of which come from a server — can
/// name a file anywhere else. Two components rather than one hex of the
/// pair, so that no choice of `app_id` can spell somebody else's origin.
/// And under `by-origin/`, which is not hex, so that nothing the store
/// held before it was keyed by origin — one file per bare `app_id`, all
/// hex — can be mistaken for an entry of the new one. Those old files are
/// left where they are and never read: carrying one over would mean
/// deciding which origin it belonged to, and that is the very thing it
/// never recorded. The cost is one more trust-on-first-use and one more
/// consent sheet per application, once.
#[must_use]
pub fn store_path(root: &Path, origin: &str, app_id: &str) -> PathBuf {
    root.join("by-origin").join(hex(&normal_origin(origin))).join(hex(app_id))
}

/// `origin` as one spelling: scheme and host lowered, a trailing slash and
/// the scheme's default port dropped. `https://App.example:443/` and
/// `https://app.example` are one origin and must find one pin.
fn normal_origin(origin: &str) -> String {
    let o = origin.trim().trim_end_matches('/').to_ascii_lowercase();
    for (scheme, port) in [("https://", ":443"), ("http://", ":80"), ("wss://", ":443"), ("ws://", ":80")] {
        if o.starts_with(scheme) {
            if let Some(bare) = o.strip_suffix(port) {
                return bare.to_owned();
            }
        }
    }
    o
}

fn hex(s: &str) -> String {
    s.bytes().map(|b| format!("{b:02x}")).collect()
}

fn write_pin(path: &Path, key: &[u8; 32]) -> Result<(), ManifestError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ManifestError::Pins(e.to_string()))?;
    }
    std::fs::write(path, key).map_err(|e| ManifestError::Pins(e.to_string()))
}
