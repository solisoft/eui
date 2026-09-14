//! The geometry container a `scene` names, and the checks that make it safe
//! to hand to a GPU.
//!
//! This decoder is the one place in the scene path where the worker
//! genuinely protects the window, and it is worth being precise about why.
//! wgpu validates a draw's index *range* against the size of the index
//! buffer. It does not validate index *values* against the size of the
//! vertex buffer, and no driver does either: on hardware where robust buffer
//! access is not guaranteed — which includes every GLES backend, where wgpu
//! generates `index: Unchecked` outright — an out-of-range index is a real
//! out-of-bounds vertex fetch. So the check has to happen here, in the
//! confined process, before the bytes become a buffer. Everything else about
//! a scene is the window's problem; this is ours.
//!
//! The container is EUI's own and not glTF. glTF would bring a JSON parser,
//! an accessor model and an extension registry into a sandbox whose whole
//! purpose is to be small, and the client's binary budget (10 §1) is already
//! a quarter over. A server holding a glTF converts it; that is a build
//! step, not a client.
//!
//! ```text
//! "EUIM" version:u8=1  flags:u8  vertex_count:u32le  index_count:u32le
//!   positions : vertex_count × 3 × f32le
//!   normals   : vertex_count × 3 × f32le   (flags bit 0)
//!   uvs       : vertex_count × 2 × f32le   (flags bit 1)
//!   indices   : index_count × u32le
//! ```
//!
//! Recognised by its leading bytes and never by a name the server sent,
//! which is 03 §1's rule for every asset and the reason a shader travels in
//! an `"EUIS"` container rather than as bare WGSL.
//!
//! The window checks the indices **again** on the way out of the worker pipe
//! (`worker.rs`, `get_list`), for the reason 11 §4 gives for re-verifying a
//! shader there: the worker is the process that reads what a server sent, so
//! a worker that has been taken over must not be able to *call* a mesh
//! checked. Two passes over an array of `u32` is not a price worth
//! negotiating over.

use std::fmt;

use eui_render::scene::Vertex;

/// What a mesh begins with.
pub const MAGIC: [u8; 4] = *b"EUIM";

/// The only version there is.
pub const VERSION: u8 = 1;

/// Normals are present.
pub const HAS_NORMALS: u8 = 1;
/// Texture coordinates are present.
pub const HAS_UVS: u8 = 2;

/// Vertices a mesh may have.
pub const MAX_VERTICES: u32 = 65_536;
/// Indices a mesh may have: [`MAX_VERTICES`] triangles' worth.
pub const MAX_INDICES: u32 = 196_608;

/// Why a mesh was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeshError {
    /// Not a mesh at all, or a version this client does not know.
    Magic,
    /// Flags outside the two that are defined.
    Flags(u8),
    /// More vertices or indices than [`MAX_VERTICES`] / [`MAX_INDICES`].
    TooBig,
    /// A count of indices that is not whole triangles.
    NotTriangles(u32),
    /// The body is not the length the header says.
    Truncated,
    /// A coordinate that is not a finite number. The wire format rejects a
    /// non-finite `Value::Float` for the same reason (`eui-proto`): a NaN in
    /// a position is a triangle that is nowhere and everywhere at once.
    NotFinite,
    /// **An index past the end of the vertices.** The check no driver makes.
    Index {
        /// The index that was out of range.
        at: u32,
        /// How many vertices there actually are.
        of: u32,
    },
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Magic => f.write_str("not an EUI mesh"),
            Self::Flags(v) => write!(f, "mesh flags {v:#04x} are not defined"),
            Self::TooBig => f.write_str("mesh is over the vertex or index cap"),
            Self::NotTriangles(n) => write!(f, "{n} indices is not whole triangles"),
            Self::Truncated => f.write_str("mesh is shorter than its header says"),
            Self::NotFinite => f.write_str("mesh has a coordinate that is not a finite number"),
            Self::Index { at, of } => write!(f, "mesh index {at} is past its {of} vertices"),
        }
    }
}

impl std::error::Error for MeshError {}

/// A mesh, checked, in the layout the window uploads without reading.
#[derive(Debug, Clone, PartialEq)]
pub struct Mesh {
    /// One per vertex.
    pub vertices: Vec<Vertex>,
    /// Three per triangle, every one of them inside `vertices`.
    pub indices: Vec<u32>,
}

/// Whether these bytes claim to be a mesh, by their first bytes alone.
#[must_use]
pub fn looks_like_mesh(bytes: &[u8]) -> bool {
    bytes.get(..4) == Some(&MAGIC)
}

