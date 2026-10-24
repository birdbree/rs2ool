[Home](../README.md) › [Conversion](README.md) › Rig inference

# Rig inference

The 317 cache has no skeletons. A frame base just lists transform slots, each with a set of vertex labels it moves. This page describes how `rs2convert` derives a bone hierarchy from those label sets, and how the result becomes a `rigs` record.

Implemented in [`rs2convert::rig`](../../crates/rs2convert/src/rig.rs). The record layout is on [Rigs](../pack/rigs.md), and the frame base on [Frame pack](../records/frame-pack.md).

## Candidates

A **bone candidate** is a distinct label set that the creature's frames actually move:

- Every frame of every collected [sequence](creatures.md) is visited, and every transform in it.
- Only translate, rotate and scale slots count. Origin and alpha slots don't.
- The slot's labels are intersected with the labels the mesh's vertices use. An empty result is skipped.
- Each distinct set is a candidate. Alongside it the code keeps a **rotation count**: how many rotate transforms with a non-zero angle used that set. A set that only ever translates or scales is still a candidate, with a count of 0.

Candidates are held sorted by set (lexicographic over sorted labels). Wherever a tie below goes to the earliest candidate, "earliest" means first in this order. Restricting to mesh labels can collapse two different slots into one candidate, and they become one bone.

A mesh with no vertex labels can't be rigged, and inference fails with an error.

## Parents

For each candidate B:

1. Collect the **strict supersets** of B among the candidates.
2. **Transitive reduction**: drop any superset A that strictly contains another superset of B. What's left are the inclusion-minimal supersets, with nothing in between them and B.
3. **Tie rule**: of those, the one with the most labels wins. Equal sizes go to the earliest candidate.

A candidate with no superset has no parent yet.

## Co-movement attach

A parentless candidate B can still hang off a candidate A that moves it. A qualifies when:

- A has more labels than B;
- A and B share at least one label;
- A's rotation count is above 0;
- B's pivot lies inside A's bounding box (the axis-aligned box of A's vertices in the rest mesh, edges included).

The largest qualifying A wins, with ties going to the earliest. If none qualifies, B stays parentless and ends up under the root.

## Overrides

`--override <file>` replaces parent choices. The file is JSON, a list of `[child labels, parent labels]` pairs, each a sorted label list naming a candidate exactly:

```json
[[[1, 3], [2, 8]], [[39, 41], []]]
```

An empty parent list means the root. Overrides are applied after the co-movement step, so they win over both rules. A list that names no candidate is an error. If the result has a cycle, inference fails with `parent cycle`.

## Ordering and the root

Candidates are sorted by depth (number of parent steps), then by size, largest first, then by set. That puts parents before children. Bone 0 is a **root** named `root`, with no labels, pivot `(0, 0, 0)` and no parent. The candidates follow as bones 1, 2, …, each named `bone_<n>` for now. A candidate without a parent gets the root as its parent.

## Label binding

Each label the mesh uses binds to the **smallest** bone whose label set contains it, so it lands on the deepest bone that moves it. Equal sizes go to the later bone. A label in no candidate binds to the root. In the record's 256-entry `labelBone` table, every mesh label has a bone, and every other entry is 255 (unbound).

## Naming

Bones are then renamed from a fixed table of body regions, by vertex label:

| Name | Labels |
| --- | --- |
| `head` | 1, 3 |
| `torso` | 2, 8 |
| `waist` | 4, 5, 29, 30 |
| `pelvis` | 39, 41 |
| `r_upper_arm` | 17, 20, 21 |
| `r_forearm` | 19 |
| `r_hand` | 27 |
| `l_upper_arm` | 23, 25, 26 |
| `l_forearm` | 22 |
| `l_hand` | 28 |
| `r_thigh` | 34, 42 |
| `r_shin` | 31, 32, 48 |
| `r_foot` | 45 |
| `l_thigh` | 35, 40 |
| `l_shin` | 37, 38, 47 |
| `l_foot` | 46 |

In bone order, each bone counts how many of the labels bound to it fall in each region not yet taken. The region with the highest count wins and is taken. On a tie, the region later in the table wins. A bone with no bound labels in any free region keeps `bone_<n>`. The table applies to every creature, humanoid or not. A non-humanoid that happens to use label 19 ends up with a bone called `r_forearm` all the same.

## Pivots

A bone's pivot is the centroid of its vertices in the rest mesh (scale baked in): the mean of every vertex whose label is in the bone's set, in cache coordinates, with integer division per axis. A set with no vertices gets `(0, 0, 0)`. The root's pivot is `(0, 0, 0)`.

## The record

`InferredRig::record` builds the `rigs` record:

| Field | Value |
| --- | --- |
| `nameHash` | FNV-1a (32-bit, offset `0x811c9dc5`, prime `0x01000193`) of the bone name's bytes |
| `parent` | parent bone index, 255 for the root |
| `restPos` | `to_pack(pivot − parent pivot)`: y and z negated, as `i16`; a bone further from its parent than an `i16` reaches is an error |
| `restRot` | `(0, 0, 0)` |
| `labelBone` | as above |

`restPos` is relative to the parent and in pack space, so a bone's rest pivot is the sum of `restPos` down its chain. More than 254 bones, root included, is an error. The record is encoded and decoded once before clips are validated against it.

## See also

- [Clips](clips.md): keys computed against these bones and pivots.
- [Validation](validation.md): the bone summary in `validation.json`.
- [Inference](inference.md): the same label-to-region table, used to place items in equipment slots.
- [Explorer](../tools/explorer.md#vertex-colours-and-bones): **View › Bones** draws the bones and pivots on the converted creature as it plays.
- [Animation](../client/animation.md): how the original client applies frame bases.
- [Glossary](../glossary.md): label, transform slot, rig.
