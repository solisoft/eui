//! Spec 08 §10: the process boundary.
//!
//! Everything that reads bytes a server chose runs in a **worker** process:
//! the frame decoder, the tree, layout, text shaping, PNG decoding, the
//! bytecode VM — the whole [`Driver`]. The **window** process keeps what
//! needs the platform: winit, the GPU, TLS, the pin store, the clipboard,
//! the accessibility adapter. Between them, two pipes carry a small
//! request/reply protocol: the window forwards raw frames and inputs, the
//! worker answers with outbound frames and, on request, a draw list and
//! the atlas bitmaps behind it. The window never decodes a frame.
//!
//! On Linux the worker confines itself with [`crate::sandbox`] before it
//! reads a byte. Elsewhere it is still its own process: a crash or a
//! runaway allocation in the decoder takes the worker, not the window,
//! which reports the session ended and stays standing.
//!
//! [`Backend`] is what the window talks to: the same calls whether the
//! driver is in this process (`EUI_SANDBOX=0`, or when a worker cannot be
//! started) or across the boundary. The wire between the two is private
//! to this crate and unversioned: both ends are always the same binary.

use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eui_proto::{Frame, ThemeMode};
use eui_render::{Atlas, Backdrop, DrawList, ImageAtlas, Quad, Run, Scroller};

use crate::a11y::{AccessNode, AccessRole, AccessSnapshot, AccessState, Checked};
use crate::assets::Hash;
use crate::driver::{Driver, FileAsk, FileWant, FileWrite, Input};

/// The argument that turns a binary into a worker.
pub const WORKER_ARG: &str = "--eui-worker";
/// The argument that runs one sandbox self-test and exits.
pub const SELFTEST_ARG: &str = "--eui-worker-selftest";

/// A message a reply can be at most: a draw list of the largest tree plus
/// both atlases, with room. The window enforces it on what the worker
/// says; the worker trusts the window.
const MAX_REPLY: usize = 64 << 20;

// ----------------------------------------------------------------- wire

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn bool(&mut self, v: bool) {
        self.0.push(u8::from(v));
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f4(&mut self, v: [f32; 4]) {
        for x in v {
            self.f32(x);
        }
    }
    fn bytes(&mut self, b: &[u8]) {
        self.u32(u32::try_from(b.len()).unwrap_or(u32::MAX));
        self.0.extend_from_slice(b);
    }
    fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
    fn hash(&mut self, h: &Hash) {
        self.0.extend_from_slice(h);
    }
    fn opt_str(&mut self, s: Option<&str>) {
        match s {
            Some(s) => {
                self.bool(true);
                self.str(s);
            }
            None => self.bool(false),
        }
    }
}

struct R<'a> {
    b: &'a [u8],
    i: usize,
}

type Wire<T> = Result<T, &'static str>;

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Wire<&'a [u8]> {
        let end = self.i.checked_add(n).ok_or("length overflow")?;
        let s = self.b.get(self.i..end).ok_or("truncated")?;
        self.i = end;
        Ok(s)
    }
    fn u8(&mut self) -> Wire<u8> {
        Ok(*self.take(1)?.first().ok_or("truncated")?)
    }
    fn bool(&mut self) -> Wire<bool> {
        Ok(self.u8()? != 0)
    }
    fn u32(&mut self) -> Wire<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes(b.try_into().map_err(|_| "u32")?))
    }
    fn u64(&mut self) -> Wire<u64> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes(b.try_into().map_err(|_| "u64")?))
    }
    fn f32(&mut self) -> Wire<f32> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes(b.try_into().map_err(|_| "f32")?))
    }
    fn f4(&mut self) -> Wire<[f32; 4]> {
        Ok([self.f32()?, self.f32()?, self.f32()?, self.f32()?])
    }
    fn bytes(&mut self) -> Wire<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
    fn str(&mut self) -> Wire<String> {
        String::from_utf8(self.bytes()?.to_vec()).map_err(|_| "utf-8")
    }
    fn hash(&mut self) -> Wire<Hash> {
        self.take(32)?.try_into().map_err(|_| "hash")
    }
    fn opt_str(&mut self) -> Wire<Option<String>> {
        Ok(if self.bool()? { Some(self.str()?) } else { None })
    }
    fn done(&self) -> Wire<()> {
        if self.i == self.b.len() {
            Ok(())
        } else {
            Err("trailing bytes")
        }
    }
}

/// What the window asks the worker.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    /// Create the driver for a window of this size and scale, granting
    /// these capabilities. First, and once.
    Config {
        /// Logical width.
        w: f32,
        /// Logical height.
        h: f32,
        /// Device px per logical px.
        scale: f32,
        /// Capability bits.
        granted: u32,
    },
    /// Change the grant before the session opens.
    Grant(u32),
    /// The opening frame, encoded.
    Hello,
    /// A frame from the server, as received.
    Frame(Vec<u8>),
    /// Something the viewer did.
    Input(Input),
    /// Bytes for a hash the worker asked for, verified by the window.
    AssetReady(Hash, Vec<u8>),
    /// A hash the window could not fetch.
    AssetFailed(Hash, String),
    /// Hashes the tree needs and nobody fetched yet.
    PendingAssets,
    /// Lay out and paint for a target this many device pixels.
    Paint(u32, u32),
    /// The clock moved: is a transition frame due?
    Tick,
    /// The accessibility tree as painted.
    AccessTree,
    /// An assistive technology's action on a node: `true` click, `false`
    /// focus.
    AccessAction(u64, bool),
    /// The viewer's desktop palette (05 §5): its mode if it has one, and
    /// colours by role id. Empty means none: the theme's own colours.
    DesktopTheme(Option<ThemeMode>, Vec<(u16, u32)>),
    /// The person chose files in the dialog `token` opened: name and size
    /// each. The bytes follow as [`Request::UploadChunk`].
    Picked {
        /// The [`FileAsk::token`] the dialog was opened for.
        token: u32,
        /// What they chose, in the order the dialog gave them.
        files: Vec<(String, u64)>,
    },
    /// A dialog was dismissed without choosing.
    Dismissed(u32),
    /// Bytes of an upload, in order.
    UploadChunk {
        /// The upload id the driver minted.
        id: u32,
        /// The next bytes of the file.
        bytes: Vec<u8>,
        /// Whether the file ends here.
        last: bool,
    },
    /// The window could not read what was picked.
    UploadFailed(u32, String),
    /// The person chose where a save should go, and what it is called.
    Saving {
        /// The [`FileAsk::token`] the dialog was opened for.
        token: u32,
        /// The name they gave it.
        name: String,
    },
    /// Spec 03 §7: mix `frames` frames of `channels` samples at `rate`.
    /// The window's audio thread asks; nothing else does.
    Audio {
        /// Frames wanted.
        frames: u32,
        /// Samples per frame, 1 or 2.
        channels: u8,
        /// Frames a second the device runs at.
        rate: u32,
    },
}

impl Request {
    fn encode(&self) -> Vec<u8> {
        let mut w = W(Vec::new());
        match self {
            Request::Config { w: width, h, scale, granted } => {
                w.u8(0);
                w.f32(*width);
                w.f32(*h);
                w.f32(*scale);
                w.u32(*granted);
            }
            Request::Grant(g) => {
                w.u8(1);
                w.u32(*g);
            }
            Request::Hello => w.u8(2),
            Request::Frame(b) => {
                w.u8(3);
                w.bytes(b);
            }
            Request::Input(i) => {
                w.u8(4);
                put_input(&mut w, i);
            }
            Request::AssetReady(h, b) => {
                w.u8(5);
                w.hash(h);
                w.bytes(b);
            }
            Request::AssetFailed(h, why) => {
                w.u8(6);
                w.hash(h);
                w.str(why);
            }
            Request::PendingAssets => w.u8(7),
            Request::Paint(pw, ph) => {
                w.u8(8);
                w.u32(*pw);
                w.u32(*ph);
            }
            Request::Tick => w.u8(9),
            Request::AccessTree => w.u8(10),
            Request::AccessAction(id, click) => {
                w.u8(11);
                w.u64(*id);
                w.bool(*click);
            }
            Request::Audio { frames, channels, rate } => {
                w.u8(13);
                w.u32(*frames);
                w.u8(*channels);
                w.u32(*rate);
            }
            Request::Picked { token, files } => {
                w.u8(14);
                w.u32(*token);
                w.u32(u32::try_from(files.len()).unwrap_or(u32::MAX));
                for (name, size) in files {
                    w.str(name);
                    w.u64(*size);
                }
            }
            Request::Dismissed(token) => {
                w.u8(15);
                w.u32(*token);
            }
            Request::UploadChunk { id, bytes, last } => {
                w.u8(16);
                w.u32(*id);
                w.bytes(bytes);
                w.bool(*last);
            }
            Request::UploadFailed(id, why) => {
                w.u8(17);
                w.u32(*id);
                w.str(why);
            }
            Request::Saving { token, name } => {
                w.u8(18);
                w.u32(*token);
                w.str(name);
            }
            Request::DesktopTheme(mode, colors) => {
                w.u8(12);
                w.u8(mode.map_or(255, |m| m as u8));
                w.u32(u32::try_from(colors.len()).unwrap_or(u32::MAX));
                for (role, rgba) in colors {
                    w.u32(u32::from(*role));
                    w.u32(*rgba);
                }
            }
        }
        w.0
    }

