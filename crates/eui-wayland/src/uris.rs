//! `text/uri-list` (RFC 2483), which is how a hand full of files crosses
//! between two Wayland clients.

use std::path::PathBuf;

/// The local paths in a `text/uri-list` body, in the order they came.
///
/// Anything that is not a file on this machine is dropped rather than
/// guessed at: a `http:` URI is a download and not a drop, and a `file:`
/// URI naming another host names a file this process cannot open.
///
/// The body is bytes and the paths are bytes. A path on Linux is not text
/// — it is any sequence without a NUL or a slash — so the percent-decoding
/// here hands back bytes and the `PathBuf` is built from them. Decoding to
/// a `String` first would be the shorter way and would quietly replace a
/// filename that is Latin-1, or Shift-JIS, or simply broken, with one full
/// of `U+FFFD` that no longer opens anything.
#[must_use]
pub fn paths(body: &[u8]) -> Vec<PathBuf> {
    // CRLF is what the RFC says and what GTK and Qt send; a bare LF is what
    // a shell script piping `echo` sends, and refusing it would be refusing
    // the easiest way there is to test this by hand.
    body.split(|b| *b == b'\n')
        .map(|line| line.strip_suffix(b"\r").unwrap_or(line))
        // A `#` line is a comment and an empty one is the trailing
        // newline every well-formed body ends with.
        .filter(|line| !line.is_empty() && !line.starts_with(b"#"))
        .filter_map(one)
        .collect()
}

/// Percent-decoded bytes as a path.
///
/// A Unix path is bytes, so on the platform this crate exists for they go
/// across whole: a name that is not UTF-8 is still a name, and dropping it
/// would mean refusing a file somebody can see on their desk.
///
/// Elsewhere there is no such conversion — and elsewhere is only reached
/// because this module is parsing, and parsing is worth compiling and
/// testing on every platform the workspace builds for. It was not gated,
/// `std::os::unix` is not there on Windows, and `Run Tests
/// (windows-latest)` had been failing on it. UTF-8 or nothing: a `file:`
/// URI naming a Windows path is UTF-8 in practice, and no real drop ever
/// arrives there — `Dnd::start` answers `None` off Linux and winit reports
/// its own drops.
#[cfg(unix)]
fn path_from_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

/// See the Unix one above: UTF-8 or nothing.
#[cfg(not(unix))]
fn path_from_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    String::from_utf8(bytes).ok().map(PathBuf::from)
}

/// The path one URI names, if it names a local file at all.
fn one(uri: &[u8]) -> Option<PathBuf> {
    // The scheme is case-insensitive (RFC 3986 §3.1) and `FILE:` does turn
    // up, from Java applications among others.
    let rest = strip_scheme(uri, b"file")?;
    let path = if let Some(after) = rest.strip_prefix(b"//") {
        // `file://<authority>/<path>`. An empty authority and `localhost`
        // both mean this machine; anything else names another one.
        let cut = after.iter().position(|b| *b == b'/')?;
        let (authority, path) = after.split_at(cut);
        if !authority.is_empty() && !authority.eq_ignore_ascii_case(b"localhost") {
            return None;
        }
        path
    } else if rest.starts_with(b"/") {
        // `file:/path`: not what anything well-behaved sends, and cheap to
        // accept from whatever does.
        rest
    } else {
        return None;
    };
    let bytes = unescape(path);
    // An empty path is not a file, and a NUL cannot be in one: a byte that
    // cannot reach the kernel is a URI to put down rather than to truncate.
    if bytes.is_empty() || bytes.contains(&0) {
        return None;
    }
    path_from_bytes(bytes)
}

/// What follows `scheme:`, or `None` when that is not the scheme.
fn strip_scheme<'a>(uri: &'a [u8], scheme: &[u8]) -> Option<&'a [u8]> {
    let head = uri.get(..scheme.len())?;
    if !head.eq_ignore_ascii_case(scheme) {
        return None;
    }
    uri.get(scheme.len()..)?.strip_prefix(b":")
}

/// Percent-decoding, to bytes.
///
/// A `%` that does not begin a pair of hex digits is passed through as
/// itself. It is malformed either way, and a filename with a literal `%`
/// in it is far likelier than a sender that meant something else by it —
/// so the byte is kept rather than the line thrown away.
fn unescape(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while let Some(b) = src.get(i) {
        let pair = || {
            let hi = hex(*src.get(i.checked_add(1)?)?)?;
            let lo = hex(*src.get(i.checked_add(2)?)?)?;
            hi.checked_mul(16)?.checked_add(lo)
        };
        match (*b == b'%').then(pair).flatten() {
            Some(byte) => {
                out.push(byte);
                i = i.saturating_add(3);
            }
            None => {
                out.push(*b);
                i = i.saturating_add(1);
            }
        }
    }
    out
}

/// One hex digit's value.
fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => b.checked_sub(b'0'),
        b'a'..=b'f' => b.checked_sub(b'a').and_then(|v| v.checked_add(10)),
        b'A'..=b'F' => b.checked_sub(b'A').and_then(|v| v.checked_add(10)),
        _ => None,
    }
}
