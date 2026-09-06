//! The window: winit in `ControlFlow::Wait`, a wgpu surface, and the driver.
//!
//! There is no render loop. The window redraws when a frame arrived, the
//! viewer did something, or the OS asked — and at no other time. That is
//! the whole of the zero-wakeup idle budget.

use std::sync::Arc;

use eui_proto::Frame;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use crate::driver::{Driver, Input};
use crate::transport::{self, Connection, Incoming};

/// Woken by the transport thread when a message is waiting.
#[derive(Debug)]
pub struct Wake;

struct Gpu {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: eui_render::Renderer,
}

/// The application.
pub struct App {
    url: String,
    window: Option<Arc<Window>>,
    gpu: Option<Gpu>,
    driver: Driver,
    conn: Option<Connection>,
    modifiers: u32,
    proxy: EventLoopProxy<Wake>,
}

impl App {
    /// Build for a session URL.
    pub fn new(url: String, proxy: EventLoopProxy<Wake>) -> Self {
        Self { url, window: None, gpu: None, driver: Driver::new(960.0, 640.0, 1.0, 0), conn: None, modifiers: 0, proxy }
    }

    fn send(&mut self, frames: Vec<Frame>) {
        let Some(conn) = &self.conn else { return };
        for f in frames {
            if conn.tx.send(f.encode()).is_err() {
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
                    Incoming::Message(bytes) => match Frame::decode(&bytes) {
                        Ok(f) => frames.push(f),
                        Err(e) => {
                            closed = Some(format!("bad frame: {e}"));
                            break;
                        }
                    },
                    Incoming::Closed(e) => {
                        closed = Some(e.to_string());
                        break;
                    }
                }
            }
        }
        for f in frames {
            let out = self.driver.handle_frame(f);
            self.send(out);
        }
        if let Some(why) = closed {
            eprintln!("eui: session ended: {why}");
            self.conn = None;
        }
        if let Some(c) = self.driver.closed() {
            eprintln!("eui: closing: {c:?}");
            self.conn = None;
        }
        if self.driver.needs_redraw() {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    fn input(&mut self, i: Input) {
        let out = self.driver.input(i);
        self.send(out);
        if self.driver.needs_redraw() {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
    }

    fn redraw(&mut self) {
        let Some(gpu) = &mut self.gpu else { return };
        let (w, h) = (gpu.config.width, gpu.config.height);
        if w == 0 || h == 0 {
            return;
        }
        let list = self.driver.paint(w, h);
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
        gpu.renderer.render(&view, (w, h), &list, self.driver.atlas_mut());
        frame.present();
    }
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        event_loop.set_control_flow(ControlFlow::Wait);
        let attrs = Window::default_attributes().with_title("EUI").with_inner_size(winit::dpi::LogicalSize::new(960.0, 640.0));
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                eprintln!("eui: cannot create a window: {e}");
                event_loop.exit();
                return;
            }
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::all(), ..Default::default() });
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
        self.driver = Driver::new(size.width as f32 / scale, size.height as f32 / scale, scale, 0);
        self.gpu = Some(Gpu { surface, config, renderer });
        self.window = Some(window);

        let hello = self.driver.hello().encode();
        let proxy = self.proxy.clone();
        match transport::connect(&self.url, hello, move || {
            let _ = proxy.send_event(Wake);
        }) {
            Ok(c) => self.conn = Some(c),
            Err(e) => eprintln!("eui: {e}"),
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: Wake) {
        self.pump();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (-x * 40.0, -y * 40.0),
                    MouseScrollDelta::PixelDelta(p) => (-p.x as f32, -p.y as f32),
                };
                self.input(Input::Wheel(dx, dy));
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
                self.input(Input::Key { key: name, modifiers: self.modifiers, down });
            }
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

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Nothing scheduled: `ControlFlow::Wait` sleeps until the OS or the
        // transport wakes us.
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

/// Run the client until the window closes.
pub fn run(url: String) -> Result<(), String> {
    let event_loop = EventLoop::<Wake>::with_user_event().build().map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    let mut app = App::new(url, proxy);
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}
