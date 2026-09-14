// The client's own scene shader: a lit cube, so the renderer can be brought
// up and tested against a picture no server had to send.
//
// It is also the worked example of the contract a server's module must meet
// -- the names, the signatures and the one binding -- and it is what
// `eui-shader` is written to accept. A module that does not look like this
// one is refused in the worker, before the window ever hands it to a driver.

struct Scene {
    mvp: mat4x4<f32>,
    // (time, age, 0, 0), seconds.
    time: vec4<f32>,
    // (width, height, scale, aspect), device pixels.
    size: vec4<f32>,
    params: vec4<f32>,
    tint: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: Scene;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
}

@vertex
fn vs_main(in: VIn) -> VOut {
    var out: VOut;
    out.pos = u.mvp * vec4<f32>(in.pos, 1.0);
    out.normal = in.normal;
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // One fixed light, and a floor under the shading so an unlit face is a
    // face rather than a hole.
    let light = normalize(vec3<f32>(0.35, 0.75, 0.55));
    let lambert = max(dot(normalize(in.normal), light), 0.0);
    let shade = 0.25 + 0.75 * lambert;
    let base = u.tint.rgb * shade;
    // Premultiplied, because that is what the target blends in and what the
    // composite quad expects to sample. Straight alpha here is the black
    // fringe around every translucent scene.
    let a = u.tint.a;
    return vec4<f32>(base * a, a);
}
