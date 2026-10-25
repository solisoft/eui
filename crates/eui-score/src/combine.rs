//! `\partCombine` (score #405): two parts on one staff, as LilyPond's part
//! combiner sets them. The parts are cut where both start a note or a rest
//! together; each stretch is, unless a `\partCombine…` command says
//! otherwise:
//!
//! - *unisono*, the same notes in the same rhythm: one note, "a2" over the
//!   first of a stretch the parts come together on;
//! - *chords*, the same rhythm, part I over part II and less than a tenth
//!   apart: one stem for both;
//! - *solo*, one part resting: the other alone, "Solo" (or "Solo II") over
//!   its first note;
//! - *apart* — different rhythms, crossing parts, wide intervals: two
//!   voices, part I's stems up and part II's down.
//!
//! The staff's settings (a clef, a metre, a mark) are part I's, and part
//! II's where part I has none of the kind there. Every note sounds: what is
//! drawn once is played once, as the same pitch at the same time is.

use crate::glyphs::Weight;
use crate::look::Combine;
use crate::parse::{merge, refuse, span, staff_level, Ev, Kind, Run, Words};
use crate::ScoreError;

/// What marks the music of a combined pair, its voices' numbers after it.
pub(crate) const COMBINED: &str = "partCombine";

/// Whether a part of `<< >>` is a `\partCombineUp` or `Down` pair.
pub(crate) fn is_pair(events: &[Ev]) -> bool {
    events.iter().any(|e| e.v.starts_with(COMBINED) && matches!(e.k, Kind::Voices | Kind::Note | Kind::Rest))
}

/// How a stretch is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    /// Both parts rest.
    Rests,
    /// One note for both.
    Unisono,
    /// One chord for both.
    Chords,
    /// Part I alone.
    SoloI,
    /// Part II alone.
    SoloII,
    /// Two voices.
    Apart,
}

/// A part's notes and rests with where they start, and its staff's settings.
struct Part {
    items: Vec<(i64, Ev)>,
    settings: Vec<(i64, Ev)>,
    end: i64,
}

fn split(events: Vec<Ev>, at: usize) -> Result<Part, ScoreError> {
    let mut items = Vec::new();
    let mut settings = Vec::new();
    let mut t = 0;
    for ev in events {
        match ev.k {
            Kind::Voices | Kind::VoiceNext | Kind::VoicesEnd => return refuse("voices inside a part \\partCombine combines are not drawn: write each part as one voice", ev.at.max(at)),
            Kind::Note | Kind::Rest => {
                // A run of bars' rest, one bar at a time: a stretch may end in it.
                let copies = ev.count.max(1);
                for _ in 0..copies {
                    let mut one = ev.clone();
                    one.count = 1;
                    items.push((t, one));
                    t += ev.ticks;
                }
            }
            ref k if staff_level(k) => settings.push((t, ev)),
            _ => {}
        }
    }
    Ok(Part { items, settings, end: t })
}

fn rests(items: &[&Ev]) -> bool {
    items.iter().all(|ev| ev.k == Kind::Rest)
}

fn pitches(ev: &Ev) -> Vec<(i64, i64)> {
    let mut p: Vec<(i64, i64)> = ev.pitches.iter().map(|p| (p.d, p.alter)).collect();
    p.sort_unstable();
    p.dedup();
    p
}

/// How a stretch is set when nothing says how: by its notes.
fn by_notes(a: &[&Ev], b: &[&Ev]) -> State {
    match (rests(a), rests(b)) {
        (true, true) => return State::Rests,
        (false, true) => return State::SoloI,
        (true, false) => return State::SoloII,
        _ => {}
    }
    if a.len() != 1 || b.len() != 1 || a[0].ticks != b[0].ticks || a[0].k != Kind::Note || b[0].k != Kind::Note {
        return State::Apart;
    }
    let (x, y) = (a[0], b[0]);
    if pitches(x) == pitches(y) {
        return State::Unisono;
    }
    // A chord when part I stands over part II, less than a tenth from it
    // (LilyPond's `chord-range`, a ninth), and they tie alike.
    let lowest_i = x.pitches.iter().map(|p| p.d).min().unwrap_or(0);
    let highest_ii = y.pitches.iter().map(|p| p.d).max().unwrap_or(0);
    let top_i = x.pitches.iter().map(|p| p.d).max().unwrap_or(0);
    let low_ii = y.pitches.iter().map(|p| p.d).min().unwrap_or(0);
    if lowest_i >= highest_ii && top_i - low_ii <= 8 && x.tie == y.tie && x.tuplet.is_some() == y.tuplet.is_some() {
        return State::Chords;
    }
    State::Apart
}

