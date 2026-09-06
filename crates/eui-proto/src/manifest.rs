//! The application manifest (01 §2.1): a record (02 §7) with magic `EUIM`,
//! signed by the publisher. This module encodes and decodes it; verifying
//! the signature is the client's job, with whatever Ed25519 it trusts —
//! the wire crate has no dependencies and keeps it that way.

use crate::error::{DecodeError, Result};
use crate::node::Value;
use crate::reader::Reader;
use crate::writer::Writer;

/// Record magic.
pub const MAGIC: [u8; 4] = *b"EUIM";
/// Record version.
pub const VERSION: u8 = 1;
/// Longest string field.
pub const MAX_STR: usize = 256;

/// Field keys, the table of 01 §2.1. Fields are encoded in this order; the
/// signature is always last and is the only field the signature excludes.
pub mod key {
    /// `app_id`, `Str`.
    pub const APP_ID: u64 = 0;
    /// `name`, `Str`.
    pub const NAME: u64 = 1;
    /// `version`, `Str`.
    pub const VERSION: u64 = 2;
    /// `protocol_min`, `Int`.
    pub const PROTOCOL_MIN: u64 = 3;
    /// `protocol_max`, `Int`.
    pub const PROTOCOL_MAX: u64 = 4;
    /// `publisher_key`, `Str` of 64 hex digits.
    pub const PUBLISHER_KEY: u64 = 5;
    /// `capabilities`, `Int` bitset of `frame::caps`.
    pub const CAPABILITIES: u64 = 6;
    /// `theme`, `Str` of 64 hex digits or `Null`.
    pub const THEME: u64 = 7;
    /// `entry`, `Str`, an absolute path.
    pub const ENTRY: u64 = 8;
    /// `rotation`, `List[Str previous key, Str signature]` or `Null`.
    pub const ROTATION: u64 = 9;
    /// `signature`, `Str` of 128 hex digits; always last, never signed.
    pub const SIGNATURE: u64 = 10;
}

/// A publisher key rotation: the previous key, and its signature over the
/// new `publisher_key`, so a client that pinned the old key can accept the
/// new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rotation {
    /// The key the client may have pinned.
    pub previous_key: [u8; 32],
    /// Ed25519 by `previous_key` over the manifest's `publisher_key`.
    pub signature: [u8; 64],
}

/// The manifest, minus its signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Stable identifier the client pins a key against.
    pub app_id: String,
    /// Shown to the person.
    pub name: String,
    /// The application's own version string.
    pub version: String,
    /// Lowest protocol version the server speaks.
    pub protocol_min: u32,
    /// Highest protocol version the server speaks.
    pub protocol_max: u32,
    /// Ed25519 public key.
    pub publisher_key: [u8; 32],
    /// Capabilities requested (`frame::caps` bits); the client grants none implicitly.
    pub capabilities: u32,
    /// BLAKE3 of the default theme asset, if any.
    pub theme: Option<[u8; 32]>,
    /// Session path, `/_eui/session` by default.
    pub entry: String,
    /// A key rotation, if the publisher key changed.
    pub rotation: Option<Rotation>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            app_id: String::new(),
            name: String::new(),
            version: String::new(),
            protocol_min: 1,
            protocol_max: 1,
            publisher_key: [0; 32],
            capabilities: 0,
            theme: None,
            entry: "/_eui/session".into(),
            rotation: None,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex<const N: usize>(s: &str) -> Result<[u8; N]> {
    let bytes = s.as_bytes();
    if bytes.len() != N.saturating_mul(2) {
        return Err(DecodeError::IllegalValue("hex field has the wrong length"));
    }
    let mut out = [0u8; N];
    for (slot, pair) in out.iter_mut().zip(bytes.chunks(2)) {
        let pair = std::str::from_utf8(pair).map_err(|_| DecodeError::BadUtf8)?;
        *slot = u8::from_str_radix(pair, 16).map_err(|_| DecodeError::IllegalValue("hex field has a non-hex digit"))?;
    }
    Ok(out)
}

