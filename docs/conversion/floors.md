[Home](../README.md) › [Conversion](README.md) › Floors

# Floors

`rs2ool pack floors` turns every `flo.dat` definition into a floor material, puts the floor names in a string table, and records the mapping in the materials manifest. This page lists what each field gets.

Implemented in [`rs2convert::floor`](../../crates/rs2convert/src/floor.rs). The source format is on [Floor](../records/flo.md), and the material layout on [Materials](../pack/materials.md). Pack data is little-endian.

## One material per floor

Floor `f` becomes material `1000 + f` (`material_id::FLOOR_BASE`), written into a `materials` pack under that id. Which mode it gets depends on whether the definition names a texture:

| Field | Textured floor | Coloured floor |
| --- | --- | --- |
| `mode` | 1 (texture) | 0 (palette) |
| `texId` | the definition's texture id | 0 |
| `period` | 1 | 0 |
| `palette` | 0 | nearest palette entry to the definition's RGB |
| `roughness` | 255 | 255 |
| `alpha` | 255 | 255 |

`texId` is the texture id itself (0–49), not the texture's material id. The palette entry comes from `Palette::index_rgb`, which looks up the definition's 24-bit RGB directly with no HSL step, using 0 when the definition has no colour (see [Palette](palette.md)). The minimap colour is not used.

## Flags

| Bit | Name | Set when |
| --- | --- | --- |
| 1 | `FLOOR` | always |
| 2 | `HIDE_UNDERLAY` | the definition has opcode 3 |
| 4 | `OCCLUDE` | the definition occludes |

The original client reads opcode 3 and does nothing with it. The converter sets the flag anyway, so the information isn't lost on the way through.

## Names, strings domain 1

Names go into a string list in floor id order, one per floor, with an empty string when a definition has no name. Unlike the [objects](objects.md) table, it isn't deduplicated, so a floor's string index is just its position in id order. The list is written as `strings` record `1 << 8 | 0` (`0x100`), domain 1 and locale 0, in its own `strings` pack.

## The `floors` manifest extra

The materials pack's manifest carries a `floors` table in `extra`, keyed by floor id as a decimal string:

```json
{
  "floors": {
    "0": { "material": 1000, "name": 0 },
    "1": { "material": 1001, "name": 1 }
  }
}
```

`material` is the material id and `name` is the string index in domain 1.

## See also

- [Floor](../records/flo.md): the definition and its opcodes.
- [Materials](../pack/materials.md): material record layout and flags.
- [Strings](../pack/strings.md): string table layout and domains.
- [Terrain](../client/terrain.md): how the original client uses floors as overlays and underlays.
