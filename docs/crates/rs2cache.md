[Home](../README.md) › [Crates](README.md) › rs2cache

# rs2cache

`rs2cache` reads a revision-317 cache: the index files, the sector chains in the data file, the jag archives, and every record type. This page is a tour of its public API; the formats themselves are described in the [cache](../cache/README.md) and [records](../records/README.md) sections.

Entry point: [`crates/rs2cache/src/lib.rs`](../../crates/rs2cache/src/lib.rs). All cache integers are big-endian.

## Module map

| Module | What it holds |
| --- | --- |
| [`cache`](../../crates/rs2cache/src/cache.rs) | `Cache`: the five index files and the 520-byte sector chains of `main_file_cache.dat`. |
| [`io`](../../crates/rs2cache/src/io.rs) | `Cursor`, the bounds-checked big-endian reader every decoder uses. |
| [`compress`](../../crates/rs2cache/src/compress.rs) | `inflate` (gzip, when the magic is present) and `bunzip` (headerless bzip2). |
| [`archive`](../../crates/rs2cache/src/archive.rs) | Jag archives of index 0, `jag_hash`, archive numbers (`TITLE` = 1 … `SOUNDS` = 8), `EntryName`, and the table of known entry names. |
| [`versionlist`](../../crates/rs2cache/src/versionlist.rs) | The versionlist tables, and `file_version_and_crc` for a stored file's trailer. |
| [`config`](../../crates/rs2cache/src/config/mod.rs) | The `ConfigType` trait, `decode_all` / `decode_one` / `count`, and one submodule per config type (`flo`, `idk`, `loc`, `npc`, `obj`, `seq`, `spotanim`, `varbit`, `varp`). |
| [`model`](../../crates/rs2cache/src/model.rs) | The model decoder: the 18-byte trailer `Header` and the section `Layout`. |
| [`anim`](../../crates/rs2cache/src/anim.rs) | The frame-pack decoder, `frame_ids`, and `FrameIndex` (global frame id → pack). |
| [`map`](../../crates/rs2cache/src/map.rs) | Region tile and location decoders, `map_index`, and `derived_height`. |
| [`sprite`](../../crates/rs2cache/src/sprite.rs) | Palette sprites and fonts; `FONTS` lists the four font names. |
| [`interface`](../../crates/rs2cache/src/interface.rs) | The interface decoder for the interface archive's `data` entry. |
| [`record`](../../crates/rs2cache/src/record.rs) | `Records`, plus one-shot `ids`, `decode_any`, `meta` and `is_absent`. |
| [`types`](../../crates/rs2cache/src/types/mod.rs) | The plain `serde` structs every decoder returns. |

## `Cache`

`Cache::open(dir)` opens a directory holding `main_file_cache.dat` and `main_file_cache.idx0` to `idx4`. The data file is memory-mapped read-only and the index files are read into memory. Nothing in the crate writes to the cache.

Every reader takes `&self`, and `Cache` is `Send + Sync`, so you can decode files from several threads at once. For tests, `Cache::from_bytes(dat, &indices)` builds one from in-memory bytes; the crate's own tests use it with synthetic sector layouts.

The lower-level calls:

- `index_count()`, `file_count(index)`, `entries(index)`, `entry(index, id)` and `dat_len()` describe what's there. A file count is the index file's size divided by 6.
- `read_raw(index, id)` follows the file's sector chain and returns the stored bytes, still compressed. Every sector header is checked: file id, chunk number and index type must match, and the chain must not end early or run off the data file. A fault comes back as `CacheError::BadSector` with a `SectorFault` saying which check failed.
- `read_raw_with_chain` does the same and also returns the sectors visited, which the metadata sidecars record.

On top of those, `lib.rs` adds the decoding readers:

- `read(index, id)` reads a file of index 1–4 and inflates it: gunzipped when it starts with the gzip magic, returned as stored otherwise.
- `archive(n)` reads and decodes archive `n` of index 0.
- `model(id)`, `frame_pack(id)`, `midi(id)` (inflated, not decoded), `region(x, y)`, `region_for(&entry)`, `map_index()`, `versionlist()` and `config::<T>()` decode one thing each.
- `frame_index()` builds the frame id → pack map by reading the id list of every index-2 file. The cache doesn't store this map anywhere, so it has to be built by scanning.

## `Records`

`Records::new(&cache)` wraps a cache and memoises the expensive steps: each archive is decoded once, each table type (the config types and interfaces) is decoded once in full, and `map_index` is read once. It's `Send + Sync` too. Use it whenever you decode more than one record.

| Method | Returns |
| --- | --- |
| `ids(t)` | Every valid id of record type `t`, ascending. |
| `decode(t, id)` | The decoded `Record`. A missing id is `Error::NoRecord`. |
| `files(t, id)` | The stored files the record came from, as `FileMeta` (source, sector chain, sizes, CRC32 of the stored bytes, compression). |
| `meta(t, id)` | The metadata sidecar: a `Meta` with the files and a `TypeSummary`. |
| `sprite_source(id)` | The archive number and entry name sprite `id` comes from. |
| `media_sprite(name)` | The sheet in entry `<name>.dat` of the media archive, as an interface sprite reference names it. |
| `archive(n)`, `map_index()`, `region_entry(id)` | The memoised archive, `map_index`, and one region's `map_index` entry. |

