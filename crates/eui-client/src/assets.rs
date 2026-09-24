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

#[cfg(has_native_net)]
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
    /// The server answered, with this status. Separate from [`Self::Http`]
    /// because a caller has to be able to tell a `404` — which for a view is
    /// "not served that way, open a socket instead" — from a malformed reply,
    /// without matching on the text of a status line.
    Status(u16),
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
            Self::Status(code) => write!(f, "http {code}"),
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

/// The address somebody typed, as the protocol spells it.
///
/// Spec 01 §2.1 already takes the view that "the protocol's own prefix is
/// the part nobody should have to type". Somebody pasting what their browser
/// shows them is doing the obvious thing, so `https://` is read as `wss://`
/// and `http://` as `ws://` — the same origin, named the way the rest of the
/// web names it — and a bare host gets `wss://`, since that is the only
/// scheme a public address can have.
///
/// This relaxes nothing. [`crate::check_url`] still refuses plain `ws://`
/// anywhere but loopback, so `http://` is a spelling of an address and not a
/// way past the rule.
pub fn normalise_url(url: &str) -> String {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("https://") {
        return format!("wss://{rest}");
    }
    if let Some(rest) = url.strip_prefix("http://") {
        return format!("ws://{rest}");
    }
    if url.contains("://") {
        return url.to_owned();
    }
    format!("wss://{url}")
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
        // `normalise_url` turns `https`/`http` into these two before any
        // address reaches here, so anything else is a caller that skipped it.
        other => return Err(AssetError::Origin(format!("unsupported scheme {other}: an address is ws:// or wss:// by the time it gets here"))),
    };
    Ok(format!("{http}://{host}"))
}

// ------------------------------------------------- the fetch, where we do it
//
// A page does not. The browser has already done the TLS, checked the chain
// and parsed the reply by the time a byte reaches this crate, so on
// `wasm32` the whole of this section is replaced by a `fetch()` in
// `transport_web.rs` — and what it hands back lands in `AssetStore::deliver`
// below, verified by the same `blake3` line and decoded by the same
// decoders. The split is here rather than inside each function because
// there is nothing in common between the two but the bytes.

/// What a `GET` returned, for a caller that needs more than the body.
///
/// The asset path never did: an asset is named by its own hash, so the
/// headers say nothing the bytes do not. A view is named by a path, so its
/// `ETag` is the only thing that makes a second fetch cheap and its
/// `Content-Type` the only thing that says the body is what was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    /// The body.
    pub body: Vec<u8>,
    /// The `ETag` exactly as the server spelled it, quotes and all. Never
    /// rebuilt from a hash of the body: a server may send a weak or suffixed
    /// tag, and the only correct `If-None-Match` is the bytes it sent.
    pub etag: Option<String>,
    /// The `Content-Type`, lowercased, without its parameters.
    pub content_type: Option<String>,
}

/// Fetch and verify one asset. Blocking; runs its own small runtime, so call
/// it from a worker thread.
#[cfg(has_native_net)]
pub fn fetch(origin: &str, hash: &Hash, cookie: Option<&str>) -> Result<Vec<u8>, AssetError> {
    fetch_within(origin, hash, cookie, MAX_ASSET_BYTES)
}

/// [`fetch`], abandoned the moment the body is known to be larger than
/// `cap` — from its `Content-Length`, before a byte of it is read, or from
/// the bytes, whichever says so first. 01 §2.2: `cap` is what is left of
/// the session's asset budget, and a response larger than that MUST be
/// abandoned mid-stream rather than read and then thrown away.
#[cfg(has_native_net)]
pub fn fetch_within(origin: &str, hash: &Hash, cookie: Option<&str>, cap: usize) -> Result<Vec<u8>, AssetError> {
    let cap = cap.min(MAX_ASSET_BYTES);
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| AssetError::Connect(e.to_string()))?;
    let bytes = rt.block_on(get_async(origin, &format!("/_eui/asset/{}", hex(hash)), "application/octet-stream", cookie, cap))?.body;
    if *blake3::hash(&bytes).as_bytes() != *hash {
        return Err(AssetError::HashMismatch);
    }
    Ok(bytes)
}

