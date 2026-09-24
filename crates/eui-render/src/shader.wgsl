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
    // z: the factor a pulsing quad's opacity is multiplied by, and w: how far
    // a bouncing quad is lifted, as a fraction of its node's height -- both
    // 03 §5, both from the window's clock like the spin.
    clock: vec4<f32>,
    // What the whole list is doing, as a layer of the frame: xy the shift in
    // device px, z a uniform scale about the target's own centre, w the
    // opacity everything in it is drawn at. Identity is (0, 0, 1, 1).
    //
    // A page slides as a layer and not as a list of moved quads: the window
    // already holds the list it drew last frame, so a transition costs it
    // one more `render` with a different four floats, and nothing is walked,
    // repainted or sent. It is deliberately *not* `viewport.zw` -- a blurred
    // fragment reads that back to find its backdrop texel, and a frosted bar
    // on a sliding page would then sample at twice the shift.
    frame: vec4<f32>,
    // The scrolls in flight (04 §7), two entries a slot from slot one: the
    // shift the content starts from and ends at, device px, as xy and zw;
    // then when it began relative to the list's clock, how long it takes,
    // and its curve. Slot zero is no scroll, and is never read.
    scroll: array<vec4<f32>, 32>,
    // The subtrees on the move (03 §5), four entries a slot from slot one:
    // where it starts (dx, dy, scale, opacity), where it ends, the clock
    // (t0, duration, curve, and the fraction to use outright when the curve
    // is `held`), and the point the scale is about. Slot zero is nothing.
    xform: array<vec4<f32>, 64>,
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
const SCENE: u32 = 64u;         // a scene's own target, premultiplied
const GRADIENT: u32 = 128u;     // the fill is a linear gradient, 02 §5.3
const PULSING: u32 = 65536u;    // opacity times the clock's pulse, 03 §5
const HELD: u32 = 4u;           // a transform the hand is driving, not the clock
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
// entrance), 2 smooth (a keyboard scroll, from rest to rest), 3 accelerate
// (something leaving).
// 02 §5.3 mixes a gradient's stops in sRGB, as CSS does, and the target is
// linear: the stops go out through the transfer function and come back.
fn to_srgb(c: f32) -> f32 {
    if (c <= 0.0031308) {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

fn to_linear(c: f32) -> f32 {
    if (c <= 0.04045) {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

// A linear colour as four sRGB bytes, which is exactly what a stop was before
// it was resolved: a theme role and a literal are both `0xRRGGBBAA`.
fn pack_srgb(c: vec4<f32>) -> u32 {
    let b = vec4<u32>(round(clamp(vec4<f32>(to_srgb(c.r), to_srgb(c.g), to_srgb(c.b), c.a), vec4<f32>(0.0), vec4<f32>(1.0)) * 255.0));
    return (b.r << 24u) | (b.g << 16u) | (b.b << 8u) | b.a;
}

// Back, premultiplied: the space CSS interpolates a gradient in.
fn unpack_premul(v: u32) -> vec4<f32> {
    let c = vec4<f32>(f32((v >> 24u) & 255u), f32((v >> 16u) & 255u), f32((v >> 8u) & 255u), f32(v & 255u)) / 255.0;
    return vec4<f32>(c.rgb * c.a, c.a);
}

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
    // 05 §2 accelerate, for something leaving.
    if (curve == 3u) {
        return bezier(c, 0.4, 0.0, 1.0, 1.0);
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
    @location(9) bounce: f32,      // the bouncing node's height, device px, 03 §5
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
    // The layer scale this quad was drawn at. The SDF below works in the
    // quad's own unscaled local space, so every distance it computes comes
    // out `1 / scale` device pixels wide -- including the half-pixel the
    // antialiasing ramp is supposed to be. Dividing by it there is what keeps
    // a shrinking page soft-edged and a growing one from going hard.
    @location(7) scale: f32,
    // 02 §5.3: a gradient quad's three stops as sRGB bytes. Flat and packed
    // because the inter-stage budget is 31 components on WebGL2 and the rest
    // of this struct takes 25; the positions arrive in `uv`, already turned
    // into how far along each of the two segments the fragment is.
    @location(8) @interpolate(flat) grad: vec4<u32>,
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
    // 04 §7: a quad in a scroller mid-glide. The layout put it where the
    // glide lands; it is drawn on its way there, from the list's clock.
    let slot = (flags >> 8u) & 15u;
    if (slot != 0u) {
        let d = u.scroll[slot * 2u];
        let t = u.scroll[slot * 2u + 1u];
        let k = ease((u.clock.x - t.x) / max(t.y, 1e-3), u32(t.z));
        centre = centre + mix(d.xy, d.zw, k);
    }
    if ((flags & SPINNING) != 0u) {
        let a = u.clock.y * TAU;
        let o = inst.spin.xy;
        let sc = vec2<f32>(cos(a), sin(a));
        centre = (centre - o) + vec2<f32>(o.x * sc.x - o.y * sc.y, o.x * sc.y + o.y * sc.x);
        angle = angle + a;
    }
    // 03 §5 `bounce`: lifted by the node's height times the clock's
    // fraction. Zero for a quad of no bouncing node, so no flag is needed.
    centre.y = centre.y - inst.bounce * u.clock.w;
    // 03 §5: the page this quad is on, or the shared element it belongs to.
    // Outside the scroller above -- a list mid-glide on a page mid-slide does
    // both -- and inside the frame below.
    var lscale = 1.0;
    var lalpha = 1.0;
    let xslot = (flags >> 12u) & 15u;
    if (xslot != 0u) {
        let base = xslot * 4u;
        let a = u.xform[base];
        let b = u.xform[base + 1u];
        let ck = u.xform[base + 2u];
        let pivot = u.xform[base + 3u].xy;
        var k = 0.0;
        if (u32(ck.z) >= HELD) {
            k = clamp(ck.w, 0.0, 1.0);
        } else {
            k = ease((u.clock.x - ck.x) / max(ck.y, 1e-3), u32(ck.z));
        }
        let v = mix(a, b, k);
        centre = (centre - pivot) * v.z + pivot + v.xy;
        lscale = v.z;
        lalpha = v.w;
    }
    let local = (c - vec2<f32>(0.5, 0.5)) * inst.rect.zw * lscale;
    let ca = cos(angle);
    let sa = sin(angle);
    var px = centre + vec2<f32>(local.x * ca - local.y * sa, local.x * sa + local.y * ca);
    // The layer, last: everything above moved this quad within its list, and
    // this moves the list. About the target's own centre, so a page scaling
    // to 92 % shrinks towards the middle of the window rather than its corner.
    let half = u.viewport.xy * 0.5;
    px = (px - half) * u.frame.z + half + u.frame.xy;
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
    out.grad = vec4<u32>(0u, 0u, 0u, 0u);
    // 02 §5.3. Where a point projects on the gradient line is affine in its
    // position, so it is worked out here at the corners and interpolated:
    // `uv` becomes how far along the first segment (first stop to middle)
    // and the second (middle to last) the fragment is.
    if ((flags & GRADIENT) != 0u) {
        let wh = inst.rect.zw;
        let code = u32(round(inst.uv.w));
        var dir = vec2<f32>(0.0, -1.0);
        if (code < 360u) {
            let a = f32(code) * TAU / 360.0;
            dir = vec2<f32>(sin(a), -cos(a));
        } else {
            // A corner: perpendicular to the diagonal that misses it, so the
            // half-way line runs corner to corner whatever the box's shape.
            var sx = 1.0;
            var sy = -1.0;
            if (code == 361u || code == 362u) {
                sy = 1.0;
            }
            if (code == 362u || code == 363u) {
                sx = -1.0;
            }
            dir = normalize(vec2<f32>(sx * wh.y, sy * wh.x));
        }
        let len = max(abs(wh.x * dir.x) + abs(wh.y * dir.y), 1e-4);
        let t = dot((c - vec2<f32>(0.5, 0.5)) * wh, dir) / len + 0.5;
        let at = inst.uv.xyz;
        out.uv = vec2<f32>((t - at.x) / max(at.y - at.x, 1e-4), (t - at.y) / max(at.z - at.y, 1e-4));
        out.grad = vec4<u32>(pack_srgb(inst.fill), pack_srgb(inst.fill_from), pack_srgb(inst.stroke_from), 0u);
    }
    out.extra = inst.extra;
    out.scale = lscale * u.frame.z;
    // 03 §5: a transition is both its ends and a clock. The instance holds
    // the ends; the clock is the list's age, so the same list is right for
    // every frame of it.
    if ((flags & ANIMATED) != 0u) {
        let curve = select(0u, 1u, (flags & DECELERATE) != 0u);
        let k = ease((u.clock.x - inst.spin.z) / max(inst.spin.w, 1e-3), curve);
        // A gradient's colours do not ease (03 §5), and its `*_from` are
        // its stops: only the opacity moves.
        if ((flags & GRADIENT) == 0u) {
            out.fill = mix(inst.fill_from, inst.fill, k);
            out.stroke = mix(inst.stroke_from, inst.stroke, k);
        }
        out.params.w = mix(inst.extra.w, inst.params.w, k);
    }
    // 03 §5 `pulse`: the clock's factor, on top of whatever the quad arrived
    // at, like the layer's opacity below.
    if ((flags & PULSING) != 0u) {
        out.params.w = out.params.w * u.clock.z;
    }
    // The layer's opacity multiplies whatever the quad arrived at, so a page
    // can fade while the things inside it are mid-transition. It is applied
    // after the mix above rather than folded into either end of it, which is
    // what keeps the two from having to share one timeline.
    out.params.w = out.params.w * lalpha * u.frame.w;
    return out;
}

// Every sample below takes an explicit LOD, and that is a validation
// requirement rather than a preference. `textureSample` computes its own
// derivatives, so WGSL may only call it from uniform control flow; every
// sample here is inside a branch on `flags`, which comes from the quad's own
// vertex data and is therefore not uniform. Tint rejects the module outright:
//
//     error: 'textureSample' must only be called from uniform control flow
//     note: control flow depends on possibly non-uniform value
//
// and a rejected module is a device with no pipeline, so nothing draws at
// all. It only bites where WebGPU is real — Brave and Chrome on macOS, over
// Metal. A machine that falls back to WebGL2 never runs the validator and
// the shader looks fine, which is how this shipped.
//
// The explicit LOD costs nothing: every texture bound here is created with
// `mip_level_count: 1`, so level 0 is the only level there has ever been and
// `textureSampleLevel(.., 0.0)` samples exactly what `textureSample` did.
@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let half = in.rect.zw * 0.5;
    let p = in.local * in.rect.zw - half;
    let r = min(in.params.x, min(half.x, half.y));
    let q = abs(p) - half + vec2<f32>(r, r);
    let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
    // `d` is in the quad's own units; the ramp wants half a *device* pixel,
    // so it is scaled by the layer before it is compared against one.
    let s = max(in.scale, 1e-4);
    var coverage = 1.0 - clamp(d * s + 0.5, 0.0, 1.0);
    // A shadow: the rect was grown by the blur; fade from opaque one blur
    // inside the true edge to nothing at the grown edge. The blur is in
    // device px and `d` is not, so the same correction applies.
    if (in.extra.y > 0.0) {
        coverage = 1.0 - smoothstep(-2.0 * in.extra.y / s, 0.0, d);
    }

    let flags = u32(in.params.z);

    // A scene is a picture and nothing else: no fill, no border, no
    // backdrop. Its target is bound where a blur would be, and it is
    // premultiplied already, so the only thing left to apply is this quad's
    // own coverage -- which is where the rounded corner, the scissor and
    // the page's opacity have all arrived by, without the scene's shader
    // knowing that any of them exist.
    if ((flags & SCENE) != 0u) {
        let t = textureSampleLevel(blur_tex, blur_smp, in.uv, 0.0);
        let a = coverage * in.params.w;
        return vec4<f32>(t.rgb * a, t.a * a);
    }

    var fill = in.fill;
    // 02 §5.3: the gradient, mixed in premultiplied sRGB and brought back to
    // the linear colour everything else here is. Before the first stop it is
    // the first stop, past the last the last.
    if ((flags & GRADIENT) != 0u) {
        let first = unpack_premul(in.grad.x);
        let mid = unpack_premul(in.grad.y);
        let last = unpack_premul(in.grad.z);
        var g = mix(mid, last, clamp(in.uv.y, 0.0, 1.0));
        if (in.uv.x < 1.0) {
            g = mix(first, mid, clamp(in.uv.x, 0.0, 1.0));
        }
        var straight = vec3<f32>(0.0, 0.0, 0.0);
        if (g.a > 0.0) {
            straight = g.rgb / g.a;
        }
        fill = vec4<f32>(to_linear(straight.r), to_linear(straight.g), to_linear(straight.b), g.a);
    }
    var color = fill;
    // 03 §2: a blurred node fills over its backdrop rather than over what
    // the target happens to hold, so a translucent fill tints frosted glass
    // instead of merely dimming what is behind it. The backdrop is opaque,
    // so what leaves here is too, and the rounded edge still comes from the
    // coverage below.
    if ((flags & BLURRED) != 0u) {
        let uv = (in.pos.xy + u.viewport.zw - u.backdrop.xy) / u.backdrop.zw;
        let back = textureSampleLevel(blur_tex, blur_smp, uv, 0.0);
        color = vec4<f32>(mix(back.rgb, fill.rgb, fill.a), 1.0);
    }
    let border = in.params.y;
    if (border > 0.0) {
        let inner = 1.0 - clamp(d + border + 0.5, 0.0, 1.0);
        color = mix(in.stroke, color, inner);
    }
    if ((flags & TEXTURED_RGBA) != 0u) {
        let t = textureSampleLevel(img_tex, img_smp, in.uv, 0.0);
        color = vec4<f32>(t.rgb, t.a * in.fill.a);
    } else if ((flags & TEXTURED) != 0u) {
        color.a = color.a * textureSampleLevel(atlas_tex, atlas_smp, in.uv, 0.0).r;
    }
    let a = color.a * coverage * in.params.w;
    return vec4<f32>(color.rgb * a, a);
}
