//! The window: a parked winit loop (see `idle_flow`), a wgpu surface, and
//! the driver.
//!
//! There is no render loop. The window redraws when a frame arrived, the
//! viewer did something, or the OS asked — and at no other time. That is
//! the whole of the zero-wakeup idle budget.

use std::sync::mpsc;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

#[cfg(has_files)]
use crate::driver::FileWant;
use crate::driver::{FileAsk, Input};
use crate::transport::{self, Connection, Incoming};
use crate::worker::Backend;

/// Why the loop woke: the transport has a message, or an assistive
/// technology wants the tree or asked for an action.
#[derive(Debug)]
pub enum Wake {
    /// A message is waiting on the connection.
    Transport,
    /// The desktop's theme changed.
    Theme,
    /// The audio thread has frames to send (a sound ended).
    Audio,
    /// A file dialog answered, or a file being read has more bytes.
    Files,
    /// The host asked the window to close (a signal, say).
    Exit,
    /// A frame the window asked to be woken for is due. See [`Timer`].
    Frame,
    /// A file is being dragged over a window, or was let go on one.
    ///
    /// Wayland only. Everywhere else winit reports a drop itself, as a
    /// window event, and there is nothing to be woken for.
    #[cfg(target_os = "linux")]
    Drop,
    /// The person clicked a notification this window raised (02 §5.2), so
    /// the window it came from should come forward.
    ///
    /// The whole of what a notification does here. Nothing goes to the
    /// server -- it is not told that one was shown, clicked or ignored --
    /// so this travels no further than the loop.
    #[cfg(not(no_subprocess))]
    Raise(WindowId),
    /// An install or an uninstall finished on its own thread, so the
    /// address row has to be drawn again to show which it is now.
    #[cfg(has_launchers)]
    Installed,
    /// AccessKit has something for the window.
    #[cfg(has_a11y)]
    Access(accesskit_winit::Event),
    /// Another `eui` handed this process its launch rather than building a
    /// second GPU stack beside this one (`crate::instance`).
    #[cfg(has_instance)]
    Open(Box<Opening>),
    /// The adapter and the device are ready, and the window that was made
    /// to ask for them is inside.
    ///
    /// Only a page. Everywhere else both are asked for and answered inside
    /// `resumed`, because the thread may block while they are; a page has
    /// one thread and it is the one drawing.
    #[cfg(target_arch = "wasm32")]
    Gpu(Box<Gpu>),
}

/// What the probe came back with: a whole [`Shared`], plus the window and
/// surface it was chosen for.
///
/// A newtype because [`Wake`] derives `Debug` and a renderer does not, and
/// because `Shared` is private and a `pub enum` may not name it.
#[cfg(target_arch = "wasm32")]
pub struct Gpu(Shared);

#[cfg(target_arch = "wasm32")]
impl std::fmt::Debug for Gpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad("Gpu { .. }")
    }
}

#[cfg(has_a11y)]
impl From<accesskit_winit::Event> for Wake {
    fn from(e: accesskit_winit::Event) -> Self {
        Wake::Access(e)
    }
}

/// The GPU, once, for every window in the process.
///
/// A second window costs no adapter, no device, no pipelines and no naga
/// output — which is most of what makes the first window's first pixel
/// expensive. What it does cost is its own surface, and every tab in it its
/// own textures.
struct Shared {
    /// Kept because every later surface is created from it, and because a
    /// surface must not outlive the instance it came from.
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    renderer: eui_render::Renderer,
    /// The window and surface the probe had to make before it could ask
    /// about them, waiting for the first [`Shell::open`] to take them.
    ///
    /// Only a page has this, and only once. wgpu's WebGL2 backend
    /// enumerates adapters *out of* a canvas's GL context, so `None` here
    /// is not a choice — an adapter cannot be asked for until there is a
    /// surface to ask about, which means the window exists before the
    /// renderer does, which is the opposite of every other target.
    #[cfg(target_arch = "wasm32")]
    made: Option<(Arc<Window>, wgpu::Surface<'static>)>,
}

/// A launch that arrived from another process: what one `eui` hands to the
/// one already running (`crate::instance`).
#[cfg(has_instance)]
#[derive(Debug, Clone)]
pub struct Opening {
    /// The applications to open, one tab each. Empty asks for a shell.
    pub launches: Vec<Launch>,
    /// Whether the window gets a tab strip and an address bar.
    pub chrome: bool,
    /// What the person granted on *that* command line, which is theirs to
    /// grant: the socket is same-user only, and running the binary is the
    /// same act.
    pub allowed: u32,
}

/// How to open an application: what the `eui` binary parses from its
/// command line, and what an embedding host fills in itself.
#[derive(Debug, Clone)]
pub struct Launch {
    /// The session URL, `wss://host/_eui/session/app`.
    pub url: String,
    /// Capabilities the person allows, if the manifest asks for them.
    pub allowed: u32,
    /// The window title.
    pub title: String,
    /// A `name=value` cookie to present on every request — a desktop host's
    /// loopback gate. `None` for a network session.
    pub cookie: Option<String>,
    /// The host embeds the server in this process: `ws://` on loopback is
    /// trusted (08 §1), and a missing manifest is tolerated.
    pub host_loopback: bool,
}

impl Launch {
    /// A network session, as the `eui` binary opens it.
    ///
    /// The address is normalised here, at the one door a typed address comes
    /// through, rather than in each place that later looks at its scheme:
    /// `eui https://host/...` is what somebody copying their address bar
    /// will write, and it names the same origin as `wss://host/...`.
    pub fn new(url: String, allowed: u32) -> Self {
        let url = crate::assets::normalise_url(&url);
        Self { url, allowed, title: "EUI".into(), cookie: None, host_loopback: false }
    }
}

/// What a tab's socket is doing (spec 01 §4.1).
///
/// A socket that breaks is not an application that ended. The session lives
/// on the server, the tree lives here, and between them a broken socket is
/// a gap to be closed — so the client closes it, rather than leaving a
/// window that looks alive and answers nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// Where there is no manifest there is no consent sheet, so a handful of
// the things that serve one are constructed nowhere. They are still
// compiled — `Link` is matched exhaustively, and a variant behind a `cfg`
// would move that cost to every match arm in the file — so the dead-code
// lint is told the reason rather than worked around.
#[cfg_attr(not(has_pins), allow(dead_code))]
enum Link {
    /// Talking.
    Up,
    /// A socket is open and has not spoken yet. `until` is when to stop
    /// waiting for it: a server that accepts a connection and says nothing
    /// is not one that is coming back on its own.
    Trying {
        /// When to give up on this attempt and try again.
        until: crate::time::Instant,
    },
    /// Nothing is open. `at` is when the next attempt is due.
    Lost {
        /// When to try again.
        at: crate::time::Instant,
    },
    /// Ended for a reason another socket cannot fix: the manifest was
    /// refused, the server sent an `Error`, the tree was unusable.
    Ended,
    /// Spec 01 §2.1: the consent sheet is up and nothing has been dialled.
    /// `Hello` carries the grant, so the question has to be answered
    /// before there is a socket to ask it on.
    Asking,
    /// The page came over `GET /_eui/view/<component>` (01 §2.4) and there
    /// is no socket, because nothing has needed one. This is not a degraded
    /// state and not a broken one: for a component whose first render is the
    /// same for everybody it is the whole session, and the server is holding
    /// nothing at all for this reader.
    Static,
}

/// A consent sheet that is up, and what it will have to remember.
#[cfg_attr(not(has_pins), allow(dead_code))]
#[derive(Clone)]
struct Asking {
    /// Whose answer this is.
    app_id: String,
    /// Everything this manifest asked for that the person has now been
    /// shown — the rows on the sheet, plus anything they had already
    /// settled on a previous run. What gets written down as asked.
    asked: u32,
}

/// How long to wait before the `tries`-th attempt: 300 ms doubling to
/// half a minute, and half a minute from then on.
///
/// The first gap is short because the common break is a wifi hop or a
/// laptop lid, which is over before a person has finished looking at the
/// window. The ceiling is there because the other common break is a server
/// being deployed, and a client that hammers it while it comes up is part
/// of the outage.
fn backoff(tries: u32) -> std::time::Duration {
    let ms = 300u64.saturating_mul(1u64 << tries.min(7));
    std::time::Duration::from_millis(ms.min(30_000))
}

/// The steps `Ctrl +` and `Ctrl -` move between, which are a browser's.
///
/// A ladder rather than a factor: multiplying by 1.1 and dividing by it
/// again does not come back to 1.0, so a person who zoomed in and out the
/// same number of times would be left at 99.99 % with no way of saying so.
/// Every step here is a number a person could name.
const ZOOM: [f32; 13] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0];

/// Where 100 % sits in [`ZOOM`], so `Ctrl 0` and a new tab agree with it.
const ZOOM_ONE: usize = 5;

/// The step next to `z`, in the direction `up`; `z` itself at either end.
///
/// The step it starts from is the nearest one rather than an exact match,
/// so a level that came from somewhere else — a rounding, a future setting
/// — still moves, instead of sticking because it is between two rungs.
/// The modifier the window's own shortcuts are held with: **Command on
/// macOS, Control everywhere else** (06 §1: 1 shift, 2 control, 4 alt,
/// 8 super).
///
/// It is the platform's convention, and it is also what makes a terminal
/// drawn inside a page usable: `Ctrl+W`, `Ctrl+T` and `Ctrl+V` are a word, a
/// transposition and a quoted insert to every shell there is, and a window
/// that ate them on a Mac left an application unable to receive the keys its
/// subject is defined by.
const SHELL_MOD: u32 = if cfg!(target_os = "macos") { 0b1000 } else { 0b0010 };

/// And the one that goes back with `ArrowLeft`: `⌘←` on macOS, `Alt+←`
/// elsewhere — where `Option+←` is "a word to the left" and nothing else.
/// `back()` declines when there is nowhere to go, and the key then falls
/// through to the page, so a window opened straight onto one address never
/// takes it at all.
const BACK_MOD: u32 = if cfg!(target_os = "macos") { 0b1000 } else { 0b0100 };

/// Whether `Shift` must be held too for the window's own tab and paste
/// chords. On a Mac it must not: `⌘T` is `⌘T`. Everywhere else it must,
/// because plain `Ctrl+T`, `Ctrl+W` and `Ctrl+V` are **the application's** —
/// transpose, delete-word and quoted-insert to every shell there is — and
/// `Ctrl+Shift+` is what a terminal emulator has always used for its own
/// tabs and its own paste. A window that took the unshifted three made an
/// embedded terminal unusable on exactly the keys a terminal is used with.
const SHELL_NEEDS_SHIFT: bool = !cfg!(target_os = "macos");

fn zoom_step(z: f32, up: bool) -> f32 {
    let mut near = ZOOM_ONE;
    let mut best = f32::INFINITY;
    for (i, s) in ZOOM.iter().enumerate() {
        let d = (s - z).abs();
        if d < best {
            best = d;
            near = i;
        }
    }
    let want = if up { near.saturating_add(1) } else { near.wrapping_sub(1) };
    ZOOM.get(want).copied().unwrap_or_else(|| ZOOM.get(near).copied().unwrap_or(1.0))
}

/// What a dialog thread answers with.
#[derive(Debug)]
#[cfg_attr(not(has_files), allow(dead_code))]
enum Dialog {
    /// An open dialog: the ask it answers, and what was chosen with the
    /// size of each.
    Picked(u32, Vec<(std::path::PathBuf, u64)>),
    /// A save dialog: the ask, and where the file goes.
    Saving(u32, std::path::PathBuf),
    /// Dismissed without choosing.
    Dismissed(u32),
}

/// A file being read for an upload, on its own thread so a slow disk never
/// holds a frame.
struct Reading {
    /// The upload id the driver minted.
    id: u32,
    /// Chunks, in order; `true` on the last.
    rx: mpsc::Receiver<Result<(Vec<u8>, bool), String>>,
}

/// A file being written for a save. The handle is opened on the first
/// chunk, so a save the server never answers leaves nothing behind.
struct Writing {
    /// Where the person said it goes.
    path: std::path::PathBuf,
    /// Open once bytes have arrived.
    file: Option<std::fs::File>,
}

/// The tab's side of spec 03 §3.2: the dialogs the tree asked for, and the
/// transfers they start.
///
/// The window does the filesystem, as it does the socket and the GPU: a
/// worker cannot open a file and must not be able to. What crosses between
/// them is a name, a size, and opaque bytes.
struct Files {
    /// Where dialog threads report.
    tx: mpsc::Sender<Dialog>,
    /// Where this tab collects them.
    rx: mpsc::Receiver<Dialog>,
    /// Files being read, by upload.
    reading: Vec<Reading>,
    /// Files being written, by the ask that chose the path.
    writing: std::collections::HashMap<u32, Writing>,
}

impl Default for Files {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx, reading: Vec::new(), writing: std::collections::HashMap::new() }
    }
}

impl std::fmt::Debug for Files {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Files").field("reading", &self.reading.len()).field("writing", &self.writing.len()).finish()
    }
}

/// One application, and nothing of the window it happens to be shown in.
///
/// This is the isolation boundary, and it is deliberately narrow. A tab has
/// its own confined worker *process* — where the platform allows one — its
/// own connection and cookie, its own glyph and image textures, and its own
/// One island's socket (01 §2.7).
///
/// An island is an ordinary session that happens to be addressed by a prop,
/// so this is an ordinary `Connection` — the only thing that makes it an
/// island is that its frames apply under an owner and its events go back
/// here rather than to the page.
struct IslandSocket {
    /// Its index in the driver's tables.
    owner: u16,
    /// The path it was opened for, whole. The key for sharing: two islands
    /// naming one path share this socket.
    path: String,
    conn: Connection,
}

/// sound. It has no handle on the window, no handle on any other tab, and
/// no way to ask for either: everything it could reach that is shared is
/// either read-only (the wgpu device) or lives in the window's process and
/// is never named in a frame.
struct Tab {
    /// The session URL, whole.
    url: String,
    /// Capabilities the person allows, if the manifest asks for them.
    allowed: u32,
    /// The driver: in a worker process when one could be started.
    backend: Backend,
    conn: Option<Connection>,
    /// How this tab fetches assets, with or without a socket. A page that
    /// arrived over HTTPS may still name a picture.
    fetch: Option<crate::transport::Fetcher>,
    /// 01 §2.7: a socket per island, beside the page's — which the page may
    /// not even have. Empty for every page with no island, which is almost
    /// all of them.
    islands: Vec<IslandSocket>,
    /// Where answers land while there is no `Connection` to carry them.
    inbox: Option<std::sync::mpsc::Receiver<Incoming>>,
    /// Something needed the server, so a socket is wanted before the next
    /// frame is drawn. A flag rather than a dial on the spot: `send` is
    /// called from deep inside the input path and has no event loop proxy to
    /// hand `dial`, and the difference to a person is one frame.
    want_socket: bool,
    /// Frames the driver produced while there was no socket, held until one
    /// is open and has answered. Not flushed on connect: they name node ids
    /// from the tree this tab is looking at, and a server that mounts a
    /// fresh one would place them somewhere else entirely.
    queued: Vec<Vec<u8>>,
    /// The cookie this tab presents, if a host set one. Per tab rather than
    /// per process: two applications must not present each other's.
    cookie: Option<String>,
    /// This tab's server is embedded in this process (08 §1). Per tab, so an
    /// embedded one cannot vouch for a network one beside it.
    host_loopback: bool,
    /// This tab's glyph, image and blur textures. Per tab because a worker
    /// picks its own uv coordinates: one texture behind two tabs would let
    /// one application sample the other's rendered text.
    textures: eui_render::SessionTextures,
    /// The audio device, open only while something is loaded (03 §7).
    #[cfg(has_audio)]
    audio: Option<crate::audio::Output>,
    /// Frames the audio thread produced, for the loop to send.
    #[cfg(has_audio)]
    audio_rx: Option<mpsc::Receiver<Vec<u8>>>,
    /// Why this tab's session ended, if it did. The blank page a dead tab
    /// falls back to shows it, in a field, so it can be selected and pasted
    /// into a bug report — which is what a diagnostic is for.
    trouble: Option<String>,
    /// What the strip calls it: the manifest's name, or the last path
    /// segment until the manifest arrives.
    title: String,
    /// This session has sent a frame, so the address is one that answers.
    /// Only then is it worth offering again on a blank page.
    answered: bool,
    /// Spec 01 §2.1: the consent sheet is up, and nothing has been
    /// dialled. `None` once it has been answered, and on every session
    /// that had nothing to ask — which is every `ws://` loopback one,
    /// because those carry no manifest to ask about.
    asking: Option<Asking>,
    /// The same question, kept after it has been answered so the chrome's
    /// padlock can put it again. `asking` is taken the moment there is an
    /// answer — that is what stops it being asked twice — so it cannot also
    /// be the record of what was asked.
    perms: Option<Asking>,
    /// What the address bar says about the origin.
    trust: crate::chrome::Trust,
    /// What the socket is doing.
    link: Link,
    /// Sockets that broke or refused since the last one that spoke. It is
    /// the backoff's exponent, and a frame arriving resets it.
    tries: u32,
    /// Dialogs and transfers.
    files: Files,
    /// The link word the chrome was last told about.
    shown_link: Option<&'static str>,
    /// Whether this address can go in the desktop's launcher, and whether
    /// it is there. `None` until the manifest says, and for every manifest
    /// that publishes no icon.
    ///
    /// Held rather than asked for on every rebuild: the chrome is rebuilt
    /// on every keystroke in the address bar and the answer is a file read.
    installable: Option<bool>,
    /// How much larger this page is drawn than the display asks for.
    ///
    /// Per tab, as a browser's zoom is per site: two applications open
    /// beside each other were not written at the same size, and a person
    /// who made one readable did not ask for the other to change.
    zoom: f32,
    /// The addresses this tab has been at, oldest first, and where in them
    /// it is standing. Opening a new one from anywhere but the end drops
    /// everything after it, which is what every back button has always done.
    ///
    /// It survives `open_url`, which replaces the whole `Tab`: a history
    /// that a step through it threw away would be a back button that works
    /// once. Carried across the replacement beside the zoom, and for the
    /// same reason — neither belongs to the session, both belong to the tab.
    history: Vec<String>,
    /// Where in `history` this tab is standing.
    at: usize,
}

/// What opening an address does to the tab's trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trail {
    /// A new destination: it goes on the end, and anything ahead is dropped.
    Push,
    /// A reload: the trail is untouched and the tab stands where it stood.
    Stay,
    /// A step through it: this entry, without adding one.
    At(usize),
}

/// One window: the surface, the chrome, and the applications in it.
///
/// The window-level state that used to sit on a session lives here, because
/// several tabs share one of each: one surface, one accessibility adapter,
/// one clipboard, one theme watcher, one pointer.
struct Shell {
    /// The Wayland data device, watched on its own thread, or `None`
    /// everywhere winit reports a drop by itself — X11, Windows, macOS,
    /// the phones — and on a compositor with no data device to watch.
    ///
    /// Declared before `window` so the drop glue stops the thread before
    /// the surface it holds a pointer to can go. `close` does it by name
    /// as well, because `close` does not use the drop glue.
    #[cfg(target_os = "linux")]
    dnd: Option<crate::wayland::Drops>,
    /// The compositor's keymap and a keyboard state this window moves from
    /// the keys it sees — what a key is *named* from, on Wayland, rather
    /// than winit's `text`. `None` everywhere else, and on a compositor
    /// whose keymap could not be read; then `text` is what it was.
    /// Declared before `window` for the same reason `dnd` is: it holds
    /// objects on the window's connection.
    #[cfg(target_os = "linux")]
    keys: Option<eui_wayland::Keys>,
    window: Arc<Window>,
    /// `None` between a suspend and the resume that follows it. Android
    /// destroys the native window when the application goes to the
    /// background and hands back a new one when it returns, so the surface
    /// is the window's, not the shell's, and it is made again each time.
    /// On a desktop it is `Some` from `open` until the window closes.
    surface: Option<wgpu::Surface<'static>>,
    config: wgpu::SurfaceConfiguration,
    /// Whether the platform's positioning is running, so it is started and
    /// stopped on the edge rather than told again every frame.
    locating: bool,
    /// The tab strip and address bar, and the textures they draw into.
    /// `None` for a window opened on a URL: `eui <url>` is one application
    /// in one chromeless window, which is what an embedding host gets.
    chrome: Option<(crate::chrome::Chrome, eui_render::SessionTextures)>,
    /// The device went away and every session has been told. Kept so the
    /// reason is reported once rather than on every frame that follows it.
    gpu_gone: bool,
    /// What the person allowed on the command line (`--allow`), for every
    /// tab this window opens.
    ///
    /// The window's, not a tab's, and this is the whole point of it: every
    /// navigation — a typed address, a link, a reload, a step back —
    /// builds a *new* `Tab` through `go_to`, and that used to pass a grant
    /// of zero. So `eui <url> --allow fs.pick` lost the grant on the first
    /// reload, and the shell never had one at all. Spec 01 §2.1: what the
    /// person allowed is intersected with what the manifest asks for, and
    /// what the person allowed does not expire when a page does.
    allowed: u32,
    /// The applications, in strip order.
    tabs: Vec<Tab>,
    /// Which of them is shown and takes the input.
    active: usize,
    modifiers: u32,
    /// The modifier keys seen pressed and not yet released, as the same
    /// bits. Kept apart from `modifiers` because the two disagree: on
    /// Wayland under Hyprland, `ModifiersChanged` came in *between* the
    /// arrows of a held `Alt` saying 0, 4, 0, 4 — the trace showed one
    /// `Alt` press, twenty arrows, one `Alt` release, and half the arrows
    /// without the bit. A key that was pressed and not released is held,
    /// whatever the compositor says in the meantime, and these bits are
    /// OR-ed into every reading. Cleared when the window loses focus,
    /// since the release then goes to somebody else.
    held: u32,
    /// A size the compositor asked for and this window has not drawn yet.
    /// Only the last one matters: see the `Resized` arm.
    pending_resize: Option<winit::dpi::PhysicalSize<u32>>,
    /// Where the pointer last was, in the window's own logical pixels.
    pointer_at: Option<(f32, f32)>,
    /// A file is over this window and has not been let go or taken away.
    ///
    /// Only ever set by winit's `HoveredFile`, so it stays false on
    /// Wayland — where `eui-wayland` is told where the file is, and needs
    /// no polling at all.
    hovering: bool,
    /// Where the poll last found the pointer while `hovering`, so a hand
    /// holding still costs one cursor query a tick rather than a hit
    /// test, a worker round trip and a frame.
    hover_at: Option<(f32, f32)>,
    /// Whether that was over the application rather than the chrome. Kept
    /// rather than recomputed so a button pressed in one and released in
    /// the other does not arrive as half a click in each.
    pointer_in_app: bool,
    proxy: Proxy,
    #[cfg(has_a11y)]
    access: Option<accesskit_winit::Adapter>,
    #[cfg(has_clipboard)]
    clip: Option<arboard::Clipboard>,
    /// The pointer shape last handed to the window.
    cursor: eui_proto::Cursor,
    /// Where the input method was last pointed, or `None` if the window was
    /// last told no method is welcome.
    ///
    /// The window's, not the active tab's: the platform keeps one input
    /// method state per window, and the chrome's address bar has no tab at
    /// all. Kept on a tab, the shell — which has no tabs — could never
    /// record what it had said, so every pass of the loop said it again.
    ime_area: Option<[f32; 4]>,
    /// How much of the window's bottom the platform's soft keyboard is
    /// over, last time it was asked. The window's, for the same reason as
    /// `ime_area`: there is one keyboard, and it is the window's.
    covered: f32,
    /// The desktop theme watcher, alive as long as the window.
    #[cfg_attr(not(has_desktop_theme), allow(dead_code))]
    theme_watch: Option<Box<dyn std::any::Any + Send>>,
    /// The palette the chrome was last put in, or `None` while it has not
    /// been told one.
    ///
    /// The window's, not a tab's: the chrome is drawn once, whatever is
    /// open below it, so what it was last told is a property of the window.
    chrome_mode: Option<eui_proto::ThemeMode>,
    /// The palette the *platform* says it is in, as last heard.
    ///
    /// Kept apart from `chrome_mode`, which follows whatever the visible
    /// application ended up in and is overwritten every pass. This one is
    /// what the machine said, and it is what a tab opened later is built
    /// in: a driver made in the wrong palette sends a `Hello` saying so,
    /// and the server renders a light page before anything can correct it.
    platform_mode: Option<eui_proto::ThemeMode>,
    /// The desktop palette last applied.
    #[cfg(has_desktop_theme)]
    desktop_theme: Option<crate::desktop_theme::DesktopTheme>,
    /// A theme wake is queued and not yet handled.
    theme_pending: Arc<std::sync::atomic::AtomicBool>,
    /// When this window started, so the renderer can be handed a monotonic
    /// clock in seconds.
    epoch: crate::time::Instant,
    /// Something happened that could have left a dialog to open or bytes to
    /// move, so the next pass of the loop asks the driver about files.
    ///
    /// `about_to_wait` runs on every pass, and on a platform where the loop
    /// wakes for reasons of its own that is a great many; two locks a tab a
    /// pass to be told "nothing" is a cost an idle window must not carry.
    /// What can produce work is short and known: an input, a frame, a
    /// dialog thread's answer, and a transfer already in flight.
    files_dirty: bool,
    /// Frames drawn since the window opened. Only ever read by
    /// [`LoopStats`], and only when it is asked for.
    frames: u64,
    /// Whether the page has been told this window drew an application.
    /// Once per window: an embed reveals its canvas on it and has nothing to
    /// do with a second.
    #[cfg(target_arch = "wasm32")]
    announced: bool,
}

/// A second's worth of loop activity, for finding out why a window that
/// ought to be asleep is not.
///
/// An idle window parks: no passes of the loop, no frames. When one costs a
/// core instead, the useful question is which of three things it is doing,
/// and nothing outside the process can answer it — `top` and Activity
/// Monitor both say "busy" and stop there. Two numbers separate them:
///
/// - **passes and frames both high**: something asks for a redraw on every
///   pass, and the loop is drawing as fast as the platform allows.
/// - **passes high, frames none**: the loop is being woken and drawing
///   nothing, so a wake source is chattering rather than the tree.
/// - **neither**: the main thread is asleep and the cost is on some other
///   thread, so this is the wrong place to be looking.
///
/// Off unless `EUI_LOOP_STATS=1`, and one line a second when it is on: this
/// is a thing to turn on when a window is misbehaving, not a thing to leave
/// running.
#[derive(Debug)]
struct LoopStats {
    /// When the second being counted began.
    since: crate::time::Instant,
    /// Passes of `about_to_wait` since then.
    passes: u64,
    /// Frames drawn by every window at that moment, so the difference is
    /// what this second cost.
    frames_at: u64,
    /// Wakes sent to the loop from elsewhere in the process, by kind. A
    /// loop that passes far more often than it draws is being woken, and
    /// this says by whom — which is the difference between a tree that
    /// asks for too many frames and a thread that will not stop talking.
    wakes: [u64; 7],
    /// Passes that parked with no deadline at all, and passes that armed
    /// one. Both park the loop on `Wait`; the deadline is the timer's.
    waits: u64,
    untils: u64,
    /// The shortest `WaitUntil` asked for, in microseconds. A loop that
    /// spins while asking to sleep is asking for something the platform
    /// will not give it.
    shortest_us: u64,
    /// Which of the two asked for that shortest sleep: the window's own
    /// next frame, or a socket waiting to be tried again.
    shortest_from: &'static str,
    /// Passes where a retry was the sooner of the two.
    retry_won: u64,
    /// Window events delivered, by kind. A loop that parks properly and
    /// wakes anyway is being handed something; this says what.
    wevents: [u64; 6],
    /// Time spent inside `about_to_wait` itself, summed.
    ///
    /// A loop that passes a hundred thousand times a second is either doing
    /// a hundred thousand small pieces of our work or being woken a hundred
    /// thousand times for none of it, and those want opposite fixes. This
    /// is the number that tells them apart: against the wall-clock second
    /// beside it, it says what share of the core is this crate's.
    body_us: u64,
    /// Every `WaitUntil` delta added up, so the *mean* sleep asked for can
    /// be read off. The minimum alone is a trap: one pass landing on the
    /// deadline makes a loop that sleeps properly look like one that never
    /// sleeps at all.
    total_us: u64,
}