/// One strict HTTPS `GET` of `path` at `origin`: no cookie, no redirect, a
/// `Content-Length` body no larger than an asset. Blocking. The manifest and
/// every asset come through here and nothing else does.
#[cfg(has_native_net)]
pub fn get(origin: &str, path: &str, accept: &str, cookie: Option<&str>) -> Result<Vec<u8>, AssetError> {
    Ok(get_full(origin, path, accept, cookie)?.body)
}

/// The same `GET`, with the two headers a view fetch needs. Blocking.
#[cfg(has_native_net)]
pub fn get_full(origin: &str, path: &str, accept: &str, cookie: Option<&str>) -> Result<Fetched, AssetError> {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| AssetError::Connect(e.to_string()))?;
    rt.block_on(get_async(origin, path, accept, cookie, MAX_ASSET_BYTES))
}

#[cfg(has_native_net)]
async fn get_async(origin: &str, path: &str, accept: &str, cookie: Option<&str>, cap: usize) -> Result<Fetched, AssetError> {
    let (scheme, hostport) = origin.split_once("://").ok_or_else(|| AssetError::Origin("no scheme".into()))?;
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.contains(']') || h.ends_with(']') => (h.trim_matches(|c| c == '[' || c == ']'), p.parse::<u16>().map_err(|_| AssetError::Origin("bad port".into()))?),
        _ => (hostport, if scheme == "https" { 443 } else { 80 }),
    };
    // The caller's cookie, not a process-wide one: two sessions in one
    // process must not present each other's.
    let cookie = cookie.map_or(String::new(), |c| format!("Cookie: {c}\r\n"));
    let request = format!("GET {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\nAccept: {accept}\r\nAccept-Encoding: identity\r\n{cookie}\r\n");

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
        read_capped(&mut tls, &mut raw, cap).await?;
    } else {
        let mut tcp = tcp;
        tcp.write_all(request.as_bytes()).await.map_err(|e| AssetError::Connect(e.to_string()))?;
        read_capped(&mut tcp, &mut raw, cap).await?;
    }
    parse_response(&raw, cap)
}

#[cfg(has_native_net)]
async fn read_capped<S: AsyncReadExt + Unpin>(s: &mut S, out: &mut Vec<u8>, cap: usize) -> Result<(), AssetError> {
    let mut buf = [0u8; 16 * 1024];
    loop {
        let n = s.read(&mut buf).await.map_err(|e| AssetError::Connect(e.to_string()))?;
        if n == 0 {
            return Ok(());
        }
        if out.len().saturating_add(n) > cap.saturating_add(4096) {
            return Err(AssetError::TooLarge);
        }
        out.extend_from_slice(buf.get(..n).unwrap_or(&[]));
    }
}

/// The smallest HTTP/1.1 response reader that is still strict: status 200,
/// a `Content-Length`, exactly that many body bytes — and now the two
/// headers a view fetch reads.
///
/// Chunked is still refused, and the endpoint is specified to send a length
/// (01 §2.4) precisely so it never has to be: a server that computed the
/// body's ETag has the whole body in hand and can say how long it is. What
/// this reader will not do is guess, because every guess here is a guess
/// about where somebody else's bytes end.
#[cfg(has_native_net)]
fn parse_response(raw: &[u8], cap: usize) -> Result<Fetched, AssetError> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| AssetError::Http("no header terminator".into()))?;
    let head = std::str::from_utf8(raw.get(..split).unwrap_or(&[])).map_err(|_| AssetError::Http("non-UTF-8 headers".into()))?;
    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or("");
    if !status.starts_with("HTTP/1.1 200") && !status.starts_with("HTTP/1.0 200") {
        // A well-formed status line the caller can branch on; anything else
        // is a reply we could not read at all.
        let code = status.split(' ').nth(1).and_then(|c| c.parse::<u16>().ok());
        return Err(code.map_or_else(|| AssetError::Http(status.to_string()), AssetError::Status));
    }
    let mut length: Option<usize> = None;
    let mut etag: Option<String> = None;
    let mut content_type: Option<String> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                length = Some(v.trim().parse().map_err(|_| AssetError::Http("bad content-length".into()))?);
            }
            if k.eq_ignore_ascii_case("transfer-encoding") {
                return Err(AssetError::Http("chunked bodies are not accepted".into()));
            }
            if k.eq_ignore_ascii_case("etag") {
                etag = Some(v.trim().to_owned());
            }
            if k.eq_ignore_ascii_case("content-type") {
                // The type without its parameters: `application/vnd.eui.frames`
                // and `application/vnd.eui.frames; charset=utf-8` are the same
                // answer to the only question being asked of it.
                content_type = Some(v.split(';').next().unwrap_or("").trim().to_ascii_lowercase());
            }
        }
    }
    let length = length.ok_or_else(|| AssetError::Http("no content-length".into()))?;
    if length > cap {
        return Err(AssetError::TooLarge);
    }
    let body = raw.get(split.saturating_add(4)..).unwrap_or(&[]);
    if body.len() != length {
        return Err(AssetError::Http(format!("body is {} bytes, header says {length}", body.len())));
    }
    Ok(Fetched { body: body.to_vec(), etag, content_type })
}

