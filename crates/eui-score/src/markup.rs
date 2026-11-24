//! Words outside the music (score #1025): a `\markup` written at the top of
//! a file — before the first score, between two, after the last — is a
//! block of the page, as a system is. This reads one as a tree (`\column`,
//! `\line`, `\fill-line`, `\bold`, `\fontsize` …) and lays it out as
//! LilyPond's markup commands do: what is written side by side on one
//! baseline, a word space apart; a column's items under one another, a
//! `baseline-skip` (three spaces, scaled by `\fontsize`) from baseline to
//! baseline; a `\fill-line`'s spread over the line's width. The block is
//! then cut into rows — the lines of words no ink crosses between — which
//! the page stacker (`engrave::stack`) sets one under the other, and from
//! a page to the next.

use std::collections::HashMap;

use crate::glyphs::Weight;
use crate::marks::{LOWER, RAISE, SUP};
use crate::parse::{at_of, deeper, is, markup_item, number_at, refuse, scheme_end, warn, Tok, T};
use crate::text::{em, text_width, ASCENT, DESCENT};
use crate::ScoreError;

/// Where a column's items, or an aligned piece, stand against their
/// reference point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// Starting at it.
    Left,
    /// Centred on it.
    Centre,
    /// Ending at it.
    Right,
}

/// How a command sizes its words.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Size {
    /// `\fontsize #n`: `n` steps larger, and the lines as much further apart.
    Step(f64),
    /// `\smaller`, `\larger`: a step from the size around it.
    Relative(f64),
    /// `\small`, `\large`, `\huge` …: a step from the words' own size.
    Set(f64),
    /// `\abs-fontsize #pt`: in points.
    Points(f64),
    /// `\magnify #k`.
    Times(f64),
}

/// A markup, as written.
#[derive(Debug, Clone, PartialEq)]
pub enum Markup {
    /// A word, or a string.
    Word(String),
    /// `{ … }`, `\line`: side by side, a word space apart.
    Line(Vec<Markup>),
    /// `\concat`: side by side, touching.
    Concat(Vec<Markup>),
    /// `\column`, `\center-column`, `\right-column`: one under the other.
    Column(Vec<Markup>, Side),
    /// `\fill-line`: spread over the line's width — one centred, two at the
    /// ends, more with the middle ones evenly between.
    Fill(Vec<Markup>),
    /// `\wordwrap`, `\justify`: broken into lines no wider than the page's.
    Wrap(Vec<Markup>),
    /// `\hspace #n`: room, in staff spaces.
    HSpace(f64),
    /// `\vspace #n`: room above, `n` lines of it.
    VSpace(f64),
    /// `\bold`, `\italic`, `\upright`.
    Face(Weight, Box<Markup>),
    /// A size.
    Sized(Size, Box<Markup>),
    /// `\raise #n`, `\lower #n`: so many spaces up.
    Raise(f64, Box<Markup>),
    /// `\super` (true) and `\sub`.
    Script(bool, Box<Markup>),
    /// `\center-align`, `\left-align`, `\right-align`, `\halign`.
    Align(Side, Box<Markup>),
    /// `\fromproperty #'header:title`: a field of the header.
    Field(String),
    /// Nothing: `\null`, or what is read and not drawn.
    Null,
}

/// A `\markup` written outside the music, and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// What it says.
    pub markup: Markup,
    /// The score it stands before, from 0; the number of scores for one
    /// written after the last.
    pub before: usize,
    /// It starts a page: a `\pageBreak` before it, or a `\bookpart` starts
    /// with it.
    pub page: bool,
    /// A `\bookpart` starts with it.
    pub part: bool,
    /// A `\pageBreak` after it: the score it stands before starts a page.
    pub then_page: bool,
    /// Where it is written.
    pub at: usize,
}