/// The names of [`LoopStats::wevents`], in its order.
const WEVENT_NAMES: [&str; 6] = ["redraw", "cursor", "occluded", "resized", "focus", "other"];

/// The names of [`LoopStats::wakes`], in its order.
const WAKE_NAMES: [&str; 7] = ["transport", "audio", "files", "theme", "access", "frame", "drop"];

impl LoopStats {
    /// A counter, if the environment asked for one.
    fn asked_for() -> Option<Self> {
        std::env::var("EUI_LOOP_STATS").is_ok_and(|v| v == "1").then(|| Self {
            since: crate::time::Instant::now(),
            passes: 0,
            frames_at: 0,
            wakes: [0; 7],
            waits: 0,
            untils: 0,
            shortest_us: u64::MAX,
            shortest_from: "-",
            retry_won: 0,
            wevents: [0; 6],
            body_us: 0,
            total_us: 0,
        })
    }
}

/// Append one chunk of a save to the file the person named, opening it on
/// the first one. `Ok(true)` when the file is whole.
///
/// Anything that goes wrong takes the partial file with it — an abort from
/// the server, a disk that filled, a directory that went away. Half an
/// export is worse than none: it looks like a whole one until it is opened.
fn append_write(slot: &mut Writing, flag: eui_proto::Chunked, bytes: &[u8]) -> Result<bool, String> {
    use std::io::Write as _;
    let fail = |slot: &mut Writing, why: String| {
        // Closed before it is removed, which is the whole point of taking
        // it. `wasm32` has no files, so there its `File` is a type with
        // nothing to drop and clippy says so; the order still matters
        // everywhere a file exists.
        #[cfg_attr(target_arch = "wasm32", allow(clippy::drop_non_drop))]
        drop(slot.file.take());
        let _ = std::fs::remove_file(&slot.path);
        Err(why)
    };
    if matches!(flag, eui_proto::Chunked::Abort) {
        let why = String::from_utf8_lossy(bytes).into_owned();
        return fail(slot, why);
    }
    if slot.file.is_none() {
        match std::fs::File::create(&slot.path) {
            Ok(f) => slot.file = Some(f),
            Err(e) => return Err(e.to_string()),
        }
    }
    let last = matches!(flag, eui_proto::Chunked::Last);
    let wrote = match slot.file.as_mut() {
        Some(f) => f.write_all(bytes).and_then(|()| if last { f.flush() } else { Ok(()) }),
        None => Ok(()),
    };
    match wrote {
        Ok(()) => Ok(last),
        Err(e) => fail(slot, e.to_string()),
    }
}

/// Read `path` into `tx` a chunk at a time, waking the loop for each. Stops
/// on the first error, which the driver turns into an abort on the wire.
fn read_chunks(path: &std::path::Path, tx: &mpsc::SyncSender<Result<(Vec<u8>, bool), String>>, wake: impl Fn()) {
    use std::io::Read as _;
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(Err(e.to_string()));
            wake();
            return;
        }
    };
    let mut buf = vec![0u8; eui_proto::limits::MAX_TRANSFER_CHUNK_BYTES];
    loop {
        let n = match file.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                let _ = tx.send(Err(e.to_string()));
                wake();
                return;
            }
        };
        let last = n == 0;
        let chunk = buf.get(..n).unwrap_or(&[]).to_vec();
        if tx.send(Ok((chunk, last))).is_err() {
            return;
        }
        wake();
        if last {
            return;
        }
    }
}

/// Fetch what an entry needs and write it. On the install thread.
#[cfg(all(has_launchers, has_pins, has_native_net))]
fn add(url: &str) -> Result<(), String> {
    // Not launched here, unlike the command line's `--install`: the
    // application is already open in the tab the person pressed the button
    // in, and a second window of what they are looking at is not what they
    // asked for.
    crate::install::install(&crate::install::from_url(url)?).map(|_| ())
}

/// A build that cannot verify a manifest will not install one either.
#[cfg(all(has_launchers, not(all(has_pins, has_native_net))))]
fn add(_url: &str) -> Result<(), String> {
    Err("this build cannot verify a manifest, so it will not install one".into())
}

/// Read a file for an upload on its own thread, a chunk at a time.
///
/// The channel holds two chunks: the disk runs ahead of the socket by that
/// much and no further, so a large attachment costs a fixed amount of
/// memory however fast the disk is and however slow the network.
fn start_reading(t: &mut Tab, id: u32, path: std::path::PathBuf, proxy: &Proxy) {
    let (tx, rx) = mpsc::sync_channel::<Result<(Vec<u8>, bool), String>>(2);
    let proxy = Arc::clone(proxy);
    let spawned = std::thread::Builder::new().name("eui-upload".into()).spawn(move || {
        read_chunks(&path, &tx, || {
            let _ = proxy.send_event(Wake::Files);
        });
    });
    match spawned {
        Ok(_) => t.files.reading.push(Reading { id, rx }),
        Err(e) => {
            eprintln!("eui: no thread to read the file: {e}");
            let frames = t.backend.upload_failed(id, "no thread to read the file".into());
            t.send(frames);
        }
    }
}

/// Split a session URL into the origin a publisher key is pinned to and
/// everything after it. The origin is the half that carries the trust, so
/// it is the half the address bar makes legible.
fn split_origin(url: &str) -> (&str, &str) {
    let after = url.find("//").map_or(0, |i| i + 2);
    match url[after..].find('/') {
        Some(i) => url.split_at(after + i),
        None => (url, ""),
    }
}

/// The whole session URL an address with no path meant, or `None` for one
/// that already names a path.
///
/// Spec 01 §2.1 puts `entry` in the manifest — "session path, defaults to
/// `/_eui/session`" — and the client had never read it, so every address had
/// to be typed down to the protocol's own prefix, which is exactly the part
/// nobody should have to know. `wss://host` is an origin, and the
/// application's own manifest is what says where its session is.
///
/// Following it adds nothing a server could not already do. `entry` rides
/// inside the manifest body the publisher's key signs, 01 §2.1 refuses one
/// that is not an absolute path, and the path it names is on the origin the
/// key was pinned to — the same origin the client was about to connect to
/// anyway.
#[cfg_attr(not(has_pins), allow(dead_code))]
pub fn completed(url: &str, entry: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    if !path.is_empty() || host.is_empty() || !entry.starts_with('/') {
        return None;
    }
    Some(format!("{scheme}://{host}{entry}"))
}

/// What to call a tab before its manifest arrives: the last path segment,
/// which for `…/_eui/session/gallery` is the component's own name.
fn name_from_url(url: &str) -> String {
    let (_, path) = split_origin(url);
    let last = path.rsplit('/').find(|s| !s.is_empty()).unwrap_or("");
    if last.is_empty() {
        split_origin(url).0.trim_start_matches("wss://").trim_start_matches("ws://").to_owned()
    } else {
        last.to_owned()
    }
}

/// Which of those words a socket in this state is owed, as a rule rather
/// than as a method, so it can be checked without standing a tab up: the
/// states it distinguishes are the states a person reads off the address
/// bar, and three of the five say nothing at all.
fn link_word_of(link: Link, answered: bool) -> Option<&'static str> {
    match link {
        Link::Up => None,
        Link::Trying { .. } | Link::Lost { .. } => Some("reconnecting"),
        // A tab that never had a session says nothing: the reason it
        // has none is already in the tab, and "offline" over an
        // address that was refused would name the wrong fault.
        Link::Ended if answered => Some("offline"),
        Link::Ended => None,
        Link::Asking => Some("permission"),
        // 01 §2.4: a page, not a socket that is down — so the word has
        // to be one that names a state and not a fault. "offline" over
        // a page that is fully drawn and answering its own clicks names
        // one that does not exist.
        //
        // But saying nothing was worse, and this is the line that was
        // wrong. The address bar shows the manifest's `entry`, which is
        // a `wss://…/_eui/session/<component>` — the application's
        // address, and the right thing to show — while this tab has no
        // session at all and the server is holding nothing for the
        // reader. With no word beside it, a `wss://` in the bar reads as
        // a socket, and the one state the endpoint exists to produce is
        // the one a person cannot see they are in.
        Link::Static => Some("page"),
    }
}

impl Tab {
    /// Open an application: its worker, its manifest check, its connection.
    ///
    /// `None` only when the worker could not be started — a refused
    /// manifest is reported in the tab rather than losing it, because in a
    /// shell the tab is where a person would look for the reason.
    fn open(launch: Launch, proxy: Proxy, renderer: &eui_render::Renderer, w: f32, h: f32, scale: f32, mode: Option<eui_proto::ThemeMode>) -> Self {
        // The driver — decoding, layout, the VM — in its own confined
        // process where the platform allows (08 §10); this process keeps
        // the window, the GPU and the network. One per tab: an application
        // that dies takes its own process with it and nothing else.
        let (backend, how) = Backend::open(w, h, scale, 0);
        eprintln!("eui {}: {how}", crate::BUILD);
        let mut tab = Tab {
            trouble: None,
            title: name_from_url(&launch.url),
            url: launch.url,
            // 11 §2.5: a scene's shader is not run on a GL backend, whatever
            // the person allowed -- the host generates unchecked indexing
            // there, and the shader is the server's. Masked here rather than
            // checked at the draw, so the capability is never granted and
            // the module is never even fetched (08 §3).
            allowed: if renderer.grants_scenes() {
                launch.allowed
            } else {
                if launch.allowed & eui_proto::caps::SCENE != 0 {
                    eprintln!("eui: this adapter is GL; `scene` is not offered on it (11 §2.5)");
                }
                launch.allowed & !eui_proto::caps::SCENE
            },
            backend,
            conn: None,
            cookie: launch.cookie,
            host_loopback: launch.host_loopback,
            textures: renderer.session(),
            #[cfg(has_audio)]
            audio: None,
            #[cfg(has_audio)]
            audio_rx: None,
            answered: false,
            asking: None,
            perms: None,
            trust: crate::chrome::Trust::Unverified,
            link: Link::Ended,
            fetch: None,
            islands: Vec::new(),
            inbox: None,
            want_socket: false,
            queued: Vec::new(),
            tries: 0,
            files: Files::default(),
            shown_link: None,
            installable: None,
            zoom: 1.0,
            history: Vec::new(),
            at: 0,
        };

        // Before the manifest and before `dial`: the driver is born light,
        // and a `Hello` built from it would tell the server so.
        tab.start_in(mode);

        // Spec 01 §2.1: the manifest first. Its signature is verified and
        // its key pinned before a byte of the session is trusted; only the
        // debug loopback of 08 §1 may go on without one.
        //
        // A page does neither, and says so rather than appearing to. There
        // is no store to pin a key in, and the fetch that would go and get
        // the manifest is a blocking one on the thread that draws. What a
        // browser build has instead is the chain the browser checked, which
        // is not nothing and is not this: `Trust::Unverified` is the honest
        // name for it, and the grant is whatever the embedding page asked
        // for — which, for a demo, is none. See `has_pins` in `build.rs`
        // and the note in `doc/docs/eui/security.md`.
        #[cfg(not(has_pins))]
        {
            tab.backend.grant(tab.allowed);
            tab.trust = crate::chrome::Trust::Unverified;
        }
        #[cfg(has_pins)]
        match crate::assets::origin_for(&tab.url).map_err(|e| e.to_string()).and_then(|origin| {
            let pins = crate::manifest::pins_dir().ok_or_else(|| "no home directory for the pin store".to_string())?;
            crate::manifest::check(&origin, &pins, tab.cookie.as_deref()).map_err(|e| e.to_string())
        }) {
            Ok(m) => {
                // Spec 01 §2.1: what the person allowed, and nothing is
                // granted by being asked for. `--allow` is one way they
                // say so; the sheet below is the other, and it is the only
                // one a person who did not start this from a terminal has.
                let before = crate::manifest::remembered_grant(&m.app_id);
                tab.allowed |= before.map_or(0, |b| b.granted);
                // What neither the command line nor a previous answer has
                // ever put in front of them. A capability they refused is
                // *answered* and is not asked about again — that is the
                // difference the store keeps two numbers for — but a new
                // version asking for something new is a different question
                // and gets asked.
                let settled = tab.allowed | before.map_or(0, |b| b.asked);
                let unanswered = m.capabilities & !settled;
                // Kept even when nothing is unanswered: the padlock exists
                // for the person who already said yes and wants to look
                // again, which is the common case and the only one the
                // sheet alone cannot serve.
                if m.capabilities & eui_proto::caps::ALL != 0 {
                    tab.perms = Some(Asking { app_id: m.app_id.clone(), asked: m.capabilities & eui_proto::caps::ALL });
                }
                let granted = m.capabilities & tab.allowed;
                let refused = m.capabilities & !tab.allowed;
                eprintln!("eui: {} {} — publisher key pinned; granted [{}], refused [{}]", m.name, m.version, eui_proto::caps::names(granted).join(", "), eui_proto::caps::names(refused).join(", "));
                tab.backend.grant(granted);
                if unanswered != 0 {
                    // The sheet instead of the session: `Hello` carries
                    // the grant, so there is nothing to dial until the
                    // question has an answer.
                    tab.asking = Some(Asking { app_id: m.app_id.clone(), asked: settled | unanswered });
                    tab.backend.ask_consent(unanswered, &m.name);
                    tab.link = Link::Asking;
                }
                // 01 §2.1: the manifest says where the session lives, so an
                // address with no path is not half an address — it is the
                // origin, and the application completes it.
                if let Some(whole) = completed(&tab.url, &m.entry) {
                    eprintln!("eui: {} → {whole} (the manifest's entry)", tab.url);
                    tab.url = whole;
                    tab.title = name_from_url(&tab.url);
                }
                // Installable only with an icon to install it as, which
                // is the one thing an entry cannot do without (01 §2.1).
                //
                // Said on stderr either way. The control is *absent* rather
                // than inert for an application that publishes no icon —
                // which is the right thing to draw and the wrong thing to
                // debug, because a missing button and a broken one look
                // exactly alike. This is the line that tells them apart.
                #[cfg(has_launchers)]
                match m.icon {
                    Some(_) => {
                        // By address, not by `app_id`: one Soli application
                        // serves every component at one origin under one id,
                        // so asking by id put a tick on the music player
                        // because somebody had installed the gallery.
                        let there = crate::install::installed(&tab.url);
                        eprintln!("eui: {} publishes an icon — the address bar offers to {} it", m.app_id, if there { "remove" } else { "install" });
                        tab.installable = Some(there);
                    }
                    None => eprintln!("eui: {} publishes no icon, so it cannot be installed (01 §2.1)", m.app_id),
                }
                if !m.name.is_empty() {
                    tab.title = m.name;
                }
                tab.trust = crate::chrome::Trust::Pinned;
            }
            Err(e) if tab.url.starts_with("ws://") => {
                eprintln!("eui: {e}; continuing on the debug loopback without a manifest");
                // Without a manifest there is nothing to intersect, so what
                // the person named on the command line is the whole of the
                // grant. 08 §1 already lets this session go on without a
                // signature; refusing the capabilities as well would leave
                // a loopback server unable to ask for anything at all.
                tab.backend.grant(tab.allowed);
                tab.trust = crate::chrome::Trust::Local;
            }
            Err(e) => {
                // The tab stays, with no connection in it: in a shell the
                // tab is where a person looks for the reason, and losing
                // it would only leave a gap in the strip.
                //
                // And the reason goes *into* it. This used to say its piece
                // on stderr and leave a blank page, which on a desktop sends
                // somebody to a terminal and on a phone leaves them nothing
                // at all — a window that knew exactly what was wrong and
                // showed an empty rectangle. There is no stderr on a phone.
                eprintln!("eui: {e}; refusing to connect");
                tab.backend.close(refusal(&e));
                return tab;
            }
        }

        if matches!(tab.link, Link::Asking) {
            return tab;
        }
        // The page first, the socket only if there is no page. A component
        // whose first render is the same for everybody costs this server
        // nothing once a cache is in front of it, and costs it a resident
        // session per reader otherwise.
        #[cfg(has_native_net)]
        if tab.try_static(&proxy, w) {
            return tab;
        }
        tab.dial(&proxy);
        tab
    }

    /// The palette the machine is in, told to a driver that has only just
    /// been made.
    ///
    /// Before `dial`, which is the whole point: `Hello` carries the
    /// viewport and the viewport carries the mode, so a server asked after
    /// this renders for the palette the person is actually looking at.
    /// Whatever frames the change produces are dropped — there is no
    /// socket yet for them to go down, and the `Hello` about to be built
    /// says the same thing.
    fn start_in(&mut self, mode: Option<eui_proto::ThemeMode>) {
        let Some(m) = mode else { return };
        let _ = self.backend.input(Input::Mode(m));
    }

    /// The person answered the consent sheet: keep what they said, tell
    /// the driver, and only now open the socket.
    fn consent_answered(&mut self, said: u32, proxy: &Proxy) {
        // Taken either way: the sheet is answered and must not be asked
        // again, whether or not there is a store to remember it in.
        let Some(asking) = self.asking.take() else { return };
        #[cfg(not(has_pins))]
        let _ = &asking;
        // Replaced for what was asked about, not OR-ed into what was there.
        // Now that the buttons say Save, the rows are the answer: one turned
        // off has to come back off, and `|=` made every row one-way — the
        // sheet would appear to work and change nothing. Capabilities the
        // question did not cover are untouched.
        self.allowed = (self.allowed & !asking.asked) | (said & asking.asked);
        // What was put to them and what they said to it — see
        // [`crate::manifest::Answered`] for why both. Nowhere to remember
        // it in a page, which is the other half of `has_pins`.
        #[cfg(has_pins)]
        crate::manifest::remember_grant(&asking.app_id, crate::manifest::Answered { asked: asking.asked, granted: self.allowed & asking.asked });
        eprintln!("eui: the person allowed [{}]", eui_proto::caps::names(self.allowed).join(", "));
        // The driver's mask, before `dial` asks it for a `Hello` carrying
        // it.
        self.backend.grant(self.allowed);
        self.dial(proxy);
    }

    /// Try to have this page over HTTPS instead of over a socket (01 §2.4).
    ///
    /// `true` when it worked, and then nothing is dialled: the tree is drawn,
    /// its pictures are fetched, its `local(...)` handlers run, and the
    /// server is holding nothing whatever for this reader. A socket is opened
    /// later only if something happens that the server has to answer.
    ///
    /// A `404` is the ordinary answer for most components and is not logged.
    /// Anything else says its piece once and falls through to dialling,
    /// because this path must never be the reason a page fails to open.
    #[cfg(has_native_net)]
    fn try_static(&mut self, proxy: &Proxy, width: f32) -> bool {
        // The whole session shape, not just the last segment: a fetch built
        // from `wss://host/blog/site` would otherwise mount whatever
        // component happens to be called `site`.
        let Some(component) = crate::chrome::session_component(&self.url) else {
            return false;
        };
        let view = match crate::transport::fetch_view(&self.url, component, eui_proto::PROTOCOL_VERSION, width.max(1.0) as u32, self.cookie.as_deref()) {
            Ok(view) => view,
            Err(crate::transport::ViewError::NotOffered) => {
                // The ordinary answer for most components, so not a warning —
                // but said somewhere, because a component that is simply not
                // static and one whose name was mistyped look identical from
                // here, and the second is the one somebody is debugging.
                crate::driver::trace(|| format!("{component} is not served as a page; opening a session"));
                return false;
            }
            Err(e) => {
                eprintln!("eui: {component} is not served as a page ({e}); opening a session instead");
                return false;
            }
        };
        let Ok(origin) = crate::assets::origin_for(&self.url) else { return false };

        let p = Arc::clone(proxy);
        let (fetch, inbox) = crate::transport::Fetcher::alone(origin, self.cookie.clone(), move || {
            let _ = p.send_event(Wake::Transport);
        });
        self.fetch = Some(fetch);
        self.inbox = Some(inbox);

        // Fed frame by frame through the ordinary path, so the driver ends
        // up in a state indistinguishable from a socket that welcomed and
        // mounted. Whatever it answers is an `Ack` to nobody.
        for frame in view.frames {
            let _ = self.backend.frame(frame);
        }
        // What the tree is, so that a socket opened later can be told and the
        // server may keep it rather than send it again (01 §2.6). The driver
        // is the only place it is kept: the tab used to hold a copy as well,
        // which nothing ever read.
        self.backend.fetched_tree(view.tree);
        // The address answered, which is what `offline` later means by it.
        self.answered = true;
        self.link = Link::Static;
        crate::driver::trace(|| format!("{component} drawn from a page; no session opened"));
        true
    }

    /// 01 §2.7: the islands this tab's tree asks for, drained and dialled.
    ///
    /// Called from `pump`, after the page's frames have been applied — the
    /// tree that names an island is the tree that just arrived.
    fn pump_islands(&mut self) {
        // What arrived on the ones already open. A batch changes only that
        // island's content; anything else it says is its own business.
        let mut ended: Vec<u16> = Vec::new();
        let mut frames: Vec<(u16, Vec<u8>)> = Vec::new();
        for island in &self.islands {
            while let Ok(msg) = island.conn.rx.try_recv() {
                match msg {
                    Incoming::Message(bytes) => frames.push((island.owner, bytes)),
                    // 01 §2.7: "leaves the page alone". The socket is
                    // forgotten, the node keeps the children the page
                    // rendered, and nothing else is torn down. Not retried
                    // here: an island that fails on every attempt would
                    // otherwise cost the page a dial per pump for as long as
                    // it stays open.
                    Incoming::Closed(e) => {
                        eprintln!("eui: island {} ended: {e}", island.path);
                        ended.push(island.owner);
                        break;
                    }
                    // An island's pictures are the page's: one asset store,
                    // content-addressed, so the same bytes fetched twice are
                    // the same bytes.
                    Incoming::Asset(hash, Ok(bytes)) => self.backend.asset_ready(hash, bytes),
                    Incoming::Asset(hash, Err(why)) => self.backend.asset_failed(hash, why),
                }
            }
        }
        for (owner, bytes) in frames {
            self.backend.island_frame(owner, bytes);
        }
        for owner in ended {
            self.backend.island_ended(owner);
            self.islands.retain(|i| i.owner != owner);
        }

        // What the islands owe their own servers. Never `self.send`: an
        // island's event names a node the page's server never created.
        for (owner, bytes) in self.backend.take_island_outbound() {
            let Some(island) = self.islands.iter().find(|i| i.owner == owner) else { continue };
            if island.conn.tx.send(bytes).is_err() {
                eprintln!("eui: island {} went away", island.path);
                self.backend.island_ended(owner);
                self.islands.retain(|i| i.owner != owner);
            }
        }
    }

