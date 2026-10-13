//! A staff's events cut into bars, with what each note's heads print
//! (`eui_builders_score.sl`, "Bars").

use std::collections::HashMap;

use crate::parse::{modulo, refuse, Clef, Ev, Head, Kind};
use crate::{ScoreError, WHOLE};

/// What changes at the start of a bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changes {
    /// The clef.
    pub clef: bool,
    /// The key, from `key_from`.
    pub key: bool,
    /// The key before.
    pub key_from: i64,
    /// The metre.
    pub time: bool,
}

impl Changes {
    pub(crate) fn any(&self) -> bool {
        self.clef || self.key || self.time
    }
}

/// A beam over notes of one bar.
#[derive(Debug, Clone, Default)]
pub struct Beam {
    /// Its notes, by their index in the bar.
    pub notes: Vec<usize>,
    /// Stems up.
    pub up: bool,
    /// Its outer edge at its first stem.
    pub x0: f64,
    /// … at its last.
    pub x1: f64,
    /// Its outer edge's y at `x0`.
    pub y0: f64,
    /// … at `x1`.
    pub y1: f64,
    /// How many beams the most it has.
    pub n: i64,
}

/// A bar's opening, as the width of a line's start needs it: one staff's,
/// or each staff's of a shared bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening {
    /// The clef.
    pub clef: Clef,
    /// The key.
    pub fifths: i64,
    /// The metre.
    pub time: (i64, i64),
    /// What changes here.
    pub changes: Changes,
}

/// A bar.
#[derive(Debug, Clone, Default)]
pub struct Bar {
    /// Its clef.
    pub clef: Clef,
    /// Its key.
    pub fifths: i64,
    /// Its metre.
    pub time: (i64, i64),
    /// Its notes and rests — or, in a shared bar, its columns.
    pub items: Vec<Ev>,
    /// Its bar line.
    pub bar: String,
    /// What changes at its start.
    pub changes: Changes,
    /// How long it is, in ticks.
    pub length: i64,
    /// Bars a gathered rest in it stands for.
    pub span: Option<i64>,
    /// The ending it is in.
    pub volta_in: Option<String>,
    /// The first bar of an ending.
    pub volta_first: bool,
    /// The last.
    pub volta_last: bool,
    /// An ending that goes back.
    pub volta_closed: bool,
    /// A line or page break after it: "line" or "page".
    pub brk: Option<String>,
    /// A repeat starts here.
    pub start_repeat: bool,
    /// Its number.
    pub number: i64,
    /// The piece's first bar.
    pub first: bool,
    /// The bar line drawn where a line break splits a repeat sign.
    pub bar_drawn: Option<String>,
    /// A start repeat drawn at the start of its line.
    pub open_repeat: Option<String>,
    /// The piece opens on a start repeat (`\bar ".|:"` before any note).
    pub open_bar: bool,
    /// Where.
    pub open_repeat_x: Option<f64>,
    /// Its start.
    pub x0: f64,
    /// Its last note's room's end.
    pub x1: f64,
    /// Its beams.
    pub beams: Vec<Beam>,
    /// In a shared bar, each staff's opening.
    pub staff_openings: Vec<Opening>,
}

impl Bar {
    pub(crate) fn opening(&self) -> Opening {
        Opening { clef: self.clef, fifths: self.fifths, time: self.time, changes: self.changes }
    }

    pub(crate) fn drawn(&self) -> &str {
        self.bar_drawn.as_deref().unwrap_or(&self.bar)
    }
}

/// The alteration a key gives each step, c = 0 … b = 6.
pub fn key_alters(fifths: i64) -> [i64; 7] {
    let mut out = [0; 7];
    let sharps = [3, 0, 4, 1, 5, 2, 6];
    let flats = [6, 2, 5, 1, 4, 0, 3];
    for k in 0..fifths.unsigned_abs().min(7) as usize {
        if fifths > 0 {
            out[sharps[k]] = 1;
        } else {
            out[flats[k]] = -1;
        }
    }
    out
}

