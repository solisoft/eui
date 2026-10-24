//! An engraving drawn as canvas paths (03 §1.1): staff lines, stems and bar
//! lines as rectangles (kind 1), everything curved as fills (kind 5), in
//! pixels at `sp` a staff space, rounded to the hundredth
//! (`eui_builders_score.sl`, "Drawing").

use std::cell::RefCell;
use std::ops::Range;

use crate::engrave::{
    acc_name, barline_parts, barline_width, beam_y, church_width, clef_glyph, digits, flag_name, flags, head_name, key_cancel, part_width, rest_name, stem_x, time_glyph, time_width, y_of, Courtesy,
    Engraved, Line, System, GRACE_K,
};
use crate::glyphs::{glyph, letters, Weight};
use crate::marks::{dyn_letters, figure_width, glyph_run_w, mark_em, ottava_em, reach, rest_lift, run_em, verse_gap, voice_of, DynEl, MarkPlace, OttavaPlace, TempoMark, TupletPlace, Volta};
use crate::measure::{clef_mid, Bar, Beam, Opening};
use crate::parse::{modulo, Clef, Ev, GraceKind, GroupKind, Kind, Run, Sign, StaffKind, Syllable};
use crate::text::{args_of, em, text_box, text_width};
use crate::RULE;

/// One canvas path, in pixels, without its colour.
#[derive(Debug, Clone, PartialEq)]
pub enum Path {
    /// Kind 1: a filled rectangle, `x y w h`.
    Rect([f64; 4]),
    /// Kind 5: a filled outline, its commands and their coordinates.
    Fill(Vec<f64>),
}

/// Where an item's paths are among a page's: its number among the notes
/// and rests of its staff's piece, and the range.
pub type Ranges = Vec<(usize, Range<usize>)>;

/// Each note head's paths: the item, the head's diatonic number, and the
/// range its glyph, accidental and dots take — what an editor colours to
/// show one note of a chord (03 §1.4).
pub type HeadRanges = Vec<(usize, i64, Range<usize>)>;

/// To the hundredth as Soli's `Float#round(2)` does it: on the shortest
/// decimal that reads back as `v`, halves away from zero — so 22.884999…
/// (written "22.884999999999998") is 22.88, where `(v * 100).round()`
/// would see 2288.5 and say 22.89.
pub(crate) fn r(v: f64) -> f64 {
    let x = v * 100.0;
    if ((x - x.trunc()).abs() - 0.5).abs() > 1e-6 || !v.is_finite() {
        return x.round() / 100.0;
    }
    let text = format!("{v}");
    let (whole, frac) = text.split_once('.').unwrap_or((&text, ""));
    if frac.len() <= 2 {
        return v;
    }
    let kept: String = format!("{whole}.{}", &frac[..2]);
    let base: f64 = kept.parse().unwrap_or(v);
    if frac.as_bytes()[2] < b'5' {
        return base;
    }
    let step = if v < 0.0 { -0.01 } else { 0.01 };
    format!("{:.2}", base + step).parse().unwrap_or(v)
}

/// A rectangle at least a px each way: a staff line or a stem drawn small
/// must not round away to nothing.
fn rect(x: f64, y: f64, w: f64, h: f64, sp: f64) -> Path {
    Path::Rect([r(x * sp), r(y * sp), r((w * sp).max(1.0)), r((h * sp).max(1.0))])
}

fn fill_k(cmds: &[f64], ox: f64, oy: f64, k: f64, sp: f64) -> Path {
    let mut out = Vec::with_capacity(cmds.len());
    let mut i = 0;
    while i < cmds.len() {
        let c = cmds[i];
        out.push(c);
        let args = args_of(c);
        for jj in 0..args {
            let v = cmds.get(i + 1 + jj).copied().unwrap_or(0.0);
            out.push(r(if jj % 2 == 0 { ox + v * k } else { oy + v * k } * sp));
        }
        i += 1 + args;
    }
    Path::Fill(out)
}

fn fill(cmds: &[f64], ox: f64, oy: f64, sp: f64) -> Path {
    fill_k(cmds, ox, oy, 1.0, sp)
}

/// Words as they were drawn, in pixels: what a PDF writes under their
/// outlines as text a reader selects, copies and searches.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Said {
    /// The words.
    pub text: String,
    /// Their face.
    pub weight: Weight,
    /// Where they start…
    pub x: f64,
    /// …on this baseline, y down.
    pub y: f64,
    /// Their em.
    pub size: f64,
}

thread_local! {
    /// The words drawn while [`said`] listens; none otherwise.
    static SAID: RefCell<Option<Vec<Said>>> = const { RefCell::new(None) };
}

/// `f`'s answer, and every word drawn while it ran.
pub(crate) fn said<T>(f: impl FnOnce() -> T) -> (T, Vec<Said>) {
    SAID.with(|s| *s.borrow_mut() = Some(Vec::new()));
    let out = f();
    let words = SAID.with(|s| s.borrow_mut().take()).unwrap_or_default();
    (out, words)
}

struct Pen<'a> {
    paths: &'a mut Vec<Path>,
    sp: f64,
    /// Where each head was drawn, when somebody asked.
    heads: Option<&'a mut HeadRanges>,
}

impl Pen<'_> {
    fn push(&mut self, p: Path) {
        self.paths.push(p);
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64) {
        let sp = self.sp;
        self.push(rect(x, y, w, h, sp));
    }

    fn fill(&mut self, cmds: &[f64], ox: f64, oy: f64) {
        let sp = self.sp;
        self.push(fill(cmds, ox, oy, sp));
    }

    fn glyph(&mut self, name: &str, x: f64, y: f64) {
        let sp = self.sp;
        self.push(fill(&glyph(name).fill, x, y, sp));
    }

    /// A glyph at `k` times its size, its origin where the full-size one's is.
    fn glyph_k(&mut self, name: &str, x: f64, y: f64, k: f64) {
        let sp = self.sp;
        self.push(fill_k(&glyph(name).fill, x, y, k, sp));
    }

    /// `words` on a baseline at `y` from `x`; answers where they end.
    fn text(&mut self, words: &str, weight: Weight, x: f64, y: f64, em: f64) -> f64 {
        if !words.trim().is_empty() {
            let sp = self.sp;
            SAID.with(|s| {
                if let Some(list) = s.borrow_mut().as_mut() {
                    list.push(Said { text: words.to_string(), weight, x: x * sp, y: y * sp, size: em * sp });
                }
            });
        }
        let face = letters().face(weight);
        let k = em / 1000.0;
        let mut at = x;
        for ch in words.chars() {
            match face.get(&ch) {
                Some(g) => {
                    if !g.f.is_empty() {
                        let sp = self.sp;
                        self.push(fill_k(&g.f, at, y, k, sp));
                    }
                    at += g.a * k;
                }
                None => at += 0.25 * em,
            }
        }
        at
    }
}

/// The paths of page `page`: its titles if it is the first, its number if
/// it is not, and its systems. `ranges` gets where each item's paths are.
pub fn page_paths(eng: &Engraved, sp: f64, page: usize, ranges: &mut Ranges) -> Vec<Path> {
    page_paths_heads(eng, sp, page, ranges, None)
}

/// [`page_paths`], and where each head was drawn.
pub fn page_paths_heads(eng: &Engraved, sp: f64, page: usize, ranges: &mut Ranges, mut heads: Option<&mut HeadRanges>) -> Vec<Path> {
    let mut paths = head_paths(eng, sp, page);
    for sys in &eng.systems {
        if sys.page == page {
            draw_system_heads(&mut paths, eng, sys, sys.top, sp, ranges, heads.as_deref_mut());
        }
    }
    paths
}

/// One canvas of a page: the titles (or the page number), or a system.
#[derive(Debug, Clone, PartialEq)]
pub struct Band {
    /// Its top, px from the page's.
    pub y: i64,
    /// Its height, px.
    pub h: i64,
    /// Its paths, in its own frame.
    pub paths: Vec<Path>,
    /// Where each item's paths are among them.
    pub ranges: Ranges,
}

/// Page `page` cut into bands, one canvas each — the titles, then each
/// system drawn with its staff's top line a whole number of pixels down —
/// their heights adding up to `total` px (`score_bands`). A band's paths
/// depend on nothing but its own system, so an edit changes only the band
/// it is in.
pub fn bands(eng: &Engraved, sp: f64, page: usize, total: i64) -> Vec<Band> {
    let systems: Vec<&System> = eng.systems.iter().filter(|s| s.page == page).collect();
    let starts: Vec<(&System, i64, i64)> = systems
        .iter()
        .map(|sys| {
            let pad = (-sys.reach_hi * sp).ceil() as i64 + 2;
            (*sys, pad, (sys.top * sp + 0.5).floor() as i64 - pad)
        })
        .collect();
    let first_y = starts.first().map_or(total, |s| s.2.max(0));
    let mut out = vec![Band { y: 0, h: first_y, paths: head_paths(eng, sp, page), ranges: Vec::new() }];
    for (k, (sys, pad, start)) in starts.into_iter().enumerate() {
        let prev_y = out.last().map_or(0, |b| b.y);
        let y = if k == 0 { first_y } else { start.max(prev_y) };
        if let Some(last) = out.last_mut() {
            last.h = y - last.y;
        }
        if sys.multi {
            for piece in staff_bands(eng, sys, pad, sp) {
                let at = y + piece.cut;
                if piece.cut > 0 {
                    if let Some(last) = out.last_mut() {
                        last.h = at - last.y;
                    }
                }
                out.push(Band { y: at, h: 0, paths: piece.paths, ranges: piece.ranges });
            }
            continue;
        }
        let mut paths = Vec::new();
        let mut ranges = Vec::new();
        draw_system(&mut paths, eng, sys, pad as f64 / sp, sp, &mut ranges);
        out.push(Band { y, h: 0, paths, ranges });
    }
    if let Some(last) = out.last_mut() {
        last.h = (total - last.y).max(0);
    }
    out
}

