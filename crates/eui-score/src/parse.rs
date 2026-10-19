//! Reading LilyPond: the subset `score()` draws, every construct outside it
//! refused with its position (`eui_builders_score.sl`, "Reading LilyPond").
//!
//! Beyond the Soli engraver's subset, what protocol 11 reads (03 §1.4): a
//! file as LilyPond's users write one — variables, `\paper`, `\layout` and
//! `\midi` blocks, `\language`, a `\new Voice` or two on a staff with a
//! `\global` of the staff's settings beside them, `\new Lyrics \lyricsto`
//! the voice it is named for — and `\fixed`, `\transpose`, `q`, `\partial`,
//! `\cadenzaOn`, spacers, and `\repeat unfold` and `percent`.

use std::cell::Cell;
use std::collections::HashMap;

use crate::glyphs::Weight;
use crate::{ScoreError, WHOLE};

/// The dynamics a score prints.
pub const DYNAMICS: [&str; 21] = ["pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff", "fp", "sf", "sff", "sp", "spp", "sfz", "sfp", "rfz", "rf", "fz", "n"];

/// The articulations a score prints, by their LilyPond names: the Soli
/// engraver's seven, and the ornaments and the hi-hat's signs protocol 11
/// adds.
pub const SCRIPTS: [&str; 13] = ["staccato", "tenuto", "accent", "marcato", "staccatissimo", "portato", "fermata", "trill", "prall", "mordent", "turn", "open", "stopped"];

/// The most tokens a source may come to once its variables are written out:
/// a variable used in a variable used in a variable multiplies.
const MAX_TOKENS: usize = 400_000;

thread_local! {
    /// The source being read uses what protocol 11 adds.
    static BEYOND: Cell<bool> = const { Cell::new(false) };
}

/// What protocol 11 adds was read: a client of protocol 10 cannot engrave
/// this source.
fn beyond() {
    BEYOND.with(|b| b.set(true));
}

/// The commands the Soli engraver reads, which a client of protocol 9 or
/// 10 engraves: a source using no other is one it can.
const READ_BEFORE: [&str; 28] = [
    "version",
    "header",
    "score",
    "new",
    "with",
    "relative",
    "clef",
    "key",
    "major",
    "minor",
    "time",
    "bar",
    "tempo",
    "break",
    "pageBreak",
    "set",
    "tuplet",
    "times",
    "repeat",
    "alternative",
    "compressEmptyMeasures",
    "compressMMRests",
    "addlyrics",
    "layout",
    "midi",
    "<",
    ">",
    "!",
];

/// The contexts it draws.
const DRAWN_BEFORE: [&str; 5] = ["Staff", "PianoStaff", "GrandStaff", "StaffGroup", "ChoirStaff"];

/// The protocol a client needs to engrave `src` (03 §1.4): 9 for what the
/// catalogue's Soli engraver reads, 11 for what protocol 11 adds. A server
/// sends an older session the paths instead.
///
/// # Errors
///
/// The engraver's refusal, for a source it cannot set at all.
pub fn protocol(src: &str) -> Result<u32, ScoreError> {
    BEYOND.with(|b| b.set(false));
    let read = parse(src);
    let level = if BEYOND.with(Cell::get) { 11 } else { 9 };
    BEYOND.with(|b| b.set(false));
    read.map(|_| level)
}

/// Marks a source using a token the Soli engraver does not read.
fn scan_beyond(toks: &[Tok]) {
    for (k, t) in toks.iter().enumerate() {
        let new_cmd = t.t == T::Cmd && !READ_BEFORE.contains(&t.v.as_str()) && !DYNAMICS.contains(&t.v.as_str()) && !SCRIPTS[..7].contains(&t.v.as_str());
        let new_sym = matches!(t.t, T::Sym(':' | '/' | '+' | '?' | '\''));
        let new_context = k > 0 && toks[k - 1].t == T::Cmd && toks[k - 1].v == "new" && !DRAWN_BEFORE.contains(&t.v.as_str());
        let joined = (t.t == T::Word || t.t == T::Cmd) && (t.v.contains('-') || t.v.contains('_'));
        if new_cmd || new_sym || new_context || joined {
            beyond();
            return;
        }
    }
}

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
    /// The `{ … }` block after `\addlyrics`, `\lyricmode` or `\lyricsto`,
    /// read whole.
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

impl Tok {
    fn new(t: T, v: impl Into<String>, at: usize) -> Self {
        Tok { t, v: v.into(), marks: 0, at, lyrics: Vec::new() }
    }
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

/// A letter of a note name or a word: French names its D `ré`.
fn word_letter(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == 'é'
}

fn digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

/// A name's letters from `i`, a `-` or `_` between two letters kept in it as
/// LilyPond keeps it in `\violin-one` or `voice_two`.
fn name_at(cs: &[char], mut i: usize, ok: fn(char) -> bool) -> (String, usize) {
    let mut word = String::new();
    while i < cs.len() {
        let c = cs[i];
        let joiner = (c == '-' || c == '_') && !word.is_empty() && cs.get(i + 1).is_some_and(|n| ok(*n));
        if !(ok(c) || joiner) {
            break;
        }
        word.push(c);
        i += 1;
    }
    (word, i)
}

fn blank(ch: char) -> bool {
    ch == ' ' || ch == '\n' || ch == '\t' || ch == '\r'
}

/// The first character from `i` that is neither blank nor in a comment.
fn skip_blank(cs: &[char], mut i: usize) -> usize {
    while i < cs.len() {
        if blank(cs[i]) {
            i += 1;
        } else if cs[i] == '%' {
            while i < cs.len() && cs[i] != '\n' {
                i += 1;
            }
        } else {
            break;
        }
    }
    i
}

pub(crate) fn tokens(src: &str) -> Result<Vec<Tok>, ScoreError> {
    let cs: Vec<char> = src.chars().collect();
    let n = cs.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let ch = cs[i];
        if blank(ch) {
            i += 1;
        } else if ch == '%' {
            // `%{ … %}` is a block comment; `%` anything else, a line's.
            if cs.get(i + 1) == Some(&'{') {
                beyond();
                i += 2;
                while i < n && !(cs[i] == '%' && cs.get(i + 1) == Some(&'}')) {
                    i += 1;
                }
                i = (i + 2).min(n);
            } else {
                while i < n && cs[i] != '\n' {
                    i += 1;
                }
            }
        } else if ch == '\\' && i + 1 < n && cs[i + 1] == '\\' {
            // `\\` between the voices of `<< { … } \\ { … } >>`.
            out.push(Tok::new(T::Sym('\\'), "\\\\", i));
            i += 2;
        } else if ch == '\\' && i + 1 < n && "<>!()".contains(cs[i + 1]) {
            out.push(Tok::new(T::Cmd, cs[i + 1].to_string(), i));
            i += 2;
        } else if ch == '\\' && i + 1 < n && digit(cs[i + 1]) {
            // `c\5`: the string a note is played on, in a tablature.
            let mut jj = i + 1;
            let mut word = String::new();
            while jj < n && digit(cs[jj]) {
                word.push(cs[jj]);
                jj += 1;
            }
            out.push(Tok::new(T::Cmd, word, i));
            i = jj;
        } else if ch == '\\' {
            let (word, jj) = name_at(&cs, i + 1, letter);
            if word.is_empty() {
                return refuse("a backslash with no command after it", i);
            }
            let at = i;
            i = jj;
            let lyric = matches!(word.as_str(), "addlyrics" | "lyricmode" | "lyrics" | "lyricsto");
            let addlyrics = word == "addlyrics";
            let lyricsto = word == "lyricsto";
            out.push(Tok::new(T::Cmd, word, at));
            if lyric {
                let mut k = skip_blank(&cs, i);
                if lyricsto {
                    if !(k < n && cs[k] == '"') {
                        return refuse("\\lyricsto wants the name of a voice, in quotes", at);
                    }
                    let mut jj = k + 1;
                    let mut name = String::new();
                    while jj < n && cs[jj] != '"' {
                        name.push(cs[jj]);
                        jj += 1;
                    }
                    if jj >= n {
                        return refuse("a string that is never closed", k);
                    }
                    out.push(Tok::new(T::Str, name, k));
                    i = jj + 1;
                    k = skip_blank(&cs, i);
                }
                if k < n && cs[k] == '{' {
                    let (items, next) = lyric_tokens(&cs, k, at)?;
                    out.push(Tok { t: T::Lyrics, v: String::new(), marks: 0, at, lyrics: items });
                    i = next;
                } else if addlyrics && !(k < n && cs[k] == '\\') {
                    return refuse("\\addlyrics wants a { block } of words", at);
                }
            }
        } else if ch == '"' {
            let mut jj = i + 1;
            let mut word = String::new();
            while jj < n && cs[jj] != '"' {
                if cs[jj] == '\\' && jj + 1 < n && (cs[jj + 1] == '"' || cs[jj + 1] == '\\') {
                    beyond();
                    jj += 1;
                }
                word.push(cs[jj]);
                jj += 1;
            }
            if jj >= n {
                return refuse("a string that is never closed", i);
            }
            out.push(Tok::new(T::Str, word, i));
            i = jj + 1;
        } else if "{}<>[]()~|.=*-^_!#:/+?'".contains(ch) {
            out.push(Tok::new(T::Sym(ch), ch.to_string(), i));
            i += 1;
        } else if digit(ch) {
            let mut jj = i;
            let mut word = String::new();
            while jj < n && (digit(cs[jj]) || (cs[jj] == '/' && cs.get(jj + 1).is_some_and(|c| digit(*c)))) {
                word.push(cs[jj]);
                jj += 1;
            }
            out.push(Tok::new(T::Num, word, i));
            i = jj;
        } else if word_letter(ch) {
            let (word, mut jj) = name_at(&cs, i, word_letter);
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

/// A lyric block from its `{`, read as LilyPond reads lyrics: words split
/// on spaces, any letter in them — "Frè", "l'eau" — a quoted string as one,
/// `--` joining a word's syllables, `__` holding one over the notes after it,
/// `_` a note that takes none; a duration written on a word (`Twin4`) is
/// LilyPond's and not the word's, `\skip 4` a note that takes none, and
/// `\set stanza = "1."` is passed over. Answers the items and where the
/// block ends.
fn lyric_tokens(cs: &[char], mut i: usize, at: usize) -> Result<(Vec<LyricItem>, usize), ScoreError> {
    let n = cs.len();
    i = skip_blank(cs, i);
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
                let (word, jj) = name_at(cs, i + 1, letter);
                let after = skip_blank(cs, jj);
                if (word == "skip" && after < n && digit(cs[after])) || word == "set" {
                    beyond();
                }
                if word == "skip" && after < n && digit(cs[after]) {
                    let mut k = after;
                    while k < n && (digit(cs[k]) || cs[k] == '.') {
                        k += 1;
                    }
                    items.push(LyricItem { s: "_".into(), quoted: false, at: i });
                    i = k;
                    continue;
                }
                if word == "set" {
                    // `\set stanza = "1."`, to the end of its string.
                    let mut k = after;
                    while k < n && cs[k] != '"' && cs[k] != '}' && cs[k] != '\n' {
                        k += 1;
                    }
                    if k < n && cs[k] == '"' {
                        k += 1;
                        while k < n && cs[k] != '"' {
                            k += 1;
                        }
                        i = (k + 1).min(n);
                        continue;
                    }
                }
                return refuse("'\\' inside lyrics is not read: they are words, -- and __", i);
            }
            let mut jj = i;
            let mut word = String::new();
            while jj < n && !" \n\t\r{}\"".contains(cs[jj]) {
                word.push(cs[jj]);
                jj += 1;
            }
            items.push(LyricItem { s: without_duration(&word), quoted: false, at: i });
            i = jj;
        }
    }
    Ok((items, i + 1))
}

/// A lyric word without the duration LilyPond lets one carry: `Twin4` is
/// "Twin", `star8.` "star"; a word that is all figures keeps them.
fn without_duration(word: &str) -> String {
    let kept = word.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    if kept.is_empty() || kept.len() == word.len() || !word[kept.len()..].starts_with(|c: char| c.is_ascii_digit()) {
        return word.to_owned();
    }
    beyond();
    kept.to_owned()
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
        let mut j = i + 1;
        if is_cmd(toks, j, "lyricmode") {
            j += 1;
        }
        if !is(toks, j, T::Lyrics) {
            return refuse("\\addlyrics wants a { block } of words", at);
        }
        last.lyrics = Some(syllables(&toks[j].lyrics, at)?);
        last.lyrics_voice = 1;
        i = j + 1;
    }
    Ok(i)
}