impl Manifest {
    /// The bytes the publisher signs: the record with every field but the
    /// signature. Canonical, so a decoder can rebuild them exactly.
    pub fn signed_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.write(&mut w, None);
        w.into_vec()
    }

    /// The record as served: the signed fields, then the signature.
    pub fn encode(&self, signature: &[u8; 64]) -> Vec<u8> {
        let mut w = Writer::new();
        self.write(&mut w, Some(signature));
        w.into_vec()
    }

    fn write(&self, w: &mut Writer, signature: Option<&[u8; 64]>) {
        let mut fields: Vec<(u64, Value)> = vec![
            (key::APP_ID, Value::Str(self.app_id.clone())),
            (key::NAME, Value::Str(self.name.clone())),
            (key::VERSION, Value::Str(self.version.clone())),
            (key::PROTOCOL_MIN, Value::Int(i64::from(self.protocol_min))),
            (key::PROTOCOL_MAX, Value::Int(i64::from(self.protocol_max))),
            (key::PUBLISHER_KEY, Value::Str(hex(&self.publisher_key))),
            (key::CAPABILITIES, Value::Int(i64::from(self.capabilities))),
            (key::THEME, self.theme.map_or(Value::Null, |t| Value::Str(hex(&t)))),
            (key::ENTRY, Value::Str(self.entry.clone())),
            (key::ROTATION, self.rotation.as_ref().map_or(Value::Null, |r| Value::List(vec![Value::Str(hex(&r.previous_key)), Value::Str(hex(&r.signature))]))),
        ];
        if let Some(sig) = signature {
            fields.push((key::SIGNATURE, Value::Str(hex(sig))));
        }
        w.raw(&MAGIC).u8(VERSION).varint(fields.len() as u64);
        for (k, v) in &fields {
            w.varint(*k);
            v.encode(w);
        }
    }

    /// Decode a served record: the manifest and its signature. Strict —
    /// magic, version, every key present once in order, the signature last,
    /// no trailing bytes — so that `signed_bytes()` of the result is exactly
    /// what was signed.
    pub fn decode(bytes: &[u8]) -> Result<(Self, [u8; 64])> {
        let mut r = Reader::new(bytes);
        if r.array::<4>()? != MAGIC {
            return Err(DecodeError::UnknownTag("manifest magic"));
        }
        if r.u8()? != VERSION {
            return Err(DecodeError::UnknownTag("manifest version"));
        }
        let count = r.varint()?;
        if count != 11 {
            return Err(DecodeError::IllegalValue("a manifest has exactly eleven fields"));
        }
        let mut m = Manifest::default();
        let mut signature = None;
        for expected in 0..count {
            let k = r.varint()?;
            if k != expected {
                return Err(DecodeError::IllegalValue("manifest fields must be in key order"));
            }
            let v = Value::decode(&mut r)?;
            let text = |v: &Value| -> Result<String> {
                match v {
                    Value::Str(s) if s.len() <= MAX_STR => Ok(s.clone()),
                    Value::Str(_) => Err(DecodeError::LimitExceeded("manifest string")),
                    _ => Err(DecodeError::IllegalValue("manifest field must be a string")),
                }
            };
            let int = |v: &Value| -> Result<u32> {
                match v {
                    Value::Int(n) => u32::try_from(*n).map_err(|_| DecodeError::IllegalValue("manifest integer out of range")),
                    _ => Err(DecodeError::IllegalValue("manifest field must be an integer")),
                }
            };
            match k {
                key::APP_ID => m.app_id = text(&v)?,
                key::NAME => m.name = text(&v)?,
                key::VERSION => m.version = text(&v)?,
                key::PROTOCOL_MIN => m.protocol_min = int(&v)?,
                key::PROTOCOL_MAX => m.protocol_max = int(&v)?,
                key::PUBLISHER_KEY => m.publisher_key = unhex::<32>(&text(&v)?)?,
                key::CAPABILITIES => m.capabilities = int(&v)?,
                key::THEME => m.theme = if v == Value::Null { None } else { Some(unhex::<32>(&text(&v)?)?) },
                key::ENTRY => m.entry = text(&v)?,
                key::ROTATION => {
                    m.rotation = match v {
                        Value::Null => None,
                        Value::List(items) => match items.as_slice() {
                            [prev, sig] => Some(Rotation { previous_key: unhex::<32>(&text(prev)?)?, signature: unhex::<64>(&text(sig)?)? }),
                            _ => return Err(DecodeError::IllegalValue("rotation is [previous key, signature]")),
                        },
                        _ => return Err(DecodeError::IllegalValue("rotation is [previous key, signature]")),
                    }
                }
                key::SIGNATURE => signature = Some(unhex::<64>(&text(&v)?)?),
                _ => return Err(DecodeError::UnknownTag("manifest key")),
            }
        }
        r.finish()?;
        if m.app_id.is_empty() {
            return Err(DecodeError::IllegalValue("manifest app_id is empty"));
        }
        if m.protocol_min == 0 || m.protocol_max < m.protocol_min {
            return Err(DecodeError::IllegalValue("manifest protocol range"));
        }
        if !m.entry.starts_with('/') {
            return Err(DecodeError::IllegalValue("manifest entry must be an absolute path"));
        }
        let signature = signature.ok_or(DecodeError::Truncated)?;
        Ok((m, signature))
    }
}
