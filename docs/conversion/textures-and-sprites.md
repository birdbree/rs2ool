[Home](../README.md) › [Conversion](README.md) › Textures and sprites

# Textures and sprites

This page covers the two image conversions: `rs2ool pack textures`, which packs the 50 textures into one atlas and writes a material for each, and `rs2ool pack sprites`, which writes media and title sprites and the four fonts as sprite records.

Implemented in [`rs2convert::texture`](../../crates/rs2convert/src/texture.rs) and [`rs2convert::sprite`](../../crates/rs2convert/src/sprite.rs). The source formats are on [Texture](../records/texture.md), [Sprite](../records/sprite.md) and [Font](../records/font.md), and the layouts on [Textures](../pack/textures.md) and [Sprites](../pack/sprites.md). Pack data is little-endian.

## Texture images

For each texture id 0–49, sub-sprite 0 is drawn at its offsets onto a canvas of the sheet's full width and height, the way the original client pads textures. Pixels that fall outside the canvas are dropped. Pixels become RGBA: palette index 0 is fully transparent (`0, 0, 0, 0`), and every other index is opaque. A texture with no sprite is an error.

## The atlas

All textures go into one 1024 × 1024 RGBA atlas, which starts out transparent black, using **shelf packing**:

1. Sort the textures by height, tallest first, then by id.
2. Place them left to right along the current shelf.
3. When the next one doesn't fit in the remaining width, start a new shelf at x 0, directly below the tallest texture on the current shelf.
4. A texture wider than 1024, or one that would run past the bottom, is an error.

The `textures` pack then holds:

| Record id | Contents |
| --- | --- |
| `0x80000000` | the atlas image: width, height, then RGBA pixels |
| texture id | the texture's rectangle: `atlas` 0, `x`, `y`, `width`, `height`, `flags` |

Every rectangle has the `NEAREST` flag (1) set, which asks for nearest filtering when sampling. Rectangles are stored by texture id whatever order they were packed in, and reading one back out of the atlas returns the padded image pixel for pixel.

## Texture materials

The same run writes a second `materials` pack with one material per texture:

| Field | Value |
| --- | --- |
| `id` | `1 + texture id` |
| `mode` | 1 (texture) |
| `texId` | the texture id |
| `period` | 1, so the texture repeats once per tile |
| `palette` | 0 |
| `roughness` | 255 |
| `flags` | 0 |
| `alpha` | 255 |

These are the material slots that textured model faces point at (see [Models](models.md)).

## Sprite records

`rs2ool pack sprites` reads the media and title archives by default. `--archive media` or `--archive title` limits it to one, and any other name is an error. `--all` is accepted and changes nothing, since every sprite is the default.

Sprites use the `rs2cache` sprite ids: media archive entries then title archive entries, in archive order, with the four font entries left out (see [Sprite](../records/sprite.md)). For each sprite in a selected archive, every sub-sprite `i` becomes record `sprite << 8 | i` with:

- `nameHash`: the jag hash of the archive entry it came from;
- `subIndex`: `i`;
- `width`, `height`, `xOffset`, `yOffset`: the sub-sprite's own, so the sprite is trimmed, not padded;
- RGBA pixels, with palette index 0 transparent as for textures.

## Font glyphs and metrics

Fonts are written only when the title archive is selected. Font `f` (0–3, in the order `p11_full`, `p12_full`, `b12_full`, `q8_full`) gives:

- one sprite record per glyph `g`, id `0x01000000 | f << 8 | g`, with `nameHash` the jag hash of `<font name>.dat`;
- one metrics record, id `0x02000000 | f`:

```text
advance        256 × u8
drawOffsetX    256 × i8
drawOffsetY    256 × i8
```

`advance` is each character's pen advance. `drawOffsetX` is the horizontal offset the original client recomputes from the glyph's pixels, rather than the stored one. `drawOffsetY` is the stored vertical offset. A font whose tables don't hold exactly 256 entries is an error. See [Font](../records/font.md) for how they're derived.

## See also

- [Textures](../pack/textures.md) and [Sprites](../pack/sprites.md): record layouts, including font metrics.
- [Materials](../pack/materials.md): the material record.
- [Rasteriser](../client/rasteriser.md): texture pages in the original client.
- [Record ids](../pack/record-ids.md): the sprite and glyph id ranges.
