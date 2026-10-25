//! Reading LilyPond: the subset `score()` draws, every construct outside it
//! refused with its position (`eui_builders_score.sl`, "Reading LilyPond").
//!
//! Beyond the Soli engraver's subset, what protocol 11 reads (03 §1.4): a
//! file as LilyPond's users write one — variables, `\paper`, `\layout` and
//! `\midi` blocks, `\language`, a `\new Voice` or two on a staff with a
//! `\global` of the staff's settings beside them, `\new Lyrics \lyricsto`
//! the voice it is named for — and `\fixed`, `\transpose`, `q`, `\partial`,
//! `\cadenzaOn`, spacers, and `\repeat unfold` and `percent`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::glyphs::Weight;
use crate::{ScoreError, WHOLE};

/// The dynamics a score prints.
pub const DYNAMICS: [&str; 21] = ["pppp", "ppp", "pp", "p", "mp", "mf", "f", "ff", "fff", "ffff", "fp", "sf", "sff", "sp", "spp", "sfz", "sfp", "rfz", "rf", "fz", "n"];

/// The articulations a score prints, by their LilyPond names: the Soli
/// engraver's seven, and the ornaments, the hi-hat's signs, the bowings,
/// the harmonic's circle and the espressivo protocol 11 adds.
pub const SCRIPTS: [&str; 17] =
    ["staccato", "tenuto", "accent", "marcato", "staccatissimo", "portato", "fermata", "trill", "prall", "mordent", "turn", "open", "stopped", "upbow", "downbow", "flageolet", "espressivo"];

/// The most tokens a source may come to once its variables are written out:
/// a variable used in a variable used in a variable multiplies.
const MAX_TOKENS: usize = 400_000;

/// Where `\afterGrace`'s grace notes start, as a part of their note:
/// LilyPond's `afterGraceFraction`.
pub const AFTER_GRACE: f64 = 0.75;

thread_local! {
    /// The source being read uses what protocol 11 adds.
    static BEYOND: Cell<bool> = const { Cell::new(false) };
    /// What the source being read says that is passed over.
    static WARNINGS: RefCell<Vec<Warning>> = const { RefCell::new(Vec::new()) };
}

/// The most warnings one source keeps: past them, a file that sets a
/// property on every note says so once more and stops.
const MAX_WARNINGS: usize = 200;

/// What a source says that is read and not drawn — a layout setting
/// (`\override`, `\stemUp` …), a `\layout` block's contents, a header
/// field no title prints — so a file that writes one still opens, and a
/// caller can say what was left out. Never a refusal: a source the
/// engraver cannot set is refused, not warned about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// What was passed over, in words: "\override Stem.direction is not
    /// drawn".
    pub message: String,
    /// The character it starts at.
    pub at: usize,
}

/// A setting read and passed over, once per place in the source.
pub(crate) fn warn(message: impl Into<String>, at: usize) {
    WARNINGS.with(|w| {
        let mut w = w.borrow_mut();
        let message = message.into();
        if w.len() < MAX_WARNINGS && !w.iter().any(|x| x.at == at && x.message == message) {
            w.push(Warning { message, at });
        }
    });
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
    // A clef or a key in the middle of a bar shows only once the bars are cut.
    if let Ok(p) = &read {
        if p.staves.iter().any(|s| crate::measure::measures(&s.events).is_ok_and(|bars| bars.iter().any(|b| !b.inline.is_empty()))) {
            beyond();
        }
    }
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
        } else if "{}<>[]()~|.=*-^_!#:/+?',".contains(ch) {
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
                if ["override", "revert", "once", "tweak"].contains(&word.as_str()) {
                    // A setting among the words, `\override LyricText.self-alignment-X
                    // = #LEFT`: read and passed over (a `\layout`'s `\Lyrics`
                    // context is where the engraver honours it).
                    beyond();
                    let k = lyric_setting_end(cs, jj);
                    warn(format!("\\{word} inside lyrics is not drawn"), i);
                    i = k;
                    continue;
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

/// Where a setting among lyrics ends, from just after its command: its
/// path, `=` and its value (`#LEFT`, `#'(1 . 2)`, `##f`), or a command after it.
fn lyric_setting_end(cs: &[char], mut i: usize) -> usize {
    let n = cs.len();
    i = skip_blank(cs, i);
    if i < n && cs[i] == '\\' {
        // `\once \override …`: the command it goes before.
        let (_, jj) = name_at(cs, i + 1, letter);
        return lyric_setting_end(cs, jj);
    }
    while i < n && (cs[i].is_ascii_alphanumeric() || ".-#'".contains(cs[i])) {
        i += 1;
    }
    i = skip_blank(cs, i);
    if i < n && cs[i] == '=' {
        i = skip_blank(cs, i + 1);
    } else {
        return i;
    }
    while i < n && (cs[i] == '#' || cs[i] == '\'') {
        i += 1;
    }
    if i < n && cs[i] == '(' {
        let mut depth = 0;
        while i < n {
            if cs[i] == '(' {
                depth += 1;
            }
            if cs[i] == ')' {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            i += 1;
        }
        return i;
    }
    while i < n && !blank(cs[i]) && cs[i] != '}' {
        i += 1;
    }
    i
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
/// note; each `\addlyrics` after the first a verse under it.
fn attach_lyrics(toks: &[Tok], mut i: usize, staves: &mut [Staff]) -> Result<usize, ScoreError> {
    while is_cmd(toks, i, "addlyrics") {
        let at = toks[i].at;
        let Some(last) = staves.last_mut() else {
            return refuse("\\addlyrics follows a staff's music", at);
        };
        let mut j = i + 1;
        if is_cmd(toks, j, "lyricmode") {
            j += 1;
        }
        if !is(toks, j, T::Lyrics) {
            return refuse("\\addlyrics wants a { block } of words", at);
        }
        let words = syllables(&toks[j].lyrics, at)?;
        add_verse(last, words, 1, at)?;
        i = j + 1;
    }
    Ok(i)
}

/// A verse of a staff's lyrics, sung to its voice `voice`: the first, or
/// one more under the ones before — all sung to one voice.
fn add_verse(staff: &mut Staff, words: Vec<Syllable>, voice: u8, at: usize) -> Result<(), ScoreError> {
    if staff.lyrics.is_none() {
        staff.lyrics = Some(words);
        staff.lyrics_voice = voice;
        return Ok(());
    }
    if staff.lyrics_voice != voice {
        return refuse("a staff's verses are sung to one voice", at);
    }
    staff.verses.push(words);
    Ok(())
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
    /// A movement starts: the music of a `\score` after the first, on a
    /// line of its own (`page`: a page, for a `\bookpart`), its bars counted
    /// from 1 and its clef, key and metre LilyPond's defaults until it says
    /// its own. `texts`: its header's `piece`, then its `opus`.
    Movement,
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
    /// Its accidental is a reminder, `cis?`: drawn in parentheses.
    pub paren: bool,
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
    /// An additive metre's parts, `2+3/8` as `[2, 3, 0, 0]`; zeros for one figure.
    pub time_parts: [u8; 4],
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
    /// The tuplet around that one, when tuplets nest: a triplet in a quintuplet.
    pub tuplet_outer: Option<TupletRef>,
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
    /// `\sustainOn`: the sustain pedal pressed with this note, "Ped."
    /// under the staff.
    pub pedal_on: bool,
    /// `\sustainOff`: let go with this note, "*" — before it is pressed
    /// again, when a note has both (`\sustainOff\sustainOn`).
    pub pedal_off: bool,
    /// `c4:16`: the note repeated in sixteenths, strokes across its stem.
    pub tremolo: i64,
    /// `\ottava`'s octaves where the note stands; on `Kind::Ottava`, the new
    /// ones.
    pub ottava: i64,
    /// The grace notes before it, `\grace` and its kin: notes of their own,
    /// taking no time, drawn small.
    pub graces: Vec<Ev>,
    /// `\afterGrace`'s grace notes, after it: drawn small before the next
    /// note, played in the end of this one, from `after_at`.
    pub after_graces: Vec<Ev>,
    /// Where `after_graces` start, as a part of the note: 3/4 unless
    /// `\afterGrace 7/8 …` says.
    pub after_at: f64,
    /// On a grace note, which: `\grace`, `\acciaccatura` (slashed and
    /// slurred), `\appoggiatura` (slurred) or `\slashedGrace`.
    pub grace: Option<GraceKind>,
    /// A `\chordmode` chord's name, as `ChordNames` prints it; "N.C." for a
    /// rest there.
    pub chord_name: Vec<Run>,
    /// A `\figuremode` figure's lines, top to bottom: `<6 4>` is two, each
    /// its number and its accidental; an empty line for `_`.
    pub figures: Vec<Vec<Run>>,
    /// Its size in LilyPond's font-size steps: 0 as usual, `\small` -1,
    /// `\tiny` -2, `\teeny` and a cue -3; each six steps half the size.
    pub size: i8,
    /// A cue note (`\new CueVoice`, `\cueDuring`): another part's, drawn
    /// small to be read, and not played.
    pub cue: bool,
    /// A clef changed in the middle of a bar just before it: drawn before
    /// it, smaller, as LilyPond draws a clef that is not a line's.
    pub clef_change: Option<Clef>,
    /// A key changed in the middle of a bar just before it: the key before,
    /// in fifths, and the new one — drawn before it, the old key's
    /// accidentals cancelled as LilyPond cancels them.
    pub key_change: Option<(i64, i64)>,
    /// A metre changed in the middle of a bar just before it (score #408):
    /// drawn before it, after a clef or a key changed there.
    pub time_change: Option<((i64, i64), [u8; 4])>,
    /// Where those changes start, from its x.
    pub change_x: Option<f64>,
    /// Room before it that its duration's space does not cover: a change's
    /// width, which LilyPond sets in a column of its own.
    pub pre: f64,
    /// `\stemUp` (1) or `\stemDown` (-1) in force where it is written; 0
    /// after `\stemNeutral`, or none written: its voice's or its own.
    pub stem_dir: i8,
    /// `\autoBeamOff` in force: beamed only where `[ ]` says.
    pub no_autobeam: bool,
    /// `\breathe` or `\caesura` after it: the sign's glyph, after the note
    /// over the staff.
    pub breath: Option<&'static str>,
    /// Where that sign stands, from its x.
    pub breath_x: f64,
    /// `\laissezVibrer`: a short tie from each head, to nothing.
    pub lv: bool,
    /// `\repeatTie`: a short tie into each head, from nothing.
    pub repeat_tie: bool,
    /// `\cresc`, `\dim`, `\decresc`: the hairpin it starts is words —
    /// "cresc." and a dashed line — instead of a wedge.
    pub hairpin_text: Option<String>,
    /// `\startTextSpan`: a dashed line over the notes from it to the
    /// `\stopTextSpan`, after the words `\override TextSpanner.bound-details
    /// .left.text` set (none: the line alone).
    pub span_start: Option<Vec<Run>>,
    /// `\stopTextSpan`.
    pub span_stop: bool,
    /// Its pitches written with `!`, `cis!`: an accidental printed whatever
    /// the key and the bar say. Diatonic numbers.
    pub forced: Vec<i64>,
    /// … with `?`, `cis?`: printed, in parentheses — a reminder.
    pub cautionary: Vec<i64>,
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
    /// The staff a note is drawn on after `\\change Staff = "…"`, by its name.
    pub cross: Option<String>,
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
    /// Its syllables of the verses after the first, a line each.
    pub verses: Vec<Option<Syllable>>,
    /// It holds the syllable of a note before it: a melisma.
    pub lyric_held: bool,
    /// `\showStaffSwitch` was in force when a `\change Staff` came just
    /// before it: a line follows the voice from the note before to it.
    pub follow: bool,
    /// `\crossStaff`: its stem joins the stem of the note sounding with it
    /// on the staff next to it, one stem for a chord over two staves.
    pub span_stem: bool,
    /// Sent to the other staff by `\autoChange`: a rest stays in its place.
    pub rest_behind: bool,
    /// On a note drawn on another staff than its own, `\change Staff`'s:
    /// the staff it was written in.
    pub moved: Option<usize>,
    /// A note moved to a staff that plays nothing with it: its stem goes as
    /// its place on that staff says, not as its voice's.
    pub free: bool,
    /// Under a beam that crosses from one staff to the other: its stem and
    /// the beam are the system's to draw, once the staves are stacked.
    pub xbeam: bool,
    /// The layout settings in force where it is written: `\override`,
    /// `\set`, `\tweak`, `\omit` and their kin (score #403).
    pub look: crate::look::Look,
    /// Notes played and not drawn, each with where it starts from this
    /// event's start: what `\partCombine` leaves out of the staff — the
    /// other part under a solo, part II in unison — and LilyPond's MIDI
    /// still plays (score #412).
    pub unseen: Vec<(i64, Ev)>,
    /// `\scaleDurations n/d`: the note lasts n/d of its written value, with
    /// no tuplet drawn (`ticks` is the scaled length).
    pub scale: Option<(i64, i64)>,
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
    /// The layout settings its `\with` honours: `\override`, `\set`,
    /// `\RemoveEmptyStaves` …
    pub look: crate::look::Map,
}

/// What a staff is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StaffKind {
    /// Five lines and notes.
    #[default]
    Notes,
    /// `\new ChordNames`: the chords' names over the staff below, no lines.
    ChordNames,
    /// `\new FiguredBass`: a thorough bass's figures under the staff above,
    /// no lines.
    Figures,
    /// `\new FretBoards`: each chord's guitar diagram over the staff below,
    /// its shape found for a guitar in standard tuning.
    FretBoards,
    /// An ossia: `\new Staff \with { alignAboveContext = … }` inside a
    /// staff's `<< >>` — another way to play a passage, on a staff over the
    /// one it belongs to, drawn only where it has notes, and not played.
    Ossia,
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
    Figures,
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
    /// Its `\addlyrics` or `\lyricsto`, a syllable a note: the first verse.
    pub lyrics: Option<Vec<Syllable>>,
    /// The verses after the first, each set on its own line under the one
    /// before, sung to the same voice.
    pub verses: Vec<Vec<Syllable>>,
    /// The voice they are sung to: 1, or 2 for the second of two.
    pub lyrics_voice: u8,
    /// Its voices' names, `\new Voice = "name"`, and their numbers.
    pub voices: Vec<(String, u8)>,
    /// Its name, `\\new Staff = "down"`: what `\\change Staff` names.
    pub name: Option<String>,
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
    /// What `\paper` and `#(set-global-staff-size …)` say of the page.
    pub paper: PaperSpec,
    /// The staves.
    pub staves: Vec<Staff>,
    /// Their groups, innermost first.
    pub groups: Vec<Group>,
    /// What the source says that was read and not drawn, in the order it
    /// is written (protocol 11).
    pub warnings: Vec<Warning>,
    /// `\override Score.SpacingSpanner …`: how durations are spaced.
    pub spacing: SpacingSpanner,
}

/// The page a source asks for: `\paper { #(set-paper-size "letter")
/// left-margin = 15\mm … }` and `#(set-global-staff-size 18)`. Lengths in
/// millimetres; none where the source says nothing, the A4 defaults then.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaperSpec {
    /// `set-paper-size`'s name: "a4", "letter", "a3landscape"…
    pub size: Option<String>,
    /// The staff size, in points (LilyPond's default is 20).
    pub staff_size: Option<f64>,
    /// `left-margin`.
    pub left: Option<f64>,
    /// `right-margin`.
    pub right: Option<f64>,
    /// `top-margin`.
    pub top: Option<f64>,
    /// `bottom-margin`.
    pub bottom: Option<f64>,
    /// `indent`: the first system's.
    pub indent: Option<f64>,
    /// `page-breaking`: `#ly:optimal-breaking` spreads the lines evenly over
    /// the pages (true), `#ly:minimal-breaking` fills each in turn (false).
    pub even_pages: Option<bool>,
    /// `systems-per-page` (or `max-systems-per-page`): at most so many
    /// systems on a page.
    pub systems_per_page: Option<usize>,
    /// `oddHeaderMarkup`: the line over the music of an odd page — the
    /// first, the third… — and of the even ones when `even_header` is none.
    pub odd_header: Option<PageLine>,
    /// `evenHeaderMarkup`.
    pub even_header: Option<PageLine>,
    /// `oddFooterMarkup`: the line at the foot of an odd page, and of the
    /// even ones when `even_footer` is none. None: the title and the page's
    /// number out of how many.
    pub odd_footer: Option<PageLine>,
    /// `evenFooterMarkup`.
    pub even_footer: Option<PageLine>,
    /// `line-width`: the music's width; the margins share what is left.
    pub line_width: Option<f64>,
    /// `short-indent`: every system's indent but the first's.
    pub short_indent: Option<f64>,
    /// `ragged-right`: every line at its natural width, not stretched to the margin.
    pub ragged_right: Option<bool>,
    /// `ragged-last`: the last line so.
    pub ragged_last: Option<bool>,
    /// `ragged-bottom`: the systems of a page at their natural distance, not
    /// spread down the page.
    pub ragged_bottom: Option<bool>,
    /// `ragged-last-bottom`: the last page's so (LilyPond's default: yes).
    pub ragged_last_bottom: Option<bool>,
    /// `system-system-spacing`: from one system to the next.
    pub system_spacing: Spacing,
    /// `score-system-spacing`: from a score's (a movement's) last system to
    /// the next score's first.
    pub score_spacing: Spacing,
    /// `markup-system-spacing`: from the titles to the first system.
    pub markup_spacing: Spacing,
    /// `max-systems-per-page`: at most so many systems on a page.
    pub max_systems_per_page: Option<usize>,
    /// `page-count`: the music on so many pages.
    pub page_count: Option<usize>,
    /// `system-count`: the music on so many systems.
    pub system_count: Option<usize>,
    /// `\enablePolymeter`, or the `Timing_translator` moved from the
    /// `\Score` to the `\Staff` in `\layout`: each staff keeps its own
    /// `\time` and `\partial`. Otherwise they are the score's, as
    /// LilyPond's, whatever staff writes them (score #408).
    pub polymeter: bool,
}

/// A flexible vertical distance, LilyPond's spacing alist: in staff spaces,
/// none where the source says nothing (LilyPond's default then).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Spacing {
    /// `basic-distance`: between the two items' reference points — a
    /// staff's middle line, a title's top — when nothing is in the way.
    pub basic: Option<f64>,
    /// `minimum-distance`: the least, however a page is filled.
    pub minimum: Option<f64>,
    /// `padding`: the least clear room between the two items' ink.
    pub padding: Option<f64>,
    /// `stretchability`: how readily it stretches to fill a page.
    pub stretch: Option<f64>,
}

/// LilyPond's horizontal spacing settings (`\override Score.SpacingSpanner
/// …`): none where the source says nothing, LilyPond's defaults then.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SpacingSpanner {
    /// `spacing-increment`: the room each doubling of a duration adds (1.2).
    pub increment: Option<f64>,
    /// `shortest-duration-space`: the room of the commonest shortest
    /// duration, in increments (2).
    pub shortest_space: Option<f64>,
    /// `base-shortest-duration`: the longest duration the shortest is taken
    /// as, in wholes (3/16).
    pub base_shortest: Option<f64>,
}

