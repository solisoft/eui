//! # EUI theme resolution
//!
//! The server sends colour *roles* and scale *indices*; this crate turns them
//! into pixels and RGBA for a given viewer, exactly as `spec/05-theme.md`
//! prescribes. The algorithm is normative because the client does the
//! resolving: two clients given the same theme and the same viewer settings
//! must agree.
//!
//! - [`Theme`] is four seeds and a few preferences, decoded from an `EUIT`
//!   record.
//! - [`Theme::resolve`] produces a [`Resolved`] palette — 28 roles as
//!   `0xRRGGBBAA` plus every scale — with WCAG contrast met **by
//!   construction**: lightness is nudged until each specified pair passes,
//!   so there is no "check" that a theme can fail after the fact.
//! - [`check_style`] rejects a `StyleRecord` whose indices run off a scale.
//!
//! No `unsafe`, no dependencies beyond `eui-proto`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod color;
pub mod error;
pub mod role;
pub mod scale;
pub mod theme;

pub use color::{contrast, contrast_oklch, Linear, Oklch};
pub use error::ThemeError;
pub use eui_proto::{Density, ThemeMode};
pub use role::Role;
pub use scale::Curve;
pub use theme::{check_style, Resolved, Theme, Viewer};
