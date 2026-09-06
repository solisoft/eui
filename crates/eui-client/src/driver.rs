//! The session driver: everything the client does that is not a window or a
//! socket. Frames in, frames out; input in, events out; a draw list when
//! asked. Pure enough to be tested without a display or a network.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use eui_layout::{Env, FontSpec, Layout, Size, TextMeasurer, TextMetrics};
use eui_proto::{
    caps, Batch, EventFrame, EventKind, Frame, Handler, Hello, NodeKind, ThemeMode, Value, Viewport, PROTOCOL_VERSION,
};
use eui_render::{colors_of, Colors, paint, Atlas, DrawList, ImageAtlas, Scene};

use crate::assets::{AssetStore, Hash};
use eui_text::TextEngine;
use eui_theme::{Resolved, Theme, Viewer};
use eui_tree::{Chunk, NodeIx, Session};

/// What the window feeds the driver.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// Pointer moved to logical `(x, y)`.
    PointerMove(f32, f32),
    /// A button went down: `0` primary, `1` secondary, `2` middle.
    PointerDown(u8),
    /// A button came up.
    PointerUp(u8),
    /// Wheel or trackpad, logical px.
    Wheel(f32, f32),
    /// Committed text.
    Text(String),
    /// An input method's composition in progress: shown in the focused
    /// field, never reported. An empty string ends the composition.
    ImePreedit(String),
    /// An input method committed `text`: the composition ends and the text
    /// is inserted as if typed.
    ImeCommit(String),
    /// A named key, with modifiers, pressed or released.
    Key {
        /// W3C `KeyboardEvent.key` name.
        key: String,
        /// `1` shift, `2` control, `4` alt, `8` super.
        modifiers: u32,
        /// `true` on press.
        down: bool,
    },
    /// The window is now `w × h` logical px at `scale` device px per logical.
    Resized(f32, f32, f32),
    /// The viewer changed palette.
    Mode(ThemeMode),
    /// The window lost focus.
    Unfocused,
}

/// Why the driver wants the session closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Close {
    /// The server sent an `Error` frame.
    ServerError(u32, String),
    /// The server's protocol version is unusable.
    Version(u32),
    /// A frame arrived that the protocol does not allow in this direction.
    Protocol(&'static str),
}

#[derive(Debug, Default)]
struct Pointer {
    x: f32,
    y: f32,
    over: Option<NodeIx>,
    pressed_on: Option<NodeIx>,
}

/// A field's local edit: the value the server last saw (`seed`) and the
/// value typed since. `change` fires only when they differ.
#[derive(Debug, Clone)]
struct Edit {
    seed: String,
    value: String,
}

/// One running transition: the colours it left, the colours it reaches,
/// and when.
#[derive(Debug, Clone, Copy)]
struct Anim {
    from: Colors,
    to: Colors,
    start: Instant,
    duration: Duration,
}

impl Anim {
    fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.duration
    }

    /// The colours at `now`, eased; exactly `to` once the time is up.
    fn at(&self, now: Instant) -> Colors {
        if self.done(now) {
            return self.to;
        }
        let t = now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32().max(1e-3);
        let k = eui_theme::scale::ease(t);
        Colors {
            bg: mix(self.from.bg, self.to.bg, k),
            fg: mix(self.from.fg, self.to.fg, k),
            border: mix(self.from.border, self.to.border, k),
            opacity: self.from.opacity + (self.to.opacity - self.from.opacity) * k,
        }
    }
}

/// Blend two optional colours; an absent side fades through transparent.
fn mix(a: Option<[f32; 4]>, b: Option<[f32; 4]>, k: f32) -> Option<[f32; 4]> {
    match (a, b) {
        (None, None) => None,
        (Some(a), Some(b)) => Some([a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k, a[3] + (b[3] - a[3]) * k]),
        (None, Some(b)) => Some([b[0], b[1], b[2], b[3] * k]),
        (Some(a), None) => Some([a[0], a[1], a[2], a[3] * (1.0 - k)]),
    }
}

