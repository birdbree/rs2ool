//! The content-pack format: the container file, the record encoders and
//! decoders, and the colour tools used to build a palette.
//!
//! A pack is one file holding records of a single [`Kind`]. Each record is
//! an opaque byte string keyed by a `u32` id; the container sorts them by
//! id, compresses each one on its own and keeps a CRC32 of each. The file's
//! named by the SHA-256 of its index and blob.
//!
//! Everything here is little-endian, which is the opposite of the 317
//! cache, so don't reach for a cache reader's integer helpers. Writing is
//! deterministic: records are sorted by id, the manifest comes from ordered
//! maps, and nothing looks at the clock. Bake the same records twice and
//! you get the same bytes and the same name.
//!
//! # Modules
//!
//! - [`pack`]: the container. [`PackWriter`] collects records and bakes
//!   them, [`PackReader`] opens a pack and hands records back checked.
//! - [`records`]: one module per record kind, each with an `encode` and a
//!   `decode` that round-trip.
//! - [`loose`]: loose directories, an unbaked pack as `pack.json` plus one
//!   `<id>.bin` per record.
//! - [`palette`]: 317 HSL to RGB, as the original client converts it, plus
//!   the nearest-colour and k-means helpers a palette is built with.
//! - [`bytes`]: the little-endian [`Writer`](bytes::Writer) and
//!   [`Reader`](bytes::Reader) every record uses.
//!
//! # Example
//!
//! ```no_run
//! use contentpack::records::material::{self, Material};
//! use contentpack::{Kind, PackReader, PackWriter};
//!
//! // A materials pack with a two-colour palette and the palette material.
//! let mut w = PackWriter::new(Kind::Materials, 0);
//! w.insert(material::PALETTE_ID, material::encode_palette(&[[0, 0, 0], [255, 255, 255]]));
//! let m = Material {
//!     id: 0,
//!     mode: material::mode::PALETTE,
//!     texture: 0,
//!     period: 0,
//!     palette: 0,
//!     roughness: 255,
//!     flags: 0,
//!     alpha: 255,
//! };
//! w.insert(0, m.encode());
//! let path = w.write_to(std::path::Path::new("packs"))?;
//!
//! // Read it back. `get` decompresses and checks the CRC.
//! let r = PackReader::open(&path)?;
//! let bytes = r.get(0).expect("record 0 is there")?;
//! assert_eq!(Material::decode(&bytes)?, m);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(missing_docs)]

pub mod bytes;
pub mod loose;
pub mod pack;
pub mod palette;
pub mod records;

pub use pack::{Codec, Kind, PackError, PackManifest, PackReader, PackWriter};
