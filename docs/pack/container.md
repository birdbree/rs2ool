[Home](../README.md) › [Pack format](README.md) › Container

# Pack container

This page specifies the pack file itself: the header, the JSON manifest, the index, the compressed blob, how a pack is named, what a reader checks, and the loose-directory form that `rs2ool pack build` bakes into a pack. Record contents are on the per-kind pages listed in [Pack format](README.md).

Implemented in [`contentpack::pack`](../../crates/contentpack/src/pack.rs) and [`contentpack::loose`](../../crates/contentpack/src/loose.rs). All integers are little-endian.

## Layout

```text
magic        4 bytes   "RS2P"
version      u16       1
kind         u8        kind number (1–11, see the kinds table)
codec        u8        0 = none, 1 = zstd
manifestLen  u32       length of the manifest in bytes
recordCount  u32       number of index entries
manifest     manifestLen bytes of UTF-8 JSON
index        recordCount × entry
blob         every record's stored bytes, back to back, to the end of the file
```

The header is 16 bytes. There's no blob length field: the blob is everything after the index.

Each index entry is 16 bytes:

```text
id      u32   record id
offset  u32   start of the stored bytes, from the start of the blob
length  u32   stored (compressed) length
crc     u32   CRC32 of the uncompressed record
```

Entries are sorted by ascending id, and each id appears once. The writer lays records out in the same order, so the first record is at offset 0 and each one starts where the previous one ended. A record may be empty (length 0 with codec none).

## Manifest

The manifest is compact JSON (no whitespace) with these fields, in this order:

| Field | Type | Contents |
| --- | --- | --- |
| `kind` | string | The kind name, lowercase: `"models"`, `"materials"`, … |
| `record_count` | number | Number of records, the same as the header's `recordCount` |
| `source_cache_crc` | number | CRC32 of the cache's stored versionlist archive (index 0, archive 5) as read from the cache, before decompression |
| `codec` | string | `"none"` or `"zstd"` |
| `extra` | object | Kind-specific tables; the field is left out when there are none |

For example:

```text
{"kind":"materials","record_count":3,"source_cache_crc":305419896,"codec":"zstd","extra":{"floors":{…}}}
```

The two `extra` tables the tools write, `floors` and `kit_parts`, are described on [Record ids](record-ids.md#manifest-extras). The [versionlist](../cache/versionlist.md) changes whenever any versioned file in the cache does, so this CRC changes with it.

## Codec

With codec 1, each record is compressed on its own as one zstd frame at level 9, with no dictionary. With codec 0, records are stored as they are. The writer defaults to zstd, and every pack the tools write uses it. The CRC in the index is always over the uncompressed bytes, whatever the codec.

## Name

A pack is named by the lowercase hex SHA-256 (64 characters) of the index bytes followed by the blob bytes. The header and manifest aren't hashed.

So two packs whose records are identical but whose manifests differ, say in `source_cache_crc`, get the same name. If that matters to you, read the manifest, not the file name.

Packs are written as `<dir>/<kind>/<hash>.pack`, where `<kind>` is the kind name, creating directories as needed. `rs2ool pack inspect <file>` prints a pack's hash, manifest and index.

## Reading

Opening a pack checks, in order:

1. The magic is `RS2P`.
2. The version is 1.
3. The kind number is one of the 11 kinds.
4. The codec byte is 0 or 1.
5. The manifest and the index fit in the file, and the manifest parses as JSON with the fields above.

The reader then recomputes the SHA-256 name from the index and blob and makes it available. It doesn't compare it with the file name, and it doesn't check that the header's kind, codec and record count agree with the manifest's, or that the index is sorted (the writer always makes them agree and sorts it). Reading one record by id finds it in the index by binary search, then checks:

1. `offset + length` lies within the blob.
2. The stored bytes decompress, using the codec named in the manifest.
3. The CRC32 of the result equals the index entry's `crc`.

The errors are:

| Error | Meaning |
| --- | --- |
| `Magic` | The file doesn't start with `RS2P` |
| `Version` | The version isn't 1 |
| `Kind` | The kind number is unknown |
| `Codec` | The codec byte is above 1 |
| `Truncated` | The header, manifest or index runs past the end of the file |
| `Manifest` | The manifest isn't valid JSON for the fields above |
| `Range` | A record's stored bytes run past the blob |
| `Decompress` | A record's zstd frame doesn't decode |
| `Crc` | A record's CRC32 doesn't match its index entry (both values are reported) |
| `Io` | The file couldn't be read or written (with its path) |

Asking for an id that isn't in the index isn't an error; the reader reports the record as absent.

## Determinism

Baking the same records always gives the same bytes and the same name:

- Records are kept in a map ordered by id, so insertion order doesn't matter. Inserting an id twice replaces the earlier record.
- `extra` tables are ordered maps, and JSON object keys inside them come out sorted, so their insertion order doesn't matter either.
- The manifest is serialised compactly, with no timestamps or paths.
- Compression uses a fixed level and no threads.

There's a unit test for this in `pack.rs`. [Determinism](../conversion/determinism.md) covers how the conversions keep their inputs to the writer stable.

## Loose directories

A loose directory is an unbaked pack: a `pack.json` plus one file per record.

```text
<dir>/
  pack.json       {"kind": "models", "source_cache_crc": 0, "extra": {}}
  1.bin           record 1
  215.bin         record 215
  …
```

`pack.json` has `kind` (the kind name), `source_cache_crc`, and an optional `extra` object that becomes the manifest's `extra`. Every file ending in `.bin` is a record, and its file stem must parse as a decimal `u32`, which is the record id; any other `.bin` name is an error. Files with other extensions are ignored. Record bytes are taken as they are: the loader doesn't decode or check them.

`rs2ool pack build <dir> -o <out>` bakes one with zstd:

- If `<out>` ends in `.pack`, the pack is written to exactly that path.
- Otherwise `<out>` is a directory and the pack is written as `<out>/<hash>.pack`. There's no `<kind>` subdirectory here, unlike the conversion commands.

It prints the path it wrote. Loading errors are `Io` (with the path), `Manifest` (a `pack.json` that doesn't parse) and `Name` (a `.bin` file whose stem isn't an id).

## See also

- [Pack format](README.md): the kinds table and conventions shared by every record.
- [Record ids](record-ids.md): which ids go in which pack, and the manifest extras.
- [Pack commands](../tools/pack-commands.md): `rs2ool pack build`, `inspect` and the conversion commands.
- [Determinism](../conversion/determinism.md): why two runs give the same pack hash.
- [contentpack](../crates/contentpack.md): the crate's API.
