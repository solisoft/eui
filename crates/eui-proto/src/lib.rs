//! # EUI/1 wire format
//!
//! Encoding and decoding for the EUI protocol, specified normatively in
//! `spec/02-wire-format.md`.
//!
//! ## Why this crate has no dependencies
//!
//! Every byte this crate touches came off a network socket. A dependency here
//! is attack surface we did not write, cannot fuzz as a unit, and cannot audit
//! on our own schedule. There is nothing in the protocol that needs more than
//! `core` and `Vec`, so there is nothing here but `core` and `Vec`.
//!
//! ## The three ideas
//!
//! **Atoms.** Repeated strings are interned once per session and referenced by
//! varint. HTML re-emits `<div class="…">` on every row of a table; EUI sends
//! the atom once.
//!
//! **Computed styles.** A [`StyleRecord`] is 64 fixed bytes of *already
//! resolved* style. There is no cascade on the client because there is nothing
//! left to cascade — a thousand table rows share three style ids.
//!
//! **Flat subtrees.** A [`Subtree`] decodes into pre-order arrays, which is
//! what allows [`Subtree::decode`] to be iterative. A 10 000-deep hostile tree
//! costs a bounds check, not the call stack.
//!
//! ## Decoding discipline
//!
//! - No `unsafe`, enforced by `#![forbid(unsafe_code)]`.
//! - No panics on any input: every read is bounds-checked and fallible.
//! - Varints must be minimally encoded — two spellings of one number is one
//!   too many for anything that gets hashed, signed, or compared.
//! - Unknown enum discriminants are rejected, never clamped. Clamping is how
//!   two implementations quietly disagree about a layout for a year.
//! - Trailing bytes are an error, not padding.
//! - Every limit in [`limits`] is checked *before* the memory it bounds is
//!   allocated.
//!
//! ## Example
//!
//! ```
//! use eui_proto::{Batch, Frame, Op, Subtree, FlatNode, NodeKind, TextRef};
//!
//! let mut tree = Subtree::default();
//! tree.nodes.push(FlatNode {
//!     kind: NodeKind::Text,
//!     id: 1,
//!     style: 1,
//!     key: 0,
//!     text: Some(TextRef::Atom(1)),
//!     props: (0, 0),
//!     handlers: (0, 0),
//!     child_count: 0,
//! });
//!
//! let frame = Frame::Batch(Batch {
//!     seq: 1,
//!     ops: vec![
//!         Op::DefAtom { id: 1, value: "Hi".into() },
//!         Op::Mount(tree),
//!     ],
//! });
//!
//! let bytes = frame.encode();
//! assert_eq!(Frame::decode(&bytes).unwrap(), frame);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod error;
pub mod frame;
pub mod limits;
pub mod manifest;
pub mod node;
pub mod op;
pub mod reader;
pub mod style;
pub mod writer;

pub use error::{DecodeError, Result};
pub use frame::{caps, Density, EventFrame, Frame, Hello, ThemeMode, Viewport, Welcome};
pub use manifest::{Manifest, Rotation};
pub use node::{EventKind, FlatNode, Handler, NodeKind, Subtree, TextRef, Value};
pub use op::{Batch, Op};
pub use reader::Reader;
pub use style::{AlignItems, AlignSelf, ColorRef, Cursor, Dim, Display, FontFamily, FontWeight, Justify, Overflow, Position, StyleRecord, TextAlign, Wrap, ANIMATION_ENTER, ANIMATION_SPIN};
pub use writer::Writer;

/// The protocol version this crate implements.
pub const PROTOCOL_VERSION: u32 = 1;
