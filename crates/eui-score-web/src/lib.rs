//! Music engraved in a web page, read only.
//!
//! The engraver the EUI client and the catalogue's `score()` share
//! (`eui-score`), compiled to WebAssembly and drawing SVG: a page shows a
//! LilyPond source with no session, no server and no GPU. What it draws is
//! what a `score` node draws for the same source, width, staff space and page
//! (spec 03 §1.4) — the same rectangles and fills, written as `<rect>` and
//! `<path>` in `currentColor`, so the page's CSS chooses the ink.
//!
//! [`render`] is the whole of it and runs natively too, which is where it is
//! tested; the `wasm32` build adds the binding `assets/web/eui-score.js`
//! calls, and that file defines the `<eui-score>` element.

use std::fmt::Write as _;

use eui_score::{Engraved, Layout, Paper, Path};

/// The largest source this engraves, in bytes: a client's limit (03 §1.4).
pub const MAX_SRC: usize = 512 * 1024;

/// How to lay the music out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    /// The width to lay it out in, px.
    pub width: f64,
    /// The distance between two staff lines, px; `8` when not positive. On A4
    /// the paper decides it.
    pub staff_space: f64,
    /// On A4 sheets, scaled to `width`, rather than to the width on a screen.
    pub a4: bool,
    /// On a screen, the height a page may take before the next system starts
    /// a new one, px; `0` for one page as tall as the music.
    pub page_height: f64,
    /// The page drawn, from 0; past the last draws the last.
    pub page: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { width: 600.0, staff_space: 8.0, a4: false, page_height: 0.0, page: 0 }
    }
}

/// A page of music as SVG, or why there is none.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    /// The page, `width` × `height` px; empty for a refusal.
    pub svg: String,
    /// Its height, px.
    pub height: f64,
    /// How many pages the music takes, at least one.
    pub pages: usize,
    /// The page drawn, after clamping.
    pub page: usize,
    /// The engraver's reason for a source it cannot set, with the position
    /// it stopped at; `""` when it set it.
    pub refusal: String,
}

impl Drawn {
    fn refused(why: String) -> Self {
        Self { svg: String::new(), height: 0.0, pages: 1, page: 0, refusal: why }
    }
}

/// Engrave `src` and draw one page of it.
pub fn render(src: &str, o: &Options) -> Drawn {
    if src.len() > MAX_SRC {
        return Drawn::refused(format!("score: the source is {} bytes, past the 512 KiB this engraves", src.len()));
    }
    if !(o.width.is_finite() && o.width > 0.0) {
        return Drawn::refused("score: no width to lay the music out in".into());
    }
    let paper = Paper::a4();
    let sp = if o.a4 {
        o.width / paper.w
    } else if o.staff_space.is_finite() && o.staff_space > 0.0 {
        o.staff_space
    } else {
        8.0
    };
    let paged = !o.a4 && o.page_height.is_finite() && o.page_height > 0.0;
    let engraved = if o.a4 {
        eui_score::engrave_a4(src)
    } else {
        let layout = Layout { page_h: paged.then(|| o.page_height / sp), ..Layout::default() };
        eui_score::engrave(src, o.width / sp, &layout)
    };
    let eng = match engraved {
        Ok(e) => e,
        Err(e) => return Drawn::refused(e.message),
    };
    let pages = eng.pages.len().max(1);
    let page = o.page.min(pages.saturating_sub(1));
    let height = page_height(&eng, &paper, sp, o.a4, paged, page);
    let paths = match eui_score::paths(&eng, sp, page) {
        Ok((paths, _)) => paths,
        Err(e) => return Drawn::refused(e.message),
    };
    Drawn { svg: svg(&paths, o.width, height), height, pages, page, refusal: String::new() }
}

