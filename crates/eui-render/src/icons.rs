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

/// The grip on something that can be dragged (03 §3.4). Three short rules,
/// and both halves of that are decided rather than chosen. Not the six dots
/// the convention draws, because a dot here is a run of one point, which comes
/// out as a disc of the stroke's own radius — a pixel and a third at the size
/// a grip is used at — and what the eye gets is a smudge. And not two rules,
/// because two rules are an equals sign.
const GRIP: Icon = &[&[(8.0, 7.0), (16.0, 7.0)], &[(8.0, 12.0), (16.0, 12.0)], &[(8.0, 17.0), (16.0, 17.0)]];

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

/// A bracket a person steps out of, right: the way out of a session. Three
/// sides of a box, open where the arrow leaves it, so the shape reads at
/// 16 px without the arrow crossing a line it should not.
const LOGOUT: Icon = &[&[(13.0, 4.0), (5.0, 4.0), (5.0, 20.0), (13.0, 20.0)], &[(11.0, 12.0), (20.0, 12.0)], &[(17.0, 9.0), (20.0, 12.0), (17.0, 15.0)]];

/// A page with two hangers and a rule under its header: a month. The one
/// icon a date field needs, so that a field which opens a calendar does not
/// have to look like a select that opens a list.
const CALENDAR: Icon = &[&[(4.0, 6.0), (20.0, 6.0), (20.0, 20.0), (4.0, 20.0), (4.0, 6.0)], &[(4.0, 10.0), (20.0, 10.0)], &[(8.5, 3.5), (8.5, 7.5)], &[(15.5, 3.5), (15.5, 7.5)]];

/// Four panes: a dashboard, which is what a dashboard looks like when it is
/// one small square. Kept off the grid's edge so the four gaps read as gaps
/// and not as one grid of lines.
const PANES: Icon = &[
    &[(4.0, 4.0), (10.5, 4.0), (10.5, 10.5), (4.0, 10.5), (4.0, 4.0)],
    &[(13.5, 4.0), (20.0, 4.0), (20.0, 10.5), (13.5, 10.5), (13.5, 4.0)],
    &[(4.0, 13.5), (10.5, 13.5), (10.5, 20.0), (4.0, 20.0), (4.0, 13.5)],
    &[(13.5, 13.5), (20.0, 13.5), (20.0, 20.0), (13.5, 20.0), (13.5, 13.5)],
];

/// A page with a folded corner and two lines of writing: an order, an
/// invoice, a document of any kind. The fold is what stops it reading as a
/// plain rectangle at 16 px.
const DOC: Icon =
    &[&[(6.0, 3.0), (14.0, 3.0), (19.0, 8.0), (19.0, 21.0), (6.0, 21.0), (6.0, 3.0)], &[(14.0, 3.0), (14.0, 8.0), (19.0, 8.0)], &[(9.5, 13.0), (15.5, 13.0)], &[(9.5, 17.0), (15.5, 17.0)]];

/// A head and the shoulders under it, with a second person behind:
/// customers, of whom there is more than one. The head is a twelve-sided
/// circle, as `search`'s ring is.
const USERS: Icon = &[
    &[(13.5, 8.0), (13.1, 9.5), (12.0, 10.6), (10.5, 11.0), (9.0, 10.6), (7.9, 9.5), (7.5, 8.0), (7.9, 6.5), (9.0, 5.4), (10.5, 5.0), (12.0, 5.4), (13.1, 6.5), (13.5, 8.0)],
    &[(3.5, 20.0), (3.5, 17.5), (6.0, 14.5), (15.0, 14.5), (17.5, 17.5), (17.5, 20.0)],
    &[(16.0, 5.5), (18.5, 7.0), (18.5, 9.5), (16.5, 11.0)],
    &[(19.0, 14.5), (20.5, 17.0), (20.5, 20.0)],
];

/// A carton seen from the front, with the seam down it: stock on a shelf.
const BOX: Icon = &[&[(3.5, 7.5), (12.0, 3.5), (20.5, 7.5), (20.5, 16.5), (12.0, 20.5), (3.5, 16.5), (3.5, 7.5)], &[(3.5, 7.5), (12.0, 11.5), (20.5, 7.5)], &[(12.0, 11.5), (12.0, 20.5)]];

/// Three bars standing on a floor: a report. The bars rise left to right so
/// the shape has a direction, which a report generally does.
const CHART: Icon = &[&[(4.0, 20.0), (20.5, 20.0)], &[(7.5, 20.0), (7.5, 14.0)], &[(12.0, 20.0), (12.0, 9.5)], &[(16.5, 20.0), (16.5, 5.0)]];

/// Two rails with a handle on each, at different settings: settings. A gear
/// is the conventional mark and a poor one here — its teeth collapse into a
/// ring by 16 px, and this set is stroked, not filled.
///
/// The handles are ticks across the rails, not dots on them. A dot is a
/// capsule of zero length, so it is a disc of the stroke's own radius — one
/// unit on this grid, two thirds of a pixel at 16 px — drawn on top of the
/// very line it is meant to mark. It was invisible, and the icon read as an
/// equals sign.
const SLIDERS: Icon = &[&[(4.0, 8.5), (20.0, 8.5)], &[(4.0, 15.5), (20.0, 15.5)], &[(9.0, 5.5), (9.0, 11.5)], &[(15.5, 12.5), (15.5, 18.5)]];

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
        "grip" | "drag" => GRIP,
        "arrow_up" | "sort_asc" => ARROW_UP,
        "arrow_down" | "sort_desc" => ARROW_DOWN,
        "arrow_left" => ARROW_LEFT,
        "arrow_right" => ARROW_RIGHT,
        "more_h" => MORE_H,
        "more_v" => MORE_V,
        "dot" => DOT,
        "search" => SEARCH,
        "circle" => CIRCLE,
        "calendar" => CALENDAR,
        "logout" => LOGOUT,
        "warning" => WARNING,
        "grid" | "dashboard" => PANES,
        "doc" | "orders" => DOC,
        "users" | "customers" => USERS,
        "box" | "inventory" => BOX,
        "chart" | "reports" => CHART,
        "sliders" | "settings" => SLIDERS,
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
    "grip",
    "drag",
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
    "calendar",
    "logout",
    "warning",
    "grid",
    "dashboard",
    "doc",
    "orders",
    "users",
    "customers",
    "box",
    "inventory",
    "chart",
    "reports",
    "sliders",
    "settings",
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
