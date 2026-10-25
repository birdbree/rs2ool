[Home](../README.md) › [Pack format](README.md) › Anims

# Anims

An `anims` record (kind 9) is one clip: per-frame rotation, translation and scale keys for the bones of one [rig](rigs.md), with frame timing in 20 ms cycles. This page gives the layout, the timing fields, the key ordering, and the rotation convention.

Implemented in [`contentpack::records::anim`](../../crates/contentpack/src/records/anim.rs); the rotation matrices are in [`rs2convert::clip`](../../crates/rs2convert/src/clip.rs). All integers are little-endian.

## Layout

```text
magic        4 bytes   "RANM"
version      u8        3
rig          u32       id of the rigs record this clip drives
frameCount   u8
frameStart   frameCount × u16   start of each frame, in 20 ms cycles
duration     u16       total length, in 20 ms cycles
loopOffset   u16       frames from the end to replay; 65535 = none
priority     u8
replayMode   u8
keyCount     u32
keys         keyCount × {
               bone   u8        bone index in the rig
               t      u8        frame index
               rot    u8 × 3    rx, ry, rz in 1/256 turn
               pos    i16 × 3   translation, pack space
               scale  u16 × 3   scale per pack-space axis, 256 = 1.0
             }
```

Each key is 17 bytes. A key's bytes put `bone` before `t`, but keys are sorted by `t` first. Don't let the byte order fool you into sorting the other way.

## Timing

Frame `i` starts `frameStart[i]` cycles into the clip and runs until `frameStart[i + 1]`, or until `duration` for the last frame. A cycle is 20 ms ([Animation](../client/animation.md)).

The tools set `frameStart[0]` to 0 and each later start to the sum of the earlier frames' lengths, where a frame's length is the sequence's delay for that frame, else the frame pack's own delay, else 1. `duration` is the sum of all lengths. Both are capped at 65535.

`loopOffset` is the sequence's loop offset, the number of frames from the end to replay, or 65535 (`NO_LOOP`) when the sequence has none. `priority` and `replayMode` are copied from the [sequence](../records/seq.md) unchanged.

## Keys

A key sets bone `bone`'s local transform at frame `t`:

```text
L = T(restPos[bone] + pos) · R(rot) · S(scale / 256)
```

where `restPos` comes from the rig. A bone with no key at a frame keeps its rest transform, `T(restPos[bone])`. The world transforms and skinning that follow are on [Rigs](rigs.md#pose-formulas).

`S` scales each pack-space axis about the bone's pivot, before the rotation; children inherit it through their parent's world transform. Keys sit on frame indices only; the format has no keys between frames. The tools write one key for every bone at every frame, root included, so `keyCount` is `frameCount × boneCount` in their output.

## Version 2

Version 2 is the same record without `scale`, so each key is 11 bytes. Decoders still read it, with every key at scale 1.0 (256), and the encoder always writes version 3. Version 3 added scale for sequences that grow or shrink a model: most spot animations, and some NPC vanishes and deaths.

## Rotation

`rot = (rx, ry, rz)` are angles in 1/256 of a turn. The angle for a value `v` is `v · 8` steps of the original client's 2048-entry sine table, `θ = 2π · v / 256`, with sines and cosines taken from that table as 16.16 fixed point (divided by 65536).

The rotation is the 317 one, conjugated into pack space:

```text
R(rx, ry, rz) = P · R317(rx, ry, rz) · P          P = diag(1, −1, −1)

R317 = Ry · Rx · Rz                                applied z first, then x, then y

Rz = |  cz   sz   0 |    Rx = | 1   0    0  |    Ry = |  cy  0   sy |
     | −sz   cz   0 |         | 0   cx  −sx |         |  0   1   0  |
     |  0    0    1 |         | 0   sx   cx |         | −sy  0   cy |
```

`cx = cos θx`, `sx = sin θx` and so on; the matrices act on column vectors. Watch the sign on `Rz`: it turns the other way from `Rx` and `Ry`'s textbook form, because that's how the original client's integer rotation turns.

`P` is the cache-to-pack axis flip (y and z negated), so `P · R317 · P` is the same rotation the original client applies, expressed in pack coordinates. See [Coordinates](../client/coordinates.md).

## Invariants

The encoder checks these before writing and the decoder after reading; a failure is `Invalid`:

- At most 255 frames.
- Keys are sorted by `(t, bone)`, strictly: no two keys share a `(t, bone)` pair.
- Every key's `t` is below `frameCount`.

The decoder also rejects a bad magic, a version other than 2, a record that ends early, and trailing bytes. A clip doesn't carry its rig's bone count, so nothing checks `bone` against the rig, and nothing checks that `frameStart` is increasing or that `loopOffset` is in range.

## Ids

Clip `rig << 16 | seq` is sequence `seq` converted for rig `rig`, and its `rig` field holds the same rig id ([Record ids](record-ids.md#rigs-and-anims)).

## Descriptions

With `--describe`, the anims pack's manifest carries a `descriptions` table in its `extra` object, saying what each described clip shows:

```text
"descriptions": { "<clip id>": { "seq": <n>, "class": <class or null>, "role": <role or null>, "text": "…", "by": "<labeller>", "reviewed": <bool> } }
```

The key is the clip id in decimal. `class` is one of the [caption classes](../tools/captions.md#the-question-and-the-classes), and `role` the [role](../conversion/inference.md#learning-roles-from-labels) it amounts to. `text` is a short description, `by` names who labelled it, and `reviewed` says whether someone checked it by eye. A clip without a caption has no entry, and with no captions at all there's no table. The clip records don't change, so a reader that doesn't know the table loses nothing.

## See also

- [Clips](../conversion/clips.md): how cache frames become keys, and how angles are quantised.
- [Rigs](rigs.md): bones, rest offsets and the skinning formula.
- [Validation](../conversion/validation.md): how converted clips are checked against the original client's deformation.
- [Animation](../client/animation.md): how the original client applies frames and times sequences.
- [Sequence](../records/seq.md): the cache sequences clips come from.
