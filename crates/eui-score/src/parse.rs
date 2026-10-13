//! Reading LilyPond: the subset `score()` draws, every construct outside it
//! refused with its position (`eui_builders_score.sl`, "Reading LilyPond").

use std::collections::HashMap;

use crate::{ScoreError, WHOLE};

/// The dynamics a score prints.
pub const DYNAMICS: [&str; 21] = ["pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff", "fp", "sf", "sff", "sp", "spp", "sfz", "sfp", "rfz", "rf", "fz", "n"];

/// The articulations a score prints, by their LilyPond names.
pub const SCRIPTS: [&str; 7] = ["staccato", "tenuto", "accent", "marcato", "staccatissimo", "portato", "fermata"];

pub(crate) fn refuse<T>(what: impl Into<String>, at: usize) -> Result<T, ScoreError> {
    Err(ScoreError { message: format!("score: {} (at character {at})", what.into()) })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum T {
    Cmd,
    Str,
    Num,
    Word,
    Sym(char),
    /// The `{ … }` block after `\addlyrics`, read whole.
    Lyrics,
}

#[derive(Debug, Clone)]
pub(crate) struct Tok {
    pub t: T,
    pub v: String,
    pub marks: i64,
    pub at: usize,
    /// A lyrics token's words.
    pub lyrics: Vec<LyricItem>,
}

/// One item of a lyric block: a word, `--`, `__`, `_`, or a quoted string.
#[derive(Debug, Clone)]
pub(crate) struct LyricItem {
    pub s: String,
    pub quoted: bool,
    pub at: usize,
}

fn letter(ch: char) -> bool {
    ch.is_ascii_alphabetic()
}

fn digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

pub(crate) fn tokens(src: &str) -> Result<Vec<Tok>, ScoreError> {
    let cs: Vec<char> = src.chars().collect();
    let n = cs.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let ch = cs[i];
        if ch == ' ' || ch == '\n' || ch == '\t' || ch == '\r' {
            i += 1;
        } else if ch == '%' {
            while i < n && cs[i] != '\n' {
                i += 1;
            }
        } else if ch == '\\' && i + 1 < n && cs[i + 1] == '\\' {
            // `\\` between the voices of `<< { … } \\ { … } >>`.
            out.push(Tok { t: T::Sym('\\'), v: "\\\\".into(), marks: 0, at: i, lyrics: Vec::new() });
            i += 2;
        } else if ch == '\\' && i + 1 < n && "<>!".contains(cs[i + 1]) {
            out.push(Tok { t: T::Cmd, v: cs[i + 1].to_string(), marks: 0, at: i, lyrics: Vec::new() });
            i += 2;
        } else if ch == '\\' {
            let mut jj = i + 1;
            let mut word = String::new();
            while jj < n && letter(cs[jj]) {
                word.push(cs[jj]);
                jj += 1;
            }
            if word.is_empty() {
                return refuse("a backslash with no command after it", i);
            }
            let addlyrics = word == "addlyrics";
            out.push(Tok { t: T::Cmd, v: word, marks: 0, at: i, lyrics: Vec::new() });
            let at = i;
            i = jj;
            if addlyrics {
                let (items, next) = lyric_tokens(&cs, i, at)?;
                out.push(Tok { t: T::Lyrics, v: String::new(), marks: 0, at, lyrics: items });
                i = next;
            }
        } else if ch == '"' {
            let mut jj = i + 1;
            let mut word = String::new();
            while jj < n && cs[jj] != '"' {
                word.push(cs[jj]);
                jj += 1;
            }
            if jj >= n {
                return refuse("a string that is never closed", i);
            }
            out.push(Tok { t: T::Str, v: word, marks: 0, at: i, lyrics: Vec::new() });
            i = jj + 1;
        } else if "{}<>[]()~|.=*-^_!#".contains(ch) {
            out.push(Tok { t: T::Sym(ch), v: ch.to_string(), marks: 0, at: i, lyrics: Vec::new() });
            i += 1;
        } else if digit(ch) {
            let mut jj = i;
            let mut word = String::new();
            while jj < n && (digit(cs[jj]) || cs[jj] == '/') {
                word.push(cs[jj]);
                jj += 1;
            }
            out.push(Tok { t: T::Num, v: word, marks: 0, at: i, lyrics: Vec::new() });
            i = jj;
        } else if letter(ch) {
            let mut jj = i;
            let mut word = String::new();
            while jj < n && letter(cs[jj]) {
                word.push(cs[jj]);
                jj += 1;
            }
            let mut marks = 0;
            while jj < n && (cs[jj] == '\'' || cs[jj] == ',') {
                marks += if cs[jj] == '\'' { 1 } else { -1 };
                jj += 1;
            }
            out.push(Tok { t: T::Word, v: word, marks, at: i, lyrics: Vec::new() });
            i = jj;
        } else {
            return refuse(format!("'{ch}' is not part of the notation this reads"), i);
        }
    }
    Ok(out)
}

fn blank(ch: char) -> bool {
    ch == ' ' || ch == '\n' || ch == '\t' || ch == '\r'
}

/// The block after `\addlyrics`, read as LilyPond reads lyrics: words split
/// on spaces, any letter in them — "Frè", "l'eau" — a quoted string as one,
/// `--` joining a word's syllables, `__` holding one over the notes after it,
/// `_` a note that takes none. Answers the items and where the block ends.
fn lyric_tokens(cs: &[char], mut i: usize, at: usize) -> Result<(Vec<LyricItem>, usize), ScoreError> {
    let n = cs.len();
    while i < n && blank(cs[i]) {
        i += 1;
    }
    if !(i < n && cs[i] == '{') {
        return refuse("\\addlyrics wants a { block } of words", at);
    }
    i += 1;
    let mut items = Vec::new();
    loop {
        while i < n && blank(cs[i]) {
            i += 1;
        }
        if i >= n {
            return refuse("a block of lyrics that is never closed", at);
        }
        if cs[i] == '}' {
            break;
        }
        if cs[i] == '%' {
            while i < n && cs[i] != '\n' {
                i += 1;
            }
        } else if cs[i] == '"' {
            let mut jj = i + 1;
            let mut word = String::new();
            while jj < n && cs[jj] != '"' {
                word.push(cs[jj]);
                jj += 1;
            }
            if jj >= n {
                return refuse("a string that is never closed", i);
            }
            items.push(LyricItem { s: word, quoted: true, at: i });
            i = jj + 1;
        } else {
            if cs[i] == '{' {
                return refuse("a { inside lyrics is not read: they are words, -- and __", i);
            }
            if cs[i] == '\\' {
                return refuse("'\\' inside lyrics is not read: they are words, -- and __", i);
            }
            let mut jj = i;
            let mut word = String::new();
            while jj < n && !" \n\t\r{}\"".contains(cs[jj]) {
                word.push(cs[jj]);
                jj += 1;
            }
            items.push(LyricItem { s: word, quoted: false, at: i });
            i = jj;
        }
    }
    Ok((items, i + 1))
}

