//! Loose record directories: `pack.json` (kind, source cache CRC and
//! optional manifest extras) plus one `<id>.bin` per record. `rs2conv pack`
//! and the explorer bake one into a hashed pack.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use crate::{Kind, PackWriter};

#[derive(Debug, Error)]
pub enum LooseError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: {source}")]
    Manifest {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("{0}: record files must be named <id>.bin")]
    Name(PathBuf),
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> LooseError + '_ {
    move |source| LooseError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct LooseManifest {
    pub kind: Kind,
    pub source_cache_crc: u32,
    #[serde(default)]
    pub extra: BTreeMap<String, Value>,
}

/// Read a loose directory into a writer.
pub fn read(dir: &Path) -> Result<PackWriter, LooseError> {
    let mpath = dir.join("pack.json");
    let m: LooseManifest = serde_json::from_slice(&std::fs::read(&mpath).map_err(io(&mpath))?)
        .map_err(|source| LooseError::Manifest {
            path: mpath.clone(),
            source,
        })?;
    let mut w = PackWriter::new(m.kind, m.source_cache_crc);
    for (k, v) in m.extra {
        w.set_extra(&k, v);
    }
    for e in std::fs::read_dir(dir).map_err(io(dir))? {
        let p = e.map_err(io(dir))?.path();
        if p.extension().is_some_and(|x| x == "bin") {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let Ok(id) = stem.parse::<u32>() else {
                return Err(LooseError::Name(p));
            };
            w.insert(id, std::fs::read(&p).map_err(io(&p))?);
        }
    }
    Ok(w)
}

/// Bake a loose directory; `out` is a `.pack` path or a directory (the
/// pack is then named by its hash). Returns the written path.
pub fn bake(dir: &Path, out: &Path) -> Result<PathBuf, LooseError> {
    let w = read(dir)?;
    let (hash, bytes) = w.bake();
    let path = if out.extension().is_some_and(|x| x == "pack") {
        out.to_path_buf()
    } else {
        std::fs::create_dir_all(out).map_err(io(out))?;
        out.join(format!("{hash}.pack"))
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io(parent))?;
    }
    std::fs::write(&path, bytes).map_err(io(&path))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PackReader;

    #[test]
    fn bakes_a_loose_directory() {
        let dir = std::env::temp_dir().join(format!("contentpack-loose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let m = LooseManifest {
            kind: Kind::Models,
            source_cache_crc: 7,
            extra: BTreeMap::new(),
        };
        std::fs::write(dir.join("pack.json"), serde_json::to_vec(&m).unwrap()).unwrap();
        std::fs::write(dir.join("3.bin"), [1, 2, 3]).unwrap();
        std::fs::write(dir.join("1.bin"), [9]).unwrap();
        let out = dir.join("out");
        let path = bake(&dir, &out).unwrap();
        let r = PackReader::open(&path).unwrap();
        assert_eq!(r.ids().collect::<Vec<_>>(), [1, 3]);
        assert_eq!(r.get(3).unwrap().unwrap(), [1, 2, 3]);
        assert_eq!(path.file_stem().unwrap().to_str().unwrap(), r.hash);

        std::fs::write(dir.join("x.bin"), [0]).unwrap();
        assert!(matches!(read(&dir), Err(LooseError::Name(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
