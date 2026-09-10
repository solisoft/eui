//! The window: winit in `ControlFlow::Wait`, a wgpu surface, and the driver.
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

use crate::driver::Input;
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
    /// The host asked the window to close (a signal, say).
    Exit,
    /// AccessKit has something for the window.
    #[cfg(feature = "a11y")]
    Access(accesskit_winit::Event),
}

#[cfg(feature = "a11y")]
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
    /// Where the input method was last pointed, while this tab was active.
    ime_area: Option<[f32; 4]>,
    /// What the strip calls it: the manifest's name, or the last path
    /// segment until the manifest arrives.
    title: String,
    /// What the address bar says about the origin.
    trust: crate::chrome::Trust,
}

/// One window: the surface, the chrome, and the applications in it.
///
/// The window-level state that used to sit on a session lives here, because
/// several tabs share one of each: one surface, one accessibility adapter,
/// one clipboard, one theme watcher, one pointer.
struct Shell {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// The tab strip and address bar, and the textures they draw into.
    /// `None` for a window opened on a URL: `eui <url>` is one application
    /// in one chromeless window, which is what an embedding host gets.
    chrome: Option<(crate::chrome::Chrome, eui_render::SessionTextures)>,
    /// The applications, in strip order.
    tabs: Vec<Tab>,
    /// Which of them is shown and takes the input.
    active: usize,
    modifiers: u32,
    /// Where the pointer last was, in the window's own logical pixels.
    pointer_at: Option<(f32, f32)>,
    /// Whether that was over the application rather than the chrome. Kept
    /// rather than recomputed so a button pressed in one and released in
    /// the other does not arrive as half a click in each.
    pointer_in_app: bool,
    proxy: EventLoopProxy<Wake>,
    #[cfg(feature = "a11y")]
    access: Option<accesskit_winit::Adapter>,
    #[cfg(feature = "clipboard")]
    clip: Option<arboard::Clipboard>,
    /// The pointer shape last handed to the window.
    cursor: eui_proto::Cursor,
    /// The desktop theme watcher, alive as long as the window.
    theme_watch: Option<Box<dyn std::any::Any + Send>>,
    /// The desktop palette last applied.
    desktop_theme: Option<crate::desktop_theme::DesktopTheme>,
    /// A theme wake is queued and not yet handled.
    theme_pending: Arc<std::sync::atomic::AtomicBool>,
    /// When this window started, so the renderer can be handed a monotonic
    /// clock in seconds.
    epoch: std::time::Instant,
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
    fn open(launch: Launch, proxy: EventLoopProxy<Wake>, renderer: &eui_render::Renderer, w: f32, h: f32, scale: f32) -> Self {
        // The driver — decoding, layout, the VM — in its own confined
        // process where the platform allows (08 §10); this process keeps
        // the window, the GPU and the network. One per tab: an application
        // that dies takes its own process with it and nothing else.
        let (backend, how) = Backend::open(w, h, scale, 0);
        eprintln!("eui: {how}");
        let mut tab = Tab {
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
            ime_area: None,
            trust: crate::chrome::Trust::Unverified,
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
                tab.trust = crate::chrome::Trust::Local;
            }
            Err(e) => {
                // The tab stays, with no connection in it: in a shell the
                // tab is where a person looks for the reason, and losing
                // it would only leave a gap in the strip.
                eprintln!("eui: {e}; refusing to connect");
                return tab;
            }
        }

