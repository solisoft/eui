//! The layout settings a score honours (score #403): `\override`, `\set`,
//! `\tweak`, `\omit` and `\hide`, `\once`, `\revert`, `\unset` and `\undo`,
//! and the switches that stand for them (`\slurUp`, `\numericTimeSignature`
//! …), in the music, in a context's `\with { }` and in `\layout { \context
//! { … } }` — as far as the engraver has a hook for the property. What it
//! has none for is still read and passed over with a warning, as before.
//!
//! A setting is kept by its grob and property, `"TextScript.extra-offset"`,
//! or by its context property, `"fontSize"`; each note, rest and setting of
//! a staff carries the settings in force where it is written (its
//! [`Look`]), what the staff's `\with` and the `\layout` say under them.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::parse::{is, path_end, value_end, Ev, Kind, Tok, T};
use crate::ScoreError;
use crate::WHOLE;

/// The settings in force: key to value.
pub type Map = BTreeMap<String, Val>;

/// A setting's value, read from its Scheme.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    /// A number: a length in staff spaces, a direction (1 up, -1 down), a
    /// font size, a moment in ticks.
    Num(f64),
    /// `#'(x . y)`: an offset, a beam's two ends.
    Pair(f64, f64),
    /// `##t`, `##f`.
    Bool(bool),
    /// A name: `#'cross`, `#format-mark-box-letters`.
    Sym(String),
    /// A list of numbers: `#'(3 3 2)`, `##(#f #t #t)` as 0 and 1.
    List(Vec<f64>),
}

/// The settings a note, a rest or a staff's setting carries.
#[derive(Debug, Clone, Default)]
pub struct Look(Option<Arc<Map>>);

impl Look {
    /// The value set for `key`.
    pub fn get(&self, key: &str) -> Option<&Val> {
        self.0.as_ref().and_then(|m| m.get(key))
    }

    /// Nothing is set.
    pub fn is_empty(&self) -> bool {
        self.0.as_ref().is_none_or(|m| m.is_empty())
    }

    /// A number set for `key`.
    pub fn num(&self, key: &str) -> Option<f64> {
        match self.get(key) {
            Some(Val::Num(n)) => Some(*n),
            _ => None,
        }
    }

    /// A pair set for `key`.
    pub fn pair(&self, key: &str) -> Option<(f64, f64)> {
        match self.get(key) {
            Some(Val::Pair(a, b)) => Some((*a, *b)),
            _ => None,
        }
    }

