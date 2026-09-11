//! Theme documents and their resolution (`spec/05-theme.md` §3–§5).

use eui_proto::{Density, Reader, StyleRecord, ThemeMode, Value, Writer};

use crate::color::{contrast_oklch, Oklch};
use crate::error::ThemeError;
use crate::role::Role;
use crate::scale;

/// The fixed status hues (§3).
const HUE_SUCCESS: f64 = 145.0;
const HUE_WARNING: f64 = 80.0;
const HUE_DANGER: f64 = 25.0;
const HUE_INFO: f64 = 250.0;

const MAGIC: &[u8; 4] = b"EUIT";
const VERSION: u8 = 1;

/// A theme: four seeds and a few preferences. Everything else is derived.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Accent seed.
    pub accent: Oklch,
    /// Surface seed; only chroma and hue are used.
    pub surface: Oklch,
    /// The `md` radius in px; `sm` and `lg` derive from it.
    pub radius_md: f32,
    /// The author's density preference. The viewer's own setting wins.
    pub density: Density,
    /// Sans face asset, or the client's built-in face.
    pub font_sans: Option<[u8; 32]>,
    /// Mono face asset, or the client's built-in face.
    pub font_mono: Option<[u8; 32]>,
}

impl Default for Theme {
    fn default() -> Self {
        Self { accent: Oklch::new(0.55, 0.18, 264.0), surface: Oklch::new(0.98, 0.006, 250.0), radius_md: 6.0, density: Density::Cozy, font_sans: None, font_mono: None }
    }
}

/// What the viewer chose. Applied over any theme, always.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewer {
    /// Palette.
    pub mode: ThemeMode,
    /// Spacing multiplier.
    pub density: Density,
    /// Text multiplier, `1.0` is unscaled.
    pub font_scale: f32,
}

impl Default for Viewer {
    fn default() -> Self {
        Self { mode: ThemeMode::Light, density: Density::Cozy, font_scale: 1.0 }
    }
}

/// Every role and scale resolved to a concrete value. Layout and paint read
/// this by index and nothing else.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// `0xRRGGBBAA` by role id; index 0 is unused and black.
    pub colors: [u32; 34],
    /// Space scale after density, whole px.
    pub space: [f32; 13],
    /// Radius scale, px.
    pub radius: [f32; scale::RADIUS_LEN],
    /// `(size, line height)` after font scale, whole px.
    pub text: [(f32, f32); 8],
    /// Shadow scale, unchanged.
    pub shadow: [(f32, f32, f32); 4],
    /// Motion durations, ms.
    pub motion: [u16; 3],
    /// Control heights after density, whole px.
    pub control: [f32; 3],
    /// The mode these colours are for.
    pub mode: ThemeMode,
}

impl Resolved {
    /// The colour for a role.
    pub fn color(&self, role: Role) -> u32 {
        self.colors.get(usize::from(role.id())).copied().unwrap_or(0x0000_00FF)
    }

    /// The colour for a role id, or `None` for an unknown id.
    pub fn color_by_id(&self, id: u16) -> Option<u32> {
        Role::from_id(id).ok().map(|r| self.color(r))
    }

    /// A `space` entry, or `None` past the end.
    pub fn space(&self, ix: u8) -> Option<f32> {
        self.space.get(usize::from(ix)).copied()
    }

    /// A `radius` entry, or `None` past the end.
    pub fn radius(&self, ix: u8) -> Option<f32> {
        self.radius.get(usize::from(ix)).copied()
    }

    /// A `text` entry, or `None` past the end.
    pub fn text(&self, ix: u8) -> Option<(f32, f32)> {
        self.text.get(usize::from(ix)).copied()
    }
}

/// Reject a style record whose scale indices run past their scales. Called
/// on `DefStyle`, so a bad index is a rejected batch rather than a surprise
/// at paint time.
pub fn check_style(r: &StyleRecord) -> Result<(), ThemeError> {
    let space = |ix: u8| {
        if usize::from(ix) < scale::SPACE.len() {
            Ok(())
        } else {
            Err(ThemeError::ScaleIndex("space", ix))
        }
    };
    space(r.gap)?;
    for ix in r.padding.iter().chain(r.margin.iter()) {
        space(*ix)?;
    }
    for d in [r.basis, r.width, r.height, r.min_width, r.min_height, r.max_width, r.max_height] {
        if let eui_proto::Dim::Space(ix) = d {
            space(ix)?;
        }
    }
    if usize::from(r.radius) >= scale::RADIUS_LEN {
        return Err(ThemeError::ScaleIndex("radius", r.radius));
    }
    if usize::from(r.shadow) >= scale::SHADOW.len() {
        return Err(ThemeError::ScaleIndex("shadow", r.shadow));
    }
    if usize::from(r.font_size) >= scale::TEXT.len() {
        return Err(ThemeError::ScaleIndex("text", r.font_size));
    }
    for c in [r.bg, r.fg, r.border_color] {
        if !c.is_literal() && !c.is_none() {
            Role::from_id(c.index())?;
        }
    }
    Ok(())
}