/// How a stretch is set when a command says how.
fn forced(mode: Combine, a: &[&Ev], b: &[&Ev]) -> State {
    let auto = by_notes(a, b);
    match mode {
        Combine::Automatic => auto,
        Combine::Apart => {
            if auto == State::Rests {
                State::Rests
            } else {
                State::Apart
            }
        }
        Combine::Chords => {
            let one = a.len() == 1 && b.len() == 1 && a[0].ticks == b[0].ticks && a[0].k == Kind::Note && b[0].k == Kind::Note;
            if one {
                State::Chords
            } else {
                auto
            }
        }
        Combine::Unisono => {
            if rests(a) && !rests(b) {
                State::SoloII
            } else {
                State::Unisono
            }
        }
        Combine::SoloI => State::SoloI,
        Combine::SoloII => State::SoloII,
    }
}

/// Part I's note with what part II writes on its own: one note for both.
fn unite(mut x: Ev, y: &Ev, chord: bool) -> Ev {
    if chord {
        for (k, p) in y.pitches.iter().enumerate() {
            if !x.pitches.iter().any(|q| q.d == p.d && q.alter == p.alter) {
                x.pitches.push(*p);
                if let Some(at) = y.pitch_at.get(k) {
                    x.pitch_at.push(*at);
                }
            }
        }
        x.forced.extend(y.forced.iter().copied());
        x.cautionary.extend(y.cautionary.iter().copied());
        x.fingers.extend(y.fingers.iter().cloned());
    }
    for sc in &y.scripts {
        if !x.scripts.iter().any(|s| s.name == sc.name) {
            x.scripts.push(sc.clone());
        }
    }
    if x.dynamic.is_none() {
        x.dynamic.clone_from(&y.dynamic);
    }
    if x.hairpin.is_none() {
        x.hairpin = y.hairpin;
        x.hairpin_end |= y.hairpin_end;
    }
    x.texts.extend(y.texts.iter().filter(|w| !x.texts.contains(w)).cloned().collect::<Vec<_>>());
    x
}

/// "a2", "Solo", "Solo II": over the note, upright, as LilyPond's
/// CombineTextScript prints them.
fn say(ev: &mut Ev, words: &str) {
    ev.texts.insert(0, Words { text: words.to_owned(), dir: -1, runs: vec![Run::words(words, Weight::Regular)] });
}

