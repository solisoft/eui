//! What goes on and around the notes once they are placed: ties and slurs,
//! tuplets, articulations, dynamics and hairpins, words, endings and tempo
//! marks, and how far all of it reaches (`eui_builders_score.sl`, "Ties and
//! slurs" and "Expression").

pub use crate::cross::CurveKind;
use crate::draw::{dot_start, past_flag, tremolo_reach};
use crate::engrave::{barline_width, rest_name, stem_x, y_of, Line};
use crate::glyphs::{glyph, Weight};
use crate::measure::Bar;
use crate::parse::{Ev, Kind, Run, StaffKind, Tempo};
use crate::text::{args_of, em, text_box, text_width};
use crate::RULE;

/// A tempo mark, where it is printed.
#[derive(Debug, Clone)]
pub struct TempoMark {
    /// Its left.
    pub x: f64,
    /// Its baseline.
    pub y: f64,
    /// What it says.
    pub mark: Tempo,
}

/// An ending's bracket.
#[derive(Debug, Clone, Default)]
pub struct Volta {
    /// "1.", "2."…
    pub label: String,
    /// Its ending starts on this line.
    pub first: bool,
    /// Its left.
    pub x0: f64,
    /// Its right.
    pub x1: f64,
    /// It goes back: a hook at its end.
    pub closed: bool,
    /// Its line.
    pub y: f64,
    /// Its bars, by index into the staff's.
    pub bars: Vec<usize>,
}

/// A tuplet's number, and its bracket if it has one.
#[derive(Debug, Clone)]
pub struct TupletPlace {
    /// Its left.
    pub x0: f64,
    /// Its right.
    pub x1: f64,
    /// Its line at `x0`.
    pub ya: f64,
    /// … at `x1`.
    pub yb: f64,
    /// Above the notes.
    pub up: bool,
    /// Drawn with a bracket.
    pub bracket: bool,
    /// The number.
    pub label: String,
}

/// An articulation's glyph, centred on `x` — or a fingering's figures.
#[derive(Debug, Clone)]
pub struct ScriptPlace {
    /// The glyph.
    pub glyph: &'static str,
    /// Its centre.
    pub x: f64,
    /// Its origin.
    pub y: f64,
    /// A fingering's figures, drawn in the time signature's digits `k` their
    /// size, in place of `glyph`.
    pub figure: Option<String>,
    /// …
    pub k: f64,
    /// The figures are a string's number, in a circle.
    pub circled: bool,
    /// The note or rest it is written on: its number among its staff's
    /// items ([`Ev::i`]).
    pub item: usize,
    /// Which of them it is, as the source writes them: its index among the
    /// note's articulations ([`Ev::scripts`]), or among its fingerings
    /// ([`Ev::fingers`]) for a `figure`.
    pub index: usize,
}

impl ScriptPlace {
    /// Its ink's box, `[left, top, right, bottom]` in its line's frame.
    pub fn bbox(&self) -> [f64; 4] {
        match &self.figure {
            Some(_) if self.circled => [self.x - STRING_R, self.y - STRING_R, self.x + STRING_R, self.y + STRING_R],
            Some(f) => {
                let (top, bottom) = figure_box(f);
                let hw = figure_width(f) / 2.0;
                [self.x - hw, self.y + top, self.x + hw, self.y + bottom]
            }
            None => {
                let bb = glyph(self.glyph).bbox;
                [self.x + bb[0], self.y + bb[1], self.x + bb[2], self.y + bb[3]]
            }
        }
    }
}

/// A string number's circle: its radius, and its figure's size against
/// the time signature's digits.
pub(crate) const STRING_R: f64 = 0.75;
/// …
pub(crate) const STRING_K: f64 = 0.45;

/// A fingering's size against the time signature's digits: LilyPond's
/// `font-size` -5.
pub(crate) const FINGER_K: f64 = 0.5612310241546865;

/// `\ottava`'s bracket on one line.
#[derive(Debug, Clone)]
pub struct OttavaPlace {
    /// Its words: "8" or "15", as LilyPond prints them.
    pub text: String,
    /// Where the words start.
    pub x0: f64,
    /// Where the dashes end.
    pub x1: f64,
    /// The words' baseline.
    pub y: f64,
    /// Above the staff.
    pub up: bool,
    /// It ends on this line: a hook.
    pub hook: bool,
}

/// A rehearsal mark, a segno or a coda over a bar line.
#[derive(Debug, Clone)]
pub struct MarkPlace {
    /// What it says.
    pub runs: Vec<Run>,
    /// Its left.
    pub x: f64,
    /// Its baseline.
    pub y: f64,
    /// A frame round it, from its formatter: "box", "circle", or "".
    pub frame: String,
}

/// How far a rehearsal mark's frame stands off its letters.
pub(crate) const FRAME_PAD: f64 = 0.35;

/// A dynamic or a hairpin on a line under the staff.
#[derive(Debug, Clone, Default)]
pub struct DynEl {
    /// The dynamic's letters; none for a hairpin.
    pub text: Option<String>,
    /// A hairpin written as words, `\cresc`: "cresc." and dashes; "" on a
    /// line it carries on to.
    pub words: Option<String>,
    /// Its left.
    pub x0: f64,
    /// Its right.
    pub x1: f64,
    /// `<` or `>`.
    pub grow: char,
    /// How open the piece is at its start, 0–1.
    pub from: f64,
    /// … at its end.
    pub to: f64,
    /// Its ink above its line.
    pub top: f64,
    /// … and below.
    pub bottom: f64,
    /// Its line.
    pub y: f64,
}

/// Words over or under the notes.
#[derive(Debug, Clone)]
pub struct TextPlace {
    /// The words.
    pub text: String,
    /// In their faces.
    pub runs: Vec<Run>,
    /// Their left.
    pub x0: f64,
    /// Their right.
    pub x1: f64,
    /// The baseline.
    pub y: f64,
    /// The ink's top.
    pub top: f64,
    /// … bottom.
    pub bottom: f64,
    /// -1 above, 1 below.
    pub dir: i64,
    /// Their size (`TextScript.font-size`).
    pub em: f64,
    /// The note or rest they are written on: its number among its staff's items.
    pub item: usize,
    /// Which of its words they are, in the order the source writes them.
    pub index: usize,
}

/// A piece of the sostenuto pedal's bracket on a line (score #1026): its
/// horizontal from `x0` to `x1` on `y`, and the hook up at its end where the
/// pedal is let go there.
#[derive(Debug, Clone, PartialEq)]
pub struct PedalBracket {
    /// Its left.
    pub x0: f64,
    /// Its right.
    pub x1: f64,
    /// Its line, in the staff's frame.
    pub y: f64,
    /// Let go at `x1`: a hook up there.
    pub hook: bool,
}

/// A text spanner on one line: its words (on the line it starts on), then
/// a dashed line.
#[derive(Debug, Clone, PartialEq)]
pub struct SpanPlace {
    /// The words before the line; none on a line it carries on to.
    pub runs: Vec<Run>,
    /// Where the words start.
    pub x0: f64,
    /// Where the dashes end.
    pub x1: f64,
    /// The words' baseline.
    pub y: f64,
}

/// One system's piece of a hairpin.
#[derive(Debug, Clone)]
pub struct HairpinPiece {
    /// Its left.
    pub x0: f64,
    /// Its right.
    pub x1: f64,
    /// `<` or `>`.
    pub grow: char,
    /// How far through the hairpin it starts, 0–1.
    pub from: f64,
    /// … ends.
    pub to: f64,
    /// `\cresc`'s words, on the piece it starts: drawn with dashes after
    /// them instead of a wedge (a later piece: the dashes alone).
    pub words: Option<String>,
    /// The layout settings of the note it starts on.
    pub look: crate::look::Look,
}

/// A tie, a slur or a phrasing slur as it is drawn on a line: the curve of
/// the same index in [`Line::curves`], and where it comes from — what an
/// editor needs to take one and reshape it.
#[derive(Debug, Clone, PartialEq)]
pub struct CurvePlace {
    /// A tie, a slur or a phrasing slur.
    pub kind: CurveKind,
    /// The note it starts on: its number among its staff's items
    /// ([`Ev::i`]).
    pub item: usize,
    /// Where that note is written: its character in the source
    /// ([`Ev::at`]).
    pub at: usize,
    /// A tie's head: its diatonic number ([`crate::parse::Head::d`]), which
    /// tells the ties of a chord apart.
    pub head: Option<i64>,
    /// The control points of its middle line, `(x, y)` in its line's frame,
    /// left to right.
    pub points: [(f64, f64); 4],
    /// It arches up, over the notes.
    pub up: bool,
    /// 0, or 1 for the piece on the line it ends on when a line break
    /// parts it.
    pub piece: usize,
}

struct ScriptRule {
    up: &'static str,
    down: &'static str,
    pad: f64,
    prio: i64,
    side: i64,
    quant: bool,
}

/// How wide a script is drawn: the wider of its two signs' ink.
pub(crate) fn script_width(name: &str) -> f64 {
    let rule = script_rule(name);
    let ink = |g: &str| {
        let b = glyph(g).bbox;
        b[2] - b[0]
    };
    ink(rule.up).max(ink(rule.down))
}

fn script_rule(name: &str) -> ScriptRule {
    let r = |up, down, pad, prio, side, quant| ScriptRule { up, down, pad, prio, side, quant };
    match name {
        "staccato" => r("articStaccatoAbove", "articStaccatoAbove", 0.2, -100, 0, true),
        "tenuto" => r("articTenutoAbove", "articTenutoAbove", 0.2, -50, 0, true),
        "accent" => r("articAccentAbove", "articAccentAbove", 0.2, 0, 0, false),
        "marcato" => r("articMarcatoAbove", "articMarcatoBelow", 0.2, 0, 0, true),
        "staccatissimo" => r("articStaccatissimoAbove", "articStaccatissimoBelow", 0.2, 0, 0, false),
        "portato" => r("articTenutoStaccatoAbove", "articTenutoStaccatoBelow", 0.45, 0, 0, false),
        "trill" => r("ornamentTrill", "ornamentTrill", 0.2, 2000, -1, false),
        "prall" => r("ornamentShortTrill", "ornamentShortTrill", 0.2, 0, -1, false),
        "mordent" => r("ornamentMordent", "ornamentMordent", 0.2, 0, -1, false),
        "turn" => r("ornamentTurn", "ornamentTurn", 0.2, 0, -1, false),
        "open" => r("brassMuteOpen", "brassMuteOpen", 0.2, 0, -1, false),
        "stopped" => r("brassMuteClosed", "brassMuteClosed", 0.2, 0, -1, false),
        // The bowings and a harmonic over the note, as LilyPond's
        // script-alist sets them; the espressivo opposite the stem.
        "upbow" => r("stringsUpBow", "stringsUpBow", 0.2, 0, -1, false),
        "downbow" => r("stringsDownBow", "stringsDownBow", 0.2, 0, -1, false),
        "flageolet" => r("stringsHarmonic", "stringsHarmonic", 0.2, 0, -1, false),
        "espressivo" => r("dynamicMessaDiVoce", "dynamicMessaDiVoce", 0.2, 0, 0, false),
        // `\segno`, `\coda`, `\varcoda` over a note, as LilyPond sets them.
        "segno" => r("segno", "segno", 0.4, 0, -1, false),
        "coda" | "varcoda" => r("coda", "coda", 0.4, 0, -1, false),
        // Score #1026: the other ornaments over the note, as the turn;
        // the snap pizzicato over it (LilyPond's `snappizzicato`, padding 0.2).
        "reverseturn" => r("ornamentTurnInverted", "ornamentTurnInverted", 0.2, 0, -1, false),
        "prallprall" => r("ornamentTremblement", "ornamentTremblement", 0.2, 0, -1, false),
        "prallmordent" => r("ornamentPrecompTrillWithMordent", "ornamentPrecompTrillWithMordent", 0.2, 0, -1, false),
        "upprall" => r("ornamentPrecompSlideTrillDAnglebert", "ornamentPrecompSlideTrillDAnglebert", 0.2, 0, -1, false),
        "lineprall" => r("ornamentPrecompAppoggTrill", "ornamentPrecompAppoggTrill", 0.2, 0, -1, false),
        "snappizzicato" => r("pluckedSnapPizzicatoAbove", "pluckedSnapPizzicatoAbove", 0.2, 0, -1, false),
        // The fermatas, each its own sign, set as `\fermata` is.
        "veryshortfermata" => r("fermataVeryShortAbove", "fermataVeryShortBelow", 0.4, 4000, -1, false),
        "shortfermata" => r("fermataShortAbove", "fermataShortBelow", 0.4, 4000, -1, false),
        "longfermata" => r("fermataLongAbove", "fermataLongBelow", 0.4, 4000, -1, false),
        "verylongfermata" => r("fermataVeryLongAbove", "fermataVeryLongBelow", 0.4, 4000, -1, false),
        "henzeshortfermata" => r("fermataShortHenzeAbove", "fermataShortHenzeBelow", 0.4, 4000, -1, false),
        "henzelongfermata" => r("fermataLongHenzeAbove", "fermataLongHenzeBelow", 0.4, 4000, -1, false),
        _ => r("fermataAbove", "fermataBelow", 0.4, 4000, -1, false),
    }
}

// ---- Ties and slurs ---------------------------------------------------------

/// LilyPond's height of a curve over `width`: it grows with the width and
/// levels off at `limit`.
pub(crate) fn curve_height(width: f64, limit: f64, ratio: f64) -> f64 {
    limit * 2.0 / std::f64::consts::PI * atan(std::f64::consts::PI / 2.0 * ratio / limit * width)
}

/// The Soli engraver's arctangent, a minimax polynomial good to 1e-5: kept
/// so the two draw the same curve to the last digit.
fn atan(v: f64) -> f64 {
    if v < 0.0 {
        return -atan(-v);
    }
    if v > 1.0 {
        return std::f64::consts::PI / 2.0 - atan(1.0 / v);
    }
    let v2 = v * v;
    v * (0.99997726 + v2 * (-0.33262347 + v2 * (0.19354346 + v2 * (-0.11643287 + v2 * (0.05265332 + v2 * -0.01172120)))))
}

/// A note, by its bar and its place there.
type At = (usize, usize);

/// How long a `\laissezVibrer` or `\repeatTie` tie is.
pub(crate) const SHORT_TIE: f64 = 1.8;

