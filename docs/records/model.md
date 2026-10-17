[Home](../README.md) › [Records](README.md) › Models

# Models

A model is one file of index 1: vertices, triangles, per-face colours and render types, and the labels that animation frames act on. This page gives the full byte layout so you can write a reader, and says what the decoder checks.

Implemented in [`rs2cache::model`](../../crates/rs2cache/src/model.rs); the decoded struct is [`rs2cache::types::model::Model`](../../crates/rs2cache/src/types/model.rs). All integers are big-endian. The file is first inflated as described in [compression](../cache/compression.md).

## The trailer

The counts and flags sit at the end, in the last 18 bytes of the inflated file:

```text
vertexCount        u16
faceCount          u16
texturedFaceCount  u8
hasFaceTypes       u8    1 = present
priority           u8    255 = per-face priorities present; otherwise every face's priority
hasAlpha           u8    1 = present
hasFaceLabels      u8    1 = present
hasVertexLabels    u8    1 = present
xLen               u16   byte length of the x deltas
yLen               u16   byte length of the y deltas
zLen               u16   byte length of the z deltas
faceIndexLen       u16   byte length of the face indices
```

A flag counts as set only when the byte is exactly 1.

## Sections

The sections run back to back from offset 0, in this order. Each one starts where the previous ends, so every offset is the sum of the lengths before it. Optional sections take no space when absent.

```text
vertex flags       u8 × vertexCount
face compression   u8 × faceCount
face priorities    u8 × faceCount          only if priority == 255
face labels        u8 × faceCount          only if hasFaceLabels
face types         u8 × faceCount          only if hasFaceTypes
vertex labels      u8 × vertexCount        only if hasVertexLabels
face alphas        u8 × faceCount          only if hasAlpha
face indices       faceIndexLen bytes      signed smarts
face colours       u16 × faceCount
textured faces     (u16 × 3) × texturedFaceCount
x deltas           xLen bytes              signed smarts
y deltas           yLen bytes              signed smarts
z deltas           zLen bytes              signed smarts
trailer            18 bytes
```

The sections must fit in the bytes before the trailer. Spare bytes between the last section and the trailer are allowed and ignored.

Note that the order in the file isn't the order you'd guess from the trailer: priorities come before labels, and face labels before face types.

## Vertices

Each vertex has a flag byte. Bit 1 means an x delta follows, bit 2 a y delta, bit 4 a z delta. Each delta is a [signed smart](../cache/primitives.md) read from that axis's own stream, added to the previous vertex's coordinate. An axis whose bit is clear keeps the previous vertex's value. The first vertex starts from `(0, 0, 0)`.

Coordinates are in the cache's own space: right-handed with y pointing down, so negative y is up. See [coordinates](../client/coordinates.md).

## Faces

Each face is a triangle `[a, b, c]` of vertex indices. The face-index stream is a run of signed smarts, each a delta from the last index read, starting from 0. One compression byte per face says how to build it from the previous face:

| Type | Update | Indices read |
| --- | --- | --- |
| 1 | `a = next; b = next; c = next` | 3 |
| 2 | `b = c; c = next` (a kept) | 1 |
| 3 | `a = c; c = next` (b kept) | 1 |
| 4 | swap `a` and `b`; `c = next` | 1 |

`next` adds the next signed smart to the running index and returns it. `a`, `b` and `c` start at 0, and the running index carries over from face to face regardless of type. The face recorded is `[a, b, c]` after the update. Any other type byte is an error.

## Colours and face types

Each face has a 16-bit [HSL colour](../client/colour.md): hue in the top 6 bits, saturation 3, lightness 7.

When the face-type section is present, the low 2 bits of each byte pick how the face is drawn:

| Low bits | Render type |
| --- | --- |
| 0 | Gouraud (smooth) |
| 1 | flat |
| 2 | textured, Gouraud |
| 3 | textured, flat |

For a textured face, `type >> 2` indexes the textured-face triples, and the face's colour is not a colour at all: it's the texture id (0–49). Without the section, every face is type 0. See [lighting](../client/lighting.md) and [the rasteriser](../client/rasteriser.md) for what each type does.

## Priorities and alpha

If the trailer's priority byte is 255, a per-face priority section is present and the decoded `priority` is 0. Otherwise `priority` is the one priority of every face and there's no section. With per-face priorities, the original client sorts faces into twelve priority groups and interleaves them with depth order instead of drawing by depth alone; see [the rasteriser](../client/rasteriser.md).

The alpha section has one byte per face. 0 is opaque; the original client blends a face as `src · (256 − a) + dst · a`, so larger values are more transparent. Without the section every face is opaque.

## Labels

Vertex labels (one byte per vertex) and face labels (one byte per face) group the model for animation. A [frame pack](frame-pack.md)'s base lists, for each transform slot, the labels it moves: origin, translate, rotate and scale slots act on vertex labels, alpha slots on face labels.

## Textured-face triples

Each triple is three `u16` vertex indices `[P, M, N]`. They define the texture-space triangle that textured faces map onto, so many faces can share one triple. How the original client turns them into texture coordinates is on [the rasteriser](../client/rasteriser.md) page.

## Validation

Decoding never panics. It fails with a `ModelError`:

| Error | When |
| --- | --- |
| `TooShort` | the file is shorter than the 18-byte trailer |
| `SectionOverrun` | the sections need more bytes than precede the trailer |
| `Truncated` | a stream ran out mid-read (for example, a delta stream shorter than its flags claim) |
| `BadCompressionType` | a face compression byte is not 1–4 |
| `FaceIndex` | a face index is negative or not below the vertex count |
| `TexturedFaceIndex` | a textured-face index is not below the vertex count |

Through `Cache::model` or `Records`, these are wrapped with the model id.

## The decoded struct

```text
Model
  vertices         [[i32; 3]]        one per vertex
  faces            [[u16; 3]]        one per face
  face_colours     [u16]             HSL, or texture id for textured faces
  face_types       optional [u8]
  priority         u8                0 when face_priorities is present
  face_priorities  optional [u8]
  face_alphas      optional [u8]
  face_labels      optional [u8]
  vertex_labels    optional [u8]
  textured_faces   [[u16; 3]]        [P, M, N]
```

`Model::bounds` gives the axis-aligned `(min, max)` of the vertices, or none for an empty model. The dump writes the struct to `model/<id>.json`; its metadata sidecar counts vertices, faces, textured faces and distinct labels, and with `--xref` lists the texture ids the model uses.

## See also

- [Frame packs](frame-pack.md): the transforms that move labelled vertices and faces.
- [Primitives](../cache/primitives.md): the signed smart and HSL colour encodings.
- [Assembly](../client/assembly.md): how the original client merges and recolours models.
- [Model conversion](../conversion/models.md): how a model becomes a pack model record.
- [Records](README.md): every record type.