/// The driver.
pub struct Driver {
    session: Session,
    layout: Layout,
    theme: Theme,
    resolved: Resolved,
    viewer: Viewer,
    text: TextEngine,
    atlas: Atlas,
    images: ImageAtlas,
    assets: AssetStore,
    size: Size,
    scale: f32,
    pointer: Pointer,
    focused: Option<NodeIx>,
    /// Focus came from the keyboard or the server: draw the ring (spec 03 §3).
    focus_visible: bool,
    /// Running transitions (spec 03 §5), the clock they run on, and when the
    /// next frame is due — the only reason the window ever wakes itself.
    anims: Vec<(NodeIx, Anim)>,
    now: Instant,
    next_due: Option<Instant>,
    edits: HashMap<u32, Edit>,
    /// The composition an input method is building in the focused field.
    preedit: String,
    /// Verified chunks by id; verification happens once per chunk.
    chunks: HashMap<u32, Option<eui_vm::Chunk>>,
    granted: u32,
    welcomed: bool,
    layout_valid: bool,
    redraw: bool,
    closed: Option<Close>,
}

impl std::fmt::Debug for Driver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Driver").field("size", &self.size).field("welcomed", &self.welcomed).field("closed", &self.closed).finish()
    }
}

impl Driver {
    /// A driver for a window of `w × h` logical px at `scale`, granting
    /// `granted` capabilities to the application.
    pub fn new(w: f32, h: f32, scale: f32, granted: u32) -> Self {
        let theme = Theme::default();
        let viewer = Viewer::default();
        let resolved = theme.resolve(viewer);
        Self {
            session: Session::new(),
            layout: Layout::new(),
            theme,
            resolved,
            viewer,
            text: TextEngine::new(),
            atlas: Atlas::new(),
            images: ImageAtlas::new(),
            assets: AssetStore::default(),
            size: Size::new(w, h),
            scale,
            pointer: Pointer::default(),
            focused: None,
            focus_visible: false,
            anims: Vec::new(),
            now: Instant::now(),
            next_due: None,
            edits: HashMap::new(),
            preedit: String::new(),
            chunks: HashMap::new(),
            granted: granted & caps::ALL,
            welcomed: false,
            layout_valid: false,
            redraw: true,
            closed: None,
        }
    }

    /// The opening frame.
    pub fn hello(&self) -> Frame {
        Frame::Hello(Hello { version: PROTOCOL_VERSION, viewport: self.viewport(), granted: self.granted })
    }

    fn viewport(&self) -> Viewport {
        Viewport {
            width: self.size.w.max(0.0) as u32,
            height: self.size.h.max(0.0) as u32,
            scale: (self.scale * 100.0).round() as u16,
            mode: self.viewer.mode,
            density: self.viewer.density,
            font_scale: (self.viewer.font_scale * 100.0).round() as u16,
        }
    }

    /// The tree.
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The last layout, valid after [`Self::paint`].
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    /// True when a frame should be drawn.
    pub fn needs_redraw(&self) -> bool {
        self.redraw
    }

    /// Why the session ended, if it did.
    pub fn closed(&self) -> Option<&Close> {
        self.closed.as_ref()
    }

    // -------------------------------------------------------------- frames

    /// Apply a frame from the server; returns frames to send back.
    pub fn handle_frame(&mut self, frame: Frame) -> Vec<Frame> {
        match frame {
            Frame::Welcome(w) => {
                if w.version == 0 || w.version > PROTOCOL_VERSION {
                    self.closed = Some(Close::Version(w.version));
                    return vec![Frame::Error { code: 100, message: format!("unsupported version {}", w.version) }];
                }
                self.welcomed = true;
                Vec::new()
            }
            Frame::Batch(batch) => self.apply(&batch),
            Frame::Ping(n) => vec![Frame::Pong(n)],
            Frame::Pong(_) => Vec::new(),
            Frame::Error { code, message } => {
                self.closed = Some(Close::ServerError(code, message));
                Vec::new()
            }
            Frame::Hello(_) | Frame::Event(_) | Frame::Ack { .. } | Frame::Resync | Frame::Viewport(_) => {
                self.closed = Some(Close::Protocol("client-only frame from server"));
                vec![Frame::Error { code: 101, message: "client-only frame from server".into() }]
            }
        }
    }

