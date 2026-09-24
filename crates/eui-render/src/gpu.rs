//! The wgpu side: a pipeline per target format, one instance buffer, one
//! atlas texture.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use crate::atlas::{Atlas, ImageAtlas};
use crate::paint::{Backdrop, DrawList, Quad, MAX_SCROLLERS, MAX_XFORMS};
use crate::scene;

/// Why the renderer could not start or draw.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RenderError {
    /// No GPU adapter at all — not even a software one.
    NoAdapter,
    /// The adapter refused a device.
    Device(String),
    /// A read-back failed.
    ReadBack(String),
    /// The device went away under us — a driver reset, a TDR, an eviction.
    /// Nothing built on it is valid any more.
    DeviceLost(String),
    /// An error no scope caught. wgpu's default for this is a panic, which
    /// in the window process would take the session's whole display with
    /// it; the renderer records it instead and lets the caller end the
    /// session with a reason, the way a dead worker already does (08 §10).
    Uncaptured(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter => f.write_str("no GPU adapter available"),
            Self::Device(e) => write!(f, "device request failed: {e}"),
            Self::ReadBack(e) => write!(f, "read-back failed: {e}"),
            Self::DeviceLost(e) => write!(f, "GPU device lost: {e}"),
            Self::Uncaptured(e) => write!(f, "GPU error: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

/// What went wrong on the device out of band — a lost device, or an error
/// no scope was open for.
///
/// wgpu delivers both through callbacks that run on whatever thread the
/// driver answers on, so this is the one piece of renderer state that is
/// shared and locked. The window reads it between frames: the first
/// trouble is kept and later ones are dropped, because the first is the
/// one that explains the rest.
#[derive(Debug, Clone, Default)]
pub struct Trouble(Arc<Mutex<Option<RenderError>>>);

impl Trouble {
    /// Record `e`, unless something is already recorded.
    fn set(&self, e: RenderError) {
        if let Ok(mut slot) = self.0.lock() {
            if slot.is_none() {
                *slot = Some(e);
            }
        }
    }

    /// What went wrong, if anything has.
    #[must_use]
    pub fn get(&self) -> Option<RenderError> {
        match self.0.lock() {
            Ok(slot) => slot.clone(),
            // A panic while holding this lock is itself a failure worth
            // reporting, and reporting it beats hiding it behind `None`.
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

/// The off-screen format, and what `read_back` hands back: sRGB, RGBA byte
/// order, which is what a PNG wants.
///
/// A window's format is **not** this one. It is whatever its surface offers,
/// and a surface is entitled to offer none of it: Metal advertises only BGRA
/// and the float formats, so asking for this one there fails `configure` and
/// takes the process with it. The window picks from `get_capabilities` and
/// tells `render` what it picked.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// GPU state that lives for the session.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    /// One per format drawn into. A session uses one or two — a window's
    /// surface format, and `FORMAT` if it also renders off-screen.
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    uniforms: wgpu::Buffer,
    uniform_bind: wgpu::BindGroup,
    atlas_layout: wgpu::BindGroupLayout,
    /// Everything the backdrop blur needs. It is built once and then sits
    /// idle: a frame with no `blur` in it touches none of this, and the
    /// off-screen textures below are not allocated until one does.
    blur_shader: wgpu::ShaderModule,
    blur_params_layout: wgpu::BindGroupLayout,
    blur_src_layout: wgpu::BindGroupLayout,
    /// Group 2 of the quad pipeline: the finished blur a run samples.
    blur_out_layout: wgpu::BindGroupLayout,
    blur_pipeline_layout: wgpu::PipelineLayout,
    /// `(reduce, gauss)` per format, as `pipelines` above.
    blur_pipelines: HashMap<wgpu::TextureFormat, (wgpu::RenderPipeline, wgpu::RenderPipeline)>,
    blur_params: wgpu::Buffer,
    blur_params_bind: wgpu::BindGroup,
    blur_sampler: wgpu::Sampler,
    /// Bound to group 2 by every run that samples nothing, because a
    /// pipeline's layout has to be satisfied whether a fragment reads it
    /// or not.
    blur_none: wgpu::BindGroup,
    /// The client's own scene shader, so a cube can be drawn before any
    /// server has sent a module.
    scene_shader: wgpu::ShaderModule,
    /// Group 0 of a scene: the uniform block, at a dynamic offset so one
    /// buffer serves every scene in the frame. The only thing bound to a
    /// scene shader, which is what `eui-shader` enforces on the module.
    scene_uniform_layout: wgpu::BindGroupLayout,
    scene_pipeline_layout: wgpu::PipelineLayout,
    /// One per distinct module, shared by every session on this device: a
    /// pipeline is compiled code, holding no texture and no buffer, so two
    /// sessions behind one cannot read each other. `None` is a module that
    /// was refused, remembered so it is not compiled again every frame.
    scene_pipelines: HashMap<scene::Key, Option<wgpu::RenderPipeline>>,
    /// Modules a server sent, by content hash, compiled once each and
    /// shared by every session that names one. A module is code, not data:
    /// it holds no texture and no buffer, so two sessions behind one cannot
    /// reach each other through it.
    scene_modules: HashMap<[u8; 32], wgpu::ShaderModule>,
    /// The source of every module a server sent, and when each was last
    /// drawn (a count of `render_scenes` calls, `scene_clock`).
    ///
    /// The compiled modules and their pipelines are capped at
    /// [`MAX_SCENE_MODULES`]: they are per process and outlive the session
    /// that sent them, and a compiled module is the naga IR plus whatever
    /// the driver made of it -- many times its text. The least recently
    /// drawn is dropped first. Its source is kept, because the worker sends
    /// a module once per hash and never again; a scene that comes back
    /// compiles it again from here, off the same text that was verified.
    scene_sources: HashMap<[u8; 32], (String, u64)>,
    scene_clock: u64,
    adapter_name: String,
    /// Whether this adapter can multisample a scene's target four ways. A
    /// count it cannot meet is a validation error, so it is asked once here
    /// rather than hoped for per frame.
    scene_msaa: bool,
    /// Whether this adapter is one a scene's shader may be trusted on at
    /// all. See [`Renderer::grants_scenes`].
    scene_ok: bool,
    /// Where wgpu's two out-of-band callbacks leave what they were told.
    trouble: Trouble,
    /// `EUI_GPU_TRACE=1` on an adapter that can: the main pass is
    /// bracketed by timestamps, resolved into a buffer that is read back
    /// the frame after, never waited on.
    timing: Option<Timing>,
}

/// The timestamp query and the two buffers it is read through.
struct Timing {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    /// Nanoseconds per timestamp tick.
    period: f32,
    /// A map of `read` was asked for and has not come back. The buffer is
    /// the GPU's until it does: it must not be written, and the map must
    /// not be asked for again -- asking twice for a mapping that is
    /// already outstanding panics wgpu on one backend and takes the
    /// process down on another.
    waiting: Option<std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
    /// The last frame's main pass, milliseconds, once it was read.
    last_ms: Option<f32>,
}

/// The textures one window owns. Everything else in `Renderer` is shared by
/// every window on the device — pipelines, layouts, shaders, the queue —
/// but these are per session and have to stay that way.
///
/// The glyph and image textures because a worker picks its own uv
/// coordinates: they are four floats in a quad, unchecked on the frame path
/// (`paint.rs`). One texture behind two sessions would let a hostile worker
/// sample the other application's rendered text. One texture each costs a
/// little memory and closes that without a per-quad check.
///
/// The blur textures for a duller reason: they are sized to the window's
/// blurred region. Shared, two windows of different sizes would take turns
/// missing `Blur::key` and reallocate every texture, every frame.
pub struct SessionTextures {
    /// The instance buffer the session's lists are uploaded into, and how
    /// many quads it holds. Per session for the reason that matters most
    /// to the budget: a list drawn again — a spinner, a transition the
    /// vertex stage interpolates — is the same bytes, and a buffer that
    /// still holds them need not be written. One buffer behind the chrome
    /// and the application would be overwritten by each in turn every
    /// frame, and neither could ever skip.
    instances: wgpu::Buffer,
    instance_cap: usize,
    /// The serial of the list in `instances`, zero when none is.
    uploaded: u64,
    atlas_tex: wgpu::Texture,
    img_tex: wgpu::Texture,
    /// The image texture's edge: 1 until a picture is packed, then
    /// `ImageAtlas::SIZE`. Kept so the grow is done once.
    img_size: u32,
    atlas_bind: wgpu::BindGroup,
    atlas_size: u32,
    /// The working textures, kept for as long as the next frame wants the
    /// same ones and dropped when a frame stops asking.
    blur: Option<Blur>,
    /// The session's scene targets, meshes and uniform slots. Per session,
    /// like the atlases above and for the same reason.
    scenes: scene::Scenes,
}

impl fmt::Debug for SessionTextures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionTextures").field("atlas", &self.atlas_size).field("images", &self.img_size).finish()
    }
}

/// The off-screen textures a blurred frame works in, and every view and
/// bind group the passes over them need -- made with the textures, not per
/// frame.
struct Blur {
    /// Format, extent, and the *reduction* each chain works at — which is
    /// everything the textures depend on, and deliberately not the standard
    /// deviations themselves nor where the region is. An entrance animates a
    /// radius from zero over some tenths of a second (03 §5); keying on the
    /// radius would throw every texture away sixty times a second to redraw
    /// the same sizes, where keying on the reduction reallocates about four
    /// times over the whole animation. And a panel sliding in moves its
    /// region every frame: keyed on the exact rectangle, that was every
    /// texture, view and bind group made again for every frame of the
    /// slide.
    format: wgpu::TextureFormat,
    /// The textures' extent in frame pixels: the region's size rounded up
    /// by [`blur_extent`]. The region is drawn into the top-left corner of
    /// the snapshot, and a region that grows or shrinks inside the same
    /// bucket keeps every texture.
    extent: (u32, u32),
    /// The frame as it stood before the first blurred quad, `extent` big.
    snap_view: wgpu::TextureView,
    chains: Vec<BlurChain>,
}

/// One radius of a blurred frame: the two textures its separable Gaussian
/// ping-pongs between, and the bind groups the three passes and the main
/// pass read them through. The result ends in `a`.
struct BlurChain {
    /// The reduction factor.
    d: u32,
    /// Texels across and down, `extent / d`.
    size: (u32, u32),
    a_view: wgpu::TextureView,
    b_view: wgpu::TextureView,
    from_snap: wgpu::BindGroup,
    from_a: wgpu::BindGroup,
    from_b: wgpu::BindGroup,
    /// Group 2 of the main pass, for a run that samples this chain.
    out: wgpu::BindGroup,
}

/// How many compiled scene modules the renderer keeps, with their pipelines.
/// A page shows a handful; this is room for several pages' worth before the
/// least recently drawn is compiled again on its next appearance.
pub const MAX_SCENE_MODULES: usize = 32;

/// The least recently used of `held` (a hash and the clock it was last used
/// at), never one used at `now`. Ties go to the smaller hash, so the choice
/// does not depend on a map's order.
fn lru_victim(held: impl Iterator<Item = ([u8; 32], u64)>, now: u64) -> Option<[u8; 32]> {
    held.filter(|(_, used)| *used != now).min_by_key(|(h, used)| (*used, *h)).map(|(h, _)| h)
}

/// The granularity of a blur's textures, in frame pixels.
const BLUR_BUCKET: u32 = 128;

/// The extent a blur's textures are made at for a region of `w × h`: each
/// side rounded up to [`BLUR_BUCKET`], and no larger than the device allows.
///
/// A multiple of 128 is a multiple of every reduction (at most 16), so a
/// chain's texture is exactly `extent / d` and a frame pixel maps onto it
/// with no rounding. And a region that changes size by less than a bucket --
/// a sheet sliding in, a scrim fading, a radius animating -- keeps the
/// textures it has.
fn blur_extent(w: u32, h: u32, max: u32) -> (u32, u32) {
    let up = |v: u32| v.max(1).div_ceil(BLUR_BUCKET).saturating_mul(BLUR_BUCKET).min(max.max(1));
    (up(w), up(h))
}

/// How far to reduce the snapshot for a blur of this standard deviation, in
/// device pixels.
///
/// The point is to keep the standard deviation at the working resolution
/// near two and a half pixels whatever was asked for, so the kernel stays
/// about fifteen taps and the cost of a blur does not grow with its radius.
/// The reduction is capped at sixteen, past which a standard deviation over
/// about 85 device pixels stops getting the full width it asked for — a
/// frost that heavy has nothing left to show anyway.
fn reduce_factor(sigma: f32) -> u32 {
    let k = (sigma / 2.5).max(1.0).log2().round().clamp(0.0, 4.0);
    1u32 << (k as u32)
}

/// The uniform block of `shader.wgsl`: four vec4, then thirty-two for the
/// scrollers (two a slot, sixteen slots), then sixty-four for the subtrees
/// on the move (four a slot, sixteen slots).
const UNIFORMS_BYTES: u64 = 16 * (4 + 32 + 64);

/// The uniform block as the shader reads it, from the frame's clock, the
/// layer transform and the list's scrollers.
fn uniforms(head: [f32; 8], clock: [f32; 4], frame: [f32; 4], list: &DrawList) -> Vec<f32> {
    let mut u = Vec::with_capacity((UNIFORMS_BYTES / 4) as usize);
    u.extend_from_slice(&head);
    u.extend_from_slice(&clock);
    u.extend_from_slice(&frame);
    // Slot zero is nothing, and is never read.
    u.extend_from_slice(&[0.0; 8]);
    for s in list.scrollers.iter().take(MAX_SCROLLERS) {
        u.extend_from_slice(&[s.from[0], s.from[1], s.to[0], s.to[1], s.t0, s.dur, s.curve as f32, 0.0]);
    }
    // The scroller array is a fixed sixteen slots whether they are all used
    // or not, so the transforms start where the shader expects them.
    u.resize(((16 * (4 + 32)) / 4) as usize, 0.0);
    // Slot zero is nothing, and takes its four vec4 all the same so that
    // slot one starts where the shader's `xslot * 4` says it does.
    u.extend_from_slice(&[0.0; 16]);
    for x in list.xforms.iter().take(MAX_XFORMS) {
        u.extend_from_slice(&x.from);
        u.extend_from_slice(&x.to);
        u.extend_from_slice(&x.clock);
        u.extend_from_slice(&x.pivot);
    }
    u.resize((UNIFORMS_BYTES / 4) as usize, 0.0);
    u
}

/// A clip rectangle where the layer transform puts it, in the same device
/// pixels it arrived in.
///
/// Rounded **outward** and clamped at the near edge. Outward because a clip
/// is a promise that nothing outside it is drawn, and half a pixel of slack
/// costs a seam nobody sees where half a pixel of bite costs a visible one;
/// clamped because the part of a sliding page that has gone off the leading
/// edge is off the screen, which is exactly what a scissor of zero width
/// says.
fn layer_clip(r: [u32; 4], size: (u32, u32), shift: (f32, f32), scale: f32) -> [u32; 4] {
    if shift == (0.0, 0.0) && scale == 1.0 {
        return r;
    }
    let half = (size.0 as f32 * 0.5, size.1 as f32 * 0.5);
    let at = |v: f32, h: f32, d: f32| (v - h) * scale + h + d;
    let x0 = at(r[0] as f32, half.0, shift.0);
    let y0 = at(r[1] as f32, half.1, shift.1);
    let x1 = at((r[0] + r[2]) as f32, half.0, shift.0);
    let y1 = at((r[1] + r[3]) as f32, half.1, shift.1);
    let lo = |v: f32| v.floor().max(0.0) as u32;
    let hi = |v: f32| v.ceil().max(0.0) as u32;
    [lo(x0), lo(y0), hi(x1).saturating_sub(lo(x0)), hi(y1).saturating_sub(lo(y0))]
}

/// A layer that is where it was laid out, at its own size and its own
/// opacity. The backdrop's snapshot pass always draws at it: a blur samples
/// what the frame actually holds, and the frame holds the page where the
/// layer put it.
const IDENTITY_LAYER: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// One `Params` in `blur.wgsl`, padded to a dynamic-offset stride.
const BLUR_PARAMS: u64 = 32;
/// wgpu's floor for `min_uniform_buffer_offset_alignment`, which every
/// adapter meets and most report exactly.
const BLUR_STRIDE: u64 = 256;

impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer").field("adapter", &self.adapter_name).finish()
    }
}