/// The syllables of a lyric block: each word one, `--` and `__` marking the
/// one before them, `_` a syllable with no words.
fn syllables(items: &[LyricItem], at: usize) -> Result<Vec<Syllable>, ScoreError> {
    let mut out: Vec<Syllable> = Vec::new();
    for it in items {
        if !it.quoted && (it.s == "--" || it.s == "__") {
            let Some(last) = out.last_mut() else {
                return refuse(format!("{} wants a syllable before it", it.s), it.at);
            };
            if it.s == "--" {
                last.hyphen = true;
            } else {
                last.extend = true;
            }
            continue;
        }
        let skip = !it.quoted && it.s == "_";
        out.push(Syllable { text: if skip { String::new() } else { it.s.clone() }, hyphen: false, extend: false });
    }
    if out.is_empty() {
        return refuse("\\addlyrics with no words in it", at);
    }
    Ok(out)
}

/// `\addlyrics { … }` after a staff's music: its words, one syllable a
/// note. One verse a staff is what this sets.
fn attach_lyrics(toks: &[Tok], mut i: usize, staves: &mut [Staff]) -> Result<usize, ScoreError> {
    while is_cmd(toks, i, "addlyrics") {
        let at = toks[i].at;
        let Some(last) = staves.last_mut() else {
            return refuse("\\addlyrics follows a staff's music", at);
        };
        if last.lyrics.is_some() {
            return refuse("one verse of lyrics a staff is what this sets", at);
        }
        last.lyrics = Some(syllables(&toks[i + 1].lyrics, at)?);
        i += 2;
    }
    Ok(i)
}

/// A note name, Dutch as LilyPond's default: `(step, alter)`.
pub(crate) fn pitch_name(word: &str) -> Option<(i64, i64)> {
    let mut chars = word.chars();
    let first = chars.next()?;
    let step = "cdefgab".find(first)? as i64;
    let rest: String = chars.collect();
    let alter = match rest.as_str() {
        "" => 0,
        "is" => 1,
        "isis" => 2,
        "es" => -1,
        "eses" => -2,
        "s" if step == 2 || step == 5 => -1,
        "ses" if step == 2 || step == 5 => -2,
        _ => return None,
    };
    Some((step, alter))
}

/// A modulo that is never negative.
pub(crate) fn modulo(u: i64, m: i64) -> i64 {
    ((u % m) + m) % m
}

/// The diatonic number of a pitch, `\relative` or absolute.
pub(crate) fn resolve(step: i64, marks: i64, reference: Option<i64>) -> i64 {
    let Some(r) = reference else { return step + 7 * marks };
    let ref_step = modulo(r, 7);
    let mut d = r - ref_step + step;
    if d - r > 3 {
        d -= 7;
    }
    if r - d > 3 {
        d += 7;
    }
    d + 7 * marks
}

/// A clef the engraver draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clef {
    /// The G clef on the second line.
    #[default]
    Treble,
    /// The F clef on the fourth line.
    Bass,
    /// The C clef on the third line.
    Alto,
}

/// What an event is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Kind {
    /// A note or a chord.
    #[default]
    Note,
    /// A rest, `r` or `R`.
    Rest,
    /// `\clef`.
    Clef,
    /// `\key`.
    Key,
    /// `\time`.
    Time,
    /// `\bar`.
    Bar,
    /// `\tempo`.
    Tempo,
    /// `\break` or `\pageBreak`.
    Break,
    /// The start of a `\repeat volta`.
    RepeatStart,
    /// Its end.
    RepeatEnd,
    /// An `\alternative` ending begins.
    VoltaStart,
    /// It ends.
    VoltaEnd,
    /// `\compressEmptyMeasures`.
    Compress,
    /// `<<` opening two voices.
    Voices,
    /// `\\` between them.
    VoiceNext,
    /// `>>` closing them.
    VoicesEnd,
    /// A column of a shared bar: every staff's note or rest at one moment.
    Col,
}

/// A pitch of a note: its diatonic number (`c'` is 7) and its alteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pitch {
    /// Diatonic steps from c,,, — `c'` is 7.
    pub d: i64,
    /// -2 … 2.
    pub alter: i64,
}

/// `\tempo`'s words, beat and speed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tempo {
    /// The words, or "".
    pub text: String,
    /// The beat's note value, or 0.
    pub base: i64,
    /// Beats a minute, or 0.
    pub bpm: i64,
}

/// An articulation on a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// Its LilyPond name.
    pub name: String,
    /// -1 above, 1 below, 0 the engraver's choice.
    pub dir: i64,
}

/// Words over or under a note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    /// The words.
    pub text: String,
    /// -1 above, 1 below.
    pub dir: i64,
}

/// The tuplet a note is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TupletRef {
    /// Its group.
    pub id: i64,
    /// `n` in the time of …
    pub n: i64,
    /// … `d`.
    pub d: i64,
}

/// A head of a placed note.
#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    /// Staff position: half spaces up from the middle line.
    pub pos: i64,
    /// The pitch's diatonic number.
    pub d: i64,
    /// Its alteration.
    pub alter: i64,
    /// The accidental printed, if one is.
    pub acc: Option<i64>,
    /// 0, or 1 right of an up stem, or -1 left of a down one.
    pub side: i64,
    /// Where its accidental starts, from the note's x.
    pub acc_x: Option<f64>,
}

