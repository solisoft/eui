//! The fixed scales (`spec/05-theme.md` §2), at cozy density and font scale 1.

/// `space` entries, index 0–12, px.
pub const SPACE: [f32; 13] = [0.0, 2.0, 4.0, 8.0, 12.0, 16.0, 20.0, 24.0, 32.0, 40.0, 48.0, 64.0, 96.0];

/// `text` entries as `(size, line height)`, index 0–7, px.
pub const TEXT: [(f32, f32); 8] = [
    (11.0, 16.0),
    (13.0, 18.0),
    (15.0, 22.0),
    (17.0, 24.0),
    (20.0, 28.0),
    (24.0, 32.0),
    (30.0, 38.0),
    (38.0, 46.0),
];

/// `shadow` entries as `(y offset, blur, opacity)`, index 0–3.
pub const SHADOW: [(f32, f32, f32); 4] = [(0.0, 0.0, 0.0), (1.0, 2.0, 0.12), (4.0, 12.0, 0.16), (12.0, 32.0, 0.24)];

/// `motion` durations in ms, index 0–2.
pub const MOTION: [u16; 3] = [100, 180, 320];

/// The easing curve shared by every motion entry, as cubic-bezier control points.
pub const EASING: [f32; 4] = [0.2, 0.0, 0.0, 1.0];

/// `control` heights, `sm md lg`, px.
pub const CONTROL: [f32; 3] = [28.0, 36.0, 44.0];

/// The value a `radius.full` resolves to.
pub const RADIUS_FULL: f32 = 9999.0;

/// Number of `radius` entries.
pub const RADIUS_LEN: usize = 5;