/// Where a frame is going: the view to draw into, the format that view was
/// made with — a window's surface format is not `FORMAT` everywhere — its
/// size in device pixels, and the two clocks the vertex stage animates
/// from: the window's, for `spin`, and the list's own age, for a
/// transition (03 §5).
#[derive(Debug, Clone, Copy)]
pub struct Target<'a> {
    /// The view to draw into.
    pub view: &'a wgpu::TextureView,
    /// The format that view was created with.
    pub format: wgpu::TextureFormat,
    /// The size of the rectangle this list is drawn into, in device pixels.
    /// The list is laid out as if that rectangle were the whole window: its
    /// own origin is (0, 0) whatever `origin` below says.
    pub size: (u32, u32),
    /// Where that rectangle sits in the view, in device pixels. `(0, 0)` for
    /// a list that owns the whole window — which is every list until a shell
    /// draws its chrome above an application.
    pub origin: (u32, u32),
    /// Clear the rectangle to the list's own background first, or draw over
    /// what is already there. The first list of a frame clears; one
    /// composited on top of it does not.
    pub clear: bool,
    /// Seconds since the window started. Double precision, so the spin's
    /// phase is exact after hours; the fraction is what reaches the GPU.
    pub now: f64,
    /// Seconds since the paint that produced the list: a transition's
    /// start time is relative to that, so the list and the clock agree
    /// whatever process painted it.
    pub age: f32,
    /// How far this whole list is moved from where it was laid out, in
    /// device pixels, and which way.
    ///
    /// Signed and fractional, which is why it is not `origin`: `origin` ends
    /// up in `set_viewport` and in every scissor rect, and wgpu refuses a
    /// viewport that leaves the attachment — so a page cannot be slid off the
    /// leading edge by moving it. `origin` says where the rectangle is;
    /// `shift` says where its contents have got to.
    pub shift: (f32, f32),
    /// A uniform scale about the rectangle's own centre; `1.0` is none.
    ///
    /// Uniform, and never two factors. The rounded-rect distance field the
    /// fragment stage works in is a true Euclidean distance only under a
    /// conformal map: scale x and y differently and the corner arcs become
    /// ellipses the field no longer describes, and the antialiasing ramp
    /// comes out a different width on the vertical edges than the horizontal
    /// ones, which shows as a kink at every corner. Nothing a page does
    /// wants anisotropy.
    pub scale: f32,
    /// The opacity everything in the list is drawn at; `1.0` is none.
    pub alpha: f32,
}

