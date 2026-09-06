//! Per-session quotas.
//!
//! Defaults are the protocol ceilings from `eui_proto::limits`. A `Welcome`
//! frame may lower them for a session, and tests lower them to reach the edge
//! without building a quarter-million nodes.

use eui_proto::limits as proto;

/// The quotas a session enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Most live nodes at once.
    pub max_nodes: u32,
    /// Deepest node, root at depth 1.
    pub max_depth: u32,
    /// Highest atom id.
    pub max_atoms: u32,
    /// Sum of all atom values, in bytes.
    pub max_atom_total_bytes: usize,
    /// Highest style id.
    pub max_styles: u32,
    /// Highest literal colour id.
    pub max_colors: u32,
    /// Highest chunk id.
    pub max_chunks: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_nodes: proto::MAX_NODES,
            max_depth: proto::MAX_TREE_DEPTH,
            max_atoms: proto::MAX_ATOMS,
            max_atom_total_bytes: proto::MAX_ATOM_TOTAL_BYTES,
            max_styles: proto::MAX_STYLES,
            max_colors: proto::MAX_COLORS,
            max_chunks: proto::MAX_CHUNKS,
        }
    }
}
