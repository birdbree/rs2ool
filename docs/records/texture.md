[Home](../README.md) › [Records](README.md) › Textures

# Textures

Textures are the fifty palette images that textured model faces draw with. This page covers where they're stored, how they're padded, and how a textured face points at one and maps it.

Decoded by [`rs2cache::sprite`](../../crates/rs2cache/src/sprite.rs) through [`rs2cache::record`](../../crates/rs2cache/src/record.rs); the texture pages the renderer builds are in [`rs2classic::texture`](../../crates/rs2classic/src/texture.rs).

## Storage

Texture ids are 0–49. Texture `n` is entry `n.dat` of the textures archive (archive 6), and all fifty share that archive's `index.dat`. The byte layout is the ordinary [sprite format](sprite.md): a `u16` offset into `index.dat`, a header with the canvas size and palette, and one 7-byte record per sub-sprite (cache data is big-endian). The decoder finds the sub-sprite count the same way as for any sprite entry, from the block end and exact pixel use. Ids of 50 and up are not textures.

## Padding

Like any sprite, a texture's stored image is trimmed to its ink, with x and y offsets onto a `max width × max height` canvas. A texture is always used padded back out to that canvas: sub-sprite 0, copied in at its offsets, with palette index 0 everywhere else (`SpriteSheet::untrimmed`). Only sub-sprite 0 is used.

Index 0 is transparent, so a texture with uncovered canvas, or with index-0 pixels inside it, has holes.

## Dump output

```text
rs2ool cache decode texture --id 1 -o out/
```

writes `texture/<id>.png`: the padded image as RGBA, index 0 fully transparent. With `--meta --xref`, the sidecar's summary also lists every model with a textured face using that texture. See [dump output](../tools/dump-output.md).

## Textured faces

A [model](model.md) face is textured when the low two bits of its render type are 2 (textured, smooth) or 3 (textured, flat). For such a face:

- the face's 16-bit colour isn't a colour at all: it holds the texture id;
- `type >> 2` indexes the model's list of texture triangles, each three vertex indices `P, M, N`;
- [lighting](../client/lighting.md) produces a shade from 0 to 127 per corner (or one for the face) instead of a lit colour.

The texture triangle is separate from the face's own triangle. It fixes a plane in model space: P is the texture's origin, the direction P → M is u and P → N is v, and one full texture spans P to M and P to N. Every face pointing at the same triangle shares one continuous mapping, which is how a texture runs smoothly across several faces.

The original client turns this into perspective-correct texture coordinates per pixel. The exact arithmetic is on the [rasteriser](../client/rasteriser.md#perspective-textured) page; the conversion to per-corner UVs is on [model conversion](../conversion/models.md).

## Texture pages

The renderer prepares each texture as a 128 × 128 page. A 64-pixel-wide texture is doubled to fill it; every other texture is read as 128 wide. Colours pass through the brightness curve, are masked to `0xf8f8ff`, and get three darker copies used for shading. Transparent texels are skipped when drawing. The details are under [texture pages](../client/rasteriser.md#texture-pages).

## See also

- [Sprites](sprite.md): the shared byte layout and the sub-sprite count rule.
- [Models](model.md): face render types and texture triangles.
- [Rasteriser](../client/rasteriser.md): texture pages and the textured filler.
- [Lighting](../client/lighting.md): textured shades.
- [Model conversion](../conversion/models.md): P/M/N to UVs.
- [Textures and sprites conversion](../conversion/textures-and-sprites.md) and [pack textures](../pack/textures.md): the texture atlas in packs.
