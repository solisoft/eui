//! # EUI renderer
//!
//! Everything on screen is a rounded rectangle: a box's fill and border, a
//! divider, a glyph textured from the atlas. One shape means one pipeline,
//! one instance buffer, and one draw call per scissor region, which is what
//! keeps a frame at a handful of GPU commands.
//!
//! - [`paint`] walks a laid-out tree into a [`DrawList`]: device-pixel
//!   snapped quads in paint order, grouped by clip.
//! - [`Atlas`] packs rasterised glyphs into one R8 texture.
//! - [`Renderer`] owns the wgpu device and pipeline, draws a list into any
//!   target, and can read an off-screen target back — so the renderer is
//!   tested by looking at pixels, on a machine with no display.
//!
//! The one exception to the single pass is a `blur` (03 §2), which has to
//! see what is under it: such a frame snapshots its backdrop, reduces and
//! convolves it, and only then draws itself. A frame with no blurred node
//! in it — every frame of most applications — never leaves the path above.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// The workspace denies unchecked arithmetic because the *decode path* handles
// bytes from the network. Nothing here does: every integer below is an atlas
// texel offset or a framebuffer stride, bounded by sizes this crate chose
// itself. Saturating those would not make a bad index safe, it would make it
// silently point somewhere else, so the lint is lifted for this crate alone.
#![allow(clippy::arithmetic_side_effects)]

pub mod atlas;
pub mod gpu;
pub mod paint;

pub use atlas::{Atlas, ImageAtlas, Region};
pub use gpu::{Offscreen, RenderError, Renderer, Target, FORMAT};
pub use paint::{colors_of, linear, paint, resolve_color, scrollbar_thumb, Backdrop, Colors, DrawList, Editing, Quad, Run, Scene, BLURRED, SCROLLBAR_WIDTH, SPINNING, TEXTURED, TEXTURED_RGBA};
