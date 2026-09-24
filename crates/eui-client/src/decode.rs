//! Decoding, off the thread that paints.
//!
//! A picture, a sound or a moving picture used to be decoded where it was
//! first needed: an image in `asset_ready`, a sound and a GIF in the paint
//! that first synced them. On a desktop that is the window's own thread,
//! holding the driver's lock — the one the audio callback takes to fill the
//! device — so a large JPEG, a long MP3 or a GIF froze the window for tens
//! of milliseconds to seconds, and a sound already playing ran dry while it
//! did. Behind the worker the window blocked on the pipe for the same time.
//!
//! Now the bytes are handed to a small pool of threads and the driver goes
//! on. What a decode produces comes back on a channel the driver drains at
//! its next tick or paint, and the nodes that name it are marked for layout
//! then — a picture arriving a frame later than it would have, rather than
//! the frame it arrives in taking as long as the decode.
//!
//! The pool is the process's, not a driver's: two tabs decode on the same
//! two threads. In the sandboxed worker it has to exist before the door
//! closes, because seccomp kills a process that creates a thread (08 §10);
//! `Driver::new` starts it, and the worker builds a throwaway driver before
//! it locks itself down for exactly this kind of reason. Where no thread can
//! be started at all — a page, which has one — the work runs where it is
//! handed over, as it did before.

use std::collections::HashSet;
use std::sync::{mpsc, Arc, OnceLock};

use crate::assets::{AssetError, Decoded, Hash};

/// Threads decoding for the whole process. Two: one long decode — a sound,
/// a GIF — does not hold up the pictures behind it, and a page of
/// thumbnails does not take every core from the window.
pub(crate) const DECODE_THREADS: usize = 2;

/// How often a driver with decodes in flight asks to be woken to collect
/// them. Only while something is in flight: at rest nothing is scheduled.
pub(crate) const DECODE_POLL: std::time::Duration = std::time::Duration::from_millis(8);

type Task = Box<dyn FnOnce() + Send>;

/// The process's decode threads: `None` when none could be started.
fn pool() -> Option<&'static mpsc::Sender<Task>> {
    static POOL: OnceLock<Option<mpsc::Sender<Task>>> = OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Task>();
        let rx = Arc::new(std::sync::Mutex::new(rx));
        let (ready_tx, ready) = mpsc::channel::<()>();
        let mut started = 0;
        for i in 0..DECODE_THREADS {
            let rx = Arc::clone(&rx);
            let ready_tx = ready_tx.clone();
            let spawned = std::thread::Builder::new().name(format!("eui-decode-{i}")).spawn(move || {
                // Everything the runtime does to start a thread — naming it
                // (`prctl(PR_SET_NAME)`), its signal stack — is done by the
                // time this line runs. `spawn` returning says only that the
                // thread exists.
                let _ = ready_tx.send(());
                drop(ready_tx);
                loop {
                    // The lock is held while waiting, so one idle thread
                    // waits on the queue and the other on the lock; a task
                    // goes to whichever holds it.
                    let task = match rx.lock() {
                        Ok(q) => q.recv(),
                        Err(_) => return,
                    };
                    match task {
                        Ok(task) => task(),
                        Err(_) => return,
                    }
                }
            });
            if spawned.is_ok() {
                started += 1;
            }
        }
        // Wait for every thread that was started to have started. In the
        // worker the lock-down follows: a thread still naming itself when
        // the filter lands is killed on that `prctl`, and the worker with
        // it — a race lost a few times in a hundred under load, and the
        // reason the worker tests failed in CI (08 §10).
        drop(ready_tx);
        for _ in 0..started {
            if ready.recv().is_err() {
                break;
            }
        }
        (started > 0).then_some(tx)
    })
    .as_ref()
}

/// Start the process's decode threads, if they are not running yet, and
/// return once each is running its own loop. The worker calls this —
/// through `Driver::new` — before it confines itself.
pub(crate) fn start() {
    let _ = pool();
}

/// What to make of the bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Work {
    /// A picture, shrunk to what the sheet takes.
    Image,
    /// A sound, under what is left of the session's room for sounds.
    Sound { max: usize },
    /// A moving picture, under what is left of the room for pictures.
    Movie { max: usize },
}

