//! What goes on and around the notes once they are placed: ties and slurs,
//! tuplets, articulations, dynamics and hairpins, words, endings and tempo
//! marks, and how far all of it reaches (`eui_builders_score.sl`, "Ties and
//! slurs" and "Expression").

use crate::draw::{dot_start, past_flag};
use crate::engrave::{barline_width, clef_glyph, rest_name, stem_x, y_of, Line};
use crate::glyphs::{glyph, Weight};
use crate::measure::Bar;
use crate::parse::{Ev, Kind, Run, StaffKind, Tempo};
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

/// An articulation's glyph, centred on `x` — or a fingering's figures.
#[derive(Debug, Clone)]
pub struct ScriptPlace {
    /// The glyph.
    pub glyph: &'static str,
    /// Its centre.
    pub x: f64,
    /// Its origin.
    pub y: f64,
    /// A fingering's figures, drawn in the time signature's digits `k` their
    /// size, in place of `glyph`.
    pub figure: Option<String>,
    /// …
    pub k: f64,
    /// The figures are a string's number, in a circle.
    pub circled: bool,
}

/// A string number's circle: its radius, and its figure's size against
/// the time signature's digits.
pub(crate) const STRING_R: f64 = 0.75;
/// …
pub(crate) const STRING_K: f64 = 0.45;

/// A fingering's size against the time signature's digits: LilyPond's
/// `font-size` -5.
pub(crate) const FINGER_K: f64 = 0.5612310241546865;

/// `\ottava`'s bracket on one line.
#[derive(Debug, Clone)]
pub struct OttavaPlace {
    /// Its words: "8" or "15", as LilyPond prints them.
    pub text: String,
    /// Where the words start.
    pub x0: f64,
    /// Where the dashes end.
    pub x1: f64,
    /// The words' baseline.
    pub y: f64,
    /// Above the staff.
    pub up: bool,
    /// It ends on this line: a hook.
    pub hook: bool,
}

/// A rehearsal mark, a segno or a coda over a bar line.
#[derive(Debug, Clone)]
pub struct MarkPlace {
    /// What it says.
    pub runs: Vec<Run>,
    /// Its left.
    pub x: f64,
    /// Its baseline.
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
    /// In their faces.
    pub runs: Vec<Run>,
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
        "trill" => r("ornamentTrill", "ornamentTrill", 0.2, 2000, -1, false),
        "prall" => r("ornamentShortTrill", "ornamentShortTrill", 0.2, 0, -1, false),
        "mordent" => r("ornamentMordent", "ornamentMordent", 0.2, 0, -1, false),
        "turn" => r("ornamentTurn", "ornamentTurn", 0.2, 0, -1, false),
        "open" => r("brassMuteOpen", "brassMuteOpen", 0.2, 0, -1, false),
        "stopped" => r("brassMuteClosed", "brassMuteClosed", 0.2, 0, -1, false),
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
    let mut slurs: Vec<(At, At)> = Vec::new();
    for &at in &notes {
        let ev = ev_at(at);
        let mine = &mut open[usize::from(voice_of(ev) - 1)];
        if ev.slur_on {
            mine.push(at);
        }
        if ev.slur_off {
            if let Some(first) = mine.pop() {
                slur(lines, bars, &notes, first, at, 0.0);
                slurs.push((first, at));
            }
        }
    }
    // Phrasing slurs, `\( \)`: drawn as slurs are, and outside any slur
    // they share notes with.
    let mut open: [Vec<At>; 2] = [Vec::new(), Vec::new()];
    for &at in &notes {
        let ev = ev_at(at);
        let mine = &mut open[usize::from(voice_of(ev) - 1)];
        if ev.phrase_on {
            mine.push(at);
        }
        if ev.phrase_off {
            if let Some(first) = mine.pop() {
                let (a, b) = (ev_at(first).n.unwrap_or(0), ev.n.unwrap_or(0));
                let inside = slurs.iter().any(|&(f, l)| voice_of(ev_at(f)) == voice_of(ev) && ev_at(f).n.unwrap_or(0) <= b && ev_at(l).n.unwrap_or(0) >= a);
                slur(lines, bars, &notes, first, at, if inside { 1.2 } else { 0.0 });
            }
        }
    }
    for line in lines.iter_mut() {
        line.glissandi = Vec::new();
    }
    for (i, &at) in notes.iter().enumerate() {
        let ev = ev_at(at);
        if !ev.gliss {
            continue;
        }
        let Some(nxt) = notes.iter().skip(i + 1).map(|&a| ev_at(a)).find(|n| voice_of(n) == voice_of(ev)) else { continue };
        glissando(lines, ev, nxt);
    }
}

