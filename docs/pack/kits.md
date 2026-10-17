[Home](../README.md) › [Pack format](README.md) › Kits

# Kits

A `kits` record (kind 6) is a table of model variants under one material: each entry maps a `u16` key to a model record. This page gives the layout, the kit kinds, and the key scheme the tools use for wall kits.

Implemented in [`contentpack::records::object`](../../crates/contentpack/src/records/object.rs) (the kit record shares a module with objects). All integers are little-endian.

## Layout

```text
materialId   u16   material id
kind         u8    0 = wall, 1 = roof, 2 = floor
count        u16
entries      count × { key u16, modelRef u32 }
```

| Value | Kind |
| --- | --- |
| 0 | `WALL` |
| 1 | `ROOF` |
| 2 | `FLOOR` |

`modelRef` is a model record id in a `models` pack. Entries are stored in the order they were given; the format doesn't require them sorted by key, and keys may repeat.

The decoder rejects a record that ends early or has bytes left over, and checks nothing else. The encoder writes `count` as the low 16 bits of the entry count without checking it, so keep a kit under 65536 entries.

## What the tools write

`rs2ool pack objects` writes only wall kits. For each location definition it converts that has at least one wall-layer model (location types 0–3 and 9, see [Objects](objects.md#layers)):

- The record id is the location id.
- `materialId` is 0, the palette material ([Record ids](record-ids.md#materials)). The wall models' faces already carry their own palette indices and texture slots, so that's the material they use.
- `kind` is 0 (`WALL`).
- Each wall-layer model contributes four entries, one per rotation `r` from 0 to 3, all naming the same model record:

```text
key = type << 2 | r        type: the model's location type, r: rotation 0–3
modelRef = 0x40000000 | loc << 4 | index
```

Entries come in model order, then rotation order. Definitions with no wall-layer models get no kit record. Roof and floor kits are defined kinds that the tools never write.

Four keys per model, differing only in rotation, look redundant, and in the tools' output they are: nothing is rotated when they're filled, and all four name the same unrotated model record.

## See also

- [Objects](objects.md): the object records written alongside kits, and the layer table.
- [Objects conversion](../conversion/objects.md): how location definitions are converted.
- [Record ids](record-ids.md): the kit, model and material id schemes.
- [Models](models.md): the records `modelRef` names.
- [Location](../records/loc.md): the cache's location types.
