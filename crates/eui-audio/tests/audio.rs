//! Decoding and mixing, on sounds the test writes itself.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use std::sync::Arc;

use eui_audio::{decode, AudioError, Control, Mixer, Sound, MAX_SOURCES};

/// A RIFF/WAVE file of 16-bit samples, so the tests need no fixture.
fn wav(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let block_align = channels * 2;
    let byte_rate = rate * u32::from(block_align);
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36u32 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM header length
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// A second of a sine at `hz`, as 16-bit mono.
fn sine(hz: f32, rate: u32, frames: usize) -> Vec<i16> {
    (0..frames)
        .map(|i| {
            let t = i as f32 / rate as f32;
            ((t * hz * std::f32::consts::TAU).sin() * 16_000.0) as i16
        })
        .collect()
}

#[test]
fn a_wav_decodes_to_its_own_rate_and_channels() {
    let bytes = wav(&sine(440.0, 8_000, 4_000), 8_000, 1);
    let sound = decode(&bytes, Some("wav")).unwrap();
    assert_eq!(sound.rate(), 8_000);
    assert_eq!(sound.channels(), 1);
    assert_eq!(sound.frames(), 4_000);
    assert_eq!(sound.duration_ms(), 500);
    assert_eq!(sound.bytes(), 4_000 * 4);
    // The samples are the sine, scaled into `-1..=1`.
    assert!(sound.samples().iter().all(|s| s.abs() <= 1.0));
    assert!(sound.samples().iter().any(|s| *s > 0.4));
    // Stereo keeps both channels.
    let stereo: Vec<i16> = (0..2_000).flat_map(|i| [i as i16, -(i as i16)]).collect();
    let sound = decode(&wav(&stereo, 44_100, 2), None).unwrap();
    assert_eq!((sound.channels(), sound.frames(), sound.rate()), (2, 2_000, 44_100));
}

#[test]
fn rubbish_is_refused_rather_than_guessed_at() {
    assert!(matches!(decode(b"", None), Err(AudioError::Unsupported(_) | AudioError::Malformed(_))));
    assert!(matches!(decode(&[0xff; 4096], Some("wav")), Err(AudioError::Unsupported(_) | AudioError::Malformed(_))));
    // A header that promises more than it delivers is not a panic.
    let mut truncated = wav(&sine(440.0, 8_000, 4_000), 8_000, 1);
    truncated.truncate(60);
    let _ = decode(&truncated, Some("wav"));
    // A PNG is not a sound.
    assert!(decode(b"\x89PNG\r\n\x1a\n", Some("png")).is_err());
}

fn tone(level: f32, rate: u32, frames: usize) -> Arc<Sound> {
    Arc::new(Sound::new(vec![level; frames], rate, 1))
}

#[test]
fn a_source_plays_only_when_told_and_reports_its_end() {
    let mut m = Mixer::new(8_000);
    let node = 7;
    assert!(m.load(node, tone(0.5, 8_000, 100)));
    assert_eq!(m.duration_ms(node), Some(12));
    // Loaded but not playing: silence, and no end.
    let mut out = [9.0f32; 32];
    assert!(m.fill(&mut out, 1).is_empty());
    assert!(out.iter().all(|s| *s == 0.0), "the buffer is overwritten, not added to");
    assert!(!m.playing(node));
    // Playing: the tone, at its own level.
    m.control(node, Control { playing: true, volume: 1.0, looping: false });
    assert!(m.fill(&mut out, 1).is_empty());
    assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6), "{out:?}");
    assert!(m.playing(node));
    assert_eq!(m.position_ms(node), Some(4));
    // Volume scales it.
    m.control(node, Control { playing: true, volume: 0.5, looping: false });
    m.fill(&mut out, 1);
    assert!(out.iter().all(|s| (*s - 0.25).abs() < 1e-6));
    // Past the end: the tail is silence, the end is reported once.
    let mut rest = [0.0f32; 128];
    assert_eq!(m.fill(&mut rest, 1), vec![node], "the end is reported");
    assert!(m.fill(&mut rest, 1).is_empty(), "and only once");
    assert!(!m.playing(node));
    assert!(rest[100..].iter().all(|s| *s == 0.0), "silence past the end");
    // Playing again from the end starts over.
    m.control(node, Control { playing: false, volume: 1.0, looping: false });
    m.control(node, Control { playing: true, volume: 1.0, looping: false });
    assert_eq!(m.position_ms(node), Some(0));
}

#[test]
fn a_looping_source_never_ends() {
    let mut m = Mixer::new(8_000);
    m.load(1, tone(0.25, 8_000, 10));
    m.control(1, Control { playing: true, volume: 1.0, looping: true });
    let mut out = [0.0f32; 100];
    assert!(m.fill(&mut out, 1).is_empty());
    assert!(out.iter().all(|s| (*s - 0.25).abs() < 1e-6), "the whole buffer is full of tone, no gap at the wrap: {out:?}");
    assert!(m.playing(1));
    // Ten loops of ten frames is a hundred frames: the position is where
    // arithmetic says, not drifting.
    assert_eq!(m.position_ms(1), Some(0));
}

