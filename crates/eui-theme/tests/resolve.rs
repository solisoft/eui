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
    // Light hover lightens, and steps back until its label still reads (§4.1).
    if r.mode == ThemeMode::Light {
        assert!(ratio(r, AccentOn, AccentHover) >= 3.0, "{label}: accent.on on accent.hover = {:.2}", ratio(r, AccentOn, AccentHover));
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
    // Light: hover lightens, as a Tailwind button does (indigo-600 to
    // indigo-500), and active presses darker (indigo-700).
    assert!(l(light.color(Role::AccentHover)) > l(light.color(Role::AccentBase)));
    assert!(l(light.color(Role::AccentActive)) < l(light.color(Role::AccentBase)));
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

/// The default theme is Tailwind UI's: gray surfaces, indigo-600. Each
/// role is compared, in OKLab ΔE ×100, with the Tailwind v4 colour it
/// stands in for. Two sit further off than the rest, and both for the same
/// reason — §4.3 moved them: Tailwind's gray-300 border is 1.4:1 on gray-50
/// and its gray-400 is 2.5:1, and the contract wants 1.5 and 3. Run with
/// `--nocapture` for the table, light and dark.
#[test]
fn the_default_palette_is_tailwinds_gray_and_indigo() {
    use Role::*;
    let oklab = |rgba: u32| {
        let c = lin(rgba).to_oklch();
        let h = c.h.to_radians();
        (c.l, c.c * h.cos(), c.c * h.sin())
    };
    let delta_e = |a: u32, b: u32| {
        let (p, q) = (oklab(a), oklab(b));
        100.0 * ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2) + (p.2 - q.2).powi(2)).sqrt()
    };
    let targets: [(Role, &str, u32, f64); 11] = [
        (SurfaceBase, "gray-50", 0xf9fafbff, 1.0),
        (SurfaceRaised, "white", 0xffffffff, 1.0),
        (SurfaceSunken, "gray-100", 0xf3f4f6ff, 1.0),
        (BorderSubtle, "gray-200", 0xe5e7ebff, 1.0),
        (BorderDefault, "gray-300", 0xd1d5dcff, 3.0),
        (BorderStrong, "gray-400", 0x99a1afff, 6.0),
        (TextMuted, "gray-500", 0x6a7282ff, 3.0),
        (TextDefault, "gray-900", 0x101828ff, 3.0),
        (AccentBase, "indigo-600", 0x4f39f6ff, 1.0),
        (AccentHover, "indigo-500", 0x615fffff, 2.0),
        (AccentActive, "indigo-700", 0x432dd7ff, 3.0),
    ];
    let light = Theme::default().resolve(Viewer::default());
    let dark = Theme::default().resolve(Viewer { mode: ThemeMode::Dark, ..Default::default() });
    println!("{:<16} {:>8} {:>8}   {:<11} {:>8} {:>5}", "role", "light", "dark", "tailwind", "", "ΔE");
    for role in Role::ALL.iter().take(28) {
        let target = targets.iter().find(|t| t.0 == *role);
        let (lc, dc) = (light.color(*role) >> 8, dark.color(*role) >> 8);
        match target {
            Some(&(_, name, rgba, _)) => println!("{:<16} #{lc:06x}  #{dc:06x}   {name:<11} #{:06x} {:>5.2}", role.name(), rgba >> 8, delta_e(light.color(*role), rgba)),
            None => println!("{:<16} #{lc:06x}  #{dc:06x}", role.name()),
        }
    }
    for (role, name, rgba, tolerance) in targets {
        let d = delta_e(light.color(role), rgba);
        assert!(d <= tolerance, "{} is #{:06x}, {name} is #{:06x}: ΔE {d:.2} > {tolerance}", role.name(), light.color(role) >> 8, rgba >> 8);
    }
}

// ---------------------------------------------------------------- scales

/// 05 §2: the text scale is Tailwind's, and every shadow is the two layers
/// of the Tailwind class it stands for.
#[test]
fn the_scales_are_tailwinds() {
    assert_eq!(scale::TEXT, [(12.0, 16.0), (14.0, 20.0), (16.0, 24.0), (18.0, 28.0), (20.0, 28.0), (24.0, 32.0), (30.0, 36.0), (36.0, 40.0)]);
    let r = Theme::default().resolve(Viewer::default());
    assert_eq!(r.text, scale::TEXT);
    assert_eq!(r.shadow[0].iter().filter(|l| l.3 > 0.0).count(), 0, "shadow.none paints nothing");
    assert_eq!(r.shadow[1], [(1.0, 2.0, 0.0, 0.05), (0.0, 0.0, 0.0, 0.0)], "shadow-sm");
    assert_eq!(r.shadow[2], [(4.0, 6.0, -1.0, 0.10), (2.0, 4.0, -2.0, 0.10)], "shadow-md");
    assert_eq!(r.shadow[3], [(10.0, 15.0, -3.0, 0.10), (4.0, 6.0, -4.0, 0.10)], "shadow-lg");
    // A negative spread never outgrows the blur it sits in: every layer
    // still reaches past the box, just less far than it is offset.
    for layer in r.shadow.iter().flatten().filter(|l| l.3 > 0.0) {
        assert!(layer.1 + layer.2 > 0.0, "{layer:?}");
    }
}

