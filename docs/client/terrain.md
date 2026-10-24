[Home](../README.md) › [Client](README.md) › Terrain

# Terrain

How a region's tile file turns into ground: explicit and derived heights, planes, blended underlay colours, ground lighting and shadows, overlay shapes, textured and invisible floors, and tile flags, plus what the explorer's region viewer still leaves out. Read this to rebuild the original client's ground exactly, noise and all.

Heights are computed in [`rs2cache::map`](../../crates/rs2cache/src/map.rs), which decodes the tile file (layout on [Map regions](../records/map.md)). The ground itself is built by [`rs2classic::terrain`](../../crates/rs2classic/src/terrain.rs), and the viewer is [`map_view.rs`](../../src/explorer/map_view.rs). Unless it says otherwise, every division below truncates towards zero and `>>` is an arithmetic shift.

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

Underlay and overlay ids are `flo` ids plus one, with 0 meaning none (see [Floors](../records/flo.md)). An overlay comes with a shape from 0 to 11 and a rotation from 0 to 3 quarter turns, both packed into its opcode. The decoder stores the shape and rotation as numbers; the ground builder cuts the tile with them (see [Tile shapes](#tile-shapes)).

A tile with neither an underlay nor an overlay gets no ground at all. Two kinds of floor draw differently as overlays:

- **Textured floors** (a `flo` with a texture) are drawn with that texture, shaded by the ground light.
- **The invisible floor**, RGB `0xFF00FF`, draws nothing: the overlay's part of the tile is a hole. Any underlay part of a shaped tile still draws.

Underlays are never textured. A floor's hide-underlay flag has no effect; an overlay hides the underlay only where its shape covers the tile.

## Floor colours

Each floor's RGB becomes a hue, saturation, lightness, chroma and hue weight when the floors load (the conversion is on [Colour](colour.md#from-rgb-floor-colours)). An overlay that isn't textured or invisible draws in its own floor's colour, packed to 16-bit HSL. An underlay draws in a colour blended from the floors around it.

## Underlay blending

Each tile with an underlay takes its colour from the underlays of a 10 × 10 window of tiles: from 4 tiles before it to 5 tiles after it on each axis, so the window sits half a tile off centre towards +x and +y. Tiles without an underlay contribute nothing; they don't count as black. With `n` underlay tiles in the window:

```text
hue        = Σ hue weight × 256 / Σ chroma
saturation = Σ saturation / n
lightness  = Σ lightness / n
colour     = pack(hue, saturation, lightness)
```

The hue is a chroma-weighted average, so grey floors barely pull it. It can come out slightly negative, which packing tolerates. The original client reads the window across region boundaries, because it blends over its whole loaded scene. Its ground colours carry no random offset: the same map always draws the same ground.

## Ground lighting

Every tile corner (grid point) gets a light value from the slope of the ground around it, minus the shadow near it. With `h(x, y)` the corner heights (negative is up):

```text
dx  = h(x+1, y) − h(x−1, y)
dy  = h(x, y+1) − h(x, y−1)
len = floor(sqrt(dx² + 65536 + dy²))
nx, ny, nz = (dx << 8) / len, 65536 / len, (dy << 8) / len
lit = 96 + (−50·nx − 10·ny − 50·nz) / 213
light = lit − shadow
```

That's the object light setup, direction (−50, −10, −50) with contrast 768 (|L| = 71, and 768 × 71 / 256 = 213; see [Lighting](lighting.md#the-two-standard-setups)). Level ground comes out at 84. Ground rising towards +x or +y faces the light and is brighter; ground falling away is darker. The shadow term is

```text
shadow = (S(x−1, y) >> 2) + (S(x+1, y) >> 3) + (S(x, y−1) >> 2) + (S(x, y+1) >> 3) + (S(x, y) >> 1)
```

over the plane's shadow grid `S`, so a point's own shadow counts half and its −x and −y neighbours count twice as much as its +x and +y ones. Nothing clamps the result here; colouring clamps it.

A corner's colour is its tile's packed colour `c` with the lightness scaled by the light and clamped:

```text
corner = (c & 0xff80) + clamp(light × (c & 0x7f) / 128, 2, 126)
```

which is the [lit colour](lighting.md#lit-colour) rule for model faces. A textured corner takes the shade `127 − clamp(light, 0, 127)` instead.

In the original client, the light is only worked out for the scene's inner grid points; the outermost ring stays at 0 and draws nearly black, but it's never in view.

## Shadows

Each plane has a grid of shadow values, 0 to 127, one per grid point, written as the region's locations are placed (in file order), and only by locations whose definition casts a shadow (see [Loc](../records/loc.md)):

| Location type | Shadow |
| --- | --- |
| 0, straight wall | both ends of the edge it stands on set to 50: west edge at rotation 0, north at 1, east at 2, south at 3 |
| 1 and 3, wall corners | the one corner it fills set to 50: north-west at rotation 0, north-east at 1, south-east at 2, south-west at 3 |
| 10 and 11, scenery | every grid point of its footprint, edges included (so `width + 1` by `length + 1` points, turned with the location), raised to at least `min(30, radius / 4)` |

Walls set their points outright, so a later wall wins; scenery only darkens. The radius is the model's horizontal radius, `sqrt(max(x² + z²)) + 0.99`, truncated. Other types cast nothing. In the original client, a whole missing region also darkens its plane 0 to 127.

## Tile shapes

A tile is drawn from points on its edges and inside it. Numbered 1–16, with offsets in units from the tile's south-west corner (x east, z north):

| Point | Offset | Point | Offset |
| --- | --- | --- | --- |
| 1 | (0, 0) south-west | 9 | (64, 32) |
| 2 | (64, 0) | 10 | (96, 64) |
| 3 | (128, 0) south-east | 11 | (64, 96) |
| 4 | (128, 64) | 12 | (32, 64) |
| 5 | (128, 128) north-east | 13 | (32, 32) |
| 6 | (64, 128) | 14 | (96, 32) |
| 7 | (0, 128) north-west | 15 | (96, 96) |
| 8 | (0, 64) | 16 | (32, 96) |

Corners take their own height and colours. An edge midpoint (2, 4, 6, 8) takes the mean of its two corners, `(a + b) >> 1`; for colours that's done on the packed values, so a carry can cross into the next field. The inner points don't interpolate: 9–12 copy the edge midpoints 2, 4, 6 and 8, and 13–16 copy the corners 1, 3, 5 and 7. Each point carries both an underlay and an overlay colour.

**Whole tiles.** A tile with only an underlay, or with an overlay of shape 0, is two triangles split along the south-east to north-west diagonal, whatever the heights: (north-east, north-west, south-east), then (south-west, south-east, north-west). Shape 0 ignores the rotation and draws only the overlay.

**Shaped tiles.** Shapes 1–11 split the tile into underlay (U) and overlay (O) triangles. Corners are numbered 0 south-west, 1 south-east, 2 north-east and 3 north-west; 4 and 5 are the extra points listed:

| Shape | Extra points | Triangles |
| --- | --- | --- |
| 1 | | U(1, 2, 3) O(0, 1, 3) |
| 2 | 6 | U(0, 1, 2) U(0, 2, 4) O(0, 4, 3) |
| 3 | 6 | U(0, 1, 4) U(0, 4, 3) O(1, 2, 4) |
| 4 | 6 | U(0, 4, 3) O(0, 1, 2) O(0, 2, 4) |
| 5 | 6 | U(1, 2, 4) O(0, 1, 4) O(0, 4, 3) |
| 6 | 2, 6 | U(4, 1, 2) U(4, 2, 5) O(0, 4, 5) O(0, 5, 3) |
| 7 | 2, 8 | U(4, 1, 2) U(4, 2, 3) U(4, 3, 5) O(0, 4, 5) |
| 8 | 2, 8 | U(0, 4, 5) O(4, 1, 2) O(4, 2, 3) O(4, 3, 5) |
| 9 | 11, 12 | U(0, 1, 5) U(1, 4, 5) U(1, 2, 4) O(0, 5, 3) O(5, 4, 3) O(4, 2, 3) |
| 10 | 11, 12 | O(0, 1, 5) O(1, 4, 5) O(1, 2, 4) U(0, 5, 3) U(5, 4, 3) U(4, 2, 3) |
| 11 | 13, 14 | O(0, 5, 4) O(0, 1, 5) U(0, 4, 3) U(4, 5, 3) U(5, 2, 3) U(1, 2, 5) |

Every shape covers the whole tile, and every triangle is wound the same way as the whole-tile ones. Triangles are drawn in the order listed.

**Rotation.** Each quarter turn rotates the shape about the tile centre so the south-west corner goes to the north-west one: `(x, z) → (z, −x)` about the centre. Every point moves with it, stepping backwards round its ring: two places round the outer ring 1–8, one place round 9–12 and round 13–16. The heights and colours are those of where the points land, so a turned shape still sits on the ground.

**Textures.** A textured overlay lays its texture with the south-west corner as the origin, running towards the south-east corner (u) and the north-west corner (v), one texture per tile, when all four corners are level. On a sloping tile a shaped tile's triangles each use their own three corners instead, and a whole tile's north-east half uses (north-east, north-west, south-east) while its south-west half keeps the level layout.

**What's skipped.** An underlay triangle is skipped when the tile has no underlay, and an overlay triangle when its floor is invisible. Textured triangles are always drawn.

## Drawing

The original client draws ground as part of its scene, which has no fog and no depth buffer. It draws plane by plane, lowest first, so everything on a plane is drawn before anything on the plane above: a roof is always drawn after the rooms beneath it. Within a plane it works tile by tile from the edges of the view in towards the camera's tile: the tiles furthest from the camera along x first, then along y. Within a tile, the ground goes down first, its triangles in the order listed under [Tile shapes](#tile-shapes) with no depth sorting, then the walls, decorations and objects standing there. An object bigger than a tile waits until the ground under all of it is down. A tile is dropped whole if any of its points is nearer the camera than 50 units, and a triangle is drawn only if it faces the camera. Tiles more than 25 tiles from the camera aren't drawn.

The explorer's classic view draws a region in that order: plane by plane, tile by tile, each tile's ground and then its objects far to near, with a multi-tile object drawn at the tile of its footprint nearest the camera. It doesn't do the 25-tile cut-off, and the modern view uses a depth buffer instead.

In its low-detail mode the original client also skips upper-plane tiles away from the player's plane, and draws textured ground untextured, in a fixed average colour per texture shaded by the same light. The tools always draw full detail.

## Tile flags

Flags are `op − 49`, a value from 1 to 32, stored exactly as read. The bits the ground cares about:

| Bit | Meaning |
| --- | --- |
| 1 | blocked |
| 2 | bridge: read on plane 1. The original client shifts that tile's planes down one level, so the plane-1 floor is what you stand on at ground level, and height lookups on that tile read the plane above. The ground that was on plane 0 is drawn first, underneath, and each plane's ground keeps the heights of the plane it came from |
| 8 | the tile counts as plane 0 for visibility, whatever plane it's on |

For visibility, each tile counts as its plane, one lower under a bridge, or plane 0 with bit 8. When the original client hides the planes above you (indoors, say), a tile whose plane counts above the highest shown plane isn't drawn; that's how roofs disappear.

The decoder doesn't act on any flag.

## The minimap

The minimap (and nothing else) uses randomised colours. Each floor gets a second packed colour when it loads, with up to ±8 on the hue and ±24 on saturation and lightness. A floor with `secondary_rgb` takes that colour's version instead (see [Flo](../records/flo.md)). Each time a scene is built, the blended underlay hue and lightness drift by a small random offset too, kept within ±8 and ±16. Minimap colours are then lit at a fixed light of 96. A textured overlay shows its texture's average colour. With every random term at 0, the minimap's colours are the ground's colours at light 96.

## What the region viewer draws

The explorer builds each plane's ground with [`rs2classic::terrain`](../../crates/rs2classic/src/terrain.rs) exactly as above: blended underlays, lighting, shadows from walls and scenery, every shape and rotation, textures and invisible floors. It shows one region at a time, but builds it from a 3 × 3 block of regions: the region and its eight neighbours, stitched into one grid. The blend window, the edge heights, the light and the shadows (from neighbouring locations within 16 tiles) all read across the region's edges as the original client's scene does, so a region's ground meets its neighbours' without a seam, and the region drawn alone looks the same as it would in the middle of a larger scene. The classic view draws that model as it is. The modern view turns the same triangles into meshes in the same lit colours and textures, drawn unlit. What it doesn't do:

- **One region on screen.** The neighbours inform the region's ground but aren't drawn. Where a neighbour doesn't exist (the edge of the map), the blend window stops at the edge and grid points past it reuse the nearest tile's height, so that edge is lit as if level. The original client darkens a missing region's ground instead.
- **No flags.** Tile flags don't change what's drawn: no bridges, no roofs hidden by plane. Each plane can be toggled on and off instead. Locations on a bridge tile's plane still cast their shadow on that plane.
- **Plain placement.** Locations sit at their footprint's centre at their tile's ground height, turned by quarter turns (see [Assembly](assembly.md#placement-rotation)). Models aren't fitted to sloping ground.

The world map is plane 0 of every region at one pixel per tile, coloured flat from each tile's overlay floor, else its underlay floor: a floor's RGB, or for a textured floor its texture's average colour (the mean of the texture's palette after the brightness curve, then a further brightness curve of 1.4). It's black where a tile has no floor. It isn't blended or lit.

## See also

- [Map regions](../records/map.md): the tile and location file layouts.
- [Floors](../records/flo.md): the `flo.dat` records overlays and underlays name.
- [Colour](colour.md#from-rgb-floor-colours): how a floor's RGB becomes HSL.
- [Lighting](lighting.md): the model lighting the ground lighting shares its light with.
- [Coordinates](coordinates.md): units, the 240-unit storey and the axis directions.
- [Assembly](assembly.md): how placed objects are shaped and turned.
- [Explorer](../tools/explorer.md#maps-and-the-world-map): navigating regions in the GUI.
