//! # EUI video
//!
//! Two things, both pure: turning the bytes of a moving picture into
//! frames, and knowing which frame is due at a given moment.
//!
//! There is no device here, no thread and no clock. Decoding runs on
//! bytes a server chose, so it belongs where every other decoder belongs —
//! the sandboxed worker (spec 08 §10) — while uploading a frame to a
//! texture is the window's business, as the GPU is.
//!
//! ## What it decodes, and why so little
//!
//! GIF and animated WebP. Both are patent-free and decode in pure Rust,
//! which is the whole argument: a video decoder is the most attacked
//! surface a browser has, and this one has no C library under it, no
//! assembly, and nothing it can call. H.264 needs a patent licence, and
//! AV1 needs either a large C library or a large Rust one; a client that
//! promises a 12 MB binary and a small attack surface does not get to
//! link one casually. The node kind ([`eui_proto::NodeKind::Video`]) does
//! not care which decoder produced the frames, so a real codec can be
//! added later without changing an application.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// Frame arithmetic on values the decoder already bounded.
#![allow(clippy::arithmetic_side_effects)]

use std::io::Cursor;

/// Why a moving picture could not be decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoError {
    /// Not a format this client decodes.
    Unsupported(String),
    /// The stream is malformed.
    Malformed(String),
    /// A frame is larger than [`MAX_PIXELS`], or there are more frames
    /// than [`MAX_FRAMES`], or the whole picture exceeds [`MAX_BYTES`].
    TooLarge,
    /// The stream holds no frame at all.
    Empty,
}

impl std::fmt::Display for VideoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(w) => write!(f, "unsupported video: {w}"),
            Self::Malformed(w) => write!(f, "malformed video: {w}"),
            Self::TooLarge => write!(f, "the picture is larger than the limits allow"),
            Self::Empty => write!(f, "the stream holds no frame"),
        }
    }
}

impl std::error::Error for VideoError {}

/// Pixels one frame may hold: 1920 × 1080. A window is not a cinema.
pub const MAX_PIXELS: usize = 1920 * 1080;

/// Frames one picture may hold: a minute at 60 a second.
pub const MAX_FRAMES: usize = 3_600;

/// Bytes of decoded frames one picture may hold. Frames are kept whole
/// and uncompressed — that is what makes playing them cost nothing — so
/// this is the real bound, and it is deliberately modest: a short loop,
/// not a film.
pub const MAX_BYTES: usize = 96 * 1024 * 1024;

/// The shortest a frame may last, so a picture claiming zero cannot spin
/// the client. Browsers do the same thing for the same reason.
pub const MIN_DELAY_MS: u32 = 20;

/// One frame: RGBA, top row first, `width * height * 4` bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    /// Pixels, `RGBA`.
    pub rgba: Vec<u8>,
    /// How long this frame is shown.
    pub delay_ms: u32,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame").field("bytes", &self.rgba.len()).field("delay_ms", &self.delay_ms).finish()
    }
}

/// A decoded moving picture: every frame, composed and ready to upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Movie {
    width: u32,
    height: u32,
    frames: Vec<Frame>,
}

