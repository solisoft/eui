//! Notes written on one staff and drawn on another — `\change Staff`,
//! `\autoChange`, `\crossStaff` — and what joins them across the two
//! staves of a system: a beam between the staves, its stems reaching to it
//! from each side (a kneed beam); slurs, phrasing slurs and ties from a
//! note on one staff to a note on the other; `\showStaffSwitch`'s line
//! following the voice across; one stem for a chord over two staves.
//!
//! The notes move before anything is laid out ([`plan`]): each goes to the
//! same moment of the same bar on the other staff, its heads placed by that
//! staff's clef, a spacer (or, for `\autoChange`, a rest) left in its place.
//! What joins the two staves is noted then, while the voice is still in one
//! piece, and taken off the notes so that neither staff draws half of it.
//! It is drawn once the staves are stacked ([`ink`]), as LilyPond sets its
//! cross-staff grobs after the vertical spacing: it does not push the staves
//! apart, it spans the room between them.

use std::collections::HashMap;

use crate::engrave::{bar_windows, beam_voice, flag_name, flags, head_name, stem_x, voice_lists, y_of, System};
use crate::glyphs::glyph;
use crate::marks::{arc_cmds, curve_height, voice_of};
use crate::measure::{clef_mid, key_alters, Bar};
use crate::parse::{modulo, refuse, Ev, Kind, Staff};
use crate::{ScoreError, RULE};

/// A note: its staff, its bar, its index among the bar's items.
pub(crate) type At = (usize, usize, usize);

/// What a curve between two staves is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CurveKind {
    /// `~`.
    Tie,
    /// `( )`.
    Slur,
    /// `\( \)`.
    Phrase,
}

/// What crosses from one staff to the other, by where its notes are drawn.
#[derive(Debug, Clone, Default)]
pub(crate) struct Plan {
    /// Kneed beams: their notes, in order.
    pub beams: Vec<Vec<At>>,
    /// Ties and slurs from a note on one staff to a note on the other.
    pub curves: Vec<(CurveKind, At, At)>,
    /// Each curve's notes, its ends with them, wherever they are drawn.
    pub under: Vec<Vec<At>>,
    /// `\showStaffSwitch`: from the note before a switch to the note after.
    pub follows: Vec<(At, At)>,
    /// `\crossStaff`: a note whose stem joins its partner's on the next
    /// staff, and that partner (resolved by [`directions`]).
    pub spans: Vec<(At, Option<At>)>,
}

impl Plan {
    fn is_empty(&self) -> bool {
        self.beams.is_empty() && self.curves.is_empty() && self.follows.is_empty() && self.spans.is_empty()
    }
}

/// A note as written: its staff, bar and index there before any moved.
type Home = (usize, usize, usize);

