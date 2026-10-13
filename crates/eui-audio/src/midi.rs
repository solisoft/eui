//! MIDI, synthesized (spec 03 §7, protocol 9).
//!
//! A Standard MIDI File becomes a [`Sound`] like any recording: rendered
//! once, offline, to stereo `f32` at [`RATE`], then played, sought, looped
//! and metered by the mixer that plays everything else. With a SoundFont
//! [`Bank`] the notes are that bank's instruments, through `rustysynth`;
//! without one they are this module's own voices — one for each of General
//! MIDI's sixteen families and a drum kit — so a MIDI sound always sounds.
//!
//! A list of commands (`notes`) is written as a MIDI file of its own and
//! takes the same road, so there is one synthesizer and not two.
//!
//! The file is read here, by hand, before `rustysynth` sees it: its length
//! is what the budget is checked against, and the client's own voices need
//! its events anyway. Every read is checked; hostile bytes are an error,
//! never a panic.

use std::io::Cursor;
use std::sync::Arc;

use crate::{AudioError, Sound};

/// The rate a MIDI sound is synthesized at.
pub const RATE: u32 = 44_100;

/// Commands one `notes` list may carry (spec 10, *Sound*).
pub const MAX_NOTE_EVENTS: usize = 512;

/// How far after the first command the last may come, in milliseconds.
pub const MAX_NOTE_DELAY_MS: i64 = 60_000;

/// What a bank may weigh, its parts together (spec 10, *Sound*).
pub const MAX_SOUNDFONT_BYTES: usize = 64 * 1024 * 1024;

/// Events one file may hold: a backstop before its length is known.
const MAX_EVENTS: usize = 1_000_000;

/// Seconds of sound kept after the last event, for releases to die away.
const TAIL: f64 = 1.0;

/// A SoundFont bank, parsed once and shared by every sound played with it.
#[derive(Clone)]
pub struct Bank {
    font: Arc<rustysynth::SoundFont>,
}

impl std::fmt::Debug for Bank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bank").field("presets", &self.font.get_presets().len()).finish()
    }
}

impl Bank {
    /// How many instruments (presets) the bank holds.
    pub fn presets(&self) -> usize {
        self.font.get_presets().len()
    }
}

/// True when `bytes` are a Standard MIDI File.
pub fn is_midi(bytes: &[u8]) -> bool {
    bytes.starts_with(b"MThd")
}

/// Parse a SoundFont 2 bank.
pub fn load_bank(bytes: &[u8]) -> Result<Bank, AudioError> {
    if bytes.len() > MAX_SOUNDFONT_BYTES {
        return Err(AudioError::TooLong);
    }
    let font = rustysynth::SoundFont::new(&mut Cursor::new(bytes)).map_err(|e| AudioError::Malformed(format!("soundfont: {e:?}")))?;
    Ok(Bank { font: Arc::new(font) })
}

/// Synthesize a Standard MIDI File, with `bank`'s instruments or the
/// client's own voices, held to `max_bytes` of samples.
pub fn render(bytes: &[u8], bank: Option<&Bank>, max_bytes: usize) -> Result<Sound, AudioError> {
    let song = parse(bytes)?;
    let seconds = song.length + TAIL;
    let frames = (seconds * f64::from(RATE)).ceil() as usize;
    if frames > crate::MAX_FRAMES || frames.saturating_mul(2).saturating_mul(4) > max_bytes {
        return Err(AudioError::TooLong);
    }
    let (left, right) = match bank {
        Some(bank) => render_bank(bytes, bank, frames)?,
        None => render_voices(&song, frames),
    };
    let mut samples = Vec::with_capacity(frames.saturating_mul(2));
    for (l, r) in left.iter().zip(right.iter()) {
        samples.push(*l);
        samples.push(*r);
    }
    Ok(Sound::new(samples, RATE, 2))
}

