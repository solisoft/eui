//! A staff's events cut into bars, with what each note's heads print
//! (`eui_builders_score.sl`, "Bars").

use std::collections::{HashMap, VecDeque};

use crate::parse::{modulo, refuse, Clef, Ev, Head, Kind, Sign, Words};
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
    /// An additive metre's parts (`Ev::time_parts`).
    pub time_parts: [u8; 4],
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
    /// An additive metre's parts (`Ev::time_parts`).
    pub time_parts: [u8; 4],
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
    /// Ticks from the piece's start to the bar's.
    pub tick0: i64,
    /// It has a second voice.
    pub poly: bool,
    /// With two voices, its columns: what starts at each moment, both
    /// voices' (`Kind::Col`, members `(0, index in items)`).
    pub cols: Vec<Ev>,
    /// A pickup bar's missing beginning, in ticks (`\partial`): where its
    /// first note stands in the metre the beams follow.
    pub offset: i64,
    /// A copy of `\repeat percent`: its notes are played and its bar is
    /// drawn as the percent sign.
    pub percent: bool,
    /// It holds a cadenza: no metre, so no beams but the ones written.
    pub cadenza: bool,
    /// Rehearsal marks, segni and codas: where in the bar, in ticks — 0 its
    /// start, its length its end — and what they say. A jump (`\jump`,
    /// `\fine`, its `dir` 1) is at the end of the bar it follows.
    pub marks: Vec<(i64, Words)>,
}