    fn apply(&mut self, batch: &Batch) -> Vec<Frame> {
        match self.session.apply(batch) {
            Ok(()) => {
                self.invalidate();
                self.note_style_changes();
                // Focus and edits follow the tree.
                if self.focused.is_some_and(|f| self.session.node(f).is_none()) {
                    self.focused = None;
                }
                self.edits.retain(|id, _| self.session.lookup(*id).is_some());
                let mut out = vec![Frame::Ack { seq: batch.seq }];
                // A `Focus` op focuses the way the keyboard does, ring included.
                if batch.ops.iter().any(|o| matches!(o, eui_proto::Op::Focus { .. })) {
                    if let Some(ix) = self.session.focused() {
                        out.extend(self.set_focus(Some(ix), true));
                    }
                }
                out
            }
            Err(e) => {
                // Recoverable by design: discard, ask for a fresh tree, rebuild.
                // Not an `Error` frame — that would end the session on both
                // sides, which is the opposite of what a resync is for.
                eprintln!("eui: batch {} rejected ({e}); resyncing", batch.seq);
                self.invalidate();
                vec![Frame::Resync]
            }
        }
    }

    fn invalidate(&mut self) {
        self.layout_valid = false;
        self.redraw = true;
    }

    // ---------------------------------------------------------- transitions

    /// Spec 03 §5: every style change whose new record asks for a transition
    /// becomes an animation from the old record's colours — or, if the node
    /// was already mid-transition, from wherever it visibly is.
    fn note_style_changes(&mut self) {
        for (ix, old) in self.session.take_style_changes() {
            if self.session.node(ix).is_none() {
                continue;
            }
            let new = self.session.style_of(ix);
            let Some(ms) = new.transition.checked_sub(1).and_then(|i| self.resolved.motion.get(usize::from(i))) else { continue };
            let to = colors_of(&self.session, &self.resolved, &new);
            let from = match self.anims.iter().position(|(n, _)| *n == ix) {
                Some(i) => self.anims.remove(i).1.at(self.now),
                None => self.session.style(old).map_or(to, |r| colors_of(&self.session, &self.resolved, r)),
            };
            self.anims.push((ix, Anim { from, to, start: self.now, duration: Duration::from_millis(u64::from(*ms)) }));
            self.next_due = Some(self.now);
            self.redraw = true;
        }
    }

    /// Advance the clock. True when a transition frame is due, so the window
    /// should redraw; false at rest, which is almost always.
    pub fn tick(&mut self, now: Instant) -> bool {
        self.now = now;
        match self.next_due {
            Some(due) if now >= due => {
                self.redraw = true;
                true
            }
            _ => false,
        }
    }

    /// When the next transition frame is due — `None` at rest. The window
    /// sleeps until then and not a moment less.
    pub fn next_frame_at(&self) -> Option<Instant> {
        self.next_due
    }

    /// True while any transition runs.
    pub fn animating(&self) -> bool {
        !self.anims.is_empty()
    }

    /// A role's colour under the viewer's current theme, `0xRRGGBBAA`.
    pub fn theme_color(&self, role: eui_theme::Role) -> u32 {
        self.resolved.color(role)
    }

    // --------------------------------------------------------------- input

    /// Feed input; returns event frames to send.
    pub fn input(&mut self, input: Input) -> Vec<Frame> {
        match input {
            Input::Resized(w, h, scale) => {
                self.size = Size::new(w, h);
                self.scale = scale;
                self.invalidate();
                vec![Frame::Viewport(self.viewport())]
            }
            Input::Mode(mode) => {
                self.viewer.mode = mode;
                self.resolved = self.theme.resolve(self.viewer);
                self.invalidate();
                vec![Frame::Viewport(self.viewport())]
            }
            Input::PointerMove(x, y) => self.pointer_move(x, y),
            Input::PointerDown(button) => self.pointer_down(button),
            Input::PointerUp(button) => self.pointer_up(button),
            Input::Wheel(dx, dy) => self.wheel(dx, dy),
            Input::Text(t) => self.text_input(&t),
            Input::ImePreedit(t) => {
                self.preedit(t);
                Vec::new()
            }
            Input::ImeCommit(t) => {
                self.preedit(String::new());
                self.text_input(&t)
            }
            Input::Key { key, modifiers, down } => self.key(&key, modifiers, down),
            Input::Unfocused => {
                let out = self.set_focus(None, false);
                self.pointer.pressed_on = None;
                out
            }
        }
    }

    fn ensure_layout(&mut self) {
        if !self.layout_valid {
            let mut measurer = Measurer { text: &mut self.text, assets: &self.assets };
            self.layout.compute(&mut Env { session: &self.session, theme: &self.resolved, text: &mut measurer }, self.size);
            self.layout_valid = true;
        }
    }

    // -------------------------------------------------------------- assets

