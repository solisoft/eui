//! What goes on and around the notes once they are placed: ties and slurs,
//! tuplets, articulations, dynamics and hairpins, words, endings and tempo
//! marks, and how far all of it reaches (`eui_builders_score.sl`, "Ties and
//! slurs" and "Expression").

use crate::engrave::{barline_width, clef_glyph, rest_name, stem_x, y_of, Line};
use crate::glyphs::{glyph, Weight};
use crate::measure::Bar;
use crate::parse::{Ev, Kind, Tempo};
use crate::text::{args_of, em, text_box, text_width};
use crate::RULE;

/// A tempo mark, where it is printed.
#[derive(Debug, Clone)]
pub struct TempoMark {
    /// Its left.
    pub x: f64,
    /// Its baseline.
    pub y: f64,
    /// What it says.
    pub mark: Tempo,
}

/// An ending's bracket.
#[derive(Debug, Clone, Default)]
pub struct Volta {
    /// "1.", "2."…
    pub label: String,
    /// Its ending starts on this line.
    pub first: bool,
    /// Its left.
    pub x0: f64,
    /// Its right.
    pub x1: f64,
    /// It goes back: a hook at its end.
    pub closed: bool,
    /// Its line.
    pub y: f64,
    /// Its bars, by index into the staff's.
    pub bars: Vec<usize>,
}

/// A tuplet's number, and its bracket if it has one.
#[derive(Debug, Clone)]
pub struct TupletPlace {
    /// Its left.
    pub x0: f64,
    /// Its right.
    pub x1: f64,
    /// Its line at `x0`.
    pub ya: f64,
    /// … at `x1`.
    pub yb: f64,
    /// Above the notes.
    pub up: bool,
    /// Drawn with a bracket.
    pub bracket: bool,
    /// The number.
    pub label: String,
}

/// An articulation's glyph, centred on `x`.
#[derive(Debug, Clone)]
pub struct ScriptPlace {
    /// The glyph.
    pub glyph: &'static str,
    /// Its centre.
    pub x: f64,
    /// Its origin.
    pub y: f64,
}

/// A dynamic or a hairpin on a line under the staff.
#[derive(Debug, Clone, Default)]
pub struct DynEl {
    /// The dynamic's letters; none for a hairpin.
    pub text: Option<String>,
    /// Its left.
    pub x0: f64,
    /// Its right.
    pub x1: f64,
    /// `<` or `>`.
    pub grow: char,
    /// How open the piece is at its start, 0–1.
    pub from: f64,
    /// … at its end.
    pub to: f64,
    /// Its ink above its line.
    pub top: f64,
    /// … and below.
    pub bottom: f64,
    /// Its line.
    pub y: f64,
}

/// Words over or under the notes.
#[derive(Debug, Clone)]
pub struct TextPlace {
    /// The words.
    pub text: String,
    /// Their left.
    pub x0: f64,
    /// Their right.
    pub x1: f64,
    /// The baseline.
    pub y: f64,
    /// The ink's top.
    pub top: f64,
    /// … bottom.
    pub bottom: f64,
    /// -1 above, 1 below.
    pub dir: i64,
}

/// One system's piece of a hairpin.
#[derive(Debug, Clone)]
pub struct HairpinPiece {
    /// Its left.
    pub x0: f64,
    /// Its right.
    pub x1: f64,
    /// `<` or `>`.
    pub grow: char,
    /// How far through the hairpin it starts, 0–1.
    pub from: f64,
    /// … ends.
    pub to: f64,
}

struct ScriptRule {
    up: &'static str,
    down: &'static str,
    pad: f64,
    prio: i64,
    side: i64,
    quant: bool,
}

fn script_rule(name: &str) -> ScriptRule {
    let r = |up, down, pad, prio, side, quant| ScriptRule { up, down, pad, prio, side, quant };
    match name {
        "staccato" => r("articStaccatoAbove", "articStaccatoAbove", 0.2, -100, 0, true),
        "tenuto" => r("articTenutoAbove", "articTenutoAbove", 0.2, -50, 0, true),
        "accent" => r("articAccentAbove", "articAccentAbove", 0.2, 0, 0, false),
        "marcato" => r("articMarcatoAbove", "articMarcatoBelow", 0.2, 0, 0, true),
        "staccatissimo" => r("articStaccatissimoAbove", "articStaccatissimoBelow", 0.2, 0, 0, false),
        "portato" => r("articTenutoStaccatoAbove", "articTenutoStaccatoBelow", 0.45, 0, 0, false),
        _ => r("fermataAbove", "fermataBelow", 0.4, 4000, -1, false),
    }
}

