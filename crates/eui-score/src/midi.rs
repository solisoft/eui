//! A score as a Standard MIDI File, as LilyPond's `\midi` writes it
//! (`eui_builders_score.sl`, "MIDI"): what is written, played once. A
//! quarter is 6 720 ticks, the engraver's own, so a tuplet lands exactly
//! where it is drawn. Several staves are a format-1 file, a track each.

use crate::measure::{measures, Bar};
use crate::parse::{modulo, parse, Kind, Props, StaffKind};
use crate::{ScoreError, WHOLE};

/// What a source has not said.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiOptions {
    /// Beats a minute for a source with no `\tempo`; 0 writes none.
    pub bpm: f64,
    /// The note value a beat is.
    pub beat: i64,
    /// The General MIDI instrument of a staff that names none.
    pub program: i64,
}

impl Default for MidiOptions {
    fn default() -> Self {
        Self { bpm: 0.0, beat: 4, program: 0 }
    }
}

/// The file's bytes.
pub fn midi(src: &str, opts: &MidiOptions) -> Result<Vec<u8>, ScoreError> {
    let parsed = parse(src)?;
    let title = parsed.header.get("title").cloned().unwrap_or_default();
    let mut measured = Vec::new();
    // A `\time` or a `\partial` written in one staff is the score's.
    let timed = crate::measure::score_timing(&parsed.staves, parsed.paper.polymeter);
    for (stf, timed) in parsed.staves.iter().zip(&timed) {
        measured.push(measures(timed.as_deref().unwrap_or(&stf.events))?);
    }
    // `\midi { \tempo 4 = 100 }`: the source's own tempo, over the host's.
    let source_tempo = parsed.paper.midi_tempo.map(|(bpm, beat)| MidiOptions { bpm: bpm as f64, beat, ..*opts });
    let opts = source_tempo.as_ref().unwrap_or(opts);
    let holds = holds(&measured, opts);
    let mut tracks: Vec<Vec<u8>> = Vec::new();
    for (si, stf) in parsed.staves.iter().enumerate() {
        // Chord names are read, not played, as LilyPond's \\midi has them.
        // A figured bass, too: the bass it figures is played.
        if (stf.kind == StaffKind::ChordNames && parsed.staves.len() > 1) || matches!(stf.kind, StaffKind::Figures | StaffKind::FretBoards | StaffKind::Ossia) {
            continue;
        }
        let props: &Props = &stf.props;
        let program = props.midi_instrument.as_deref().and_then(program_of).unwrap_or(opts.program);
        let name = props.instrument_name.clone().unwrap_or_else(|| if si == 0 { title.clone() } else { String::new() });
        // A drum staff plays on General MIDI's percussion channel, the tenth.
        let channel = if stf.kind == StaffKind::Drums { 9 } else { (if si < 9 { si } else { si + 1 }).min(15) as i64 };
        // An instrument changed in the middle of the staff (score #1023):
        // its program and its transposition from where each is written.
        // Drums have neither.
        let voice = Voice {
            program,
            transposition: if channel == 9 { 0 } else { props.transposition },
            programs: if channel == 9 { Vec::new() } else { crate::look::changes(&stf.events, crate::look::MIDI_INSTRUMENT) },
            transpositions: if channel == 9 { Vec::new() } else { crate::look::changes(&stf.events, crate::look::TRANSPOSITION) },
        };
        let track = track(&measured[si], opts, &holds, &voice, &name, channel, tracks.is_empty());
        let mut controls: Vec<u8> = Vec::new();
        if let Some(v) = props.midi_maximum_volume {
            controls.extend([0, (176 + channel) as u8, 7, (v.clamp(0.0, 1.0) * 127.0 + 0.5) as u8]);
        }
        if let Some(p) = props.midi_pan_position {
            controls.extend([0, (176 + channel) as u8, 10, (64.0 + p.clamp(-1.0, 1.0) * 63.0 + 0.5) as u8]);
        }
        controls.extend(track);
        tracks.push(controls);
    }
    let quarter = WHOLE / 4 / MIDI_SCALE;
    let mut out: Vec<u8> = vec![77, 84, 104, 100, 0, 0, 0, 6, 0, if tracks.len() > 1 { 1 } else { 0 }];
    out.extend(be(tracks.len() as i64, 2));
    out.extend(be(quarter, 2));
    for t in tracks {
        out.extend([77, 84, 114, 107]);
        out.extend(be(t.len() as i64, 4));
        out.extend(t);
    }
    Ok(out)
}

