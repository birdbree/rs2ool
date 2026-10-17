//! Jag archives: the files of index 0.
//!
//! ```text
//! uncompressed   u24
//! compressed     u24
//! body           the rest of the file
//!
//! body (after bunzip if the archive is packed as a whole)
//!   count        u16
//!   per entry:
//!     nameHash       i32   see jag_hash
//!     uncompressed   u24
//!     compressed     u24
//!   entry data, in table order, each taking its compressed size
//! ```
//!
//! When the two header sizes differ, the body is one headerless bzip2 stream
//! and the entries inside it are stored plain. When they match, the body is
//! plain and every entry is bzip2'd on its own. Entries are found by the
//! hash of their name; the names themselves aren't stored.

use std::fmt;

use thiserror::Error;

use crate::compress::{self, CompressError};
use crate::io::{Cursor, IoError};

/// Title archive: title-screen images and the four fonts.
pub const TITLE: u8 = 1;
/// Config archive: `loc.dat`, `npc.dat`, `seq.dat` and the rest.
pub const CONFIG: u8 = 2;
/// Interface archive: the single `data` entry.
pub const INTERFACE: u8 = 3;
/// Media archive: game-screen sprites.
pub const MEDIA: u8 = 4;
/// Versionlist archive: per-file versions, CRCs and `map_index`.
pub const VERSIONLIST: u8 = 5;
/// Textures archive: `0.dat` to `49.dat` plus `index.dat`.
pub const TEXTURES: u8 = 6;
/// Word-filter archive.
pub const WORDENC: u8 = 7;
/// Sound effects archive.
pub const SOUNDS: u8 = 8;

/// `(number, name)` for every archive in index 0. Archive 0 is unused.
pub const ARCHIVES: &[(u8, &str)] = &[
    (TITLE, "title"),
    (CONFIG, "config"),
    (INTERFACE, "interface"),
    (MEDIA, "media"),
    (VERSIONLIST, "versionlist"),
    (TEXTURES, "textures"),
    (WORDENC, "wordenc"),
    (SOUNDS, "sounds"),
];

/// Look up an archive number by name (`"config"`) or number (`"2"`).
pub fn archive_by_name(name: &str) -> Option<u8> {
    if let Ok(n) = name.parse::<u8>() {
        return ARCHIVES.iter().any(|&(a, _)| a == n).then_some(n);
    }
    ARCHIVES
        .iter()
        .find(|(_, s)| s.eq_ignore_ascii_case(name))
        .map(|&(a, _)| a)
}

/// The name of archive number `archive` (`2` gives `"config"`).
pub fn archive_name(archive: u8) -> Option<&'static str> {
    ARCHIVES
        .iter()
        .find(|&&(a, _)| a == archive)
        .map(|&(_, s)| s)
}

const TEXTURE_NAMES: [&str; 51] = [
    "0.dat",
    "1.dat",
    "2.dat",
    "3.dat",
    "4.dat",
    "5.dat",
    "6.dat",
    "7.dat",
    "8.dat",
    "9.dat",
    "10.dat",
    "11.dat",
    "12.dat",
    "13.dat",
    "14.dat",
    "15.dat",
    "16.dat",
    "17.dat",
    "18.dat",
    "19.dat",
    "20.dat",
    "21.dat",
    "22.dat",
    "23.dat",
    "24.dat",
    "25.dat",
    "26.dat",
    "27.dat",
    "28.dat",
    "29.dat",
    "30.dat",
    "31.dat",
    "32.dat",
    "33.dat",
    "34.dat",
    "35.dat",
    "36.dat",
    "37.dat",
    "38.dat",
    "39.dat",
    "40.dat",
    "41.dat",
    "42.dat",
    "43.dat",
    "44.dat",
    "45.dat",
    "46.dat",
    "47.dat",
    "48.dat",
    "49.dat",
    "index.dat",
];

