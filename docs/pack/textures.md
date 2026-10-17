[Home](../README.md) › [Pack format](README.md) › Textures

# Textures

A `textures` pack (kind 3) holds two record types: atlas images, which are plain RGBA8 bitmaps, and texture rectangles, which say where each texture sits in an atlas. This page gives both layouts and their ids.

Implemented in [`contentpack::records::texture`](../../crates/contentpack/src/records/texture.rs). All integers are little-endian.

## Texture rectangle

Record id `t` is texture `t`. The record is exactly 11 bytes:

```text
atlas    u16   atlas number (the atlas image is record 0x80000000 | atlas)
x        u16   first column in the atlas, in pixels
y        u16   first row in the atlas, in pixels
width    u16   in pixels
height   u16   in pixels
flags    u8
```

| Bit | Value | Flag |
| --- | --- | --- |
| 0 | 1 | `NEAREST`: sample with nearest filtering |

Bits 1–7 are unused. The tools set `NEAREST` on every rectangle.

The decoder rejects a record that isn't exactly 11 bytes. It doesn't check that the atlas exists or that the rectangle fits inside it.

## Atlas image

Record id `0x80000000 | a` is atlas `a`:

```text
width    u16
height   u16
pixels   width × height × { r u8, g u8, b u8, a u8 }   row-major, row 0 first
```

The encoder rejects an image whose pixel buffer isn't exactly `width × height × 4` bytes. The decoder reads exactly that many bytes and rejects a record that ends early or has bytes left over.

A pixel row starts at byte `4 + 4 × y × width`, so texture `t`'s pixels are rows `y` to `y + height − 1`, columns `x` to `x + width − 1` of its atlas.

## What the tools write

`rs2ool pack textures` writes one atlas, `0x80000000`, 1024 × 1024 pixels, and one rectangle per texture, ids 0–49, all with `atlas` 0. Each texture is stored at its full, untrimmed size. A texel whose cache palette index is 0 is written with alpha 0 (and black RGB), every other texel with alpha 255. Atlas pixels that no texture covers are all zeros.

At 1024 × 1024 that's 4 MiB of pixels in one record before compression, so records aren't always small.

The same command writes a `materials` pack with one texture-mode material per texture, material id `1 + t` naming texture `t` ([Materials](materials.md)). How textures are laid out in the atlas is on [Textures and sprites](../conversion/textures-and-sprites.md).

## See also

- [Textures and sprites](../conversion/textures-and-sprites.md): untrimming and shelf-packing the cache textures.
- [Texture](../records/texture.md): the cache's texture sprites.
- [Materials](materials.md): the texture-mode materials that point here.
- [Record ids](record-ids.md#textures): the id scheme.
- [Models](models.md): the per-face UVs used with a texture.
