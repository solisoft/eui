//! Laying a score out: beams, spacing, lines, places, stems, staves and
//! pages (`eui_builders_score.sl`, "Stems and beams" to "Placing" and
//! "Several staves"). Function for function the Soli engraver's, under
//! the same names without their `score_` prefix.

use std::ops::Range;

use crate::breaking::{line_starts, Opts, Widths};
use crate::glyphs::{glyph, Weight};
use crate::marks::{self, curves, reach, DynEl, HairpinPiece, MarkPlace, OttavaPlace, ScriptPlace, SpanPlace, TempoMark, TextPlace, TupletPlace, Volta};
use crate::measure::{measures, measures_keeping, Bar, Beam, Changes, Opening};
use crate::parse::{refuse, Clef, Ev, Group, GroupKind, Kind, Parsed, Sign, StaffKind, Syllable, Tempo, GUITAR};
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
    /// The baseline its syllables stand on, when it has any.
    pub lyric_y: Option<f64>,
    /// How many lines of lyrics it holds: its verses, one under another.
    pub lyric_lines: usize,
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
    /// Glissandi: from `(x, y)` to `(x, y)`.
    pub glissandi: Vec<[f64; 4]>,
    /// `\ottava`'s brackets.
    pub ottavas: Vec<OttavaPlace>,
    /// Rehearsal marks, segni and codas.
    pub rehearsal: Vec<MarkPlace>,
    /// What kind of staff it is.
    pub kind: StaffKind,
    /// Text spanners, `\startTextSpan` … `\stopTextSpan`.
    pub spans: Vec<SpanPlace>,
    /// Left out of its system: an empty staff `\RemoveEmptyStaves` removes.
    pub hidden: bool,
    /// The title over it when a movement starts with it: `piece`, `opus`.
    pub title: Option<(String, String)>,
    /// Where they start: the piece's left, the opus's right.
    pub title_x: (f64, f64),
    /// Their baseline, once placed.
    pub title_y: Option<f64>,
}

/// How tall a staff of a kind is, from its top line to its bottom one.
pub(crate) fn staff_height(kind: StaffKind) -> f64 {
    match kind {
        StaffKind::Tab => 7.5,
        _ => 4.0,
    }
}

/// The size of a chord's name: LilyPond's `font-size` 1.5.
pub(crate) fn chord_em() -> f64 {
    em(1.5)
}

/// The baseline chord names stand on, in their line's frame.
pub(crate) const CHORD_BASE: f64 = 3.0;

/// The size of a figured bass's figures.
pub(crate) fn figure_em() -> f64 {
    em(-1.0)
}

/// A figure's first baseline in its line's frame, and how far each line of
/// it is under the one above.
pub(crate) fn figure_line(k: usize) -> f64 {
    let step = crate::draw::digit_height(figure_em()) * 1.25;
    step + 0.2 + step * k as f64
}

/// A figure's width: its widest line.
pub(crate) fn figure_width(ev: &Ev) -> f64 {
    ev.figures.iter().map(|line| marks::runs_width(line, figure_em())).fold(0.0, f64::max)
}

/// The size of a tablature's frets, which LilyPond sets in bold.
pub(crate) fn fret_em() -> f64 {
    em(-1.5)
}

/// Each head of a tablature's note on a string, with its fret: the string
/// written (`\\5`), or the highest string the note reaches with a fret
/// of 0 or more not taken by another head of the chord, the highest note
/// first.
pub(crate) fn tab_frets(ev: &mut Ev) {
    let mut used: Vec<i64> = Vec::new();
    let mut out = Vec::new();
    for h in ev.heads.iter().rev() {
        let key = crate::parse::semitones(h.d, h.alter) + 48;
        // A string that cannot sound the note is not the one it is played on.
        let written = ev.strings.iter().find(|s| s.0 == h.d).map(|s| s.1).filter(|&s| !used.contains(&s) && key >= GUITAR[(s - 1) as usize]);
        let string = written.or_else(|| (1..=6).find(|&s| !used.contains(&s) && key >= GUITAR[(s - 1) as usize]));
        let Some(s) = string else { continue };
        used.push(s);
        out.push((s, key - GUITAR[(s - 1) as usize]));
    }
    ev.tab = out;
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
    /// What crosses between its staves — kneed beams and their stems,
    /// curves from one staff to the other, a voice's staff-switch line — as
    /// fill commands in its frame: y down from its first staff's top line.
    pub cross: Vec<Vec<f64>>,
    /// Its kneed beams, placed.
    pub kneed: Vec<crate::cross::KneedBeam>,
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
    /// The room a page's header takes over the titles (`Layout::head_room`).
    pub head_room: f64,
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
        64 => 4,
        128 => 5,
        _ => 0,
    }
}

fn stem_up(ev: &Ev) -> bool {
    let top = ev.heads.last().map_or(0, |h| h.pos);
    let low = ev.heads.first().map_or(0, |h| h.pos);
    low + top < 0
}

/// The windows a bar's beams follow: its metre's — and where it shows
/// another over bars of the same length (`timeSignatureFraction` with
/// `\scaleDurations`), the shown metre's, shrunk or stretched to the bar:
/// 9/8 over 3/4 beams its scaled eighths three by three.
pub(crate) fn bar_windows(bar: &Bar) -> (i64, i64) {
    let Some(real) = bar.beats else { return beam_windows(bar.time) };
    let (a, b) = beam_windows(bar.time);
    let (real_len, shown_len) = (real.0 * WHOLE / real.1, bar.time.0 * WHOLE / bar.time.1);
    if shown_len == 0 || (a * real_len) % shown_len != 0 || (b * real_len) % shown_len != 0 {
        return beam_windows(real);
    }
    (a * real_len / shown_len, b * real_len / shown_len)
}