thread_local! {
    /// The spacing settings read from the source being parsed.
    static SPACING: RefCell<SpacingSpanner> = const { RefCell::new(SpacingSpanner { increment: None, shortest_space: None, base_shortest: None }) };
}

/// `\override [Score.]SpacingSpanner.<prop> = <value>` whose path ends at
/// `j` (the `=`): read into the source's spacing; whether it was one.
fn spacing_override(toks: &[Tok], from: usize, j: usize) -> bool {
    let path = path_words(toks, from, j);
    let n = path.len();
    if n < 2 || path[n - 2] != "SpacingSpanner" {
        return false;
    }
    let mut k = j + 1;
    while is(toks, k, T::Sym('#')) {
        k += 1;
    }
    let value = if is(toks, k, T::Sym('(')) && toks.get(k + 1).is_some_and(|t| t.v == "ly") {
        // `#(ly:make-moment 1/16)`
        toks.get(k + 4).filter(|t| t.t == T::Num).and_then(|t| fraction(&t.v))
    } else {
        number_at(toks, k).map(|(v, _)| v)
    };
    let Some(v) = value.filter(|v| *v > 0.0) else { return false };
    SPACING.with(|sp| {
        let mut sp = sp.borrow_mut();
        match path[n - 1].as_str() {
            "spacing-increment" => sp.increment = Some(v.min(10.0)),
            "shortest-duration-space" => sp.shortest_space = Some(v.min(20.0)),
            "base-shortest-duration" => sp.base_shortest = Some(v.min(1.0)),
            _ => return false,
        }
        true
    })
}

/// `1/16` as a number, or `2` as one.
fn fraction(v: &str) -> Option<f64> {
    match v.split_once('/') {
        Some((a, b)) => {
            let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
            (b > 0.0).then(|| a / b)
        }
        None => v.parse().ok(),
    }
}

/// A page's header or footer: what `\markup \fill-line { … }` puts at the
/// left, in the middle and at the right. One item is centred, two stand at
/// the ends, three take the three places — as LilyPond's `\fill-line`
/// spreads them; a markup with no `\fill-line` stands at the left.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageLine {
    /// At the left.
    pub left: Vec<PagePart>,
    /// In the middle.
    pub centre: Vec<PagePart>,
    /// At the right.
    pub right: Vec<PagePart>,
}

/// A piece of a header or a footer.
#[derive(Debug, Clone, PartialEq)]
pub enum PagePart {
    /// Words, as written.
    Text(String),
    /// `\fromproperty #'header:title`: a field of the header.
    Field(String),
    /// `\fromproperty #'page:page-number-string`: the page's number.
    Page,
    /// `\on-the-fly #first-page …` and its kin: what follows, on the pages
    /// the condition names only.
    Only(PageCond, Vec<PagePart>),
}

/// Which pages an `\on-the-fly` piece is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageCond {
    /// `#first-page`.
    First,
    /// `#not-first-page`, `#print-page-number-check-first`.
    NotFirst,
    /// `#last-page`.
    Last,
    /// `#not-last-page`.
    NotLast,
}

/// Lyrics sung to a voice by its name, attached once every voice is read.
struct Pending {
    voice: String,
    syllables: Vec<Syllable>,
    at: usize,
}

struct Base {
    header: HashMap<String, String>,
    paper: PaperSpec,
    counter: usize,
    notes: usize,
    lang: Lang,
    pending: Vec<Pending>,
    /// `\addQuote "name" { … }`: a part's music that a cue quotes, by name.
    quotes: HashMap<String, Vec<Ev>>,
    /// Inside `\compressMMRests` around staves: each staff read gathers its
    /// runs of bars' rest.
    compress: usize,
    /// The header being read belongs to a movement after the first, not to
    /// the piece: its `piece` and `opus` are the movement's title.
    local: Option<HashMap<String, String>>,
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
    /// The tuplet the current one is inside, when they nest (two deep at most).
    tuplet_outer: Option<TupletState>,
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
    /// The staff's own name, and the one `\\change Staff` sends its notes to.
    own: Option<String>,
    cross: Option<String>,
    /// The notes' size from here (`\small`, `\tiny`, `\teeny`, a cue voice).
    size: i8,
    /// In a cue voice: drawn small, not played.
    cue: bool,
    /// The parts a cue may quote.
    quotes: std::rc::Rc<HashMap<String, Vec<Ev>>>,
    /// Where the `<< >>` being read starts in the staff's time.
    here: i64,
    /// The ossias read in the staff: where each starts, and its music.
    ossias: Vec<(i64, Vec<Ev>)>,
    /// `\stemUp` 1, `\stemDown` -1, else 0.
    stem: i8,
    /// `\autoBeamOff` in force.
    no_autobeam: bool,
    /// `\showStaffSwitch` in force.
    follow: bool,
    /// A `\change Staff` read since the last note, under `\showStaffSwitch`.
    follow_next: bool,
    /// Inside `\crossStaff`.
    span_stem: bool,
    /// The words a text spanner starts with, from `\override
    /// TextSpanner.bound-details.left.text`.
    span_text: Vec<Run>,
    /// The layout settings in force.
    look: crate::look::LookState,
    /// `\scaleDurations`' factor in force, (1, 1) outside one.
    scale: (i64, i64),
    /// The first ossia's `\with`: the ossia staff's settings.
    ossia_props: Option<Props>,
}

#[derive(Clone, Copy)]
struct TupletState {
    id: i64,
    n: i64,
    d: i64,
    span: i64,
    used: i64,
}

pub(crate) fn is(toks: &[Tok], i: usize, t: T) -> bool {
    toks.get(i).is_some_and(|k| k.t == t)
}

