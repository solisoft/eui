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

struct Gpu {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// The glyph, image and blur textures for this window. Separate from
    /// the renderer because the renderer is what the windows share and
    /// these are what they must not.
    textures: eui_render::SessionTextures,
}

/// The GPU, once, for every window in the process.
///
/// A second window costs no adapter, no device, no pipelines and no naga
/// output — which is most of what makes the first window's first pixel
/// expensive. What it does cost is its own surface and its own textures,
/// both of which are in `Gpu` above.
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

/// One application, in its own window.
///
/// Everything here is per session and stays per session however many share
/// a process: the window and its surface, the confined worker the tree
/// lives in, the connection and its cookie, the textures, the audio device.
/// What several sessions can share sits on [`App`], above them.
struct Session {
    url: String,
    /// Capabilities the person allows, if the manifest asks for them.
    allowed: u32,
    window: Arc<Window>,
    gpu: Gpu,
    /// The driver: in a worker process when one could be started.
    backend: Backend,
    conn: Option<Connection>,
    modifiers: u32,
    proxy: EventLoopProxy<Wake>,
    #[cfg(feature = "a11y")]
    access: Option<accesskit_winit::Adapter>,
    #[cfg(feature = "clipboard")]
    clip: Option<arboard::Clipboard>,
    /// The field the input method was last pointed at, if any.
    ime_area: Option<[f32; 4]>,
    /// The pointer shape last handed to the window.
    cursor: eui_proto::Cursor,
    /// The desktop theme watcher, alive as long as the window.
    theme_watch: Option<Box<dyn std::any::Any + Send>>,
    /// The desktop palette last applied.
    desktop_theme: Option<crate::desktop_theme::DesktopTheme>,
    /// A theme wake is queued and not yet handled.
    theme_pending: Arc<std::sync::atomic::AtomicBool>,
    /// The audio device, open only while something is loaded (03 §7).
    audio: Option<crate::audio::Output>,
    /// Frames the audio thread produced, for this loop to send.
    audio_rx: Option<mpsc::Receiver<Vec<u8>>>,
    /// The cookie this session presents, if a host set one. Held here
    /// rather than in a process global: two sessions in one process must
    /// not present each other's.
    cookie: Option<String>,
    /// This session's server is embedded in this process, so `ws://` on
    /// loopback is trusted (08 §1). Per session, not per process: an
    /// embedded session must not vouch for the network sessions beside it.
    host_loopback: bool,
    /// When this window started, so the renderer can be handed a monotonic
    /// clock in seconds. `spin` and the backdrop want elapsed time, not a
    /// wall clock, and the driver's own epoch is in the worker process.
    epoch: std::time::Instant,
}

