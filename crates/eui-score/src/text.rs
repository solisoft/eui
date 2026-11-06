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
/// below. A centred field wider than `width` (a long title on a phone's
/// page) breaks between words onto further lines, and a word or a side field
/// that still does not fit is set smaller, so nothing starts left of the page
/// or ends right of it.
pub fn header_lines(header: &HashMap<String, String>, width: f64) -> HeadBlock {
    let get = |k: &str| header.get(k).cloned().unwrap_or_default();
    let none = |fs: f64| (String::new(), Weight::Regular, em(fs));
    let field = |k: &str, weight: Weight, fs: f64| (get(k), weight, em(fs));
    let rows: Vec<[(String, Weight, f64); 3]> = vec![
        [none(0.0), field("dedication", Weight::Regular, 0.0), none(0.0)],
        [none(4.0), field("title", Weight::Bold, 4.0), none(4.0)],
        [none(1.0), field("subtitle", Weight::Bold, 1.0), none(1.0)],
        [none(-1.0), field("subsubtitle", Weight::Bold, -1.0), none(-1.0)],
        [field("poet", Weight::Regular, 0.0), field("instrument", Weight::Bold, 1.0), field("composer", Weight::Regular, 0.0)],
        [field("meter", Weight::Regular, 0.0), none(0.0), field("arranger", Weight::Regular, 0.0)],
    ];
    let mut out: Vec<HeadLine> = Vec::new();
    let mut y = 0.0;
    let mut prev_descent = 0.0;
    for row in rows {
        let [left, centre, right] = row;
        let (left, right) = (fit_one(left, width), fit_one(right, width));
        let mut broken = fit_lines(&centre.0, centre.1, centre.2, width).into_iter();
        let first = broken.next().unwrap_or((String::new(), centre.2));
        let first = (first.0, centre.1, first.1);
        let mut lines = Vec::new();
        if !crowded(&left, &first, &right, width) {
            lines.push([left, first, right]);
            lines.extend(broken.map(|(words, e)| [none(0.0), (words, centre.1, e), none(0.0)]));
        } else {
            // Side by side they would touch (a long instrument and composer on
            // a phone's page): the centre on its own lines, the sides below it,
            // each on a line of its own if they still meet.
            lines.push([none(0.0), first, none(0.0)]);
            lines.extend(broken.map(|(words, e)| [none(0.0), (words, centre.1, e), none(0.0)]));
            let blank = none(0.0);
            if crowded(&left, &blank, &right, width) {
                lines.push([left, none(0.0), none(0.0)]);
                lines.push([none(0.0), none(0.0), right]);
            } else {
                lines.push([left, none(0.0), right]);
            }
        }
        for line in lines {
            let mut tallest: f64 = 0.0;
            for cell in &line {
                if !cell.0.is_empty() {
                    tallest = tallest.max(cell.2);
                }
            }
            if tallest == 0.0 {
                continue;
            }
            let ascent = 0.72 * tallest;
            y = if out.is_empty() { TITLE_PAD + ascent } else { (y + 3.5_f64).max(y + prev_descent + ascent + 0.3) };
            let mut cells = Vec::new();
            for (ci, cell) in line.into_iter().enumerate() {
                if cell.0.is_empty() {
                    continue;
                }
                let w = text_width(&cell.0, cell.1, cell.2);
                let x = match ci {
                    1 => (width - w) / 2.0,
                    2 => width - w,
                    _ => 0.0,
                };
                cells.push(Cell { text: cell.0, weight: cell.1, em: cell.2, x });
            }
            out.push(HeadLine { y, cells });
            prev_descent = 0.25 * tallest;
        }
    }
    let bottom = if out.is_empty() { 0.0 } else { y + prev_descent + TITLE_PAD };
    HeadBlock { lines: out, bottom }
}

/// Room kept between two fields of one title line, in staff spaces.
pub const FIELD_GAP: f64 = 1.0;

