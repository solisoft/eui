//! The fixed scales (`spec/05-theme.md` §2), at cozy density and font scale 1.

/// `space` entries, index 0–12, px.
pub const SPACE: [f32; 13] = [0.0, 2.0, 4.0, 8.0, 12.0, 16.0, 20.0, 24.0, 32.0, 40.0, 48.0, 64.0, 96.0];

/// `text` entries as `(size, line height)`, index 0–7, px.
pub const TEXT: [(f32, f32); 8] = [(11.0, 16.0), (13.0, 18.0), (15.0, 22.0), (17.0, 24.0), (20.0, 28.0), (24.0, 32.0), (30.0, 38.0), (38.0, 46.0)];

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

/// A symmetric ease, in and out — `cubic-bezier(0.45, 0, 0.55, 1)` — for a
/// motion that starts from rest and ends at rest, like a keyboard scroll.
pub fn ease_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// The motion easing curve of 05 §4, `cubic-bezier(0.2, 0, 0, 1)`, as
/// progress `0..=1` → eased `0..=1`. Solved for the parameter by Newton's
/// method: five steps are plenty for a monotone curve.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let (x1, y1, x2, y2) = (0.2f32, 0.0f32, 0.0f32, 1.0f32);
    let bez = |a: f32, b: f32, s: f32| 3.0 * a * (1.0 - s) * (1.0 - s) * s + 3.0 * b * (1.0 - s) * s * s + s * s * s;
    let dbez = |a: f32, b: f32, s: f32| 3.0 * a * (1.0 - s) * (1.0 - 3.0 * s) + 3.0 * b * s * (2.0 - 3.0 * s) + 3.0 * s * s;
    let mut s = t;
    for _ in 0..5 {
        let d = dbez(x1, x2, s);
        if d.abs() < 1e-6 {
            break;
        }
        s = (s - (bez(x1, x2, s) - t) / d).clamp(0.0, 1.0);
    }
    bez(y1, y2, s)
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
