//! The session driver: everything the client does that is not a window or a
//! socket. Frames in, frames out; input in, events out; a draw list when
//! asked. Pure enough to be tested without a display or a network.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eui_audio::Control;
use eui_layout::{Env, FontSpec, Layout, Rect, Size, TextMeasurer, TextMetrics};
use eui_proto::{
    caps, AlignItems, Batch, ColorRef, Cursor, Dim, Display, EventFrame, EventKind, FlatNode, FontWeight, Frame, Handler, Hello, Justify, NodeKind, Op, StyleRecord, Subtree, TextAlign, TextRef,
    ThemeMode, Value, Viewport, PROTOCOL_VERSION,
};
use eui_render::{colors_of, paint, scrollbar_thumb, Atlas, Colors, DrawList, Editing, ImageAtlas, Scene, SCROLLBAR_WIDTH};

use crate::assets::{AssetStore, Hash};
use eui_text::TextEngine;
use eui_theme::{Resolved, Theme, Viewer};
use eui_tree::{Chunk, NodeIx, Session};

/// What the window feeds the driver.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// Pointer moved to logical `(x, y)`.
    PointerMove(f32, f32),
    /// The pointer left the window. Without this the node it was over
    /// never hears `pointer_leave`, and a local handler that lit it on
    /// enter leaves it lit — a hover that outlives the pointer.
    PointerOut,
    /// A button went down: `0` primary, `1` secondary, `2` middle.
    PointerDown(u8),
    /// A button came up.
    PointerUp(u8),
    /// Trackpad or a wheel already reported in pixels: applied at once.
    Wheel(f32, f32),
    /// Wheel notches, in lines: the driver turns each into a short eased
    /// scroll so a notched wheel reads as smoothly as a trackpad.
    WheelStep(f32, f32),
    /// Committed text.
    Text(String),
    /// An input method's composition in progress: shown in the focused
    /// field, never reported. An empty string ends the composition.
    ImePreedit(String),
    /// An input method committed `text`: the composition ends and the text
    /// is inserted as if typed.
    ImeCommit(String),
    /// The person pasted `text` into the focused field (the window read the
    /// clipboard on their `Ctrl+V`): inserted at the caret, replacing the
    /// selection, reported as one `text_input`.
    Paste(String),
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
    /// The transport handed over something that was not a frame, or went
    /// away; the window's reason.
    Transport(String),
}

impl std::fmt::Display for Close {
    /// The sentence a person reads when the window stops talking to its
    /// application (01 §4), not a debug print.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ServerError(code, message) => write!(f, "{message} (error {code})"),
            Self::Version(v) => write!(f, "the server speaks protocol version {v}, which this client does not"),
            Self::Protocol(why) => write!(f, "the connection broke the protocol: {why}"),
            Self::Transport(why) => write!(f, "{why}"),
        }
    }
}

#[derive(Debug, Default)]
struct Pointer {
    x: f32,
    y: f32,
    over: Option<NodeIx>,
    pressed_on: Option<NodeIx>,
    /// A scrollbar thumb being dragged: the scroller and where in the thumb
    /// the pointer took hold.
    dragging_thumb: Option<(NodeIx, f32)>,
    /// The pointer moved while a frame was owed; hover is settled at paint.
    hover_pending: bool,
    /// The scroller whose scrollbar strip the pointer rests on.
    over_scrollbar: Option<NodeIx>,
    /// Spec 06 §2: at most one `pointer_move` per frame per node. A slider
    /// drag stores the latest and flushes it at paint, so the server is not
    /// asked to re-render the page a hundred times a second.
    coalesced_move: Option<(NodeIx, Value)>,
}

/// A field's local edit: the value the server last saw (`seed`), the value
/// typed since, and the caret and selection anchor as byte offsets into it.
/// `change` fires only when seed and value differ.
#[derive(Debug, Clone)]
struct Edit {
    seed: String,
    value: String,
    caret: usize,
    anchor: usize,
    /// Logical px the field's text is scrolled left to keep the caret in view.
    scroll_x: f32,
}

impl Edit {
    fn selection(&self) -> std::ops::Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    /// Replace the selection (or insert at the caret) with `text`.
    fn insert(&mut self, text: &str) {
        let r = self.selection();
        self.value.replace_range(r.clone(), text);
        self.caret = r.start.saturating_add(text.len());
        self.anchor = self.caret;
    }

    /// Delete the selection, or one char before (`forward == false`) or
    /// after the caret.
    fn delete(&mut self, forward: bool) {
        let r = self.selection();
        let r = if !r.is_empty() {
            r
        } else if forward {
            self.caret..next_char(&self.value, self.caret)
        } else {
            prev_char(&self.value, self.caret)..self.caret
        };
        self.value.replace_range(r.clone(), "");
        self.caret = r.start;
        self.anchor = self.caret;
    }

    fn place(&mut self, at: usize, extend: bool) {
        self.caret = at.min(self.value.len());
        if !extend {
            self.anchor = self.caret;
        }
    }
}

fn prev_char(s: &str, at: usize) -> usize {
    s[..at.min(s.len())].char_indices().next_back().map_or(0, |(i, _)| i)
}

fn next_char(s: &str, at: usize) -> usize {
    let at = at.min(s.len());
    s[at..].chars().next().map_or(at, |c| at.saturating_add(c.len_utf8()))
}

/// The start of the word before `at`: back over spaces, then over the word.
fn word_left(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    while i > 0 && s[..i].ends_with(char::is_whitespace) {
        i = prev_char(s, i);
    }
    while i > 0 && !s[..i].ends_with(char::is_whitespace) {
        i = prev_char(s, i);
    }
    i
}

/// The end of the word after `at`: over the word, then over the spaces.
fn word_right(s: &str, at: usize) -> usize {
    let mut i = at.min(s.len());
    while i < s.len() && !s[i..].starts_with(char::is_whitespace) {
        i = next_char(s, i);
    }
    while i < s.len() && s[i..].starts_with(char::is_whitespace) {
        i = next_char(s, i);
    }
    i
}

/// The start and end of the line holding `at`.
fn line_bounds(s: &str, at: usize) -> (usize, usize) {
    let at = at.min(s.len());
    let start = s[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = s[at..].find('\n').map_or(s.len(), |i| at + i);
    (start, end)
}

/// One running transition: the colours it left, the colours it reaches,
/// and when.
#[derive(Debug, Clone, Copy)]
struct Anim {
    from: Colors,
    to: Colors,
    start: Instant,
    duration: Duration,
    /// A style change eases along the theme's own curve; something
    /// arriving decelerates instead, so it reads as having come from
    /// somewhere rather than having been switched on (03 §5).
    curve: eui_theme::Curve,
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
        let k = self.curve.at(t);
        Colors {
            bg: mix(self.from.bg, self.to.bg, k),
            fg: mix(self.from.fg, self.to.fg, k),
            border: mix(self.from.border, self.to.border, k),
            opacity: self.from.opacity + (self.to.opacity - self.from.opacity) * k,
            blur: self.from.blur + (self.to.blur - self.from.blur) * k,
        }
    }
}

/// `EUI_TRACE=1`: a line on stderr for the events a screen shows and a
/// test cannot — focus, caret placement, scrolls.
pub fn trace(line: impl FnOnce() -> String) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var("EUI_TRACE").as_deref() == Ok("1")) {
        let t = START.get_or_init(Instant::now).elapsed().as_secs_f64();
        eprintln!("eui {t:9.3}: {}", line());
    }
}

/// How long a scroll must have been still before a windowed list asks
/// for the rows now in view (04 §7.1).
/// How often a `spin` alone asks for a frame: 30 a second. A transition
/// runs at 60, but a spinner is a mark at rest, and the machine it is on
/// should be too (10 §1).
const SPIN_FRAME: Duration = Duration::from_millis(33);
const WINDOW_SETTLE: Duration = Duration::from_millis(120);
/// While a scroll is still moving, how often it may ask for rows it has
/// outrun: often enough that a drag sees rows rather than placeholders,
/// seldom enough that a drag is not a server render a frame.
const WINDOW_OUTRUN: Duration = Duration::from_millis(50);
/// Wait this long after the last resize before telling the server. Sending
/// every size during a drag makes charts and grids step through layouts.
const VIEWPORT_SETTLE: Duration = Duration::from_millis(50);

/// Spec 06 §1.1: the fastest a node may ask to be woken. A clock is not a
/// render loop, and the budget of 10 §1 says a window at rest costs
/// nothing — so what asks for time says how much, and cannot ask for all
/// of it.
const MIN_WAKE_MS: u64 = 100;

/// How many nodes may be waking at once. Four clocks is a player, a
/// countdown and two things left over; a thousand is a server spinning
/// the client.
const MAX_WAKES: usize = 4;

/// A scroll offset easing from `from` to `to`, per wheel notch (03 §5's
/// `motion.base`); a notch arriving mid-way retargets from where the view is.
#[derive(Debug, Clone, Copy)]
struct ScrollAnim {
    /// Ease in and out (a key press: the view departs as gently as it
    /// arrives) rather than out only (a wheel notch, already in motion).
    smooth: bool,
    node: NodeIx,
    from: (f32, f32),
    to: (f32, f32),
    start: Instant,
    duration: Duration,
}