    /// Hashes the tree needs and the client has not fetched: images'
    /// `src` props and chunks defined by hash. The caller fetches them from
    /// the session's origin and calls [`Self::asset_ready`].
    pub fn pending_assets(&mut self) -> Vec<Hash> {
        if let Some(root) = self.session.root() {
            let wanted: Vec<Hash> = self
                .session
                .preorder(root)
                .filter_map(|ix| self.session.node(ix))
                .filter(|n| n.kind == NodeKind::Image)
                .flat_map(|n| n.props.iter().filter_map(|(_, v)| if let Value::Asset(h) = v { Some(*h) } else { None }))
                .collect();
            for h in wanted {
                self.assets.want(h);
            }
        }
        self.assets.take_pending()
    }

    /// Deliver verified bytes for a hash. Images are decoded and packed for
    /// the renderer; the tree is relaid out because an image now has a size.
    pub fn asset_ready(&mut self, hash: Hash, bytes: Vec<u8>) {
        self.assets.deliver(hash, bytes);
        if let Some(img) = self.assets.image(&hash) {
            self.images.insert(hash, img.width, img.height, &img.rgba);
        }
        self.invalidate();
    }

    /// Record that a hash could not be fetched.
    pub fn asset_failed(&mut self, hash: Hash, why: String) {
        eprintln!("eui: asset {}: {why}", crate::assets::hex(&hash));
        self.assets.fail(hash, why);
    }

    /// The asset store.
    pub fn assets(&self) -> &AssetStore {
        &self.assets
    }

    /// The nearest node at or above `from` carrying a handler for `kind`.
    fn target(&self, from: NodeIx, kind: EventKind) -> Option<(NodeIx, Handler)> {
        let mut cur = Some(from);
        while let Some(ix) = cur {
            let node = self.session.node(ix)?;
            if let Some(h) = node.handler(kind) {
                return Some((ix, h));
            }
            cur = if node.parent.is_some() { Some(node.parent) } else { None };
        }
        None
    }

    /// Emit `kind` for the nearest handler at or above `from`, per spec 06 §2
    /// and spec 07 §6: a local chunk runs first and may queue events; a
    /// `LocalThenServer` then sends its named event; an aborted chunk sends
    /// nothing.
    fn emit(&mut self, from: NodeIx, kind: EventKind, payload: Value) -> Vec<Frame> {
        let Some((target, handler)) = self.target(from, kind) else { return Vec::new() };
        let node = self.session.node(target).map(|n| n.id).unwrap_or(0);
        let mut out = Vec::new();
        let name = match handler {
            Handler::Server(name) => Some(name),
            Handler::Local(chunk) => {
                match self.run_local(chunk) {
                    Ok(queued) => {
                        let state = self.root_state();
                        out.extend(queued.into_iter().map(|n| Frame::Event(EventFrame { node, event: kind, name: n, payload: state.clone() })));
                    }
                    Err(e) => eprintln!("eui: local handler {chunk}: {e}"),
                }
                None
            }
            Handler::LocalThenServer { chunk, name } => match self.run_local(chunk) {
                Ok(queued) => {
                    let state = self.root_state();
                    out.extend(queued.into_iter().map(|n| Frame::Event(EventFrame { node, event: kind, name: n, payload: state.clone() })));
                    Some(name)
                }
                Err(e) => {
                    eprintln!("eui: local handler {chunk}: {e}; sending nothing");
                    None
                }
            },
        };
        if let Some(name) = name {
            out.push(Frame::Event(EventFrame { node, event: kind, name, payload }));
        }
        out
    }

    /// The root props as an event payload: the local state, for the server
    /// to compare against its own.
    fn root_state(&self) -> Value {
        let Some(root) = self.session.root() else { return Value::Null };
        let Some(n) = self.session.node(root) else { return Value::Null };
        Value::List(n.props.iter().flat_map(|(a, v)| [Value::Atom(*a), v.clone()]).collect())
    }

