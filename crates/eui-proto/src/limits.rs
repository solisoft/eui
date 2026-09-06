//! Protocol limits, normative in `spec/02-wire-format.md` §6.
//!
//! Every one of these is enforced during decoding, before the memory it would
//! bound is allocated. A hostile server can be annoying; it cannot make the
//! client exhaust itself.

/// Largest frame payload accepted, in bytes.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
/// Deepest node nesting accepted.
pub const MAX_TREE_DEPTH: u32 = 256;
/// Most nodes a session may hold.
pub const MAX_NODES: u32 = 250_000;
/// Highest atom id.
pub const MAX_ATOMS: u32 = 65_535;
/// Largest single atom value, in bytes.
pub const MAX_ATOM_BYTES: usize = 64 * 1024;
/// Largest total of all atom values in a session, in bytes.
pub const MAX_ATOM_TOTAL_BYTES: usize = 8 * 1024 * 1024;
/// Highest style id.
pub const MAX_STYLES: u32 = 65_535;
/// Highest literal colour id.
pub const MAX_COLORS: u32 = 4_095;
/// Highest bytecode chunk id.
pub const MAX_CHUNKS: u32 = 4_095;
/// Most children a single node may have.
pub const MAX_CHILDREN: u32 = 65_535;
/// Most properties a single node may carry.
pub const MAX_PROPS: u32 = 64;
/// Most handlers a single node may carry.
pub const MAX_HANDLERS: u32 = 16;
/// Most ops a single batch may carry.
pub const MAX_OPS_PER_BATCH: u32 = 65_535;
/// Largest inline (non-interned) string, in bytes.
pub const MAX_INLINE_STR: usize = 4 * 1024;
/// Deepest nesting of `Value::List`.
pub const MAX_VALUE_DEPTH: u32 = 4;
/// Most elements in a `Value::List`.
pub const MAX_VALUE_LIST: u32 = 1024;

/// Largest inline bytecode chunk, in bytes.
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;

/// Size of a `StyleRecord` on the wire, in bytes.
pub const STYLE_RECORD_BYTES: usize = 64;
/// Size of a BLAKE3 asset hash, in bytes.
pub const HASH_BYTES: usize = 32;
