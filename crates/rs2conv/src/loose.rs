//! Loose record directories: `pack.json` (kind, source cache CRC and
//! optional manifest extras) plus one `<id>.bin` per record. `rs2conv pack`
//! bakes one into a hashed pack.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use rs2pack::{Kind, PackWriter};
use serde_json::Value;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct LooseManifest {
    pub kind: Kind,
    pub source_cache_crc: u32,
    #[serde(default)]
    pub extra: BTreeMap<String, Value>,
}

/// Read a loose directory into a writer.
pub fn read(dir: &Path) -> anyhow::Result<PackWriter> {
    let m: LooseManifest = serde_json::from_slice(
        &std::fs::read(dir.join("pack.json"))
            .with_context(|| format!("{}/pack.json", dir.display()))?,
    )?;
    let mut w = PackWriter::new(m.kind, m.source_cache_crc);
    for (k, v) in m.extra {
        w.set_extra(&k, v);
    }
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x == "bin") {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let Ok(id) = stem.parse::<u32>() else {
                bail!("{}: record files must be named <id>.bin", p.display());
            };
            w.insert(id, std::fs::read(&p)?);
        }
    }
    Ok(w)
}

/// Bake a loose directory; `out` is a `.pack` path or a directory.
pub fn bake(dir: &Path, out: &Path) -> anyhow::Result<PathBuf> {
    let w = read(dir)?;
    let (hash, bytes) = w.bake();
    let path = if out.extension().is_some_and(|x| x == "pack") {
        out.to_path_buf()
    } else {
        std::fs::create_dir_all(out)?;
        out.join(format!("{hash}.pack"))
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}