    /// A name set for `key`.
    pub fn sym(&self, key: &str) -> Option<&str> {
        match self.get(key) {
            Some(Val::Sym(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// A switch set for `key`.
    pub fn flag(&self, key: &str) -> Option<bool> {
        match self.get(key) {
            Some(Val::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// The direction a grob is set to: 1 up, -1 down; none for neutral.
    pub fn dir(&self, grob: &str) -> Option<i64> {
        let d = self.num(&format!("{grob}.direction"))?;
        if d > 0.0 {
            Some(1)
        } else if d < 0.0 {
            Some(-1)
        } else {
            None
        }
    }

    /// The same as a script's `dir`: -1 above, 1 below.
    pub fn side(&self, grob: &str) -> Option<i64> {
        self.dir(grob).map(|d| -d)
    }

    /// A grob not drawn: `\omit` (its stencil `##f`) or `\hide` (made
    /// transparent).
    pub fn omitted(&self, grob: &str) -> bool {
        self.flag(&format!("{grob}.stencil")) == Some(false) || self.flag(&format!("{grob}.transparent")) == Some(true)
    }

    /// `extra-offset`, as a move on the page: x right, y down.
    pub fn extra(&self, grob: &str) -> (f64, f64) {
        self.pair(&format!("{grob}.extra-offset")).map_or((0.0, 0.0), |(x, y)| (x, -y))
    }

    /// The settings themselves.
    pub fn map(&self) -> Option<&Map> {
        self.0.as_deref()
    }

    pub(crate) fn of(map: Map) -> Self {
        if map.is_empty() {
            Look(None)
        } else {
            Look(Some(Arc::new(map)))
        }
    }
}

// ---- What is honoured -----------------------------------------------------------

/// The grobs `\omit` and `\hide` are honoured on.
const OMITTABLE: [&str; 14] =
    ["TimeSignature", "BarNumber", "Stem", "Flag", "Beam", "StaffSymbol", "Clef", "KeySignature", "TupletNumber", "TupletBracket", "BarLine", "Script", "DynamicText", "TextScript"];

/// The grobs whose direction is honoured.
const DIRECTED: [&str; 11] = ["Stem", "Slur", "Tie", "PhrasingSlur", "DynamicLineSpanner", "DynamicText", "TextScript", "Script", "Fingering", "TupletBracket", "TupletNumber"];

/// The grobs that are moved: `extra-offset`, `X-offset`, `Y-offset`.
const MOVED: [&str; 6] = ["TextScript", "DynamicText", "Script", "RehearsalMark", "MetronomeMark", "Fingering"];

/// The grobs kept off what is around them: `padding`, `outside-staff-padding`,
/// `staff-padding`.
const PADDED: [&str; 7] = ["TextScript", "DynamicText", "DynamicLineSpanner", "Script", "RehearsalMark", "MetronomeMark", "Fingering"];

/// The grobs aligned by `self-alignment-X`.
const ALIGNED: [&str; 3] = ["TextScript", "RehearsalMark", "LyricText"];

/// The note head styles drawn.
const HEAD_STYLES: [&str; 9] = ["default", "cross", "xcircle", "diamond", "triangle", "slash", "harmonic", "harmonic-black", "harmonic-mixed"];

/// The grobs a staff (or the score) makes, not a voice: their settings hold
/// for the staff, whatever voice writes them.
const STAFF_GROBS: [&str; 14] = [
    "TimeSignature",
    "KeySignature",
    "Clef",
    "StaffSymbol",
    "BarLine",
    "BarNumber",
    "RehearsalMark",
    "MetronomeMark",
    "VerticalAxisGroup",
    "SystemStartBracket",
    "SystemStartBrace",
    "SpanBar",
    "SpacingSpanner",
    "StaffGrouper",
];

/// The contexts a path may start with.
const CONTEXTS: [&str; 17] = [
    "Score",
    "Staff",
    "Voice",
    "PianoStaff",
    "GrandStaff",
    "StaffGroup",
    "ChoirStaff",
    "DrumStaff",
    "DrumVoice",
    "TabStaff",
    "TabVoice",
    "Lyrics",
    "Timing",
    "ChordNames",
    "Dynamics",
    "RhythmicStaff",
    "CueVoice",
];

/// The contexts whose settings hold for a whole staff, or more.
const WIDE: [&str; 11] = ["Score", "Staff", "PianoStaff", "GrandStaff", "StaffGroup", "ChoirStaff", "DrumStaff", "TabStaff", "Timing", "RhythmicStaff", "ChordNames"];

/// The context properties `\set` is honoured for, and whether they are a
/// staff's (or the score's) rather than a voice's.
const CONTEXT_PROPS: [(&str, bool); 9] = [
    ("fontSize", false),
    ("subdivideBeams", true),
    ("baseMoment", true),
    ("beatStructure", true),
    ("beamExceptions", true),
    ("markFormatter", true),
    ("rehearsalMarkFormatter", true),
    ("barNumberVisibility", true),
    ("currentBarNumber", true),
];

/// A setting's path split: its context, if written, and its key.
fn split(words: &[String], grob: bool) -> Option<(Option<String>, String)> {
    let (ctx, rest) = match words.first() {
        Some(w) if CONTEXTS.contains(&w.as_str()) && words.len() > 1 => (Some(w.clone()), &words[1..]),
        _ => (None, words),
    };
    if grob && rest.len() < 2 || !grob && rest.len() != 1 {
        return None;
    }
    Some((ctx, rest.join(".")))
}

/// Whether `key` (and the value read for it) is one the engraver draws.
fn honoured(key: &str, v: &Val) -> bool {
    if let Some(prop) = CONTEXT_PROPS.iter().find(|p| p.0 == key) {
        return match prop.0 {
            "fontSize" | "currentBarNumber" | "baseMoment" => matches!(v, Val::Num(_)),
            "subdivideBeams" => matches!(v, Val::Bool(_)),
            "beatStructure" | "beamExceptions" => matches!(v, Val::List(_)),
            "barNumberVisibility" => matches!(v, Val::Sym(_) | Val::List(_)),
            _ => matches!(v, Val::Sym(s) if formatter(s).is_some()),
        };
    }
    let Some((grob, prop)) = key.split_once('.') else { return false };
    match prop {
        "direction" => DIRECTED.contains(&grob) && matches!(v, Val::Num(_)),
        "extra-offset" => MOVED.contains(&grob) && matches!(v, Val::Pair(..)),
        "X-offset" | "Y-offset" => MOVED.contains(&grob) && matches!(v, Val::Num(_)),
        "padding" | "outside-staff-padding" | "staff-padding" => PADDED.contains(&grob) && matches!(v, Val::Num(_)),
        "self-alignment-X" => ALIGNED.contains(&grob) && matches!(v, Val::Num(_)),
        "font-size" => ["NoteHead", "TextScript"].contains(&grob) && matches!(v, Val::Num(_)),
        "stencil" => OMITTABLE.contains(&grob) && *v == Val::Bool(false),
        "transparent" => OMITTABLE.contains(&grob) && matches!(v, Val::Bool(_)),
        "style" => match grob {
            "TimeSignature" => matches!(v, Val::Sym(s) if ["numbered", "C", "single-digit", "default"].contains(&s.as_str())),
            "NoteHead" => matches!(v, Val::Sym(s) if HEAD_STYLES.contains(&s.as_str())),
            _ => false,
        },
        "positions" => grob == "Beam" && matches!(v, Val::Pair(..)),
        "damping" => grob == "Beam" && matches!(v, Val::Num(_)),
        "break-visibility" => grob == "BarNumber" && matches!(v, Val::List(l) if l.len() == 3),
        "remove-empty" | "remove-first" => grob == "VerticalAxisGroup" && matches!(v, Val::Bool(_)),
        "collapse-height" => ["SystemStartBracket", "SystemStartBrace"].contains(&grob) && matches!(v, Val::Num(_)),
        // A staff drawn smaller (score #407): its lines' distance, and their
        // thickness, which follows it.
        "staff-space" | "thickness" => grob == "StaffSymbol" && matches!(v, Val::Num(n) if *n >= 0.25 && *n <= 4.0),
        _ => spacing_key(grob, prop, v),
    }
}

/// The vertical spacing alists between staves (score #409):
/// `VerticalAxisGroup.staff-staff-spacing` and `default-staff-staff-spacing`,
/// `StaffGrouper.staff-staff-spacing` and `staffgroup-staff-spacing` — the
/// whole alist (kept as [`SPACING_KEYS`]' four numbers, NaN where it says
/// nothing) or one of its keys (`….basic-distance = #12`).
fn spacing_key(grob: &str, prop: &str, v: &Val) -> bool {
    let (alist, key) = match prop.split_once('.') {
        Some((a, k)) => (a, Some(k)),
        None => (prop, None),
    };
    let known = match grob {
        "VerticalAxisGroup" => ["staff-staff-spacing", "default-staff-staff-spacing"].contains(&alist),
        "StaffGrouper" => ["staff-staff-spacing", "staffgroup-staff-spacing"].contains(&alist),
        _ => false,
    };
    known
        && match key {
            Some(k) => SPACING_KEYS.contains(&k) && matches!(v, Val::Num(n) if n.is_finite()),
            None => matches!(v, Val::List(l) if l.len() == SPACING_KEYS.len()),
        }
}

/// A spacing alist's keys, in the order its value keeps them.
pub const SPACING_KEYS: [&str; 4] = ["basic-distance", "minimum-distance", "padding", "stretchability"];

/// Whether a setting of `key`, in context `ctx`, holds for the whole staff.
fn wide(ctx: Option<&str>, key: &str) -> bool {
    if ctx.is_some_and(|c| WIDE.contains(&c)) {
        return true;
    }
    if let Some(p) = CONTEXT_PROPS.iter().find(|p| p.0 == key) {
        return p.1 && ctx.is_none_or(|c| c != "Voice");
    }
    key.split_once('.').is_some_and(|(g, _)| STAFF_GROBS.contains(&g))
}

// ---- Reading Scheme --------------------------------------------------------------

/// A Scheme datum, as far as a setting's value needs one.
#[derive(Debug, Clone, PartialEq)]
enum Sx {
    Num(f64),
    Word(String),
    Str(String),
    Bool(bool),
    List(Vec<Sx>),
    Pair(Box<Sx>, Box<Sx>),
}

fn tok_end(t: &Tok) -> usize {
    t.at + t.v.chars().count()
}

/// A number from `i` — `-1`, `0.5`, `1/8`, `+inf.0` — and the token after it.
fn number(toks: &[Tok], mut i: usize, end: usize) -> Option<(f64, usize)> {
    let mut sign = 1.0;
    if i < end && matches!(toks[i].t, T::Sym('-') | T::Sym('+')) && toks.get(i + 1).is_some_and(|n| n.at == toks[i].at + 1) {
        if toks[i].t == T::Sym('-') {
            sign = -1.0;
        }
        i += 1;
    }
    if i < end && toks[i].t == T::Word && toks[i].v == "inf" {
        let mut j = i + 1;
        if j + 1 < end && is(toks, j, T::Sym('.')) && is(toks, j + 1, T::Num) {
            j += 2;
        }
        return Some((sign * f64::INFINITY, j));
    }
    if i >= end || toks[i].t != T::Num {
        return None;
    }
    let t = &toks[i];
    let mut value = match t.v.split_once('/') {
        Some((a, b)) => a.parse::<f64>().ok()? / b.parse::<f64>().ok().filter(|d| *d != 0.0)?,
        None => t.v.parse::<f64>().ok()?,
    };
    let mut j = i + 1;
    if j + 1 < end && is(toks, j, T::Sym('.')) && toks[j].at == tok_end(t) && is(toks, j + 1, T::Num) && toks[j + 1].at == toks[j].at + 1 {
        value = format!("{}.{}", t.v, toks[j + 1].v).parse().ok()?;
        j += 2;
    }
    Some((sign * value, j))
}

/// A datum from `i`, inside a Scheme expression; and the token after it.
fn datum(toks: &[Tok], mut i: usize, end: usize) -> Option<(Sx, usize)> {
    while i < end && toks[i].t == T::Sym('\'') {
        i += 1;
    }
    let t = toks.get(i).filter(|_| i < end)?;
    match t.t {
        T::Sym('#') => {
            // `#t`, `#f`, `#(…)` a vector.
            let n = toks.get(i + 1)?;
            match n.t {
                T::Word if n.v == "t" || n.v == "f" => Some((Sx::Bool(n.v == "t"), i + 2)),
                T::Sym('(') => datum(toks, i + 1, end),
                _ => datum(toks, i + 1, end),
            }
        }
        T::Sym('(') => {
            let mut items = Vec::new();
            let mut j = i + 1;
            let mut tail = None;
            while j < end && !is(toks, j, T::Sym(')')) {
                if is(toks, j, T::Sym('.')) {
                    let (d, k) = datum(toks, j + 1, end)?;
                    tail = Some(d);
                    j = k;
                    continue;
                }
                let (d, k) = datum(toks, j, end)?;
                items.push(d);
                j = k;
            }
            if !is(toks, j, T::Sym(')')) {
                return None;
            }
            let value = match (tail, items.len()) {
                (Some(t), 1) => Sx::Pair(Box::new(items.remove(0)), Box::new(t)),
                (Some(_), _) => return None,
                (None, _) => Sx::List(items),
            };
            Some((value, j + 1))
        }
        T::Num | T::Sym('-' | '+') => {
            let (n, j) = number(toks, i, end)?;
            Some((Sx::Num(n), j))
        }
        T::Word if t.v == "inf" => {
            let (n, j) = number(toks, i, end)?;
            Some((Sx::Num(n), j))
        }
        T::Word => {
            // `ly:make-moment`: a name with its module.
            let mut name = t.v.clone();
            let mut j = i + 1;
            while j + 1 < end && is(toks, j, T::Sym(':')) && toks[j].at == tok_end(&toks[j - 1]) && is(toks, j + 1, T::Word) {
                name = format!("{name}:{}", toks[j + 1].v);
                j += 2;
            }
            Some((Sx::Word(name), j))
        }
        T::Str => Some((Sx::Str(t.v.clone()), i + 1)),
        _ => None,
    }
}

/// A setting's value from `i` to `end` (its tokens, as [`value_end`] cuts
/// them), as a datum.
fn read(toks: &[Tok], i: usize, end: usize) -> Option<Sx> {
    let t = toks.get(i)?;
    match t.t {
        T::Sym('#') => {
            let (d, j) = datum(toks, i + 1, end)?;
            (j == end).then_some(d)
        }
        T::Str => Some(Sx::Str(t.v.clone())),
        T::Num | T::Sym('-') => {
            // `2,2,3`: LilyPond's shorthand for a list of numbers.
            let (n, mut j) = number(toks, i, end)?;
            let mut list = vec![Sx::Num(n)];
            while j < end && is(toks, j, T::Sym(',')) {
                let (m, k) = number(toks, j + 1, end)?;
                list.push(Sx::Num(m));
                j = k;
            }
            Some(if list.len() == 1 { Sx::Num(n) } else { Sx::List(list) })
        }
        T::Word => Some(Sx::Word(t.v.clone())),
        _ => None,
    }
}

/// Where a value written `2,2,3` ends: [`value_end`] stops at its first number.
fn list_end(toks: &[Tok], mut j: usize) -> usize {
    while is(toks, j, T::Sym(',')) && is(toks, j + 1, T::Num) {
        j += 2;
    }
    j
}

/// A number a datum stands for: `1`, `#UP`, `#LEFT`, `(magstep -2)` …
fn as_num(d: &Sx) -> Option<f64> {
    match d {
        Sx::Num(n) => Some(*n),
        Sx::Word(w) => match w.as_str() {
            "UP" | "RIGHT" | "START" => Some(1.0),
            "DOWN" | "LEFT" | "STOP" => Some(-1.0),
            "CENTER" => Some(0.0),
            _ => None,
        },
        Sx::List(items) => match items.as_slice() {
            [Sx::Word(f), Sx::Num(n)] if f == "magstep" => Some(*n),
            [Sx::Word(f), Sx::Num(n)] if f == "ly:make-moment" => Some(*n),
            [Sx::Word(f), Sx::Num(a), Sx::Num(b)] if f == "ly:make-moment" && *b != 0.0 => Some(a / b),
            [Sx::Word(f), x] if f == "-" => as_num(x).map(|n| -n),
            _ => None,
        },
        _ => None,
    }
}

/// A size as a factor: `#2/3`, `#0.7`, `#(magstep -3)` — 2 to the -3/6.
fn factor(d: &Sx) -> Option<f64> {
    match d {
        Sx::List(items) => match items.as_slice() {
            [Sx::Word(f), Sx::Num(n)] if f == "magstep" => Some(2f64.powf(n / 6.0)),
            [Sx::Word(f), x] if f == "magstep" => as_num(x).map(|n| 2f64.powf(n / 6.0)),
            _ => as_num(d),
        },
        _ => as_num(d),
    }
}

/// The value of `key` a datum stands for, typed as the key wants it.
fn value_for(key: &str, d: &Sx) -> Option<Val> {
    let prop = key.rsplit('.').next().unwrap_or(key);
    Some(match prop {
        "extra-offset" | "positions" => match d {
            Sx::Pair(a, b) => Val::Pair(as_num(a)?, as_num(b)?),
            Sx::List(items) => match items.as_slice() {
                [Sx::Word(f), a, b] if f == "cons" => Val::Pair(as_num(a)?, as_num(b)?),
                _ => return None,
            },
            _ => return None,
        },
        "stencil" | "transparent" | "subdivideBeams" | "remove-empty" | "remove-first" => match d {
            Sx::Bool(b) => Val::Bool(*b),
            _ => return None,
        },
        "style" | "markFormatter" | "rehearsalMarkFormatter" => match d {
            Sx::Word(w) => Val::Sym(w.clone()),
            _ => return None,
        },
        "beatStructure" => match d {
            Sx::List(items) => Val::List(items.iter().map(as_num).collect::<Option<Vec<f64>>>()?),
            _ => return None,
        },
        "beamExceptions" => beam_exceptions(d)?,
        "break-visibility" => match d {
            Sx::List(items) if items.len() == 3 => Val::List(items.iter().map(|b| if *b == Sx::Bool(true) { 1.0 } else { 0.0 }).collect()),
            Sx::Word(w) => Val::List(match w.as_str() {
                "all-visible" => vec![1.0, 1.0, 1.0],
                "begin-of-line-visible" => vec![0.0, 0.0, 1.0],
                "end-of-line-visible" => vec![1.0, 0.0, 0.0],
                "end-of-line-invisible" => vec![0.0, 1.0, 1.0],
                "begin-of-line-invisible" => vec![1.0, 1.0, 0.0],
                "center-visible" => vec![0.0, 1.0, 0.0],
                "center-invisible" => vec![1.0, 0.0, 1.0],
                "all-invisible" => vec![0.0, 0.0, 0.0],
                _ => return None,
            }),
            _ => return None,
        },
        "barNumberVisibility" => match d {
            Sx::Word(w) => Val::Sym(w.clone()),
            Sx::List(items) => match items.as_slice() {
                [Sx::Word(f), n] if f == "every-nth-bar-number-visible" => Val::List(vec![as_num(n)?, 0.0]),
                [Sx::Word(f), n, m] if f == "modulo-bar-number-visible" => Val::List(vec![as_num(n)?, as_num(m)?]),
                _ => return None,
            },
            _ => return None,
        },
        "baseMoment" => Val::Num(as_num(d)? * WHOLE as f64),
        // A factor, `#(magstep -3)` the size six steps make smaller by half.
        "staff-space" | "thickness" => Val::Num(factor(d)?),
        "staff-staff-spacing" | "default-staff-staff-spacing" | "staffgroup-staff-spacing" => {
            let Sx::List(items) = d else { return None };
            let mut out = vec![f64::NAN; SPACING_KEYS.len()];
            for item in items {
                let Sx::Pair(k, v) = item else { return None };
                let Sx::Word(k) = &**k else { return None };
                if let Some(slot) = SPACING_KEYS.iter().position(|s| s == k) {
                    out[slot] = as_num(v)?;
                }
            }
            Val::List(out)
        }
        _ => Val::Num(as_num(d)?),
    })
}

/// `beamExceptions`: `#'()` none, or `#'((end . (((1 . 8) . (3 3 2)))))` —
/// the grouping of the eighths (or the first value given), in its eighths.
/// Kept as the grouping's lengths in ticks.
fn beam_exceptions(d: &Sx) -> Option<Val> {
    let Sx::List(items) = d else { return None };
    if items.is_empty() {
        return Some(Val::List(Vec::new()));
    }
    for item in items {
        let Sx::Pair(kind, rules) = item else { continue };
        if **kind != Sx::Word("end".into()) {
            continue;
        }
        let Sx::List(rules) = &**rules else { continue };
        for rule in rules {
            let Sx::Pair(moment, grouping) = rule else { continue };
            let Sx::Pair(n, dd) = &**moment else { continue };
            let unit = as_num(n)? / as_num(dd)?;
            let Sx::List(counts) = &**grouping else { continue };
            let lens = counts.iter().map(|c| as_num(c).map(|c| c * unit * WHOLE as f64)).collect::<Option<Vec<f64>>>()?;
            return Some(Val::List(lens));
        }
    }
    None
}

/// A rehearsal mark formatter: its frame and how it counts.
pub(crate) fn formatter(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "format-mark-letters" | "format-mark-alphabet" => ("", "letters"),
        "format-mark-numbers" => ("", "numbers"),
        "format-mark-box-letters" | "format-mark-box-alphabet" => ("box", "letters"),
        "format-mark-box-numbers" => ("box", "numbers"),
        "format-mark-circle-letters" | "format-mark-circle-alphabet" => ("circle", "letters"),
        "format-mark-circle-numbers" => ("circle", "numbers"),
        "format-mark-barnumbers" => ("", "bars"),
        "format-mark-box-barnumbers" => ("box", "bars"),
        "format-mark-circle-barnumbers" => ("circle", "bars"),
        _ => return None,
    })
}

// ---- The state while a staff is read -------------------------------------------

/// What `\partCombine`'s own commands say from where they are written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combine {
    /// `\partCombineAutomatic`.
    Automatic,
    /// `\partCombineApart`.
    Apart,
    /// `\partCombineChords`.
    Chords,
    /// `\partCombineUnisono`.
    Unisono,
    /// `\partCombineSoloI`.
    SoloI,
    /// `\partCombineSoloII`.
    SoloII,
}

/// The settings in force while a staff's music is read.
#[derive(Debug, Clone, Default)]
pub(crate) struct LookState {
    /// A voice's own settings.
    pub voice: Map,
    /// The staff's and the score's.
    pub staff: Map,
    /// `\once`'s, for the next note or rest.
    once: Map,
    /// What the merge of `staff` and `voice` gave last.
    cached: Option<Look>,
    /// `\partCombine…` written in a part: where, in ticks from its start.
    pub combine: Vec<(i64, Combine)>,
}

impl LookState {
    fn set(&mut self, ctx: Option<&str>, key: String, v: Val, once: bool) {
        self.cached = None;
        if key.ends_with("-spacing") {
            // A whole alist replaces what was set of its keys.
            let keys = format!("{key}.");
            for m in [&mut self.voice, &mut self.staff, &mut self.once] {
                m.retain(|k, _| !k.starts_with(&keys));
            }
        }
        if once || key == "currentBarNumber" {
            self.once.insert(key, v);
        } else if wide(ctx, &key) {
            self.voice.remove(&key);
            self.staff.insert(key, v);
        } else {
            self.voice.insert(key, v);
        }
    }

    fn unset(&mut self, ctx: Option<&str>, key: &str) {
        self.cached = None;
        if wide(ctx, key) {
            self.staff.remove(key);
        }
        self.voice.remove(key);
    }

    /// What is in force now.
    pub(crate) fn current(&mut self) -> Look {
        if self.once.is_empty() {
            if let Some(c) = &self.cached {
                return c.clone();
            }
        }
        let mut m = self.staff.clone();
        m.extend(self.voice.iter().map(|(k, v)| (k.clone(), v.clone())));
        let look = Look::of(m.clone());
        if self.once.is_empty() {
            self.cached = Some(look.clone());
            return look;
        }
        m.extend(self.once.iter().map(|(k, v)| (k.clone(), v.clone())));
        Look::of(m)
    }

    /// What a note or a rest takes: what is in force, `\once`'s with it,
    /// which it uses up.
    pub(crate) fn stamp(&mut self) -> Look {
        let look = self.current();
        self.once.clear();
        look
    }

    /// `\tweak`'s setting, for the next note.
    pub(crate) fn set_once(&mut self, key: String, v: Val) {
        self.set(None, key, v, true);
    }

    /// Set a voice's own settings back, at the end of the voice.
    pub(crate) fn restore_voice(&mut self, voice: Map) {
        self.cached = None;
        self.voice = voice;
    }
}

// ---- The commands --------------------------------------------------------------

fn dir_switch(word: &str) -> Option<(&'static str, Option<f64>)> {
    let (grob, rest) = [("phrasingSlur", "PhrasingSlur"), ("slur", "Slur"), ("tie", "Tie"), ("dynamic", "DynamicLineSpanner"), ("tuplet", "TupletBracket"), ("stem", "Stem")]
        .iter()
        .find_map(|(p, g)| word.strip_prefix(p).map(|r| (*g, r)))?;
    let d = match rest {
        "Up" => Some(1.0),
        "Down" => Some(-1.0),
        "Neutral" => None,
        _ => return None,
    };
    Some((grob, d))
}

/// A switch that stands for a setting: what it sets (none: reverts).
fn switch(word: &str) -> Option<Vec<(&'static str, Option<Val>)>> {
    if let Some((grob, d)) = dir_switch(word) {
        let key = match grob {
            "Slur" => "Slur.direction",
            "PhrasingSlur" => "PhrasingSlur.direction",
            "Tie" => "Tie.direction",
            "DynamicLineSpanner" => "DynamicLineSpanner.direction",
            "TupletBracket" => "TupletBracket.direction",
            _ => "Stem.direction",
        };
        return Some(vec![(key, d.map(Val::Num))]);
    }
    let sym = |s: &str| Some(Val::Sym(s.to_owned()));
    Some(match word {
        "numericTimeSignature" => vec![("TimeSignature.style", sym("numbered"))],
        "defaultTimeSignature" => vec![("TimeSignature.style", None)],
        "xNotesOn" => vec![("NoteHead.style", sym("cross"))],
        "xNotesOff" => vec![("NoteHead.style", None)],
        "harmonicsOn" => vec![("NoteHead.style", sym("harmonic"))],
        "harmonicsOff" => vec![("NoteHead.style", None)],
        "RemoveEmptyStaves" => vec![("VerticalAxisGroup.remove-empty", Some(Val::Bool(true)))],
        "RemoveAllEmptyStaves" => vec![("VerticalAxisGroup.remove-empty", Some(Val::Bool(true))), ("VerticalAxisGroup.remove-first", Some(Val::Bool(true)))],
        _ => return None,
    })
}

/// The partCombine commands.
fn combine_cmd(word: &str) -> Option<Combine> {
    Some(match word {
        "partCombineAutomatic" | "partcombineAutomatic" => Combine::Automatic,
        "partCombineApart" | "partcombineApart" => Combine::Apart,
        "partCombineChords" | "partcombineChords" => Combine::Chords,
        "partCombineUnisono" | "partcombineUnisono" => Combine::Unisono,
        "partCombineSoloI" | "partcombineSoloI" => Combine::SoloI,
        "partCombineSoloII" | "partcombineSoloII" => Combine::SoloII,
        _ => return None,
    })
}

/// A path's words, `Staff.TimeSignature.style` or `Stem #'direction`.
fn words(toks: &[Tok], i: usize, end: usize) -> Vec<String> {
    toks[i..end.min(toks.len())].iter().filter(|t| t.t == T::Word).map(|t| t.v.clone()).collect()
}

/// A setting at `i` honoured into `state` — `\override`, `\revert`, `\set`,
/// `\unset`, `\omit`, `\hide`, `\undo`, `\once` before one of them, a switch
/// standing for one — and the token after it; none when it is not one the
/// engraver draws (the caller passes it over, with its warning). `elapsed`
/// is how far the music being read has come, for `\partCombine…`.
pub(crate) fn command(toks: &[Tok], i: usize, state: &mut LookState, elapsed: i64) -> Result<Option<usize>, ScoreError> {
    setting(toks, i, state, false, elapsed)
}

fn setting(toks: &[Tok], i: usize, state: &mut LookState, once: bool, elapsed: i64) -> Result<Option<usize>, ScoreError> {
    let Some(t) = toks.get(i) else { return Ok(None) };
    if t.t == T::Word {
        // `fontSize = #-3` in a `\with`: a `\set` without the word.
        let j = path_end(toks, i);
        if !is(toks, j, T::Sym('=')) {
            return Ok(None);
        }
        let path = words(toks, i, j);
        let Some((ctx, key)) = split(&path, false) else { return Ok(None) };
        return assign(toks, j, state, ctx.as_deref(), key, once);
    }
    if t.t != T::Cmd {
        return Ok(None);
    }
    match t.v.as_str() {
        "magnifyStaff" => {
            // `\magnifyStaff #2/3`: the staff's lines that much closer and
            // thinner, and its music that much smaller (score #407).
            let end = value_end(toks, i + 1)?;
            let Some(r) = read(toks, i + 1, end).as_ref().and_then(factor).filter(|r| (0.25..=4.0).contains(r)) else { return Ok(None) };
            state.set(Some("Staff"), "StaffSymbol.staff-space".into(), Val::Num(r), false);
            state.set(Some("Staff"), "StaffSymbol.thickness".into(), Val::Num(r), false);
            state.set(Some("Staff"), "fontSize".into(), Val::Num(6.0 * r.log2()), false);
            Ok(Some(end))
        }
        "once" | "single" => setting(toks, i + 1, state, true, elapsed),
        "temporary" => setting(toks, i + 1, state, once, elapsed),
        "override" => {
            let j = path_end(toks, i + 1);
            if !is(toks, j, T::Sym('=')) {
                return Ok(None);
            }
            let path = words(toks, i + 1, j);
            let Some((ctx, key)) = split(&path, true) else { return Ok(None) };
            assign(toks, j, state, ctx.as_deref(), key, once)
        }
        "set" => {
            let j = path_end(toks, i + 1);
            if !is(toks, j, T::Sym('=')) {
                return Ok(None);
            }
            let path = words(toks, i + 1, j);
            let Some((ctx, key)) = split(&path, false) else { return Ok(None) };
            assign(toks, j, state, ctx.as_deref(), key, once)
        }
        "revert" | "unset" => {
            let j = path_end(toks, i + 1);
            let path = words(toks, i + 1, j);
            let Some((ctx, key)) = split(&path, t.v == "revert") else { return Ok(None) };
            let probe = known_probe(&key);
            if !probe {
                return Ok(None);
            }
            state.unset(ctx.as_deref(), &key);
            Ok(Some(j))
        }
        "omit" | "hide" => {
            let j = path_end(toks, i + 1);
            let path = words(toks, i + 1, j);
            let Some((ctx, grob)) = split(&path, false) else { return Ok(None) };
            if !OMITTABLE.contains(&grob.as_str()) {
                return Ok(None);
            }
            let (key, v) = if t.v == "omit" { (format!("{grob}.stencil"), Val::Bool(false)) } else { (format!("{grob}.transparent"), Val::Bool(true)) };
            state.set(ctx.as_deref(), key, v, once);
            Ok(Some(j))
        }
        "undo" => {
            // `\undo \omit Staff.TimeSignature`: what it undoes, reverted.
            let Some(inner) = toks.get(i + 1).filter(|n| n.t == T::Cmd && (n.v == "omit" || n.v == "hide" || n.v == "override" || n.v == "set")) else { return Ok(None) };
            let j = path_end(toks, i + 2);
            let path = words(toks, i + 2, j);
            let grob_path = inner.v != "set";
            let Some((ctx, key)) = split(&path, grob_path && inner.v == "override") else { return Ok(None) };
            let key = match inner.v.as_str() {
                "omit" => format!("{key}.stencil"),
                "hide" => format!("{key}.transparent"),
                _ => key,
            };
            if !known_probe(&key) {
                return Ok(None);
            }
            state.unset(ctx.as_deref(), &key);
            let end = if is(toks, j, T::Sym('=')) { list_end(toks, value_end(toks, j + 1)?) } else { j };
            Ok(Some(end))
        }
        w => {
            if let Some(mode) = combine_cmd(w) {
                state.combine.push((elapsed, mode));
                return Ok(Some(i + 1));
            }
            let Some(sets) = switch(w) else { return Ok(None) };
            for (key, v) in sets {
                match v {
                    Some(v) => state.set(None, key.to_owned(), v, once),
                    None => state.unset(None, key),
                }
            }
            Ok(Some(i + 1))
        }
    }
}

/// A key the engraver draws, whatever its value: what `\revert` may take back.
fn known_probe(key: &str) -> bool {
    let probes = [
        Val::Num(0.0),
        Val::Num(1.0),
        Val::Pair(0.0, 0.0),
        Val::Bool(false),
        Val::Bool(true),
        Val::List(vec![0.0, 0.0, 0.0]),
        Val::List(vec![0.0; SPACING_KEYS.len()]),
        Val::Sym("numbered".into()),
        Val::Sym("cross".into()),
        Val::Sym("format-mark-numbers".into()),
    ];
    probes.iter().any(|v| honoured(key, v))
}

/// `= value` at `j` for `key`: set when honoured.
fn assign(toks: &[Tok], j: usize, state: &mut LookState, ctx: Option<&str>, key: String, once: bool) -> Result<Option<usize>, ScoreError> {
    let end = list_end(toks, value_end(toks, j + 1)?);
    let Some(v) = read(toks, j + 1, end).and_then(|d| value_for(&key, &d)) else { return Ok(None) };
    if !honoured(&key, &v) {
        return Ok(None);
    }
    state.set(ctx, key, v, once);
    Ok(Some(end))
}

/// A `\tweak` at `i`: `\tweak [Grob.]property value`, before what it
/// tweaks. Its grob, when not written, is the one that follows: a note's
/// head, words, an articulation, a dynamic, a slur. Answers the setting and
/// the token after the value; none when it is not one the engraver draws.
pub(crate) fn tweak(toks: &[Tok], i: usize) -> Result<Option<(String, Val, usize)>, ScoreError> {
    // `Grob.property`, or the older `#'property`; the value after it.
    let mut j = i + 1;
    if is(toks, j, T::Sym('#')) && is(toks, j + 1, T::Sym('\'')) && is(toks, j + 2, T::Word) {
        j += 3;
    } else {
        while is(toks, j, T::Word) {
            j += 1;
            if is(toks, j, T::Sym('.')) && is(toks, j + 1, T::Word) {
                j += 1;
            } else {
                break;
            }
        }
    }
    let path = words(toks, i + 1, j);
    let end = list_end(toks, value_end(toks, j)?);
    let key = match path.len() {
        1 => {
            let Some(grob) = tweaked(toks, end) else { return Ok(None) };
            format!("{grob}.{}", path[0])
        }
        2 => path.join("."),
        _ => return Ok(None),
    };
    let Some(v) = read(toks, j, end).and_then(|d| value_for(&key, &d)) else { return Ok(None) };
    if !honoured(&key, &v) {
        return Ok(None);
    }
    Ok(Some((key, v, end)))
}

/// The grob a `\tweak` meets at `k`.
fn tweaked(toks: &[Tok], mut k: usize) -> Option<&'static str> {
    // `-\tweak … -"words"`, `^\tweak … ->`: past the sign.
    if matches!(toks.get(k).map(|t| t.t), Some(T::Sym('-' | '^' | '_'))) {
        k += 1;
    }
    let t = toks.get(k)?;
    Some(match t.t {
        T::Str => "TextScript",
        T::Cmd if t.v == "markup" => "TextScript",
        T::Cmd if crate::parse::DYNAMICS.contains(&t.v.as_str()) => "DynamicText",
        T::Cmd if crate::parse::SCRIPTS.contains(&t.v.as_str()) => "Script",
        T::Cmd if t.v == "(" => "PhrasingSlur",
        T::Cmd if t.v == "mark" => "RehearsalMark",
        T::Sym('.' | '>' | '!') => "Script",
        T::Sym('(') => "Slur",
        T::Sym('~') => "Tie",
        T::Num => "Fingering",
        T::Word | T::Sym('<') => "NoteHead",
        _ => return None,
    })
}

/// A look with one setting more.
pub(crate) fn with_one(look: &Look, key: String, v: Val) -> Look {
    let mut m = look.map().cloned().unwrap_or_default();
    m.insert(key, v);
    Look::of(m)
}

// ---- \with and \layout -----------------------------------------------------------

/// A setting of a `\with { }` or a `\layout`'s `\context { }` at `i`,
/// honoured into `map`; none when it is not one the engraver draws.
pub(crate) fn context_setting(toks: &[Tok], i: usize, map: &mut Map) -> Result<Option<usize>, ScoreError> {
    let mut state = LookState::default();
    let got = setting(toks, i, &mut state, false, 0)?;
    if got.is_some() {
        map.extend(state.staff);
        map.extend(state.voice);
        map.extend(state.once);
    }
    Ok(got)
}

thread_local! {
    /// What the `\layout` blocks of the source being read set, by context.
    static LAYOUT: std::cell::RefCell<Vec<(String, Map)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A new source: no `\layout` read yet.
pub(crate) fn reset() {
    LAYOUT.with(|l| l.borrow_mut().clear());
}

/// A `\layout`'s `\context { \Name … }`'s honoured settings.
pub(crate) fn layout_context(name: &str, map: Map) {
    if !map.is_empty() {
        LAYOUT.with(|l| l.borrow_mut().push((name.to_owned(), map)));
    }
}

fn layout_of(names: &[&str]) -> Map {
    let mut out = Map::new();
    LAYOUT.with(|l| {
        for (name, m) in l.borrow().iter() {
            if names.contains(&name.as_str()) {
                out.extend(m.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }
    });
    out
}

/// The settings a staff's notes take where they set none: the `\layout`'s,
/// its groups', its own `\with`'s — and those of each note on them. A
/// note's `fontSize` and stem direction go into its size and stem.
pub(crate) fn settle(staves: &mut [crate::parse::Staff], groups: &[crate::parse::Group]) {
    use crate::parse::{GroupKind, StaffKind};
    for (s, stf) in staves.iter_mut().enumerate() {
        let mut names: Vec<&str> = vec!["Score", "Lyrics"];
        let mut outer: Vec<&crate::parse::Group> = groups.iter().filter(|g| g.first <= s && s <= g.last).collect();
        outer.sort_by_key(|g| g.depth);
        for g in outer {
            match g.kind {
                GroupKind::Brace => names.extend(["PianoStaff", "GrandStaff"]),
                GroupKind::Bracket => names.push("StaffGroup"),
                GroupKind::Choir => names.push("ChoirStaff"),
                GroupKind::None => {}
            }
        }
        names.push(match stf.kind {
            StaffKind::Drums => "DrumStaff",
            StaffKind::Tab => "TabStaff",
            StaffKind::ChordNames => "ChordNames",
            _ => "Staff",
        });
        names.push(match stf.kind {
            StaffKind::Drums => "DrumVoice",
            StaffKind::Tab => "TabVoice",
            _ => "Voice",
        });
        let mut base = Map::new();
        for n in &names {
            base.extend(layout_of(&[n]));
        }
        base.extend(stf.props.look.iter().map(|(k, v)| (k.clone(), v.clone())));
        let mut cache: Vec<(*const Map, Look)> = Vec::new();
        let plain = Look::of(base.clone());
        for ev in &mut stf.events {
            settle_one(ev, &base, &plain, &mut cache);
            for g in ev.graces.iter_mut().chain(ev.after_graces.iter_mut()) {
                settle_one(g, &base, &plain, &mut cache);
            }
        }
        // A staff drawn smaller (`StaffSymbol.staff-space`, `\magnifyStaff`):
        // everything on it is drawn at its scale, so a note's size is its own
        // against the staff's — the `fontSize` that goes with the staff's
        // lines (or a cue voice's -3 on a cue-sized staff) is the staff's
        // own size, 0 (score #407).
        if let Some(scale) = staff_scale(&stf.events) {
            let steps = 6.0 * scale.log2();
            for ev in &mut stf.events {
                rescale(ev, steps);
                for g in ev.graces.iter_mut().chain(ev.after_graces.iter_mut()) {
                    rescale(g, steps);
                }
            }
        }
        // The style of a metre no `\time` writes: the staff's first.
        let first_note = stf.events.iter().position(|e| matches!(e.k, Kind::Note | Kind::Rest)).unwrap_or(stf.events.len());
        let timed = stf.events[..first_note].iter().any(|e| e.k == Kind::Time);
        let look = stf.events.get(first_note).map(|e| e.look.clone()).unwrap_or_else(|| plain.clone());
        let parts = time_style(&look);
        if !timed && parts != 0 && matches!(stf.kind, StaffKind::Notes | StaffKind::Drums | StaffKind::Ossia) {
            let mut t = Ev { k: Kind::Time, time: (4, 4), look, v: crate::parse::DEFAULT_METRE.into(), at: stf.events.first().map_or(0, |e| e.at), ..Ev::default() };
            t.time_parts[3] = parts;
            stf.events.insert(0, t);
        }
    }
}

/// How much smaller (or larger) a staff is drawn: its first note's or
/// rest's `StaffSymbol.staff-space` — its `\with`'s, a `\layout`'s, one
/// written before its music. None for a staff of the usual size.
pub fn staff_scale(events: &[Ev]) -> Option<f64> {
    let first = events.iter().find(|e| matches!(e.k, Kind::Note | Kind::Rest)).or_else(|| events.first())?;
    first.look.num("StaffSymbol.staff-space").filter(|s| (*s - 1.0).abs() > 1e-6 && (0.25..=4.0).contains(s))
}

/// A note's size against its staff's, `steps` the staff's own.
fn rescale(ev: &mut Ev, steps: f64) {
    // The `fontSize` it took, unrounded where it is the one it took.
    let set = ev.look.num("NoteHead.font-size").or_else(|| ev.look.num("fontSize"));
    let own = match set {
        Some(f) if !ev.cue && f.round().clamp(-12.0, 12.0) as i8 == ev.size => f,
        _ => f64::from(ev.size),
    };
    ev.size = (own - steps).round().clamp(-12.0, 12.0) as i8;
}

fn settle_one(ev: &mut Ev, base: &Map, plain: &Look, cache: &mut Vec<(*const Map, Look)>) {
    ev.look = match ev.look.map() {
        None => plain.clone(),
        Some(own) => {
            let key: *const Map = own;
            match cache.iter().find(|c| c.0 == key) {
                Some(c) => c.1.clone(),
                None => {
                    let mut m = base.clone();
                    m.extend(own.iter().map(|(k, v)| (k.clone(), v.clone())));
                    let l = Look::of(m);
                    cache.push((key, l.clone()));
                    l
                }
            }
        }
    };
    let look = ev.look.clone();
    // Words written with `-`: the side TextScript.direction says, else
    // under the note — a markup over it, as the Soli engraver set it.
    for w in &mut ev.texts {
        if w.dir == 0 {
            w.dir = look.side("TextScript").unwrap_or(if w.runs.is_empty() { 1 } else { -1 });
        }
    }
    if look.is_empty() {
        return;
    }
    if ev.stem_dir == 0 {
        if let Some(d) = look.dir("Stem") {
            ev.stem_dir = d as i8;
        }
    }
    if ev.size == 0 && !ev.cue {
        let steps = look.num("NoteHead.font-size").or_else(|| look.num("fontSize"));
        if let Some(fs) = steps {
            ev.size = fs.round().clamp(-12.0, 12.0) as i8;
        }
    }
    if ev.k == Kind::Mark && ev.texts.first().is_some_and(|w| w.dir < 0) {
        mark_settle(ev, &look);
    }
    if ev.k == Kind::Time {
        ev.time_parts[3] = time_style(&look).max(ev.time_parts[3]);
    }
}

/// How a metre is drawn, in its last `time_parts` slot: 0 as LilyPond
/// does by default, else one of these.
pub(crate) const TIME_NUMBERED: u8 = 255;
/// `TimeSignature.style = #'single-digit`: the numerator alone.
pub(crate) const TIME_SINGLE: u8 = 254;
/// `\omit Staff.TimeSignature`: none.
pub(crate) const TIME_OMITTED: u8 = 253;

pub(crate) fn time_style(look: &Look) -> u8 {
    if look.omitted("TimeSignature") {
        return TIME_OMITTED;
    }
    match look.sym("TimeSignature.style") {
        Some("numbered") => TIME_NUMBERED,
        Some("single-digit") => TIME_SINGLE,
        _ => 0,
    }
}

// ---- Using them -------------------------------------------------------------------

/// The head glyph a note's style gives it, for `base`; none for the default.
pub(crate) fn styled_head(style: &str, base: i64) -> Option<&'static str> {
    Some(match (style, base) {
        ("cross", 1) => "noteheadXWhole",
        ("cross", 2) => "noteheadXHalf",
        ("cross", _) => "noteheadXBlack",
        ("xcircle", _) => "noteheadCircleX",
        ("diamond", 1) => "noteheadDiamondWhole",
        ("diamond", 2) => "noteheadDiamondHalf",
        ("diamond", _) => "noteheadDiamondBlack",
        ("triangle", 1) => "noteheadTriangleUpWhole",
        ("triangle", 2) => "noteheadTriangleUpHalf",
        ("triangle", _) => "noteheadTriangleUpBlack",
        ("slash", 1) => "noteheadSlashWhiteWhole",
        ("slash", 2) => "noteheadSlashWhiteHalf",
        ("slash", _) => "noteheadSlashVerticalEnds",
        ("harmonic", _) => "noteheadDiamondWhite",
        ("harmonic-black", _) => "noteheadDiamondBlackWide",
        ("harmonic-mixed", 1 | 2) => "noteheadDiamondWhite",
        ("harmonic-mixed", _) => "noteheadDiamondBlackWide",
        _ => return None,
    })
}

/// A note's head glyph by its `NoteHead.style`, none for the default.
pub(crate) fn head_of(ev: &Ev) -> Option<&'static str> {
    ev.look.sym("NoteHead.style").and_then(|s| styled_head(s, ev.base))
}

/// Whether bar `number`'s number is printed, by `barNumberVisibility` and
/// `BarNumber.break-visibility` — at the start of a line, or in it. As
/// LilyPond's defaults have it: at a line's start only, never the first bar.
pub(crate) fn bar_number_shown(look: &Look, number: i64, line_start: bool, first_bar: bool) -> bool {
    if look.omitted("BarNumber") {
        return false;
    }
    let vis = match look.get("BarNumber.break-visibility") {
        Some(Val::List(v)) if v.len() == 3 => v.clone(),
        _ => vec![0.0, 0.0, 1.0],
    };
    let place = if line_start { vis[2] } else { vis[1] };
    if place == 0.0 {
        return false;
    }
    match look.get("barNumberVisibility") {
        Some(Val::Sym(s)) if s == "all-bar-numbers-visible" => true,
        Some(Val::Sym(s)) if s == "first-bar-number-invisible-and-no-parenthesized-bar-numbers" || s == "first-bar-number-invisible" || s == "first-bar-number-invisible-save-broken-bars" => {
            !first_bar
        }
        Some(Val::List(v)) if v.len() == 2 && v[0] >= 1.0 => {
            let (n, m) = (v[0] as i64, v[1] as i64);
            (number - m).rem_euclid(n) == 0
        }
        _ => !first_bar,
    }
}

/// `\set Score.currentBarNumber`: the bars counted from the one it is
/// written in, as it says.
pub(crate) fn renumber(staves: &mut [Vec<crate::measure::Bar>]) {
    let count = staves.first().map_or(0, Vec::len);
    let mut shift: Option<i64> = None;
    for b in 0..count {
        let set = staves.iter().find_map(|bars| bars.get(b).and_then(|bar| bar.items.iter().find_map(|ev| ev.look.num("currentBarNumber"))));
        if let Some(n) = set {
            let own = staves[0][b].number;
            shift = Some(n as i64 - own);
        }
        if let Some(d) = shift {
            for bars in staves.iter_mut() {
                if let Some(bar) = bars.get_mut(b) {
                    bar.number += d;
                }
            }
        }
        // A movement counts its bars again.
        if b + 1 < count && staves[0][b + 1].fresh {
            shift = None;
        }
    }
}

/// The beams' windows a bar's settings give: the lengths, in ticks, of
/// the groups the beams end at (`beatStructure` in `baseMoment`s, or the
/// grouping `beamExceptions` gives); none for the metre's own.
pub(crate) fn beam_groups(look: &Look, time: (i64, i64)) -> Option<Vec<i64>> {
    let (num, den) = time;
    let exceptions = match look.get("beamExceptions") {
        Some(Val::List(l)) if !l.is_empty() => return Some(l.iter().map(|t| *t as i64).filter(|t| *t > 0).collect()).filter(|v: &Vec<i64>| !v.is_empty()),
        Some(Val::List(_)) => false,
        _ => true,
    };
    let base = look.num("baseMoment").map(|t| t as i64);
    let structure = match look.get("beatStructure") {
        Some(Val::List(l)) => Some(l.clone()),
        _ => None,
    };
    if base.is_none() && structure.is_none() {
        return None;
    }
    // LilyPond's own exceptions still beam a 4/4, a 3/4 or a 2/2's eighths
    // over them, unless they are cleared.
    if exceptions && [(4, 4), (3, 4), (2, 2), (6, 8), (9, 8), (12, 8)].contains(&time) {
        return None;
    }
    let unit = base.unwrap_or(WHOLE / den).max(1);
    let counts = structure.unwrap_or_else(|| {
        let length = num * WHOLE / den;
        vec![1.0; (length / unit).max(1) as usize]
    });
    let out: Vec<i64> = counts.iter().map(|c| (*c as i64) * unit).filter(|t| *t > 0).collect();
    (!out.is_empty()).then_some(out)
}

/// The window a moment `pos` of a bar falls in, by groups' lengths.
pub(crate) fn window_in(groups: &[i64], pos: i64) -> i64 {
    let mut at = 0;
    let mut k = 0;
    loop {
        let len = groups[k % groups.len()];
        if pos < at + len {
            return k as i64;
        }
        at += len;
        k += 1;
        if k > 10_000 {
            return k as i64;
        }
    }
}

/// `subdivideBeams`: the step its beams break at, in ticks, and how many
/// beams go on over the break.
pub(crate) fn subdivision(look: &Look, time: (i64, i64)) -> Option<(i64, i64)> {
    if look.flag("subdivideBeams") != Some(true) {
        return None;
    }
    let step = look.num("baseMoment").map_or(WHOLE / time.1.max(1), |t| t as i64).max(1);
    let value = WHOLE / step;
    let keep = crate::engrave::flags(value).max(1);
    Some((step, keep))
}

/// The mark a `\mark \default` or `\mark n` says: its number written as
/// the formatter in force writes it, and its frame.
pub(crate) fn mark_text(look: &Look, n: i64, bar: i64) -> (String, &'static str) {
    let name = look.sym("rehearsalMarkFormatter").or_else(|| look.sym("markFormatter")).unwrap_or("format-mark-letters");
    let (frame, count) = formatter(name).unwrap_or(("", "letters"));
    let text = match count {
        "numbers" => n.to_string(),
        "bars" => bar.to_string(),
        _ => crate::parse::letters(n),
    };
    (text, frame)
}

// ---- Rehearsal marks ----------------------------------------------------------

/// How a rehearsal mark is drawn, beyond its words: its frame, how it is
/// aligned on its bar line and moved. Carried to the placing in a first
/// run the mark's words begin with, as a bar keeps a mark's words alone.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MarkStyle {
    /// "", "box" or "circle".
    pub frame: String,
    /// `self-alignment-X`: -1 its left at the bar line, 0 centred.
    pub align: Option<f64>,
    /// `X-offset`, from the bar line.
    pub x: Option<f64>,
    /// `Y-offset`, from the middle line.
    pub y: Option<f64>,
    /// `extra-offset`, on the page: x right, y down.
    pub extra: (f64, f64),
    /// `padding` and `outside-staff-padding`, over what is under it.
    pub pad: Option<f64>,
    /// `format-mark-barnumbers`: the number of the bar it stands at.
    pub bars: bool,
}

/// What starts a run carrying a [`MarkStyle`].
const MARK_STYLE: char = '\u{1}';

fn opt(v: Option<f64>) -> String {
    v.map_or(String::new(), |v| v.to_string())
}

fn mark_settle(ev: &mut Ev, look: &Look) {
    let formatter_set = look.sym("rehearsalMarkFormatter").or_else(|| look.sym("markFormatter")).is_some();
    let pad = match (look.num("RehearsalMark.padding"), look.num("RehearsalMark.outside-staff-padding")) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.8) + b.unwrap_or(0.0)),
    };
    let mut style = MarkStyle {
        frame: String::new(),
        align: look.num("RehearsalMark.self-alignment-X"),
        x: look.num("RehearsalMark.X-offset"),
        y: look.num("RehearsalMark.Y-offset"),
        extra: look.extra("RehearsalMark"),
        pad,
        bars: false,
    };
    if formatter_set && ev.count > 0 {
        let (text, frame) = mark_text(look, ev.count, 0);
        style.frame = frame.to_owned();
        style.bars = look.sym("rehearsalMarkFormatter").or_else(|| look.sym("markFormatter")).is_some_and(|f| f.contains("barnumbers"));
        if let Some(w) = ev.texts.first_mut() {
            w.runs = vec![crate::parse::Run::words(text, crate::glyphs::Weight::Bold)];
        }
    }
    if style == MarkStyle::default() {
        return;
    }
    let code = format!("{MARK_STYLE}{}|{}|{}|{}|{}|{}|{}|{}", style.frame, opt(style.align), opt(style.x), opt(style.y), style.extra.0, style.extra.1, opt(style.pad), style.bars);
    if let Some(w) = ev.texts.first_mut() {
        let mut first = crate::parse::Run::words(code, crate::glyphs::Weight::Regular);
        first.gap = false;
        w.runs.insert(0, first);
    }
}

/// A mark's style and its words without the run carrying it.
pub(crate) fn mark_style(runs: &[crate::parse::Run]) -> (MarkStyle, Vec<crate::parse::Run>) {
    let Some(code) = runs.first().and_then(|r| r.text.strip_prefix(MARK_STYLE)) else { return (MarkStyle::default(), runs.to_vec()) };
    let f: Vec<&str> = code.split('|').collect();
    let num = |k: usize| f.get(k).and_then(|v| v.parse::<f64>().ok());
    let style = MarkStyle {
        frame: f.first().map_or(String::new(), |v| (*v).to_owned()),
        align: num(1),
        x: num(2),
        y: num(3),
        extra: (num(4).unwrap_or(0.0), num(5).unwrap_or(0.0)),
        pad: num(6),
        bars: f.get(7).is_some_and(|v| *v == "true"),
    };
    (style, runs[1..].to_vec())
}
