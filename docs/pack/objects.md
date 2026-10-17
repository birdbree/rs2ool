[Home](../README.md) › [Pack format](README.md) › Objects

# Objects

An `objects` record (kind 5) describes one placeable world object: its footprint, layer and placement flags, colour overrides, a model per placement type, a list of states with their collision and options, and transitions between states. This page gives the layout, every flag value, and what the tools write.

Implemented in [`contentpack::records::object`](../../crates/contentpack/src/records/object.rs). All integers are little-endian.

## Layout

```text
id               u16   location definition id
nameRef          u32   string index, or NONE
width            u8    footprint, in tiles
height           u8    footprint, in tiles
layer            u8    see below
placementFlags   u16   see below
surfaceCount     u8
surfaces         surfaceCount × { from u16, to u16 }
modelCount       u8
models           modelCount × { type u8, modelRef u32 }
stateCount       u16
states           stateCount × {
                   modelRef       u32   model record id, or NONE
                   collisionMask  u16   see below
                   lightRef       u32   NONE
                   soundRef       u32   NONE
                   optionRefs     5 × u32   string index each, or NONE
                 }
transitionCount  u16
transitions      transitionCount × { from u16, to u16, animRef u32, ticks u16 }
wear             u8    must be 0
```

`NONE` is `0xFFFFFFFF`. String indices refer to the objects string table, `strings` record `0x200` ([Strings](strings.md)). Model refs are record ids in a `models` pack.

## Layers

| Value | Name | Location types |
| --- | --- | --- |
| 0 | `WALL` | 0–3, 9 |
| 1 | `WALL_DECORATION` | 4–8 |
| 2 | `FURNITURE` | 10, 11 |
| 3 | `ROOF` | 12–21 |
| 4 | `FLOOR_DECORATION` | 22 (and any other value) |

The tools take the layer from the location type of the object's first model, treating an object with no models as type 10 (`FURNITURE`). See [Location](../records/loc.md) for the 23 location types.

## Placement flags

| Bit | Value | Name | Set when the location definition has |
| --- | --- | --- | --- |
| 0 | 1 | `ADJUST_TO_TERRAIN` | its adjust-to-terrain flag |
| 1 | 2 | `OBSTRUCTS_GROUND` | its obstructs-ground flag |
| 2 | 4 | `OCCLUDES` | its occludes flag |
| 3 | 8 | `CASTS_SHADOW` | its casts-shadow flag |
| 4 | 16 | `SUPPORTS_ITEMS` | a non-zero support-items value |
| 5 | 32 | `HOLLOW` | its hollow flag |
| 6 | 64 | `FLAT_SHADED` | flat shading |
| 7 | 128 | `INTERACTIVE` | its has-actions flag |

Bits 8–15 are unused.

## Collision mask

| Bits | Value | Meaning |
| --- | --- | --- |
| 0 | 1 | `SOLID` |
| 1 | 2 | `IMPENETRABLE` |
| 2–3 | | Unused (0) |
| 4–7 | `blocked << 4` | The low four bits of the definition's blocked-directions mask |
| 8–15 | | Unused (0) |

## Surfaces and models

Each surface is a colour override `{from, to}`, both palette indices ([Materials](materials.md#palette)): the location definition's recolours, each colour mapped to its nearest palette entry.

Each model entry pairs a 317 location type (0–22) with the model record for that type. When none of the definition's models has a type, the tools merge them all into one model with type 10. Model records for locations are `0x40000000 | loc << 4 | index`, so a definition can have at most 16 ([Record ids](record-ids.md#models)).

## States and transitions

State 0 is the definition itself. When the definition has a morph table, each child it names follows as states 1, 2, …, in table order, with empty slots skipped. A state's fields come from its own definition:

- `modelRef` is that definition's first model record, `0x40000000 | id << 4`, or `NONE` when it has no models.
- `collisionMask` is built from its solid, impenetrable and blocked-directions fields.
- `optionRefs` are its five action strings, `NONE` for an empty slot.
- `lightRef` and `soundRef` are always `NONE`.

A transition `{from, to}` names two state indices. The tools write one transition per state whose definition has an animation: `from` and `to` are both that state, `animRef` is that animation's [sequence](../records/seq.md) id, and `ticks` is 0.

It's tempting to look `animRef` up in an `anims` pack. It isn't there: it's a cache sequence id, not a clip id.

## Invariants

- The encoder fails if `surfaceCount` or `modelCount` exceeds 255, or `stateCount` or `transitionCount` exceeds 65535.
- The encoder always writes `wear` as 0, and the decoder rejects any other value as `Invalid` ("wear data is not supported").
- The decoder rejects a record that ends early or has bytes left over.

Nothing else is checked: the decoder accepts any layer, flags, state indices in transitions, and references.

## Ids

Object record `loc` is location definition `loc`, and the `id` field repeats it as a `u16`. `rs2ool pack objects` writes object records for the definitions you ask for; model records for the morph children their states use are written too, but those children get no object record of their own unless you asked for them.

## See also

- [Objects conversion](../conversion/objects.md): how location definitions become objects, models, kits and strings.
- [Location](../records/loc.md): the cache's location definitions.
- [Kits](kits.md): the wall kits written alongside objects.
- [Strings](strings.md): the table that `nameRef` and `optionRefs` index.
- [Record ids](record-ids.md): model, object and string ids.
