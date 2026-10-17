//! Little-endian writer and reader for pack records.
//!
//! The container and every record layout use these. They're little-endian,
//! unlike the cache, and there are no varints or padding: just `u8`, `i8`,
//! `u16`, `i16` and `u32`, plus raw byte runs.

use thiserror::Error;

/// A read ran past the end of the buffer.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("record truncated at offset {offset}: needed {needed} bytes")]
pub struct Truncated {
    /// Where the failed read started, in bytes from the start of the buffer.
    pub offset: usize,
    /// How many bytes the read wanted. Fewer than that were left.
    pub needed: usize,
}

/// Append-only little-endian writer.
///
/// The methods return `&mut Self`, so you can chain fields:
/// `w.u16(id).u8(mode)`. Take the bytes out of `.0` when you're done.
#[derive(Debug, Default, Clone)]
pub struct Writer(
    /// The bytes written so far.
    pub Vec<u8>,
);

impl Writer {
    /// Append a byte.
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    /// Append a signed byte, two's complement.
    pub fn i8(&mut self, v: i8) -> &mut Self {
        self.0.push(v as u8);
        self
    }
    /// Append a little-endian `u16`.
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    /// Append a little-endian `i16`.
    pub fn i16(&mut self, v: i16) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    /// Append a little-endian `u32`.
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend(v.to_le_bytes());
        self
    }
    /// Append raw bytes as they are.
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.0.extend_from_slice(v);
        self
    }
}

/// Bounds-checked little-endian reader.
///
/// Every read either takes its bytes and moves on, or fails with
/// [`Truncated`] and leaves the position where it was.
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Start reading at the beginning of `buf`.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }
    /// Current offset into the buffer.
    pub fn pos(&self) -> usize {
        self.pos
    }
    /// Bytes left after the current position.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }
    /// Take the next `n` bytes, borrowed from the buffer.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], Truncated> {
        if n > self.remaining() {
            return Err(Truncated {
                offset: self.pos,
                needed: n,
            });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    // Fixed-size read the integer readers decode from.
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], Truncated> {
        let mut a = [0; N];
        a.copy_from_slice(self.bytes(N)?);
        Ok(a)
    }
    /// Read a byte.
    pub fn u8(&mut self) -> Result<u8, Truncated> {
        Ok(self.arr::<1>()?[0])
    }
    /// Read a signed byte.
    pub fn i8(&mut self) -> Result<i8, Truncated> {
        Ok(self.u8()? as i8)
    }
    /// Read a little-endian `u16`.
    pub fn u16(&mut self) -> Result<u16, Truncated> {
        Ok(u16::from_le_bytes(self.arr()?))
    }
    /// Read a little-endian `i16`.
    pub fn i16(&mut self) -> Result<i16, Truncated> {
        Ok(i16::from_le_bytes(self.arr()?))
    }
    /// Read a little-endian `u32`.
    pub fn u32(&mut self) -> Result<u32, Truncated> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
}
