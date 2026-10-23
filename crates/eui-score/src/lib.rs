//! A score engraver: a subset of LilyPond's language laid out as LilyPond
//! lays it out, and drawn as canvas paths (03 §1.1, kinds 1 and 5); the
//! same source played as a Standard MIDI File and printed as a PDF.
//!
//! This is the catalogue's `score()` engraver (`eui_builders_score.sl`),
//! ported function for function so that lang can call it natively: same
//! fonts, same rules, same numbers — `tests/golden.rs` holds the two to the
//! hundredth of a pixel on every corpus piece, and to the byte on the MIDI
//! and the PDF. A source the engraver cannot set is refused with the
//! character it stopped at, never half drawn.
//!
//! Every length inside is in staff spaces, y down, a staff's top line at 0
//! and its bottom one at 4; the paths are in pixels at `sp` a space.

// The engraver is arithmetic on the structures it has just built — bars it
// cut, items it placed — indexed by positions it took from them, with
// lengths in ticks that a parsed source bounds far below overflow. The
// workspace's decode-path lints are for bytes off the wire; a source is
// checked by the parser, and what follows it is guarded at the door
// instead: every public function runs under `catch_unwind`, so a fault is a
// refusal, not a crashed worker.
#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]

use std::panic::{catch_unwind, AssertUnwindSafe};

pub mod draw;
pub mod edit;
pub mod engrave;
pub mod glyphs;
pub mod marks;
pub mod measure;
pub mod midi;
pub mod parse;
mod pdf;
pub mod text;

pub use draw::{Band, HeadRanges, Path, Ranges};
pub use engrave::Engraved;
pub use midi::MidiOptions;

/// The ticks in a whole note. 256 × 315: a 1/128 with a dot is a whole
/// number of ticks, and so is any of them in a triplet, a quintuplet, a
/// sextuplet or a septuplet — or in a triplet inside a triplet. A quarter is
/// 20160, under the 32768 a MIDI file's division allows.
pub const WHOLE: i64 = 80640;

/// The thicknesses, LilyPond's defaults at its staff size 20, in staff
/// spaces.
#[derive(Debug, Clone, Copy)]
pub struct Rule {
    /// A staff line.
    pub staff: f64,
    /// A stem.
    pub stem: f64,
    /// A ledger line.
    pub ledger: f64,
    /// How far a ledger line reaches past its head, in heads.
    pub ledger_out: f64,
    /// A thin bar line.
    pub thin: f64,
    /// A thick one.
    pub thick: f64,
    /// The space between a bar line's parts.
    pub kern: f64,
    /// A beam.
    pub beam: f64,
    /// From one beam to the next.
    pub beam_step: f64,
    /// A tie's or a slur's ends.
    pub curve_tip: f64,
    /// … and its middle.
    pub curve_mid: f64,
}

/// LilyPond's.
pub const RULE: Rule = Rule { staff: 0.1, stem: 0.13, ledger: 0.2, ledger_out: 0.25, thin: 0.19, thick: 0.6, kern: 0.3, beam: 0.48, beam_step: 0.75, curve_tip: 0.08, curve_mid: 0.2 };

/// A refusal: what the engraver could not set, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoreError {
    /// "score: … (at character N)".
    pub message: String,
}

impl std::fmt::Display for ScoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ScoreError {}

/// How a score is laid out: on one endless page (a screen), or on paper.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Layout {
    /// The paper's height; none for a screen.
    pub page_h: Option<f64>,
    /// The top margin.
    pub top: Option<f64>,
    /// The bottom margin.
    pub bottom: Option<f64>,
    /// The left margin; none on a screen, where a line after the first
    /// makes room for its bar number.
    pub left: Option<f64>,
    /// The first system's indent.
    pub indent: Option<f64>,
    /// From one system's top line to the next's, at least.
    pub gap: Option<f64>,
    /// The lines spread evenly over the pages, the last one too; else each
    /// page is filled in turn.
    pub even_pages: bool,
}

/// A sheet of paper, in staff spaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paper {
    /// Its width.
    pub w: f64,
    /// Its height.
    pub h: f64,
    /// The margins.
    pub left: f64,
    /// …
    pub right: f64,
    /// …
    pub top: f64,
    /// …
    pub bottom: f64,
    /// The first system's indent.
    pub indent: f64,
    /// Points a staff space: 5 at LilyPond's default staff size, 20 pt.
    pub pt: f64,
    /// The lines spread evenly over the pages (`page-breaking =
    /// #ly:optimal-breaking`), rather than each page filled in turn.
    pub even_pages: bool,
}

impl Paper {
    /// A4 at LilyPond's default staff size, 20 pt — a staff space of 5 pt —
    /// and its default margins: 10 mm at the sides, 5 at the top, the first
    /// system indented 15. The bottom one is 12, not LilyPond's 6: the
    /// PDF's footer sits on the 6 mm line, and the music stops above it.
    pub fn a4() -> Self {
        let space = 25.4 * 5.0 / 72.0;
        Paper { w: 210.0 / space, h: 297.0 / space, left: 10.0 / space, right: 10.0 / space, top: 5.0 / space, bottom: 12.0 / space, indent: 15.0 / space, pt: 5.0, even_pages: false }
    }

