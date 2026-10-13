//! An engraving drawn as canvas paths (03 §1.1): staff lines, stems and bar
//! lines as rectangles (kind 1), everything curved as fills (kind 5), in
//! pixels at `sp` a staff space, rounded to the hundredth
//! (`eui_builders_score.sl`, "Drawing").

use std::ops::Range;

use crate::engrave::{
    acc_name, barline_parts, barline_width, beam_y, church_width, clef_glyph, digits, flag_name, flags, head_name, key_cancel, part_width, rest_name, stem_x, time_glyph, time_width, y_of, Courtesy,
    Engraved, Line, System,
};
use crate::glyphs::{glyph, letters, Weight};
use crate::marks::{dyn_letters, rest_lift, DynEl, TempoMark, TupletPlace, Volta};
use crate::measure::{Bar, Beam, Opening};
use crate::parse::{Clef, Ev, GroupKind, Kind};
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

struct Pen<'a> {
    paths: &'a mut Vec<Path>,
    sp: f64,
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

    /// `words` on a baseline at `y` from `x`; answers where they end.
    fn text(&mut self, words: &str, weight: Weight, x: f64, y: f64, em: f64) -> f64 {
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
    let mut paths = head_paths(eng, sp, page);
    for sys in &eng.systems {
        if sys.page == page {
            draw_system(&mut paths, eng, sys, sys.top, sp, ranges);
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

/// A page's titles if it is the first, its number if it is numbered.
pub fn head_paths(eng: &Engraved, sp: f64, page: usize) -> Vec<Path> {
    let mut paths = Vec::new();
    let mut pen = Pen { paths: &mut paths, sp };
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
    let margin = if !eng.pages.is_empty() && eng.paper.is_some() { 2.83 } else { 1.0 };
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
    draw_titles(&mut Pen { paths: &mut paths, sp }, eng, page);
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
    let mut pen = Pen { paths: &mut paths, sp };
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

/// One system with its first staff's top line at `top`.
pub fn draw_system(paths: &mut Vec<Path>, eng: &Engraved, sys: &System, top: f64, sp: f64, ranges: &mut Ranges) {
    if sys.multi {
        for (s, line) in sys.lines.iter().enumerate() {
            draw_line(paths, &eng.staves[s], line, top + sys.offsets[s], sp, ranges);
        }
        draw_groups(&mut Pen { paths, sp }, eng, sys, top);
        return;
    }
    draw_line(paths, &eng.staves[0], &sys.lines[0], top, sp, ranges);
}

fn clef_y(clef: Clef) -> f64 {
    match clef {
        Clef::Treble => 3.0,
        Clef::Bass => 1.0,
        Clef::Alto => 2.0,
    }
}

fn draw_line(paths: &mut Vec<Path>, bars: &[Bar], line: &Line, top: f64, sp: f64, ranges: &mut Ranges) {
    let mut pen = Pen { paths, sp };
    let right = line.right;
    let left = line.left;
    for i in 0..5 {
        pen.rect(left, top + i as f64 - RULE.staff / 2.0, right - left, RULE.staff);
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
    pen.glyph(clef, x, top + clef_y(first.clef));
    x += line.clef_w.unwrap_or(glyph(clef).advance) + 1.0;
    if let Some(key_w) = line.key_w {
        if first.fifths != 0 {
            draw_key(&mut pen, first.fifths, first.clef, x, top, false, 0);
        }
        if key_w > 0.0 {
            x += key_w + 0.8;
        }
    } else if first.fifths != 0 {
        x = draw_key(&mut pen, first.fifths, first.clef, x, top, false, 0) + 0.8;
    }
    if first.first || first.changes.time {
        draw_time(&mut pen, first.time, x, top);
    }
    for (bi, b) in line.bars.clone().enumerate() {
        let bar = &bars[b];
        if bi > 0 {
            draw_changes(&mut pen, &bar.opening(), bar.x0, top);
        }
        for ev in &bar.items {
            let from_k = pen.paths.len();
            if ev.k == Kind::Rest {
                draw_rest(&mut pen, ev, top);
            } else {
                draw_note(&mut pen, ev, top);
            }
            ranges.push((ev.i, from_k..pen.paths.len()));
        }
        for g in &bar.beams {
            draw_beam(&mut pen, bar, g, top);
        }
        let kind = bar.drawn();
        draw_bar(&mut pen, kind, bar.x1 + barline_width(kind), top, line.connect_down);
        if bi == 0 {
            if let Some(ox) = bar.open_repeat_x {
                draw_bar(&mut pen, ".|:", ox, top, 0.0);
            }
        }
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
        pen.glyph(sc.glyph, sc.x, top + sc.y);
    }
    for dy in &line.dynamics {
        draw_dynamic(&mut pen, dy, top);
    }
    for tx in &line.texts {
        pen.text(&tx.text, Weight::Regular, tx.x0, top + tx.y, em(0.0));
    }
    if line.courtesy > 0.0 {
        if let Some(Courtesy::Single(o)) = &line.courtesy_bar {
            draw_courtesy(&mut pen, o, line.bar_end, top);
        }
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

fn digit_height(e: f64) -> f64 {
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
    let last = offsets.len() - 1;
    if offsets.len() > 1 {
        pen.rect(left - 0.08, top - 0.05, 0.16, offsets[last] + 4.1);
    }
    let mut edge = left - 0.3;
    let deepest = eng.groups.iter().map(|g| g.depth).max().unwrap_or(0);
    for g in &eng.groups {
        if g.kind == GroupKind::None {
            continue;
        }
        let y0 = top + offsets[g.first];
        let y1 = top + offsets[g.last] + 4.0;
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
        draw_time(pen, o.time, x, top);
    }
}

fn draw_key(pen: &mut Pen, fifths: i64, clef: Clef, mut x: f64, top: f64, cancel: bool, skip: i64) -> f64 {
    let sharps = [4, 1, 5, 2, -1, 3, 0];
    let flats = [0, 3, -1, 2, -2, 1, -3];
    let shift = match clef {
        Clef::Treble => 0,
        Clef::Bass => -2,
        Clef::Alto => -1,
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
        let pos = if fifths > 0 { sharps[i as usize] } else { flats[i as usize] } + shift;
        pen.glyph(name, x, top + y_of(pos));
        x += g.advance + 0.1;
    }
    x
}

fn draw_time(pen: &mut Pen, time: (i64, i64), x: f64, top: f64) {
    if let Some(one) = time_glyph(time) {
        pen.glyph(one, x, top + 2.0);
        return;
    }
    let wide = time_width(time);
    for (pi, part) in [time.0, time.1].into_iter().enumerate() {
        let names = digits(part);
        let w = names.iter().fold(0.0, |acc, nm| acc + glyph(nm).advance);
        let mut cx = x + (wide - w) / 2.0;
        for nm in &names {
            pen.glyph(nm, cx, top + if pi == 0 { 1.0 } else { 3.0 });
            cx += glyph(nm).advance;
        }
    }
}

fn draw_changes(pen: &mut Pen, o: &Opening, x0: f64, top: f64) {
    let ch = o.changes;
    let mut x = x0 + 0.6;
    if ch.clef {
        let clef = clef_glyph(o.clef);
        pen.glyph(clef, x, top + clef_y(o.clef));
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
        draw_time(pen, o.time, x, top);
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
    let y = if ev.base == 1 { 1.0 } else { 2.0 } + rest_lift(ev);
    pen.glyph(name, ev.x, top + y);
    if ev.dots > 0 {
        let mut dx = ev.x + glyph(name).advance + 0.3;
        for _ in 0..ev.dots {
            pen.glyph("augmentationDot", dx, top + 1.5 + rest_lift(ev));
            dx += 0.6;
        }
    }
}

fn draw_note(pen: &mut Pen, ev: &Ev, top: f64) {
    let hname = head_name(ev.base);
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
    let mut dot_x = ev.x + hw + 0.3;
    if wide && ev.up {
        dot_x += hw - RULE.stem;
    }
    for hd in &ev.heads {
        let hx = ev.x + hd.side as f64 * (hw - RULE.stem);
        let y = top + y_of(hd.pos);
        pen.glyph(hname, hx, y);
        if let Some(acc) = hd.acc {
            pen.glyph(acc_name(acc), ev.x + hd.acc_x.unwrap_or(0.0), y);
        }
        if ev.dots > 0 {
            let dot_pos = if hd.pos % 2 == 0 { hd.pos + 1 } else { hd.pos };
            let mut dx = dot_x;
            for _ in 0..ev.dots {
                pen.glyph("augmentationDot", dx, top + y_of(dot_pos));
                dx += 0.6;
            }
        }
    }
    if ev.base == 1 {
        return;
    }
    let g = glyph(hname);
    let sx;
    let tip = top + ev.stem_end;
    if ev.up {
        let root = top + y_of(lo) + g.anchors.get("stemUpSE").map_or(0.0, |a| a[1]);
        sx = stem_x(ev) - RULE.stem;
        pen.rect(sx, tip, RULE.stem, root - tip);
    } else {
        let root = top + y_of(hi) + g.anchors.get("stemDownNW").map_or(0.0, |a| a[1]);
        sx = stem_x(ev);
        pen.rect(sx, root, RULE.stem, tip - root);
    }
    if ev.beam.is_none() && flags(ev.base) > 0 {
        let flag = flag_name(ev.base, ev.up);
        let anchor = glyph(&flag).anchors.get(if ev.up { "stemUpNW" } else { "stemDownSW" }).copied().unwrap_or([0.0, 0.0]);
        pen.glyph(&flag, sx - anchor[0], tip - anchor[1]);
    }
}

fn draw_beam(pen: &mut Pen, bar: &Bar, g: &Beam, top: f64) {
    let notes: Vec<&Ev> = g.notes.iter().map(|&j| &bar.items[j]).collect();
    let dir = if g.up { 1.0 } else { -1.0 };
    for level in 0..g.n {
        let off = level as f64 * RULE.beam_step * dir;
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
            let bt = RULE.beam;
            let cmds = if g.up { [0.0, xa, ya, 1.0, xb, yb, 1.0, xb, yb + bt, 1.0, xa, ya + bt, 4.0] } else { [0.0, xa, ya - bt, 1.0, xb, yb - bt, 1.0, xb, yb, 1.0, xa, ya, 4.0] };
            pen.fill(&cmds, 0.0, top);
            i = jj + 1;
        }
    }
}
