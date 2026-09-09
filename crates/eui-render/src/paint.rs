//! From a laid-out tree to a flat list of quads.
//!
//! Everything the renderer draws is a rounded rectangle: a box's background
//! and border, a divider, a glyph (a rectangle textured from the atlas). One
//! shape means one pipeline and one draw call per scissor region.

use eui_layout::{Layout, Rect, Style};
use eui_proto::{ColorRef, Display, NodeKind, Value};
use eui_text::TextEngine;
use eui_theme::{Resolved, Role};
use eui_tree::{Node, NodeIx, Session};

use crate::atlas::{Atlas, ImageAtlas};

/// Flag bit: sample the glyph atlas for alpha.
pub const TEXTURED: u32 = 1;
/// Flag bit: sample the image atlas for colour and alpha.
pub const TEXTURED_RGBA: u32 = 2;
/// Flag bit: fill over the frame's blurred backdrop rather than over what
/// happens to be in the target (03 §2).
pub const BLURRED: u32 = 4;
/// Flag bit: the quad belongs to a spinning node (03 §5), and the vertex
/// stage turns it from the clock in the uniforms. The draw list therefore
/// does not change from one spin frame to the next, which is what lets the
/// window redraw one without repainting anything.
pub const SPINNING: u32 = 8;

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
/// What a transition interpolates — and, since 03 §5's entrance animates a
/// frost as well as a fade, the blur radius rides here too.
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
    /// Backdrop blur, device-independent px, before the device scale.
    pub blur: f32,
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
    /// `angle` in radians about the rect's centre (a canvas segment), the
    /// shadow's blur, the backdrop's standard deviation in device px, then
    /// one spare float.
    pub extra: [f32; 4],
    /// Where this quad's centre sits relative to the centre of the spinning
    /// node it belongs to, device px, when `SPINNING` is set; zero
    /// otherwise. The vertex stage turns the offset and adds it back, so
    /// the node's centre never has to be sent: it is this quad's centre
    /// minus this offset. Two spare floats follow.
    pub spin: [f32; 4],
}

/// One `draw` call: a span of instances, the scissor rect they are clipped
/// to, and the blurred backdrop they may sample.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// Index into [`DrawList::clips`].
    pub clip: u32,
    /// Index into [`Backdrop::sigmas`] — which blur this run's quads see
    /// their backdrop through. `0` for a run with no blurred quad in it,
    /// which binds a blur no fragment then samples.
    pub chain: u32,
    /// First instance.
    pub first: u32,
    /// How many instances.
    pub count: u32,
}

/// What the frame's blurred quads see behind them (03 §2).
///
/// One snapshot serves them all: the frame as it stood when the first of
/// them was about to be painted. Two frosted panels that overlap therefore
/// both show what is under the pair, not one through the other.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Backdrop {
    /// The region to snapshot, device pixels, `x, y, w, h`, already clipped
    /// to the framebuffer. It is the union of the blurred rects grown by
    /// three standard deviations, which is where a Gaussian stops mattering.
    pub rect: [u32; 4],
    /// The first instance that may sample it: `quads[..first]` *is* the
    /// backdrop, and nothing in it is blurred.
    pub first: u32,
    /// One standard deviation in device pixels per blur, in the order the
    /// radii were first met. A [`Run`]'s `chain` indexes this.
    pub sigmas: Vec<f32>,
}

/// A frame's worth of quads, in paint order, grouped by scissor rect.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DrawList {
    /// Instances in paint order.
    pub quads: Vec<Quad>,
    /// Runs, in order.
    pub runs: Vec<Run>,
    /// Scissor rects in device pixels, `x, y, w, h`.
    pub clips: Vec<[u32; 4]>,
    /// The clear colour, linear RGBA.
    pub clear: [f32; 4],
    /// Something on screen animates by itself (a `spin`): the next frame
    /// is due at once rather than when an input arrives.
    pub wants_frame: bool,
    /// Set by the driver when a `spin` is the *only* reason another frame
    /// is due — no transition, no scroll, no timer, nothing dirty. The
    /// angle lives in the vertex stage, so such a frame draws this very
    /// list again with nothing but the clock moved on, and the window may
    /// redraw it without asking the driver for anything.
    pub spin_only: bool,
    /// Set when some node wears a `blur`: the extra passes the frame needs
    /// before its own. `None` is the ordinary single-pass frame.
    pub backdrop: Option<Backdrop>,
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
    /// Seconds on the client's clock, for `spin` (03 §5).
    pub now: f32,
    /// The scroller whose scrollbar the pointer is on or dragging: its
    /// thumb paints wider and darker.
    pub scrollbar_hot: Option<NodeIx>,
    /// Device pixels per logical pixel.
    pub scale: f32,
    /// Framebuffer size in device pixels.
    pub size: (u32, u32),
}