/// Move every note sent to another staff there, and note what will join
/// the staves. A name no staff has is refused.
pub(crate) fn plan(all: &mut [Vec<Bar>], staves: &[Staff]) -> Result<Plan, ScoreError> {
    // Where each note sent away goes.
    let mut dest: HashMap<Home, usize> = HashMap::new();
    let mut spanned = false;
    for (a, bars) in all.iter().enumerate() {
        for (b, bar) in bars.iter().enumerate() {
            for (j, item) in bar.items.iter().enumerate() {
                spanned |= item.span_stem && item.k == Kind::Note;
                let Some(name) = item.cross.as_ref().filter(|_| item.k == Kind::Note) else { continue };
                let Some(t) = staves.iter().position(|s| s.name.as_deref() == Some(name.as_str())) else {
                    return refuse(format!("\\change Staff = \"{name}\" names no staff: a \\new Staff = \"{name}\" is what it goes to"), item.at);
                };
                if t != a {
                    dest.insert((a, b, j), t);
                }
            }
        }
    }
    let mut plan = Plan::default();
    if dest.is_empty() && !spanned {
        for bars in all.iter_mut() {
            for bar in bars.iter_mut() {
                for ev in &mut bar.items {
                    ev.cross = None;
                }
            }
        }
        return Ok(plan);
    }
    let to = |h: Home| dest.get(&h).copied().unwrap_or(h.0);
    // The pitches each note is tied into from the note before it in its
    // voice: what its accidentals leave out wherever it ends up.
    let tied_in = tied_into(all);

    // Beams over notes that end on both staves: one beam, drawn between them.
    let mut kneed: Vec<Vec<Home>> = Vec::new();
    for (a, bars) in all.iter().enumerate() {
        for (b, bar) in bars.iter().enumerate() {
            if !bar.items.iter().enumerate().any(|(j, _)| dest.contains_key(&(a, b, j))) {
                continue;
            }
            let wins = bar_windows(bar);
            let groups = bar.items.first().and_then(|ev| crate::look::beam_groups(&ev.look, bar.metre()));
            for list in voice_lists(bar) {
                for g in beam_voice(bar, &list, wins, groups.as_deref())? {
                    let first = to((a, b, g[0]));
                    if g.iter().any(|&j| to((a, b, j)) != first) {
                        kneed.push(g.iter().map(|&j| (a, b, j)).collect());
                    }
                }
            }
        }
    }
    for g in &kneed {
        for &(a, b, j) in g {
            all[a][b].items[j].xbeam = true;
        }
    }

    // Each voice's notes in order, as written: what ties and slurs join.
    let mut curves: Vec<(CurveKind, Home, Home)> = Vec::new();
    let mut under: Vec<Vec<Home>> = Vec::new();
    let mut follows: Vec<(Home, Home)> = Vec::new();
    let mut spans: Vec<Home> = Vec::new();
    for a in 0..all.len() {
        for v in 1u8..=4 {
            let seq: Vec<Home> = (0..all[a].len()).flat_map(|b| all[a][b].items.iter().enumerate().filter(|(_, ev)| ev.k == Kind::Note && voice_of(ev) == v).map(move |(j, _)| (a, b, j))).collect();
            if seq.is_empty() {
                continue;
            }
            let ev = |h: Home| &all[h.0][h.1].items[h.2];
            for k in 0..seq.len() {
                let h = seq[k];
                if ev(h).span_stem {
                    spans.push(h);
                }
                if let Some(&nxt) = seq.get(k + 1) {
                    if ev(h).tie && to(h) != to(nxt) {
                        curves.push((CurveKind::Tie, h, nxt));
                        under.push(vec![h, nxt]);
                    }
                }
                if k > 0 && ev(h).follow && to(h) != to(seq[k - 1]) {
                    follows.push((seq[k - 1], h));
                }
            }
            for kind in [CurveKind::Slur, CurveKind::Phrase] {
                let mut open: Vec<usize> = Vec::new();
                for (k, &h) in seq.iter().enumerate() {
                    let (on, off) = match kind {
                        CurveKind::Phrase => (ev(h).phrase_on, ev(h).phrase_off),
                        _ => (ev(h).slur_on, ev(h).slur_off),
                    };
                    if off {
                        if let Some(first) = open.pop() {
                            if to(seq[first]) != to(h) {
                                curves.push((kind, seq[first], h));
                                under.push(seq[first..=k].to_vec());
                            }
                        }
                    }
                    if on {
                        open.push(k);
                    }
                }
            }
        }
    }
    // Neither staff draws half of what the system draws whole.
    for &(kind, f, l) in &curves {
        match kind {
            CurveKind::Tie => all[f.0][f.1].items[f.2].tie = false,
            CurveKind::Slur => {
                all[f.0][f.1].items[f.2].slur_on = false;
                all[l.0][l.1].items[l.2].slur_off = false;
            }
            CurveKind::Phrase => {
                all[f.0][f.1].items[f.2].phrase_on = false;
                all[l.0][l.1].items[l.2].phrase_off = false;
            }
        }
    }

    // Who each note is, to find it again once moved.
    let ident: HashMap<Home, (usize, i64)> = dest.keys().map(|&h| (h, (all[h.0][h.1].items[h.2].i, all[h.0][h.1].items[h.2].start))).collect();
    let mut touched: Vec<(usize, usize)> = Vec::new();
    let mut moves: Vec<(&Home, &usize)> = dest.iter().collect();
    moves.sort();
    let moves_from: Vec<(usize, usize)> = moves.iter().map(|m| (m.0 .0, m.0 .1)).collect();
    for (&(a, b, j), &t) in moves {
        let item = &all[a][b].items[j];
        let mut moved = item.clone();
        let lift = clef_mid(all[a][b].clef_at(item.start).0) - clef_mid(all[t][b].clef_at(item.start).0);
        for h in &mut moved.heads {
            h.pos += lift;
        }
        moved.voice = Some(if t > a { 3 } else { 4 });
        moved.cross = None;
        moved.moved = Some(a);
        (moved.clef_change, moved.key_change) = (None, None);
        let ev = &mut all[a][b].items[j];
        ev.k = Kind::Rest;
        ev.spacer = !ev.rest_behind;
        ev.heads.clear();
        ev.cross = None;
        ev.xbeam = false;
        ev.follow = false;
        ev.span_stem = false;
        ev.tie = false;
        (ev.slur_on, ev.slur_off, ev.phrase_on, ev.phrase_off) = (false, false, false, false);
        if !ev.spacer {
            // A rest where the other hand plays: no marks of the note's.
            (ev.scripts, ev.texts, ev.dynamic, ev.fingers) = (Vec::new(), Vec::new(), None, Vec::new());
            ev.graces.clear();
            ev.after_graces.clear();
        }
        all[t][b].items.push(moved);
        all[t][b].poly = true;
        touched.push((t, b));
    }
    for bars in all.iter_mut() {
        for bar in bars.iter_mut() {
            for ev in &mut bar.items {
                ev.cross = None;
            }
        }
    }
    touched.sort_unstable();
    touched.dedup();
    // The accidentals of the bars notes left and reached, counted again on
    // the staff each note is drawn on (score #411).
    let mut recounted: Vec<(usize, usize)> = moves_from.iter().copied().chain(touched.iter().copied()).collect();
    recounted.sort_unstable();
    recounted.dedup();
    for &(s, b) in &recounted {
        recount(&mut all[s][b], b, &tied_in);
    }
    for &(t, b) in &touched {
        let bar = &mut all[t][b];
        bar.items.sort_by_key(|ev| ev.start * 8 + i64::from(ev.voice.unwrap_or(1)));
        // A note alone on the staff it went to stems as its place there
        // says; one that sounds with that staff's own notes keeps clear of
        // them, stem up under them (a third voice), down over them.
        let own: Vec<(i64, i64)> = bar.items.iter().filter(|e| e.k == Kind::Note && e.moved.is_none()).map(|e| (e.start, e.start + e.ticks)).collect();
        for ev in bar.items.iter_mut().filter(|e| e.moved.is_some()) {
            ev.free = !own.iter().any(|&(s, e)| s < ev.start + ev.ticks && e > ev.start);
        }
    }

    let find = |h: Home| -> Option<At> {
        let t = to(h);
        if t == h.0 {
            return Some(h);
        }
        let &(i, start) = ident.get(&h)?;
        all[t][h.1].items.iter().position(|e| e.moved == Some(h.0) && e.i == i && e.start == start).map(|j| (t, h.1, j))
    };
    plan.beams = kneed.iter().filter_map(|g| g.iter().map(|&h| find(h)).collect::<Option<Vec<At>>>()).collect();
    for (&(k, f, l), notes) in curves.iter().zip(&under) {
        if let (Some(f), Some(l)) = (find(f), find(l)) {
            plan.curves.push((k, f, l));
            plan.under.push(notes.iter().filter_map(|&h| find(h)).collect());
        }
    }
    plan.follows = follows.iter().filter_map(|&(f, l)| Some((find(f)?, find(l)?))).collect();
    plan.spans = spans.iter().filter_map(|&h| Some((find(h)?, None))).collect();
    Ok(plan)
}

