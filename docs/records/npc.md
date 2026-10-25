[Home](../README.md) › [Records](README.md) › Npc

# Npc

`npc.dat` holds the definitions of non-player characters: which models make up the body, how it's recoloured and scaled, and which sequences it stands and walks with. This page lists the opcodes and shows how the tools assemble a definition into one animated creature.

Decoded by [`rs2cache::config::npc`](../../crates/rs2cache/src/config/npc.rs) into `NpcDef` ([`rs2cache::types::config`](../../crates/rs2cache/src/types/config.rs)). Cache data is big-endian.

## Storage

Npc is an **indexed** type: `npc.idx` gives a length per definition and `npc.dat` holds the opcode streams. The index layout, the opcode loop and the shared encodings are on [Config archive](../cache/config.md).

```text
rs2ool cache decode npc --id 1 -o out/
```

## Opcodes

Each definition ends with opcode 0. An opcode not listed is logged, added to `unknown_opcodes`, and skipped without reading anything.

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 1 | `count u8`, then `model u16` × count | `models` |
| 2 | `string` | `name` |
| 3 | `string` | `examine` |
| 12 | `u8`, signed | `size`: footprint in tiles |
| 13 | `u16` | `stand_anim` |
| 14 | `u16` | `walk_anim` |
| 17 | `u16` × 4 | `walk_anim`, `walk_back_anim`, `walk_left_anim`, `walk_right_anim` (overwrites opcode 14) |
| 30–34 | `string` | `actions[op − 30]`; `"hidden"` (any case) is stored as none |
| 40 | `count u8`, then `{from u16, to u16}` × count | `recolours` |
| 60 | `count u8`, then `model u16` × count | `chat_head_models` |
| 90–92 | `u16` | Read and discarded |
| 93 | nothing | `draw_minimap_dot = false` |
| 95 | `u16` | `combat_level` |
| 97 | `u16` | `scale_xz`: horizontal scale (128 = 1.0) |
| 98 | `u16` | `scale_y`: vertical scale |
| 99 | nothing | `visible = true`: the original client adds these NPCs to the scene in an earlier pass than other characters |
| 100 | `u8`, signed | `ambient` |
| 101 | `u8`, signed | `contrast`, stored multiplied by 5 |
| 102 | `u16` | `head_icon` |
| 103 | `u16` | `turn_speed` |
| 106 | morph table (see [Config archive](../cache/config.md)) | `morph`: children are npc ids |
| 107 | nothing | `clickable = false` |

The sequence ids from opcodes 13, 14 and 17 are kept exactly as read; unlike the loc animation field, 65535 isn't turned into none here, as the original client doesn't either. The same goes for the models and the head icon. See [65535 as none](../cache/config.md#65535-as-none) for which `u16` references keep 65535 as an id.

## Defaults

| Field | Default |
| --- | --- |
| `size` | 1 |
| `scale_xz`, `scale_y` | 128 |
| `turn_speed` | 32 |
| `draw_minimap_dot`, `clickable` | true |
| `visible` | false |
| `ambient`, `contrast` | 0 |
| `models`, `recolours`, `chat_head_models`, `actions` | empty |
| every sequence, `name`, `examine`, `combat_level`, `head_icon`, `morph` | none |

## Post-decode derivations

The only derivation is contrast: the original client multiplies opcode 101's signed byte by 5, and the decoder does the same, so `contrast` is an `i16` in the range −640 to 635. If you're comparing against raw bytes, divide by 5 first.

## Fields

`NpcDef` holds: `id`, `models`, `name`, `examine`, `size` (`i8`), `stand_anim`, `walk_anim`, `walk_back_anim` (walking away from the facing direction), `walk_left_anim` and `walk_right_anim` (walking sideways, facing a quarter turn either way), `actions`, `recolours`, `chat_head_models`, `draw_minimap_dot`, `combat_level`, `scale_xz`, `scale_y`, `visible`, `ambient` (`i8`), `contrast` (`i16`), `head_icon`, `turn_speed`, `morph`, `clickable`, and `unknown_opcodes` (omitted from JSON when empty).

## Assembly into a creature

[`rs2convert::creature::npc`](../../crates/rs2convert/src/creature.rs) builds a creature from a definition the way the original client builds one:

1. **Models.** Every model in `models` is decoded and merged into one mesh. A single model is used as it is; with several, faces are appended in order and each face vertex is welded to the first existing vertex at the same position, keeping that vertex's label; vertices no face uses are dropped.
2. **Recolours.** Each `recolours` pair is applied in order to the merged mesh's face colours, so a later pair can recolour the result of an earlier one.
3. **Sequences.** The five sequence fields (stand, walk, walk back, walk left, walk right), in that order, plus any extra ids given with `--seq`, are resolved with duplicates dropped. Each sequence's frames are looked up in the frame index and their frame packs loaded; a frame that's in no pack is an error. `npc.dat` names no other animations, which is why `--seq` exists.
4. **Scale.** The scale is `[scale_xz, scale_y, scale_xz] / 128`. It is applied after animation, as in the original client, and is also baked into the rest mesh the converted model and rig start from.

The creature's name is the definition's `name`, or `npc N` when there isn't one. [Model assembly](../client/assembly.md) describes merging and scale-after-animation; [Creature conversion](../conversion/creatures.md) carries on from here into rigs and clips.

```text
rs2ool pack npc --id 1 --with-anims -o packs/
```

The explorer plays an npc's sequences when they resolve, and otherwise shows the merged, recoloured, scaled model ([`rs2convert::preview`](../../crates/rs2convert/src/preview.rs)).

## See also

- [Config archive](../cache/config.md): index layout, recolours, morph tables, hidden actions.
- [Model assembly](../client/assembly.md): welding, recolours, scale after animation.
- [Creature conversion](../conversion/creatures.md): from the assembled creature to model, rig and clips.
- [Seq](seq.md): the sequence definitions the anim fields point to.
- [Inference](../conversion/inference.md): finding the sequences an NPC plays beyond the ones it names.
- [Pack commands](../tools/pack-commands.md): `rs2ool pack npc` and its flags.
