[Home](../README.md) › [Records](README.md) › Obj

# Obj: items

`obj.dat` holds item definitions: the inventory model and how it's posed for an icon, the models worn on each body type, options, recolours and stack-size variants. This page lists the opcodes and explains how the equipment model is built.

Decoded by [`rs2cache::config::obj`](../../crates/rs2cache/src/config/obj.rs) into `ObjDef` ([`rs2cache::types::config`](../../crates/rs2cache/src/types/config.rs)). Cache data is big-endian.

## Storage

Obj is an **indexed** type: `obj.idx` gives a length per definition and `obj.dat` holds the opcode streams. See [Config archive](../cache/config.md) for the index layout and the shared encodings.

```text
rs2ool cache decode obj --id 1 -o out/
```

## Opcodes

Each definition ends with opcode 0. An opcode not listed is logged, added to `unknown_opcodes`, and skipped without reading anything.

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 1 | `u16` | `inventory_model` |
| 2 | `string` | `name` |
| 3 | `string` | `examine` |
| 4 | `u16` | `icon_zoom` |
| 5 | `u16` | `icon_pitch` |
| 6 | `u16` | `icon_yaw` |
| 7 | `i16` | `icon_offset_x` |
| 8 | `i16` | `icon_offset_y` |
| 10 | `u16` | Read and discarded |
| 11 | nothing | `stackable = true` |
| 12 | `i32` | `value` |
| 16 | nothing | `members = true` |
| 23 | `u16`, then `u8` signed | `male_equip1`, `male_offset` |
| 24 | `u16` | `male_equip2` |
| 25 | `u16`, then `u8` signed | `female_equip1`, `female_offset` |
| 26 | `u16` | `female_equip2` |
| 30–34 | `string` | `ground_actions[op − 30]`; `"hidden"` (any case) is stored as none |
| 35–39 | `string` | `inventory_actions[op − 35]`, kept as written, `"hidden"` included |
| 40 | `count u8`, then `{from u16, to u16}` × count | `recolours` |
| 78 | `u16` | `male_equip3` |
| 79 | `u16` | `female_equip3` |
| 90 | `u16` | `male_chat_head1` |
| 91 | `u16` | `female_chat_head1` |
| 92 | `u16` | `male_chat_head2` |
| 93 | `u16` | `female_chat_head2` |
| 95 | `u16` | `icon_roll` |
| 97 | `u16` | `noted_id`, an obj id |
| 98 | `u16` | `noted_template`, an obj id |
| 100–109 | `item u16`, `count u16` | Stack variant in slot `op − 100` |
| 110 | `u16` | `scale_x` (128 = 1.0) |
| 111 | `u16` | `scale_y` |
| 112 | `u16` | `scale_z` |
| 113 | `u8`, signed | `ambient` |
| 114 | `u8`, signed | `contrast`, stored multiplied by 5 |
| 115 | `u8` | `team` |

Ground actions and inventory actions treat `"hidden"` differently. That's not a typo in the table: the decoder only maps it to none for the ground options.

## Defaults

| Field | Default |
| --- | --- |
| `inventory_model` | 0 |
| `icon_zoom` | 2000 |
| `value` | 1 |
| `scale_x`, `scale_y`, `scale_z` | 128 |
| icon angles and offsets, `male_offset`, `female_offset`, `ambient`, `contrast`, `team` | 0 |
| `stackable`, `members` | false |
| equipment and chat-head models, `noted_id`, `noted_template`, `name`, `examine` | none |
| actions, `recolours`, `stack_variants` | empty |

## Post-decode derivations

As in the original client, opcode 114's signed byte is multiplied by 5, so `contrast` is an `i16` from −640 to 635. Nothing else is derived.

## Fields

`ObjDef` holds: `id`, `inventory_model`, `name`, `examine`, `icon_zoom`, `icon_pitch`, `icon_yaw`, `icon_roll`, `icon_offset_x`, `icon_offset_y` (`i16`), `stackable`, `value` (`i32`), `members`, `male_equip1/2/3`, `male_offset` (`i8`), `female_equip1/2/3`, `female_offset` (`i8`), `male_chat_head1/2`, `female_chat_head1/2`, `ground_actions`, `inventory_actions`, `recolours`, `noted_id`, `noted_template`, `stack_variants`, `scale_x/y/z`, `ambient` (`i8`), `contrast` (`i16`), `team`, and `unknown_opcodes` (omitted from JSON when empty).

## Stack variants

Opcodes 100–109 fill ten slots. Each `StackVariant` is `{ slot, item, count }`: slot `op − 100`, an obj id and a stack count. If a slot appears twice, the later one replaces it, and `stack_variants` is always sorted by slot, whatever order the opcodes came in. Slots that never appear are missing from the list. The inspector links each variant's `item` to its obj definition.

## Equipment models

[`rs2convert::creature::equip_model`](../../crates/rs2convert/src/creature.rs) builds the model a player wears, for one body type (male or female), as the original client does:

1. Take that body type's three equip models.
2. With no first model, there's no equipment model at all.
3. With a first and a second, use those two plus the third if it is set.
4. With a first but no second, use only the first. A third model without a second is ignored.
5. Merge the chosen models (vertex welding, as for npcs), then add the body type's offset (`male_offset` or `female_offset`) to every vertex's y.
6. Apply `recolours` in order.

The explorer's kit view uses this model for any slot holding an item, in place of the identity-kit part. The definition doesn't say which slot that is; [Inference](../conversion/inference.md#equipment-slots) covers how to work it out from the wear models. See [Model assembly](../client/assembly.md) for equipment models and [Idk](idk.md) for the body parts they replace.

The plain preview of an obj, in the explorer, is `inventory_model` with the recolours applied and scaled by `[scale_x, scale_y, scale_z] / 128` ([`rs2convert::preview`](../../crates/rs2convert/src/preview.rs)).

## Conversion

`rs2ool pack items` converts definitions with [`rs2convert::item`](../../crates/rs2convert/src/item.rs) into `items` records: names, examine texts and options into a string table; the inventory model (recoloured) and each body type's equipment model as model records; the icon camera, scale, stack variants, recolours as palette overrides, noted ids, flags and value. The definition doesn't say which slot an item is worn in, so the record carries the slot and hide mask inferred from its wear models, flagged as inferred. See [Items](../pack/items.md).

## See also

- [Config archive](../cache/config.md): index layout, recolours, hidden actions.
- [Model assembly](../client/assembly.md): merging and equipment models.
- [Idk](idk.md): the body parts equipment models sit on.
- [Model](model.md): the model records the obj fields refer to.
- [Explorer](../tools/explorer.md): the kit view that dresses a body with items.
- [Items](../pack/items.md): the record items convert to.
- [Inference](../conversion/inference.md#equipment-slots): an item's equipment slot from its wear models.