/// Build the draw list for a scene.
pub fn paint(scene: &mut Scene<'_>) -> DrawList {
    let mut list = DrawList { clear: linear(scene.theme.color(Role::SurfaceBase)), ..Default::default() };
    list.clips.push([0, 0, scene.size.0, scene.size.1]);
    let Some(root) = scene.session.root() else {
        return list;
    };
    let mut p = Painter { scene, list, clip: 0, chain: 0, run_start: 0, inherited_fg: vec![], deferred: Vec::new(), in_top: false, fade: 1.0, blur: None };
    p.node(root);
    // 03 §2.4: an `overlay` is a layer above the normal flow — it paints
    // after everything, clipped by the window and by nothing else, so a
    // dialog inside a card and a popover inside a scroller are both whole.
    p.in_top = true;
    while let Some(top) = p.deferred.first().copied() {
        p.deferred.remove(0);
        p.set_clip(0);
        p.node(top);
    }
    p.close_run();
    p.settle_backdrop();
    p.list
}

struct Painter<'s, 'a> {
    scene: &'s mut Scene<'a>,
    list: DrawList,
    clip: u32,
    /// Which blur the quads being pushed now sample; `0` is none.
    chain: u32,
    run_start: u32,
    inherited_fg: Vec<[f32; 4]>,
    /// Overlays met during the walk, kept for the top layer.
    deferred: Vec<NodeIx>,
    /// True once the top layer is being painted, so the overlays in it
    /// are drawn instead of deferred again.
    in_top: bool,
    /// The entrances (03 §5) this node sits inside, multiplied together.
    /// `opacity` is otherwise a property of one node's own quads, but an
    /// entrance is the node *and everything painted for it* arriving — a
    /// dialog whose panel faded up while its words were already at full
    /// strength would read as the text arriving on its own.
    fade: f32,
    /// The backdrop being accumulated, once some node has asked for one:
    /// the region as `x0, y0, x1, y1` in device pixels, the first blurred
    /// instance, and the standard deviations met.
    blur: Option<([f32; 4], u32, Vec<f32>)>,
}

impl Painter<'_, '_> {
    fn close_run(&mut self) {
        let end = self.list.quads.len() as u32;
        if end > self.run_start {
            self.list.runs.push(Run { clip: self.clip, chain: self.chain, first: self.run_start, count: end - self.run_start });
            self.run_start = end;
        }
    }

    fn set_clip(&mut self, clip: u32) {
        if clip != self.clip {
            self.close_run();
            self.clip = clip;
        }
    }

    /// Draws that follow sample blur `chain`. A run binds one blur, so a
    /// change of chain ends the run exactly as a change of scissor does.
    fn set_chain(&mut self, chain: u32) {
        if chain != self.chain {
            self.close_run();
            self.chain = chain;
        }
    }

    /// Register a node's blur: grow the region it needs snapshotted, and
    /// find (or add) the chain for its standard deviation. Returns the
    /// chain index to bind while its quad is drawn.
    ///
    /// The region is grown by three standard deviations, past which a
    /// Gaussian contributes less than half a per cent and the clamped
    /// sampler's edge repeat is not visible.
    fn note_blur(&mut self, sigma: f32, dev: [f32; 4]) -> u32 {
        let reach = sigma * 3.0;
        let (x0, y0) = (dev[0] - reach, dev[1] - reach);
        let (x1, y1) = (dev[0] + dev[2] + reach, dev[1] + dev[3] + reach);
        let first = self.list.quads.len() as u32;
        let (rect, _, sigmas) = self.blur.get_or_insert(([x0, y0, x1, y1], first, Vec::new()));
        rect[0] = rect[0].min(x0);
        rect[1] = rect[1].min(y0);
        rect[2] = rect[2].max(x1);
        rect[3] = rect[3].max(y1);
        match sigmas.iter().position(|s| *s == sigma) {
            Some(i) => i as u32,
            None => {
                sigmas.push(sigma);
                (sigmas.len() - 1) as u32
            }
        }
    }