    /// Verify (once) and run a chunk against the session. Returns the atoms
    /// the chunk asked to emit, in order.
    fn run_local(&mut self, chunk_id: u32) -> Result<Vec<u32>, String> {
        let verified = match self.chunks.get(&chunk_id) {
            Some(Some(c)) => c.clone(),
            Some(None) => return Err("chunk failed verification earlier".into()),
            None => {
                let result = match self.session.chunk(chunk_id) {
                    Some(Chunk::Bytes(bytes)) => eui_vm::Chunk::verify(bytes).map_err(|e| e.to_string()),
                    Some(Chunk::Hash(h)) => match self.assets.raw(h) {
                        Some(bytes) => eui_vm::Chunk::verify(&bytes).map_err(|e| e.to_string()),
                        None => {
                            // Ask for it; until it arrives the handler is inert.
                            let h = *h;
                            self.assets.want(h);
                            return Err("chunk not fetched yet".into());
                        }
                    },
                    None => Err("undefined chunk".into()),
                };
                match result {
                    Ok(c) => {
                        self.chunks.insert(chunk_id, Some(c.clone()));
                        c
                    }
                    Err(e) => {
                        if e != "chunk not fetched yet" {
                            self.chunks.insert(chunk_id, None);
                        }
                        return Err(e);
                    }
                }
            }
        };
        let mut host = SessionHost { session: &mut self.session, emitted: Vec::new(), touched: false };
        let result = eui_vm::run(&verified, &mut host);
        let touched = host.touched;
        let emitted = host.emitted;
        if touched {
            self.invalidate();
        }
        result.map_err(|e| e.to_string())?;
        Ok(emitted)
    }

    /// Spec 06 §1: a pointer payload is local to the node the event is
    /// reported for — the one holding the handler, not the leaf under the
    /// pointer. A click on a slider's fill bar is measured from the slider.
    fn local_point(&self, from: NodeIx, kind: EventKind, x: f32, y: f32) -> (f32, f32) {
        let ix = self.target(from, kind).map_or(from, |t| t.0);
        let r = self.layout.rect(ix).unwrap_or_default();
        (x - r.x, y - r.y)
    }

    fn point_payload(&self, from: NodeIx, kind: EventKind, x: f32, y: f32) -> Value {
        let (lx, ly) = self.local_point(from, kind, x, y);
        Value::List(vec![Value::Float(f64::from(lx)), Value::Float(f64::from(ly))])
    }

    fn button_payload(&self, from: NodeIx, kind: EventKind, x: f32, y: f32, button: u8) -> Value {
        let (lx, ly) = self.local_point(from, kind, x, y);
        Value::List(vec![Value::Float(f64::from(lx)), Value::Float(f64::from(ly)), Value::Int(i64::from(button))])
    }

    fn pointer_move(&mut self, x: f32, y: f32) -> Vec<Frame> {
        self.ensure_layout();
        self.pointer.x = x;
        self.pointer.y = y;
        let now = self.layout.hit(&self.session, x, y);
        let mut out = Vec::new();
        if now != self.pointer.over {
            if let Some(old) = self.pointer.over {
                if self.session.node(old).is_some() {
                    out.extend(self.emit(old, EventKind::PointerLeave, Value::Null));
                }
            }
            if let Some(new) = now {
                out.extend(self.emit(new, EventKind::PointerEnter, Value::Null));
            }
            self.pointer.over = now;
        }
        if let Some(ix) = now {
            let p = self.point_payload(ix, EventKind::PointerMove, x, y);
            out.extend(self.emit(ix, EventKind::PointerMove, p));
        }
        out
    }

    fn pointer_down(&mut self, button: u8) -> Vec<Frame> {
        self.ensure_layout();
        let (x, y) = (self.pointer.x, self.pointer.y);
        let Some(ix) = self.layout.hit(&self.session, x, y) else { return Vec::new() };
        self.pointer.pressed_on = Some(ix);
        // Focus moves to the nearest editable node on the path, or nowhere;
        // a pointer never shows the ring.
        let editable = self.ancestor_where(ix, |k| matches!(k, NodeKind::Input | NodeKind::TextArea));
        let mut out = self.set_focus(editable, false);
        let payload = self.button_payload(ix, EventKind::PointerDown, x, y, button);
        out.extend(self.emit(ix, EventKind::PointerDown, payload));
        out
    }

    fn pointer_up(&mut self, button: u8) -> Vec<Frame> {
        self.ensure_layout();
        let (x, y) = (self.pointer.x, self.pointer.y);
        let mut out = Vec::new();
        let Some(ix) = self.layout.hit(&self.session, x, y) else {
            self.pointer.pressed_on = None;
            return out;
        };
        let payload = self.button_payload(ix, EventKind::PointerUp, x, y, button);
        out.extend(self.emit(ix, EventKind::PointerUp, payload));
        // A click is a press and a release that resolve to the same handler.
        if let Some(pressed) = self.pointer.pressed_on.take() {
            let same = self.target(pressed, EventKind::Click).map(|t| t.0) == self.target(ix, EventKind::Click).map(|t| t.0);
            if same {
                let kind = if button == 1 { EventKind::ContextMenu } else { EventKind::Click };
                let p = self.point_payload(ix, kind, x, y);
                out.extend(self.emit(ix, kind, p));
            }
        }
        out
    }

