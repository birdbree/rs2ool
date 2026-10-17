[Home](../README.md) › [Conversion](README.md) › Objects

# Objects

`rs2ool pack objects` turns `loc.dat` definitions into four packs: shaped models, one `objects` record per definition, wall kits, and a string table. This page shows how each field is filled.

Implemented in [`rs2convert::object`](../../crates/rs2convert/src/object.rs). The source format is on [Location](../records/loc.md), and the layouts on [Objects](../pack/objects.md), [Kits](../pack/kits.md) and [Strings](../pack/strings.md). Pack data is little-endian.

## Morph children

A definition with a morph table can turn into other definitions. Starting from the requested ids (`--id`, or every definition with `--all`), the converter keeps pulling in morph children until there are none left it hasn't seen. Children (absent entries skipped) get their **models** converted so their states can point at them. An `objects` record is written only for the ids actually requested. With `--all` that's every definition anyway.

## Shaping

Each model is shaped as the original client builds it before placement (see [Assembly](../client/assembly.md)):

1. **Mirror**, if the definition is mirrored: z is negated and each face's first and third index swap, which keeps the winding.
2. **Scale**: `v × scale / 128` per axis, using the definition's x, y and z scale (skipped when all three are 128).
3. **Offset**: the definition's x, y and z offset is added.

The shaped model then goes through the [model conversion](models.md). Recolours are **not** baked into object models. They go into the record's surfaces instead (below).

## Models per definition

`models_of` decides which models a definition has:

- If no model has a placement type (the untyped form), all of them are merged into one model of type 10.
- Otherwise each model is kept separately with its type, and a missing type counts as 10.

Model record `n` of definition `loc` is `0x40000000 | loc << 4 | n`, so a definition can hold at most 16. More than that is an error.

## The `objects` record

| Field | Filled from |
| --- | --- |
| `id` | the definition id |
| `nameRef` | the name, through the string table (`0xFFFFFFFF` when absent) |
| `width`, `height` | the definition's size in x and y |
| `layer` | from the **first** model's type: 0–3 and 9 wall, 4–8 wall decoration, 10–11 furniture, 12–21 roof, anything else floor decoration (10 when there are no models) |
| `placementFlags` | the bits below |
| `surfaces` | each recolour as `(palette index of from, palette index of to)` |
| `models` | `(type, model record id)` for each model |
| `states` | the definition itself, then one per morph child |
| `transitions` | idle animations |
| `wear` | 0 |

Placement bits:

| Bit | Set when the definition |
| --- | --- |
| 1 | adjusts to terrain |
| 2 | obstructs the ground |
| 4 | occludes |
| 8 | casts a shadow |
| 16 | supports items (non-zero) |
| 32 | is hollow |
| 64 | is flat-shaded |
| 128 | is interactive |

### States

State 0 is the definition itself. Each morph child follows in the order its table lists it, using the child's own definition. A state holds:

- `modelRef`: the definition's first model record (`0x40000000 | loc << 4`), or `0xFFFFFFFF` if it has no models;
- `collisionMask`: 1 if solid, plus 2 if impenetrable, plus the 317 blocked-directions mask (low four bits) shifted left by 4;
- `lightRef` and `soundRef`: always `0xFFFFFFFF`;
- `optionRefs`: the five options through the string table, `0xFFFFFFFF` where an option is absent or hidden.

### Transitions

An idle animation becomes a self-transition, `from = to = state`, with `animRef` set to the sequence id and `ticks` 0. The definition's own animation uses state 0, and each child's animation uses that child's state. Nothing else produces transitions.

### Surfaces

Recolour pairs are kept as palette-index pairs, not applied to the geometry. Both sides go through the same nearest-entry lookup as faces (see [Palette](palette.md)), so a pair whose two colours share one entry comes out as two equal indices.

## Wall kits

A definition that has at least one model on the wall layer (types 0–3 and 9) also gets a `kits` record with the same id:

- `materialId`: `2000 + definition id` (truncated to 16 bits). No conversion writes a material with that id.
- `kind`: 0 (wall).
- entries: for each wall-layer model and each rotation 0–3, the key `type << 2 | rotation`, all four pointing at that model's record.

Each model is written once, and the four rotations share it.

## Strings, domain 2

Names and options go into one string table, deduplicated: the first time a string appears it gets the next index, and later uses share it. Definitions are visited in id order, and within one, its states' options come first and its name last. The table is written as `strings` record `2 << 8 | 0` (`0x200`), domain 2 and locale 0.

## Packs written

`run::objects` returns the models, objects, kits and strings packs, leaving out any that ended up empty, for example the kits pack when no definition has a wall model.

## See also

- [Location](../records/loc.md): the definition fields used here.
- [Objects](../pack/objects.md) and [Kits](../pack/kits.md): record layouts.
- [Terrain](../client/terrain.md): placement types and rotations in the original client.
- [Floors](floors.md): the other string domain.
