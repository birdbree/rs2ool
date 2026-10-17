[Home](../README.md) › [Cache](README.md) › Data and index files

# Data and index files

How a stored file is found and read: the 6-byte index records, the 520-byte sectors of the data file, the checks made on every sector of a chain, and the 2-byte version every file in indices 1–4 ends with.

All integers here are big-endian. Implemented in [`rs2cache::cache`](../../crates/rs2cache/src/cache.rs); the version trailer is read by [`rs2cache::versionlist`](../../crates/rs2cache/src/versionlist.rs).

## Index records

Each of `main_file_cache.idx0` to `main_file_cache.idx4` is a flat array of 6-byte records, one per file id, starting at id 0:

```text
size          u24   stored length of the file in bytes
firstSector   u24   sector number in main_file_cache.dat
```

Record `id` sits at byte `id * 6`. The number of file ids in an index is the `.idx` file's size divided by 6; a trailing partial record, if there ever is one, is ignored. A 317 cache always has exactly five indices, 0 to 4.

### Absent files

A record whose `firstSector` is 0 is an **absent** file: the id is in range but nothing is stored for it. The original client treats such a record as missing, and so does the reader. Gaps like this are normal, so an absent file is reported as absent rather than as damage, and code that walks a whole index (`rs2ool cache verify`, building the frame index) skips them.

Asking for a file can fail in three distinct ways before any sector is read:

| Error | When |
| --- | --- |
| `NoSuchIndex` | The index number is past the last index (4 in a real cache) |
| `NoSuchFile` | The id is at or past the index's file count |
| `Absent` | The record's first sector is 0 |

## Sectors

`main_file_cache.dat` is a run of 520-byte sectors. Sector `n` starts at byte `n * 520`. Each one is an 8-byte header followed by up to 512 bytes of payload:

```text
fileId       u16   the file's id
chunk        u16   position of this sector in the file's chain, from 0
nextSector   u24   the next sector of the chain, or 0 at the end
indexType    u8    the index number plus one
payload      512 bytes, or fewer in the file's last sector
```

A file of `size` bytes takes `ceil(size / 512)` sectors. Only the bytes it needs are read from the last one, so a data file can end in the middle of a final sector and still read cleanly.

`fileId` is only 16 bits wide, while an index could in principle hold more ids than that. The reader compares the header's id with the requested id directly, the same way the original client does.

## Reading a chain

Starting from `firstSector` with an expected chunk of 0, the reader repeats until it has collected `size` bytes:

1. If the current sector number is 0, the chain has ended early: `ChainEnded`.
2. If `sector * 520` is at or past the end of the data file: `OutOfRange`.
3. If the 8-byte header plus the payload still needed runs past the end of the data file: `Short`.
4. If the header's `fileId` isn't the requested id: `FileIdMismatch`.
5. If the header's `chunk` isn't the expected chunk: `ChunkMismatch`.
6. If `indexType` isn't the index number plus one: `TypeMismatch`.
7. Append `min(512, bytes still needed)` bytes of payload, move to `nextSector`, and add one to the expected chunk.

Once `size` bytes are in hand the loop stops, so the last sector's `nextSector` is never looked at. A file with `size` 0 and a nonzero first sector reads as zero bytes without touching the data file.

### Fault kinds

Every failure inside a chain is a `BadSector` error carrying the index, the file id, the chunk and sector where it happened, and one of these faults:

| Fault | Meaning |
| --- | --- |
| `OutOfRange` | The sector number points outside the data file |
| `Short` | The sector's header or needed payload runs past the end of the data file |
| `FileIdMismatch { found }` | The header names a different file |
| `ChunkMismatch { found }` | The header has the wrong position in the chain |
| `TypeMismatch { found }` | The header names a different index |
| `ChainEnded { remaining }` | `nextSector` was 0 while `remaining` bytes were still to be read |

`rs2ool cache verify` sorts these into two columns: `OutOfRange` and `Short` count as *truncated*, the other four as *bad-sector*. See [Cache commands](../tools/cache-commands.md).

The reader can also return the `(sector, chunk)` pairs it visited; the sector numbers become the `sector_chain` of a metadata sidecar (see [Dump output](../tools/dump-output.md)).

## The version trailer

Every stored file in indices 1–4 ends with a 2-byte version:

```text
body       size - 2 bytes   the file as stored (a gzip stream, usually)
version    u16
```

The CRC of the file is the standard CRC-32 of the body, the bytes before the version. Both are computed on the stored bytes, before any decompression. A file shorter than 2 bytes has neither.

The versionlist archive holds a table of expected versions and CRCs for each of these indices, and `rs2ool cache verify` compares every file against it. A mismatch there means the file changed after the table was built; it is reported as *modified*, not as damage. See [Version list](versionlist.md).

Archives in index 0 are not covered by those tables.

One thing that trips people up: the `crc32` in a metadata sidecar is taken over all of the stored bytes, trailer included, so it won't equal the versionlist's CRC for the same file. Both are correct; they cover different bytes.

## See also

- [Compression](compression.md): what to do with the bytes once the chain is read.
- [Archives](archives.md): the container format of index 0 files.
- [Version list](versionlist.md): the version and CRC tables.
- [Primitives](primitives.md): the `u24` and `u16` reads used above.