// ------------------------------------------- everything below is portable

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
    resized(img, nw, nh)
}

/// `img` at exactly `nw × nh`, by the filter [`fit_to_atlas`] describes.
///
/// Split out because the atlas is no longer the only thing that wants it:
/// an icon handed to a desktop has to be the size that desktop's format
/// declares — 512 for an `.icns` entry, 256 for an `.ico` — and an
/// application's own PNG is whatever the publisher drew.
pub fn resized(img: &Image, nw: u32, nh: u32) -> Option<Image> {
    if nw == 0 || nh == 0 || img.width == 0 || img.height == 0 {
        return None;
    }
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

/// True for bytes that open like a font file the shaper can read.
///
/// `sfnt` in its two spellings — `0x00010000` for TrueType outlines,
/// `OTTO` for CFF — plus the `ttcf` collection. WOFF and WOFF2 are
/// deliberately absent: their tables are compressed, the client carries no
/// inflate or brotli, and a server that wants a face on this client sends
/// the face rather than an archive of it (05 §3).
pub fn looks_like_font(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some(b"\x00\x01\x00\x00" | b"OTTO" | b"true" | b"ttcf"))
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

/// Spec 10, *Assets*: bytes one session's asset store may hold — the
/// fetched files it keeps and the pictures it has decoded, together.
///
/// Past it the store lets go of what the live tree no longer names, least
/// recently used first. What the tree does name is never let go, so a page
/// that shows more than this at once holds more than this; what it cannot
/// do is *fetch* more, because a fetch is capped at what is left once the
/// named assets are counted (01 §2.2), and the one that does not fit is
/// abandoned mid-stream.
pub const MAX_STORE_BYTES: usize = 128 * 1024 * 1024;

/// A picture as the store keeps it: the size it was drawn at, which the
/// layout measures an unsized `image` by (03 §1), and the pixels as the
/// sheet will hold them — no larger than [`ATLAS_EDGE`] on a side.
///
/// The natural pixels are not kept. Nothing but the atlas ever read them,
/// and the atlas only ever read the shrunk copy; a 4096 × 4096 photograph
/// held whole beside it was 64 MiB kept for a 4 MiB upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    /// Natural width, in pixels.
    pub width: u32,
    /// Natural height, in pixels.
    pub height: u32,
    /// What goes into the sheet.
    pub pixels: Image,
}

/// Everything a picture costs before it can be packed: the decode and the
/// shrink. Run off the window's thread (see `decode.rs`), because on a
/// photograph both are tens of milliseconds.
pub fn prepare_image(bytes: &[u8]) -> Result<Decoded, AssetError> {
    let img = decode_image(bytes)?;
    let (width, height) = (img.width, img.height);
    let pixels = fit_to_atlas(&img).unwrap_or(img);
    Ok(Decoded { width, height, pixels })
}

/// Whether the file of a picture is worth keeping once it is decoded.
///
/// A PNG or a JPEG is only ever a still, so once its pixels are held its
/// file is dead weight. A WebP may be a moving picture as well (03 §8),
/// and a `video` node decodes it from the file — so that one is kept.
fn keeps_file(bytes: &[u8]) -> bool {
    !(bytes.starts_with(b"\x89PNG") || bytes.starts_with(&[0xff, 0xd8, 0xff]))
}