    /// Turn the accumulated region into the frame's [`Backdrop`], clipped to
    /// the framebuffer. A region that falls entirely outside it leaves the
    /// frame single-pass, and the quads that asked keep their flag over a
    /// blur of nothing — which is what a node off screen deserves.
    fn settle_backdrop(&mut self) {
        let Some((r, first, sigmas)) = self.blur.take() else {
            return;
        };
        let (w, h) = (self.scene.size.0 as f32, self.scene.size.1 as f32);
        let x0 = r[0].max(0.0).min(w);
        let y0 = r[1].max(0.0).min(h);
        let x1 = r[2].max(0.0).min(w);
        let y1 = r[3].max(0.0).min(h);
        let rect = [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32];
        if rect[2] == 0 || rect[3] == 0 {
            return;
        }
        self.list.backdrop = Some(Backdrop { rect, first, sigmas });
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
        let Some(c) = self.list.clips.get(self.clip as usize) else {
            return false;
        };
        let (cx, cy, cw, ch) = (c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32);
        rect[0] < cx + cw && rect[0] + rect[2] > cx && rect[1] < cy + ch && rect[1] + rect[3] > cy
    }

    fn push(&mut self, mut q: Quad) {
        q.params[3] *= self.fade;
        // A rotated quad's `rect` is not its bounding box; the scissor
        // handles it, the cull does not.
        if (q.extra[0] != 0.0 || self.visible(q.rect)) && q.rect[2] > 0.0 && q.rect[3] > 0.0 {
            self.list.quads.push(q);
        }
    }

    fn node(&mut self, ix: NodeIx) {
        let Some(rect) = self.scene.layout.rect(ix) else {
            return;
        };
        // An overlay met in the flow is not painted here; it is put by for
        // the top layer, which paints it whole.
        if !self.in_top && self.scene.session.node(ix).is_some_and(|n| n.kind == NodeKind::Overlay) {
            self.deferred.push(ix);
            return;
        }
        let spinning = self.scene.session.style_of(ix).animation == 1;
        let first = self.list.quads.len();
        self.node_inner(ix, rect);
        if spinning {
            // Spec 03 §5 `spin`: everything painted for the node turns about
            // its centre, one revolution per 1.2 s. The angle is not applied
            // here. Baking it made every frame a different draw list, so a
            // spinner repainted the whole tree and pushed it across the
            // worker pipe sixty times a second to move eighteen pixels. What
            // is recorded instead is where each quad sits relative to the
            // node's centre; the vertex stage turns it from the clock, and
            // the list comes out identical frame after frame.
            let dev = self.device(rect);
            let (cx, cy) = (dev[0] + dev[2] / 2.0, dev[1] + dev[3] / 2.0);
            for q in self.list.quads.iter_mut().skip(first) {
                q.spin[0] = q.rect[0] + q.rect[2] / 2.0 - cx;
                q.spin[1] = q.rect[1] + q.rect[3] / 2.0 - cy;
                #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "params[2] is a small flag bitfield carried as a float")]
                let flags = q.params[2] as u32 | SPINNING;
                q.params[2] = flags as f32;
            }
            self.list.wants_frame = true;
        }
    }