/// Entry names the original client looks up, per archive, plus a few found
/// elsewhere. Entries whose hash isn't in this table can still be read by
/// hash.
pub fn known_names(archive: u8) -> &'static [&'static str] {
    match archive {
        TITLE => &[
            "title.dat",
            "index.dat",
            "logo.dat",
            "titlebox.dat",
            "titlebutton.dat",
            "runes.dat",
            "p11_full.dat",
            "p12_full.dat",
            "b12_full.dat",
            "q8_full.dat",
        ],
        CONFIG => &[
            "loc.dat",
            "loc.idx",
            "npc.dat",
            "npc.idx",
            "obj.dat",
            "obj.idx",
            "idk.dat",
            "flo.dat",
            "seq.dat",
            "spotanim.dat",
            "varp.dat",
            "varbit.dat",
            // Present in the archive but never read by the original client.
            "idk.idx",
            "flo.idx",
            "seq.idx",
            "spotanim.idx",
            "varp.idx",
            "varbit.idx",
            "mesanim.dat",
            "mesanim.idx",
            "mes.dat",
            "mes.idx",
            "param.dat",
            "param.idx",
        ],
        INTERFACE => &["data"],
        MEDIA => &[
            "index.dat",
            "invback.dat",
            "chatback.dat",
            "mapback.dat",
            "backbase1.dat",
            "backbase2.dat",
            "backhmid1.dat",
            "backhmid2.dat",
            "backleft1.dat",
            "backleft2.dat",
            "backright1.dat",
            "backright2.dat",
            "backtop1.dat",
            "backvmid1.dat",
            "backvmid2.dat",
            "backvmid3.dat",
            "sideicons.dat",
            "compass.dat",
            "mapedge.dat",
            "mapfunction.dat",
            "mapmarker.dat",
            "mapscene.dat",
            "mapdots.dat",
            "hitmarks.dat",
            "headicons_hint.dat",
            "headicons_pk.dat",
            "headicons_prayer.dat",
            "mod_icons.dat",
            "overlay_multiway.dat",
            "cross.dat",
            "scrollbar.dat",
            "redstone1.dat",
            "redstone2.dat",
            "redstone3.dat",
            // Named by interface sprite references (`name,index`).
            "button_brown_big.dat",
            "button_red.dat",
            "chest.dat",
            "coins.dat",
            "combatboxes.dat",
            "combaticons.dat",
            "combaticons2.dat",
            "combaticons3.dat",
            "key.dat",
            "keys.dat",
            "leftarrow.dat",
            "leftarrow_small.dat",
            "letter.dat",
            "magicoff.dat",
            "magicoff2.dat",
            "magicon.dat",
            "magicon2.dat",
            "miscgraphics.dat",
            "miscgraphics2.dat",
            "miscgraphics3.dat",
            "number_button.dat",
            "overlay_duel.dat",
            "prayerglow.dat",
            "prayeroff.dat",
            "prayeron.dat",
            "rightarrow.dat",
            "rightarrow_small.dat",
            "startgame.dat",
            "staticons.dat",
            "staticons2.dat",
            "steelborder.dat",
            "steelborder2.dat",
            "sworddecor.dat",
            "tex_brown.dat",
            "tex_red.dat",
            "titlescroll.dat",
            "tradebacking.dat",
            "wornicons.dat",
            // Recovered by inverting the hash.
            "pen.dat",
        ],
        VERSIONLIST => &[
            "model_version",
            "anim_version",
            "midi_version",
            "map_version",
            "model_crc",
            "anim_crc",
            "midi_crc",
            "map_crc",
            "model_index",
            "anim_index",
            "midi_index",
            "map_index",
        ],
        TEXTURES => &TEXTURE_NAMES,
        WORDENC => &[
            "fragmentsenc.txt",
            "badenc.txt",
            "domainenc.txt",
            "tldlist.txt",
        ],
        SOUNDS => &["sounds.dat"],
        _ => &[],
    }
}

/// The jag name hash: uppercase, then `h = h * 61 + (c - 32)` with wrapping
/// 32-bit arithmetic.
pub fn jag_hash(name: &str) -> i32 {
    name.bytes().fold(0i32, |h, c| {
        h.wrapping_mul(61)
            .wrapping_add(i32::from(c.to_ascii_uppercase()))
            .wrapping_sub(32)
    })
}

/// How an archive entry is addressed: by name (hashed) or by raw hash.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EntryName {
    /// A name such as `"loc.dat"`; looked up by its [`jag_hash`].
    Named(String),
    /// A raw name hash, for entries whose name isn't known.
    Hash(i32),
}

impl EntryName {
    /// The name hash this refers to.
    pub fn hash(&self) -> i32 {
        match self {
            Self::Named(n) => jag_hash(n),
            Self::Hash(h) => *h,
        }
    }

