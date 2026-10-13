//! The outlines the engraver draws with, read once from `data/`.
//!
//! `smufl.json` is Emmentaler — LilyPond's music font — keyed by SMuFL's
//! glyph names, in staff spaces, y down; `letters.json` is TeX Gyre Schola,
//! the face LilyPond sets its text in, in thousandths of an em. Both were
//! written by `xtask smufl` and `xtask letters` from the fonts, and are the
//! very numbers the Soli engraver (`eui_builders_score.sl`) reads, so the two
//! draw the same thing to the hundredth of a pixel.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

/// A music glyph: its fill commands (03 §1.1 kind 5, without the kind and
/// the colour), its advance, its box `[x0, y0, x1, y1]` and its stem anchors.
#[derive(Debug, Clone, Default)]
pub struct Glyph {
    /// Canvas fill commands in staff spaces.
    pub fill: Vec<f64>,
    /// How far the next glyph starts.
    pub advance: f64,
    /// `[x0, y0, x1, y1]`, y down.
    pub bbox: [f64; 4],
    /// Named points, `stemUpSE` and its kin.
    pub anchors: HashMap<String, [f64; 2]>,
}

/// A letter: its advance and outline, in thousandths of an em.
#[derive(Debug, Clone, Default)]
pub struct Letter {
    /// The advance.
    pub a: f64,
    /// The fill commands.
    pub f: Vec<f64>,
}

/// The three faces a score prints in.
#[derive(Debug, Default)]
pub struct Letters {
    /// Titles, words, numbers.
    pub regular: HashMap<char, Letter>,
    /// Titles and tempo marks.
    pub bold: HashMap<char, Letter>,
    /// Tuplet numbers.
    pub italic: HashMap<char, Letter>,
}

/// A text weight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// Regular.
    Regular,
    /// Bold.
    Bold,
    /// Italic.
    Italic,
}

impl Letters {
    /// The face of a weight.
    pub fn face(&self, weight: Weight) -> &HashMap<char, Letter> {
        match weight {
            Weight::Regular => &self.regular,
            Weight::Bold => &self.bold,
            Weight::Italic => &self.italic,
        }
    }
}

fn numbers(v: Option<&Value>) -> Vec<f64> {
    v.and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default()
}

fn read_glyphs() -> HashMap<String, Glyph> {
    let data: Value = serde_json::from_str(include_str!("../data/smufl.json")).unwrap_or(Value::Null);
    let mut out = HashMap::new();
    let Some(map) = data.as_object() else { return out };
    for (name, g) in map {
        let bbox = numbers(g.get("bbox"));
        let mut b = [0.0; 4];
        for (slot, v) in b.iter_mut().zip(bbox) {
            *slot = v;
        }
        let mut anchors = HashMap::new();
        if let Some(a) = g.get("anchors").and_then(Value::as_object) {
            for (k, v) in a {
                let p = numbers(Some(v));
                if let [x, y] = p.as_slice() {
                    anchors.insert(k.clone(), [*x, *y]);
                }
            }
        }
        out.insert(name.clone(), Glyph { fill: numbers(g.get("fill")), advance: g.get("advance").and_then(Value::as_f64).unwrap_or(0.0), bbox: b, anchors });
    }
    out
}

fn read_letters() -> Letters {
    let data: Value = serde_json::from_str(include_str!("../data/letters.json")).unwrap_or(Value::Null);
    let face = |name: &str| -> HashMap<char, Letter> {
        let mut out = HashMap::new();
        if let Some(map) = data.get(name).and_then(Value::as_object) {
            for (k, v) in map {
                let Some(ch) = k.chars().next() else { continue };
                out.insert(ch, Letter { a: v.get("a").and_then(Value::as_f64).unwrap_or(0.0), f: numbers(v.get("f")) });
            }
        }
        out
    };
    Letters { regular: face("regular"), bold: face("bold"), italic: face("italic") }
}

/// Every music glyph, by its SMuFL name.
pub fn glyphs() -> &'static HashMap<String, Glyph> {
    static GLYPHS: OnceLock<HashMap<String, Glyph>> = OnceLock::new();
    GLYPHS.get_or_init(read_glyphs)
}

/// One glyph; an empty one for a name the font has not got, which draws
/// nothing rather than stopping the page.
pub fn glyph(name: &str) -> &'static Glyph {
    static EMPTY: OnceLock<Glyph> = OnceLock::new();
    glyphs().get(name).unwrap_or_else(|| EMPTY.get_or_init(Glyph::default))
}

/// The letters.
pub fn letters() -> &'static Letters {
    static LETTERS: OnceLock<Letters> = OnceLock::new();
    LETTERS.get_or_init(read_letters)
}