    fn decode(b: &[u8]) -> Wire<Self> {
        let mut r = R { b, i: 0 };
        let out = match r.u8()? {
            0 => Request::Config { w: r.f32()?, h: r.f32()?, scale: r.f32()?, granted: r.u32()? },
            1 => Request::Grant(r.u32()?),
            2 => Request::Hello,
            3 => Request::Frame(r.bytes()?.to_vec()),
            4 => Request::Input(get_input(&mut r)?),
            5 => Request::AssetReady(r.hash()?, r.bytes()?.to_vec()),
            6 => Request::AssetFailed(r.hash()?, r.str()?),
            7 => Request::PendingAssets,
            8 => Request::Paint(r.u32()?, r.u32()?),
            9 => Request::Tick,
            10 => Request::AccessTree,
            11 => Request::AccessAction(r.u64()?, r.bool()?),
            12 => {
                let mode = match r.u8()? {
                    255 => None,
                    m => Some(ThemeMode::from_u8(m).map_err(|_| "theme mode")?),
                };
                let n = r.u32()? as usize;
                let mut colors = Vec::with_capacity(n.min(64));
                for _ in 0..n {
                    colors.push((u16::try_from(r.u32()?).map_err(|_| "role")?, r.u32()?));
                }
                Request::DesktopTheme(mode, colors)
            }
            13 => Request::Audio { frames: r.u32()?, channels: r.u8()?, rate: r.u32()? },
            14 => {
                let token = r.u32()?;
                let n = r.u32()? as usize;
                let mut files = Vec::with_capacity(n.min(64));
                for _ in 0..n {
                    files.push((r.str()?, r.u64()?));
                }
                Request::Picked { token, files }
            }
            15 => Request::Dismissed(r.u32()?),
            16 => Request::UploadChunk { id: r.u32()?, bytes: r.bytes()?.to_vec(), last: r.bool()? },
            17 => Request::UploadFailed(r.u32()?, r.str()?),
            18 => Request::Saving { token: r.u32()?, name: r.str()? },
            _ => return Err("unknown request"),
        };
        r.done()?;
        Ok(out)
    }
}

fn put_input(w: &mut W, i: &Input) {
    match i {
        Input::PointerMove(x, y) => {
            w.u8(0);
            w.f32(*x);
            w.f32(*y);
        }
        Input::PointerDown(b) => {
            w.u8(1);
            w.u8(*b);
        }
        Input::PointerUp(b) => {
            w.u8(2);
            w.u8(*b);
        }
        Input::Wheel(x, y) => {
            w.u8(3);
            w.f32(*x);
            w.f32(*y);
        }
        Input::WheelStep(x, y) => {
            w.u8(4);
            w.f32(*x);
            w.f32(*y);
        }
        Input::Text(s) => {
            w.u8(5);
            w.str(s);
        }
        Input::ImePreedit(s) => {
            w.u8(6);
            w.str(s);
        }
        Input::ImeCommit(s) => {
            w.u8(7);
            w.str(s);
        }
        Input::Paste(s) => {
            w.u8(8);
            w.str(s);
        }
        Input::Key { key, modifiers, down } => {
            w.u8(9);
            w.str(key);
            w.u32(*modifiers);
            w.bool(*down);
        }
        Input::Resized(x, y, s) => {
            w.u8(10);
            w.f32(*x);
            w.f32(*y);
            w.f32(*s);
        }
        Input::Mode(m) => {
            w.u8(11);
            w.u8(*m as u8);
        }
        Input::Unfocused => w.u8(12),
        Input::PointerOut => w.u8(13),
    }
}

fn get_input(r: &mut R<'_>) -> Wire<Input> {
    Ok(match r.u8()? {
        0 => Input::PointerMove(r.f32()?, r.f32()?),
        1 => Input::PointerDown(r.u8()?),
        2 => Input::PointerUp(r.u8()?),
        3 => Input::Wheel(r.f32()?, r.f32()?),
        4 => Input::WheelStep(r.f32()?, r.f32()?),
        5 => Input::Text(r.str()?),
        6 => Input::ImePreedit(r.str()?),
        7 => Input::ImeCommit(r.str()?),
        8 => Input::Paste(r.str()?),
        9 => Input::Key { key: r.str()?, modifiers: r.u32()?, down: r.bool()? },
        10 => Input::Resized(r.f32()?, r.f32()?, r.f32()?),
        11 => Input::Mode(ThemeMode::from_u8(r.u8()?).map_err(|_| "theme mode")?),
        12 => Input::Unfocused,
        13 => Input::PointerOut,
        _ => return Err("unknown input"),
    })
}

/// What every reply carries: the driver's state the window asks about
/// between requests, so those questions never cross the pipe.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    /// Frames to send to the server, encoded.
    pub outbound: Vec<Vec<u8>>,
    /// A frame should be drawn.
    pub needs_redraw: bool,
    /// Why the session ended, if it did.
    pub closed: Option<String>,
    /// Where an input method's candidate window goes, logical px.
    pub ime: Option<[f32; 4]>,
    /// Text the viewer copied, for the system clipboard.
    pub clipboard: Option<String>,
    /// Milliseconds until the next transition frame, if one is due.
    pub next_due_ms: Option<u32>,
    /// The pointer's shape over what it is on, as [`eui_proto::Cursor`]'s
    /// wire byte.
    pub cursor: u8,
    /// A sound is loaded: the window keeps its audio device open (03 §7).
    pub audio: bool,
    /// A picture is playing (03 §8).
    pub video: bool,
    /// Dialogs the tree asked for and the window has not opened yet
    /// (spec 03 §3.2).
    pub files: Vec<FileAsk>,
    /// Bytes a save is owed, for the window to put on disk.
    pub writes: Vec<FileWrite>,
}

/// What a reply carries besides its [`Status`], by request.
#[derive(Debug, Clone, PartialEq)]
pub enum Payload {
    /// Nothing more.
    None,
    /// `Config`: what the sandbox enforced, or why it could not.
    Sandbox(Result<String, String>),
    /// `Hello`: the frame, encoded.
    Hello(Vec<u8>),
    /// `PendingAssets`.
    Assets(Vec<Hash>),
    /// `Paint`: the draw list, and the rows of each atlas that changed
    /// since the last paint — bands of `(edge length, y0, y1, bytes)` for
    /// the glyph atlas, which grows, `(y0, y1, bytes)` for the image atlas.
    Paint {
        /// The frame. Shared, so a reply drawn again is not copied again.
        list: Arc<DrawList>,
        /// Coverage rows.
        glyphs: Vec<(u32, u32, u32, Vec<u8>)>,
        /// RGBA rows.
        images: Option<(u32, u32, Vec<u8>)>,
    },
    /// `Tick`: a transition frame is due.
    Tick(bool),
    /// `AccessTree`.
    Access(AccessSnapshot),
    /// `Audio`: interleaved `f32` frames, `channels` per frame.
    Pcm(Vec<f32>),
    /// `Picked`: the upload id for each file, in the order they came.
    Uploads(Vec<u32>),
}

/// One reply.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    /// State after the request.
    pub status: Status,
    /// The request's own answer.
    pub payload: Payload,
}

impl Reply {
    fn encode(&self) -> Vec<u8> {
        let mut w = W(Vec::new());
        let s = &self.status;
        w.u32(u32::try_from(s.outbound.len()).unwrap_or(u32::MAX));
        for f in &s.outbound {
            w.bytes(f);
        }
        w.bool(s.needs_redraw);
        w.opt_str(s.closed.as_deref());
        match s.ime {
            Some(r) => {
                w.bool(true);
                w.f4(r);
            }
            None => w.bool(false),
        }
        w.opt_str(s.clipboard.as_deref());
        match s.next_due_ms {
            Some(ms) => {
                w.bool(true);
                w.u32(ms);
            }
            None => w.bool(false),
        }
        w.u8(s.cursor);
        w.bool(s.audio);
        w.bool(s.video);
        w.u32(u32::try_from(s.files.len()).unwrap_or(u32::MAX));
        for a in &s.files {
            w.u32(a.token);
            w.u32(a.node);
            match &a.want {
                FileWant::Open { accept, multiple, max } => {
                    w.u8(0);
                    w.str(accept);
                    w.bool(*multiple);
                    w.u64(*max);
                }
                FileWant::Save { name } => {
                    w.u8(1);
                    w.str(name);
                }
            }
        }
        w.u32(u32::try_from(s.writes.len()).unwrap_or(u32::MAX));
        for f in &s.writes {
            w.u32(f.token);
            w.u8(f.flag as u8);
            w.bytes(&f.bytes);
        }
        match &self.payload {
            Payload::None => w.u8(0),
            Payload::Sandbox(r) => {
                w.u8(1);
                match r {
                    Ok(s) => {
                        w.bool(true);
                        w.str(s);
                    }
                    Err(e) => {
                        w.bool(false);
                        w.str(e);
                    }
                }
            }
            Payload::Hello(b) => {
                w.u8(2);
                w.bytes(b);
            }
            Payload::Assets(hs) => {
                w.u8(3);
                w.u32(u32::try_from(hs.len()).unwrap_or(u32::MAX));
                for h in hs {
                    w.hash(h);
                }
            }
            Payload::Paint { list, glyphs, images } => {
                w.u8(4);
                put_list(&mut w, list);
                w.u32(u32::try_from(glyphs.len()).unwrap_or(u32::MAX));
                for (size, y0, y1, px) in glyphs {
                    w.u32(*size);
                    w.u32(*y0);
                    w.u32(*y1);
                    w.bytes(px);
                }
                match images {
                    Some((y0, y1, px)) => {
                        w.bool(true);
                        w.u32(*y0);
                        w.u32(*y1);
                        w.bytes(px);
                    }
                    None => w.bool(false),
                }
            }
            Payload::Tick(due) => {
                w.u8(5);
                w.bool(*due);
            }
            Payload::Access(snap) => {
                w.u8(6);
                put_access(&mut w, snap);
            }
            Payload::Uploads(ids) => {
                w.u8(8);
                w.u32(u32::try_from(ids.len()).unwrap_or(u32::MAX));
                for id in ids {
                    w.u32(*id);
                }
            }
            Payload::Pcm(samples) => {
                w.u8(7);
                w.u32(u32::try_from(samples.len()).unwrap_or(u32::MAX));
                for s in samples {
                    w.f32(*s);
                }
            }
        }
        w.0
    }