impl Block {
    /// Its words, a line each from the top, on a page `width` wide.
    pub fn lines(&self, width: f64) -> Vec<String> {
        let mut pieces: Vec<Piece> = rows(&self.markup, width, &HashMap::new()).into_iter().flat_map(|r| r.pieces).collect();
        pieces.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        let mut out: Vec<(f64, String)> = Vec::new();
        for p in pieces {
            match out.last_mut() {
                Some((y, line)) if (*y - p.y).abs() < 1e-6 => {
                    line.push(' ');
                    line.push_str(&p.text);
                }
                _ => out.push((p.y, p.text)),
            }
        }
        out.into_iter().map(|(_, line)| line).collect()
    }
}

/// Words of one face and size, placed.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// The words.
    pub text: String,
    /// Their face.
    pub weight: Weight,
    /// Their em.
    pub em: f64,
    /// Where they start, from the line's left.
    pub x: f64,
    /// Their baseline, from the block's top.
    pub y: f64,
}

/// A row of a block: what no page break cuts.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Its top, from the block's.
    pub top: f64,
    /// Where its ink ends.
    pub bottom: f64,
    /// Its words.
    pub pieces: Vec<Piece>,
}

// ---- Reading ------------------------------------------------------------------

/// The markup at `i`, just after `\markup`, and the token after it.
pub(crate) fn read(toks: &[Tok], i: usize, at: usize) -> Result<(Markup, usize), ScoreError> {
    let (markup, j) = item(toks, i, at)?;
    crate::written::spend_text(chars(&markup), at)?;
    Ok((markup, j))
}

/// The characters a markup draws.
fn chars(m: &Markup) -> usize {
    match m {
        Markup::Word(w) => crate::written::chars(w),
        Markup::Line(v) | Markup::Concat(v) | Markup::Column(v, _) | Markup::Fill(v) | Markup::Wrap(v) => v.iter().map(chars).sum(),
        Markup::Face(_, m) | Markup::Sized(_, m) | Markup::Raise(_, m) | Markup::Script(_, m) | Markup::Align(_, m) => chars(m),
        Markup::HSpace(_) | Markup::VSpace(_) | Markup::Field(_) | Markup::Null => 0,
    }
}

/// A number written as a command's argument, `#-1.5` or `2`, and the token
/// after it; 0 for an argument that is not a number.
fn number(toks: &[Tok], i: usize) -> Result<(f64, usize), ScoreError> {
    let from = if is(toks, i, T::Sym('#')) { i + 1 } else { i };
    match number_at(toks, from) {
        Some((n, j)) if n.is_finite() => Ok((n.clamp(-1000.0, 1000.0), j)),
        _ if is(toks, i, T::Sym('#')) => Ok((0.0, scheme_end(toks, i)?)),
        _ => Ok((0.0, i + 1)),
    }
}

/// The items of the `{ … }` at `i`, and the token after its `}`.
fn list(toks: &[Tok], i: usize, at: usize) -> Result<(Vec<Markup>, usize), ScoreError> {
    let mut items: Vec<Markup> = Vec::new();
    let mut j = i + 1;
    while j < toks.len() && toks[j].t != T::Sym('}') {
        // A full stop, a comma: on the word before it.
        if let (T::Sym(c), Some(Markup::Word(last))) = (toks[j].t, items.last_mut()) {
            if ".,;:!?)]".contains(c) {
                last.push(c);
                j += 1;
                continue;
            }
        }
        let (m, after) = item(toks, j, at)?;
        j = after;
        items.push(m);
    }
    if j >= toks.len() {
        return refuse("a \\markup { that is never closed", at);
    }
    Ok((items, j + 1))
}

/// The item after a command that takes a list: its items, or the one item.
fn items_after(toks: &[Tok], i: usize, at: usize) -> Result<(Vec<Markup>, usize), ScoreError> {
    if is(toks, i, T::Sym('{')) {
        return list(toks, i, at);
    }
    let (m, j) = item(toks, i, at)?;
    Ok((
        match m {
            Markup::Line(v) => v,
            Markup::Null => Vec::new(),
            other => vec![other],
        },
        j,
    ))
}