    /// Dial the islands this tree asks for that are not open yet.
    ///
    /// Separate from `pump_islands` because it needs the event-loop proxy to
    /// build a waker, and `pump` has none.
    fn dial_islands(&mut self, proxy: &Proxy) {
        let Ok(origin) = crate::assets::origin_for(&self.url) else { return };
        for (node, path) in self.backend.islands_wanted() {
            // One session per distinct path (01 §2.7). A second island
            // naming a path already open is the same island as far as the
            // server is concerned, and opening a second socket for it would
            // cost exactly what the feature exists to save.
            if let Some(shared) = self.islands.iter().find(|i| i.path == path).map(|i| i.owner) {
                let _ = shared;
                continue;
            }
            let Some(owner) = self.backend.open_island(node, &path) else {
                // Past the ceiling, or a path this client will not dial.
                // Either way the node stays as the page rendered it.
                continue;
            };
            // The address is the page's origin and the island's path, and
            // it is built here rather than taken from the tree: §2.7 allows
            // a path and nothing else, so nothing the tree says can decide
            // where this connects.
            let url = format!("{}{path}", origin.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1));
            let p = Arc::clone(proxy);
            match transport::connect(&url, self.backend.hello(), self.cookie.clone(), self.host_loopback, move || {
                let _ = p.send_event(Wake::Transport);
            }) {
                Ok(conn) => self.islands.push(IslandSocket { owner, path, conn }),
                Err(e) => {
                    // 01 §2.7 again: an island that cannot be opened leaves
                    // the page alone, and is simply not open.
                    eprintln!("eui: island {path}: {e}");
                    self.backend.island_ended(owner);
                }
            }
        }
    }

    /// Open a socket for this tab's URL, with whatever the driver says the
    /// opening frame is now — a fresh `Hello` on the first attempt, and one
    /// offering the session back on every attempt after it (spec 01 §4.1).
    fn dial(&mut self, proxy: &Proxy) {
        let hello = self.backend.hello();
        let p = Arc::clone(proxy);
        match transport::connect(&self.url, hello, self.cookie.clone(), self.host_loopback, move || {
            let _ = p.send_event(Wake::Transport);
        }) {
            Ok(c) => {
                self.conn = Some(c);
                // Open is not talking. Until a frame arrives this is still
                // an attempt, and one that stalls is one to make again.
                self.link = Link::Trying { until: crate::time::Instant::now() + std::time::Duration::from_secs(20) };
            }
            // A URL the client refuses is not a network fault: trying it
            // again would refuse it again, in the same words, forever.
            //
            // And the words go on the glass, not only on stderr. This is a
            // window that never showed anything at all — no session, and in
            // a chromeless window no address bar either — so a refusal kept
            // to the log is one the person watching an empty rectangle has
            // no way to reach. `close` is what puts it on the page the
            // driver mounts for a session that ended, where it can be
            // selected and copied.
            Err(e @ transport::TransportError::Insecure(_)) => {
                eprintln!("eui: {e}");
                self.trouble = Some(e.to_string());
                self.backend.close(e.to_string());
                self.link = Link::Ended;
            }
            Err(e) => {
                eprintln!("eui: {e}");
                self.lost();
            }
        }
    }

    /// The GPU went away under every session on this device. Ends this one
    /// the way a refused tree or a dead socket does -- the reason kept on
    /// the blank page rather than only on stderr -- and does not retry:
    /// another socket would draw on the same lost device.
    fn gpu_gone(&mut self, reason: &str) {
        self.trouble = Some(reason.to_owned());
        self.backend.close(reason.to_owned());
        self.conn = None;
        self.link = Link::Ended;
    }

    /// The socket is gone: count the attempt and say when the next is due.
    fn lost(&mut self) {
        self.conn = None;
        self.tries = self.tries.saturating_add(1);
        let wait = backoff(self.tries.saturating_sub(1));
        self.link = Link::Lost { at: crate::time::Instant::now() + wait };
        eprintln!("eui: trying again in {:.1}s", wait.as_secs_f32());
    }

    /// The word last shown for this tab's socket, so the chrome is rebuilt
    /// when it changes and not once a frame.
    fn link_changed(&mut self) -> bool {
        let word = self.link_word();
        if self.shown_link == word {
            return false;
        }
        self.shown_link = word;
        true
    }

    /// The word the address bar puts on the socket, if it needs one.
    fn link_word(&self) -> Option<&'static str> {
        link_word_of(self.link, self.answered)
    }

    /// How the strip should show this tab.
    ///
    /// The component, not the manifest's name: one server serves many, so a
    /// Soli application with a gallery, a feed and a music view answers
    /// `demo-app` for all of them and three tabs of it would carry the
    /// same word three times. The name is the server's; the component is
    /// this session's.
    fn view(&self) -> crate::chrome::TabView<'_> {
        let (origin, path) = split_origin(&self.url);
        let component = crate::chrome::component_of(&self.url);
        let title = if component.is_empty() { self.title.as_str() } else { component };
        // Back is live when the *application* would do something with it —
        // its own pages are its own, and 06 §1.3's `back` is how it is asked
        // to pop one — or, failing that, when the tab has been at another
        // address. Forward is the trail alone: the protocol has no event for
        // it, because a server that keeps a stack was never asked to keep
        // what it popped.
        crate::chrome::TabView {
            grants: self.perms.as_ref().map(|p| p.asked),
            title,
            origin,
            path,
            trust: Some(self.trust),
            link: self.link_word(),
            can_back: self.backend.takes_back() || self.at > 0,
            can_forward: self.at + 1 < self.history.len(),
            // Held rather than asked: this is rebuilt on every keystroke in
            // the address bar, and the answer is a file read.
            installed: self.installable,
        }
    }

    fn send(&mut self, frames: Vec<Vec<u8>>) {
        // No socket, for whatever reason — never dialled, or dialled and
        // lost. Either way a frame the server must answer is held rather
        // than dropped: a reader whose click disappeared because the socket
        // happened to be down between two attempts has no way to know that,
        // and no reason to suspect it.
        if self.conn.is_none() && !matches!(self.link, Link::Ended) {
            let wanted: Vec<Vec<u8>> = frames.into_iter().filter(|f| f.first().copied().is_some_and(crate::transport::kind_needs_server)).collect();
            if wanted.is_empty() {
                // A `local(...)` handler ran and changed the tree in place,
                // or the window was resized. Nothing to tell anybody.
                return;
            }
            self.queued.extend(wanted);
            self.want_socket = true;
            return;
        }
        let Some(conn) = &self.conn else { return };
        for f in frames {
            if conn.tx.send(f).is_err() {
                eprintln!("eui: connection gone");
                self.conn = None;
                return;
            }
        }
    }

    /// Everything waiting on this tab's connection. `true` if it wants the
    /// glass redrawn — which it only gets while it is the active tab.
    fn pump(&mut self) -> bool {
        let mut frames = Vec::new();
        let mut closed = None;
        // Either the socket's channel or, with no socket, the tab's own —
        // a page fetched over HTTPS still asks for its pictures.
        let rx = self.conn.as_ref().map(|c| &c.rx).or(self.inbox.as_ref());
        if let Some(rx) = rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    // Decoded by the driver, wherever it runs: the window
                    // never reads a frame.
                    Incoming::Message(bytes) => {
                        self.answered = true;
                        // The socket spoke: this attempt worked, and the
                        // next break starts its own backoff from the top.
                        self.link = Link::Up;
                        self.tries = 0;
                        frames.push(bytes);
                    }
                    Incoming::Closed(e) => {
                        closed = Some(e);
                        break;
                    }
                    Incoming::Asset(hash, Ok(bytes)) => self.backend.asset_ready(hash, bytes),
                    Incoming::Asset(hash, Err(why)) => self.backend.asset_failed(hash, why),
                }
            }
        }
        for f in frames {
            let out = self.backend.frame(f);
            self.send(out);
        }
        for hash in self.backend.pending_assets() {
            if let Some(fetch) = &self.fetch {
                fetch.request_asset(hash);
            }
        }
        // The socket has spoken, so what the page queued can go. It is sent
        // *after* the first frame rather than on connect, because until the
        // server has answered there is no telling whether it kept the tree
        // this reader is looking at or mounted a fresh one.
        //
        // The honest caveat, until a session can adopt a fetched tree
        // (01 §2.6): a fresh mount is the same render of the same state, so
        // its node ids are the same ids and the held event lands where it
        // was aimed — but that is a property of the encoder being
        // deterministic rather than a promise the protocol makes yet.
        if matches!(self.link, Link::Up) && !self.queued.is_empty() {
            let held = std::mem::take(&mut self.queued);
            self.send(held);
        }
        self.pump_islands();
        match closed {
            // An HTTP status is the server answering, not the network
            // failing: this address is not a session and will not become
            // one, so trying again would ask the same question and be told
            // the same thing, on a ladder, for ever. The reason goes on the
            // glass, where a window that never drew anything can show it.
            Some(e @ transport::TransportError::Refused(..)) => {
                eprintln!("eui: {e}");
                self.trouble = Some(e.to_string());
                self.backend.close(e.to_string());
                self.conn = None;
                self.link = Link::Ended;
            }
            // Spec 01 §4.1. The session is the server's; only the socket
            // broke. Say so, and go and get it back.
            Some(e) => {
                eprintln!("eui: the connection went away: {e}");
                self.lost();
            }
            None => {}
        }
        if let Some(c) = self.backend.closed() {
            // The session itself ended — a version, a refused tree, an
            // `Error` from the server. Another socket would end the same
            // way, so this one is not tried again.
            eprintln!("eui: closing: {c}");
            // Kept, not only logged: a reason that lives on stderr alone is
            // a reason nobody reading the window will ever see.
            self.trouble = Some(c.to_string());
            self.conn = None;
            self.link = Link::Ended;
        }
        self.backend.needs_redraw()
    }

    /// Spec 03 §7: the device is open exactly while the tab has a sound
    /// loaded — nothing playing, nothing running, no wakeups. A tab keeps
    /// its sound when it goes to the back, as a browser tab does.
    #[cfg(has_audio)]
    fn sync_audio(&mut self, proxy: &Proxy) {
        let wanted = self.backend.audio_playing();
        match (wanted, self.audio.is_some()) {
            (true, false) => {
                let (tx, rx) = mpsc::channel();
                let proxy = Arc::clone(proxy);
                match crate::audio::Output::start(self.backend.audio_tap(), tx, move || {
                    let _ = proxy.send_event(Wake::Audio);
                }) {
                    Ok(out) => {
                        eprintln!("eui: audio out {} Hz, {} channel(s)", out.rate(), out.channels());
                        self.audio = Some(out);
                        self.audio_rx = Some(rx);
                    }
                    Err(e) => eprintln!("eui: no audio output: {e}"),
                }
            }
            (false, true) => {
                self.audio = None;
                self.audio_rx = None;
            }
            _ => {}
        }
    }

    /// No device to open, so nothing to keep in step with one. The driver
    /// goes on mixing — that is 03 §7's arithmetic and it is portable — and
    /// a page simply never asks for the samples.
    #[cfg(not(has_audio))]
    fn sync_audio(&mut self, _proxy: &Proxy) {}

    /// What the audio thread produced since the last look: a sound's end.
    #[cfg(has_audio)]
    fn drain_audio(&mut self) {
        let mut frames = Vec::new();
        if let Some(rx) = &self.audio_rx {
            while let Ok(f) = rx.try_recv() {
                frames.push(f);
            }
        }
        if !frames.is_empty() {
            self.send(frames);
        }
    }

    /// No thread, so nothing it produced.
    #[cfg(not(has_audio))]
    fn drain_audio(&mut self) {}

    /// Let this tab go, worker and all.
    ///
    /// Dropping it is what ends the worker: the pipe closes, and the
    /// confined process on the other side of it exits. Everything else the
    /// tab owns — the socket, the sound, the textures — goes with it, and
    /// none of it was ever shared with another tab.
    fn close(self, why: &str) {
        crate::driver::trace(|| format!("tab closed: {why}"));
        drop(self);
    }
}

/// What to put on the glass when a session is refused: the reason, and
/// where it is worth saying, the way out of it.
///
/// A certificate the client does not know is the one refusal a person is
/// likely to hit on purpose — a private CA, a `mkcert` name, a staging box
/// — and it is the one where the answer is a single environment variable
/// they cannot be expected to guess. The platform trust store carries it on
/// a desktop; on a phone there is no such store to read, so the name of the
/// flag has to be in the message.
#[cfg_attr(not(has_pins), allow(dead_code))]
fn refusal(why: &str) -> String {
    let cert = why.contains("certificate") || why.contains("UnknownIssuer") || why.contains("CaUsedAsEndEntity");
    if cert {
        format!("{why}\n\nThe certificate is signed by an authority this client does not know. EUI_CA_FILE=<pem> adds one — a mkcert root is at `$(mkcert -CAROOT)/rootCA.pem`. A phone has no platform trust store to read, so it must be named.")
    } else {
        why.to_owned()
    }
}

/// The window's own icon: the 64 px raster of `assets/icon/eui.svg`, which
/// `scripts/make-icons.py` writes beside it. X11 puts it in the title bar
/// and the task switcher and Windows in the taskbar; Wayland has no such
/// call and matches a `.desktop` file by app id instead (`deploy/eui.desktop`),
/// and macOS reads the bundle's `.icns`. Three kilobytes, decoded once.
/// Which palette the machine is in, before a driver exists to ask.
///
/// winit answers this on macOS, on Windows and in a page, and on no other
/// platform: its Wayland answer is the decoration theme *this* process
/// asked for — `None` until it asks — and its X11, Android and iOS ones
/// are flatly `None`. So the desktop is asked first where there is one to
/// ask, and the window only where winit has a real answer.
fn platform_mode(window: &Window) -> Option<eui_proto::ThemeMode> {
    #[cfg(has_desktop_theme)]
    if let Some(m) = crate::desktop_theme::os_mode() {
        return Some(m);
    }
    window.theme().map(|t| match t {
        winit::window::Theme::Dark => eui_proto::ThemeMode::Dark,
        winit::window::Theme::Light => eui_proto::ThemeMode::Light,
    })
}

fn window_icon() -> Option<winit::window::Icon> {
    let image = crate::assets::decode_png(include_bytes!("../../../assets/icon/png/eui-64.png")).ok()?;
    winit::window::Icon::from_rgba(image.rgba, image.width, image.height).ok()
}

