//! From a laid-out tree to a flat list of quads.
//!
//! Everything the renderer draws is a rounded rectangle: a box's background
//! and border, a divider, a glyph (a rectangle textured from the atlas). One
//! shape means one pipeline and one draw call per scissor region.

use eui_layout::{Layout, Rect, Style};
use eui_proto::{ColorRef, Display, NodeKind};
use eui_theme::{Resolved, Role};
use eui_tree::{NodeIx, Session};
use eui_text::TextEngine;

use crate::atlas::Atlas;

/// Flag bit: sample the atlas for alpha.
pub const TEXTURED: u32 = 1;

/// One instance. Layout matches the vertex buffer in `shader.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Quad {
    /// `x, y, w, h` in device pixels.
    pub rect: [f32; 4],
    /// `radius, border width, flags, opacity`.
    pub params: [f32; 4],
    /// Fill, linear premultiplied-ready RGBA.
    pub fill: [f32; 4],
    /// Border colour.
    pub stroke: [f32; 4],
    /// Atlas `u0, v0, u1, v1`.
    pub uv: [f32; 4],
}

/// A frame's worth of quads, in paint order, grouped by scissor rect.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DrawList {
    /// Instances in paint order.
    pub quads: Vec<Quad>,
    /// `(clip index, first quad, quad count)` runs, in order.
    pub runs: Vec<(u32, u32, u32)>,
    /// Scissor rects in device pixels, `x, y, w, h`.
    pub clips: Vec<[u32; 4]>,
    /// The clear colour, linear RGBA.
    pub clear: [f32; 4],
}

/// Inputs to one paint.
pub struct Scene<'a> {
    /// The tree.
    pub session: &'a Session,
    /// Its layout for this frame.
    pub layout: &'a Layout,
    /// The viewer's theme.
    pub theme: &'a Resolved,
    /// Shaping and rasterisation.
    pub text: &'a mut TextEngine,
    /// The glyph atlas.
    pub atlas: &'a mut Atlas,
    /// Device pixels per logical pixel.
    pub scale: f32,
    /// Framebuffer size in device pixels.
    pub size: (u32, u32),
}

/// Build the draw list for a scene.
pub fn paint(scene: &mut Scene<'_>) -> DrawList {
    let mut list = DrawList { clear: linear(scene.theme.color(Role::SurfaceBase)), ..Default::default() };
    list.clips.push([0, 0, scene.size.0, scene.size.1]);
    let Some(root) = scene.session.root() else { return list };
    let mut p = Painter { scene, list, clip: 0, run_start: 0, inherited_fg: vec![] };
    p.node(root);
    p.close_run();
    p.list
}

struct Painter<'s, 'a> {
    scene: &'s mut Scene<'a>,
    list: DrawList,
    clip: u32,
    run_start: u32,
    inherited_fg: Vec<[f32; 4]>,
}

impl Painter<'_, '_> {
    fn close_run(&mut self) {
        let end = self.list.quads.len() as u32;
        if end > self.run_start {
            self.list.runs.push((self.clip, self.run_start, end - self.run_start));
            self.run_start = end;
        }
    }

    fn set_clip(&mut self, clip: u32) {
        if clip != self.clip {
            self.close_run();
            self.clip = clip;
        }
    }

    fn color(&self, c: ColorRef, inherit: Option<[f32; 4]>) -> Option<[f32; 4]> {
        if c.is_none() {
            return inherit;
        }
        let rgba = if c.is_literal() {
            self.scene.session.color(u32::from(c.index()))?
        } else {
            self.scene.theme.color_by_id(c.index())?
        };
        Some(linear(rgba))
    }

    fn device(&self, r: Rect) -> [f32; 4] {
        let s = self.scene.scale;
        let x0 = (r.x * s).round();
        let y0 = (r.y * s).round();
        let x1 = ((r.x + r.w) * s).round();
        let y1 = ((r.y + r.h) * s).round();
        [x0, y0, x1 - x0, y1 - y0]
    }

    fn visible(&self, rect: [f32; 4]) -> bool {
        let Some(c) = self.list.clips.get(self.clip as usize) else { return false };
        let (cx, cy, cw, ch) = (c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32);
        rect[0] < cx + cw && rect[0] + rect[2] > cx && rect[1] < cy + ch && rect[1] + rect[3] > cy
    }

    fn push(&mut self, q: Quad) {
        if self.visible(q.rect) && q.rect[2] > 0.0 && q.rect[3] > 0.0 {
            self.list.quads.push(q);
        }
    }

