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

/// The application.
pub struct App {
    url: String,
    title: String,
    /// Capabilities the person allows, if the manifest asks for them.
    allowed: u32,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
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
}

impl App {
    /// Build for a session URL.
    pub fn new(launch: Launch, proxy: EventLoopProxy<Wake>) -> Self {
        if launch.host_loopback {
            transport::allow_host_loopback();
        }
        transport::set_session_cookie(launch.cookie);
        Self {
            url: launch.url,
            title: launch.title,
            allowed: launch.allowed,
            window: None,
            gpu: None,
            backend: Backend::local(crate::driver::Driver::new(960.0, 640.0, 1.0, 0)),
            conn: None,
            modifiers: 0,
            proxy,
            #[cfg(feature = "a11y")]
            access: None,
            #[cfg(feature = "clipboard")]
            clip: None,
            ime_area: None,
            cursor: eui_proto::Cursor::Default,
            theme_watch: None,
            desktop_theme: None,
            theme_pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            audio: None,
            audio_rx: None,
        }
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
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    /// The pointer takes the shape of what it is over — a hand on a button,
    /// a beam on a field — told to the window only on a change.
    fn sync_cursor(&mut self) {
        let want = self.backend.cursor();
        if want == self.cursor {
            return;
        }
        self.cursor = want;
        if let Some(w) = &self.window {
            use eui_proto::Cursor as C;
            use winit::window::CursorIcon as I;
            w.set_cursor(match want {
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
            if let Some(w) = &self.window {
                w.request_redraw();
            }
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
        if let Some(w) = &self.window {
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

    fn redraw(&mut self) {
        let Some(gpu) = &mut self.gpu else { return };
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
                gpu.surface.configure(gpu.renderer.device(), &gpu.config);
                return;
            }
            Err(e) => {
                eprintln!("eui: surface: {e}");
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        self.backend.with_atlases(|atlas, images| gpu.renderer.render(&view, (w, h), &list, atlas, images));
        frame.present();
        crate::driver::trace(|| format!("frame: layout+paint {:.1} ms, render+present {:.1} ms, {} quads", painted.as_secs_f64() * 1e3, t0.elapsed().as_secs_f64() * 1e3 - painted.as_secs_f64() * 1e3, list.quads.len()));
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
                    if let Some(w) = &self.window {
                        w.request_redraw();
                    }
                }
            }
            A::AccessibilityDeactivated => {}
        }
    }
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        event_loop.set_control_flow(ControlFlow::Wait);
        let attrs = Window::default_attributes().with_title(self.title.clone()).with_inner_size(winit::dpi::LogicalSize::new(960.0, 640.0));
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
                event_loop.exit();
                return;
            }
        };

        // Assistive technologies register before the window shows; the tree
        // itself is built only if one asks.
        #[cfg(feature = "a11y")]
        {
            self.access = Some(accesskit_winit::Adapter::with_event_loop_proxy(event_loop, &window, self.proxy.clone()));
        }

        // Vulkan, Metal or DX12 — never GL: on Linux a GL instance loads
        // Mesa's gallium and its LLVM (34 MB of the window's 64 MB PSS,
        // measured), for a backend the primary ones make unneeded.
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..Default::default() });
        let surface = match instance.create_surface(Arc::clone(&window)) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("eui: cannot create a surface: {e}");
                event_loop.exit();
                return;
            }
        };
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }));
        let Some(adapter) = adapter else {
            eprintln!("eui: no GPU adapter");
            event_loop.exit();
            return;
        };
        let renderer = match eui_render::Renderer::with_adapter(&adapter) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("eui: {e}");
                event_loop.exit();
                return;
            }
        };
        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: eui_render::FORMAT,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(renderer.device(), &config);
        let scale = window.scale_factor() as f32;
        // The driver — decoding, layout, the VM — in its own confined
        // process where the platform allows (08 §10); this process keeps
        // the window, the GPU and the network.
        let (backend, how) = Backend::open(size.width as f32 / scale, size.height as f32 / scale, scale, 0);
        eprintln!("eui: {how}");
        self.backend = backend;
        // The desktop's own colours, before the first frame; and again
        // whenever the desktop changes them.
        if !crate::desktop_theme::disabled() {
            self.follow_desktop_theme();
            // One wake per burst of changes: a switch touches several files
            // and the window re-reads the theme once, when it gets to it.
            let proxy = self.proxy.clone();
            let pending = Arc::clone(&self.theme_pending);
            self.theme_watch = crate::desktop_theme::watch(move || {
                if !pending.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    let _ = proxy.send_event(Wake::Theme);
                }
            });
        }
        self.gpu = Some(Gpu { surface, config, renderer });
        self.window = Some(window);

        // Spec 01 §2.1: the manifest first. Its signature is verified and
        // its key pinned before a byte of the session is trusted; only the
        // debug loopback of 08 §1 may go on without one.
        match crate::assets::origin_for(&self.url).map_err(|e| e.to_string()).and_then(|origin| {
            let pins = crate::manifest::pins_dir().ok_or_else(|| "no home directory for the pin store".to_string())?;
            crate::manifest::check(&origin, &pins).map_err(|e| e.to_string())
        }) {
            Ok(m) => {
                let granted = m.capabilities & self.allowed;
                let refused = m.capabilities & !self.allowed;
                eprintln!("eui: {} {} — publisher key pinned; granted [{}], refused [{}]", m.name, m.version, eui_proto::caps::names(granted).join(", "), eui_proto::caps::names(refused).join(", "));
                self.backend.grant(granted);
            }
            Err(e) if self.url.starts_with("ws://") => eprintln!("eui: {e}; continuing on the debug loopback without a manifest"),
            Err(e) => {
                eprintln!("eui: {e}; refusing to connect");
                event_loop.exit();
                return;
            }
        }
        let hello = self.backend.hello();
        let proxy = self.proxy.clone();
        match transport::connect(&self.url, hello, move || {
            let _ = proxy.send_event(Wake::Transport);
        }) {
            Ok(c) => self.conn = Some(c),
            Err(e) => eprintln!("eui: {e}"),
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: Wake) {
        match event {
            Wake::Transport => self.pump(),
            Wake::Exit => _event_loop.exit(),
            Wake::Audio => self.drain_audio(),
            Wake::Theme => {
                crate::driver::trace(|| "desktop theme wake".into());
                self.theme_pending.store(false, std::sync::atomic::Ordering::SeqCst);
                self.follow_desktop_theme();
            }
            #[cfg(feature = "a11y")]
            Wake::Access(e) => self.access_event(e),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        #[cfg(feature = "a11y")]
        if let (Some(a), Some(w)) = (&mut self.access, &self.window) {
            a.process_event(w, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::Resized(size) => {
                let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor() as f32);
                if let Some(gpu) = &mut self.gpu {
                    gpu.config.width = size.width.max(1);
                    gpu.config.height = size.height.max(1);
                    gpu.surface.configure(gpu.renderer.device(), &gpu.config);
                }
                self.input(Input::Resized(size.width as f32 / scale, size.height as f32 / scale, scale));
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let size = self.window.as_ref().map(|w| w.inner_size()).unwrap_or_default();
                let scale = scale_factor as f32;
                self.input(Input::Resized(size.width as f32 / scale, size.height as f32 / scale, scale));
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor() as f32);
                self.input(Input::PointerMove(position.x as f32 / scale, position.y as f32 / scale));
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    _ => return,
                };
                self.input(if state == ElementState::Pressed { Input::PointerDown(b) } else { Input::PointerUp(b) });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                crate::driver::trace(|| format!("raw wheel {delta:?}"));
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => self.input(Input::WheelStep(-x, -y)),
                    MouseScrollDelta::PixelDelta(p) => {
                        let scale = self.window.as_ref().map_or(1.0, |w| w.scale_factor() as f32);
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
                    _ => return,
                };
                if down && self.modifiers & 0b1110 == 0 {
                    if let Some(text) = &event.text {
                        if !matches!(event.logical_key, Key::Named(_)) {
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
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        crate::driver::trace(|| format!("about_to_wait: due={:?}", self.backend.next_frame_at().map(|d| d.saturating_duration_since(std::time::Instant::now()))));
        // A running transition is the only thing that ever wakes the loop by
        // itself; at rest `ControlFlow::Wait` sleeps until the OS or the
        // transport speaks.
        if self.backend.tick(std::time::Instant::now()) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        // A frame already due has its redraw requested above; waiting on
        // an instant in the past would spin until the compositor delivers
        // it — and a hidden window's it may never come.
        let now = std::time::Instant::now();
        event_loop.set_control_flow(match self.backend.next_frame_at() {
            Some(at) if at > now => ControlFlow::WaitUntil(at),
            _ => ControlFlow::Wait,
        });
    }
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

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
    let event_loop = EventLoop::<Wake>::with_user_event().build().map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    let mut app = App::new(launch, proxy.clone());
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
    // The worker and the GPU go here, on this thread, before anyone exits.
    drop(app);
    result
}
