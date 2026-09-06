//! OKLab / OKLCH / sRGB conversions, gamut clipping, and WCAG contrast.
//!
//! Constants and formulas are those of `spec/05-theme.md` §8, transcribed
//! rather than derived, so that a reader can check them line by line.

/// A colour in OKLCH: perceptual lightness, chroma, hue in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oklch {
    /// Lightness, `0..=1`.
    pub l: f64,
    /// Chroma, `0..`; sRGB rarely reaches `0.4`.
    pub c: f64,
    /// Hue in degrees, `0..360`.
    pub h: f64,
}

impl Oklch {
    /// Construct, clamping `l` to `[0, 1]`, `c` to `[0, 0.4]`, and taking `h`
    /// modulo 360.
    pub fn new(l: f64, c: f64, h: f64) -> Self {
        Self { l: l.clamp(0.0, 1.0), c: c.clamp(0.0, 0.4), h: h.rem_euclid(360.0) }
    }

    /// The same colour with a different lightness.
    pub fn with_l(self, l: f64) -> Self {
        Self { l: l.clamp(0.0, 1.0), ..self }
    }

    /// The same colour with a different chroma.
    pub fn with_c(self, c: f64) -> Self {
        Self { c: c.clamp(0.0, 0.4), ..self }
    }

    /// The same colour with a different hue.
    pub fn with_h(self, h: f64) -> Self {
        Self { h: h.rem_euclid(360.0), ..self }
    }

    /// Linear sRGB, not gamut-checked.
    pub fn to_linear(self) -> Linear {
        let hr = self.h.to_radians();
        let a = self.c * hr.cos();
        let b = self.c * hr.sin();
        let l_ = self.l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
        let m_ = self.l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
        let s_ = self.l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
        let l = l_ * l_ * l_;
        let m = m_ * m_ * m_;
        let s = s_ * s_ * s_;
        Linear {
            r: 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
            g: -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
            b: -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
        }
    }

    /// True when the colour lies inside sRGB.
    pub fn in_gamut(self) -> bool {
        self.to_linear().in_gamut()
    }

    /// Bring the colour into sRGB by reducing chroma alone (§4.4): sixteen
    /// bisection steps, keeping the largest chroma that fits. Lightness and
    /// hue are never touched.
    pub fn clip(self) -> Self {
        if self.in_gamut() {
            return self;
        }
        let mut lo = 0.0;
        let mut hi = self.c;
        for _ in 0..16 {
            let mid = (lo + hi) / 2.0;
            if self.with_c(mid).in_gamut() {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        self.with_c(lo)
    }

    /// Clip, then encode to `0xRRGGBBAA`.
    pub fn to_rgba(self) -> u32 {
        self.clip().to_linear().to_rgba()
    }
}

/// Linear-light sRGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Linear {
    /// Red.
    pub r: f64,
    /// Green.
    pub g: f64,
    /// Blue.
    pub b: f64,
}

impl Linear {
    /// From packed 8-bit sRGB.
    pub fn from_rgba(rgba: u32) -> Self {
        let ch = |shift: u32| decode_transfer(f64::from((rgba >> shift) & 0xFF) / 255.0);
        Self { r: ch(24), g: ch(16), b: ch(8) }
    }

    /// True when every component is within `[0, 1]`, with a hair of slack for
    /// the rounding that a round trip through OKLab introduces.
    pub fn in_gamut(self) -> bool {
        const EPS: f64 = 1e-6;
        let ok = |x: f64| (-EPS..=1.0 + EPS).contains(&x);
        ok(self.r) && ok(self.g) && ok(self.b)
    }

    /// WCAG 2.x relative luminance.
    pub fn luminance(self) -> f64 {
        0.2126 * self.r.clamp(0.0, 1.0) + 0.7152 * self.g.clamp(0.0, 1.0) + 0.0722 * self.b.clamp(0.0, 1.0)
    }

    /// To OKLCH.
    pub fn to_oklch(self) -> Oklch {
        let l = 0.412_221_470_8 * self.r + 0.536_332_536_3 * self.g + 0.051_445_992_9 * self.b;
        let m = 0.211_903_498_2 * self.r + 0.680_699_545_1 * self.g + 0.107_396_956_6 * self.b;
        let s = 0.088_302_461_9 * self.r + 0.281_718_837_6 * self.g + 0.629_978_700_5 * self.b;
        let l_ = l.cbrt();
        let m_ = m.cbrt();
        let s_ = s.cbrt();
        let lab_l = 0.210_454_255_3 * l_ + 0.793_617_785_0 * m_ - 0.004_072_046_8 * s_;
        let a = 1.977_998_495_1 * l_ - 2.428_592_205_0 * m_ + 0.450_593_709_9 * s_;
        let b = 0.025_904_037_1 * l_ + 0.782_771_766_2 * m_ - 0.808_675_766_0 * s_;
        Oklch { l: lab_l, c: (a * a + b * b).sqrt(), h: b.atan2(a).to_degrees().rem_euclid(360.0) }
    }

    /// Encode to `0xRRGGBBAA`, round-half-up per channel (§4.5).
    pub fn to_rgba(self) -> u32 {
        let q = |x: f64| (encode_transfer(x.clamp(0.0, 1.0)) * 255.0 + 0.5).floor() as u32;
        (q(self.r) << 24) | (q(self.g) << 16) | (q(self.b) << 8) | 0xFF
    }
}

fn encode_transfer(x: f64) -> f64 {
    if x <= 0.003_130_8 {
        12.92 * x
    } else {
        1.055 * x.powf(1.0 / 2.4) - 0.055
    }
}

fn decode_transfer(x: f64) -> f64 {
    if x <= 0.040_45 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG contrast ratio between two colours, `1..=21`.
pub fn contrast(a: Linear, b: Linear) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// Contrast of two OKLCH colours after gamut clipping and 8-bit quantisation —
/// the value a viewer actually sees.
pub fn contrast_oklch(a: Oklch, b: Oklch) -> f64 {
    contrast(Linear::from_rgba(a.to_rgba()), Linear::from_rgba(b.to_rgba()))
}
