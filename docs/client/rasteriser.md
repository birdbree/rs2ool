[Home](../README.md) › [Client](README.md) › Rasteriser

# Rasteriser

How the original client draws a lit model: projection, face culling, depth sorting with priorities, and the three triangle fillers, all in fixed-point integer arithmetic. The tools reimplement it; this page describes that reimplementation and the two places it deliberately differs.

Implemented in [`rs2classic`](../crates/rs2classic.md): projection and face order in [`render.rs`](../../crates/rs2classic/src/render.rs), the fillers in [`raster.rs`](../../crates/rs2classic/src/raster.rs), texture pages in [`texture.rs`](../../crates/rs2classic/src/texture.rs) and lookup tables in [`tables.rs`](../../crates/rs2classic/src/tables.rs). Its input is a model lit as described in [lighting](lighting.md). Integer arithmetic wraps on overflow and division truncates towards zero, as in the original client.

## Tables

```text
sin[k]   = trunc(65536 · sin(k · 0.0030679615))     k = 0 … 2047
cos[k]   = trunc(65536 · cos(k · 0.0030679615))
recip[k] = 32768 / k,  recip[0] = 0                 k = 0 … 2047
hsl[c]   = the colour table, c = 0 … 65535
```

Angles are 2048 per turn (see [coordinates](coordinates.md)), and sines are 16.16 fixed point, so a rotation is `(a·cos + b·sin) >> 16`. The reciprocal table replaces a division when stepping colours and shades along a span; its index is clamped to 0–2047. The colour table is the HSL conversion from [colour](colour.md) at brightness 0.8.

## Model-viewer camera

The model-viewer path takes a camera of `yaw`, `pitch`, `roll` and an offset `(x, y, z)`, where z is the distance. Each vertex is transformed in this order:

```text
roll about z (skipped when roll = 0):
  x' = (y·sin r + x·cos r) >> 16
  y' = (y·cos r − x·sin r) >> 16
yaw about y (skipped when yaw = 0):
  x' = (z·sin y + x·cos y) >> 16
  z' = (z·cos y − x·sin y) >> 16
translate by (x, y, z)
pitch about x:
  y' = (y·cos p − z·sin p) >> 16
  z' = (y·sin p + z·cos p) >> 16
```

Since pitch turns about the camera, an orbit camera has to tilt its offset with it: `Camera::orbit` sets `y = distance·sin p >> 16` and `z = distance·cos p >> 16`, which keeps the model's origin at `distance` along the view axis.

## Projection

A camera-space vertex with `z ≥ near` projects about the buffer's centre `(cx, cy) = (width / 2, height / 2)` with a focal length of 512:

```text
sx = cx + (x << 9) / z
sy = cy + (y << 9) / z
```

In the model viewer, `near` is 1. A vertex nearer than that isn't projected and is marked as behind the camera.

Each vertex also gets a depth for sorting: its camera-space z minus the camera-space z of the model's origin, `(y·sin p + z·cos p) >> 16` for the camera's offset.

## World view

To draw a model placed in the world, the camera is an eye position plus yaw and pitch. A world point is moved relative to the eye, turned by yaw about y, then by pitch about x, with the same formulas as above. Vertices nearer than `NEAR = 50` count as behind the camera.

The model's depth origin is the camera-space z of the mean of its vertices (integer division), and its extent (below) is computed from the vertices relative to that mean. A model with no vertex in front of the camera draws nothing.

## Back-face test

A face is considered only if all three vertices were projected. With screen corners `(x0, y0)`, `(x1, y1)`, `(x2, y2)`:

```text
cross = (x0 − x1)·(y2 − y1) − (y0 − y1)·(x2 − x1)
```

The face is drawn only when `cross > 0`; zero-area and back-facing faces are dropped. A face with any corner left of 0 or right of `width − 1` is flagged for horizontal clipping, and its spans are clamped. Rows are always clipped.

## Depth buckets