    fn decode(b: &[u8]) -> Wire<Self> {
        let mut r = R { b, i: 0 };
        let n = r.u32()? as usize;
        let mut outbound = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            outbound.push(r.bytes()?.to_vec());
        }
        let needs_redraw = r.bool()?;
        let closed = r.opt_str()?;
        let ime = if r.bool()? { Some(r.f4()?) } else { None };
        let clipboard = r.opt_str()?;
        let next_due_ms = if r.bool()? { Some(r.u32()?) } else { None };
        let cursor = r.u8()?;
        let audio = r.bool()?;
        let video = r.bool()?;
        let n = r.u32()? as usize;
        let mut files = Vec::with_capacity(n.min(64));
        for _ in 0..n {
            let (token, node) = (r.u32()?, r.u32()?);
            let want = match r.u8()? {
                0 => FileWant::Open { accept: r.str()?, multiple: r.bool()?, max: r.u64()? },
                1 => FileWant::Save { name: r.str()? },
                _ => return Err("file ask"),
            };
            files.push(FileAsk { token, node, want });
        }
        let n = r.u32()? as usize;
        let mut writes = Vec::with_capacity(n.min(1024));
        for _ in 0..n {
            let token = r.u32()?;
            let flag = eui_proto::Chunked::from_u8(r.u8()?).map_err(|_| "chunk flag")?;
            writes.push(FileWrite { token, flag, bytes: r.bytes()?.to_vec() });
        }
        let status = Status { outbound, needs_redraw, closed, ime, clipboard, next_due_ms, cursor, audio, video, files, writes };
        let payload = match r.u8()? {
            0 => Payload::None,
            1 => Payload::Sandbox(if r.bool()? { Ok(r.str()?) } else { Err(r.str()?) }),
            2 => Payload::Hello(r.bytes()?.to_vec()),
            3 => {
                let n = r.u32()? as usize;
                let mut hs = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    hs.push(r.hash()?);
                }
                Payload::Assets(hs)
            }
            4 => {
                let list = get_list(&mut r)?;
                let n = r.u32()? as usize;
                let mut glyphs = Vec::with_capacity(n.min(16));
                for _ in 0..n {
                    glyphs.push((r.u32()?, r.u32()?, r.u32()?, r.bytes()?.to_vec()));
                }
                let images = if r.bool()? { Some((r.u32()?, r.u32()?, r.bytes()?.to_vec())) } else { None };
                Payload::Paint { list: Arc::new(list), glyphs, images }
            }
            5 => Payload::Tick(r.bool()?),
            6 => Payload::Access(get_access(&mut r)?),
            8 => {
                let n = r.u32()? as usize;
                let mut ids = Vec::with_capacity(n.min(64));
                for _ in 0..n {
                    ids.push(r.u32()?);
                }
                Payload::Uploads(ids)
            }
            7 => {
                let n = r.u32()? as usize;
                let mut samples = Vec::with_capacity(n.min(1 << 20));
                for _ in 0..n {
                    samples.push(r.f32()?);
                }
                Payload::Pcm(samples)
            }
            _ => return Err("unknown payload"),
        };
        r.done()?;
        Ok(Reply { status, payload })
    }
}

/// A quad on the pipe: a byte naming which of its eight parts are not
/// all zero, then those parts. A glyph is a rect, its params, a fill and
/// its uvs -- 65 bytes rather than 128; a plain box, 49. The zero parts
/// are exactly the ones the renderer ignores for that quad.
fn put_quad(w: &mut W, q: &Quad) {
    let parts = [q.rect, q.params, q.fill, q.stroke, q.uv, q.extra, q.spin];
    let mut mask = 0u8;
    for (i, part) in parts.iter().enumerate() {
        if *part != [0.0; 4] {
            mask |= 1 << i;
        }
    }
    if q.from != [0; 8] {
        mask |= 1 << 7;
    }
    w.u8(mask);
    for (i, part) in parts.iter().enumerate() {
        if mask & (1 << i) != 0 {
            w.f4(*part);
        }
    }
    if mask & (1 << 7) != 0 {
        for pair in q.from.chunks_exact(2) {
            if let [lo, hi] = *pair {
                w.u32(u32::from(lo) | (u32::from(hi) << 16));
            }
        }
    }
}

fn get_quad(r: &mut R<'_>) -> Wire<Quad> {
    let mask = r.u8()?;
    let mut parts = [[0.0f32; 4]; 7];
    for (i, part) in parts.iter_mut().enumerate() {
        if mask & (1 << i) != 0 {
            *part = r.f4()?;
        }
    }
    let mut from = [0u16; 8];
    if mask & (1 << 7) != 0 {
        for pair in from.chunks_exact_mut(2) {
            let v = r.u32()?;
            if let [lo, hi] = pair {
                #[expect(clippy::cast_possible_truncation, reason = "two sixteen-bit halves of one word")]
                {
                    *lo = v as u16;
                    *hi = (v >> 16) as u16;
                }
            }
        }
    }
    let [rect, params, fill, stroke, uv, extra, spin] = parts;
    Ok(Quad { rect, params, fill, stroke, uv, extra, spin, from })
}

fn put_list(w: &mut W, list: &DrawList) {
    w.u32(u32::try_from(list.quads.len()).unwrap_or(u32::MAX));
    for q in &list.quads {
        put_quad(w, q);
    }
    w.u32(u32::try_from(list.runs.len()).unwrap_or(u32::MAX));
    for r in &list.runs {
        w.u32(r.clip);
        w.u32(r.chain);
        w.u32(r.first);
        w.u32(r.count);
    }
    w.u32(u32::try_from(list.clips.len()).unwrap_or(u32::MAX));
    for c in &list.clips {
        for v in c {
            w.u32(*v);
        }
    }
    w.f4(list.clear);
    w.u32(u32::try_from(list.scrollers.len()).unwrap_or(u32::MAX));
    for s in &list.scrollers {
        w.f4([s.from[0], s.from[1], s.to[0], s.to[1]]);
        w.f32(s.t0);
        w.f32(s.dur);
        w.u32(s.curve);
    }
    w.bool(list.wants_frame);
    w.bool(list.gpu_only);
    w.u32(list.repeat_until_ms);
    w.u64(list.serial);
    match &list.backdrop {
        None => w.u32(0),
        Some(b) => {
            // The count doubles as the tag: a backdrop always has at least
            // one standard deviation in it, or `paint` would not have made
            // one.
            w.u32(u32::try_from(b.sigmas.len()).unwrap_or(u32::MAX));
            for v in b.rect {
                w.u32(v);
            }
            w.u32(b.first);
            for s in &b.sigmas {
                w.f32(*s);
            }
        }
    }
}

fn get_list(r: &mut R<'_>) -> Wire<DrawList> {
    let n = r.u32()? as usize;
    let mut quads = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        quads.push(get_quad(r)?);
    }
    let n = r.u32()? as usize;
    let mut runs = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        runs.push(Run { clip: r.u32()?, chain: r.u32()?, first: r.u32()?, count: r.u32()? });
    }
    let n = r.u32()? as usize;
    let mut clips = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        clips.push([r.u32()?, r.u32()?, r.u32()?, r.u32()?]);
    }
    let clear = r.f4()?;
    let n = r.u32()? as usize;
    let mut scrollers = Vec::with_capacity(n.min(1 << 8));
    for _ in 0..n {
        let [fx, fy, tx, ty] = r.f4()?;
        scrollers.push(Scroller { from: [fx, fy], to: [tx, ty], t0: r.f32()?, dur: r.f32()?, curve: r.u32()?, pad: 0 });
    }
    let wants_frame = r.bool()?;
    let gpu_only = r.bool()?;
    let repeat_until_ms = r.u32()?;
    let serial = r.u64()?;
    let n = r.u32()? as usize;
    let backdrop = if n == 0 {
        None
    } else {
        let rect = [r.u32()?, r.u32()?, r.u32()?, r.u32()?];
        let first = r.u32()?;
        let mut sigmas = Vec::with_capacity(n.min(1 << 8));
        for _ in 0..n {
            sigmas.push(r.f32()?);
        }
        Some(Backdrop { rect, first, sigmas })
    };
    Ok(DrawList { quads, runs, clips, clear, wants_frame, gpu_only, repeat_until_ms, serial, cpu_bound: false, scrollers, backdrop })
}

fn put_access(w: &mut W, s: &AccessSnapshot) {
    w.u32(u32::try_from(s.nodes.len()).unwrap_or(u32::MAX));
    for n in &s.nodes {
        w.u64(n.id);
        w.u8(n.role as u8);
        w.f4(n.bounds);
        w.str(&n.label);
        w.str(&n.value);
        w.u8(u8::from(n.click) | (u8::from(n.focus) << 1));
        w.u32(u32::try_from(n.children.len()).unwrap_or(u32::MAX));
        for c in &n.children {
            w.u64(*c);
        }
        put_state(w, &n.state);
    }
    w.u64(s.focus);
    w.f32(s.scale);
}

/// The declared state of one node. The booleans and the two tri-states pack
/// into a bitfield; the numbers follow, each behind its own presence bit,
/// because `0` is a value a slider may really sit at.
fn put_state(w: &mut W, st: &AccessState) {
    let mut flags: u32 = 0;
    let mut set = |on: bool, bit: u32| {
        if on {
            flags |= 1 << bit;
        }
    };
    set(st.disabled, 0);
    set(st.read_only, 1);
    set(st.required, 2);
    set(st.invalid, 3);
    set(st.busy, 4);
    set(st.modal, 5);
    set(st.checked.is_some(), 6);
    set(matches!(st.checked, Some(Checked::Yes)), 7);
    set(matches!(st.checked, Some(Checked::Mixed)), 8);
    set(st.expanded.is_some(), 9);
    set(st.expanded == Some(true), 10);
    set(st.selected.is_some(), 11);
    set(st.selected == Some(true), 12);
    set(st.value_now.is_some(), 13);
    set(st.value_min.is_some(), 14);
    set(st.value_max.is_some(), 15);
    w.u32(flags);
    w.str(&st.description);
    for v in [st.value_now, st.value_min, st.value_max] {
        w.f32(v.unwrap_or(0.0) as f32);
    }
    w.u32(st.pos_in_set);
    w.u32(st.set_size);
    w.u32(st.level);
    w.u8(st.orientation);
    w.u8(st.live);
}

