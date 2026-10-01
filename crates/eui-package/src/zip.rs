//! The zip an APK and an iOS bundle travel in: read every entry, write a new
//! archive from them.
//!
//! An entry that is not changed is carried across as the compressed bytes it
//! arrived as, with the checksum it arrived with, so the 11 MB shared object
//! in a client package is never inflated and deflated again on the way
//! through. New entries are stored, not deflated: they are PNGs, which are
//! compressed already, a manifest of two kilobytes and a one-line address.
//!
//! No zip64, no encryption, no spanning: a client package has none, and an
//! archive that does is refused by name rather than half-read.

/// Stored, not compressed.
pub const STORED: u16 = 0;
/// Deflate.
pub const DEFLATED: u16 = 8;

/// The zipalign extra field: an id, then the alignment, then padding.
const ALIGN_EXTRA_ID: u16 = 0xD935;

/// One entry, with its data as it is in the archive.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The path inside the archive.
    pub name: String,
    /// [`STORED`] or [`DEFLATED`].
    pub method: u16,
    /// The CRC-32 of the uncompressed data.
    pub crc: u32,
    /// The uncompressed length.
    pub size: u32,
    /// The data as it is stored: compressed when `method` is [`DEFLATED`].
    pub data: Vec<u8>,
    /// DOS time and date, carried across.
    pub time: u16,
    /// DOS date.
    pub date: u16,
    /// "Version made by": its high byte says whose `external` this is.
    pub made_by: u16,
    /// External attributes: on a Unix-made entry the mode is in the high half,
    /// and that is where the executable bit of an iOS binary lives.
    pub external: u32,
}

impl Entry {
    /// A new stored entry. `mode` is a Unix mode, or zero for "the default".
    pub fn stored(name: &str, data: Vec<u8>, mode: u32) -> Self {
        Self {
            name: name.to_owned(),
            method: STORED,
            crc: crc32fast::hash(&data),
            size: u32::try_from(data.len()).unwrap_or(u32::MAX),
            data,
            // 1980-01-01 00:00, the zip epoch: the same bytes on every run.
            time: 0,
            date: 0x21,
            made_by: (3 << 8) | 20,
            external: if mode == 0 { 0o100_644 << 16 } else { mode << 16 },
        }
    }

    /// The uncompressed bytes, checked against the entry's checksum.
    pub fn contents(&self) -> Result<Vec<u8>, String> {
        let out = match self.method {
            STORED => self.data.clone(),
            DEFLATED => miniz_oxide::inflate::decompress_to_vec_with_limit(&self.data, self.size as usize).map_err(|e| format!("{}: does not inflate ({e:?})", self.name))?,
            m => return Err(format!("{}: compression method {m} is neither stored nor deflate", self.name)),
        };
        if out.len() != self.size as usize || crc32fast::hash(&out) != self.crc {
            return Err(format!("{}: the data does not match its checksum", self.name));
        }
        Ok(out)
    }

    /// The Unix mode, where the entry was made on a Unix and says one.
    pub fn unix_mode(&self) -> Option<u32> {
        let mode = self.external >> 16;
        (self.made_by >> 8 == 3 && mode != 0).then_some(mode & 0o7777)
    }
}

