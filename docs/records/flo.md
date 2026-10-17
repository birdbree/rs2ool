[Home](../README.md) › [Records](README.md) › Flo

# Flo: floor types

`flo.dat` holds floor types: the colours and textures of the underlays and overlays a map paints on its tiles. This page lists the opcodes, how map tiles refer to a floor, and how the tools turn floors into materials.

Decoded by [`rs2cache::config::flo`](../../crates/rs2cache/src/config/flo.rs) into `FloDef` ([`rs2cache::types::config`](../../crates/rs2cache/src/types/config.rs)). Cache data is big-endian.

## Storage

Flo is a **sequential** type: `flo.dat` is `count u16` followed by the definitions back to back, with no `.idx`. See [Config archive](../cache/config.md).

```text
rs2ool cache decode flo --all -o out/
```

## Opcodes

Each definition ends with opcode 0. An opcode not listed is logged, added to `unknown_opcodes`, and skipped without reading anything.

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 1 | `u24` | `rgb`: 24-bit RGB colour, `0xRRGGBB` |
| 2 | `u8` | `texture`: texture id |
| 3 | nothing | `hide_underlay = true` |
| 5 | nothing | `occlude = false` |
| 6 | `string` | `name` |
| 7 | `u24` | `secondary_rgb`: the colour used on the minimap |

Opcode 3 is read by the original client and then ignored; the decoder still records it, and floor conversion carries it as a flag.

## Defaults

`occlude` starts true and `hide_underlay` false. `rgb`, `texture`, `name` and `secondary_rgb` are none.

## Post-decode derivations

None. The decoder keeps `rgb` as the 24-bit value it read; it doesn't convert it to HSL. See [Colour](../client/colour.md) for the colour spaces involved.

## Fields

`FloDef` holds: `id`, `rgb` (`Option<u32>`), `texture` (`Option<u8>`), `hide_underlay` (omitted from JSON when false), `occlude`, `name`, `secondary_rgb` (`Option<u32>`), and `unknown_opcodes` (omitted from JSON when empty).

## How map tiles refer to floors

A region's underlay and overlay bytes are 1-based: 0 means no floor, and value `n` refers to flo `n − 1`. The explorer's map views colour a tile from its overlay's `rgb` first, then its underlay's. See [Map](map.md).

## Conversion

`rs2ool pack floors` converts every floor with [`rs2convert::floor`](../../crates/rs2convert/src/floor.rs):

- Flo `id` becomes material `1000 + id`, with the floor flag set, plus the hide-underlay flag when `hide_underlay` is true and the occlude flag when `occlude` is true.
- A floor with a `texture` is a texture-mode material on that texture, with period 1.
- A floor without one is a palette-mode material whose palette index is the palette entry nearest to `rgb`, or nearest to black (`0x000000`) when there's no `rgb`.
- Names go into a strings pack as one table at record `1 << 8` (domain 1, locale 0); a floor with no name gets an empty string.
- The materials manifest gains a `floors` map from each flo id to `{ material, name }`, where `name` is the index into that string table.

`secondary_rgb` isn't used in conversion. The details are on [Floor conversion](../conversion/floors.md) and [Materials](../pack/materials.md).

## See also

- [Config archive](../cache/config.md): sequential storage and the opcode loop.
- [Map](map.md): the underlay and overlay ids that point here.
- [Texture](texture.md): what a floor's texture id refers to.
- [Floor conversion](../conversion/floors.md): floors into materials and strings.
- [Terrain](../client/terrain.md): how the original client lays floors on tiles.
