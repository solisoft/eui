//! The layout engine (`spec/04-layout.md`).
//!
//! Two phases. `measure` answers "how big would this node be under these
//! constraints" and is memoised per frame and pure. `arrange` runs exactly
//! once per node, top-down, with the node's final size, and is the only thing
//! that writes positions. Keeping them apart is what stops a child measured
//! under three different constraints from keeping positions computed under
//! the wrong one.

use std::collections::HashMap;

use eui_proto::{AlignItems, AlignSelf, Display, Justify, NodeKind, Position, Value, Wrap};
use eui_theme::Resolved;
use eui_tree::{NodeIx, Session};

use crate::geom::{Constraint, Rect, Size};
use crate::measure::TextMeasurer;
use crate::style::{Length, Style};

/// Border-box size and first baseline, margins excluded. `content_w` /
/// `content_h` are the border-box size the content alone asks for, before the
/// node's own `width`/`height` and the parent's `Exact` constraint apply —
/// the CSS min-content size, used for the automatic minimum of §4.3.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Metrics {
    w: f32,
    h: f32,
    baseline: f32,
    content_w: f32,
    content_h: f32,
}

/// One child placed inside its container, relative to the container's
/// border-box origin, margins already applied.
#[derive(Debug, Clone, Copy)]
struct Placed {
    ix: NodeIx,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    /// Skipped by virtualisation: assigned a size, never measured.
    virtual_: bool,
}

/// What placing a container's children produced.
#[derive(Debug, Default)]
struct Placement {
    children: Vec<Placed>,
    /// Extent of the content, from the content-box origin.
    content: Size,
    /// First in-flow child's baseline, relative to the content-box origin.
    baseline: Option<f32>,
}

/// `(node, width constraint, height constraint)`.
/// `(node index, node id, width constraint, height constraint)`.
type MemoKey = (u32, u32, (u8, u32), (u8, u32));

/// Per-frame work counters, for the budget harness.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Stats {
    /// `measure` calls that missed the memo and did work.
    pub measures: u32,
    /// `measure` calls served from the memo.
    pub memo_hits: u32,
    /// Times a virtualised list was placed.
    pub list_placements: u32,
    /// Rows of virtualised lists that were actually measured.
    pub rows_measured: u32,
    /// Rows assigned by arithmetic and never visited.
    pub rows_virtual: u32,
    /// Memoised measures kept after the frame: what the next one can reuse.
    pub memo_size: u32,
}

/// Per-frame results, indexed by [`NodeIx::raw`].
#[derive(Debug, Default)]
pub struct Layout {
    stats: Stats,
    rect: Vec<Rect>,
    baseline: Vec<f32>,
    content: Vec<Size>,
    present: Vec<bool>,
    virtual_: Vec<bool>,
    /// Resolved records by style id: a 10 000-row table has four distinct
    /// styles, so resolving per node was 10 000 resolves for four answers —
    /// and a per-node cache was seven megabytes of memset per frame.
    by_style_id: HashMap<u32, Style>,
    memo: HashMap<MemoKey, (Metrics, u32)>,
    /// The frame being computed; memo entries not read or written in it
    /// are dropped at its end, so the memo never outgrows one frame's work.
    generation: u32,
    columns_atom: Option<u32>,
    item_height_atom: Option<u32>,
    viewport: Size,
}

/// What the engine needs from outside for one frame. (Not `Frame`: that is
/// the wire frame in `eui-proto`, and the two meet in the client.)
pub struct Env<'a> {
    /// The tree.
    pub session: &'a Session,
    /// The viewer's resolved theme.
    pub theme: &'a Resolved,
    /// The text engine.
    pub text: &'a mut dyn TextMeasurer,
}

impl Layout {
    /// An empty layout.
    pub fn new() -> Self {
        Self::default()
    }

    /// Lay out the whole tree into `viewport`. A poisoned or empty session
    /// yields an empty layout.
    pub fn compute(&mut self, f: &mut Env<'_>, viewport: Size) {
        let n = f.session.arena_len();
        // Rects, baselines and content sizes are only ever read behind
        // `present`, so they need sizing, not clearing. The two bitmaps are
        // what a frame resets: fifty thousand bytes each, not megabytes.
        if self.rect.len() < n {
            self.rect.resize(n, Rect::default());
            self.baseline.resize(n, 0.0);
            self.content.resize(n, Size::default());
        }
        self.present.clear();
        self.present.resize(n, false);
        self.virtual_.clear();
        self.virtual_.resize(n, false);
        self.by_style_id.clear();
        // Measures survive across frames for nodes nothing touched: the
        // session's dirty bits say which subtrees changed (a node's own
        // change sets SELF, its ancestors' DESCENDANT), and an index reused
        // by a new node carries a different id. A scroll dirties only the
        // scroller and its ancestors, so a scrolled frame re-measures the
        // rows entering the window and nothing else.
        self.generation = self.generation.wrapping_add(1);
        self.memo.retain(|k, _| f.session.node(NodeIx::from_raw(k.0)).is_some_and(|n| n.id == k.1 && n.dirty == 0));
        self.stats = Stats::default();
        self.columns_atom = f.session.atom_id("columns");
        self.item_height_atom = f.session.atom_id("item_height");
        self.viewport = viewport;

        let Some(root) = f.session.root() else { return };
        let m = self.measure(f, root, Constraint::Exact(viewport.w), Constraint::Exact(viewport.h));
        self.arrange(f, root, 0.0, 0.0, m.w, m.h);
        // Rows scrolled out of a list are not dirty, so they would stay
        // memoised forever; a frame that did not touch them lets them go.
        let generation = self.generation;
        self.memo.retain(|_, (_, seen)| *seen == generation);
        self.stats.memo_size = self.memo.len() as u32;
    }