/// The file's ticks are the engraver's divided by this: a quarter of 6720,
/// as before WHOLE took a second factor of three for nested triplets — the
/// files stay what they were, and only a triplet in a triplet rounds.
const MIDI_SCALE: i64 = 3;

struct Sounding {
    until: i64,
    tied: bool,
    off: usize,
}

struct Hairpin {
    left: i64,
    step: f64,
}

/// What a staff sounds as: its General MIDI program and how far from
/// where it is written, at its start, and where each changes after.
struct Voice {
    program: i64,
    transposition: i64,
    programs: Vec<(i64, crate::look::Val)>,
    transpositions: Vec<(i64, crate::look::Val)>,
}

/// The notes struck and let go in `ev_out` from `from` on, `by` semitones
/// from where they are written (`\transposition`).
fn transpose(ev_out: &mut [(i64, i64, Vec<u8>)], from: usize, by: i64) {
    if by == 0 {
        return;
    }
    for e in ev_out.iter_mut().skip(from) {
        if let [status, key, ..] = e.2.as_mut_slice() {
            if matches!(*status & 0xF0, 0x80 | 0x90) {
                *key = (i64::from(*key) + by).clamp(0, 127) as u8;
            }
        }
    }
}

fn track(bars: &[Bar], opts: &MidiOptions, holds: &[Hold], voice: &Voice, name: &str, channel: i64, first_track: bool) -> Vec<u8> {
    let program = voice.program;
    let mut ev_out: Vec<(i64, i64, Vec<u8>)> = Vec::new();
    if !name.is_empty() {
        let mut meta = vec![255, 3];
        meta.extend(vlq(name.len() as i64));
        meta.extend(name.bytes());
        ev_out.push((0, 0, meta));
    }
    ev_out.push((0, 0, vec![(192 + channel) as u8, program as u8]));
    if opts.bpm > 0.0 && first_track {
        ev_out.push((0, 0, tempo(opts.bpm, opts.beat)));
    }
    let mut at = 0;
    // From which of `ev_out` the notes are still as written, and how far
    // from it they sound.
    let mut shift = (0, 0);
    let mut volume: f64 = 90.0;
    let mut hairpin: Option<Hairpin> = None;
    let mut sounding: Vec<(i64, Sounding)> = Vec::new();
    // The notes left out of the staff tie among themselves: a part's own
    // tie, not one to the note drawn on the same key.
    let mut sounding_unseen: Vec<(i64, Sounding)> = Vec::new();
    // Each bar's ticks from the staff's start, as written and as played.
    let mut starts: Vec<(i64, i64)> = Vec::with_capacity(bars.len());
    for (bi, bar) in bars.iter().enumerate() {
        starts.push((bar.tick0, at));
        // A metre changed in the middle of the bar before shows at none.
        if bi == 0 || bar.changes.time || bar.time != bars[bi - 1].time {
            ev_out.push((at, 0, vec![255, 88, 4, bar.time.0 as u8, log2(bar.time.1) as u8, 24, 8]));
        }
        if bi == 0 || bar.changes.key {
            ev_out.push((at, 0, vec![255, 89, 2, modulo(bar.fifths, 256) as u8, 0]));
        }
        for ev in &bar.items {
            let t0 = at + ev.start;
            // What the item before struck sounds where its instrument
            // does; this one's, where the staff's is by now.
            transpose(&mut ev_out, shift.0, shift.1);
            let sounds = match crate::look::in_force(&voice.transpositions, bar.tick0 + ev.start) {
                Some(crate::look::Val::Num(n)) => *n as i64,
                _ => voice.transposition,
            };
            shift = (ev_out.len(), sounds);
            let span = ev.ticks * ev.mm.unwrap_or(1);
            if let Some(mark) = &ev.tempo {
                if mark.bpm > 0 && first_track {
                    ev_out.push((t0, 0, tempo(mark.bpm as f64, mark.base)));
                }
            }
            if let Some(d) = &ev.dynamic {
                if let Some(v) = volume_of(d) {
                    volume = v as f64;
                }
                hairpin = None;
            }
            if let Some(h) = &mut hairpin {
                if h.left > 0 {
                    volume = (volume + h.step).clamp(8.0, 127.0);
                    h.left -= 1;
                }
            }
            if ev.hairpin.is_some() {
                hairpin = Some(hairpin_of(bars, bi, ev, volume));
            }
            if ev.hairpin_end {
                hairpin = None;
            }
            // Grace notes sound before the note they go to, in the time they
            // take from it — at most half of it.
            let mut stolen = 0;
            let written: i64 = ev.graces.iter().map(|g| g.ticks).sum();
            if written > 0 {
                let take = written.min(span / 2).max(1);
                let vel = (volume + 0.5).clamp(1.0, 127.0) as u8;
                let mut t = t0;
                for g in &ev.graces {
                    let len = (g.ticks * take / written).max(1);
                    for hd in &g.heads {
                        let key = key_of(hd.d, hd.alter);
                        ev_out.push((t, 1, vec![(144 + channel) as u8, key as u8, vel]));
                        ev_out.push((t + len, 0, vec![(128 + channel) as u8, key as u8, 64]));
                    }
                    t += len;
                }
                stolen = take;
            }
            // The sustain pedal, controller 64, with the note it is written
            // on: let go before the notes there start, pressed with them.
            if ev.pedal_off {
                ev_out.push((t0, 0, vec![(176 + channel) as u8, 64, 0]));
            }
            if ev.pedal_on {
                ev_out.push((t0, 1, vec![(176 + channel) as u8, 64, 127]));
            }
            // The sostenuto (66) and the soft pedal (67) the same way
            // (score #1026).
            for &(cc, on) in &ev.pedals {
                ev_out.push((t0, i64::from(on), vec![(176 + channel) as u8, cc, if on { 127 } else { 0 }]));
            }
            let t0 = t0 + stolen;
            let span = span - stolen;
            // What `\partCombine` leaves out of the staff, played all the
            // same (score #412).
            for (off, u) in &ev.unseen {
                play(&mut ev_out, &mut sounding_unseen, u, at + ev.start + off, u.ticks, volume, channel, bar.fifths);
            }
            // A cue is another part's notes, to read: not played.
            if ev.k != Kind::Note || ev.cue {
                continue;
            }
            play(&mut ev_out, &mut sounding, ev, t0, span, volume, channel, bar.fifths);
        }
        at += bar.length * bar.span.unwrap_or(1);
    }
    transpose(&mut ev_out, shift.0, shift.1);
    // The program where the staff takes another instrument: one this
    // knows, and not the one it has.
    let mut current = program;
    for (t, name) in &voice.programs {
        let crate::look::Val::Sym(name) = name else { continue };
        let Some(next) = program_of(name).filter(|next| *next != current) else { continue };
        current = next;
        if *t > 0 {
            let at = starts.iter().rev().find(|s| s.0 <= *t).map_or(*t, |s| s.1 + (*t - s.0));
            ev_out.push((at, 0, vec![(192 + channel) as u8, next as u8]));
        }
    }
    if !holds.is_empty() {
        for e in &mut ev_out {
            // A note let go where a fermata ends is let go once it is held;
            // what starts there waits for the breath too (score #977).
            let off = e.2.first().is_some_and(|b| b & 0xF0 == 0x80);
            e.0 = held(holds, e.0, off);
        }
    }
    ev_out.sort_by_key(|e| e.0 * 4 + e.1);
    let mut out = Vec::new();
    let mut last = 0;
    for (t, _, bytes) in ev_out {
        // Written at the file's resolution (MIDI_SCALE), from absolute times: no drift.
        let at = (t + MIDI_SCALE / 2) / MIDI_SCALE;
        out.extend(vlq(at - last));
        out.extend(bytes);
        last = at;
    }
    out.extend([0, 255, 47, 0]);
    out
}

