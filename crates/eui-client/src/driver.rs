//! The session driver: everything the client does that is not a window or a
//! socket. Frames in, frames out; input in, events out; a draw list when
//! asked. Pure enough to be tested without a display or a network.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eui_audio::Control;
use eui_layout::{Env, FontSpec, Layout, Rect, Size, TextMeasurer, TextMetrics};
use eui_proto::limits::{DEFAULT_UPLOAD_BYTES, MAX_SAVE_BYTES, MAX_TRANSFER_CHUNK_BYTES, MAX_UPLOAD_BYTES};
use eui_proto::{
    caps, AlignItems, Batch, Chunked, ColorRef, Cursor, Dim, Display, EventFrame, EventKind, FlatNode, FontWeight, Frame, Handler, Hello, Justify, NodeKind, Op, Resume, StyleRecord, Subtree,
    TextAlign, TextRef, ThemeMode, Transfer, Value, Viewport, PROTOCOL_VERSION,
};
use eui_render::{colors_of, paint, scrollbar_thumb, Atlas, Colors, DrawList, Editing, Glide, GpuAnim, ImageAtlas, PaintCache, Scene, SCROLLBAR_WIDTH};

use crate::assets::{AssetStore, Hash};
use eui_text::TextEngine;
use eui_theme::{Resolved, Theme, Viewer};
use eui_tree::{Chunk, NodeIx, Session};

/// A dialog the tree asked for and the window has not opened yet
/// (spec 03 §3.2).
///
/// The driver never opens one: it has no window and, in a worker, no
/// filesystem either. It only says that a node the person just activated
/// carries `pick` or `save`, that the application declared a handler for
/// the answer, and that the capability behind it was granted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAsk {
    /// The driver's own name for this dialog; every later call about it
    /// carries it back.
    pub token: u32,
    /// The node that asked, by id.
    pub node: u32,
    /// Which dialog, and what to put in it.
    pub want: FileWant,
}

/// Which dialog a [`FileAsk`] wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileWant {
    /// The platform's open dialog.
    Open {
        /// Extensions to offer, comma separated and without dots;
        /// empty for every file.
        accept: String,
        /// Whether more than one file may be chosen.
        multiple: bool,
        /// The ceiling one file may not pass, in bytes.
        max: u64,
    },
    /// The platform's save dialog.
    Save {
        /// The name to suggest.
        name: String,
    },
}

/// Bytes the server owes a save, for the window to put on disk.
///
/// The driver decodes them, as it decodes everything; it does not write
/// them. The window knows the path, because the person chose it there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWrite {
    /// The [`FileAsk::token`] of the save these bytes belong to.
    pub token: u32,
    /// Whether more follow, this is the last, or the transfer failed —
    /// in which case `bytes` is the reason and the partial file goes.
    pub flag: Chunked,
    /// The bytes to append.
    pub bytes: Vec<u8>,
}

/// An upload in flight: what the driver must know to frame its chunks.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Upload {
    /// The node whose `pick` this answers.
    node: u32,
    /// The next chunk index to frame.
    seq: u32,
    /// Bytes framed so far.
    sent: u64,
    /// The ceiling the node asked for.
    max: u64,
}

/// A save in flight, keyed by the node the person answered.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Save {
    /// The ask the window still knows the path for.
    token: u32,
    /// The next chunk index expected: a gap ends the transfer.
    seq: u32,
    /// Bytes handed to the window so far.
    written: u64,
}

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
    /// A finger landed at logical `(x, y)`. `id` names the contact; only
    /// the first one on the glass is followed (spec 06 §5).
    TouchDown(u64, f32, f32),
    /// That finger moved to logical `(x, y)`.
    TouchMove(u64, f32, f32),
    /// That finger left the glass at logical `(x, y)`.
    TouchUp(u64, f32, f32),
    /// The window took the gesture away: a system edge swipe, a call
    /// arriving, the app going to the background. Whatever the finger
    /// pressed hears `pointer_up`, and no `click` follows it.
    TouchCancel(u64),
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
    /// When the last drag move was sent, while the server has not answered
    /// it. One move is in flight at a time: a drag that outruns the server
    /// otherwise builds a queue, and the queue is what a hand feels when
    /// it stops and the thing it was dragging goes on moving.
    move_in_flight: Option<Instant>,
}

/// How far a finger may wander from where it landed and still be a press
/// rather than a scroll, in logical pixels. Below this a tap that wobbles
/// still clicks what it landed on; above it, the finger is carrying the
/// view and the press it began with is taken back.
const TOUCH_SLOP: f32 = 8.0;

/// The time constant of a fling, in milliseconds. A finger that leaves the
/// glass at `v` logical px/ms carries the view `v * TOUCH_FLING_TAU_MS`
/// further, settling over three times that — the shape an exponential
/// decay would draw, flattened into the eased glide the scroller already
/// knows how to run (04 §7).
const TOUCH_FLING_TAU_MS: f32 = 110.0;

/// A finger that has been still for this long before it lifts is not
/// flinging, whatever the last samples said: it was placed, moved, and
/// held. Without this a stroke that ends in a pause throws the view.
const TOUCH_STILL_MS: u128 = 80;

/// The fastest fling honoured, in logical px/ms. A finger flicked off the
/// edge of the glass can report an enormous last sample; the view should
/// travel a long way, not an unbounded one.
const TOUCH_MAX_SPEED: f32 = 6.0;

/// What the finger turned out to be doing (spec 06 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum TouchPhase {
    /// Nothing on the glass.
    #[default]
    Off,
    /// Down, and not yet either a press or a scroll: the node under it did
    /// not ask for moves, and it has not wandered past the slop.
    Undecided,
    /// The node under it took the moves — a slider, a split bar, a
    /// scrollbar thumb — so every move is the pointer's and none of it
    /// scrolls.
    Dragging,
    /// It went past the slop without being taken: the finger carries the
    /// view, and the press it began with has been given back.
    Scrolling,
}

/// One finger, followed from the glass to the pointer (spec 06 §5).
///
/// The client reports contacts; the protocol has only a pointer. The
/// translation cannot be done by the window, because whether a stroke is a
/// press or a scroll depends on whether the node under it asked to hear
/// moves — which only the driver knows. So it is done here, on the near
/// side of the tree.
#[derive(Debug, Default)]
struct Touch {
    /// The contact being followed. Later fingers are ignored while it
    /// lasts: version 1 has no gesture that wants two.
    id: Option<u64>,
    /// Where it landed, in logical px — the slop is measured from here.
    from: (f32, f32),
    /// Where it last was, and when. The two a fling's speed comes from.
    last: (f32, f32),
    at: Option<Instant>,
    /// Logical px per millisecond, smoothed towards the newest sample.
    speed: (f32, f32),
    phase: TouchPhase,
}

impl Touch {
    /// Fold one sample into the smoothed speed. A stroke's fling is the
    /// last few milliseconds of it, not its average, so the newest sample
    /// carries most of the weight; a pause resets it to nothing.
    fn sample(&mut self, x: f32, y: f32, now: Instant) {
        if let Some(at) = self.at {
            let ms = now.saturating_duration_since(at).as_millis();
            if ms >= TOUCH_STILL_MS {
                self.speed = (0.0, 0.0);
            } else if ms > 0 {
                let dt = ms as f32;
                let v = ((x - self.last.0) / dt, (y - self.last.1) / dt);
                self.speed = (self.speed.0 * 0.3 + v.0 * 0.7, self.speed.1 * 0.3 + v.1 * 0.7);
            }
        }
        self.last = (x, y);
        self.at = Some(now);
    }

    /// How far the finger is from where it landed.
    fn wandered(&self, x: f32, y: f32) -> f32 {
        let (dx, dy) = (x - self.from.0, y - self.from.1);
        (dx * dx + dy * dy).sqrt()
    }

    /// Forget the contact; the next `TouchDown` starts over.
    fn end(&mut self) {
        *self = Self::default();
    }
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

    /// Whether the vertex stage can run this one from the list's clock.
    /// Colours and opacity it can; a blur it cannot, because the backdrop
    /// pass sizes its textures from the radius, so a transition that moves
    /// the blur is painted here frame by frame, as every one once was.
    fn gpu(&self) -> bool {
        self.from.blur == self.to.blur
    }

