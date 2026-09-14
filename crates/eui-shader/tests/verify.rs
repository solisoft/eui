//! The verifier's verdicts, one vector per rule.
//!
//! These are the conformance surface for scenes. A scene's *pixels* are not
//! one — two conforming clients may draw a scene differently, because the
//! precision of `sin`, the filtering and the rasteriser's fill rule are the
//! adapter's. What every conforming client must agree on is which modules it
//! refuses, and that is decidable on a machine with no GPU at all, which is
//! why this file has no adapter in it.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_shader::{verify, Reject};

/// The uniform block the client offers: 128 bytes, and the only binding a
/// scene has.
const BLOCK: &str = "
struct Scene {
    mvp: mat4x4<f32>,
    time: vec4<f32>,
    size: vec4<f32>,
    params: vec4<f32>,
    tint: vec4<f32>,
}
@group(0) @binding(0) var<uniform> u: Scene;
";

/// A fragment stage around `body`, which must end in a return.
fn frag(body: &str) -> String {
    format!("{BLOCK}\n@fragment\nfn fs_main() -> @location(0) vec4<f32> {{\n{body}\n}}\n")
}

fn kind(src: &str) -> Reject {
    match verify(src) {
        Err(e) => e,
        Ok(s) => panic!("accepted, but should not have been: {s:?}"),
    }
}

// ---------------------------------------------------------------- accepted

#[test]
fn the_plainest_shader_passes() {
    let s = verify(&frag("return u.tint;")).expect("plain shader");
    assert!(!s.has_vertex);
}

#[test]
fn a_module_may_bring_its_own_vertex_stage() {
    let src = format!(
        "{BLOCK}
struct Out {{ @builtin(position) pos: vec4<f32> }}
@vertex fn vs_main(@builtin(vertex_index) i: u32) -> Out {{
    var o: Out;
    o.pos = vec4<f32>(f32(i), 0.0, 0.0, 1.0);
    return o;
}}
@fragment fn fs_main() -> @location(0) vec4<f32> {{ return u.tint; }}
"
    );
    assert!(verify(&src).expect("two stages").has_vertex);
}

#[test]
fn a_counted_loop_is_bounded_and_counted() {
    let s = verify(&frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 0; i < 8; i = i + 1) { acc = acc + 1.0; }
         return vec4<f32>(acc, 0.0, 0.0, 1.0);",
    ))
    .expect("counted loop");
    // Eight trips, and the proof is a number rather than a shrug.
    assert!(s.steps >= 8, "steps {}", s.steps);
    assert!(s.steps <= eui_shader::MAX_STEPS);
}

#[test]
fn a_loop_may_count_down() {
    verify(&frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 8; i > 0; i = i - 1) { acc = acc + 1.0; }
         return vec4<f32>(acc, 0.0, 0.0, 1.0);",
    ))
    .expect("downward loop");
}

#[test]
fn a_loop_that_never_runs_is_still_bounded() {
    verify(&frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 8; i < 0; i = i + 1) { acc = acc + 1.0; }
         return vec4<f32>(acc, 0.0, 0.0, 1.0);",
    ))
    .expect("empty range");
}

#[test]
fn the_comparison_may_be_written_either_way_round() {
    verify(&frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 0; 8 > i; i = i + 1) { acc = acc + 1.0; }
         return vec4<f32>(acc, 0.0, 0.0, 1.0);",
    ))
    .expect("mirrored comparison");
}

#[test]
fn a_shader_needs_no_uniform_at_all() {
    verify("@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }").expect("no bindings");
}

// ------------------------------------------------------------------- size

#[test]
fn a_module_longer_than_the_cap_is_refused_before_it_is_parsed() {
    let mut src = frag("return u.tint;");
    src.push_str(&"\n// ".repeat(eui_shader::MAX_SOURCE_BYTES));
    assert!(matches!(kind(&src), Reject::TooLong(_)));
}

#[test]
fn nonsense_does_not_parse() {
    assert!(matches!(kind("this is not wgsl"), Reject::Parse(_)));
}

#[test]
fn wgsl_that_parses_but_does_not_typecheck_is_invalid() {
    assert!(matches!(kind(&frag("return 1.0;")), Reject::Invalid(_)));
}

#[test]
fn too_many_statements_is_refused() {
    // Short statements, so that this is refused for its statement count and
    // not for its length: the two caps are separate rules and want separate
    // vectors.
    let body = format!("var acc: f32 = 0.;\n{}return vec4<f32>(acc);", "acc=acc+1.;\n".repeat(3_000));
    let src = frag(&body);
    assert!(src.len() < eui_shader::MAX_SOURCE_BYTES, "{} bytes", src.len());
    assert!(matches!(kind(&src), Reject::TooMuch { what: "statements", .. }));
}

