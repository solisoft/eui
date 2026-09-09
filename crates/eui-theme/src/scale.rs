//! The fixed scales (`spec/05-theme.md` §2), at cozy density and font scale 1.

/// `space` entries, index 0–12, px.
pub const SPACE: [f32; 13] = [0.0, 2.0, 4.0, 8.0, 12.0, 16.0, 20.0, 24.0, 32.0, 40.0, 48.0, 64.0, 96.0];

/// `text` entries as `(size, line height)`, index 0–7, px.
pub const TEXT: [(f32, f32); 8] = [(11.0, 16.0), (13.0, 18.0), (15.0, 22.0), (17.0, 24.0), (20.0, 28.0), (24.0, 32.0), (30.0, 38.0), (38.0, 46.0)];

/// `shadow` entries as `(y offset, blur, opacity)`, index 0–3.
pub const SHADOW: [(f32, f32, f32); 4] = [(0.0, 0.0, 0.0), (1.0, 2.0, 0.12), (4.0, 12.0, 0.16), (12.0, 32.0, 0.24)];

/// `motion` durations in ms, index 0–2.
pub const MOTION: [u16; 3] = [100, 180, 320];

/// The easing curve shared by every motion entry, as cubic-bezier control
/// points. The same numbers as [`Curve::STANDARD`], kept as an array
/// because that is the shape `05-theme.md` §2 states them in.
pub const EASING: [f32; 4] = [0.2, 0.0, 0.0, 1.0];

/// A cubic Bézier easing curve: the two control points of a path from
/// `(0, 0)` to `(1, 1)`, exactly as CSS's `cubic-bezier()` names them.
///
/// The curve gives `y` for an `x`, and `x` is the fraction of the duration
/// elapsed — so evaluating one means first solving `bezier_x(s) = t` for
/// the parameter `s`, which has no closed form. [`Curve::at`] does what a
/// browser does: Newton–Raphson, with bisection behind it for the flat
/// stretches where the derivative gives no useful step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    /// First control point, x.
    pub x1: f32,
    /// First control point, y.
    pub y1: f32,
    /// Second control point, x.
    pub x2: f32,
    /// Second control point, y.
    pub y2: f32,
}

impl Curve {
    /// The theme's own curve (`05-theme.md` §2): leaves at once, arrives
    /// gently. What a style change eases along.
    pub const STANDARD: Self = Self::new(0.2, 0.0, 0.0, 1.0);
    /// Enters at speed and settles. What something arriving should do —
    /// it was already moving when you first saw it, so it reads as having
    /// come from somewhere rather than having been switched on.
    pub const DECELERATE: Self = Self::new(0.0, 0.0, 0.2, 1.0);
    /// Starts from rest and leaves at speed. What something departing
    /// should do; the mirror of [`Curve::DECELERATE`].
    pub const ACCELERATE: Self = Self::new(0.4, 0.0, 1.0, 1.0);
    /// Symmetric, rest to rest — a keyboard scroll, which begins and ends
    /// stationary and should not snap at either end.
    pub const SMOOTH: Self = Self::new(0.45, 0.0, 0.55, 1.0);
    /// No easing at all. Correct for a value that is not a movement — a
    /// progress bar tracking real work, which should not lie about pace.
    pub const LINEAR: Self = Self::new(0.0, 0.0, 1.0, 1.0);

    /// A curve from its two control points.
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// The eased fraction at `t`, itself a fraction of the duration.
    /// `t` outside `0..=1` is clamped, and the ends are exact.
    pub fn at(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.0 || t >= 1.0 {
            return t;
        }
        // One axis of the curve at parameter `s`, and its slope.
        let bez = |a: f32, b: f32, s: f32| {
            let u = 1.0 - s;
            3.0 * a * u * u * s + 3.0 * b * u * s * s + s * s * s
        };
        let slope = |a: f32, b: f32, s: f32| {
            let u = 1.0 - s;
            3.0 * a * u * (u - 2.0 * s) + 3.0 * b * s * (2.0 * u - s) + 3.0 * s * s
        };
        let mut s = t;
        for _ in 0..8 {
            let dx = bez(self.x1, self.x2, s) - t;
            if dx.abs() < 1e-5 {
                return bez(self.y1, self.y2, s);
            }
            let d = slope(self.x1, self.x2, s);
            if d.abs() < 1e-6 {
                break;
            }
            s = (s - dx / d).clamp(0.0, 1.0);
        }
        // Newton stalled — a curve with a flat run in x. Bisection is
        // slower and cannot fail, and twenty-four halvings put `s` well
        // inside a pixel of anything this drives.
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        let mut s = t;
        for _ in 0..24 {
            let x = bez(self.x1, self.x2, s);
            if (x - t).abs() < 1e-5 {
                break;
            }
            if x < t {
                lo = s;
            } else {
                hi = s;
            }
            s = (lo + hi) * 0.5;
        }
        bez(self.y1, self.y2, s)
    }
}

/// `control` heights, `sm md lg`, px.
pub const CONTROL: [f32; 3] = [28.0, 36.0, 44.0];

/// The value a `radius.full` resolves to.
pub const RADIUS_FULL: f32 = 9999.0;

/// Number of `radius` entries.
pub const RADIUS_LEN: usize = 5;

/// A symmetric ease, in and out — `cubic-bezier(0.45, 0, 0.55, 1)` — for a
/// motion that starts from rest and ends at rest, like a keyboard scroll.
pub fn ease_in_out(t: f32) -> f32 {
    Curve::SMOOTH.at(t)
}

/// The motion easing curve of 05 §4, `cubic-bezier(0.2, 0, 0, 1)`, as
/// progress `0..=1` → eased `0..=1`. Solved for the parameter by Newton's
/// method: five steps are plenty for a monotone curve.
pub fn ease(t: f32) -> f32 {
    Curve::STANDARD.at(t)
}

#[cfg(test)]
mod ease_tests {
    #[test]
    fn the_curve_is_monotone_and_pinned_at_both_ends() {
        assert_eq!(super::ease(0.0), 0.0);
        assert!((super::ease(1.0) - 1.0).abs() < 1e-5);
        let mut last = 0.0;
        for i in 1..=20 {
            let v = super::ease(i as f32 / 20.0);
            assert!(v >= last, "{i}: {v} < {last}");
            last = v;
        }
        // Ease-out: past the midpoint of time, most of the way there.
        assert!(super::ease(0.5) > 0.8, "{}", super::ease(0.5));
    }
}