impl Session {
    /// Open a window on the session `launch` describes: the window, the
    /// GPU, the worker, the manifest check and the connection.
    ///
    /// `None` if the platform could not give a window, a surface or an
    /// adapter, or if the manifest refused the origin. The caller decides
    /// what that means — for the only session it means give up, for the
    /// second of several it means carry on without it.
    fn open(launch: Launch, event_loop: &ActiveEventLoop, proxy: EventLoopProxy<Wake>, shared: &mut Option<Shared>) -> Option<Self> {
        // Born hidden, shown once the renderer exists. Two reasons: the
        // AccessKit adapter must exist before the window is first shown, and
        // macOS enforces that with a panic where AT-SPI merely tolerates it;
        // and a window shown before its first frame is a flash of nothing.
        let attrs = Window::default_attributes().with_title(launch.title.clone()).with_visible(false).with_inner_size(winit::dpi::LogicalSize::new(960.0, 640.0));
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
        // The driver — decoding, layout, the VM — in its own confined
        // process where the platform allows (08 §10); this process keeps
        // the window, the GPU and the network.
        let (backend, how) = Backend::open(size.width as f32 / scale, size.height as f32 / scale, scale, 0);
        eprintln!("eui: {how}");
        let textures = gpu_shared.renderer.session();
        let gpu = Gpu { surface, config, textures };
        // Everything the first frame needs is in place, and any assistive
        // technology has already registered: it is safe to be seen.
        //
        // Ask for that first frame explicitly. A window that was visible at
        // creation is told to redraw as it maps; one shown later is not, and
        // on Wayland it simply maps blank and stays blank until some
        // unrelated event happens to ask for a frame.
        window.set_visible(true);
        window.request_redraw();

        let mut s = Self {
            url: launch.url,
            allowed: launch.allowed,
            window,
            gpu,
            backend,
            conn: None,
            modifiers: 0,
            proxy,
            #[cfg(feature = "a11y")]
            access,
            #[cfg(feature = "clipboard")]
            clip: None,
            ime_area: None,
            cursor: eui_proto::Cursor::Default,
            theme_watch: None,
            desktop_theme: None,
            theme_pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            audio: None,
            audio_rx: None,
            cookie: launch.cookie,
            host_loopback: launch.host_loopback,
            epoch: std::time::Instant::now(),
        };

        // The desktop's own colours, before the first frame; and again
        // whenever the desktop changes them.
        if !crate::desktop_theme::disabled() {
            s.follow_desktop_theme();
            // One wake per burst of changes: a switch touches several files
            // and the window re-reads the theme once, when it gets to it.
            let proxy = s.proxy.clone();
            let pending = Arc::clone(&s.theme_pending);
            s.theme_watch = crate::desktop_theme::watch(move || {
                if !pending.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    let _ = proxy.send_event(Wake::Theme);
                }
            });
        }