#[test]
fn seeking_moves_a_source_and_is_clamped() {
    let ramp: Vec<f32> = (0..1_000).map(|i| i as f32 / 1_000.0).collect();
    let mut m = Mixer::new(1_000);
    m.load(1, Arc::new(Sound::new(ramp, 1_000, 1)));
    m.control(1, Control { playing: true, volume: 1.0, looping: false });
    m.seek(1, 500);
    assert_eq!(m.position_ms(1), Some(500));
    let mut out = [0.0f32; 1];
    m.fill(&mut out, 1);
    assert!((out[0] - 0.5).abs() < 1e-3, "{}", out[0]);
    // Past the end lands on the end, not outside it.
    m.seek(1, 99_999);
    assert_eq!(m.position_ms(1), Some(1_000));
    assert!(!m.playing(1));
    // And back to the start.
    m.seek(1, 0);
    assert_eq!(m.position_ms(1), Some(0));
    assert!(m.playing(1));
}

#[test]
fn two_sources_mix_and_the_sum_is_clamped() {
    let mut m = Mixer::new(8_000);
    m.load(1, tone(0.5, 8_000, 1_000));
    m.load(2, tone(0.75, 8_000, 1_000));
    m.control(1, Control { playing: true, volume: 1.0, looping: false });
    m.control(2, Control { playing: true, volume: 1.0, looping: false });
    let mut out = [0.0f32; 16];
    m.fill(&mut out, 1);
    assert!(out.iter().all(|s| (*s - 1.0).abs() < 1e-6), "0.5 + 0.75 clamps to 1.0: {out:?}");
    // The viewer's own gain is over everything and the application cannot
    // see it.
    m.set_master(0.5);
    m.fill(&mut out, 1);
    assert!(out.iter().all(|s| (*s - 0.625).abs() < 1e-6), "{out:?}");
}

#[test]
fn a_source_is_resampled_to_the_output_rate() {
    // A ramp at 1 kHz played at 2 kHz lasts twice as long and rises half
    // as fast, interpolated in between.
    let ramp: Vec<f32> = (0..10).map(|i| i as f32 / 10.0).collect();
    let mut m = Mixer::new(2_000);
    m.load(1, Arc::new(Sound::new(ramp, 1_000, 1)));
    m.control(1, Control { playing: true, volume: 1.0, looping: false });
    let mut out = [0.0f32; 20];
    assert!(m.fill(&mut out, 1).is_empty(), "twenty output frames is exactly its length");
    for (i, s) in out.iter().enumerate() {
        let want = (i as f32 / 2.0 / 10.0).min(0.9);
        assert!((s - want).abs() < 1e-3, "sample {i}: {s} vs {want}");
    }
    // It ends on the next fill, which finds nothing left.
    assert_eq!(m.fill(&mut out, 1), vec![1]);
    assert!(out.iter().all(|s| *s == 0.0));
}

#[test]
fn a_mono_source_reaches_both_channels_and_stereo_stays_stereo() {
    let mut m = Mixer::new(8_000);
    m.load(1, tone(0.5, 8_000, 100));
    m.control(1, Control { playing: true, volume: 1.0, looping: false });
    let mut out = [0.0f32; 8];
    m.fill(&mut out, 2);
    assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6), "mono plays on both: {out:?}");
    // A stereo source keeps its sides apart.
    let lr: Vec<f32> = (0..200).flat_map(|_| [1.0, -1.0]).collect();
    let mut m = Mixer::new(8_000);
    m.load(1, Arc::new(Sound::new(lr, 8_000, 2)));
    m.control(1, Control { playing: true, volume: 1.0, looping: false });
    m.fill(&mut out, 2);
    for pair in out.chunks(2) {
        assert!((pair[0] - 1.0).abs() < 1e-6 && (pair[1] + 1.0).abs() < 1e-6, "{pair:?}");
    }
}

#[test]
fn the_tree_owns_the_sources() {
    let mut m = Mixer::new(8_000);
    for id in 1..=4 {
        assert!(m.load(id, tone(0.1, 8_000, 10)));
    }
    assert_eq!(m.len(), 4);
    assert_eq!(m.bytes(), 4 * 10 * 4);
    // The nodes that are still in the tree keep their sound; the others go.
    m.retain(&[2, 3]);
    assert_eq!(m.len(), 2);
    assert!(m.has(2) && !m.has(1));
    m.remove(2);
    assert!(!m.has(2));
    // A ninth sound is refused rather than queued.
    let mut m = Mixer::new(8_000);
    for id in 0..MAX_SOURCES as u32 {
        assert!(m.load(id, tone(0.1, 8_000, 10)), "source {id}");
    }
    assert!(!m.load(99, tone(0.1, 8_000, 10)), "the {}th is refused", MAX_SOURCES + 1);
    // Reloading one that is already there is not a new source.
    assert!(m.load(0, tone(0.2, 8_000, 10)));
    // An unknown node is not an error, and a control set before the sound
    // arrives is kept.
    m.control(1234, Control { playing: true, volume: 1.0, looping: false });
    assert_eq!(m.position_ms(1234), None);
}

#[test]
fn a_control_survives_the_sound_being_replaced() {
    let mut m = Mixer::new(8_000);
    m.load(1, tone(0.5, 8_000, 100));
    m.control(1, Control { playing: true, volume: 0.5, looping: true });
    m.load(1, tone(1.0, 8_000, 100));
    let mut out = [0.0f32; 4];
    m.fill(&mut out, 1);
    assert!(out.iter().all(|s| (*s - 0.5).abs() < 1e-6), "still playing at half: {out:?}");
    assert_eq!(m.position_ms(1), Some(0), "the new sound starts at its beginning");
}