impl Shell {
    /// Open a window, and the applications named in `launches`.
    ///
    /// With `chrome`, the window gets a tab strip and can be given more
    /// applications later; without it, it is the one chromeless window
    /// `eui <url>` and an embedding host have always had.
    ///
    /// `allowed` is what the person granted on the command line, kept for
    /// every tab this window will open rather than only the first.
    fn open(launches: Vec<Launch>, chrome: bool, allowed: u32, event_loop: &ActiveEventLoop, proxy: Proxy, shared: &mut Option<Shared>) -> Option<Self> {
        // The build is in the title because a window cannot otherwise be
        // told from one built an hour earlier, and a demo downloaded from
        // the wrong run looks exactly like the right one.
        let title = format!("{} — {}", launches.first().map_or("EUI", |l| l.title.as_str()), crate::BUILD);
        // Born hidden, shown once the renderer exists. Two reasons: the
        // AccessKit adapter must exist before the window is first shown, and
        // macOS enforces that with a panic where AT-SPI merely tolerates it;
        // and a window shown before its first frame is a flash of nothing.
        let attrs = Window::default_attributes().with_title(title).with_visible(false);
        // A desktop window opens at a readable size and is moved from
        // there. A phone has exactly one window, already the size of the
        // screen, and asking for 960x640 there is either ignored or —
        // worse — honoured.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(960.0, 640.0));
        let attrs = match window_icon() {
            Some(icon) => attrs.with_window_icon(Some(icon)),
            None => attrs,
        };
        // The Wayland app id, so a compositor can match rules and a taskbar
        // an icon; on X11 the same two strings are the WM_CLASS.
        #[cfg(target_os = "linux")]
        let attrs = {
            use winit::platform::wayland::WindowAttributesExtWayland;
            use winit::platform::x11::WindowAttributesExtX11;
            WindowAttributesExtWayland::with_name(attrs, "eui", "eui").pipe(|a| WindowAttributesExtX11::with_name(a, "eui", "eui"))
        };
        // None of what was just built means anything to a canvas: it has
        // no title bar to put a build number in, no icon, no app id for a
        // taskbar, and a size the page's stylesheet decides. It is built
        // anyway rather than threaded behind another `cfg`, because the
        // chain above is four platforms deep already and a fifth arm in
        // each is a worse trade than one discard with a reason.
        #[cfg(target_arch = "wasm32")]
        let _ = (&event_loop, attrs);

        // A page's window was made before the renderer was, because the
        // adapter had to be asked about its surface (see `Shared::made`).
        // So it is taken, not made — and there is exactly one of it: a
        // second window in a tab is a window the page never gave us.
        #[cfg(target_arch = "wasm32")]
        let (window, premade) = match shared.as_mut().and_then(|g| g.made.take()) {
            Some(pair) => pair,
            None => {
                eprintln!("eui: a page has one canvas, and it is already in use");
                return None;
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("eui: cannot create a window: {e}");
                return None;
            }
        };

        // Assistive technologies register before the window shows; the tree
        // itself is built only if one asks.
        //
        // `EUI_A11Y=0` leaves the adapter out of this window, the way
        // `EUI_SANDBOX=0` leaves out the worker. Building without the
        // feature does the same thing permanently; this is for finding out
        // whether the platform's accessibility is behind a cost, on a
        // binary somebody already has, without asking them to build one.
        #[cfg(has_a11y)]
        let access = (!std::env::var("EUI_A11Y").is_ok_and(|v| v == "0")).then(|| accesskit_winit::Adapter::with_event_loop_proxy(event_loop, &window, EventLoopProxy::clone(&proxy)));

        // Vulkan, Metal or DX12 — never GL on a desktop: on Linux a GL
        // instance loads Mesa's gallium and its LLVM (34 MB of the window's
        // 64 MB PSS, measured), for a backend the primary ones make
        // unneeded.
        //
        // Android is the exception, and it is not a small one: there is no
        // Mesa and no LLVM to load there — GLES *is* the system driver — and
        // an emulator very often has no working Vulkan at all. Without GL in
        // the set `request_adapter` answers `None`, and the only thing this
        // function can then do is refuse to open, which ends the event loop
        // and closes the activity. That reads as an application that does not
        // start, and the whole of its report is one line in `logcat`.
        //
        // Once per process, along with the adapter and the device: the
        // second window's surface comes from the same instance and draws
        // through the same renderer.
        let make_surface = |instance: &wgpu::Instance| match instance.create_surface(Arc::clone(&window)) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("eui: cannot create a surface: {e}");
                None
            }
        };
        #[cfg(target_arch = "wasm32")]
        let surface = {
            // Made by the probe, from the same instance now in `shared`.
            let _ = &make_surface;
            premade
        };
        #[cfg(not(target_arch = "wasm32"))]
        let surface = match shared.as_ref() {
            Some(g) => make_surface(&g.instance)?,
            None => {
                let backends = if cfg!(target_os = "android") { wgpu::Backends::PRIMARY | wgpu::Backends::GL } else { wgpu::Backends::PRIMARY };
                let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends, ..Default::default() });
                let surface = make_surface(&instance)?;
                // The adapter is chosen for the first window's surface, so
                // it is asked to be compatible with it.
                let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                }));
                let Some(adapter) = adapter else {
                    eprintln!("eui: no GPU adapter");
                    return None;
                };
                let renderer = match eui_render::Renderer::with_adapter(&adapter) {
                    Ok(r) => r,
                    Err(e) => {
                        eprintln!("eui: {e}");
                        return None;
                    }
                };
                *shared = Some(Shared { instance, adapter, renderer });
                surface
            }
        };
        let gpu_shared = shared.as_mut()?;
        // A later window is not asked about: it has to live on the adapter
        // the first one settled. On one GPU that is always true; on a
        // laptop with two it need not be, and configuring a surface the
        // adapter does not support is a validation abort inside a callback
        // that cannot unwind — so it is checked, not risked.
        if !gpu_shared.adapter.is_surface_supported(&surface) {
            eprintln!("eui: this window's surface is not supported by the adapter the first one chose; refusing to open it");
            return None;
        }
        #[cfg(not(target_arch = "wasm32"))]
        let size = window.inner_size();
        // A page's window reports the canvas's *backing store*, and
        // `with_inner_size` only asks for one — on a slow load the ask has
        // not landed by the time this reads it, so the surface is configured
        // at 1x1 and stays there. Nothing corrects it later either: winit
        // resizes from a `ResizeObserver` on the CSS box, and the CSS box
        // never changed. The session then connects, decodes and lays out
        // perfectly into one pixel.
        //
        // So the page's own box is asked again here, and asserted rather
        // than requested. Measured against production, where the race is
        // lost reliably and is won every time locally.
        #[cfg(target_arch = "wasm32")]
        let size = {
            use winit::platform::web::WindowExtWebSys;
            let asked = window.inner_size();
            let measured = window.canvas().map(|canvas| {
                let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio()).clamp(1.0, 2.0);
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                winit::dpi::PhysicalSize::new((f64::from(canvas.client_width().max(1)) * dpr).round() as u32, (f64::from(canvas.client_height().max(1)) * dpr).round() as u32)
            });
            match measured {
                Some(m) if m.width > 1 && m.height > 1 && (asked.width <= 1 || asked.height <= 1) => {
                    eprintln!("eui: the window opened at {}x{}; the canvas says {}x{} and the canvas is right", asked.width, asked.height, m.width, m.height);
                    let _ = window.request_inner_size(m);
                    m
                }
                _ => asked,
            }
        };
        let caps = surface.get_capabilities(&gpu_shared.adapter);
        // A surface only takes a format it advertises, and configuring it with
        // any other is a validation error inside wgpu — which aborts, because
        // it happens in a callback that cannot unwind. Metal advertises BGRA
        // and the float formats and no RGBA8 at all, so the off-screen
        // `FORMAT` is a request the Mac cannot serve.
        //
        // The shader writes linear values and leaves the conversion to the
        // target, so the choice has to stay sRGB; only the channel order gives.
        let format = if caps.formats.contains(&eui_render::FORMAT) {
            eui_render::FORMAT
        } else if let Some(srgb) = caps.formats.iter().copied().find(wgpu::TextureFormat::is_srgb) {
            srgb
        } else {
            // No sRGB *surface* format. This is the normal case for a WebGPU
            // canvas, which is `bgra8unorm` and nothing else — so it is what
            // a browser on a Mac gets, while the same page on a machine with
            // no WebGPU falls back to WebGL2, is handed an sRGB surface, and
            // looks right. That is the whole of "too black, but only in the
            // browser, and only on the Mac".
            //
            // Writing linear values into a target that does not encode them
            // darkens everything and crushes the bottom of the scale: 0.0137
            // is meant to leave as #1f1f1f and leaves as #040404 instead, so
            // `surface.base` and the card on top of it become the same
            // black. (The note that used to be here said the colours would
            // look *light*. It had the direction backwards, which is its own
            // small lesson about untested diagnostics.)
            //
            // A view format fixes it without touching the shader: the
            // surface stays `bgra8unorm`, the *view* is its sRGB sibling,
            // and the hardware encodes on write exactly as it would for an
            // sRGB surface. Both WebGPU and Metal allow that pairing.
            let first = caps.formats.first().copied().unwrap_or(eui_render::FORMAT);
            let srgb = first.add_srgb_suffix();
            if srgb == first {
                eprintln!("eui: no sRGB surface format and none to view {first:?} as — the dark end will crush");
            }
            first
        };
        // Empty when `format` is already sRGB: asking to view an sRGB format
        // as itself is a validation error, not a no-op.
        let view_formats = if format.is_srgb() { Vec::new() } else { vec![format.add_srgb_suffix()] };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats,
            desired_maximum_frame_latency: 2,
        };
        surface.configure(gpu_shared.renderer.device(), &config);
        let scale = window.scale_factor() as f32;

        let textures = gpu_shared.renderer.session();
        let (logical_w, logical_h) = (size.width as f32 / scale, size.height as f32 / scale);

        // Which of the two palettes this machine is in — asked here, where
        // the window exists and no driver does yet.
        //
        // It used to be asked after the tabs were open, and a driver is
        // born light: the first `Hello` said light whatever the desktop
        // was, and the correction that followed only ever reached the tab
        // that was active at the time. Every tab opened afterwards — a
        // typed address, a link, a reload, anything through `go_to` — got
        // a fresh driver in the light and nothing to tell it otherwise.
        //
        // Linux hid that too. There the Omarchy palette is handed to each
        // new tab by `theme_one`, and it carries a mode with it, so the
        // one desktop this was developed on corrected itself. macOS and
        // Windows have no such palette: the shell came up dark, the person
        // opened an application, and it was light from then on.
        let mode = platform_mode(&window);
        match mode {
            Some(m) => eprintln!("eui: the platform is in the {}", if m == eui_proto::ThemeMode::Dark { "dark" } else { "light" }),
            None => eprintln!("eui: the platform does not say which palette it is in; light unless an application asks for another"),
        }

        let mut chrome = chrome.then(|| {
            let mut c = crate::chrome::Chrome::new(logical_w, logical_h, scale);
            if let Some(m) = mode {
                c.set_mode(m);
            }
            c.set_recents(crate::recent::load());
            (c, textures)
        });

        let mut shell = Self {
            #[cfg(target_os = "linux")]
            dnd: None,
            #[cfg(target_os = "linux")]
            keys: None,
            window,
            surface: Some(surface),
            config,
            gpu_gone: false,
            allowed,
            tabs: Vec::new(),
            active: 0,
            modifiers: 0,
            held: 0,
            pending_resize: None,
            pointer_at: None,
            hovering: false,
            hover_at: None,
            pointer_in_app: false,
            proxy,
            #[cfg(has_a11y)]
            access,
            #[cfg(has_clipboard)]
            clip: None,
            cursor: eui_proto::Cursor::Default,
            ime_area: None,
            covered: 0.0,
            chrome_mode: mode,
            platform_mode: mode,
            locating: false,
            theme_watch: None,
            #[cfg(has_desktop_theme)]
            desktop_theme: None,
            theme_pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            epoch: crate::time::Instant::now(),
            frames: 0,
            #[cfg(target_arch = "wasm32")]
            announced: false,
            files_dirty: true,
            chrome: chrome.take(),
        };

        let renderer = &gpu_shared.renderer;
        let (w, h) = shell.content_size();
        for l in launches {
            let tab = Tab::open(l, Arc::clone(&shell.proxy), renderer, w, h, scale, mode);
            shell.tabs.push(tab);
        }
        shell.rebuild_chrome();

        // The desktop's own colours, before the first frame; and again
        // whenever the desktop changes them.
        //
        // After the tabs, not before: `follow_desktop_theme` hands the
        // palette to the applications that are open, and it remembers what
        // it last read, so running it against an empty window read the
        // theme, told nobody, and made every later call a no-op. The
        // window then came up in the default palette and stayed there.
        #[cfg(has_desktop_theme)]
        if !crate::desktop_theme::disabled() {
            shell.follow_desktop_theme();
            // One wake per burst of changes: a switch touches several files
            // and the window re-reads the theme once, when it gets to it.
            let proxy = Arc::clone(&shell.proxy);
            let pending = Arc::clone(&shell.theme_pending);
            shell.theme_watch = crate::desktop_theme::watch(move || {
                if !pending.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    let _ = proxy.send_event(Wake::Theme);
                }
            });
        }

        // The one thing winit will not report on this platform: a file
        // over the window (03 §3.2). Started last, where the window
        // certainly exists and nothing after it can fail, and stopped in
        // `close`. On X11 this is `None` and winit's own path runs.
        #[cfg(target_os = "linux")]
        {
            shell.dnd = crate::wayland::watch(&shell.window, &shell.proxy);
            shell.keys = crate::wayland::keys(&shell.window);
        }

        // Everything the first frame needs is in place, and any assistive
        // technology has already registered: it is safe to be seen.
        //
        // Ask for that first frame explicitly. A window that was visible at
        // creation is told to redraw as it maps; one shown later is not, and
        // on Wayland it simply maps blank and stays blank until some
        // unrelated event happens to ask for a frame.
        shell.window.set_visible(true);
        shell.window.request_redraw();
        Some(shell)
    }

    /// Device pixels per logical pixel, as the display asks for it. The
    /// chrome is drawn at this and nothing else: a tab strip that grew with
    /// the page would be a browser whose toolbar zooms, which none do.
    fn scale(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    /// Whether what is held is the window's own modifier *and* whatever else
    /// this platform asks for — see `SHELL_NEEDS_SHIFT`.
    fn shell_chord(&self) -> bool {
        self.modifiers & SHELL_MOD != 0 && (!SHELL_NEEDS_SHIFT || self.modifiers & 0b0001 != 0)
    }

    /// The active tab's zoom, or 1.0 when there is no tab to ask.
    fn zoom(&self) -> f32 {
        self.tabs.get(self.active).map_or(1.0, |t| t.zoom)
    }

    /// Device pixels per logical pixel *as the application is told it*.
    ///
    /// This is the whole of the zoom. Everything downstream — the layout,
    /// the glyph raster, the paint cache key, the `Viewport` the server is
    /// sent — already turns on the scale it is handed, so a page drawn half
    /// again as large is a page told the display is half again as dense.
    fn app_scale(&self) -> f32 {
        self.scale() * self.zoom()
    }

    /// A point in the window's logical px, as the page under the chrome
    /// sees it: the strip's height off the top, then the zoom out of both
    /// axes. The inverse of what `paint.rs` does with the same factor.
    fn to_app(x: f32, y: f32, top: f32, zoom: f32) -> (f32, f32) {
        (x / zoom, (y - top) / zoom)
    }

    /// Where the pointer is **now**, in the window's logical pixels.
    ///
    /// Asked of the platform rather than remembered, because the one
    /// moment this is wanted is the one moment the remembered answer is
    /// wrong: no backend delivers `CursorMoved` while a drag is in flight,
    /// so `pointer_at` is from before the gesture began — usually
    /// somewhere else on the page, and `None` if the pointer had not been
    /// over the window at all since it opened. That was a file let go on
    /// the drop zone landing whereever the mouse had last rested, or being
    /// discarded without a word.
    ///
    /// Falls back to `pointer_at` where there is nobody to ask: X11, whose
    /// `XdndPosition` winit drops on the floor, and Wayland, which never
    /// reaches here.
    fn pointer_from_platform(&self) -> Option<(f32, f32)> {
        self.platform_pointer().or(self.pointer_at)
    }

    /// The platform's own answer, with no fallback: `None` means there was
    /// nobody to ask, not that the pointer is nowhere.
    ///
    /// The distinction is what decides whether the hover poll is worth
    /// arming. On X11 the answer never comes, so polling for it would be
    /// a wake-up every 20 ms to compare a stale value with itself.
    fn platform_pointer(&self) -> Option<(f32, f32)> {
        let scale = self.scale();
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        crate::cursor::position(&self.window).map(|(x, y)| (x / scale, y / scale))
    }

    /// The same point for something arriving over the window from outside
    /// it — a file being dragged — or `None` when it is over the chrome
    /// strip and so not the page's at all. A file let go on the address bar
    /// is not for the page.
    ///
    /// Shared by the two paths that can put a file over a window, which is
    /// why it is a function and not an expression written twice: winit's
    /// `HoveredFile`, which carries no position and has to use the last
    /// pointer, and Wayland's `wl_data_device.enter`, which carries a real
    /// one because winit does not report that event at all (`wayland.rs`).
    fn page_point(x: f32, y: f32, top: f32, zoom: f32) -> Option<(f32, f32)> {
        (y >= top).then(|| Self::to_app(x, y, top, zoom))
    }

    /// Draw the active page at `z`, and tell it so.
    ///
    /// Nothing else is needed: the driver compares the scale it is handed
    /// with the one it had, and on a change throws away the layout and the
    /// paint cache and re-rasterises every glyph — which is exactly the
    /// work a zoom asks for, and was already there for a window dragged
    /// between two monitors of different densities.
    ///
    /// Not through `rebuild_chrome`, which would have been the short way:
    /// a rebuild resets the address bar's edit state, so zooming while
    /// typing an address would have eaten what was typed.
    fn set_zoom(&mut self, z: f32) -> bool {
        let at = self.active;
        let Some(t) = self.tabs.get_mut(at) else { return true };
        if (t.zoom - z).abs() < f32::EPSILON {
            return true;
        }
        t.zoom = z;
        let (w, h) = self.content_size();
        self.send_to_tab(Input::Resized(w, h, self.app_scale()));
        // The pointer the driver holds is in the *old* page's units, and
        // it is what decides which scroller a wheel turns and what stays
        // lit. Without this, the first scroll after a zoom step goes to
        // whatever used to be under the cursor.
        if let (true, Some((x, y))) = (self.pointer_in_app, self.pointer_at) {
            let top = self.chrome.as_ref().map_or(0.0, |(c, _)| c.content_top());
            let (x, y) = Self::to_app(x, y, top, z);
            self.send_to_tab(Input::PointerMove(x, y));
        }
        crate::driver::trace(|| format!("zoom: tab {at} at {:.0} %", z * 100.0));
        self.window.request_redraw();
        true
    }

    /// The size an application's viewport gets, in *its* logical px: the
    /// window, less whatever the chrome is holding above it.
    ///
    /// Taken from the device pixels rather than from the window's logical
    /// ones, because with a zoom on they are no longer the same unit: the
    /// strip above is measured in the window's, the page below in its own.
    fn content_size(&self) -> (f32, f32) {
        let s = self.app_scale();
        let top = self.content_origin() as f32;
        let w = self.config.width as f32 / s;
        let h = ((self.config.height as f32 - top) / s).max(1.0);
        (w, h)
    }

    /// Where an application's list starts in the window, in device pixels.
    fn content_origin(&self) -> u32 {
        let scale = self.scale();
        let top = self.chrome.as_ref().map_or(0.0, |(c, _)| c.content_top());
        if top.is_finite() {
            (top * scale) as u32
        } else {
            self.config.height
        }
    }

    /// True when the chrome, not an application, owns the area below the
    /// strip — an empty tab.
    fn showing_blank(&self) -> bool {
        self.chrome.as_ref().is_some_and(|(c, _)| c.is_blank())
    }

    /// Tell the chrome what the tabs look like now, and tell the newly
    /// active application how much room it has.
    fn rebuild_chrome(&mut self) {
        let Some((chrome, _)) = &mut self.chrome else { return };
        let views: Vec<_> = self.tabs.iter().map(Tab::view).collect();
        // An empty shell still shows one tab, so there is something to
        // click and something to type into.
        if views.is_empty() {
            let blank = crate::chrome::TabView { title: "New tab", origin: "", path: "", trust: None, link: None, grants: None, can_back: false, can_forward: false, installed: None };
            chrome.set_trouble(None);
            chrome.rebuild(&[blank], 0);
        } else {
            // Why the active tab has nothing in it, if that is a fault
            // rather than a tab nobody has used yet.
            chrome.set_trouble(self.tabs.get(self.active).and_then(|t| t.trouble.clone()));
            chrome.rebuild(&views, self.active);
        }
        let (w, h) = self.content_size();
        // The tab that just became active, at *its* zoom: this is what
        // gives every tab its own level back as the strip is clicked.
        let scale = self.app_scale();
        if let Some(t) = self.tabs.get_mut(self.active) {
            let out = t.backend.input(Input::Resized(w, h, scale));
            t.send(out);
        }
        self.window.request_redraw();
    }

    /// Open `url` in the active tab, or in a new one if there is none.
    fn open_url(&mut self, url: String, renderer: &eui_render::Renderer) {
        self.go_to(url, renderer, Trail::Push);
    }

    /// Open `url` in the active tab, and what that does to the tab's trail.
    fn go_to(&mut self, url: String, renderer: &eui_render::Renderer, trail: Trail) {
        let (w, h) = self.content_size();
        // The zoom stays with the tab, not with the session in it: a reload
        // — which comes through here — would otherwise throw it away, and
        // so would typing the same address again.
        let zoom = self.zoom();
        let scale = self.app_scale();
        // Likewise the trail. `Tab::open` makes a new tab and the old one is
        // dropped below, so anything that belongs to the tab rather than to
        // the session in it has to be carried over by hand.
        let (mut history, mut at) = self.tabs.get(self.active).map_or_else(|| (Vec::new(), 0), |t| (t.history.clone(), t.at));
        match trail {
            Trail::Push => {
                // Opening from the middle drops what was ahead: the forward
                // half of a trail is a guess about where somebody was going,
                // and going somewhere else is the answer to it.
                if !history.is_empty() {
                    history.truncate(at.saturating_add(1));
                }
                // The same address twice running is a reload, not a step.
                if history.last().map(String::as_str) != Some(url.as_str()) {
                    history.push(url.clone());
                }
                at = history.len().saturating_sub(1);
            }
            Trail::Stay => {}
            Trail::At(n) => at = n,
        }
        // The window's grant, not nothing. This line used to read
        // `Launch::new(url, 0)`, and since `go_to` is the one funnel every
        // navigation goes through — a typed address, a link, a reload, a
        // step back — that made `--allow` last exactly one page and made
        // the shell, which reaches every application through here, unable
        // to be granted anything at all. A dialog that never opens and a
        // dropped file that never arrives were both this.
        let launch = Launch::new(url, self.allowed);
        let mut tab = Tab::open(launch, Arc::clone(&self.proxy), renderer, w, h, scale, self.platform_mode);
        tab.zoom = zoom;
        tab.history = history;
        tab.at = at;
        if let Some(slot) = self.tabs.get_mut(self.active) {
            let old = std::mem::replace(slot, tab);
            old.close("replaced");
        } else {
            self.tabs.push(tab);
            self.active = self.tabs.len().saturating_sub(1);
        }
        let at = self.active;
        self.theme_one(at);
        self.rebuild_chrome();
    }

    /// A step back or forward through the active tab's trail. Nothing at
    /// either end: the buttons are drawn dim there and this is the guard
    /// behind them.
    fn step(&mut self, back: bool, renderer: &eui_render::Renderer) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let Some(to) = (if back { t.at.checked_sub(1) } else { (t.at + 1 < t.history.len()).then(|| t.at + 1) }) else {
            return;
        };
        let Some(url) = t.history.get(to).cloned() else { return };
        if let Some((c, _)) = &mut self.chrome {
            c.leave_address();
        }
        self.go_to(url, renderer, Trail::At(to));
    }

    /// Close tab `n`. The last one takes the window with it.
    fn close_tab(&mut self, n: usize) -> bool {
        if n >= self.tabs.len() {
            return true;
        }
        self.tabs.remove(n).close("closed");
        if self.tabs.is_empty() {
            return false;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.rebuild_chrome();
        true
    }

    /// Act on what the chrome said a click meant.
    fn chrome_action(&mut self, a: crate::chrome::Action, renderer: &eui_render::Renderer) -> bool {
        use crate::chrome::Action as A;
        match a {
            A::Select(n) if n < self.tabs.len() => {
                self.active = n;
                self.rebuild_chrome();
            }
            A::Select(_) => {}
            A::Close(n) => return self.close_tab(n),
            A::NewTab => {
                // An empty tab has no session and no worker: the chrome
                // draws its page itself, so it costs a node, not a process.
                self.active = self.tabs.len();
                self.rebuild_chrome();
            }
            A::EditAddress => {
                if let Some((c, _)) = &mut self.chrome {
                    c.edit_address();
                }
                self.rebuild_chrome();
            }
            // The sheet again, for an application that has already been
            // answered. The grant rides in `Hello` (01 §2.1), so there is no
            // way to change it on a live session: the socket is dropped here
            // and `consent_answered` dials a new one with the new answer.
            // That is honest rather than convenient — a capability taken
            // away has to stop being true, and a session that kept running
            // would still be holding it.
            // Into the desktop's launcher, or out of it.
            //
            // On a thread, because installing is two HTTPS round trips —
            // the manifest and the icon — and a window that stops painting
            // while a button is pressed is the thing this client is most
            // careful not to be. The record is the only shared state and
            // the wake is what says it moved.
            #[cfg(has_launchers)]
            A::Install => self.launcher_entry(true),
            #[cfg(has_launchers)]
            A::Uninstall => self.launcher_entry(false),
            #[cfg(not(has_launchers))]
            A::Install | A::Uninstall => {}
            A::Permissions => {
                if let Some(t) = self.tabs.get_mut(self.active) {
                    if let Some(p) = t.perms.clone() {
                        // Nothing is cleared here any more: `consent_answered`
                        // replaces the asked-about bits with the answer, so
                        // the sheet is free to come up showing what is
                        // actually granted — and Close can leave it alone.
                        t.conn = None;
                        t.link = Link::Asking;
                        let name = t.title.clone();
                        t.asking = Some(p.clone());
                        t.backend.ask_consent(p.asked, &name);
                    }
                }
                self.rebuild_chrome();
                self.window.request_redraw();
            }
            A::LeaveAddress => {
                if let Some((c, _)) = &mut self.chrome {
                    c.leave_address();
                }
                self.rebuild_chrome();
            }
            A::Reload => {
                // The same address from nothing, standing where it stands: a
                // reload is not a step, and a trail that grew an entry every
                // time a server was restarted would be a back button that
                // goes nowhere.
                if let Some(url) = self.tabs.get(self.active).map(|t| t.url.clone()) {
                    self.go_to(url, renderer, Trail::Stay);
                }
            }
            A::Back => {
                // The page first, the address second. Clicking a menu item
                // never changed the address, so nothing about it is in the
                // tab's trail — an application's pages are the application's,
                // and a server that draws them keeps the stack (06 §1.3).
                // The arrow asks it to pop one, the same event Android's
                // back button, `Alt+Left`, the mouse's fourth button and a
                // swipe from the leading edge already arrive as. Only a page
                // with nowhere left to go leaves the arrow meaning the trail.
                if !self.back() {
                    self.step(true, renderer);
                }
            }
            A::Forward => self.step(false, renderer),
            A::Open(url) => {
                if let Some((c, _)) = &mut self.chrome {
                    c.leave_address();
                }
                self.open_url(url, renderer);
            }
            A::Forget(url) => {
                let list = crate::recent::forget(&url);
                if let Some((c, _)) = &mut self.chrome {
                    c.set_recents(list);
                }
                self.rebuild_chrome();
            }
            A::Rebuild => self.rebuild_chrome(),
        }
        true
    }

    /// Hand the palette the window is already following to one tab.
    ///
    /// [`Self::follow_desktop_theme`] only acts when the desktop *changed*,
    /// so a tab opened afterwards would never hear the colours at all.
    #[cfg(has_desktop_theme)]
    fn theme_one(&mut self, at: usize) {
        let Some(t) = self.desktop_theme.as_ref() else { return };
        let (mode, colors) = (Some(t.mode), t.colors.clone());
        let Some(tab) = self.tabs.get_mut(at) else { return };
        let out = tab.backend.desktop_theme(mode, colors);
        tab.send(out);
    }

    /// No desktop and no palette on disk, so nothing to hand on. The
    /// light/dark *mode* still arrives — winit reports `ThemeChanged` in a
    /// page too — and that goes through `Input::Mode` as it always did.
    #[cfg(not(has_desktop_theme))]
    fn theme_one(&mut self, _at: usize) {}

    /// Follow the desktop's palette (05 §5): read it, hand it to every tab
    /// and to the chrome if it changed, and say so once.
    #[cfg(has_desktop_theme)]
    fn follow_desktop_theme(&mut self) {
        let now = crate::desktop_theme::current();
        if now == self.desktop_theme {
            return;
        }
        match &now {
            Some(t) => eprintln!("eui: following the {} ({})", t.source, if t.mode == eui_proto::ThemeMode::Dark { "dark" } else { "light" }),
            None => eprintln!("eui: no desktop theme to follow"),
        }
        let (mode, colors) = now.as_ref().map_or((None, Vec::new()), |t| (Some(t.mode), t.colors.clone()));
        self.desktop_theme = now;
        if let Some((c, _)) = &mut self.chrome {
            c.set_desktop_theme(mode, colors.clone());
        }
        if let Some(m) = mode {
            self.chrome_mode = Some(m);
            // A desktop that publishes a palette has said which of the two
            // it is in, and said it more directly than the window could.
            self.platform_mode = Some(m);
        }
        for t in &mut self.tabs {
            let out = t.backend.desktop_theme(mode, colors.clone());
            t.send(out);
        }
        self.window.request_redraw();
    }

    /// Put the active tab's application in the desktop's launcher, or take
    /// it out. Runs on a thread and wakes the loop when it is done.
    #[cfg(has_launchers)]
    fn launcher_entry(&mut self, adding: bool) {
        let Some(t) = self.tabs.get(self.active) else { return };
        if t.installable.is_none() {
            return;
        }
        let url = t.url.clone();
        let proxy = Arc::clone(&self.proxy);
        let spawned = std::thread::Builder::new().name("eui-install".into()).spawn(move || {
            let done = if adding { add(&url) } else { crate::install::uninstall(&url).map(|_| ()) };
            match done {
                Ok(()) => eprintln!("eui: {url} {}", if adding { "is in the launcher" } else { "is out of the launcher" }),
                Err(e) => eprintln!("eui: {e}"),
            }
            let _ = proxy.send_event(Wake::Installed);
        });
        if let Err(e) = spawned {
            eprintln!("eui: no thread to install with: {e}");
        }
    }

    /// An install or an uninstall finished: ask the record again for every
    /// tab, and draw the address row if any of them changed.
    #[cfg(has_launchers)]
    fn installed_wake(&mut self) {
        let mut moved = false;
        for t in &mut self.tabs {
            let Some(was) = t.installable else { continue };
            let there = crate::install::installed(&t.url);
            if there != was {
                t.installable = Some(there);
                moved = true;
            }
        }
        if moved {
            self.rebuild_chrome();
            self.window.request_redraw();
        }
    }

    /// The pointer takes the shape of what it is over — a hand on a button,
    /// a beam on a field — told to the window only on a change.
    ///
    /// Where the pointer *is* comes from `pointer_in_app`, which the move
    /// that put it there set, and not from the call site. It used to be an
    /// argument, and every call site passed the truth about itself rather
    /// than about the pointer: the chrome's own event path said "over the
    /// chrome", and the paint path said "over the page" — so a page that
    /// repaints on a clock stole the shape back ten times a second while
    /// the pointer stood still on a tab. A page that never repaints never
    /// showed it, which is why it survived this long.
    fn sync_cursor(&mut self) {
        let over_chrome = !self.pointer_in_app;
        let want = match (over_chrome, self.chrome.as_ref()) {
            (true, Some((c, _))) => c.cursor(),
            _ => self.tabs.get(self.active).map_or(eui_proto::Cursor::Default, |t| t.backend.cursor()),
        };
        if want == self.cursor {
            return;
        }
        // `EUI_TRACE=1`: every change of shape, with the two things that
        // decide it. A pointer that flickers while it is not moving is
        // either the page changing its mind under it or this window
        // changing which of the two it asks — and from a screen the two
        // look identical. The line says which.
        crate::driver::trace(|| format!("cursor {:?} -> {want:?} · in_app {} · chrome {}", self.cursor, self.pointer_in_app, self.chrome.is_some()));
        self.cursor = want;
        use eui_proto::Cursor as C;
        use winit::window::CursorIcon as I;
        self.window.set_cursor(match want {
            C::Default => I::Default,
            C::Pointer => I::Pointer,
            C::Text => I::Text,
            C::Grab => I::Grab,
            C::Grabbing => I::Grabbing,
            C::ResizeH => I::EwResize,
            C::ResizeV => I::NsResize,
            C::Wait => I::Wait,
            C::NotAllowed => I::NotAllowed,
        });
    }

    #[cfg(has_clipboard)]
    fn clipboard(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clip.is_none() {
            self.clip = arboard::Clipboard::new().ok();
        }
        self.clip.as_mut()
    }

    /// Everything waiting on every tab's connection.
    ///
    /// Every tab is pumped, not only the visible one — a background
    /// application still answers what it was asked — but only the active
    /// one can ask for the glass.
    fn pump(&mut self) {
        // A `Blob` arrives as a frame, and the bytes it carries are owed to
        // a file (01 §6).
        self.files_dirty = true;
        let active = self.active;
        let mut redraw = false;
        let mut remember = None;
        let proxy = Arc::clone(&self.proxy);
        for (i, t) in self.tabs.iter_mut().enumerate() {
            let before = t.answered;
            let wants = t.pump();
            // 01 §2.7, after the frames that may have named one: the tree
            // that asks for an island is the tree that just arrived.
            t.dial_islands(&proxy);
            redraw |= wants && i == active;
            // The first frame of a session is what makes its address worth
            // keeping: it answered. An address that was merely typed, or
            // one whose connection failed, is not offered again.
            if !before && t.answered {
                remember = Some((t.url.clone(), t.title.clone()));
            }
        }
        if let Some((url, name)) = remember {
            let list = crate::recent::remember(&url, &name);
            if let Some((c, _)) = &mut self.chrome {
                c.set_recents(list);
            }
        }
        // Spec 02 §5.2: what a batch asked to say to the person. Here
        // rather than in `serve_files`, because a notification arrives with
        // a frame and nothing else has to have happened for it — no input,
        // no dialog, and no reason for the window to be in front.
        #[cfg(not(no_subprocess))]
        {
            let id = self.window.id();
            let proxy = Arc::clone(&self.proxy);
            for note in self.tabs.iter_mut().flat_map(|t| t.backend.take_notes()) {
                show_note(&note, id, &proxy);
            }
        }
        if redraw {
            self.window.request_redraw();
        }
    }

    /// Spec 01 §4.1: sockets that broke, tried again when they are due.
    ///
    /// Returns the earliest moment this window wants to be woken for one of
    /// them, so a window waiting on a server that is coming back up sleeps
    /// until it is worth another attempt and not a millisecond less.
    fn serve_links(&mut self, now: crate::time::Instant) -> Option<crate::time::Instant> {
        let proxy = Arc::clone(&self.proxy);
        let mut due: Option<crate::time::Instant> = None;
        let mut changed = false;
        for t in &mut self.tabs {
            // Something happened on a page that only the server can answer.
            // This is the moment the session this endpoint avoided becomes
            // one it needs, and it is the reader's own doing rather than
            // ours: a window opened, a form filled, a row asked for.
            if t.want_socket && matches!(t.link, Link::Static) {
                t.want_socket = false;
                eprintln!("eui: {} needs the server; opening a session", t.url);
                t.dial(&proxy);
                changed = true;
                continue;
            }
            match t.link {
                Link::Lost { at } if at <= now => {
                    eprintln!("eui: reconnecting to {}", t.url);
                    t.dial(&proxy);
                }
                // A socket that was accepted and then said nothing at all.
                Link::Trying { until } if until <= now => {
                    eprintln!("eui: {} accepted the connection and said nothing", t.url);
                    t.lost();
                }
                _ => {}
            }
            if let Link::Lost { at } = t.link {
                due = Some(due.map_or(at, |d: crate::time::Instant| d.min(at)));
            }
            if let Link::Trying { until } = t.link {
                due = Some(due.map_or(until, |d: crate::time::Instant| d.min(until)));
            }
            changed |= t.link_changed();
        }
        if changed {
            self.rebuild_chrome();
            // A chromeless window has no address bar to put a word in, so
            // the word goes where a window says everything else: its title.
            if self.chrome.is_none() {
                if let Some(t) = self.tabs.first() {
                    let title = match t.link_word() {
                        Some(word) => format!("{} — {word}", t.title),
                        None => t.title.clone(),
                    };
                    self.window.set_title(&title);
                }
            }
            self.window.request_redraw();
        }
        due
    }

    /// Spec 03 §3.2: the dialogs the tree asked for, the files they chose,
    /// and the bytes moving either way.
    fn serve_files(&mut self) {
        // A transfer in flight is its own reason to look: its thread has
        // chunks for the socket and the socket has bytes for the disk.
        let moving = self.tabs.iter().any(|t| !t.files.reading.is_empty() || !t.files.writing.is_empty());
        if !self.files_dirty && !moving {
            return;
        }
        self.files_dirty = false;
        let proxy = Arc::clone(&self.proxy);
        // Spec 01 §2.1: a consent sheet that has been answered. Before the
        // dialogs, because the answer is what decides whether `fs.pick`
        // will let one open at all.
        for t in &mut self.tabs {
            let Some(said) = t.backend.take_consent() else { continue };
            t.consent_answered(said, &proxy);
        }
        for i in 0..self.tabs.len() {
            let asks = match self.tabs.get_mut(i) {
                Some(t) => t.backend.take_file_asks(),
                None => continue,
            };
            for ask in asks {
                self.open_dialog(i, ask, &proxy);
            }
            // Scans the tree asked for. The platform that has a reader
            // starts one; the ones that do not say so and end it, which is
            // reported to nobody (06 §3) and leaves the node exactly as it
            // was rather than waiting on an answer that is not coming.
            let scans = match self.tabs.get_mut(i) {
                Some(t) => t.backend.take_nfc_asks(),
                None => continue,
            };
            for scan in scans {
                self.start_scan(i, scan);
            }
            // What the dialogs answered.
            let mut answers = Vec::new();
            if let Some(t) = self.tabs.get(i) {
                while let Ok(d) = t.files.rx.try_recv() {
                    answers.push(d);
                }
            }
            for answer in answers {
                self.dialog_answered(i, answer, &proxy);
            }
            // Bytes read for an upload, framed by the driver and sent.
            self.pump_uploads(i);
            // Bytes the server owes a save, put on disk.
            self.pump_writes(i);
            // And where the machine is, if this tab asked. The driver owns
            // the clock, the coarsening and the four conditions of 06 §1.2;
            // the window's whole part is to run the platform while it is
            // wanted and to hand over what arrives.
            self.serve_location(i);
        }
    }

    /// Run the platform's positioning while a tab wants it, and hand what
    /// arrives to that tab's driver.
    ///
    /// Asked every pass, and a comparison when nothing has changed. The
    /// start and stop are edges: a radio told to run twice is a radio that
    /// was already running, and telling it so once a frame is how a phone
    /// spends a battery on a page that is doing nothing.
    fn serve_location(&mut self, i: usize) {
        let wanted = self.tabs.get(i).is_some_and(|t| t.backend.wants_location());
        if wanted != self.locating {
            self.locating = wanted;
            crate::place::running(wanted);
        }
        if !wanted {
            return;
        }
        crate::place::dev_fix();
        let Some(fix) = crate::place::take() else { return };
        if let Some(t) = self.tabs.get_mut(i) {
            t.backend.located(fix.latitude, fix.longitude, fix.accuracy_m);
        }
    }

    /// Start the platform's own scan (03 §3.3).
    ///
    /// Only the phones have a reader. Everywhere else this ends the scan at
    /// once, which is the same answer the person cancelling would give and
    /// leaves nothing in flight.
    fn start_scan(&mut self, i: usize, ask: crate::driver::NfcAsk) {
        let Some(t) = self.tabs.get_mut(i) else { return };
        if crate::nfc::start(&ask) {
            return;
        }
        eprintln!("eui: this build has no tag reader");
        t.backend.scan_ended(ask.token);
    }

    /// Open the platform's own dialog, on its own thread: a modal panel
    /// must not stop the window drawing behind it, and a portal on Linux
    /// can take a second to appear.
    fn open_dialog(&mut self, i: usize, ask: FileAsk, proxy: &Proxy) {
        let Some(t) = self.tabs.get_mut(i) else { return };
        let (token, tx, proxy) = (ask.token, t.files.tx.clone(), Arc::clone(proxy));
        #[cfg(has_files)]
        {
            eprintln!(
                "eui: node {} asked for {}",
                ask.node,
                match &ask.want {
                    FileWant::Open { accept, multiple, max, source } => format!("{source:?} (accept [{accept}], multiple {multiple}, at most {max} bytes)"),
                    FileWant::Save { name } => format!("somewhere to save \"{name}\""),
                }
            );
            let answer = move |d: Dialog| {
                let _ = tx.send(d);
                let _ = proxy.send_event(Wake::Files);
            };
            let spawned = std::thread::Builder::new().name("eui-dialog".into()).spawn(move || match ask.want {
                // A camera on a desktop is not `rfd`'s to open and is not
                // a file dialog with a different title: there is no capture
                // path on this platform yet, and offering the file system
                // instead would spend a `camera` grant on `fs.pick`'s
                // power. The phones are where this one lands.
                FileWant::Open { source: source @ (crate::driver::PickSource::Camera | crate::driver::PickSource::Microphone), .. } => {
                    eprintln!("eui: this build has no {source:?}");
                    answer(Dialog::Dismissed(token));
                }
                FileWant::Open { accept, multiple, .. } => {
                    let mut dialog = rfd::FileDialog::new();
                    let exts: Vec<&str> = accept.split(',').map(str::trim).filter(|e| !e.is_empty()).collect();
                    if !exts.is_empty() {
                        dialog = dialog.add_filter("Accepted", &exts).add_filter("Every file", &["*"]);
                    }
                    let chosen = if multiple { dialog.pick_files() } else { dialog.pick_file().map(|p| vec![p]) };
                    match chosen {
                        Some(paths) if !paths.is_empty() => {
                            let sized = paths
                                .into_iter()
                                .map(|p| {
                                    let n = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                                    (p, n)
                                })
                                .collect();
                            answer(Dialog::Picked(token, sized));
                        }
                        _ => answer(Dialog::Dismissed(token)),
                    }
                }
                FileWant::Save { name } => match rfd::FileDialog::new().set_file_name(&name).save_file() {
                    Some(path) => answer(Dialog::Saving(token, path)),
                    None => answer(Dialog::Dismissed(token)),
                },
            });
            if spawned.is_err() {
                eprintln!("eui: no thread for the file dialog");
                t.backend.dismissed(token);
            }
        }
        #[cfg(not(has_files))]
        {
            let _ = (tx, proxy, ask);
            eprintln!("eui: this build has no file dialogs");
            t.backend.dismissed(token);
        }
    }

    /// A dialog came back.
    fn dialog_answered(&mut self, i: usize, answer: Dialog, proxy: &Proxy) {
        let Some(t) = self.tabs.get_mut(i) else { return };
        match answer {
            Dialog::Dismissed(token) => t.backend.dismissed(token),
            Dialog::Saving(token, path) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let out = t.backend.saving(token, name);
                t.send(out);
                t.files.writing.insert(token, Writing { path, file: None });
            }
            Dialog::Picked(token, files) => {
                let named: Vec<(String, u64)> = files.iter().map(|(p, n)| (p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), *n)).collect();
                let (ids, frames) = t.backend.picked(token, named);
                t.send(frames);
                // The driver refused the ones past the ceiling and said so
                // on the wire; the window does not read them either.
                for (id, (path, _)) in ids.into_iter().zip(files) {
                    start_reading(t, id, path, proxy);
                }
            }
        }
    }

    /// Chunks a reader thread has ready, framed by the driver and sent.
    fn pump_uploads(&mut self, i: usize) {
        let Some(t) = self.tabs.get_mut(i) else { return };
        let mut out = Vec::new();
        let mut done = Vec::new();
        for r in &t.files.reading {
            loop {
                match r.rx.try_recv() {
                    Ok(Ok((bytes, last))) => {
                        out.push((r.id, bytes, last));
                        if last {
                            done.push(r.id);
                            break;
                        }
                    }
                    Ok(Err(why)) => {
                        out.push((r.id, Vec::new(), false));
                        out.pop();
                        done.push(r.id);
                        eprintln!("eui: reading the file for upload {}: {why}", r.id);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        done.push(r.id);
                        break;
                    }
                }
            }
        }
        for (id, bytes, last) in out {
            let frames = t.backend.upload_chunk(id, bytes, last);
            t.send(frames);
        }
        for id in &done {
            // A reader that stopped without a last chunk failed; the
            // driver knows whether the upload is still open and tells the
            // server only if it is.
            let frames = t.backend.upload_failed(*id, "the file could not be read to the end".into());
            t.send(frames);
        }
        if let Some(t) = self.tabs.get_mut(i) {
            t.files.reading.retain(|r| !done.contains(&r.id));
        }
    }

    /// Bytes a save is owed, appended to the file the person named.
    fn pump_writes(&mut self, i: usize) {
        let Some(t) = self.tabs.get_mut(i) else { return };
        for w in t.backend.take_writes() {
            let Some(slot) = t.files.writing.get_mut(&w.token) else { continue };
            match append_write(slot, w.flag, &w.bytes) {
                Ok(false) => {}
                Ok(true) => {
                    eprintln!("eui: saved {}", slot.path.display());
                    t.files.writing.remove(&w.token);
                }
                Err(why) => {
                    eprintln!("eui: {} was not saved: {why}", slot.path.display());
                    t.files.writing.remove(&w.token);
                }
            }
        }
    }

    /// An input for the active application.
    ///
    /// Pointer positions arrive in the window's coordinates and the
    /// application was laid out as though it owned the window from the top,
    /// so the chrome's height comes off here — and an input over the chrome
    /// never reaches the application at all.
    /// A file is over the page, or is not. Only a report — the box lights,
    /// nothing is read (spec 03 §3.2).
    fn files_dragged(&mut self, at: Option<(f32, f32)>) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let out = t.backend.file_dragged(at);
        // 06 §1: `file_drag` is sent only when the node under the file
        // changes, so nothing to send is nothing to draw either. Worth the
        // branch because this is now called from a poll rather than once
        // an event: without it, a file held over the window repainted the
        // page fifty times a second to show the same highlight.
        if out.is_empty() {
            return;
        }
        t.send(out);
        self.window.request_redraw();
    }

    /// What the Wayland thread saw since the last wake (spec 03 §3.2, the
    /// drop half, on the one platform winit does not report it).
    ///
    /// Nothing here is Wayland-shaped by the time it lands: it goes into
    /// the same two calls the winit arms use, through the same
    /// [`Self::page_point`], so a drop that arrived this way and one that
    /// arrived winit's way are indistinguishable from here down — which is
    /// what spec 03 §3.2 asks of a pick and a drop, and is just as true of
    /// two ways of hearing about the same drop.
    #[cfg(target_os = "linux")]
    fn drain_drops(&mut self) {
        let top = self.chrome.as_ref().map_or(0.0, |(c, _)| c.content_top());
        let zoom = self.zoom();
        // Collected, not iterated: the borrow on `self.dnd` has to end
        // before the window is asked to act on any of it.
        let Some(said) = self.dnd.as_mut().map(crate::wayland::Drops::take) else { return };
        for drag in said {
            match drag {
                // Surface-local logical px are the window's own logical px.
                // There is no scale to take out here, and that is not an
                // omission: winit's Wayland pointer multiplies the very same
                // numbers by the scale factor, and `CursorMoved` above
                // divides it straight back out.
                eui_wayland::Drag::Over(at) => {
                    let at = at.and_then(|(x, y)| Self::page_point(x, y, top, zoom));
                    self.files_dragged(at);
                }
                eui_wayland::Drag::Dropped { at, paths } => {
                    let Some(at) = Self::page_point(at.0, at.1, top, zoom) else { continue };
                    // One call per path, as the winit arm gets one event
                    // per file: each is a whole arrival — an id, an event
                    // and its own bytes.
                    for path in paths {
                        self.file_dropped_at(at, path);
                    }
                }
            }
        }
    }

    /// Spec 03 §3.2: keep the zone lit under the file while it is held
    /// over the window.
    ///
    /// winit says a file arrived and then says nothing more until it
    /// lands, so there is no event to hang this on and it is a poll. It
    /// runs only while a file is actually over this window, and only on
    /// the platforms that have a pointer to ask about — never at rest,
    /// and never on Wayland, where the compositor reports the motion and
    /// `hovering` is never set.
    ///
    /// A hand holding still costs one cursor query and stops here: the
    /// driver would collapse the repeat anyway, but not before a hit test
    /// and, in the sandboxed configuration, a round trip to the worker.
    fn serve_hover(&mut self) {
        if !self.hovering {
            return;
        }
        let live = self.platform_pointer();
        if live == self.hover_at {
            return;
        }
        self.hover_at = live;
        let top = self.chrome.as_ref().map_or(0.0, |(c, _)| c.content_top());
        let zoom = self.zoom();
        let at = live.and_then(|(x, y)| Self::page_point(x, y, top, zoom));
        self.files_dragged(at);
    }

    /// A file was let go over the page at `at`: the same arrival a dialog
    /// gives — an id, a `file_pick`, and the bytes read off the same
    /// reader thread ([`start_reading`]).
    fn file_dropped_at(&mut self, at: (f32, f32), path: std::path::PathBuf) {
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let proxy = Arc::clone(&self.proxy);
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let (ids, frames) = t.backend.file_dropped(at, vec![(name, size)]);
        t.send(frames);
        // The driver refused anything past the ceiling and said so on the
        // wire; the window does not read those either.
        for id in ids {
            start_reading(t, id, path.clone(), &proxy);
        }
        self.files_dirty = true;
        self.window.request_redraw();
    }

    /// Hand a back to the page, and say whether it took it (06 §1.3).
    ///
    /// Asked of `Status` rather than of the driver, so the question does not
    /// cross the worker pipe on every keystroke — which is the whole reason
    /// that struct exists.
    ///
    /// `false` means the page has nowhere to go back to, and the caller then
    /// does whatever the platform would have: nothing on a desktop, and
    /// leaving the application on a phone.
    fn back(&mut self) -> bool {
        let Some(t) = self.tabs.get_mut(self.active) else { return false };
        if !t.backend.takes_back() {
            return false;
        }
        let out = t.backend.input(Input::Back);
        t.send(out);
        true
    }

    fn send_to_tab(&mut self, i: Input) {
        // A click or a key is where a dialog comes from (03 §3.2).
        self.files_dirty = true;
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let out = t.backend.input(i);
        t.send(out);
        // 03 §3.5. The driver has already decided: the person activated a
        // node carrying `open`, the capability was granted, and the address
        // is `https:` with a plain host. All that is left is the platform
        // call, which belongs here because the window owns the platform.
        //
        // Nothing is reported back. Whether the browser opened, how long it
        // took and whether it exists at all stay on this side (08 §8).
        let opening = t.backend.take_open();
        #[cfg(has_clipboard)]
        if let Some(text) = t.backend.take_clipboard() {
            if let Some(c) = self.clipboard() {
                let _ = c.set_text(text);
            }
        }
        if let Some(url) = opening {
            open_in_browser(&url);
        }
        self.settle_ime();
        self.settle_covered();
        if self.tabs.get(self.active).is_some_and(|t| t.backend.needs_redraw()) {
            self.window.request_redraw();
        }
        self.sync_cursor();
    }

    /// The platform changed palette, or has just said which one it was in.
    ///
    /// Both halves matter. The application hears it, as it always did — and
    /// so does the chrome, which has a driver and a palette of its own and
    /// was never told: a tab strip and an address bar in the light above a
    /// page in the dark, which is exactly as odd as it sounds.
    fn set_mode(&mut self, theme: winit::window::Theme) {
        let mode = match theme {
            winit::window::Theme::Dark => eui_proto::ThemeMode::Dark,
            winit::window::Theme::Light => eui_proto::ThemeMode::Light,
        };
        self.follow_platform_mode(mode);
    }

    /// The machine is in this palette now: remember it, and put everything
    /// open into it.
    ///
    /// Every tab, not the active one. A mode is not an event aimed at
    /// whoever has the pointer — it is a fact about the machine, and a tab
    /// in the background is a live session whose server was told a mode
    /// and would otherwise keep rendering for the wrong one until somebody
    /// clicked on it.
    fn follow_platform_mode(&mut self, mode: eui_proto::ThemeMode) {
        if self.platform_mode == Some(mode) {
            return;
        }
        crate::driver::trace(|| format!("platform palette: {mode:?}"));
        self.platform_mode = Some(mode);
        for t in &mut self.tabs {
            let out = t.backend.input(Input::Mode(mode));
            t.send(out);
        }
        if let Some((c, _)) = &mut self.chrome {
            c.set_mode(mode);
        }
        self.chrome_mode = Some(mode);
        self.window.request_redraw();
    }

    /// Where the keyboard belongs: the chrome's address bar when the chrome
    /// holds the keys, else the focused field of the active tab, and the
    /// offset its rectangle has to be read against.
    ///
    /// The chrome half of that was missing, and on a desktop nothing showed
    /// it. A desktop has a keyboard whatever the window believes, so failing
    /// to say "an input method is welcome here" costs a candidate window and
    /// no more. A phone has no keyboard until the application asks for one,
    /// so the same omission is a window nobody can type into — which is what
    /// the shell was on a phone: an address bar, a tap, and nothing.
    fn ime_target(&self) -> (Option<[f32; 4]>, f32) {
        if self.chrome_has_keys() {
            // The chrome draws at the top of the window in the window's own
            // coordinates, so its rectangle needs no offset.
            let area = self.chrome.as_ref().and_then(|(c, _)| c.ime_area());
            return (area.map(|r| [r.x, r.y, r.w, r.h]), 0.0);
        }
        // The rectangle comes back in the page's own px; winit wants the
        // window's, which the zoom has pulled apart. Multiplied here, so
        // `sync_ime` keeps speaking one unit.
        let z = self.zoom();
        let area = self.tabs.get(self.active).and_then(|t| t.backend.ime_area()).map(|[x, y, w, h]| [x * z, y * z, w * z, h * z]);
        (area, self.chrome.as_ref().map_or(0.0, |(c, _)| c.content_top()))
    }

    /// Put the chrome in the palette the visible application is in.
    ///
    /// The chrome draws a box the size of the window under everything, so
    /// that box *is* the page's background wherever the application paints
    /// none of its own — and an application's root very often paints none.
    /// The tab's own clear colour cannot do it: `LoadOp` has no sub-rect,
    /// so the list drawn over the chrome is not allowed to clear at all
    /// (see [`Self::redraw`]), and the ground stays whatever the chrome
    /// put there.
    ///
    /// So the chrome follows. It heard the platform and the desktop
    /// already; what it never heard was the viewer reaching for the
    /// application's own light/dark control, which runs as a local handler
    /// and never leaves the tab. The result was a page in one palette over
    /// a floor in the other.
    ///
    /// Asked once a pass rather than pushed, for the reason
    /// [`Self::settle_ime`] gives: the change can happen without the
    /// transport hearing a thing, and comparing two bytes is cheaper than
    /// finding every place it could have happened.
    fn settle_chrome_mode(&mut self, renderer: &eui_render::Renderer) {
        if self.chrome.is_none() {
            return;
        }
        // An empty tab has no session to take a palette from: the chrome is
        // drawing its own page, and keeps the one it has.
        let Some(mode) = self.tabs.get_mut(self.active).map(|t| t.backend.mode()) else { return };
        if self.chrome_mode == Some(mode) {
            return;
        }
        self.chrome_mode = Some(mode);
        crate::driver::trace(|| format!("chrome follows the page into {mode:?}"));
        self.chrome_input(Input::Mode(mode), renderer);
    }

    /// Tell the page how much of the window a soft keyboard is standing on.
    ///
    /// Asked once a pass, beside [`Self::settle_ime`] and for the same
    /// reason: the keyboard goes up and down because focus moved, and focus
    /// moves for reasons the transport never hears. `Input::Covered` returns
    /// at once when the number has not changed, so asking costs a
    /// comparison.
    ///
    /// Only a phone answers. A desktop keyboard is a thing on a desk and
    /// stands on nothing, so this is zero there and the page is the size of
    /// the window, as it has always been.
    fn settle_covered(&mut self) {
        let covered = self.platform_covered();
        if (self.covered - covered).abs() < 0.5 {
            return;
        }
        self.covered = covered;
        crate::driver::trace(|| format!("the keyboard stands on {covered:.0} px of the page"));
        self.send_to_tab(Input::Covered(covered));
    }

    /// What the platform says its soft keyboard is over, in logical px.
    #[allow(clippy::unused_self)]
    fn platform_covered(&self) -> f32 {
        #[cfg(target_os = "android")]
        {
            crate::android::covered(self.scale())
        }
        #[cfg(target_os = "ios")]
        {
            crate::ios::covered(&self.window)
        }
        // A keyboard on a desk stands on nothing.
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            0.0
        }
    }

    /// Make the platform agree with [`Self::ime_target`].
    ///
    /// Called once a pass of the loop rather than only when the transport
    /// speaks. It used to hang off the pump, which meant a window with no
    /// session — the shell — never reached it at all, and a focus change
    /// that never left the client did not either. `sync_ime` returns at once
    /// when nothing moved, so the cost of asking every pass is a comparison.
    fn settle_ime(&mut self) {
        let (area, top) = self.ime_target();
        self.sync_ime(area, top);
    }

    /// An input method is welcome exactly while a field has focus, and its
    /// candidate window sits under that field. Told only on a change: every
    /// toggle is a protocol round trip with the input method, and inputs
    /// arrive hundreds of times a second.
    fn sync_ime(&mut self, area: Option<[f32; 4]>, top: f32) {
        let had = self.ime_area;
        if area == had {
            return;
        }
        let top = if top.is_finite() { top } else { 0.0 };
        match area {
            Some([x, y, wd, h]) => {
                if had.is_none() {
                    self.window.set_ime_allowed(true);
                }
                self.window.set_ime_cursor_area(LogicalPosition::new(x, y + top), LogicalSize::new(wd, h));
            }
            None => self.window.set_ime_allowed(false),
        }
        // Android has no input-method service to welcome: the keyboard is a
        // window, and it goes up because the application asked. The change
        // of focus that tells a desktop the method is welcome is the same
        // one that raises and dismisses it here.
        #[cfg(target_os = "android")]
        crate::android::soft_input(area.is_some());
        self.ime_area = area;
    }

    /// Take the size the window last settled at, if it moved since the
    /// previous frame. One layout per frame drawn, however many configures
    /// the compositor sent between them.
    fn apply_resize(&mut self, renderer: &eui_render::Renderer) {
        let Some(size) = self.pending_resize.take() else { return };
        let scale = self.scale();
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        if let Some(surface) = &self.surface {
            surface.configure(renderer.device(), &self.config);
        }
        let (w, h) = (size.width as f32 / scale, size.height as f32 / scale);
        if let Some((c, _)) = &mut self.chrome {
            c.resized(w, h, scale);
        }
        let (cw, ch) = self.content_size();
        self.send_to_tab(Input::Resized(cw, ch, self.app_scale()));
    }

    /// Draw the window: the chrome, then the active application over it.
    /// Tell the page that this window has drawn, once.
    ///
    /// A `CustomEvent` on the canvas rather than a callback through
    /// `wasm_bindgen`, so that `start`'s signature stays the three arguments
    /// an embed already passes and a page that does not care listens for
    /// nothing.
    #[cfg(target_arch = "wasm32")]
    fn announce_first_frame(&self) {
        use winit::platform::web::WindowExtWebSys;
        let Some(canvas) = self.window.canvas() else { return };
        let Ok(event) = web_sys::CustomEvent::new("eui:frame") else { return };
        let _ = canvas.dispatch_event(&event);
    }

    fn redraw(&mut self, renderer: &mut eui_render::Renderer) {
        // A lost device is not this frame's problem: every pipeline, buffer
        // and texture built on it is invalid, so there is nothing to draw
        // and nothing to retry. One device stands behind every tab, so
        // every session ends, each with the same reason -- which is what
        // 08 §10 promises a failure does, and the window itself stays up.
        if let Some(e) = renderer.trouble() {
            if !self.gpu_gone {
                self.gpu_gone = true;
                eprintln!("eui: {e}");
                let reason = e.to_string();
                for tab in &mut self.tabs {
                    tab.gpu_gone(&reason);
                }
                self.rebuild_chrome();
            }
            return;
        }
        self.frames = self.frames.saturating_add(1);
        // The page has been waiting to know this, and cannot find it out for
        // itself: an embed shows a still of the application until the session
        // draws, and a canvas revealed before that is the empty rectangle the
        // still exists to prevent. There is no way to ask a `<canvas>` whether
        // anything has been drawn into it — a WebGL context without
        // `preserveDrawingBuffer` reads back blank, and WebGPU offers nothing
        // at all — so the client says so once, and the embed listens.
        //
        // The *first* frame is the wrong one to say it on, and saying it there
        // was worse than the timer it replaced: a window paints its own
        // background before a socket has answered anything, so a session that
        // never connects would still uncover an empty canvas — promptly.
        // A frame drawn while the link is up is a frame with an application
        // in it.
        //
        // `Asking` counts too, and leaving it out was a real fault rather
        // than a nicety. The consent sheet goes up *before* anything is
        // dialled — `Hello` carries the grant, so the question has to be
        // answered first — and it is drawn on this canvas like everything
        // else. Waiting for `Up` therefore kept the canvas at zero opacity
        // underneath the poster for exactly as long as the person was being
        // asked something, which is to say for ever: they are looking at a
        // still image and a note that says "Connecting…", the question is
        // invisible beneath it, and there is no way to answer a question you
        // cannot see. Reported as "l'app demande les droits, mais impossible
        // d'accepter ou pas", which is precisely what it looks like from the
        // outside.
        //
        // Both states mean the same thing to the page: there is something on
        // this canvas that the reader is meant to look at.
        #[cfg(target_arch = "wasm32")]
        if !self.announced && self.tabs.get(self.active).is_some_and(|t| matches!(t.link, Link::Up | Link::Asking | Link::Static)) {
            self.announced = true;
            self.announce_first_frame();
        }
        self.settle_chrome_mode(renderer);
        self.apply_resize(renderer);
        let (w, h) = (self.config.width, self.config.height);
        if w == 0 || h == 0 {
            return;
        }
        let t0 = crate::time::Instant::now();
        let top = self.content_origin();
        let (app_w, app_h) = (w, h.saturating_sub(top));

        // Painted before the surface texture is acquired, so a slow layout
        // does not hold a swapchain image while it runs.
        let chrome_list = self.chrome.as_mut().map(|(c, _)| c.paint(w, h));
        let app = if self.showing_blank() { None } else { self.tabs.get_mut(self.active).map(|t| (t.backend.paint(app_w, app_h.max(1)), t)) };
        let painted = t0.elapsed();

        // Suspended: the work above is thrown away rather than skipped,
        // because the paint is what settles hover and hands the driver its
        // clock. Only the picture has nowhere to go.
        let Some(surface) = &self.surface else { return };
        let frame = match surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                surface.configure(renderer.device(), &self.config);
                return;
            }
            Err(e) => {
                eprintln!("eui: surface: {e}");
                return;
            }
        };
        // The sRGB sibling where the surface itself is not sRGB, so the
        // hardware encodes what the shader wrote; the surface's own format
        // when it already is. `view_formats` above declared this.
        let format = if self.config.format.is_srgb() { self.config.format } else { self.config.format.add_srgb_suffix() };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor { format: Some(format), ..Default::default() });
        let now = self.epoch.elapsed().as_secs_f64();
        let at = crate::time::Instant::now();

        // The chrome first, clearing the whole window; then the application
        // over the part of it below the chrome, which is why the second
        // list must not clear.
        let mut stats = eui_render::RenderStats::default();
        if let (Some(list), Some((chrome, tex))) = (chrome_list, self.chrome.as_mut()) {
            let target = eui_render::Target::whole(&view, format, (w, h), now).aged(chrome.list_age(at));
            let (atlas, images) = chrome.atlases_mut();
            stats = renderer.render(tex, target, &list, atlas, images);
        }
        let mut landed = Vec::new();
        if let Some(((list, l), tab)) = app {
            let target = eui_render::Target {
                view: &view,
                format,
                size: (app_w, app_h.max(1)),
                origin: (0, top.min(h)),
                // With a chrome above it the application must not clear:
                // `LoadOp` has no sub-rect, so a clear here would take the
                // strip with it.
                clear: self.chrome.is_none(),
                now,
                age: tab.backend.list_age(at),
                // Where it was laid out, at its own size: a page on its way
                // somewhere says otherwise here (03 §5).
                shift: (0.0, 0.0),
                scale: 1.0,
                alpha: 1.0,
            };
            landed = l;
            // A scene's mesh and module cross into this process once, on
            // the frame their hash first appears, and are uploaded before
            // the frame that samples them is drawn. Everything else about a
            // scene -- the clock, the uniforms, the turning -- stays on this
            // side and costs the worker nothing.
            for (hash, asset) in tab.backend.take_scene_assets() {
                match asset {
                    crate::driver::SceneAsset::Mesh(m) => renderer.load_mesh(&mut tab.textures, hash, &m.vertices, &m.indices),
                    crate::driver::SceneAsset::Shader(src) => {
                        // Verified again, here, on a module the worker
                        // already approved (11 §4). That is not belt and
                        // braces for its own sake: the worker is the process
                        // that reads what a server sent, so a worker that has
                        // been taken over must not be able to *call* a module
                        // verified. wgpu re-parses the WGSL either way, which
                        // buys memory safety; it knows nothing of bounded
                        // loops, of there being no compute stage, or of the
                        // one binding a scene has. Measured at 81 µs for a
                        // real module, which is a price worth paying once per
                        // hash for a promise that would otherwise rest on the
                        // sandbox holding.
                        //
                        // Then compiled inside a validation scope, because
                        // wgpu's default for an uncaptured error is a panic
                        // -- in this process, which holds the display for
                        // every session. A module refused at either step
                        // leaves its node drawing its own background, and the
                        // session carries on.
                        match eui_shader::verify(&src) {
                            Ok(_) => {
                                if let Err(e) = renderer.load_shader(hash, &src) {
                                    eprintln!("eui: {e}");
                                }
                            }
                            Err(e) => eprintln!("eui: a shader the worker passed was refused here: {e}"),
                        }
                    }
                }
            }
            let tex = &mut tab.textures;
            if let Some(st) = tab.backend.with_atlases(|atlas, images| renderer.render(tex, target, &list, atlas, images)) {
                stats.quads += st.quads;
                stats.runs += st.runs;
                stats.passes += st.passes;
                stats.submits += st.submits;
                stats.instance_bytes += st.instance_bytes;
                stats.atlas_bytes += st.atlas_bytes;
                stats.upload_skipped &= st.upload_skipped;
                stats.gpu_ms = st.gpu_ms.or(stats.gpu_ms);
            }
        }
        frame.present();
        // Still moving? Ask for the next frame here, not on a timer.
        //
        // A deadline of `now + 16 ms` is armed at the *start* of a paint,
        // and the present at the end of it waits for the display. By the
        // time this returns, the deadline is already in the past, so the
        // loop asked for the frame again, and again, and the one it got
        // landed on the refresh after the one it wanted: 34 frames of 60
        // with a page sliding, and six hundred passes a second to get them
        // (measured on a 60 Hz screen, 2026-09-14). Asking now hands the
        // pacing to the compositor's own frame callback, which is the
        // clock an animation should be keeping anyway.
        //
        // Only for a frame that is due within a refresh: a caret's blink is
        // half a second away and a spin asks for thirty a second, and
        // neither wants to be woken sixty times for it.
        let by_now = crate::time::Instant::now();
        let soon = self.tabs.get(self.active).and_then(|t| t.backend.next_frame_at()).is_some_and(|at| at.saturating_duration_since(by_now) <= std::time::Duration::from_millis(20));
        if soon {
            self.window.request_redraw();
        }
        crate::driver::trace(|| {
            format!(
                "frame: layout+paint {:.1} ms, render+present {:.1} ms, {} quads in {} runs, {} passes, {} submits, uploaded {} B instances + {} B atlas{}{}",
                painted.as_secs_f64() * 1e3,
                t0.elapsed().as_secs_f64() * 1e3 - painted.as_secs_f64() * 1e3,
                stats.quads,
                stats.runs,
                stats.passes,
                stats.submits,
                stats.instance_bytes,
                stats.atlas_bytes,
                if stats.upload_skipped { " (the same lists again)" } else { "" },
                stats.gpu_ms.map_or(String::new(), |ms| format!(", gpu {ms:.2} ms"))
            )
        });

        if let Some(t) = self.tabs.get_mut(self.active) {
            // A scroll that landed during this paint reports its offset now.
            t.send(landed);
            // A batch may have added a sound, or taken the last one away.
            let proxy = Arc::clone(&self.proxy);
            t.sync_audio(&proxy);
        }
        // Hover settles at paint; so does what the pointer is over.
        self.sync_cursor();
        // A screen reader that is listening gets the tree as painted; one
        // that is not costs nothing here.
        #[cfg(has_a11y)]
        if let (Some(a), Some(t)) = (&mut self.access, self.tabs.get_mut(self.active)) {
            let backend = &mut t.backend;
            a.update_if_active(|| crate::a11y::to_update(&backend.access_tree()));
        }
    }

    /// An assistive technology's request, turned into what a keyboard user
    /// could do: focus, or focus and press. It reaches the active tab only.
    #[cfg(has_a11y)]
    fn access_event(&mut self, event: accesskit_winit::Event) {
        use accesskit_winit::WindowEvent as A;
        let active = self.active;
        match event.window_event {
            A::InitialTreeRequested => {
                if let (Some(a), Some(t)) = (&mut self.access, self.tabs.get_mut(active)) {
                    let backend = &mut t.backend;
                    a.update_if_active(|| crate::a11y::to_update(&backend.access_tree()));
                }
            }
            A::ActionRequested(req) => {
                let Some(t) = self.tabs.get_mut(active) else { return };
                // 03 §6: focus, click, and the two custom moves. AccessKit has
                // no drag vocabulary — ARIA deprecated `grabbed` and
                // `dropeffect`, and nothing replaced them — so a move arrives
                // as the custom action the node offered, by the index it was
                // published under.
                let out = match req.action {
                    accesskit::Action::Click => t.backend.access_action(req.target_node.0, 1),
                    accesskit::Action::Focus => t.backend.access_action(req.target_node.0, 0),
                    accesskit::Action::CustomAction => match req.data {
                        Some(accesskit::ActionData::CustomAction(i @ (2 | 3))) => t.backend.access_action(req.target_node.0, i as u8),
                        _ => Vec::new(),
                    },
                    _ => Vec::new(),
                };
                t.send(out);
                if t.backend.needs_redraw() {
                    self.window.request_redraw();
                }
            }
            A::AccessibilityDeactivated => {}
        }
    }

    /// The desktop changed its palette. One wake can stand for several
    /// changes, so the flag is cleared before the read, not after.
    fn theme_wake(&mut self) {
        crate::driver::trace(|| "desktop theme wake".into());
        self.theme_pending.store(false, std::sync::atomic::Ordering::SeqCst);
        #[cfg(has_desktop_theme)]
        self.follow_desktop_theme();
        // A desktop with no palette to publish can still have changed its
        // mind about light and dark — that is what the portal's
        // `SettingChanged` is — and `follow_desktop_theme` has nothing to
        // compare in that case and returns having done nothing.
        if let Some(m) = platform_mode(&self.window) {
            self.follow_platform_mode(m);
        }
    }

    /// One event for this window. `false` when it should close.
    fn event(&mut self, renderer: &mut eui_render::Renderer, event: WindowEvent) -> bool {
        #[cfg(has_a11y)]
        if let Some(a) = &mut self.access {
            a.process_event(&self.window, &event);
        }
        // The window's own units: what the chrome is drawn and clicked in,
        // and what winit hands out. The page's are these divided by its
        // zoom, which is what `to_app` below is for.
        let scale = self.scale();
        let zoom = self.zoom();
        let top = self.chrome.as_ref().map_or(0.0, |(c, _)| c.content_top());
        match event {
            WindowEvent::CloseRequested => return false,
            WindowEvent::RedrawRequested => self.redraw(renderer),
            // Kept, not acted on. A compositor sends a configure for every
            // step of a drag — 73 a second, measured on Hyprland — and each
            // one that is laid out before the next arrives is a whole tree
            // walked for a picture nobody sees. Only the last size before a
            // frame is real, so the work is moved to the frame.
            WindowEvent::Resized(size) => {
                self.pending_resize = Some(size);
                self.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let size = self.window.inner_size();
                let scale = scale_factor as f32;
                if let Some((c, _)) = &mut self.chrome {
                    c.resized(size.width as f32 / scale, size.height as f32 / scale, scale);
                }
                let (cw, ch) = self.content_size();
                self.send_to_tab(Input::Resized(cw, ch, scale * zoom));
            }
            // Spec 03 §3.2, the drop half. winit reports a file over the
            // window without telling us where — no platform gives a
            // position with these — so the last pointer we saw is the
            // position, which is what it is on every desktop that has a
            // pointer at all. The chrome strip takes none of it: a file
            // let go over the address bar is not for the page.
            WindowEvent::HoveredFile(_) => {
                // One of these per file, and only on entering the window:
                // no backend sends another as the hand moves. So this is
                // where the poll starts, and `serve_hover` is what keeps
                // the zone lit under the file rather than lit wherever it
                // first crossed the edge.
                let live = self.platform_pointer();
                // Armed only where there is something to poll. X11 keeps
                // the old behaviour — the zone lights where the pointer
                // last was and stays there — because winit discards the
                // `XdndPosition` that would fix it and there is no second
                // way to ask.
                self.hovering = live.is_some();
                self.hover_at = live;
                let at = live.or(self.pointer_at).and_then(|(x, y)| Self::page_point(x, y, top, zoom));
                self.files_dragged(at);
            }
            WindowEvent::HoveredFileCancelled => {
                self.hovering = false;
                self.hover_at = None;
                self.files_dragged(None);
            }
            WindowEvent::DroppedFile(path) => {
                // One event per file, so one call per file: a hand that
                // let go of six gives six of these, and each is a whole
                // arrival — an id, an event and its own bytes.
                // Windows sends no `DragLeave` after a drop and macOS's
                // exit is not promised either, so the poll is stopped
                // here rather than waited on.
                self.hovering = false;
                self.hover_at = None;
                let at = self.pointer_from_platform().and_then(|(x, y)| Self::page_point(x, y, top, zoom));
                if let Some(at) = at {
                    self.file_dropped_at(at, path);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x as f32 / scale, position.y as f32 / scale);
                self.pointer_at = Some((x, y));
                if y < top {
                    // Over the chrome: the application is told the pointer
                    // left, so it does not keep a hover lit under a strip
                    // it cannot see.
                    if self.pointer_in_app {
                        self.pointer_in_app = false;
                        self.send_to_tab(Input::PointerOut);
                    }
                    self.chrome_input(Input::PointerMove(x, y), renderer);
                } else {
                    self.pointer_in_app = true;
                    let (x, y) = Self::to_app(x, y, top, zoom);
                    self.send_to_tab(Input::PointerMove(x, y));
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // The fourth button is a back, not a press on a node: 06 §1
                // defines buttons 0, 1 and 2 and nothing else, so widening
                // `pointer_down` to carry this would be inventing a button
                // the protocol does not have.
                if button == MouseButton::Back {
                    if state == ElementState::Pressed {
                        self.back();
                    }
                    return true;
                }
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    _ => return true,
                };
                let down = state == ElementState::Pressed;
                let i = if down { Input::PointerDown(b) } else { Input::PointerUp(b) };
                if self.pointer_in_app {
                    // A press in the application takes the keyboard back
                    // from the address bar, as clicking a page does.
                    if down {
                        if let Some((c, _)) = &mut self.chrome {
                            c.leave_address();
                        }
                    }
                    self.send_to_tab(i);
                } else {
                    return self.chrome_input(i, renderer);
                }
            }
            // Spec 06 §5: contacts become the pointer. The window decides
            // only which half of itself the gesture belongs to; what it
            // *is* — a tap, a drag, a scroll — is the driver's to work out,
            // because it turns on whether the node under the finger asked
            // to hear moves, and only the driver knows the tree.
            WindowEvent::Touch(t) => {
                use winit::event::TouchPhase as P;
                let (x, y) = (t.location.x as f32 / scale, t.location.y as f32 / scale);
                // A gesture belongs for its whole length to the half it
                // started in: a finger that begins on the tab strip and
                // slides into the page is still pressing a tab.
                if matches!(t.phase, P::Started) {
                    self.pointer_in_app = y >= top;
                    // A touch in the page takes the keyboard back from the
                    // address bar, as a click does.
                    if self.pointer_in_app {
                        if let Some((c, _)) = &mut self.chrome {
                            c.leave_address();
                        }
                    }
                }
                if !self.pointer_in_app {
                    // The chrome has no gestures — a tab is pressed and
                    // released — so the pointer events it already
                    // understands are the whole of it.
                    return match t.phase {
                        P::Started => self.chrome_input(Input::PointerMove(x, y), renderer) && self.chrome_input(Input::PointerDown(0), renderer),
                        P::Moved => self.chrome_input(Input::PointerMove(x, y), renderer),
                        P::Ended => self.chrome_input(Input::PointerUp(0), renderer),
                        P::Cancelled => true,
                    };
                }
                self.pointer_at = Some((x, y));
                let (x, y) = Self::to_app(x, y, top, zoom);
                self.send_to_tab(match t.phase {
                    P::Started => Input::TouchDown(t.id, x, y),
                    P::Moved => Input::TouchMove(t.id, x, y),
                    P::Ended => Input::TouchUp(t.id, x, y),
                    P::Cancelled => Input::TouchCancel(t.id),
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                crate::driver::trace(|| format!("raw wheel {delta:?}"));
                // A pixel delta is a finger on a trackpad: it has moved a
                // distance on the glass, and the page should move the same
                // distance whatever it is drawn at, so it is divided by the
                // zoom of the half it lands in. A line delta is a notch,
                // which asks for a *line* — and a line of a zoomed page is
                // taller, so it is not divided. Built inside the branch
                // because the chrome is never zoomed.
                let scale = if self.pointer_in_app { scale * zoom } else { scale };
                let i = match delta {
                    MouseScrollDelta::LineDelta(x, y) => Input::WheelStep(-x, -y),
                    MouseScrollDelta::PixelDelta(p) => Input::Wheel(-p.x as f32 / scale, -p.y as f32 / scale),
                };
                if self.pointer_in_app {
                    self.send_to_tab(i);
                } else {
                    return self.chrome_input(i, renderer);
                }
            }
            // The chords below are the *window's*, not the page's, and which
            // modifier carries them is a platform's own business. On a Mac
            // they are Command's: Control there belongs to the application,
            // and a terminal drawn in a page needs it — `Ctrl+W` is a word,
            // not a window, and `Option+Left` is a word, not a page. Taking
            // those on macOS made an embedded terminal unusable for exactly
            // the keys a terminal is used with.
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                // OR-ed with the keys held, not replaced by what the
                // compositor reports: see `held`.
                self.modifiers = u32::from(s.shift_key()) | (u32::from(s.control_key()) << 1) | (u32::from(s.alt_key()) << 2) | (u32::from(s.super_key()) << 3) | self.held;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                // The modifier keys are read off the key events too, not
                // only off `ModifiersChanged`. On Wayland under Hyprland
                // the change arrives *after* the key it applies to often
                // enough to matter: `EUI_TRACE=1` showed `Alt` pressed, then
                // `ArrowDown · mods 0`, then the same chord a moment later
                // with `mods 4` — the same Option+↓ walked the workspaces
                // one time and scrolled the terminal's history the next.
                // A press sets the bit before the next key is read and a
                // release clears it; `ModifiersChanged`, when it comes, says
                // the same thing.
                if let Key::Named(m) = &event.logical_key {
                    let bit = match m {
                        NamedKey::Shift => 0b0001,
                        NamedKey::Control => 0b0010,
                        NamedKey::Alt | NamedKey::AltGraph => 0b0100,
                        NamedKey::Super | NamedKey::Meta => 0b1000,
                        _ => 0,
                    };
                    if bit != 0 {
                        if down {
                            self.held |= bit;
                            self.modifiers |= bit;
                        } else {
                            self.held &= !bit;
                            self.modifiers &= !bit;
                        }
                    }
                }
                // What the key types, from the keyboard state this window
                // keeps itself (`eui-wayland`'s `Keys`, on Wayland; `None`
                // anywhere else). Every key goes through it, modifiers
                // included, because the state is *moved by the keys*: a
                // Shift it never saw pressed is a Shift it never applies.
                // That is the point of it. winit's `text` below is filled
                // from a state winit moves only on `ModifiersChanged`, and
                // under Hyprland that arrives after the key it applies to
                // — `held` repaired the bitset, but a fast `Shift`+`1` still
                // went out *named* `1`, and a terminal writes the name.
                #[cfg(target_os = "linux")]
                let typed: Option<String> = self.keys.as_mut().and_then(|k| crate::scancode::scancode_of(event.physical_key).and_then(|s| k.typed(s, down, event.repeat)));
                #[cfg(not(target_os = "linux"))]
                let typed: Option<String> = None;
                // Only when no control, alt or super is held: with those,
                // the character is the control byte or nothing, and the
                // letter is what a chord is named by. Empty is a modifier,
                // a dead key mid-sequence, or a key that types nothing —
                // all of which winit's own answer is left to.
                let typed = typed.filter(|t| !t.is_empty() && self.modifiers & 0b1110 == 0);
                // 06 §1's `key` is the W3C key value, and for a printable
                // that value is **what was typed** — `A` for Shift+a, `é`
                // for a dead key and an e. winit's `logical_key` is not
                // always that: on some backends and layouts it is the
                // unshifted character, so an application reading key names
                // (a terminal, an editor — anything 03 §3.1's `typing` is
                // for, which is told in the same breath that it will never
                // receive `text_input`) saw every capital arrive lowercase
                // and every composed character not arrive at all.
                //
                // `text` is only consulted when no control, alt or super is
                // held: with those, `text` is the control byte or nothing,
                // and the letter is what a chord is named by.
                let name = match &event.logical_key {
                    Key::Named(n) => named(*n),
                    Key::Character(c) => match (&typed, &event.text) {
                        (Some(typed), _) => typed.clone(),
                        (None, Some(typed)) if self.modifiers & 0b1110 == 0 && !typed.is_empty() => typed.to_string(),
                        // No text with the event — a repeat, or a backend
                        // that only fills it on the first press — so the
                        // shift has to be applied here or a held key types
                        // `Aaaa`. Only for a single character, and only
                        // when the layout's own uppercase is a single
                        // character too: `ß` uppercases to `SS`, which is
                        // not a key name.
                        _ if self.modifiers & 0b0001 != 0 && self.modifiers & 0b1110 == 0 => {
                            let said = c.to_string();
                            let up: String = said.to_uppercase();
                            if said.chars().count() == 1 && up.chars().count() == 1 {
                                up
                            } else {
                                said
                            }
                        }
                        _ => c.to_string(),
                    },
                    _ => return true,
                };
                // `EUI_TRACE=1`: every key as the window saw it, with the
                // repeat flag winit gave it. A page that receives six
                // hundred `ArrowDown` in twenty seconds is either a held key
                // or a repeat the backend never stopped, and from the
                // server's log the two are the same line; this one says
                // which, and whether the release ever arrived.
                crate::driver::trace(|| format!("key {name} · down {down} · repeat {} · mods {} · xkb {}", event.repeat, self.modifiers, typed.as_deref().unwrap_or("-")));
                // Going back (06 §1.3). The window takes it before the
                // application hears a keystroke, because 08 §7 says an
                // application never sees one it did not ask for, and
                // `BrowserBack` would otherwise reach it as a key named
                // after a browser this client is not.
                //
                // On Android the system back arrives here and nowhere else:
                // winit maps `KEYCODE_BACK` to `NamedKey::BrowserBack` and
                // reports the event **handled**, so the platform's own
                // "leave the app" is already suppressed by the time this
                // runs. A session that does not take back therefore has to
                // be let go of deliberately, or there is no way out of the
                // application at all.
                // …and the page is asked first about the chord, though not
                // about the button: `BrowserBack` is a button that means one
                // thing, while `Alt+←` is a key an application may have
                // claimed (03 §3.1) — a terminal walks its tabs with it.
                let chord_back = name == "ArrowLeft" && self.modifiers & BACK_MOD != 0 && !self.tabs.get(self.active).is_some_and(|t| t.backend.claims_left());
                if down && (name == "BrowserBack" || chord_back) {
                    if self.back() {
                        return true;
                    }
                    if name == "BrowserBack" {
                        request_exit();
                        return true;
                    }
                }
                // Escape leaves the address bar and puts the address back,
                // which is the only way out for someone who clicked into it
                // and changed nothing.
                if down && name == "Escape" && self.chrome.as_ref().is_some_and(|(c, _)| c.holds_keys()) && !self.showing_blank() {
                    if let Some((c, _)) = &mut self.chrome {
                        c.leave_address();
                    }
                    self.rebuild_chrome();
                    return true;
                }
                // Ctrl/⌘ +, − and 0: the zoom of the page below. The shell's
                // own, like the two below it, and never the application's.
                //
                // No `self.chrome.is_some()` guard, unlike those: a window
                // opened straight onto one address has no strip but is
                // still a page somebody may find too small.
                //
                // `=` and `_` are not padding. On a US layout `+` is
                // Shift+`=` and `−` is unshifted, so what arrives is `=`
                // as often as `+`; every browser accepts both, and a
                // person who holds Shift gets `_`. The release is left to
                // fall through to the page, as Ctrl+T's and Ctrl+W's are.
                if down && self.modifiers & SHELL_MOD != 0 && self.modifiers & 0b0001 == 0 {
                    match name.as_str() {
                        "+" | "=" => return self.set_zoom(zoom_step(self.zoom(), true)),
                        "-" | "_" => return self.set_zoom(zoom_step(self.zoom(), false)),
                        "0" => return self.set_zoom(1.0),
                        _ => {}
                    }
                }
                // Ctrl+T, Ctrl+W: the shell's own, and never the
                // application's — a page must not be able to eat them.
                if down && self.chrome.is_some() && self.shell_chord() {
                    match name.as_str() {
                        "t" | "T" => {
                            return self.chrome_action(crate::chrome::Action::NewTab, renderer);
                        }
                        "w" | "W" => {
                            return self.close_tab(self.active);
                        }
                        _ => {}
                    }
                }
                // While an empty tab or the address bar has focus, the keys
                // are the chrome's.
                let to_chrome = self.chrome_has_keys();
                if down && self.modifiers & 0b1110 == 0 {
                    // The window's own reading first, for the reason above:
                    // a field is as wrong about a fast `Shift`+`1` as a
                    // terminal is.
                    let text = typed.clone().or_else(|| event.text.as_ref().map(ToString::to_string));
                    if let Some(text) = text {
                        if types_text(&event.logical_key) {
                            let i = Input::Text(text);
                            if to_chrome {
                                self.chrome_input(i, renderer);
                            } else {
                                self.send_to_tab(i);
                            }
                        }
                    }
                }
                // Ctrl+V / ⌘V: the person's own clipboard into the field they
                // are editing. The window reads it; the driver never can.
                //
                // The address bar is a field like any other. It was excluded
                // here, which made the one field a person meets before any
                // application has loaded the one field they could not paste
                // an address into.
                //
                // Whether a paste lands is the driver's to say: it drops
                // one that reaches no editable field, and never reports
                // it. The window used to decide instead, by asking
                // whether an input method had somewhere to sit -- which
                // wants the focused field to have a box *this frame*, so
                // a field focused but not laid out refused the paste with
                // no way of knowing why.
                #[cfg(has_clipboard)]
                if down && self.shell_chord() && (name == "v" || name == "V") {
                    let text = self.clipboard().and_then(|c| c.get_text().ok());
                    crate::driver::trace(|| {
                        format!("paste: modifiers {:04b}, {} chars, to the {}", self.modifiers, text.as_ref().map_or(0, String::len), if to_chrome { "address bar" } else { "page" })
                    });
                    if let Some(text) = text {
                        if to_chrome {
                            if !self.chrome_input(Input::Paste(text), renderer) {
                                return false;
                            }
                        } else {
                            self.send_to_tab(Input::Paste(text));
                        }
                    }
                }
                let i = Input::Key { key: name, modifiers: self.modifiers, down };
                if to_chrome {
                    return self.chrome_input(i, renderer);
                }
                self.send_to_tab(i);
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => self.send_to_tab(Input::ImePreedit(text)),
            WindowEvent::Ime(Ime::Commit(text)) => self.send_to_tab(Input::ImeCommit(text)),
            WindowEvent::CursorLeft { .. } => {
                self.pointer_at = None;
                self.pointer_in_app = false;
                self.send_to_tab(Input::PointerOut);
                self.chrome_input(Input::PointerOut, renderer);
            }
            WindowEvent::Focused(false) => {
                // Whatever was held, its release goes to somebody else now.
                self.held = 0;
                self.send_to_tab(Input::Unfocused)
            }
            // Its twin was ignored until `location` arrived: nothing the
            // client did cared that the window had come back, and now
            // something does (06 §3).
            WindowEvent::Focused(true) => self.send_to_tab(Input::Refocused),
            WindowEvent::ThemeChanged(t) => self.set_mode(t),
            _ => {}
        }
        true
    }

    /// Whether the keyboard belongs to the chrome: an empty tab, whose page
    /// is the chrome's own, or an address bar being edited.
    fn chrome_has_keys(&self) -> bool {
        self.tabs.is_empty() || self.chrome.as_ref().is_some_and(|(c, _)| c.holds_keys())
    }

    /// An input for the chrome, and whatever it turned out to mean.
    /// `false` when the window should close.
    fn chrome_input(&mut self, i: Input, renderer: &eui_render::Renderer) -> bool {
        let Some((c, _)) = &mut self.chrome else { return true };
        let actions = c.input(i);
        let redraw = c.needs_redraw();
        // A copy or a cut out of the address bar. The window owns the
        // clipboard on both sides: the chrome asks, it never reaches it.
        #[cfg(has_clipboard)]
        {
            let copied = c.take_clipboard();
            if let Some(text) = copied {
                if let Some(clip) = self.clipboard() {
                    let _ = clip.set_text(text);
                }
            }
        }
        if redraw {
            self.window.request_redraw();
        }
        self.sync_cursor();
        for a in actions {
            if !self.chrome_action(a, renderer) {
                return false;
            }
        }
        true
    }

    /// The platform took the native window away — Android does this every
    /// time the application goes to the background. The surface it was
    /// drawing into is gone with it, so it is dropped here rather than
    /// discovered dead at the next frame, and the finger that was on the
    /// glass is told the gesture ended: nothing else will ever say so.
    fn suspend(&mut self, renderer: &eui_render::Renderer) {
        if self.surface.is_none() {
            return;
        }
        crate::driver::trace(|| "suspended: the surface goes".into());
        // The input goes with the window: a finger on the glass will never
        // be seen to lift, and a press left outstanding is a button that
        // fires when the application comes back. `Unfocused` is the right
        // word for it — the window cannot name the contact, only say that
        // it no longer has the input.
        self.send_to_tab(Input::Unfocused);
        self.pointer_at = None;
        self.pointer_in_app = false;
        // A frame may still be in flight; the surface must not go under it.
        renderer.device().poll(wgpu::Maintain::Wait);
        self.surface = None;
    }

    /// The native window came back. A new surface is made from the same
    /// instance and configured as the old one was, and the window is asked
    /// to redraw: nothing else will ask, because from the application's
    /// side nothing happened.
    fn resume(&mut self, shared: &Shared) {
        if self.surface.is_some() {
            return;
        }
        let surface = match shared.instance.create_surface(Arc::clone(&self.window)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("eui: cannot make the surface again after a suspend: {e}");
                return;
            }
        };
        if !shared.adapter.is_surface_supported(&surface) {
            eprintln!("eui: the surface this window came back with is not supported by the adapter it opened on");
            return;
        }
        // The window may have been resized, or turned, while it was away.
        let size = self.window.inner_size();
        self.config.width = size.width.max(1);
        self.config.height = size.height.max(1);
        // A new surface need not advertise the format the old one took.
        let caps = surface.get_capabilities(&shared.adapter);
        if !caps.formats.contains(&self.config.format) {
            if let Some(f) = caps.formats.iter().copied().find(wgpu::TextureFormat::is_srgb).or_else(|| caps.formats.first().copied()) {
                self.config.format = f;
            }
        }
        surface.configure(shared.renderer.device(), &self.config);
        self.surface = Some(surface);
        let scale = self.scale();
        if let Some((c, _)) = &mut self.chrome {
            c.resized(size.width as f32 / scale, size.height as f32 / scale, scale);
        }
        let (cw, ch) = self.content_size();
        self.send_to_tab(Input::Resized(cw, ch, self.app_scale()));
        self.window.request_redraw();
        crate::driver::trace(|| "resumed: a new surface".into());
    }

    /// Take this window down, in the order the platforms insist on.
    ///
    /// Not left to the drop glue: that runs in field order, which puts the
    /// window first, and both of the steps below have to happen while it is
    /// still alive.
    fn close(self, renderer: &eui_render::Renderer) {
        #[cfg(all(has_a11y, target_os = "linux"))]
        let Shell { window, surface, tabs, access, dnd, .. } = self;
        #[cfg(all(has_a11y, not(target_os = "linux")))]
        let Shell { window, surface, tabs, access, .. } = self;
        #[cfg(all(not(has_a11y), target_os = "linux"))]
        let Shell { window, surface, tabs, dnd, .. } = self;
        #[cfg(all(not(has_a11y), not(target_os = "linux")))]
        let Shell { window, surface, tabs, .. } = self;
        // The thread holds this window's `wl_surface` pointer and reads it
        // on every event the compositor sends. Stopped and joined here,
        // before the window it points into can go — and before the tabs,
        // because a drop still in flight would be talking to one.
        #[cfg(target_os = "linux")]
        drop(dnd);
        for t in tabs {
            t.close("the window closed");
        }
        // The adapter holds the window's platform handle and talks to it as
        // it goes; on macOS a window dropped first leaves it calling into a
        // dead view.
        #[cfg(has_a11y)]
        drop(access);
        // Dropping a surface with a frame still in flight is the classic
        // hang. Wait for the device to go idle, let the surface go, and only
        // then the window it was made from.
        renderer.device().poll(wgpu::Maintain::Wait);
        drop(surface);
        drop(window);
    }

    /// What this window wants of the loop before it parks: `None` to sleep
    /// until something happens, or the instant it wants to be woken at.
    ///
    /// Only the active tab is ticked. A background application is mounted
    /// and idle — it is not being clicked, and nothing it could animate is
    /// on the glass — so four open tabs cost what one does at rest. A frame
    /// the server sends of its own accord still lands: the transport wakes
    /// the loop and the batch is applied wherever it belongs. What a
    /// background tab does not get is a clock, which is the expensive half.
    fn park(&mut self, now: crate::time::Instant) -> Option<crate::time::Instant> {
        let t = self.tabs.get_mut(self.active)?;
        crate::driver::trace(|| format!("about_to_wait: due={:?}", t.backend.next_frame_at().map(|d| d.saturating_duration_since(crate::time::Instant::now()))));
        // A running transition is the only thing that ever wakes the loop by
        // itself; at rest `ControlFlow::Wait` sleeps until the OS or the
        // transport speaks.
        let mut requested = false;
        if t.backend.tick(now) {
            self.window.request_redraw();
            requested = true;
        }
        // A frame already due does not park the loop. It used to: `Wait`
        // sleeps until the OS or the transport speaks, and when the driver
        // said a frame was due *now* while `tick` had not yet agreed — the
        // two read their own clocks, and in the sandboxed configuration the
        // worker's answer is a round trip behind — nothing was scheduled
        // and nothing asked for a redraw. The loop then slept until the
        // next pointer event, which is why an animation ran only while the
        // mouse moved and stopped the moment it was still. Come back in a
        // millisecond instead: it costs a wake-up while a frame is pending
        // and nothing at all at rest, where `next_frame_at` is `None`.
        //
        // But once a redraw *has* been asked for, the frame is the OS's to
        // deliver, at its display's pace, and the due time — which the
        // paint will move on — says nothing until then. Polling it every
        // millisecond meanwhile was a thousand wake-ups a second on macOS,
        // where the redraw comes with the next display refresh rather
        // than at once: a spinner alone kept a core a fifth busy.
        //
        // That last paragraph assumes something it does not say: that the
        // loop passes through here *again* once the redraw is delivered, and
        // re-arms then. True on Linux, and on Android, whose backend forces a
        // zero timeout when a redraw was asked for during `AboutToWait`. On
        // **iOS it is false**: `AboutToWait` is the last event of a pass.
        // winit's UIKit backend dispatches the queued redraws, then
        // `AboutToWait`, then parks the waker at `f64::MAX`
        // (`ios/app_state.rs`, `events_cleared_transition`). The redraw asked
        // for here is honoured — CoreAnimation's commit observer calls
        // `drawRect:` later in the same pass — but nothing comes back to read
        // the deadline that paint sets, so a window that threw its own away
        // paints one more frame and then sleeps for ever. Which is exactly
        // what an iOS spinner did: one frame, then still.
        //
        // Nor can it fall through to the arms below. `tick` has already taken
        // `next_due` by the time this runs, so `due` is `None` here and there
        // is no deadline left to return. Come back and look once the paint
        // has set one, for the same reason and at the same cost as the arm
        // below: a wake-up while a frame is pending, nothing at rest.
        let due = self.tabs.get(self.active).and_then(|t| t.backend.next_frame_at());
        let next = match due {
            _ if requested && cfg!(target_os = "ios") => Some(now + std::time::Duration::from_millis(1)),
            _ if requested => None,
            Some(at) if at > now => Some(at),
            Some(_) => Some(now + std::time::Duration::from_millis(1)),
            None => None,
        };
        // A file held over the window is the one thing here that has to be
        // looked at rather than waited for: winit announces the arrival
        // and then says nothing until the drop, so a loop that parked on
        // `Wait` would leave the zone lit where the file first crossed the
        // edge. This is the only deadline in this function that is not
        // about drawing, and it is armed exactly while a drag is in flight
        // — `HoveredFile` to `DroppedFile`, on the backends that send
        // them, which is never Wayland.
        if self.hovering {
            let poll = now + HOVER_POLL;
            return Some(next.map_or(poll, |at| at.min(poll)));
        }
        next
    }
}

