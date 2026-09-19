//! Spec 05 §5: the viewer's desktop palette, followed.
//!
//! The theme is resolved on the client from roles, so a desktop that
//! publishes its colours can be followed exactly: the window reads them,
//! maps them onto the roles, and hands them to the driver — wherever it
//! runs — as overrides on top of the application's theme. The server never
//! learns them; it sees the palette's light/dark as the viewport's mode,
//! as it would any mode change.
//!
//! Omarchy (Hyprland on Arch) is the first desktop followed: its current
//! theme is a directory of files under `~/.local/state/omarchy/current/theme`,
//! among them `colors.toml`, a flat list of `name = "#rrggbb"` lines with a
//! `mode`. That file is parsed here by hand — a dozen keys, no TOML crate
//! for it — and the directory is watched, so switching the theme with
//! Omarchy's own menu recolours every open EUI window at once.
//!
//! `EUI_DESKTOP_THEME=0` leaves the application's own theme alone.

use std::path::{Path, PathBuf};

use eui_proto::ThemeMode;
use eui_theme::{contrast, Linear, Role};

/// A desktop palette mapped onto the roles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopTheme {
    /// The palette's own light or dark.
    pub mode: ThemeMode,
    /// Colours by role, `0xRRGGBBAA`.
    pub colors: Vec<(Role, u32)>,
    /// Where it came from, for the log.
    pub source: String,
}

/// Whether following is turned off by the environment.
pub fn disabled() -> bool {
    std::env::var("EUI_DESKTOP_THEME").is_ok_and(|v| v == "0")
}

/// Omarchy's current-theme directory, if this looks like an Omarchy desktop.
pub fn omarchy_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let dir = Path::new(&home).join(".local/state/omarchy/current");
    dir.join("theme.name").is_file().then_some(dir)
}

/// Which of the two palettes the machine is in, asked of the desktop.
///
/// Separate from [`current`] because it is a smaller question with more
/// answers: a desktop that publishes no colours at all still knows whether
/// it is light or dark, and winit will not say on this platform — its
/// Wayland `theme()` is the decoration theme *this* process asked for, and
/// its X11 one is flatly `None`. Without this a GNOME or KDE desktop in
/// the dark got a light client and no way to say otherwise.
///
/// Asked before any window is built, which is the point: a driver made in
/// the wrong palette sends a `Hello` that says so.
#[cfg(target_os = "linux")]
pub fn os_mode() -> Option<ThemeMode> {
    if disabled() {
        return None;
    }
    // The palette in force answers first and answers exactly. The portal
    // is the fallback, and on Omarchy it is usually the *desktop's* idea
    // of light and dark rather than the theme's.
    current().map(|t| t.mode).or_else(portal_mode)
}

/// Every other platform: winit answers for the window, and there is no
/// desktop file to read behind it. macOS and Windows report a real theme;
/// a page reports `prefers-color-scheme`; Android and iOS report nothing
/// and come up light until the application asks for otherwise.
#[cfg(not(target_os = "linux"))]
pub fn os_mode() -> Option<ThemeMode> {
    None
}

/// `color-scheme` from the XDG desktop portal — the setting a browser
/// answers `prefers-color-scheme` from, and the one cross-desktop place a
/// light/dark preference lives on Linux.
///
/// `1` is a preference for dark and `2` for light; `0` is "no preference",
/// which is not the same as light and is answered here as no answer, so
/// the application's own theme decides as it did before.
///
/// `ReadOne` is the current call and answers a variant. Portals older than
/// 0.19 have only `Read`, whose answer is a variant holding another — so
/// the value is unwrapped through however many layers it arrives in and
/// one code path serves both.
#[cfg(target_os = "linux")]
fn portal_mode() -> Option<ThemeMode> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let call = |method: &str| {
        conn.call_method(Some("org.freedesktop.portal.Desktop"), "/org/freedesktop/portal/desktop", Some("org.freedesktop.portal.Settings"), method, &("org.freedesktop.appearance", "color-scheme"))
            .ok()
    };
    let reply = call("ReadOne").or_else(|| call("Read"))?;
    let body = reply.body();
    let value = body.deserialize::<zbus::zvariant::Value<'_>>().ok()?;
    scheme(&value)
}

