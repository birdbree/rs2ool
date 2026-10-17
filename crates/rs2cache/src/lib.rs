//! Reader for the RuneScape 2 revision-317 cache: sector chains, jag
//! archives, and every record decoder. Everything decoded is returned as an
//! `rs2intermediate` struct.

pub mod cache;
pub mod io;

pub use cache::{Cache, CacheError, IndexEntry, SectorFault, SectorVisit};
