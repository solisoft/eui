//! Why a batch could not be applied.
//!
//! An apply error poisons the session (`spec/01-transport.md` §4): the tree is
//! no longer trustworthy, so the client discards it and asks for a resync. The
//! error therefore names *what* went wrong for the diagnostic, and nothing
//! about how to continue, because there is no continuing.

use core::fmt;

/// Which session table an id belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    /// Interned strings.
    Atom,
    /// Computed style records.
    Style,
    /// Literal colours.
    Color,
    /// Bytecode chunks.
    Chunk,
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Atom => "atom",
            Self::Style => "style",
            Self::Color => "color",
            Self::Chunk => "chunk",
        })
    }
}

/// A batch was rejected. The session is poisoned until the next `Mount`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApplyError {
    /// The session was poisoned by an earlier failure and has not been
    /// re-mounted.
    Poisoned,
    /// An op other than a definition or `Mount` arrived before any `Mount`.
    NoTree,
    /// A definition reused an id.
    Redefined(Table, u32),
    /// A definition's id exceeds the table's ceiling.
    IdOutOfRange(Table, u32),
    /// A reference to an id that was never defined.
    Undefined(Table, u32),
    /// The sum of atom values exceeded the session budget.
    AtomBudget,
    /// A node id that is not in the tree.
    UnknownNode(u32),
    /// A subtree introduced an id that is already live.
    DuplicateNode(u32),
    /// A child index past the end of the parent's child list.
    ChildIndexOutOfRange {
        /// The parent.
        parent: u32,
        /// The offending index.
        index: u32,
        /// How many children the parent has.
        len: u32,
    },
    /// The op needs a node that can hold children, and this kind cannot.
    NotAContainer(u32),
    /// `ScrollTo` on a node that is neither `scroll` nor `list`.
    NotScrollable(u32),
    /// `RemoveChild` would remove the root, or `Replace` targeted nothing.
    CannotRemoveRoot,
    /// The live node count would exceed the session limit.
    TooManyNodes,
    /// The tree would exceed the depth limit.
    TooDeep,
    /// A `text`, `props`, or `handlers` change on a kind that refuses it.
    InertNode(u32),
    /// A node would carry more props than the protocol allows.
    TooManyProps(u32),
    /// A node would carry more handlers than the protocol allows.
    TooManyHandlers(u32),
    /// A batch sequence number that does not increase.
    OutOfOrder {
        /// The last sequence number applied.
        last: u64,
        /// The one that arrived.
        got: u64,
    },
    /// An arena invariant broke. Never expected; reported rather than panicked.
    Internal,
}

impl fmt::Display for ApplyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Poisoned => f.write_str("session is poisoned; a Mount is required"),
            Self::NoTree => f.write_str("op before the first Mount"),
            Self::Redefined(t, id) => write!(f, "{t} {id} defined twice"),
            Self::IdOutOfRange(t, id) => write!(f, "{t} id {id} exceeds the table limit"),
            Self::Undefined(t, id) => write!(f, "{t} {id} is not defined"),
            Self::AtomBudget => f.write_str("atom byte budget exceeded"),
            Self::UnknownNode(id) => write!(f, "node {id} is not in the tree"),
            Self::DuplicateNode(id) => write!(f, "node {id} is already in the tree"),
            Self::ChildIndexOutOfRange { parent, index, len } => {
                write!(f, "child index {index} out of range for node {parent} with {len} children")
            }
            Self::NotAContainer(id) => write!(f, "node {id} cannot hold children"),
            Self::NotScrollable(id) => write!(f, "node {id} is not scrollable"),
            Self::CannotRemoveRoot => f.write_str("the root cannot be removed"),
            Self::TooManyNodes => f.write_str("node limit exceeded"),
            Self::TooDeep => f.write_str("tree depth limit exceeded"),
            Self::InertNode(id) => write!(f, "node {id} is inert and carries no content"),
            Self::TooManyProps(id) => write!(f, "node {id} exceeds the props limit"),
            Self::TooManyHandlers(id) => write!(f, "node {id} exceeds the handlers limit"),
            Self::OutOfOrder { last, got } => write!(f, "batch {got} after batch {last}"),
            Self::Internal => f.write_str("arena invariant violated"),
        }
    }
}

impl std::error::Error for ApplyError {}

/// Result of applying ops.
pub type Result<T> = core::result::Result<T, ApplyError>;
