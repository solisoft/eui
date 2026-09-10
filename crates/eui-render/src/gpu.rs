//! The wgpu side: a pipeline per target format, one instance buffer, one
//! atlas texture.

use std::collections::HashMap;
use std::fmt;

use crate::atlas::{Atlas, ImageAtlas};
use crate::paint::{Backdrop, DrawList, Quad};

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
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter => f.write_str("no GPU adapter available"),
            Self::Device(e) => write!(f, "device request failed: {e}"),
            Self::ReadBack(e) => write!(f, "read-back failed: {e}"),
        }
    }
}

impl std::error::Error for RenderError {}

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
    instances: wgpu::Buffer,
    instance_cap: usize,
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
    adapter_name: String,
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
}

impl fmt::Debug for SessionTextures {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionTextures").field("atlas", &self.atlas_size).field("images", &self.img_size).finish()
    }
}

/// The off-screen textures a blurred frame works in.
struct Blur {
    /// Format, region, and the *reduction* each chain works at — which is
    /// everything the sizes below depend on, and deliberately not the
    /// standard deviations themselves. An entrance animates a radius from
    /// zero over some tenths of a second (03 §5); keying on the radius
    /// would throw every texture away sixty times a second to redraw the
    /// same sizes, where keying on the reduction reallocates about four
    /// times over the whole animation.
    key: (wgpu::TextureFormat, [u32; 4], Vec<u32>),
    /// The frame as it stood before the first blurred quad, at the region's
    /// own size.
    snap: wgpu::Texture,
    /// Per chain: the reduction factor, and the two textures the separable
    /// Gaussian ping-pongs between. The result ends in the first.
    chains: Vec<(u32, wgpu::Texture, wgpu::Texture)>,
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
/// size in device pixels, and seconds on the client's clock, which the
/// vertex stage animates `spin` from.
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
    /// Seconds since the window started.
    pub now: f32,
}