    fn node_inner(&mut self, ix: NodeIx, rect: Rect) {
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
                spin: [0.0; 4],
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
        // 03 §2: a `blur` shows the backdrop through the border box, and the
        // background is composited over that. It is therefore worth a quad
        // even when `bg` is none — a pane of clear frosted glass.
        let sigma = over.map_or_else(|| f32::from(record.blur), |c| c.blur) * scale;
        let frosted = sigma > 0.0 && dev[2] > 0.0 && dev[3] > 0.0 && self.visible(dev);
        let (chain, flags) = if frosted { (self.note_blur(sigma, dev), BLURRED as f32) } else { (0, 0.0) };
        if uniform {
            if fill.is_some() || stroke.is_some() || frosted {
                self.set_chain(chain);
                self.push(Quad {
                    rect: dev,
                    params: [radius, if stroke.is_some() { border_w } else { 0.0 }, flags, opacity],
                    fill: fill.unwrap_or([0.0; 4]),
                    stroke: stroke.unwrap_or([0.0; 4]),
                    uv: [0.0; 4],
                    extra: [0.0, 0.0, sigma, 0.0],
                    spin: [0.0; 4],
                });
                self.set_chain(0);
            }
        } else {
            if fill.is_some() || frosted {
                self.set_chain(chain);
                self.push(Quad { rect: dev, params: [radius, 0.0, flags, opacity], fill: fill.unwrap_or([0.0; 4]), stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0, 0.0, sigma, 0.0], spin: [0.0; 4] });
                self.set_chain(0);
            }
            if let Some(stroke) = stroke {
                let [x, y, w, h] = dev;
                let edge = |rect: [f32; 4]| Quad { rect, params: [0.0, 0.0, 0.0, opacity], fill: stroke, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4], spin: [0.0; 4] };
                let (t, r, bo, l) = ((b.t * scale).round(), (b.r * scale).round(), (b.b * scale).round(), (b.l * scale).round());
                if t > 0.0 {
                    self.push(edge([x, y, w, t]));
                }
                if bo > 0.0 {
                    self.push(edge([x, y + h - bo, w, bo]));
                }
                if l > 0.0 {
                    self.push(edge([x, y, l, h]));
                }
                if r > 0.0 {
                    self.push(edge([x + w - r, y, r, h]));
                }
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
                spin: [0.0; 4],
            });
        }

        // Foreground colour inherits down the tree; text.default is the floor.
        let parent_fg = self.inherited_fg.last().copied().unwrap_or_else(|| linear(self.scene.theme.color(Role::TextDefault)));
        let fg = over.and_then(|c| c.fg).unwrap_or_else(|| self.color(record.fg, Some(parent_fg)).unwrap_or(parent_fg));
        self.inherited_fg.push(fg);

        let virtual_ = self.scene.layout.is_virtual(ix);
        match node.kind {
            // A video's current frame lives in the image atlas under the
            // picture's own hash, rewritten as it plays: to the painter it
            // is a picture.
            NodeKind::Image | NodeKind::Video => {
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
                        spin: [0.0; 4],
                    });
                }
            }
            NodeKind::Divider => {
                let mut q = Quad {
                    rect: dev,
                    params: [0.0, 0.0, 0.0, opacity],
                    fill: self.color(record.bg, None).unwrap_or_else(|| linear(self.scene.theme.color(Role::BorderDefault))),
                    stroke: [0.0; 4],
                    uv: [0.0; 4],
                    extra: [0.0; 4],
                    spin: [0.0; 4],
                };
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
            // A node mid-entrance dims everything below it by as much as it
            // is dimmed itself. `over` is set only while an animation runs,
            // so this is 1.0 for every node of every ordinary frame.
            let saved_fade = self.fade;
            if record.animation == eui_proto::ANIMATION_ENTER {
                if let Some(c) = over {
                    self.fade *= c.opacity;
                }
            }
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
            if node.kind == NodeKind::List {
                self.placeholders(ix, rect, &style, opacity);
            }
            self.fade = saved_fade;
            if clips {
                self.set_clip(saved);
                self.scrollbar(ix, rect, opacity);
            }
        }
        self.inherited_fg.pop();
    }

    /// Spec 04 §7.1: a windowed list's rows in view that have no child yet
    /// — asked for, on their way — are drawn as placeholders, a rounded
    /// block in `surface.sunken` inset by `space.2`, so a fast scroll shows
    /// where the rows are rather than nothing.
    fn placeholders(&mut self, ix: NodeIx, rect: Rect, style: &Style, opacity: f32) {
        let layout = self.scene.layout;
        let (Some(tops), Some(placed)) = (layout.row_tops(ix), layout.placed_rows(ix)) else {
            return;
        };
        let Some(node) = self.scene.session.node(ix) else {
            return;
        };
        let sy = node.scroll.1 as f32;
        let (x0, y0) = (rect.x + style.border.l + style.padding.l, rect.y + style.border.t + style.padding.t);
        let inner_w = (rect.w - style.inset_h()).max(0.0);
        let view_h = rect.h;
        let inset = self.scene.theme.space.get(2).copied().unwrap_or(8.0);
        let radius = self.scene.theme.radius.get(2).copied().unwrap_or(6.0);
        let fill = linear(self.scene.theme.color(Role::SurfaceSunken));
        let n = tops.len().saturating_sub(1);
        let first = tops.partition_point(|t| *t <= sy).saturating_sub(1);
        for row in first..n {
            let top = tops.get(row).copied().unwrap_or(0.0);
            if top - sy > view_h {
                break;
            }
            if placed.binary_search(&(row as u32)).is_ok() {
                continue;
            }
            let h = tops.get(row + 1).copied().unwrap_or(top) - top;
            let r = Rect::new(x0 + inset, y0 + top - sy + inset / 2.0, (inner_w - 2.0 * inset).max(0.0), (h - inset).max(0.0));
            let q = self.device(r);
            self.push(Quad { rect: q, params: [radius * self.scene.scale, 0.0, 0.0, opacity], fill, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4], spin: [0.0; 4] });
        }
    }

    /// Spec 03 §2: a scroller whose content overflows wears a thumb along
    /// its right edge — as long as view ÷ content, never under 24 px —
    /// painted after its children so it sits on top of them.
    fn scrollbar(&mut self, ix: NodeIx, rect: Rect, opacity: f32) {
        let Some(mut t) = scrollbar_thumb(self.scene.session, self.scene.layout, ix, rect) else {
            return;
        };
        let hot = self.scene.scrollbar_hot == Some(ix);
        let mut color = linear(self.scene.theme.color(if hot { Role::TextDefault } else { Role::TextMuted }));
        color[3] *= if hot { 0.7 } else { 0.45 };
        if hot {
            // Under the pointer the thumb fills its strip.
            t.x -= 1.0;
            t.w += 2.0;
        }
        let q = self.device(t);
        self.push(Quad { rect: q, params: [q[2] / 2.0, 0.0, 0.0, opacity], fill: color, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4], spin: [0.0; 4] });
    }

    fn text(&mut self, ix: NodeIx, rect: Rect, style: &Style, fg: [f32; 4], opacity: f32) {
        let Some(text) = self.scene.session.text_of(ix) else {
            return;
        };
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
        // A single-line field taller than its line — a control at the
        // theme's control height, say — centres its text; every other
        // node's text starts at the top of its content box.
        let inner_h = (rect.h - style.inset_v()).max(0.0);
        let centred = self.scene.session.node(ix).is_some_and(|n| n.kind == NodeKind::Input) && inner_h > shaped.metrics.height;
        let origin_y = rect.y + style.border.t + style.padding.t + if centred { ((inner_h - shaped.metrics.height) / 2.0).round() } else { 0.0 };
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
                    self.push(Quad { rect: r, params: [0.0, 0.0, 0.0, opacity], fill: accent, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4], spin: [0.0; 4] });
                }
            }
            let (cx, cy) = shaped.caret(e.caret);
            let x = ((origin_x + cx) * scale).round();
            let y0 = ((origin_y + cy - above) * scale).round();
            let y1 = ((origin_y + cy + below) * scale).round();
            self.push(Quad { rect: [x, y0, scale.max(1.0).round(), y1 - y0], params: [0.0, 0.0, 0.0, opacity], fill: fg, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4], spin: [0.0; 4] });
        }
        // Per-glyph colour, for syntax highlighting: the `spans` prop, a flat
        // list of `[start byte, length, colour]` triples.
        let spans = self
            .scene
            .session
            .atom_id("spans")
            .and_then(|atom| self.scene.session.node(ix).and_then(|n| n.prop(atom)))
            .and_then(|prop| match prop {
                Value::List(list) => Some(spans_of(list, |v| self.path_color(v))),
                _ => None,
            })
            .unwrap_or_default();

        let atlas_size = self.scene.atlas.size() as f32;
        // The glyphs arrive in text order and the spans are sorted, so the
        // cursor only ever moves forward: O(glyphs + spans), not the product.
        let mut cursor = 0usize;
        for g in &shaped.glyphs {
            let Some(region) = self.scene.atlas.get(self.scene.text, g.key, scale) else {
                continue;
            };
            let fill = span_at(&spans, &mut cursor, g.start).unwrap_or(fg);
            let gx = ((origin_x + g.x) * scale).round() + region.left as f32;
            let gy = ((origin_y + g.y) * scale).round() - region.top as f32;
            let (rx, ry, rw, rh) = (region.x as f32, region.y as f32, region.w as f32, region.h as f32);
            self.push(Quad {
                rect: [gx, gy, rw, rh],
                params: [0.0, 0.0, TEXTURED as f32, opacity],
                fill,
                stroke: [0.0; 4],
                uv: [rx / atlas_size, ry / atlas_size, (rx + rw) / atlas_size, (ry + rh) / atlas_size],
                extra: [0.0; 4],
                spin: [0.0; 4],
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
        let Some(atom) = session.atom_id("paths") else {
            return;
        };
        let Some(Value::List(paths)) = node.prop(atom) else {
            return;
        };
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
            let Some(Value::Int(kind)) = p.first() else {
                continue;
            };
            let Some(color) = p.get(1).and_then(|c| self.path_color(c)) else {
                continue;
            };
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
                    self.push(Quad { rect: q, params: [at(4) * scale, 0.0, 0.0, opacity], fill: color, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4], spin: [0.0; 4] });
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
                            self.push(Quad {
                                rect: [x, y.min(base), 1.0, (base - y).abs()],
                                params: [0.0, 0.0, 0.0, opacity],
                                fill: color,
                                stroke: [0.0; 4],
                                uv: [0.0; 4],
                                extra: [0.0; 4],
                                spin: [0.0; 4],
                            });
                            x += 1.0;
                        }
                    }
                }
                3 => {
                    let (cx, cy) = pt(0);
                    let r = at(2) * scale;
                    self.push(Quad { rect: [cx - r, cy - r, 2.0 * r, 2.0 * r], params: [r, 0.0, 0.0, opacity], fill: color, stroke: [0.0; 4], uv: [0.0; 4], extra: [0.0; 4], spin: [0.0; 4] });
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
            spin: [0.0; 4],
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

/// The `spans` prop, parsed into `(start byte, end byte, colour)`.
///
/// The server promises the triples are sorted by start and do not overlap;
/// [`span_at`] walks them on that promise. A ragged tail or a triple of the
/// wrong shape is dropped rather than rejected, because a malformed prop
/// should cost the highlighting, not the text.
/// The colour of each triple is resolved by `colour`, which is the same
/// reader a `canvas` path uses — a `Color`, a role id, a role name, or a
/// `#rrggbb`. A server writing Soli has only the name, so requiring the
/// wire's `Color` here made the prop unreachable from the language that
/// produces it.
fn spans_of<F>(list: &[Value], mut colour: F) -> Vec<(usize, usize, [f32; 4])>
where
    F: FnMut(&Value) -> Option<[f32; 4]>,
{
    list.chunks_exact(3)
        .filter_map(|c| match (c.first(), c.get(1), c.get(2)) {
            (Some(Value::Int(start)), Some(Value::Int(len)), Some(v)) if *start >= 0 && *len >= 0 => {
                let start = *start as usize;
                Some((start, start.saturating_add(*len as usize), colour(v)?))
            }
            _ => None,
        })
        .collect()
}

/// The colour of the span covering byte `at`, or `None` where no span does.
///
/// `cursor` is carried between calls and only moves forward, so walking a
/// run of glyphs in text order costs one pass over the spans rather than a
/// search per glyph.
fn span_at(spans: &[(usize, usize, [f32; 4])], cursor: &mut usize, at: usize) -> Option<[f32; 4]> {
    while spans.get(*cursor).is_some_and(|s| s.1 <= at) {
        *cursor = cursor.saturating_add(1);
    }
    spans.get(*cursor).filter(|s| at >= s.0 && at < s.1).map(|s| s.2)
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
        blur: f32::from(record.blur),
    }
}

