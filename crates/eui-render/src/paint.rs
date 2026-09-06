//! From a laid-out tree to a flat list of quads.
//!
//! Everything the renderer draws is a rounded rectangle: a box's background
//! and border, a divider, a glyph (a rectangle textured from the atlas). One
//! shape means one pipeline and one draw call per scissor region.

use eui_layout::{Layout, Rect, Style};
use eui_proto::{ColorRef, Display, NodeKind, Value};
use eui_theme::{Resolved, Role};
use eui_tree::{Node, NodeIx, Session};
use eui_text::TextEngine;

use crate::atlas::{Atlas, ImageAtlas};

/// Flag bit: sample the glyph atlas for alpha.
pub const TEXTURED: u32 = 1;
/// Flag bit: sample the image atlas for colour and alpha.
pub const TEXTURED_RGBA: u32 = 2;

/// The caret and selection of the focused editable node, as byte offsets
/// into the text the node shows, plus how far the text is scrolled left to
/// keep the caret in view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Editing {
    /// The field.
    pub node: NodeIx,
    /// Selection start, `<= end`; equal to `end` when nothing is selected.
    pub start: usize,
    /// Selection end.
    pub end: usize,
    /// Where the caret is drawn.
    pub caret: usize,
    /// Logical px the text is shifted left.
    pub scroll_x: f32,
}

