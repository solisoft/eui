//! The General MIDI synthesizer macOS ships, for a MIDI sound that came
//! without a SoundFont (spec 03 §7).
//!
//! `eui-audio` renders a MIDI file once, offline, to stereo samples: with a
//! SoundFont bank through `rustysynth`, and without one on its own voices —
//! one a General MIDI family, so a violin, a viola and a cello all sound
//! alike. macOS has a whole General MIDI set already, Apple's DLS synth
//! (AudioToolbox's `kAudioUnitSubType_DLSSynth`, the instruments of
//! `gs_instruments.dls`), and a "music device" Audio Unit is played the same
//! way whichever instrument it is: notes in, samples pulled out. So without a
//! bank, a Mac renders with it; every other platform keeps the client's own
//! voices.
//!
//! The unit is driven offline, as the rest of the render is: the song's
//! channel messages are handed over slice by slice at their sample offsets
//! (`MusicDeviceMIDIEvent`) and each slice is pulled with `AudioUnitRender`
//! into the two channels — no audio device, no thread, no callback. Any
//! status that is not `noErr`, or a render that comes back silent, answers
//! `None`, and the caller falls back to its own voices.
//!
//! Every call is `unsafe`, which is why this is a crate: `eui-audio` and
//! `eui-client` are `#![forbid(unsafe_code)]`. The same split as
//! `eui-cursor`: the caller does the safe half — parsing the file, the
//! budget, the fallback — and hands over plain events.

#![allow(unsafe_code)]

/// One channel message, at its time in seconds: `(time, status, data1, data2)`.
pub type Event = (f64, u8, u8, u8);

/// The song's channel messages rendered with the platform's General MIDI
/// synthesizer, `frames` frames at `rate` per second, as the left and the
/// right channel. `None` where there is no such synthesizer (anywhere but
/// macOS), or when it fails.
pub fn render(events: &[Event], frames: usize, rate: u32) -> Option<(Vec<f32>, Vec<f32>)> {
    #[cfg(target_os = "macos")]
    {
        mac::render(events, frames, rate)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (events, frames, rate);
        None
    }
}

/// Whether this platform has a General MIDI synthesizer [`render`] can use.
pub fn available() -> bool {
    cfg!(target_os = "macos")
}