    /// Move focus to `new`, blurring (and committing) the old node. `visible`
    /// says whether the ring is drawn: keyboard and server yes, pointer no.
    fn set_focus(&mut self, new: Option<NodeIx>, visible: bool) -> Vec<Frame> {
        let mut out = Vec::new();
        if new != self.focused {
            if let Some(old) = self.focused.take() {
                self.preedit.clear();
                self.show_edit(old);
                out.extend(self.commit_edit(old));
                out.extend(self.emit(old, EventKind::Blur, Value::Null));
            }
            if let Some(n) = new {
                self.focused = Some(n);
                out.extend(self.emit(n, EventKind::Focus, Value::Null));
            }
        }
        if self.focus_visible != (visible && new.is_some()) || new != self.focused {
            self.redraw = true;
        }
        self.focus_visible = visible && new.is_some();
        out
    }

    /// Spec 03 §3: editable nodes and nodes with their own `click` handler,
    /// in document order, skipping what is not laid out.
    fn focus_order(&self) -> Vec<NodeIx> {
        let mut order = Vec::new();
        let Some(root) = self.session.root() else { return order };
        let mut stack = vec![root];
        while let Some(ix) = stack.pop() {
            let Some(node) = self.session.node(ix) else { continue };
            let focusable = matches!(node.kind, NodeKind::Input | NodeKind::TextArea) || node.handler(EventKind::Click).is_some();
            if focusable && self.layout.rect(ix).is_some() && !self.layout.is_virtual(ix) {
                order.push(ix);
            }
            stack.extend(self.session.children(ix).iter().rev());
        }
        order
    }

    /// `Tab` / `Shift+Tab`: the next or previous focusable node, wrapping.
    fn move_focus(&mut self, backwards: bool) -> Vec<Frame> {
        self.ensure_layout();
        let order = self.focus_order();
        if order.is_empty() {
            return self.set_focus(None, false);
        }
        let at = self.focused.and_then(|f| order.iter().position(|&o| o == f));
        let next = match (at, backwards) {
            (None, false) => 0,
            (None, true) => order.len().saturating_sub(1),
            (Some(i), false) => (i.saturating_add(1)) % order.len(),
            (Some(i), true) => i.checked_sub(1).unwrap_or(order.len().saturating_sub(1)),
        };
        let target = order.get(next).copied();
        self.set_focus(target, true)
    }

    /// `Enter` / `Space` on a focused activatable node: a click at its centre.
    fn activate(&mut self, f: NodeIx) -> Vec<Frame> {
        let r = self.layout.rect(f).unwrap_or_default();
        let p = Value::List(vec![Value::Float(f64::from(r.w / 2.0)), Value::Float(f64::from(r.h / 2.0))]);
        self.emit(f, EventKind::Click, p)
    }

    fn ancestor_where(&self, from: NodeIx, pred: impl Fn(NodeKind) -> bool) -> Option<NodeIx> {
        let mut cur = Some(from);
        while let Some(ix) = cur {
            let node = self.session.node(ix)?;
            if pred(node.kind) {
                return Some(ix);
            }
            cur = if node.parent.is_some() { Some(node.parent) } else { None };
        }
        None
    }

    fn wheel(&mut self, dx: f32, dy: f32) -> Vec<Frame> {
        self.ensure_layout();
        let Some(hit) = self.layout.hit(&self.session, self.pointer.x, self.pointer.y) else { return Vec::new() };
        let Some(scroller) = self.ancestor_where(hit, |k| matches!(k, NodeKind::Scroll | NodeKind::List)) else { return Vec::new() };
        let (sx, sy) = self.session.node(scroller).map(|n| n.scroll).unwrap_or((0, 0));
        let content = self.layout.content_size(scroller).unwrap_or_default();
        let view = self.layout.rect(scroller).unwrap_or_default();
        let max_x = (content.w - view.w).max(0.0) as i64;
        let max_y = (content.h - view.h).max(0.0) as i64;
        // A virtualised list's content height drifts a little as different
        // rows get measured, so the stored offset can sit past the current
        // bound. Normalise before applying the delta: "already at the end"
        // must stay a no-op rather than an event.
        let (sx, sy) = (sx.clamp(0, max_x), sy.clamp(0, max_y));
        let nx = (sx + dx as i64).clamp(0, max_x);
        let ny = (sy + dy as i64).clamp(0, max_y);
        if (nx, ny) == (sx, sy) {
            return Vec::new();
        }
        self.session.set_scroll(scroller, nx, ny);
        self.invalidate();
        self.emit(scroller, EventKind::Scroll, Value::List(vec![Value::Int(nx), Value::Int(ny)]))
    }

