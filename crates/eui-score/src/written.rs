//! What a source writes out, counted while it is read (score #540).
//!
//! A few lines of LilyPond can stand for a great deal of music: a repeat in
//! a repeat in a repeat, a variable used in a variable used in a variable,
//! a whole-bar rest counted a million times, a long markup on every note of
//! a repeat. Each is read in moments and costs the layout everything after
//! it. So the reading keeps an account of what layout will have to draw —
//! every note, rest and skip (a chord's every head, `q`'s too), grace notes,
//! lyric syllables, the bars a long rest or skip spans in its metre, the
//! characters of the words — and stops at [`MAX_WRITTEN_EVENTS`] and
//! [`MAX_WRITTEN_TEXT`], before the music past them is made: a repeat is
//! counted before it is written out.
//!
//! Two accounts, kept apart on purpose:
//!
//! * the running one ([`spend`]): charged as the music is read — a note
//!   where it is read, a repeat's copies before they are made, a cue's
//!   notes, a staff, a verse — which refuses early. A marking written after
//!   its note (`c4->`, `c4(`) is counted with every copy of the note and in
//!   the finished account, by the same [`note_weight`]; a source of plain
//!   writing is no longer than its tokens, which are bounded already.
//!   `\addQuote`'s music is read on an account of its own ([`quoting`]):
//!   it is drawn only where a cue copies it, and counted there;
//! * the finished one ([`finished`]): the staves as layout gets them, every
//!   event of them, each staff as long as the longest — what [`Written`]
//!   reports.

use std::cell::Cell;

use crate::parse::{refuse, Ev, Kind, Parsed, SHOWN};
use crate::{ScoreError, WHOLE};

/// The most a source may write out, in notes: every note, rest and skip, a
/// chord's every head, grace notes, lyric syllables, each bar a long rest
/// or skip spans, the markings on the notes ([`markings`]), and the other
/// things a staff draws (a clef, a bar line), as the music is played —
/// repeats written out, variables at each use, each staff as long as the
/// longest.
///
/// Measured on this engraver (release build, an M-series Mac): 16,384
/// plain quarter notes lay out in 52 ms and print in 0.34 s (7 MB), so
/// plain notes at the budget in some 65 ms and 0.42 s. Slurs cost far more
/// and grow faster than their number: 8,192 slurs over 16,384 notes took
/// 0.64 s to lay out and 2.1 s to print — hence [`SLUR`]. At the budget
/// now, slurred music (1,664 slurs on 3,328 notes) lays out in 25 ms and
/// prints in 0.21 s; a trill on every note (6,656 notes) in 31 ms and
/// 0.62 s; a dynamic on every note in 29 ms and 0.69 s. The score app's
/// largest piece (symphony-5) writes out to 765, 26 times under.
pub const MAX_WRITTEN_EVENTS: usize = 20_000;

/// What a slur or a phrasing slur counts, where it starts: its curve is
/// placed among many shapes, each scored against the notes under it.
pub const SLUR: usize = 10;

/// What an articulation, an ornament or a fermata counts: a glyph placed
/// clear of the others on the note (a trill costs twice an accent).
pub const SCRIPT: usize = 2;

/// The most characters of words a source may write out: markups, `^"…"`,
/// marks, tempo words, chord names, figures, lyrics, headers, instrument
/// names — each as often as it is drawn. Words cost more than notes to
/// draw: each letter is a glyph's outline, on the screen and in the PDF.
/// Measured on this engraver, 192,000 characters take 2.8 s and 143 MB to
/// print where 20,000 notes take 0.33 s and 11 MB; 40,000 characters, two
/// a note, cost about what the notes' budget does. The score app's wordiest
/// piece writes 266; four verses of a long hymn some 6,000; a
/// 200-character markup on a note repeated 4,096 times, 819,200.
pub const MAX_WRITTEN_TEXT: usize = 40_000;

/// What a source writes out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Written {
    /// Notes, rests, skips, heads, syllables, bars spanned: what
    /// [`MAX_WRITTEN_EVENTS`] counts.
    pub events: usize,
    /// Characters of words: what [`MAX_WRITTEN_TEXT`] counts.
    pub text: usize,
}

impl Written {
    fn plus(self, o: Written) -> Written {
        Written { events: self.events.saturating_add(o.events), text: self.text.saturating_add(o.text) }
    }

