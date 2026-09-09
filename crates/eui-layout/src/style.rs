//! A `StyleRecord` with its scale indices resolved to px for one viewer.

use eui_proto::{AlignItems, AlignSelf, Dim, Display, Justify, Position, StyleRecord, Wrap};
use eui_theme::Resolved;

use crate::geom::Constraint;
use crate::measure::FontSpec;

/// A length after scale resolution, before layout decides `Auto`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Length {
    /// Layout decides.
    Auto,
    /// Fixed px.
    Px(f32),
    /// Fraction of the available bound, `0.5` is 50 %.
    Fraction(f32),
}

impl Length {
    fn from_dim(d: Dim, theme: &Resolved) -> Self {
        match d {
            Dim::Auto | Dim::Fr(_) => Self::Auto,
            Dim::Px(n) => Self::Px(f32::from(n)),
            Dim::Percent(n) => Self::Fraction(f32::from(n) / 10_000.0),
            Dim::Space(i) => theme.space(i).map_or(Self::Auto, Self::Px),
        }
    }

    /// Resolve against a constraint's bound; `Fraction` of `Unbounded` stays
    /// unresolved.
    pub fn resolve(self, against: Constraint) -> Option<f32> {
        match self {
            Self::Auto => None,
            Self::Px(v) => Some(v),
            Self::Fraction(f) => against.bound().map(|b| b * f),
        }
    }
}

/// Four edges, px: top, right, bottom, left.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Edges {
    /// Top.
    pub t: f32,
    /// Right.
    pub r: f32,
    /// Bottom.
    pub b: f32,
    /// Left.
    pub l: f32,
}

impl Edges {
    /// `left + right`.
    pub fn horizontal(&self) -> f32 {
        self.l + self.r
    }

    /// `top + bottom`.
    pub fn vertical(&self) -> f32 {
        self.t + self.b
    }
}

/// The parts of a style layout reads, resolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// Arrangement.
    pub display: Display,
    /// Wrapping.
    pub wrap: Wrap,
    /// Main-axis packing.
    pub justify: Justify,
    /// Children's cross alignment.
    pub align_items: AlignItems,
    /// Own cross alignment.
    pub align_self: AlignSelf,
    /// Flow participation.
    pub position: Position,
    /// Grow factor.
    pub grow: f32,
    /// Shrink factor.
    pub shrink: f32,
    /// Gap between children.
    pub gap: f32,
    /// Flex basis.
    pub basis: Length,
    /// Border-box width.
    pub width: Length,
    /// Border-box height.
    pub height: Length,
    /// Minimum width.
    pub min_width: Length,
    /// Minimum height.
    pub min_height: Length,
    /// Maximum width.
    pub max_width: Length,
    /// Maximum height.
    pub max_height: Length,
    /// Padding.
    pub padding: Edges,
    /// Margin.
    pub margin: Edges,
    /// Border widths.
    pub border: Edges,
    /// Stacking order.
    pub z: u8,
    /// Scrolls on both axes when `true`, else vertically only.
    pub scroll_both: bool,
    /// Text: face, weight, size, line height.
    pub font: FontSpec,
    /// Text: line clamp.
    pub line_clamp: u8,
}

impl Style {
    /// Resolve a record for a viewer's theme.
    pub fn resolve(r: &StyleRecord, theme: &Resolved) -> Self {
        let sp = |i: u8| theme.space(i).unwrap_or(0.0);
        let edges = |e: [u8; 4]| Edges { t: sp(e[0]), r: sp(e[1]), b: sp(e[2]), l: sp(e[3]) };
        let (size, line_height) = theme.text(r.font_size).unwrap_or((15.0, 22.0));
        Self {
            display: r.display,
            wrap: r.wrap,
            justify: r.justify,
            align_items: r.align_items,
            align_self: r.align_self,
            position: r.position,
            grow: f32::from(r.grow),
            shrink: f32::from(r.shrink),
            gap: sp(r.gap),
            basis: Length::from_dim(r.basis, theme),
            width: Length::from_dim(r.width, theme),
            height: Length::from_dim(r.height, theme),
            min_width: Length::from_dim(r.min_width, theme),
            min_height: Length::from_dim(r.min_height, theme),
            max_width: Length::from_dim(r.max_width, theme),
            max_height: Length::from_dim(r.max_height, theme),
            padding: edges(r.padding),
            margin: edges(r.margin),
            border: Edges { t: f32::from(r.border_width[0]), r: f32::from(r.border_width[1]), b: f32::from(r.border_width[2]), l: f32::from(r.border_width[3]) },
            z: r.z,
            scroll_both: r.overflow == eui_proto::Overflow::Scroll,
            font: FontSpec { family: r.font_family, weight: r.font_weight, size, line_height },
            line_clamp: r.line_clamp,
        }
    }

    /// Horizontal padding plus border.
    pub fn inset_h(&self) -> f32 {
        self.padding.horizontal() + self.border.horizontal()
    }

    /// Vertical padding plus border.
    pub fn inset_v(&self) -> f32 {
        self.padding.vertical() + self.border.vertical()
    }

    /// Clamp a width by `min_width`/`max_width`, min winning.
    pub fn clamp_w(&self, w: f32, against: Constraint) -> f32 {
        clamp(w, self.min_width.resolve(against), self.max_width.resolve(against))
    }

    /// Clamp a height by `min_height`/`max_height`, min winning.
    pub fn clamp_h(&self, h: f32, against: Constraint) -> f32 {
        clamp(h, self.min_height.resolve(against), self.max_height.resolve(against))
    }
}

fn clamp(v: f32, min: Option<f32>, max: Option<f32>) -> f32 {
    let v = match max {
        Some(m) => v.min(m),
        None => v,
    };
    match min {
        Some(m) => v.max(m),
        None => v,
    }
}
