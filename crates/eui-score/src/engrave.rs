//! Laying a score out: beams, spacing, lines, places, stems, staves and
//! pages (`eui_builders_score.sl`, "Stems and beams" to "Placing" and
//! "Several staves"). Function for function the Soli engraver's, under
//! the same names without their `score_` prefix.

use std::ops::Range;

use crate::glyphs::{glyph, Weight};
use crate::marks::{self, curves, reach, DynEl, HairpinPiece, ScriptPlace, TempoMark, TextPlace, TupletPlace, Volta};
use crate::measure::{measures, Bar, Beam, Changes, Opening};
use crate::parse::{refuse, Clef, Ev, Group, GroupKind, Kind, Parsed, Tempo};
use crate::text::{em, header_lines, text_width, HeadBlock};
use crate::{Layout, ScoreError, RULE, WHOLE};

/// What a line ends with when the next one opens with a new key or metre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Courtesy {
    /// One staff's next opening.
    Single(Opening),
    /// Each staff's, for a shared bar.
    Multi(Vec<Opening>),
}

/// One staff's line of music.
#[derive(Debug, Clone, Default)]
pub struct Line {
    /// Its bars, by index into its staff's bars.
    pub bars: Range<usize>,
    /// Where the staff starts.
    pub left: f64,
    /// Where the music starts, after the clef, key and time.
    pub prefix: f64,
    /// Where the staff ends.
    pub right: f64,
    /// Where the last bar line ends.
    pub bar_end: f64,
    /// The stretch.
    pub k: f64,
    /// Room for the next line's opening, announced at this one's end.
    pub courtesy: f64,
    /// That opening.
    pub courtesy_bar: Option<Courtesy>,
    /// The staff's top line, from the page's top.
    pub top: f64,
    /// Where the ink ends.
    pub bottom: f64,
    /// How far the ink reaches above the top line.
    pub reach_hi: f64,
    /// Its page.
    pub page: usize,
    /// Ties and slurs, as fill commands in staff spaces.
    pub curves: Vec<Vec<f64>>,
    /// Tempo marks.
    pub marks: Vec<TempoMark>,
    /// Endings' brackets.
    pub voltas: Vec<Volta>,
    /// Tuplets' numbers and brackets.
    pub tuplets: Vec<TupletPlace>,
    /// Articulations.
    pub scripts: Vec<ScriptPlace>,
    /// Dynamics and hairpins.
    pub dynamics: Vec<DynEl>,
    /// Words.
    pub texts: Vec<TextPlace>,
    /// Hairpins, before they are placed.
    pub hairpins: Vec<HairpinPiece>,
    /// In a score of several staves, which.
    pub staff_index: Option<usize>,
    /// The widest clef of the line's staves.
    pub clef_w: Option<f64>,
    /// The widest key.
    pub key_w: Option<f64>,
    /// How far its bar lines run on down to the next staff.
    pub connect_down: f64,
}

/// A system: one line, or one line of each staff.
#[derive(Debug, Clone, Default)]
pub struct System {
    /// Its lines, one a staff.
    pub lines: Vec<Line>,
    /// Several staves.
    pub multi: bool,
    /// Each staff's top line from the first's.
    pub offsets: Vec<f64>,
    /// The ink's reach, above the first staff and below the last.
    pub reach: Option<(f64, f64)>,
    /// The names printed left of each staff.
    pub names: Vec<String>,
    /// The first system.
    pub first_line: bool,
    /// Where its staves start.
    pub left: f64,
    /// Where they end.
    pub right: f64,
    /// Its first staff's top line, from the page's top.
    pub top: f64,
    /// Where its ink ends.
    pub bottom: f64,
    /// How far its ink reaches above it.
    pub reach_hi: f64,
    /// Its page.
    pub page: usize,
}

/// A page and its systems.
#[derive(Debug, Clone, Default)]
pub struct Page {
    /// From 0.
    pub index: usize,
    /// Its systems, by index.
    pub systems: Vec<usize>,
}

/// A score laid out.
#[derive(Debug, Clone, Default)]
pub struct Engraved {
    /// Each staff's bars.
    pub staves: Vec<Vec<Bar>>,
    /// The systems.
    pub systems: Vec<System>,
    /// How tall it is on a screen.
    pub height: f64,
    /// The line width.
    pub width: f64,
    /// Where the staves start.
    pub left: f64,
    /// The titles.
    pub header: HeadBlock,
    /// The pages.
    pub pages: Vec<Page>,
    /// The paper's height, on paper.
    pub paper: Option<f64>,
    /// Its pages are numbered: on paper, which has margins. A screen cut
    /// into pages (`page_h` alone) has a pager saying which page it is.
    pub numbered: bool,
    /// The staves' groups.
    pub groups: Vec<Group>,
}

pub(crate) fn y_of(pos: i64) -> f64 {
    2.0 - pos as f64 / 2.0
}

// ---- Stems and beams --------------------------------------------------

pub(crate) fn flags(base: i64) -> i64 {
    match base {
        8 => 1,
        16 => 2,
        32 => 3,
        _ => 0,
    }
}

fn stem_up(ev: &Ev) -> bool {
    let top = ev.heads.last().map_or(0, |h| h.pos);
    let low = ev.heads.first().map_or(0, |h| h.pos);
    low + top < 0
}

fn beam_windows(time: (i64, i64)) -> (i64, i64) {
    let (num, den) = time;
    let beat = WHOLE / den;
    if den == 8 && num % 3 == 0 {
        return (WHOLE * 3 / 8, WHOLE * 3 / 8);
    }
    if num == 4 && den == 4 {
        return (WHOLE / 2, WHOLE / 4);
    }
    if den == 4 && (num == 2 || num == 3) {
        return (num * beat, WHOLE / 4);
    }
    if num == 2 && den == 2 {
        return (WHOLE / 2, WHOLE / 4);
    }
    (beat, beat)
}