// ---- Ties and slurs ---------------------------------------------------------

/// LilyPond's height of a curve over `width`: it grows with the width and
/// levels off at `limit`.
pub(crate) fn curve_height(width: f64, limit: f64, ratio: f64) -> f64 {
    limit * 2.0 / std::f64::consts::PI * atan(std::f64::consts::PI / 2.0 * ratio / limit * width)
}

/// The Soli engraver's arctangent, a minimax polynomial good to 1e-5: kept
/// so the two draw the same curve to the last digit.
fn atan(v: f64) -> f64 {
    if v < 0.0 {
        return -atan(-v);
    }
    if v > 1.0 {
        return std::f64::consts::PI / 2.0 - atan(1.0 / v);
    }
    let v2 = v * v;
    v * (0.99997726 + v2 * (-0.33262347 + v2 * (0.19354346 + v2 * (-0.11643287 + v2 * (0.05265332 + v2 * -0.01172120)))))
}

/// A note, by its bar and its place there.
type At = (usize, usize);

/// Every tie and slur of a staff, on the lines they cross.
pub(crate) fn curves(bars: &mut [Bar], lines: &mut [Line]) {
    let mut notes: Vec<At> = Vec::new();
    for (si, line) in lines.iter_mut().enumerate() {
        for b in line.bars.clone() {
            for (j, ev) in bars[b].items.iter_mut().enumerate() {
                if ev.k == Kind::Note {
                    ev.sys = si;
                    notes.push((b, j));
                }
            }
        }
        line.curves = Vec::new();
    }
    let ev_at = |at: At| -> &Ev { &bars[at.0].items[at.1] };
    for (i, &at) in notes.iter().enumerate() {
        let ev = ev_at(at);
        if !ev.tie {
            continue;
        }
        // The next note of the same voice.
        let Some(nxt) = notes.iter().skip(i + 1).map(|&a| ev_at(a)).find(|n| voice_of(n) == voice_of(ev)) else { continue };
        let count = ev.heads.len();
        for (hi, hd) in ev.heads.iter().enumerate() {
            let Some(other) = nxt.heads.iter().find(|o| o.d == hd.d) else { continue };
            let mut over = if count == 1 { !ev.up } else { hi >= count / 2 };
            // Two voices: the first's ties over, the second's under.
            if let Some(v) = ev.voice {
                over = v == 1;
            }
            tie(lines, ev, hd.pos, nxt, other.pos, over);
        }
    }
    // Each voice's slurs close on its own notes.
    let mut open: [Vec<At>; 2] = [Vec::new(), Vec::new()];
    for &at in &notes {
        let ev = ev_at(at);
        let mine = &mut open[usize::from(voice_of(ev) - 1)];
        if ev.slur_on {
            mine.push(at);
        }
        if ev.slur_off {
            if let Some(first) = mine.pop() {
                slur(lines, bars, &notes, first, at);
            }
        }
    }
}

fn tie(lines: &mut [Line], ev: &Ev, pos: i64, nxt: &Ev, other: i64, over: bool) {
    let dir = if over { -1.0 } else { 1.0 };
    let y = y_of(pos) + dir * 0.35;
    let y2 = y_of(other) + dir * 0.35;
    let xa = ev.x + ev.head_w.unwrap_or(0.0) + 0.15;
    let xb = nxt.x - 0.15;
    if ev.sys == nxt.sys {
        arc(&mut lines[ev.sys], xa, y, xb, y2, dir, 1.0, 0.333);
    } else {
        let end = lines[ev.sys].bar_end - 0.5;
        arc(&mut lines[ev.sys], xa, y, end, y, dir, 1.0, 0.333);
        let later = &mut lines[nxt.sys];
        let start = later.prefix - 0.6;
        arc(later, start, y2, xb, y2, dir, 1.0, 0.333);
    }
}