/// One item of markup at `i` and the token after it.
fn item(toks: &[Tok], i: usize, at: usize) -> Result<(Markup, usize), ScoreError> {
    let _deeper = deeper(at_of(toks, i))?;
    let Some(t) = toks.get(i) else { return refuse("a \\markup that is never finished", at) };
    // What a command makes of the item after it; nothing of nothing.
    let wrapped = |wrap: &dyn Fn(Box<Markup>) -> Markup, from: usize| -> Result<(Markup, usize), ScoreError> {
        let (m, j) = item(toks, from, at)?;
        Ok((if m == Markup::Null { m } else { wrap(Box::new(m)) }, j))
    };
    match t.t {
        T::Sym('{') => {
            let (mut items, j) = list(toks, i, at)?;
            Ok((if items.len() == 1 { items.remove(0) } else { Markup::Line(items) }, j))
        }
        T::Str => Ok((Markup::Word(t.v.clone()), i + 1)),
        T::Word | T::Num => {
            let mut text = t.v.clone();
            if t.t == T::Word {
                for _ in 0..t.marks.max(0) {
                    text.push('\'');
                }
                for _ in 0..(-t.marks).max(0) {
                    text.push(',');
                }
            }
            Ok((Markup::Word(text), i + 1))
        }
        T::Sym(c) if ".,;:!?-+=*/'()[]".contains(c) => Ok((Markup::Word(c.to_string()), i + 1)),
        T::Cmd => match t.v.as_str() {
            "bold" => wrapped(&|m| Markup::Face(Weight::Bold, m), i + 1),
            "italic" => wrapped(&|m| Markup::Face(Weight::Italic, m), i + 1),
            "upright" | "normal-text" | "roman" | "medium" => wrapped(&|m| Markup::Face(Weight::Regular, m), i + 1),
            "line" => {
                let (items, j) = items_after(toks, i + 1, at)?;
                Ok((Markup::Line(items), j))
            }
            "concat" => {
                let (items, j) = items_after(toks, i + 1, at)?;
                Ok((Markup::Concat(items), j))
            }
            "column" | "left-column" | "dir-column" | "center-column" | "right-column" => {
                let side = match t.v.as_str() {
                    "center-column" => Side::Centre,
                    "right-column" => Side::Right,
                    _ => Side::Left,
                };
                let (items, j) = items_after(toks, i + 1, at)?;
                Ok((Markup::Column(items, side), j))
            }
            "fill-line" => {
                let (items, j) = items_after(toks, i + 1, at)?;
                Ok((Markup::Fill(items), j))
            }
            "wordwrap" | "justify" => {
                let (items, j) = items_after(toks, i + 1, at)?;
                Ok((Markup::Wrap(items), j))
            }
            "wordwrap-string" | "justify-string" => {
                let from = if is(toks, i + 1, T::Sym('#')) { i + 2 } else { i + 1 };
                let Some(s) = toks.get(from).filter(|s| s.t == T::Str) else { return refuse(format!("\\{} wants its words in quotes", t.v), t.at) };
                Ok((Markup::Wrap(s.v.split_whitespace().map(|w| Markup::Word(w.to_owned())).collect()), from + 1))
            }
            "hspace" => number(toks, i + 1).map(|(n, j)| (Markup::HSpace(n), j)),
            "vspace" => number(toks, i + 1).map(|(n, j)| (Markup::VSpace(n), j)),
            "null" => Ok((Markup::Null, i + 1)),
            "fontsize" | "abs-fontsize" | "magnify" | "raise" | "lower" | "halign" => {
                let (n, j) = number(toks, i + 1)?;
                match t.v.as_str() {
                    "fontsize" => wrapped(&|m| Markup::Sized(Size::Step(n), m), j),
                    "abs-fontsize" => wrapped(&|m| Markup::Sized(Size::Points(n), m), j),
                    "magnify" => wrapped(&|m| Markup::Sized(Size::Times(n), m), j),
                    "raise" => wrapped(&|m| Markup::Raise(n, m), j),
                    "lower" => wrapped(&|m| Markup::Raise(-n, m), j),
                    _ => wrapped(
                        &|m| {
                            Markup::Align(
                                if n < -0.5 {
                                    Side::Left
                                } else if n > 0.5 {
                                    Side::Right
                                } else {
                                    Side::Centre
                                },
                                m,
                            )
                        },
                        j,
                    ),
                }
            }
            "smaller" => wrapped(&|m| Markup::Sized(Size::Relative(-1.0), m), i + 1),
            "larger" => wrapped(&|m| Markup::Sized(Size::Relative(1.0), m), i + 1),
            "teeny" => wrapped(&|m| Markup::Sized(Size::Set(-3.0), m), i + 1),
            "tiny" => wrapped(&|m| Markup::Sized(Size::Set(-2.0), m), i + 1),
            "small" => wrapped(&|m| Markup::Sized(Size::Set(-1.0), m), i + 1),
            "normalsize" => wrapped(&|m| Markup::Sized(Size::Set(0.0), m), i + 1),
            "large" => wrapped(&|m| Markup::Sized(Size::Set(1.0), m), i + 1),
            "huge" => wrapped(&|m| Markup::Sized(Size::Set(2.0), m), i + 1),
            "super" => wrapped(&|m| Markup::Script(true, m), i + 1),
            "sub" => wrapped(&|m| Markup::Script(false, m), i + 1),
            "center-align" => wrapped(&|m| Markup::Align(Side::Centre, m), i + 1),
            "left-align" => wrapped(&|m| Markup::Align(Side::Left, m), i + 1),
            "right-align" => wrapped(&|m| Markup::Align(Side::Right, m), i + 1),
            "fromproperty" => {
                // `#'header:title`: quote, `header`, `:`, the field.
                let end = if is(toks, i + 1, T::Sym('#')) { scheme_end(toks, i + 1)? } else { i + 2 };
                let name = toks.get(i + 1..end).unwrap_or(&[]).iter().rev().find(|w| w.t == T::Word).map(|w| w.v.clone());
                let header = toks.get(i + 1..end).unwrap_or(&[]).iter().any(|w| w.t == T::Word && w.v == "header");
                Ok((
                    match name {
                        Some(name) if header && name != "header" => Markup::Field(name),
                        _ => Markup::Null,
                    },
                    end,
                ))
            }
            "box" | "circle" | "rounded-box" | "underline" | "sans" | "serif" | "caps" | "smallCaps" | "typewriter" | "number" | "dynamic" | "text" | "bracket" | "parenthesize" | "vcenter" => {
                item(toks, i + 1, at)
            }
            "with-color" | "translate" | "translate-scaled" | "override" | "with-url" | "with-link" | "page-link" | "pad-markup" | "pad-around" | "pad-x" | "hcenter-in" | "rotate" | "scale"
            | "with-dimensions" | "general-align" | "pad-to-box" => {
                // Scheme arguments, then the markup they style, drawn as it is.
                let mut j = i + 1;
                if t.v == "with-color" && !is(toks, j, T::Sym('#')) {
                    j += 1;
                }
                while is(toks, j, T::Sym('#')) {
                    j = scheme_end(toks, j)?;
                }
                item(toks, j, at)
            }
            "combine" => {
                // One markup over another (score #1037): side by side here,
                // what is not drawn of them left out.
                let (first, j) = item(toks, i + 1, at)?;
                let (second, j) = item(toks, j, at)?;
                Ok((
                    match (first, second) {
                        (Markup::Null, m) | (m, Markup::Null) => m,
                        (a, b) => Markup::Concat(vec![a, b]),
                    },
                    j,
                ))
            }
            "musicglyph" | "fret-diagram" | "fret-diagram-terse" | "rN" => {
                // What a note's words draw and a page's do not: read, so a
                // wrong one is refused as it is there.
                let mut runs = Vec::new();
                let j = markup_item(toks, i, Weight::Regular, &mut runs, at)?;
                warn(format!("\\{} in a \\markup outside the music is not drawn", t.v), t.at);
                Ok((Markup::Null, j))
            }
            other => {
                // Markup this does not draw (score #1031) — a picture
                // (`\epsfile`), a line, a command of the file's own — is
                // passed over with its Scheme arguments, and said so; what
                // it styles, when it styles anything, is kept.
                warn(if other == "epsfile" { "\\epsfile: a picture in a \\markup is not drawn".to_owned() } else { format!("\\{other} is not markup this draws") }, t.at);
                let mut j = i + 1;
                while is(toks, j, T::Sym('#')) {
                    j = scheme_end(toks, j)?;
                }
                const ALONE: [&str; 13] =
                    ["epsfile", "strut", "draw-line", "draw-hline", "draw-dashed-line", "draw-dotted-line", "draw-circle", "char", "postscript", "filled-box", "eyeglasses", "hline", "path"];
                match toks.get(j).map(|n| n.t) {
                    Some(T::Sym('{') | T::Str | T::Cmd) if !ALONE.contains(&other) => item(toks, j, at),
                    _ => Ok((Markup::Null, j)),
                }
            }
        },
        _ => refuse(format!("'{}' is not markup this draws", t.v), t.at),
    }
}