/// Whether `left`, `centre` and `right`, set on one line `width` wide (the
/// left flush left, the centre centred, the right flush right), come closer
/// than `FIELD_GAP` to one another.
fn crowded(left: &(String, Weight, f64), centre: &(String, Weight, f64), right: &(String, Weight, f64), width: f64) -> bool {
    let wide = |cell: &(String, Weight, f64)| if cell.0.is_empty() { None } else { Some(text_width(&cell.0, cell.1, cell.2)) };
    let (l, c, r) = (wide(left), wide(centre), wide(right));
    match c {
        Some(c) => {
            let from = (width - c) / 2.0;
            l.is_some_and(|l| l + FIELD_GAP > from) || r.is_some_and(|r| from + c + FIELD_GAP > width - r)
        }
        None => matches!((l, r), (Some(l), Some(r)) if l + FIELD_GAP > width - r),
    }
}

/// A side field as it fits in `width`: smaller if it is wider.
fn fit_one(cell: (String, Weight, f64), width: f64) -> (String, Weight, f64) {
    let e = shrunk(&cell.0, cell.1, cell.2, width);
    (cell.0, cell.1, e)
}

/// `words` at `em`, or the em that makes them exactly `width` wide if they are
/// wider.
fn shrunk(words: &str, weight: Weight, em: f64, width: f64) -> f64 {
    let w = text_width(words, weight, em);
    if w > width && width > 0.0 {
        em * width / w
    } else {
        em
    }
}