pub(crate) fn beam_windows(time: (i64, i64)) -> (i64, i64) {
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

/// A bar's notes and rests a voice at a time, by index: everything but the
/// second voice, then the second — one list for a bar of one voice.
pub(crate) fn voice_lists(bar: &Bar) -> Vec<Vec<usize>> {
    let all = 0..bar.items.len();
    if !bar.poly {
        return vec![all.collect()];
    }
    // One list a voice, in their order; a note outside the voices goes with the first.
    let mut lists: Vec<Vec<usize>> = Vec::new();
    for v in 1u8..=4 {
        let list: Vec<usize> = all.clone().filter(|&j| bar.items[j].voice.unwrap_or(1) == v).collect();
        if !list.is_empty() {
            lists.push(list);
        }
    }
    lists
}

/// Whether a staff of a kind beams its notes: a tablature shows no rhythm,
/// and chord names have no stems.
fn beamed(kind: StaffKind) -> bool {
    matches!(kind, StaffKind::Notes | StaffKind::Drums)
}

pub(crate) fn beams(bars: &mut [Bar]) -> Result<(), ScoreError> {
    for bar in bars.iter_mut() {
        let wins = bar_windows(bar);
        // `\set Timing.beatStructure`, `baseMoment`, `beamExceptions`: the
        // groups the beams end at, the bar's first note's settings say.
        let groups = bar.items.first().and_then(|ev| crate::look::beam_groups(&ev.look, bar.metre()));
        let mut all: Vec<Vec<usize>> = Vec::new();
        for list in voice_lists(bar) {
            all.extend(beam_voice(bar, &list, wins, groups.as_deref())?);
        }
        // In the order of their first notes, as the drawing meets them.
        all.sort_by_key(|g| g[0]);
        for g in all {
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
            let mut up = -lowest > highest;
            // A note moved to a staff that plays nothing with it stems by its place.
            if let Some(v) = bar.items[g[0]].voice.filter(|_| !bar.items[g[0]].free) {
                up = v % 2 == 1;
            }
            // `Beam.positions` set and no direction: the stems go to the beam.
            if let Some((l, r)) = bar.items[g[0]].look.pair("Beam.positions").filter(|_| bar.items[g[0]].voice.is_none()) {
                let mid = f64::from((lowest + highest) as i32) / 4.0;
                up = (l + r) / 2.0 > mid;
            }
            // `\stemUp`, `\stemDown`: over the voice's direction too.
            if bar.items[g[0]].stem_dir != 0 {
                up = bar.items[g[0]].stem_dir > 0;
            }
            let gi = bar.beams.len();
            for &j in &g {
                bar.items[j].beam = Some(gi);
                bar.items[j].up = up;
            }
            bar.beams.push(Beam { notes: g, up, ..Beam::default() });
        }
        for ev in &mut bar.items {
            if ev.k != Kind::Note {
                continue;
            }
            match ev.voice.filter(|_| !ev.free) {
                Some(v) => ev.up = v % 2 == 1,
                None if ev.beam.is_none() => ev.up = stem_up(ev),
                None => {}
            }
            if ev.stem_dir != 0 && ev.beam.is_none() {
                ev.up = ev.stem_dir > 0;
            }
        }
    }
    Ok(())
}

/// One voice's beam groups in a bar, by the metre's windows, `[ ]` first:
/// the groups of two notes or more, by index into the bar's items. A beam
/// stops where the voice is silent between two notes — a second voice's
/// spacer (`s`) is no item, and two eighths either side of one are not
/// neighbours.
pub(crate) fn beam_voice(bar: &Bar, list: &[usize], wins: (i64, i64), bounds: Option<&[i64]>) -> Result<Vec<Vec<usize>>, ScoreError> {
    let window = |pos: i64| match bounds {
        Some(g) => crate::look::window_in(g, pos),
        None => pos / wins.0,
    };
    {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut cur: Vec<usize> = Vec::new();
        let mut manual = false;
        let mut win = -1;
        let mut last_end = -1;
        for &j in list {
            let ev = &bar.items[j];
            let gap = last_end >= 0 && ev.start != last_end;
            last_end = ev.start + ev.ticks;
            // A note under a beam that crosses the staves is the system's to beam.
            let fl = if ev.k == Kind::Note && !ev.xbeam { flags(ev.base) } else { 0 };
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
            // A cadenza has no beats to beam by, and `\autoBeamOff` beams by
            // none: only `[ ]` beams there.
            let fl = if bar.cadenza || ev.no_autobeam { 0 } else { fl };
            let w = window(ev.start + bar.offset);
            if (fl == 0 || w != win || gap) && !cur.is_empty() {
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
                    let w = if bounds.is_some() { window(ev.start + bar.offset) } else { (ev.start + bar.offset) / wins.1 };
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
        Ok(by_beat.into_iter().filter(|g| g.len() >= 2).collect())
    }
}

// ---- Columns -----------------------------------------------------------

/// The room a duration of `t` ticks wants, the commonest shortest being
/// `shortest`: LilyPond's `get_duration_space`, `spacing-increment` a
/// doubling and `shortest-duration-space` the shortest's, in increments.
pub(crate) fn duration_space(t: i64, shortest: i64) -> f64 {
    let d = crate::breaking::durations();
    let ratio = t as f64 / shortest as f64;
    if ratio < 1.0 {
        return d.incr * (d.k - 1.0 + ratio) + 0.1;
    }
    d.incr * (d.k + log2(ratio)) + 0.1
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
    best.min(crate::breaking::durations().base)
}

fn space_ticks(ev: &Ev) -> i64 {
    let mut t = WHOLE / ev.base.max(1);
    for tp in [ev.tuplet, ev.tuplet_outer].into_iter().flatten() {
        t = t * tp.d / tp.n;
    }
    t
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
        32 => "rest32nd",
        64 => "rest64th",
        _ => "rest128th",
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
        32 => "32nd",
        64 => "64th",
        _ => "128th",
    };
    format!("flag{stem}{}", if up { "Up" } else { "Down" })
}

/// Grace notes' size against a note's: LilyPond's `font-size` -3.
pub(crate) const GRACE_K: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Where a note's grace notes go: left of it and of everything it has
/// there, each at its x from the note's, stems up (down in a second voice),
/// beamed when every one has a flag, the stems of a beam ending together.
/// The note's room on the left grows by theirs.
fn grace_layout(ev: &mut Ev) {
    if ev.graces.is_empty() {
        return;
    }
    let k = GRACE_K;
    let beamed = ev.graces.len() > 1 && ev.graces.iter().all(|g| flags(g.base) > 0);
    let mut x = 0.0;
    let mut xs = Vec::with_capacity(ev.graces.len());
    for g in &ev.graces {
        let acc_w = g.heads.iter().filter_map(|h| h.acc).map(|a| glyph(acc_name(a)).advance).fold(0.0, f64::max) * k;
        if acc_w > 0.0 {
            x += acc_w + 0.15;
        }
        xs.push(x);
        x += glyph(head_name(g.base)).advance * k;
        if !beamed && flags(g.base) > 0 && g.up {
            x += (glyph(&flag_name(g.base, true)).advance - RULE.stem) * k;
        }
        x += 0.3;
    }
    let room = x + 0.2;
    let before = ev.left;
    let len = 3.0 * k;
    let top = |g: &Ev| y_of(g.heads.last().map_or(0, |h| h.pos));
    let low = |g: &Ev| y_of(g.heads.first().map_or(0, |h| h.pos));
    let up = ev.graces[0].up;
    let tip = if up { ev.graces.iter().map(|g| top(g) - len).fold(f64::INFINITY, f64::min) } else { ev.graces.iter().map(|g| low(g) + len).fold(f64::NEG_INFINITY, f64::max) };
    for (g, gx) in ev.graces.iter_mut().zip(xs) {
        g.x = gx - before - room;
        g.head_w = Some(glyph(head_name(g.base)).advance * k);
        g.stem_end = if beamed {
            tip
        } else if up {
            top(g) - len
        } else {
            low(g) + len
        };
        g.beam = beamed.then_some(0);
    }
    ev.left += room;
}

/// Where `\afterGrace`'s notes go: right of the note and all it has
/// there, a little apart, laid out as grace notes before a note are; the
/// note's room on the right grows by theirs. The drawing moves them up to
/// the next note when the spacing leaves more room (`draw::after_shift`).
fn after_layout(ev: &mut Ev) {
    if ev.after_graces.is_empty() {
        return;
    }
    let k = GRACE_K;
    let beamed = ev.after_graces.len() > 1 && ev.after_graces.iter().all(|g| flags(g.base) > 0);
    let start = ev.right + 0.5;
    let mut x = 0.0;
    let mut xs = Vec::with_capacity(ev.after_graces.len());
    for g in &ev.after_graces {
        let acc_w = g.heads.iter().filter_map(|h| h.acc).map(|a| glyph(acc_name(a)).advance).fold(0.0, f64::max) * k;
        if acc_w > 0.0 {
            x += acc_w + 0.15;
        }
        xs.push(x);
        x += glyph(head_name(g.base)).advance * k;
        if !beamed && flags(g.base) > 0 && g.up {
            x += (glyph(&flag_name(g.base, true)).advance - RULE.stem) * k;
        }
        x += 0.3;
    }
    let len = 3.0 * k;
    let top = |g: &Ev| y_of(g.heads.last().map_or(0, |h| h.pos));
    let low = |g: &Ev| y_of(g.heads.first().map_or(0, |h| h.pos));
    let up = ev.after_graces[0].up;
    let tip = if up { ev.after_graces.iter().map(|g| top(g) - len).fold(f64::INFINITY, f64::min) } else { ev.after_graces.iter().map(|g| low(g) + len).fold(f64::NEG_INFINITY, f64::max) };
    for (g, gx) in ev.after_graces.iter_mut().zip(xs) {
        g.x = start + gx;
        g.head_w = Some(glyph(head_name(g.base)).advance * k);
        g.stem_end = if beamed {
            tip
        } else if up {
            top(g) - len
        } else {
            low(g) + len
        };
        g.beam = beamed.then_some(0);
    }
    ev.right = start + x;
}

fn columns(bars: &mut [Bar], shortest: i64, kind: StaffKind) {
    for bar in bars.iter_mut() {
        for ev in &mut bar.items {
            let t = space_ticks(ev);
            ev.ideal = duration_space(t, shortest);
            if kind == StaffKind::Figures {
                // A figure, centred on its note: the room it is wide.
                let (hw, w) = (glyph("noteheadBlack").advance, figure_width(ev));
                ev.head_w = Some(hw);
                ev.left = if w > 0.0 { ((w - hw) / 2.0).max(0.0) + 0.2 } else { 0.0 };
                ev.right = if w > 0.0 { hw + ((w - hw) / 2.0).max(0.0) + 0.2 } else { 0.0 };
                continue;
            }
            if matches!(kind, StaffKind::ChordNames | StaffKind::FretBoards) {
                // A name takes the room it is wide.
                ev.left = 0.0;
                ev.right = if ev.chord_name.is_empty() { 0.0 } else { marks::runs_width(&ev.chord_name, chord_em()) + 0.6 };
                ev.head_w = Some(glyph("noteheadBlack").advance);
                continue;
            }
            if kind == StaffKind::Tab && ev.k == Kind::Note {
                tab_frets(ev);
                let hw = glyph("noteheadBlack").advance;
                let w = ev.tab.iter().map(|t| text_width(&t.1.to_string(), Weight::Bold, fret_em())).fold(0.0, f64::max);
                ev.head_w = Some(hw);
                ev.left = (w - hw).max(0.0) / 2.0 + 0.2;
                ev.right = hw + (w - hw).max(0.0) / 2.0 + 0.2;
                continue;
            }
            if kind == StaffKind::Tab {
                ev.left = 0.0;
                ev.right = 0.0;
                continue;
            }
            if ev.k == Kind::Rest {
                if let Some(mm) = ev.mm {
                    ev.ideal = if mm <= 10 { 3.0 + church_width(mm) } else { 7.0 + log2(mm as f64) };
                    ev.left = 0.0;
                    ev.right = if mm <= 10 { church_width(mm) + 1.0 } else { 4.0 };
                    continue;
                }
                let g = glyph(rest_name(ev.base));
                let k = mag(ev);
                ev.left = 0.0;
                ev.right = (g.advance + if ev.dots > 0 { 0.3 + 0.6 * ev.dots as f64 } else { 0.0 }) * k;
                breath_room(ev);
                grace_layout(ev);
                change_room(ev);
                continue;
            }
            let k = mag(ev);
            // `\override NoteHead.style`: its head's own width.
            let hw = glyph(crate::look::head_of(ev).unwrap_or(head_name(ev.base))).advance * k;
            ev.head_w = Some(hw);
            displace(ev);
            let mut right = hw;
            if ev.heads.iter().any(|h| h.side == 1) {
                right += hw - RULE.stem;
            }
            if ev.dots > 0 {
                right += (0.3 + 0.6 * ev.dots as f64) * k;
            }
            if ev.up && ev.beam.is_none() && !ev.xbeam && !ev.span_stem && flags(ev.base) > 0 {
                right += glyph(&flag_name(ev.base, true)).advance * k - RULE.stem;
            }
            ev.right = right;
            if ev.lv {
                ev.right = ev.right.max(hw + 0.15 + marks::SHORT_TIE + 0.2);
            }
            breath_room(ev);
            after_layout(ev);
            let shifted = ev.heads.iter().any(|h| h.side == -1);
            let lefts = if shifted { hw - RULE.stem } else { 0.0 };
            ev.left = lefts + place_accidentals(ev);
            if ev.repeat_tie {
                ev.left += marks::SHORT_TIE + 0.3;
            }
            if ev.arpeggio {
                // The wavy line before the chord and its accidentals.
                let w = glyph("wiggleArpeggiatoUp").advance;
                ev.left += 0.3 + w;
                ev.arp_x = Some(-ev.left);
            }
            grace_layout(ev);
            change_room(ev);
            lyric_room(ev);
        }
        if beamed(kind) {
            let lists = voice_lists(bar);
            crate::breaking::optical(bar, &lists);
        }
        if bar.poly {
            rest_collisions(bar);
            poly_cols(bar, shortest);
        }
    }
}

// ---- Lyrics -----------------------------------------------------------------
//
// `\addlyrics { … }` after a staff's music, as LilyPond sets it: a syllable
// a note of the first voice, centred on the note's head, on one line under
// the staff and all its marks; `--` a hyphen centred between a word's
// syllables, `__` a line under the notes a syllable is held over. The
// notes make room for their syllables, as LilyPond's spacing does.

/// The syllables onto the notes. A note tied from the one before, or under
/// a slur after its first, holds the syllable before — a melisma — and
/// takes none; rests take none; syllables past the last note are dropped,
/// as LilyPond drops them.
fn lyrics_assign(bars: &mut [Bar], sylls: Option<&[Syllable]>, voice: u8) {
    verse_assign(bars, sylls, voice, 0);
}

/// Verse `verse` (0 the first) onto the notes, as [`lyrics_assign`] does.
fn verse_assign(bars: &mut [Bar], sylls: Option<&[Syllable]>, voice: u8, verse: usize) {
    let Some(sylls) = sylls else { return };
    let mut k = 0;
    let mut tied = false;
    let mut slurred = false;
    for bar in bars.iter_mut() {
        for ev in &mut bar.items {
            if marks::voice_of(ev) != voice || ev.k != Kind::Note || ev.percent.is_some() {
                continue;
            }
            if tied || slurred {
                ev.lyric_held = true;
            } else if let Some(s) = sylls.get(k) {
                if verse == 0 {
                    ev.lyric = Some(s.clone());
                } else {
                    if ev.verses.len() < verse {
                        ev.verses.resize(verse, None);
                    }
                    ev.verses[verse - 1] = Some(s.clone());
                }
                k += 1;
            }
            tied = ev.tie;
            slurred = (slurred || ev.slur_on) && !ev.slur_off;
        }
    }
}

/// A syllable's room: half its width either side of the head's middle and
/// LilyPond's 0.35 of word space beyond, more after it when a hyphen
/// follows.
fn lyric_room(ev: &mut Ev) {
    // The widest syllable of every verse sung on the note.
    let sung: Vec<Syllable> = ev.lyric.iter().chain(ev.verses.iter().flatten()).filter(|l| !l.text.is_empty()).cloned().collect();
    for lyr in sung {
        let w = text_width(&lyr.text, Weight::Regular, em(0.0));
        let c = ev.head_w.unwrap_or(0.0) / 2.0;
        let after = if lyr.hyphen { 0.9 } else { 0.35 };
        ev.right = ev.right.max(c + w / 2.0 + after);
        ev.left = ev.left.max(w / 2.0 - c + 0.35);
    }
}

/// Rests of two voices clear of the other voice's notes, as LilyPond's
/// rest collision moves them: a first voice's rest stands two spaces up,
/// higher by whole spaces while a note of the second sounding with it
/// reaches it — a high C under a sixteenth rest drew on top of it — and a
/// second voice's two spaces down, lower the same way.
fn rest_collisions(bar: &mut Bar) {
    let notes: Vec<(u8, i64, i64, f64, f64)> = bar
        .items
        .iter()
        .filter(|e| e.k == Kind::Note && !e.heads.is_empty())
        .map(|e| {
            let top = y_of(e.heads.last().map_or(0, |h| h.pos)) - 0.5;
            let bottom = y_of(e.heads.first().map_or(0, |h| h.pos)) + 0.5;
            (marks::voice_of(e), e.start, e.start + e.ticks, top, bottom)
        })
        .collect();
    for ev in bar.items.iter_mut().filter(|e| e.k == Kind::Rest && !e.full && e.voice.is_some()) {
        let voice = marks::voice_of(ev);
        let (a, b) = (ev.start, ev.start + ev.ticks);
        let others = notes.iter().filter(|n| n.0 != voice && n.1 < b && n.2 > a);
        // The rest's ink, as its extent counts it, is 1 to 3 spaces down
        // from the top line, moved by its lift; half a space more is clear.
        if voice == 1 {
            let top = others.map(|n| n.3).fold(f64::MAX, f64::min);
            if top < f64::MAX {
                ev.rest_y = Some((-2.0_f64).min((top - 3.5).floor()));
            }
        } else {
            let bottom = others.map(|n| n.4).fold(f64::MIN, f64::max);
            if bottom > f64::MIN {
                ev.rest_y = Some(2.0_f64.max((bottom - 0.5).ceil()));
            }
        }
    }
}

/// What a bar is spaced by: its notes and rests, or, with two voices, its
/// columns.
pub(crate) fn spacers(bar: &Bar) -> &[Ev] {
    if bar.cols.is_empty() {
        &bar.items
    } else {
        &bar.cols
    }
}

/// A bar of two voices in columns, each voice read on its own for what
/// follows a note; where the voices' heads meet — a unison, a second, or
/// voices that cross — the first voice's note stands a head to the right.
fn poly_cols(bar: &mut Bar, shortest: i64) {
    let mut starts: Vec<i64> = Vec::new();
    for ev in &bar.items {
        if !starts.contains(&ev.start) {
            starts.push(ev.start);
        }
    }
    starts.sort_unstable();
    let length = bar.length;
    let voices = voice_lists(bar);
    let mut cols = Vec::new();
    for (ci, &at) in starts.iter().enumerate() {
        let upto = starts.get(ci + 1).copied().unwrap_or(length);
        let mut col = Ev { k: Kind::Col, start: at, ticks: upto - at, ..Ev::default() };
        let mut ideal: Option<f64> = None;
        for list in &voices {
            for (jj, &j) in list.iter().enumerate() {
                let ev = &bar.items[j];
                if ev.start != at {
                    continue;
                }
                col.members.push((0, j));
                let want = if ev.ticks <= upto - at { ev.ideal } else { duration_space(upto - at, shortest) };
                ideal = Some(ideal.map_or(want, |v| v.min(want)));
                col.left = col.left.max(ev.left);
                col.pre = col.pre.max(ev.pre);
                let after = list.get(jj + 1).map_or(length, |&n| bar.items[n].start);
                if after == upto {
                    col.right = col.right.max(ev.right);
                }
            }
        }
        col.ideal = ideal.unwrap_or(0.0);
        // The stem-up voices (first, third) against the stem-down (second, fourth).
        let note = |j: usize, two: bool| bar.items[j].k == Kind::Note && (bar.items[j].voice.is_some_and(|v| v % 2 == 0)) == two;
        let ones: Vec<usize> = col.members.iter().map(|m| m.1).filter(|&j| note(j, false)).collect();
        let twos: Vec<usize> = col.members.iter().map(|m| m.1).filter(|&j| note(j, true)).collect();
        // They meet when the first voice's lowest head comes within a step
        // of the second's highest: a second, a unison, or voices that cross,
        // the stem-up one under the stem-down one — their stems would run
        // through each other's heads.
        let low = |j: usize| bar.items[j].heads.first().map_or(0, |h| h.pos);
        let high = |j: usize| bar.items[j].heads.last().map_or(0, |h| h.pos);
        let meet = ones.iter().any(|&a| twos.iter().any(|&b| low(a) - high(b) <= 1));
        if meet {
            for &a in &ones {
                let ev = &mut bar.items[a];
                let hw = ev.head_w.unwrap_or(0.0);
                ev.shift = Some(hw);
                col.right = col.right.max(ev.right + hw);
            }
        }
        // A third or fourth voice stands a head to the right of the first two
        // where they sound together, as LilyPond's \voiceThree and \voiceFour do.
        let inner = col.members.iter().any(|m| bar.items[m.1].k == Kind::Note && bar.items[m.1].voice.is_none_or(|v| v <= 2));
        if inner {
            for m in col.members.clone() {
                let ev = &mut bar.items[m.1];
                if ev.k == Kind::Note && ev.voice.is_some_and(|v| v >= 3) {
                    let hw = ev.head_w.unwrap_or(1.2);
                    let at = ev.shift.unwrap_or(0.0) + hw;
                    ev.shift = Some(at);
                    col.right = col.right.max(ev.right + at);
                }
            }
        }
        cols.push(col);
    }
    bar.cols = cols;
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
        let mut g = glyph(acc_name(acc)).clone();
        if ev.heads[hk].paren {
            g.advance = paren_width(acc);
        }
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

/// A reminder accidental's width, `cis?`: the sign between parentheses, as
/// LilyPond's `accidentals.leftparen` and `rightparen` hold it — the left
/// one reaching back from the sign's origin, the right one on from its
/// edge.
pub(crate) fn paren_width(acc: i64) -> f64 {
    PAREN_LEFT + glyph(acc_name(acc)).advance + PAREN_RIGHT
}

/// How far a reminder's left parenthesis reaches before its sign.
pub(crate) const PAREN_LEFT: f64 = 0.6;
/// … and its right one past the sign's advance.
pub(crate) const PAREN_RIGHT: f64 = 0.5;

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

/// The room a metre takes in an opening, `gap` after it: none when it is
/// not drawn (`\omit Staff.TimeSignature`).
fn time_room(o: &Opening, gap: f64) -> f64 {
    if o.time_parts[3] == crate::look::TIME_OMITTED {
        return 0.0;
    }
    time_width(o.time, o.time_parts) + gap
}

/// An additive metre's own parts, without the style a setting keeps in
/// the last slot (`look::TIME_NUMBERED` and its kin).
fn own_parts(parts: [u8; 4]) -> [u8; 4] {
    let mut out = parts;
    if out[3] > 99 {
        out[3] = 0;
    }
    out
}

pub(crate) fn time_glyph(time: (i64, i64), parts: [u8; 4]) -> Option<&'static str> {
    // `\numericTimeSignature`: the figures, a 4/4 too.
    if parts[3] == crate::look::TIME_NUMBERED || parts[3] == crate::look::TIME_SINGLE {
        return None;
    }
    let parts = own_parts(parts);
    if parts[1] != 0 {
        return None;
    }
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

/// A metre's numerator, as glyphs: its figure, or an additive metre's
/// parts with a + between them.
pub(crate) fn numerator(time: (i64, i64), parts: [u8; 4]) -> Vec<String> {
    let parts = own_parts(parts);
    if parts[1] == 0 {
        return digits(time.0);
    }
    let mut out = Vec::new();
    for (k, p) in parts.iter().filter(|p| **p > 0).enumerate() {
        if k > 0 {
            out.push("timeSigPlus".to_owned());
        }
        out.extend(digits(i64::from(*p)));
    }
    out
}

pub(crate) fn time_width(time: (i64, i64), parts: [u8; 4]) -> f64 {
    if let Some(one) = time_glyph(time, parts) {
        return glyph(one).advance;
    }
    if parts[3] == crate::look::TIME_SINGLE {
        return numerator(time, parts).iter().fold(0.0, |acc, nm| acc + glyph(nm).advance);
    }
    let mut wide: f64 = 0.0;
    for names in [numerator(time, parts), digits(time.1)] {
        let w = names.iter().fold(0.0, |acc, nm| acc + glyph(nm).advance);
        if w > wide {
            wide = w;
        }
    }
    wide
}

/// A clef changed in the middle of a bar: the sign smaller, as LilyPond's
/// `clefs.G_change` and its kin.
pub(crate) fn clef_change_glyph(clef: Clef) -> &'static str {
    match clef.sign {
        Sign::G => "gClefChange",
        Sign::F => "fClefChange",
        Sign::C => "cClefChange",
        _ => clef_glyph(clef),
    }
}

/// The room a clef, a key or a metre changed in the middle of a bar takes
/// before its note: the clef, then the naturals that cancel the old key and
/// the new one, then the metre.
pub(crate) fn mid_change_width(clef: Option<Clef>, key: Option<(i64, i64)>, time: Option<((i64, i64), [u8; 4])>) -> f64 {
    let mut w = 0.0;
    if let Some(c) = clef {
        w += glyph(clef_change_glyph(c)).advance + 0.6;
    }
    if let Some((from, to)) = key {
        let skip = key_cancel(from, to);
        if skip >= 0 {
            w += (from.abs() - skip) as f64 * (glyph("accidentalNatural").advance + 0.1) + 0.5;
        }
        if to != 0 {
            w += key_width(to) + 0.8;
        }
    }
    if let Some((t, parts)) = time {
        w += time_width(t, parts) + 0.6;
    }
    w
}

/// A breath or a caesura after a note: half a space after its ink, and the
/// note's room on the right as far as the sign's.
fn breath_room(ev: &mut Ev) {
    if let Some(name) = ev.breath {
        ev.breath_x = ev.right + 0.5;
        ev.right = ev.breath_x + glyph(name).advance + 0.2;
    }
}

/// A note's room on the left grows by a change drawn before it — half a
/// space clear of what comes before, three tenths of what follows — and the
/// change starts there.
fn change_room(ev: &mut Ev) {
    let w = mid_change_width(ev.clef_change, ev.key_change, ev.time_change);
    if w > 0.0 {
        ev.left += 0.5 + w + 0.3;
        ev.change_x = Some(0.5 - ev.left);
        ev.pre = 0.5 + w + 0.3;
    }
}

pub(crate) fn clef_glyph(clef: Clef) -> &'static str {
    match clef.sign {
        Sign::G => "gClef",
        Sign::F => "fClef",
        Sign::C => "cClef",
        Sign::Percussion => "unpitchedPercussionClef1",
        Sign::Tab => "6stringTabClef",
    }
}

fn prefix_width_of(o: &Opening, first: bool) -> f64 {
    let mut w = 0.8 + glyph(clef_glyph(o.clef)).advance + 1.0;
    if o.fifths != 0 {
        w += key_width(o.fifths) + 0.8;
    }
    if first || o.changes.time {
        w += time_room(o, 1.0);
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
        w += time_room(o, 1.0);
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
    let items = spacers(bar);
    if items.is_empty() {
        return vec![(6.0 * k).max(4.0)];
    }
    let mut out = vec![1.2 + items[0].left];
    for i in 0..items.len() {
        let ev = &items[i];
        if let Some(nxt) = items.get(i + 1) {
            out.push((ev.ideal * k + nxt.pre).max(ev.right + 0.6 + nxt.left).max(dynamics_apart(ev, nxt)));
        } else {
            out.push((ev.ideal * k).max(ev.right + 0.8));
        }
    }
    out
}

/// Two dynamics on notes side by side — `\\fff` then `\\pp` — each
/// centred on its note: the notes stand far enough apart that the letters
/// do not run into one word, as LilyPond's spacing keeps them. Nothing
/// moves where there are not two.
fn dynamics_apart(ev: &Ev, nxt: &Ev) -> f64 {
    let (Some(a), Some(b)) = (&ev.dynamic, &nxt.dynamic) else { return 0.0 };
    let width = |d: &str| marks::dyn_letters(d).iter().fold(0.0, |acc, nm| acc + glyph(nm).advance);
    width(a) / 2.0 + width(b) / 2.0 + 0.6
}

fn bar_springs(bar: &Bar) -> f64 {
    let spacers = spacers(bar);
    if spacers.is_empty() {
        return 6.0;
    }
    spacers.iter().fold(0.0, |acc, ev| acc + ev.ideal)
}

pub(crate) fn courtesy_of(bar: &Bar) -> Courtesy {
    if !bar.staff_openings.is_empty() {
        return Courtesy::Multi(bar.staff_openings.iter().map(courtesy_opening).collect());
    }
    Courtesy::Single(courtesy_opening(&bar.opening()))
}

fn courtesy_opening(o: &Opening) -> Opening {
    Opening { clef: o.clef, fifths: o.fifths, time: o.time, time_parts: o.time_parts, changes: Changes { clef: false, key: o.changes.key, key_from: o.changes.key_from, time: o.changes.time } }
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
        cw += time_room(o, 0.5);
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
        "" => &[],
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
    if parts.is_empty() {
        return 0.0;
    }
    parts.iter().fold(0.0, |acc, p| acc + part_width(p)) + RULE.kern * (parts.len() as f64 - 1.0)
}

fn system_width(bars: &[Bar], k: f64) -> f64 {
    let mut total = prefix_width(&bars[0], bars[0].first);
    for (i, b) in bars.iter().enumerate() {
        total += bar_width(b, k, i == 0);
    }
    total
}

fn break_bars(bars: &mut [Bar], width: f64, indent: f64, rest: f64, opts: &Opts) -> Vec<Line> {
    bars[0].first = true;
    if bars[0].open_bar {
        bars[0].open_repeat = Some(".|:".into());
    }
    let count = bars.len();
    // A movement's first bar opens a line, indented, with its metre, and the
    // line before it announces nothing.
    let widths = Widths {
        opening_w: bars.iter().map(|b| bar_width(b, 1.0, true)).collect(),
        inner_w: bars.iter().map(|b| bar_width(b, 1.0, false)).collect(),
        opening_min: bars.iter().map(|b| bar_width(b, 0.0, true)).collect(),
        inner_min: bars.iter().map(|b| bar_width(b, 0.0, false)).collect(),
        springs: bars.iter().map(bar_springs).collect(),
        courtesy_w: bars.iter().map(|b| if b.fresh { 0.0 } else { courtesy_width(&courtesy_of(b)) }).collect(),
        prefix_w: (0..count).map(|t| prefix_width(&bars[t], t == 0 || bars[t].first)).collect(),
        forced: bars.iter().map(|b| b.brk.is_some()).collect(),
        keep: bars.iter().map(|b| b.no_break).collect(),
        fresh: bars.iter().map(|b| b.fresh).collect(),
    };
    let cuts = line_starts(&widths, |i| if i == 0 || bars[i].fresh { width - indent } else { width - rest }, opts);
    break_lines(bars, width, indent, &cuts, rest, opts)
}

fn break_lines(bars: &mut [Bar], width: f64, indent: f64, starts: &[usize], rest: f64, opts: &Opts) -> Vec<Line> {
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
        let opens = si == 0 || bars[lines[si].bars.start].fresh;
        let mut room = if opens { width - indent } else { width - rest };
        lines[si].courtesy = 0.0;
        if si + 1 < n && !bars[lines[si + 1].bars.start].fresh {
            let next_first = lines[si + 1].bars.start;
            let c = courtesy_of(&bars[next_first]);
            lines[si].courtesy = courtesy_width(&c);
            lines[si].courtesy_bar = Some(c);
            room -= lines[si].courtesy;
        }
        let range = lines[si].bars.clone();
        let natural = system_width(&bars[range.clone()], 1.0);
        // Filled to the margin, unless ragged: the last line of a score too,
        // as LilyPond's `ragged-last` is false; a score of one line is not.
        let last_of_score = si == n - 1 || bars[lines[si + 1].bars.start].fresh;
        let mut k = 1.0;
        if natural > room {
            k = fit(&bars[range], room, 0.0, 1.0);
        } else if !opts.ragged(opens, last_of_score) {
            k = fit(&bars[range], room, 1.0, 100.0);
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
        let items = spacers(bar);
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
        line.left = left + if si == 0 || bars[range.start].fresh { indent } else { rest };
        // A movement's title: the piece at the margin, the opus at the right.
        line.title = bars[range.start].title.clone();
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
            let whole = bar.items.len() == 1 && bar.items[0].k == Kind::Rest && !bar.items[0].spacer && (bar.items[0].full || bar.items[0].ticks == length);
            if whole {
                let span = gs.iter().fold(0.0, |acc, g| acc + g);
                let it = &mut bar.items[0];
                it.x = x + span / 2.0 - glyph("restWhole").advance / 2.0;
                it.whole_bar = true;
                it.span0 = x;
                it.span1 = x + span;
                x += span;
            } else if !bar.cols.is_empty() {
                // Two voices: a column's notes, both voices', at its place.
                x += gs[0];
                for i in 0..bar.cols.len() {
                    bar.cols[i].x = x;
                    for m in 0..bar.cols[i].members.len() {
                        let j = bar.cols[i].members[m].1;
                        let shift = bar.items[j].shift.unwrap_or(0.0);
                        bar.items[j].x = x + shift;
                    }
                    x += gs[i + 1];
                }
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
        if let Some((_, opus)) = &line.title {
            line.title_x = (left, line.right - text_width(opus, Weight::Regular, em(0.0)));
        }
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

/// How large a note is drawn: LilyPond's magnification, 2^(size/6).
pub(crate) fn mag(ev: &Ev) -> f64 {
    2f64.powf(f64::from(ev.size) / 6.0)
}

fn stem(ev: &mut Ev) {
    let hi = ev.heads.last().map_or(0, |h| h.pos);
    let lo = ev.heads.first().map_or(0, |h| h.pos);
    if ev.base == 1 {
        ev.stem_end = y_of(hi);
        return;
    }
    // Each flag past the second needs room on the stem: a 64th's and a
    // 128th's are a space and two spaces taller than a 32nd's.
    let len =
        (3.5 + match ev.base {
            32 => 0.75,
            64 => 1.75,
            128 => 2.75,
            _ => 0.0,
        }) * mag(ev);
    // A small note's stem keeps its length: it is not drawn to the middle line.
    if ev.size < 0 {
        ev.stem_end = if ev.up { y_of(hi) - len } else { y_of(lo) + len };
        return;
    }
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
    let thick = RULE.beam * notes.first().map_or(1.0, |&j| mag(&bar.items[j]));
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
    // `\override Beam.damping`: the slope that much flatter, flat at +inf.0.
    let look = notes.first().map(|&j| bar.items[j].look.clone()).unwrap_or_default();
    if let Some(damping) = look.num("Beam.damping").filter(|d| *d > 0.0) {
        slope = if damping.is_infinite() { 0.0 } else { slope / damping };
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
    // `\override Beam.positions = #'(l . r)`: its ends where it says, in
    // spaces up from the middle line, to the middle of the outer beam.
    if let Some((l, r)) = look.pair("Beam.positions") {
        let half = if up { -thick / 2.0 } else { thick / 2.0 };
        q_left = 2.0 - l + half;
        q_right = 2.0 - r + half;
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

fn stack(systems: &mut [System], staves: &[Vec<Bar>], layout: &Layout, head: &HeadBlock, opts: &Opts) -> Vec<Page> {
    let page_h = layout.page_h;
    let margin = layout.top.unwrap_or(0.0);
    let floor_y = page_h.map_or(0.0, |h| h - layout.bottom.unwrap_or(0.0));
    let titled = !head.lines.is_empty();
    let reaches: Vec<(f64, f64)> = systems.iter().map(|sys| system_reach(sys, staves)).collect();
    let last_bar = |sys: &System| staves[0].get(sys.lines[0].bars.end - 1);
    // A \pageBreak at the end of a system: the next starts a page; a
    // \noPageBreak: it does not.
    let forced: Vec<bool> = systems.iter().map(|sys| last_bar(sys).is_some_and(|b| b.brk.as_deref() == Some("page"))).collect();
    let keep: Vec<bool> = systems.iter().map(|sys| last_bar(sys).is_some_and(|b| b.no_page_break)).collect();
    // A movement starts with it: the room between two scores.
    let fresh: Vec<bool> = systems.iter().map(|sys| staves[0].get(sys.lines[0].bars.start).is_some_and(|b| b.fresh)).collect();
    // Each system's last staff from its first: LilyPond measures the room
    // between two systems from the middle line of the one's last staff to
    // the middle line of the other's first.
    let last_off: Vec<f64> = systems.iter().map(|sys| sys.offsets.last().copied().unwrap_or(0.0)).collect();
    // Where a system goes: the first under the title (`markup-system-spacing`),
    // the first of a later page under the margin, the others under the one
    // before (`system-system-spacing`, `score-system-spacing` where a
    // movement starts): their basic distance apart, their ink their
    // padding apart at least.
    let (m_basic, m_pad) = opts.markup;
    let first_top = |si: usize| {
        let r = reaches[si];
        if si == 0 {
            let mut tp = if titled { (margin + head.bottom + m_basic - 2.0).max(margin + head.bottom + m_pad - r.0) } else { margin + 0.5 - r.0 };
            if titled && page_h.is_none() {
                tp += 1.0;
            }
            tp
        } else {
            margin + 3.5 + 1.0 - r.0
        }
    };
    let next_top = |ptop: f64, pbottom: f64, si: usize| {
        let (basic, padding, _) = if fresh[si] { opts.score } else { opts.system };
        (ptop + last_off[si - 1] + basic).max(pbottom + padding - reaches[si].0)
    };
    // `systems-per-page`, `max-systems-per-page`: a page holds no more, and
    // a system that would not fit still starts the next one.
    let per_page = layout.systems_per_page.into_iter().chain(opts.max_per_page).min().unwrap_or(usize::MAX).max(1);
    let n = systems.len();
    let starts = match page_h {
        Some(_) => {
            let fits = |i: usize, j: usize| -> Option<f64> {
                if j + 1 - i > per_page {
                    return None;
                }
                let top = first_top(i);
                let room = (floor_y - 1.0 - top).max(1.0);
                let (mut ptop, mut pbottom) = (top, top + reaches[i].1);
                for (k, r) in reaches.iter().enumerate().take(j + 1).skip(i + 1) {
                    ptop = next_top(ptop, pbottom, k);
                    pbottom = ptop + r.1;
                    if pbottom + 1.0 > floor_y {
                        return None;
                    }
                }
                Some(((floor_y - 1.0 - pbottom) / room).max(0.0))
            };
            let counted = opts.page_count.and_then(|count| crate::breaking::counted_pages(n, count, fits, |j| !keep[j], |j| forced[j]));
            match counted {
                Some(starts) => starts,
                None if layout.even_pages => page_starts(n, &reaches, &forced, &keep, floor_y, per_page, first_top, next_top),
                None => filled_starts(n, &reaches, &forced, &keep, floor_y, per_page, !opts.minimal, first_top, next_top),
            }
        }
        None => vec![0],
    };
    let mut pages: Vec<Page> = Vec::new();
    let mut prev: Option<(f64, f64)> = None;
    for (si, sys) in systems.iter_mut().enumerate() {
        let r = reaches[si];
        let tp = match prev {
            Some((ptop, pbottom)) if !starts.contains(&si) => next_top(ptop, pbottom, si),
            _ => {
                pages.push(Page { index: pages.len(), systems: Vec::new() });
                first_top(si)
            }
        };
        sys.top = tp;
        sys.bottom = tp + r.1;
        sys.reach_hi = r.0;
        sys.page = pages.len() - 1;
        let pi = pages.len() - 1;
        pages[pi].systems.push(si);
        prev = Some((sys.top, sys.bottom));
    }
    if pages.is_empty() {
        pages.push(Page { index: 0, systems: Vec::new() });
    }
    // A page's systems spread down it (`ragged-bottom` false), the last
    // page's only under `ragged-last-bottom = ##f`.
    if page_h.is_some() && !opts.ragged_bottom {
        let count = pages.len();
        for (pi, pg) in pages.iter().enumerate() {
            if (pi == count - 1 && !opts.fill_last) || pg.systems.len() < 2 {
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

/// The systems that start a page when each is filled in turn: a system goes
/// on the page it fits on, else starts the next — unless a `\noPageBreak`
/// keeps it with the one before, and the page is cut where it may be,
/// earlier. A last page left with one system takes the one before it too,
/// when the page before keeps two or more and the last has the room.
#[allow(clippy::too_many_arguments)]
fn filled_starts(
    n: usize,
    reaches: &[(f64, f64)],
    forced: &[bool],
    keep: &[bool],
    floor_y: f64,
    per_page: usize,
    no_lone: bool,
    first_top: impl Fn(usize) -> f64,
    next_top: impl Fn(f64, f64, usize) -> f64,
) -> Vec<usize> {
    let mut starts = vec![0];
    let mut prev: Option<(f64, f64)> = None;
    let mut si = 0;
    while si < n {
        let top = match prev {
            Some((ptop, pbottom)) => {
                let tp = next_top(ptop, pbottom, si);
                let page = starts.last().copied().unwrap_or(0);
                let full = si - page >= per_page;
                if forced[si - 1] || full || tp + reaches[si].1 + 1.0 > floor_y {
                    // Cut where a page may end: here, or the nearest earlier.
                    let at = (page + 1..=si).rev().find(|&k| forced[k - 1] || !keep[k - 1]).unwrap_or(si);
                    starts.push(at);
                    if at < si {
                        si = at;
                    }
                    first_top(si)
                } else {
                    tp
                }
            }
            None => first_top(si),
        };
        prev = Some((top, top + reaches[si].1));
        si += 1;
    }
    // Not a lone system on the last page, where the one before can join it.
    if let [.., before, last] = starts[..] {
        let fits = |from: usize| {
            let mut pbottom = first_top(from) + reaches[from].1;
            let mut ptop = first_top(from);
            for (k, r) in reaches.iter().enumerate().take(n).skip(from + 1) {
                ptop = next_top(ptop, pbottom, k);
                pbottom = ptop + r.1;
            }
            pbottom + 1.0 <= floor_y
        };
        if no_lone && n - last == 1 && last - before >= 3 && !forced[last - 1] && !keep[last - 2] && per_page >= 2 && fits(last - 1) {
            if let Some(l) = starts.last_mut() {
                *l = last - 1;
            }
        }
    }
    starts
}

/// The systems that start a page, the first included: as few pages as the
/// systems need, and among those ways to cut them, the one whose pages are
/// most evenly full — the last one too, so that it is not left with a line
/// on it. A system starts a page after a \pageBreak, never after a
/// \noPageBreak; one taller than a page has a page to itself.
#[allow(clippy::too_many_arguments)]
fn page_starts(
    n: usize,
    reaches: &[(f64, f64)],
    forced: &[bool],
    keep: &[bool],
    floor_y: f64,
    per_page: usize,
    first_top: impl Fn(usize) -> f64,
    next_top: impl Fn(f64, f64, usize) -> f64,
) -> Vec<usize> {
    if n == 0 {
        return vec![0];
    }
    // best[j]: the least (pages, unevenness) for systems 0..j, and where its last page starts.
    let mut best: Vec<Option<(usize, f64, usize)>> = vec![None; n + 1];
    best[0] = Some((0, 0.0, 0));
    for i in 0..n {
        let Some((pages, cost, _)) = best[i] else { continue };
        let top = first_top(i);
        let room = (floor_y - 1.0 - top).max(1.0);
        let (mut ptop, mut pbottom) = (top, top + reaches[i].1);
        for j in i..n {
            if j - i >= per_page {
                break;
            }
            if j > i {
                ptop = next_top(ptop, pbottom, j);
                pbottom = ptop + reaches[j].1;
                if pbottom + 1.0 > floor_y {
                    break;
                }
            }
            if j + 1 < n && keep[j] && !forced[j] {
                continue;
            }
            let empty = ((floor_y - 1.0 - pbottom) / room).max(0.0);
            let candidate = (pages + 1, cost + empty * empty, i);
            let better = match best[j + 1] {
                None => true,
                Some((p, c, _)) => candidate.0 < p || (candidate.0 == p && candidate.1 < c - 1e-9),
            };
            if better {
                best[j + 1] = Some(candidate);
            }
            if forced[j] {
                break;
            }
        }
    }
    let mut starts = Vec::new();
    let mut j = n;
    while j > 0 {
        let Some((_, _, i)) = best[j] else { break };
        starts.push(i);
        j = i;
    }
    starts.reverse();
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    starts
}

fn system_reach(sys: &System, staves: &[Vec<Bar>]) -> (f64, f64) {
    if let Some(r) = sys.reach {
        return r;
    }
    reach(&staves[0], &sys.lines[0])
}

// ---- The whole --------------------------------------------------------------

pub(crate) fn engrave_parsed(parsed: &Parsed, width: f64, layout: &Layout) -> Result<Engraved, ScoreError> {
    let opts = Opts::of(layout, &parsed.paper);
    crate::breaking::with_durations(&parsed.spacing, || {
        if parsed.staves.len() > 1 {
            return engrave_many(parsed, width, layout, &opts);
        }
        engrave_one(parsed, width, layout, &opts)
    })
}

fn engrave_one(parsed: &Parsed, mut width: f64, layout: &Layout, opts: &Opts) -> Result<Engraved, ScoreError> {
    let kind = parsed.staves[0].kind;
    if let Some(ev) = parsed.staves[0].events.iter().find(|ev| ev.cross.is_some()) {
        return refuse("\\change Staff wants a second staff to go to", ev.at);
    }
    let mut bars = measures(&parsed.staves[0].events)?;
    // `\crossStaff` with no staff to cross to: the note keeps its stem.
    for ev in bars.iter_mut().flat_map(|b| b.items.iter_mut()) {
        ev.span_stem = false;
    }
    crate::look::renumber(std::slice::from_mut(&mut bars));
    first_title(&mut bars, parsed);
    lyrics_assign(&mut bars, parsed.staves[0].lyrics.as_deref(), parsed.staves[0].lyrics_voice);
    for (k, verse) in parsed.staves[0].verses.iter().enumerate() {
        verse_assign(&mut bars, Some(verse), parsed.staves[0].lyrics_voice, k + 1);
    }
    if beamed(kind) {
        beams(&mut bars)?;
    }
    let sh = shortest(bars.iter());
    columns(&mut bars, sh, kind);
    let mut left = layout.left.unwrap_or(0.0);
    let indent = layout.indent.unwrap_or(0.0);
    let mut lines = break_bars(&mut bars, width, indent, opts.short_indent, opts);
    if layout.left.is_none() && lines.len() > 1 {
        let last_number = bars[bars.len() - 1].number;
        left = text_width(&last_number.to_string(), Weight::Regular, em(-2.0)) + 0.2;
        width -= left;
        for bar in &mut bars {
            bar.bar_drawn = None;
            bar.open_repeat = None;
        }
        lines = break_bars(&mut bars, width, indent, opts.short_indent, opts);
    }
    place(&mut bars, &mut lines, left, indent, opts.short_indent);
    for line in &mut lines {
        line.kind = kind;
    }
    curves(&mut bars, &mut lines);
    marks::marks(&mut bars, &mut lines);
    let head = header_lines(&parsed.header, width);
    let mut systems: Vec<System> = lines.into_iter().map(|line| System { left: line.left, right: line.right, lines: vec![line], ..System::default() }).collect();
    let staves = vec![bars];
    let pages = stack(&mut systems, &staves, layout, &head, opts);
    let bottom = systems.iter().fold(0.0, |acc: f64, s| acc.max(s.bottom));
    Ok(Engraved {
        staves,
        systems,
        height: bottom + 1.0,
        width,
        left,
        header: head,
        pages,
        paper: layout.page_h,
        numbered: layout.page_h.is_some() && layout.top.is_some(),
        groups: Vec::new(),
        head_room: layout.head_room,
    })
}

/// The first movement's title, `piece` and `opus` of the header, over the
/// first line (LilyPond's `scoreTitleMarkup` under the book's titles).
fn first_title(bars: &mut [Bar], parsed: &Parsed) {
    let get = |k: &str| parsed.header.get(k).cloned().unwrap_or_default();
    let (piece, opus) = (get("piece"), get("opus"));
    if let Some(b) = bars.first_mut().filter(|_| !(piece.is_empty() && opus.is_empty())) {
        b.title = Some((piece, opus));
    }
}

fn master_bar(all: &[Vec<Bar>], k: usize, shortest: i64) -> Bar {
    let first = &all[0][k];
    let mut mb = first.clone();
    mb.items = Vec::new();
    mb.beams = Vec::new();
    // Spaced by its own columns, every staff's, not the first staff's voices'.
    mb.cols = Vec::new();
    mb.poly = false;
    mb.staff_openings = all.iter().map(|bars| bars[k].opening()).collect();
    // A bar line is the system's, as LilyPond's is: the first staff's that
    // is not a plain one — a chord name's line has none to say.
    if let Some(kind) = all.iter().map(|bars| &bars[k].bar).find(|b| b.as_str() != "|") {
        mb.bar = kind.clone();
    }
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
                col.pre = col.pre.max(ev.pre);
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
            // The piece's first bar shows its metre on every staff, as the
            // room the shared bar keeps for it says.
            sb.first = mb.first;
            sb.bar_drawn = mb.bar_drawn.clone().or_else(|| (sb.bar != mb.bar).then(|| mb.bar.clone()));
            sb.open_repeat = mb.open_repeat.clone();
            sb.open_repeat_x = mb.open_repeat_x;
        }
        for col in &mb.items {
            for &(s, j) in &col.members {
                let shift = staves[s][b].items[j].shift.unwrap_or(0.0);
                staves[s][b].items[j].x = col.x + shift;
            }
        }
        for bars in staves.iter_mut() {
            let sb = &mut bars[b];
            let length = sb.length;
            if sb.items.len() == 1 && sb.items[0].k == Kind::Rest && !sb.items[0].spacer && (sb.items[0].full || sb.items[0].ticks == length) {
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

fn engrave_many(parsed: &Parsed, width: f64, layout: &Layout, opts: &Opts) -> Result<Engraved, ScoreError> {
    let staves_src = &parsed.staves;
    // A line of chord names or figures that says no metre takes the one the
    // music starts in, and its pickup, as LilyPond's \time and \partial,
    // which are the whole score's.
    let opening: Vec<Ev> = staves_src
        .iter()
        .find(|stf| stf.kind == StaffKind::Notes)
        .map(|stf| stf.events.iter().take_while(|ev| !matches!(ev.k, Kind::Note | Kind::Rest)).filter(|ev| matches!(ev.k, Kind::Time | Kind::Partial)).cloned().collect())
        .unwrap_or_default();
    let mut all: Vec<Vec<Bar>> = Vec::new();
    let mut sources: Vec<std::borrow::Cow<'_, [Ev]>> = Vec::new();
    // A `\time` or a `\partial` written in one staff is the score's.
    let timed = crate::measure::score_timing(staves_src, parsed.paper.polymeter);
    for (stf, timed) in staves_src.iter().zip(timed) {
        if let Some(events) = timed {
            all.push(measures(&events)?);
            sources.push(events.into());
            continue;
        }
        let words = matches!(stf.kind, StaffKind::ChordNames | StaffKind::Figures | StaffKind::FretBoards);
        if words && !stf.events.iter().any(|ev| matches!(ev.k, Kind::Time | Kind::Partial)) && !opening.is_empty() {
            let events: Vec<Ev> = opening.iter().cloned().chain(stf.events.iter().cloned()).collect();
            all.push(measures(&events)?);
            sources.push(events.into());
            continue;
        }
        all.push(measures(&stf.events)?);
        sources.push(stf.events.as_slice().into());
    }
    // Bars of rest gathered in one staff and not in another: gathered only
    // where every staff rests the same bars.
    if all.iter().any(|bars| bars.iter().any(|b| b.span.is_some())) {
        let gathered = |bars: &[Bar]| -> Vec<(i64, i64)> { bars.iter().filter_map(|b| b.span.map(|n| (b.tick0, n))).collect() };
        let mut keep = gathered(&all[0]);
        for bars in &all[1..] {
            let theirs = gathered(bars);
            keep.retain(|g| theirs.contains(g));
        }
        for (si, events) in sources.iter().enumerate() {
            if gathered(&all[si]).iter().any(|g| !keep.contains(g)) {
                all[si] = measures_keeping(events, Some(&keep))?;
            }
        }
    }
    first_title(&mut all[0], parsed);
    let count = all[0].len();
    for (si, bars) in all.iter().enumerate() {
        if bars.len() != count {
            if parsed.paper.polymeter {
                return refuse(
                    format!(
                        "staff {} has {} bars and the first has {count}: staves whose bars differ in length are not drawn — \\scaleDurations and \\set Staff.timeSignatureFraction line them up",
                        si + 1,
                        bars.len()
                    ),
                    0,
                );
            }
            return refuse(format!("staff {} has {} bars and the first has {count}: every staff has the same bars", si + 1, bars.len()), 0);
        }
        for (k, bar) in bars.iter().enumerate() {
            if bar.length * bar.span.unwrap_or(1) != all[0][k].length * all[0][k].span.unwrap_or(1) {
                if parsed.paper.polymeter {
                    return refuse(
                        format!(
                            "bar {} is not as long in staff {} as in the first: staves whose bars differ in length are not drawn — \\scaleDurations and \\set Staff.timeSignatureFraction line them up",
                            k + 1,
                            si + 1
                        ),
                        0,
                    );
                }
                return refuse(format!("bar {} is not as long in staff {} as in the first", k + 1, si + 1), 0);
            }
        }
    }
    crate::look::renumber(&mut all);
    let mut plan = crate::cross::plan(&mut all, staves_src)?;
    for (si, bars) in all.iter_mut().enumerate() {
        lyrics_assign(bars, staves_src[si].lyrics.as_deref(), staves_src[si].lyrics_voice);
        for (k, verse) in staves_src[si].verses.iter().enumerate() {
            verse_assign(bars, Some(verse), staves_src[si].lyrics_voice, k + 1);
        }
        if beamed(staves_src[si].kind) {
            beams(bars)?;
        }
    }
    crate::cross::directions(&mut all, &mut plan);
    let sh = shortest(all.iter().flatten());
    for (si, bars) in all.iter_mut().enumerate() {
        columns(bars, sh, staves_src[si].kind);
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
    let rest = deco.max(deco + short_w - left).max(opts.short_indent);
    let mut lines = break_bars(&mut master, width, indent, rest, opts);
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
                kind: staves_src[s].kind,
                bars: range.clone(),
                // Which staff, an ossia not counted: the first staff writes the
                // bar numbers, the endings and the marks; an ossia none of them.
                staff_index: Some(if staves_src[s].kind == StaffKind::Ossia { usize::MAX } else { staves_src[..s].iter().filter(|x| x.kind != StaffKind::Ossia).count() }),
                left: line.left,
                prefix: line.prefix,
                right: line.right,
                bar_end: line.bar_end,
                k: line.k,
                courtesy: line.courtesy,
                ..Line::default()
            };
            if s == 0 {
                (ss.title, ss.title_x) = (line.title.clone(), line.title_x);
            }
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
        for (ss, gone) in staff_lines.iter_mut().zip(removed(staves_src, &all, &range, si == 0)) {
            ss.hidden = gone;
        }
        top_staff_moves(&mut all, &mut staff_lines, &range);
        systems.push(System { lines: staff_lines, multi: true, left: line.left, right: line.right, first_line: si == 0 || master[range.start].fresh, ..System::default() });
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
    crate::cross::ink(&mut systems, &all, &plan);
    let head = header_lines(&parsed.header, width);
    let pages = stack(&mut systems, &all, layout, &head, opts);
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
        head_room: layout.head_room,
    })
}

fn stack_staves(sys: &mut System, all: &[Vec<Bar>], groups: &[Group], staves: &[crate::parse::Staff]) {
    let reaches: Vec<(f64, f64)> = sys.lines.iter().enumerate().map(|(s, l)| reach(&all[s], l)).collect();
    let mut offsets = vec![0.0];
    // An ossia with nothing in this system takes no room.
    let idle = |s: usize| sys.lines[s].kind == StaffKind::Ossia && sys.lines[s].bars.clone().all(|b| all[s][b].items.iter().all(|ev| ev.spacer));
    // Nor does a staff removed from it (`\RemoveEmptyStaves`): the next one
    // goes under the last that is drawn.
    let hidden: Vec<bool> = sys.lines.iter().map(|l| l.hidden).collect();
    let gone = |s: usize| hidden[s];
    for s in 1..reaches.len() {
        if gone(s) {
            offsets.push(offsets[s - 1]);
            continue;
        }
        let Some(p) = (0..s).rev().find(|&k| !gone(k)) else {
            offsets.push(offsets[s - 1]);
            continue;
        };
        if idle(p) {
            offsets.push(offsets[p]);
            continue;
        }
        // Chord names sit as close over their staff as their ink lets them,
        // an ossia closer than another staff.
        let words = |k: StaffKind| matches!(k, StaffKind::ChordNames | StaffKind::Figures | StaffKind::FretBoards);
        let names = words(sys.lines[p].kind) || words(sys.lines[s].kind);
        let least: f64 = if names {
            0.0
        } else if sys.lines[p].kind == StaffKind::Ossia {
            7.0
        } else {
            9.0
        };
        let o = (offsets[p] + least).max(offsets[p] + reaches[p].1 + if names { 0.6 } else { 1.0 } - reaches[s].0);
        offsets.push(o);
    }
    let last = (0..offsets.len()).rev().find(|&s| !gone(s)).unwrap_or(offsets.len() - 1);
    let high = (0..reaches.len()).filter(|&s| !idle(s) && !gone(s)).map(|s| offsets[s] + reaches[s].0).fold(f64::INFINITY, f64::min);
    sys.reach = Some((if high.is_finite() { high } else { reaches[0].0 }, offsets[last] + reaches[last].1));
    sys.names = staves.iter().map(|stf| if sys.first_line { stf.props.instrument_name.clone().unwrap_or_default() } else { stf.props.short_instrument_name.clone().unwrap_or_default() }).collect();
    let n = sys.lines.len();
    for s in 0..n {
        let next = (s + 1..n).find(|&t| !gone(t));
        let ss = &mut sys.lines[s];
        let through = |t: usize| groups.iter().any(|g| (g.kind == GroupKind::Brace || g.kind == GroupKind::Bracket) && g.first <= s && t <= g.last);
        ss.connect_down = match next {
            Some(t) if !ss.hidden && through(t) => offsets[t] - offsets[s] - staff_height(ss.kind),
            _ => 0.0,
        };
    }
    sys.offsets = offsets;
}

/// When the system leaves out the staff that carries what stands over the
/// score — bar numbers, rehearsal marks, jumps, tempo marks, endings — the
/// topmost staff it draws carries them on that system: their bars' marks
/// and endings, and each tempo mark onto its note there, as LilyPond puts
/// them over the top staff drawn.
fn top_staff_moves(all: &mut [Vec<Bar>], lines: &mut [Line], range: &Range<usize>) {
    let Some(z) = lines.iter().position(|l| l.kind != StaffKind::Ossia) else { return };
    if !lines[z].hidden {
        return;
    }
    let Some(top) = (0..lines.len()).find(|&s| !lines[s].hidden && lines[s].kind != StaffKind::Ossia) else { return };
    for b in range.clone() {
        let from = all[z][b].clone();
        let to = &mut all[top][b];
        if to.marks.is_empty() {
            to.marks.clone_from(&from.marks);
        }
        if to.volta_in.is_none() && from.volta_in.is_some() {
            to.volta_in.clone_from(&from.volta_in);
            to.volta_first = from.volta_first;
            to.volta_last = from.volta_last;
            to.volta_closed = from.volta_closed;
        }
        for ev in from.items.iter().filter(|ev| ev.tempo.is_some()) {
            if let Some(there) = to.items.iter_mut().find(|t| t.start == ev.start && t.tempo.is_none()) {
                there.tempo.clone_from(&ev.tempo);
            }
        }
        // The score's bar number settings, where the staff drawn has none.
        if let (Some(look), Some(first)) = (from.items.first().map(|e| e.look.clone()), to.items.first_mut()) {
            if first.look.is_empty() {
                first.look = look;
            }
        }
    }
    let mut k = 0;
    for l in lines.iter_mut() {
        if l.kind == StaffKind::Ossia {
            continue;
        }
        if l.hidden {
            l.staff_index = Some(usize::MAX);
        } else {
            l.staff_index = Some(k);
            k += 1;
        }
    }
}

/// The staves a system leaves out (`\RemoveEmptyStaves`, LilyPond's
/// `VerticalAxisGroup.remove-empty`): a staff of notes whose bars on the
/// system hold rests and spacers only — never on the first system unless
/// `remove-first` (`\RemoveAllEmptyStaves`) says, and never every staff.
fn removed(staves: &[crate::parse::Staff], all: &[Vec<Bar>], range: &Range<usize>, first: bool) -> Vec<bool> {
    let mut out: Vec<bool> = staves
        .iter()
        .enumerate()
        .map(|(s, stf)| {
            if !matches!(stf.kind, StaffKind::Notes | StaffKind::Drums | StaffKind::Tab) {
                return false;
            }
            let look = stf.events.iter().find(|ev| !ev.look.is_empty()).map(|ev| ev.look.clone()).unwrap_or_default();
            if look.flag("VerticalAxisGroup.remove-empty") != Some(true) || (first && look.flag("VerticalAxisGroup.remove-first") != Some(true)) {
                return false;
            }
            range.clone().all(|b| all[s][b].items.iter().all(|ev| ev.k != Kind::Note))
        })
        .collect();
    let kept = staves.iter().enumerate().any(|(s, stf)| !out[s] && matches!(stf.kind, StaffKind::Notes | StaffKind::Drums | StaffKind::Tab));
    if !kept {
        out = vec![false; staves.len()];
    }
    out
}

/// The tempo marks for a test's eyes: unused otherwise.
#[allow(dead_code)]
pub(crate) fn tempo_of(ev: &Ev) -> Option<&Tempo> {
    ev.tempo.as_ref()
}