/// How often to ask where a file being dragged has got to.
///
/// Fifty a second: fast enough that the zone lights as the file crosses
/// into it rather than after it, slow enough to be a rounding error beside
/// what the compositor is already doing to drag an icon around. It costs
/// one cursor query per tick while a file is over the window and nothing
/// whatsoever otherwise — `serve_hover` returns at its first line, and
/// `park` arms no deadline.
const HOVER_POLL: std::time::Duration = std::time::Duration::from_millis(20);

/// The deadline the loop wants to be woken at, kept by a thread of our own.
///
/// `ControlFlow::WaitUntil` is the obvious way to do this and it does not
/// work: measured on this client, a window with a thirty-a-second animation
/// passed through `about_to_wait` **125 000 times a second** while winit
/// delivered thirty events, and it did so with a deadline a whole second
/// out just as readily as with one 16 ms out. `WaitUntil` behaves as
/// `Poll` — on Wayland and on macOS alike, which is what made an idle
/// window cost an entire core. `ControlFlow::Wait` sleeps properly, on
/// Linux; on Apple see [`idle_flow`], where neither of them does.
///
/// So the deadline is kept here rather than given to the loop: one thread
/// that sleeps until the moment asked for and then wakes the loop through
/// the proxy — the same path the transport already uses, which demonstrably
/// works. It costs one thread per process and nothing at rest, because a
/// thread waiting on a channel is not running.
struct Timer {
    /// Rearm, or `None` to sleep until told otherwise. The thread ends when
    /// this is dropped.
    tx: mpsc::Sender<Option<crate::time::Instant>>,
    /// What it was last told, so an unchanged deadline is not re-sent on
    /// every pass of the loop.
    armed: Option<crate::time::Instant>,
}