/// One staff's band of a system of several: its top, px below the
/// system's, its paths and where each item's are among them.
struct Piece {
    cut: i64,
    paths: Vec<Path>,
    ranges: Ranges,
}

/// A system of several staves as a band a staff (`score_staff_bands`), so
/// that an edit on one staff resends that staff's band and no other. The
/// system is drawn once, as one band would draw it; each staff's band is
/// cut half way between its staff's lowest ink and the next one's highest,
/// and takes its staff's paths and whatever else reaches into it — the bar
/// lines a brace or a bracket draws down from the staff above, the brace,
/// the bracket, the names — moved to its own top: the canvas clips each to
/// its box, and the shares meet where they were.
fn staff_bands(eng: &Engraved, sys: &System, pad: i64, sp: f64) -> Vec<Piece> {
    let n = sys.lines.len();
    let top = pad as f64 / sp;
    let mut paths = Vec::new();
    let mut ranges: Ranges = Vec::new();
    let mut starts = Vec::with_capacity(n + 1);
    for (s, line) in sys.lines.iter().enumerate() {
        starts.push(paths.len());
        let offset = sys.offsets.get(s).copied().unwrap_or(0.0);
        if let Some(bars) = eng.staves.get(s) {
            draw_line(&mut paths, bars, line, top + offset, sp, &mut ranges, None);
        }
    }
    starts.push(paths.len());
    draw_groups(&mut Pen { paths: &mut paths, sp, heads: None }, eng, sys, top);
    let tops: Vec<f64> = (0..n).map(|s| pad as f64 + sys.offsets.get(s).copied().unwrap_or(0.0) * sp).collect();
    let reaches: Vec<(f64, f64)> = sys.lines.iter().enumerate().map(|(s, line)| eng.staves.get(s).map_or((0.0, 4.0), |bars| reach(bars, line))).collect();
    let mut cuts: Vec<i64> = vec![0];
    for s in 1..n {
        let a = tops[s - 1] + reaches[s - 1].1 * sp;
        let b = tops[s] + reaches[s].0 * sp;
        let prev = cuts.last().copied().unwrap_or(0);
        cuts.push((prev + 1).max(((a + b) / 2.0).floor() as i64));
    }
    let mut out = Vec::with_capacity(n);
    for s in 0..n {
        let first = starts[s];
        let upto = starts[s + 1];
        let cut = cuts[s];
        let bottom = if s + 1 < n { cuts[s + 1] - cut } else { -1 };
        let mut mine: Vec<Path> = paths.get(first..upto).unwrap_or(&[]).iter().map(|pa| moved(pa, cut)).collect();
        for (j, pa) in paths.iter().enumerate() {
            if j >= first && j < upto {
                continue;
            }
            let pa = moved(pa, cut);
            if meets(&pa, bottom) {
                mine.push(pa);
            }
        }
        let own: Ranges = ranges.iter().filter(|(_, r)| r.start >= first && r.end <= upto).map(|(item, r)| (*item, (r.start - first)..(r.end - first))).collect();
        out.push(Piece { cut, paths: mine, ranges: own });
    }
    out
}

/// A path moved `dy` px up, to the hundredth as every path is.
fn moved(pa: &Path, dy: i64) -> Path {
    let dy = dy as f64;
    match pa {
        Path::Rect([x, y, w, h]) => Path::Rect([*x, r(y - dy), *w, *h]),
        Path::Fill(cmds) => {
            let mut out = Vec::with_capacity(cmds.len());
            let mut i = 0;
            while i < cmds.len() {
                let c = cmds[i];
                out.push(c);
                let args = args_of(c);
                for jj in 0..args {
                    let v = cmds.get(i + 1 + jj).copied().unwrap_or(0.0);
                    out.push(if jj % 2 == 1 { r(v - dy) } else { v });
                }
                i += 1 + args;
            }
            Path::Fill(out)
        }
    }
}

/// Whether a path reaches into a band `bottom` px tall (-1: no bottom).
fn meets(pa: &Path, bottom: i64) -> bool {
    let (lo, hi) = match pa {
        Path::Rect([_, y, _, h]) => (*y, y + h),
        Path::Fill(cmds) => {
            let mut lo = 0.0;
            let mut hi = 0.0;
            let mut first = true;
            let mut i = 0;
            while i < cmds.len() {
                let args = args_of(cmds[i]);
                for jj in 0..args {
                    if jj % 2 == 1 {
                        let v = cmds.get(i + 1 + jj).copied().unwrap_or(0.0);
                        lo = if first { v } else { f64::min(lo, v) };
                        hi = if first { v } else { f64::max(hi, v) };
                        first = false;
                    }
                }
                i += 1 + args;
            }
            (lo, hi)
        }
    };
    hi >= 0.0 && (bottom < 0 || lo <= bottom as f64)
}

/// A page's titles if it is the first, its number if it is numbered.
pub fn head_paths(eng: &Engraved, sp: f64, page: usize) -> Vec<Path> {
    let mut paths = Vec::new();
    let mut pen = Pen { paths: &mut paths, sp, heads: None };
    let left = eng.left;
    draw_titles(&mut pen, eng, page);
    if page > 0 && eng.numbered {
        let number = (page + 1).to_string();
        let e = em(0.0);
        let nx = if (page + 1) % 2 == 1 { left + eng.width - text_width(&number, Weight::Regular, e) } else { left };
        pen.text(&number, Weight::Regular, nx, 2.83 + 0.72 * e, e);
    }
    paths
}

fn draw_titles(pen: &mut Pen, eng: &Engraved, page: usize) {
    if page != 0 {
        return;
    }
    let margin = if !eng.pages.is_empty() && eng.paper.is_some() { 2.83 + eng.head_room } else { 1.0 };
    for line in &eng.header.lines {
        for cell in &line.cells {
            pen.text(&cell.text, cell.weight, eng.left + cell.x, margin + line.y, cell.em);
        }
    }
}

/// Page `page` as the PDF prints it: its titles if it is the first and its
/// systems, with no number at the top — the footer carries it.
pub fn print_paths(eng: &Engraved, sp: f64, page: usize) -> Vec<Path> {
    let mut paths = Vec::new();
    draw_titles(&mut Pen { paths: &mut paths, sp, heads: None }, eng, page);
    for sys in &eng.systems {
        if sys.page == page {
            draw_system(&mut paths, eng, sys, sys.top, sp, &mut Vec::new());
        }
    }
    paths
}

/// The footer of page `page` of `count`, its baseline at `y`: `title` at the
/// left, cut short with an ellipsis where it would reach the number, and
/// `n / count` at the right, two steps smaller than the music's words.
pub fn foot_paths(eng: &Engraved, sp: f64, page: usize, count: usize, title: &str, y: f64) -> Vec<Path> {
    let mut paths = Vec::new();
    let mut pen = Pen { paths: &mut paths, sp, heads: None };
    let e = em(-2.0);
    let number = format!("{} / {count}", page + 1);
    let number_w = text_width(&number, Weight::Regular, e);
    pen.text(&number, Weight::Regular, eng.left + eng.width - number_w, y, e);
    let room = eng.width - number_w - e;
    let mut words = title.to_string();
    if text_width(&words, Weight::Regular, e) > room {
        let mut kept: Vec<char> = title.chars().collect();
        loop {
            kept.pop();
            words = kept.iter().collect::<String>() + "\u{2026}";
            if kept.is_empty() || text_width(&words, Weight::Regular, e) <= room {
                break;
            }
        }
    }
    pen.text(&words, Weight::Regular, eng.left, y, e);
    paths
}

/// `words` from `x` on the baseline `y`, an em of `e`: a header's, a
/// footer's, the copyright's.
pub(crate) fn words_paths(sp: f64, words: &str, x: f64, y: f64, e: f64) -> Vec<Path> {
    let mut paths = Vec::new();
    Pen { paths: &mut paths, sp, heads: None }.text(words, Weight::Regular, x, y, e);
    paths
}

/// `words` cut short with an ellipsis where they would be wider than
/// `room`, at an em of `e`.
pub(crate) fn cut_to(words: &str, room: f64, e: f64) -> String {
    if text_width(words, Weight::Regular, e) <= room {
        return words.to_string();
    }
    let mut kept: Vec<char> = words.chars().collect();
    loop {
        kept.pop();
        let cut = kept.iter().collect::<String>() + "\u{2026}";
        if kept.is_empty() || text_width(&cut, Weight::Regular, e) <= room {
            return cut;
        }
    }
}

/// One system with its first staff's top line at `top`.
pub fn draw_system(paths: &mut Vec<Path>, eng: &Engraved, sys: &System, top: f64, sp: f64, ranges: &mut Ranges) {
    draw_system_heads(paths, eng, sys, top, sp, ranges, None);
}

