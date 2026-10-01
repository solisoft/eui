//! A session over a pipe (spec 01 §7).
//!
//! An application on the person's own machine starts `eui --pipe` and speaks
//! to it over the client's standard input and output: the same frames as a
//! socket, one after another with nothing between them, and the assets in
//! the session as `Fetch` and `Asset` because there is no origin to `GET`
//! them from.
//!
//! What this builds is a [`Connection`] like any other — two channels, and
//! a [`crate::transport::Fetcher`] for the pictures — so the window treats a
//! pipe session as it treats a socket one, and the worker still decodes
//! every frame (08 §10). The window's side of the pipe only *cuts*: it reads
//! a frame's kind and length to know where it ends, and an `Asset` frame's
//! header to know whose bytes these are, exactly as it reads an HTTPS
//! response's to fetch an asset over a socket. No payload is interpreted
//! here beyond that.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use eui_proto::{Chunked, Frame};

use crate::transport::{Connection, Fetcher, Incoming, TransportError, BACKLOG_LOW};

pub use crate::dial::{is_pipe, PIPE_URL as URL};

/// Read one whole frame off `input`: the kind byte, the length varint, and
/// exactly that many payload bytes (01 §7.2).
///
/// `Ok(None)` is the end of the pipe *between* frames, which is how a
/// session ends cleanly. The end of the pipe anywhere inside a frame is a
/// truncated frame. The declared length is checked against
/// `MAX_FRAME_BYTES` **before** anything is reserved for it: on a pipe
/// nothing else bounds what a length asks for.
pub fn read_frame(input: &mut impl Read) -> Result<Option<Vec<u8>>, String> {
    let mut head = Vec::with_capacity(11);
    let mut byte = [0u8; 1];
    // The kind. Zero bytes here is the clean end.
    match read_byte(input, &mut byte)? {
        false => return Ok(None),
        true => head.push(byte[0]),
    }
    // The length: a varint of at most ten bytes, the last without its high
    // bit. Read a byte at a time so nothing past the header is consumed.
    loop {
        if !read_byte(input, &mut byte)? {
            return Err("the pipe ended in the middle of a frame".into());
        }
        head.push(byte[0]);
        if byte[0] & 0x80 == 0 {
            break;
        }
        if head.len() >= 11 {
            return Err("a frame's length is not a varint".into());
        }
    }
    // Where the frame ends, by the same function a run of frames from
    // `/_eui/view` is walked with — and it is the one that refuses a length
    // past the ceiling, before a byte of room is asked for.
    let total = Frame::framed_len(&head).map_err(|e| format!("a frame's header was refused: {e}"))?;
    let mut frame = head;
    let have = frame.len();
    frame.resize(total, 0);
    if let Some(rest) = frame.get_mut(have..) {
        input.read_exact(rest).map_err(|e| if e.kind() == std::io::ErrorKind::UnexpectedEof { "the pipe ended in the middle of a frame".to_owned() } else { e.to_string() })?;
    }
    Ok(Some(frame))
}