impl Timer {
    /// No thread to keep a deadline on, and none wanted.
    ///
    /// `ControlFlow::WaitUntil` on this backend *is* a `setTimeout`, which
    /// is precisely what the thread below emulates where a platform's own
    /// wait cannot be trusted to fire. So the fallback is not a fallback
    /// here — it is the native mechanism — and `None` asks for it.
    ///
    /// (The note on `idle_flow` about Linux and Apple never sleeping is
    /// about those two platforms and does not apply to a page.)
    #[cfg(target_arch = "wasm32")]
    fn start(_proxy: Proxy) -> Option<Self> {
        None
    }

    /// Start the thread. `None` if one could not be spawned, in which case
    /// the loop falls back to `WaitUntil` and its old behaviour.
    #[cfg(not(target_arch = "wasm32"))]
    fn start(proxy: Proxy) -> Option<Self> {
        let (tx, rx) = mpsc::channel::<Option<crate::time::Instant>>();
        let spawned = std::thread::Builder::new().name("eui-frame-timer".into()).spawn(move || {
            let mut deadline: Option<crate::time::Instant> = None;
            loop {
                let told = match deadline {
                    Some(at) => {
                        let now = crate::time::Instant::now();
                        if now >= at {
                            deadline = None;
                            // The loop does the work; this only says when.
                            if proxy.send_event(Wake::Frame).is_err() {
                                return;
                            }
                            continue;
                        }
                        rx.recv_timeout(at.saturating_duration_since(now))
                    }
                    None => rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected),
                };
                match told {
                    Ok(at) => deadline = at,
                    // The deadline arrived; the top of the loop sends for it.
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    // Every sender is gone: the loop is over.
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
        });
        match spawned {
            Ok(_) => Some(Self { tx, armed: None }),
            Err(e) => {
                eprintln!("eui: no thread for the frame timer ({e}); falling back to the event loop's own, which spins");
                None
            }
        }
    }

    /// Ask to be woken at `at`, or not at all. Sent only when it changes: at
    /// rest this is one message and then silence.
    fn arm(&mut self, at: Option<crate::time::Instant>) {
        if self.armed == at {
            return;
        }
        self.armed = at;
        let _ = self.tx.send(at);
    }
}

/// A window asked for and not yet opened.
struct Pending {
    /// The applications to open in it, one tab each. Empty for the shell,
    /// which opens with nothing in it and is typed into.
    launches: Vec<Launch>,
    /// Whether it gets a tab strip and an address bar.
    chrome: bool,
    /// What the person allowed on the command line.
    ///
    /// The window's, not a launch's. The shell has no launch to hang it on
    /// and still opens tabs that need it, and a chromeless window that
    /// follows a link or reloads builds a new tab from an address alone --
    /// so a grant kept only on the `Launch` is a grant that lasts exactly
    /// one page.
    allowed: u32,
}

/// The process: the event loop, and every window running in it.
pub struct App {
    proxy: Proxy,
    /// The GPU, made by the first window to open and used by every one
    /// after it. `None` until then, and on a machine with no adapter.
    shared: Option<Shared>,
    /// Windows asked for and not yet opened. `resumed` drains it; on the
    /// platforms that suspend and resume, a window already open is not
    /// opened twice.
    pending: Vec<Pending>,
    shells: std::collections::HashMap<WindowId, Shell>,
    /// `EUI_LOOP_STATS=1`: one line a second saying what the loop did.
    loop_stats: Option<LoopStats>,
    /// The next frame's deadline, kept off the event loop. See [`Timer`].
    timer: Option<Timer>,
    /// The socket other `eui` processes hand their launches to, while this
    /// process is the one holding it (`crate::instance`). `None` in an
    /// embedding host, and in the second process of a race.
    #[cfg(has_instance)]
    door: Option<crate::instance::Door>,
}

impl App {
    /// Build for the applications to open when the loop resumes: one
    /// chromeless window each.
    pub fn new(launches: Vec<Launch>, proxy: EventLoopProxy<Wake>) -> Self {
        let proxy = Arc::new(proxy);
        let timer = Timer::start(Arc::clone(&proxy));
        let pending = launches.into_iter().map(|l| Pending { allowed: l.allowed, launches: vec![l], chrome: false }).collect();
        Self {
            proxy,
            shared: None,
            pending,
            shells: std::collections::HashMap::new(),
            loop_stats: LoopStats::asked_for(),
            timer,
            #[cfg(has_instance)]
            door: None,
        }
    }

