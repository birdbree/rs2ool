//! Big-endian byte cursor used by every decoder.
//!
//! Every read is bounds-checked and returns [`IoError`] instead of panicking,
//! so a truncated or corrupt record turns up as an error you can point at.
//! A failed read leaves the position where it was.
//!
//! There are two smart encodings, and plenty of code out there calls the
//! signed one "smart" too, so go by the formula:
//!
//! ```text
//! smart          b < 128 ? b      : u16 − 32768    0..=32767        map locations
//! signed_smart   b < 128 ? b − 64 : u16 − 49152    −16384..=16383   model and frame deltas
//! ```

use thiserror::Error;

/// A read past the end of the buffer, or a string with no terminator.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IoError {
    /// A read needed more bytes than were left.
    #[error(
        "unexpected end of data at offset {offset}: needed {needed} bytes, {remaining} remaining"
    )]
    Eof {
        /// Where the read started.
        offset: usize,
        /// Bytes the read wanted.
        needed: usize,
        /// Bytes that were actually left.
        remaining: usize,
    },
    /// A string ran to the end of the buffer without a `\n`.
    #[error("unterminated string starting at offset {offset}")]
    UnterminatedString {
        /// Where the string started.
        offset: usize,
    },
    /// A seek past the end of the buffer.
    #[error("seek to offset {offset} is past the end of a {len}-byte buffer")]
    Seek {
        /// The requested position.
        offset: usize,
        /// The buffer's length.
        len: usize,
    },
}