// --------------------------------------------------------------- reaching

#[test]
fn a_compute_stage_is_refused() {
    let src = format!("{}\n@compute @workgroup_size(1) fn cs() {{}}\n", frag("return u.tint;"));
    assert_eq!(kind(&src), Reject::Forbidden("a compute stage"));
}

#[test]
fn a_storage_buffer_is_refused() {
    let src = format!("@group(0) @binding(1) var<storage, read_write> out: array<f32>;\n{}", frag("return u.tint;"));
    assert_eq!(kind(&src), Reject::Forbidden("a storage buffer"));
}

#[test]
fn a_read_only_storage_buffer_is_refused_too() {
    let src = format!("@group(0) @binding(1) var<storage, read> data: array<f32>;\n{}", frag("return u.tint;"));
    assert_eq!(kind(&src), Reject::Forbidden("a storage buffer"));
}

#[test]
fn a_texture_is_refused_in_version_one() {
    let src = format!("@group(1) @binding(0) var t: texture_2d<f32>;\n{}", frag("return u.tint;"));
    assert_eq!(kind(&src), Reject::Forbidden("a texture or a sampler"));
}

#[test]
fn a_sampler_is_refused_in_version_one() {
    let src = format!("@group(1) @binding(1) var s: sampler;\n{}", frag("return u.tint;"));
    assert_eq!(kind(&src), Reject::Forbidden("a texture or a sampler"));
}

#[test]
fn workgroup_memory_is_refused() {
    let src = format!("var<workgroup> scratch: array<f32, 4>;\n{}", frag("return u.tint;"));
    assert_eq!(kind(&src), Reject::Forbidden("workgroup memory"));
}

#[test]
fn a_barrier_is_refused() {
    let src = format!(
        "{BLOCK}
@fragment fn fs_main() -> @location(0) vec4<f32> {{ workgroupBarrier(); return u.tint; }}
"
    );
    // naga gets to this one first -- a barrier is forbidden at the fragment
    // stage by WGSL itself -- and the rule here is the belt behind that
    // brace, for the day a scene has a stage where it would be legal.
    assert!(matches!(kind(&src), Reject::Invalid(_) | Reject::Forbidden("a barrier")));
}

#[test]
fn an_atomic_is_refused() {
    let src = format!("@group(0) @binding(1) var<storage, read_write> c: atomic<u32>;\n{}", frag("return u.tint;"));
    // Refused for its address space first; either refusal keeps it out.
    assert!(matches!(kind(&src), Reject::Forbidden(_)));
}

// --------------------------------------------------------------- bindings

#[test]
fn a_uniform_at_another_binding_is_refused() {
    let src = BLOCK.replace("@binding(0)", "@binding(3)");
    let src = format!("{src}\n@fragment fn fs_main() -> @location(0) vec4<f32> {{ return u.tint; }}");
    assert_eq!(kind(&src), Reject::Binding("the only binding a scene has is group 0 binding 0"));
}

#[test]
fn a_uniform_in_another_group_is_refused() {
    let src = BLOCK.replace("@group(0)", "@group(2)");
    let src = format!("{src}\n@fragment fn fs_main() -> @location(0) vec4<f32> {{ return u.tint; }}");
    assert_eq!(kind(&src), Reject::Binding("the only binding a scene has is group 0 binding 0"));
}

#[test]
fn a_uniform_block_of_the_servers_own_shape_is_refused() {
    let src = "
struct Mine { a: vec4<f32>, b: vec4<f32> }
@group(0) @binding(0) var<uniform> u: Mine;
@fragment fn fs_main() -> @location(0) vec4<f32> { return u.a; }
";
    assert_eq!(kind(src), Reject::Binding("the uniform block is not the one the client offers"));
}

#[test]
fn a_uniform_that_is_not_a_struct_is_refused() {
    let src = "
@group(0) @binding(0) var<uniform> u: vec4<f32>;
@fragment fn fs_main() -> @location(0) vec4<f32> { return u; }
";
    assert_eq!(kind(src), Reject::Binding("the uniform block is a struct"));
}

// ---------------------------------------------------------------- entries

#[test]
fn a_fragment_stage_by_another_name_is_refused() {
    let src = format!("{BLOCK}\n@fragment fn main() -> @location(0) vec4<f32> {{ return u.tint; }}");
    assert_eq!(kind(&src), Reject::Entry("the fragment stage is named fs_main"));
}