/// A note's keys sounded from `t0` for `span` ticks: its scripts' lengths,
/// its `\afterGrace` notes, its tremolo, a tie into it held on. A note not
/// laid out in a bar (one `\partCombine` leaves out) has no heads yet:
/// its pitches are its keys.
#[allow(clippy::too_many_arguments)]
fn play(ev_out: &mut Vec<(i64, i64, Vec<u8>)>, sounding: &mut Vec<(i64, Sounding)>, ev: &crate::parse::Ev, t0: i64, span: i64, volume: f64, channel: i64, fifths: i64) {
    let heads: Vec<(i64, i64)> = if ev.heads.is_empty() { ev.pitches.iter().map(|p| (p.d, p.alter)).collect() } else { ev.heads.iter().map(|h| (h.d, h.alter)).collect() };
    let has = |n: &str| ev.scripts.iter().any(|sc| sc.name == n);
    let mut vel = volume;
    if has("accent") {
        vel += 20.0;
    }
    if has("marcato") {
        vel += 30.0;
    }
    let vel = (vel + 0.5).clamp(1.0, 127.0) as u8;
    let mut length = span;
    if has("staccato") {
        length = span / 2;
    }
    if has("staccatissimo") {
        length = span / 8;
    }
    if has("portato") {
        length = span * 3 / 4;
    }
    // `\afterGrace`: the note gives its end, from `after_at` (3/4 of
    // it), to its grace notes, which share that time as written.
    if !ev.after_graces.is_empty() && span > 1 {
        let part = if ev.after_at > 0.0 && ev.after_at < 1.0 { ev.after_at } else { crate::parse::AFTER_GRACE };
        let from = ((span as f64 * part).round() as i64).clamp(1, span - 1);
        let room = span - from;
        let written: i64 = ev.after_graces.iter().map(|g| g.ticks.max(1)).sum();
        let mut t = t0 + from;
        for (gi, g) in ev.after_graces.iter().enumerate() {
            let end = if gi + 1 == ev.after_graces.len() { t0 + span } else { (t + g.ticks.max(1) * room / written).max(t + 1) };
            for hd in &g.heads {
                let key = key_of(hd.d, hd.alter);
                ev_out.push((t, 1, vec![(144 + channel) as u8, key as u8, vel]));
                ev_out.push((end, 0, vec![(128 + channel) as u8, key as u8, 64]));
            }
            t = end;
        }
        // Tied on, the note is held under them into the next, as
        // LilyPond plays it: unless one of them is its own key,
        // which strikes it again.
        let own = ev.after_graces.iter().flat_map(|g| &g.heads).any(|g| heads.iter().any(|h| key_of(h.0, h.1) == key_of(g.d, g.alter)));
        if !ev.tie || own {
            length = length.min(from);
        }
    }
    if ev.trem_beams > 0 && span > 0 {
        // `\repeat tremolo 4 { c16 e }`: the two notes in turn, each its
        // written value, over the time both take — the first on the even
        // turns from its start, the second on the odd ones from the first's.
        let each = (WHOLE / (4 << ev.trem_beams)).clamp(1, span);
        let from = if ev.trem_first { t0 } else { t0 - span };
        let mut t = from;
        let mut turn = 0;
        while t < from + 2 * span {
            let len = each.min(from + 2 * span - t);
            if (turn % 2 == 0) == ev.trem_first {
                for &(d, alter) in &heads {
                    let key = key_of(d, alter);
                    ev_out.push((t, 1, vec![(144 + channel) as u8, key as u8, vel]));
                    ev_out.push((t + len, 0, vec![(128 + channel) as u8, key as u8, 64]));
                }
            }
            t += each;
            turn += 1;
        }
        return;
    }
    if ev.tremolo > 0 {
        // `c4:16`: the note again each sixteenth of it.
        let each = (WHOLE / ev.tremolo).max(1);
        let mut t = t0;
        while t < t0 + span {
            let len = each.min(t0 + span - t);
            for &(d, alter) in &heads {
                let key = key_of(d, alter);
                ev_out.push((t, 1, vec![(144 + channel) as u8, key as u8, vel]));
                ev_out.push((t + len, 0, vec![(128 + channel) as u8, key as u8, 64]));
            }
            t += each;
        }
        return;
    }
    // A trill, a mordent, a turn, a rolled chord: played as written out
    // (score #978). Not on a drum staff, nor with grace notes after it.
    let ornament = ORNAMENTS.into_iter().find(|n| has(n));
    // `\arpeggioBracket`'s chord is played together: its sign says so;
    // `\arpeggioArrowDown`'s rolled from the top.
    let rolled = match (ev.arpeggio, ev.arp_style) {
        (false, _) | (true, crate::parse::ArpStyle::Bracket) => Roll::None,
        (true, crate::parse::ArpStyle::ArrowDown) => Roll::Down,
        _ => Roll::Up,
    };
    if (ornament.is_some() || rolled != Roll::None) && ev.drums.is_empty() && ev.after_graces.is_empty() && !heads.is_empty() {
        ornamented(ev_out, &heads, ornament, rolled, t0, length, vel, channel, fifths);
        return;
    }
    for &(d, alter) in &heads {
        let key = ev.drums.iter().find(|dr| dr.d == d).map_or_else(|| key_of(d, alter), |dr| dr.key);
        if let Some((_, held)) = sounding.iter_mut().find(|(k, _)| *k == key) {
            if held.until == t0 && held.tied {
                held.until = t0 + length;
                held.tied = ev.tie;
                ev_out[held.off].0 = t0 + length;
                continue;
            }
        }
        ev_out.push((t0, 1, vec![(144 + channel) as u8, key as u8, vel]));
        ev_out.push((t0 + length, 0, vec![(128 + channel) as u8, key as u8, 64]));
        let s = Sounding { until: t0 + length, tied: ev.tie, off: ev_out.len() - 1 };
        match sounding.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = s,
            None => sounding.push((key, s)),
        }
    }
}