/// What the client holds: raw bytes by hash, decoded pictures by hash, and
/// the set of hashes it has asked for and not yet received — all of it
/// counted against a budget ([`MAX_STORE_BYTES`]).
#[derive(Debug)]
pub struct AssetStore {
    raw: HashMap<Hash, Arc<Vec<u8>>>,
    images: HashMap<Hash, Arc<Image>>,
    /// Natural sizes of every picture ever decoded, kept after the pixels
    /// are let go: a picture evicted and fetched again measures the same in
    /// the meantime, so the page does not jump while it comes back.
    sizes: HashMap<Hash, (u32, u32)>,
    failed: HashMap<Hash, String>,
    wanted: HashSet<Hash>,
    pending: Vec<Hash>,
    /// Bytes held: every raw file plus every decoded picture's pixels.
    held: usize,
    budget: usize,
    /// When each hash was last delivered or last found named by the tree,
    /// on a counter rather than a clock: least recently used goes first.
    used: HashMap<Hash, u64>,
    clock: u64,
}

impl Default for AssetStore {
    fn default() -> Self {
        Self {
            raw: HashMap::new(),
            images: HashMap::new(),
            sizes: HashMap::new(),
            failed: HashMap::new(),
            wanted: HashSet::new(),
            pending: Vec::new(),
            held: 0,
            budget: MAX_STORE_BYTES,
            used: HashMap::new(),
            clock: 0,
        }
    }
}

impl AssetStore {
    /// Raw bytes, if fetched and still held. A PNG or a JPEG lets go of its
    /// file once it is decoded; see [`Self::decoded`].
    pub fn raw(&self, hash: &Hash) -> Option<Arc<Vec<u8>>> {
        self.raw.get(hash).cloned()
    }

    /// A decoded picture, as the sheet holds it — no larger than
    /// [`ATLAS_EDGE`] on a side. Its natural size is [`Self::size`].
    pub fn image(&self, hash: &Hash) -> Option<Arc<Image>> {
        self.images.get(hash).cloned()
    }

    /// A picture's natural size, once it has been decoded — and still after
    /// its pixels were let go.
    pub fn size(&self, hash: &Hash) -> Option<(u32, u32)> {
        self.sizes.get(hash).copied()
    }

    /// Why a hash could not be used, if it failed.
    pub fn failure(&self, hash: &Hash) -> Option<&str> {
        self.failed.get(hash).map(String::as_str)
    }

