//! Spec 03 §1.4, protocol 10: a note's pitch moved in the source itself.
//!
//! A client that edits a `score` does not ask the server where a note goes:
//! it reads the source with the same parser it engraves with, works out the
//! pitch the way the score editor does, and rewrites the few characters that
//! spell it. Everything here is a function of the source and the item, so
//! the server, writing a pitch the same way, arrives at the same bytes.
//!
//! Pitches are diatonic numbers as the parser counts them — `c` is 0 and
//! `c'`, middle C, is 7 — with an alteration from −2 to 2.

use crate::measure::{clef_mid, key_alters};
use crate::parse::{self, Clef, Kind, T};

/// The semitones of the seven steps above C.
const NATURAL: [i64; 7] = [0, 2, 4, 5, 7, 9, 11];

/// How far a head may go from its staff's middle line, in half spaces.
pub const REACH: i64 = 14;

/// One note of a source, as an editor needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Its number among every note and rest of the piece (03 §1.4 `marks`).
    pub item: usize,
    /// The clef in force where it stands.
    pub clef: Clef,
    /// The key in force there, in fifths.
    pub fifths: i64,
    /// Its heads, in the order the source lists them: `(d, alter)`.
    pub heads: Vec<(i64, i64)>,
    /// Where each head's pitch is written, as a byte range of the source.
    pub spans: Vec<std::ops::Range<usize>>,
}

/// Why a source cannot be edited here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotEditable {
    /// The engraver refused it.
    Refused,
    /// It uses `\relative` or `\fixed`: a pitch there depends on the ones
    /// before it.
    Relative,
    /// It uses `\transpose`: a pitch drawn is not the pitch written.
    Transposed,
    /// It names its notes in another language than LilyPond's default,
    /// which is the one an edit writes.
    Language,
}

/// The note numbered `item` in `src`, or `Ok(None)` when there is no such
/// item or it is a rest.
///
/// # Errors
///
/// [`NotEditable`] when the source cannot be edited at all.
pub fn item(src: &str, item: usize) -> Result<Option<Item>, NotEditable> {
    let toks = parse::tokens(src).map_err(|_| NotEditable::Refused)?;
    if toks.iter().any(|t| t.t == T::Cmd && (t.v == "relative" || t.v == "fixed")) {
        return Err(NotEditable::Relative);
    }
    if toks.iter().any(|t| t.t == T::Cmd && t.v == "transpose") {
        return Err(NotEditable::Transposed);
    }
    if toks.iter().any(|t| t.t == T::Cmd && (t.v == "language" || t.v == "include")) {
        return Err(NotEditable::Language);
    }
    let parsed = parse::parse(src).map_err(|_| NotEditable::Refused)?;
    // Character positions, as the tokens count them, to bytes.
    let bytes: Vec<usize> = src.char_indices().map(|(b, _)| b).chain(std::iter::once(src.len())).collect();
    for staff in &parsed.staves {
        let (mut clef, mut fifths, mut ottava) = (Clef::default(), 0, 0);
        for ev in &staff.events {
            match ev.k {
                Kind::Clef => clef = ev.clef,
                Kind::Key => fifths = ev.fifths,
                Kind::Ottava => ottava = ev.ottava,
                // A chord symbol, a drum and a `q` write no pitch to move.
                Kind::Note if ev.i == item && (ev.pitch_at.len() != ev.pitches.len() || !ev.drums.is_empty() || !ev.chord_name.is_empty()) => return Ok(None),
                Kind::Note if ev.i == item => {
                    let mut spans = Vec::with_capacity(ev.pitch_at.len());
                    for &at in &ev.pitch_at {
                        let start = *bytes.get(at).ok_or(NotEditable::Refused)?;
                        spans.push(start..start + pitch_len(&src[start..]));
                    }
                    let heads = ev.pitches.iter().map(|p| (p.d, p.alter)).collect();
                    // Under `\ottava` a line stands for a pitch an octave off,
                    // as under a clef that moves the music.
                    let clef = Clef { octave: clef.octave + ottava, ..clef };
                    return Ok(Some(Item { item, clef, fifths, heads, spans }));
                }
                Kind::Rest if ev.i == item => return Ok(None),
                _ => {}
            }
        }
    }
    Ok(None)
}

