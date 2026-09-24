//! A session: the four tables, the tree, and `apply`.
//!
//! `apply` is all-or-nothing at the *session* level rather than the batch
//! level: an op that fails poisons the session, and only a successful `Mount`
//! lifts that. This is exactly the recovery the transport specifies — discard,
//! resync, rebuild — so there is no need to snapshot the tree before every
//! batch to be able to roll back.

use std::sync::Arc;

use eui_proto::limits::MAX_ISLANDS;
use eui_proto::{limits as proto, Batch, ColorRef, EventKind, Handler, NodeKind, Op, StyleRecord, Subtree, TextRef, Value};

use crate::arena::{dirty, Arena, Node, NodeIx};
use crate::error::{ApplyError, Result, Table};
use crate::hash::{FastMap, FastSet};
use crate::limits::Limits;
use crate::tables::DefineOnce;

/// How many font roles a session holds: `0` sans, `1` mono, and the
/// application's own up to [`eui_proto::limits::MAX_FONT_ROLE`].
const FONT_ROLES: usize = proto::MAX_FONT_ROLE as usize + 1;

/// A bytecode chunk as the session holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    /// Named by content hash; the bytes come from the asset endpoint.
    Hash([u8; proto::HASH_BYTES]),
    /// Delivered inline.
    Bytes(Vec<u8>),
}

/// One namespace of interned definitions: everything a server names by id.
///
/// A session has its own, and one more per live region it is showing
/// (01 §2.7). The set a node's ids are read against is the set belonging to
/// whoever sent that node.
#[derive(Debug)]
pub(crate) struct Tables {
    /// Each value once, shared with `atom_ids`: the reverse index used to
    /// hold a `String` copy of every atom, which made the 8 MiB atom quota
    /// 16 MiB of memory.
    pub(crate) atoms: DefineOnce<Arc<str>>,
    /// Value -> first id. Keyed on the server's strings, so it keeps std's
    /// seeded SipHash rather than the integer hasher of [`crate::hash`].
    pub(crate) atom_ids: std::collections::HashMap<Arc<str>, u32>,
    pub(crate) atom_bytes: usize,
    /// Inline chunk bytes defined in this namespace. Budgeted across all of
    /// them together (`Limits::max_chunk_total_bytes`), and counted per
    /// namespace so that the total is always the sum of the sets that exist.
    pub(crate) chunk_bytes: usize,
    pub(crate) styles: DefineOnce<StyleRecord>,
    pub(crate) colors: DefineOnce<u32>,
    pub(crate) chunks: DefineOnce<Chunk>,
    /// Font roles, by role byte, each holding the asset hashes of its faces
    /// (02 §5). A fixed, tiny namespace rather than a `DefineOnce` table:
    /// an id table hands out names, and these are not handed out — roles
    /// `0` and `1` already mean sans and mono before a server says anything,
    /// and the rest are slots an application fills. Rebinding one is
    /// therefore a change of mind, not a redefinition, and is allowed: the
    /// client drops what it shaped in the old face.
    pub(crate) fonts: [Option<Vec<[u8; proto::HASH_BYTES]>>; FONT_ROLES],
}

impl Tables {
    fn new(limits: Limits) -> Self {
        Self {
            atoms: DefineOnce::new(Table::Atom, limits.max_atoms),
            atom_ids: std::collections::HashMap::new(),
            atom_bytes: 0,
            chunk_bytes: 0,
            styles: DefineOnce::new(Table::Style, limits.max_styles),
            colors: DefineOnce::new(Table::Color, limits.max_colors),
            chunks: DefineOnce::new(Table::Chunk, limits.max_chunks),
            fonts: [const { None }; FONT_ROLES],
        }
    }
}

/// Session state: tables, tree, focus, and poison.
#[derive(Debug)]
pub struct Session {
    limits: Limits,
    /// What this session's own server has defined.
    ///
    /// Grouped rather than six fields because a live region (01 §2.7) has a
    /// set of its own: its ids are its server's, allocated from 1 like every
    /// other session's, and they mean nothing in here. They cannot be
    /// re-interned into this set either — `DefineOnce` is a dense vector
    /// filled by ids *this* server allocates, so any id the client chose for
    /// a region might be the next one the page is given, and the second write
    /// is a `Redefined`. Node ids are translated into one space because
    /// layout needs one tree; table ids are not, because nobody needs them to
    /// be and the arithmetic does not work.
    tables: Tables,
    /// Which session's batch is being applied right now (01 §2.7): `0` for
    /// the page, an island's index for one of its batches.
    ///
    /// A field rather than an argument threaded through forty call sites,
    /// and it is set and cleared by the two entry points that can change it.
    /// Everything below reads it to know whose ids an op names and whose
    /// tables a reference resolves against.
    applying: u16,
    /// One [`Tables`] per island, in the order they were opened (01 §2.7).
    ///
    /// Not one `Session` per island, which was the other design and is
    /// worse: the page would then need an arena, a layout and a paint for
    /// each, and neither the layout engine nor the painter would be able to
    /// stay ignorant of islands. One tree, one arena, and a node that says
    /// which set of tables its ids mean.
    ///
    /// Empty for every page that has no island, which is almost all of them.
    ///
    /// A slot is `None` once its island is closed ([`Session::close_island`])
    /// or its host is released (`prune_sets`), and the next island opened
    /// takes the lowest free one. They used to be pushed and never taken
    /// back: the ninth island a long session ever opened was refused however
    /// few were open, and each dead slot kept a whole set of tables alive.
    islands: Vec<Option<Tables>>,
    /// Where each island's content hangs: its owner index and the page node
    /// carrying the `island` prop. The boundary 01 §2.7 draws — that node
    /// belongs to the page, everything below it to the island.
    ///
    /// Pruned with the host. An arena index is a slot and not a name: once
    /// the host is released the next node allocated may land on the same
    /// index, and an island frame that still found it here would release a
    /// page node's children and graft under it.
    island_roots: Vec<(u16, NodeIx)>,
    arena: Arena,
    root: NodeIx,
    focused: NodeIx,
    /// Nodes grafted since the last [`Session::take_entrances`] whose style
    /// asks to be animated in (`animation` carries `ANIMATION_ENTER`, 03 §5).
    /// Only those: a mount is the whole tree, and a list that recorded all
    /// of it would hand the client a hundred thousand indices to discard.
    entrances: Vec<NodeIx>,
    /// The **ids** of nodes released since the last [`Session::take_exits`]
    /// whose style asked to leave rather than vanish (`animation` carries
    /// `ANIMATION_EXIT`, 03 §5), with the direction they asked to leave in.
    ///
    /// Ids and not indices, because the node is gone: an index would already
    /// have been handed to whatever was grafted next. What a client does with
    /// this is keep the *painting* it made of that subtree for as long as the
    /// leaving takes — the tree is not kept, and cannot be, which is what
    /// makes a departing page inert by construction rather than by a check at
    /// every door.
    exits: Vec<(u32, eui_proto::Motion, u32)>,
    /// The `audio` and `video` nodes in the tree, in the order they were
    /// grafted: what the client's players are told to agree with (03 §7,
    /// §8), kept here so agreeing is a look at these rather than a walk of
    /// fifty thousand rows after every batch.
    media: Vec<NodeIx>,
    /// The nodes with a `wake` handler (06 §1.1), likewise.
    wakers: Vec<NodeIx>,
    /// Nodes carrying a `location` handler, so the driver does not walk the
    /// tree to find them. Kept exactly as `wakers` is: both are answered on
    /// a clock rather than in response to anything, so both have to be
    /// found without a search per tick.
    locators: Vec<NodeIx>,
    /// The atom ids of the names the client reads per node — `spans`,
    /// `paths`, `row` — so a painter asks a struct rather than hashes a
    /// string per node per frame.
    known: WellKnown,
    /// `(node, previous style id)` for every style change since the last
    /// [`Session::take_style_changes`]: what a client transitions from.
    style_changes: Vec<(NodeIx, u32)>,
    /// `node -> the style the server last gave it`, for the nodes a local
    /// handler has restyled since the last batch. A local change is a
    /// preview the server never hears about, so the server's own diff
    /// cannot undo it: a hover that lit a card and a frame that arrived
    /// before the pointer left would have left the card lit for good.
    /// Applying a batch puts these back first.
    local_styles: FastMap<NodeIx, u32>,
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
            tables: Tables::new(limits),
            arena: Arena::default(),
            root: NodeIx::NONE,
            focused: NodeIx::NONE,
            entrances: Vec::new(),
            applying: 0,
            islands: Vec::new(),
            island_roots: Vec::new(),
            exits: Vec::new(),
            media: Vec::new(),
            wakers: Vec::new(),
            locators: Vec::new(),
            known: WellKnown::default(),
            style_changes: Vec::new(),
            local_styles: FastMap::default(),
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