/// A list of commands, `[delay_ms, status, data1, data2]` each, as a MIDI
/// file of its own: one track, a tick a millisecond. A note on that no
/// note off ends is ended a second after the last command.
pub fn notes_to_midi(events: &[[i64; 4]]) -> Result<Vec<u8>, AudioError> {
    if events.len() > MAX_NOTE_EVENTS {
        return Err(AudioError::TooLong);
    }
    let mut list: Vec<(i64, u8, u8, u8)> = Vec::with_capacity(events.len());
    for e in events {
        let [delay, status, d1, d2] = *e;
        if !(0..=MAX_NOTE_DELAY_MS).contains(&delay) {
            return Err(AudioError::Malformed(format!("a delay of {delay} ms")));
        }
        let ok = (0x80..=0xEF).contains(&status) && (0..=127).contains(&d1) && (0..=127).contains(&d2);
        if !ok {
            return Err(AudioError::Malformed(format!("the command [{status}, {d1}, {d2}]")));
        }
        list.push((delay, status as u8, d1 as u8, d2 as u8));
    }
    list.sort_by_key(|e| e.0);
    let last = list.last().map_or(0, |e| e.0);
    // Ends for the notes left sounding.
    let mut held: Vec<(u8, u8)> = Vec::new();
    for &(_, status, key, vel) in &list {
        let channel = status & 0x0F;
        match status & 0xF0 {
            0x90 if vel > 0 => held.push((channel, key)),
            0x80 | 0x90 => held.retain(|h| *h != (channel, key)),
            _ => {}
        }
    }
    for (channel, key) in held {
        list.push((last + 1000, 0x80 | channel, key, 0));
    }
    // A tick a millisecond: 500 ticks a quarter at 120 a minute.
    let mut track: Vec<u8> = vec![0x00, 0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20];
    let mut at = 0i64;
    for (time, status, d1, d2) in list {
        push_vlq(&mut track, (time - at).max(0) as u32);
        at = time;
        track.push(status);
        track.push(d1);
        if !matches!(status & 0xF0, 0xC0 | 0xD0) {
            track.push(d2);
        }
    }
    track.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
    let mut out = b"MThd\x00\x00\x00\x06\x00\x00\x00\x01\x01\xF4MTrk".to_vec();
    out.extend_from_slice(&(track.len() as u32).to_be_bytes());
    out.extend_from_slice(&track);
    Ok(out)
}

fn push_vlq(out: &mut Vec<u8>, value: u32) {
    let mut bytes = vec![(value & 0x7F) as u8];
    let mut v = value >> 7;
    while v > 0 {
        bytes.push(((v & 0x7F) | 0x80) as u8);
        v >>= 7;
    }
    bytes.reverse();
    out.extend_from_slice(&bytes);
}

// ---- Reading a file -------------------------------------------------------

/// One channel message, at its time in seconds.
#[derive(Debug, Clone, Copy)]
struct Event {
    time: f64,
    status: u8,
    data1: u8,
    data2: u8,
}

#[derive(Debug)]
struct Song {
    events: Vec<Event>,
    length: f64,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Result<u8, AudioError> {
        let b = *self.bytes.get(self.at).ok_or_else(|| AudioError::Malformed("the MIDI file ends early".into()))?;
        self.at += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&[u8], AudioError> {
        let end = self.at.checked_add(n).ok_or_else(|| AudioError::Malformed("a length past the file".into()))?;
        let out = self.bytes.get(self.at..end).ok_or_else(|| AudioError::Malformed("the MIDI file ends early".into()))?;
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, AudioError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([*b.first().unwrap_or(&0), *b.get(1).unwrap_or(&0), *b.get(2).unwrap_or(&0), *b.get(3).unwrap_or(&0)]))
    }

    fn u16(&mut self) -> Result<u16, AudioError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([*b.first().unwrap_or(&0), *b.get(1).unwrap_or(&0)]))
    }

    fn vlq(&mut self) -> Result<u32, AudioError> {
        let mut v: u32 = 0;
        for _ in 0..4 {
            let b = self.byte()?;
            v = (v << 7) | u32::from(b & 0x7F);
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(AudioError::Malformed("a time longer than four bytes".into()))
    }
}

