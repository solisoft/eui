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
use std::time::{Duration, Instant};

use eui_proto::{Frame, ThemeMode};
use eui_render::{Atlas, DrawList, ImageAtlas, Quad};

use crate::a11y::{AccessNode, AccessRole, AccessSnapshot};
use crate::assets::Hash;
use crate::driver::{Driver, Input};

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
        Ok(String::from_utf8(self.bytes()?.to_vec()).map_err(|_| "utf-8")?)
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
    /// since the last paint — `(edge length, y0, y1, bytes)` for the glyph
    /// atlas, which grows, `(y0, y1, bytes)` for the image atlas.
    Paint {
        /// The frame.
        list: DrawList,
        /// Coverage rows.
        glyphs: Option<(u32, u32, u32, Vec<u8>)>,
        /// RGBA rows.
        images: Option<(u32, u32, Vec<u8>)>,
    },
    /// `Tick`: a transition frame is due.
    Tick(bool),
    /// `AccessTree`.
    Access(AccessSnapshot),
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
                match glyphs {
                    Some((size, y0, y1, px)) => {
                        w.bool(true);
                        w.u32(*size);
                        w.u32(*y0);
                        w.u32(*y1);
                        w.bytes(px);
                    }
                    None => w.bool(false),
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
        let status = Status { outbound, needs_redraw, closed, ime, clipboard, next_due_ms, cursor };
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
                let glyphs = if r.bool()? { Some((r.u32()?, r.u32()?, r.u32()?, r.bytes()?.to_vec())) } else { None };
                let images = if r.bool()? { Some((r.u32()?, r.u32()?, r.bytes()?.to_vec())) } else { None };
                Payload::Paint { list, glyphs, images }
            }
            5 => Payload::Tick(r.bool()?),
            6 => Payload::Access(get_access(&mut r)?),
            _ => return Err("unknown payload"),
        };
        r.done()?;
        Ok(Reply { status, payload })
    }
}

fn put_list(w: &mut W, list: &DrawList) {
    w.u32(u32::try_from(list.quads.len()).unwrap_or(u32::MAX));
    for q in &list.quads {
        w.f4(q.rect);
        w.f4(q.params);
        w.f4(q.fill);
        w.f4(q.stroke);
        w.f4(q.uv);
        w.f4(q.extra);
    }
    w.u32(u32::try_from(list.runs.len()).unwrap_or(u32::MAX));
    for (c, a, b) in &list.runs {
        w.u32(*c);
        w.u32(*a);
        w.u32(*b);
    }
    w.u32(u32::try_from(list.clips.len()).unwrap_or(u32::MAX));
    for c in &list.clips {
        for v in c {
            w.u32(*v);
        }
    }
    w.f4(list.clear);
    w.bool(list.wants_frame);
}