    /// The interned tables `owner` reads: the page's for `0`, an island's
    /// for `1..` (01 §2.7).
    ///
    /// An owner that names no open island falls back to the page's, which
    /// cannot happen from a batch — `apply_region` refuses an index it did
    /// not hand out — and is the harmless answer for a node left over from
    /// an island that has since been closed.
    fn tables_of(&self, owner: u16) -> &Tables {
        usize::from(owner).checked_sub(1).and_then(|i| self.islands.get(i)).and_then(Option::as_ref).unwrap_or(&self.tables)
    }

    /// The tables of whichever session's batch is being applied.
    fn cur(&self) -> &Tables {
        self.tables_of(self.applying)
    }

    fn cur_mut(&mut self) -> &mut Tables {
        match usize::from(self.applying).checked_sub(1).and_then(|i| self.islands.get_mut(i)).and_then(Option::as_mut) {
            Some(t) => t,
            None => &mut self.tables,
        }
    }

    /// The tables the node at `ix` reads — its owner's.
    fn tables_at(&self, ix: NodeIx) -> &Tables {
        self.arena.get(ix).map_or(&self.tables, |n| self.tables_of(n.owner))
    }

    /// A node's index by server id, **in the page's space**.
    ///
    /// An island's ids are its own (01 §2.7), so this cannot answer for one:
    /// `1` names a node on the page and a different node in every island
    /// open on it. [`Session::lookup_in`] takes the owner.
    pub fn lookup(&self, id: u32) -> Option<NodeIx> {
        self.arena.lookup(0, id)
    }