/// Decode and check.
///
/// # Errors
///
/// [`MeshError`] names the rule that refused it. Nothing partial comes back:
/// a mesh is uploaded whole or not at all.
pub fn decode(bytes: &[u8]) -> Result<Mesh, MeshError> {
    if !looks_like_mesh(bytes) || bytes.get(4) != Some(&VERSION) {
        return Err(MeshError::Magic);
    }
    let flags = *bytes.get(5).ok_or(MeshError::Magic)?;
    if flags & !(HAS_NORMALS | HAS_UVS) != 0 {
        return Err(MeshError::Flags(flags));
    }
    let word = |at: usize| -> Result<u32, MeshError> {
        let b = bytes.get(at..at.checked_add(4).ok_or(MeshError::Truncated)?).ok_or(MeshError::Truncated)?;
        Ok(u32::from_le_bytes(b.try_into().map_err(|_| MeshError::Truncated)?))
    };
    let vertex_count = word(6)?;
    let index_count = word(10)?;
    if vertex_count > MAX_VERTICES || index_count > MAX_INDICES {
        return Err(MeshError::TooBig);
    }
    if index_count % 3 != 0 {
        return Err(MeshError::NotTriangles(index_count));
    }

    // Lengths first, so a header claiming a gigabyte of vertices is refused
    // before anything is allocated for it.
    let floats_per_vertex = 3 + if flags & HAS_NORMALS != 0 { 3 } else { 0 } + if flags & HAS_UVS != 0 { 2 } else { 0 };
    let vertex_bytes = (vertex_count as usize).checked_mul(floats_per_vertex * 4).ok_or(MeshError::TooBig)?;
    let index_bytes = (index_count as usize).checked_mul(4).ok_or(MeshError::TooBig)?;
    let want = 14usize.checked_add(vertex_bytes).and_then(|v| v.checked_add(index_bytes)).ok_or(MeshError::TooBig)?;
    if bytes.len() != want {
        return Err(MeshError::Truncated);
    }

    let f32_at = |at: usize| -> Result<f32, MeshError> {
        let b = bytes.get(at..at.saturating_add(4)).ok_or(MeshError::Truncated)?;
        let v = f32::from_le_bytes(b.try_into().map_err(|_| MeshError::Truncated)?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(MeshError::NotFinite)
        }
    };
    // The three arrays are stored one after another, not interleaved, so a
    // server can leave out the ones it has no use for.
    let normals_at = 14usize.saturating_add((vertex_count as usize).saturating_mul(12));
    let uvs_at = normals_at.saturating_add(if flags & HAS_NORMALS != 0 { (vertex_count as usize).saturating_mul(12) } else { 0 });
    let indices_at = uvs_at.saturating_add(if flags & HAS_UVS != 0 { (vertex_count as usize).saturating_mul(8) } else { 0 });

    let mut vertices = Vec::with_capacity(vertex_count as usize);
    for i in 0..vertex_count as usize {
        let p = 14usize.saturating_add(i.saturating_mul(12));
        let pos = [f32_at(p)?, f32_at(p.saturating_add(4))?, f32_at(p.saturating_add(8))?];
        let normal = if flags & HAS_NORMALS != 0 {
            let n = normals_at.saturating_add(i.saturating_mul(12));
            [f32_at(n)?, f32_at(n.saturating_add(4))?, f32_at(n.saturating_add(8))?]
        } else {
            // Facing the camera, so a mesh with no normals is lit flatly
            // rather than not at all.
            [0.0, 0.0, 1.0]
        };
        let uv = if flags & HAS_UVS != 0 {
            let t = uvs_at.saturating_add(i.saturating_mul(8));
            [f32_at(t)?, f32_at(t.saturating_add(4))?]
        } else {
            [0.0, 0.0]
        };
        vertices.push(Vertex { pos, normal, uv });
    }

    let mut indices = Vec::with_capacity(index_count as usize);
    for i in 0..index_count as usize {
        let at = indices_at.saturating_add(i.saturating_mul(4));
        let b = bytes.get(at..at.saturating_add(4)).ok_or(MeshError::Truncated)?;
        let v = u32::from_le_bytes(b.try_into().map_err(|_| MeshError::Truncated)?);
        // The check this module exists for.
        if v >= vertex_count {
            return Err(MeshError::Index { at: v, of: vertex_count });
        }
        indices.push(v);
    }
    Ok(Mesh { vertices, indices })
}

/// Write a mesh out, for tests and for a server written in Rust.
#[must_use]
pub fn encode(vertices: &[Vertex], indices: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(HAS_NORMALS | HAS_UVS);
    out.extend_from_slice(&u32::try_from(vertices.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&u32::try_from(indices.len()).unwrap_or(u32::MAX).to_le_bytes());
    for v in vertices {
        for c in v.pos {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    for v in vertices {
        for c in v.normal {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    for v in vertices {
        for c in v.uv {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    for i in indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}
