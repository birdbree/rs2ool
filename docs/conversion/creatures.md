[Home](../README.md) › [Conversion](README.md) › Creatures

# Creatures

A creature is a merged, recoloured mesh plus the sequences that animate it and the frame packs those sequences use. This page covers how NPCs and the player kit are assembled into one, and which record ids the results get. The rig and clips built from a creature are on [Rig inference](rig-inference.md) and [Clips](clips.md).

Implemented in [`rs2convert::creature`](../../crates/rs2convert/src/creature.rs) and [`rs2convert::pipeline`](../../crates/rs2convert/src/pipeline.rs).

## Merging and recolouring

`merge` follows the original client's model merge, described on [Assembly](../client/assembly.md):

- A single model is used as it is, untouched.
- With several models, faces are appended in order. Each face vertex is welded to the first vertex already in the output at the same coordinates (which keeps that vertex's label), or appended if there isn't one. Texture triangle vertices are welded the same way. Vertices nothing refers to are dropped.
- Optional per-face sections (types, alphas, face labels) are present in the output if any input has them. Inputs without them contribute the defaults: type 0, alpha 0, label 0. Vertex labels default to 0 too.
- A textured face's triangle index is shifted by the number of texture triangles already merged.
- Priorities stay uniform only when no input has per-face priorities and all inputs share one uniform value. Otherwise every face gets its own, taken from its model's per-face value or its uniform priority.

`recolour` then applies the definition's recolour pairs in order. Each pair replaces every face colour equal to `from` with `to`, so a later pair can recolour the result of an earlier one.

## NPCs

`creature::npc` builds NPC `id` from its `npc.dat` entry:

1. Decode each model the definition lists, merge them and recolour.
2. Collect the sequences: stand, walk, walk back, walk left and walk right, whichever are present, followed by any extra ids passed with `--seq`. `npc.dat` names no other animations, so extras have to be given by hand ([Inference](inference.md) covers finding candidates). Repeated ids are dropped, and the first one is kept.
3. Take the scale as `[scale_xz, scale_y, scale_xz]`, where 128 is 1.
4. Use the definition's name, or `npc <id>` when it has none.

The original client scales a creature after animating it. The converter bakes that scale into a **rest mesh**: each vertex becomes `v × scale / 128` per axis, in integer arithmetic (skipped when every axis is 128). The converted model record, the rig's pivots and validation's skinning all start from this rest mesh. The unscaled merged mesh is kept too, because the reference deformation animates that one and scales afterwards, as the original client does.

## Sequences and the frame index

A 317 sequence lists global 16-bit frame ids, and nothing in the cache says which index-2 pack holds a frame. `Cache::frame_index` reads the frame id list of every index-2 file (missing files are skipped) and builds a map from frame id to pack. When two packs list the same id, the later pack wins, and the duplicate is recorded. [Frame pack](../records/frame-pack.md) has the pack format.

For each sequence, `creature::sequences` resolves every frame through that map and decodes each pack it needs once. A frame that's in no pack is an error. Each frame is then found in its pack by id. Only a sequence frame's primary frame is used. See [Clips](clips.md) for what isn't carried.

## The player kit

`pipeline::player_kit` walks every `idk.dat` entry in ascending id order:

- Entries with no body part are skipped.
- The entry's models are merged and recoloured. Kit parts aren't scaled.
- The entry is recorded as a kit part: `idk` (its id), `slot = body_part % 7`, `body_type = body_part / 7`, `hide_mask` (always 0) and `selectable`.

The **default body** takes, for each body part 0–6 (the first body type), the first selectable entry by id. Those parts are merged in body-part order into one mesh named `player`, with no scale, and given the stance sequences.

The cache doesn't record which sequences a player stands, turns, walks and runs with. The converter uses a fixed list, `pipeline::PLAYER_SEQUENCES`: 808, 823, 819, 820, 821, 822, 824. In order, those are stand, turn on the spot, walk, a half turn, a quarter turn each way, and run.

## Records written

| Record | NPC run | Player-kit run |
| --- | --- | --- |
| merged model | `models` `0x10000000 \| npc id` | `models` `0x30000000` |
| kit parts | none | `models` `0x20000000 \| idk id`, one per part |
| rig | `rigs` `npc id` | `rigs` `0xFFFF` |
| clips (with `--with-anims`) | `anims` `npc id << 16 \| seq id` | `anims` `0xFFFF << 16 \| seq id` |
| reports | `npc_<id>/validation.json` | `player/validation.json` |

The player-kit run also puts a `kit_parts` table in the models pack's manifest `extra`. It's keyed by each part's model record id as a decimal string:

```json
{
  "kit_parts": {
    "536870912": { "idk": 0, "slot": 0, "body_type": 0, "hide_mask": 0, "selectable": true }
  }
}
```

The rigs and clips come from `pipeline::convert`, which converts the rest model, infers the rig, and (with `--with-anims`) converts and [validates](validation.md) each sequence. The `anims` pack is written only with `--with-anims`.

## See also

- [Assembly](../client/assembly.md): how the original client merges, recolours and scales.
- [NPC](../records/npc.md) and [Identity kit](../records/idk.md): the source definitions.
- [Rig inference](rig-inference.md): bones from the creature's frames.
- [Inference](inference.md): candidate sequences for an NPC beyond the ones it names.
- [Models](../pack/models.md), [Rigs](../pack/rigs.md) and [Record ids](../pack/record-ids.md): record layouts and id namespaces.