/// The two parts as one staff's events: `dir` 1 for `\partCombineUp` (its
/// stems up, its voices the first and third), -1 for `\partCombineDown`.
/// `modes`: the `\partCombine…` commands written in either part, by time.
pub(crate) fn combine(part_i: Vec<Ev>, part_ii: Vec<Ev>, mut modes: Vec<(i64, Combine)>, dir: i8, at: usize) -> Result<Vec<Ev>, ScoreError> {
    let originals = (part_i.clone(), part_ii.clone());
    let a = split(part_i, at)?;
    let b = split(part_ii, at)?;
    modes.sort_by_key(|m| m.0);
    // The stretches: cut where both parts start something.
    let starts = |p: &Part| -> Vec<i64> { p.items.iter().map(|it| it.0).chain(std::iter::once(p.end)).collect() };
    let (sa, sb) = (starts(&a), starts(&b));
    let end = a.end.max(b.end);
    let mut cuts: Vec<i64> = sa.iter().copied().filter(|t| sb.contains(t) || *t >= b.end).chain(sb.iter().copied().filter(|t| *t >= a.end)).collect();
    cuts.push(0);
    cuts.push(end);
    cuts.sort_unstable();
    cuts.dedup();
    let (voice_i, voice_ii) = if dir < 0 {
        (2u8, 4u8)
    } else if dir > 0 {
        (1, 3)
    } else {
        (1, 2)
    };
    // What tells `<< >>` the parts are two voices of one combined pair.
    let tag = format!("{COMBINED} {voice_i} {voice_ii}");
    let mut out: Vec<Ev> = Vec::new();
    let mut apart: Option<(Vec<Ev>, Vec<Ev>)> = None;
    let mut last_said: Option<State> = None;
    let close = |out: &mut Vec<Ev>, apart: &mut Option<(Vec<Ev>, Vec<Ev>)>| {
        if let Some((one, two)) = apart.take() {
            out.push(Ev { k: Kind::Voices, v: tag.clone(), at, ..Ev::default() });
            out.extend(one);
            out.push(Ev { k: Kind::VoiceNext, at, ..Ev::default() });
            out.extend(two);
            out.push(Ev { k: Kind::VoicesEnd, at, ..Ev::default() });
        }
    };
    for w in cuts.windows(2) {
        let (t0, t1) = (w[0], w[1]);
        let ia: Vec<&Ev> = a.items.iter().filter(|it| it.0 >= t0 && it.0 < t1).map(|it| &it.1).collect();
        let ib: Vec<&Ev> = b.items.iter().filter(|it| it.0 >= t0 && it.0 < t1).map(|it| &it.1).collect();
        let mode = modes.iter().rev().find(|m| m.0 <= t0).map_or(Combine::Automatic, |m| m.1);
        // Past the end of a part, the other is alone.
        let state = if ib.is_empty() && !rests(&ia) {
            State::SoloI
        } else if ia.is_empty() && !rests(&ib) {
            State::SoloII
        } else if ia.is_empty() || ib.is_empty() {
            State::Rests
        } else {
            forced(mode, &ia, &ib)
        };
        let mut set: Vec<Ev> = match state {
            State::Apart => {
                let mark = |list: &[&Ev], v: u8| -> Vec<Ev> {
                    list.iter()
                        .map(|ev| {
                            let mut e = (*ev).clone();
                            e.voice = Some(v);
                            e
                        })
                        .collect()
                };
                let (mut one, two) = apart.take().unwrap_or_default();
                let mut two = two;
                one.extend(mark(&ia, voice_i));
                two.extend(mark(&ib, voice_ii));
                apart = Some((one, two));
                Vec::new()
            }
            State::Rests => {
                // Part I's rests, unless they are spacers and part II's are not.
                let keep = if ia.iter().all(|e| e.spacer) && !ib.iter().all(|e| e.spacer) { &ib } else { &ia };
                keep.iter().map(|e| (*e).clone()).collect()
            }
            State::SoloI => ia.iter().map(|e| (*e).clone()).collect(),
            State::SoloII => ib.iter().map(|e| (*e).clone()).collect(),
            State::Unisono => {
                if ia.len() == 1 && ib.len() == 1 {
                    vec![unite(ia[0].clone(), ib[0], false)]
                } else {
                    ia.iter().map(|e| (*e).clone()).collect()
                }
            }
            State::Chords => vec![unite(ia[0].clone(), ib[0], true)],
        };
        if state != State::Apart {
            close(&mut out, &mut apart);
        }
        // What the stretch says when it starts something new.
        let words = match state {
            State::Unisono if last_said != Some(State::Unisono) => Some("a2"),
            State::SoloI if last_said != Some(State::SoloI) => Some("Solo"),
            State::SoloII if last_said != Some(State::SoloII) => Some("Solo II"),
            _ => None,
        };
        if state != State::Rests {
            last_said = Some(state);
        }
        if let Some(words) = words {
            if let Some(first) = set.iter_mut().find(|e| e.k == Kind::Note) {
                say(first, words);
            }
        }
        if dir != 0 {
            for e in &mut set {
                e.v.clone_from(&tag);
                if e.k == Kind::Note {
                    e.stem_dir = dir;
                }
            }
        }
        out.extend(set);
    }
    close(&mut out, &mut apart);
    // The staff's settings: part I's, and part II's where part I has none
    // of the kind at that moment.
    let (out, _) = merge(out, originals.0);
    let mine: Vec<(i64, std::mem::Discriminant<Kind>)> = a.settings.iter().map(|(t, e)| (*t, std::mem::discriminant(&e.k))).collect();
    let theirs: Vec<Ev> = {
        let mut t = 0;
        let mut kept = Vec::new();
        for ev in originals.1 {
            if staff_level(&ev.k) {
                if mine.contains(&(t, std::mem::discriminant(&ev.k))) {
                    continue;
                }
            } else {
                t += span(&ev);
            }
            kept.push(ev);
        }
        kept
    };
    let (out, _) = merge(out, theirs);
    Ok(out)
}

/// A part of `<< … >>` holding the voices of a `\partCombineUp` (or
/// `Down`): each voice a part of its own, the music both share in the
/// first and a spacer as long in the second — so that two combined pairs
/// share one staff, four voices on it. Each with its voice's number.
pub(crate) fn unnest(events: Vec<Ev>) -> Vec<(u8, Vec<Ev>)> {
    let numbers: Vec<u8> = events.iter().find_map(|e| e.v.strip_prefix(COMBINED)).map(|rest| rest.split_whitespace().filter_map(|n| n.parse().ok()).collect()).unwrap_or_default();
    let (a, b) = (numbers.first().copied().unwrap_or(1), numbers.get(1).copied().unwrap_or(2));
    let mut one = Vec::new();
    let mut two = Vec::new();
    let mut where_ = 0;
    for ev in events {
        match ev.k {
            Kind::Voices => where_ = 1,
            Kind::VoiceNext => where_ = 2,
            Kind::VoicesEnd => where_ = 0,
            _ if where_ == 2 => two.push(ev),
            Kind::Note | Kind::Rest if where_ == 0 => {
                let mut held = Ev {
                    k: Kind::Rest,
                    spacer: true,
                    ticks: ev.ticks,
                    base: ev.base,
                    dots: ev.dots,
                    count: ev.count,
                    tuplet: ev.tuplet,
                    tuplet_outer: ev.tuplet_outer,
                    at: ev.at,
                    i: ev.i,
                    ..Ev::default()
                };
                held.look = ev.look.clone();
                two.push(held);
                one.push(ev);
            }
            _ => one.push(ev),
        }
    }
    vec![(a, one), (b, two)]
}