/// The channel messages of every track, timed by the file's own tempo map.
fn parse(bytes: &[u8]) -> Result<Song, AudioError> {
    let mut r = Reader { bytes, at: 0 };
    if r.take(4)? != b"MThd" {
        return Err(AudioError::Unsupported("not a MIDI file".into()));
    }
    let header = r.u32()? as usize;
    let _format = r.u16()?;
    let tracks = r.u16()?;
    let division = r.u16()?;
    if header > 6 {
        r.take(header - 6)?;
    }
    if division & 0x8000 != 0 || division == 0 {
        return Err(AudioError::Unsupported("a MIDI file timed in SMPTE frames".into()));
    }
    // (tick, order, status, data1, data2); tempos as (tick, µs a quarter).
    let mut raw: Vec<(u64, usize, u8, u8, u8)> = Vec::new();
    let mut tempos: Vec<(u64, u32)> = vec![(0, 500_000)];
    let mut last_tick = 0u64;
    for _ in 0..tracks {
        if r.take(4)? != b"MTrk" {
            return Err(AudioError::Malformed("a MIDI track is missing".into()));
        }
        let length = r.u32()? as usize;
        let end = r.at.checked_add(length).ok_or_else(|| AudioError::Malformed("a track past the file".into()))?;
        if end > bytes.len() {
            return Err(AudioError::Malformed("a track past the file".into()));
        }
        let mut tick = 0u64;
        let mut running = 0u8;
        while r.at < end {
            tick = tick.saturating_add(u64::from(r.vlq()?));
            let mut status = r.byte()?;
            if status < 0x80 {
                if running == 0 {
                    return Err(AudioError::Malformed("data with no status".into()));
                }
                r.at -= 1;
                status = running;
            }
            match status {
                0xFF => {
                    let kind = r.byte()?;
                    let len = r.vlq()? as usize;
                    let data = r.take(len)?;
                    if kind == 0x51 && len == 3 {
                        let us = (u32::from(*data.first().unwrap_or(&0)) << 16) | (u32::from(*data.get(1).unwrap_or(&0)) << 8) | u32::from(*data.get(2).unwrap_or(&0));
                        if us > 0 {
                            tempos.push((tick, us));
                        }
                    }
                }
                0xF0 | 0xF7 => {
                    let len = r.vlq()? as usize;
                    r.take(len)?;
                }
                0x80..=0xEF => {
                    running = status;
                    let data1 = r.byte()?;
                    let data2 = if matches!(status & 0xF0, 0xC0 | 0xD0) { 0 } else { r.byte()? };
                    if raw.len() >= MAX_EVENTS {
                        return Err(AudioError::TooLong);
                    }
                    raw.push((tick, raw.len(), status, data1 & 0x7F, data2 & 0x7F));
                }
                _ => return Err(AudioError::Malformed(format!("the status byte {status:#04x}"))),
            }
        }
        r.at = end;
        last_tick = last_tick.max(tick);
    }
    tempos.sort_by_key(|t| t.0);
    raw.sort_by_key(|e| (e.0, e.1));
    let seconds = |tick: u64| -> f64 {
        let mut s = 0.0;
        let mut from = 0u64;
        let mut us = 500_000u32;
        for &(at, tempo) in &tempos {
            if at > tick {
                break;
            }
            s += (at - from) as f64 * f64::from(us) / f64::from(division) / 1e6;
            from = at;
            us = tempo;
        }
        s + (tick - from) as f64 * f64::from(us) / f64::from(division) / 1e6
    };
    let events = raw.iter().map(|&(tick, _, status, data1, data2)| Event { time: seconds(tick), status, data1, data2 }).collect();
    Ok(Song { events, length: seconds(last_tick) })
}