fn u16_at(b: &[u8], at: usize) -> Result<u16, String> {
    let s = b.get(at..at.saturating_add(2)).ok_or("the archive is truncated")?;
    Ok(u16::from_le_bytes([s.first().copied().unwrap_or(0), s.get(1).copied().unwrap_or(0)]))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, String> {
    let s = b.get(at..at.saturating_add(4)).ok_or("the archive is truncated")?;
    let mut a = [0u8; 4];
    a.copy_from_slice(s);
    Ok(u32::from_le_bytes(a))
}

/// Every entry of an archive, in central-directory order.
pub fn read(b: &[u8]) -> Result<Vec<Entry>, String> {
    // The end record is the last 22 bytes, unless a comment follows it.
    let lowest = b.len().saturating_sub(22 + 0xFFFF);
    let mut eocd = None;
    let mut at = b.len().saturating_sub(22);
    loop {
        if u32_at(b, at).ok() == Some(0x0605_4b50) {
            eocd = Some(at);
            break;
        }
        if at == lowest || at == 0 {
            break;
        }
        at = at.saturating_sub(1);
    }
    let eocd = eocd.ok_or("not a zip archive: no end-of-central-directory record")?;
    let count = u16_at(b, eocd + 10)?;
    let cd_offset = u32_at(b, eocd + 16)?;
    if count == 0xFFFF || cd_offset == 0xFFFF_FFFF {
        return Err("a zip64 archive; a client package is never one".into());
    }
    let mut entries = Vec::with_capacity(count as usize);
    let mut p = cd_offset as usize;
    for _ in 0..count {
        if u32_at(b, p)? != 0x0201_4b50 {
            return Err("the central directory is damaged".into());
        }
        let made_by = u16_at(b, p + 4)?;
        let flags = u16_at(b, p + 8)?;
        let method = u16_at(b, p + 10)?;
        let time = u16_at(b, p + 12)?;
        let date = u16_at(b, p + 14)?;
        let crc = u32_at(b, p + 16)?;
        let csize = u32_at(b, p + 20)? as usize;
        let size = u32_at(b, p + 24)?;
        let name_len = u16_at(b, p + 28)? as usize;
        let extra_len = u16_at(b, p + 30)? as usize;
        let comment_len = u16_at(b, p + 32)? as usize;
        let external = u32_at(b, p + 38)?;
        let local = u32_at(b, p + 42)? as usize;
        let name = b.get(p + 46..p + 46 + name_len).ok_or("the archive is truncated")?;
        let name = String::from_utf8(name.to_vec()).map_err(|_| "an entry's name is not UTF-8")?;
        if flags & 1 != 0 {
            return Err(format!("{name}: encrypted"));
        }
        if u32_at(b, local)? != 0x0403_4b50 {
            return Err(format!("{name}: its local header is missing"));
        }
        let start = local + 30 + u16_at(b, local + 26)? as usize + u16_at(b, local + 28)? as usize;
        let data = b.get(start..start + csize).ok_or_else(|| format!("{name}: truncated"))?.to_vec();
        entries.push(Entry { name, method, crc, size, data, time, date, made_by, external });
        p += 46 + name_len + extra_len + comment_len;
    }
    Ok(entries)
}

/// An archive, and where its central directory starts — which is where an
/// APK signing block goes.
pub struct Written {
    /// The whole archive.
    pub bytes: Vec<u8>,
    /// The offset of the central directory.
    pub cd_offset: usize,
}

/// Write `entries` in order. A stored entry's data starts on a multiple of
/// `align(entry)`, which is what `zipalign` does: 4 for anything, 4096 for a
/// shared object the platform may map straight out of the package.
pub fn write(entries: &[Entry], align: impl Fn(&Entry) -> usize) -> Written {
    let mut out: Vec<u8> = Vec::new();
    let mut offsets = Vec::with_capacity(entries.len());
    for e in entries {
        let offset = out.len();
        offsets.push(offset);
        let name = e.name.as_bytes();
        let mut extra: Vec<u8> = Vec::new();
        let a = align(e).max(1);
        if e.method == STORED && a > 1 {
            // The data starts after the 30-byte header, the name and the
            // extra field; the extra is an id, a length, the alignment and
            // as many zeros as it takes.
            let base = offset + 30 + name.len() + 6;
            let pad = (a - base % a) % a;
            extra.extend_from_slice(&ALIGN_EXTRA_ID.to_le_bytes());
            extra.extend_from_slice(&u16::try_from(2 + pad).unwrap_or(2).to_le_bytes());
            extra.extend_from_slice(&u16::try_from(a).unwrap_or(4).to_le_bytes());
            extra.resize(6 + pad, 0);
        }
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        // UTF-8 names; never a data descriptor, the sizes are known.
        out.extend_from_slice(&0x0800u16.to_le_bytes());
        out.extend_from_slice(&e.method.to_le_bytes());
        out.extend_from_slice(&e.time.to_le_bytes());
        out.extend_from_slice(&e.date.to_le_bytes());
        out.extend_from_slice(&e.crc.to_le_bytes());
        out.extend_from_slice(&u32::try_from(e.data.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(&e.size.to_le_bytes());
        out.extend_from_slice(&u16::try_from(name.len()).unwrap_or(u16::MAX).to_le_bytes());
        out.extend_from_slice(&u16::try_from(extra.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(name);
        out.extend_from_slice(&extra);
        out.extend_from_slice(&e.data);
    }
    let cd_offset = out.len();
    for (e, offset) in entries.iter().zip(&offsets) {
        let name = e.name.as_bytes();
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&e.made_by.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0x0800u16.to_le_bytes());
        out.extend_from_slice(&e.method.to_le_bytes());
        out.extend_from_slice(&e.time.to_le_bytes());
        out.extend_from_slice(&e.date.to_le_bytes());
        out.extend_from_slice(&e.crc.to_le_bytes());
        out.extend_from_slice(&u32::try_from(e.data.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(&e.size.to_le_bytes());
        out.extend_from_slice(&u16::try_from(name.len()).unwrap_or(u16::MAX).to_le_bytes());
        // No extra, no comment, disk 0, no internal attributes.
        out.extend_from_slice(&[0u8; 8]);
        out.extend_from_slice(&e.external.to_le_bytes());
        out.extend_from_slice(&u32::try_from(*offset).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(name);
    }
    let cd_size = out.len() - cd_offset;
    let n = u16::try_from(entries.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 4]);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&u32::try_from(cd_size).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&u32::try_from(cd_offset).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Written { bytes: out, cd_offset }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn what_is_written_reads_back_aligned() {
        let deflated = miniz_oxide::deflate::compress_to_vec(b"hello hello hello hello", 6);
        let entries = vec![
            Entry::stored("a.txt", b"one".to_vec(), 0),
            Entry { name: "b.txt".into(), method: DEFLATED, crc: crc32fast::hash(b"hello hello hello hello"), size: 23, data: deflated, time: 0, date: 0x21, made_by: 20, external: 0 },
            Entry::stored("lib/x/libz.so", vec![7u8; 100], 0o755),
        ];
        let w = write(&entries, |e| if e.name.ends_with(".so") { 4096 } else { 4 });
        let back = read(&w.bytes).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[0].contents().unwrap(), b"one");
        assert_eq!(back[1].contents().unwrap(), b"hello hello hello hello");
        assert_eq!(back[2].unix_mode(), Some(0o755));
        // Where the data of each stored entry starts.
        for (e, a) in [(0usize, 4usize), (2, 4096)] {
            let local = w.bytes.windows(back[e].name.len()).position(|n| n == back[e].name.as_bytes()).unwrap();
            let extra = u16_at(&w.bytes, local - 30 + 28).unwrap() as usize;
            assert_eq!((local + back[e].name.len() + extra) % a, 0, "{} is not aligned to {a}", back[e].name);
        }
    }
}