/// A language LilyPond names its notes in (`\language`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// LilyPond's default: `cis`, `es`, `bes`.
    #[default]
    Nederlands,
    /// `cs`, `ef`, `bf`, `fss`.
    English,
    /// `cis`, `es`, `h`, and `b` for B flat.
    Deutsch,
    /// `do`, `re`, `mi`, `fad`, `sib`.
    Italiano,
    /// `do`, `ré`, `mi`, `fad`, `sib`.
    Francais,
    /// `do`, `re`, `mi`, `fas`, `sib`.
    Espanol,
}

impl Lang {
    fn named(name: &str) -> Option<Lang> {
        Some(match name {
            "nederlands" => Lang::Nederlands,
            "english" => Lang::English,
            "deutsch" => Lang::Deutsch,
            "italiano" => Lang::Italiano,
            "français" | "francais" => Lang::Francais,
            "español" | "espanol" => Lang::Espanol,
            _ => return None,
        })
    }
}

/// A note name in `lang`: `(step, alter)`.
pub(crate) fn pitch_name_in(lang: Lang, word: &str) -> Option<(i64, i64)> {
    match lang {
        Lang::Nederlands => {
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
        Lang::English => {
            let mut chars = word.chars();
            let first = chars.next()?;
            let step = "cdefgab".find(first)? as i64;
            let rest: String = chars.collect();
            let alter = match rest.trim_start_matches('-') {
                "" => 0,
                "s" | "sharp" => 1,
                "ss" | "x" | "sharpsharp" => 2,
                "f" | "flat" => -1,
                "ff" | "flatflat" => -2,
                _ => return None,
            };
            Some((step, alter))
        }
        Lang::Deutsch => {
            let named = match word {
                "b" => Some((6, -1)),
                "heses" | "bes" => Some((6, -2)),
                "h" => Some((6, 0)),
                "his" => Some((6, 1)),
                "hisis" => Some((6, 2)),
                _ => None,
            };
            if named.is_some() {
                return named;
            }
            if word.starts_with('b') {
                return None;
            }
            pitch_name_in(Lang::Nederlands, word)
        }
        Lang::Italiano | Lang::Francais | Lang::Espanol => {
            let names: [&[&str]; 7] = [&["do"], &["re", "ré"], &["mi"], &["fa"], &["sol"], &["la"], &["si"]];
            let (step, rest) = names.iter().enumerate().find_map(|(k, ns)| ns.iter().find_map(|nm| word.strip_prefix(nm).map(|r| (k as i64, r))))?;
            let alter = match (lang, rest) {
                (_, "") => 0,
                (Lang::Espanol, "s") | (Lang::Italiano | Lang::Francais, "d") => 1,
                (Lang::Espanol, "ss" | "x") | (Lang::Italiano | Lang::Francais, "dd") | (Lang::Francais, "x") => 2,
                (_, "b") => -1,
                (_, "bb") => -2,
                _ => return None,
            };
            Some((step, alter))
        }
    }
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

/// The semitones of the seven steps above C.
const NATURAL: [i64; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Semitones from c,,, of a pitch.
pub(crate) fn semitones(d: i64, alter: i64) -> i64 {
    12 * d.div_euclid(7) + NATURAL[d.rem_euclid(7) as usize] + alter
}

/// The sign a clef is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sign {
    /// The G clef.
    #[default]
    G,
    /// The C clef.
    C,
    /// The F clef.
    F,
    /// A percussion staff's.
    Percussion,
    /// A tablature's "TAB".
    Tab,
}

/// A clef: its sign, the line it is on, and the octaves it moves the music.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clef {
    /// The sign.
    pub sign: Sign,
    /// The line, as a staff position: half spaces up from the middle line.
    pub line: i64,
    /// Octaves: `treble_8` is -1, `treble^8` 1.
    pub octave: i64,
}

impl Clef {
    /// The G clef on the second line.
    pub const TREBLE: Clef = Clef { sign: Sign::G, line: -2, octave: 0 };
    /// The F clef on the fourth line.
    pub const BASS: Clef = Clef { sign: Sign::F, line: 2, octave: 0 };
    /// The C clef on the third line.
    pub const ALTO: Clef = Clef { sign: Sign::C, line: 0, octave: 0 };

    /// A clef by its LilyPond name: `treble`, `"bass_8"`, `tenor`…
    pub fn named(name: &str) -> Option<Clef> {
        let (base, octave) = match name.find(['_', '^']) {
            None => (name, 0),
            Some(k) => {
                let by = match name[k + 1..].trim_matches(['(', ')', '[', ']']) {
                    "8" => 1,
                    "15" => 2,
                    _ => return None,
                };
                (&name[..k], if name[k..].starts_with('_') { -by } else { by })
            }
        };
        let (sign, line) = match base {
            "treble" | "violin" | "G" | "G2" => (Sign::G, -2),
            "french" | "G1" => (Sign::G, -4),
            "soprano" | "C1" => (Sign::C, -4),
            "mezzosoprano" | "C2" => (Sign::C, -2),
            "alto" | "C" | "C3" => (Sign::C, 0),
            "tenor" | "C4" => (Sign::C, 2),
            "baritone" | "C5" => (Sign::C, 4),
            "varbaritone" | "F3" => (Sign::F, 0),
            "bass" | "F" | "F4" => (Sign::F, 2),
            "subbass" | "F5" => (Sign::F, 4),
            "tenorG" => return Some(Clef { sign: Sign::G, line: -2, octave: -1 + octave }),
            "percussion" | "perc" => (Sign::Percussion, 0),
            "tab" | "moderntab" => (Sign::Tab, 0),
            _ => return None,
        };
        if octave != 0 && matches!(sign, Sign::Percussion | Sign::Tab) {
            return None;
        }
        Some(Clef { sign, line, octave })
    }
}

impl Default for Clef {
    fn default() -> Self {
        Clef::TREBLE
    }
}

/// What an event is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Kind {
    /// A note or a chord.
    #[default]
    Note,
    /// A rest, `r` or `R`, or a spacer `s`.
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
    /// `\partial`: the first bar is `ticks` long.
    Partial,
    /// `\cadenzaOn`: the bar goes on, unmetred, until `\cadenzaOff`.
    CadenzaOn,
    /// `\cadenzaOff`.
    CadenzaOff,
    /// `\ottava #n`: the notes after it drawn `ottava` octaves down (or up),
    /// under a bracket.
    Ottava,
    /// `\mark`: a rehearsal mark, or a segno or a coda, over the bar line.
    Mark,
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
#[derive(Debug, Clone, PartialEq)]
pub struct Words {
    /// The words.
    pub text: String,
    /// -1 above, 1 below.
    pub dir: i64,
    /// What `\markup` makes of them: pieces in their own faces, and music
    /// glyphs. Empty for a plain string, which is `text` upright.
    pub runs: Vec<Run>,
}

impl Words {
    /// A plain string's words.
    pub fn plain(text: String, dir: i64) -> Self {
        Words { text, dir, runs: Vec::new() }
    }

    /// Its pieces: `runs`, or the string upright.
    pub fn pieces(&self) -> Vec<Run> {
        if self.runs.is_empty() {
            vec![Run::words(self.text.clone(), Weight::Regular)]
        } else {
            self.runs.clone()
        }
    }
}

/// A piece of `\markup`: words in one face, or a music glyph by its SMuFL
/// name (`\musicglyph`).
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    /// The words, or the glyph's name.
    pub text: String,
    /// The face.
    pub weight: Weight,
    /// `text` names a glyph.
    pub glyph: bool,
    /// Set small and raised: a chord name's `7`.
    pub sup: bool,
    /// A word space before it.
    pub gap: bool,
    /// A glyph's size against the staff's: an accidental in a chord's name
    /// is smaller than one on the staff.
    pub size: f64,
}

impl Run {
    /// Words in a face, a space before them.
    pub fn words(text: impl Into<String>, weight: Weight) -> Self {
        Run { text: text.into(), weight, glyph: false, sup: false, gap: true, size: 1.0 }
    }

    /// A music glyph by its SMuFL name.
    pub fn glyph(name: impl Into<String>) -> Self {
        Run { text: name.into(), weight: Weight::Regular, glyph: true, sup: false, gap: true, size: 1.0 }
    }
}

/// What a grace note is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraceKind {
    /// `\grace`.
    Plain,
    /// `\acciaccatura`: a slash through its stem, a slur to its note.
    Acciaccatura,
    /// `\appoggiatura`: a slur to its note.
    Appoggiatura,
    /// `\slashedGrace`: a slash, no slur.
    Slashed,
}

/// A fingering: its figures and where it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finger {
    /// `1` … `5`.
    pub text: String,
    /// -1 above, 1 below, 0 the engraver's choice.
    pub dir: i64,
    /// In a chord, the diatonic number of the head it is written on.
    pub head: Option<i64>,
    /// A string's number, `\\2`, drawn in a circle.
    pub circled: bool,
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
    /// A drum's head: "" a plain one, "X", "CircleX".
    pub style: &'static str,
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
    /// Where each of `pitches` is written: the character its name starts
    /// at, in the order the source lists them. What an editor rewrites
    /// (03 §1.4, protocol 10). Empty for a `q`, which writes none.
    pub pitch_at: Vec<usize>,
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
    /// A copy `\repeat percent` makes: played, and drawn as the percent
    /// sign of its bar. The copy's number.
    pub percent: Option<i64>,
    /// Fingerings, `-1` and `<c-1 e-3>`'s.
    pub fingers: Vec<Finger>,
    /// `\(`: a phrasing slur starts.
    pub phrase_on: bool,
    /// `\)`.
    pub phrase_off: bool,
    /// `\arpeggio`: the chord rolled, a wavy line before it.
    pub arpeggio: bool,
    /// `\glissando` to the next note.
    pub gliss: bool,
    /// `c4:16`: the note repeated in sixteenths, strokes across its stem.
    pub tremolo: i64,
    /// `\ottava`'s octaves where the note stands; on `Kind::Ottava`, the new
    /// ones.
    pub ottava: i64,
    /// The grace notes before it, `\grace` and its kin: notes of their own,
    /// taking no time, drawn small.
    pub graces: Vec<Ev>,
    /// On a grace note, which: `\grace`, `\acciaccatura` (slashed and
    /// slurred), `\appoggiatura` (slurred) or `\slashedGrace`.
    pub grace: Option<GraceKind>,
    /// A `\chordmode` chord's name, as `ChordNames` prints it; "N.C." for a
    /// rest there.
    pub chord_name: Vec<Run>,
    /// A `\drummode` note's strokes.
    pub drums: Vec<Drum>,
    /// The strings a note's heads are played on: `(diatonic number, string)`,
    /// written with `\5`.
    pub strings: Vec<(i64, i64)>,
    /// On a tablature, each head's `(string, fret)`, the first string the
    /// highest.
    pub tab: Vec<(i64, i64)>,
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
    /// A rest of two voices: where it stands, in spaces from where one
    /// voice's rest would — moved clear of the other voice's notes.
    pub rest_y: Option<f64>,
    /// `s`: a rest that takes time and is not drawn.
    pub spacer: bool,
    /// How far a first voice's note stands right of its column, clear of
    /// the second's heads a step away.
    pub shift: Option<f64>,
    /// Where its arpeggio's wavy line starts, from its x.
    pub arp_x: Option<f64>,
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

/// What a staff is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StaffKind {
    /// Five lines and notes.
    #[default]
    Notes,
    /// `\new ChordNames`: the chords' names over the staff below, no lines.
    ChordNames,
    /// `\new DrumStaff`: a drum kit on five lines.
    Drums,
    /// `\new TabStaff`: a guitar's six strings, and frets on them.
    Tab,
}

/// How a note's words are read: `\chordmode`, `\drummode`, or as notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Notes,
    Chords,
    Drums,
}

/// A drum's stroke: its place on the staff, its General MIDI key, its head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drum {
    /// The head's diatonic number on a percussion staff.
    pub d: i64,
    /// Its General MIDI percussion key.
    pub key: i64,
    /// Its head: "" a plain one, "X" a cross, "CircleX" a cross in a circle.
    pub head: &'static str,
}

