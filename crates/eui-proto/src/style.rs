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
use crate::limits::{MAX_GRADIENT_ANGLE, MAX_GRADIENT_STOPS, STYLE_RECORD_BYTES};
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

/// Which font role to shape with.
///
/// Zero and one are the client's own faces and are always available. Two and
/// up are the application's, bound to their faces by `DefFont` (02 §5) and
/// carried as assets like every other byte a server chooses. Written by hand
/// rather than through `u8_enum!` because the open half is the point: the
/// wire format reserved `2+` for granted font roles before there was
/// anything to put there, and a decoder that refuses them refuses the
/// extension it documents.
///
/// A role a session never bound is still a legal byte. It is not the
/// decoder's business: the tree knows which roles were defined, and the text
/// engine falls back to `Sans` for one that was not — a missing face draws
/// the text in another face, never nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontFamily {
    /// Proportional UI face.
    Sans,
    /// Fixed-pitch face.
    Mono,
    /// A face the application supplied, `2..=255`.
    Role(u8),
}

impl FontFamily {
    /// Decode from the wire. Every byte is a role, so this cannot fail; it
    /// returns a `Result` so the field decodes like its neighbours.
    pub const fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::Sans),
            1 => Ok(Self::Mono),
            r => Ok(Self::Role(r)),
        }
    }

    /// The wire discriminant.
    pub const fn to_u8(self) -> u8 {
        match self {
            Self::Sans => 0,
            Self::Mono => 1,
            Self::Role(r) => r,
        }
    }
}

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

/// A colour: a theme role, a literal from the session table, a gradient
/// from the session table (a `bg` only), or nothing.
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
    /// The gradient range, `0x4000..=0x7FFF` (02 §3.2), carved from the
    /// reserved role ids by version 6.
    const GRADIENT_BIT: u16 = 0x4000;

    /// A theme colour role.
    pub const fn role(id: u16) -> Self {
        Self(id & 0x7FFF)
    }

    /// An entry in the session's literal colour table.
    pub const fn literal(index: u16) -> Self {
        Self((index & 0x7FFF) | Self::LITERAL_BIT)
    }

    /// An entry in the session's gradient table (02 §5.3). Only a record's
    /// `bg` may carry one.
    pub const fn gradient(id: u16) -> Self {
        Self((id & 0x3FFF) | Self::GRADIENT_BIT)
    }

    /// True when this is a literal rather than a role.
    pub const fn is_literal(self) -> bool {
        self.0 & Self::LITERAL_BIT != 0
    }

    /// True when this names a gradient: `0x4000..=0x7FFF`.
    pub const fn is_gradient(self) -> bool {
        !self.is_literal() && self.0 & Self::GRADIENT_BIT != 0
    }

    /// The gradient id this names, or `None` for anything else.
    pub const fn gradient_id(self) -> Option<u16> {
        if self.is_gradient() {
            Some(self.0 & 0x3FFF)
        } else {
            None
        }
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
/// `animation`: the node's painting is kept after it is released, and leaves
/// over its `transition` duration, along the accelerate curve (03 §5).
///
/// The mirror of [`ANIMATION_ENTER`], and the reason `animation` is a bit set
/// rather than an enumeration: a node has to say how it will leave while it
/// is still there to say it. There is no later chance — the op that removes a
/// node is the op that removes it, and a `SetStyle` aimed at one on its way
/// out would be a style change on something already gone.
pub const ANIMATION_EXIT: u8 = 4;
/// `animation`: everything painted for the node is drawn at its opacity
/// times a factor that falls to a half and back every 2 s — Tailwind's
/// `animate-pulse` (03 §5). Version 6.
pub const ANIMATION_PULSE: u8 = 8;
/// `animation`: everything painted for the node is lifted a quarter of its
/// height and let fall, once a second — Tailwind's `animate-bounce`
/// (03 §5). Painting only: layout and hit testing do not move. Version 6.
pub const ANIMATION_BOUNCE: u8 = 16;
/// Every `animation` bit this revision defines. A record setting anything
/// outside it is refused, so a bit meaning something later cannot be read as
/// nothing today.
pub const ANIMATION_MASK: u8 = ANIMATION_SPIN | ANIMATION_ENTER | ANIMATION_EXIT | ANIMATION_PULSE | ANIMATION_BOUNCE;
/// The `animation` bits a session below version 6 knows: its decoder refuses
/// the rest, so [`StyleRecord::for_protocol`] takes them off.
const ANIMATION_BEFORE_6: u8 = ANIMATION_SPIN | ANIMATION_ENTER | ANIMATION_EXIT;

/// One stop of a [`Gradient`]: a colour, and where along the gradient line
/// it sits, in 255ths (02 §5.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GradientStop {
    /// A role or a literal; never none, never a gradient.
    pub color: ColorRef,
    /// `0` the start of the gradient line, `255` its end.
    pub at: u8,
}