    /// What the painter is told: both ends, the clock relative to this
    /// paint, and where it stands now for what has to be baked.
    fn to_gpu(self, now: Instant) -> GpuAnim {
        GpuAnim {
            from: self.from,
            to: self.to,
            at: self.at(now),
            t0: -now.saturating_duration_since(self.start).as_secs_f32(),
            dur: self.duration.as_secs_f32(),
            decelerate: self.curve == eui_theme::Curve::DECELERATE,
            baked: !self.gpu(),
        }
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

/// The last list painted, and for how long it may be drawn again.
#[derive(Debug, Clone)]
struct Cached {
    list: Arc<DrawList>,
    /// When it was painted: the list's own clock starts here, and the
    /// window measures its age from it.
    painted_at: Instant,
    /// The last moment the list is right — the end of the last animation
    /// in it, or the next timer — `None` while nothing is owed at all.
    until: Option<Instant>,
    /// How often the window should draw it meanwhile: a transition's
    /// sixty a second, a spin's thirty, or nothing at rest.
    cadence: Option<Duration>,
}

/// A serial for each paint, so a list can say which paint it came from and
/// the renderer can tell the same list from a new one that looks alike.
/// Process-wide, seeded from the clock: a worker and the window that draws
/// its chrome each number their own lists, and neither must collide with
/// the other in a buffer that only remembers a number.
fn next_serial() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: std::sync::OnceLock<AtomicU64> = std::sync::OnceLock::new();
    let counter = SERIAL.get_or_init(|| {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        AtomicU64::new(u64::try_from(nanos & 0xFFFF_FFFF).unwrap_or(0) << 32)
    });
    counter.fetch_add(1, Ordering::Relaxed).saturating_add(1)
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
/// How long a scrollbar stays up once the scrolling stops, and how long it
/// then takes to go. A bar reports a movement, so there is nothing for one
/// to say about a page that is sitting still — and a strip of furniture
/// down the right of every scroller is a strip of furniture in every
/// screenshot of every page. Overlay bars, on Safari's timings.
const BAR_HOLD: Duration = Duration::from_millis(800);
const BAR_FADE: Duration = Duration::from_millis(400);
const WINDOW_SETTLE: Duration = Duration::from_millis(120);
/// How long a drag waits for the server to answer its last move before
/// sending another. A server that answers paces the drag itself, so this
/// is only for one that says nothing at all -- two frames, which is slow
/// enough not to build a queue and quick enough that a hand cannot feel
/// the wait.
const DRAG_ANSWER_WAIT: Duration = Duration::from_millis(32);
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
    /// The vertex stage carries it (04 §7): the layout is done once, at
    /// the landing offset, and the frames between are the same list.
    /// False for a glide too long for a virtualised list to hold both
    /// ends of, which moves the offset frame by frame as every glide
    /// once did.
    gpu: bool,
    /// The landing offset is in the tree and the layout knows the glide.
    armed: bool,
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
    /// The finger being followed, when the window reports contacts rather
    /// than a mouse (spec 06 §5).
    touch: Touch,
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
    /// The session the server named in `Welcome`, and what to offer it if
    /// the socket breaks (spec 01 §4.1).
    session_id: Option<[u8; 16]>,
    /// The last batch sequence applied, which is also the last acked.
    acked: u64,
    /// Dialogs the tree asked for that the window has not been handed yet.
    file_asks: Vec<FileAsk>,
    /// Dialogs opened and not yet answered, by token.
    asks: HashMap<u32, FileAsk>,
    /// The next token, for a dialog or an upload. Never reused, so a chunk
    /// that arrives late cannot land in a later transfer.
    next_token: u32,
    /// Uploads in flight, by their id on the wire.
    uploads: HashMap<u32, Upload>,
    /// Saves in flight, by the node the person answered.
    saves: HashMap<u32, Save>,
    /// Bytes waiting for the window to put on disk.
    writes: Vec<FileWrite>,
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
    /// The last list painted, while it may be drawn again: everything
    /// that moves in it moves in the vertex stage from the clock (03 §5),
    /// so the tree need not be walked to produce the same quads. A list at
    /// rest is the same list until something reaches the driver; one with
    /// a spin in it, until a timer or a scroll is owed.
    cached: Option<Cached>,
    /// Something reached the driver since the last paint — an input, a
    /// frame, an asset, a theme — that could change what the tree paints.
    /// The clock is not that: `tick` leaves it alone.
    touched: bool,
    /// Frames answered from `cached`, for the trace and the tests.
    spin_repeats: u64,
    /// Layouts computed, for the trace and the tests: a frame that draws
    /// the last list again, or moves a scroll in the vertex stage, does
    /// not add to it.
    relayouts: u64,
    /// The text engine's counters at the last paint, so the trace can say
    /// what this frame did rather than what the session has.
    last_text_stats: eui_text::Stats,
    /// What text nodes painted last frame, for the ones that did not
    /// change since.
    paint_cache: PaintCache,
    /// Its counters at the last paint, for the trace.
    last_paint_stats: eui_render::PaintStats,
    /// When a scroll offset last changed: a windowed list asks for rows
    /// once the view has been still for a moment, not per frame of a drag.
    scroll_touched: Option<Instant>,
    /// The scroller that last moved and when, for the bar it wears.
    scrolled: Option<(NodeIx, Instant)>,
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
            touch: Touch::default(),
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
            session_id: None,
            acked: 0,
            file_asks: Vec::new(),
            asks: HashMap::new(),
            next_token: 1,
            uploads: HashMap::new(),
            saves: HashMap::new(),
            writes: Vec::new(),
            resyncing: false,
            hover_relight: false,
            layout_valid: false,
            redraw: true,
            closed: None,
            desktop_colors: Vec::new(),
            desktop_mode: None,
            windows: HashMap::new(),
            outrun_at: None,
            cached: None,
            touched: true,
            spin_repeats: 0,
            relayouts: 0,
            last_text_stats: eui_text::Stats::default(),
            paint_cache: PaintCache::new(),
            last_paint_stats: eui_render::PaintStats::default(),
            scroll_touched: None,
            scrolled: None,
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
    ///
    /// On the first socket it offers nothing. On a later one — the client
    /// reconnecting after the network went away — it offers the session it
    /// still holds a tree for and the last batch it applied (spec 01 §4.1).
    /// Whether that offer is taken is the server's to say.
    pub fn hello(&self) -> Frame {
        let resume = self.session_id.filter(|_| self.session.root().is_some()).map(|session| Resume { session, acked: self.acked });
        Frame::Hello(Hello { version: PROTOCOL_VERSION, viewport: self.viewport(), granted: self.granted, resume })
    }

    /// The session id the server named, once it has.
    pub fn session_id(&self) -> Option<[u8; 16]> {
        self.session_id
    }

    /// The last batch applied and acked.
    pub fn acked(&self) -> u64 {
        self.acked
    }

    /// Everything a session owns, dropped: the tree, the tables, focus, the
    /// edits, the verified chunks, the transfers. What survives is what
    /// belongs to the window rather than to the session — the theme, the
    /// viewer, the atlases, and assets, which are named by their content
    /// and so are the same bytes in any session.
    fn start_over(&mut self) {
        self.session = Session::new();
        self.layout = Layout::new();
        self.focused = None;
        self.focus_visible = false;
        self.edits.clear();
        self.preedit.clear();
        self.chunks.clear();
        self.provisional.clear();
        self.anims.clear();
        self.scroll_anim = None;
        self.windows.clear();
        self.cached = None;
        self.uploads.clear();
        self.saves.clear();
        self.file_asks.clear();
        self.asks.clear();
        // A save whose bytes will never come: the window deletes the
        // partial file rather than leaving half an export behind.
        let orphans: Vec<u32> = self.writes.iter().map(|w| w.token).collect();
        self.writes.clear();
        for token in orphans {
            self.writes.push(FileWrite { token, flag: Chunked::Abort, bytes: b"the session ended".to_vec() });
        }
        self.acked = 0;
        self.resyncing = false;
        self.invalidate();
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
        // The server has spoken since the drag's last move, whatever it
        // said, so the next may go. Waiting for a batch in particular
        // would hold a drag whose move changed nothing the server draws --
        // a bar already against its stop answers with silence, and the
        // hand would feel it as a bar that will not move.
        self.pointer.move_in_flight = None;
        self.touched = true;
        match frame {
            Frame::Welcome(w) => {
                if w.version == 0 || w.version > PROTOCOL_VERSION {
                    self.closed = Some(Close::Version(w.version));
                    return vec![Frame::Error { code: 100, message: format!("unsupported version {}", w.version) }];
                }
                // Spec 01 §4.1. The server decides whether the session the
                // client offered is still there, and the client believes
                // it: a tree kept against a server that has forgotten it
                // would answer clicks the server cannot place.
                if w.resumed {
                    if self.session_id != Some(w.session) {
                        self.closed = Some(Close::Protocol("resumed a session the client did not offer"));
                        return vec![Frame::Error { code: 103, message: "resumed a session the client did not offer".into() }];
                    }
                } else if self.welcomed {
                    self.start_over();
                }
                self.session_id = Some(w.session);
                self.welcomed = true;
                Vec::new()
            }
            Frame::Blob(t) => self.blob(t),
            Frame::Batch(batch) => self.apply(&batch),
            Frame::Ping(n) => vec![Frame::Pong(n)],
            Frame::Pong(_) => Vec::new(),
            Frame::Error { code, message } => {
                self.closed = Some(Close::ServerError(code, message));
                Vec::new()
            }
            Frame::Hello(_) | Frame::Event(_) | Frame::Ack { .. } | Frame::Resync | Frame::Viewport(_) | Frame::Upload(_) => {
                self.closed = Some(Close::Protocol("client-only frame from server"));
                vec![Frame::Error { code: 101, message: "client-only frame from server".into() }]
            }
        }
    }

    fn apply(&mut self, batch: &Batch) -> Vec<Frame> {
        // A resumed session replays what the socket dropped, and the last
        // batch before it broke may well have landed. Applying it twice is
        // not always harmless — a `SetText` is, an `Insert` is not — so a
        // sequence already applied is acked again and otherwise ignored.
        if batch.seq <= self.acked {
            return vec![Frame::Ack { seq: self.acked }];
        }
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
                self.acked = batch.seq;
                let mut out = vec![Frame::Ack { seq: batch.seq }];
                // A `Focus` op focuses the way the keyboard does, ring included.
                if batch.ops.iter().any(|o| matches!(o, eui_proto::Op::Focus { .. })) {
                    if let Some(ix) = self.session.focused() {
                        out.extend(self.set_focus(Some(ix), true));
                    }
                } else {
                    out.extend(self.take_autofocus());
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

    /// Whether a glide from `from` to `to` can be the vertex stage's (04
    /// §7). A plain scroller lays out all its content, so any distance
    /// can; a virtualised list materialises the rows at both ends of the
    /// travel, so only a travel of a couple of viewports -- a notch, a
    /// page -- can, and a jump to the end of ten thousand rows moves the
    /// offset frame by frame instead.
    fn glide_on_gpu(&self, scroller: NodeIx, from: (f32, f32), to: (f32, f32)) -> bool {
        if self.layout.row_tops(scroller).is_none() {
            return true;
        }
        let view_h = self.layout.rect(scroller).map_or(0.0, |r| r.h);
        (to.1 - from.1).abs() <= 2.0 * view_h
    }

    /// Move a scroll animation to `now`. A glide the vertex stage carries
    /// puts its landing offset into the tree once and tells the layout
    /// how far the content stands from it; one it does not puts the
    /// offset of the moment in, frame by frame. Returns the frames to
    /// send once it lands.
    fn advance_scroll(&mut self) -> Vec<Frame> {
        let Some(mut a) = self.scroll_anim else {
            self.layout.clear_glides();
            return Vec::new();
        };
        if self.session.node(a.node).is_none() {
            self.scroll_anim = None;
            self.layout.clear_glides();
            return Vec::new();
        }
        if !a.gpu {
            let (x, y) = a.at(self.now);
            self.session.set_scroll(a.node, x.round() as i64, y.round() as i64);
            self.layout_valid = false;
            self.scroll_touched = Some(self.now);
            self.scrolled = Some((a.node, self.now));
            if a.done(self.now) {
                self.scroll_anim = None;
                let (nx, ny) = (a.to.0.round() as i64, a.to.1.round() as i64);
                return self.emit(a.node, EventKind::Scroll, Value::List(vec![Value::Int(nx), Value::Int(ny)]));
            }
            return Vec::new();
        }
        let (bx, by) = (a.to.0.round(), a.to.1.round());
        let (ax, ay) = a.at(self.now);
        let delta = (bx - ax, by - ay);
        if a.armed {
            self.layout.set_glide_delta(a.node, delta);
        } else {
            // One layout, at the landing: a retarget mid-flight lands
            // somewhere else and takes another.
            self.session.set_scroll(a.node, bx as i64, by as i64);
            self.layout.set_glide(a.node, a.from.1.min(a.to.1), a.from.1.max(a.to.1), delta);
            self.layout_valid = false;
            self.scroll_touched = Some(self.now);
            self.scrolled = Some((a.node, self.now));
            a.armed = true;
            self.scroll_anim = Some(a);
        }
        if a.done(self.now) {
            self.scroll_anim = None;
            self.layout.clear_glide(a.node);
            self.scroll_touched = Some(self.now);
            self.scrolled = Some((a.node, self.now));
            return self.emit(a.node, EventKind::Scroll, Value::List(vec![Value::Int(bx as i64), Value::Int(by as i64)]));
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
        self.input_at(input, Instant::now())
    }

    /// [`Self::input`] with the clock named rather than read.
    ///
    /// A stroke is a shape in time — how far the finger went and how long it
    /// took — so a test of one has to be able to say when each sample
    /// happened. Sleeping between them instead makes the test a race with
    /// the machine it runs on, and it is a race that loses: the fling test
    /// did exactly that and went red on a loaded CI runner, where the sleeps
    /// stretched past the pause that means a finger was held rather than
    /// thrown.
    ///
    /// Nothing in the client calls this; `input` is what a window uses, and
    /// it reads the wall clock as it always did.
    pub fn input_at(&mut self, input: Input, now: Instant) -> Vec<Frame> {
        self.touched = true;
        self.now = now;
        match input {
            Input::Resized(w, h, scale) => {
                let rescaled = (self.scale - scale).abs() > f32::EPSILON;
                self.size = Size::new(w, h);
                self.scale = scale;
                // A new size is not new text. What a node measures depends
                // on the constraints it was given, and those are in the
                // memo's key, so a width that changed brings its own key
                // and one that did not keeps its answer -- a window
                // dragged wider re-measures what the width reaches and
                // leaves the rest. A new *scale* is another matter: the
                // glyphs are rasterised afresh, so nothing may be kept.
                if rescaled {
                    self.layout.invalidate_all();
                    self.paint_cache.clear();
                }
                self.invalidate();
                // The clock this input arrived on, not a fresh reading of
                // the wall: `input_at` exists so that a test can say when.
                let now = self.now;
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
            Input::TouchDown(id, x, y) => self.touch_down(id, x, y),
            Input::TouchMove(id, x, y) => self.touch_move(id, x, y),
            Input::TouchUp(id, x, y) => self.touch_up(id, x, y),
            Input::TouchCancel(id) => self.touch_cancel(id),
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
                // Whatever the window has lost the input to, it is not
                // holding a finger any more. The contact is forgotten here
                // rather than by a `TouchCancel`, because the window cannot
                // name the contact it never saw an id for — it only knows
                // that the input is gone.
                self.touch.end();
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
            self.relayouts = self.relayouts.saturating_add(1);
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
        self.paint_cache.clear();
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
        let mut host = SessionHost { session: &mut self.session, emitted: Vec::new(), touched: false, repaint: false, undo: provisional.then(Vec::new), mode: None };
        let result = eui_vm::run(&verified, &mut host);
        let touched = host.touched;
        let repaint = host.repaint;
        let emitted = host.emitted;
        let mode = host.mode;
        if let Some(undo) = host.undo {
            self.provisional.extend(undo);
        }
        if touched {
            self.audio_dirty = true;
            self.video_dirty = true;
            self.invalidate();
        } else if repaint {
            self.redraw = true;
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
                trace(|| format!("drag: captured move {x},{y} for node {:?}", self.session.node(pressed).map(|n| n.id)));
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
    /// The node drawn under a point. Mid-glide the content is on its way
    /// from where the layout put it (04 §7), and the frames between were
    /// the same list -- no paint moved anything -- so the layout is told
    /// where the content stands now before it is asked.
    fn hit_now(&mut self, x: f32, y: f32) -> Option<NodeIx> {
        if let Some(a) = self.scroll_anim.filter(|a| a.gpu && a.armed) {
            let (ax, ay) = a.at(self.now);
            self.layout.set_glide_delta(a.node, (a.to.0.round() - ax, a.to.1.round() - ay));
        }
        self.layout.hit(&self.session, x, y)
    }

    fn hover(&mut self, x: f32, y: f32) -> Vec<Frame> {
        self.pointer.hover_pending = false;
        let now = self.hit_now(x, y);
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
            // A node that asked to hear the pointer only while it is being
            // dragged hears nothing on a bare hover. Everything else is
            // unchanged: the handler stays in the tree, so no event ever
            // arrives naming a handler the tree no longer offers.
            if self.pointer.pressed_on.is_some() || !self.drag_only(ix) {
                let p = self.point_payload(ix, EventKind::PointerMove, x, y);
                out.extend(self.emit(ix, EventKind::PointerMove, p));
            }
        }
        out
    }

    /// Whether the node that would *receive* a `pointer_move` from here
    /// carries `drag_only` (06 §1): the move is for dragging it, not for
    /// passing over it.
    ///
    /// The receiver, not the node under the pointer: an event travels up to
    /// the nearest handler (06 §2), so the pointer is almost always over
    /// some deep child of the node that declared the intent. Asking the
    /// child was the first version of this, and it never matched.
    ///
    /// The alternative — taking the handler off the node while no drag is
    /// in flight — looks equivalent and is not: an event already in flight
    /// then names a handler the server has just removed, and every one of
    /// them is refused. Measured on a split, that was 167 of 429 events
    /// dropped and a drag that could not follow the hand.
    fn drag_only(&self, from: NodeIx) -> bool {
        let Some(atom) = self.session.atoms().drag_only else {
            return false;
        };
        let Some((ix, _)) = self.target(from, EventKind::PointerMove) else {
            return false;
        };
        self.session.node(ix).is_some_and(|n| n.props.iter().any(|(a, v)| *a == atom && matches!(v, Value::Bool(true))))
    }

    /// The drag's move for this frame, if the server is ready for one.
    ///
    /// Sending one a frame regardless outruns a server that needs longer
    /// than a frame to answer -- the gallery re-renders nine hundred nodes
    /// for each -- and the moves queue. What that feels like is letting go
    /// and watching the thing carry on: the hand stopped, the queue did
    /// not. So one is in flight at a time, and the latest position waits
    /// its turn rather than joining a line. A handler that answers nothing
    /// would hold the drag for ever, so the wait has an end.
    fn flush_drag_move(&mut self) -> Vec<Frame> {
        if self.pointer.coalesced_move.is_none() {
            return Vec::new();
        }
        if let Some(at) = self.pointer.move_in_flight.filter(|at| self.now.saturating_duration_since(*at) < DRAG_ANSWER_WAIT) {
            trace(|| format!("drag: holding, {:?} since the last went unanswered", self.now.saturating_duration_since(at)));
            return Vec::new();
        }
        let out = self.flush_coalesced_move();
        trace(|| format!("drag: {} move(s) sent this frame", out.len()));
        if !out.is_empty() {
            self.pointer.move_in_flight = Some(self.now);
            // The next frame is owed: the position it holds may be the one
            // that never gets sent otherwise.
            self.redraw = true;
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
        // Only a slider. Three children under a `pointer_move` handler is
        // not enough to know one: a split pane is a panel, a divider and a
        // panel, dragged by the same handler, and laying its three out as
        // a track, a thumb and the rest leaves it somewhere it never asked
        // to be. The node says what it is (03 §9), so ask it.
        if !self.declares_role(ix, "slider") {
            return;
        }
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

    /// Whether the node declares this accessibility `role` (03 §9).
    fn declares_role(&self, ix: NodeIx, role: &str) -> bool {
        let Some(atom) = self.session.atom_id("role") else {
            return false;
        };
        self.session.node(ix).and_then(|n| n.prop(atom)).is_some_and(|v| matches!(v, Value::Str(s) if s == role))
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
        let Some(ix) = self.hit_now(x, y) else {
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
        trace(|| format!("press on node {:?}, moves go to {:?}", self.session.node(ix).map(|n| n.id), self.target(ix, EventKind::PointerMove).and_then(|(t, _)| self.session.node(t)).map(|n| n.id)));
        // A new drag starts owing nothing, whatever the last one left.
        self.pointer.move_in_flight = None;
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
        let hit = self.hit_now(x, y);
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
                    if matches!(kind, EventKind::Click) {
                        self.offer_files(ix);
                    }
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

    // ---------------------------------------------------------- the finger

    /// A finger landed. It is reported as the pointer arriving and pressing
    /// (spec 06 §5), and what happens to the moves after that depends on
    /// what it landed on: a node that asked to hear `pointer_move` — a
    /// slider, a split bar — has taken a drag and keeps every move; a
    /// scrollbar thumb likewise. Anything else leaves the gesture undecided
    /// until the finger either lifts (a tap) or wanders past the slop (a
    /// scroll).
    ///
    /// A second finger while one is down is ignored: version 1 has no
    /// gesture that wants two, and a stray palm must not move the view.
    fn touch_down(&mut self, id: u64, x: f32, y: f32) -> Vec<Frame> {
        if self.touch.id.is_some() {
            return Vec::new();
        }
        self.touch = Touch { id: Some(id), from: (x, y), last: (x, y), at: Some(self.now), speed: (0.0, 0.0), phase: TouchPhase::Undecided };
        // The pointer arrives before it presses, so the press lands on the
        // node under the finger and not on wherever the last one was.
        let mut out = self.pointer_move(x, y);
        out.extend(self.pointer_down(0));
        // Taken as a drag, and by whom: the thumb is the client's own, the
        // rest is whatever asked for moves.
        let taken = self.pointer.dragging_thumb.is_some() || self.pointer.pressed_on.is_some_and(|ix| self.target(ix, EventKind::PointerMove).is_some());
        if taken {
            self.touch.phase = TouchPhase::Dragging;
        }
        trace(|| format!("touch {id} down at {x},{y}: {:?}", self.touch.phase));
        out
    }

    /// The finger moved. While the gesture is undecided the pointer is left
    /// where it landed — a tap that wobbles five pixels should still click
    /// what it was aimed at — and the move is only a measurement, until it
    /// crosses the slop and becomes a scroll.
    fn touch_move(&mut self, id: u64, x: f32, y: f32) -> Vec<Frame> {
        if self.touch.id != Some(id) {
            return Vec::new();
        }
        let (dx, dy) = (x - self.touch.last.0, y - self.touch.last.1);
        self.touch.sample(x, y, self.now);
        match self.touch.phase {
            TouchPhase::Dragging => self.pointer_move(x, y),
            TouchPhase::Scrolling => self.wheel(-dx, -dy),
            TouchPhase::Undecided if self.touch.wandered(x, y) > TOUCH_SLOP => {
                trace(|| format!("touch {id} became a scroll at {x},{y}"));
                self.touch.phase = TouchPhase::Scrolling;
                // The press is given back before the view moves: what it
                // landed on hears `pointer_up` and no `click`, because
                // none was meant. A button under a scrolling thumb must
                // not fire when the thumb lifts.
                let mut out = self.cancel_press();
                // From where it landed, not from the last sample: the slop
                // is part of the gesture, and swallowing it makes the view
                // lag the finger by eight pixels for the whole stroke.
                let (fx, fy) = (x - self.touch.from.0, y - self.touch.from.1);
                out.extend(self.wheel(-fx, -fy));
                out
            }
            TouchPhase::Undecided | TouchPhase::Off => Vec::new(),
        }
    }

    /// The finger left the glass.
    fn touch_up(&mut self, id: u64, x: f32, y: f32) -> Vec<Frame> {
        if self.touch.id != Some(id) {
            return Vec::new();
        }
        self.touch.sample(x, y, self.now);
        let phase = self.touch.phase;
        let speed = self.touch.speed;
        let mut out = match phase {
            // The drag ends where the finger did.
            TouchPhase::Dragging => {
                let mut out = self.pointer_move(x, y);
                out.extend(self.pointer_up(0));
                out
            }
            // A tap: the release is taken at the point it landed on, so
            // press and release resolve to the same handler and a `click`
            // follows even though the finger moved a little.
            TouchPhase::Undecided => self.pointer_up(0),
            // A stroke that was carrying the view: no press is outstanding,
            // and the view goes on if the finger was still moving.
            TouchPhase::Scrolling => self.fling(speed),
            TouchPhase::Off => Vec::new(),
        };
        self.touch.end();
        // A finger leaves no hover behind. Without this the node it lifted
        // from stays lit, which on a touch screen is a highlight nothing
        // will ever put out.
        out.extend(self.clear_hover());
        trace(|| format!("touch {id} up at {x},{y} after {phase:?}"));
        out
    }

    /// The window took the gesture away. Whatever was pressed hears
    /// `pointer_up`; nothing clicks, and nothing flings.
    fn touch_cancel(&mut self, id: u64) -> Vec<Frame> {
        if self.touch.id != Some(id) {
            return Vec::new();
        }
        self.touch.end();
        let mut out = self.cancel_press();
        self.pointer.dragging_thumb = None;
        out.extend(self.clear_hover());
        out
    }

    /// End a press without a click. The node that took it hears
    /// `pointer_up` — a button lit under the finger puts itself out — and
    /// no `click` follows, because the finger turned out to be carrying the
    /// view rather than pressing what it landed on.
    fn cancel_press(&mut self) -> Vec<Frame> {
        let Some(pressed) = self.pointer.pressed_on.take() else {
            return Vec::new();
        };
        let mut out = self.flush_coalesced_move();
        let (x, y) = (self.pointer.x, self.pointer.y);
        let payload = self.button_payload(pressed, EventKind::PointerUp, x, y, 0);
        out.extend(self.emit(pressed, EventKind::PointerUp, payload));
        out
    }

    /// A finger that leaves the glass still moving carries the view on.
    ///
    /// The speed it lifted at gives a distance — `speed * tau` — and the
    /// glide the scroller already runs for a wheel notch (04 §7) does the
    /// drawing, over three tau so that it arrives rather than stops. Below
    /// a tenth of a pixel per millisecond there is nothing to carry: the
    /// finger was placed and lifted, and the view stays where it was put.
    fn fling(&mut self, speed: (f32, f32)) -> Vec<Frame> {
        let clamp = |v: f32| v.clamp(-TOUCH_MAX_SPEED, TOUCH_MAX_SPEED);
        let (vx, vy) = (clamp(speed.0), clamp(speed.1));
        if vx.abs() < 0.1 && vy.abs() < 0.1 {
            return Vec::new();
        }
        // The view travels against the finger, as it did during the stroke.
        let (dx, dy) = (-vx * TOUCH_FLING_TAU_MS, -vy * TOUCH_FLING_TAU_MS);
        let Some(scroller) = self.scroller_under_pointer_for(dx, dy) else {
            return Vec::new();
        };
        let content = self.layout.content_size(scroller).unwrap_or_default();
        let view = self.layout.rect(scroller).unwrap_or_default();
        let (max_x, max_y) = ((content.w - view.w).max(0.0), (content.h - view.h).max(0.0));
        let (sx, sy) = self.session.node(scroller).map(|n| n.scroll).unwrap_or((0, 0));
        let from = ((sx as f32).clamp(0.0, max_x), (sy as f32).clamp(0.0, max_y));
        let to = ((from.0 + dx).clamp(0.0, max_x), (from.1 + dy).clamp(0.0, max_y));
        if (to.0 - from.0).abs() < 0.5 && (to.1 - from.1).abs() < 0.5 {
            return Vec::new();
        }
        trace(|| format!("fling {vx:.2},{vy:.2} px/ms: {from:?} -> {to:?}"));
        let ms = (TOUCH_FLING_TAU_MS * 3.0) as u64;
        let gpu = self.glide_on_gpu(scroller, from, to);
        // `smooth: false` — it is already moving when the finger lets go,
        // so it eases out and never in.
        self.scroll_anim = Some(ScrollAnim { smooth: false, node: scroller, from, to, start: self.now, duration: Duration::from_millis(ms), gpu, armed: false });
        self.next_due = Some(self.now);
        self.redraw = true;
        Vec::new()
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
        // A modal owns the keyboard while it is up. Tab used to walk the whole
        // tree from the root, so it left an open dialog and wandered the page
        // behind it — and a server cannot fix that, because it does not own
        // Tab. The innermost laid-out node carrying `modal` becomes the root
        // of the walk instead, which nests: a dialog opened over a dialog
        // traps inside the second.
        let mut stack = vec![self.modal_root().unwrap_or(root)];
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
                || node.handler(EventKind::KeyUp).is_some()
                || node.handler(EventKind::FilePick).is_some()
                || node.handler(EventKind::FileSave).is_some();
            if focusable && self.layout.rect(ix).is_some() && !self.layout.is_virtual(ix) {
                order.push(ix);
            }
            stack.extend(self.session.children(ix).iter().rev());
        }
        order
    }

    /// The innermost laid-out node declaring itself modal, if any. Preorder,
    /// so a modal inside a modal comes later and wins.
    fn modal_root(&self) -> Option<NodeIx> {
        let atom = self.session.atom_id("modal")?;
        let root = self.session.root()?;
        let mut found = None;
        for ix in self.session.preorder(root) {
            if self.layout.rect(ix).is_some() && !self.layout.is_virtual(ix) && self.session.node(ix).and_then(|n| n.prop(atom)) == Some(&Value::Bool(true)) {
                found = Some(ix);
            }
        }
        found
    }

    /// The first laid-out node asking to be focused when it appears. A dialog
    /// that opens with focus still behind it has nothing for `Escape` to fire
    /// from, and nothing for the trap above to hold.
    fn autofocus_target(&self) -> Option<NodeIx> {
        let atom = self.session.atom_id("autofocus")?;
        let from = self.modal_root().or_else(|| self.session.root())?;
        self.session.preorder(from).find(|ix| self.layout.rect(*ix).is_some() && !self.layout.is_virtual(*ix) && self.session.node(*ix).and_then(|n| n.prop(atom)) == Some(&Value::Bool(true)))
    }

    /// Put focus where a newly-arrived surface asked for it.
    ///
    /// Only when focus is not already where it belongs, so a batch that
    /// arrives while someone is tabbing through an open dialog does not yank
    /// them back to its first field. A modal claims focus whenever focus is
    /// outside it; a page with no modal claims it only when nothing has it.
    fn take_autofocus(&mut self) -> Vec<Frame> {
        if self.session.atom_id("autofocus").is_none() {
            return Vec::new();
        }
        self.ensure_layout();
        let Some(target) = self.autofocus_target() else {
            return Vec::new();
        };
        let settled = match self.modal_root() {
            Some(m) => self.focused.is_some_and(|f| self.session.preorder(m).any(|x| x == f)),
            None => self.focused.is_some(),
        };
        if settled {
            return Vec::new();
        }
        self.set_focus(Some(target), true)
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
        let out = self.emit(f, EventKind::Click, p);
        self.offer_files(f);
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

    /// Whether `f` asked for this key itself, and so should not have the
    /// client's meaning put on it.
    ///
    /// A node holding `key_down` used to claim *every* key, which made a
    /// widget choose: take the arrows, or keep `Enter` and `Space` as the
    /// press they stand for. A `keys` prop naming what it wants settles it —
    /// a tab can take `ArrowLeft` and `ArrowRight` and still be activated by
    /// `Enter`. Without the prop the old all-or-nothing rule stands, so a
    /// tracker that wants `Space` for itself keeps it.
    fn claims_key(&self, f: NodeIx, key: &str) -> bool {
        let Some(node) = self.session.node(f) else {
            return false;
        };
        if node.handler(EventKind::KeyDown).is_none() {
            return false;
        }
        let Some(atom) = self.session.atom_id("keys") else {
            return true;
        };
        match node.prop(atom) {
            Some(Value::List(want)) => want.iter().any(|k| matches!(k, Value::Str(s) if s == key)),
            _ => true,
        }
    }

    /// Hit node under the pointer, without requiring it to be a scroller.
    fn hit_under_pointer(&mut self) -> Option<NodeIx> {
        let settled = self.pointer.over.filter(|o| self.session.node(*o).is_some());
        match settled {
            Some(o) if !self.layout_valid => Some(o),
            _ => {
                self.ensure_layout();
                self.hit_now(self.pointer.x, self.pointer.y)
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
        self.scrolled = Some((scroller, Instant::now()));
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
        let gpu = self.glide_on_gpu(scroller, from, to);
        self.scroll_anim = Some(ScrollAnim { smooth, node: scroller, from, to, start: self.now, duration: Duration::from_millis(u64::from(ms)), gpu, armed: false });
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
        self.paint_cache.clear();
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
        self.paint_cache.clear();
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
        let gpu = self.glide_on_gpu(scroller, from, to);
        self.scroll_anim = Some(ScrollAnim { smooth: false, node: scroller, from, to, start: self.now, duration: Duration::from_millis(u64::from(ms)), gpu, armed: false });
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
        self.scrolled = Some((scroller, Instant::now()));
        self.invalidate();
        self.emit(scroller, EventKind::Scroll, Value::List(vec![Value::Int(nx), Value::Int(ny)]))
    }

    /// The bar the scroller that last moved is wearing, and how far in.
    /// Empty once it has faded, which is what stops the frames.
    fn scrollbars(&mut self) -> Vec<(NodeIx, f32)> {
        let Some((ix, at)) = self.scrolled else {
            return Vec::new();
        };
        if self.session.node(ix).is_none() {
            self.scrolled = None;
            return Vec::new();
        }
        let since = self.now.saturating_duration_since(at);
        if since <= BAR_HOLD {
            return vec![(ix, 1.0)];
        }
        let gone = (since - BAR_HOLD).as_secs_f32() / BAR_FADE.as_secs_f32();
        if gone >= 1.0 {
            self.scrolled = None;
            return Vec::new();
        }
        vec![(ix, 1.0 - gone)]
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

    // -------------------------------------------------------------- files

    /// Dialogs the tree asked for since the last call (spec 03 §3.2). The
    /// window opens them; nothing else may.
    pub fn take_file_asks(&mut self) -> Vec<FileAsk> {
        std::mem::take(&mut self.file_asks)
    }

    /// Bytes a save is owed, for the window to append to the file the
    /// person named.
    pub fn take_writes(&mut self) -> Vec<FileWrite> {
        std::mem::take(&mut self.writes)
    }

    /// A token nothing else will carry.
    fn mint(&mut self) -> u32 {
        let t = self.next_token;
        self.next_token = self.next_token.saturating_add(1);
        t
    }

    /// Spec 03 §3.2: the person activated a node. If it carries `pick` or
    /// `save`, declares the handler that answers it, and the capability
    /// behind it was granted, the window is asked for the platform's own
    /// dialog.
    ///
    /// Three conditions, and every one of them is checked here rather than
    /// where the dialog opens: a dialog the person did not ask for is the
    /// whole of what makes a file picker dangerous.
    fn offer_files(&mut self, from: NodeIx) {
        for (kind, prop, cap) in [(EventKind::FilePick, "pick", caps::FS_PICK), (EventKind::FileSave, "save", caps::FS_SAVE)] {
            let Some((ix, handler)) = self.target(from, kind) else { continue };
            // A local chunk cannot be given a file and cannot answer with
            // one: both ends of a transfer are the server's.
            if !matches!(handler, Handler::Server(_)) {
                continue;
            }
            let Some(atom) = self.session.atom_id(prop) else { continue };
            let Some(node) = self.session.node(ix) else { continue };
            let id = node.id;
            let Some(value) = node.prop(atom).cloned() else { continue };
            if self.granted & cap == 0 {
                eprintln!("eui: node {id} carries `{prop}`, which needs a capability the person did not grant; nothing opens");
                continue;
            }
            // One dialog at a time per node: a second click while the
            // first is open must not stack two of them.
            if self.asks.values().any(|a| a.node == id) {
                continue;
            }
            let want = match kind {
                EventKind::FilePick => {
                    let (accept, flags, max) = match &value {
                        Value::Str(a) => (a.clone(), 0, DEFAULT_UPLOAD_BYTES),
                        Value::List(l) => {
                            let accept = match l.first() {
                                Some(Value::Str(a)) => a.clone(),
                                _ => String::new(),
                            };
                            let flags = match l.get(1) {
                                Some(Value::Int(f)) => *f,
                                _ => 0,
                            };
                            let max = match l.get(2) {
                                Some(Value::Int(m)) if *m > 0 => (*m as u64).min(MAX_UPLOAD_BYTES),
                                _ => DEFAULT_UPLOAD_BYTES,
                            };
                            (accept, flags, max)
                        }
                        _ => (String::new(), 0, DEFAULT_UPLOAD_BYTES),
                    };
                    FileWant::Open { accept, multiple: flags & 1 != 0, max }
                }
                _ => {
                    let name = match &value {
                        Value::Str(n) => basename(n).to_owned(),
                        Value::List(l) => match l.first() {
                            Some(Value::Str(n)) => basename(n).to_owned(),
                            _ => String::new(),
                        },
                        _ => String::new(),
                    };
                    FileWant::Save { name: if name.is_empty() { "download".into() } else { name } }
                }
            };
            let token = self.mint();
            let ask = FileAsk { token, node: id, want };
            self.asks.insert(token, ask.clone());
            self.file_asks.push(ask);
            self.touched = true;
        }
    }

    /// The person chose files in the dialog `token` opened: one `file_pick`
    /// event each, and an upload id each for the window to stream against.
    ///
    /// A file past the ceiling the node asked for gets its event and an
    /// immediate abort, so the application can say why rather than watch
    /// nothing happen; its id accepts no chunks.
    pub fn picked(&mut self, token: u32, files: Vec<(String, u64)>) -> (Vec<u32>, Vec<Frame>) {
        let Some(ask) = self.asks.remove(&token) else { return (Vec::new(), Vec::new()) };
        let FileWant::Open { max, .. } = ask.want else { return (Vec::new(), Vec::new()) };
        self.touched = true;
        let Some(ix) = self.session.lookup(ask.node) else { return (Vec::new(), Vec::new()) };
        let (mut ids, mut out) = (Vec::new(), Vec::new());
        for (name, size) in files {
            let id = self.mint();
            let name = basename(&name).to_owned();
            let payload = Value::List(vec![Value::Int(i64::from(id)), Value::Str(name), Value::Int(i64::try_from(size).unwrap_or(i64::MAX))]);
            out.extend(self.emit(ix, EventKind::FilePick, payload));
            if size > max {
                out.push(Frame::Upload(Transfer { id, seq: 0, flag: Chunked::Abort, bytes: format!("file is {size} bytes; this one accepts {max}").into_bytes() }));
            } else {
                self.uploads.insert(id, Upload { node: ask.node, seq: 0, sent: 0, max });
            }
            ids.push(id);
        }
        (ids, out)
    }

    /// The dialog `token` opened was dismissed. A cancel is not an event:
    /// nothing happened, and the application hears nothing.
    pub fn dialog_dismissed(&mut self, token: u32) {
        if self.asks.remove(&token).is_some() {
            self.touched = true;
        }
    }

    /// Bytes of an upload, in order, framed for the wire. `last` closes it.
    pub fn upload_chunk(&mut self, id: u32, bytes: &[u8], last: bool) -> Vec<Frame> {
        let Some(up) = self.uploads.get(&id) else { return Vec::new() };
        self.touched = true;
        let (mut seq, sent, max) = (up.seq, up.sent, up.max);
        let total = sent.saturating_add(bytes.len() as u64);
        if total > max {
            self.uploads.remove(&id);
            return vec![Frame::Upload(Transfer { id, seq, flag: Chunked::Abort, bytes: format!("more than the {max} bytes this one accepts").into_bytes() })];
        }
        let mut out = Vec::new();
        let mut rest = bytes;
        loop {
            let take = rest.len().min(MAX_TRANSFER_CHUNK_BYTES);
            let (head, tail) = rest.split_at(take);
            let done = last && tail.is_empty();
            out.push(Frame::Upload(Transfer { id, seq, flag: if done { Chunked::Last } else { Chunked::More }, bytes: head.to_vec() }));
            seq = seq.saturating_add(1);
            rest = tail;
            if rest.is_empty() {
                break;
            }
        }
        if last {
            self.uploads.remove(&id);
        } else if let Some(up) = self.uploads.get_mut(&id) {
            up.seq = seq;
            up.sent = total;
        }
        out
    }

    /// The window could not read what the person picked. The server is told
    /// so it can stop waiting for bytes that are not coming.
    pub fn upload_failed(&mut self, id: u32, why: String) -> Vec<Frame> {
        if self.uploads.remove(&id).is_none() {
            return Vec::new();
        }
        self.touched = true;
        let mut bytes = why.into_bytes();
        bytes.truncate(eui_proto::limits::MAX_ABORT_REASON);
        vec![Frame::Upload(Transfer { id, seq: 0, flag: Chunked::Abort, bytes })]
    }

    /// The person chose where what this node offers should go. The server
    /// is asked for it; the path stays with the window.
    pub fn saving(&mut self, token: u32, name: String) -> Vec<Frame> {
        let Some(ask) = self.asks.remove(&token) else { return Vec::new() };
        if !matches!(ask.want, FileWant::Save { .. }) {
            return Vec::new();
        }
        self.touched = true;
        let Some(ix) = self.session.lookup(ask.node) else { return Vec::new() };
        self.saves.insert(ask.node, Save { token, seq: 0, written: 0 });
        self.emit(ix, EventKind::FileSave, Value::Str(basename(&name).to_owned()))
    }

    /// A chunk of what a save is owed (spec 01 §6).
    ///
    /// The client writes a file only where the person just said, and only
    /// for the node they activated: a blob for anything else is a server
    /// trying to put bytes on a disk nobody offered it, and ends the
    /// session.
    fn blob(&mut self, t: Transfer) -> Vec<Frame> {
        let Some(save) = self.saves.get_mut(&t.id) else {
            self.closed = Some(Close::Protocol("a blob for a save nobody asked for"));
            return vec![Frame::Error { code: 104, message: "blob for a save nobody asked for".into() }];
        };
        let token = save.token;
        if t.seq != save.seq {
            self.saves.remove(&t.id);
            self.writes.push(FileWrite { token, flag: Chunked::Abort, bytes: b"the server sent the chunks out of order".to_vec() });
            return Vec::new();
        }
        match t.flag {
            Chunked::Abort => {
                self.saves.remove(&t.id);
                self.writes.push(FileWrite { token, flag: Chunked::Abort, bytes: t.bytes });
            }
            flag => {
                let written = save.written.saturating_add(t.bytes.len() as u64);
                if written > MAX_SAVE_BYTES {
                    self.saves.remove(&t.id);
                    self.writes.push(FileWrite { token, flag: Chunked::Abort, bytes: format!("more than the {MAX_SAVE_BYTES} bytes one save may write").into_bytes() });
                    return Vec::new();
                }
                save.written = written;
                save.seq = save.seq.saturating_add(1);
                if matches!(flag, Chunked::Last) {
                    self.saves.remove(&t.id);
                }
                self.writes.push(FileWrite { token, flag, bytes: t.bytes });
            }
        }
        Vec::new()
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
            if !down {
                return Vec::new();
            }
            // 03 §3: Escape shuts what is open before it lets go of focus.
            // It used to return here always, so no dialog, sheet, menu or
            // popover could close on it — the server never learned the key
            // had been pressed. Now anything on the path that asked for keys
            // hears it, and focus stays where it is so the surface can put it
            // back where it belongs; with nothing listening, Escape means what
            // it always meant.
            if self.ancestor_keyed(f).is_none() || !self.wants_key(f, key) {
                return self.set_focus(None, false);
            }
            return self.emit(f, EventKind::KeyDown, Value::List(vec![Value::Str(key.to_owned()), Value::Int(i64::from(modifiers))]));
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
                "Enter" | " " if !editable && !self.claims_key(f, key) => {
                    out.extend(self.activate(f));
                }
                _ => {}
            }
        }
        let kind = if down { EventKind::KeyDown } else { EventKind::KeyUp };
        if self.wants_key(f, key) {
            out.extend(self.emit(f, kind, Value::List(vec![Value::Str(key.to_owned()), Value::Int(i64::from(modifiers))])));
        }
        out
    }

    /// Whether the node that would handle this key actually asked for it.
    ///
    /// A `key_down` handler used to receive *every* key, which is why a
    /// dialog could not simply listen for `Escape`: it would hear each letter
    /// typed into the field inside it too, and a handler that closes on a key
    /// press would close on all of them. A `keys` prop names what the node
    /// wants and the rest are not sent — fewer round trips, and a server
    /// handler that cannot fire on a key it never asked for. A node without
    /// the prop still hears everything, so nothing that worked stops.
    fn wants_key(&self, f: NodeIx, key: &str) -> bool {
        let Some(target) = self.ancestor_keyed(f) else {
            return true;
        };
        let Some(atom) = self.session.atom_id("keys") else {
            return true;
        };
        match self.session.node(target).and_then(|n| n.prop(atom)) {
            Some(Value::List(want)) => want.iter().any(|k| matches!(k, Value::Str(s) if s == key)),
            _ => true,
        }
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
    pub fn paint(&mut self, device_w: u32, device_h: u32) -> Arc<DrawList> {
        // A frame owed to a spin alone, with nothing having reached the
        // driver since the last one, is the last list again: the vertex
        // stage turns the node from the clock the window passes it (03 §5),
        // so walking the tree would produce the same quads. Where the
        // driver shares the window's process — every platform but Linux —
        // this is what keeps a spinner from costing a layout and a paint
        // thirty times a second.
        if !self.touched {
            if let Some(c) = &self.cached {
                if c.until.map_or(true, |u| self.now < u) {
                    self.redraw = false;
                    // Due at its cadence, and no later than the list runs
                    // out: the settle, the timer or the report it is
                    // waiting on is a real paint's to make.
                    self.next_due = match (c.cadence, c.until) {
                        (Some(cadence), Some(until)) => Some((self.now + cadence).min(until)),
                        (Some(cadence), None) => Some(self.now + cadence),
                        (None, until) => until,
                    };
                    self.spin_repeats = self.spin_repeats.saturating_add(1);
                    return Arc::clone(&c.list);
                }
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
        // A drag captured the pointer and coalesced its moves: the server
        // hears one a frame rather than one per OS sample, and it hears it
        // here. Held back until the button came up instead, anything whose
        // shape only the server knows could not follow the hand -- a
        // slider hides that by moving its own thumb locally, a split pane
        // has nothing to hide it with and sat where it started.
        let dragged = self.flush_drag_move();
        self.pending.extend(dragged);
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
        // A transition that has run its course paints its final colours
        // this frame -- the record's own, with no clock on them.
        self.anims.retain(|(ix, a)| !a.done(now) && self.session.node(*ix).is_some());
        let anims: Vec<(NodeIx, GpuAnim)> = self.anims.iter().map(|(ix, a)| (*ix, a.to_gpu(now))).collect();
        // A glide the vertex stage carries (04 §7): the content starts as
        // far from where the layout put it as the landing is from where
        // the glide began.
        let glides: Vec<(NodeIx, Glide)> = self
            .scroll_anim
            .filter(|a| a.gpu && a.armed)
            .map(|a| {
                let from = (a.to.0.round() - a.from.0, a.to.1.round() - a.from.1);
                (a.node, Glide { from, t0: -now.saturating_duration_since(a.start).as_secs_f32(), dur: a.duration.as_secs_f32(), smooth: a.smooth })
            })
            .into_iter()
            .collect();
        let editing = self.editing();
        // Hold, then fade: while it is held there is nothing to redraw
        // until the fade starts, so come back then rather than every frame
        // of the wait.
        let bars = self.scrollbars();
        if let Some((_, at)) = self.scrolled {
            let due = if self.now.saturating_duration_since(at) <= BAR_HOLD { at + BAR_HOLD } else { self.now + SPIN_FRAME };
            self.next_due = Some(self.next_due.map_or(due, |d| d.min(due)));
        }
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
            anims: &anims,
            glides: &glides,
            cache: &mut self.paint_cache,
            editing,
            now: self.now.saturating_duration_since(self.epoch).as_secs_f32(),
            scrollbar_hot: self.pointer.dragging_thumb.map(|(s, _)| s).or(self.pointer.over_scrollbar),
            scrollbars: &bars,
        });
        if list.wants_frame && self.next_due.is_none() {
            self.next_due = Some(now + SPIN_FRAME);
        }
        self.session.clear_all_dirty();
        let text_stats = self.text.stats();
        trace(|| {
            let st = self.layout.stats();
            let layout =
                if relaid { format!("layout {layout_ms:.1} ms ({} measures, {} memo hits, {} rows measured)", st.measures, st.memo_hits, st.rows_measured) } else { "layout cached".to_owned() };
            let was = self.last_text_stats;
            let (pc, pw) = (self.paint_cache.stats(), self.last_paint_stats);
            format!(
                "paint: {layout}, paint {:.1} ms, {} quads, text this frame: {} hits, {} misses, {} reused, {} evicted; retained: {} as were, {} moved, {} rebuilt",
                t_layout.elapsed().as_secs_f64() * 1e3 - layout_ms,
                list.quads.len(),
                text_stats.hits.saturating_sub(was.hits),
                text_stats.misses.saturating_sub(was.misses),
                text_stats.reused.saturating_sub(was.reused),
                text_stats.evictions.saturating_sub(was.evictions),
                pc.hits.saturating_sub(pw.hits),
                pc.translated.saturating_sub(pw.translated),
                pc.rebuilt.saturating_sub(pw.rebuilt),
            )
        });
        self.last_text_stats = text_stats;
        self.last_paint_stats = self.paint_cache.stats();
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
        for due in others.into_iter().flatten() {
            self.next_due = Some(self.next_due.map_or(due, |d| d.min(due)));
        }
        // What this list cannot say for itself. A transition of the blur
        // is interpolated here, frame by frame, and a scroll in flight
        // moves the offset the layout bakes in: either one makes the next
        // frame a different list. Otherwise the list is the frame until
        // the last transition in it ends or the next timer fires -- the
        // colours between are the vertex stage's, from the list's own
        // clock, as a spin's angle is -- so the window can draw it again
        // without asking for it, at the transition's cadence, the spin's,
        // or not at all. Anything that reaches the driver puts an end to
        // that, because it may change what the tree paints.
        let cpu_owed = self.anims.iter().any(|(_, a)| !a.gpu()) || self.scroll_anim.is_some_and(|a| !a.gpu) || list.cpu_bound;
        let motion_end = self.anims.iter().map(|(_, a)| a.start + a.duration).chain(self.scroll_anim.map(|a| a.start + a.duration)).max();
        // A sound or a picture playing reports its position four times a
        // second (03 §7, §8), from a paint: the list holds until the next
        // report, whenever the window next draws it, and no frame is asked
        // for on its account -- that would be a wake-up a playing tab did
        // not have before.
        let report_due = (!self.mixer.is_empty() || !self.players.is_empty()).then(|| self.audio_reported.map_or(now, |t| t + Duration::from_millis(250)));
        let until = others.into_iter().flatten().chain(report_due).chain(motion_end).min();
        let cadence = if !self.anims.is_empty() || self.scroll_anim.is_some() { Some(Duration::from_millis(16)) } else { list.wants_frame.then_some(SPIN_FRAME) };
        list.gpu_only = !cpu_owed;
        list.repeat_until_ms = until.map_or(u32::MAX, |u| u32::try_from(u.saturating_duration_since(now).as_millis()).unwrap_or(u32::MAX));
        list.serial = next_serial();
        let list = Arc::new(list);
        self.cached = (!cpu_owed).then(|| Cached { list: Arc::clone(&list), painted_at: now, until, cadence });
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
        if self.session.root().is_none() {
            self.players.clear();
            return;
        }
        let known = *self.session.atoms();
        let (a_src, a_playing, a_loop, a_position) = (known.src, known.playing, known.loop_, known.position);
        let mut live: Vec<u32> = Vec::new();
        let mut work: Vec<(u32, Hash, bool, bool, Option<i64>)> = Vec::new();
        for ix in self.session.media().iter().copied() {
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
        self.paint_cache.clear();
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
        if self.session.root().is_none() {
            self.mixer.retain(&[]);
            self.audio_at.clear();
            self.audio_src.clear();
            return;
        }
        let known = *self.session.atoms();
        let (a_src, a_playing, a_volume, a_loop, a_position) = (known.src, known.playing, known.volume, known.loop_, known.position);
        let mut live: Vec<u32> = Vec::new();
        let mut work: Vec<(u32, Hash, Control, Option<i64>)> = Vec::new();
        for ix in self.session.media().iter().copied() {
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
        let playing: Vec<u32> = self
            .session
            .media()
            .iter()
            .filter_map(|ix| self.session.node(*ix))
            .filter(|n| n.kind == NodeKind::Audio && n.handler(EventKind::TimeUpdate).is_some())
            .map(|n| n.id)
            .filter(|id| self.mixer.playing(*id))
            .collect();
        // The same for pictures: the node asks, the client answers.
        let moving: Vec<(u32, u64, u64)> = {
            self.session
                .media()
                .iter()
                .filter_map(|ix| self.session.node(*ix))
                .filter(|n| n.kind == NodeKind::Video && n.handler(EventKind::TimeUpdate).is_some())
                .filter_map(|n| {
                    let (hash, player) = self.players.get(&n.id)?;
                    let movie = self.movies.get(hash)?.as_ref()?;
                    player.playing.then(|| (n.id, player.position_ms(), movie.duration_ms()))
                })
                .collect()
        };
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
        if self.session.root().is_none() {
            self.wakes.clear();
            return;
        }
        let Some(atom) = self.session.atoms().wake else {
            self.wakes.clear();
            return;
        };
        let now = self.now;
        let asked: Vec<(u32, Duration)> = self
            .session
            .wakers()
            .iter()
            .filter_map(|ix| self.session.node(*ix))
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

    /// How many frames were answered from the last list rather than
    /// painted.
    pub fn spin_repeats(&self) -> u64 {
        self.spin_repeats
    }

    /// How many layouts were computed so far.
    pub fn relayouts(&self) -> u64 {
        self.relayouts
    }

    /// How old the list last handed out is, in seconds: its own clock
    /// starts at the paint that produced it, and the vertex stage animates
    /// from there (03 §5). Zero until something was painted.
    pub fn list_age(&self, now: Instant) -> f32 {
        self.cached.as_ref().map_or(0.0, |c| now.saturating_duration_since(c.painted_at).as_secs_f32())
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
    /// Something the layout reads changed.
    touched: bool,
    /// Something only the painter reads changed -- a hover lit a node --
    /// so a repaint is owed and no layout is.
    repaint: bool,
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
        if let Some(undo) = &mut self.undo {
            undo.push(Undo::Style(ix, self.session.node(ix).map_or(0, |n| n.style)));
        }
        match self.session.set_style_local(ix, style) {
            Some(true) => {
                self.repaint = true;
                true
            }
            Some(false) => {
                self.touched = true;
                true
            }
            None => false,
        }
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

/// The last segment of what a file dialog called a file, on either
/// separator. Spec 06 §3: a path is the person's business, and the server
/// is told the name it picked, not where it lives.
fn basename(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}
