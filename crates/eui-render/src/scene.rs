//! A 3D scene, rendered into its own target and composited as one quad.
//!
//! The whole design is in that sentence. A scene never draws into the frame:
//! it draws into a texture of its own, with its own depth buffer, and the
//! list then carries a single [`crate::SCENE`] quad that samples it. Four
//! things follow, and each of them is a thing not written:
//!
//! - the main pipeline keeps `depth_stencil: None`, so no application that
//!   has no scene pays for a depth buffer the size of its window;
//! - the scissor of a scroller, the rounded corner of a style, the opacity
//!   and slide of a page in transition all reach the scene through the quad,
//!   applied by a vertex stage that was already doing it for everything
//!   else. An inlined 3D pass would have had to reimplement every one of
//!   them inside each shader a server wrote;
//! - a tiled GPU -- which is to say every phone -- is not asked to store and
//!   reload the whole framebuffer partway through the frame;
//! - and the blast radius of a hostile shader is one texture the size of one
//!   node, which is the same argument `SessionTextures` already makes about
//!   the glyph atlas.
//!
//! What it costs, and it is worth saying: one target and one extra sample
//! per scene per frame, and a bilinear resample when a page transition
//! scales the layer -- the same softness the glyphs already take during a
//! transition.

use std::collections::HashMap;

/// The uniform block every scene shader is given, and the only thing bound
/// to one.
///
/// Exactly 128 bytes, which is what `eui-shader` checks a module's own
/// declaration against: a server does not define this struct, it receives
/// it, and a module that declared one of its own is refused by the verifier
/// rather than by the driver -- whose message would name the driver (08 §8).
///
/// The split matters. `mvp`, `time` and `size` are the client's, computed
/// here; `params` and `tint` are the author's eight floats, carried on the
/// wire. So a server never sends a matrix, and therefore can never send a
/// degenerate one.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SceneUniforms {
    /// Projection × view × model, column-major.
    pub mvp: [f32; 16],
    /// `(time, age, 0, 0)` in seconds: the window's clock, and the age of
    /// the list. The first for something that turns; the second for
    /// something that should behave like a transition.
    pub time: [f32; 4],
    /// `(width, height, scale, aspect)` of the target, in device pixels.
    pub size: [f32; 4],
    /// The author's first four floats.
    pub params: [f32; 4],
    /// The author's second four.
    pub tint: [f32; 4],
}

/// The stride of one vertex: position, normal, texture coordinate.
pub const VERTEX_BYTES: u64 = 32;

/// One vertex of a mesh, in the layout the window uploads without reading.
///
/// The worker validates a mesh and hands over exactly this; the window
/// memcpies it into a buffer. That division is the one place where the
/// worker genuinely protects the window: wgpu checks a draw's index *range*
/// against the size of the index buffer, and never checks index *values*
/// against the size of the vertex buffer. No driver will do it either.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Model space.
    pub pos: [f32; 3],
    /// Unit length, model space.
    pub normal: [f32; 3],
    /// Texture coordinate, for a shader that wants one.
    pub uv: [f32; 2],
}

/// Targets are sized to a multiple of this many device pixels.
///
/// The lesson is already in `Blur`, which keys on its reduction and not on
/// its sigma so that an animating blur does not reallocate sixty times a
/// second. A node being dragged wider, or scaled by a page transition, would
/// do the same to a scene. Rounding up and cropping with the quad's own `uv`
/// costs nothing -- the field is in every quad already -- and turns a
/// reallocation per frame into one per sixty-four pixels.
pub const QUANTUM: u32 = 64;

/// Targets one session may hold at once.
///
/// A quota in the sense of 08 §6, and the case it exists for is concrete: a
/// virtualised `list` of ten thousand rows, each with a scene, must not be
/// able to ask for ten thousand render targets. Past this a scene draws its
/// own background, which is what a scene whose module has not compiled yet
/// already does — so the failure mode is one the application has to handle
/// anyway.
pub const MAX_TARGETS: usize = 8;

/// The largest target a scene gets, before the device's own limit trims it.
///
/// The same number `MAX_IMAGE_EDGE` uses, and for the same reason: past it,
/// the picture is upscaled rather than refused. A slightly soft scene beats
/// no scene, and a full-screen 4K window at 2× would otherwise ask for 7 680
/// pixels -- which a GLES adapter, capped at 2 048, cannot give at all.
pub const MAX_EDGE: u32 = 4096;

/// What a scene's pipeline is built for. Deliberately not the window's
/// colour format: a target is always [`crate::FORMAT`], so one shader is
/// compiled once per process rather than once per surface format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    /// BLAKE3 of the module, or all zero for the client's own.
    pub shader: [u8; 32],
    /// Whether the pipeline writes and tests depth.
    pub depth: bool,
    /// Samples per pixel: 1, or 4 where the adapter can.
    pub samples: u32,
}