/// Every tie and slur of a staff, on the lines they cross.
pub(crate) fn curves(bars: &mut [Bar], lines: &mut [Line]) {
    let mut notes: Vec<At> = Vec::new();
    for (si, line) in lines.iter_mut().enumerate() {
        for b in line.bars.clone() {
            for (j, ev) in bars[b].items.iter_mut().enumerate() {
                if ev.k == Kind::Note {
                    ev.sys = si;
                    notes.push((b, j));
                }
            }
        }
        line.curves = Vec::new();
        line.curve_places = Vec::new();
    }
    let ev_at = |at: At| -> &Ev { &bars[at.0].items[at.1] };
    for (i, &at) in notes.iter().enumerate() {
        let ev = ev_at(at);
        if !ev.tie {
            continue;
        }
        // The next note of the same voice.
        let Some(nxt) = notes.iter().skip(i + 1).map(|&a| ev_at(a)).find(|n| voice_of(n) == voice_of(ev)) else { continue };
        let count = ev.heads.len();
        for (hi, hd) in ev.heads.iter().enumerate() {
            let Some(other) = nxt.heads.iter().find(|o| o.d == hd.d) else { continue };
            let mut over = if count == 1 { !ev.up } else { hi >= count / 2 };
            // Two voices: the first's ties over, the second's under.
            if let Some(v) = ev.voice.filter(|_| !ev.free) {
                over = v % 2 == 1;
            }
            // `\tieUp`, `\override Tie.direction`: over all of it.
            if let Some(d) = ev.look.dir("Tie") {
                over = d > 0;
            }
            tie(lines, ev, hd, nxt, other.pos, over);
        }
    }
    // `\laissezVibrer` and `\repeatTie`: a short tie from each head to
    // nothing, or into it from nothing, on the side a tie would take.
    for &at in &notes {
        let ev = ev_at(at);
        if !(ev.lv || ev.repeat_tie) {
            continue;
        }
        let count = ev.heads.len();
        for (hi, hd) in ev.heads.iter().enumerate() {
            let mut over = if count == 1 { !ev.up } else { hi >= count / 2 };
            if let Some(v) = ev.voice.filter(|_| !ev.free) {
                over = v % 2 == 1;
            }
            if let Some(d) = ev.look.dir("Tie") {
                over = d > 0;
            }
            let dir = if over { -1.0 } else { 1.0 };
            let y = y_of(hd.pos) + dir * 0.35;
            let line = &mut lines[ev.sys];
            let from = Source { kind: CurveKind::Tie, item: ev.i, at: ev.at, head: Some(hd.d), piece: 0 };
            if ev.lv {
                let xa = ev.x + ev.head_w.unwrap_or(0.0) + 0.15;
                arc(line, &ev.look, from, xa, y, xa + SHORT_TIE, y, dir);
            }
            if ev.repeat_tie {
                let acc = ev.heads.iter().filter_map(|h| h.acc_x).fold(0.0, f64::min);
                let xb = ev.x + acc - 0.15;
                arc(line, &ev.look, from, xb - SHORT_TIE, y, xb, y, dir);
            }
        }
    }
    // Each voice's slurs close on its own notes.
    let mut open: [Vec<At>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let mut slurs: Vec<(At, At)> = Vec::new();
    let mut drawn: Vec<Drawn> = Vec::new();
    for &at in &notes {
        let ev = ev_at(at);
        let mine = &mut open[usize::from(voice_of(ev) - 1)];
        if ev.slur_on {
            mine.push(at);
        }
        if ev.slur_off {
            if let Some(first) = mine.pop() {
                let side = forced(ev_at(first).slur_dir, ev_at(first).look.dir("Slur"));
                slur(lines, bars, &notes, (first, at), CurveKind::Slur, (0.0, 0.0), side, &mut drawn);
                slurs.push((first, at));
            }
        }
    }
    // Phrasing slurs, `\( \)`: drawn as slurs are, and outside any slur
    // they share notes with.
    let mut open: [Vec<At>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for &at in &notes {
        let ev = ev_at(at);
        let mine = &mut open[usize::from(voice_of(ev) - 1)];
        if ev.phrase_on {
            mine.push(at);
        }
        if ev.phrase_off {
            if let Some(first) = mine.pop() {
                let (a, b) = (ev_at(first).n.unwrap_or(0), ev.n.unwrap_or(0));
                // Where a slur starts or ends on the note it does, it starts
                // and ends farther out.
                let shared = |n: usize| slurs.iter().any(|&(f, l)| voice_of(ev_at(f)) == voice_of(ev) && (ev_at(f).n == Some(n) || ev_at(l).n == Some(n)));
                let out = |n: usize| if shared(n) { PHRASE_OUT } else { 0.0 };
                let side = forced(ev_at(first).phrase_dir, ev_at(first).look.dir("PhrasingSlur"));
                slur(lines, bars, &notes, (first, at), CurveKind::Phrase, (out(a), out(b)), side, &mut drawn);
            }
        }
    }
    for line in lines.iter_mut() {
        line.glissandi = Vec::new();
    }
    for (i, &at) in notes.iter().enumerate() {
        let ev = ev_at(at);
        if !ev.gliss {
            continue;
        }
        let Some(nxt) = notes.iter().skip(i + 1).map(|&a| ev_at(a)).find(|n| voice_of(n) == voice_of(ev)) else { continue };
        glissando(lines, ev, nxt);
    }
}

/// A glissando's line, from after a note's head to before the next one's —
/// head to head, the lowest of a chord to the lowest of the next.
fn glissando(lines: &mut [Line], ev: &Ev, nxt: &Ev) {
    let (Some(a), Some(b)) = (ev.heads.first(), nxt.heads.first()) else { return };
    let x0 = ev.x + ev.head_w.unwrap_or(0.0) + 0.3 + if ev.dots > 0 { 0.3 + 0.6 * ev.dots as f64 } else { 0.0 };
    let x1 = nxt.x - nxt.left - 0.3;
    let (y0, y1) = (y_of(a.pos), y_of(b.pos));
    if ev.sys == nxt.sys {
        lines[ev.sys].glissandi.push([x0, y0, x1, y1]);
        return;
    }
    let end = lines[ev.sys].bar_end - 0.5;
    let start = lines[nxt.sys].prefix - 0.6;
    let mid = (y0 + y1) / 2.0;
    lines[ev.sys].glissandi.push([x0, y0, end, mid]);
    lines[nxt.sys].glissandi.push([start, mid, x1, y1]);
}

fn tie(lines: &mut [Line], ev: &Ev, hd: &crate::parse::Head, nxt: &Ev, other: i64, over: bool) {
    let pos = hd.pos;
    let from = Source { kind: CurveKind::Tie, item: ev.i, at: ev.at, head: Some(hd.d), piece: 0 };
    let dir = if over { -1.0 } else { 1.0 };
    let y = y_of(pos) + dir * 0.35;
    let y2 = y_of(other) + dir * 0.35;
    let mut xa = ev.x + ev.head_w.unwrap_or(0.0) + 0.15;
    if ev.dots > 0 && over {
        // A dot moved past its note's flag, out in the space above: a tie
        // over the note starts after it rather than arching over it.
        let dot_pos = if pos % 2 == 0 { pos + 1 } else { pos };
        if past_flag(ev, dot_pos) > 0.0 {
            xa = xa.max(ev.x + dot_start(ev, dot_pos) + 0.6 * (ev.dots - 1) as f64 + glyph("augmentationDot").advance + 0.15);
        }
    }
    let xb = nxt.x - 0.15;
    if ev.sys == nxt.sys {
        arc(&mut lines[ev.sys], &ev.look, from, xa, y, xb, y2, dir);
    } else {
        let end = lines[ev.sys].bar_end - 0.5;
        arc(&mut lines[ev.sys], &ev.look, from, xa, y, end, y, dir);
        let later = &mut lines[nxt.sys];
        let start = later.prefix - 0.6;
        arc(later, &ev.look, Source { piece: 1, ..from }, start, y2, xb, y2, dir);
    }
}

/// The side a slur is forced to: its own (`^(` 1, `_(` -1) over a
/// setting's (`\slurUp`), and none when neither says.
fn forced(own: i64, setting: Option<i64>) -> Option<i64> {
    if own != 0 {
        Some(own)
    } else {
        setting
    }
}

// ---- Slurs ------------------------------------------------------------------
//
// A slur is chosen as LilyPond's slur scoring chooses one, reduced: each end
// may stand where its note puts it or a few steps farther out; each pair of
// ends is arched just high enough to clear what the slur goes round — the
// heads, the stems and beams on its side, the accidentals, the articulations
// next to the notes — and scored; the cheapest is drawn.

/// How far a slur keeps from what it goes round.
pub(crate) const SLUR_CLEAR: f64 = 0.5;
/// … from the accidental of the note it ends on, which it comes down past.
const SLUR_CLEAR_LAST: f64 = 0.2;
/// How far out a phrasing slur starts, where a slur starts on the same note.
const PHRASE_OUT: f64 = 1.2;
/// How far out of its place an end is tried, in spaces: LilyPond's
/// `region-size` is 4, in half spaces.
const SLUR_LIFTS: [f64; 8] = [0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0];
/// LilyPond's `edge-attraction-factor`: what a space out of its place costs
/// an end.
const SLUR_EDGE: f64 = 4.0;
/// … an end that runs along its note's stem: a fifth, as LilyPond's.
const SLUR_EDGE_STEM: f64 = 0.8;
/// `steeper-slope-factor`: a slur steeper than its notes, by the space.
const SLUR_STEEPER: f64 = 50.0;
/// `max-slope-factor` (counted twice, as LilyPond does), past a slope
/// gentler than LilyPond's `max-slope` of 1.1: this curve is leaned, not
/// turned, and a steep one reads as a stroke through its notes.
const SLUR_MAX_SLOPE: f64 = 0.7;
/// …
const SLUR_MAX_SLOPE_FACTOR: f64 = 20.0;
/// `non-horizontal-penalty`: a slope between two notes of one height.
const SLUR_TILTED: f64 = 15.0;
/// `same-slope-penalty`: a slope against its notes'.
const SLUR_AGAINST: f64 = 20.0;
/// What an end on a staff line costs.
const SLUR_ON_LINE: f64 = 3.0;

/// A slur drawn on a line: what a phrasing slur over it goes round.
struct Drawn {
    sys: usize,
    voice: u8,
    first: usize,
    last: usize,
    points: [(f64, f64); 4],
}

/// Where a curve comes from: what [`CurvePlace`] says of it.
#[derive(Clone, Copy)]
struct Source {
    kind: CurveKind,
    item: usize,
    at: usize,
    head: Option<i64>,
    piece: usize,
}

/// Where a slur meets the note it starts or ends on.
#[derive(Clone, Copy)]
struct Tip {
    /// Its place when nothing moves it.
    x: f64,
    y: f64,
    /// The stem it runs along, beside it up to its end and over it past
    /// there: its middle and its end.
    stem: Option<(f64, f64)>,
    /// What a space out of its place costs.
    pull: f64,
    /// It stands on a beam.
    beamed: bool,
}

impl Tip {
    /// The end `lift` out of its place, on the side `s` of the notes.
    fn at(&self, lift: f64, s: f64, left: bool) -> (f64, f64) {
        let y = self.y + s * lift;
        match self.stem {
            Some((sx, end)) if s * (y - end) < 0.25 => (sx + if left { 0.365 } else { -0.365 }, y),
            Some((sx, _)) => (sx, y),
            None => (self.x, y),
        }
    }
}

/// `y`, moved off a staff line it would lie on (LilyPond's
/// `move_away_from_staffline`).
fn off_line(y: f64, s: f64) -> f64 {
    if on_line(y, 0.1) {
        y + s * 0.15
    } else {
        y
    }
}

fn on_line(y: f64, near: f64) -> bool {
    (y - y.round()).abs() < near && (-0.5..=4.5).contains(&y)
}

/// The head a slur on a side of a note meets: its middle's y.
fn slur_head(ev: &Ev, over: bool) -> f64 {
    y_of(if over { ev.heads.last() } else { ev.heads.first() }.map_or(0, |h| h.pos))
}

/// The stem's middle, for a note whose stem is on the slur's side.
fn slur_stem(ev: &Ev, over: bool) -> Option<f64> {
    (ev.base > 1 && over == ev.up && !ev.look.omitted("Stem")).then(|| stem_x(ev) + if ev.up { -RULE.stem / 2.0 } else { RULE.stem / 2.0 })
}

/// Where a slur meets an end note, as LilyPond's base attachments: half a
/// space off the head — beside the stem when the stem is on its side — or
/// half a space off the stem's end, when a beam goes on from there under the
/// slur (`beam_in`) or a flag hangs where the slur would start.
fn slur_tip(ev: &Ev, over: bool, left: bool, beam_in: bool) -> Tip {
    let s = if over { -1.0 } else { 1.0 };
    let by_head = off_line(slur_head(ev, over) + s, s);
    let Some(sx) = slur_stem(ev, over) else {
        return Tip { x: ev.x + ev.head_w.unwrap_or(0.0) / 2.0, y: by_head, stem: None, pull: SLUR_EDGE, beamed: false };
    };
    let flagged = ev.beam.is_none() && ev.base >= 8 && !ev.xbeam;
    if beam_in || (left && flagged) {
        return Tip { x: sx, y: ev.stem_end + s * 0.5, stem: None, pull: SLUR_EDGE, beamed: beam_in };
    }
    Tip { x: sx, y: by_head, stem: Some((sx, ev.stem_end)), pull: SLUR_EDGE_STEM, beamed: false }
}

/// The articulations a slur on a side of a note goes round, as boxes
/// `[left, top, right, bottom]`.
fn slur_scripts(ev: &Ev, over: bool) -> Vec<[f64; 4]> {
    if ev.scripts.is_empty() {
        return Vec::new();
    }
    let stack = script_stack(ev, &[]);
    let mid = (slur_head(ev, true) + slur_head(ev, false)) / 2.0;
    stack.placed.iter().zip(&stack.inside).filter(|(_, inside)| **inside).map(|(sc, _)| sc.bbox()).filter(|b| ((b[1] + b[3]) / 2.0 < mid) == over).collect()
}

/// What a slur clears of a note under it: `(x, y, how far)`. `last` for
/// the note it ends on, of which it clears the accidental alone.
fn slur_avoids(ev: &Ev, over: bool, last: bool, out: &mut Vec<(f64, f64, f64)>) {
    let s = if over { -1.0 } else { 1.0 };
    for hd in &ev.heads {
        let Some(acc) = hd.acc else { continue };
        let bb = glyph(crate::engrave::acc_name(acc)).bbox;
        let x = ev.x + hd.acc_x.unwrap_or(0.0);
        let y = y_of(hd.pos) + if over { bb[1] } else { bb[3] };
        let clear = if last { SLUR_CLEAR_LAST } else { SLUR_CLEAR };
        out.extend(slur_box([x + bb[0], y, x + bb[2], y], over, clear));
    }
    if last {
        return;
    }
    let (y, w) = (slur_head(ev, over), ev.head_w.unwrap_or(0.0));
    out.extend([(ev.x + w / 2.0, y + s * 0.5, SLUR_CLEAR), (ev.x + 0.1 * w, y + s * 0.3, SLUR_CLEAR), (ev.x + 0.9 * w, y + s * 0.3, SLUR_CLEAR)]);
    if let Some(sx) = slur_stem(ev, over) {
        out.extend(slur_box([sx, ev.stem_end, sx, ev.stem_end], over, SLUR_CLEAR));
    }
    for b in slur_scripts(ev, over) {
        out.extend(slur_box(b, over, SLUR_CLEAR));
    }
}

/// What a slur clears of a box `[left, top, right, bottom]`: its edge on the
/// slur's side by `clear`, and a little to each side of it — a slur that
/// comes down steeply past a corner keeps off it too.
fn slur_box(b: [f64; 4], over: bool, clear: f64) -> [(f64, f64, f64); 5] {
    let y = if over { b[1] } else { b[3] };
    let beside = (clear - 0.2).max(0.0);
    [(b[0], y, clear), ((b[0] + b[2]) / 2.0, y, clear), (b[2], y, clear), (b[0] - beside, y, 0.2), (b[2] + beside, y, 0.2)]
}

/// A slur's shape between two ends: how far out of the line between them
/// its two control points stand, and how far in from its ends.
#[derive(Clone, Copy)]
struct Bow {
    xa: f64,
    ya: f64,
    xb: f64,
    yb: f64,
    /// The control point after its start, out of the line.
    ca: f64,
    /// … before its end.
    cb: f64,
    inset: f64,
}

impl Bow {
    /// Its middle line's control points, arched to the side `s`.
    fn points(&self, s: f64) -> [(f64, f64); 4] {
        [(self.xa, self.ya), (self.xa + self.inset, self.ya + s * self.ca), (self.xb - self.inset, self.yb + s * self.cb), (self.xb, self.yb)]
    }
}

/// How far out its control points stand for a slur of LilyPond's height
/// over `width`.
fn slur_plain(width: f64) -> f64 {
    curve_height(width, 2.0, 0.25) * 4.0 / 3.0
}

/// The least arch from `(xa, ya)` to `(xb, yb)` that clears every point of
/// `avoid` on the side `s`: LilyPond's height for its width, or higher —
/// on the side of what is in its way more than on the other, and never
/// more than twice as high on one side as on the other.
fn slur_bow(avoid: &[(f64, f64, f64)], xa: f64, ya: f64, xb: f64, yb: f64, s: f64) -> Bow {
    let w = xb - xa;
    let inset = w * 0.25;
    let (mut ca, mut cb) = (slur_plain(w), slur_plain(w));
    for &(px, py, clear) in avoid {
        if px <= xa + 0.05 || px >= xb - 0.05 {
            continue;
        }
        // Where along the curve it passes `px`: its x grows with t.
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for _ in 0..22 {
            let t = (lo + hi) / 2.0;
            let u = 1.0 - t;
            let x = xa * u * u * u + 3.0 * (xa + inset) * u * u * t + 3.0 * (xb - inset) * u * t * t + xb * t * t * t;
            if x < px {
                lo = t;
            } else {
                hi = t;
            }
        }
        let t = (lo + hi) / 2.0;
        let u = 1.0 - t;
        // There, y = chord + s · (b1 · ca + b2 · cb).
        let chord = ya + (yb - ya) * (3.0 * t * t - 2.0 * t * t * t);
        let (b1, b2) = (3.0 * u * u * t, 3.0 * u * t * t);
        let short = clear + RULE.curve_mid / 2.0 + s * (py - chord) - (b1 * ca + b2 * cb);
        if short > 0.0 {
            let k = short / (b1 * b1 + b2 * b2).max(1e-9);
            ca += k * b1;
            cb += k * b2;
        }
    }
    let least = ca.max(cb) / 2.0;
    Bow { xa, ya, xb, yb, ca: ca.max(least), cb: cb.max(least), inset }
}

/// What an arch higher than its width's costs: little for a little, and
/// more and more — a slur lifts its ends rather than balloon.
fn slur_height_cost(bow: &Bow) -> f64 {
    let over = (bow.ca.max(bow.cb) - slur_plain(bow.xb - bow.xa)).max(0.0) * 0.75;
    2.0 * over + 0.5 * over * over
}

/// A slur from `ends.0` to `ends.1`; `forced` the direction a setting gives
/// it (`\slurUp`: 1), over the one its notes and voice give it; `out` how
/// much farther out each end stands (a phrasing slur's over a slur's).
#[allow(clippy::too_many_arguments)]
fn slur(lines: &mut [Line], bars: &[Bar], notes: &[At], ends: (At, At), kind: CurveKind, out: (f64, f64), forced: Option<i64>, drawn: &mut Vec<Drawn>) {
    let ev_at = |at: At| -> &Ev { &bars[at.0].items[at.1] };
    let (f, l) = (ev_at(ends.0), ev_at(ends.1));
    let (fnum, lnum) = (f.n.unwrap_or(0), l.n.unwrap_or(0));
    let ups: Vec<&Ev> = notes.iter().map(|&a| ev_at(a)).filter(|ev| ev.n.unwrap_or(0) >= fnum && ev.n.unwrap_or(0) <= lnum && voice_of(ev) == voice_of(f)).collect();
    let all_up = ups.iter().all(|ev| ev.up);
    let mut over = !all_up;
    // Of two voices, the first's slurs above and the second's below.
    if let Some(v) = f.voice.filter(|_| !f.free) {
        over = v % 2 == 1;
    }
    if let Some(d) = forced {
        over = d > 0;
    }
    let s = if over { -1.0 } else { 1.0 };
    // A beam that goes on under the slur from its first note, or comes
    // from under it to its last.
    let beam_in = |at: At, left: bool| {
        let ev = ev_at(at);
        ev.beam.and_then(|g| bars[at.0].beams.get(g)).is_some_and(|g| if left { g.notes.last() != Some(&at.1) } else { g.notes.first() != Some(&at.1) })
    };
    let mut tips = (slur_tip(f, over, true, beam_in(ends.0, true)), slur_tip(l, over, false, beam_in(ends.1, false)));
    // Past the articulations of the notes it starts and ends on: beyond one
    // over its end, round one beside it.
    let mut end_avoid: Vec<(usize, f64, f64, f64)> = Vec::new();
    for (tip, ev, push) in [(&mut tips.0, f, out.0), (&mut tips.1, l, out.1)] {
        for b in slur_scripts(ev, over) {
            if tip.stem.is_none() && b[0] < tip.x + 0.4 && b[2] > tip.x - 0.4 {
                tip.y = s * (s * tip.y).max(s * if over { b[1] } else { b[3] } + 0.35);
            } else {
                end_avoid.extend(slur_box(b, over, SLUR_CLEAR).map(|a| (ev.sys, a.0, a.1, a.2)));
            }
        }
        tip.y += s * push;
    }
    let from = Source { kind, item: f.i, at: f.at, head: None, piece: 0 };
    // What it goes round on a line: its own notes there, and for a
    // phrasing slur the slurs under it.
    let avoid_on = |sys: usize, drawn: &[Drawn]| -> Vec<(f64, f64, f64)> {
        let mut avoid: Vec<(f64, f64, f64)> = end_avoid.iter().filter(|a| a.0 == sys).map(|a| (a.1, a.2, a.3)).collect();
        for ev in ups.iter().filter(|ev| ev.sys == sys && ev.n != f.n) {
            slur_avoids(ev, over, ev.n == l.n, &mut avoid);
        }
        if kind == CurveKind::Phrase {
            for d in drawn.iter().filter(|d| d.sys == sys && d.voice == voice_of(f) && d.first <= lnum && d.last >= fnum) {
                for k in 1..10 {
                    let (x, y) = bezier(&d.points, f64::from(k) / 10.0);
                    avoid.push((x, y, SLUR_CLEAR));
                }
            }
        }
        avoid
    };
    let grob = if kind == CurveKind::Phrase { "PhrasingSlur" } else { "Slur" };
    let keep = |line: &mut Line, drawn: &mut Vec<Drawn>, sys: usize, bow: Bow, from: Source| {
        let points = shaped(bow.points(s), &f.look, grob, from.piece);
        drawn.push(Drawn { sys, voice: voice_of(f), first: fnum, last: lnum, points });
        line.curves.push(curve_outline(&points, s));
        line.curve_places.push(CurvePlace { kind: from.kind, item: from.item, at: from.at, head: None, points, up: over, piece: from.piece });
    };
    if f.sys == l.sys {
        let avoid = avoid_on(f.sys, drawn);
        // The notes' own slope, head to head.
        let rise = slur_head(l, over) - slur_head(f, over);
        let beams = tips.0.beamed || tips.1.beamed;
        let stems = tips.0.stem.is_some() || tips.1.stem.is_some();
        let mut best: Option<(f64, Bow)> = None;
        for la in SLUR_LIFTS {
            for lb in SLUR_LIFTS {
                let ((xa, ya), (xb, yb)) = (tips.0.at(la, s, true), tips.1.at(lb, s, false));
                if xb - xa <= 0.2 {
                    continue;
                }
                let bow = slur_bow(&avoid, xa, ya, xb, yb, s);
                let dy = yb - ya;
                let mut cost = tips.0.pull * la + tips.1.pull * lb + slur_height_cost(&bow);
                cost += SLUR_STEEPER * (dy.abs() - rise.abs() - 0.2 - if beams { 1.0 } else { 0.0 }).max(0.0);
                cost += SLUR_MAX_SLOPE_FACTOR * ((dy / (xb - xa)).abs() - SLUR_MAX_SLOPE).max(0.0);
                if rise == 0.0 && dy.abs() > 1e-6 {
                    cost += SLUR_TILTED;
                }
                if rise * dy < -1e-9 {
                    // A stem under an end tilts a slur as a beam does: it
                    // starts by the stem's end to clear the stems after it.
                    cost += if beams || stems { SLUR_AGAINST / 10.0 } else { SLUR_AGAINST };
                }
                cost += [ya, yb].iter().filter(|y| on_line(**y, 0.12)).count() as f64 * SLUR_ON_LINE;
                if best.as_ref().is_none_or(|b| cost < b.0 - 1e-9) {
                    best = Some((cost, bow));
                }
            }
        }
        if let Some((_, bow)) = best {
            keep(&mut lines[f.sys], drawn, f.sys, bow, from);
        }
        return;
    }
    // Over a line's end: level to the end of the first line, and from the
    // start of the last.
    let end = lines[f.sys].bar_end - 0.5;
    let start = lines[l.sys].prefix - 0.6;
    for (sys, tip, left, other, piece) in [(f.sys, tips.0, true, end, 0), (l.sys, tips.1, false, start, 1)] {
        let avoid = avoid_on(sys, drawn);
        let mut best: Option<(f64, Bow)> = None;
        for lift in SLUR_LIFTS {
            let (x, y) = tip.at(lift, s, left);
            // Its open end, at the line's: level with the other, or
            // farther out to clear what is between.
            for open in SLUR_LIFTS {
                let far = y + s * open;
                let (xa, ya, xb, yb) = if left { (x, y, other, far) } else { (other, far, x, y) };
                if xb - xa <= 0.2 {
                    continue;
                }
                let bow = slur_bow(&avoid, xa, ya, xb, yb, s);
                let cost = tip.pull * lift + SLUR_EDGE_STEM * open + slur_height_cost(&bow) + if on_line(y, 0.12) { SLUR_ON_LINE } else { 0.0 };
                if best.as_ref().is_none_or(|b| cost < b.0 - 1e-9) {
                    best = Some((cost, bow));
                }
            }
        }
        if let Some((_, bow)) = best {
            keep(&mut lines[sys], drawn, sys, bow, Source { piece, ..from });
        }
    }
}

/// A point of a cubic curve.
pub(crate) fn bezier(p: &[(f64, f64); 4], t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (a * p[0].0 + b * p[1].0 + c * p[2].0 + d * p[3].0, a * p[0].1 + b * p[1].1 + c * p[2].1 + d * p[3].1)
}

/// A curve's control points moved as `\shape` says for this piece of it:
/// each by its offset, up for a positive `dy`.
fn shaped(points: [(f64, f64); 4], look: &crate::look::Look, grob: &str, piece: usize) -> [(f64, f64); 4] {
    let Some(by) = look.shape(grob, piece) else { return points };
    [0, 1, 2, 3].map(|k| (points[k].0 + by[k].0, points[k].1 - by[k].1))
}

/// A tie: LilyPond's height for its width; `look` its note's settings.
#[allow(clippy::too_many_arguments)]
fn arc(line: &mut Line, look: &crate::look::Look, from: Source, xa: f64, ya: f64, xb: f64, yb: f64, dir: f64) {
    let hgt = curve_height((xb - xa).max(0.1), 1.0, 0.333);
    if let Some(mut cmds) = arc_cmds(xa, ya, xb, yb, dir, hgt) {
        let inset = (xb - xa) * 0.25;
        let c = hgt * 4.0 / 3.0;
        let plain = [(xa, ya), (xa + inset, ya + dir * c), (xb - inset, yb + dir * c), (xb, yb)];
        let points = shaped(plain, look, "Tie", from.piece);
        if points != plain {
            cmds = curve_outline(&points, dir);
        }
        line.curves.push(cmds);
        line.curve_places.push(CurvePlace { kind: from.kind, item: from.item, at: from.at, head: from.head, points, up: dir < 0.0, piece: from.piece });
    }
}

/// A curve as a closed outline round its middle line's control points: out
/// along the outer Bézier, back along the inner one, `RULE.curve_tip` thick
/// at the ends and `curve_mid` between; `dir` the side it arches to.
pub(crate) fn curve_outline(p: &[(f64, f64); 4], dir: f64) -> Vec<f64> {
    let m = dir * RULE.curve_mid / 2.0 * 4.0 / 3.0;
    let t2 = dir * RULE.curve_tip / 2.0;
    vec![0.0, p[0].0, p[0].1 + t2, 3.0, p[1].0, p[1].1 + m, p[2].0, p[2].1 + m, p[3].0, p[3].1 + t2, 1.0, p[3].0, p[3].1 - t2, 3.0, p[2].0, p[2].1 - m, p[1].0, p[1].1 - m, p[0].0, p[0].1 - t2, 4.0]
}

/// A curve's outline from its ends and its height ([`curve_outline`]), or
/// none for a curve with no width.
pub(crate) fn arc_cmds(xa: f64, ya: f64, xb: f64, yb: f64, dir: f64, hgt: f64) -> Option<Vec<f64>> {
    let w = xb - xa;
    if w <= 0.0 {
        return None;
    }
    let inset = w * 0.25;
    let c_out = (hgt + RULE.curve_mid / 2.0) * 4.0 / 3.0;
    let c_in = (hgt - RULE.curve_mid / 2.0) * 4.0 / 3.0;
    let t2 = RULE.curve_tip / 2.0;
    Some(vec![
        0.0,
        xa,
        ya + dir * t2,
        3.0,
        xa + inset,
        ya + dir * c_out,
        xb - inset,
        yb + dir * c_out,
        xb,
        yb + dir * t2,
        1.0,
        xb,
        yb - dir * t2,
        3.0,
        xb - inset,
        yb + dir * c_in,
        xa + inset,
        ya + dir * c_in,
        xa,
        ya - dir * t2,
        4.0,
    ])
}

/// The points of a fill's commands, `(x, y)`.
pub(crate) fn points(cmds: &[f64]) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut ci = 0;
    while ci < cmds.len() {
        let args = args_of(cmds[ci]);
        for jj in (0..args).step_by(2) {
            if let (Some(x), Some(y)) = (cmds.get(ci + 1 + jj), cmds.get(ci + 2 + jj)) {
                out.push((*x, *y));
            }
        }
        ci += 1 + args;
    }
    out
}

// ---- Marks ------------------------------------------------------------------

/// Everything around a staff's notes, line by line.
pub(crate) fn marks(bars: &mut [Bar], lines: &mut [Line]) {
    hairpins(bars, lines);
    let brackets = sostenuto_spans(bars, lines);
    for (li, line) in lines.iter_mut().enumerate() {
        line.tuplets = tuplets(bars, line);
        scripts(bars, line);
        dynamics(bars, line);
        texts(bars, line);
        pedal_lines(bars, line, brackets.get(li).map_or(&[][..], Vec::as_slice));
        // After what stands above the staff: a bracket goes over the words,
        // the scripts and the tuplets under it, as LilyPond's
        // `outside-staff-priority` puts a VoltaBracket over TextScripts.
        line.voltas = if line.staff_index.unwrap_or(0) == 0 { voltas(bars, line) } else { Vec::new() };
        lyric_line(bars, line);
        line.marks = Vec::new();
        let range = line.bars.clone();
        for (bi, b) in range.clone().enumerate() {
            for (ii, ev) in bars[b].items.iter().enumerate() {
                let Some(mark) = &ev.tempo else { continue };
                let mut x = ev.x;
                if bi == 0 && ii == 0 {
                    x = line.left + 0.8 + crate::engrave::clef_advance(bars[b].clef) + 1.0;
                }
                let w = mark_width(mark);
                let mut low: f64 = -1.0;
                for ob in range.clone() {
                    for other in &bars[ob].items {
                        if other.x + 1.5 < x || other.x > x + w + 1.0 || other.k != Kind::Note {
                            continue;
                        }
                        low = low.min(y_of(other.heads.last().map_or(0, |h| h.pos)) - 0.7);
                        low = low.min(other.stem_end - 0.3);
                    }
                }
                for tp in &line.tuplets {
                    if !tp.up || tp.x1 < x - 1.0 || tp.x0 > x + w + 1.0 {
                        continue;
                    }
                    low = low.min(tp.ya.min(tp.yb) - 1.0);
                }
                for ob in range.clone() {
                    for other in &bars[ob].items {
                        if other.x + 1.5 < x || ink_left(other) > x + w + 1.0 {
                            continue;
                        }
                        low = low.min(other.reach_up.unwrap_or(0.0) - 0.3);
                    }
                }
                for tx in &line.texts {
                    if tx.dir > 0 || tx.x1 < x - 1.0 || tx.x0 > x + w + 1.0 {
                        continue;
                    }
                    low = low.min(tx.top - 0.3);
                }
                for c in &line.curves {
                    for (px, py) in points(c) {
                        if px >= x - 1.0 && px <= x + w + 1.0 {
                            low = low.min(py - 0.2);
                        }
                    }
                }
                // `padding` over what is under it; `X-offset` from the note,
                // `Y-offset` from the middle line, `extra-offset` after it all.
                let look = &ev.look;
                let mut y = low - look.num("MetronomeMark.padding").map_or(0.9, |p| p + 0.4);
                if let Some(dx) = look.num("MetronomeMark.X-offset") {
                    x = ev.x + dx;
                }
                if let Some(v) = look.num("MetronomeMark.Y-offset") {
                    y = 2.0 - v;
                }
                let (dx, dy) = look.extra("MetronomeMark");
                line.marks.push(TempoMark { x: x + dx, y: y + dy, mark: mark.clone() });
            }
        }
        line.ottavas = ottavas(bars, line);
        line.rehearsal = if line.staff_index.unwrap_or(0) == 0 { rehearsal(bars, line) } else { Vec::new() };
    }
    text_spans(bars, lines);
    for line in lines.iter_mut() {
        // A movement's title over all the line holds, as LilyPond sets
        // `scoreTitleMarkup` over its first system.
        line.title_y = None;
        if line.title.as_ref().is_some_and(|t| !(t.0.is_empty() && t.1.is_empty())) && line.staff_index.unwrap_or(0) == 0 {
            let (hi, _) = reach(bars, line);
            line.title_y = Some(hi - 1.0 - 0.25 * em(0.0));
        }
    }
}

/// Text spanners, `\startTextSpan` to `\stopTextSpan` (or the staff's end):
/// over the notes they span, the words they start with on their first
/// line, a dashed line on to the last note.
// Each line is read, then pushed to: an index, not an iterator.
#[allow(clippy::needless_range_loop)]
fn text_spans(bars: &[Bar], lines: &mut [Line]) {
    let mut flat: Vec<(usize, usize, usize)> = Vec::new();
    for (si, line) in lines.iter_mut().enumerate() {
        line.spans = Vec::new();
        for b in line.bars.clone() {
            for j in 0..bars[b].items.len() {
                flat.push((si, b, j));
            }
        }
    }
    let ev_of = |f: (usize, usize, usize)| -> &Ev { &bars[f.1].items[f.2] };
    let e = em(0.0);
    for (k, &at) in flat.iter().enumerate() {
        let Some(runs) = &ev_of(at).span_start else { continue };
        let stop = flat.iter().enumerate().skip(k + 1).find(|(_, f)| ev_of(**f).span_stop).map_or(flat.len() - 1, |(j, _)| j);
        let end = flat[stop];
        let (first, last) = (at.0, end.0);
        for si in first..=last {
            let line = &lines[si];
            let x0 = if si == first { ev_of(at).x } else { line.prefix };
            let x1 = if si == last { ev_of(end).x + item_w(ev_of(end)) } else { line.right };
            let words = if si == first { runs.clone() } else { Vec::new() };
            let (bt, bb) = if words.is_empty() { (-0.4, 0.4) } else { runs_box(&words, e) };
            let mut edge: f64 = -0.5;
            for b in line.bars.clone() {
                for ev in &bars[b].items {
                    if ev.x + item_w(ev) >= x0 - 0.2 && ink_left(ev) <= x1 + 0.2 {
                        edge = edge.min(ev.reach_up.unwrap_or(0.0));
                        if ev.k == Kind::Note {
                            edge = edge.min(ev.stem_end - 0.3);
                        }
                    }
                }
            }
            for tx in &line.texts {
                if tx.dir < 0 && tx.x1 > x0 && tx.x0 < x1 {
                    edge = edge.min(tx.top - 0.3);
                }
            }
            for tp in &line.tuplets {
                if tp.up && tp.x1 > x0 && tp.x0 < x1 {
                    edge = edge.min(tp.ya.min(tp.yb) - 0.9);
                }
            }
            let y = edge - 0.6 - bb;
            let _ = bt;
            lines[si].spans.push(SpanPlace { runs: words, x0, x1: x1.max(x0 + 1.0), y });
        }
    }
}

/// The bar numbers a line prints, as boxes: their left, right and top —
/// what words and a rehearsal mark over the staff stand clear of, as
/// LilyPond's BarNumber is nearer the staff than either.
pub(crate) fn bar_number_boxes(bars: &[Bar], line: &Line) -> Vec<(f64, f64, f64)> {
    let mut out = Vec::new();
    if line.staff_index.unwrap_or(0) != 0 {
        return out;
    }
    let e = em(-2.0);
    for (bi, b) in line.bars.clone().enumerate() {
        let bar = &bars[b];
        let Some(look) = bar.items.first().map(|ev| &ev.look).filter(|l| !l.is_empty()) else { continue };
        if bar.number < 1 || (bi == 0 && bar.first) || !crate::look::bar_number_shown(look, bar.number, bi == 0, bar.number <= 1) {
            continue;
        }
        let text = bar.number.to_string();
        let w = text_width(&text, Weight::Regular, e);
        let (t, _) = text_box(&text, Weight::Regular, e);
        let x0 = if bi == 0 { line.left } else { bar.x0 - barline_width(bars[b - 1].drawn()) / 2.0 - w / 2.0 };
        out.push((x0, x0 + w, -1.6 + t));
    }
    out
}

/// The size of an ottava's words, and of a rehearsal mark's.
pub(crate) fn ottava_em() -> f64 {
    em(-1.0)
}

/// The rehearsal mark's size: LilyPond's `font-size` 2.
pub(crate) fn mark_em() -> f64 {
    em(2.0)
}

/// `\ottava`'s brackets on a line: one a run of notes and rests at the same
/// octaves, from its first to its last, above the notes for an octave up and
/// under them for one down, a hook at the end of the run.
fn ottavas(bars: &[Bar], line: &Line) -> Vec<OttavaPlace> {
    let items: Vec<&Ev> = line.bars.clone().flat_map(|b| bars[b].items.iter()).filter(|ev| ev.percent.is_none()).collect();
    let mut out = Vec::new();
    let mut k = 0;
    while k < items.len() {
        let o = items[k].ottava;
        if o == 0 {
            k += 1;
            continue;
        }
        let mut j = k;
        while j + 1 < items.len() && items[j + 1].ottava == o {
            j += 1;
        }
        let first = items[k];
        let last = items[j];
        let x0 = first.x - first.left.min(0.5);
        let x1 = last.x + item_w(last) + 0.5;
        let up = o > 0;
        let text = if o.abs() == 1 { "8" } else { "15" };
        let e = ottava_em();
        let (bt, bb) = text_box(text, Weight::Bold, e);
        let near = |ev: &&&Ev| ev.x + item_w(ev) >= x0 - 0.2 && ink_left(ev) <= x1 + 0.2;
        let y = if up {
            let edge = items.iter().filter(near).map(|ev| ev.reach_up.unwrap_or(0.0)).fold(-0.5, f64::min);
            edge - 0.6 - bb
        } else {
            let edge = items.iter().filter(near).map(|ev| ev.reach_down.unwrap_or(4.0)).fold(4.5, f64::max);
            edge + 0.6 - bt
        };
        let hook = j + 1 < items.len() || line.bars.end >= bars.len() || bars[line.bars.end].items.first().is_none_or(|n| n.ottava != o);
        out.push(OttavaPlace { text: text.into(), x0, x1, y, up, hook });
        k = j + 1;
    }
    out
}

/// Where a line's rehearsal marks go: over the bar line they are written
/// at, at the start of the line over its first note's room, above all the
/// line holds there.
fn rehearsal(bars: &[Bar], line: &Line) -> Vec<MarkPlace> {
    let mut out = Vec::new();
    let e = mark_em();
    for (bi, b) in line.bars.clone().enumerate() {
        let bar = &bars[b];
        for (pos, words) in &bar.marks {
            // Its formatter's frame, its alignment and its moves (score #403).
            let (style, runs) = crate::look::mark_style(&words.pieces());
            let runs = if style.bars { vec![Run::words(bar.number.to_string(), Weight::Bold)] } else { runs };
            let framed = if style.frame.is_empty() { 0.0 } else { FRAME_PAD };
            let w = runs_width(&runs, e);
            let (_, foot) = runs_box(&runs, e);
            let (anchor, align) = if *pos >= bar.length && words.dir > 0 {
                // A jump ends at its bar line, as LilyPond right-aligns it.
                (bar.x1 + barline_width(bar.drawn()) / 2.0, 1.0)
            } else if *pos >= bar.length {
                (bar.x1 + barline_width(bar.drawn()) / 2.0, 0.0)
            } else if *pos == 0 && bi == 0 {
                (line.left, -1.0)
            } else if *pos == 0 {
                (bar.x0 - barline_width(&bars[b - 1].bar) / 2.0, 0.0)
            } else {
                (bar.items.iter().find(|ev| ev.start >= *pos).map_or(bar.x0, |ev| ev.x), 0.0)
            };
            let align = style.align.unwrap_or(align).clamp(-1.0, 1.0);
            let anchor = anchor + style.x.unwrap_or(0.0);
            let x = anchor - w * (align + 1.0) / 2.0 + if *pos == 0 && bi == 0 { framed } else { 0.0 };
            let mut low: f64 = -1.5;
            for ob in line.bars.clone() {
                for other in &bars[ob].items {
                    if other.x + item_w(other) < x - 0.5 || ink_left(other) > x + w + 0.5 {
                        continue;
                    }
                    low = low.min(other.reach_up.unwrap_or(0.0));
                    if other.k == Kind::Note {
                        low = low.min(other.stem_end - 0.3);
                    }
                }
            }
            for mk in &line.marks {
                if mk.x < x + w + 0.5 && mk.x + mark_width(&mk.mark) > x - 0.5 {
                    low = low.min(mk.y - 1.8);
                }
            }
            for vb in &line.voltas {
                if vb.x0 < x + w && vb.x1 > x {
                    low = low.min(vb.y - 0.4);
                }
            }
            // Two marks at one bar line — a D.S. ending a bar, a letter
            // starting the next — stand one over the other.
            for placed in &out {
                let MarkPlace { runs: other, x: ox, y: oy, .. } = placed;
                if *ox < x + w + 0.3 && *ox + runs_width(other, e) > x - 0.3 {
                    let (top, _) = runs_box(other, e);
                    low = low.min(oy + top - 0.4 + 0.8 + foot);
                }
            }
            for (nx0, nx1, ntop) in bar_number_boxes(bars, line) {
                if nx0 < x + w + 0.3 && nx1 > x - 0.3 {
                    low = low.min(ntop - 0.2);
                }
            }
            // Over the words above the staff, as LilyPond's priorities stack them.
            for tx in &line.texts {
                if tx.dir < 0 && tx.x0 < x + w + 0.3 && tx.x1 > x - 0.3 {
                    low = low.min(tx.top - 0.2);
                }
            }
            // An ottava's figure just after the bar line still sits under it.
            for ot in &line.ottavas {
                if ot.up && ot.x0 < x + w + 1.0 && ot.x1 > x - 1.0 {
                    low = low.min(ot.y - 1.6);
                }
            }
            let mut y = low - style.pad.unwrap_or(0.8) - foot - framed;
            if let Some(v) = style.y {
                y = 2.0 - v;
            }
            out.push(MarkPlace { runs, x: x + style.extra.0, y: y + style.extra.1, frame: style.frame });
        }
    }
    out
}

fn tuplets(bars: &[Bar], line: &Line) -> Vec<TupletPlace> {
    // Each group: its id, its number, whether it holds another, its notes.
    let mut groups: Vec<(i64, i64, bool, Vec<&Ev>)> = Vec::new();
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            for (t, outer) in [(ev.tuplet, false), (ev.tuplet_outer, true)] {
                let Some(t) = t else { continue };
                match groups.iter_mut().find(|g| g.0 == t.id) {
                    Some(g) => g.3.push(ev),
                    None => groups.push((t.id, t.n, outer, vec![ev])),
                }
            }
        }
    }
    groups
        .iter()
        .map(|(_, n, outer, items)| {
            // `\tupletUp`, `\omit TupletNumber`, `\omit TupletBracket`.
            let look = &items[0].look;
            let forced = look.dir("TupletBracket").or_else(|| look.dir("TupletNumber")).map(|d| d > 0);
            let mut tp = tuplet_place(bars, items, *n, *outer, forced);
            if look.omitted("TupletNumber") {
                tp.label = String::new();
            }
            if look.omitted("TupletBracket") {
                tp.bracket = false;
            }
            tp
        })
        .collect()
}