/// A glissando's line, from after a note's head to before the next one's —
/// head to head, the lowest of a chord to the lowest of the next.
fn glissando(lines: &mut [Line], ev: &Ev, nxt: &Ev) {
    let (Some(a), Some(b)) = (ev.heads.first(), nxt.heads.first()) else { return };
    let x0 = ev.x + ev.head_w.unwrap_or(0.0) + 0.3 + if ev.dots > 0 { 0.3 + 0.6 * ev.dots as f64 } else { 0.0 };
    let x1 = nxt.x - nxt.left - 0.3;
    let (y0, y1) = (y_of(a.pos), y_of(b.pos));
    if ev.sys == nxt.sys {
        lines[ev.sys].glissandi.push([x0, y0, x1, y1]);
        return;
    }
    let end = lines[ev.sys].bar_end - 0.5;
    let start = lines[nxt.sys].prefix - 0.6;
    let mid = (y0 + y1) / 2.0;
    lines[ev.sys].glissandi.push([x0, y0, end, mid]);
    lines[nxt.sys].glissandi.push([start, mid, x1, y1]);
}

fn tie(lines: &mut [Line], ev: &Ev, pos: i64, nxt: &Ev, other: i64, over: bool) {
    let dir = if over { -1.0 } else { 1.0 };
    let y = y_of(pos) + dir * 0.35;
    let y2 = y_of(other) + dir * 0.35;
    let mut xa = ev.x + ev.head_w.unwrap_or(0.0) + 0.15;
    if ev.dots > 0 && over {
        // A dot moved past its note's flag, out in the space above: a tie
        // over the note starts after it rather than arching over it.
        let dot_pos = if pos % 2 == 0 { pos + 1 } else { pos };
        if past_flag(ev, dot_pos) > 0.0 {
            xa = xa.max(ev.x + dot_start(ev, dot_pos) + 0.6 * (ev.dots - 1) as f64 + glyph("augmentationDot").advance + 0.15);
        }
    }
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

fn slur(lines: &mut [Line], bars: &[Bar], notes: &[At], first: At, last: At, extra: f64) {
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
    let (e0, e1) = ((e0.0, e0.1 + dir * extra), (e1.0, e1.1 + dir * extra));
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
    if let Some(cmds) = arc_cmds(xa, ya, xb, yb, dir, hgt) {
        line.curves.push(cmds);
    }
}

/// [`arc_h`]'s outline, or none for a curve with no width.
pub(crate) fn arc_cmds(xa: f64, ya: f64, xb: f64, yb: f64, dir: f64, hgt: f64) -> Option<Vec<f64>> {
    let w = xb - xa;
    if w <= 0.0 {
        return None;
    }
    let inset = w * 0.25;
    let c_out = (hgt + RULE.curve_mid / 2.0) * 4.0 / 3.0;
    let c_in = (hgt - RULE.curve_mid / 2.0) * 4.0 / 3.0;
    let t2 = RULE.curve_tip / 2.0;
    Some(vec![
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
    ])
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
        line.tuplets = tuplets(bars, line);
        scripts(bars, line);
        dynamics(bars, line);
        texts(bars, line);
        // After what stands above the staff: a bracket goes over the words,
        // the scripts and the tuplets under it, as LilyPond's
        // `outside-staff-priority` puts a VoltaBracket over TextScripts.
        line.voltas = if line.staff_index.unwrap_or(0) == 0 { voltas(bars, line) } else { Vec::new() };
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
        line.ottavas = ottavas(bars, line);
        line.rehearsal = if line.staff_index.unwrap_or(0) == 0 { rehearsal(bars, line) } else { Vec::new() };
    }
}

