//! The session driver: everything the client does that is not a window or a
//! socket. Frames in, frames out; input in, events out; a draw list when
//! asked. Pure enough to be tested without a display or a network.

use std::collections::HashMap;

use eui_layout::{Env, Layout, Size};
use eui_proto::{
    caps, Batch, EventFrame, EventKind, Frame, Handler, Hello, NodeKind, ThemeMode, Value, Viewport, PROTOCOL_VERSION,
};
use eui_render::{paint, Atlas, DrawList, Scene};
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

/// The driver.
pub struct Driver {
    session: Session,
    layout: Layout,
    theme: Theme,
    resolved: Resolved,
    viewer: Viewer,
    text: TextEngine,
    atlas: Atlas,
    size: Size,
    scale: f32,
    pointer: Pointer,
    focused: Option<NodeIx>,
    edits: HashMap<u32, String>,
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
            size: Size::new(w, h),
            scale,
            pointer: Pointer::default(),
            focused: None,
            edits: HashMap::new(),
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
                // Focus and edits follow the tree.
                if self.focused.is_some_and(|f| self.session.node(f).is_none()) {
                    self.focused = None;
                }
                self.edits.retain(|id, _| self.session.lookup(*id).is_some());
                vec![Frame::Ack { seq: batch.seq }]
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
            Input::Key { key, modifiers, down } => self.key(&key, modifiers, down),
            Input::Unfocused => {
                let mut out = Vec::new();
                if let Some(f) = self.focused.take() {
                    out.extend(self.commit_edit(f));
                    out.extend(self.emit(f, EventKind::Blur, Value::Null));
                    self.redraw = true;
                }
                self.pointer.pressed_on = None;
                out
            }
        }
    }

    fn ensure_layout(&mut self) {
        if !self.layout_valid {
            self.layout.compute(&mut Env { session: &self.session, theme: &self.resolved, text: &mut self.text }, self.size);
            self.layout_valid = true;
        }
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
                    Some(Chunk::Hash(_)) => Err("chunk by hash: asset fetching is not implemented".into()),
                    None => Err("undefined chunk".into()),
                };
                match result {
                    Ok(c) => {
                        self.chunks.insert(chunk_id, Some(c.clone()));
                        c
                    }
                    Err(e) => {
                        self.chunks.insert(chunk_id, None);
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

    fn local_point(&self, ix: NodeIx, x: f32, y: f32) -> Value {
        let r = self.layout.rect(ix).unwrap_or_default();
        Value::List(vec![Value::Float(f64::from(x - r.x)), Value::Float(f64::from(y - r.y))])
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
            let p = self.local_point(ix, x, y);
            out.extend(self.emit(ix, EventKind::PointerMove, p));
        }
        out
    }

    fn pointer_down(&mut self, button: u8) -> Vec<Frame> {
        self.ensure_layout();
        let (x, y) = (self.pointer.x, self.pointer.y);
        let Some(ix) = self.layout.hit(&self.session, x, y) else { return Vec::new() };
        self.pointer.pressed_on = Some(ix);
        let mut out = Vec::new();
        // Focus moves to the nearest editable node on the path, or nowhere.
        let editable = self.ancestor_where(ix, |k| matches!(k, NodeKind::Input | NodeKind::TextArea));
        if editable != self.focused {
            if let Some(old) = self.focused.take() {
                out.extend(self.commit_edit(old));
                out.extend(self.emit(old, EventKind::Blur, Value::Null));
            }
            if let Some(new) = editable {
                self.focused = Some(new);
                self.edits.entry(self.session.node(new).map(|n| n.id).unwrap_or(0)).or_insert_with(|| self.session.text_of(new).unwrap_or("").to_owned());
                out.extend(self.emit(new, EventKind::Focus, Value::Null));
            }
            self.redraw = true;
        }
        let r = self.layout.rect(ix).unwrap_or_default();
        let payload = Value::List(vec![Value::Float(f64::from(x - r.x)), Value::Float(f64::from(y - r.y)), Value::Int(i64::from(button))]);
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
        let r = self.layout.rect(ix).unwrap_or_default();
        let payload = Value::List(vec![Value::Float(f64::from(x - r.x)), Value::Float(f64::from(y - r.y)), Value::Int(i64::from(button))]);
        out.extend(self.emit(ix, EventKind::PointerUp, payload));
        // A click is a press and a release that resolve to the same handler.
        if let Some(pressed) = self.pointer.pressed_on.take() {
            let same = self.target(pressed, EventKind::Click).map(|t| t.0) == self.target(ix, EventKind::Click).map(|t| t.0);
            if same {
                let kind = if button == 1 { EventKind::ContextMenu } else { EventKind::Click };
                let p = self.local_point(ix, x, y);
                out.extend(self.emit(ix, kind, p));
            }
        }
        out
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

    fn text_input(&mut self, t: &str) -> Vec<Frame> {
        let Some(f) = self.focused else { return Vec::new() };
        let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
        let buf = self.edits.entry(id).or_default();
        buf.push_str(t);
        let value = buf.clone();
        self.session.set_text_local(f, value);
        self.invalidate();
        self.emit(f, EventKind::TextInput, Value::Str(t.to_owned()))
    }

    fn key(&mut self, key: &str, modifiers: u32, down: bool) -> Vec<Frame> {
        let Some(f) = self.focused else { return Vec::new() };
        let mut out = Vec::new();
        if down {
            match key {
                "Backspace" => {
                    let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
                    if let Some(buf) = self.edits.get_mut(&id) {
                        buf.pop();
                        let value = buf.clone();
                        self.session.set_text_local(f, value);
                        self.invalidate();
                    }
                }
                "Enter" if self.session.node(f).map(|n| n.kind) == Some(NodeKind::Input) => {
                    out.extend(self.commit_edit(f));
                    out.extend(self.emit(f, EventKind::Submit, Value::Null));
                }
                _ => {}
            }
        }
        let kind = if down { EventKind::KeyDown } else { EventKind::KeyUp };
        out.extend(self.emit(f, kind, Value::List(vec![Value::Str(key.to_owned()), Value::Int(i64::from(modifiers))])));
        out
    }

    fn commit_edit(&mut self, f: NodeIx) -> Vec<Frame> {
        let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
        match self.edits.get(&id).cloned() {
            Some(value) => self.emit(f, EventKind::Change, Value::Str(value)),
            None => Vec::new(),
        }
    }

    // --------------------------------------------------------------- paint

    /// Lay out if needed and produce this frame's draw list for a
    /// `w × h` device-pixel target. Clears the redraw flag.
    pub fn paint(&mut self, device_w: u32, device_h: u32) -> DrawList {
        self.ensure_layout();
        self.redraw = false;
        let list = paint(&mut Scene {
            session: &self.session,
            layout: &self.layout,
            theme: &self.resolved,
            text: &mut self.text,
            atlas: &mut self.atlas,
            scale: self.scale,
            size: (device_w, device_h),
        });
        self.session.clear_all_dirty();
        list
    }

    /// The atlas, for the renderer's upload.
    pub fn atlas_mut(&mut self) -> &mut Atlas {
        &mut self.atlas
    }

    /// The node under the pointer, if any.
    pub fn hovered(&self) -> Option<NodeIx> {
        self.pointer.over
    }

    /// The focused editable node, if any.
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
    fn emit(&mut self, atom: u32) {
        self.emitted.push(atom);
    }
}
