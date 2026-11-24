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
    let mut letters = Letters { regular: face("regular"), bold: face("bold"), italic: face("italic") };
    for face in [&mut letters.regular, &mut letters.bold, &mut letters.italic] {
        if let Some(sign) = face.get(&'c').filter(|_| !face.contains_key(&'©')).map(copyright_sign) {
            face.insert('©', sign);
        }
        accented(face);
    }
    letters
}

/// The accented letters TeX Gyre Schola's outlines here lack (score #1037:
/// "Thòiseach" lost its ò): each made of a letter's body and the accent of
/// one the face has — ò of o and à's grave — or, where the face has no such
/// accent, the bare letter; a long s is an s. A word keeps all its letters.
fn accented(face: &mut HashMap<char, Letter>) {
    // (the letter, its body's letter, the letter whose accent it takes)
    const MADE: [(char, char, char); 26] = [
        ('ì', 'î', 'à'),
        ('ò', 'o', 'à'),
        ('á', 'a', 'é'),
        ('í', 'î', 'é'),
        ('ó', 'o', 'é'),
        ('ú', 'u', 'é'),
        ('ý', 'y', 'é'),
        ('Ì', 'I', 'À'),
        ('Ò', 'O', 'À'),
        ('Á', 'A', 'É'),
        ('Í', 'I', 'É'),
        ('Ó', 'O', 'É'),
        ('Ú', 'U', 'É'),
        ('Ý', 'Y', 'É'),
        ('ñ', 'n', ' '),
        ('Ñ', 'N', ' '),
        ('ã', 'a', ' '),
        ('õ', 'o', ' '),
        ('å', 'a', ' '),
        ('Å', 'A', ' '),
        ('š', 's', ' '),
        ('č', 'c', ' '),
        ('ž', 'z', ' '),
        ('ě', 'e', ' '),
        ('ř', 'r', ' '),
        ('ſ', 's', ' '),
    ];
    // A letter's outlines above `line` (its accent) and the others (its body).
    let split = |l: &Letter, line: f64| -> (Vec<f64>, Vec<f64>) {
        let (mut over, mut body) = (Vec::new(), Vec::new());
        let mut i = 0;
        while i < l.f.len() {
            let mut j = i;
            let mut low = f64::MIN;
            loop {
                let args = crate::text::args_of(l.f.get(j).copied().unwrap_or(4.0));
                for k in (1..args).step_by(2) {
                    low = low.max(l.f.get(j + 1 + k).copied().unwrap_or(0.0));
                }
                j += 1 + args;
                if j >= l.f.len() || l.f.get(j).copied().unwrap_or(0.0) == 0.0 {
                    break;
                }
            }
            if low < line { &mut over } else { &mut body }.extend_from_slice(&l.f[i..j]);
            i = j;
        }
        (over, body)
    };
    let top = |face: &HashMap<char, Letter>, ch: char| -> f64 {
        let Some(l) = face.get(&ch) else { return 0.0 };
        let mut high: f64 = 0.0;
        let mut i = 0;
        while i < l.f.len() {
            let args = crate::text::args_of(l.f[i]);
            for k in (1..args).step_by(2) {
                high = high.min(l.f.get(i + 1 + k).copied().unwrap_or(0.0));
            }
            i += 1 + args;
        }
        high
    };
    for (ch, base, from) in MADE {
        if face.contains_key(&ch) {
            continue;
        }
        // Over the small letters' height, or the capitals'.
        let line = if ch.is_uppercase() { top(face, 'X') } else { top(face, 'x') } - 15.0;
        let Some(body_of) = face.get(&base) else { continue };
        let (_, mut f) = split(body_of, line);
        let a = body_of.a;
        if let (Some(src), Some(plain)) = (face.get(&from).filter(|_| from != ' '), face.get(&from_base(from))) {
            let (accent, _) = split(src, line);
            let dx = (a - plain.a) / 2.0;
            let mut i = 0;
            while i < accent.len() {
                let args = crate::text::args_of(accent[i]);
                f.push(accent[i]);
                for k in 0..args {
                    f.push(accent.get(i + 1 + k).copied().unwrap_or(0.0) + if k % 2 == 0 { dx } else { 0.0 });
                }
                i += 1 + args;
            }
        }
        face.insert(ch, Letter { a, f });
    }
}

/// The plain letter under an accented one the face has: à's a, É's E.
fn from_base(ch: char) -> char {
    match ch {
        'à' => 'a',
        'é' => 'e',
        'À' => 'A',
        'É' => 'E',
        other => other,
    }
}

/// A circle at (`cx`, `cy`) of radius `r`, one way round (`turn` 1) or the
/// other (-1), as fill commands.
fn circle(cx: f64, cy: f64, r: f64, turn: f64) -> Vec<f64> {
    let k = 0.552_284_749_830_793_4 * r;
    let (ry, ky) = (r * turn, k * turn);
    vec![
        0.0,
        cx + r,
        cy,
        3.0,
        cx + r,
        cy + ky,
        cx + k,
        cy + ry,
        cx,
        cy + ry,
        3.0,
        cx - k,
        cy + ry,
        cx - r,
        cy + ky,
        cx - r,
        cy,
        3.0,
        cx - r,
        cy - ky,
        cx - k,
        cy - ry,
        cx,
        cy - ry,
        3.0,
        cx + k,
        cy - ry,
        cx + r,
        cy - ky,
        cx + r,
        cy,
        4.0,
    ]
}

/// The copyright sign, which TeX Gyre Schola's outlines here lack: the
/// face's `c`, smaller, in a ring as tall as a capital — what a header's
/// `copyright = "© 2026 …"` begins with.
fn copyright_sign(c: &Letter) -> Letter {
    let (cx, cy, outer, inner) = (410.0, -350.0, 380.0, 332.0);
    let mut f = circle(cx, cy, outer, 1.0);
    f.extend(circle(cx, cy, inner, -1.0));
    // The `c`'s box, to centre it in the ring.
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut i = 0;
    while i < c.f.len() {
        let args = crate::text::args_of(c.f[i]);
        for j in (0..args).step_by(2) {
            let (x, y) = (c.f.get(i + 1 + j).copied().unwrap_or(0.0), c.f.get(i + 2 + j).copied().unwrap_or(0.0));
            (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
        }
        i += 1 + args;
    }
    let k = 0.62;
    let (dx, dy) = (cx - (x0 + x1) / 2.0 * k, cy - (y0 + y1) / 2.0 * k);
    let mut i = 0;
    while i < c.f.len() {
        let args = crate::text::args_of(c.f[i]);
        f.push(c.f[i]);
        for j in 0..args {
            let v = c.f.get(i + 1 + j).copied().unwrap_or(0.0);
            f.push(v * k + if j % 2 == 0 { dx } else { dy });
        }
        i += 1 + args;
    }
    Letter { a: 2.0 * cx, f }
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
