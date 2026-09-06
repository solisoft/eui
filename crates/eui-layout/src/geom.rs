//! Geometry primitives. Plain `f32` px; nothing here is snapped.

/// A size.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Size {
    /// Construct.
    pub const fn new(w: f32, h: f32) -> Self {
        Self { w, h }
    }
}

/// An axis-aligned rectangle in absolute window coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Rect {
    /// Construct.
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// True when the point lies inside, edges inclusive on the near side.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.x + self.w && py < self.y + self.h
    }

    /// The overlap of two rectangles, or an empty one.
    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = (self.x + self.w).min(o.x + o.w);
        let b = (self.y + self.h).min(o.y + o.h);
        Rect::new(x, y, (r - x).max(0.0), (b - y).max(0.0))
    }

    /// True when the area is zero.
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
}

/// How much room a node is given on one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Constraint {
    /// The node is exactly this size on the axis.
    Exact(f32),
    /// Shrink-to-fit, but no larger than this.
    AtMost(f32),
    /// Indefinite: content size.
    Unbounded,
}

impl Constraint {
    /// The bound, for `Exact` and `AtMost`.
    pub fn bound(self) -> Option<f32> {
        match self {
            Self::Exact(v) | Self::AtMost(v) => Some(v),
            Self::Unbounded => None,
        }
    }

    /// The bound minus `inset`, keeping the variant; never below zero.
    pub fn shrink(self, inset: f32) -> Self {
        match self {
            Self::Exact(v) => Self::Exact((v - inset).max(0.0)),
            Self::AtMost(v) => Self::AtMost((v - inset).max(0.0)),
            Self::Unbounded => Self::Unbounded,
        }
    }

    /// `Exact` becomes `AtMost`; used when handing a parent's fixed size down
    /// as a shrink-to-fit bound.
    pub fn loosen(self) -> Self {
        match self {
            Self::Exact(v) => Self::AtMost(v),
            other => other,
        }
    }

    /// Bits for memo keys; `f32` is not `Hash`.
    pub(crate) fn key(self) -> (u8, u32) {
        match self {
            Self::Exact(v) => (0, v.to_bits()),
            Self::AtMost(v) => (1, v.to_bits()),
            Self::Unbounded => (2, 0),
        }
    }
}
