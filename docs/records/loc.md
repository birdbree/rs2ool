[Home](../README.md) › [Records](README.md) › Loc

# Loc: world objects

`loc.dat` holds the definitions of world objects: everything a map places on a tile, from walls to furniture to floor decorations. This page gives the full opcode stream, the derived flags the original client computes after decoding, and how the tools turn a definition into geometry and packs.

Decoded by [`rs2cache::config::loc`](../../crates/rs2cache/src/config/loc.rs) into `LocDef` ([`rs2cache::types::config`](../../crates/rs2cache/src/types/config.rs)). Cache data is big-endian.

## Storage

Loc is an **indexed** type: `loc.idx` gives a length per definition, so any definition can be found without decoding the ones before it. `loc.idx` and `loc.dat` must agree on the count, or decoding fails. The index layout, the opcode loop and the shared encodings (recolours, morph tables, `"hidden"` actions, 65535 meaning none) are on [Config archive](../cache/config.md).

```text
rs2ool cache decode loc --id 1 -o out/
```

## Opcodes

Each definition is an opcode stream ending with opcode 0. An opcode not listed here is logged, added to `unknown_opcodes`, and skipped without reading anything.

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 1 | `count u8`, then `{model u16, type u8}` × count | Models, each for one placement type. Replaces any earlier model list. |
| 2 | `string` | `name` |
| 3 | `string` | `examine` |
| 5 | `count u8`, then `model u16` × count | Models with no type (drawn as type 10). Replaces any earlier model list. |
| 14 | `u8` | `size_x` in tiles |
| 15 | `u8` | `size_y` in tiles |
| 17 | nothing | `solid = false` |
| 18 | nothing | `impenetrable = false` |
| 19 | `u8` | Interactive flag: `has_actions = (value == 1)` |
| 21 | nothing | `adjust_to_terrain = true` |
| 22 | nothing | `flat_shading = true` |
| 23 | nothing | `occludes = true` |
| 24 | `u16` | `animation`, a sequence id; 65535 means none |
| 28 | `u8` | `decor_displacement` |
| 29 | `u8`, signed | `ambient` |
| 30–34 | `string` | `actions[op − 30]`; `"hidden"` (any case) is stored as none |
| 35–38 | `string` | Read and recorded as unknown. The original client's action range runs to 38 but it has five slots, so these fail in its own loader; real caches don't use them |
| 39 | `u8`, signed | `contrast` (stored as read, not multiplied) |
| 40 | `count u8`, then `{from u16, to u16}` × count | `recolours` |
| 60 | `u16` | `minimap_function` |
| 62 | nothing | `mirrored = true` |
| 64 | nothing | `casts_shadow = false` |
| 65 | `u16` | `scale_x` (128 = 1.0) |
| 66 | `u16` | `scale_y` |
| 67 | `u16` | `scale_z` |
| 68 | `u16` | `mapscene` |
| 69 | `u8` | `blocked_directions`, a bit mask |
| 70 | `i16` | `offset_x` |
| 71 | `i16` | `offset_y` |
| 72 | `i16` | `offset_z` |
| 73 | nothing | `obstructs_ground = true` |
| 74 | nothing | `hollow = true` |
| 75 | `u8` | `support_items` |
| 77 | morph table (see [Config archive](../cache/config.md)) | `morph` |