fn slur(lines: &mut [Line], bars: &[Bar], notes: &[At], first: At, last: At) {
    let ev_at = |at: At| -> &Ev { &bars[at.0].items[at.1] };
    let (f, l) = (ev_at(first), ev_at(last));
    let (fnum, lnum) = (f.n.unwrap_or(0), l.n.unwrap_or(0));
    let ups: Vec<&Ev> = notes.iter().map(|&a| ev_at(a)).filter(|ev| ev.n.unwrap_or(0) >= fnum && ev.n.unwrap_or(0) <= lnum && voice_of(ev) == voice_of(f)).collect();
    let all_up = ups.iter().all(|ev| ev.up);
    let mut over = !all_up;
    // Of two voices, the first's slurs above and the second's below.
    if let Some(v) = f.voice {
        over = v == 1;
    }
    let dir = if over { -1.0 } else { 1.0 };
    let e0 = slur_end(f, over);
    let e1 = slur_end(l, over);
    let inner: Vec<&Ev> = ups.iter().copied().filter(|ev| ev.n != f.n && ev.n != l.n).collect();
    if f.sys == l.sys {
        slur_piece(&mut lines[f.sys], &inner, e0.0, e0.1, e1.0, e1.1, dir, over);
        return;
    }
    let mine: Vec<&Ev> = inner.iter().copied().filter(|ev| ev.sys == f.sys).collect();
    let end = lines[f.sys].bar_end - 0.5;
    slur_piece(&mut lines[f.sys], &mine, e0.0, e0.1, end, e0.1, dir, over);
    let theirs: Vec<&Ev> = inner.iter().copied().filter(|ev| ev.sys == l.sys).collect();
    let start = lines[l.sys].prefix - 0.6;
    slur_piece(&mut lines[l.sys], &theirs, start, e1.1, e1.0, e1.1, dir, over);
}

#[allow(clippy::too_many_arguments)]
fn slur_piece(line: &mut Line, inner: &[&Ev], xa: f64, mut ya: f64, xb: f64, mut yb: f64, dir: f64, over: bool) {
    let width = xb - xa;
    if width <= 0.2 {
        return;
    }
    let rise = yb - ya;
    let most = 0.125 * width;
    if rise.abs() > most {
        if (rise > 0.0) == (dir < 0.0) {
            yb = ya + if rise > 0.0 { most } else { -most };
        } else {
            ya = yb - if rise > 0.0 { most } else { -most };
        }
    }
    let mut blocked = false;
    for ev in inner {
        let u = (ev.x + ev.head_w.unwrap_or(0.0) / 2.0 - xa) / width;
        let p = slur_end(ev, over);
        let base = ya + (yb - ya) * u;
        if u > 0.0 && u < 1.0 && (base - p.1) * -dir > 0.0 {
            blocked = true;
        }
    }
    if blocked {
        let level = if dir < 0.0 { ya.min(yb) } else { ya.max(yb) };
        ya = level;
        yb = level;
    }
    let mut hgt = curve_height(width, 2.0, 0.25);
    let mut lift: f64 = 0.0;
    for ev in inner {
        let u = (ev.x + ev.head_w.unwrap_or(0.0) / 2.0 - xa) / width;
        if u <= 0.0 || u >= 1.0 {
            continue;
        }
        let p = slur_end(ev, over);
        let base = ya + (yb - ya) * u;
        let need = (base - p.1) * -dir;
        let bulge = 4.0 * u * (1.0 - u);
        if need > hgt * bulge {
            let want = need / bulge;
            if want <= 3.0 {
                hgt = want;
            } else {
                hgt = 3.0;
                lift = lift.max(need - 3.0 * bulge);
            }
        }
    }
    ya += dir * lift;
    yb += dir * lift;
    arc_h(line, xa, ya, xb, yb, dir, hgt);
}

fn slur_end(ev: &Ev, over: bool) -> (f64, f64) {
    let mid_x = ev.x + ev.head_w.unwrap_or(0.0) / 2.0;
    if over {
        let top = y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.75;
        if ev.up && ev.base > 1 {
            return (stem_x(ev) - 0.06, ev.stem_end - 0.3);
        }
        return (mid_x, top);
    }
    let low = y_of(ev.heads.first().map_or(0, |h| h.pos)) + 0.75;
    if !ev.up && ev.base > 1 {
        return (stem_x(ev) + 0.06, ev.stem_end + 0.3);
    }
    (mid_x, low)
}

#[allow(clippy::too_many_arguments)]
fn arc(line: &mut Line, xa: f64, ya: f64, xb: f64, yb: f64, dir: f64, limit: f64, ratio: f64) {
    let hgt = curve_height((xb - xa).max(0.1), limit, ratio);
    arc_h(line, xa, ya, xb, yb, dir, hgt);
}

/// A curve as a closed outline: out along the outer Bézier, back along the
/// inner one, `RULE.curve_tip` thick at the ends and `curve_mid` between.
fn arc_h(line: &mut Line, xa: f64, ya: f64, xb: f64, yb: f64, dir: f64, hgt: f64) {
    let w = xb - xa;
    if w <= 0.0 {
        return;
    }
    let inset = w * 0.25;
    let c_out = (hgt + RULE.curve_mid / 2.0) * 4.0 / 3.0;
    let c_in = (hgt - RULE.curve_mid / 2.0) * 4.0 / 3.0;
    let t2 = RULE.curve_tip / 2.0;
    line.curves.push(vec![
        0.0,
        xa,
        ya + dir * t2,
        3.0,
        xa + inset,
        ya + dir * c_out,
        xb - inset,
        yb + dir * c_out,
        xb,
        yb + dir * t2,
        1.0,
        xb,
        yb - dir * t2,
        3.0,
        xb - inset,
        yb + dir * c_in,
        xa + inset,
        ya + dir * c_in,
        xa,
        ya - dir * t2,
        4.0,
    ]);
}

