//! Spec 03 §7: the audio device, which belongs to the window.
//!
//! The worker decodes and mixes ([`eui_audio`]); this opens the platform's
//! output and keeps it fed. Two threads meet here and neither may block
//! the other for long: a **filler** asks the driver for frames every few
//! milliseconds and appends them to a buffer, and the device's own
//! callback takes what it needs from that buffer. The callback holds the
//! lock only for a copy; the filler prepares its frames before taking it.
//! An empty buffer is silence, not a stall.
//!
//! The device is open only while the session has a sound loaded, so an
//! application that plays nothing wakes nothing.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::worker::AudioTap;

/// Frames held ahead of the device. Enough that a paint holding the
/// driver's lock cannot starve the callback; short enough that a pause
/// is heard as a pause.
const BUFFER_MS: u32 = 200;

/// How often the filler tops the buffer up.
const FILL_EVERY: Duration = Duration::from_millis(25);

/// Device callbacks that found the ring short, and the frames they could
/// not fill. An underrun *is* the crackle: the callback writes silence into
/// the middle of a sound, and there is no way to hear that as anything else.
///
/// Counted always and printed only under `EUI_AUDIO_STATS=1`, because the
/// alternative is guessing at a symptom that has already been mistaken for
/// two different faults. Nothing here decides anything — it says whether
/// the ring ran dry, which is the one question that separates "the supply
/// is late" from "what is being supplied is wrong".
pub(crate) static UNDERRUNS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub(crate) static STARVED_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// An open output device, and the thread that feeds it. Dropping it stops
/// both.
pub struct Output {
    stream: cpal::Stream,
    stop: Arc<AtomicBool>,
    filler: Option<std::thread::JoinHandle<()>>,
    rate: u32,
    channels: u16,
}

impl std::fmt::Debug for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Output").field("rate", &self.rate).field("channels", &self.channels).finish()
    }
}

impl Output {
    /// Open the default output and start feeding it from `tap`. Frames the
    /// mix produces — a sound's `ended` — go to `out`, and `wake` is called
    /// so the window's loop comes to collect them.
    pub fn start(tap: AudioTap, out: mpsc::Sender<Vec<u8>>, wake: impl Fn() + Send + 'static) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no output device")?;
        let config = device.default_output_config().map_err(|e| format!("no output config: {e}"))?;
        let rate = config.sample_rate().0.max(1);
        // More than two channels: the first two carry the sound and the
        // rest stay silent. A player is not a surround mixer.
        let device_channels = config.channels().max(1);
        let channels = device_channels.min(2);
        let ring: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let format = config.sample_format();
        let config: cpal::StreamConfig = config.into();

        let callback_ring = Arc::clone(&ring);
        let err = |e| eprintln!("eui: audio device: {e}");
        let stream = match format {
            cpal::SampleFormat::F32 => device.build_output_stream(&config, move |data: &mut [f32], _| fill_device(data, device_channels, channels, &callback_ring), err, None),
            other => return Err(format!("the device wants {other} samples, which this client does not write")),
        }
        .map_err(|e| format!("cannot open the output: {e}"))?;
        stream.play().map_err(|e| format!("cannot start the output: {e}"))?;

        let stop = Arc::new(AtomicBool::new(false));
        let filler_stop = Arc::clone(&stop);
        let filler_ring = Arc::clone(&ring);
        let want = (rate.saturating_mul(BUFFER_MS) / 1000) as usize;
        let filler = std::thread::Builder::new()
            .name("eui-audio".into())
            .spawn(move || {
                let stats = std::env::var("EUI_AUDIO_STATS").is_ok_and(|v| v != "0");
                let mut last_underruns = 0u64;
                while !filler_stop.load(Ordering::SeqCst) {
                    let have = filler_ring.lock().map(|r| r.len()).unwrap_or(0) / usize::from(channels);
                    if have < want {
                        // Prepared outside the lock: the callback must never
                        // wait on a driver that is busy painting.
                        let mut chunk = vec![0.0f32; (want - have) * usize::from(channels)];
                        let frames = tap.fill(&mut chunk, channels, rate);
                        // Silence is never queued, and that is the whole of
                        // the latency fix.
                        //
                        // `fill_device` zeroes its buffer and returns the
                        // moment the ring runs out, so an empty ring already
                        // *is* silence — queueing it buys nothing. What it
                        // cost was a delay on every sound: the device is
                        // opened as soon as a sound is loaded, not when it
                        // plays, so a chime sitting paused kept 200 ms of
                        // zeroes in front of the speaker. Pressing Play then
                        // had to drain them first, and the sound arrived a
                        // fifth of a second late — which is audible, and was
                        // reported as lag.
                        //
                        // The tap is still called at the same cadence: it
                        // moves the mixer's clock and it is where `ended`
                        // comes from. Only the pushing is skipped, and only
                        // when there is nothing in the chunk at all. A chunk
                        // that starts silent and turns into a sound halfway
                        // is queued whole.
                        if chunk.iter().any(|s| *s != 0.0) {
                            if let Ok(mut r) = filler_ring.lock() {
                                r.extend(chunk);
                            }
                        }
                        if !frames.is_empty() {
                            for f in frames {
                                if out.send(f).is_err() {
                                    return;
                                }
                            }
                            wake();
                        }
                    }
                    if stats {
                        let n = UNDERRUNS.load(Ordering::Relaxed);
                        if n != last_underruns {
                            last_underruns = n;
                            eprintln!(
                                "eui: audio underruns {n}, {} frames of silence written into a sound, ring {}/{} ms",
                                STARVED_FRAMES.load(Ordering::Relaxed),
                                have * 1000 / rate.max(1) as usize,
                                BUFFER_MS
                            );
                        }
                    }
                    std::thread::sleep(FILL_EVERY);
                }
            })
            .map_err(|e| format!("cannot start the audio thread: {e}"))?;

        Ok(Self { stream, stop, filler: Some(filler), rate, channels })
    }

    /// The device's frame rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Channels the mix writes: one or two, whatever the device has.
    pub fn channels(&self) -> u16 {
        self.channels
    }
}

impl Drop for Output {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = self.stream.pause();
        if let Some(t) = self.filler.take() {
            let _ = t.join();
        }
    }
}

/// The device's callback: take what is buffered, pad with silence, and
/// leave any channels past the second silent.
fn fill_device(data: &mut [f32], device_channels: u16, mix_channels: u16, ring: &Arc<Mutex<VecDeque<f32>>>) {
    for s in data.iter_mut() {
        *s = 0.0;
    }
    let Ok(mut r) = ring.lock() else { return };
    let dev = usize::from(device_channels.max(1));
    let mix = usize::from(mix_channels.max(1));
    for frame in data.chunks_mut(dev) {
        for c in 0..mix.min(dev) {
            match r.pop_front() {
                Some(s) => {
                    if let Some(slot) = frame.get_mut(c) {
                        *slot = s;
                    }
                }
                // Nothing buffered: silence, and the filler catches up.
                // Counted on the way out — this is the crackle itself.
                None => {
                    UNDERRUNS.fetch_add(1, Ordering::Relaxed);
                    let done = (frame.as_ptr() as usize).saturating_sub(data.as_ptr() as usize) / (dev * size_of::<f32>());
                    STARVED_FRAMES.fetch_add((data.len() / dev).saturating_sub(done) as u64, Ordering::Relaxed);
                    return;
                }
            }
        }
    }
}