impl Theme {
    /// Resolve every role and scale for a viewer.
    pub fn resolve(&self, viewer: Viewer) -> Resolved {
        let colors = self.palette(viewer.mode);

        let density = match viewer.density {
            Density::Compact => 0.8,
            Density::Cozy => 1.0,
            Density::Comfortable => 1.25,
        };
        let fs = if viewer.font_scale.is_finite() && viewer.font_scale > 0.0 { viewer.font_scale } else { 1.0 };

        let mut space = scale::SPACE;
        for v in &mut space {
            *v = (*v * density).round();
        }
        let mut control = scale::CONTROL;
        for v in &mut control {
            *v = (*v * density).round();
        }
        let mut text = scale::TEXT;
        for (size, line) in &mut text {
            *size = (*size * fs).round();
            *line = (*line * fs).round();
        }
        let md = if self.radius_md.is_finite() { self.radius_md.max(0.0) } else { 6.0 };
        let radius = [0.0, (md / 2.0).round(), md.round(), (md * 2.0).round(), scale::RADIUS_FULL];

        Resolved { colors, space, radius, text, shadow: scale::SHADOW, motion: scale::MOTION, control, mode: viewer.mode }
    }

    /// §4: targets, contrast enforcement, `on` roles, packing.
    fn palette(&self, mode: ThemeMode) -> [u32; 34] {
        use Role::*;
        let col = |light: f64, dark: f64, hc: f64| match mode {
            ThemeMode::Light => light,
            ThemeMode::Dark => dark,
            ThemeMode::HighContrast => hc,
        };
        let (s_c, s_h) = (self.surface.c, self.surface.h);
        let (a_l, a_c, a_h) = (self.accent.l, self.accent.c, self.accent.h);
        let surf = |l: f64| Oklch::new(l, s_c.min(0.02), s_h);
        let txt = |l: f64| Oklch::new(l, s_c.min(0.015), s_h);

        let mut p: [Oklch; 34] = [Oklch::new(0.0, 0.0, 0.0); 34];
        let mut set = |r: Role, c: Oklch| {
            if let Some(slot) = p.get_mut(usize::from(r.id())) {
                *slot = c;
            }
        };

        set(SurfaceBase, surf(col(0.985, 0.19, 0.0)));
        set(SurfaceRaised, surf(col(1.0, 0.24, 0.05)));
        set(SurfaceSunken, surf(col(0.955, 0.15, 0.0)));
        set(SurfaceOverlay, surf(col(1.0, 0.27, 0.08)));
        set(TextDefault, txt(col(0.18, 0.93, 1.0)));
        set(TextMuted, txt(col(0.45, 0.72, 0.90)));
        set(TextInverted, txt(col(0.98, 0.15, 0.0)));
        set(TextDisabled, txt(col(0.65, 0.50, 0.70)));

        let accent_l = match mode {
            ThemeMode::Light => a_l.clamp(0.40, 0.60),
            ThemeMode::Dark => a_l.clamp(0.60, 0.80),
            ThemeMode::HighContrast => 0.80,
        };
        set(AccentBase, Oklch::new(accent_l, a_c, a_h));

        let status = |hue: f64, warning: bool| {
            let base = Oklch::new(col(0.52, 0.72, 0.80), if warning { 0.12 } else { 0.14 }, hue);
            let subtle = Oklch::new(col(0.95, 0.25, 0.15), col(0.04, 0.06, 0.06), hue);
            (base, subtle)
        };
        let (b, s) = status(HUE_SUCCESS, false);
        set(SuccessBase, b);
        set(SuccessSubtle, s);
        let (b, s) = status(HUE_WARNING, true);
        set(WarningBase, b);
        set(WarningSubtle, s);
        let (b, s) = status(HUE_DANGER, false);
        set(DangerBase, b);
        set(DangerSubtle, s);
        let (b, s) = status(HUE_INFO, false);
        set(InfoBase, b);
        set(InfoSubtle, s);

        set(BorderSubtle, surf(col(0.92, 0.26, 0.50)));
        set(BorderDefault, surf(col(0.86, 0.32, 0.70)));
        set(BorderStrong, surf(col(0.70, 0.45, 0.90)));
        set(FocusRing, Oklch::new(col(0.55, 0.75, 0.85), a_c.max(0.18), a_h));

        // §1.1 — the categorical series of a chart.
        //
        // These are *not* the status roles. `success` and `warning` were
        // pressed into that job and they are reserved: a green series means
        // "good" to a reader who has learned the rest of the interface, and
        // a fifth series wrapped round to the accent again. Worse, they do
        // not separate — measured against the six checks, `accent` and
        // `info` are both blue at ΔE 6.4 to a full-colour reader, and
        // `success` and `warning` collapse to ΔE 3.6 under deuteranopia.
        //
        // This ramp was searched rather than chosen, and the search was the
        // checks themselves: five hues, the first pinned to the theme's own
        // accent so a one-series chart still looks like the product, with
        // the lightness of each picked per mode. Worst adjacent pair under
        // the worst simulated deficiency, against a target of 8: ΔE 21.5
        // light, 15.7 dark, 12.4 high-contrast; worst pair to normal vision,
        // against a floor of 15: 22.7, 22.6, 19.9. Series 3 and 5 in light
        // and series 4 in dark fall below 3:1 on their surface, which is the
        // documented relax — a chart carries its legend and its value chips,
        // so identity is never colour alone. High contrast takes them all
        // above 3:1 instead, which is why its steps sit outside the band the
        // other two modes keep to: there, contrast outranks uniformity.
        //
        // Five, and no cycling. A sixth series is not a sixth hue — a
        // generated one is indistinguishable under CVD from one already
        // here — so a tail folds into a single "other", or the chart
        // becomes small multiples.
        let series = |hue: f64, l_light: f64, l_dark: f64, l_hc: f64| Oklch::new(col(l_light, l_dark, l_hc), 0.12, hue);
        set(Series1, series(264.0, 0.50, 0.52, 0.72));
        set(Series2, series(70.0, 0.50, 0.52, 0.80));
        set(Series3, series(170.0, 0.74, 0.66, 0.88));
        set(Series4, series(330.0, 0.48, 0.50, 0.70));
        set(Series5, series(195.0, 0.70, 0.66, 0.84));

        // §4.3 — contrast by construction.
        let surfaces = [SurfaceBase, SurfaceRaised, SurfaceSunken, SurfaceOverlay];
        enforce(&mut p, TextDefault, &surfaces, 7.0);
        enforce(&mut p, TextMuted, &surfaces, 4.5);
        enforce(&mut p, TextDisabled, &[SurfaceBase], 3.0);
        for r in [AccentBase, SuccessBase, WarningBase, DangerBase, InfoBase, FocusRing, BorderStrong] {
            enforce(&mut p, r, &[SurfaceBase], 3.0);
        }
        enforce(&mut p, BorderDefault, &[SurfaceBase], 1.5);

        // Hover and active follow the adjusted base.
        let base = get(&p, AccentBase);
        let dir = if mode == ThemeMode::Light { -1.0 } else { 1.0 };
        put(&mut p, AccentHover, base.with_l(base.l + dir * 0.06));
        put(&mut p, AccentActive, base.with_l(base.l + dir * 0.12));

        // §4.2 — the `on` roles.
        for (base, on) in [(AccentBase, AccentOn), (SuccessBase, SuccessOn), (WarningBase, WarningOn), (DangerBase, DangerOn), (InfoBase, InfoOn)] {
            let chosen = on_color(get(&p, base));
            put(&mut p, on, chosen);
        }

        let mut out = [0x0000_00FFu32; 34];
        for (slot, c) in out.iter_mut().zip(p.iter()).skip(1) {
            *slot = c.to_rgba();
        }
        out
    }