// ---- Laying out -----------------------------------------------------------------

/// LilyPond's `baseline-skip`: from a line's baseline to the next's.
pub(crate) const BASELINE_SKIP: f64 = 3.0;

/// An interval, empty when it ends before it starts.
type Span = (f64, f64);

const EMPTY: Span = (f64::INFINITY, f64::NEG_INFINITY);

fn empty(s: Span) -> bool {
    s.0 > s.1
}

fn join(a: Span, b: Span) -> Span {
    (a.0.min(b.0), a.1.max(b.1))
}

/// A markup laid out around its reference point: the left end of its
/// baseline.
#[derive(Debug, Clone)]
struct Laid {
    pieces: Vec<Piece>,
    /// Its width's reach.
    x: Span,
    /// Its height's, y down.
    y: Span,
}

impl Laid {
    fn nothing() -> Self {
        Laid { pieces: Vec::new(), x: EMPTY, y: EMPTY }
    }

    fn moved(mut self, dx: f64, dy: f64) -> Self {
        for p in &mut self.pieces {
            p.x += dx;
            p.y += dy;
        }
        if !empty(self.x) {
            self.x = (self.x.0 + dx, self.x.1 + dx);
        }
        if !empty(self.y) {
            self.y = (self.y.0 + dy, self.y.1 + dy);
        }
        self
    }

