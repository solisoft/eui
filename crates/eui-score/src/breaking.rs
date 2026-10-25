//! How the music is laid on lines and pages as LilyPond lays it (score
//! #406): the `\paper` settings that shape it — `line-width`, the indents,
//! the `ragged-*` switches, the vertical spacing alists, the counts of
//! systems and pages — the spacing of durations (`SpacingSpanner`'s
//! `spacing-increment`, `shortest-duration-space`, `base-shortest-duration`)
//! with its optical correction for stems, and the choice of line breaks:
//! the one that, over the whole score, stretches and squeezes the lines
//! least (LilyPond's optimal breaking, after Knuth and Plass).

use std::cell::Cell;

use crate::engrave::y_of;
use crate::measure::Bar;
use crate::parse::{Ev, Kind, PaperSpec, SpacingSpanner};
use crate::vertical::Spec;
use crate::{Layout, WHOLE};

/// The spacing of durations: the room each doubling adds, the room of the
/// shortest common duration in those steps, and the longest the shortest
/// is taken as, in ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Durations {
    /// `spacing-increment`.
    pub incr: f64,
    /// `shortest-duration-space`.
    pub k: f64,
    /// `base-shortest-duration`, in ticks.
    pub base: i64,
}

impl Durations {
    /// LilyPond's: 1.2, 2 and a dotted eighth.
    pub(crate) const DEFAULT: Durations = Durations { incr: 1.2, k: 2.0, base: WHOLE * 3 / 16 };

    fn of(spec: &SpacingSpanner) -> Self {
        let d = Self::DEFAULT;
        Durations { incr: spec.increment.unwrap_or(d.incr), k: spec.shortest_space.unwrap_or(d.k), base: spec.base_shortest.map_or(d.base, |b| ((b * WHOLE as f64).round() as i64).max(1)) }
    }
}

thread_local! {
    static DURATIONS: Cell<Durations> = const { Cell::new(Durations::DEFAULT) };
}

/// The spacing of durations in force while a score is engraved.
pub(crate) fn durations() -> Durations {
    DURATIONS.with(Cell::get)
}

/// Run `f` with the source's spacing of durations in force.
pub(crate) fn with_durations<T>(spec: &SpacingSpanner, f: impl FnOnce() -> T) -> T {
    let before = DURATIONS.with(|d| d.replace(Durations::of(spec)));
    let out = f();
    DURATIONS.with(|d| d.set(before));
    out
}

/// LilyPond's `stem-spacing-correction`: the room, in increments, a stem up
/// followed by a stem down that overlap all their length gains — and one
/// down followed by one up loses.
const STEM_CORRECTION: f64 = 0.5;

/// The stem's reach, top and bottom, a note's heads and the octave of stem
/// it would have: where its stem stands, roughly, before stems are drawn.
fn stem_span(ev: &Ev) -> (f64, f64) {
    let top = y_of(ev.heads.last().map_or(0, |h| h.pos));
    let low = y_of(ev.heads.first().map_or(0, |h| h.pos));
    if ev.up {
        (top - 3.5, low)
    } else {
        (top, low + 3.5)
    }
}

/// Optical spacing for stems (LilyPond's `Note_spacing::stem_dir_correction`):
/// a note whose stem goes up before one whose stem comes down over it looks
/// cramped, the two stems face each other — it gets more room; one down
/// before one up looks loose — it gets less. In proportion to how much of
/// their length the stems share; none for notes under one beam, or stems
/// the same way.
pub(crate) fn optical(bar: &mut Bar, lists: &[Vec<usize>]) {
    let incr = durations().incr;
    for list in lists {
        for w in list.windows(2) {
            let (a, b) = (&bar.items[w[0]], &bar.items[w[1]]);
            let stems = |e: &Ev| e.k == Kind::Note && e.base > 1 && !e.heads.is_empty() && !e.xbeam && !e.span_stem;
            if !stems(a) || !stems(b) || a.up == b.up || (a.beam.is_some() && a.beam == b.beam) || a.start + a.ticks != b.start {
                continue;
            }
            let (sa, sb) = (stem_span(a), stem_span(b));
            let shared = (sa.1.min(sb.1) - sa.0.max(sb.0)).max(0.0);
            let part = (shared / 3.5).min(1.0);
            if part <= 0.0 {
                continue;
            }
            let corr = STEM_CORRECTION * incr * part;
            let ev = &mut bar.items[w[0]];
            ev.ideal = if ev.up { ev.ideal + corr } else { (ev.ideal - corr).max(ev.ideal * 0.75) };
        }
    }
}

