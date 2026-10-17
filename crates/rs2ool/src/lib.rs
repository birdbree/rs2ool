//! `rs2ool`'s commands as a library: [`cache`] lists, dumps and verifies a
//! cache, and [`explorer`] is the GUI. The binary maps its command line onto
//! them, and the explorer's exports call [`cache`], so the GUI writes the
//! same files the CLI does.

pub mod cache;
#[cfg(feature = "explorer")]
pub mod explorer;