/// A note by who it is: its number and its bar, and where it starts there.
type Who = (usize, usize, i64);

/// For each note tied into from the note before it in its voice, the
/// pitches tied: `(diatonic number, alteration)`.
fn tied_into(all: &[Vec<Bar>]) -> HashMap<Who, Vec<(i64, i64)>> {
    let mut out = HashMap::new();
    for bars in all {
        let mut last: HashMap<u8, (i64, Vec<(i64, i64)>)> = HashMap::new();
        for (b, bar) in bars.iter().enumerate() {
            for ev in bar.items.iter().filter(|e| matches!(e.k, Kind::Note | Kind::Rest) && !e.spacer) {
                let v = voice_of(ev);
                let at = bar.tick0 + ev.start;
                if ev.k == Kind::Note {
                    if let Some((ends, pitches)) = last.get(&v) {
                        if *ends == at {
                            out.insert((ev.i, b, ev.start), pitches.clone());
                        }
                    }
                }
                let tied: Vec<(i64, i64)> = if ev.k == Kind::Note && ev.tie { ev.pitches.iter().map(|p| (p.d, p.alter)).collect() } else { Vec::new() };
                if tied.is_empty() {
                    last.remove(&v);
                } else {
                    last.insert(v, (at + ev.ticks, tied));
                }
            }
        }
    }
    out
}