#[test]
fn the_space_scale_appends_tailwinds_half_steps() {
    // 05 §2: thirteen steps in order, then Tailwind's 1.5 2.5 3.5 20 32
    // appended at 13-17 by version 6. An index cannot move, so the new ones
    // are out of order on purpose.
    assert_eq!(scale::SPACE, [0.0, 2.0, 4.0, 8.0, 12.0, 16.0, 20.0, 24.0, 32.0, 40.0, 48.0, 64.0, 96.0, 6.0, 10.0, 14.0, 80.0, 128.0]);
    assert_eq!(scale::SPACE.len(), eui_proto::space_steps(eui_proto::PROTOCOL_VERSION));
    assert_eq!(eui_proto::space_steps(5), 13);
    // Each fallback is the nearest older step, ties going down — computed
    // here from the pixels, so the table in eui-proto cannot drift from them.
    let old = &scale::SPACE[..13];
    for (n, &fallback) in eui_proto::SPACE_FALLBACK.iter().enumerate() {
        let want = scale::SPACE[13 + n];
        let best = (0..13).min_by(|&a, &b| (old[a] - want).abs().total_cmp(&(old[b] - want).abs()).then(old[a].total_cmp(&old[b]))).unwrap();
        assert_eq!(usize::from(fallback), best, "space {} ({want} px) falls back to {} px", 13 + n, old[best]);
    }
    // Density applies to them like any other step.
    let compact = Theme::default().resolve(Viewer { density: Density::Compact, ..Default::default() });
    assert_eq!(compact.space(13), Some(5.0)); // 6 × 0.8 = 4.8
    assert_eq!(compact.space(17), Some(102.0)); // 128 × 0.8 = 102.4
    assert_eq!(compact.space(18), None);
}

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
    assert_eq!(big.text(1), Some((21.0, 30.0))); // 14 × 1.5 = 21, 20 × 1.5 = 30
    assert_eq!(big.text(0), Some((18.0, 24.0))); // 12 × 1.5, 16 × 1.5
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
    assert_eq!(Theme::default().resolve(Viewer::default()).radius, [0.0, 4.0, 8.0, 16.0, scale::RADIUS_FULL], "rounded / rounded-lg / rounded-2xl");
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
    assert_eq!(bad(|r| r.gap = 18), ThemeError::ScaleIndex("space", 18));
    assert_eq!(bad(|r| r.padding[2] = 200), ThemeError::ScaleIndex("space", 200));
    assert_eq!(bad(|r| r.width = Dim::Space(18)), ThemeError::ScaleIndex("space", 18));
    // Version 6's steps are on the scale (05 §2).
    for ix in 13..18 {
        let r = StyleRecord { gap: ix, padding: [ix; 4], margin: [ix; 4], width: Dim::Space(ix), ..Default::default() };
        assert!(check_style(&r).is_ok(), "space {ix}");
    }
    assert_eq!(bad(|r| r.radius = 5), ThemeError::ScaleIndex("radius", 5));
    assert_eq!(bad(|r| r.shadow = 4), ThemeError::ScaleIndex("shadow", 4));
    assert_eq!(bad(|r| r.font_size = 8), ThemeError::ScaleIndex("text", 8));
    assert_eq!(bad(|r| r.bg = ColorRef::role(34)), ThemeError::UnknownRole(34));
    // Literals and `none` are not role lookups.
    let r = StyleRecord { bg: ColorRef::literal(5), fg: ColorRef::NONE, ..Default::default() };
    assert!(check_style(&r).is_ok());
    // Nor is a gradient `bg` (02 §5.3): it names a table entry, and the
    // role range it was carved from still refuses its last reserved id.
    assert!(check_style(&StyleRecord { bg: ColorRef::gradient(1), ..Default::default() }).is_ok());
    assert_eq!(bad(|r| r.bg = ColorRef::role(0x3FFF)), ThemeError::UnknownRole(0x3FFF));
}

#[test]
fn role_ids_are_stable_and_bounded() {
    assert_eq!(Role::from_id(1).unwrap(), Role::SurfaceBase);
    assert_eq!(Role::from_id(28).unwrap(), Role::FocusRing);
    assert_eq!(Role::from_id(33).unwrap(), Role::Series5);
    assert_eq!(Role::from_id(0).unwrap_err(), ThemeError::UnknownRole(0));
    assert_eq!(Role::from_id(34).unwrap_err(), ThemeError::UnknownRole(34));
    for (i, role) in Role::ALL.iter().enumerate() {
        assert_eq!(usize::from(role.id()), i + 1);
        assert_eq!(Role::from_id(role.id()).unwrap(), *role);
    }
    assert_eq!(Role::AccentOn.name(), "accent.on");
}

// -------------------------------------------------------------- document

