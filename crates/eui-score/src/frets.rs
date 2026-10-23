//! Guitar chord diagrams, as LilyPond draws them: `\fret-diagram` and
//! `\fret-diagram-terse` in markup, and a `FretBoards` line, whose shapes
//! are found here for a guitar in standard tuning when no table gives them.
//!
//! A diagram travels in markup as a run whose text is [`FRET`] then its
//! terse spec — `x;3;2;o;1;o;`, strings 6 to 1, and `|6-1-1` for a barre
//! from string 6 to string 1 at the first fret — so it stands wherever
//! words stand: over a note, in a line of chord names.

use crate::text::em;

/// What starts a run that is a diagram, not words.
pub const FRET: &str = "\u{1}fret:";

/// The open strings of a guitar in standard tuning, low E first, as MIDI keys.
const TUNING: [i64; 6] = [40, 45, 50, 55, 59, 64];

/// A chord's shape: each string's fret, low E first — `None` not played,
/// `Some(0)` open — and a barre `(from, to, fret)` in string numbers (6 to 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagram {
    /// Each string's fret, low E first: none when it is not played, 0 open.
    pub frets: [Option<i64>; 6],
    /// A barre: from string, to string (6 to 1), at its fret.
    pub barre: Option<(i64, i64, i64)>,
}

impl Diagram {
    /// The terse spec this travels as.
    pub fn spec(&self) -> String {
        let mut out: String = self
            .frets
            .iter()
            .map(|f| match f {
                None => "x;".to_string(),
                Some(0) => "o;".to_string(),
                Some(n) => format!("{n};"),
            })
            .collect();
        if let Some((a, b, f)) = self.barre {
            out.push_str(&format!("|{a}-{b}-{f}"));
        }
        out
    }

    /// The fret the drawing starts at: the first, with its nut, when the
    /// shape reaches no further than the fourth; else its lowest fret.
    fn base(&self) -> i64 {
        let fretted: Vec<i64> = self.frets.iter().flatten().copied().filter(|f| *f > 0).collect();
        let high = fretted.iter().copied().max().unwrap_or(0);
        if high <= 4 {
            1
        } else {
            fretted.iter().copied().min().unwrap_or(1)
        }
    }

    fn rows(&self) -> i64 {
        let high = self.frets.iter().flatten().copied().max().unwrap_or(0);
        (high - self.base() + 1).max(4)
    }
}

/// `\fret-diagram "6-x;5-3;4-2;3-o;2-1;1-o;"`: each string, `x` or `o` or its
/// fret (a finger after a second `-` is read and not drawn); `c:6-1-1;` a
/// barre. A string it does not name is not played.
pub fn verbose(spec: &str) -> Result<Diagram, String> {
    let mut d = Diagram { frets: [None; 6], barre: None };
    for item in spec.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some(barre) = item.strip_prefix("c:") {
            let n: Vec<i64> = barre.split('-').map(|x| x.trim().parse::<i64>()).collect::<Result<_, _>>().map_err(|_| format!("'{item}' is not a barre (c:6-1-1)"))?;
            let [a, b, f] = n[..] else { return Err(format!("'{item}' is not a barre (c:6-1-1)")) };
            if !(1..=6).contains(&a) || !(1..=6).contains(&b) || !(1..=24).contains(&f) {
                return Err(format!("'{item}' is not a barre this draws"));
            }
            d.barre = Some((a.max(b), a.min(b), f));
            continue;
        }
        let mut parts = item.split('-');
        let string: i64 = parts.next().and_then(|s| s.trim().parse().ok()).filter(|s| (1..=6).contains(s)).ok_or_else(|| format!("'{item}' names no string 1 to 6"))?;
        let what = parts.next().map(str::trim).ok_or_else(|| format!("'{item}' says nothing of its string"))?;
        d.frets[(6 - string) as usize] = fret_of(what).map_err(|_| format!("'{item}': x, o or a fret"))?;
    }
    barred(&mut d);
    Ok(d)
}

/// `\fret-diagram-terse "x;3;2;o;1;o;"`: the six strings, low E first.
pub fn terse(spec: &str) -> Result<Diagram, String> {
    let items: Vec<&str> = spec.split(';').map(str::trim).filter(|s| !s.is_empty()).collect();
    if items.len() != 6 {
        return Err(format!("\"{spec}\" names {} strings; a guitar has 6", items.len()));
    }
    let mut d = Diagram { frets: [None; 6], barre: None };
    for (k, item) in items.iter().enumerate() {
        // `3-2`: a finger after the fret; `1-(` a barre's start, read as its fret.
        let what = item.split('-').next().unwrap_or("");
        d.frets[k] = fret_of(what).map_err(|_| format!("'{item}': x, o or a fret"))?;
    }
    Ok(d)
}

