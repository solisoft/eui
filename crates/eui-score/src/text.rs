//! The words a score prints, in TeX Gyre Schola's outlines
//! (`eui_builders_score.sl`, "Text").

use std::collections::HashMap;

use crate::glyphs::{letters, Weight};

/// The em, in staff spaces, of LilyPond's `font-size` step `fs`: 11 pt on a
/// 20 pt staff is 2.2 spaces, and each step is the sixth root of two.
pub fn em(fs: f64) -> f64 {
    2.2 * (fs * 0.1155245300933242).exp()
}

/// How wide `words` are; a letter the face has not got is a quarter em.
pub fn text_width(words: &str, weight: Weight, em: f64) -> f64 {
    let face = letters().face(weight);
    let mut total = 0.0;
    for ch in words.chars() {
        total += face.get(&ch).map_or(250.0, |l| l.a);
    }
    total * em / 1000.0
}

/// How far words rise above their baseline (negative) and fall below it.
pub fn text_box(words: &str, weight: Weight, em: f64) -> (f64, f64) {
    let face = letters().face(weight);
    let mut rise: f64 = 0.0;
    let mut fall: f64 = 0.0;
    for ch in words.chars() {
        let Some(l) = face.get(&ch) else { continue };
        let cmds = &l.f;
        let mut ci = 0;
        while ci < cmds.len() {
            let args = args_of(cmds[ci]);
            let mut jj = 1;
            while jj < args {
                if let Some(v) = cmds.get(ci + 1 + jj) {
                    rise = rise.min(*v);
                    fall = fall.max(*v);
                }
                jj += 2;
            }
            ci += 1 + args;
        }
    }
    (rise * em / 1000.0, fall * em / 1000.0)
}

/// The number of coordinates a fill command takes.
pub(crate) fn args_of(c: f64) -> usize {
    match c as i64 {
        0 | 1 => 2,
        2 => 4,
        3 => 6,
        _ => 0,
    }
}

/// One cell of a title line.
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Its words.
    pub text: String,
    /// Its weight.
    pub weight: Weight,
    /// Its em.
    pub em: f64,
    /// Where it starts.
    pub x: f64,
}

/// One title line: its baseline and its cells.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadLine {
    /// The baseline.
    pub y: f64,
    /// Left, centre and right.
    pub cells: Vec<Cell>,
}

/// The title block, and where it ends.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HeadBlock {
    /// Its lines.
    pub lines: Vec<HeadLine>,
    /// Its bottom.
    pub bottom: f64,
}

/// Room above the titles and below them, in staff spaces: the title is not
/// set flush against the top margin, nor the first system against it.
pub const TITLE_PAD: f64 = 2.0;

/// LilyPond's `bookTitleMarkup`, line by line, `TITLE_PAD` clear above and
/// below.
pub fn header_lines(header: &HashMap<String, String>, width: f64) -> HeadBlock {
    let get = |k: &str| header.get(k).cloned().unwrap_or_default();
    let rows: Vec<[(String, Weight, f64); 3]> = vec![
        [(String::new(), Weight::Regular, 0.0), (get("dedication"), Weight::Regular, 0.0), (String::new(), Weight::Regular, 0.0)],
        [(String::new(), Weight::Bold, 4.0), (get("title"), Weight::Bold, 4.0), (String::new(), Weight::Bold, 4.0)],
        [(String::new(), Weight::Bold, 1.0), (get("subtitle"), Weight::Bold, 1.0), (String::new(), Weight::Bold, 1.0)],
        [(String::new(), Weight::Bold, -1.0), (get("subsubtitle"), Weight::Bold, -1.0), (String::new(), Weight::Bold, -1.0)],
        [(get("poet"), Weight::Regular, 0.0), (get("instrument"), Weight::Bold, 1.0), (get("composer"), Weight::Regular, 0.0)],
        [(get("meter"), Weight::Regular, 0.0), (String::new(), Weight::Regular, 0.0), (get("arranger"), Weight::Regular, 0.0)],
    ];
    let mut out: Vec<HeadLine> = Vec::new();
    let mut y = 0.0;
    let mut prev_descent = 0.0;
    for row in rows {
        let mut tallest: f64 = 0.0;
        for cell in &row {
            if !cell.0.is_empty() {
                tallest = tallest.max(em(cell.2));
            }
        }
        if tallest == 0.0 {
            continue;
        }
        let ascent = 0.72 * tallest;
        y = if out.is_empty() { TITLE_PAD + ascent } else { (y + 3.5_f64).max(y + prev_descent + ascent + 0.3) };
        let mut cells = Vec::new();
        for (ci, cell) in row.iter().enumerate() {
            if cell.0.is_empty() {
                continue;
            }
            let e = em(cell.2);
            let w = text_width(&cell.0, cell.1, e);
            let mut x = 0.0;
            if ci == 1 {
                x = (width - w) / 2.0;
            }
            if ci == 2 {
                x = width - w;
            }
            cells.push(Cell { text: cell.0.clone(), weight: cell.1, em: e, x });
        }
        out.push(HeadLine { y, cells });
        prev_descent = 0.25 * tallest;
    }
    let bottom = if out.is_empty() { 0.0 } else { y + prev_descent + TITLE_PAD };
    HeadBlock { lines: out, bottom }
}
