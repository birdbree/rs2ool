[Home](../README.md) › [Pack format](README.md) › Record ids

# Record ids

Every record in a pack is keyed by a `u32` id. This page lists every id namespace the tools use, kind by kind, along with the reserved ids and the manifest `extra` tables that map cache ids to pack ids. Material ids, which records use to refer to materials, are here too.

The constants live in [`rs2convert::pipeline::ids`](../../crates/rs2convert/src/pipeline.rs), [`rs2convert::object`](../../crates/rs2convert/src/object.rs), [`rs2convert::item`](../../crates/rs2convert/src/item.rs), [`rs2convert::sprite`](../../crates/rs2convert/src/sprite.rs), [`rs2convert::floor`](../../crates/rs2convert/src/floor.rs), [`rs2convert::material_id`](../../crates/rs2convert/src/lib.rs), and the record modules [`material`](../../crates/contentpack/src/records/material.rs) and [`texture`](../../crates/contentpack/src/records/texture.rs).

Ids only need to be unique within one pack. Several namespaces share a kind, so they're spaced out by their top bits.

## models

| Id | Record |
| --- | --- |
| `m` (below `0x10000000`) | Cache model `m` (index 1 file id), converted on its own |
| `0x10000000 \| n` | NPC `n`'s merged model |
| `0x20000000 \| k` | Identity-kit part `k`'s merged model |
| `0x30000000` | The default player body's merged model |
| `0x40000000 \| loc << 4 \| i` | Model `i` (0–15) of location definition `loc` |
| `0x50000000 \| item` | Item `item`'s inventory model |
| `0x60000000 \| item << 1 \| body` | Item `item`'s wear model for body type `body` (0 male, 1 female) |

A location definition can list at most 16 models after untyped ones are merged; the conversion fails on one with more. See [Models](models.md) for the layout.

## rigs and anims

| Kind | Id | Record |
| --- | --- | --- |
| `rigs` | `n` | NPC `n`'s rig: the rig id is the NPC id |
| `rigs` | `0xFFFF` | The player rig |
| `anims` | `rig << 16 \| seq` | Sequence `seq` converted for rig `rig` |

The clip id packs the rig id into the top 16 bits, so it's only unique while rig ids stay below `0x10000`. A clip also names its rig in its own `rig` field. See [Rigs](rigs.md) and [Anims](anims.md).

## materials

A `materials` record's id is its material id, except for the palette:

| Id | Record |
| --- | --- |
| `0xFFFFFFFF` | The palette (`count` RGB entries), not a material |
| `0` | The palette-mode material every untextured model face uses |
| `1 + t` | Texture `t`'s material (textures are 0–49, so ids 1–50) |
| `1000 + f` | Floor `f`'s material (`flo.dat` entry `f`) |

Material ids are `u16`. A model face names its material through a `u8` slot, so only materials below 256 (the palette material and the texture materials) can appear on model faces. Floor materials are referred to by id from the `floors` table below.

[Kit records](kits.md) name material 0, the palette material, since their models' faces carry their own colours.

## textures

| Id | Record |
| --- | --- |
| `t` | Texture `t`'s rectangle in an atlas (0–49) |
| `0x80000000 \| a` | Atlas image `a`; the tools write one atlas, so one record at `0x80000000` |

## sprites

| Id | Record |
| --- | --- |
| `s << 8 \| i` | Sub-sprite `i` of sprite `s` |
| `0x01000000 \| f << 8 \| g` | Glyph `g` (0–255) of font `f` |
| `0x02000000 \| f` | Font `f`'s metrics |

`s` is an `rs2cache` sprite id: the position of the sprite's entry in the list of media-archive sprite entries followed by title-archive sprite entries, fonts left out (see [Sprite](../records/sprite.md)). `f` is the font id, 0–3, in the order `p11_full`, `p12_full`, `b12_full`, `q8_full` ([Font](../records/font.md)). The sub-index `i` is the sprite's position in its sheet and is also stored in the record. See [Sprites](sprites.md).

## objects, items, kits and strings

| Kind | Id | Record |
| --- | --- | --- |
| `objects` | `loc` | Location definition `loc` |
| `items` | `item` | Item definition `item` |
| `kits` | `loc` | The wall kit of location definition `loc` (only for definitions with wall-layer models) |
| `strings` | `domain << 8 \| locale` | One string table |

String-table domains and locales:

| Domain | Contents | Record id (locale 0) |
| --- | --- | --- |
| 1 | Floor names | `0x100` |
| 2 | Object names and options | `0x200` |
| 3 | Item names, examine texts and options | `0x300` |

Locale 0 is English and is the only locale the tools write. Object records refer to strings by their index in the domain 2 table, and item records by their index in the domain 3 table; see [Objects](objects.md), [Items](items.md) and [Strings](strings.md).

## Manifest extras

Two packs carry a table in the manifest's `extra` object ([Container](container.md#manifest)). JSON object keys are decimal strings.

`floors`, in the `materials` pack the floor conversion writes, maps each `flo.dat` id to its material and its name:

```text
"floors": { "<flo id>": { "material": <material id>, "name": <index into strings 0x100> } }
```

Every floor gets a name entry, an empty string when the definition has no name, so `name` is the floor's position in the floor table.

`kit_parts`, in the `models` pack the player-kit conversion writes, describes each identity-kit part model:

```text
"kit_parts": { "<model record id>": { "idk": <kit id>, "slot": <0–6>, "body_type": <n>, "hide_mask": 0, "selectable": <bool> } }
```

The key is the model record id (`0x20000000 | idk`, written in decimal). `slot` is the kit's body part modulo 7 and `body_type` is the body part divided by 7. `hide_mask` is always 0. `selectable` is copied from the kit definition ([Identity kit](../records/idk.md)).

## See also

- [Container](container.md): where ids sit in the index.
- [Conversion](../conversion/README.md): which cache records each id comes from.
- [Creatures](../conversion/creatures.md): NPC and player-kit models, rigs and clips.
- [Objects conversion](../conversion/objects.md): location models, objects, kits and strings.
- [Textures and sprites](../conversion/textures-and-sprites.md): the atlas, sprites and fonts.
- [Floors](../conversion/floors.md): floor materials and the `floors` table.