fn get_state(r: &mut R<'_>) -> Wire<AccessState> {
    let flags = r.u32()?;
    let on = |bit: u32| flags & (1 << bit) != 0;
    let description = r.str()?;
    let nums = [r.f32()?, r.f32()?, r.f32()?];
    let some = |present: bool, v: f32| if present { Some(f64::from(v)) } else { None };
    Ok(AccessState {
        description,
        checked: on(6).then(|| {
            if on(8) {
                Checked::Mixed
            } else if on(7) {
                Checked::Yes
            } else {
                Checked::No
            }
        }),
        expanded: on(9).then(|| on(10)),
        selected: on(11).then(|| on(12)),
        disabled: on(0),
        read_only: on(1),
        required: on(2),
        invalid: on(3),
        busy: on(4),
        modal: on(5),
        value_now: some(on(13), nums[0]),
        value_min: some(on(14), nums[1]),
        value_max: some(on(15), nums[2]),
        pos_in_set: r.u32()?,
        set_size: r.u32()?,
        level: r.u32()?,
        orientation: r.u8()?,
        live: r.u8()?,
    })
}

fn get_access(r: &mut R<'_>) -> Wire<AccessSnapshot> {
    let n = r.u32()? as usize;
    let mut nodes = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        let id = r.u64()?;
        let role = AccessRole::from_u8(r.u8()?).ok_or("role")?;
        let bounds = r.f4()?;
        let label = r.str()?;
        let value = r.str()?;
        let actions = r.u8()?;
        let k = r.u32()? as usize;
        let mut children = Vec::with_capacity(k.min(1 << 16));
        for _ in 0..k {
            children.push(r.u64()?);
        }
        let state = get_state(r)?;
        nodes.push(AccessNode { id, role, bounds, label, value, click: actions & 1 != 0, focus: actions & 2 != 0, children, state });
    }
    Ok(AccessSnapshot { nodes, focus: r.u64()?, scale: r.f32()? })
}

fn write_message(out: &mut impl Write, payload: &[u8]) -> std::io::Result<()> {
    let len = u32::try_from(payload.len()).map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "message too long"))?;
    out.write_all(&len.to_le_bytes())?;
    out.write_all(payload)?;
    out.flush()
}

fn read_message(input: &mut impl Read, max: usize) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    input.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > max {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "message too long"));
    }
    let mut buf = vec![0u8; len];
    input.read_exact(&mut buf)?;
    Ok(buf)
}

// --------------------------------------------------------------- worker

/// The worker's side: the driver, and the loop that answers requests until
/// the window closes the pipe. `sandbox` is what [`crate::sandbox::lock_down`]
/// said, reported back with the first reply.
pub fn serve(input: &mut impl Read, output: &mut impl Write, sandbox: Result<String, String>) -> std::io::Result<()> {
    let mut driver: Option<Driver> = None;
    let mut sandbox = Some(sandbox);
    loop {
        let bytes = match read_message(input, usize::MAX) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(e),
        };
        let request = Request::decode(&bytes).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let reply = match (&mut driver, request) {
            (_, Request::Config { w, h, scale, granted }) => {
                let mut d = Driver::new(w, h, scale, granted);
                // Both atlases start "dirty" so a first upload happens; the
                // window's own copies do the same, so nothing is owed yet.
                let (atlas, images) = d.atlases_mut();
                atlas.mark_clean();
                images.mark_clean();
                driver = Some(d);
                let status = driver.as_mut().map(status_of).unwrap_or_default();
                Reply { status, payload: Payload::Sandbox(sandbox.take().unwrap_or_else(|| Err("reported already".into()))) }
            }
            (None, _) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "a request before Config")),
            (Some(d), request) => {
                let payload = match request {
                    Request::Config { .. } => Payload::None,
                    Request::Grant(g) => {
                        d.grant(g);
                        Payload::None
                    }
                    Request::Hello => Payload::Hello(d.hello().encode()),
                    Request::Frame(b) => {
                        match Frame::decode(&b) {
                            Ok(f) => {
                                let out = d.handle_frame(f);
                                d.pending_mut().extend(out);
                            }
                            Err(e) => d.close(format!("bad frame: {e}")),
                        }
                        Payload::None
                    }
                    Request::Input(i) => {
                        let out = d.input(i);
                        d.pending_mut().extend(out);
                        Payload::None
                    }
                    Request::AssetReady(h, b) => {
                        d.asset_ready(h, b);
                        Payload::None
                    }
                    Request::AssetFailed(h, why) => {
                        d.asset_failed(h, why);
                        Payload::None
                    }
                    Request::PendingAssets => Payload::Assets(d.pending_assets()),
                    Request::Paint(w, h) => {
                        // The window answers ticks itself while it draws
                        // the last list again, so the clock here has to be
                        // moved on before the paint that ends that.
                        d.tick(Instant::now());
                        let list = d.paint(w, h);
                        let (atlas, images) = d.atlases_mut();
                        let glyphs: Vec<(u32, u32, u32, Vec<u8>)> = atlas.dirty_bands().iter().map(|&(y0, y1)| (atlas.size(), y0, y1, atlas.rows(y0, y1).to_vec())).collect();
                        atlas.mark_clean();
                        let images = images.dirty_rows().map(|(y0, y1)| {
                            images.mark_clean();
                            (y0, y1, images.rows(y0, y1).to_vec())
                        });
                        Payload::Paint { list, glyphs, images }
                    }
                    Request::Tick => Payload::Tick(d.tick(Instant::now())),
                    Request::AccessTree => Payload::Access(d.access_snapshot()),
                    Request::AccessAction(id, click) => {
                        if let Some(ix) = d.node_for_accessibility(id) {
                            let out = if click { d.activate_node(ix) } else { d.focus_node(ix) };
                            d.pending_mut().extend(out);
                        }
                        Payload::None
                    }
                    Request::Audio { frames, channels, rate } => {
                        let channels = u16::from(channels).clamp(1, 2);
                        let n = (frames as usize).saturating_mul(usize::from(channels)).min(1 << 20);
                        let mut pcm = vec![0.0f32; n];
                        let out = d.fill_audio(&mut pcm, channels, rate);
                        d.pending_mut().extend(out);
                        Payload::Pcm(pcm)
                    }
                    Request::Picked { token, files } => {
                        let (ids, out) = d.picked(token, files);
                        d.pending_mut().extend(out);
                        Payload::Uploads(ids)
                    }
                    Request::Dismissed(token) => {
                        d.dialog_dismissed(token);
                        Payload::None
                    }
                    Request::UploadChunk { id, bytes, last } => {
                        let out = d.upload_chunk(id, &bytes, last);
                        d.pending_mut().extend(out);
                        Payload::None
                    }
                    Request::UploadFailed(id, why) => {
                        let out = d.upload_failed(id, why);
                        d.pending_mut().extend(out);
                        Payload::None
                    }
                    Request::Saving { token, name } => {
                        let out = d.saving(token, name);
                        d.pending_mut().extend(out);
                        Payload::None
                    }
                    Request::DesktopTheme(mode, colors) => {
                        let colors = colors.into_iter().filter_map(|(id, c)| eui_theme::Role::from_id(id).ok().map(|r| (r, c))).collect();
                        let out = d.set_desktop_theme(mode, colors);
                        d.pending_mut().extend(out);
                        Payload::None
                    }
                };
                Reply { status: status_of(d), payload }
            }
        };
        write_message(output, &reply.encode())?;
    }
}

/// The driver's state the window will ask about, taken now. Outbound
/// frames are whatever accumulated since the last reply: what a frame or
/// an input produced, and what a paint's scroll landing emitted.
fn status_of(d: &mut Driver) -> Status {
    let now = Instant::now();
    Status {
        outbound: d.take_pending().iter().map(Frame::encode).collect(),
        needs_redraw: d.needs_redraw(),
        closed: d.closed().map(|c| format!("{c:?}")),
        ime: d.ime_area().map(|r| [r.x, r.y, r.w, r.h]),
        clipboard: d.take_clipboard(),
        next_due_ms: d.next_frame_at().map(|at| u32::try_from(at.saturating_duration_since(now).as_millis()).unwrap_or(u32::MAX)),
        cursor: d.cursor().to_u8(),
        audio: d.audio_playing(),
        video: d.video_playing(),
        files: d.take_file_asks(),
        writes: d.take_writes(),
    }
}

/// Leave SIGINT, SIGTERM and SIGHUP to the window process: a worker that
/// died on a terminal's Ctrl+C would take the session with it while the
/// window was still shutting down in order.
#[cfg(unix)]
fn ignore_terminal_signals() {
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        let _ = signal_hook::flag::register(sig, std::sync::Arc::clone(&flag));
    }
}

#[cfg(not(unix))]
fn ignore_terminal_signals() {}

/// Build and drop a driver so that everything that initialises itself
/// lazily has done so before the sandbox closes — the text engine's font
/// loader starts a thread pool, and asks the system for its core count.
fn warm_up() {
    drop(Driver::new(1.0, 1.0, 1.0, 0));
}

/// The worker binary's entry: `Some(exit code)` when `args` name a worker
/// role, `None` when this is an ordinary launch. A host binary that embeds
/// the client calls this first thing in `main`, so the process it spawns
/// for the worker — itself — takes the role.
pub fn entry(args: &[String]) -> Option<i32> {
    let first = args.first().map(String::as_str)?;
    match first {
        WORKER_ARG => {
            // A terminal's Ctrl+C reaches the whole process group; the
            // worker leaves when its window does, not before.
            ignore_terminal_signals();
            warm_up();
            let sandbox = crate::sandbox::lock_down();
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let mut input = BufReader::new(stdin.lock());
            let mut output = BufWriter::new(stdout.lock());
            Some(match serve(&mut input, &mut output, sandbox) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("eui worker: {e}");
                    1
                }
            })
        }
        SELFTEST_ARG => Some(selftest(args.get(1).map(String::as_str).unwrap_or(""))),
        _ => None,
    }
}

