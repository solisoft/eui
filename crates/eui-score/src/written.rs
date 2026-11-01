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
//!   notes, a staff, a verse — which refuses early, and may count what is
//!   read and then not drawn (an `\addQuote`'s music);
//! * the finished one ([`finished`]): the staves as layout gets them, every
//!   event of them, each staff as long as the longest — what [`Written`]
//!   reports.

use std::cell::Cell;

use crate::parse::{refuse, Ev, Kind, Parsed, SHOWN};
use crate::{ScoreError, WHOLE};

/// The most a source may write out, in notes: every note, rest and skip, a
/// chord's every head, grace notes, lyric syllables, each bar a long rest
/// or skip spans, and the other things a staff draws (a clef, a bar line),
/// as the music is played — repeats written out, variables at each use,
/// each staff as long as the longest. The score app's largest piece writes
/// out to 555 — 36 times under; a piece just under it lays out in under
/// 0.1 s and prints in a third of a second.
pub const MAX_WRITTEN_EVENTS: usize = 20_000;

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
fn grouped(n: usize) -> String {
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
}

/// A new source: nothing spent.
pub(crate) fn reset() {
    SPENT.with(|s| s.set(Written::default()));
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
pub(crate) fn note_weight(heads: usize, ticks: i64, bar: i64) -> usize {
    let bar = bar.max(1);
    let ticks = ticks.max(0);
    let bars = usize::try_from(ticks / bar + i64::from(ticks % bar != 0)).unwrap_or(usize::MAX).max(1);
    heads.max(1).saturating_mul(bars)
}

/// The characters of a string.
pub(crate) fn chars(s: &str) -> usize {
    s.chars().count()
}

/// What one event counts, in bars of `bar` ticks: its notes' worth and its
/// words, its grace notes with it.
fn event(ev: &Ev, bar: i64) -> Written {
    let events = match ev.k {
        Kind::Note | Kind::Rest => {
            let span = note_weight(ev.pitches.len(), ev.ticks.saturating_mul(ev.count.max(1)), bar);
            // `R4*16` in 4/4 is four bars, each counted bar a rest of its own.
            if ev.full {
                span.max(usize::try_from(ev.count).unwrap_or(0))
            } else {
                span
            }
        }
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

/// How many bars a staff's music lasts: its voices side by side, each
/// `\time` the bars after it.
fn bars(evs: &[Ev]) -> usize {
    let mut bar = WHOLE;
    let mut pos = 0.0f64;
    let mut stack: Vec<(f64, f64)> = Vec::new();
    for ev in evs {
        match ev.k {
            Kind::Time if ev.v != SHOWN => bar = bar_ticks(ev.time.0, ev.time.1),
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
            Kind::Note | Kind::Rest if ev.grace.is_none() => {
                #[allow(clippy::cast_precision_loss)]
                let part = ev.ticks.saturating_mul(ev.count.max(1)) as f64 / bar as f64;
                pos += part;
            }
            _ => {}
        }
    }
    let end = stack.iter().fold(pos, |a, s| a.max(s.1));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = end.ceil().min(1e15) as usize;
    n
}

/// What the parsed source writes out, as layout gets it: each staff's
/// events and words, as long as the longest staff (a staff is drawn across
/// every bar of the score), its lyrics, the headers.
pub(crate) fn finished(p: &Parsed) -> Written {
    let longest = p.staves.iter().map(|s| bars(&s.events)).max().unwrap_or(0);
    let mut w = Written::default();
    for s in &p.staves {
        let mut own = cost(&s.events, WHOLE);
        own.events = own.events.max(longest);
        for verse in s.lyrics.iter().chain(&s.verses) {
            own = own.plus(Written { events: verse.len(), text: verse.iter().map(|y| chars(&y.text)).sum() });
        }
        for name in [&s.props.instrument_name, &s.props.short_instrument_name].into_iter().flatten() {
            own.text = own.text.saturating_add(chars(name));
        }
        w = w.plus(own);
    }
    w.text = w.text.saturating_add(p.header.values().map(|v| chars(v)).sum());
    w
}