/// How a chord is rolled: not, from its lowest note, or from its highest.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Roll {
    None,
    Up,
    Down,
}

/// The ornaments played (score #978), as LilyPond's articulate.ly plays them.
const ORNAMENTS: [&str; 6] = ["trill", "prall", "mordent", "turn", "reverseturn", "prallprall"];

/// A note's ornament, `ornament`, and its chord rolled when `rolled`, from
/// `t0` for `length` (score #978). A rolled chord's notes come in from the
/// lowest, a 64th apart (half the note at most between the first and the
/// last), and all are held to its end. The ornament is the top note's, in
/// 32nds (a quarter of the note at most each), its neighbours the key's:
/// a trill goes between the note and the one above to the note's end; a
/// prall is the note, the one above, the note; a mordent the note, the one
/// below, the note; a turn the one above, the note, the one below, the note;
/// a reverse turn the one below, the note, the one above, the note; a double
/// prall the note and the one above twice, then the note.
#[allow(clippy::too_many_arguments)]
fn ornamented(ev_out: &mut Vec<(i64, i64, Vec<u8>)>, heads: &[(i64, i64)], ornament: Option<&str>, rolled: Roll, t0: i64, length: i64, vel: u8, channel: i64, fifths: i64) {
    let mut keys: Vec<(i64, i64, i64)> = heads.iter().map(|&(d, alter)| (key_of(d, alter), d, alter)).collect();
    keys.sort_unstable();
    keys.dedup_by_key(|k| k.0);
    let end = t0 + length.max(1);
    let n = keys.len() as i64;
    let roll = if rolled != Roll::None && n > 1 { (WHOLE / 64).min(length / 2 / (n - 1)).max(0) } else { 0 };
    // `\arpeggioArrowDown`: in from the highest (score #1026).
    let entry = |i: i64| if rolled == Roll::Down { n - 1 - i } else { i };
    let mut sound = |key: i64, from: i64, to: i64| {
        if to > from {
            ev_out.push((from, 1, vec![(144 + channel) as u8, key as u8, vel]));
            ev_out.push((to, 0, vec![(128 + channel) as u8, key as u8, 64]));
        }
    };
    for (i, &(key, _, _)) in keys.iter().enumerate().take(keys.len().saturating_sub(1)) {
        sound(key, t0 + entry(i as i64) * roll, end);
    }
    let Some(&(top, d, _)) = keys.last() else { return };
    let start = t0 + entry(n - 1) * roll;
    let room = end - start;
    let unit = (WHOLE / 32).min(room / 4).max(1);
    let above = key_of(d + 1, in_key(d + 1, fifths));
    let below = key_of(d - 1, in_key(d - 1, fifths));
    let turn: &[i64] = match ornament {
        Some("prall") => &[top, above],
        Some("mordent") => &[top, below],
        Some("turn") => &[above, top, below],
        // Score #1026: the turn the other way round, and the prall twice.
        Some("reverseturn") => &[below, top, above],
        Some("prallprall") => &[top, above, top, above],
        _ => &[],
    };
    let mut t = start;
    if ornament == Some("trill") {
        let mut upper = false;
        while t < end {
            let next = (t + unit).min(end);
            sound(if upper { above } else { top }, t, next);
            upper = !upper;
            t = next;
        }
        return;
    }
    for &key in turn {
        let next = (t + unit).min(end);
        sound(key, t, next);
        t = next;
    }
    sound(top, t, end);
}