/// One event of a staff, and once it is in a bar, everything placing it
/// works out — the Soli engraver's hash for a note, field for field.
#[derive(Debug, Clone, Default)]
pub struct Ev {
    /// What it is.
    pub k: Kind,
    /// Its character in the source.
    pub at: usize,
    // -- settings
    /// `\clef`'s.
    pub clef: Clef,
    /// `\key`'s fifths.
    pub fifths: i64,
    /// `\time`'s numerator and denominator.
    pub time: (i64, i64),
    /// `\bar`'s kind, an ending's label.
    pub v: String,
    /// `\pageBreak`.
    pub page: bool,
    /// An ending that goes back.
    pub closed: bool,
    /// `\tempo`'s mark, and on a note the mark written before it.
    pub tempo: Option<Tempo>,
    // -- notes and rests
    /// Its pitches, none for a rest.
    pub pitches: Vec<Pitch>,
    /// Its written value: 1, 2, 4 … 32.
    pub base: i64,
    /// Its dots.
    pub dots: i64,
    /// How long it lasts, in ticks.
    pub ticks: i64,
    /// Its number among every note and rest of the piece.
    pub i: usize,
    /// Its number among the notes, for a note.
    pub n: Option<usize>,
    /// `R`, a whole bar's rest.
    pub full: bool,
    /// `R1*4`'s count.
    pub count: i64,
    /// `~`.
    pub tie: bool,
    /// `(`.
    pub slur_on: bool,
    /// `)`.
    pub slur_off: bool,
    /// `[`.
    pub beam_on: bool,
    /// `]`.
    pub beam_off: bool,
    /// Articulations.
    pub scripts: Vec<Script>,
    /// Words.
    pub texts: Vec<Words>,
    /// A dynamic.
    pub dynamic: Option<String>,
    /// `\<` or `\>`.
    pub hairpin: Option<char>,
    /// `\!`.
    pub hairpin_end: bool,
    /// The tuplet it is in.
    pub tuplet: Option<TupletRef>,
    // -- placed
    /// Ticks from its bar's start.
    pub start: i64,
    /// Its bar's index.
    pub bar: usize,
    /// Bars a gathered rest stands for.
    pub mm: Option<i64>,
    /// Its heads, low to high.
    pub heads: Vec<Head>,
    /// Stem up.
    pub up: bool,
    /// The beam it is under, in its bar.
    pub beam: Option<usize>,
    /// The space it wants after it.
    pub ideal: f64,
    /// Ink left of its x.
    pub left: f64,
    /// Ink right of its x.
    pub right: f64,
    /// A note's head width.
    pub head_w: Option<f64>,
    /// Where it is.
    pub x: f64,
    /// A rest drawn in the middle of its bar.
    pub whole_bar: bool,
    /// Its bar's room, for a whole-bar rest.
    pub span0: f64,
    /// …
    pub span1: f64,
    /// Where its stem ends.
    pub stem_end: f64,
    /// How far its ink and its articulations reach up.
    pub reach_up: Option<f64>,
    /// … and down.
    pub reach_down: Option<f64>,
    /// The line it is on.
    pub sys: usize,
    /// A column's notes and rests: (staff, index in that staff's bar).
    pub members: Vec<(usize, usize)>,
    /// Of `<< { … } \\ { … } >>`, the voice: 1 stems up, 2 stems down.
    pub voice: Option<u8>,
    /// `s`: a rest that takes time and is not drawn.
    pub spacer: bool,
    /// How far a first voice's note stands right of its column, clear of
    /// the second's heads a step away.
    pub shift: Option<f64>,
    /// Its syllable, centred under its head.
    pub lyric: Option<Syllable>,
    /// It holds the syllable of a note before it: a melisma.
    pub lyric_held: bool,
}

/// A syllable of `\addlyrics`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Syllable {
    /// Its words; none for `_`, a note that takes no syllable.
    pub text: String,
    /// `--`: the word goes on in the next syllable.
    pub hyphen: bool,
    /// `__`: held over the notes after it.
    pub extend: bool,
}

/// A staff's settings, from `\with` or `\set Staff.…`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Props {
    /// `instrumentName`.
    pub instrument_name: Option<String>,
    /// `shortInstrumentName`.
    pub short_instrument_name: Option<String>,
    /// `midiInstrument`.
    pub midi_instrument: Option<String>,
    /// `midiMaximumVolume`, 0–1.
    pub midi_maximum_volume: Option<f64>,
    /// `midiPanPosition`, -1–1.
    pub midi_pan_position: Option<f64>,
}

/// One staff of the source.
#[derive(Debug, Clone, Default)]
pub struct Staff {
    /// Its events.
    pub events: Vec<Ev>,
    /// Its settings.
    pub props: Props,
    /// Its `\addlyrics`, a syllable a note.
    pub lyrics: Option<Vec<Syllable>>,
}

/// How a group of staves is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    /// `<< >>`: nothing.
    None,
    /// A piano's brace.
    Brace,
    /// An orchestra's bracket.
    Bracket,
    /// A choir's bracket, bar lines stopping at each staff.
    Choir,
}

/// A group of staves.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// How it is marked.
    pub kind: GroupKind,
    /// Its first staff.
    pub first: usize,
    /// Its last staff.
    pub last: usize,
    /// How deep it is in other groups.
    pub depth: usize,
    /// Its name.
    pub name: String,
    /// Its short name.
    pub short: String,
}

/// A source read.
#[derive(Debug, Clone, Default)]
pub struct Parsed {
    /// `\header`'s fields.
    pub header: HashMap<String, String>,
    /// The staves.
    pub staves: Vec<Staff>,
    /// Their groups, innermost first.
    pub groups: Vec<Group>,
}

struct Base {
    header: HashMap<String, String>,
    counter: usize,
    notes: usize,
}

struct St {
    reference: Option<i64>,
    base: i64,
    dots: i64,
    events: Vec<Ev>,
    notes: usize,
    items: usize,
    props: Props,
    alts: i64,
    tuplets: i64,
    tuplet: Option<TupletState>,
    voice: Option<u8>,
}

#[derive(Clone, Copy)]
struct TupletState {
    id: i64,
    n: i64,
    d: i64,
    span: i64,
    used: i64,
}

fn is(toks: &[Tok], i: usize, t: T) -> bool {
    toks.get(i).is_some_and(|k| k.t == t)
}

fn is_cmd(toks: &[Tok], i: usize, v: &str) -> bool {
    toks.get(i).is_some_and(|k| k.t == T::Cmd && k.v == v)
}

fn at_of(toks: &[Tok], i: usize) -> usize {
    toks.get(i).map_or(0, |k| k.at)
}