    /// Note that `hash` is needed; queues a fetch the first time. A hash
    /// whose bytes were let go is fetched again.
    pub fn want(&mut self, hash: Hash) {
        if self.raw.contains_key(&hash) || self.images.contains_key(&hash) || self.failed.contains_key(&hash) {
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

    /// Forget that `hash` was asked for, so the next [`Self::want`] asks
    /// again: a fetch that failed and is owed another try.
    pub fn unwant(&mut self, hash: &Hash) {
        self.wanted.remove(hash);
    }

    /// Hold fetched, already-verified bytes, undecoded. Returns them shared,
    /// for whatever decodes them next.
    pub fn hold(&mut self, hash: Hash, bytes: Vec<u8>) -> Arc<Vec<u8>> {
        self.wanted.remove(&hash);
        let bytes = Arc::new(bytes);
        self.held = self.held.saturating_add(bytes.len());
        if let Some(old) = self.raw.insert(hash, Arc::clone(&bytes)) {
            self.held = self.held.saturating_sub(old.len());
        }
        self.stamp(hash);
        bytes
    }

    /// A picture's decode, finished. The pixels are held; the file is let
    /// go when nothing else could want it ([`keeps_file`]), and a picture
    /// that would not decode is remembered as failed, file and all gone.
    pub fn decoded(&mut self, hash: Hash, result: Result<Decoded, AssetError>) {
        let drop_file = match result {
            Ok(d) => {
                self.sizes.insert(hash, (d.width, d.height));
                self.held = self.held.saturating_add(d.pixels.rgba.len());
                if let Some(old) = self.images.insert(hash, Arc::new(d.pixels)) {
                    self.held = self.held.saturating_sub(old.rgba.len());
                }
                self.stamp(hash);
                self.raw.get(&hash).is_some_and(|b| !keeps_file(b))
            }
            Err(e) => {
                self.failed.insert(hash, e.to_string());
                true
            }
        };
        if drop_file {
            if let Some(old) = self.raw.remove(&hash) {
                self.held = self.held.saturating_sub(old.len());
            }
        }
    }

    /// Deliver fetched, already-verified bytes and decode a picture now, on
    /// this thread. The driver decodes off it ([`Self::hold`], then
    /// [`Self::decoded`]); this is the whole of that for everyone else.
    pub fn deliver(&mut self, hash: Hash, bytes: Vec<u8>) {
        let bytes = self.hold(hash, bytes);
        if looks_like_image(&bytes) {
            let result = prepare_image(&bytes);
            self.decoded(hash, result);
        }
    }

    /// Record a fetch failure so the hash is not asked for again.
    pub fn fail(&mut self, hash: Hash, why: String) {
        self.wanted.remove(&hash);
        self.failed.insert(hash, why);
    }

    /// Bytes held: files and decoded pictures.
    pub fn held(&self) -> usize {
        self.held
    }

    /// What the store may hold, [`MAX_STORE_BYTES`] unless a test lowered it.
    pub fn budget(&self) -> usize {
        self.budget
    }

    /// Lower the budget, so a test can reach it with a few small pictures.
    /// Never raises it past [`MAX_STORE_BYTES`].
    #[doc(hidden)]
    pub fn set_budget(&mut self, bytes: usize) {
        self.budget = bytes.min(MAX_STORE_BYTES);
    }

    /// True when the store holds more than its budget.
    pub fn over_budget(&self) -> bool {
        self.held > self.budget
    }

    /// Bytes held for `hash`: its file, if kept, and its pixels, if decoded.
    fn weight(&self, hash: &Hash) -> usize {
        self.raw.get(hash).map_or(0, |b| b.len()).saturating_add(self.images.get(hash).map_or(0, |i| i.rgba.len()))
    }

    /// How large an asset may still be fetched, given what the tree names:
    /// the budget less what `live` holds, since everything else can be let
    /// go to make room. 01 §2.2's "remaining asset budget".
    pub fn room(&self, live: &HashSet<Hash>) -> usize {
        let pinned: usize = live.iter().map(|h| self.weight(h)).fold(0, usize::saturating_add);
        self.budget.saturating_sub(pinned)
    }

    /// Let go of what `live` does not name, least recently used first,
    /// until the store is within its budget. Returns how many assets went.
    ///
    /// What `live` names is marked used now, so a picture that was on the
    /// page at the last pass outlives one that has not been since. The
    /// size of a picture let go is kept ([`Self::size`]); a hash let go is
    /// neither held nor failed, so the next [`Self::want`] fetches it again.
    pub fn evict(&mut self, live: &HashSet<Hash>) -> usize {
        for h in live {
            if self.raw.contains_key(h) || self.images.contains_key(h) {
                self.stamp(*h);
            }
        }
        if !self.over_budget() {
            return 0;
        }
        let mut idle: Vec<(u64, Hash)> = self.raw.keys().chain(self.images.keys()).filter(|h| !live.contains(*h)).map(|h| (self.used.get(h).copied().unwrap_or(0), *h)).collect();
        idle.sort_unstable();
        idle.dedup();
        let mut gone = 0;
        for (_, h) in idle {
            if !self.over_budget() {
                break;
            }
            self.held = self.held.saturating_sub(self.weight(&h));
            self.raw.remove(&h);
            self.images.remove(&h);
            self.used.remove(&h);
            gone += 1;
        }
        gone
    }

    fn stamp(&mut self, hash: Hash) {
        self.clock = self.clock.saturating_add(1);
        self.used.insert(hash, self.clock);
    }

    /// Number of assets held.
    pub fn len(&self) -> usize {
        self.raw.len() + self.images.keys().filter(|h| !self.raw.contains_key(*h)).count()
    }

    /// True when nothing is held.
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty() && self.images.is_empty()
    }
}
