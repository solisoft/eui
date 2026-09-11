//! Content-addressed assets: fetching, verifying, decoding, holding.
//!
//! An asset is named by the BLAKE3 hash of its bytes. The fetch is a single
//! HTTP/1.1 `GET` over TLS to the session's origin — hand-written rather than
//! pulled in as a client library, because the only server it ever talks to
//! is ours, the reply is one `Content-Length` body, and every byte of that
//! parser is ours to bound. The hash is checked before anything is decoded,
//! so a substituted body is discarded, not displayed.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A BLAKE3 hash.
pub type Hash = [u8; 32];

/// Largest asset accepted, in bytes.
pub const MAX_ASSET_BYTES: usize = 16 * 1024 * 1024;
/// Largest image edge decoded, in pixels.
pub const MAX_IMAGE_EDGE: u32 = 4096;

/// Why a fetch or decode failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetError {
    /// The session URL could not be turned into an origin.
    Origin(String),
    /// TCP or TLS failed.
    Connect(String),
    /// The server did not answer `200` with a `Content-Length`.
    Http(String),
    /// The body exceeded [`MAX_ASSET_BYTES`].
    TooLarge,
    /// The body's hash is not the name it was fetched by.
    HashMismatch,
    /// The bytes are not a decodable image.
    Decode(String),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Origin(e) => write!(f, "bad origin: {e}"),
            Self::Connect(e) => write!(f, "connect: {e}"),
            Self::Http(e) => write!(f, "http: {e}"),
            Self::TooLarge => f.write_str("asset too large"),
            Self::HashMismatch => f.write_str("asset bytes do not match their hash"),
            Self::Decode(e) => write!(f, "decode: {e}"),
        }
    }
}

impl std::error::Error for AssetError {}

/// `hash` as lowercase hex.
pub fn hex(hash: &Hash) -> String {
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// The HTTPS origin an asset is fetched from, derived from the session URL:
/// `wss://host/_eui/session/x` → `https://host`. A `ws://` session (debug
/// loopback only) fetches over `http://` from the same host.
pub fn origin_for(session_url: &str) -> Result<String, AssetError> {
    let (scheme, rest) = session_url.split_once("://").ok_or_else(|| AssetError::Origin("no scheme".into()))?;
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty() {
        return Err(AssetError::Origin("no host".into()));
    }
    let http = match scheme {
        "wss" => "https",
        "ws" => "http",
        other => return Err(AssetError::Origin(format!("unsupported scheme {other}"))),
    };
    Ok(format!("{http}://{host}"))
}

/// Fetch and verify one asset. Blocking; runs its own small runtime, so call
/// it from a worker thread.
pub fn fetch(origin: &str, hash: &Hash, cookie: Option<&str>) -> Result<Vec<u8>, AssetError> {
    let bytes = get(origin, &format!("/_eui/asset/{}", hex(hash)), "application/octet-stream", cookie)?;
    if *blake3::hash(&bytes).as_bytes() != *hash {
        return Err(AssetError::HashMismatch);
    }
    Ok(bytes)
}

/// One strict HTTPS `GET` of `path` at `origin`: no cookie, no redirect, a
/// `Content-Length` body no larger than an asset. Blocking. The manifest and
/// every asset come through here and nothing else does.
pub fn get(origin: &str, path: &str, accept: &str, cookie: Option<&str>) -> Result<Vec<u8>, AssetError> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| AssetError::Connect(e.to_string()))?;
    rt.block_on(get_async(origin, path, accept, cookie))
}

async fn get_async(origin: &str, path: &str, accept: &str, cookie: Option<&str>) -> Result<Vec<u8>, AssetError> {
    let (scheme, hostport) = origin.split_once("://").ok_or_else(|| AssetError::Origin("no scheme".into()))?;
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.contains(']') || h.ends_with(']') => (h.trim_matches(|c| c == '[' || c == ']'), p.parse::<u16>().map_err(|_| AssetError::Origin("bad port".into()))?),
        _ => (hostport, if scheme == "https" { 443 } else { 80 }),
    };
    // The caller's cookie, not a process-wide one: two sessions in one
    // process must not present each other's.
    let cookie = cookie.map_or(String::new(), |c| format!("Cookie: {c}\r\n"));
    let request = format!("GET {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\nAccept: {accept}\r\n{cookie}\r\n");

    let tcp = tokio::net::TcpStream::connect((host, port)).await.map_err(|e| AssetError::Connect(e.to_string()))?;
    let mut raw = Vec::new();
    if scheme == "https" {
        // The same roots and the same TLS 1.3 as the session socket: the
        // manifest, the assets and the session are one origin's, and they
        // cannot be trusted differently.
        let connector = tokio_rustls::TlsConnector::from(crate::transport::tls_config());
        let name = rustls::pki_types::ServerName::try_from(host.to_string()).map_err(|_| AssetError::Origin("bad host name".into()))?;
        let mut tls = connector.connect(name, tcp).await.map_err(|e| AssetError::Connect(e.to_string()))?;
        tls.write_all(request.as_bytes()).await.map_err(|e| AssetError::Connect(e.to_string()))?;
        read_capped(&mut tls, &mut raw).await?;
    } else {
        let mut tcp = tcp;
        tcp.write_all(request.as_bytes()).await.map_err(|e| AssetError::Connect(e.to_string()))?;
        read_capped(&mut tcp, &mut raw).await?;
    }
    parse_response(&raw)
}

