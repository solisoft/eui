//! MIDI sounds and MIDI commands (spec 03 §7, 10 *Sound*).

#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::arithmetic_side_effects, missing_docs)]

use eui_audio::midi::{self, MAX_NOTE_EVENTS, RATE};
use eui_audio::AudioError;

/// A one-track file, 480 ticks a quarter, a quarter a second (60 a
/// minute), the given events as (tick, status, data1, data2).
fn smf(events: &[(u32, u8, u8, u8)]) -> Vec<u8> {
    let mut track = vec![0x00, 0xFF, 0x51, 0x03, 0x0F, 0x42, 0x40];
    let mut at = 0;
    for &(tick, status, d1, d2) in events {
        let delta = tick - at;
        at = tick;
        let mut v = vec![(delta & 0x7F) as u8];
        let mut rest = delta >> 7;
        while rest > 0 {
            v.push(((rest & 0x7F) | 0x80) as u8);
            rest >>= 7;
        }
        v.reverse();
        track.extend(v);
        track.extend([status, d1]);
        if !matches!(status & 0xF0, 0xC0 | 0xD0) {
            track.push(d2);
        }
    }
    track.extend([0x00, 0xFF, 0x2F, 0x00]);
    let mut out = b"MThd\x00\x00\x00\x06\x00\x00\x00\x01\x01\xE0MTrk".to_vec();
    out.extend((track.len() as u32).to_be_bytes());
    out.extend(track);
    out
}

fn energy(samples: &[f32], channel: usize, from_s: f32, to_s: f32) -> f32 {
    let a = (from_s * RATE as f32) as usize * 2;
    let b = ((to_s * RATE as f32) as usize * 2).min(samples.len());
    samples[a..b].iter().skip(channel).step_by(2).map(|s| s * s).sum()
}

/// A platform synthesizer that says how many messages it was given: every
/// sample a hundredth of their count, left positive and right negative.
fn counting(events: &[(f64, u8, u8, u8)], frames: usize, _rate: u32) -> Option<(Vec<f32>, Vec<f32>)> {
    let v = events.len() as f32 / 100.0;
    Some((vec![v; frames], vec![-v; frames]))
}

fn declining(_: &[(f64, u8, u8, u8)], _: usize, _: u32) -> Option<(Vec<f32>, Vec<f32>)> {
    None
}

fn short(_: &[(f64, u8, u8, u8)], frames: usize, _: u32) -> Option<(Vec<f32>, Vec<f32>)> {
    Some((vec![0.5; frames / 2], vec![0.5; frames / 2]))
}

#[test]
fn without_a_bank_the_platforms_synthesizer_plays_and_its_refusal_falls_back() {
    let bytes = smf(&[(0, 0xC0, 73, 0), (0, 0x90, 60, 100), (480, 0x80, 60, 0)]);
    let ours = midi::render(&bytes, None, eui_audio::MAX_BYTES).unwrap();
    let theirs = midi::render_with(&bytes, None, eui_audio::MAX_BYTES, Some(counting)).unwrap();
    assert_eq!(theirs.samples().len(), ours.samples().len(), "the same length either way");
    assert_eq!(&theirs.samples()[..4], &[0.03, -0.03, 0.03, -0.03], "the platform's samples, given the file's three messages");
    let declined = midi::render_with(&bytes, None, eui_audio::MAX_BYTES, Some(declining)).unwrap();
    assert_eq!(declined.samples(), ours.samples(), "a synthesizer that answers None leaves the client's own voices");
    let wrong = midi::render_with(&bytes, None, eui_audio::MAX_BYTES, Some(short)).unwrap();
    assert_eq!(wrong.samples(), ours.samples(), "and so does one that renders the wrong length");
}

#[test]
fn a_midi_file_lasts_what_its_tempo_says() {
    // Two quarters at sixty: two seconds, and a second of tail.
    let bytes = smf(&[(0, 0x90, 60, 100), (960, 0x80, 60, 0)]);
    assert!(midi::is_midi(&bytes));
    let sound = eui_audio::decode(&bytes, None).unwrap();
    assert_eq!(sound.channels(), 2);
    assert_eq!(sound.rate(), RATE);
    assert!((sound.duration_ms() as i64 - 3000).abs() <= 30, "{} ms", sound.duration_ms());
}

#[test]
fn a_midi_note_sounds_at_its_time_and_not_before() {
    let bytes = smf(&[(480, 0x90, 64, 110), (960, 0x80, 64, 0)]);
    let sound = eui_audio::decode(&bytes, None).unwrap();
    let s = sound.samples();
    assert!(energy(s, 0, 0.0, 0.95) < 1e-6, "silence before the note");
    assert!(energy(s, 0, 1.0, 1.5) > 1.0, "the note after it starts");
}

