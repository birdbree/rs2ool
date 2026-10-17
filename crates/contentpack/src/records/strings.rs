//! `strings` record: one table of UTF-8 strings for one domain and locale.
//!
//! ```text
//! count     u32
//! offsets   (count + 1) × u32   byte offsets into data
//! data      offsets[count] bytes of UTF-8
//! ```
//!
//! String `i` is `data[offsets[i]..offsets[i + 1]]`, unterminated. Yes,
//! there are `count + 1` offsets: the leading 0 is stored too, and the last
//! one is the length of `data`.
//!
//! The record id is `domain << 8 | locale`. Locale 0 is English; domain 1
//! is floor names and domain 2 is object names and options, so the two
//! tables in use are `0x100` and `0x200`. Other records point at a string
//! by its index in the table.

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

/// Encode a string table. Strings go back to back in order, so the
/// offsets never decrease.
pub fn encode(strings: &[String]) -> Vec<u8> {
    let mut w = Writer::default();
    w.u32(strings.len() as u32);
    // Running end offset of the data written so far.
    let mut at = 0u32;
    w.u32(0);
    for s in strings {
        at += s.len() as u32;
        w.u32(at);
    }
    for s in strings {
        w.bytes(s.as_bytes());
    }
    w.0
}

/// Decode a string table.
///
/// Each string's range has to sit inside the data and not run backwards,
/// and be valid UTF-8. `offsets[0]` doesn't have to be 0.
pub fn decode(data: &[u8]) -> Result<Vec<String>, RecordError> {
    let mut r = Reader::new(data);
    let n = r.u32()? as usize;
    let offsets = (0..=n)
        .map(|_| r.u32().map(|v| v as usize))
        .collect::<Result<Vec<_>, _>>()?;
    // The last offset is the data length.
    let text = r.bytes(offsets.last().copied().unwrap_or(0))?;
    finish(&r)?;
    // Each adjacent offset pair brackets one string.
    offsets
        .windows(2)
        .map(|w| {
            let s = text
                .get(w[0]..w[1])
                .ok_or_else(|| RecordError::Invalid("string offsets".into()))?;
            String::from_utf8(s.to_vec())
                .map_err(|_| RecordError::Invalid("string is not UTF-8".into()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = vec!["grass".to_owned(), String::new(), "café".to_owned()];
        assert_eq!(decode(&encode(&s)).unwrap(), s);
        assert_eq!(decode(&encode(&[])).unwrap(), Vec::<String>::new());
    }
}
