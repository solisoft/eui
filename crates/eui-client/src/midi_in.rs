//! The person's MIDI instruments, read on the window's side (06 §1.4).
//!
//! Opened while the active tab wants them — the `midi` capability granted,
//! the window in front, a node holding a `midi` handler — and let go of the
//! moment that stops being true: dropping a [`MidiIn`] closes every port it
//! opened. Every input the platform lists is opened, and the list is read
//! again every [`RESCAN`], so a keyboard plugged in after the page asked is
//! heard without a reload.
//!
//! Only channel messages are kept, `0x80..=0xEF`: what a person played.
//! Clock and active sensing arrive dozens of times a second and say nothing,
//! and a SysEx can carry a device's serial number, which identifies the
//! person rather than the music. They are dropped here, before anything
//! leaves this process.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eui_proto::limits::MAX_MIDI_QUEUE;

/// How often the ports are listed again while the instruments are wanted.
pub const RESCAN: Duration = Duration::from_secs(2);

type Queue = Arc<Mutex<VecDeque<(Instant, [u8; 3])>>>;

/// The open inputs, and what they have played that the driver has not had.
pub struct MidiIn {
    open: Vec<(String, midir::MidiInputConnection<()>)>,
    queue: Queue,
    wake: Arc<dyn Fn() + Send + Sync>,
    scanned: Option<Instant>,
}

impl std::fmt::Debug for MidiIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MidiIn").field("open", &self.open.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()).finish_non_exhaustive()
    }
}

impl MidiIn {
    /// Nothing open yet; `wake` is called from the platform's thread each
    /// time a message arrives, so the window's loop comes round to take it.
    pub fn new(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self { open: Vec::new(), queue: Arc::new(Mutex::new(VecDeque::new())), wake, scanned: None }
    }

    /// The names of the inputs open now.
    pub fn ports(&self) -> Vec<&str> {
        self.open.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// Open the inputs that are not open yet and let go of the ones that
    /// went, at most once every [`RESCAN`].
    pub fn scan(&mut self, now: Instant) {
        if self.scanned.is_some_and(|at| now.saturating_duration_since(at) < RESCAN) {
            return;
        }
        self.scanned = Some(now);
        let Ok(probe) = midir::MidiInput::new("eui") else { return };
        let names: Vec<String> = probe.ports().iter().filter_map(|p| probe.port_name(p).ok()).collect();
        // Dropping a connection closes it.
        self.open.retain(|(n, _)| names.contains(n));
        for name in names {
            if self.open.iter().any(|(n, _)| *n == name) {
                continue;
            }
            // One `MidiInput` per connection — `connect` consumes it — and
            // the port found again on it, since a port belongs to the
            // client that listed it.
            let Ok(input) = midir::MidiInput::new("eui") else { continue };
            let Some(port) = input.ports().into_iter().find(|p| input.port_name(p).is_ok_and(|n| n == name)) else { continue };
            let queue = Arc::clone(&self.queue);
            let wake = Arc::clone(&self.wake);
            let opened = input.connect(
                &port,
                "eui-in",
                move |_stamp, bytes, _| {
                    let Some(message) = channel_message(bytes) else { return };
                    if let Ok(mut q) = queue.lock() {
                        if q.len() >= MAX_MIDI_QUEUE {
                            q.pop_front();
                            eprintln!("eui: MIDI: {MAX_MIDI_QUEUE} messages waiting; the oldest goes");
                        }
                        q.push_back((Instant::now(), message));
                    }
                    wake();
                },
                (),
            );
            match opened {
                Ok(conn) => {
                    eprintln!("eui: MIDI input open: {name}");
                    self.open.push((name, conn));
                }
                Err(e) => eprintln!("eui: MIDI input {name}: {e}"),
            }
        }
    }

    /// What arrived since the last call, as the driver takes it: how long
    /// before `now` each message arrived, then the message.
    pub fn take(&mut self, now: Instant) -> Vec<[u32; 4]> {
        let Ok(mut q) = self.queue.lock() else { return Vec::new() };
        q.drain(..)
            .map(|(at, m)| {
                let age = u32::try_from(now.saturating_duration_since(at).as_millis()).unwrap_or(u32::MAX);
                [age, u32::from(m[0]), u32::from(m[1]), u32::from(m[2])]
            })
            .collect()
    }
}

/// A channel message as three bytes, `0` for the data byte a two-byte one
/// does not have; `None` for anything else — system messages, a stray data
/// byte, an empty read.
pub fn channel_message(bytes: &[u8]) -> Option<[u8; 3]> {
    let status = *bytes.first()?;
    if !(0x80..=0xEF).contains(&status) {
        return None;
    }
    let d1 = bytes.get(1).copied()? & 0x7F;
    let d2 = if matches!(status & 0xF0, 0xC0 | 0xD0) { 0 } else { bytes.get(2).copied().unwrap_or(0) & 0x7F };
    Some([status, d1, d2])
}

#[cfg(test)]
mod tests {
    use super::channel_message;

    #[test]
    fn only_what_was_played_is_kept() {
        assert_eq!(channel_message(&[0x90, 60, 100]), Some([0x90, 60, 100]), "a note on");
        assert_eq!(channel_message(&[0xC3, 40]), Some([0xC3, 40, 0]), "a program change has one data byte");
        assert_eq!(channel_message(&[0xF8]), None, "clock");
        assert_eq!(channel_message(&[0xFE]), None, "active sensing");
        assert_eq!(channel_message(&[0xF0, 0x7E, 0x00, 0x06, 0x02, 0xF7]), None, "a SysEx identity reply");
        assert_eq!(channel_message(&[0x90]), None, "a status with no data");
        assert_eq!(channel_message(&[]), None);
    }
}