fn tuplet_place(bars: &[Bar], items: &[&Ev], n: i64, outer: bool, forced: Option<bool>) -> TupletPlace {
    let notes: Vec<&Ev> = items.iter().copied().filter(|ev| ev.k == Kind::Note).collect();
    let first_beam = notes.first().and_then(|ev| ev.beam.map(|b| (ev.bar, b)));
    let beamed = notes.len() == items.len() && notes.len() > 1 && first_beam.is_some() && notes.iter().all(|ev| ev.beam.map(|b| (ev.bar, b)) == first_beam);
    let ups = notes.iter().filter(|ev| ev.up).count();
    let beam_up = beamed && notes[0].beam.is_some_and(|b| bars[notes[0].bar].beams[b].up);
    // Set to the side away from its beam: placed as an unbeamed one is, bracket and all.
    let beamed = beamed && forced.is_none_or(|f| f == beam_up);
    let up = forced.unwrap_or(if beamed { beam_up } else { ups * 2 >= notes.len() });
    let last = items[items.len() - 1];
    let x0 = items[0].x - 0.2;
    let x1 = last.x + last.head_w.unwrap_or(1.2) + 0.2;
    let dir = if up { -1.0 } else { 1.0 };
    let mut ya;
    let mut yb;
    if beamed {
        let g = &bars[notes[0].bar].beams[notes[0].beam.unwrap_or(0)];
        ya = crate::engrave::beam_y(g, x0) + 1.1 * dir;
        yb = crate::engrave::beam_y(g, x1) + 1.1 * dir;
    } else {
        let edges: Vec<f64> = items.iter().map(|ev| tuplet_edge(ev, up)).collect();
        ya = edges[0] + 1.1 * dir;
        yb = edges[edges.len() - 1] + 1.1 * dir;
        yb = ya + (yb - ya).clamp(-1.0, 1.0);
        let span = (x1 - x0).max(0.01);
        let mut push: f64 = 0.0;
        for (k, ev) in items.iter().enumerate() {
            let at = ya + (yb - ya) * (ev.x - x0) / span;
            let need = edges[k] + 1.1 * dir;
            push = if up { push.min(need - at) } else { push.max(need - at) };
        }
        let reach = if up { ya.min(yb) + push + 0.7 } else { ya.max(yb) + push - 0.7 };
        push += if up { (-0.25 - reach).min(0.0) } else { (4.25 - reach).max(0.0) };
        ya += push;
        yb += push;
    }
    // A tuplet holding another stands beyond it, with its own bracket.
    if outer {
        ya += 1.8 * dir;
        yb += 1.8 * dir;
    }
    TupletPlace { x0, x1, ya, yb, up, bracket: !beamed || outer, label: n.to_string() }
}