The free functions `ids`, `decode_any` and `meta` each build a throwaway `Records`, so they're fine for one call and wasteful in a loop. `is_absent(&err)` tells you whether an error just means "no such record" (an absent file or `NoRecord`).

Ids per type: models, frame packs and MIDI use their index file id; maps use the region id `x << 8 | y`; config types use the definition id; textures are 0–49; fonts are 0–3 in `FONTS` order; interfaces use the component id; sprites are positions in the media archive's sprite entries followed by the title archive's, in stored order, with fonts left out. The [records overview](../records/README.md) has the full table.

## `RecordType` and `Record`

`RecordType` (in `types::record`) names the 17 record kinds: `model`, `anim`, `map`, `loc`, `npc`, `obj`, `idk`, `seq`, `spotanim`, `flo`, `varp`, `varbit`, `texture`, `sprite`, `font`, `interface` and `midi`. `RecordType::ALL` lists them in that order, `name()` gives the command-line name, and `FromStr` parses one case-insensitively (an unknown name is an `UnknownRecordType` error listing the valid ones).

`Record` is the matching enum of decoded values, serialised as `{"type": …, "record": …}`. Textures and sprites share `SpriteSheet`, and `Record::Midi` holds the inflated bytes, since MIDI isn't decoded. `record.record_type()` maps back.

`Meta` is the sidecar: `record_type`, `id`, `files`, and a flattened `TypeSummary` with one arm per family (model, anim, map, config, sprite, font, interface, midi). The config arm carries the whole decoded definition.

## `types`

`rs2cache::types` holds only data: structs with `serde` derives and a couple of small accessors, no logic.

| Module | Main types |
| --- | --- |
| `types::model` | `Model` (vertices, faces, HSL face colours, optional face types, priorities, alphas, labels, textured faces) with `bounds()`. |
| `types::anim` | `FramePack`, `Frame`, `FrameTransform`, `BaseTransform`, and the transform `kind` constants. |
| `types::config` | `LocDef`, `NpcDef`, `ObjDef`, `IdkDef`, `SeqDef`, `SpotAnimDef`, `FloDef`, `VarpDef`, `VarbitDef`, plus `Recolour` and `Morph`. |
| `types::map` | `Region`, `Plane`, `Location`, and `REGION_SIZE` (64). |
| `types::sprite` | `SpriteSheet`, `Sprite` and `Font`. |
| `types::interface` | `Interface` and its `Content` variants. |
| `types::versionlist` | `VersionList` and `FileTable`. |
| `types::record` | `RecordType`, `Record`, `Meta`, `FileMeta`, `Source`, `TypeSummary`. |

Config fields hold what the original client ends up with after decoding: its defaults for absent opcodes, its derived flags, and `None` where the client uses -1. Opcodes a decoder doesn't know are kept in each definition's `unknown_opcodes`.

## Errors

Each module has its own `thiserror` enum, and `rs2cache::Error` wraps them with context (which file, archive, model or region):

| Type | Raised for |
| --- | --- |
| `CacheError` | I/O on open, an index or file id out of range, an absent file, a bad sector. |
| `CompressError` | A corrupt gzip or bzip2 stream. |
| `ArchiveError` | A truncated archive, or an inflated size that doesn't match its header or table. |
| `io::IoError` | A read past the end, an unterminated string, a seek past the end. |
| `config::ConfigError` | A missing config entry, a truncated index or definition, `.dat`/`.idx` count mismatch, no such definition. |
| `model::ModelError` | File shorter than the trailer, sections overrunning it, a bad face compression type, a vertex index out of range. |
| `anim::FrameError` | File shorter than the trailer, sections overrunning it, a slot count past the base, a section length mismatch (strict mode). |
| `map::MapError` | A truncated `map_index`, tile file or location file. |
| `sprite::SpriteError` | A truncated sheet, no `index.dat`, a bad pixel type, pixel bytes not used exactly. |
| `interface::InterfaceError` | A truncated component, with its id and offset. |

`Error` adds `MissingEntry` (an archive lacks a named entry), `NoRegion` and `NoRecord`. No decoder panics on bad input; see [conventions](../development/conventions.md).

## Example

List a few NPC definitions and summarise the first model:

```rust
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};

fn main() -> Result<(), rs2cache::Error> {
    let dir = std::env::var_os("RS2_CACHE").expect("set RS2_CACHE to a cache directory");
    let cache = Cache::open(&dir)?;
    let records = Records::new(&cache);

    for id in records.ids(RecordType::Npc)?.into_iter().take(5) {
        if let Record::Npc(def) = records.decode(RecordType::Npc, id)? {
            println!("npc {id}: {:?}, {} models", def.name, def.models.len());
        }
    }

    let first = records.ids(RecordType::Model)?[0];
    let model = cache.model(first)?;
    println!("model {first}: {} vertices, bounds {:?}", model.vertices.len(), model.bounds());

    let meta = records.meta(RecordType::Model, first)?;
    println!("{}", meta.files[0].compression);
    Ok(())
}
```

`Cache::open` returns `CacheError`, which converts into `rs2cache::Error` with `?`.

## See also

- [Cache format](../cache/README.md): the files and indices `Cache` reads.
- [Primitives](../cache/primitives.md): what `Cursor` reads, including the two smart forms.
- [Records](../records/README.md): one page per record type and its decoder.
- [Dump output](../tools/dump-output.md): how `Meta` sidecars appear on disk.
- [rs2convert](rs2convert.md): uses `Records` for all its input.