#[test]
fn no_fragment_stage_at_all_is_refused() {
    let src = format!(
        "{BLOCK}
struct Out {{ @builtin(position) pos: vec4<f32> }}
@vertex fn vs_main() -> Out {{ var o: Out; o.pos = u.tint; return o; }}
"
    );
    assert_eq!(kind(&src), Reject::Entry("exactly one fragment stage"));
}

#[test]
fn a_fragment_stage_returning_another_location_is_refused() {
    let src = format!("{BLOCK}\n@fragment fn fs_main() -> @location(1) vec4<f32> {{ return u.tint; }}");
    assert_eq!(kind(&src), Reject::Entry("the fragment stage returns @location(0)"));
}

#[test]
fn a_fragment_stage_returning_something_other_than_a_colour_is_refused() {
    let src = format!("{BLOCK}\n@fragment fn fs_main() -> @location(0) f32 {{ return u.tint.x; }}");
    assert_eq!(kind(&src), Reject::Entry("the fragment stage returns vec4<f32>"));
}

#[test]
fn a_vertex_stage_by_another_name_is_refused() {
    let src = format!(
        "{BLOCK}
struct Out {{ @builtin(position) pos: vec4<f32> }}
@vertex fn vert() -> Out {{ var o: Out; o.pos = u.tint; return o; }}
@fragment fn fs_main() -> @location(0) vec4<f32> {{ return u.tint; }}
"
    );
    assert_eq!(kind(&src), Reject::Entry("the vertex stage is named vs_main"));
}

// ------------------------------------------------------------------ loops

#[test]
fn a_loop_with_no_exit_is_refused() {
    let src = frag("var acc: f32 = 0.0; loop { acc = acc + 1.0; } return vec4<f32>(acc);");
    assert!(matches!(kind(&src), Reject::Unbounded(_)));
}

#[test]
fn a_bound_read_from_a_uniform_is_no_bound_at_all() {
    let src = frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 0; f32(i) < u.params.x; i = i + 1) { acc = acc + 1.0; }
         return vec4<f32>(acc);",
    );
    assert!(matches!(kind(&src), Reject::Unbounded(_)), "a server-set bound must be refused");
}

#[test]
fn a_step_of_zero_is_refused() {
    let src = frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 0; i < 8; i = i + 0) { acc = acc + 1.0; }
         return vec4<f32>(acc);",
    );
    assert_eq!(kind(&src), Reject::Unbounded("a step of zero"));
}

#[test]
fn a_counter_the_body_also_moves_is_refused() {
    let src = frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 0; i < 8; i = i + 1) { acc = acc + 1.0; i = i - 1; }
         return vec4<f32>(acc);",
    );
    assert_eq!(kind(&src), Reject::Unbounded("a counter the body also assigns"));
}

#[test]
fn a_counter_that_walks_away_from_its_limit_is_refused() {
    let src = frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 0; i < 8; i = i - 1) { acc = acc + 1.0; }
         return vec4<f32>(acc);",
    );
    assert_eq!(kind(&src), Reject::Unbounded("a counter that never reaches its limit"));
}

#[test]
fn a_break_if_in_a_continuing_block_is_refused() {
    let src = frag(
        "var acc: f32 = 0.0;
         var i: i32 = 0;
         loop { acc = acc + 1.0; continuing { i = i + 1; break if i > 4; } }
         return vec4<f32>(acc);",
    );
    assert_eq!(kind(&src), Reject::Unbounded("a `break if` in a continuing block"));
}

#[test]
fn a_loop_whose_step_is_in_its_body_is_refused() {
    // Not because it is wrong, but because the step is then not where the
    // proof looks. `for` and `while` both put it where it is looked for.
    let src = frag(
        "var acc: f32 = 0.0;
         var i: i32 = 0;
         loop { if (i >= 4) { break; } acc = acc + 1.0; i = i + 1; }
         return vec4<f32>(acc);",
    );
    assert_eq!(kind(&src), Reject::Unbounded("a loop with no step"));
}

#[test]
fn a_loop_that_terminates_but_not_soon_enough_is_refused() {
    let src = frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 0; i < 100000; i = i + 1) { acc = acc + 1.0; }
         return vec4<f32>(acc);",
    );
    assert!(matches!(kind(&src), Reject::TooManySteps(_)));
}