/// How far a note or a rest reaches on one side, in staff y: its stem's end
/// or the outside of its heads; a rest, the middle of the staff and a space.
fn tuplet_edge(ev: &Ev, up: bool) -> f64 {
    if ev.k != Kind::Note {
        return if up { 1.0 } else { 3.0 } + rest_lift(ev);
    }
    let y_top = y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.5;
    let y_bottom = y_of(ev.heads.first().map_or(0, |h| h.pos)) + 0.5;
    if up {
        return y_top.min(if ev.up && ev.base > 1 { ev.stem_end } else { y_top });
    }
    y_bottom.max(if !ev.up && ev.base > 1 { ev.stem_end } else { y_bottom })
}

/// Where a rest of two voices stands: the first voice's two spaces up, the
/// second's two down; a rest of one voice on the middle of the staff.
pub(crate) fn rest_lift(ev: &Ev) -> f64 {
    if let Some(y) = ev.rest_y {
        return y;
    }
    match ev.voice {
        Some(1) => -2.0,
        Some(2) => 2.0,
        Some(3) => -3.0,
        Some(4) => 3.0,
        _ => 0.0,
    }
}

/// The voice a note sings in: the second, or the first, which a note
/// outside `<< \\\\ >>` goes on as.
pub(crate) fn voice_of(ev: &Ev) -> u8 {
    ev.voice.unwrap_or(1)
}