/// The alteration the key of `fifths` gives the step of `d`: a sharp for the
/// first `fifths` of F C G D A E B, a flat for the first of B E A D G C F.
pub(crate) fn in_key(d: i64, fifths: i64) -> i64 {
    const SHARPS: [i64; 7] = [3, 0, 4, 1, 5, 2, 6];
    const FLATS: [i64; 7] = [6, 2, 5, 1, 4, 0, 3];
    let step = modulo(d, 7);
    let count = fifths.unsigned_abs().min(7) as usize;
    if fifths > 0 && SHARPS[..count].contains(&step) {
        1
    } else if fifths < 0 && FLATS[..count].contains(&step) {
        -1
    } else {
        0
    }
}

/// How long the music breathes after a fermata, in seconds (score #977).
pub const FERMATA_BREATH_SECONDS: f64 = 2.0;

/// A fermata's hold: where the held note ends (`ends`), how much longer it
/// sounds (`sustain`: half its value again, so 1.5 times it), then the
/// silence before the music goes on (`breath`: FERMATA_BREATH_SECONDS at
/// the tempo of that moment), in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Hold {
    pub ends: i64,
    pub sustain: i64,
    pub breath: i64,
}

/// Where the fermatas hold, over every staff, in time order: of several
/// notes under a fermata that end together, the longest is the one held.
pub(crate) fn holds(measured: &[Vec<Bar>], opts: &MidiOptions) -> Vec<Hold> {
    let mut by_end: Vec<(i64, i64, f64)> = Vec::new();
    let mut tempi: Vec<(i64, f64)> = Vec::new();
    for bars in measured {
        let mut at = 0;
        for bar in bars {
            for ev in &bar.items {
                if let Some(mark) = &ev.tempo {
                    if mark.bpm > 0 {
                        tempi.push((at + ev.start, quarter_seconds(mark.bpm as f64, mark.base)));
                    }
                }
                let factor = ev.scripts.iter().filter_map(|sc| fermata_factor(&sc.name)).fold(0.0, f64::max);
                if ev.k == Kind::Note && factor > 0.0 {
                    let span = ev.ticks * ev.mm.unwrap_or(1);
                    let ends = at + ev.start + span;
                    match by_end.iter_mut().find(|h| h.0 == ends) {
                        Some(h) => {
                            h.1 = h.1.max(span);
                            h.2 = h.2.max(factor);
                        }
                        None => by_end.push((ends, span, factor)),
                    }
                }
            }
            at += bar.length * bar.span.unwrap_or(1);
        }
    }
    by_end.sort_by_key(|h| h.0);
    tempi.sort_by_key(|t| t.0);
    // A source with no `\tempo` plays at the options' tempo, or at MIDI's
    // own, 120 quarters a minute, when they give none.
    let start = if opts.bpm > 0.0 { quarter_seconds(opts.bpm, opts.beat) } else { 0.5 };
    by_end
        .into_iter()
        .map(|(ends, span, factor)| {
            let quarter = tempi.iter().take_while(|t| t.0 < ends).last().map_or(start, |t| t.1);
            let scaled = |t: i64| (t as f64 * factor).round() as i64;
            Hold { ends, sustain: scaled(span / 2), breath: scaled(breath_ticks(quarter)) }
        })
        .collect()
}