#[cfg(target_os = "macos")]
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod mac {
    use core::ffi::c_void;
    use core::ptr;

    use super::Event;

    type OsStatus = i32;
    type AudioComponent = *mut c_void;
    type AudioUnit = *mut c_void;

    #[repr(C)]
    struct AudioComponentDescription {
        component_type: u32,
        component_sub_type: u32,
        component_manufacturer: u32,
        component_flags: u32,
        component_flags_mask: u32,
    }

    #[repr(C)]
    struct AudioStreamBasicDescription {
        sample_rate: f64,
        format_id: u32,
        format_flags: u32,
        bytes_per_packet: u32,
        frames_per_packet: u32,
        bytes_per_frame: u32,
        channels_per_frame: u32,
        bits_per_channel: u32,
        reserved: u32,
    }

    #[repr(C)]
    struct SmpteTime {
        subframes: i16,
        subframe_divisor: i16,
        counter: u32,
        kind: u32,
        flags: u32,
        hours: i16,
        minutes: i16,
        seconds: i16,
        frames: i16,
    }

    #[repr(C)]
    struct AudioTimeStamp {
        sample_time: f64,
        host_time: u64,
        rate_scalar: f64,
        word_clock_time: u64,
        smpte_time: SmpteTime,
        flags: u32,
        reserved: u32,
    }

    #[repr(C)]
    struct AudioBuffer {
        number_channels: u32,
        data_byte_size: u32,
        data: *mut c_void,
    }

    /// `AudioBufferList` with room for the two buffers a non-interleaved
    /// stereo stream renders into.
    #[repr(C)]
    struct StereoBufferList {
        number_buffers: u32,
        buffers: [AudioBuffer; 2],
    }

    #[link(name = "AudioToolbox", kind = "framework")]
    extern "C" {
        fn AudioComponentFindNext(component: AudioComponent, desc: *const AudioComponentDescription) -> AudioComponent;
        fn AudioComponentInstanceNew(component: AudioComponent, out: *mut AudioUnit) -> OsStatus;
        fn AudioComponentInstanceDispose(unit: AudioUnit) -> OsStatus;
        fn AudioUnitSetProperty(unit: AudioUnit, id: u32, scope: u32, element: u32, data: *const c_void, size: u32) -> OsStatus;
        fn AudioUnitInitialize(unit: AudioUnit) -> OsStatus;
        fn AudioUnitUninitialize(unit: AudioUnit) -> OsStatus;
        fn AudioUnitRender(unit: AudioUnit, flags: *mut u32, stamp: *const AudioTimeStamp, bus: u32, frames: u32, data: *mut StereoBufferList) -> OsStatus;
        fn MusicDeviceMIDIEvent(unit: AudioUnit, status: u32, data1: u32, data2: u32, offset: u32) -> OsStatus;
    }

    const fn four_cc(code: &[u8; 4]) -> u32 {
        ((code[0] as u32) << 24) | ((code[1] as u32) << 16) | ((code[2] as u32) << 8) | (code[3] as u32)
    }

    const MUSIC_DEVICE: u32 = four_cc(b"aumu");
    const DLS_SYNTH: u32 = four_cc(b"dls ");
    const APPLE: u32 = four_cc(b"appl");
    const LINEAR_PCM: u32 = four_cc(b"lpcm");
    const FLOAT_PACKED_NON_INTERLEAVED: u32 = 1 | 8 | 32;
    const PROPERTY_STREAM_FORMAT: u32 = 8;
    const PROPERTY_MAXIMUM_FRAMES_PER_SLICE: u32 = 14;
    const SCOPE_GLOBAL: u32 = 0;
    const SCOPE_OUTPUT: u32 = 2;
    const SAMPLE_TIME_VALID: u32 = 1;

    /// Frames pulled a call: the unit's default most, and what it is told.
    const SLICE: usize = 512;

    /// The unit, disposed of however the render ends.
    struct Unit(AudioUnit);

    impl Drop for Unit {
        fn drop(&mut self) {
            // SAFETY: `self.0` came from `AudioComponentInstanceNew` and is
            // disposed of once, here; uninitializing one that never was is
            // an error status, not undefined behaviour.
            unsafe {
                AudioUnitUninitialize(self.0);
                AudioComponentInstanceDispose(self.0);
            }
        }
    }

    pub(super) fn render(events: &[Event], frames: usize, rate: u32) -> Option<(Vec<f32>, Vec<f32>)> {
        let unit = open(rate)?;
        let mut left = vec![0.0f32; frames];
        let mut right = vec![0.0f32; frames];
        let mut next = 0usize;
        let mut pos = 0usize;
        while pos < frames {
            let n = SLICE.min(frames - pos);
            while next < events.len() {
                let (time, status, data1, data2) = events[next];
                let at = (time.max(0.0) * f64::from(rate)).round() as usize;
                if at >= pos + n {
                    break;
                }
                let offset = at.saturating_sub(pos) as u32;
                // SAFETY: a live, initialized unit; plain integers.
                let st = unsafe { MusicDeviceMIDIEvent(unit.0, u32::from(status), u32::from(data1), u32::from(data2), offset) };
                if st != 0 {
                    return None;
                }
                next += 1;
            }
            let mut list = StereoBufferList {
                number_buffers: 2,
                buffers: [
                    AudioBuffer { number_channels: 1, data_byte_size: (n * 4) as u32, data: left[pos..pos + n].as_mut_ptr().cast() },
                    AudioBuffer { number_channels: 1, data_byte_size: (n * 4) as u32, data: right[pos..pos + n].as_mut_ptr().cast() },
                ],
            };
            let stamp = AudioTimeStamp {
                sample_time: pos as f64,
                host_time: 0,
                rate_scalar: 0.0,
                word_clock_time: 0,
                smpte_time: SmpteTime { subframes: 0, subframe_divisor: 0, counter: 0, kind: 0, flags: 0, hours: 0, minutes: 0, seconds: 0, frames: 0 },
                flags: SAMPLE_TIME_VALID,
                reserved: 0,
            };
            let mut flags = 0u32;
            // SAFETY: the two buffers point at `n` floats each of `left` and
            // `right`, which outlive the call and are not otherwise borrowed;
            // the unit renders at most `n` frames, the slice size it was
            // told it may be asked for.
            let st = unsafe { AudioUnitRender(unit.0, &mut flags, &stamp, 0, n as u32, &mut list) };
            if st != 0 {
                return None;
            }
            pos += n;
        }
        let heard = left.iter().chain(right.iter()).any(|v| v.abs() > 1.0e-4);
        heard.then_some((left, right))
    }

    /// Apple's DLS synth, set to render `rate` stereo floats, a channel a
    /// buffer, and initialized.
    fn open(rate: u32) -> Option<Unit> {
        let desc = AudioComponentDescription { component_type: MUSIC_DEVICE, component_sub_type: DLS_SYNTH, component_manufacturer: APPLE, component_flags: 0, component_flags_mask: 0 };
        // SAFETY: a null component asks for the first match; `desc` lives
        // across the call.
        let component = unsafe { AudioComponentFindNext(ptr::null_mut(), &desc) };
        if component.is_null() {
            return None;
        }
        let mut raw: AudioUnit = ptr::null_mut();
        // SAFETY: `component` is a component the system returned; `raw` is
        // written only on success.
        if unsafe { AudioComponentInstanceNew(component, &mut raw) } != 0 || raw.is_null() {
            return None;
        }
        let unit = Unit(raw);
        let format = AudioStreamBasicDescription {
            sample_rate: f64::from(rate),
            format_id: LINEAR_PCM,
            format_flags: FLOAT_PACKED_NON_INTERLEAVED,
            bytes_per_packet: 4,
            frames_per_packet: 1,
            bytes_per_frame: 4,
            channels_per_frame: 2,
            bits_per_channel: 32,
            reserved: 0,
        };
        let slice = SLICE as u32;
        // SAFETY: a live unit; each property is given a pointer to a value
        // of the type and size it takes, which outlives the call.
        unsafe {
            if AudioUnitSetProperty(unit.0, PROPERTY_STREAM_FORMAT, SCOPE_OUTPUT, 0, (&format as *const AudioStreamBasicDescription).cast(), core::mem::size_of::<AudioStreamBasicDescription>() as u32)
                != 0
            {
                return None;
            }
            if AudioUnitSetProperty(unit.0, PROPERTY_MAXIMUM_FRAMES_PER_SLICE, SCOPE_GLOBAL, 0, (&slice as *const u32).cast(), 4) != 0 {
                return None;
            }
            if AudioUnitInitialize(unit.0) != 0 {
                return None;
            }
        }
        Some(unit)
    }
}

