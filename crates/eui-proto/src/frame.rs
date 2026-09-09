//! Session frames (`spec/01-transport.md` §3).

use crate::error::{DecodeError, Result};
use crate::limits::{MAX_FRAME_BYTES, MAX_INLINE_STR};
use crate::node::{EventKind, Value};
use crate::op::Batch;
use crate::reader::Reader;
use crate::writer::Writer;

/// Which palette the viewer is using.
///
/// This is a *client* fact. It reaches the server only so that a server can
/// choose a matching image asset; the server never resolves colours, so it
/// never needs to know, and an app that ignores this field still renders
/// correctly in dark mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ThemeMode {
    /// Light palette.
    #[default]
    Light = 0,
    /// Dark palette.
    Dark = 1,
    /// Maximum-contrast palette.
    HighContrast = 2,
}

impl ThemeMode {
    /// Decode.
    pub const fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::Light),
            1 => Ok(Self::Dark),
            2 => Ok(Self::HighContrast),
            _ => Err(DecodeError::UnknownTag("theme mode")),
        }
    }
}

/// How tightly controls are packed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Density {
    /// Tight, for dense data views.
    Compact = 0,
    /// The default.
    #[default]
    Cozy = 1,
    /// Loose, and easier to hit.
    Comfortable = 2,
}

impl Density {
    /// Decode.
    pub const fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::Compact),
            1 => Ok(Self::Cozy),
            2 => Ok(Self::Comfortable),
            _ => Err(DecodeError::UnknownTag("density")),
        }
    }
}

/// Capability bits, as granted by the user and reported to the server.
///
/// A capability that is not in this set has no code path in the client at all.
/// The check is not "is this allowed?" at the call site — the call site does
/// not exist.
pub mod caps {
    /// Read frames from a camera.
    pub const CAMERA: u32 = 1 << 0;
    /// Read audio from a microphone.
    pub const MICROPHONE: u32 = 1 << 1;
    /// Read the system clipboard.
    pub const CLIPBOARD_READ: u32 = 1 << 2;
    /// Write the system clipboard.
    pub const CLIPBOARD_WRITE: u32 = 1 << 3;
    /// Post OS notifications.
    pub const NOTIFICATIONS: u32 = 1 << 4;
    /// Read a coarse location.
    pub const LOCATION: u32 = 1 << 5;
    /// Open a file the user picks.
    pub const FS_PICK: u32 = 1 << 6;

    /// The names of 01 §2.1, in bit order.
    pub const NAMES: [(&str, u32); 7] = [
        ("camera", CAMERA),
        ("microphone", MICROPHONE),
        ("clipboard.read", CLIPBOARD_READ),
        ("clipboard.write", CLIPBOARD_WRITE),
        ("notifications", NOTIFICATIONS),
        ("location", LOCATION),
        ("fs.pick", FS_PICK),
    ];

    /// A capability by its name, `clipboard.read`.
    pub fn from_name(name: &str) -> Option<u32> {
        NAMES.iter().find(|(n, _)| *n == name).map(|(_, bit)| *bit)
    }

    /// The names of the bits set in `mask`.
    pub fn names(mask: u32) -> Vec<&'static str> {
        NAMES.iter().filter(|(_, bit)| mask & bit != 0).map(|(n, _)| *n).collect()
    }
    /// Save to a file the user picks.
    pub const FS_SAVE: u32 = 1 << 7;
    /// Every bit this protocol version defines.
    pub const ALL: u32 = 0xFF;
}

/// The viewer's presentation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    /// Logical width in device-independent pixels.
    pub width: u32,
    /// Logical height in device-independent pixels.
    pub height: u32,
    /// Device pixel ratio, in hundredths (`200` = 2×).
    pub scale: u16,
    /// Palette in use.
    pub mode: ThemeMode,
    /// Control density.
    pub density: Density,
    /// Accessibility text scale, in hundredths (`125` = 125 %).
    pub font_scale: u16,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { width: 0, height: 0, scale: 100, mode: ThemeMode::default(), density: Density::default(), font_scale: 100 }
    }
}

impl Viewport {
    /// Decode.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self { width: r.varint32()?, height: r.varint32()?, scale: r.u16()?, mode: ThemeMode::from_u8(r.u8()?)?, density: Density::from_u8(r.u8()?)?, font_scale: r.u16()? })
    }

    /// Encode.
    pub fn encode(&self, w: &mut Writer) {
        w.varint32(self.width).varint32(self.height).u16(self.scale).u8(self.mode as u8).u8(self.density as u8).u16(self.font_scale);
    }
}

/// The client's opening frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// Highest EUI version the client implements.
    pub version: u32,
    /// Presentation state at connect time.
    pub viewport: Viewport,
    /// Capabilities the user has granted this application.
    pub granted: u32,
}