// ---- A bank's instruments ---------------------------------------------------

fn render_bank(bytes: &[u8], bank: &Bank, frames: usize) -> Result<(Vec<f32>, Vec<f32>), AudioError> {
    let file = rustysynth::MidiFile::new(&mut Cursor::new(bytes)).map_err(|e| AudioError::Malformed(format!("midi: {e:?}")))?;
    let settings = rustysynth::SynthesizerSettings::new(RATE as i32);
    let synth = rustysynth::Synthesizer::new(&bank.font, &settings).map_err(|e| AudioError::Malformed(format!("synthesizer: {e:?}")))?;
    let mut sequencer = rustysynth::MidiFileSequencer::new(synth);
    sequencer.play(&Arc::new(file), false);
    let mut left = vec![0.0f32; frames];
    let mut right = vec![0.0f32; frames];
    for (l, r) in left.chunks_mut(4096).zip(right.chunks_mut(4096)) {
        sequencer.render(l, r);
    }
    Ok((left, right))
}

// ---- The client's own voices --------------------------------------------------

/// How one family of General MIDI instruments sounds: the strength of its
/// partials, how it starts, whether it dies away by itself (`decay`
/// seconds, at A4) or holds (`sustain`), how it ends, and its vibrato.
struct Timbre {
    partials: &'static [f32],
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    vibrato: f32,
}

fn timbre(program: u8) -> Timbre {
    match program / 8 {
        // Piano, chromatic percussion, guitar, bass: struck or plucked, dying away.
        0 => Timbre { partials: &[1.0, 0.45, 0.25, 0.12, 0.08, 0.05], attack: 0.004, decay: 1.6, sustain: 0.0, release: 0.25, vibrato: 0.0 },
        1 => Timbre { partials: &[1.0, 0.0, 0.35, 0.0, 0.18], attack: 0.002, decay: 0.9, sustain: 0.0, release: 0.4, vibrato: 0.0 },
        3 => Timbre { partials: &[1.0, 0.6, 0.35, 0.25, 0.15, 0.1], attack: 0.003, decay: 1.1, sustain: 0.0, release: 0.15, vibrato: 0.0 },
        4 => Timbre { partials: &[1.0, 0.5, 0.2, 0.08], attack: 0.004, decay: 1.4, sustain: 0.0, release: 0.12, vibrato: 0.0 },
        // Organ: drawbars, held.
        2 => Timbre { partials: &[1.0, 0.8, 0.6, 0.45, 0.3, 0.2, 0.12], attack: 0.01, decay: 0.0, sustain: 0.85, release: 0.06, vibrato: 0.0 },
        // Strings and ensembles: bowed, a slow start, a vibrato.
        5 | 6 => Timbre { partials: &[1.0, 0.5, 0.33, 0.25, 0.2, 0.16, 0.12], attack: 0.09, decay: 0.0, sustain: 0.85, release: 0.3, vibrato: 0.004 },
        // Brass: bright, quick to speak.
        7 => Timbre { partials: &[1.0, 0.85, 0.65, 0.5, 0.38, 0.28, 0.2], attack: 0.035, decay: 0.0, sustain: 0.8, release: 0.12, vibrato: 0.002 },
        // Reeds: the odd partials of a closed pipe.
        8 => Timbre { partials: &[1.0, 0.05, 0.55, 0.05, 0.35, 0.03, 0.2], attack: 0.03, decay: 0.0, sustain: 0.85, release: 0.1, vibrato: 0.003 },
        // Pipes and flutes: nearly pure.
        9 => Timbre { partials: &[1.0, 0.18, 0.06], attack: 0.045, decay: 0.0, sustain: 0.9, release: 0.12, vibrato: 0.004 },
        // Synth leads and pads.
        10 => Timbre { partials: &[1.0, 0.0, 0.33, 0.0, 0.2, 0.0, 0.14], attack: 0.01, decay: 0.0, sustain: 0.85, release: 0.1, vibrato: 0.0 },
        11 => Timbre { partials: &[1.0, 0.5, 0.33, 0.25], attack: 0.35, decay: 0.0, sustain: 0.8, release: 0.6, vibrato: 0.003 },
        // Ethnic (plucked) and percussive instruments.
        13 | 14 => Timbre { partials: &[1.0, 0.4, 0.3, 0.1], attack: 0.003, decay: 0.8, sustain: 0.0, release: 0.2, vibrato: 0.0 },
        _ => Timbre { partials: &[1.0, 0.4, 0.2], attack: 0.01, decay: 0.0, sustain: 0.8, release: 0.15, vibrato: 0.0 },
    }
}

