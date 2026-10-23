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
    for stf in &parsed.staves {
        measured.push(measures(&stf.events)?);
    }
    let holds = holds(&measured);
    let mut tracks: Vec<Vec<u8>> = Vec::new();
    for (si, stf) in parsed.staves.iter().enumerate() {
        // Chord names are read, not played, as LilyPond's \\midi has them.
        // A figured bass, too: the bass it figures is played.
        if (stf.kind == StaffKind::ChordNames && parsed.staves.len() > 1) || matches!(stf.kind, StaffKind::Figures | StaffKind::FretBoards) {
            continue;
        }
        let props: &Props = &stf.props;
        let program = props.midi_instrument.as_deref().and_then(program_of).unwrap_or(opts.program);
        let name = props.instrument_name.clone().unwrap_or_else(|| if si == 0 { title.clone() } else { String::new() });
        // A drum staff plays on General MIDI's percussion channel, the tenth.
        let channel = if stf.kind == StaffKind::Drums { 9 } else { (if si < 9 { si } else { si + 1 }).min(15) as i64 };
        let track = track(&measured[si], opts, &holds, program, &name, channel, tracks.is_empty());
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

#[allow(clippy::too_many_arguments)]
fn track(bars: &[Bar], opts: &MidiOptions, holds: &[(i64, i64)], program: i64, name: &str, channel: i64, first_track: bool) -> Vec<u8> {
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
    let mut volume: f64 = 90.0;
    let mut hairpin: Option<Hairpin> = None;
    let mut sounding: Vec<(i64, Sounding)> = Vec::new();
    for (bi, bar) in bars.iter().enumerate() {
        if bi == 0 || bar.changes.time {
            ev_out.push((at, 0, vec![255, 88, 4, bar.time.0 as u8, log2(bar.time.1) as u8, 24, 8]));
        }
        if bi == 0 || bar.changes.key {
            ev_out.push((at, 0, vec![255, 89, 2, modulo(bar.fifths, 256) as u8, 0]));
        }
        for ev in &bar.items {
            let t0 = at + ev.start;
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
            let t0 = t0 + stolen;
            let span = span - stolen;
            if ev.k != Kind::Note {
                continue;
            }
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
            if ev.tremolo > 0 {
                // `c4:16`: the note again each sixteenth of it.
                let each = (WHOLE / ev.tremolo).max(1);
                let mut t = t0;
                while t < t0 + span {
                    let len = each.min(t0 + span - t);
                    for hd in &ev.heads {
                        let key = key_of(hd.d, hd.alter);
                        ev_out.push((t, 1, vec![(144 + channel) as u8, key as u8, vel]));
                        ev_out.push((t + len, 0, vec![(128 + channel) as u8, key as u8, 64]));
                    }
                    t += each;
                }
                continue;
            }
            for hd in &ev.heads {
                let key = ev.drums.iter().find(|dr| dr.d == hd.d).map_or_else(|| key_of(hd.d, hd.alter), |dr| dr.key);
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
        at += bar.length * bar.span.unwrap_or(1);
    }
    if !holds.is_empty() {
        for e in &mut ev_out {
            e.0 = held(holds, e.0);
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

/// Where the fermatas hold: (tick the held note ends, ticks it is held
/// for), over every staff, in time order.
pub(crate) fn holds(measured: &[Vec<Bar>]) -> Vec<(i64, i64)> {
    let mut by_end: Vec<(i64, i64)> = Vec::new();
    for bars in measured {
        let mut at = 0;
        for bar in bars {
            for ev in &bar.items {
                if ev.k == Kind::Note && ev.scripts.iter().any(|sc| sc.name == "fermata") {
                    let span = ev.ticks * ev.mm.unwrap_or(1);
                    let ends = at + ev.start + span;
                    match by_end.iter_mut().find(|h| h.0 == ends) {
                        Some(h) => h.1 = h.1.max(span),
                        None => by_end.push((ends, span)),
                    }
                }
            }
            at += bar.length * bar.span.unwrap_or(1);
        }
    }
    by_end.sort_by_key(|h| h.0);
    by_end
}

/// A time moved past every hold that ends at it or before.
pub(crate) fn held(holds: &[(i64, i64)], t: i64) -> i64 {
    t + holds.iter().filter(|h| h.0 <= t).map(|h| h.1).sum::<i64>()
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