    // ----------------------------------------------------------- document

    /// Encode as an `EUIT` record.
    pub fn encode(&self) -> Vec<u8> {
        let seed = |c: Oklch| Value::List(vec![Value::Float(c.l), Value::Float(c.c), Value::Float(c.h)]);
        let mut fields: Vec<(u32, Value)> = vec![(1, seed(self.accent)), (2, seed(self.surface)), (3, Value::Float(f64::from(self.radius_md))), (4, Value::Int(i64::from(self.density as u8)))];
        if let Some(h) = self.font_sans {
            fields.push((5, Value::Asset(h)));
        }
        if let Some(h) = self.font_mono {
            fields.push((6, Value::Asset(h)));
        }
        let mut w = Writer::new();
        w.raw(MAGIC).u8(VERSION).varint32(fields.len() as u32);
        for (key, value) in &fields {
            w.varint32(*key);
            value.encode(&mut w);
        }
        w.into_vec()
    }

    /// Decode an `EUIT` record. Unknown keys are an error: a theme is
    /// content-addressed, so there is no version skew to be lenient about.
    pub fn decode(bytes: &[u8]) -> Result<Self, ThemeError> {
        let bad = ThemeError::BadDocument;
        let mut r = Reader::new(bytes);
        if r.array::<4>().map_err(|_| bad("truncated"))? != *MAGIC {
            return Err(bad("not a theme"));
        }
        if r.u8().map_err(|_| bad("truncated"))? != VERSION {
            return Err(bad("unsupported version"));
        }
        let count = r.varint32_max(16, "fields").map_err(|_| bad("field count"))?;
        let mut theme = Theme::default();
        let mut seen = 0u32;
        for _ in 0..count {
            let key = r.varint32().map_err(|_| bad("truncated"))?;
            let value = Value::decode(&mut r).map_err(|_| bad("value"))?;
            let bit = 1u32.checked_shl(key).unwrap_or(0);
            if key == 0 || key > 6 || seen & bit != 0 {
                return Err(bad("unknown or repeated key"));
            }
            seen |= bit;
            match (key, value) {
                (1, v) => theme.accent = seed_of(&v)?,
                (2, v) => theme.surface = seed_of(&v)?,
                (3, Value::Float(f)) if f.is_finite() && f >= 0.0 => theme.radius_md = f as f32,
                (4, Value::Int(n)) => {
                    theme.density = Density::from_u8(u8::try_from(n).map_err(|_| bad("density"))?).map_err(|_| bad("density"))?;
                }
                (5, Value::Asset(h)) => theme.font_sans = Some(h),
                (6, Value::Asset(h)) => theme.font_mono = Some(h),
                _ => return Err(bad("wrong value type")),
            }
        }
        r.finish().map_err(|_| bad("trailing bytes"))?;
        Ok(theme)
    }
}

