[Home](../README.md) › [Conversion](README.md) › Validation

# Validation

Every converted clip is checked against the original client's own deformation, vertex by vertex and frame by frame. This page gives the pose formula, the error measure, the threshold, and the shape of the two report files.

Implemented in [`rs2convert::validate`](../../crates/rs2convert/src/validate.rs), with the report files written by [`rs2convert::run::write`](../../crates/rs2convert/src/run.rs).

## What is compared

Validation doesn't use the in-memory rig or keys. `pipeline::convert` encodes the rig record and each clip record, decodes them again, and validates the decoded copies. The report therefore describes the records exactly as they're stored in the pack.

The **reference** for frame `i` is the merged, unscaled mesh deformed by that frame with the integer frame application from [`rs2convert::deform`](../../crates/rs2convert/src/deform.rs), then scaled by the creature's scale, as the original client does it (see [Animation](../client/animation.md)). Positions are compared in pack space: y and z negated on both sides, which is a rotation, so distances are in the same units as the cache.

## The pose formula

From the rig and clip records alone, for frame `t`:

```text
pivot_b = pivot_parent + restPos_b                     (root: restPos_root)
local_b = T(restPos_b + pos) · R(rot) · S(scale/256)   if bone b has a key at t
        = T(restPos_b)                                 otherwise
world_b = world_parent · local_b                       (root: local_root)
R(rot)  = P · Ry · Rx · Rz · P,  P = diag(1, −1, −1)
S(s)    = diag(s)                                      (per axis)
```

`Ry · Rx · Rz` is the original client's rotation for 8-bit angles (z first, then x, then y), described on [Clips](clips.md). A vertex with rest position `v` (pack space) and label `l` follows bone `b = labelBone[l]`, or bone 0 when that entry is 255:

```text
skinned = world_b · (v − pivot_b)
```

The rest positions are the creature's rest mesh, with scale baked in, in pack space.

## Error per vertex

The error for a vertex is the Euclidean distance between `skinned` and the reference position. For each frame the report keeps:

- `max`: the largest vertex error;
- `mean`: the mean over all vertices;
- `worst`: the five largest, each with vertex index, label, bone and error, largest first. Equal errors stay in vertex order.

A frame that can't be found in its pack is skipped. For the clip, `max` is the largest frame `max` and `mean` is the mean of the frame means.

## The threshold

A clip is **flagged** when its `max` exceeds `validate::THRESHOLD`, which is 4.0 units, or when any face fades differently from the original client ([Fades](#fades)). A creature is flagged when any of its clips is, alone or mixed.

Shear isn't carried into keys (see [Clips](clips.md#what-isnt-carried)), so a sequence that relies on it can end up flagged. When a clip goes over, the per-frame `worst` list tells you which vertex, label and bone to look at first.

## Fades

A clip's [fades](clips.md#fades) are exact, so they're checked face for face rather than against a threshold: at every frame, each face's transparency under the clip's fades (from the model's own) has to equal the original client's (`Creature::reference_alphas`). `fade_mismatches` counts the faces that don't, over every frame, and any at all flag the clip. All 592 spot animation clips, 436 of which fade, have none.

## Mixes

A clip with [moving bones](clips.md#moving-bones-and-mix-keys) is also checked mixed with each of the creature's movement clips (its walk and turn sequences, for the player 819–824), when they share its frame base. `validate::validate_moving` poses every pair of their frames through `pose_moving`, the moving bones from the movement clip's keys and the rest from the clip's mix keys:

```text
local_b = movement key of b at its frame   if b is a moving bone
        = mix key of b at the clip's frame  otherwise
```

and skins the rest mesh against the original client's mix of the two sequences (`Creature::reference_moving`). It checks the mix's fades the same way, the clip's own against the original client's (`reference_alphas_moving`), and counts mismatches in the mix's `fade_mismatches`. A mix is flagged over `validate::MIX_THRESHOLD`, 8.0 units: twice the clip threshold, since it carries both clips' error. On the player, the worst of the 135 actions that share the walk's base is 2.8 mixed with the walk, the walk back or the turn, 4.6 and 6.4 mixed with walking to either side, and 5.3 mixed with the run.

## `validation.json`

Each creature gets `<dir>/<key>/validation.json`, where the key is `npc_<id>` or `player`:

```json
{
  "creature": "name",
  "bones": [
    { "name": "root", "parent": null, "labels": [] },
    { "name": "bone_1", "parent": 0, "labels": [1, 2] }
  ],
  "max": 0.0,
  "flagged": false,
  "clips": [
    {
      "sequence": 0,
      "max": 0.0,
      "mean": 0.0,
      "flagged": false,
      "frames": [
        {
          "frame": 0,
          "max": 0.0,
          "mean": 0.0,
          "worst": [{ "vertex": 0, "label": 0, "bone": 0, "error": 0.0 }]
        }
      ],
      "moving": [{ "sequence": 819, "max": 0.0, "mean": 0.0, "flagged": false }],
      "fade_mismatches": 0
    }
  ]
}
```

`bones` comes from the inferred rig: names, parent indices (`null` for the root) and sorted label sets. The top-level `max` is the largest clip `max`. Without `--with-anims`, `clips` is empty. `fade_mismatches` is only there when it isn't 0 ([Fades](#fades)), on a clip and on each of its mixes. `moving` is only there for a clip with moving bones: one entry per movement clip it was checked against, with the worst and mean error over every pair of frames ([Mixes](#mixes)).

## `validation_summary.json`

`<dir>/validation_summary.json` has one entry per creature key:

```json
{
  "npc_1": { "name": "name", "max": 0.0, "flagged": false, "bones": 0 }
}
```

`bones` here is the bone count. The writer reads any summary already in the directory, replaces the entries for the creatures it just converted and keeps the rest. Converting several creatures into one directory builds up one summary. A summary it can't parse is started over.

## Exit code 3

`rs2ool pack npc` and `rs2ool pack player-kit` print one line per clip (id, sequence, max, mean, and `FLAGGED` when over the threshold), and under it a `while moving (seq N)` line per mix checked. If any clip is flagged, the process exits with status **3** after writing every pack and report. The packs are still complete. The exit status only tells you something deserves a look. The explorer's convert action reports the same condition in its status message instead.

## See also

- [Clips](clips.md): how keys are chosen to minimise this error.
- [Rig inference](rig-inference.md): bones, pivots and label binding.
- [Pack commands](../tools/pack-commands.md): flags, outputs and exit codes.
- [`cache render --compare`](../tools/cache-commands.md#render): a clip's frames drawn beside the reference, to see where the error is.
- [Rigs](../pack/rigs.md) and [Anims](../pack/anims.md): the records being re-skinned.
