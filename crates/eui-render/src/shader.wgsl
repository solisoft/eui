// One shape: a rounded rectangle, optionally textured from the R8 atlas.
// Anti-aliasing comes from the signed distance to the rectangle's edge, so
// the same pipeline draws crisp boxes, hairlines, borders and glyphs.

struct Uniforms {
    // Target width and height, then the frame-space origin the target's own
    // (0, 0) stands for. It is zero for the window and the backdrop's
    // corner for the snapshot pass, which draws a sub-rect of the frame.
    viewport: vec4<f32>,
    // The backdrop region in frame device pixels: x, y, w, h. A blurred
    // fragment maps its own position into this to find its blurred texel.
    backdrop: vec4<f32>,
    // x: the list's age -- seconds since the paint that produced it, which
    // is what an `ANIMATED` instance's start time is relative to. y: the
    // spin phase, a fraction of a revolution (03 §5). Both are computed
    // here rather than baked into the instances, so a spinning or
    // transitioning node does not make the frame a different draw list.
    // Two spare.
    clock: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_smp: sampler;
@group(1) @binding(2) var img_tex: texture_2d<f32>;
@group(1) @binding(3) var img_smp: sampler;
@group(2) @binding(0) var blur_tex: texture_2d<f32>;
@group(2) @binding(1) var blur_smp: sampler;

const TEXTURED: u32 = 1u;       // the glyph atlas, R8, alpha only
const TEXTURED_RGBA: u32 = 2u;  // the image atlas
const BLURRED: u32 = 4u;        // fill over the blurred backdrop
const SPINNING: u32 = 8u;       // turns about its node's centre, 03 §5
const ANIMATED: u32 = 16u;      // fill, stroke, opacity mix from `*_from`
const DECELERATE: u32 = 32u;    // along the entrance curve, not the standard
const TAU: f32 = 6.2831855;

// y of cubic-bezier(x1, 0, x2, 1) at time t, the curve solved for its
// parameter by four Newton steps from s = t. The three curves in use never
// flatten in x, so four steps land within 1e-5 -- the same tolerance the
// CPU stops at (05 §4), and no accuracy is given up by moving here.
fn bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    var s = t;
    for (var i = 0; i < 4; i++) {
        let u = 1.0 - s;
        let x = 3.0 * x1 * u * u * s + 3.0 * x2 * u * s * s + s * s * s - t;
        let dx = 3.0 * x1 * u * (u - 2.0 * s) + 3.0 * x2 * s * (2.0 * u - s) + 3.0 * s * s;
        s = clamp(s - x / max(dx, 1e-4), 0.0, 1.0);
    }
    let u = 1.0 - s;
    return 3.0 * y1 * u * u * s + 3.0 * y2 * u * s * s + s * s * s;
}

// The eased fraction at t: 0 the theme's standard curve, 1 decelerate (an
// entrance), 2 smooth (a keyboard scroll, from rest to rest).
fn ease(t: f32, curve: u32) -> f32 {
    let c = clamp(t, 0.0, 1.0);
    if (c <= 0.0 || c >= 1.0) {
        return c;
    }
    if (curve == 1u) {
        return bezier(c, 0.0, 0.0, 0.2, 1.0);
    }
    if (curve == 2u) {
        return bezier(c, 0.45, 0.0, 0.55, 1.0);
    }
    return bezier(c, 0.2, 0.0, 0.0, 1.0);
}