#[cfg(all(test, target_os = "macos"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::arithmetic_side_effects)]
mod tests {
    use super::*;

    const RATE: u32 = 44_100;

    /// A program change on channel 1, then middle C held half a second.
    fn note(program: u8) -> Vec<Event> {
        vec![(0.0, 0xC0, program, 0), (0.0, 0x90, 60, 100), (0.5, 0x80, 60, 0)]
    }

    fn energy(samples: &[f32]) -> f64 {
        samples.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>()
    }

    #[test]
    fn the_system_synth_sounds_a_note() {
        let frames = RATE as usize;
        let (left, right) = render(&note(0), frames, RATE).expect("Apple's DLS synth renders");
        assert_eq!((left.len(), right.len()), (frames, frames));
        assert!(energy(&left) > 0.01, "a piano's middle C is heard");
        let tail = &left[(RATE as usize * 9 / 10)..];
        assert!(energy(tail) < energy(&left[..RATE as usize / 2]), "and dies away after its note off");
    }

    #[test]
    fn a_flute_is_not_a_piano() {
        let frames = RATE as usize;
        let (piano, _) = render(&note(0), frames, RATE).expect("a piano");
        let (flute, _) = render(&note(73), frames, RATE).expect("a flute");
        let differ: f64 = piano.iter().zip(&flute).map(|(a, b)| f64::from(a - b).abs()).sum();
        assert!(differ > 1.0, "two General MIDI programs, two different sounds");
    }

    #[test]
    fn nothing_to_play_is_none() {
        assert!(render(&[], RATE as usize / 4, RATE).is_none(), "a silent render answers None, so the caller falls back");
    }
}