impl<'a> Target<'a> {
    /// A target that is the whole view: no offset, and it clears.
    pub fn whole(view: &'a wgpu::TextureView, format: wgpu::TextureFormat, size: (u32, u32), now: f32) -> Self {
        Self { view, format, size, origin: (0, 0), clear: true, now }
    }
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
    pub fn new_headless() -> Result<Self, RenderError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::all(), ..Default::default() });
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::LowPower, compatible_surface: None, force_fallback_adapter: false }))
                .ok_or(RenderError::NoAdapter)?;
        Self::with_adapter(&adapter)
    }

    /// A renderer on an adapter the caller chose (for a window surface).
    pub fn with_adapter(adapter: &wgpu::Adapter) -> Result<Self, RenderError> {
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("eui"),
                required_features: wgpu::Features::empty(),
                // Downlevel limits cap textures at 2048 px, which a
                // high-DPI window exceeds on its first frame. Ask for the
                // ordinary defaults, trimmed to what the adapter has.
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
            },
            None,
        ))
        .map_err(|e| RenderError::Device(e.to_string()))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("eui quad"), source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()) });

        let blur_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("eui blur"), source: wgpu::ShaderSource::Wgsl(include_str!("blur.wgsl").into()) });

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
            // viewport, backdrop, clock.
            size: 48,
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

        let instance_cap = 4096;
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (instance_cap * std::mem::size_of::<Quad>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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
            instances,
            instance_cap,
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
            adapter_name: adapter.get_info().name,
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
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4],
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
        SessionTextures { atlas_tex, img_tex, img_size: 1, atlas_bind, atlas_size, blur: None }
    }

    /// The adapter's name, for diagnostics.
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
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
    fn sync_atlas(&self, tex: &mut SessionTextures, atlas: &mut Atlas, images: &mut ImageAtlas) {
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
            images.mark_dirty_all();
        }
        // Only the rows that changed cross to the GPU: a few glyph rows
        // per frame of new text, not a megabyte.
        if let Some((y0, y1)) = images.dirty_rows() {
            self.queue.write_texture(
                wgpu::ImageCopyTexture { texture: &tex.img_tex, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: y0, z: 0 }, aspect: wgpu::TextureAspect::All },
                images.rows(y0, y1),
                wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(images.size() * 4), rows_per_image: Some(y1 - y0) },
                wgpu::Extent3d { width: images.size(), height: y1 - y0, depth_or_array_layers: 1 },
            );
            images.mark_clean();
        }
        let Some((y0, y1)) = atlas.dirty_rows() else {
            return;
        };
        self.queue.write_texture(
            wgpu::ImageCopyTexture { texture: &tex.atlas_tex, mip_level: 0, origin: wgpu::Origin3d { x: 0, y: y0, z: 0 }, aspect: wgpu::TextureAspect::All },
            atlas.rows(y0, y1),
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(atlas.size()), rows_per_image: Some(y1 - y0) },
            wgpu::Extent3d { width: atlas.size(), height: y1 - y0, depth_or_array_layers: 1 },
        );
        atlas.mark_clean();
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
    /// region, the radii or the format have moved since the last frame.
    fn blur_targets(&self, tex: &mut SessionTextures, format: wgpu::TextureFormat, b: &Backdrop) {
        let key = (format, b.rect, b.sigmas.iter().map(|s| reduce_factor(*s)).collect::<Vec<_>>());
        if tex.blur.as_ref().is_some_and(|x| x.key == key) {
            return;
        }
        let (rw, rh) = (b.rect[2].max(1), b.rect[3].max(1));
        let device = &self.device;
        let make = |label: &str, w: u32, h: u32| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        };
        let snap = make("backdrop", rw, rh);
        let chains = b
            .sigmas
            .iter()
            .map(|sigma| {
                let d = reduce_factor(*sigma);
                let (w, h) = (rw / d, rh / d);
                (d, make("blur a", w, h), make("blur b", w, h))
            })
            .collect();
        tex.blur = Some(Blur { key, snap, chains });
    }

    /// Draw a list into a target.
    pub fn render(&mut self, tex: &mut SessionTextures, target: Target<'_>, list: &DrawList, atlas: &mut Atlas, images: &mut ImageAtlas) {
        let Target { view, format, size, origin, clear, now } = target;
        self.sync_atlas(tex, atlas, images);
        if list.quads.len() > self.instance_cap {
            self.instance_cap = list.quads.len().next_power_of_two();
            self.instances = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("instances"),
                size: (self.instance_cap * std::mem::size_of::<Quad>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !list.quads.is_empty() {
            self.queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&list.quads));
        }

        // Built (or found) before the encoder, so the pass below can hold it
        // alongside the immutable borrows of the buffers it also needs.
        self.pipeline_for(format);
        if !self.pipelines.contains_key(&format) {
            return;
        }

        // 03 §2: a frame with a `blur` in it snapshots what is behind the
        // blurred nodes and convolves it, in passes of its own, before the
        // one below. A frame with none — every frame of most applications —
        // reaches none of this and stays the single pass it always was.
        let backdrop = list.backdrop.as_ref().filter(|b| !b.sigmas.is_empty());
        let outs = match backdrop {
            Some(b) => self.render_backdrop(tex, format, now, list, b),
            None => Vec::new(),
        };
        let region = backdrop.map_or([0.0; 4], |b| [b.rect[0] as f32, b.rect[1] as f32, b.rect[2].max(1) as f32, b.rect[3].max(1) as f32]);
        self.queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0, region[0], region[1], region[2], region[3], now, 0.0, 0.0, 0.0]));
        let Some(pipeline) = self.pipelines.get(&format) else {
            return;
        };

        let c = list.clear;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("eui frame") });
        {
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
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            // The list drew itself as though it owned a window of `size`;
            // the viewport puts that window where it belongs in the view.
            pass.set_viewport(origin.0 as f32, origin.1 as f32, size.0 as f32, size.1 as f32, 0.0, 1.0);
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_bind, &[]);
            pass.set_bind_group(1, &tex.atlas_bind, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            let mut bound = u32::MAX;
            for run in &list.runs {
                let Some(r) = list.clips.get(run.clip as usize) else {
                    continue;
                };
                let w = r[2].min(size.0.saturating_sub(r[0]));
                let h = r[3].min(size.1.saturating_sub(r[1]));
                if w == 0 || h == 0 {
                    continue;
                }
                if bound != run.chain {
                    pass.set_bind_group(2, outs.get(run.chain as usize).unwrap_or(&self.blur_none), &[]);
                    bound = run.chain;
                }
                // Scissors are in the view's own pixels, not the
                // viewport's, so this is the one place the origin has to be
                // added back on.
                pass.set_scissor_rect(origin.0.saturating_add(r[0]), origin.1.saturating_add(r[1]), w, h);
                pass.draw(0..6, run.first..run.first.saturating_add(run.count));
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Everything a blurred frame needs before its own pass: the frame as it
    /// stood before the first blurred quad, snapshotted over the region that
    /// wants it, then reduced and convolved once per distinct radius.
    ///
    /// Returns the bind groups the main pass hands to its runs, in chain
    /// order. It writes the shared uniform buffer and submits on its own, so
    /// the caller must write its own uniforms afterwards — a queue's writes
    /// and submits are ordered, and that ordering is what keeps the two
    /// passes reading different values out of one buffer.
    fn render_backdrop(&mut self, tex: &mut SessionTextures, format: wgpu::TextureFormat, now: f32, list: &DrawList, b: &Backdrop) -> Vec<wgpu::BindGroup> {
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
            return Vec::new();
        };
        let (rx, ry) = (b.rect[0], b.rect[1]);
        let (rw, rh) = (b.rect[2].max(1), b.rect[3].max(1));

        // One `Params` per pass, laid out at the offsets the draws will name.
        let mut params = vec![0.0f32; (need / 4) as usize];
        let stride = (BLUR_STRIDE / 4) as usize;
        for (i, (sigma, (d, a, _))) in b.sigmas.iter().zip(&blur.chains).enumerate() {
            let (w, h) = (a.width() as f32, a.height() as f32);
            let s = sigma / *d as f32;
            let slots = [[1.0 / rw as f32, 1.0 / rh as f32, 0.0, 0.0, *d as f32, 0.0], [1.0 / w, 1.0 / h, 1.0, 0.0, 0.0, s], [1.0 / w, 1.0 / h, 0.0, 1.0, 0.0, s]];
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
        self.queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&[rw as f32, rh as f32, rx as f32, ry as f32, 0.0, 0.0, 0.0, 0.0, now, 0.0, 0.0, 0.0]));

        let src_bind = |t: &wgpu::Texture| {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur src"),
                layout: &self.blur_src_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&t.create_view(&Default::default())) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.blur_sampler) },
                ],
            })
        };
        let c = list.clear;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("eui backdrop") });
        if let Some(pipeline) = self.pipelines.get(&format) {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("backdrop"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &blur.snap.create_view(&Default::default()),
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: f64::from(c[0]), g: f64::from(c[1]), b: f64::from(c[2]), a: f64::from(c[3]) }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.uniform_bind, &[]);
            pass.set_bind_group(1, &tex.atlas_bind, &[]);
            pass.set_bind_group(2, &self.blur_none, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
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
                pass.set_scissor_rect(x0 - rx, y0 - ry, x1 - x0, y1 - y0);
                pass.draw(0..6, run.first..run.first.saturating_add(count));
            }
        }
        let mut outs = Vec::with_capacity(blur.chains.len());
        for (i, (_, a, bb)) in blur.chains.iter().enumerate() {
            let (from_snap, from_a, from_b) = (src_bind(&blur.snap), src_bind(a), src_bind(bb));
            let a_view = a.create_view(&Default::default());
            let b_view = bb.create_view(&Default::default());
            // Reduce into `a`, convolve across into `b`, down into `a`.
            let steps = [(reduce, &from_snap, &a_view), (gauss, &from_a, &b_view), (gauss, &from_b, &a_view)];
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
            outs.push(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("blur out"),
                layout: &self.blur_out_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&a.create_view(&Default::default())) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.blur_sampler) },
                ],
            }));
        }
        self.queue.submit([encoder.finish()]);
        outs
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
    pub fn render_offscreen(&mut self, tex: &mut SessionTextures, target: &Offscreen, now: f32, list: &DrawList, atlas: &mut Atlas, images: &mut ImageAtlas) {
        let view = target.texture.create_view(&Default::default());
        self.render(tex, Target::whole(&view, FORMAT, (target.width, target.height), now), list, atlas, images);
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