fn draw_system_heads(paths: &mut Vec<Path>, eng: &Engraved, sys: &System, top: f64, sp: f64, ranges: &mut Ranges, mut heads: Option<&mut HeadRanges>) {
    if sys.multi {
        for (s, line) in sys.lines.iter().enumerate() {
            draw_line(paths, &eng.staves[s], line, top + sys.offsets[s], sp, ranges, heads.as_deref_mut());
        }
        draw_groups(&mut Pen { paths, sp, heads: None }, eng, sys, top);
        return;
    }
    draw_line(paths, &eng.staves[0], &sys.lines[0], top, sp, ranges, heads);
}

/// A clef's origin: the line it marks — a tablature's in the middle of its
/// six strings.
fn clef_y(clef: Clef) -> f64 {
    if clef.sign == Sign::Tab {
        return crate::engrave::staff_height(StaffKind::Tab) / 2.0;
    }
    y_of(clef.line)
}

/// A head's glyph: a drum's cross or circled cross, or the value's.
fn head_glyph(base: i64, style: &str) -> &'static str {
    match (style, base) {
        ("X" | "CircleX", 1) => "noteheadXWhole",
        ("X" | "CircleX", 2) => "noteheadXHalf",
        ("X", _) => "noteheadXBlack",
        ("CircleX", _) => "noteheadCircleX",
        _ => head_name(base),
    }
}

/// A clef at `x`, and the 8 or 15 of one that moves the music an octave or
/// two: in italic, small, centred under its foot or over its head, as
/// LilyPond's `ClefModifier` sets it.
fn draw_clef(pen: &mut Pen, clef: Clef, x: f64, top: f64) {
    draw_clef_as(pen, clef, clef_glyph(clef), x, top);
}

/// A clef in the glyph given: its own, or the smaller one of a change in
/// the middle of a bar.
fn draw_clef_as(pen: &mut Pen, clef: Clef, name: &str, x: f64, top: f64) {
    let g = glyph(name);
    let y = top + clef_y(clef);
    pen.glyph(name, x, y);
    if clef.octave == 0 {
        return;
    }
    let e = em(-4.0);
    let figure = if clef.octave.abs() == 1 { "8" } else { "15" };
    let w = text_width(figure, Weight::Italic, e);
    let fx = x + (g.bbox[0] + g.bbox[2]) / 2.0 - w / 2.0;
    let base = if clef.octave < 0 { y + g.bbox[3] + 0.1 + digit_height(e) } else { y + g.bbox[1] - 0.1 };
    pen.text(figure, Weight::Italic, fx, base, e);
}

fn draw_line(paths: &mut Vec<Path>, bars: &[Bar], line: &Line, top: f64, sp: f64, ranges: &mut Ranges, heads: Option<&mut HeadRanges>) {
    let mut pen = Pen { paths, sp, heads };
    let right = line.right;
    let left = line.left;
    if line.kind == StaffKind::Figures {
        // The figures, each line under the one above, centred on its note.
        let e = crate::engrave::figure_em();
        for b in line.bars.clone() {
            for ev in &bars[b].items {
                let from_k = pen.paths.len();
                let mid = ev.x + ev.head_w.unwrap_or(0.0) / 2.0;
                for (k, runs) in ev.figures.iter().enumerate() {
                    let w = crate::marks::runs_width(runs, e);
                    draw_runs(&mut pen, runs, mid - w / 2.0, top + crate::engrave::figure_line(k), e);
                }
                ranges.push((ev.i, from_k..pen.paths.len()));
            }
        }
        return;
    }
    if matches!(line.kind, StaffKind::ChordNames | StaffKind::FretBoards) {
        // The chords' names, and nothing else: no lines, no bar lines.
        for b in line.bars.clone() {
            for ev in &bars[b].items {
                let from_k = pen.paths.len();
                draw_runs(&mut pen, &ev.chord_name, ev.x, top + crate::engrave::CHORD_BASE, crate::engrave::chord_em());
                ranges.push((ev.i, from_k..pen.paths.len()));
            }
        }
        return;
    }
    if line.kind == StaffKind::Ossia {
        draw_ossia(&mut pen, bars, line, top, ranges);
        return;
    }
    let tab = line.kind == StaffKind::Tab;
    if tab {
        draw_strings(&mut pen, bars, line, top);
    } else {
        for i in 0..5 {
            pen.rect(left, top + i as f64 - RULE.staff / 2.0, right - left, RULE.staff);
        }
    }
    let first = &bars[line.bars.start];
    if first.number > 1 && line.staff_index.unwrap_or(0) == 0 {
        let e = em(-2.0);
        let number = first.number.to_string();
        let number_x = if line.staff_index.is_some() { left } else { left - text_width(&number, Weight::Regular, e) - 0.2 };
        pen.text(&number, Weight::Regular, number_x, top - 1.6, e);
    }
    let mut x = left + 0.8;
    let clef = clef_glyph(first.clef);
    draw_clef(&mut pen, first.clef, x, top);
    x += line.clef_w.unwrap_or(glyph(clef).advance) + 1.0;
    if tab {
        // A tablature's frets say what a key and a metre would.
    } else if let Some(key_w) = line.key_w {
        if first.fifths != 0 {
            draw_key(&mut pen, first.fifths, first.clef, x, top, false, 0);
        }
        if key_w > 0.0 {
            x += key_w + 0.8;
        }
    } else if first.fifths != 0 {
        x = draw_key(&mut pen, first.fifths, first.clef, x, top, false, 0) + 0.8;
    }
    if (first.first || first.changes.time) && !tab {
        draw_time(&mut pen, first.time, first.time_parts, x, top);
    }
    let down = line.connect_down + crate::engrave::staff_height(line.kind) - 4.0;
    for (bi, b) in line.bars.clone().enumerate() {
        let bar = &bars[b];
        if bi > 0 && !tab {
            draw_changes(&mut pen, &bar.opening(), bar.x0, top);
        }
        if tab {
            for ev in &bar.items {
                let from_k = pen.paths.len();
                draw_frets(&mut pen, ev, top);
                ranges.push((ev.i, from_k..pen.paths.len()));
            }
            let kind = bar.drawn();
            draw_bar(&mut pen, kind, bar.x1 + barline_width(kind), top, down);
            continue;
        }
        for ev in &bar.items {
            if let Some(cx) = ev.change_x {
                draw_mid_change(&mut pen, ev, bar.clef_at(ev.start).0, ev.x + cx, top);
            }
            let from_k = pen.paths.len();
            if ev.percent.is_none() {
                draw_graces(&mut pen, ev, top);
            }
            if ev.percent.is_some() || ev.spacer {
                // Played, and drawn as nothing: a spacer, or a copy the
                // percent sign stands for.
            } else if ev.k == Kind::Rest {
                draw_rest(&mut pen, ev, top);
            } else {
                draw_note(&mut pen, ev, top);
            }
            if let Some(name) = ev.breath.filter(|_| !ev.spacer) {
                // A comma just over the top line; a caesura across it.
                let g = glyph(name);
                let y = if name == "caesura" { -(g.bbox[1] + g.bbox[3]) / 2.0 } else { -0.1 - g.bbox[3] };
                pen.glyph(name, ev.x + ev.breath_x, top + y);
            }
            ranges.push((ev.i, from_k..pen.paths.len()));
        }
        if bar.percent {
            draw_percent(&mut pen, bar, top);
        } else {
            for g in &bar.beams {
                draw_beam(&mut pen, bar, g, top);
            }
        }
        let kind = bar.drawn();
        draw_bar(&mut pen, kind, bar.x1 + barline_width(kind), top, down);
        if bi == 0 {
            if let Some(ox) = bar.open_repeat_x {
                draw_bar(&mut pen, ".|:", ox, top, 0.0);
            }
        }
    }
    if tab {
        return;
    }
    for c in &line.curves {
        pen.fill(c, 0.0, top);
    }
    for mk in &line.marks {
        draw_tempo(&mut pen, mk, top);
    }
    for vb in &line.voltas {
        draw_volta(&mut pen, vb, top);
    }
    for tp in &line.tuplets {
        draw_tuplet(&mut pen, tp, top);
    }
    for sc in &line.scripts {
        match &sc.figure {
            Some(figure) => {
                if sc.circled {
                    draw_ring(&mut pen, sc.x, top + sc.y, crate::marks::STRING_R, 0.1);
                }
                let w: f64 = figure.chars().map(|ch| glyph(&format!("timeSig{ch}")).advance * sc.k).sum();
                let mut x = sc.x - if sc.circled { w } else { figure_width(figure) } / 2.0;
                let sp = pen.sp;
                for ch in figure.chars() {
                    let g = glyph(&format!("timeSig{ch}"));
                    pen.push(fill_k(&g.fill, x, top + sc.y, sc.k, sp));
                    x += g.advance * sc.k;
                }
            }
            None => pen.glyph(sc.glyph, sc.x, top + sc.y),
        }
    }
    for dy in &line.dynamics {
        draw_dynamic(&mut pen, dy, top);
    }
    for tx in &line.texts {
        draw_runs(&mut pen, &tx.runs, tx.x0, top + tx.y, em(0.0));
    }
    for g in &line.glissandi {
        draw_line_seg(&mut pen, g[0], top + g[1], g[2], top + g[3], 0.1);
    }
    for ot in &line.ottavas {
        draw_ottava(&mut pen, ot, top);
    }
    for sp in &line.spans {
        draw_span(&mut pen, sp, top);
    }
    if let (Some((piece, opus)), Some(y)) = (&line.title, line.title_y) {
        let e = em(0.0);
        pen.text(piece, Weight::Regular, line.title_x.0, top + y, e);
        pen.text(opus, Weight::Regular, line.title_x.1, top + y, e);
    }
    for mk in &line.rehearsal {
        draw_mark(&mut pen, mk, top);
    }
    if let Some(ly) = line.lyric_y {
        for verse in 0..line.lyric_lines.max(1) {
            draw_lyrics(&mut pen, bars, line, top + ly + verse_gap() * verse as f64, verse);
        }
    }
    if line.courtesy > 0.0 {
        if let Some(Courtesy::Single(o)) = &line.courtesy_bar {
            draw_courtesy(&mut pen, o, line.bar_end, top);
        }
    }
}

