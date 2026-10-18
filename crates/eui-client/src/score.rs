//! Spec 03 §1.4: music the client engraves.
//!
//! A `score` carries LilyPond source and the client lays it out with
//! `eui-score` — the engraver the catalogue's `score()` uses on the server,
//! held to it by that crate's golden tests — so an edit is a new string and
//! not a new set of paths. This module keeps the engravings, answers layout
//! for a page's height, hands the painter a page as canvas paths in their
//! colours, and works out where in the music a press landed (06 §1.5).
//!
//! An engraving is keyed by what moves the music — the source, the width, the
//! staff space and the page settings — and by nothing that only redraws it:
//! a new `marks`, `lit`, `page_index` or `colour` repaints from the one kept.

use std::collections::HashMap;
use std::sync::Arc;

use eui_proto::Value;
use eui_score::{Engraved, Layout, Paper, Path};
use eui_tree::ScoreProps;

/// The largest source a client engraves, in bytes (03 §1.4).
pub const MAX_SRC: usize = 512 * 1024;

/// Engravings kept. A page shows a handful of scores; an editor keeps one
/// and replaces it at every edit.
const KEEP: usize = 16;

/// What moves the music. Floats are kept as quarter pixels, so a width that
/// differs only in noise is the same engraving.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    src: [u8; 32],
    width: i64,
    staff_space: i64,
    a4: bool,
    page_height: i64,
    /// How far the systems were lowered, in quarter spaces: see
    /// [`Scores::lower`].
    lowered: i64,
}

fn quarter(v: f32) -> i64 {
    (v * 4.0).round() as i64
}

/// A source laid out, and the scale it is drawn at.
#[derive(Debug, Clone)]
pub struct Laid {
    /// The engraving, in staff spaces.
    pub eng: Engraved,
    /// px a staff space.
    pub sp: f64,
    /// Laid out on A4 sheets.
    pub a4: bool,
    /// Paged on a screen.
    pub paged: bool,
}

/// One engraving, or the engraver's refusal. `serial` tells one engraving
/// from the next for `score_info`, which is sent once each (06 §1.5).
#[derive(Debug)]
pub struct Engraving {
    /// The music, or why there is none.
    pub laid: Result<Laid, String>,
    /// Which engraving this is, counted from 1 in this client.
    pub serial: u64,
}

impl Engraving {
    /// Pages the music takes, at least one.
    pub fn pages(&self) -> usize {
        match &self.laid {
            Ok(l) => l.eng.pages.len().max(1),
            Err(_) => 1,
        }
    }

    /// The refusal, or `""`.
    pub fn refusal(&self) -> &str {
        match &self.laid {
            Ok(_) => "",
            Err(e) => e,
        }
    }

    /// `page_index` as drawn: one past the last draws the last.
    pub fn page_shown(&self, page_index: u32) -> usize {
        (page_index as usize).min(self.pages() - 1)
    }

    /// The height of the page drawn, px (03 §1.4), as the catalogue sizes
    /// it: an A4 sheet; on a paged screen, down to three spaces under the
    /// page's lowest system (`score_ed_page_height`); otherwise the whole
    /// engraving (`score()`).
    pub fn height(&self, page_index: u32) -> f32 {
        let Ok(l) = &self.laid else { return 0.0 };
        if l.a4 {
            return (Paper::a4().h * l.sp).floor() as f32;
        }
        if l.paged {
            let page = self.page_shown(page_index);
            let bottom = l.eng.systems.iter().filter(|s| s.page == page).map(|s| s.bottom).fold(0.0_f64, f64::max);
            return ((bottom + 3.0) * l.sp).floor() as f32;
        }
        (l.eng.height * l.sp).ceil() as f32
    }
}

/// Every engraving this client holds.
#[derive(Debug, Default)]
pub struct Scores {
    kept: HashMap<Key, Arc<Engraving>>,
    order: Vec<Key>,
    /// Sources an edit made in this client, and how many staff spaces lower
    /// their systems are drawn than the engraver puts them (03 §1.4).
    lowered: HashMap<[u8; 32], f64>,
    lowered_order: Vec<[u8; 32]>,
    /// Engravings made, ever: how a test tells a redraw from an engraving.
    pub engraved: u64,
}