impl Movie {
    /// A picture from frames already composed, for tests and generators.
    /// `None` when a frame is not `width * height * 4` bytes, or there are
    /// no frames.
    pub fn new(width: u32, height: u32, frames: Vec<Frame>) -> Option<Self> {
        let want = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
        if frames.is_empty() || want == 0 || frames.iter().any(|f| f.rgba.len() != want) {
            return None;
        }
        Some(Self { width, height, frames })
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Frames.
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// How long the picture runs, once.
    pub fn duration_ms(&self) -> u64 {
        self.frames.iter().map(|f| u64::from(f.delay_ms)).sum()
    }

    /// Bytes the decoded frames occupy, for the session's quota.
    pub fn bytes(&self) -> usize {
        self.frames.iter().map(|f| f.rgba.len()).sum()
    }

    /// The frame shown at `ms` from the start, and its index. The last
    /// frame answers for anything past the end.
    pub fn at(&self, ms: u64) -> (usize, &Frame) {
        let mut acc = 0u64;
        for (i, f) in self.frames.iter().enumerate() {
            acc += u64::from(f.delay_ms);
            if ms < acc {
                return (i, f);
            }
        }
        let last = self.frames.len().saturating_sub(1);
        (last, self.frames.get(last).unwrap_or_else(|| unreachable(&self.frames)))
    }
}

/// `Movie` never holds an empty frame list, so this is unreachable; it
/// exists so the lookup can be total without an `unwrap`.
fn unreachable(frames: &[Frame]) -> &Frame {
    frames.first().unwrap_or(const { &Frame { rgba: Vec::new(), delay_ms: MIN_DELAY_MS } })
}

/// Decode a moving picture. `hint` is a file extension or MIME type when
/// the caller has one; the bytes are sniffed either way.
pub fn decode(bytes: &[u8], hint: Option<&str>) -> Result<Movie, VideoError> {
    let hint = hint.map(|h| h.rsplit('/').next().unwrap_or(h).trim_start_matches('.').to_ascii_lowercase());
    let looks_webp = bytes.len() >= 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP");
    let looks_gif = bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a");
    match (looks_gif, looks_webp, hint.as_deref()) {
        (true, _, _) | (_, _, Some("gif")) => decode_gif(bytes),
        (_, true, _) | (_, _, Some("webp")) => decode_webp(bytes),
        _ => Err(VideoError::Unsupported("not a GIF or a WebP".into())),
    }
}

/// Add `n` bytes to the budget, or refuse.
fn budget(used: &mut usize, n: usize) -> Result<(), VideoError> {
    *used = used.checked_add(n).ok_or(VideoError::TooLarge)?;
    if *used > MAX_BYTES {
        return Err(VideoError::TooLarge);
    }
    Ok(())
}

fn check_size(width: u32, height: u32) -> Result<usize, VideoError> {
    let pixels = (width as usize).checked_mul(height as usize).ok_or(VideoError::TooLarge)?;
    if pixels == 0 || pixels > MAX_PIXELS {
        return Err(VideoError::TooLarge);
    }
    pixels.checked_mul(4).ok_or(VideoError::TooLarge)
}

/// GIF, composed frame by frame: the format sends patches, with a
/// disposal method saying what to do with the last one before drawing the
/// next. A decoder that ignores that shows garbage on half the GIFs in
/// the world, so this one does not.
fn decode_gif(bytes: &[u8]) -> Result<Movie, VideoError> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    options.check_frame_consistency(true);
    let mut decoder = options.read_info(Cursor::new(bytes)).map_err(|e| VideoError::Malformed(e.to_string()))?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    let stride = check_size(width, height)?;
    let mut canvas = vec![0u8; stride];
    let mut frames: Vec<Frame> = Vec::new();
    let mut used = 0usize;
    loop {
        let frame = match decoder.read_next_frame() {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                if frames.is_empty() {
                    return Err(VideoError::Malformed(e.to_string()));
                }
                break;
            }
        };
        if frames.len() >= MAX_FRAMES {
            return Err(VideoError::TooLarge);
        }
        // What to restore after this frame, decided before drawing it.
        let saved = match frame.dispose {
            gif::DisposalMethod::Previous => Some(canvas.clone()),
            _ => None,
        };
        let (fx, fy, fw, fh) = (u32::from(frame.left), u32::from(frame.top), u32::from(frame.width), u32::from(frame.height));
        for row in 0..fh {
            let y = fy + row;
            if y >= height {
                break;
            }
            for col in 0..fw {
                let x = fx + col;
                if x >= width {
                    break;
                }
                let src = ((row * fw + col) * 4) as usize;
                let dst = ((y * width + x) * 4) as usize;
                let (Some(px), Some(slot)) = (frame.buffer.get(src..src + 4), canvas.get_mut(dst..dst + 4)) else { continue };
                // A transparent pixel leaves what is underneath.
                if px.get(3).copied().unwrap_or(0) == 0 {
                    continue;
                }
                slot.copy_from_slice(px);
            }
        }
        budget(&mut used, stride)?;
        // `delay` is in hundredths of a second, and zero means "as fast as
        // you can", which is not a thing this client does.
        let delay_ms = u32::from(frame.delay).saturating_mul(10).max(MIN_DELAY_MS);
        frames.push(Frame { rgba: canvas.clone(), delay_ms });
        match frame.dispose {
            gif::DisposalMethod::Background => {
                for row in 0..fh {
                    let y = fy + row;
                    if y >= height {
                        break;
                    }
                    let start = ((y * width + fx.min(width)) * 4) as usize;
                    let end = (start + (fw.min(width - fx.min(width)) * 4) as usize).min(canvas.len());
                    if let Some(slice) = canvas.get_mut(start..end) {
                        slice.fill(0);
                    }
                }
            }
            gif::DisposalMethod::Previous => {
                if let Some(prev) = saved {
                    canvas = prev;
                }
            }
            _ => {}
        }
    }
    Movie::new(width, height, frames).ok_or(VideoError::Empty)
}