/// The server's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Welcome {
    /// Version both sides will use. MUST NOT exceed the client's.
    pub version: u32,
    /// Opaque session identifier, 16 bytes.
    pub session: [u8; 16],
}

/// A client-originated event.
#[derive(Debug, Clone, PartialEq)]
pub struct EventFrame {
    /// The node the event happened on.
    pub node: u32,
    /// Which event.
    pub event: EventKind,
    /// The atom naming the server-side handler, from `Handler::Server`.
    pub name: u32,
    /// Event-specific payload.
    pub payload: Value,
}

/// A whole session message.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// C→S opening frame.
    Hello(Hello),
    /// S→C answer.
    Welcome(Welcome),
    /// S→C ordered ops.
    Batch(Batch),
    /// C→S event.
    Event(EventFrame),
    /// C→S acknowledgement of the last applied batch.
    Ack {
        /// Sequence number applied.
        seq: u64,
    },
    /// Liveness probe.
    Ping([u8; 8]),
    /// Liveness answer.
    Pong([u8; 8]),
    /// Fatal diagnostic; the session ends.
    Error {
        /// Stable code, see [`crate::DecodeError::code`].
        code: u32,
        /// Human-readable detail.
        message: String,
    },
    /// C→S: client state is unrecoverable, send a fresh `Mount`.
    Resync,
    /// C→S: presentation state changed.
    Viewport(Viewport),
}

impl Frame {
    /// Decode a complete WebSocket message.
    ///
    /// Rejects trailing bytes: a frame's declared length must account for every
    /// byte of the message. "Ignore what you don't understand" is how one
    /// implementation's frame becomes another's smuggling channel.
    pub fn decode(message: &[u8]) -> Result<Self> {
        let mut r = Reader::new(message);
        let kind = r.u8()?;
        let len = usize::try_from(r.varint()?).map_err(|_| DecodeError::BadVarint)?;
        if len > MAX_FRAME_BYTES {
            return Err(DecodeError::LimitExceeded("frame length"));
        }
        let payload = r.take(len)?;
        r.finish()?;

        let mut p = Reader::new(payload);
        let frame = match kind {
            0x01 => {
                let version = p.varint32()?;
                let viewport = Viewport::decode(&mut p)?;
                let granted = p.varint32()?;
                if granted & !caps::ALL != 0 {
                    return Err(DecodeError::IllegalValue("unknown capability bit"));
                }
                Self::Hello(Hello { version, viewport, granted })
            }
            0x02 => {
                let version = p.varint32()?;
                Self::Welcome(Welcome { version, session: p.array::<16>()? })
            }
            0x03 => Self::Batch(Batch::decode(&mut p)?),
            0x04 => Self::Event(EventFrame { node: p.varint32()?, event: EventKind::from_u8(p.u8()?)?, name: p.varint32()?, payload: Value::decode(&mut p)? }),
            0x05 => Self::Ack { seq: p.varint()? },
            0x06 | 0x07 => {
                let nonce = p.array::<8>()?;
                if kind == 0x06 {
                    Self::Ping(nonce)
                } else {
                    Self::Pong(nonce)
                }
            }
            0x08 => Self::Error { code: p.varint32()?, message: p.str(MAX_INLINE_STR, "error message")?.to_owned() },
            0x09 => Self::Resync,
            0x0A => Self::Viewport(Viewport::decode(&mut p)?),
            _ => return Err(DecodeError::UnknownTag("frame kind")),
        };
        p.finish()?;
        Ok(frame)
    }

    /// Encode a complete WebSocket message.
    pub fn encode(&self) -> Vec<u8> {
        let mut body = Writer::new();
        let kind = match self {
            Self::Hello(h) => {
                body.varint32(h.version);
                h.viewport.encode(&mut body);
                body.varint32(h.granted);
                0x01
            }
            Self::Welcome(v) => {
                body.varint32(v.version).raw(&v.session);
                0x02
            }
            Self::Batch(b) => {
                b.encode(&mut body);
                0x03
            }
            Self::Event(e) => {
                body.varint32(e.node).u8(e.event.to_u8()).varint32(e.name);
                e.payload.encode(&mut body);
                0x04
            }
            Self::Ack { seq } => {
                body.varint(*seq);
                0x05
            }
            Self::Ping(n) => {
                body.raw(n);
                0x06
            }
            Self::Pong(n) => {
                body.raw(n);
                0x07
            }
            Self::Error { code, message } => {
                body.varint32(*code).str(message);
                0x08
            }
            Self::Resync => 0x09,
            Self::Viewport(v) => {
                v.encode(&mut body);
                0x0A
            }
        };

        let body = body.into_vec();
        let mut out = Writer::with_capacity(body.len().saturating_add(8));
        out.u8(kind).varint(body.len() as u64).raw(&body);
        out.into_vec()
    }
}