/// How the lines and pages of a score are set: its `Layout`, and what its
/// `\paper` and `\layout` say.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Opts {
    /// Every system's indent but the first's, in staff spaces.
    pub short_indent: f64,
    /// `ragged-right`: none says LilyPond's default — ragged for a score of
    /// one system, filled for more.
    pub ragged_right: Option<bool>,
    /// `ragged-last`.
    pub ragged_last: Option<bool>,
    /// `ragged-bottom`: no page's systems spread down it.
    pub ragged_bottom: bool,
    /// `ragged-last-bottom = ##f`: the last page's are.
    pub fill_last: bool,
    /// `system-system-spacing`: from the middle line of a system's last
    /// staff to the next one's first.
    pub system: Spec,
    /// `score-system-spacing`, before a movement's first system.
    pub score: Spec,
    /// `markup-system-spacing`, from the titles to the first system — its
    /// basic distance from the titles' foot to the staff's middle line.
    pub markup: Spec,
    /// `top-system-spacing`, from the top of a page to its first system.
    pub top: Spec,
    /// `last-bottom-spacing`, from the last system to the bottom margin.
    pub bottom: Spec,
    /// `max-systems-per-page`.
    pub max_per_page: Option<usize>,
    /// `page-count`.
    pub page_count: Option<usize>,
    /// `system-count`.
    pub system_count: Option<usize>,
    /// A page breaker other than LilyPond's default asked for
    /// (`#ly:minimal-breaking` and its kin): pages filled in turn, nothing more.
    pub minimal: bool,
}

impl Opts {
    /// Out of a layout and the source's `\paper`: on paper, its lengths in
    /// staff spaces at the staff size; on a screen, what shapes the lines
    /// and the space between them.
    pub(crate) fn of(layout: &Layout, spec: &PaperSpec) -> Self {
        let space_mm = 25.4 * spec.staff_size.unwrap_or(20.0) / 4.0 / 72.0;
        let paper = layout.left.is_some();
        // LilyPond's `\paper` defaults, the basic distance between systems
        // the layout's (12 spaces on paper, as LilyPond's; 10 on a screen).
        let system = Spec::new(layout.gap.unwrap_or(10.0), 8.0, 1.0, Some(60.0)).over(&spec.system_spacing);
        Opts {
            short_indent: if paper { spec.short_indent.map_or(0.0, |mm| mm / space_mm) } else { 0.0 },
            ragged_right: spec.ragged_right,
            ragged_last: spec.ragged_last,
            ragged_bottom: spec.ragged_bottom.unwrap_or(false),
            fill_last: spec.ragged_last_bottom == Some(false),
            system,
            score: Spec::new(system.basic.max(14.0), system.min, system.padding, Some(120.0)).over(&spec.score_spacing),
            markup: Spec::new(5.0, 0.0, 0.5, Some(30.0)).over(&spec.markup_spacing),
            top: Spec::new(1.0, 0.0, 1.0, None).over(&spec.top_spacing),
            bottom: Spec::new(1.0, 0.0, 1.0, Some(30.0)).over(&spec.last_bottom_spacing),
            max_per_page: spec.max_systems_per_page,
            page_count: spec.page_count,
            system_count: spec.system_count,
            minimal: spec.even_pages == Some(false),
        }
    }

    /// Whether a line is set at its natural width: under `ragged-right`,
    /// the last of its score under `ragged-last`, a score's only line
    /// unless `ragged-right = ##f` says otherwise (LilyPond's defaults).
    pub(crate) fn ragged(&self, first_of_score: bool, last_of_score: bool) -> bool {
        self.ragged_right == Some(true) || (first_of_score && last_of_score && self.ragged_right != Some(false)) || (last_of_score && self.ragged_last == Some(true))
    }
}

impl Default for Opts {
    fn default() -> Self {
        Opts::of(&Layout::default(), &PaperSpec::default())
    }
}

/// What the line breaker knows of each bar, at its natural spacing (`w`)
/// and its tightest (`min`).
pub(crate) struct Widths {
    /// A bar opening a line.
    pub opening_w: Vec<f64>,
    /// … and inside one.
    pub inner_w: Vec<f64>,
    /// The tightest a bar opening a line can be.
    pub opening_min: Vec<f64>,
    /// … inside one.
    pub inner_min: Vec<f64>,
    /// Its springs: how much it stretches.
    pub springs: Vec<f64>,
    /// What the line before it announces of it at its end.
    pub courtesy_w: Vec<f64>,
    /// The clef, key and metre a line opening with it starts with.
    pub prefix_w: Vec<f64>,
    /// A line ends after it (`\break`, `\pageBreak`).
    pub forced: Vec<bool>,
    /// No line ends after it (`\noBreak`).
    pub keep: Vec<bool>,
    /// A movement starts with it: a line starts here.
    pub fresh: Vec<bool>,
}

/// One line's badness, `i..jj`: none where it cannot be one; LilyPond's
/// demerits, the square of how far its springs stretch or squeeze — a
/// squeeze four times dearer — and one for the line. A ragged line costs
/// nothing for its room left; a line too full for its tightest spacing, a
/// thousand.
struct Line {
    jj: usize,
    cost: f64,
}