/// A bar's accidentals counted again, once notes have come to it from
/// another staff or left it for one: as LilyPond's Accidental_engraver
/// counts them on the staff a note is drawn on, every voice there together
/// — an alteration shown in the bar is not shown again on its line or space,
/// one the bar has not shown yet is. The notes starting together are read
/// against the bar as it stood before them; grace notes before their note,
/// `\afterGrace`'s after it; two at once on one line, altered
/// differently, each show theirs.
fn recount(bar: &mut Bar, b: usize, tied_in: &HashMap<Who, Vec<(i64, i64)>>) {
    let mut seen: HashMap<i64, i64> = HashMap::new();
    let mut order: Vec<usize> = (0..bar.items.len()).filter(|&j| !bar.items[j].spacer && (bar.items[j].k == Kind::Note || !bar.items[j].graces.is_empty())).collect();
    order.sort_by_key(|&j| bar.items[j].start);
    let mut fifths = None;
    let mut k = 0;
    while k < order.len() {
        let start = bar.items[order[k]].start;
        let same: Vec<usize> = order[k..].iter().copied().take_while(|&j| bar.items[j].start == start).collect();
        k += same.len();
        let key = bar.clef_at(start).1;
        if fifths.is_some_and(|f| f != key) {
            // A new key starts the bar's accidentals afresh.
            seen.clear();
        }
        fifths = Some(key);
        let alters = key_alters(key);
        let now = |seen: &HashMap<i64, i64>, d: i64| seen.get(&d).copied().unwrap_or(alters[modulo(d, 7) as usize]);
        // Grace notes first, each against what came before it.
        for &j in &same {
            for g in &mut bar.items[j].graces {
                for h in &mut g.heads {
                    let paren = g.cautionary.contains(&h.d);
                    let show = h.alter != now(&seen, h.d) || paren || g.forced.contains(&h.d);
                    seen.insert(h.d, h.alter);
                    h.acc = show.then_some(h.alter);
                }
            }
        }
        let before = seen.clone();
        // Two notes on one line or space at once, altered differently: each
        // shows its accidental, as LilyPond's engraver forces them.
        let at_once: Vec<(i64, i64)> = same.iter().filter(|&&j| bar.items[j].k == Kind::Note).flat_map(|&j| bar.items[j].heads.iter().map(|h| (h.d, h.alter))).collect();
        for &j in &same {
            let ev = &mut bar.items[j];
            if ev.k != Kind::Note {
                continue;
            }
            let tied = tied_in.get(&(ev.i, b, ev.start));
            for h in &mut ev.heads {
                let mut show = h.alter != now(&before, h.d) || at_once.iter().any(|o| o.0 == h.d && o.1 != h.alter);
                if tied.is_some_and(|t| t.contains(&(h.d, h.alter))) {
                    show = false;
                }
                let paren = ev.cautionary.contains(&h.d);
                show = show || paren || ev.forced.contains(&h.d);
                seen.insert(h.d, h.alter);
                h.acc = show.then_some(h.alter);
            }
        }
        for &j in &same {
            for g in &mut bar.items[j].after_graces {
                for h in &mut g.heads {
                    let paren = g.cautionary.contains(&h.d);
                    let show = h.alter != now(&seen, h.d) || paren || g.forced.contains(&h.d);
                    seen.insert(h.d, h.alter);
                    h.acc = show.then_some(h.alter);
                }
            }
        }
    }
}

/// The stems' directions the staves join by, once each staff has beamed
/// its own notes: under a kneed beam, the upper staff's notes stem down
/// to it and the lower staff's up; a `\crossStaff` note and its partner on
/// the staff its stem points to share one direction.
pub(crate) fn directions(all: &mut [Vec<Bar>], plan: &mut Plan) {
    for g in &plan.beams {
        let low = g.iter().map(|m| m.0).max().unwrap_or(0);
        for &(s, b, j) in g {
            all[s][b].items[j].up = s == low;
        }
    }
    for (n, partner) in &mut plan.spans {
        let ev = &all[n.0][n.1].items[n.2];
        let up = ev.up;
        let start = ev.start;
        let other = if up { n.0.checked_sub(1) } else { Some(n.0 + 1).filter(|&s| s < all.len()) };
        *partner = other.and_then(|m| all[m][n.1].items.iter().position(|e| e.k == Kind::Note && e.start == start && !e.span_stem && !e.heads.is_empty()).map(|j| (m, n.1, j)));
        match *partner {
            Some((m, b, j)) => {
                let p = &mut all[m][b].items[j];
                if p.beam.is_none() && !p.xbeam {
                    p.up = up;
                }
            }
            None => all[n.0][n.1].items[n.2].span_stem = false,
        }
    }
    plan.spans.retain(|s| s.1.is_some());
}

/// A fill's commands moved by `(dx, dy)`.
fn shifted(cmds: &[f64], dx: f64, dy: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(cmds.len());
    let mut i = 0;
    while i < cmds.len() {
        let args = crate::text::args_of(cmds[i]);
        out.push(cmds[i]);
        for k in 0..args {
            out.push(cmds.get(i + 1 + k).copied().unwrap_or(0.0) + if k % 2 == 0 { dx } else { dy });
        }
        i += 1 + args;
    }
    out
}