    /// The page a source asks for (`\paper`, `set-global-staff-size`), A4 at
    /// 20 pt where it says nothing. The bottom margin keeps room for the
    /// footer, 6 mm up: never under 12 mm.
    pub fn of(spec: &parse::PaperSpec) -> Self {
        let pt = spec.staff_size.unwrap_or(20.0) / 4.0;
        let space = 25.4 * pt / 72.0;
        let name = spec.size.as_deref().unwrap_or("a4");
        let landscape = name.ends_with("landscape");
        let (w, h) = match name.trim_end_matches("landscape") {
            "a3" => (297.0, 420.0),
            "a5" => (148.0, 210.0),
            "a6" => (105.0, 148.0),
            "b4" => (250.0, 353.0),
            "b5" => (176.0, 250.0),
            "letter" => (215.9, 279.4),
            "legal" => (215.9, 355.6),
            "tabloid" | "11x17" => (279.4, 431.8),
            _ => (210.0, 297.0),
        };
        let (w, h) = if landscape { (h, w) } else { (w, h) };
        Paper {
            w: w / space,
            h: h / space,
            left: spec.left.unwrap_or(10.0) / space,
            right: spec.right.unwrap_or(10.0) / space,
            top: spec.top.unwrap_or(5.0) / space,
            bottom: spec.bottom.unwrap_or(12.0).max(12.0) / space,
            indent: spec.indent.unwrap_or(15.0) / space,
            pt,
            even_pages: spec.even_pages.unwrap_or(false),
        }
    }

    /// The footer's baseline, up from the paper's bottom edge.
    pub fn foot(&self) -> f64 {
        6.0 / (25.4 * self.pt / 72.0)
    }

    /// The line width.
    pub fn line_w(&self) -> f64 {
        self.w - self.left - self.right
    }

    /// The layout that sets music on it.
    pub fn layout(&self) -> Layout {
        Layout { page_h: Some(self.h), top: Some(self.top), bottom: Some(self.bottom), left: Some(self.left), indent: Some(self.indent), gap: Some(12.0), even_pages: self.even_pages }
    }
}

fn guarded<T>(f: impl FnOnce() -> Result<T, ScoreError>) -> Result<T, ScoreError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => Err(ScoreError { message: "score: the engraver failed on this source (an internal fault, not the source's)".into() }),
    }
}

/// The protocol a client needs to engrave `src` (03 §1.4): 9 for the
/// subset the Soli engraver reads, 11 for one using what protocol 11 adds.
pub fn protocol(src: &str) -> Result<u32, ScoreError> {
    guarded(|| parse::protocol(src))
}

/// Parse, lay out and place everything, `width` staff spaces wide.
pub fn engrave(src: &str, width: f64, layout: &Layout) -> Result<Engraved, ScoreError> {
    guarded(|| {
        let parsed = parse::parse(src)?;
        engrave::engrave_parsed(&parsed, width, layout)
    })
}

/// The music on A4.
pub fn engrave_a4(src: &str) -> Result<Engraved, ScoreError> {
    let paper = Paper::a4();
    engrave(src, paper.line_w(), &paper.layout())
}

/// The paths of page `page` at `sp` px a staff space, and where each item's
/// are among them.
pub fn paths(eng: &Engraved, sp: f64, page: usize) -> Result<(Vec<Path>, Ranges), ScoreError> {
    guarded(|| {
        let mut ranges = Vec::new();
        let paths = draw::page_paths(eng, sp, page, &mut ranges);
        Ok((paths, ranges))
    })
}

/// [`paths`], and where each note head was drawn: what a client editing a
/// chord colours to show which of its notes the arrows move (03 §1.4).
pub fn paths_heads(eng: &Engraved, sp: f64, page: usize) -> Result<(Vec<Path>, Ranges, HeadRanges), ScoreError> {
    guarded(|| {
        let mut ranges = Vec::new();
        let mut heads = Vec::new();
        let paths = draw::page_paths_heads(eng, sp, page, &mut ranges, Some(&mut heads));
        Ok((paths, ranges, heads))
    })
}

/// Page `page` as bands, one canvas each, `total` px tall in all.
pub fn bands(eng: &Engraved, sp: f64, page: usize, total: i64) -> Result<Vec<Band>, ScoreError> {
    guarded(|| Ok(draw::bands(eng, sp, page, total)))
}

/// The source as a Standard MIDI File.
pub fn midi(src: &str, opts: &MidiOptions) -> Result<Vec<u8>, ScoreError> {
    guarded(|| midi::midi(src, opts))
}

/// The music on the paper its source asks for (`\paper`), A4 otherwise.
pub fn engrave_paper(src: &str) -> Result<(Engraved, Paper), ScoreError> {
    guarded(|| {
        let parsed = parse::parse(src)?;
        let paper = Paper::of(&parsed.paper);
        Ok((engrave::engrave_parsed(&parsed, paper.line_w(), &paper.layout())?, paper))
    })
}

/// The source as a PDF, on the paper it asks for (A4 otherwise), each page's
/// footer its title and number; `title` for one its header does not give.
pub fn pdf(src: &str, title: &str) -> Result<String, ScoreError> {
    guarded(|| {
        let parsed = parse::parse(src)?;
        let paper = Paper::of(&parsed.paper);
        let eng = engrave::engrave_parsed(&parsed, paper.line_w(), &paper.layout())?;
        let title = parsed.header.get("title").map_or(title, String::as_str);
        Ok(pdf::pdf(&eng, title, &paper))
    })
}