pub(crate) fn beams(bars: &mut [Bar]) -> Result<(), ScoreError> {
    for bar in bars.iter_mut() {
        let wins = beam_windows(bar.time);
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut cur: Vec<usize> = Vec::new();
        let mut manual = false;
        let mut win = -1;
        for (j, ev) in bar.items.iter().enumerate() {
            let fl = if ev.k == Kind::Note { flags(ev.base) } else { 0 };
            if manual {
                if fl == 0 && ev.k == Kind::Note {
                    return refuse("a beam over a note that has no flag", ev.at);
                }
                if ev.k == Kind::Note {
                    cur.push(j);
                }
                if ev.beam_off {
                    groups.push(std::mem::take(&mut cur));
                    manual = false;
                }
                continue;
            }
            if ev.beam_on && fl > 0 {
                if !cur.is_empty() {
                    groups.push(std::mem::take(&mut cur));
                }
                cur = vec![j];
                manual = true;
                continue;
            }
            let w = ev.start / wins.0;
            if (fl == 0 || w != win) && !cur.is_empty() {
                groups.push(std::mem::take(&mut cur));
            }
            if fl > 0 {
                cur.push(j);
                win = w;
            }
        }
        if !cur.is_empty() {
            groups.push(cur);
        }
        let mut by_beat: Vec<Vec<usize>> = Vec::new();
        for g in groups {
            let short = g.iter().any(|&j| flags(bar.items[j].base) > 1 || bar.items[j].tuplet.is_some());
            if g.len() > 1 && short && !bar.items[g[0]].beam_on {
                let mut part: Vec<usize> = Vec::new();
                let mut pw = -1;
                let mut pt: Option<i64> = None;
                for &j in &g {
                    let ev = &bar.items[j];
                    let w = ev.start / wins.1;
                    let tid = ev.tuplet.map(|t| t.id);
                    if (w != pw || tid != pt) && !part.is_empty() {
                        by_beat.push(std::mem::take(&mut part));
                    }
                    part.push(j);
                    pw = w;
                    pt = tid;
                }
                by_beat.push(part);
            } else {
                by_beat.push(g);
            }
        }
        for g in by_beat {
            if g.len() < 2 {
                continue;
            }
            let mut lowest = 0;
            let mut highest = 0;
            for &j in &g {
                let ev = &bar.items[j];
                if let Some(h) = ev.heads.first() {
                    if h.pos < lowest {
                        lowest = h.pos;
                    }
                }
                if let Some(h) = ev.heads.last() {
                    if h.pos > highest {
                        highest = h.pos;
                    }
                }
            }
            let up = -lowest > highest;
            let gi = bar.beams.len();
            for &j in &g {
                bar.items[j].beam = Some(gi);
                bar.items[j].up = up;
            }
            bar.beams.push(Beam { notes: g, up, ..Beam::default() });
        }
        for ev in &mut bar.items {
            if ev.k == Kind::Note && ev.beam.is_none() {
                ev.up = stem_up(ev);
            }
        }
    }
    Ok(())
}

// ---- Columns -----------------------------------------------------------

pub(crate) fn duration_space(t: i64, shortest: i64) -> f64 {
    let ratio = t as f64 / shortest as f64;
    if ratio < 1.0 {
        return 1.2 * (1.0 + ratio) + 0.1;
    }
    1.2 * (2.0 + log2(ratio)) + 0.1
}

pub(crate) fn log2(v: f64) -> f64 {
    v.ln() / 2.0_f64.ln()
}

pub(crate) fn shortest<'a>(bars: impl Iterator<Item = &'a Bar>) -> i64 {
    let mut counts: Vec<(i64, i64)> = Vec::new();
    for bar in bars {
        let mut least = 0;
        for ev in &bar.items {
            let t = space_ticks(ev);
            if least == 0 || t < least {
                least = t;
            }
        }
        if least > 0 {
            match counts.iter_mut().find(|(k, _)| *k == least) {
                Some(c) => c.1 += 1,
                None => counts.push((least, 1)),
            }
        }
    }
    let mut best = WHOLE / 4;
    let mut most = 0;
    for (t, c) in counts {
        if c > most || (c == most && t < best) {
            best = t;
            most = c;
        }
    }
    best.min(WHOLE * 3 / 16)
}

fn space_ticks(ev: &Ev) -> i64 {
    let t = WHOLE / ev.base.max(1);
    match ev.tuplet {
        None => t,
        Some(tp) => t * tp.d / tp.n,
    }
}

pub(crate) fn head_name(base: i64) -> &'static str {
    match base {
        1 => "noteheadWhole",
        2 => "noteheadHalf",
        _ => "noteheadBlack",
    }
}

pub(crate) fn rest_name(base: i64) -> &'static str {
    match base {
        1 => "restWhole",
        2 => "restHalf",
        4 => "restQuarter",
        8 => "rest8th",
        16 => "rest16th",
        _ => "rest32nd",
    }
}

pub(crate) fn acc_name(alter: i64) -> &'static str {
    match alter {
        -2 => "accidentalDoubleFlat",
        -1 => "accidentalFlat",
        0 => "accidentalNatural",
        1 => "accidentalSharp",
        _ => "accidentalDoubleSharp",
    }
}

pub(crate) fn flag_name(base: i64, up: bool) -> String {
    let stem = match base {
        8 => "8th",
        16 => "16th",
        _ => "32nd",
    };
    format!("flag{stem}{}", if up { "Up" } else { "Down" })
}

fn columns(bars: &mut [Bar], shortest: i64) {
    for bar in bars.iter_mut() {
        for ev in &mut bar.items {
            let t = space_ticks(ev);
            ev.ideal = duration_space(t, shortest);
            if ev.k == Kind::Rest {
                if let Some(mm) = ev.mm {
                    ev.ideal = if mm <= 10 { 3.0 + church_width(mm) } else { 7.0 + log2(mm as f64) };
                    ev.left = 0.0;
                    ev.right = if mm <= 10 { church_width(mm) + 1.0 } else { 4.0 };
                    continue;
                }
                let g = glyph(rest_name(ev.base));
                ev.left = 0.0;
                ev.right = g.advance + if ev.dots > 0 { 0.3 + 0.6 * ev.dots as f64 } else { 0.0 };
                continue;
            }
            let hw = glyph(head_name(ev.base)).advance;
            ev.head_w = Some(hw);
            displace(ev);
            let mut right = hw;
            if ev.heads.iter().any(|h| h.side == 1) {
                right += hw - RULE.stem;
            }
            if ev.dots > 0 {
                right += 0.3 + 0.6 * ev.dots as f64;
            }
            if ev.up && ev.beam.is_none() && flags(ev.base) > 0 {
                right += glyph(&flag_name(ev.base, true)).advance - RULE.stem;
            }
            ev.right = right;
            let shifted = ev.heads.iter().any(|h| h.side == -1);
            let lefts = if shifted { hw - RULE.stem } else { 0.0 };
            ev.left = lefts + place_accidentals(ev);
        }
    }
}

