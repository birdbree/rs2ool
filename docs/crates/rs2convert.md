[Home](../README.md) › [Crates](README.md) › rs2convert

# rs2convert

`rs2convert` turns decoded cache records into pack records. It builds the palette, converts models, assembles creatures, infers rigs, converts sequences into clips and validates them. This page maps the crate's API; the [conversion section](../conversion/README.md) explains each step in depth.

Entry point: [`crates/rs2convert/src/lib.rs`](../../crates/rs2convert/src/lib.rs). It depends on `rs2cache` for input and `contentpack` for output.

## Module map

| Module | What it does |
| --- | --- |
| [`run`](../../crates/rs2convert/src/run.rs) | The conversion commands as library calls, each returning an `Outcome`, and `write` to put one on disk. |
| [`pipeline`](../../crates/rs2convert/src/pipeline.rs) | Whole-creature conversion: model, rig, clips and validation. Also the player kit and the creature record-id namespaces. |
| [`creature`](../../crates/rs2convert/src/creature.rs) | Assembles an NPC or a kit part the way the original client does: models merged, recoloured, with its sequences and frame packs. |
| [`rig`](../../crates/rs2convert/src/rig.rs) | Rig inference from frame-base label sets. |
| [`clip`](../../crates/rs2convert/src/clip.rs) | Converts each frame of a sequence into pose keys per bone. |
| [`infer`](../../crates/rs2convert/src/infer.rs) | Links the cache doesn't store: candidate sequences for a model from frame-base families, label coverage and id distance, and each one's role (death, action, down, drop, vanish, idle or other) from its motion. See [inference](../conversion/inference.md). |
| [`classify`](../../crates/rs2convert/src/classify.rs) | A small decision tree that tells a sequence's role from its motion measures, learned from labelled sequences, with leave-one-out scoring, and `RoleTrees`: a global tree plus trees for skeleton families. `infer::npc_candidates_by` and `with_roles_by` take any judge, such as a tree, in place of the rules. See [learning roles](../conversion/inference.md#learning-roles-from-labels). |
| [`slot`](../../crates/rs2convert/src/slot.rs) | An item's equipment slot from its wear models' vertex labels, and which default body parts it covers. See [inference](../conversion/inference.md#equipment-slots). |
| [`validate`](../../crates/rs2convert/src/validate.rs) | Re-skins the creature's rest mesh with the encoded rig and clip records and compares it with the reference deformation. |
| [`deform`](../../crates/rs2convert/src/deform.rs) | The original client's frame application in integer arithmetic: the reference deformation. |
| [`space`](../../crates/rs2convert/src/space.rs) | Cache space to pack space. |
| [`palette`](../../crates/rs2convert/src/palette.rs) | The palette (up to 1024 entries) built from every model face colour in the cache. |
| [`model`](../../crates/rs2convert/src/model.rs) | One cache model to one `models` record, with texture UVs. |
| [`texture`](../../crates/rs2convert/src/texture.rs) | The 50 textures packed into one 1024 × 1024 atlas with shelf packing, plus a texture-mode material per texture. |
| [`sprite`](../../crates/rs2convert/src/sprite.rs) | Media and title sprites, and fonts, into a `sprites` pack. |
| [`floor`](../../crates/rs2convert/src/floor.rs) | One floor material per `flo.dat` entry, their names in a string table, and the floor → material map in the materials manifest. |
| [`object`](../../crates/rs2convert/src/object.rs) | `loc.dat` definitions into models, `objects` records, `kits` entries and a string table. |
| [`item`](../../crates/rs2convert/src/item.rs) | `obj.dat` definitions into `items` records, their inventory and wear models, and a string table, with inferred slots. See [Items](../pack/items.md). |
| [`preview`](../../crates/rs2convert/src/preview.rs) | The model a record describes, assembled as the original client draws it, for viewers. |

## `run`: the entry points

Every conversion `rs2ool pack` offers is a function here, and the explorer calls the same functions, so a conversion started from either produces byte-identical packs. All of them take a `&Records`, and all but `palette` take the source cache CRC to stamp into each pack.

| Function | Produces |
| --- | --- |
| `palette(records)` | The `Palette` and its `PaletteError` report, without writing anything. |
| `materials(records, crc)` | The materials pack, `palette.json` and a `palette.png` swatch. |
| `textures(records, crc)` | The texture atlas pack and the texture materials. |
| `models(records, crc, id)` | One model, or every model when `id` is `None`, converted in parallel. |
| `npc(records, crc, id, with_anims, extra, overrides)` | One NPC's merged model and rig, plus validated clips with `with_anims`. `extra` adds sequence ids; `overrides` is a parent table for the rig. |
| `player_kit(records, crc, with_anims, overrides)` | Every identity-kit part as a model, and the default body rigged with the stance sequences. |
| `sprites(records, crc, archives)` | Sprites and fonts from the archives `sprite_archives(name)` picks (media, title, or both). |
| `floors(records, crc)` | Floor materials and their string table. |
| `objects(records, crc, id)` | One location definition or all of them: models, objects, kits and strings packs (empty ones dropped). |

An `Outcome` holds `packs` (one `PackWriter` each), side `files`, RGBA `images`, the converted `creatures`, and a human-readable `log`. `flagged()` is true when any converted clip went over the validation threshold.

`run::write(&outcome, dir, png)` puts an outcome on disk: each pack at `<dir>/<kind>/<hash>.pack`, the side files, the images (through the `png` callback you pass, so the library needn't depend on an image encoder), each creature's `<key>/validation.json`, and a `validation_summary.json` merged with whatever summary is already in `dir`. It returns `(kind, record count, path)` per pack.

## `pipeline` and `creature`

`creature::npc` assembles an NPC: its models merged, recoloured, and its stand and walk sequences (plus any `extra` ids) with their frame packs. `merge` welds each face vertex to the first existing vertex at the same coordinates and drops unused vertices, as the original client's merge does; `recolour` applies a definition's recolours; `equip_model` builds an item's equipment model. The resulting `Creature` keeps the merged `model`, the definition `scale`, the `rest` mesh with that scale baked in, the resolved `sequences`, and the frame packs they use. `Creature::posed(s, i)` gives the reference vertices at frame `i` of sequence `s`.

`pipeline::convert(creature, palette, rig_id, with_anims, overrides)` runs the rest: model conversion, rig inference, then per sequence a clip conversion and validation. Clips are validated from their encoded and decoded records, so the report covers exactly the bytes that get written. `CreatureOutput::write` adds the model, rig and clips to three writers, and `pipeline::writers` makes those writers.

`pipeline::ids` holds the record-id namespaces for converted creatures (`CREATURE_MODEL`, `KIT_MODEL`, `PLAYER_MODEL`, `PLAYER_RIG`, and `clip(rig, seq) = rig << 16 | seq`); [record ids](../pack/record-ids.md) lists them all. The player's stance sequences aren't stored in the cache, so `pipeline` carries a fixed list of them.

## `rig`

`rig::infer(creature, overrides)` builds an `InferredRig`. A bone candidate is a distinct label set moved by a transform the creature's frames use, restricted to the labels its mesh has. `A` is `B`'s parent when `B` is a strict subset of `A` (after removing transitive edges, the largest of several minimal supersets wins). A candidate with no superset that overlaps another is attached by co-movement, and a root bone sits above everything. Each label binds to the smallest candidate containing it; labels in no candidate bind to the root.

Bones are ordered parents first, with pivots at the centroid of their rest vertices in cache coordinates. They're named from a built-in table of body regions by vertex label where one matches, and `bone_<n>` otherwise. `InferredRig::record()` produces the `rigs` record; `posed_pivots` gives the pivots in a posed frame (each bone's vertices' centroid) and `vertex_bones` each vertex's bone; `name_hash` is the FNV-1a hash used for bone names. A `ParentOverride` (the `--override` file) replaces the parent of the candidates it names. [Rig inference](../conversion/rig-inference.md) walks through it.

## `clip`, `deform` and `validate`

`deform` reproduces the original client's frame application vertex for vertex: `Groups` (vertices by label), `apply_frame`, `deformed` (frame, then the definition's scale) and `apply_scale`, with the client's 2048-entry sine tables in 16.16 fixed point.

`clip::convert(creature, rig, sequence, rig_id)` replays each frame's ops. Alongside the integer reference deformation, every bone accumulates the affine transform of the ops that move its whole label set. Bones are processed parents first against the parent's quantised transform, so rounding doesn't pile up down the chain. For each bone, `frame_keys` tries the quantised angles of its exact rotation and their neighbours, each with the least-squares translation for its vertices, and keeps the best. Frame lengths come from `delays`: the sequence's delay, else the frame's own, else 1. A sequence over 255 frames is an error.

`validate::validate` poses the rig from the decoded records (`pose`), skins the creature's rest mesh in pack space, and measures the distance from the reference deformation for every vertex and frame. It reports the max, the mean, and the five worst vertices per frame. A clip whose max exceeds `THRESHOLD` (4.0 cache units) is flagged. `validate::skinned` gives the skinned vertices of a converted model at one frame, `validate::rest_pivots` the bones' rest pivots, and `validate::vertex_bones` the bone each vertex follows. The explorer's bone overlay draws from `pose` and `rest_pivots`, and colours by bone with `vertex_bones`. See [validation](../conversion/validation.md) and [clips](../conversion/clips.md).

## `space`

The cache is right-handed with y down. Pack space negates y and z: `space::to_pack([x, y, z]) = [x, −y, −z]`, and `space::FLIP` is the sign vector `(1, −1, −1)`. Negating two axes is half a turn about x, a rotation, so models keep their handedness. Negating y alone looks just as plausible and mirrors every asset, so if a converted model comes out left-handed, check here first.

A key rotation `R` in a clip means `P · R317 · P` with `P = diag(1, −1, −1)`, where `R317` is the original client's rotation for the same 8-bit angles (`clip::rotation_317`, applied z, then x, then y). See [coordinates](../client/coordinates.md).

## `preview`

`preview::model_for(records, record)` returns the model a record describes as the original client would draw it: a model as is; an NPC, item (inventory model), identity-kit part or spot animation merged, recoloured and scaled; a location shaped for placement type 10, or its first type. `location_model(records, def, kind)` picks a location's model for one placement type. Viewers use these; packs don't.

## `material_id` and `source_cache_crc`

`material_id` fixes the material numbering: `PALETTE` = 0 is the palette-mode material every untextured face uses, texture `t` is `TEXTURE_BASE + t` (base 1), and floor `f` is `FLOOR_BASE + f` (base 1000). Faces reference materials through a `u8` slot, which is why model materials stay below 256.

`source_cache_crc(cache)` is the CRC32 of the stored versionlist archive. It changes whenever any versioned file does, and every pack records it in its manifest.

## Errors

`ConvertError` wraps `rs2cache::Error` and `contentpack::records::RecordError`, and adds `Invalid(String)` for problems found during conversion (a creature without vertex labels, a missing frame, a write failure).

## See also

- [Conversion overview](../conversion/README.md): the source → record mapping.
- [Creatures](../conversion/creatures.md): assembly in detail.
- [Animation](../client/animation.md): the frame application `deform` reproduces.
- [Pack commands](../tools/pack-commands.md): the CLI over `run`.
- [contentpack](contentpack.md): the records these functions produce.