fn fret_of(what: &str) -> Result<Option<i64>, ()> {
    match what {
        "x" => Ok(None),
        "o" | "0" => Ok(Some(0)),
        n => n.parse::<i64>().ok().filter(|f| (1..=24).contains(f)).map(Some).ok_or(()),
    }
}

/// A barre's strings take its fret where nothing else stops them.
fn barred(d: &mut Diagram) {
    if let Some((a, b, f)) = d.barre {
        for s in b..=a {
            let k = (6 - s) as usize;
            if d.frets[k].is_none_or(|x| x < f) {
                d.frets[k] = Some(f);
            }
        }
    }
}

/// A diagram's spec back from a run's text.
pub fn from_spec(text: &str) -> Option<Diagram> {
    let spec = text.strip_prefix(FRET)?;
    let (frets, barre) = spec.split_once('|').unwrap_or((spec, ""));
    let mut d = terse(frets).ok()?;
    if !barre.is_empty() {
        let n: Vec<i64> = barre.split('-').filter_map(|x| x.parse().ok()).collect();
        if let [a, b, f] = n[..] {
            d.barre = Some((a, b, f));
        }
    }
    Some(d)
}

/// A shape for a chord's pitch classes on a guitar, its bass `bass` on the
/// lowest string that sounds: the first position if one holds it, else the
/// lowest position up the neck. All the chord's notes sound (a seventh
/// chord's fifth may go), the hand spans three frets, four fingers or a
/// barre and three; most strings, then the fewest fingers, then the lowest.
pub fn voicing(classes: &[i64], bass: i64) -> Option<Diagram> {
    let classes: Vec<i64> = classes.iter().map(|c| c.rem_euclid(12)).collect();
    let bass = bass.rem_euclid(12);
    let fifth = (bass + 7) % 12;
    for base in 0..=12 {
        let window: Vec<i64> = if base == 0 { (0..=3).collect() } else { (base..base + 4).collect() };
        let choices: Vec<Vec<Option<i64>>> = TUNING
            .iter()
            .map(|open| {
                let mut c: Vec<Option<i64>> = vec![None];
                c.extend(window.iter().filter(|f| classes.contains(&((open + *f) % 12))).map(|f| Some(*f)));
                c
            })
            .collect();
        let mut best: Option<(i64, Diagram)> = None;
        let mut pick = [0usize; 6];
        loop {
            let frets: [Option<i64>; 6] = std::array::from_fn(|k| choices[k][pick[k]]);
            if let Some(d) = playable(&frets, &classes, bass, fifth) {
                let score = rank(&d);
                if best.as_ref().is_none_or(|(s, _)| score < *s) {
                    best = Some((score, d));
                }
            }
            // The next combination, odometer-wise.
            let mut k = 0;
            while k < 6 {
                pick[k] += 1;
                if pick[k] < choices[k].len() {
                    break;
                }
                pick[k] = 0;
                k += 1;
            }
            if k == 6 {
                break;
            }
        }
        if let Some((_, d)) = best {
            return Some(d);
        }
    }
    None
}

fn playable(frets: &[Option<i64>; 6], classes: &[i64], bass: i64, fifth: i64) -> Option<Diagram> {
    let sounding: Vec<usize> = (0..6).filter(|k| frets[*k].is_some()).collect();
    let (&low, &high) = (sounding.first()?, sounding.last()?);
    // At least four strings, together, the bass on the lowest, every note there.
    if sounding.len() < 4 || high - low + 1 != sounding.len() {
        return None;
    }
    let pc = |k: usize| (TUNING[k] + frets[k].unwrap_or(0)) % 12;
    if pc(low) != bass {
        return None;
    }
    let heard: Vec<i64> = sounding.iter().map(|k| pc(*k)).collect();
    let missing: Vec<&i64> = classes.iter().filter(|c| !heard.contains(c)).collect();
    if !(missing.is_empty() || (classes.len() >= 4 && missing == [&fifth])) {
        return None;
    }
    let fretted: Vec<i64> = sounding.iter().filter_map(|k| frets[*k]).filter(|f| *f > 0).collect();
    let (lo, hi) = (fretted.iter().copied().min().unwrap_or(0), fretted.iter().copied().max().unwrap_or(0));
    if hi - lo > 3 {
        return None;
    }
    let mut d = Diagram { frets: *frets, barre: None };
    if fretted.len() <= 4 {
        return Some(d);
    }
    // Five or six fretted: a barre at the lowest fret, across to the top
    // string, nothing open under it, and three fingers left.
    let under: Vec<usize> = sounding.iter().copied().filter(|k| frets[*k] == Some(lo)).collect();
    let from = *under.first()?;
    if !(from..6).all(|k| frets[k].is_some_and(|f| f >= lo)) {
        return None;
    }
    let others = (from..6).filter(|k| frets[*k] != Some(lo)).count() + (low..from).count();
    if others > 3 {
        return None;
    }
    d.barre = Some((6 - from as i64, 1, lo));
    Some(d)
}

