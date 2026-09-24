//! Decoding and mixing, on sounds the test writes itself.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic, clippy::arithmetic_side_effects)]

use std::sync::Arc;

use eui_audio::{decode, decode_within, AudioError, Control, Mixer, Sound, MAX_BYTES, MAX_SOURCES};

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

#[test]
fn a_sound_past_its_decoded_budget_is_refused() {
    // Half a second of 8 kHz mono is 4 000 samples, 16 000 bytes of `f32`.
    let bytes = wav(&sine(440.0, 8_000, 4_000), 8_000, 1);
    assert_eq!(decode_within(&bytes, None, 16_000).unwrap().bytes(), 16_000, "exactly the budget fits");
    assert_eq!(decode_within(&bytes, None, 15_996), Err(AudioError::TooLong), "one sample over does not");
    // A budget above the crate's own is the crate's own: no caller can
    // raise it. 128 MiB is spec 10's number, and it is the bound a 16 MB
    // file can no longer expand past — it used to be an hour of stereo.
    assert_eq!(MAX_BYTES, 128 * 1024 * 1024);
    assert!(decode_within(&bytes, None, usize::MAX).is_ok());
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

// ---------------------------------------------------------------- the meter
//
// What a `level` event carries. The property these pin is not "the number
// is right" but "the number says nothing about the machine": everything a
// meter reports has to be recoverable from the bytes the server sent and
// the gain the server asked for, and nothing else.

#[test]
fn peak_is_before_the_viewers_own_gain() {
    let mut m = Mixer::new(8_000);
    m.load(1, tone(0.8, 8_000, 4_000));
    m.control(1, Control { playing: true, volume: 1.0, looping: false });

    let mut out = [0.0f32; 16];
    m.fill(&mut out, 1);
    let (l, r) = m.take_peak(1).expect("a loaded source has a peak");
    assert!((l - 0.8).abs() < 1e-6 && (r - 0.8).abs() < 1e-6, "full gain: {l} {r}");

    // The viewer turns it down. The output follows; the meter does not.
    m.set_master(0.25);
    m.fill(&mut out, 1);
    assert!(out.iter().all(|s| (*s - 0.2).abs() < 1e-6), "master really applied: {out:?}");
    let (l, _) = m.take_peak(1).unwrap();
    assert!((l - 0.8).abs() < 1e-6, "spec 03 §7: the viewer's volume is not the application's to read, got {l}");

    // And the case that matters most: muted must be indistinguishable
    // from playing, or a server learns the viewer silenced it.
    m.set_master(0.0);
    m.fill(&mut out, 1);
    assert!(out.iter().all(|s| *s == 0.0), "muted: {out:?}");
    let (l, _) = m.take_peak(1).unwrap();
    assert!((l - 0.8).abs() < 1e-6, "a muted viewer must not be inferable, got {l}");

    // The application's own gain *is* the server's to know: it set it.
    m.control(1, Control { playing: true, volume: 0.5, looping: false });
    m.fill(&mut out, 1);
    let (l, _) = m.take_peak(1).unwrap();
    assert!((l - 0.4).abs() < 1e-6, "the application's own volume is in the reading, got {l}");
}

#[test]
fn peak_is_the_maximum_since_the_last_read() {
    let mut m = Mixer::new(8_000);
    // Quiet, with one loud frame a third of the way in.
    let mut samples = vec![0.1f32; 300];
    samples[150] = 0.9;
    m.load(1, Arc::new(Sound::new(samples, 8_000, 1)));
    m.control(1, Control { playing: true, volume: 1.0, looping: false });

    // Nothing has played yet.
    assert_eq!(m.take_peak(1), Some((0.0, 0.0)));

    // Three fills of 100 frames each: the transient is in the second, and
    // only the drain after all three reads it. A meter that sampled
    // instead of accumulating would miss it.
    let mut out = [0.0f32; 100];
    for _ in 0..3 {
        m.fill(&mut out, 1);
    }
    let (l, _) = m.take_peak(1).unwrap();
    assert!((l - 0.9).abs() < 1e-3, "the transient between two readings survives, got {l}");

    // A read drains: the same peak is not reported twice.
    assert_eq!(m.take_peak(1), Some((0.0, 0.0)), "a reading covers one interval, once");
    assert_eq!(m.take_peak(999), None, "a node with no sound has no reading");
}

#[test]
fn peak_is_stereo_even_on_a_mono_output() {
    let lr: Vec<f32> = (0..200).flat_map(|_| [1.0, -0.25]).collect();
    let mut m = Mixer::new(8_000);
    m.load(1, Arc::new(Sound::new(lr, 8_000, 2)));
    m.control(1, Control { playing: true, volume: 1.0, looping: false });

    // One channel out. The right side is still measured, and the sign is
    // not the meter's business.
    let mut out = [0.0f32; 8];
    m.fill(&mut out, 1);
    let (l, r) = m.take_peak(1).unwrap();
    assert!((l - 1.0).abs() < 1e-6, "left: {l}");
    assert!((r - 0.25).abs() < 1e-6, "right survives a mono device, and is absolute: {r}");
}

#[test]
fn a_stopped_source_peaks_at_zero() {
    let mut m = Mixer::new(8_000);
    m.load(1, tone(0.7, 8_000, 4_000));
    m.control(1, Control { playing: true, volume: 1.0, looping: false });
    let mut out = [0.0f32; 16];
    m.fill(&mut out, 1);
    assert!(m.take_peak(1).unwrap().0 > 0.5);

    // Paused. The meter must fall, or the last bar stays lit for ever.
    m.control(1, Control { playing: false, volume: 1.0, looping: false });
    m.fill(&mut out, 1);
    assert_eq!(m.take_peak(1), Some((0.0, 0.0)), "a paused sound reads zero");

    // A replacement starts from a clean meter, which is the opposite of
    // what `a_control_survives_the_sound_being_replaced` pins for the
    // control: the gain is the application's intent and carries over, the
    // reading is the sound's and does not.
    m.control(1, Control { playing: true, volume: 1.0, looping: false });
    m.fill(&mut out, 1);
    m.load(1, tone(0.3, 8_000, 4_000));
    assert_eq!(m.take_peak(1), Some((0.0, 0.0)), "a new sound starts from a clean meter");
}
