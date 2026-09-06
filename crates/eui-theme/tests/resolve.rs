//! Theme resolution: the contrast guarantee, conversions, scales, documents.
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::arithmetic_side_effects)]

use eui_proto::{ColorRef, Dim, StyleRecord};
use eui_theme::*;

const MODES: [ThemeMode; 3] = [ThemeMode::Light, ThemeMode::Dark, ThemeMode::HighContrast];

fn lin(rgba: u32) -> Linear {
    Linear::from_rgba(rgba)
}

fn ratio(r: &Resolved, a: Role, b: Role) -> f64 {
    contrast(lin(r.color(a)), lin(r.color(b)))
}

fn xorshift(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Every pair in spec §4.3, for one resolved palette.
fn assert_contract(r: &Resolved, label: &str) {
    use Role::*;
    let surfaces = [SurfaceBase, SurfaceRaised, SurfaceSunken, SurfaceOverlay];
    for s in surfaces {
        assert!(ratio(r, TextDefault, s) >= 7.0, "{label}: text.default on {} = {:.2}", s.name(), ratio(r, TextDefault, s));
        assert!(ratio(r, TextMuted, s) >= 4.5, "{label}: text.muted on {} = {:.2}", s.name(), ratio(r, TextMuted, s));
    }
    assert!(ratio(r, TextDisabled, SurfaceBase) >= 3.0, "{label}: text.disabled");
    for role in [AccentBase, SuccessBase, WarningBase, DangerBase, InfoBase, FocusRing, BorderStrong] {
        assert!(ratio(r, role, SurfaceBase) >= 3.0, "{label}: {} on surface.base = {:.2}", role.name(), ratio(r, role, SurfaceBase));
    }
    assert!(ratio(r, BorderDefault, SurfaceBase) >= 1.5, "{label}: border.default");
    for (base, on) in [(AccentBase, AccentOn), (SuccessBase, SuccessOn), (WarningBase, WarningOn), (DangerBase, DangerOn), (InfoBase, InfoOn)] {
        assert!(ratio(r, on, base) >= 3.0, "{label}: {} on {} = {:.2}", on.name(), base.name(), ratio(r, on, base));
    }
    for id in 1..=Role::MAX_ID {
        assert_eq!(r.color_by_id(id).unwrap() & 0xFF, 0xFF, "{label}: alpha is opaque");
    }
}

// ------------------------------------------------------------ conversions

#[test]
fn oklab_round_trips_and_matches_reference_values() {
    // Ottosson's published reference: sRGB red is L≈0.628, C≈0.258, h≈29.2°.
    let red = Linear::from_rgba(0xFF0000FF).to_oklch();
    assert!((red.l - 0.628).abs() < 0.002, "L {}", red.l);
    assert!((red.c - 0.258).abs() < 0.002, "C {}", red.c);
    assert!((red.h - 29.23).abs() < 0.2, "h {}", red.h);

    assert!((Linear::from_rgba(0xFFFFFFFF).to_oklch().l - 1.0).abs() < 1e-3);
    assert!(Linear::from_rgba(0x000000FF).to_oklch().l.abs() < 1e-3);

    for rgba in [0xFF0000FF, 0x00FF00FF, 0x0000FFFF, 0x808080FF, 0x123456FF, 0xFEDCBAFF] {
        let back = Linear::from_rgba(rgba).to_oklch().to_rgba();
        for shift in [24, 16, 8] {
            let a = (rgba >> shift) & 0xFF;
            let b = (back >> shift) & 0xFF;
            assert!(a.abs_diff(b) <= 1, "{rgba:#010x} → {back:#010x}");
        }
    }
}

#[test]
fn gamut_clipping_keeps_lightness_and_hue() {
    let wild = Oklch::new(0.6, 0.4, 140.0);
    assert!(!wild.in_gamut());
    let clipped = wild.clip();
    assert!(clipped.in_gamut());
    assert_eq!(clipped.l, wild.l);
    assert_eq!(clipped.h, wild.h);
    assert!(clipped.c < wild.c && clipped.c > 0.0);
    // Already inside: untouched.
    let tame = Oklch::new(0.5, 0.05, 30.0);
    assert_eq!(tame.clip(), tame);
}

#[test]
fn wcag_contrast_has_the_known_extremes() {
    let white = lin(0xFFFFFFFF);
    let black = lin(0x000000FF);
    assert!((contrast(white, black) - 21.0).abs() < 1e-9);
    assert!((contrast(black, white) - 21.0).abs() < 1e-9);
    assert!((contrast(white, white) - 1.0).abs() < 1e-9);
}

// --------------------------------------------------------------- palette

#[test]
fn the_default_theme_meets_every_contrast_pair_in_every_mode() {
    for mode in MODES {
        let r = Theme::default().resolve(Viewer { mode, ..Default::default() });
        assert_contract(&r, &format!("{mode:?}"));
    }
}

#[test]
fn contrast_holds_for_hostile_seeds() {
    // Seeds that a careless author, or a hostile server, might ship: near-white
    // accents, saturated surfaces, hues at the boundaries.
    let mut state = 0x1F2E_3D4C_5B6A_7988u64;
    for i in 0..300 {
        let f = |s: &mut u64| (xorshift(s) % 10_000) as f64 / 10_000.0;
        let theme = Theme {
            accent: Oklch::new(f(&mut state), f(&mut state) * 0.4, f(&mut state) * 360.0),
            surface: Oklch::new(f(&mut state), f(&mut state) * 0.4, f(&mut state) * 360.0),
            ..Default::default()
        };
        for mode in MODES {
            let r = theme.resolve(Viewer { mode, ..Default::default() });
            assert_contract(&r, &format!("seed {i} {mode:?}"));
        }
    }
    // The corners too.
    for accent in [Oklch::new(0.0, 0.0, 0.0), Oklch::new(1.0, 0.0, 0.0), Oklch::new(1.0, 0.4, 359.9)] {
        for mode in MODES {
            let r = Theme { accent, ..Default::default() }.resolve(Viewer { mode, ..Default::default() });
            assert_contract(&r, &format!("corner {accent:?} {mode:?}"));
        }
    }
}

#[test]
fn resolution_is_deterministic() {
    let t = Theme { accent: Oklch::new(0.62, 0.19, 30.0), ..Default::default() };
    let v = Viewer { mode: ThemeMode::Dark, density: Density::Compact, font_scale: 1.3 };
    assert_eq!(t.resolve(v), t.resolve(v));
}

#[test]
fn modes_actually_differ_and_dark_is_dark() {
    let t = Theme::default();
    let light = t.resolve(Viewer::default());
    let dark = t.resolve(Viewer { mode: ThemeMode::Dark, ..Default::default() });
    let hc = t.resolve(Viewer { mode: ThemeMode::HighContrast, ..Default::default() });
    assert!(lin(light.color(Role::SurfaceBase)).luminance() > 0.85);
    assert!(lin(dark.color(Role::SurfaceBase)).luminance() < 0.05);
    assert_eq!(hc.color(Role::SurfaceBase), 0x000000FF);
    assert_eq!(hc.color(Role::TextDefault), 0xFFFFFFFF);
    assert_ne!(light.colors, dark.colors);
}

#[test]
fn on_roles_pick_the_stronger_of_white_and_black() {
    let t = Theme::default();
    let light = t.resolve(Viewer::default());
    // A mid-lightness accent in light mode is dark enough for white text.
    assert_eq!(light.color(Role::AccentOn), 0xFFFFFFFF);
    let dark = t.resolve(Viewer { mode: ThemeMode::Dark, ..Default::default() });
    // A bright accent in dark mode takes black.
    assert_eq!(dark.color(Role::AccentOn), 0x000000FF);
}

#[test]
fn accent_hover_and_active_step_away_from_base() {
    let t = Theme::default();
    let light = t.resolve(Viewer::default());
    let l = |c: u32| lin(c).luminance();
    assert!(l(light.color(Role::AccentHover)) < l(light.color(Role::AccentBase)));
    assert!(l(light.color(Role::AccentActive)) < l(light.color(Role::AccentHover)));
    let dark = t.resolve(Viewer { mode: ThemeMode::Dark, ..Default::default() });
    assert!(l(dark.color(Role::AccentHover)) > l(dark.color(Role::AccentBase)));
    assert!(l(dark.color(Role::AccentActive)) > l(dark.color(Role::AccentHover)));
}

#[test]
fn status_hues_are_fixed_by_the_protocol() {
    // Whatever the accent, danger stays red-orange and success stays green.
    let t = Theme { accent: Oklch::new(0.5, 0.2, 145.0), ..Default::default() };
    let r = t.resolve(Viewer::default());
    let danger = lin(r.color(Role::DangerBase)).to_oklch();
    let success = lin(r.color(Role::SuccessBase)).to_oklch();
    assert!((danger.h - 25.0).abs() < 4.0, "danger hue {}", danger.h);
    assert!((success.h - 145.0).abs() < 4.0, "success hue {}", success.h);
}

// ---------------------------------------------------------------- scales

#[test]
fn density_scales_space_and_controls_but_not_text() {
    let t = Theme::default();
    let cozy = t.resolve(Viewer::default());
    let compact = t.resolve(Viewer { density: Density::Compact, ..Default::default() });
    let roomy = t.resolve(Viewer { density: Density::Comfortable, ..Default::default() });
    assert_eq!(cozy.space, scale::SPACE);
    assert_eq!(compact.space(4), Some(10.0)); // 12 × 0.8 = 9.6 → 10
    assert_eq!(roomy.space(4), Some(15.0)); // 12 × 1.25
    assert_eq!(compact.space(0), Some(0.0));
    assert_eq!(compact.control, [22.0, 29.0, 35.0]);
    assert_eq!(compact.text, cozy.text);
}

#[test]
fn font_scale_scales_text_and_nothing_else() {
    let t = Theme::default();
    let big = t.resolve(Viewer { font_scale: 1.5, ..Default::default() });
    assert_eq!(big.text(2), Some((23.0, 33.0))); // 15 × 1.5 = 22.5 → 23, 22 × 1.5 = 33
    assert_eq!(big.space, scale::SPACE);
    // Nonsense scales fall back to 1.0 rather than producing NaN layouts.
    let nan = t.resolve(Viewer { font_scale: f32::NAN, ..Default::default() });
    assert_eq!(nan.text, scale::TEXT);
    let zero = t.resolve(Viewer { font_scale: 0.0, ..Default::default() });
    assert_eq!(zero.text, scale::TEXT);
}

#[test]
fn radius_scale_derives_from_radius_md() {
    let r = Theme { radius_md: 10.0, ..Default::default() }.resolve(Viewer::default());
    assert_eq!(r.radius, [0.0, 5.0, 10.0, 20.0, scale::RADIUS_FULL]);
    assert_eq!(Theme::default().resolve(Viewer::default()).radius(2), Some(6.0));
    assert_eq!(r.radius(5), None);
}

// ---------------------------------------------------------- style checks

#[test]
fn check_style_rejects_indices_past_a_scale() {
    assert!(check_style(&StyleRecord::default()).is_ok());
    let bad = |f: fn(&mut StyleRecord)| {
        let mut r = StyleRecord::default();
        f(&mut r);
        check_style(&r).unwrap_err()
    };
    assert_eq!(bad(|r| r.gap = 13), ThemeError::ScaleIndex("space", 13));
    assert_eq!(bad(|r| r.padding[2] = 200), ThemeError::ScaleIndex("space", 200));
    assert_eq!(bad(|r| r.width = Dim::Space(13)), ThemeError::ScaleIndex("space", 13));
    assert_eq!(bad(|r| r.radius = 5), ThemeError::ScaleIndex("radius", 5));
    assert_eq!(bad(|r| r.shadow = 4), ThemeError::ScaleIndex("shadow", 4));
    assert_eq!(bad(|r| r.font_size = 8), ThemeError::ScaleIndex("text", 8));
    assert_eq!(bad(|r| r.bg = ColorRef::role(29)), ThemeError::UnknownRole(29));
    // Literals and `none` are not role lookups.
    let r = StyleRecord { bg: ColorRef::literal(5), fg: ColorRef::NONE, ..Default::default() };
    assert!(check_style(&r).is_ok());
}

#[test]
fn role_ids_are_stable_and_bounded() {
    assert_eq!(Role::from_id(1).unwrap(), Role::SurfaceBase);
    assert_eq!(Role::from_id(28).unwrap(), Role::FocusRing);
    assert_eq!(Role::from_id(0).unwrap_err(), ThemeError::UnknownRole(0));
    assert_eq!(Role::from_id(29).unwrap_err(), ThemeError::UnknownRole(29));
    for (i, role) in Role::ALL.iter().enumerate() {
        assert_eq!(usize::from(role.id()), i + 1);
        assert_eq!(Role::from_id(role.id()).unwrap(), *role);
    }
    assert_eq!(Role::AccentOn.name(), "accent.on");
}

// -------------------------------------------------------------- document

#[test]
fn theme_document_round_trips() {
    let t = Theme {
        accent: Oklch::new(0.62, 0.19, 264.0),
        surface: Oklch::new(0.98, 0.01, 150.0),
        radius_md: 8.0,
        density: Density::Comfortable,
        font_sans: Some([7; 32]),
        font_mono: None,
    };
    let bytes = t.encode();
    assert_eq!(&bytes[..4], b"EUIT");
    assert_eq!(Theme::decode(&bytes).unwrap(), t);
    assert_eq!(Theme::decode(&Theme::default().encode()).unwrap(), Theme::default());
}

#[test]
fn theme_document_rejects_garbage() {
    assert!(Theme::decode(b"").is_err());
    assert!(Theme::decode(b"EUIM\x01\x00").is_err(), "a manifest is not a theme");
    assert!(Theme::decode(b"EUIT\x02\x00").is_err(), "unknown version");
    let mut trailing = Theme::default().encode();
    trailing.push(0);
    assert!(Theme::decode(&trailing).is_err());
    // Unknown key 7.
    let mut w = eui_proto::Writer::new();
    w.raw(b"EUIT").u8(1).varint32(1).varint32(7).u8(0x00);
    assert_eq!(Theme::decode(w.as_slice()).unwrap_err(), ThemeError::BadDocument("unknown or repeated key"));
    // Random bytes never panic.
    let mut state = 0xABCDu64;
    for _ in 0..5_000 {
        let len = (xorshift(&mut state) % 64) as usize;
        let buf: Vec<u8> = (0..len).map(|_| (xorshift(&mut state) & 0xFF) as u8).collect();
        let _ = Theme::decode(&buf);
    }
}