/// The size of an ottava's words, and of a rehearsal mark's.
pub(crate) fn ottava_em() -> f64 {
    em(-1.0)
}

/// The rehearsal mark's size: LilyPond's `font-size` 2.
pub(crate) fn mark_em() -> f64 {
    em(2.0)
}

/// `\ottava`'s brackets on a line: one a run of notes and rests at the same
/// octaves, from its first to its last, above the notes for an octave up and
/// under them for one down, a hook at the end of the run.
fn ottavas(bars: &[Bar], line: &Line) -> Vec<OttavaPlace> {
    let items: Vec<&Ev> = line.bars.clone().flat_map(|b| bars[b].items.iter()).filter(|ev| ev.percent.is_none()).collect();
    let mut out = Vec::new();
    let mut k = 0;
    while k < items.len() {
        let o = items[k].ottava;
        if o == 0 {
            k += 1;
            continue;
        }
        let mut j = k;
        while j + 1 < items.len() && items[j + 1].ottava == o {
            j += 1;
        }
        let first = items[k];
        let last = items[j];
        let x0 = first.x - first.left.min(0.5);
        let x1 = last.x + item_w(last) + 0.5;
        let up = o > 0;
        let text = if o.abs() == 1 { "8" } else { "15" };
        let e = ottava_em();
        let (bt, bb) = text_box(text, Weight::Bold, e);
        let near = |ev: &&&Ev| ev.x + item_w(ev) >= x0 - 0.2 && ev.x <= x1 + 0.2;
        let y = if up {
            let edge = items.iter().filter(near).map(|ev| ev.reach_up.unwrap_or(0.0)).fold(-0.5, f64::min);
            edge - 0.6 - bb
        } else {
            let edge = items.iter().filter(near).map(|ev| ev.reach_down.unwrap_or(4.0)).fold(4.5, f64::max);
            edge + 0.6 - bt
        };
        let hook = j + 1 < items.len() || line.bars.end >= bars.len() || bars[line.bars.end].items.first().is_none_or(|n| n.ottava != o);
        out.push(OttavaPlace { text: text.into(), x0, x1, y, up, hook });
        k = j + 1;
    }
    out
}