/// Width of the strip a scrollbar occupies, logical px.
pub const SCROLLBAR_WIDTH: f32 = 8.0;

/// The thumb of `ix`'s vertical scrollbar in window coordinates, or `None`
/// when the content fits. Shared with the driver, which drags it.
pub fn scrollbar_thumb(session: &Session, layout: &Layout, ix: NodeIx, rect: Rect) -> Option<Rect> {
    let content = layout.content_size(ix)?;
    if content.h <= rect.h + 0.5 || rect.h <= 0.0 {
        return None;
    }
    let offset = session.node(ix)?.scroll.1 as f32;
    let max = (content.h - rect.h).max(1.0);
    let track = rect.h - 4.0;
    let len = (track * rect.h / content.h).max(24.0).min(track);
    let y = rect.y + 2.0 + (track - len) * (offset.clamp(0.0, max) / max);
    Some(Rect::new(rect.x + rect.w - SCROLLBAR_WIDTH + 1.0, y, SCROLLBAR_WIDTH - 2.0, len))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Stands in for the painter's colour reader. Distinguishes the forms a
    /// server may write — the wire's own `Color`, and the role *name*, which
    /// is all a Soli server has to hand.
    fn colour(v: &Value) -> Option<[f32; 4]> {
        match v {
            Value::Color(c) => Some([f32::from(c.index()), 0.0, 0.0, 1.0]),
            Value::Str(s) if s == "accent.base" => Some([9.0, 0.0, 0.0, 1.0]),
            _ => None,
        }
    }

    fn span(start: i64, len: i64, role: u16) -> [Value; 3] {
        [Value::Int(start), Value::Int(len), Value::Color(ColorRef::role(role))]
    }

    fn tone(role: u16) -> [f32; 4] {
        [f32::from(role), 0.0, 0.0, 1.0]
    }

    #[test]
    fn spans_parse_into_byte_ranges() {
        let list: Vec<Value> = [span(0, 5, 1), span(10, 5, 2)].concat();
        assert_eq!(spans_of(&list, colour), [(0, 5, tone(1)), (10, 15, tone(2))]);
    }

    #[test]
    fn a_colour_may_be_named_rather_than_encoded() {
        // The reason the prop was unreachable from Soli: a server writing the
        // language has the role's name, not the wire's `Color`.
        let list = vec![Value::Int(0), Value::Int(4), Value::Str("accent.base".into())];
        assert_eq!(spans_of(&list, colour), [(0, 4, tone(9))]);
    }

    #[test]
    fn a_malformed_prop_costs_the_highlighting_not_the_text() {
        // A ragged tail, an unreadable colour and a negative offset each drop
        // their own triple; the well-formed ones still stand.
        let mut list: Vec<Value> = span(0, 5, 1).into();
        list.extend(span(-4, 5, 2));
        list.extend([Value::Int(20), Value::Int(5), Value::Str("no such role".into())]);
        list.extend(span(30, 5, 4));
        list.push(Value::Int(40)); // ragged tail
        assert_eq!(spans_of(&list, colour), [(0, 5, tone(1)), (30, 35, tone(4))]);
    }

    #[test]
    fn the_cursor_pairs_glyphs_with_spans_in_one_pass() {
        let spans = spans_of(&[span(0, 5, 1), span(10, 5, 2), span(20, 5, 3)].concat(), colour);
        let mut cursor = 0;
        let got: Vec<_> = [0, 3, 7, 12, 22].iter().map(|at| span_at(&spans, &mut cursor, *at)).collect();
        assert_eq!(
            got,
            [
                Some(tone(1)), // inside the first span
                Some(tone(1)), // still inside it
                None,          // in the gap between spans
                Some(tone(2)), // the cursor advanced past the first
                Some(tone(3)),
            ]
        );
        // Every span was passed exactly once: the walk is O(glyphs + spans).
        assert_eq!(cursor, 2);
    }

    #[test]
    fn no_spans_leaves_every_glyph_to_the_node_colour() {
        let mut cursor = 0;
        assert_eq!(span_at(&[], &mut cursor, 7), None);
        assert_eq!(cursor, 0);
    }
}