/// Lock down, then try one thing the worker must not be able to do. Exit
/// `0` when the attempt was refused, `3` when it went through; a seccomp
/// kill ends the process on a signal before either.
fn selftest(what: &str) -> i32 {
    warm_up();
    match crate::sandbox::lock_down() {
        Ok(s) => eprintln!("sandbox: {s}"),
        Err(e) => eprintln!("sandbox: {e}"),
    }
    let attempt: Result<(), String> = match what {
        "none" => Ok(()),
        "fs" => std::fs::read("/etc/hostname").map(|_| ()).map_err(|e| e.to_string()),
        "net" => std::net::TcpStream::connect("127.0.0.1:9").map(|_| ()).map_err(|e| e.to_string()),
        "exec" => Command::new("/bin/true").status().map(|_| ()).map_err(|e| e.to_string()),
        other => {
            eprintln!("unknown self-test {other:?}");
            return 2;
        }
    };
    match (what, attempt) {
        ("none", _) => 0,
        (_, Err(e)) => {
            eprintln!("refused: {e}");
            0
        }
        (_, Ok(())) => {
            eprintln!("went through");
            3
        }
    }
}

// --------------------------------------------------------------- window

/// The window's handle on a worker process.
pub struct Worker {
    child: Child,
    input: BufWriter<std::process::ChildStdin>,
    output: BufReader<std::process::ChildStdout>,
    status: Status,
    /// When the last reply's `next_due_ms` was taken, to turn it back into
    /// an instant.
    due: Option<Instant>,
    dead: Option<String>,
    /// Bytes written to and read from the pipe since the worker started,
    /// framing included. What the process boundary costs is a number the
    /// budgets ask for (10 §1), and only this side can count it.
    traffic: (u64, u64),
    /// The last `Paint` reply, while the driver said the list may be
    /// drawn again. While that holds, the next `Paint` is answered from
    /// here and the pipe is not touched: the list cannot have changed,
    /// because the only thing that moves is a clock the vertex stage
    /// reads. Any request but a tick clears it -- a request is the one way
    /// the tree can come to paint differently, and every one of them
    /// passes through [`Worker::call`]. A tick is a clock, and while the
    /// list is repeated the window keeps that clock itself.
    repeat: Option<Repeat>,
    /// When a list with a new serial last arrived: its own clock starts
    /// there, and the window measures its age from it.
    received: Instant,
}

/// A `Paint` reply the window may hand back for the next paint, with
/// everything that happens once taken out of it: frames bound for the
/// server, a clipboard hand-off, an IME rect and the atlas rows are all
/// answers to something that already happened, and replaying them would
/// say it twice. What is left is the list, that a redraw is wanted, and
/// when.
#[derive(Debug, Clone, PartialEq)]
struct Repeat {
    reply: Reply,
    /// The last moment the list is right; `None` for as long as nothing
    /// reaches the driver.
    until: Option<Instant>,
}

impl Repeat {
    /// What a paint reply leaves behind for the paints after it, if the
    /// driver said it may be drawn again.
    fn of(reply: &Reply, received: Instant) -> Option<Self> {
        let Payload::Paint { list, .. } = &reply.payload else {
            return None;
        };
        if !list.gpu_only || reply.status.closed.is_some() {
            return None;
        }
        let until = (list.repeat_until_ms != u32::MAX).then(|| received + Duration::from_millis(u64::from(list.repeat_until_ms)));
        let reply = Reply {
            status: Status { needs_redraw: reply.status.needs_redraw, next_due_ms: reply.status.next_due_ms, ..Status::default() },
            payload: Payload::Paint { list: Arc::clone(list), glyphs: Vec::new(), images: None },
        };
        Some(Self { reply, until })
    }

    /// The reply for a paint at `now`, unless the list has run out.
    fn answer(&self, now: Instant) -> Option<Reply> {
        let left = match self.until {
            Some(u) if now >= u => return None,
            Some(u) => Some(u.saturating_duration_since(now)),
            None => None,
        };
        let mut reply = self.reply.clone();
        // Due at its cadence, and no later than the list runs out.
        if let Some(left) = left {
            let left = u32::try_from(left.as_millis()).unwrap_or(u32::MAX);
            reply.status.next_due_ms = Some(reply.status.next_due_ms.map_or(left, |ms| ms.min(left)));
        }
        Some(reply)
    }

    /// Whether the window can answer `request` from this, or the driver
    /// must be asked and the list forgotten.
    fn survives(request: &Request) -> bool {
        matches!(request, Request::Paint(..) | Request::Tick)
    }
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker").field("pid", &self.child.id()).field("dead", &self.dead).finish()
    }
}

