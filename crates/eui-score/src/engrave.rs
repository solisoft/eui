//! Laying a score out: beams, spacing, lines, places, stems, staves and
//! pages (`eui_builders_score.sl`, "Stems and beams" to "Placing" and
//! "Several staves"). Function for function the Soli engraver's, under
//! the same names without their `score_` prefix.

use std::ops::Range;

use crate::glyphs::{glyph, Weight};
use crate::marks::{self, curves, reach, DynEl, HairpinPiece, MarkPlace, OttavaPlace, ScriptPlace, SpanPlace, TempoMark, TextPlace, TupletPlace, Volta};
use crate::measure::{clef_mid, measures, measures_keeping, Bar, Beam, Changes, Opening};
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
        let wins = beam_windows(bar.time);
        let mut all: Vec<Vec<usize>> = Vec::new();
        for list in voice_lists(bar) {
            all.extend(beam_voice(bar, &list, wins)?);
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
            if let Some(v) = bar.items[g[0]].voice {
                up = v % 2 == 1;
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
            match ev.voice {
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
fn beam_voice(bar: &Bar, list: &[usize], wins: (i64, i64)) -> Result<Vec<Vec<usize>>, ScoreError> {
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
            // A cadenza has no beats to beam by, and `\autoBeamOff` beams by
            // none: only `[ ]` beams there.
            let fl = if bar.cadenza || ev.no_autobeam { 0 } else { fl };
            let w = (ev.start + bar.offset) / wins.0;
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
                    let w = (ev.start + bar.offset) / wins.1;
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
            let hw = glyph(head_name(ev.base)).advance * k;
            ev.head_w = Some(hw);
            displace(ev);
            let mut right = hw;
            if ev.heads.iter().any(|h| h.side == 1) {
                right += hw - RULE.stem;
            }
            if ev.dots > 0 {
                right += (0.3 + 0.6 * ev.dots as f64) * k;
            }
            if ev.up && ev.beam.is_none() && flags(ev.base) > 0 {
                right += glyph(&flag_name(ev.base, true)).advance * k - RULE.stem;
            }
            ev.right = right;
            if ev.lv {
                ev.right = ev.right.max(hw + 0.15 + marks::SHORT_TIE + 0.2);
            }
            breath_room(ev);
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

pub(crate) fn time_glyph(time: (i64, i64), parts: [u8; 4]) -> Option<&'static str> {
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

/// The room a clef or a key changed in the middle of a bar takes before
/// its note: the clef, then the naturals that cancel the old key and the
/// new one.
pub(crate) fn mid_change_width(clef: Option<Clef>, key: Option<(i64, i64)>) -> f64 {
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
    let w = mid_change_width(ev.clef_change, ev.key_change);
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
        w += time_width(o.time, o.time_parts) + 1.0;
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
        w += time_width(o.time, o.time_parts) + 1.0;
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
        cw += time_width(o.time, o.time_parts) + 0.5;
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
    // A movement's first bar opens a line, indented, with its metre, and the
    // line before it announces nothing.
    let courtesy_w: Vec<f64> = bars.iter().map(|b| if b.fresh { 0.0 } else { courtesy_width(&courtesy_of(b)) }).collect();
    let prefix_w: Vec<f64> = (0..count).map(|t| prefix_width(&bars[t], t == 0 || bars[t].first)).collect();
    let mut best = vec![-1.0; count + 1];
    best[0] = 0.0;
    let mut from_bar = vec![0usize; count + 1];
    for i in 0..count {
        if best[i] < 0.0 {
            continue;
        }
        let room = if i == 0 || bars[i].fresh { width - indent } else { width - rest };
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
    let reaches: Vec<(f64, f64)> = systems.iter().map(|sys| system_reach(sys, staves)).collect();
    // A \pageBreak at the end of a system: the next starts a page.
    let forced: Vec<bool> = systems
        .iter()
        .map(|sys| {
            let last_bar = sys.lines[0].bars.end - 1;
            staves[0].get(last_bar).is_some_and(|b| b.brk.as_deref() == Some("page"))
        })
        .collect();
    // Where a system goes: the first under the title, the first of a later
    // page under the margin, the others under the one before.
    let first_top = |si: usize| {
        let r = reaches[si];
        if si == 0 {
            let mut tp = if titled { (margin + head.bottom + 3.0).max(margin + head.bottom + 0.5 - r.0) } else { margin + 0.5 - r.0 };
            if titled && page_h.is_none() {
                tp += 1.0;
            }
            tp
        } else {
            margin + 3.5 + 1.0 - r.0
        }
    };
    let next_top = |ptop: f64, pbottom: f64, si: usize| (ptop + gap).max(pbottom + 1.0 - reaches[si].0);
    // `systems-per-page`: a page holds no more, and a system that would not
    // fit still starts the next one.
    let per_page = layout.systems_per_page.unwrap_or(usize::MAX).max(1);
    let starts = match page_h {
        Some(_) if layout.even_pages => page_starts(systems.len(), &reaches, &forced, floor_y, per_page, first_top, next_top),
        Some(_) => filled_starts(systems.len(), &reaches, &forced, floor_y, per_page, first_top, next_top),
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

/// The systems that start a page when each is filled in turn: a system goes
/// on the page it fits on, else starts the next.
fn filled_starts(n: usize, reaches: &[(f64, f64)], forced: &[bool], floor_y: f64, per_page: usize, first_top: impl Fn(usize) -> f64, next_top: impl Fn(f64, f64, usize) -> f64) -> Vec<usize> {
    let mut starts = vec![0];
    let mut prev: Option<(f64, f64)> = None;
    for si in 0..n {
        let top = match prev {
            Some((ptop, pbottom)) => {
                let tp = next_top(ptop, pbottom, si);
                let full = starts.last().is_some_and(|&s| si - s >= per_page);
                if forced[si - 1] || full || tp + reaches[si].1 + 1.0 > floor_y {
                    starts.push(si);
                    first_top(si)
                } else {
                    tp
                }
            }
            None => first_top(si),
        };
        prev = Some((top, top + reaches[si].1));
    }
    starts
}

/// The systems that start a page, the first included: as few pages as the
/// systems need, and among those ways to cut them, the one whose pages are
/// most evenly full — the last one too, so that it is not left with a line
/// on it. A system starts a page after a \pageBreak; one taller than a page
/// has a page to itself.
fn page_starts(n: usize, reaches: &[(f64, f64)], forced: &[bool], floor_y: f64, per_page: usize, first_top: impl Fn(usize) -> f64, next_top: impl Fn(f64, f64, usize) -> f64) -> Vec<usize> {
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

pub(crate) fn engrave_parsed(parsed: &Parsed, mut width: f64, layout: &Layout) -> Result<Engraved, ScoreError> {
    if parsed.staves.len() > 1 {
        return engrave_many(parsed, width, layout);
    }
    let kind = parsed.staves[0].kind;
    if let Some(ev) = parsed.staves[0].events.iter().find(|ev| ev.cross.is_some()) {
        return refuse("\\change Staff wants a second staff to go to", ev.at);
    }
    let mut bars = measures(&parsed.staves[0].events)?;
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
    for line in &mut lines {
        line.kind = kind;
    }
    curves(&mut bars, &mut lines);
    marks::marks(&mut bars, &mut lines);
    let head = header_lines(&parsed.header, width);
    let mut systems: Vec<System> = lines.into_iter().map(|line| System { left: line.left, right: line.right, lines: vec![line], ..System::default() }).collect();
    let staves = vec![bars];
    let pages = stack(&mut systems, &staves, layout, &head);
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

/// Notes after `\\change Staff = "…"` drawn on that staff: each moves to the
/// same moment of the same bar there, its heads placed by that staff's clef,
/// its stem towards the staff it came from (a third voice's up, a fourth's
/// down); its own staff keeps a spacer in its place. They sound in their own
/// staff, as they were written. Beams do not cross from one staff to the
/// other: each side beams its own.
fn cross_staff(all: &mut [Vec<Bar>], staves: &[crate::parse::Staff]) -> Result<(), ScoreError> {
    let mut touched: Vec<(usize, usize)> = Vec::new();
    for a in 0..all.len() {
        for b in 0..all[a].len() {
            for j in 0..all[a][b].items.len() {
                let item = &all[a][b].items[j];
                let Some(name) = item.cross.clone().filter(|_| item.k == Kind::Note) else { continue };
                let Some(t) = staves.iter().position(|s| s.name.as_deref() == Some(name.as_str())) else {
                    return refuse(format!("\\change Staff = \"{name}\" names no staff: a \\new Staff = \"{name}\" is what it goes to"), item.at);
                };
                if t == a {
                    continue;
                }
                let mut moved = item.clone();
                let lift = clef_mid(all[a][b].clef_at(item.start).0) - clef_mid(all[t][b].clef_at(item.start).0);
                for h in &mut moved.heads {
                    h.pos += lift;
                }
                moved.voice = Some(if t > a { 3 } else { 4 });
                moved.cross = None;
                (moved.clef_change, moved.key_change) = (None, None);
                let ev = &mut all[a][b].items[j];
                ev.k = Kind::Rest;
                ev.spacer = true;
                ev.heads.clear();
                ev.cross = None;
                all[t][b].items.push(moved);
                all[t][b].poly = true;
                touched.push((t, b));
            }
        }
    }
    for (t, b) in touched {
        all[t][b].items.sort_by_key(|ev| ev.start * 8 + i64::from(ev.voice.unwrap_or(1)));
    }
    Ok(())
}

fn engrave_many(parsed: &Parsed, width: f64, layout: &Layout) -> Result<Engraved, ScoreError> {
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
    for stf in staves_src {
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
            return refuse(format!("staff {} has {} bars and the first has {count}: every staff has the same bars", si + 1, bars.len()), 0);
        }
        for (k, bar) in bars.iter().enumerate() {
            if bar.length * bar.span.unwrap_or(1) != all[0][k].length * all[0][k].span.unwrap_or(1) {
                return refuse(format!("bar {} is not as long in staff {} as in the first", k + 1, si + 1), 0);
            }
        }
    }
    cross_staff(&mut all, staves_src)?;
    for (si, bars) in all.iter_mut().enumerate() {
        lyrics_assign(bars, staves_src[si].lyrics.as_deref(), staves_src[si].lyrics_voice);
        for (k, verse) in staves_src[si].verses.iter().enumerate() {
            verse_assign(bars, Some(verse), staves_src[si].lyrics_voice, k + 1);
        }
        if beamed(staves_src[si].kind) {
            beams(bars)?;
        }
    }
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
        head_room: layout.head_room,
    })
}

fn stack_staves(sys: &mut System, all: &[Vec<Bar>], groups: &[Group], staves: &[crate::parse::Staff]) {
    let reaches: Vec<(f64, f64)> = sys.lines.iter().enumerate().map(|(s, l)| reach(&all[s], l)).collect();
    let mut offsets = vec![0.0];
    // An ossia with nothing in this system takes no room.
    let idle = |s: usize| sys.lines[s].kind == StaffKind::Ossia && sys.lines[s].bars.clone().all(|b| all[s][b].items.iter().all(|ev| ev.spacer));
    for s in 1..reaches.len() {
        if idle(s - 1) {
            offsets.push(offsets[s - 1]);
            continue;
        }
        // Chord names sit as close over their staff as their ink lets them,
        // an ossia closer than another staff.
        let words = |k: StaffKind| matches!(k, StaffKind::ChordNames | StaffKind::Figures | StaffKind::FretBoards);
        let names = words(sys.lines[s - 1].kind) || words(sys.lines[s].kind);
        let least: f64 = if names {
            0.0
        } else if sys.lines[s - 1].kind == StaffKind::Ossia {
            7.0
        } else {
            9.0
        };
        let o = (offsets[s - 1] + least).max(offsets[s - 1] + reaches[s - 1].1 + if names { 0.6 } else { 1.0 } - reaches[s].0);
        offsets.push(o);
    }
    let last = offsets.len() - 1;
    let high = (0..reaches.len()).filter(|&s| !idle(s)).map(|s| offsets[s] + reaches[s].0).fold(f64::INFINITY, f64::min);
    sys.reach = Some((if high.is_finite() { high } else { reaches[0].0 }, offsets[last] + reaches[last].1));
    sys.names = staves.iter().map(|stf| if sys.first_line { stf.props.instrument_name.clone().unwrap_or_default() } else { stf.props.short_instrument_name.clone().unwrap_or_default() }).collect();
    for (s, ss) in sys.lines.iter_mut().enumerate() {
        let through = groups.iter().any(|g| (g.kind == GroupKind::Brace || g.kind == GroupKind::Bracket) && g.first <= s && s < g.last);
        ss.connect_down = if through { offsets[s + 1] - offsets[s] - staff_height(ss.kind) } else { 0.0 };
    }
    sys.offsets = offsets;
}

/// The tempo marks for a test's eyes: unused otherwise.
#[allow(dead_code)]
pub(crate) fn tempo_of(ev: &Ev) -> Option<&Tempo> {
    ev.tempo.as_ref()
}