async fn read_capped<S: AsyncReadExt + Unpin>(s: &mut S, out: &mut Vec<u8>) -> Result<(), AssetError> {
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = s.read(&mut buf).await.map_err(|e| AssetError::Connect(e.to_string()))?;
        if n == 0 {
            return Ok(());
        }
        if out.len().saturating_add(n) > MAX_ASSET_BYTES.saturating_add(4096) {
            return Err(AssetError::TooLarge);
        }
        out.extend_from_slice(buf.get(..n).unwrap_or(&[]));
    }
}

/// The smallest HTTP/1.1 response reader that is still strict: status 200,
/// a `Content-Length`, exactly that many body bytes.
fn parse_response(raw: &[u8]) -> Result<Vec<u8>, AssetError> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| AssetError::Http("no header terminator".into()))?;
    let head = std::str::from_utf8(raw.get(..split).unwrap_or(&[])).map_err(|_| AssetError::Http("non-UTF-8 headers".into()))?;
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or("");
    if !status.starts_with("HTTP/1.1 200") && !status.starts_with("HTTP/1.0 200") {
        return Err(AssetError::Http(status.to_string()));
    }
    let mut length: Option<usize> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                length = Some(v.trim().parse().map_err(|_| AssetError::Http("bad content-length".into()))?);
            }
            if k.eq_ignore_ascii_case("transfer-encoding") {
                return Err(AssetError::Http("chunked bodies are not accepted".into()));
            }
        }
    }
    let length = length.ok_or_else(|| AssetError::Http("no content-length".into()))?;
    if length > MAX_ASSET_BYTES {
        return Err(AssetError::TooLarge);
    }
    let body = raw.get(split.saturating_add(4)..).unwrap_or(&[]);
    if body.len() != length {
        return Err(AssetError::Http(format!("body is {} bytes, header says {length}", body.len())));
    }
    Ok(body.to_vec())
}

/// A decoded image, RGBA8, row-major, straight (non-premultiplied) alpha.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width × height × 4` bytes.
    pub rgba: Vec<u8>,
}

/// The longest edge a picture may have once it is packed, in texels.
///
/// The sheet is 2048 across (`ImageAtlas::SIZE`) and nothing is ever
/// evicted from it, so a picture packed at the sheet's own edge would take
/// the whole arena and every picture after it would be refused. Half the
/// edge leaves room for four such pictures side by side, and 1024 texels is
/// 512 logical pixels on a 2x display — wider than anything the catalogue
/// draws inline.
pub const ATLAS_EDGE: u32 = 1024;