pub(crate) fn is_cmd(toks: &[Tok], i: usize, v: &str) -> bool {
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
    const AROUND: [&str; 14] = ["relative", "fixed", "transpose", "new", "context", "sequential", "simultaneous", "chordmode", "drummode", "chords", "drums", "notemode", "figuremode", "figures"];
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
pub(crate) fn scheme_end(toks: &[Tok], i: usize) -> Result<usize, ScoreError> {
    let mut j = i + 1;
    while is(toks, j, T::Sym('#')) || is(toks, j, T::Sym('\'')) || is(toks, j, T::Sym('-')) || is(toks, j, T::Sym('+')) {
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
    // `#ly:optimal-breaking`, `#:roman`: a name with its module.
    while is(toks, j, T::Sym(':')) && is(toks, j + 1, T::Word) && toks[j + 1].at == toks[j].at + 1 {
        j += 2;
    }
    Ok(j)
}

// ---- Settings ---------------------------------------------------------------
//
// What a file says of its layout — `\override`, `\set`, `\tweak`, `\stemUp`,
// a `\with { … }` of a context, `\layout { … }` — that nearly every real file
// writes: read whole, so the music after it reads, and passed over with a
// warning; honoured where the engraver has the hook (`\stemUp`, `\autoBeamOff`,
// a text spanner's words).

/// The commands that only switch a layout setting on or off, passed over.
const SWITCHES: [&str; 96] = [
    "slurUp",
    "slurDown",
    "slurNeutral",
    "slurDashed",
    "slurDotted",
    "slurSolid",
    "slurHalfDashed",
    "slurHalfSolid",
    "phrasingSlurUp",
    "phrasingSlurDown",
    "phrasingSlurNeutral",
    "phrasingSlurDashed",
    "phrasingSlurDotted",
    "phrasingSlurSolid",
    "tieUp",
    "tieDown",
    "tieNeutral",
    "tieDashed",
    "tieDotted",
    "tieSolid",
    "dynamicUp",
    "dynamicDown",
    "dynamicNeutral",
    "tupletUp",
    "tupletDown",
    "tupletNeutral",
    "textSpannerUp",
    "textSpannerDown",
    "textSpannerNeutral",
    "dotsUp",
    "dotsDown",
    "dotsNeutral",
    "arpeggioArrowUp",
    "arpeggioArrowDown",
    "arpeggioNormal",
    "arpeggioBracket",
    "arpeggioParenthesis",
    "arpeggioParenthesisDashed",
    "textLengthOn",
    "textLengthOff",
    "markLengthOn",
    "markLengthOff",
    "hideNotes",
    "unHideNotes",
    "shiftOn",
    "shiftOnn",
    "shiftOnnn",
    "shiftOff",
    "mergeDifferentlyHeadedOn",
    "mergeDifferentlyHeadedOff",
    "mergeDifferentlyDottedOn",
    "mergeDifferentlyDottedOff",
    "numericTimeSignature",
    "defaultTimeSignature",
    "crescTextCresc",
    "dimTextDecresc",
    "dimTextDecr",
    "dimTextDim",
    "crescHairpin",
    "dimHairpin",
    "harmonicsOn",
    "harmonicsOff",
    "easyHeadsOn",
    "easyHeadsOff",
    "improvisationOn",
    "improvisationOff",
    "xNotesOn",
    "xNotesOff",
    "germanChords",
    "semiGermanChords",
    "italianChords",
    "frenchChords",
    "powerChords",
    "showStaffSwitch",
    "hideStaffSwitch",
    "bassFigureExtendersOn",
    "bassFigureExtendersOff",
    "bassFigureStaffAlignmentDown",
    "bassFigureStaffAlignmentUp",
    "bassFigureStaffAlignmentNeutral",
    "autoLineBreaksOn",
    "autoLineBreaksOff",
    "autoPageBreaksOn",
    "autoPageBreaksOff",
    "noBreak",
    "noPageBreak",
    "pageTurn",
    "noPageTurn",
    "allowPageTurn",
    "expandEmptyMeasures",
    "expandFullBarRests",
    "balloonLengthOn",
    "balloonLengthOff",
    "endSpanners",
    "predefinedFretboardsOn",
    "predefinedFretboardsOff",
];

/// Where a property path ends: `Staff.TimeSignature.stencil`, `Stem
/// #'direction`, `#'color`, `TextSpanner.bound-details.left.text`.
pub(crate) fn path_end(toks: &[Tok], i: usize) -> usize {
    let mut j = i;
    let mut want_word = true;
    loop {
        let quoted = is(toks, j, T::Sym('#')) && is(toks, j + 1, T::Sym('\''));
        if quoted {
            j = scheme_end(toks, j).unwrap_or(j + 2);
            want_word = false;
            continue;
        }
        if want_word && is(toks, j, T::Word) {
            j += 1;
            want_word = false;
            continue;
        }
        if !want_word && is(toks, j, T::Sym('.')) && is(toks, j + 1, T::Word) {
            j += 1;
            want_word = true;
            continue;
        }
        break;
    }
    j
}

/// The words of a property path from `i` to `end`: `["Staff", "TimeSignature", "stencil"]`.
fn path_words(toks: &[Tok], i: usize, end: usize) -> Vec<String> {
    toks[i..end.min(toks.len())].iter().filter(|t| t.t == T::Word).map(|t| t.v.clone()).collect()
}

/// The token after a setting's value: Scheme (`#red`, `##t`, `#'(1 . 2)`,
/// `#(…)`), a string, a number and its unit (`2\cm`), a `\markup`, a `{ }`
/// block, a word, or a command (`\default`).
pub(crate) fn value_end(toks: &[Tok], i: usize) -> Result<usize, ScoreError> {
    let Some(t) = toks.get(i) else { return refuse("a setting with no value", at_of(toks, i.saturating_sub(1))) };
    match t.t {
        T::Sym('#') => scheme_end(toks, i),
        T::Sym('{') => skip_block(toks, i),
        T::Str | T::Word | T::Lyrics => Ok(i + 1),
        T::Num | T::Sym('-') => {
            let j = number_at(toks, i).map_or(i + 1, |(_, j)| j);
            let unit = toks.get(j).is_some_and(|u| u.t == T::Cmd && ["cm", "mm", "in", "pt", "bp"].contains(&u.v.as_str()));
            Ok(if unit { j + 1 } else { j })
        }
        T::Cmd if t.v == "markup" || t.v == "markuplist" => markup_skip(toks, i + 1),
        T::Cmd => Ok(i + 1),
        _ => refuse(format!("'{}' is not a setting's value this reads", t.v), t.at),
    }
}

/// The token after one markup item, read leniently — any command, its Scheme
/// arguments and the markup it takes: how a markup this does not draw is
/// passed over.
fn markup_skip(toks: &[Tok], i: usize) -> Result<usize, ScoreError> {
    let Some(t) = toks.get(i) else { return refuse("a \\markup that is never finished", at_of(toks, i.saturating_sub(1))) };
    match t.t {
        T::Sym('{') => skip_block(toks, i),
        T::Sym('#') => scheme_end(toks, i),
        T::Cmd => {
            let mut j = i + 1;
            while is(toks, j, T::Sym('#')) {
                j = scheme_end(toks, j)?;
            }
            // A command that takes markup: the item after it. One that takes
            // none (`\null`) is followed by what is not markup, or the end.
            match toks.get(j).map(|n| n.t) {
                Some(T::Sym('{') | T::Str | T::Cmd) => markup_skip(toks, j),
                _ => Ok(j),
            }
        }
        _ => Ok(i + 1),
    }
}

/// A layout setting at `i`, read whole: `\override path = value`, `\revert
/// path`, `\set path = value`, `\unset path`, `\once` / `\temporary` (and
/// what follows), `\consists "…"` and its kin in a `\with`, `name = value`,
/// a switch (`\slurUp`), or a context's name (`\Staff` in `\context { }`).
/// None when `i` is none of them. `what` says where, for the warning.
fn skip_setting(toks: &[Tok], i: usize, what: &str) -> Result<Option<usize>, ScoreError> {
    let Some(t) = toks.get(i) else { return Ok(None) };
    let named = |j: usize| path_words(toks, i + 1, j).join(".");
    let end = match (t.t, t.v.as_str()) {
        (T::Cmd, "override") => {
            let j = path_end(toks, i + 1);
            if !is(toks, j, T::Sym('=')) {
                return refuse("\\override wants a property, = and a value", t.at);
            }
            if spacing_override(toks, i + 1, j) {
                beyond();
            } else {
                warn(format!("\\override {}{what} is not drawn", named(j)), t.at);
            }
            value_end(toks, j + 1)?
        }
        (T::Cmd, "set") => {
            let j = path_end(toks, i + 1);
            if j == i + 1 || !is(toks, j, T::Sym('=')) {
                return refuse("\\set wants a property, = and a value", t.at);
            }
            warn(format!("\\set {}{what} is not drawn", named(j)), t.at);
            value_end(toks, j + 1)?
        }
        (T::Cmd, "revert" | "unset" | "hide" | "omit" | "undo") => {
            let j = path_end(toks, i + 1);
            warn(format!("\\{} {}{what} is not drawn", t.v, named(j)), t.at);
            j
        }
        (T::Cmd, "once" | "temporary" | "single") => i + 1,
        (T::Cmd, "consists" | "remove" | "accepts" | "denies" | "alias" | "defaultchild" | "name" | "description") => {
            warn(format!("\\{}{what} is not read", t.v), t.at);
            value_end(toks, i + 1)?
        }
        (T::Cmd, w) if SWITCHES.contains(&w) => {
            warn(format!("\\{w}{what} is not drawn"), t.at);
            i + 1
        }
        // A context's name, `\\Staff` in `\\context { \\Staff … }`: only where one is
        // read (`what` names the block), never a variable no one defined.
        (T::Cmd, w) if !what.is_empty() && w.starts_with(|c: char| c.is_ascii_uppercase()) => i + 1,
        (T::Word, _) => {
            let j = path_end(toks, i);
            if !is(toks, j, T::Sym('=')) {
                return Ok(None);
            }
            warn(format!("{}{what} is not read", path_words(toks, i, j).join(".")), t.at);
            value_end(toks, j + 1)?
        }
        _ => return Ok(None),
    };
    beyond();
    Ok(Some(end))
}

// ---- The file ---------------------------------------------------------------

/// Read a source.
pub fn parse(src: &str) -> Result<Parsed, ScoreError> {
    WARNINGS.with(|w| w.borrow_mut().clear());
    SPACING.with(|sp| *sp.borrow_mut() = SpacingSpanner::default());
    crate::look::reset();
    let toks = expand(tokens(src)?)?;
    scan_beyond(&toks);
    let mut st = Base { header: HashMap::new(), paper: PaperSpec::default(), counter: 0, notes: 0, lang: Lang::default(), pending: Vec::new(), quotes: HashMap::new(), compress: 0, local: None };
    // The file's scores, one after another: `\score { … }` blocks, music
    // written without one, inside `\book { }` and `\bookpart { }`.
    let mut movements: Vec<Movement> = Vec::new();
    let mut books: Vec<usize> = Vec::new();
    let mut new_page = false;
    let mut i = 0;
    loop {
        let before = i;
        i = preamble(&toks, i, &mut st)?;
        if i != before && !movements.is_empty() {
            beyond();
        }
        let Some(t) = toks.get(i) else { break };
        if t.t == T::Cmd && (t.v == "book" || t.v == "bookpart") {
            if !is(&toks, i + 1, T::Sym('{')) {
                return refuse(format!("\\{} wants a {{ block }}", t.v), t.at);
            }
            beyond();
            // A book part after the first starts a page; its header titles it.
            if t.v == "bookpart" && !movements.is_empty() {
                new_page = true;
                st.local = Some(HashMap::new());
            }
            books.push(t.at);
            i += 2;
            continue;
        }
        if t.t == T::Sym('}') && !books.is_empty() {
            books.pop();
            i += 1;
            continue;
        }
        if !movements.is_empty() {
            if t.t == T::Sym('}') || !(t.t == T::Cmd || t.t == T::Sym('{') || twin(&toks, i, '<')) {
                return refuse(format!("'{}' after the end of the music", t.v), t.at);
            }
            beyond();
            st.local.get_or_insert_with(HashMap::new);
        }
        let at = t.at;
        let (staves, groups, j) = score(&toks, i, &mut st)?;
        i = j;
        let header = st.local.take().unwrap_or_default();
        movements.push(Movement { staves, groups, header, page: std::mem::take(&mut new_page), at });
    }
    if let Some(&at) = books.last() {
        return refuse("a \\book block that is never closed", at);
    }
    if movements.is_empty() {
        return refuse("there is no music", 0);
    }
    let first = movements.remove(0);
    let (mut staves, groups) = (first.staves, first.groups);
    for (k, mv) in movements.into_iter().enumerate() {
        // Each movement goes on after the one before, on the same staves.
        let same = mv.staves.len() == staves.len() && mv.staves.iter().zip(&staves).all(|(a, b)| a.kind == b.kind);
        if !same {
            warn(format!("movement {} is not engraved: its staves are not the first movement's ({} against {})", k + 2, mv.staves.len(), staves.len()), mv.at);
            continue;
        }
        let get = |name: &str| mv.header.get(name).cloned().unwrap_or_default();
        let piece = if get("piece").is_empty() { get("title") } else { get("piece") };
        let texts = vec![Words::plain(piece, -1), Words::plain(get("opus"), -1)];
        for (dst, src) in staves.iter_mut().zip(mv.staves) {
            dst.events.push(Ev { k: Kind::Movement, page: mv.page, texts: texts.clone(), at: mv.at, ..Ev::default() });
            dst.events.extend(src.events);
            dst.voices.extend(src.voices);
            if src.lyrics.is_some() {
                warn(format!("the lyrics of movement {} are not set: a staff's words are its first movement's", k + 2), mv.at);
            }
        }
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
    crate::look::settle(&mut staves, &groups);
    let mut warnings = WARNINGS.with(|w| std::mem::take(&mut *w.borrow_mut()));
    warnings.sort_by_key(|w| w.at);
    let spacing = SPACING.with(|sp| *sp.borrow());
    Ok(Parsed { header: st.header, paper: st.paper, staves, groups, warnings, spacing })
}

/// A score of the file, read: its staves and groups, its own header (a
/// movement after the first's), whether a book part starts a page with it.
struct Movement {
    staves: Vec<Staff>,
    groups: Vec<Group>,
    header: HashMap<String, String>,
    page: bool,
    at: usize,
}

/// One score from `i`: `\score { … }`, or music written without one; its
/// staves, their groups, and the token after it. The lyrics sung to its
/// voices by name are set on them.
fn score(toks: &[Tok], mut i: usize, st: &mut Base) -> Result<(Vec<Staff>, Vec<Group>, usize), ScoreError> {
    let in_score = is_cmd(toks, i, "score");
    if in_score {
        if !is(toks, i + 1, T::Sym('{')) {
            return refuse("\\score wants a { block }", toks[i].at);
        }
        i = preamble(toks, i + 2, st)?;
    }
    let mut staves = Vec::new();
    let mut groups = Vec::new();
    i = part(toks, i, st, &mut staves, &mut groups, 0)?;
    i = attach_lyrics(toks, i, &mut staves)?;
    if in_score {
        while i < toks.len() && toks[i].t == T::Cmd && ["layout", "midi", "header"].contains(&toks[i].v.as_str()) {
            if toks[i].v == "header" {
                i = header(toks, i, st)?;
            } else {
                i = layout_block(toks, i, &mut st.paper)?;
            }
        }
        if i >= toks.len() || toks[i].t != T::Sym('}') {
            return refuse("a \\score block that is never closed", 0);
        }
        i += 1;
    }
    for p in std::mem::take(&mut st.pending) {
        let found = staves.iter().position(|s| s.voices.iter().any(|v| v.0 == p.voice));
        let Some(si) = found else {
            return refuse(format!("\\lyricsto \"{}\" names no voice: a \\new Voice = \"{}\" is what it sings to", p.voice, p.voice), p.at);
        };
        let staff = &mut staves[si];
        let voice = staff.voices.iter().find(|v| v.0 == p.voice).map_or(1, |v| v.1);
        add_verse(staff, p.syllables, voice, p.at)?;
    }
    Ok((staves, groups, i))
}

/// What a file says before and after its music: `\version`, `\header`,
/// `\language`, `\paper`, `\layout` and `\midi` blocks, Scheme, and the
/// switches nothing drawn depends on.
fn preamble(toks: &[Tok], mut i: usize, st: &mut Base) -> Result<usize, ScoreError> {
    while i < toks.len() {
        let t = &toks[i];
        if t.t == T::Sym('#') {
            beyond();
            scheme_paper(toks, i, &mut st.paper);
            i = scheme_end(toks, i)?;
            continue;
        }
        if t.t != T::Cmd {
            break;
        }
        match t.v.as_str() {
            "version" => i += 2,
            "header" => i = header(toks, i, st)?,
            "paper" => {
                beyond();
                i = paper_block(toks, i + 1, &mut st.paper)?;
            }
            "layout" | "midi" => {
                beyond();
                i = layout_block(toks, i, &mut st.paper)?;
            }
            "markup" | "markuplist" => {
                // Words between the scores — a title page, a dedication: not set.
                beyond();
                warn(format!("\\{} outside the music is not drawn", t.v), t.at);
                i = markup_skip(toks, i + 1)?;
            }
            "pointAndClickOff" | "pointAndClickOn" => i += 1,
            "language" => {
                let Some(name) = toks.get(i + 1).filter(|n| n.t == T::Str) else { return refuse("\\language wants a language's name, in quotes", t.at) };
                st.lang = Lang::named(&name.v).ok_or_else(|| ScoreError {
                    message: format!("score: the language \"{}\" is not one this reads (nederlands, english, deutsch, italiano, français, español) (at character {})", name.v, name.at),
                })?;
                i += 2;
            }
            "addQuote" => {
                // A part's music a cue may quote, read as a staff's and kept by name.
                beyond();
                let Some(name) = toks.get(i + 1).filter(|n| n.t == T::Str).map(|n| n.v.clone()) else { return refuse("\\addQuote wants the quote's name, in quotes, then its music", t.at) };
                if !starts_expr(toks, i + 2) && !is(toks, i + 2, T::Word) {
                    return refuse("\\addQuote wants the quote's name, in quotes, then its music", t.at);
                }
                let (counter, notes) = (st.counter, st.notes);
                let mut read = Vec::new();
                i = staff_named(toks, i + 2, st, &mut read, Props::default(), StaffKind::Notes, None)?;
                (st.counter, st.notes) = (counter, notes);
                st.quotes.insert(name, read.pop().map(|s| s.events).unwrap_or_default());
            }
            "include" => {
                let Some(name) = toks.get(i + 1).filter(|n| n.t == T::Str) else { return refuse("\\include wants a file's name, in quotes", t.at) };
                match name.v.strip_suffix(".ly").and_then(Lang::named) {
                    Some(lang) => st.lang = lang,
                    None => {
                        // A score reads one source: what another file defines is not here.
                        beyond();
                        warn(format!("\\include \"{}\" is not read: a score reads one source", name.v), t.at);
                    }
                }
                i += 2;
            }
            _ => break,
        }
    }
    Ok(i)
}

/// `#(set-paper-size "letter")` or `#(set-global-staff-size 18)` at `i` (a
/// `#`), into `paper`; any other Scheme is left to be skipped.
fn scheme_paper(toks: &[Tok], i: usize, paper: &mut PaperSpec) {
    if !is(toks, i + 1, T::Sym('(')) {
        return;
    }
    let Some(name) = toks.get(i + 2).filter(|t| t.t == T::Word) else { return };
    match name.v.as_str() {
        "set-paper-size" => {
            if let Some(size) = toks.get(i + 3).filter(|t| t.t == T::Str) {
                paper.size = Some(size.v.to_lowercase());
            }
        }
        "set-global-staff-size" => paper.staff_size = number_at(toks, i + 3).map(|(n, _)| n).filter(|n| (8.0..=40.0).contains(n)),
        _ => {}
    }
}

/// A number at `i` — `15`, `-2`, `17.5` — and where it ends.
fn number_at(toks: &[Tok], mut i: usize) -> Option<(f64, usize)> {
    let negative = is(toks, i, T::Sym('-'));
    if negative {
        i += 1;
    }
    let whole = toks.get(i).filter(|t| t.t == T::Num)?;
    let mut text = whole.v.clone();
    i += 1;
    if is(toks, i, T::Sym('.')) && is(toks, i + 1, T::Num) {
        text = format!("{text}.{}", toks[i + 1].v);
        i += 2;
    }
    let n: f64 = text.parse().ok()?;
    Some((if negative { -n } else { n }, i))
}

/// `##t`, `#t`, `##f`, `#f` at `i`: the boolean, and where it ends.
fn boolean_at(toks: &[Tok], mut i: usize) -> Option<(bool, usize)> {
    let mut hashes = 0;
    while is(toks, i, T::Sym('#')) {
        hashes += 1;
        i += 1;
    }
    let w = toks.get(i).filter(|w| w.t == T::Word && hashes > 0)?;
    match w.v.as_str() {
        "t" | "true" => Some((true, i + 1)),
        "f" | "false" => Some((false, i + 1)),
        _ => None,
    }
}

/// A length's unit after its number `n` at `after`: `\mm` (the default),
/// `\cm`, `\in`, `\pt`, `\staff-space` (at the staff size in force); the
/// length in millimetres and where it ends. `* 2` after it multiplies.
fn length_unit(toks: &[Tok], n: f64, after: usize, staff_size: Option<f64>) -> (f64, usize) {
    let space_mm = staff_size.unwrap_or(20.0) / 4.0 * 25.4 / 72.27;
    let unit = toks.get(after).filter(|u| u.t == T::Cmd).map(|u| u.v.as_str());
    let (mm, end) = match unit {
        Some("mm") => (n, after + 1),
        Some("cm") => (n * 10.0, after + 1),
        Some("in") => (n * 25.4, after + 1),
        Some("pt") => (n * 25.4 / 72.27, after + 1),
        Some("staff-space") => (n * space_mm, after + 1),
        Some("staff") if is(toks, after + 1, T::Sym('-')) && toks.get(after + 2).is_some_and(|w| w.v == "space") => (n * space_mm, after + 3),
        _ => (n, after),
    };
    if is(toks, end, T::Sym('*')) {
        if let Some((k, e)) = number_at(toks, end + 1) {
            return (mm * k, e);
        }
    }
    (mm, end)
}

/// `system-system-spacing.basic-distance = #12`, or `system-system-spacing
/// = #'((basic-distance . 12) (padding . 1))`, at `i`: read into `paper`
/// (in staff spaces); where it ends, or none for anything else.
fn spacing_setting(toks: &[Tok], i: usize, paper: &mut PaperSpec) -> Option<usize> {
    let t = toks.get(i).filter(|t| t.t == T::Word)?;
    let slot = match t.v.as_str() {
        "system-system-spacing" => &mut paper.system_spacing,
        "score-system-spacing" => &mut paper.score_spacing,
        "markup-system-spacing" => &mut paper.markup_spacing,
        _ => return None,
    };
    let put = |slot: &mut Spacing, key: &str, v: f64| match key {
        "basic-distance" => slot.basic = Some(v.clamp(0.0, 100.0)),
        "minimum-distance" => slot.minimum = Some(v.clamp(0.0, 100.0)),
        "padding" => slot.padding = Some(v.clamp(-10.0, 100.0)),
        "stretchability" => slot.stretch = Some(v.clamp(0.0, 1e6)),
        _ => {}
    };
    if is(toks, i + 1, T::Sym('.')) && is(toks, i + 3, T::Sym('=')) {
        let key = toks.get(i + 2)?.v.clone();
        let mut k = i + 4;
        while is(toks, k, T::Sym('#')) {
            k += 1;
        }
        let (v, end) = number_at(toks, k)?;
        put(slot, &key, v);
        return Some(end);
    }
    if !is(toks, i + 1, T::Sym('=')) {
        return None;
    }
    let mut k = i + 2;
    while is(toks, k, T::Sym('#')) || is(toks, k, T::Sym('\'')) {
        k += 1;
    }
    if !is(toks, k, T::Sym('(')) {
        return None;
    }
    let end = balanced(toks, k, '(', ')').ok()?;
    // `(key . value)` pairs.
    for p in k + 1..end.saturating_sub(3) {
        if is(toks, p, T::Sym('(')) && is(toks, p + 2, T::Sym('.')) {
            if let Some((v, _)) = number_at(toks, p + 3) {
                let key = toks[p + 1].v.clone();
                put(slot, &key, v);
            }
        }
    }
    Some(end)
}

/// A `\paper { … }` block from `i` (its `{`), its sizes into `paper`;
/// what it says that this does not lay out is passed over.
fn paper_block(toks: &[Tok], i: usize, paper: &mut PaperSpec) -> Result<usize, ScoreError> {
    let end = skip_block(toks, i)?;
    let mut j = i + 1;
    while j + 1 < end {
        let t = &toks[j];
        if t.t == T::Sym('#') {
            scheme_paper(toks, j, paper);
            j = scheme_end(toks, j)?;
            continue;
        }
        // `oddHeaderMarkup = \markup \fill-line { … }`: a page's header or footer.
        if t.t == T::Word && t.v.ends_with("Markup") && is(toks, j + 1, T::Sym('=')) && is_cmd(toks, j + 2, "markup") {
            let slot = match t.v.as_str() {
                "oddHeaderMarkup" => Some(&mut paper.odd_header),
                "evenHeaderMarkup" => Some(&mut paper.even_header),
                "oddFooterMarkup" => Some(&mut paper.odd_footer),
                "evenFooterMarkup" => Some(&mut paper.even_footer),
                _ => None,
            };
            if let Some(slot) = slot {
                let (line, after) = page_markup(toks, j + 3)?;
                *slot = Some(line);
                j = after;
                continue;
            }
        }
        // `page-breaking = #ly:optimal-breaking`: the lines spread over the pages.
        if t.t == T::Word && t.v == "page-breaking" && is(toks, j + 1, T::Sym('=')) && is(toks, j + 2, T::Sym('#')) {
            let name = toks.get(j + 5).filter(|w| w.t == T::Word && toks[j + 3].v == "ly" && is(toks, j + 4, T::Sym(':')));
            match name.map(|w| w.v.as_str()) {
                Some("optimal-breaking") => paper.even_pages = Some(true),
                Some("minimal-breaking" | "page-turn-breaking" | "one-page-breaking" | "one-line-breaking") => paper.even_pages = Some(false),
                _ => {}
            }
            j += 6;
            continue;
        }
        // `system-system-spacing.basic-distance = #12`, or the whole alist.
        if let Some(after) = spacing_setting(toks, j, paper) {
            j = after;
            continue;
        }
        if t.t == T::Word && is(toks, j + 1, T::Sym('=')) {
            // `ragged-right = ##t`.
            if let Some((yes, after)) = boolean_at(toks, j + 2) {
                let slot = match t.v.as_str() {
                    "ragged-right" => Some(&mut paper.ragged_right),
                    "ragged-last" => Some(&mut paper.ragged_last),
                    "ragged-bottom" => Some(&mut paper.ragged_bottom),
                    "ragged-last-bottom" => Some(&mut paper.ragged_last_bottom),
                    _ => None,
                };
                if let Some(slot) = slot {
                    *slot = Some(yes);
                    j = after;
                    continue;
                }
            }
            let at_number = if is(toks, j + 2, T::Sym('#')) { j + 3 } else { j + 2 };
            if let Some((n, after)) = number_at(toks, at_number) {
                // A length: millimetres unless a unit says otherwise.
                let (mm, after) = length_unit(toks, n, after, paper.staff_size);
                let slot = match t.v.as_str() {
                    "left-margin" => Some(&mut paper.left),
                    "right-margin" => Some(&mut paper.right),
                    "top-margin" => Some(&mut paper.top),
                    "bottom-margin" => Some(&mut paper.bottom),
                    "indent" => Some(&mut paper.indent),
                    "line-width" => Some(&mut paper.line_width),
                    "short-indent" => Some(&mut paper.short_indent),
                    "systems-per-page" | "max-systems-per-page" | "page-count" | "system-count" => {
                        // A count, not a length.
                        let count = (n >= 1.0).then(|| n.min(if t.v == "system-count" { 10000.0 } else { 100.0 }) as usize);
                        match t.v.as_str() {
                            "systems-per-page" => paper.systems_per_page = count,
                            "max-systems-per-page" => paper.max_systems_per_page = count,
                            "page-count" => paper.page_count = count,
                            _ => paper.system_count = count,
                        }
                        j = after;
                        continue;
                    }
                    _ => None,
                };
                match slot {
                    Some(slot) => *slot = Some(mm.max(0.0)),
                    None => warn(format!("\\paper's {} is not read", t.v), t.at),
                }
                j = after;
                continue;
            }
            // `ragged-right = ##t`, `print-page-number = ##f`: passed over.
            if !(j > 0 && is(toks, j - 1, T::Sym('.'))) {
                warn(format!("\\paper's {} is not read", t.v), t.at);
            }
            j = value_end(toks, j + 2).unwrap_or(j + 2);
            continue;
        }
        j += 1;
    }
    Ok(end)
}

/// `\\layout { … }` or `\\midi { … }` at `i`: passed over, with a warning
/// when it says anything.
fn layout_block(toks: &[Tok], i: usize, paper: &mut PaperSpec) -> Result<usize, ScoreError> {
    let end = skip_block(toks, i + 1)?;
    if toks[i].v != "layout" {
        if end > i + 3 {
            warn(format!("the \\{} block's settings are not read", toks[i].v), toks[i].at);
        }
        return Ok(end);
    }
    // What a `\layout` block may say of the lines — `ragged-right`,
    // `ragged-last`, `indent`, `short-indent`, `line-width` — and the
    // spacing of durations (`\override Score.SpacingSpanner …`) is read;
    // in a `\context { \Staff \RemoveEmptyStaves \override … }`, what the
    // engraver draws is honoured for that context (score #403); anything
    // else is passed over, with a warning.
    let mut unread = false;
    let mut j = i + 2;
    while j + 1 < end {
        let t = &toks[j];
        if is_cmd(toks, j, "enablePolymeter") {
            beyond();
            paper.polymeter = true;
            j += 1;
            continue;
        }
        if is_cmd(toks, j, "context") && is(toks, j + 1, T::Sym('{')) {
            let close = skip_block(toks, j + 1)? - 1;
            let mut k = j + 2;
            let mut name = String::new();
            if toks.get(k).is_some_and(|t| t.t == T::Cmd && t.v.starts_with(|c: char| c.is_ascii_uppercase())) {
                name = toks[k].v.clone();
                k += 1;
            }
            let mut map = crate::look::Map::new();
            while k < close {
                // LilyPond's polymetre the old way: the Timing_translator (and
                // the bar lines' engraver) taken from the \Score and given to each
                // \Staff, which then keeps its own metre.
                if (is_cmd(toks, k, "remove") || is_cmd(toks, k, "consists")) && toks.get(k + 1).is_some_and(|a| a.t == T::Str && (a.v == "Timing_translator" || a.v == "Default_bar_line_engraver")) {
                    beyond();
                    if toks[k + 1].v == "Timing_translator" && toks[k].v == "consists" && name != "Score" {
                        paper.polymeter = true;
                    }
                    k += 2;
                    continue;
                }
                if is_cmd(toks, k, "override") {
                    let m = path_end(toks, k + 1);
                    if is(toks, m, T::Sym('=')) && spacing_override(toks, k + 1, m) {
                        beyond();
                        k = value_end(toks, m + 1)?;
                        continue;
                    }
                }
                if let Some(next) = crate::look::context_setting(toks, k, &mut map)? {
                    k = next;
                    continue;
                }
                match skip_setting(toks, k, " in \\layout")? {
                    Some(next) => k = next.max(k + 1),
                    None => k += 1,
                }
            }
            crate::look::layout_context(&name, map);
            j = close + 1;
            continue;
        }
        if t.t == T::Cmd && t.v == "override" {
            let k = path_end(toks, j + 1);
            if is(toks, k, T::Sym('=')) && spacing_override(toks, j + 1, k) {
                beyond();
                j = value_end(toks, k + 1)?;
                continue;
            }
        }
        if t.t == T::Word && is(toks, j + 1, T::Sym('=')) {
            if let Some((yes, after)) = boolean_at(toks, j + 2).filter(|_| matches!(t.v.as_str(), "ragged-right" | "ragged-last")) {
                *(if t.v == "ragged-right" { &mut paper.ragged_right } else { &mut paper.ragged_last }) = Some(yes);
                j = after;
                continue;
            }
            let at_number = if is(toks, j + 2, T::Sym('#')) { j + 3 } else { j + 2 };
            if let Some((n, after)) = number_at(toks, at_number).filter(|_| matches!(t.v.as_str(), "indent" | "short-indent" | "line-width")) {
                let (mm, after) = length_unit(toks, n, after, paper.staff_size);
                let slot = match t.v.as_str() {
                    "indent" => &mut paper.indent,
                    "short-indent" => &mut paper.short_indent,
                    _ => &mut paper.line_width,
                };
                *slot = Some(mm.max(0.0));
                j = after;
                continue;
            }
        }
        if !matches!(t.t, T::Sym('{' | '}')) {
            unread = true;
        }
        j += 1;
    }
    if unread {
        warn("the \\layout block's settings are not read", toks[i].at);
    }
    Ok(end)
}

pub(crate) fn skip_block(toks: &[Tok], mut i: usize) -> Result<usize, ScoreError> {
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
        "FiguredBass" => StaffKind::Figures,
        "FretBoards" => StaffKind::FretBoards,
        "DrumStaff" => StaffKind::Drums,
        "TabStaff" => StaffKind::Tab,
        _ => return None,
    })
}

/// A context a staff's music is sung in.
fn voice_kind(kind: &str) -> bool {
    matches!(kind, "Voice" | "DrumVoice" | "TabVoice" | "CueVoice")
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
            T::Cmd if depth == 0 && (toks[j].v == "chords" || toks[j].v == "drums" || toks[j].v == "figures") => {
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
    if tk.t == T::Cmd && tk.v == "compressMMRests" && (is_cmd(toks, i + 1, "new") || is_cmd(toks, i + 1, "context") || double(toks, i + 1, '<')) {
        // Around staves: every staff in it gathers its runs of bars' rest.
        beyond();
        st.compress += 1;
        let read = part(toks, i + 1, st, staves, groups, depth);
        st.compress -= 1;
        return read;
    }
    if tk.t == T::Cmd && (tk.v == "chords" || tk.v == "drums" || tk.v == "figures") {
        // `\chords { … }` is `\new ChordNames \chordmode { … }`, `\drums`
        // the same of a drum staff, `\figures` of a figured bass.
        let kind = match tk.v.as_str() {
            "chords" => StaffKind::ChordNames,
            "figures" => StaffKind::Figures,
            _ => StaffKind::Drums,
        };
        return staff(toks, i + 1, st, staves, Props::default(), kind);
    }
    if tk.t == T::Cmd && (tk.v == "new" || tk.v == "context") {
        let kind = toks.get(i + 1).map(|t| t.v.clone()).unwrap_or_default();
        if voice_kind(&kind) {
            return staff(toks, i, st, staves, Props::default(), StaffKind::Notes);
        }
        if staff_kind(&kind).is_none() && group_kind(&kind).is_none() {
            return refuse(
                format!("\\new {kind} is not drawn: a Staff, a ChordNames, a FiguredBass, a FretBoards, a DrumStaff, a TabStaff, a PianoStaff, a GrandStaff, a StaffGroup or a ChoirStaff is"),
                tk.at,
            );
        }
        i += 2;
        let mut name = None;
        if i + 1 < toks.len() && toks[i].t == T::Sym('=') && toks[i + 1].t == T::Str {
            name = Some(toks[i + 1].v.clone());
            i += 2;
        }
        let mut props = Props::default();
        if is_cmd(toks, i, "with") {
            i = with(toks, i, &mut props)?;
        }
        if let Some(sk) = staff_kind(&kind) {
            return staff_named(toks, i, st, staves, props, sk, name);
        }
        if is_cmd(toks, i, "autoChange") || is_cmd(toks, i, "autochange") {
            // `\new PianoStaff \autoChange { … }`: one voice over two staves.
            let first = staves.len();
            let j = auto_change(toks, i, st, staves)?;
            let gk = group_kind(&kind).unwrap_or(GroupKind::None);
            groups.push(Group {
                kind: gk,
                first,
                last: staves.len() - 1,
                depth,
                name: props.instrument_name.clone().unwrap_or_default(),
                short: props.short_instrument_name.clone().unwrap_or_default(),
            });
            return Ok(j);
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
    if tk.t == T::Cmd && (tk.v == "autoChange" || tk.v == "autochange") {
        let first = staves.len();
        let j = auto_change(toks, i, st, staves)?;
        groups.push(Group { kind: GroupKind::None, first, last: staves.len() - 1, depth, name: String::new(), short: String::new() });
        return Ok(j);
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
        // A setting between the staves, `\\set Score.… = …`: passed over.
        if let Some(j) = skip_setting(toks, i, "")? {
            i = j;
            continue;
        }
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
    // The group's `\with`, under each staff's own.
    for stf in &mut staves[first..] {
        let own = std::mem::replace(&mut stf.props.look, props.look.clone());
        stf.props.look.extend(own);
    }
    groups.push(Group { kind, first, last: staves.len() - 1, depth, name: props.instrument_name.clone().unwrap_or_default(), short: props.short_instrument_name.clone().unwrap_or_default() });
    Ok(i + 2)
}

/// `\autoChange [pitch] music` at `i`: LilyPond's automatic staff
/// changes. The music is read as one voice on a staff named "up", over a
/// staff named "down" in the bass clef; each note under the pitch (middle
/// C when none is written) — a chord by its lowest note — is drawn on the
/// lower staff, and goes there as `\change Staff = "down"` sends it, a rest
/// left in its place. The lower staff rests wherever the upper one plays,
/// and both rest where the music does. The notes sound once, from "up".
fn auto_change(toks: &[Tok], i: usize, st: &mut Base, staves: &mut Vec<Staff>) -> Result<usize, ScoreError> {
    let at = toks[i].at;
    beyond();
    let mut j = i + 1;
    let mut split = semitones(7, 0);
    if let Some(t) = toks.get(j).filter(|t| t.t == T::Word && starts_expr(toks, j + 1)) {
        let Some((step, alter)) = pitch_name_in(st.lang, &t.v) else { return refuse("\\autoChange wants a pitch, then its music", t.at) };
        split = semitones(step + 7 * t.marks, alter);
        j += 1;
    }
    if !starts_expr(toks, j) && !is(toks, j, T::Word) {
        return refuse("\\autoChange wants its music", at);
    }
    let mut read = Vec::new();
    j = staff_named(toks, j, st, &mut read, Props::default(), StaffKind::Notes, Some("up".into()))?;
    let Some(mut up) = read.pop().filter(|s| s.kind == StaffKind::Notes) else { return refuse("\\autoChange wants its music", at) };
    let mut down = vec![Ev { k: Kind::Clef, clef: Clef { sign: Sign::F, line: 2, octave: 0 }, at, ..Ev::default() }];
    for ev in &mut up.events {
        match ev.k {
            Kind::Voices | Kind::VoiceNext | Kind::VoicesEnd => return refuse("\\autoChange splits one voice between the staves: two voices in it are not drawn", ev.at),
            Kind::Note | Kind::Rest => {
                if ev.cross.is_some() {
                    return refuse("\\change Staff inside \\autoChange: the notes are already sent from one staff to the other", ev.at);
                }
                let low = ev.pitches.iter().map(|p| semitones(p.d, p.alter)).min();
                let below = ev.k == Kind::Note && low.is_some_and(|l| l < split);
                if below {
                    ev.cross = Some("down".into());
                    ev.rest_behind = true;
                }
                down.push(Ev {
                    k: Kind::Rest,
                    base: ev.base,
                    dots: ev.dots,
                    ticks: ev.ticks,
                    full: ev.full,
                    count: ev.count,
                    tuplet: ev.tuplet,
                    tuplet_outer: ev.tuplet_outer,
                    spacer: below || ev.spacer,
                    i: st.counter,
                    at: ev.at,
                    ..Ev::default()
                });
                st.counter += 1;
            }
            Kind::Clef | Kind::Tempo | Kind::Mark | Kind::Ottava => {}
            _ => down.push(Ev { at: ev.at, ..ev.clone() }),
        }
    }
    staves.push(up);
    staves.push(Staff { kind: StaffKind::Notes, events: down, props: Props::default(), lyrics: None, verses: Vec::new(), lyrics_voice: 1, voices: Vec::new(), name: Some("down".into()) });
    Ok(j)
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
            add_verse(last, syllables, 1, at)?;
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
        let known = toks[i].t == T::Word && STAFF_PROPS.contains(&toks[i].v.as_str()) && is(toks, i + 1, T::Sym('='));
        if known {
            i = staff_prop(toks, i, props, "\\with")?;
            continue;
        }
        // What the engraver draws — `\RemoveEmptyStaves`, `\override
        // TimeSignature.style`, `fontSize = #-2` — is honoured for the
        // context; what else it says (`\consists` …) is read and passed over.
        if let Some(j) = crate::look::context_setting(toks, i, &mut props.look)? {
            beyond();
            i = j;
            continue;
        }
        // `\remove "Time_signature_engraver"`: a staff with no metre drawn,
        // as a cue or an ossia staff often is.
        if is_cmd(toks, i, "remove") && toks.get(i + 1).is_some_and(|a| matches!(a.t, T::Str | T::Word) && a.v == "Time_signature_engraver") {
            beyond();
            props.look.insert("TimeSignature.stencil".into(), crate::look::Val::Bool(false));
            i += 2;
            continue;
        }
        match skip_setting(toks, i, " in \\with")? {
            Some(j) => i = j,
            None => return staff_prop(toks, i, props, "\\with"),
        }
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
    // `instrumentName = \markup { … }`: its words.
    if is_cmd(toks, i, "markup") {
        let (runs, j) = markup(toks, i + 1, toks[i].at)?;
        let value = runs.iter().filter(|r| !r.glyph).map(|r| r.text.as_str()).collect::<Vec<_>>().join(" ");
        beyond();
        match tk.v.as_str() {
            "instrumentName" => props.instrument_name = Some(value),
            "shortInstrumentName" => props.short_instrument_name = Some(value),
            _ => props.midi_instrument = Some(value),
        }
        return Ok(j);
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

fn staff(toks: &[Tok], i: usize, base: &mut Base, staves: &mut Vec<Staff>, props: Props, kind: StaffKind) -> Result<usize, ScoreError> {
    staff_named(toks, i, base, staves, props, kind, None)
}

fn staff_named(toks: &[Tok], mut i: usize, base: &mut Base, staves: &mut Vec<Staff>, props: Props, kind: StaffKind, name: Option<String>) -> Result<usize, ScoreError> {
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
        tuplet_outer: None,
        chord: None,
        voice_cmd: None,
        voice_name: None,
        voice_names: Vec::new(),
        in_part: false,
        percents: 0,
        marks: 0,
        graces: Vec::new(),
        mode: match kind {
            StaffKind::ChordNames | StaffKind::FretBoards => Mode::Chords,
            StaffKind::Figures => Mode::Figures,
            StaffKind::Drums => Mode::Drums,
            _ => Mode::Notes,
        },
        own: name.clone(),
        cross: None,
        size: 0,
        cue: false,
        quotes: std::rc::Rc::new(base.quotes.clone()),
        here: 0,
        ossias: Vec::new(),
        stem: 0,
        no_autobeam: false,
        follow: false,
        follow_next: false,
        span_stem: false,
        span_text: Vec::new(),
        look: crate::look::LookState::default(),
        scale: (1, 1),
        ossia_props: None,
    };
    if base.compress > 0 {
        st.events.push(Ev { k: Kind::Compress, at: toks.get(i).map_or(0, |t| t.at), ..Ev::default() });
    }
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
    if !st.ossias.is_empty() {
        let events = ossia_events(&st.events, std::mem::take(&mut st.ossias))?;
        staves.push(Staff { kind: StaffKind::Ossia, events, props: st.ossia_props.take().unwrap_or_default(), lyrics: None, verses: Vec::new(), lyrics_voice: 1, voices: Vec::new(), name: None });
    }
    if kind == StaffKind::FretBoards {
        // Each chord's shape on the guitar, its lowest note the bass; a rest shows none.
        for ev in &mut st.events {
            ev.chord_name = match (ev.k == Kind::Note, ev.pitches.first()) {
                (true, Some(bass)) => {
                    let pc = |p: &Pitch| crate::midi::key_of(p.d, p.alter);
                    let classes: Vec<i64> = ev.pitches.iter().map(pc).collect();
                    crate::frets::voicing(&classes, pc(bass)).map(|d| vec![Run::words(format!("{}{}", crate::frets::FRET, d.spec()), Weight::Regular)]).unwrap_or_default()
                }
                _ => Vec::new(),
            };
        }
    }
    staves.push(Staff { kind, events: st.events, props: st.props, lyrics: None, verses: Vec::new(), lyrics_voice: 1, voices: st.voice_names, name });
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
            i = value_end(toks, i + 2)?;
            continue;
        }
        let ok = name.t == T::Word && i + 2 < toks.len() && toks[i + 1].t == T::Sym('=');
        if !ok {
            return refuse("a header field is a name, = and a string", name.at);
        }
        // A field no title prints — `encodingsoftware`, `copyright` set
        // twice — is passed over; a `\markup` value is its words.
        let value = match &toks[i + 2] {
            v if v.t == T::Str => Some((v.v.clone(), i + 3)),
            v if v.t == T::Cmd && v.v == "markup" => {
                beyond();
                match markup(toks, i + 3, v.at) {
                    Ok((runs, j)) => Some((runs.iter().filter(|r| !r.glyph).map(|r| r.text.as_str()).collect::<Vec<_>>().join(" "), j)),
                    Err(_) => None,
                }
            }
            _ => None,
        };
        let known = fields.contains(&name.v.as_str());
        match value {
            Some((text, j)) if known => {
                match st.local.as_mut() {
                    Some(local) => local.insert(name.v.clone(), text),
                    None => st.header.insert(name.v.clone(), text),
                };
                i = j;
            }
            _ => {
                beyond();
                warn(if known { format!("the header field '{}' is not words this prints", name.v) } else { format!("the header field '{}' is not one a title page prints", name.v) }, name.at);
                i = value_end(toks, i + 2)?;
            }
        }
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
        // A setting written here — a clef, a metre, a mark — takes the
        // layout settings in force, as a note does.
        let before = st.events.len();
        let read = music_item(toks, i, st, hd, &tk);
        for ev in st.events.iter_mut().skip(before) {
            if ev.look.is_empty() && !matches!(ev.k, Kind::Note | Kind::Rest) {
                ev.look = st.look.current();
            }
        }
        match read? {
            Some(j) => i = j,
            None => break,
        }
    }
    Ok(i)
}

/// One item of [`music`] at `i`: the token after it, none at its `}`.
fn music_item(toks: &[Tok], mut i: usize, st: &mut St, hd: &mut HashMap<String, String>, tk: &Tok) -> Result<Option<usize>, ScoreError> {
    {
        let tk = tk.clone();
        match tk.t {
            T::Sym('}') => return Ok(None),
            T::Sym('{') => {
                beyond();
                i = expr(toks, i, st, hd)?;
            }
            T::Cmd => i = command(toks, i, st, hd)?,
            T::Sym('<') if twin(toks, i, '<') => i = voices(toks, i, st, hd)?,
            T::Word | T::Sym('<') => i = note(toks, i, st)?,
            T::Sym('|') => i += 1,
            T::Sym('~' | '-' | '^' | '_' | '(' | ')' | '[' | ']') => i = post_sign(toks, i, st, hd)?,
            _ => return refuse(format!("'{}' is not expected here", tk.v), tk.at),
        }
    }
    Ok(Some(i))
}

/// A sign written after a note, for it: `~`, `(` `)`, `[` `]`, or `-` `^`
/// `_` and what follows them.
fn post_sign(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let tk = toks[i].clone();
    match tk.t {
        T::Sym('-' | '^' | '_') => postfix(toks, i, st, hd),
        T::Sym(c) => {
            let last = last_note(&mut st.events, &tk)?;
            match c {
                '~' => last.tie = true,
                '(' => last.slur_on = true,
                ')' => last.slur_off = true,
                '[' => last.beam_on = true,
                _ => last.beam_off = true,
            }
            Ok(i + 1)
        }
        _ => refuse(format!("'{}' is not expected here", tk.v), tk.at),
    }
}

/// Whether the token at `i` is written after a note, for it — a tie, a
/// slur, a beam, an articulation, a fingering, words, a dynamic, a hairpin,
/// a phrasing slur — rather than music of its own.
fn post_event(toks: &[Tok], i: usize) -> bool {
    let Some(t) = toks.get(i) else { return false };
    match t.t {
        T::Sym('~' | '-' | '^' | '_' | '(' | ')' | '[' | ']') => true,
        T::Cmd => {
            let w = t.v.as_str();
            DYNAMICS.contains(&w)
                || SCRIPTS.contains(&w)
                || (!w.is_empty() && w.chars().all(|c| c.is_ascii_digit()))
                || [
                    "<",
                    ">",
                    "!",
                    "(",
                    ")",
                    "arpeggio",
                    "glissando",
                    "sustainOn",
                    "sustainOff",
                    "startTrillSpan",
                    "stopTrillSpan",
                    "laissezVibrer",
                    "repeatTie",
                    "cresc",
                    "dim",
                    "decresc",
                    "startTextSpan",
                    "stopTextSpan",
                ]
                .contains(&w)
        }
        _ => false,
    }
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

/// `\new Staff \with { alignAboveContext = "main" } { … }` beside a staff's
/// music in `<< >>`: an ossia, kept to be set over the staff.
fn ossia(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let at = toks[i].at;
    let mut j = i + 2;
    if is(toks, j, T::Sym('=')) && is(toks, j + 1, T::Str) {
        j += 2;
    }
    let mut above = false;
    let mut props = Props::default();
    if is_cmd(toks, j, "with") {
        let end = skip_block(toks, j + 1)?;
        above = toks[j + 1..end].iter().any(|t| t.t == T::Word && t.v == "alignAboveContext");
        if above {
            // What it says of its size and its metre (score #407): the
            // rest is the ossia's own business — where it goes, its first
            // clef, which an ossia never draws.
            ossia_with(toks, j, &mut props)?;
        }
        j = end;
    }
    if !above {
        return refuse("a staff inside a staff's music is an ossia: \\new Staff \\with { alignAboveContext = … }", at);
    }
    if !starts_expr(toks, j) {
        return refuse("an ossia wants its music", at);
    }
    beyond();
    let outer = std::mem::take(&mut st.events);
    let outer_part = st.in_part;
    st.in_part = false;
    let start = st.here + elapsed(&outer);
    j = expr(toks, j, st, hd)?;
    st.in_part = outer_part;
    let music = std::mem::replace(&mut st.events, outer);
    st.ossias.push((start, music));
    if st.ossia_props.is_none() {
        st.ossia_props = Some(props);
    }
    Ok(j)
}

/// An ossia's `\with`: its size and the settings drawn honoured, where it
/// goes (`alignAboveContext`) and what an ossia never draws (`firstClef`,
/// its clef's and its metre's engravers) read and passed over in silence;
/// anything else with a warning.
fn ossia_with(toks: &[Tok], j: usize, props: &mut Props) -> Result<(), ScoreError> {
    let end = skip_block(toks, j + 1)? - 1;
    let mut i = j + 2;
    while i < end {
        let t = &toks[i];
        if t.t == T::Word && matches!(t.v.as_str(), "alignAboveContext" | "alignBelowContext" | "firstClef") && is(toks, i + 1, T::Sym('=')) {
            i = value_end(toks, i + 2)?;
            continue;
        }
        if (is_cmd(toks, i, "remove") || is_cmd(toks, i, "consists"))
            && toks.get(i + 1).is_some_and(|a| matches!(a.t, T::Str | T::Word) && ["Time_signature_engraver", "Clef_engraver", "Bar_number_engraver"].contains(&a.v.as_str()))
        {
            i += 2;
            continue;
        }
        if let Some(next) = crate::look::context_setting(toks, i, &mut props.look)? {
            i = next;
            continue;
        }
        match skip_setting(toks, i, " in \\with")? {
            Some(next) => i = next.max(i + 1),
            None => i += 1,
        }
    }
    Ok(())
}

/// The ossias' staff: the music of each where it starts, spacers between,
/// and what the staff says of its bars — clef, key, metre, breaks, bar
/// lines, repeats, marks — at the same moments, for the staves' bars are
/// one and the staff above is the one that says them.
fn ossia_events(main: &[Ev], mut ossias: Vec<(i64, Vec<Ev>)>) -> Result<Vec<Ev>, ScoreError> {
    let mut said: Vec<(i64, Ev)> = Vec::new();
    let (mut t, mut second) = (0, false);
    for ev in main {
        match ev.k {
            Kind::Voices => {}
            Kind::VoiceNext => second = true,
            Kind::VoicesEnd => second = false,
            Kind::Note | Kind::Rest => {
                if !second {
                    t += ev.ticks * ev.count.max(1);
                }
            }
            _ if !second => said.push((t, ev.clone())),
            _ => {}
        }
    }
    let total = t;
    ossias.sort_by_key(|o| o.0);
    let mut out = Vec::new();
    let mut t = 0;
    let mut said = said.into_iter().peekable();
    // Up to `to`: spacers, and what the staff says on the way.
    let fill = |out: &mut Vec<Ev>, t: &mut i64, to: i64, said: &mut std::iter::Peekable<std::vec::IntoIter<(i64, Ev)>>| {
        while let Some((at, _)) = said.peek() {
            if *at > to {
                break;
            }
            let (at, ev) = said.next().unwrap_or((to, Ev::default()));
            if at < *t {
                continue;
            }
            if at > *t {
                out.push(Ev { k: Kind::Rest, spacer: true, ticks: at - *t, base: 4, ..Ev::default() });
                *t = at;
            }
            out.push(ev);
        }
        if to > *t {
            out.push(Ev { k: Kind::Rest, spacer: true, ticks: to - *t, base: 4, ..Ev::default() });
            *t = to;
        }
    };
    for (start, music) in ossias {
        if start < t {
            return refuse("two ossias over the same notes", music.first().map_or(0, |ev| ev.at));
        }
        fill(&mut out, &mut t, start, &mut said);
        let end = start + elapsed(&music);
        // What the staff says inside the ossia's bars is the ossia's own to say.
        while said.peek().is_some_and(|(at, _)| *at < end) {
            said.next();
        }
        out.extend(music);
        t = end;
    }
    fill(&mut out, &mut t, total, &mut said);
    Ok(out)
}

/// Where a staff's music has got to: the time of its first voice's notes
/// and rests so far.
pub(crate) fn elapsed(events: &[Ev]) -> i64 {
    let mut t = 0;
    let mut second = false;
    for ev in events {
        match ev.k {
            Kind::VoiceNext => second = true,
            Kind::VoicesEnd => second = false,
            Kind::Note | Kind::Rest if !second => t += ev.ticks * ev.count.max(1),
            _ => {}
        }
    }
    t
}

/// `\cueDuring "flute" #UP { R1 }`: the music, and over it, small and
/// silent, what the quote named plays meanwhile — a voice of its own, stems
/// up (`#UP`) or down, the music the other voice.
fn cue_during(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let at = toks[i].at;
    beyond();
    let Some(name) = toks.get(i + 1).filter(|n| n.t == T::Str).map(|n| n.v.clone()) else {
        return refuse("\\cueDuring wants a quote's name in quotes, #UP or #DOWN, then the music", at);
    };
    let up = match (toks.get(i + 2).map(|t| t.t), toks.get(i + 3).map(|t| t.v.as_str())) {
        (Some(T::Sym('#')), Some("UP")) => true,
        (Some(T::Sym('#')), Some("DOWN")) => false,
        _ => return refuse("\\cueDuring wants a quote's name in quotes, #UP or #DOWN, then the music", at),
    };
    let Some(quote) = st.quotes.get(&name).cloned() else {
        return refuse(format!("\\cueDuring \"{name}\": no \\addQuote names it"), at);
    };
    let from = elapsed(&st.events);
    let outer = std::mem::take(&mut st.events);
    let j = expr(toks, i + 4, st, hd)?;
    let mut music = std::mem::replace(&mut st.events, outer);
    // A run of bars' rest, R1*4, a bar each: a voice's rests are counted one by one.
    music = music
        .into_iter()
        .flat_map(|ev| {
            let n = if ev.k == Kind::Rest && ev.full { ev.count.max(1) } else { 1 };
            (0..n).map(move |_| Ev { count: if n > 1 { 1 } else { ev.count }, ..ev.clone() })
        })
        .collect();
    let to = from + elapsed(&music);
    // What the quote plays from `from` to `to`, its notes whole; a spacer where it does not.
    let mut cue = Vec::new();
    let (mut t, mut held) = (0, from);
    let mut second = false;
    let mut tuplets: HashMap<i64, i64> = HashMap::new();
    for ev in &quote {
        match ev.k {
            Kind::VoiceNext => second = true,
            Kind::VoicesEnd => second = false,
            Kind::Note | Kind::Rest if !second => {
                let (start, end) = (t, t + ev.ticks * ev.count.max(1));
                t = end;
                if ev.k != Kind::Note || start < from || end > to {
                    continue;
                }
                if start > held {
                    cue.push(Ev { k: Kind::Rest, spacer: true, ticks: start - held, base: 4, at, i: st.items, ..Ev::default() });
                    st.items += 1;
                }
                let mut note = ev.clone();
                note.i = st.items;
                st.items += 1;
                note.n = Some(st.notes);
                st.notes += 1;
                (note.size, note.cue) = (-3, true);
                (note.tie, note.slur_on, note.slur_off, note.beam_on, note.beam_off) = (false, false, false, false, false);
                (note.dynamic, note.hairpin, note.hairpin_end) = (None, None, false);
                note.texts.clear();
                note.voice = None;
                if let Some(tup) = note.tuplet.as_mut() {
                    let next = st.tuplets + 1 + tuplets.len() as i64;
                    tup.id = *tuplets.entry(tup.id).or_insert(next);
                }
                cue.push(note);
                held = end;
            }
            _ => {}
        }
    }
    st.tuplets += tuplets.len() as i64;
    if to > held {
        cue.push(Ev { k: Kind::Rest, spacer: true, ticks: to - held, base: 4, at, i: st.items, ..Ev::default() });
        st.items += 1;
    }
    let (first, second_voice) = if up { (cue, music) } else { (music, cue) };
    st.events.push(Ev { k: Kind::Voices, at, ..Ev::default() });
    for (v, events) in [(1u8, first), (2u8, second_voice)] {
        if v == 2 {
            st.events.push(Ev { k: Kind::VoiceNext, at, ..Ev::default() });
        }
        for mut ev in events {
            if matches!(ev.k, Kind::Note | Kind::Rest) {
                ev.voice = Some(v);
            }
            st.events.push(ev);
        }
    }
    st.events.push(Ev { k: Kind::VoicesEnd, at, ..Ev::default() });
    Ok(j)
}

/// Whether a part sings — has notes or rests, where a `\global` of the
/// staff's settings and spacers has none.
fn sings(events: &[Ev]) -> bool {
    events.iter().any(|ev| ev.k == Kind::Note || (ev.k == Kind::Rest && !ev.spacer))
}

/// What belongs to the staff rather than to a voice: the staff's settings
/// and the marks between its bars.
pub(crate) fn staff_level(k: &Kind) -> bool {
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
pub(crate) fn span(ev: &Ev) -> i64 {
    match ev.k {
        Kind::Note | Kind::Rest => ev.ticks * ev.count.max(1),
        _ => 0,
    }
}

/// `main` with the staff's settings of `other` set in it at the moments
/// they are written — a `\global`'s `\key` and `\time`, a second voice's
/// `\clef` — and `other` without them.
pub(crate) fn merge(main: Vec<Ev>, other: Vec<Ev>) -> (Vec<Ev>, Vec<Ev>) {
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

/// `\partCombine \partI \partII` (and `\partcombine`, `\partCombineUp`,
/// `\partCombineDown`): two parts read one after the other — under
/// `\relative` the second from the first's last pitch, as LilyPond reads a
/// music function's arguments — then set on one staff (`combine`).
fn part_combine(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>, dir: i8) -> Result<usize, ScoreError> {
    beyond();
    let at = toks[i].at;
    if !starts_expr(toks, i + 1) {
        return refuse("\\partCombine wants two parts, each its music", at);
    }
    let outer = std::mem::take(&mut st.events);
    let outer_modes = std::mem::take(&mut st.look.combine);
    let outer_voice = st.look.voice.clone();
    let mut parts = Vec::new();
    let mut j = i + 1;
    for _ in 0..2 {
        if !starts_expr(toks, j) {
            st.events = outer;
            return refuse("\\partCombine wants two parts, each its music", at);
        }
        st.look.restore_voice(outer_voice.clone());
        j = expr(toks, j, st, hd)?;
        if let Some(g) = st.graces.first() {
            return refuse("a grace note goes before a note, and none follows it in its part", g.at);
        }
        parts.push(std::mem::take(&mut st.events));
    }
    let modes = std::mem::replace(&mut st.look.combine, outer_modes);
    st.look.restore_voice(outer_voice);
    st.events = outer;
    let second = parts.pop().unwrap_or_default();
    let first = parts.pop().unwrap_or_default();
    let combined = crate::combine::combine(first, second, modes, dir, at)?;
    st.events.extend(combined);
    Ok(j)
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
    let outer_here = st.here;
    st.here += elapsed(&outer);
    let outer_cmd = st.voice_cmd;
    let outer_part = st.in_part;
    // Each voice's own settings start from the ones before `<<`, and stay its own.
    let outer_look = st.look.voice.clone();
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
            if apart == 4 {
                return refuse("four voices on a staff are what this draws, no more", toks[i].at);
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
        st.look.restore_voice(outer_look.clone());
        let at = toks[i].at;
        i = expr(toks, i, st, hd)?;
        if let Some(g) = st.graces.first() {
            return refuse("a grace note goes before a note, and none follows it in its voice", g.at);
        }
        parts.push(Part { events: std::mem::take(&mut st.events), voice: st.voice_cmd, name: st.voice_name.take(), at });
    }
    // A `\partCombineUp` and its kin: its two voices, each a part.
    if parts.iter().any(|p| p.voice.is_none() && crate::combine::is_pair(&p.events)) {
        let mut flat = Vec::new();
        for p in parts {
            if p.voice.is_none() && crate::combine::is_pair(&p.events) {
                let at = p.at;
                for (v, events) in crate::combine::unnest(p.events) {
                    flat.push(Part { events, voice: Some(v), name: None, at });
                }
            } else {
                flat.push(p);
            }
        }
        parts = flat;
    }
    st.events = outer;
    st.here = outer_here;
    st.look.restore_voice(outer_look);
    st.voice_cmd = outer_cmd;
    st.in_part = outer_part;
    let singing: Vec<usize> = (0..parts.len()).filter(|&k| sings(&parts[k].events)).collect();
    if parts.len() != 2 || apart != 1 || parts.iter().skip(1).any(|p| p.events.iter().any(|ev| staff_level(&ev.k))) {
        beyond();
    }
    if singing.len() > 4 {
        return refuse("four voices on a staff are what this draws, no more", parts[singing[4]].at);
    }
    if singing.len() > 2 {
        beyond();
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
    // Each voice its number: the one \\voiceOne … \\voiceFour gives it, else
    // the first left in the order the parts come.
    let mut numbers: Vec<Option<u8>> = singing.iter().map(|&k| parts[k].voice).collect();
    for x in 0..numbers.len() {
        if let Some(v) = numbers[x] {
            if numbers.iter().take(x).any(|w| *w == Some(v)) {
                return refuse("two voices on a staff with the same \\voiceOne … \\voiceFour", parts[singing[x]].at);
            }
        }
    }
    for x in 0..numbers.len() {
        if numbers[x].is_none() {
            let free = (1u8..=4).find(|v| !numbers.contains(&Some(*v))).unwrap_or(4);
            numbers[x] = Some(free);
        }
    }
    let mut voices: Vec<(u8, usize)> = singing.iter().zip(&numbers).map(|(&k, v)| (v.unwrap_or(1), k)).collect();
    voices.sort_by_key(|v| v.0);
    let mut lists: Vec<Vec<Ev>> = Vec::new();
    for &(_, k) in &voices {
        lists.push(std::mem::take(&mut parts[k].events));
    }
    // The voices as written, by their names (`\new Voice = "alto"`).
    let named = voices.clone();
    // Voices inside a voice (score #408): each made one more voice of
    // these, silent (spacers) where it is not written.
    if lists.iter().any(|l| l.iter().any(|ev| ev.k == Kind::Voices)) {
        let (flat_lists, flat_numbers) = nested_voices(lists, voices.iter().map(|v| v.0).collect())?;
        lists = flat_lists;
        voices = flat_numbers.into_iter().map(|v| (v, voices[0].1)).collect();
    }
    // What a voice says for the whole staff (a key, a metre) goes with the first.
    for x in 1..lists.len() {
        let other = std::mem::take(&mut lists[x]);
        let first = std::mem::take(&mut lists[0]);
        let (merged, kept) = merge(first, other);
        lists[0] = merged;
        lists[x] = kept;
    }
    for p in parts.iter_mut() {
        if !p.events.is_empty() {
            let first = std::mem::take(&mut lists[0]);
            lists[0] = merge(first, std::mem::take(&mut p.events)).0;
        }
    }
    for (events, &(v, _)) in lists.iter_mut().zip(&voices) {
        for ev in events.iter_mut() {
            if ev.k == Kind::Note || ev.k == Kind::Rest {
                if ev.count > 1 {
                    return refuse("a run of bars' rest is the staff's: write it outside << \\\\ >>", ev.at);
                }
                ev.voice = Some(v);
            }
        }
    }
    for &(v, k) in &named {
        if let Some(name) = parts[k].name.take() {
            st.voice_names.push((name, v));
        }
    }
    st.events.push(Ev { k: Kind::Voices, at: open_at, ..Ev::default() });
    for (x, events) in lists.into_iter().enumerate() {
        if x > 0 {
            st.events.push(Ev { k: Kind::VoiceNext, at: parts[voices[x].1].at, ..Ev::default() });
        }
        st.events.extend(events);
    }
    st.events.push(Ev { k: Kind::VoicesEnd, at: open_at, ..Ev::default() });
    Ok(i)
}

/// The voices of a `<< \\ >>` some of which hold a `<< \\ >>` of their
/// own: each inner one beyond its first made a voice of the outer, under the
/// number LilyPond's `\\` gives its stems — a second (or fourth) down, a
/// third up — where one is free, else any free one; silent before and after
/// the passage it sings (spacers), as long as the voice it came from. The
/// inner first goes on in its voice. Four voices at once are the most.
fn nested_voices(lists: Vec<Vec<Ev>>, numbers: Vec<u8>) -> Result<(Vec<Vec<Ev>>, Vec<u8>), ScoreError> {
    let mut used: Vec<u8> = numbers.clone();
    let mut out: Vec<(u8, Vec<Ev>)> = Vec::new();
    let mut extra: Vec<(u8, Vec<Ev>)> = Vec::new();
    for (list, v) in lists.into_iter().zip(numbers) {
        let total = elapsed(&list);
        let mut main: Vec<Ev> = Vec::new();
        let mut t = 0;
        let mut k = 0;
        while k < list.len() {
            if list[k].k != Kind::Voices {
                t += span(&list[k]);
                main.push(list[k].clone());
                k += 1;
                continue;
            }
            // The inner voices, to their `>>`.
            let open_at = list[k].at;
            let mut inner: Vec<Vec<Ev>> = vec![Vec::new()];
            k += 1;
            while k < list.len() && list[k].k != Kind::VoicesEnd {
                if list[k].k == Kind::VoiceNext {
                    inner.push(Vec::new());
                } else if let Some(last) = inner.last_mut() {
                    last.push(list[k].clone());
                }
                k += 1;
            }
            k += 1;
            let length = elapsed(&inner[0]);
            if inner.iter().any(|l| elapsed(l) != length) {
                return refuse("the voices of << \\\\ >> are not as long as each other", open_at);
            }
            let mut first = inner.remove(0);
            for ev in &mut first {
                if matches!(ev.k, Kind::Note | Kind::Rest) {
                    ev.voice = Some(v);
                }
            }
            main.extend(first);
            for (x, mut events) in inner.into_iter().enumerate() {
                // LilyPond's `\\` makes its second voice's stems go down,
                // its third's up, its fourth's down.
                let down = x % 2 == 0;
                let want: [u8; 4] = if down { [2, 4, 3, 1] } else { [3, 1, 4, 2] };
                let Some(n) = want.iter().copied().find(|n| !used.contains(n)) else {
                    return refuse("four voices on a staff are what this draws, no more", open_at);
                };
                used.push(n);
                for ev in &mut events {
                    if matches!(ev.k, Kind::Note | Kind::Rest) {
                        ev.voice = Some(n);
                    }
                }
                let mut voice = silence(t, open_at);
                voice.extend(events);
                voice.extend(silence(total - t - length, open_at));
                extra.push((n, voice));
            }
            t += length;
        }
        out.push((v, main));
    }
    out.extend(extra);
    out.sort_by_key(|o| o.0);
    Ok((out.iter().map(|o| o.1.clone()).collect(), out.iter().map(|o| o.0).collect()))
}

/// Spacers lasting `ticks`, each a note value the time is written in where
/// one is: what an inner voice is silent for.
fn silence(ticks: i64, at: usize) -> Vec<Ev> {
    if ticks <= 0 {
        return Vec::new();
    }
    let spacer = |base: i64, dots: i64, ticks: i64| Ev { k: Kind::Rest, spacer: true, base, dots, ticks, at, i: usize::MAX, ..Ev::default() };
    match crate::measure::spell(ticks) {
        Some(values) => values.into_iter().map(|(base, dots, t)| spacer(base, dots, t)).collect(),
        None => vec![spacer(1, 0, ticks)],
    }
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
        // Written with `-`: the side is settled with the layout settings (`look::settle`).
        last.texts.push(Words::plain(nxt.v, dir));
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
        last.texts.push(Words { text, dir, runs });
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
            // A mode's signature: its tonic's major key, moved round the circle of fifths.
            let shift = match mode.v.as_str() {
                "major" | "ionian" => 0,
                "minor" | "aeolian" => -3,
                "dorian" => -2,
                "phrygian" => -4,
                "lydian" => 1,
                "mixolydian" => -1,
                "locrian" => -5,
                _ => {
                    return refuse(
                        format!("the mode '{}' is not one a key signature is written for (\\major, \\minor, \\dorian, \\phrygian, \\lydian, \\mixolydian, \\aeolian, \\locrian, \\ionian)", mode.v),
                        mode.at,
                    )
                }
            };
            if shift != 0 && shift != -3 {
                beyond();
            }
            let tonic = transposed(st, step, alter, tk.at)?;
            let fifths = [0, 2, 4, -1, 1, 3, 5][modulo(tonic.d, 7) as usize] + 7 * tonic.alter + shift;
            if !(-7..=7).contains(&fifths) {
                return refuse(format!("a key of {fifths} fifths has no signature"), tk.at);
            }
            st.events.push(Ev { k: Kind::Key, fifths, at: tk.at, ..Ev::default() });
            return Ok(i + 3);
        }
        "time" => {
            // `3/4`, or an additive metre, `2+3/8`, its parts summed and kept to be drawn.
            let mut j = i + 1;
            let mut added: Vec<i64> = Vec::new();
            while is(toks, j, T::Num) && !toks[j].v.contains('/') && is(toks, j + 1, T::Sym('+')) {
                added.push(toks[j].v.parse().unwrap_or(0));
                j += 2;
            }
            let last = toks.get(j).filter(|t| t.t == T::Num);
            let parts: Vec<String> = last.map(|a| a.v.split('/').map(str::to_owned).collect()).unwrap_or_default();
            if parts.len() != 2 {
                return refuse("\\time wants a fraction like 3/4, or 2+3/8", tk.at);
            }
            added.push(parts[0].parse().unwrap_or(0));
            let num: i64 = added.iter().sum();
            let den: i64 = parts[1].parse().unwrap_or(0);
            let fits = added.iter().all(|p| (1..=99).contains(p)) && added.len() <= 4 && num <= 99;
            if !(fits && [1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 32, 64].contains(&den)) {
                let a = arg.unwrap_or(tk.clone());
                return refuse(format!("\\time {} is not a metre this draws", a.v), a.at);
            }
            if added.len() > 1 || num > 12 || ![1, 2, 4, 8, 16].contains(&den) {
                beyond();
            }
            let mut time_parts = [0u8; 4];
            if added.len() > 1 {
                for (slot, p) in time_parts.iter_mut().zip(&added) {
                    *slot = u8::try_from(*p).unwrap_or(0);
                }
            }
            st.events.push(Ev { k: Kind::Time, time: (num, den), time_parts, at: tk.at, ..Ev::default() });
            return Ok(j + 1);
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
            if kind == "Staff" && st.in_part {
                return ossia(toks, i, st, hd);
            }
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
            // A cue voice: another part's notes, small, read and not played.
            let saved = (st.size, st.cue);
            if kind == "CueVoice" {
                beyond();
                (st.size, st.cue) = (-3, true);
            }
            j = expr(toks, j, st, hd)?;
            (st.size, st.cue) = saved;
            if let Some(name) = name {
                if st.in_part {
                    st.voice_name = Some(name);
                } else {
                    st.voice_names.push((name, 1));
                }
            }
            return Ok(j);
        }
        "voiceOne" | "voiceTwo" | "voiceThree" | "voiceFour" => {
            st.voice_cmd = Some(match word {
                "voiceOne" => 1,
                "voiceTwo" => 2,
                "voiceThree" => 3,
                _ => 4,
            });
            return Ok(i + 1);
        }
        "oneVoice" => return Ok(i + 1),
        "cueDuring" => return cue_during(toks, i, st, hd),
        "partCombine" | "partcombine" | "partCombineUp" | "partcombineUp" | "partCombineDown" | "partcombineDown" => {
            let dir = if word.ends_with("Up") {
                1
            } else if word.ends_with("Down") {
                -1
            } else {
                0
            };
            return part_combine(toks, i, st, hd, dir);
        }
        "magnifyMusic" => {
            // `\magnifyMusic 0.63 { … }`: the music in it that much smaller,
            // in LilyPond's font-size steps (six to an octave of size).
            let mut j = i + 1;
            if is(toks, j, T::Sym('#')) {
                j += 1;
            }
            let Some((ratio, after)) = number_at(toks, j).filter(|r| r.0 > 0.0) else { return refuse("\\magnifyMusic wants a ratio, like 0.63, then its music", tk.at) };
            if !starts_expr(toks, after) {
                return refuse("\\magnifyMusic wants a ratio, like 0.63, then its music", tk.at);
            }
            let saved = st.size;
            st.size = (6.0 * ratio.log2()).round().clamp(-12.0, 12.0) as i8;
            let end = expr(toks, after, st, hd)?;
            st.size = saved;
            beyond();
            return Ok(end);
        }
        "normalsize" | "small" | "tiny" | "teeny" => {
            // The notes after it, smaller: LilyPond's font-size steps.
            beyond();
            st.size = match word {
                "small" => -1,
                "tiny" => -2,
                "teeny" => -3,
                _ => 0,
            };
            return Ok(i + 1);
        }
        "change" => {
            // `\\change Staff = "down"`: the notes after it are drawn on that staff.
            let ok = toks.get(i + 1).is_some_and(|t| t.t == T::Word && t.v == "Staff") && is(toks, i + 2, T::Sym('=')) && is(toks, i + 3, T::Str);
            if !ok {
                return refuse("\\change wants Staff = and a staff's name in quotes", tk.at);
            }
            beyond();
            let name = toks[i + 3].v.clone();
            let to = if st.own.as_deref() == Some(name.as_str()) { None } else { Some(name) };
            st.follow_next |= st.follow && to != st.cross;
            st.cross = to;
            return Ok(i + 4);
        }
        "showStaffSwitch" | "hideStaffSwitch" => {
            // The line that follows a voice from one staff to the other.
            beyond();
            st.follow = word == "showStaffSwitch";
            return Ok(i + 1);
        }
        "crossStaff" => {
            // `\crossStaff { e4 }`: each note's stem joins the one on the
            // staff next to it (LilyPond's Span_stem_engraver).
            beyond();
            let saved = st.span_stem;
            st.span_stem = true;
            let j = expr(toks, i + 1, st, hd)?;
            st.span_stem = saved;
            return Ok(j);
        }
        "sequential" => return expr(toks, i + 1, st, hd),
        "chordmode" | "chords" | "drummode" | "drums" | "notemode" | "figuremode" | "figures" => {
            let saved = st.mode;
            st.mode = match word {
                "chordmode" | "chords" => Mode::Chords,
                "drummode" | "drums" => Mode::Drums,
                "figuremode" | "figures" => Mode::Figures,
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
        "afterGrace" => {
            // `\afterGrace c2 { d16 e }`, or `\afterGrace 7/8 c2 { … }`: the
            // grace notes after the note, at that point of it (3/4 when no
            // fraction is written, LilyPond's afterGraceFraction).
            beyond();
            if st.tuplet.is_some() {
                return refuse("a grace note in a tuplet is not drawn", tk.at);
            }
            let mut j = i + 1;
            let mut fraction = AFTER_GRACE;
            if is(toks, j, T::Num) && toks[j].v.contains('/') {
                let parts: Vec<f64> = toks[j].v.split('/').map(|p| p.parse().unwrap_or(0.0)).collect();
                let (n, d) = (parts.first().copied().unwrap_or(0.0), parts.get(1).copied().unwrap_or(0.0));
                if !(d > 0.0 && n > 0.0 && n < d) {
                    return refuse("\\afterGrace's fraction is a part of its note, between 0 and 1, like 7/8", toks[j].at);
                }
                fraction = n / d;
                j += 1;
            }
            let before = st.events.len();
            let mut j = expr(toks, j, st, hd)?;
            if st.events.len() != before + 1 || st.events[before].k != Kind::Note || st.events[before].grace.is_some() {
                return refuse("\\afterGrace wants one note, then its grace notes in { }", tk.at);
            }
            // `\afterGrace c''1~ { d''16 }`, `c''1( { … }`, `c''1\f { … }`:
            // what is written after the note is the note's, as LilyPond reads it.
            while post_event(toks, j) {
                j = if toks[j].t == T::Cmd { command(toks, j, st, hd)? } else { post_sign(toks, j, st, hd)? };
            }
            if st.events.len() != before + 1 {
                return refuse("\\afterGrace wants one note, then its grace notes in { }", tk.at);
            }
            if !is(toks, j, T::Sym('{')) && !is(toks, j, T::Sym('<')) && !is(toks, j, T::Word) {
                return refuse("\\afterGrace wants its grace notes after the note, in { }", tk.at);
            }
            let (items, notes) = (st.items, st.notes);
            let waiting = std::mem::take(&mut st.graces);
            let after = st.events.len();
            let j = expr(toks, j, st, hd)?;
            let read = st.events.split_off(after);
            st.graces = waiting;
            st.items = items;
            st.notes = notes;
            if read.is_empty() {
                return refuse("\\afterGrace wants grace notes after its note", tk.at);
            }
            let mut graces = Vec::with_capacity(read.len());
            for mut g in read {
                if g.k != Kind::Note || !g.graces.is_empty() {
                    return refuse("grace notes are notes: no rests or settings among them", g.at);
                }
                g.grace = Some(GraceKind::Plain);
                g.slur_on = false;
                g.slur_off = false;
                g.n = None;
                graces.push(g);
            }
            let main = &mut st.events[before];
            main.after_graces = graces;
            main.after_at = fraction;
            return Ok(j);
        }
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
        "sustainOn" | "sustainOff" => {
            // Drawn as LilyPond's default pedal style, text: "Ped." and "*"
            // under the staff, at the note.
            let on = word == "sustainOn";
            let last = last_item(&mut st.events, &tk)?;
            if on {
                last.pedal_on = true;
            } else {
                last.pedal_off = true;
            }
            let glyph = if on { "keyboardPedalPed" } else { "keyboardPedalUp" };
            // Let go and pressed again on one note: "* Ped." side by side.
            let pedal = last.texts.iter_mut().find(|w| w.dir > 0 && w.runs.iter().any(|r| r.glyph && r.text.starts_with("keyboardPedal")));
            match pedal {
                Some(w) => w.runs.push(Run::glyph(glyph)),
                None => last.texts.push(Words { text: String::new(), dir: 1, runs: vec![Run::glyph(glyph)] }),
            }
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
        "jump" | "fine" => {
            // LilyPond 2.24's jumps: `\jump "D.S. al Coda"`, `\fine`. Set in
            // italics at the end of the bar they are written after, against
            // its bar line; `dir` 1 tells them from a mark over it.
            let (runs, j) = if word == "fine" {
                (vec![Run::words("Fine", Weight::Italic)], i + 1)
            } else {
                match toks.get(i + 1) {
                    Some(t) if t.t == T::Str => (vec![Run::words(t.v.clone(), Weight::Italic)], i + 2),
                    Some(t) if t.t == T::Cmd && t.v == "markup" => markup(toks, i + 2, t.at)?,
                    _ => return refuse("\\jump wants a string or \\markup", tk.at),
                }
            };
            st.events.push(Ev { k: Kind::Mark, texts: vec![Words { text: String::new(), dir: 1, runs }], at: tk.at, ..Ev::default() });
            return Ok(j);
        }
        "mark" | "segnoMark" | "codaMark" => {
            let (runs, j) = match word {
                "segnoMark" => (vec![Run::glyph("segno")], i + 1),
                "codaMark" => (vec![Run::glyph("coda")], i + 1),
                _ => mark_label(toks, i, st)?,
            };
            // `\mark \default` and `\mark 3` keep their number, for the
            // formatter the layout may set (`look::settle`).
            let k = if is(toks, i + 1, T::Sym('#')) { i + 2 } else { i + 1 };
            let numbered = word == "mark" && (is_cmd(toks, k, "default") || is(toks, k, T::Num));
            let count = if numbered { st.marks } else { 0 };
            st.events.push(Ev { k: Kind::Mark, texts: vec![Words { text: String::new(), dir: -1, runs }], count, at: tk.at, ..Ev::default() });
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
    if word == "noBreak" || word == "noPageBreak" {
        // No line (or page) break at the bar line it stands at.
        beyond();
        st.events.push(Ev { k: Kind::Break, page: word == "noPageBreak", v: "no".into(), at: tk.at, ..Ev::default() });
        return Ok(i + 1);
    }
    if word == "set" && is(toks, i + 2, T::Sym('.')) && toks.get(i + 3).is_some_and(|t| t.t == T::Word && t.v == "skipBars") && is(toks, i + 4, T::Sym('=')) && is(toks, i + 5, T::Sym('#')) {
        // `\set Score.skipBars = ##t`: the older way to gather bars of rest.
        beyond();
        let end = scheme_end(toks, i + 5)?;
        if toks[i + 5..end].iter().any(|t| t.t == T::Word && t.v == "t") {
            st.events.push(Ev { k: Kind::Compress, at: tk.at, ..Ev::default() });
        }
        return Ok(end);
    }
    if word == "set"
        && is(toks, i + 2, T::Sym('.'))
        && toks.get(i + 1).is_some_and(|t| t.t == T::Word && t.v == "Staff")
        && toks.get(i + 3).is_some_and(|t| t.t == T::Word && t.v == "timeSignatureFraction")
        && is(toks, i + 4, T::Sym('='))
    {
        return shown_fraction(toks, i + 5, st, tk.at);
    }
    if word == "set" {
        let staff =
            toks.get(i + 1).is_some_and(|t| t.t == T::Word && t.v == "Staff") && is(toks, i + 2, T::Sym('.')) && toks.get(i + 3).is_some_and(|t| t.t == T::Word && STAFF_PROPS.contains(&t.v.as_str()));
        if staff {
            return staff_prop(toks, i + 3, &mut st.props, "\\set Staff");
        }
        // `\set Staff.autoBeaming = ##f`: beams only where written.
        let j = path_end(toks, i + 1);
        if path_words(toks, i + 1, j).last().is_some_and(|w| w == "autoBeaming") && is(toks, j, T::Sym('=')) && is(toks, j + 1, T::Sym('#')) {
            beyond();
            let end = scheme_end(toks, j + 1)?;
            st.no_autobeam = toks[j + 1..end].iter().any(|t| t.t == T::Word && t.v == "f");
            return Ok(end);
        }
        // What the engraver draws is honoured; anything else it sets is
        // read and passed over.
        if let Some(end) = crate::look::command(toks, i, &mut st.look, elapsed(&st.events))? {
            beyond();
            return Ok(end);
        }
        return match skip_setting(toks, i, "")? {
            Some(end) => Ok(end),
            None => refuse("\\set wants a property, = and a value", tk.at),
        };
    }
    if word == "tuplet" || word == "times" {
        return tuplet(toks, i, st, word, hd);
    }
    if word == "scaleDurations" {
        return scale_durations(toks, i, st, hd);
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
        // LilyPond's music function: the music after it, `{ … }` or any
        // other expression, its runs of bars' rest gathered.
        st.events.push(Ev { k: Kind::Compress, at: tk.at, ..Ev::default() });
        if !arg.is_some_and(|a| a.t == T::Sym('{')) {
            if !starts_expr(toks, i + 1) && !is(toks, i + 1, T::Word) {
                return refuse("\\compressMMRests wants its music", tk.at);
            }
            beyond();
            return expr(toks, i + 1, st, hd);
        }
        let j = music(toks, i + 2, st, hd)?;
        if j >= toks.len() || toks[j].t != T::Sym('}') {
            return refuse("a \\compressMMRests block that is never closed", tk.at);
        }
        return Ok(j + 1);
    }
    if let Some(j) = layout_command(toks, i, st)? {
        return Ok(j);
    }
    refuse(format!("\\{word} is not a command this reads"), tk.at)
}

/// The staff's settings `\set Staff.…` and `\with` read.
const STAFF_PROPS: [&str; 5] = ["instrumentName", "shortInstrumentName", "midiInstrument", "midiMaximumVolume", "midiPanPosition"];

/// The layout settings and the signs a staff's music may say beyond the
/// Soli engraver's: honoured where the engraver can, else read and passed
/// over with a warning. None for a command that is neither.
fn layout_command(toks: &[Tok], i: usize, st: &mut St) -> Result<Option<usize>, ScoreError> {
    let tk = toks[i].clone();
    let word = tk.v.as_str();
    let at = tk.at;
    let done = |j: usize| {
        beyond();
        Ok(Some(j))
    };
    match word {
        "stemUp" | "stemDown" | "stemNeutral" => {
            st.stem = match word {
                "stemUp" => 1,
                "stemDown" => -1,
                _ => 0,
            };
            done(i + 1)
        }
        "autoBeamOff" | "autoBeamOn" => {
            st.no_autobeam = word == "autoBeamOff";
            done(i + 1)
        }
        "override" => {
            let j = path_end(toks, i + 1);
            let path = path_words(toks, i + 1, j);
            if !is(toks, j, T::Sym('=')) {
                return refuse("\\override wants a property, = and a value", at);
            }
            let is_path = |want: &[&str]| path.len() >= want.len() && path[path.len() - want.len()..].iter().zip(want).all(|(a, b)| a == b);
            // A text spanner's words, which `\startTextSpan` sets before its line.
            if is_path(&["TextSpanner", "bound-details", "left", "text"]) {
                let v = toks.get(j + 1);
                st.span_text = match v {
                    Some(s) if s.t == T::Str => vec![Run::words(s.v.clone(), Weight::Regular)],
                    Some(s) if s.t == T::Cmd && s.v == "markup" => markup(toks, j + 2, s.at)?.0,
                    _ => Vec::new(),
                };
                return done(value_end(toks, j + 1)?);
            }
            // What the engraver draws — `\override Stem.direction = #UP`,
            // `TextScript.extra-offset` … — is honoured; the rest passed over.
            if let Some(end) = crate::look::command(toks, i, &mut st.look, elapsed(&st.events))? {
                return done(end);
            }
            Ok(skip_setting(toks, i, "")?)
        }
        "revert" | "unset" | "hide" | "omit" | "undo" | "once" | "temporary" | "single" | "magnifyStaff" => {
            if let Some(end) = crate::look::command(toks, i, &mut st.look, elapsed(&st.events))? {
                return done(end);
            }
            Ok(skip_setting(toks, i, "")?)
        }
        w if SWITCHES.contains(&w) => {
            if let Some(end) = crate::look::command(toks, i, &mut st.look, elapsed(&st.events))? {
                return done(end);
            }
            Ok(skip_setting(toks, i, "")?)
        }
        "tweak" => {
            // `\tweak color #red`, `-\tweak Accidental.color #red`: before what it
            // tweaks — the next note, or after a note the sign that follows.
            if let Some((key, v, end)) = crate::look::tweak(toks, i)? {
                let after_note = i > 0 && matches!(toks[i - 1].t, T::Sym('-' | '^' | '_')) && toks[i - 1].at + 1 == at;
                if after_note {
                    let last = last_item(&mut st.events, &tk)?;
                    last.look = crate::look::with_one(&last.look, key, v);
                } else {
                    st.look.set_once(key, v);
                }
                return done(end);
            }
            let j = path_end(toks, i + 1);
            warn(format!("\\tweak {} is not drawn", path_words(toks, i + 1, j).join(".")), at);
            done(value_end(toks, j)?)
        }
        "accidentalStyle" => {
            let j = path_end(toks, i + 1);
            warn(format!("\\accidentalStyle {} is not drawn: accidentals are printed as LilyPond's default style prints them", path_words(toks, i + 1, j).join(".")), at);
            done(j)
        }
        "shape" => {
            // `\shape #'((0 . 0) …) Slur`: the curve's offsets, then the grob.
            let j = value_end(toks, i + 1)?;
            warn("\\shape is not drawn", at);
            done(path_end(toks, j))
        }
        "transposition" => {
            if !is(toks, i + 1, T::Word) {
                return refuse("\\transposition wants a pitch", at);
            }
            warn("\\transposition is not read: the notes are played as written", at);
            done(i + 2)
        }
        "compressFullBarRests" => {
            st.events.push(Ev { k: Kind::Compress, at, ..Ev::default() });
            done(i + 1)
        }
        "breathe" | "caesura" => {
            last_item(&mut st.events, &tk)?.breath = Some(if word == "breathe" { "breathMarkComma" } else { "caesura" });
            done(i + 1)
        }
        "laissezVibrer" | "repeatTie" => {
            let last = last_note(&mut st.events, &tk)?;
            if word == "laissezVibrer" {
                last.lv = true;
            } else {
                last.repeat_tie = true;
            }
            done(i + 1)
        }
        "cresc" | "dim" | "decresc" => {
            let last = last_item(&mut st.events, &tk)?;
            last.hairpin = Some(if word == "cresc" { '<' } else { '>' });
            last.hairpin_text = Some(format!("{word}."));
            done(i + 1)
        }
        "startTextSpan" => {
            last_item(&mut st.events, &tk)?.span_start = Some(st.span_text.clone());
            done(i + 1)
        }
        "stopTextSpan" => {
            last_item(&mut st.events, &tk)?.span_stop = true;
            done(i + 1)
        }
        "large" | "huge" => {
            // Larger notes from here, as `\small` makes them smaller.
            st.size = if word == "large" { 1 } else { 2 };
            done(i + 1)
        }
        _ => {
            // `\RemoveEmptyStaves`, `\partCombineApart` and their kin.
            if let Some(end) = crate::look::command(toks, i, &mut st.look, elapsed(&st.events))? {
                return done(end);
            }
            Ok(None)
        }
    }
}

/// A duration written as an argument — `4`, `8.`, `2*3`, `4*3/2` — in
/// ticks, and the token after it.
fn duration_arg(toks: &[Tok], mut i: usize) -> Option<(i64, usize)> {
    let base: i64 = toks.get(i).filter(|t| t.t == T::Num && !t.v.contains('/'))?.v.parse().ok()?;
    if ![1, 2, 4, 8, 16, 32, 64, 128].contains(&base) {
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
    if st.tuplet_outer.is_some() {
        return refuse("a tuplet three deep is not drawn: two tuplets nest, no more", tk.at);
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
    if st.tuplet.is_some() {
        beyond();
    }
    st.tuplets += 1;
    st.tuplet_outer = st.tuplet.take();
    st.tuplet = Some(TupletState { id: st.tuplets, n, d, span, used: 0 });
    let k = music(toks, j + 1, st, hd)?;
    if k >= toks.len() || toks[k].t != T::Sym('}') {
        return refuse("a tuplet that is never closed", tk.at);
    }
    st.tuplet = st.tuplet_outer.take();
    Ok(k + 1)
}

/// `\scaleDurations 2/3 music`: the music's durations so much shorter
/// (or longer), drawn as written, no tuplet's number or bracket — how
/// LilyPond lines up the bars of staves in different metres (score #408).
fn scale_durations(toks: &[Tok], i: usize, st: &mut St, hd: &mut HashMap<String, String>) -> Result<usize, ScoreError> {
    let at = toks[i].at;
    let ratio = toks.get(i + 1).filter(|t| t.t == T::Num && t.v.contains('/')).map(|t| t.v.split('/').map(|p| p.parse::<i64>().unwrap_or(0)).collect::<Vec<_>>());
    let Some([n, d]) = ratio.as_deref() else { return refuse("\\scaleDurations wants a ratio, like 2/3, then its music", at) };
    let (n, d) = (*n, *d);
    if !((1..=16).contains(&n) && (1..=16).contains(&d)) || !starts_expr(toks, i + 2) {
        return refuse("\\scaleDurations wants a ratio, like 2/3, then its music", at);
    }
    let outer = st.scale;
    let (sn, sd) = (outer.0 * n, outer.1 * d);
    let g = gcd(sn, sd);
    st.scale = (sn / g, sd / g);
    let j = expr(toks, i + 2, st, hd);
    st.scale = outer;
    j
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs().max(1)
    } else {
        gcd(b, a % b)
    }
}

/// `\set Staff.timeSignatureFraction = 9/8` (or `#'(9 . 8)`): the metre a
/// staff shows, its bars as long as the score's metre makes them — polymetre
/// with bars of one length (score #408). Kept as a `\time` marked "shown".
fn shown_fraction(toks: &[Tok], j: usize, st: &mut St, at: usize) -> Result<usize, ScoreError> {
    let wrong = || refuse("\\set Staff.timeSignatureFraction wants a fraction, like 9/8", at);
    let (num, den, end) = if toks.get(j).is_some_and(|t| t.t == T::Num && t.v.contains('/')) {
        let parts: Vec<i64> = toks[j].v.split('/').map(|p| p.parse().unwrap_or(0)).collect();
        (parts[0], parts.get(1).copied().unwrap_or(0), j + 1)
    } else if is(toks, j, T::Sym('#')) {
        // `#'(9 . 8)`.
        let end = value_end(toks, j)?;
        let nums: Vec<i64> = toks[j..end].iter().filter(|t| t.t == T::Num).filter_map(|t| t.v.parse().ok()).collect();
        if nums.len() != 2 {
            return wrong();
        }
        (nums[0], nums[1], end)
    } else {
        return wrong();
    };
    if !((1..=99).contains(&num) && [1, 2, 4, 8, 16, 32, 64].contains(&den)) {
        return wrong();
    }
    beyond();
    st.events.push(Ev { k: Kind::Time, time: (num, den), v: SHOWN.into(), at, ..Ev::default() });
    Ok(end)
}

/// What marks the 4/4 a staff opens in when no `\time` says one and its
/// metre is drawn otherwise (`\numericTimeSignature`, `\omit`): the
/// score's own metre, where another staff writes one, takes its place.
pub const DEFAULT_METRE: &str = "default";

/// What marks a `\time` that changes only the metre shown
/// (`timeSignatureFraction`), not the bars' length.
pub const SHOWN: &str = "shown";

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
    let mut figures: Vec<Vec<Run>> = Vec::new();
    // `cis!` and `cis?`: the accidentals written as a reminder.
    let (mut forced, mut cautionary): (Vec<i64>, Vec<i64>) = (Vec::new(), Vec::new());
    if st.mode == Mode::Figures && tk.t == T::Sym('<') {
        // `<6 4+>`: its figures, top to bottom; held where a note would be,
        // and sounding nothing.
        let (lines, j) = figure_lines(toks, i)?;
        figures = lines;
        rest = Some("s".into());
        i = j;
    } else if tk.t == T::Sym('<') {
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
            i = reminder(toks, i, pitches.last().map_or(0, |p| p.d), &mut forced, &mut cautionary);
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
        if let Some(p) = pitches.last().filter(|_| pitch_at.len() == 1 && st.mode == Mode::Notes) {
            i = reminder(toks, i, p.d, &mut forced, &mut cautionary);
        }
    }
    if is(toks, i, T::Num) {
        let base: i64 = toks[i].v.parse().unwrap_or(0);
        if ![1, 2, 4, 8, 16, 32, 64, 128].contains(&base) {
            return refuse(format!("the duration {} is not one this draws (1, 2, 4, 8, 16, 32, 64, 128)", toks[i].v), toks[i].at);
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
    let mut ev = Ev {
        k: Kind::Note,
        pitches,
        pitch_at,
        base,
        dots,
        ticks,
        at: tk.at,
        i: st.items,
        fingers: chord_fingers,
        tremolo,
        chord_name,
        figures,
        drums,
        strings,
        scripts,
        forced,
        cautionary,
        ..Ev::default()
    };
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
        // Inside another: that one's ratio too, its time counted.
        if let Some(mut outer) = st.tuplet_outer {
            if (ev.ticks * outer.d) % outer.n != 0 {
                return refuse(format!("a 1/{base} in these tuplets cannot be cut {} ways", outer.n), tk.at);
            }
            ev.ticks = ev.ticks * outer.d / outer.n;
            ev.tuplet_outer = Some(TupletRef { id: outer.id, n: outer.n, d: outer.d });
            outer.used += ev.ticks;
            st.tuplet_outer = Some(outer);
        }
    }
    if st.scale != (1, 1) {
        // `\scaleDurations 2/3`: the note that much shorter, no tuplet drawn.
        let (n, d) = st.scale;
        if (ev.ticks * n) % d != 0 {
            return refuse(format!("a 1/{base} cannot be scaled by {n}/{d}"), tk.at);
        }
        ev.ticks = ev.ticks * n / d;
        ev.scale = Some((n, d));
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
    if ev.k == Kind::Note {
        ev.cross = st.cross.clone();
        ev.follow = std::mem::take(&mut st.follow_next);
        ev.span_stem = st.span_stem;
    }
    ev.size = st.size;
    ev.stem_dir = st.stem;
    ev.no_autobeam = st.no_autobeam;
    ev.cue = st.cue;
    ev.look = st.look.stamp();
    st.events.push(ev);
    Ok(i)
}

/// `!` or `?` written against the pitch before `i` — `cis!`, `cis?` — onto
/// `forced` or `cautionary` (by the pitch's diatonic number `d`); the token
/// after it.
fn reminder(toks: &[Tok], i: usize, d: i64, forced: &mut Vec<i64>, cautionary: &mut Vec<i64>) -> usize {
    let against = i > 0 && toks.get(i).is_some_and(|t| t.at == tok_end(&toks[i - 1]));
    match toks.get(i).map(|t| t.t) {
        Some(T::Sym('!')) if against => {
            beyond();
            forced.push(d);
            i + 1
        }
        Some(T::Sym('?')) if against => {
            cautionary.push(d);
            i + 1
        }
        _ => i,
    }
}

/// A figure's lines from its `<` at `i`, and the token after its `>`: each
/// a number or `_` (none), then `+`, `-` or `!` — sharp, flat, natural —
/// set before it; `\\` and `/` (a stroke through it) are read and not drawn.
fn figure_lines(toks: &[Tok], mut i: usize) -> Result<(Vec<Vec<Run>>, usize), ScoreError> {
    let at = toks[i].at;
    i += 1;
    let mut lines: Vec<Vec<Run>> = Vec::new();
    loop {
        let Some(t) = toks.get(i) else { return refuse("a figure that is never closed", at) };
        let (number, mut j) = match t.t {
            T::Sym('>') => break,
            T::Num if !t.v.contains('/') => (Some(t.v.clone()), i + 1),
            T::Sym('_') => (None, i + 1),
            T::Sym('[' | ']') => {
                i += 1;
                continue;
            }
            _ => return refuse("a figure is a number or _, then +, - or !: <6 4+>", t.at),
        };
        let mut accidental = None;
        while let Some(m) = toks.get(j) {
            match m.t {
                T::Sym('+') => accidental = Some('♯'),
                T::Sym('-') => accidental = Some('♭'),
                T::Sym('!') => accidental = Some('♮'),
                T::Sym('\\' | '/' | ']') => {}
                _ => break,
            }
            j += 1;
        }
        // The accidental before the number, in the figures' own face.
        let text: String = accidental.into_iter().chain(number.unwrap_or_default().chars()).collect();
        lines.push(if text.is_empty() { Vec::new() } else { vec![Run::words(text, Weight::Regular)] });
        i = j;
    }
    if lines.is_empty() {
        return refuse("an empty figure: <_> holds none", at);
    }
    Ok((lines, i + 1))
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
pub(crate) fn letters(n: i64) -> String {
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
            "fret-diagram" | "fret-diagram-terse" => {
                // `\fret-diagram "6-x;5-3;…"`: a guitar's chord, drawn.
                let Some(spec) = toks.get(i + 1).filter(|s| s.t == T::Str) else { return refuse(format!("\\{} wants its strings in quotes", t.v), t.at) };
                let read = if t.v == "fret-diagram" { crate::frets::verbose(&spec.v) } else { crate::frets::terse(&spec.v) };
                let d = read.map_err(|why| ScoreError { message: format!("score: \\{} {why} (at character {})", t.v, spec.at) })?;
                runs.push(Run::words(format!("{}{}", crate::frets::FRET, d.spec()), weight));
                Ok(i + 2)
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

// ---- Headers and footers --------------------------------------------------------
//
// `\paper`'s `oddHeaderMarkup`, `evenHeaderMarkup`, `oddFooterMarkup` and
// `evenFooterMarkup`: words, the header's fields (`\fromproperty
// #'header:title`) and the page's number (`#'page:page-number-string`), in a
// `\fill-line` that spreads them, `\on-the-fly` naming the pages they go
// on. A command that only styles its words is read through; any other is
// passed over with a warning.

/// A page's line from `i`, just after `\markup`, and the token after it.
fn page_markup(toks: &[Tok], i: usize) -> Result<(PageLine, usize), ScoreError> {
    // `\markup { \fill-line { … } }` is `\markup \fill-line { … }`.
    let braced = is(toks, i, T::Sym('{')) && is_cmd(toks, i + 1, "fill-line");
    let at = if braced { i + 1 } else { i };
    if !is_cmd(toks, at, "fill-line") {
        let mut left = Vec::new();
        let j = page_item(toks, i, &mut left)?;
        return Ok((PageLine { left, ..PageLine::default() }, j));
    }
    let mut j = at + 1;
    let mut cells: Vec<Vec<PagePart>> = Vec::new();
    if is(toks, j, T::Sym('{')) {
        j += 1;
        while j < toks.len() && !is(toks, j, T::Sym('}')) {
            let mut cell = Vec::new();
            j = page_item(toks, j, &mut cell)?;
            cells.push(cell);
        }
        if j >= toks.len() {
            return refuse("a \\fill-line { that is never closed", toks[at].at);
        }
        j += 1;
    } else {
        let mut cell = Vec::new();
        j = page_item(toks, j, &mut cell)?;
        cells.push(cell);
    }
    if braced {
        if !is(toks, j, T::Sym('}')) {
            return refuse("a \\markup { that is never closed", toks[i].at);
        }
        j += 1;
    }
    let mut line = PageLine::default();
    match cells.len() {
        0 => {}
        1 => line.centre = cells.remove(0),
        2 => {
            line.right = cells.remove(1);
            line.left = cells.remove(0);
        }
        n => {
            line.right = cells.remove(n - 1);
            line.left = cells.remove(0);
            for (k, cell) in cells.into_iter().enumerate() {
                if k > 0 {
                    line.centre.push(PagePart::Text(" ".into()));
                }
                line.centre.extend(cell);
            }
        }
    }
    Ok((line, j))
}

/// The pages `#name` names, for `\on-the-fly`; none for a procedure this
/// does not know, whose words are then drawn on every page.
fn page_cond(name: &str) -> Option<PageCond> {
    Some(match name {
        "first-page" => PageCond::First,
        "not-first-page" | "print-page-number-check-first" => PageCond::NotFirst,
        "last-page" => PageCond::Last,
        "not-last-page" => PageCond::NotLast,
        _ => return None,
    })
}

/// One item of a header's markup onto `parts`; the token after it.
fn page_item(toks: &[Tok], i: usize, parts: &mut Vec<PagePart>) -> Result<usize, ScoreError> {
    let Some(t) = toks.get(i) else { return refuse("a \\markup that is never finished", at_of(toks, i.saturating_sub(1))) };
    match t.t {
        T::Sym('{') => page_group(toks, i, parts, true),
        T::Str | T::Word | T::Num => {
            parts.push(PagePart::Text(t.v.clone()));
            Ok(i + 1)
        }
        T::Sym(c) if ".,;:!?-+=*/()[]|&".contains(c) => {
            parts.push(PagePart::Text(c.to_string()));
            Ok(i + 1)
        }
        T::Cmd => match t.v.as_str() {
            "null" => Ok(i + 1),
            "line" | "fill-line" => page_group(toks, i + 1, parts, true),
            "concat" => page_group(toks, i + 1, parts, false),
            "fromproperty" => {
                // `#'header:title`: `#`, `'`, the group, `:`, the name.
                let mut j = i + 1;
                while is(toks, j, T::Sym('#')) || is(toks, j, T::Sym('\'')) {
                    j += 1;
                }
                let group = toks.get(j).filter(|w| w.t == T::Word).map(|w| w.v.clone()).unwrap_or_default();
                let name = toks.get(j + 2).filter(|w| w.t == T::Word && is(toks, j + 1, T::Sym(':'))).map(|w| w.v.clone()).unwrap_or_default();
                match (group.as_str(), name.as_str()) {
                    ("page", "page-number-string") => parts.push(PagePart::Page),
                    ("header", field) if !field.is_empty() => parts.push(PagePart::Field(field.to_string())),
                    _ => warn(format!("\\fromproperty #'{group}:{name} is not drawn"), t.at),
                }
                Ok(if name.is_empty() { j + 1 } else { j + 3 })
            }
            "on-the-fly" => {
                let name = toks.get(i + 2).filter(|w| w.t == T::Word && is(toks, i + 1, T::Sym('#'))).map(|w| w.v.clone()).unwrap_or_default();
                let j = if is(toks, i + 1, T::Sym('#')) { scheme_end(toks, i + 1)? } else { i + 1 };
                let mut inner = Vec::new();
                let j = page_item(toks, j, &mut inner)?;
                match page_cond(&name) {
                    Some(cond) => parts.push(PagePart::Only(cond, inner)),
                    None => {
                        warn(format!("\\on-the-fly #{name} is not read: its words are on every page"), t.at);
                        parts.extend(inner);
                    }
                }
                Ok(j)
            }
            "bold" | "italic" | "upright" | "normal-text" | "roman" | "medium" | "small" | "smaller" | "large" | "larger" | "huge" | "tiny" | "teeny" | "normalsize" | "sans" | "serif" | "caps"
            | "smallCaps" | "box" | "center-align" | "left-align" | "right-align" | "center-column" | "column" | "underline" => page_item(toks, i + 1, parts),
            "fontsize" | "abs-fontsize" | "raise" | "lower" | "halign" | "with-color" | "translate" | "magnify" | "override" => {
                let mut j = i + 1;
                if is(toks, j, T::Sym('#')) {
                    j = scheme_end(toks, j)?;
                } else {
                    j += 1;
                }
                page_item(toks, j, parts)
            }
            "hspace" | "vspace" => Ok(if is(toks, i + 1, T::Sym('#')) { scheme_end(toks, i + 1)? } else { i + 2 }),
            other => {
                warn(format!("\\{other} in a header or footer is not drawn"), t.at);
                markup_skip(toks, i)
            }
        },
        T::Sym('#') => {
            warn("Scheme in a header or footer is not drawn", t.at);
            scheme_end(toks, i)
        }
        _ => refuse(format!("'{}' is not markup this reads", t.v), t.at),
    }
}

/// A `{ … }` at `i` (or the one item there): its items, a space between
/// two when `spaced` (`\line`), none when not (`\concat`).
fn page_group(toks: &[Tok], i: usize, parts: &mut Vec<PagePart>, spaced: bool) -> Result<usize, ScoreError> {
    if !is(toks, i, T::Sym('{')) {
        return page_item(toks, i, parts);
    }
    let mut j = i + 1;
    let mut first = true;
    while j < toks.len() && !is(toks, j, T::Sym('}')) {
        let mut one = Vec::new();
        j = page_item(toks, j, &mut one)?;
        if one.is_empty() {
            continue;
        }
        if spaced && !first {
            parts.push(PagePart::Text(" ".into()));
        }
        parts.extend(one);
        first = false;
    }
    if j >= toks.len() {
        return refuse("a \\markup { that is never closed", toks[i].at);
    }
    Ok(j + 1)
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
