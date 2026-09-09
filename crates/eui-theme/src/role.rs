//! Colour roles (`spec/05-theme.md` §1).

use crate::error::ThemeError;

/// One of the 28 colour roles. The discriminant is the wire id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
#[allow(missing_docs)]
pub enum Role {
    SurfaceBase = 1,
    SurfaceRaised = 2,
    SurfaceSunken = 3,
    SurfaceOverlay = 4,
    TextDefault = 5,
    TextMuted = 6,
    TextInverted = 7,
    TextDisabled = 8,
    AccentBase = 9,
    AccentHover = 10,
    AccentActive = 11,
    AccentOn = 12,
    SuccessBase = 13,
    SuccessSubtle = 14,
    SuccessOn = 15,
    WarningBase = 16,
    WarningSubtle = 17,
    WarningOn = 18,
    DangerBase = 19,
    DangerSubtle = 20,
    DangerOn = 21,
    InfoBase = 22,
    InfoSubtle = 23,
    InfoOn = 24,
    BorderSubtle = 25,
    BorderDefault = 26,
    BorderStrong = 27,
    FocusRing = 28,
}

impl Role {
    /// Every role, in id order.
    pub const ALL: [Role; 28] = [
        Role::SurfaceBase,
        Role::SurfaceRaised,
        Role::SurfaceSunken,
        Role::SurfaceOverlay,
        Role::TextDefault,
        Role::TextMuted,
        Role::TextInverted,
        Role::TextDisabled,
        Role::AccentBase,
        Role::AccentHover,
        Role::AccentActive,
        Role::AccentOn,
        Role::SuccessBase,
        Role::SuccessSubtle,
        Role::SuccessOn,
        Role::WarningBase,
        Role::WarningSubtle,
        Role::WarningOn,
        Role::DangerBase,
        Role::DangerSubtle,
        Role::DangerOn,
        Role::InfoBase,
        Role::InfoSubtle,
        Role::InfoOn,
        Role::BorderSubtle,
        Role::BorderDefault,
        Role::BorderStrong,
        Role::FocusRing,
    ];

    /// The highest defined id.
    pub const MAX_ID: u16 = 28;

    /// From a wire id; `0` and anything above [`Self::MAX_ID`] are rejected.
    pub fn from_id(id: u16) -> Result<Self, ThemeError> {
        Self::ALL.get(usize::from(id).wrapping_sub(1)).copied().ok_or(ThemeError::UnknownRole(id))
    }

    /// The wire id.
    pub const fn id(self) -> u16 {
        self as u16
    }

    /// The role written as in the spec, `accent.base`, if it is one.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|r| r.name() == name)
    }

    /// The role's name as written in the spec, `surface.base`.
    pub const fn name(self) -> &'static str {
        match self {
            Role::SurfaceBase => "surface.base",
            Role::SurfaceRaised => "surface.raised",
            Role::SurfaceSunken => "surface.sunken",
            Role::SurfaceOverlay => "surface.overlay",
            Role::TextDefault => "text.default",
            Role::TextMuted => "text.muted",
            Role::TextInverted => "text.inverted",
            Role::TextDisabled => "text.disabled",
            Role::AccentBase => "accent.base",
            Role::AccentHover => "accent.hover",
            Role::AccentActive => "accent.active",
            Role::AccentOn => "accent.on",
            Role::SuccessBase => "success.base",
            Role::SuccessSubtle => "success.subtle",
            Role::SuccessOn => "success.on",
            Role::WarningBase => "warning.base",
            Role::WarningSubtle => "warning.subtle",
            Role::WarningOn => "warning.on",
            Role::DangerBase => "danger.base",
            Role::DangerSubtle => "danger.subtle",
            Role::DangerOn => "danger.on",
            Role::InfoBase => "info.base",
            Role::InfoSubtle => "info.subtle",
            Role::InfoOn => "info.on",
            Role::BorderSubtle => "border.subtle",
            Role::BorderDefault => "border.default",
            Role::BorderStrong => "border.strong",
            Role::FocusRing => "focus.ring",
        }
    }
}