Only the animation (opcode 24) and the morph table turn 65535 into none; the models, minimap function and mapscene keep it as an id, as the original client does. See [65535 as none](../cache/config.md#65535-as-none).

## Defaults

Before the first opcode the decoder starts from the original client's values:

| Field | Default |
| --- | --- |
| `size_x`, `size_y` | 1 |
| `solid`, `impenetrable`, `casts_shadow` | true |
| `decor_displacement` | 16 |
| `scale_x`, `scale_y`, `scale_z` | 128 |
| `ambient`, `contrast`, `blocked_directions`, offsets | 0 |
| every other flag | false |
| `models`, `recolours`, `actions` | empty |
| `name`, `examine`, `animation`, `minimap_function`, `mapscene`, `morph` | none |

## Post-decode derivations

Three fields aren't final until the stream ends. The decoder applies them in the same order as the original client:

1. **`has_actions`.** If opcode 19 was present, its value decides (`1` is true, anything else false). Otherwise the object is interactive when its first model is untyped (opcode 5) or has type 10, **or** when any of opcodes 30–34 appeared. A `"hidden"` action still counts here, even though it's stored as none. A definition with no models and no actions is not interactive.
2. **`hollow`.** If opcode 74 was present, `solid` and `impenetrable` are both cleared, whatever opcodes 17 and 18 said.
3. **`support_items`.** If opcode 75 was present, its value is kept. Otherwise it is 1 when the object is still `solid` after step 2, else 0. So a hollow object never supports items by default.

The order matters: `support_items` reads `solid` after `hollow` has cleared it. Swap the last two steps and every hollow object quietly starts holding items.

## Fields

`LocDef` holds: `id`, `models` (a list of `LocModel { model, kind }`, where `kind` is `None` for opcode 5 models), `name`, `examine`, `size_x`, `size_y`, `solid`, `impenetrable`, `has_actions`, `adjust_to_terrain`, `flat_shading`, `occludes`, `animation`, `decor_displacement`, `ambient` (`i8`), `contrast` (`i8`), `actions` (five optional strings), `recolours`, `minimap_function`, `mirrored`, `casts_shadow`, `scale_x/y/z` (`u16`), `mapscene`, `blocked_directions`, `offset_x/y/z` (`i16`), `obstructs_ground`, `hollow`, `support_items`, `morph`, and `unknown_opcodes` (omitted from JSON when empty).

## Placement types and layers

A map places a location with a placement type from 0 to 22 (see [Map](map.md) and [Terrain](../client/terrain.md)). Each type falls into one layer class, as [`rs2convert::object::layer_of`](../../crates/rs2convert/src/object.rs) maps it:

| Types | Layer |
| --- | --- |
| 0–3, 9 | wall |
| 4–8 | wall decoration |
| 10, 11 | furniture |
| 12–21 | roof |
| 22 | floor decoration |

The model drawn for a placement is picked like this ([`rs2convert::preview::location_model`](../../crates/rs2convert/src/preview.rs)): type 11 is treated as type 10 (it's a rotated type 10 in the original client), a typed definition draws the model listed for that type, and an untyped definition draws only for types 10 and 11, using all its models merged into one.

## Shaping

Before placement, each model is shaped as the original client shapes it ([`rs2convert::object::shaped`](../../crates/rs2convert/src/object.rs)):

1. If `mirrored`, negate every vertex's z and swap each face's first and third vertex so the winding stays outward.
2. Scale each axis by `scale / 128` (integer, truncating), skipped when all three are 128.
3. Add `offset_x`, `offset_y`, `offset_z`.

Untyped models are merged (with vertex welding) before shaping, into one type-10 model. Recolours are applied afterwards in the previews. [Model assembly](../client/assembly.md) covers merging, mirroring and placement rotation in full.

## Morph table

When `morph` is set, the location shows one of `children` (loc ids, each possibly none) chosen by the value of a varbit or, failing that, a varp. See [Varp and varbit](varp-varbit.md) for those and [Config archive](../cache/config.md) for the encoding. Conversion doesn't evaluate player variables; object conversion collects every child instead, as described next. The explorer can evaluate one: see [Object states](../tools/explorer.md#object-states).

## Conversion

`rs2ool pack objects --id N` (or `--all`) converts definitions with [`rs2convert::object`](../../crates/rs2convert/src/object.rs). In outline:

- Each shaped model becomes a model record `0x4000_0000 | loc << 4 | index`, so a definition may have at most 16 models (more is an error).
- Each definition becomes an object record. Its layer comes from the first model's type (10 if none). Its states are the definition itself followed by each morph child in order; each state's collision is built from `solid`, `impenetrable` and the low four bits of `blocked_directions`, and a state with an `animation` gets a looping transition.
- The placement flags carry `adjust_to_terrain`, `obstructs_ground`, `occludes`, `casts_shadow`, `support_items != 0`, `hollow`, `flat_shading` and `has_actions`.
- Wall-layer models also produce a kit record with four rotation variants each.
- Names and actions go into a deduplicated string table.

Morph children are decoded and their models converted along with the requested ids, but only the requested ids get object records. The details and record layouts are on [Object conversion](../conversion/objects.md), [Objects pack records](../pack/objects.md) and [Kits](../pack/kits.md).

## See also

- [Config archive](../cache/config.md): index layout, opcode loop, shared encodings.
- [Model assembly](../client/assembly.md): merging, mirroring, scale and offset, placement rotation.
- [Object conversion](../conversion/objects.md): states, transitions, collision and kits in detail.
- [Map](map.md): where placement types and rotations come from.
- [Seq](seq.md): the sequences `animation` refers to.
- [Inference](../conversion/inference.md): other sequences a location's model could play.