/// Every line that can start at bar `i`, with its badness.
fn lines_from(m: &Widths, i: usize, room: f64, opts: &Opts) -> Vec<Line> {
    let n = m.springs.len();
    let mut out = Vec::new();
    let mut w = m.prefix_w[i] + m.opening_w[i];
    let mut tight = m.prefix_w[i] + m.opening_min[i];
    let mut spring = m.springs[i];
    let first_of_score = i == 0 || m.fresh[i];
    for jj in i + 1..=n {
        if jj > i + 1 {
            if m.forced[jj - 2] || m.fresh[jj - 1] {
                break;
            }
            w += m.inner_w[jj - 1];
            tight += m.inner_min[jj - 1];
            spring += m.springs[jj - 1];
        }
        let courtesy = if jj < n { m.courtesy_w[jj] } else { 0.0 };
        if tight + courtesy > room && jj > i + 1 {
            break;
        }
        let last_of_score = jj == n || m.fresh[jj];
        if !last_of_score && m.keep[jj - 1] && !m.forced[jj - 1] {
            continue;
        }
        let ratio = (room - w - courtesy) / spring.max(1.0);
        let cost = if tight + courtesy > room {
            1000.0
        } else if ratio < 0.0 {
            ratio * ratio * 40.0 + 1.0
        } else if opts.ragged(first_of_score, last_of_score) {
            if last_of_score {
                0.0
            } else {
                1.0 + ratio * ratio * 0.1
            }
        } else {
            ratio * ratio * 10.0 + 1.0
        };
        out.push(Line { jj, cost });
    }
    out
}

/// The bars that start a line: the cuts whose lines' badness adds up
/// least over the whole score, `system-count` lines when the source asks
/// for so many and they can be had. `room(i)` is the room of a line
/// starting at bar `i`.
pub(crate) fn line_starts(m: &Widths, room: impl Fn(usize) -> f64, opts: &Opts) -> Vec<usize> {
    let n = m.springs.len();
    let options: Vec<Vec<Line>> = (0..n).map(|i| lines_from(m, i, room(i), opts)).collect();
    if let Some(count) = opts.system_count.filter(|&c| c >= 1 && c <= n) {
        if let Some(starts) = counted(&options, n, count) {
            return starts;
        }
    }
    let mut best: Vec<Option<(f64, usize)>> = vec![None; n + 1];
    best[0] = Some((0.0, 0));
    for i in 0..n {
        let Some((base, _)) = best[i] else { continue };
        for l in &options[i] {
            let total = base + l.cost;
            if best[l.jj].is_none_or(|(c, _)| total < c) {
                best[l.jj] = Some((total, i));
            }
        }
    }
    let mut starts = Vec::new();
    let mut at = n;
    while at > 0 {
        let Some((_, from)) = best[at] else { break };
        starts.push(from);
        at = from;
    }
    starts.reverse();
    if starts.first() != Some(&0) {
        starts.insert(0, 0);
    }
    starts
}

/// The cheapest cuts into exactly `count` lines, if there are any.
fn counted(options: &[Vec<Line>], n: usize, count: usize) -> Option<Vec<usize>> {
    // best[j][k]: the least badness of k lines ending at bar j, and the start of the last.
    let mut best: Vec<Vec<Option<(f64, usize)>>> = vec![vec![None; count + 1]; n + 1];
    best[0][0] = Some((0.0, 0));
    for i in 0..n {
        for k in 0..count {
            let Some((base, _)) = best[i][k] else { continue };
            for l in &options[i] {
                let total = base + l.cost;
                if best[l.jj][k + 1].is_none_or(|(c, _)| total < c) {
                    best[l.jj][k + 1] = Some((total, i));
                }
            }
        }
    }
    best[n][count]?;
    let mut starts = Vec::new();
    let (mut at, mut k) = (n, count);
    while k > 0 {
        let (_, from) = best[at][k]?;
        starts.push(from);
        at = from;
        k -= 1;
    }
    starts.reverse();
    Some(starts)
}

/// The systems that start a page when the source asks for `count` pages:
/// the cuts into that many whose pages are most evenly full, or none when
/// the systems do not go on so many (too few systems, or too many to fit).
/// `fits(i, j)`: systems `i..=j` fit on one page, and how empty it is left
/// (0 full, 1 empty); `cut(j)`: a page may end after system `j`; `forced(j)`:
/// it must.
pub(crate) fn counted_pages(n: usize, count: usize, fits: impl Fn(usize, usize) -> Option<f64>, cut: impl Fn(usize) -> bool, forced: impl Fn(usize) -> bool) -> Option<Vec<usize>> {
    if count == 0 || count > n {
        return None;
    }
    let mut best: Vec<Vec<Option<(f64, usize)>>> = vec![vec![None; count + 1]; n + 1];
    best[0][0] = Some((0.0, 0));
    for i in 0..n {
        for k in 0..count {
            let Some((base, _)) = best[i][k] else { continue };
            for j in i..n {
                let Some(empty) = fits(i, j) else { break };
                if j + 1 == n || cut(j) {
                    let total = base + empty * empty;
                    if best[j + 1][k + 1].is_none_or(|(c, _)| total < c) {
                        best[j + 1][k + 1] = Some((total, i));
                    }
                }
                if forced(j) {
                    break;
                }
            }
        }
    }
    best[n][count]?;
    let mut starts = Vec::new();
    let (mut at, mut k) = (n, count);
    while k > 0 {
        let (_, from) = best[at][k]?;
        starts.push(from);
        at = from;
        k -= 1;
    }
    starts.reverse();
    Some(starts)
}
