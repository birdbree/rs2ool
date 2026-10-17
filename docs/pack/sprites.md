[Home](../README.md) › [Pack format](README.md) › Sprites

# Sprites

A `sprites` pack (kind 4) holds 2D images: sprite records, each one sub-sprite of a cache sprite sheet or one glyph of a font, plus a metrics record per font. This page gives both layouts and the id scheme.

Implemented in [`contentpack::records::sprite`](../../crates/contentpack/src/records/sprite.rs). All integers are little-endian.

## Sprite record

```text
nameHash   u32   jag hash of the archive entry the sprite came from
subIndex   u8    the sprite's position in its sheet
width      u16   in pixels
height     u16   in pixels
xOffset    u8    draw offset, in pixels
yOffset    u8    draw offset, in pixels
pixels     width × height × { r u8, g u8, b u8, a u8 }   row-major, row 0 first
```

The header is 11 bytes, then the pixels. The encoder rejects a sprite whose pixel buffer isn't exactly `width × height × 4` bytes, and the decoder reads exactly that many and rejects a record that ends early or has bytes left over.

- **nameHash** is the [jag hash](../cache/archives.md) of the entry name, stored as the hash's 32 bits. For a sheet from the media or title archive it's the sheet's entry (for example the hash of `<name>.dat`); for a font glyph it's the hash of the font's entry, `p11_full.dat` and so on.
- **subIndex** equals the low byte of the record id.
- **xOffset** and **yOffset** are the sub-sprite's offsets as stored in the cache sheet ([Sprite](../records/sprite.md)).
- **pixels**: a pixel whose cache palette index is 0 has alpha 0 and black RGB; every other pixel has alpha 255 and the palette colour.

## Font metrics

A font's metrics record is exactly 768 bytes, three tables of 256 entries indexed by character code:

```text
advance       256 × u8   pen advance per character, in pixels
drawOffsetX   256 × i8   horizontal draw offset per glyph
drawOffsetY   256 × i8   vertical draw offset per glyph
```

The decoder rejects a record that isn't exactly 768 bytes. The values come from the decoded font ([Font](../records/font.md)): `drawOffsetY` is each glyph's stored y offset, and `drawOffsetX` is the 0-or-1 offset the original client derives from the glyph's pixels rather than the stored x offset. The conversion fails if the decoded font doesn't have exactly 256 entries in each table.

The glyph records carry the stored offsets in `xOffset` and `yOffset`, so for a font glyph `xOffset` and `drawOffsetX` can disagree. It's easy to reach for the sprite field out of habit; `drawOffsetX` is the one that matches the original client's placement.

## Ids

| Id | Record |
| --- | --- |
| `s << 8 \| i` | Sub-sprite `i` of sprite `s` |
| `0x01000000 \| f << 8 \| g` | Glyph `g` of font `f` |
| `0x02000000 \| f` | Metrics of font `f` |

`s` is an `rs2cache` sprite id, `f` a font id (0–3) and `g` a character code (0–255); see [Record ids](record-ids.md#sprites).

`rs2ool pack sprites` writes every sprite of the media and title archives, or just one of them with `--archive media` or `--archive title`. Fonts come from the title archive, so their glyphs and metrics are written only when it's included.

## See also

- [Textures and sprites](../conversion/textures-and-sprites.md): how sheets, glyphs and metrics are converted.
- [Sprite](../records/sprite.md): the cache's sprite sheets and sprite ids.
- [Font](../records/font.md): the cache's fonts and the derived metrics.
- [Archives](../cache/archives.md): the jag hash behind `nameHash`.
- [Record ids](record-ids.md#sprites): the full id scheme.
