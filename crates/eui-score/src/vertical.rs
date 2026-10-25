//! The vertical spacing of a page as LilyPond's page layout sets it (score
//! #409): every distance between two things stacked down a page — the top
//! of the page and its first system, two staves of a system, two systems,
//! the last system and the bottom margin — is a spring with a basic
//! distance, a least distance and a stretchability, read from the spacing
//! alists (`system-system-spacing`, `staff-staff-spacing` …), and never
//! shorter than the ink of the two things allows with their padding
//! between — the ink seen as LilyPond's skylines see it, column by column,
//! so a high note over one bar does not push away a staff whose low note is
//! under another. A page too full squeezes its springs towards their least
//! distances (each as far as its basic distance less its least lets it,
//! LilyPond's compressibility) before a system goes to the next page; a page
//! with room stretches them, each in proportion to its stretchability.

use crate::draw::Path;
use crate::parse::Spacing;

/// A spacing alist with every key known, in staff spaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spec {
    /// `basic-distance`.
    pub basic: f64,
    /// `minimum-distance`.
    pub min: f64,
    /// `padding`.
    pub padding: f64,
    /// `stretchability`.
    pub stretch: f64,
}

impl Spec {
    /// LilyPond's default when a spacing alist leaves out `stretchability`:
    /// its basic distance.
    pub(crate) const fn new(basic: f64, min: f64, padding: f64, stretch: Option<f64>) -> Self {
        Spec {
            basic,
            min,
            padding,
            stretch: match stretch {
                Some(s) => s,
                None => basic,
            },
        }
    }

    /// What the source says over `d`: each key it gives.
    pub(crate) fn over(self, s: &Spacing) -> Self {
        Spec { basic: s.basic.unwrap_or(self.basic), min: s.minimum.unwrap_or(self.min), padding: s.padding.unwrap_or(self.padding), stretch: s.stretch.unwrap_or(self.stretch) }
    }

    /// The four numbers of an alist kept by a setting (NaN where it says
    /// nothing), over this one.
    pub(crate) fn over_list(self, l: &[f64]) -> Self {
        let pick = |k: usize, d: f64| l.get(k).copied().filter(|v| v.is_finite()).unwrap_or(d);
        Spec { basic: pick(0, self.basic), min: pick(1, self.min), padding: pick(2, self.padding), stretch: pick(3, self.stretch).max(0.0) }
    }

    /// The spring between two things whose ink needs `need` between their
    /// reference points (padding included): never shorter than that, nor
    /// than the least distance; at its basic distance when nothing is in
    /// the way.
    pub(crate) fn spring(&self, need: f64) -> Spring {
        let min = self.min.max(need);
        Spring { natural: self.basic.max(min), min, stretch: self.stretch.max(0.0), compress: (self.basic - self.min).max(0.0) }
    }
}

/// A distance that gives: its natural length, the least it squeezes to,
/// how readily it stretches and how readily it squeezes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spring {
    /// Its length when nothing pulls or pushes.
    pub natural: f64,
    /// The least.
    pub min: f64,
    /// Stretchability.
    pub stretch: f64,
    /// Compressibility.
    pub compress: f64,
}

impl Spring {
    /// A length that never changes.
    pub(crate) const fn rigid(len: f64) -> Self {
        Spring { natural: len, min: len, stretch: 0.0, compress: 0.0 }
    }

    /// Its length under `force`: stretched when positive, squeezed (never
    /// past its least) when negative.
    pub(crate) fn at(&self, force: f64) -> f64 {
        if force >= 0.0 {
            self.natural + force * self.stretch
        } else {
            (self.natural + force * self.compress).max(self.min)
        }
    }
}

