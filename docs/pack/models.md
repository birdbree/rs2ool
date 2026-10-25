[Home](../README.md) › [Pack format](README.md) › Models

# Models

A `models` record (kind 1) is one triangle mesh in pack space: vertices, faces, a material and palette colour per face, and optional UV, alpha, priority and vertex-label sections. This page gives the byte layout and the checks the encoder and decoder make.

Implemented in [`contentpack::records::model`](../../crates/contentpack/src/records/model.rs). All integers are little-endian.

## Layout

```text
magic          4 bytes   "RMDL"
version        u8        3
vertexCount    u16
faceCount      u16
flags          u8        which optional sections follow (see below)
attachCount    u8
vertices       vertexCount × { x i16, y i16, z i16 }
faces          faceCount × { a, b, c }       u8 each when vertexCount < 256, else u16 each
materialSlot   faceCount × u8                material id per face
paletteIndex   faceCount × u16               palette entry per face
[UV]           faceCount × { u0 u8, v0 u8, u1 u8, v1 u8, u2 u8, v2 u8 }
[ALPHA]        faceCount × u8                255 = opaque
[PRIORITY]     faceCount × u8
[LABELS]       vertexCount × u8              vertex label per vertex
[FACE_GROUPS]  faceCount × u8                fade group per face
attachPoints   attachCount × { nameHash u32, x i16, y i16, z i16, yaw u8 }
```

Bracketed sections are present only when their flag is set, and always in this order.

Face indices use one byte each when the model has fewer than 256 vertices and two bytes otherwise. No flag says which: the vertex count does, so read it first.

## Flags

| Bit | Value | Section |
| --- | --- | --- |
| 0 | 1 | `UV`: per-face texture coordinates |
| 1 | 2 | `ALPHA`: per-face alpha |
| 2 | 4 | `PRIORITY`: per-face priority |
| 3 | 8 | `LABELS`: per-vertex labels |
| 4 | 16 | `FACE_GROUPS`: per-face fade groups |

Bits 5–7 are unused. The encoder derives the flags from which sections are present, so it never writes them.

## Fields

- **Vertices** are in pack space: `x` is the cache x, `y` and `z` are the cache y and z negated. That's a rotation, so faces keep the cache's winding. See [Pack format](README.md#coordinates).
- **Faces** are triangles `a, b, c`, indices into the vertex list.
- **materialSlot** is the face's material id. It's a `u8`, so it can name materials 0–255 only. The tools write 0 (the palette material) for coloured faces and `1 + t` for faces textured with texture `t`. See [Record ids](record-ids.md#materials).
- **paletteIndex** is an index into the palette record of the `materials` pack ([Materials](materials.md#palette)). The tools write 0 for textured faces.
- **UV** gives `(u, v)` for corners `a`, `b` and `c`, each 0–255 for 0 to 1 across the texture. When the section is present every face has an entry; untextured faces have all zeros.
- **Alpha** is per-face opacity; 255 is opaque. The tools write 255 minus the cache model's face alpha.
- **Priority** is the face's draw-order priority value, copied from the cache model.
- **Labels** give each vertex's label. A rig binds labels to bones through its `labelBone` table ([Rigs](rigs.md)).
- **Face groups** give each face's fade group, which a clip's fades change the opacity of ([Anims](anims.md#fades)). The tools write the cache model's face labels. A model without the section has no faces a clip can fade.
- **Attach points** are named points with a position in pack space and a `u8` yaw. The tools never write any, so `attachCount` is 0 in their output.

## Invariants

The encoder checks these before writing, and the decoder checks them again after reading. A record that fails one is `Invalid`:

- `vertexCount` and `faceCount` are at most 65535, and there are at most 255 attach points.
- `materialSlot` and `paletteIndex` have exactly one entry per face.
- `UV`, `ALPHA`, `PRIORITY` and `FACE_GROUPS`, when present, have exactly one entry per face.
- `LABELS`, when present, has exactly one entry per vertex.
- Every face index is below `vertexCount`.

The decoder also rejects a bad magic, a version other than 2 or 3, a record that ends early, and trailing bytes. It doesn't check that material slots name materials that exist, or that palette indices are within the palette.

## Version 2

Version 2 is the same record without the `FACE_GROUPS` section. Decoders still read it, and the encoder always writes version 3. Version 3 added face groups so that clips can carry fades.

## Ids

Model record ids are listed on [Record ids](record-ids.md#models): cache model ids as they are, `0x10000000 | npc` for NPCs, `0x20000000 | idk` for identity-kit parts, `0x30000000` for the player body, and `0x40000000 | loc << 4 | index` for location models.

## See also

- [Models conversion](../conversion/models.md): how the tools fill each section from a cache model.
- [Model](../records/model.md): the cache's own model format.
- [Materials](materials.md): the material ids and the palette that faces refer to.
- [Rigs](rigs.md): how vertex labels bind to bones.
- [Textures](textures.md): the atlas rectangles that texture materials name.