    fn take(&mut self, other: Laid) {
        self.x = join(self.x, other.x);
        self.y = join(self.y, other.y);
        self.pieces.extend(other.pieces);
    }

    /// Against its reference point, as `side` says.
    fn aligned(self, side: Side) -> Self {
        if empty(self.x) {
            return self;
        }
        let dx = match side {
            Side::Left => -self.x.0,
            Side::Centre => -(self.x.0 + self.x.1) / 2.0,
            Side::Right => -self.x.1,
        };
        self.moved(dx, 0.0)
    }
}

/// What the words around a markup say of it.
#[derive(Clone, Copy)]
struct Style<'a> {
    weight: Weight,
    em: f64,
    /// From a line to the next.
    skip: f64,
    /// The line's width.
    width: f64,
    header: &'a HashMap<String, String>,
}

impl Style<'_> {
    /// The room between two words: a space of their face.
    fn word_space(&self) -> f64 {
        text_width(" ", self.weight, self.em)
    }
}

/// `items` side by side from their reference point, `gap` between them.
fn in_line(items: &[Markup], st: &Style, spaced: bool) -> Laid {
    let mut out = Laid::nothing();
    let mut at = 0.0;
    let mut first = true;
    for m in items {
        let gap = if spaced && !first { st.word_space() } else { 0.0 };
        if let Markup::HSpace(n) = m {
            at += gap + n;
            first = false;
            continue;
        }
        let laid = lay(m, st);
        if empty(laid.x) {
            // Room above, or nothing: no word of the line.
            out.y = join(out.y, laid.y);
            continue;
        }
        let dx = at + gap - laid.x.0;
        let laid = laid.moved(dx, 0.0);
        at = laid.x.1;
        first = false;
        out.take(laid);
    }
    if !first {
        out.x = join(out.x, (0.0, at.max(0.0)));
    }
    out
}

