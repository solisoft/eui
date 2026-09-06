//! Decoding failures.
//!
//! There is no error recovery in EUI: a malformed frame ends the session
//! (`spec/01-transport.md` §4). So an error carries a stable numeric code —
//! what goes into the `Error` frame — and a description, and nothing else.
//! Nothing here is recoverable, so nothing here is fine-grained enough to
//! tempt a caller into continuing.

use core::fmt;

/// Why a frame was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeError {
    /// Ran out of bytes mid-field.
    Truncated,
    /// Bytes remained after the frame's declared content.
    TrailingBytes,
    /// A varint was non-minimally encoded, or overflowed its target width.
    BadVarint,
    /// A length or count field exceeded its normative limit.
    LimitExceeded(&'static str),
    /// A tag, opcode, or enum discriminant is not defined by the protocol.
    UnknownTag(&'static str),
    /// A field's value is defined but not legal here (a reserved byte that is
    /// not zero, a NaN float, a bool that is not 0 or 1, a zero node id).
    IllegalValue(&'static str),
    /// A string field was not well-formed UTF-8.
    BadUtf8,
    /// A node kind that must be a leaf was given children.
    NotALeaf,
}

impl DecodeError {
    /// The stable code sent in an `Error` frame.
    pub const fn code(self) -> u32 {
        match self {
            Self::Truncated => 1,
            Self::TrailingBytes => 2,
            Self::BadVarint => 3,
            Self::LimitExceeded(_) => 4,
            Self::UnknownTag(_) => 5,
            Self::IllegalValue(_) => 6,
            Self::BadUtf8 => 7,
            Self::NotALeaf => 8,
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("truncated frame"),
            Self::TrailingBytes => f.write_str("trailing bytes after frame content"),
            Self::BadVarint => f.write_str("non-minimal or overflowing varint"),
            Self::LimitExceeded(what) => write!(f, "limit exceeded: {what}"),
            Self::UnknownTag(what) => write!(f, "unknown tag: {what}"),
            Self::IllegalValue(what) => write!(f, "illegal value: {what}"),
            Self::BadUtf8 => f.write_str("string field is not valid UTF-8"),
            Self::NotALeaf => f.write_str("leaf node kind was given children"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Result of any decoding step.
pub type Result<T> = core::result::Result<T, DecodeError>;