/// An ossia: its staff and its notes over the bars it plays in, and only
/// there; its ties and slurs.
fn draw_ossia(pen: &mut Pen, bars: &[Bar], line: &Line, top: f64, ranges: &mut Ranges) {
    for b in line.bars.clone() {
        let bar = &bars[b];
        if bar.items.iter().all(|ev| ev.spacer) {
            continue;
        }
        let kind = bar.drawn();
        let x1 = bar.x1 + barline_width(kind);
        for i in 0..5 {
            pen.rect(bar.x0, top + i as f64 - RULE.staff / 2.0, x1 - bar.x0, RULE.staff);
        }
        draw_bar(pen, "|", bar.x0, top, 0.0);
        draw_bar(pen, kind, x1, top, 0.0);
        for ev in &bar.items {
            let from_k = pen.paths.len();
            if !ev.spacer {
                draw_graces(pen, ev, top);
                if ev.k == Kind::Rest {
                    draw_rest(pen, ev, top);
                } else {
                    draw_note(pen, ev, top);
                }
            }
            ranges.push((ev.i, from_k..pen.paths.len()));
        }
        for g in &bar.beams {
            draw_beam(pen, bar, g, top);
        }
    }
    for c in &line.curves {
        pen.fill(c, 0.0, top);
    }
}

/// The syllables on the baseline `y`, each centred on its note's head; a
/// hyphen centred in the gap to the next syllable, 0.66 long, shorter where
/// they stand close and none where they touch, after the last syllable of a
/// line whose word goes on; an extender at the foot of the letters from a
/// held syllable's end to the last head it is held over.
fn draw_lyrics(pen: &mut Pen, bars: &[Bar], line: &Line, y: f64, verse: usize) {
    let e = em(0.0);
    // The voice they are sung to: the one whose notes hold them.
    let sung = line.bars.clone().flat_map(|b| bars[b].items.iter()).find(|ev| verse_of(ev, verse).is_some() || ev.lyric_held).map_or(1, voice_of);
    let notes: Vec<&Ev> = line.bars.clone().flat_map(|b| bars[b].items.iter()).filter(|ev| ev.k == Kind::Note && voice_of(ev) == sung).collect();
    for (k, ev) in notes.iter().enumerate() {
        let Some(lyr) = verse_of(ev, verse) else { continue };
        let w = if lyr.text.is_empty() { 0.0 } else { text_width(&lyr.text, Weight::Regular, e) };
        let x0 = ev.x + ev.head_w.unwrap_or(0.0) / 2.0 - w / 2.0;
        let x1 = x0 + w;
        if w > 0.0 {
            pen.text(&lyr.text, Weight::Regular, x0, y, e);
        }
        let mut nxt: Option<&Ev> = None;
        let mut held: Option<&Ev> = None;
        let mut j = k + 1;
        while j < notes.len() && nxt.is_none() {
            let other = notes[j];
            if verse_of(other, verse).is_none() {
                if other.lyric_held {
                    held = Some(other);
                }
            } else {
                nxt = Some(other);
            }
            j += 1;
        }
        if lyr.hyphen && w > 0.0 {
            let n0 = match nxt {
                Some(nx) => nx.x + nx.head_w.unwrap_or(0.0) / 2.0 - verse_of(nx, verse).map_or(0.0, |l| text_width(&l.text, Weight::Regular, e)) / 2.0,
                None => line.right,
            };
            let len = 0.66_f64.min(n0 - x1 - 0.3);
            if len > 0.15 {
                let mid = if nxt.is_none() { x1 + 0.3 + len / 2.0 } else { (x1 + n0) / 2.0 };
                pen.rect(mid - len / 2.0, y - 0.51, len, 0.12);
            }
        }
        if let Some(h) = held.filter(|_| lyr.extend && w > 0.0) {
            let end_x = h.x + h.head_w.unwrap_or(0.0);
            if end_x > x1 + 0.6 {
                pen.rect(x1 + 0.2, y - 0.05, end_x - x1 - 0.2, 0.1);
            }
        }
    }
}

/// Verse `verse`'s syllable on a note: the first verse's own, or one after it.
fn verse_of(ev: &Ev, verse: usize) -> Option<&Syllable> {
    if verse == 0 {
        ev.lyric.as_ref()
    } else {
        ev.verses.get(verse - 1).and_then(Option::as_ref)
    }
}

fn draw_tempo(pen: &mut Pen, mk: &TempoMark, top: f64) {
    let mark = &mk.mark;
    let e = em(0.0);
    let y = top + mk.y;
    let mut x = mk.x;
    if !mark.text.is_empty() {
        x = pen.text(&mark.text, Weight::Bold, x, y, e);
        if mark.bpm > 0 {
            x += 0.6;
        }
    }
    if mark.bpm == 0 {
        return;
    }
    x = pen.text("(", Weight::Regular, x, y, e) + 0.25;
    let k = 0.72;
    let head = glyph(head_name(mark.base));
    let hy = y - 0.35;
    let sp = pen.sp;
    pen.push(fill_k(&head.fill, x, hy, k, sp));
    if mark.base > 1 {
        let sx = x + (head.advance - RULE.stem) * k;
        let tip = hy - 3.0 * k;
        pen.rect(sx, tip, RULE.stem * k, hy - 0.12 - tip);
        if mark.base >= 8 {
            let flag = glyph(&flag_name(mark.base, true));
            let ax = flag.anchors.get("stemUpNW").map_or(0.0, |a| a[0]);
            pen.push(fill_k(&flag.fill, sx - ax * k, tip, k, sp));
        }
    }
    x += head.advance * k + 0.7;
    if mark.base >= 8 {
        x += glyph(&flag_name(mark.base, true)).advance * k * 0.8;
    }
    pen.text(&format!("= {})", mark.bpm), Weight::Regular, x, y, e);
}

pub(crate) fn digit_height(e: f64) -> f64 {
    let face = &letters().italic;
    let mut rise: f64 = 0.0;
    for ch in "0123456789".chars() {
        let Some(l) = face.get(&ch) else { continue };
        let cmds = &l.f;
        let mut ci = 0;
        while ci < cmds.len() {
            let args = args_of(cmds[ci]);
            let mut jj = 1;
            while jj < args {
                if let Some(v) = cmds.get(ci + 1 + jj) {
                    rise = rise.min(*v);
                }
                jj += 2;
            }
            ci += 1 + args;
        }
    }
    -rise * e / 1000.0
}

fn draw_tuplet(pen: &mut Pen, tp: &TupletPlace, top: f64) {
    let e = em(-2.0);
    let w = text_width(&tp.label, Weight::Italic, e);
    let h = digit_height(e);
    let xm = (tp.x0 + tp.x1) / 2.0;
    let span = (tp.x1 - tp.x0).max(0.01);
    let slope = (tp.yb - tp.ya) / span;
    let ym = tp.ya + slope * (xm - tp.x0);
    pen.text(&tp.label, Weight::Italic, xm - w / 2.0, top + ym + h / 2.0, e);
    if !tp.bracket {
        return;
    }
    let half = 0.08;
    let gap = w / 2.0 + 0.35;
    for seg in [(tp.x0, xm - gap), (xm + gap, tp.x1)] {
        let ya = tp.ya + slope * (seg.0 - tp.x0);
        let yb = tp.ya + slope * (seg.1 - tp.x0);
        pen.fill(&[0.0, seg.0, ya - half, 1.0, seg.1, yb - half, 1.0, seg.1, yb + half, 1.0, seg.0, ya + half, 4.0], 0.0, top);
    }
    let hook = 0.7;
    for end_at in [(tp.x0, tp.ya), (tp.x1 - 2.0 * half, tp.yb)] {
        let y0 = if tp.up { end_at.1 - half } else { end_at.1 - hook };
        pen.rect(end_at.0, top + y0, 2.0 * half, hook + half);
    }
}

