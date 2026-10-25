[Home](README.md) › Glossary

# Glossary

The words this wiki uses, in one place. Most have a fuller page; follow the links.

## Cache

**Cache.** The data directory of the original Java client: `main_file_cache.dat` plus five index files. See [The cache](cache/README.md).

**Index.** One of the five `main_file_cache.idx<N>` files, and by extension the set of files it lists. Index 0 holds archives, 1 models, 2 frame packs, 3 MIDI, 4 map files. See [Data and index files](cache/data-and-index-files.md).

**Sector.** A 520-byte block of the data file: an 8-byte header plus up to 512 bytes of payload. A file is a chain of sectors.

**File id.** A file's position in its index. Ids are dense; an *absent* file has an index record with no first sector.

**Version trailer.** The last two bytes of every stored file in indices 1–4, which the [versionlist](cache/versionlist.md) also records.

**Archive** (or jag archive). A file of index 0 that bundles named entries, compressed with headerless bzip2. See [Archives](cache/archives.md).

**Entry.** One named file inside an archive. Names are stored only as hashes.

**Jag hash.** The 32-bit hash archive entry names are looked up by: uppercase the name, then `h = h × 61 + (c − 32)` for each character, wrapping.

**Smart.** A variable-length integer, one byte when small and two when not. There's an unsigned form and a signed form; the formulas are on [Primitives](cache/primitives.md).

**Config.** The archive holding definition tables (`loc.dat`, `npc.dat` and the rest). Each definition is an opcode stream ending in 0. See [Config](cache/config.md).

**Opcode.** One field tag in a config definition, followed by that field's data.

## Records

**Record type.** One of the 17 kinds the tools list, dump and decode: model, anim, map, loc, npc, obj, idk, seq, spotanim, flo, varp, varbit, texture, sprite, font, interface, midi. See [Records](records/README.md).

**HSL colour.** A 16-bit colour: 6 bits hue, 3 bits saturation, 7 bits lightness. Model faces and recolours use it. See [Colour](client/colour.md).

**Recolour.** A `from → to` pair of HSL colours a definition applies to its models' faces.

**Label.** A byte tagging a model vertex (vertex label) or face (face label) as part of a group animations can move. See [Models](records/model.md).

**Textured face.** A face drawn with one of the 50 textures. Its colour field holds the texture id, and a *P/M/N triple* of vertices defines the texture's orientation.

**Priority.** A per-face draw-order group (0–11) the rasteriser uses to settle overlaps that depth sorting gets wrong. See [Rasteriser](client/rasteriser.md).

**Frame pack.** A file of index 2 holding animation frames that share one *frame base*. See [Frame packs](records/frame-pack.md).

**Frame base.** The list of *transform slots* a pack's frames use: each slot has a kind (origin, translate, rotate, scale, alpha) and the labels it moves.

**Frame-base family.** Frame packs with identical frame bases, and the sequences that use them: one skeleton's worth of animations. See [Inference](conversion/inference.md).

**Frame.** One pose: for some slots, the x/y/z values to apply. Frame ids are global 16-bit numbers.

**Sequence.** A seq.dat definition: an ordered list of frame ids with delays, plus loop and priority rules. See [Sequences](records/seq.md).

**Morph table.** A loc or npc field that swaps the definition for one of several children depending on a player variable. See [Config](cache/config.md).

**Region.** A 64 × 64-tile block of the map, id `x << 8 | y`. See [Map regions](records/map.md).

**Plane.** One of a region's four height levels, 0 being the ground.

**Location.** An object placed on a region tile: a loc id, plane, x, y, type (0–22) and rotation.

**Client cycle.** 20 ms, the unit of animation timing.

## Packs and conversion

**Pack.** One file of the content-pack format: a header, a JSON manifest, an index and a blob of records of one *kind*. See [Packs](pack/README.md).

**Kind.** What a pack holds: models, materials, textures, sprites, objects, kits, rigs, anims, strings (plus reserved numbers for items and fluids).

**Record id.** A record's u32 key within a pack. Several namespaces share each kind's id space; see [Record ids](pack/record-ids.md).

**Palette.** The 1024-entry RGB table pack models index into, built from every face colour in the cache. See [Palette](conversion/palette.md).

**Pack space.** Pack coordinates: cache coordinates with y and z negated, which is a half turn about x rather than a mirror. See [Coordinates](client/coordinates.md).

**Rig.** A bone hierarchy with rest pivots and a label → bone table. See [Rigs](pack/rigs.md) and [Rig inference](conversion/rig-inference.md).

**Clip.** One converted sequence: pose keys per bone per frame. See [Anims](pack/anims.md).

**Structure.** A building found in the map: rooms joined by doors and stacked floors, with its walls, roofs and contents. See [Structures](conversion/structures.md).

**Room.** Roofed tiles on one plane that reach each other without crossing a wall; the roof flag is tile flag 4. See [Structures](conversion/structures.md#finding-rooms).

**Yard.** A room open to the sky: unroofed tiles walled in all round, such as a courtyard, a walled garden, a pen or a roof terrace. See [Structures](conversion/structures.md#yards).

**Furnishing category.** What a placed object is in general terms (a bed, a counter, storage), from its name; the evidence for what a room is for, and what lets one object stand in for another in a blueprint. See [Structure purposes](conversion/structure-purposes.md#furnishing-categories).

**Material.** What a wall, roof piece or floor is made of (stone, wood, thatch, or plain with a colour), from its texture, its name or its colour. See [Materials](conversion/structures.md#materials).

**Site.** Neighbouring structures that belong together, such as a castle's blocks or a farm's barns, as the learned forests group them. See [Learned from labels](conversion/structure-purposes.md#forests).

**Opening.** An edge where a roofed room meets the outside with no wall: an open doorway, an archway, a stall's open front.

**Reference deformation.** A frame applied to a model exactly as the original client applies it, in its integer arithmetic. Validation compares converted clips against it. See [Validation](conversion/validation.md).

## See also

- [Architecture](architecture.md): where each of these lives in code.
- [The cache](cache/README.md): the format these terms describe.