    /// Parse a CLI-style entry: `#123` or `#-45` is a hash, anything else a
    /// name.
    pub fn parse(s: &str) -> Self {
        match s.strip_prefix('#').map(str::parse) {
            Some(Ok(h)) => Self::Hash(h),
            _ => Self::Named(s.to_owned()),
        }
    }
}

impl fmt::Display for EntryName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(n) => f.write_str(n),
            Self::Hash(h) => write!(f, "#{h}"),
        }
    }
}

/// An archive that wouldn't decode.
#[derive(Debug, Error)]
pub enum ArchiveError {
    /// The header, table or an entry's data ran out.
    #[error("truncated archive: {0}")]
    Truncated(#[from] IoError),
    /// A bzip2 stream was corrupt.
    #[error(transparent)]
    Compress(#[from] CompressError),
    /// The whole-archive stream inflated to the wrong size.
    #[error("whole-archive bzip2 inflated to {got} bytes, header says {expected}")]
    WholeSize {
        /// Size from the archive header.
        expected: usize,
        /// Size actually produced.
        got: usize,
    },
    /// An entry came out a different size from what the table says.
    #[error("entry {hash}: inflated to {got} bytes, table says {expected}")]
    EntrySize {
        /// The entry's name hash.
        hash: i32,
        /// Uncompressed size from the table.
        expected: usize,
        /// Size actually produced.
        got: usize,
    },
}

/// One decoded entry.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The entry's name hash, as stored.
    pub hash: i32,
    /// The name, when its hash is in [`known_names`].
    pub name: Option<&'static str>,
    /// Uncompressed size from the table.
    pub uncompressed_size: u32,
    /// Stored size from the table.
    pub compressed_size: u32,
    /// Whether this entry was bzip2'd on its own.
    pub compressed: bool,
    /// The unpacked bytes.
    pub data: Vec<u8>,
}

impl Entry {
    /// The entry's name if known, otherwise its hash.
    pub fn entry_name(&self) -> EntryName {
        match self.name {
            Some(n) => EntryName::Named(n.to_owned()),
            None => EntryName::Hash(self.hash),
        }
    }
}

/// A decoded jag archive: every entry, unpacked.
#[derive(Debug, Clone)]
pub struct Archive {
    /// Whether the archive was bzip2'd as a whole.
    pub whole_compressed: bool,
    /// Entries in table order.
    pub entries: Vec<Entry>,
}

impl Archive {
    /// Decode an archive. `archive` selects the known-name table; pass
    /// `None` to leave every entry unnamed.
    pub fn decode(data: &[u8], archive: Option<u8>) -> Result<Self, ArchiveError> {
        let mut c = Cursor::new(data);
        // Header sizes: equal means "body stored plain".
        let uncompressed = c.u24()? as usize;
        let compressed = c.u24()? as usize;
        let whole_compressed = uncompressed != compressed;
        // Declared out here so `body` can borrow it.
        let inflated;
        let body: &[u8] = if whole_compressed {
            inflated = compress::bunzip(&data[c.pos()..])?;
            if inflated.len() != uncompressed {
                return Err(ArchiveError::WholeSize {
                    expected: uncompressed,
                    got: inflated.len(),
                });
            }
            &inflated
        } else {
            &data[c.pos()..]
        };

        let names = archive.map(known_names).unwrap_or_default();
        let mut c = Cursor::new(body);
        let count = c.u16()?;
        let mut table = Vec::with_capacity(count.into());
        for _ in 0..count {
            // (name hash, uncompressed size, stored size)
            table.push((c.i32()?, c.u24()?, c.u24()?));
        }
        let mut entries = Vec::with_capacity(table.len());
        // Entry data starts right after the table and runs in table order.
        for (hash, usize_, csize) in table {
            let stored = c.bytes(csize as usize)?;
            let data = if whole_compressed {
                // Stored plain; like the original client, take the
                // uncompressed size from the front of the stored bytes.
                Cursor::new(stored).bytes(usize_ as usize)?.to_vec()
            } else {
                compress::bunzip(stored)?
            };
            if data.len() != usize_ as usize {
                return Err(ArchiveError::EntrySize {
                    hash,
                    expected: usize_ as usize,
                    got: data.len(),
                });
            }
            entries.push(Entry {
                hash,
                name: names.iter().copied().find(|n| jag_hash(n) == hash),
                uncompressed_size: usize_,
                compressed_size: csize,
                compressed: !whole_compressed,
                data,
            });
        }
        Ok(Self {
            whole_compressed,
            entries,
        })
    }

