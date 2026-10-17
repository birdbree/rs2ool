[Home](../README.md) › [Client](README.md) › Coordinates

# Coordinates

How the 317 cache lays out space and angles, and how the tools map that frame into pack space and the explorer's world. Read this before you move a vertex, a bone or a camera between the two, because the mistakes here look like art problems rather than code problems.

Implemented in [`rs2convert::space`](../../crates/rs2convert/src/space.rs), the explorer's `world` in [`mesh.rs`](../../src/explorer/mesh.rs), and the angle tables in [`rs2convert::deform`](../../crates/rs2convert/src/deform.rs).

## The cache frame

Model vertices, frame deltas, tile heights and placed locations all share one frame:

| Axis | Direction in the cache |
| --- | --- |
| x | along the tile x axis |
| y | vertical, **down**: more negative is higher |
| z | along the tile y axis |

The frame is right-handed (x × y = z). The corner of tile `(x, y)` in a region sits at cache `(x·128, height, y·128)`. Ground heights are zero or negative: plane-0 derived heights, for example, fall between −480 and −80 (see [Terrain](terrain.md)).

The explorer's world map and region step buttons treat +x as east and +tile y as north.

## Units

| Quantity | Units |
| --- | --- |
| One tile | 128 |
| One region (64 × 64 tiles) | 8192 along each side |
| One plane (storey) | 240 |
| Explicit tile heights | steps of 8 |
| Scales in definitions | 128 = 1.0 (`v · scale / 128`) |

The explorer's 3D viewport draws a 128-unit grid and a 128 × 240 × 128 box (one tile, one storey) beside every model, so a scale error shows at a glance.

## Angles

The original client's rotation tables have 2048 entries per turn and hold 16.16 fixed-point values:

```text
sin[k] = trunc(65536 × sin(k × 0.0030679615))   k = 0..2047
cos[k] = trunc(65536 × cos(k × 0.0030679615))
```

`0.0030679615` is 2π / 2048. Because of the truncation a quarter turn gives `sin[512] = 65535`, not 65536. Lookups wrap with `k & 2047`.

Animation frames store rotations as 8-bit angles, 256 per turn, and scale them into the table with `(v & 0xff) × 8`. A quarter turn is 64 frame units, which is 512 table units. Rotation order and sign conventions are on [Animation](animation.md).

Placed locations rotate by quarter turns only, a rotation value of 0–3 (see [Assembly](assembly.md#placement-rotation)). The classic renderer's camera yaw and pitch are in the same 2048-per-turn units.

## Pack space

Pack space is y up. The tools map a cache point into it by negating y **and** z:

```text
to_pack(x, y, z) = (x, −y, −z)
P = diag(1, −1, −1)
```

`P` has determinant +1, so it's a rotation, half a turn about the x axis. Handedness survives: the mapped x, y and z axes still satisfy x × y = z, and a unit test checks that. In pack space +y is up and the tile y direction runs along −z.

Negating y alone would also make y point up, and it's the obvious fix for a y-down frame. It's also a mirror (determinant −1): every model would come out flipped left for right, with its face winding reversed. Converted models, rig pivots and clip keys would all agree with each other, which is why nobody notices until the art looks subtly wrong.

What the mapping means in practice:

- Points and offsets map with `P`: `to_pack([1, −2, 3]) = [1, 2, −3]`.
- A rotation `R` in cache space becomes `P · R · P` in pack space. Clip keys are converted this way.
- Distances are unchanged, so a validation error measured in pack space is the same number of cache units.

## The explorer's world frame

The explorer uses the same mapping for everything it draws with the modern renderer. Its `world` function turns a cache vertex `(x, y, z)` into `(x, −y, −z)`, and a terrain corner at tile `(x, y)` with height `h` lands at `(x·128, −h, −y·128)`. Pack models need no conversion at all, since pack space is the explorer's world space. Cache models, converted models and map terrain therefore sit in one frame, which is what lets compare mode put an original and its converted version side by side under one camera.

The classic renderer works in cache coordinates. It takes the orbit camera's eye and target, negates their y and z to get back to the cache frame, and derives its yaw and pitch from there.

## See also

- [Animation](animation.md): frame angles, rotation order and the sine tables in use.
- [Terrain](terrain.md): tile heights, planes and how the 240-unit storey is applied.
- [Assembly](assembly.md): mirroring and quarter-turn placement of objects.
- [Clips](../conversion/clips.md): how frame rotations become pack-space keys.
- [Glossary](../glossary.md): pack space, plane, region.