/// A `color-scheme` value, through however many variants it is wrapped in.
#[cfg(target_os = "linux")]
fn scheme(v: &zbus::zvariant::Value<'_>) -> Option<ThemeMode> {
    match v {
        zbus::zvariant::Value::U32(1) => Some(ThemeMode::Dark),
        zbus::zvariant::Value::U32(2) => Some(ThemeMode::Light),
        zbus::zvariant::Value::Value(inner) => scheme(inner),
        _ => None,
    }
}

/// The desktop's palette, if there is one to follow.
pub fn current() -> Option<DesktopTheme> {
    if disabled() {
        return None;
    }
    let dir = omarchy_dir()?;
    let colors = std::fs::read_to_string(dir.join("theme/colors.toml")).ok()?;
    let name = std::fs::read_to_string(dir.join("theme.name")).map(|n| n.trim().to_owned()).unwrap_or_else(|_| "omarchy".into());
    parse_omarchy(&colors).map(|(mode, colors)| DesktopTheme { mode, colors, source: format!("omarchy theme {name}") })
}

/// One `key = "value"` line of Omarchy's `colors.toml`.
fn line(l: &str) -> Option<(&str, &str)> {
    let l = l.trim();
    if l.starts_with('#') {
        return None;
    }
    let (k, v) = l.split_once('=')?;
    Some((k.trim(), v.trim().trim_matches('"')))
}

/// `#rrggbb` or `#rrggbbaa` as `0xRRGGBBAA`.
fn hex(v: &str) -> Option<u32> {
    let h = v.strip_prefix('#')?;
    match h.len() {
        6 => u32::from_str_radix(h, 16).ok().map(|c| (c << 8) | 0xff),
        8 => u32::from_str_radix(h, 16).ok(),
        _ => None,
    }
}

/// The text colour that reads on `bg`, out of the palette's two.
fn on(bg: u32, fg: u32, alt: u32) -> u32 {
    let (b, f, a) = (Linear::from_rgba(bg), Linear::from_rgba(fg), Linear::from_rgba(alt));
    if contrast(b, f) >= contrast(b, a) {
        fg
    } else {
        alt
    }
}

/// `rgba` with its lightness moved by `dl` in OKLCH, clipped to gamut.
fn shift(rgba: u32, dl: f64) -> u32 {
    let c = Linear::from_rgba(rgba).to_oklch();
    let alpha = rgba & 0xff;
    (c.with_l((c.l + dl).clamp(0.0, 1.0)).clip().to_rgba() & !0xff) | alpha
}

/// `rgba` at `alpha` (0–255).
fn at(rgba: u32, alpha: u32) -> u32 {
    (rgba & !0xff) | (alpha & 0xff)
}

/// A secondary text colour that still reads: the foreground pulled toward
/// the background in linear light, no further than a 4.5:1 contrast on
/// the least contrasting of `surfaces` allows. A desktop's own `muted` is
/// made for inactive chrome, not for a name or a caption, and on most
/// palettes it falls well under that.
fn muted_text(fg: u32, bg: u32, surfaces: &[u32]) -> u32 {
    let (f, b) = (Linear::from_rgba(fg), Linear::from_rgba(bg));
    let worst = |c: Linear| surfaces.iter().map(|s| contrast(c, Linear::from_rgba(*s))).fold(f64::INFINITY, f64::min);
    for k in [0.55, 0.62, 0.7, 0.78, 0.86, 0.94] {
        let c = Linear { r: b.r + (f.r - b.r) * k, g: b.g + (f.g - b.g) * k, b: b.b + (f.b - b.b) * k };
        if worst(c) >= 4.5 {
            return (c.to_rgba() & !0xff) | 0xff;
        }
    }
    fg
}

