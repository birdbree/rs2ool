[Home](../README.md) › [Pack format](README.md) › Rigs

# Rigs

A `rigs` record (kind 7) is a bone hierarchy plus a table binding each vertex label to a bone. Together with a model's vertex labels and an [anim](anims.md) clip, it defines how every vertex moves. This page gives the layout, the invariants, the name hash, and the world-transform and skinning formulas.

Implemented in [`contentpack::records::rig`](../../crates/contentpack/src/records/rig.rs); the pose formulas are in [`rs2convert::validate`](../../crates/rs2convert/src/validate.rs). All integers are little-endian.

## Layout

```text
magic       4 bytes   "RRIG"
version     u8        2
boneCount   u8
bones       boneCount × {
              nameHash   u32
              parent     u8      bone index, 255 = none
              restPos    i16 × 3 offset from the parent's pivot, pack space
              restRot    u8 × 3
            }
labelBone   256 × u8  bone index for each vertex label 0–255; 255 = unbound
```

Each bone is 14 bytes, so a record is `5 + 1 + 14 × boneCount + 256` bytes.

- **nameHash** is the 32-bit FNV-1a hash of the bone's name (below).
- **parent** is the index of the parent bone, or 255 for a bone with no parent.
- **restPos** is the bone's pivot relative to its parent's pivot, or relative to the origin for a bone with no parent, in pack space ([Pack format](README.md#coordinates)).
- **restRot** is three bytes the tools always write as 0. The formulas below don't use it.
- **labelBone** maps a vertex label to the bone that moves it.

## Invariants

The encoder checks these before writing and the decoder checks them after reading; a failure is `Invalid`:

- At most 254 bones.
- Every bone's parent is 255 or a lower index: parents come before their children. A parent can't refer to itself or to a later bone, so the hierarchy can't contain a cycle.
- Every `labelBone` entry is 255 or a valid bone index.

The decoder also rejects a bad magic, a version other than 2, a record that ends early, and trailing bytes.

## Bone name hash

```text
h = 0x811C9DC5
for each byte c of the name, as UTF-8:
    h = (h XOR c) × 0x01000193      (wrapping 32-bit)
nameHash = h
```

That's standard 32-bit FNV-1a. The hash of the empty string is `0x811C9DC5`.

## Pose formulas

Write `T(v)` for a translation by `v`, `R(rot)` for a key's rotation matrix in pack space (defined on [Anims](anims.md#rotation)), `S(s)` for a scale by `s` per axis, and `p(b)` for bone `b`'s parent.

Rest pivots, in bone order:

```text
Q[b] = restPos[b]                  if b has no parent
Q[b] = Q[p(b)] + restPos[b]        otherwise
```

Local and world transforms at frame `t` of a clip:

```text
L[b] = T(restPos[b] + pos) · R(rot) · S(scale / 256)    if the clip has a key (t, b)
L[b] = T(restPos[b])                    if it has none
W[b] = L[b]                             if b has no parent
W[b] = W[p(b)] · L[b]                   otherwise
```

Skinning a model vertex `v` (in pack space) with label `l`:

```text
b  = labelBone[l]          (bone 0 if labelBone[l] is 255)
v' = W[b] · (v − Q[b])
```

A model without a `LABELS` section has every vertex at label 0. With no keys, `W[b] = T(Q[b])` and every vertex stays where it is, which is a quick check that your pivots add up.

Skinning is rigid: each vertex follows exactly one bone, with no weights.

## What the tools write

Bone 0 is a root named `root` with no parent, a `restPos` of 0 and no labels. Every other bone has a parent, and the bones are ordered by depth in the tree (then by label-set size, largest first), so parents always come first. A bone's pivot is the centroid of its vertices in the creature's rest mesh, and `restPos` is that pivot minus the parent's pivot, mapped to pack space. Bones are named after a body region when their labels match one, `bone_<n>` otherwise.

In `labelBone`, every label the mesh uses is bound to a bone (to the root if no bone claims it); labels the mesh doesn't use are 255.

Rig record ids are the NPC id for NPCs and `0xFFFF` for the player ([Record ids](record-ids.md#rigs-and-anims)).

## See also

- [Rig inference](../conversion/rig-inference.md): how bones, parents and pivots are chosen from frame-base label sets.
- [Anims](anims.md): the clips that pose a rig, and the rotation convention.
- [Validation](../conversion/validation.md): the skinning check against the original client's deformation.
- [Models](models.md): the vertex labels that `labelBone` binds.
- [Frame pack](../records/frame-pack.md): the cache's labels and transforms the rig is built from.
