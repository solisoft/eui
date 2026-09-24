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
// The panic lints are `warn` for the workspace and `deny` here, outside
// tests: this crate is on the path from the socket to the tree, and on iOS,
// Android, the web and `EUI_SANDBOX=0` there is no worker process around it —
// it runs inside the application, under `panic = "abort"`, so a panic a
// server can reach closes the whole app rather than one worker. Denied at the
// crate so the promise does not depend on which flags a build was run with.
#![cfg_attr(not(test), deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used, clippy::expect_used, clippy::arithmetic_side_effects))]

pub mod arena;
pub mod error;
mod hash;
pub mod limits;
pub mod session;
mod tables;

pub use arena::{dirty, Node, NodeIx};
pub use error::{ApplyError, Result, Table};
pub use eui_proto::Batch;
pub use limits::Limits;
pub use session::{same_layout, secret_display, secret_offset, secret_unoffset, Chunk, Preorder, Session, WellKnown};

/// One disc per Unicode scalar, so `i` and `W` occupy the same width.
pub const SECRET_MARK: char = '•';
