//! The wgpu side: one pipeline, one instance buffer, one atlas texture.

use std::fmt;

use crate::atlas::Atlas;
use crate::paint::{DrawList, Quad};

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

/// The output format for every target, on screen or off.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// GPU state that lives for the session.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    uniform_bind: wgpu::BindGroup,
    atlas_layout: wgpu::BindGroupLayout,
    atlas_tex: wgpu::Texture,
    atlas_bind: wgpu::BindGroup,
    atlas_size: u32,
    instances: wgpu::Buffer,
    instance_cap: usize,
    adapter_name: String,
}

impl fmt::Debug for Renderer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Renderer").field("adapter", &self.adapter_name).field("atlas", &self.atlas_size).finish()
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
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or(RenderError::NoAdapter)?;
        Self::with_adapter(&adapter)
    }

    /// A renderer on an adapter the caller chose (for a window surface).
    pub fn with_adapter(adapter: &wgpu::Adapter) -> Result<Self, RenderError> {
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("eui"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
            },
            None,
        ))
        .map_err(|e| RenderError::Device(e.to_string()))?;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("eui quad"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("eui"),
            bind_group_layouts: &[&uniform_layout, &atlas_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("eui quad"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Quad>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
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

        let atlas_size = Atlas::INITIAL;
        let (atlas_tex, atlas_bind) = Self::make_atlas(&device, &atlas_layout, atlas_size);

        Ok(Self {
            device,
            queue,
            pipeline,
            uniforms,
            uniform_bind,
            atlas_layout,
            atlas_tex,
            atlas_bind,
            atlas_size,
            instances,
            instance_cap,
            adapter_name: adapter.get_info().name,
        })
    }

    fn make_atlas(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, size: u32) -> (wgpu::Texture, wgpu::BindGroup) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let view = tex.create_view(&Default::default());
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        (tex, bind)
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

    /// Upload the atlas if it changed, growing the texture with it.
    fn sync_atlas(&mut self, atlas: &mut Atlas) {
        if atlas.size() != self.atlas_size {
            let (tex, bind) = Self::make_atlas(&self.device, &self.atlas_layout, atlas.size());
            self.atlas_tex = tex;
            self.atlas_bind = bind;
            self.atlas_size = atlas.size();
        }
        if !atlas.is_dirty() {
            return;
        }
        self.queue.write_texture(
            wgpu::ImageCopyTexture { texture: &self.atlas_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            atlas.pixels(),
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(atlas.size()), rows_per_image: Some(atlas.size()) },
            wgpu::Extent3d { width: atlas.size(), height: atlas.size(), depth_or_array_layers: 1 },
        );
        atlas.mark_clean();
    }

    /// Draw a list into a target view of the given device size.
    pub fn render(&mut self, view: &wgpu::TextureView, size: (u32, u32), list: &DrawList, atlas: &mut Atlas) {
        self.sync_atlas(atlas);
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
        self.queue.write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]));

        let c = list.clear;
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("eui frame") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("eui"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: f64::from(c[0]), g: f64::from(c[1]), b: f64::from(c[2]), a: f64::from(c[3]) }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bind, &[]);
            pass.set_bind_group(1, &self.atlas_bind, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            for (clip, first, count) in &list.runs {
                let Some(r) = list.clips.get(*clip as usize) else { continue };
                let w = r[2].min(size.0.saturating_sub(r[0]));
                let h = r[3].min(size.1.saturating_sub(r[1]));
                if w == 0 || h == 0 {
                    continue;
                }
                pass.set_scissor_rect(r[0], r[1], w, h);
                pass.draw(0..6, *first..first.saturating_add(*count));
            }
        }
        self.queue.submit([encoder.finish()]);
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
    pub fn render_offscreen(&mut self, target: &Offscreen, list: &DrawList, atlas: &mut Atlas) {
        let view = target.texture.create_view(&Default::default());
        self.render(&view, (target.width, target.height), list, atlas);
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
