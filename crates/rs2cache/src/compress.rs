//! The two codecs a 317 cache uses: gzip for index 1–4 files and headerless
//! bzip2 inside jag archives.

use std::io::Read;

use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use thiserror::Error;

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];
/// Jag archives strip the 4-byte bzip2 stream header (`BZh` plus the block
/// size digit); the client always compresses with 100 kB blocks.
const BZIP2_HEADER: &[u8; 4] = b"BZh1";

#[derive(Debug, Error)]
pub enum CompressError {
    #[error("gzip stream is corrupt: {0}")]
    Gzip(std::io::Error),
    #[error("bzip2 stream is corrupt: {0}")]
    Bzip2(std::io::Error),
}

/// Which codec a stored file used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    Gzip,
    Bzip2,
}

pub fn is_gzip(data: &[u8]) -> bool {
    data.starts_with(&GZIP_MAGIC)
}

/// Gunzip `data` if it starts with the gzip magic, else return it unchanged.
pub fn inflate(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    if !is_gzip(data) {
        return Ok(data.to_vec());
    }
    let mut out = Vec::with_capacity(data.len() * 4);
    GzDecoder::new(data)
        .read_to_end(&mut out)
        .map_err(CompressError::Gzip)?;
    Ok(out)
}

/// Decompress a headerless bzip2 stream as stored in jag archives.
pub fn bunzip(data: &[u8]) -> Result<Vec<u8>, CompressError> {
    let mut out = Vec::with_capacity(data.len() * 4);
    BzDecoder::new(BZIP2_HEADER.chain(data))
        .read_to_end(&mut out)
        .map_err(CompressError::Bzip2)?;
    Ok(out)
}

#[cfg(test)]
pub(crate) mod synth {
    use std::io::Write;

    pub fn gzip(data: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    /// Bzip2 with the 4-byte header stripped, as jag archives store it.
    pub fn bzip(data: &[u8]) -> Vec<u8> {
        let mut e = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::new(1));
        e.write_all(data).unwrap();
        let mut out = e.finish().unwrap();
        assert_eq!(&out[..4], b"BZh1");
        out.drain(..4);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inflate_passes_through_non_gzip() {
        assert_eq!(inflate(b"plain").unwrap(), b"plain");
        assert_eq!(inflate(b"").unwrap(), b"");
    }

    #[test]
    fn inflate_round_trips() {
        let data: Vec<u8> = (0..5000u32).map(|i| (i * 7 % 251) as u8).collect();
        assert_eq!(inflate(&synth::gzip(&data)).unwrap(), data);
    }

    #[test]
    fn bunzip_round_trips_headerless_streams() {
        let data = b"loc.dat loc.idx npc.dat".repeat(40);
        assert_eq!(bunzip(&synth::bzip(&data)).unwrap(), data);
    }

    #[test]
    fn corrupt_streams_are_errors() {
        let mut gz = synth::gzip(b"hello world, hello world");
        gz.truncate(gz.len() - 6);
        assert!(matches!(inflate(&gz), Err(CompressError::Gzip(_))));
        assert!(matches!(bunzip(b"not bzip2"), Err(CompressError::Bzip2(_))));
    }
}
