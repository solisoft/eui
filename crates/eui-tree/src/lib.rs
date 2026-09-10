//! # EUI session state
//!
//! Where `eui-proto` answers "is this frame well-formed?", this crate answers
//! "does it make sense against what this session already knows?" — and keeps
//! the resulting tree.
//!
//! - [`Session`] owns the four append-only tables (atoms, styles, colours,
//!   chunks), the node arena, focus, and the poison flag.
//! - [`Session::apply`] applies a [`Batch`] op by op. Every reference is
//!   checked against the tables *before* anything is placed, every quota is
//!   checked before the memory it bounds is allocated, and a failure poisons
//!   the session until the next successful `Mount` — the transport's own
//!   recovery, so no per-batch snapshot is needed.
//! - The arena is one `Vec<Node>` with a free list; a subtree removal is an
//!   iterative walk, never a recursive drop.
//!
//! No `unsafe`, and the same strict lints as the decoder: nothing on the apply
//! path may panic on a hostile batch.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod arena;
pub mod error;
pub mod limits;
pub mod session;
mod tables;

pub use arena::{dirty, Node, NodeIx};
pub use error::{ApplyError, Result, Table};
pub use eui_proto::Batch;
pub use limits::Limits;
pub use session::{same_layout, Chunk, Preorder, Session, WellKnown};