/// A linear gradient, as `DefGradient` defines it (02 §5.3).
///
/// Fixed-size rather than a `Vec`, so a session table of them is plain
/// data and a server can key its interning on the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gradient {
    /// `0..=359` degrees, CSS's: `0` to the top, clockwise. `360..=363` the
    /// corner keywords: to top right, bottom right, bottom left, top left.
    pub angle: u16,
    /// How many of `stops` are real: `2..=3`.
    pub count: u8,
    /// The stops, the first `count` of them meaningful and the rest zero.
    pub stops: [GradientStop; MAX_GRADIENT_STOPS],
}

impl Gradient {
    /// `angle`: to top right, the first corner keyword.
    pub const TO_TOP_RIGHT: u16 = 360;
    /// `angle`: to bottom right.
    pub const TO_BOTTOM_RIGHT: u16 = 361;
    /// `angle`: to bottom left.
    pub const TO_BOTTOM_LEFT: u16 = 362;
    /// `angle`: to top left.
    pub const TO_TOP_LEFT: u16 = 363;

    /// A gradient from two or three stops. `None` for any other count.
    pub fn new(angle: u16, stops: &[GradientStop]) -> Option<Self> {
        if !(2..=MAX_GRADIENT_STOPS).contains(&stops.len()) {
            return None;
        }
        let mut out = [GradientStop::default(); MAX_GRADIENT_STOPS];
        for (slot, s) in out.iter_mut().zip(stops) {
            *slot = *s;
        }
        Some(Self { angle, count: u8::try_from(stops.len()).ok()?, stops: out })
    }

    /// The stops that are real.
    pub fn stops(&self) -> &[GradientStop] {
        self.stops.get(..usize::from(self.count)).unwrap_or(&[])
    }

    /// The colour the gradient starts from: what a session below version 6
    /// is sent as a solid `bg` instead (02 §5.3).
    pub fn first(&self) -> ColorRef {
        self.stops.first().map_or(ColorRef::NONE, |s| s.color)
    }

    /// Decode the payload after the id.
    pub fn decode(r: &mut Reader<'_>) -> Result<Self> {
        let angle = r.u16()?;
        if angle > MAX_GRADIENT_ANGLE {
            return Err(DecodeError::IllegalValue("gradient angle is 0-359 degrees or a corner, 360-363"));
        }
        let count = r.u8()?;
        if !(2..=MAX_GRADIENT_STOPS).contains(&usize::from(count)) {
            return Err(DecodeError::LimitExceeded("gradient stops"));
        }
        let mut stops = [GradientStop::default(); MAX_GRADIENT_STOPS];
        let mut last = 0u8;
        for slot in stops.iter_mut().take(usize::from(count)) {
            let color = ColorRef(r.u16()?);
            if color.is_none() || color.is_gradient() {
                return Err(DecodeError::IllegalValue("a gradient stop is a role or a literal"));
            }
            let at = r.u8()?;
            if at < last {
                return Err(DecodeError::IllegalValue("gradient stops go forwards"));
            }
            last = at;
            *slot = GradientStop { color, at };
        }
        Ok(Self { angle, count, stops })
    }

    /// Encode the payload after the id.
    pub fn encode(&self, w: &mut Writer) {
        w.u16(self.angle).u8(self.count);
        for s in self.stops() {
            w.u16(s.color.0).u8(s.at);
        }
    }
}

u8_enum!(
    /// Which way an [`ANIMATION_ENTER`] arrives, and an [`ANIMATION_EXIT`]
    /// leaves (03 §5).
    ///
    /// A direction and never a duration: the duration is `transition`, and
    /// keeping the two apart is what stops a page from carrying a timing
    /// nobody gets right twice (05 §2).
    Motion, "motion", {
        /// No movement: the fade an entrance has always been.
        Fade = 0,
        /// From, or to, beyond the leading edge of the parent's content box.
        Leading = 1,
        /// Likewise the trailing edge.
        Trailing = 2,
        /// Likewise the top.
        Top = 3,
        /// Likewise the bottom.
        Bottom = 4,
        /// From, or to, 92 % about the node's own centre.
        Scale = 5,
        /// This node pairs with the one carrying the same `key` on the other
        /// side of the change, and flies between their two boxes.
        Paired = 6,
    }
);