fn draw_dynamic(pen: &mut Pen, dy: &DynEl, top: f64) {
    if let Some(text) = &dy.text {
        let mut x = dy.x0;
        for name in dyn_letters(text) {
            pen.glyph(name, x, top + dy.y + 0.6);
            x += glyph(name).advance;
        }
        return;
    }
    if let Some(words) = &dy.words {
        // `\cresc`: "cresc." in italics, then dashes to where it ends.
        let e = em(0.0);
        let mut x = dy.x0;
        if !words.is_empty() {
            x = pen.text(words, Weight::Italic, dy.x0, top + dy.y + 0.45, e) + 0.4;
        }
        dashes(pen, x, dy.x1, top + dy.y);
        return;
    }
    let h = 0.6666;
    let grows = dy.grow == '<';
    let open_a = if grows { dy.from } else { 1.0 - dy.from };
    let open_b = if grows { dy.to } else { 1.0 - dy.to };
    let half = 0.05;
    for side in [-1.0, 1.0] {
        let ya = dy.y + side * h * open_a;
        let yb = dy.y + side * h * open_b;
        pen.fill(&[0.0, dy.x0, ya - half, 1.0, dy.x1, yb - half, 1.0, dy.x1, yb + half, 1.0, dy.x0, ya + half, 4.0], 0.0, top);
    }
}

fn draw_groups(pen: &mut Pen, eng: &Engraved, sys: &System, top: f64) {
    let offsets = &sys.offsets;
    let left = sys.left;
    // The system's line joins its staves — not the chord names over them.
    let staves: Vec<usize> = (0..sys.lines.len()).filter(|&s| !matches!(sys.lines[s].kind, StaffKind::ChordNames | StaffKind::Figures | StaffKind::FretBoards | StaffKind::Ossia)).collect();
    if let (Some(&a), Some(&b)) = (staves.first(), staves.last()) {
        if staves.len() > 1 {
            let bottom = offsets[b] + crate::engrave::staff_height(sys.lines[b].kind);
            pen.rect(left - 0.08, top + offsets[a] - 0.05, 0.16, bottom - offsets[a] + 0.1);
        }
    }
    let mut edge = left - 0.3;
    let deepest = eng.groups.iter().map(|g| g.depth).max().unwrap_or(0);
    for g in &eng.groups {
        if g.kind == GroupKind::None {
            continue;
        }
        // An ossia over a group's first staff stands outside its bracket.
        let first = (g.first..=g.last).find(|&s| sys.lines[s].kind != StaffKind::Ossia).unwrap_or(g.first);
        let y0 = top + offsets[first];
        let y1 = top + offsets[g.last] + crate::engrave::staff_height(sys.lines[g.last].kind);
        let x = left - 0.3 - (deepest - g.depth) as f64 * 1.6;
        if g.kind == GroupKind::Brace {
            draw_brace(pen, x, y0, y1);
            edge = edge.min(x - 1.3);
        } else {
            let bx = x - 0.5;
            pen.rect(bx, y0, 0.45, y1 - y0);
            pen.glyph("bracketTop", bx, y0);
            pen.glyph("bracketBottom", bx, y1);
            edge = edge.min(bx - 0.3);
        }
    }
    let e = em(0.0);
    let label = |g: &crate::parse::Group| if sys.first_line { g.name.clone() } else { g.short.clone() };
    for (s, name) in sys.names.iter().enumerate() {
        if name.is_empty() {
            continue;
        }
        let in_group = eng.groups.iter().any(|g| g.first <= s && s <= g.last && g.kind != GroupKind::None && g.first < g.last && !label(g).is_empty());
        if in_group {
            continue;
        }
        draw_name(pen, name, edge, top + offsets[s] + 2.0, e);
    }
    for g in &eng.groups {
        let l = label(g);
        if l.is_empty() {
            continue;
        }
        draw_name(pen, &l, edge, top + (offsets[g.first] + offsets[g.last] + 4.0) / 2.0, e);
    }
}

fn draw_name(pen: &mut Pen, name: &str, right: f64, middle: f64, e: f64) {
    let bx = text_box(name, Weight::Regular, e);
    pen.text(name, Weight::Regular, right - 0.6 - text_width(name, Weight::Regular, e), middle - (bx.0 + bx.1) / 2.0, e);
}

fn draw_brace(pen: &mut Pen, x: f64, y0: f64, y1: f64) {
    let want = y1 - y0;
    let mut best: Option<&crate::glyphs::Glyph> = None;
    for n in 10..46 {
        let Some(g) = crate::glyphs::glyphs().get(&format!("brace{}", n * 10)) else { continue };
        let h = g.bbox[3] - g.bbox[1];
        if best.is_none_or(|b| (h - want).abs() < (b.bbox[3] - b.bbox[1] - want).abs()) {
            best = Some(g);
        }
    }
    let Some(best) = best else { return };
    let scale = want / (best.bbox[3] - best.bbox[1]);
    let mid = (y0 + y1) / 2.0;
    let cmds = &best.fill;
    let mut out = Vec::with_capacity(cmds.len());
    let mut ci = 0;
    while ci < cmds.len() {
        let c = cmds[ci];
        out.push(c);
        let args = args_of(c);
        for jj in 0..args {
            let v = cmds.get(ci + 1 + jj).copied().unwrap_or(0.0);
            out.push(if jj % 2 == 0 { v - best.bbox[2] } else { v * scale });
        }
        ci += 1 + args;
    }
    pen.fill(&out, x, mid);
}

fn draw_volta(pen: &mut Pen, vb: &Volta, top: f64) {
    let line = 0.16;
    let y = top + vb.y;
    let x0 = vb.x0 + 0.3;
    let x1 = vb.x1 - 0.3;
    pen.rect(x0, y - line / 2.0, x1 - x0, line);
    if vb.first {
        pen.rect(x0, y - line / 2.0, line, 1.8);
        pen.text(&vb.label, Weight::Bold, x0 + 0.5, y + 1.5, em(-2.0));
    }
    if vb.closed {
        pen.rect(x1 - line, y - line / 2.0, line, 1.8);
    }
}

fn draw_courtesy(pen: &mut Pen, o: &Opening, bar_end: f64, top: f64) {
    let ch = o.changes;
    let mut x = bar_end + 0.6;
    if ch.key {
        let skip = key_cancel(ch.key_from, o.fifths);
        if skip >= 0 {
            x = draw_key(pen, ch.key_from, o.clef, x, top, true, skip) + 0.5;
        }
        if o.fifths != 0 {
            x = draw_key(pen, o.fifths, o.clef, x, top, false, 0) + 0.5;
        }
    }
    if ch.time {
        draw_time(pen, o.time, o.time_parts, x, top);
    }
}

fn draw_key(pen: &mut Pen, fifths: i64, clef: Clef, mut x: f64, top: f64, cancel: bool, skip: i64) -> f64 {
    // LilyPond's places (`key-signature-interface::alteration-positions`):
    // each the highest of its step's not above a ceiling that is the
    // clef's — where its middle C falls — and the accidental's.
    let c0 = modulo(7 - clef_mid(clef), 7);
    let place = |k: usize| -> i64 {
        let (step, hi) = if fifths > 0 { ([3, 0, 4, 1, 5, 2, 6][k], [4, 5, 4, 2, 3, 2, 3][c0 as usize]) } else { ([6, 2, 5, 1, 4, 0, 3][k], [2, 3, 4, 2, 1, 2, 1][c0 as usize]) };
        hi - modulo(hi - step - c0, 7)
    };
    let name = if cancel {
        "accidentalNatural"
    } else if fifths > 0 {
        "accidentalSharp"
    } else {
        "accidentalFlat"
    };
    let g = glyph(name);
    let n = fifths.abs().min(7);
    for i in skip.max(0)..n {
        let pos = place(i as usize);
        pen.glyph(name, x, top + y_of(pos));
        x += g.advance + 0.1;
    }
    x
}

fn draw_time(pen: &mut Pen, time: (i64, i64), parts: [u8; 4], x: f64, top: f64) {
    if let Some(one) = time_glyph(time, parts) {
        pen.glyph(one, x, top + 2.0);
        return;
    }
    let wide = time_width(time, parts);
    for (pi, names) in [crate::engrave::numerator(time, parts), digits(time.1)].into_iter().enumerate() {
        let w = names.iter().fold(0.0, |acc, nm| acc + glyph(nm).advance);
        let mut cx = x + (wide - w) / 2.0;
        for nm in &names {
            pen.glyph(nm, cx, top + if pi == 0 { 1.0 } else { 3.0 });
            cx += glyph(nm).advance;
        }
    }
}

/// A clef or a key changed in the middle of a bar, from `x` before its
/// note: the clef smaller, the old key's naturals, the new key.
fn draw_mid_change(pen: &mut Pen, ev: &Ev, clef: Clef, mut x: f64, top: f64) {
    if let Some(c) = ev.clef_change {
        let name = crate::engrave::clef_change_glyph(c);
        draw_clef_as(pen, c, name, x, top);
        x += glyph(name).advance + 0.6;
    }
    if let Some((from, to)) = ev.key_change {
        let skip = key_cancel(from, to);
        if skip >= 0 {
            x = draw_key(pen, from, clef, x, top, true, skip) + 0.5;
        }
        if to != 0 {
            draw_key(pen, to, clef, x, top, false, 0);
        }
    }
}

fn draw_changes(pen: &mut Pen, o: &Opening, x0: f64, top: f64) {
    let ch = o.changes;
    let mut x = x0 + 0.6;
    if ch.clef {
        let clef = clef_glyph(o.clef);
        draw_clef(pen, o.clef, x, top);
        x += glyph(clef).advance + 0.8;
    }
    if ch.key {
        let skip = key_cancel(ch.key_from, o.fifths);
        if skip >= 0 {
            x = draw_key(pen, ch.key_from, o.clef, x, top, true, skip) + 0.5;
        }
        if o.fifths != 0 {
            x = draw_key(pen, o.fifths, o.clef, x, top, false, 0) + 0.8;
        }
    }
    if ch.time {
        draw_time(pen, o.time, o.time_parts, x, top);
    }
}