/// The bytes a pitch takes at the start of `s`: its letters, then its octave
/// marks — what the tokenizer reads as one word.
fn pitch_len(s: &str) -> usize {
    let letters = s.bytes().take_while(u8::is_ascii_alphabetic).count();
    letters + s[letters..].bytes().take_while(|b| *b == b'\'' || *b == b',').count()
}

/// A pitch as the score editor writes it, in absolute octaves: `c'` is
/// middle C, and the flats of E and A are `es`, `eses`, `as`, `ases`.
pub fn spell(d: i64, alter: i64) -> String {
    let step = d.rem_euclid(7);
    let name = ["c", "d", "e", "f", "g", "a", "b"][step as usize];
    let suffix = match (alter, step) {
        (1, _) => "is",
        (2, _) => "isis",
        (-1, 2 | 5) => "s",
        (-2, 2 | 5) => "ses",
        (-1, _) => "es",
        (-2, _) => "eses",
        _ => "",
    };
    let marks = d.div_euclid(7);
    let mut out = format!("{name}{suffix}");
    for _ in 0..marks.max(0) {
        out.push('\'');
    }
    for _ in 0..(-marks).max(0) {
        out.push(',');
    }
    out
}

/// The MIDI key of a pitch.
pub fn midi(d: i64, alter: i64) -> i64 {
    12 * (d.div_euclid(7) + 4) + NATURAL[d.rem_euclid(7) as usize] + alter
}

/// The octave 06 §1.5 reports, middle C's being 4.
pub fn octave(d: i64) -> i64 {
    d.div_euclid(7) + 3
}

/// Within the reach of 03 §1.4: 14 half spaces of the middle line, and a
/// key a piano has.
pub fn fits(clef: Clef, d: i64, alter: i64) -> bool {
    (d - clef_mid(clef)).abs() <= REACH && (21..=108).contains(&midi(d, alter))
}

/// The pitch on the line or space `pos` half spaces down from the top line
/// (06 §1.5's `pos`), altered as the key alters that step.
pub fn at_position(clef: Clef, fifths: i64, pos: i64) -> (i64, i64) {
    let d = clef_mid(clef) + 4 - pos;
    (d, key_alters(fifths)[d.rem_euclid(7) as usize])
}

/// The diatonic pitch `d`, altered as the key alters its step.
pub fn in_key(fifths: i64, d: i64) -> (i64, i64) {
    (d, key_alters(fifths)[d.rem_euclid(7) as usize])
}

/// The heads of `it` with head `head` moved `by` steps up, in the key, the
/// heads kept ascending; `None` past the reach. A drag (03 §1.4).
pub fn dragged(it: &Item, head: usize, from: i64, by: i64) -> Option<Vec<(i64, i64)>> {
    let (d, alter) = in_key(it.fifths, from + by);
    if !fits(it.clef, d, alter) {
        return None;
    }
    let mut out = it.heads.clone();
    *out.get_mut(head)? = (d, alter);
    out.sort_by_key(|h| h.0);
    Some(out)
}

/// [`dragged`] by semitones: head `head` moved `by` semitones from the
/// MIDI key of `from`, spelled as the key spells it. A drag with Shift held.
pub fn dragged_chromatic(it: &Item, head: usize, from: (i64, i64), by: i64) -> Option<Vec<(i64, i64)>> {
    let (d, alter) = if by == 0 { from } else { spell_key(it.fifths, midi(from.0, from.1) + by, by.signum()) };
    if !fits(it.clef, d, alter) {
        return None;
    }
    let mut out = it.heads.clone();
    *out.get_mut(head)? = (d, alter);
    out.sort_by_key(|h| h.0);
    Some(out)
}

