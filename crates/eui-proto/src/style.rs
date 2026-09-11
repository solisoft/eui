//! Computed style: the 64-byte record of `spec/02-wire-format.md` §3.
//!
//! Everything in here is *already resolved*. There is no cascade, no
//! specificity, no inheritance to walk — a client's entire styling cost is one
//! indexed lookup into the session's style table. A thousand table rows share
//! three ids.
//!
//! Enumerated fields reject unknown values rather than clamping them. Clamping
//! is how two implementations quietly disagree about a layout for a year.

use crate::error::{DecodeError, Result};
use crate::limits::STYLE_RECORD_BYTES;
use crate::reader::Reader;
use crate::writer::Writer;

macro_rules! u8_enum {
    (
        $(#[$outer:meta])*
        $name:ident, $what:expr, { $( $(#[$inner:meta])* $variant:ident = $value:expr ),+ $(,)? }
    ) => {
        $(#[$outer])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum $name {
            $( $(#[$inner])* $variant = $value ),+
        }

        impl $name {
            /// Decode from the wire, rejecting undefined discriminants.
            pub const fn from_u8(v: u8) -> Result<Self> {
                match v {
                    $( $value => Ok(Self::$variant), )+
                    _ => Err(DecodeError::UnknownTag($what)),
                }
            }

            /// The wire discriminant.
            pub const fn to_u8(self) -> u8 {
                self as u8
            }
        }
    };
}

u8_enum!(
    /// How a node arranges its children.
    Display, "display", {
        /// Children flow left to right.
        Row = 0,
        /// Children flow top to bottom.
        Column = 1,
        /// Children overlap, ordered by `z`.
        Stack = 2,
        /// Children are placed on tracks.
        Grid = 3,
        /// The node and its subtree are not laid out and not drawn.
        None = 4,
    }
);

u8_enum!(
    /// Whether a flow wraps onto further lines.
    Wrap, "wrap", {
        /// Single line.
        NoWrap = 0,
        /// Wrap forward onto new lines.
        Wrap = 1,
        /// Wrap backward.
        WrapReverse = 2,
    }
);

u8_enum!(
    /// Distribution along the main axis.
    Justify, "justify", {
        /// Pack at the start.
        Start = 0,
        /// Pack at the centre.
        Center = 1,
        /// Pack at the end.
        End = 2,
        /// Space between items.
        Between = 3,
        /// Half-space at the edges.
        Around = 4,
        /// Equal space everywhere.
        Evenly = 5,
    }
);

u8_enum!(
    /// Alignment across the cross axis.
    AlignItems, "align_items", {
        /// Cross-start.
        Start = 0,
        /// Cross-centre.
        Center = 1,
        /// Cross-end.
        End = 2,
        /// Fill the cross axis.
        Stretch = 3,
        /// Align text baselines.
        Baseline = 4,
    }
);

u8_enum!(
    /// A child's own cross-axis alignment.
    AlignSelf, "align_self", {
        /// Cross-start.
        Start = 0,
        /// Cross-centre.
        Center = 1,
        /// Cross-end.
        End = 2,
        /// Fill the cross axis.
        Stretch = 3,
        /// Align text baselines.
        Baseline = 4,
        /// Defer to the parent's `align_items`.
        Auto = 5,
    }
);

u8_enum!(
    /// Which font role to shape with.
    FontFamily, "font_family", {
        /// Proportional UI face.
        Sans = 0,
        /// Fixed-pitch face.
        Mono = 1,
    }
);

u8_enum!(
    /// Weight, as a role rather than a numeric axis value.
    FontWeight, "font_weight", {
        /// Body text.
        Regular = 0,
        /// Slightly emphasised.
        Medium = 1,
        /// Headings and labels.
        Semibold = 2,
        /// Strong emphasis.
        Bold = 3,
    }
);

u8_enum!(
    /// Horizontal alignment of text within its box.
    TextAlign, "text_align", {
        /// Leading edge.
        Start = 0,
        /// Centred.
        Center = 1,
        /// Trailing edge.
        End = 2,
        /// Justified.
        Justify = 3,
    }
);

u8_enum!(
    /// What happens to content past the node's box.
    Overflow, "overflow", {
        /// Draw outside the box.
        Visible = 0,
        /// Clip to the box.
        Clip = 1,
        /// Clip and offer scrolling.
        Scroll = 2,
    }
);

u8_enum!(
    /// Whether a node participates in its parent's flow.
    Position, "position", {
        /// Laid out by the parent.
        Flow = 0,
        /// Positioned within a `stack` parent.
        Absolute = 1,
        /// Out of flow like `Absolute`, but placed at the pointer rather than
        /// against its anchor: a tooltip follows the hand, and only the client
        /// knows where the hand is.
        Pointer = 2,
    }
);

impl Position {
    /// Out of the parent's flow: sized to its content, placed rather than
    /// laid out, and never counted into the box it sits in.
    #[must_use]
    pub const fn out_of_flow(self) -> bool {
        matches!(self, Self::Absolute | Self::Pointer)
    }
}

u8_enum!(
    /// Pointer shape over the node.
    Cursor, "cursor", {
        /// System default.
        Default = 0,
        /// Actionable.
        Pointer = 1,
        /// Text selection.
        Text = 2,
        /// Draggable.
        Grab = 3,
        /// Being dragged.
        Grabbing = 4,
        /// Horizontal resize.
        ResizeH = 5,
        /// Vertical resize.
        ResizeV = 6,
        /// Busy.
        Wait = 7,
        /// Unavailable.
        NotAllowed = 8,
    }
);

/// A length, in one of the five forms the layout algorithm understands.
///
/// There is no `calc()` and no unit arithmetic. A server that wants a computed
/// length computes it; the wire carries the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dim {
    /// Determined by content and constraints.
    #[default]
    Auto,
    /// Device-independent pixels.
    Px(u16),
    /// Hundredths of a percent of the parent's content box (`5000` = 50 %).
    Percent(u16),
    /// Hundredths of a flex fraction.
    Fr(u16),
    /// An index into the theme's `space` scale.
    Space(u8),
}

impl Dim {
    /// Decode the 3-byte wire form.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let tag = r.u8()?;
        let value = r.u16()?;
        match tag {
            0 if value == 0 => Ok(Self::Auto),
            0 => Err(DecodeError::IllegalValue("Dim::Auto carries a value")),
            1 => Ok(Self::Px(value)),
            2 => Ok(Self::Percent(value)),
            3 => Ok(Self::Fr(value)),
            4 => u8::try_from(value).map(Self::Space).map_err(|_| DecodeError::IllegalValue("space index above 255")),
            _ => Err(DecodeError::UnknownTag("Dim")),
        }
    }

    /// Encode the 3-byte wire form.
    pub fn encode(self, w: &mut Writer) {
        let (tag, value) = match self {
            Self::Auto => (0u8, 0u16),
            Self::Px(v) => (1, v),
            Self::Percent(v) => (2, v),
            Self::Fr(v) => (3, v),
            Self::Space(v) => (4, u16::from(v)),
        };
        w.u8(tag).u16(value);
    }
}

/// A colour: a theme role, a literal from the session table, or nothing.
///
/// Roles are strongly preferred. Only a role follows the viewer's light/dark
/// mode, contrast preference and density — a literal is frozen at whatever the
/// designer picked, which is right for a brand mark and wrong for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ColorRef(pub u16);

impl ColorRef {
    /// No colour: inherit from the parent, or draw nothing.
    pub const NONE: Self = Self(0);
    const LITERAL_BIT: u16 = 0x8000;

    /// A theme colour role.
    pub const fn role(id: u16) -> Self {
        Self(id & 0x7FFF)
    }

    /// An entry in the session's literal colour table.
    pub const fn literal(index: u16) -> Self {
        Self((index & 0x7FFF) | Self::LITERAL_BIT)
    }

    /// True when this is a literal rather than a role.
    pub const fn is_literal(self) -> bool {
        self.0 & Self::LITERAL_BIT != 0
    }

    /// True when nothing should be drawn.
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    /// The role id or literal index, without the discriminating bit.
    pub const fn index(self) -> u16 {
        self.0 & 0x7FFF
    }
}

/// `animation`: the node's painting turns about its own centre, one
/// revolution every 1.2 s, for as long as it is on screen (03 §5).
pub const ANIMATION_SPIN: u8 = 1;
/// `animation`: the node fades in — and frosts in, if it wears a `blur` —
/// when it is mounted, over its `transition` duration (03 §5).
///
/// This is the one exception to "a node that is mounted appears at once",
/// and it is opt-in for exactly that reason: a dialog should arrive, but a
/// button that carries a transition for its hover should not fade in every
/// time a resync rebuilds the tree.
pub const ANIMATION_ENTER: u8 = 2;

/// The 64-byte computed style record.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyleRecord {
    /// Child arrangement.
    pub display: Display,
    /// Line wrapping in a flow.
    pub wrap: Wrap,
    /// Main-axis distribution.
    pub justify: Justify,
    /// Cross-axis alignment of children.
    pub align_items: AlignItems,
    /// This node's own cross-axis alignment.
    pub align_self: AlignSelf,
    /// Flex grow factor.
    pub grow: u8,
    /// Flex shrink factor.
    pub shrink: u8,
    /// Gap between children, as a `space` scale index.
    pub gap: u8,
    /// Flex basis.
    pub basis: Dim,
    /// Preferred width.
    pub width: Dim,
    /// Preferred height.
    pub height: Dim,
    /// Lower bound on width.
    pub min_width: Dim,
    /// Lower bound on height.
    pub min_height: Dim,
    /// Upper bound on width.
    pub max_width: Dim,
    /// Upper bound on height.
    pub max_height: Dim,
    /// Padding as `space` indices: top, right, bottom, left.
    pub padding: [u8; 4],
    /// Margin as `space` indices: top, right, bottom, left.
    pub margin: [u8; 4],
    /// Background fill.
    pub bg: ColorRef,
    /// Foreground, which text inherits.
    pub fg: ColorRef,
    /// Border stroke.
    pub border_color: ColorRef,
    /// Border width in px: top, right, bottom, left.
    pub border_width: [u8; 4],
    /// Corner radius, as a `radius` scale index.
    pub radius: u8,
    /// Drop shadow, as a `shadow` scale index.
    pub shadow: u8,
    /// Opacity, 0–255.
    pub opacity: u8,
    /// Font role.
    pub font_family: FontFamily,
    /// Size, as a `text` scale index.
    pub font_size: u8,
    /// Weight role.
    pub font_weight: FontWeight,
    /// Text alignment.
    pub text_align: TextAlign,
    /// Maximum lines before ellipsis; 0 is unlimited.
    pub line_clamp: u8,
    /// Bitfield: 1 underline, 2 strikethrough.
    pub text_decoration: u8,
    /// Overflow handling.
    pub overflow: Overflow,
    /// Flow participation.
    pub position: Position,
    /// Stacking order within the parent.
    pub z: u8,
    /// Pointer shape.
    pub cursor: Cursor,
    /// `0` none, else a `motion` scale index + 1: colours and opacity
    /// animate into this record when a node's style changes to it.
    pub transition: u8,
    /// `0` none, `1` [`ANIMATION_SPIN`], `2` [`ANIMATION_ENTER`] (03 §5).
    pub animation: u8,
    /// Backdrop blur: the standard deviation, in device-independent px, of
    /// the Gaussian the node sees its backdrop through; `0` is none (03 §2).
    pub blur: u8,
}

impl Default for StyleRecord {
    /// The neutral record: a transparent row that inherits everything it can.
    fn default() -> Self {
        Self {
            display: Display::Row,
            wrap: Wrap::NoWrap,
            justify: Justify::Start,
            align_items: AlignItems::Stretch,
            align_self: AlignSelf::Auto,
            grow: 0,
            shrink: 1,
            gap: 0,
            basis: Dim::Auto,
            width: Dim::Auto,
            height: Dim::Auto,
            min_width: Dim::Auto,
            min_height: Dim::Auto,
            max_width: Dim::Auto,
            max_height: Dim::Auto,
            padding: [0; 4],
            margin: [0; 4],
            bg: ColorRef::NONE,
            fg: ColorRef::NONE,
            border_color: ColorRef::NONE,
            border_width: [0; 4],
            radius: 0,
            shadow: 0,
            opacity: 255,
            font_family: FontFamily::Sans,
            // Index 2 is `base` on the text scale (`spec/05-theme.md` §2); a
            // default of 0 would be `xs`.
            font_size: 2,
            font_weight: FontWeight::Regular,
            text_align: TextAlign::Start,
            line_clamp: 0,
            text_decoration: 0,
            overflow: Overflow::Visible,
            position: Position::Flow,
            z: 0,
            cursor: Cursor::Default,
            transition: 0,
            animation: 0,
            blur: 0,
        }
    }
}

impl StyleRecord {
    /// Decode exactly [`STYLE_RECORD_BYTES`] bytes.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let raw = r.take(STYLE_RECORD_BYTES)?;
        let mut f = Reader::new(raw);

        let out = Self {
            display: Display::from_u8(f.u8()?)?,
            wrap: Wrap::from_u8(f.u8()?)?,
            justify: Justify::from_u8(f.u8()?)?,
            align_items: AlignItems::from_u8(f.u8()?)?,
            align_self: AlignSelf::from_u8(f.u8()?)?,
            grow: f.u8()?,
            shrink: f.u8()?,
            gap: f.u8()?,
            basis: Dim::decode(&mut f)?,
            width: Dim::decode(&mut f)?,
            height: Dim::decode(&mut f)?,
            min_width: Dim::decode(&mut f)?,
            min_height: Dim::decode(&mut f)?,
            max_width: Dim::decode(&mut f)?,
            max_height: Dim::decode(&mut f)?,
            padding: f.array::<4>()?,
            margin: f.array::<4>()?,
            bg: ColorRef(f.u16()?),
            fg: ColorRef(f.u16()?),
            border_color: ColorRef(f.u16()?),
            border_width: f.array::<4>()?,
            radius: f.u8()?,
            shadow: f.u8()?,
            opacity: f.u8()?,
            font_family: FontFamily::from_u8(f.u8()?)?,
            font_size: f.u8()?,
            font_weight: FontWeight::from_u8(f.u8()?)?,
            text_align: TextAlign::from_u8(f.u8()?)?,
            line_clamp: f.u8()?,
            text_decoration: f.u8()?,
            overflow: Overflow::from_u8(f.u8()?)?,
            position: Position::from_u8(f.u8()?)?,
            z: f.u8()?,
            cursor: Cursor::from_u8(f.u8()?)?,
            transition: f.u8()?,
            animation: f.u8()?,
            blur: f.u8()?,
        };
        if out.transition > 3 {
            return Err(DecodeError::IllegalValue("transition is a motion index + 1, at most 3"));
        }
        if out.animation > ANIMATION_ENTER {
            return Err(DecodeError::IllegalValue("animation is 0, 1 (spin) or 2 (enter)"));
        }

        if out.text_decoration & !0b11 != 0 {
            return Err(DecodeError::IllegalValue("text_decoration has unknown bits"));
        }
        // One byte of the tail is left. Accepting garbage there today would
        // make that last field unusable, because some deployed server would
        // already be putting something else in it.
        if f.array::<1>()? != [0; 1] {
            return Err(DecodeError::IllegalValue("the reserved byte must be zero"));
        }
        f.finish()?;
        Ok(out)
    }

    /// Encode exactly [`STYLE_RECORD_BYTES`] bytes.
    pub fn encode(&self, w: &mut Writer) {
        w.u8(self.display.to_u8()).u8(self.wrap.to_u8()).u8(self.justify.to_u8()).u8(self.align_items.to_u8()).u8(self.align_self.to_u8()).u8(self.grow).u8(self.shrink).u8(self.gap);
        for d in [self.basis, self.width, self.height, self.min_width, self.min_height, self.max_width, self.max_height] {
            d.encode(w);
        }
        w.raw(&self.padding)
            .raw(&self.margin)
            .u16(self.bg.0)
            .u16(self.fg.0)
            .u16(self.border_color.0)
            .raw(&self.border_width)
            .u8(self.radius)
            .u8(self.shadow)
            .u8(self.opacity)
            .u8(self.font_family.to_u8())
            .u8(self.font_size)
            .u8(self.font_weight.to_u8())
            .u8(self.text_align.to_u8())
            .u8(self.line_clamp)
            .u8(self.text_decoration)
            .u8(self.overflow.to_u8())
            .u8(self.position.to_u8())
            .u8(self.z)
            .u8(self.cursor.to_u8())
            .u8(self.transition)
            .u8(self.animation)
            .u8(self.blur)
            .raw(&[0; 1]);
    }
}