/// A bar line whose right edge is at `x`; `down` on to the next staff.
fn draw_bar(pen: &mut Pen, kind: &str, x: f64, top: f64, down: f64) {
    let bar_top = top - RULE.staff / 2.0;
    let bar_h = 4.0 + RULE.staff + down;
    let mut at = x - barline_width(kind);
    for part in barline_parts(kind) {
        if *part == "dots" {
            pen.glyph("augmentationDot", at, top + 1.5);
            pen.glyph("augmentationDot", at, top + 2.5);
            at += glyph("augmentationDot").advance;
        } else {
            let w = part_width(part);
            pen.rect(at, bar_top, w, bar_h);
            at += w;
        }
        at += RULE.kern;
    }
}

fn draw_rest(pen: &mut Pen, ev: &Ev, top: f64) {
    if let Some(mm) = ev.mm {
        let x0 = ev.span0 + 1.0;
        let x1 = ev.span1 - 1.0;
        if mm <= 10 {
            let mut cx = (x0 + x1) / 2.0 - church_width(mm) / 2.0;
            let mut left_bars = mm;
            while left_bars > 0 {
                if left_bars >= 4 {
                    pen.rect(cx, top + 1.0, 0.5, 2.0);
                    left_bars -= 4;
                    cx += 0.5 + 0.6;
                } else if left_bars >= 2 {
                    pen.rect(cx, top + 1.0, 0.5, 1.0);
                    left_bars -= 2;
                    cx += 0.5 + 0.6;
                } else {
                    pen.glyph("restWhole", cx, top + 1.0);
                    left_bars -= 1;
                    cx += glyph("restWhole").advance + 0.6;
                }
            }
        } else {
            pen.rect(x0, top + 1.75, x1 - x0, 0.5);
            pen.rect(x0, top + 1.0, 0.18, 2.0);
            pen.rect(x1 - 0.18, top + 1.0, 0.18, 2.0);
        }
        let names = digits(mm);
        let wide = names.iter().fold(0.0, |acc, nm| acc + glyph(nm).advance);
        let mut nx = (x0 + x1) / 2.0 - wide / 2.0;
        for nm in &names {
            pen.glyph(nm, nx, top - 1.3);
            nx += glyph(nm).advance;
        }
        return;
    }
    if ev.whole_bar {
        pen.glyph("restWhole", ev.x, top + 1.0 + rest_lift(ev));
        return;
    }
    let name = rest_name(ev.base);
    let k = crate::engrave::mag(ev);
    let y = if ev.base == 1 { 1.0 } else { 2.0 } + rest_lift(ev);
    pen.glyph_k(name, ev.x, top + y, k);
    if ev.dots > 0 {
        let mut dx = ev.x + (glyph(name).advance + 0.3) * k;
        for _ in 0..ev.dots {
            pen.glyph_k("augmentationDot", dx, top + 1.5 + rest_lift(ev), k);
            dx += 0.6 * k;
        }
    }
}

fn draw_note(pen: &mut Pen, ev: &Ev, top: f64) {
    let hname = head_name(ev.base);
    let k = crate::engrave::mag(ev);
    let hw = ev.head_w.unwrap_or(0.0);
    let hi = ev.heads.last().map_or(0, |h| h.pos);
    let lo = ev.heads.first().map_or(0, |h| h.pos);
    let wide = ev.heads.iter().any(|h| h.side != 0);
    let reach_out = hw * RULE.ledger_out;
    let mut lx = ev.x - reach_out;
    let mut lw = hw + 2.0 * reach_out;
    if wide && ev.up {
        lw += hw - RULE.stem;
    }
    if wide && !ev.up {
        lx -= hw - RULE.stem;
        lw += hw - RULE.stem;
    }
    let lt = RULE.ledger;
    let mut pos = 6;
    while pos <= hi {
        pen.rect(lx, top + y_of(pos) - lt / 2.0, lw, lt);
        pos += 2;
    }
    pos = -6;
    while pos >= lo {
        pen.rect(lx, top + y_of(pos) - lt / 2.0, lw, lt);
        pos -= 2;
    }
    for hd in &ev.heads {
        let from = pen.paths.len();
        let hx = ev.x + hd.side as f64 * (hw - RULE.stem);
        let y = top + y_of(hd.pos);
        pen.glyph_k(head_glyph(ev.base, hd.style), hx, y, k);
        if let Some(acc) = hd.acc {
            let ax = ev.x + hd.acc_x.unwrap_or(0.0);
            if hd.paren {
                // A reminder, `cis?`: the sign between parentheses.
                let at = ax + crate::engrave::PAREN_LEFT;
                pen.glyph_k("accidentalParensLeft", at, y, k);
                pen.glyph_k(acc_name(acc), at, y, k);
                pen.glyph_k("accidentalParensRight", at + glyph(acc_name(acc)).advance + crate::engrave::PAREN_RIGHT - glyph("accidentalParensRight").bbox[2], y, k);
            } else {
                pen.glyph_k(acc_name(acc), ax, y, k);
            }
        }
        if ev.dots > 0 {
            let dot_pos = if hd.pos % 2 == 0 { hd.pos + 1 } else { hd.pos };
            let mut dx = ev.x + dot_start(ev, dot_pos);
            for _ in 0..ev.dots {
                pen.glyph_k("augmentationDot", dx, top + y_of(dot_pos), k);
                dx += 0.6 * k;
            }
        }
        let to = pen.paths.len();
        if let Some(h) = pen.heads.as_deref_mut() {
            h.push((ev.i, hd.d, from..to));
        }
    }
    if let Some(ax) = ev.arp_x {
        draw_arpeggio(pen, ev.x + ax, top + y_of(hi) - 0.6, top + y_of(lo) + 0.6);
    }
    if ev.base == 1 {
        if ev.tremolo > 0 {
            draw_tremolo(pen, ev, top, ev.x + hw / 2.0, top + y_of(if ev.up { hi } else { lo }), ev.up);
        }
        return;
    }
    if ev.tremolo > 0 {
        let sx = stem_x(ev) - if ev.up { RULE.stem / 2.0 } else { -RULE.stem / 2.0 };
        draw_tremolo(pen, ev, top, sx, top + (ev.stem_end + y_of(if ev.up { lo } else { hi })) / 2.0, ev.up);
    }
    let g = glyph(hname);
    let sx;
    let tip = top + ev.stem_end;
    if ev.up {
        let root = top + y_of(lo) + g.anchors.get("stemUpSE").map_or(0.0, |a| a[1]) * k;
        sx = stem_x(ev) - RULE.stem;
        pen.rect(sx, tip, RULE.stem, root - tip);
    } else {
        let root = top + y_of(hi) + g.anchors.get("stemDownNW").map_or(0.0, |a| a[1]) * k;
        sx = stem_x(ev);
        pen.rect(sx, root, RULE.stem, tip - root);
    }
    if ev.beam.is_none() && flags(ev.base) > 0 {
        let flag = flag_name(ev.base, ev.up);
        let anchor = glyph(&flag).anchors.get(if ev.up { "stemUpNW" } else { "stemDownSW" }).copied().unwrap_or([0.0, 0.0]);
        pen.glyph_k(&flag, sx - anchor[0] * k, tip - anchor[1] * k, k);
    }
}

/// Where a note's dots at `dot_pos` start, from its x: right of its head
/// (of a head moved right of the stem, in a chord stem up), past a flag
/// hanging there.
pub(crate) fn dot_start(ev: &Ev, dot_pos: i64) -> f64 {
    let hw = ev.head_w.unwrap_or(0.0);
    let mut x = hw + 0.3;
    if ev.up && ev.heads.iter().any(|h| h.side != 0) {
        x += hw - RULE.stem;
    }
    x.max(past_flag(ev, dot_pos))
}

/// Where a dot at `dot_pos` must start, from the note's x, to stand clear
/// of its stem-up flag: past the flag when the flag hangs down to the dot's
/// height (a dotted eighth on a line, its dot in the space above), else 0.
pub(crate) fn past_flag(ev: &Ev, dot_pos: i64) -> f64 {
    if !ev.up || ev.beam.is_some() || ev.base <= 1 || flags(ev.base) == 0 {
        return 0.0;
    }
    let flag = glyph(&flag_name(ev.base, true));
    let anchor = flag.anchors.get("stemUpNW").copied().unwrap_or([0.0, 0.0]);
    let (top, bottom) = (ev.stem_end - anchor[1] + flag.bbox[1], ev.stem_end - anchor[1] + flag.bbox[3]);
    let dot = glyph("augmentationDot");
    let y = y_of(dot_pos);
    if y + dot.bbox[3] + 0.1 <= top || y + dot.bbox[1] - 0.1 >= bottom {
        return 0.0;
    }
    stem_x(ev) - ev.x - RULE.stem - anchor[0] + flag.bbox[2] + 0.2
}