fn displace(ev: &mut Ev) {
    let up = ev.up;
    let n = ev.heads.len();
    let order: Vec<usize> = if up { (0..n).collect() } else { (0..n).rev().collect() };
    let mut prev: Option<usize> = None;
    for k in order {
        ev.heads[k].side = 0;
        if let Some(p) = prev {
            if (ev.heads[k].pos - ev.heads[p].pos).abs() == 1 && ev.heads[p].side == 0 {
                ev.heads[k].side = if up { 1 } else { -1 };
            }
        }
        prev = Some(k);
    }
}

struct AccCol {
    spans: Vec<(f64, f64)>,
    w: f64,
    x: f64,
}

fn place_accidentals(ev: &mut Ev) -> f64 {
    let mut cols: Vec<AccCol> = Vec::new();
    let mut col_of: Vec<Option<usize>> = vec![None; ev.heads.len()];
    for hk in (0..ev.heads.len()).rev() {
        let Some(acc) = ev.heads[hk].acc else { continue };
        let g = glyph(acc_name(acc));
        let y = 2.0 - ev.heads[hk].pos as f64 / 2.0;
        let top = y + g.bbox[1];
        let low = y + g.bbox[3];
        let mut placed = false;
        for (ci, col) in cols.iter_mut().enumerate() {
            let clear = col.spans.iter().all(|sn| low <= sn.0 + 0.1 || top >= sn.1 - 0.1);
            if clear {
                col.spans.push((top, low));
                col.w = col.w.max(g.advance);
                col_of[hk] = Some(ci);
                placed = true;
                break;
            }
        }
        if !placed {
            cols.push(AccCol { spans: vec![(top, low)], w: g.advance, x: 0.0 });
            col_of[hk] = Some(cols.len() - 1);
        }
    }
    let shifted = ev.heads.iter().any(|h| h.side == -1);
    let hw = ev.head_w.unwrap_or(0.0);
    let mut rim = if shifted { -(hw - RULE.stem) } else { 0.0 };
    for col in &mut cols {
        col.x = rim - 0.2 - col.w;
        rim = col.x;
    }
    for (hk, c) in col_of.into_iter().enumerate() {
        if let Some(ci) = c {
            ev.heads[hk].acc_x = Some(cols[ci].x);
        }
    }
    if rim < 0.0 {
        -rim
    } else {
        0.0
    }
}

pub(crate) fn church_width(count: i64) -> f64 {
    let mut left_bars = count;
    let mut w = 0.0;
    let mut parts = 0;
    while left_bars > 0 {
        if left_bars >= 4 {
            w += 0.5;
            left_bars -= 4;
        } else if left_bars >= 2 {
            w += 0.5;
            left_bars -= 2;
        } else {
            w += glyph("restWhole").advance;
            left_bars -= 1;
        }
        parts += 1;
    }
    w + 0.6 * (parts - 1) as f64
}

// ---- Systems ------------------------------------------------------------

pub(crate) fn key_width(fifths: i64) -> f64 {
    if fifths == 0 {
        return 0.0;
    }
    let g = glyph(if fifths > 0 { "accidentalSharp" } else { "accidentalFlat" });
    fifths.abs() as f64 * (g.advance + 0.1)
}

pub(crate) fn time_glyph(time: (i64, i64)) -> Option<&'static str> {
    if time == (4, 4) {
        return Some("timeSigCommon");
    }
    if time == (2, 2) {
        return Some("timeSigCutCommon");
    }
    None
}

pub(crate) fn digits(v: i64) -> Vec<String> {
    v.to_string().chars().map(|ch| format!("timeSig{ch}")).collect()
}

pub(crate) fn time_width(time: (i64, i64)) -> f64 {
    if let Some(one) = time_glyph(time) {
        return glyph(one).advance;
    }
    let mut wide: f64 = 0.0;
    for part in [time.0, time.1] {
        let w = digits(part).iter().fold(0.0, |acc, nm| acc + glyph(nm).advance);
        if w > wide {
            wide = w;
        }
    }
    wide
}

pub(crate) fn clef_glyph(clef: Clef) -> &'static str {
    match clef {
        Clef::Treble => "gClef",
        Clef::Bass => "fClef",
        Clef::Alto => "cClef",
    }
}

fn prefix_width_of(o: &Opening, first: bool) -> f64 {
    let mut w = 0.8 + glyph(clef_glyph(o.clef)).advance + 1.0;
    if o.fifths != 0 {
        w += key_width(o.fifths) + 0.8;
    }
    if first || o.changes.time {
        w += time_width(o.time) + 1.0;
    }
    w
}

fn prefix_width(bar: &Bar, first: bool) -> f64 {
    if !bar.staff_openings.is_empty() {
        return bar.staff_openings.iter().map(|o| prefix_width_of(o, first)).fold(0.0, f64::max);
    }
    prefix_width_of(&bar.opening(), first)
}

/// Which of the old key's accidentals a change cancels, or -1.
pub(crate) fn key_cancel(old_key: i64, new_key: i64) -> i64 {
    if old_key == 0 {
        return -1;
    }
    if new_key == 0 || (old_key > 0) != (new_key > 0) {
        return 0;
    }
    let had = old_key.abs();
    let has = new_key.abs();
    if has < had {
        has
    } else {
        -1
    }
}

fn change_width_of(o: &Opening) -> f64 {
    let mut w = 0.0;
    let ch = o.changes;
    if ch.clef {
        w += glyph(clef_glyph(o.clef)).advance + 0.8;
    }
    if ch.key {
        let skip = key_cancel(ch.key_from, o.fifths);
        if skip >= 0 {
            let cancelled = ch.key_from.abs() - skip;
            w += cancelled as f64 * (glyph("accidentalNatural").advance + 0.1) + 0.5;
        }
        if o.fifths != 0 {
            w += key_width(o.fifths) + 0.8;
        }
    }
    if ch.time {
        w += time_width(o.time) + 1.0;
    }
    w
}

fn change_width(bar: &Bar) -> f64 {
    if !bar.staff_openings.is_empty() {
        return bar.staff_openings.iter().map(change_width_of).fold(0.0, f64::max);
    }
    change_width_of(&bar.opening())
}