    /// The entry a name or hash refers to.
    pub fn entry(&self, name: &EntryName) -> Option<&Entry> {
        let hash = name.hash();
        self.entries.iter().find(|e| e.hash == hash)
    }

    /// An entry's bytes by name.
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.entry(&EntryName::Named(name.to_owned()))
            .map(|e| e.data.as_slice())
    }
}

#[cfg(test)]
pub(crate) mod synth {
    use super::*;
    use crate::compress::synth::bzip;

    fn table(entries: &[(&str, Vec<u8>)], stored: &[Vec<u8>]) -> Vec<u8> {
        let mut out = (entries.len() as u16).to_be_bytes().to_vec();
        for ((name, data), s) in entries.iter().zip(stored) {
            out.extend_from_slice(&jag_hash(name).to_be_bytes());
            out.extend_from_slice(&(data.len() as u32).to_be_bytes()[1..]);
            out.extend_from_slice(&(s.len() as u32).to_be_bytes()[1..]);
        }
        for s in stored {
            out.extend_from_slice(s);
        }
        out
    }

    fn header(u: usize, c: usize) -> Vec<u8> {
        let mut out = (u as u32).to_be_bytes()[1..].to_vec();
        out.extend_from_slice(&(c as u32).to_be_bytes()[1..]);
        out
    }

    /// An archive whose entries are each bzip2'd.
    pub fn per_entry(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let stored: Vec<_> = entries.iter().map(|(_, d)| bzip(d)).collect();
        let body = table(entries, &stored);
        let mut out = header(body.len(), body.len());
        out.extend(body);
        out
    }

    /// An archive bzip2'd as a whole, entries stored plain.
    pub fn whole(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let stored: Vec<_> = entries.iter().map(|(_, d)| d.clone()).collect();
        let body = table(entries, &stored);
        let packed = bzip(&body);
        let mut out = header(body.len(), packed.len());
        out.extend(packed);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_known_constants() {
        // Computed independently from the original client's formula.
        assert_eq!(jag_hash("loc.dat"), 682_978_269);
        assert_eq!(jag_hash("obj.idx"), -1_667_598_946);
        assert_eq!(jag_hash("loc.dat"), jag_hash("LOC.DAT"));
        assert_eq!(jag_hash(""), 0);
    }

    #[test]
    fn hash_wraps_on_long_names() {
        // Long enough to overflow i32 several times; must not panic.
        let _ = jag_hash("headicons_prayer.dat_and_some_more_characters");
    }

    fn sample() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("loc.dat", b"locations".repeat(30)),
            ("npc.dat", vec![]),
            ("unknown.bin", (0..=255).collect()),
        ]
    }

    fn check(a: &Archive, whole: bool) {
        assert_eq!(a.whole_compressed, whole);
        assert_eq!(a.entries.len(), 3);
        for (name, data) in sample() {
            assert_eq!(a.get(name).unwrap(), data, "{name}");
        }
        assert_eq!(a.entries[0].name, Some("loc.dat"));
        assert_eq!(a.entries[2].name, None);
        assert_eq!(
            a.entries[2].entry_name(),
            EntryName::Hash(jag_hash("unknown.bin"))
        );
        assert_eq!(a.entries[0].compressed, !whole);
    }

    #[test]
    fn per_entry_bzip2() {
        check(
            &Archive::decode(&synth::per_entry(&sample()), Some(CONFIG)).unwrap(),
            false,
        );
    }

    #[test]
    fn whole_archive_bzip2() {
        check(
            &Archive::decode(&synth::whole(&sample()), Some(CONFIG)).unwrap(),
            true,
        );
    }

    #[test]
    fn truncated_archives_are_errors() {
        let full = synth::per_entry(&sample());
        for len in [0, 5, 7, 20, full.len() - 1] {
            assert!(Archive::decode(&full[..len], None).is_err(), "len {len}");
        }
    }

    #[test]
    fn entry_names_parse() {
        assert_eq!(EntryName::parse("#-12"), EntryName::Hash(-12));
        assert_eq!(
            EntryName::parse("loc.dat"),
            EntryName::Named("loc.dat".into())
        );
        assert_eq!(EntryName::parse("#x"), EntryName::Named("#x".into()));
        assert_eq!(archive_by_name("Config"), Some(CONFIG));
        assert_eq!(archive_by_name("5"), Some(VERSIONLIST));
        assert_eq!(archive_by_name("0"), None);
    }
}