impl Work {
    /// The kind alone: one hash may be decoded as a picture and as a moving
    /// picture both (a WebP), but not twice as either.
    fn kind(self) -> u8 {
        match self {
            Self::Image => 0,
            Self::Sound { .. } => 1,
            Self::Movie { .. } => 2,
        }
    }
}

/// What a decode made.
#[derive(Debug)]
pub(crate) enum Done {
    Image(Result<Decoded, AssetError>),
    Sound(Result<eui_audio::Sound, String>),
    Movie(Result<eui_video::Movie, String>),
}

fn run(bytes: &[u8], work: Work) -> Done {
    // A decoder that panics on bytes a server chose must not take a pool
    // thread with it: the result would never come back, and the hash would
    // be "decoding" for the rest of the session. (A release build aborts on
    // a panic, which ends the worker and with it the session — the same
    // outcome a panic on the old path had. This is for the builds that
    // unwind: tests, and `release-checked`.)
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match work {
        Work::Image => Done::Image(crate::assets::prepare_image(bytes)),
        Work::Sound { max } => Done::Sound(eui_audio::decode_within(bytes, None, max).map_err(|e| e.to_string())),
        Work::Movie { max } => Done::Movie(eui_video::decode_within(bytes, None, max).map_err(|e| e.to_string())),
    }));
    caught.unwrap_or_else(|_| match work {
        Work::Image => Done::Image(Err(AssetError::Decode("the decoder panicked".into()))),
        Work::Sound { .. } => Done::Sound(Err("the decoder panicked".into())),
        Work::Movie { .. } => Done::Movie(Err("the decoder panicked".into())),
    })
}

/// One driver's decodes: what it handed over and what has come back.
#[derive(Debug)]
pub(crate) struct Decoder {
    tx: mpsc::Sender<(Hash, u8, Done)>,
    rx: mpsc::Receiver<(Hash, u8, Done)>,
    in_flight: HashSet<(Hash, u8)>,
}

impl Default for Decoder {
    fn default() -> Self {
        start();
        let (tx, rx) = mpsc::channel();
        Self { tx, rx, in_flight: HashSet::new() }
    }
}

impl Decoder {
    /// True while a decode handed over has not been collected.
    pub(crate) fn busy(&self) -> bool {
        !self.in_flight.is_empty()
    }

    /// True while `hash` is being decoded as `work`'s kind.
    pub(crate) fn decoding(&self, hash: &Hash, work: Work) -> bool {
        self.in_flight.contains(&(*hash, work.kind()))
    }

    /// Hand `bytes` over to be decoded. Nothing happens if the same hash is
    /// already being decoded the same way.
    pub(crate) fn submit(&mut self, hash: Hash, bytes: Arc<Vec<u8>>, work: Work) {
        let kind = work.kind();
        if !self.in_flight.insert((hash, kind)) {
            return;
        }
        let tx = self.tx.clone();
        let task: Task = Box::new(move || {
            let _ = tx.send((hash, kind, run(&bytes, work)));
        });
        match pool() {
            Some(pool) => {
                if let Err(mpsc::SendError(task)) = pool.send(task) {
                    task();
                }
            }
            None => task(),
        }
    }

    /// Whatever has finished since the last call. Never waits.
    pub(crate) fn take(&mut self) -> Vec<(Hash, Done)> {
        let mut out = Vec::new();
        while let Ok((hash, kind, done)) = self.rx.try_recv() {
            self.in_flight.remove(&(hash, kind));
            out.push((hash, done));
        }
        out
    }

    /// Everything in flight, waiting up to `limit` for it.
    pub(crate) fn wait(&mut self, limit: std::time::Duration) -> Vec<(Hash, Done)> {
        let deadline = crate::time::Instant::now() + limit;
        let mut out = self.take();
        while self.busy() {
            let left = deadline.saturating_duration_since(crate::time::Instant::now());
            match self.rx.recv_timeout(left) {
                Ok((hash, kind, done)) => {
                    self.in_flight.remove(&(hash, kind));
                    out.push((hash, done));
                }
                Err(_) => break,
            }
        }
        out
    }
}