    fn times(self, n: usize) -> Written {
        Written { events: self.events.saturating_mul(n), text: self.text.saturating_mul(n) }
    }

    fn minus(self, o: Written) -> Written {
        Written { events: self.events.saturating_sub(o.events), text: self.text.saturating_sub(o.text) }
    }

    /// The refusal past either budget, at character `at`.
    pub(crate) fn within(self, at: usize) -> Result<(), ScoreError> {
        if self.events > MAX_WRITTEN_EVENTS {
            return refuse(format!("this music writes out to more than {} notes; the engraver stops there", grouped(MAX_WRITTEN_EVENTS)), at);
        }
        if self.text > MAX_WRITTEN_TEXT {
            return refuse(format!("this music writes out to more than {} characters of words; the engraver stops there", grouped(MAX_WRITTEN_TEXT)), at);
        }
        Ok(())
    }
}

/// `20000` as `20,000`.
pub(crate) fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (k, c) in digits.chars().enumerate() {
        if k > 0 && (digits.len() - k).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

thread_local! {
    /// The running account of the source being read.
    static SPENT: Cell<Written> = const { Cell::new(Written { events: 0, text: 0 }) };
    /// The account of its `\addQuote`s' music: read and kept, drawn only
    /// where a cue copies it (and counted there), held to a budget of its own.
    static QUOTED: Cell<Written> = const { Cell::new(Written { events: 0, text: 0 }) };
}

/// A new source: nothing spent.
pub(crate) fn reset() {
    SPENT.with(|s| s.set(Written::default()));
    QUOTED.with(|s| s.set(Written::default()));
}

/// `read` run on the quotes' account instead of the music's: an
/// `\addQuote`'s music is not drawn where it is written.
/// Past the quotes' budget, the refusal says a quote is what wrote it out.
pub(crate) fn quoting<T>(read: impl FnOnce() -> Result<T, ScoreError>) -> Result<T, ScoreError> {
    let music = SPENT.with(|s| s.replace(QUOTED.with(Cell::get)));
    let out = read();
    let quoted = SPENT.with(|s| s.replace(music));
    QUOTED.with(|q| q.set(quoted));
    out.map_err(|mut e| {
        if quoted.within(0).is_err() {
            e.message = e.message.replacen("score: this music", "score: the \\addQuote music", 1);
        }
        e
    })
}

/// `w` more written out, the source's character `at` writing it; the
/// refusal once the running account is past a budget.
pub(crate) fn spend(w: Written, at: usize) -> Result<(), ScoreError> {
    let now = SPENT.with(|s| {
        let now = s.get().plus(w);
        s.set(now);
        now
    });
    now.within(at)
}

/// `n` more notes' worth.
pub(crate) fn spend_events(n: usize, at: usize) -> Result<(), ScoreError> {
    spend(Written { events: n, text: 0 }, at)
}

/// `n` more characters of words.
pub(crate) fn spend_text(n: usize, at: usize) -> Result<(), ScoreError> {
    spend(Written { events: 0, text: n }, at)
}

/// A bar's length in ticks under `\time num/den`.
pub(crate) fn bar_ticks(num: i64, den: i64) -> i64 {
    if den <= 0 || num <= 0 {
        return WHOLE;
    }
    (WHOLE / den).saturating_mul(num).max(1)
}

/// What a note, rest or skip of `heads` heads (none for a rest), `ticks`
/// long in all, counts in bars of `bar` ticks: each head once for every
/// bar it spans — a whole note in 2/4 is drawn as two tied halves, `R1*16`
/// is sixteen bars, `s1*16` sixteen bars of a staff to draw.
pub(crate) fn span_weight(heads: usize, ticks: i64, bar: i64) -> usize {
    let bar = bar.max(1);
    let ticks = ticks.max(0);
    let bars = usize::try_from(ticks / bar + i64::from(ticks % bar != 0)).unwrap_or(usize::MAX).max(1);
    heads.max(1).saturating_mul(bars)
}

/// What the markings written on a note count, each one drawn: an
/// articulation or ornament [`SCRIPT`], a slur's or phrasing slur's start
/// [`SLUR`], a dynamic one and a letter more, a hairpin's start and end, a
/// tie, a text spanner's ends, an arpeggio, a glissando, a tremolo, the
/// pedal, a breath, a beam, each fingering, string number, harmonic and
/// reminder accidental. Their words are counted with the other words.
pub(crate) fn markings(ev: &Ev) -> usize {
    let flags = [
        ev.tie,
        ev.lv,
        ev.repeat_tie,
        ev.hairpin.is_some(),
        ev.hairpin_end,
        ev.span_start.is_some(),
        ev.span_stop,
        ev.arpeggio,
        ev.gliss,
        ev.tremolo > 0,
        ev.pedal_on,
        ev.pedal_off,
        ev.breath.is_some(),
        ev.beam_on,
    ];
    let mut n = flags.iter().filter(|f| **f).count();
    n = n.saturating_add(ev.scripts.len().saturating_mul(SCRIPT));
    n = n.saturating_add((usize::from(ev.slur_on) + usize::from(ev.phrase_on)) * SLUR);
    if let Some(d) = &ev.dynamic {
        n = n.saturating_add(1 + chars(d));
    }
    n.saturating_add(ev.fingers.len()).saturating_add(ev.strings.len()).saturating_add(ev.harmonics.len()).saturating_add(ev.forced.len()).saturating_add(ev.cautionary.len())
}

/// What a note, rest or skip counts in bars of `bar` ticks, without its
/// grace notes: its heads for every bar it spans (a counted bar's rest
/// each bar of it), and its [`markings`]. The one weight, used where a note
/// is read and wherever its copies are counted.
pub(crate) fn note_weight(ev: &Ev, bar: i64) -> usize {
    let span = span_weight(ev.pitches.len(), ev.ticks.saturating_mul(ev.count.max(1)), bar);
    // `R4*16` in 4/4 is four bars, each counted bar a rest of its own.
    let span = if ev.full { span.max(usize::try_from(ev.count).unwrap_or(usize::MAX)) } else { span };
    span.saturating_add(markings(ev))
}

/// What a `\key` counts where it changes: its sharps or flats, and the
/// naturals cancelling the key before it — seven at most, counted as seven.
pub(crate) fn key_weight(fifths: i64) -> usize {
    1 + usize::try_from(fifths.unsigned_abs()).unwrap_or(7).min(7) + 7
}

/// The characters of a string.
pub(crate) fn chars(s: &str) -> usize {
    s.chars().count()
}

/// What one event counts, in bars of `bar` ticks: its notes' worth and its
/// words, its grace notes with it.
fn event(ev: &Ev, bar: i64) -> Written {
    let events = match ev.k {
        Kind::Note | Kind::Rest => note_weight(ev, bar),
        Kind::Key => key_weight(ev.fifths),
        _ => 1,
    };
    let mut text = 0usize;
    for w in &ev.texts {
        text = text.saturating_add(if w.runs.is_empty() { chars(&w.text) } else { w.runs.iter().map(|r| chars(&r.text)).sum() });
    }
    text = text.saturating_add(ev.chord_name.iter().map(|r| chars(&r.text)).sum());
    text = text.saturating_add(ev.figures.iter().flatten().map(|r| chars(&r.text)).sum());
    text = text.saturating_add(ev.fingers.iter().map(|f| chars(&f.text)).sum());
    if let Some(t) = &ev.tempo {
        text = text.saturating_add(chars(&t.text));
    }
    text = text.saturating_add(ev.span_start.iter().flatten().map(|r| chars(&r.text)).sum());
    text = text.saturating_add(ev.hairpin_text.as_deref().map_or(0, chars));
    let mut w = Written { events, text };
    for g in ev.graces.iter().chain(&ev.after_graces) {
        w = w.plus(event(g, bar));
    }
    w
}

/// What a run of events counts, its bars `bar` ticks long where it starts;
/// a `\time` among them changes the bars after it.
pub(crate) fn cost(evs: &[Ev], mut bar: i64) -> Written {
    let mut w = Written::default();
    for ev in evs {
        if ev.k == Kind::Time && ev.v != SHOWN {
            bar = bar_ticks(ev.time.0, ev.time.1);
        }
        w = w.plus(event(ev, bar));
    }
    w
}

/// `\repeat unfold count`'s copies, before they are made: the body each
/// time, the ending `ending(time)` names after it; less what was already
/// counted when the body and the endings were read, once each.
pub(crate) fn unfolded(body: &Written, endings: &[Written], count: usize, ending: impl Fn(usize) -> Option<usize>) -> Written {
    let mut all = body.times(count);
    for time in 0..count {
        if let Some(e) = ending(time).and_then(|k| endings.get(k)) {
            all = all.plus(*e);
        }
    }
    let once = endings.iter().fold(*body, |a, e| a.plus(*e));
    all.minus(once)
}

/// How long a staff's music lasts in ticks, its voices side by side, and
/// where its `\time`s change the bars: (tick, a bar's ticks from there).
fn timeline(evs: &[Ev]) -> (i64, Vec<(i64, i64)>) {
    let mut pos = 0i64;
    let mut changes = Vec::new();
    let mut stack: Vec<(i64, i64)> = Vec::new();
    for ev in evs {
        match ev.k {
            Kind::Time if ev.v != SHOWN => changes.push((pos, bar_ticks(ev.time.0, ev.time.1))),
            Kind::Voices => stack.push((pos, pos)),
            Kind::VoiceNext => {
                if let Some(top) = stack.last_mut() {
                    top.1 = top.1.max(pos);
                    pos = top.0;
                }
            }
            Kind::VoicesEnd => {
                if let Some((_, end)) = stack.pop() {
                    pos = pos.max(end);
                }
            }
            Kind::Note | Kind::Rest if ev.grace.is_none() => pos = pos.saturating_add(ev.ticks.saturating_mul(ev.count.max(1))),
            _ => {}
        }
    }
    (stack.iter().fold(pos, |a, s| a.max(s.1)), changes)
}

/// How many bars the score lasts: its longest staff, in the metre any staff
/// sets — a `\time` is the whole score's, `\time 1/64` on one staff cuts
/// the other's `R1*300` into 19,200 bars.
fn score_bars(p: &Parsed) -> usize {
    let lines: Vec<(i64, Vec<(i64, i64)>)> = p.staves.iter().map(|s| timeline(&s.events)).collect();
    let end = lines.iter().map(|l| l.0).max().unwrap_or(0);
    let mut changes: Vec<(i64, i64)> = lines.into_iter().flat_map(|l| l.1).collect();
    // At one tick, the shortest bar any staff sets.
    changes.sort_unstable();
    changes.dedup_by_key(|c| c.0);
    let (mut bars, mut at, mut bar) = (0i64, 0i64, WHOLE);
    for (tick, next) in changes.into_iter().filter(|c| c.0 < end) {
        bars = bars.saturating_add(ceil_div((tick - at).max(0), bar));
        (at, bar) = (tick, next.max(1));
    }
    bars = bars.saturating_add(ceil_div((end - at).max(0), bar));
    usize::try_from(bars).unwrap_or(usize::MAX)
}

/// `a / b` rounded up, for ticks (`b` > 0).
fn ceil_div(a: i64, b: i64) -> i64 {
    a / b + i64::from(a % b != 0)
}

/// What the parsed source writes out, as layout gets it: each staff's
/// events and words, as long as the longest staff (a staff is drawn across
/// every bar of the score), its lyrics, the headers.
pub(crate) fn finished(p: &Parsed) -> Written {
    let longest = score_bars(p);
    let mut w = Written::default();
    for s in &p.staves {
        let mut own = cost(&s.events, WHOLE);
        own.events = own.events.max(longest);
        for verse in s.lyrics.iter().chain(&s.verses) {
            own = own.plus(Written { events: verse.len(), text: verse.iter().map(|y| chars(&y.text)).sum() });
        }
        // The name before the first system, and the short one before each
        // system after it: a system for every four bars, about.
        let systems = longest.div_ceil(4).max(1);
        // Each system opens with the staff's clef and key again (its
        // metre only once): the most sharps or flats the staff has.
        let fifths = s.events.iter().filter(|e| e.k == Kind::Key).map(|e| usize::try_from(e.fifths.unsigned_abs()).unwrap_or(7).min(7)).max().unwrap_or(0);
        own.events = own.events.saturating_add(systems.saturating_mul(1 + fifths));
        own.text = own.text.saturating_add(s.props.instrument_name.as_deref().map_or(0, chars));
        own.text = own.text.saturating_add(s.props.short_instrument_name.as_deref().map_or(0, chars).saturating_mul(systems));
        w = w.plus(own);
    }
    w.text = w.text.saturating_add(p.header.values().map(|v| chars(v)).sum());
    w
}