/// Where a line's rehearsal marks go: over the bar line they are written
/// at, at the start of the line over its first note's room, above all the
/// line holds there.
fn rehearsal(bars: &[Bar], line: &Line) -> Vec<MarkPlace> {
    let mut out = Vec::new();
    let e = mark_em();
    for (bi, b) in line.bars.clone().enumerate() {
        let bar = &bars[b];
        for (pos, words) in &bar.marks {
            let runs = words.pieces();
            let w = runs_width(&runs, e);
            let (_, foot) = runs_box(&runs, e);
            let x = if *pos >= bar.length && words.dir > 0 {
                // A jump ends at its bar line, as LilyPond right-aligns it.
                bar.x1 + barline_width(bar.drawn()) / 2.0 - w
            } else if *pos >= bar.length {
                bar.x1 + barline_width(bar.drawn()) / 2.0 - w / 2.0
            } else if *pos == 0 && bi == 0 {
                line.left
            } else if *pos == 0 {
                bar.x0 - barline_width(&bars[b - 1].bar) / 2.0 - w / 2.0
            } else {
                bar.items.iter().find(|ev| ev.start >= *pos).map_or(bar.x0, |ev| ev.x) - w / 2.0
            };
            let mut low: f64 = -1.5;
            for ob in line.bars.clone() {
                for other in &bars[ob].items {
                    if other.x + item_w(other) < x - 0.5 || other.x > x + w + 0.5 {
                        continue;
                    }
                    low = low.min(other.reach_up.unwrap_or(0.0));
                    if other.k == Kind::Note {
                        low = low.min(other.stem_end - 0.3);
                    }
                }
            }
            for mk in &line.marks {
                if mk.x < x + w + 0.5 && mk.x + mark_width(&mk.mark) > x - 0.5 {
                    low = low.min(mk.y - 1.8);
                }
            }
            for vb in &line.voltas {
                if vb.x0 < x + w && vb.x1 > x {
                    low = low.min(vb.y - 0.4);
                }
            }
            // Two marks at one bar line — a D.S. ending a bar, a letter
            // starting the next — stand one over the other.
            for placed in &out {
                let MarkPlace { runs: other, x: ox, y: oy } = placed;
                if *ox < x + w + 0.3 && *ox + runs_width(other, e) > x - 0.3 {
                    let (top, _) = runs_box(other, e);
                    low = low.min(oy + top - 0.4 + 0.8 + foot);
                }
            }
            // An ottava's figure just after the bar line still sits under it.
            for ot in &line.ottavas {
                if ot.up && ot.x0 < x + w + 1.0 && ot.x1 > x - 1.0 {
                    low = low.min(ot.y - 1.6);
                }
            }
            out.push(MarkPlace { runs, x, y: low - 0.8 - foot });
        }
    }
    out
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
    if let Some(y) = ev.rest_y {
        return y;
    }
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

/// How far a note's ink reaches up and down — its grace notes' with it,
/// so what stands over the staff (a tempo mark, words) clears them too.
fn item_extent(ev: &Ev) -> (f64, f64) {
    if ev.k != Kind::Note {
        return (1.0 + rest_lift(ev), 3.0 + rest_lift(ev));
    }
    let (mut up, mut down) = (tuplet_edge(ev, true), tuplet_edge(ev, false));
    for g in &ev.graces {
        for hd in &g.heads {
            up = up.min(y_of(hd.pos) - 0.4);
            down = down.max(y_of(hd.pos) + 0.4);
        }
        if g.base > 1 {
            up = up.min(g.stem_end);
            down = down.max(g.stem_end);
        }
    }
    (up, down)
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
            if ev.scripts.is_empty() && ev.fingers.is_empty() {
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
            // A fingering stacks with the articulations, nearer the note than
            // a fermata, farther than a staccato (LilyPond's priority 100),
            // above unless written under.
            // A chord's: the upper half's above it, the lower half's under
            // it, each nearest its own head.
            let mut heads: Vec<i64> = ev.heads.iter().map(|h| h.d).collect();
            heads.sort_unstable();
            let finger_dir = |f: &crate::parse::Finger| match (f.dir, f.head) {
                (0, Some(d)) if heads.len() > 1 => {
                    let rank = heads.iter().position(|&h| h == d).unwrap_or(0);
                    if rank * 2 >= heads.len() {
                        -1
                    } else {
                        1
                    }
                }
                (0, _) => -1,
                (dir, _) => dir,
            };
            let mut fingers: Vec<(&crate::parse::Finger, i64)> = ev.fingers.iter().map(|f| (f, finger_dir(f))).collect();
            fingers.sort_by_key(|(f, dir)| f.head.unwrap_or(0) * -dir);
            let mut list: Vec<(&str, i64, Option<&str>, bool)> = ev.scripts.iter().map(|sc| (sc.name.as_str(), sc.dir, None, false)).collect();
            list.extend(fingers.iter().map(|(f, dir)| ("", *dir, Some(f.text.as_str()), f.circled)));
            list.sort_by_key(|sc| if sc.2.is_some() { 100 } else { script_rule(sc.0).prio });
            for (name, sdir, figure, circled) in list {
                if let Some(figure) = figure {
                    let bb = if circled { (-STRING_R - 0.05, STRING_R + 0.05) } else { figure_box(figure) };
                    let y;
                    if sdir < 0 {
                        y = up_cursor - 0.5 - bb.1;
                        up_cursor = y + bb.0;
                    } else {
                        y = down_cursor + 0.5 - bb.0;
                        down_cursor = y + bb.1;
                    }
                    line.scripts.push(ScriptPlace { glyph: "", x: xc, y, figure: Some(figure.to_owned()), k: if circled { STRING_K } else { FINGER_K }, circled });
                    continue;
                }
                let rule = script_rule(name);
                let mut dir = sdir;
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
                line.scripts.push(ScriptPlace { glyph: name, x: xc, y, figure: None, k: 1.0, circled: false });
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
                let runs = tx.pieces();
                let w = runs_width(&runs, e);
                let bx = runs_box(&runs, e);
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
                line.texts.push(TextPlace { text: tx.text.clone(), runs, x0, x1, y, top: y + bx.0, bottom: y + bx.1, dir: tx.dir });
            }
        }
    }
}

