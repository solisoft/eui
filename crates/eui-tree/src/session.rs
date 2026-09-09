//! A session: the four tables, the tree, and `apply`.
//!
//! `apply` is all-or-nothing at the *session* level rather than the batch
//! level: an op that fails poisons the session, and only a successful `Mount`
//! lifts that. This is exactly the recovery the transport specifies — discard,
//! resync, rebuild — so there is no need to snapshot the tree before every
//! batch to be able to roll back.

use eui_proto::{limits as proto, Batch, ColorRef, EventKind, Handler, NodeKind, Op, StyleRecord, Subtree, TextRef, Value};

use crate::arena::{dirty, Arena, Node, NodeIx};
use crate::error::{ApplyError, Result, Table};
use crate::limits::Limits;
use crate::tables::DefineOnce;

/// A bytecode chunk as the session holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    /// Named by content hash; the bytes come from the asset endpoint.
    Hash([u8; proto::HASH_BYTES]),
    /// Delivered inline.
    Bytes(Vec<u8>),
}

/// Session state: tables, tree, focus, and poison.
#[derive(Debug)]
pub struct Session {
    limits: Limits,
    atoms: DefineOnce<String>,
    atom_ids: std::collections::HashMap<String, u32>,
    atom_bytes: usize,
    styles: DefineOnce<StyleRecord>,
    colors: DefineOnce<u32>,
    chunks: DefineOnce<Chunk>,
    arena: Arena,
    root: NodeIx,
    focused: NodeIx,
    /// `(node, previous style id)` for every style change since the last
    /// [`Session::take_style_changes`]: what a client transitions from.
    style_changes: Vec<(NodeIx, u32)>,
    /// `node -> the style the server last gave it`, for the nodes a local
    /// handler has restyled since the last batch. A local change is a
    /// preview the server never hears about, so the server's own diff
    /// cannot undo it: a hover that lit a card and a frame that arrived
    /// before the pointer left would have left the card lit for good.
    /// Applying a batch puts these back first.
    local_styles: std::collections::HashMap<NodeIx, u32>,
    /// Whether the last batch put any of those back, so the client knows
    /// to run the pointer's `enter` again over the fresh tree.
    restored_local: bool,
    poisoned: bool,
    last_seq: Option<u64>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// A session with the protocol's default quotas.
    pub fn new() -> Self {
        Self::with_limits(Limits::default())
    }