/// The middle line's diatonic number under each clef.
pub fn clef_mid(clef: Clef) -> i64 {
    match clef {
        Clef::Treble => 13,
        Clef::Bass => 1,
        Clef::Alto => 7,
    }
}

fn kind_name(k: &Kind) -> &'static str {
    match k {
        Kind::Clef => "clef",
        Kind::Key => "key",
        _ => "time",
    }
}

/// The bars of a staff.
pub fn measures(events: &[Ev]) -> Result<Vec<Bar>, ScoreError> {
    let mut clef = Clef::Treble;
    let mut fifths = 0;
    let (mut tn, mut td) = (4, 4);
    let mut bars: Vec<Bar> = Vec::new();
    let mut cur: Option<usize> = None;
    let mut pos = 0;
    let mut seen: HashMap<i64, i64> = HashMap::new();
    let mut alters = key_alters(0);
    let mut tied: HashMap<i64, i64> = HashMap::new();
    let mut tempo = None;
    let mut volta_open: Option<String> = None;
    let mut tuplet_bar: HashMap<i64, usize> = HashMap::new();
    let compress = events.iter().any(|ev| ev.k == Kind::Compress);
    let mut flat: Vec<Ev> = Vec::new();
    for ev in events {
        if ev.k == Kind::Rest && ev.count > 1 && !compress {
            for _ in 0..ev.count {
                flat.push(Ev { count: 1, ..ev.clone() });
            }
        } else {
            flat.push(ev.clone());
        }
    }
    for mut ev in flat {
        if ev.k == Kind::Tempo {
            tempo = ev.tempo.clone();
            continue;
        }
        if ev.k == Kind::Compress {
            continue;
        }
        let ci = match cur {
            Some(ci) => ci,
            None => {
                let b = Bar { clef, fifths, time: (tn, td), bar: "|".into(), length: tn * WHOLE / td, volta_in: volta_open.clone(), ..Bar::default() };
                bars.push(b);
                pos = 0;
                seen.clear();
                let ci = bars.len() - 1;
                cur = Some(ci);
                ci
            }
        };
        match ev.k {
            Kind::Break => {
                if pos > 0 {
                    return refuse("a \\break goes at a bar line", ev.at);
                }
                if bars.len() < 2 {
                    return refuse("a \\break before any music", ev.at);
                }
                let n = bars.len();
                let ending = &mut bars[n - 2];
                ending.brk = Some(if ev.page || ending.brk.as_deref() == Some("page") { "page".into() } else { "line".into() });
                continue;
            }
            Kind::RepeatStart => {
                if pos > 0 {
                    return refuse("a repeat starts at a bar line", ev.at);
                }
                bars[ci].start_repeat = true;
                continue;
            }
            Kind::VoltaStart => {
                if pos > 0 {
                    return refuse("an alternative starts at a bar line", ev.at);
                }
                volta_open = Some(ev.v.clone());
                bars[ci].volta_in = volta_open.clone();
                bars[ci].volta_first = true;
                continue;
            }
            Kind::VoltaEnd => {
                let ended_before = pos == 0 && bars.len() > 1;
                let n = bars.len();
                let ending = if ended_before { &mut bars[n - 2] } else { &mut bars[ci] };
                ending.volta_last = true;
                ending.volta_closed = ev.closed;
                volta_open = None;
                if ended_before {
                    bars[ci].volta_in = None;
                }
                continue;
            }
            Kind::RepeatEnd => {
                if pos == 0 && bars.len() > 1 {
                    let n = bars.len();
                    bars[n - 2].bar = ":|.".into();
                } else if pos > 0 {
                    bars[ci].bar = ":|.".into();
                    bars[ci].length = pos;
                    cur = None;
                } else {
                    return refuse("an end repeat before any music", ev.at);
                }
                continue;
            }
            Kind::Clef | Kind::Key | Kind::Time => {
                if pos > 0 {
                    return refuse(format!("\\{} in the middle of a bar", kind_name(&ev.k)), ev.at);
                }
                let bar = &mut bars[ci];
                match ev.k {
                    Kind::Clef => {
                        clef = ev.clef;
                        bar.clef = clef;
                        bar.changes.clef = true;
                    }
                    Kind::Key => {
                        bar.changes.key_from = fifths;
                        fifths = ev.fifths;
                        bar.fifths = fifths;
                        bar.changes.key = true;
                    }
                    _ => {
                        tn = ev.time.0;
                        td = ev.time.1;
                        bar.time = (tn, td);
                        bar.length = tn * WHOLE / td;
                        bar.changes.time = true;
                    }
                }
                alters = key_alters(fifths);
                continue;
            }
            Kind::Bar => {
                if pos == 0 && bars.len() > 1 {
                    let n = bars.len();
                    bars[n - 2].bar = ev.v.clone();
                } else if pos > 0 {
                    bars[ci].bar = ev.v.clone();
                    bars[ci].length = pos;
                    cur = None;
                } else if ev.v == ".|:" {
                    // A start repeat before the first note: drawn at the
                    // start of the first line, after the clef, key and
                    // time, where `\repeat volta` from the start draws none.
                    bars[ci].open_bar = true;
                } else {
                    return refuse("a bar line before any music", ev.at);
                }
                continue;
            }
            _ => {}
        }
        if ev.count > 1 {
            if !(pos == 0 && ev.ticks == bars[ci].length) {
                return refuse(format!("a run of bars' rest lasts a whole bar each: in {tn}/{td} write R and the bar's length"), ev.at);
            }
            ev.mm = Some(ev.count);
            bars[ci].span = Some(ev.count);
        }
        if pos + ev.ticks > bars[ci].length {
            return refuse(format!("a note runs past the bar line (bar {})", bars.len()), ev.at);
        }
        ev.start = pos;
        ev.bar = bars.len() - 1;
        if let Some(t) = ev.tuplet {
            if tuplet_bar.get(&t.id).is_some_and(|b| *b != ev.bar) {
                return refuse("a tuplet runs across a bar line", ev.at);
            }
            tuplet_bar.insert(t.id, ev.bar);
        }
        if tempo.is_some() {
            ev.tempo = tempo.take();
        }
        if ev.k == Kind::Note {
            let mid = clef_mid(clef);
            let mut heads = Vec::new();
            for p in &ev.pitches {
                let now = match seen.get(&p.d) {
                    Some(v) => *v,
                    None => alters[modulo(p.d, 7) as usize],
                };
                let mut show = p.alter != now;
                if tied.get(&p.d) == Some(&p.alter) {
                    show = false;
                }
                seen.insert(p.d, p.alter);
                heads.push(Head { pos: p.d - mid, d: p.d, alter: p.alter, acc: if show { Some(p.alter) } else { None }, side: 0, acc_x: None });
            }
            heads.sort_by_key(|h| h.pos);
            ev.heads = heads;
            tied.clear();
            if ev.tie {
                for p in &ev.pitches {
                    tied.insert(p.d, p.alter);
                }
            }
        } else {
            tied.clear();
        }
        let ticks = ev.ticks;
        bars[ci].items.push(ev);
        pos += ticks;
        if pos == bars[ci].length {
            cur = None;
        }
    }
    let mut bars: Vec<Bar> = bars.into_iter().filter(|b| !b.items.is_empty() || b.changes.any()).collect();
    if bars.is_empty() {
        return refuse("there is no music", 0);
    }
    let mut number = 1;
    for bi in 0..bars.len() {
        bars[bi].number = number;
        number += bars[bi].span.unwrap_or(1);
        if bars[bi].start_repeat && bi > 0 {
            let before = &mut bars[bi - 1];
            before.bar = if before.bar == ":|." { ":|.|:".into() } else { ".|:".into() };
        }
    }
    Ok(bars)
}