/// A note being played, from its start to its end, then its release.
struct Voice {
    channel: u8,
    key: u8,
    velocity: u8,
    program: u8,
    start: f64,
    end: f64,
    volume: u8,
    pan: u8,
}

#[allow(clippy::indexing_slicing)] // every index is a channel masked to 0..16
fn render_voices(song: &Song, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let mut program = [0u8; 16];
    let mut volume = [100u8; 16];
    let mut pan = [64u8; 16];
    let mut voices: Vec<Voice> = Vec::new();
    for e in &song.events {
        let ch = usize::from(e.status & 0x0F);
        match e.status & 0xF0 {
            0x90 if e.data2 > 0 => {
                for v in voices.iter_mut().filter(|v| usize::from(v.channel) == ch && v.key == e.data1 && v.end > e.time) {
                    v.end = e.time;
                }
                voices.push(Voice { channel: ch as u8, key: e.data1, velocity: e.data2, program: program[ch], start: e.time, end: f64::INFINITY, volume: volume[ch], pan: pan[ch] });
            }
            0x80 | 0x90 => {
                for v in voices.iter_mut().filter(|v| usize::from(v.channel) == ch && v.key == e.data1 && v.end > e.time) {
                    v.end = e.time;
                }
            }
            0xC0 => program[ch] = e.data1,
            0xB0 => match e.data1 {
                7 => volume[ch] = e.data2,
                10 => pan[ch] = e.data2,
                120 | 123 => {
                    for v in voices.iter_mut().filter(|v| usize::from(v.channel) == ch && v.end > e.time) {
                        v.end = e.time;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    let mut left = vec![0.0f32; frames];
    let mut right = vec![0.0f32; frames];
    for v in &mut voices {
        if !v.end.is_finite() {
            v.end = song.length;
        }
        if v.channel == 9 {
            drum(v, &mut left, &mut right);
        } else {
            tone(v, &mut left, &mut right);
        }
    }
    // Under full scale: a chord of many loud voices is scaled down whole,
    // not clipped.
    let peak = left.iter().chain(right.iter()).fold(0.0f32, |p, s| p.max(s.abs()));
    if peak > 0.95 {
        let k = 0.95 / peak;
        for s in left.iter_mut().chain(right.iter_mut()) {
            *s *= k;
        }
    }
    (left, right)
}

/// The gains a voice is heard at, left and right: its velocity, its
/// channel's volume, and its pan at equal power.
fn gains(v: &Voice) -> (f32, f32) {
    let loud = (f32::from(v.velocity) / 127.0).powf(1.5) * (f32::from(v.volume) / 127.0) * 0.22;
    let angle = f32::from(v.pan) / 127.0 * std::f32::consts::FRAC_PI_2;
    (loud * angle.cos(), loud * angle.sin())
}

fn tone(v: &Voice, left: &mut [f32], right: &mut [f32]) {
    let t = timbre(v.program);
    let freq = 440.0 * 2f32.powf((f32::from(v.key) - 69.0) / 12.0);
    // A higher string dies sooner.
    let decay = if t.decay > 0.0 { t.decay * (440.0 / freq).powf(0.35) } else { 0.0 };
    let held = (v.end - v.start).max(0.0) as f32;
    let tail = t.release.max(0.01) * 5.0;
    let rate = RATE as f32;
    let first = (v.start * f64::from(RATE)) as usize;
    let count = ((held + tail) * rate) as usize;
    let (gl, gr) = gains(v);
    let norm: f32 = t.partials.iter().sum::<f32>().max(1.0);
    let mut phase = 0.0f32;
    for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).skip(first).take(count).enumerate() {
        let s = i as f32 / rate;
        let vib = 1.0 + t.vibrato * (std::f32::consts::TAU * 5.2 * s).sin();
        phase += freq * vib / rate;
        if phase > 1.0 {
            phase -= 1.0;
        }
        let mut env = (s / t.attack.max(0.001)).min(1.0);
        env *= if decay > 0.0 { (-s / decay).exp() } else { t.sustain + (1.0 - t.sustain) * (-s / 0.12).exp() };
        if s > held {
            env *= (-(s - held) / t.release.max(0.01)).exp();
        }
        let mut wave = 0.0;
        for (k, p) in t.partials.iter().enumerate() {
            if *p != 0.0 {
                wave += p * (std::f32::consts::TAU * phase * (k + 1) as f32).sin();
            }
        }
        let out = wave / norm * env;
        *l += out * gl;
        *r += out * gr;
    }
}

/// General MIDI's drum kit, by key: kicks and toms as falling tones,
/// snares as tone and noise, cymbals and hats as noise that rings longer
/// or shorter.
fn drum(v: &Voice, left: &mut [f32], right: &mut [f32]) {
    let rate = RATE as f32;
    let first = (v.start * f64::from(RATE)) as usize;
    let (gl, gr) = gains(v);
    // (tone start, tone end, tone weight, noise weight, ring seconds, bright noise)
    let (f0, f1, tone_w, noise_w, ring, bright) = match v.key {
        35 | 36 => (120.0, 48.0, 1.0, 0.05, 0.3, false),
        38 | 40 => (190.0, 160.0, 0.45, 0.7, 0.16, false),
        37 | 39 => (800.0, 700.0, 0.1, 0.9, 0.06, true),
        42 | 44 => (0.0, 0.0, 0.0, 0.6, 0.045, true),
        46 => (0.0, 0.0, 0.0, 0.6, 0.35, true),
        49 | 52 | 55 | 57 => (0.0, 0.0, 0.0, 0.55, 1.2, true),
        51 | 53 | 59 => (4200.0, 4200.0, 0.08, 0.4, 0.7, true),
        41 | 43 | 45 | 47 | 48 | 50 => {
            let f = 70.0 + f32::from(v.key.saturating_sub(41)) * 18.0;
            (f, f * 0.7, 1.0, 0.08, 0.35, false)
        }
        54 => (0.0, 0.0, 0.0, 0.5, 0.15, true),
        56 => (560.0, 560.0, 0.8, 0.0, 0.25, false),
        75..=77 => (1800.0, 1700.0, 0.9, 0.05, 0.05, false),
        _ => (300.0, 250.0, 0.3, 0.5, 0.12, false),
    };
    let count = (ring * 5.0 * rate) as usize;
    let mut phase = 0.0f32;
    let mut seed: u32 = 0x9E37_79B9 ^ (u32::from(v.key) << 8) ^ (first as u32);
    let mut last_noise = 0.0f32;
    for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).skip(first).take(count).enumerate() {
        let s = i as f32 / rate;
        let f = f1 + (f0 - f1) * (-s / 0.04).exp();
        phase += f / rate;
        if phase > 1.0 {
            phase -= 1.0;
        }
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let white = (seed >> 8) as f32 / 8_388_608.0 - 1.0;
        let noise = if bright { white - last_noise } else { white };
        last_noise = white;
        let env = (-s / ring).exp();
        let out = (tone_w * (std::f32::consts::TAU * phase).sin() + noise_w * noise) * env;
        *l += out * gl;
        *r += out * gr;
    }
}