fn draw_beam(pen: &mut Pen, bar: &Bar, g: &Beam, top: f64) {
    let notes: Vec<&Ev> = g.notes.iter().map(|&j| &bar.items[j]).collect();
    let dir = if g.up { 1.0 } else { -1.0 };
    let k = notes.first().map_or(1.0, |ev| crate::engrave::mag(ev));
    for level in 0..g.n {
        let off = level as f64 * RULE.beam_step * k * dir;
        let mut i = 0;
        while i < notes.len() {
            if flags(notes[i].base) <= level {
                i += 1;
                continue;
            }
            let mut jj = i;
            while jj + 1 < notes.len() && flags(notes[jj + 1].base) > level {
                jj += 1;
            }
            let mut xa = stem_x(notes[i]) - if g.up { RULE.stem } else { 0.0 };
            let mut xb = stem_x(notes[jj]) + if g.up { 0.0 } else { RULE.stem };
            if i == jj {
                let stub = 1.1;
                if i == 0 {
                    xb = xa + stub;
                } else {
                    xb = xa + RULE.stem;
                    xa = xb - stub;
                }
            }
            let ya = beam_y(g, xa) + off;
            let yb = beam_y(g, xb) + off;
            let bt = RULE.beam * k;
            let cmds = if g.up { [0.0, xa, ya, 1.0, xb, yb, 1.0, xb, yb + bt, 1.0, xa, ya + bt, 4.0] } else { [0.0, xa, ya - bt, 1.0, xb, yb - bt, 1.0, xb, yb, 1.0, xa, ya, 4.0] };
            pen.fill(&cmds, 0.0, top);
            i = jj + 1;
        }
    }
}

/// The percent sign of a bar `\repeat percent` copies, in the middle of
/// the bar: a slash from the fourth line to the second, a dot in the third
/// space before it and one in the second after it, LilyPond's measures.
fn draw_percent(pen: &mut Pen, bar: &Bar, top: f64) {
    let c = (bar.x0 + bar.x1) / 2.0;
    let (y0, y1) = (top + 2.94, top + 1.0);
    pen.fill(&[0.0, c - 1.28, y0, 1.0, c - 0.41, y0, 1.0, c + 1.28, y1, 1.0, c + 0.41, y1, 4.0], 0.0, 0.0);
    let dot = glyph("augmentationDot");
    let half = (dot.bbox[0] + dot.bbox[2]) / 2.0;
    pen.glyph("augmentationDot", c - 0.81 - half, top + 1.5);
    pen.glyph("augmentationDot", c + 0.84 - half, top + 2.5);
}

/// Pieces of markup from `x` on the baseline `y`, a word space before each
/// that asks for one; a music glyph stands on the baseline, a superscript
/// smaller and raised.
fn draw_runs(pen: &mut Pen, runs: &[Run], mut x: f64, y: f64, e: f64) {
    let space = text_width(" ", Weight::Regular, e);
    for (k, r) in runs.iter().enumerate() {
        let (re, rise) = run_em(r, e);
        if k > 0 && r.gap {
            x += space * re / e;
        }
        if let Some(d) = crate::frets::from_spec(&r.text) {
            // A guitar's chord: its diagram standing on the baseline.
            let (w, h) = crate::frets::size(&d, re);
            for shape in crate::frets::shapes(&d, x, y - rise - h, re) {
                match shape {
                    crate::frets::Shape::Rect(sx, sy, sw, sh) => pen.rect(sx, sy, sw, sh),
                    crate::frets::Shape::Fill(cmds) => pen.fill(&cmds, 0.0, 0.0),
                    crate::frets::Shape::Text(t, tx, ty, te) => {
                        pen.text(&t, Weight::Regular, tx, ty, te);
                    }
                }
            }
            x += w;
        } else if r.glyph {
            let g = glyph(&r.text);
            let sp = pen.sp;
            pen.push(fill_k(&g.fill, x - g.bbox[0] * r.size, y - rise - g.bbox[3] * r.size, r.size, sp));
            x += glyph_run_w(r);
        } else if r.text == crate::marks::TRIANGLE {
            // An outline: the outer edge one way round, the inner the other.
            let (w, h, t) = (0.55 * re, 0.55 * re, 0.06 * re);
            let (x0, y0) = (x + 0.04 * re, y - rise);
            let (ti, wi) = (t * 1.8, t * 1.0);
            pen.fill(&[0.0, x0, y0, 1.0, x0 + w / 2.0, y0 - h, 1.0, x0 + w, y0, 4.0, 0.0, x0 + ti, y0 - wi, 1.0, x0 + w - ti, y0 - wi, 1.0, x0 + w / 2.0, y0 - h + ti * 1.2, 4.0], 0.0, 0.0);
            x += 0.62 * re;
        } else {
            x = pen.text(&r.text, r.weight, x, y - rise, re);
        }
    }
}

/// A tablature's six strings, each broken where a fret is written on it.
fn draw_strings(pen: &mut Pen, bars: &[Bar], line: &Line, top: f64) {
    let e = crate::engrave::fret_em();
    let mut gaps: Vec<Vec<(f64, f64)>> = vec![Vec::new(); 6];
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            let xc = ev.x + ev.head_w.unwrap_or(0.0) / 2.0;
            for &(s, fret) in &ev.tab {
                let w = text_width(&fret.to_string(), Weight::Bold, e);
                if let Some(g) = gaps.get_mut((s - 1) as usize) {
                    g.push((xc - w / 2.0 - 0.15, xc + w / 2.0 + 0.15));
                }
            }
        }
    }
    for (k, g) in gaps.iter_mut().enumerate() {
        g.sort_by(|a, b| a.0.total_cmp(&b.0));
        let y = top + k as f64 * 1.5 - RULE.staff / 2.0;
        let mut x = line.left;
        for &(a, b) in g.iter() {
            if a > x {
                pen.rect(x, y, a - x, RULE.staff);
            }
            x = x.max(b);
        }
        if line.right > x {
            pen.rect(x, y, line.right - x, RULE.staff);
        }
    }
}

/// A note's frets on its strings, centred where its head would be.
fn draw_frets(pen: &mut Pen, ev: &Ev, top: f64) {
    let e = crate::engrave::fret_em();
    let xc = ev.x + ev.head_w.unwrap_or(0.0) / 2.0;
    for &(s, fret) in &ev.tab {
        let text = fret.to_string();
        let w = text_width(&text, Weight::Bold, e);
        let (t, b) = text_box(&text, Weight::Bold, e);
        let y = top + (s - 1) as f64 * 1.5 - (t + b) / 2.0;
        pen.text(&text, Weight::Bold, xc - w / 2.0, y, e);
    }
}

/// A straight line `thick` thick from `(x0, y0)` to `(x1, y1)`.
fn draw_line_seg(pen: &mut Pen, x0: f64, y0: f64, x1: f64, y1: f64, thick: f64) {
    let h = thick / 2.0;
    pen.fill(&[0.0, x0, y0 - h, 1.0, x1, y1 - h, 1.0, x1, y1 + h, 1.0, x0, y0 + h, 4.0], 0.0, 0.0);
}

/// An arpeggio's wavy line at `x`, from `y0` down to `y1`: the wiggle's
/// segment again every 0.8 of a space, as LilyPond stacks it.
fn draw_arpeggio(pen: &mut Pen, x: f64, y0: f64, y1: f64) {
    let g = glyph("wiggleArpeggiatoUp");
    let step = g.advance;
    let mut y = y1 - g.bbox[3];
    while y + g.bbox[1] > y0 - step {
        pen.glyph("wiggleArpeggiatoUp", x, y);
        y -= step;
    }
}

/// A tremolo's strokes across a stem at `x`, centred on `y` — or over a
/// whole note's head: a stroke for each beam the repetition has that the
/// note has not.
fn draw_tremolo(pen: &mut Pen, ev: &Ev, top: f64, x: f64, y: f64, up: bool) {
    let _ = top;
    let mut strokes = 0;
    let mut v = ev.tremolo;
    while v > 4 {
        strokes += 1;
        v /= 2;
    }
    let n = (strokes - flags(ev.base)).max(1);
    let (w, rise) = (if ev.base == 1 { 1.4 } else { 1.1 }, 0.5);
    let step = RULE.beam_step;
    let height = (n - 1) as f64 * step;
    let first = if ev.base == 1 {
        if up {
            y - 1.4 - height
        } else {
            y + 1.4
        }
    } else {
        y - height / 2.0
    };
    for k in 0..n {
        let yc = first + k as f64 * step;
        let (xa, xb) = (x - w / 2.0, x + w / 2.0);
        let (ya, yb) = (yc + rise / 2.0, yc - rise / 2.0);
        let t = RULE.beam;
        pen.fill(&[0.0, xa, ya - t / 2.0, 1.0, xb, yb - t / 2.0, 1.0, xb, yb + t / 2.0, 1.0, xa, ya + t / 2.0, 4.0], 0.0, 0.0);
    }
}

/// `\ottava`'s bracket: its figure in bold, dashes to the end of the run,
/// and a hook back toward the staff where it ends.
fn draw_ottava(pen: &mut Pen, ot: &OttavaPlace, top: f64) {
    let e = ottava_em();
    let y = top + ot.y;
    let end_text = pen.text(&ot.text, Weight::Bold, ot.x0, y, e);
    let (t, b) = text_box(&ot.text, Weight::Bold, e);
    let mid = y + (t + b) / 2.0;
    let line_y = mid;
    let mut x = end_text + 0.3;
    let (dash, gap, thick) = (0.6, 0.4, 0.1);
    while x < ot.x1 {
        let to = (x + dash).min(ot.x1);
        pen.rect(x, line_y - thick / 2.0, to - x, thick);
        x = to + gap;
    }
    if ot.hook {
        let h = 0.8;
        if ot.up {
            pen.rect(ot.x1 - thick, line_y - thick / 2.0, thick, h);
        } else {
            pen.rect(ot.x1 - thick, line_y - h + thick / 2.0, thick, h);
        }
    }
}