Faces are sorted into buckets by average depth. The bucket range comes from the model's extent, computed from its vertices (y down, so "up" is negative y):

```text
up    = max(−y)            over all vertices, at least 0
down  = max(y)             at least 0
r     = trunc(sqrt(max(x² + z²)) + 0.99)
near  = trunc(sqrt(r² + up²) + 0.99)
range = near + trunc(sqrt(r² + down²) + 0.99)
```

This `near` is an offset into the buckets, not the projection's near plane. There are `range` buckets (at least one), and a face goes in bucket

```text
(depth_a + depth_b + depth_c) / 3 + near
```

Within a bucket, faces keep model order. Buckets are drawn from the highest index (farthest) to 0 (nearest).

## Priority interleave

A model without face priorities is drawn bucket by bucket, far to near. A model with them is drawn in twelve priority groups instead. Walking the buckets far to near, each face joins the group of its priority (a priority of 12 or more drops the face), so each group is in far-to-near order. For groups 0–9 the sum of their bucket indices is kept, for the averages below; groups 10 and 11 keep each face's bucket index.

```text
avg(a, b) = (sum[a] + sum[b]) / (count[a] + count[b]),  or 0 if both are empty
d12 = avg(1, 2)
d34 = avg(3, 4)
d68 = avg(6, 8)
```

Groups 10 and 11 form one queue: group 10's faces, then group 11's. Groups 0 to 9 are then drawn in order, but:

- before group 0, queued faces are drawn while the next one's bucket is greater than `d12`;
- before group 3, while it is greater than `d34`;
- before group 5, while it is greater than `d68`;
- after group 9, whatever is left in the queue.

So the two high groups slot in behind the lower ones, as long as they're deeper than the average depth of the groups they would otherwise cover. Yes, it's 6 and 8, not 6 and 7.

## Face kinds

| Kind | Drawn with |
| --- | --- |
| 0 smooth | Gouraud filler, the three lit colours |
| 1 flat | flat filler, `hsl[lit colour]` |
| 2 textured smooth | textured filler, three shades |
| 3 textured flat | textured filler, one shade at every corner |
| 4 | flat filler, a 24-bit RGB colour used as is |

Kind 4 isn't in the cache: it's a flat face in an exact 24-bit RGB colour, for the tools' own use. Map ground is drawn with kinds 0 and 2, like the original client's (see [Terrain](terrain.md)). A textured face whose texture triangle index is out of range isn't drawn; one whose texture isn't loaded is drawn flat in grey `0x808080`. The textured filler ignores transparency; the other fillers use the face's transparency (see [alpha blending](#alpha-blending)).

## The fillers

All three walk the triangle the same way. The topmost vertex (lowest y; ties go to the earlier vertex) starts two edges, stepped in 16.16 fixed point per row, and the third edge takes over where the shorter one ends. Rows from 0 to `height − 1` are drawn; rows above the buffer are skipped by advancing the edges. Each row is a half-open span `[left, right)` with `left` and `right` the edges' integer x, and the bottom vertex's row isn't drawn. Per-corner values (colours or shades) are carried along the edges in fixed point.

On a face flagged for clipping, the span's right end is clamped to `width − 1` (exclusive, so the last column is left alone) and its left end to 0, with the start value adjusted to match.

### Gouraud

The lit HSL colours are interpolated along the span with 8 fractional bits, and looked up in the colour table once per 4-pixel run: every pixel in a run gets the same colour, and the leftover `(right − left) & 3` pixels share one more. Unclipped, the step per run is `(end − start)·recip(runs) >> 15` with `runs = (right − left) >> 2`. Clipped, it's four times the per-pixel step `(end − start) / (right − left)`, or 0 for spans of 3 pixels or fewer.

Interpolating the packed 16-bit value sounds like it should smear hue into saturation. It doesn't, because all three corners of a face share the same hue and saturation bits and differ only in lightness.

### Flat

Every pixel of the span gets the one RGB colour.

### Perspective-textured

