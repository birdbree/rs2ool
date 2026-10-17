[Home](../README.md) › [Cache](README.md) › Archives

# Archives

The files of index 0 are jag archives: small containers of named entries, compressed either as a whole or entry by entry. This page covers the archive numbers, the container layout, the entry name hash, and how to ask for an entry by hash when its name is unknown.

All integers are big-endian. Implemented in [`rs2cache::archive`](../../crates/rs2cache/src/archive.rs).

## The archive table

Each archive is a file of index 0, addressed by its file id. Id 0 is unused.

| Number | Name | Holds |
| --- | --- | --- |
| 1 | `title` | The four [fonts](../records/font.md) and other [sprites](../records/sprite.md) |
| 2 | `config` | The [config tables](config.md): loc, npc, obj, idk, seq, spotanim, flo, varp, varbit |
| 3 | `interface` | One entry, `data`, holding every [interface](../records/interface.md) |
| 4 | `media` | [Sprite](../records/sprite.md) sheets and their shared `index.dat` |
| 5 | `versionlist` | The [version list](versionlist.md) tables |
| 6 | `textures` | [Textures](../records/texture.md) `0.dat`–`49.dat` and `index.dat` |
| 7 | `wordenc` | Text tables; not decoded by these tools |
| 8 | `sounds` | Sound data; not decoded by these tools |

Wherever the tools take an archive (`rs2ool cache list --archive`, `rs2ool cache dump --archive`), you can give either the name, in any case, or the number.

## Container layout

An archive starts with a 6-byte header:

```text
uncompressed   u24   size of the body once inflated
compressed     u24   size of the body as stored
body           the rest of the file
```

The body, once inflated, is an entry table followed by the entry data:

```text
count            u16
count × {
  nameHash       i32   jag hash of the entry's name
  uncompressed   u24   the entry's inflated size
  compressed     u24   the entry's stored size
}
entry data       one block per table row, in table order, each `compressed` bytes long
```

There are no offsets in the table. Entry `n` starts where entry `n − 1` ended, so you have to walk the table in order.

## Whole-archive vs per-entry compression

The two header sizes decide how the archive is compressed. Both arrangements use headerless bzip2 (see [Compression](compression.md#headerless-bzip2-archives)).

**Sizes differ: whole-archive.** The body is one bzip2 stream. Inflate it to get the table and the entry data. The entries inside are stored plain: each entry's block is `compressed` bytes long and its data is the first `uncompressed` bytes of that block, which is what the original client copies.

**Sizes equal: per-entry.** The body is stored plain. The table can be read straight away, and each entry's `compressed`-byte block is its own bzip2 stream that inflates to `uncompressed` bytes.

```text
uncompressed != compressed   body = bunzip(body); entries plain
uncompressed == compressed   body plain;          each entry = bunzip(block)
```

`rs2ool cache list --archive <name>` prints "compressed whole" after the entry count for the first kind, and both sizes for every entry.

### Size checks

The decoder checks every size it is given:

| Error | Raised when |
| --- | --- |
| `WholeSize` | The whole-archive stream inflates to a size other than the header's `uncompressed` |
| `EntrySize` | A per-entry stream inflates to a size other than its row's `uncompressed` |
| `Truncated` | The header, the table, or an entry's block runs past the end of the data |
| `Compress` | A bzip2 stream is corrupt |

In a whole-archive, an entry whose `compressed` size is smaller than its `uncompressed` size can't supply enough bytes and fails as `Truncated`.

## The name hash

Entries aren't stored with their names, only with a 32-bit hash. To find an entry by name, hash the name and look for that value in the table. The hash is:

```text
h = 0
for each character c of the name, uppercased:
    h = h * 61 + (c - 32)          wrapping 32-bit signed arithmetic
```

Uppercasing comes first, so `loc.dat` and `LOC.DAT` hash the same. The empty name hashes to 0.

### Worked example: `flo.dat`

Uppercased, the name is `FLO.DAT`.

| Char | Code | c − 32 | h before | h × 61 + (c − 32) | h after (wrapped) |
| --- | --- | --- | --- | --- | --- |
| `F` | 70 | 38 | 0 | 38 | 38 |
| `L` | 76 | 44 | 38 | 2362 | 2362 |
| `O` | 79 | 47 | 2362 | 144129 | 144129 |
| `.` | 46 | 14 | 144129 | 8791883 | 8791883 |
| `D` | 68 | 36 | 8791883 | 536304899 | 536304899 |
| `A` | 65 | 33 | 536304899 | 32714598872 | −1645139496 |
| `T` | 84 | 52 | −1645139496 | −100353509204 | −1569261396 |

The sixth step is the first to overflow: 32 714 598 872 modulo 2³² is 2 649 827 800, which as a signed 32-bit value is 2 649 827 800 − 2³² = −1 645 139 496. So `jag_hash("flo.dat")` is **−1569261396**.

Any name longer than a few characters overflows somewhere along the way. Do the arithmetic as wrapping 32-bit integers, or truncate to 32 bits before comparing; a hash that doesn't fit in an `i32` has skipped that step.

### Known names

The hash can't be undone, so the reader keeps a built-in table of the entry names it knows for each archive and matches every entry's hash against it. A matched entry gets its name; an unmatched one is shown by its hash.

The names in the table include:

- `config`: `loc.dat`, `loc.idx`, `npc.dat`, `npc.idx`, `obj.dat`, `obj.idx`, `idk.dat`, `flo.dat`, `seq.dat`, `spotanim.dat`, `varp.dat`, `varbit.dat`, plus entries the original client never reads (see [Config](config.md#entries-present-but-unread)).
- `versionlist`: the twelve tables listed on [Version list](versionlist.md).
- `interface`: `data`.
- `textures`: `0.dat` to `49.dat` and `index.dat`.
- `title`: `index.dat`, the four fonts `p11_full.dat`, `p12_full.dat`, `b12_full.dat`, `q8_full.dat`, and other sprite names.
- `media`: `index.dat` and a table of sprite sheet names.

An entry missing from the table is still fully readable; it just doesn't have a name.

## Addressing entries by hash

Anywhere the tools take an entry name, you can write `#` followed by the hash as a signed 32-bit decimal instead: `#682978269`, `#-1569261396`. Anything else, including `#` followed by something that isn't a number, is taken as a name and hashed. Entries without a known name are listed, and written to disk by `rs2ool cache dump`, under the same `#<hash>` form.

```sh
rs2ool cache list --archive config
rs2ool cache dump --archive config --entry flo.dat -o out/
rs2ool cache dump --archive config --entry '#-1569261396' -o out/
```

The last two commands write the same entry. Quote the `#` form in your shell, where `#` can start a comment. Lookup returns the first table row with the matching hash. See [Cache commands](../tools/cache-commands.md) for the rest of the flags.

## See also

- [Compression](compression.md): the headerless bzip2 both arrangements use.
- [Config](config.md): what the `config` archive's entries contain.
- [Version list](versionlist.md): the `versionlist` archive's tables.
- [Sprite](../records/sprite.md): how media and title entries are decoded.
- [Glossary](../glossary.md): archive, entry, jag hash.