impl Bar {
    pub(crate) fn opening(&self) -> Opening {
        Opening { clef: self.clef, fifths: self.fifths, time: self.time, time_parts: self.time_parts, changes: self.changes }
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

/// The middle line's diatonic number under a clef: the pitch the sign
/// names — g' for G, c' for C, f for F — on the clef's line, moved by its
/// octaves. A percussion staff reads as a C clef on the middle line, as
/// LilyPond's does.
pub fn clef_mid(clef: Clef) -> i64 {
    let named = match clef.sign {
        Sign::G => 11,
        Sign::C | Sign::Percussion | Sign::Tab => 7,
        Sign::F => 3,
    };
    named + 7 * clef.octave - clef.line
}

/// Written values, longest first: `(base, dots, ticks)` — whole to 128th,
/// with up to two dots.
fn values() -> Vec<(i64, i64, i64)> {
    let mut out = Vec::new();
    for base in [1, 2, 4, 8, 16, 32, 64, 128] {
        for dots in 0..=2 {
            let one = WHOLE / base;
            let ticks = one * (2 * (1 << dots) - 1) / (1 << dots);
            if one % (1 << dots) == 0 {
                out.push((base, dots, ticks));
            }
        }
    }
    out.sort_by_key(|v| -v.2);
    out
}

/// `ticks` as written values, as few as can be, longest first; none when
/// no sum of them makes it.
fn spell(mut ticks: i64) -> Option<Vec<(i64, i64, i64)>> {
    let mut out = Vec::new();
    while ticks > 0 {
        let v = values().into_iter().find(|v| v.2 <= ticks)?;
        out.push(v);
        ticks -= v.2;
    }
    Some(out)
}

/// A note or rest that runs past its bar line, as the pieces it is written
/// in: `room` ticks in its own bar, then whole bars of `length`, then what is
/// left. A note's pieces are tied, each to the next; what belongs to its
/// start (dynamics, scripts, words, a slur's start, grace notes) stays on
/// the first, what belongs to its end (a slur's end, its own tie, a
/// glissando) on the last.
fn cut_at_bar_lines(ev: &Ev, room: i64, length: i64) -> Option<Vec<Ev>> {
    let mut spans = vec![room];
    let mut left = ev.ticks - room;
    while left > 0 {
        spans.push(left.min(length));
        left -= length;
    }
    let mut out = Vec::new();
    for span in spans {
        for (base, dots, ticks) in spell(span)? {
            let mut piece = ev.clone();
            piece.base = base;
            piece.dots = dots;
            piece.ticks = ticks;
            out.push(piece);
        }
    }
    let last = out.len() - 1;
    for (k, piece) in out.iter_mut().enumerate() {
        if k > 0 {
            piece.dynamic = None;
            piece.hairpin = None;
            piece.scripts.clear();
            piece.texts.clear();
            piece.fingers.clear();
            piece.slur_on = false;
            piece.phrase_on = false;
            piece.graces.clear();
            piece.arpeggio = false;
            piece.pedal_on = false;
            piece.pedal_off = false;
            piece.chord_name.clear();
        }
        if k < last {
            piece.slur_off = false;
            piece.phrase_off = false;
            piece.hairpin_end = false;
            piece.gliss = false;
            piece.tie = piece.k == Kind::Note;
        }
    }
    Some(out)
}

fn kind_name(k: &Kind) -> &'static str {
    match k {
        Kind::Clef => "clef",
        Kind::Key => "key",
        _ => "time",
    }
}

/// Where the second voice of `<< \\\\ >>` is.
struct Poly {
    /// `\ottava`'s octaves from each tick on, the first voice's.
    ottavas: Vec<(i64, i64)>,
    from: i64,
    to: i64,
    at: i64,
    two: bool,
    seen: HashMap<i64, i64>,
    seen_bar: Option<usize>,
    tied: HashMap<i64, i64>,
}

/// A note or a rest of the second voice, in the bar the first voice made at
/// the same moment; its accidentals and ties its own voice's.
fn voice_two(bars: &mut [Bar], mut ev: Ev, p: &mut Poly) -> Result<(), ScoreError> {
    let at = p.at;
    let Some(bi) = bars.iter().rposition(|b| b.tick0 <= at && at < b.tick0 + b.length) else {
        return refuse("the second voice runs past the first", ev.at);
    };
    if ev.spacer {
        p.at = at + ev.ticks;
        return Ok(());
    }
    let pos = at - bars[bi].tick0;
    if pos + ev.ticks > bars[bi].length && ev.tuplet.is_none() && matches!(ev.k, Kind::Note | Kind::Rest) {
        // Past the bar line, cut as the first voice's notes are.
        if let Some(pieces) = cut_at_bar_lines(&ev, bars[bi].length - pos, bars[bi].length) {
            for piece in pieces {
                voice_two(bars, piece, p)?;
            }
            return Ok(());
        }
    }
    let bar = &mut bars[bi];
    if pos + ev.ticks > bar.length {
        return refuse(format!("a note runs past the bar line (bar {})", bi + 1), ev.at);
    }
    ev.start = pos;
    ev.bar = bi;
    // Under the clef and the key of the bar it lands in.
    let alters = key_alters(bar.fifths);
    let clef = bar.clef;
    if p.seen_bar != Some(bi) {
        p.seen.clear();
        p.seen_bar = Some(bi);
    }
    ev.ottava = p.ottavas.iter().rev().find(|o| o.0 <= at).map_or(0, |o| o.1);
    grace_heads(&mut ev.graces, clef_mid(clef) + 7 * ev.ottava, ev.ottava, &mut p.seen, &alters, ev.voice);
    if ev.k == Kind::Note {
        let mid = clef_mid(clef) + 7 * ev.ottava;
        let mut heads = Vec::new();
        for pt in &ev.pitches {
            let now = match p.seen.get(&pt.d) {
                Some(v) => *v,
                None => alters[modulo(pt.d, 7) as usize],
            };
            let mut show = pt.alter != now;
            if p.tied.get(&pt.d) == Some(&pt.alter) {
                show = false;
            }
            p.seen.insert(pt.d, pt.alter);
            heads.push(Head { pos: pt.d - mid, d: pt.d, alter: pt.alter, acc: if show { Some(pt.alter) } else { None }, side: 0, acc_x: None, style: "" });
        }
        heads.sort_by_key(|h| h.pos);
        ev.heads = heads;
        drum_heads(&mut ev);
        p.tied.clear();
        if ev.tie {
            for pt in &ev.pitches {
                p.tied.insert(pt.d, pt.alter);
            }
        }
    } else {
        p.tied.clear();
    }
    p.at = at + ev.ticks;
    bar.items.push(ev);
    Ok(())
}

/// A drum note's heads, each its drum's.
fn drum_heads(ev: &mut Ev) {
    for h in &mut ev.heads {
        if let Some(dr) = ev.drums.iter().find(|dr| dr.d == h.d) {
            h.style = dr.head;
        }
    }
}

/// The heads of grace notes, before the note they go to: their accidentals
/// as the bar has them so far, and counted in it.
fn grace_heads(graces: &mut [Ev], mid: i64, ottava: i64, seen: &mut HashMap<i64, i64>, alters: &[i64; 7], voice: Option<u8>) {
    for g in graces {
        g.ottava = ottava;
        g.voice = voice;
        g.up = voice.is_none_or(|v| v % 2 == 1);
        let mut heads = Vec::new();
        for p in &g.pitches {
            let now = match seen.get(&p.d) {
                Some(v) => *v,
                None => alters[modulo(p.d, 7) as usize],
            };
            seen.insert(p.d, p.alter);
            heads.push(Head { pos: p.d - mid, d: p.d, alter: p.alter, acc: if p.alter != now { Some(p.alter) } else { None }, side: 0, acc_x: None, style: "" });
        }
        heads.sort_by_key(|h| h.pos);
        g.heads = heads;
    }
}

/// The bars of a staff.
pub fn measures(events: &[Ev]) -> Result<Vec<Bar>, ScoreError> {
    let mut clef = Clef::default();
    let mut cadenza = false;
    let mut ottava = 0;
    let mut ottavas: Vec<(i64, i64)> = Vec::new();
    let mut fifths = 0;
    let (mut tn, mut td) = (4, 4);
    let mut tparts = [0u8; 4];
    let mut bars: Vec<Bar> = Vec::new();
    let mut cur: Option<usize> = None;
    let mut pos = 0;
    let mut seen: HashMap<i64, i64> = HashMap::new();
    let mut alters = key_alters(0);
    let mut tied: HashMap<i64, i64> = HashMap::new();
    let mut tempo = None;
    let mut volta_open: Option<String> = None;
    let mut tuplet_bar: HashMap<i64, usize> = HashMap::new();
    // Ticks from the start, and the second voice of `<< \\\\ >>` placed by
    // them in the bars the first made (`voice_two`).
    let mut tick_at = 0;
    let mut poly: Option<Poly> = None;
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
    let mut queue: VecDeque<Ev> = flat.into();
    while let Some(mut ev) = queue.pop_front() {
        if ev.k == Kind::Tempo {
            tempo = ev.tempo.clone();
            continue;
        }
        if ev.k == Kind::Compress {
            continue;
        }
        match ev.k {
            Kind::Voices => {
                poly = Some(Poly { ottavas: ottavas.clone(), from: tick_at, to: 0, at: 0, two: false, seen: HashMap::new(), seen_bar: None, tied: HashMap::new() });
                continue;
            }
            Kind::VoiceNext => {
                if let Some(p) = &mut poly {
                    // A third or fourth voice: the one before it as long as the first.
                    if p.two && p.at != p.to {
                        return refuse("the voices of << \\\\ >> are not as long as each other", ev.at);
                    }
                    if !p.two {
                        p.to = tick_at;
                    }
                    p.two = true;
                    p.at = p.from;
                    // Each voice its own accidentals and ties.
                    p.seen.clear();
                    p.seen_bar = None;
                    p.tied.clear();
                    p.ottavas = ottavas.clone();
                }
                continue;
            }
            Kind::VoicesEnd => {
                if poly.as_ref().is_some_and(|p| p.at != p.to) {
                    return refuse("the voices of << \\\\ >> are not as long as each other", ev.at);
                }
                poly = None;
                continue;
            }
            _ => {}
        }
        if let Some(p) = poly.as_mut().filter(|p| p.two) {
            voice_two(&mut bars, ev, p)?;
            continue;
        }
        let ci = match cur {
            Some(ci) => ci,
            None => {
                let b = Bar { clef, fifths, time: (tn, td), time_parts: tparts, bar: "|".into(), length: tn * WHOLE / td, volta_in: volta_open.clone(), tick0: tick_at, ..Bar::default() };
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
                        tparts = ev.time_parts;
                        bar.time = (tn, td);
                        bar.time_parts = tparts;
                        bar.length = tn * WHOLE / td;
                        bar.changes.time = true;
                    }
                }
                alters = key_alters(fifths);
                continue;
            }
            Kind::Partial => {
                if bars.len() > 1 || pos > 0 {
                    return refuse("\\partial goes before the first note", ev.at);
                }
                let bar = &mut bars[ci];
                let full = bar.time.0 * WHOLE / bar.time.1;
                if ev.ticks >= full {
                    return refuse("a \\partial is shorter than a bar", ev.at);
                }
                bar.length = ev.ticks;
                bar.offset = full - ev.ticks;
                continue;
            }
            Kind::CadenzaOn => {
                cadenza = true;
                continue;
            }
            Kind::Ottava => {
                ottava = ev.ottava;
                ottavas.push((tick_at, ottava));
                continue;
            }
            Kind::Mark => {
                if let Some(w) = ev.texts.first() {
                    // A jump written at a bar line ends the bar before it.
                    if w.dir > 0 && pos == 0 && ci > 0 {
                        let end = bars[ci - 1].length;
                        bars[ci - 1].marks.push((end, w.clone()));
                    } else {
                        bars[ci].marks.push((pos, w.clone()));
                    }
                }
                continue;
            }
            Kind::CadenzaOff => {
                cadenza = false;
                // The bar ends where the cadenza does, with no bar line
                // unless one is written after it.
                if pos > 0 {
                    bars[ci].length = pos;
                    bars[ci].bar = String::new();
                    cur = None;
                }
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
        if cadenza {
            bars[ci].cadenza = true;
            if pos + ev.ticks > bars[ci].length {
                bars[ci].length = pos + ev.ticks;
            }
        }
        if pos + ev.ticks > bars[ci].length && ev.spacer {
            // A spacer across bar lines, `s1*16`: a piece in each bar.
            let room = bars[ci].length - pos;
            let mut rest = ev.clone();
            rest.ticks -= room;
            ev.ticks = room;
            queue.push_front(rest);
        }
        if pos + ev.ticks > bars[ci].length && !cadenza && ev.tuplet.is_none() && ev.mm.is_none() && matches!(ev.k, Kind::Note | Kind::Rest) {
            // A note longer than what is left of its bar is cut at the bar
            // lines into tied notes, as LilyPond's Completion_heads_engraver
            // cuts it; a rest into rests. Each piece is written as few
            // values as can be.
            if let Some(pieces) = cut_at_bar_lines(&ev, bars[ci].length - pos, bars[ci].length) {
                let mut pieces = pieces.into_iter();
                if let Some(first) = pieces.next() {
                    for piece in pieces.rev() {
                        queue.push_front(piece);
                    }
                    ev = first;
                }
            }
        }
        if pos + ev.ticks > bars[ci].length {
            return refuse(format!("a note runs past the bar line (bar {})", bars.len()), ev.at);
        }
        ev.start = pos;
        ev.bar = bars.len() - 1;
        for t in [ev.tuplet, ev.tuplet_outer].into_iter().flatten() {
            if tuplet_bar.get(&t.id).is_some_and(|b| *b != ev.bar) {
                return refuse("a tuplet runs across a bar line", ev.at);
            }
            tuplet_bar.insert(t.id, ev.bar);
        }
        if tempo.is_some() {
            ev.tempo = tempo.take();
        }
        ev.ottava = ottava;
        grace_heads(&mut ev.graces, clef_mid(clef) + 7 * ottava, ottava, &mut seen, &alters, ev.voice);
        if ev.k == Kind::Note {
            let mid = clef_mid(clef) + 7 * ottava;
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
                heads.push(Head { pos: p.d - mid, d: p.d, alter: p.alter, acc: if show { Some(p.alter) } else { None }, side: 0, acc_x: None, style: "" });
            }
            heads.sort_by_key(|h| h.pos);
            ev.heads = heads;
            drum_heads(&mut ev);
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
        tick_at += ticks * ev.mm.unwrap_or(1);
        bars[ci].items.push(ev);
        pos += ticks;
        if pos == bars[ci].length && !cadenza {
            cur = None;
        }
    }
    // A bar with two voices lists its notes by when they start, the first
    // voice's first.
    for b in &mut bars {
        if b.items.iter().any(|ev| ev.voice.is_some_and(|v| v >= 2)) {
            b.poly = true;
            b.items.sort_by_key(|ev| ev.start * 8 + i64::from(ev.voice.unwrap_or(1)));
        }
    }
    // A mark after the last note stands over the last bar line.
    for bi in 1..bars.len() {
        if bars[bi].items.is_empty() && !bars[bi].changes.any() && !bars[bi].marks.is_empty() {
            let marks = std::mem::take(&mut bars[bi].marks);
            let end = bars[bi - 1].length;
            bars[bi - 1].marks.extend(marks.into_iter().map(|m| (end, m.1)));
        }
    }
    let mut bars: Vec<Bar> = bars.into_iter().filter(|b| !b.items.is_empty() || b.changes.any()).collect();
    if bars.is_empty() {
        return refuse("there is no music", 0);
    }
    percent_bars(&mut bars)?;
    // A pickup is bar 0, as LilyPond counts.
    let mut number = if bars[0].offset > 0 { 0 } else { 1 };
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

/// The bars `\repeat percent` copies, each drawn as the percent sign: a copy
/// fills its bar, and nothing else is in it.
fn percent_bars(bars: &mut [Bar]) -> Result<(), ScoreError> {
    for bi in 0..bars.len() {
        let Some(first) = bars[bi].items.iter().find_map(|ev| ev.percent.map(|p| (p, ev.at))) else { continue };
        let whole = bars[bi].items.iter().all(|ev| ev.percent == Some(first.0));
        let alone = bars.iter().enumerate().all(|(k, b)| k == bi || b.items.iter().all(|ev| ev.percent != Some(first.0)));
        if !(whole && alone) {
            return refuse("\\repeat percent draws a body of one whole bar", first.1);
        }
        bars[bi].percent = true;
    }
    Ok(())
}