/// LilyPond's `drums-style`, by `\drummode`'s short names and its long
/// ones: staff position, MIDI key, head, and the sign over it.
const DRUMS: [(&str, &str, i64, i64, &str, &str); 30] = [
    ("bd", "bassdrum", -3, 36, "", ""),
    ("bda", "acousticbassdrum", -3, 35, "", ""),
    ("sn", "snare", 1, 38, "", ""),
    ("sna", "acousticsnare", 1, 38, "", ""),
    ("sne", "electricsnare", 1, 40, "", ""),
    ("ss", "sidestick", 1, 37, "X", ""),
    ("hc", "handclap", 1, 39, "", ""),
    ("hh", "hihat", 3, 42, "X", ""),
    ("hhc", "closedhihat", 3, 42, "X", "stopped"),
    ("hho", "openhihat", 3, 46, "X", "open"),
    ("hhho", "halfopenhihat", 3, 46, "X", ""),
    ("hhp", "pedalhihat", -5, 44, "X", ""),
    ("cymc", "crashcymbal", 5, 49, "CircleX", ""),
    ("cymca", "crashcymbala", 5, 49, "CircleX", ""),
    ("cymcb", "crashcymbalb", 5, 57, "X", ""),
    ("cymr", "ridecymbal", 5, 51, "X", ""),
    ("cymra", "ridecymbala", 5, 51, "X", ""),
    ("cymrb", "ridecymbalb", 5, 59, "X", ""),
    ("rb", "ridebell", 5, 53, "", ""),
    ("cymch", "chinesecymbal", 5, 52, "", ""),
    ("cyms", "splashcymbal", 5, 55, "", ""),
    ("cb", "cowbell", 5, 56, "", ""),
    ("vibs", "vibraslap", 4, 58, "", ""),
    ("tomh", "hightom", 4, 50, "", ""),
    ("tommh", "himidtom", 2, 48, "", ""),
    ("tomml", "lowmidtom", 0, 47, "", ""),
    ("toml", "lowtom", -1, 45, "", ""),
    ("tomfh", "highfloortom", -2, 43, "", ""),
    ("tomfl", "lowfloortom", -4, 41, "", ""),
    ("tamb", "tambourine", 5, 54, "", ""),
];

/// A drum by its name: its stroke, and the sign over it.
fn drum(name: &str) -> Option<(Drum, &'static str)> {
    let &(_, _, pos, key, head, script) = DRUMS.iter().find(|d| d.0 == name || d.1 == name)?;
    Some((Drum { d: pos + 7, key, head }, script))
}

/// A guitar's strings in standard tuning, the first the highest: e' b g d
/// a e, as MIDI keys.
pub const GUITAR: [i64; 6] = [64, 59, 55, 50, 45, 40];

/// One staff of the source.
#[derive(Debug, Clone, Default)]
pub struct Staff {
    /// What it is.
    pub kind: StaffKind,
    /// Its events.
    pub events: Vec<Ev>,
    /// Its settings.
    pub props: Props,
    /// Its `\addlyrics` or `\lyricsto`, a syllable a note.
    pub lyrics: Option<Vec<Syllable>>,
    /// The voice they are sung to: 1, or 2 for the second of two.
    pub lyrics_voice: u8,
    /// Its voices' names, `\new Voice = "name"`, and their numbers.
    pub voices: Vec<(String, u8)>,
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

/// Lyrics sung to a voice by its name, attached once every voice is read.
struct Pending {
    voice: String,
    syllables: Vec<Syllable>,
    at: usize,
}

struct Base {
    header: HashMap<String, String>,
    counter: usize,
    notes: usize,
    lang: Lang,
    pending: Vec<Pending>,
}

struct St {
    reference: Option<i64>,
    /// `\fixed`'s octave, in diatonic steps: the octave written pitches are
    /// in.
    fixed: Option<i64>,
    /// `\transpose`'s interval: diatonic steps and semitones.
    transpose: (i64, i64),
    lang: Lang,
    base: i64,
    dots: i64,
    events: Vec<Ev>,
    notes: usize,
    items: usize,
    props: Props,
    alts: i64,
    tuplets: i64,
    tuplet: Option<TupletState>,
    /// The pitches of the last chord, which `q` repeats.
    chord: Option<Vec<Pitch>>,
    /// `\voiceOne` … `\voiceFour` in the music being read: 1 or 2.
    voice_cmd: Option<u8>,
    /// `\new Voice = "name"` around the music being read.
    voice_name: Option<String>,
    /// The voices of the staff, named.
    voice_names: Vec<(String, u8)>,
    /// Inside a part of `<< >>`, whose voice is numbered when it is closed.
    in_part: bool,
    /// The copies `\repeat percent` has made.
    percents: i64,
    /// The rehearsal marks `\mark \default` has numbered.
    marks: i64,
    /// Grace notes read, waiting for the note they go before.
    graces: Vec<Ev>,
    /// How words are read.
    mode: Mode,
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

// ---- Variables ------------------------------------------------------------
//
// `melody = \relative c'' { … }` at the top of a file, and `\melody` where
// the music goes: LilyPond's own way of writing a score, and the way nearly
// every file found in the wild is written. A definition is written out where
// it is used, token for token, before anything is read; each token keeps the
// place it was written, so an error names the definition and an edit
// rewrites it.

/// The source's tokens with its definitions taken out and every use of one
/// written out in its place.
fn expand(toks: Vec<Tok>) -> Result<Vec<Tok>, ScoreError> {
    let mut defs: HashMap<String, Vec<Tok>> = HashMap::new();
    let mut out: Vec<Tok> = Vec::with_capacity(toks.len());
    let mut depth = 0i64;
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        if depth == 0 && t.t == T::Word && is(&toks, i + 1, T::Sym('=')) && !definition_blocked(&toks, i) {
            beyond();
            let end = expr_end(&toks, i + 2)?;
            let mut value = Vec::with_capacity(end - i - 2);
            put(&toks[i + 2..end], &defs, &mut value)?;
            defs.insert(t.v.clone(), value);
            i = end;
            continue;
        }
        match t.t {
            T::Sym('{') => depth += 1,
            T::Sym('}') => depth -= 1,
            _ => {}
        }
        put(std::slice::from_ref(t), &defs, &mut out)?;
        i += 1;
    }
    Ok(out)
}

/// A word and `=` that do not define anything: `\new Staff = "up"`, and
/// `Staff.instrumentName =` after `\set`.
fn definition_blocked(toks: &[Tok], i: usize) -> bool {
    if i == 0 {
        return false;
    }
    let before = &toks[i - 1];
    (before.t == T::Cmd && (before.v == "new" || before.v == "context")) || before.t == T::Sym('.')
}

/// `from` onto `out`, each use of a definition written out.
fn put(from: &[Tok], defs: &HashMap<String, Vec<Tok>>, out: &mut Vec<Tok>) -> Result<(), ScoreError> {
    for t in from {
        match defs.get(&t.v).filter(|_| t.t == T::Cmd) {
            Some(value) => out.extend(value.iter().cloned()),
            None => out.push(t.clone()),
        }
        if out.len() > MAX_TOKENS {
            return refuse("the variables write out to more music than this engraves", t.at);
        }
    }
    Ok(())
}

/// Where the expression that starts at `i` ends: a variable's value.
fn expr_end(toks: &[Tok], i: usize) -> Result<usize, ScoreError> {
    let Some(t) = toks.get(i) else {
        return refuse("a variable with no value", at_of(toks, i.saturating_sub(1)));
    };
    match t.t {
        T::Sym('{') => balanced(toks, i, '{', '}'),
        T::Sym('<') if twin(toks, i, '<') => twin_end(toks, i),
        T::Str | T::Num | T::Lyrics => Ok(i + 1),
        T::Sym('#') => scheme_end(toks, i),
        T::Cmd => match t.v.as_str() {
            "relative" => {
                let j = if is(toks, i + 1, T::Word) && starts_expr(toks, i + 2) { i + 2 } else { i + 1 };
                expr_end(toks, j)
            }
            "fixed" => expr_end(toks, i + 2),
            "transpose" => expr_end(toks, i + 3),
            "new" | "context" => {
                let mut j = i + 2;
                if is(toks, j, T::Sym('=')) {
                    j += 2;
                }
                if is_cmd(toks, j, "with") {
                    j = balanced(toks, j + 1, '{', '}')?;
                }
                expr_end(toks, j)
            }
            "chordmode" | "drummode" | "notemode" | "lyricmode" | "chords" | "drums" | "lyrics" | "sequential" | "simultaneous" => expr_end(toks, i + 1),
            "lyricsto" => expr_end(toks, i + 2),
            "repeat" => {
                let j = expr_end(toks, i + 3)?;
                if is_cmd(toks, j, "alternative") {
                    return balanced(toks, j + 1, '{', '}');
                }
                Ok(j)
            }
            "markup" => {
                if is(toks, i + 1, T::Sym('{')) {
                    balanced(toks, i + 1, '{', '}')
                } else {
                    Ok(i + 2)
                }
            }
            _ => Ok(i + 1),
        },
        _ => refuse("a variable's value is music in { }, << >>, \\relative …, a string or a number", t.at),
    }
}

/// Whether one music expression that holds the rest starts at `i`: a block,
/// `<< >>`, or a command around music — where a bare `\clef` starts a
/// sequence.
fn wraps(toks: &[Tok], i: usize) -> bool {
    const AROUND: [&str; 12] = ["relative", "fixed", "transpose", "new", "context", "sequential", "simultaneous", "chordmode", "drummode", "chords", "drums", "notemode"];
    match toks.get(i) {
        Some(t) => t.t == T::Sym('{') || twin(toks, i, '<') || (t.t == T::Cmd && AROUND.contains(&t.v.as_str())),
        None => false,
    }
}

/// Whether a music expression starts at `i`.
fn starts_expr(toks: &[Tok], i: usize) -> bool {
    match toks.get(i) {
        Some(t) => t.t == T::Sym('{') || twin(toks, i, '<') || t.t == T::Cmd,
        None => false,
    }
}

/// The token after the one that closes the `open` at `i`.
fn balanced(toks: &[Tok], i: usize, open: char, close: char) -> Result<usize, ScoreError> {
    if !is(toks, i, T::Sym(open)) {
        return refuse(format!("a {open} block {close} was expected"), at_of(toks, i));
    }
    let mut depth = 0;
    let mut j = i;
    while j < toks.len() {
        if toks[j].t == T::Sym(open) {
            depth += 1;
        } else if toks[j].t == T::Sym(close) {
            depth -= 1;
            if depth == 0 {
                return Ok(j + 1);
            }
        }
        j += 1;
    }
    refuse(format!("a '{open}' that is never closed"), toks[i].at)
}

/// The token after the `>>` that closes the `<<` at `i`.
fn twin_end(toks: &[Tok], i: usize) -> Result<usize, ScoreError> {
    let mut depth = 0;
    let mut j = i;
    while j < toks.len() {
        if twin(toks, j, '<') {
            depth += 1;
            j += 2;
            continue;
        }
        if twin(toks, j, '>') {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return Ok(j);
            }
            continue;
        }
        j += 1;
    }
    refuse("a << that is never closed by >>", toks[i].at)
}

/// The token after a Scheme value: `#(…)`, `##t`, `#'name`, `#"…"`, `#-1`.
fn scheme_end(toks: &[Tok], i: usize) -> Result<usize, ScoreError> {
    let mut j = i + 1;
    while is(toks, j, T::Sym('#')) || is(toks, j, T::Sym('\'')) || is(toks, j, T::Sym('-')) {
        j += 1;
    }
    if is(toks, j, T::Sym('(')) {
        return balanced(toks, j, '(', ')');
    }
    if toks.get(j).is_none() {
        return refuse("a # with nothing after it", toks[i].at);
    }
    j += 1;
    // `#0.5`
    if is(toks, j, T::Sym('.')) && is(toks, j + 1, T::Num) {
        j += 2;
    }
    Ok(j)
}

// ---- The file ---------------------------------------------------------------

/// Read a source.
pub fn parse(src: &str) -> Result<Parsed, ScoreError> {
    let toks = expand(tokens(src)?)?;
    scan_beyond(&toks);
    let mut st = Base { header: HashMap::new(), counter: 0, notes: 0, lang: Lang::default(), pending: Vec::new() };
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
    let after = i;
    i = preamble(&toks, i, &mut st)?;
    if i != after {
        beyond();
    }
    if i < toks.len() {
        return refuse(format!("'{}' after the end of the music", toks[i].v), toks[i].at);
    }
    // A spacer anywhere but a second voice is protocol 11's.
    for stf in &staves {
        let mut two = false;
        for ev in &stf.events {
            match ev.k {
                Kind::VoiceNext => two = true,
                Kind::VoicesEnd => two = false,
                Kind::Rest if ev.spacer && !two => beyond(),
                _ => {}
            }
        }
    }
    for p in st.pending {
        let found = staves.iter().position(|s| s.voices.iter().any(|v| v.0 == p.voice));
        let Some(si) = found else {
            return refuse(format!("\\lyricsto \"{}\" names no voice: a \\new Voice = \"{}\" is what it sings to", p.voice, p.voice), p.at);
        };
        let staff = &mut staves[si];
        if staff.lyrics.is_some() {
            return refuse("one verse of lyrics a staff is what this sets", p.at);
        }
        staff.lyrics_voice = staff.voices.iter().find(|v| v.0 == p.voice).map_or(1, |v| v.1);
        staff.lyrics = Some(p.syllables);
    }
    Ok(Parsed { header: st.header, staves, groups })
}