/// How far a note's ink reaches up and down — its grace notes' with it,
/// so what stands over the staff (a tempo mark, words) clears them too.
fn item_extent(ev: &Ev) -> (f64, f64) {
    let (up, down) = item_ink(ev);
    // A breath over the staff after it.
    if ev.breath.is_some() {
        return (up.min(-1.2), down);
    }
    (up, down)
}

fn item_ink(ev: &Ev) -> (f64, f64) {
    if ev.mm.is_some() {
        // The count over a gathered rest: its digits, centred a space and a
        // third over the staff.
        return (-2.3, 3.0);
    }
    if ev.k != Kind::Note {
        return (1.0 + rest_lift(ev), 3.0 + rest_lift(ev));
    }
    let (mut up, mut down) = (tuplet_edge(ev, true), tuplet_edge(ev, false));
    // Its accidentals: a natural or a flat stands a space above its head,
    // and the words over a bar's first note cleared its head but not its
    // sign (score #668).
    for hd in ev.heads.iter().filter(|hd| hd.acc.is_some()) {
        let g = glyph(crate::engrave::acc_name(hd.acc.unwrap_or(0)));
        up = up.min(y_of(hd.pos) + g.bbox[1]);
        down = down.max(y_of(hd.pos) + g.bbox[3]);
    }
    for g in ev.graces.iter().chain(&ev.after_graces) {
        for hd in &g.heads {
            up = up.min(y_of(hd.pos) - 0.4);
            down = down.max(y_of(hd.pos) + 0.4);
        }
        if g.base > 1 {
            up = up.min(g.stem_end);
            down = down.max(g.stem_end);
        }
    }
    // A whole note's tremolo, its strokes over or under its head: what
    // goes there clears them — a dynamic was drawn through them (score #982).
    if let Some(reach) = tremolo_reach(ev) {
        let ys = ev.heads.iter().map(|hd| y_of(hd.pos));
        if ev.up {
            up = up.min(ys.fold(f64::INFINITY, f64::min) - reach);
        } else {
            down = down.max(ys.fold(f64::NEG_INFINITY, f64::max) + reach);
        }
    }
    (up, down)
}

/// Where a note's ink starts: its head, or the accidental before it —
/// what stands over the staff clears the sign as it clears the head.
fn ink_left(ev: &Ev) -> f64 {
    ev.heads.iter().filter_map(|hd| hd.acc.map(|acc| ev.x + hd.acc_x.unwrap_or(0.0) + glyph(crate::engrave::acc_name(acc)).bbox[0])).fold(ev.x, f64::min)
}

pub(crate) fn item_w(ev: &Ev) -> f64 {
    if let Some(w) = ev.head_w {
        return w;
    }
    if ev.mm.is_some() {
        return (ev.span1 - ev.span0 - 2.0).max(1.0);
    }
    glyph(rest_name(ev.base)).advance
}

/// The articulations a slur goes round, with their note: LilyPond's
/// `avoid-slur` `inside`. The others — ornaments, fermatas, bowings,
/// fingerings — stand outside a slur that passes there.
const INSIDE_SLUR: [&str; 6] = ["staccato", "tenuto", "accent", "marcato", "staccatissimo", "portato"];

/// A note's articulations and fingerings where they stand, and how far
/// they reach.
pub(crate) struct Stack {
    /// Each one, nearest the note first on its side.
    pub(crate) placed: Vec<ScriptPlace>,
    /// Which of them a slur on their side goes round.
    pub(crate) inside: Vec<bool>,
    /// How far the stack reaches up.
    up: f64,
    /// … and down.
    down: f64,
}

/// One note's or rest's articulations and fingerings, stacked from its
/// head or its stem outward. `slurs` are the slurs drawn on its line, as
/// outlines: what is not an articulation next to the note (`INSIDE_SLUR`)
/// goes past a slur it would stand in. With none given, the stack is the
/// one the slurs themselves are drawn round.
pub(crate) fn script_stack(ev: &Ev, slurs: &[&[f64]]) -> Stack {
    let ext = item_extent(ev);
    let mut stack = Stack { placed: Vec::new(), inside: Vec::new(), up: ext.0, down: ext.1 };
    if ev.scripts.is_empty() && ev.fingers.is_empty() {
        return stack;
    }
    let xc = ev.x + item_w(ev) / 2.0;
    let note = ev.k == Kind::Note;
    let heads_top = if note { y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.5 } else { 1.0 + rest_lift(ev) };
    let heads_bottom = if note { y_of(ev.heads.first().map_or(0, |h| h.pos)) + 0.5 } else { 3.0 + rest_lift(ev) };
    let mut up_cursor = heads_top;
    let mut down_cursor = heads_bottom;
    if note && ev.up {
        up_cursor = ext.0;
    }
    if note && !ev.up {
        down_cursor = ext.1;
    }
    // Past a slur in the way: from `cursor` a box `(top, bottom)` high from
    // its origin, half `hw` wide, set `pad` off on the side `dir`.
    let past_slur = |cursor: f64, pad: f64, bb: (f64, f64), hw: f64, dir: i64| -> f64 {
        let mut cursor = cursor;
        for c in slurs {
            let Some((lo, hi)) = curve_span(c, xc - hw - 0.1, xc + hw + 0.1) else { continue };
            let y = if dir < 0 { cursor - pad - bb.1 } else { cursor + pad - bb.0 };
            if y + bb.1 + 0.1 > lo && y + bb.0 - 0.1 < hi {
                cursor = if dir < 0 { cursor.min(lo) } else { cursor.max(hi) };
            }
        }
        cursor
    };
    // A fingering stacks with the articulations, nearer the note than
    // a fermata, farther than a staccato (LilyPond's priority 100),
    // above unless written under.
    // A chord's: the upper half's above it, the lower half's under
    // it, each nearest its own head.
    let mut heads: Vec<i64> = ev.heads.iter().map(|h| h.d).collect();
    heads.sort_unstable();
    let finger_dir = |f: &crate::parse::Finger| match (f.dir, f.head) {
        (0, Some(d)) if heads.len() > 1 => {
            let rank = heads.iter().position(|&h| h == d).unwrap_or(0);
            if rank * 2 >= heads.len() {
                -1
            } else {
                1
            }
        }
        (0, _) => -1,
        (dir, _) => dir,
    };
    // `\override Fingering.direction`: where one written with `-` goes.
    let finger_side = ev.look.side("Fingering");
    let finger_dir = |f: &crate::parse::Finger| if f.dir == 0 { finger_side.unwrap_or_else(|| finger_dir(f)) } else { finger_dir(f) };
    let mut fingers: Vec<(usize, &crate::parse::Finger, i64)> = ev.fingers.iter().enumerate().map(|(k, f)| (k, f, finger_dir(f))).collect();
    fingers.sort_by_key(|(_, f, dir)| f.head.unwrap_or(0) * -dir);
    let mut list: Vec<(&str, i64, Option<&str>, bool, usize)> = ev.scripts.iter().enumerate().map(|(k, sc)| (sc.name.as_str(), sc.dir, None, false, k)).collect();
    list.extend(fingers.iter().map(|(k, f, dir)| ("", *dir, Some(f.text.as_str()), f.circled, *k)));
    list.sort_by_key(|sc| if sc.2.is_some() { 100 } else { script_rule(sc.0).prio });
    let finger_pad = ev.look.num("Fingering.padding").unwrap_or(0.5);
    // Once something that stands outside a slur is placed on a side, what
    // is stacked on it is outside too.
    let (mut out_up, mut out_down) = (false, false);
    for (name, sdir, figure, circled, index) in list {
        if let Some(figure) = figure {
            let bb = if circled { (-STRING_R - 0.05, STRING_R + 0.05) } else { figure_box(figure) };
            let hw = if circled { STRING_R } else { figure_width(figure) / 2.0 };
            let y;
            if sdir < 0 {
                up_cursor = past_slur(up_cursor, finger_pad, bb, hw, -1);
                y = up_cursor - finger_pad - bb.1;
                up_cursor = y + bb.0;
                out_up = true;
            } else {
                down_cursor = past_slur(down_cursor, finger_pad, bb, hw, 1);
                y = down_cursor + finger_pad - bb.0;
                down_cursor = y + bb.1;
                out_down = true;
            }
            stack.placed.push(ScriptPlace { glyph: "", x: xc, y, figure: Some(figure.to_owned()), k: if circled { STRING_K } else { FINGER_K }, circled, item: ev.i, index });
            stack.inside.push(false);
            continue;
        }
        let inside = INSIDE_SLUR.contains(&name);
        let mut rule = script_rule(name);
        if let Some(pad) = ev.look.num("Script.padding") {
            rule.pad = pad;
        }
        let mut dir = sdir;
        // `\override Script.direction`: where one written with `-` goes.
        if dir == 0 {
            dir = ev.look.side("Script").unwrap_or(0);
        }
        // Two voices: the first's over its stems, the second's under
        // them, as `\voiceOne` and `\voiceTwo` set Script.direction —
        // opposite the stem, the first's would sit in the second's notes.
        if dir == 0 {
            if let Some(v) = ev.voice.filter(|_| !ev.free) {
                dir = if v % 2 == 1 { -1 } else { 1 };
            }
        }
        if dir == 0 && rule.side != 0 {
            dir = rule.side;
        }
        if dir == 0 {
            dir = if ev.up { 1 } else { -1 };
        }
        let name = if dir < 0 { rule.up } else { rule.down };
        let bb = glyph(name).bbox;
        let hw = (bb[2] - bb[0]) / 2.0;
        let mut y;
        if dir < 0 {
            out_up |= !inside;
            if out_up {
                up_cursor = past_slur(up_cursor, rule.pad, (bb[1], bb[3]), hw, -1);
            }
            y = up_cursor - rule.pad - bb[3];
            if rule.quant {
                let mid = y + (bb[1] + bb[3]) / 2.0;
                if mid > -0.5 && mid < 4.5 {
                    y += space_above(mid) - mid;
                }
            } else if y + bb[3] > -0.25 {
                y = -0.25 - bb[3];
            }
            up_cursor = y + bb[1];
            stack.inside.push(!out_up);
        } else {
            out_down |= !inside;
            if out_down {
                down_cursor = past_slur(down_cursor, rule.pad, (bb[1], bb[3]), hw, 1);
            }
            y = down_cursor + rule.pad - bb[1];
            if rule.quant {
                let mid = y + (bb[1] + bb[3]) / 2.0;
                if mid > -0.5 && mid < 4.5 {
                    y += space_below(mid) - mid;
                }
            } else if y + bb[1] < 4.25 {
                y = 4.25 - bb[1];
            }
            down_cursor = y + bb[3];
            stack.inside.push(!out_down);
        }
        stack.placed.push(ScriptPlace { glyph: name, x: xc, y, figure: None, k: 1.0, circled: false, item: ev.i, index });
    }
    // Moved where a setting says: `X-offset` and `Y-offset` from the
    // note's head and the middle line, `extra-offset` from where they
    // were placed — after the placing, as LilyPond's moves them.
    for sc in &mut stack.placed {
        let grob = if sc.figure.is_some() { "Fingering" } else { "Script" };
        if let Some(x) = ev.look.num(&format!("{grob}.X-offset")) {
            sc.x = ev.x + x;
        }
        if let Some(y) = ev.look.num(&format!("{grob}.Y-offset")) {
            let mid = match &sc.figure {
                Some(f) => {
                    let bb = figure_box(f);
                    (bb.0 + bb.1) / 2.0
                }
                None => {
                    let bb = glyph(sc.glyph).bbox;
                    (bb[1] + bb[3]) / 2.0
                }
            };
            sc.y = 2.0 - y - mid;
        }
        let (dx, dy) = ev.look.extra(grob);
        sc.x += dx;
        sc.y += dy;
    }
    if ev.look.omitted("Script") {
        let keep: Vec<bool> = stack.placed.iter().map(|sc| sc.figure.is_some()).collect();
        let mut k = 0;
        stack.placed.retain(|_| {
            k += 1;
            keep[k - 1]
        });
        k = 0;
        stack.inside.retain(|_| {
            k += 1;
            keep[k - 1]
        });
    }
    stack.up = ext.0.min(up_cursor);
    stack.down = ext.1.max(down_cursor);
    stack
}

