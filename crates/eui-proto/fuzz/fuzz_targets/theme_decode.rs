//! Theme documents from arbitrary bytes; a decoded theme must resolve in every
//! mode with every contrast pair met — the guarantee is by construction and
//! must hold for any seeds, hostile ones included.
#![no_main]
use libfuzzer_sys::fuzz_target;
use eui_theme::{contrast, Linear, Role, Theme, ThemeMode, Viewer};

fuzz_target!(|data: &[u8]| {
    let Ok(theme) = Theme::decode(data) else { return };
    for mode in [ThemeMode::Light, ThemeMode::Dark, ThemeMode::HighContrast] {
        let r = theme.resolve(Viewer { mode, ..Default::default() });
        let c = |a: Role, b: Role| contrast(Linear::from_rgba(r.color(a)), Linear::from_rgba(r.color(b)));
        for s in [Role::SurfaceBase, Role::SurfaceRaised, Role::SurfaceSunken, Role::SurfaceOverlay] {
            assert!(c(Role::TextDefault, s) >= 7.0);
            assert!(c(Role::TextMuted, s) >= 4.5);
        }
        assert!(c(Role::AccentOn, Role::AccentBase) >= 3.0);
    }
});