#[test]
fn theme_document_round_trips() {
    let t = Theme { accent: Oklch::new(0.62, 0.19, 264.0), surface: Oklch::new(0.98, 0.01, 150.0), radius_md: 8.0, density: Density::Comfortable, font_sans: Some([7; 32]), font_mono: None };
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

// ------------------------------------------------------------------ curves

/// Every easing curve is a path from (0,0) to (1,1) that never goes
/// backwards. A solver that stalled, overshot or returned NaN fails here
/// whatever curve it was handed.
#[test]
fn every_curve_runs_from_nothing_to_everything_without_turning_back() {
    use eui_theme::Curve;
    for (name, c) in [("standard", Curve::STANDARD), ("decelerate", Curve::DECELERATE), ("accelerate", Curve::ACCELERATE), ("smooth", Curve::SMOOTH), ("linear", Curve::LINEAR)] {
        assert_eq!(c.at(0.0), 0.0, "{name} starts at rest");
        assert_eq!(c.at(1.0), 1.0, "{name} arrives exactly");
        assert_eq!(c.at(-1.0), 0.0, "{name} clamps below");
        assert_eq!(c.at(2.0), 1.0, "{name} clamps above");
        let mut last = 0.0;
        for i in 0..=100 {
            let y = c.at(i as f32 / 100.0);
            assert!(y.is_finite(), "{name} at {i} is {y}");
            assert!(y >= last - 1e-4, "{name} went backwards at {i}: {last} then {y}");
            assert!((-1e-4..=1.0 + 1e-4).contains(&y), "{name} left the unit square at {i}: {y}");
            last = y;
        }
    }
}

/// `linear` is the identity, which is the one curve whose every value can
/// be checked against arithmetic rather than against itself.
#[test]
fn the_linear_curve_is_the_identity() {
    for i in 0..=20 {
        let t = i as f32 / 20.0;
        assert!((eui_theme::Curve::LINEAR.at(t) - t).abs() < 1e-3, "at {t}");
    }
}

/// What separates the two is which side of the diagonal they sit on:
/// a decelerating curve is always ahead of its own clock, an accelerating
/// one always behind it. (They are not reflections of each other — these
/// are the familiar `(0, 0, 0.2, 1)` and `(0.4, 0, 1, 1)`, and the exact
/// mirror of the first would be `(0.8, 0, 1, 1)`.)
#[test]
fn decelerate_leads_its_clock_and_accelerate_trails_it() {
    use eui_theme::Curve;
    for i in 1..20 {
        let t = i as f32 / 20.0;
        assert!(Curve::DECELERATE.at(t) > t, "decelerate at {t} is {}", Curve::DECELERATE.at(t));
        assert!(Curve::ACCELERATE.at(t) < t, "accelerate at {t} is {}", Curve::ACCELERATE.at(t));
    }
    // The theme's own curve leads too — it leaves at once and arrives gently.
    assert!(Curve::STANDARD.at(0.5) > 0.5);
    // A symmetric one crosses the diagonal exactly at the middle.
    assert!((Curve::SMOOTH.at(0.5) - 0.5).abs() < 1e-3);
}

/// `scale::ease` is the theme's curve of 05 §2 and nothing else, so the
/// control points in the document and the ones in the code cannot drift.
#[test]
fn the_themes_easing_is_the_standard_curve() {
    use eui_theme::scale::{ease, EASING};
    assert_eq!(EASING, [eui_theme::Curve::STANDARD.x1, eui_theme::Curve::STANDARD.y1, eui_theme::Curve::STANDARD.x2, eui_theme::Curve::STANDARD.y2]);
    for i in 0..=10 {
        let t = i as f32 / 10.0;
        assert!((ease(t) - eui_theme::Curve::STANDARD.at(t)).abs() < 1e-6, "at {t}");
    }
}

/// The five categorical series colours are validated, not chosen by eye: each mode's
/// ramp clears the adjacent-pair CVD separation floor (ΔE ≥ 8 in OKLab×100 under
/// protan/deutan/tritan) and the normal-vision floor (ΔE ≥ 15) against the mode's own
/// surface. Re-run the palette validator before touching the hues, chroma, or
/// lightnesses in `theme.rs` — these hexes are the record of what passed.
#[test]
fn series_roles_keep_their_validated_colors() {
    let hex = |mode| {
        let r = Theme::default().resolve(Viewer { mode, ..Default::default() });
        Role::ALL[28..33].iter().map(|role| format!("#{:06x}", r.color(*role) >> 8)).collect::<Vec<_>>()
    };
    assert_eq!(hex(ThemeMode::Light), ["#3f60a7", "#8a5600", "#4bc39f", "#82417d", "#00b5b5"]);
    assert_eq!(hex(ThemeMode::Dark), ["#4566ae", "#915b00", "#29a987", "#884783", "#00a7a7"]);
    assert_eq!(hex(ThemeMode::HighContrast), ["#7ea3f0", "#efb062", "#7df1cb", "#c882c2", "#55e3e3"]);
}