/// The colour target a scene is drawn into, its depth buffer, and the bind
/// group the composite quad samples it through.
pub(crate) struct Target {
    /// The multisampled colour attachment, when there is one. Drawn into and
    /// then resolved down to `_color` in the same pass, so nothing samples
    /// it and nothing needs to.
    pub _msaa: Option<wgpu::Texture>,
    /// Its view, which is what the pass attaches.
    pub msaa_view: Option<wgpu::TextureView>,
    /// Held only to keep the view above alive: dropping the texture takes
    /// the contents of every view onto it. Never read directly, and
    /// deliberately not an `Offscreen` -- that is the type `read_back`
    /// accepts, and this one has no `COPY_SRC` to read back with.
    pub _color: wgpu::Texture,
    pub view: wgpu::TextureView,
    /// Kept beside its view because dropping the texture would take the
    /// view's contents with it.
    pub _depth: Option<wgpu::Texture>,
    pub depth_view: Option<wgpu::TextureView>,
    /// Group 2 of the quad pipeline: the same layout a finished blur binds,
    /// which is why a scene needs no new bind group layout and no change to
    /// the pipeline layout at all.
    pub bind: wgpu::BindGroup,
    pub size: (u32, u32),
    /// Samples it was made with, so a change of `msaa` remakes it.
    pub samples: u32,
    /// What was last drawn into it. A scene whose uniforms, clock and size
    /// have not moved holds the right pixels already, so the pass is not
    /// encoded again -- which is what makes a still scene genuinely free
    /// rather than merely cheap. It is `gpu.rs`'s `uploaded == serial`
    /// trick, one level up.
    pub last: Option<SceneUniforms>,
    /// The frame this target was last asked for. One frame of hysteresis
    /// before it is dropped: a scene in a virtualised list crosses the edge
    /// of the viewport repeatedly, and `Blur` -- which evicts at once -- is
    /// right for one backdrop and wrong for that.
    pub used: u64,
}

/// A mesh's two buffers and how many indices to draw.
pub(crate) struct Mesh {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub count: u32,
}

/// Everything a session's scenes own. Per session, like the atlases and for
/// the same reason: these are data a server chose, and one texture behind
/// two sessions is one application sampling another's.
#[derive(Default)]
pub(crate) struct Scenes {
    pub targets: HashMap<u64, Target>,
    pub meshes: HashMap<[u8; 32], Mesh>,
    /// One buffer, one 256-byte slot per scene in the frame, bound by
    /// dynamic offset -- the shape `blur_params` already uses, and 256 is
    /// wgpu's floor for `min_uniform_buffer_offset_alignment`. Allocated
    /// the first time a session has a scene, and never for one that has
    /// none.
    pub uniforms: Option<(wgpu::Buffer, wgpu::BindGroup)>,
    /// How many slots that buffer holds.
    pub slots: usize,
    /// Frames drawn, for the hysteresis above.
    pub frame: u64,
}

/// The stride of one scene's uniform slot.
///
/// wgpu's floor for `min_uniform_buffer_offset_alignment`, which is the same
/// number `BLUR_STRIDE` uses and for the same reason.
pub const SLOT: u64 = 256;

/// The depth format a scene tests against.
///
/// `Depth32Float` because it is available everywhere. `Depth24PlusStencil8`
/// is not, and a scene that draws on one machine and not another is a worse
/// failure than a depth buffer that costs four bytes a pixel.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The uniform block for one scene, this frame.
///
/// The camera is the client's, always. A server sends eight floats and a
/// clock bit; it does not send a matrix, so it cannot send a singular one,
/// and the near and far planes are not its business either.
#[must_use]
pub fn uniforms_for(draw: &crate::paint::SceneDraw, size: (u32, u32), now: f64, age: f32) -> SceneUniforms {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    // Reduced before it is narrowed. `now` is seconds since the window
    // opened, and after a few hours an f32 of it has lost the milliseconds
    // -- which is a scene that visibly steps rather than turns. `spin_phase`
    // already solves this for a spinner by working in f64 and sending only a
    // fraction; this does the same over a minute.
    const PERIOD: f64 = 60.0;
    let t = (now.rem_euclid(PERIOD)) as f32;
    let spin = if draw.flags & crate::SCENE_ANIMATED != 0 { t } else { 0.0 };
    let model = mul(rotate_y(spin * 0.9), rotate_x(spin * 0.55));
    let view = look_at([2.4, 1.9, 3.4]);
    let proj = perspective(50f32.to_radians(), w / h, 0.1, 100.0);
    SceneUniforms {
        mvp: mul(proj, mul(view, model)),
        time: [t, age, 0.0, 0.0],
        size: [w, h, 1.0, w / h],
        params: [draw.uniforms[0], draw.uniforms[1], draw.uniforms[2], draw.uniforms[3]],
        tint: [draw.uniforms[4], draw.uniforms[5], draw.uniforms[6], draw.uniforms[7]],
    }
}

