[Home](../README.md) › Pack format

# The content-pack format

This section specifies the content-pack format that `rs2ool pack` writes: the container file, the record id scheme, and the byte layout of every record kind. Read it if you want to write your own pack reader or writer, or check what a pack holds.

Implemented in the [`contentpack`](../crates/contentpack.md) crate: the container in [`pack.rs`](../../crates/contentpack/src/pack.rs), the records in [`records/`](../../crates/contentpack/src/records/mod.rs).

## What a pack is

A pack is one file holding records of a single kind. Each record is an opaque byte string keyed by a `u32` record id. The container stores the records sorted by id, compresses each one on its own, and keeps a CRC32 of each so a reader can check what it decompressed. The file is named by a SHA-256 of its contents. See [Container](container.md) for the details.

All pack data is little-endian: the container header, the index, and every record layout in this section. That's the opposite of the cache, which is big-endian throughout, so don't reuse a cache reader's integer helpers here.

## Kinds

The kind number is stored in the container header; the kind name is used in the JSON manifest and as the directory a pack is written to.

| Number | Name | Record layouts |
| --- | --- | --- |
| 1 | `models` | [Models](models.md) |
| 2 | `materials` | [Materials](materials.md), including the palette record |
| 3 | `textures` | [Textures](textures.md): texture rectangles and atlas images |
| 4 | `sprites` | [Sprites](sprites.md), including font metrics |
| 5 | `objects` | [Objects](objects.md) |
| 6 | `kits` | [Kits](kits.md) |
| 7 | `rigs` | [Rigs](rigs.md) |
| 8 | `items` | Reserved: no record layout implemented |
| 9 | `anims` | [Anims](anims.md): one animation clip per record |
| 10 | `fluids` | Reserved: no record layout implemented |
| 11 | `strings` | [Strings](strings.md): string tables |

Kinds 8 (`items`) and 10 (`fluids`) are valid kind numbers: the container reader accepts them and a loose directory can name them. No record encoder or decoder exists for them, and no `rs2ool pack` command writes them.

Which ids each kind uses, and the reserved ids within them, are on [Record ids](record-ids.md).

## Conventions shared by the records

- Types are `u8`, `i8`, `u16`, `i16` and `u32`, all little-endian. There are no varints and no padding.
- A record holds nothing but its own fields: no length prefix, since the container's index gives the length.
- Every decoder rejects a record with bytes left over after its last field, and a record that ends early.
- Three kinds start with a 4-byte magic and a version byte: models (`RMDL`), rigs (`RRIG`) and anims (`RANM`). The version is 2 for all three, and the decoder rejects any other value. The other kinds have no header.
- Optional references use `0xFFFFFFFF` (`u32::MAX`) for "none", and 8-bit bone and parent fields use 255.

Decoding errors are one enum shared by every record kind:

| Error | When |
| --- | --- |
| `Truncated` | A field runs past the end of the record (gives the offset and the bytes needed) |
| `Magic` | The first four bytes aren't the kind's magic |
| `Version` | The version byte isn't 2 |
| `Invalid` | The record breaks an invariant its page lists (an index out of range, keys out of order, …) |
| `Trailing` | Bytes are left over after the last field |

## Coordinates

Models, rigs and anims use pack space: the cache's coordinates with y and z both negated. Negating two axes is half a turn about x, so handedness is kept and nothing is mirrored; y points up. Units are unchanged (128 per tile). [Coordinates](../client/coordinates.md) covers the cache's own axes.

## See also

- [Container](container.md): header, manifest, index, naming and loose directories.
- [Record ids](record-ids.md): every id namespace across the kinds.
- [Conversion](../conversion/README.md): how the tools fill these records from the cache.
- [Pack commands](../tools/pack-commands.md): the `rs2ool pack` subcommands that write packs.
- [contentpack](../crates/contentpack.md): the crate that implements the format.
