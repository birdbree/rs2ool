[Home](../README.md) › [Pack format](README.md) › Items

# Items

An `items` record (kind 8) describes one item: its name, examine text and options, its inventory model and the model it's worn as for each body type, how its icon is drawn, how its stacks look, its noted form, and where it's worn. This page gives the layout, every flag and slot value, and what the tools write.

Implemented in [`contentpack::records::item`](../../crates/contentpack/src/records/item.rs). All integers are little-endian.

## Layout

```text
id              u16   item definition id
nameRef         u32   string index, or NONE
examineRef      u32   string index, or NONE
flags           u8    see below
value           u32   the item's value, as a signed 32-bit number
worldModelRef   u32   the inventory and ground model, or NONE
heldModelRef    2 × u32   male, female; always NONE (see below)
equipModelRef   2 × u32   male, female wear model, or NONE
iconZoom        u16   distance of the icon camera
iconPitch       u16   2048 per turn
iconYaw         u16   2048 per turn
iconRoll        u16   2048 per turn
iconOffsetX     i16
iconOffsetY     i16
scaleX          u16   128 = 1.0
scaleY          u16
scaleZ          u16
stackCount      u8
stacks          stackCount × { count u16, item u16 }
surfaceCount    u8
surfaces        surfaceCount × { from u16, to u16 }
wearSlot        u8    see below
hideMask        u16   see below
wearFlags       u8    see below
notedId         u16   the noted form of this item, or 0xFFFF
notedTemplate   u16   for a noted item, the template it's drawn with, or 0xFFFF
optionRefs      10 × u32   five ground options, then five inventory options; string index each, or NONE
```

`NONE` is `0xFFFFFFFF`. String indices refer to the items string table, `strings` record `0x300` ([Strings](strings.md)). Model refs are record ids in a `models` pack. There's no header; the decoder checks lengths and nothing else.

## Flags

| Bit | Value | Name | Meaning |
| --- | --- | --- | --- |
| 0 | 1 | `STACKABLE` | copies stack in one inventory slot |
| 1 | 2 | `MEMBERS` | members only |

Bits 2–7 are unused.

## Models

`worldModelRef` is the model drawn in the inventory and on the ground: the definition's inventory model, with its recolours applied but not its scale, which is in `scaleX`–`scaleZ`. `equipModelRef` holds the model worn for each body type, built the way the original client builds it: the body type's up to three wear models merged, moved by its offset and recoloured ([Equipment models](../records/obj.md#equipment-models)). `heldModelRef` is in the layout for formats that keep a separate model for an item in the hand. 317 has none, since a held item is drawn with its wear model, so the tools always write `NONE` there.

## Stacks and surfaces

Each stack variant `{count, item}` means a stack of at least `count` looks like item `item` ([Stack variants](../records/obj.md#stack-variants)). Each surface is a colour override `{from, to}`, both palette indices ([Materials](materials.md#palette)): the definition's recolours, each colour mapped to its nearest palette entry. The models already have the recolours applied; the list is there for readers that recolour themselves.

## Wear

The wear slot uses the original client's equipment slot numbers:

| Value | Slot |
| --- | --- |
| 0 | head |
| 1 | cape |
| 2 | amulet |
| 3 | weapon (right hand, or both) |
| 4 | body |
| 5 | shield (left hand) |
| 7 | legs |
| 9 | hands |
| 10 | feet |
| 255 | not worn, or not known |

The numbers it skips (6 arms, 8 hair, 11 jaw) are body parts only an identity kit fills.

`hideMask` has one bit per identity-kit body part the item hides: bit 0 hair, 1 jaw, 2 torso, 3 arms, 4 hands, 5 legs, 6 feet. Bits 7–15 are unused.

| Bit | Value | Wear flag | Meaning |
| --- | --- | --- | --- |
| 0 | 1 | `INFERRED` | the slot and hide mask were inferred, not stated by the source; a reader may override them |
| 1 | 2 | `HELD_ONLY` | the item has wear models but no option that puts it on: they're only shown while it's held during an animation |
| 2 | 4 | `BOTH_HANDS` | a weapon drawn on both hands |

317 doesn't record any of this. The tools infer the slot and hide mask from the wear models' vertex labels ([Equipment slots](../conversion/inference.md#equipment-slots)), set `INFERRED` on every item with a wear model, and leave items without one at slot 255 with no flags. The hide mask is the default body parts an item covers, a guess rather than the original game's rule: in the original client, hiding a body part is the server's decision, made outside the cache.

## What the tools write

`rs2ool pack items --id N` or `--all` converts item definitions with [`rs2convert::item`](../../crates/rs2convert/src/item.rs) and writes three packs:

| Pack | Records |
| --- | --- |
| `items` | one per definition, keyed by item id |
| `models` | `0x50000000 \| item` for each inventory model that decodes; `0x60000000 \| item << 1 \| body` for each wear model (body 0 male, 1 female) |
| `strings` | the items string table, record `0x300`: names, examine texts and options, each distinct string once, in the order items first use them |

An inventory model that won't decode leaves `worldModelRef` at `NONE`. On the test cache, all 7956 items convert in about 2.4 s (release build), with 11,124 model records. 1586 items have a wear model, and 1572 of those get a slot.

## See also

- [Obj](../records/obj.md): the source definitions, wear models and stack variants.
- [Equipment slots](../conversion/inference.md#equipment-slots): how the slot and hide mask are inferred.
- [Record ids](record-ids.md): the model id namespaces.
- [Strings](strings.md): the items string table.
- [Pack commands](../tools/pack-commands.md): `rs2ool pack items`.