/// The line the syllables stand on: 0.6 under everything the staff has
/// below it — notes, stems, slurs, dynamics, words — and never nearer the
/// staff than 1.2 spaces from its bottom line to the top of the letters.
fn lyric_line(bars: &[Bar], line: &mut Line) {
    let items = || line.bars.clone().flat_map(|b| bars[b].items.iter());
    let has = items().any(|ev| ev.lyric.is_some() || ev.verses.iter().any(Option::is_some));
    if !has {
        return;
    }
    let e = em(0.0);
    line.lyric_y = Some((reach(bars, line).1 + 0.6).max(5.2) + 0.72 * e);
    line.lyric_lines = 1 + items().map(|ev| ev.verses.len()).max().unwrap_or(0);
}

/// From one verse's baseline to the next's, as LilyPond stacks a staff's
/// Lyrics contexts.
pub(crate) fn verse_gap() -> f64 {
    1.35 * em(0.0)
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
        // Its label, "1.", where draw_volta writes it: what stands under the
        // label is cleared by the label's height, not by the line's alone.
        let lx0 = vb.x0 + 0.5;
        let lx1 = if vb.first { lx0 + 0.3 + text_width(&vb.label, Weight::Bold, em(-2.0)) + 0.3 } else { lx0 };
        let mut low: f64 = -2.0;
        let mut label_low: f64 = 0.0;
        for &b in &vb.bars {
            for ev in &bars[b].items {
                if ev.k != Kind::Note {
                    continue;
                }
                let top = (y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.7)
                    .min(ev.stem_end - 0.3)
                    // An articulation above the note, a fermata: as its stem.
                    .min(ev.reach_up.unwrap_or(0.0) - 0.3);
                low = low.min(top);
                if ev.x <= lx1 && ev.x + ev.head_w.unwrap_or(1.2) >= lx0 {
                    label_low = label_low.min(top);
                }
            }
        }
        // What else stands above the staff under the bracket: its label,
        // "1.", hangs 1.8 spaces below the line, so the line clears it all
        // by that and a little more.
        let (x0, x1) = (vb.x0, vb.x1);
        let under = |x: f64| x >= x0 - 0.5 && x <= x1 + 0.5;
        let mut above: f64 = 0.0;
        for tx in &line.texts {
            if tx.dir < 0 && tx.x1 >= x0 && tx.x0 <= x1 {
                above = above.min(tx.top - 0.3);
            }
        }
        for tp in &line.tuplets {
            if tp.up && tp.x1 >= x0 && tp.x0 <= x1 {
                above = above.min(tp.ya.min(tp.yb) - 1.0);
            }
        }
        for c in &line.curves {
            for (px, py) in points(c) {
                if under(px) {
                    above = above.min(py - 0.2);
                }
            }
        }
        vb.y = (low - 1.0).min(label_low - 1.7).min(above - 2.0);
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
    match line.kind {
        StaffKind::ChordNames => {
            let base = crate::engrave::CHORD_BASE;
            let (mut hi, mut lo) = (base - 1.0, base);
            for b in line.bars.clone() {
                for ev in &bars[b].items {
                    let (t, bo) = runs_box(&ev.chord_name, crate::engrave::chord_em());
                    hi = hi.min(base + t);
                    lo = lo.max(base + bo);
                }
            }
            return (hi - 0.2, lo + 0.2);
        }
        StaffKind::Tab => return (-1.0, 8.5),
        _ => {}
    }
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
    for b in range.clone() {
        for ev in &bars[b].items {
            for g in &ev.graces {
                for hd in &g.heads {
                    hi = hi.min(y_of(hd.pos) - 0.6);
                    lo = lo.max(y_of(hd.pos) + 0.6);
                }
                hi = hi.min(g.stem_end - 0.4);
                lo = lo.max(g.stem_end + 0.4);
            }
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
        lo = lo.max(ly + verse_gap() * line.lyric_lines.saturating_sub(1) as f64 + 0.25 * em(0.0) + 0.6);
    }
    for ot in &line.ottavas {
        let (t, b) = text_box(&ot.text, Weight::Bold, ottava_em());
        hi = hi.min(ot.y + t - 0.2);
        lo = lo.max(ot.y + b + 0.2);
    }
    for mk in &line.rehearsal {
        let (t, _) = runs_box(&mk.runs, mark_em());
        hi = hi.min(mk.y + t - 0.3);
    }
    for g in &line.glissandi {
        hi = hi.min(g[1].min(g[3]));
        lo = lo.max(g[1].max(g[3]));
    }
    for c in &line.curves {
        for (_, y) in points(c) {
            hi = hi.min(y);
            lo = lo.max(y);
        }
    }
    (hi, lo)
}

/// A fingering's ink above and below its origin, `(top, bottom)`.
pub(crate) fn figure_box(figure: &str) -> (f64, f64) {
    let mut top: f64 = 0.0;
    let mut bottom: f64 = 0.0;
    for ch in figure.chars() {
        let g = glyph(&format!("timeSig{ch}"));
        top = top.min(g.bbox[1] * FINGER_K);
        bottom = bottom.max(g.bbox[3] * FINGER_K);
    }
    (top, bottom)
}

/// A fingering's width.
pub(crate) fn figure_width(figure: &str) -> f64 {
    figure.chars().map(|ch| glyph(&format!("timeSig{ch}")).advance * FINGER_K).sum()
}

/// A superscript's size against its words', and how far it is raised, in
/// their ems.
pub(crate) const SUP: f64 = 0.7;
/// …
pub(crate) const RAISE: f64 = 0.45;

/// A piece's size and how far above the baseline it stands.
pub(crate) fn run_em(r: &Run, e: f64) -> (f64, f64) {
    if r.sup {
        (e * SUP, e * RAISE)
    } else {
        (e, 0.0)
    }
}

/// How wide pieces of markup are, a word space before each that asks.
pub(crate) fn runs_width(runs: &[Run], e: f64) -> f64 {
    let space = text_width(" ", Weight::Regular, e);
    let mut w = 0.0;
    for (k, r) in runs.iter().enumerate() {
        let (re, _) = run_em(r, e);
        if k > 0 && r.gap {
            w += space * re / e;
        }
        w += if r.glyph {
            glyph_run_w(r)
        } else if r.text == TRIANGLE {
            0.62 * re
        } else {
            text_width(&r.text, r.weight, re)
        };
    }
    w
}

/// The major seventh's triangle, which the text face has not got: drawn as
/// LilyPond's markup draws it, an outline.
pub(crate) const TRIANGLE: &str = "△";

/// A glyph of markup's width.
pub(crate) fn glyph_run_w(r: &Run) -> f64 {
    let g = glyph(&r.text);
    (g.bbox[2] - g.bbox[0]) * r.size
}

/// Pieces of markup's ink above and below their baseline, `(top, bottom)`;
/// a glyph stands on the baseline.
pub(crate) fn runs_box(runs: &[Run], e: f64) -> (f64, f64) {
    let mut top: f64 = 0.0;
    let mut bottom: f64 = 0.0;
    for r in runs {
        let (re, rise) = run_em(r, e);
        let (t, b) = if r.glyph {
            let g = glyph(&r.text);
            (-(g.bbox[3] - g.bbox[1]) * r.size, 0.0)
        } else if r.text == TRIANGLE {
            (-0.55 * re, 0.0)
        } else {
            text_box(&r.text, r.weight, re)
        };
        top = top.min(t - rise);
        bottom = bottom.max(b - rise);
    }
    (top, bottom)
}
