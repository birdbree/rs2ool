[Home](../README.md) › [Conversion](README.md) › Structure profiles

# Structure profiles

[Structures](structures.md) finds buildings as blueprints, and [Structure purposes](structure-purposes.md) says what each one and each of its rooms is for. A profile sums up all the structures of one class: how big they are, which rooms they have, how those rooms connect, what they're made of, which furnishings go in which rooms, and what they stand with. It's what a generator needs to build a new tavern or smithy that keeps to the rules the map's own follow. Read this page to use the profiles or to know what their figures rest on.

**Status:** implemented in [`rs2convert::profile`](../../crates/rs2convert/src/profile.rs) and written by [`rs2ool cache structures -o DIR --profiles`](../tools/cache-commands.md#structures), one JSON file per class. Profiles are built from the [learned](structure-purposes.md#learned-from-labels) classes, room uses and sites, so they're as good as those guesses: about two in three classes and room uses are right held out. Carrying profiles and blueprints in a pack record is the [next stage](structures.md#next).

## What a profile holds

A profile covers the structures the learned forests put in one class. Sizes are **spreads**: the least, the quartiles and the most, each one of the actual values (by nearest rank), so `median` is a size some structure has, and a generator can pick within the middle half for a usual one or reach out to the ends for an odd one. Counts are plain counts, and every list is commonest first.

| Field | What it is |
| --- | --- |
| `class`, `structures` | The class and how many structures are in it |
| `width`, `depth` | The spread of their boxes' sizes in tiles, east–west and north–south |
| `planes` | How many planes they have rooms on |
| `rooms`, `yards` | How many rooms they have (yards included), and how many of those are yards |
| `entrances` | How many ways lead outside: closed doors, and runs of open edges (an open doorway, an archway, a stall's open front), each run counted once |
| `uses` | Their rooms, by use (below) |
| `links` | How rooms of each use connect (below) |
| `floors` | What their rooms' floors are made of, counted in tiles |
| `walls`, `roofs` | What their walls and roof pieces are made of, counted in pieces |
| `outside` | What hangs on their outside walls, by furnishing category |
| `on_sites` | How many share a [site](structure-purposes.md#forests) with other structures |
| `site_classes` | The classes of the structures they share a site with, each pair counted once from each side |
| `site_gap` | For those on a site, the spread of the gap to the nearest other structure on it, in tiles |

### Rooms by use

Each entry in `uses` covers the class's rooms of one use:

| Field | What it is |
| --- | --- |
| `use` | The use |
| `rooms` | How many rooms have it |
| `structures` | How many structures have at least one |
| `tiles` | The spread of their sizes in tiles |
| `ground` | How many are on their structure's lowest plane |
| `roofed` | How many are roofed; the rest are yards |
| `entrances` | How many lead outside, through a closed door or an opening |
| `furnishings` | By [furnishing category](structure-purposes.md#furnishing-categories): how many of these rooms have at least one (`rooms`) and how many there are in all (`count`), most-often-present first |

So a generator can read, for a tavern's bar, that nearly every tavern has one, that it's usually on the ground floor with a way out, and that seats, tables, storage and a bar go in nearly all of them, while a hearth goes in about one in four.

### Links

Each entry in `links` counts pairs of rooms that connect, by their uses and how:

- **`door`:** a closed door between two rooms on one plane. Either way round, so `from` is the lesser use in the order the [uses](structure-purposes.md#what-a-room-is-for) are listed and `to` the other.
- **`above`:** `to` is on the plane above `from`, over some of its tiles. That's how floors stack: stairs and ladders sit in such rooms, and the map has no other record of which rooms they join.

Each pair of rooms counts once, however many doors join them. Open doors don't count: an open door walls nothing, so the rooms either side are one room, or its doorway is an opening to the outside. Nor do yards that open onto a roofed room with no wall between: those join one structure without a door.

## What it finds

On this cache's 936 structures, with the built-in forests (sizes are medians; the uses and links are the commonest few):

| Class | Structures | Size | Planes | Rooms | Ways out | Commonest rooms | Commonest links |
| --- | --- | --- | --- | --- | --- | --- | --- |
| house | 231 | 6×6 | 1 | 1 | 1 | bedroom, living room, terrace, kitchen | bedroom over living room, terrace over bedroom |
| shack | 114 | 4×4 | 1 | 1 | 1 | storeroom, empty, bedroom | (few) |
| shop | 76 | 7×7 | 2 | 2 | 2 | shop floor, terrace, storeroom, bedroom | bedroom over shop floor, terrace over shop floor |
| enclosure | 57 | 3×3 | 1 | 1 | 0 | courtyard | courtyard to courtyard by door |
| castle | 38 | 22×19 | 3 | 9 | 6 | terrace, hall, storeroom | terrace over hall, hall over hall |
| cave | 34 | 28×27 | 2 | 2 | 10 | terrace, cave, courtyard | terrace over terrace, cave over cave |
| crypt | 34 | 5×5 | 1 | 1 | 1 | other, corridor | (few) |
| ruin | 33 | 6×6 | 1 | 1 | 1 | storeroom, courtyard, bedroom | (few) |
| tavern | 30 | 11×11 | 2 | 2 | 4 | bar, dining, storeroom, bedroom | bedroom over bar, dining over bar |
| smithy | 30 | 8×6 | 1 | 1 | 1 | forge, bedroom | forge to empty room, bedroom to forge |
| tower | 27 | 5×5 | 3 | 3 | 2 | empty, terrace, stairwell | terrace over empty room, empty room over stairwell |
| hall | 26 | 12×12 | 1 | 1 | 3 | throne room, empty, dining | throne room over throne room |
| bank | 26 | 8×10 | 1 | 1 | 1 | bank hall, terrace, storeroom | terrace over bank hall |
| storehouse | 20 | 6×5 | 1 | 1 | 1 | storeroom | storeroom over storeroom |
| church | 20 | 11×12 | 2 | 2 | 3 | empty, chapel, bedroom | empty room over chapel |
| workshop | 18 | 6×7 | 1 | 1 | 1 | workshop, terrace, bedroom | terrace over workshop |
| ship | 18 | 11×14 | 2 | 2 | 4 | other, terrace, storeroom | terrace over other |
| farm | 16 | 9×4 | 1 | 1 | 1 | storeroom, kitchen, garden | kitchen to storeroom by door |
| garden | 13 | 5×6 | 1 | 1 | 2 | garden, courtyard | (none) |
| magic tower | 13 | 8×9 | 3 | 3 | 4 | terrace, bedroom, library | bedroom over library |
| barracks | 6 | 5×5 | 1 | 1 | 0 | bedroom, courtyard | bedroom to dining room by door |
| prison | 5 | 5×7 | 1 | 2 | 1 | cell, storeroom | cell to hall by door, bedroom over cell |
| library | 5 | 9×7 | 2 | 3 | 2 | library, stairwell | library over library |
| stall | 5 | 6×5 | 1 | 1 | 4 | (other) | (none) |
| mill | 4 | 8×8 | 3 | 3 | 14 | workshop, storeroom | workshop over storeroom |

37 structures are `other`. Crypt and ship rooms are mostly `other`, since no use fits a tomb chamber or a deck. A terrace is a yard above the ground: all 452 are unroofed, and all but 10 are above their structure's lowest plane. Castles, towers and caves have many, the caves because underground rooms often aren't flagged as roofed.

Walls are mostly plain-coloured or stone; castles, storehouses, farms, churches and prisons lean to stone. Floors are wood in houses, shops and towers, mud in shacks, taverns and storehouses, grass in gardens and water under ships. Of those on sites, most stand with their own class; beyond that, castles and shacks stand with each other (a castle's outbuildings), towers with shacks, halls and barracks, enclosures with farms and gardens, and storehouses with ruins and smithies.

## Limits

- **As good as the guesses.** A profile is built from the learned classes and uses, which are right about two times in three held out. A shack read as a house lands in the house profile; the commonest entries are sound, but rare ones may be mistakes.
- **Few of some classes.** Barracks, prisons, libraries, stalls and mills have four to six structures each, so their spreads are thin.
- **Stairs are inferred.** An `above` link says a room sits over another; it doesn't say where the stairs or ladder are. The room that holds them has `stairs` among its furnishings.
- **Per structure, not per site.** A castle's blocks are profiled as separate castles; `site_classes` and `site_gap` say what they stand with, not how a whole complex is laid out.

## See also

- [Structures](structures.md): how structures and rooms are found.
- [Structure purposes](structure-purposes.md): the classes, uses and sites profiles are built from.
- [Cache commands](../tools/cache-commands.md#structures): the `--profiles` option.