/// The articulations and fingerings of a line: each note's stack, the
/// articulations nearest the notes where the slurs were drawn round them
/// ([`curves`]), the rest past any slur in the way.
fn scripts(bars: &mut [Bar], line: &mut Line) {
    line.scripts = Vec::new();
    let slurs: Vec<&[f64]> = line.curve_places.iter().zip(&line.curves).filter(|(p, _)| p.kind != CurveKind::Tie).map(|(_, c)| c.as_slice()).collect();
    for b in line.bars.clone() {
        for ev in &mut bars[b].items {
            let stack = script_stack(ev, &slurs);
            ev.reach_up = Some(stack.up);
            ev.reach_down = Some(stack.down);
            line.scripts.extend(stack.placed);
        }
    }
}

/// How far a curve's outline reaches between `x0` and `x1`: its highest
/// and lowest y there, or none when it does not pass.
pub(crate) fn curve_span(cmds: &[f64], x0: f64, x1: f64) -> Option<(f64, f64)> {
    let mut span: Option<(f64, f64)> = None;
    let mut last: Option<(f64, f64)> = None;
    // Each stretch of the outline, cut to the window.
    let mut keep = |x: f64, y: f64, pen_up: bool| {
        if let Some((px, py)) = last.filter(|_| !pen_up) {
            let (lo, hi) = (px.min(x), px.max(x));
            if hi >= x0 && lo <= x1 {
                let at = |xx: f64| if (x - px).abs() < 1e-9 { y } else { py + (y - py) * (xx - px) / (x - px) };
                for yy in [at(lo.max(x0)), at(hi.min(x1))] {
                    span = Some(span.map_or((yy, yy), |s: (f64, f64)| (s.0.min(yy), s.1.max(yy))));
                }
                if (x - px).abs() < 1e-9 {
                    span = span.map(|s| (s.0.min(py), s.1.max(py)));
                }
            }
        }
        last = Some((x, y));
    };
    let (mut cx, mut cy) = (0.0, 0.0);
    let mut ci = 0;
    while ci < cmds.len() {
        match cmds[ci] as i64 {
            0 | 1 if ci + 2 < cmds.len() => {
                (cx, cy) = (cmds[ci + 1], cmds[ci + 2]);
                keep(cx, cy, cmds[ci] as i64 == 0);
                ci += 3;
            }
            3 if ci + 6 < cmds.len() => {
                let c = &cmds[ci + 1..ci + 7];
                for k in 1..=24 {
                    let t = f64::from(k) / 24.0;
                    let u = 1.0 - t;
                    let (a, b, d, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    keep(a * cx + b * c[0] + d * c[2] + e * c[4], a * cy + b * c[1] + d * c[3] + e * c[5], false);
                }
                (cx, cy) = (c[4], c[5]);
                ci += 7;
            }
            _ => ci += 1,
        }
    }
    span
}

fn space_above(y: f64) -> f64 {
    (y - 0.5).floor() + 0.5
}

fn space_below(y: f64) -> f64 {
    (y - 0.5).ceil() + 0.5
}

fn hairpins(bars: &[Bar], lines: &mut [Line]) {
    let mut flat: Vec<(usize, usize, usize)> = Vec::new();
    for (si, line) in lines.iter_mut().enumerate() {
        line.hairpins = Vec::new();
        for b in line.bars.clone() {
            for j in 0..bars[b].items.len() {
                flat.push((si, b, j));
            }
        }
    }
    let ev_of = |f: (usize, usize, usize)| -> &Ev { &bars[f.1].items[f.2] };
    for (k, &at) in flat.iter().enumerate() {
        let start_ev = ev_of(at);
        let Some(grow) = start_ev.hairpin else { continue };
        let mut stop = flat.len() - 1;
        let mut how = "end";
        for (jj, &f) in flat.iter().enumerate().skip(k + 1) {
            let other = ev_of(f);
            if other.hairpin_end {
                stop = jj;
                how = "note";
                break;
            }
            if other.dynamic.is_some() || other.hairpin.is_some() {
                stop = jj;
                how = "next";
                break;
            }
        }
        let mut x0 = start_ev.x;
        if let Some(d) = &start_ev.dynamic {
            x0 = x0.max(dyn_x(start_ev, d) + dyn_width(d) + 1.0);
        }
        let mut end_at = flat[stop];
        let end_ev = ev_of(end_at);
        let mut x1 = end_ev.x + item_w(end_ev) + 0.15;
        if how == "next" {
            x1 = match &end_ev.dynamic {
                None => end_ev.x - 0.6,
                Some(d) => dyn_x(end_ev, d) - 1.0,
            };
        }
        if how != "end" && end_ev.start == 0 && stop > k && bars[flat[stop - 1].1].number != bars[end_at.1].number {
            let before = &bars[flat[stop - 1].1];
            x1 = x1.min(before.x1 + barline_width(&before.bar) - 0.8);
            end_at = flat[stop - 1];
        }
        let first = at.0;
        let last = end_at.0;
        for (si, line) in lines.iter_mut().enumerate().take(last + 1).skip(first) {
            let a = if si == first { x0 } else { line.prefix };
            let b = if si == last { x1 } else { line.right };
            let parts = (last - first + 1) as f64;
            let ka = (si - first) as f64 / parts;
            let kb = (si - first + 1) as f64 / parts;
            let words = start_ev.hairpin_text.as_ref().map(|w| if si == first { w.clone() } else { String::new() });
            line.hairpins.push(HairpinPiece { x0: a, x1: b.max(a + 2.0), grow, from: ka, to: kb, words, look: start_ev.look.clone() });
        }
    }
}

pub(crate) fn dyn_letters(dynamic: &str) -> Vec<&'static str> {
    dynamic
        .chars()
        .map(|ch| match ch {
            'p' => "dynamicPiano",
            'm' => "dynamicMezzo",
            'f' => "dynamicForte",
            'r' => "dynamicRinforzando",
            's' => "dynamicSforzando",
            'z' => "dynamicZ",
            _ => "dynamicNiente",
        })
        .collect()
}

fn dyn_width(dynamic: &str) -> f64 {
    dyn_letters(dynamic).iter().fold(0.0, |acc, nm| acc + glyph(nm).advance)
}

fn dyn_x(ev: &Ev, dynamic: &str) -> f64 {
    ev.x + item_w(ev) / 2.0 - dyn_width(dynamic) / 2.0
}

/// Whether a dynamic or a hairpin goes over the staff: `\dynamicUp`,
/// `\override DynamicLineSpanner.direction = #UP`.
fn dyn_up(look: &crate::look::Look) -> bool {
    look.dir("DynamicLineSpanner").or_else(|| look.dir("DynamicText")) == Some(1)
}

/// How far a dynamic keeps from the staff and from the notes:
/// `staff-padding` and `padding`.
fn dyn_pads(look: &crate::look::Look) -> (f64, f64) {
    let staff = look.num("DynamicLineSpanner.staff-padding").or_else(|| look.num("DynamicText.staff-padding")).unwrap_or(0.6);
    let pad = look.num("DynamicLineSpanner.padding").or_else(|| look.num("DynamicText.padding")).unwrap_or(0.6);
    (staff, pad)
}

fn dynamics(bars: &[Bar], line: &mut Line) {
    let mut els: Vec<(DynEl, crate::look::Look)> = Vec::new();
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            let Some(d) = &ev.dynamic else { continue };
            if ev.look.omitted("DynamicText") {
                continue;
            }
            let mut x = dyn_x(ev, d);
            if let Some(dx) = ev.look.num("DynamicText.X-offset") {
                x = ev.x + dx;
            }
            let mut top: f64 = 0.0;
            let mut bottom: f64 = 0.0;
            for name in dyn_letters(d) {
                let bb = glyph(name).bbox;
                top = top.min(bb[1]);
                bottom = bottom.max(bb[3]);
            }
            els.push((DynEl { text: Some(d.clone()), x0: x, x1: x + dyn_width(d), top: 0.6 + top, bottom: 0.6 + bottom, ..DynEl::default() }, ev.look.clone()));
        }
    }
    for hp in &line.hairpins {
        els.push((DynEl { text: None, words: hp.words.clone(), x0: hp.x0, x1: hp.x1, grow: hp.grow, from: hp.from, to: hp.to, top: -0.6666, bottom: 0.6666, y: 0.0 }, hp.look.clone()));
    }
    els.sort_by(|a, b| a.0.x0.total_cmp(&b.0.x0));
    line.dynamics = Vec::new();
    // Under the staff, as LilyPond's DynamicLineSpanner; over it when set so.
    for up in [false, true] {
        let mine: Vec<&(DynEl, crate::look::Look)> = els.iter().filter(|e| dyn_up(&e.1) == up).collect();
        let mut k = 0;
        while k < mine.len() {
            let mut group = vec![k];
            let mut reach = mine[k].0.x1;
            k += 1;
            while k < mine.len() && mine[k].0.x0 - reach < 1.5 {
                group.push(k);
                reach = reach.max(mine[k].0.x1);
                k += 1;
            }
            let pads = dyn_pads(&mine[group[0]].1);
            let mut level: f64 = if up { f64::INFINITY } else { 0.0 };
            for &g in &group {
                level = if up { level.min(dyn_line_above(bars, line, &mine[g].0, pads)) } else { level.max(dyn_line(bars, line, &mine[g].0, pads)) };
            }
            for &g in &group {
                let (mut el, look) = mine[g].clone();
                el.y = level;
                el.bottom = level + el.bottom + if up { 0.0 } else { 0.3 };
                // `Y-offset` from the middle line, `extra-offset` from where it was put.
                if el.text.is_some() {
                    if let Some(v) = look.num("DynamicText.Y-offset") {
                        el.y = 2.0 - v - 0.6;
                    }
                    let (dx, dy) = look.extra("DynamicText");
                    el.x0 += dx;
                    el.x1 += dx;
                    el.y += dy;
                }
                line.dynamics.push(el);
            }
        }
    }
    line.dynamics.sort_by(|a, b| a.x0.total_cmp(&b.x0));
}

fn dyn_line(bars: &[Bar], line: &Line, el: &DynEl, pads: (f64, f64)) -> f64 {
    let (staff_pad, pad) = pads;
    let mut need = 4.0 + staff_pad - el.top;
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            let right = ev.x + item_w(ev);
            if right < el.x0 - 0.2 || ev.x > el.x1 + 0.2 {
                continue;
            }
            let mut rise = -el.top;
            if el.text.is_none() && el.words.is_none() {
                let span = (el.x1 - el.x0).max(0.01);
                let at = ((ev.x - el.x0) / span).clamp(0.0, 1.0);
                let mut open = el.from + (el.to - el.from) * at;
                if el.grow == '>' {
                    open = 1.0 - open;
                }
                rise = 0.6666 * open;
            }
            need = need.max(ev.reach_down.unwrap_or(4.0) + pad + rise);
        }
    }
    for tp in &line.tuplets {
        if tp.up || tp.x1 < el.x0 || tp.x0 > el.x1 {
            continue;
        }
        need = need.max(tp.ya.max(tp.yb) + 0.9 + 0.6 - el.top);
    }
    // A slur or a tie under the notes: the dynamics go under it, not over
    // it — a hairpin under a run slurred below used to sit on the slur.
    let open = if el.text.is_none() && el.words.is_none() { 0.6666 } else { -el.top };
    for curve in &line.curves {
        if let Some(low) = curve_low(curve, el.x0 - 0.2, el.x1 + 0.2) {
            need = need.max(low + pad + open);
        }
    }
    need
}

