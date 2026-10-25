[Home](../README.md) › [Conversion](README.md) › Structures

# Structures

The map stores ground tiles and placed objects, but it never says where a building is, what it's for, or which room is the kitchen. This page covers how whole structures are found in the map, with their rooms, doors, walls, roofs and everything inside, and how they're written out as blueprints. Read it before using structure data, to know what's found from cache facts and where the method stops.

**Status:** segmentation is implemented in [`rs2convert::structure`](../../crates/rs2convert/src/structure.rs) and run by [`rs2ool cache structures`](../tools/cache-commands.md#structures), which writes JSON. What each structure and room is likely for is guessed by hand-written rules and by forests learned from labelled structures, which also group structures into sites, on [Structure purposes](structure-purposes.md). What its walls, roofs and floors are made of comes from their textures, names and colours ([Materials](#materials)). What the structures of each class are like, taken together, is on [Structure profiles](structure-profiles.md). A pack record for blueprints and profiles is the next stage ([Next](#next)).

## What a blueprint is for

A blueprint is meant to be used in several ways: placed whole, used to lay out new towns, remixed with other materials and furnishings while keeping its structure, or reduced to rules per class of structure so new ones can be generated. That shapes what's kept:

- **Structure apart from dressing.** Walls, doors and roofs are kept as tile edges and tiles, so the shape survives a change of materials. Each placed object keeps the id it was found with and its [furnishing category](structure-purposes.md#furnishing-categories) (a bed, a counter, storage), so one can be swapped for another, and walls, roofs and floors say what they're [made of](#materials).
- **Rooms and how they connect.** Each room is a set of tiles, and each door says which rooms it joins, so the room graph is there for both remixing and generating.
- **Relative positions.** Everything is placed relative to the structure's south-west corner.

## Finding rooms

### The roof flag

Tile flag 4 marks a tile under a roof. The original client reads it to decide when to hide the planes above: when the camera is over such a tile, it draws nothing above the current plane, which is how the roof comes off as you go indoors. So map makers set it on every tile inside a building, including upper floors, and it outlines interiors far better than walls alone would: irregular footprints, porches and cut-off corners included. See [Terrain](../client/terrain.md#tile-flags).

### Walls on tile edges

A wall stands on a tile's edge, and which edge depends on its placement type and rotation, the same edges the original client blocks movement across:

| Type | What it is | Edges, by rotation 0 / 1 / 2 / 3 |
| --- | --- | --- |
| 0 | straight wall | west / north / east / south |
| 2 | corner wall | west and north / north and east / east and south / south and west |
| 1, 3 | corner post | none (it only blocks moving diagonally past a corner) |
| 9 | diagonal wall | none (it stands across the tile) |

An edge is named by the tile south or west of it, so a wall on one tile's east edge and a wall on the next tile's west edge are the same edge.

### Rooms

A room is a patch of roofed tiles on one plane that reach each other without crossing a walled edge: a flood fill from each roofed tile not yet in a room, moving north, south, east and west. Every roofed tile ends up in exactly one room.

### Doors

A wall is a door when its definition is named a door or a gate, or offers to open or close it. Doors split rooms like any wall, but join them into one structure.

The map places some doors already open. An open door is a different definition (it offers to close and not to open) standing a quarter turn from its doorway, so the edge it's on isn't a wall at all. Open doors wall nothing: their doorway is an unwalled edge, so it shows as an [opening](#openings) or, between two rooms, joins them into one.

### Yards

A yard is a room open to the sky: unroofed tiles on one plane, walled in on every side, found by the same flood fill. A courtyard, a walled garden, a pen or a field, a roof terrace. Each room says whether it's `roofed`. A yard has to be:

- **walled in all round** within the regions searched: a patch that reaches the edge of what was searched, or the edge of the map, is open country;
- **between 4 and 4096 tiles** (`YARD_MIN`, `YARD_MAX`): smaller walled-in patches are the gaps between walls, and bigger ones are stretches of land fenced off, or a walled town;
- **floored, above the ground:** on planes 1–3, only tiles with an overlay or an underlay count; the rest is open air;
- **on a building, above the ground:** walled-in floor on an upper plane that joins no building is attic floor or the tops of walls, so it's dropped. On the ground, a yard with no building is a structure of its own: a pen, a walled garden, a field.

Not every unroofed room is open to the sky. Underground buildings often have no roof flag, since nothing needs to come off as you go in, so their rooms come out as yards: walled in, with doors and furniture, but `roofed: false`.

## Joining rooms into structures

Rooms join into one structure when:

1. **a door lies between them;**
2. **they overlap on neighbouring planes:** any tile of one is under any tile of the other, so the floors of a building stack up, and a roof terrace sits on its building;
3. **a roofed room meets a yard with no wall between them:** a porch onto a courtyard, an open-fronted shed in a yard;
4. **a roofed room has no door, nothing above or below it and no yard outside,** in which case it joins the roofed room it shares the most walled edges with: a closet or alcove belongs to the room beside it.

Two buildings that share a wall without a door stay apart, and so do a building and a yard that only share a wall: a fence along a house isn't a way in.

### Across region boundaries

Everything works in world tiles, not region tiles: edges, rooms, footprints and roofs all look across region boundaries, and a wall placed in one region walls the edge it shares with the next. So a building that straddles a boundary comes out whole, as long as both regions are searched. On this cache, 28 of the 936 structures cross a region boundary. Each comes out the same from `--region` as from `--all`, yards included: a yard is only kept when it closes within what was searched, and the neighbours cover any yard that starts in the region and is under 64 tiles across.

A structure belongs to the region its south-west corner is in, so it's reported once. `--region` searches the region and its eight neighbours, which covers any building that starts in the region and is under 64 tiles across; one that reaches the edge of what was searched is marked `clipped`.

## What belongs to a structure

| What | How it's placed | Belongs when |
| --- | --- | --- |
| Walls and doors (types 0 and 2) | on edges | a room or yard of the structure is on either side of one of its edges |
| Corner posts and diagonal walls (types 1, 3, 9) | on a tile | its tile or a neighbouring tile is in a room |
| Roof pieces (types 12–21) | on a tile, usually a plane above the rooms | a room is on that tile or a neighbour, on its plane or the one below |
| Furnishings (types 10, 11) | on a tile, with a footprint | any tile of its footprint is in a room; it's in that room |
| Floor decorations (type 22) | on a tile | its tile is in a room |
| Wall decorations (types 4–8) | on a tile | its tile is in a room; or, for a straight wall decoration (4, 5) outside, the wall it hangs on has a room behind it, and it's an outside decoration of that room's structure with no room of its own |

A wall between two structures belongs to both, so each blueprint has all its walls.

## Openings

An opening is an edge where a roofed room meets the outside with no wall: an open doorway, an archway, the open front of a stall or a passage. Where it meets a yard instead, the two join (see above), so that edge isn't an opening. Yards have none: they're walled in all round, and where an upper-floor yard's floor stops, that's a drop, not a way in. Doors that lead outside and openings together are the ways in.

A cut-off corner is a diagonal wall with a corner post beside it, and the roofed tiles there have no straight wall on their outer edges. Tiles with a corner post or a diagonal wall therefore have no openings.

## Materials

What a structure is made of is the other half of its dressing, so a building can be rebuilt in other materials: stone walls in wood, a wooden floor in marble. [`material::dress`](../../crates/rs2convert/src/material.rs) fills it in after the structures are found, since it needs the models: `rs2ool cache structures` does that before writing.

- **Walls, doors and roof pieces** are made of the model their definition draws for their placement type. When textured faces cover at least half its area, it's made of the texture covering the most; otherwise it's `plain`, with the area-weighted colour of its faces.
- **A room's floor** is made of its commonest floor (each tile's overlay, else its underlay): the floor's texture if it has one, else what its name says, else `plain`, with its colour. An overlay of the colour the original client draws as nothing is `invisible`.

Each is a `material`, with the `texture` it's drawn with or, when plain, its `colour` (`0xRRGGBB`).

### Textures

What each of the 50 textures is made of was read off the textures by eye, then checked against the names of the floors that use them: every textured floor's name agrees with its texture's reading (water, marble, lava, wooden floors, stone tiles, pebbles), and three first readings were corrected that way. Textures that show a thing rather than a surface (a painting, a bookshelf) are `picture`; those that are no material (a cobweb, stars, plain blue) have none.

| Texture | Shows | Material |
| --- | --- | --- |
| 0 | dark wooden planks with a handle | wood |
| 1 | rippling blue water | water |
| 2 | grey stone bricks laid upright | stone |
| 3 | brown floorboards | wood |
| 4 | an ornate panel | wood |
| 5 | dark wood | wood |
| 6 | red roof tiles | roof tiles |
| 7 | dark slats | wood |
| 8 | green leaves | foliage |
| 9 | the rings of a cut log | wood |
| 10 | plain blue | — |
| 11 | rough grey-brown stone | stone |
| 12 | a dark lattice | metal |
| 13 | a landscape painting | picture |
| 14 | a portrait | picture |
| 15 | marble | marble |
| 16 | wooden planks | wood |
| 17 | falling drops of water | water |
| 18 | straw | thatch |
| 19 | dark shingles | roof tiles |
| 20 | a bookshelf | picture |
| 21 | brown thatch | thatch |
| 22 | reddish planks | wood |
| 23 | mossy stone bricks laid upright | stone |
| 24 | rippling blue water | water |
| 25 | green water | water |
| 26 | a cobweb | — |
| 27 | brown shingles | roof tiles |
| 28 | dark moss | foliage |
| 29 | a fringe of grass | grass |
| 30 | a fern | foliage |
| 31 | lava | lava |
| 32 | brown earth | mud |
| 33 | autumn leaves | foliage |
| 34 | stars | — |
| 35 | sandstone blocks | sandstone |
| 36 | a carved wooden panel | wood |
| 37 | metal mesh | metal |
| 38 | sand | sand |
| 39 | a portrait | picture |
| 40 | lava | lava |
| 41 | leaves | foliage |
| 42 | plain blue | — |
| 43 | grey stone slabs | stone |
| 44 | grey stone bricks | stone |
| 45 | a wooden panel | wood |
| 46 | cobblestones | cobbles |
| 47 | pale stone bricks laid upright | stone |
| 48 | sandstone carved with glyphs | sandstone |
| 49 | parchment with writing | plaster |

### Floor names

A floor without a texture is made of what its name says. Floor names run words together ("woodenfloor"), so these words match anywhere in the name, ignoring case, tried in order:

| Material | Words |
| --- | --- |
| invisible | invisible |
| water | water, swamp |
| lava | lava |
| marble | marble |
| brick | brick |
| carpet | carpet, rug |
| wood | wood, plank |
| thatch | thatch, straw |
| roof tiles | roof |
| mud | mud, dirt, earth |
| sand | sand, desert |
| snow | snow, ice |
| grass | grass |
| cobbles | road, cobble, path |
| stone | stone, rock, cliff, mountain, tile, slate, granite |

### What they're made of

Of 60,486 straight and corner walls in structures, 59,252 have a material: 39,830 plain, 16,985 stone, 946 wood, 944 sandstone, 503 metal and a few plaster and cobbles. Plain walls are the colour-only walls many towns are built of; their colour is kept. By its commonest wall material, a structure is plain (482), stone (315), sandstone (28), wood (15), metal (9) or plaster (3), and 83 have no straight walls. Roof pieces are plain (20,522), stone (17,228; many roof pieces are gable walls), roof tiles (1,846), wood (1,569), thatch (1,386) and plaster (85).

Rooms' floors are plain (632), wood (485), mud (456), stone (253), invisible (202), sand (157), cobbles (132), roof tiles (88; the floors of roof terraces), marble (46), sandstone (42), water (34), grass (19) and snow (1); 57 rooms have no floor at all. In region `50_50`, the castle, the shops, the house and the church all have stone walls; the church's floor is wood and most of the castle's rooms are paved with cobbles.

## The blueprint

[`structure::find`](../../crates/rs2convert/src/structure.rs) takes decoded regions and the definitions placed in them and returns the structures, sorted by origin (x, then y) and lowest plane. Every list in a structure is sorted, so the result doesn't depend on the order the regions come in. Positions are in tiles relative to the origin, x east and y north; heights are in client units (240 to a plane).

| Field | What it holds |
| --- | --- |
| `origin` | the south-west corner of the box around its rooms, in world tiles (a region's tiles start at `region x · 64`, `region y · 64`) |
| `size` | that box in tiles, east and north |
| `planes` | the planes it has rooms on, lowest first |
| `rooms` | each room's plane, whether it's `roofed` (false for a [yard](#yards)), what its `floor` is made of ([Materials](#materials)), and its tiles; a tile has its position, its overlay and underlay floor (0 for none, else the `flo` id + 1, as the map stores them) and its south-west corner's height above the structure's lowest corner |
| `doors` | each door's placement, whether the map places it open, and the rooms either side of its edge (west then east, or south then north; none for outside) |
| `openings` | each opening's room, plane, tile and side (0 west, 1 north, 2 east, 3 south) |
| `walls` | walls, corner posts and diagonal walls, doors not included |
| `roofs` | roof pieces |
| `objects` | furnishings, floor decorations and wall decorations, each with its room (none for an outside wall decoration) |
| `clipped` | whether a room reaches the edge of the regions searched, so the structure may go on past them |

Each placement has the definition id, its name where it has one, its [furnishing category](structure-purposes.md#furnishing-categories), its [minimap icon](structure-purposes.md#minimap-icons) if its definition draws one, for walls, doors and roof pieces what it's made of (its `material`, [Materials](#materials)), the placement type, the rotation, the plane, its position (negative just outside the origin: a roof's overhang, a post at an outside corner) and its room. The command adds each structure's `purpose` beside these fields; see [Structure purposes](structure-purposes.md#in-the-output).

## Plans and sheets

`rs2ool cache structures -o DIR --plans` also draws each structure's floor plan and writes a text sheet beside it, as `DIR/plans/RX_RY/X_Y_P.png` and `.md`, named by the structure's id: its origin and lowest plane. They're for looking at structures, and for labelling them by hand or otherwise.

**The plan** ([`cache::plans`](../../src/cache/plans.rs)) is drawn from above, 16 pixels to a tile, north up, one panel per plane from the lowest, each with its plane number in the corner and a margin of one tile:

- roofed rooms are tinted a pale shade of their floor's material, yards pale grey, with faint lines between tiles so sizes can be counted;
- walls are drawn on their edges in the colour of their material (a plain wall in its own colour), diagonal walls across their tiles, and corner posts as dots at their corners;
- closed doors are thick green lines, open doors pale green, and openings thin red ones;
- each furnishing or floor decoration with a category is a square in its category's colour (a wall decoration a smaller one), and each minimap icon a yellow diamond;
- each room's number is drawn in its middle tile.

| Category | Colour |
| --- | --- |
| bank | `#ffd700` |
| bar | `#b8860b` |
| smithing | `#404040` |
| crafting | `#8b4513` |
| worship | `#9370db` |
| throne | `#ff00ff` |
| counter | `#ff8c00` |
| display | `#ffa07a` |
| military | `#b22222` |
| ship | `#1e90ff` |
| mill | `#deb887` |
| farm | `#9acd32` |
| magic | `#00ced1` |
| books | `#800000` |
| cooking | `#ff4500` |
| washing | `#87ceeb` |
| bed | `#4169e1` |
| seat | `#a0522d` |
| table | `#d2691e` |
| storage | `#808000` |
| hearth | `#dc143c` |
| light | `#ffff66` |
| decoration | `#ee82ee` |
| plant | `#228b22` |
| grave | `#696969` |
| prison | `#2f4f4f` |
| fixture | `#c0c0c0` |
| nature | `#556b2f` |
| stairs | `#000080` |
| remains | `#f5f5dc` |

**The sheet** gives the structure's size, planes, rooms, doors and openings; its walls' and roof pieces' materials, commonest first; its current guess with the evidence; each room's plane, whether it's roofed, its size, floor and guessed use, and what it contains (named objects with their categories, minimap icons with what they show, unnamed decorations counted); what hangs on its outside walls; and up to 12 neighbouring structures within 24 tiles, nearest first, each with its id, size, planes, gap, direction and guess. The neighbours are there so a labeller can say which belong to the same site; they're the pairs the [site forest](structure-purposes.md#forests) decides.

## What it finds

On the whole map (239 regions with structures), it finds 936 structures in about four seconds in a release build; most of that is the flood fill over the open country, which only proves it isn't a yard.

| Measure | Count |
| --- | --- |
| Rooms | 2604: 1852 roofed and 752 yards |
| Yards by plane | 243 on the ground, 319 on plane 1, 125 on plane 2, 65 on plane 3 |
| Buildings with yards | 220 (courtyards, gardens, roof terraces) |
| Yards on their own | 122 (pens, walled gardens and fields, unflagged rooms underground) |
| Structures with a single roofed room and nothing else | 408 |
| Structures on more than one plane | 359 |
| Doors | 2725, gates included: 143 placed open, 508 closed doors to the outside |
| Structures with no door | 441, most of them open-sided or entered by an opening |
| Structures with no way in at all | 148 (no closed door to the outside and no opening), many of them yards whose gate is placed open |
| Placed objects | 111,802, of which 19,620 have names |

Most objects without names are floor and wall decorations: of the 111,802, 53,060 are floor decorations and 16,925 wall decorations.

Region `50_50` shows what the structures look like. Its eight structures, with their named contents:

| Origin | Size | Planes | Rooms | Doors | Openings | Commonest named objects |
| --- | --- | --- | --- | --- | --- | --- |
| 3203, 3206 | 14 × 26 | 0–2 | 12, 2 of them yards | 12 | 32 | hanging tapestries and banners, candles, chairs, suits of armour, statues, a throne |
| 3208, 3242 | 7 × 10 | 0 | 1 | 1 (open) | 1 | shelves, stools, counters, crates, barrels |
| 3222, 3252 | 8 × 6 | 0 | 1 | 0 | 4 | workbenches, blacksmith's tools, furnaces |
| 3226, 3236 | 7 × 6 | 0 | 2 | 2 | 3 | beds, tables, a bookcase, a cooking pot |
| 3227, 3201 | 7 × 8 | 0 | 2 | 2 | 1 | shelves, a counter, an axe cabinet; a bed and a bookcase in the back room |
| 3227, 3212 | 4 × 14 | 0–2 | 3, 1 of them a yard | 2 | 12 | barrels, banners, ladders, crates, javelins |
| 3238, 3204 | 10 × 12 | 0 | 1 | 3 | 0 | church pews, candles, an altar, an organ |
| 3243, 3244 | 6 × 5 | 0 | 1 | 1 | 0 | standing torches, a spit roast, boxes |

The contents already say a lot: a castle block, a general shop, a smithy, a house, a shop with living quarters behind it, a castle tower, a church and a cookhouse. [Structure purposes](structure-purposes.md) says so. The two castle blocks each have a walled roof terrace on plane 2, and the main block a walled yard on plane 1.

## Limits

- **Open grounds don't join.** Blocks joined only by open ground stay separate structures: the first and sixth structures above are blocks of one castle, but the courtyard between them opens straight onto the grounds, which aren't walled in, so it isn't a yard. The learned [sites](structure-purposes.md#forests) put them together.
- **Broken walls don't close.** A ruin whose walls have gaps leaks into the open country, so it isn't found yet; nor is any enclosure bigger than `YARD_MAX`.
- **Open doors and gates merge rooms.** With the door open, nothing walls the doorway, so the rooms either side flood into one; an open gate leaves a yard unclosed.
- **Diagonal walls don't split rooms.** A tile with a diagonal wall belongs wholly to the room it's roofed with, and a yard's diagonal wall doesn't close it.
- **Large roofed areas.** Some caves and underground areas are roofed throughout, so whole regions of them come out as single structures with many openings. Most lie far north of the surface (tile y above 6400), but not all.
- **What the cache doesn't hold.** NPCs and items on the ground are placed by the server, not the map, so shopkeepers, bankers and a shop's stock can't be seen. Parts of a building that lie in an underground region (a cellar reached by a ladder) aren't joined to it.
- **Plain materials.** Most walls are colour only, so they're `plain` with a colour, not stone or plaster; and a wall's model may mix materials, of which only the main one is kept.
- **Region edges.** With `--region`, the region and its eight neighbours are searched; a structure that goes on past them is marked `clipped`. With `--all`, only structures at the edge of the map are.

## Next

Segmentation, furnishing categories, minimap icons, materials, rules, labels and learned forests are done, sites group structures into complexes ([Structure purposes](structure-purposes.md)), and each class has a profile ([Structure profiles](structure-profiles.md)). What's left is **a pack record** for blueprints and class profiles, laid out with the client project.

## See also

- [Terrain](../client/terrain.md#tile-flags): tile flags, including the roof flag.
- [Map regions](../records/map.md): the tile and location files.
- [Loc](../records/loc.md#placement-types-and-layers): placement types and their layers.
- [Inference](inference.md): the other links the tools infer, with evidence.
