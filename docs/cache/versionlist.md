[Home](../README.md) › [Cache](README.md) › Version list

# Version list

The `versionlist` archive (index 0, archive 5) holds per-file tables for indices 1–4: the version and CRC every file should have, a byte per model and per MIDI file, a table sized by the number of frame ids, and the map index that ties regions to their files. This page gives the layout of each table and what the reader does with it.

All integers are big-endian. Implemented in [`rs2cache::versionlist`](../../crates/rs2cache/src/versionlist.rs), with `map_index` in [`rs2cache::map`](../../crates/rs2cache/src/map.rs) and the decoded tables in [`rs2cache::types::versionlist`](../../crates/rs2cache/src/types/versionlist.rs).

## The entries

The archive has twelve entries, named by type and table. The four types correspond to indices 1–4:

| Type | Index |
| --- | --- |
| `model` | 1 |
| `anim` | 2 |
| `midi` | 3 |
| `map` | 4 |

| Entry | Layout | Length |
| --- | --- | --- |
| `model_version`, `anim_version`, `midi_version`, `map_version` | `u16` per file id | one per id in the index |
| `model_crc`, `anim_crc`, `midi_crc`, `map_crc` | 32-bit CRC per file id | one per id in the index |
| `model_index` | `u8` per model id | one per model |
| `anim_index` | `u16` per global frame id | one per frame id |
| `midi_index` | `u8` per MIDI id | one per MIDI file |
| `map_index` | 7-byte record per region | one per region |

Entry `n` of a version, CRC or flag table belongs to file id `n` of its index. The tables carry no count; the length of the entry divided by the record size is the count, and a trailing partial record is ignored. A table whose entry is missing from the archive decodes as empty rather than failing.

## Versions and CRCs

Every stored file in indices 1–4 ends with a 2-byte version, and its CRC is the CRC-32 of the stored bytes before that version (see [Data and index files](data-and-index-files.md#the-version-trailer)). The `_version` and `_crc` tables record the values each file had when the tables were built.

```text
<type>_version   count × u16
<type>_crc       count × u32     CRC-32, stored big-endian
```

`rs2ool cache verify` reads the trailer and CRC of every present file and compares them with these tables. A file that doesn't match is reported in a separate *modified* list, since a file that was updated after the tables were built is still a perfectly good file. It doesn't count as damage and doesn't change the exit code. If the versionlist archive itself can't be read, verify says so and skips the comparison. See [Cache commands](../tools/cache-commands.md).

## `model_index` and `midi_index`

One byte per model id and one byte per MIDI id. The reader exposes them as raw bytes and attaches no meaning to them.

## `anim_index`

One `u16` per global frame id. The original client uses only its **length**: the number of entries sets the size of its global frame table.

`anim_index` is **not** a frame id → frame pack map. Frame ids are global 16-bit ids, and each frame pack in index 2 lists the ids it defines. The frame → pack map is built by reading every pack's id list (`Cache::frame_index`). The name practically begs you to treat it as a lookup table. Don't. See [Frame pack](../records/frame-pack.md).

## `map_index`

A run of 7-byte records, one per region:

```text
region         u16   x << 8 | y
tileFile       u16   index-4 file id of the region's tile file
locationFile   u16   index-4 file id of the region's location file
members        u8    flag; 1 means set
```

`x` and `y` are region coordinates. A region covers 64 × 64 tiles, so region `(x, y)` starts at tile `(x * 64, y * 64)`. The tools name a region `RX_RY` on the command line (`--region RX_RY`, both decimal) and use the `region` value, `x << 8 | y`, as its record id.

To decode region `(x, y)`, the reader finds the record whose `region` equals `x << 8 | y`, then reads index-4 files `tileFile` and `locationFile` and gunzips them. A region with no record in the table doesn't exist as far as the reader is concerned; asking for it is a `NoRegion` error. The two files are described on [Map](../records/map.md); how terrain and locations are placed is on [Terrain](../client/terrain.md).

## See also

- [Data and index files](data-and-index-files.md): where the version trailer and CRC come from.
- [Archives](archives.md): how the `versionlist` archive and its entries are read.
- [Frame pack](../records/frame-pack.md): building the real frame → pack map.
- [Map](../records/map.md): the tile and location files `map_index` points to.