/// The lowest point of a curve's outline between `x0` and `x1` (y grows
/// downwards), or none when it does not reach there.
pub fn curve_low(cmds: &[f64], x0: f64, x1: f64) -> Option<f64> {
    let mut low: Option<f64> = None;
    let mut keep = |x: f64, y: f64| {
        if x >= x0 && x <= x1 {
            low = Some(low.map_or(y, |l: f64| l.max(y)));
        }
    };
    let (mut cx, mut cy) = (0.0, 0.0);
    let mut ci = 0;
    while ci < cmds.len() {
        match cmds[ci] as i64 {
            0 | 1 if ci + 2 < cmds.len() => {
                (cx, cy) = (cmds[ci + 1], cmds[ci + 2]);
                keep(cx, cy);
                ci += 3;
            }
            3 if ci + 6 < cmds.len() => {
                let c = &cmds[ci + 1..ci + 7];
                for k in 1..=16 {
                    let t = k as f64 / 16.0;
                    let u = 1.0 - t;
                    let (a, b, d, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    keep(a * cx + b * c[0] + d * c[2] + e * c[4], a * cy + b * c[1] + d * c[3] + e * c[5]);
                }
                (cx, cy) = (c[4], c[5]);
                ci += 7;
            }
            _ => ci += 1,
        }
    }
    low
}

/// The line a dynamic set over the staff stands on: clear of the notes
/// and the articulations over it.
fn dyn_line_above(bars: &[Bar], line: &Line, el: &DynEl, pads: (f64, f64)) -> f64 {
    let (staff_pad, pad) = pads;
    let mut need = -staff_pad - el.bottom;
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            let right = ev.x + item_w(ev);
            if right < el.x0 - 0.2 || ev.x > el.x1 + 0.2 {
                continue;
            }
            need = need.min(ev.reach_up.unwrap_or(0.0) - pad - el.bottom);
        }
    }
    for tp in &line.tuplets {
        if !tp.up || tp.x1 < el.x0 || tp.x0 > el.x1 {
            continue;
        }
        need = need.min(tp.ya.min(tp.yb) - 0.9 - 0.6 - el.bottom);
    }
    need
}

fn texts(bars: &[Bar], line: &mut Line) {
    line.texts = Vec::new();
    let numbers = bar_number_boxes(bars, line);
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            for (index, tx) in ev.texts.iter().enumerate() {
                // A pedal's words are on the pedal's own line ([`pedal_lines`]).
                if tx.pedal > 0 {
                    continue;
                }
                let runs = tx.pieces();
                let look = ev.look.clone();
                if look.omitted("TextScript") {
                    continue;
                }
                let e = em(look.num("TextScript.font-size").unwrap_or(0.0));
                let w = runs_width(&runs, e);
                let bx = runs_box(&runs, e);
                // Over a gathered rest the words are centred on it, over its count;
                // else from the note's head, as `self-alignment-X` and
                // `X-offset` say (LilyPond's default: the words' left at the head's).
                let align = look.num("TextScript.self-alignment-X").unwrap_or(-1.0).clamp(-1.0, 1.0);
                let anchor = match look.num("TextScript.X-offset") {
                    Some(dx) => ev.x + dx,
                    None => ev.x + ev.head_w.unwrap_or(0.0) * (align + 1.0) / 2.0,
                };
                let x0 = if ev.mm.is_some() { (ev.span0 + ev.span1) / 2.0 - w / 2.0 } else { anchor - w * (align + 1.0) / 2.0 };
                let x1 = x0 + w;
                let pad = look.num("TextScript.padding").unwrap_or(0.45);
                let staff_pad = look.num("TextScript.staff-padding").unwrap_or(0.5);
                let between = look.num("TextScript.outside-staff-padding").unwrap_or(0.0);
                let y = if let Some(v) = look.num("TextScript.Y-offset") {
                    2.0 - v
                } else if tx.dir < 0 {
                    let mut edge: f64 = -staff_pad;
                    for ob in line.bars.clone() {
                        for other in &bars[ob].items {
                            if other.x + item_w(other) < x0 - 0.2 || ink_left(other) > x1 + 0.2 {
                                continue;
                            }
                            edge = edge.min(other.reach_up.unwrap_or(0.0));
                        }
                    }
                    for prev in &line.texts {
                        if prev.dir < 0 && prev.x1 > x0 && prev.x0 < x1 {
                            edge = edge.min(prev.top - between);
                        }
                    }
                    for dy in &line.dynamics {
                        if dy.y < 2.0 && dy.x1 > x0 - 0.2 && dy.x0 < x1 + 0.2 {
                            edge = edge.min(dy.y + dy.top);
                        }
                    }
                    for (nx0, nx1, ntop) in &numbers {
                        if *nx0 < x1 + 0.2 && *nx1 > x0 - 0.2 {
                            edge = edge.min(ntop - 0.2);
                        }
                    }
                    // A slur over the notes under the words, as LilyPond's
                    // TextScript goes around it.
                    for c in &line.curves {
                        for (px, py) in points(c) {
                            if px >= x0 - 0.2 && px <= x1 + 0.2 {
                                edge = edge.min(py - 0.2);
                            }
                        }
                    }
                    edge - pad - bx.1
                } else {
                    let mut edge: f64 = 4.0 + staff_pad;
                    for ob in line.bars.clone() {
                        for other in &bars[ob].items {
                            if other.x + item_w(other) < x0 - 0.2 || ink_left(other) > x1 + 0.2 {
                                continue;
                            }
                            edge = edge.max(other.reach_down.unwrap_or(4.0));
                        }
                    }
                    for dy in &line.dynamics {
                        if dy.x1 > x0 - 0.2 && dy.x0 < x1 + 0.2 {
                            edge = edge.max(dy.bottom);
                        }
                    }
                    for prev in &line.texts {
                        if prev.dir > 0 && prev.x1 > x0 && prev.x0 < x1 {
                            edge = edge.max(prev.bottom + between);
                        }
                    }
                    edge + pad - bx.0
                };
                // `extra-offset`: moved once it is placed, as LilyPond moves it —
                // what comes after is placed against where it was.
                let (dx, dy) = look.extra("TextScript");
                line.texts.push(TextPlace { text: tx.text.clone(), runs, x0: x0 + dx, x1: x1 + dx, y: y + dy, top: y + bx.0, bottom: y + bx.1, dir: tx.dir, em: e, item: ev.i, index });
            }
        }
    }
}

/// The pedals' lines under a staff, as LilyPond's SustainPedalLineSpanner,
/// SostenutoPedalLineSpanner and UnaCordaPedalLineSpanner stack them
/// (score #1026): under everything else the line has below the staff, each
/// pedal at one height along the whole line — the sustain pedal's nearest,
/// then the sostenuto's, then the soft pedal's — their words at their notes,
/// and the sostenuto's brackets (`spans`, from [`sostenuto_spans`]) on its
/// words' baseline.
fn pedal_lines(bars: &[Bar], line: &mut Line, spans: &[(f64, f64, bool)]) {
    line.pedal_brackets = Vec::new();
    let e = em(0.0);
    let mut edge: f64 = 4.5;
    for b in line.bars.clone() {
        for ev in &bars[b].items {
            edge = edge.max(ev.reach_down.unwrap_or(4.0));
        }
    }
    for dy in &line.dynamics {
        edge = edge.max(dy.bottom);
    }
    for tx in &line.texts {
        if tx.dir > 0 {
            edge = edge.max(tx.bottom);
        }
    }
    for c in &line.curves {
        for (_, py) in points(c) {
            edge = edge.max(py);
        }
    }
    for cc in [64u8, 66, 67] {
        let mut words: Vec<(&Ev, usize, Vec<Run>)> = Vec::new();
        for b in line.bars.clone() {
            for ev in &bars[b].items {
                for (index, tx) in ev.texts.iter().enumerate() {
                    if tx.pedal == cc && tx.dir > 0 {
                        words.push((ev, index, tx.pieces()));
                    }
                }
            }
        }
        if words.is_empty() && (cc != 66 || spans.is_empty()) {
            continue;
        }
        let mut bx = if cc == 66 { runs_box(&[Run::words("Sost. Ped.", Weight::Italic)], e) } else { (0.0, 0.0) };
        for (_, _, runs) in &words {
            let b = runs_box(runs, e);
            bx = (bx.0.min(b.0), bx.1.max(b.1));
        }
        let y = edge + 0.45 - bx.0;
        for (ev, index, runs) in words {
            let w = runs_width(&runs, e);
            line.texts.push(TextPlace { text: String::new(), runs, x0: ev.x, x1: ev.x + w, y, top: y + bx.0, bottom: y + bx.1, dir: 1, em: e, item: ev.i, index });
        }
        if cc == 66 {
            line.pedal_brackets = spans.iter().map(|&(x0, x1, hook)| PedalBracket { x0, x1, y, hook }).collect();
        }
        edge = y + bx.1.max(0.1) + 0.3;
    }
}

/// Where the sostenuto pedal's brackets go, line by line (score #1026): from
/// after the "Sost. Ped." it is pressed with — or from a line's start, when
/// it is held over the line break — to the note it is let go with, hooked
/// there, or to the line's end. LilyPond's 'mixed style.
fn sostenuto_spans(bars: &[Bar], lines: &[Line]) -> Vec<Vec<(f64, f64, bool)>> {
    let words = runs_width(&[Run::words("Sost. Ped.", Weight::Italic)], em(0.0));
    let mut out = vec![Vec::new(); lines.len()];
    let mut open: Option<f64> = None;
    for (li, line) in lines.iter().enumerate() {
        if open.is_some() {
            open = Some(line.prefix);
        }
        let mut evs: Vec<&Ev> = line.bars.clone().flat_map(|b| bars[b].items.iter()).filter(|ev| ev.pedals.iter().any(|p| p.0 == 66)).collect();
        evs.sort_by(|a, b| a.bar.cmp(&b.bar).then(a.start.cmp(&b.start)));
        for ev in evs {
            for &(_, on) in ev.pedals.iter().filter(|p| p.0 == 66) {
                if on {
                    open = Some(ev.x + words + 0.3);
                } else if let Some(x0) = open.take() {
                    out[li].push((x0, ev.x.max(x0), true));
                }
            }
        }
        if let Some(x0) = open {
            out[li].push((x0, line.right.max(x0), false));
        }
    }
    out
}

/// The line the syllables stand on: 0.6 under everything the staff has
/// below it — notes, stems, slurs, dynamics, words — and never nearer the
/// staff than 1.2 spaces from its bottom line to the top of the letters.
fn lyric_line(bars: &[Bar], line: &mut Line) {
    let items = || line.bars.clone().flat_map(|b| bars[b].items.iter());
    let has = items().any(|ev| ev.lyric.is_some() || ev.verses.iter().any(Option::is_some));
    if !has {
        return;
    }
    let e = em(0.0);
    line.lyric_y = Some((reach(bars, line).1 + 0.6).max(5.2) + 0.72 * e);
    line.lyric_lines = 1 + items().map(|ev| ev.verses.len()).max().unwrap_or(0);
}

/// From one verse's baseline to the next's, as LilyPond stacks a staff's
/// Lyrics contexts.
pub(crate) fn verse_gap() -> f64 {
    1.35 * em(0.0)
}

fn voltas(bars: &[Bar], line: &Line) -> Vec<Volta> {
    let mut out: Vec<Volta> = Vec::new();
    let mut run: Option<Volta> = None;
    let last_b = line.bars.end - 1;
    for b in line.bars.clone() {
        let bar = &bars[b];
        let label = bar.volta_in.clone();
        if let (Some(l), None) = (&label, &run) {
            run = Some(Volta { label: l.clone(), first: bar.volta_first, x0: bar.x0, ..Volta::default() });
        }
        if let Some(r) = &mut run {
            r.bars.push(b);
            r.x1 = bar.x1 + barline_width(bar.drawn());
            r.closed = bar.volta_last && bar.volta_closed;
            if bar.volta_last || b == last_b || bars[b + 1].volta_in != label {
                out.extend(run.take());
            }
        }
    }
    for vb in &mut out {
        // Its label, "1.", where draw_volta writes it: what stands under the
        // label is cleared by the label's height, not by the line's alone.
        let lx0 = vb.x0 + 0.5;
        let lx1 = if vb.first { lx0 + 0.3 + text_width(&vb.label, Weight::Bold, em(-2.0)) + 0.3 } else { lx0 };
        let mut low: f64 = -2.0;
        let mut label_low: f64 = 0.0;
        for &b in &vb.bars {
            for ev in &bars[b].items {
                if ev.k != Kind::Note {
                    continue;
                }
                let top = (y_of(ev.heads.last().map_or(0, |h| h.pos)) - 0.7)
                    .min(ev.stem_end - 0.3)
                    // An articulation above the note, a fermata: as its stem.
                    .min(ev.reach_up.unwrap_or(0.0) - 0.3);
                low = low.min(top);
                if ev.x <= lx1 && ev.x + ev.head_w.unwrap_or(1.2) >= lx0 {
                    label_low = label_low.min(top);
                }
            }
        }
        // What else stands above the staff under the bracket: its label,
        // "1.", hangs 1.8 spaces below the line, so the line clears it all
        // by that and a little more.
        let (x0, x1) = (vb.x0, vb.x1);
        let under = |x: f64| x >= x0 - 0.5 && x <= x1 + 0.5;
        let mut above: f64 = 0.0;
        for tx in &line.texts {
            if tx.dir < 0 && tx.x1 >= x0 && tx.x0 <= x1 {
                above = above.min(tx.top - 0.3);
            }
        }
        for tp in &line.tuplets {
            if tp.up && tp.x1 >= x0 && tp.x0 <= x1 {
                above = above.min(tp.ya.min(tp.yb) - 1.0);
            }
        }
        for c in &line.curves {
            for (px, py) in points(c) {
                if under(px) {
                    above = above.min(py - 0.2);
                }
            }
        }
        vb.y = (low - 1.0).min(label_low - 1.7).min(above - 2.0);
    }
    out
}