        // Spec 01 §2.1: the manifest first. Its signature is verified and
        // its key pinned before a byte of the session is trusted; only the
        // debug loopback of 08 §1 may go on without one.
        match crate::assets::origin_for(&s.url).map_err(|e| e.to_string()).and_then(|origin| {
            let pins = crate::manifest::pins_dir().ok_or_else(|| "no home directory for the pin store".to_string())?;
            crate::manifest::check(&origin, &pins, s.cookie.as_deref()).map_err(|e| e.to_string())
        }) {
            Ok(m) => {
                let granted = m.capabilities & s.allowed;
                let refused = m.capabilities & !s.allowed;
                eprintln!("eui: {} {} — publisher key pinned; granted [{}], refused [{}]", m.name, m.version, eui_proto::caps::names(granted).join(", "), eui_proto::caps::names(refused).join(", "));
                s.backend.grant(granted);
            }
            Err(e) if s.url.starts_with("ws://") => {
                eprintln!("eui: {e}; continuing on the debug loopback without a manifest")
            }
            Err(e) => {
                eprintln!("eui: {e}; refusing to connect");
                return None;
            }
        }
        let hello = s.backend.hello();
        let proxy = s.proxy.clone();
        match transport::connect(&s.url, hello, s.cookie.clone(), s.host_loopback, move || {
            let _ = proxy.send_event(Wake::Transport);
        }) {
            Ok(c) => s.conn = Some(c),
            Err(e) => eprintln!("eui: {e}"),
        }
        Some(s)
    }

    /// Follow the desktop's palette (05 §5): read it, hand it to the driver
    /// if it changed, and say so once.
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
        let out = self.backend.desktop_theme(mode, colors);
        self.send(out);
        self.window.request_redraw();
    }

    /// The pointer takes the shape of what it is over — a hand on a button,
    /// a beam on a field — told to the window only on a change.
    fn sync_cursor(&mut self) {
        let want = self.backend.cursor();
        if want == self.cursor {
            return;
        }
        self.cursor = want;
        {
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

    fn pump(&mut self) {
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
        if self.backend.needs_redraw() {
            self.window.request_redraw();
        }
    }

    #[cfg(feature = "clipboard")]
    fn clipboard(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.clip.is_none() {
            self.clip = arboard::Clipboard::new().ok();
        }
        self.clip.as_mut()
    }

    fn input(&mut self, i: Input) {
        let out = self.backend.input(i);
        self.send(out);
        #[cfg(feature = "clipboard")]
        if let Some(text) = self.backend.take_clipboard() {
            if let Some(c) = self.clipboard() {
                let _ = c.set_text(text);
            }
        }
        {
            let w = &self.window;
            // An input method is welcome exactly while a field has focus,
            // and its candidate window sits under that field. Told only on
            // a change: every toggle is a protocol round trip with the
            // input method, and inputs arrive hundreds of times a second.
            let area = self.backend.ime_area();
            if area != self.ime_area {
                match area {
                    Some([x, y, wd, h]) => {
                        if self.ime_area.is_none() {
                            w.set_ime_allowed(true);
                        }
                        w.set_ime_cursor_area(LogicalPosition::new(x, y), LogicalSize::new(wd, h));
                    }
                    None => w.set_ime_allowed(false),
                }
                self.ime_area = area;
            }
            if self.backend.needs_redraw() {
                w.request_redraw();
            }
        }
        self.sync_cursor();
    }

    /// Spec 03 §7: the device is open exactly while the session has a
    /// sound loaded — nothing playing, nothing running, no wakeups.
    fn sync_audio(&mut self) {
        let wanted = self.backend.audio_playing();
        match (wanted, self.audio.is_some()) {
            (true, false) => {
                let (tx, rx) = mpsc::channel();
                let proxy = self.proxy.clone();
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

    fn redraw(&mut self, renderer: &mut eui_render::Renderer) {
        let gpu = &mut self.gpu;
        let (w, h) = (gpu.config.width, gpu.config.height);
        if w == 0 || h == 0 {
            return;
        }
        let t0 = std::time::Instant::now();
        let (list, landed) = self.backend.paint(w, h);
        let painted = t0.elapsed();
        let frame = match gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                gpu.surface.configure(renderer.device(), &gpu.config);
                return;
            }
            Err(e) => {
                eprintln!("eui: surface: {e}");
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let format = gpu.config.format;
        let now = self.epoch.elapsed().as_secs_f32();
        let target = eui_render::Target { view: &view, format, size: (w, h), now };
        let textures = &mut gpu.textures;
        self.backend.with_atlases(|atlas, images| renderer.render(textures, target, &list, atlas, images));
        frame.present();
        crate::driver::trace(|| {
            format!("frame: layout+paint {:.1} ms, render+present {:.1} ms, {} quads", painted.as_secs_f64() * 1e3, t0.elapsed().as_secs_f64() * 1e3 - painted.as_secs_f64() * 1e3, list.quads.len())
        });
        // A scroll that landed during this paint reports its offset now.
        self.send(landed);
        // Hover settles at paint; so does what the pointer is over.
        self.sync_cursor();
        // A batch may have added a sound, or taken the last one away.
        self.sync_audio();
        // A screen reader that is listening gets the tree as painted; one
        // that is not costs nothing here.
        #[cfg(feature = "a11y")]
        if let Some(a) = &mut self.access {
            let backend = &mut self.backend;
            a.update_if_active(|| crate::a11y::to_update(&backend.access_tree()));
        }
    }

    /// An assistive technology's request, turned into what a keyboard user
    /// could do: focus, or focus and press.
    #[cfg(feature = "a11y")]
    fn access_event(&mut self, event: accesskit_winit::Event) {
        use accesskit_winit::WindowEvent as A;
        match event.window_event {
            A::InitialTreeRequested => {
                if let Some(a) = &mut self.access {
                    let backend = &mut self.backend;
                    a.update_if_active(|| crate::a11y::to_update(&backend.access_tree()));
                }
            }
            A::ActionRequested(req) => {
                let out = match req.action {
                    accesskit::Action::Click => self.backend.access_action(req.target_node.0, true),
                    accesskit::Action::Focus => self.backend.access_action(req.target_node.0, false),
                    _ => Vec::new(),
                };
                self.send(out);
                if self.backend.needs_redraw() {
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

    /// One event for this session's window. `false` when the window should
    /// close — which for the last session means the process is done.
    fn event(&mut self, renderer: &mut eui_render::Renderer, event: WindowEvent) -> bool {
        #[cfg(feature = "a11y")]
        if let Some(a) = &mut self.access {
            a.process_event(&self.window, &event);
        }
        match event {
            WindowEvent::CloseRequested => return false,
            WindowEvent::RedrawRequested => self.redraw(renderer),
            WindowEvent::Resized(size) => {
                let scale = self.window.scale_factor() as f32;
                let gpu = &mut self.gpu;
                gpu.config.width = size.width.max(1);
                gpu.config.height = size.height.max(1);
                gpu.surface.configure(renderer.device(), &gpu.config);
                self.input(Input::Resized(size.width as f32 / scale, size.height as f32 / scale, scale));
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let size = self.window.inner_size();
                let scale = scale_factor as f32;
                self.input(Input::Resized(size.width as f32 / scale, size.height as f32 / scale, scale));
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.scale_factor() as f32;
                self.input(Input::PointerMove(position.x as f32 / scale, position.y as f32 / scale));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    _ => return true,
                };
                self.input(if state == ElementState::Pressed { Input::PointerDown(b) } else { Input::PointerUp(b) });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                crate::driver::trace(|| format!("raw wheel {delta:?}"));
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => self.input(Input::WheelStep(-x, -y)),
                    MouseScrollDelta::PixelDelta(p) => {
                        let scale = self.window.scale_factor() as f32;
                        self.input(Input::Wheel(-p.x as f32 / scale, -p.y as f32 / scale));
                    }
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
                if down && self.modifiers & 0b1110 == 0 {
                    if let Some(text) = &event.text {
                        if types_text(&event.logical_key) {
                            self.input(Input::Text(text.to_string()));
                        }
                    }
                }
                // Ctrl+V / ⌘V: the person's own clipboard into the field they
                // are editing. The window reads it; the driver never can.
                #[cfg(feature = "clipboard")]
                if down && self.modifiers & 0b1010 != 0 && (name == "v" || name == "V") && self.backend.ime_area().is_some() {
                    if let Some(text) = self.clipboard().and_then(|c| c.get_text().ok()) {
                        self.input(Input::Paste(text));
                    }
                }
                self.input(Input::Key { key: name, modifiers: self.modifiers, down });
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => self.input(Input::ImePreedit(text)),
            WindowEvent::Ime(Ime::Commit(text)) => self.input(Input::ImeCommit(text)),
            WindowEvent::CursorLeft { .. } => self.input(Input::PointerOut),
            WindowEvent::Focused(false) => self.input(Input::Unfocused),
            WindowEvent::ThemeChanged(t) => {
                let mode = match t {
                    winit::window::Theme::Dark => eui_proto::ThemeMode::Dark,
                    winit::window::Theme::Light => eui_proto::ThemeMode::Light,
                };
                self.input(Input::Mode(mode));
            }
            _ => {}
        }
        true
    }

    /// Take this session down, in the order the platforms insist on.
    ///
    /// Not left to the drop glue: that runs in field order, which puts the
    /// window first, and both of the steps below have to happen while it is
    /// still alive.
    fn close(self, renderer: &eui_render::Renderer) {
        #[cfg(feature = "a11y")]
        let Session { window, gpu, access, .. } = self;
        #[cfg(not(feature = "a11y"))]
        let Session { window, gpu, .. } = self;
        // The adapter holds the window's platform handle and talks to it as
        // it goes; on macOS a window dropped first leaves it calling into a
        // dead view.
        #[cfg(feature = "a11y")]
        drop(access);
        // Dropping a surface with a frame still in flight is the classic
        // hang. Wait for the device to go idle, let the surface go, and only
        // then the window it was made from.
        renderer.device().poll(wgpu::Maintain::Wait);
        drop(gpu);
        drop(window);
    }

    /// What this session wants of the loop before it parks: `None` to sleep
    /// until something happens, or the instant it wants to be woken at.
    /// The loop takes the earliest across every session.
    fn park(&mut self, now: std::time::Instant) -> Option<std::time::Instant> {
        crate::driver::trace(|| format!("about_to_wait: due={:?}", self.backend.next_frame_at().map(|d| d.saturating_duration_since(std::time::Instant::now()))));
        // A running transition is the only thing that ever wakes the loop by
        // itself; at rest `ControlFlow::Wait` sleeps until the OS or the
        // transport speaks.
        let mut requested = false;
        if self.backend.tick(now) {
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
        match self.backend.next_frame_at() {
            _ if requested => None,
            Some(at) if at > now => Some(at),
            Some(_) => Some(now + std::time::Duration::from_millis(1)),
            None => None,
        }
    }
}

/// The process: the event loop, and every session running in it.
///
/// One window each, and — for now — one GPU device each. What the sessions
/// share at this point is the loop and the process; step by step more of
/// the device side moves up here.
pub struct App {
    proxy: EventLoopProxy<Wake>,
    /// The GPU, made by the first session to open and used by every one
    /// after it. `None` until then, and on a machine with no adapter.
    shared: Option<Shared>,
    /// Sessions asked for and not yet opened. `resumed` drains it; on the
    /// platforms that suspend and resume, a session already open is not
    /// opened twice.
    pending: Vec<Launch>,
    sessions: std::collections::HashMap<WindowId, Session>,
}

impl App {
    /// Build for the sessions to open when the loop resumes.
    pub fn new(launches: Vec<Launch>, proxy: EventLoopProxy<Wake>) -> Self {
        Self { proxy, shared: None, pending: launches, sessions: std::collections::HashMap::new() }
    }

    /// One window closed. The last one takes the process with it: a client
    /// with no window is not something a person can get back to.
    fn close(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        if let (Some(s), Some(g)) = (self.sessions.remove(&id), self.shared.as_ref()) {
            s.close(&g.renderer);
        }
        if self.sessions.is_empty() {
            event_loop.exit();
        }
    }

    /// Every session down, in order, on this thread — before anything in
    /// the process exits under a live GPU device.
    fn shutdown(&mut self) {
        if let Some(g) = &self.shared {
            for (_, s) in self.sessions.drain() {
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
        for launch in std::mem::take(&mut self.pending) {
            match Session::open(launch, event_loop, self.proxy.clone(), &mut self.shared) {
                Some(s) => {
                    self.sessions.insert(s.window.id(), s);
                }
                // The window, the adapter or the manifest said no. With
                // nothing else running there is nothing left to do.
                None if self.sessions.is_empty() => {
                    event_loop.exit();
                    return;
                }
                None => {}
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Wake) {
        match event {
            // Which session the transport, the audio thread or the desktop
            // meant is not in the wake, and asking each is a `try_recv` on
            // an empty channel — cheaper than carrying an id would be.
            Wake::Transport => self.sessions.values_mut().for_each(Session::pump),
            Wake::Audio => self.sessions.values_mut().for_each(Session::drain_audio),
            Wake::Theme => self.sessions.values_mut().for_each(Session::theme_wake),
            Wake::Exit => event_loop.exit(),
            #[cfg(feature = "a11y")]
            Wake::Access(e) => {
                if let Some(s) = self.sessions.get_mut(&e.window_id) {
                    s.access_event(e);
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(shared) = self.shared.as_mut() else { return };
        let Some(session) = self.sessions.get_mut(&id) else { return };
        if !session.event(&mut shared.renderer, event) {
            self.close(event_loop, id);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = std::time::Instant::now();
        // The earliest instant any session asked for. A session that wants
        // to sleep does not hold the others back, and one that wants a
        // frame does not let them park.
        let due = self.sessions.values_mut().filter_map(|s| s.park(now)).min();
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
    let event_loop = EventLoop::<Wake>::with_user_event().build().map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    let mut app = App::new(launches, proxy.clone());
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