/// How the arrows move a note (03 §1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// A step up (`1`) or down (`-1`) the staff, in the key.
    Step(i64),
    /// A semitone up or down.
    Semitone(i64),
    /// An octave up or down.
    Octave(i64),
}

/// Every head of `it` moved, or `None` when one would leave the reach — a
/// chord moves all its heads or none.
pub fn moved(it: &Item, how: Move) -> Option<Vec<(i64, i64)>> {
    let out: Vec<(i64, i64)> = it
        .heads
        .iter()
        .map(|&(d, alter)| match how {
            Move::Step(by) => {
                let d = d + by.signum();
                (d, key_alters(it.fifths)[d.rem_euclid(7) as usize])
            }
            Move::Octave(by) => (d + 7 * by.signum(), alter),
            Move::Semitone(by) => spell_key(it.fifths, midi(d, alter) + by.signum(), by.signum()),
        })
        .collect();
    out.iter().all(|&(d, a)| fits(it.clef, d, a)).then_some(out)
}

/// Head `head` of `it` moved, the others where they are and the heads
/// kept ascending; `None` past the reach. The arrows on a chord (03 §1.4).
pub fn moved_head(it: &Item, head: usize, how: Move) -> Option<Vec<(i64, i64)>> {
    let one = Item { heads: vec![*it.heads.get(head)?], ..it.clone() };
    let to = *moved(&one, how)?.first()?;
    let mut out = it.heads.clone();
    *out.get_mut(head)? = to;
    out.sort_by_key(|h| h.0);
    Some(out)
}

/// The head of `it` nearest the pitch `d`: the one a press there chose.
pub fn nearest_head(it: &Item, d: i64) -> usize {
    it.heads.iter().enumerate().min_by_key(|(_, h)| (h.0 - d).abs()).map_or(0, |(k, _)| k)
}

/// A MIDI key spelled as 03 §1.4 says: with the key's alteration of a step
/// if one sounds it, else natural, else sharp going up and flat going down.
fn spell_key(fifths: i64, key: i64, way: i64) -> (i64, i64) {
    let pc = key.rem_euclid(12);
    let in_key = key_alters(fifths);
    let choices = [in_key, [0; 7], [way; 7], [-way; 7]];
    for choice in choices {
        for step in 0..7 {
            if (NATURAL[step] + choice[step] - pc).rem_euclid(12) == 0 {
                let alter = choice[step];
                let oct = (key - NATURAL[step] - alter).div_euclid(12) - 1;
                return ((oct - 3) * 7 + step as i64, alter);
            }
        }
    }
    // Every pitch class is a natural or a step's sharp: never reached.
    (0, 0)
}

/// The head of `it` nearest `d` put on `(d, alter)`, the heads kept in
/// ascending order; `None` past the reach.
pub fn placed(it: &Item, d: i64, alter: i64) -> Option<Vec<(i64, i64)>> {
    if !fits(it.clef, d, alter) {
        return None;
    }
    let nearest = it.heads.iter().enumerate().min_by_key(|(_, h)| (h.0 - d).abs())?.0;
    let mut out = it.heads.clone();
    if let Some(slot) = out.get_mut(nearest) {
        *slot = (d, alter);
    }
    out.sort_by_key(|h| h.0);
    Some(out)
}

/// `src` with the heads of `it` spelled as `heads`, in the slots the source
/// lists them in; every other byte as it was.
pub fn rewrite(src: &str, it: &Item, heads: &[(i64, i64)]) -> String {
    let mut out = String::with_capacity(src.len() + 8);
    let mut from = 0;
    for (span, &(d, alter)) in it.spans.iter().zip(heads) {
        out.push_str(src.get(from..span.start).unwrap_or(""));
        out.push_str(&spell(d, alter));
        from = span.end;
    }
    out.push_str(src.get(from..).unwrap_or(""));
    out
}