/// How long a fermata holds against `\fermata`'s hold (score #1026): the
/// very short one a quarter of it, the short ones half, the long ones half
/// as long again, the very long one twice — the note's extra length and
/// the breath after it alike. None for a script that is no fermata.
fn fermata_factor(name: &str) -> Option<f64> {
    Some(match name {
        "fermata" => 1.0,
        "veryshortfermata" => 0.25,
        "shortfermata" | "henzeshortfermata" => 0.5,
        "longfermata" | "henzelongfermata" => 1.5,
        "verylongfermata" => 2.0,
        _ => return None,
    })
}

/// A quarter's length in seconds at `bpm` beats of `beat` (4 a quarter) a minute.
fn quarter_seconds(bpm: f64, beat: i64) -> f64 {
    60.0 / bpm * beat as f64 / 4.0
}

/// FERMATA_BREATH_SECONDS in ticks, when a quarter lasts `quarter` seconds.
pub(crate) fn breath_ticks(quarter: f64) -> i64 {
    if quarter <= 0.0 {
        return 0;
    }
    (FERMATA_BREATH_SECONDS / quarter * (WHOLE / 4) as f64).round() as i64
}

/// A time moved past every hold before it: by the held note's extra length
/// and its breath. At a hold's own end, a note let go (`off`) moves by the
/// extra length only; anything else there starts after the breath.
pub(crate) fn held(holds: &[Hold], t: i64, off: bool) -> i64 {
    t + holds.iter().filter(|h| h.ends <= t).map(|h| if h.ends == t && off { h.sustain } else { h.sustain + h.breath }).sum::<i64>()
}