    /// Forget every memoised measure: the viewport, theme or scale changed,
    /// so nothing measured before applies.
    pub fn invalidate_all(&mut self) {
        self.memo.clear();
    }

    /// Work done by the last `compute`.
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// The node's absolute border box, if it was laid out this frame.
    pub fn rect(&self, ix: NodeIx) -> Option<Rect> {
        let i = ix.raw() as usize;
        if self.present.get(i).copied().unwrap_or(false) { self.rect.get(i).copied() } else { None }
    }

    /// True for a virtualised list row that was assigned a size but never
    /// measured: it has a rect and no laid-out content, so a painter must
    /// draw its box and nothing inside it.
    pub fn is_virtual(&self, ix: NodeIx) -> bool {
        self.virtual_.get(ix.raw() as usize).copied().unwrap_or(false)
    }

    /// First baseline from the node's top, if laid out.
    pub fn baseline(&self, ix: NodeIx) -> Option<f32> {
        self.rect(ix).and_then(|_| self.baseline.get(ix.raw() as usize).copied())
    }

    /// A `scroll` or `list` node's content extent, for clamping offsets.
    pub fn content_size(&self, ix: NodeIx) -> Option<Size> {
        self.rect(ix).and_then(|_| self.content.get(ix.raw() as usize).copied())
    }

    /// The deepest laid-out node under a point, honouring stack order and
    /// scroll clipping.
    pub fn hit(&self, s: &Session, x: f32, y: f32) -> Option<NodeIx> {
        let root = s.root()?;
        self.hit_in(s, root, x, y, Rect::new(f32::MIN / 2.0, f32::MIN / 2.0, f32::MAX, f32::MAX))
    }

    fn hit_in(&self, s: &Session, ix: NodeIx, x: f32, y: f32, clip: Rect) -> Option<NodeIx> {
        let rect = self.rect(ix)?;
        let node = s.node(ix)?;
        let clips = matches!(node.kind, NodeKind::Scroll | NodeKind::List);
        let inner_clip = if clips { clip.intersect(&rect) } else { clip };
        if clips && !inner_clip.contains(x, y) {
            return None;
        }
        // Topmost first: later children paint over earlier ones, higher z
        // paints over lower.
        let mut order: Vec<NodeIx> = node.children.clone();
        if node.kind == NodeKind::Box && self.style_of(s, ix).map(|st| st.display) == Some(Display::Stack) {
            order.sort_by_key(|c| self.style_of(s, *c).map(|st| st.z).unwrap_or(0));
        }
        for child in order.iter().rev() {
            if let Some(hit) = self.hit_in(s, *child, x, y, inner_clip) {
                return Some(hit);
            }
        }
        if rect.contains(x, y) && clip.contains(x, y) { Some(ix) } else { None }
    }

    /// The style resolved this frame for the node's style id; absent for a
    /// style id that no laid-out node used, which hit-testing treats as
    /// "not a stack" rather than guessing.
    fn style_of(&self, s: &Session, ix: NodeIx) -> Option<Style> {
        let style_id = s.node(ix)?.style;
        self.by_style_id.get(&style_id).copied()
    }

    fn style(&mut self, f: &Env<'_>, ix: NodeIx) -> Style {
        let style_id = f.session.node(ix).map_or(0, |n| n.style);
        if let Some(st) = self.by_style_id.get(&style_id) {
            return *st;
        }
        let st = Style::resolve(&f.session.style_of(ix), f.theme);
        self.by_style_id.insert(style_id, st);
        st
    }

