//! # EUI audio
//!
//! Two things, both pure: turning the bytes of a sound into PCM, and
//! mixing the sounds that are playing into one buffer of frames.
//!
//! There is no device here, and no thread. That is deliberate. Decoding
//! runs on bytes a server chose, so it belongs where every other decoder
//! belongs — the sandboxed worker (spec 08 §10) — while opening an audio
//! device is a platform privilege and belongs to the window process, like
//! the GPU. This crate is what the worker runs; [`Mixer::fill`] is what
//! the window's audio callback ultimately consumes.
//!
//! Everything is `f32` interleaved. A source is resampled to the output
//! rate as it plays, by linear interpolation: cheap, and honest about
//! what it is — a player, not a mastering chain.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// Sample arithmetic on values already bounded by the decoder.
#![allow(clippy::arithmetic_side_effects)]

use std::collections::HashMap;
use std::sync::Arc;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Why a sound could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    /// No decoder for these bytes, or they are not a sound at all.
    Unsupported(String),
    /// The stream is malformed.
    Malformed(String),
    /// The sound is longer than [`MAX_FRAMES`] allows.
    TooLong,
    /// The stream declares no audio track.
    NoTrack,
}

impl std::fmt::Display for AudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(w) => write!(f, "unsupported audio: {w}"),
            Self::Malformed(w) => write!(f, "malformed audio: {w}"),
            Self::TooLong => write!(f, "the sound is longer than the limit"),
            Self::NoTrack => write!(f, "the stream has no audio track"),
        }
    }
}

impl std::error::Error for AudioError {}

/// Frames a single sound may hold: an hour of stereo at 48 kHz, about
/// 690 MB decoded — the real bound is the session's asset quota, this one
/// is the backstop that keeps a crafted header from asking for terabytes.
pub const MAX_FRAMES: usize = 48_000 * 3_600;

/// Sources one mixer plays at once. A tenth sound is refused rather than
/// queued: an application that wants more is doing something the client
/// should not pay for.
pub const MAX_SOURCES: usize = 8;

/// A decoded sound: interleaved `f32` frames at its own sample rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Sound {
    /// Interleaved samples, `channels` per frame.
    samples: Vec<f32>,
    /// Sample rate the samples are at.
    rate: u32,
    /// Channels per frame, 1 or 2 (more are folded to 2 when decoded).
    channels: u16,
}

impl Sound {
    /// A sound from samples already decoded, for tests and generators.
    /// `channels` is clamped to 1 or 2 and the tail of a partial frame is
    /// dropped, so a `Sound` always holds whole frames.
    pub fn new(mut samples: Vec<f32>, rate: u32, channels: u16) -> Self {
        let channels = channels.clamp(1, 2);
        let rate = rate.max(1);
        let whole = samples.len() - samples.len() % usize::from(channels);
        samples.truncate(whole);
        Self { samples, rate, channels }
    }

    /// Interleaved samples.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// Sample rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Channels per frame.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// Frames.
    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    /// Duration in milliseconds.
    pub fn duration_ms(&self) -> u64 {
        (self.frames() as u64).saturating_mul(1000) / u64::from(self.rate)
    }

    /// Bytes the decoded sound occupies, for the session's quota.
    pub fn bytes(&self) -> usize {
        self.samples.len().saturating_mul(std::mem::size_of::<f32>())
    }

    /// The frame at `i`, as `(left, right)`; silence past the end.
    fn frame(&self, i: usize) -> (f32, f32) {
        let c = usize::from(self.channels);
        let base = i.saturating_mul(c);
        let l = self.samples.get(base).copied().unwrap_or(0.0);
        let r = if c == 2 { self.samples.get(base + 1).copied().unwrap_or(0.0) } else { l };
        (l, r)
    }
}