fn hairpin_of(bars: &[Bar], bi: usize, start: &crate::parse::Ev, volume: f64) -> Hairpin {
    let mut notes = 0;
    let mut target: Option<f64> = None;
    let mut started = false;
    let mut done = false;
    for bar in &bars[bi..] {
        for ev in &bar.items {
            if done {
                continue;
            }
            if !started {
                started = ev.i == start.i;
                continue;
            }
            notes += 1;
            if let Some(d) = &ev.dynamic {
                target = volume_of(d).map(|v| v as f64);
            }
            done = ev.dynamic.is_some() || ev.hairpin_end || ev.hairpin.is_some();
        }
        if done {
            break;
        }
    }
    let target = target.unwrap_or(volume + if start.hairpin == Some('<') { 16.0 } else { -16.0 });
    let n = notes.max(1);
    Hairpin { left: n, step: (target - volume) / n as f64 }
}

pub(crate) fn key_of(d: i64, alter: i64) -> i64 {
    let step = modulo(d, 7);
    12 * ((d - step) / 7 + 4) + [0, 2, 4, 5, 7, 9, 11][step as usize] + alter
}

fn tempo(bpm: f64, beat: i64) -> Vec<u8> {
    let us = (60_000_000.0 / bpm * beat as f64 / 4.0) as i64;
    let mut out = vec![255, 81, 3];
    out.extend(be(us, 3));
    out
}

fn log2(mut v: i64) -> i64 {
    let mut out = 0;
    while v > 1 {
        v /= 2;
        out += 1;
    }
    out
}

fn vlq(mut v: i64) -> Vec<u8> {
    let mut out = vec![(v % 128) as u8];
    v /= 128;
    while v > 0 {
        out.insert(0, (v % 128 + 128) as u8);
        v /= 128;
    }
    out
}

fn be(v: i64, size: usize) -> Vec<u8> {
    (0..size).map(|k| ((v >> (8 * (size - 1 - k))) % 256) as u8).collect()
}