impl Scores {
    /// The engraving of `p` at `width` px, made now if it is not kept —
    /// lowered, when an edit here asked for it.
    pub fn engraving(&mut self, p: &ScoreProps<'_>, width: f32) -> Arc<Engraving> {
        let src = *blake3::hash(p.src.as_bytes()).as_bytes();
        let lower = if p.a4 { 0.0 } else { self.lowered.get(&src).copied().unwrap_or(0.0) };
        let key = Key { src, width: quarter(width), staff_space: quarter(p.staff_space), a4: p.a4, page_height: quarter(p.page_height), lowered: quarter(lower as f32) };
        if let Some(e) = self.kept.get(&key) {
            return Arc::clone(e);
        }
        let laid = if key.lowered > 0 {
            let raw = self.engraving_at(Key { lowered: 0, ..key }, p, width);
            raw.laid.clone().map(|l| Laid { eng: lowered(l.eng, lower), ..l })
        } else {
            engrave(p, width)
        };
        self.keep(key, laid)
    }

    fn engraving_at(&mut self, key: Key, p: &ScoreProps<'_>, width: f32) -> Arc<Engraving> {
        if let Some(e) = self.kept.get(&key) {
            return Arc::clone(e);
        }
        self.keep(key, engrave(p, width))
    }

    fn keep(&mut self, key: Key, laid: Result<Laid, String>) -> Arc<Engraving> {
        self.engraved = self.engraved.saturating_add(1);
        let e = Arc::new(Engraving { laid, serial: self.engraved });
        if self.order.len() >= KEEP {
            let gone = self.order.remove(0);
            self.kept.remove(&gone);
        }
        self.order.push(key);
        self.kept.insert(key, Arc::clone(&e));
        e
    }

    /// Draw `src` with its systems `spaces` lower than the engraver puts
    /// them. An edit changes the ink, and the engraver places the first
    /// system by its highest ink — a B's stem up, a C's down — so the whole
    /// score moved under the hand at every step. The client keeps the
    /// edited system where it was instead (03 §1.4).
    pub fn lower(&mut self, src: &str, spaces: f64) {
        let key = *blake3::hash(src.as_bytes()).as_bytes();
        if !self.lowered.contains_key(&key) {
            if self.lowered_order.len() >= KEEP {
                let gone = self.lowered_order.remove(0);
                self.lowered.remove(&gone);
            }
            self.lowered_order.push(key);
        }
        self.lowered.insert(key, spaces);
    }
}

/// Every system `dy` spaces lower, and the music that much taller.
fn lowered(mut eng: Engraved, dy: f64) -> Engraved {
    for sys in &mut eng.systems {
        sys.top += dy;
        sys.bottom += dy;
        for line in &mut sys.lines {
            line.top += dy;
            line.bottom += dy;
        }
    }
    eng.height += dy;
    eng
}

/// The top of the system holding `item`, in staff spaces.
pub fn item_top(e: &Engraving, item: usize) -> Option<f64> {
    let l = e.laid.as_ref().ok()?;
    for sys in &l.eng.systems {
        for (n, line) in sys.lines.iter().enumerate() {
            let staff = if sys.multi { n } else { line.staff_index.unwrap_or(0) };
            let Some(bars) = l.eng.staves.get(staff).and_then(|b| b.get(line.bars.clone())) else { continue };
            if bars.iter().flat_map(|b| b.items.iter()).any(|ev| ev.i == item) {
                return Some(sys.top);
            }
        }
    }
    None
}

fn engrave(p: &ScoreProps<'_>, width: f32) -> Result<Laid, String> {
    if p.src.len() > MAX_SRC {
        return Err(format!("score: the source is {} bytes, past the 512 KiB a client engraves", p.src.len()));
    }
    if !(width.is_finite() && width > 0.0) {
        return Err("score: no width to lay the music out in".into());
    }
    if p.a4 {
        let paper = Paper::a4();
        let eng = eui_score::engrave_a4(&p.src).map_err(|e| e.message)?;
        return Ok(Laid { eng, sp: f64::from(width) / paper.w, a4: true, paged: true });
    }
    let sp = f64::from(p.staff_space);
    let paged = p.page_height > 0.0;
    let layout = Layout { page_h: paged.then(|| f64::from(p.page_height) / sp), ..Layout::default() };
    let eng = eui_score::engrave(&p.src, f64::from(width) / sp, &layout).map_err(|e| e.message)?;
    Ok(Laid { eng, sp, a4: false, paged })
}

