//! Theme errors.

use core::fmt;

/// Why a theme, role, or style could not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ThemeError {
    /// A role id outside `1..=28`.
    UnknownRole(u16),
    /// A scale index past the end of its scale.
    ScaleIndex(&'static str, u8),
    /// A theme document that could not be read.
    BadDocument(&'static str),
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRole(id) => write!(f, "unknown colour role {id}"),
            Self::ScaleIndex(scale, ix) => write!(f, "index {ix} is past the end of the {scale} scale"),
            Self::BadDocument(what) => write!(f, "bad theme document: {what}"),
        }
    }
}

impl std::error::Error for ThemeError {}
