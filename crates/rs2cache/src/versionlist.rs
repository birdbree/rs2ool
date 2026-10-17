//! The versionlist archive (index 0, archive 5).
//!
//! `<type>_version` is a `u16` per file and `<type>_crc` an `i32` per file,
//! for types model, anim, midi and map (indices 1–4). Every stored file in
//! those indices ends with its 2-byte version, and its CRC32 is computed
//! over the bytes before it. `model_index` and `midi_index` are a byte per
//! file; `anim_index` a `u16` per global frame id.

use rs2intermediate::versionlist::{FileTable, VersionList};

use crate::archive::Archive;
use crate::io::{Cursor, IoError};

fn u16s(data: &[u8]) -> Result<Vec<u16>, IoError> {
    let mut c = Cursor::new(data);
    (0..data.len() / 2).map(|_| c.u16()).collect()
}

fn u32s(data: &[u8]) -> Result<Vec<u32>, IoError> {
    let mut c = Cursor::new(data);
    (0..data.len() / 4).map(|_| c.u32()).collect()
}

/// Decode the versionlist tables. Missing entries decode as empty tables.
pub fn decode(archive: &Archive) -> Result<VersionList, IoError> {
    let get = |name: &str| archive.get(name).unwrap_or_default();
    let table = |kind: &str| -> Result<FileTable, IoError> {
        Ok(FileTable {
            versions: u16s(get(&format!("{kind}_version")))?,
            crcs: u32s(get(&format!("{kind}_crc")))?,
        })
    };
    Ok(VersionList {
        models: table("model")?,
        anims: table("anim")?,
        midis: table("midi")?,
        maps: table("map")?,
        model_index: get("model_index").to_vec(),
        anim_index: u16s(get("anim_index"))?,
        midi_index: get("midi_index").to_vec(),
    })
}

/// A stored file's 2-byte version trailer and the CRC32 of the bytes before
/// it, or `None` for a file shorter than 2 bytes.
pub fn file_version_and_crc(raw: &[u8]) -> Option<(u16, u32)> {
    let n = raw.len().checked_sub(2)?;
    let version = u16::from_be_bytes([raw[n], raw[n + 1]]);
    Some((version, crc32fast::hash(&raw[..n])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_trailer_and_crc() {
        let raw = [b'a', b'b', b'c', 0x01, 0x02];
        assert_eq!(file_version_and_crc(&raw), Some((0x0102, 0x352441c2)));
        assert_eq!(file_version_and_crc(&[1]), None);
    }
}