/// `words` broken between words into lines no wider than `width`, each with
/// its em: the em given, or smaller for a single word wider than the page.
pub fn fit_lines(words: &str, weight: Weight, em: f64, width: f64) -> Vec<(String, f64)> {
    if words.is_empty() || text_width(words, weight, em) <= width {
        return if words.is_empty() { Vec::new() } else { vec![(words.to_string(), em)] };
    }
    let mut lines: Vec<String> = Vec::new();
    for word in words.split_whitespace() {
        match lines.last_mut() {
            Some(line) if text_width(&format!("{line} {word}"), weight, em) <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    if lines.is_empty() {
        return Vec::new();
    }
    lines
        .into_iter()
        .map(|line| {
            let e = shrunk(&line, weight, em, width);
            (line, e)
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn titled(title: &str, composer: &str) -> HashMap<String, String> {
        HashMap::from([("title".to_string(), title.to_string()), ("composer".to_string(), composer.to_string())])
    }

    fn inside(head: &HeadBlock, width: f64) {
        for line in &head.lines {
            for cell in &line.cells {
                let w = text_width(&cell.text, cell.weight, cell.em);
                assert!(cell.x >= -1e-9 && cell.x + w <= width + 1e-9, "{:?} runs off a page {width} wide", cell.text);
            }
        }
    }

    #[test]
    fn a_title_that_fits_stays_one_line_at_its_size() {
        let head = header_lines(&titled("Sonata", ""), 120.0);
        assert_eq!(head.lines.len(), 1);
        assert_eq!(head.lines[0].cells[0].text, "Sonata");
        assert_eq!(head.lines[0].cells[0].em, em(4.0));
    }

    #[test]
    fn a_long_title_on_a_narrow_page_breaks_between_words_and_stays_on_it() {
        let title = "Quartet in G minor, first movement";
        let wide = header_lines(&titled(title, ""), 120.0);
        let narrow = header_lines(&titled(title, ""), 40.0);
        assert_eq!(wide.lines.len(), 1);
        assert!(narrow.lines.len() > 1);
        inside(&narrow, 40.0);
        let words: Vec<String> = narrow.lines.iter().map(|l| l.cells[0].text.clone()).collect();
        assert_eq!(words.join(" "), title);
        assert!(narrow.lines.iter().all(|l| l.cells[0].em == em(4.0)));
        assert!(narrow.lines.windows(2).all(|p| p[1].y > p[0].y));
        assert!(narrow.bottom > wide.bottom);
    }

    #[test]
    fn a_single_word_wider_than_the_page_is_set_smaller_to_fit() {
        let head = header_lines(&titled("Streichquartettsatzfragment", ""), 30.0);
        assert_eq!(head.lines.len(), 1);
        let cell = &head.lines[0].cells[0];
        assert!(cell.em < em(4.0));
        assert!((text_width(&cell.text, cell.weight, cell.em) - 30.0).abs() < 1e-6);
        inside(&head, 30.0);
    }

    #[test]
    fn the_composer_stays_on_the_title_rows_first_line_and_on_the_page() {
        let head = header_lines(&titled("Quartet in G minor, first movement", "Wolfgang Amadeus Mozart, arranged"), 25.0);
        inside(&head, 25.0);
        let composers: Vec<&HeadLine> = head.lines.iter().filter(|l| l.cells.iter().any(|c| c.text.starts_with("Wolfgang"))).collect();
        assert_eq!(composers.len(), 1);
        let composer = composers[0];
        assert!(composer.cells.len() == 1 && composer.cells[0].em < em(0.0));
    }

    /// No two cells of one line closer than `FIELD_GAP`.
    fn apart(head: &HeadBlock) {
        for line in &head.lines {
            let mut spans: Vec<(f64, f64, &str)> = line.cells.iter().map(|c| (c.x, c.x + text_width(&c.text, c.weight, c.em), c.text.as_str())).collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            for pair in spans.windows(2) {
                assert!(pair[0].1 + FIELD_GAP <= pair[1].0 + 1e-9, "{:?} and {:?} touch", pair[0].2, pair[1].2);
            }
        }
    }

    fn line_of<'a>(head: &'a HeadBlock, words: &str) -> &'a HeadLine {
        head.lines.iter().find(|l| l.cells.iter().any(|c| c.text == words)).expect(words)
    }

    #[test]
    fn an_instrument_and_a_composer_that_would_touch_take_a_line_each() {
        // Tonnerre et Éclairs on a phone (score #511): "Saxo Ténor Sib" ran into "Johann Strauss".
        let mut header = titled("Tonnerre et Éclairs", "Johann Strauss");
        header.insert("instrument".to_string(), "Saxo Ténor Sib".to_string());
        let narrow = header_lines(&header, 50.0);
        inside(&narrow, 50.0);
        apart(&narrow);
        let (instrument, composer) = (line_of(&narrow, "Saxo Ténor Sib"), line_of(&narrow, "Johann Strauss"));
        assert!(composer.y > instrument.y, "the composer below the instrument");
        assert_eq!(composer.cells.len(), 1);
        let c = &composer.cells[0];
        assert!((c.x + text_width(&c.text, c.weight, c.em) - 50.0).abs() < 1e-6, "still flush right");
        // On a page wide enough, the same line as ever.
        let wide = header_lines(&header, 120.0);
        apart(&wide);
        assert_eq!(line_of(&wide, "Saxo Ténor Sib").y, line_of(&wide, "Johann Strauss").y);
        assert!(narrow.bottom > wide.bottom);
    }

    #[test]
    fn a_poet_and_a_composer_that_still_meet_go_one_above_the_other() {
        let mut header = titled("", "Johann Sebastian Bach");
        header.insert("poet".to_string(), "Christian Friedrich Henrici".to_string());
        let head = header_lines(&header, 45.0);
        inside(&head, 45.0);
        apart(&head);
        assert!(line_of(&head, "Johann Sebastian Bach").y > line_of(&head, "Christian Friedrich Henrici").y);
    }

    #[test]
    fn no_header_takes_no_room() {
        let head = header_lines(&HashMap::new(), 40.0);
        assert!(head.lines.is_empty());
        assert_eq!(head.bottom, 0.0);
    }
}
