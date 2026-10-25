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
- The entry is recorded as a kit part: `idk` (its id), `slot = body_part % 7`, `body_type = body_part / 7`, `hide_mask` and `selectable`. `hide_mask` is always 0, because kit parts hide nothing; see [Record ids](../pack/record-ids.md#manifest-extras).

There are two **default bodies**. The male one takes, for each body part 0–6, the first selectable entry by id; the female one does the same for body parts 7–13. Each body's parts are merged in body-part order into one mesh, named `player` and `player_female`, with no scale, and given the stance sequences. Each is rigged, converted and validated on its own. A kit with no selectable female parts gets only the male body.

The cache doesn't record which sequences a player stands, turns, walks and runs with. The converter uses a fixed list, `pipeline::PLAYER_SEQUENCES`: 808, 823, 819, 820, 821, 822, 824. In order, those are stand, turn on the spot, walk, a half turn, a quarter turn each way, and run.

## Records written

| Record | NPC run | Player-kit run |
| --- | --- | --- |
| merged model | `models` `0x10000000 \| npc id` | `models` `0x30000000` (male), `0x30000001` (female) |
| kit parts | none | `models` `0x20000000 \| idk id`, one per part |
| rig | `rigs` `npc id` | `rigs` `0xFFFF` (male), `0xFFFE` (female) |
| clips (with `--with-anims`) | `anims` `npc id << 16 \| seq id` | `anims` `0xFFFF << 16 \| seq id` and `0xFFFE << 16 \| seq id` |
| reports | `npc_<id>/validation.json` | `player/validation.json`, `player_female/validation.json` |

The player-kit run also puts a `kit_parts` table in the models pack's manifest `extra`. It's keyed by each part's model record id as a decimal string:

```json
{
  "kit_parts": {
    "536870912": { "idk": 0, "slot": 0, "body_type": 0, "hide_mask": 0, "selectable": true }
  }
}
```

The rigs and clips come from `pipeline::convert`, which converts the rest model, infers the rig, and (with `--with-anims`) converts and [validates](validation.md) each sequence. The `anims` pack is written only with `--with-anims`.

## Spot animations

`rs2ool pack spotanims --id N` or `--all` ([`run::spotanims`](../../crates/rs2convert/src/run.rs)) converts spot animations the same way. [`creature::spotanim`](../../crates/rs2convert/src/creature.rs) assembles one: its model recoloured, its scale as `[scale_xy, scale_z, scale_xy]`, and its sequence. That follows the original client, which recolours the model, poses it with the sequence, then scales it.

| Record | Id |
| --- | --- |
| model | `models` `0x70000000 \| spotanim id` |
| rig | `rigs` `0x8000 \| spotanim id`, above every NPC id and below the player rigs |
| clip | `anims` `rig << 16 \| seq id` |
| report | `spotanim_<id>/validation.json` |

A spot animation with no sequence, or whose model has no vertex labels for one to move, is just a model: no rig or clip. On the test cache all 666 convert in under 3 seconds (release build), 592 with a clip and 74 without. 49 clips are flagged by validation. 33 of those scale in a way a key can't hold ([What isn't carried](clips.md#what-isnt-carried)); the rest move vertices no bone can follow, as with creatures.

The original client also turns a spot animation by its `rotation` (0, 90, 180 or 270 degrees about the vertical axis, after posing and scaling) and lights it with ambient `64 + ambient` and contrast `850 + contrast`. No record has a place for those, so the models pack's manifest lists every spot animation under `spotanims`, keyed by model record id:

```json
{
  "spotanims": {
    "1879048192": { "spotanim": 0, "rig": 32768, "clip": 2147483733, "rotation": 0, "ambient": 0, "contrast": 0 }
  }
}
```

`rig` and `clip` are null for one without a clip. One spot animation on the test cache is turned (by 270 degrees), and 261 have their own lighting.

## See also

- [Assembly](../client/assembly.md): how the original client merges, recolours and scales.
- [NPC](../records/npc.md) and [Identity kit](../records/idk.md): the source definitions.
- [Rig inference](rig-inference.md): bones from the creature's frames.
- [Inference](inference.md): candidate sequences for an NPC beyond the ones it names.
- [Models](../pack/models.md), [Rigs](../pack/rigs.md) and [Record ids](../pack/record-ids.md): record layouts and id namespaces.
