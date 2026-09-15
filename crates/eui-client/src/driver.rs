//! The session driver: everything the client does that is not a window or a
//! socket. Frames in, frames out; input in, events out; a draw list when
//! asked. Pure enough to be tested without a display or a network.

use crate::time::{Duration, Instant};
use std::collections::HashMap;
use std::sync::Arc;

use eui_audio::Control;
use eui_layout::{Env, FontSpec, Layout, Rect, Size, TextMeasurer, TextMetrics};
use eui_proto::limits::{DEFAULT_UPLOAD_BYTES, MAX_SAVE_BYTES, MAX_TRANSFER_CHUNK_BYTES, MAX_TREE_DEPTH, MAX_UPLOAD_BYTES};
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

/// Where the bytes an [`FileAsk`] wants are to come from.
///
/// The distinction is not cosmetic and it is not the platform's to make:
/// reading what someone already has and making something new with a camera
/// are different powers, gated by different capabilities (01 §2.1). It
/// rides on `pick` rather than on a prop of its own because everything
/// after the sheet closes is identical — a name, a size, and `Upload`
/// frames (01 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PickSource {
    /// Whatever the person already has: the file system, or on a phone the
    /// photo library. Needs `fs.pick`.
    #[default]
    Held,
    /// A picture taken now. Needs `camera`, and nothing on the filesystem
    /// is read.
    Camera,
    /// A recording made now. Needs `microphone`.
    ///
    /// A *finished* recording, which is the whole reason this fits: what
    /// comes back is a file with an end, carried by `Upload` like every
    /// other file (01 §6). Listening to a microphone as it runs is a
    /// stream, has no transport in this protocol and is not this
    /// (08 §9.1).
    Microphone,
}

impl PickSource {
    /// The capability without which there is no sheet and no diagnostic.
    pub const fn capability(self) -> u32 {
        match self {
            Self::Held => caps::FS_PICK,
            Self::Camera => caps::CAMERA,
            Self::Microphone => caps::MICROPHONE,
        }
    }

    /// For the pipe between the worker and the window.
    pub const fn to_u8(self) -> u8 {
        match self {
            Self::Held => 0,
            Self::Camera => 1,
            Self::Microphone => 2,
        }
    }

    /// Back from the pipe. An unknown source is `Held`, which is the one
    /// that asks for the least.
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Camera,
            2 => Self::Microphone,
            _ => Self::Held,
        }
    }
}

/// A scan the tree asked for and the window has not started yet.
///
/// The same shape as a [`FileAsk`] and for the same reason: on iOS a scan
/// **is** a system sheet, raised by `NFCNDEFReaderSession`, and it may only
/// be raised because a person did something. Android has no sheet and
/// listens while the activity is in front, but the rule is the client's
/// rather than the platform's, so both behave alike: a scan begins on an
/// activation and on nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfcAsk {
    /// The driver's own name for this scan.
    pub token: u32,
    /// The node that asked, by id.
    pub node: u32,
    /// What to tell the person the scan is for. iOS shows it; Android has
    /// nowhere to put it and the application should say it in the tree.
    pub prompt: String,
}

/// One record off a tag.
///
/// Deliberately not a general NDEF model: a client that shipped one would
/// be parsing a format a server chose, in a process that on both phones has
/// no worker to be confined to (08 §10). `kind` is `text`, `uri`, `mime:…`
/// or `raw`, and `payload` is what that kind means — UTF-8 for the first
/// three, lower-case hex for the last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NfcRecord {
    /// What `payload` is.
    pub kind: String,
    /// The record, as `kind` says to read it.
    pub payload: String,
}

/// Where the machine is, as the platform reported it.
///
/// Handed in by the window — the driver has no radio, no GPS and, in a
/// worker, no way to reach either. What it does own is the rounding: a fix
/// is coarsened on the way out (see [`COARSE_STEPS_PER_DEGREE`]) and the server is
/// never told more than that.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    /// Degrees north, WGS 84.
    pub latitude: f64,
    /// Degrees east, WGS 84.
    pub longitude: f64,
    /// The radius the platform claims, in metres.
    pub accuracy_m: f64,
}

impl Fix {
    /// This fix as a server may hear it: rounded, and honest about how
    /// rounded.
    fn coarse(self) -> [f64; 3] {
        let round = |v: f64| (v * COARSE_STEPS_PER_DEGREE).round() / COARSE_STEPS_PER_DEGREE;
        [round(self.latitude), round(self.longitude), self.accuracy_m.max(COARSE_ACCURACY_M)]
    }
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
        /// Where the bytes come from.
        source: PickSource,
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
    /// The person asked to go back (06 §1.3).
    ///
    /// One input for four gestures — a system back button, the mouse's
    /// fourth button, `Alt+Left`, and a swipe from the leading edge — because
    /// §5's rule that a server cannot tell a finger from a mouse holds just
    /// as well here, and 00 refuses the fingerprinting surface that telling
    /// them apart would be.
    Back,
    /// The window is now `w × h` logical px at `scale` device px per logical.
    ///
    /// `scale` is what the *page* is drawn at, which is the display's own
    /// density times whatever zoom the window applies: a driver has no
    /// notion of zoom and does not need one, because everything it would
    /// change is already a function of this number.
    Resized(f32, f32, f32),
    /// The bottom `px` logical px of the window are underneath something
    /// the platform put there, and a soft keyboard is the only thing that
    /// ever is.
    ///
    /// The page is laid out into what is left, exactly as though the window
    /// had got shorter — which is what Android does for itself when
    /// `adjustResize` is allowed to work, and what iOS never does. Scrolling
    /// alone cannot answer this: a field at the end of a page is at the end
    /// of its scroller's travel too, and no amount of scrolling lifts it out
    /// from under a keyboard. Shortening the page is what gives the scroller
    /// the room to.
    ///
    /// A platform whose window really does get shorter sends nothing: the
    /// `Resized` it already sends says all of it, and the bottom of a window
    /// that shrank is not covered by anything.
    Covered(f32),
    /// The viewer changed palette.
    Mode(ThemeMode),
    /// The window lost focus.
    Unfocused,
    /// The window has the input again.
    ///
    /// Its twin has been here since the beginning; this one arrived with
    /// `location`, which is the first thing the client does that must stop
    /// when the window is not in front and start again when it is.
    Refocused,
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
    /// Whether `x`/`y` mean anything: a pointer that has left the window has
    /// no position, and a panel that follows it must not be placed at one.
    inside: bool,
    /// The drag this press became, if it became one (06 §6).
    drag: Option<Drag>,
    /// The scroller a drag is currently running along the edge of, and when it
    /// was last stepped. 06 §6.4: the offset moves every frame and the
    /// `scroll` event goes once, when the movement stops.
    autoscroll: Option<(NodeIx, Instant)>,
}

/// How long a contact must be held still before it becomes a grab, per
/// 06 §5.1. Both phones use half a second for their own long press and a
/// person's expectation is theirs, not ours.
const TOUCH_HOLD_MS: u64 = 500;

/// How close to a scroller's edge a drag has to be held before the view
/// starts moving, in logical pixels — or a fifth of the viewport, whichever is
/// less. The fraction is what stops a short list scrolling from its middle
/// (06 §6.4).
const AUTOSCROLL_BAND: f32 = 48.0;

/// How fast it moves at the very edge, in logical pixels a second, ramping
/// from nothing at the band's inner edge.
const AUTOSCROLL_MAX: f32 = 900.0;

/// What a key means on the way to the focused node, per 03 §3.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Claim {
    /// Nobody on the path is listening for this key — or one is, and named
    /// the keys it wants, and this is not among them. It is not sent.
    Ignored,
    /// A keyed node hears it, and the client's own meaning stands as well.
    Reported,
    /// A keyed node hears it, and the client's own meaning is withheld.
    Claimed,
}

/// A drag in flight (06 §6). The client owns the whole of the hand and tells
/// the server only what changed: one `drag_start`, a `drag_over` per boundary
/// crossed, one `drop`.
///
/// The source is held by **key atom**, not by `NodeIx` or id, and that is the
/// correctness core. A move between containers is a removal and an insertion
/// (02 §5), so the node is destroyed and rebuilt under the hand and its id
/// changes; the key is the only thing that survives, and `by_key` re-resolves
/// it for free. 03 §3.4 requires a draggable node to carry one.
#[derive(Debug, Clone, Copy)]
struct Drag {
    /// The key atom of the node in the hand.
    source: u32,
    /// Where the press landed, for the slop.
    from: (f32, f32),
    /// Whether the slop has been crossed, or a handle or a hold took it
    /// straight there. Before this nothing has been emitted and the gesture
    /// is still a click if it ends here.
    grabbed: bool,
    /// The target and slot last reported, so a drag that crosses no boundary
    /// says nothing (06 §2).
    sent: Option<(u32, i64)>,
}

/// How far a finger may wander from where it landed and still be a press
/// rather than a scroll, in logical pixels. Below this a tap that wobbles
/// still clicks what it landed on; above it, the finger is carrying the
/// view and the press it began with is taken back.
/// The gap [`Driver::reveal`] leaves between what it reveals and the edge it
/// reveals it from. A field flush against the top of a soft keyboard reads
/// as half covered even when every pixel of it is there.
const REVEAL_MARGIN: f32 = 8.0;

const TOUCH_SLOP: f32 = 8.0;

/// How far in from the leading edge a contact may land and still belong to
/// the navigator rather than to what is under it, in logical px (06 §5).
///
/// Twenty, which is the width of the bezel a thumb finds without being
/// aimed, and narrow enough that what it takes from the application is a
/// strip nobody puts a control against. Wider would take sliders; narrower
/// would have to be looked at to be hit, and a gesture you have to look at
/// is a button with extra steps.
const TOUCH_EDGE: f32 = 20.0;

/// How fast a back-swipe has to be still moving when the finger leaves the
/// glass for the page to go anyway, in logical px per millisecond.
///
/// The same shape as a fling (06 §5 step 6): a stroke that was still going
/// carries on, and one that was placed and held does not. Half a pixel a
/// millisecond is a deliberate flick and not a hand coming to rest.
const POP_FLING: f32 = 0.5;

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
    /// It landed on the leading edge of a page that can be gone back from,
    /// and went inwards: the finger is carrying that page off, and the back
    /// is reported when it lets go past halfway (06 §5).
    Popping,
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
    /// When an undecided contact becomes a held one (06 §5.1). Cleared the
    /// moment it wanders or lifts, because neither of those is a hold.
    hold: Option<Instant>,
    /// It landed inside the leading-edge strip of a page that can be gone
    /// back from, so a stroke inwards is a back rather than whatever is
    /// under it (06 §5).
    edge: bool,
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

/// Which way a track's line runs (03 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrackAxis {
    X,
    Y,
}

/// A node declaring `track`, with what its line measures.
#[derive(Debug, Clone, Copy)]
struct Track {
    node: NodeIx,
    axis: TrackAxis,
    min: i64,
    max: i64,
    step: i64,
}

impl Track {
    /// The nearest whole `step` from `min`, inside the ends. The one place
    /// a value is quantised, so the pointer, the keyboard and the server's
    /// own number all land on the same set.
    fn snap(&self, v: i64) -> i64 {
        let (lo, hi) = (self.min.min(self.max), self.min.max(self.max));
        let v = v.clamp(lo, hi);
        let step = self.step.max(1);
        let k = (v - self.min).saturating_add(step / 2).div_euclid(step);
        (self.min + k.saturating_mul(step)).clamp(lo, hi)
    }
}

/// Where a track's handles are. `hi` is `None` for a one-thumb track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrackValue {
    lo: i64,
    hi: Option<i64>,
}

/// The pieces of a track the client places.
#[derive(Debug, Default, Clone)]
struct TrackParts {
    groove: Option<NodeIx>,
    fill: Option<NodeIx>,
    thumbs: Vec<NodeIx>,
}

/// The travel a track's handles have, measured this frame.
#[derive(Debug, Clone, Copy)]
struct TrackGeom {
    /// The track's leading edge along the axis.
    start: f32,
    /// Its whole extent along the axis.
    extent: f32,
    /// Where the centres begin: half a thumb in from `start`.
    origin: f32,
    /// How far they travel: the extent less one thumb.
    span: f32,
    /// A thumb's size along the axis.
    thick: f32,
    /// The middle of the line across the axis.
    cross_mid: f32,
}

/// Which handle of a range a gesture took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Handle {
    Lo,
    Hi,
}

/// The track the client is driving -- under the hand, or under the arrows.
///
/// 06 §6's argument, for a value rather than a position: the client owns
/// the gesture and the server is told the number it landed on.
#[derive(Debug, Clone)]
struct TrackDrag {
    track: Track,
    /// Chosen by the press and kept for the gesture: a handle that changed
    /// identity mid-drag would be a different thing under a finger that
    /// never left it.
    handle: Handle,
    /// Where in the thumb the pointer took hold, so nothing jumps to centre
    /// itself under the hand.
    grab: f32,
    /// What the client is drawing.
    value: TrackValue,
    /// 06 §2's "the last it sent" -- this track's `Edit::seed`.
    ///
    /// What was *sent*, never what the server last said. Re-seeding this
    /// from an answering batch is a livelock: a server that clamps 80 to 75
    /// would be told 80 again by a hand still at 80, once per round trip,
    /// for as long as the hand stayed there.
    sent: Option<TrackValue>,
    /// A hand is on it. False for a keyboard-only change, and what decides
    /// whether an answering batch may move the value (07 §6).
    holding: bool,
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
    /// When the value last moved away from `seed`. `None` means the field
    /// agrees with the server and owes it nothing (06 §2).
    ///
    /// Armed on *disagreement* rather than on "a key arrived", which is what
    /// makes "never report a value that has not moved" true by construction:
    /// type a character and take it back, and the deadline disarms itself.
    typed_at: Option<Instant>,
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

/// A page that has left the tree and is still being drawn (03 §5).
///
/// The quads it painted, not the nodes it was made of. Nothing here can be
/// laid out, hit-tested, focused, woken, played or read out, because there is
/// nothing here to do any of that to — "a departing page is inert" is a fact
/// about the representation rather than a rule anything has to enforce. The
/// memory is what was on screen, so a ten-thousand-row table costs its forty
/// visible rows.
#[derive(Debug, Clone)]
struct Departing {
    /// What it painted, exactly as it painted it.
    quads: Vec<eui_render::Quad>,
    /// The clip rectangle those quads were drawn under.
    clip: [u32; 4],
    /// Where it is going, and when.
    go: Move,
    /// Over the page arriving, or under it. A pop uncovers what was beneath,
    /// so the page leaving is on top; a push covers it, so it is not.
    over: bool,
    /// The atlas generation its glyph uvs were taken from. An atlas that
    /// grows or is cleared forgets every glyph in it and hands the same
    /// coordinates to different pixels, so a page frozen before that has to
    /// be let go rather than drawn wrong.
    atlas: u32,
    /// The framebuffer it was painted for. A window resized mid-transition
    /// has nowhere sensible to put a picture of the old one.
    size: (u32, u32),
}

/// One subtree on the move (03 §5), as the driver keeps it: where it starts,
/// where it ends, and either a clock or the hand.
///
/// `from` and `to` are `dx`, `dy` in logical px, a uniform scale and an
/// opacity — the four the vertex stage interpolates. Absolute times, like
/// every other animation here: there is no delta, so a frame that is skipped
/// costs nothing and a frame that is repeated is still right.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Move {
    from: [f32; 4],
    to: [f32; 4],
    start: crate::time::Instant,
    duration: Duration,
    /// `0` standard, `1` decelerate (arriving), `3` accelerate (leaving).
    curve: u32,
    /// Set while a gesture is driving it: the fraction the hand is holding
    /// it at, and the clock is not read at all (06 §5).
    held: Option<f32>,
}

impl Move {
    fn done(&self, now: crate::time::Instant) -> bool {
        self.held.is_none() && now.saturating_duration_since(self.start) >= self.duration
    }