    /// A session with explicit quotas.
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            limits,
            atoms: DefineOnce::new(Table::Atom, limits.max_atoms),
            atom_ids: std::collections::HashMap::new(),
            atom_bytes: 0,
            styles: DefineOnce::new(Table::Style, limits.max_styles),
            colors: DefineOnce::new(Table::Color, limits.max_colors),
            chunks: DefineOnce::new(Table::Chunk, limits.max_chunks),
            arena: Arena::default(),
            root: NodeIx::NONE,
            focused: NodeIx::NONE,
            style_changes: Vec::new(),
            local_styles: std::collections::HashMap::new(),
            restored_local: false,
            poisoned: false,
            last_seq: None,
        }
    }

    // ------------------------------------------------------------ queries

    /// The root, once mounted.
    pub fn root(&self) -> Option<NodeIx> {
        if self.root.is_some() && !self.poisoned {
            Some(self.root)
        } else {
            None
        }
    }

    /// A node by arena index.
    pub fn node(&self, ix: NodeIx) -> Option<&Node> {
        self.arena.get(ix)
    }

    /// A node's index by server id.
    pub fn lookup(&self, id: u32) -> Option<NodeIx> {
        self.arena.lookup(id)
    }

    /// The first live node whose key is the atom `key`. This is how a local
    /// handler names its target without depending on one render's ids.
    pub fn lookup_key(&self, key: u32) -> Option<NodeIx> {
        self.arena.lookup_key(key)
    }

    /// The children of `ix`, in order.
    pub fn children(&self, ix: NodeIx) -> &[NodeIx] {
        self.arena.get(ix).map(|n| n.children.as_slice()).unwrap_or(&[])
    }

    /// The node's text, atom resolved.
    pub fn text_of(&self, ix: NodeIx) -> Option<&str> {
        match self.arena.get(ix)?.text.as_ref()? {
            TextRef::Atom(id) => self.atoms.get(*id).map(String::as_str),
            TextRef::Inline(s) => Some(s.as_str()),
        }
    }

    /// The node's computed style; id 0 is the default record.
    pub fn style_of(&self, ix: NodeIx) -> StyleRecord {
        self.arena.get(ix).and_then(|n| self.styles.get(n.style)).copied().unwrap_or_default()
    }

    /// The node's handler for `event`.
    pub fn handler(&self, ix: NodeIx, event: EventKind) -> Option<Handler> {
        self.arena.get(ix)?.handler(event)
    }

    /// An atom's value.
    pub fn atom(&self, id: u32) -> Option<&str> {
        self.atoms.get(id).map(String::as_str)
    }

    /// The first atom defined with this exact value, if any. Used to find
    /// well-known prop names such as `columns` and `item_height`.
    pub fn atom_id(&self, value: &str) -> Option<u32> {
        self.atom_ids.get(value).copied()
    }

    /// A style record.
    pub fn style(&self, id: u32) -> Option<&StyleRecord> {
        self.styles.get(id)
    }

    /// A literal colour, `0xRRGGBBAA`.
    pub fn color(&self, id: u32) -> Option<u32> {
        self.colors.get(id).copied()
    }

    /// A chunk, by hash or inline.
    pub fn chunk(&self, id: u32) -> Option<&Chunk> {
        self.chunks.get(id)
    }

    /// The props of the root node: a component's local state
    /// (`spec/07-bytecode.md` §1).
    pub fn root_prop(&self, atom: u32) -> Option<&Value> {
        let root = self.root()?;
        self.arena.get(root)?.prop(atom)
    }

    /// Set a root prop from a local handler. Returns `false` with no tree.
    pub fn set_root_prop_local(&mut self, atom: u32, value: Value) -> bool {
        let Some(root) = self.root() else { return false };
        let Some(n) = self.arena.get_mut(root) else { return false };
        match n.props.iter_mut().find(|(a, _)| *a == atom) {
            Some(slot) => slot.1 = value,
            None => {
                if n.props.len() >= proto::MAX_PROPS as usize {
                    return false;
                }
                n.props.push((atom, value));
            }
        }
        true
    }

    /// Point a node at a style id from a local handler. The id must exist;
    /// a chunk cannot invent styles, only pick among the session's.
    pub fn set_style_local(&mut self, ix: NodeIx, style: u32) -> bool {
        if style != 0 && self.styles.get(style).is_none() {
            return false;
        }
        match self.arena.get_mut(ix) {
            Some(n) => {
                let old = n.style;
                n.style = style;
                self.local_styles.entry(ix).or_insert(old);
                self.style_changes.push((ix, old));
                self.arena.mark_dirty(ix).is_ok()
            }
            None => false,
        }
    }

    /// Set any node's prop from a local handler; `false` for an unknown node.
    pub fn set_prop_local(&mut self, ix: NodeIx, atom: u32, value: Value) -> bool {
        let Some(n) = self.arena.get_mut(ix) else { return false };
        if n.kind.is_inert() {
            return false;
        }
        match n.props.iter_mut().find(|(a, _)| *a == atom) {
            Some(slot) => slot.1 = value,
            None => {
                if n.props.len() >= proto::MAX_PROPS as usize {
                    return false;
                }
                n.props.push((atom, value));
            }
        }
        self.arena.mark_dirty(ix).is_ok()
    }

    /// The focused node.
    pub fn focused(&self) -> Option<NodeIx> {
        if self.focused.is_some() {
            Some(self.focused)
        } else {
            None
        }
    }

    /// Live node count.
    pub fn live_nodes(&self) -> u32 {
        self.arena.live()
    }

    /// One past the highest arena index ever used — the size a per-node side
    /// table needs.
    pub fn arena_len(&self) -> usize {
        self.arena.len()
    }

    /// Bytes of atom values defined so far.
    pub fn atom_bytes(&self) -> usize {
        self.atom_bytes
    }

    /// Number of atoms, styles, colours and chunks defined.
    pub fn table_sizes(&self) -> [usize; 4] {
        [self.atoms.len(), self.styles.len(), self.colors.len(), self.chunks.len()]
    }

    /// True after a failed apply, until the next successful `Mount`.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    /// The last batch applied.
    pub fn last_seq(&self) -> Option<u64> {
        self.last_seq
    }

    /// Depth of `ix`, root at 1.
    pub fn depth(&self, ix: NodeIx) -> Option<u32> {
        self.arena.depth(ix).ok()
    }

    /// Pre-order traversal from `ix`, iterative.
    pub fn preorder(&self, ix: NodeIx) -> Preorder<'_> {
        Preorder { session: self, stack: if self.arena.get(ix).is_some() { vec![ix] } else { Vec::new() } }
    }

    /// Set a scroll node's offsets from local input. The client clamps against
    /// the layout's content size; this only records the value.
    pub fn set_scroll(&mut self, ix: NodeIx, x: i64, y: i64) -> bool {
        match self.arena.get_mut(ix) {
            Some(n) if matches!(n.kind, NodeKind::Scroll | NodeKind::List) => {
                n.scroll = (x, y);
                self.arena.mark_scrolled(ix).is_ok()
            }
            _ => false,
        }
    }

    /// Replace a node's text from local input or a local handler, ahead of
    /// the server. The next server `SetText` wins; this is the optimistic
    /// half of typing and of a local increment.
    pub fn set_text_local(&mut self, ix: NodeIx, text: String) -> bool {
        match self.arena.get_mut(ix) {
            Some(n) if !n.kind.is_inert() => {
                n.text = Some(TextRef::Inline(text));
                self.arena.mark_dirty(ix).is_ok()
            }
            _ => false,
        }
    }

    /// The style changes since the last call, oldest first, as `(node,
    /// previous style id)`. A node removed since is still listed; look it up.
    pub fn take_style_changes(&mut self) -> Vec<(NodeIx, u32)> {
        std::mem::take(&mut self.style_changes)
    }

    /// True when any node changed since the dirty bits were last cleared.
    pub fn is_dirty(&self) -> bool {
        self.root().and_then(|r| self.arena.get(r)).is_some_and(|n| n.dirty != 0)
    }

    /// Clear the dirty bits on `ix` alone.
    pub fn clear_dirty(&mut self, ix: NodeIx) {
        if let Some(n) = self.arena.get_mut(ix) {
            n.dirty = 0;
        }
    }

    /// Clear every dirty bit. Walks only the dirty paths: a node with no
    /// bit set has no dirty descendant (`mark_dirty` marks every ancestor),
    /// so a scroll step over ten thousand rows clears a handful of nodes.
    pub fn clear_all_dirty(&mut self) {
        let Some(root) = self.root() else { return };
        let mut stack = vec![root];
        while let Some(ix) = stack.pop() {
            let Some(n) = self.arena.get_mut(ix) else { continue };
            if n.dirty == 0 {
                continue;
            }
            n.dirty = 0;
            let children = n.children.clone();
            stack.extend(children.into_iter().filter(|c| self.arena.get(*c).is_some_and(|n| n.dirty != 0)));
        }
    }

    // -------------------------------------------------------------- apply

    /// Apply a batch. On error the session is poisoned and the caller must
    /// resync.
    pub fn apply(&mut self, batch: &Batch) -> Result<()> {
        if let Some(last) = self.last_seq {
            if batch.seq <= last {
                self.poisoned = true;
                return Err(ApplyError::OutOfOrder { last, got: batch.seq });
            }
        }
        self.restore_local_styles();
        for op in &batch.ops {
            if let Err(e) = self.apply_op(op) {
                self.poisoned = true;
                return Err(e);
            }
        }
        self.last_seq = Some(batch.seq);
        Ok(())
    }

    /// Put every previewed style back to what the server last said, so a
    /// batch diffs against the tree the server believes it sent. The
    /// client re-runs the pointer's `enter` after the batch, so a card
    /// still under the pointer lights again in the same frame.
    fn restore_local_styles(&mut self) {
        for (ix, style) in std::mem::take(&mut self.local_styles) {
            if let Some(n) = self.arena.get_mut(ix) {
                if n.style != style {
                    let old = n.style;
                    n.style = style;
                    self.style_changes.push((ix, old));
                    self.restored_local = true;
                    let _ = self.arena.mark_dirty(ix);
                }
            }
        }
    }

    /// Whether the last batch undid a previewed style, and clear the flag.
    pub fn take_restored_local(&mut self) -> bool {
        std::mem::take(&mut self.restored_local)
    }

    /// Apply one op. Definitions and `Mount` are accepted while poisoned;
    /// everything else needs a healthy tree.
    pub fn apply_op(&mut self, op: &Op) -> Result<()> {
        match op {
            Op::DefAtom { id, value } => {
                let total = self.atom_bytes.saturating_add(value.len());
                if total > self.limits.max_atom_total_bytes {
                    return Err(ApplyError::AtomBudget);
                }
                self.atoms.define(*id, value.clone())?;
                self.atom_ids.entry(value.clone()).or_insert(*id);
                self.atom_bytes = total;
                Ok(())
            }
            Op::DefStyle { id, record } => {
                self.check_style_record(record)?;
                self.styles.define(*id, *record)
            }
            Op::DefColor { id, rgba } => self.colors.define(*id, *rgba),
            Op::DefChunk { id, hash } => self.chunks.define(*id, Chunk::Hash(*hash)),
            Op::DefChunkBytes { id, bytes } => self.chunks.define(*id, Chunk::Bytes(bytes.clone())),
            Op::Mount(subtree) => self.mount(subtree),
            _ => {
                if self.poisoned {
                    return Err(ApplyError::Poisoned);
                }
                if self.root.is_none() {
                    return Err(ApplyError::NoTree);
                }
                self.apply_tree_op(op)
            }
        }
    }

    fn apply_tree_op(&mut self, op: &Op) -> Result<()> {
        match op {
            Op::Replace { node, subtree } => self.replace(*node, subtree),
            Op::SetStyle { node, style } => {
                if *style != 0 {
                    self.styles.require(*style)?;
                }
                let ix = self.find(*node)?;
                let n = self.arena.require_mut(ix)?;
                let old = n.style;
                n.style = *style;
                self.local_styles.remove(&ix);
                self.style_changes.push((ix, old));
                self.arena.mark_dirty(ix)
            }
            Op::SetText { node, text } => {
                let ix = self.find(*node)?;
                self.check_text(text)?;
                let n = self.arena.require_mut(ix)?;
                if n.kind.is_inert() {
                    return Err(ApplyError::InertNode(*node));
                }
                n.text = Some(text.clone());
                self.arena.mark_dirty(ix)
            }
            Op::SetProp { node, prop, value } => {
                let ix = self.find(*node)?;
                self.atoms.require(*prop)?;
                self.check_value(value)?;
                let n = self.arena.require_mut(ix)?;
                if n.kind.is_inert() {
                    return Err(ApplyError::InertNode(*node));
                }
                match n.props.iter_mut().find(|(a, _)| a == prop) {
                    Some(slot) => slot.1 = value.clone(),
                    None => {
                        if n.props.len() >= proto::MAX_PROPS as usize {
                            return Err(ApplyError::TooManyProps(*node));
                        }
                        n.props.push((*prop, value.clone()));
                    }
                }
                self.arena.mark_dirty(ix)
            }
            Op::InsertChild { parent, index, subtree } => {
                let pix = self.find(*parent)?;
                let p = self.arena.require(pix)?;
                if p.kind.is_leaf() {
                    return Err(ApplyError::NotAContainer(*parent));
                }
                let len = p.children.len() as u32;
                if *index > len {
                    return Err(ApplyError::ChildIndexOutOfRange { parent: *parent, index: *index, len });
                }
                let depth = self.arena.depth(pix)?;
                let child = self.graft(subtree, pix, depth.saturating_add(1))?;
                self.arena.require_mut(pix)?.children.insert(*index as usize, child);
                self.arena.mark_dirty(pix)
            }
            Op::RemoveChild { parent, index, count } => {
                let pix = self.find(*parent)?;
                let len = self.arena.require(pix)?.children.len() as u32;
                let end = index.checked_add(*count).ok_or(ApplyError::ChildIndexOutOfRange { parent: *parent, index: *index, len })?;
                if end > len {
                    return Err(ApplyError::ChildIndexOutOfRange { parent: *parent, index: end, len });
                }
                let removed: Vec<NodeIx> = self.arena.require_mut(pix)?.children.drain(*index as usize..end as usize).collect();
                for ix in removed {
                    if self.focused_within(ix) {
                        self.focused = NodeIx::NONE;
                    }
                    self.arena.release(ix)?;
                }
                self.arena.mark_dirty(pix)
            }
            Op::MoveChild { parent, from, to } => {
                let pix = self.find(*parent)?;
                let children = &mut self.arena.require_mut(pix)?.children;
                let len = children.len() as u32;
                if *from >= len {
                    return Err(ApplyError::ChildIndexOutOfRange { parent: *parent, index: *from, len });
                }
                if *to >= len {
                    return Err(ApplyError::ChildIndexOutOfRange { parent: *parent, index: *to, len });
                }
                if from != to {
                    let child = children.remove(*from as usize);
                    children.insert(*to as usize, child);
                }
                self.arena.mark_dirty(pix)
            }
            Op::SetHandler { node, event, handler } => {
                let ix = self.find(*node)?;
                self.check_handler(handler)?;
                let n = self.arena.require_mut(ix)?;
                if n.kind.is_inert() {
                    return Err(ApplyError::InertNode(*node));
                }
                match n.handlers.iter_mut().find(|(e, _)| e == event) {
                    Some(slot) => slot.1 = *handler,
                    None => {
                        if n.handlers.len() >= proto::MAX_HANDLERS as usize {
                            return Err(ApplyError::TooManyHandlers(*node));
                        }
                        n.handlers.push((*event, *handler));
                    }
                }
                Ok(())
            }
            Op::ClearHandler { node, event } => {
                let ix = self.find(*node)?;
                self.arena.require_mut(ix)?.handlers.retain(|(e, _)| e != event);
                Ok(())
            }
            Op::Focus { node } => {
                self.focused = self.find(*node)?;
                Ok(())
            }
            Op::ScrollTo { node, x, y } => {
                let ix = self.find(*node)?;
                let n = self.arena.require_mut(ix)?;
                if !matches!(n.kind, NodeKind::Scroll | NodeKind::List) {
                    return Err(ApplyError::NotScrollable(*node));
                }
                n.scroll = (*x, *y);
                self.arena.mark_scrolled(ix)
            }
            // Handled by `apply_op`.
            Op::DefAtom { .. } | Op::DefStyle { .. } | Op::DefColor { .. } | Op::DefChunk { .. } | Op::DefChunkBytes { .. } | Op::Mount(_) => Err(ApplyError::Internal),
        }
    }

    fn mount(&mut self, subtree: &Subtree) -> Result<()> {
        if self.root.is_some() {
            self.arena.release(self.root)?;
            self.root = NodeIx::NONE;
        }
        self.focused = NodeIx::NONE;
        let root = self.graft(subtree, NodeIx::NONE, 1)?;
        self.root = root;
        self.poisoned = false;
        self.arena.mark_dirty(root)
    }

    fn replace(&mut self, id: u32, subtree: &Subtree) -> Result<()> {
        let old = self.find(id)?;
        let parent = self.arena.require(old)?.parent;
        if self.focused_within(old) {
            self.focused = NodeIx::NONE;
        }
        if parent.is_none() {
            self.arena.release(old)?;
            self.root = NodeIx::NONE;
            let root = self.graft(subtree, NodeIx::NONE, 1)?;
            self.root = root;
            return self.arena.mark_dirty(root);
        }
        let position = self.arena.require(parent)?.children.iter().position(|c| *c == old).ok_or(ApplyError::Internal)?;
        self.arena.release(old)?;
        let depth = self.arena.depth(parent)?;
        let fresh = self.graft(subtree, parent, depth.saturating_add(1))?;
        let siblings = &mut self.arena.require_mut(parent)?.children;
        match siblings.get_mut(position) {
            Some(slot) => *slot = fresh,
            None => return Err(ApplyError::Internal),
        }
        self.arena.mark_dirty(parent)
    }

    /// Validate a subtree against the tables and quotas, then place it under
    /// `parent` with its root at `depth`. Returns the new root's index.
    fn graft(&mut self, subtree: &Subtree, parent: NodeIx, depth: u32) -> Result<NodeIx> {
        let incoming = u32::try_from(subtree.nodes.len()).map_err(|_| ApplyError::TooManyNodes)?;
        if incoming == 0 {
            return Err(ApplyError::CannotRemoveRoot);
        }
        if self.arena.live().saturating_add(incoming) > self.limits.max_nodes {
            return Err(ApplyError::TooManyNodes);
        }

        // Validate every reference first, so a bad node deep in the subtree
        // does not leave half a graft in the arena before poisoning.
        for flat in &subtree.nodes {
            if flat.style != 0 {
                self.styles.require(flat.style)?;
            }
            if let Some(t) = &flat.text {
                self.check_text(t)?;
            }
            for (prop, value) in subtree.props_of(flat) {
                self.atoms.require(*prop)?;
                self.check_value(value)?;
            }
            for (_, handler) in subtree.handlers_of(flat) {
                self.check_handler(handler)?;
            }
            if self.arena.lookup(flat.id).is_some() {
                return Err(ApplyError::DuplicateNode(flat.id));
            }
        }

        // Pre-order placement. `open` holds (parent index, children still to
        // come, depth of those children).
        let mut open: Vec<(NodeIx, u32, u32)> = Vec::new();
        let mut root = NodeIx::NONE;
        for flat in &subtree.nodes {
            let (p, d) = match open.last() {
                Some(&(p, _, d)) => (p, d),
                None => (parent, depth),
            };
            if d > self.limits.max_depth {
                return Err(ApplyError::TooDeep);
            }
            let ix = self.arena.alloc(Node {
                id: flat.id,
                kind: flat.kind,
                style: flat.style,
                key: flat.key,
                text: flat.text.clone(),
                props: subtree.props_of(flat).to_vec(),
                handlers: subtree.handlers_of(flat).to_vec(),
                parent: p,
                children: Vec::with_capacity(flat.child_count as usize),
                scroll: (0, 0),
                dirty: dirty::SELF,
            })?;
            if root.is_none() {
                root = ix;
            }
            if let Some(top) = open.last_mut() {
                self.arena.require_mut(top.0)?.children.push(ix);
                top.1 = top.1.saturating_sub(1);
            }
            if flat.child_count > 0 {
                open.push((ix, flat.child_count, d.saturating_add(1)));
            } else {
                while matches!(open.last(), Some(&(_, 0, _))) {
                    open.pop();
                }
            }
        }
        if !open.is_empty() {
            // A subtree that promised more children than it carried. The
            // decoder rejects this, so reaching here is a caller bug.
            return Err(ApplyError::Internal);
        }
        Ok(root)
    }

    // ---------------------------------------------------------- checking

    fn find(&self, id: u32) -> Result<NodeIx> {
        self.arena.lookup(id).ok_or(ApplyError::UnknownNode(id))
    }

    fn focused_within(&self, ix: NodeIx) -> bool {
        let mut cur = self.focused;
        while cur.is_some() {
            if cur == ix {
                return true;
            }
            cur = self.arena.get(cur).map(|n| n.parent).unwrap_or(NodeIx::NONE);
        }
        false
    }

    fn check_text(&self, text: &TextRef) -> Result<()> {
        if let TextRef::Atom(id) = text {
            self.atoms.require(*id)?;
        }
        Ok(())
    }

    fn check_color(&self, c: ColorRef) -> Result<()> {
        if c.is_literal() {
            self.colors.require(u32::from(c.index()))?;
        }
        Ok(())
    }

    fn check_style_record(&self, r: &StyleRecord) -> Result<()> {
        self.check_color(r.bg)?;
        self.check_color(r.fg)?;
        self.check_color(r.border_color)
    }

    fn check_value(&self, v: &Value) -> Result<()> {
        match v {
            Value::Atom(id) => self.atoms.require(*id).map(|_| ()),
            Value::Color(c) => self.check_color(*c),
            Value::List(items) => items.iter().try_for_each(|i| self.check_value(i)),
            _ => Ok(()),
        }
    }

    fn check_handler(&self, h: &Handler) -> Result<()> {
        match h {
            Handler::Server(name) => self.atoms.require(*name).map(|_| ()),
            Handler::Local(chunk) => self.chunks.require(*chunk).map(|_| ()),
            Handler::LocalThenServer { chunk, name } => {
                self.chunks.require(*chunk)?;
                self.atoms.require(*name).map(|_| ())
            }
        }
    }
}

/// Iterative pre-order traversal.
pub struct Preorder<'a> {
    session: &'a Session,
    stack: Vec<NodeIx>,
}

impl Iterator for Preorder<'_> {
    type Item = NodeIx;

    fn next(&mut self) -> Option<NodeIx> {
        let ix = self.stack.pop()?;
        let children = self.session.children(ix);
        self.stack.extend(children.iter().rev().copied());
        Some(ix)
    }
}
