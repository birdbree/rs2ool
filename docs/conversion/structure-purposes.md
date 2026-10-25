[Home](../README.md) › [Conversion](README.md) › Structure purposes

# Structure purposes

[Structures](structures.md) finds buildings and their rooms; this page covers how the tools guess what each one is for: a house, a smithy, a shop and what it sells, a church, a castle, and each room's use, from a bedroom to a forge. Read it before relying on those guesses, to know what each rests on and where it goes wrong.

**Status:** implemented in [`rs2convert::furnish`](../../crates/rs2convert/src/furnish.rs) (furnishing categories and minimap icons), [`rs2convert::purpose`](../../crates/rs2convert/src/purpose.rs) (hand-written rules) and [`rs2convert::learn`](../../crates/rs2convert/src/learn.rs) (forests learned from labelled structures, which also group structures into sites), and written by [`rs2ool cache structures`](../tools/cache-commands.md#structures). The rules come with their evidence and the forests do better held out ([Learned from labels](#learned-from-labels)); none of it is a cache fact, since the map never says what a building is.

## Furnishing categories

Each placed object gets a category from its definition's name. Categories do two jobs: they're the evidence for what a room is for, and they let a blueprint keep its structure apart from its dressing, since one bed can stand in for another.

A name falls in the first category in the table below one of whose keywords it contains as whole words, ignoring case and punctuation other than apostrophes. Order matters: "smashed chair" is remains before "chair" makes it a seat, "cooking shelves" is cooking before "shelves" makes it storage, and a barrel isn't a bar, since "bar" has to be a whole word. The keywords are plain words, never proper names. A name that matches nothing has no category.

| Category | Keywords, in the order tried |
| --- | --- |
| remains | corpse, skeleton, skull, skulls, bones, smashed, broken, web, cocoon, rubble, bricks, charred, remains, boarded |
| bank | bank booth, bank table, bank chest, deposit box, bank notice, bank |
| bar | bar pumps, barstool, bar stool, barrel tap, keg, bar |
| smithing | anvil, furnace, blacksmith's tools, blacksmiths' tools, forge, bellows |
| crafting | spinning wheel, potter's wheel, pottery oven, kiln, loom, tanning, vat, churn, workbench, work bench, clothes equipment, dye, paint pots, paintcans |
| worship | altar, pew, organ, font, shrine, pyre |
| throne | throne |
| counter | counter, stall |
| display | clothes model, clothing shelves, weapon rack, sword cabinet, bow cabinet, axe cabinet, display |
| military | cannon, cannonballs, multicannon, standard, flagpole, target, javelin, armour, dummy, spear wall, defence |
| ship | ship's wheel, anchor, mast, gangplank, figurehead, jolly roger, hull, net, rope, winch, crane |
| mill | hopper, millstones, flour |
| farm | hay, haystack, trough, coop, hutch, plough, scarecrow, eggs, onion, potato, cabbage |
| magic | magical, glyphs, crystal ball, portal, pillar of light, magic |
| books | bookcase, bookshelf, bookshelves, study desk, scrolls, chart, telescope, globe |
| cooking | range, oven, cooking, pot, pots, spit roast, hanging meat, cauldron, kitchen, stove |
| washing | sink, bath, washbasin, mangle |
| bed | bed, hammock, bunk |
| seat | chair, stool, bench, chaise, seat |
| table | table, desk |
| storage | crate, crates, box, boxes, bucket, broom, barrel, barrels, sack, sacks, chest, drawers, cupboard, cuboard, cabinet, shelves, shelf, wardrobe, dresser, rack, basket, locker, urns, hat stand |
| hearth | fireplace, fire |
| light | candles, candle, candlestick, candelabrum, lamp, lantern, torch, chandelier |
| decoration | painting, portrait, landscape, statue, vase, plaque, mirror, clock, head, stuffed, banner, tapestry, aquarium, bird cage, screen, rug, piano, fountain |
| plant | potted, plant, fern, flower, flowers, flowerbed, rosebush, sunflower, sunflowers, daisies, hedge, bush, vine |
| grave | grave, gravestone, coffin, tomb, headstone |
| prison | cage, stocks, chains |
| fixture | pillar, column, plinth, pedestal, support, scaffold, pipe, pipes |
| nature | tree, rock, rocks, rockslide, stalagmite, stalagmites, roots, fungus, fungi, nettles, log pile, mushrooms |
| stairs | ladder, staircase, stairs, stepladder, trapdoor, climbing rope |

Of the 18,193 named furnishings (placement types 10 and 11) in structures on this cache, 17,725 (97.4%) get a category. The rest are one-offs: machinery, cave features, odd props.

## Minimap icons

A location definition can name a minimap icon (opcode 60): an index into the `mapfunction` sprites, drawn on the minimap over the place. Icons are placed by nameless definitions of their own, one per icon, usually as a floor decoration inside the building they mark, so they come out as objects of the structure, each placement with its `icon`. 54 of the 62 icons in use appear in structures.

What each icon shows was read off the sprites by eye, and what it suggests follows from the picture: a dollar sign for a bank, a beer mug for a pub, a sword for a sword shop. Where the picture doesn't say enough (a coloured star, an exclamation mark), the icon suggests nothing.

| Icon | Shows | Suggests |
| --- | --- | --- |
| 0 | a pot | a shop: general |
| 1 | a sword | a shop: swords |
| 2 | a flame | a shop: magic |
| 3 | an axe | a shop: axes |
| 4 | a helmet | a shop: helmets |
| 5 | a dollar sign | a bank |
| 6 | a blue star | — |
| 7 | an amulet | a shop: amulets |
| 8 | a pickaxe | mining |
| 9 | a furnace | a smithy |
| 10 | an anvil | a smithy |
| 11 | a training dummy on a frame | combat training |
| 12 | an exclamation mark | — |
| 13 | a staff | a shop: staffs |
| 14 | a platebody | a shop: platebodies |
| 15 | platelegs | a shop: platelegs |
| 16 | a scimitar | a shop: scimitars |
| 17 | a hammer | — |
| 18 | a shield | a shop: shields |
| 19 | an altar | worship |
| 20 | herbs | a shop: herbs |
| 21 | a ring | a shop: jewellery |
| 22 | a gem | a shop: gems |
| 23 | a chisel | a shop: crafting |
| 24 | a candle | a shop: candles |
| 25 | a fishing rod | a shop: fishing |
| 26 | a fish | fishing |
| 27 | a green shirt | a shop: clothes |
| 28 | a potion | a shop: potions |
| 29 | a bolt of cloth | a shop: cloth |
| 30 | a kebab | a shop: kebabs |
| 31 | a beer mug | a pub |
| 32 | a mace | a shop: maces |
| 33 | a leather top | a shop: leather |
| 34 | a tree | — |
| 35 | a spinning wheel | spinning |
| 36 | a loaf | a shop: food |
| 37 | a fork and knife | a shop: cookery |
| 38 | a red star | — |
| 39 | a water drop | water |
| 40 | a frying pan | cooking |
| 41 | a skirt | a shop: skirts |
| 42 | a clay bowl | pottery |
| 43 | a yellow cross | — |
| 44 | a golden pickaxe | a shop: mining |
| 45 | chainmail | a shop: chainmail |
| 46 | a lump of ore | a shop: silver |
| 47 | a pelt | a shop: furs |
| 48 | a bowl of spice | a shop: spices |
| 49 | a running figure | agility |
| 50 | an apple | a shop: fruit |
| 51 | a skull | — |
| 52 | scissors | hairdresser |
| 53 | a spade | — |
| 54 | a face | makeover |
| 55 | a question mark | guide |
| 56 | an arrow | transport |
| 57 | a pink star | — |
| 58 | a clay pot | pottery |
| 59 | a chequered flag | — |
| 60 | a barrel | — |
| 61 | a butter churn | dairy |
| 62 | a flask | — |
| 63 | a paw print | a shop: pets |
| 64 | a purple orb | — |
| 65 | a hide | tannery |
| 66 | the letter i | information |
| 67 | a house | houses |
| 68 | a saw blade | sawmill |
| 69 | a hammer and stone | stonemason |

## What a structure is

Each class scores points from its evidence. The best class needs at least 3 points (`MIN_SCORE`); below that a structure is `unknown`. Classes are tried in the order below, and on a tie the earlier one wins: the order runs from the most telling evidence to the least. Classes other than the best that reach 3 points come along as alternatives, best first.

| Class | Points |
| --- | --- |
| bank | 5 per bank furnishing (up to 20); 10 for the bank icon |
| church | 3 per altar, pew, organ or font (up to 15); 6 for the altar icon |
| tavern | 4 per bar, pump or bar stool (up to 16); 10 for the beer-mug icon |
| smithy | 4 per anvil, furnace or smith's tools (up to 16); 8 for the furnace or anvil icon |
| shop | 4 per counter or stall (up to 12); 2 per display of goods (up to 8); 10 for a shop icon, which also says what it sells (its `detail`) |
| castle | 4 for two or more floors with six or more roofed rooms; 3 for 300 or more roofed tiles; 6 for a throne; 1 per arms or defence (up to 6); 1 for a yard on a building of two floors or more |
| hall | 5 for 120 or more roofed tiles with eight or more seats and a table |
| magic tower | 3 for three or more floors; 3 for being round (a quarter or more of its walls diagonal); 3 per magic object (up to 9); 1 per bookcase or study desk (up to 3). It needs both magic and the tower (three floors, or round), or it can't reach 3 points |
| library | four or more bookcases or study desks: a point each, up to 10 |
| workshop | 3 per craft's tool (up to 12); 6 for the spinning-wheel or pottery icons |
| mill | 5 per hopper or millstones (up to 15) |
| ship | 2 per ship's fitting (up to 12) |
| prison | 3 per cage, stocks or chains (up to 12) |
| crypt | 2 per grave or coffin (up to 12) |
| ruin | 2 per remains or broken furniture (up to 10); 3 for no roofed room, when there are remains |
| farm | 2 per hay, trough, coop or crop (up to 10); 2 for no roofed room, when there are any |
| garden | no roofed room and three or more plants: a point a plant, up to 8 |
| house | 2 per bed (up to 6); 1 per cooking (up to 3); 1 for a table; 1 for one to six roofed rooms in 150 tiles or fewer, with a bed |
| shack | 4 for one roofed room of 20 tiles or fewer, no yard, one floor and at most six furnishings |

Each piece of evidence is written down as a line, such as `2 × bank furniture` or `minimap icon: a dollar sign`, so a guess can be checked and argued with.

## What a room is for

A room's use comes from its own objects, the same way: each use scores points, and the best one with at least 3 points wins, ties going to the earlier use below. A yard is always at least a courtyard (on the ground) or a terrace (above it). A room with no use is `empty` when it has no furnishings and `unknown` when it has some.

| Use | Points |
| --- | --- |
| bank hall | 5 per bank furnishing |
| bar | 4 per bar, pump or bar stool |
| forge | 4 per anvil, furnace or smith's tools |
| chapel | 3 per altar, pew, organ or font |
| throne room | 6 per throne |
| shop floor | 4 per counter or stall and 2 per display of goods |
| workshop | 3 per craft's tool |
| bedroom | 3 per bed |
| kitchen | 3 per cooking |
| library | 2 per bookcase or study desk, with two or more |
| cell | 3 per cage, stocks or chains |
| armoury | 1 per arms or defence, with two or more |
| dining | a table and three or more seats: 2, plus a point a seat up to 6 |
| bathroom | 3 per sink or bath, with no cooking |
| storeroom | at least three storage pieces making half or more of the furnishings: a point each, up to 8 |
| garden | a yard with three or more plants: a point a plant, up to 8 |
| pen | a yard: 2 per hay, trough, coop or crop |
| courtyard, terrace | a yard on the ground, or above it: 2 |
| stairwell | ladders or stairs and nothing else, in 16 tiles or fewer: 4 |
| hall | a roofed room of 40 or more tiles with at most one furnishing per 8 tiles: 3 |

## Learned from labels

The rules were set by hand against a few regions. Every structure on the map was then labelled from its [plan and sheet](structures.md#plans-and-sheets): what it is, what each room is for, and which of its neighbours belong to the same site. Forests of decision trees ([`rs2convert::tree`](../../crates/rs2convert/src/tree.rs)) learned from those labels do better than the rules, and they group structures into sites, which the rules can't.

### The labels

A labels file is JSON: `{"labels": […]}` (other fields alongside are ignored) or just the list. Each label has the structure's `id` (`X_Y_P`, its origin and lowest plane), its `class`, a `detail` (what a shop sells, a workshop's craft), a `use` for each room by its number (`rooms: [{"room": 0, "use": "kitchen"}, …]`), the ids of the neighbours on the same `site`, and a `confidence` (`high`, `medium` or `low`; a low one counts half). Labels stay outside the repository; only what's learned from them is kept.

Labels can name more classes and uses than the rules give. Classes the rules never give: `town_hall`, `cave` (an underground or natural place), `tower` (one that's neither a magic tower nor part of a castle), `enclosure` (a walled or fenced open area with little built in it: a pen, a compound, an arena), `stall` (a stall, a tent, a booth), `barracks` (sleeping quarters for many), `storehouse` and `other`. Uses the rules never give: `living_room`, `study`, `office`, `corridor`, `cave` and `other`.

The labels on this cache cover all 936 structures. A first pass left 173 as `other`; a second pass over those, with `tower`, `enclosure`, `stall`, `barracks` and `storehouse` added, left 45, mostly a lattice of placeholder rooms. The classes came out as house 219, shack 105, shop 76, other 45, enclosure 45, cave 38, castle 38, ruin 35, hall 34, crypt 32, tavern, tower and smithy 29 each, bank 26, garden 22, church and storehouse 20 each, workshop 19, ship 18, farm 15, magic tower 14, barracks 7, prison and stall 6 each, library 5 and mill 4; 286 structures are on a site with others.

### Features

[`rs2convert::learn`](../../crates/rs2convert/src/learn.rs) describes each structure, room and pair of neighbours as numbers:

- **A structure:** how many objects of each [furnishing category](#furnishing-categories) it has; how many minimap icons suggest a shop, a bank, a pub, a smithy, worship, cooking, a craft, water or a service; its planes, lowest plane, roofed rooms, yards, upper-floor yards, roofed and yard tiles, width, depth, aspect, roundness (the share of its walls that are diagonal), doors, doors to the outside, openings, walls, roof pieces, furnishings and unnamed objects; whether it lies in the map's underground rows (tile y 6400 and up); the shares of its walls in stone, wood, plain, sandstone and metal, and of its rooms' floors in wood, stone, mud, cobbles, plain, invisible, sand and marble; and every rule class's points ([What a structure is](#what-a-structure-is)).
- **A room:** its own furnishing category counts; its tiles, whether it's roofed, its plane, furnishings, unnamed objects, icons and its share of the structure's tiles; and every rule use's points.
- **A pair of neighbours** (each structure's 12 nearest within 24 tiles, the neighbours its sheet lists): the gap between them, the smaller and larger area and their ratio, the fewer and more planes, whether both have upper-floor yards, whether their commonest wall material and floor material agree, the share of wall definitions they have in common, whether the rules put them in the same class, the smaller of their castle points, and how far their boxes overlap east–west and north–south. The features don't depend on which comes first.

The rules' own points being features means a tree can follow a rule where it's right and overrule it where the labels disagree.

### Forests

A single tree learned the classes worse than the rules at small depths (49% held out at depth 8, against the rules' 50%) and only a little better when deep (60% at depth 16), since 26 classes need many splits and the labels are noisy where classes blur. A forest does much better: 50 trees, each grown on a resample of the labels (as many as there are, with repeats), each split trying a random square root's worth of the features, up to 16 deep with leaves of any weight; the trees vote. The resampling and the feature picks come from a fixed pseudo-random sequence seeded by each tree's number, so a forest grows the same every time.

Scores are held out by ten-fold cross-validation: structures are split into ten groups by their place in the labels, and each group is guessed by a forest grown on the other nine. A room is held out with its structure, and a pair with the first of its two structures in the order they're found.

| What | Forests | Rules |
| --- | --- | --- |
| Classes | 638 of 936 (68%) | 472 (50%) |
| Room uses | 1797 of 2604 (69%) | 1550 (60%) |
| Sites | of 3019 neighbouring pairs, 403 labelled together; 297 guessed together, 231 of them rightly (precision 78%, recall 57%) | — |

The commonest class mistakes are between neighbours in meaning: a shack taken for a house (22), a hall for a house (12), a ruin for a shack (11), something unclassifiable for a shack (9), a garden for an enclosure (8). Smaller forests are smaller to keep but do worse: 30 trees up to 12 deep with leaves of at least 2 get 600 of the classes (64%) in about a third of the space.

A site is a group of neighbours the site forest joins: every pair it guesses together is joined, and a site is what the joins connect. On the whole map, 244 structures fall into 75 sites with others: 42 pairs, 15 threes, 10 fours, a few of five to seven, and two large groups underground: a grid of 25 identical tomb chambers in region `43_149`, and a tomb's corridors with the grid of repeated blocks beside them in `55_151` (16). The two blocks of region `50_50`'s castle are one site.

### The built-in forests

`rs2ool cache structures --all --learn LABELS --save FILE` learns the forests from a labels file, prints the held-out scores and the commonest mistakes, and saves them as compact JSON. The tools come with the forests learned on this cache's labels ([`learn::BUILT_IN`](../../crates/rs2convert/src/learn.rs), from `crates/rs2convert/data/structure-trees.json`, about 650 KB); `--trees FILE` uses saved ones instead. To update the built-in forests, learn and save them, check the scores, and copy the saved file over the data file. The file holds thresholds and labels, nothing from the cache itself. Features are numbered in the order above, so a saved forest only fits the code that wrote it.

## In the output

Each structure in the `rs2ool cache structures -o` files has a `purpose`, the rules' guess: its `class` (`what`, a shop's `detail`, `score` and `evidence`), its `alternatives`, and a guess for each of its `rooms`, in room order. Beside it, `learned` has the forests' `class`, a use for each of its `rooms`, and its `site`, the id of the site's first structure, when it's on a site with others. Every placement has its `category` and its `icon`. The listing starts each line with the forests' class, with a shop's detail from the rules when they agree, and the rules' guess beside it when they don't; it ends with the site:

```text
50_50  shop (axes)                   at 3227,3201  7×8  plane 0  2 rooms  2 doors (1 to outside)  1 open edges  46 objects (28 unnamed): Shelves ×5, …
50_50  shack (rules: ?)              at 3243,3244  6×5  plane 0  1 rooms  1 doors (1 to outside)  0 open edges  32 objects (25 unnamed): …
50_50  castle                        at 3227,3212  4×14  planes 0–2  3 rooms (1 yards)  …  site 3203_3206_0
```

## What it finds

By the rules, on the whole map's 936 structures:

| Class | Structures |
| --- | --- |
| unknown | 281 |
| house | 170 |
| shack | 92 |
| shop | 83 |
| castle | 59 |
| smithy, ruin | 36 each |
| bank, tavern | 31 each |
| ship | 23 |
| church | 20 |
| workshop | 19 |
| library, magic tower | 14 each |
| farm | 12 |
| hall | 6 |
| mill | 4 |
| crypt | 3 |
| prison | 2 |

Of the 83 shops, 67 have an icon that says what they sell: 23 general stores, 7 clothes shops, 5 food shops, 3 each selling fishing gear, swords and herbs, and one or two each of 19 other kinds. The rooms come out as 423 terraces, 255 bedrooms, 224 storerooms, 208 dining rooms, 188 courtyards, 136 halls, 91 kitchens and fewer of the rest, with 419 unknown and 232 empty. 301 structures have an alternative; the commonest pairs are a shack and a house (52), a shop and a house (20), a smithy and a workshop (10), and a castle and a house, or a hall (10 each).

Region `50_50` reads as it looks: two castle blocks, a general shop, a smithy, a house, an axe shop with living quarters behind it, and a church. Its cookhouse, with a spit roast, standing torches and boxes, has too little to go on and comes out unknown by the rules; the forests call it a shack.

By the built-in forests (which learned from these same structures, so this shows the labels' shape rather than an honest score), the map is house 231, shack 114, shop 76, enclosure 57, castle 38, other 37, cave and crypt 34 each, ruin 33, smithy and tavern 30 each, tower 27, hall and bank 26 each, storehouse and church 20 each, workshop and ship 18 each, farm 16, magic tower and garden 13 each, barracks 6, stall, library and prison 5 each and mill 4. They agree with the rules on 488.

## Limits

- **Names can mislead.** Categories come from names, so an object named for something it isn't lands in the wrong category, and an unusual name in none.
- **Icons were read by eye.** What each icon shows and suggests is a reading of a small picture; a few are marked as saying nothing, and a few readings may be wrong.
- **Halls and warehouses.** A big room with many seats and a table reads as a hall even when it's mostly crates; the evidence shows which.
- **Houses and shacks overlap.** A small house with one room and a bed scores as both, and either may win.
- **Classes are per structure.** Each block of a castle or a farm is classed on its own, and a block that holds only, say, the castle's kitchen can read as a house; its site says what it belongs with.
- **No people.** Without NPCs, a shop with no counter and no icon, or a bank with its booths missing, has nothing to show for it.
- **Hand-written rules, noisy labels.** The rules' points were set by hand against a few regions. The forests do better but learn from labels made once, by eye, with nearly half at medium and a quarter at low confidence; where classes blur (shack and house, garden and enclosure), they can't be more certain than the labels.
- **One cache.** The forests learned on this cache's structures; on another, their features mean the same, but how well they do there hasn't been measured.

## See also

- [Structures](structures.md): how structures and rooms are found.
- [Inference](inference.md): the same approach for sequence roles, rules first, then learned trees.
- [Structures](structures.md#plans-and-sheets): the plans and sheets the labels were made from.