/// A copy of `img` small enough for the atlas, or `None` when it already
/// fits and nothing needs copying.
///
/// A picture with an edge past the sheet's was refused by `pack`, the
/// refusal was remembered so it was never retried, and the painter, finding
/// no region, drew nothing — a box of bare background, which on a dark
/// surface is a black square, with no error in the log and no way to tell
/// it from a picture that is genuinely black. A photograph off a phone or a
/// screenshot of a whole page is over the line by default, so this was the
/// common case rather than the edge one.
///
/// The natural size is left alone: `Image` is what the layout measures an
/// unsized picture by (03 §1), and a picture that laid out at the size we
/// happened to pack it at would change shape for the wrong reason. Only the
/// texels handed to the atlas shrink, and the painter stretches whatever
/// region it finds across the node's box, so the drawing is unchanged.
///
/// Box-filtered, not sampled: a screenshot of text reduced by nearest
/// neighbour is noise. Each destination pixel averages the source pixels
/// that fall under it, weighted by alpha so that what is transparent does
/// not drag colour into what is not. One pass over the picture, once, when
/// its bytes arrive.
pub fn fit_to_atlas(img: &Image) -> Option<Image> {
    let long = img.width.max(img.height);
    if long <= ATLAS_EDGE || img.width == 0 || img.height == 0 {
        return None;
    }
    let scale = f64::from(ATLAS_EDGE) / f64::from(long);
    let nw = ((f64::from(img.width) * scale).round() as u32).clamp(1, ATLAS_EDGE);
    let nh = ((f64::from(img.height) * scale).round() as u32).clamp(1, ATLAS_EDGE);
    let (w, h) = (img.width as usize, img.height as usize);
    if img.rgba.len() != w.checked_mul(h)?.checked_mul(4)? {
        return None;
    }
    let (dw, dh) = (nw as usize, nh as usize);
    let mut out = vec![0u8; dw.checked_mul(dh)?.checked_mul(4)?];
    for y in 0..dh {
        let y0 = y * h / dh;
        let y1 = (((y + 1) * h).div_ceil(dh)).clamp(y0 + 1, h);
        for x in 0..dw {
            let x0 = x * w / dw;
            let x1 = (((x + 1) * w).div_ceil(dw)).clamp(x0 + 1, w);
            // Colour is summed already multiplied by its own alpha, so a
            // transparent pixel contributes none of its colour; the sum is
            // divided back out at the end.
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let Some(&[pr, pg, pb, pa]) = img.rgba.get((sy * w + sx) * 4..(sy * w + sx) * 4 + 4) else { continue };
                    let al = u64::from(pa);
                    r += u64::from(pr) * al;
                    g += u64::from(pg) * al;
                    b += u64::from(pb) * al;
                    a += al;
                    n += 1;
                }
            }
            if a == 0 || n == 0 {
                continue;
            }
            let Some([dr, dg, db, da]) = out.get_mut((y * dw + x) * 4..(y * dw + x) * 4 + 4) else { continue };
            *dr = (r / a) as u8;
            *dg = (g / a) as u8;
            *db = (b / a) as u8;
            *da = (a / n) as u8;
        }
    }
    Some(Image { width: nw, height: nh, rgba: out })
}

/// Decode whatever the bytes are, by their first bytes: PNG always, JPEG
/// and WebP when the client was built with them (`jpeg`, `webp`). A format
/// the build does not carry is a decode failure with its name in it, so
/// the reason reaches the log rather than a blank box.
pub fn decode_image(bytes: &[u8]) -> Result<Image, AssetError> {
    if bytes.starts_with(b"\x89PNG") {
        return decode_png(bytes);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        #[cfg(feature = "jpeg")]
        return decode_jpeg(bytes);
        #[cfg(not(feature = "jpeg"))]
        return Err(AssetError::Decode("JPEG: this build has no decoder".into()));
    }
    if bytes.len() > 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        #[cfg(feature = "webp")]
        return decode_webp(bytes);
        #[cfg(not(feature = "webp"))]
        return Err(AssetError::Decode("WebP: this build has no decoder".into()));
    }
    Err(AssetError::Decode("not a PNG, JPEG or WebP".into()))
}

/// True for bytes that name a format the client decodes at all, whatever
/// this build carries: what tells a picture from a sound or a font.
pub fn looks_like_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG") || bytes.starts_with(&[0xff, 0xd8, 0xff]) || (bytes.len() > 12 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
}

/// Decode a JPEG. Baseline and progressive, greyscale or colour; the
/// decoder hands back RGB and the alpha is filled in.
#[cfg(feature = "jpeg")]
pub fn decode_jpeg(bytes: &[u8]) -> Result<Image, AssetError> {
    use zune_jpeg::zune_core::colorspace::ColorSpace;
    use zune_jpeg::zune_core::options::DecoderOptions;
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGB).set_max_width(MAX_IMAGE_EDGE as usize).set_max_height(MAX_IMAGE_EDGE as usize);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(std::io::Cursor::new(bytes), options);
    let rgb = decoder.decode().map_err(|e| AssetError::Decode(e.to_string()))?;
    let (w, h) = decoder.dimensions().ok_or_else(|| AssetError::Decode("no dimensions".to_string()))?;
    let (width, height) = (u32::try_from(w).unwrap_or(u32::MAX), u32::try_from(h).unwrap_or(u32::MAX));
    if width > MAX_IMAGE_EDGE || height > MAX_IMAGE_EDGE {
        return Err(AssetError::Decode("image too large".into()));
    }
    let want = (width as usize).saturating_mul(height as usize).saturating_mul(3);
    if rgb.len() != want {
        return Err(AssetError::Decode("unexpected pixel count".into()));
    }
    let rgba = rgb
        .chunks_exact(3)
        .flat_map(|p| match p {
            [r, g, b] => [*r, *g, *b, 255],
            _ => [0, 0, 0, 0],
        })
        .collect();
    Ok(Image { width, height, rgba })
}