fn gaps(bar: &Bar, k: f64) -> Vec<f64> {
    let items = &bar.items;
    if items.is_empty() {
        return vec![(6.0 * k).max(4.0)];
    }
    let mut out = vec![1.2 + items[0].left];
    for i in 0..items.len() {
        let ev = &items[i];
        if let Some(nxt) = items.get(i + 1) {
            out.push((ev.ideal * k).max(ev.right + 0.6 + nxt.left));
        } else {
            out.push((ev.ideal * k).max(ev.right + 0.8));
        }
    }
    out
}

fn bar_springs(bar: &Bar) -> f64 {
    if bar.items.is_empty() {
        return 6.0;
    }
    bar.items.iter().fold(0.0, |acc, ev| acc + ev.ideal)
}

pub(crate) fn courtesy_of(bar: &Bar) -> Courtesy {
    if !bar.staff_openings.is_empty() {
        return Courtesy::Multi(bar.staff_openings.iter().map(courtesy_opening).collect());
    }
    Courtesy::Single(courtesy_opening(&bar.opening()))
}

fn courtesy_opening(o: &Opening) -> Opening {
    Opening { clef: o.clef, fifths: o.fifths, time: o.time, changes: Changes { clef: false, key: o.changes.key, key_from: o.changes.key_from, time: o.changes.time } }
}

fn courtesy_width_of(o: &Opening) -> f64 {
    let ch = o.changes;
    let mut cw = 0.0;
    if ch.key {
        let skip = key_cancel(ch.key_from, o.fifths);
        if skip >= 0 {
            cw += (ch.key_from.abs() - skip) as f64 * (glyph("accidentalNatural").advance + 0.1) + 0.5;
        }
        if o.fifths != 0 {
            cw += key_width(o.fifths) + 0.5;
        }
    }
    if ch.time {
        cw += time_width(o.time) + 0.5;
    }
    if cw > 0.0 {
        cw + 0.6
    } else {
        0.0
    }
}

pub(crate) fn courtesy_width(c: &Courtesy) -> f64 {
    match c {
        Courtesy::Single(o) => courtesy_width_of(o),
        Courtesy::Multi(all) => all.iter().map(courtesy_width_of).fold(0.0, f64::max),
    }
}

fn bar_width(bar: &Bar, k: f64, opening: bool) -> f64 {
    let mut total = gaps(bar, k).iter().fold(0.0, |acc, g| acc + g);
    if !opening {
        total += change_width(bar);
    }
    if opening && bar.open_repeat.is_some() {
        total += barline_width(".|:") + 0.4;
    }
    total + barline_width(bar.drawn())
}

pub(crate) fn barline_parts(kind: &str) -> &'static [&'static str] {
    match kind {
        "||" => &["thin", "thin"],
        "|." => &["thin", "thick"],
        ".|:" => &["thick", "thin", "dots"],
        ":|." => &["dots", "thin", "thick"],
        ":|.|:" => &["dots", "thin", "thick", "thin", "dots"],
        _ => &["thin"],
    }
}

pub(crate) fn part_width(part: &str) -> f64 {
    match part {
        "thin" => RULE.thin,
        "thick" => RULE.thick,
        _ => glyph("augmentationDot").advance,
    }
}

pub(crate) fn barline_width(kind: &str) -> f64 {
    let parts = barline_parts(kind);
    parts.iter().fold(0.0, |acc, p| acc + part_width(p)) + RULE.kern * (parts.len() as f64 - 1.0)
}

fn system_width(bars: &[Bar], k: f64) -> f64 {
    let mut total = prefix_width(&bars[0], bars[0].first);
    for (i, b) in bars.iter().enumerate() {
        total += bar_width(b, k, i == 0);
    }
    total
}

fn break_bars(bars: &mut [Bar], width: f64, indent: f64, rest: f64) -> Vec<Line> {
    bars[0].first = true;
    if bars[0].open_bar {
        bars[0].open_repeat = Some(".|:".into());
    }
    let count = bars.len();
    let opening_w: Vec<f64> = bars.iter().map(|b| bar_width(b, 1.0, true)).collect();
    let inner_w: Vec<f64> = bars.iter().map(|b| bar_width(b, 1.0, false)).collect();
    let opening_min: Vec<f64> = bars.iter().map(|b| bar_width(b, 0.0, true)).collect();
    let inner_min: Vec<f64> = bars.iter().map(|b| bar_width(b, 0.0, false)).collect();
    let springs: Vec<f64> = bars.iter().map(bar_springs).collect();
    let courtesy_w: Vec<f64> = bars.iter().map(|b| courtesy_width(&courtesy_of(b))).collect();
    let prefix_w: Vec<f64> = (0..count).map(|t| prefix_width(&bars[t], t == 0)).collect();
    let mut best = vec![-1.0; count + 1];
    best[0] = 0.0;
    let mut from_bar = vec![0usize; count + 1];
    for i in 0..count {
        if best[i] < 0.0 {
            continue;
        }
        let room = if i == 0 { width - indent } else { width - rest };
        let mut w = prefix_w[i] + opening_w[i];
        let mut tight = prefix_w[i] + opening_min[i];
        let mut spring = springs[i];
        let mut jj = i + 1;
        while jj <= count {
            if jj > i + 1 && bars[jj - 2].brk.is_some() {
                break;
            }
            if jj > i + 1 {
                w += inner_w[jj - 1];
                tight += inner_min[jj - 1];
                spring += springs[jj - 1];
            }
            let courtesy = if jj < count { courtesy_w[jj] } else { 0.0 };
            let line_w = w + courtesy;
            if tight + courtesy > room && jj > i + 1 {
                break;
            }
            let ratio = (room - line_w) / spring.max(1.0);
            let cost = if tight + courtesy > room {
                1000.0
            } else if ratio < 0.0 {
                ratio * ratio * 40.0 + 1.0
            } else if jj < count {
                ratio * ratio * 10.0 + 1.0
            } else {
                0.0
            };
            let total = best[i] + cost;
            if best[jj] < 0.0 || total < best[jj] {
                best[jj] = total;
                from_bar[jj] = i;
            }
            jj += 1;
        }
    }
    let mut cuts = Vec::new();
    let mut at = count;
    while at > 0 {
        cuts.push(from_bar[at]);
        at = from_bar[at];
    }
    cuts.reverse();
    break_lines(bars, width, indent, &cuts, rest)
}