/// The force that makes `springs` add up to `room`: stretching only when
/// `stretch`, squeezing whenever they do not fit at their natural lengths —
/// all at their least when even that does not fit.
pub(crate) fn solve(springs: &[Spring], room: f64, stretch: bool) -> f64 {
    let natural: f64 = springs.iter().map(|s| s.natural).sum();
    if natural <= room {
        let give: f64 = springs.iter().map(|s| s.stretch).sum();
        return if stretch && give > 1e-9 { (room - natural) / give } else { 0.0 };
    }
    // Squeezed: the force where their lengths add up to the room.
    let most = springs.iter().filter(|s| s.compress > 1e-9).map(|s| (s.natural - s.min) / s.compress).fold(0.0, f64::max);
    let (mut lo, mut hi) = (-most, 0.0);
    let total = |f: f64| springs.iter().map(|s| s.at(f)).sum::<f64>();
    if total(lo) >= room {
        return lo;
    }
    for _ in 0..50 {
        let mid = (lo + hi) / 2.0;
        if total(mid) > room {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    (lo + hi) / 2.0
}

/// The width of a skyline's column, in staff spaces.
const COLUMN: f64 = 0.5;

/// The ink of a staff's line, column by column: how high it reaches and
/// how low, in staff spaces from the staff's top line, y down.
#[derive(Debug, Clone, Default)]
pub(crate) struct Skyline {
    /// The first column's left edge.
    x0: f64,
    /// Each column's highest ink (+inf where it has none).
    top: Vec<f64>,
    /// … and lowest (-inf).
    bottom: Vec<f64>,
}

impl Skyline {
    /// Out of paths drawn at `sp` px a space, their top line at 0.
    pub(crate) fn of(paths: &[Path], sp: f64) -> Self {
        let mut boxes: Vec<[f64; 4]> = Vec::with_capacity(paths.len());
        for p in paths {
            match p {
                Path::Rect([x, y, w, h]) => boxes.push([x / sp, y / sp, (x + w) / sp, (y + h) / sp]),
                Path::Fill(cmds) => fill_boxes(cmds, sp, &mut boxes),
            }
        }
        let x0 = boxes.iter().map(|b| b[0]).fold(f64::INFINITY, f64::min);
        let x1 = boxes.iter().map(|b| b[2]).fold(f64::NEG_INFINITY, f64::max);
        if !x0.is_finite() || !x1.is_finite() {
            return Skyline::default();
        }
        let x0 = (x0 / COLUMN).floor() * COLUMN;
        let n = (((x1 - x0) / COLUMN).ceil() as usize + 1).min(100_000);
        let mut sky = Skyline { x0, top: vec![f64::INFINITY; n], bottom: vec![f64::NEG_INFINITY; n] };
        for b in boxes {
            let k0 = (((b[0] - x0) / COLUMN).floor().max(0.0) as usize).min(n - 1);
            let k1 = (((b[2] - x0) / COLUMN).floor().max(0.0) as usize).min(n - 1);
            for k in k0..=k1 {
                sky.top[k] = sky.top[k].min(b[1]);
                sky.bottom[k] = sky.bottom[k].max(b[3]);
            }
        }
        sky
    }

    /// Every column whose lowest ink is under `y` reaching `by` lower.
    pub(crate) fn lower_from(&mut self, y: f64, by: f64) {
        for b in &mut self.bottom {
            if *b > y {
                *b += by;
            }
        }
    }

    /// How far below `self`'s reference point `under`'s must be for no ink
    /// of `under` to come above any ink of `self` in the same column: none
    /// when no column has ink of both.
    pub(crate) fn clear_of(&self, under: &Skyline) -> Option<f64> {
        let mut need: Option<f64> = None;
        for (k, &low) in self.bottom.iter().enumerate() {
            if !low.is_finite() {
                continue;
            }
            let x = self.x0 + k as f64 * COLUMN;
            let j = ((x - under.x0) / COLUMN).round();
            if j < 0.0 || j as usize >= under.top.len() {
                continue;
            }
            let high = under.top[j as usize];
            if high.is_finite() {
                need = Some(need.map_or(low - high, |n: f64| n.max(low - high)));
            }
        }
        need
    }
}

/// A fill's outline as boxes: a small fill one box, a wide one (a beam, a
/// slur, a line of words) its edges in pieces a column wide, each piece's
/// box — so a sloping beam's skyline slopes with it.
fn fill_boxes(cmds: &[f64], sp: f64, out: &mut Vec<[f64; 4]>) {
    let (x0, y0, x1, y1) = crate::draw::cmds_box(cmds);
    let (x0, y0, x1, y1) = (x0 / sp, y0 / sp, x1 / sp, y1 / sp);
    if x1 - x0 <= 2.0 * COLUMN {
        out.push([x0, y0, x1, y1]);
        return;
    }
    let mut points: Vec<(f64, f64)> = Vec::with_capacity(cmds.len() / 2);
    let mut starts: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < cmds.len() {
        let c = cmds[i] as i64;
        let args = crate::text::args_of(cmds[i]);
        if c == 0 {
            starts.push(points.len());
        }
        let mut k = 0;
        while k + 1 < args {
            let (Some(&x), Some(&y)) = (cmds.get(i + 1 + k), cmds.get(i + 2 + k)) else { break };
            points.push((x / sp, y / sp));
            k += 2;
        }
        i += 1 + args;
    }
    // The control polygon: a curve lies inside it.
    starts.push(points.len());
    for w in starts.windows(2) {
        let sub = &points[w[0]..w[1]];
        for k in 0..sub.len() {
            let (a, b) = (sub[k], sub[(k + 1) % sub.len()]);
            let pieces = ((a.0 - b.0).abs() / COLUMN).ceil().max(1.0) as usize;
            for p in 0..pieces {
                let (t0, t1) = (p as f64 / pieces as f64, (p + 1) as f64 / pieces as f64);
                let (xa, ya) = (a.0 + (b.0 - a.0) * t0, a.1 + (b.1 - a.1) * t0);
                let (xb, yb) = (a.0 + (b.0 - a.0) * t1, a.1 + (b.1 - a.1) * t1);
                out.push([xa.min(xb), ya.min(yb), xa.max(xb), ya.max(yb)]);
            }
        }
    }
}