pub(crate) fn mark_width(mark: &Tempo) -> f64 {
    let e = em(0.0);
    let mut w = if mark.text.is_empty() { 0.0 } else { text_width(&mark.text, Weight::Bold, e) };
    if mark.bpm > 0 {
        if w > 0.0 {
            w += 0.6;
        }
        w += text_width(&format!("( = {})", mark.bpm), Weight::Regular, e) + 1.6;
    }
    w
}

/// How far above the top line (negative) and below it a staff's line's ink
/// goes.
pub(crate) fn reach(bars: &[Bar], line: &Line) -> (f64, f64) {
    match line.kind {
        StaffKind::ChordNames | StaffKind::FretBoards => {
            let base = crate::engrave::CHORD_BASE;
            let (mut hi, mut lo) = (base - 1.0, base);
            for b in line.bars.clone() {
                for ev in &bars[b].items {
                    let (t, bo) = runs_box(&ev.chord_name, crate::engrave::chord_em());
                    hi = hi.min(base + t);
                    lo = lo.max(base + bo);
                }
            }
            return (hi - 0.2, lo + 0.2);
        }
        StaffKind::Figures => {
            let lines = line.bars.clone().flat_map(|b| bars[b].items.iter()).map(|ev| ev.figures.len()).max().unwrap_or(0);
            if lines == 0 {
                return (0.0, 0.5);
            }
            let (_, bottom) = runs_box(&[Run::words("0", Weight::Regular)], crate::engrave::figure_em());
            return (0.0, crate::engrave::figure_line(lines - 1) + bottom + 0.3);
        }
        StaffKind::Tab | StaffKind::TabStrings(_) => return (-1.0, crate::engrave::staff_height(line.kind) + 1.0),
        StaffKind::Ossia if line.bars.clone().all(|b| bars[b].items.iter().all(|ev| ev.spacer)) => return (0.0, 0.0),
        _ => {}
    }
    let mut hi: f64 = 0.0;
    let mut lo: f64 = 4.0;
    let range = line.bars.clone();
    for b in range.clone() {
        for ev in &bars[b].items {
            if ev.k != Kind::Note {
                continue;
            }
            for hd in &ev.heads {
                let y = y_of(hd.pos);
                let pad = if hd.acc.is_none() { 0.6 } else { 1.8 };
                hi = hi.min(y - pad);
                lo = lo.max(y + pad);
            }
            // A stem to a beam between the staves is not the staff's to make room for.
            if !ev.xbeam {
                hi = hi.min(ev.stem_end - 0.6);
                lo = lo.max(ev.stem_end + 0.6);
            }
        }
    }
    for b in range.clone() {
        for ev in &bars[b].items {
            for g in ev.graces.iter().chain(&ev.after_graces) {
                for hd in &g.heads {
                    hi = hi.min(y_of(hd.pos) - 0.6);
                    lo = lo.max(y_of(hd.pos) + 0.6);
                }
                hi = hi.min(g.stem_end - 0.4);
                lo = lo.max(g.stem_end + 0.4);
            }
        }
    }
    for mk in &line.marks {
        hi = hi.min(mk.y - 1.8);
    }
    for sp in &line.spans {
        let (t, _) = if sp.runs.is_empty() { (-0.4, 0.0) } else { runs_box(&sp.runs, em(0.0)) };
        hi = hi.min(sp.y + t - 0.2);
    }
    if let Some(y) = line.title_y {
        hi = hi.min(y - 0.75 * em(0.0));
    }
    // Room for the bar numbers printed: at the line's start, and in it.
    let numbered = range.clone().enumerate().any(|(bi, b)| {
        let look = bars[b].items.first().map(|ev| ev.look.clone()).unwrap_or_default();
        if look.is_empty() {
            return bi == 0 && bars[b].number > 1;
        }
        line.staff_index.unwrap_or(0) == 0 && bars[b].number >= 1 && !(bi == 0 && bars[b].first) && crate::look::bar_number_shown(&look, bars[b].number, bi == 0, bars[b].first || bars[b].number <= 1)
    });
    if numbered {
        hi = hi.min(-3.0);
    }
    for vb in &line.voltas {
        hi = hi.min(vb.y - 0.4);
    }
    for tp in &line.tuplets {
        if tp.up {
            hi = hi.min(tp.ya.min(tp.yb) - 0.9);
        } else {
            lo = lo.max(tp.ya.max(tp.yb) + 0.9);
        }
    }
    for b in range.clone() {
        for ev in &bars[b].items {
            hi = hi.min(ev.reach_up.unwrap_or(0.0));
            lo = lo.max(ev.reach_down.unwrap_or(4.0));
        }
    }
    for dy in &line.dynamics {
        lo = lo.max(dy.bottom);
        hi = hi.min(dy.y + dy.top);
    }
    for tx in &line.texts {
        if tx.dir < 0 {
            hi = hi.min(tx.top);
        }
        if tx.dir > 0 {
            lo = lo.max(tx.bottom);
        }
    }
    for br in &line.pedal_brackets {
        lo = lo.max(br.y + 0.2);
    }
    for b in range {
        if bars[b].items.first().is_some_and(|ev| ev.mm.is_some()) {
            hi = hi.min(-2.6);
        }
    }
    if let Some(ly) = line.lyric_y {
        lo = lo.max(ly + verse_gap() * line.lyric_lines.saturating_sub(1) as f64 + 0.25 * em(0.0) + 0.6);
    }
    for ot in &line.ottavas {
        let (t, b) = text_box(&ot.text, Weight::Bold, ottava_em());
        hi = hi.min(ot.y + t - 0.2);
        lo = lo.max(ot.y + b + 0.2);
    }
    for mk in &line.rehearsal {
        let (t, _) = runs_box(&mk.runs, mark_em());
        let framed = if mk.frame.is_empty() { 0.0 } else { FRAME_PAD + 0.1 };
        hi = hi.min(mk.y + t - 0.3 - framed);
    }
    for g in &line.glissandi {
        hi = hi.min(g[1].min(g[3]));
        lo = lo.max(g[1].max(g[3]));
    }
    for c in &line.curves {
        for (_, y) in points(c) {
            hi = hi.min(y);
            lo = lo.max(y);
        }
    }
    (hi, lo)
}

/// A fingering's ink above and below its origin, `(top, bottom)`.
pub(crate) fn figure_box(figure: &str) -> (f64, f64) {
    let mut top: f64 = 0.0;
    let mut bottom: f64 = 0.0;
    for ch in figure.chars() {
        let g = glyph(&format!("timeSig{ch}"));
        top = top.min(g.bbox[1] * FINGER_K);
        bottom = bottom.max(g.bbox[3] * FINGER_K);
    }
    (top, bottom)
}

/// A fingering's width.
pub(crate) fn figure_width(figure: &str) -> f64 {
    figure.chars().map(|ch| glyph(&format!("timeSig{ch}")).advance * FINGER_K).sum()
}

/// A superscript's size against its words', and how far it is raised, in
/// their ems.
pub(crate) const SUP: f64 = 0.7;
/// …
pub(crate) const RAISE: f64 = 0.45;

/// A piece's size and how far above the baseline it stands.
pub(crate) fn run_em(r: &Run, e: f64) -> (f64, f64) {
    if r.sup {
        (e * SUP, e * RAISE)
    } else {
        (e, 0.0)
    }
}

/// How wide pieces of markup are, a word space before each that asks.
pub(crate) fn runs_width(runs: &[Run], e: f64) -> f64 {
    let space = text_width(" ", Weight::Regular, e);
    let mut w = 0.0;
    for (k, r) in runs.iter().enumerate() {
        let (re, _) = run_em(r, e);
        if k > 0 && r.gap {
            w += space * re / e;
        }
        w += if let Some(d) = crate::frets::from_spec(&r.text) {
            crate::frets::size(&d, re).0
        } else if r.glyph {
            glyph_run_w(r)
        } else if r.text == TRIANGLE {
            0.62 * re
        } else {
            text_width(&r.text, r.weight, re)
        };
    }
    w
}

/// The major seventh's triangle, which the text face has not got: drawn as
/// LilyPond's markup draws it, an outline.
pub(crate) const TRIANGLE: &str = "△";

/// A glyph of markup's width.
pub(crate) fn glyph_run_w(r: &Run) -> f64 {
    let g = glyph(&r.text);
    (g.bbox[2] - g.bbox[0]) * r.size
}

/// Pieces of markup's ink above and below their baseline, `(top, bottom)`;
/// a glyph stands on the baseline.
pub(crate) fn runs_box(runs: &[Run], e: f64) -> (f64, f64) {
    let mut top: f64 = 0.0;
    let mut bottom: f64 = 0.0;
    for r in runs {
        let (re, rise) = run_em(r, e);
        let (t, b) = if let Some(d) = crate::frets::from_spec(&r.text) {
            (-crate::frets::size(&d, re).1, 0.0)
        } else if r.glyph {
            let g = glyph(&r.text);
            (-(g.bbox[3] - g.bbox[1]) * r.size, 0.0)
        } else if r.text == TRIANGLE {
            (-0.55 * re, 0.0)
        } else {
            text_box(&r.text, r.weight, re)
        };
        top = top.min(t - rise);
        bottom = bottom.max(b - rise);
    }
    (top, bottom)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;
    use crate::engrave::acc_name;

    /// The ink of each accidental drawn on a bar's first note, in staff
    /// spaces from the staff's top line: left, top, right, bottom.
    fn first_accidentals(eng: &crate::Engraved, bar: usize) -> Vec<[f64; 4]> {
        let ev = eng.staves[0][bar].items.iter().find(|e| e.k == Kind::Note).unwrap();
        ev.heads
            .iter()
            .filter_map(|hd| {
                let acc = hd.acc?;
                let g = glyph(acc_name(acc));
                let (ax, ay) = (ev.x + hd.acc_x.unwrap_or(0.0), y_of(hd.pos));
                Some([ax + g.bbox[0], ay + g.bbox[1], ax + g.bbox[2], ay + g.bbox[3]])
            })
            .collect()
    }

    /// Two boxes `[left, top, right, bottom]` that share ink.
    fn touch(a: [f64; 4], b: [f64; 4]) -> bool {
        a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
    }

    /// A mark over the bar line after `||` — "Al Coda", in roman or italics,
    /// a segno, a coda, a letter — stands over the natural or the flat of the
    /// bar's first note, a high one starting a run of sixteenths down under
    /// a slur and over an ff: it was drawn through the sign (score #668).
    #[test]
    fn a_mark_at_a_bars_start_clears_its_first_notes_accidental() {
        let run = |first: &str| format!(r"{first}16\ff( a'' g'' f'' e'' d'' c'' b' a' g' f' e' f'4)");
        let mut seen = 0;
        for (key, first) in [("f", "b''"), ("c", "bes''")] {
            for mark in [r#"\mark "Al Coda""#, r#"\mark \markup \italic "Al Coda""#, r"\segnoMark", r"\codaMark", r"\mark \default"] {
                let src = format!(r#"{{ \key {key} \major c'4 d' e' f' \bar "||" {mark} {} }}"#, run(first));
                let eng = crate::engrave(&src, 120.0, &crate::Layout::default()).unwrap();
                let line = &eng.systems[0].lines[0];
                assert_eq!(line.rehearsal.len(), 1, "{src}");
                let mk = &line.rehearsal[0];
                let e = mark_em();
                let (top, bottom) = runs_box(&mk.runs, e);
                let words = [mk.x, mk.y + top, mk.x + runs_width(&mk.runs, e), mk.y + bottom];
                for acc in first_accidentals(&eng, 1) {
                    if words[0] < acc[2] && acc[0] < words[2] {
                        assert!(!touch(words, acc), "{src}: the mark {words:?} runs into the accidental {acc:?}");
                        seen += 1;
                    }
                }
            }
        }
        assert!(seen >= 4, "\"Al Coda\" stands over the signs: {seen}");
    }

    /// Words over a note — "To Coda" — run on over the next note's flat:
    /// they clear it as they clear its head.
    #[test]
    fn words_over_a_note_clear_the_next_notes_accidental() {
        let eng = crate::engrave(r#"{ c''8^"To Coda" bes''8 c''4 c''2 }"#, 120.0, &crate::Layout::default()).unwrap();
        let line = &eng.systems[0].lines[0];
        let tx = &line.texts[0];
        let words = [tx.x0, tx.top, tx.x1, tx.bottom];
        let ev = &eng.staves[0][0].items[1];
        let hd = &ev.heads[0];
        let g = glyph(acc_name(hd.acc.unwrap()));
        let ax = ev.x + hd.acc_x.unwrap_or(0.0);
        let flat = [ax + g.bbox[0], y_of(hd.pos) + g.bbox[1], ax + g.bbox[2], y_of(hd.pos) + g.bbox[3]];
        assert!(words[0] < flat[2] && flat[0] < words[2], "the words reach over the flat");
        assert!(!touch(words, flat), "the words {words:?} run into the flat {flat:?}");
    }

    /// The bar the user saw: "Al Coda" over the rest ending a bar runs on
    /// past the `||` over the next bar's first note, a natural starting a
    /// slurred run of sixteenths down over an ff (score #668).
    #[test]
    fn words_before_a_double_bar_clear_the_next_bars_natural() {
        let src = r#"{ \key g \major \time 2/4 b'4 r8 r8^"Al Coda" \bar "||" f''16\ff( e'' d'' c'' b'8) r8 }"#;
        let eng = crate::engrave(src, 120.0, &crate::Layout::default()).unwrap();
        let tx = &eng.systems[0].lines[0].texts[0];
        let words = [tx.x0, tx.top, tx.x1, tx.bottom];
        let natural = first_accidentals(&eng, 1)[0];
        assert!(words[0] < natural[2] && natural[0] < words[2], "the words reach over the natural");
        assert!(!touch(words, natural), "the words {words:?} run into the natural {natural:?}");
    }

    /// The same bar with no sign on its first note: the words clear the
    /// slur starting under them.
    #[test]
    fn words_before_a_double_bar_clear_the_next_bars_slur() {
        let src = r#"{ \time 2/4 b'4 r8 r8^"Al Coda" \bar "||" f''16\ff( e'' d'' c'' b'8) r8 }"#;
        let eng = crate::engrave(src, 120.0, &crate::Layout::default()).unwrap();
        let line = &eng.systems[0].lines[0];
        let tx = &line.texts[0];
        let under: Vec<(f64, f64)> = line.curves.iter().flat_map(|c| points(c)).filter(|p| p.0 >= tx.x0 && p.0 <= tx.x1).collect();
        assert!(!under.is_empty(), "the slur starts under the words");
        for (px, py) in under {
            assert!(tx.bottom + 0.4 <= py, "the words reach {} by the slur at ({px}, {py})", tx.bottom);
        }
    }
}