/// A read position over a borrowed byte slice.
#[derive(Debug, Clone)]
pub struct Cursor<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// A cursor at the start of `buf`.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// A cursor starting at `pos`; fails if `pos` is past the end.
    pub fn at(buf: &'a [u8], pos: usize) -> Result<Self, IoError> {
        let mut c = Self::new(buf);
        c.seek(pos)?;
        Ok(c)
    }

    /// Current offset from the start of the buffer.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Length of the whole buffer, not what's left of it.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether the whole buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Bytes left after the current position.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Move to `pos`. Seeking to exactly the end is fine; past it is an
    /// error.
    pub fn seek(&mut self, pos: usize) -> Result<(), IoError> {
        if pos > self.buf.len() {
            return Err(IoError::Seek {
                offset: pos,
                len: self.buf.len(),
            });
        }
        self.pos = pos;
        Ok(())
    }

    /// Take the next `n` bytes as a slice and advance past them.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], IoError> {
        if n > self.remaining() {
            return Err(IoError::Eof {
                offset: self.pos,
                needed: n,
                remaining: self.remaining(),
            });
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// Advance past `n` bytes without looking at them.
    pub fn skip(&mut self, n: usize) -> Result<(), IoError> {
        self.bytes(n).map(|_| ())
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], IoError> {
        let mut out = [0; N];
        out.copy_from_slice(self.bytes(N)?);
        Ok(out)
    }

    /// The next byte without advancing.
    pub fn peek(&self) -> Result<u8, IoError> {
        self.buf.get(self.pos).copied().ok_or(IoError::Eof {
            offset: self.pos,
            needed: 1,
            remaining: 0,
        })
    }

    /// One unsigned byte.
    pub fn u8(&mut self) -> Result<u8, IoError> {
        Ok(self.array::<1>()?[0])
    }

    /// One signed byte.
    pub fn i8(&mut self) -> Result<i8, IoError> {
        Ok(self.u8()? as i8)
    }

    /// Big-endian unsigned 16-bit.
    pub fn u16(&mut self) -> Result<u16, IoError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    /// Big-endian signed 16-bit.
    pub fn i16(&mut self) -> Result<i16, IoError> {
        Ok(i16::from_be_bytes(self.array()?))
    }

    /// Big-endian unsigned 24-bit, widened to `u32`.
    pub fn u24(&mut self) -> Result<u32, IoError> {
        let [a, b, c] = self.array()?;
        Ok(u32::from_be_bytes([0, a, b, c]))
    }

    /// Big-endian unsigned 32-bit.
    pub fn u32(&mut self) -> Result<u32, IoError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    /// Big-endian signed 32-bit.
    pub fn i32(&mut self) -> Result<i32, IoError> {
        Ok(i32::from_be_bytes(self.array()?))
    }

    /// Unsigned smart: one byte if it's below 128, else a `u16 - 32768`.
    /// Range `0..=32767`.
    pub fn smart(&mut self) -> Result<u16, IoError> {
        if self.peek()? < 128 {
            Ok(self.u8()?.into())
        } else {
            // The high bit is set, so this can't underflow.
            Ok(self.u16()? - 32768)
        }
    }

    /// Signed smart: `byte - 64` if the first byte is below 128, else
    /// `u16 - 49152`. Range `-16384..=16383`.
    pub fn signed_smart(&mut self) -> Result<i16, IoError> {
        if self.peek()? < 128 {
            Ok(i16::from(self.u8()?) - 64)
        } else {
            // The high bit is set, so the u16 is at least 32768 and the
            // result lands in -16384..=16383; the cast can't wrap.
            Ok((i32::from(self.u16()?) - 49152) as i16)
        }
    }

    /// A string terminated by `\n` (10), decoded as Latin-1. The terminator
    /// is consumed and not returned.
    pub fn string(&mut self) -> Result<String, IoError> {
        let start = self.pos;
        let rest = &self.buf[self.pos..];
        let end = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or(IoError::UnterminatedString { offset: start })?;
        // Latin-1 maps each byte straight to the code point of the same value.
        let s = rest[..end].iter().map(|&b| char::from(b)).collect();
        self.pos += end + 1;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn fixed_width_reads_are_big_endian() {
        let data = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0xff];
        let mut c = Cursor::new(&data);
        assert_eq!(c.u16().unwrap(), 0x1234);
        assert_eq!(c.u24().unwrap(), 0x56789a);
        assert_eq!(c.i32().unwrap(), 0xbcdef0ffu32 as i32);
        assert_eq!(c.remaining(), 0);
    }

    #[test]
    fn signed_reads() {
        let mut c = Cursor::new(&[0xff, 0xff, 0xfe, 0x80]);
        assert_eq!(c.i8().unwrap(), -1);
        assert_eq!(c.i16().unwrap(), -2);
        assert_eq!(c.i8().unwrap(), -128);
    }

    #[test]
    fn smart_table() {
        let cases: &[(&[u8], u16)] = &[
            (&[0x00], 0),
            (&[0x7f], 127),
            (&[0x80, 0x80], 128),
            (&[0x80, 0x00], 0),
            (&[0xff, 0xff], 32767),
        ];
        for (bytes, want) in cases {
            let mut c = Cursor::new(bytes);
            assert_eq!(c.smart().unwrap(), *want, "{bytes:02x?}");
            assert_eq!(c.remaining(), 0);
        }
    }

    #[test]
    fn signed_smart_table() {
        let cases: &[(&[u8], i16)] = &[
            (&[0x00], -64),
            (&[0x40], 0),
            (&[0x7f], 63),
            (&[0x80, 0x00], -16384),
            (&[0xbf, 0xbf], -65),
            (&[0xc0, 0x40], 64),
            (&[0xff, 0xff], 16383),
        ];
        for (bytes, want) in cases {
            let mut c = Cursor::new(bytes);
            assert_eq!(c.signed_smart().unwrap(), *want, "{bytes:02x?}");
            assert_eq!(c.remaining(), 0);
        }
    }

    #[test]
    fn string_is_newline_terminated_latin1() {
        let mut c = Cursor::new(b"Abc\n\xe9\nrest");
        assert_eq!(c.string().unwrap(), "Abc");
        assert_eq!(c.string().unwrap(), "\u{e9}");
        assert_eq!(c.string(), Err(IoError::UnterminatedString { offset: 6 }));
        assert_eq!(c.pos(), 6, "a failed read does not advance");
    }

    #[test]
    fn eof_is_an_error_not_a_panic() {
        let mut c = Cursor::new(&[1]);
        assert_eq!(
            c.u16(),
            Err(IoError::Eof {
                offset: 0,
                needed: 2,
                remaining: 1
            })
        );
        assert!(Cursor::new(&[]).smart().is_err());
        assert!(Cursor::new(&[0x80]).signed_smart().is_err());
        assert!(Cursor::at(&[0; 4], 5).is_err());
    }

    /// Encode as the original client's writer does, for the round-trip
    /// properties.
    fn encode_smart(v: u16) -> Vec<u8> {
        if v < 128 {
            vec![v as u8]
        } else {
            (v + 32768).to_be_bytes().to_vec()
        }
    }

    fn encode_signed_smart(v: i16) -> Vec<u8> {
        if (-64..64).contains(&v) {
            vec![(v + 64) as u8]
        } else {
            ((i32::from(v) + 49152) as u16).to_be_bytes().to_vec()
        }
    }

    proptest! {
        #[test]
        fn smart_round_trips(v in 0u16..=32767) {
            let bytes = encode_smart(v);
            let mut c = Cursor::new(&bytes);
            prop_assert_eq!(c.smart().unwrap(), v);
            prop_assert_eq!(c.remaining(), 0);
        }

        #[test]
        fn signed_smart_round_trips(v in -16384i16..=16383) {
            let bytes = encode_signed_smart(v);
            let mut c = Cursor::new(&bytes);
            prop_assert_eq!(c.signed_smart().unwrap(), v);
            prop_assert_eq!(c.remaining(), 0);
        }

        #[test]
        fn reads_never_panic(data in proptest::collection::vec(any::<u8>(), 0..16)) {
            let mut c = Cursor::new(&data);
            while c.remaining() > 0 {
                if c.signed_smart().is_err() || c.string().is_err() {
                    break;
                }
            }
            let mut c = Cursor::new(&data);
            let _ = (c.u24(), c.i32(), c.smart(), c.u16());
        }
    }
}