/// Decode a sound. `hint` is a file extension or MIME type when the
/// caller knows one; the probe works without it.
///
/// More than two channels are folded to stereo (the first two), because
/// a client that draws a window is not a home cinema and a surround
/// stream should not cost six times the memory.
pub fn decode(bytes: &[u8], hint: Option<&str>) -> Result<Sound, AudioError> {
    let source = std::io::Cursor::new(bytes.to_vec());
    let stream = MediaSourceStream::new(Box::new(source), Default::default());
    let mut probe_hint = Hint::new();
    if let Some(h) = hint {
        let h = h.rsplit('/').next().unwrap_or(h).trim_start_matches('.');
        probe_hint.with_extension(h);
    }
    let probed = symphonia::default::get_probe().format(&probe_hint, stream, &FormatOptions::default(), &MetadataOptions::default()).map_err(|e| AudioError::Unsupported(e.to_string()))?;
    let mut format = probed.format;
    let track = format.tracks().iter().find(|t| t.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_NULL).ok_or(AudioError::NoTrack)?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default()).map_err(|e| AudioError::Unsupported(e.to_string()))?;

    let mut samples: Vec<f32> = Vec::new();
    let mut rate = track.codec_params.sample_rate.unwrap_or(44_100);
    let mut channels: u16 = 2;
    let mut buffer: Option<SampleBuffer<f32>> = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // End of stream, whatever shape the reader reports it in.
            Err(symphonia::core::errors::Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(symphonia::core::errors::Error::ResetRequired) => break,
            Err(e) => {
                if samples.is_empty() {
                    return Err(AudioError::Malformed(e.to_string()));
                }
                break;
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged packet is skipped; a damaged stream ends here.
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(e) => {
                if samples.is_empty() {
                    return Err(AudioError::Malformed(e.to_string()));
                }
                break;
            }
        };
        let spec = *decoded.spec();
        rate = spec.rate;
        let src_channels = spec.channels.count().max(1);
        channels = if src_channels == 1 { 1 } else { 2 };
        let buf = buffer.get_or_insert_with(|| SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
        buf.copy_interleaved_ref(decoded);
        let frame_len = usize::from(channels);
        for frame in buf.samples().chunks(src_channels) {
            if samples.len() / frame_len >= MAX_FRAMES {
                return Err(AudioError::TooLong);
            }
            samples.push(frame.first().copied().unwrap_or(0.0));
            if frame_len == 2 {
                samples.push(frame.get(1).copied().unwrap_or_else(|| frame.first().copied().unwrap_or(0.0)));
            }
        }
    }
    if samples.is_empty() {
        return Err(AudioError::Malformed("no frames decoded".into()));
    }
    Ok(Sound::new(samples, rate, channels))
}

/// What a source is doing, as the application asked for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Control {
    /// Playing, or held where it is.
    pub playing: bool,
    /// Gain, `0.0..=1.0`.
    pub volume: f32,
    /// Start again at the end rather than stopping.
    pub looping: bool,
}

impl Default for Control {
    fn default() -> Self {
        Self { playing: false, volume: 1.0, looping: false }
    }
}

/// One playing sound.
#[derive(Debug, Clone)]
struct Source {
    sound: Arc<Sound>,
    control: Control,
    /// Position in source frames, fractional because the source rate and
    /// the output rate rarely agree.
    at: f64,
    /// The source reached its end since the last [`Mixer::fill`].
    ended: bool,
}

/// The sounds a session is playing, mixed to one output rate.
///
/// Sources are named by the id of the node that owns them, so a tree
/// update that drops the node drops the sound with it.
#[derive(Debug)]
pub struct Mixer {
    rate: u32,
    sources: HashMap<u32, Source>,
    /// Output gain, the viewer's own volume; the application cannot see it.
    master: f32,
}

impl Mixer {
    /// A mixer for an output at `rate` frames a second.
    pub fn new(rate: u32) -> Self {
        Self { rate: rate.max(1), sources: HashMap::new(), master: 1.0 }
    }

    /// The output rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Change the output rate — the device came back at another one. Every
    /// source keeps its position in its own frames, so nothing jumps.
    pub fn set_rate(&mut self, rate: u32) {
        self.rate = rate.max(1);
    }

    /// The viewer's own gain over everything, `0.0..=1.0`.
    pub fn set_master(&mut self, gain: f32) {
        self.master = gain.clamp(0.0, 1.0);
    }

    /// Sources currently loaded.
    pub fn len(&self) -> usize {
        self.sources.len()
    }

    /// True when nothing is loaded.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// True when `node` has a sound loaded.
    pub fn has(&self, node: u32) -> bool {
        self.sources.contains_key(&node)
    }

    /// Give `node` its sound, at the start and not playing. Replacing a
    /// node's sound starts the new one from the beginning. `false` when
    /// the mixer already holds [`MAX_SOURCES`] other sounds.
    pub fn load(&mut self, node: u32, sound: Arc<Sound>) -> bool {
        if !self.sources.contains_key(&node) && self.sources.len() >= MAX_SOURCES {
            return false;
        }
        let control = self.sources.get(&node).map_or_else(Control::default, |s| s.control);
        self.sources.insert(node, Source { sound, control, at: 0.0, ended: false });
        true
    }

    /// Forget a node's sound.
    pub fn remove(&mut self, node: u32) {
        self.sources.remove(&node);
    }