/// The size a target is actually made at, for a node this big.
#[must_use]
pub fn target_size(rect: [f32; 4], max: u32) -> (u32, u32) {
    let round = |v: f32| {
        // Never zero: a target of no pixels is not a target wgpu will make.
        let px = if v.is_finite() { v.max(1.0) } else { 1.0 };
        #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "clamped to a finite positive below the edge cap")]
        let px = px.min(f32::from(u16::MAX)) as u32;
        let up = px.saturating_add(QUANTUM.saturating_sub(1)) / QUANTUM * QUANTUM;
        up.clamp(QUANTUM, MAX_EDGE.min(max.max(QUANTUM)))
    };
    (round(rect.get(2).copied().unwrap_or(1.0)), round(rect.get(3).copied().unwrap_or(1.0)))
}

/// A unit cube, four vertices a face so that each face has its own normal
/// and reads as a face rather than as a smooth blob.
///
/// The client carries one mesh of its own for the same reason it carries one
/// shader of its own: so that the renderer can be brought up, and tested,
/// against a picture no server had to send.
#[must_use]
pub fn cube() -> (Vec<Vertex>, Vec<u32>) {
    // Each face as (normal, four corners anticlockwise seen from outside).
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([0.0, 0.0, 1.0], [[-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0]]),
        ([0.0, 0.0, -1.0], [[1.0, -1.0, -1.0], [-1.0, -1.0, -1.0], [-1.0, 1.0, -1.0], [1.0, 1.0, -1.0]]),
        ([1.0, 0.0, 0.0], [[1.0, -1.0, 1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [1.0, 1.0, 1.0]]),
        ([-1.0, 0.0, 0.0], [[-1.0, -1.0, -1.0], [-1.0, -1.0, 1.0], [-1.0, 1.0, 1.0], [-1.0, 1.0, -1.0]]),
        ([0.0, 1.0, 0.0], [[-1.0, 1.0, 1.0], [1.0, 1.0, 1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0]]),
        ([0.0, -1.0, 0.0], [[-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, -1.0, 1.0], [-1.0, -1.0, 1.0]]),
    ];
    let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (f, (normal, corners)) in faces.into_iter().enumerate() {
        let base = u32::try_from(f).unwrap_or(0).saturating_mul(4);
        for (i, pos) in corners.into_iter().enumerate() {
            vertices.push(Vertex { pos, normal, uv: uvs.get(i).copied().unwrap_or([0.0, 0.0]) });
        }
        for i in [0, 1, 2, 0, 2, 3] {
            indices.push(base.saturating_add(i));
        }
    }
    (vertices, indices)
}

/// `a × b`, both column-major.
#[must_use]
pub fn mul(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                let (x, y) = (a.get(k * 4 + row).copied().unwrap_or(0.0), b.get(col * 4 + k).copied().unwrap_or(0.0));
                sum += x * y;
            }
            if let Some(slot) = out.get_mut(col * 4 + row) {
                *slot = sum;
            }
        }
    }
    out
}

/// A right-handed perspective onto wgpu's clip space, whose depth runs 0 to
/// 1 rather than -1 to 1. Getting that wrong is a scene that is either
/// entirely clipped or entirely flat, so it is written out rather than
/// borrowed.
#[must_use]
pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_y * 0.5).tan();
    let a = if aspect.is_finite() && aspect > 1e-4 { aspect } else { 1.0 };
    let d = near - far;
    [f / a, 0.0, 0.0, 0.0, 0.0, f, 0.0, 0.0, 0.0, 0.0, far / d, -1.0, 0.0, 0.0, near * far / d, 0.0]
}

/// Looking from `eye` at the origin, with +y up.
#[must_use]
pub fn look_at(eye: [f32; 3]) -> [f32; 16] {
    let norm = |v: [f32; 3]| {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        [v[0] / len, v[1] / len, v[2] / len]
    };
    let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    // The camera looks down its own -z, so the basis vector is eye - target
    // and the target is the origin.
    let z = norm(eye);
    let x = norm(cross([0.0, 1.0, 0.0], z));
    let y = cross(z, x);
    [x[0], y[0], z[0], 0.0, x[1], y[1], z[1], 0.0, x[2], y[2], z[2], 0.0, -dot(x, eye), -dot(y, eye), -dot(z, eye), 1.0]
}

/// A turn of `a` radians about +y.
#[must_use]
pub fn rotate_y(a: f32) -> [f32; 16] {
    let (s, c) = a.sin_cos();
    [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0]
}

/// A turn of `a` radians about +x.
#[must_use]
pub fn rotate_x(a: f32) -> [f32; 16] {
    let (s, c) = a.sin_cos();
    [1.0, 0.0, 0.0, 0.0, 0.0, c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0]
}
