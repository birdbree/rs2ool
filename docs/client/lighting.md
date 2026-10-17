[Home](../README.md) › [Client](README.md) › Lighting

# Lighting

The original client lights a model once, in integer arithmetic, turning each face's HSL colour into lit colours (or, for textured faces, into shades). This page gives the exact formulas, so a reimplementation produces the same values.

Implemented in [`rs2classic::light`](../../crates/rs2classic/src/light.rs). All divisions below are integer divisions that truncate towards zero, and `>>` is an arithmetic shift, as in the original client.

## Inputs

Lighting needs the model's vertices, faces, face colours and face render types (see [models](../records/model.md)), plus a light setup:

- `ambient`: the base light level;
- `contrast`: how strongly the angle to the light matters (bigger means weaker);
- `direction`: a vector `(lx, ly, lz)` in cache coordinates (y down). A face whose normal points along it gets brighter.

The low two bits of a face's render type pick its kind. A model without render types treats every face as kind 0.

| `type & 3` | Kind |
| --- | --- |
| 0 | smooth |
| 1 | flat |
| 2 | textured, smooth |
| 3 | textured, flat |

Bit 0 decides flat versus smooth; bit 1 decides textured.

The client lights the rest pose and keeps the results while the model animates, so lit colours never change with the pose.

## Face normals

For a face with corners `a, b, c`:

```text
d1 = b − a
d2 = c − a
n  = (d1.y·d2.z − d2.y·d1.z,
      d1.z·d2.x − d2.z·d1.x,
      d1.x·d2.y − d2.x·d1.y)

while any component of n is outside −8192 … 8192:
    n = n >> 1                        (all three components)

len = max(1, trunc(sqrt(n.x² + n.y² + n.z²)))
n   = n · 256 / len                   (each component)
```

After the halving, the sum of squares fits comfortably in 32 bits. The result is a normal of length about 256.

## The light's scale

```text
L = trunc(sqrt(lx² + ly² + lz²))
k = (contrast · L) >> 8
```

## Flat faces

A face with bit 0 set gets one light value from its own normal:

```text
light = ambient + (lx·n.x + ly·n.y + lz·n.z) / max(1, k + k/2)
```

## Smooth faces

Each vertex sums the normals of the smooth faces (bit 0 clear) that use it, and counts them. Flat faces don't contribute, even when they share the vertex. Then each corner of a smooth face gets:

```text
s, count = summed normal and face count at the corner's vertex
light    = ambient + (lx·s.x + ly·s.y + lz·s.z) / max(1, k · count)
```

Note the divisors differ: a flat face divides by `1.5k`, a smooth corner by `k` per contributing face. So a lone smooth face responds to the light half as strongly again as the same face drawn flat. It's easy to miss when you're comparing your output with the client's.

## Lit colour

A light value is applied to the face's 16-bit colour `c` (see [colour](colour.md)):

```text
if the face is textured (bit 1):
    shade = 127 − clamp(light, 0, 127)
else:
    lum   = clamp((light · (c & 0x7f)) >> 7, 2, 126)
    lit   = (c & 0xff80) + lum
```

For untextured faces, only the lightness changes: hue and saturation (`c & 0xff80`) pass through. A light value of 128 leaves the lightness as it is (within the clamp). The result is always between 2 and 126, never fully black or fully white.

For textured faces the colour holds the texture id, so it isn't touched. The shade runs from 0 (fully lit) to 127 (darkest), and picks among the darkened copies of the [texture page](rasteriser.md#texture-pages).

A flat face stores one value; a smooth face stores one per corner.

## The two standard setups

The original client uses one of two fixed setups, depending on what the model is:

| Setup | Used for | Ambient | Contrast | Direction |
| --- | --- | --- | --- | --- |
| `light::NPC` | NPC and player models | 64 | 850 | (−30, −50, −30) |
| `light::OBJECT` | world objects and inventory models | 64 | 768 | (−50, −10, −50) |

Both directions have negative y, so with y down both light from above. Worked through: `NPC` has `L = 65` and `k = 215`, so a flat face divides by 322; `OBJECT` has `L = 71` and `k = 213`, dividing by 319.

Map ground uses the `OBJECT` direction and contrast too, but lights grid points from the slope of the ground rather than faces, with a base of 96 instead of the ambient term, minus the shadows walls and scenery cast. See [Terrain](terrain.md#ground-lighting).

## Output

The lit model keeps the faces, each face's kind, the lit values (or shades), the original face colours (texture ids for textured faces), the texture-triangle index (`type >> 2`), per-face transparency, and face priorities. The [rasteriser](rasteriser.md) draws from that.

## See also

- [Colour](colour.md): the HSL layout and the table lit colours are looked up in.
- [Rasteriser](rasteriser.md): how lit colours and shades are drawn.
- [Terrain](terrain.md#ground-lighting): how map ground is lit.
- [Models](../records/model.md): face render types and colours.
- [Textures](../records/texture.md): what a textured face's colour means.
- [Coordinates](coordinates.md): the cache's axes, y down.
- [`rs2classic`](../crates/rs2classic.md): the renderer crate.