/// `lines` one under the other, the first's baseline the whole's.
fn stacked(lines: Vec<Laid>, skip: f64) -> Laid {
    let mut out = Laid::nothing();
    let mut baseline = 0.0;
    // How far under its baseline the line before reaches.
    let mut under = 0.0;
    for (k, line) in lines.into_iter().enumerate() {
        let over = if empty(line.y) { 0.0 } else { -line.y.0 };
        if k > 0 {
            baseline += skip.max(under + over);
        }
        under = if empty(line.y) { 0.0 } else { line.y.1 };
        out.take(line.moved(0.0, baseline));
    }
    out
}

fn lay(m: &Markup, st: &Style) -> Laid {
    match m {
        Markup::Word(text) => {
            let w = text_width(text, st.weight, st.em);
            if text.trim().is_empty() {
                // Spaces: as wide as they are, and no ink.
                return Laid { pieces: Vec::new(), x: (0.0, w), y: EMPTY };
            }
            Laid { pieces: vec![Piece { text: text.trim_end().to_owned(), weight: st.weight, em: st.em, x: 0.0, y: 0.0 }], x: (0.0, w), y: (-ASCENT * st.em, DESCENT * st.em) }
        }
        Markup::Field(name) => match st.header.get(name).filter(|v| !v.trim().is_empty()) {
            Some(v) => lay(&Markup::Word(v.clone()), st),
            None => Laid::nothing(),
        },
        Markup::Line(items) => in_line(items, st, true),
        Markup::Concat(items) => in_line(items, st, false),
        Markup::Column(items, side) => stacked(items.iter().map(|m| lay(m, st).aligned_in_column(*side)).collect(), st.skip),
        Markup::Fill(items) => {
            let laid: Vec<Laid> = items.iter().map(|m| lay(m, st)).collect();
            let n = laid.len();
            let mut out = Laid::nothing();
            for (k, l) in laid.into_iter().enumerate() {
                if empty(l.x) {
                    out.y = join(out.y, l.y);
                    continue;
                }
                let w = l.x.1 - l.x.0;
                let from = if n == 1 {
                    (st.width - w) / 2.0
                } else if k == 0 {
                    0.0
                } else if k + 1 == n {
                    st.width - w
                } else {
                    st.width * k as f64 / (n - 1) as f64 - w / 2.0
                };
                let dx = from - l.x.0;
                out.take(l.moved(dx, 0.0));
            }
            out.x = join(out.x, (0.0, st.width));
            out
        }
        Markup::Wrap(items) => {
            let mut lines: Vec<Vec<Markup>> = Vec::new();
            let mut wide = 0.0;
            for m in items {
                let w = {
                    let l = lay(m, st);
                    if empty(l.x) {
                        0.0
                    } else {
                        l.x.1 - l.x.0
                    }
                };
                match lines.last_mut() {
                    Some(line) if wide + st.word_space() + w <= st.width => {
                        line.push(m.clone());
                        wide += st.word_space() + w;
                    }
                    _ => {
                        lines.push(vec![m.clone()]);
                        wide = w;
                    }
                }
            }
            stacked(lines.iter().map(|line| in_line(line, st, true)).collect(), st.skip)
        }
        Markup::HSpace(n) => Laid { pieces: Vec::new(), x: (0.0, n.max(0.0)), y: EMPTY },
        Markup::VSpace(n) => Laid { pieces: Vec::new(), x: EMPTY, y: (-(n * BASELINE_SKIP).max(0.0), 0.0) },
        Markup::Face(weight, m) => lay(m, &Style { weight: *weight, ..*st }),
        Markup::Sized(size, m) => {
            let step = |n: f64| (n * 0.1155245300933242).exp();
            let (e, skip) = match *size {
                Size::Step(n) => (st.em * step(n), st.skip * step(n)),
                Size::Relative(n) => (st.em * step(n), st.skip),
                Size::Set(n) => (em(n), st.skip),
                Size::Points(pt) if pt > 0.0 => (pt * 0.2, st.skip * pt * 0.2 / st.em),
                Size::Times(k) if k > 0.0 => (st.em * k, st.skip),
                _ => (st.em, st.skip),
            };
            lay(m, &Style { em: e.clamp(0.2, 60.0), skip: skip.clamp(0.2, 80.0), ..*st })
        }
        Markup::Raise(by, m) => lay(m, st).moved(0.0, -by),
        Markup::Script(up, m) => {
            let dy = if *up { -st.em * RAISE } else { st.em * LOWER };
            lay(m, &Style { em: st.em * SUP, ..*st }).moved(0.0, dy)
        }
        Markup::Align(side, m) => lay(m, st).aligned(*side),
        Markup::Null => Laid::nothing(),
    }
}