/// The points of a fill's commands, `(x, y)`.
pub(crate) fn points(cmds: &[f64]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut ci = 0;
    while ci < cmds.len() {
        let args = args_of(cmds[ci]);
        for jj in (0..args).step_by(2) {
            if let (Some(x), Some(y)) = (cmds.get(ci + 1 + jj), cmds.get(ci + 2 + jj)) {
                out.push((*x, *y));
            }
        }
        ci += 1 + args;
    }
    out
}

// ---- Marks ------------------------------------------------------------------

/// Everything around a staff's notes, line by line.
pub(crate) fn marks(bars: &mut [Bar], lines: &mut [Line]) {
    hairpins(bars, lines);
    for line in lines.iter_mut() {
        line.voltas = if line.staff_index.unwrap_or(0) == 0 { voltas(bars, line) } else { Vec::new() };
        line.tuplets = tuplets(bars, line);
        scripts(bars, line);
        dynamics(bars, line);
        texts(bars, line);
        lyric_line(bars, line);
        line.marks = Vec::new();
        let range = line.bars.clone();
        for (bi, b) in range.clone().enumerate() {
            for (ii, ev) in bars[b].items.iter().enumerate() {
                let Some(mark) = &ev.tempo else { continue };
                let mut x = ev.x;
                if bi == 0 && ii == 0 {
                    x = line.left + 0.8 + glyph(clef_glyph(bars[b].clef)).advance + 1.0;
                }
                let w = mark_width(mark);
                let mut low: f64 = -1.0;
                for ob in range.clone() {
                    for other in &bars[ob].items {
                        if other.x + 1.5 < x || other.x > x + w + 1.0 || other.k != Kind::Note {
                            continue;
                        }
                        low = low.min(y_of(other.heads.last().map_or(0, |h| h.pos)) - 0.7);
                        low = low.min(other.stem_end - 0.3);
                    }
                }
                for tp in &line.tuplets {
                    if !tp.up || tp.x1 < x - 1.0 || tp.x0 > x + w + 1.0 {
                        continue;
                    }
                    low = low.min(tp.ya.min(tp.yb) - 1.0);
                }
                for ob in range.clone() {
                    for other in &bars[ob].items {
                        if other.x + 1.5 < x || other.x > x + w + 1.0 {
                            continue;
                        }
                        low = low.min(other.reach_up.unwrap_or(0.0) - 0.3);
                    }
                }
                for tx in &line.texts {
                    if tx.dir > 0 || tx.x1 < x - 1.0 || tx.x0 > x + w + 1.0 {
                        continue;
                    }
                    low = low.min(tx.top - 0.3);
                }
                for c in &line.curves {
                    for (px, py) in points(c) {
                        if px >= x - 1.0 && px <= x + w + 1.0 {
                            low = low.min(py - 0.2);
                        }
                    }
                }
                line.marks.push(TempoMark { x, y: low - 0.9, mark: mark.clone() });
            }
        }
    }
}

fn tuplets(bars: &[Bar], line: &Line) -> Vec<TupletPlace> {
    let mut groups: Vec<(i64, i64, Vec<&Ev>)> = Vec::new();
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            let Some(t) = ev.tuplet else { continue };
            match groups.iter_mut().find(|g| g.0 == t.id) {
                Some(g) => g.2.push(ev),
                None => groups.push((t.id, t.n, vec![ev])),
            }
        }
    }
    groups.iter().map(|(_, n, items)| tuplet_place(bars, items, *n)).collect()
}