    fn to_paint(self, now: crate::time::Instant) -> eui_render::Mover {
        eui_render::Mover { from: self.from, to: self.to, t0: -now.saturating_duration_since(self.start).as_secs_f32(), dur: self.duration.as_secs_f32(), curve: self.curve, held: self.held }
    }
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
        let nanos = crate::time::SystemTime::now().duration_since(crate::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
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
/// Half the caret's blink, in seconds: up for this long, down for this
/// long. The number every desktop uses, and the one the caret is up for
/// after each keystroke.
const CARET_BLINK: f32 = 0.53;
/// How long the caret goes on blinking with nothing happening, before it
/// settles and stays up.
///
/// A blink is a wake-up twice a second, and 10 §1's idle budget is zero of
/// them; a caret that blinked for as long as a field was focused would turn
/// every window left open on a form into a process that never sleeps. So it
/// blinks while somebody is plainly there and stops when they are not,
/// which is GTK's `gtk-cursor-blink-time` and its default to the second.
/// A caret left up is the honest resting state: it still says where typing
/// would land.
const CARET_BLINK_FOR: f32 = 10.0;
/// How long a scrollbar stays up once the scrolling stops, and how long it
/// then takes to go. A bar reports a movement, so there is nothing for one
/// to say about a page that is sitting still — and a strip of furniture
/// down the right of every scroller is a strip of furniture in every
/// screenshot of every page. Overlay bars, on Safari's timings.
const BAR_HOLD: Duration = Duration::from_millis(800);
const BAR_FADE: Duration = Duration::from_millis(400);
const WINDOW_SETTLE: Duration = Duration::from_millis(120);
/// How long a field must be quiet before it tells the server what is in it
/// (06 §2). Long enough that a word typed at speed is one event and not
/// eight; short enough that someone who stops to look at the screen has
/// already been answered.
const CHANGE_IDLE: Duration = Duration::from_millis(300);
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

/// The fastest a node may be told where the machine is.
///
/// A second, not the hundred milliseconds a clock gets. No positioning
/// hardware moves faster than this in any way that means anything, and the
/// cost of asking is a radio rather than a timer — which is battery on the
/// two platforms this exists for.
const MIN_LOCATE_MS: u64 = 1_000;

/// How many nodes may ask at once. One page wants one fix; four is the
/// same allowance a clock gets and is already generous.
const MAX_LOCATORS: usize = 2;

/// How coarse a coarse location is, as steps of a degree.
///
/// `01 §2.1` grants the power to read a **coarse** location and there is no
/// second capability for a fine one, so the client rounds before anything
/// leaves it: a thousandth of a degree is about 110 m at the equator and
/// less everywhere else. The rounding happens here rather than at the
/// platform because this is the side a server cannot argue with.
///
/// Multiply, round, divide — in that order. Dividing by `0.001` and
/// multiplying back puts the error straight back into the answer: it turns
/// 48.8583721 into 48.858000000000004, which is not three decimal places
/// and is a different number from the one anyone would write down.
const COARSE_STEPS_PER_DEGREE: f64 = 1_000.0;

/// The floor put under a reported accuracy, in metres. A fix rounded to
/// [`COARSE_DEGREES`] is not better than this, whatever the receiver said,
/// and telling a server otherwise would be a lie the client authored.
const COARSE_ACCURACY_M: f64 = 100.0;

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

/// A scene's asset, checked in the worker and ready for the window.
///
/// The window uploads a mesh without reading it and compiles a module
/// without trusting it. Both halves of that are load-bearing: the bytes here
/// have passed the checks no driver performs, and the window still re-parses
/// what it is given rather than believing this enum.
#[derive(Debug, Clone, PartialEq)]
pub enum SceneAsset {
    /// Geometry whose every index is already known to be in range.
    Mesh(crate::mesh::Mesh),
    /// WGSL that has passed `eui-shader`'s verifier.
    Shader(String),
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
    /// Scene assets checked and waiting for the window, which owns the GPU.
    scene_assets: Vec<(Hash, SceneAsset)>,
    size: Size,
    scale: f32,
    pointer: Pointer,
    /// The finger being followed, when the window reports contacts rather
    /// than a mouse (spec 06 §5).
    touch: Touch,
    focused: Option<NodeIx>,
    /// Focus came from the keyboard or the server: draw the ring (spec 03 §3).
    focus_visible: bool,
    /// How much of the window's bottom edge a soft keyboard is over
    /// ([`Input::Covered`]). Zero everywhere a keyboard is a piece of
    /// hardware.
    ///
    /// `size` is the window less this, because that is the part a page may
    /// use; `window_h` is what the window actually is, kept so that a
    /// keyboard arriving and leaving can be taken off and put back without
    /// asking the window its size again.
    covered: f32,
    /// The window's own height, before [`Self::covered`] is taken off it.
    window_h: f32,
    /// Running transitions (spec 03 §5), the clock they run on, and when the
    /// next frame is due — the only reason the window ever wakes itself.
    anims: Vec<(NodeIx, Anim)>,
    /// Subtrees on the move (03 §5): a page arriving or leaving, and the
    /// shared elements flying between them. The counterpart to `anims`, for
    /// the half of a lifecycle that is geometry rather than colour.
    movers: Vec<(NodeIx, Move)>,
    /// The page that has left the tree and is still on screen (03 §5).
    ///
    /// Its painting and not its tree. One at a time: a second navigation
    /// finishes the first at once, which makes the memory a constant instead
    /// of a function of how fast someone taps — the same shape as one contact
    /// at a time and one drag at a time.
    departing: Option<Departing>,
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
    /// The track the client is drawing for itself, if any. While it is set
    /// and held, a server batch does not move it (07 §6).
    track: Option<TrackDrag>,
    /// The composition an input method is building in the focused field.
    preedit: String,
    /// Text the person copied or cut, for the window to hand the clipboard.
    clipboard: Option<String>,
    /// What the caret's blink is timed from: the moment it last moved.
    caret_since: Instant,
    /// Where it was then — node, selection and offset — so that a caret that
    /// has not moved keeps the clock it already had.
    caret_was: Option<(NodeIx, usize, usize, usize)>,
    /// When the blink next turns over, `None` with no field focused.
    caret_due: Option<Instant>,
    /// The focused field's caret, as a 1 px box, for the IME cursor area.
    /// The platform draws its own caret at this origin; passing the whole
    /// field put it on the left of a centred run.
    ime_spot: Option<eui_layout::Rect>,
    /// Verified chunks by id; verification happens once per chunk.
    chunks: HashMap<u32, Option<eui_vm::Chunk>>,
    /// Effects of local-then-server handlers awaiting the server's answer.
    provisional: Vec<Undo>,
    /// Where in `provisional` the running drag's own `drag_start` began, so
    /// the end of the gesture can take back exactly what the grab put up —
    /// the ghost of 06 §6.3 — and leave anyone else's alone. `None` when no
    /// drag is grabbed, and whenever a batch has already reverted the lot.
    drag_provisional: Option<usize>,
    granted: u32,
    /// The node a dragged file is currently over, by id — so that leaving
    /// it can be reported once, and entering another lights only the new
    /// one (spec 03 §3.2).
    drop_over: Option<u32>,
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
    /// Scans the tree asked for and the window has not started.
    nfc_asks: Vec<NfcAsk>,
    /// Scans in flight, by token, so an answer can be matched to the node
    /// that asked and an answer for a scan nobody started is refused.
    scans: HashMap<u32, u32>,
    /// The next token, for a dialog or an upload. Never reused, so a chunk
    /// that arrives late cannot land in a later transfer.
    next_token: u32,
    /// Scrollers a batch's `ScrollTo` just moved, waiting for the layout
    /// that says how far they were allowed to go (spec 04 §7).
    ///
    /// The op carries absolute pixels, and a server cannot know the height
    /// the client gave the scroller — so a server that wants the foot of a
    /// list can only estimate it, and an estimate that is over would leave
    /// the view in blank space past the last row. The offset is therefore
    /// clamped to the content the way a wheel notch already is, which also
    /// means a hostile offset can move nothing it could not have reached.
    scroll_asked: Vec<u32>,
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
    /// Nodes asking where the machine is, with their interval and when
    /// each is next owed one. The same shape as `wakes`, for the same
    /// reason: a clock that keeps its phase across re-renders.
    locators: Vec<(u32, Duration, Instant)>,
    /// The freshest fix the window handed over, if any.
    fix: Option<Fix>,
    /// Whether the window has the input. A location is not reported while
    /// it does not (06 §3): an application does not get to follow someone
    /// around because its window is open behind something else.
    in_front: bool,
    /// Whether the tree changed since the wakes were last collected.
    wake_dirty: bool,
    locate_dirty: bool,
    /// Whether the notice of [`Self::show_stopped`] has replaced the tree,
    /// so it is mounted once and not on every frame after.
    stopped: bool,
    /// Spec 01 §2.1: the person is being asked what this application may
    /// do, and nothing has been dialled yet. `None` once they have
    /// answered, and on every session that had nothing to ask.
    consent: Option<Consent>,
    /// The answer, waiting for the window to come and take it
    /// ([`Self::take_consent`]).
    consent_said: Option<u32>,
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
            scene_assets: Vec::new(),
            size: Size::new(w, h),
            scale,
            pointer: Pointer::default(),
            touch: Touch::default(),
            focused: None,
            focus_visible: false,
            covered: 0.0,
            window_h: h,
            anims: Vec::new(),
            movers: Vec::new(),
            departing: None,
            scroll_anim: None,
            epoch: Instant::now(),
            pending: Vec::new(),
            wheel_rest: (0.0, 0.0),
            now: Instant::now(),
            next_due: None,
            edits: HashMap::new(),
            track: None,
            preedit: String::new(),
            clipboard: None,
            caret_since: Instant::now(),
            caret_was: None,
            caret_due: None,
            ime_spot: None,
            chunks: HashMap::new(),
            provisional: Vec::new(),
            drag_provisional: None,
            granted: granted & caps::ALL,
            drop_over: None,
            welcomed: false,
            session_id: None,
            acked: 0,
            file_asks: Vec::new(),
            asks: HashMap::new(),
            nfc_asks: Vec::new(),
            scans: HashMap::new(),
            next_token: 1,
            scroll_asked: Vec::new(),
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
            locators: Vec::new(),
            fix: None,
            in_front: true,
            wake_dirty: false,
            locate_dirty: false,
            stopped: false,
            consent: None,
            consent_said: None,
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

    /// Hand over the freshest fix (06 §1.2).
    ///
    /// The window calls this; the driver has no radio and, in a worker, no
    /// way to reach one. Nothing is sent from here — the fix is kept and
    /// goes out on the interval the nodes asked for, coarsened on the way.
    /// A fix that arrives without the capability is dropped rather than
    /// stored: what is not kept cannot leak.
    pub fn located(&mut self, fix: Fix) {
        if self.granted & caps::LOCATION == 0 {
            return;
        }
        self.fix = Some(fix);
        self.touched = true;
    }

    /// Whether the platform should have its positioning turned on.
    ///
    /// True exactly while a node asks, the capability is granted and the
    /// window has the input — so the window can stop the radio the moment
    /// the page that wanted it goes away, rather than leaving it running
    /// for the life of the session.
    pub fn wants_location(&self) -> bool {
        self.granted & caps::LOCATION != 0 && self.in_front && !self.locators.is_empty()
    }

    /// The fix the driver is holding, for a window that wants to show one.
    pub fn fix(&self) -> Option<Fix> {
        self.fix
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
        self.track = None;
        self.preedit.clear();
        self.chunks.clear();
        self.provisional.clear();
        self.drag_provisional = None;
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
                self.locate_dirty = true;
                self.invalidate();
                self.note_style_changes();
                self.note_entrances();
                // The batch put back every style a local handler had
                // previewed. Whatever the pointer is still over must light
                // again, so its `enter` runs once more at the next paint —
                // where hover settles anyway. `hovered()` does not move in
                // the meantime: the pointer never went anywhere.
                //
                // And so must whatever has focus. A field's focus ring is a
                // local style like any other, so a batch wiped it and only
                // hover was put back: on a page with a clock — a batch every
                // tick — the caret sat in a box that blinked its own border
                // off and on for as long as you looked at it. Focus has not
                // moved either; re-running its `focus` is the same repair.
                if self.session.take_restored_local() {
                    self.pointer.hover_pending = true;
                    self.hover_relight = true;
                    if let Some(f) = self.focused.filter(|f| self.session.node(*f).is_some()) {
                        let relit = self.emit(f, eui_proto::EventKind::Focus, eui_proto::Value::Null);
                        self.pending.extend(relit);
                    }
                }
                // Focus and edits follow the tree.
                if self.focused.is_some_and(|f| self.session.node(f).is_none()) {
                    self.focused = None;
                }
                self.edits.retain(|id, _| self.session.lookup(*id).is_some());
                // 03 §3.4 and 07 §6: a batch does not move a track under the
                // hand. The client draws its own value until the gesture ends
                // and adopts the server's after it, so a server that clamps is
                // obeyed at the end of the gesture rather than in the middle
                // of it -- a handle that jumped back under a finger that had
                // not moved would be the widget fighting the person using it.
                //
                // `sent` is deliberately *not* re-seeded from the batch: it is
                // what was sent, not what the server said. Re-seeding it would
                // have a hand still at 80 tell a server that clamps to 75 about
                // 80 again, once per round trip, for as long as it stayed there.
                if self.track.as_ref().is_some_and(|d| !d.holding || self.session.node(d.track.node).is_none()) {
                    self.track = None;
                }
                self.forget_released();
                // A `SetText` on a field the person is editing is the server
                // saying what that field now holds, and the client's buffer
                // has to agree — otherwise a composer that the handler
                // emptied on send goes on showing the message that was just
                // sent, because the local edit outlived the text under it.
                //
                // There is no race to lose here: the server only emits
                // `SetText` when its own value changed, and it never learns
                // a keystroke it was not told about.
                for op in &batch.ops {
                    if let eui_proto::Op::SetText { node, text } = op {
                        let value = match text {
                            eui_proto::TextRef::Inline(t) => t.clone(),
                            eui_proto::TextRef::Atom(a) => self.session.atom(*a).unwrap_or("").to_owned(),
                        };
                        if let Some(ix) = self.session.lookup(*node) {
                            self.reseed_edit(ix, value);
                        }
                    }
                }
                self.acked = batch.seq;
                for op in &batch.ops {
                    if let eui_proto::Op::ScrollTo { node, .. } = op {
                        self.scroll_asked.push(*node);
                    }
                }
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
    /// Drop what is keyed by the id of a node the tree no longer has.
    ///
    /// These four hang off the server's node id rather than an index, so
    /// nothing prunes them when the node goes: until now only `start_over`
    /// emptied them, and a mount is once a session. Under a navigator a page
    /// leaves on every interaction, and the entries of every list and every
    /// player it held stay behind for the life of the socket.
    ///
    /// Against the reference server that is a leak and nothing worse, because
    /// its ids only ever count up (`fresh_id`). 02 §4 permits a server to
    /// reuse an id once its node is gone, though, and against one that does,
    /// a stale entry is not merely wasted space — it is this client's answer
    /// about whichever later node was given the same number. Pruned exactly
    /// as `edits` is, a line above the call.
    fn forget_released(&mut self) {
        self.windows.retain(|id, _| self.session.lookup(*id).is_some());
        self.video_at.retain(|id, _| self.session.lookup(*id).is_some());
        self.audio_at.retain(|id, _| self.session.lookup(*id).is_some());
        self.audio_src.retain(|id, _| self.session.lookup(*id).is_some());
    }

    /// Put the page that has left back into the frame, on its way out.
    ///
    /// Its quads are added whole and given a transform slot of their own, so
    /// the list is right for every frame of the leaving and not just this
    /// one: the vertex stage moves them from the clock the window hands it,
    /// nothing is walked again, and across a worker's pipe nothing more is
    /// sent. Which is the same bargain a spin, a transition and a glide all
    /// strike, one subtree larger.
    fn splice_departing(&mut self, list: &mut DrawList, now: Instant, device: (u32, u32)) {
        let Some(d) = self.departing.as_ref() else { return };
        // Let it go rather than draw it wrong. A grown or cleared atlas has
        // handed every glyph in it a different place, and a resized window
        // has nowhere to put a picture of the old one.
        if d.go.done(now) || d.atlas != self.atlas.generation() || d.size != (self.size.w.max(0.0) as u32, self.size.h.max(0.0) as u32) {
            self.departing = None;
            return;
        }
        if list.xforms.len() >= eui_render::MAX_XFORMS {
            return;
        }
        let s = self.scale;
        let dev = |v: [f32; 4]| [v[0] * s, v[1] * s, v[2], v[3]];
        let pivot = [device.0 as f32 / 2.0, device.1 as f32 / 2.0, 0.0, 0.0];
        let m = d.go.to_paint(now);
        list.xforms.push(eui_render::Xform { from: dev(m.from), to: dev(m.to), clock: [m.t0, m.dur, m.curve as f32, 0.0], pivot });
        let Ok(slot) = u32::try_from(list.xforms.len()) else { return };
        let first = u32::try_from(list.quads.len()).unwrap_or(u32::MAX);
        let count = u32::try_from(d.quads.len()).unwrap_or(0);
        if count == 0 {
            return;
        }
        let flag = slot << eui_render::XFORM_SHIFT;
        list.quads.extend(d.quads.iter().map(|q| {
            let mut q = *q;
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "params[2] is a small flag bitfield carried as a float")]
            let flags = q.params[2] as u32;
            // It kept whatever slot it had when it was painted, and that
            // slot belonged to a list that is gone. The page it is now part
            // of is the only thing left moving it.
            q.params[2] = ((flags & !eui_render::XFORM_MASK) | flag) as f32;
            q
        }));
        let clip = u32::try_from(list.clips.len()).unwrap_or(0);
        list.clips.push(d.clip);
        let run = eui_render::Run { clip, chain: 0, first, count, scene: 0 };
        // A pop uncovers what was beneath, so the page leaving is drawn last
        // and is on top; a push covers it, so it goes first and the arriving
        // page is drawn over it. Runs are drawn in order, and putting one at
        // the front means every other run's instances have moved along by
        // the count.
        if d.over {
            list.runs.push(run);
        } else {
            // Everything already in the list is drawn over it, so it has to
            // come first — and moving instances to the front moves every
            // index that pointed into them: each run's, and the backdrop's
            // (03 §2), which names the first instance that samples it.
            for r in &mut list.runs {
                r.first = r.first.saturating_add(count);
            }
            if let Some(b) = list.backdrop.as_mut() {
                b.first = b.first.saturating_add(count);
            }
            list.quads.rotate_right(count as usize);
            list.runs.insert(0, eui_render::Run { clip, chain: 0, first: 0, count, scene: 0 });
        }
    }

    /// Take the painting of a page the tree has just let go (03 §5).
    ///
    /// The list it is taken from is the one painted *before* the batch, which
    /// is the last frame that still had the page in it — so the quads are
    /// where the page was, which is where it has to leave from.
    ///
    /// Where it goes is the mirror of where the page arriving beside it came
    /// from: a push sends the old page the way the new one did not come, and
    /// a pop is the same sentence read backwards. So the leaving record never
    /// has to name a direction, and a server never has to decide one.
    fn note_exits(&mut self) {
        let exits = self.session.take_exits();
        if exits.is_empty() {
            return;
        }
        let Some(cached) = self.cached.as_ref() else { return };
        let list = Arc::clone(&cached.list);
        // A second navigation while one is running finishes the first at
        // once. Anything else makes the memory a function of how fast a
        // person can tap.
        self.departing = None;
        for (id, motion) in exits {
            let Some(d) = list.departures.iter().find(|d| d.id == id).copied() else { continue };
            // Not all of it is here: a menu was open, and its quads are in
            // the top layer outside this span. Let the page go at once
            // rather than slide it out and leave the menu behind.
            if !d.whole || d.count == 0 {
                continue;
            }
            let Some(clip) = list.clips.get(d.clip as usize).copied() else { continue };
            let first = d.first as usize;
            let end = first.saturating_add(d.count as usize).min(list.quads.len());
            let Some(leaving_quads) = list.quads.get(first..end).map(<[_]>::to_vec) else { continue };
            if leaving_quads.is_empty() {
                continue;
            }
            let leaving = motion.mirrored();
            let Some(to) = self.leaving_towards(leaving) else { continue };
            let ms = self.resolved.motion.get(1).copied().unwrap_or(180);
            self.departing = Some(Departing {
                quads: leaving_quads,
                clip,
                // Accelerate: 05 §2 names that curve for something leaving,
                // and until now nothing had ever used it.
                go: Move { from: [0.0, 0.0, 1.0, 1.0], to, start: self.now, duration: Duration::from_millis(u64::from(ms)), curve: 3, held: None },
                over: matches!(leaving, eui_proto::Motion::Trailing | eui_proto::Motion::Bottom),
                atlas: self.atlas.generation(),
                size: (self.size.w.max(0.0) as u32, self.size.h.max(0.0) as u32),
            });
            self.next_due = Some(self.now);
            self.redraw = true;
            break;
        }
    }

    /// Where a page leaving in this direction ends up, in logical px.
    ///
    /// A third of the way, not the whole way. The page underneath is not
    /// being replaced, it is being uncovered — and something that slides out
    /// at the same speed as the thing covering it reads as two slides rather
    /// than as a stack with a depth to it. A third is what every platform
    /// that got this right settled on, and it is prose here rather than a
    /// field because it is not a decision an application should be making.
    ///
    /// **And it fades while it goes.** A page sliding out at full opacity is
    /// a second page competing with the one arriving; fading it hands the
    /// eye one thing to follow. The `accelerate` curve is what makes this
    /// read well rather than look like a dissolve: it holds the page nearly
    /// solid for the first half of the move and then lets it go. All four
    /// directions, not only the one that prompted it — a push wearing
    /// `trailing` sends the old page out `leading` (the motion is mirrored
    /// in `note_exits`), so fading one direction alone would fade the case
    /// nobody asked about and leave the one they did.
    fn leaving_towards(&mut self, motion: eui_proto::Motion) -> Option<[f32; 4]> {
        use eui_proto::Motion as M;
        let (vw, vh) = (self.size.w.max(0.0), self.size.h.max(0.0));
        let (w, h) = (vw / 3.0, vh / 3.0);
        Some(match motion {
            M::Leading => [-w, 0.0, 1.0, 0.0],
            M::Trailing => [vw, 0.0, 1.0, 0.0],
            M::Top => [0.0, -h, 1.0, 0.0],
            M::Bottom => [0.0, vh, 1.0, 0.0],
            M::Scale => [0.0, 0.0, 0.92, 0.0],
            M::Fade => [0.0, 0.0, 1.0, 0.0],
            M::Paired => return None,
        })
    }

    /// Where a node wearing this motion stands at the start of its
    /// entrance, in logical px plus a scale and an opacity.
    ///
    /// A slide is the node's own box, so a page comes in from exactly its
    /// own width away and a sheet from its own height — measured rather than
    /// named, which is what keeps a half-height sheet from travelling a
    /// whole screen. A scale is 92 % and a fade with it, which is what a
    /// fade-through between two siblings is made of.
    fn arriving_from(&mut self, ix: NodeIx, motion: eui_proto::Motion) -> Option<[f32; 4]> {
        use eui_proto::Motion as M;
        self.ensure_layout();
        let r = self.layout.rect(ix)?;
        Some(match motion {
            M::Leading => [-r.w, 0.0, 1.0, 1.0],
            M::Trailing => [r.w, 0.0, 1.0, 1.0],
            M::Top => [0.0, -r.h, 1.0, 1.0],
            M::Bottom => [0.0, r.h, 1.0, 1.0],
            M::Scale => [0.0, 0.0, 0.92, 0.0],
            // A fade is the entrance this client has always had, and a
            // pairing is resolved against its partner rather than from a
            // direction; neither is a mover.
            M::Fade | M::Paired => return None,
        })
    }

    fn note_entrances(&mut self) {
        for ix in self.session.take_entrances() {
            if self.session.node(ix).is_none() {
                continue;
            }
            let record = self.session.style_of(ix);
            let ms = record.transition.checked_sub(1).and_then(|i| self.resolved.motion.get(usize::from(i))).or_else(|| self.resolved.motion.get(1)).copied();
            let Some(ms) = ms else { continue };
            // 03 §5: an entrance that names a direction arrives from it.
            // The movement and the opacity are one transform on the whole
            // subtree, so a page slides with its contents and a fade-through
            // dims them with it — and neither goes near the entrance fade
            // below, which is what a `fade` entrance has always been and
            // stays, byte for byte.
            if record.motion != eui_proto::Motion::Fade {
                if let Some(from) = self.arriving_from(ix, record.motion) {
                    self.movers.retain(|(n, _)| *n != ix);
                    self.movers.push((ix, Move { from, to: [0.0, 0.0, 1.0, 1.0], start: self.now, duration: Duration::from_millis(u64::from(ms)), curve: 1, held: None }));
                    self.next_due = Some(self.now);
                    self.redraw = true;
                }
                continue;
            }
            let to = colors_of(&self.session, &self.resolved, &record);
            // `mix` fades an absent colour through transparent, so leaving
            // the three of them `None` is what makes this a fade rather than
            // a wash through some arbitrary starting colour.
            let from = Colors { bg: None, fg: None, border: None, opacity: 0.0, blur: 0.0 };
            self.anims.retain(|(n, _)| *n != ix);
            self.anims.push((ix, Anim { from, to, start: self.now, duration: Duration::from_millis(u64::from(ms)), curve: eui_theme::Curve::DECELERATE }));
            self.next_due = Some(self.now);
            self.redraw = true;
        }
    }

    /// Advance the clock. True when a transition frame is due, so the window
    /// should redraw; false at rest, which is almost always.
    ///
    /// The deadline that fired is **taken**. A redraw asked for is not a
    /// redraw delivered — the compositor brings it at the next refresh — and
    /// until then the window's loop passes through here again, and again,
    /// and a deadline left behind fires on every one of them. Measured on a
    /// 60 Hz screen with a page sliding: six hundred passes a second, every
    /// one asking for the same frame, and thirty-four frames delivered out
    /// of sixty. The next deadline is `paint`'s to set, which is where every
    /// other line in this file already expects it to come from.
    pub fn tick(&mut self, now: Instant) -> bool {
        self.now = now;
        match [self.next_due, self.viewport_due].into_iter().flatten().min() {
            Some(due) if now >= due => {
                self.redraw = true;
                if self.next_due.is_some_and(|d| now >= d) {
                    self.next_due = None;
                }
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
        !self.anims.is_empty() || !self.nothing_on_the_clock() || self.scroll_anim.is_some()
    }

    /// Whether nothing in flight is owed a frame — every transform there is
    /// being held by a hand rather than run by a clock (06 §5).
    ///
    /// A held one owes nothing: the finger asks for the next frame by moving,
    /// so a drag held still costs what an idle window costs, which is what
    /// 01 §5 requires of anything that is not a `wake`.
    ///
    /// **The page on its way out counts.** It is not in `movers` — it has no
    /// node to hang off — and leaving it out here was a freeze you could
    /// watch: a page removed on its own, or one whose replacement finished
    /// arriving first, stopped being owed frames while it was still half way
    /// off the screen. Nothing then dropped it either, because the only thing
    /// that drops it is a paint, so both pages sat there until some unrelated
    /// event woke the window.
    fn nothing_on_the_clock(&self) -> bool {
        self.movers.iter().all(|(_, m)| m.held.is_some()) && self.departing.as_ref().map_or(true, |d| d.go.held.is_some())
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

    /// Device px per logical px: the display's density, times the window's
    /// zoom where it has one.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Which palette this session is in.
    ///
    /// The viewer may have chosen it through the application's own control
    /// — `theme.toggle()` in a local handler — and a window that draws
    /// anything of its own behind this session has to know, or it draws it
    /// in the palette nobody asked for.
    pub fn mode(&self) -> ThemeMode {
        self.viewer.mode
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

    /// 03 §6: what an assistive technology asked for. `0` focus, `1` click,
    /// `2` move before, `3` move after — and the two moves are the keyboard's
    /// `Ctrl` with an arrow, which is what keeps the ceiling rule true.
    pub fn access_act(&mut self, ix: NodeIx, action: u8) -> Vec<Frame> {
        match action {
            1 => self.activate_node(ix),
            2 | 3 => {
                let mut out = self.focus_node(ix);
                out.extend(self.move_once(ix, if action == 2 { -1 } else { 1 }));
                out
            }
            _ => self.focus_node(ix),
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
            Input::Back => self.go_back(),
            Input::Resized(w, h, scale) => {
                let rescaled = (self.scale - scale).abs() > f32::EPSILON;
                self.window_h = h;
                // A window that got shorter for the keyboard itself reports
                // no covering, so this takes nothing off; one that did not
                // has the covering applied over whatever size it now is.
                self.size = Size::new(w, (h - self.covered).max(0.0));
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
                    // And the glyphs themselves, which are packed under the
                    // scale they were rasterised at. Kept, they would be
                    // one sheet of dead coverage per zoom level a page has
                    // been through, until the sheet fills and the next
                    // glyph is drawn as nothing.
                    self.atlas.clear();
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
            Input::Covered(px) => self.set_covered(px),
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
            Input::Refocused => {
                self.in_front = true;
                Vec::new()
            }
            Input::Unfocused => {
                // 06 §3: nothing about where the machine is while the
                // window is not the one being used. The fix is kept —
                // coming back should not cost a cold start — but no node
                // hears it, and `wants_location` goes false so the window
                // can put the radio away.
                self.in_front = false;
                // Whatever the window has lost the input to, it is not
                // holding a finger any more. The contact is forgotten here
                // rather than by a `TouchCancel`, because the window cannot
                // name the contact it never saw an id for — it only knows
                // that the input is gone.
                self.touch.end();
                // 06 §6.1 step 5: a window that loses the input has lost the
                // hand with it, and a drag it cannot see the end of must not
                // be left open.
                let mut out = self.finish_drag(true);
                out.extend(self.track_cancel());
                out.extend(self.set_focus(None, false));
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
            // The layout places a `position: pointer` panel where the hand is,
            // so it has to be told where that is before it walks — a chip shown
            // by the same hover that moved the pointer would otherwise land at
            // the origin for one frame.
            self.layout.set_pointer(self.pointer.inside.then_some((self.pointer.x, self.pointer.y)));
            self.layout.set_carrying(self.pointer.drag.is_some_and(|d| d.grabbed));
            self.layout.compute(&mut Env { session: &self.session, theme: &self.resolved, text: &mut measurer }, self.size);
            self.layout_valid = true;
            self.relayouts = self.relayouts.saturating_add(1);
        }
        self.settle_scrolls();
    }

    /// Bring every offset a `ScrollTo` set inside the content it names, now
    /// that there is a layout to measure it against. `max_y` is the same
    /// number the wheel and the arrow keys are held to.
    fn settle_scrolls(&mut self) {
        if self.scroll_asked.is_empty() {
            return;
        }
        for id in std::mem::take(&mut self.scroll_asked) {
            let Some(ix) = self.session.lookup(id) else {
                continue;
            };
            let (Some(view), Some(content)) = (self.layout.rect(ix), self.layout.content_size(ix)) else {
                continue;
            };
            let (max_x, max_y) = ((content.w - view.w).max(0.0) as i64, (content.h - view.h).max(0.0) as i64);
            let (x, y) = self.session.node(ix).map_or((0, 0), |n| n.scroll);
            let (cx, cy) = (x.clamp(0, max_x), y.clamp(0, max_y));
            if (cx, cy) != (x, y) {
                self.session.set_scroll(ix, cx, cy);
                self.invalidate();
            }
        }
    }

    // -------------------------------------------------------------- assets

    /// Hashes the tree needs and the client has not fetched: images'
    /// `src` props and chunks defined by hash. The caller fetches them from
    /// the session's origin and calls [`Self::asset_ready`].
    pub fn pending_assets(&mut self) -> Vec<Hash> {
        // What the grant actually guards is **a program the server wrote**,
        // not the node kind (08 §4). A scene that names no `shader` draws
        // with the client's own module, so nothing third-party is compiled
        // and there is nothing for a person to consent to; its mesh is
        // vertices, checked in the worker like any other asset. A scene that
        // names one is the case the capability exists for, and without the
        // grant the module is not even fetched -- an absent code path rather
        // than a check that fails, which is what 08 §3 asks for.
        let shader_atom = self.session.atoms().shader;
        let modules = self.granted & caps::SCENE != 0;
        if let Some(root) = self.session.root() {
            let wanted: Vec<Hash> = self
                .session
                .preorder(root)
                .filter_map(|ix| self.session.node(ix))
                .filter(|n| matches!(n.kind, NodeKind::Image | NodeKind::Audio | NodeKind::Video | NodeKind::Scene))
                .flat_map(|n| {
                    let scene = n.kind == NodeKind::Scene;
                    n.props.iter().filter_map(move |(a, v)| {
                        let Value::Asset(h) = v else { return None };
                        let module = scene && Some(*a) == shader_atom;
                        (!module || modules).then_some(*h)
                    })
                })
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
        // A scene's two assets are checked here, in the worker, and only
        // what passes is handed on. This is the division 08 §10 asks for:
        // the parse that meets bytes a server chose happens under seccomp,
        // and the window receives a mesh whose indices are already known to
        // be in range and a module that has already passed the verifier.
        //
        // The window will parse the WGSL again -- wgpu's front end is naga
        // too -- and that duplication is deliberate: it means a worker that
        // has been taken over cannot mark a module verified that is not.
        if crate::mesh::looks_like_mesh(&bytes) {
            match crate::mesh::decode(&bytes) {
                Ok(m) => self.scene_assets.push((hash, SceneAsset::Mesh(m))),
                Err(e) => self.assets.fail(hash, e.to_string()),
            }
        } else if eui_shader::looks_like_shader(&bytes) {
            match eui_shader::verify_asset(&bytes) {
                Ok((_, source)) => self.scene_assets.push((hash, SceneAsset::Shader(source.to_owned()))),
                // The reason is kept for the blank page and the log. It is
                // never sent back: a compiler's diagnostic names the
                // compiler, and through it the machine (08 §8).
                Err(e) => self.assets.fail(hash, e.to_string()),
            }
        }
        self.assets.deliver(hash, bytes);
        // It may be a sound or a picture a node is waiting for.
        self.audio_dirty = true;
        self.video_dirty = true;
        if let Some(img) = self.assets.image(&hash) {
            // Shrunk first when it is bigger than the sheet: the atlas
            // refuses what will not fit and remembers the refusal, so a
            // picture handed over whole would be drawn as nothing, for
            // ever, in silence.
            match crate::assets::fit_to_atlas(&img) {
                Some(small) => self.images.insert(hash, small.width, small.height, &small.rgba),
                None => self.images.insert(hash, img.width, img.height, &img.rgba),
            };
        }
        // An image's intrinsic size just changed under nodes nothing marked
        // dirty: the memoised measures cannot be trusted.
        self.layout.invalidate_all();
        self.paint_cache.clear();
        self.invalidate();
    }

    /// The scene assets checked since the last call, for the window to
    /// upload. Taken rather than borrowed, so each is handed over once.
    pub fn take_scene_assets(&mut self) -> Vec<(Hash, SceneAsset)> {
        std::mem::take(&mut self.scene_assets)
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

    // ------------------------------------------------------------- dragging
    //
    // 06 §6. The prop says what a node *is*; the handler says who *hears*.
    // Those are two different nodes found by two different walks, and keeping
    // them apart is what lets a row be draggable while the list it sits in is
    // what the drop reaches.

    /// The nearest node at or above `from` carrying `drag`, with its key —
    /// the source of a gesture that starts here. A node with no key cannot be
    /// dragged (03 §3.4): the client would have nothing to hold it by once the
    /// server rebuilt it in another container.
    fn drag_source(&self, from: NodeIx) -> Option<(NodeIx, u32)> {
        let atom = self.session.atoms().drag?;
        let mut cur = Some(from);
        while let Some(ix) = cur {
            let node = self.session.node(ix)?;
            if node.prop(atom).is_some_and(|v| !matches!(v, Value::Bool(false))) {
                return (node.key != 0).then_some((ix, node.key));
            }
            cur = if node.parent.is_some() { Some(node.parent) } else { None };
        }
        None
    }

    /// Whether a press here grabs at once rather than waiting out the slop or
    /// the hold — 03 §3.4's `drag_handle`, and 06 §5 step 2's amendment.
    fn on_drag_handle(&self, from: NodeIx) -> bool {
        let Some(atom) = self.session.atoms().drag_handle else {
            return false;
        };
        let mut cur = Some(from);
        while let Some(ix) = cur {
            let Some(node) = self.session.node(ix) else {
                return false;
            };
            if node.prop(atom).is_some_and(|v| !matches!(v, Value::Bool(false))) {
                return true;
            }
            cur = if node.parent.is_some() { Some(node.parent) } else { None };
        }
        false
    }

    /// The groups a node carries, as the prop gives them: `true` is the empty
    /// group, a string is one, a list is several.
    fn groups(node_prop: Option<&Value>) -> Option<Vec<&str>> {
        match node_prop? {
            Value::Bool(false) => None,
            Value::Bool(true) => Some(Vec::new()),
            Value::Str(s) => Some(vec![s.as_str()]),
            Value::List(items) => Some(items.iter().filter_map(|v| if let Value::Str(s) = v { Some(s.as_str()) } else { None }).collect()),
            _ => None,
        }
    }

    /// The nearest container at or above `from` that accepts what is in the
    /// hand. Matching is the client's affordance and not authorisation —
    /// 06 §4 still has the server re-derive everything.
    fn drop_target(&self, from: NodeIx, carried: NodeIx) -> Option<NodeIx> {
        let wk = self.session.atoms();
        let (drag_atom, accept_atom) = (wk.drag?, wk.accepts?);
        let want = Self::groups(self.session.node(carried)?.prop(drag_atom))?;
        let mut cur = Some(from);
        while let Some(ix) = cur {
            let node = self.session.node(ix)?;
            if let Some(takes) = Self::groups(node.prop(accept_atom)) {
                // `drag: true` is the empty group and matches `accepts: true`
                // alone; a named group matches a container naming it.
                let matched = if want.is_empty() { takes.is_empty() } else { want.iter().any(|g| takes.contains(g)) };
                if matched {
                    return Some(ix);
                }
            }
            cur = if node.parent.is_some() { Some(node.parent) } else { None };
        }
        None
    }

    /// 06 §6.2: the slot the thing in the hand would take, counted among the
    /// container's draggable items so a header or a divider is skipped and the
    /// number indexes the server's records rather than its nodes.
    ///
    /// The rule is the slot whose box contains the pointer, clamped to the
    /// ends — not the nearest boundary. Once the server previews the move by
    /// making it, the thing in the hand is under the pointer, so the slot does
    /// not change again until the pointer leaves that box; a midpoint rule
    /// oscillates there, and needs hysteresis for rows of unequal height.
    fn slot_in(&self, container: NodeIx, x: f32, y: f32) -> i64 {
        let Some(drag_atom) = self.session.atoms().drag else {
            return 0;
        };
        // 04 §7.1: a windowed list's children are not its rows — they are the
        // window's, and each says which row it is. Counting ordinals there
        // would give the position within the window, which is a number about
        // the client's scroll offset rather than about the server's records.
        let windowed = self.session.atoms().count.and_then(|a| self.session.node(container)?.prop(a)).and_then(|v| if let Value::Int(n) = v { Some(*n) } else { None });
        let vertical = self.drag_axis_is_vertical(container);
        let mut slot: i64 = 0;
        let mut seen: i64 = 0;
        let Some(node) = self.session.node(container) else {
            return 0;
        };
        for child in &node.children {
            let carries = self.session.node(*child).is_some_and(|n| n.prop(drag_atom).is_some_and(|v| !matches!(v, Value::Bool(false))));
            if !carries {
                continue;
            }
            let Some(r) = self.layout.rect(*child) else {
                continue;
            };
            let here = match windowed {
                Some(_) => self.session.atoms().row.and_then(|a| self.session.node(*child)?.prop(a)).and_then(|v| if let Value::Int(n) = v { Some(*n) } else { None }).unwrap_or(seen),
                None => seen,
            };
            let (lo, hi, at) = if vertical { (r.y, r.y + r.h, y) } else { (r.x, r.x + r.w, x) };
            if at >= lo && at < hi {
                return here;
            }
            if at >= hi {
                slot = here.saturating_add(1);
            }
            seen = seen.saturating_add(1);
        }
        // Past the last item, the slot *after* it — there are n + 1 places in a
        // list of n, and the last of them is what "drop this at the bottom"
        // means. Clamping to n − 1 instead made the end of a list the one
        // position a hand could not reach.
        let last = windowed.unwrap_or(seen).max(0);
        slot.min(last)
    }

    /// A container's `drag_axis`, defaulting to the way a column runs.
    fn drag_axis_is_vertical(&self, container: NodeIx) -> bool {
        let Some(atom) = self.session.atoms().drag_axis else {
            return true;
        };
        !matches!(self.session.node(container).and_then(|n| n.prop(atom)), Some(Value::Str(s)) if s == "x")
    }

    /// The gesture becomes a drag: 06 §6.1 step 2. Emitted once, to the
    /// nearest handler above the **source** — the node carrying `drag`, which
    /// is not in general the node the press landed on.
    fn begin_drag(&mut self) -> Vec<Frame> {
        let Some(mut d) = self.pointer.drag.filter(|d| !d.grabbed) else {
            return Vec::new();
        };
        let Some(src) = self.session.lookup_key(d.source) else {
            self.pointer.drag = None;
            return Vec::new();
        };
        d.grabbed = true;
        self.pointer.drag = Some(d);
        self.redraw = true;
        let (x, y) = (self.pointer.x, self.pointer.y);
        let payload = self.button_payload(src, EventKind::DragStart, x, y, 0);
        trace(|| format!("drag: grabbed node {:?}", self.session.node(src).map(|n| n.id)));
        // Where this gesture's own provisional changes begin. 06 §6.3 says the
        // `drag_start` handler's local chunk is what reveals the ghost; what it
        // does not say, and what nothing did, is that the end of the gesture
        // puts it away. See `finish_drag`.
        self.drag_provisional = Some(self.provisional.len());
        self.emit(src, EventKind::DragStart, payload)
    }

    /// 06 §6.1 step 3. The client resolves the target and the slot each frame
    /// and reports **only when the pair has changed** — a drag that crosses no
    /// boundary is silent, so six hundred samples down a list of forty is
    /// forty events.
    fn drag_over(&mut self) -> Vec<Frame> {
        let Some(mut d) = self.pointer.drag.filter(|d| d.grabbed) else {
            return Vec::new();
        };
        // The source may have been rebuilt under a new id by a cross-container
        // move; the key finds it either way. Gone entirely means the server
        // has moved on, and the gesture with it.
        let Some(src) = self.session.lookup_key(d.source) else {
            trace(|| "drag: the source left the tree".to_owned());
            return self.finish_drag(true);
        };
        let (x, y) = (self.pointer.x, self.pointer.y);
        // What is under the hand, blind to what is *in* it (06 §6.2).
        let found = self.layout.hit_skipping(&self.session, x, y, Some(src)).and_then(|ix| self.drop_target(ix, src));
        let Some(target) = found else {
            return Vec::new();
        };
        let slot = self.slot_in(target, x, y);
        let id = self.session.node(target).map(|n| n.id).unwrap_or(0);
        if d.sent == Some((id, slot)) {
            return Vec::new();
        }
        d.sent = Some((id, slot));
        self.pointer.drag = Some(d);
        let (lx, ly) = self.local_point(target, EventKind::DragOver, x, y);
        let payload = Value::List(vec![Value::Float(f64::from(lx)), Value::Float(f64::from(ly)), Value::Int(slot)]);
        trace(|| format!("drag: over node {id} slot {slot}"));
        self.emit(target, EventKind::DragOver, payload)
    }

    /// The end of a gesture, either way. 06 §6.1 steps 4 and 5: a cancel is an
    /// ordinary `drop` carrying `slot = -1`, so a server that handles `drop`
    /// and nothing else is correct and complete.
    fn finish_drag(&mut self, cancelled: bool) -> Vec<Frame> {
        let Some(d) = self.pointer.drag.take() else {
            return Vec::new();
        };
        if !d.grabbed {
            return Vec::new();
        }
        self.redraw = true;
        // The hand is empty: take back what the grab's own chunk put up.
        //
        // 06 §6.3 gives the reveal to the `drag_start` handler and never says
        // who takes it back, and until now nobody did — the only undo a
        // provisional change had was an incoming server batch (07 §6), so the
        // ghost stayed up until one arrived. A drag can end without one: two
        // of the returns below emit nothing at all, a target with no `drop`
        // handler above it emits nothing, and a drop that changes nothing the
        // server draws is answered with no diff. On a desktop the next thing
        // that produced a batch cleared it; on a phone, where there is no
        // `Escape`, nothing to unfocus and no pointer to move away, the ghost
        // simply stayed, following the last touch for the rest of the session.
        //
        // Before the returns, because every way out of a gesture is the end of
        // one. Only this gesture's own changes go (see `begin_drag`).
        if let Some(mark) = self.drag_provisional.take() {
            self.revert_provisional_from(mark);
        }
        // Whatever the hand was dragging the view along by, it has stopped.
        // The `scroll` that owes its landing goes with the drop.
        let mut out = match self.pointer.autoscroll.take() {
            Some((node, _)) => {
                let (sx, sy) = self.session.node(node).map(|n| n.scroll).unwrap_or((0, 0));
                self.emit(node, EventKind::Scroll, Value::List(vec![Value::Int(sx), Value::Int(sy)]))
            }
            None => Vec::new(),
        };
        let (x, y) = (self.pointer.x, self.pointer.y);
        let src = self.session.lookup_key(d.source);
        // Who hears it. A drop goes to whatever is under the hand; a cancel
        // goes to the source, and when the source is what went missing, to the
        // container last reported over — that one is still there, and it is
        // the node that would have received the drop. If neither is left there
        // is nobody to tell and nothing worth saying: the server removed the
        // row itself, so it already knows.
        let last = d.sent.and_then(|(id, _)| self.session.lookup(id));
        let under = src.and_then(|s| self.layout.hit_skipping(&self.session, x, y, Some(s)).and_then(|ix| self.drop_target(ix, s)));
        let target = if cancelled { src.or(last) } else { under.or(src) };
        let Some(target) = target else {
            return Vec::new();
        };
        // Where it lands is where the last `drag_over` said it would. That is
        // the same answer as asking the pointer again for a drag the hand
        // made, and the only answer there is for one the keyboard made.
        let reported = d.sent.filter(|(id, _)| self.session.node(target).map(|n| n.id) == Some(*id)).map(|(_, s)| s);
        let slot = if cancelled { -1 } else { reported.unwrap_or_else(|| self.slot_in(target, x, y)) };
        let (lx, ly) = self.local_point(target, EventKind::Drop, x, y);
        let payload = Value::List(vec![Value::Float(f64::from(lx)), Value::Float(f64::from(ly)), Value::Int(slot)]);
        trace(|| format!("drag: dropped in node {:?} slot {slot}", self.session.node(target).map(|n| n.id)));
        out.extend(self.emit(target, EventKind::Drop, payload));
        out
    }

    /// 03 §3's keyboard half of a drag. `None` when the key was not one of
    /// these, so the caller goes on to everything else it means.
    fn drag_key(&mut self, key: &str, modifiers: u32) -> Option<Vec<Frame>> {
        let focused = self.focused?;
        let grabbed = self.pointer.drag.is_some_and(|d| d.grabbed);
        let axis = self.session.node(focused).map(|n| n.parent).filter(|p| p.is_some());
        let vertical = axis.map_or(true, |p| self.drag_axis_is_vertical(p));
        let (back, on) = if vertical { ("ArrowUp", "ArrowDown") } else { ("ArrowLeft", "ArrowRight") };

        // A grab in progress owns the arrows, `Space`, `Escape` and the ends —
        // and only those, and only while it lasts.
        if grabbed {
            let step = match key {
                k if k == back => Some(-1),
                k if k == on => Some(1),
                _ => None,
            };
            if let Some(step) = step {
                return Some(self.move_grabbed(step));
            }
            if key == " " || key == "Enter" {
                return Some(self.finish_drag(false));
            }
            return None;
        }

        // Not grabbed, nothing is claimed unless the focused node can be moved
        // and has no `click` to stand for — that row keeps `Enter`/`Space` for
        // what it is, and reaches the grab through a handle of its own.
        let (src, key_atom) = self.drag_source(focused)?;
        if src != focused {
            return None;
        }
        if key == " " && self.session.node(focused).is_some_and(|n| n.handler(EventKind::Click).is_none()) {
            let at = self.layout.rect(focused).unwrap_or_default();
            self.pointer.drag = Some(Drag { source: key_atom, from: (at.x, at.y), grabbed: false, sent: None });
            return Some(self.begin_drag());
        }
        let _ = modifiers;
        None
    }

    /// One place along, as a whole gesture: a grab, a move and a drop, with
    /// nothing in between for anyone to be in the middle of. It is what an
    /// assistive technology's two move actions do (03 §6), and it is the same
    /// three events a hand produces, so a server needs no second path for it.
    fn move_once(&mut self, ix: NodeIx, step: i64) -> Vec<Frame> {
        let Some((src, key_atom)) = self.drag_source(ix) else {
            return Vec::new();
        };
        let at = self.layout.rect(src).unwrap_or_default();
        self.pointer.drag = Some(Drag { source: key_atom, from: (at.x, at.y), grabbed: false, sent: None });
        let mut out = self.begin_drag();
        out.extend(self.move_grabbed(step));
        out.extend(self.finish_drag(false));
        out
    }

    /// One place along, from the keyboard. The slot is the source's own plus
    /// the step, clamped, and it is reported as a `drag_over` so that a server
    /// previewing a pointer drag previews this one the same way.
    fn move_grabbed(&mut self, step: i64) -> Vec<Frame> {
        let Some(mut d) = self.pointer.drag.filter(|d| d.grabbed) else {
            return Vec::new();
        };
        let Some(src) = self.session.lookup_key(d.source) else {
            return self.finish_drag(true);
        };
        let Some(parent) = self.session.node(src).map(|n| n.parent).filter(|p| p.is_some()) else {
            return Vec::new();
        };
        let Some(target) = self.drop_target(parent, src) else {
            return Vec::new();
        };
        let at = self.layout.rect(src).unwrap_or_default();
        let here = self.slot_in(target, at.x + at.w / 2.0, at.y + at.h / 2.0);
        let want = match d.sent {
            Some((_, last)) => last.saturating_add(step),
            None => here.saturating_add(step),
        };
        let last = self.session.node(target).map_or(0, |n| n.children.len() as i64).saturating_sub(1).max(0);
        let slot = want.clamp(0, last);
        let id = self.session.node(target).map(|n| n.id).unwrap_or(0);
        if d.sent == Some((id, slot)) {
            return Vec::new();
        }
        d.sent = Some((id, slot));
        self.pointer.drag = Some(d);
        let payload = Value::List(vec![Value::Float(0.0), Value::Float(0.0), Value::Int(slot)]);
        self.emit(target, EventKind::DragOver, payload)
    }

    /// 06 §5.1: an undecided contact held still past the deadline. Three
    /// outcomes, and the first is the reason the section exists — a finger can
    /// pick a row up out of a list it would otherwise only be able to scroll.
    ///
    /// Run from `paint`, because a contact that is being held is by definition
    /// sending no events to be run from.
    fn touch_hold(&mut self) -> Vec<Frame> {
        let Some(due) = self.touch.hold.filter(|d| self.now >= *d) else {
            return Vec::new();
        };
        self.touch.hold = None;
        let _ = due;
        if self.touch.phase != TouchPhase::Undecided {
            return Vec::new();
        }
        let Some(pressed) = self.pointer.pressed_on else {
            return Vec::new();
        };
        // A thing that can be picked up is picked up. The press is not given
        // back — 06 §2, the lift is the drop — and the contact is a drag from
        // here, so `touch_move` sends moves rather than scrolling the view.
        if self.pointer.drag.is_some_and(|d| !d.grabbed) {
            trace(|| "touch: held long enough to grab".to_owned());
            self.touch.phase = TouchPhase::Dragging;
            self.pointer.pressed_on = None;
            let mut out = self.flush_coalesced_move();
            out.extend(self.begin_drag());
            out.extend(self.drag_over());
            return out;
        }
        // Otherwise the reserved kind finally gets its use. The press *is*
        // given back, the way a scroll gives it back: a long press that opened
        // a menu must not also activate what it opened from.
        if self.target(pressed, EventKind::LongPress).is_some() {
            trace(|| "touch: long press".to_owned());
            let (x, y) = (self.pointer.x, self.pointer.y);
            let payload = self.point_payload(pressed, EventKind::LongPress, x, y);
            let mut out = self.emit(pressed, EventKind::LongPress, payload);
            out.extend(self.cancel_press());
            self.touch.phase = TouchPhase::Scrolling;
            return out;
        }
        Vec::new()
    }

    /// 06 §6.4: a drag held near the edge of a scroller scrolls it, because
    /// the alternative is a list you cannot reach the bottom of without
    /// letting go. Run once a frame while a drag is live.
    ///
    /// The offset moves directly rather than through [`Self::scroll_by`],
    /// which would emit a `scroll` a frame. One goes when the movement stops,
    /// the way a glide's does and for the same reason.
    fn autoscroll(&mut self) -> Vec<Frame> {
        let live = self.pointer.drag.is_some_and(|d| d.grabbed) && self.pointer.inside;
        let step = live.then(|| self.autoscroll_step()).flatten();
        let Some((scroller, dy)) = step else {
            // Nothing to do, and if something was being done it has stopped:
            // one `scroll` says where it ended up.
            let Some((node, _)) = self.pointer.autoscroll.take() else {
                return Vec::new();
            };
            let (_, sy) = self.session.node(node).map(|n| n.scroll).unwrap_or((0, 0));
            let (sx, _) = self.session.node(node).map(|n| n.scroll).unwrap_or((0, 0));
            return self.emit(node, EventKind::Scroll, Value::List(vec![Value::Int(sx), Value::Int(sy)]));
        };
        let now = self.now;
        // The first frame of a move has no elapsed time to scale by, and a
        // stalled one must not be allowed to jump the view: a frame is worth
        // at most the 32 ms the drag's own back-pressure already waits.
        let dt = match self.pointer.autoscroll {
            Some((was, at)) if was == scroller => (now.saturating_duration_since(at).as_secs_f32()).min(0.032),
            _ => 0.0,
        };
        self.pointer.autoscroll = Some((scroller, now));
        self.redraw = true;
        self.next_due = Some(now);
        if dt <= 0.0 {
            return Vec::new();
        }
        let (sx, sy) = self.session.node(scroller).map(|n| n.scroll).unwrap_or((0, 0));
        let content = self.layout.content_size(scroller).unwrap_or_default();
        let view = self.layout.rect(scroller).unwrap_or_default();
        let max_y = (content.h - view.h).max(0.0) as i64;
        let ny = (sy + (dy * dt).round() as i64).clamp(0, max_y);
        if ny == sy {
            return Vec::new();
        }
        self.session.set_scroll(scroller, sx, ny);
        self.invalidate();
        self.scroll_touched = Some(now);
        self.scrolled = Some((scroller, now));
        // The rows moved under a pointer that did not: the slot is resolved
        // again, and §6.1's change rule keeps that to one event a row crossed.
        self.drag_over()
    }

    /// The scroller the hand is at the edge of, and how fast to move it, in
    /// logical pixels a second. `None` when the hand is not near an edge, or
    /// near one nothing can move towards.
    fn autoscroll_step(&mut self) -> Option<(NodeIx, f32)> {
        let (x, y) = (self.pointer.x, self.pointer.y);
        let mut cur = self.layout.hit(&self.session, x, y);
        while let Some(ix) = cur {
            if matches!(self.session.node(ix).map(|n| n.kind), Some(NodeKind::Scroll | NodeKind::List)) {
                if let Some(view) = self.layout.rect(ix) {
                    let band = AUTOSCROLL_BAND.min(view.h * 0.2);
                    if band > 0.0 {
                        let over = ((view.y + band) - y).max(0.0);
                        let under = (y - (view.y + view.h - band)).max(0.0);
                        let depth = if over > 0.0 { -over } else { under };
                        if depth != 0.0 {
                            let rate = (depth / band).clamp(-1.0, 1.0) * AUTOSCROLL_MAX;
                            if self.scroller_accepts(ix, 0.0, rate) {
                                return Some((ix, rate));
                            }
                        }
                    }
                }
            }
            cur = self.session.node(ix).and_then(|n| if n.parent.is_some() { Some(n.parent) } else { None });
        }
        None
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
    /// The topmost node that has said how it leaves (03 §5): what a
    /// back-swipe carries off.
    ///
    /// Deepest-last in preorder, so a sheet over a page is taken before the
    /// page under it — the thing on top is the thing a stroke means. A node
    /// that never said how it leaves is not dragged anywhere: knowing how to
    /// go is how a page volunteers for this, and it is the same byte that
    /// makes a push look like a push.
    fn topmost_leaver(&mut self) -> Option<NodeIx> {
        self.ensure_layout();
        let root = self.session.root()?;
        let mut best = None;
        for ix in self.session.preorder(root) {
            if self.layout.rect(ix).is_some() && self.session.style_of(ix).animation & eui_proto::ANIMATION_EXIT != 0 {
                best = Some(ix);
            }
        }
        best
    }

    /// How far along the back-swipe is, as a fraction of the window's width.
    fn pop_fraction(&self, x: f32) -> f32 {
        let w = self.size.w.max(1.0);
        ((x - self.touch.from.0) / w).clamp(0.0, 1.0)
    }

    /// Put the page where the finger has it.
    ///
    /// Held rather than timed: the transform's fraction is written straight
    /// in, so the list does not change and the driver asks for no frames of
    /// its own — the finger asks for the next one by moving, and a hand held
    /// still costs what an idle window costs (01 §5).
    fn hold_pop(&mut self, x: f32) {
        let Some(ix) = self.topmost_leaver() else { return };
        let Some(to) = self.leaving_towards(eui_proto::Motion::Trailing) else { return };
        let k = self.pop_fraction(x);
        self.movers.retain(|(n, _)| *n != ix);
        self.movers.push((ix, Move { from: [0.0, 0.0, 1.0, 1.0], to, start: self.now, duration: Duration::from_millis(1), curve: 1, held: Some(k) }));
        self.redraw = true;
    }

    /// Let go of a back-swipe: finish it, or put the page back.
    ///
    /// Either way the hand's fraction becomes a clock's, starting from where
    /// the finger left it — so the movement is continuous through the
    /// release rather than jumping to an end the hand never reached. The
    /// duration is scaled by what is left to travel, because a page released
    /// at nine tenths must not take the full time to cross the last tenth.
    fn release_pop(&mut self, commit: bool) {
        let Some(ix) = self.topmost_leaver() else { return };
        let Some((_, m)) = self.movers.iter().find(|(n, _)| *n == ix).copied() else { return };
        let k = m.held.unwrap_or(0.0).clamp(0.0, 1.0);
        let at = [m.from[0] + (m.to[0] - m.from[0]) * k, m.from[1] + (m.to[1] - m.from[1]) * k, 1.0, 1.0];
        let (to, left) = if commit { (m.to, 1.0 - k) } else { (m.from, k) };
        let full = self.resolved.motion.get(1).copied().unwrap_or(180);
        let ms = (f32::from(full) * left).round().max(1.0) as u64;
        self.movers.retain(|(n, _)| *n != ix);
        self.movers.push((ix, Move { from: at, to, start: self.now, duration: Duration::from_millis(ms), curve: if commit { 3 } else { 1 }, held: None }));
        self.next_due = Some(self.now);
        self.redraw = true;
    }

    /// Report that the person asked to go back (06 §1.3).
    ///
    /// To the mounted root or to nobody. §2's walk to the nearest handler
    /// has nothing to walk from — there is no node under a system back — and
    /// the root is the one node a server always knows, which is already where
    /// a component's own state lives (07 §1).
    ///
    /// A session whose root holds no `back` handler hears nothing, and the
    /// window is expected to let the platform have the gesture instead. That
    /// is not politeness: on Android the alternative is an application nobody
    /// can leave.
    pub fn go_back(&mut self) -> Vec<Frame> {
        let Some(root) = self.session.root() else { return Vec::new() };
        if self.session.handler(root, EventKind::Back).is_none() {
            return Vec::new();
        }
        self.emit(root, EventKind::Back, Value::Null)
    }

    /// Whether this session would do anything with a back.
    ///
    /// The window asks so that it can decide whether to keep the gesture or
    /// hand it to the platform, and it asks *here* rather than across the
    /// pipe per keystroke — which is what `Status` is for.
    pub fn takes_back(&self) -> bool {
        self.session.root().is_some_and(|r| self.session.handler(r, EventKind::Back).is_some())
    }

    fn emit(&mut self, from: NodeIx, kind: EventKind, payload: Value) -> Vec<Frame> {
        let Some((target, handler)) = self.target(from, kind) else {
            return Vec::new();
        };
        let node = self.session.node(target).map(|n| n.id).unwrap_or(0);
        let mut out = Vec::new();
        let name = match handler {
            Handler::Server(name) => Some(name),
            Handler::Local(chunk) => {
                match self.run_local(chunk, false, target) {
                    Ok(queued) => {
                        let state = self.root_state();
                        out.extend(queued.into_iter().map(|n| Frame::Event(EventFrame { node, event: kind, name: n, payload: state.clone() })));
                    }
                    Err(e) => eprintln!("eui: local handler {chunk}: {e}"),
                }
                None
            }
            Handler::LocalThenServer { chunk, name } => match self.run_local(chunk, true, target) {
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
    fn run_local(&mut self, chunk_id: u32, provisional: bool, here: NodeIx) -> Result<Vec<u32>, String> {
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
        let mut host = SessionHost {
            session: &mut self.session,
            here: Some(here),
            emitted: Vec::new(),
            went_back: false,
            texts: Vec::new(),
            touched: false,
            repaint: false,
            undo: provisional.then(Vec::new),
            mode: None,
        };
        let result = eui_vm::run(&verified, &mut host);
        let touched = host.touched;
        let repaint = host.repaint;
        let emitted = host.emitted;
        let went_back = host.went_back;
        let wrote = host.texts;
        let mode = host.mode;
        if let Some(undo) = host.undo {
            self.provisional.extend(undo);
        }
        // 07 §1: a chunk may set a node's text, and if that node is the one
        // being typed in, the client's buffer has to agree. Without this the
        // chunk emptied the tree and the next keystroke put the old value
        // back — so the whole point of a field a handler can clear, which is
        // a composer or a tag field, did not work.
        for ix in wrote {
            let value = self.session.text_of(ix).unwrap_or("").to_owned();
            self.reseed_edit(ix, value);
        }
        if touched {
            self.audio_dirty = true;
            self.video_dirty = true;
            self.invalidate();
        } else if repaint {
            self.redraw = true;
        }
        // 07 §3 `go_back`: a tapped back button and a swipe from the edge
        // take the same path from here on, so the two cannot come to mean
        // different things.
        if went_back {
            let back = self.go_back();
            self.pending.extend(back);
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
        // Nothing of the drag's is left to take back once the lot has gone,
        // and a mark kept past that would point into somebody else's changes.
        self.drag_provisional = None;
        self.revert_provisional_from(0);
    }

    /// The same, for the changes made since `mark` only.
    ///
    /// The end of a drag takes back what its own `drag_start` put up (06 §6.3)
    /// and must leave every other outstanding chunk exactly where it is, so it
    /// cannot use the whole-vector form above.
    fn revert_provisional_from(&mut self, mark: usize) {
        if mark >= self.provisional.len() {
            return;
        }
        let changes = self.provisional.split_off(mark);
        for change in changes.into_iter().rev() {
            match change {
                Undo::Style(ix, style) => {
                    self.session.set_style_local(ix, style);
                }
                Undo::Text(ix, text) => {
                    let value = text.map_or(String::new(), |t| match t {
                        TextRef::Inline(s) => s,
                        TextRef::Atom(a) => self.session.atom(a).unwrap_or("").to_owned(),
                    });
                    self.session.set_text_local(ix, value.clone());
                    // The same rule the other way round: a provisional clear
                    // that is taken back must take the buffer back with it, or
                    // the tree holds the old text and the buffer holds none.
                    self.reseed_edit(ix, value);
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
        self.pointer.inside = true;
        // A chip that follows the hand follows it here, not at the next
        // layout: the pointer moves many times a frame and the tree it is
        // moving over has not changed. Only the panel's origin does, which
        // is a shift of one subtree and a repaint.
        self.layout.set_carrying(self.pointer.drag.is_some_and(|d| d.grabbed));
        if self.layout_valid && self.layout.track_pointer(&self.session, x, y) {
            self.redraw = true;
        }
        // A track resolves its own gesture (03 §3.4): the value moves, the
        // parts are placed next frame, and no `pointer_move` is emitted.
        if let Some(out) = self.track_move(x, y) {
            return out;
        }
        // A thumb drag needs no layout: the scroller's box does not move.
        if let Some((scroller, grip)) = self.pointer.dragging_thumb {
            return self.drag_thumb(scroller, grip, y);
        }
        // 06 §6.1 step 2: the press becomes a drag when the pointer leaves the
        // slop, or at once if it landed on a handle. Everything after that is
        // the drag's, and the capture below never runs — a drag reports
        // `drag_over`, not `pointer_move`.
        if let Some(d) = self.pointer.drag {
            self.ensure_layout();
            if !d.grabbed {
                let handle = self.pointer.pressed_on.is_some_and(|ix| self.on_drag_handle(ix));
                let wandered = (x - d.from.0).hypot(y - d.from.1) >= TOUCH_SLOP;
                if !handle && !wandered {
                    return Vec::new();
                }
                // The press is dropped, not given back: 06 §2 says a gesture
                // that became a drag reports no `pointer_up` and no `click`,
                // and a phantom release between the press and the drop would
                // be a third thing for a server to reason about. What the
                // move had coalesced still goes, so nothing is lost.
                self.pointer.pressed_on = None;
                let mut out = self.flush_coalesced_move();
                out.extend(self.begin_drag());
                out.extend(self.drag_over());
                return out;
            }
            return self.drag_over();
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
        self.pointer.inside = false;
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

    /// The props of a node declaring `track` (03 §3.4), read once per
    /// gesture: they do not change under a hand, and the role sniffing this
    /// replaced paid for four atom lookups a frame.
    fn track_of(&self, from: NodeIx) -> Option<Track> {
        let w = self.session.atoms();
        let axis = match self.session.node(from)?.prop(w.track?)? {
            Value::Str(s) if s == "x" => TrackAxis::X,
            Value::Str(s) if s == "y" => TrackAxis::Y,
            _ => return None,
        };
        let num = |atom: Option<u32>, or: i64| -> i64 {
            let Some(a) = atom else { return or };
            match self.session.node(from).and_then(|n| n.prop(a)) {
                Some(Value::Int(i)) => *i,
                Some(Value::Float(f)) => *f as i64,
                _ => or,
            }
        };
        let step = num(w.track_step, 1).max(1);
        Some(Track { node: from, axis, min: num(w.track_min, 0), max: num(w.track_max, 100), step })
    }

    /// The track at or above `from`, which is where a press lands: the
    /// pointer is almost always over a groove or a thumb, not the track.
    fn track_above(&self, from: NodeIx) -> Option<Track> {
        let mut at = from;
        for _ in 0..MAX_TREE_DEPTH {
            if let Some(t) = self.track_of(at) {
                return Some(t);
            }
            let up = self.session.node(at)?.parent;
            if up == at {
                return None;
            }
            at = up;
        }
        None
    }

    /// The value the server last put on the track.
    fn track_value_of(&self, t: &Track) -> TrackValue {
        let lo_hi = |v: &Value| -> Option<i64> {
            match v {
                Value::Int(i) => Some(*i),
                Value::Float(f) => Some(*f as i64),
                _ => None,
            }
        };
        let at = self.session.atoms().track_value.and_then(|a| self.session.node(t.node).and_then(|n| n.prop(a)));
        match at {
            // A pair is a range; one number, or a bare number, is a slider.
            Some(Value::List(items)) => match &items[..] {
                [lo, hi, ..] => TrackValue { lo: t.snap(lo_hi(lo).unwrap_or(t.min)), hi: Some(t.snap(lo_hi(hi).unwrap_or(t.max))) },
                [lo] => TrackValue { lo: t.snap(lo_hi(lo).unwrap_or(t.min)), hi: None },
                [] => TrackValue { lo: t.min, hi: None },
            },
            Some(v) => TrackValue { lo: t.snap(lo_hi(v).unwrap_or(t.min)), hi: None },
            None => TrackValue { lo: t.min, hi: None },
        }
    }

    /// The groove, the fill and the thumbs, in document order. Descendants
    /// rather than children: a builder may wrap a thumb to position it.
    fn track_parts(&self, t: &Track) -> TrackParts {
        let mut parts = TrackParts::default();
        let Some(atom) = self.session.atoms().track_part else { return parts };
        for ix in self.session.preorder(t.node) {
            let Some(v) = self.session.node(ix).and_then(|n| n.prop(atom)) else { continue };
            match v {
                Value::Str(s) if s == "groove" => parts.groove = Some(ix),
                Value::Str(s) if s == "fill" => parts.fill = Some(ix),
                // Two at most: a third handle has no meaning the client
                // could resolve, and silently placing it on the second is
                // less confusing than placing it anywhere.
                Value::Str(s) if s == "thumb" && parts.thumbs.len() < 2 => parts.thumbs.push(ix),
                _ => {}
            }
        }
        parts
    }

    /// Where the handles travel, and how thick the line is across it.
    ///
    /// The centres run the track's extent **less one thumb**, so a handle
    /// at either end is inside the line rather than half outside it. The
    /// value map and the placement share this one span; the pair they
    /// replaced did not, which is why a thumb parked on `max` used to read
    /// back as something less.
    fn track_geom(&self, t: &Track, parts: &TrackParts) -> Option<TrackGeom> {
        let r = self.layout.rect(t.node)?;
        let thumb = parts.thumbs.first().and_then(|ix| self.layout.rect(*ix));
        let (extent, start, cross_mid) = match t.axis {
            TrackAxis::X => (r.w, r.x, r.y + r.h / 2.0),
            TrackAxis::Y => (r.h, r.y, r.x + r.w / 2.0),
        };
        if extent <= 0.0 {
            return None;
        }
        let thick = thumb.map(|tr| match t.axis {
            TrackAxis::X => tr.w,
            TrackAxis::Y => tr.h,
        });
        let thick = thick.filter(|v| *v > 0.0).unwrap_or(0.0);
        Some(TrackGeom { start, extent, origin: start + thick / 2.0, span: (extent - thick).max(0.0), thick, cross_mid })
    }

    /// The quantised value at a pointer coordinate, holding a thumb that
    /// was grabbed `grab` from its centre.
    fn track_value_at(t: &Track, g: &TrackGeom, p: f32, grab: f32) -> i64 {
        if g.span <= 0.0 || t.max == t.min {
            return t.min;
        }
        let frac = (((p - grab) - g.origin) / g.span).clamp(0.0, 1.0);
        // `min` at the bottom of a vertical track: down the screen is less.
        let frac = match t.axis {
            TrackAxis::X => frac,
            TrackAxis::Y => 1.0 - frac,
        };
        t.snap(t.min + (frac as f64 * (t.max - t.min) as f64).round() as i64)
    }

    /// The centre of a handle at `v` -- [`Self::track_value_at`] run
    /// backwards, so the value read at a handle is the value that put it
    /// there.
    fn track_centre(t: &Track, g: &TrackGeom, v: i64) -> f32 {
        if t.max == t.min {
            return g.origin;
        }
        let frac = (v - t.min) as f64 / (t.max - t.min) as f64;
        let frac = match t.axis {
            TrackAxis::X => frac,
            TrackAxis::Y => 1.0 - frac,
        };
        g.origin + g.span * frac.clamp(0.0, 1.0) as f32
    }

    /// Lay every track's parts on its own value, each frame, after layout.
    ///
    /// Every track and not only the one under the hand: the server sends a
    /// value and no geometry at all, so a track the server just moved wants
    /// placing exactly as much as one a finger is on. The pass is
    /// idempotent and tracks are few.
    fn place_tracks(&mut self) {
        let Some(track_atom) = self.session.atoms().track else { return };
        let Some(root) = self.session.root() else { return };
        let tracks: Vec<NodeIx> = self.session.preorder(root).filter(|ix| self.session.node(*ix).is_some_and(|n| n.prop(track_atom).is_some())).collect();
        for ix in tracks {
            let Some(t) = self.track_of(ix) else { continue };
            let parts = self.track_parts(&t);
            let Some(g) = self.track_geom(&t, &parts) else { continue };
            // The hand's value while it is on this track, the server's
            // otherwise.
            let v = match self.track.as_ref().filter(|d| d.track.node == ix) {
                Some(d) => d.value,
                None => self.track_value_of(&t),
            };
            let lo_c = Self::track_centre(&t, &g, v.lo);
            let hi_c = v.hi.map(|h| Self::track_centre(&t, &g, h));
            if let Some(groove) = parts.groove {
                self.place_part(groove, &t, &g, g.start, g.start + g.extent);
            }
            if let Some(fill) = parts.fill {
                // Up to the handle with one; between them with two.
                let (a, b) = match hi_c {
                    Some(hi) => (lo_c, hi),
                    None => (g.start, lo_c),
                };
                self.place_part(fill, &t, &g, a, b);
            }
            for (i, thumb) in parts.thumbs.iter().enumerate() {
                let c = if i == 0 { lo_c } else { hi_c.unwrap_or(lo_c) };
                self.place_part(*thumb, &t, &g, c - g.thick / 2.0, c + g.thick / 2.0);
            }
        }
    }

    /// One part's box, from `a` to `b` along the axis, keeping the cross
    /// thickness the layout gave it and centred on the line.
    ///
    /// Only this node: a part draws its own box and its descendants are not
    /// carried with it (03 §3.4), which is what keeps this one `set_rect`
    /// rather than a subtree walk.
    fn place_part(&mut self, ix: NodeIx, t: &Track, g: &TrackGeom, a: f32, b: f32) {
        let Some(r) = self.layout.rect(ix) else { return };
        let (a, b) = if a <= b { (a, b) } else { (b, a) };
        let len = (b - a).max(0.0);
        let rect = match t.axis {
            TrackAxis::X => Rect::new(a, g.cross_mid - r.h / 2.0, len, r.h),
            TrackAxis::Y => Rect::new(g.cross_mid - r.w / 2.0, a, r.w, len),
        };
        self.layout.set_rect(ix, rect);
    }

    /// Take a press for a track, if it landed on one. `true` when it did:
    /// the track is the more specific claim, so the press does not also arm
    /// a drag (03 §3.4).
    fn track_press(&mut self, ix: NodeIx, x: f32, y: f32) -> bool {
        let Some(t) = self.track_above(ix) else { return false };
        let parts = self.track_parts(&t);
        let Some(g) = self.track_geom(&t, &parts) else { return false };
        let value = self.track_value_of(&t);
        let p = match t.axis {
            TrackAxis::X => x,
            TrackAxis::Y => y,
        };
        let lo_c = Self::track_centre(&t, &g, value.lo);
        let hi_c = value.hi.map(|h| Self::track_centre(&t, &g, h));
        // A press inside a thumb holds it where it was grabbed; anywhere
        // else takes the nearer handle and moves it there at once.
        let on_thumb = |c: f32| (p - c).abs() <= g.thick / 2.0;
        let (handle, grab) = match hi_c {
            Some(hi_c) => {
                if on_thumb(lo_c) && (!on_thumb(hi_c) || p <= lo_c) {
                    (Handle::Lo, p - lo_c)
                } else if on_thumb(hi_c) {
                    (Handle::Hi, p - hi_c)
                } else {
                    // Which is nearer -- and when both sit on the same
                    // value, which side of them the press is on. A pair
                    // closed at the minimum has to be openable, so a press
                    // at or past them takes the high one.
                    let (d_lo, d_hi) = ((p - lo_c).abs(), (p - hi_c).abs());
                    if d_lo < d_hi || (d_lo == d_hi && p < lo_c) {
                        (Handle::Lo, 0.0)
                    } else {
                        (Handle::Hi, 0.0)
                    }
                }
            }
            None => (Handle::Lo, if on_thumb(lo_c) { p - lo_c } else { 0.0 }),
        };
        let mut drag = TrackDrag { track: t, handle, grab, value, sent: None, holding: true };
        Self::track_put(&mut drag, Self::track_value_at(&t, &g, p, grab));
        self.track = Some(drag);
        // The handle is the focus stop, not the track: a range then has two,
        // and the arrows always mean the one the ring is on.
        let thumb = match handle {
            Handle::Lo => parts.thumbs.first().copied(),
            Handle::Hi => parts.thumbs.get(1).copied(),
        };
        let _ = self.set_focus(thumb.or(Some(t.node)), false);
        self.publish_track();
        self.redraw = true;
        true
    }

    /// The hand's move, if it is on a track. `Some` swallows the move: a
    /// track emits no `pointer_move` at all, and does not hover.
    fn track_move(&mut self, x: f32, y: f32) -> Option<Vec<Frame>> {
        let d = self.track.as_ref().filter(|d| d.holding)?;
        let (t, grab) = (d.track, d.grab);
        let parts = self.track_parts(&t);
        let g = self.track_geom(&t, &parts)?;
        let p = match t.axis {
            TrackAxis::X => x,
            TrackAxis::Y => y,
        };
        let at = Self::track_value_at(&t, &g, p, grab);
        let d = self.track.as_mut()?;
        Self::track_put(d, at);
        self.publish_track();
        self.redraw = true;
        Some(self.flush_track_change(false))
    }

    /// The lift. Whatever is still owed goes now, brake or no brake: a
    /// released track sitting at a value the server has not heard is a
    /// thumb drawn where the server does not think it is.
    fn track_release(&mut self) -> Vec<Frame> {
        let Some(d) = self.track.as_mut() else { return Vec::new() };
        if !d.holding {
            return Vec::new();
        }
        d.holding = false;
        self.flush_track_change(true)
    }

    /// The gesture did not happen: put the value back to the tree's, and
    /// say so only if that is not what was last sent.
    fn track_cancel(&mut self) -> Vec<Frame> {
        let Some(d) = self.track.as_ref() else { return Vec::new() };
        let t = d.track;
        if self.session.node(t.node).is_none() {
            self.track = None;
            return Vec::new();
        }
        let back = self.track_value_of(&t);
        if let Some(d) = self.track.as_mut() {
            d.value = back;
            d.holding = false;
        }
        self.publish_track();
        self.redraw = true;
        let out = self.flush_track_change(true);
        self.track = None;
        out
    }

    /// Move the held handle to `at`, keeping the pair in order.
    ///
    /// They may meet and they never swap: a handle that swapped would change
    /// identity under a finger that never left it, and would report a change
    /// in which both numbers moved for one gesture.
    fn track_put(d: &mut TrackDrag, at: i64) {
        match (d.handle, d.value.hi) {
            (Handle::Lo, Some(hi)) => d.value.lo = at.min(hi),
            (Handle::Hi, Some(_)) => d.value.hi = Some(at.max(d.value.lo)),
            (_, None) => d.value.lo = at,
        }
    }

    /// Write the live value into the node's `track_value`, so the
    /// accessibility tree and the thumb under the hand are one number
    /// rather than two that agree once a round trip (03 §3.4).
    fn publish_track(&mut self) {
        let Some(atom) = self.session.atoms().track_value else { return };
        let Some(d) = self.track.as_ref() else { return };
        let (node, v) = (d.track.node, d.value);
        let value = match v.hi {
            Some(hi) => Value::List(vec![Value::Int(v.lo), Value::Int(hi)]),
            None => Value::Int(v.lo),
        };
        self.session.set_prop_local(node, atom, value);
    }

    /// 06 §2, for a track: say the value when it has moved, and never say
    /// one equal to the last sent.
    ///
    /// The quantiser does for a hand what the 300 ms idle does for a field,
    /// so there is no timer here. The brake stays, though: the step bounds
    /// events per unit *distance*, not per unit *time*, and a flick across
    /// forty steps in a third of a second would otherwise queue forty whole
    /// page renders. While one is unanswered the latest value waits -- the
    /// latest, never a backlog, because the comparison is against `sent`.
    fn flush_track_change(&mut self, force: bool) -> Vec<Frame> {
        let Some(d) = self.track.as_ref() else { return Vec::new() };
        if d.sent == Some(d.value) {
            return Vec::new();
        }
        let node = d.track.node;
        // 06 §2: nothing is owed for a node that has left the tree.
        if self.session.node(node).is_none() {
            self.track = None;
            return Vec::new();
        }
        if !force && self.pointer.move_in_flight.is_some_and(|at| self.now.saturating_duration_since(at) < DRAG_ANSWER_WAIT) {
            self.redraw = true;
            return Vec::new();
        }
        let value = d.value;
        let payload = match value.hi {
            Some(hi) => Value::List(vec![Value::Int(value.lo), Value::Int(hi)]),
            None => Value::Int(value.lo),
        };
        if let Some(d) = self.track.as_mut() {
            d.sent = Some(value);
        }
        self.pointer.move_in_flight = Some(self.now);
        self.emit(node, EventKind::Change, payload)
    }

    /// The arrows, the page keys and the ends, on a focused thumb. `Some`
    /// consumes the key: nothing is reported but the `change` it makes.
    fn track_key(&mut self, key: &str, _modifiers: u32) -> Option<Vec<Frame>> {
        let focused = self.focused?;
        let part = self.session.atoms().track_part?;
        if !matches!(self.session.node(focused)?.prop(part)?, Value::Str(s) if s == "thumb") {
            return None;
        }
        let t = self.track_above(focused)?;
        let parts = self.track_parts(&t);
        let handle = if parts.thumbs.first() == Some(&focused) { Handle::Lo } else { Handle::Hi };
        // Seed from the tree unless this very handle is already in hand.
        let fresh = !matches!(self.track.as_ref(), Some(d) if d.track.node == t.node && d.handle == handle);
        if fresh {
            self.track = Some(TrackDrag { track: t, handle, grab: 0.0, value: self.track_value_of(&t), sent: None, holding: false });
        }
        let d = self.track.as_ref()?;
        let now = match (handle, d.value.hi) {
            (Handle::Hi, Some(hi)) => hi,
            _ => d.value.lo,
        };
        let step = t.step.max(1);
        let at = match key {
            "ArrowRight" | "ArrowUp" => now.saturating_add(step),
            "ArrowLeft" | "ArrowDown" => now.saturating_sub(step),
            "PageUp" => now.saturating_add(step.saturating_mul(10)),
            "PageDown" => now.saturating_sub(step.saturating_mul(10)),
            "Home" => t.min,
            "End" => t.max,
            _ => {
                if fresh {
                    self.track = None;
                }
                return None;
            }
        };
        let at = t.snap(at);
        let d = self.track.as_mut()?;
        Self::track_put(d, at);
        self.publish_track();
        self.redraw = true;
        // Not braked: a key press is one change, and autorepeat is bounded
        // by the keyboard. The brake is for a hand that crosses forty steps
        // in a third of a second.
        Some(self.flush_track_change(true))
    }

    fn pointer_down(&mut self, button: u8) -> Vec<Frame> {
        self.ensure_layout();
        let (x, y) = (self.pointer.x, self.pointer.y);
        let Some(ix) = self.hit_now(x, y) else {
            return Vec::new();
        };
        // 03 §1: whatever else this press turns out to be, it is a press
        // somewhere — and somewhere is outside every open panel but the one
        // it landed in. A scrollbar is outside a menu too, so this comes
        // before the strip below rather than after it.
        let dismissed = self.dismiss_overlays(ix);
        // Spec 03 §2: the scrollbar strip belongs to the client. A press on
        // the thumb takes hold of it; a press on the track pages.
        if button == 0 {
            if let Some(scroller) = self.scroller_strip_at(ix, x) {
                let rect = self.layout.rect(scroller).unwrap_or_default();
                if let Some(thumb) = scrollbar_thumb(&self.session, &self.layout, scroller, rect) {
                    let mut out = dismissed;
                    if y >= thumb.y && y <= thumb.y + thumb.h {
                        self.pointer.dragging_thumb = Some((scroller, y - thumb.y));
                    } else {
                        let page = if y < thumb.y { -rect.h } else { rect.h };
                        out.extend(self.scroll_by(scroller, 0.0, page));
                    }
                    return out;
                }
            }
        }
        self.pointer.pressed_on = Some(ix);
        // 03 §3.4: a track is the more specific claim on a press, so a
        // slider on a draggable card is moved by its handle and the card by
        // its margin.
        let on_track = self.track_press(ix, x, y);
        // 06 §6.1: a press whose path reaches a `drag` node *arms* the
        // gesture, and nothing more. Nothing is emitted and the server is told
        // nothing — an arming press that turns out to be a click must be
        // indistinguishable from one that never armed. A handle skips the
        // slop and grabs here.
        self.pointer.drag = (!on_track).then(|| self.drag_source(ix).map(|(_, key)| Drag { source: key, from: (self.pointer.x, self.pointer.y), grabbed: false, sent: None })).flatten();
        trace(|| format!("press on node {:?}, moves go to {:?}", self.session.node(ix).map(|n| n.id), self.target(ix, EventKind::PointerMove).and_then(|(t, _)| self.session.node(t)).map(|n| n.id)));
        // A new drag starts owing nothing, whatever the last one left.
        self.pointer.move_in_flight = None;
        // Focus moves to the nearest editable node on the path, else to the
        // nearest one that handles keys — 03 §3: a grid or a canvas is
        // typed into after a click, not after finding it with `Tab`.
        // Nowhere, if the path has neither; a pointer never shows the ring.
        let editable = self.ancestor_where(ix, |k| matches!(k, NodeKind::Input | NodeKind::TextArea));
        let takes_keys = editable.or_else(|| self.ancestor_keyed(ix));
        let mut out = dismissed;
        out.extend(self.set_focus(takes_keys, false));
        // A press on a track is one deliberate event, so it goes now rather
        // than waiting on the brake -- which is there for the stream of
        // moves that follows, not for this.
        if on_track {
            out.extend(self.flush_track_change(true));
        }
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
        // 06 §2: a gesture that became a drag reports no `pointer_up` and no
        // `click` — the lift *is* the drop. Without this, putting a card down
        // also activates it.
        if self.pointer.drag.is_some_and(|d| d.grabbed) {
            self.pointer.pressed_on = None;
            return self.finish_drag(false);
        }
        self.pointer.drag = None;
        let mut out = self.track_release();
        out.extend(self.flush_coalesced_move());
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
                        // Before the two offers, and in place of them while
                        // the sheet is up: nothing on it carries `pick` or
                        // `nfc`, and there is no session for either to
                        // belong to yet.
                        if !self.consent_click(ix) {
                            self.offer_files(ix);
                            self.offer_scan(ix);
                        }
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
        self.touch = Touch { id: Some(id), from: (x, y), last: (x, y), at: Some(self.now), speed: (0.0, 0.0), phase: TouchPhase::Undecided, hold: None, edge: false };
        // The pointer arrives before it presses, so the press lands on the
        // node under the finger and not on wherever the last one was.
        let mut out = self.pointer_move(x, y);
        out.extend(self.pointer_down(0));
        // Taken as a drag, and by whom: the thumb is the client's own, the
        // rest is whatever asked for moves.
        // 06 §5 step 2, as amended: a grip takes the contact at once. It is
        // what lets a finger carry a row out of a list it could otherwise only
        // scroll — a draggable row carries a *prop*, not a `pointer_move`
        // handler, precisely so that the stroke stays the list's.
        // 03 §3.4: a track takes the stroke because it declared `track`, where
        // it used to take it because it declared `pointer_move`. The prop
        // replaces the handler in the one place the handler was being used as
        // a prop -- otherwise a slider on a phone scrolls the page.
        let taken = self.pointer.dragging_thumb.is_some()
            || self.track.as_ref().is_some_and(|d| d.holding)
            || self.pointer.pressed_on.is_some_and(|ix| self.target(ix, EventKind::PointerMove).is_some() || self.on_drag_handle(ix));
        // 06 §5: a contact that lands on the leading edge of a page that can
        // be gone back from is the navigator's, whatever it landed on.
        //
        // This is the one place a gesture outranks the tree, and it costs
        // what it costs: a slider or a split bar within the strip loses its
        // stroke. That is why the strip is twenty pixels and not a thumb's
        // width — wide enough to be found without looking, narrow enough that
        // what it takes is an edge nobody puts a control against. An
        // application that wants the whole edge says so by not taking back.
        self.touch.edge = x <= TOUCH_EDGE && self.takes_back();
        if taken && !self.touch.edge {
            self.touch.phase = TouchPhase::Dragging;
        } else if self.touch.edge {
            // No hold on the edge: a long press there would be a menu on
            // whatever the strip happens to cover, and the strip is not
            // aimed at anything.
        } else {
            // 06 §5.1: undecided is also held. The clock runs from where the
            // contact landed, and a frame is owed when it elapses.
            self.touch.hold = Some(self.now + Duration::from_millis(TOUCH_HOLD_MS));
            self.next_due = Some(self.now + Duration::from_millis(TOUCH_HOLD_MS));
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
            TouchPhase::Popping => {
                self.hold_pop(x);
                Vec::new()
            }
            // The edge, decided before the slop below and by the same
            // measure: inwards is a back, along is a scroll, and a lift
            // inside the slop is still the tap it was aimed at.
            TouchPhase::Undecided if self.touch.edge && self.touch.wandered(x, y) > TOUCH_SLOP => {
                let (fx, fy) = (x - self.touch.from.0, y - self.touch.from.1);
                if fx > 0.0 && fx.abs() > fy.abs() {
                    trace(|| format!("touch {id} became a back at {x},{y}"));
                    self.touch.phase = TouchPhase::Popping;
                    self.touch.hold = None;
                    // Given back exactly as a scroll gives it back: whatever
                    // it landed on hears `pointer_up` and no `click`.
                    let out = self.cancel_press();
                    self.hold_pop(x);
                    return out;
                }
                self.touch.edge = false;
                self.touch.phase = TouchPhase::Scrolling;
                self.touch.hold = None;
                let mut out = self.cancel_press();
                out.extend(self.wheel(-fx, -fy));
                out
            }
            TouchPhase::Undecided if self.touch.wandered(x, y) > TOUCH_SLOP => {
                trace(|| format!("touch {id} became a scroll at {x},{y}"));
                self.touch.phase = TouchPhase::Scrolling;
                // 06 §5.1: a contact that wandered is not a held one.
                self.touch.hold = None;
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
            // Past halfway, or still moving inwards when it left the glass:
            // the page goes, and the server is told. Otherwise it springs
            // back and **nothing is reported at all** — an abandoned gesture
            // is not an event, for the same reason a dismissed dialog is not.
            TouchPhase::Popping => {
                let k = self.pop_fraction(x);
                if k > 0.5 || speed.0 > POP_FLING {
                    self.release_pop(true);
                    self.go_back()
                } else {
                    self.release_pop(false);
                    Vec::new()
                }
            }
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
        // 06 §5 step 7 and §6.1 step 5: a gesture the platform took away ends
        // the drag it had become, with the sentinel slot.
        let mut out = self.finish_drag(true);
        out.extend(self.track_cancel());
        out.extend(self.cancel_press());
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
        let moved = new != self.focused;
        if moved {
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
        // Whatever took focus is brought into view, however it took it: the
        // `Tab` that walked past the fold and the tap that raised a phone's
        // keyboard are the same problem, and a node already on the screen
        // scrolls nothing.
        if moved {
            if let Some(n) = new {
                out.extend(self.reveal(n));
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
                || node.handler(EventKind::FileSave).is_some()
                // 03 §3: a thing that can be moved can be reached without a
                // pointer. Without this a draggable row with no `click` is
                // unreachable by keyboard — and by 03 §6's ceiling rule, that
                // would forbid the assistive action too.
                || self.session.atoms().drag.is_some_and(|a| node.prop(a).is_some_and(|v| !matches!(v, Value::Bool(false))))
                // 03 §3.4: a track's handle is the stop, not the track. A
                // range then has two, and the arrows always mean the one the
                // ring is on -- no modifier, and no "the handle moved last".
                || self.session.atoms().track_part.is_some_and(|a| node.prop(a).is_some_and(|v| matches!(v, Value::Str(s) if s == "thumb")));
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
        // The consent sheet is answerable from the keyboard, like anything
        // else with a click handler: it is the one page a person may reach
        // before they have decided to trust the application at all.
        self.consent_click(f);
        self.offer_files(f);
        self.offer_scan(f);
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

    /// Spec 03 §1: a press outside an open overlay dismisses it.
    ///
    /// The client owns the hand, so the client owns this. Nothing new is
    /// invented on the wire: an overlay that carries a `blur` handler hears
    /// `blur`, which already means "this stopped being the thing being
    /// used", and one that carries none hears nothing at all. So an
    /// application opts in per panel and says for itself what closing means
    /// — no application can get it wrong by forgetting, and none is closed
    /// behind its back.
    ///
    /// **Outside is outside the overlay's parent**, not outside the overlay.
    /// A panel and the control that raised it are siblings under one box —
    /// that is what `stack` plus an absolute overlay is — and a press on the
    /// control is a press on the widget, not outside it. Were it the overlay
    /// alone, a select would shut on the press and its own click would open
    /// it again, and no select could ever be closed by clicking it.
    fn dismiss_overlays(&mut self, pressed: NodeIx) -> Vec<Frame> {
        let Some(root) = self.session.root() else {
            return Vec::new();
        };
        let mut shut = Vec::new();
        for ix in self.session.preorder(root) {
            let Some(node) = self.session.node(ix) else {
                continue;
            };
            if node.kind != NodeKind::Overlay || node.handler(EventKind::Blur).is_none() {
                continue;
            }
            // An overlay the layout never placed is not on the screen, so
            // there is nothing a press can be outside of.
            if self.layout.rect(ix).is_none() || self.layout.is_virtual(ix) {
                continue;
            }
            let widget = if node.parent == ix { ix } else { node.parent };
            if !self.under(widget, pressed) {
                shut.push(ix);
            }
        }
        let mut out = Vec::new();
        for ix in shut {
            out.extend(self.emit(ix, EventKind::Blur, Value::Null));
        }
        out
    }

    /// Whether `ix` is `top` or stands somewhere under it.
    fn under(&self, top: NodeIx, ix: NodeIx) -> bool {
        let mut cur = Some(ix);
        while let Some(at) = cur {
            if at == top {
                return true;
            }
            let Some(node) = self.session.node(at) else {
                return false;
            };
            cur = if node.parent.is_some() && node.parent != at { Some(node.parent) } else { None };
        }
        false
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

    /// What a key means on the way to `f`, per 03 §3.1. One walk, because the
    /// two questions it answers are one question asked at one node: does
    /// anybody hear this key, and does the client's own meaning still stand.
    ///
    /// They used to be two predicates over two different nodes — `wants_key`
    /// walked the path, `claims_key` looked only at `f` — and a node could
    /// therefore be sent a key whose meaning it had not taken, or take a
    /// meaning at a node that was never sent it.
    fn key_claim(&self, f: NodeIx, key: &str) -> Claim {
        let Some(target) = self.ancestor_keyed(f) else {
            return Claim::Ignored;
        };
        let named = self.session.atom_id("keys").and_then(|atom| self.session.node(target).and_then(|n| n.prop(atom)).cloned());
        match named {
            // It said what it wants, so it hears that and nothing else — and
            // what it named is its to mean, however far up the path it is.
            // That is what lets a combobox claim `Enter` on behalf of the
            // field inside it.
            Some(Value::List(want)) => {
                if want.iter().any(|k| matches!(k, Value::Str(s) if s == key)) {
                    Claim::Claimed
                } else {
                    Claim::Ignored
                }
            }
            // It named nothing, so it hears everything — but the old
            // all-or-nothing meaning is kept narrow, to the node itself. A
            // dialog carrying a bare `key_down` used to swallow `Enter` from
            // every button inside it.
            _ => {
                if target == f {
                    Claim::Claimed
                } else {
                    Claim::Reported
                }
            }
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

    /// A soft keyboard came up, went away, or changed size ([`Input::Covered`]).
    ///
    /// The field that has focus is the one the keyboard was raised for, so
    /// it is the one it lands on: revealed again here, because the covering
    /// arrives *after* the focus that caused it and the reveal done then
    /// was measured against a window with nothing over it.
    fn set_covered(&mut self, px: f32) -> Vec<Frame> {
        let px = if px.is_finite() { px.clamp(0.0, self.window_h) } else { 0.0 };
        if (self.covered - px).abs() < 0.5 {
            return Vec::new();
        }
        trace(|| format!("{px:.0} px of the window's bottom are covered"));
        self.covered = px;
        self.size = Size::new(self.size.w, (self.window_h - px).max(0.0));
        self.invalidate();
        match self.focused {
            Some(f) => self.reveal(f),
            None => Vec::new(),
        }
    }

    /// Bring `ix` where it can be seen: every `scroll` or `list` between it
    /// and the root moves just far enough, and no further, to put the node's
    /// rectangle inside what that scroller actually shows.
    ///
    /// *Actually shows* is the scroller's own rectangle with whatever a soft
    /// keyboard is over taken off the bottom ([`Input::Covered`]). A phone
    /// raises the keyboard **because** a field took focus, which means the
    /// field it was raised for is the one it lands on top of; without this,
    /// the typing lands somewhere the person cannot see.
    ///
    /// Innermost scroller first, and the node's rectangle is read again
    /// after each one: scrolling a pane moves what is inside the pane, and
    /// the next scroller out has to be told where the node ended up.
    ///
    /// Moved at once rather than eased. A scroll in flight is a scroll that
    /// has not happened yet, and the keystroke after this one must not have
    /// to race it to decide what is on the screen.
    fn reveal(&mut self, ix: NodeIx) -> Vec<Frame> {
        self.ensure_layout();
        let mut out = Vec::new();
        // `size` is already the window less whatever covers it, so the page
        // ends where the keyboard begins and a scroller's own rectangle is
        // the whole of the question.
        let mut cur = self.session.node(ix).and_then(|n| n.parent.is_some().then_some(n.parent));
        while let Some(s) = cur {
            let Some(node) = self.session.node(s) else { break };
            let up = node.parent.is_some().then_some(node.parent);
            if matches!(node.kind, NodeKind::Scroll | NodeKind::List) {
                if let (Some(r), Some(v)) = (self.layout.rect(ix), self.layout.rect(s)) {
                    let (top, bottom) = (v.y, (v.y + v.h).min(self.size.h));
                    // Never so far that the top of the node leaves to bring
                    // its bottom in: a field taller than what is left of the
                    // window is read from its first line, not its last.
                    let dy = if r.y < top {
                        r.y - top - REVEAL_MARGIN
                    } else if r.y + r.h > bottom {
                        (r.y + r.h - bottom + REVEAL_MARGIN).min((r.y - top).max(0.0))
                    } else {
                        0.0
                    };
                    let (left, right) = (v.x, v.x + v.w);
                    let dx = if r.x < left {
                        r.x - left
                    } else if r.x + r.w > right {
                        (r.x + r.w - right).min((r.x - left).max(0.0))
                    } else {
                        0.0
                    };
                    if dx != 0.0 || dy != 0.0 {
                        out.extend(self.scroll_by(s, dx, dy));
                        self.ensure_layout();
                    }
                }
            }
            cur = up;
        }
        out
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
        // 03 §3: while a drag is live the shape is `grabbing`, over
        // everything — the hand is holding something wherever it happens to
        // be, and what it is over says nothing about that.
        if self.pointer.drag.is_some_and(|d| d.grabbed) {
            return Cursor::Grabbing;
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
            // 03 §3: `grab` over a thing that can be picked up, ahead of the
            // hand a `click` would draw — a draggable row is usually both, and
            // what the pointer is about to do to it is pick it up.
            if self.session.atoms().drag.is_some_and(|a| node.prop(a).is_some_and(|v| !matches!(v, Value::Bool(false)))) {
                return Cursor::Grab;
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
        Some(self.edits.entry(id).or_insert_with(|| Edit { seed: seed.clone(), value: seed, caret: len, anchor: len, scroll_x: 0.0, typed_at: None }))
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
        self.note_typing(f);
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
        let secret = self.session.is_secret(e);
        let display = if secret { eui_tree::secret_display(&text) } else { text.clone() };
        let scroll_x = self.edits.get(&self.session.node(e)?.id).map_or(0.0, |ed| ed.scroll_x);
        let inner_w = (rect.w - style.inset_h()).max(0.0);
        let shaped = self.text.shape(&display, style.font, Some(inner_w), style.line_clamp);
        let lx = x - (rect.x + style.border.l + style.padding.l) - style.text_pad_x(inner_w, shaped.metrics.width) + scroll_x;
        let ly = y - (rect.y + style.border.t + style.padding.t);
        let at = shaped.byte_at(lx, ly).min(display.len());
        Some(if secret { eui_tree::secret_unoffset(&text, at) } else { at.min(text.len()) })
    }

    // -------------------------------------------------------------- files

    /// Dialogs the tree asked for since the last call (spec 03 §3.2). The
    /// window opens them; nothing else may.
    pub fn take_file_asks(&mut self) -> Vec<FileAsk> {
        std::mem::take(&mut self.file_asks)
    }

    /// Scans the tree asked for and the window has not started (03 §3.3).
    pub fn take_nfc_asks(&mut self) -> Vec<NfcAsk> {
        std::mem::take(&mut self.nfc_asks)
    }

    /// A tag was read for the scan `token` opened.
    ///
    /// A scan ends when it reads something: the token is spent here, so a
    /// platform that delivers twice is answered once. A token nobody
    /// started is ignored — the window is trusted, but a bug in it must not
    /// put an event on the wire that no node asked for.
    pub fn scanned(&mut self, token: u32, uid: &str, records: &[NfcRecord]) -> Vec<Frame> {
        let Some(id) = self.scans.remove(&token) else { return Vec::new() };
        let Some(ix) = self.session.lookup(id) else { return Vec::new() };
        self.touched = true;
        let listed = records.iter().map(|r| Value::List(vec![Value::Str(r.kind.clone()), Value::Str(r.payload.clone())])).collect();
        self.emit(ix, EventKind::NfcTag, Value::List(vec![Value::Str(uid.to_owned()), Value::List(listed)]))
    }

    /// The scan `token` opened ended without reading anything.
    ///
    /// Nothing is reported, for the reason a dismissed dialog is not
    /// (06 §3): an application learns that someone held their phone up and
    /// thought better of it only if it is told, and it is not told.
    pub fn scan_ended(&mut self, token: u32) {
        if self.scans.remove(&token).is_some() {
            self.touched = true;
        }
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
    /// Spec 03 §3.3: the person activated a node carrying `nfc`. If it also
    /// declares a **server** handler for `nfc_tag` and the `nfc` capability
    /// was granted, the window is asked to start a scan.
    ///
    /// The three conditions are 03 §3.2's, word for word, and for the same
    /// reason: a scan the person did not ask for is the whole of what makes
    /// a reader dangerous. A tree that merely arrives starts nothing.
    fn offer_scan(&mut self, from: NodeIx) {
        let Some((ix, handler)) = self.target(from, EventKind::NfcTag) else { return };
        // A local chunk cannot be handed a tag: what a reader saw is the
        // server's, as both ends of a transfer are.
        if !matches!(handler, Handler::Server(_)) {
            return;
        }
        let Some(atom) = self.session.atom_id("nfc") else { return };
        let Some(node) = self.session.node(ix) else { return };
        let id = node.id;
        let prompt = match node.prop(atom) {
            Some(Value::Str(p)) => p.clone(),
            Some(Value::Null) | None => return,
            Some(_) => String::new(),
        };
        if self.granted & caps::NFC == 0 {
            eprintln!("eui: node {id} carries `nfc`, which needs a capability the person did not grant; nothing scans");
            return;
        }
        // One scan at a time per node, as one dialog is.
        if self.scans.values().any(|n| *n == id) {
            return;
        }
        let token = self.mint();
        self.scans.insert(token, id);
        self.nfc_asks.push(NfcAsk { token, node: id, prompt });
        self.touched = true;
    }

    fn offer_files(&mut self, from: NodeIx) {
        for (kind, prop) in [(EventKind::FilePick, "pick"), (EventKind::FileSave, "save")] {
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
                    // Bit 1 asks for the camera, bit 2 for a recording.
                    // Both together is a contradiction and the camera wins,
                    // because a client that guessed would guess differently
                    // from the next one.
                    //
                    // Either one gets exactly one file: neither platform
                    // photographs or records several things in one sheet,
                    // and a `multiple` the sheet cannot honour is a promise
                    // to the server that the client would then break.
                    let source = if flags & 2 != 0 {
                        PickSource::Camera
                    } else if flags & 4 != 0 {
                        PickSource::Microphone
                    } else {
                        PickSource::Held
                    };
                    FileWant::Open { accept, multiple: flags & 1 != 0 && source == PickSource::Held, max, source }
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
            // The capability is whatever *this* sheet needs, which for a
            // `pick` is not known until its flags have been read: taking a
            // photograph and reading a folder are not the same grant, and
            // reading the prop is what says which one was asked for.
            let cap = match &want {
                FileWant::Open { source, .. } => source.capability(),
                FileWant::Save { .. } => caps::FS_SAVE,
            };
            if self.granted & cap == 0 {
                eprintln!("eui: node {id} carries `{prop}`, which needs a capability the person did not grant; nothing opens");
                continue;
            }
            let token = self.mint();
            let ask = FileAsk { token, node: id, want };
            self.asks.insert(token, ask.clone());
            self.file_asks.push(ask);
            self.touched = true;
        }
    }

    /// The node under `at` that would take a dropped file: it carries the
    /// `drop` prop and a **server** handler for `file_pick`. Both, or
    /// nothing — the same rule the dialog follows, for the same reason
    /// (spec 03 §3.2): a local chunk cannot be given a file, because both
    /// ends of a transfer are the server's.
    fn file_drop_target(&mut self, at: (f32, f32)) -> Option<(NodeIx, u32, u64)> {
        let hit = self.hit_now(at.0, at.1)?;
        let (ix, handler) = self.target(hit, EventKind::FilePick)?;
        if !matches!(handler, Handler::Server(_)) {
            return None;
        }
        let atom = self.session.atom_id("drop")?;
        let node = self.session.node(ix)?;
        let id = node.id;
        let value = node.prop(atom).cloned()?;
        // `drop` is the shape of `pick` (spec 03 §3.2) and only its ceiling
        // matters here: a dialog's `accept` filters what can be chosen, and
        // nothing filters what a hand lets go of. The application refuses
        // what it does not want, and says why.
        let max = match &value {
            Value::List(l) => match l.get(2) {
                Some(Value::Int(m)) if *m > 0 => (*m as u64).min(MAX_UPLOAD_BYTES),
                _ => DEFAULT_UPLOAD_BYTES,
            },
            _ => DEFAULT_UPLOAD_BYTES,
        };
        Some((ix, id, max))
    }

    /// A file is being dragged over the window, or has left it. `at` is
    /// where the pointer is, `None` when the drag ended or went away.
    ///
    /// Reports `file_drag` on the node that would take it — once on
    /// entering, once on leaving — so the box can show it would. Nothing
    /// is read and nothing is transferred: this is the announcement, and
    /// [`Self::file_dropped`] is the act.
    pub fn file_dragged(&mut self, at: Option<(f32, f32)>) -> Vec<Frame> {
        let found = at.and_then(|p| self.file_drop_target(p));
        let now = found.as_ref().map(|(_, id, _)| *id);
        if now == self.drop_over {
            return Vec::new();
        }
        let mut out = Vec::new();
        if let Some(old) = self.drop_over.take() {
            if let Some(ix) = self.session.lookup(old) {
                out.extend(self.emit(ix, EventKind::FileDrag, Value::List(vec![Value::Bool(false)])));
            }
        }
        if let Some((ix, id, _)) = found {
            // Without the grant there is no drop to come, so there is no
            // point lighting a box that will take nothing.
            if self.granted & caps::FS_PICK != 0 {
                self.drop_over = Some(id);
                out.extend(self.emit(ix, EventKind::FileDrag, Value::List(vec![Value::Bool(true)])));
            }
        }
        self.touched = true;
        out
    }

    /// Files were let go over the window at `at`: one `file_pick` event
    /// each and an upload id each, exactly as the dialog gives
    /// ([`Self::picked`]) — a drop and a pick differ in the gesture and in
    /// nothing after it.
    ///
    /// Needs `fs.pick`. A window that drops without it gets nothing and the
    /// application is told nothing, which is the same silence a capability
    /// that was not granted gives everywhere else (spec 08 §3).
    pub fn file_dropped(&mut self, at: (f32, f32), files: Vec<(String, u64)>) -> (Vec<u32>, Vec<Frame>) {
        let mut out = self.file_dragged(None);
        if self.granted & caps::FS_PICK == 0 {
            eprintln!("eui: a file was dropped, which needs `fs.pick`; the person did not grant it, so nothing arrives");
            return (Vec::new(), out);
        }
        let Some((ix, node, max)) = self.file_drop_target(at) else { return (Vec::new(), out) };
        self.touched = true;
        let mut ids = Vec::new();
        for (name, size) in files {
            let id = self.mint();
            let name = basename(&name).to_owned();
            let payload = Value::List(vec![Value::Int(i64::from(id)), Value::Str(name), Value::Int(i64::try_from(size).unwrap_or(i64::MAX))]);
            out.extend(self.emit(ix, EventKind::FilePick, payload));
            if size > max {
                out.push(Frame::Upload(Transfer { id, seq: 0, flag: Chunked::Abort, bytes: format!("file is {size} bytes; this one accepts {max}").into_bytes() }));
            } else {
                self.uploads.insert(id, Upload { node, seq: 0, sent: 0, max });
            }
            ids.push(id);
        }
        (ids, out)
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
        let Some(f) = self.focused.filter(|f| self.is_editable(*f)) else {
            self.ime_spot = None;
            self.caret_due = None;
            return None;
        };
        let out = self.editing_of(f);
        if out.is_none() {
            self.ime_spot = None;
            self.caret_due = None;
        }
        out
    }

    /// Spec 03 §3: the caret blinks. Half a period on, half off — and the
    /// clock starts again every time the caret moves, so it is up for the
    /// whole of that first half and typing is never interrupted by a bar
    /// that happens not to be there.
    ///
    /// The `until` this puts on the frame is what makes the window come
    /// back: a focused field costs two paints a second, which is what a
    /// blinking caret costs anywhere.
    fn caret_blink(&mut self, at: (NodeIx, usize, usize, usize)) -> bool {
        if self.caret_was != Some(at) {
            self.caret_was = Some(at);
            self.caret_since = self.now;
        }
        let elapsed = self.now.saturating_duration_since(self.caret_since).as_secs_f32();
        if elapsed >= CARET_BLINK_FOR {
            self.caret_due = None;
            return true;
        }
        let into = elapsed.rem_euclid(CARET_BLINK);
        self.caret_due = Some(self.now + Duration::from_secs_f32((CARET_BLINK - into).max(0.001)));
        elapsed.rem_euclid(CARET_BLINK * 2.0) < CARET_BLINK
    }

    fn editing_of(&mut self, f: NodeIx) -> Option<Editing> {
        let rect = self.layout.rect(f)?;
        let style = eui_layout::Style::resolve(&self.session.style_of(f), &self.resolved);
        let text = self.session.text_of(f).unwrap_or("").to_owned();
        let secret = self.session.is_secret(f);
        let display = if secret { eui_tree::secret_display(&text) } else { text.clone() };
        let shaped = self.text.shape(&display, style.font, Some((rect.w - style.inset_h()).max(0.0)), style.line_clamp);
        let pre = self.preedit.len();
        let id = self.session.node(f)?.id;
        let edit = self.edits.get_mut(&id)?;
        let shown = |o: usize| {
            let at = if o > edit.caret { o.saturating_add(pre) } else { o };
            if secret {
                eui_tree::secret_offset(&text, at)
            } else {
                at
            }
        };
        let caret = {
            let at = edit.caret.saturating_add(pre);
            if secret {
                eui_tree::secret_offset(&text, at)
            } else {
                at
            }
        };
        let inner_w = (rect.w - style.inset_h()).max(0.0);
        let inner_h = (rect.h - style.inset_v()).max(0.0);
        let cx = shaped.caret(caret).0;
        if cx - edit.scroll_x > inner_w {
            edit.scroll_x = cx - inner_w;
        } else if cx < edit.scroll_x {
            edit.scroll_x = cx;
        }
        if shaped.metrics.width <= inner_w {
            edit.scroll_x = 0.0;
        }
        let pad = style.text_pad_x(inner_w, shaped.metrics.width);
        let (cx, cy) = shaped.caret(caret);
        let origin_x = rect.x + style.border.l + style.padding.l - edit.scroll_x + pad;
        let origin_y = rect.y + style.border.t + style.padding.t + if inner_h > shaped.metrics.height { (inner_h - shaped.metrics.height) * 0.5 } else { 0.0 };
        let above = style.font.size * 0.9;
        let below = style.font.size * 0.25;
        self.ime_spot = Some(eui_layout::Rect::new(origin_x + cx, origin_y + cy - above, 1.0, above + below));
        let r = edit.selection();
        let (start, end) = (shown(r.start), shown(r.end));
        let caret_on = self.caret_blink((f, start, end, caret));
        Some(Editing { node: f, start, end, caret, scroll_x: self.edits.get(&id).map_or(0.0, |e| e.scroll_x), caret_on })
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
        self.note_typing(f);
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

    /// Where an input method should put its candidate window: the box of the
    /// focused node that takes typing, if any.
    ///
    /// That is an `input` or a `textarea`, and also any node carrying
    /// `typing` (03 §3.1) — the node that owns its own caret. The second is
    /// what raises the soft keyboard on a phone: there, the keyboard *is*
    /// this call (`set_ime_allowed` is `becomeFirstResponder` on iOS), so a
    /// code editor or a pattern grid built out of a box and `key_down` could
    /// be looked at and never typed into. It reads as the view being
    /// read-only, because nothing anywhere reports an error: the keys the
    /// application is waiting for are simply never pressed.
    ///
    /// Nothing else changes for such a node. iOS sends typing as ordinary
    /// key events — one `Key::Character` per character, `Backspace` named —
    /// so what arrives once the keyboard is up is the `key_down` the view
    /// already handles, and the client still owns no caret here.
    pub fn ime_area(&self) -> Option<eui_layout::Rect> {
        let f = self.focused.filter(|f| self.is_editable(*f) || self.takes_typing(*f))?;
        if self.is_editable(f) {
            return self.ime_spot.or_else(|| self.layout.rect(f));
        }
        self.layout.rect(f)
    }

    /// Does this node declare `typing`?
    ///
    /// Opt-in, and deliberately not inferred from holding a `key_down`: a
    /// page that handles a shortcut at its root would then raise the phone's
    /// keyboard on any focus at all and have no way to decline it.
    fn takes_typing(&self, ix: NodeIx) -> bool {
        self.session.atom_id("typing").and_then(|atom| self.session.node(ix).and_then(|n| n.prop(atom))) == Some(&Value::Bool(true))
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
        // 03 §3.4: a focused track handle takes the arrows, the page keys and
        // the ends, and they are consumed -- only the `change` is reported.
        // Before the scrolling keys, or a slider near the bottom of a page
        // would scroll it instead of moving.
        if down {
            if let Some(out) = self.track_key(key, modifiers) {
                return out;
            }
        }
        let claimed = self.focused.is_some_and(|f| self.is_editable(f) || self.ancestor_keyed(f).is_some());
        if down && modifiers & 0b1110 == 0 && matches!(key, "ArrowUp" | "ArrowDown" | "PageUp" | "PageDown" | "Home" | "End") && !claimed {
            if let Some(out) = self.scroll_key(key) {
                return out;
            }
        }
        // 06 §6.1 step 5: `Escape` puts down what is in the hand. It is
        // tested before focus, because a pointer drag leaves focus wherever it
        // was — usually nowhere — and the guard below would swallow the key.
        if down && key == "Escape" && self.pointer.drag.is_some_and(|d| d.grabbed) {
            return self.finish_drag(true);
        }
        // The same for a hand on a track: the gesture did not happen.
        if down && key == "Escape" && self.track.as_ref().is_some_and(|d| d.holding) {
            self.pointer.pressed_on = None;
            return self.track_cancel();
        }
        // 03 §3: a thing that can be moved is picked up with `Space` and put
        // down with it, and the arrows move it in between. Nothing is claimed
        // until it is grabbed, and `Space` is claimed only where there is no
        // `click` to stand for. It reports what a pointer drag reports and
        // nothing else, so a server needs no second path for the keyboard.
        if down {
            if let Some(out) = self.drag_key(key, modifiers) {
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
            if self.key_claim(f, key) == Claim::Ignored {
                return self.set_focus(None, false);
            }
            return self.emit(f, EventKind::KeyDown, Value::List(vec![Value::Str(key.to_owned()), Value::Int(i64::from(modifiers))]));
        }
        let claim = self.key_claim(f, key);
        let mut out = Vec::new();
        // 03 §3.1: whether the client's own editing had a use for this key.
        // It is the whole of the rule inside a field — a key the client used
        // is one it *keeps*, and a key it could not use is one it yields, so
        // `Backspace` reaches the application exactly when there was nothing
        // to delete. Reporting it either way would leave the application
        // unable to tell the two apart, which is the same as not reporting it.
        let mut used = false;
        if down {
            let editable = self.is_editable(f);
            match key {
                // The first tier: editing the text is never withheld. A field
                // you cannot type into is not a field, and no prop may make
                // one — so this arm runs before the claim is consulted, and
                // `edit_key` declines when it has nothing to do.
                _ if editable && self.edit_key(f, key, modifiers) => used = true,
                "Enter" if self.session.node(f).map(|n| n.kind) == Some(NodeKind::Input) => {
                    // The value goes either way. A server that claimed `Enter`
                    // — to take the highlighted suggestion rather than the text
                    // that was typed — still needs to know what was typed, and
                    // needs it *before* the key that acts on it. What a claim
                    // withholds is the `submit`, exactly as a claim on a button
                    // withholds the `click` it stands for.
                    out.extend(self.commit_edit(f));
                    if claim != Claim::Claimed {
                        out.extend(self.emit(f, EventKind::Submit, Value::Null));
                    }
                }
                // A node that handles keys is not activated by `Enter` or
                // `Space`: it asked for the keys, and in a tracker `Space`
                // is what starts the song, not a click on the pattern.
                "Enter" | " " if !editable && claim != Claim::Claimed => {
                    out.extend(self.activate(f));
                }
                _ => {}
            }
        }
        let kind = if down { EventKind::KeyDown } else { EventKind::KeyUp };
        if claim != Claim::Ignored && !used {
            out.extend(self.emit(f, kind, Value::List(vec![Value::Str(key.to_owned()), Value::Int(i64::from(modifiers))])));
        }
        out
    }

    /// Whether the node that would handle this key actually asked for it.
    ///
    /// Spec 03 §3: the caret, the selection and the clipboard belong to the
    /// client. True when the key was an editing key and has been applied.
    fn edit_key(&mut self, f: NodeIx, key: &str, modifiers: u32) -> bool {
        let (shift, ctrl) = (modifiers & 1 != 0, modifiers & (2 | 8) != 0);
        let multiline = self.session.node(f).map(|n| n.kind) == Some(NodeKind::TextArea);
        let secret = self.session.is_secret(f);
        let Some(edit) = self.edit_mut(f) else {
            return false;
        };
        // 03 §3.1, the third tier: a key the client's own editing would not
        // act on is not the client's to keep. `Backspace` with nothing before
        // the caret is not editing — it is a gesture the field has no answer
        // for, so the field is not the one to answer it, and a tag list above
        // can take it as "remove the last".
        //
        // A fingerprint rather than a predicate per arm, because a
        // fingerprint cannot drift out of step with the arms it guards: every
        // arm that changes the value changes its length.
        let before = (edit.value.len(), edit.caret, edit.anchor);
        let mut copied = None;
        let mut hid = false;
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
                    if secret {
                        hid = true;
                    } else {
                        copied = Some(edit.value[r].to_owned());
                    }
                }
            }
            "x" | "X" if ctrl => {
                let r = edit.selection();
                if !r.is_empty() {
                    if !secret {
                        copied = Some(edit.value[r].to_owned());
                    }
                    edit.delete(false);
                }
            }
            "Enter" if multiline => edit.insert("\n"),
            _ => return false,
        }
        let moved = (edit.value.len(), edit.caret, edit.anchor) != before;
        let took = copied.is_some();
        if took {
            self.clipboard = copied;
        }
        // Nothing moved and nothing was taken: the key was one of ours by
        // name and none of ours in this position. Declining also skips the
        // invalidate, so a dead key no longer costs a frame.
        // A secret field still consumes copy: the key is ours, the clipboard
        // is not.
        if !moved && !took && !hid {
            return false;
        }
        self.show_edit(f);
        self.note_typing(f);
        true
    }

    /// The field owes the server a `change` exactly while it disagrees with
    /// it — 06 §2's idle commit is armed here and nowhere else.
    ///
    /// Only when something is listening: a field nobody asked about must not
    /// wake the process 300 ms after it was typed into, and `emit` would drop
    /// the event anyway (spec 10 §1).
    /// The value under a local edit changed out from under it, so the buffer
    /// has to agree — otherwise the next keystroke writes the old value back
    /// over the new one. Three things can do it: a server's `SetText`, a local
    /// chunk's, and the undo that takes a provisional one back.
    ///
    /// The caret lands at the end and the seed advances, so the change owes no
    /// `change` of its own: this is the server, or a handler acting for it,
    /// saying what the field now holds.
    fn reseed_edit(&mut self, ix: NodeIx, value: String) {
        let id = self.session.node(ix).map(|n| n.id).unwrap_or(0);
        let Some(edit) = self.edits.get_mut(&id) else {
            return;
        };
        let end = value.len();
        *edit = Edit { seed: value.clone(), value, caret: end, anchor: end, scroll_x: 0.0, typed_at: None };
    }

    fn note_typing(&mut self, f: NodeIx) {
        let now = self.now;
        let listening = self.target(f, EventKind::Change).is_some();
        if let Some(edit) = self.edit_mut(f) {
            edit.typed_at = (listening && edit.value != edit.seed).then_some(now);
        }
    }

    /// 06 §2: a field that has gone quiet says what is in it, without waiting
    /// for a blur that may never come — a search box is typed into and looked
    /// at, not tabbed out of.
    ///
    /// Run from `paint`, like `wake_events` and for the same reason: nothing
    /// is arriving, so there is no input to hang it off.
    fn idle_changes(&mut self) -> Vec<Frame> {
        let now = self.now;
        let due: Vec<u32> = self.edits.iter().filter(|(_, e)| e.typed_at.is_some_and(|t| now.saturating_duration_since(t) >= CHANGE_IDLE)).map(|(id, _)| *id).collect();
        if due.is_empty() {
            return Vec::new();
        }
        // A composition in progress is input. `show_edit` puts the preedit in
        // the tree but not in `value`, so committing here would send a value
        // with the composing text missing. The deadline is pushed *forward*
        // rather than left in the past: a deadline already behind us makes
        // every pass due, which is a spin.
        if !self.preedit.is_empty() {
            for id in due {
                if let Some(edit) = self.edits.get_mut(&id) {
                    edit.typed_at = Some(now);
                }
            }
            return Vec::new();
        }
        let mut out = Vec::new();
        for id in due {
            let Some(ix) = self.session.lookup(id) else {
                continue;
            };
            out.extend(self.commit_edit(ix));
        }
        out
    }

    /// `change`, if the field's value differs from what the server has.
    fn commit_edit(&mut self, f: NodeIx) -> Vec<Frame> {
        let id = self.session.node(f).map(|n| n.id).unwrap_or(0);
        let Some(edit) = self.edits.get_mut(&id) else {
            return Vec::new();
        };
        // Either way the field no longer owes one: it has just been told, or
        // it had nothing to tell.
        edit.typed_at = None;
        if edit.value == edit.seed {
            return Vec::new();
        }
        edit.seed = edit.value.clone();
        let value = edit.value.clone();
        self.emit(f, EventKind::Change, Value::Str(value))
    }

    // -------------------------------------------------------- consent

    /// Spec 01 §2.1: ask what this application may do, before it is dialled.
    ///
    /// `asked` is what the manifest wants and the command line did not
    /// already grant; `name` is what the manifest calls itself. Every row
    /// starts chosen, and each one can be turned off on its own: an
    /// application that wants a camera and a file picker must not be able
    /// to make somebody grant the camera to drop a CSV on it.
    ///
    /// The sheet is a tree this client mounts itself, the same way
    /// [`Self::show_stopped`] mounts its notice, and for the same reason: a
    /// window opened straight onto a URL has no chrome to put a question
    /// in, and a phone has no chrome at all.
    pub fn ask_consent(&mut self, asked: u32, name: &str) {
        let asked = asked & caps::ALL;
        if asked == 0 {
            self.consent_said = Some(0);
            return;
        }
        self.consent = Some(Consent { asked, chosen: asked, name: name.to_owned() });
        self.show_consent();
    }

    /// The answer, once there is one. Taken, so the window asks on every
    /// pass and acts once.
    pub fn take_consent(&mut self) -> Option<u32> {
        self.consent_said.take()
    }

    /// True while the sheet is up and the session has not been dialled.
    pub fn asking_consent(&self) -> bool {
        self.consent.is_some()
    }

    /// A click landed while the sheet is up. `true` if it was the sheet's.
    ///
    /// `ix` is whatever was under the pointer, which for a button is the
    /// label inside it — so the handler is resolved the same way a click
    /// resolves one, by walking out to the node that declared it.
    fn consent_click(&mut self, ix: NodeIx) -> bool {
        if self.consent.is_none() {
            return false;
        }
        let Some((ix, _)) = self.target(ix, EventKind::Click) else { return false };
        let Some(c) = self.consent.as_mut() else { return false };
        let Some(id) = self.session.node(ix).map(|n| n.id) else { return false };
        match id {
            CONSENT_ALLOW => {
                let said = c.chosen;
                self.consent = None;
                self.consent_said = Some(said);
                self.done_asking();
            }
            CONSENT_DENY => {
                // Close leaves things as they were, which is what the word
                // says. Answering `0` would have been right while the button
                // read "Don't allow" and the sheet was only ever seen once;
                // it is a silent revocation now that the padlock reopens it
                // on an application that has already been granted something.
                // On a first question nothing is granted yet, so this is the
                // same `0` it always was.
                let keep = self.granted & c.asked;
                self.consent = None;
                self.consent_said = Some(keep);
                self.done_asking();
            }
            id if id >= CONSENT_ROW => {
                // The rows are in bit order, so the row's distance from the
                // first names the bit it stands for.
                let Some(nth) = id.checked_sub(CONSENT_ROW) else { return false };
                let Some(&(_, bit)) = caps::NAMES.iter().filter(|(_, b)| c.asked & b != 0).nth(nth as usize) else {
                    return false;
                };
                c.chosen ^= bit;
                self.show_consent();
            }
            _ => return false,
        }
        self.touched = true;
        self.redraw = true;
        true
    }

    /// The sheet is answered: take its tree back out of the session.
    ///
    /// The sheet is mounted into `self.session` like any other tree, with
    /// ids, styles and atoms of its own — and nothing was removing it. The
    /// window then dialled, and the application's first batch arrived on a
    /// session that still held the sheet: refused, resynced, refused again,
    /// and the session closed with "resync refused". So answering the
    /// question was what broke the session it was asked for, which is why
    /// `--allow` appeared to work — with nothing to ask, nothing was ever
    /// mounted.
    ///
    /// `start_over` and not a surgical removal: everything it clears — the
    /// tree, focus, the edits, the verified chunks — belongs to a session
    /// that does not exist yet. There is nothing here worth keeping, and a
    /// partial teardown is how this happened in the first place.
    fn done_asking(&mut self) {
        self.start_over();
        self.redraw = true;
    }

    /// Draw the sheet, from scratch, for what is currently chosen.
    ///
    /// Remounted on every toggle rather than patched. The tree is a dozen
    /// nodes and this runs when a finger moves, not when a frame does; a
    /// diff here would be more code to be wrong in than the whole sheet.
    fn show_consent(&mut self) {
        let Some(c) = self.consent.clone() else { return };
        let role = |r: eui_theme::Role| ColorRef::role(r.id());
        let page = StyleRecord {
            display: Display::Column,
            // Not `Justify::Center`, tempting as it is on a page this
            // small. Ten capabilities on a short phone is a column taller
            // than the window, and centred content that overflows puts its
            // top above the origin where no scroll offset can reach it —
            // which on *this* page means an answer nobody can give.
            justify: Justify::Start,
            align_items: AlignItems::Center,
            // And the other half of that: past the bottom of the window the
            // rows are still gettable to.
            overflow: eui_proto::Overflow::Scroll,
            gap: 4,
            padding: [6, 6, 6, 6],
            bg: role(eui_theme::Role::SurfaceBase),
            ..Default::default()
        };
        let heading = StyleRecord { font_size: 3, font_weight: FontWeight::Bold, fg: role(eui_theme::Role::TextDefault), text_align: TextAlign::Center, ..Default::default() };
        let hint = StyleRecord { font_size: 0, fg: role(eui_theme::Role::TextMuted), text_align: TextAlign::Center, max_width: Dim::Px(460), ..Default::default() };
        let list = StyleRecord { display: Display::Column, gap: 2, max_width: Dim::Px(460), width: Dim::Percent(10_000), ..Default::default() };
        let row = |on: bool| StyleRecord {
            display: Display::Row,
            align_items: AlignItems::Center,
            gap: 3,
            padding: [2, 3, 2, 3],
            radius: 2,
            border_width: [1, 1, 1, 1],
            border_color: role(if on { eui_theme::Role::AccentBase } else { eui_theme::Role::BorderSubtle }),
            bg: role(eui_theme::Role::SurfaceRaised),
            fg: role(if on { eui_theme::Role::TextDefault } else { eui_theme::Role::TextMuted }),
            cursor: Cursor::Pointer,
            ..Default::default()
        };
        let mark = StyleRecord { font_family: eui_proto::FontFamily::Mono, font_size: 1, ..Default::default() };
        let buttons = StyleRecord { display: Display::Row, gap: 3, justify: Justify::Center, ..Default::default() };
        let button = |accent: bool| StyleRecord {
            display: Display::Row,
            align_items: AlignItems::Center,
            justify: Justify::Center,
            padding: [2, 4, 2, 4],
            radius: 2,
            bg: role(if accent { eui_theme::Role::AccentBase } else { eui_theme::Role::SurfaceRaised }),
            fg: role(if accent { eui_theme::Role::AccentOn } else { eui_theme::Role::TextDefault }),
            border_width: [1, 1, 1, 1],
            border_color: role(if accent { eui_theme::Role::AccentBase } else { eui_theme::Role::BorderSubtle }),
            cursor: Cursor::Pointer,
            font_weight: FontWeight::Bold,
            ..Default::default()
        };

        // Styles 1..=6 are fixed; a row takes 7 when it is on and 8 when it
        // is off, so a toggle is a different style and not a new record.
        let mut ops = vec![
            Op::DefStyle { id: 1, record: page },
            Op::DefStyle { id: 2, record: heading },
            Op::DefStyle { id: 3, record: hint },
            Op::DefStyle { id: 4, record: list },
            Op::DefStyle { id: 5, record: buttons },
            Op::DefStyle { id: 6, record: mark },
            Op::DefStyle { id: 7, record: row(true) },
            Op::DefStyle { id: 8, record: row(false) },
            Op::DefStyle { id: 9, record: button(true) },
            Op::DefStyle { id: 10, record: button(false) },
            Op::DefAtom { id: ATOM_ROLE, value: "role".into() },
            Op::DefAtom { id: ATOM_LABEL, value: "label".into() },
            Op::DefAtom { id: ATOM_EVENT, value: "consent".into() },
        ];

        let wanted: Vec<(&str, u32)> = caps::NAMES.iter().copied().filter(|(_, b)| c.asked & b != 0).collect();
        let mut tree = Subtree::default();
        let button_node = |tree: &mut Subtree, id: u32, style: u32, text: &str| {
            let h = u32::try_from(tree.handlers.len()).unwrap_or(0);
            tree.handlers.push((EventKind::Click, Handler::Server(ATOM_EVENT)));
            let pr = u32::try_from(tree.props.len()).unwrap_or(0);
            tree.props.push((ATOM_ROLE, Value::Str("button".into())));
            tree.props.push((ATOM_LABEL, Value::Str(text.to_owned())));
            tree.nodes.push(FlatNode { kind: NodeKind::Box, id, style, key: 0, text: None, props: (pr, 2), handlers: (h, 1), child_count: 1 });
            tree.nodes.push(FlatNode {
                kind: NodeKind::Text,
                id: id.saturating_add(CONSENT_TEXT),
                style: 0,
                key: 0,
                text: Some(TextRef::Inline(text.to_owned())),
                props: (0, 0),
                handlers: (0, 0),
                child_count: 0,
            });
        };

        // The page: a heading, a line saying what a grant is, the rows, and
        // the two answers.
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 4 });
        let who = if c.name.is_empty() { "This application".to_owned() } else { c.name.clone() };
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 2, key: 0, text: Some(TextRef::Inline(format!("{who} is asking for:"))), props: (0, 0), handlers: (0, 0), child_count: 0 });
        tree.nodes.push(FlatNode {
            kind: NodeKind::Text,
            id: 3,
            style: 3,
            key: 0,
            // Said plainly, because the alternative is a list of words from
            // a specification: what is turned off here is refused for the
            // whole session, and the application is told nothing about it.
            text: Some(TextRef::Inline("Anything you turn off is simply not there for it.".into())),
            props: (0, 0),
            handlers: (0, 0),
            child_count: 0,
        });
        // The rows.
        let rows = u32::try_from(wanted.len()).unwrap_or(0);
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 4, style: 4, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: rows });
        for (nth, (name, bit)) in wanted.iter().enumerate() {
            let on = c.chosen & bit != 0;
            let id = CONSENT_ROW.saturating_add(u32::try_from(nth).unwrap_or(0));
            let said = cap_in_words(name);
            let h = u32::try_from(tree.handlers.len()).unwrap_or(0);
            tree.handlers.push((EventKind::Click, Handler::Server(ATOM_EVENT)));
            let pr = u32::try_from(tree.props.len()).unwrap_or(0);
            tree.props.push((ATOM_ROLE, Value::Str("checkbox".into())));
            tree.props.push((ATOM_LABEL, Value::Str(said.to_owned())));
            tree.nodes.push(FlatNode { kind: NodeKind::Box, id, style: if on { 7 } else { 8 }, key: 0, text: None, props: (pr, 2), handlers: (h, 1), child_count: 2 });
            // A mark and not a glyph from an icon font: this tree is mounted
            // before any asset has been fetched, and a box that draws
            // nothing is a row nobody can tell the state of.
            tree.nodes.push(FlatNode {
                kind: NodeKind::Text,
                id: id.saturating_add(CONSENT_MARK),
                style: 6,
                key: 0,
                text: Some(TextRef::Inline(if on { "[x]".into() } else { "[ ]".into() })),
                props: (0, 0),
                handlers: (0, 0),
                child_count: 0,
            });
            tree.nodes.push(FlatNode {
                kind: NodeKind::Text,
                id: id.saturating_add(CONSENT_TEXT),
                style: 0,
                key: 0,
                text: Some(TextRef::Inline(said.to_owned())),
                props: (0, 0),
                handlers: (0, 0),
                child_count: 0,
            });
        }
        // The two answers.
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 5, style: 5, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 2 });
        // "Save" and "Close" rather than "Allow" and "Don't allow". The rows
        // are the decision — each one turns off on its own — so the buttons
        // are what happens to it, not a second, coarser answer beside it.
        // "Don't allow" also read as a verdict on the application when what
        // it did was discard the choices above it.
        button_node(&mut tree, CONSENT_ALLOW, 9, "Save");
        button_node(&mut tree, CONSENT_DENY, 10, "Close");

        ops.push(Op::Mount(tree));
        // A fresh session, as `show_stopped` does: the ids below are this
        // client's and must not collide with any a server has defined.
        self.session = Session::new();
        self.focused = None;
        self.pointer = Pointer::default();
        if self.session.apply(&Batch { seq: 1, ops }).is_err() {
            // The sheet is this client's own tree, so this cannot happen
            // from anything a server sent — but a question nobody can see
            // is worse than none, and refusing is the safe answer.
            self.consent = None;
            self.consent_said = Some(0);
            return;
        }
        self.invalidate();
        self.redraw = true;
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
        self.locators.clear();
        self.fix = None;
        self.scans.clear();
        self.nfc_asks.clear();
        self.anims.clear();
        self.edits.clear();
        self.track = None;
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
        // The reason is a field and not a label, because a label cannot be
        // selected (04: only an editable carries a selection) and this page
        // is very often the only record of what went wrong: an application
        // that will not start has no window of its own to say it in, and a
        // window opened straight onto a URL has no chrome to fall back to.
        // Unselectable, the reason had to be copied off the screen by hand
        // or photographed — which is what it came to.
        //
        // `TextArea` and not `Input`: it wraps. A parser's complaint names
        // a file, a line and a column and does not fit on one.
        let reason = StyleRecord {
            font_size: 1,
            font_family: eui_proto::FontFamily::Mono,
            fg: role(eui_theme::Role::DangerBase),
            bg: role(eui_theme::Role::SurfaceRaised),
            max_width: Dim::Px(560),
            padding: [2, 3, 2, 3],
            radius: 2,
            ..Default::default()
        };
        let hint = StyleRecord { font_size: 0, fg: role(eui_theme::Role::TextMuted), text_align: TextAlign::Center, ..Default::default() };
        // Both modifiers already work — the driver reads control and super
        // as the same bit for an editable — so this only has to name the
        // one the person is holding.
        let keys = if cfg!(target_os = "macos") { "Select it to copy: \u{2318}A, then \u{2318}C." } else { "Select it to copy: Ctrl+A, then Ctrl+C." };
        let mut tree = Subtree::default();
        tree.nodes.push(FlatNode { kind: NodeKind::Box, id: 1, style: 1, key: 0, text: None, props: (0, 0), handlers: (0, 0), child_count: 3 });
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 2, style: 2, key: 0, text: Some(TextRef::Inline("The application stopped".into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
        tree.nodes.push(FlatNode { kind: NodeKind::TextArea, id: 3, style: 3, key: 0, text: Some(TextRef::Inline(why)), props: (0, 0), handlers: (0, 0), child_count: 0 });
        tree.nodes.push(FlatNode { kind: NodeKind::Text, id: 4, style: 4, key: 0, text: Some(TextRef::Inline(keys.into())), props: (0, 0), handlers: (0, 0), child_count: 0 });
        let batch = Batch {
            seq: 1,
            ops: vec![Op::DefStyle { id: 1, record: page }, Op::DefStyle { id: 2, record: heading }, Op::DefStyle { id: 3, record: reason }, Op::DefStyle { id: 4, record: hint }, Op::Mount(tree)],
        };
        if self.session.apply(&batch).is_err() {
            return;
        }
        // Focused here rather than with an `Op::Focus`: this batch never
        // goes near `handle_frame`, which is what would have read one, and
        // there is no server left to tell about it either way. Without it
        // the two keystrokes the hint names do nothing until the field is
        // found and clicked.
        self.focused = self.session.lookup(3);
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
        // A drag is never a spin: the hand may be at the edge of a list, and
        // 06 §6.4's scrolling is this frame's to do. Nothing has touched the
        // tree — the hand is not the tree — so without this the view would sit
        // still for exactly as long as the pointer did.
        let dragging = self.pointer.drag.is_some_and(|d| d.grabbed) || self.touch.hold.is_some();
        if !self.touched && !dragging {
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
        // Before the entrances: the page leaving takes its painting from the
        // list that still had it, and the page arriving has not been laid out
        // yet, so the two must not be resolved in the other order.
        self.note_exits();
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
        // 06 §2: a value the brake held goes out now, and it is the latest
        // -- never a backlog, because the comparison is against `sent`.
        let told = self.flush_track_change(false);
        self.pending.extend(told);
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
        self.place_tracks();
        // 06 §5.1: a contact held still long enough to mean something. It
        // sends nothing while it is being held, so the clock is read here.
        let held = self.touch_hold();
        self.pending.extend(held);
        // 06 §6.4: a drag held at the edge of a list moves it. After layout,
        // because the band is measured against the boxes this frame placed.
        let carried = self.autoscroll();
        self.pending.extend(carried);
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
        let settled = self.idle_changes();
        self.pending.extend(settled);
        let placed = self.location_events();
        self.pending.extend(placed);
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
        self.movers.retain(|(ix, m)| !m.done(now) && self.session.node(*ix).is_some());
        let movers: Vec<(NodeIx, eui_render::Mover)> = self.movers.iter().map(|(ix, m)| (*ix, m.to_paint(now))).collect();
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
            movers: &movers,
            glides: &glides,
            cache: &mut self.paint_cache,
            editing,
            now: self.now.saturating_duration_since(self.epoch).as_secs_f32(),
            // 08 §3: without the grant the painter never builds a scene, so
            // there is no target, no fetch and nothing compiled -- not a
            // check that fails, a path that is not taken.
            scenes_allowed: self.granted & caps::SCENE != 0,
            scrollbar_hot: self.pointer.dragging_thumb.map(|(s, _)| s).or(self.pointer.over_scrollbar),
            scrollbars: &bars,
        });
        self.splice_departing(&mut list, now, (device_w, device_h));
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
        let moving = !self.nothing_on_the_clock();
        self.next_due = if self.anims.is_empty() && !moving && self.scroll_anim.is_none() && !list.wants_frame {
            None
        } else if self.scroll_anim.is_some() {
            // A glide at sixty, not a hundred and twenty-five. Every frame
            // of it lays the page out again — the rows a windowed list
            // shows move with the offset — and presents; on a display that
            // refreshes at 120 Hz an 8 ms cadence asked for both twice as
            // often as the eye needs, and a glide through the docs dialog
            // was a fifth of a core on macOS.
            Some(now + Duration::from_millis(16))
        } else if self.anims.is_empty() && !moving {
            // Only a spin: half the frames a transition gets. A revolution
            // is 1.2 s (03 §5), which is 12° a frame at thirty — smooth —
            // and thirty frames is half the work of sixty, on a display
            // that would otherwise be asked for a hundred and twenty.
            //
            // A scene is the exception, and it earns it: 33 ms of a cube
            // turning is visibly stepped in a way a spinner at the same
            // cadence is not. The interval comes off the list itself, so
            // the window can keep it while repeating a `gpu_only` frame
            // without asking the driver anything.
            let scene_frame = list.scenes.iter().filter(|s| s.flags & eui_render::SCENE_ANIMATED != 0 && s.fps > 0).map(|s| Duration::from_millis(1000 / u64::from(s.fps.clamp(1, 60)))).min();
            Some(now + scene_frame.map_or(SPIN_FRAME, |d| d.min(SPIN_FRAME)))
        } else {
            Some(now + Duration::from_millis(16))
        };
        let wake_due = self.wakes.iter().map(|(_, _, at)| *at).min();
        // The same for a node that asked where the machine is: without it
        // the list outlives the interval, the cached frame is handed back
        // instead of a real paint, and the event that was due is never
        // looked for. A clock nobody winds is not a clock.
        let locate_due = self.fix.and_then(|_| self.locators.iter().map(|(_, _, at)| *at).min());
        // And the same again for a field that has been typed into (06 §2).
        // This one is the easiest of the three to get half right: a keystroke
        // sets `touched`, this frame clears it, and 300 ms later the field is
        // untouched — so the deadline has to reach `until` as well as
        // `next_due`, or the cached list is handed back and the `change` is
        // never looked for. `others` feeds both, which is the whole of it.
        let change_due = self.edits.values().filter_map(|e| e.typed_at).min().map(|t| t + CHANGE_IDLE);
        let others = [settle_due, self.video_due, self.viewport_due, wake_due, locate_due, change_due, self.caret_due];
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
        let motion_end = self
            .anims
            .iter()
            .map(|(_, a)| a.start + a.duration)
            .chain(self.movers.iter().filter(|(_, m)| m.held.is_none()).map(|(_, m)| m.start + m.duration))
            .chain(self.departing.as_ref().filter(|d| d.go.held.is_none()).map(|d| d.go.start + d.go.duration))
            .chain(self.scroll_anim.map(|a| a.start + a.duration))
            .max();
        // A sound or a picture playing reports its position four times a
        // second (03 §7, §8), from a paint: the list holds until the next
        // report, whenever the window next draws it, and no frame is asked
        // for on its account -- that would be a wake-up a playing tab did
        // not have before.
        let report_due = (!self.mixer.is_empty() || !self.players.is_empty()).then(|| self.audio_reported.map_or(now, |t| t + Duration::from_millis(250)));
        let until = others.into_iter().flatten().chain(report_due).chain(motion_end).min();
        let cadence = if !self.anims.is_empty() || moving || self.scroll_anim.is_some() { Some(Duration::from_millis(16)) } else { list.wants_frame.then_some(SPIN_FRAME) };
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

    /// Spec 06 §1.2: the nodes that asked where the machine is, answered on
    /// their own interval from the freshest fix the window handed over.
    ///
    /// Four things must all hold, and every one of them is checked here:
    /// the `location` capability was granted, the window has the input, a
    /// fix exists, and the node's interval has elapsed. Without the first
    /// there is no list to walk — a capability that was not granted has no
    /// code path, and the tree is not even read for it.
    fn location_events(&mut self) -> Vec<Frame> {
        if self.granted & caps::LOCATION == 0 {
            self.locators.clear();
            return Vec::new();
        }
        if self.locate_dirty {
            self.locate_dirty = false;
            self.collect_locators();
        }
        if self.locators.is_empty() || !self.in_front {
            return Vec::new();
        }
        let Some(fix) = self.fix else { return Vec::new() };
        let now = self.now;
        let mut due: Vec<u32> = Vec::new();
        for (id, period, at) in self.locators.iter_mut() {
            if *at <= now {
                due.push(*id);
                // From now, as a wake is: a window that was not painted for
                // a minute does not owe sixty fixes.
                *at = now + *period;
            }
        }
        let [lat, lon, accuracy] = fix.coarse();
        let mut out = Vec::new();
        for id in due {
            let Some(ix) = self.session.lookup(id) else { continue };
            out.extend(self.emit(ix, EventKind::Location, Value::List(vec![Value::Float(lat), Value::Float(lon), Value::Float(accuracy)])));
        }
        out
    }

    /// Walk the tree for the nodes that asked where the machine is. The
    /// twin of [`Self::collect_wakes`], down to keeping the phase of the
    /// ones already running.
    fn collect_locators(&mut self) {
        if self.session.root().is_none() {
            self.locators.clear();
            return;
        }
        let Some(atom) = self.session.atoms().locate else {
            self.locators.clear();
            return;
        };
        let now = self.now;
        let asked: Vec<(u32, Duration)> = self
            .session
            .locators()
            .iter()
            .filter_map(|ix| self.session.node(*ix))
            .filter(|n| n.handler(EventKind::Location).is_some())
            .filter_map(|n| match n.prop(atom) {
                Some(Value::Int(ms)) if *ms > 0 => Some((n.id, Duration::from_millis((*ms as u64).max(MIN_LOCATE_MS)))),
                _ => None,
            })
            .take(MAX_LOCATORS)
            .collect();
        let old = std::mem::take(&mut self.locators);
        self.locators = asked
            .into_iter()
            .map(|(id, period)| {
                let kept = old.iter().find(|(o, p, _)| *o == id && *p == period).map(|(_, _, at)| *at);
                (id, period, kept.unwrap_or(now))
            })
            .collect();
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
    /// The node whose handler is running, for `self` (spec 07 §3).
    ///
    /// A chunk names a node by the atom of its key, and key atom `0` — which
    /// no node can have, since an unkeyed node is never entered in the key
    /// map — means *this* one. Resolving `self` here rather than baking the
    /// key in at compile time is what lets one chunk serve every row of a
    /// list: a source compiled per key is one interned chunk per key, and
    /// the table holds 4 095.
    here: Option<NodeIx>,
    emitted: Vec<u32>,
    /// The chunk asked to go back (07 §3). Once, however often it asked: a
    /// request repeated is still one request.
    went_back: bool,
    /// Editable nodes whose text the chunk wrote, so the client's own buffer
    /// for them can be brought back into agreement afterwards. Without this a
    /// chunk can empty the field it is in and the next keystroke writes the
    /// old value straight back over it.
    texts: Vec<NodeIx>,
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
        // The VM refuses a non-finite float at every instruction that could
        // make one, so this cannot carry one onto the wire.
        eui_vm::Value::Float(f) => Value::Float(f),
        eui_vm::Value::Str(s) => Value::Str(s),
    }
}

fn from_wire(v: &Value) -> eui_vm::Value {
    match v {
        Value::Bool(b) => eui_vm::Value::Bool(*b),
        Value::Int(n) => eui_vm::Value::Int(*n),
        Value::Str(s) => eui_vm::Value::Str(s.clone()),
        // A float was truncated to an integer here while the VM had no
        // floats. It has them now, so a value read back out of local state
        // is the value that was put there.
        Value::Float(f) => eui_vm::Value::Float(*f),
        _ => eui_vm::Value::Null,
    }
}

impl SessionHost<'_> {
    /// The node a chunk named: by its key's atom, or — for atom `0` — the
    /// one the handler is on.
    fn named(&self, key: u32) -> Option<NodeIx> {
        if key == 0 {
            return self.here;
        }
        self.session.lookup_key(key)
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
        let Some(ix) = self.named(key) else {
            return false;
        };
        self.touched = true;
        if let Some(undo) = &mut self.undo {
            undo.push(Undo::Text(ix, self.session.node(ix).and_then(|n| n.text.clone())));
        }
        self.texts.push(ix);
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
    fn set_scene_uniform(&mut self, key: u32, index: u32, value: f64) -> bool {
        let Some(ix) = self.named(key) else {
            return false;
        };
        // Painted, not dirty: 03 §1.2's uniforms change what the node draws
        // and nothing it measures. `set_scene_uniform_local` makes that
        // distinction, and refuses a node that is not a scene, an index past
        // the block, and a value that is not a number.
        //
        // No undo entry. A local handler's effects are provisional and the
        // server's next batch overwrites them (07 §1); a uniform is eight
        // floats of appearance, and rolling one back would cost more than
        // the frame it saves.
        self.touched = true;
        self.session.set_scene_uniform_local(ix, index, value)
    }
    fn set_style(&mut self, key: u32, style: u32) -> bool {
        let Some(ix) = self.named(key) else {
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
    fn go_back(&mut self) {
        self.went_back = true;
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

// ------------------------------------------------------------- consent

/// What the person is being asked, and what they have said so far.
///
/// Spec 01 §2.1: a client grants the intersection of what a manifest asks
/// for with what the person allowed, and reports it in `Hello.granted`. So
/// the answer has to exist *before* the socket opens — this stands in
/// front of a session rather than over one, and there is no protocol for
/// changing its mind afterwards.
#[derive(Debug, Clone)]
struct Consent {
    /// What the manifest wants that the command line did not already give.
    asked: u32,
    /// What is still ticked. Everything, until somebody unticks something.
    chosen: u32,
    /// What the manifest calls the application.
    name: String,
}

/// The first row's node id. The rows run upward from here in bit order, so
/// a row's distance from this names the capability it stands for.
const CONSENT_ROW: u32 = 100;
/// Added to a row's or a button's id for the label inside it.
const CONSENT_TEXT: u32 = 1_000;
/// Added to a row's id for the `[x]` beside its label.
const CONSENT_MARK: u32 = 2_000;
/// The two answers.
const CONSENT_ALLOW: u32 = 10;
const CONSENT_DENY: u32 = 11;

/// Atoms the sheet defines for itself. Ids in a session of this client's
/// own making, so they collide with nothing a server ever sent.
const ATOM_ROLE: u32 = 1;
const ATOM_LABEL: u32 = 2;
const ATOM_EVENT: u32 = 3;

/// What a capability lets an application do, said to the person who has to
/// decide about it.
///
/// Not the name from 01 §2.1. `fs.pick` is a line in a specification;
/// "open files you choose" is a thing somebody can agree to or not, and a
/// permission sheet that shows the former is asking a question it knows
/// the reader cannot answer.
fn cap_in_words(name: &str) -> &'static str {
    match name {
        "camera" => "Take photographs with the camera",
        "microphone" => "Make recordings with the microphone",
        "clipboard.read" => "Read what you have copied",
        "clipboard.write" => "Put things on your clipboard",
        "notifications" => "Show you notifications",
        "location" => "Read roughly where you are",
        "fs.pick" => "Open files you choose or drop on it",
        "fs.save" => "Save files where you say",
        "nfc" => "Read a tag you hold against the machine",
        "scene" => "Draw with a graphics program of its own",
        // Every name in `caps::NAMES` is above, and the sheet only ever
        // shows rows for those. A row rather than none all the same: a
        // capability this build cannot name is still one nobody should be
        // granted without being shown a line about it.
        _ => "Something this build has no words for",
    }
}