/// Animated WebP. The decoder composes the frames; a still WebP is a
/// picture of one frame, which plays as a picture should.
fn decode_webp(bytes: &[u8]) -> Result<Movie, VideoError> {
    let mut decoder = image_webp::WebPDecoder::new(Cursor::new(bytes)).map_err(|e| VideoError::Unsupported(e.to_string()))?;
    let (width, height) = decoder.dimensions();
    let stride = check_size(width, height)?;
    let count = decoder.num_frames().max(1) as usize;
    if count > MAX_FRAMES {
        return Err(VideoError::TooLarge);
    }
    let mut frames = Vec::with_capacity(count.min(64));
    let mut used = 0usize;
    if decoder.is_animated() {
        for _ in 0..count {
            budget(&mut used, stride)?;
            let mut rgba = vec![0u8; stride];
            match decoder.read_frame(&mut rgba) {
                Ok(delay) => frames.push(Frame { rgba, delay_ms: delay.max(MIN_DELAY_MS) }),
                Err(e) => {
                    if frames.is_empty() {
                        return Err(VideoError::Malformed(e.to_string()));
                    }
                    break;
                }
            }
        }
    } else {
        budget(&mut used, stride)?;
        let mut rgba = vec![0u8; stride];
        // A still picture may be RGB; the buffer size the decoder asks for
        // says which, and a still frame lasts as long as anyone waits.
        if decoder.output_buffer_size() == Some(stride) {
            decoder.read_image(&mut rgba).map_err(|e| VideoError::Malformed(e.to_string()))?;
        } else {
            let mut rgb = vec![0u8; (width as usize) * (height as usize) * 3];
            decoder.read_image(&mut rgb).map_err(|e| VideoError::Malformed(e.to_string()))?;
            for (out, px) in rgba.chunks_mut(4).zip(rgb.chunks(3)) {
                let Some(colour) = out.get_mut(..3) else { continue };
                colour.copy_from_slice(px);
                if let Some(alpha) = out.get_mut(3) {
                    *alpha = 255;
                }
            }
        }
        frames.push(Frame { rgba, delay_ms: u32::MAX / 2 });
    }
    Movie::new(width, height, frames).ok_or(VideoError::Empty)
}

/// Where a picture is: what it should be doing, and which frame that
/// makes due. The clock is the caller's — the driver's — so a player is
/// as testable as arithmetic.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Player {
    /// Playing, or held on the frame it is on.
    pub playing: bool,
    /// Start again at the end rather than stopping on the last frame.
    pub looping: bool,
    /// Where it is, in milliseconds from the start.
    at_ms: u64,
    /// The frame that position lands on.
    index: usize,
    /// It reached the end since the last [`Self::take_ended`].
    ended: bool,
}

impl Player {
    /// A player at the start of `movie`, not playing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Where it is.
    pub fn position_ms(&self) -> u64 {
        self.at_ms
    }

    /// The frame it is on.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Move to `ms` from the start, clamped to the picture's length.
    pub fn seek(&mut self, movie: &Movie, ms: u64) {
        self.at_ms = ms.min(movie.duration_ms());
        self.index = movie.at(self.at_ms).0;
        self.ended = false;
    }

    /// Advance by `elapsed_ms` of wall clock. Returns `true` when the
    /// frame on screen must change — the only reason to upload anything.
    pub fn advance(&mut self, movie: &Movie, elapsed_ms: u64) -> bool {
        if !self.playing || movie.frames().is_empty() {
            return false;
        }
        let length = movie.duration_ms().max(1);
        let was = self.index;
        self.at_ms = self.at_ms.saturating_add(elapsed_ms);
        if self.at_ms >= length {
            if self.looping {
                self.at_ms %= length;
            } else {
                self.at_ms = length;
                self.playing = false;
                self.ended = true;
            }
        }
        self.index = movie.at(self.at_ms).0;
        self.index != was
    }

    /// True once when the picture reached its end.
    pub fn take_ended(&mut self) -> bool {
        std::mem::take(&mut self.ended)
    }

    /// How long until the frame changes, so the window can sleep exactly
    /// that long instead of polling. `None` when nothing is playing.
    pub fn next_frame_in_ms(&self, movie: &Movie) -> Option<u64> {
        if !self.playing {
            return None;
        }
        let mut acc = 0u64;
        for f in movie.frames() {
            acc += u64::from(f.delay_ms);
            if self.at_ms < acc {
                return Some(acc - self.at_ms);
            }
        }
        Some(0)
    }
}