/// Read a source.
pub fn parse(src: &str) -> Result<Parsed, ScoreError> {
    let toks = tokens(src)?;
    let mut st = Base { header: HashMap::new(), counter: 0, notes: 0 };
    let mut i = preamble(&toks, 0, &mut st)?;
    let in_score = is_cmd(&toks, i, "score");
    if in_score {
        if !is(&toks, i + 1, T::Sym('{')) {
            return refuse("\\score wants a { block }", toks[i].at);
        }
        i = preamble(&toks, i + 2, &mut st)?;
    }
    let mut staves = Vec::new();
    let mut groups = Vec::new();
    i = part(&toks, i, &mut st, &mut staves, &mut groups, 0)?;
    i = attach_lyrics(&toks, i, &mut staves)?;
    if in_score {
        while i < toks.len() && toks[i].t == T::Cmd && ["layout", "midi", "header"].contains(&toks[i].v.as_str()) {
            if toks[i].v == "header" {
                i = header(&toks, i, &mut st)?;
            } else {
                i = skip_block(&toks, i + 1)?;
            }
        }
        if i >= toks.len() || toks[i].t != T::Sym('}') {
            return refuse("a \\score block that is never closed", 0);
        }
        i += 1;
    }
    if i < toks.len() {
        return refuse(format!("'{}' after the end of the music", toks[i].v), toks[i].at);
    }
    Ok(Parsed { header: st.header, staves, groups })
}

fn preamble(toks: &[Tok], mut i: usize, st: &mut Base) -> Result<usize, ScoreError> {
    while i < toks.len() && toks[i].t == T::Cmd && (toks[i].v == "version" || toks[i].v == "header") {
        if toks[i].v == "version" {
            i += 2;
        } else {
            i = header(toks, i, st)?;
        }
    }
    Ok(i)
}

fn skip_block(toks: &[Tok], mut i: usize) -> Result<usize, ScoreError> {
    if i >= toks.len() || toks[i].t != T::Sym('{') {
        return refuse("a { block } was expected", at_of(toks, i));
    }
    let mut depth = 0;
    while i < toks.len() {
        if toks[i].t == T::Sym('{') {
            depth += 1;
        }
        if toks[i].t == T::Sym('}') {
            depth -= 1;
        }
        i += 1;
        if depth == 0 {
            break;
        }
    }
    Ok(i)
}

fn double(toks: &[Tok], i: usize, kind: char) -> bool {
    i + 1 < toks.len() && toks[i].t == T::Sym(kind) && toks[i + 1].t == T::Sym(kind) && toks[i + 1].at == toks[i].at + 1
}

fn group_kind(kind: &str) -> Option<GroupKind> {
    match kind {
        "PianoStaff" | "GrandStaff" => Some(GroupKind::Brace),
        "StaffGroup" => Some(GroupKind::Bracket),
        "ChoirStaff" => Some(GroupKind::Choir),
        _ => None,
    }
}

fn part(toks: &[Tok], mut i: usize, st: &mut Base, staves: &mut Vec<Staff>, groups: &mut Vec<Group>, depth: usize) -> Result<usize, ScoreError> {
    if i >= toks.len() {
        return refuse("there is no music", 0);
    }
    let tk = toks[i].clone();
    if tk.t == T::Cmd && tk.v == "new" {
        let kind = toks.get(i + 1).map(|t| t.v.clone()).unwrap_or_default();
        if kind != "Staff" && group_kind(&kind).is_none() {
            return refuse(format!("\\new {kind} is not drawn: a Staff, a PianoStaff, a GrandStaff, a StaffGroup or a ChoirStaff is"), tk.at);
        }
        i += 2;
        if i + 1 < toks.len() && toks[i].t == T::Sym('=') && toks[i + 1].t == T::Str {
            i += 2;
        }
        let mut props = Props::default();
        if is_cmd(toks, i, "with") {
            i = with(toks, i, &mut props)?;
        }
        if kind == "Staff" {
            return staff(toks, i, st, staves, props);
        }
        if !double(toks, i, '<') {
            return refuse(format!("\\new {kind} wants its staves between << and >>"), tk.at);
        }
        let gk = group_kind(&kind).unwrap_or(GroupKind::None);
        return group(toks, i, st, staves, groups, depth, gk, &props);
    }
    if double(toks, i, '<') {
        return group(toks, i, st, staves, groups, depth, GroupKind::None, &Props::default());
    }
    staff(toks, i, st, staves, Props::default())
}

#[allow(clippy::too_many_arguments)]
fn group(toks: &[Tok], mut i: usize, st: &mut Base, staves: &mut Vec<Staff>, groups: &mut Vec<Group>, depth: usize, kind: GroupKind, props: &Props) -> Result<usize, ScoreError> {
    let at = toks[i].at;
    i += 2;
    let first = staves.len();
    while i < toks.len() && !double(toks, i, '>') {
        if !(toks[i].t == T::Cmd && toks[i].v == "new") {
            return refuse("two voices in one staff are not drawn: give each its own \\new Staff", toks[i].at);
        }
        i = part(toks, i, st, staves, groups, depth + 1)?;
        i = attach_lyrics(toks, i, staves)?;
    }
    if i >= toks.len() {
        return refuse("a << that is never closed by >>", at);
    }
    if staves.len() == first {
        return refuse("<< >> with no staff in it", at);
    }
    groups.push(Group { kind, first, last: staves.len() - 1, depth, name: props.instrument_name.clone().unwrap_or_default(), short: props.short_instrument_name.clone().unwrap_or_default() });
    Ok(i + 2)
}

fn with(toks: &[Tok], mut i: usize, props: &mut Props) -> Result<usize, ScoreError> {
    let at = toks[i].at;
    if !is(toks, i + 1, T::Sym('{')) {
        return refuse("\\with wants a { block }", at);
    }
    i += 2;
    while i < toks.len() && toks[i].t != T::Sym('}') {
        i = staff_prop(toks, i, props, "\\with")?;
    }
    if i >= toks.len() {
        return refuse("a \\with block that is never closed", at);
    }
    Ok(i + 1)
}