    /// Build for one window with a tab strip in it, and nothing open.
    ///
    /// `allowed` is `--allow` on the command line. It used to be thrown
    /// away here -- the shell took no grant at all -- so every tab opened
    /// by typing an address asked for capabilities that could never be
    /// given, and `fs.pick` in particular was unreachable from the shell
    /// since the day the shell landed.
    pub fn shell(allowed: u32, proxy: EventLoopProxy<Wake>) -> Self {
        let proxy = Arc::new(proxy);
        let timer = Timer::start(Arc::clone(&proxy));
        let pending = vec![Pending { launches: Vec::new(), chrome: true, allowed }];
        Self {
            proxy,
            shared: None,
            pending,
            shells: std::collections::HashMap::new(),
            loop_stats: LoopStats::asked_for(),
            timer,
            #[cfg(has_instance)]
            door: None,
        }
    }

    /// One window closed. The last one takes the process with it: a client
    /// with no window is not something a person can get back to.
    fn close(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        if let (Some(s), Some(g)) = (self.shells.remove(&id), self.shared.as_ref()) {
            s.close(&g.renderer);
        }
        if self.shells.is_empty() {
            event_loop.exit();
        }
    }

    /// Every window down, in order, on this thread — before anything in
    /// the process exits under a live GPU device.
    fn shutdown(&mut self) {
        if let Some(g) = &self.shared {
            for (_, s) in self.shells.drain() {
                s.close(&g.renderer);
            }
        }
        // Every surface is gone; the device may follow.
        self.shared = None;
    }
}

/// The two halves of opening a window that `ApplicationHandler` calls
/// into but does not define: draining what is pending, and — where the
/// GPU answers late — asking for it.
impl App {
    /// Open every window that was waiting on a GPU. Called from `resumed`
    /// on every target, and again from `Wake::Gpu` on the one where the
    /// answer arrives a turn later.
    fn open_pending(&mut self, event_loop: &ActiveEventLoop) {
        for Pending { launches, chrome, allowed } in std::mem::take(&mut self.pending) {
            match Shell::open(launches, chrome, allowed, event_loop, Arc::clone(&self.proxy), &mut self.shared) {
                Some(s) => {
                    self.shells.insert(s.window.id(), s);
                }
                // The window, the adapter or the manifest said no. With
                // nothing else running there is nothing left to do.
                None if self.shells.is_empty() => {
                    event_loop.exit();
                    return;
                }
                None => {}
            }
        }
    }

