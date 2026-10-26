[Home](../README.md) › [Pack format](README.md) › Structures

# Structures

A `structures` pack (kind 12) holds three kinds of record: a **blueprint** for each structure found in the map, a **site** for each group of structures that belong together, and a **profile** for each class of structure. This page gives the layouts, the id scheme, the axes and the code tables. Read it to place blueprints, generate structures from profiles, or write your own reader.

Implemented in [`contentpack::records::structure`](../../crates/contentpack/src/records/structure.rs) and written by [`rs2ool pack structures`](../tools/pack-commands.md#structures) through [`rs2convert::blueprint`](../../crates/rs2convert/src/blueprint.rs). All integers are little-endian. How structures are found is on [Structures](../conversion/structures.md); how they're classed and grouped into sites, on [Structure purposes](../conversion/structure-purposes.md); what goes into a profile, on [Structure profiles](../conversion/structure-profiles.md).

## Ids

| Id | Record |
| --- | --- |
| `plane << 28 \| x << 14 \| y` | The blueprint of the structure whose origin (the south-west corner of its box) is world tile `(x, y)` and whose lowest plane is `plane` |
| `0x40000000 \| b` | The site whose first member, by id, is blueprint `b` |
| `0x80000000 \| class` | The profile of class code `class` |

World tiles are x east and y north from the map's south-west corner, as in the cache; the blueprint id only names the structure, and nothing inside a blueprint uses these axes. Each record also starts with its own magic, so a reader can tell them apart without the id.

## Axes

Positions inside a record are tiles in pack axes ([Coordinates](README.md#coordinates)): **x east and z south**, counted from the **north-west corner** of the blueprint's box (or the site's), since pack space negates the cache's north. The box's tiles run from `x = 0` to `width − 1` and `z = 0` to `depth − 1`; a piece just outside it (a roof's overhang, an outside wall's corner post) can be at −1 or one past the end. Heights are in client units, 240 to a plane, up positive, above the blueprint's lowest tile corner.

Sides of a tile are named by compass: 0 west, 1 north, 2 east, 3 south. A piece's `rotation` is the map's: quarter turns clockwise seen from above, the turn the client applies to the location's model and footprint. A door's two rooms are west then east for a door on a north–south edge, south then north for one on an east–west edge, again by compass.

## Shared parts

```text
dressing  material     u8    material code, or 255 for nothing known
          materialRef  u32   materials record (1 + texture), or NONE when plain
          colour       u32   0xRRGGBB when plain, otherwise NONE

piece     object       u32   objects record id (the location definition id)
          category     u8    furnishing category code, or 255
          icon         u16   minimap icon (mapfunction sprite index), or 0xFFFF
          kind         u8    317 location type, 0–22
          rotation     u8    quarter turns clockwise, 0–3
          plane        u8
          x            i16   tiles east of the north-west corner
          z            i16   tiles south of it
          room         u16   the room it's in, or 0xFFFF
          dressing           what it's made of (walls, doors and roof pieces)

spread    min, low, median, high, max   5 × u16
```

`NONE` is `0xFFFFFFFF`. A piece is 25 bytes, a dressing 9, a spread 10. The `object` id is an `objects` record id ([Objects](objects.md)), so a client can place a piece as found; its `category` and `dressing` let it swap the piece for another of the same category, or another material, and keep the blueprint's structure. Objects carry no dressing (material 255); walls, doors and roof pieces carry their model's main texture or colour ([Materials](../conversion/structures.md#materials)).

## Blueprint

```text
magic        "RBLU", version u8 = 1
width        u16   box size in tiles, east–west
depth        u16   north–south
planeCount   u8
planes       planeCount × u8   planes with rooms, lowest first
class        u8    class code
site         u32   site record id, or NONE when it stands alone
flags        u8    bit 0: clipped (it reached the edge of what was searched)
roomCount    u16
rooms        roomCount × {
               plane     u8
               flags     u8    bit 0: roofed (a yard isn't)
               use       u8    use code
               floor     dressing   its commonest floor's material
               tileCount u16
               tiles     tileCount × { x u16, z u16, overlay u8, underlay u8, height i16 }
             }
doorCount    u16
doors        doorCount × { piece, open u8, roomA u16, roomB u16 }   0xFFFF = outside
openingCount u16
openings     openingCount × { room u16, plane u8, x u16, z u16, side u8 }
wallCount    u16
walls        wallCount × piece
roofCount    u16
roofs        roofCount × piece
objectCount  u16
objects      objectCount × piece
```

A tile's `overlay` and `underlay` are floor ids as the map stores them: 0 for none, otherwise `flo` id + 1, so floor `f` is material `1000 + f` in the `materials` pack the floor conversion writes ([Floors](../conversion/floors.md)). Rooms come in the structure's room order (by plane, then by south-west-most tile), and every room index in the record is checked against `roomCount`.

**Walls as edges.** A wall piece of type 0 walls one edge of its tile, the side its rotation names (0 west, 1 north, 2 east, 3 south); type 2 walls two, that side and the next one clockwise; types 1, 3 and 9 (corner posts and diagonal walls) wall none. A closed door walls its edge the same way; an open door walls nothing, and its doorway is an opening or joins the rooms either side. **Openings** are edges where a roofed room meets the outside with no wall.

**Objects** are everything else: wall decorations (types 4–8), furnishings (10 and 11) and floor decorations (22), each in its room, or in none for a decoration on an outside wall.

## Site

```text
magic        "RSIT", version u8 = 1
width        u16   size of the box round all its members, east–west
depth        u16   north–south
memberCount  u16
members      memberCount × { blueprint u32, x u16, z u16 }   by blueprint id
```

Each member's `x` and `z` place its blueprint's north-west corner from the site's. Each member's blueprint names the site in its `site` field. Sites hold two members or more; a structure on its own has no site.

## Profile

```text
magic         "RPRF", version u8 = 1
class         u8    class code
structures    u16   how many structures are in the class
sizes         6 × spread   width, depth, planes, rooms, yards, ways out
useCount      u8
uses          useCount × {
                use         u8
                rooms       u16   rooms with this use
                structures  u16   structures with at least one
                tiles       spread   their sizes in tiles
                ground      u16   how many are on the lowest plane
                roofed      u16
                entrances   u16   how many lead outside
                furnishingCount u8
                furnishings furnishingCount × { category u8, rooms u16, count u32 }
              }
linkCount     u16
links         linkCount × { from u8, to u8, via u8, count u16 }
floors        count u8, then count × { material u8, tiles u32 }
walls         count u8, then count × { material u8, pieces u32 }
roofs         count u8, then count × { material u8, pieces u32 }
outside       count u8, then count × { category u8, pieces u32 }
onSites       u16   how many share a site
siteClasses   count u8, then count × { class u8, count u32 }
hasSiteGap    u8    1 if a site gap follows
siteGap       spread   (only when hasSiteGap is 1)
```

Every list is commonest first. A link's `via` is 0 for a closed door between two rooms on one plane (with `from` the lesser use code), and 1 for `to` on the plane above `from`, over some of its tiles. The fields mean what they mean in the JSON profiles; see [Structure profiles](../conversion/structure-profiles.md#what-a-profile-holds).

## Codes

Classes, uses, furnishing categories, materials and vias are `u8` codes. The pack's manifest lists their names under `extra.names`, each list indexed by code:

```text
"names": { "classes": [...], "uses": [...], "categories": [...], "materials": [...], "vias": ["door", "above"] }
```

The codes the tools write today:

| Code | Class | Use | Category | Material |
| --- | --- | --- | --- | --- |
| 0 | `house` | `bedroom` | `bank` | `wood` |
| 1 | `shack` | `kitchen` | `bar` | `stone` |
| 2 | `tavern` | `dining` | `smithing` | `brick` |
| 3 | `smithy` | `bar` | `crafting` | `marble` |
| 4 | `shop` | `bank_hall` | `worship` | `sandstone` |
| 5 | `bank` | `shop_floor` | `throne` | `cobbles` |
| 6 | `church` | `forge` | `counter` | `plaster` |
| 7 | `castle` | `workshop` | `display` | `thatch` |
| 8 | `hall` | `chapel` | `military` | `roof_tiles` |
| 9 | `magic_tower` | `library` | `ship` | `metal` |
| 10 | `library` | `throne_room` | `mill` | `foliage` |
| 11 | `workshop` | `storeroom` | `farm` | `grass` |
| 12 | `mill` | `bathroom` | `magic` | `mud` |
| 13 | `ship` | `armoury` | `books` | `sand` |
| 14 | `prison` | `cell` | `cooking` | `snow` |
| 15 | `crypt` | `stairwell` | `washing` | `carpet` |
| 16 | `ruin` | `hall` | `bed` | `water` |
| 17 | `farm` | `courtyard` | `seat` | `lava` |
| 18 | `garden` | `terrace` | `table` | `picture` |
| 19 | `town_hall` | `garden` | `storage` | `invisible` |
| 20 | `cave` | `pen` | `hearth` | `plain` |
| 21 | `tower` | `empty` | `light` | |
| 22 | `enclosure` | `living_room` | `decoration` | |
| 23 | `stall` | `study` | `plant` | |
| 24 | `barracks` | `office` | `grave` | |
| 25 | `storehouse` | `corridor` | `prison` | |
| 26 | `other` | `cave` | `stairs` | |
| 27 | `unknown` | `other` | `remains` | |
| 28 | | `unknown` | `fixture` | |
| 29 | | | `nature` | |

Read the names from the manifest rather than this table: they're what the pack was written with.

## Invariants

- Blueprint room indices (in pieces, doors and openings) are below `roomCount`, or `0xFFFF` where outside or none is allowed.
- Site members are in blueprint id order.
- No record has bytes after its last field.

## What the tools write

`rs2ool pack structures` writes every structure in the map, classed and grouped by the built-in learned trees: on this cache 936 blueprints, 75 sites and 26 profiles, 1037 records in about 1.3 MB. The pack is the same, byte for byte, every run.

## See also

- [Structures](../conversion/structures.md): how structures, rooms, doors and openings are found, and their materials.
- [Structure purposes](../conversion/structure-purposes.md): classes, room uses and sites.
- [Structure profiles](../conversion/structure-profiles.md): what each profile field means, with this cache's figures.
- [Objects](objects.md): the records a piece's `object` id names.
- [Record ids](record-ids.md): every id namespace.