    fn node(&mut self, ix: NodeIx) {
        let Some(rect) = self.scene.layout.rect(ix) else { return };
        let Some(node) = self.scene.session.node(ix) else { return };
        let style = Style::resolve(&self.scene.session.style_of(ix), self.scene.theme);
        if style.display == Display::None {
            return;
        }
        let record = self.scene.session.style_of(ix);
        let scale = self.scene.scale;
        let opacity = f32::from(record.opacity) / 255.0;
        let dev = self.device(rect);

        // Background and border.
        let fill = self.color(record.bg, None);
        let border_w = style.border.t.max(style.border.r).max(style.border.b).max(style.border.l) * scale;
        let stroke = if border_w > 0.0 { self.color(record.border_color, None) } else { None };
        let radius = self.scene.theme.radius(record.radius).unwrap_or(0.0) * scale;
        if fill.is_some() || stroke.is_some() {
            self.push(Quad {
                rect: dev,
                params: [radius, if stroke.is_some() { border_w } else { 0.0 }, 0.0, opacity],
                fill: fill.unwrap_or([0.0; 4]),
                stroke: stroke.unwrap_or([0.0; 4]),
                uv: [0.0; 4],
            });
        }

        // Foreground colour inherits down the tree; text.default is the floor.
        let parent_fg = self.inherited_fg.last().copied().unwrap_or_else(|| linear(self.scene.theme.color(Role::TextDefault)));
        let fg = self.color(record.fg, Some(parent_fg)).unwrap_or(parent_fg);
        self.inherited_fg.push(fg);

        let virtual_ = self.scene.layout.is_virtual(ix);
        match node.kind {
            NodeKind::Divider => {
                let mut q = Quad { rect: dev, params: [0.0, 0.0, 0.0, opacity], fill: self.color(record.bg, None).unwrap_or_else(|| linear(self.scene.theme.color(Role::BorderDefault))), stroke: [0.0; 4], uv: [0.0; 4] };
                q.rect[3] = q.rect[3].max(1.0);
                self.push(q);
            }
            NodeKind::Text | NodeKind::Input | NodeKind::TextArea if !virtual_ => {
                self.text(ix, rect, &style, fg, opacity);
            }
            _ => {}
        }

        // Children, with a scissor for scrolling containers.
        if !virtual_ {
            let clips = matches!(node.kind, NodeKind::Scroll | NodeKind::List);
            let saved = self.clip;
            if clips {
                let parent = self.list.clips.get(saved as usize).copied().unwrap_or([0, 0, 0, 0]);
                let inner = intersect(parent, dev);
                self.list.clips.push(inner);
                self.set_clip(self.list.clips.len() as u32 - 1);
            }
            let mut children: Vec<NodeIx> = node.children.clone();
            if style.display == Display::Stack {
                children.sort_by_key(|c| self.scene.session.style_of(*c).z);
            }
            for c in children {
                self.node(c);
            }
            if clips {
                self.set_clip(saved);
            }
        }
        self.inherited_fg.pop();
    }

    fn text(&mut self, ix: NodeIx, rect: Rect, style: &Style, fg: [f32; 4], opacity: f32) {
        let Some(text) = self.scene.session.text_of(ix) else { return };
        let max_w = (rect.w - style.inset_h()).max(0.0);
        let shaped = self.scene.text.shape(text, style.font, Some(max_w), style.line_clamp);
        let scale = self.scene.scale;
        let origin_x = rect.x + style.border.l + style.padding.l;
        let origin_y = rect.y + style.border.t + style.padding.t;
        let atlas_size = self.scene.atlas.size() as f32;
        for g in &shaped.glyphs {
            let Some(region) = self.scene.atlas.get(self.scene.text, g.key, scale) else { continue };
            let gx = ((origin_x + g.x) * scale).round() + region.left as f32;
            let gy = ((origin_y + g.y) * scale).round() - region.top as f32;
            let (rx, ry, rw, rh) = (region.x as f32, region.y as f32, region.w as f32, region.h as f32);
            self.push(Quad {
                rect: [gx, gy, rw, rh],
                params: [0.0, 0.0, TEXTURED as f32, opacity],
                fill: fg,
                stroke: [0.0; 4],
                uv: [rx / atlas_size, ry / atlas_size, (rx + rw) / atlas_size, (ry + rh) / atlas_size],
            });
        }
    }
}

fn intersect(a: [u32; 4], b: [f32; 4]) -> [u32; 4] {
    let bx0 = b[0].max(0.0) as u32;
    let by0 = b[1].max(0.0) as u32;
    let bx1 = (b[0] + b[2]).max(0.0) as u32;
    let by1 = (b[1] + b[3]).max(0.0) as u32;
    let x0 = a[0].max(bx0);
    let y0 = a[1].max(by0);
    let x1 = (a[0].saturating_add(a[2])).min(bx1);
    let y1 = (a[1].saturating_add(a[3])).min(by1);
    [x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0)]
}

/// `0xRRGGBBAA` sRGB to linear RGBA floats.
pub fn linear(rgba: u32) -> [f32; 4] {
    let l = eui_theme::Linear::from_rgba(rgba);
    [l.r as f32, l.g as f32, l.b as f32, (rgba & 0xFF) as f32 / 255.0]
}