fn staff_prop(toks: &[Tok], mut i: usize, props: &mut Props, what: &str) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let names = ["instrumentName", "shortInstrumentName", "midiInstrument", "midiMaximumVolume", "midiPanPosition"];
    let ok = tk.t == T::Word && names.contains(&tk.v.as_str()) && is(toks, i + 1, T::Sym('='));
    if !ok {
        return refuse(format!("{what} sets instrumentName, shortInstrumentName, midiInstrument, midiMaximumVolume or midiPanPosition, not '{}'", tk.v), tk.at);
    }
    i += 2;
    if is(toks, i, T::Sym('#')) {
        i += 1;
    }
    if tk.v == "midiMaximumVolume" || tk.v == "midiPanPosition" {
        let mut sign = 1.0;
        if is(toks, i, T::Sym('-')) {
            sign = -1.0;
            i += 1;
        }
        if !(i < toks.len() && toks[i].t == T::Num && !toks[i].v.contains('/')) {
            return refuse(format!("{} wants a number", tk.v), tk.at);
        }
        let mut value: f64 = toks[i].v.parse().unwrap_or(0.0);
        i += 1;
        if i + 1 < toks.len() && toks[i].t == T::Sym('.') && toks[i + 1].t == T::Num && toks[i + 1].at == toks[i].at + 1 {
            let digits = &toks[i + 1].v;
            let mut tenths = 1.0;
            for _ in 0..digits.chars().count() {
                tenths *= 10.0;
            }
            value += digits.parse::<f64>().unwrap_or(0.0) / tenths;
            i += 2;
        }
        if tk.v == "midiMaximumVolume" {
            props.midi_maximum_volume = Some(sign * value);
        } else {
            props.midi_pan_position = Some(sign * value);
        }
        return Ok(i);
    }
    if !is(toks, i, T::Str) {
        return refuse(format!("{} wants a \"string\"", tk.v), tk.at);
    }
    let value = toks[i].v.clone();
    match tk.v.as_str() {
        "instrumentName" => props.instrument_name = Some(value),
        "shortInstrumentName" => props.short_instrument_name = Some(value),
        _ => props.midi_instrument = Some(value),
    }
    Ok(i + 1)
}

fn staff(toks: &[Tok], mut i: usize, base: &mut Base, staves: &mut Vec<Staff>, props: Props) -> Result<usize, ScoreError> {
    let mut st = St { reference: None, base: 4, dots: 0, events: Vec::new(), notes: base.notes, items: base.counter, props, alts: 0, tuplets: 0, tuplet: None, voice: None };
    if is_cmd(toks, i, "relative") {
        let pitch = toks.get(i + 1);
        let name = pitch.and_then(|p| pitch_name(&p.v));
        match (pitch, name) {
            (Some(p), Some((step, _))) if p.t == T::Word => {
                st.reference = Some(resolve(step, p.marks, None));
            }
            _ => return refuse("\\relative wants a pitch", toks[i].at),
        }
        i += 2;
    }
    let braced = is(toks, i, T::Sym('{'));
    if braced {
        i += 1;
    }
    i = music(toks, i, &mut st, &mut base.header)?;
    if braced {
        if i >= toks.len() || toks[i].t != T::Sym('}') {
            return refuse("a '{' that is never closed", 0);
        }
        i += 1;
    }
    base.counter = st.items;
    base.notes = st.notes;
    staves.push(Staff { events: st.events, props: st.props, lyrics: None });
    Ok(i)
}

fn header(toks: &[Tok], mut i: usize, st: &mut Base) -> Result<usize, ScoreError> {
    let fields = ["dedication", "title", "subtitle", "subsubtitle", "poet", "instrument", "composer", "meter", "arranger", "piece", "opus", "copyright"];
    let at = toks[i].at;
    i += 1;
    if i >= toks.len() || toks[i].t != T::Sym('{') {
        return refuse("\\header wants a { block }", at);
    }
    i += 1;
    while i < toks.len() && toks[i].t != T::Sym('}') {
        let name = toks[i].clone();
        let ok = name.t == T::Word && i + 2 < toks.len() && toks[i + 1].t == T::Sym('=') && toks[i + 2].t == T::Str;
        if !ok {
            return refuse("a header field is a name, = and a string", name.at);
        }
        if !fields.contains(&name.v.as_str()) {
            return refuse(format!("the header field '{}' is not one a title page prints", name.v), name.at);
        }
        st.header.insert(name.v.clone(), toks[i + 2].v.clone());
        i += 3;
    }
    if i >= toks.len() {
        return refuse("a \\header that is never closed", at);
    }
    Ok(i + 1)
}

fn music(toks: &[Tok], mut i: usize, st: &mut St, header_fields: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    while i < toks.len() {
        let tk = toks[i].clone();
        match tk.t {
            T::Sym('}') => break,
            T::Cmd => i = command(toks, i, st, header_fields)?,
            T::Sym('<') if twin(toks, i, '<') => i = voices(toks, i, st, header_fields)?,
            T::Word | T::Sym('<') => i = note(toks, i, st)?,
            T::Sym('|') => i += 1,
            T::Sym('~') => {
                last_note(&mut st.events, &tk)?.tie = true;
                i += 1;
            }
            T::Sym('-' | '^' | '_') => i = postfix(toks, i, st, header_fields)?,
            T::Sym(c @ ('(' | ')' | '[' | ']')) => {
                let last = last_note(&mut st.events, &tk)?;
                match c {
                    '(' => last.slur_on = true,
                    ')' => last.slur_off = true,
                    '[' => last.beam_on = true,
                    _ => last.beam_off = true,
                }
                i += 1;
            }
            _ => return refuse(format!("'{}' is not expected here", tk.v), tk.at),
        }
    }
    Ok(i)
}

/// `<<` or `>>`: two of the sign written together.
fn twin(toks: &[Tok], i: usize, sign: char) -> bool {
    matches!((toks.get(i), toks.get(i + 1)), (Some(a), Some(b)) if a.t == T::Sym(sign) && b.t == T::Sym(sign) && b.at == a.at + 1)
}