/// A rectangle as a fill's commands.
fn rect(x: f64, y: f64, w: f64, h: f64) -> Vec<f64> {
    vec![0.0, x, y, 1.0, x + w, y, 1.0, x + w, y + h, 1.0, x, y + h, 4.0]
}

/// A straight line `thick` wide from `(x0, y0)` to `(x1, y1)`.
fn segment(x0: f64, y0: f64, x1: f64, y1: f64, thick: f64) -> Vec<f64> {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    let (nx, ny) = (-dy / len * thick / 2.0, dx / len * thick / 2.0);
    vec![0.0, x0 + nx, y0 + ny, 1.0, x1 + nx, y1 + ny, 1.0, x1 - nx, y1 - ny, 1.0, x0 - nx, y0 - ny, 4.0]
}

/// A curve from `(xa, ya)` to `(xb, yb)` bowed `hgt` away from the chord
/// between them on the side `dir` (-1 up, 1 down), thick as a tie or a slur
/// is (`RULE.curve_tip` at its ends, `curve_mid` between) across the curve:
/// a cross-staff curve falls steeply, and a thickness taken straight up and
/// down would thin it to a hair.
fn bow(xa: f64, ya: f64, xb: f64, yb: f64, dir: f64, hgt: f64) -> Option<Vec<f64>> {
    let (dx, dy) = (xb - xa, yb - ya);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.2 || dx <= 0.0 {
        return None;
    }
    let (ux, uy) = (dx / len, dy / len);
    // The normal on the side the curve bows to.
    let (mut nx, mut ny) = (-uy, ux);
    if ny * dir < 0.0 {
        (nx, ny) = (-nx, -ny);
    }
    let inset = len * 0.25;
    let c_out = (hgt + RULE.curve_mid / 2.0) * 4.0 / 3.0;
    let c_in = (hgt - RULE.curve_mid / 2.0) * 4.0 / 3.0;
    let t2 = RULE.curve_tip / 2.0;
    let at = |along: f64, off: f64| [xa + ux * along + nx * off, ya + uy * along + ny * off];
    let p = [at(0.0, t2), at(inset, c_out), at(len - inset, c_out), at(len, t2), at(len, -t2), at(len - inset, c_in), at(inset, c_in), at(0.0, -t2)];
    Some(vec![0.0, p[0][0], p[0][1], 3.0, p[1][0], p[1][1], p[2][0], p[2][1], p[3][0], p[3][1], 1.0, p[4][0], p[4][1], 3.0, p[5][0], p[5][1], p[6][0], p[6][1], p[7][0], p[7][1], 4.0])
}

/// A kneed beam once placed, in its system's frame (y down from the first
/// staff's top line): its primary beam's middle at its first and last stem.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KneedBeam {
    /// The first stem's x.
    pub x0: f64,
    /// The last's.
    pub x1: f64,
    /// The primary beam's middle at `x0`.
    pub y0: f64,
    /// … at `x1`.
    pub y1: f64,
    /// Each note's stem: its staff, from where to where (y down).
    pub stems: Vec<(usize, f64, f64)>,
}