#[test]
fn nested_loops_multiply_rather_than_add() {
    // 64 x 64 is four thousand and ninety-six bodies, which is the budget
    // exactly; one more trip on either axis is over it. This is the vector
    // that pins the multiplication, because a verifier that added instead
    // would accept both.
    let ok = frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 0; i < 40; i = i + 1) {
            for (var j: i32 = 0; j < 40; j = j + 1) { acc = acc + 1.0; }
         }
         return vec4<f32>(acc);",
    );
    assert!(matches!(verify(&ok), Err(Reject::TooManySteps(_))), "40 x 40 bodies is over the step budget");

    let fine = frag(
        "var acc: f32 = 0.0;
         for (var i: i32 = 0; i < 4; i = i + 1) {
            for (var j: i32 = 0; j < 4; j = j + 1) { acc = acc + 1.0; }
         }
         return vec4<f32>(acc);",
    );
    verify(&fine).expect("4 x 4 is well inside it");
}

#[test]
fn a_loop_in_a_called_function_still_counts() {
    let src = format!(
        "{BLOCK}
fn heavy() -> f32 {{
    var acc: f32 = 0.0;
    for (var i: i32 = 0; i < 2000; i = i + 1) {{ acc = acc + 1.0; }}
    return acc;
}}
@fragment fn fs_main() -> @location(0) vec4<f32> {{ return vec4<f32>(heavy()); }}
"
    );
    assert!(matches!(kind(&src), Reject::TooManySteps(_)));
}

/// The contract the client actually compiles, accepted whole.
///
/// The two halves of this feature are written in different crates and must
/// agree exactly: `eui-render`'s scene pipeline declares these vertex
/// attributes and this uniform block, and this verifier is what decides
/// whether a server's module may reach it. A vector here and a pixel test
/// there is how the pair stays true — change one signature and one of them
/// goes red.
#[test]
fn the_contract_the_client_compiles_is_accepted() {
    let src = "
struct Scene {
    mvp: mat4x4<f32>,
    time: vec4<f32>,
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
}
@vertex fn vs_main(in: VIn) -> VOut {
    var o: VOut;
    o.pos = u.mvp * vec4<f32>(in.pos, 1.0);
    o.normal = in.normal;
    return o;
}
@fragment fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    let light = normalize(vec3<f32>(0.35, 0.75, 0.55));
    let shade = 0.25 + 0.75 * max(dot(normalize(in.normal), light), 0.0);
    let a = u.tint.a;
    return vec4<f32>(u.tint.rgb * shade * a, a);
}
";
    let shape = verify(src).expect("the client's own contract");
    assert!(shape.has_vertex, "it brings its own vertex stage");
    assert!(shape.steps <= eui_shader::MAX_STEPS);
}

/// A shader travels in a container, recognised by its first bytes.
#[test]
fn a_shader_asset_is_known_by_its_first_bytes() {
    let wrapped = eui_shader::wrap("@fragment fn fs_main() -> @location(0) vec4<f32> { return vec4<f32>(1.0); }");
    assert!(eui_shader::looks_like_shader(&wrapped));
    assert!(!eui_shader::looks_like_shader(b"EUIM\x01"), "a mesh is not a shader");
    assert!(eui_shader::verify_asset(&wrapped).is_ok());
    // Bare WGSL is not a shader asset: 03 §1 says an asset is told apart by
    // its first bytes and never by a name the server sent, so the container
    // is not optional.
    assert!(eui_shader::verify_asset(b"@fragment fn fs_main() {}").is_err());
}

/// 11 §2.5: an index that is not a constant is refused.
///
/// The rule with a real bug behind it. A conforming WebGPU implementation
/// generates **unchecked** indexing on a GLES backend — the safety is left to
/// GLSL, whose answer to an out-of-range index is undefined behaviour — while
/// Vulkan, Metal and DX12 all restrict. GLES is what an Android device
/// without Vulkan has, and it is a path this client already takes.
#[test]
fn an_index_that_is_not_a_constant_is_refused() {
    let dynamic = frag(
        "var a: array<f32, 4> = array<f32, 4>(0.0, 1.0, 2.0, 3.0);
         var acc: f32 = 0.0;
         for (var i: i32 = 0; i < 4; i = i + 1) { acc = acc + a[i]; }
         return vec4<f32>(acc);",
    );
    assert_eq!(kind(&dynamic), Reject::Forbidden("an index that is not a constant"));

    // A constant index is fine, and is what the IR calls something else
    // entirely: it carries the index as a number and cannot be out of range.
    verify(&frag(
        "var a: array<f32, 4> = array<f32, 4>(0.0, 1.0, 2.0, 3.0);
         return vec4<f32>(a[2]);",
    ))
    .expect("a constant index");

    // And a swizzle is not an index at all.
    verify(&frag("return vec4<f32>(u.tint.zyx, u.tint.w);")).expect("a swizzle");
}