fn tuplet_place(bars: &[Bar], items: &[&Ev], n: i64) -> TupletPlace {
    let notes: Vec<&Ev> = items.iter().copied().filter(|ev| ev.k == Kind::Note).collect();
    let first_beam = notes.first().and_then(|ev| ev.beam.map(|b| (ev.bar, b)));
    let beamed = notes.len() == items.len() && notes.len() > 1 && first_beam.is_some() && notes.iter().all(|ev| ev.beam.map(|b| (ev.bar, b)) == first_beam);
    let ups = notes.iter().filter(|ev| ev.up).count();
    let up = if beamed { notes[0].beam.is_some_and(|b| bars[notes[0].bar].beams[b].up) } else { ups * 2 >= notes.len() };
    let last = items[items.len() - 1];
    let x0 = items[0].x - 0.2;
    let x1 = last.x + last.head_w.unwrap_or(1.2) + 0.2;
    let dir = if up { -1.0 } else { 1.0 };
    let mut ya;
    let mut yb;
    if beamed {
        let g = &bars[notes[0].bar].beams[notes[0].beam.unwrap_or(0)];
        ya = crate::engrave::beam_y(g, x0) + 1.1 * dir;
        yb = crate::engrave::beam_y(g, x1) + 1.1 * dir;
    } else {
        let edges: Vec<f64> = items.iter().map(|ev| tuplet_edge(ev, up)).collect();
        ya = edges[0] + 1.1 * dir;
        yb = edges[edges.len() - 1] + 1.1 * dir;
        yb = ya + (yb - ya).clamp(-1.0, 1.0);
        let span = (x1 - x0).max(0.01);
        let mut push: f64 = 0.0;
        for (k, ev) in items.iter().enumerate() {
            let at = ya + (yb - ya) * (ev.x - x0) / span;
            let need = edges[k] + 1.1 * dir;
            push = if up { push.min(need - at) } else { push.max(need - at) };
        }
        let reach = if up { ya.min(yb) + push + 0.7 } else { ya.max(yb) + push - 0.7 };
        push += if up { (-0.25 - reach).min(0.0) } else { (4.25 - reach).max(0.0) };
        ya += push;
        yb += push;
    }
    TupletPlace { x0, x1, ya, yb, up, bracket: !beamed, label: n.to_string() }
}

/// How far a note or a rest reaches on one side, in staff y: its stem's end
/// or the outside of its heads; a rest, the middle of the staff and a space.
fn tuplet_edge(ev: &Ev, up: bool) -> f64 {
    if ev.k != Kind::Note {
        return if up { 1.0 } else { 3.0 } + rest_lift(ev);
    }
    let y_top = y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.5;
    let y_bottom = y_of(ev.heads.first().map_or(0, |h| h.pos)) + 0.5;
    if up {
        return y_top.min(if ev.up && ev.base > 1 { ev.stem_end } else { y_top });
    }
    y_bottom.max(if !ev.up && ev.base > 1 { ev.stem_end } else { y_bottom })
}

/// Where a rest of two voices stands: the first voice's two spaces up, the
/// second's two down; a rest of one voice on the middle of the staff.
pub(crate) fn rest_lift(ev: &Ev) -> f64 {
    match ev.voice {
        Some(1) => -2.0,
        Some(2) => 2.0,
        _ => 0.0,
    }
}

/// The voice a note sings in: the second, or the first, which a note
/// outside `<< \\\\ >>` goes on as.
pub(crate) fn voice_of(ev: &Ev) -> u8 {
    if ev.voice == Some(2) {
        2
    } else {
        1
    }
}

fn item_extent(ev: &Ev) -> (f64, f64) {
    if ev.k != Kind::Note {
        return (1.0 + rest_lift(ev), 3.0 + rest_lift(ev));
    }
    (tuplet_edge(ev, true), tuplet_edge(ev, false))
}

pub(crate) fn item_w(ev: &Ev) -> f64 {
    if let Some(w) = ev.head_w {
        return w;
    }
    if ev.mm.is_some() {
        return (ev.span1 - ev.span0 - 2.0).max(1.0);
    }
    glyph(rest_name(ev.base)).advance
}