fn rank(d: &Diagram) -> i64 {
    let sounding = d.frets.iter().filter(|f| f.is_some()).count() as i64;
    let fretted: Vec<i64> = d.frets.iter().flatten().copied().filter(|f| *f > 0).collect();
    let fingers = if d.barre.is_some() { 1 + fretted.iter().filter(|f| Some(**f) != d.barre.map(|b| b.2)).count() as i64 } else { fretted.len() as i64 };
    -sounding * 100 + fingers * 10 + fretted.iter().sum::<i64>()
}

// ---- Drawing --------------------------------------------------------------------

/// What a diagram is drawn with, in staff spaces from its top left corner.
pub enum Shape {
    /// A rectangle: x, y, width, height.
    Rect(f64, f64, f64, f64),
    /// An outline's fill commands, in place.
    Fill(Vec<f64>),
    /// Words on a baseline: text, x, y, size.
    Text(String, f64, f64, f64),
}

struct Metrics {
    s: f64,
    gap: f64,
    fret: f64,
    top: f64,
    label: f64,
}

fn metrics(d: &Diagram, e: f64) -> Metrics {
    let s = e / em(0.0);
    let label = if d.base() > 1 { crate::text::text_width(&d.base().to_string(), crate::glyphs::Weight::Regular, e * 0.6) + 0.25 * s } else { 0.0 };
    Metrics { s, gap: 0.6 * s, fret: 0.75 * s, top: 0.7 * s, label }
}

/// A diagram's width and its height above the baseline it stands on.
pub fn size(d: &Diagram, e: f64) -> (f64, f64) {
    let m = metrics(d, e);
    (m.label + 5.0 * m.gap + 0.3 * m.s, m.top + d.rows() as f64 * m.fret + 0.1 * m.s)
}

/// A circle as fill commands: four quarters.
fn circle(cx: f64, cy: f64, r: f64, reverse: bool) -> Vec<f64> {
    let k = 0.5523 * r;
    let mut c = vec![0.0, cx + r, cy];
    let quarters =
        [[cx + r, cy + k, cx + k, cy + r, cx, cy + r], [cx - k, cy + r, cx - r, cy + k, cx - r, cy], [cx - r, cy - k, cx - k, cy - r, cx, cy - r], [cx + k, cy - r, cx + r, cy - k, cx + r, cy]];
    if reverse {
        let back =
            [[cx + r, cy - k, cx + k, cy - r, cx, cy - r], [cx - k, cy - r, cx - r, cy - k, cx - r, cy], [cx - r, cy + k, cx - k, cy + r, cx, cy + r], [cx + k, cy + r, cx + r, cy + k, cx + r, cy]];
        for q in back {
            c.push(3.0);
            c.extend(q);
        }
    } else {
        for q in quarters {
            c.push(3.0);
            c.extend(q);
        }
    }
    c.push(4.0);
    c
}