/// The page drawn, as canvas paths (03 §1.1, kinds `1` and `5`) in their
/// colours: `colour` for the ink, an item's `marks` entry instead, and
/// `accent.base` for one in `lit`. The same values lang's `eui_score_bands`
/// writes, so a client that engraves and a server that falls back draw one
/// picture.
pub fn page_paths(e: &Engraving, page_index: u32, ink: &Value, marks: Option<&Value>, lit: Option<&Value>) -> Vec<Value> {
    let Ok(l) = &e.laid else { return Vec::new() };
    let page = e.page_shown(page_index);
    let Ok((paths, ranges)) = eui_score::paths(&l.eng, l.sp, page) else {
        return Vec::new();
    };
    let mut colours: Vec<Option<Value>> = vec![None; paths.len()];
    let marked = |item: usize| -> Option<Value> {
        if let Some(Value::List(items)) = lit {
            if items.iter().any(|v| matches!(v, Value::Int(n) if usize::try_from(*n).ok() == Some(item))) {
                return Some(Value::Str("accent.base".into()));
            }
        }
        // `List[List[Int item, colour]]`: a value is never a map (02 §4.4).
        match marks {
            Some(Value::List(pairs)) => pairs.iter().find_map(|pair| match pair {
                Value::List(kv) => match kv.as_slice() {
                    [Value::Int(n), colour] if usize::try_from(*n).ok() == Some(item) => Some(colour.clone()),
                    _ => None,
                },
                _ => None,
            }),
            _ => None,
        }
    };
    for (item, range) in &ranges {
        if let Some(c) = marked(*item) {
            for slot in colours.iter_mut().skip(range.start).take(range.len()) {
                *slot = Some(c.clone());
            }
        }
    }
    paths.iter().zip(colours).map(|(p, c)| path_value(p, c.as_ref().unwrap_or(ink))).collect()
}

fn path_value(p: &Path, ink: &Value) -> Value {
    match p {
        Path::Rect([x, y, w, h]) => Value::List(vec![Value::Int(1), ink.clone(), Value::Float(*x), Value::Float(*y), Value::Float(*w), Value::Float(*h), Value::Int(0)]),
        Path::Fill(cmds) => {
            let mut out = Vec::with_capacity(cmds.len() + 2);
            out.push(Value::Int(5));
            out.push(ink.clone());
            let mut k = 0;
            while let Some(c) = cmds.get(k) {
                out.push(Value::Int(*c as i64));
                let args = match *c as i64 {
                    0 | 1 => 2,
                    2 => 4,
                    3 => 6,
                    _ => 0,
                };
                out.extend(cmds.iter().skip(k + 1).take(args).map(|v| Value::Float(*v)));
                k += 1 + args;
            }
            Value::List(out)
        }
    }
}

/// Where in the music a point is (06 §1.5), the point in px from the
/// score's content box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    /// The page drawn.
    pub page: usize,
    /// The system on it, from 0 down the page.
    pub system: usize,
    /// The staff of that system, from 0 at the top.
    pub staff: usize,
    /// Half staff spaces down from that staff's top line.
    pub pos: i64,
    /// The item whose head's centre is nearest across; `-1` for none.
    pub near: i64,
    /// How far that centre is, in staff spaces; `-1` with no `near`.
    pub gap: f64,
    /// The last item left of the point; `-1` for none.
    pub before: i64,
    /// The first right of it; `-1` for none.
    pub after: i64,
    /// Between `before`'s centre and `after`'s, `0..=1`; `-1` unless both.
    pub frac: f64,
}

/// The head's width an item without one is measured with, in spaces — a
/// notehead, as the editor takes it.
const HEAD_W: f64 = 1.18;