/// Draw what crosses between the staves, each system's in its own frame.
pub(crate) fn ink(systems: &mut [System], all: &[Vec<Bar>], plan: &Plan) {
    if plan.is_empty() {
        return;
    }
    let sys_of = |b: usize| systems.iter().position(|s| s.lines.first().is_some_and(|l| l.bars.contains(&b)));
    let ev = |m: At| &all[m.0][m.1].items[m.2];
    let mut out: Vec<(usize, Vec<f64>)> = Vec::new();
    let mut placed: Vec<(usize, KneedBeam)> = Vec::new();
    for g in &plan.beams {
        let Some(si) = sys_of(g[0].1) else { continue };
        let offs = &systems[si].offsets;
        let scales: Vec<f64> = systems[si].lines.iter().map(|l| l.scale.unwrap_or(1.0)).collect();
        if let Some((beam, ink)) = kneed(g, all, offs, &scales) {
            out.extend(ink.into_iter().map(|c| (si, c)));
            placed.push((si, beam));
        }
    }
    for (ci, &(kind, f, l)) in plan.curves.iter().enumerate() {
        let (Some(sf), Some(sl)) = (sys_of(f.1), sys_of(l.1)) else { continue };
        let between = plan.under.get(ci).map_or(&[][..], Vec::as_slice);
        let (a, z) = (ev(f), ev(l));
        let (of, ol) = (systems[sf].offsets[f.0], systems[sl].offsets[l.0]);
        let pieces: Vec<(f64, f64, f64, f64, f64)> = match kind {
            CurveKind::Tie => {
                let mut v = Vec::new();
                let count = a.heads.len();
                for (hi, hd) in a.heads.iter().enumerate() {
                    let Some(other) = z.heads.iter().find(|o| o.d == hd.d) else { continue };
                    let mut over = if count == 1 { !a.up } else { hi >= count / 2 };
                    if let Some(vo) = a.voice.filter(|_| !a.free && a.moved.is_none()) {
                        over = vo % 2 == 1;
                    }
                    let dir = if over { -1.0 } else { 1.0 };
                    v.push((a.x + a.head_w.unwrap_or(0.0) + 0.15, of + y_of(hd.pos) + dir * 0.35, z.x - 0.15, ol + y_of(other.pos) + dir * 0.35, dir));
                }
                v
            }
            _ => {
                // Over unless every note under it stems up, as a slur in one staff.
                let over = !between.iter().all(|&m| ev(m).up);
                let dir = if over { -1.0 } else { 1.0 };
                let extra = if kind == CurveKind::Phrase { 0.6 } else { 0.0 };
                let (xa, ya) = slur_end(a, over);
                let (xb, yb) = slur_end(z, over);
                vec![(xa, of + ya + dir * extra, xb, ol + yb + dir * extra, dir)]
            }
        };
        let (limit, ratio) = if kind == CurveKind::Tie { (1.0, 0.333) } else { (2.0, 0.25) };
        for (xa, ya, xb, yb, dir) in pieces {
            if sf == sl {
                let len = ((xb - xa).powi(2) + (yb - ya).powi(2)).sqrt();
                let mut hgt = curve_height(len.max(0.1), limit, ratio);
                let (mut ya, mut yb) = (ya, yb);
                if kind != CurveKind::Tie {
                    // Clear of the notes under it: rounder, then lifted, as a slur in one staff.
                    let ny = ((xb - xa) / len.max(1e-9)).abs();
                    let mut lift: f64 = 0.0;
                    for &m in &between[1..between.len().saturating_sub(1)] {
                        let e = ev(m);
                        let Some(so) = sys_of(m.1).filter(|&x| x == sf) else { continue };
                        let (x, y) = slur_end(e, dir < 0.0);
                        let y = systems[so].offsets[m.0] + y;
                        let u = (x - xa) / (xb - xa);
                        if u <= 0.0 || u >= 1.0 {
                            continue;
                        }
                        let need = (ya + (yb - ya) * u - y) * -dir;
                        let bulge = 4.0 * u * (1.0 - u) * ny;
                        if need > hgt * bulge {
                            let want = need / bulge.max(0.05);
                            if want <= 4.0 {
                                hgt = want;
                            } else {
                                hgt = 4.0;
                                lift = lift.max(need - 4.0 * bulge);
                            }
                        }
                    }
                    ya += dir * lift;
                    yb += dir * lift;
                }
                if let Some(c) = bow(xa, ya, xb, yb, dir, hgt) {
                    out.push((sf, c));
                }
                continue;
            }
            // Over a line's end: to the end of the first, from the start of the next.
            let end = systems[sf].lines[0].bar_end - 0.5;
            let start = systems[sl].lines[0].prefix - 0.6;
            if let Some(c) = arc_cmds(xa, ya, end, ya, dir, curve_height((end - xa).max(0.1), limit, ratio)) {
                out.push((sf, c));
            }
            if let Some(c) = arc_cmds(start, yb, xb, yb, dir, curve_height((xb - start).max(0.1), limit, ratio)) {
                out.push((sl, c));
            }
        }
    }
    for &(f, l) in &plan.follows {
        let (Some(sf), Some(sl)) = (sys_of(f.1), sys_of(l.1)) else { continue };
        if sf != sl {
            continue;
        }
        let (a, z) = (ev(f), ev(l));
        let offs = &systems[sf].offsets;
        let mid = |e: &Ev| (y_of(e.heads.first().map_or(0, |h| h.pos)) + y_of(e.heads.last().map_or(0, |h| h.pos))) / 2.0;
        let xa = a.x + a.head_w.unwrap_or(0.0) + 0.5 + if a.dots > 0 { 0.3 + 0.6 * a.dots as f64 } else { 0.0 };
        let xb = z.x - z.left.max(0.0) - 0.5;
        if xb > xa {
            out.push((sf, segment(xa, offs[f.0] + mid(a), xb, offs[l.0] + mid(z), 0.1)));
        }
    }
    for &(n, partner) in &plan.spans {
        let Some(p) = partner else { continue };
        let Some(si) = sys_of(n.1) else { continue };
        let offs = &systems[si].offsets;
        let (a, z) = (ev(n), ev(p));
        // From the note's root to its partner's, along the partner's stem.
        let root = |e: &Ev, s: usize| offs[s] + y_of(if e.up { e.heads.first().map_or(0, |h| h.pos) } else { e.heads.last().map_or(0, |h| h.pos) });
        let (ya, yb) = (root(a, n.0), root(z, p.0));
        let x = stem_x(z) - if z.up { RULE.stem } else { 0.0 };
        out.push((si, rect(x, ya.min(yb), RULE.stem, (ya - yb).abs())));
        // The joined stem's flag, at its far end — its partner's tip — when
        // the note's value is an eighth or shorter and its partner draws
        // fewer flags of its own.
        if a.beam.is_none() && z.beam.is_none() && !z.xbeam && flags(a.base) > flags(z.base) && !a.look.omitted("Flag") && !z.look.omitted("Stem") {
            let name = flag_name(a.base, z.up);
            let g = glyph(&name);
            let anchor = g.anchors.get(if z.up { "stemUpNW" } else { "stemDownSW" }).copied().unwrap_or([0.0, 0.0]);
            out.push((si, shifted(&g.fill, x - anchor[0], offs[p.0] + z.stem_end - anchor[1])));
        }
    }
    for sys in systems.iter_mut() {
        sys.cross.clear();
        sys.kneed.clear();
    }
    for (si, c) in out {
        systems[si].cross.push(c);
    }
    for (si, k) in placed {
        systems[si].kneed.push(k);
    }
}