/// A node's resolved paint colours, linear RGBA; `None` draws nothing.
/// What a transition interpolates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Colors {
    /// Background.
    pub bg: Option<[f32; 4]>,
    /// Foreground, inherited by text.
    pub fg: Option<[f32; 4]>,
    /// Border.
    pub border: Option<[f32; 4]>,
    /// Opacity, `0..=1`.
    pub opacity: f32,
}

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
    /// `angle` in radians about the rect's centre (a canvas segment), then
    /// three spare floats.
    pub extra: [f32; 4],
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
    /// The image atlas, filled by the client as assets arrive.
    pub images: &'a ImageAtlas,
    /// The node wearing the focus ring, when focus is keyboard-visible.
    pub focus: Option<NodeIx>,
    /// Nodes mid-transition (03 §5) with the colours to paint this frame,
    /// in place of their record's.
    pub overrides: &'a [(NodeIx, Colors)],
    /// The field being edited: its caret and selection (03 §3).
    pub editing: Option<Editing>,
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
        resolve_color(self.scene.session, self.scene.theme, c).or(inherit)
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
        // A rotated quad's `rect` is not its bounding box; the scissor
        // handles it, the cull does not.
        if (q.extra[0] != 0.0 || self.visible(q.rect)) && q.rect[2] > 0.0 && q.rect[3] > 0.0 {
            self.list.quads.push(q);
        }
    }

    fn node(&mut self, ix: NodeIx) {
        let Some(rect) = self.scene.layout.rect(ix) else { return };
        // Cull before resolving a style or touching text: a virtualised list
        // has thousands of rows with rects and no business being painted.
        // Only a scroll container may hold visible content outside its own
        // box's intersection with the clip, and it clips itself.
        if !self.visible(self.device(rect)) {
            return;
        }
        let session: &Session = self.scene.session;
        let Some(node) = session.node(ix) else { return };
        let style = Style::resolve(&session.style_of(ix), self.scene.theme);
        if style.display == Display::None {
            return;
        }
        let record = self.scene.session.style_of(ix);
        let scale = self.scene.scale;
        let over = self.scene.overrides.iter().find(|(n, _)| *n == ix).map(|(_, c)| *c);
        let opacity = over.map_or(f32::from(record.opacity) / 255.0, |c| c.opacity);
        let dev = self.device(rect);
        let radius = self.scene.theme.radius(record.radius).unwrap_or(0.0) * scale;

        // Spec 03 §2: the shadow first — black, offset, grown by the blur,
        // fading across it (`extra[1]` is the blur for the fragment stage).
        if let Some(&(dy, blur, alpha)) = self.scene.theme.shadow.get(usize::from(record.shadow)).filter(|_| record.shadow != 0) {
            let (dy, blur) = (dy * scale, blur * scale);
            let [x, y, w, h] = dev;
            self.push(Quad {
                rect: [x - blur, y + dy - blur, w + 2.0 * blur, h + 2.0 * blur],
                params: [radius + blur, 0.0, 0.0, opacity],
                fill: [0.0, 0.0, 0.0, alpha],
                stroke: [0.0; 4],
                uv: [0.0; 4],
                extra: [0.0, blur, 0.0, 0.0],
            });
        }

        // Background and border. A uniform border is one stroked quad; a
        // border that differs per side — a tab's underline, a banner's left
        // bar — is the fill plus up to four thin quads, square-cornered.
        let fill = over.map_or_else(|| self.color(record.bg, None), |c| c.bg);
        let b = style.border;
        let uniform = b.t == b.r && b.r == b.b && b.b == b.l;
        let border_w = b.t.max(b.r).max(b.b).max(b.l) * scale;
        let stroke = if border_w > 0.0 { over.map_or_else(|| self.color(record.border_color, None), |c| c.border) } else { None };
        if uniform {
            if fill.is_some() || stroke.is_some() {
                self.push(Quad {
                    rect: dev,
                    params: [radius, if stroke.is_some() { border_w } else { 0.0 }, 0.0, opacity],
                    fill: fill.unwrap_or([0.0; 4]),
                    stroke: stroke.unwrap_or([0.0; 4]),
                    uv: [0.0; 4],
                extra: [0.0; 4],
                });
            }
        } else {
            if let Some(fill) = fill {
                self.push(Quad { rect: dev, params: [radius, 0.0, 0.0, opacity], fill, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4] });
            }
            if let Some(stroke) = stroke {
                let [x, y, w, h] = dev;
                let edge = |rect: [f32; 4]| Quad { rect, params: [0.0, 0.0, 0.0, opacity], fill: stroke, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4] };
                let (t, r, bo, l) = ((b.t * scale).round(), (b.r * scale).round(), (b.b * scale).round(), (b.l * scale).round());
                if t > 0.0 { self.push(edge([x, y, w, t])); }
                if bo > 0.0 { self.push(edge([x, y + h - bo, w, bo])); }
                if l > 0.0 { self.push(edge([x, y, l, h])); }
                if r > 0.0 { self.push(edge([x + w - r, y, r, h])); }
            }
        }

        // Spec 03 §3: a 2 px ring in `focus.ring`, outside the border box.
        if self.scene.focus == Some(ix) {
            let ring = 2.0 * scale;
            let [x, y, w, h] = dev;
            self.push(Quad {
                rect: [x - ring, y - ring, w + 2.0 * ring, h + 2.0 * ring],
                params: [radius + ring, ring, 0.0, 1.0],
                fill: [0.0; 4],
                stroke: linear(self.scene.theme.color(Role::FocusRing)),
                uv: [0.0; 4],
            extra: [0.0; 4],
            });
        }

        // Foreground colour inherits down the tree; text.default is the floor.
        let parent_fg = self.inherited_fg.last().copied().unwrap_or_else(|| linear(self.scene.theme.color(Role::TextDefault)));
        let fg = over.and_then(|c| c.fg).unwrap_or_else(|| self.color(record.fg, Some(parent_fg)).unwrap_or(parent_fg));
        self.inherited_fg.push(fg);

        let virtual_ = self.scene.layout.is_virtual(ix);
        match node.kind {
            NodeKind::Image => {
                let hash = node.props.iter().find_map(|(_, v)| match v {
                    Value::Asset(h) => Some(*h),
                    _ => None,
                });
                if let Some(region) = hash.and_then(|h| self.scene.images.get(&h)) {
                    let n = self.scene.images.size() as f32;
                    let (rx, ry, rw, rh) = (region.x as f32, region.y as f32, region.w as f32, region.h as f32);
                    self.push(Quad {
                        rect: dev,
                        params: [radius, 0.0, TEXTURED_RGBA as f32, opacity],
                        fill: [1.0, 1.0, 1.0, 1.0],
                        stroke: [0.0; 4],
                        uv: [rx / n, ry / n, (rx + rw) / n, (ry + rh) / n],
                    extra: [0.0; 4],
                    });
                }
            }
            NodeKind::Divider => {
                let mut q = Quad { rect: dev, params: [0.0, 0.0, 0.0, opacity], fill: self.color(record.bg, None).unwrap_or_else(|| linear(self.scene.theme.color(Role::BorderDefault))), stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4] };
                q.rect[3] = q.rect[3].max(1.0);
                self.push(q);
            }
            NodeKind::Canvas => self.canvas(node, rect, &style, opacity),
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
            if style.display == Display::Stack {
                let mut children: Vec<NodeIx> = node.children.clone();
                children.sort_by_key(|c| session.style_of(*c).z);
                for c in children {
                    self.node(c);
                }
            } else {
                for &c in &node.children {
                    self.node(c);
                }
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
        let editing = self.scene.editing.filter(|e| e.node == ix);
        // An edited field clips to its box and scrolls its text to the caret.
        let saved = self.clip;
        if editing.is_some() {
            let parent = self.list.clips.get(saved as usize).copied().unwrap_or([0, 0, 0, 0]);
            self.list.clips.push(intersect(parent, self.device(rect)));
            self.set_clip(self.list.clips.len() as u32 - 1);
        }
        let origin_x = rect.x + style.border.l + style.padding.l - editing.map_or(0.0, |e| e.scroll_x);
        let origin_y = rect.y + style.border.t + style.padding.t;
        let (above, below) = (style.font.size * 0.9, style.font.size * 0.25);
        if let Some(e) = editing {
            // Spec 03 §3: the selection in accent.base at 30 %, one rect per
            // line; then the caret in the text colour, one device px wide.
            if e.start < e.end {
                let mut accent = linear(self.scene.theme.color(Role::AccentBase));
                accent[3] *= 0.3;
                let mut lines: Vec<(f32, f32, f32)> = Vec::new(); // baseline, min x, max x
                for g in shaped.glyphs.iter().filter(|g| g.start >= e.start && g.start < e.end) {
                    match lines.iter_mut().find(|l| l.0 == g.y) {
                        Some(l) => {
                            l.1 = l.1.min(g.x);
                            l.2 = l.2.max(g.x + g.w);
                        }
                        None => lines.push((g.y, g.x, g.x + g.w)),
                    }
                }
                for (y, x0, x1) in lines {
                    let r = self.device(Rect::new(origin_x + x0, origin_y + y - above, x1 - x0, above + below));
                    self.push(Quad { rect: r, params: [0.0, 0.0, 0.0, opacity], fill: accent, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4] });
                }
            }
            let (cx, cy) = shaped.caret(e.caret);
            let x = ((origin_x + cx) * scale).round();
            let y0 = ((origin_y + cy - above) * scale).round();
            let y1 = ((origin_y + cy + below) * scale).round();
            self.push(Quad { rect: [x, y0, scale.max(1.0).round(), y1 - y0], params: [0.0, 0.0, 0.0, opacity], fill: fg, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4] });
        }
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
                extra: [0.0; 4],
            });
        }
        if editing.is_some() {
            self.set_clip(saved);
        }
    }
}

