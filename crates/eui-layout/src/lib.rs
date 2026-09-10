//! # EUI layout
//!
//! One algorithm, specified in `spec/04-layout.md`, so that every client
//! places every node at the same pixel: flow (`row`/`column` with wrap, grow,
//! shrink, gap, justify and align), `stack`, a one-shape `grid`, `scroll`,
//! and a virtualised `list` that does not measure what it cannot see.
//!
//! The engine borrows a [`Session`] and a resolved theme, asks a
//! [`TextMeasurer`] for text sizes, and writes one [`Rect`] per node.
//! `measure` is memoised per frame and pure; `arrange` runs once per node.
//!
//! No `unsafe`, no dependencies beyond the other EUI crates.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod engine;
pub mod geom;
pub mod measure;
pub mod style;

pub use engine::{Env, Glide, Layout, Stats};
pub use eui_tree::{NodeIx, Session};
pub use geom::{Constraint, Rect, Size};
pub use measure::{FontSpec, Monospace, TextMeasurer, TextMetrics};
pub use style::{Edges, Length, Style};