/// What a file says before and after its music: `\version`, `\header`,
/// `\language`, `\paper`, `\layout` and `\midi` blocks, Scheme, and the
/// switches nothing drawn depends on.
fn preamble(toks: &[Tok], mut i: usize, st: &mut Base) -> Result<usize, ScoreError> {
    while i < toks.len() {
        let t = &toks[i];
        if t.t == T::Sym('#') {
            beyond();
            i = scheme_end(toks, i)?;
            continue;
        }
        if t.t != T::Cmd {
            break;
        }
        match t.v.as_str() {
            "version" => i += 2,
            "header" => i = header(toks, i, st)?,
            "paper" | "layout" | "midi" => {
                beyond();
                i = skip_block(toks, i + 1)?;
            }
            "pointAndClickOff" | "pointAndClickOn" => i += 1,
            "language" => {
                let Some(name) = toks.get(i + 1).filter(|n| n.t == T::Str) else { return refuse("\\language wants a language's name, in quotes", t.at) };
                st.lang = Lang::named(&name.v).ok_or_else(|| ScoreError {
                    message: format!("score: the language \"{}\" is not one this reads (nederlands, english, deutsch, italiano, français, español) (at character {})", name.v, name.at),
                })?;
                i += 2;
            }
            "include" => {
                let Some(name) = toks.get(i + 1).filter(|n| n.t == T::Str) else { return refuse("\\include wants a file's name, in quotes", t.at) };
                let Some(lang) = name.v.strip_suffix(".ly").and_then(Lang::named) else {
                    return refuse(format!("\\include \"{}\": a score reads one source, and includes none", name.v), t.at);
                };
                st.lang = lang;
                i += 2;
            }
            _ => break,
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

/// A context that is a staff of its own, and what kind.
fn staff_kind(kind: &str) -> Option<StaffKind> {
    Some(match kind {
        "Staff" => StaffKind::Notes,
        "ChordNames" => StaffKind::ChordNames,
        "DrumStaff" => StaffKind::Drums,
        "TabStaff" => StaffKind::Tab,
        _ => return None,
    })
}

/// A context a staff's music is sung in.
fn voice_kind(kind: &str) -> bool {
    matches!(kind, "Voice" | "DrumVoice" | "TabVoice")
}

/// Whether the `<<` at `i` holds staves, rather than the voices of one.
fn holds_staves(toks: &[Tok], i: usize) -> bool {
    let Ok(end) = twin_end(toks, i) else { return false };
    let mut depth = 0;
    let mut j = i + 2;
    while j + 1 < end {
        if twin(toks, j, '<') {
            depth += 1;
            j += 2;
            continue;
        }
        if twin(toks, j, '>') {
            depth -= 1;
            j += 2;
            continue;
        }
        match toks[j].t {
            T::Sym('{') => depth += 1,
            T::Sym('}') => depth -= 1,
            T::Cmd if depth == 0 && (toks[j].v == "new" || toks[j].v == "context") => {
                let kind = toks.get(j + 1).map_or("", |k| k.v.as_str());
                if staff_kind(kind).is_some() || group_kind(kind).is_some() || kind == "Lyrics" {
                    return true;
                }
            }
            T::Cmd if depth == 0 && (toks[j].v == "chords" || toks[j].v == "drums") => {
                return true;
            }
            _ => {}
        }
        j += 1;
    }
    false
}

fn part(toks: &[Tok], mut i: usize, st: &mut Base, staves: &mut Vec<Staff>, groups: &mut Vec<Group>, depth: usize) -> Result<usize, ScoreError> {
    if i >= toks.len() {
        return refuse("there is no music", 0);
    }
    let tk = toks[i].clone();
    if tk.t == T::Cmd && (tk.v == "chords" || tk.v == "drums") {
        // `\chords { … }` is `\new ChordNames \chordmode { … }`, `\drums`
        // the same of a drum staff.
        let kind = if tk.v == "chords" { StaffKind::ChordNames } else { StaffKind::Drums };
        return staff(toks, i + 1, st, staves, Props::default(), kind);
    }
    if tk.t == T::Cmd && (tk.v == "new" || tk.v == "context") {
        let kind = toks.get(i + 1).map(|t| t.v.clone()).unwrap_or_default();
        if voice_kind(&kind) {
            return staff(toks, i, st, staves, Props::default(), StaffKind::Notes);
        }
        if staff_kind(&kind).is_none() && group_kind(&kind).is_none() {
            return refuse(format!("\\new {kind} is not drawn: a Staff, a ChordNames, a DrumStaff, a TabStaff, a PianoStaff, a GrandStaff, a StaffGroup or a ChoirStaff is"), tk.at);
        }
        i += 2;
        if i + 1 < toks.len() && toks[i].t == T::Sym('=') && toks[i + 1].t == T::Str {
            i += 2;
        }
        let mut props = Props::default();
        if is_cmd(toks, i, "with") {
            i = with(toks, i, &mut props)?;
        }
        if let Some(sk) = staff_kind(&kind) {
            return staff(toks, i, st, staves, props, sk);
        }
        if !double(toks, i, '<') {
            return refuse(format!("\\new {kind} wants its staves between << and >>"), tk.at);
        }
        let gk = group_kind(&kind).unwrap_or(GroupKind::None);
        return group(toks, i, st, staves, groups, depth, gk, &props);
    }
    if double(toks, i, '<') && holds_staves(toks, i) {
        return group(toks, i, st, staves, groups, depth, GroupKind::None, &Props::default());
    }
    if double(toks, i, '<') {
        beyond();
    }
    staff(toks, i, st, staves, Props::default(), StaffKind::Notes)
}

#[allow(clippy::too_many_arguments)]
fn group(toks: &[Tok], mut i: usize, st: &mut Base, staves: &mut Vec<Staff>, groups: &mut Vec<Group>, depth: usize, kind: GroupKind, props: &Props) -> Result<usize, ScoreError> {
    let at = toks[i].at;
    i += 2;
    let first = staves.len();
    let mut bare = false;
    while i < toks.len() && !double(toks, i, '>') {
        let ctx = toks[i].t == T::Cmd && (toks[i].v == "new" || toks[i].v == "context");
        let kind_here = if ctx { toks.get(i + 1).map_or("", |k| k.v.as_str()) } else { "" };
        if kind_here == "Lyrics" {
            i = lyrics_part(toks, i, st, staves)?;
            continue;
        }
        // A voice, or music, with no staff of its own: LilyPond makes it one.
        let shorthand = toks[i].t == T::Cmd && (toks[i].v == "chords" || toks[i].v == "drums");
        let voice = !shorthand && (!ctx || voice_kind(kind_here));
        if voice {
            if bare || !wraps(toks, i) {
                return refuse("two voices in one staff are not drawn: give each its own \\new Staff", toks[i].at);
            }
            bare = true;
            beyond();
            i = staff(toks, i, st, staves, Props::default(), StaffKind::Notes)?;
            i = attach_lyrics(toks, i, staves)?;
            continue;
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

/// `\new Lyrics [= "name"] [\with { … }] \lyricsto "voice" { … }`, or with
/// `\lyricmode { … }`: the words sung to the voice named, or to the staff
/// before them.
fn lyrics_part(toks: &[Tok], mut i: usize, st: &mut Base, staves: &mut [Staff]) -> Result<usize, ScoreError> {
    let at = toks[i].at;
    i += 2;
    if is(toks, i, T::Sym('=')) && is(toks, i + 1, T::Str) {
        i += 2;
    }
    if is_cmd(toks, i, "with") {
        i = skip_block(toks, i + 1)?;
    }
    let braced = is(toks, i, T::Sym('{'));
    if braced {
        i += 1;
    }
    let mut voice = None;
    if is_cmd(toks, i, "lyricsto") {
        voice = toks.get(i + 1).map(|t| t.v.clone());
        i += 2;
    }
    if is_cmd(toks, i, "lyricmode") || is_cmd(toks, i, "lyrics") {
        i += 1;
    }
    if !is(toks, i, T::Lyrics) {
        return refuse("\\new Lyrics wants \\lyricsto \"voice\" { words } or \\lyricmode { words }", at);
    }
    let syllables = syllables(&toks[i].lyrics, at)?;
    i += 1;
    if braced {
        if !is(toks, i, T::Sym('}')) {
            return refuse("a '{' that is never closed", at);
        }
        i += 1;
    }
    match voice {
        Some(v) => st.pending.push(Pending { voice: v, syllables, at }),
        None => {
            let Some(last) = staves.last_mut() else { return refuse("\\new Lyrics follows the staff it is sung to", at) };
            if last.lyrics.is_some() {
                return refuse("one verse of lyrics a staff is what this sets", at);
            }
            last.lyrics = Some(syllables);
            last.lyrics_voice = 1;
        }
    }
    Ok(i)
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

fn staff(toks: &[Tok], mut i: usize, base: &mut Base, staves: &mut Vec<Staff>, props: Props, kind: StaffKind) -> Result<usize, ScoreError> {
    let mut st = St {
        reference: None,
        fixed: None,
        transpose: (0, 0),
        lang: base.lang,
        base: 4,
        dots: 0,
        events: Vec::new(),
        notes: base.notes,
        items: base.counter,
        props,
        alts: 0,
        tuplets: 0,
        tuplet: None,
        chord: None,
        voice_cmd: None,
        voice_name: None,
        voice_names: Vec::new(),
        in_part: false,
        percents: 0,
        marks: 0,
        graces: Vec::new(),
        mode: match kind {
            StaffKind::ChordNames => Mode::Chords,
            StaffKind::Drums => Mode::Drums,
            _ => Mode::Notes,
        },
    };
    match kind {
        StaffKind::Drums => st.events.push(Ev { k: Kind::Clef, clef: Clef { sign: Sign::Percussion, line: 0, octave: 0 }, at: toks.get(i).map_or(0, |t| t.at), ..Ev::default() }),
        StaffKind::Tab => st.events.push(Ev { k: Kind::Clef, clef: Clef { sign: Sign::Tab, line: 0, octave: 0 }, at: toks.get(i).map_or(0, |t| t.at), ..Ev::default() }),
        _ => {}
    }
    if wraps(toks, i) {
        i = expr(toks, i, &mut st, &mut base.header)?;
    } else {
        i = music(toks, i, &mut st, &mut base.header)?;
    }
    if let Some(g) = st.graces.first() {
        return refuse("a grace note goes before a note, and none follows it", g.at);
    }
    base.counter = st.items;
    base.notes = st.notes;
    staves.push(Staff { kind, events: st.events, props: st.props, lyrics: None, lyrics_voice: 1, voices: st.voice_names });
    Ok(i)
}

fn header(toks: &[Tok], mut i: usize, st: &mut Base) -> Result<usize, ScoreError> {
    let fields = ["dedication", "title", "subtitle", "subsubtitle", "poet", "instrument", "composer", "meter", "arranger", "piece", "opus", "copyright"];
    // What a header may say that a title page does not print: kept quiet.
    let silent = [
        "tagline",
        "enteredby",
        "maintainer",
        "maintainerEmail",
        "maintainerWeb",
        "source",
        "style",
        "lastupdated",
        "date",
        "license",
        "mutopiatitle",
        "mutopiacomposer",
        "mutopiapoet",
        "mutopiaopus",
        "mutopiainstrument",
        "moreInfo",
        "footer",
    ];
    let at = toks[i].at;
    i += 1;
    if i >= toks.len() || toks[i].t != T::Sym('{') {
        return refuse("\\header wants a { block }", at);
    }
    i += 1;
    while i < toks.len() && toks[i].t != T::Sym('}') {
        let name = toks[i].clone();
        if name.t == T::Word && silent.contains(&name.v.as_str()) && is(toks, i + 1, T::Sym('=')) {
            beyond();
            i = expr_end(toks, i + 2)?;
            continue;
        }
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

// ---- Music ------------------------------------------------------------------

/// One music expression: `{ … }`, `<< … >>`, a command and what it takes,
/// or a note.
fn expr(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let Some(t) = toks.get(i) else { return refuse("there is no music", 0) };
    match t.t {
        T::Sym('{') => {
            let j = music(toks, i + 1, st, hd)?;
            if j >= toks.len() || toks[j].t != T::Sym('}') {
                return refuse("a '{' that is never closed", 0);
            }
            Ok(j + 1)
        }
        T::Sym('<') if twin(toks, i, '<') => voices(toks, i, st, hd),
        T::Cmd => command(toks, i, st, hd),
        T::Word | T::Sym('<') => note(toks, i, st),
        _ => refuse(format!("'{}' is not expected here", t.v), t.at),
    }
}

fn music(toks: &[Tok], mut i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    while i < toks.len() {
        let tk = toks[i].clone();
        match tk.t {
            T::Sym('}') => break,
            T::Sym('{') => {
                beyond();
                i = expr(toks, i, st, hd)?;
            }
            T::Cmd => i = command(toks, i, st, hd)?,
            T::Sym('<') if twin(toks, i, '<') => i = voices(toks, i, st, hd)?,
            T::Word | T::Sym('<') => i = note(toks, i, st)?,
            T::Sym('|') => i += 1,
            T::Sym('~') => {
                last_note(&mut st.events, &tk)?.tie = true;
                i += 1;
            }
            T::Sym('-' | '^' | '_') => i = postfix(toks, i, st, hd)?,
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

/// A part of `<< >>`: its events, the voice `\voiceOne` or `\voiceTwo` gave
/// it, and its name.
struct Part {
    events: Vec<Ev>,
    voice: Option<u8>,
    name: Option<String>,
    at: usize,
}

/// Whether a part sings — has notes or rests, where a `\global` of the
/// staff's settings and spacers has none.
fn sings(events: &[Ev]) -> bool {
    events.iter().any(|ev| ev.k == Kind::Note || (ev.k == Kind::Rest && !ev.spacer))
}

/// What belongs to the staff rather than to a voice: the staff's settings
/// and the marks between its bars.
fn staff_level(k: &Kind) -> bool {
    matches!(
        k,
        Kind::Clef
            | Kind::Key
            | Kind::Time
            | Kind::Bar
            | Kind::Tempo
            | Kind::Break
            | Kind::RepeatStart
            | Kind::RepeatEnd
            | Kind::VoltaStart
            | Kind::VoltaEnd
            | Kind::Compress
            | Kind::Partial
            | Kind::CadenzaOn
            | Kind::CadenzaOff
            | Kind::Ottava
            | Kind::Mark
    )
}

/// The ticks an event of one voice takes.
fn span(ev: &Ev) -> i64 {
    match ev.k {
        Kind::Note | Kind::Rest => ev.ticks * ev.count.max(1),
        _ => 0,
    }
}

/// `main` with the staff's settings of `other` set in it at the moments
/// they are written — a `\global`'s `\key` and `\time`, a second voice's
/// `\clef` — and `other` without them.
fn merge(main: Vec<Ev>, other: Vec<Ev>) -> (Vec<Ev>, Vec<Ev>) {
    let mut put: Vec<(i64, Ev)> = Vec::new();
    let mut kept = Vec::new();
    let mut t = 0;
    let mut two = false;
    for ev in other {
        match ev.k {
            Kind::VoiceNext => two = true,
            Kind::VoicesEnd => two = false,
            _ => {}
        }
        if staff_level(&ev.k) {
            put.push((t, ev));
            continue;
        }
        if !two {
            t += span(&ev);
        }
        kept.push(ev);
    }
    if put.is_empty() {
        return (main, kept);
    }
    let mut out = Vec::with_capacity(main.len() + put.len());
    let mut t = 0;
    let mut k = 0;
    let mut two = false;
    for ev in main {
        match ev.k {
            Kind::VoiceNext => two = true,
            Kind::VoicesEnd => two = false,
            _ => {}
        }
        if !two && span(&ev) > 0 {
            while k < put.len() && put[k].0 <= t {
                out.push(put[k].1.clone());
                k += 1;
            }
            t += span(&ev);
        }
        out.push(ev);
    }
    out.extend(put.into_iter().skip(k).map(|p| p.1));
    (out, kept)
}

/// Simultaneous music on one staff, `<< … >>`: two voices written apart
/// with `\\`, or side by side — `{ \voiceOne … } \new Voice { \voiceTwo … }`
/// — under `\relative` each from the last pitch of the part before it, and
/// what follows from the last part's, as LilyPond reads them
/// (`music_list_to_relative`). A part with no notes, a `\global` of the
/// staff's settings, is set in the first voice where its settings fall.
fn voices(toks: &[Tok], mut i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let open_at = toks[i].at;
    i += 2;
    // Under `\relative` the voices are read as LilyPond reads them, which
    // the Soli engraver did not.
    if st.reference.is_some() {
        beyond();
    }
    let outer = std::mem::take(&mut st.events);
    let outer_cmd = st.voice_cmd;
    let outer_part = st.in_part;
    let mut parts: Vec<Part> = Vec::new();
    let mut apart = 0;
    loop {
        if twin(toks, i, '>') {
            i += 2;
            break;
        }
        if i >= toks.len() || toks[i].t == T::Sym('}') {
            return refuse("<< { … } \\\\ { … } >> is not closed with >>", open_at);
        }
        if is(toks, i, T::Sym('\\')) {
            apart += 1;
            if apart == 2 {
                return refuse("two voices on a staff are what this draws", toks[i].at);
            }
            i += 1;
            continue;
        }
        if apart > 0 && !(is(toks, i, T::Sym('{')) || toks[i].t == T::Cmd) {
            return refuse("<< wants { music } \\\\ { music } >>", open_at);
        }
        if !is(toks, i, T::Sym('{')) || parts.len() != apart {
            beyond();
        }
        st.voice_cmd = None;
        st.voice_name = None;
        st.in_part = true;
        let at = toks[i].at;
        i = expr(toks, i, st, hd)?;
        if let Some(g) = st.graces.first() {
            return refuse("a grace note goes before a note, and none follows it in its voice", g.at);
        }
        parts.push(Part { events: std::mem::take(&mut st.events), voice: st.voice_cmd, name: st.voice_name.take(), at });
    }
    st.events = outer;
    st.voice_cmd = outer_cmd;
    st.in_part = outer_part;
    let singing: Vec<usize> = (0..parts.len()).filter(|&k| sings(&parts[k].events)).collect();
    if parts.len() != 2 || apart != 1 || parts.iter().skip(1).any(|p| p.events.iter().any(|ev| staff_level(&ev.k))) {
        beyond();
    }
    if singing.len() > 2 {
        return refuse("two voices on a staff are what this draws", parts[singing[2]].at);
    }
    if singing.len() < 2 {
        // One voice, and perhaps the staff's settings beside it.
        let main_k = singing.first().copied().unwrap_or(0);
        let mut main = std::mem::take(&mut parts[main_k].events);
        for (k, p) in parts.iter_mut().enumerate() {
            if k != main_k {
                main = merge(main, std::mem::take(&mut p.events)).0;
            }
        }
        if let Some(name) = parts[main_k].name.take() {
            st.voice_names.push((name, 1));
        }
        st.events.extend(main);
        return Ok(i);
    }
    let (a, b) = (singing[0], singing[1]);
    let (va, _) = match (parts[a].voice, parts[b].voice) {
        (Some(x), Some(y)) if x == y => return refuse("two voices on a staff are what this draws: one \\voiceOne, one \\voiceTwo", parts[b].at),
        (Some(x), _) => (x, 3 - x),
        (None, Some(y)) => (3 - y, y),
        (None, None) => (1, 2),
    };
    let (one, two) = if va == 1 { (a, b) } else { (b, a) };
    if parts[one].events.iter().chain(&parts[two].events).any(|ev| ev.k == Kind::Voices) {
        return refuse("two voices on a staff are what this draws", parts[b].at);
    }
    let mut first = std::mem::take(&mut parts[one].events);
    let mut second = std::mem::take(&mut parts[two].events);
    (first, second) = merge(first, second);
    for (k, p) in parts.iter_mut().enumerate() {
        if k != one && k != two {
            first = merge(first, std::mem::take(&mut p.events)).0;
        }
    }
    for (events, v) in [(&mut first, 1u8), (&mut second, 2u8)] {
        for ev in events.iter_mut() {
            if ev.k == Kind::Note || ev.k == Kind::Rest {
                if ev.count > 1 {
                    return refuse("a run of bars' rest is the staff's: write it outside << \\\\ >>", ev.at);
                }
                ev.voice = Some(v);
            }
        }
    }
    for (k, v) in [(one, 1u8), (two, 2u8)] {
        if let Some(name) = parts[k].name.take() {
            st.voice_names.push((name, v));
        }
    }
    st.events.push(Ev { k: Kind::Voices, at: open_at, ..Ev::default() });
    st.events.extend(first);
    st.events.push(Ev { k: Kind::VoiceNext, at: parts[two].at, ..Ev::default() });
    st.events.extend(second);
    st.events.push(Ev { k: Kind::VoicesEnd, at: open_at, ..Ev::default() });
    Ok(i)
}

fn postfix(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
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
        last.texts.push(Words::plain(nxt.v, if dir == 0 { 1 } else { dir }));
        return Ok(i + 2);
    }
    if nxt.t == T::Num && !nxt.v.contains('/') {
        beyond();
        let last = last_note(&mut st.events, &tk)?;
        last.fingers.push(Finger { text: nxt.v, dir, head: None, circled: false });
        return Ok(i + 2);
    }
    if nxt.t == T::Cmd && nxt.v == "markup" {
        let (runs, j) = markup(toks, i + 2, nxt.at)?;
        let text = runs.iter().filter(|r| !r.glyph).map(|r| r.text.as_str()).collect::<Vec<_>>().join(" ");
        let last = last_item(&mut st.events, &tk)?;
        last.texts.push(Words { text, dir: if dir == 0 { -1 } else { dir }, runs });
        return Ok(j);
    }
    if nxt.t == T::Cmd {
        if SCRIPTS.contains(&nxt.v.as_str()) {
            let last = last_note(&mut st.events, &tk)?;
            last.scripts.push(Script { name: nxt.v, dir });
            return Ok(i + 2);
        }
        return command(toks, i + 1, st, hd);
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

/// A pitch written as a word, in the language and the octave mode in force:
/// its diatonic number before `\transpose`, and its alteration.
fn written(st: &St, tk: &Tok, reference: Option<i64>) -> Option<(i64, i64)> {
    let (step, alter) = pitch_name_in(st.lang, &tk.v)?;
    let d = match (reference, st.fixed) {
        (Some(_), _) => resolve(step, tk.marks, reference),
        (None, Some(octave)) => step + octave + 7 * tk.marks,
        (None, None) => step + 7 * tk.marks,
    };
    Some((d, alter))
}

/// A pitch moved by `\transpose`'s interval, or why it cannot be.
fn transposed(st: &St, d: i64, alter: i64, at: usize) -> Result<Pitch, ScoreError> {
    let (dd, ds) = st.transpose;
    if dd == 0 && ds == 0 {
        return Ok(Pitch { d, alter });
    }
    let to = d + dd;
    let a = semitones(d, alter) + ds - semitones(to, 0);
    if !(-2..=2).contains(&a) {
        return refuse("\\transpose takes this note past a double sharp or a double flat", at);
    }
    Ok(Pitch { d: to, alter: a })
}

/// An absolute pitch argument — `\transpose c d`, `\fixed c'` — in the
/// language in force.
fn pitch_arg(st: &St, toks: &[Tok], i: usize, what: &str, at: usize) -> Result<(i64, i64), ScoreError> {
    match toks.get(i).filter(|t| t.t == T::Word).and_then(|t| pitch_name_in(st.lang, &t.v).map(|p| (p, t.marks))) {
        Some(((step, alter), marks)) => Ok((step + 7 * marks, alter)),
        None => refuse(format!("{what} wants a pitch"), at),
    }
}

fn command(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let word = tk.v.as_str();
    let arg = toks.get(i + 1).cloned();
    match word {
        "clef" => {
            let Some(arg) = arg else { return refuse("\\clef wants a name", tk.at) };
            if !["treble", "violin", "G", "bass", "F", "alto", "C"].contains(&arg.v.as_str()) {
                beyond();
            }
            let Some(clef) = Clef::named(&arg.v) else {
                return refuse(format!("the clef '{}' is not one this draws (treble, bass, alto, tenor, soprano, baritone, \"treble_8\" and their kin, percussion)", arg.v), arg.at);
            };
            st.events.push(Ev { k: Kind::Clef, clef, at: tk.at, ..Ev::default() });
            return Ok(i + 2);
        }
        "key" => {
            let mode = toks.get(i + 2).cloned();
            let name = arg.as_ref().and_then(|a| pitch_name_in(st.lang, &a.v));
            let (Some((step, alter)), Some(mode)) = (name, mode) else { return refuse("\\key wants a pitch and \\major or \\minor", tk.at) };
            if mode.t != T::Cmd {
                return refuse("\\key wants a pitch and \\major or \\minor", tk.at);
            }
            if mode.v != "major" && mode.v != "minor" {
                return refuse(format!("the mode '{}' is not \\major or \\minor", mode.v), mode.at);
            }
            let tonic = transposed(st, step, alter, tk.at)?;
            let mut fifths = [0, 2, 4, -1, 1, 3, 5][modulo(tonic.d, 7) as usize] + 7 * tonic.alter;
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
            if !["|", "||", "|.", ".|:", ":|.", ":|.|:", ""].contains(&arg.v.as_str()) {
                return refuse(format!("the bar line \"{}\" is not one this draws (|, ||, |., .|:, :|., :|.|:)", arg.v), arg.at);
            }
            if arg.v.is_empty() {
                beyond();
            }
            st.events.push(Ev { k: Kind::Bar, v: arg.v, at: tk.at, ..Ev::default() });
            return Ok(i + 2);
        }
        "tempo" => return tempo(toks, i, st),
        "relative" => {
            // The Soli engraver reads it at the start of a staff, with a pitch
            // and a block.
            let opening = st.events.is_empty() && st.reference.is_none() && is(toks, i + 1, T::Word) && is(toks, i + 2, T::Sym('{'));
            if !opening {
                beyond();
            }
            let (saved_ref, saved_fixed) = (st.reference, st.fixed);
            let mut j = i + 1;
            // `\relative c' { … }`, or `\relative { … }` from f, which reads
            // the first note's octave as written.
            let reference = match toks.get(j) {
                Some(p) if p.t == T::Word && starts_expr(toks, j + 1) => {
                    let Some((step, _)) = pitch_name_in(st.lang, &p.v) else { return refuse("\\relative wants a pitch", tk.at) };
                    j += 1;
                    resolve(step, p.marks, None)
                }
                Some(_) if starts_expr(toks, j) => 3,
                _ => return refuse("\\relative wants a pitch", tk.at),
            };
            st.reference = Some(reference);
            st.fixed = None;
            j = expr(toks, j, st, hd)?;
            st.reference = saved_ref.map(|_| st.reference.unwrap_or(reference));
            st.fixed = saved_fixed;
            return Ok(j);
        }
        "fixed" => {
            let (d, _) = pitch_arg(st, toks, i + 1, "\\fixed", tk.at)?;
            let (saved_ref, saved_fixed) = (st.reference, st.fixed);
            st.reference = None;
            st.fixed = Some(d - modulo(d, 7));
            let j = expr(toks, i + 2, st, hd)?;
            st.reference = saved_ref;
            st.fixed = saved_fixed;
            return Ok(j);
        }
        "transpose" => {
            let from = pitch_arg(st, toks, i + 1, "\\transpose", tk.at)?;
            let to = pitch_arg(st, toks, i + 2, "\\transpose", tk.at)?;
            let saved = st.transpose;
            st.transpose = (saved.0 + to.0 - from.0, saved.1 + semitones(to.0, to.1) - semitones(from.0, from.1));
            let j = expr(toks, i + 3, st, hd)?;
            st.transpose = saved;
            return Ok(j);
        }
        "new" | "context" => {
            let kind = arg.as_ref().map_or("", |a| a.v.as_str());
            if !voice_kind(kind) {
                return refuse(format!("\\new {kind} inside a staff's music is not drawn: a staff's music has voices"), tk.at);
            }
            let mut j = i + 2;
            let mut name = None;
            if is(toks, j, T::Sym('=')) && is(toks, j + 1, T::Str) {
                name = Some(toks[j + 1].v.clone());
                j += 2;
            }
            if is_cmd(toks, j, "with") {
                j = skip_block(toks, j + 1)?;
            }
            if !starts_expr(toks, j) {
                return refuse(format!("\\new {kind} wants its music"), tk.at);
            }
            j = expr(toks, j, st, hd)?;
            if let Some(name) = name {
                if st.in_part {
                    st.voice_name = Some(name);
                } else {
                    st.voice_names.push((name, 1));
                }
            }
            return Ok(j);
        }
        "voiceOne" | "voiceThree" => {
            st.voice_cmd = Some(1);
            return Ok(i + 1);
        }
        "voiceTwo" | "voiceFour" => {
            st.voice_cmd = Some(2);
            return Ok(i + 1);
        }
        "oneVoice" => return Ok(i + 1),
        "sequential" => return expr(toks, i + 1, st, hd),
        "chordmode" | "chords" | "drummode" | "drums" | "notemode" => {
            let saved = st.mode;
            st.mode = match word {
                "chordmode" | "chords" => Mode::Chords,
                "drummode" | "drums" => Mode::Drums,
                _ => Mode::Notes,
            };
            let j = expr(toks, i + 1, st, hd)?;
            st.mode = saved;
            return Ok(j);
        }
        "partial" => {
            let Some((ticks, j)) = duration_arg(toks, i + 1) else { return refuse("\\partial wants a duration, like 4 or 8.", tk.at) };
            st.events.push(Ev { k: Kind::Partial, ticks, at: tk.at, ..Ev::default() });
            return Ok(j);
        }
        "cadenzaOn" => {
            st.events.push(Ev { k: Kind::CadenzaOn, at: tk.at, ..Ev::default() });
            return Ok(i + 1);
        }
        "cadenzaOff" => {
            st.events.push(Ev { k: Kind::CadenzaOff, at: tk.at, ..Ev::default() });
            return Ok(i + 1);
        }
        "language" => return refuse("\\language goes before the music", tk.at),
        "grace" | "acciaccatura" | "appoggiatura" | "slashedGrace" => {
            let kind = match word {
                "acciaccatura" => GraceKind::Acciaccatura,
                "appoggiatura" => GraceKind::Appoggiatura,
                "slashedGrace" => GraceKind::Slashed,
                _ => GraceKind::Plain,
            };
            if st.tuplet.is_some() {
                return refuse("a grace note in a tuplet is not drawn", tk.at);
            }
            let before = st.events.len();
            let (items, notes) = (st.items, st.notes);
            let waiting = std::mem::take(&mut st.graces);
            let j = expr(toks, i + 1, st, hd)?;
            let read = st.events.split_off(before);
            st.graces = waiting;
            // Grace notes take no time and no number: the note after them
            // has both.
            st.items = items;
            st.notes = notes;
            for mut g in read {
                if g.k != Kind::Note {
                    return refuse("grace notes are notes: no rests or settings among them", g.at);
                }
                g.grace = Some(kind);
                g.slur_on = false;
                g.slur_off = false;
                g.n = None;
                st.graces.push(g);
            }
            return Ok(j);
        }
        "(" | ")" => {
            let last = last_note(&mut st.events, &tk)?;
            if word == "(" {
                last.phrase_on = true;
            } else {
                last.phrase_off = true;
            }
            return Ok(i + 1);
        }
        "arpeggio" => {
            last_note(&mut st.events, &tk)?.arpeggio = true;
            return Ok(i + 1);
        }
        "glissando" => {
            last_note(&mut st.events, &tk)?.gliss = true;
            return Ok(i + 1);
        }
        "startTrillSpan" => {
            last_note(&mut st.events, &tk)?.scripts.push(Script { name: "trill".into(), dir: 0 });
            return Ok(i + 1);
        }
        "stopTrillSpan" => return Ok(i + 1),
        "ottava" => {
            let mut j = i + 1;
            if is(toks, j, T::Sym('#')) {
                j += 1;
            }
            let sign = if is(toks, j, T::Sym('-')) {
                j += 1;
                -1
            } else {
                1
            };
            let n: i64 = toks.get(j).filter(|t| t.t == T::Num).and_then(|t| t.v.parse().ok()).unwrap_or(9);
            if n > 2 {
                return refuse("\\ottava wants #-2, #-1, #0, #1 or #2", tk.at);
            }
            st.events.push(Ev { k: Kind::Ottava, ottava: sign * n, at: tk.at, ..Ev::default() });
            return Ok(j + 1);
        }
        "mark" | "segnoMark" | "codaMark" => {
            let (runs, j) = match word {
                "segnoMark" => (vec![Run::glyph("segno")], i + 1),
                "codaMark" => (vec![Run::glyph("coda")], i + 1),
                _ => mark_label(toks, i, st)?,
            };
            st.events.push(Ev { k: Kind::Mark, texts: vec![Words { text: String::new(), dir: -1, runs }], at: tk.at, ..Ev::default() });
            return Ok(j);
        }
        _ => {}
    }
    if word.chars().all(|c| c.is_ascii_digit()) {
        let n: i64 = word.parse().unwrap_or(0);
        let last = last_note(&mut st.events, &tk)?;
        if !(1..=6).contains(&n) {
            return refuse("a string is 1 to 6", tk.at);
        }
        for p in last.pitches.clone() {
            last.strings.push((p.d, n));
        }
        last.fingers.push(Finger { text: word.to_owned(), dir: -1, head: None, circled: true });
        return Ok(i + 1);
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
        return tuplet(toks, i, st, word, hd);
    }
    if word == "repeat" {
        return repeat(toks, i, st, hd);
    }
    if word == "alternative" {
        return alternative(toks, i, st, hd);
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
        let j = music(toks, i + 2, st, hd)?;
        if j >= toks.len() || toks[j].t != T::Sym('}') {
            return refuse("a \\compressMMRests block that is never closed", tk.at);
        }
        return Ok(j + 1);
    }
    refuse(format!("\\{word} is not a command this reads"), tk.at)
}

/// A duration written as an argument — `4`, `8.`, `2*3`, `4*3/2` — in
/// ticks, and the token after it.
fn duration_arg(toks: &[Tok], mut i: usize) -> Option<(i64, usize)> {
    let base: i64 = toks.get(i).filter(|t| t.t == T::Num && !t.v.contains('/'))?.v.parse().ok()?;
    if ![1, 2, 4, 8, 16, 32, 64].contains(&base) {
        return None;
    }
    i += 1;
    let mut ticks = WHOLE / base;
    let mut add = ticks;
    while is(toks, i, T::Sym('.')) {
        add /= 2;
        ticks += add;
        i += 1;
    }
    if is(toks, i, T::Sym('*')) {
        let f = toks.get(i + 1).filter(|t| t.t == T::Num)?;
        let mut nd = f.v.split('/');
        let n: i64 = nd.next()?.parse().ok()?;
        let d: i64 = nd.next().map_or(Some(1), |v| v.parse().ok())?;
        if n < 1 || d < 1 || (ticks * n) % d != 0 {
            return None;
        }
        ticks = ticks * n / d;
        i += 2;
    }
    Some((ticks, i))
}

fn repeat(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let kind = toks.get(i + 1).filter(|t| t.t == T::Word).map_or("", |t| t.v.as_str());
    let counted = is(toks, i + 2, T::Num);
    if (kind == "unfold" || kind == "percent") && counted && starts_expr(toks, i + 3) || (kind == "unfold" && counted && is(toks, i + 3, T::Word)) {
        let count: i64 = toks[i + 2].v.parse().unwrap_or(0);
        if !(1..=64).contains(&count) {
            return refuse("a repeat is written out between 1 and 64 times", tk.at);
        }
        return unfold(toks, i, st, hd, count, kind == "percent");
    }
    let ok = i + 3 < toks.len() && kind == "volta" && counted && toks[i + 3].t == T::Sym('{');
    if !ok {
        return refuse("\\repeat wants volta, unfold or percent, a count and a { block } — the others are not drawn", tk.at);
    }
    st.events.push(Ev { k: Kind::RepeatStart, at: tk.at, ..Ev::default() });
    let alts_before = st.alts;
    let mut j = music(toks, i + 4, st, hd)?;
    if j >= toks.len() || toks[j].t != T::Sym('}') {
        return refuse("a \\repeat block that is never closed", tk.at);
    }
    j += 1;
    if is_cmd(toks, j, "alternative") {
        j = alternative(toks, j, st, hd)?;
    }
    if st.alts == alts_before {
        st.events.push(Ev { k: Kind::RepeatEnd, at: tk.at, ..Ev::default() });
    }
    Ok(j)
}

/// `\repeat unfold n { … }`: the music written out `n` times, as it sounds —
/// each time the same pitches, `\relative` having been read once — with an
/// `\alternative`'s endings in turn, the first standing for the times the
/// endings do not number. `\repeat percent n { … }` is the same music, its
/// copies drawn as the percent sign of the bar they fill.
fn unfold(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>, count: i64, percent: bool) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    let before = st.events.len();
    let (items0, notes0, tuplets0) = (st.items, st.notes, st.tuplets);
    let mut j = expr(toks, i + 3, st, hd)?;
    let body: Vec<Ev> = st.events.split_off(before);
    let mut endings: Vec<Vec<Ev>> = Vec::new();
    if is_cmd(toks, j, "alternative") {
        if percent {
            return refuse("\\repeat percent has no \\alternative", toks[j].at);
        }
        if !is(toks, j + 1, T::Sym('{')) {
            return refuse("\\alternative wants a { block } of { endings }", toks[j].at);
        }
        let mut k = j + 2;
        while starts_expr(toks, k) {
            k = expr(toks, k, st, hd)?;
            endings.push(st.events.split_off(before));
        }
        if !is(toks, k, T::Sym('}')) {
            return refuse("an \\alternative that is never closed", toks[j].at);
        }
        if endings.is_empty() || endings.len() as i64 > count {
            return refuse("an \\alternative has an ending for each time, at most", toks[j].at);
        }
        j = k + 1;
    }
    let mut out: Vec<Ev> = Vec::new();
    let mut tuplet_map: HashMap<i64, i64> = HashMap::new();
    let mut fresh = tuplets0;
    for time in 0..count {
        tuplet_map.clear();
        let ending = if endings.is_empty() { None } else { endings.get((time - (count - endings.len() as i64)).max(0) as usize) };
        let copy = percent && time > 0;
        if copy {
            st.percents += 1;
        }
        for ev in body.iter().chain(ending.into_iter().flatten()) {
            let mut e = ev.clone();
            if let Some(t) = &mut e.tuplet {
                let id = *tuplet_map.entry(t.id).or_insert_with(|| {
                    fresh += 1;
                    fresh
                });
                t.id = id;
            }
            if copy {
                e.percent = Some(st.percents);
            }
            out.push(e);
        }
    }
    // Numbered again in the order they now stand: the copies are notes of
    // their own, each its number, the source's spelling shared.
    let (mut items, mut notes) = (items0, notes0);
    for e in &mut out {
        if e.k == Kind::Note || e.k == Kind::Rest {
            e.i = items;
            items += 1;
        }
        if e.n.is_some() {
            e.n = Some(notes);
            notes += 1;
        }
    }
    if out.len() > MAX_TOKENS {
        return refuse("the repeat writes out to more music than this engraves", tk.at);
    }
    st.items = items;
    st.notes = notes;
    st.tuplets = fresh;
    st.events.extend(out);
    Ok(j)
}

fn alternative(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
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
        music(toks, start + 1, st, hd)?;
        if !last {
            st.events.push(Ev { k: Kind::RepeatEnd, at: toks[start].at, ..Ev::default() });
        }
        st.events.push(Ev { k: Kind::VoltaEnd, closed: !last, at: toks[start].at, ..Ev::default() });
    }
    st.alts += 1;
    Ok(close_at + 1)
}

fn tuplet(toks: &[Tok], i: usize, st: &mut St, word: &str, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
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
    let k = music(toks, j + 1, st, hd)?;
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
    let mut pitch_at = Vec::new();
    let mut chord_fingers = Vec::new();
    let mut strings = Vec::new();
    let mut drums = Vec::new();
    let mut drum_scripts = Vec::new();
    let mut root: Option<Pitch> = None;
    let mut rest: Option<String> = None;
    if tk.t == T::Sym('<') {
        i += 1;
        let mut reference = st.reference;
        let mut first = None;
        while i < toks.len() && toks[i].t == T::Word {
            if st.mode == Mode::Drums {
                let Some((dr, script)) = drum(&toks[i].v) else { return refuse(format!("'{}' is not a drum this reads", toks[i].v), toks[i].at) };
                pitches.push(Pitch { d: dr.d, alter: 0 });
                drum_scripts.extend((!script.is_empty()).then_some(script));
                drums.push(dr);
                pitch_at.push(toks[i].at);
                i += 1;
                continue;
            }
            let Some((d, alter)) = written(st, &toks[i], reference) else { return refuse(format!("'{}' is not a note name", toks[i].v), toks[i].at) };
            if reference.is_some() {
                reference = Some(d);
            }
            if first.is_none() {
                first = Some(d);
            }
            pitches.push(transposed(st, d, alter, toks[i].at)?);
            pitch_at.push(toks[i].at);
            i += 1;
            // `<c\5 e\4>`: the strings of a tablature.
            while toks.get(i).is_some_and(|t| t.t == T::Cmd && !t.v.is_empty() && t.v.chars().all(|c| c.is_ascii_digit())) {
                let n: i64 = toks[i].v.parse().unwrap_or(0);
                if !(1..=6).contains(&n) {
                    return refuse("a string is 1 to 6", toks[i].at);
                }
                if let Some(p) = pitches.last() {
                    strings.push((p.d, n));
                    chord_fingers.push(Finger { text: toks[i].v.clone(), dir: -1, head: Some(p.d), circled: true });
                }
                i += 1;
            }
            // `<c-1 e-3 g-5>`: a head's fingering.
            while matches!(toks.get(i).map(|t| t.t), Some(T::Sym('-' | '^' | '_'))) && is(toks, i + 1, T::Num) {
                beyond();
                let dir = match toks[i].t {
                    T::Sym('^') => -1,
                    T::Sym('_') => 1,
                    _ => 0,
                };
                chord_fingers.push(Finger { text: toks[i + 1].v.clone(), dir, head: pitches.last().map(|p: &Pitch| p.d), circled: false });
                i += 2;
            }
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
        st.chord = Some(pitches.clone());
        i += 1;
    } else {
        let word = tk.v.clone();
        if word == "r" || word == "R" || word == "s" {
            rest = Some(word);
        } else if word == "q" {
            beyond();
            let Some(chord) = st.chord.clone() else { return refuse("q repeats the chord before it, and there is none", tk.at) };
            pitches = chord;
        } else if st.mode == Mode::Drums {
            let Some((dr, script)) = drum(&word) else { return refuse(format!("'{word}' is not a drum this reads"), tk.at) };
            pitches.push(Pitch { d: dr.d, alter: 0 });
            drum_scripts.extend((!script.is_empty()).then_some(script));
            drums.push(dr);
            pitch_at.push(tk.at);
        } else if st.mode == Mode::Chords {
            // A chord's root, from c' as `\chordmode` counts.
            let Some((step, alter)) = pitch_name_in(st.lang, &word) else { return refuse(format!("'{word}' is not a note name"), tk.at) };
            root = Some(transposed(st, step + 7 * (tk.marks + 1), alter, tk.at)?);
        } else {
            let Some((d, alter)) = written(st, &tk, st.reference) else { return refuse(format!("'{word}' is not a note name"), tk.at) };
            if st.reference.is_some() {
                st.reference = Some(d);
            }
            pitches.push(transposed(st, d, alter, tk.at)?);
            pitch_at.push(tk.at);
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
    let mut chord_name = Vec::new();
    if let Some(root) = root {
        let (ps, name, j) = chord_symbol(toks, i, st, root)?;
        pitches = ps;
        chord_name = name;
        i = j;
    } else if st.mode == Mode::Chords && rest.as_deref() == Some("r") {
        chord_name = vec![Run::words("N.C.", Weight::Regular)];
    }
    // `c4:16`, a stroke a beam shorter than the value it is repeated in.
    let mut tremolo = 0;
    if is(toks, i, T::Sym(':')) && st.mode != Mode::Chords {
        tremolo = 32;
        if is(toks, i + 1, T::Num) {
            tremolo = toks[i + 1].v.parse().unwrap_or(0);
            i += 1;
        }
        i += 1;
        if rest.is_some() || ![8, 16, 32, 64].contains(&tremolo) || tremolo <= st.base {
            return refuse("a tremolo is a note's, in 8, 16, 32 or 64, shorter than the note", tk.at);
        }
    }
    let mut count = 1;
    if is(toks, i, T::Sym('*')) {
        if rest.as_deref() == Some("s") {
            beyond();
        }
        if !matches!(rest.as_deref(), Some("R" | "s")) {
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
    // A spacer counted, `s1*16`: the time of all of them in one.
    if rest.as_deref() == Some("s") && count > 1 {
        ticks *= count;
        count = 1;
    }
    let scripts = drum_scripts.iter().map(|n| Script { name: (*n).to_owned(), dir: -1 }).collect();
    let mut ev = Ev { k: Kind::Note, pitches, pitch_at, base, dots, ticks, at: tk.at, i: st.items, fingers: chord_fingers, tremolo, chord_name, drums, strings, scripts, ..Ev::default() };
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
    ev.graces = std::mem::take(&mut st.graces);
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

// ---- Markup -------------------------------------------------------------------
//
// `\markup` as a score's words need it: words, strings and figures, `\bold`,
// `\italic` and `\upright` faces, `\line` and `\concat`, a `\musicglyph`;
// what only sizes or moves the words — `\small`, `\large`, `\box`,
// `\raise #1` — is read and drawn at the words' own size and place.

/// The rehearsal mark's label: `\mark \default` the next letter, `\mark 5`
/// the fifth, `\mark "A"` or `\mark \markup { … }` as written.
fn mark_label(toks: &[Tok], i: usize, st: &mut St) -> Result<(Vec<Run>, usize), ScoreError> {
    let at = toks[i].at;
    let mut j = i + 1;
    if is(toks, j, T::Sym('#')) {
        j += 1;
    }
    let bold = |text: String| vec![Run::words(text, Weight::Bold)];
    match toks.get(j) {
        Some(t) if t.t == T::Cmd && t.v == "default" => {
            st.marks += 1;
            Ok((bold(letters(st.marks)), j + 1))
        }
        Some(t) if t.t == T::Num && !t.v.contains('/') => {
            let n: i64 = t.v.parse().unwrap_or(1);
            st.marks = n;
            Ok((bold(letters(n)), j + 1))
        }
        Some(t) if t.t == T::Str => Ok((bold(t.v.clone()), j + 1)),
        Some(t) if t.t == T::Cmd && t.v == "markup" => markup(toks, j + 1, t.at),
        _ => refuse("\\mark wants \\default, a number, a string or \\markup", at),
    }
}

/// A rehearsal mark's letters: A … Z without I, then AA, AB …
fn letters(n: i64) -> String {
    const ABC: &str = "ABCDEFGHJKLMNOPQRSTUVWXYZ";
    let mut n = n.max(1) - 1;
    let mut out = String::new();
    loop {
        out.insert(0, ABC.as_bytes()[(n % 25) as usize] as char);
        if n < 25 {
            break;
        }
        n = n / 25 - 1;
    }
    out
}

/// A `\markup`'s pieces from `i`, just after the command, and the token after
/// it.
fn markup(toks: &[Tok], i: usize, at: usize) -> Result<(Vec<Run>, usize), ScoreError> {
    let mut runs = Vec::new();
    let j = markup_item(toks, i, Weight::Regular, &mut runs, at)?;
    if runs.is_empty() {
        return refuse("a \\markup with nothing in it", at);
    }
    Ok((runs, j))
}

/// The SMuFL name of a glyph `\musicglyph` names in LilyPond's.
fn music_glyph(name: &str) -> Option<&'static str> {
    Some(match name {
        "scripts.segno" => "segno",
        "scripts.coda" | "scripts.varcoda" => "coda",
        "scripts.ufermata" => "fermataAbove",
        "scripts.dfermata" => "fermataBelow",
        "scripts.trill" => "ornamentTrill",
        "scripts.prall" => "ornamentShortTrill",
        "scripts.mordent" => "ornamentMordent",
        "scripts.turn" => "ornamentTurn",
        "scripts.staccato" => "articStaccatoAbove",
        "scripts.sforzato" => "articAccentAbove",
        _ => return None,
    })
}

/// One item of markup — a word, a string, `{ … }` or a command and what it
/// takes — onto `runs`, in `weight`.
fn markup_item(toks: &[Tok], i: usize, weight: Weight, runs: &mut Vec<Run>, at: usize) -> Result<usize, ScoreError> {
    let Some(t) = toks.get(i) else { return refuse("a \\markup that is never finished", at) };
    match t.t {
        T::Sym('{') => {
            let mut j = i + 1;
            while j < toks.len() && toks[j].t != T::Sym('}') {
                j = markup_item(toks, j, weight, runs, at)?;
            }
            if j >= toks.len() {
                return refuse("a \\markup { that is never closed", at);
            }
            Ok(j + 1)
        }
        T::Str | T::Word | T::Num => {
            let mut text = t.v.clone();
            if t.t == T::Word {
                for _ in 0..t.marks.max(0) {
                    text.push('\'');
                }
                for _ in 0..(-t.marks).max(0) {
                    text.push(',');
                }
            }
            runs.push(Run::words(text, weight));
            Ok(i + 1)
        }
        T::Sym(c) if ".,;:!?-+=*/'()[]".contains(c) => {
            match runs.last_mut().filter(|r| !r.glyph) {
                Some(last) if ".,;:!?)]".contains(c) => last.text.push(c),
                _ => runs.push(Run::words(c.to_string(), weight)),
            }
            Ok(i + 1)
        }
        T::Cmd => match t.v.as_str() {
            "bold" => markup_item(toks, i + 1, Weight::Bold, runs, at),
            "italic" => markup_item(toks, i + 1, Weight::Italic, runs, at),
            "upright" | "normal-text" | "roman" | "medium" => markup_item(toks, i + 1, Weight::Regular, runs, at),
            "line" | "concat" | "small" | "smaller" | "large" | "larger" | "huge" | "tiny" | "teeny" | "normalsize" | "box" | "circle" | "center-align" | "left-align" | "right-align" | "column"
            | "center-column" | "fill-line" | "sans" | "serif" | "caps" | "smallCaps" => markup_item(toks, i + 1, weight, runs, at),
            "fontsize" | "abs-fontsize" | "raise" | "lower" | "halign" | "with-color" | "translate" | "magnify" => {
                // `\fontsize #2`, `\raise #1`: an argument the drawing leaves be.
                let mut j = i + 1;
                if is(toks, j, T::Sym('#')) {
                    j = scheme_end(toks, j)?;
                } else {
                    j += 1;
                }
                markup_item(toks, j, weight, runs, at)
            }
            "hspace" | "vspace" => {
                let j = if is(toks, i + 1, T::Sym('#')) { scheme_end(toks, i + 1)? } else { i + 2 };
                Ok(j)
            }
            "musicglyph" => {
                let mut j = i + 1;
                if is(toks, j, T::Sym('#')) {
                    j += 1;
                }
                let name = toks.get(j).filter(|n| n.t == T::Str).map(|n| n.v.clone()).unwrap_or_default();
                let Some(glyph) = music_glyph(&name) else { return refuse(format!("\\musicglyph \"{name}\" is not one this draws"), t.at) };
                runs.push(Run { weight, ..Run::glyph(glyph) });
                Ok(j + 1)
            }
            other => refuse(format!("\\{other} is not markup this draws"), t.at),
        },
        _ => refuse(format!("'{}' is not markup this draws", t.v), t.at),
    }
}

// ---- Chords ---------------------------------------------------------------------
//
// `\chordmode`: a root, then `:` and what LilyPond's chord modifiers say —
// `m`, `maj`, `dim`, `aug`, `sus`, the steps up to a number, `.5-` and
// `.9+` altering one, `^7` taking one away — and `/e` the bass. Its pitches
// for a staff, its name for `ChordNames`, named as LilyPond's default
// (Ignatzek) names it: C, Cm, C⁷, C△, C°, Cø, C+, Csus4, C⁷ ♭⁹, C/E.

/// The natural semitones of a chord's steps, 1 to 13.
fn step_semis(step: i64) -> i64 {
    match step {
        1 => 0,
        2 => 2,
        3 => 4,
        4 => 5,
        5 => 7,
        6 => 9,
        7 => 11,
        9 => 14,
        11 => 17,
        13 => 21,
        _ => 0,
    }
}

/// Where a token's characters end.
fn tok_end(t: &Tok) -> usize {
    t.at + t.v.chars().count() + t.marks.unsigned_abs() as usize + usize::from(t.t == T::Str) * 2
}

/// The modifiers after a chord's root and duration, `:m7.5-/g`, and the
/// chord they make: its pitches, its name, and the token after them.
fn chord_symbol(toks: &[Tok], mut i: usize, st: &St, root: Pitch) -> Result<(Vec<Pitch>, Vec<Run>, usize), ScoreError> {
    let at = toks.get(i.saturating_sub(1)).map_or(0, |t| t.at);
    // The steps sounding, by number, with their alteration from natural
    // (a 7 is a minor seventh unless `maj` says).
    let mut steps: Vec<(i64, i64)> = vec![(1, 0), (3, 0), (5, 0)];
    let (mut minor, mut dim, mut aug, mut maj, mut sus) = (false, false, false, false, 0);
    let mut extension = 0;
    let mut altered: Vec<(i64, i64)> = Vec::new();
    let mut removed: Vec<i64> = Vec::new();
    // Every token of the modifiers touches the one before it.
    let adjacent = |j: usize| j > 0 && toks.get(j).is_some_and(|t| t.at == tok_end(&toks[j - 1]));
    if is(toks, i, T::Sym(':')) && adjacent(i + 1) {
        i += 1;
        let mut first = true;
        let mut removing = false;
        while let Some(t) = toks.get(i) {
            if !adjacent(i) {
                break;
            }
            match (t.t, t.v.as_str()) {
                (T::Word, "m" | "min") if first => minor = true,
                (T::Word, "maj") if first => {
                    maj = true;
                    if !is(toks, i + 1, T::Num) || !adjacent(i + 1) {
                        extension = extension.max(7);
                    }
                }
                (T::Word, "dim") if first => {
                    dim = true;
                    minor = true;
                }
                (T::Word, "aug") if first => aug = true,
                (T::Word, "sus") => {
                    sus = 4;
                    if is(toks, i + 1, T::Num) && adjacent(i + 1) {
                        sus = toks[i + 1].v.trim_end_matches('/').parse().unwrap_or(4);
                        i += 1;
                    }
                }
                (T::Num, v) => {
                    let n: i64 = v.trim_end_matches('/').parse().unwrap_or(0);
                    let mut alter = 0;
                    if is(toks, i + 1, T::Sym('-')) && adjacent(i + 1) {
                        alter = -1;
                        i += 1;
                    } else if is(toks, i + 1, T::Sym('+')) && adjacent(i + 1) {
                        alter = 1;
                        i += 1;
                    }
                    if removing {
                        removed.push(n);
                    } else if first && alter == 0 && [6, 7, 9, 11, 13].contains(&n) {
                        extension = extension.max(n);
                    } else if first && alter == 0 && n == 5 {
                    } else if first && alter == 0 && n == 1 {
                        removed.push(3);
                    } else {
                        altered.push((n, alter));
                    }
                    first = false;
                    if v.ends_with('/') {
                        i += 1;
                        break;
                    }
                }
                (T::Sym('.'), _) => first = false,
                (T::Sym('^'), _) => removing = true,
                _ => break,
            }
            i += 1;
        }
    }
    if dim {
        steps[2].1 = -1;
    }
    if minor {
        steps[1].1 = -1;
    }
    if aug {
        steps[2].1 = 1;
    }
    let seventh = if maj {
        0
    } else if dim && extension == 7 {
        -2
    } else {
        -1
    };
    match extension {
        6 => steps.push((6, 0)),
        7 => steps.push((7, seventh)),
        9 => steps.extend([(7, seventh), (9, 0)]),
        11 => steps.extend([(7, seventh), (9, 0), (11, 0)]),
        13 => steps.extend([(7, seventh), (9, 0), (13, 0)]),
        _ => {}
    }
    for &(n, a) in &altered {
        match steps.iter_mut().find(|s| s.0 == n) {
            Some(s) => s.1 = a,
            None => steps.push((n, a)),
        }
    }
    if sus > 0 {
        steps.retain(|s| s.0 != 3);
        steps.push((sus, 0));
    }
    steps.retain(|s| !removed.contains(&s.0));
    steps.sort_by_key(|s| step_semis(s.0) + s.1);
    let mut pitches = Vec::new();
    for &(n, a) in &steps {
        let d = root.d + n - 1;
        let want = semitones(root.d, root.alter) + step_semis(n) + a;
        let alter = want - semitones(d, 0);
        if !(-2..=2).contains(&alter) {
            return refuse("this chord's steps need more than a double sharp or flat", at);
        }
        pitches.push(Pitch { d, alter });
    }
    // `/e`, the bass: a step of the chord an octave down, or `/+e` one more.
    let mut bass_name = None;
    if is(toks, i, T::Sym('/')) || toks.get(i.wrapping_sub(1)).is_some_and(|t| t.t == T::Num && t.v.ends_with('/')) {
        if is(toks, i, T::Sym('/')) {
            i += 1;
        }
        let added = is(toks, i, T::Sym('+'));
        if added {
            i += 1;
        }
        let Some((step, alter)) = toks.get(i).filter(|t| t.t == T::Word).and_then(|t| pitch_name_in(st.lang, &t.v)) else { return refuse("a chord's bass after / is a note name", at) };
        let bass = transposed(st, step + 7, alter, at)?;
        i += 1;
        let pc = |p: &Pitch| semitones(p.d, p.alter).rem_euclid(12);
        match pitches.iter().position(|p| pc(p) == pc(&bass)) {
            Some(k) if !added => {
                let mut b = pitches.remove(k);
                while b.d >= root.d {
                    b.d -= 7;
                }
                pitches.insert(0, b);
            }
            _ => {
                let mut b = bass;
                while b.d >= root.d {
                    b.d -= 7;
                }
                pitches.insert(0, b);
            }
        }
        bass_name = Some(bass);
    }
    let name = chord_name(root, &steps, minor, dim, aug, maj, sus, extension, &altered, bass_name);
    Ok((pitches, name, i))
}

/// A pitch's name in a chord symbol: its letter and its accidental.
fn root_runs(p: Pitch) -> Vec<Run> {
    let letter = ["C", "D", "E", "F", "G", "A", "B"][p.d.rem_euclid(7) as usize];
    let mut out = vec![Run { gap: false, ..Run::words(letter, Weight::Regular) }];
    let acc = match p.alter {
        -2 => Some("accidentalDoubleFlat"),
        -1 => Some("accidentalFlat"),
        1 => Some("accidentalSharp"),
        2 => Some("accidentalDoubleSharp"),
        _ => None,
    };
    if let Some(a) = acc {
        out.push(Run { gap: false, size: 0.6, ..Run::glyph(a) });
    }
    out
}

/// A chord's name as LilyPond's Ignatzek names it.
#[allow(clippy::too_many_arguments)]
fn chord_name(root: Pitch, steps: &[(i64, i64)], minor: bool, dim: bool, aug: bool, maj: bool, sus: i64, extension: i64, altered: &[(i64, i64)], bass: Option<Pitch>) -> Vec<Run> {
    let mut out = root_runs(root);
    let sup = |text: &str| Run { sup: true, gap: false, ..Run::words(text, Weight::Regular) };
    let words = |text: &str| Run { gap: false, ..Run::words(text, Weight::Regular) };
    let has = |n: i64, a: i64| steps.iter().any(|s| s.0 == n && s.1 == a);
    let half_dim = minor && has(5, -1) && has(7, -1);
    if half_dim {
        out.push(sup("ø"));
    } else if dim {
        out.push(words("°"));
        if extension == 7 {
            out.push(sup("7"));
        }
    } else {
        if minor {
            out.push(words("m"));
        }
        if aug && extension == 0 {
            out.push(words("+"));
        }
        let top = match extension {
            13 => "9 13",
            11 => "11",
            9 => "9",
            7 => "7",
            6 => "6",
            _ => "",
        };
        if maj {
            out.push(sup("△"));
            if extension > 7 {
                out.push(Run { gap: true, ..sup(top) });
            }
        } else if !top.is_empty() {
            out.push(sup(top));
        } else if altered.iter().any(|a| a.0 == 9 && a.1 == 0) {
            out.push(sup("9"));
        }
    }
    for &(n, a) in altered {
        if a == 0 || (half_dim && n == 5) {
            continue;
        }
        let acc = if a < 0 { "accidentalFlat" } else { "accidentalSharp" };
        out.push(Run { sup: true, size: 0.45, ..Run::glyph(acc) });
        out.push(sup(&n.to_string()));
    }
    if aug && extension > 0 {
        out.push(Run { sup: true, size: 0.45, ..Run::glyph("accidentalSharp") });
        out.push(sup("5"));
    }
    if sus > 0 {
        out.push(sup(&format!("sus{sus}")));
    }
    if let Some(b) = bass {
        out.push(words("/"));
        out.extend(root_runs(b));
    }
    out
}
