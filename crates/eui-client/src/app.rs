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
    /// AccessKit has something for the window.
    #[cfg(has_a11y)]
    Access(accesskit_winit::Event),
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
    pub fn new(url: String, allowed: u32) -> Self {
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
enum Link {
    /// Talking.
    Up,
    /// A socket is open and has not spoken yet. `until` is when to stop
    /// waiting for it: a server that accepts a connection and says nothing
    /// is not one that is coming back on its own.
    Trying {
        /// When to give up on this attempt and try again.
        until: std::time::Instant,
    },
    /// Nothing is open. `at` is when the next attempt is due.
    Lost {
        /// When to try again.
        at: std::time::Instant,
    },
    /// Ended for a reason another socket cannot fix: the manifest was
    /// refused, the server sent an `Error`, the tree was unusable.
    Ended,
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
    audio: Option<crate::audio::Output>,
    /// Frames the audio thread produced, for the loop to send.
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
    /// How much larger this page is drawn than the display asks for.
    ///
    /// Per tab, as a browser's zoom is per site: two applications open
    /// beside each other were not written at the same size, and a person
    /// who made one readable did not ask for the other to change.
    zoom: f32,
}

/// One window: the surface, the chrome, and the applications in it.
///
/// The window-level state that used to sit on a session lives here, because
/// several tabs share one of each: one surface, one accessibility adapter,
/// one clipboard, one theme watcher, one pointer.
struct Shell {
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
    /// The applications, in strip order.
    tabs: Vec<Tab>,
    /// Which of them is shown and takes the input.
    active: usize,
    modifiers: u32,
    /// A size the compositor asked for and this window has not drawn yet.
    /// Only the last one matters: see the `Resized` arm.
    pending_resize: Option<winit::dpi::PhysicalSize<u32>>,
    /// Where the pointer last was, in the window's own logical pixels.
    pointer_at: Option<(f32, f32)>,
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
    /// The desktop theme watcher, alive as long as the window.
    theme_watch: Option<Box<dyn std::any::Any + Send>>,
    /// The palette the chrome was last put in, or `None` while it has not
    /// been told one.
    ///
    /// The window's, not a tab's: the chrome is drawn once, whatever is
    /// open below it, so what it was last told is a property of the window.
    chrome_mode: Option<eui_proto::ThemeMode>,
    /// The desktop palette last applied.
    desktop_theme: Option<crate::desktop_theme::DesktopTheme>,
    /// A theme wake is queued and not yet handled.
    theme_pending: Arc<std::sync::atomic::AtomicBool>,
    /// When this window started, so the renderer can be handed a monotonic
    /// clock in seconds.
    epoch: std::time::Instant,
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
    since: std::time::Instant,
    /// Passes of `about_to_wait` since then.
    passes: u64,
    /// Frames drawn by every window at that moment, so the difference is
    /// what this second cost.
    frames_at: u64,
    /// Wakes sent to the loop from elsewhere in the process, by kind. A
    /// loop that passes far more often than it draws is being woken, and
    /// this says by whom — which is the difference between a tree that
    /// asks for too many frames and a thread that will not stop talking.
    wakes: [u64; 6],
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
const WAKE_NAMES: [&str; 6] = ["transport", "audio", "files", "theme", "access", "frame"];

impl LoopStats {
    /// A counter, if the environment asked for one.
    fn asked_for() -> Option<Self> {
        std::env::var("EUI_LOOP_STATS").is_ok_and(|v| v == "1").then(|| Self {
            since: std::time::Instant::now(),
            passes: 0,
            frames_at: 0,
            wakes: [0; 6],
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

impl Tab {
    /// Open an application: its worker, its manifest check, its connection.
    ///
    /// `None` only when the worker could not be started — a refused
    /// manifest is reported in the tab rather than losing it, because in a
    /// shell the tab is where a person would look for the reason.
    fn open(launch: Launch, proxy: Proxy, renderer: &eui_render::Renderer, w: f32, h: f32, scale: f32) -> Self {
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
            allowed: launch.allowed,
            backend,
            conn: None,
            cookie: launch.cookie,
            host_loopback: launch.host_loopback,
            textures: renderer.session(),
            audio: None,
            audio_rx: None,
            answered: false,
            trust: crate::chrome::Trust::Unverified,
            link: Link::Ended,
            tries: 0,
            files: Files::default(),
            shown_link: None,
            zoom: 1.0,
        };

        // Spec 01 §2.1: the manifest first. Its signature is verified and
        // its key pinned before a byte of the session is trusted; only the
        // debug loopback of 08 §1 may go on without one.
        match crate::assets::origin_for(&tab.url).map_err(|e| e.to_string()).and_then(|origin| {
            let pins = crate::manifest::pins_dir().ok_or_else(|| "no home directory for the pin store".to_string())?;
            crate::manifest::check(&origin, &pins, tab.cookie.as_deref()).map_err(|e| e.to_string())
        }) {
            Ok(m) => {
                let granted = m.capabilities & tab.allowed;
                let refused = m.capabilities & !tab.allowed;
                eprintln!("eui: {} {} — publisher key pinned; granted [{}], refused [{}]", m.name, m.version, eui_proto::caps::names(granted).join(", "), eui_proto::caps::names(refused).join(", "));
                tab.backend.grant(granted);
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

        tab.dial(&proxy);
        tab
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
                self.link = Link::Trying { until: std::time::Instant::now() + std::time::Duration::from_secs(20) };
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

    /// The socket is gone: count the attempt and say when the next is due.
    fn lost(&mut self) {
        self.conn = None;
        self.tries = self.tries.saturating_add(1);
        let wait = backoff(self.tries.saturating_sub(1));
        self.link = Link::Lost { at: std::time::Instant::now() + wait };
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
        match self.link {
            Link::Up => None,
            Link::Trying { .. } | Link::Lost { .. } => Some("reconnecting"),
            // A tab that never had a session says nothing: the reason it
            // has none is already in the tab, and "offline" over an
            // address that was refused would name the wrong fault.
            Link::Ended if self.answered => Some("offline"),
            Link::Ended => None,
        }
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
        crate::chrome::TabView { title, origin, path, trust: Some(self.trust), link: self.link_word() }
    }

    fn send(&mut self, frames: Vec<Vec<u8>>) {
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
        if let Some(conn) = &self.conn {
            while let Ok(msg) = conn.rx.try_recv() {
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
                        closed = Some(e.to_string());
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
            if let Some(conn) = &self.conn {
                conn.request_asset(hash);
            }
        }
        if let Some(why) = closed {
            // Spec 01 §4.1. The session is the server's; only the socket
            // broke. Say so, and go and get it back.
            eprintln!("eui: the connection went away: {why}");
            self.lost();
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

    /// What the audio thread produced since the last look: a sound's end.
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
    fn open(launches: Vec<Launch>, chrome: bool, event_loop: &ActiveEventLoop, proxy: Proxy, shared: &mut Option<Shared>) -> Option<Self> {
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

        // Vulkan, Metal or DX12 — never GL: on Linux a GL instance loads
        // Mesa's gallium and its LLVM (34 MB of the window's 64 MB PSS,
        // measured), for a backend the primary ones make unneeded.
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
        let surface = match shared.as_ref() {
            Some(g) => make_surface(&g.instance)?,
            None => {
                let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..Default::default() });
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
        let size = window.inner_size();
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
            // No sRGB anywhere: draw rather than refuse, and say why the
            // colours look washed out.
            let first = caps.formats.first().copied().unwrap_or(eui_render::FORMAT);
            eprintln!("eui: no sRGB surface format, falling back to {first:?} — colours will be light");
            first
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(gpu_shared.renderer.device(), &config);
        let scale = window.scale_factor() as f32;

        let textures = gpu_shared.renderer.session();
        let (logical_w, logical_h) = (size.width as f32 / scale, size.height as f32 / scale);
        let mut chrome = chrome.then(|| {
            let mut c = crate::chrome::Chrome::new(logical_w, logical_h, scale);
            c.set_recents(crate::recent::load());
            (c, textures)
        });

        let mut shell = Self {
            window,
            surface: Some(surface),
            config,
            tabs: Vec::new(),
            active: 0,
            modifiers: 0,
            pending_resize: None,
            pointer_at: None,
            pointer_in_app: false,
            proxy,
            #[cfg(has_a11y)]
            access,
            #[cfg(has_clipboard)]
            clip: None,
            cursor: eui_proto::Cursor::Default,
            ime_area: None,
            chrome_mode: None,
            locating: false,
            theme_watch: None,
            desktop_theme: None,
            theme_pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            epoch: std::time::Instant::now(),
            frames: 0,
            files_dirty: true,
            chrome: chrome.take(),
        };

        let renderer = &gpu_shared.renderer;
        let (w, h) = shell.content_size();
        for l in launches {
            let tab = Tab::open(l, Arc::clone(&shell.proxy), renderer, w, h, scale);
            shell.tabs.push(tab);
        }
        shell.rebuild_chrome();

        // Which of the two palettes the platform is in, *before* the first
        // frame. Only `ThemeChanged` used to say, and that fires when the
        // desktop changes its mind — never when a window opens into a
        // choice already made. A client started in the dark drew itself
        // light and stayed that way until somebody toggled the system.
        //
        // Linux hid it: the Omarchy palette below carries a mode with it,
        // so the one desktop this was developed on always knew. macOS and
        // Windows have no such file and were simply wrong.
        if let Some(theme) = shell.window.theme() {
            shell.set_mode(theme, renderer);
        }

        // The desktop's own colours, before the first frame; and again
        // whenever the desktop changes them.
        //
        // After the tabs, not before: `follow_desktop_theme` hands the
        // palette to the applications that are open, and it remembers what
        // it last read, so running it against an empty window read the
        // theme, told nobody, and made every later call a no-op. The
        // window then came up in the default palette and stayed there.
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
            let blank = crate::chrome::TabView { title: "New tab", origin: "", path: "", trust: None, link: None };
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
        let (w, h) = self.content_size();
        // The zoom stays with the tab, not with the session in it: a reload
        // — which comes through here — would otherwise throw it away, and
        // so would typing the same address again.
        let zoom = self.zoom();
        let scale = self.app_scale();
        let launch = Launch::new(url, 0);
        let mut tab = Tab::open(launch, Arc::clone(&self.proxy), renderer, w, h, scale);
        tab.zoom = zoom;
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
            A::LeaveAddress => {
                if let Some((c, _)) = &mut self.chrome {
                    c.leave_address();
                }
                self.rebuild_chrome();
            }
            A::Reload => {
                if let Some(url) = self.tabs.get(self.active).map(|t| t.url.clone()) {
                    self.open_url(url, renderer);
                }
            }
            A::Open(url) => {
                if let Some((c, _)) = &mut self.chrome {
                    c.leave_address();
                }
                self.open_url(url, renderer);
            }
        }
        true
    }

    /// Hand the palette the window is already following to one tab.
    ///
    /// [`Self::follow_desktop_theme`] only acts when the desktop *changed*,
    /// so a tab opened afterwards would never hear the colours at all.
    fn theme_one(&mut self, at: usize) {
        let Some(t) = self.desktop_theme.as_ref() else { return };
        let (mode, colors) = (Some(t.mode), t.colors.clone());
        let Some(tab) = self.tabs.get_mut(at) else { return };
        let out = tab.backend.desktop_theme(mode, colors);
        tab.send(out);
    }

    /// Follow the desktop's palette (05 §5): read it, hand it to every tab
    /// and to the chrome if it changed, and say so once.
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
        }
        for t in &mut self.tabs {
            let out = t.backend.desktop_theme(mode, colors.clone());
            t.send(out);
        }
        self.window.request_redraw();
    }

    /// The pointer takes the shape of what it is over — a hand on a button,
    /// a beam on a field — told to the window only on a change.
    fn sync_cursor(&mut self, over_chrome: bool) {
        let want = match (over_chrome, self.chrome.as_ref()) {
            (true, Some((c, _))) => c.cursor(),
            _ => self.tabs.get(self.active).map_or(eui_proto::Cursor::Default, |t| t.backend.cursor()),
        };
        if want == self.cursor {
            return;
        }
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
        for (i, t) in self.tabs.iter_mut().enumerate() {
            let before = t.answered;
            let wants = t.pump();
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
        if redraw {
            self.window.request_redraw();
        }
    }

    /// Spec 01 §4.1: sockets that broke, tried again when they are due.
    ///
    /// Returns the earliest moment this window wants to be woken for one of
    /// them, so a window waiting on a server that is coming back up sleeps
    /// until it is worth another attempt and not a millisecond less.
    fn serve_links(&mut self, now: std::time::Instant) -> Option<std::time::Instant> {
        let proxy = Arc::clone(&self.proxy);
        let mut due: Option<std::time::Instant> = None;
        let mut changed = false;
        for t in &mut self.tabs {
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
                due = Some(due.map_or(at, |d: std::time::Instant| d.min(at)));
            }
            if let Link::Trying { until } = t.link {
                due = Some(due.map_or(until, |d: std::time::Instant| d.min(until)));
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
    fn send_to_tab(&mut self, i: Input) {
        // A click or a key is where a dialog comes from (03 §3.2).
        self.files_dirty = true;
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let out = t.backend.input(i);
        t.send(out);
        #[cfg(has_clipboard)]
        if let Some(text) = t.backend.take_clipboard() {
            if let Some(c) = self.clipboard() {
                let _ = c.set_text(text);
            }
        }
        self.settle_ime();
        if self.tabs.get(self.active).is_some_and(|t| t.backend.needs_redraw()) {
            self.window.request_redraw();
        }
        self.sync_cursor(false);
    }

    /// The platform changed palette, or has just said which one it was in.
    ///
    /// Both halves matter. The application hears it, as it always did — and
    /// so does the chrome, which has a driver and a palette of its own and
    /// was never told: a tab strip and an address bar in the light above a
    /// page in the dark, which is exactly as odd as it sounds.
    fn set_mode(&mut self, theme: winit::window::Theme, renderer: &eui_render::Renderer) {
        let mode = match theme {
            winit::window::Theme::Dark => eui_proto::ThemeMode::Dark,
            winit::window::Theme::Light => eui_proto::ThemeMode::Light,
        };
        crate::driver::trace(|| format!("platform palette: {mode:?}"));
        self.send_to_tab(Input::Mode(mode));
        self.chrome_mode = Some(mode);
        self.chrome_input(Input::Mode(mode), renderer);
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
    fn redraw(&mut self, renderer: &mut eui_render::Renderer) {
        self.frames = self.frames.saturating_add(1);
        self.settle_chrome_mode(renderer);
        self.apply_resize(renderer);
        let (w, h) = (self.config.width, self.config.height);
        if w == 0 || h == 0 {
            return;
        }
        let t0 = std::time::Instant::now();
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
        let view = frame.texture.create_view(&Default::default());
        let format = self.config.format;
        let now = self.epoch.elapsed().as_secs_f64();
        let at = std::time::Instant::now();

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
            };
            landed = l;
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
        self.sync_cursor(false);
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
        self.follow_desktop_theme();
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
            WindowEvent::ModifiersChanged(m) => {
                let s = m.state();
                self.modifiers = u32::from(s.shift_key()) | (u32::from(s.control_key()) << 1) | (u32::from(s.alt_key()) << 2) | (u32::from(s.super_key()) << 3);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                let name = match &event.logical_key {
                    Key::Named(n) => named(*n),
                    Key::Character(c) => c.to_string(),
                    _ => return true,
                };
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
                if down && self.modifiers & 0b1010 != 0 {
                    match name.as_str() {
                        "+" | "=" => return self.set_zoom(zoom_step(self.zoom(), true)),
                        "-" | "_" => return self.set_zoom(zoom_step(self.zoom(), false)),
                        "0" => return self.set_zoom(1.0),
                        _ => {}
                    }
                }
                // Ctrl+T, Ctrl+W: the shell's own, and never the
                // application's — a page must not be able to eat them.
                if down && self.chrome.is_some() && self.modifiers & 0b1010 != 0 {
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
                    if let Some(text) = &event.text {
                        if types_text(&event.logical_key) {
                            let i = Input::Text(text.to_string());
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
                if down && self.modifiers & 0b1010 != 0 && (name == "v" || name == "V") {
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
            WindowEvent::Focused(false) => self.send_to_tab(Input::Unfocused),
            // Its twin was ignored until `location` arrived: nothing the
            // client did cared that the window had come back, and now
            // something does (06 §3).
            WindowEvent::Focused(true) => self.send_to_tab(Input::Refocused),
            WindowEvent::ThemeChanged(t) => self.set_mode(t, renderer),
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
        self.sync_cursor(true);
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
        #[cfg(has_a11y)]
        let Shell { window, surface, tabs, access, .. } = self;
        #[cfg(not(has_a11y))]
        let Shell { window, surface, tabs, .. } = self;
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
    fn park(&mut self, now: std::time::Instant) -> Option<std::time::Instant> {
        let t = self.tabs.get_mut(self.active)?;
        crate::driver::trace(|| format!("about_to_wait: due={:?}", t.backend.next_frame_at().map(|d| d.saturating_duration_since(std::time::Instant::now()))));
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
        let due = self.tabs.get(self.active).and_then(|t| t.backend.next_frame_at());
        match due {
            _ if requested => None,
            Some(at) if at > now => Some(at),
            Some(_) => Some(now + std::time::Duration::from_millis(1)),
            None => None,
        }
    }
}

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
    tx: mpsc::Sender<Option<std::time::Instant>>,
    /// What it was last told, so an unchanged deadline is not re-sent on
    /// every pass of the loop.
    armed: Option<std::time::Instant>,
}

impl Timer {
    /// Start the thread. `None` if one could not be spawned, in which case
    /// the loop falls back to `WaitUntil` and its old behaviour.
    fn start(proxy: Proxy) -> Option<Self> {
        let (tx, rx) = mpsc::channel::<Option<std::time::Instant>>();
        let spawned = std::thread::Builder::new().name("eui-frame-timer".into()).spawn(move || {
            let mut deadline: Option<std::time::Instant> = None;
            loop {
                let told = match deadline {
                    Some(at) => {
                        let now = std::time::Instant::now();
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
    fn arm(&mut self, at: Option<std::time::Instant>) {
        if self.armed == at {
            return;
        }
        self.armed = at;
        let _ = self.tx.send(at);
    }
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
    pending: Vec<(Vec<Launch>, bool)>,
    shells: std::collections::HashMap<WindowId, Shell>,
    /// `EUI_LOOP_STATS=1`: one line a second saying what the loop did.
    loop_stats: Option<LoopStats>,
    /// The next frame's deadline, kept off the event loop. See [`Timer`].
    timer: Option<Timer>,
}

impl App {
    /// Build for the applications to open when the loop resumes: one
    /// chromeless window each.
    pub fn new(launches: Vec<Launch>, proxy: EventLoopProxy<Wake>) -> Self {
        let proxy = Arc::new(proxy);
        let timer = Timer::start(Arc::clone(&proxy));
        Self { proxy, shared: None, pending: launches.into_iter().map(|l| (vec![l], false)).collect(), shells: std::collections::HashMap::new(), loop_stats: LoopStats::asked_for(), timer }
    }

    /// Build for one window with a tab strip in it, and nothing open.
    pub fn shell(proxy: EventLoopProxy<Wake>) -> Self {
        let proxy = Arc::new(proxy);
        let timer = Timer::start(Arc::clone(&proxy));
        Self { proxy, shared: None, pending: vec![(Vec::new(), true)], shells: std::collections::HashMap::new(), loop_stats: LoopStats::asked_for(), timer }
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
        for (launches, chrome) in std::mem::take(&mut self.pending) {
            match Shell::open(launches, chrome, event_loop, Arc::clone(&self.proxy), &mut self.shared) {
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
                Wake::Exit => usize::MAX,
            };
            if let Some(slot) = stats.wakes.get_mut(which) {
                *slot = slot.saturating_add(1);
            }
        }
        match event {
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
            Wake::Exit => event_loop.exit(),
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
        let now = std::time::Instant::now();
        let body = self.loop_stats.is_some().then(std::time::Instant::now);
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
                    wakes: [0; 6],
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
            // Where the keyboard belongs may have changed for a reason the
            // transport never heard about — a tap into the address bar, a
            // local handler moving focus. On a phone that is the difference
            // between a keyboard and none.
            s.settle_ime();
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
fn idle_or_deadline(due: Option<std::time::Instant>, now: std::time::Instant) -> ControlFlow {
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
fn idle_flow(_now: std::time::Instant) -> ControlFlow {
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
pub fn shell() -> Result<(), String> {
    run_loop(App::shell)
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
}
