//! The mesh container's refusals.
//!
//! One vector per rule, in the style of `eui-proto/tests/reject.rs`, and for
//! the same reason: this decoder reads bytes a server chose, and what it
//! refuses is a security property rather than a convenience.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_client::mesh::{decode, encode, looks_like_mesh, Mesh, MeshError, HAS_NORMALS, HAS_UVS, MAGIC};
use eui_render::scene::Vertex;

fn triangle() -> (Vec<Vertex>, Vec<u32>) {
    (
        vec![
            Vertex { pos: [0.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 0.0] },
            Vertex { pos: [1.0, 0.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [1.0, 0.0] },
            Vertex { pos: [0.0, 1.0, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0, 1.0] },
        ],
        vec![0, 1, 2],
    )
}

#[test]
fn a_mesh_round_trips() {
    let (v, i) = triangle();
    let got = decode(&encode(&v, &i)).expect("a triangle");
    assert_eq!(got, Mesh { vertices: v, indices: i });
}

#[test]
fn a_mesh_is_known_by_its_first_bytes_and_never_by_a_name() {
    let (v, i) = triangle();
    assert!(looks_like_mesh(&encode(&v, &i)));
    assert!(!looks_like_mesh(b"\x89PNG\r\n\x1a\n"));
    assert!(!looks_like_mesh(b"EUI"));
}

#[test]
fn something_that_is_not_a_mesh_is_refused() {
    assert_eq!(decode(b"not a mesh at all").unwrap_err(), MeshError::Magic);
    assert_eq!(decode(&[]).unwrap_err(), MeshError::Magic);
}

#[test]
fn a_version_this_client_does_not_know_is_refused() {
    let (v, i) = triangle();
    let mut bytes = encode(&v, &i);
    bytes[4] = 2;
    assert_eq!(decode(&bytes).unwrap_err(), MeshError::Magic);
}

#[test]
fn undefined_flags_are_refused() {
    let (v, i) = triangle();
    let mut bytes = encode(&v, &i);
    bytes[5] = HAS_NORMALS | HAS_UVS | 0x80;
    assert_eq!(decode(&bytes).unwrap_err(), MeshError::Flags(0x83));
}

#[test]
fn a_header_claiming_more_than_the_cap_is_refused_before_anything_is_allocated() {
    let mut bytes = Vec::from(MAGIC);
    bytes.push(1);
    bytes.push(0);
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(decode(&bytes).unwrap_err(), MeshError::TooBig);
}

#[test]
fn indices_that_are_not_whole_triangles_are_refused() {
    let (v, _) = triangle();
    let bytes = encode(&v, &[0, 1]);
    assert_eq!(decode(&bytes).unwrap_err(), MeshError::NotTriangles(2));
}

#[test]
fn a_body_that_is_not_the_length_the_header_says_is_refused() {
    let (v, i) = triangle();
    let bytes = encode(&v, &i);
    assert_eq!(decode(&bytes[..bytes.len() - 1]).unwrap_err(), MeshError::Truncated);
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(decode(&longer).unwrap_err(), MeshError::Truncated);
}

#[test]
fn a_coordinate_that_is_not_a_number_is_refused() {
    let (mut v, i) = triangle();
    v[1].pos[0] = f32::NAN;
    assert_eq!(decode(&encode(&v, &i)).unwrap_err(), MeshError::NotFinite);
    let (mut v, i) = triangle();
    v[2].normal[2] = f32::INFINITY;
    assert_eq!(decode(&encode(&v, &i)).unwrap_err(), MeshError::NotFinite);
}

/// The vector this module exists for.
///
/// wgpu checks a draw's index *range* against the index buffer's size. It
/// does not check index *values* against the vertex buffer's, and on a GLES
/// backend it generates `index: Unchecked` outright — so an index past the
/// end is a real out-of-bounds fetch on the hardware. No driver refuses
/// this; the worker has to.
#[test]
fn an_index_past_the_end_of_the_vertices_is_refused() {
    let (v, _) = triangle();
    assert_eq!(decode(&encode(&v, &[0, 1, 3])).unwrap_err(), MeshError::Index { at: 3, of: 3 });
    assert_eq!(decode(&encode(&v, &[0, 1, u32::MAX])).unwrap_err(), MeshError::Index { at: u32::MAX, of: 3 });
}

#[test]
fn a_mesh_may_leave_out_what_it_has_no_use_for() {
    // Positions only: no normals, no texture coordinates.
    let mut bytes = Vec::from(MAGIC);
    bytes.push(1);
    bytes.push(0);
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    for c in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        bytes.extend_from_slice(&c.to_le_bytes());
    }
    for i in [0u32, 1, 2] {
        bytes.extend_from_slice(&i.to_le_bytes());
    }
    let m = decode(&bytes).expect("positions alone are a mesh");
    assert_eq!(m.vertices.len(), 3);
    // Facing the camera, so it is lit flatly rather than not at all.
    assert_eq!(m.vertices[0].normal, [0.0, 0.0, 1.0]);
}