/// Two voices on one staff, `<< { … } \\ { … } >>`: each from the pitch
/// before `<<` under `\relative`, what follows from the first voice's last.
fn voices(toks: &[Tok], mut i: usize, st: &mut St, header_fields: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let open_at = toks[i].at;
    i += 2;
    let start_ref = st.reference;
    let mut after_ref = None;
    let mut voice = 1u8;
    st.events.push(Ev { k: Kind::Voices, at: open_at, ..Ev::default() });
    loop {
        if !is(toks, i, T::Sym('{')) {
            return refuse("<< wants { music } \\\\ { music } >>", open_at);
        }
        st.reference = start_ref;
        st.voice = Some(voice);
        i = music(toks, i + 1, st, header_fields)?;
        if i >= toks.len() {
            return refuse("a voice that is never closed", open_at);
        }
        i += 1;
        if voice == 1 {
            after_ref = st.reference;
        }
        if is(toks, i, T::Sym('\\')) {
            if voice == 2 {
                return refuse("two voices on a staff are what this draws", toks[i].at);
            }
            st.events.push(Ev { k: Kind::VoiceNext, at: toks[i].at, ..Ev::default() });
            voice += 1;
            i += 1;
        } else if twin(toks, i, '>') {
            i += 2;
            break;
        } else {
            return refuse("<< { … } \\\\ { … } >> is not closed with >>", open_at);
        }
    }
    if voice < 2 {
        return refuse("<< { … } >> with one voice: write the voices apart with \\\\", open_at);
    }
    st.voice = None;
    st.reference = after_ref;
    st.events.push(Ev { k: Kind::VoicesEnd, at: open_at, ..Ev::default() });
    Ok(i)
}

fn postfix(toks: &[Tok], i: usize, st: &mut St, header_fields: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let dir = match tk.t {
        T::Sym('^') => -1,
        T::Sym('_') => 1,
        _ => 0,
    };
    if i + 1 >= toks.len() {
        return refuse(format!("'{}' wants something after it: -. -> -- ^\"text\"", tk.v), tk.at);
    }
    let nxt = toks[i + 1].clone();
    if nxt.t == T::Str {
        let last = last_item(&mut st.events, &tk)?;
        last.texts.push(Words { text: nxt.v, dir: if dir == 0 { 1 } else { dir } });
        return Ok(i + 2);
    }
    if nxt.t == T::Cmd {
        if SCRIPTS.contains(&nxt.v.as_str()) {
            let last = last_note(&mut st.events, &tk)?;
            last.scripts.push(Script { name: nxt.v, dir });
            return Ok(i + 2);
        }
        return command(toks, i + 1, st, header_fields);
    }
    let name = match nxt.t {
        T::Sym('.') => "staccato",
        T::Sym('>') => "accent",
        T::Sym('-') => "tenuto",
        T::Sym('^') => "marcato",
        T::Sym('!') => "staccatissimo",
        T::Sym('_') => "portato",
        _ => return refuse(format!("'{}{}' is not an articulation this draws", tk.v, nxt.v), tk.at),
    };
    let last = last_note(&mut st.events, &tk)?;
    last.scripts.push(Script { name: name.to_owned(), dir });
    Ok(i + 2)
}

fn last_item<'a>(evs: &'a mut [Ev], tk: &Tok) -> Result<&'a mut Ev, ScoreError> {
    match evs.last_mut() {
        Some(last) if last.k == Kind::Note || last.k == Kind::Rest => Ok(last),
        _ => refuse(format!("'{}' must follow a note or a rest", tk.v), tk.at),
    }
}

fn last_note<'a>(evs: &'a mut [Ev], tk: &Tok) -> Result<&'a mut Ev, ScoreError> {
    match evs.last_mut() {
        Some(last) if last.k == Kind::Note => Ok(last),
        _ => refuse(format!("'{}' must follow a note", tk.v), tk.at),
    }
}

fn command(toks: &[Tok], i: usize, st: &mut St, header_fields: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let word = tk.v.as_str();
    let arg = toks.get(i + 1).cloned();
    // The first voice carries the staff's changes, as the bars are its.
    if st.voice == Some(2) && ["clef", "key", "time", "bar", "tempo", "break", "pageBreak", "repeat", "alternative", "compressEmptyMeasures"].contains(&word) {
        return refuse(format!("\\{word} belongs to the staff: write it in the first voice"), tk.at);
    }
    match word {
        "clef" => {
            let Some(arg) = arg else { return refuse("\\clef wants a name", tk.at) };
            let clef = match arg.v.as_str() {
                "treble" | "violin" | "G" => Clef::Treble,
                "bass" | "F" => Clef::Bass,
                "alto" | "C" => Clef::Alto,
                other => return refuse(format!("the clef '{other}' is not one this draws (treble, bass, alto)"), arg.at),
            };
            st.events.push(Ev { k: Kind::Clef, clef, at: tk.at, ..Ev::default() });
            return Ok(i + 2);
        }
        "key" => {
            let mode = toks.get(i + 2).cloned();
            let name = arg.as_ref().and_then(|a| pitch_name(&a.v));
            let (Some((step, alter)), Some(mode)) = (name, mode) else { return refuse("\\key wants a pitch and \\major or \\minor", tk.at) };
            if mode.t != T::Cmd {
                return refuse("\\key wants a pitch and \\major or \\minor", tk.at);
            }
            if mode.v != "major" && mode.v != "minor" {
                return refuse(format!("the mode '{}' is not \\major or \\minor", mode.v), mode.at);
            }
            let mut fifths = [0, 2, 4, -1, 1, 3, 5][step as usize] + 7 * alter;
            if mode.v == "minor" {
                fifths -= 3;
            }
            if !(-7..=7).contains(&fifths) {
                return refuse(format!("a key of {fifths} fifths has no signature"), tk.at);
            }
            st.events.push(Ev { k: Kind::Key, fifths, at: tk.at, ..Ev::default() });
            return Ok(i + 3);
        }
        "time" => {
            let parts: Vec<String> = arg.as_ref().map(|a| a.v.split('/').map(str::to_owned).collect()).unwrap_or_default();
            if parts.len() != 2 {
                return refuse("\\time wants a fraction like 3/4", tk.at);
            }
            let num: i64 = parts[0].parse().unwrap_or(0);
            let den: i64 = parts[1].parse().unwrap_or(0);
            if !(num > 0 && num < 13 && [1, 2, 4, 8, 16].contains(&den)) {
                let a = arg.unwrap_or(tk.clone());
                return refuse(format!("\\time {} is not a metre this draws", a.v), a.at);
            }
            st.events.push(Ev { k: Kind::Time, time: (num, den), at: tk.at, ..Ev::default() });
            return Ok(i + 2);
        }
        "bar" => {
            let Some(arg) = arg.filter(|a| a.t == T::Str) else { return refuse("\\bar wants a string", tk.at) };
            if !["|", "||", "|.", ".|:", ":|.", ":|.|:"].contains(&arg.v.as_str()) {
                return refuse(format!("the bar line \"{}\" is not one this draws (|, ||, |., .|:, :|., :|.|:)", arg.v), arg.at);
            }
            st.events.push(Ev { k: Kind::Bar, v: arg.v, at: tk.at, ..Ev::default() });
            return Ok(i + 2);
        }
        "tempo" => return tempo(toks, i, st),
        _ => {}
    }
    if DYNAMICS.contains(&word) {
        last_item(&mut st.events, &tk)?.dynamic = Some(word.to_owned());
        return Ok(i + 1);
    }
    if word == "<" || word == ">" {
        last_item(&mut st.events, &tk)?.hairpin = word.chars().next();
        return Ok(i + 1);
    }
    if word == "!" {
        last_item(&mut st.events, &tk)?.hairpin_end = true;
        return Ok(i + 1);
    }
    if SCRIPTS.contains(&word) {
        last_note(&mut st.events, &tk)?.scripts.push(Script { name: word.to_owned(), dir: 0 });
        return Ok(i + 1);
    }
    if word == "break" || word == "pageBreak" {
        st.events.push(Ev { k: Kind::Break, page: word == "pageBreak", at: tk.at, ..Ev::default() });
        return Ok(i + 1);
    }
    if word == "set" {
        let ok = i + 3 < toks.len() && toks[i + 1].t == T::Word && toks[i + 1].v == "Staff" && toks[i + 2].t == T::Sym('.');
        if !ok {
            return refuse("\\set sets Staff.instrumentName, Staff.shortInstrumentName or Staff.midiInstrument", tk.at);
        }
        return staff_prop(toks, i + 3, &mut st.props, "\\set Staff");
    }
    if word == "tuplet" || word == "times" {
        return tuplet(toks, i, st, word, header_fields);
    }
    if word == "repeat" {
        return repeat(toks, i, st, header_fields);
    }
    if word == "alternative" {
        return alternative(toks, i, st, header_fields);
    }
    if word == "compressEmptyMeasures" {
        st.events.push(Ev { k: Kind::Compress, at: tk.at, ..Ev::default() });
        return Ok(i + 1);
    }
    if word == "compressMMRests" {
        st.events.push(Ev { k: Kind::Compress, at: tk.at, ..Ev::default() });
        if !arg.is_some_and(|a| a.t == T::Sym('{')) {
            return refuse("\\compressMMRests wants a { block }", tk.at);
        }
        let j = music(toks, i + 2, st, header_fields)?;
        if j >= toks.len() || toks[j].t != T::Sym('}') {
            return refuse("a \\compressMMRests block that is never closed", tk.at);
        }
        return Ok(j + 1);
    }
    refuse(format!("\\{word} is not a command this reads"), tk.at)
}

