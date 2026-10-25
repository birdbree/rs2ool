[Home](../README.md) › [Conversion](README.md) › Models

# Models

This page covers how one decoded cache model becomes a `models` record: the axis change, the range check, colours, textured faces, alpha, priorities and labels. Creatures and objects go through the same function after they've been assembled, so everything here applies to them too.

Implemented in `rs2convert::model::convert` ([`model.rs`](../../crates/rs2convert/src/model.rs)). The record layout is on [Models](../pack/models.md), and the cache model format on [Model](../records/model.md). Pack data is little-endian.

## Axes

Every vertex goes through `rs2convert::space::to_pack`: `(x, y, z) → (x, −y, −z)`. The cache is right-handed with y down. Negating y and z together is half a turn about x, which is a rotation, so handedness and face winding are kept and faces are copied as they are. Negating y alone would mirror every asset. [Coordinates](../client/coordinates.md) has the details.

## The i16 range check

Pack vertices are `i16`. Each pack-space coordinate has to fit −32768…32767, or the whole conversion fails with an error naming the axis and value (`y 40000 does not fit i16`). Nothing is clamped or wrapped.

## Colours and palette indices

Every face starts with material slot 0, the palette material, and palette index 0. Untextured faces get `palette.index_of(colour)`, the nearest [palette](palette.md) entry to the face's HSL colour. Recolours have already been applied by the time a creature or object model gets here.

## Textured faces

A face is textured when the model has face types and the face's type has bit 1 set (`type & 2`). Its colour field then holds a texture id, and `type >> 2` picks one of the model's texture triangles, three vertex indices P, M and N.

When the texture id is below 50 and that triangle exists:

- the material slot is `1 + texture id`, the texture's material (see [Textures and sprites](textures-and-sprites.md));
- the palette index stays 0;
- the face's three UVs are computed from the triangle.

The UVs project each corner onto the plane spanned by `M − P` and `N − P`, with u along `M − P` and v along `N − P`. With `e1 = M − P` and `e2 = N − P`:

```text
a = e1·e1    b = e1·e2    c = e2·e2    det = a·c − b²
for each corner V:
    d1 = (V − P)·e1    d2 = (V − P)·e2
    u = (c·d1 − b·d2) / det
    v = (a·d2 − b·d1) / det
    stored u = clamp(round(u × 255), 0, 255)
    stored v = clamp(round(v × 255), 0, 255)
```

If `|det| < 1e-9` (a degenerate texture triangle), every UV is `(0, 0)`. Coordinates outside the triangle's unit square saturate at 0 or 255 instead of wrapping, so it's worth remembering when a face's texture looks stretched at the edges. The projection runs on the cache-space positions, which give the same dot products as pack space because the axis change is a rotation.

When the texture id is 50 or more, or the triangle index points past the model's triangles, the face is logged at `warn` ("textured face without a usable texture; drawn as colour") and treated as untextured. Its texture id is looked up in the palette as if it were an HSL colour.

The UV section is written only when at least one face got a texture. Untextured faces then hold zeros in it.

## Alpha

If the model has face alphas, each one is stored as `255 − alpha`. In the cache 0 is opaque, while the record uses 255 for opaque. Models without face alphas have no alpha section.

## Priorities

| Cache model | Record |
| --- | --- |
| per-face priorities | copied as they are |
| no per-face priorities, uniform priority ≠ 0 | the uniform value repeated for every face |
| no per-face priorities, uniform priority 0 | no priority section |

## Labels and attach points

Vertex labels are copied into the record's label section, which [rigs](rig-inference.md) bind to bones. Face labels, which alpha transforms address, become the record's face groups, which clips' [fades](../pack/anims.md#fades) address. No attach points are written, so `attachCount` is always 0.

## See also

- [Model](../records/model.md): the cache format and its sections.
- [Models record](../pack/models.md): the byte layout these fields go into.
- [Creatures](creatures.md): merged meshes converted through this function.
- [Objects](objects.md): shaped location models converted through this function.