        let hello = tab.backend.hello();
        match transport::connect(&tab.url, hello, tab.cookie.clone(), tab.host_loopback, move || {
            let _ = proxy.send_event(Wake::Transport);
        }) {
            Ok(c) => tab.conn = Some(c),
            Err(e) => eprintln!("eui: {e}"),
        }
        tab
    }

    /// How the strip should show this tab.
    fn view(&self) -> crate::chrome::TabView<'_> {
        let (origin, path) = split_origin(&self.url);
        crate::chrome::TabView { title: &self.title, origin, path, trust: Some(self.trust) }
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
                    Incoming::Message(bytes) => frames.push(bytes),
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
            eprintln!("eui: session ended: {why}");
            self.conn = None;
        }
        if let Some(c) = self.backend.closed() {
            eprintln!("eui: closing: {c}");
            self.conn = None;
        }
        self.backend.needs_redraw()
    }

    /// Spec 03 §7: the device is open exactly while the tab has a sound
    /// loaded — nothing playing, nothing running, no wakeups. A tab keeps
    /// its sound when it goes to the back, as a browser tab does.
    fn sync_audio(&mut self, proxy: &EventLoopProxy<Wake>) {
        let wanted = self.backend.audio_playing();
        match (wanted, self.audio.is_some()) {
            (true, false) => {
                let (tx, rx) = mpsc::channel();
                let proxy = proxy.clone();
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

impl Shell {
    /// Open a window, and the applications named in `launches`.
    ///
    /// With `chrome`, the window gets a tab strip and can be given more
    /// applications later; without it, it is the one chromeless window
    /// `eui <url>` and an embedding host have always had.
    fn open(launches: Vec<Launch>, chrome: bool, event_loop: &ActiveEventLoop, proxy: EventLoopProxy<Wake>, shared: &mut Option<Shared>) -> Option<Self> {
        let title = launches.first().map_or("EUI", |l| l.title.as_str()).to_owned();
        // Born hidden, shown once the renderer exists. Two reasons: the
        // AccessKit adapter must exist before the window is first shown, and
        // macOS enforces that with a panic where AT-SPI merely tolerates it;
        // and a window shown before its first frame is a flash of nothing.
        let attrs = Window::default_attributes().with_title(title).with_visible(false).with_inner_size(winit::dpi::LogicalSize::new(960.0, 640.0));
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
        #[cfg(feature = "a11y")]
        let access = Some(accesskit_winit::Adapter::with_event_loop_proxy(event_loop, &window, proxy.clone()));

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
        let mut chrome = chrome.then(|| (crate::chrome::Chrome::new(logical_w, logical_h, scale), textures));

        let mut shell = Self {
            window,
            surface,
            config,
            tabs: Vec::new(),
            active: 0,
            modifiers: 0,
            pointer_at: None,
            pointer_in_app: false,
            proxy,
            #[cfg(feature = "a11y")]
            access,
            #[cfg(feature = "clipboard")]
            clip: None,
            cursor: eui_proto::Cursor::Default,
            theme_watch: None,
            desktop_theme: None,
            theme_pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            epoch: std::time::Instant::now(),
            chrome: chrome.take(),
        };

        let renderer = &gpu_shared.renderer;
        let (w, h) = shell.content_size();
        for l in launches {
            let tab = Tab::open(l, shell.proxy.clone(), renderer, w, h, scale);
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
        if !crate::desktop_theme::disabled() {
            shell.follow_desktop_theme();
            // One wake per burst of changes: a switch touches several files
            // and the window re-reads the theme once, when it gets to it.
            let proxy = shell.proxy.clone();
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

    /// The size an application's viewport gets, in device-independent px:
    /// the window, less whatever the chrome is holding above it.
    fn content_size(&self) -> (f32, f32) {
        let scale = self.window.scale_factor() as f32;
        let (w, h) = (self.config.width as f32 / scale, self.config.height as f32 / scale);
        let top = self.chrome.as_ref().map_or(0.0, |(c, _)| c.content_top());
        (w, (h - top).max(1.0))
    }

    /// Where an application's list starts in the window, in device pixels.
    fn content_origin(&self) -> u32 {
        let scale = self.window.scale_factor() as f32;
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
            let blank = crate::chrome::TabView { title: "New tab", origin: "", path: "", trust: None };
            chrome.rebuild(&[blank], 0);
        } else {
            chrome.rebuild(&views, self.active);
        }
        let (w, h) = self.content_size();
        let scale = self.window.scale_factor() as f32;
        if let Some(t) = self.tabs.get_mut(self.active) {
            let out = t.backend.input(Input::Resized(w, h, scale));
            t.send(out);
        }
        self.window.request_redraw();
    }

    /// Open `url` in the active tab, or in a new one if there is none.
    fn open_url(&mut self, url: String, renderer: &eui_render::Renderer) {
        let (w, h) = self.content_size();
        let scale = self.window.scale_factor() as f32;
        let launch = Launch::new(url, 0);
        let tab = Tab::open(launch, self.proxy.clone(), renderer, w, h, scale);
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
            A::Open(url) => self.open_url(url, renderer),
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

    #[cfg(feature = "clipboard")]
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
        let active = self.active;
        let mut redraw = false;
        for (i, t) in self.tabs.iter_mut().enumerate() {
            let wants = t.pump();
            redraw |= wants && i == active;
        }
        if redraw {
            self.window.request_redraw();
        }
    }

    /// An input for the active application.
    ///
    /// Pointer positions arrive in the window's coordinates and the
    /// application was laid out as though it owned the window from the top,
    /// so the chrome's height comes off here — and an input over the chrome
    /// never reaches the application at all.
    fn send_to_tab(&mut self, i: Input) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let out = t.backend.input(i);
        t.send(out);
        #[cfg(feature = "clipboard")]
        if let Some(text) = t.backend.take_clipboard() {
            if let Some(c) = self.clipboard() {
                let _ = c.set_text(text);
            }
        }
        let (area, redraw) = match self.tabs.get(self.active) {
            Some(t) => (t.backend.ime_area(), t.backend.needs_redraw()),
            None => (None, false),
        };
        self.sync_ime(area, self.chrome.as_ref().map_or(0.0, |(c, _)| c.content_top()));
        if redraw {
            self.window.request_redraw();
        }
        self.sync_cursor(false);
    }

    /// An input method is welcome exactly while a field has focus, and its
    /// candidate window sits under that field. Told only on a change: every
    /// toggle is a protocol round trip with the input method, and inputs
    /// arrive hundreds of times a second.
    fn sync_ime(&mut self, area: Option<[f32; 4]>, top: f32) {
        let had = self.tabs.get(self.active).and_then(|t| t.ime_area);
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
        if let Some(t) = self.tabs.get_mut(self.active) {
            t.ime_area = area;
        }
    }

    /// Draw the window: the chrome, then the active application over it.
    fn redraw(&mut self, renderer: &mut eui_render::Renderer) {
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

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(renderer.device(), &self.config);
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
            let proxy = self.proxy.clone();
            t.sync_audio(&proxy);
        }
        // Hover settles at paint; so does what the pointer is over.
        self.sync_cursor(false);
        // A screen reader that is listening gets the tree as painted; one
        // that is not costs nothing here.
        #[cfg(feature = "a11y")]
        if let (Some(a), Some(t)) = (&mut self.access, self.tabs.get_mut(self.active)) {
            let backend = &mut t.backend;
            a.update_if_active(|| crate::a11y::to_update(&backend.access_tree()));
        }
    }

    /// An assistive technology's request, turned into what a keyboard user
    /// could do: focus, or focus and press. It reaches the active tab only.
    #[cfg(feature = "a11y")]
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
                let out = match req.action {
                    accesskit::Action::Click => t.backend.access_action(req.target_node.0, true),
                    accesskit::Action::Focus => t.backend.access_action(req.target_node.0, false),
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
        #[cfg(feature = "a11y")]
        if let Some(a) = &mut self.access {
            a.process_event(&self.window, &event);
        }
        let scale = self.window.scale_factor() as f32;
        let top = self.chrome.as_ref().map_or(0.0, |(c, _)| c.content_top());
        match event {
            WindowEvent::CloseRequested => return false,
            WindowEvent::RedrawRequested => self.redraw(renderer),
            WindowEvent::Resized(size) => {
                self.config.width = size.width.max(1);
                self.config.height = size.height.max(1);
                self.surface.configure(renderer.device(), &self.config);
                let (w, h) = (size.width as f32 / scale, size.height as f32 / scale);
                if let Some((c, _)) = &mut self.chrome {
                    c.resized(w, h, scale);
                }
                let (cw, ch) = self.content_size();
                self.send_to_tab(Input::Resized(cw, ch, scale));
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let size = self.window.inner_size();
                let scale = scale_factor as f32;
                if let Some((c, _)) = &mut self.chrome {
                    c.resized(size.width as f32 / scale, size.height as f32 / scale, scale);
                }
                let (cw, ch) = self.content_size();
                self.send_to_tab(Input::Resized(cw, ch, scale));
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
                    self.send_to_tab(Input::PointerMove(x, y - top));
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
                    self.send_to_tab(i);
                } else {
                    return self.chrome_input(i, renderer);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                crate::driver::trace(|| format!("raw wheel {delta:?}"));
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
                #[cfg(feature = "clipboard")]
                if down && self.modifiers & 0b1010 != 0 && (name == "v" || name == "V") && !to_chrome && self.tabs.get(self.active).is_some_and(|t| t.backend.ime_area().is_some()) {
                    if let Some(text) = self.clipboard().and_then(|c| c.get_text().ok()) {
                        self.send_to_tab(Input::Paste(text));
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
            WindowEvent::ThemeChanged(t) => {
                let mode = match t {
                    winit::window::Theme::Dark => eui_proto::ThemeMode::Dark,
                    winit::window::Theme::Light => eui_proto::ThemeMode::Light,
                };
                self.send_to_tab(Input::Mode(mode));
            }
            _ => {}
        }
        true
    }

    /// Whether the keyboard belongs to the chrome: an empty tab, whose page
    /// is the chrome's own, or an address bar being edited.
    fn chrome_has_keys(&self) -> bool {
        self.showing_blank() || self.tabs.is_empty()
    }

    /// An input for the chrome, and whatever it turned out to mean.
    /// `false` when the window should close.
    fn chrome_input(&mut self, i: Input, renderer: &eui_render::Renderer) -> bool {
        let Some((c, _)) = &mut self.chrome else { return true };
        let actions = c.input(i);
        if c.needs_redraw() {
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

    /// Take this window down, in the order the platforms insist on.
    ///
    /// Not left to the drop glue: that runs in field order, which puts the
    /// window first, and both of the steps below have to happen while it is
    /// still alive.
    fn close(self, renderer: &eui_render::Renderer) {
        #[cfg(feature = "a11y")]
        let Shell { window, surface, tabs, access, .. } = self;
        #[cfg(not(feature = "a11y"))]
        let Shell { window, surface, tabs, .. } = self;
        for t in tabs {
            t.close("the window closed");
        }
        // The adapter holds the window's platform handle and talks to it as
        // it goes; on macOS a window dropped first leaves it calling into a
        // dead view.
        #[cfg(feature = "a11y")]
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
    /// and idle — it is not being clicked, EUI has no server push, and
    /// nothing it could animate is on the glass — so four open tabs cost
    /// what one does at rest.
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

/// The process: the event loop, and every window running in it.
pub struct App {
    proxy: EventLoopProxy<Wake>,
    /// The GPU, made by the first window to open and used by every one
    /// after it. `None` until then, and on a machine with no adapter.
    shared: Option<Shared>,
    /// Windows asked for and not yet opened. `resumed` drains it; on the
    /// platforms that suspend and resume, a window already open is not
    /// opened twice.
    pending: Vec<(Vec<Launch>, bool)>,
    shells: std::collections::HashMap<WindowId, Shell>,
}

impl App {
    /// Build for the applications to open when the loop resumes: one
    /// chromeless window each.
    pub fn new(launches: Vec<Launch>, proxy: EventLoopProxy<Wake>) -> Self {
        Self { proxy, shared: None, pending: launches.into_iter().map(|l| (vec![l], false)).collect(), shells: std::collections::HashMap::new() }
    }

    /// Build for one window with a tab strip in it, and nothing open.
    pub fn shell(proxy: EventLoopProxy<Wake>) -> Self {
        Self { proxy, shared: None, pending: vec![(Vec::new(), true)], shells: std::collections::HashMap::new() }
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
        for (launches, chrome) in std::mem::take(&mut self.pending) {
            match Shell::open(launches, chrome, event_loop, self.proxy.clone(), &mut self.shared) {
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

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        match event {
            // Which window the transport, the audio thread or the desktop
            // meant is not in the wake, and asking each is a `try_recv` on
            // an empty channel — cheaper than carrying an id would be.
            Wake::Transport => self.shells.values_mut().for_each(Shell::pump),
            Wake::Audio => self.shells.values_mut().for_each(|s| s.tabs.iter_mut().for_each(Tab::drain_audio)),
            Wake::Theme => self.shells.values_mut().for_each(Shell::theme_wake),
            Wake::Exit => event_loop.exit(),
            #[cfg(feature = "a11y")]
            Wake::Access(e) => {
                if let Some(s) = self.shells.get_mut(&e.window_id) {
                    s.access_event(e);
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(shared) = self.shared.as_mut() else { return };
        let Some(shell) = self.shells.get_mut(&id) else { return };
        if !shell.event(&mut shared.renderer, event) {
            self.close(event_loop, id);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = std::time::Instant::now();
        // The earliest instant any window asked for. One that wants to
        // sleep does not hold the others back, and one that wants a frame
        // does not let them park.
        let due = self.shells.values_mut().filter_map(|s| s.park(now)).min();
        event_loop.set_control_flow(match due {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }
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

/// The event loop, whatever is going to run in it. Must be called on the
/// main thread.
fn run_loop(build: impl FnOnce(EventLoopProxy<Wake>) -> App) -> Result<(), String> {
    let event_loop = EventLoop::<Wake>::with_user_event().build().map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    let mut app = build(proxy.clone());
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
    use super::*;

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
