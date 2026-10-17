[Home](../README.md) › [Pack format](README.md) › Materials

# Materials

A `materials` pack (kind 2) holds fixed-size material records and, at a reserved id, the palette that palette-mode materials and model faces index into. This page gives both layouts, the mode and flag values, and how material ids are assigned.

Implemented in [`contentpack::records::material`](../../crates/contentpack/src/records/material.rs). All integers are little-endian.

## Material record

A material is exactly 11 bytes:

```text
id          u16   material id (the tools write the record id here too)
mode        u8    0 = palette, 1 = texture
texture     u16   texture id (texture mode)
period      u8    texture repeat period, in tiles
palette     u16   palette index (palette mode)
roughness   u8
flags       u8    bit set, see below
alpha       u8    255 = opaque
```

There's no magic or version byte. The decoder rejects a record shorter or longer than 11 bytes and checks nothing else: it accepts any mode or flag value, and doesn't compare `id` with the record id.

### Modes

| Value | Name | Meaning |
| --- | --- | --- |
| 0 | `PALETTE` | Flat colour: `palette` is an index into the palette record |
| 1 | `TEXTURE` | Textured: `texture` is a texture id, the id of a [texture rectangle](textures.md) |

### Flags

| Bit | Value | Name |
| --- | --- | --- |
| 0 | 1 | `FLOOR`: a floor material |
| 1 | 2 | `HIDE_UNDERLAY` |
| 2 | 4 | `OCCLUDE` |
| 3 | 8 | `SPECULAR` |
| 4 | 16 | `EMISSIVE` |

Bits 5–7 are unused. `HIDE_UNDERLAY` and `OCCLUDE` are copied from the floor definition's flags of the same names ([Floor](../records/flo.md)). `SPECULAR` and `EMISSIVE` are defined values that the tools never set.

### What the tools write

Every material the tools write has `roughness` 255 and `alpha` 255.

| Material | Id | mode | texture | period | palette | flags |
| --- | --- | --- | --- | --- | --- | --- |
| Palette material | 0 | 0 | 0 | 0 | 0 | 0 |
| Texture `t` | `1 + t` | 1 | `t` | 1 | 0 | 0 |
| Textured floor `f` | `1000 + f` | 1 | the floor's texture | 1 | 0 | `FLOOR` + floor flags |
| Coloured floor `f` | `1000 + f` | 0 | 0 | 0 | nearest palette entry to the floor's RGB | `FLOOR` + floor flags |

Each set comes from a different command, so they arrive in separate `materials` packs: `rs2ool pack materials` writes the palette and material 0, `textures` the texture materials (alongside the `textures` pack), and `floors` the floor materials with the `floors` manifest table ([Record ids](record-ids.md#manifest-extras)).

## Palette

The palette is record `0xFFFFFFFF` of the `materials` pack:

```text
count     u16
entries   count × { r u8, g u8, b u8 }
```

A palette index is a position in `entries`, from 0. The palette shares the pack with the materials at the one id a `u16` material id can never reach. The decoder rejects a record that ends early or has bytes left over. The record can hold up to 65535 entries; the tools write at most 1024, sorted by `(r, g, b)` with duplicates removed. [Palette](../conversion/palette.md) explains how they're chosen.

Model faces carry a palette index directly ([Models](models.md)), and so do palette-mode materials and the `surfaces` table of an [object](objects.md).

## Ids

Material ids are `u16`; see [Record ids](record-ids.md#materials) for the whole scheme: 0 for the palette material, `1 + t` for textures, `1000 + flo` for floors, and `2000 + loc` for wall kits, which kit records name but no `materials` record carries. Model faces hold a `u8` material slot, so faces can only use ids below 256.

## See also

- [Palette](../conversion/palette.md): how the palette is built from the cache's face colours.
- [Floors](../conversion/floors.md): how floor definitions become floor materials.
- [Textures and sprites](../conversion/textures-and-sprites.md): the texture materials and atlas.
- [Textures](textures.md): what a texture-mode material's `texture` names.
- [Colour](../client/colour.md): the cache's HSL colours the palette approximates.