/// Decode a WebP, lossy or lossless, with its alpha when it has one. An
/// animation is decoded to its first frame: a still is what an `image`
/// node draws.
#[cfg(feature = "webp")]
pub fn decode_webp(bytes: &[u8]) -> Result<Image, AssetError> {
    let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).map_err(|e| AssetError::Decode(e.to_string()))?;
    let (width, height) = decoder.dimensions();
    if width > MAX_IMAGE_EDGE || height > MAX_IMAGE_EDGE {
        return Err(AssetError::Decode("image too large".into()));
    }
    let channels = if decoder.has_alpha() { 4 } else { 3 };
    let count = (width as usize).saturating_mul(height as usize);
    let mut buf = vec![0u8; count.saturating_mul(channels)];
    decoder.read_image(&mut buf).map_err(|e| AssetError::Decode(e.to_string()))?;
    let rgba = if channels == 4 {
        buf
    } else {
        buf.chunks_exact(3)
            .flat_map(|p| match p {
                [r, g, b] => [*r, *g, *b, 255],
                _ => [0, 0, 0, 0],
            })
            .collect()
    };
    if rgba.len() != count.saturating_mul(4) {
        return Err(AssetError::Decode("unexpected pixel count".into()));
    }
    Ok(Image { width, height, rgba })
}

/// Decode a PNG.
pub fn decode_png(bytes: &[u8]) -> Result<Image, AssetError> {
    let mut decoder = png::Decoder::new(bytes);
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(|e| AssetError::Decode(e.to_string()))?;
    let info = reader.info();
    if info.width > MAX_IMAGE_EDGE || info.height > MAX_IMAGE_EDGE {
        return Err(AssetError::Decode("image too large".into()));
    }
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let out = reader.next_frame(&mut buf).map_err(|e| AssetError::Decode(e.to_string()))?;
    let (width, height) = (out.width, out.height);
    let data = buf.get(..out.buffer_size()).unwrap_or(&[]);
    let rgba: Vec<u8> = match out.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|p| match p {
                [r, g, b] => [*r, *g, *b, 255],
                _ => [0, 0, 0, 0],
            })
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|p| match p {
                [g, a] => [*g, *g, *g, *a],
                _ => [0, 0, 0, 0],
            })
            .collect(),
        png::ColorType::Indexed => return Err(AssetError::Decode("indexed PNG not expanded".into())),
    };
    if rgba.len() != (width as usize).saturating_mul(height as usize).saturating_mul(4) {
        return Err(AssetError::Decode("unexpected pixel count".into()));
    }
    Ok(Image { width, height, rgba })
}

/// What the client holds: raw bytes by hash, decoded images by hash, and
/// the set of hashes it has asked for and not yet received.
#[derive(Debug, Default)]
pub struct AssetStore {
    raw: HashMap<Hash, Arc<Vec<u8>>>,
    images: HashMap<Hash, Arc<Image>>,
    failed: HashMap<Hash, String>,
    wanted: HashSet<Hash>,
    pending: Vec<Hash>,
}

impl AssetStore {
    /// Raw bytes, if fetched.
    pub fn raw(&self, hash: &Hash) -> Option<Arc<Vec<u8>>> {
        self.raw.get(hash).cloned()
    }

    /// A decoded image, if fetched and decodable.
    pub fn image(&self, hash: &Hash) -> Option<Arc<Image>> {
        self.images.get(hash).cloned()
    }

    /// Why a hash could not be used, if it failed.
    pub fn failure(&self, hash: &Hash) -> Option<&str> {
        self.failed.get(hash).map(String::as_str)
    }

    /// Note that `hash` is needed; queues a fetch the first time.
    pub fn want(&mut self, hash: Hash) {
        if self.raw.contains_key(&hash) || self.failed.contains_key(&hash) {
            return;
        }
        if self.wanted.insert(hash) {
            self.pending.push(hash);
        }
    }

    /// Hashes queued since the last call. The caller fetches them.
    pub fn take_pending(&mut self) -> Vec<Hash> {
        std::mem::take(&mut self.pending)
    }

    /// Deliver fetched, already-verified bytes. Images are decoded now.
    pub fn deliver(&mut self, hash: Hash, bytes: Vec<u8>) {
        self.wanted.remove(&hash);
        if looks_like_image(&bytes) {
            match decode_image(&bytes) {
                Ok(img) => {
                    self.images.insert(hash, Arc::new(img));
                }
                Err(e) => {
                    self.failed.insert(hash, e.to_string());
                }
            }
        }
        self.raw.insert(hash, Arc::new(bytes));
    }

    /// Record a fetch failure so the hash is not asked for again.
    pub fn fail(&mut self, hash: Hash, why: String) {
        self.wanted.remove(&hash);
        self.failed.insert(hash, why);
    }

    /// Number of assets held.
    pub fn len(&self) -> usize {
        self.raw.len()
    }

    /// True when nothing is held.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }
}
