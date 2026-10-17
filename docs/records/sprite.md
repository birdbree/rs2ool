[Home](../README.md) › [Records](README.md) › Sprites

# Sprites

Palette sprites are the 2D images stored in the media and title archives. This page covers their byte layout, how the decoder finds them and counts their sub-sprites, how sprite ids are assigned, and what the dump writes.

Implemented in [`rs2cache::sprite`](../../crates/rs2cache/src/sprite.rs), with the types in [`rs2cache::types::sprite`](../../crates/rs2cache/src/types/sprite.rs) and the id scheme in [`rs2cache::record`](../../crates/rs2cache/src/record.rs). Textures and fonts use the same format; see [textures](texture.md) and [fonts](font.md).

## Where they live

A sprite archive ([jag archive](../cache/archives.md)) holds one `index.dat` entry plus any number of sprite entries `<name>.dat`. Each sprite entry holds one or more sub-sprites that share a palette and a canvas size. The media archive (archive 4) and the title archive (archive 1) are both laid out this way. The decoder has a built-in table of known entry names; entries it has no name for are still found by their [name hash](../cache/archives.md) and shown as `#<hash>`.

All integers are big-endian, as everywhere in the cache (see [primitives](../cache/primitives.md)).

## Layout

The sprite entry starts with an offset into `index.dat`, then holds the pixel bytes of every sub-sprite back to back:

```text
<name>.dat
  u16   offset into index.dat of this entry's header
  u8[]  pixels of sub-sprite 0, then sub-sprite 1, …
```

At that offset, `index.dat` holds the entry's header followed by one record per sub-sprite:

```text
index.dat, at offset
  u16   max width        canvas the offsets are relative to
  u16   max height
  u8    palette count n
  u24   rgb × (n − 1)    palette entries 1 … n − 1

  per sub-sprite (7 bytes):
  u8    x offset
  u8    y offset
  u16   width
  u16   height
  u8    pixel order      0 = row-major, 1 = column-major
```

Palette entry 0 isn't stored: it is always transparent, and the decoder fills it in as 0. Each pixel byte is a palette index, and index 0 is transparent wherever it appears.

Each sub-sprite takes `width × height` pixel bytes. With order 0 they run row by row (`x + y·width`); with order 1 they run column by column (`x·height + y`). The decoder always hands back row-major pixels and keeps a `column_major` flag recording how they were stored. Any other order byte is an error.

## Finding entries and counting sub-sprites

The cache never says how many sub-sprites an entry holds. The original client just knows how many it wants. The decoder has no such luck, so it works it out:

1. Every entry other than `index.dat` whose first `u16` leaves room for the 5-byte header inside `index.dat` counts as a sprite entry.
2. The entries are sorted by that offset. An entry's block of records runs up to the next entry's offset, or to the end of `index.dat` for the last one.
3. Records are read while a whole 7-byte record still fits before the block end and pixel bytes remain. Before taking a record, the decoder checks that its `width × height` fits in the pixel bytes left; if not, it stops.
4. When it stops, the pixel bytes must be used up exactly. Leftover bytes are an error.

Because reading stops once the pixels run out, a zero-size sub-sprite at the very end of a block is never counted. Entries whose first two bytes merely happen to land inside `index.dat` pass step 1 and then fail to decode; the dump reports them as failures.

Fonts are the one case where the count is known: a font always has 256 glyphs, and the decoder reads exactly that many (see [fonts](font.md)).

## Sprite ids

Sprite ids are positions in one list: the media archive's sprite entries in stored archive order, then the title archive's, with the four [font](font.md) entries left out. Textures have their own ids and aren't in this list.

Interfaces don't use these ids. They name a sprite as `name,index`, meaning sub-sprite `index` of entry `<name>.dat` in the media archive (see [interfaces](interface.md#sprite-references)).

## Untrimmed images

The stored sub-sprites are trimmed: each is only as big as its ink, and the offsets say where it sits on the `max width × max height` canvas. `SpriteSheet::untrimmed` pads one back out: it makes a canvas of that size filled with index 0 and copies the sub-sprite in at `(x offset, y offset)`, dropping any pixel that falls outside. Textures are always used untrimmed; ordinary sprite dumps are not.

## RGBA export

`SpriteSheet::rgba` turns a sub-sprite into RGBA8, row-major. Index 0 becomes `(0, 0, 0, 0)`. Every other index becomes its palette colour with alpha 255; an index past the end of the palette comes out opaque black. There's no partial transparency in this format.

## Dump output

```text
rs2ool cache decode sprite --id 1 -o out/
```

Each sub-sprite is written as its own trimmed PNG, `sprite/<id>-<archive>-<entry>-<n>.png`, where `<archive>` is `media` or `title` and `<entry>` is the entry name without `.dat` (or `#<hash>`). With `--meta`, the sidecar's summary gives the canvas size, the palette size and the sub-sprite count. `rs2ool cache dump --archive media --decode` dumps just the sprites of one archive. See [dump output](../tools/dump-output.md) and [cache commands](../tools/cache-commands.md).

## See also

- [Textures](texture.md): the same format, always untrimmed.
- [Fonts](font.md): 256-glyph sprite entries with derived metrics.
- [Interfaces](interface.md): the `name,index` sprite reference.
- [Jag archives](../cache/archives.md): entries, name hashes and known names.
- [Textures and sprites conversion](../conversion/textures-and-sprites.md): how sprites become pack records.
- [Pack sprites](../pack/sprites.md): the pack record layout.