struct Inst {
    @location(0) rect: vec4<f32>,
    @location(1) params: vec4<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) stroke: vec4<f32>,
    @location(4) uv: vec4<f32>,
    @location(5) extra: vec4<f32>, // x: rotation about the centre, radians; w: opacity from
    @location(6) spin: vec4<f32>,  // xy: offset from the spinning node's centre; zw: t0, duration
    @location(7) fill_from: vec4<f32>,
    @location(8) stroke_from: vec4<f32>,
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) params: vec4<f32>,
    @location(3) fill: vec4<f32>,
    @location(4) stroke: vec4<f32>,
    @location(5) uv: vec2<f32>,
    @location(6) extra: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32, inst: Inst) -> VOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    let c = corners[vi];
    // Rotate about the centre; the SDF below works in unrotated local
    // space, so a canvas segment gets the same anti-aliased edge as a box.
    var centre = inst.rect.xy + inst.rect.zw * 0.5;
    var angle = inst.extra.x;
    // 03 §5 `spin`: the node turns, so this quad turns about its own centre
    // and its centre travels around the node's. The instance carries the
    // offset between the two, so the node's centre is this quad's centre
    // less that offset, and nothing has to be recomputed on the CPU.
    let flags = u32(inst.params.z);
    if ((flags & SPINNING) != 0u) {
        let a = u.clock.y * TAU;
        let o = inst.spin.xy;
        let sc = vec2<f32>(cos(a), sin(a));
        centre = (centre - o) + vec2<f32>(o.x * sc.x - o.y * sc.y, o.x * sc.y + o.y * sc.x);
        angle = angle + a;
    }
    let local = (c - vec2<f32>(0.5, 0.5)) * inst.rect.zw;
    let ca = cos(angle);
    let sa = sin(angle);
    let px = centre + vec2<f32>(local.x * ca - local.y * sa, local.x * sa + local.y * ca);
    let t = px - u.viewport.zw;
    let ndc = vec2<f32>(t.x / u.viewport.x * 2.0 - 1.0, 1.0 - t.y / u.viewport.y * 2.0);
    var out: VOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local = c;
    out.rect = inst.rect;
    out.params = inst.params;
    out.fill = inst.fill;
    out.stroke = inst.stroke;
    out.uv = mix(inst.uv.xy, inst.uv.zw, c);
    out.extra = inst.extra;
    // 03 §5: a transition is both its ends and a clock. The instance holds
    // the ends; the clock is the list's age, so the same list is right for
    // every frame of it.
    if ((flags & ANIMATED) != 0u) {
        let curve = select(0u, 1u, (flags & DECELERATE) != 0u);
        let k = ease((u.clock.x - inst.spin.z) / max(inst.spin.w, 1e-3), curve);
        out.fill = mix(inst.fill_from, inst.fill, k);
        out.stroke = mix(inst.stroke_from, inst.stroke, k);
        out.params.w = mix(inst.extra.w, inst.params.w, k);
    }
    return out;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let half = in.rect.zw * 0.5;
    let p = in.local * in.rect.zw - half;
    let r = min(in.params.x, min(half.x, half.y));
    let q = abs(p) - half + vec2<f32>(r, r);
    let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
    var coverage = 1.0 - clamp(d + 0.5, 0.0, 1.0);
    // A shadow: the rect was grown by the blur; fade from opaque one blur
    // inside the true edge to nothing at the grown edge.
    if (in.extra.y > 0.0) {
        coverage = 1.0 - smoothstep(-2.0 * in.extra.y, 0.0, d);
    }

    let flags = u32(in.params.z);

    var color = in.fill;
    // 03 §2: a blurred node fills over its backdrop rather than over what
    // the target happens to hold, so a translucent fill tints frosted glass
    // instead of merely dimming what is behind it. The backdrop is opaque,
    // so what leaves here is too, and the rounded edge still comes from the
    // coverage below.
    if ((flags & BLURRED) != 0u) {
        let uv = (in.pos.xy + u.viewport.zw - u.backdrop.xy) / u.backdrop.zw;
        let back = textureSample(blur_tex, blur_smp, uv);
        color = vec4<f32>(mix(back.rgb, in.fill.rgb, in.fill.a), 1.0);
    }
    let border = in.params.y;
    if (border > 0.0) {
        let inner = 1.0 - clamp(d + border + 0.5, 0.0, 1.0);
        color = mix(in.stroke, color, inner);
    }
    if ((flags & TEXTURED_RGBA) != 0u) {
        let t = textureSample(img_tex, img_smp, in.uv);
        color = vec4<f32>(t.rgb, t.a * in.fill.a);
    } else if ((flags & TEXTURED) != 0u) {
        color.a = color.a * textureSample(atlas_tex, atlas_smp, in.uv).r;
    }
    let a = color.a * coverage * in.params.w;
    return vec4<f32>(color.rgb * a, a);
}
