// The backdrop chain: reduce, then a separable Gaussian.
//
// A scrim covers the window, so convolving at full resolution is out: a
// hundred-tap kernel over two megapixels twice is not a frame. Instead the
// snapshot is reduced by a power of two chosen so that the standard
// deviation at the working resolution lands near two and a half pixels, and
// the Gaussian there is about fifteen taps whatever was asked for. A wider
// blur buys a smaller texture rather than more samples, so the cost does
// not grow with the radius -- and the detail a reduction loses is detail
// the blur was about to destroy anyway.
//
// Every target here carries the format the frame is drawn in, which is
// always sRGB: sampling decodes and writing encodes, so the weighted sums
// below are taken in linear light, where an average of colours is the
// average of the light they stand for.

struct Params {
    // 1 / source size in texels.
    inv_src: vec2<f32>,
    // The step between taps, in source texels: (1,0) across, (0,1) down.
    dir: vec2<f32>,
    // The reduction factor for `reduce`, in source texels per output texel.
    reduce_by: f32,
    // The standard deviation for `gauss`, in source texels.
    sigma: f32,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> p: Params;
@group(1) @binding(0) var src: texture_2d<f32>;
@group(1) @binding(1) var smp: sampler;

// A triangle that covers the target; no vertex buffer, no index buffer.
@vertex
fn vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let x = f32(i32(vi) / 2) * 4.0 - 1.0;
    let y = f32(i32(vi) & 1) * 4.0 - 1.0;
    return vec4<f32>(x, -y, 0.0, 1.0);
}

// Average a `reduce_by` x `reduce_by` block of the source. Each tap sits
// between two texels in each axis, so bilinear filtering makes one sample
// stand for four and the loop runs a quarter of the times it looks like.
@fragment
fn reduce(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let n = max(i32(p.reduce_by) / 2, 1);
    let step = p.reduce_by / f32(n);
    let base = floor(pos.xy) * p.reduce_by;
    var sum = vec4<f32>(0.0);
    for (var y = 0; y < n; y = y + 1) {
        for (var x = 0; x < n; x = x + 1) {
            let at = base + (vec2<f32>(f32(x), f32(y)) + 0.5) * step;
            sum = sum + textureSample(src, smp, at * p.inv_src);
        }
    }
    return sum / f32(n * n);
}

// One axis of the Gaussian. The sampler clamps, so a tap that falls off the
// snapshot repeats its edge -- which is why the snapshot was grown by three
// standard deviations in the first place: nothing that matters is out there.
@fragment
fn gauss(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // An entrance animates the radius up from zero (03 §5), and a standard
    // deviation of zero is a division by zero here -- every weight NaN, and
    // a pane of noise for the frame it passes through. A twentieth of a
    // pixel is below what the reduction can resolve, so the floor costs
    // nothing and the first frame of a dialog is simply sharp.
    let sigma = max(p.sigma, 0.05);
    let hw = clamp(i32(ceil(3.0 * sigma)), 1, 16);
    let denom = 2.0 * sigma * sigma;
    let at = floor(pos.xy) + 0.5;
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -hw; i <= hw; i = i + 1) {
        let f = f32(i);
        let w = exp(-f * f / denom);
        sum = sum + textureSample(src, smp, (at + p.dir * f) * p.inv_src) * w;
        total = total + w;
    }
    return sum / total;
}
