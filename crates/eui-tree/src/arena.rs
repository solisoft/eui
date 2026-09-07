//! The node arena: one contiguous `Vec<Node>`, indices instead of pointers,
//! and a free list so a long session does not grow without bound.

use std::collections::HashMap;

use eui_proto::{EventKind, Handler, NodeKind, TextRef, Value};

use crate::error::{ApplyError, Result};

/// An index into the arena. Stable for the life of the node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeIx(u32);

impl NodeIx {
    /// The absent node.
    pub const NONE: Self = Self(u32::MAX);

    /// True for [`Self::NONE`].
    pub const fn is_none(self) -> bool {
        self.0 == u32::MAX
    }

    /// True for a real index.
    pub const fn is_some(self) -> bool {
        !self.is_none()
    }

    /// The raw index, for callers that keep per-node side tables (layout
    /// results, paint caches). Stable while the node is live; a freed slot is
    /// reused by a later node, so a side table must be cleared with the tree.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// The index back from [`NodeIx::raw`]; only meaningful with a session
    /// that handed the raw value out.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    fn usize(self) -> usize {
        self.0 as usize
    }
}

/// Dirty bits, for incremental layout.
pub mod dirty {
    /// This node's own content, style, or children changed.
    pub const SELF: u8 = 1;
    /// Something below this node changed.
    pub const DESCENDANT: u8 = 2;
}

/// One node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Server id. `0` marks a free slot.
    pub id: u32,
    /// Primitive kind.
    pub kind: NodeKind,
    /// Style table id; `0` is the default record.
    pub style: u32,
    /// Reconciliation key; `0` is positional.
    pub key: u32,
    /// Text content.
    pub text: Option<TextRef>,
    /// `(name atom, value)` pairs.
    pub props: Vec<(u32, Value)>,
    /// `(event, handler)` pairs, at most one per event.
    pub handlers: Vec<(EventKind, Handler)>,
    /// Parent, or `NONE` for the root.
    pub parent: NodeIx,
    /// Children in order.
    pub children: Vec<NodeIx>,
    /// Scroll offsets, meaningful for `scroll` and `list`.
    pub scroll: (i64, i64),
    /// See [`dirty`].
    pub dirty: u8,
}

impl Node {
    /// The handler for `event`, if any.
    pub fn handler(&self, event: EventKind) -> Option<Handler> {
        self.handlers.iter().find(|(e, _)| *e == event).map(|(_, h)| *h)
    }

    /// The value of the prop named by `atom`, if any.
    pub fn prop(&self, atom: u32) -> Option<&Value> {
        self.props.iter().find(|(a, _)| *a == atom).map(|(_, v)| v)
    }
}

/// Node storage.
#[derive(Debug, Default)]
pub(crate) struct Arena {
    nodes: Vec<Node>,
    free: Vec<NodeIx>,
    by_id: HashMap<u32, NodeIx>,
    /// Key atom -> first node carrying it, in placement order. Keys are
    /// unique among siblings by contract and usually unique per component;
    /// a local handler names a node by key, so the first match wins.
    by_key: HashMap<u32, NodeIx>,
    live: u32,
}

impl Arena {
    pub(crate) fn live(&self) -> u32 {
        self.live
    }

    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn lookup(&self, id: u32) -> Option<NodeIx> {
        self.by_id.get(&id).copied()
    }

    pub(crate) fn lookup_key(&self, key: u32) -> Option<NodeIx> {
        self.by_key.get(&key).copied()
    }

    pub(crate) fn get(&self, ix: NodeIx) -> Option<&Node> {
        self.nodes.get(ix.usize()).filter(|n| n.id != 0)
    }

    pub(crate) fn get_mut(&mut self, ix: NodeIx) -> Option<&mut Node> {
        self.nodes.get_mut(ix.usize()).filter(|n| n.id != 0)
    }

    pub(crate) fn require(&self, ix: NodeIx) -> Result<&Node> {
        self.get(ix).ok_or(ApplyError::Internal)
    }

    pub(crate) fn require_mut(&mut self, ix: NodeIx) -> Result<&mut Node> {
        self.get_mut(ix).ok_or(ApplyError::Internal)
    }

    /// Place a node, reusing a freed slot when there is one.
    ///
    /// Fails on a live duplicate id; the caller has already checked the node
    /// budget.
    pub(crate) fn alloc(&mut self, node: Node) -> Result<NodeIx> {
        if self.by_id.contains_key(&node.id) {
            return Err(ApplyError::DuplicateNode(node.id));
        }
        let id = node.id;
        let key = node.key;
        let ix = match self.free.pop() {
            Some(ix) => {
                let slot = self.nodes.get_mut(ix.usize()).ok_or(ApplyError::Internal)?;
                *slot = node;
                ix
            }
            None => {
                let ix = u32::try_from(self.nodes.len()).map_err(|_| ApplyError::TooManyNodes)?;
                self.nodes.push(node);
                NodeIx(ix)
            }
        };
        self.by_id.insert(id, ix);
        if key != 0 {
            self.by_key.entry(key).or_insert(ix);
        }
        self.live = self.live.saturating_add(1);
        Ok(ix)
    }

    /// Free `ix` and every descendant. Iterative: a deep subtree costs a
    /// `Vec` push per node, not a stack frame.
    ///
    /// Does not touch the parent's child list; the caller owns that edit.
    pub(crate) fn release(&mut self, ix: NodeIx) -> Result<()> {
        let mut stack = vec![ix];
        while let Some(cur) = stack.pop() {
            let node = self.nodes.get_mut(cur.usize()).ok_or(ApplyError::Internal)?;
            if node.id == 0 {
                return Err(ApplyError::Internal);
            }
            stack.append(&mut node.children);
            self.by_id.remove(&node.id);
            if node.key != 0 && self.by_key.get(&node.key) == Some(&cur) {
                self.by_key.remove(&node.key);
            }
            node.id = 0;
            node.text = None;
            node.props = Vec::new();
            node.handlers = Vec::new();
            node.parent = NodeIx::NONE;
            self.free.push(cur);
            self.live = self.live.saturating_sub(1);
        }
        Ok(())
    }

    /// Set [`dirty::SELF`] on `ix` and [`dirty::DESCENDANT`] on its ancestors,
    /// stopping at the first ancestor already marked — everything above it
    /// already is.
    pub(crate) fn mark_dirty(&mut self, ix: NodeIx) -> Result<()> {
        let node = self.require_mut(ix)?;
        node.dirty |= dirty::SELF;
        let mut cur = node.parent;
        while cur.is_some() {
            let node = self.require_mut(cur)?;
            if node.dirty & dirty::DESCENDANT != 0 {
                break;
            }
            node.dirty |= dirty::DESCENDANT;
            cur = node.parent;
        }
        Ok(())
    }

    /// Distance from the root, root at 1.
    pub(crate) fn depth(&self, ix: NodeIx) -> Result<u32> {
        let mut depth = 0u32;
        let mut cur = ix;
        while cur.is_some() {
            depth = depth.saturating_add(1);
            cur = self.require(cur)?.parent;
        }
        Ok(depth)
    }
}
