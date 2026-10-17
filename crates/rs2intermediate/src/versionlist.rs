//! The versionlist archive's per-file tables.

use serde::{Deserialize, Serialize};

/// Version and CRC tables for one of indices 1–4.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileTable {
    /// The 2-byte version stored at the end of each file.
    pub versions: Vec<u16>,
    /// CRC32 of each stored file, excluding its 2-byte version trailer.
    pub crcs: Vec<u32>,
}

/// Every table in the versionlist archive except `map_index`, which is
/// decoded with the maps.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionList {
    pub models: FileTable,
    pub anims: FileTable,
    pub midis: FileTable,
    pub maps: FileTable,
    /// One flag byte per model.
    pub model_index: Vec<u8>,
    /// One `u16` per global frame id; the client uses only its length.
    pub anim_index: Vec<u16>,
    /// One byte per MIDI file.
    pub midi_index: Vec<u8>,
}

impl VersionList {
    /// The table for index 1–4.
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
