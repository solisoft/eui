// One shape: a rounded rectangle, optionally textured from the R8 atlas.
// Anti-aliasing comes from the signed distance to the rectangle's edge, so
// the same pipeline draws crisp boxes, hairlines, borders and glyphs.

struct Uniforms {
    viewport: vec4<f32>, // width, height, unused, unused
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var atlas_tex: texture_2d<f32>;
@group(1) @binding(1) var atlas_smp: sampler;
@group(1) @binding(2) var img_tex: texture_2d<f32>;
@group(1) @binding(3) var img_smp: sampler;

struct Inst {
    @location(0) rect: vec4<f32>,
    @location(1) params: vec4<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) stroke: vec4<f32>,
    @location(4) uv: vec4<f32>,
    @location(5) extra: vec4<f32>, // x: rotation about the centre, radians
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) params: vec4<f32>,
    @location(3) fill: vec4<f32>,
    @location(4) stroke: vec4<f32>,
    @location(5) uv: vec2<f32>,
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
    let centre = inst.rect.xy + inst.rect.zw * 0.5;
    let local = (c - vec2<f32>(0.5, 0.5)) * inst.rect.zw;
    let ca = cos(inst.extra.x);
    let sa = sin(inst.extra.x);
    let px = centre + vec2<f32>(local.x * ca - local.y * sa, local.x * sa + local.y * ca);
    let ndc = vec2<f32>(px.x / u.viewport.x * 2.0 - 1.0, 1.0 - px.y / u.viewport.y * 2.0);
    var out: VOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    out.local = c;
    out.rect = inst.rect;
    out.params = inst.params;
    out.fill = inst.fill;
    out.stroke = inst.stroke;
    out.uv = mix(inst.uv.xy, inst.uv.zw, c);
    return out;
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    let half = in.rect.zw * 0.5;
    let p = in.local * in.rect.zw - half;
    let r = min(in.params.x, min(half.x, half.y));
    let q = abs(p) - half + vec2<f32>(r, r);
    let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
    let coverage = 1.0 - clamp(d + 0.5, 0.0, 1.0);

    var color = in.fill;
    let border = in.params.y;
    if (border > 0.0) {
        let inner = 1.0 - clamp(d + border + 0.5, 0.0, 1.0);
        color = mix(in.stroke, in.fill, inner);
    }
    if (in.params.z >= 2.0) {
        let t = textureSample(img_tex, img_smp, in.uv);
        color = vec4<f32>(t.rgb, t.a * in.fill.a);
    } else if (in.params.z >= 1.0) {
        color.a = color.a * textureSample(atlas_tex, atlas_smp, in.uv).r;
    }
    let a = color.a * coverage * in.params.w;
    return vec4<f32>(color.rgb * a, a);
}