    fn int_prop(&self, f: &Env<'_>, ix: NodeIx, atom: Option<u32>) -> Option<i64> {
        let atom = atom?;
        match f.session.node(ix)?.prop(atom)? {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    // ------------------------------------------------------------- measure

    fn measure(&mut self, f: &mut Env<'_>, ix: NodeIx, cw: Constraint, ch: Constraint) -> Metrics {
        let id = f.session.node(ix).map_or(0, |n| n.id);
        let key = (ix.raw(), id, cw.key(), ch.key());
        let generation = self.generation;
        if let Some((m, seen)) = self.memo.get_mut(&key) {
            *seen = generation;
            self.stats.memo_hits = self.stats.memo_hits.saturating_add(1);
            return *m;
        }
        self.stats.measures = self.stats.measures.saturating_add(1);
        let m = self.measure_uncached(f, ix, cw, ch);
        self.memo.insert(key, (m, generation));
        m
    }

    fn measure_uncached(&mut self, f: &mut Env<'_>, ix: NodeIx, cw: Constraint, ch: Constraint) -> Metrics {
        let st = self.style(f, ix);
        if st.display == Display::None {
            return Metrics::default();
        }
        let Some(node) = f.session.node(ix) else { return Metrics::default() };
        let kind = node.kind;

        // Exact wins: the parent decided. Otherwise the node's own dims.
        let own_w = match cw {
            Constraint::Exact(v) => Some(v),
            _ => st.width.resolve(cw),
        };
        let own_h = match ch {
            Constraint::Exact(v) => Some(v),
            _ => st.height.resolve(ch),
        };
        let inner_w = own_w.map_or(cw.shrink(st.inset_h()), |w| Constraint::Exact((w - st.inset_h()).max(0.0)));
        let inner_h = own_h.map_or(ch.shrink(st.inset_v()), |h| Constraint::Exact((h - st.inset_v()).max(0.0)));

        let (content, baseline) = match kind {
            NodeKind::Text | NodeKind::Input | NodeKind::TextArea => {
                let text = f.session.text_of(ix).unwrap_or("");
                let tm = f.text.measure(text, st.font, inner_w.bound(), st.line_clamp);
                let mut size = Size::new(tm.width, tm.height);
                if kind != NodeKind::Text {
                    // An editable field is at least one control tall and never
                    // collapses to zero width when empty.
                    size.h = size.h.max(f.theme.control.get(1).copied().unwrap_or(36.0) - st.inset_v());
                    size.w = size.w.max(st.font.size * 4.0);
                }
                (size, Some(tm.baseline))
            }
            NodeKind::Image | NodeKind::Icon => {
                let hash = node.props.iter().find_map(|(_, v)| match v {
                    Value::Asset(h) => Some(*h),
                    _ => None,
                });
                let (w, h) = hash.and_then(|h| f.text.asset_size(&h)).unwrap_or((0.0, 0.0));
                (Size::new(w, h), None)
            }
            NodeKind::Spacer => (Size::default(), None),
            NodeKind::Divider => (Size::new(1.0, 1.0), None),
            NodeKind::Scroll | NodeKind::List => {
                let p = self.place_scroll(f, ix, st, inner_w, inner_h, true);
                let baseline = p.baseline;
                let content = p.content;
                if let Some(slot) = self.content.get_mut(ix.raw() as usize) {
                    *slot = content;
                }
                // A scroll box is its bound, not its content.
                let visible = Size::new(inner_w.bound().unwrap_or(content.w), inner_h.bound().unwrap_or(content.h));
                (visible, baseline)
            }
            _ => {
                let p = self.place(f, ix, st, inner_w, inner_h);
                (p.content, p.baseline)
            }
        };

        let w = own_w.unwrap_or(content.w + st.inset_h());
        let h = own_h.unwrap_or(content.h + st.inset_v());
        let w = match cw {
            Constraint::Exact(v) => v,
            _ => st.clamp_w(w, cw),
        };
        let h = match ch {
            Constraint::Exact(v) => v,
            _ => st.clamp_h(h, ch),
        };
        let baseline = baseline.map_or(h, |b| b + st.padding.t + st.border.t);
        Metrics { w, h, baseline, content_w: content.w + st.inset_h(), content_h: content.h + st.inset_v() }
    }

    // ------------------------------------------------------------- arrange

    fn arrange(&mut self, f: &mut Env<'_>, ix: NodeIx, x: f32, y: f32, w: f32, h: f32) {
        let i = ix.raw() as usize;
        let st = self.style(f, ix);
        let m = self.measure(f, ix, Constraint::Exact(w), Constraint::Exact(h));
        if let Some(r) = self.rect.get_mut(i) {
            *r = Rect::new(x, y, w, h);
        }
        if let Some(b) = self.baseline.get_mut(i) {
            *b = m.baseline;
        }
        if let Some(p) = self.present.get_mut(i) {
            *p = st.display != Display::None;
        }
        if st.display == Display::None {
            return;
        }
        let Some(node) = f.session.node(ix) else { return };
        let kind = node.kind;
        let (sx, sy) = node.scroll;
        let inner_w = Constraint::Exact((w - st.inset_h()).max(0.0));
        let inner_h = Constraint::Exact((h - st.inset_v()).max(0.0));

        let placement = match kind {
            NodeKind::Scroll | NodeKind::List => self.place_scroll(f, ix, st, inner_w, inner_h, false),
            NodeKind::Text | NodeKind::Input | NodeKind::TextArea | NodeKind::Image | NodeKind::Icon | NodeKind::Spacer | NodeKind::Divider => {
                Placement::default()
            }
            _ => self.place(f, ix, st, inner_w, inner_h),
        };
        if let Some(slot) = self.content.get_mut(i) {
            *slot = placement.content;
        }

        let (ox, oy) = if matches!(kind, NodeKind::Scroll | NodeKind::List) {
            // Clamp the offset to the content; the session keeps the raw
            // value and the client normalises it after each frame.
            let max_x = (placement.content.w - inner_w.bound().unwrap_or(0.0)).max(0.0);
            let max_y = (placement.content.h - inner_h.bound().unwrap_or(0.0)).max(0.0);
            ((sx as f32).clamp(0.0, max_x), (sy as f32).clamp(0.0, max_y))
        } else {
            (0.0, 0.0)
        };
        let base_x = x + st.border.l + st.padding.l - ox;
        let base_y = y + st.border.t + st.padding.t - oy;

        for p in placement.children {
            if p.virtual_ {
                // Present with a rect so hit-testing and scrollbars are right,
                // but its own subtree is not visited.
                let ci = p.ix.raw() as usize;
                if let Some(r) = self.rect.get_mut(ci) {
                    *r = Rect::new(base_x + p.x, base_y + p.y, p.w, p.h);
                }
                if let Some(pr) = self.present.get_mut(ci) {
                    *pr = true;
                }
                if let Some(v) = self.virtual_.get_mut(ci) {
                    *v = true;
                }
                continue;
            }
            self.arrange(f, p.ix, base_x + p.x, base_y + p.y, p.w, p.h);
        }
    }

    // --------------------------------------------------------------- place

    /// Dispatch on `display`. Absolute children of a non-stack container are
    /// placed as if in a stack over the same content box, after the flow.
    fn place(&mut self, f: &mut Env<'_>, ix: NodeIx, st: Style, inner_w: Constraint, inner_h: Constraint) -> Placement {
        let mut p = match st.display {
            Display::Row | Display::Column => self.place_flow(f, ix, st, inner_w, inner_h, None),
            Display::Stack => return self.place_stack(f, ix, st, inner_w, inner_h, false),
            Display::Grid => self.place_grid(f, ix, st, inner_w, inner_h),
            Display::None => Placement::default(),
        };
        let abs = self.place_stack(f, ix, st, inner_w, inner_h, true);
        p.children.extend(abs.children);
        p
    }

    /// §7: a column with indefinite height (and width when `scroll_both`),
    /// virtualised when the node is a `list` carrying `item_height`.
    /// `measure_only` skips materialising off-screen rows: a measure pass
    /// wants the content size, and only `arrange` places children.
    fn place_scroll(&mut self, f: &mut Env<'_>, ix: NodeIx, st: Style, inner_w: Constraint, inner_h: Constraint, measure_only: bool) -> Placement {
        let col = Style { display: Display::Column, wrap: Wrap::NoWrap, ..st };
        let content_w = if st.scroll_both { Constraint::Unbounded } else { inner_w };
        let is_list = f.session.node(ix).map(|n| n.kind) == Some(NodeKind::List);
        let virt = if is_list {
            self.int_prop(f, ix, self.item_height_atom).filter(|h| *h > 0).map(|h| {
                let sy = f.session.node(ix).map(|n| n.scroll.1).unwrap_or(0) as f32;
                let vh = inner_h.bound().unwrap_or(self.viewport.h);
                (h as f32, sy - vh, sy + 2.0 * vh)
            })
        } else {
            None
        };
        match virt {
            Some(v) => self.place_virtual_list(f, ix, col, content_w, v, measure_only),
            None => self.place_flow(f, ix, col, content_w, Constraint::Unbounded, None),
        }
    }

    /// §7, the virtualised case, done arithmetically: row `i` sits at
    /// `i × (item height + gap)`, so a row outside the window costs nothing
    /// at all — it is not measured, not placed, and has no rect this frame.
    /// It cannot be hit or painted anyway; the content size, which is what
    /// scrollbars need, comes from the count. Running the general flow over
    /// ten thousand such rows was milliseconds of work for numbers this
    /// arithmetic produces for free. `measure_only` is the measure pass,
    /// which wants the content size and places nothing.
    fn place_virtual_list(&mut self, f: &mut Env<'_>, ix: NodeIx, st: Style, inner_w: Constraint, (item_h, start, end): (f32, f32, f32), measure_only: bool) -> Placement {
        self.stats.list_placements = self.stats.list_placements.saturating_add(1);
        let children: &[NodeIx] = f.session.children(ix);
        let n = children.len();
        let width = inner_w.bound().unwrap_or(0.0);
        // §7: a row's height is the list's `item_height` unless the row
        // carries its own. Rows are walked once for their tops — an add per
        // row, no measure — so a feed of cards of two heights still costs
        // nothing off screen.
        let atom = self.item_height_atom;
        let mut tops: Vec<f32> = Vec::with_capacity(n.saturating_add(1));
        let mut y = 0.0f32;
        for &c in children {
            tops.push(y);
            let h = self.int_prop(f, c, atom).filter(|h| *h > 0).map_or(item_h, |h| h as f32);
            y += h + st.gap;
        }
        tops.push(y);
        let content_h = if n == 0 { 0.0 } else { (y - st.gap).max(0.0) };
        let first = tops.partition_point(|t| *t <= start).saturating_sub(1).min(n.saturating_sub(1));
        let last = tops.partition_point(|t| *t < end).saturating_sub(1).min(n.saturating_sub(1));
        let window = if n == 0 { 0 } else { last.saturating_sub(first).saturating_add(1) };
        self.stats.rows_virtual = self.stats.rows_virtual.saturating_add(n.saturating_sub(window) as u32);
        if measure_only || n == 0 {
            return Placement { children: Vec::new(), content: Size::new(width, content_h), baseline: None };
        }
        let mut placed = Vec::with_capacity(window);
        let mut first_baseline = None;
        for (i, &c) in children.iter().enumerate().take(last.saturating_add(1)).skip(first) {
            self.stats.rows_measured = self.stats.rows_measured.saturating_add(1);
            let y = tops.get(i).copied().unwrap_or(0.0);
            let cst = self.style(f, c);
            if cst.display == Display::None {
                continue;
            }
            let m = self.measure(f, c, Constraint::Exact((width - cst.margin.horizontal()).max(0.0)), Constraint::Unbounded);
            if first_baseline.is_none() {
                first_baseline = Some(y + cst.margin.t + m.baseline);
            }
            placed.push(Placed { ix: c, x: cst.margin.l, y: y + cst.margin.t, w: m.w, h: m.h, virtual_: false });
        }
        Placement { children: placed, content: Size::new(width, content_h), baseline: first_baseline }
    }

    /// §4. `virt` is `(item height, window start, window end)` for a
    /// virtualised list.
    fn place_flow(
        &mut self,
        f: &mut Env<'_>,
        ix: NodeIx,
        st: Style,
        inner_w: Constraint,
        inner_h: Constraint,
        virt: Option<(f32, f32, f32)>,
    ) -> Placement {
        let row = st.display == Display::Row;
        let (main_c, cross_c) = if row { (inner_w, inner_h) } else { (inner_h, inner_w) };
        let children: Vec<NodeIx> = f.session.children(ix).to_vec();

        struct Item {
            ix: NodeIx,
            st: Style,
            hyp: f32,
            main: f32,
            cross: f32,
            m_before: f32,
            m_after: f32,
            c_before: f32,
            c_after: f32,
            frozen: bool,
            virtual_: bool,
            baseline: f32,
            /// §4.3 automatic minimum: the content main size, below which a
            /// non-scrolling child with an `auto` main-axis `min` never shrinks.
            auto_min: Option<f32>,
        }
        let mut items: Vec<Item> = Vec::new();
        let mut cursor = 0.0f32; // for virtualisation, main-axis position so far

        // §4.1 hypothetical main sizes.
        for c in children {
            let cst = self.style(f, c);
            if cst.display == Display::None || cst.position == Position::Absolute {
                continue;
            }
            let (m_before, m_after, c_before, c_after) = if row {
                (cst.margin.l, cst.margin.r, cst.margin.t, cst.margin.b)
            } else {
                (cst.margin.t, cst.margin.b, cst.margin.l, cst.margin.r)
            };
            let mut virtual_ = false;
            let hyp = if let Some((item_h, start, end)) = virt {
                let outside = cursor + item_h < start || cursor > end;
                if outside {
                    virtual_ = true;
                    item_h
                } else {
                    self.hyp_main(f, c, cst, main_c, cross_c, row)
                }
            } else {
                self.hyp_main(f, c, cst, main_c, cross_c, row)
            };
            let hyp = if row { cst.clamp_w(hyp, main_c) } else { cst.clamp_h(hyp, main_c) };
            let min_dim = if row { cst.min_width } else { cst.min_height };
            let scrolls = matches!(f.session.node(c).map(|n| n.kind), Some(NodeKind::Scroll | NodeKind::List)) || cst.scroll_both;
            let auto_min = if !virtual_ && cst.shrink > 0.0 && !scrolls && min_dim.resolve(main_c).is_none() {
                // min(content size, specified size), as CSS does: a box given
                // `width: 200` with nothing inside still shrinks. Along a row
                // the content size is the *min-content* width — the longest
                // word, measured as if the width were zero — so a paragraph
                // wraps before it squeezes its siblings; down a column it is
                // the height at the width on offer.
                let main_for_min = if row { Constraint::AtMost(0.0) } else { main_c.loosen().shrink(m_before + m_after) };
                let m = self.measure_axes(f, c, row, main_for_min, cross_c.loosen());
                Some(if row { m.content_w.min(m.w) } else { m.content_h.min(m.h) })
            } else {
                None
            };
            cursor += hyp + m_before + m_after + st.gap;
            items.push(Item {
                ix: c,
                st: cst,
                hyp,
                main: hyp,
                cross: 0.0,
                m_before,
                m_after,
                c_before,
                c_after,
                frozen: false,
                virtual_,
                baseline: 0.0,
                auto_min,
            });
        }

        // §4.2 lines.
        let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
        if st.wrap == Wrap::NoWrap || main_c.bound().is_none() {
            lines = vec![(0..items.len()).collect()];
        } else {
            let bound = main_c.bound().unwrap_or(f32::MAX);
            let mut used = 0.0f32;
            for (i, it) in items.iter().enumerate() {
                let outer = it.hyp + it.m_before + it.m_after;
                let line_len = lines.last().map_or(0, Vec::len);
                let needed = if line_len == 0 { outer } else { used + st.gap + outer };
                if line_len > 0 && needed > bound + 1e-3 {
                    lines.push(vec![i]);
                    used = outer;
                } else {
                    if let Some(l) = lines.last_mut() {
                        l.push(i);
                    }
                    used = needed;
                }
            }
        }
        if st.wrap == Wrap::WrapReverse {
            lines.reverse();
        }

        // §4.3 free space, per line, with a bounded freeze loop.
        let mut main_extent = 0.0f32;
        for line in &lines {
            let gaps = st.gap * line.len().saturating_sub(1) as f32;
            let outer_sum = |items: &Vec<Item>| line.iter().map(|&i| items.get(i).map_or(0.0, |it| it.main + it.m_before + it.m_after)).sum::<f32>();
            if let Some(bound) = main_c.bound() {
                let grow_allowed = matches!(main_c, Constraint::Exact(_));
                for _ in 0..8 {
                    let free = bound - outer_sum(&items) - gaps;
                    let live: Vec<usize> = line.iter().copied().filter(|&i| items.get(i).is_some_and(|it| !it.frozen)).collect();
                    if live.is_empty() {
                        break;
                    }
                    let mut changed = false;
                    if free > 1e-3 && grow_allowed {
                        let total: f32 = live.iter().map(|&i| items.get(i).map_or(0.0, |it| it.st.grow)).sum();
                        if total <= 0.0 {
                            break;
                        }
                        for &i in &live {
                            if let Some(it) = items.get_mut(i) {
                                if it.st.grow > 0.0 {
                                    it.main += free * it.st.grow / total;
                                }
                            }
                        }
                    } else if free < -1e-3 {
                        let total: f32 = live.iter().map(|&i| items.get(i).map_or(0.0, |it| it.st.shrink * it.hyp)).sum();
                        if total <= 0.0 {
                            break;
                        }
                        for &i in &live {
                            if let Some(it) = items.get_mut(i) {
                                if it.st.shrink > 0.0 {
                                    it.main += free * (it.st.shrink * it.hyp) / total;
                                }
                            }
                        }
                    } else {
                        break;
                    }
                    for &i in &live {
                        if let Some(it) = items.get_mut(i) {
                            let clamped = if row { it.st.clamp_w(it.main, main_c) } else { it.st.clamp_h(it.main, main_c) };
                            let clamped = it.auto_min.map_or(clamped, |m| clamped.max(m));
                            if (clamped - it.main).abs() > 1e-3 {
                                it.main = clamped;
                                it.frozen = true;
                                changed = true;
                            }
                        }
                    }
                    if !changed {
                        break;
                    }
                    // Unfrozen items are re-derived from their hypothetical size
                    // on the next pass.
                    for &i in &live {
                        if let Some(it) = items.get_mut(i) {
                            if !it.frozen {
                                it.main = it.hyp;
                            }
                        }
                    }
                }
            }
            main_extent = main_extent.max(outer_sum(&items) + gaps);
        }

        // §4.5 cross sizes and baselines.
        let single_definite = lines.len() == 1 && matches!(cross_c, Constraint::Exact(_));
        let mut line_cross: Vec<f32> = Vec::with_capacity(lines.len());
        let mut line_baseline: Vec<f32> = Vec::with_capacity(lines.len());
        for line in &lines {
            let mut lc = 0.0f32;
            let mut lb = 0.0f32;
            for &i in line {
                let Some(it) = items.get(i) else { continue };
                let (cix, cst, main, virtual_) = (it.ix, it.st, it.main, it.virtual_);
                let cross_dim = if row { cst.height } else { cst.width };
                let (cross, baseline) = if virtual_ {
                    (cross_c.bound().unwrap_or(0.0) - it.c_before - it.c_after, 0.0)
                } else if let Some(v) = cross_dim.resolve(cross_c) {
                    // A specified cross size still obeys the node's own
                    // min/max: `width: 100%` with `max_width: 480` is 480.
                    let v = if row { cst.clamp_h(v, cross_c) } else { cst.clamp_w(v, cross_c) };
                    let m = self.measure_axes(f, cix, row, Constraint::Exact(main), Constraint::Exact(v));
                    (v, m.baseline)
                } else {
                    let m = self.measure_axes(f, cix, row, Constraint::Exact(main), cross_c.loosen().shrink(it.c_before + it.c_after));
                    (if row { m.h } else { m.w }, m.baseline)
                };
                if let Some(it) = items.get_mut(i) {
                    it.cross = cross;
                    it.baseline = baseline;
                    lc = lc.max(cross + it.c_before + it.c_after);
                    if row && self.align_of(st, cst) == AlignItems::Baseline {
                        lb = lb.max(baseline + it.c_before);
                    }
                }
            }
            if single_definite {
                lc = cross_c.bound().unwrap_or(lc);
            }
            line_cross.push(lc);
            line_baseline.push(lb);
        }
        let cross_extent: f32 = line_cross.iter().sum::<f32>() + st.gap * lines.len().saturating_sub(1) as f32;

        // §4.4 positions.
        let mut placed = Vec::with_capacity(items.len());
        let mut first_baseline = None;
        let mut cross_pos = 0.0f32;
        let main_bound = main_c.bound().unwrap_or(main_extent);
        for (li, line) in lines.iter().enumerate() {
            let lc = line_cross.get(li).copied().unwrap_or(0.0);
            let lb = line_baseline.get(li).copied().unwrap_or(0.0);
            let n = line.len();
            let used: f32 = line.iter().map(|&i| items.get(i).map_or(0.0, |it| it.main + it.m_before + it.m_after)).sum::<f32>()
                + st.gap * n.saturating_sub(1) as f32;
            let free = if matches!(main_c, Constraint::Exact(_)) { (main_bound - used).max(0.0) } else { 0.0 };
            let (mut pos, between) = match (st.justify, n) {
                (Justify::Start, _) | (Justify::Between, 1) => (0.0, 0.0),
                (Justify::Center, _) | (Justify::Around, 1) | (Justify::Evenly, 1) => (free / 2.0, 0.0),
                (Justify::End, _) => (free, 0.0),
                (Justify::Between, _) => (0.0, free / n.saturating_sub(1) as f32),
                (Justify::Around, _) => (free / n as f32 / 2.0, free / n as f32),
                (Justify::Evenly, _) => (free / n.saturating_add(1) as f32, free / n.saturating_add(1) as f32),
            };
            for &i in line {
                let Some(it) = items.get(i) else { continue };
                let align = self.align_of(st, it.st);
                let cross_dim_auto = matches!(if row { it.st.height } else { it.st.width }, Length::Auto);
                let avail = lc - it.c_before - it.c_after;
                // Stretch fills the line but still honours the child's own
                // min/max on that axis.
                let stretched = if row { it.st.clamp_h(avail.max(0.0), cross_c) } else { it.st.clamp_w(avail.max(0.0), cross_c) };
                let (cross_off, cross_size) = match align {
                    AlignItems::Stretch if cross_dim_auto => (it.c_before, stretched),
                    AlignItems::Start | AlignItems::Stretch => (it.c_before, it.cross),
                    AlignItems::Center => (it.c_before + (avail - it.cross) / 2.0, it.cross),
                    AlignItems::End => (it.c_before + avail - it.cross, it.cross),
                    AlignItems::Baseline if row => (lb - it.baseline, it.cross),
                    AlignItems::Baseline => (it.c_before, it.cross),
                };
                let main_pos = pos + it.m_before;
                let (x, y, w, h) = if row {
                    (main_pos, cross_pos + cross_off, it.main, cross_size)
                } else {
                    (cross_pos + cross_off, main_pos, cross_size, it.main)
                };
                if first_baseline.is_none() && !it.virtual_ {
                    first_baseline = Some(y + it.baseline);
                }
                placed.push(Placed { ix: it.ix, x, y, w, h, virtual_: it.virtual_ });
                pos += it.m_before + it.main + it.m_after + st.gap + between;
            }
            cross_pos += lc + st.gap;
        }

        let content = if row { Size::new(main_extent, cross_extent) } else { Size::new(cross_extent, main_extent) };
        Placement { children: placed, content, baseline: first_baseline }
    }

    fn align_of(&self, container: Style, child: Style) -> AlignItems {
        match child.align_self {
            AlignSelf::Auto => container.align_items,
            AlignSelf::Start => AlignItems::Start,
            AlignSelf::Center => AlignItems::Center,
            AlignSelf::End => AlignItems::End,
            AlignSelf::Stretch => AlignItems::Stretch,
            AlignSelf::Baseline => AlignItems::Baseline,
        }
    }

    /// Measure with `(main, cross)` constraints expressed for a `row` or a
    /// `column` parent.
    fn measure_axes(&mut self, f: &mut Env<'_>, ix: NodeIx, row: bool, main: Constraint, cross: Constraint) -> Metrics {
        if row { self.measure(f, ix, main, cross) } else { self.measure(f, ix, cross, main) }
    }

    fn hyp_main(&mut self, f: &mut Env<'_>, c: NodeIx, cst: Style, main_c: Constraint, cross_c: Constraint, row: bool) -> f32 {
        if let Some(b) = cst.basis.resolve(main_c) {
            return b;
        }
        let main_dim = if row { cst.width } else { cst.height };
        if let Some(v) = main_dim.resolve(main_c) {
            return v;
        }
        let margins = if row { cst.margin.horizontal() } else { cst.margin.vertical() };
        let m = self.measure_axes(f, c, row, main_c.loosen().shrink(margins), cross_c.loosen());
        if row { m.w } else { m.h }
    }

    /// §5, and the absolute children of any container when `absolute_only`.
    fn place_stack(&mut self, f: &mut Env<'_>, ix: NodeIx, st: Style, inner_w: Constraint, inner_h: Constraint, absolute_only: bool) -> Placement {
        let children: Vec<NodeIx> = f.session.children(ix).to_vec();
        let mut placed = Vec::new();
        let mut extent = Size::default();
        let mut first_baseline = None;
        for c in children {
            let cst = self.style(f, c);
            if cst.display == Display::None || (absolute_only && cst.position != Position::Absolute) {
                continue;
            }
            let avail_w = inner_w.loosen().shrink(cst.margin.horizontal());
            let avail_h = inner_h.loosen().shrink(cst.margin.vertical());
            let align = self.align_of(st, cst);
            let cw = match (align, cst.width, inner_w) {
                _ if cst.width.resolve(inner_w).is_some() => Constraint::Exact(cst.width.resolve(inner_w).unwrap_or(0.0)),
                (AlignItems::Stretch, Length::Auto, Constraint::Exact(b)) if !absolute_only => Constraint::Exact((b - cst.margin.horizontal()).max(0.0)),
                _ => avail_w,
            };
            let ch = match (align, cst.height, inner_h) {
                _ if cst.height.resolve(inner_h).is_some() => Constraint::Exact(cst.height.resolve(inner_h).unwrap_or(0.0)),
                (AlignItems::Stretch, Length::Auto, Constraint::Exact(b)) if !absolute_only => Constraint::Exact((b - cst.margin.vertical()).max(0.0)),
                _ => avail_h,
            };
            let m = self.measure(f, c, cw, ch);
            let bw = inner_w.bound().unwrap_or(m.w + cst.margin.horizontal());
            let bh = inner_h.bound().unwrap_or(m.h + cst.margin.vertical());
            let x = match st.justify {
                Justify::Center | Justify::Around | Justify::Evenly => cst.margin.l + (bw - cst.margin.horizontal() - m.w) / 2.0,
                Justify::End => bw - cst.margin.r - m.w,
                Justify::Start | Justify::Between => cst.margin.l,
            };
            let y = match align {
                AlignItems::Center => cst.margin.t + (bh - cst.margin.vertical() - m.h) / 2.0,
                AlignItems::End => bh - cst.margin.b - m.h,
                AlignItems::Start | AlignItems::Stretch | AlignItems::Baseline => cst.margin.t,
            };
            if first_baseline.is_none() {
                first_baseline = Some(y + m.baseline);
            }
            extent.w = extent.w.max(m.w + cst.margin.horizontal());
            extent.h = extent.h.max(m.h + cst.margin.vertical());
            placed.push(Placed { ix: c, x, y, w: m.w, h: m.h, virtual_: false });
        }
        // Paint order is ascending z; `place` returns children in that order.
        let z_of: HashMap<u32, u8> = placed.iter().map(|p| (p.ix.raw(), self.style_of(f.session, p.ix).map_or(0, |s| s.z))).collect();
        placed.sort_by_key(|p| z_of.get(&p.ix.raw()).copied().unwrap_or(0));
        Placement { children: placed, content: extent, baseline: first_baseline }
    }

    /// §6: `N` equal columns, row-major.
    fn place_grid(&mut self, f: &mut Env<'_>, ix: NodeIx, st: Style, inner_w: Constraint, inner_h: Constraint) -> Placement {
        let n = self.int_prop(f, ix, self.columns_atom).filter(|n| *n >= 1).map_or(1usize, |n| n.min(4096) as usize);
        let children: Vec<NodeIx> = f
            .session
            .children(ix)
            .iter()
            .copied()
            .filter(|c| {
                let s = self.style(f, *c);
                s.display != Display::None && s.position != Position::Absolute
            })
            .collect();
        let gaps = st.gap * n.saturating_sub(1) as f32;
        let col_w = inner_w.bound().map(|b| ((b - gaps) / n as f32).max(0.0));
        let _ = inner_h;

        let mut placed = Vec::with_capacity(children.len());
        let mut y = 0.0f32;
        let mut first_baseline = None;
        let mut max_w = 0.0f32;
        for row_nodes in children.chunks(n) {
            // Measure the row at column width, find its height.
            let mut metrics = Vec::with_capacity(row_nodes.len());
            let mut row_h = 0.0f32;
            for c in row_nodes {
                let cst = self.style(f, *c);
                let cw = match col_w {
                    Some(w) => Constraint::Exact((w - cst.margin.horizontal()).max(0.0)),
                    None => Constraint::Unbounded,
                };
                let m = self.measure(f, *c, cw, Constraint::Unbounded);
                row_h = row_h.max(m.h + cst.margin.vertical());
                metrics.push((cst, m));
            }
            let cell_w = col_w.unwrap_or_else(|| metrics.iter().map(|(cst, m)| m.w + cst.margin.horizontal()).fold(0.0, f32::max));
            for (i, (c, (cst, m))) in row_nodes.iter().zip(metrics.iter()).enumerate() {
                let x = i as f32 * (cell_w + st.gap) + cst.margin.l;
                let h = if matches!(cst.height, Length::Auto) { row_h - cst.margin.vertical() } else { m.h };
                let w = if matches!(cst.width, Length::Auto) { cell_w - cst.margin.horizontal() } else { m.w };
                if first_baseline.is_none() {
                    first_baseline = Some(y + cst.margin.t + m.baseline);
                }
                placed.push(Placed { ix: *c, x, y: y + cst.margin.t, w, h, virtual_: false });
            }
            max_w = max_w.max(n as f32 * cell_w + gaps);
            y += row_h + st.gap;
        }
        let content_h = (y - st.gap).max(0.0);
        Placement { children: placed, content: Size::new(max_w, content_h), baseline: first_baseline }
    }
}
