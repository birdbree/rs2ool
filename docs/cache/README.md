[Home](../README.md) › Cache

# The 317 cache

This section describes the container format of a revision-317 cache: the files on disk, how a file is pulled out of them, and how the archives and config tables inside are laid out. Read it if you want to write your own reader, or to understand what `rs2ool cache` is telling you.

No cache is included in the repository. Point the tools at your own copy with `--cache <dir>` or the `RS2_CACHE` environment variable (see [Getting started](../getting-started.md)). The reader maps the data file read-only and never writes to it.

Implemented in [`rs2cache::cache`](../../crates/rs2cache/src/cache.rs), [`rs2cache::archive`](../../crates/rs2cache/src/archive.rs) and [`rs2cache::compress`](../../crates/rs2cache/src/compress.rs). The crate as a whole is described on [rs2cache](../crates/rs2cache.md).

## The files

A cache is a directory holding six files:

| File | Contents |
| --- | --- |
| `main_file_cache.dat` | Every stored file, as chains of 520-byte sectors |
| `main_file_cache.idx0` … `main_file_cache.idx4` | One 6-byte record per file id, giving the file's size and first sector |

The five `.idx` files are the five *indices*. A file is addressed by `(index, id)`: the index picks the `.idx` file, the id picks the record within it. See [Data and index files](data-and-index-files.md) for the record and sector layouts.

## The five indices

| Index | Holds | Stored as | Described in |
| --- | --- | --- | --- |
| 0 | Jag archives, numbered 1–8 | Archive container with bzip2 | [Archives](archives.md) |
| 1 | Models, one per id | gzip | [Model](../records/model.md) |
| 2 | Animation frame packs, one per id | gzip | [Frame pack](../records/frame-pack.md) |
| 3 | MIDI files, one per id | gzip | [MIDI](../records/midi.md) |
| 4 | Map tile and location files | gzip | [Map](../records/map.md) |

Files in indices 1–4 are gunzipped when they start with the gzip magic and used as stored otherwise; each also ends with a 2-byte version. Index 0 files are read as stored and decoded as archives. See [Compression](compression.md) and [Version list](versionlist.md).

Index-4 file ids don't name regions directly. The versionlist archive's `map_index` maps each region to its pair of files.

## What lives where

| Record | Location |
| --- | --- |
| Models | Index 1, by id |
| Frame packs | Index 2, by id |
| MIDI | Index 3, by id |
| Map regions | Index 4, via `map_index` in the versionlist archive |
| loc, npc, obj, idk, seq, spotanim, flo, varp, varbit | Config archive (2), as [config tables](config.md) |
| Textures | Textures archive (6), entries `0.dat`–`49.dat` |
| Sprites | Media archive (4) and title archive (1) |
| Fonts | Title archive (1), four named entries |
| Interfaces | Interface archive (3), entry `data` |

The full list of record types, with their id schemes, is on [Records](../records/README.md).

## Reading a file, end to end

1. Look up record `id` in `main_file_cache.idx<index>`. A first sector of 0 means the file is absent.
2. Follow the sector chain through `main_file_cache.dat`, checking every sector header, until `size` bytes are collected.
3. For index 0, decode the result as a jag archive. For indices 1–4, gunzip it if it carries the gzip magic.
4. Hand the bytes to the record decoder.

`rs2ool cache verify` runs steps 1–3 on every file in every index and reports what fails. See [Cache commands](../tools/cache-commands.md).

## Pages in this section

- [Primitives](primitives.md): integers, the two smart forms, strings, 16-bit HSL colours.
- [Data and index files](data-and-index-files.md): index records, sectors, chain validation, version trailers.
- [Compression](compression.md): gzip files and headerless bzip2.
- [Archives](archives.md): the archive table, the container layout, the name hash.
- [Version list](versionlist.md): version and CRC tables, `model_index`, `anim_index`, `midi_index`, `map_index`.
- [Config](config.md): `.dat`/`.idx` pairs, opcode streams and the encodings they share.

## See also

- [Architecture](../architecture.md): where the cache reader sits in the workspace.
- [Glossary](../glossary.md): index, sector, archive, entry and the other terms used here.
- [Records](../records/README.md): the decoders built on top of this container.