fn scripts(bars: &mut [Bar], line: &mut Line) {
    line.scripts = Vec::new();
    for b in line.bars.clone() {
        for ev in &mut bars[b].items {
            let ext = item_extent(ev);
            ev.reach_up = Some(ext.0);
            ev.reach_down = Some(ext.1);
            if ev.scripts.is_empty() {
                continue;
            }
            let xc = ev.x + item_w(ev) / 2.0;
            let note = ev.k == Kind::Note;
            let heads_top = if note { y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.5 } else { 1.0 + rest_lift(ev) };
            let heads_bottom = if note { y_of(ev.heads.first().map_or(0, |h| h.pos)) + 0.5 } else { 3.0 + rest_lift(ev) };
            let mut up_cursor = heads_top;
            let mut down_cursor = heads_bottom;
            if note && ev.up {
                up_cursor = ext.0;
            }
            if note && !ev.up {
                down_cursor = ext.1;
            }
            let mut list: Vec<_> = ev.scripts.iter().collect();
            list.sort_by_key(|sc| script_rule(&sc.name).prio);
            for sc in list {
                let rule = script_rule(&sc.name);
                let mut dir = sc.dir;
                if dir == 0 && rule.side != 0 {
                    dir = rule.side;
                }
                if dir == 0 {
                    dir = if ev.up { 1 } else { -1 };
                }
                let name = if dir < 0 { rule.up } else { rule.down };
                let bb = glyph(name).bbox;
                let mut y;
                if dir < 0 {
                    y = up_cursor - rule.pad - bb[3];
                    if rule.quant {
                        let mid = y + (bb[1] + bb[3]) / 2.0;
                        if mid > -0.5 && mid < 4.5 {
                            y += space_above(mid) - mid;
                        }
                    } else if y + bb[3] > -0.25 {
                        y = -0.25 - bb[3];
                    }
                    up_cursor = y + bb[1];
                } else {
                    y = down_cursor + rule.pad - bb[1];
                    if rule.quant {
                        let mid = y + (bb[1] + bb[3]) / 2.0;
                        if mid > -0.5 && mid < 4.5 {
                            y += space_below(mid) - mid;
                        }
                    } else if y + bb[1] < 4.25 {
                        y = 4.25 - bb[1];
                    }
                    down_cursor = y + bb[3];
                }
                line.scripts.push(ScriptPlace { glyph: name, x: xc, y });
            }
            ev.reach_up = Some(ext.0.min(up_cursor));
            ev.reach_down = Some(ext.1.max(down_cursor));
        }
    }
}

fn space_above(y: f64) -> f64 {
    (y - 0.5).floor() + 0.5
}

fn space_below(y: f64) -> f64 {
    (y - 0.5).ceil() + 0.5
}

fn hairpins(bars: &[Bar], lines: &mut [Line]) {
    let mut flat: Vec<(usize, usize, usize)> = Vec::new();
    for (si, line) in lines.iter_mut().enumerate() {
        line.hairpins = Vec::new();
        for b in line.bars.clone() {
            for j in 0..bars[b].items.len() {
                flat.push((si, b, j));
            }
        }
    }
    let ev_of = |f: (usize, usize, usize)| -> &Ev { &bars[f.1].items[f.2] };
    for (k, &at) in flat.iter().enumerate() {
        let start_ev = ev_of(at);
        let Some(grow) = start_ev.hairpin else { continue };
        let mut stop = flat.len() - 1;
        let mut how = "end";
        for (jj, &f) in flat.iter().enumerate().skip(k + 1) {
            let other = ev_of(f);
            if other.hairpin_end {
                stop = jj;
                how = "note";
                break;
            }
            if other.dynamic.is_some() || other.hairpin.is_some() {
                stop = jj;
                how = "next";
                break;
            }
        }
        let mut x0 = start_ev.x;
        if let Some(d) = &start_ev.dynamic {
            x0 = x0.max(dyn_x(start_ev, d) + dyn_width(d) + 1.0);
        }
        let mut end_at = flat[stop];
        let end_ev = ev_of(end_at);
        let mut x1 = end_ev.x + item_w(end_ev) + 0.15;
        if how == "next" {
            x1 = match &end_ev.dynamic {
                None => end_ev.x - 0.6,
                Some(d) => dyn_x(end_ev, d) - 1.0,
            };
        }
        if how != "end" && end_ev.start == 0 && stop > k && bars[flat[stop - 1].1].number != bars[end_at.1].number {
            let before = &bars[flat[stop - 1].1];
            x1 = x1.min(before.x1 + barline_width(&before.bar) - 0.8);
            end_at = flat[stop - 1];
        }
        let first = at.0;
        let last = end_at.0;
        for (si, line) in lines.iter_mut().enumerate().take(last + 1).skip(first) {
            let a = if si == first { x0 } else { line.prefix };
            let b = if si == last { x1 } else { line.right };
            let parts = (last - first + 1) as f64;
            let ka = (si - first) as f64 / parts;
            let kb = (si - first + 1) as f64 / parts;
            line.hairpins.push(HairpinPiece { x0: a, x1: b.max(a + 2.0), grow, from: ka, to: kb });
        }
    }
}

pub(crate) fn dyn_letters(dynamic: &str) -> Vec<&'static str> {
    dynamic
        .chars()
        .map(|ch| match ch {
            'p' => "dynamicPiano",
            'm' => "dynamicMezzo",
            'f' => "dynamicForte",
            'r' => "dynamicRinforzando",
            's' => "dynamicSforzando",
            'z' => "dynamicZ",
            _ => "dynamicNiente",
        })
        .collect()
}