#[test]
fn a_pan_moves_the_sound_to_one_side() {
    // Controller 10 at 0: hard left.
    let bytes = smf(&[(0, 0xB0, 10, 0), (0, 0x90, 60, 100), (480, 0x80, 60, 0)]);
    let sound = eui_audio::decode(&bytes, None).unwrap();
    let s = sound.samples();
    let left = energy(s, 0, 0.0, 1.0);
    let right = energy(s, 1, 0.0, 1.0);
    assert!(left > 100.0 * right.max(1e-9), "left {left}, right {right}");
}

#[test]
fn a_drum_on_channel_ten_is_a_drum() {
    // A kick on channel 10 sounds even with no program for it.
    let bytes = smf(&[(0, 0x99, 36, 120), (240, 0x89, 36, 0)]);
    let sound = eui_audio::decode(&bytes, None).unwrap();
    assert!(energy(sound.samples(), 0, 0.0, 0.3) > 1.0);
}

#[test]
fn notes_are_a_midi_file_of_their_own() {
    let notes = [[0, 0x90, 67, 100], [400, 0x80, 67, 0], [400, 0x90, 72, 90]];
    let bytes = midi::notes_to_midi(&notes).unwrap();
    assert!(midi::is_midi(&bytes));
    let sound = eui_audio::decode(&bytes, None).unwrap();
    // The last note on has no note off: it ends a second after the last
    // command, and a second of tail follows.
    assert!((sound.duration_ms() as i64 - 2400).abs() <= 30, "{} ms", sound.duration_ms());
    assert!(energy(sound.samples(), 0, 0.45, 0.9) > 0.5, "the second note");
}

#[test]
fn a_notes_list_past_its_ceiling_is_refused() {
    let many = vec![[0, 0x90, 60, 100]; MAX_NOTE_EVENTS + 1];
    assert_eq!(midi::notes_to_midi(&many), Err(AudioError::TooLong));
    assert!(matches!(midi::notes_to_midi(&[[70_000, 0x90, 60, 100]]), Err(AudioError::Malformed(_))));
    assert!(matches!(midi::notes_to_midi(&[[0, 0x30, 60, 100]]), Err(AudioError::Malformed(_))));
}

#[test]
fn a_midi_past_its_budget_is_refused() {
    // Ten minutes of one held note, under 1 MiB of samples.
    let bytes = smf(&[(0, 0x90, 60, 100), (480 * 600, 0x80, 60, 0)]);
    assert_eq!(midi::render(&bytes, None, 1 << 20).unwrap_err(), AudioError::TooLong);
}

#[test]
fn a_bank_too_large_falls_back_to_the_clients_voices() {
    let huge = vec![0u8; midi::MAX_SOUNDFONT_BYTES + 1];
    assert_eq!(midi::load_bank(&huge).unwrap_err(), AudioError::TooLong);
    assert!(midi::load_bank(b"RIFF\x04\x00\x00\x00sfbk").is_err());
    // And the file still plays, on the client's own voices.
    let bytes = smf(&[(0, 0x90, 60, 100), (480, 0x80, 60, 0)]);
    assert!(midi::render(&bytes, None, eui_audio::MAX_BYTES).is_ok());
}

#[test]
fn a_soundfont_plays_its_own_instruments() {
    // A real bank, when the machine running the tests has one:
    // EUI_TEST_SOUNDFONT=/path/to/GeneralUser-GS.sf2.
    let Ok(path) = std::env::var("EUI_TEST_SOUNDFONT") else {
        eprintln!("EUI_TEST_SOUNDFONT unset: the bank's own instruments are not tested here");
        return;
    };
    let bank = midi::load_bank(&std::fs::read(path).unwrap()).unwrap();
    assert!(bank.presets() > 100, "{} presets", bank.presets());
    let bytes = smf(&[(0, 0xC0, 40, 0), (0, 0x90, 67, 100), (480, 0x80, 67, 0)]);
    let violin = midi::render(&bytes, Some(&bank), eui_audio::MAX_BYTES).unwrap();
    let ours = midi::render(&bytes, None, eui_audio::MAX_BYTES).unwrap();
    assert!(energy(violin.samples(), 0, 0.0, 1.0) > 0.1);
    assert_ne!(violin.samples()[2000..2100], ours.samples()[2000..2100]);
}