fn read_byte(input: &mut impl Read, byte: &mut [u8; 1]) -> Result<bool, String> {
    loop {
        match input.read(byte) {
            Ok(0) => return Ok(false),
            Ok(_) => return Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// One asset on its way in: what it may cost and what has arrived of it.
struct InFlight {
    cap: usize,
    next: u32,
    got: Vec<u8>,
}

/// The assets a pipe session has asked for and not yet been given
/// (01 §7.3): the asking half, called from the window, and the arriving
/// half, called from the pipe's reader.
pub struct Assets {
    out: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    /// The connection's count of bytes not yet written, which the writer
    /// takes every frame it writes out of — a `Fetch` included.
    backlog: Arc<AtomicUsize>,
    wanted: Mutex<HashMap<[u8; 32], InFlight>>,
}

impl Assets {
    /// Ask for `hash`, at most `cap` bytes of it — unless it is already on
    /// its way, which a client MUST NOT ask for twice.
    pub fn ask(&self, hash: [u8; 32], cap: usize) {
        let Ok(mut wanted) = self.wanted.lock() else { return };
        if wanted.contains_key(&hash) {
            return;
        }
        wanted.insert(hash, InFlight { cap, next: 0, got: Vec::new() });
        drop(wanted);
        let bytes = Frame::Fetch { hash, cap: cap as u64 }.encode();
        let n = bytes.len();
        self.backlog.fetch_add(n, Ordering::Relaxed);
        if self.out.send(bytes).is_err() {
            self.backlog.fetch_sub(n, Ordering::Relaxed);
        }
    }

    /// An `Asset` frame read off the pipe. `Ok(Some)` when an asset is
    /// whole — verified against its name — or refused by the server;
    /// `Ok(None)` while more is owed; `Err` for what ends the session: an
    /// asset nobody asked for, a gap, bytes past the cap.
    fn arrived(&self, frame: &[u8]) -> Result<Option<Incoming>, String> {
        let Ok(Frame::Asset(chunk)) = Frame::decode_pipe(frame) else {
            return Err("an Asset frame that does not decode".into());
        };
        let mut wanted = self.wanted.lock().map_err(|_| "the asset table is poisoned".to_owned())?;
        let Some(entry) = wanted.get_mut(&chunk.hash) else {
            return Err(format!("an asset nobody asked for arrived ({})", short(&chunk.hash)));
        };
        if chunk.seq != entry.next {
            return Err(format!("asset {} skipped from chunk {} to {}", short(&chunk.hash), entry.next, chunk.seq));
        }
        match chunk.flag {
            Chunked::Abort => {
                wanted.remove(&chunk.hash);
                Ok(Some(Incoming::Asset(chunk.hash, Err(String::from_utf8_lossy(&chunk.bytes).into_owned()))))
            }
            Chunked::More | Chunked::Last => {
                if entry.got.len().saturating_add(chunk.bytes.len()) > entry.cap {
                    return Err(format!("asset {} passed the {} bytes it was asked for", short(&chunk.hash), entry.cap));
                }
                entry.got.extend_from_slice(&chunk.bytes);
                entry.next = entry.next.saturating_add(1);
                if matches!(chunk.flag, Chunked::More) {
                    return Ok(None);
                }
                let Some(whole) = wanted.remove(&chunk.hash) else { return Ok(None) };
                // The name is the content, on a pipe as over HTTPS: the
                // pipe is not hostile, but a cache that held one entry
                // nobody checked would promise nothing about the rest.
                if *blake3::hash(&whole.got).as_bytes() == chunk.hash {
                    Ok(Some(Incoming::Asset(chunk.hash, Ok(whole.got))))
                } else {
                    Ok(Some(Incoming::Asset(chunk.hash, Err(format!("asset {} did not hash to its name; discarded", short(&chunk.hash))))))
                }
            }
        }
    }
}

fn short(hash: &[u8; 32]) -> String {
    crate::assets::hex(hash).get(..8).unwrap_or("").to_owned()
}

/// A session over `input` and `output`, with `first` — the `Hello` — written
/// before anything else (01 §7.1).
///
/// Two threads, so that neither direction ever waits on the other: the
/// reader cuts frames and hands them on, the writer drains what the window
/// sends. A pipe holds a few tens of kilobytes, and two processes each
/// blocked writing into a full one is the way a pipe protocol stops without
/// an error (01 §7.2).
pub fn connect<R, W>(input: R, output: W, first: Vec<u8>, notify: impl Fn() + Send + Sync + 'static) -> Result<Connection, TransportError>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let (in_tx, in_rx) = mpsc::channel::<Incoming>();
    let notify: Arc<dyn Fn() + Send + Sync> = Arc::new(notify);
    let backlog = Arc::new(AtomicUsize::new(0));
    let assets = Arc::new(Assets { out: out_tx.clone(), backlog: Arc::clone(&backlog), wanted: Mutex::new(HashMap::new()) });

    let written = Arc::clone(&backlog);
    let wake_writer = Arc::clone(&notify);
    thread::Builder::new()
        .name("eui-pipe-out".into())
        .spawn(move || {
            let mut output = output;
            if output.write_all(&first).and_then(|()| output.flush()).is_err() {
                return;
            }
            // `blocking_recv` sleeps until there is something to write, so
            // an idle session costs this thread nothing.
            while let Some(bytes) = out_rx.blocking_recv() {
                let n = bytes.len();
                if output.write_all(&bytes).and_then(|()| output.flush()).is_err() {
                    // The application is gone; the reader is about to see
                    // the end of its pipe and say so.
                    return;
                }
                let before = written.fetch_sub(n, Ordering::Relaxed);
                if before >= BACKLOG_LOW && before.saturating_sub(n) < BACKLOG_LOW {
                    wake_writer();
                }
            }
            // Every sender is gone: the window closed. Dropping `output`
            // here is the end of the application's input (01 §7.4).
        })
        .map_err(|e| TransportError::Connect(e.to_string()))?;

    let reader_assets = Arc::clone(&assets);
    let reader_tx = in_tx.clone();
    let wake_reader = Arc::clone(&notify);
    thread::Builder::new()
        .name("eui-pipe-in".into())
        .spawn(move || {
            let mut input = input;
            loop {
                let event = match read_frame(&mut input) {
                    Ok(None) => Incoming::Closed(TransportError::Closed),
                    Err(why) => Incoming::Closed(TransportError::Pipe(why)),
                    // An asset is the transport's to assemble, as an HTTPS
                    // body is; everything else goes to the driver whole.
                    Ok(Some(frame)) if frame.first() == Some(&0x0E) => match reader_assets.arrived(&frame) {
                        Ok(Some(done)) => done,
                        Ok(None) => continue,
                        Err(why) => Incoming::Closed(TransportError::Pipe(why)),
                    },
                    Ok(Some(frame)) => Incoming::Message(frame),
                };
                let fatal = matches!(event, Incoming::Closed(_));
                if reader_tx.send(event).is_err() {
                    return;
                }
                wake_reader();
                if fatal {
                    return;
                }
            }
        })
        .map_err(|e| TransportError::Connect(e.to_string()))?;

    let fetch = Fetcher::piped(assets, in_tx, notify);
    Ok(Connection::assembled(out_tx, in_rx, backlog, fetch))
}

/// The process's own standard input and output, once (01 §7.1): a second
/// session in this process has no second pipe to speak on.
pub fn stdio(first: Vec<u8>, notify: impl Fn() + Send + Sync + 'static) -> Result<Connection, TransportError> {
    static TAKEN: AtomicBool = AtomicBool::new(false);
    if TAKEN.swap(true, Ordering::SeqCst) {
        return Err(TransportError::Pipe("standard input and output already carry this process's session".into()));
    }
    connect(std::io::stdin(), std::io::stdout(), first, notify)
}

/// What a window does when its pipe ends (01 §7.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ending {
    /// Show this and stay open: the application said why it stopped, or
    /// the pipe broke a rule on the way.
    Show(String),
    /// The application finished: close the window and exit with status 0.
    Finished,
}

/// Decide [`Ending`] from why the pipe closed and whether the session had
/// already been ended by the application's own `Error` (`closed`, the
/// driver's reason).
#[must_use]
pub fn ending(why: &TransportError, closed: Option<String>) -> Ending {
    match (why, closed) {
        (TransportError::Pipe(broke), _) => Ending::Show(format!("the pipe: {broke}")),
        (_, Some(reason)) => Ending::Show(reason),
        (_, None) => Ending::Finished,
    }
}
