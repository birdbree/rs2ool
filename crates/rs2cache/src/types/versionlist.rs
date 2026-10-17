//! The versionlist archive's per-file tables.
//!
//! Entry `n` of a table belongs to file id `n` of its index.

use serde::{Deserialize, Serialize};

/// Version and CRC tables for one of indices 1–4.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTable {
    /// The 2-byte version stored at the end of each file.
    pub versions: Vec<u16>,
    /// CRC-32 of each stored file, not counting its 2-byte version trailer.
    pub crcs: Vec<u32>,
}

/// Every table in the versionlist archive except `map_index`, which is
/// decoded with the maps.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionList {
    /// Index 1 (`model_version`, `model_crc`).
    pub models: FileTable,
    /// Index 2 (`anim_version`, `anim_crc`).
    pub anims: FileTable,
    /// Index 3 (`midi_version`, `midi_crc`).
    pub midis: FileTable,
    /// Index 4 (`map_version`, `map_crc`).
    pub maps: FileTable,
    /// One byte per model, kept raw; no meaning is attached.
    pub model_index: Vec<u8>,
    /// One `u16` per global frame id. The original client only uses its
    /// length. It is **not** a frame → pack map; that's
    /// [`FrameIndex`](crate::anim::FrameIndex).
    pub anim_index: Vec<u16>,
    /// One byte per MIDI file, kept raw.
    pub midi_index: Vec<u8>,
}

impl VersionList {
    /// The table for index 1–4, or `None` for any other index.
    pub fn table(&self, index: u8) -> Option<&FileTable> {
        match index {
            1 => Some(&self.models),
            2 => Some(&self.anims),
            3 => Some(&self.midis),
            4 => Some(&self.maps),
            _ => None,
        }
    }
}