/// Where `(x, y)` px falls in the music of the page drawn, or `None` for a
/// score with no music — refused, or with no system on that page.
pub fn pick(e: &Engraving, page_index: u32, x: f32, y: f32) -> Option<Pick> {
    let l = e.laid.as_ref().ok()?;
    let page = e.page_shown(page_index);
    let (xs, ys) = (f64::from(x) / l.sp, f64::from(y) / l.sp);
    // The staff whose middle line is nearest the point, on this page.
    let mut best: Option<(usize, usize, usize, f64, f64)> = None; // (system on page, index, staff, top, distance)
    for (n, (ix, sys)) in l.eng.systems.iter().enumerate().filter(|(_, s)| s.page == page).enumerate() {
        let tops: Vec<f64> = if sys.multi { sys.offsets.iter().map(|o| sys.top + o).collect() } else { vec![sys.top] };
        for (staff, top) in tops.into_iter().enumerate() {
            let d = (ys - top - 2.0).abs();
            if best.is_none_or(|b| d < b.4) {
                best = Some((n, ix, staff, top, d));
            }
        }
    }
    let (system, ix, staff, top, _) = best?;
    let sys = l.eng.systems.get(ix)?;
    let line = if sys.multi { sys.lines.get(staff)? } else { sys.lines.first()? };
    let bars = l.eng.staves.get(if sys.multi { staff } else { line.staff_index.unwrap_or(0) })?.get(line.bars.clone())?;
    let (mut near, mut gap) = (-1_i64, f64::MAX);
    let (mut before, mut after) = ((-1_i64, 0.0_f64), (-1_i64, 0.0_f64));
    for ev in bars.iter().flat_map(|b| b.items.iter()) {
        let mid = ev.x + ev.head_w.unwrap_or(HEAD_W) / 2.0;
        let item = ev.i as i64;
        if (xs - mid).abs() < gap {
            gap = (xs - mid).abs();
            near = item;
        }
        if mid <= xs {
            before = (item, mid);
        } else if after.0 < 0 {
            after = (item, mid);
        }
    }
    let frac = if before.0 >= 0 && after.0 >= 0 && after.1 > before.1 { ((xs - before.1) / (after.1 - before.1)).clamp(0.0, 1.0) } else { -1.0 };
    let gap = if near >= 0 { gap } else { -1.0 };
    Some(Pick { page, system, staff, pos: ((ys - top) * 2.0).round() as i64, near, gap, before: before.0, after: after.0, frac })
}

/// `score_pick`'s payload (06 §1.5).
pub fn pick_payload(phase: i64, button: u8, p: &Pick, x: f32, y: f32) -> Value {
    Value::List(vec![
        Value::Int(phase),
        Value::Int(i64::from(button)),
        Value::Int(p.page as i64),
        Value::Int(p.system as i64),
        Value::Int(p.staff as i64),
        Value::Int(p.pos),
        Value::Int(p.near),
        Value::Float(p.gap),
        Value::Int(p.before),
        Value::Int(p.after),
        Value::Float(p.frac),
        Value::Float(f64::from(x)),
        Value::Float(f64::from(y)),
    ])
}

/// `score_info`'s payload (06 §1.5): the pages, the refusal, and for each
/// page the lowest item number of each staff on it, `-1` for none.
pub fn info_payload(e: &Engraving) -> Value {
    let starts = match &e.laid {
        Ok(l) => {
            let staves = l.eng.staves.len().max(1);
            let mut firsts: Vec<Vec<i64>> = vec![vec![-1; staves]; e.pages()];
            for sys in &l.eng.systems {
                let Some(page) = firsts.get_mut(sys.page) else { continue };
                for (n, line) in sys.lines.iter().enumerate() {
                    let staff = if sys.multi { n } else { line.staff_index.unwrap_or(0) };
                    let Some(bars) = l.eng.staves.get(staff).and_then(|b| b.get(line.bars.clone())) else { continue };
                    if let (Some(slot), Some(low)) = (page.get_mut(staff), bars.iter().flat_map(|b| b.items.iter()).map(|ev| ev.i as i64).min()) {
                        *slot = if *slot < 0 { low } else { (*slot).min(low) };
                    }
                }
            }
            firsts.into_iter().map(|p| Value::List(p.into_iter().map(Value::Int).collect())).collect()
        }
        Err(_) => Vec::new(),
    };
    Value::List(vec![Value::Int(e.pages() as i64), Value::Str(e.refusal().to_owned()), Value::List(starts)])
}
