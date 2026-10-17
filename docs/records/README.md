[Home](../README.md) › Records

# Records

The tools know 17 record types. This page lists each one with its id scheme, where it's stored in the cache, which module decodes it, and what `rs2ool cache dump` writes for it. Read it to find the right page for a type, or to work out where a dumped file came from.

Every type has a command-line name, used by `--type` and by `rs2ool cache decode <type>`. Decoding goes through [`rs2cache::Records`](../../crates/rs2cache/src/record.rs), which memoises archives and whole config tables, and every decoded record is a plain `serde` struct from `rs2cache::types`. See [the rs2cache crate page](../crates/rs2cache.md) for the API.

## The table

Index numbers refer to the [five indices](../cache/README.md); "archive N" is a [jag archive](../cache/archives.md) stored as file N of index 0.

| Type | Id | Stored in | Decoder | Dump output |
| --- | --- | --- | --- | --- |
| [`model`](model.md) | index-1 file id | index 1, one file each | `rs2cache::model` | `model/<id>.json` |
| [`anim`](frame-pack.md) | index-2 file id (a frame pack, not a frame) | index 2, one file each | `rs2cache::anim` | `anim/<id>.json` |
| [`map`](map.md) | region id `x << 8 \| y` | index 4, two files per region, named by `map_index` | `rs2cache::map` | `map/<x>_<y>.json` |
| [`loc`](loc.md) | definition id | archive 2 (`config`): `loc.dat` + `loc.idx` | `rs2cache::config::loc` | `loc/<id>.json` |
| [`npc`](npc.md) | definition id | archive 2: `npc.dat` + `npc.idx` | `rs2cache::config::npc` | `npc/<id>.json` |
| [`obj`](obj.md) | definition id | archive 2: `obj.dat` + `obj.idx` | `rs2cache::config::obj` | `obj/<id>.json` |
| [`idk`](idk.md) | definition id | archive 2: `idk.dat` | `rs2cache::config::idk` | `idk/<id>.json` |
| [`seq`](seq.md) | definition id | archive 2: `seq.dat` | `rs2cache::config::seq` | `seq/<id>.json` |
| [`spotanim`](spotanim.md) | definition id | archive 2: `spotanim.dat` | `rs2cache::config::spotanim` | `spotanim/<id>.json` |
| [`flo`](flo.md) | definition id | archive 2: `flo.dat` | `rs2cache::config::flo` | `flo/<id>.json` |
| [`varp`](varp-varbit.md) | definition id | archive 2: `varp.dat` | `rs2cache::config::varp` | `varp/<id>.json` |
| [`varbit`](varp-varbit.md) | definition id | archive 2: `varbit.dat` | `rs2cache::config::varbit` | `varbit/<id>.json` |
| [`texture`](texture.md) | 0–49 | archive 6 (`textures`): `<id>.dat` + `index.dat` | `rs2cache::sprite` | `texture/<id>.png` |
| [`sprite`](sprite.md) | position in the media archive's sprite entries, then the title archive's, in stored order, fonts excluded | archive 4 (`media`) or 1 (`title`): the entry + `index.dat` | `rs2cache::sprite` | `sprite/<id>-<archive>-<name>-<n>.png`, one per sub-sprite |
| [`font`](font.md) | 0–3: `p11_full`, `p12_full`, `b12_full`, `q8_full` | archive 1 (`title`): `<name>.dat` + `index.dat` | `rs2cache::sprite` | `font/<name>.png` + `font/<name>.metrics.json` |
| [`interface`](interface.md) | component id | archive 3 (`interface`): `data` | `rs2cache::interface` | `interface/<id>.json` |
| [`midi`](midi.md) | index-3 file id | index 3, one file each | none: inflated bytes | `midi/<id>.mid` |

## Notes on the id schemes

- **Index-backed types** (`model`, `anim`, `midi`) list every file of their index that isn't absent. An absent file is a gap in the id space, not an error.
- **Config types** number definitions by position. `loc`, `npc` and `obj` are indexed by their `.idx` entry; the rest are sequential, so finding one means decoding every definition before it. See [config archives](../cache/config.md).
- **Map ids** come from the versionlist's `map_index`, sorted ascending. A region id packs the region's x and y, each 0–255, and the region covers world tiles `x * 64 ..` and `y * 64 ..`.
- **Sprite ids** are positional. An entry counts as a sprite entry when its first two bytes point inside the archive's `index.dat`. The media archive's sprite entries come first, then the title archive's, each in the order the archive stores them, with the four font entries left out. The `<name>` in the dump path is the entry name without `.dat`, or `#<hash>` when the name isn't one of the known names.
- **Fonts** are numbered by their place in `rs2cache::sprite::FONTS`.

Sprite ids are the one scheme that depends on the cache you point the tools at. If the archive stores its entries in another order, the ids move with it, so treat a sprite id as a handle for that cache, not a name.

## Dump output

`rs2ool cache dump --type <type>` and `rs2ool cache decode <type>` write one output per record, under a directory named after the type:

- JSON outputs hold the record's struct itself, not the type-tagged wrapper.
- A texture is written as one PNG of its full, untrimmed sprite. A sprite sheet gets one PNG per sub-sprite, numbered from 0. A font gets a 16 × 16 glyph sheet and a metrics file with its advances, draw offsets and glyph sizes. Images with a zero width or height are skipped.
- A MIDI file is written as its inflated bytes.
- With `--meta`, each output gets a `<stem>.meta.json` sidecar beside it: the record type, id, the stored files it came from, and a type-specific summary. `--meta-only` writes just the sidecars.
- With `--jsonl`, the type's records go into one `<type>.jsonl` file, one record per line, instead of a file each.
- Dumping more than one interface also writes `interface/tree.json`.

Raw dumps (`--index`, `--region` without `--decode`) use other paths, such as `idx1/<id>.bin` and `map/<x>_<y>.m.bin`; see [dump output](../tools/dump-output.md).

```sh
rs2ool cache decode model --id 1 -o out/
rs2ool cache dump --type seq --all --meta -o out/
```

## See also

- [Cache overview](../cache/README.md): the files and indices the records live in.
- [Archives](../cache/archives.md): jag archive layout and entry names.
- [Cache commands](../tools/cache-commands.md): every selector and flag for `list`, `dump` and `decode`.
- [Dump output](../tools/dump-output.md): manifests, sidecars and cross-references.
- [Conversion](../conversion/README.md): which records become which pack records.