    /// Keep only these nodes: the tree no longer holds the others.
    pub fn retain(&mut self, nodes: &[u32]) {
        self.sources.retain(|id, _| nodes.contains(id));
    }

    /// Set what a source is doing. Unknown nodes are ignored — the sound
    /// may not have arrived yet; [`Self::load`] keeps the control.
    pub fn control(&mut self, node: u32, control: Control) {
        if let Some(s) = self.sources.get_mut(&node) {
            let was = s.control;
            s.control = Control { volume: control.volume.clamp(0.0, 1.0), ..control };
            // Playing again after the end starts over, as a player does.
            if !was.playing && control.playing && s.at >= s.sound.frames() as f64 {
                s.at = 0.0;
            }
        }
    }

    /// Move a source to `ms` from its start, clamped to its length.
    pub fn seek(&mut self, node: u32, ms: u64) {
        if let Some(s) = self.sources.get_mut(&node) {
            let frames = (ms as f64) * f64::from(s.sound.rate()) / 1000.0;
            s.at = frames.clamp(0.0, s.sound.frames() as f64);
            s.ended = false;
        }
    }

    /// Where a source is, in milliseconds from its start.
    pub fn position_ms(&self, node: u32) -> Option<u64> {
        let s = self.sources.get(&node)?;
        Some((s.at * 1000.0 / f64::from(s.sound.rate())) as u64)
    }

    /// How long a source's sound is, in milliseconds.
    pub fn duration_ms(&self, node: u32) -> Option<u64> {
        Some(self.sources.get(&node)?.sound.duration_ms())
    }

    /// True while a source has frames left to play.
    pub fn playing(&self, node: u32) -> bool {
        self.sources.get(&node).is_some_and(|s| s.control.playing && s.at < s.sound.frames() as f64)
    }

    /// Bytes the loaded sounds occupy.
    pub fn bytes(&self) -> usize {
        self.sources.values().map(|s| s.sound.bytes()).sum()
    }

    /// Mix into `out`, `channels` samples per frame, interleaved. Returns
    /// the nodes whose sound reached its end during this fill, in the
    /// order they are stored — each reported once, until it plays again.
    ///
    /// `out` is overwritten, silence included: an audio callback hands us
    /// whatever was in the buffer before.
    pub fn fill(&mut self, out: &mut [f32], channels: u16) -> Vec<u32> {
        for s in out.iter_mut() {
            *s = 0.0;
        }
        let channels = usize::from(channels.clamp(1, 2));
        let mut ended = Vec::new();
        for (node, source) in &mut self.sources {
            if !source.control.playing {
                continue;
            }
            let frames = source.sound.frames();
            if frames == 0 {
                continue;
            }
            let step = f64::from(source.sound.rate()) / f64::from(self.rate);
            let gain = source.control.volume * self.master;
            for frame in out.chunks_mut(channels) {
                if source.at >= frames as f64 {
                    if !source.control.looping {
                        if !source.ended {
                            source.ended = true;
                            ended.push(*node);
                        }
                        break;
                    }
                    // Wrap keeping the overshoot, so a loop does not drift
                    // by a fraction of a frame every time round.
                    source.at -= frames as f64;
                    // `max` returns the other operand for a NaN, which is
                    // the point: an overshoot that went bad starts again.
                    source.at = source.at.max(0.0);
                }
                let i = (source.at as usize).min(frames.saturating_sub(1));
                // Linear interpolation between the two neighbouring frames.
                // The last frame interpolates towards itself: a sound must
                // not fade into a click at its end.
                let t = (source.at - i as f64) as f32;
                let (l0, r0) = source.sound.frame(i);
                let (l1, r1) = if i + 1 < frames { source.sound.frame(i + 1) } else { (l0, r0) };
                let l = (l0 + (l1 - l0) * t) * gain;
                let r = (r0 + (r1 - r0) * t) * gain;
                if let Some(s) = frame.first_mut() {
                    *s += l;
                }
                if channels == 2 {
                    if let Some(s) = frame.get_mut(1) {
                        *s += r;
                    }
                }
                source.at += step;
            }
            // A loop that lands exactly on the end is at its start, not
            // past it: otherwise the source reads as finished between two
            // buffers.
            if source.control.looping && source.at >= frames as f64 {
                source.at %= frames as f64;
            }
        }
        // Two sounds at full volume must not wrap around; clamping is what
        // every mixer does and what a listener expects.
        for s in out.iter_mut() {
            *s = s.clamp(-1.0, 1.0);
        }
        ended.sort_unstable();
        ended
    }
}