/// The page's height, px, as a client sizes a `score` (03 §1.4): an A4 sheet;
/// on a paged screen, down to three spaces under the page's lowest system;
/// otherwise the whole engraving.
fn page_height(eng: &Engraved, paper: &Paper, sp: f64, a4: bool, paged: bool, page: usize) -> f64 {
    if a4 {
        return (paper.h * sp).floor();
    }
    if paged {
        let bottom = eng.systems.iter().filter(|s| s.page == page).map(|s| s.bottom).fold(0.0_f64, f64::max);
        return ((bottom + 3.0) * sp).floor();
    }
    (eng.height * sp).ceil()
}

/// The paths as one SVG document in `currentColor`. A fill is its own
/// `<path>`: two glyphs' contours in one would wind against each other where
/// they overlap and cut a hole the engraver never drew.
fn svg(paths: &[Path], width: f64, height: f64) -> String {
    let mut out = String::with_capacity(paths.len().saturating_mul(96).saturating_add(160));
    let (w, h) = (num(width), num(height));
    let _ = write!(out, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" fill="currentColor" role="img">"#);
    for p in paths {
        match p {
            Path::Rect([x, y, rw, rh]) => {
                let _ = write!(out, r#"<rect x="{}" y="{}" width="{}" height="{}"/>"#, num(*x), num(*y), num(*rw), num(*rh));
            }
            Path::Fill(cmds) => {
                out.push_str(r#"<path d=""#);
                fill_d(&mut out, cmds);
                out.push_str(r#""/>"#);
            }
        }
    }
    out.push_str("</svg>");
    out
}

/// A fill's commands as path data: `0` move, `1` line, `2` quadratic and `3`
/// cubic curves with their points, `4` close (03 §1.1, kind 5).
fn fill_d(out: &mut String, cmds: &[f64]) {
    let mut k = 0;
    while let Some(c) = cmds.get(k) {
        let (letter, args) = match *c as i64 {
            0 => ('M', 2),
            1 => ('L', 2),
            2 => ('Q', 4),
            3 => ('C', 6),
            _ => ('Z', 0),
        };
        out.push(letter);
        for (n, v) in cmds.iter().skip(k.saturating_add(1)).take(args).enumerate() {
            if n > 0 {
                out.push(' ');
            }
            out.push_str(&num(*v));
        }
        k = k.saturating_add(args).saturating_add(1);
    }
}

/// A coordinate to a hundredth of a pixel, without the zeros: the drawing is
/// held to that by the engraver's golden tests, and the page is smaller.
fn num(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.to_owned()
    }
}

/// The binding a page calls, through `eui_score_web.js`.
#[cfg(target_arch = "wasm32")]
pub mod web {
    use wasm_bindgen::prelude::wasm_bindgen;

    /// One page of music, for JavaScript.
    #[wasm_bindgen]
    #[derive(Debug)]
    pub struct Page {
        drawn: super::Drawn,
    }

    #[wasm_bindgen]
    impl Page {
        /// The page as SVG; empty for a refusal.
        #[wasm_bindgen(getter)]
        pub fn svg(&self) -> String {
            self.drawn.svg.clone()
        }
        /// Its height, px.
        #[wasm_bindgen(getter)]
        pub fn height(&self) -> f64 {
            self.drawn.height
        }
        /// How many pages the music takes.
        #[wasm_bindgen(getter)]
        pub fn pages(&self) -> u32 {
            u32::try_from(self.drawn.pages).unwrap_or(u32::MAX)
        }
        /// The page drawn.
        #[wasm_bindgen(getter)]
        pub fn page(&self) -> u32 {
            u32::try_from(self.drawn.page).unwrap_or(u32::MAX)
        }
        /// Why the source could not be set, or `""`.
        #[wasm_bindgen(getter)]
        pub fn refusal(&self) -> String {
            self.drawn.refusal.clone()
        }
    }

    /// Engrave `src` at `width` px and draw page `page` (see `Options`).
    #[wasm_bindgen]
    pub fn engrave(src: &str, width: f64, staff_space: f64, a4: bool, page_height: f64, page: u32) -> Page {
        let o = super::Options { width, staff_space, a4, page_height, page: page as usize };
        Page { drawn: super::render(src, &o) }
    }
}
