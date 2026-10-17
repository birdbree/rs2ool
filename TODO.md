# TODO

Rough edges, gaps and features we don't have yet. Roughly in order of how much they'd matter.

## Bugs and rough edges

- [ ] **65535 isn't always "none".** npc stand/walk sequences, obj equip/chat-head/noted ids and spotanim model/sequence keep 65535 as a real id, so it gets looked up (`creature::sequences` would try seq 65535). Check what the original client does per field and match it.
- [ ] **Clip ids can collide.** `rig << 16 | seq` loses bits once a rig id reaches 0x10000, and an NPC with id 0xFFFF shares a rig id with the player. Pick an id scheme that can't overlap.
- [ ] **Merging wraps texture triangle indices.** `creature::merge` shifts a textured face's triangle index inside a `u8` face type, so it wraps past 63 merged triangles. Error out, or widen it.
- [ ] **Unchecked counts in two encoders.** `KitRecord::encode` and `material::encode_palette` write `len() as u16`. Past 65535 the record is wrong, and its own decoder rejects it. `ObjectRecord` checks; these should too.
- [ ] **Pack reader trusts the header.** It decodes with the manifest's codec, never checks that header and manifest agree on kind, codec and count, and doesn't check the index is sorted, though lookup is a binary search.
- [ ] **Pack names ignore the manifest.** Same records with a different manifest (or kind) get the same name. Either hash the manifest too or document it as intended.
- [ ] **Loose directories accept `1.bin`, `01.bin` and `+1.bin` as the same id**, and whichever the directory lists last wins. Reject non-canonical names.
- [ ] **Sidecar CRCs mean two things.** `crc32` is over the stored bytes for index files (trailer included) but over the unpacked data for archive entries, and neither matches the versionlist CRC. Pick one meaning, or add the versionlist-style CRC alongside.
- [ ] **A missing font entry says "archive has no index.dat".** `decode_font` returns `SpriteError::NoIndex` for both cases. Give it its own error.
- [ ] **`LocDef::flat_shading` is probably named backwards.** Opcode 22 makes the original client keep vertex normals rather than light the model straight away. The `FLAT_SHADED` placement flag inherits the confusion.
- [ ] **A sequence with no frames stays empty.** The original client gives it one frame of −1. Probably harmless; worth matching.
- [ ] **Sprite ids include entries that aren't sprites.** Any title or media entry whose first two bytes happen to land inside `index.dat` gets an id, then fails to decode (the title JPEG does). Filter those out of the id list.
- [ ] **The font column rule sums palette indices as unsigned.** The original client sums signed bytes. Same answer below index 128.
- [ ] **`--meta-only --xref` rebuilds the cross-references every run**, because `xref.json` is a payload and `--meta-only` skips payloads.
- [ ] **`rs2ool pack sprites --all` does nothing.** It's accepted and ignored. Either drop it or make `--archive` and `--all` mean something together.
- [ ] **Terrain colours disagree between views.** The modern view colours a tile underlay-first, the classic view and the world map overlay-first.
- [ ] **The classic renderer leaves the last column of clipped faces undrawn**, and a missing-texture face drawn grey keeps its alpha while textured faces are always opaque.
- [ ] **Have a look at engine lighting.** The normals fix is tested but not yet eyeballed in the window.

## Fidelity gaps

- [ ] **Secondary frames and interleaving.** Decoded and stored, but neither playback nor clip conversion mixes a secondary frame in through the interleave slots.
- [ ] **Scale in clips.** Clip keys have no scale, so scale transforms show up as validation error instead of being carried.
- [ ] **Alpha transforms.** Kind 5 frames are decoded but nothing applies them, in playback or anywhere else.
- [ ] **Classic terrain.** Tiles are flat colours. The original client blends underlays, draws the overlay shapes, lights the ground and textures it.
- [ ] **Overlay shapes in both views.** Shape and rotation are decoded, but the modern view only does whole tiles and half-tile triangles, ignoring rotation.
- [ ] **Neighbouring regions aren't stitched**, so edge corners use the edge tile's own height.
- [ ] **Interfaces draw text in egui's font**, and models, inventories and item text as labelled boxes. Use the cache's bitmap fonts and draw the models.
- [ ] **A pixel-exact check against the original renderer.** Needs a Java runtime to render reference frames from the original client and diff them.

## Features

- [ ] **CI.** A workflow running fmt, clippy (with and without the explorer) and the tests that don't need a cache.
- [ ] **glTF export** for looking at a converted model, rig and clips in other tools.
- [ ] **Render to PNG from the command line**, with the classic renderer: `rs2ool cache render model --id N`, plus a camera and size. Handy for docs and diffs.
- [ ] **Item packs.** Kind 8 is reserved with no record layout: inventory model, equip models per body type, icon camera, stack variants. Note that 317 doesn't record which slot an item goes in.
- [ ] **Spot animation conversion** into a model plus clip.
- [ ] **A bone overlay in the explorer.** `Overlays::bones` is there but never drawn. Show pivots and parent lines for converted rigs, and maybe which bone each vertex follows.
- [ ] **Evaluate morph tables.** Let the explorer set a varp or varbit value and show the state the original client would pick, not just list the states.
- [ ] **Search every type at once** in the explorer, not one list at a time.
- [ ] **Female default body** for the player kit conversion, and real `hide_mask` values (always 0 at the moment).
- [ ] **Compare two caches**: which files differ, by versionlist and by content.

## Housekeeping

- [ ] **Rename the test files.** `a1_raw.rs`, `c4_c7_creatures.rs`, `d6_kit.rs` and the rest still carry their old step numbers. Name them by what they test.
- [ ] **`insta` with no snapshots.** It's a dev-dependency nothing uses. Add synthetic snapshot tests for decoded records, or drop it.
