//! `strings` record: one string table per domain and locale (record id
//! `domain << 8 | locale`; locale 0 is English, domain 1 floors, 2 objects). `count u32`, `offsets u32 × (count + 1)` into the UTF-8 data
//! that follows (string `i` is `data[offsets[i]..offsets[i + 1]]`).

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish};

pub fn encode(strings: &[String]) -> Vec<u8> {
    let mut w = Writer::default();
    w.u32(strings.len() as u32);
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

pub fn decode(data: &[u8]) -> Result<Vec<String>, RecordError> {
    let mut r = Reader::new(data);
    let n = r.u32()? as usize;
    let offsets = (0..=n)
        .map(|_| r.u32().map(|v| v as usize))
        .collect::<Result<Vec<_>, _>>()?;
    let text = r.bytes(offsets.last().copied().unwrap_or(0))?;
    finish(&r)?;
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
