//! The client palette, built from every model face colour in the cache.

use std::collections::BTreeMap;

use rayon::prelude::*;
use rs2cache::Records;
use rs2intermediate::model::Model;
use rs2intermediate::record::{Record, RecordType};
use rs2pack::palette::{hsl_to_rgb, kmeans, nearest, rgb_distance, split};
use rs2pack::records::material::{self, Material, mode};
use rs2pack::{Kind, PackWriter};
use serde::Serialize;

use crate::{ConvertError, material_id};

/// Entries in the client palette (indices are `u16`).
pub const PALETTE_SIZE: usize = 1024;
const ITERATIONS: usize = 100;

fn rgbf(rgb: u32) -> [f64; 3] {
    split(rgb).map(f64::from)
}

/// Whether face `i` of a model is textured.
pub fn is_textured(m: &Model, i: usize) -> bool {
    m.face_types.as_ref().is_some_and(|t| t[i] & 2 != 0)
}

/// Every untextured face colour in the cache, with how many faces use it.
pub fn corpus(records: &Records) -> Result<BTreeMap<u16, u64>, ConvertError> {
    let ids = records.ids(RecordType::Model)?;
    let parts: Vec<BTreeMap<u16, u64>> = ids
        .par_iter()
        .map(|&id| {
            let mut counts = BTreeMap::new();
            if let Ok(Record::Model(m)) = records.decode(RecordType::Model, id) {
                for (i, &c) in m.face_colours.iter().enumerate() {
                    if !is_textured(&m, i) {
                        *counts.entry(c).or_insert(0) += 1;
                    }
                }
            }
            counts
        })
        .collect();
    let mut out = BTreeMap::new();
    for p in parts {
        for (c, n) in p {
            *out.entry(c).or_insert(0) += n;
        }
    }
    Ok(out)
}

/// The palette and a lookup from 317 HSL to palette index.
#[derive(Debug, Clone)]
pub struct Palette {
    pub rgb: Vec<[u8; 3]>,
    rgbf: Vec<[f64; 3]>,
    lookup: BTreeMap<u16, u16>,
}

/// Quantisation error over the corpus, in RGB units.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct PaletteError {
    pub colours: usize,
    pub faces: u64,
    /// Mean over faces.
    pub mean: f64,
    /// Worst distinct colour.
    pub max: f64,
    pub worst_hsl: u16,
}

impl Palette {
    /// Quantise the corpus to [`PALETTE_SIZE`] entries: k-means in RGB,
    /// each colour weighted by the square root of its face count so rare
    /// colours are not swallowed by common ones.
    pub fn build(corpus: &BTreeMap<u16, u64>) -> Self {
        let points: Vec<([f64; 3], f64)> = corpus
            .iter()
            .map(|(&c, &n)| (rgbf(hsl_to_rgb(c)), (n as f64).sqrt()))
            .collect();
        let centres = kmeans(&points, PALETTE_SIZE, ITERATIONS);
        let mut rgb: Vec<[u8; 3]> = centres
            .iter()
            .map(|c| c.map(|v| v.round().clamp(0.0, 255.0) as u8))
            .collect();
        // Sort entries so the palette's order does not depend on seeding.
        rgb.sort_unstable();
        rgb.dedup();
        Self::from_rgb(rgb)
    }

    pub fn from_rgb(rgb: Vec<[u8; 3]>) -> Self {
        let rgbf = rgb.iter().map(|c| c.map(f64::from)).collect();
        Self {
            rgb,
            rgbf,
            lookup: BTreeMap::new(),
        }
    }

    /// The palette index for a 317 HSL colour, memoised per colour.
    pub fn index(&mut self, hsl: u16) -> u16 {
        if let Some(&i) = self.lookup.get(&hsl) {
            return i;
        }
        let i = self.index_rgb(hsl_to_rgb(hsl));
        self.lookup.insert(hsl, i);
        i
    }

    /// Nearest palette index without memoising.
    pub fn index_of(&self, hsl: u16) -> u16 {
        self.lookup
            .get(&hsl)
            .copied()
            .unwrap_or_else(|| self.index_rgb(hsl_to_rgb(hsl)))
    }

    /// Nearest palette index for a 24-bit RGB colour.
    pub fn index_rgb(&self, rgb: u32) -> u16 {
        nearest(&self.rgbf, rgbf(rgb)) as u16
    }

    /// Fill the lookup for every corpus colour, in parallel.
    pub fn prime(&mut self, corpus: &BTreeMap<u16, u64>) {
        let found: Vec<(u16, u16)> = corpus
            .keys()
            .copied()
            .collect::<Vec<_>>()
            .par_iter()
            .map(|&c| (c, self.index_of(c)))
            .collect();
        self.lookup.extend(found);
    }

    pub fn error(&self, corpus: &BTreeMap<u16, u64>) -> PaletteError {
        let mut e = PaletteError {
            colours: corpus.len(),
            faces: 0,
            mean: 0.0,
            max: 0.0,
            worst_hsl: 0,
        };
        let mut sum = 0.0;
        for (&c, &n) in corpus {
            let orig = split(hsl_to_rgb(c));
            let d = rgb_distance(orig, self.rgb[usize::from(self.index_of(c))]);
            sum += d * n as f64;
            e.faces += n;
            if d > e.max {
                e.max = d;
                e.worst_hsl = c;
            }
        }
        e.mean = if e.faces > 0 {
            sum / e.faces as f64
        } else {
            0.0
        };
        e
    }

    /// The `materials` pack: the palette record and the default palette
    /// material.
    pub fn pack(&self, source_cache_crc: u32) -> PackWriter {
        let mut w = PackWriter::new(Kind::Materials, source_cache_crc);
        w.insert(material::PALETTE_ID, material::encode_palette(&self.rgb));
        let m = Material {
            id: material_id::PALETTE,
            mode: mode::PALETTE,
            texture: 0,
            period: 0,
            palette: 0,
            roughness: 255,
            flags: 0,
            alpha: 255,
        };
        w.insert(m.id.into(), m.encode());
        w
    }

    /// A swatch image: 32 × 32 cells of `cell` pixels, RGBA.
    pub fn swatch(&self, cell: usize) -> (u32, u32, Vec<u8>) {
        let side = 32 * cell;
        let mut out = vec![0u8; side * side * 4];
        for (i, c) in self.rgb.iter().enumerate() {
            let (cx, cy) = ((i % 32) * cell, (i / 32) * cell);
            for y in cy..cy + cell {
                for x in cx..cx + cell {
                    let p = (y * side + x) * 4;
                    out[p..p + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                }
            }
        }
        (side as u32, side as u32, out)
    }
}