impl Motion {
    /// The way out that matches this way in, and the other way about.
    ///
    /// A page that leaves is never told where to go: it goes wherever the
    /// page arriving beside it did not come from. That is the whole of what
    /// makes a push and a pop mirror each other, and it is why the leaving
    /// record needs no direction of its own.
    #[must_use]
    pub const fn mirrored(self) -> Self {
        match self {
            Self::Leading => Self::Trailing,
            Self::Trailing => Self::Leading,
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
            other => other,
        }
    }
}

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
    /// A bit set: [`ANIMATION_SPIN`], [`ANIMATION_ENTER`], [`ANIMATION_EXIT`]
    /// (03 §5). Bits outside [`ANIMATION_MASK`] are refused.
    pub animation: u8,
    /// Backdrop blur: the standard deviation, in device-independent px, of
    /// the Gaussian the node sees its backdrop through; `0` is none (03 §2).
    pub blur: u8,
    /// Which way this record's entrance arrives and its exit leaves (03 §5).
    ///
    /// Offset 63, which was the record's last reserved byte. §3 of 02 fixes
    /// the record at 64 bytes, so there was never more than one further field
    /// in it; this is that field, and the next one is a wider record and a
    /// version bump.
    pub motion: Motion,
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
            motion: Motion::Fade,
        }
    }
}

/// How many `space` steps a session at `version` has (05 §2): thirteen
/// until version 6 appended `1.5 2.5 3.5 20 32`, in Tailwind's names.
pub const fn space_steps(version: u32) -> usize {
    if version >= 6 {
        18
    } else {
        13
    }
}

// Every appended step has a fallback, and nothing else does.
const _: () = assert!(space_steps(6) - space_steps(5) == SPACE_FALLBACK.len());

/// The step each `space` index from 13 up falls back to for a session
/// below version 6 (05 §2): the nearest older step, ties going down. Every
/// one but the last sits exactly half-way, so the rule is the table.
pub const SPACE_FALLBACK: [u8; 5] = [2, 3, 4, 11, 12];

impl StyleRecord {
    /// This record as a session at `version` can take it: every `space`
    /// index that version does not have is replaced by its
    /// [`SPACE_FALLBACK`] (05 §2), and the `animation` bits it does not
    /// have — [`ANIMATION_PULSE`], [`ANIMATION_BOUNCE`] (03 §5) — are taken
    /// off. A server calls it on the way to `DefStyle`, so a view is written
    /// once and each session is sent what its client can draw. At version 6
    /// and above it is the identity.
    ///
    /// A gradient `bg` is not rewritten here, because a record does not
    /// carry the gradient it names: the server that interned it sends a
    /// session below 6 the first stop in its place (02 §5.3). One that
    /// reaches here all the same is taken off rather than sent as a role id
    /// the older client does not know.
    #[must_use]
    pub fn for_protocol(mut self, version: u32) -> Self {
        if version >= 6 {
            return self;
        }
        self.animation &= ANIMATION_BEFORE_6;
        if self.bg.is_gradient() {
            self.bg = ColorRef::NONE;
        }
        let down = |ix: u8| match ix.checked_sub(13) {
            Some(n) => SPACE_FALLBACK.get(usize::from(n)).copied().unwrap_or(ix),
            None => ix,
        };
        self.gap = down(self.gap);
        for ix in self.padding.iter_mut().chain(self.margin.iter_mut()) {
            *ix = down(*ix);
        }
        for d in [&mut self.basis, &mut self.width, &mut self.height, &mut self.min_width, &mut self.min_height, &mut self.max_width, &mut self.max_height] {
            if let Dim::Space(ix) = d {
                *ix = down(*ix);
            }
        }
        self
    }

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
            motion: Motion::from_u8(f.u8()?)?,
        };
        if out.transition > 5 {
            return Err(DecodeError::IllegalValue("transition is a motion index + 1, at most 5"));
        }
        if out.animation & !ANIMATION_MASK != 0 {
            return Err(DecodeError::IllegalValue("animation is a bit set of 1 (spin), 2 (enter), 4 (exit), 8 (pulse) and 16 (bounce)"));
        }
        // 02 §3.2: a gradient is a background. Id 0 names none, and a
        // gradient anywhere but `bg` is a colour asked for and a picture
        // given, which two clients would draw two ways.
        if out.bg.gradient_id() == Some(0) {
            return Err(DecodeError::IllegalValue("gradient id 0"));
        }
        if out.fg.is_gradient() || out.border_color.is_gradient() {
            return Err(DecodeError::IllegalValue("only bg may name a gradient"));
        }
        // A direction with nothing to direct. Refusing it costs a server one
        // more interned record in the rare mixed case, and is what keeps two
        // implementations from disagreeing about a byte one of them ignored.
        if out.motion != Motion::Fade && out.animation & (ANIMATION_ENTER | ANIMATION_EXIT) == 0 {
            return Err(DecodeError::IllegalValue("motion needs an entrance or an exit to belong to"));
        }

        if out.text_decoration & !0b11 != 0 {
            return Err(DecodeError::IllegalValue("text_decoration has unknown bits"));
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
            .u8(self.motion.to_u8());
    }
}