/// Map Omarchy's `colors.toml` onto the roles. `None` unless it has at
/// least a background, a foreground and an accent.
pub fn parse_omarchy(text: &str) -> Option<(ThemeMode, Vec<(Role, u32)>)> {
    let get = |key: &str| text.lines().filter_map(line).find(|(k, _)| *k == key).map(|(_, v)| v.to_owned());
    let mode = match get("mode").as_deref() {
        Some("light") => ThemeMode::Light,
        _ => ThemeMode::Dark,
    };
    let color = |key: &str| get(key).and_then(|v| hex(&v));
    let bg = color("background")?;
    let fg = color("foreground")?;
    let accent = color("accent")?;
    let dark = |l: f64| shift(bg, if mode == ThemeMode::Dark { -l } else { l });
    let raised = color("lighter_background").unwrap_or_else(|| shift(bg, if mode == ThemeMode::Dark { 0.05 } else { 0.02 }));
    let sunken = color("dark_background").unwrap_or_else(|| dark(0.03));
    let deeper = color("darker_background").unwrap_or_else(|| dark(0.06));
    // The desktop's `muted` is chrome, not text: borders and disabled
    // things take it, secondary text gets a readable derivative.
    let muted = color("muted").or_else(|| color("dark_foreground")).unwrap_or_else(|| shift(fg, if mode == ThemeMode::Dark { -0.25 } else { 0.25 }));
    let muted_fg = muted_text(fg, bg, &[bg, raised, sunken]);
    let light_fg = color("light_foreground").unwrap_or(fg);
    let (green, yellow, red, blue) = (color("green"), color("yellow"), color("red"), color("blue"));
    let hover = shift(accent, if mode == ThemeMode::Dark { 0.06 } else { -0.06 });
    let active = shift(accent, if mode == ThemeMode::Dark { 0.12 } else { -0.12 });
    let mut colors = vec![
        (Role::SurfaceBase, bg),
        (Role::SurfaceRaised, raised),
        (Role::SurfaceSunken, sunken),
        (Role::SurfaceOverlay, raised),
        (Role::TextDefault, fg),
        (Role::TextMuted, muted_fg),
        (Role::TextInverted, bg),
        (Role::TextDisabled, muted),
        (Role::AccentBase, accent),
        (Role::AccentHover, hover),
        (Role::AccentActive, active),
        (Role::AccentOn, on(accent, fg, bg)),
        (Role::BorderSubtle, deeper),
        (Role::BorderDefault, at(muted, 0x66)),
        (Role::BorderStrong, muted),
        (Role::FocusRing, accent),
    ];
    for (base, subtle, on_role, c) in [
        (Role::SuccessBase, Role::SuccessSubtle, Role::SuccessOn, green),
        (Role::WarningBase, Role::WarningSubtle, Role::WarningOn, yellow),
        (Role::DangerBase, Role::DangerSubtle, Role::DangerOn, red),
        (Role::InfoBase, Role::InfoSubtle, Role::InfoOn, blue),
    ] {
        if let Some(c) = c {
            colors.push((base, c));
            colors.push((subtle, at(c, 0x2e)));
            colors.push((on_role, on(c, fg, light_fg)));
        }
    }
    Some((mode, colors))
}

/// Watch the desktop's theme and call `changed` (from another thread)
/// whenever it does. Returns the watcher to keep alive, `None` when there
/// is nothing to watch or watching is unavailable.
///
/// Two desktops to watch, not one. Omarchy publishes a directory and the
/// files in it are the theme, so the files are watched. Every other Linux
/// desktop publishes light or dark through the portal and says so with a
/// signal, so the signal is listened for. Neither is the other's fallback
/// in any interesting sense — a machine has one or the other.
pub fn watch(changed: impl Fn() + Send + 'static) -> Option<Box<dyn std::any::Any + Send>> {
    match omarchy_dir() {
        Some(dir) => watch_files(dir, changed),
        #[cfg(target_os = "linux")]
        None => watch_portal(changed),
        #[cfg(not(target_os = "linux"))]
        None => None,
    }
}