fn get_list(r: &mut R<'_>) -> Wire<DrawList> {
    let n = r.u32()? as usize;
    let mut quads = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        quads.push(Quad { rect: r.f4()?, params: r.f4()?, fill: r.f4()?, stroke: r.f4()?, uv: r.f4()?, extra: r.f4()? });
    }
    let n = r.u32()? as usize;
    let mut runs = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        runs.push((r.u32()?, r.u32()?, r.u32()?));
    }
    let n = r.u32()? as usize;
    let mut clips = Vec::with_capacity(n.min(1 << 16));
    for _ in 0..n {
        clips.push([r.u32()?, r.u32()?, r.u32()?, r.u32()?]);
    }
    let clear = r.f4()?;
    let wants_frame = r.bool()?;
    Ok(DrawList { quads, runs, clips, clear, wants_frame })
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
    }
    w.u64(s.focus);
    w.f32(s.scale);
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
        nodes.push(AccessNode { id, role, bounds, label, value, click: actions & 1 != 0, focus: actions & 2 != 0, children });
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
                        let list = d.paint(w, h);
                        let (atlas, images) = d.atlases_mut();
                        let glyphs = atlas.dirty_rows().map(|(y0, y1)| {
                            atlas.mark_clean();
                            (atlas.size(), y0, y1, atlas.rows(y0, y1).to_vec())
                        });
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
    atlas: Atlas,
    images: ImageAtlas,
    dead: Option<String>,
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
        let mut worker = Self { child, input: BufWriter::new(input), output: BufReader::new(output), status: Status::default(), due: None, atlas: Atlas::new(), images: ImageAtlas::new(), dead: None };
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
        let result = write_message(&mut self.input, &request.encode()).and_then(|()| read_message(&mut self.output, MAX_REPLY)).and_then(|b| Reply::decode(&b).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)));
        match result {
            Ok(reply) => {
                self.status = reply.status.clone();
                self.due = Some(Instant::now());
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

    /// State after the last reply.
    pub fn status(&self) -> &Status {
        &self.status
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
#[derive(Debug)]
pub enum Backend {
    /// In this process.
    Local(Driver),
    /// In a worker process.
    Remote(Worker),
}

impl Backend {
    /// A driver in a worker process when one can be started, else in this
    /// process; the string says which and, for a worker, what its sandbox
    /// enforced. `EUI_SANDBOX=0` asks for this process outright.
    pub fn open(w: f32, h: f32, scale: f32, granted: u32) -> (Self, String) {
        if std::env::var("EUI_SANDBOX").is_ok_and(|v| v == "0") {
            return (Backend::Local(Driver::new(w, h, scale, granted)), "driver in this process (EUI_SANDBOX=0)".into());
        }
        let program = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => return (Backend::Local(Driver::new(w, h, scale, granted)), format!("driver in this process: cannot find this binary ({e})")),
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
                (Backend::Remote(worker), how)
            }
            Err(e) => (Backend::Local(Driver::new(w, h, scale, granted)), format!("driver in this process: {e}")),
        }
    }

    /// Wrap a driver of this process.
    pub fn local(driver: Driver) -> Self {
        Backend::Local(driver)
    }

    fn remote_status(&self) -> Option<&Status> {
        match self {
            Backend::Remote(w) => Some(w.status()),
            Backend::Local(_) => None,
        }
    }

    /// Change the grant before the session opens.
    pub fn grant(&mut self, granted: u32) {
        match self {
            Backend::Local(d) => d.grant(granted),
            Backend::Remote(w) => {
                w.call(&Request::Grant(granted));
            }
        }
    }

    /// The opening frame, encoded.
    pub fn hello(&mut self) -> Vec<u8> {
        match self {
            Backend::Local(d) => d.hello().encode(),
            Backend::Remote(w) => match w.call(&Request::Hello).map(|r| r.payload) {
                Some(Payload::Hello(b)) => b,
                _ => Vec::new(),
            },
        }
    }

    /// A frame from the server, as received. Returns encoded frames to send
    /// back.
    pub fn frame(&mut self, bytes: Vec<u8>) -> Vec<Vec<u8>> {
        match self {
            Backend::Local(d) => match Frame::decode(&bytes) {
                Ok(f) => d.handle_frame(f).iter().map(Frame::encode).collect(),
                Err(e) => {
                    d.close(format!("bad frame: {e}"));
                    Vec::new()
                }
            },
            Backend::Remote(w) => w.call(&Request::Frame(bytes)).map(|r| r.status.outbound).unwrap_or_default(),
        }
    }

    /// Something the viewer did. Returns encoded frames to send.
    pub fn input(&mut self, input: Input) -> Vec<Vec<u8>> {
        match self {
            Backend::Local(d) => d.input(input).iter().map(Frame::encode).collect(),
            Backend::Remote(w) => w.call(&Request::Input(input)).map(|r| r.status.outbound).unwrap_or_default(),
        }
    }

    /// Verified bytes for a hash.
    pub fn asset_ready(&mut self, hash: Hash, bytes: Vec<u8>) {
        match self {
            Backend::Local(d) => d.asset_ready(hash, bytes),
            Backend::Remote(w) => {
                w.call(&Request::AssetReady(hash, bytes));
            }
        }
    }

    /// A hash that could not be fetched.
    pub fn asset_failed(&mut self, hash: Hash, why: String) {
        match self {
            Backend::Local(d) => d.asset_failed(hash, why),
            Backend::Remote(w) => {
                w.call(&Request::AssetFailed(hash, why));
            }
        }
    }

    /// Hashes the tree needs and nobody fetched yet.
    pub fn pending_assets(&mut self) -> Vec<Hash> {
        match self {
            Backend::Local(d) => d.pending_assets(),
            Backend::Remote(w) => match w.call(&Request::PendingAssets).map(|r| r.payload) {
                Some(Payload::Assets(hs)) => hs,
                _ => Vec::new(),
            },
        }
    }

    /// Lay out and paint for a `w × h` device-pixel target. Returns the
    /// draw list and the encoded frames a scroll landing during the paint
    /// emitted, to send after drawing.
    pub fn paint(&mut self, w: u32, h: u32) -> (DrawList, Vec<Vec<u8>>) {
        match self {
            Backend::Local(d) => {
                let list = d.paint(w, h);
                (list, d.take_pending().iter().map(Frame::encode).collect())
            }
            Backend::Remote(worker) => match worker.call(&Request::Paint(w, h)) {
                Some(Reply { status, payload: Payload::Paint { list, glyphs, images } }) => {
                    if let Some((size, y0, y1, px)) = glyphs {
                        if !worker.atlas.set_rows(size, y0, y1, &px) {
                            eprintln!("eui: the worker sent glyph atlas rows of the wrong size");
                        }
                    }
                    if let Some((y0, y1, px)) = images {
                        if !worker.images.set_rows(y0, y1, &px) {
                            eprintln!("eui: the worker sent image atlas rows of the wrong size");
                        }
                    }
                    (list, status.outbound)
                }
                _ => (DrawList::default(), Vec::new()),
            },
        }
    }

    /// The atlases the last draw list refers to, for the renderer's upload.
    pub fn atlases_mut(&mut self) -> (&mut Atlas, &mut ImageAtlas) {
        match self {
            Backend::Local(d) => d.atlases_mut(),
            Backend::Remote(w) => (&mut w.atlas, &mut w.images),
        }
    }

    /// Advance the clock; true when a transition frame is due.
    pub fn tick(&mut self, now: Instant) -> bool {
        match self {
            Backend::Local(d) => d.tick(now),
            Backend::Remote(w) => {
                // Nothing can be due before the worker said something would
                // be: skip the round trip at rest.
                if w.status.next_due_ms.is_none() {
                    return false;
                }
                matches!(w.call(&Request::Tick).map(|r| r.payload), Some(Payload::Tick(true)))
            }
        }
    }

    /// When the next transition frame is due, `None` at rest.
    pub fn next_frame_at(&self) -> Option<Instant> {
        match self {
            Backend::Local(d) => d.next_frame_at(),
            Backend::Remote(w) => {
                let ms = w.status.next_due_ms?;
                Some(w.due.unwrap_or_else(Instant::now) + Duration::from_millis(u64::from(ms)))
            }
        }
    }

    /// A frame should be drawn.
    pub fn needs_redraw(&self) -> bool {
        match self {
            Backend::Local(d) => d.needs_redraw(),
            Backend::Remote(w) => w.status.needs_redraw,
        }
    }

    /// Why the session ended, if it did.
    pub fn closed(&self) -> Option<String> {
        match self {
            Backend::Local(d) => d.closed().map(|c| format!("{c:?}")),
            Backend::Remote(w) => w.status.closed.clone(),
        }
    }

    /// Where an input method's candidate window goes: `x, y, w, h` in
    /// logical px, if a field has focus.
    pub fn ime_area(&self) -> Option<[f32; 4]> {
        match self {
            Backend::Local(d) => d.ime_area().map(|r| [r.x, r.y, r.w, r.h]),
            Backend::Remote(w) => w.status.ime,
        }
    }

    /// The pointer's shape over what it is on.
    pub fn cursor(&self) -> eui_proto::Cursor {
        match self {
            Backend::Local(d) => d.cursor(),
            Backend::Remote(w) => eui_proto::Cursor::from_u8(w.status.cursor).unwrap_or(eui_proto::Cursor::Default),
        }
    }

    /// Text the viewer copied since the last call.
    pub fn take_clipboard(&mut self) -> Option<String> {
        match self {
            Backend::Local(d) => d.take_clipboard(),
            Backend::Remote(w) => w.status.clipboard.take(),
        }
    }

    /// The viewer's desktop palette (05 §5), or none. Returns encoded
    /// frames to send: the viewport, when the palette's mode differs.
    pub fn desktop_theme(&mut self, mode: Option<ThemeMode>, colors: Vec<(eui_theme::Role, u32)>) -> Vec<Vec<u8>> {
        match self {
            Backend::Local(d) => d.set_desktop_theme(mode, colors).iter().map(Frame::encode).collect(),
            Backend::Remote(w) => {
                let wire = colors.iter().map(|(r, c)| (r.id(), *c)).collect();
                w.call(&Request::DesktopTheme(mode, wire)).map(|r| r.status.outbound).unwrap_or_default()
            }
        }
    }

    /// The accessibility tree as painted.
    pub fn access_tree(&mut self) -> AccessSnapshot {
        match self {
            Backend::Local(d) => d.access_snapshot(),
            Backend::Remote(w) => match w.call(&Request::AccessTree).map(|r| r.payload) {
                Some(Payload::Access(s)) => s,
                _ => AccessSnapshot { nodes: Vec::new(), focus: 0, scale: 1.0 },
            },
        }
    }

    /// An assistive technology's action on a node: `click`, or focus.
    /// Returns encoded frames to send.
    pub fn access_action(&mut self, id: u64, click: bool) -> Vec<Vec<u8>> {
        match self {
            Backend::Local(d) => {
                let Some(ix) = d.node_for_accessibility(id) else { return Vec::new() };
                let out = if click { d.activate_node(ix) } else { d.focus_node(ix) };
                out.iter().map(Frame::encode).collect()
            }
            Backend::Remote(w) => w.call(&Request::AccessAction(id, click)).map(|r| r.status.outbound).unwrap_or_default(),
        }
    }

    /// The driver, when it is in this process.
    pub fn driver(&self) -> Option<&Driver> {
        match self {
            Backend::Local(d) => Some(d),
            Backend::Remote(_) => None,
        }
    }

    /// What the last reply said, when the driver is in a worker.
    pub fn worker_status(&self) -> Option<&Status> {
        self.remote_status()
    }
}

#[cfg(test)]
mod tests {
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
        ];
        for r in all {
            assert_eq!(Request::decode(&r.encode()), Ok(r));
        }
    }

    #[test]
    fn replies_round_trip() {
        let status = Status { outbound: vec![vec![1], vec![2, 3]], needs_redraw: true, closed: Some("x".into()), ime: Some([1.0, 2.0, 3.0, 4.0]), clipboard: Some("c".into()), next_due_ms: Some(16), cursor: 1 };
        let list = DrawList { quads: vec![Quad { rect: [1.0; 4], params: [2.0; 4], fill: [3.0; 4], stroke: [4.0; 4], uv: [5.0; 4], extra: [6.0; 4] }], runs: vec![(0, 0, 1)], clips: vec![[0, 0, 10, 10]], clear: [0.5; 4], wants_frame: true };
        let snap = AccessSnapshot { nodes: vec![AccessNode { id: 1, role: AccessRole::Button, bounds: [1.0, 2.0, 3.0, 4.0], label: "Go".into(), value: String::new(), click: true, focus: true, children: vec![] }, AccessNode { id: 0, role: AccessRole::Window, bounds: [0.0; 4], label: "EUI".into(), value: String::new(), click: false, focus: false, children: vec![1] }], focus: 1, scale: 2.0 };
        let all = vec![
            Payload::None,
            Payload::Sandbox(Ok("ok".into())),
            Payload::Sandbox(Err("no".into())),
            Payload::Hello(vec![1, 2]),
            Payload::Assets(vec![[1; 32], [2; 32]]),
            Payload::Paint { list, glyphs: Some((2, 1, 2, vec![0, 1])), images: Some((0, 1, vec![7; 8192])) },
            Payload::Tick(true),
            Payload::Access(snap),
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
