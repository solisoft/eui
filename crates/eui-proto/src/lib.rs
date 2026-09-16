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
pub use frame::{caps, Chunked, Density, EventFrame, Frame, Hello, Resume, ThemeMode, Transfer, Viewport, Welcome};
pub use manifest::{Manifest, Rotation};
pub use node::{EventKind, FlatNode, Handler, NodeKind, Subtree, TextRef, Value};
pub use op::{Batch, Op};
pub use reader::Reader;
pub use style::{
    AlignItems, AlignSelf, ColorRef, Cursor, Dim, Display, FontFamily, FontWeight, Justify, Motion, Overflow, Position, StyleRecord, TextAlign, Wrap, ANIMATION_ENTER, ANIMATION_EXIT, ANIMATION_MASK,
    ANIMATION_SPIN,
};
pub use writer::Writer;

/// The protocol version this crate implements.
///
/// Two since the `scene` kind (03 §1.2). Adding a node kind is a version,
/// deliberately and by the rule in `00-rationale.md`: an older client meets
/// `0x11` as a decode error and ends the session rather than showing half a
/// tree, so the price of a new primitive is a new client. Version one is
/// still spoken -- a manifest says the range it serves, a `Welcome` names
/// the lower of the two ends, and an application that asked for nothing new
/// goes on working with the clients it already had.
pub const PROTOCOL_VERSION: u32 = 4;
// 4 carries `DefFont` (02 §5) and the open half of `font_family`: roles
// `2..` are the application's faces. Both are a version for the same reason
// the `scene` kind was — a client at 3 meets opcode `0x15` as an unknown
// opcode and a style byte of `2` as an unknown tag, and ends the session
// rather than drawing a page in the wrong face. A server advertises 1-4 and
// asks for a *minimum* of 4 only from the applications that declare a font
// role; every other application keeps the clients it had.
//
// 3 carries `EventKind::Level` (03 §7), which `soli` gates on it.
//
// Safe to move now, and it was not this morning: the client's manifest
// check required this to be *inside* the range a server advertises, so
// claiming 3 refused every server still serving 2 — production included.
// It now asks only that the ranges meet, which is what the session always
// did (`hello.version.min(...)` on one side, any `Welcome` at or below this
// on the other). A server that speaks 2 gets a conversation in 2 and no
// level events; one that speaks 3 gets them.
//
// `manifest::check` requires this to be *inside* the range a server
// advertises — there is no negotiating down — so a client that claims a
// version no deployed server serves refuses every one of them:
//
//     manifest: the server speaks EUI 2-2, this client 3
//
// which is what every `eui` built from main did, against production and
// against a local `soli` alike. The bump belongs with a `soli` that
// advertises 2-3; published on its own it is not a new protocol, it is an
// outage. `EventKind::Level` stays defined: a tag nobody sends yet costs
// nothing, and it is what the peaks will ride on when the server side
// lands.