fn repeat(toks: &[Tok], i: usize, st: &mut St, header_fields: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let ok = i + 3 < toks.len() && toks[i + 1].t == T::Word && toks[i + 1].v == "volta" && toks[i + 2].t == T::Num && toks[i + 3].t == T::Sym('{');
    if !ok {
        return refuse("\\repeat wants volta, a count and a { block } — the others are not drawn", tk.at);
    }
    st.events.push(Ev { k: Kind::RepeatStart, at: tk.at, ..Ev::default() });
    let alts_before = st.alts;
    let mut j = music(toks, i + 4, st, header_fields)?;
    if j >= toks.len() || toks[j].t != T::Sym('}') {
        return refuse("a \\repeat block that is never closed", tk.at);
    }
    j += 1;
    if is_cmd(toks, j, "alternative") {
        j = alternative(toks, j, st, header_fields)?;
    }
    if st.alts == alts_before {
        st.events.push(Ev { k: Kind::RepeatEnd, at: tk.at, ..Ev::default() });
    }
    Ok(j)
}

fn alternative(toks: &[Tok], i: usize, st: &mut St, header_fields: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    if !is(toks, i + 1, T::Sym('{')) {
        return refuse("\\alternative wants a { block } of { endings }", tk.at);
    }
    let mut j = i + 2;
    let mut endings = Vec::new();
    while j < toks.len() && toks[j].t == T::Sym('{') {
        endings.push(j);
        let mut depth = 0;
        let mut jj = j;
        while jj < toks.len() {
            if toks[jj].t == T::Sym('{') {
                depth += 1;
            }
            if toks[jj].t == T::Sym('}') {
                depth -= 1;
            }
            if depth == 0 {
                break;
            }
            jj += 1;
        }
        j = jj + 1;
    }
    if endings.is_empty() {
        return refuse("an \\alternative with no endings", tk.at);
    }
    if j >= toks.len() || toks[j].t != T::Sym('}') {
        return refuse("an \\alternative that is never closed", tk.at);
    }
    let close_at = j;
    let count = endings.len();
    for (k, start) in endings.into_iter().enumerate() {
        let last = k == count - 1;
        st.events.push(Ev { k: Kind::VoltaStart, v: format!("{}.", k + 1), at: toks[start].at, ..Ev::default() });
        music(toks, start + 1, st, header_fields)?;
        if !last {
            st.events.push(Ev { k: Kind::RepeatEnd, at: toks[start].at, ..Ev::default() });
        }
        st.events.push(Ev { k: Kind::VoltaEnd, closed: !last, at: toks[start].at, ..Ev::default() });
    }
    st.alts += 1;
    Ok(close_at + 1)
}

fn tuplet(toks: &[Tok], i: usize, st: &mut St, word: &str, header_fields: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let ok = i + 1 < toks.len() && toks[i + 1].t == T::Num && toks[i + 1].v.contains('/');
    if !ok {
        return refuse(format!("\\{word} wants a ratio, like 3/2, and a {{ block }}"), tk.at);
    }
    let parts: Vec<i64> = toks[i + 1].v.split('/').map(|p| p.parse().unwrap_or(0)).collect();
    let (mut n, mut d) = (parts.first().copied().unwrap_or(0), parts.get(1).copied().unwrap_or(0));
    if word == "times" {
        std::mem::swap(&mut n, &mut d);
    }
    if !((2..=12).contains(&n) && (1..=12).contains(&d) && n != d) {
        return refuse(format!("a tuplet of {n} in the time of {d} is not one this draws"), tk.at);
    }
    if st.tuplet.is_some() {
        return refuse("a tuplet inside a tuplet is not drawn", tk.at);
    }
    let mut j = i + 2;
    let mut span = 0;
    if is(toks, j, T::Num) {
        let base: i64 = toks[j].v.parse().unwrap_or(0);
        if ![1, 2, 4, 8, 16].contains(&base) {
            return refuse("a tuplet's span is a note value: 1, 2, 4, 8 or 16", toks[j].at);
        }
        span = WHOLE / base;
        j += 1;
    }
    if !is(toks, j, T::Sym('{')) {
        return refuse(format!("\\{word} wants a {{ block }}"), tk.at);
    }
    st.tuplets += 1;
    st.tuplet = Some(TupletState { id: st.tuplets, n, d, span, used: 0 });
    let k = music(toks, j + 1, st, header_fields)?;
    if k >= toks.len() || toks[k].t != T::Sym('}') {
        return refuse("a tuplet that is never closed", tk.at);
    }
    st.tuplet = None;
    Ok(k + 1)
}