    /// A node's index by server id in `owner`'s space — `0` for the page,
    /// and an island's index for one of its nodes.
    pub fn lookup_in(&self, owner: u16, id: u32) -> Option<NodeIx> {
        self.arena.lookup(owner, id)
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
            TextRef::Atom(id) => self.tables_at(ix).atoms.get(*id).map(|a| &**a),
            TextRef::Inline(s) => Some(s.as_str()),
        }
    }

    /// The node's computed style; id 0 is the default record.
    pub fn style_of(&self, ix: NodeIx) -> StyleRecord {
        self.arena.get(ix).and_then(|n| self.tables_of(n.owner).styles.get(n.style)).copied().unwrap_or_default()
    }

    /// The node's handler for `event`.
    pub fn handler(&self, ix: NodeIx, event: EventKind) -> Option<Handler> {
        self.arena.get(ix)?.handler(event)
    }

    /// An atom's value.
    pub fn atom(&self, id: u32) -> Option<&str> {
        self.tables.atoms.get(id).map(|a| &**a)
    }

    /// The first atom defined with this exact value, if any. Used to find
    /// well-known prop names such as `columns` and `item_height`.
    pub fn atom_id(&self, value: &str) -> Option<u32> {
        self.tables.atom_ids.get(value).copied()
    }

    /// The atom ids of the names the client reads per node, as far as the
    /// server has defined them.
    pub fn atoms(&self) -> &WellKnown {
        &self.known
    }

    /// Spec 03 §3: an `input` carrying `secret: true` is a password field.
    /// The client paints marks, measures those, and copies nothing from it.
    /// A `textarea` ignores the prop: a secret that wraps is not a password.
    pub fn is_secret(&self, ix: NodeIx) -> bool {
        let Some(node) = self.node(ix) else {
            return false;
        };
        if node.kind != NodeKind::Input {
            return false;
        }
        let Some(atom) = self.known.secret else {
            return false;
        };
        matches!(node.prop(atom), Some(Value::Bool(true)))
    }

    /// The `audio` and `video` nodes in the tree (03 §7, §8).
    pub fn media(&self) -> &[NodeIx] {
        &self.media
    }

    /// The nodes with a `wake` handler (06 §1.1).
    pub fn wakers(&self) -> &[NodeIx] {
        &self.wakers
    }

    /// The nodes that asked where the machine is (06 §1.2).
    pub fn locators(&self) -> &[NodeIx] {
        &self.locators
    }

    /// Forget the nodes that were released.
    fn prune_sets(&mut self) {
        let arena = &self.arena;
        self.media.retain(|ix| arena.get(*ix).is_some_and(|n| n.id != 0));
        self.wakers.retain(|ix| arena.get(*ix).is_some_and(|n| n.id != 0));
        self.locators.retain(|ix| arena.get(*ix).is_some_and(|n| n.id != 0));
        // An island whose host went — a page `Mount`, a `Replace`, a
        // `RemoveChild` — went with it: its content was below the host and
        // was released in the same walk. Its tables and its slot go now,
        // before anything is grafted that could take the host's index.
        let islands = &mut self.islands;
        self.island_roots.retain(|(owner, at)| {
            let alive = arena.get(*at).is_some_and(|n| n.id != 0);
            if !alive {
                if let Some(slot) = usize::from(*owner).checked_sub(1).and_then(|i| islands.get_mut(i)) {
                    *slot = None;
                }
            }
            alive
        });
    }

    /// A style record.
    pub fn style(&self, id: u32) -> Option<&StyleRecord> {
        self.tables.styles.get(id)
    }

    /// A literal colour, `0xRRGGBBAA`.
    pub fn color(&self, id: u32) -> Option<u32> {
        self.tables.colors.get(id).copied()
    }

    /// A chunk, by hash or inline.
    pub fn chunk(&self, id: u32) -> Option<&Chunk> {
        self.tables.chunks.get(id)
    }

    /// The faces bound to a font role, or `None` for one nothing bound.
    pub fn font(&self, role: u8) -> Option<&[[u8; proto::HASH_BYTES]]> {
        self.tables.fonts.get(usize::from(role))?.as_deref()
    }

    /// Every bound role, lowest first, with its faces. What the client turns
    /// into asset requests.
    pub fn fonts(&self) -> impl Iterator<Item = (u8, &[[u8; proto::HASH_BYTES]])> {
        self.tables.fonts.iter().enumerate().filter_map(|(role, faces)| Some((role as u8, faces.as_deref()?)))
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
    pub fn set_style_local(&mut self, ix: NodeIx, style: u32) -> Option<bool> {
        if style != 0 && self.cur().styles.get(style).is_none() {
            return None;
        }
        let old = self.arena.get(ix)?.style;
        // A restyle that only recolours -- a hover, mostly -- owes a
        // repaint and not a layout: nothing the node measures changed.
        let record = |id: u32| if id == 0 { Some(StyleRecord::default()) } else { self.cur().styles.get(id).copied() };
        let paint_only = matches!((record(old), record(style)), (Some(a), Some(b)) if same_layout(&a, &b));
        let n = self.arena.get_mut(ix)?;
        n.style = style;
        self.local_styles.entry(ix).or_insert(old);
        self.style_changes.push((ix, old));
        let marked = if paint_only { self.arena.mark_painted(ix) } else { self.arena.mark_dirty(ix) };
        marked.ok().map(|()| paint_only)
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

    /// Write one float of a `scene` node's uniform block (03 §1.2).
    ///
    /// Marked **painted** and not dirty. A uniform changes what the node
    /// draws and nothing it measures, so no layout is owed — which is the
    /// same distinction a scroll already draws, and it matters for the same
    /// reason: a chunk following the pointer writes this sixty times a
    /// second, and sixty relayouts a second is the cost this whole design
    /// exists to avoid.
    ///
    /// Three refusals, and they are the reason this is a method of its own
    /// rather than a `set_prop` of a list: a node that is not a scene, an
    /// index outside the eight the block holds, and a value that is not a
    /// finite number.
    pub fn set_scene_uniform_local(&mut self, ix: NodeIx, index: u32, value: f64) -> bool {
        let Some(atom) = self.known.uniforms else { return false };
        if index >= 8 || !value.is_finite() {
            return false;
        }
        let Some(n) = self.arena.get_mut(ix) else { return false };
        if n.kind != NodeKind::Scene {
            return false;
        }
        let slot = match n.props.iter_mut().find(|(a, _)| *a == atom) {
            Some(slot) => slot,
            None => {
                if n.props.len() >= proto::MAX_PROPS as usize {
                    return false;
                }
                n.props.push((atom, Value::List(vec![Value::Float(0.0); 8])));
                let Some(slot) = n.props.last_mut() else { return false };
                slot
            }
        };
        // A server may have sent fewer than eight; the block is eight wide,
        // so it is filled out rather than written past.
        let Value::List(vs) = &mut slot.1 else {
            slot.1 = Value::List(vec![Value::Float(0.0); 8]);
            let Value::List(vs) = &mut slot.1 else { return false };
            if let Some(v) = vs.get_mut(index as usize) {
                *v = Value::Float(value);
            }
            return self.arena.mark_painted(ix).is_ok();
        };
        vs.resize(8, Value::Float(0.0));
        if let Some(v) = vs.get_mut(index as usize) {
            *v = Value::Float(value);
        }
        self.arena.mark_painted(ix).is_ok()
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
        self.tables.atom_bytes
    }

    /// Bytes of inline chunks defined so far, by the page and every island
    /// together — the figure `Limits::max_chunk_total_bytes` bounds.
    pub fn chunk_bytes(&self) -> usize {
        self.islands.iter().flatten().fold(self.tables.chunk_bytes, |sum, t| sum.saturating_add(t.chunk_bytes))
    }

    /// Number of atoms, styles, colours and chunks defined.
    pub fn table_sizes(&self) -> [usize; 4] {
        [self.tables.atoms.len(), self.tables.styles.len(), self.tables.colors.len(), self.tables.chunks.len()]
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

    /// The nodes grafted since the last call whose style asks to be animated
    /// in (03 §5): what a client starts an entrance from.
    pub fn take_entrances(&mut self) -> Vec<NodeIx> {
        std::mem::take(&mut self.entrances)
    }

    /// The nodes released since the last call whose style asked to leave
    /// rather than vanish (03 §5), as `(id, direction)`.
    /// Each entry is the node's id, the direction its style asked to leave
    /// in, and its **key** — the last of which is what 03 §5.3 pairs on: a
    /// node arriving under the same key is the same thing in a new place,
    /// and the two fly between their boxes instead of each going the way its
    /// page goes. `0` is a positional node and pairs with nothing.
    pub fn take_exits(&mut self) -> Vec<(u32, eui_proto::Motion, u32)> {
        std::mem::take(&mut self.exits)
    }

    /// Record a subtree about to be released, if its root asked to leave.
    ///
    /// Only the root. A page is one node as far as leaving is concerned, and
    /// walking the subtree would hand a client a hundred thousand ids to
    /// throw away — the same reason `entrances` records only what was
    /// grafted wearing the bit.
    fn note_exit(&mut self, ix: NodeIx) {
        if ix.is_none() {
            return;
        }
        let Some(node) = self.arena.get(ix) else { return };
        let (id, style, key, owner) = (node.id, node.style, node.key, node.owner);
        if style == 0 {
            return;
        }
        // The node's own tables: a node leaving an island wears a style the
        // island defined, and the page's table would answer for a different
        // record under the same id.
        if let Some(r) = self.tables_of(owner).styles.get(style) {
            if r.animation & eui_proto::ANIMATION_EXIT != 0 {
                self.exits.push((id, r.motion, key));
            }
        }
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
        self.arena.clear_dirty_from(root);
    }

    // -------------------------------------------------------------- apply

    /// Apply a batch. On error the session is poisoned and the caller must
    /// resync.
    ///
    /// By value, so what the decoder just allocated — atom strings, prop
    /// values, inline chunks of up to 64 KiB, whole subtrees — is moved into
    /// the session rather than copied out of a batch that is then dropped.
    /// A caller that still wants something from the batch afterwards takes
    /// it out first.
    pub fn apply(&mut self, batch: Batch) -> Result<()> {
        if let Some(last) = self.last_seq {
            if batch.seq <= last {
                self.poisoned = true;
                return Err(ApplyError::OutOfOrder { last, got: batch.seq });
            }
        }
        self.restore_local_styles();
        for op in batch.ops {
            if let Err(e) = self.apply_owned(op) {
                self.poisoned = true;
                return Err(e);
            }
        }
        self.last_seq = Some(batch.seq);
        Ok(())
    }

    /// Open an island (01 §2.7): a session of its own whose content hangs
    /// under `at`, a node of the page.
    ///
    /// Returns the owner index its batches must be applied under. `None`
    /// when the page already holds [`MAX_ISLANDS`] of them — past the
    /// ceiling a client opens no more and leaves the node as it was
    /// rendered, which §2.7 asks for in as many words and 10 §1 counts.
    /// The ceiling exists because a tree is data: a view that derived an
    /// island per row would otherwise open a socket per row.
    ///
    /// The ceiling counts islands **open**, not islands ever opened: a slot
    /// freed by [`Session::close_island`] or by its host going is taken
    /// again, lowest first.
    pub fn open_island(&mut self, at: NodeIx) -> Option<u16> {
        if !self.arena.get(at).is_some_and(|n| n.id != 0) {
            return None;
        }
        let i = match self.islands.iter().position(Option::is_none) {
            Some(i) => i,
            None if self.islands.len() < MAX_ISLANDS => {
                self.islands.push(None);
                self.islands.len().saturating_sub(1)
            }
            None => return None,
        };
        let owner = u16::try_from(i.saturating_add(1)).ok()?;
        *self.islands.get_mut(i)? = Some(Tables::new(self.limits));
        self.island_roots.push((owner, at));
        Some(owner)
    }

    /// Whether `owner` names an island that is open now — opened, and
    /// neither closed nor gone with its host.
    pub fn island_is_open(&self, owner: u16) -> bool {
        self.island_roots.iter().any(|(o, _)| *o == owner)
    }

    /// Close an island for good: its content, its tables and its slot.
    ///
    /// Not what an island whose socket *ended* gets — 01 §2.7 has that one
    /// leave the page alone, content and all, and the client keeps it open
    /// here for as long as its host stands. This is for an island the page
    /// no longer asks for: its host lost the prop, or names another path.
    /// The host's own children that the island never replaced — the cached
    /// render, still showing because the island never mounted — belong to
    /// the page and stay; the ones the island grafted are released, since
    /// once its tables are gone nothing could say what they look like.
    ///
    /// Returns `false` for an owner that is not open.
    pub fn close_island(&mut self, owner: u16) -> bool {
        let Some(pos) = self.island_roots.iter().position(|(o, _)| *o == owner) else { return false };
        let (_, at) = self.island_roots.remove(pos);
        let theirs: Vec<NodeIx> = self.children(at).iter().copied().filter(|c| self.arena.get(*c).is_some_and(|n| n.owner == owner)).collect();
        for child in &theirs {
            if self.focused_within(*child) {
                self.focused = NodeIx::NONE;
            }
            self.note_exit(*child);
            let _ = self.arena.release(*child);
        }
        if let Some(n) = self.arena.get_mut(at) {
            n.children.retain(|c| !theirs.contains(c));
        }
        if !theirs.is_empty() {
            let _ = self.arena.mark_dirty(at);
        }
        if let Some(slot) = usize::from(owner).checked_sub(1).and_then(|i| self.islands.get_mut(i)) {
            *slot = None;
        }
        self.prune_sets();
        true
    }

    /// Apply a batch that came from an island's socket.
    ///
    /// Everything inside is resolved in that island's space: its node ids,
    /// its atoms, its styles, its colours, its chunks and its font roles.
    /// The one thing it may not do is reach the page — `find` cannot see a
    /// node the island did not create, because the id index is keyed by
    /// owner — which is 01 §2.7's "no id it sends can name a node it did
    /// not create", enforced by the shape of the map rather than by a check
    /// that could be forgotten.
    ///
    /// An island's `Mount` replaces **that island's content and nothing
    /// else**: the node carrying the prop belongs to the page, everything
    /// below it to the island.
    ///
    /// An owner that is not open — never opened, closed, or gone with its
    /// host — is refused and nothing is applied.
    pub fn apply_region(&mut self, owner: u16, batch: &Batch) -> Result<()> {
        let at = self.island_roots.iter().find(|(o, _)| *o == owner).map(|(_, ix)| *ix).ok_or(ApplyError::Internal)?;
        let prev = std::mem::replace(&mut self.applying, owner);
        let out = self.apply_within(at, batch);
        self.applying = prev;
        out
    }

    /// The body of [`Session::apply_region`], so that `applying` is put back
    /// on every road out of it — including the error ones, of which there
    /// are several and each of which would otherwise leave the whole session
    /// resolving the page's ops against an island's tables.
    fn apply_within(&mut self, at: NodeIx, batch: &Batch) -> Result<()> {
        for op in &batch.ops {
            match op {
                // An island's `Mount` is a graft under its node, not a new
                // document: the page keeps its root, its focus and every
                // other island.
                Op::Mount(subtree) => {
                    let old: Vec<NodeIx> = self.children(at).to_vec();
                    for child in old {
                        if self.focused_within(child) {
                            self.focused = NodeIx::NONE;
                        }
                        self.note_exit(child);
                        self.arena.release(child)?;
                    }
                    if let Some(n) = self.arena.get_mut(at) {
                        n.children.clear();
                    }
                    self.prune_sets();
                    let depth = self.depth_of(at).saturating_add(1);
                    let root = self.graft(subtree.clone(), at, depth)?;
                    if let Some(n) = self.arena.get_mut(at) {
                        n.children.push(root);
                    }
                    self.arena.mark_dirty(at)?;
                }
                other => self.apply_op(other)?,
            }
        }
        Ok(())
    }

    /// How deep `ix` sits, so an island's graft is checked against the same
    /// depth ceiling as the page it hangs in — an island cannot be a way
    /// round 02's limit.
    fn depth_of(&self, ix: NodeIx) -> u32 {
        let mut d = 1u32;
        let mut cur = self.arena.get(ix).map_or(NodeIx::NONE, |n| n.parent);
        while cur.is_some() {
            d = d.saturating_add(1);
            cur = match self.arena.get(cur) {
                Some(n) => n.parent,
                None => break,
            };
        }
        d
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
    ///
    /// Borrowed, so the op is copied; [`Session::apply`] moves its ops in.
    pub fn apply_op(&mut self, op: &Op) -> Result<()> {
        self.apply_owned(op.clone())
    }

    fn apply_owned(&mut self, op: Op) -> Result<()> {
        match op {
            Op::DefAtom { id, value } => {
                let total = self.cur().atom_bytes.saturating_add(value.len());
                if total > self.limits.max_atom_total_bytes {
                    return Err(ApplyError::AtomBudget);
                }
                let value: Arc<str> = Arc::from(value);
                self.cur_mut().atoms.define(id, Arc::clone(&value))?;
                // The well-known names stay the page's: they are how *this*
                // client recognises a prop, and an island that interns
                // `item_height` under an id of its own means the same by it.
                self.known.note(id, &value);
                let tables = self.cur_mut();
                tables.atom_ids.entry(value).or_insert(id);
                tables.atom_bytes = total;
                Ok(())
            }
            Op::DefStyle { id, record } => {
                self.check_style_record(&record)?;
                self.cur_mut().styles.define(id, record)
            }
            Op::DefColor { id, rgba } => self.cur_mut().colors.define(id, rgba),
            Op::DefChunk { id, hash } => self.cur_mut().chunks.define(id, Chunk::Hash(hash)),
            Op::DefChunkBytes { id, bytes } => {
                // Budgeted like atoms, and across every namespace at once:
                // 64 KiB times 4 095 ids is 256 MiB a namespace, and a page
                // with its eight islands has nine, which a hostile server
                // could fill over as many batches as it liked.
                let len = bytes.len();
                if self.chunk_bytes().saturating_add(len) > self.limits.max_chunk_total_bytes {
                    return Err(ApplyError::ChunkBudget);
                }
                self.cur_mut().chunks.define(id, Chunk::Bytes(bytes))?;
                let tables = self.cur_mut();
                tables.chunk_bytes = tables.chunk_bytes.saturating_add(len);
                Ok(())
            }
            Op::DefFont { role, faces } => {
                // The decoder already bounded the role and the face count;
                // this is the slot's own check, so a session built by hand
                // in a test cannot write past the array either.
                let slot = self.cur_mut().fonts.get_mut(usize::from(role)).ok_or(ApplyError::UnknownFontRole(role))?;
                *slot = Some(faces);
                Ok(())
            }
            Op::Mount(subtree) => self.mount(subtree),
            // The one op that names no node (02 §5.2): what it changes is
            // not the document but what somebody is told. So it is here,
            // beside the definitions, rather than below the two questions
            // this crate asks of a tree op — a notification in the same
            // batch as the mount that opens a window is a legitimate thing
            // for a server to send, and so is one during a resync.
            //
            // Nothing is kept: the client reads it off the batch again,
            // and who acts on it is not this crate.
            Op::Notify { .. } => Ok(()),
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

    fn apply_tree_op(&mut self, op: Op) -> Result<()> {
        match op {
            Op::Replace { node, subtree } => self.replace(node, subtree),
            Op::SetStyle { node, style } => {
                if style != 0 {
                    self.cur().styles.require(style)?;
                }
                let ix = self.find(node)?;
                let n = self.arena.require_mut(ix)?;
                let old = n.style;
                n.style = style;
                self.local_styles.remove(&ix);
                self.style_changes.push((ix, old));
                self.arena.mark_dirty(ix)
            }
            Op::SetText { node, text } => {
                let ix = self.find(node)?;
                self.check_text(&text)?;
                let n = self.arena.require_mut(ix)?;
                if n.kind.is_inert() {
                    return Err(ApplyError::InertNode(node));
                }
                n.text = Some(text);
                self.arena.mark_dirty(ix)
            }
            Op::SetProp { node, prop, value } => {
                let ix = self.find(node)?;
                self.cur().atoms.require(prop)?;
                self.check_value(&value)?;
                let n = self.arena.require_mut(ix)?;
                if n.kind.is_inert() {
                    return Err(ApplyError::InertNode(node));
                }
                match n.props.iter_mut().find(|(a, _)| *a == prop) {
                    Some(slot) => slot.1 = value,
                    None => {
                        if n.props.len() >= proto::MAX_PROPS as usize {
                            return Err(ApplyError::TooManyProps(node));
                        }
                        n.props.push((prop, value));
                    }
                }
                self.arena.mark_dirty(ix)
            }
            Op::InsertChild { parent, index, subtree } => {
                let pix = self.find(parent)?;
                let p = self.arena.require(pix)?;
                if p.kind.is_leaf() {
                    return Err(ApplyError::NotAContainer(parent));
                }
                let len = p.children.len() as u32;
                if index > len {
                    return Err(ApplyError::ChildIndexOutOfRange { parent, index, len });
                }
                let depth = self.arena.depth(pix)?;
                let child = self.graft(subtree, pix, depth.saturating_add(1))?;
                self.arena.require_mut(pix)?.children.insert(index as usize, child);
                self.arena.mark_dirty(pix)
            }
            Op::RemoveChild { parent, index, count } => {
                let pix = self.find(parent)?;
                let len = self.arena.require(pix)?.children.len() as u32;
                let end = index.checked_add(count).ok_or(ApplyError::ChildIndexOutOfRange { parent, index, len })?;
                if end > len {
                    return Err(ApplyError::ChildIndexOutOfRange { parent, index: end, len });
                }
                let removed: Vec<NodeIx> = self.arena.require_mut(pix)?.children.drain(index as usize..end as usize).collect();
                for ix in removed {
                    if self.focused_within(ix) {
                        self.focused = NodeIx::NONE;
                    }
                    self.note_exit(ix);
                    self.arena.release(ix)?;
                }
                // Once for the whole range, not once per child: each call
                // walks the media, wake and location lists, and a list
                // emptied row by row made that rows times their length.
                self.prune_sets();
                self.arena.mark_dirty(pix)
            }
            Op::MoveChild { parent, from, to } => {
                let pix = self.find(parent)?;
                let children = &mut self.arena.require_mut(pix)?.children;
                let len = children.len() as u32;
                if from >= len {
                    return Err(ApplyError::ChildIndexOutOfRange { parent, index: from, len });
                }
                if to >= len {
                    return Err(ApplyError::ChildIndexOutOfRange { parent, index: to, len });
                }
                if from != to {
                    let child = children.remove(from as usize);
                    children.insert(to as usize, child);
                }
                self.arena.mark_dirty(pix)
            }
            Op::SetHandler { node, event, handler } => {
                let ix = self.find(node)?;
                self.check_handler(&handler)?;
                let n = self.arena.require_mut(ix)?;
                if n.kind.is_inert() {
                    return Err(ApplyError::InertNode(node));
                }
                // A node is in `wakers` exactly when it carries a `wake`
                // handler, and `locators` likewise: `graft` adds it with the
                // handler, `ClearHandler` and `prune_sets` take it away. So
                // whether it is already listed is whether it already had
                // one — a look at at most sixteen handlers on the node, not
                // a search of every waker on the page.
                let had = n.handler(event).is_some();
                match n.handlers.iter_mut().find(|(e, _)| *e == event) {
                    Some(slot) => slot.1 = handler,
                    None => {
                        if n.handlers.len() >= proto::MAX_HANDLERS as usize {
                            return Err(ApplyError::TooManyHandlers(node));
                        }
                        n.handlers.push((event, handler));
                    }
                }
                if !had && event == EventKind::Wake {
                    self.wakers.push(ix);
                }
                if !had && event == EventKind::Location {
                    self.locators.push(ix);
                }
                Ok(())
            }
            Op::ClearHandler { node, event } => {
                let ix = self.find(node)?;
                let n = self.arena.require_mut(ix)?;
                let had = n.handler(event).is_some();
                n.handlers.retain(|(e, _)| *e != event);
                if had && event == EventKind::Wake {
                    self.wakers.retain(|w| *w != ix);
                }
                if had && event == EventKind::Location {
                    self.locators.retain(|w| *w != ix);
                }
                Ok(())
            }
            Op::Focus { node } => {
                self.focused = self.find(node)?;
                Ok(())
            }
            Op::ScrollTo { node, x, y } => {
                let ix = self.find(node)?;
                let n = self.arena.require_mut(ix)?;
                if !matches!(n.kind, NodeKind::Scroll | NodeKind::List) {
                    return Err(ApplyError::NotScrollable(node));
                }
                n.scroll = (x, y);
                self.arena.mark_scrolled(ix)
            }
            // Handled by `apply_op`.
            Op::Notify { .. } | Op::DefAtom { .. } | Op::DefStyle { .. } | Op::DefColor { .. } | Op::DefChunk { .. } | Op::DefChunkBytes { .. } | Op::DefFont { .. } | Op::Mount(_) => {
                Err(ApplyError::Internal)
            }
        }
    }

    fn mount(&mut self, subtree: Subtree) -> Result<()> {
        if self.root.is_some() {
            self.note_exit(self.root);
            self.arena.release(self.root)?;
            self.prune_sets();
            self.root = NodeIx::NONE;
        }
        // Between the old tree and the new, nothing is live: the one moment
        // the arena can hand back what the largest page it ever held left
        // behind.
        self.arena.shrink_if_empty(subtree.nodes.len());
        self.focused = NodeIx::NONE;
        let root = self.graft(subtree, NodeIx::NONE, 1)?;
        self.root = root;
        self.poisoned = false;
        self.arena.mark_dirty(root)
    }

    fn replace(&mut self, id: u32, subtree: Subtree) -> Result<()> {
        let old = self.find(id)?;
        let parent = self.arena.require(old)?.parent;
        if self.focused_within(old) {
            self.focused = NodeIx::NONE;
        }
        if parent.is_none() {
            self.note_exit(old);
            self.arena.release(old)?;
            self.prune_sets();
            self.root = NodeIx::NONE;
            let root = self.graft(subtree, NodeIx::NONE, 1)?;
            self.root = root;
            return self.arena.mark_dirty(root);
        }
        let position = self.arena.require(parent)?.children.iter().position(|c| *c == old).ok_or(ApplyError::Internal)?;
        self.note_exit(old);
        self.arena.release(old)?;
        self.prune_sets();
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
    ///
    /// **All or nothing.** Every refusal a subtree can earn — a bad
    /// reference, an id already live, an id used twice *within* the subtree,
    /// a node past the depth limit, a shape that does not close — is found
    /// before the first node is allocated. It used to be that the last three
    /// were found during placement, which left the nodes placed so far live
    /// in the arena with no parent: the session was poisoned as it should
    /// be, and then the resync's `Mount`, carrying the same ids, was refused
    /// as a duplicate of those orphans, and the session closed. Placement can
    /// now fail only on a broken arena invariant, and even then what was
    /// placed is released before the error is returned.
    fn graft(&mut self, mut subtree: Subtree, parent: NodeIx, depth: u32) -> Result<NodeIx> {
        let incoming = u32::try_from(subtree.nodes.len()).map_err(|_| ApplyError::TooManyNodes)?;
        if incoming == 0 {
            return Err(ApplyError::CannotRemoveRoot);
        }
        if self.arena.live().saturating_add(incoming) > self.limits.max_nodes {
            return Err(ApplyError::TooManyNodes);
        }

        // Validate every reference first, so a bad node deep in the subtree
        // does not leave half a graft in the arena before poisoning.
        let mut ids: FastSet<u32> = FastSet::with_capacity_and_hasher(subtree.nodes.len(), Default::default());
        for flat in &subtree.nodes {
            if flat.style != 0 {
                self.cur().styles.require(flat.style)?;
            }
            if let Some(t) = &flat.text {
                self.check_text(t)?;
            }
            for (prop, value) in subtree.props_of(flat) {
                self.cur().atoms.require(*prop)?;
                self.check_value(value)?;
            }
            for (_, handler) in subtree.handlers_of(flat) {
                self.check_handler(handler)?;
            }
            if !ids.insert(flat.id) || self.arena.lookup(self.applying, flat.id).is_some() {
                return Err(ApplyError::DuplicateNode(flat.id));
            }
        }
        drop(ids);

        // Then the shape, walked exactly as placement will walk it but
        // allocating nothing: every node's depth against the limit, and a
        // subtree that is one tree and closes.
        let mut open: Vec<(u32, u32)> = Vec::new();
        for (i, flat) in subtree.nodes.iter().enumerate() {
            let d = match open.last() {
                Some(&(_, d)) => d,
                // Past the first node, an empty stack is a second root: a
                // node placement would hang from nothing. The decoder refuses
                // this, so reaching here is a caller bug.
                None if i > 0 => return Err(ApplyError::Internal),
                None => depth,
            };
            if d > self.limits.max_depth {
                return Err(ApplyError::TooDeep);
            }
            if let Some(top) = open.last_mut() {
                top.0 = top.0.saturating_sub(1);
            }
            if flat.child_count > 0 {
                open.push((flat.child_count, d.saturating_add(1)));
            } else {
                while matches!(open.last(), Some(&(0, _))) {
                    open.pop();
                }
            }
        }
        if !open.is_empty() {
            // A subtree that promised more children than it carried. The
            // decoder rejects this, so reaching here is a caller bug.
            return Err(ApplyError::Internal);
        }

        let entrances = self.entrances.len();
        let mut root = NodeIx::NONE;
        match self.place(&mut subtree, parent, depth, &mut root) {
            Ok(()) => Ok(root),
            Err(e) => {
                if root.is_some() {
                    // Every node placed hangs below the root by now, so this
                    // takes all of them; the lists that noted any are pruned.
                    let _ = self.arena.release(root);
                }
                self.entrances.truncate(entrances);
                self.prune_sets();
                Err(e)
            }
        }
    }

    /// The placement half of [`Session::graft`], on a subtree it has already
    /// checked. Writes the root to `root` as soon as it exists, so a failure
    /// after it can be undone.
    ///
    /// Takes the subtree's text and prop values rather than copying them —
    /// the batch they came in is being consumed.
    fn place(&mut self, subtree: &mut Subtree, parent: NodeIx, depth: u32, root: &mut NodeIx) -> Result<()> {
        let mut props = std::mem::take(&mut subtree.props);
        let all_handlers = std::mem::take(&mut subtree.handlers);
        // Pre-order placement. `open` holds (parent index, children still to
        // come, depth of those children).
        let mut open: Vec<(NodeIx, u32, u32)> = Vec::new();
        for flat in &mut subtree.nodes {
            let (p, d) = match open.last() {
                Some(&(p, _, d)) => (p, d),
                None => (parent, depth),
            };
            let (start, len) = flat.props;
            let own: Vec<(u32, Value)> = props
                .get_mut(start as usize..(start as usize).saturating_add(len as usize))
                .map(|slice| slice.iter_mut().map(|(a, v)| (*a, std::mem::replace(v, Value::Null))).collect())
                .unwrap_or_default();
            let (start, len) = flat.handlers;
            let handlers = all_handlers.get(start as usize..(start as usize).saturating_add(len as usize)).unwrap_or(&[]).to_vec();
            let wakes = handlers.iter().any(|(e, _)| *e == EventKind::Wake);
            let locates = handlers.iter().any(|(e, _)| *e == EventKind::Location);
            let ix = self.arena.alloc(Node {
                id: flat.id,
                kind: flat.kind,
                style: flat.style,
                key: flat.key,
                text: flat.text.take(),
                props: own,
                handlers,
                parent: p,
                children: Vec::with_capacity(flat.child_count as usize),
                scroll: (0, 0),
                dirty: dirty::SELF,
                owner: self.applying,
            })?;
            if root.is_none() {
                *root = ix;
            }
            // Every new node in the session passes through here — `Mount`,
            // `Replace` and `InsertChild` all graft — so this is the one
            // place an entrance can be noticed.
            if flat.style != 0 && self.cur().styles.get(flat.style).is_some_and(|r| r.animation & eui_proto::ANIMATION_ENTER != 0) {
                self.entrances.push(ix);
            }
            if matches!(flat.kind, NodeKind::Audio | NodeKind::Video) {
                self.media.push(ix);
            }
            if wakes {
                self.wakers.push(ix);
            }
            if locates {
                self.locators.push(ix);
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
        Ok(())
    }

    // ---------------------------------------------------------- checking

    /// The node an op names, in the space of the session whose batch is
    /// being applied (01 §2.7). `applying` is `0` for the page and the
    /// island's index inside one of its batches.
    fn find(&self, id: u32) -> Result<NodeIx> {
        self.arena.lookup(self.applying, id).ok_or(ApplyError::UnknownNode(id))
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
            self.cur().atoms.require(*id)?;
        }
        Ok(())
    }

    fn check_color(&self, c: ColorRef) -> Result<()> {
        if c.is_literal() {
            self.cur().colors.require(u32::from(c.index()))?;
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
            Value::Atom(id) => self.cur().atoms.require(*id).map(|_| ()),
            Value::Color(c) => self.check_color(*c),
            Value::List(items) => items.iter().try_for_each(|i| self.check_value(i)),
            _ => Ok(()),
        }
    }

    fn check_handler(&self, h: &Handler) -> Result<()> {
        match h {
            Handler::Server(name) => self.cur().atoms.require(*name).map(|_| ()),
            Handler::Local(chunk) => self.cur().chunks.require(*chunk).map(|_| ()),
            Handler::LocalThenServer { chunk, name } => {
                self.cur().chunks.require(*chunk)?;
                self.cur().atoms.require(*name).map(|_| ())
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

/// The atom ids of the names the client reads per node, as far as the
/// server has defined them: a painter asking for a text node's `spans`
/// three thousand times a frame asks a field, not a hash table.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WellKnown {
    /// Per-glyph colour spans on a text node.
    pub spans: Option<u32>,
    /// A canvas node's paths.
    pub paths: Option<u32>,
    /// An icon node's name, per spec 03 §1.
    pub name: Option<u32>,
    /// A grid's column count.
    pub columns: Option<u32>,
    /// A windowed list child's row.
    pub row: Option<u32>,
    /// A virtualised list's row height.
    pub item_height: Option<u32>,
    /// A windowed list's row count.
    pub count: Option<u32>,
    /// A windowed list's row heights.
    pub heights: Option<u32>,
    /// The path of the session an **island** takes its content from
    /// (01 §2.7). An absolute path on the same origin; anything else is
    /// refused by the client rather than dialled.
    pub island: Option<u32>,
    /// A node's wake period.
    pub wake: Option<u32>,
    /// How often a node wants to be told where the machine is, in ms.
    pub locate: Option<u32>,
    /// A node that wants `pointer_move` only while a button is held: a
    /// splitter, a slider, anything dragged. Without it a hover costs a
    /// round trip per pointer position (06 §1).
    pub drag_only: Option<u32>,
    /// A node that can be picked up, and the group it belongs to (03 §3.4).
    pub drag: Option<u32>,
    /// A node that takes what others carry, and the groups it takes.
    pub accepts: Option<u32>,
    /// A grip inside a draggable node: a press here grabs at once, with no
    /// slop to cross and no hold to wait out. It is what makes a row
    /// draggable by a finger without stealing the stroke that scrolls the
    /// list it is in (06 §5).
    pub drag_handle: Option<u32>,
    /// Which way a container's slots run, and so which arrows move them.
    pub drag_axis: Option<u32>,
    /// A node that is a value on a line: a slider, a range, a scrubber
    /// (03 §3.4). Its presence is the declaration, and its value says
    /// which way the line runs. The client resolves the whole gesture
    /// against it and reports only the value, so unlike `drag_only` there
    /// is no `pointer_move` here to gate.
    pub track: Option<u32>,
    /// The value at the start of the line.
    pub track_min: Option<u32>,
    /// The value at its end.
    pub track_max: Option<u32>,
    /// The quantum the value lands on, and the arrows move by.
    pub track_step: Option<u32>,
    /// Where the handle is, or both handles.
    pub track_value: Option<u32>,
    /// Which piece of a track a descendant draws: the groove, the fill, or
    /// a thumb.
    pub track_part: Option<u32>,
    /// A media node's asset.
    pub src: Option<u32>,
    /// Whether it plays.
    pub playing: Option<u32>,
    /// Whether it loops.
    pub loop_: Option<u32>,
    /// Where it is.
    pub position: Option<u32>,
    /// How loud.
    pub volume: Option<u32>,
    /// An `input` that hides what was typed (03 §3).
    pub secret: Option<u32>,
    /// A scene's WGSL module, by content hash (03 §1.2).
    pub shader: Option<u32>,
    /// A scene's geometry, by content hash.
    pub mesh: Option<u32>,
    /// A scene's eight floats: the author's half of the uniform block. The
    /// client fills the other half -- the matrix, the clock, the size -- so
    /// a server never sends a camera and cannot send a broken one.
    pub uniforms: Option<u32>,
    /// Frames a second an animating scene asks for, capped by the client.
    pub fps: Option<u32>,
    /// Samples a scene asks to be drawn with: 1 or 4.
    pub msaa: Option<u32>,
}

impl WellKnown {
    fn note(&mut self, id: u32, value: &str) {
        let slot = match value {
            "spans" => &mut self.spans,
            "paths" => &mut self.paths,
            "name" => &mut self.name,
            "columns" => &mut self.columns,
            "row" => &mut self.row,
            "item_height" => &mut self.item_height,
            "count" => &mut self.count,
            "heights" => &mut self.heights,
            "island" => &mut self.island,
            "wake" => &mut self.wake,
            "locate" => &mut self.locate,
            "drag_only" => &mut self.drag_only,
            "drag" => &mut self.drag,
            "accepts" => &mut self.accepts,
            "drag_handle" => &mut self.drag_handle,
            "drag_axis" => &mut self.drag_axis,
            "track" => &mut self.track,
            "track_min" => &mut self.track_min,
            "track_max" => &mut self.track_max,
            "track_step" => &mut self.track_step,
            "track_value" => &mut self.track_value,
            "track_part" => &mut self.track_part,
            "src" => &mut self.src,
            "playing" => &mut self.playing,
            "loop" => &mut self.loop_,
            "position" => &mut self.position,
            "volume" => &mut self.volume,
            "secret" => &mut self.secret,
            "shader" => &mut self.shader,
            "mesh" => &mut self.mesh,
            "uniforms" => &mut self.uniforms,
            "fps" => &mut self.fps,
            "msaa" => &mut self.msaa,
            _ => return,
        };
        slot.get_or_insert(id);
    }
}

/// One disc per Unicode scalar. A password of `i`s and a password of `W`s
/// occupy the same width, which is the whole point of masking.
pub fn secret_display(text: &str) -> String {
    text.chars().map(|_| crate::SECRET_MARK).collect()
}

/// A byte offset in `text` as a byte offset in [`secret_display`].
pub fn secret_offset(text: &str, byte: usize) -> usize {
    let mut n = byte.min(text.len());
    while n > 0 && !text.is_char_boundary(n) {
        n = n.saturating_sub(1);
    }
    text[..n].chars().count().saturating_mul(crate::SECRET_MARK.len_utf8())
}

/// A byte offset in [`secret_display`] as a byte offset in `text`.
pub fn secret_unoffset(text: &str, display_byte: usize) -> usize {
    let i = display_byte.checked_div(crate::SECRET_MARK.len_utf8()).unwrap_or(0);
    text.char_indices().nth(i).map(|(b, _)| b).unwrap_or(text.len())
}

/// Whether two records lay out alike: everything but what only the
/// painter reads -- colours, radius, shadow, opacity, blur, cursor, and
/// how a change is animated.
pub fn same_layout(a: &StyleRecord, b: &StyleRecord) -> bool {
    let painted_like_a = StyleRecord {
        bg: a.bg,
        fg: a.fg,
        border_color: a.border_color,
        radius: a.radius,
        shadow: a.shadow,
        opacity: a.opacity,
        blur: a.blur,
        cursor: a.cursor,
        transition: a.transition,
        animation: a.animation,
        text_align: a.text_align,
        ..*b
    };
    painted_like_a == *a
}