/// A dashed line from `x0` to `x1` at `y`, as an ottava's or a spanner's.
fn dashes(pen: &mut Pen, mut x: f64, x1: f64, y: f64) {
    let (dash, gap, thick) = (0.6, 0.4, 0.1);
    while x < x1 {
        let to = (x + dash).min(x1);
        pen.rect(x, y - thick / 2.0, to - x, thick);
        x = to + gap;
    }
}

/// A text spanner: its words, then dashes.
fn draw_span(pen: &mut Pen, sp: &crate::marks::SpanPlace, top: f64) {
    let e = em(0.0);
    let y = top + sp.y;
    let mut x = sp.x0;
    let mid = if sp.runs.is_empty() {
        y
    } else {
        let w = crate::marks::runs_width(&sp.runs, e);
        draw_runs(pen, &sp.runs, sp.x0, y, e);
        x += w + 0.4;
        let (t, b) = crate::marks::runs_box(&sp.runs, e);
        y + (t + b) / 2.0
    };
    dashes(pen, x, sp.x1, mid);
}

/// A rehearsal mark, a segno or a coda.
fn draw_mark(pen: &mut Pen, mk: &MarkPlace, top: f64) {
    draw_runs(pen, &mk.runs, mk.x, top + mk.y, mark_em());
}

/// A note's grace notes, small, before it: heads, accidentals and ledger
/// lines, stems with a flag or under one beam, the slash of an
/// acciaccatura, and its slur — or an appoggiatura's — to the note.
fn draw_graces(pen: &mut Pen, ev: &Ev, top: f64) {
    let Some(first) = ev.graces.first() else { return };
    let k = GRACE_K;
    let sp = pen.sp;
    let beamed = first.beam.is_some();
    let mut stems: Vec<f64> = Vec::new();
    for g in &ev.graces {
        let x = ev.x + g.x;
        let hname = head_name(g.base);
        let hw = g.head_w.unwrap_or(0.0);
        let hi = g.heads.last().map_or(0, |h| h.pos);
        let lo = g.heads.first().map_or(0, |h| h.pos);
        let lt = RULE.ledger * k;
        let mut pos = 6;
        while pos <= hi {
            pen.rect(x - hw * 0.2, top + y_of(pos) - lt / 2.0, hw * 1.4, lt);
            pos += 2;
        }
        pos = -6;
        while pos >= lo {
            pen.rect(x - hw * 0.2, top + y_of(pos) - lt / 2.0, hw * 1.4, lt);
            pos -= 2;
        }
        for hd in &g.heads {
            let y = top + y_of(hd.pos);
            pen.push(fill_k(&glyph(hname).fill, x, y, k, sp));
            if let Some(acc) = hd.acc {
                let a = glyph(acc_name(acc));
                pen.push(fill_k(&a.fill, x - a.advance * k - 0.15, y, k, sp));
            }
        }
        if g.base == 1 {
            continue;
        }
        let head = glyph(hname);
        let tip = top + g.stem_end;
        let sx = if g.up { x + hw - RULE.stem * k } else { x };
        if g.up {
            let root = top + y_of(lo) + head.anchors.get("stemUpSE").map_or(0.0, |a| a[1]) * k;
            pen.rect(sx, tip, RULE.stem * k, root - tip);
        } else {
            let root = top + y_of(hi) + head.anchors.get("stemDownNW").map_or(0.0, |a| a[1]) * k;
            pen.rect(sx, root, RULE.stem * k, tip - root);
        }
        stems.push(sx);
        if !beamed && flags(g.base) > 0 {
            let flag = flag_name(g.base, g.up);
            let anchor = glyph(&flag).anchors.get(if g.up { "stemUpNW" } else { "stemDownSW" }).copied().unwrap_or([0.0, 0.0]);
            pen.push(fill_k(&glyph(&flag).fill, sx - anchor[0] * k, tip - anchor[1] * k, k, sp));
        }
        let slashed = matches!(g.grace, Some(GraceKind::Acciaccatura | GraceKind::Slashed));
        if slashed && (ev.graces.len() == 1 || std::ptr::eq(g, first)) {
            let name = if g.up { "graceNoteSlashStemUp" } else { "graceNoteSlashStemDown" };
            pen.push(fill_k(&glyph(name).fill, sx, tip, k, sp));
        }
    }
    if beamed && stems.len() > 1 {
        let up = first.up;
        let n = ev.graces.iter().map(|g| flags(g.base)).max().unwrap_or(1);
        let y = top + first.stem_end;
        let (xa, xb) = (stems[0], stems[stems.len() - 1] + RULE.stem * k);
        let t = RULE.beam * k;
        for level in 0..n {
            let off = level as f64 * RULE.beam_step * k * if up { 1.0 } else { -1.0 };
            let y0 = if up { y + off } else { y + off - t };
            pen.rect(xa, y0, xb - xa, t);
        }
    }
    if matches!(first.grace, Some(GraceKind::Acciaccatura | GraceKind::Appoggiatura)) && !ev.heads.is_empty() {
        // Under the heads when the grace stems are up, over them when down.
        let below = first.up;
        let dir = if below { 1.0 } else { -1.0 };
        let gx = ev.x + first.x + first.head_w.unwrap_or(0.0) / 2.0;
        let gy = if below { y_of(first.heads.first().map_or(0, |h| h.pos)) + 0.5 } else { y_of(first.heads.last().map_or(0, |h| h.pos)) - 0.5 };
        let nx = ev.x + ev.head_w.unwrap_or(0.0) / 2.0;
        let ny = if below { y_of(ev.heads.first().map_or(0, |h| h.pos)) + 0.6 } else { y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.6 };
        let hgt = crate::marks::curve_height((nx - gx).max(0.1), 1.0, 0.333);
        if let Some(cmds) = crate::marks::arc_cmds(gx, gy, nx, ny, dir, hgt) {
            pen.fill(&cmds, 0.0, top);
        }
    }
}

/// A ring at `(x, y)`, `r` its outer radius, `t` thick: the outer circle one
/// way round and the inner the other, so the fill leaves the middle empty.
fn draw_ring(pen: &mut Pen, x: f64, y: f64, r: f64, t: f64) {
    let circle = |r: f64, turn: f64| -> Vec<f64> {
        let c = 0.552_284_749_830_793_4 * r;
        let ry = r * turn;
        let cy = c * turn;
        vec![
            0.0,
            x + r,
            y,
            3.0,
            x + r,
            y + cy,
            x + c,
            y + ry,
            x,
            y + ry,
            3.0,
            x - c,
            y + ry,
            x - r,
            y + cy,
            x - r,
            y,
            3.0,
            x - r,
            y - cy,
            x - c,
            y - ry,
            x,
            y - ry,
            3.0,
            x + c,
            y - ry,
            x + r,
            y - cy,
            x + r,
            y,
            4.0,
        ]
    };
    let mut cmds = circle(r, 1.0);
    cmds.extend(circle(r - t, -1.0));
    pen.fill(&cmds, 0.0, 0.0);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// A dotted eighth, stem up and unbeamed: on a line its dot goes in the
    /// space above, where the flag hangs — it stands past the flag; in a
    /// space, under the flag's tip, it stays by the head.
    #[test]
    fn a_dotted_eighths_dot_stands_past_its_flag() {
        let eng = crate::engrave(r"{ \time 2/4 g'8.~ g'4 r16 | f'8.~ f'4 r16 | a'8.~ a'4 r16 }", 120.0, &crate::Layout::default()).unwrap();
        let dotted: Vec<&Ev> = eng.staves[0].iter().flat_map(|b| &b.items).filter(|e| e.dots == 1 && e.k == Kind::Note).collect();
        assert_eq!(dotted.len(), 3);
        let dot_pos = |ev: &Ev| {
            let pos = ev.heads[0].pos;
            if pos % 2 == 0 {
                pos + 1
            } else {
                pos
            }
        };
        let flag = glyph(&flag_name(8, true));
        for ev in &dotted {
            assert!(ev.up && ev.beam.is_none(), "a lone eighth, stem up");
            let flag_right = stem_x(ev) - ev.x - RULE.stem - flag.anchors["stemUpNW"][0] + flag.bbox[2];
            let moved = past_flag(ev, dot_pos(ev));
            let flag_bottom = ev.stem_end + flag.bbox[3];
            let dot_top = y_of(dot_pos(ev)) - 0.224;
            if dot_top < flag_bottom {
                // The flag reaches the dot's height: the dot starts right of it.
                assert!(moved >= flag_right + 0.1, "the dot at {moved} is under the flag ending at {flag_right}");
            } else {
                assert!(moved == 0.0, "a dot clear of the flag stays by its head");
            }
        }
        // g' is on a line (its dot under the flag), f' and a' in a space.
        assert!(past_flag(dotted[0], dot_pos(dotted[0])) > 0.0);
        assert!(past_flag(dotted[2], dot_pos(dotted[2])) == 0.0);
    }
}
