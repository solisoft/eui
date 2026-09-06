//! An append-only byte sink, the exact mirror of [`crate::reader::Reader`].
//!
//! Encoding is infallible: a server building a frame it has already validated
//! cannot fail halfway. Limits are the decoder's job, and the round-trip tests
//! are what keep the two halves honest.

/// A growable output buffer.
#[derive(Debug, Default, Clone)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    /// A new, empty writer.
    pub const fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// A writer with room for `cap` bytes already reserved.
    pub fn with_capacity(cap: usize) -> Self {
        Self { buf: Vec::with_capacity(cap) }
    }

    /// Bytes written so far.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// True when nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Borrow what has been written.
    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    /// Take the written bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }

    /// Forget everything written, keeping the allocation.
    ///
    /// The server encodes one batch per tick per session; reusing the buffer is
    /// the difference between an allocation per frame and none.
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// Append one byte.
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf.push(v);
        self
    }

    /// Append raw bytes.
    pub fn raw(&mut self, v: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(v);
        self
    }

    /// Append a little-endian `u16`.
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }

    /// Append a little-endian `u32`.
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }

    /// Append a little-endian `f64`.
    pub fn f64(&mut self, v: f64) -> &mut Self {
        self.raw(&v.to_le_bytes())
    }

    /// Append a minimally-encoded LEB128 varint.
    pub fn varint(&mut self, mut v: u64) -> &mut Self {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.buf.push(byte);
                return self;
            }
            self.buf.push(byte | 0x80);
        }
    }

    /// Append a `u32` as a varint.
    pub fn varint32(&mut self, v: u32) -> &mut Self {
        self.varint(u64::from(v))
    }

    /// Append a zigzag-encoded signed varint.
    pub fn svarint(&mut self, v: i64) -> &mut Self {
        self.varint(((v << 1) ^ (v >> 63)) as u64)
    }

    /// Append a length-prefixed byte string.
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.varint(v.len() as u64).raw(v)
    }

    /// Append a length-prefixed UTF-8 string.
    pub fn str(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }
}