impl Worker {
    /// Start `program` as a worker and configure it. `sandbox` receives
    /// what the worker managed to enforce.
    pub fn spawn(program: PathBuf, w: f32, h: f32, scale: f32, granted: u32) -> Result<(Self, Result<String, String>), String> {
        let mut cmd = Command::new(&program);
        cmd.arg(WORKER_ARG).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).env_clear();
        for var in ["EUI_TRACE", "EUI_SECCOMP_LOG", "RUST_BACKTRACE"] {
            if let Ok(v) = std::env::var(var) {
                cmd.env(var, v);
            }
        }
        let mut child = cmd.spawn().map_err(|e| format!("cannot start the worker {}: {e}", program.display()))?;
        let input = child.stdin.take().ok_or("worker has no stdin")?;
        let output = child.stdout.take().ok_or("worker has no stdout")?;
        let mut worker =
            Self { child, input: BufWriter::new(input), output: BufReader::new(output), status: Status::default(), due: None, dead: None, traffic: (0, 0), repeat: None, received: Instant::now() };
        let reply = worker.call(&Request::Config { w, h, scale, granted });
        match reply.map(|r| r.payload) {
            Some(Payload::Sandbox(s)) => Ok((worker, s)),
            _ => Err(worker.dead.take().unwrap_or_else(|| "the worker did not answer its configuration".into())),
        }
    }

    /// Send one request and wait for its reply. `None` once the worker is
    /// gone; [`Self::status`] then says why.
    pub fn call(&mut self, request: &Request) -> Option<Reply> {
        if self.dead.is_some() {
            return None;
        }
        // A repeat of a paint the driver said may be drawn again: the same
        // list, with the clock moved on. Nothing crosses the pipe and the
        // driver is not woken, so a spinner costs a draw and no more.
        if matches!(request, Request::Paint(..)) {
            let now = Instant::now();
            if let Some(reply) = self.repeat.as_ref().and_then(|r| r.answer(now)) {
                self.keep(reply.status.clone());
                self.due = Some(now);
                crate::driver::trace(|| "paint: the last list again, the pipe untouched".to_owned());
                return Some(reply);
            }
        }
        if !Repeat::survives(request) {
            // Anything else may change what the tree paints.
            self.repeat = None;
        }
        let encoded = request.encode();
        let result = write_message(&mut self.input, &encoded).and_then(|()| read_message(&mut self.output, MAX_REPLY));
        let result = result.and_then(|b| {
            // Four bytes of length either way, the same framing both ends
            // agreed on above.
            self.traffic.0 = self.traffic.0.saturating_add(encoded.len() as u64 + 4);
            self.traffic.1 = self.traffic.1.saturating_add(b.len() as u64 + 4);
            Reply::decode(&b).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        });
        match result {
            Ok(reply) => {
                let now = Instant::now();
                self.keep(reply.status.clone());
                self.due = Some(now);
                if let Payload::Paint { .. } = reply.payload {
                    self.received = now;
                    self.repeat = Repeat::of(&reply, now);
                }
                Some(reply)
            }
            Err(e) => {
                // A dead worker is usually a killed one; give the kernel a
                // moment to say so, since the signal is the whole story.
                let mut status = None;
                for _ in 0..20 {
                    if let Ok(Some(st)) = self.child.try_wait() {
                        status = Some(st);
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                let why = match status {
                    Some(st) => format!("the worker exited: {st}"),
                    None => format!("the worker stopped answering: {e}"),
                };
                eprintln!("eui: {why}");
                self.status.closed = Some(why.clone());
                self.status.needs_redraw = false;
                self.status.next_due_ms = None;
                self.dead = Some(why);
                None
            }
        }
    }

    /// The status after a reply, keeping what the window has not collected
    /// yet. Everything else in a status is the driver's state now, and the
    /// new answer replaces it; a dialog and a chunk of a file are *events*,
    /// and a second call before the window looked must not drop them.
    fn keep(&mut self, mut status: Status) {
        if !self.status.files.is_empty() {
            let mut files = std::mem::take(&mut self.status.files);
            files.append(&mut status.files);
            status.files = files;
        }
        if !self.status.writes.is_empty() {
            let mut writes = std::mem::take(&mut self.status.writes);
            writes.append(&mut status.writes);
            status.writes = writes;
        }
        if status.clipboard.is_none() {
            status.clipboard = self.status.clipboard.take();
        }
        self.status = status;
    }

    /// State after the last reply.
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// `(sent, received)` bytes over the pipe since the worker started.
    pub fn traffic(&self) -> (u64, u64) {
        self.traffic
    }

    /// Kill the worker, for the test that checks a dead one is reported
    /// rather than fatal.
    #[doc(hidden)]
    pub fn kill_for_test(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Closing its stdin ends the loop; then reap it.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Where the window's driver runs.
///
/// Both variants keep the driver behind a lock, because two threads reach
/// it: the window's loop, and the audio thread that keeps the device fed
/// (03 §7). The audio side holds the lock for the microseconds a mix
/// takes; a paint holds it for a frame.
#[derive(Debug, Clone)]
pub enum Backend {
    /// In this process.
    Local(Arc<Mutex<Driver>>),
    /// In a worker process, with the atlases it last sent.
    Remote {
        /// The worker.
        worker: Arc<Mutex<Worker>>,
        /// The glyph atlas, as the worker last painted it.
        atlas: Arc<Mutex<Atlas>>,
        /// The image atlas.
        images: Arc<Mutex<ImageAtlas>>,
    },
}

/// What the window's audio thread holds: a way to ask for frames, and
/// nothing else. `Send`, unlike anything that draws.
#[derive(Debug, Clone)]
pub enum AudioTap {
    /// The driver in this process.
    Local(Arc<Mutex<Driver>>),
    /// The driver in the worker.
    Remote(Arc<Mutex<Worker>>),
}

impl AudioTap {
    /// Mix into `out`, `channels` samples a frame at `rate`. Returns the
    /// encoded frames the mix produced — a sound's `ended` — to send.
    /// Silence, and nothing to send, when the driver cannot be reached.
    pub fn fill(&self, out: &mut [f32], channels: u16, rate: u32) -> Vec<Vec<u8>> {
        match self {
            AudioTap::Local(d) => match d.lock() {
                Ok(mut d) => d.fill_audio(out, channels, rate).iter().map(Frame::encode).collect(),
                Err(_) => {
                    silence(out);
                    Vec::new()
                }
            },
            AudioTap::Remote(w) => {
                let channels = channels.clamp(1, 2);
                let frames = u32::try_from(out.len() / usize::from(channels)).unwrap_or(0);
                let request = Request::Audio { frames, channels: channels as u8, rate };
                let reply = match w.lock() {
                    Ok(mut w) => w.call(&request),
                    Err(_) => None,
                };
                match reply {
                    Some(Reply { status, payload: Payload::Pcm(pcm) }) if pcm.len() == out.len() => {
                        out.copy_from_slice(&pcm);
                        status.outbound
                    }
                    _ => {
                        silence(out);
                        Vec::new()
                    }
                }
            }
        }
    }
}

fn silence(out: &mut [f32]) {
    for s in out.iter_mut() {
        *s = 0.0;
    }
}

impl Backend {
    /// A driver in a worker process when one can be started, else in this
    /// process; the string says which and, for a worker, what its sandbox
    /// enforced. `EUI_SANDBOX=0` asks for this process outright.
    pub fn open(w: f32, h: f32, scale: f32, granted: u32) -> (Self, String) {
        if std::env::var("EUI_SANDBOX").is_ok_and(|v| v == "0") {
            return (Backend::local(Driver::new(w, h, scale, granted)), "driver in this process (EUI_SANDBOX=0)".into());
        }
        let program = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => return (Backend::local(Driver::new(w, h, scale, granted)), format!("driver in this process: cannot find this binary ({e})")),
        };
        Self::open_with(program, w, h, scale, granted)
    }

    /// [`Self::open`] with the worker binary named.
    pub fn open_with(program: PathBuf, w: f32, h: f32, scale: f32, granted: u32) -> (Self, String) {
        match Worker::spawn(program, w, h, scale, granted) {
            Ok((worker, sandbox)) => {
                let pid = worker.child.id();
                let how = match sandbox {
                    Ok(s) => format!("driver in worker {pid}: {s}"),
                    Err(e) => format!("driver in worker {pid}, unconfined: {e}"),
                };
                let backend = Backend::Remote { worker: Arc::new(Mutex::new(worker)), atlas: Arc::new(Mutex::new(Atlas::new())), images: Arc::new(Mutex::new(ImageAtlas::new())) };
                (backend, how)
            }
            Err(e) => (Backend::local(Driver::new(w, h, scale, granted)), format!("driver in this process: {e}")),
        }
    }

    /// Wrap a driver of this process.
    pub fn local(driver: Driver) -> Self {
        Backend::Local(Arc::new(Mutex::new(driver)))
    }

    /// Run `f` against the driver when it is in this process.
    fn with_local<T>(&self, f: impl FnOnce(&mut Driver) -> T) -> Option<T> {
        match self {
            Backend::Local(d) => d.lock().ok().map(|mut d| f(&mut d)),
            Backend::Remote { .. } => None,
        }
    }

    /// Run `f` against the worker when the driver is in one.
    fn with_worker<T>(&self, f: impl FnOnce(&mut Worker) -> T) -> Option<T> {
        match self {
            Backend::Remote { worker, .. } => worker.lock().ok().map(|mut w| f(&mut w)),
            Backend::Local(_) => None,
        }
    }

    /// `(sent, received)` bytes over the pipe since the worker started, or
    /// `None` when the driver is in this process and nothing crosses one.
    pub fn traffic(&self) -> Option<(u64, u64)> {
        match self {
            Backend::Local(_) => None,
            Backend::Remote { worker, .. } => worker.lock().ok().map(|w| w.traffic()),
        }
    }

    /// What the window's audio thread holds.
    pub fn audio_tap(&self) -> AudioTap {
        match self {
            Backend::Local(d) => AudioTap::Local(Arc::clone(d)),
            Backend::Remote { worker, .. } => AudioTap::Remote(Arc::clone(worker)),
        }
    }

    /// Change the grant before the session opens.
    pub fn grant(&mut self, granted: u32) {
        self.with_local(|d| d.grant(granted));
        self.with_worker(|w| {
            w.call(&Request::Grant(granted));
        });
    }

    /// The opening frame, encoded.
    pub fn hello(&mut self) -> Vec<u8> {
        if let Some(b) = self.with_local(|d| d.hello().encode()) {
            return b;
        }
        match self.with_worker(|w| w.call(&Request::Hello).map(|r| r.payload)) {
            Some(Some(Payload::Hello(b))) => b,
            _ => Vec::new(),
        }
    }

    /// A frame from the server, as received. Returns encoded frames to send
    /// back.
    pub fn frame(&mut self, bytes: Vec<u8>) -> Vec<Vec<u8>> {
        if let Some(out) = self.with_local(|d| match Frame::decode(&bytes) {
            Ok(f) => d.handle_frame(f).iter().map(Frame::encode).collect(),
            Err(e) => {
                d.close(format!("bad frame: {e}"));
                Vec::new()
            }
        }) {
            return out;
        }
        self.with_worker(|w| w.call(&Request::Frame(bytes)).map(|r| r.status.outbound).unwrap_or_default()).unwrap_or_default()
    }

    /// Something the viewer did. Returns encoded frames to send.
    pub fn input(&mut self, input: Input) -> Vec<Vec<u8>> {
        if let Some(out) = self.with_local(|d| d.input(input.clone()).iter().map(Frame::encode).collect::<Vec<_>>()) {
            return out;
        }
        self.with_worker(|w| w.call(&Request::Input(input)).map(|r| r.status.outbound).unwrap_or_default()).unwrap_or_default()
    }

    /// Verified bytes for a hash.
    pub fn asset_ready(&mut self, hash: Hash, bytes: Vec<u8>) {
        if self.with_local(|d| d.asset_ready(hash, bytes.clone())).is_some() {
            return;
        }
        self.with_worker(|w| {
            w.call(&Request::AssetReady(hash, bytes));
        });
    }

    /// A hash that could not be fetched.
    pub fn asset_failed(&mut self, hash: Hash, why: String) {
        if self.with_local(|d| d.asset_failed(hash, why.clone())).is_some() {
            return;
        }
        self.with_worker(|w| {
            w.call(&Request::AssetFailed(hash, why));
        });
    }

    /// Hashes the tree needs and nobody fetched yet.
    pub fn pending_assets(&mut self) -> Vec<Hash> {
        if let Some(h) = self.with_local(|d| d.pending_assets()) {
            return h;
        }
        match self.with_worker(|w| w.call(&Request::PendingAssets).map(|r| r.payload)) {
            Some(Some(Payload::Assets(hs))) => hs,
            _ => Vec::new(),
        }
    }

    /// Lay out and paint for a `w × h` device-pixel target. Returns the
    /// draw list and the encoded frames a scroll landing during the paint
    /// emitted, to send after drawing.
    pub fn paint(&mut self, w: u32, h: u32) -> (Arc<DrawList>, Vec<Vec<u8>>) {
        if let Some(out) = self.with_local(|d| {
            let list = d.paint(w, h);
            (list, d.take_pending().iter().map(Frame::encode).collect::<Vec<_>>())
        }) {
            return out;
        }
        let Backend::Remote { worker, atlas, images } = self else {
            return (Arc::new(DrawList::default()), Vec::new());
        };
        let request = Request::Paint(w, h);
        let reply = match worker.lock() {
            Ok(mut worker) => worker.call(&request),
            Err(_) => None,
        };
        match reply {
            Some(Reply { status, payload: Payload::Paint { list, glyphs, images: image_rows } }) => {
                // Only the rows that changed cross the pipe.
                if let Ok(mut a) = atlas.lock() {
                    for (size, y0, y1, px) in glyphs {
                        if !a.set_rows(size, y0, y1, &px) {
                            eprintln!("eui: the worker sent glyph atlas rows of the wrong size");
                        }
                    }
                }
                if let (Some((y0, y1, px)), Ok(mut i)) = (image_rows, images.lock()) {
                    if !i.set_rows(y0, y1, &px) {
                        eprintln!("eui: the worker sent image atlas rows of the wrong size");
                    }
                }
                (list, status.outbound)
            }
            _ => (Arc::new(DrawList::default()), Vec::new()),
        }
    }

    /// How old the list last handed out is, in seconds — its own clock,
    /// which the vertex stage animates from (03 §5).
    pub fn list_age(&self, now: Instant) -> f32 {
        if let Some(age) = self.with_local(|d| d.list_age(now)) {
            return age;
        }
        self.with_worker(|w| now.saturating_duration_since(w.received).as_secs_f32()).unwrap_or(0.0)
    }

    /// The atlases the last draw list refers to, for the renderer's upload.
    pub fn with_atlases<T>(&mut self, f: impl FnOnce(&mut Atlas, &mut ImageAtlas) -> T) -> Option<T> {
        match self {
            Backend::Local(d) => d.lock().ok().map(|mut d| {
                let (a, i) = d.atlases_mut();
                f(a, i)
            }),
            Backend::Remote { atlas, images, .. } => {
                let (Ok(mut a), Ok(mut i)) = (atlas.lock(), images.lock()) else {
                    return None;
                };
                Some(f(&mut a, &mut i))
            }
        }
    }

    /// Advance the clock; true when a transition frame is due.
    pub fn tick(&mut self, now: Instant) -> bool {
        if let Some(due) = self.with_local(|d| d.tick(now)) {
            return due;
        }
        self.with_worker(|w| {
            // Nothing can be due before the worker said something would be:
            // skip the round trip at rest.
            let Some(ms) = w.status.next_due_ms else {
                return false;
            };
            // While the last list is being drawn again the clock is the
            // window's to keep: the driver would only say what the reply
            // already did, and the round trip is the one cost a repeated
            // frame has left.
            if w.repeat.as_ref().is_some_and(|r| r.until.map_or(true, |u| now < u)) {
                return w.due.is_some_and(|d| now >= d + Duration::from_millis(u64::from(ms)));
            }
            matches!(w.call(&Request::Tick).map(|r| r.payload), Some(Payload::Tick(true)))
        })
        .unwrap_or(false)
    }

    /// When the next transition frame is due, `None` at rest.
    pub fn next_frame_at(&self) -> Option<Instant> {
        if let Some(at) = self.with_local(|d| d.next_frame_at()) {
            return at;
        }
        self.with_worker(|w| {
            let ms = w.status.next_due_ms?;
            Some(w.due.unwrap_or_else(Instant::now) + Duration::from_millis(u64::from(ms)))
        })
        .flatten()
    }

    /// A frame should be drawn.
    pub fn needs_redraw(&self) -> bool {
        self.with_local(|d| d.needs_redraw()).or_else(|| self.with_worker(|w| w.status.needs_redraw)).unwrap_or(false)
    }

    /// Why the session ended, if it did.
    pub fn closed(&self) -> Option<String> {
        self.with_local(|d| d.closed().map(ToString::to_string)).or_else(|| self.with_worker(|w| w.status.closed.clone())).flatten()
    }

    /// Where an input method's candidate window goes: `x, y, w, h` in
    /// logical px, if a field has focus.
    pub fn ime_area(&self) -> Option<[f32; 4]> {
        self.with_local(|d| d.ime_area().map(|r| [r.x, r.y, r.w, r.h])).or_else(|| self.with_worker(|w| w.status.ime)).flatten()
    }

    /// The pointer's shape over what it is on.
    pub fn cursor(&self) -> eui_proto::Cursor {
        self.with_local(|d| d.cursor()).or_else(|| self.with_worker(|w| eui_proto::Cursor::from_u8(w.status.cursor).unwrap_or(eui_proto::Cursor::Default))).unwrap_or(eui_proto::Cursor::Default)
    }

    /// Text the viewer copied since the last call.
    pub fn take_clipboard(&mut self) -> Option<String> {
        self.with_local(|d| d.take_clipboard()).or_else(|| self.with_worker(|w| w.status.clipboard.take())).flatten()
    }

    /// Dialogs the tree asked for since the last call (spec 03 §3.2).
    pub fn take_file_asks(&mut self) -> Vec<FileAsk> {
        if let Some(asks) = self.with_local(|d| d.take_file_asks()) {
            return asks;
        }
        self.with_worker(|w| std::mem::take(&mut w.status.files)).unwrap_or_default()
    }

    /// Bytes a save is owed, for the window to put on disk.
    pub fn take_writes(&mut self) -> Vec<FileWrite> {
        if let Some(writes) = self.with_local(|d| d.take_writes()) {
            return writes;
        }
        self.with_worker(|w| std::mem::take(&mut w.status.writes)).unwrap_or_default()
    }

    /// The person chose files: an upload id each, and the frames that tell
    /// the server what is coming.
    pub fn picked(&mut self, token: u32, files: Vec<(String, u64)>) -> (Vec<u32>, Vec<Vec<u8>>) {
        if let Some(out) = self.with_local(|d| {
            let (ids, frames) = d.picked(token, files.clone());
            (ids, frames.iter().map(Frame::encode).collect::<Vec<_>>())
        }) {
            return out;
        }
        match self.with_worker(|w| w.call(&Request::Picked { token, files })) {
            Some(Some(reply)) => {
                let ids = match reply.payload {
                    Payload::Uploads(ids) => ids,
                    _ => Vec::new(),
                };
                (ids, reply.status.outbound)
            }
            _ => (Vec::new(), Vec::new()),
        }
    }

    /// A dialog was dismissed without choosing.
    pub fn dismissed(&mut self, token: u32) {
        if self.with_local(|d| d.dialog_dismissed(token)).is_some() {
            return;
        }
        self.with_worker(|w| {
            w.call(&Request::Dismissed(token));
        });
    }

    /// Bytes of an upload, framed for the wire.
    pub fn upload_chunk(&mut self, id: u32, bytes: Vec<u8>, last: bool) -> Vec<Vec<u8>> {
        if let Some(out) = self.with_local(|d| d.upload_chunk(id, &bytes, last).iter().map(Frame::encode).collect::<Vec<_>>()) {
            return out;
        }
        self.with_worker(|w| w.call(&Request::UploadChunk { id, bytes, last }).map(|r| r.status.outbound).unwrap_or_default()).unwrap_or_default()
    }

    /// The window could not read what was picked.
    pub fn upload_failed(&mut self, id: u32, why: String) -> Vec<Vec<u8>> {
        if let Some(out) = self.with_local(|d| d.upload_failed(id, why.clone()).iter().map(Frame::encode).collect::<Vec<_>>()) {
            return out;
        }
        self.with_worker(|w| w.call(&Request::UploadFailed(id, why)).map(|r| r.status.outbound).unwrap_or_default()).unwrap_or_default()
    }

    /// The person chose where a save goes; the server is asked for it.
    pub fn saving(&mut self, token: u32, name: String) -> Vec<Vec<u8>> {
        if let Some(out) = self.with_local(|d| d.saving(token, name.clone()).iter().map(Frame::encode).collect::<Vec<_>>()) {
            return out;
        }
        self.with_worker(|w| w.call(&Request::Saving { token, name }).map(|r| r.status.outbound).unwrap_or_default()).unwrap_or_default()
    }

    /// The viewer's desktop palette (05 §5), or none. Returns encoded
    /// frames to send: the viewport, when the palette's mode differs.
    pub fn desktop_theme(&mut self, mode: Option<ThemeMode>, colors: Vec<(eui_theme::Role, u32)>) -> Vec<Vec<u8>> {
        let wire: Vec<(u16, u32)> = colors.iter().map(|(r, c)| (r.id(), *c)).collect();
        if let Some(out) = self.with_local(|d| d.set_desktop_theme(mode, colors).iter().map(Frame::encode).collect::<Vec<_>>()) {
            return out;
        }
        self.with_worker(|w| w.call(&Request::DesktopTheme(mode, wire)).map(|r| r.status.outbound).unwrap_or_default()).unwrap_or_default()
    }

    /// The accessibility tree as painted.
    pub fn access_tree(&mut self) -> AccessSnapshot {
        if let Some(s) = self.with_local(|d| d.access_snapshot()) {
            return s;
        }
        match self.with_worker(|w| w.call(&Request::AccessTree).map(|r| r.payload)) {
            Some(Some(Payload::Access(s))) => s,
            _ => AccessSnapshot { nodes: Vec::new(), focus: 0, scale: 1.0 },
        }
    }

    /// An assistive technology's action on a node: `click`, or focus.
    /// Returns encoded frames to send.
    pub fn access_action(&mut self, id: u64, click: bool) -> Vec<Vec<u8>> {
        if let Some(out) = self.with_local(|d| {
            let Some(ix) = d.node_for_accessibility(id) else {
                return Vec::new();
            };
            let out = if click { d.activate_node(ix) } else { d.focus_node(ix) };
            out.iter().map(Frame::encode).collect::<Vec<_>>()
        }) {
            return out;
        }
        self.with_worker(|w| w.call(&Request::AccessAction(id, click)).map(|r| r.status.outbound).unwrap_or_default()).unwrap_or_default()
    }

    /// True while a picture is playing (03 §8).
    pub fn video_playing(&self) -> bool {
        self.with_local(|d| d.video_playing()).or_else(|| self.with_worker(|w| w.status.video)).unwrap_or(false)
    }

    /// True while any sound is loaded: the window opens its audio device
    /// only then, and closes it when nothing is left.
    pub fn audio_playing(&self) -> bool {
        self.with_local(|d| d.audio_playing()).or_else(|| self.with_worker(|w| w.status.audio)).unwrap_or(false)
    }

    /// Run `f` against the driver when it is in this process — tests, and
    /// the window's own diagnostics.
    pub fn driver<T>(&self, f: impl FnOnce(&Driver) -> T) -> Option<T> {
        match self {
            Backend::Local(d) => d.lock().ok().map(|d| f(&d)),
            Backend::Remote { .. } => None,
        }
    }

    /// What the last reply said, when the driver is in a worker.
    pub fn worker_status(&self) -> Option<Status> {
        self.with_worker(|w| w.status.clone())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn requests_round_trip() {
        let all = vec![
            Request::Config { w: 1.5, h: 2.5, scale: 2.0, granted: 7 },
            Request::Grant(3),
            Request::Hello,
            Request::Frame(vec![1, 2, 3]),
            Request::Input(Input::PointerMove(1.0, 2.0)),
            Request::Input(Input::Key { key: "Enter".into(), modifiers: 2, down: true }),
            Request::Input(Input::Mode(ThemeMode::Dark)),
            Request::Input(Input::Unfocused),
            Request::AssetReady([7; 32], vec![9; 10]),
            Request::AssetFailed([8; 32], "gone".into()),
            Request::PendingAssets,
            Request::Paint(640, 480),
            Request::Tick,
            Request::AccessTree,
            Request::AccessAction(42, true),
            Request::DesktopTheme(Some(ThemeMode::Dark), vec![(1, 0x101a26ff), (9, 0xf7a96aff)]),
            Request::DesktopTheme(None, Vec::new()),
            Request::Audio { frames: 512, channels: 2, rate: 48_000 },
        ];
        for r in all {
            assert_eq!(Request::decode(&r.encode()), Ok(r));
        }
    }

    /// The window may draw a repeatable frame again, but it must not *do* it
    /// again: the frames bound for the server, the clipboard hand-off, the
    /// IME rect and the atlas rows all answer something that happened once.
    #[test]
    fn a_repeated_frame_carries_the_list_and_nothing_that_happens_once() {
        let list = |gpu_only: bool| DrawList {
            quads: vec![Quad { rect: [1.0; 4], params: [8.0; 4], fill: [3.0; 4], stroke: [4.0; 4], uv: [5.0; 4], extra: [0.0; 4], spin: [2.0, 3.0, 0.0, 0.0], from: [0; 8] }],
            runs: vec![Run { clip: 0, chain: 0, first: 0, count: 1 }],
            clips: vec![[0, 0, 10, 10]],
            clear: [0.5; 4],
            wants_frame: true,
            gpu_only,
            repeat_until_ms: u32::MAX,
            serial: 7,
            cpu_bound: false,
            scrollers: Vec::new(),
            backdrop: None,
        };
        let status = Status { outbound: vec![vec![1, 2]], needs_redraw: true, clipboard: Some("copied".into()), ime: Some([1.0; 4]), next_due_ms: Some(16), ..Status::default() };
        let paint = |l: DrawList, st: Status| Reply { status: st, payload: Payload::Paint { list: Arc::new(l), glyphs: vec![(2, 1, 2, vec![0, 1])], images: Some((0, 1, vec![7; 8])) } };
        let t0 = Instant::now();

        let repeat = Repeat::of(&paint(list(true), status.clone()), t0).expect("a list the driver said may be drawn again");
        let again = repeat.answer(t0 + Duration::from_millis(5)).expect("and it is, for as long as nothing reaches the driver");
        let Payload::Paint { list: kept, glyphs, images } = &again.payload else { panic!("still a paint") };
        assert_eq!(**kept, list(true), "the list itself is what gets drawn again");
        assert!(glyphs.is_empty() && images.is_none(), "the atlas rows already landed");
        assert!(again.status.outbound.is_empty(), "the server must not be told twice");
        assert!(again.status.clipboard.is_none(), "nor the clipboard written twice");
        assert!(again.status.ime.is_none());
        assert!(again.status.needs_redraw, "but a redraw is still wanted");
        assert_eq!(again.status.next_due_ms, Some(16), "and still due when it was");
        assert!(repeat.answer(t0 + Duration::from_secs(3600)).is_some(), "an hour on, still the same list: nothing has reached the driver");

        assert!(Repeat::of(&paint(list(false), status.clone()), t0).is_none(), "anything else owed and the driver must be asked");
        let closed = Status { closed: Some("gone".into()), ..status.clone() };
        assert!(Repeat::of(&paint(list(true), closed), t0).is_none(), "a closed session is not repeated");
        assert!(Repeat::of(&Reply { status: Status::default(), payload: Payload::None }, t0).is_none(), "and only a paint is");

        // A tick is the clock, which the window keeps itself while it
        // repeats; anything else may change what the tree paints.
        assert!(Repeat::survives(&Request::Tick), "a tick does not evict the list");
        assert!(Repeat::survives(&Request::Paint(1, 1)));
        assert!(!Repeat::survives(&Request::Input(Input::PointerMove(1.0, 1.0))), "an input does");
        assert!(!Repeat::survives(&Request::Frame(vec![])), "so does a frame");

        // A list with a timer in it runs out: due no later than that, and
        // then the driver is asked.
        let timed = DrawList { repeat_until_ms: 100, ..list(true) };
        let repeat = Repeat::of(&paint(timed, status), t0).expect("repeatable until the timer");
        assert_eq!(repeat.until, Some(t0 + Duration::from_millis(100)));
        let late = repeat.answer(t0 + Duration::from_millis(90)).expect("still on");
        assert_eq!(late.status.next_due_ms, Some(10), "due when the list runs out, not a cadence later");
        assert!(repeat.answer(t0 + Duration::from_millis(100)).is_none(), "and then it is the driver's turn");
    }

    /// Every pattern of zero and non-zero parts survives the pipe, and a
    /// glyph's worth costs what it should.
    #[test]
    fn a_quad_crosses_the_pipe_sparsely_and_whole() {
        let full = Quad { rect: [1.0; 4], params: [2.0; 4], fill: [3.0; 4], stroke: [4.0; 4], uv: [5.0; 4], extra: [6.0; 4], spin: [7.0; 4], from: [1, 2, 3, 4, 5, 6, 7, 8] };
        for mask in 0u8..=255 {
            let mut q = full;
            let parts: [&mut [f32; 4]; 7] = [&mut q.rect, &mut q.params, &mut q.fill, &mut q.stroke, &mut q.uv, &mut q.extra, &mut q.spin];
            for (i, part) in parts.into_iter().enumerate() {
                if mask & (1 << i) == 0 {
                    *part = [0.0; 4];
                }
            }
            if mask & (1 << 7) == 0 {
                q.from = [0; 8];
            }
            let mut w = W(Vec::new());
            put_quad(&mut w, &q);
            let bytes = w.0;
            let mut r = R { b: &bytes, i: 0 };
            assert_eq!(get_quad(&mut r).unwrap(), q, "mask {mask:#b}");
            assert_eq!(bytes.len(), 1 + 16 * usize::from((mask & 127).count_ones() as u8) + if mask & 128 != 0 { 16 } else { 0 });
        }
        // A glyph: rect, params, fill, uv.
        let glyph = Quad { rect: [1.0; 4], params: [0.0, 0.0, 1.0, 1.0], fill: [1.0; 4], uv: [0.5; 4], ..Quad::default() };
        let mut w = W(Vec::new());
        put_quad(&mut w, &glyph);
        assert_eq!(w.0.len(), 65);
    }

    #[test]
    fn replies_round_trip() {
        let status = Status {
            outbound: vec![vec![1], vec![2, 3]],
            needs_redraw: true,
            closed: Some("x".into()),
            ime: Some([1.0, 2.0, 3.0, 4.0]),
            clipboard: Some("c".into()),
            next_due_ms: Some(16),
            cursor: 1,
            audio: true,
            video: false,
            files: vec![
                FileAsk { token: 3, node: 9, want: FileWant::Open { accept: "csv".into(), multiple: true, max: 1 << 20 } },
                FileAsk { token: 4, node: 10, want: FileWant::Save { name: "export.csv".into() } },
            ],
            writes: vec![FileWrite { token: 4, flag: eui_proto::Chunked::Last, bytes: vec![7, 7, 7] }],
        };
        let list = DrawList {
            quads: vec![Quad { rect: [1.0; 4], params: [2.0; 4], fill: [3.0; 4], stroke: [4.0; 4], uv: [5.0; 4], extra: [6.0; 4], spin: [0.0; 4], from: [1, 2, 3, 4, 5, 65535, 7, 8] }],
            runs: vec![Run { clip: 0, chain: 0, first: 0, count: 1 }],
            clips: vec![[0, 0, 10, 10]],
            clear: [0.5; 4],
            wants_frame: true,
            gpu_only: true,
            repeat_until_ms: 250,
            serial: 0x1234_5678_9abc,
            cpu_bound: false,
            scrollers: vec![Scroller { from: [0.0, -40.0], to: [0.0, 0.0], t0: -0.05, dur: 0.1, curve: 2, pad: 0 }],
            backdrop: None,
        };
        let snap = AccessSnapshot {
            nodes: vec![
                AccessNode {
                    id: 1,
                    role: AccessRole::Button,
                    bounds: [1.0, 2.0, 3.0, 4.0],
                    label: "Go".into(),
                    value: String::new(),
                    click: true,
                    focus: true,
                    children: vec![],
                    state: AccessState::default(),
                },
                AccessNode {
                    id: 0,
                    role: AccessRole::Window,
                    bounds: [0.0; 4],
                    label: "EUI".into(),
                    value: String::new(),
                    click: false,
                    focus: false,
                    children: vec![1],
                    state: AccessState::default(),
                },
            ],
            focus: 1,
            scale: 2.0,
        };
        let all = vec![
            Payload::None,
            Payload::Sandbox(Ok("ok".into())),
            Payload::Sandbox(Err("no".into())),
            Payload::Hello(vec![1, 2]),
            Payload::Assets(vec![[1; 32], [2; 32]]),
            Payload::Paint { list: Arc::new(list), glyphs: vec![(2, 1, 2, vec![0, 1]), (2, 0, 1, vec![3, 4])], images: Some((0, 1, vec![7; 8192])) },
            Payload::Tick(true),
            Payload::Access(snap),
            Payload::Pcm(vec![0.0, 0.25, -0.5, 1.0]),
        ];
        for payload in all {
            let reply = Reply { status: status.clone(), payload };
            assert_eq!(Reply::decode(&reply.encode()), Ok(reply));
        }
        assert!(Reply::decode(&[1, 2, 3]).is_err());
    }

    #[test]
    fn the_loop_answers_over_buffers() {
        // A worker over in-memory pipes: configure, hello, paint.
        let mut script = Vec::new();
        for r in [Request::Config { w: 100.0, h: 50.0, scale: 1.0, granted: 0 }, Request::Hello, Request::Paint(100, 50)] {
            let bytes = r.encode();
            script.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            script.extend_from_slice(&bytes);
        }
        let mut input = std::io::Cursor::new(script);
        let mut output = Vec::new();
        serve(&mut input, &mut output, Ok("test".into())).unwrap();
        let mut replies = Vec::new();
        let mut cursor = std::io::Cursor::new(output);
        while let Ok(b) = read_message(&mut cursor, MAX_REPLY) {
            replies.push(Reply::decode(&b).unwrap());
        }
        assert_eq!(replies.len(), 3);
        assert_eq!(replies[0].payload, Payload::Sandbox(Ok("test".into())));
        assert!(matches!(&replies[1].payload, Payload::Hello(b) if Frame::decode(b).is_ok()));
        assert!(matches!(&replies[2].payload, Payload::Paint { .. }));
    }
}