The face's texture triangle `P, M, N` (camera-space positions of those three vertices; see [textures](../records/texture.md#textured-faces)) sets up three linear functions of the screen position:

```text
M' = P − M
N' = N − P

U0 = (N'.x·P.y − N'.y·P.x) << 14
Ux = (N'.y·P.z − N'.z·P.y) << 8
Uy = (N'.z·P.x − N'.x·P.z) << 5

V0 = (M'.x·P.y − M'.y·P.x) << 14
Vx = (M'.y·P.z − M'.z·P.y) << 8
Vy = (M'.z·P.x − M'.x·P.z) << 5

D0 = (M'.y·N'.x − M'.x·N'.y) << 14
Dx = (M'.z·N'.y − M'.y·N'.z) << 8
Dy = (M'.x·N'.z − M'.z·N'.x) << 5
```

For row `y`, `U = U0 + Uy·(y − cy)` (likewise V and D). At the span's left end, each gets `(Ux >> 3)·(left − cx)` added; `Ux`, `Vx` and `Dx` are the change per 8 pixels. The texture coordinates are

```text
u = U / (D >> 14),  v = V / (D >> 14)
```

worked out only at the span start and every 8 pixels after it, and stepped linearly by `(next − current) >> 3` per pixel in between. Both are in texels × 128. `u` is clamped to 0–16256 (7–16256 after the first divide); `v` isn't clamped and wraps. Across the face, u runs from 0 at P to 128 texels at M, and v from 0 at P to 128 at N.

The shade (0–127, from lighting) is interpolated along the span too, but changes only once per 8-pixel run. For shade `S`, the texel is read from darkened copy `(S >> 4) & 3` of the page and then shifted right by `S >> 6`:

```text
texel = page[copy·16384 + (v & 0x3f80) + (u >> 7)] >> (S >> 6)
```

A texel of 0 is transparent and leaves the pixel alone, unless the page has no transparent texels at all.

## Texture pages

Each texture (see [textures](../records/texture.md)) is prepared once as a page of four 128 × 128 copies:

1. Take the [untrimmed](../records/sprite.md#untrimmed-images) sub-sprite 0.
2. Pass every palette colour through `gamma` at the brightness (see [colour](colour.md#brightness)). Any entry other than index 0 whose colour would mask to 0 becomes 1, so only index 0 is transparent.
3. Fill 128 × 128 texels. If the texture is 64 pixels wide, each pixel is doubled in both directions; otherwise it's read as 128 wide.
4. Mask every texel to `0xf8f8ff`. Any texel that is now 0 marks the page as having transparency.
5. Build three darker copies, each masked to `0xf8f8ff` again:

```text
copy 1 = t − (t >> 3)              7/8
copy 2 = t − (t >> 2)              3/4
copy 3 = t − (t >> 2) − (t >> 3)   5/8
```

The mask clears the low three bits of red and green, so a whole texel can be shifted right by up to three bits without one channel's bits landing in the next.

## Alpha blending

A face's transparency `a` (0 opaque) blends each pixel with what's already there:

```text
out = src·(256 − a) / 256 + dst·a / 256
```

computed in two halves: red and blue together under the mask `0xff00ff`, green under `0xff00`, each product shifted right by 8 and re-masked. With `a = 0` the source is written directly.

## Deviations from the original client

- **Faces crossing behind the camera are skipped.** A face with any vertex nearer than the near plane (1 in the model viewer, 50 in the world view) isn't drawn at all, rather than clipped against the plane.
- **Out-of-range buckets drop the face.** A face whose bucket index falls outside `0 … range − 1` isn't drawn.

## See also

- [Lighting](lighting.md): the lit colours and shades the fillers consume.
- [Colour](colour.md): the HSL table and the brightness curve.
- [Textures](../records/texture.md): texture ids and P/M/N triangles.
- [Models](../records/model.md): face types, priorities and transparency.
- [Coordinates](coordinates.md): axes and angle units.
- [Explorer](../tools/explorer.md): the classic renderer view.