impl ScrollAnim {
    fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.duration
    }

    fn at(&self, now: Instant) -> (f32, f32) {
        if self.done(now) {
            return self.to;
        }
        let t = now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32().max(1e-3);
        let k = if self.smooth { eui_theme::scale::ease_in_out(t) } else { eui_theme::scale::ease(t) };
        (self.from.0 + (self.to.0 - self.from.0) * k, self.from.1 + (self.to.1 - self.from.1) * k)
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
    /// A wheel notch in flight: the offset it left, the one it reaches.
    scroll_anim: Option<ScrollAnim>,
    /// When the driver was made: `spin` phases count from here.
    epoch: Instant,
    /// Frames produced outside an input: see [`Driver::take_pending`].
    pending: Vec<Frame>,
    /// Sub-pixel wheel motion not yet applied: a trackpad reports fractions
    /// of a pixel per event, and truncating each one would swallow them all.
    wheel_rest: (f32, f32),
    now: Instant,
    next_due: Option<Instant>,
    edits: HashMap<u32, Edit>,
    /// The composition an input method is building in the focused field.
    preedit: String,
    /// Text the person copied or cut, for the window to hand the clipboard.
    clipboard: Option<String>,
    /// Verified chunks by id; verification happens once per chunk.
    chunks: HashMap<u32, Option<eui_vm::Chunk>>,
    /// Effects of local-then-server handlers awaiting the server's answer.
    provisional: Vec<Undo>,
    granted: u32,
    welcomed: bool,
    /// A `Resync` went out and its answer has not arrived: if that answer is
    /// refused too, the session ends rather than looping.
    resyncing: bool,
    layout_valid: bool,
    /// A batch undid a previewed style: the node under the pointer runs
    /// its `enter` again at the next hover settle, even though the pointer
    /// has not moved.
    hover_relight: bool,
    redraw: bool,
    closed: Option<Close>,
    /// Spec 04 §7.1: the row range last reported by each windowed list,
    /// by node id, so a range is reported once.
    windows: HashMap<u32, (u32, u32)>,
    /// When a moving scroll last asked for rows it had outrun.
    outrun_at: Option<Instant>,
    /// The last list painted, when a `spin` was the only thing owed: the
    /// next frame is the same list with the clock moved on, and the vertex
    /// stage moves the clock (03 §5), so the tree need not be walked again.
    spin_list: Option<DrawList>,
    /// Something reached the driver since the last paint — an input, a
    /// frame, an asset, a theme — that could change what the tree paints.
    /// The spin clock is not that: `tick` leaves it alone.
    touched: bool,
    /// Frames answered from `spin_list`, for the trace and the tests.
    spin_repeats: u64,
    /// When a scroll offset last changed: a windowed list asks for rows
    /// once the view has been still for a moment, not per frame of a drag.
    scroll_touched: Option<Instant>,
    /// Spec 03 §8: the moving pictures in the tree, and where each node
    /// is in its own. Decoding runs here, in the worker; the frame the
    /// clock makes due is written into the image atlas, so the painter
    /// draws a video exactly as it draws a picture.
    movies: HashMap<Hash, Option<Arc<eui_video::Movie>>>,
    players: HashMap<u32, (Hash, eui_video::Player)>,
    /// A frame is in the atlas for these hashes, so the next one is an
    /// overwrite rather than a fresh packing.
    framed: HashMap<Hash, usize>,
    /// Frame sizes, so the layout can measure a video before a frame is
    /// ever uploaded.
    video_sizes: HashMap<Hash, (f32, f32)>,
    /// The tree changed, so the video nodes must be looked at again.
    video_dirty: bool,
    /// Spec 06 §1.1: the nodes that asked to be woken, as `(node id,
    /// period, when it is next due)`. Rebuilt from the tree whenever a
    /// batch changed it, and kept otherwise so a re-render does not reset
    /// the phase of a clock that is already running.
    wakes: Vec<(u32, Duration, Instant)>,
    /// Whether the tree changed since the wakes were last collected.
    wake_dirty: bool,
    /// Whether the notice of [`Self::show_stopped`] has replaced the tree,
    /// so it is mounted once and not on every frame after.
    stopped: bool,
    /// When the players were last advanced.
    video_clock: Option<Instant>,
    /// When the next video frame is due. Applied at the end of the paint,
    /// after the transition scheduling, which overwrites `next_due`.
    video_due: Option<Instant>,
    /// After a resize, wait [`VIEWPORT_SETTLE`] before sending `Viewport`
    /// so a drag does not restyle the tree once per pixel.
    viewport_due: Option<Instant>,
    /// Spec 03 §7: the sounds this session is playing, and the decoded
    /// bytes behind them. The mixer lives here — in the worker — because
    /// decoding runs on bytes a server chose; the window owns the device.
    mixer: eui_audio::Mixer,
    sounds: HashMap<Hash, Option<Arc<eui_audio::Sound>>>,
    /// The `position` prop each video node last carried, for the same
    /// reason as the audio one.
    video_at: HashMap<u32, i64>,
    /// The `position` prop each audio node last carried: a seek happens
    /// when the value changes, not on every render.
    audio_at: HashMap<u32, i64>,
    /// The asset each sounding node was loaded from. A node keeps its id
    /// when its `src` changes — a tracker renders a new wav into the same
    /// node — and without this the mixer would go on playing the first
    /// sound it was ever given.
    audio_src: HashMap<u32, Hash>,
    /// The tree changed, so the audio nodes must be looked at again.
    audio_dirty: bool,
    /// When `time_update` was last sent, for the rate limit of 03 §7.
    audio_reported: Option<Instant>,
    /// The viewer's desktop palette by role, on top of the theme (05 §5),
    /// and the mode it is for: in the other mode the theme's own colours
    /// show, so a light/dark switch still switches something.
    desktop_colors: Vec<(eui_theme::Role, u32)>,
    desktop_mode: Option<ThemeMode>,
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
            scroll_anim: None,
            epoch: Instant::now(),
            pending: Vec::new(),
            wheel_rest: (0.0, 0.0),
            now: Instant::now(),
            next_due: None,
            edits: HashMap::new(),
            preedit: String::new(),
            clipboard: None,
            chunks: HashMap::new(),
            provisional: Vec::new(),
            granted: granted & caps::ALL,
            welcomed: false,
            resyncing: false,
            hover_relight: false,
            layout_valid: false,
            redraw: true,
            closed: None,
            desktop_colors: Vec::new(),
            desktop_mode: None,
            windows: HashMap::new(),
            outrun_at: None,
            spin_list: None,
            touched: true,
            spin_repeats: 0,
            scroll_touched: None,
            movies: HashMap::new(),
            players: HashMap::new(),
            framed: HashMap::new(),
            video_sizes: HashMap::new(),
            video_dirty: false,
            wakes: Vec::new(),
            wake_dirty: false,
            stopped: false,
            video_clock: None,
            video_due: None,
            viewport_due: None,
            mixer: eui_audio::Mixer::new(48_000),
            sounds: HashMap::new(),
            audio_at: HashMap::new(),
            audio_src: HashMap::new(),
            video_at: HashMap::new(),
            audio_dirty: false,
            audio_reported: None,
        }
    }

    /// Grant capabilities before the session opens — what the manifest asked
    /// for, intersected with what the person allowed. Never more than
    /// [`caps::ALL`]; never anything implicitly.
    pub fn grant(&mut self, granted: u32) {
        self.touched = true;
        self.granted = granted & caps::ALL;
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

    /// End the session from outside: the transport spoke nonsense or went
    /// away. Nothing is sent; the window reports `why`.
    pub fn close(&mut self, why: String) {
        self.touched = true;
        if self.closed.is_none() {
            self.closed = Some(Close::Transport(why));
        }
    }

    /// Frames waiting for the window to send, to add to. What
    /// [`Self::take_pending`] hands over.
    pub fn pending_mut(&mut self) -> &mut Vec<Frame> {
        self.touched = true;
        &mut self.pending
    }

    // -------------------------------------------------------------- frames

    /// Apply a frame from the server; returns frames to send back.
    pub fn handle_frame(&mut self, frame: Frame) -> Vec<Frame> {
        self.touched = true;
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
        self.revert_provisional();
        match self.session.apply(batch) {
            Ok(()) => {
                self.resyncing = false;
                self.audio_dirty = true;
                self.video_dirty = true;
                self.wake_dirty = true;
                self.invalidate();
                self.note_style_changes();
                self.note_entrances();
                // The batch put back every style a local handler had
                // previewed. Whatever the pointer is still over must light
                // again, so its `enter` runs once more at the next paint —
                // where hover settles anyway. `hovered()` does not move in
                // the meantime: the pointer never went anywhere.
                if self.session.take_restored_local() {
                    self.pointer.hover_pending = true;
                    self.hover_relight = true;
                }
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
            Err(e) if self.resyncing => {
                // The fresh tree is refused too: nothing the server sends
                // again will pass, so say why and stop instead of asking
                // forever. A tree past the client's limits is the usual cause.
                let message = format!("the resynced tree was refused: {e}");
                eprintln!("eui: {message}; closing");
                self.closed = Some(Close::Protocol("resync refused"));
                vec![Frame::Error { code: 102, message }]
            }
            Err(e) => {
                // Recoverable by design: discard, ask for a fresh tree, rebuild.
                // Not an `Error` frame — that would end the session on both
                // sides, which is the opposite of what a resync is for.
                eprintln!("eui: batch {} rejected ({e}); resyncing", batch.seq);
                self.resyncing = true;
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
            let Some(ms) = new.transition.checked_sub(1).and_then(|i| self.resolved.motion.get(usize::from(i))) else {
                continue;
            };
            let to = colors_of(&self.session, &self.resolved, &new);
            let from = match self.anims.iter().position(|(n, _)| *n == ix) {
                Some(i) => self.anims.remove(i).1.at(self.now),
                None => self.session.style(old).map_or(to, |r| colors_of(&self.session, &self.resolved, r)),
            };
            self.anims.push((ix, Anim { from, to, start: self.now, duration: Duration::from_millis(u64::from(*ms)), curve: eui_theme::Curve::STANDARD }));
            self.next_due = Some(self.now);
            self.redraw = true;
        }
    }

    /// Spec 03 §5: a node grafted with `animation` = `enter` arrives from
    /// nothing — transparent, unblurred — and reaches its own record over
    /// its `transition` duration, or `motion.base` when it names none.
    ///
    /// This is the only thing a mount animates, and it has to be asked for.
    /// A node that is restyled mid-entrance is left to `note_style_changes`,
    /// which picks the animation up from wherever it visibly is.
    fn note_entrances(&mut self) {
        for ix in self.session.take_entrances() {
            if self.session.node(ix).is_none() {
                continue;
            }
            let record = self.session.style_of(ix);
            let ms = record.transition.checked_sub(1).and_then(|i| self.resolved.motion.get(usize::from(i))).or_else(|| self.resolved.motion.get(1));
            let Some(ms) = ms else { continue };
            let to = colors_of(&self.session, &self.resolved, &record);
            // `mix` fades an absent colour through transparent, so leaving
            // the three of them `None` is what makes this a fade rather than
            // a wash through some arbitrary starting colour.
            let from = Colors { bg: None, fg: None, border: None, opacity: 0.0, blur: 0.0 };
            self.anims.retain(|(n, _)| *n != ix);
            self.anims.push((ix, Anim { from, to, start: self.now, duration: Duration::from_millis(u64::from(*ms)), curve: eui_theme::Curve::DECELERATE }));
            self.next_due = Some(self.now);
            self.redraw = true;
        }
    }

    /// Advance the clock. True when a transition frame is due, so the window
    /// should redraw; false at rest, which is almost always.
    pub fn tick(&mut self, now: Instant) -> bool {
        self.now = now;
        match [self.next_due, self.viewport_due].into_iter().flatten().min() {
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
        [self.next_due, self.viewport_due].into_iter().flatten().min()
    }

    /// True while any transition runs.
    pub fn animating(&self) -> bool {
        !self.anims.is_empty() || self.scroll_anim.is_some()
    }

    /// Move a scroll animation to `now`: the offset it dictates goes into
    /// the tree before layout. Returns the frames to send once it lands.
    fn advance_scroll(&mut self) -> Vec<Frame> {
        let Some(a) = self.scroll_anim else {
            return Vec::new();
        };
        if self.session.node(a.node).is_none() {
            self.scroll_anim = None;
            return Vec::new();
        }
        let (x, y) = a.at(self.now);
        self.session.set_scroll(a.node, x.round() as i64, y.round() as i64);
        self.layout_valid = false;
        self.scroll_touched = Some(self.now);
        if a.done(self.now) {
            self.scroll_anim = None;
            let (nx, ny) = (a.to.0.round() as i64, a.to.1.round() as i64);
            return self.emit(a.node, EventKind::Scroll, Value::List(vec![Value::Int(nx), Value::Int(ny)]));
        }
        Vec::new()
    }

    /// The display scale, device px per logical px.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Focus `ix` as the keyboard would — ring shown — for an assistive
    /// technology's Focus action.
    pub fn focus_node(&mut self, ix: NodeIx) -> Vec<Frame> {
        self.touched = true;
        self.ensure_layout();
        if self.focus_order().contains(&ix) {
            self.set_focus(Some(ix), true)
        } else {
            Vec::new()
        }
    }

    /// Focus and press `ix`, as Tab then Enter would — an assistive
    /// technology's Click action. Nothing a keyboard could not do.
    pub fn activate_node(&mut self, ix: NodeIx) -> Vec<Frame> {
        self.touched = true;
        let mut out = self.focus_node(ix);
        if self.focused == Some(ix) && !self.is_editable(ix) {
            out.extend(self.activate(ix));
        }
        out
    }

    /// A role's colour under the viewer's current theme, `0xRRGGBBAA`.
    pub fn theme_color(&self, role: eui_theme::Role) -> u32 {
        self.resolved.color(role)
    }

    // --------------------------------------------------------------- input

    /// Feed input; returns event frames to send.
    pub fn input(&mut self, input: Input) -> Vec<Frame> {
        self.touched = true;
        self.now = Instant::now();
        match input {
            Input::Resized(w, h, scale) => {
                self.size = Size::new(w, h);
                self.scale = scale;
                self.layout.invalidate_all();
                self.invalidate();
                let now = Instant::now();
                self.now = now;
                let due = now + VIEWPORT_SETTLE;
                self.viewport_due = Some(due);
                self.next_due = Some(self.next_due.map_or(due, |d| d.min(due)));
                Vec::new()
            }
            Input::Mode(mode) => self.set_mode(mode),
            Input::PointerMove(x, y) => self.pointer_move(x, y),
            Input::PointerDown(button) => self.pointer_down(button),
            Input::PointerUp(button) => self.pointer_up(button),
            Input::Wheel(dx, dy) => self.wheel(dx, dy),
            Input::WheelStep(lines_x, lines_y) => self.wheel_step(lines_x, lines_y),
            Input::Text(t) => self.text_input(&t),
            Input::ImePreedit(t) => {
                self.preedit(t);
                Vec::new()
            }
            Input::ImeCommit(t) => {
                self.preedit(String::new());
                self.text_input(&t)
            }
            Input::Paste(t) => self.text_input(&t),
            Input::Key { key, modifiers, down } => self.key(&key, modifiers, down),
            Input::PointerOut => self.clear_hover(),
            Input::Unfocused => {
                let mut out = self.set_focus(None, false);
                if let Some(pressed) = self.pointer.pressed_on.take() {
                    let (x, y) = (self.pointer.x, self.pointer.y);
                    let payload = self.button_payload(pressed, EventKind::PointerUp, x, y, 0);
                    out.extend(self.emit(pressed, EventKind::PointerUp, payload));
                }
                out
            }
        }
    }

    fn ensure_layout(&mut self) {
        if !self.layout_valid {
            let mut measurer = Measurer { text: &mut self.text, assets: &self.assets, videos: &self.video_sizes };
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
                .filter(|n| matches!(n.kind, NodeKind::Image | NodeKind::Audio | NodeKind::Video))
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
        self.touched = true;
        self.assets.deliver(hash, bytes);
        // It may be a sound or a picture a node is waiting for.
        self.audio_dirty = true;
        self.video_dirty = true;
        if let Some(img) = self.assets.image(&hash) {
            self.images.insert(hash, img.width, img.height, &img.rgba);
        }
        // An image's intrinsic size just changed under nodes nothing marked
        // dirty: the memoised measures cannot be trusted.
        self.layout.invalidate_all();
        self.invalidate();
    }

    /// Record that a hash could not be fetched.
    pub fn asset_failed(&mut self, hash: Hash, why: String) {
        self.touched = true;
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
        let Some((target, handler)) = self.target(from, kind) else {
            return Vec::new();
        };
        let node = self.session.node(target).map(|n| n.id).unwrap_or(0);
        let mut out = Vec::new();
        let name = match handler {
            Handler::Server(name) => Some(name),
            Handler::Local(chunk) => {
                match self.run_local(chunk, false) {
                    Ok(queued) => {
                        let state = self.root_state();
                        out.extend(queued.into_iter().map(|n| Frame::Event(EventFrame { node, event: kind, name: n, payload: state.clone() })));
                    }
                    Err(e) => eprintln!("eui: local handler {chunk}: {e}"),
                }
                None
            }
            Handler::LocalThenServer { chunk, name } => match self.run_local(chunk, true) {
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
        let Some(root) = self.session.root() else {
            return Value::Null;
        };
        let Some(n) = self.session.node(root) else {
            return Value::Null;
        };
        Value::List(n.props.iter().flat_map(|(a, v)| [Value::Atom(*a), v.clone()]).collect())
    }

    /// Verify (once) and run a chunk against the session. Returns the atoms
    /// the chunk asked to emit, in order.
    fn run_local(&mut self, chunk_id: u32, provisional: bool) -> Result<Vec<u32>, String> {
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
        let mut host = SessionHost { session: &mut self.session, emitted: Vec::new(), touched: false, undo: provisional.then(Vec::new), mode: None };
        let result = eui_vm::run(&verified, &mut host);
        let touched = host.touched;
        let emitted = host.emitted;
        let mode = host.mode;
        if let Some(undo) = host.undo {
            self.provisional.extend(undo);
        }
        if touched {
            self.audio_dirty = true;
            self.video_dirty = true;
            self.invalidate();
        }
        // The viewer's choice, made through the application's own control:
        // never provisional, never undone by a batch.
        if let Some(request) = mode {
            let mode = match request.as_str() {
                "light" => Some(ThemeMode::Light),
                "dark" => Some(ThemeMode::Dark),
                "high_contrast" => Some(ThemeMode::HighContrast),
                "toggle" => Some(if self.viewer.mode == ThemeMode::Dark { ThemeMode::Light } else { ThemeMode::Dark }),
                _ => None,
            };
            if let Some(mode) = mode {
                let out = self.set_mode(mode);
                self.pending.extend(out);
            }
        }
        result.map_err(|e| e.to_string())?;
        Ok(emitted)
    }

    /// Spec 07 §6: a server batch supersedes every provisional change made
    /// since the last one. Put the old values back, newest first, before
    /// the batch applies — its ops are relative to the tree the server has.
    fn revert_provisional(&mut self) {
        let changes = std::mem::take(&mut self.provisional);
        if changes.is_empty() {
            return;
        }
        for change in changes.into_iter().rev() {
            match change {
                Undo::Style(ix, style) => {
                    self.session.set_style_local(ix, style);
                }
                Undo::Text(ix, text) => {
                    self.session.set_text_local(
                        ix,
                        text.map_or(String::new(), |t| match t {
                            TextRef::Inline(s) => s,
                            TextRef::Atom(a) => self.session.atom(a).unwrap_or("").to_owned(),
                        }),
                    );
                }
                Undo::Prop(ix, atom, old) => {
                    self.session.set_prop_local(ix, atom, old.unwrap_or(Value::Null));
                }
            }
        }
        self.invalidate();
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
        self.pointer.x = x;
        self.pointer.y = y;
        // A thumb drag needs no layout: the scroller's box does not move.
        if let Some((scroller, grip)) = self.pointer.dragging_thumb {
            return self.drag_thumb(scroller, grip, y);
        }
        // Dragging inside the focused field extends the selection; the
        // field's box does not move while its text is edited.
        if let (Some(e), Some(pressed)) = (self.focused.filter(|f| self.is_editable(*f)), self.pointer.pressed_on) {
            if self.ancestor_where(pressed, |k| matches!(k, NodeKind::Input | NodeKind::TextArea)) == Some(e) {
                if let Some(at) = self.byte_at_pointer(e, x, y) {
                    if let Some(edit) = self.edit_mut(e) {
                        edit.place(at, true);
                    }
                    self.show_edit(e);
                }
            }
        }
        // A press on a `pointer_move` handler captures the pointer. The
        // thumb follows locally this frame; the event waits for paint so
        // the server sees one move, not one per OS sample.
        if let Some(pressed) = self.pointer.pressed_on {
            if self.session.node(pressed).is_some() && self.target(pressed, EventKind::PointerMove).is_some() {
                let p = self.point_payload(pressed, EventKind::PointerMove, x, y);
                self.pointer.coalesced_move = Some((pressed, p));
                self.redraw = true;
                return Vec::new();
            }
        }
        // Pointer events arrive faster than frames. When a frame is already
        // owed — a scroll just invalidated the layout — hover waits for it
        // rather than forcing a layout per event; the paint settles it.
        if !self.layout_valid {
            self.pointer.hover_pending = true;
            return Vec::new();
        }
        self.hover(x, y)
    }

    /// The pointer is over nothing: whatever it was over hears
    /// `pointer_leave`, once, and the scrollbar it may have been on stops
    /// being hot. Same path as a move that hits nothing, without needing
    /// coordinates for a pointer that is no longer on this window.
    fn clear_hover(&mut self) -> Vec<Frame> {
        self.pointer.hover_pending = false;
        if self.pointer.over_scrollbar.is_some() {
            self.pointer.over_scrollbar = None;
            self.redraw = true;
        }
        let Some(old) = self.pointer.over.take() else {
            return Vec::new();
        };
        if self.session.node(old).is_none() {
            return Vec::new();
        }
        self.emit(old, EventKind::PointerLeave, Value::Null)
    }

    /// Enter, leave and move for the node under `(x, y)`, on a valid layout.
    fn hover(&mut self, x: f32, y: f32) -> Vec<Frame> {
        self.pointer.hover_pending = false;
        let now = self.layout.hit(&self.session, x, y);
        let strip = now.and_then(|hit| self.scroller_strip_at(hit, x));
        if strip != self.pointer.over_scrollbar {
            self.pointer.over_scrollbar = strip;
            self.redraw = true;
        }
        let mut out = Vec::new();
        let relight = std::mem::take(&mut self.hover_relight);
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
        } else if relight {
            if let Some(ix) = now {
                out.extend(self.emit(ix, EventKind::PointerEnter, Value::Null));
            }
        }
        if let Some(ix) = now {
            let p = self.point_payload(ix, EventKind::PointerMove, x, y);
            out.extend(self.emit(ix, EventKind::PointerMove, p));
        }
        out
    }

    fn flush_coalesced_move(&mut self) -> Vec<Frame> {
        let Some((from, payload)) = self.pointer.coalesced_move.take() else {
            return Vec::new();
        };
        if self.session.node(from).is_none() {
            return Vec::new();
        }
        self.emit(from, EventKind::PointerMove, payload)
    }

    /// Place a slider's fill and thumb on the pointer this frame, so a drag
    /// is not waiting for the server's next tree. The press is often a child
    /// of the track; the handler node is the row with the three parts.
    fn follow_slider_drag(&mut self) {
        let Some(pressed) = self.pointer.pressed_on else {
            return;
        };
        let Some((ix, _)) = self.target(pressed, EventKind::PointerMove) else {
            return;
        };
        let (lead, thumb, rest) = {
            let kids = self.session.children(ix);
            let [lead, thumb, rest] = kids[..] else {
                return;
            };
            (lead, thumb, rest)
        };
        let Some(track) = self.layout.rect(ix) else {
            return;
        };
        if track.w <= 0.0 {
            return;
        }
        let thumb_w = self.layout.rect(thumb).map(|r| r.w).unwrap_or(16.0);
        let thumb_h = self.layout.rect(thumb).map(|r| r.h).unwrap_or(16.0);
        let lead_h = self.layout.rect(lead).map(|r| r.h).unwrap_or(4.0);
        let rest_h = self.layout.rect(rest).map(|r| r.h).unwrap_or(4.0);
        let max_lead = (track.w - thumb_w).max(0.0);
        let at = ((self.pointer.x - track.x) - thumb_w / 2.0).clamp(0.0, max_lead);
        let mid_y = track.y + track.h / 2.0;
        self.layout.set_rect(lead, Rect::new(track.x, mid_y - lead_h / 2.0, at, lead_h));
        self.layout.set_rect(thumb, Rect::new(track.x + at, mid_y - thumb_h / 2.0, thumb_w, thumb_h));
        let rest_x = track.x + at + thumb_w;
        self.layout.set_rect(rest, Rect::new(rest_x, mid_y - rest_h / 2.0, (track.x + track.w - rest_x).max(0.0), rest_h));
        if let Some(value) = self.slider_value_at(ix, track) {
            self.update_slider_caption(ix, value);
        }
    }

    fn slider_value_at(&self, ix: NodeIx, track: Rect) -> Option<i64> {
        let n = self.session.node(ix)?;
        let int_prop = |name: &str| {
            let atom = self.session.atom_id(name)?;
            match n.prop(atom)? {
                Value::Int(i) => Some(*i),
                Value::Float(f) => Some(*f as i64),
                _ => None,
            }
        };
        let min = int_prop("min").unwrap_or(0);
        let max = int_prop("max").unwrap_or(100);
        let w = track.w.max(1.0);
        let x = (self.pointer.x - track.x).clamp(0.0, w);
        let span = (max - min) as f32;
        Some((min + (x / w * span).round() as i64).clamp(min.min(max), min.max(max)))
    }

    fn update_slider_caption(&mut self, slider: NodeIx, value: i64) {
        let want = format!("Value {value}");
        if let Some(atom) = self.session.atom_id("gallery_slider_value") {
            if let Some(label) = self.session.lookup_key(atom) {
                let cur = self.session.text_of(label).map(str::to_owned);
                if cur.as_deref() != Some(want.as_str()) {
                    self.session.set_text_local(label, want);
                }
                return;
            }
        }
        let parent = self.session.node(slider).map(|n| n.parent).filter(|p| p.is_some());
        let Some(parent) = parent else { return };
        let kids: Vec<_> = self.session.children(parent).to_vec();
        for c in kids {
            let cur = self.session.text_of(c).map(str::to_owned);
            if cur.as_deref().is_some_and(|t| t.starts_with("Value ") && t != want) {
                self.session.set_text_local(c, want);
                break;
            }
        }
    }

    fn pointer_down(&mut self, button: u8) -> Vec<Frame> {
        self.ensure_layout();
        let (x, y) = (self.pointer.x, self.pointer.y);
        let Some(ix) = self.layout.hit(&self.session, x, y) else {
            return Vec::new();
        };
        // Spec 03 §2: the scrollbar strip belongs to the client. A press on
        // the thumb takes hold of it; a press on the track pages.
        if button == 0 {
            if let Some(scroller) = self.scroller_strip_at(ix, x) {
                let rect = self.layout.rect(scroller).unwrap_or_default();
                if let Some(thumb) = scrollbar_thumb(&self.session, &self.layout, scroller, rect) {
                    if y >= thumb.y && y <= thumb.y + thumb.h {
                        self.pointer.dragging_thumb = Some((scroller, y - thumb.y));
                    } else {
                        let page = if y < thumb.y { -rect.h } else { rect.h };
                        return self.scroll_by(scroller, 0.0, page);
                    }
                    return Vec::new();
                }
            }
        }
        self.pointer.pressed_on = Some(ix);
        // Focus moves to the nearest editable node on the path, else to the
        // nearest one that handles keys — 03 §3: a grid or a canvas is
        // typed into after a click, not after finding it with `Tab`.
        // Nowhere, if the path has neither; a pointer never shows the ring.
        let editable = self.ancestor_where(ix, |k| matches!(k, NodeKind::Input | NodeKind::TextArea));
        let takes_keys = editable.or_else(|| self.ancestor_keyed(ix));
        let mut out = self.set_focus(takes_keys, false);
        if let Some(e) = editable {
            let at = self.byte_at_pointer(e, x, y);
            trace(|| {
                format!(
                    "click in field {:?} at ({x:.1},{y:.1}) rect={:?} text={:?} preedit={:?} -> byte {at:?}",
                    self.session.node(e).map(|n| n.id),
                    self.layout.rect(e),
                    self.session.text_of(e),
                    self.preedit
                )
            });
            if let Some(at) = at {
                if let Some(edit) = self.edit_mut(e) {
                    edit.place(at, false);
                }
                self.show_edit(e);
            }
        }
        let payload = self.button_payload(ix, EventKind::PointerDown, x, y, button);
        out.extend(self.emit(ix, EventKind::PointerDown, payload));
        out
    }

    fn pointer_up(&mut self, button: u8) -> Vec<Frame> {
        if button == 0 && self.pointer.dragging_thumb.take().is_some() {
            return Vec::new();
        }
        let mut out = self.flush_coalesced_move();
        self.ensure_layout();
        let (x, y) = (self.pointer.x, self.pointer.y);
        let hit = self.layout.hit(&self.session, x, y);
        let pressed = self.pointer.pressed_on.take();
        if let Some(ix) = hit {
            let payload = self.button_payload(ix, EventKind::PointerUp, x, y, button);
            out.extend(self.emit(ix, EventKind::PointerUp, payload));
            // A click is a press and a release that resolve to the same handler.
            if let Some(pressed) = pressed {
                let same = self.target(pressed, EventKind::Click).map(|t| t.0) == self.target(ix, EventKind::Click).map(|t| t.0);
                if same {
                    let kind = if button == 1 { EventKind::ContextMenu } else { EventKind::Click };
                    let p = self.point_payload(ix, kind, x, y);
                    out.extend(self.emit(ix, kind, p));
                }
            }
        }
        // Capture: the press target hears `pointer_up` even if the release
        // is off it, so a slider drag can end off the track.
        if let Some(pressed) = pressed {
            let already = hit.and_then(|ix| self.target(ix, EventKind::PointerUp).map(|t| t.0));
            let captured = self.target(pressed, EventKind::PointerUp).map(|t| t.0);
            if captured.is_some() && captured != already {
                let payload = self.button_payload(pressed, EventKind::PointerUp, x, y, button);
                out.extend(self.emit(pressed, EventKind::PointerUp, payload));
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
        let Some(root) = self.session.root() else {
            return order;
        };
        let mut stack = vec![root];
        while let Some(ix) = stack.pop() {
            let Some(node) = self.session.node(ix) else {
                continue;
            };
            // 03 §3: what can be typed into can be reached with `Tab` — a
            // field, something clickable, and anything that asked for keys.
            // A pattern editor that only the mouse can focus is not one.
            let focusable = matches!(node.kind, NodeKind::Input | NodeKind::TextArea)
                || node.handler(EventKind::Click).is_some()
                || node.handler(EventKind::KeyDown).is_some()
                || node.handler(EventKind::KeyUp).is_some();
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

    /// The nearest node on the path — itself first — that asked for keys.
    fn ancestor_keyed(&self, from: NodeIx) -> Option<NodeIx> {
        let mut cur = Some(from);
        while let Some(ix) = cur {
            let node = self.session.node(ix)?;
            if node.handler(EventKind::KeyDown).is_some() || node.handler(EventKind::KeyUp).is_some() {
                return Some(ix);
            }
            cur = if node.parent.is_some() { Some(node.parent) } else { None };
        }
        None
    }

    /// Hit node under the pointer, without requiring it to be a scroller.
    fn hit_under_pointer(&mut self) -> Option<NodeIx> {
        let settled = self.pointer.over.filter(|o| self.session.node(*o).is_some());
        match settled {
            Some(o) if !self.layout_valid => Some(o),
            _ => {
                self.ensure_layout();
                self.layout.hit(&self.session, self.pointer.x, self.pointer.y)
            }
        }
    }

    /// Whether `ix` can still move in the direction `(dx, dy)`. A zero delta
    /// means "overflows at all". A nested list that cannot move must not eat
    /// the wheel: the page underneath should.
    fn scroller_accepts(&self, ix: NodeIx, dx: f32, dy: f32) -> bool {
        let Some(content) = self.layout.content_size(ix) else {
            return false;
        };
        let Some(view) = self.layout.rect(ix) else {
            return false;
        };
        let max_x = (content.w - view.w).max(0.0);
        let max_y = (content.h - view.h).max(0.0);
        let (sx, sy) = self.session.node(ix).map(|n| n.scroll).unwrap_or((0, 0));
        if dx == 0.0 && dy == 0.0 {
            return max_x > 0.5 || max_y > 0.5;
        }
        (dx < 0.0 && sx > 0) || (dx > 0.0 && (sx as f32) + 0.5 < max_x) || (dy < 0.0 && sy > 0) || (dy > 0.0 && (sy as f32) + 0.5 < max_y)
    }

    /// Nearest `scroll`/`list` ancestor of `from` that [`Self::scroller_accepts`].
    fn scroller_from(&self, from: NodeIx, dx: f32, dy: f32) -> Option<NodeIx> {
        let mut cur = Some(from);
        while let Some(ix) = cur {
            let Some(node) = self.session.node(ix) else {
                break;
            };
            if matches!(node.kind, NodeKind::Scroll | NodeKind::List) && self.scroller_accepts(ix, dx, dy) {
                return Some(ix);
            }
            cur = if node.parent.is_some() { Some(node.parent) } else { None };
        }
        None
    }

    /// Spec 03 §3: the scroller under the pointer that can still move in
    /// `(dx, dy)`, else its ancestor that can. A nested list that fits its
    /// rows must not swallow the page's wheel.
    fn scroller_under_pointer_for(&mut self, dx: f32, dy: f32) -> Option<NodeIx> {
        let hit = self.hit_under_pointer()?;
        self.ensure_layout();
        self.scroller_from(hit, dx, dy)
    }

    /// The scroller whose scrollbar strip the pointer is in, if the hit node
    /// is inside a scroller that overflows and `x` lies in its right strip.
    fn scroller_strip_at(&self, hit: NodeIx, x: f32) -> Option<NodeIx> {
        let scroller = self.ancestor_where(hit, |k| matches!(k, NodeKind::Scroll | NodeKind::List))?;
        let rect = self.layout.rect(scroller)?;
        let content = self.layout.content_size(scroller)?;
        (content.h > rect.h + 0.5 && x >= rect.x + rect.w - SCROLLBAR_WIDTH).then_some(scroller)
    }

    /// Move the thumb so the pointer keeps its grip on it.
    fn drag_thumb(&mut self, scroller: NodeIx, grip: f32, y: f32) -> Vec<Frame> {
        let rect = self.layout.rect(scroller).unwrap_or_default();
        let content = self.layout.content_size(scroller).unwrap_or_default();
        let track = (rect.h - 4.0).max(1.0);
        let len = (track * rect.h / content.h.max(1.0)).max(24.0).min(track);
        let travel = (track - len).max(1.0);
        let max = (content.h - rect.h).max(0.0);
        let target = ((y - grip - rect.y - 2.0) / travel * max).clamp(0.0, max);
        let sy = self.session.node(scroller).map_or(0, |n| n.scroll.1);
        self.scroll_by(scroller, 0.0, target - sy as f32)
    }

    /// Scroll `scroller` by a delta at once, clamped; reports the offset.
    fn scroll_by(&mut self, scroller: NodeIx, dx: f32, dy: f32) -> Vec<Frame> {
        self.scroll_anim = None;
        let (sx, sy) = self.session.node(scroller).map(|n| n.scroll).unwrap_or((0, 0));
        let content = self.layout.content_size(scroller).unwrap_or_default();
        let view = self.layout.rect(scroller).unwrap_or_default();
        let max_x = (content.w - view.w).max(0.0) as i64;
        let max_y = (content.h - view.h).max(0.0) as i64;
        let (sx, sy) = (sx.clamp(0, max_x), sy.clamp(0, max_y));
        let nx = (sx + dx.round() as i64).clamp(0, max_x);
        let ny = (sy + dy.round() as i64).clamp(0, max_y);
        if (nx, ny) == (sx, sy) {
            return Vec::new();
        }
        self.session.set_scroll(scroller, nx, ny);
        self.invalidate();
        self.scroll_touched = Some(Instant::now());
        self.emit(scroller, EventKind::Scroll, Value::List(vec![Value::Int(nx), Value::Int(ny)]))
    }

    /// Spec 03 §3: `ArrowUp`/`ArrowDown` land on the previous/next row of a
    /// list (a 40 px step where there are no rows), `PageUp`/`PageDown` move
    /// a viewport, `Home`/`End` the whole way — on the scroller under the
    /// pointer, else the focused node's, else the page's first. `None` when
    /// there is nothing to scroll.
    fn scroll_key(&mut self, key: &str) -> Option<Vec<Frame>> {
        let dy = match key {
            "ArrowDown" | "PageDown" | "End" => 1.0,
            "ArrowUp" | "PageUp" | "Home" => -1.0,
            _ => return None,
        };
        self.ensure_layout();
        let scroller = self.scroller_under_pointer_for(0.0, dy).or_else(|| self.focused.and_then(|f| self.scroller_from(f, 0.0, dy))).or_else(|| {
            let root = self.session.root()?;
            self.session.preorder(root).find(|ix| self.session.node(*ix).is_some_and(|n| matches!(n.kind, NodeKind::Scroll | NodeKind::List)) && self.scroller_accepts(*ix, 0.0, dy))
        })?;
        let view = self.layout.rect(scroller)?;
        let content = self.layout.content_size(scroller)?;
        let max_y = (content.h - view.h).max(0.0);
        let here = (self.session.node(scroller).map_or(0, |n| n.scroll.1) as f32).clamp(0.0, max_y);
        // Presses chain onto a scroll in flight, as wheel notches do.
        let base = self.scroll_anim.filter(|a| a.node == scroller).map_or(here, |a| a.to.1);
        // Rows to land on: a virtualised list's own, else the scroller's
        // laid-out children — but only where there are rows to speak of. A
        // page whose content is a single column has exactly one "row top",
        // at 0, and an `ArrowUp` that honoured it would be `Home`: that is
        // not a row above, it is the beginning of the document.
        let mut tops: Vec<f32> = match self.layout.row_tops(scroller) {
            Some(t) => t.to_vec(),
            None => self.session.children(scroller).iter().filter_map(|c| self.layout.rect(*c)).map(|r| r.y - view.y + here).collect(),
        };
        if tops.len() < 2 {
            tops.clear();
        }
        // And a row is only the next one if it is one press away: an arrow
        // never travels further than `PageUp` would, so a row beyond a
        // viewport — a card three screens tall — is the plain step instead.
        let near = |t: f32| (t - base).abs() <= view.h;
        let target = match key {
            "ArrowDown" => tops.iter().copied().find(|t| *t > base + 0.5).filter(|t| near(*t)).unwrap_or(base + 40.0),
            "ArrowUp" => tops.iter().rev().copied().find(|t| *t < base - 0.5).filter(|t| near(*t)).unwrap_or(base - 40.0),
            "PageDown" => base + view.h,
            "PageUp" => base - view.h,
            "Home" => 0.0,
            "End" => max_y,
            _ => return None,
        };
        Some(self.ease_to(scroller, target.clamp(0.0, max_y)))
    }

    /// Ease `scroller` to a vertical offset over `motion.slow`, in and out,
    /// from wherever a scroll in flight has got to: a key press reads as
    /// the view settling on the row, not snapping to it.
    fn ease_to(&mut self, scroller: NodeIx, target_y: f32) -> Vec<Frame> {
        let content = self.layout.content_size(scroller).unwrap_or_default();
        let view = self.layout.rect(scroller).unwrap_or_default();
        let (max_x, max_y) = ((content.w - view.w).max(0.0), (content.h - view.h).max(0.0));
        let (sx, sy) = self.session.node(scroller).map(|n| n.scroll).unwrap_or((0, 0));
        let here = ((sx as f32).clamp(0.0, max_x), (sy as f32).clamp(0.0, max_y));
        let in_flight = self.scroll_anim.filter(|a| a.node == scroller);
        let from = in_flight.map_or(here, |a| a.at(self.now));
        let to = (from.0, target_y.clamp(0.0, max_y));
        if (to.1 - from.1).abs() < 0.5 {
            return Vec::new();
        }
        // From rest, the view departs and arrives gently over motion.slow.
        // A press that lands while the view is already moving — a key held
        // down — must not start it over from rest each time: it keeps the
        // momentum, easing out to the new row over motion.base like a wheel
        // notch, so a burst of presses reads as one continuous glide.
        let (smooth, ms) = if in_flight.is_some() { (false, self.resolved.motion.get(1).copied().unwrap_or(180)) } else { (true, self.resolved.motion.get(2).copied().unwrap_or(320)) };
        self.scroll_anim = Some(ScrollAnim { smooth, node: scroller, from, to, start: self.now, duration: Duration::from_millis(u64::from(ms)) });
        self.next_due = Some(self.now);
        self.redraw = true;
        Vec::new()
    }

    /// The pointer's shape over what it is on: the nearest ancestor's
    /// `cursor` style if any names one, else a text beam over an editable
    /// node, else a hand over anything with a `click` handler, else the
    /// arrow — and always the arrow on a scrollbar.
    pub fn cursor(&self) -> Cursor {
        if self.pointer.dragging_thumb.is_some() || self.pointer.over_scrollbar.is_some() {
            return Cursor::Default;
        }
        let Some(over) = self.pointer.over else {
            return Cursor::Default;
        };
        let mut cur = Some(over);
        let mut first = true;
        while let Some(ix) = cur {
            let Some(node) = self.session.node(ix) else {
                break;
            };
            let styled = self.session.style_of(ix).cursor;
            if styled != Cursor::Default {
                return styled;
            }
            if first && self.is_editable(ix) {
                return Cursor::Text;
            }
            if node.handler(EventKind::Click).is_some() {
                return Cursor::Pointer;
            }
            first = false;
            cur = if node.parent.is_some() { Some(node.parent) } else { None };
        }
        Cursor::Default
    }

    /// The viewer's palette mode changed — from the window, or from a local
    /// handler's `theme` statement: styles re-resolve, everything relays
    /// out, and the server learns the new viewport.
    fn set_mode(&mut self, mode: ThemeMode) -> Vec<Frame> {
        if self.viewer.mode == mode {
            return Vec::new();
        }
        self.viewer.mode = mode;
        self.resolve_theme();
        self.layout.invalidate_all();
        self.invalidate();
        vec![Frame::Viewport(self.viewport())]
    }

    /// Spec 05 §5: the viewer's desktop palette, followed. `mode` is the
    /// palette's own light/dark, which the viewer takes; `colors` its values
    /// by role, applied in that mode — the other mode is the theme's own,
    /// so the application's light/dark switch still does something. Both
    /// empty means the desktop has none and the theme's colours return.
    pub fn set_desktop_theme(&mut self, mode: Option<ThemeMode>, colors: Vec<(eui_theme::Role, u32)>) -> Vec<Frame> {
        self.touched = true;
        let same = self.desktop_colors == colors && self.desktop_mode == mode && mode.map_or(true, |m| m == self.viewer.mode);
        if same {
            return Vec::new();
        }
        self.desktop_colors = colors;
        self.desktop_mode = mode;
        let mode_changed = mode.is_some_and(|m| m != self.viewer.mode);
        if let Some(m) = mode {
            self.viewer.mode = m;
        }
        self.resolve_theme();
        self.layout.invalidate_all();
        self.invalidate();
        if mode_changed {
            vec![Frame::Viewport(self.viewport())]
        } else {
            Vec::new()
        }
    }

    /// Resolve the theme for the viewer, the desktop's colours on top when
    /// the viewer is in the desktop's mode.
    fn resolve_theme(&mut self) {
        self.resolved = self.theme.resolve(self.viewer);
        if self.desktop_mode == Some(self.viewer.mode) {
            self.resolved.apply_overrides(&self.desktop_colors);
        }
    }

    /// A notched wheel: 100 logical px per notch — what browsers scroll per
    /// click of a wheel — eased out over `motion.fast` so a step starts at
    /// once and settles quickly. Notches accumulate onto the running target,
    /// so a fast spin covers ground without waiting for each step to land.
    fn wheel_step(&mut self, lines_x: f32, lines_y: f32) -> Vec<Frame> {
        let Some(scroller) = self.scroller_under_pointer_for(lines_x, lines_y) else {
            return Vec::new();
        };
        let content = self.layout.content_size(scroller).unwrap_or_default();
        let view = self.layout.rect(scroller).unwrap_or_default();
        let (max_x, max_y) = ((content.w - view.w).max(0.0), (content.h - view.h).max(0.0));
        let (sx, sy) = self.session.node(scroller).map(|n| n.scroll).unwrap_or((0, 0));
        let here = ((sx as f32).clamp(0.0, max_x), (sy as f32).clamp(0.0, max_y));
        let (from, base) = match self.scroll_anim.filter(|a| a.node == scroller) {
            Some(a) => (a.at(self.now), a.to),
            None => (here, here),
        };
        let to = ((base.0 + lines_x * 100.0).clamp(0.0, max_x), (base.1 + lines_y * 100.0).clamp(0.0, max_y));
        trace(|| format!("wheel step {lines_x},{lines_y}: {from:?} -> {to:?} (max {max_x},{max_y})"));
        if to == from {
            return Vec::new();
        }
        let ms = self.resolved.motion.first().copied().unwrap_or(100);
        self.scroll_anim = Some(ScrollAnim { smooth: false, node: scroller, from, to, start: self.now, duration: Duration::from_millis(u64::from(ms)) });
        self.next_due = Some(self.now);
        self.redraw = true;
        Vec::new()
    }

    fn wheel(&mut self, dx: f32, dy: f32) -> Vec<Frame> {
        // A Magic Mouse on Wayland interleaves each notch with a stream of
        // zero-valued pixel events; they must not cancel the notch's motion.
        if dx == 0.0 && dy == 0.0 {
            return Vec::new();
        }
        self.scroll_anim = None;
        let Some(scroller) = self.scroller_under_pointer_for(dx, dy) else {
            return Vec::new();
        };
        // Whole pixels move the view; the fraction waits for the next event.
        let (ax, ay) = (self.wheel_rest.0 + dx, self.wheel_rest.1 + dy);
        // Ten events of 0.7 px are 7 px, not 6.999: snap before truncating.
        let whole = |v: f32| {
            if (v - v.round()).abs() < 1e-3 {
                v.round()
            } else {
                v.trunc()
            }
        };
        let (dx, dy) = (whole(ax), whole(ay));
        self.wheel_rest = (ax - dx, ay - dy);
        trace(|| format!("wheel {dx},{dy} (rest {:.2},{:.2})", self.wheel_rest.0, self.wheel_rest.1));
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
        self.scroll_touched = Some(Instant::now());
        self.invalidate();
        self.emit(scroller, EventKind::Scroll, Value::List(vec![Value::Int(nx), Value::Int(ny)]))
    }

    fn is_editable(&self, ix: NodeIx) -> bool {
        matches!(self.session.node(ix).map(|n| n.kind), Some(NodeKind::Input | NodeKind::TextArea))
    }

    /// The edit of an editable node, seeded from its text on first use with
    /// the caret at the end.
    fn edit_mut(&mut self, f: NodeIx) -> Option<&mut Edit> {
        if !self.is_editable(f) {
            return None;
        }
        let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
        let seed = self.session.text_of(f).unwrap_or("").to_owned();
        let len = seed.len();
        Some(self.edits.entry(id).or_insert_with(|| Edit { seed: seed.clone(), value: seed, caret: len, anchor: len, scroll_x: 0.0 }))
    }

    fn text_input(&mut self, t: &str) -> Vec<Frame> {
        let Some(f) = self.focused else {
            return Vec::new();
        };
        let Some(edit) = self.edit_mut(f) else {
            return Vec::new();
        };
        edit.insert(t);
        self.show_edit(f);
        self.emit(f, EventKind::TextInput, Value::Str(t.to_owned()))
    }

    /// The byte offset in `e`'s value under the pointer, from the shaped text.
    fn byte_at_pointer(&mut self, e: NodeIx, x: f32, y: f32) -> Option<usize> {
        if !self.preedit.is_empty() {
            return None;
        }
        let rect = self.layout.rect(e)?;
        let style = eui_layout::Style::resolve(&self.session.style_of(e), &self.resolved);
        let text = self.session.text_of(e).unwrap_or("").to_owned();
        let scroll_x = self.edits.get(&self.session.node(e)?.id).map_or(0.0, |ed| ed.scroll_x);
        let shaped = self.text.shape(&text, style.font, Some((rect.w - style.inset_h()).max(0.0)), style.line_clamp);
        let lx = x - (rect.x + style.border.l + style.padding.l) + scroll_x;
        let ly = y - (rect.y + style.border.t + style.padding.t);
        Some(shaped.byte_at(lx, ly).min(text.len()))
    }

    /// Text the person copied or cut since the last call, for the clipboard.
    pub fn take_clipboard(&mut self) -> Option<String> {
        self.clipboard.take()
    }

    /// What the painter needs to draw the focused field's caret and
    /// selection, with the scroll that keeps the caret in view — updated
    /// here, once per paint.
    fn editing(&mut self) -> Option<Editing> {
        let f = self.focused.filter(|f| self.is_editable(*f))?;
        let rect = self.layout.rect(f)?;
        let style = eui_layout::Style::resolve(&self.session.style_of(f), &self.resolved);
        let text = self.session.text_of(f).unwrap_or("").to_owned();
        let shaped = self.text.shape(&text, style.font, Some((rect.w - style.inset_h()).max(0.0)), style.line_clamp);
        let pre = self.preedit.len();
        let id = self.session.node(f)?.id;
        let edit = self.edits.get_mut(&id)?;
        let shown = |o: usize| {
            if o > edit.caret {
                o.saturating_add(pre)
            } else {
                o
            }
        };
        let caret = edit.caret.saturating_add(pre);
        let inner_w = (rect.w - style.inset_h()).max(0.0);
        let cx = shaped.caret(caret).0;
        if cx - edit.scroll_x > inner_w {
            edit.scroll_x = cx - inner_w;
        } else if cx < edit.scroll_x {
            edit.scroll_x = cx;
        }
        if shaped.metrics.width <= inner_w {
            edit.scroll_x = 0.0;
        }
        let r = edit.selection();
        Some(Editing { node: f, start: shown(r.start), end: shown(r.end), caret, scroll_x: edit.scroll_x })
    }

    /// Spec 06 §3: a composition is local. The field shows its buffer plus
    /// the preedit; nothing leaves the client until the method commits.
    fn preedit(&mut self, t: String) {
        let Some(f) = self.focused.filter(|f| self.is_editable(*f)) else {
            return;
        };
        self.preedit = t;
        let _ = self.edit_mut(f);
        self.show_edit(f);
    }

    /// Put the field's value, with any composition at the caret, into the tree.
    fn show_edit(&mut self, f: NodeIx) {
        let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
        let Some(edit) = self.edits.get(&id) else {
            return;
        };
        let mut value = edit.value.clone();
        value.insert_str(edit.caret.min(value.len()), &self.preedit);
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
        // Spec 03 §3: the scrolling keys belong to the client only while
        // nothing that wants keys has focus. A field has them; so does a
        // pattern editor, a grid or a game that asked for `key_down` — an
        // application that cannot use the arrows is not much of one, and a
        // scroller anywhere in the tree used to be enough to take them.
        let claimed = self.focused.is_some_and(|f| self.is_editable(f) || self.ancestor_keyed(f).is_some());
        if down && modifiers & 0b1110 == 0 && matches!(key, "ArrowUp" | "ArrowDown" | "PageUp" | "PageDown" | "Home" | "End") && !claimed {
            if let Some(out) = self.scroll_key(key) {
                return out;
            }
        }
        let Some(f) = self.focused else {
            return Vec::new();
        };
        if key == "Escape" {
            return if down { self.set_focus(None, false) } else { Vec::new() };
        }
        let mut out = Vec::new();
        if down {
            let editable = self.is_editable(f);
            match key {
                _ if editable && self.edit_key(f, key, modifiers) => {}
                "Enter" if self.session.node(f).map(|n| n.kind) == Some(NodeKind::Input) => {
                    out.extend(self.commit_edit(f));
                    out.extend(self.emit(f, EventKind::Submit, Value::Null));
                }
                // A node that handles keys is not activated by `Enter` or
                // `Space`: it asked for the keys, and in a tracker `Space`
                // is what starts the song, not a click on the pattern.
                "Enter" | " " if !editable && self.session.node(f).map_or(true, |n| n.handler(EventKind::KeyDown).is_none()) => {
                    out.extend(self.activate(f));
                }
                _ => {}
            }
        }
        let kind = if down { EventKind::KeyDown } else { EventKind::KeyUp };
        out.extend(self.emit(f, kind, Value::List(vec![Value::Str(key.to_owned()), Value::Int(i64::from(modifiers))])));
        out
    }

    /// Spec 03 §3: the caret, the selection and the clipboard belong to the
    /// client. True when the key was an editing key and has been applied.
    fn edit_key(&mut self, f: NodeIx, key: &str, modifiers: u32) -> bool {
        let (shift, ctrl) = (modifiers & 1 != 0, modifiers & (2 | 8) != 0);
        let multiline = self.session.node(f).map(|n| n.kind) == Some(NodeKind::TextArea);
        let Some(edit) = self.edit_mut(f) else {
            return false;
        };
        let mut copied = None;
        match key {
            "Backspace" => edit.delete(false),
            "Delete" => edit.delete(true),
            "ArrowLeft" => {
                let r = edit.selection();
                let at = if ctrl {
                    word_left(&edit.value, edit.caret)
                } else if !shift && !r.is_empty() {
                    r.start
                } else {
                    prev_char(&edit.value, edit.caret)
                };
                edit.place(at, shift);
            }
            "ArrowRight" => {
                let r = edit.selection();
                let at = if ctrl {
                    word_right(&edit.value, edit.caret)
                } else if !shift && !r.is_empty() {
                    r.end
                } else {
                    next_char(&edit.value, edit.caret)
                };
                edit.place(at, shift);
            }
            "Home" => {
                let at = if ctrl { 0 } else { line_bounds(&edit.value, edit.caret).0 };
                edit.place(at, shift);
            }
            "End" => {
                let at = if ctrl { edit.value.len() } else { line_bounds(&edit.value, edit.caret).1 };
                edit.place(at, shift);
            }
            "a" | "A" if ctrl => {
                edit.anchor = 0;
                edit.caret = edit.value.len();
            }
            "c" | "C" if ctrl => {
                let r = edit.selection();
                if !r.is_empty() {
                    copied = Some(edit.value[r].to_owned());
                }
            }
            "x" | "X" if ctrl => {
                let r = edit.selection();
                if !r.is_empty() {
                    copied = Some(edit.value[r].to_owned());
                    edit.delete(false);
                }
            }
            "Enter" if multiline => edit.insert("\n"),
            _ => return false,
        }
        if copied.is_some() {
            self.clipboard = copied;
        }
        self.show_edit(f);
        true
    }

    /// `change`, if the field's value differs from what the server has.
    fn commit_edit(&mut self, f: NodeIx) -> Vec<Frame> {
        let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
        let Some(edit) = self.edits.get_mut(&id) else {
            return Vec::new();
        };
        if edit.value == edit.seed {
            return Vec::new();
        }
        edit.seed = edit.value.clone();
        let value = edit.value.clone();
        self.emit(f, EventKind::Change, Value::Str(value))
    }

    /// Spec 01 §4: when a session ends, say so on the glass.
    ///
    /// A window that stopped talking to its application must not look like
    /// one that is merely idle — that is a person clicking at a picture. So
    /// the last tree is replaced, once, by a small one the client mounts
    /// itself: what stopped, and why. The session is a fresh one, so the
    /// notice cannot collide with the ids the server had defined, and what
    /// the old tree was driving — sound, pictures, clocks, focus — goes
    /// with it.
    fn show_stopped(&mut self) {
        if self.stopped || self.closed.is_none() {
            return;
        }
        self.stopped = true;
        let why = self.closed.as_ref().map(ToString::to_string).unwrap_or_default();
        self.mixer = eui_audio::Mixer::new(48_000);
        self.players.clear();
        self.wakes.clear();
        self.anims.clear();
        self.edits.clear();
        self.windows.clear();
        self.focused = None;
        self.pointer = Pointer::default();
        self.scroll_anim = None;
        self.next_due = None;
        self.session = Session::new();
        let role = |r: eui_theme::Role| ColorRef::role(r.id());
        let page = StyleRecord {
            display: Display::Column,
            justify: Justify::Center,
            align_items: AlignItems::Center,
            gap: 3,
            padding: [6, 6, 6, 6],
            bg: role(eui_theme::Role::SurfaceBase),
            ..Default::default()
        };
        let heading = StyleRecord { font_size: 4, font_weight: FontWeight::Bold, fg: role(eui_theme::Role::TextDefault), ..Default::default() };
        let reason = StyleRecord { font_size: 1, fg: role(eui_theme::Role::TextMuted), text_align: TextAlign::Center, max_width: Dim::Px(420), ..Default::default() };
        let mut tree = Subtree::default();
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 2, key: 0, text: Some(TextRef::Inline("The application stopped".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 3, style: 3, key: 0, text: Some(TextRef::Inline(why)), props: (0, 0), handlers: (0, 0), child_count: 0 });
        let batch = Batch { seq: 1, ops: vec![Op::DefStyle { id: 1, record: page }, Op::DefStyle { id: 2, record: heading }, Op::DefStyle { id: 3, record: reason }, Op::Mount(tree)] };
        if self.session.apply(&batch).is_err() {
            return;
        }
        self.invalidate();
        self.redraw = true;
    }

    // --------------------------------------------------------------- paint

    /// Lay out if needed and produce this frame's draw list for a
    /// `w × h` device-pixel target. Clears the redraw flag.
    pub fn paint(&mut self, device_w: u32, device_h: u32) -> DrawList {
        // A frame owed to a spin alone, with nothing having reached the
        // driver since the last one, is the last list again: the vertex
        // stage turns the node from the clock the window passes it (03 §5),
        // so walking the tree would produce the same quads. Where the
        // driver shares the window's process — every platform but Linux —
        // this is what keeps a spinner from costing a layout and a paint
        // thirty times a second.
        if !self.touched {
            if let Some(list) = &self.spin_list {
                self.redraw = false;
                self.next_due = Some(self.now + SPIN_FRAME);
                self.spin_repeats = self.spin_repeats.saturating_add(1);
                return list.clone();
            }
        }
        self.show_stopped();
        self.note_style_changes();
        self.note_entrances();
        // Spec 03 §7 and §8: the tree says what should be playing, and a
        // picture that just decoded has a size the layout must know before
        // it measures anything.
        self.sync_audio();
        self.sync_video();
        let moved = self.advance_videos();
        self.pending.extend(moved);
        // A scroll in flight moves the view before layout; what it emits when
        // it lands is picked up by the next input or frame turn.
        let landed = self.advance_scroll();
        self.pending.extend(landed);
        let t_layout = Instant::now();
        let relaid = !self.layout_valid;
        // A hover left for this frame is settled on a fresh layout first;
        // what its local handlers restyle is laid out again below — cheap,
        // since only the restyled nodes lost their memoised measures.
        if self.pointer.hover_pending {
            self.ensure_layout();
            let (x, y) = (self.pointer.x, self.pointer.y);
            let settled = self.hover(x, y);
            self.pending.extend(settled);
        }
        self.ensure_layout();
        self.follow_slider_drag();
        let now = self.now;
        if let Some(due) = self.viewport_due {
            if now >= due {
                self.viewport_due = None;
                self.pending.push(Frame::Viewport(self.viewport()));
            }
        }
        let ticks = self.time_updates();
        self.pending.extend(ticks);
        let woken = self.wake_events();
        self.pending.extend(woken);
        // Spec 04 §7.1: a windowed list whose visible rows changed asks for
        // them — once the view has been still for a moment, not per frame
        // of a glide or a drag: a request a frame is a server render a
        // frame, and rows that will be scrolled past before they arrive.
        let settled = self.scroll_anim.is_none() && self.scroll_touched.map_or(true, |t| now.saturating_duration_since(t) >= WINDOW_SETTLE);
        let mut settle_due = None;
        if settled {
            let asked = self.window_events();
            self.pending.extend(asked);
        } else {
            // Still moving. A drag that has outrun the rows it holds would
            // show placeholders until it stopped; it asks for the rows it
            // is about to need instead, throttled, and the settle below
            // then finds nothing new to ask.
            let outran = self.window_outrun_events(now);
            self.pending.extend(outran);
            if self.scroll_anim.is_none() && !self.layout.windowed_lists().is_empty() {
                // Come back when it has.
                settle_due = Some(self.scroll_touched.map_or(now, |t| t + WINDOW_SETTLE));
            }
        }
        let layout_ms = t_layout.elapsed().as_secs_f64() * 1e3;
        self.redraw = false;
        let now = self.now;
        let overrides: Vec<(NodeIx, Colors)> = self.anims.iter().map(|(ix, a)| (*ix, a.at(now))).collect();
        let editing = self.editing();
        trace(|| format!("paint: focused={:?} editing={editing:?}", self.focused.and_then(|f| self.session.node(f)).map(|n| n.id)));
        let mut list = paint(&mut Scene {
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
            editing,
            now: self.now.saturating_duration_since(self.epoch).as_secs_f32(),
            scrollbar_hot: self.pointer.dragging_thumb.map(|(s, _)| s).or(self.pointer.over_scrollbar),
        });
        if list.wants_frame && self.next_due.is_none() {
            self.next_due = Some(now + SPIN_FRAME);
        }
        self.session.clear_all_dirty();
        trace(|| {
            let st = self.layout.stats();
            format!(
                "paint: layout {layout_ms:.1} ms (relaid {relaid}, {} measures, {} memo hits, {} rows measured), paint {:.1} ms, text cache misses {}",
                st.measures,
                st.memo_hits,
                st.rows_measured,
                t_layout.elapsed().as_secs_f64() * 1e3 - layout_ms,
                self.text.stats().misses
            )
        });
        // A finished transition painted its final colours this frame.
        self.anims.retain(|(ix, a)| !a.done(now) && self.session.node(*ix).is_some());
        self.next_due = if self.anims.is_empty() && self.scroll_anim.is_none() && !list.wants_frame {
            None
        } else if self.scroll_anim.is_some() {
            // A glide at sixty, not a hundred and twenty-five. Every frame
            // of it lays the page out again — the rows a windowed list
            // shows move with the offset — and presents; on a display that
            // refreshes at 120 Hz an 8 ms cadence asked for both twice as
            // often as the eye needs, and a glide through the docs dialog
            // was a fifth of a core on macOS.
            Some(now + Duration::from_millis(16))
        } else if self.anims.is_empty() {
            // Only a spin: half the frames a transition gets. A revolution
            // is 1.2 s (03 §5), which is 12° a frame at thirty — smooth —
            // and thirty frames is half the work of sixty, on a display
            // that would otherwise be asked for a hundred and twenty.
            Some(now + SPIN_FRAME)
        } else {
            Some(now + Duration::from_millis(16))
        };
        let wake_due = self.wakes.iter().map(|(_, _, at)| *at).min();
        let others = [settle_due, self.video_due, self.viewport_due, wake_due];
        // A spin is the only thing owed when nothing else is: no transition
        // to interpolate, no scroll to carry, no timer about to fire. The
        // angle is not in the list -- the vertex stage takes it from the
        // clock -- so the next frame is this list again, and the window can
        // draw it without asking for it. Anything it sends the driver puts
        // an end to that, because it may change what the tree paints.
        list.spin_only = list.wants_frame && self.anims.is_empty() && self.scroll_anim.is_none() && others.iter().all(Option::is_none);
        for due in others.into_iter().flatten() {
            self.next_due = Some(self.next_due.map_or(due, |d| d.min(due)));
        }
        self.spin_list = if list.spin_only { Some(list.clone()) } else { None };
        self.touched = false;
        list
    }

    /// Spec 03 §8: look at the tree's `video` nodes and make the players
    /// agree with what they say. Walks the tree only after something
    /// changed it, like [`Self::sync_audio`].
    fn sync_video(&mut self) {
        if !self.video_dirty {
            return;
        }
        self.video_dirty = false;
        let Some(root) = self.session.root() else {
            self.players.clear();
            return;
        };
        let atom = |name: &str| self.session.atom_id(name);
        let (a_src, a_playing, a_loop, a_position) = (atom("src"), atom("playing"), atom("loop"), atom("position"));
        let mut live: Vec<u32> = Vec::new();
        let mut work: Vec<(u32, Hash, bool, bool, Option<i64>)> = Vec::new();
        for ix in self.session.preorder(root) {
            let Some(node) = self.session.node(ix) else {
                continue;
            };
            if node.kind != NodeKind::Video {
                continue;
            }
            let prop = |a: Option<u32>| a.and_then(|a| node.prop(a));
            let Some(Value::Asset(hash)) = prop(a_src) else {
                continue;
            };
            live.push(node.id);
            work.push((
                node.id,
                *hash,
                matches!(prop(a_playing), Some(Value::Bool(true))),
                matches!(prop(a_loop), Some(Value::Bool(true))),
                match prop(a_position) {
                    Some(Value::Int(ms)) => Some(*ms),
                    _ => None,
                },
            ));
        }
        self.players.retain(|id, _| live.contains(id));
        for (id, hash, playing, looping, position) in work {
            let Some(movie) = self.movie(&hash) else {
                continue;
            };
            let entry = self.players.entry(id).or_insert_with(|| (hash, eui_video::Player::new()));
            // A node pointed at another picture starts that one over.
            if entry.0 != hash {
                *entry = (hash, eui_video::Player::new());
            }
            entry.1.playing = playing;
            entry.1.looping = looping;
            if let Some(ms) = position {
                let seen = self.video_at.get(&id).copied();
                if seen != Some(ms) {
                    self.video_at.insert(id, ms);
                    entry.1.seek(&movie, ms.max(0) as u64);
                }
            }
        }
        self.video_at.retain(|id, _| self.players.contains_key(id));
    }

    /// The decoded picture for a hash, decoding it the first time and
    /// remembering a failure so a tree that keeps naming it does not
    /// re-decode it every frame.
    fn movie(&mut self, hash: &Hash) -> Option<Arc<eui_video::Movie>> {
        if let Some(known) = self.movies.get(hash) {
            return known.clone();
        }
        let bytes = self.assets.raw(hash)?;
        let decoded = match eui_video::decode(&bytes, None) {
            Ok(movie) => {
                trace(|| {
                    format!(
                        "video: {} decoded, {}×{}, {} frames, {} ms, {} kB",
                        crate::assets::hex(hash),
                        movie.width(),
                        movie.height(),
                        movie.frames().len(),
                        movie.duration_ms(),
                        movie.bytes() / 1024
                    )
                });
                self.video_sizes.insert(*hash, (movie.width() as f32, movie.height() as f32));
                Some(Arc::new(movie))
            }
            Err(e) => {
                eprintln!("eui: video {}: {e}", crate::assets::hex(hash));
                None
            }
        };
        self.movies.insert(*hash, decoded.clone());
        // A picture that just arrived changes what the layout measures.
        self.layout.invalidate_all();
        self.invalidate();
        decoded
    }

    /// Spec 03 §8: move every player to `now`, put the frame each one
    /// makes due into the atlas, and say when the next frame is. Returns
    /// the frames a picture's end produced.
    fn advance_videos(&mut self) -> Vec<Frame> {
        if self.players.is_empty() {
            self.video_clock = None;
            return Vec::new();
        }
        let now = self.now;
        let elapsed = self.video_clock.map_or(0, |t| now.saturating_duration_since(t).as_millis().min(u128::from(u64::MAX)) as u64);
        self.video_clock = Some(now);
        let mut out = Vec::new();
        let mut soonest: Option<u64> = None;
        let mut ids: Vec<u32> = self.players.keys().copied().collect();
        ids.sort_unstable();
        // Several nodes may name the same picture — a feed of cards with
        // one animation on them. They share the decoded frames and the
        // atlas region, so the first of them decides which frame is up;
        // otherwise they would overwrite each other's frame every paint.
        let mut uploaded: Vec<Hash> = Vec::new();
        for id in ids {
            let Some((hash, player)) = self.players.get_mut(&id) else {
                continue;
            };
            let (hash, mut player) = (*hash, player.clone());
            let Some(movie) = self.movies.get(&hash).cloned().flatten() else {
                continue;
            };
            let changed = player.advance(&movie, elapsed);
            let ended = player.take_ended();
            let index = player.index();
            if let Some(next) = player.next_frame_in_ms(&movie) {
                soonest = Some(soonest.map_or(next, |s: u64| s.min(next)));
            }
            if let Some(slot) = self.players.get_mut(&id) {
                slot.1 = player;
            }
            // The first frame of a picture is packed; the ones after it
            // overwrite the same region, so a video costs one region.
            let first_for_picture = !uploaded.contains(&hash);
            uploaded.push(hash);
            let fresh = self.framed.get(&hash) != Some(&index);
            if first_for_picture && (changed || fresh) {
                if let Some(frame) = movie.frames().get(index) {
                    let packed = self.images.get(&hash).is_some();
                    let ok = if packed { self.images.update(&hash, &frame.rgba) } else { self.images.insert(hash, movie.width(), movie.height(), &frame.rgba).is_some() };
                    if ok {
                        self.framed.insert(hash, index);
                        self.redraw = true;
                    }
                }
            }
            if ended {
                if let Some(ix) = self.session.lookup(id) {
                    out.extend(self.emit(ix, EventKind::Ended, Value::Null));
                }
            }
        }
        // Sleep exactly until the next frame is due, and not a moment less.
        self.video_due = soonest.map(|ms| now + Duration::from_millis(ms.max(1)));
        out
    }

    /// Where a video node's picture is, in milliseconds from its start.
    /// The client owns this clock; the server sees it only through
    /// `time_update`.
    pub fn video_position_ms(&self, node: u32) -> Option<u64> {
        self.players.get(&node).map(|(_, p)| p.position_ms())
    }

    /// True while any picture is loaded.
    pub fn video_playing(&self) -> bool {
        self.players.values().any(|(_, p)| p.playing)
    }

    /// Spec 03 §7: look at the tree's `audio` nodes — what they name,
    /// what they should be doing — and make the mixer agree. Cheap when
    /// nothing changed: the walk happens only after a batch or a local
    /// handler touched the tree.
    fn sync_audio(&mut self) {
        if !self.audio_dirty {
            return;
        }
        self.audio_dirty = false;
        let Some(root) = self.session.root() else {
            self.mixer.retain(&[]);
            self.audio_at.clear();
            self.audio_src.clear();
            return;
        };
        let atom = |name: &str| self.session.atom_id(name);
        let (a_src, a_playing, a_volume, a_loop, a_position) = (atom("src"), atom("playing"), atom("volume"), atom("loop"), atom("position"));
        let mut live: Vec<u32> = Vec::new();
        let mut work: Vec<(u32, Hash, Control, Option<i64>)> = Vec::new();
        for ix in self.session.preorder(root) {
            let Some(node) = self.session.node(ix) else {
                continue;
            };
            if node.kind != NodeKind::Audio {
                continue;
            }
            let prop = |a: Option<u32>| a.and_then(|a| node.prop(a));
            let Some(Value::Asset(hash)) = prop(a_src) else {
                continue;
            };
            let control = Control {
                playing: matches!(prop(a_playing), Some(Value::Bool(true))),
                volume: match prop(a_volume) {
                    Some(Value::Int(v)) => (*v as f32 / 100.0).clamp(0.0, 1.0),
                    Some(Value::Float(v)) => (*v as f32).clamp(0.0, 1.0),
                    _ => 1.0,
                },
                looping: matches!(prop(a_loop), Some(Value::Bool(true))),
            };
            let position = match prop(a_position) {
                Some(Value::Int(ms)) => Some(*ms),
                _ => None,
            };
            live.push(node.id);
            work.push((node.id, *hash, control, position));
        }
        self.mixer.retain(&live);
        self.audio_at.retain(|id, _| live.contains(id));
        self.audio_src.retain(|id, _| live.contains(id));
        for (id, hash, control, position) in work {
            // Load when the node is new to the mixer, and again when it is
            // pointed at a different asset: same node, another sound.
            if !self.mixer.has(id) || self.audio_src.get(&id) != Some(&hash) {
                match self.sound(&hash) {
                    Some(sound) => {
                        if !self.mixer.load(id, sound) {
                            trace(|| format!("audio: node {id} refused, {} sources already", self.mixer.len()));
                            continue;
                        }
                        self.audio_src.insert(id, hash);
                        // A new sound starts at its own beginning; the
                        // position the tree carries is applied below only
                        // when it changed, which a fresh source has not.
                        self.audio_at.remove(&id);
                    }
                    // Not fetched yet, or it failed: the node stays silent.
                    None => continue,
                }
            }
            self.mixer.control(id, control);
            if let Some(ms) = position {
                if self.audio_at.get(&id) != Some(&ms) {
                    self.audio_at.insert(id, ms);
                    self.mixer.seek(id, ms.max(0) as u64);
                }
            }
        }
    }

    /// The decoded sound for a hash, decoding it the first time. A sound
    /// that will not decode is remembered as such, so a tree that keeps
    /// naming it does not re-decode it every frame.
    fn sound(&mut self, hash: &Hash) -> Option<Arc<eui_audio::Sound>> {
        if let Some(known) = self.sounds.get(hash) {
            return known.clone();
        }
        let bytes = self.assets.raw(hash)?;
        let decoded = match eui_audio::decode(&bytes, None) {
            Ok(sound) => {
                trace(|| format!("audio: {} decoded, {} ms, {} kB", crate::assets::hex(hash), sound.duration_ms(), sound.bytes() / 1024));
                Some(Arc::new(sound))
            }
            Err(e) => {
                eprintln!("eui: audio {}: {e}", crate::assets::hex(hash));
                None
            }
        };
        self.sounds.insert(*hash, decoded.clone());
        // Bytes the decoder produced count against the session, like an
        // image's: a sound that will not decode costs nothing but its file.
        decoded
    }

    /// Mix the sounds that are playing into `out`, `channels` samples a
    /// frame at `rate` frames a second. Called by the window's audio
    /// thread — the only part of the driver another thread reaches — and
    /// returns the frames a source's end produced, to send.
    pub fn fill_audio(&mut self, out: &mut [f32], channels: u16, rate: u32) -> Vec<Frame> {
        if self.mixer.rate() != rate {
            self.mixer.set_rate(rate);
        }
        let ended = self.mixer.fill(out, channels);
        let mut frames = Vec::new();
        for id in ended {
            let Some(ix) = self.session.lookup(id) else {
                continue;
            };
            frames.extend(self.emit(ix, EventKind::Ended, Value::Null));
        }
        frames
    }

    /// True while any sound is playing: the window keeps its device open
    /// and its buffer fed only then.
    pub fn audio_playing(&self) -> bool {
        !self.mixer.is_empty()
    }

    /// Spec 03 §7 and §8: `time_update` for the sounds and pictures whose
    /// node asks for it, at most four a second — a progress bar's input.
    fn time_updates(&mut self) -> Vec<Frame> {
        if self.mixer.is_empty() && self.players.is_empty() {
            return Vec::new();
        }
        let now = self.now;
        if self.audio_reported.is_some_and(|t| now.saturating_duration_since(t) < Duration::from_millis(250)) {
            return Vec::new();
        }
        let playing: Vec<u32> = self.session.root().map_or_else(Vec::new, |root| {
            self.session
                .preorder(root)
                .filter_map(|ix| self.session.node(ix))
                .filter(|n| n.kind == NodeKind::Audio && n.handler(EventKind::TimeUpdate).is_some())
                .map(|n| n.id)
                .filter(|id| self.mixer.playing(*id))
                .collect()
        });
        // The same for pictures: the node asks, the client answers.
        let moving: Vec<(u32, u64, u64)> = self.session.root().map_or_else(Vec::new, |root| {
            self.session
                .preorder(root)
                .filter_map(|ix| self.session.node(ix))
                .filter(|n| n.kind == NodeKind::Video && n.handler(EventKind::TimeUpdate).is_some())
                .filter_map(|n| {
                    let (hash, player) = self.players.get(&n.id)?;
                    let movie = self.movies.get(hash)?.as_ref()?;
                    player.playing.then(|| (n.id, player.position_ms(), movie.duration_ms()))
                })
                .collect()
        });
        if playing.is_empty() && moving.is_empty() {
            return Vec::new();
        }
        self.audio_reported = Some(now);
        let mut out = Vec::new();
        for id in playing {
            let (Some(ix), Some(at), Some(len)) = (self.session.lookup(id), self.mixer.position_ms(id), self.mixer.duration_ms(id)) else {
                continue;
            };
            out.extend(self.emit(ix, EventKind::TimeUpdate, Value::List(vec![Value::Int(at as i64), Value::Int(len as i64)])));
        }
        for (id, at, len) in moving {
            let Some(ix) = self.session.lookup(id) else {
                continue;
            };
            out.extend(self.emit(ix, EventKind::TimeUpdate, Value::List(vec![Value::Int(at as i64), Value::Int(len as i64)])));
        }
        out
    }

    /// Spec 06 §1.1: the nodes that asked to be woken.
    ///
    /// A node carrying a `wake` prop (milliseconds) and a `wake` handler
    /// is sent one `wake` event every period for as long as it carries
    /// both — the only event nobody caused, and the only way an
    /// application can watch something that moves without it. The period
    /// has a floor and the count a ceiling ([`MIN_WAKE_MS`],
    /// [`MAX_WAKES`]): a server that asks for a thousand clocks gets four,
    /// slowly.
    ///
    /// A clock already running keeps its phase across re-renders: only a
    /// node that was not waking before, or whose period changed, is armed
    /// afresh.
    fn wake_events(&mut self) -> Vec<Frame> {
        if self.wake_dirty {
            self.wake_dirty = false;
            self.collect_wakes();
        }
        if self.wakes.is_empty() {
            return Vec::new();
        }
        let now = self.now;
        let mut out = Vec::new();
        let mut due: Vec<u32> = Vec::new();
        for (id, period, at) in self.wakes.iter_mut() {
            if *at <= now {
                due.push(*id);
                // From now, not from the missed instant: a window that was
                // not painted for a second does not owe five events.
                *at = now + *period;
            }
        }
        for id in due {
            let Some(ix) = self.session.lookup(id) else {
                continue;
            };
            out.extend(self.emit(ix, EventKind::Wake, Value::Null));
        }
        out
    }

    /// Walk the tree for the nodes that ask to be woken, keeping the phase
    /// of the ones already running.
    fn collect_wakes(&mut self) {
        let Some(root) = self.session.root() else {
            self.wakes.clear();
            return;
        };
        let Some(atom) = self.session.atom_id("wake") else {
            self.wakes.clear();
            return;
        };
        let now = self.now;
        let asked: Vec<(u32, Duration)> = self
            .session
            .preorder(root)
            .filter_map(|ix| self.session.node(ix))
            .filter(|n| n.handler(EventKind::Wake).is_some())
            .filter_map(|n| match n.prop(atom) {
                Some(Value::Int(ms)) if *ms > 0 => Some((n.id, Duration::from_millis((*ms as u64).max(MIN_WAKE_MS)))),
                _ => None,
            })
            .take(MAX_WAKES)
            .collect();
        let old = std::mem::take(&mut self.wakes);
        self.wakes = asked
            .into_iter()
            .map(|(id, period)| {
                let kept = old.iter().find(|(o, p, _)| *o == id && *p == period).map(|(_, _, at)| *at);
                (id, period, kept.unwrap_or(now + period))
            })
            .collect();
    }

    /// Spec 04 §7.1: for every windowed list laid out this frame, the rows
    /// in view plus a viewport of margin; emitted as `window` when the
    /// range differs from the one last reported for that node.
    fn window_events(&mut self) -> Vec<Frame> {
        let lists = self.layout.windowed_lists().to_vec();
        if lists.is_empty() && self.windows.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut seen = Vec::with_capacity(lists.len());
        for ix in lists {
            let Some(node) = self.session.node(ix) else {
                continue;
            };
            let (id, sy) = (node.id, node.scroll.1 as f32);
            let Some(range) = self.layout.row_window(ix, sy) else {
                continue;
            };
            seen.push(id);
            if self.windows.get(&id) == Some(&range) {
                continue;
            }
            self.windows.insert(id, range);
            out.extend(self.emit(ix, EventKind::Window, Value::List(vec![Value::Int(i64::from(range.0)), Value::Int(i64::from(range.1))])));
        }
        self.windows.retain(|id, _| seen.contains(id));
        out
    }

    /// Spec 04 §7.1, while the view is moving: for each windowed list whose
    /// rows within half a viewport of the view are not all among those last
    /// asked for, ask for the full window around it now — at most once per
    /// [`WINDOW_OUTRUN`]. The range is recorded as asked, so the settle that
    /// follows the drag repeats nothing.
    fn window_outrun_events(&mut self, now: Instant) -> Vec<Frame> {
        if self.outrun_at.is_some_and(|t| now.saturating_duration_since(t) < WINDOW_OUTRUN) {
            return Vec::new();
        }
        let lists = self.layout.windowed_lists().to_vec();
        let mut out = Vec::new();
        for ix in lists {
            let Some(node) = self.session.node(ix) else {
                continue;
            };
            let (id, sy) = (node.id, node.scroll.1 as f32);
            let Some(near) = self.layout.row_span(ix, sy, 0.5, 0.5) else {
                continue;
            };
            if self.windows.get(&id).is_some_and(|(a, b)| near.0 >= *a && near.1 <= *b) {
                continue;
            }
            let Some(range) = self.layout.row_window(ix, sy) else {
                continue;
            };
            self.windows.insert(id, range);
            self.outrun_at = Some(now);
            out.extend(self.emit(ix, EventKind::Window, Value::List(vec![Value::Int(i64::from(range.0)), Value::Int(i64::from(range.1))])));
        }
        out
    }

    /// How many frames were answered from the last spin-only list rather
    /// than painted.
    pub fn spin_repeats(&self) -> u64 {
        self.spin_repeats
    }

    /// Frames a paint produced — a scroll that landed reports its offset —
    /// for the window to send after drawing.
    pub fn take_pending(&mut self) -> Vec<Frame> {
        std::mem::take(&mut self.pending)
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
    /// What to put back if the server's answer does not confirm it: the
    /// effects of a `LocalThenServer` chunk are provisional (07 §6).
    undo: Option<Vec<Undo>>,
    /// A `set_mode` the chunk asked for, applied by the driver after the run.
    mode: Option<String>,
}

/// One provisional change, with the value it replaced.
#[derive(Debug, Clone)]
enum Undo {
    Style(NodeIx, u32),
    Text(NodeIx, Option<TextRef>),
    Prop(NodeIx, u32, Option<Value>),
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
        if let (Some(undo), Some(root)) = (&mut self.undo, self.session.root()) {
            undo.push(Undo::Prop(root, atom, self.session.root_prop(atom).cloned()));
        }
        self.session.set_root_prop_local(atom, to_wire(value))
    }
    fn set_text(&mut self, key: u32, text: String) -> bool {
        let Some(ix) = self.session.lookup_key(key) else {
            return false;
        };
        self.touched = true;
        if let Some(undo) = &mut self.undo {
            undo.push(Undo::Text(ix, self.session.node(ix).and_then(|n| n.text.clone())));
        }
        self.session.set_text_local(ix, text)
    }
    fn set_prop(&mut self, key: u32, atom: u32, value: eui_vm::Value) -> bool {
        let Some(ix) = self.session.lookup_key(key) else {
            return false;
        };
        self.touched = true;
        if let Some(undo) = &mut self.undo {
            let old = self.session.node(ix).and_then(|n| n.props.iter().find(|(a, _)| *a == atom).map(|(_, v)| v.clone()));
            undo.push(Undo::Prop(ix, atom, old));
        }
        self.session.set_prop_local(ix, atom, to_wire(value))
    }
    fn set_style(&mut self, key: u32, style: u32) -> bool {
        let Some(ix) = self.session.lookup_key(key) else {
            return false;
        };
        self.touched = true;
        if let Some(undo) = &mut self.undo {
            undo.push(Undo::Style(ix, self.session.node(ix).map_or(0, |n| n.style)));
        }
        self.session.set_style_local(ix, style)
    }
    fn emit(&mut self, atom: u32) {
        self.emitted.push(atom);
    }
    fn set_mode(&mut self, mode: &str) -> bool {
        if !matches!(mode, "light" | "dark" | "high_contrast" | "toggle") {
            return false;
        }
        self.mode = Some(mode.to_owned());
        true
    }
}

/// Layout's view of text and assets: shaping from the text engine, image
/// sizes from the store. An image not yet fetched has no size, and gets one
/// the moment it arrives.
struct Measurer<'a> {
    text: &'a mut TextEngine,
    assets: &'a AssetStore,
    videos: &'a HashMap<Hash, (f32, f32)>,
}

impl TextMeasurer for Measurer<'_> {
    fn measure(&mut self, text: &str, font: FontSpec, max_width: Option<f32>, line_clamp: u8) -> TextMetrics {
        self.text.measure(text, font, max_width, line_clamp)
    }
    fn asset_size(&mut self, hash: &[u8; 32]) -> Option<(f32, f32)> {
        self.assets
            .image(hash)
            .map(|i| (i.width as f32, i.height as f32))
            // A video is measured by its frame, which no image store holds.
            .or_else(|| self.videos.get(hash).copied())
    }
}