fn tempo(toks: &[Tok], i: usize, st: &mut St) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let mut j = i + 1;
    let mut mark = Tempo::default();
    if is(toks, j, T::Str) {
        mark.text = toks[j].v.clone();
        j += 1;
    }
    if is(toks, j, T::Num) {
        let base: i64 = toks[j].v.parse().unwrap_or(0);
        let ok = [1, 2, 4, 8, 16].contains(&base) && j + 2 < toks.len() && toks[j + 1].t == T::Sym('=') && toks[j + 2].t == T::Num;
        if !ok {
            return refuse("\\tempo wants a note value, = and a number, like 4 = 120", tk.at);
        }
        mark.base = base;
        mark.bpm = toks[j + 2].v.parse().unwrap_or(0);
        if !(10..=400).contains(&mark.bpm) {
            return refuse(format!("a tempo of {} is not one anybody plays", mark.bpm), tk.at);
        }
        j += 3;
    }
    if mark.text.is_empty() && mark.bpm == 0 {
        return refuse("\\tempo wants words, a beat or both", tk.at);
    }
    st.events.push(Ev { k: Kind::Tempo, tempo: Some(mark), at: tk.at, ..Ev::default() });
    Ok(j)
}

fn note(toks: &[Tok], mut i: usize, st: &mut St) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let mut pitches = Vec::new();
    let mut rest: Option<String> = None;
    if tk.t == T::Sym('<') {
        i += 1;
        let mut reference = st.reference;
        let mut first = None;
        while i < toks.len() && toks[i].t == T::Word {
            let Some((step, alter)) = pitch_name(&toks[i].v) else { return refuse(format!("'{}' is not a note name", toks[i].v), toks[i].at) };
            let d = resolve(step, toks[i].marks, reference);
            if reference.is_some() {
                reference = Some(d);
            }
            if first.is_none() {
                first = Some(d);
            }
            pitches.push(Pitch { d, alter });
            i += 1;
        }
        if i >= toks.len() || toks[i].t != T::Sym('>') {
            return refuse("a chord that is never closed", tk.at);
        }
        if pitches.is_empty() {
            return refuse("an empty chord", tk.at);
        }
        if st.reference.is_some() {
            st.reference = first;
        }
        i += 1;
    } else {
        let word = tk.v.clone();
        if word == "r" || word == "R" || word == "s" {
            // `s`, a spacer: time and nothing drawn, where a second voice is silent.
            if word == "s" && st.voice != Some(2) {
                return refuse("a spacer s is for the second voice of << \\\\ >>", tk.at);
            }
            rest = Some(word);
        } else {
            let Some((step, alter)) = pitch_name(&word) else { return refuse(format!("'{word}' is not a note name"), tk.at) };
            let d = resolve(step, tk.marks, st.reference);
            if st.reference.is_some() {
                st.reference = Some(d);
            }
            pitches.push(Pitch { d, alter });
        }
        i += 1;
    }
    if is(toks, i, T::Num) {
        let base: i64 = toks[i].v.parse().unwrap_or(0);
        if ![1, 2, 4, 8, 16, 32].contains(&base) {
            return refuse(format!("the duration {} is not one this draws (1, 2, 4, 8, 16, 32)", toks[i].v), toks[i].at);
        }
        st.base = base;
        st.dots = 0;
        i += 1;
        while is(toks, i, T::Sym('.')) {
            st.dots += 1;
            i += 1;
        }
    }
    let mut count = 1;
    if is(toks, i, T::Sym('*')) {
        if rest.as_deref() != Some("R") {
            return refuse("a * count is for whole-bar rests, R1*4", toks[i].at);
        }
        if !(i + 1 < toks.len() && toks[i + 1].t == T::Num && !toks[i + 1].v.contains('/')) {
            return refuse("a * wants a count, R1*4", toks[i].at);
        }
        count = toks[i + 1].v.parse().unwrap_or(0);
        if count < 1 {
            return refuse("a count of bars is at least one", toks[i].at);
        }
        i += 2;
    }
    let base = st.base;
    let dots = st.dots;
    let mut ticks = WHOLE / base;
    let mut add = ticks;
    for _ in 0..dots {
        if add % 2 == 1 {
            return refuse(format!("too many dots for a 1/{base}"), tk.at);
        }
        add /= 2;
        ticks += add;
    }
    let mut ev = Ev { k: Kind::Note, pitches, base, dots, ticks, at: tk.at, i: st.items, ..Ev::default() };
    if let Some(v) = st.voice {
        if count > 1 {
            return refuse("a run of bars' rest is the staff's: write it outside << \\\\ >>", tk.at);
        }
        ev.voice = Some(v);
    }
    if let Some(mut tup) = st.tuplet {
        if rest.as_deref() == Some("R") {
            return refuse("a whole-bar rest cannot be in a tuplet", tk.at);
        }
        if tup.span > 0 && tup.used >= tup.span {
            st.tuplets += 1;
            tup.id = st.tuplets;
            tup.used = 0;
        }
        if (ticks * tup.d) % tup.n != 0 {
            return refuse(format!("a 1/{base} cannot be cut {} ways", tup.n), tk.at);
        }
        ev.ticks = ticks * tup.d / tup.n;
        ev.tuplet = Some(TupletRef { id: tup.id, n: tup.n, d: tup.d });
        tup.used += ev.ticks;
        st.tuplet = Some(tup);
    }
    st.items += 1;
    match rest {
        None => {
            ev.n = Some(st.notes);
            st.notes += 1;
        }
        Some(r) => {
            ev.k = Kind::Rest;
            ev.full = r == "R";
            ev.spacer = r == "s";
            ev.count = count;
        }
    }
    st.events.push(ev);
    Ok(i)
}