fn seed_of(v: &Value) -> Result<Oklch, ThemeError> {
    match v {
        Value::List(items) => match items.as_slice() {
            [Value::Float(l), Value::Float(c), Value::Float(h)] => Ok(Oklch::new(*l, *c, *h)),
            _ => Err(ThemeError::BadDocument("seed shape")),
        },
        _ => Err(ThemeError::BadDocument("seed type")),
    }
}

fn get(p: &[Oklch; 34], r: Role) -> Oklch {
    p.get(usize::from(r.id())).copied().unwrap_or(Oklch::new(0.0, 0.0, 0.0))
}

fn put(p: &mut [Oklch; 34], r: Role, c: Oklch) {
    if let Some(slot) = p.get_mut(usize::from(r.id())) {
        *slot = c;
    }
}

/// §4.3: nudge `fg` away from the backgrounds until every pair meets `min`.
fn enforce(p: &mut [Oklch; 34], fg: Role, bgs: &[Role], min: f64) {
    let bg_colors: Vec<Oklch> = bgs.iter().map(|b| get(p, *b)).collect();
    let mean_l = bg_colors.iter().map(|c| c.l).sum::<f64>() / bg_colors.len().max(1) as f64;
    let dir = if mean_l < 0.5 { 1.0 } else { -1.0 };
    let mut c = get(p, fg);
    for _ in 0..60 {
        if bg_colors.iter().all(|bg| contrast_oklch(c, *bg) >= min) {
            break;
        }
        c = c.with_l(c.l + dir * 0.01);
    }
    put(p, fg, c);
}

/// §4.2: white or black, whichever contrasts more with `base`.
fn on_color(base: Oklch) -> Oklch {
    let white = Oklch::new(1.0, 0.0, 0.0);
    let black = Oklch::new(0.0, 0.0, 0.0);
    if contrast_oklch(white, base) >= contrast_oklch(black, base) {
        white
    } else {
        black
    }
}

impl Resolved {
    /// Replace roles with the viewer's own colours — a desktop palette the
    /// client follows (05 §5). Unknown roles are ignored; a role given twice
    /// takes the last value.
    pub fn apply_overrides(&mut self, overrides: &[(Role, u32)]) {
        for (role, rgba) in overrides {
            if let Some(slot) = self.colors.get_mut(usize::from(role.id())) {
                *slot = *rgba;
            }
        }
    }
}
