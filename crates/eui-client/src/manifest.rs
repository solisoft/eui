//! Spec 01 §2.1 and 08 §2: fetch the application's manifest, verify the
//! publisher's signature, and pin the key on first use. Nothing else in the
//! session is trusted before this returns.

use std::path::{Path, PathBuf};

use eui_proto::{Manifest, PROTOCOL_VERSION};
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
    /// The pinned key for this `app_id` differs and no valid rotation was offered.
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
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn config_dir() -> Option<PathBuf> {
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
fn config_dir() -> Option<PathBuf> {
    crate::android::data_dir()
}

/// The application's own container, under `Library/Application Support`.
#[cfg(target_os = "ios")]
fn config_dir() -> Option<PathBuf> {
    crate::ios::data_dir()
}

/// Fetch `/.well-known/eui` from `origin` and run [`verify`] against `pins`.
pub fn check(origin: &str, pins: &Path, cookie: Option<&str>) -> Result<Manifest, ManifestError> {
    let bytes = assets::get(origin, "/.well-known/eui", "application/vnd.eui.manifest", cookie).map_err(ManifestError::Fetch)?;
    verify(&bytes, pins)
}

/// Decode, verify the signature, check the protocol range, and pin or
/// compare the publisher key (trust on first use; a different key needs a
/// rotation signed by the pinned one). Pure but for the pin file.
pub fn verify(bytes: &[u8], pins: &Path) -> Result<Manifest, ManifestError> {
    let (manifest, signature) = Manifest::decode(bytes).map_err(|e| ManifestError::Decode(e.to_string()))?;
    let key = UnparsedPublicKey::new(&ED25519, manifest.publisher_key);
    key.verify(&manifest.signed_bytes(), &signature).map_err(|_| ManifestError::BadSignature)?;
    if !(manifest.protocol_min..=manifest.protocol_max).contains(&PROTOCOL_VERSION) {
        return Err(ManifestError::Protocol { min: manifest.protocol_min, max: manifest.protocol_max });
    }
    let pin = pins.join(pin_name(&manifest.app_id));
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

/// A file name from an `app_id`: its bytes, hex, so no id can escape the dir.
fn pin_name(app_id: &str) -> String {
    app_id.bytes().map(|b| format!("{b:02x}")).collect()
}

fn write_pin(path: &Path, key: &[u8; 32]) -> Result<(), ManifestError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ManifestError::Pins(e.to_string()))?;
    }
    std::fs::write(path, key).map_err(|e| ManifestError::Pins(e.to_string()))
}
