//! A bounds-checked cursor over a frame payload.
//!
//! Every read is fallible and every read advances only on success, so a caller
//! that propagates errors with `?` can never observe a half-consumed field.
//! There is no `peek`-then-`assume` shape anywhere in this crate: that pattern
//! is where length-confusion bugs live.

use crate::error::{DecodeError, Result};

/// A cursor over borrowed bytes.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Wrap a buffer.
    pub const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Bytes consumed so far.
    pub const fn position(&self) -> usize {
        self.pos
    }

    /// Bytes not yet consumed.
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// True when every byte has been consumed.
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// Fail unless the buffer is fully consumed.
    ///
    /// Called at the end of every top-level decode. Trailing bytes are an
    /// error rather than padding, because "ignore what you don't understand"
    /// is how one implementation's frame becomes another's smuggling channel.
    pub fn finish(&self) -> Result<()> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }

    /// Take exactly `n` bytes.
    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(DecodeError::Truncated)?;
        let slice = self.buf.get(self.pos..end).ok_or(DecodeError::Truncated)?;
        self.pos = end;
        Ok(slice)
    }

    /// Take one byte.
    pub fn u8(&mut self) -> Result<u8> {
        let b = *self.buf.get(self.pos).ok_or(DecodeError::Truncated)?;
        self.pos = self.pos.saturating_add(1);
        Ok(b)
    }

    /// Take exactly `N` bytes as an array.
    ///
    /// Preferred over [`Self::take`] plus indexing everywhere the width is
    /// known: it moves the length proof from a comment into the type, so no
    /// fixed-width read in this crate can be a panic in disguise.
    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?.try_into().map_err(|_| DecodeError::Truncated)
    }

    /// Take a little-endian `u16`.
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    /// Take a little-endian `u32`.
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// Take a little-endian `f64`, rejecting NaN and infinities.
    ///
    /// A NaN in a layout constraint poisons every comparison downstream and
    /// turns a sorting routine into undefined ordering, so it is refused at
    /// the door rather than defended against in the layout engine.
    pub fn f64(&mut self) -> Result<f64> {
        let v = f64::from_le_bytes(self.array()?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(DecodeError::IllegalValue("float must be finite"))
        }
    }

    /// Take a LEB128 unsigned varint, requiring a minimal encoding.
    pub fn varint(&mut self) -> Result<u64> {
        let mut result: u64 = 0;
        let mut shift: u32 = 0;
        loop {
            if shift >= 64 {
                return Err(DecodeError::BadVarint);
            }
            let byte = self.u8()?;
            let low = u64::from(byte & 0x7f);
            // At shift 63 only one payload bit still fits.
            if shift == 63 && low > 1 {
                return Err(DecodeError::BadVarint);
            }
            result |= low << shift;
            if byte & 0x80 == 0 {
                // A multi-byte encoding whose final byte carries no bits is a
                // longer spelling of a shorter number. Two spellings of one
                // value is one spelling too many for anything that gets hashed,
                // signed, or compared.
                if shift > 0 && byte == 0 {
                    return Err(DecodeError::BadVarint);
                }
                return Ok(result);
            }
            shift = shift.saturating_add(7);
        }
    }

    /// Take a varint that must fit in a `u32`.
    pub fn varint32(&mut self) -> Result<u32> {
        u32::try_from(self.varint()?).map_err(|_| DecodeError::BadVarint)
    }

    /// Take a varint that must fit in a `u32` and not exceed `max`.
    pub fn varint32_max(&mut self, max: u32, what: &'static str) -> Result<u32> {
        let v = self.varint32()?;
        if v > max {
            return Err(DecodeError::LimitExceeded(what));
        }
        Ok(v)
    }

    /// Take a zigzag-encoded signed varint.
    pub fn svarint(&mut self) -> Result<i64> {
        let raw = self.varint()?;
        // Zigzag: the low bit is the sign, the rest is the magnitude.
        Ok(((raw >> 1) as i64) ^ ((raw & 1) as i64).wrapping_neg())
    }

    /// Take a length-prefixed byte string, bounded by `max`.
    pub fn bytes(&mut self, max: usize, what: &'static str) -> Result<&'a [u8]> {
        let len = usize::try_from(self.varint()?).map_err(|_| DecodeError::BadVarint)?;
        if len > max {
            return Err(DecodeError::LimitExceeded(what));
        }
        self.take(len)
    }

    /// Take a length-prefixed UTF-8 string, bounded by `max`.
    pub fn str(&mut self, max: usize, what: &'static str) -> Result<&'a str> {
        core::str::from_utf8(self.bytes(max, what)?).map_err(|_| DecodeError::BadUtf8)
    }
}