/// The diagram's shapes, its top left corner at `(x, y)`.
pub fn shapes(d: &Diagram, x: f64, y: f64, e: f64) -> Vec<Shape> {
    let m = metrics(d, e);
    let rows = d.rows();
    let base = d.base();
    let left = x + m.label + 0.15 * m.s;
    let nut_y = y + m.top;
    let line = 0.07 * m.s;
    let mut out = Vec::new();
    let width = 5.0 * m.gap;
    let height = rows as f64 * m.fret;
    for k in 0..6 {
        out.push(Shape::Rect(left + k as f64 * m.gap - line / 2.0, nut_y, line, height));
    }
    for r in 0..=rows {
        let thick = if r == 0 && base == 1 { 0.28 * m.s } else { line };
        let ry = nut_y + r as f64 * m.fret - if r == 0 && base == 1 { thick } else { line / 2.0 };
        out.push(Shape::Rect(left - line / 2.0, ry, width + line, thick));
    }
    if base > 1 {
        let ex = e * 0.6;
        // Beside the first fret's row, its baseline a little under the row's middle.
        let digit = crate::text::text_box("7", crate::glyphs::Weight::Regular, ex).0.abs();
        out.push(Shape::Text(base.to_string(), x, nut_y + 0.5 * m.fret + digit / 2.0, ex));
    }
    let dot = 0.24 * m.s;
    for (k, f) in d.frets.iter().enumerate() {
        let cx = left + k as f64 * m.gap;
        match f {
            None => {
                // An x over the nut.
                let (h, t, cy) = (0.18 * m.s, 0.05 * m.s, y + m.top * 0.45);
                out.push(Shape::Fill(vec![0.0, cx - h, cy - h + t, 1.0, cx - h + t, cy - h, 1.0, cx + h, cy + h - t, 1.0, cx + h - t, cy + h, 4.0]));
                out.push(Shape::Fill(vec![0.0, cx + h - t, cy - h, 1.0, cx + h, cy - h + t, 1.0, cx - h + t, cy + h, 1.0, cx - h, cy + h - t, 4.0]));
            }
            Some(0) => {
                // An open ring over the nut.
                let cy = y + m.top * 0.45;
                let mut ring = circle(cx, cy, 0.19 * m.s, false);
                ring.extend(circle(cx, cy, 0.13 * m.s, true));
                out.push(Shape::Fill(ring));
            }
            Some(n) => {
                let row = (n - base) as f64;
                let cy = nut_y + (row + 0.5) * m.fret;
                if d.barre.is_some_and(|(_, _, bf)| bf == *n) && d.barre.is_some_and(|(a, b, _)| (b..=a).contains(&(6 - k as i64))) {
                    continue;
                }
                out.push(Shape::Fill(circle(cx, cy, dot, false)));
            }
        }
    }
    if let Some((a, b, f)) = d.barre {
        let (x0, x1) = (left + (6 - a) as f64 * m.gap, left + (6 - b) as f64 * m.gap);
        let cy = nut_y + ((f - base) as f64 + 0.5) * m.fret;
        out.push(Shape::Rect(x0, cy - dot, x1 - x0, 2.0 * dot));
        out.push(Shape::Fill(circle(x0, cy, dot, false)));
        out.push(Shape::Fill(circle(x1, cy, dot, false)));
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn shape(classes: &[i64]) -> String {
        voicing(classes, classes[0]).map(|d| d.spec()).unwrap_or_default()
    }

    #[test]
    fn the_first_position_shapes_a_guitarist_plays() {
        assert_eq!(shape(&[0, 4, 7]), "x;3;2;o;1;o;", "C");
        assert_eq!(shape(&[7, 11, 2]), "3;2;o;o;o;3;", "G");
        assert_eq!(shape(&[2, 6, 9]), "x;x;o;2;3;2;", "D");
        assert_eq!(shape(&[9, 0, 4]), "x;o;2;2;1;o;", "Am");
        assert_eq!(shape(&[4, 7, 11]), "o;2;2;o;o;o;", "Em");
        assert_eq!(shape(&[5, 9, 0]), "1;3;3;2;1;1;|6-1-1", "F, barred");
        assert_eq!(shape(&[7, 11, 2, 5]), "3;2;o;o;o;1;", "G7");
    }

    #[test]
    fn diagrams_read_as_lilypond_writes_them() {
        assert_eq!(verbose("6-x;5-3;4-2;3-o;2-1;1-o;").unwrap().spec(), "x;3;2;o;1;o;");
        assert_eq!(terse("x;3;2;o;1;o;").unwrap().spec(), "x;3;2;o;1;o;");
        let f = verbose("c:6-1-1;5-3;4-3;3-2;").unwrap();
        assert_eq!(f.spec(), "1;3;3;2;1;1;|6-1-1");
        assert_eq!(from_spec(&format!("{FRET}{}", f.spec())), Some(f));
        assert!(terse("x;3;2;").unwrap_err().contains("a guitar has 6"));
        assert!(verbose("7-1;").unwrap_err().contains("names no string"));
        assert!(verbose("6-y;").unwrap_err().contains("x, o or a fret"));
    }

    #[test]
    fn a_shape_up_the_neck_says_where_it_starts() {
        let d = terse("x;x;7;9;10;9;").unwrap();
        assert_eq!(d.base(), 7);
        assert_eq!(d.rows(), 4);
        let (w, h) = size(&d, em(0.0));
        assert!(w > 3.0 && h > 3.0);
        assert!(shapes(&d, 0.0, 0.0, em(0.0)).iter().any(|s| matches!(s, Shape::Text(t, ..) if t == "7")));
    }
}