/// The portal's `SettingChanged`, which is how every other Linux desktop
/// says it has changed its mind about light and dark.
///
/// The signal's own body is not read. Anything the rule lets through means
/// "ask again", and asking again is a method call that costs nothing and
/// settles to the same answer when nothing moved — which is a great deal
/// less code than decoding a variant to learn what a re-read would tell us
/// anyway.
///
/// The thread outlives its usefulness by up to one signal: it is parked in
/// the iterator, and the flag it checks is only looked at when something
/// wakes it. One idle thread per window, ended when the window is.
#[cfg(target_os = "linux")]
fn watch_portal(changed: impl Fn() + Send + 'static) -> Option<Box<dyn std::any::Any + Send>> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let rule = zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface("org.freedesktop.portal.Settings").ok()?.member("SettingChanged").ok()?.build();
    let signals = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, None).ok()?;
    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let mine = std::sync::Arc::clone(&alive);
    std::thread::Builder::new()
        .name("eui-theme-portal".into())
        .spawn(move || {
            for _ in signals {
                if !mine.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                changed();
            }
        })
        .ok()?;
    Some(Box::new(Stop(alive)))
}

/// Dropped with the window: the watching thread stops calling back.
#[cfg(target_os = "linux")]
struct Stop(std::sync::Arc<std::sync::atomic::AtomicBool>);

#[cfg(target_os = "linux")]
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Omarchy's theme directory, watched as files.
fn watch_files(dir: PathBuf, changed: impl Fn() + Send + 'static) -> Option<Box<dyn std::any::Any + Send>> {
    use notify::Watcher;
    let mut watcher = notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
        // Only what a theme switch touches — the `theme` link and the
        // `theme.name` file — and only a write, a creation, a rename or a
        // removal of them: the directory sees other traffic (the wallpaper
        // link, reads) that is not a change of theme.
        let Ok(event) = event else { return };
        let kind_matters = matches!(event.kind, notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_));
        let path_matters = event.paths.iter().any(|p| p.file_name().is_some_and(|n| n == "theme" || n == "theme.name"));
        if kind_matters && path_matters {
            changed();
        }
    })
    .ok()?;
    // `theme` is a symlink Omarchy swaps; `theme.name` is rewritten. Either
    // is a change of theme, and the directory sees both.
    watcher.watch(&dir, notify::RecursiveMode::NonRecursive).ok()?;
    Some(Box::new(watcher))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn omarchy_colors_map_onto_roles() {
        let text =
            "# Baltic Dusk\nmode = \"dark\"\n\naccent = \"#f7a96a\"\nmuted = \"#527493\"\nbackground = \"#101a26\"\nlighter_background = \"#1e3952\"\nforeground = \"#e6eef7\"\nred = \"#e06c75\"\n";
        let (mode, colors) = parse_omarchy(text).unwrap();
        assert_eq!(mode, ThemeMode::Dark);
        let of = |r: Role| colors.iter().find(|(x, _)| *x == r).map(|(_, c)| *c);
        assert_eq!(of(Role::SurfaceBase), Some(0x101a26ff));
        assert_eq!(of(Role::SurfaceRaised), Some(0x1e3952ff));
        assert_eq!(of(Role::AccentBase), Some(0xf7a96aff));
        assert_eq!(of(Role::AccentOn), Some(0x101a26ff), "dark text on a light accent");
        assert_eq!(of(Role::DangerBase), Some(0xe06c75ff));
        assert_eq!(of(Role::DangerSubtle), Some(0xe06c752e));
        assert!(of(Role::SuccessBase).is_none(), "no green given, the theme's own stays");
        // Secondary text reads on every surface, whatever the desktop's `muted`.
        let muted = of(Role::TextMuted).unwrap();
        for surface in [Role::SurfaceBase, Role::SurfaceRaised, Role::SurfaceSunken] {
            let c = contrast(Linear::from_rgba(muted), Linear::from_rgba(of(surface).unwrap()));
            assert!(c >= 4.5, "muted text on {surface:?}: {c:.2}");
        }
        assert_eq!(of(Role::TextDisabled), Some(0x527493ff), "the desktop's muted is for disabled things");
        // Hover lightens on a dark palette.
        let (h, a) = (of(Role::AccentHover).unwrap(), of(Role::AccentBase).unwrap());
        assert!(Linear::from_rgba(h).luminance() > Linear::from_rgba(a).luminance());
        assert!(parse_omarchy("mode = \"light\"\n").is_none());
    }
}