fn break_lines(bars: &mut [Bar], width: f64, indent: f64, starts: &[usize], rest: f64) -> Vec<Line> {
    let count = bars.len();
    let mut lines: Vec<Line> = Vec::new();
    for (si, &start) in starts.iter().enumerate() {
        let stop = starts.get(si + 1).copied().unwrap_or(count);
        lines.push(Line { bars: start..stop, ..Line::default() });
    }
    let n = lines.len();
    for si in 0..n {
        if si + 1 >= n {
            continue;
        }
        let tail = lines[si].bars.end - 1;
        let kind = bars[tail].bar.clone();
        if kind == ".|:" || kind == ":|.|:" {
            bars[tail].bar_drawn = Some(if kind == ".|:" { "|".into() } else { ":|.".into() });
            let next_first = lines[si + 1].bars.start;
            bars[next_first].open_repeat = Some(".|:".into());
        }
    }
    for si in 0..n {
        let mut room = if si == 0 { width - indent } else { width - rest };
        lines[si].courtesy = 0.0;
        if si + 1 < n {
            let next_first = lines[si + 1].bars.start;
            let c = courtesy_of(&bars[next_first]);
            lines[si].courtesy = courtesy_width(&c);
            lines[si].courtesy_bar = Some(c);
            room -= lines[si].courtesy;
        }
        let range = lines[si].bars.clone();
        let natural = system_width(&bars[range.clone()], 1.0);
        let last = si == n - 1;
        let mut k = 1.0;
        if natural > room {
            k = fit(&bars[range], room, 0.0, 1.0);
        } else if !last || natural >= room * 0.85 {
            k = fit(&bars[range], room, 1.0, 8.0);
        }
        lines[si].k = k;
    }
    lines
}

fn line_terms(bars: &[Bar]) -> (f64, Vec<f64>, Vec<f64>) {
    let mut base = prefix_width(&bars[0], bars[0].first);
    let mut ideals = Vec::new();
    let mut leasts = Vec::new();
    for (i, bar) in bars.iter().enumerate() {
        let items = &bar.items;
        if items.is_empty() {
            ideals.push(6.0);
            leasts.push(4.0);
        } else {
            base += 1.2 + items[0].left;
            for j in 0..items.len() {
                let ev = &items[j];
                ideals.push(ev.ideal);
                leasts.push(match items.get(j + 1) {
                    Some(nxt) => ev.right + 0.6 + nxt.left,
                    None => ev.right + 0.8,
                });
            }
        }
        if i != 0 {
            base += change_width(bar);
        }
        if i == 0 && bar.open_repeat.is_some() {
            base += barline_width(".|:") + 0.4;
        }
        base += barline_width(bar.drawn());
    }
    (base, ideals, leasts)
}