impl Laid {
    /// A column's item: where it starts for a `\column`, centred or ended
    /// on the column's line for the others.
    fn aligned_in_column(self, side: Side) -> Self {
        match side {
            Side::Left => self,
            other => self.aligned(other),
        }
    }
}

/// A block's rows on a line `width` wide, from its top: the lines of
/// words no ink crosses between, each with what it draws — words of one
/// face written with a space between them as one piece.
pub fn rows(m: &Markup, width: f64, header: &HashMap<String, String>) -> Vec<Row> {
    let st = Style { weight: Weight::Regular, em: em(0.0), skip: BASELINE_SKIP, width, header };
    let laid = lay(m, &st);
    // Nothing left of the line's start, and the block's top at 0.
    let dx = if !empty(laid.x) && laid.x.0 < 0.0 { -laid.x.0 } else { 0.0 };
    let (top, tall) = if empty(laid.y) { (0.0, 0.0) } else { (laid.y.0, laid.y.1 - laid.y.0) };
    let laid = laid.moved(dx, -top);
    let mut pieces = laid.pieces;
    pieces.sort_by(|a, b| (a.y - ASCENT * a.em).total_cmp(&(b.y - ASCENT * b.em)));
    let mut out: Vec<Row> = Vec::new();
    for p in pieces {
        let (hi, lo) = (p.y - ASCENT * p.em, p.y + DESCENT * p.em);
        match out.last_mut() {
            Some(row) if hi < row.bottom - 1e-9 => {
                row.bottom = row.bottom.max(lo);
                row.pieces.push(p);
            }
            _ => out.push(Row { top: if out.is_empty() { 0.0 } else { hi }, bottom: lo, pieces: vec![p] }),
        }
    }
    match out.last_mut() {
        Some(last) => last.bottom = last.bottom.max(tall),
        None => out.push(Row { top: 0.0, bottom: tall, pieces: Vec::new() }),
    }
    for row in &mut out {
        row.pieces.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        let mut merged: Vec<Piece> = Vec::with_capacity(row.pieces.len());
        for p in row.pieces.drain(..) {
            match merged.last_mut() {
                Some(last)
                    if last.weight == p.weight
                        && (last.em - p.em).abs() < 1e-9
                        && (last.y - p.y).abs() < 1e-9
                        && (last.x + text_width(&last.text, last.weight, last.em) + text_width(" ", last.weight, last.em) - p.x).abs() < 1e-6 =>
                {
                    last.text.push(' ');
                    last.text.push_str(&p.text);
                }
                _ => merged.push(p),
            }
        }
        row.pieces = merged;
    }
    out
}