impl Painter<'_, '_> {
    /// Spec 03 §1.1: a `canvas` node's `paths`, in logical px from its
    /// content box, clipped to its border box. Everything becomes the one
    /// rounded-rectangle quad: a segment is a rotated capsule, an area is a
    /// strip per device column, an arc is a fan of capsules.
    fn canvas(&mut self, node: &Node, rect: Rect, style: &Style, opacity: f32) {
        let session: &Session = self.scene.session;
        let Some(atom) = session.atom_id("paths") else { return };
        let Some(Value::List(paths)) = node.prop(atom) else { return };
        let scale = self.scene.scale;
        let dev = self.device(rect);
        let saved = self.clip;
        let parent = self.list.clips.get(saved as usize).copied().unwrap_or([0, 0, 0, 0]);
        self.list.clips.push(intersect(parent, dev));
        self.set_clip(self.list.clips.len() as u32 - 1);
        let (cx0, cy0) = (rect.x + style.border.l + style.padding.l, rect.y + style.border.t + style.padding.t);
        let (ox, oy) = (cx0 * scale, cy0 * scale);
        for path in paths {
            let Value::List(p) = path else { continue };
            let Some(Value::Int(kind)) = p.first() else { continue };
            let Some(color) = p.get(1).and_then(|c| self.path_color(c)) else { continue };
            let n: Vec<f32> = p.iter().skip(2).filter_map(num).collect();
            let at = |i: usize| n.get(i).copied().unwrap_or(0.0);
            let pt = |i: usize| (ox + at(i) * scale, oy + at(i + 1) * scale);
            match kind {
                0 => {
                    let w = at(0) * scale;
                    for i in (1..n.len().saturating_sub(2)).step_by(2) {
                        self.segment(pt(i), pt(i + 2), w, color, opacity);
                    }
                }
                1 => {
                    let q = self.device(Rect::new(cx0 + at(0), cy0 + at(1), at(2), at(3)));
                    self.push(Quad { rect: q, params: [at(4) * scale, 0.0, 0.0, opacity], fill: color, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4] });
                }
                2 => {
                    let base = oy + at(0) * scale;
                    for i in (1..n.len().saturating_sub(2)).step_by(2) {
                        let ((x0, y0), (x1, y1)) = (pt(i), pt(i + 2));
                        if x1 <= x0 {
                            continue;
                        }
                        let mut x = x0.ceil();
                        while x <= x1.floor() {
                            let y = y0 + (y1 - y0) * (x - x0) / (x1 - x0);
                            self.push(Quad { rect: [x, y.min(base), 1.0, (base - y).abs()], params: [0.0, 0.0, 0.0, opacity], fill: color, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4] });
                            x += 1.0;
                        }
                    }
                }
                3 => {
                    let (cx, cy) = pt(0);
                    let r = at(2) * scale;
                    self.push(Quad { rect: [cx - r, cy - r, 2.0 * r, 2.0 * r], params: [r, 0.0, 0.0, opacity], fill: color, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4] });
                }
                4 => {
                    let w = at(0) * scale;
                    let (cx, cy) = pt(1);
                    let r = at(3) * scale;
                    let (a0, a1) = (at(4), at(5));
                    // 6° chords: a 14 px ring shows no facets at that pitch.
                    let steps = ((a1 - a0).abs() / 6f32.to_radians()).ceil().max(1.0) as usize;
                    let step = (a1 - a0) / steps as f32;
                    let on = |a: f32| (cx + r * a.cos(), cy + r * a.sin());
                    for i in 0..steps {
                        let a = a0 + step * i as f32;
                        self.segment(on(a), on(a + step), w, color, opacity);
                    }
                }
                _ => {}
            }
        }
        self.set_clip(saved);
    }

    /// A capsule from `a` to `b`: a quad `width` tall, rotated about its
    /// centre, with a radius of half the width so the ends are round — which
    /// also covers the joins of a polyline.
    fn segment(&mut self, a: (f32, f32), b: (f32, f32), width: f32, color: [f32; 4], opacity: f32) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        let w = width.max(1.0);
        let (mx, my) = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let angle = if len < 1e-3 { 0.0 } else { dy.atan2(dx) };
        self.push(Quad {
            rect: [mx - (len + w) / 2.0, my - w / 2.0, len + w, w],
            params: [w / 2.0, 0.0, 0.0, opacity],
            fill: color,
            stroke: [0.0; 4],
            uv: [0.0; 4],
            extra: [angle, 0.0, 0.0, 0.0],
        });
    }

    /// A path colour: resolved on the wire (`Color`), a role id, or — for a
    /// hand-written tree — a role name or `#RRGGBB[AA]`.
    fn path_color(&self, v: &Value) -> Option<[f32; 4]> {
        match v {
            Value::Color(c) => self.color(*c, None),
            Value::Int(id) => u16::try_from(*id).ok().and_then(|id| self.scene.theme.color_by_id(id)).map(linear),
            Value::Str(s) => {
                if let Some(hex) = s.strip_prefix('#') {
                    let n = u32::from_str_radix(hex, 16).ok()?;
                    return Some(linear(if hex.len() == 6 { (n << 8) | 0xFF } else { n }));
                }
                Role::from_name(s).map(|r| linear(self.scene.theme.color(r)))
            }
            _ => None,
        }
    }
}

fn num(v: &Value) -> Option<f32> {
    match v {
        Value::Int(n) => Some(*n as f32),
        Value::Float(f) => Some(*f as f32),
        _ => None,
    }
}

/// A colour reference against the session's literal table and the theme,
/// linear RGBA; `None` for "none" or an unknown id.
pub fn resolve_color(session: &Session, theme: &Resolved, c: ColorRef) -> Option<[f32; 4]> {
    if c.is_none() {
        return None;
    }
    let rgba = if c.is_literal() { session.color(u32::from(c.index()))? } else { theme.color_by_id(c.index())? };
    Some(linear(rgba))
}

/// The colours a record paints with, for a transition's endpoints.
pub fn colors_of(session: &Session, theme: &Resolved, record: &eui_proto::StyleRecord) -> Colors {
    Colors {
        bg: resolve_color(session, theme, record.bg),
        fg: resolve_color(session, theme, record.fg),
        border: resolve_color(session, theme, record.border_color),
        opacity: f32::from(record.opacity) / 255.0,
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