fn dyn_width(dynamic: &str) -> f64 {
    dyn_letters(dynamic).iter().fold(0.0, |acc, nm| acc + glyph(nm).advance)
}

fn dyn_x(ev: &Ev, dynamic: &str) -> f64 {
    ev.x + item_w(ev) / 2.0 - dyn_width(dynamic) / 2.0
}

fn dynamics(bars: &[Bar], line: &mut Line) {
    let mut els: Vec<DynEl> = Vec::new();
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            let Some(d) = &ev.dynamic else { continue };
            let x = dyn_x(ev, d);
            let mut top: f64 = 0.0;
            let mut bottom: f64 = 0.0;
            for name in dyn_letters(d) {
                let bb = glyph(name).bbox;
                top = top.min(bb[1]);
                bottom = bottom.max(bb[3]);
            }
            els.push(DynEl { text: Some(d.clone()), x0: x, x1: x + dyn_width(d), top: 0.6 + top, bottom: 0.6 + bottom, ..DynEl::default() });
        }
    }
    for hp in &line.hairpins {
        els.push(DynEl { text: None, x0: hp.x0, x1: hp.x1, grow: hp.grow, from: hp.from, to: hp.to, top: -0.6666, bottom: 0.6666, y: 0.0 });
    }
    els.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    line.dynamics = Vec::new();
    let mut k = 0;
    while k < els.len() {
        let mut group = vec![k];
        let mut reach = els[k].x1;
        k += 1;
        while k < els.len() && els[k].x0 - reach < 1.5 {
            group.push(k);
            reach = reach.max(els[k].x1);
            k += 1;
        }
        let mut level: f64 = 0.0;
        for &g in &group {
            level = level.max(dyn_line(bars, line, &els[g]));
        }
        for &g in &group {
            let mut el = els[g].clone();
            el.y = level;
            el.bottom = level + el.bottom + 0.3;
            line.dynamics.push(el);
        }
    }
}

fn dyn_line(bars: &[Bar], line: &Line, el: &DynEl) -> f64 {
    let mut need = 4.0 + 0.6 - el.top;
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            let right = ev.x + item_w(ev);
            if right < el.x0 - 0.2 || ev.x > el.x1 + 0.2 {
                continue;
            }
            let mut rise = -el.top;
            if el.text.is_none() {
                let span = (el.x1 - el.x0).max(0.01);
                let at = ((ev.x - el.x0) / span).clamp(0.0, 1.0);
                let mut open = el.from + (el.to - el.from) * at;
                if el.grow == '>' {
                    open = 1.0 - open;
                }
                rise = 0.6666 * open;
            }
            need = need.max(ev.reach_down.unwrap_or(4.0) + 0.6 + rise);
        }
    }
    for tp in &line.tuplets {
        if tp.up || tp.x1 < el.x0 || tp.x0 > el.x1 {
            continue;
        }
        need = need.max(tp.ya.max(tp.yb) + 0.9 + 0.6 - el.top);
    }
    need
}

fn texts(bars: &[Bar], line: &mut Line) {
    line.texts = Vec::new();
    let e = em(0.0);
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            for tx in &ev.texts {
                let w = text_width(&tx.text, Weight::Regular, e);
                let bx = text_box(&tx.text, Weight::Regular, e);
                let x0 = ev.x;
                let x1 = x0 + w;
                let y = if tx.dir < 0 {
                    let mut edge: f64 = -0.5;
                    for ob in line.bars.clone() {
                        for other in &bars[ob].items {
                            if other.x + item_w(other) < x0 - 0.2 || other.x > x1 + 0.2 {
                                continue;
                            }
                            edge = edge.min(other.reach_up.unwrap_or(0.0));
                        }
                    }
                    for prev in &line.texts {
                        if prev.dir < 0 && prev.x1 > x0 && prev.x0 < x1 {
                            edge = edge.min(prev.top);
                        }
                    }
                    edge - 0.45 - bx.1
                } else {
                    let mut edge: f64 = 4.5;
                    for ob in line.bars.clone() {
                        for other in &bars[ob].items {
                            if other.x + item_w(other) < x0 - 0.2 || other.x > x1 + 0.2 {
                                continue;
                            }
                            edge = edge.max(other.reach_down.unwrap_or(4.0));
                        }
                    }
                    for dy in &line.dynamics {
                        if dy.x1 > x0 - 0.2 && dy.x0 < x1 + 0.2 {
                            edge = edge.max(dy.bottom);
                        }
                    }
                    for prev in &line.texts {
                        if prev.dir > 0 && prev.x1 > x0 && prev.x0 < x1 {
                            edge = edge.max(prev.bottom);
                        }
                    }
                    edge + 0.45 - bx.0
                };
                line.texts.push(TextPlace { text: tx.text.clone(), x0, x1, y, top: y + bx.0, bottom: y + bx.1, dir: tx.dir });
            }
        }
    }
}

