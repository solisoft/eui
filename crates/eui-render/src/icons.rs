//! A stroked icon set, drawn with the one capsule the renderer already has.
//!
//! `icon` has been a defined node kind since the first protocol version, laid
//! out like a picture and painted by nothing, so the catalogue drew its
//! chevrons and ticks as text characters against a fallback symbols face. A
//! glyph is a poor icon: it cannot be sized against the control it sits in, it
//! cannot take a colour apart from its label, and it reaches a screen reader as
//! itself.
//!
//! Every icon here is polylines on a 24-unit grid with a 2-unit stroke — the
//! proportions that stay legible at 16 px — and every segment is the same
//! rounded capsule a chart's line is made of. So this costs no new pipeline, no
//! font, no asset fetch and no protocol version: a name, and a table.
//!
//! A run of one point is a dot: a capsule of zero length is a circle of the
//! stroke's own radius.

/// The grid every icon is drawn on.
pub const GRID: f32 = 24.0;

/// The stroke width on that grid.
pub const STROKE: f32 = 2.0;

/// One icon: polylines, each a run of points on the 24-grid.
pub type Icon = &'static [&'static [(f32, f32)]];

const CHEVRON_DOWN: Icon = &[&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]];
const CHEVRON_UP: Icon = &[&[(6.0, 15.0), (12.0, 9.0), (18.0, 15.0)]];
const CHEVRON_LEFT: Icon = &[&[(15.0, 6.0), (9.0, 12.0), (15.0, 18.0)]];
const CHEVRON_RIGHT: Icon = &[&[(9.0, 6.0), (15.0, 12.0), (9.0, 18.0)]];

const CHECK: Icon = &[&[(5.0, 12.5), (10.0, 17.5), (19.0, 6.5)]];
const MINUS: Icon = &[&[(5.0, 12.0), (19.0, 12.0)]];
const PLUS: Icon = &[&[(12.0, 5.0), (12.0, 19.0)], &[(5.0, 12.0), (19.0, 12.0)]];
const CLOSE: Icon = &[&[(6.0, 6.0), (18.0, 18.0)], &[(18.0, 6.0), (6.0, 18.0)]];

const MENU: Icon = &[&[(4.0, 7.0), (20.0, 7.0)], &[(4.0, 12.0), (20.0, 12.0)], &[(4.0, 17.0), (20.0, 17.0)]];

const ARROW_UP: Icon = &[&[(12.0, 19.0), (12.0, 5.0)], &[(6.0, 11.0), (12.0, 5.0), (18.0, 11.0)]];
const ARROW_DOWN: Icon = &[&[(12.0, 5.0), (12.0, 19.0)], &[(6.0, 13.0), (12.0, 19.0), (18.0, 13.0)]];
const ARROW_LEFT: Icon = &[&[(19.0, 12.0), (5.0, 12.0)], &[(11.0, 6.0), (5.0, 12.0), (11.0, 18.0)]];
const ARROW_RIGHT: Icon = &[&[(5.0, 12.0), (19.0, 12.0)], &[(13.0, 6.0), (19.0, 12.0), (13.0, 18.0)]];

const MORE_H: Icon = &[&[(5.5, 12.0)], &[(12.0, 12.0)], &[(18.5, 12.0)]];
const MORE_V: Icon = &[&[(12.0, 5.5)], &[(12.0, 12.0)], &[(12.0, 18.5)]];
const DOT: Icon = &[&[(12.0, 12.0)]];

/// A twelve-sided circle, which at icon sizes is a circle. Centre (11, 11),
/// radius 6 — the ring a magnifier is made of.
const SEARCH: Icon = &[
    &[(17.0, 11.0), (16.196, 14.0), (14.0, 16.196), (11.0, 17.0), (8.0, 16.196), (5.804, 14.0), (5.0, 11.0), (5.804, 8.0), (8.0, 5.804), (11.0, 5.0), (14.0, 5.804), (16.196, 8.0), (17.0, 11.0)],
    &[(15.5, 15.5), (20.0, 20.0)],
];

const CIRCLE: Icon = &[&[(20.0, 12.0), (19.0, 16.0), (16.0, 19.0), (12.0, 20.0), (8.0, 19.0), (5.0, 16.0), (4.0, 12.0), (5.0, 8.0), (8.0, 5.0), (12.0, 4.0), (16.0, 5.0), (19.0, 8.0), (20.0, 12.0)]];

/// A triangle with a bang in it, for a warning.
const WARNING: Icon = &[&[(12.0, 4.0), (21.0, 19.5), (3.0, 19.5), (12.0, 4.0)], &[(12.0, 10.0), (12.0, 14.5)], &[(12.0, 17.0)]];

/// The name a server sends, and the paths it means. An unknown name draws
/// nothing, which is the same forward-compatibility rule the rest of the
/// vocabulary keeps: a client that has not learned an icon leaves a gap of the
/// right size rather than refusing the batch.
#[must_use]
pub fn icon(name: &str) -> Option<Icon> {
    Some(match name {
        "chevron_down" => CHEVRON_DOWN,
        "chevron_up" => CHEVRON_UP,
        "chevron_left" => CHEVRON_LEFT,
        "chevron_right" => CHEVRON_RIGHT,
        "check" => CHECK,
        "minus" | "dash" => MINUS,
        "plus" => PLUS,
        "close" => CLOSE,
        "menu" => MENU,
        "arrow_up" | "sort_asc" => ARROW_UP,
        "arrow_down" | "sort_desc" => ARROW_DOWN,
        "arrow_left" => ARROW_LEFT,
        "arrow_right" => ARROW_RIGHT,
        "more_h" => MORE_H,
        "more_v" => MORE_V,
        "dot" => DOT,
        "search" => SEARCH,
        "circle" => CIRCLE,
        "warning" => WARNING,
        _ => return None,
    })
}

/// Every name this client draws, for a catalogue that wants to check itself.
pub const NAMES: &[&str] = &[
    "chevron_down",
    "chevron_up",
    "chevron_left",
    "chevron_right",
    "check",
    "minus",
    "dash",
    "plus",
    "close",
    "menu",
    "arrow_up",
    "arrow_down",
    "arrow_left",
    "arrow_right",
    "sort_asc",
    "sort_desc",
    "more_h",
    "more_v",
    "dot",
    "search",
    "circle",
    "warning",
];

#[cfg(test)]
mod tests {
    use super::{icon, GRID, NAMES};

    #[test]
    fn every_named_icon_resolves_and_fits_its_grid() {
        let missing: Vec<&str> = NAMES.iter().copied().filter(|n| icon(n).is_none()).collect();
        assert!(missing.is_empty(), "named but undrawn: {missing:?}");
        for name in NAMES {
            let Some(paths) = icon(name) else { continue };
            assert!(!paths.is_empty(), "{name} is empty");
            for stroke in paths {
                assert!(!stroke.is_empty(), "{name} has an empty stroke");
                for (x, y) in *stroke {
                    assert!((0.0..=GRID).contains(x), "{name} runs off the grid at x {x}");
                    assert!((0.0..=GRID).contains(y), "{name} runs off the grid at y {y}");
                }
            }
        }
    }

    #[test]
    fn an_unknown_name_is_not_an_error() {
        assert!(icon("no_such_icon").is_none());
    }
}