/// Where a slur meets a note of a cross-staff curve: over or under its
/// heads, or at its stem's end where the stem points that way and is its own.
fn slur_end(ev: &Ev, over: bool) -> (f64, f64) {
    let mid_x = ev.x + ev.head_w.unwrap_or(0.0) / 2.0;
    let own = !ev.xbeam && ev.base > 1;
    if over {
        if ev.up && own {
            return (stem_x(ev) - 0.06, ev.stem_end - 0.3);
        }
        return (mid_x, y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.75);
    }
    if !ev.up && own {
        return (stem_x(ev) + 0.06, ev.stem_end + 0.3);
    }
    (mid_x, y_of(ev.heads.first().map_or(0, |h| h.pos)) + 0.75)
}

/// A kneed beam over notes on two staves, `offs` each staff's top from the
/// system's: the primary beam half way between the notes that stem down to
/// it and those that stem up, sloped a little as its ends' notes are, each
/// stem at least [`KNEE_STEM`] long; a shorter value's beams stacked on the
/// side of the stems that carry them, as LilyPond stacks a kneed beam's.
fn kneed(g: &[At], all: &[Vec<Bar>], offs: &[f64], scales: &[f64]) -> Option<(KneedBeam, Vec<Vec<f64>>)> {
    // A staff drawn smaller (score #407): its heads where its scale puts
    // them, up and down from its top line and across from each note's place.
    let sc = |s: usize| scales.get(s).copied().unwrap_or(1.0);
    let sx = |s: usize, e: &Ev| e.x + (stem_x(e) - e.x) * sc(s);
    let notes: Vec<(usize, &Ev)> = g.iter().map(|m| (m.0, &all[m.0][m.1].items[m.2])).collect();
    if notes.len() < 2 {
        return None;
    }
    let bt = RULE.beam;
    let step = RULE.beam_step;
    // A note's head nearest the beam and its root, in the system's frame.
    let near = |s: usize, e: &Ev| offs[s] + sc(s) * y_of(if e.up { e.heads.last() } else { e.heads.first() }.map_or(0, |h| h.pos));
    let xs: Vec<f64> = notes.iter().map(|&(s, e)| sx(s, e) - if e.up { RULE.stem / 2.0 } else { -RULE.stem / 2.0 }).collect();
    let x0 = xs[0];
    let dx = (xs[xs.len() - 1] - x0).max(0.01);
    // Each run of shorter values stacks its beams towards its first note's heads.
    let levels: Vec<i64> = notes.iter().map(|(_, e)| flags(e.base).max(1)).collect();
    let n = levels.iter().copied().max().unwrap_or(1);
    let mut side = vec![0.0; notes.len()];
    let mut run_side: Vec<Vec<f64>> = vec![vec![0.0; notes.len()]; n as usize];
    for level in 1..n {
        let mut i = 0;
        while i < notes.len() {
            if levels[i] <= level {
                i += 1;
                continue;
            }
            let mut jj = i;
            while jj + 1 < notes.len() && levels[jj + 1] > level {
                jj += 1;
            }
            let dir = if notes[i].1.up { 1.0 } else { -1.0 };
            for k in i..=jj {
                run_side[level as usize][k] = dir;
                side[k] = dir;
            }
            i = jj + 1;
        }
    }
    // How far each note's beams reach past the primary's middle, towards its head.
    let reach_in = |k: usize| -> f64 {
        let extra = (levels[k] - 1) as f64 * step;
        let towards_head = if notes[k].1.up { side[k] > 0.0 } else { side[k] < 0.0 };
        bt / 2.0 + if towards_head { extra } else { 0.0 }
    };
    // Its place as LilyPond's beam quanting puts a kneed beam's
    // (`crate::quanting`), y up: the stems from both sides as long as they
    // may be, the slope damped and dear, its ends clear of the lines of
    // both staves.
    let last = notes.len() - 1;
    let qstems: Vec<crate::quanting::QStem> =
        notes.iter().enumerate().map(|(k, &(s, e))| crate::quanting::QStem { x: xs[k], head: -near(s, e), dir: if e.up { 1.0 } else { -1.0 }, stack: reach_in(k) }).collect();
    let mut staves: Vec<usize> = notes.iter().map(|n| n.0).collect();
    staves.sort_unstable();
    staves.dedup();
    let lines: Vec<f64> = staves.iter().flat_map(|&s| (0..5).map(move |l| -(offs[s] + sc(s) * f64::from(l)))).collect();
    let look = &notes[0].1.look;
    let spec = crate::quanting::QBeam {
        count: n,
        edge_counts: (levels[0], levels[last]),
        thick: bt,
        step,
        line: RULE.staff,
        scale: 1.0,
        damping: look.num("Beam.damping").filter(|d| *d > 0.0).unwrap_or(1.0),
        origin: -offs[notes[0].0],
        lines: &lines,
        staff: None,
    };
    let (yl, yr) = crate::quanting::place(&qstems, &spec);
    let slope = -(yr - yl) / dx;
    let b = -yl;
    let y_at = |x: f64| b + slope * (x - x0);
    let mut ink = Vec::new();
    let mut stems = Vec::new();
    for (k, &(s, e)) in notes.iter().enumerate() {
        // From the head furthest from the beam, through the rest, to the far
        // edge of the beams it carries.
        let g = glyph(head_name(e.base));
        let x = sx(s, e) - if e.up { RULE.stem } else { 0.0 };
        let yb = y_at(xs[k]);
        let outer = (levels[k] - 1) as f64 * step;
        let away = if e.up { side[k] < 0.0 } else { side[k] > 0.0 };
        let (from, tip) = if e.up {
            let root = offs[s] + sc(s) * (y_of(e.heads.first().map_or(0, |h| h.pos)) + g.anchors.get("stemUpSE").map_or(0.0, |a| a[1]));
            (root, yb - bt / 2.0 - if away { outer } else { 0.0 })
        } else {
            let root = offs[s] + sc(s) * (y_of(e.heads.last().map_or(0, |h| h.pos)) + g.anchors.get("stemDownNW").map_or(0.0, |a| a[1]));
            (root, yb + bt / 2.0 + if away { outer } else { 0.0 })
        };
        ink.push(rect(x, from.min(tip), RULE.stem, (from - tip).abs()));
        stems.push((s, from, tip));
    }
    // The beams: the primary over all, the others over their runs, a stub
    // where a run is one note.
    let edge = |k: usize, right: bool| -> f64 {
        let (s, e) = notes[k];
        let x = sx(s, e) - if e.up { RULE.stem } else { 0.0 };
        if right {
            x + RULE.stem
        } else {
            x
        }
    };
    for level in 0..n {
        let mut i = 0;
        while i < notes.len() {
            if level > 0 && levels[i] <= level {
                i += 1;
                continue;
            }
            let mut jj = i;
            while jj + 1 < notes.len() && (level == 0 || levels[jj + 1] > level) {
                jj += 1;
            }
            let (mut xa, mut xb) = (edge(i, false), edge(jj, true));
            if i == jj {
                let stub = 1.1;
                if i == 0 {
                    xb = xa + stub;
                } else {
                    xa = xb - stub;
                }
            }
            let off = if level == 0 { 0.0 } else { run_side[level as usize][i] * level as f64 * step };
            let (ya, yb) = (y_at(xa) + off, y_at(xb) + off);
            ink.push(vec![0.0, xa, ya - bt / 2.0, 1.0, xb, yb - bt / 2.0, 1.0, xb, yb + bt / 2.0, 1.0, xa, ya + bt / 2.0, 4.0]);
            i = jj + 1;
        }
    }
    Some((KneedBeam { x0, x1: xs[last], y0: y_at(x0), y1: y_at(xs[last]), stems }, ink))
}