/// The line the syllables stand on: 0.6 under everything the staff has
/// below it — notes, stems, slurs, dynamics, words — and never nearer the
/// staff than 1.2 spaces from its bottom line to the top of the letters.
fn lyric_line(bars: &[Bar], line: &mut Line) {
    let has = line.bars.clone().any(|b| bars[b].items.iter().any(|ev| ev.lyric.is_some()));
    if !has {
        return;
    }
    let e = em(0.0);
    line.lyric_y = Some((reach(bars, line).1 + 0.6).max(5.2) + 0.72 * e);
}

fn voltas(bars: &[Bar], line: &Line) -> Vec<Volta> {
    let mut out: Vec<Volta> = Vec::new();
    let mut run: Option<Volta> = None;
    let last_b = line.bars.end - 1;
    for b in line.bars.clone() {
        let bar = &bars[b];
        let label = bar.volta_in.clone();
        if let (Some(l), None) = (&label, &run) {
            run = Some(Volta { label: l.clone(), first: bar.volta_first, x0: bar.x0, ..Volta::default() });
        }
        if let Some(r) = &mut run {
            r.bars.push(b);
            r.x1 = bar.x1 + barline_width(bar.drawn());
            r.closed = bar.volta_last && bar.volta_closed;
            if bar.volta_last || b == last_b || bars[b + 1].volta_in != label {
                out.extend(run.take());
            }
        }
    }
    for vb in &mut out {
        let mut low: f64 = -2.0;
        for &b in &vb.bars {
            for ev in &bars[b].items {
                if ev.k != Kind::Note {
                    continue;
                }
                low = low.min(y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.7);
                low = low.min(ev.stem_end - 0.3);
            }
        }
        vb.y = low - 1.0;
    }
    out
}

pub(crate) fn mark_width(mark: &Tempo) -> f64 {
    let e = em(0.0);
    let mut w = if mark.text.is_empty() { 0.0 } else { text_width(&mark.text, Weight::Bold, e) };
    if mark.bpm > 0 {
        if w > 0.0 {
            w += 0.6;
        }
        w += text_width(&format!("( = {})", mark.bpm), Weight::Regular, e) + 1.6;
    }
    w
}

/// How far above the top line (negative) and below it a staff's line's ink
/// goes.
pub(crate) fn reach(bars: &[Bar], line: &Line) -> (f64, f64) {
    let mut hi: f64 = 0.0;
    let mut lo: f64 = 4.0;
    let range = line.bars.clone();
    for b in range.clone() {
        for ev in &bars[b].items {
            if ev.k != Kind::Note {
                continue;
            }
            for hd in &ev.heads {
                let y = y_of(hd.pos);
                let pad = if hd.acc.is_none() { 0.6 } else { 1.8 };
                hi = hi.min(y - pad);
                lo = lo.max(y + pad);
            }
            hi = hi.min(ev.stem_end - 0.6);
            lo = lo.max(ev.stem_end + 0.6);
        }
    }
    for mk in &line.marks {
        hi = hi.min(mk.y - 1.8);
    }
    if bars[range.start].number > 1 {
        hi = hi.min(-3.0);
    }
    for vb in &line.voltas {
        hi = hi.min(vb.y - 0.4);
    }
    for tp in &line.tuplets {
        if tp.up {
            hi = hi.min(tp.ya.min(tp.yb) - 0.9);
        } else {
            lo = lo.max(tp.ya.max(tp.yb) + 0.9);
        }
    }
    for b in range.clone() {
        for ev in &bars[b].items {
            hi = hi.min(ev.reach_up.unwrap_or(0.0));
            lo = lo.max(ev.reach_down.unwrap_or(4.0));
        }
    }
    for dy in &line.dynamics {
        lo = lo.max(dy.bottom);
    }
    for tx in &line.texts {
        if tx.dir < 0 {
            hi = hi.min(tx.top);
        }
        if tx.dir > 0 {
            lo = lo.max(tx.bottom);
        }
    }
    for b in range {
        if bars[b].items.first().is_some_and(|ev| ev.mm.is_some()) {
            hi = hi.min(-2.6);
        }
    }
    if let Some(ly) = line.lyric_y {
        lo = lo.max(ly + 0.25 * em(0.0) + 0.6);
    }
    for c in &line.curves {
        for (_, y) in points(c) {
            hi = hi.min(y);
            lo = lo.max(y);
        }
    }
    (hi, lo)
}