fn fit(bars: &[Bar], width: f64, mut lo: f64, mut hi: f64) -> f64 {
    let (base, ideals, leasts) = line_terms(bars);
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        let mut total = base;
        for j in 0..ideals.len() {
            total += (ideals[j] * mid).max(leasts[j]);
        }
        if total > width {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    lo
}

// ---- Placing ---------------------------------------------------------------

fn place(bars: &mut [Bar], lines: &mut [Line], left: f64, indent: f64, rest: f64) {
    for (si, line) in lines.iter_mut().enumerate() {
        let range = line.bars.clone();
        line.left = left + if si == 0 { indent } else { rest };
        let first = &bars[range.start];
        let mut x = line.left + prefix_width(first, first.first);
        line.prefix = x;
        line.right = 0.0;
        for (bi, b) in range.clone().enumerate() {
            let k = line.k;
            let bar = &mut bars[b];
            bar.x0 = x;
            if bi != 0 {
                x += change_width(bar);
            }
            if bi == 0 && bar.open_repeat.is_some() {
                x += barline_width(".|:");
                bar.open_repeat_x = Some(x);
                x += 0.4;
            }
            let gs = gaps(bar, k);
            let length = bar.length;
            let whole = bar.items.len() == 1 && bar.items[0].k == Kind::Rest && (bar.items[0].full || bar.items[0].ticks == length);
            if whole {
                let span = gs.iter().fold(0.0, |acc, g| acc + g);
                let it = &mut bar.items[0];
                it.x = x + span / 2.0 - glyph("restWhole").advance / 2.0;
                it.whole_bar = true;
                it.span0 = x;
                it.span1 = x + span;
                x += span;
            } else {
                x += gs[0];
                for (i, ev) in bar.items.iter_mut().enumerate() {
                    ev.x = x;
                    x += gs[i + 1];
                }
            }
            bar.x1 = x;
            x += barline_width(bar.drawn());
        }
        line.bar_end = x;
        line.right = x + line.courtesy;
        stems_beams(&mut bars[range]);
    }
}

pub(crate) fn stems_beams(bars: &mut [Bar]) {
    for bar in bars.iter_mut() {
        for ev in &mut bar.items {
            if ev.k == Kind::Note && ev.beam.is_none() {
                stem(ev);
            }
        }
        for gi in 0..bar.beams.len() {
            beam(bar, gi);
        }
    }
}

fn stem(ev: &mut Ev) {
    let hi = ev.heads.last().map_or(0, |h| h.pos);
    let lo = ev.heads.first().map_or(0, |h| h.pos);
    if ev.base == 1 {
        ev.stem_end = y_of(hi);
        return;
    }
    let len = 3.5 + if ev.base == 32 { 0.75 } else { 0.0 };
    if ev.up {
        ev.stem_end = (y_of(hi) - len).min(2.0);
    } else {
        ev.stem_end = (y_of(lo) + len).max(2.0);
    }
}

pub(crate) fn stem_x(ev: &Ev) -> f64 {
    if ev.up {
        ev.x + ev.head_w.unwrap_or(0.0)
    } else {
        ev.x
    }
}

fn quant(y: f64, toward: i64) -> f64 {
    let inside = y > -0.6 && y < 4.6;
    let mut best: Option<f64> = None;
    let base = y.floor() as i64 - 2;
    for i in 0..6 {
        let line = (base + i) as f64;
        let half_line = RULE.staff / 2.0;
        let bt = RULE.beam;
        let candidates: Vec<f64> = if inside { vec![line - bt - half_line, line - bt / 2.0, line + half_line] } else { vec![line, line + 0.25, line + 0.5, line + 0.75] };
        for c in candidates {
            let mut ok = true;
            if toward < 0 {
                ok = c <= y + 0.0001;
            }
            if toward > 0 {
                ok = c >= y - 0.0001;
            }
            if ok && best.is_none_or(|b| (c - y).abs() < (b - y).abs()) {
                best = Some(c);
            }
        }
    }
    best.unwrap_or(y)
}

fn beam(bar: &mut Bar, gi: usize) {
    let notes = bar.beams[gi].notes.clone();
    let up = bar.beams[gi].up;
    let n_beams = notes.iter().fold(0, |acc, &j| acc.max(flags(bar.items[j].base)));
    let thick = RULE.beam;
    let near: Vec<f64> = notes
        .iter()
        .map(|&j| {
            let ev = &bar.items[j];
            y_of(if up { ev.heads.last().map_or(0, |h| h.pos) } else { ev.heads.first().map_or(0, |h| h.pos) })
        })
        .collect();
    let x0 = stem_x(&bar.items[notes[0]]);
    let x1 = stem_x(&bar.items[notes[notes.len() - 1]]);
    let dx = (x1 - x0).max(0.01);
    let dy = near[near.len() - 1] - near[0];
    let mut slope = (dy * 0.55).clamp(-1.5, 1.5) / dx;
    let mut concave = false;
    for (i, &y) in near.iter().enumerate() {
        if i == 0 || i == near.len() - 1 {
            continue;
        }
        if up && y < near[0].min(near[near.len() - 1]) {
            concave = true;
        }
        if !up && y > near[0].max(near[near.len() - 1]) {
            concave = true;
        }
    }
    if concave {
        slope = 0.0;
    }
    let ideal = [3.36, 3.6, 3.8][(n_beams.min(3) - 1).max(0) as usize];
    let shortest = 2.5 + (n_beams - 1) as f64 * RULE.beam_step;
    let mut b = 0.0;
    for (i, &j) in notes.iter().enumerate() {
        let xs = stem_x(&bar.items[j]) - x0;
        b += if up { near[i] - ideal - slope * xs } else { near[i] + ideal - slope * xs };
    }
    b /= notes.len() as f64;
    for (i, &j) in notes.iter().enumerate() {
        let xs = stem_x(&bar.items[j]) - x0;
        let len = if up { near[i] - (b + slope * xs) } else { (b + slope * xs) - near[i] };
        if up && len < shortest {
            b -= shortest - len;
        }
        if !up && len < shortest {
            b += shortest - len;
        }
    }
    for &j in &notes {
        let xs = stem_x(&bar.items[j]) - x0;
        let rim = b + slope * xs;
        if up && rim > 2.0 {
            b -= rim - 2.0;
        }
        if !up && rim < 2.0 {
            b += 2.0 - rim;
        }
    }
    let y_left = b;
    let y_right = b + slope * dx;
    let mut toward = 0;
    for (i, &j) in notes.iter().enumerate() {
        let xs = stem_x(&bar.items[j]) - x0;
        let len = if up { near[i] - (b + slope * xs) } else { (b + slope * xs) - near[i] };
        if len < shortest + 0.25 {
            toward = if up { -1 } else { 1 };
        }
    }
    let (mut q_left, mut q_right) = if up { (quant(y_left, toward), quant(y_right, toward)) } else { (quant(y_left - thick, toward) + thick, quant(y_right - thick, toward) + thick) };
    if (q_right - q_left).abs() > 1.5 {
        let held = if q_right > q_left { q_left + 1.5 } else { q_left - 1.5 };
        let back = if q_right > q_left { -1 } else { 1 };
        q_right = if up { quant(held, back) } else { quant(held - thick, back) + thick };
    }
    let turned = (q_right - q_left) * (y_right - y_left) < 0.0;
    let levelled = slope == 0.0 && q_right != q_left;
    if turned || levelled {
        let q = if up { q_left.min(q_right) } else { q_left.max(q_right) };
        q_left = q;
        q_right = q;
    }
    let g = &mut bar.beams[gi];
    g.x0 = x0;
    g.x1 = x1;
    g.y0 = q_left;
    g.y1 = q_right;
    g.n = n_beams;
    let g = g.clone();
    for &j in &notes {
        let sx = stem_x(&bar.items[j]);
        bar.items[j].stem_end = beam_y(&g, sx);
    }
}

pub(crate) fn beam_y(g: &Beam, x: f64) -> f64 {
    let span = g.x1 - g.x0;
    if span <= 0.0 {
        return g.y0;
    }
    g.y0 + (g.y1 - g.y0) * (x - g.x0) / span
}

// ---- Vertical spacing -----------------------------------------------------

fn stack(systems: &mut [System], staves: &[Vec<Bar>], layout: &Layout, head: &HeadBlock) -> Vec<Page> {
    let page_h = layout.page_h;
    let margin = layout.top.unwrap_or(0.0);
    let floor_y = page_h.map_or(0.0, |h| h - layout.bottom.unwrap_or(0.0));
    let gap = layout.gap.unwrap_or(10.0);
    let titled = !head.lines.is_empty();
    let mut pages = vec![Page { index: 0, systems: Vec::new() }];
    let mut prev: Option<(f64, f64, bool)> = None;
    for (si, sys) in systems.iter_mut().enumerate() {
        let r = system_reach(sys, staves);
        let mut tp;
        match prev {
            None => {
                tp = if titled { (margin + head.bottom + 3.0).max(margin + head.bottom + 0.5 - r.0) } else { margin + 0.5 - r.0 };
                if titled && page_h.is_none() {
                    tp += 1.0;
                }
            }
            Some((ptop, pbottom, _)) => {
                tp = (ptop + gap).max(pbottom + 1.0 - r.0);
            }
        }
        let forced = prev.is_some_and(|p| p.2);
        if page_h.is_some() && prev.is_some() && (forced || tp + r.1 + 1.0 > floor_y) {
            pages.push(Page { index: pages.len(), systems: Vec::new() });
            tp = margin + 3.5 + 1.0 - r.0;
        }
        sys.top = tp;
        sys.bottom = tp + r.1;
        sys.reach_hi = r.0;
        sys.page = pages.len() - 1;
        let pi = pages.len() - 1;
        pages[pi].systems.push(si);
        let last_bar = sys.lines[0].bars.end - 1;
        let brk_page = staves[0].get(last_bar).is_some_and(|b| b.brk.as_deref() == Some("page"));
        prev = Some((sys.top, sys.bottom, brk_page));
    }
    if page_h.is_some() {
        let n = pages.len();
        for (pi, pg) in pages.iter().enumerate() {
            if pi == n - 1 || pg.systems.len() < 2 {
                continue;
            }
            let last = pg.systems[pg.systems.len() - 1];
            let extra = floor_y - 1.0 - systems[last].bottom;
            let step = (extra / (pg.systems.len() - 1) as f64).min(6.0);
            if step <= 0.0 {
                continue;
            }
            for (k, &s) in pg.systems.iter().enumerate() {
                systems[s].top += step * k as f64;
                systems[s].bottom += step * k as f64;
            }
        }
    }
    for sys in systems.iter_mut() {
        if !sys.multi {
            let (top, bottom, reach_hi, page) = (sys.top, sys.bottom, sys.reach_hi, sys.page);
            let line = &mut sys.lines[0];
            line.top = top;
            line.bottom = bottom;
            line.reach_hi = reach_hi;
            line.page = page;
        }
    }
    pages
}

fn system_reach(sys: &System, staves: &[Vec<Bar>]) -> (f64, f64) {
    if let Some(r) = sys.reach {
        return r;
    }
    reach(&staves[0], &sys.lines[0])
}

// ---- The whole --------------------------------------------------------------

pub(crate) fn engrave_parsed(parsed: &Parsed, mut width: f64, layout: &Layout) -> Result<Engraved, ScoreError> {
    if parsed.staves.len() > 1 {
        return engrave_many(parsed, width, layout);
    }
    let mut bars = measures(&parsed.staves[0].events)?;
    beams(&mut bars)?;
    let sh = shortest(bars.iter());
    columns(&mut bars, sh);
    let mut left = layout.left.unwrap_or(0.0);
    let indent = layout.indent.unwrap_or(0.0);
    let mut lines = break_bars(&mut bars, width, indent, 0.0);
    if layout.left.is_none() && lines.len() > 1 {
        let last_number = bars[bars.len() - 1].number;
        left = text_width(&last_number.to_string(), Weight::Regular, em(-2.0)) + 0.2;
        width -= left;
        for bar in &mut bars {
            bar.bar_drawn = None;
            bar.open_repeat = None;
        }
        lines = break_bars(&mut bars, width, indent, 0.0);
    }
    place(&mut bars, &mut lines, left, indent, 0.0);
    curves(&mut bars, &mut lines);
    marks::marks(&mut bars, &mut lines);
    let head = header_lines(&parsed.header, width);
    let mut systems: Vec<System> = lines.into_iter().map(|line| System { left: line.left, right: line.right, lines: vec![line], ..System::default() }).collect();
    let staves = vec![bars];
    let pages = stack(&mut systems, &staves, layout, &head);
    let bottom = systems.iter().fold(0.0, |acc: f64, s| acc.max(s.bottom));
    Ok(Engraved { staves, systems, height: bottom + 1.0, width, left, header: head, pages, paper: layout.page_h, numbered: layout.page_h.is_some() && layout.top.is_some(), groups: Vec::new() })
}

fn master_bar(all: &[Vec<Bar>], k: usize, shortest: i64) -> Bar {
    let first = &all[0][k];
    let mut mb = first.clone();
    mb.items = Vec::new();
    mb.beams = Vec::new();
    mb.staff_openings = all.iter().map(|bars| bars[k].opening()).collect();
    let mut starts: Vec<i64> = Vec::new();
    for bars in all {
        for ev in &bars[k].items {
            if !starts.contains(&ev.start) {
                starts.push(ev.start);
            }
        }
    }
    starts.sort_unstable();
    let length = first.length * first.span.unwrap_or(1);
    for (ci, &at) in starts.iter().enumerate() {
        let upto = starts.get(ci + 1).copied().unwrap_or(length);
        let mut col = Ev { k: Kind::Col, start: at, ticks: upto - at, ..Ev::default() };
        let mut ideal: Option<f64> = None;
        for (s, bars) in all.iter().enumerate() {
            let items = &bars[k].items;
            for (j, ev) in items.iter().enumerate() {
                if ev.start != at {
                    continue;
                }
                col.members.push((s, j));
                let own = ev.ticks * ev.mm.unwrap_or(1);
                let want = if own <= upto - at { ev.ideal } else { duration_space(upto - at, shortest) };
                ideal = Some(ideal.map_or(want, |v| v.min(want)));
                col.left = col.left.max(ev.left);
                let after = items.get(j + 1).map_or(length, |n| n.start);
                if after == upto {
                    col.right = col.right.max(ev.right);
                }
            }
        }
        col.ideal = ideal.unwrap_or(0.0);
        mb.items.push(col);
    }
    mb
}

fn groups_width(groups: &[Group], count: usize) -> f64 {
    let mut widest: f64 = 0.0;
    for g in groups {
        if g.kind == GroupKind::None {
            continue;
        }
        widest = widest.max((g.depth + 1) as f64 * 1.6);
    }
    widest + if count > 1 { 0.3 } else { 0.0 }
}

fn spread(master: &[Bar], staves: &mut [Vec<Bar>], range: Range<usize>) {
    for b in range.clone() {
        let mb = &master[b];
        let content0 = if mb.items.is_empty() { mb.x0 } else { mb.items[0].x - 1.2 - mb.items[0].left };
        for bars in staves.iter_mut() {
            let sb = &mut bars[b];
            sb.x0 = mb.x0;
            sb.x1 = mb.x1;
            sb.bar_drawn = mb.bar_drawn.clone();
            sb.open_repeat = mb.open_repeat.clone();
            sb.open_repeat_x = mb.open_repeat_x;
        }
        for col in &mb.items {
            for &(s, j) in &col.members {
                staves[s][b].items[j].x = col.x;
            }
        }
        for bars in staves.iter_mut() {
            let sb = &mut bars[b];
            let length = sb.length;
            if sb.items.len() == 1 && sb.items[0].k == Kind::Rest && (sb.items[0].full || sb.items[0].ticks == length) {
                let it = &mut sb.items[0];
                it.x = (content0 + mb.x1) / 2.0 - glyph("restWhole").advance / 2.0;
                it.whole_bar = true;
                it.span0 = content0;
                it.span1 = mb.x1;
            }
        }
    }
    for bars in staves.iter_mut() {
        stems_beams(&mut bars[range.clone()]);
    }
}

fn engrave_many(parsed: &Parsed, width: f64, layout: &Layout) -> Result<Engraved, ScoreError> {
    let staves_src = &parsed.staves;
    let mut all: Vec<Vec<Bar>> = Vec::new();
    for stf in staves_src {
        all.push(measures(&stf.events)?);
    }
    let count = all[0].len();
    for (si, bars) in all.iter().enumerate() {
        if bars.len() != count {
            return refuse(format!("staff {} has {} bars and the first has {count}: every staff has the same bars", si + 1, bars.len()), 0);
        }
        for (k, bar) in bars.iter().enumerate() {
            if bar.length * bar.span.unwrap_or(1) != all[0][k].length * all[0][k].span.unwrap_or(1) {
                return refuse(format!("bar {} is not as long in staff {} as in the first", k + 1, si + 1), 0);
            }
        }
    }
    for bars in &mut all {
        beams(bars)?;
    }
    let sh = shortest(all.iter().flatten());
    for bars in &mut all {
        columns(bars, sh);
    }
    let mut master: Vec<Bar> = (0..count).map(|k| master_bar(&all, k, sh)).collect();
    let deco = groups_width(&parsed.groups, staves_src.len());
    let e = em(0.0);
    let names: Vec<String> = staves_src.iter().map(|s| s.props.instrument_name.clone().unwrap_or_default()).chain(parsed.groups.iter().map(|g| g.name.clone())).collect();
    let shorts: Vec<String> = staves_src.iter().map(|s| s.props.short_instrument_name.clone().unwrap_or_default()).chain(parsed.groups.iter().map(|g| g.short.clone())).collect();
    let name_w = names.iter().map(|nm| if nm.is_empty() { 0.0 } else { text_width(nm, Weight::Regular, e) + 1.0 }).fold(0.0, f64::max);
    let short_w = shorts.iter().map(|nm| if nm.is_empty() { 0.0 } else { text_width(nm, Weight::Regular, e) + 1.0 }).fold(0.0, f64::max);
    let left = layout.left.unwrap_or(0.0);
    let indent = layout.indent.unwrap_or(0.0).max(deco + name_w - left);
    let rest = deco.max(deco + short_w - left);
    let mut lines = break_bars(&mut master, width, indent, rest);
    place(&mut master, &mut lines, left, indent, rest);
    let mut systems: Vec<System> = Vec::new();
    let n_lines = lines.len();
    for si in 0..n_lines {
        let range = lines[si].bars.clone();
        spread(&master, &mut all, range.clone());
        let line = &lines[si];
        let mut staff_lines: Vec<Line> = Vec::new();
        for (s, bars) in all.iter().enumerate() {
            let mut ss = Line {
                bars: range.clone(),
                staff_index: Some(s),
                left: line.left,
                prefix: line.prefix,
                right: line.right,
                bar_end: line.bar_end,
                k: line.k,
                courtesy: line.courtesy,
                ..Line::default()
            };
            if si + 1 < n_lines && line.courtesy > 0.0 {
                let next_first = lines[si + 1].bars.start;
                ss.courtesy_bar = Some(courtesy_of(&bars[next_first]));
            }
            staff_lines.push(ss);
        }
        let mut clef_w: f64 = 0.0;
        let mut key_w: f64 = 0.0;
        for bars in &all {
            let first = &bars[range.start];
            clef_w = clef_w.max(glyph(clef_glyph(first.clef)).advance);
            key_w = key_w.max(if first.fifths == 0 { 0.0 } else { key_width(first.fifths) });
        }
        for ss in &mut staff_lines {
            ss.clef_w = Some(clef_w);
            ss.key_w = Some(key_w);
        }
        systems.push(System { lines: staff_lines, multi: true, left: line.left, right: line.right, first_line: si == 0, ..System::default() });
    }
    for (s, bars) in all.iter_mut().enumerate() {
        let mut staff_lines: Vec<Line> = systems.iter().map(|o| o.lines[s].clone()).collect();
        curves(bars, &mut staff_lines);
        marks::marks(bars, &mut staff_lines);
        for (o, l) in systems.iter_mut().zip(staff_lines) {
            o.lines[s] = l;
        }
    }
    for o in &mut systems {
        stack_staves(o, &all, &parsed.groups, staves_src);
    }
    let head = header_lines(&parsed.header, width);
    let pages = stack(&mut systems, &all, layout, &head);
    let mut bottom: f64 = 0.0;
    for o in &mut systems {
        let (top, page) = (o.top, o.page);
        let offsets = o.offsets.clone();
        for (s, ss) in o.lines.iter_mut().enumerate() {
            ss.top = top + offsets[s];
            ss.page = page;
        }
        bottom = bottom.max(o.bottom);
    }
    Ok(Engraved {
        staves: all,
        systems,
        height: bottom + 1.0,
        width,
        left,
        header: head,
        pages,
        paper: layout.page_h,
        numbered: layout.page_h.is_some() && layout.top.is_some(),
        groups: parsed.groups.clone(),
    })
}

fn stack_staves(sys: &mut System, all: &[Vec<Bar>], groups: &[Group], staves: &[crate::parse::Staff]) {
    let reaches: Vec<(f64, f64)> = sys.lines.iter().enumerate().map(|(s, l)| reach(&all[s], l)).collect();
    let mut offsets = vec![0.0];
    for s in 1..reaches.len() {
        let o = (offsets[s - 1] + 9.0_f64).max(offsets[s - 1] + reaches[s - 1].1 + 1.0 - reaches[s].0);
        offsets.push(o);
    }
    let last = offsets.len() - 1;
    sys.reach = Some((reaches[0].0, offsets[last] + reaches[last].1));
    sys.names = staves.iter().map(|stf| if sys.first_line { stf.props.instrument_name.clone().unwrap_or_default() } else { stf.props.short_instrument_name.clone().unwrap_or_default() }).collect();
    for (s, ss) in sys.lines.iter_mut().enumerate() {
        let through = groups.iter().any(|g| (g.kind == GroupKind::Brace || g.kind == GroupKind::Bracket) && g.first <= s && s < g.last);
        ss.connect_down = if through { offsets[s + 1] - offsets[s] - 4.0 } else { 0.0 };
    }
    sys.offsets = offsets;
}

/// The tempo marks for a test's eyes: unused otherwise.
#[allow(dead_code)]
pub(crate) fn tempo_of(ev: &Ev) -> Option<&Tempo> {
    ev.tempo.as_ref()
}
