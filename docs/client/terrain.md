[Home](../README.md) › [Client](README.md) › Terrain

# Terrain

How a region's tile file turns into ground: explicit and derived heights, planes, overlay shapes and tile flags, plus what the explorer's region viewer simplifies. Read this to rebuild the original client's heights exactly, noise and all.

Heights are computed in [`rs2cache::map`](../../crates/rs2cache/src/map.rs), which decodes the tile file (layout on [Map regions](../records/map.md)). The viewer is [`map_view.rs`](../../src/explorer/map_view.rs).

## Tiles and planes

A region is 64 × 64 tiles on each of 4 planes. The tile file runs plane 0 to 3, then x 0–63, then y 0–63, and every grid in a decoded plane is indexed `x * 64 + y`. Heights are in cache units, where more negative is higher (see [Coordinates](coordinates.md)).

Each tile is a run of opcode bytes. Opcodes 2 and up set fields and the run continues; opcode 0 or 1 sets the height and ends the tile. A field set twice keeps the later value.

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 0 | | height derived; end of tile |
| 1 | `u8` h | explicit height; end of tile |
| 2–49 | `u8` overlay | overlay id, shape `(op − 2) / 4`, rotation `(op − 2) & 3` |
| 50–81 | | flags `= op − 49` |
| 82–255 | | underlay `= op − 81` |

## Explicit heights

Opcode 1 reads a byte `h`, with 1 read as 0. The height is relative to the plane below:

```text
height = below − h × 8
```

On plane 0, `below` is 0. Every upper-plane height, explicit or derived, stacks on the plane below, so plane 2's numbers mean nothing until you've decoded planes 0 and 1.

## Derived heights

Opcode 0 derives the height. On planes 1–3 it's one storey above the same tile on the plane below:

```text
height = below − 240
```

On plane 0 it comes from the original client's noise function, evaluated at world tile coordinates (`region x × 64 + x`, `region y × 64 + y`). All arithmetic is 32-bit with wrapping, as in Java; `/` truncates toward zero and `>>` is arithmetic.

```text
noise(x, y):
    n = x + y × 57
    n = (n << 13) ^ n
    m = (n × (n × n × 15731 + 789221) + 1376312589) & 0x7fffffff
    return (m >> 19) & 0xff

smooth(x, y):
    corners = noise(x−1, y−1) + noise(x+1, y−1) + noise(x−1, y+1) + noise(x+1, y+1)
    sides   = noise(x−1, y) + noise(x+1, y) + noise(x, y−1) + noise(x, y+1)
    return corners / 16 + sides / 8 + noise(x, y) / 4

cosine(k) = trunc(65536 × cos(k × 0.0030679615))

interpolate(a, b, t, scale):
    f = (65536 − cosine(t × 1024 / scale)) >> 1
    return ((a × (65536 − f)) >> 16) + ((b × f) >> 16)

interpolated(x, y, scale):
    ix = x / scale, fx = x & (scale − 1)
    iy = y / scale, fy = y & (scale − 1)
    top    = interpolate(smooth(ix, iy),     smooth(ix + 1, iy),     fx, scale)
    bottom = interpolate(smooth(ix, iy + 1), smooth(ix + 1, iy + 1), fx, scale)
    return interpolate(top, bottom, fy, scale)

derived_height(x, y):
    x = x + 932731
    y = y + 556238
    k =  (interpolated(x + 45365, y + 91923, 4) − 128)
      + ((interpolated(x + 10294, y + 37821, 2) − 128) >> 1)
      + ((interpolated(x,         y,         1) − 128) >> 2)
    k = trunc(k × 0.3) + 35
    k = clamp(k, 10, 60)
    return −k × 8
```

That's three octaves at scales 4, 2 and 1, each half the weight of the one before. Plane-0 derived heights therefore always fall between −480 and −80, in steps of 8.

The decoder records which heights were derived (`height_derived`), so a dump shows explicit and derived tiles apart.

## Corner heights

A tile's height is the height of its `(x, y)` corner. A tile's four corners are its own height and those of the tiles at `(x + 1, y)`, `(x + 1, y + 1)` and `(x, y + 1)`, so the ground is a mesh of corner heights rather than flat steps.

## Overlays and underlays

Underlay and overlay ids are `flo` ids plus one, with 0 meaning none (see [Floors](../records/flo.md)). An overlay comes with a shape from 0 to 11 and a rotation from 0 to 3 quarter turns, both packed into its opcode. The decoder stores the shape and rotation as numbers and doesn't expand them into tile masks.

## Tile flags

Flags are `op − 49`, a value from 1 to 32, stored exactly as read. The two lowest bits mean:

| Bit | Meaning |
| --- | --- |
| 1 | blocked |
| 2 | bridge: read on plane 1. The original client shifts that tile's planes down one level, so the plane-1 floor is what you stand on at ground level, and height lookups on that tile read the plane above |

The decoder doesn't act on any flag.

## The region viewer's simplifications

The explorer draws a region to look at, not to match the original client pixel for pixel:

- **One region at a time.** Neighbouring regions aren't stitched. Corners on the far edge reuse the edge tile's own height instead of reading the next region.
- **Flat colours.** A tile takes its `flo` record's RGB directly, with no blending, texturing or terrain lighting. The modern view uses the underlay colour (or the overlay's when there's no underlay). The classic view and the world map use the overlay colour, else the underlay's.
- **Rough overlay shapes.** In the modern view, shape 0 covers the whole tile and any other shape covers one of the tile's two triangles with the overlay and the other with the base colour. Overlay rotation is ignored.
- **Missing floors.** Plane 0 draws every tile, using a default ground colour where there's no floor. Planes 1–3 draw only tiles that have one.
- **No flags.** Tile flags, bridges included, don't change what's drawn. Each plane can be toggled on and off instead.
- **Plain placement.** Locations sit at their footprint's centre at their tile's ground height, turned by quarter turns (see [Assembly](assembly.md#placement-rotation)). Models aren't fitted to sloping ground.

The world map is plane 0 of every region at one pixel per tile, coloured the same way, with black where a tile has no floor.

## See also

- [Map regions](../records/map.md): the tile and location file layouts.
- [Floors](../records/flo.md): the `flo.dat` records overlays and underlays name.
- [Coordinates](coordinates.md): units, the 240-unit storey and the axis directions.
- [Assembly](assembly.md): how placed objects are shaped and turned.
- [Explorer](../tools/explorer.md#maps-and-the-world-map): navigating regions in the GUI.