/// LilyPond's `dynamic-absolute-volume`, out of 127.
fn volume_of(d: &str) -> Option<i64> {
    Some(match d {
        "pppp" => 25,
        "ppp" => 32,
        "pp" => 51,
        "p" => 64,
        "mp" => 76,
        "mf" => 89,
        "f" => 102,
        "ff" => 114,
        "fff" => 121,
        "ffff" => 127,
        "fp" => 102,
        "sf" => 114,
        "sff" => 121,
        "sp" => 76,
        "spp" => 64,
        "sfz" | "sfp" | "rfz" | "rf" | "fz" => 114,
        "n" => 16,
        _ => return None,
    })
}

/// General MIDI's instruments by LilyPond's names for them.
pub fn program_of(name: &str) -> Option<i64> {
    PROGRAMS.iter().position(|p| *p == name).map(|k| k as i64)
}

/// LilyPond's `instrument-names-alist`, in program order.
pub const PROGRAMS: [&str; 128] = [
    "acoustic grand",
    "bright acoustic",
    "electric grand",
    "honky-tonk",
    "electric piano 1",
    "electric piano 2",
    "harpsichord",
    "clav",
    "celesta",
    "glockenspiel",
    "music box",
    "vibraphone",
    "marimba",
    "xylophone",
    "tubular bells",
    "dulcimer",
    "drawbar organ",
    "percussive organ",
    "rock organ",
    "church organ",
    "reed organ",
    "accordion",
    "harmonica",
    "concertina",
    "acoustic guitar (nylon)",
    "acoustic guitar (steel)",
    "electric guitar (jazz)",
    "electric guitar (clean)",
    "electric guitar (muted)",
    "overdriven guitar",
    "distorted guitar",
    "guitar harmonics",
    "acoustic bass",
    "electric bass (finger)",
    "electric bass (pick)",
    "fretless bass",
    "slap bass 1",
    "slap bass 2",
    "synth bass 1",
    "synth bass 2",
    "violin",
    "viola",
    "cello",
    "contrabass",
    "tremolo strings",
    "pizzicato strings",
    "orchestral harp",
    "timpani",
    "string ensemble 1",
    "string ensemble 2",
    "synthstrings 1",
    "synthstrings 2",
    "choir aahs",
    "voice oohs",
    "synth voice",
    "orchestra hit",
    "trumpet",
    "trombone",
    "tuba",
    "muted trumpet",
    "french horn",
    "brass section",
    "synthbrass 1",
    "synthbrass 2",
    "soprano sax",
    "alto sax",
    "tenor sax",
    "baritone sax",
    "oboe",
    "english horn",
    "bassoon",
    "clarinet",
    "piccolo",
    "flute",
    "recorder",
    "pan flute",
    "blown bottle",
    "shakuhachi",
    "whistle",
    "ocarina",
    "lead 1 (square)",
    "lead 2 (sawtooth)",
    "lead 3 (calliope)",
    "lead 4 (chiff)",
    "lead 5 (charang)",
    "lead 6 (voice)",
    "lead 7 (fifths)",
    "lead 8 (bass+lead)",
    "pad 1 (new age)",
    "pad 2 (warm)",
    "pad 3 (polysynth)",
    "pad 4 (choir)",
    "pad 5 (bowed)",
    "pad 6 (metallic)",
    "pad 7 (halo)",
    "pad 8 (sweep)",
    "fx 1 (rain)",
    "fx 2 (soundtrack)",
    "fx 3 (crystal)",
    "fx 4 (atmosphere)",
    "fx 5 (brightness)",
    "fx 6 (goblins)",
    "fx 7 (echoes)",
    "fx 8 (sci-fi)",
    "sitar",
    "banjo",
    "shamisen",
    "koto",
    "kalimba",
    "bagpipe",
    "fiddle",
    "shanai",
    "tinkle bell",
    "agogo",
    "steel drums",
    "woodblock",
    "taiko drum",
    "melodic tom",
    "synth drum",
    "reverse cymbal",
    "guitar fret noise",
    "breath noise",
    "seashore",
    "bird tweet",
    "telephone ring",
    "helicopter",
    "applause",
    "gunshot",
];