    fn is_editable(&self, ix: NodeIx) -> bool {
        matches!(self.session.node(ix).map(|n| n.kind), Some(NodeKind::Input | NodeKind::TextArea))
    }

    /// The edit buffer of an editable node, seeded from its text on first use.
    fn edit_buf(&mut self, f: NodeIx) -> Option<&mut String> {
        if !self.is_editable(f) {
            return None;
        }
        let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
        let seed = self.session.text_of(f).unwrap_or("").to_owned();
        Some(&mut self.edits.entry(id).or_insert_with(|| Edit { seed: seed.clone(), value: seed }).value)
    }

    fn text_input(&mut self, t: &str) -> Vec<Frame> {
        let Some(f) = self.focused else { return Vec::new() };
        let Some(buf) = self.edit_buf(f) else { return Vec::new() };
        buf.push_str(t);
        self.show_edit(f);
        self.emit(f, EventKind::TextInput, Value::Str(t.to_owned()))
    }

    /// Spec 06 §3: a composition is local. The field shows its buffer plus
    /// the preedit; nothing leaves the client until the method commits.
    fn preedit(&mut self, t: String) {
        let Some(f) = self.focused.filter(|f| self.is_editable(*f)) else { return };
        self.preedit = t;
        let _ = self.edit_buf(f);
        self.show_edit(f);
    }

    /// Put the field's buffer, with any composition, into the tree.
    fn show_edit(&mut self, f: NodeIx) {
        let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
        let Some(edit) = self.edits.get(&id) else { return };
        let mut value = edit.value.clone();
        value.push_str(&self.preedit);
        self.session.set_text_local(f, value);
        self.invalidate();
    }

    /// Where an input method should put its candidate window: the focused
    /// editable node's box, if any.
    pub fn ime_area(&self) -> Option<eui_layout::Rect> {
        let f = self.focused.filter(|f| self.is_editable(*f))?;
        self.layout.rect(f)
    }

    fn key(&mut self, key: &str, modifiers: u32, down: bool) -> Vec<Frame> {
        // Navigation keys belong to the client and are never reported.
        if key == "Tab" {
            return if down { self.move_focus(modifiers & 1 != 0) } else { Vec::new() };
        }
        let Some(f) = self.focused else { return Vec::new() };
        if key == "Escape" {
            return if down { self.set_focus(None, false) } else { Vec::new() };
        }
        let mut out = Vec::new();
        if down {
            let editable = self.is_editable(f);
            match key {
                "Backspace" if editable => {
                    if let Some(buf) = self.edit_buf(f) {
                        buf.pop();
                        self.show_edit(f);
                    }
                }
                "Enter" if self.session.node(f).map(|n| n.kind) == Some(NodeKind::Input) => {
                    out.extend(self.commit_edit(f));
                    out.extend(self.emit(f, EventKind::Submit, Value::Null));
                }
                "Enter" | " " if !editable => out.extend(self.activate(f)),
                _ => {}
            }
        }
        let kind = if down { EventKind::KeyDown } else { EventKind::KeyUp };
        out.extend(self.emit(f, kind, Value::List(vec![Value::Str(key.to_owned()), Value::Int(i64::from(modifiers))])));
        out
    }

    /// `change`, if the field's value differs from what the server has.
    fn commit_edit(&mut self, f: NodeIx) -> Vec<Frame> {
        let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
        let Some(edit) = self.edits.get_mut(&id) else { return Vec::new() };
        if edit.value == edit.seed {
            return Vec::new();
        }
        edit.seed = edit.value.clone();
        let value = edit.value.clone();
        self.emit(f, EventKind::Change, Value::Str(value))
    }

    // --------------------------------------------------------------- paint