impl<'a> Target<'a> {
    /// A target that is the whole view: no offset, and it clears. The list
    /// is as old as the paint: `age` zero.
    pub fn whole(view: &'a wgpu::TextureView, format: wgpu::TextureFormat, size: (u32, u32), now: f64) -> Self {
        Self { view, format, size, origin: (0, 0), clear: true, now, age: 0.0, shift: (0.0, 0.0), scale: 1.0, alpha: 1.0 }
    }

    /// The same target, for a list this old.
    #[must_use]
    pub fn aged(self, age: f32) -> Self {
        Self { age, ..self }
    }

    /// The same target with the list moved, scaled and faded as one layer:
    /// a page on its way in or out (03 §5).
    #[must_use]
    pub fn layered(self, shift: (f32, f32), scale: f32, alpha: f32) -> Self {
        Self { shift, scale, alpha, ..self }
    }

    /// Whether this layer is anywhere other than where it was laid out.
    #[must_use]
    pub fn moved(&self) -> bool {
        self.shift != (0.0, 0.0) || self.scale != 1.0
    }
}

/// The spin phase at `now`: a fraction of one revolution (03 §5), computed
/// in double precision so a spinner is as smooth after a day as at the
/// start.
#[expect(clippy::cast_possible_truncation, reason = "a fraction of one")]
fn spin_phase(now: f64) -> f32 {
    (now / 1.2).fract() as f32
}

/// What one `render` cost the queue: the numbers the trace prints and the
/// budgets of 10 §1 are checked against. A frame that draws the last list
/// again uploads nothing, and this is where that shows.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct RenderStats {
    /// Instances in the list.
    pub quads: usize,
    /// Draw calls.
    pub runs: usize,
    /// Render passes encoded, the backdrop's included.
    pub passes: usize,
    /// Queue submissions.
    pub submits: usize,
    /// Bytes written to the instance buffer; zero when the list was
    /// already there.
    pub instance_bytes: usize,
    /// Bytes written to the glyph and image textures.
    pub atlas_bytes: usize,
    /// The list's serial matched what the buffer holds, so no instance
    /// bytes were written.
    pub upload_skipped: bool,
    /// The main pass of the frame before this one on the GPU, in
    /// milliseconds, when the renderer was asked to time it
    /// (`EUI_GPU_TRACE=1`) and the adapter can.
    pub gpu_ms: Option<f32>,
}

/// An off-screen target that can be read back.
#[derive(Debug)]
pub struct Offscreen {
    texture: wgpu::Texture,
    width: u32,
    height: u32,
}