    /// Make the page's window, then ask the browser about it.
    ///
    /// Both halves have to be here and in this order. The window is made
    /// synchronously because `create_window` is synchronous even on this
    /// backend, and it is made *first* because wgpu's WebGL2 backend builds
    /// its context out of the canvas — `request_adapter` with no surface
    /// finds WebGPU or nothing, which is a blank rectangle on every browser
    /// that has only WebGL2. The two awaits then go to the browser, and
    /// what comes back is a whole [`Shared`] with the window and surface
    /// inside it.
    #[cfg(target_arch = "wasm32")]
    fn probe_gpu(&mut self, event_loop: &ActiveEventLoop) {
        use winit::platform::web::WindowAttributesExtWebSys;

        let Some(canvas) = crate::web::canvas() else {
            eprintln!("eui: no canvas was handed over; call start() first");
            event_loop.exit();
            return;
        };
        // How big, in device pixels, asked of the page rather than assumed.
        //
        // winit takes a canvas's *backing store* as the window's size, and
        // a fresh canvas's backing store is 300x150 or, where the embed set
        // it to nothing, zero — which the surface then clamps to 1x1 and
        // the session paints one pixel nobody can see. The box the
        // stylesheet gave it is the real answer, and `devicePixelRatio` is
        // what turns that into pixels.
        //
        // Clamped at 2, deliberately. A 3x phone at this size would be
        // asking for a texture twice the area for a paragraph's
        // illustration, and WebGL2's downlevel limits cap an edge at 2048
        // besides.
        let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio()).clamp(1.0, 2.0);
        // The canvas's own box, then the box it sits in, then a desktop's
        // worth of pixels. A canvas that is still `display: none` when this
        // runs measures zero, and a window opened at zero is a surface
        // clamped to one pixel and a session nobody can see — so each
        // fallback is a *different* question rather than the same one
        // retried, and the last cannot fail.
        let (mut css_w, mut css_h) = (canvas.client_width(), canvas.client_height());
        if css_w <= 0 || css_h <= 0 {
            if let Some(parent) = canvas.parent_element() {
                css_w = parent.client_width();
                css_h = parent.client_height();
            }
        }
        if css_w <= 0 || css_h <= 0 {
            eprintln!("eui: the canvas measures nothing yet — opening at 960x640 and waiting for the page to say otherwise");
            css_w = 960;
            css_h = 640;
        }
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        let size = winit::dpi::PhysicalSize::new((f64::from(css_w) * dpr).round() as u32, (f64::from(css_h) * dpr).round() as u32);
        eprintln!("eui: canvas {css_w}x{css_h} css at {dpr}x -> {}x{} device", size.width, size.height);
        // `with_prevent_default` is what stops an arrow key or a space from
        // scrolling the page out from under a session that has focus.
        let attrs = Window::default_attributes().with_canvas(Some(canvas)).with_prevent_default(true).with_inner_size(size);
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("eui: cannot take the canvas: {e}");
                event_loop.exit();
                return;
            }
        };
        // WebGPU where there is one, WebGL2 where there is not. Both, and
        // in that order, because a browser with neither should say so
        // rather than draw nothing.
        let proxy = Arc::clone(&self.proxy);
        wasm_bindgen_futures::spawn_local(async move {
            // Which backend, decided **before** the canvas is touched.
            //
            // `BROWSER_WEBGPU | GL` in one instance looks like the obvious
            // thing and is a trap. A canvas has exactly one context for its
            // lifetime: `create_surface` takes a `webgpu` one wherever
            // WebGPU is compiled in, and from that moment
            // `getContext("webgl2")` on the same canvas returns null for
            // ever. So a browser that *exposes* `navigator.gpu` but hands
            // out no adapter loses WebGL2 as well, and a machine that could
            // have drawn draws nothing. Brave does this with its
            // fingerprinting defences on, and so does a Chrome with the
            // flag off — this was found on the first one.
            //
            // WebGPU can be asked without a surface, though, and asking
            // touches no canvas. So it is asked first about nothing: if it
            // answers, this instance keeps the canvas; if it does not, it
            // never held one and a GL instance gets it instead.
            let webgpu = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::BROWSER_WEBGPU, ..Default::default() });
            let has_webgpu =
                webgpu.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::LowPower, compatible_surface: None, force_fallback_adapter: false }).await.is_some();
            let instance = if has_webgpu {
                webgpu
            } else {
                drop(webgpu);
                eprintln!("eui: no WebGPU adapter here; drawing through WebGL2");
                wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::GL, ..Default::default() })
            };
            let surface = match instance.create_surface(Arc::clone(&window)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("eui: cannot create a surface: {e}");
                    return;
                }
            };
            // Asked again, and this time about the surface: wgpu's WebGL2
            // backend builds its context out of the canvas, so an adapter
            // for that backend does not exist until there is one to ask
            // about.
            let adapter =
                instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::LowPower, compatible_surface: Some(&surface), force_fallback_adapter: false }).await;
            let Some(adapter) = adapter else {
                eprintln!("eui: this browser would not give the page a GPU — WebGPU and WebGL2 both said no");
                return;
            };
            let renderer = match eui_render::Renderer::with_adapter_async(&adapter, false).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("eui: {e}");
                    return;
                }
            };
            let shared = Shared { instance, adapter, renderer, made: Some((window, surface)) };
            // If this fails the loop is already gone, and so is the page.
            let _ = proxy.send_event(Wake::Gpu(Box::new(Gpu(shared))));
        });
    }
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        // Android calls this again every time the application comes back
        // from the background, with a new native window behind each of the
        // ones already open. Their surfaces were dropped on the way out and
        // are made again here, before any window that was still pending.
        if let Some(shared) = self.shared.as_ref() {
            for s in self.shells.values_mut() {
                s.resume(shared);
            }
        }
        // A page asks for its GPU before it can open anything, and the
        // asking takes a turn of the browser's loop. `pending` is left
        // exactly as it is; `Wake::Gpu` arrives and drains it.
        #[cfg(target_arch = "wasm32")]
        if self.shared.is_none() {
            self.probe_gpu(event_loop);
            return;
        }
        self.open_pending(event_loop);
    }

    /// The platform is taking the native windows away. On a desktop this
    /// never fires; on Android it fires whenever the application leaves the
    /// foreground, and a surface still held at that point is a crash on the
    /// way back.
    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        let Some(shared) = self.shared.as_ref() else { return };
        for s in self.shells.values_mut() {
            s.suspend(&shared.renderer);
        }
    }

    /// The loop is ending, which on a page is the only moment there is.
    ///
    /// Native builds do this after `run_app` returns, on the thread that
    /// holds the device (see `run_loop`). `spawn_app` never returns, so
    /// this is where the same work goes — reached whenever something calls
    /// `event_loop.exit()`, which the GPU probe does when a browser gives
    /// the page no adapter. A page closed by the person navigating away
    /// does not come through here at all, and nothing can be done about
    /// that: the browser reclaims the device either way.
    #[cfg(target_arch = "wasm32")]
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.shutdown();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        if let Some(stats) = &mut self.loop_stats {
            let which = match event {
                Wake::Transport => 0,
                Wake::Audio => 1,
                Wake::Files => 2,
                Wake::Theme => 3,
                #[cfg(has_a11y)]
                Wake::Access(_) => 4,
                Wake::Frame => 5,
                #[cfg(target_os = "linux")]
                Wake::Drop => 6,
                #[cfg(target_arch = "wasm32")]
                Wake::Gpu(_) => 7,
                #[cfg(has_instance)]
                Wake::Open(_) => 8,
                #[cfg(not(no_subprocess))]
                Wake::Raise(_) => 9,
                #[cfg(has_launchers)]
                Wake::Installed => 10,
                Wake::Exit => usize::MAX,
            };
            if let Some(slot) = stats.wakes.get_mut(which) {
                *slot = slot.saturating_add(1);
            }
        }
        match event {
            // The browser answered. This is `resumed` picking up where it
            // left off: the window and the surface it made to ask with are
            // inside, and what was pending has been pending since.
            #[cfg(target_arch = "wasm32")]
            Wake::Gpu(g) => {
                self.shared = Some(g.0);
                self.open_pending(event_loop);
            }
            // Which window the transport, the audio thread or the desktop
            // meant is not in the wake, and asking each is a `try_recv` on
            // an empty channel — cheaper than carrying an id would be.
            Wake::Transport => self.shells.values_mut().for_each(Shell::pump),
            Wake::Audio => self.shells.values_mut().for_each(|s| s.tabs.iter_mut().for_each(Tab::drain_audio)),
            // A dialog answered or a chunk is ready. Both are collected in
            // `about_to_wait`, which runs after this and after every other
            // event the loop had waiting — so the wake is nearly the whole
            // message, and the flag is the rest of it: without it that pass
            // would look like every other idle one and skip the collection.
            Wake::Files => self.shells.values_mut().for_each(|s| s.files_dirty = true),
            // The deadline arrived. Everything it was for — ticking the
            // driver, asking for the frame — is `about_to_wait`'s, and that
            // runs after this and after every other event the loop had
            // waiting. The wake is the whole message.
            Wake::Frame => {}
            Wake::Theme => self.shells.values_mut().for_each(Shell::theme_wake),
            // The launcher changed under us. Which window asked is not in
            // the wake and does not need to be: every address row shows
            // whether *its* application is installed, and they are all
            // looking at the same record.
            #[cfg(has_launchers)]
            Wake::Installed => self.shells.values_mut().for_each(Shell::installed_wake),
            // Which window the file was over is not in the wake either;
            // the same `try_recv` on an empty channel answers it.
            #[cfg(target_os = "linux")]
            Wake::Drop => self.shells.values_mut().for_each(Shell::drain_drops),
            Wake::Exit => event_loop.exit(),
            // Somebody clicked a notification (02 §5.2). The window that
            // raised it comes forward, and that is all that happens: the
            // server is not told, the tab is not changed, and a compositor
            // that refuses to move focus without a token of its own has
            // refused nothing the session can see.
            #[cfg(not(no_subprocess))]
            Wake::Raise(id) => {
                if let Some(s) = self.shells.get(&id) {
                    s.window.set_minimized(false);
                    s.window.focus_window();
                }
            }
            // A launch from another process. It opens a window here, with
            // its own tab and its own confined worker -- everything that
            // was separate about it stays separate; what it does not do is
            // ask this machine for a second adapter, device and pipeline
            // set (`crate::instance`).
            #[cfg(has_instance)]
            Wake::Open(open) => {
                let Opening { launches, chrome, allowed } = *open;
                self.pending.push(Pending { launches, chrome, allowed });
                self.open_pending(event_loop);
            }
            #[cfg(has_a11y)]
            Wake::Access(e) => {
                if let Some(s) = self.shells.get_mut(&e.window_id) {
                    // An assistive technology's action is an activation like
                    // any other, and may ask for a dialog.
                    s.files_dirty = true;
                    s.access_event(e);
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(stats) = &mut self.loop_stats {
            let which = match event {
                WindowEvent::RedrawRequested => 0,
                WindowEvent::CursorMoved { .. } => 1,
                WindowEvent::Occluded(_) => 2,
                WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => 3,
                WindowEvent::Focused(_) => 4,
                _ => 5,
            };
            if let Some(slot) = stats.wevents.get_mut(which) {
                *slot = slot.saturating_add(1);
            }
        }
        let Some(shared) = self.shared.as_mut() else { return };
        let Some(shell) = self.shells.get_mut(&id) else { return };
        if !shell.event(&mut shared.renderer, event) {
            self.close(event_loop, id);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = crate::time::Instant::now();
        let body = self.loop_stats.is_some().then(crate::time::Instant::now);
        if let Some(stats) = &mut self.loop_stats {
            stats.passes = stats.passes.saturating_add(1);
            let elapsed = now.saturating_duration_since(stats.since);
            if elapsed >= std::time::Duration::from_secs(1) {
                let frames: u64 = self.shells.values().map(|s| s.frames).sum();
                let woken: String = WAKE_NAMES.iter().zip(stats.wakes.iter()).filter(|(_, n)| **n > 0).map(|(name, n)| format!(", {n} {name}")).collect();
                let events: String = WEVENT_NAMES.iter().zip(stats.wevents.iter()).filter(|(_, n)| **n > 0).map(|(name, n)| format!(" {n} {name}")).collect();
                let shortest = if stats.shortest_us == u64::MAX { "-".to_owned() } else { format!("{}us", stats.shortest_us) };
                eprintln!(
                    "eui loop: {} passes, {} frames in {:.2}s{}; parked {} on Wait / {} on WaitUntil, soonest {} from {}, retry won {}; window events:{}; mean sleep asked {}us; {:.1} ms of the second spent in about_to_wait",
                    stats.passes,
                    frames.saturating_sub(stats.frames_at),
                    elapsed.as_secs_f32(),
                    woken,
                    stats.waits,
                    stats.untils,
                    shortest,
                    stats.shortest_from,
                    stats.retry_won,
                    if events.is_empty() { " none".to_owned() } else { events },
                    stats.total_us.checked_div(stats.untils).unwrap_or(0),
                    stats.body_us as f64 / 1000.0
                );
                *stats = LoopStats {
                    since: now,
                    passes: 0,
                    frames_at: frames,
                    wakes: [0; 7],
                    waits: 0,
                    untils: 0,
                    shortest_us: u64::MAX,
                    shortest_from: "-",
                    retry_won: 0,
                    wevents: [0; 6],
                    body_us: 0,
                    total_us: 0,
                };
            }
        }
        // Dialogs the last events asked for, and the bytes they moved.
        for s in self.shells.values_mut() {
            s.serve_files();
            // Where a file being dragged has got to. Nothing at all
            // unless one is over that window right now.
            s.serve_hover();
            // Where the keyboard belongs may have changed for a reason the
            // transport never heard about — a tap into the address bar, a
            // local handler moving focus. On a phone that is the difference
            // between a keyboard and none.
            s.settle_ime();
            s.settle_covered();
        }
        // A socket that is due to be tried again.
        let retry = self.shells.values_mut().filter_map(|s| s.serve_links(now)).min();
        // The earliest instant any window asked for. One that wants to
        // sleep does not hold the others back, and one that wants a frame
        // does not let them park.
        let due = self.shells.values_mut().filter_map(|s| s.park(now)).min();
        let from = match (due, retry) {
            (Some(a), Some(b)) if b < a => "retry",
            (_, Some(_)) if due.is_none() => "retry",
            _ => "frame",
        };
        let due = match (due, retry) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        if let Some(stats) = &mut self.loop_stats {
            if let Some(at) = due {
                let us = u64::try_from(at.saturating_duration_since(now).as_micros()).unwrap_or(u64::MAX);
                if us < stats.shortest_us {
                    stats.shortest_us = us;
                    stats.shortest_from = from;
                }
                stats.total_us = stats.total_us.saturating_add(us);
                if from == "retry" {
                    stats.retry_won = stats.retry_won.saturating_add(1);
                }
            }
        }
        // The deadline goes to the thread that keeps it (see `Timer`); what
        // the loop parks on is `idle_flow`.
        let flow = match &mut self.timer {
            Some(timer) => {
                timer.arm(due);
                idle_flow(now)
            }
            // No thread to keep it: the old behaviour, which at least
            // animates, rather than a window that freezes.
            None => idle_or_deadline(due, now),
        };
        if let Some(stats) = &mut self.loop_stats {
            // What the loop was actually parked on, not what it had due.
            // A counter that reports the question rather than the answer is
            // how two builds come to look identical when they are not —
            // which cost a round trip to a Mac and back. Read from `flow`
            // itself for the same reason: derived a second time from `due`,
            // it went on reporting `Wait` for a loop the timer thread had
            // already taken the deadline off.
            match flow {
                ControlFlow::Wait => stats.waits = stats.waits.saturating_add(1),
                _ => stats.untils = stats.untils.saturating_add(1),
            }
        }
        event_loop.set_control_flow(flow);
        if let (Some(stats), Some(body)) = (&mut self.loop_stats, body) {
            stats.body_us = stats.body_us.saturating_add(u64::try_from(body.elapsed().as_micros()).unwrap_or(0));
        }
    }
}

/// The control flow for a window with no timer thread of its own: the
/// deadline where there is one, and [`idle_flow`] where there is not.
fn idle_or_deadline(due: Option<crate::time::Instant>, now: crate::time::Instant) -> ControlFlow {
    match due {
        Some(at) => ControlFlow::WaitUntil(at),
        None => idle_flow(now),
    }
}

/// How to park the loop with nothing due.
///
/// `Wait`, and on Linux that is the one that sleeps: `WaitUntil` never does,
/// at any distance — measured, a deadline a whole second out spun the loop
/// 125 000 times a second while `Wait` went silent on the instant. That is
/// why a deadline is kept on a thread of ours at all (see [`Timer`]).
///
/// **On Apple neither control flow sleeps, and winit's waker is not why.**
/// That waker is a `CFRunLoopTimer` with a hundred-nanosecond repeating
/// interval; `Wait` parks it at `f64::MAX` behind a guard that fires once,
/// and `WaitUntil` re-arms it at a fresh date on every pass. If the timer
/// were what kept the loop awake, those two could not behave alike. They do.
/// Measured on an idle shell with no session, the accessibility adapter off
/// and the input method quiet — so with every confound of `ab564e0`'s week
/// removed:
///
/// ```text
/// Wait         250 000 passes a second, 2.0 us of each in this function
/// WaitUntil    283 000 passes a second, 1.0 us of each in this function
/// ```
///
/// Both spin, and the faster of the two is the one whose body got cheaper —
/// which says the loop is running flat out rather than being woken at some
/// outside rate, because an outside rate would not care what our body costs.
/// So the run loop's wait returns immediately, every time: something is
/// always due or always readable, and it is neither a window event nor a
/// wake from any of our threads, because the counter sees none of either.
///
/// `WaitUntil` was tried here (`bb8dd5f`) on the theory that winit's
/// once-only `stop()` left that repeating timer loose. The numbers above are
/// what came back, and the theory is withdrawn rather than left in the tree
/// asserting itself; CoreFoundation clamps an absurd fire date rather than
/// overflowing on it, which is very likely why the guard was never fatal.
/// What is left is a negative result worth having: after `7cfec14` the waker
/// is exonerated on evidence, not on argument, and the next reading has to
/// come from a profile of the spinning thread.
fn idle_flow(_now: crate::time::Instant) -> ControlFlow {
    ControlFlow::Wait
}

// Only the Wayland/X11 window attributes are threaded through this, so on
// every other platform the trait is dead and `-D warnings` says so.
#[cfg(target_os = "linux")]
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
#[cfg(target_os = "linux")]
impl<T> Pipe for T {}

/// Whether a key press should insert the text winit reports for it.
///
/// Most named keys carry text they must not insert: Enter reports "\r",
/// Tab "\t", Backspace "\u{8}". `Space` is the exception — it is a named
/// key whose text is an ordinary character, and excluding the whole class
/// is what made it impossible to type a space into a field.
fn types_text(key: &Key) -> bool {
    match key {
        Key::Named(n) => *n == NamedKey::Space,
        _ => true,
    }
}

fn named(n: NamedKey) -> String {
    match n {
        NamedKey::Enter => "Enter",
        NamedKey::Backspace => "Backspace",
        NamedKey::Tab => "Tab",
        NamedKey::Escape => "Escape",
        NamedKey::Space => " ",
        NamedKey::ArrowLeft => "ArrowLeft",
        NamedKey::ArrowRight => "ArrowRight",
        NamedKey::ArrowUp => "ArrowUp",
        NamedKey::ArrowDown => "ArrowDown",
        NamedKey::Delete => "Delete",
        NamedKey::Home => "Home",
        NamedKey::End => "End",
        NamedKey::PageUp => "PageUp",
        NamedKey::PageDown => "PageDown",
        other => return format!("{other:?}"),
    }
    .to_owned()
}

/// Set while a window's event loop runs.
static WINDOW_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set by [`request_exit`]; the loop closes when it sees it.
static EXIT_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// True while a window is up: a host with a signal handler should then
/// [`request_exit`] and let its main thread return, rather than exit the
/// process under a live GPU device.
pub fn window_is_open() -> bool {
    WINDOW_OPEN.load(std::sync::atomic::Ordering::SeqCst)
}

/// Ask the window to close and [`launch`] to return. Only an atomic store,
/// so it is safe from a signal handler; the loop notices within 100 ms.
pub fn request_exit() {
    EXIT_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Run the client until the window closes.
pub fn run(url: String, allowed: u32) -> Result<(), String> {
    launch(Launch::new(url, allowed))
}

/// Open the shell: one window with a tab strip and nothing in it yet.
///
/// This is what bare `eui` does. Applications are opened by typing an
/// address, and each one that opens gets its own tab — its own confined
/// worker, connection, cookie and textures — beside the others.
///
/// `allowed` is `--allow` on the command line, and it covers every tab the
/// window opens. Bare `eui` with no `--allow` grants nothing, which is
/// what it should do and what it has always said it does; what it used to
/// do as well was ignore the flag when it was given.
pub fn shell(allowed: u32) -> Result<(), String> {
    run_loop(move |proxy| App::shell(allowed, proxy))
}

/// Open a window on the session `launch` describes and run until it closes.
/// Must be called on the main thread.
pub fn launch(launch: Launch) -> Result<(), String> {
    launch_all(vec![launch])
}

/// Open a window on each of `launches` and run until the last one closes.
/// Must be called on the main thread.
///
/// They share this process: one event loop, one GPU device, one set of
/// pipelines, one tokio runtime and one set of TLS roots. What they do not
/// share is anything an application could reach — each keeps its own
/// window, its own confined worker, its own connection and cookie, and its
/// own textures.
///
/// A session that embeds its own server (`host_loopback`) should not be
/// here: the trust it is given is its own, and a shared process would put
/// it beside sessions that do not have it.
pub fn launch_all(launches: Vec<Launch>) -> Result<(), String> {
    run_loop(move |proxy| App::new(launches, proxy))
}

/// The desktop binary's way in: run the loop, and hold the socket that lets
/// a later `eui` open its window in this process instead of building a
/// second GPU stack (`crate::instance`).
///
/// Separate from [`launch_all`] and [`shell`] because an embedding host has
/// no business taking a machine-wide socket: it is the `eui` command that
/// is launched many times over, and only it.
#[cfg(has_instance)]
pub fn joined(launches: Vec<Launch>, chrome: bool, allowed: u32) -> Result<(), String> {
    run_loop(move |proxy| {
        let mut app = if chrome { App::shell(allowed, proxy) } else { App::new(launches, proxy) };
        app.door = crate::instance::listen(Arc::clone(&app.proxy));
        app
    })
}

/// One handle on the loop, held by everything that has to wake it.
///
/// Shared rather than cloned, because on macOS `EventLoopProxy::clone` is
/// not a clone. It builds a whole new `CFRunLoopSource`, adds it to the
/// main run loop under `kCFRunLoopCommonModes`, and calls `CFRunLoopWakeUp`
/// — and its `Drop` releases the source without ever removing it from the
/// loop. So every clone woke the loop and left another source behind for
/// ever; cloning one per pass of `about_to_wait`, as `serve_links` and
/// `serve_files` did, is a loop that wakes itself a quarter of a million
/// times a second and walks a set of sources that never stops growing.
///
/// Measured on an idle window with no session: 283 000 passes a second, a
/// microsecond of each inside `CFRunLoopAddSource`, and it was the profile
/// of the spinning thread that finally named it — no counter here could,
/// because nothing was ever *sent* and so nothing was ever counted.
///
/// An `Arc` costs an atomic increment and touches no run loop at all. The
/// one place that still needs a real one is the accessibility adapter,
/// which takes a proxy by value: one source per window, made once.
type Proxy = Arc<EventLoopProxy<Wake>>;

/// The event loop this platform starts from.
#[cfg(not(target_os = "android"))]
fn build_event_loop() -> Result<EventLoop<Wake>, String> {
    EventLoop::<Wake>::with_user_event().build().map_err(|e| e.to_string())
}

/// Android's loop is built on the activity's own looper, so it has to be
/// handed the `AndroidApp` the platform gave `android_main`. Without one
/// there is no loop to build and nothing sensible to do.
#[cfg(target_os = "android")]
fn build_event_loop() -> Result<EventLoop<Wake>, String> {
    use winit::platform::android::EventLoopBuilderExtAndroid;
    let app = crate::android::app().ok_or("the activity was never handed over: android_main must call eui_client::android::start first")?;
    let mut builder = EventLoop::<Wake>::with_user_event();
    builder.with_android_app(app);
    builder.build().map_err(|e| e.to_string())
}

/// The event loop, whatever is going to run in it. Must be called on the
/// main thread.
#[cfg(target_arch = "wasm32")]
fn run_loop(build: impl FnOnce(EventLoopProxy<Wake>) -> App) -> Result<(), String> {
    use winit::platform::web::EventLoopExtWebSys;
    let event_loop = build_event_loop()?;
    let app = build(event_loop.create_proxy());
    WINDOW_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
    EXIT_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
    // `spawn_app` hands `App` to the browser and returns at once, so there
    // is no "after the loop" here and `Ok(())` means *started*, not
    // finished. No `eui-exit-watch` either: a page has no threads and no
    // signals to watch for, and `request_exit` is nobody's to call.
    //
    // And no `shutdown`. The device, the surfaces and the workers go when
    // the page goes, which is the only moment a browser offers and is not
    // one this is called back on. `run_app` exists on this backend too and
    // reaches its `!` return by throwing a JavaScript exception up through
    // the whole Rust stack; that is not a thing to do to a documentation
    // page.
    event_loop.spawn_app(app);
    Ok(())
}

/// The event loop, whatever is going to run in it. Must be called on the
/// main thread.
#[cfg(not(target_arch = "wasm32"))]
fn run_loop(build: impl FnOnce(EventLoopProxy<Wake>) -> App) -> Result<(), String> {
    let event_loop = build_event_loop()?;
    // Two asked of the loop rather than one cloned: on macOS a clone is a
    // run-loop source that is never taken back out (see `Proxy`), and
    // `create_proxy` is the same cost said plainly.
    let mut app = build(event_loop.create_proxy());
    let proxy = event_loop.create_proxy();
    // A signal handler can only store a flag; this thread turns the flag
    // into a wake, and stops when the loop is gone.
    WINDOW_OPEN.store(true, std::sync::atomic::Ordering::SeqCst);
    EXIT_REQUESTED.store(false, std::sync::atomic::Ordering::SeqCst);
    std::thread::Builder::new()
        .name("eui-exit-watch".into())
        .spawn(move || {
            while WINDOW_OPEN.load(std::sync::atomic::Ordering::SeqCst) {
                if EXIT_REQUESTED.load(std::sync::atomic::Ordering::SeqCst) {
                    let _ = proxy.send_event(Wake::Exit);
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        })
        .ok();
    let result = event_loop.run_app(&mut app).map_err(|e| e.to_string());
    WINDOW_OPEN.store(false, std::sync::atomic::Ordering::SeqCst);
    // The workers and the GPU go here, on this thread, before anyone exits.
    app.shutdown();
    drop(app);
    result
}

/// Hand an address to the platform's opener.
///
/// The one place this client starts another program, and it starts exactly
/// one: the opener, with one argument that has already been checked to be
/// an `https:` URL (`driver::https_host`). It is spawned and forgotten --
/// the status is not waited on and not reported, because there is nobody
/// to report it to.
fn open_in_browser(url: &str) {
    #[cfg(target_os = "linux")]
    let opener = "xdg-open";
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = url;
        return;
    }
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        use std::process::{Command, Stdio};
        let spawned = Command::new(opener).arg(url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        if spawned.is_err() {
            eprintln!("eui: no {opener} on this machine; the address was not opened");
        }
    }
}

/// Say one line to the person through the machine's own notifier
/// (02 §5.2).
///
/// The second place this client starts another program, and the same
/// shape as the first: one program, arguments that were checked before
/// they got here, nothing waited on that the session can see. The driver
/// decided — the capability is granted, the batch was within its four, the
/// line carries no control character — and the platform is the window's.
///
/// The child outlives this call. `notify-send` given an action waits for
/// the notification to be clicked or to expire, and that wait is the whole
/// mechanism by which a click reaches the loop: it prints the action key
/// and exits, and the thread turns that into a [`Wake::Raise`]. A machine
/// whose notifier has no actions at all does the rest of this correctly
/// and simply never prints one.
/// Notification ids by tag, so a second notification carrying a tag still
/// on screen replaces the first (02 §5.2).
///
/// The freedesktop way, which is the only one every daemon implements: the
/// id comes back from the daemon, and `replaces_id` on the next call is
/// that id. The hint some desktops take instead (`x-canonical-private-
/// synchronous`) is honoured by two of them and ignored by the rest.
///
/// Filled by the thread that reads `--print-id` and read by the next
/// notification with the same tag, so two of a tag raised before the first
/// id came back stack rather than replace. That is the right way for this
/// to be wrong: a person sees one notification too many, never one too few.
#[cfg(all(not(no_subprocess), target_os = "linux"))]
static TAGGED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, u32>>> = std::sync::OnceLock::new();

/// The map, made on first use. A `OnceLock` rather than a `LazyLock`
/// because this crate builds on Rust 1.75 and that is 1.80.
#[cfg(all(not(no_subprocess), target_os = "linux"))]
fn tagged() -> &'static std::sync::Mutex<std::collections::HashMap<String, u32>> {
    TAGGED.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Tags remembered before the map is emptied. A session that notifies with
/// a thousand distinct tags is not replacing anything, and the ids of the
/// ones it has stopped showing are worth nothing.
#[cfg(all(not(no_subprocess), target_os = "linux"))]
const MAX_TAGS: usize = 64;

/// Say one line to the person through the machine's own notifier
/// (02 §5.2).
///
/// The second place this client starts another program, and the same shape
/// as the first: one program, arguments that were checked before they got
/// here, nothing the session can observe. The driver decided — the
/// capability is granted, the batch was within its four, the line carries
/// no control character — and the platform is the window's.
///
/// The child outlives this call. `notify-send` given an action waits for
/// the notification to be clicked or to expire, and that wait is the whole
/// mechanism by which a click reaches the loop: it prints the action's name
/// and exits, and the thread turns that into a [`Wake::Raise`]. A daemon
/// with no actions at all does the rest of this correctly and simply never
/// prints one.
#[cfg(not(no_subprocess))]
fn show_note(note: &crate::driver::Note, window: WindowId, proxy: &Proxy) {
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (window, proxy);
        eprintln!("eui: this platform has no notifier this client can reach; \"{}\" was not shown", note.title);
    }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::process::{Command, Stdio};

        #[cfg(target_os = "linux")]
        let mut cmd = {
            let mut c = Command::new("notify-send");
            // `--print-id` writes the daemon's id on the first line, before
            // the wait; `--action` puts a name on the second, after it.
            c.arg("--app-name=eui").arg("--print-id").arg("--action=default=Open");
            if !note.tag.is_empty() {
                if let Some(id) = tagged().lock().ok().and_then(|m| m.get(&note.tag).copied()) {
                    c.arg(format!("--replace-id={id}"));
                }
            }
            // `--` because a title is the server's string: one beginning
            // with a dash is a title, never a flag.
            c.arg("--").arg(&note.title);
            if !note.body.is_empty() {
                c.arg(&note.body);
            }
            c
        };
        // `display notification` takes AppleScript rather than arguments,
        // so the two characters that could end a string in that language
        // are escaped. Nothing else can: the driver took the control
        // characters out, and a script this short has nowhere for a
        // newline to hide. There is no tag here and no click: `osascript`
        // neither replaces a notification nor reports one.
        #[cfg(target_os = "macos")]
        let mut cmd = {
            let script = format!("display notification \"{}\" with title \"{}\"", applescript(&note.body), applescript(&note.title));
            let mut c = Command::new("osascript");
            c.arg("-e").arg(script);
            c
        };
        let Ok(mut child) = cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
            eprintln!("eui: no notifier on this machine; \"{}\" was not shown", note.title);
            return;
        };
        let proxy = Arc::clone(proxy);
        #[cfg(target_os = "linux")]
        let tag = note.tag.clone();
        // A thread each, because the wait *is* the click. They are as many
        // as there are notifications on screen — bounded by the four an
        // application may send in one batch and by how long a daemon keeps
        // one up — and each ends when its notification does.
        std::thread::Builder::new()
            .name("eui-notify".into())
            .spawn(move || {
                use std::io::BufRead;
                let Some(out) = child.stdout.take() else { return };
                for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
                    let line = line.trim();
                    #[cfg(target_os = "linux")]
                    if !tag.is_empty() {
                        if let Ok(id) = line.parse::<u32>() {
                            if let Ok(mut map) = tagged().lock() {
                                if map.len() >= MAX_TAGS {
                                    map.clear();
                                }
                                map.insert(tag.clone(), id);
                            }
                            continue;
                        }
                    }
                    if line == "default" {
                        let _ = proxy.send_event(Wake::Raise(window));
                    }
                }
                let _ = child.wait();
            })
            .ok();
    }
}

/// A string as AppleScript will read it back: the two characters that
/// would end it, escaped.
#[cfg(all(target_os = "macos", not(no_subprocess)))]
fn applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn the_window_carries_an_icon() {
        // The raster is committed and included at compile time, so the only
        // way this fails is that it was replaced by something winit will
        // not take — which would otherwise show up as a window with the
        // blank icon and no error anywhere.
        let icon = window_icon();
        assert!(icon.is_some(), "assets/icon/png/eui-64.png did not decode into an icon");
    }

    /// 01 §4.1: short enough that a wifi hop is over before anyone looks
    /// at the window, capped so a server coming back up is not hammered.
    #[test]
    fn the_backoff_starts_short_and_stops_at_half_a_minute() {
        assert_eq!(backoff(0), std::time::Duration::from_millis(300));
        assert_eq!(backoff(1), std::time::Duration::from_millis(600));
        assert_eq!(backoff(4), std::time::Duration::from_millis(4800));
        assert_eq!(backoff(7), std::time::Duration::from_secs(30));
        assert_eq!(backoff(u32::MAX), std::time::Duration::from_secs(30), "and never grows past it");
    }

    /// 03 §3.2: a file let go over the address bar is not for the page.
    ///
    /// Both paths that can put a file over a window go through this — the
    /// winit one, which has to guess the position from the last pointer,
    /// and the Wayland one, which is given a real one — so this is the
    /// test that keeps them answering alike.
    #[test]
    fn a_file_over_the_chrome_is_not_over_the_page() {
        // The strip is 40 px and the page starts under it.
        assert_eq!(Shell::page_point(10.0, 39.9, 40.0, 1.0), None, "over the strip");
        assert_eq!(Shell::page_point(10.0, 40.0, 40.0, 1.0), Some((10.0, 0.0)), "the page's first row");
        assert_eq!(Shell::page_point(10.0, 60.0, 40.0, 1.0), Some((10.0, 20.0)));
        // The zoom comes out of both axes, and the strip is taken off in
        // window px before it — the strip does not zoom with the page.
        assert_eq!(Shell::page_point(20.0, 80.0, 40.0, 2.0), Some((10.0, 20.0)));
        assert_eq!(Shell::page_point(10.0, 60.0, 40.0, 0.5), Some((20.0, 40.0)));
        // A chromeless window — `eui <url>`, and every phone — has no
        // strip, so nothing is excluded and nothing is subtracted.
        assert_eq!(Shell::page_point(10.0, 0.0, 0.0, 1.0), Some((10.0, 0.0)));
    }

    /// The ladder is the point: a factor applied and undone the same
    /// number of times would not come back to a level anyone can name.
    #[test]
    fn the_zoom_walks_its_ladder_and_stops_at_both_ends() {
        assert_eq!(zoom_step(1.0, true), 1.1);
        assert_eq!(zoom_step(1.1, true), 1.25);
        assert_eq!(zoom_step(1.1, false), 1.0, "in and out again is exactly where it started");
        assert_eq!(zoom_step(3.0, true), 3.0, "the top is not walked off");
        assert_eq!(zoom_step(0.5, false), 0.5, "nor the bottom");
        // A level from somewhere other than this ladder still moves, from
        // the rung nearest it, rather than sticking between two.
        assert_eq!(zoom_step(1.13, true), 1.25);
        assert_eq!(zoom_step(1.13, false), 1.0);
    }

    /// Spec 03 §3.2: a save is created on its first chunk, not when the
    /// path is chosen, and anything that goes wrong takes it away again.
    #[test]
    fn a_save_lands_whole_or_not_at_all() {
        let dir = std::env::temp_dir().join(format!("eui-save-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("export.csv");
        let _ = std::fs::remove_file(&path);

        let mut slot = Writing { path: path.clone(), file: None };
        assert!(!path.exists(), "nothing is written when the path is merely chosen");
        assert_eq!(append_write(&mut slot, eui_proto::Chunked::More, b"a,b\n"), Ok(false));
        assert!(path.exists(), "the first chunk creates it");
        assert_eq!(append_write(&mut slot, eui_proto::Chunked::Last, b"1,2\n"), Ok(true));
        assert_eq!(std::fs::read_to_string(&path).ok().as_deref(), Some("a,b\n1,2\n"));

        // An abort halfway through leaves nothing behind: half an export
        // looks like a whole one until it is opened.
        let half = dir.join("half.csv");
        let mut slot = Writing { path: half.clone(), file: None };
        assert_eq!(append_write(&mut slot, eui_proto::Chunked::More, b"a,b\n"), Ok(false));
        assert!(half.exists());
        assert_eq!(append_write(&mut slot, eui_proto::Chunked::Abort, b"the query failed"), Err("the query failed".into()));
        assert!(!half.exists(), "the partial file is gone");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The other half of a transfer: a file read into chunks that fit a
    /// frame, ending with one that says so.
    #[test]
    fn a_file_is_read_in_chunks_and_the_last_one_says_so() {
        let dir = std::env::temp_dir().join(format!("eui-read-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("rows.csv");
        let big = vec![b'x'; eui_proto::limits::MAX_TRANSFER_CHUNK_BYTES + 5];
        std::fs::write(&path, &big).expect("write the file to read");

        let (tx, rx) = mpsc::sync_channel(8);
        read_chunks(&path, &tx, || {});
        drop(tx);
        let chunks: Vec<_> = rx.into_iter().collect();
        assert_eq!(chunks.len(), 3, "two full chunks, then the tail");
        assert_eq!(chunks[0], Ok((vec![b'x'; eui_proto::limits::MAX_TRANSFER_CHUNK_BYTES], false)));
        assert_eq!(chunks[1], Ok((vec![b'x'; 5], false)));
        assert_eq!(chunks[2], Ok((Vec::new(), true)), "and an empty last one closes it");

        // A file that is not there is an error, not a silent nothing: the
        // driver turns it into an abort so the server stops waiting.
        let (tx, rx) = mpsc::sync_channel(8);
        read_chunks(&dir.join("gone.csv"), &tx, || {});
        drop(tx);
        assert!(matches!(rx.into_iter().next(), Some(Err(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 01 §2.4: a tab whose page came over `GET /_eui/view/<component>`
    /// holds no session, and has to say so.
    ///
    /// The address bar shows the manifest's `entry` — `wss://host/_eui/…`,
    /// which is the application's address and the right thing to show — so
    /// without a word beside it a page reads exactly like a socket. This
    /// used to be `None`, on the reasoning that there was no fault to
    /// report: true, and not the question. The word names a state.
    #[test]
    fn a_page_says_it_is_a_page_and_a_socket_that_is_up_says_nothing() {
        use crate::time::Instant;
        let at = Instant::now();
        assert_eq!(link_word_of(Link::Static, false), Some("page"), "no session, and the bar shows a wss:// address");
        assert_eq!(link_word_of(Link::Up, false), None, "a socket that is talking is the ordinary case and needs no word");

        // The three that were already right, so that a change to one of
        // them has to come through here.
        assert_eq!(link_word_of(Link::Trying { until: at }, false), Some("reconnecting"));
        assert_eq!(link_word_of(Link::Lost { at }, false), Some("reconnecting"));
        assert_eq!(link_word_of(Link::Asking, false), Some("permission"));

        // A tab that never had a session says nothing: the reason is
        // already in the tab, and "offline" would name the wrong fault.
        assert_eq!(link_word_of(Link::Ended, false), None, "refused before it ever connected");
        assert_eq!(link_word_of(Link::Ended, true), Some("offline"), "it had a session and lost it for good");
    }

    #[test]
    fn space_is_the_named_key_that_types() {
        // winit calls Space a *named* key, and the guard here used to
        // exclude that whole class — which is right for every other member
        // of it and wrong for this one. A field could not take a space.
        assert!(types_text(&Key::Named(NamedKey::Space)), "a space must reach the field");

        // The rest of the class reports text that must not be inserted:
        // Enter would type a carriage return, Tab a tab, Backspace a
        // control character.
        for named in [NamedKey::Enter, NamedKey::Tab, NamedKey::Backspace, NamedKey::Escape, NamedKey::ArrowLeft, NamedKey::Delete] {
            assert!(!types_text(&Key::Named(named)), "{named:?} must not insert its own text");
        }

        // An ordinary character always types.
        assert!(types_text(&Key::Character("a".into())));
        assert!(types_text(&Key::Character("é".into())));
    }

    /// 01 §2.1: an address with no path is an origin, and the manifest's
    /// `entry` is what completes it. One that names a path is already whole.
    #[test]
    fn a_pathless_address_is_completed_by_the_manifests_entry() {
        assert_eq!(completed("wss://host.example", "/_eui/session/gallery").as_deref(), Some("wss://host.example/_eui/session/gallery"));
        // A bare origin with the slash typed is the same address.
        assert_eq!(completed("wss://host.example/", "/_eui/session/gallery").as_deref(), Some("wss://host.example/_eui/session/gallery"));
        assert_eq!(completed("ws://127.0.0.1:5092", "/_eui/session/counter").as_deref(), Some("ws://127.0.0.1:5092/_eui/session/counter"));

        // Already whole: what was typed wins, so a person who names a
        // component gets that component and not the default.
        assert_eq!(completed("wss://host.example/_eui/session/music", "/_eui/session/gallery"), None);
        assert_eq!(completed("wss://host.example/anything", "/x"), None);

        // Nothing to complete from, or nothing to complete.
        assert_eq!(completed("wss://host.example", "_eui/session"), None, "01 §2.1: entry is an absolute path");
        assert_eq!(completed("wss://", "/x"), None, "no host");
        assert_eq!(completed("host.example", "/x"), None, "no scheme");
    }
}