    /// Lay out if needed and produce this frame's draw list for a
    /// `w × h` device-pixel target. Clears the redraw flag.
    pub fn paint(&mut self, device_w: u32, device_h: u32) -> DrawList {
        self.note_style_changes();
        self.ensure_layout();
        self.redraw = false;
        let now = self.now;
        let overrides: Vec<(NodeIx, Colors)> = self.anims.iter().map(|(ix, a)| (*ix, a.at(now))).collect();
        let list = paint(&mut Scene {
            session: &self.session,
            layout: &self.layout,
            theme: &self.resolved,
            text: &mut self.text,
            atlas: &mut self.atlas,
            images: &self.images,
            scale: self.scale,
            size: (device_w, device_h),
            focus: if self.focus_visible { self.focused } else { None },
            overrides: &overrides,
        });
        self.session.clear_all_dirty();
        // A finished transition painted its final colours this frame.
        self.anims.retain(|(ix, a)| !a.done(now) && self.session.node(*ix).is_some());
        self.next_due = if self.anims.is_empty() { None } else { Some(now + Duration::from_millis(16)) };
        list
    }

    /// The atlases, for the renderer's upload.
    pub fn atlases_mut(&mut self) -> (&mut Atlas, &mut ImageAtlas) {
        (&mut self.atlas, &mut self.images)
    }

    /// The node under the pointer, if any.
    pub fn hovered(&self) -> Option<NodeIx> {
        self.pointer.over
    }

    /// The focused node, if any.
    pub fn focused(&self) -> Option<NodeIx> {
        self.focused
    }
}


/// The chunk's window onto the session: root props as state, text and props
/// on nodes by key atom, and an event queue. Nothing else is reachable.
struct SessionHost<'a> {
    session: &'a mut Session,
    emitted: Vec<u32>,
    touched: bool,
}

fn to_wire(v: eui_vm::Value) -> Value {
    match v {
        eui_vm::Value::Null => Value::Null,
        eui_vm::Value::Bool(b) => Value::Bool(b),
        eui_vm::Value::Int(n) => Value::Int(n),
        eui_vm::Value::Str(s) => Value::Str(s),
    }
}

fn from_wire(v: &Value) -> eui_vm::Value {
    match v {
        Value::Bool(b) => eui_vm::Value::Bool(*b),
        Value::Int(n) => eui_vm::Value::Int(*n),
        Value::Str(s) => eui_vm::Value::Str(s.clone()),
        Value::Float(f) => eui_vm::Value::Int(*f as i64),
        _ => eui_vm::Value::Null,
    }
}

impl eui_vm::Host for SessionHost<'_> {
    fn atom(&self, id: u32) -> Option<&str> {
        self.session.atom(id)
    }
    fn load(&self, atom: u32) -> eui_vm::Value {
        self.session.root_prop(atom).map_or(eui_vm::Value::Null, from_wire)
    }
    fn store(&mut self, atom: u32, value: eui_vm::Value) -> bool {
        self.session.set_root_prop_local(atom, to_wire(value))
    }
    fn set_text(&mut self, key: u32, text: String) -> bool {
        let Some(ix) = self.session.lookup_key(key) else { return false };
        self.touched = true;
        self.session.set_text_local(ix, text)
    }
    fn set_prop(&mut self, key: u32, atom: u32, value: eui_vm::Value) -> bool {
        let Some(ix) = self.session.lookup_key(key) else { return false };
        self.touched = true;
        self.session.set_prop_local(ix, atom, to_wire(value))
    }
    fn set_style(&mut self, key: u32, style: u32) -> bool {
        let Some(ix) = self.session.lookup_key(key) else { return false };
        self.touched = true;
        self.session.set_style_local(ix, style)
    }
    fn emit(&mut self, atom: u32) {
        self.emitted.push(atom);
    }
}


/// Layout's view of text and assets: shaping from the text engine, image
/// sizes from the store. An image not yet fetched has no size, and gets one
/// the moment it arrives.
struct Measurer<'a> {
    text: &'a mut TextEngine,
    assets: &'a AssetStore,
}

impl TextMeasurer for Measurer<'_> {
    fn measure(&mut self, text: &str, font: FontSpec, max_width: Option<f32>, line_clamp: u8) -> TextMetrics {
        self.text.measure(text, font, max_width, line_clamp)
    }
    fn asset_size(&mut self, hash: &[u8; 32]) -> Option<(f32, f32)> {
        self.assets.image(hash).map(|i| (i.width as f32, i.height as f32))
    }
}