impl Renderer {
    /// A renderer with no window: adapter chosen without a surface, so this
    /// works on a headless machine as long as any adapter exists.
    ///
    /// Native only, and not merely for want of a thread to block. wgpu's
    /// WebGL2 backend enumerates adapters *out of* a canvas's GL context,
    /// so a probe with no surface finds WebGPU or nothing — a page must
    /// make its surface first and ask about that. See `app.rs`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_headless() -> Result<Self, RenderError> {
        Self::new_headless_timed(std::env::var("EUI_GPU_TRACE").as_deref() == Ok("1"))
    }

    /// [`Self::new_headless`], timing the GPU or not as asked rather than
    /// as the environment says.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_headless_timed(timed: bool) -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::all(), ..Default::default() });
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::LowPower, compatible_surface: None, force_fallback_adapter: false }))
                .ok_or(RenderError::NoAdapter)?;
        Self::with_adapter_timed(&adapter, timed)
    }

    /// A renderer on an adapter the caller chose (for a window surface).
    /// `EUI_GPU_TRACE=1` asks for the GPU's own timing of each frame.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_adapter(adapter: &wgpu::Adapter) -> Result<Self, RenderError> {
        Self::with_adapter_timed(adapter, std::env::var("EUI_GPU_TRACE").as_deref() == Ok("1"))
    }

    /// [`Self::with_adapter`], timing the GPU or not as asked.
    ///
    /// The blocking half of [`Self::with_adapter_async`]: the device
    /// request is the only part of this that is asynchronous, and a thread
    /// that may wait for it is the ordinary case. A browser is the one that
    /// may not — see the async twin.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_adapter_timed(adapter: &wgpu::Adapter, timed: bool) -> Result<Self, RenderError> {
        pollster::block_on(Self::with_adapter_async(adapter, timed))
    }

    /// [`Self::with_adapter_timed`] without a thread to block.
    ///
    /// Asking a browser for a device is asking the page's one thread to
    /// wait for the page, which is the one thing it may never do:
    /// `request_device` is a promise, and blocking on it parks the thread
    /// that has to run it. So the await is the whole difference, and
    /// everything after it — the modules, the layouts, the pipelines — is
    /// the same work in the same order, which is why it is one body with
    /// two front doors rather than two bodies.
    pub async fn with_adapter_async(adapter: &wgpu::Adapter, timed: bool) -> Result<Self, RenderError> {
        // Timestamps only when asked, and only where they exist: a feature
        // asked for and absent is no device at all.
        let timed = timed && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let features = if timed { wgpu::Features::TIMESTAMP_QUERY } else { wgpu::Features::empty() };
        let descriptor = |limits: wgpu::Limits| wgpu::DeviceDescriptor { label: Some("eui"), required_features: features, required_limits: limits, memory_hints: wgpu::MemoryHints::MemoryUsage };
        // Downlevel limits cap textures at 2048 px, which a high-DPI window
        // exceeds on its first frame. Ask for the ordinary defaults, trimmed
        // to what the adapter has.
        //
        // `using_resolution` only trims the texture dimensions, though, and
        // the defaults are a desktop's on every other axis — so a GLES
        // adapter, which is what an Android emulator has where it has no
        // Vulkan, can refuse a set it cannot meet. Asking for exactly what
        // the adapter reports is always satisfiable and is never less than
        // downlevel, so the second attempt opens a device wherever one can
        // be opened at all. Second and not first, because the defaults are
        // the floor the renderer is written against and a device that meets
        // them should be held to them.
        //
        // A third tier below those two, for the one adapter that can refuse
        // both: WebGL2. `adapter.limits()` is satisfiable by construction
        // everywhere a driver reports honestly, and on the web it is the
        // browser reporting — so this is the floor to fall back to rather
        // than a device that never opens and a canvas that never draws.
        // Last, because it caps a texture at 2048 px and the renderer would
        // rather have the room.
        let (device, queue) = match adapter.request_device(&descriptor(wgpu::Limits::default().using_resolution(adapter.limits())), None).await {
            Ok(d) => d,
            Err(first) => match adapter.request_device(&descriptor(adapter.limits()), None).await {
                Ok(d) => d,
                Err(_) => {
                    adapter.request_device(&descriptor(wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())), None).await.map_err(|_| RenderError::Device(first.to_string()))?
                }
            },
        };

        // Both of wgpu's out-of-band reports, recorded rather than fatal.
        //
        // Without these two lines a lost device is a silent freeze and an
        // uncaptured error is a panic -- in the window process, which holds
        // the display, TLS and the pin store for every session. 08 §10
        // promises that a failure ends the session with a reason and leaves
        // the window standing; that promise is only kept if the window is
        // told. Neither callback may allocate a session's worth of work or
        // block: they run on the driver's thread.
        let trouble = Trouble::default();
        let lost = trouble.clone();
        device.set_device_lost_callback(move |reason, message| {
            lost.set(RenderError::DeviceLost(format!("{reason:?}: {message}")));
        });
        let uncaught = trouble.clone();
        device.on_uncaptured_error(Box::new(move |e| {
            uncaught.set(RenderError::Uncaptured(e.to_string()));
        }));

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("eui quad"), source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()) });

        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("eui blur"), source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()) });

        let scene_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("eui scene"), source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()) });

        let scene_uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("scene uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                // One buffer, one slot per scene in the frame: the shape
                // `blur_params` already uses, for the same reason.
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<scene::SceneUniforms>() as u64),
                },
                count: None,
            }],
        });
        let scene_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene"),
            // One group, and that is the whole reach of a scene shader.
            bind_group_layouts: &[&scene_uniform_layout],
            push_constant_ranges: &[],
        });

        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                // The fragment stage reads the backdrop's region out of it.
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let sampled = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let sampler_at = |binding| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None };
        let blur_out_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("blur out"), entries: &[sampled(0), sampler_at(1)] });
        let blur_src_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("blur src"), entries: &[sampled(0), sampler_at(1)] });
        let blur_params_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur params"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    // Every pass of a chain differs only in these 32 bytes,
                    // so they all ride in one buffer and each pass names its
                    // own slice rather than waiting for a write between
                    // submits.
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(BLUR_PARAMS),
                },
                count: None,
            }],
        });
        let layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("eui"), bind_group_layouts: &[&uniform_layout, &atlas_layout, &blur_out_layout], push_constant_ranges: &[] });
        let blur_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("eui blur"), bind_group_layouts: &[&blur_params_layout, &blur_src_layout], push_constant_ranges: &[] });

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            // viewport, backdrop, clock, then the scrollers in flight.
            size: UNIFORMS_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blur_params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blur params"),
            size: BLUR_STRIDE * 3,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blur_params_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur params"),
            layout: &blur_params_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &blur_params, offset: 0, size: wgpu::BufferSize::new(BLUR_PARAMS) }) }],
        });
        let blur_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blur"),
            // Clamped, so a tap past the snapshot's edge repeats it rather
            // than wrapping the far side of the window into the blur.
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let none = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blur none"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let blur_none = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur none"),
            layout: &blur_out_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&none.create_view(&Default::default())) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blur_sampler) },
            ],
        });
        let uniform_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });

        let timing = timed.then(|| Timing {
            set: device.create_query_set(&wgpu::QuerySetDescriptor { label: Some("frame timing"), ty: wgpu::QueryType::Timestamp, count: 2 }),
            resolve: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("timing resolve"),
                size: 16,
                usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            }),
            read: device.create_buffer(&wgpu::BufferDescriptor { label: Some("timing read"), size: 16, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false }),
            period: queue.get_timestamp_period(),
            waiting: None,
            last_ms: None,
        });

        Ok(Self {
            device,
            queue,
            shader,
            pipeline_layout: layout,
            pipelines: HashMap::new(),
            uniforms,
            uniform_bind,
            atlas_layout,
            blur_shader,
            blur_params_layout,
            blur_src_layout,
            blur_out_layout,
            blur_pipeline_layout,
            blur_pipelines: HashMap::new(),
            blur_params,
            blur_params_bind,
            blur_sampler,
            blur_none,
            scene_shader,
            scene_uniform_layout,
            scene_pipeline_layout,
            scene_pipelines: HashMap::new(),
            scene_modules: HashMap::new(),
            scene_sources: HashMap::new(),
            scene_clock: 0,
            adapter_name: adapter.get_info().name,
            scene_msaa: adapter.get_texture_format_features(FORMAT).flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4)
                && adapter.get_texture_format_features(scene::DEPTH_FORMAT).flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_X4),
            // GL generates unchecked indexing. A browser is the other
            // case, and a different one: on WebGPU nothing can be caught
            // at all, because `pop_error_scope` is a promise and the one
            // thread a page has is the thread drawing. A module a server
            // chose that cannot be checked before it is compiled is not
            // offered — `Tab::open` masks the capability out and says why.
            scene_ok: !matches!(adapter.get_info().backend, wgpu::Backend::Gl | wgpu::Backend::BrowserWebGpu),
            trouble,
            timing,
        })
    }

    /// The pipeline for a target format, built the first time that format is
    /// drawn into. A pipeline's colour target must match the view it writes
    /// to, so this cannot be settled once at start-up: the window does not
    /// know its surface format until it has an adapter to ask.
    fn pipeline_for(&mut self, format: wgpu::TextureFormat) -> &wgpu::RenderPipeline {
        let (device, shader, layout) = (&self.device, &self.shader, &self.pipeline_layout);
        self.pipelines.entry(format).or_insert_with(|| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("eui quad"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Quad>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Unorm16x4, 8 => Unorm16x4],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            })
        })
    }

    /// `img_size` is the image atlas's edge. It is 1 until a picture is
    /// actually packed: the full texture is 2048² RGBA — 16 MiB of GPU
    /// memory — and most applications never show one.
    fn make_atlas(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, size: u32, img_size: u32) -> (wgpu::Texture, wgpu::Texture, wgpu::BindGroup) {
        let make = |label: &str, size: u32, format: wgpu::TextureFormat| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let tex = make("atlas", size, wgpu::TextureFormat::R8Unorm);
        let img = make("images", img_size, wgpu::TextureFormat::Rgba8UnormSrgb);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("atlas"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let view = tex.create_view(&Default::default());
        let img_view = img.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&img_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        (tex, img, bind)
    }

    /// The textures for one more window on this device. A session holds
    /// this for as long as its window lives and hands it back to every
    /// `render` call; the renderer itself keeps no window's textures.
    pub fn session(&self) -> SessionTextures {
        let atlas_size = Atlas::INITIAL;
        let (atlas_tex, img_tex, atlas_bind) = Self::make_atlas(&self.device, &self.atlas_layout, atlas_size, 1);
        let instance_cap = 4096;
        let instances = Self::make_instances(&self.device, instance_cap);
        SessionTextures { instances, instance_cap, uploaded: 0, atlas_tex, img_tex, img_size: 1, atlas_bind, atlas_size, blur: None, scenes: scene::Scenes::default() }
    }

    fn make_instances(device: &wgpu::Device, cap: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (cap * std::mem::size_of::<Quad>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// The adapter's name, for diagnostics.
    ///
    /// It names the machine's graphics hardware, so it is a fingerprint:
    /// 08 §8. It may reach a log and a person, and it must reach neither
    /// the tree nor an event payload.
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Whether a scene's shader may be run on this adapter at all (11 §2.5).
    ///
    /// False on a GL backend, and this is the one refusal in the renderer
    /// that is about somebody else's bug. wgpu generates **unchecked**
    /// indexing there -- `index`, `buffer` and `binding_array` all
    /// `Unchecked`, with the safety left to GLSL, whose answer to an
    /// out-of-range index is undefined behaviour. Vulkan, Metal and DX12
    /// restrict; GLES does not, and GLES is what an Android device without
    /// Vulkan has.
    ///
    /// The verifier already refuses every index that is not a constant, so
    /// this is the belt behind that brace. Both, because the GLES translator
    /// is the least exercised path in the stack and the shader is the
    /// attacker's.
    #[must_use]
    pub fn grants_scenes(&self) -> bool {
        self.scene_ok
    }

    /// Whether this adapter can draw a scene four times over and resolve.
    ///
    /// A client that cannot does not refuse the scene: it draws it once, and
    /// the cube's silhouette is a little harder than it would be elsewhere.
    /// Exposed so a test can say which of those it is looking at rather than
    /// pass vacuously on a machine that was never going to multisample.
    #[must_use]
    pub fn can_multisample_scenes(&self) -> bool {
        self.scene_msaa
    }

    /// What the device reported out of band, if anything: a lost device, or
    /// an error no scope was open for. The window asks between frames and
    /// ends the session with the reason rather than drawing on.
    ///
    /// Once this answers, it goes on answering: nothing built on a lost
    /// device is valid, so there is nothing to retry and nothing to clear.
    #[must_use]
    pub fn trouble(&self) -> Option<RenderError> {
        self.trouble.get()
    }

    /// Run `f` with a validation scope open, so that what it builds fails as
    /// a `Result` instead of reaching the uncaptured handler.
    ///
    /// This is what makes it safe to compile something the network chose:
    /// wgpu's default for an uncaptured validation error is a panic, and a
    /// panic here is the window process. `what` names the thing being built,
    /// for the message.
    ///
    /// The scope catches validation only. A device lost under `f` lands in
    /// [`Self::trouble`] instead, which is where the caller looks next.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn scoped<T>(&self, what: &str, f: impl FnOnce(&wgpu::Device) -> T) -> Result<T, RenderError> {
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let made = f(&self.device);
        match pollster::block_on(self.device.pop_error_scope()) {
            None => Ok(made),
            // The message is for a log, never for the server: a driver's
            // diagnostic names the driver (08 §8).
            Some(e) => Err(RenderError::Uncaptured(format!("{what}: {e}"))),
        }
    }

    /// [`Self::scoped`] where a scope cannot answer in time — so it does
    /// not open one and does not pretend to.
    ///
    /// WebGPU's validation errors are asynchronous by specification: the
    /// verdict on a module arrives through a promise, after the thing built
    /// from it has been used, and the one thread a page has is the thread
    /// drawing. There is no synchronous escape and no amount of structure
    /// makes one.
    ///
    /// Since what this guards is a program the *server* wrote (11 §2.5),
    /// the answer is not to build it unchecked but to not offer the
    /// capability: `scene_ok` is false on this backend, `Tab::open` masks
    /// `scene` out of the grant, and nothing reaches here. This refuses
    /// rather than succeeding quietly, so that if a path ever does reach
    /// it the session ends with a reason instead of compiling something
    /// nobody verified.
    #[cfg(target_arch = "wasm32")]
    pub fn scoped<T>(&self, what: &str, _f: impl FnOnce(&wgpu::Device) -> T) -> Result<T, RenderError> {
        Err(RenderError::Uncaptured(format!("{what}: a browser cannot check a module before it is used, so one is never built here")))
    }

    /// The device, for a client that manages its own surface.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Upload the atlases if they changed, growing the glyph texture with
    /// its atlas.
    fn sync_atlas(&self, tex: &mut SessionTextures, atlas: &mut Atlas, images: &mut ImageAtlas) -> usize {
        let mut bytes = 0;
        // The image texture is made full size the first time a picture is
        // actually packed, and never before.
        let want_img = if images.is_empty() { tex.img_size } else { ImageAtlas::SIZE };
        if atlas.size() != tex.atlas_size || want_img != tex.img_size {
            let (glyphs, img, bind) = Self::make_atlas(&self.device, &self.atlas_layout, atlas.size(), want_img);
            tex.img_size = want_img;
            tex.atlas_tex = glyphs;
            tex.img_tex = img;
            tex.atlas_bind = bind;
            tex.atlas_size = atlas.size();
            // Both textures were remade -- they share a bind group -- so
            // both are blank, and everything either atlas holds is owed
            // again. A picture arriving over the network, frames after the
            // words were uploaded, would otherwise take the words with it.
            atlas.mark_dirty_all();
            images.mark_dirty_all();
        }
        // Only what changed crosses to the GPU. For pictures that is a
        // rectangle apiece, read straight out of the sheet at its offset: a
        // `bytes_per_row` of the whole sheet's width with an extent of the
        // rectangle's makes wgpu stage the rectangle's texels and not the
        // rows they sit on, so a video's frame costs its own bytes.
        let img_row = images.size() * 4;
        for &[x, y, w, h] in images.dirty_regions() {
            if images.pixels().is_empty() || w == 0 || h == 0 {
                continue;
            }
            bytes += (w * h * 4) as usize;
            self.queue.write_texture(
                wgpu::ImageCopyTexture { texture: &tex.img_tex, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
                images.pixels(),
                wgpu::ImageDataLayout { offset: u64::from(y) * u64::from(img_row) + u64::from(x) * 4, bytes_per_row: Some(img_row), rows_per_image: Some(h) },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
        }
        images.mark_clean();
        // Band by band: two glyphs on shelves far apart cost their rows,
        // not the rows between.
        for &(y0, y1) in atlas.dirty_bands() {
            let rows = atlas.rows(y0, y1);
            bytes += rows.len();
            self.queue.write_texture(
                wgpu::ImageCopyTexture { texture: &tex.atlas_tex, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: y0, z: 0 }, aspect: wgpu::TextureAspect::All },
                rows,
                wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(atlas.size()), rows_per_image: Some(y1 - y0) },
                wgpu::Extent3d { width: atlas.size(), height: y1 - y0, depth_or_array_layers: 1 },
            );
        }
        atlas.mark_clean();
        bytes
    }

    /// The two blur pipelines for a target format, built the first time a
    /// frame in that format asks for a blur — which for most sessions is
    /// never.
    fn blur_pipeline_for(&mut self, format: wgpu::TextureFormat) {
        let (device, shader, layout) = (&self.device, &self.blur_shader, &self.blur_pipeline_layout);
        self.blur_pipelines.entry(format).or_insert_with(|| {
            let make = |entry: &'static str| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(entry),
                    layout: Some(layout),
                    vertex: wgpu::VertexState { module: shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                    fragment: Some(wgpu::FragmentState {
                        module: shader,
                        entry_point: Some(entry),
                        compilation_options: Default::default(),
                        // Every pass here overwrites its whole target, so
                        // there is nothing to blend with.
                        targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                    }),
                    primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview: None,
                    cache: None,
                })
            };
            (make("reduce"), make("gauss"))
        });
    }

    /// The textures the frame's blurs work in, made afresh only when the
    /// extent's bucket, the reductions or the format have moved since the
    /// last frame -- compared in place, so a frame that keeps them
    /// allocates nothing to find that out.
    fn blur_targets(&self, tex: &mut SessionTextures, format: wgpu::TextureFormat, b: &Backdrop) {
        let extent = blur_extent(b.rect[2], b.rect[3], self.device.limits().max_texture_dimension_2d);
        let same =
            tex.blur.as_ref().is_some_and(|x| x.format == format && x.extent == extent && x.chains.len() == b.sigmas.len() && x.chains.iter().zip(&b.sigmas).all(|(c, s)| c.d == reduce_factor(*s)));
        if same {
            return;
        }
        let device = &self.device;
        let make = |label: &str, w: u32, h: u32| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let bind = |label: &str, layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.blur_sampler) },
                ],
            })
        };
        let snap_view = make("backdrop", extent.0, extent.1);
        let chains = b
            .sigmas
            .iter()
            .map(|sigma| {
                let d = reduce_factor(*sigma);
                let size = ((extent.0 / d).max(1), (extent.1 / d).max(1));
                let (a_view, b_view) = (make("blur a", size.0, size.1), make("blur b", size.0, size.1));
                BlurChain {
                    d,
                    size,
                    from_snap: bind("blur src", &self.blur_src_layout, &snap_view),
                    from_a: bind("blur src", &self.blur_src_layout, &a_view),
                    from_b: bind("blur src", &self.blur_src_layout, &b_view),
                    out: bind("blur out", &self.blur_out_layout, &a_view),
                    a_view,
                    b_view,
                }
            })
            .collect();
        tex.blur = Some(Blur { format, extent, snap_view, chains });
    }

    /// The pipeline for one module, built the first time it is drawn.
    ///
    /// Never from inside `render`: compiling a fresh module costs tens of
    /// milliseconds, and doing it on the frame that first wants it would put
    /// that straight into "launch to first pixel" (10 §1). A module that is
    /// not ready yet draws nothing this frame and is ready for the next.
    ///
    /// The error scope is the load-bearing part. wgpu's default for an
    /// uncaptured validation error is a panic, and this is the one place in
    /// the client where the thing being compiled was chosen by a server: a
    /// panic here would be the window process, which holds the display, TLS
    /// and the pin store for every session.
    fn scene_pipeline_for(&mut self, key: scene::Key) {
        if self.scene_pipelines.contains_key(&key) {
            return;
        }
        let shader = if key.shader == [0u8; 32] {
            &self.scene_shader
        } else {
            // Evicted, and wanted again: compiled once more from the source
            // that was kept for it.
            if !self.scene_modules.contains_key(&key.shader) {
                let Some((src, _)) = self.scene_sources.get(&key.shader) else {
                    // Not cached as a refusal: the module may simply not have
                    // arrived yet, and a hash that is still being fetched must
                    // be able to compile on the frame it lands.
                    return;
                };
                let owned = src.clone();
                match self.scoped("scene shader", move |device| device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("scene"), source: wgpu::ShaderSource::Wgsl(owned.into()) })) {
                    Ok(m) => {
                        self.scene_modules.insert(key.shader, m);
                    }
                    Err(e) => {
                        eprintln!("eui: {e}");
                        self.scene_sources.remove(&key.shader);
                        return;
                    }
                }
            }
            let Some(m) = self.scene_modules.get(&key.shader) else { return };
            m
        };
        let layout = &self.scene_pipeline_layout;
        let samples = key.samples.max(1);
        let depth = key.depth.then_some(wgpu::DepthStencilState {
            format: scene::DEPTH_FORMAT,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        });
        let built = self.scoped("scene pipeline", |device| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("eui scene"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: scene::VERTEX_BYTES,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    // Always `FORMAT`, never the window's: a target is the
                    // renderer's own texture, so one module is compiled once
                    // per process instead of once per surface format.
                    targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL })],
                }),
                primitive: wgpu::PrimitiveState { cull_mode: Some(wgpu::Face::Back), ..Default::default() },
                depth_stencil: depth,
                multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
                multiview: None,
                cache: None,
            })
        });
        match built {
            Ok(p) => {
                self.scene_pipelines.insert(key, Some(p));
            }
            Err(e) => {
                // Remembered as a refusal so it is not attempted again every
                // frame. The message is for a log and never for the server:
                // a driver's diagnostic names the driver (08 §8).
                eprintln!("eui: {e}");
                self.scene_pipelines.insert(key, None);
            }
        }
    }

    /// What a scene's pipeline is built for.
    ///
    /// The sample count is the client's answer, not the server's request: a
    /// count this adapter cannot meet is a validation error, so an author
    /// who asked for four on a machine that has none gets one, silently and
    /// correctly.
    fn scene_key(&self, s: &crate::SceneDraw) -> scene::Key {
        let samples = if s.flags & crate::SCENE_MSAA != 0 && self.scene_msaa { 4 } else { 1 };
        scene::Key { shader: s.shader, depth: s.flags & crate::SCENE_DEPTH != 0, samples }
    }

    /// Render every scene the list carries into a target of its own, before
    /// the frame that samples them is drawn.
    ///
    /// One pass and one submit per scene. They write into their own uniform
    /// slots at dynamic offsets, so — unlike the backdrop, which has to
    /// share `self.uniforms` with the main pass and therefore submits
    /// separately to order the writes — there is no ordering puzzle here at
    /// all. That is a reason to like the design, not an accident of it.
    fn render_scenes(&mut self, tex: &mut SessionTextures, list: &DrawList, now: f64, age: f32) -> usize {
        // What this frame draws is marked first, so the cap never takes a
        // module out from under the frame that wants it.
        self.scene_clock = self.scene_clock.wrapping_add(1);
        for s in &list.scenes {
            if let Some((_, used)) = self.scene_sources.get_mut(&s.shader) {
                *used = self.scene_clock;
            }
        }
        self.evict_scene_modules();
        if list.scenes.is_empty() {
            // A session that stops having scenes stops paying for them.
            tex.scenes.targets.clear();
            return 0;
        }
        tex.scenes.frame = tex.scenes.frame.wrapping_add(1);
        let max = self.device.limits().max_texture_dimension_2d;

        // Pipelines first: `render_scenes` may not compile inside its own
        // encoder, and the borrow checker agrees with the budget here.
        for s in &list.scenes {
            self.scene_pipeline_for(self.scene_key(s));
        }

        // The client's own cube, uploaded once per session.
        tex.scenes.meshes.entry([0u8; 32]).or_insert_with(|| {
            let (vertices, indices) = scene::cube();
            self.upload_mesh(&vertices, &indices)
        });

        // One uniform buffer for the frame, grown in powers of two.
        let want = list.scenes.len();
        if tex.scenes.uniforms.is_none() || tex.scenes.slots < want {
            let slots = want.next_power_of_two().max(1);
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene uniforms"),
                size: (slots as u64).saturating_mul(scene::SLOT),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("scene uniforms"),
                layout: &self.scene_uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buffer, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<scene::SceneUniforms>() as u64) }),
                }],
            });
            tex.scenes.uniforms = Some((buffer, bind));
            tex.scenes.slots = slots;
        }

        // Targets, and the uniforms that go with them.
        let mut redraw: Vec<bool> = Vec::with_capacity(list.scenes.len());
        for (slot, s) in list.scenes.iter().enumerate() {
            let size = scene::target_size(s.rect, max);
            let key = self.scene_key(s);
            let depth = key.depth;
            let held = tex.scenes.targets.get(&s.id);
            let fresh = held.map_or(true, |t| t.size != size || t.depth_view.is_some() != depth || t.samples != key.samples);
            // The quota (08 §6). A scene that cannot have a target draws its
            // own background, exactly as one whose module is still compiling
            // does: the application already has to cope with that, so this
            // adds no new failure for it to handle.
            if fresh && (held.is_some() || tex.scenes.targets.len() < scene::MAX_TARGETS) {
                let t = self.make_scene_target(size, depth, key.samples);
                tex.scenes.targets.insert(s.id, t);
            }
            let u = scene::uniforms_for(s, size, now, age);
            let Some(target) = tex.scenes.targets.get_mut(&s.id) else {
                redraw.push(false);
                continue;
            };
            target.used = tex.scenes.frame;
            // Nothing moved: the target already holds the right pixels, so
            // the pass below is not encoded at all. This is what makes the
            // "a still scene costs nothing" line of 10 §1 true rather than
            // merely nearly true.
            let same = target.last == Some(u) && !fresh;
            target.last = Some(u);
            redraw.push(!same);
            if let Some((buffer, _)) = &tex.scenes.uniforms {
                self.queue.write_buffer(buffer, (slot as u64).saturating_mul(scene::SLOT), bytemuck::bytes_of(&u));
            }
        }

        // A target nothing asked for this frame or last is dropped. One
        // frame of hysteresis, unlike `Blur`, which evicts at once: a scene
        // in a virtualised list crosses the edge of the viewport over and
        // over, and evicting on the first frame out would reallocate on the
        // next one in.
        let (frame, keep) = (tex.scenes.frame, tex.scenes.frame.wrapping_sub(1));
        tex.scenes.targets.retain(|_, t| t.used == frame || t.used == keep);

        let mut passes = 0usize;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("eui scenes") });
        for (slot, s) in list.scenes.iter().enumerate() {
            if redraw.get(slot) != Some(&true) {
                continue;
            }
            let key = self.scene_key(s);
            let (Some(Some(pipeline)), Some(target), Some(mesh), Some((_, bind))) =
                (self.scene_pipelines.get(&key), tex.scenes.targets.get(&s.id), tex.scenes.meshes.get(&s.mesh), tex.scenes.uniforms.as_ref())
            else {
                // A module still compiling, refused, or a mesh not yet
                // delivered: the quad samples a target that was cleared, and
                // the node shows its own background. Never an error, and
                // never a frame that fails to arrive.
                continue;
            };
            let c = s.clear;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    // Multisampled where there is one, resolving into the
                    // texture the composite quad samples; otherwise straight
                    // into it. A resolve target attached wrongly does not
                    // give a jagged picture, it gives a blank one -- which is
                    // what the vector for this checks first.
                    view: target.msaa_view.as_ref().unwrap_or(&target.view),
                    resolve_target: target.msaa_view.as_ref().map(|_| &target.view),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: f64::from(c[0]), g: f64::from(c[1]), b: f64::from(c[2]), a: f64::from(c[3]) }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: target.depth_view.as_ref().map(|v| wgpu::RenderPassDepthStencilAttachment {
                    view: v,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            #[expect(clippy::cast_possible_truncation, reason = "a frame's scenes are capped far below u32")]
            pass.set_bind_group(0, bind, &[(slot as u64 * scene::SLOT) as u32]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, 0..1);
            passes = passes.saturating_add(1);
        }
        // Nothing encoded, nothing submitted. A frame in which every scene
        // is still is a frame that costs the queue nothing at all.
        if passes > 0 {
            self.queue.submit([encoder.finish()]);
        }
        passes
    }

    /// Compile a module a server sent, once per content hash.
    ///
    /// The source reaching here has already passed `eui-shader`'s verifier
    /// in the worker. It is compiled anyway inside an error scope, for two
    /// reasons: a worker that has been taken over could send anything, and
    /// wgpu's default for an uncaptured validation error is a panic — in
    /// this process, which holds the display, TLS and the pin store for
    /// every session.
    ///
    /// # Errors
    ///
    /// [`RenderError::Uncaptured`] when the driver's front end refuses it.
    /// The message is for a log: forwarding it to a server would name the
    /// driver, and through it the machine (08 §8).
    pub fn load_shader(&mut self, hash: [u8; 32], source: &str) -> Result<(), RenderError> {
        if self.scene_modules.contains_key(&hash) {
            return Ok(());
        }
        let owned = source.to_owned();
        let module = self.scoped("scene shader", move |device| device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("scene"), source: wgpu::ShaderSource::Wgsl(owned.into()) }))?;
        self.scene_modules.insert(hash, module);
        // Stamped as drawn last frame: newer than anything not drawn since,
        // and not yet protected as this frame's. The cap is applied at the
        // next frame, which is when it can tell what is on screen.
        self.scene_sources.insert(hash, (source.to_owned(), self.scene_clock));
        Ok(())
    }

    /// Past [`MAX_SCENE_MODULES`] compiled modules, drop the least recently
    /// drawn, and every pipeline built from it. Never one drawn this frame.
    fn evict_scene_modules(&mut self) {
        while self.scene_modules.len() > MAX_SCENE_MODULES {
            let held = self.scene_modules.keys().map(|h| (*h, self.scene_sources.get(h).map_or(0, |(_, used)| *used)));
            let Some(victim) = lru_victim(held, self.scene_clock) else { break };
            self.scene_modules.remove(&victim);
            self.scene_pipelines.retain(|k, _| k.shader != victim);
        }
    }

    /// Compiled scene modules held, for a test of the cap.
    pub fn scene_modules_held(&self) -> usize {
        self.scene_modules.len()
    }

    /// Upload a mesh a server sent, once per session per content hash.
    ///
    /// Per session because a mesh is *data*: the image atlas is content
    /// addressed and still not shared between sessions (see the note on
    /// [`SessionTextures`]), and one rule in the repository beats two.
    pub fn load_mesh(&self, tex: &mut SessionTextures, hash: [u8; 32], vertices: &[scene::Vertex], indices: &[u32]) {
        if tex.scenes.meshes.contains_key(&hash) {
            return;
        }
        let mesh = self.upload_mesh(vertices, indices);
        tex.scenes.meshes.insert(hash, mesh);
    }

    /// A mesh's two buffers. The bytes are copied and never read: what makes
    /// that safe is that the worker already checked every index against the
    /// vertex count, which is the one bounds check no driver performs.
    fn upload_mesh(&self, vertices: &[scene::Vertex], indices: &[u32]) -> scene::Mesh {
        use wgpu::util::DeviceExt as _;
        let vertices = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("scene mesh"), contents: bytemuck::cast_slice(vertices), usage: wgpu::BufferUsages::VERTEX });
        let index_bytes = bytemuck::cast_slice(indices);
        let index_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("scene index"), contents: index_bytes, usage: wgpu::BufferUsages::INDEX });
        scene::Mesh { vertices, indices: index_buffer, count: u32::try_from(indices.len()).unwrap_or(0) }
    }

    /// A scene's colour target, its depth buffer, and the bind group the
    /// composite quad reads it through.
    ///
    /// **No `COPY_SRC`.** That is how 08 §8's "no canvas readback" is kept
    /// here: not by a rule somebody has to remember, but by a usage flag
    /// that makes `copy_texture_to_buffer` a validation error. The target is
    /// deliberately not an [`Offscreen`], which is the type `read_back`
    /// takes.
    fn make_scene_target(&self, size: (u32, u32), depth: bool, samples: u32) -> scene::Target {
        let color = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("scene"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = color.create_view(&Default::default());
        // The multisampled attachment, drawn into and resolved down to
        // `color` in the same pass. Nothing ever samples it, so it wants no
        // `TEXTURE_BINDING` -- and the depth buffer below MUST carry the
        // same count, or the pass is invalid.
        let (msaa, msaa_view) = if samples > 1 {
            let t = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scene msaa"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            (Some(t), Some(v))
        } else {
            (None, None)
        };
        let (depth_tex, depth_view) = if depth {
            let t = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("scene depth"),
                size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
                mip_level_count: 1,
                // Must match the colour target's, or the pass is invalid.
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format: scene::DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            (Some(t), Some(v))
        } else {
            (None, None)
        };
        // The same layout a finished blur binds, which is why a scene needs
        // no new bind group layout and no change to the quad pipeline.
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("scene out"),
            layout: &self.blur_out_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.blur_sampler) },
            ],
        });
        scene::Target { _msaa: msaa, msaa_view, _color: color, view, _depth: depth_tex, depth_view, bind, size, samples, last: None, used: 0 }
    }

    /// Draw a list into a target. Returns what the frame cost the GPU's
    /// queue, for the trace and the budgets (10 §1).
    pub fn render(&mut self, tex: &mut SessionTextures, target: Target<'_>, list: &DrawList, atlas: &mut Atlas, images: &mut ImageAtlas) -> RenderStats {
        let Target { view, format, size, origin, clear, now, age, shift, scale, alpha } = target;
        let frame = [shift.0, shift.1, scale, alpha];
        let clock = [age, spin_phase(now), 0.0, 0.0];
        let mut stats = RenderStats { quads: list.quads.len(), runs: list.runs.len(), ..RenderStats::default() };
        stats.atlas_bytes = self.sync_atlas(tex, atlas, images);
        // The same list as last time — a spinner, a transition the vertex
        // stage interpolates — is already in the buffer. The clock below
        // is what moves; the instances do not.
        let same = list.serial != 0 && tex.uploaded == list.serial;
        if list.quads.len() > tex.instance_cap {
            tex.instance_cap = list.quads.len().next_power_of_two();
            tex.instances = Self::make_instances(&self.device, tex.instance_cap);
            tex.uploaded = 0;
        }
        if same {
            stats.upload_skipped = true;
        } else {
            if !list.quads.is_empty() {
                self.queue.write_buffer(&tex.instances, 0, bytemuck::cast_slice(&list.quads));
                stats.instance_bytes = std::mem::size_of_val(list.quads.as_slice());
            }
            tex.uploaded = list.serial;
        }

        // Built (or found) before the encoder, so the pass below can hold it
        // alongside the immutable borrows of the buffers it also needs.
        self.pipeline_for(format);
        if !self.pipelines.contains_key(&format) {
            return stats;
        }

        // Scenes first: the quads below sample what these passes leave, and
        // the backdrop below that has to be able to see them -- a frosted
        // pane over a cube shows the cube, not a hole where one was.
        let scene_passes = self.render_scenes(tex, list, now, age);
        if scene_passes > 0 {
            stats.passes += scene_passes;
            stats.submits += 1;
        }

        // 03 §2: a frame with a `blur` in it snapshots what is behind the
        // blurred nodes and convolves it, in passes of its own, before the
        // one below. A frame with none — every frame of most applications —
        // reaches none of this and stays the single pass it always was.
        let backdrop = list.backdrop.as_ref().filter(|b| !b.sigmas.is_empty());
        let blurred = match backdrop {
            Some(b) => {
                // One snapshot pass, then a reduce and two Gaussian passes
                // per distinct radius, in a submit of their own.
                stats.passes += 1 + 3 * b.sigmas.len();
                stats.submits += 1;
                self.render_backdrop(tex, format, clock, list, b)
            }
            None => false,
        };
        // The region's corner, and the extent of the textures rather than
        // the region's own size: the region sits in their top-left corner,
        // so a fragment maps into them by the extent.
        let region = match (backdrop, tex.blur.as_ref().filter(|_| blurred)) {
            (Some(b), Some(blur)) => [b.rect[0] as f32, b.rect[1] as f32, blur.extent.0 as f32, blur.extent.1 as f32],
            _ => [0.0; 4],
        };
        let outs: &[BlurChain] = match tex.blur.as_ref().filter(|_| blurred) {
            Some(blur) => &blur.chains,
            None => &[],
        };
        self.queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&uniforms([size.0 as f32, size.1 as f32, 0.0, 0.0, region[0], region[1], region[2], region[3]], clock, frame, list)));
        stats.gpu_ms = self.read_timing();
        let Some(pipeline) = self.pipelines.get(&format) else {
            return stats;
        };

        let c = list.clear;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("eui frame") });
        {
            let timestamp_writes = self.timing.as_ref().filter(|t| t.waiting.is_none()).map(|t| wgpu::RenderPassTimestampWrites {
                query_set: &t.set,
                beginning_of_pass_write_index: Some(0),
                end_of_pass_write_index: Some(1),
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("eui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if clear {
                            wgpu::LoadOp::Clear(wgpu::Color { r: f64::from(c[0]), g: f64::from(c[1]), b: f64::from(c[2]), a: f64::from(c[3]) })
                        } else {
                            // A list drawn over one already in the target:
                            // keep what is there and paint into our own
                            // rectangle. A `Clear` here would wipe the whole
                            // view, chrome included — `LoadOp` has no notion
                            // of a sub-rect, which is what the scissor below
                            // is for.
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
            });
            // The list drew itself as though it owned a window of `size`;
            // the viewport puts that window where it belongs in the view.
            pass.set_viewport(origin.0 as f32, origin.1 as f32, size.0 as f32, size.1 as f32, 0.0, 1.0);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_bind, &[]);
            pass.set_bind_group(1, &tex.atlas_bind, &[]);
            pass.set_vertex_buffer(0, tex.instances.slice(..));
            let mut bound = (u32::MAX, u32::MAX);
            for run in &list.runs {
                let Some(r) = list.clips.get(run.clip as usize) else {
                    continue;
                };
                // A clip says where the layout put a scroller's window. The
                // layer moved everything in it, so the window moves too --
                // otherwise a page at 92 % keeps full-size scissors and shows
                // the over-scrolled rows either side of its own edge.
                let r = layer_clip(*r, size, shift, scale);
                let w = r[2].min(size.0.saturating_sub(r[0]));
                let h = r[3].min(size.1.saturating_sub(r[1]));
                if w == 0 || h == 0 {
                    continue;
                }
                if bound != (run.chain, run.scene) {
                    // Group 2 is one slot with two tenants: the blur a run
                    // samples, or the scene it draws. A run never wants both
                    // -- a scene quad has no backdrop of its own -- so they
                    // share the binding rather than needing a fourth group.
                    let group = match run.scene.checked_sub(1).and_then(|i| list.scenes.get(i as usize)).and_then(|s| tex.scenes.targets.get(&s.id)) {
                        Some(t) => &t.bind,
                        // Checked at the point of use, the way `clips` is
                        // just above: a run naming a target that is not
                        // there draws nothing rather than sampling whatever
                        // group 2 happens to hold.
                        None if run.scene != 0 => continue,
                        None => outs.get(run.chain as usize).map_or(&self.blur_none, |c| &c.out),
                    };
                    pass.set_bind_group(2, group, &[]);
                    bound = (run.chain, run.scene);
                }
                // Scissors are in the view's own pixels, not the
                // viewport's, so this is the one place the origin has to be
                // added back on.
                pass.set_scissor_rect(origin.0.saturating_add(r[0]), origin.1.saturating_add(r[1]), w, h);
                pass.draw(0..6, run.first..run.first.saturating_add(run.count));
            }
        }
        let timed = self.timing.as_ref().is_some_and(|t| t.waiting.is_none());
        if let Some(t) = self.timing.as_mut().filter(|_| timed) {
            encoder.resolve_query_set(&t.set, 0..2, &t.resolve, 0);
            encoder.copy_buffer_to_buffer(&t.resolve, 0, &t.read, 0, 16);
        }
        self.queue.submit([encoder.finish()]);
        // Asked for once, after the submit that fills the buffer, and read
        // whenever it comes back -- never waited on by the frame.
        if let Some(t) = self.timing.as_mut().filter(|_| timed) {
            let (tx, rx) = std::sync::mpsc::channel();
            t.read.slice(..).map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r);
            });
            t.waiting = Some(rx);
        }
        stats.passes += 1;
        stats.submits += 1;
        stats
    }

    /// The timestamps of the frame before, if they have landed: asked for
    /// without waiting, so a frame that is still on the GPU costs nothing
    /// here and is read the frame after. The last figure read is what a
    /// frame reports until the next one lands.
    fn read_timing(&mut self) -> Option<f32> {
        // Nudge the queue only while something is owed; the mapping itself
        // was asked for once, when the frame that wrote the timestamps was
        // submitted, and asking again is what must never happen.
        if self.timing.as_ref()?.waiting.is_some() {
            self.device.poll(wgpu::Maintain::Poll);
        }
        let t = self.timing.as_mut()?;
        match t.waiting.as_ref().and_then(|rx| rx.try_recv().ok()) {
            Some(Ok(())) => {
                let stamps: [u64; 2] = {
                    let data = t.read.slice(..).get_mapped_range();
                    let words: &[u64] = bytemuck::cast_slice(&data);
                    [words.first().copied().unwrap_or(0), words.get(1).copied().unwrap_or(0)]
                };
                t.read.unmap();
                t.waiting = None;
                #[expect(clippy::cast_precision_loss, reason = "a frame's worth of ticks fits a float's mantissa")]
                let ns = stamps[1].saturating_sub(stamps[0]) as f32 * t.period;
                t.last_ms = Some(ns / 1e6);
            }
            // The mapping failed: nothing was mapped, so nothing is
            // unmapped, and the next frame may write the buffer again.
            Some(Err(_)) => t.waiting = None,
            // Still on the GPU. Leave it alone until it is not.
            None => {}
        }
        t.last_ms
    }

    /// Everything a blurred frame needs before its own pass: the frame as it
    /// stood before the first blurred quad, snapshotted over the region that
    /// wants it, then reduced and convolved once per distinct radius.
    ///
    /// Returns whether it ran; the bind groups the main pass hands to its
    /// runs are the chains' own, kept in `tex.blur`. It writes the shared
    /// uniform buffer and submits on its own, so
    /// the caller must write its own uniforms afterwards — a queue's writes
    /// and submits are ordered, and that ordering is what keeps the two
    /// passes reading different values out of one buffer.
    fn render_backdrop(&mut self, tex: &mut SessionTextures, format: wgpu::TextureFormat, clock: [f32; 4], list: &DrawList, b: &Backdrop) -> bool {
        self.blur_pipeline_for(format);
        self.blur_targets(tex, format, b);
        let need = BLUR_STRIDE * 3 * b.sigmas.len() as u64;
        if self.blur_params.size() < need {
            self.blur_params = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("blur params"),
                size: need,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.blur_params_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur params"),
                layout: &self.blur_params_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.blur_params, offset: 0, size: wgpu::BufferSize::new(BLUR_PARAMS) }),
                }],
            });
        }
        let (Some(blur), Some((reduce, gauss))) = (tex.blur.as_ref(), self.blur_pipelines.get(&format)) else {
            return false;
        };
        let (rx, ry) = (b.rect[0], b.rect[1]);
        let (rw, rh) = (b.rect[2].max(1).min(blur.extent.0), b.rect[3].max(1).min(blur.extent.1));

        // One `Params` per pass, laid out at the offsets the draws will name.
        let mut params = vec![0.0f32; (need / 4) as usize];
        let stride = (BLUR_STRIDE / 4) as usize;
        // The reduce reads the snapshot only as far as the region reaches:
        // past it, the texture holds whatever an earlier frame's region
        // left, and a tap there would bleed it in. It clamps to the region
        // instead, which is what the sampler's clamp-to-edge did when the
        // texture was the region's own size -- and, run over the whole
        // chain texture, it leaves the region's edge repeated out to the
        // texture's, so the Gaussians after it need no clamp of their own.
        let (ew, eh) = (blur.extent.0 as f32, blur.extent.1 as f32);
        for (i, (sigma, c)) in b.sigmas.iter().zip(&blur.chains).enumerate() {
            let (w, h) = (c.size.0 as f32, c.size.1 as f32);
            let s = sigma / c.d as f32;
            let slots = [[1.0 / ew, 1.0 / eh, 0.0, 0.0, c.d as f32, 0.0, rw as f32, rh as f32], [1.0 / w, 1.0 / h, 1.0, 0.0, 0.0, s, w, h], [1.0 / w, 1.0 / h, 0.0, 1.0, 0.0, s, w, h]];
            for (j, slot) in slots.iter().enumerate() {
                let at = (i * 3 + j) * stride;
                if let Some(dst) = params.get_mut(at..at + slot.len()) {
                    dst.copy_from_slice(slot);
                }
            }
        }
        self.queue.write_buffer(&self.blur_params, 0, bytemuck::cast_slice(&params));
        // The snapshot draws a sub-rect of the frame, so the vertex stage is
        // told where the target's own origin sits in it.
        self.queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&uniforms([rw as f32, rh as f32, rx as f32, ry as f32, 0.0, 0.0, 0.0, 0.0], clock, IDENTITY_LAYER, list)));

        let c = list.clear;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("eui backdrop") });
        if let Some(pipeline) = self.pipelines.get(&format) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("backdrop"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &blur.snap_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: f64::from(c[0]), g: f64::from(c[1]), b: f64::from(c[2]), a: f64::from(c[3]) }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // The region is drawn into the snapshot's top-left corner, at
            // its own size: the uniforms above say the target is `rw × rh`.
            pass.set_viewport(0.0, 0.0, rw as f32, rh as f32, 0.0, 1.0);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_bind, &[]);
            pass.set_bind_group(1, &tex.atlas_bind, &[]);
            pass.set_vertex_buffer(0, tex.instances.slice(..));
            for run in &list.runs {
                // Only what was painted before the first blurred quad is the
                // backdrop; the rest of the frame is not behind itself.
                let count = run.count.min(b.first.saturating_sub(run.first));
                let Some(r) = list.clips.get(run.clip as usize) else {
                    continue;
                };
                if count == 0 || run.first >= b.first {
                    continue;
                }
                // The scissor is in the frame's coordinates and the target
                // is the region, so it moves with it.
                let x0 = r[0].max(rx);
                let y0 = r[1].max(ry);
                let x1 = r[0].saturating_add(r[2]).min(rx.saturating_add(rw));
                let y1 = r[1].saturating_add(r[3]).min(ry.saturating_add(rh));
                if x1 <= x0 || y1 <= y0 {
                    continue;
                }
                // A scene is part of its own backdrop. This pass replays the
                // frame's early runs into the snapshot, and binding the
                // empty texture to every one of them -- which is what this
                // did before scenes existed -- would leave a hole exactly
                // where a frosted pane sits over a cube. Bound inside the
                // loop rather than once outside it, because which run wants
                // which texture is now a property of the run.
                let group = match run.scene.checked_sub(1).and_then(|i| list.scenes.get(i as usize)).and_then(|s| tex.scenes.targets.get(&s.id)) {
                    Some(t) => &t.bind,
                    None if run.scene != 0 => continue,
                    None => &self.blur_none,
                };
                pass.set_bind_group(2, group, &[]);
                pass.set_scissor_rect(x0 - rx, y0 - ry, x1 - x0, y1 - y0);
                pass.draw(0..6, run.first..run.first.saturating_add(count));
            }
        }
        for (i, c) in blur.chains.iter().enumerate() {
            // Reduce into `a`, convolve across into `b`, down into `a`.
            let steps = [(reduce, &c.from_snap, &c.a_view), (gauss, &c.from_a, &c.b_view), (gauss, &c.from_b, &c.a_view)];
            for (j, (pipeline, src, target)) in steps.into_iter().enumerate() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blur"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &self.blur_params_bind, &[((i * 3 + j) as u64 * BLUR_STRIDE) as u32]);
                pass.set_bind_group(1, src, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        true
    }

    /// An off-screen target.
    pub fn offscreen(&self, width: u32, height: u32) -> Offscreen {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        Offscreen { texture, width, height }
    }

    /// Draw into an off-screen target.
    pub fn render_offscreen(&mut self, tex: &mut SessionTextures, target: &Offscreen, now: f64, list: &DrawList, atlas: &mut Atlas, images: &mut ImageAtlas) -> RenderStats {
        self.render_offscreen_at(tex, target, now, 0.0, list, atlas, images)
    }

    /// Draw into an off-screen target a list `age` seconds after it was
    /// painted, for a test that wants to see a transition part way.
    #[expect(clippy::too_many_arguments, reason = "a test's view of render(): two clocks and the four things a frame is drawn from")]
    pub fn render_offscreen_at(&mut self, tex: &mut SessionTextures, target: &Offscreen, now: f64, age: f32, list: &DrawList, atlas: &mut Atlas, images: &mut ImageAtlas) -> RenderStats {
        let view = target.texture.create_view(&Default::default());
        self.render(tex, Target::whole(&view, FORMAT, (target.width, target.height), now).aged(age), list, atlas, images)
    }

    /// Draw into an off-screen target as one layer of a frame: moved,
    /// scaled and faded as a page mid-transition is (03 §5).
    #[expect(clippy::too_many_arguments, reason = "a test's view of render(): the layer, two clocks and the four things a frame is drawn from")]
    pub fn render_offscreen_layered(
        &mut self,
        tex: &mut SessionTextures,
        target: &Offscreen,
        age: f32,
        layer: ((f32, f32), f32, f32),
        list: &DrawList,
        atlas: &mut Atlas,
        images: &mut ImageAtlas,
    ) -> RenderStats {
        let view = target.texture.create_view(&Default::default());
        let t = Target::whole(&view, FORMAT, (target.width, target.height), 0.0).aged(age).layered(layer.0, layer.1, layer.2);
        self.render(tex, t, list, atlas, images)
    }

    /// Read an off-screen target back as tightly packed sRGB RGBA8.
    pub fn read_back(&self, target: &Offscreen) -> Result<Vec<u8>, RenderError> {
        let bpr = (target.width * 4).div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(bpr) * u64::from(target.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::ImageCopyTexture { texture: &target.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::ImageCopyBuffer { buffer: &buffer, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(target.height) } },
            wgpu::Extent3d { width: target.width, height: target.height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv().map_err(|e| RenderError::ReadBack(e.to_string()))?.map_err(|e| RenderError::ReadBack(e.to_string()))?;
        let data = slice.get_mapped_range();
        let row = (target.width * 4) as usize;
        let mut out = Vec::with_capacity(row * target.height as usize);
        for y in 0..target.height as usize {
            let start = y * bpr as usize;
            if let Some(r) = data.get(start..start + row) {
                out.extend_from_slice(r);
            }
        }
        drop(data);
        buffer.unmap();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blur_region_that_moves_or_grows_a_little_keeps_its_textures() {
        // Keyed on the exact rectangle, a panel sliding in reallocated
        // every texture every frame. The extent is a bucket: the position
        // is not in it at all, and a size inside the same bucket is the
        // same extent.
        assert_eq!(blur_extent(300, 200, 8192), (384, 256));
        assert_eq!(blur_extent(301, 250, 8192), (384, 256), "a region a few pixels larger keeps the bucket");
        assert_eq!(blur_extent(385, 257, 8192), (512, 384), "and one past it takes the next");
        assert_eq!(blur_extent(0, 0, 8192), (128, 128), "never empty");
        assert_eq!(blur_extent(8190, 10, 8192), (8192, 128), "never past what the device allows");
        // Every reduction divides the extent, so a chain's texture is
        // exactly `extent / d` and a frame pixel lands on it unrounded.
        for d in [1, 2, 4, 8, 16] {
            assert_eq!(blur_extent(333, 1, 8192).0 % d, 0);
        }
        assert_eq!(reduce_factor(40.0), 16);
    }

    #[test]
    fn the_scene_module_dropped_first_is_the_one_drawn_longest_ago() {
        let h = |b: u8| [b; 32];
        let held = [(h(1), 5), (h(2), 3), (h(3), 9)];
        assert_eq!(lru_victim(held.into_iter(), 9), Some(h(2)), "the oldest");
        // One drawn this frame is never the victim, even if it is the only
        // candidate: its pipeline is about to be used.
        assert_eq!(lru_victim([(h(4), 7)].into_iter(), 7), None);
        assert_eq!(lru_victim([(h(6), 1), (h(5), 1)].into_iter(), 9), Some(h(5)), "ties by hash, not by map order");
    }
}
