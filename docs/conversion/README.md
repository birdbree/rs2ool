[Home](../README.md) › Conversion

# Conversion

This section explains how decoded cache records become pack records: which cache data feeds which record, what each step computes, and which constants and limits apply. Read it when you want to know why a converted record holds the values it does.

All conversion code lives in [`rs2convert`](../crates/rs2convert.md) ([`crates/rs2convert/src`](../../crates/rs2convert/src)). It reads cache data through [`rs2cache`](../crates/rs2cache.md), and it builds records with the encoders in [`contentpack`](../crates/contentpack.md). The record layouts themselves are on the [pack pages](../pack/README.md). This section covers how the fields get filled.

## Pages

- [Palette](palette.md): the 1024-entry colour palette built from every untextured face colour.
- [Models](models.md): a cache model as a `models` record (axes, palette indices, texture UVs, alpha, priorities).
- [Creatures](creatures.md): NPC and player-kit assembly, the sequences collected, and the frame index.
- [Rig inference](rig-inference.md): bones derived from frame-base label sets.
- [Clips](clips.md): sequences as per-bone rotation and translation keys.
- [Validation](validation.md): re-skinning the encoded records and measuring the error against the original client's deformation.
- [Objects](objects.md): location definitions as models, `objects` records, wall kits and strings.
- [Floors](floors.md): floor definitions as floor materials.
- [Textures and sprites](textures-and-sprites.md): the texture atlas, texture materials, sprites and font glyphs.
- [Determinism](determinism.md): why two runs produce byte-identical packs.
- [Inference](inference.md): links the cache doesn't store (which sequences animate an NPC, an item's equipment slot) and how they can be inferred.
- [Structures](structures.md): whole buildings found in the map, with their rooms, doors, walls, roofs and contents, as blueprints.

## Source to record mapping

Each `rs2ool pack` subcommand is one function in [`rs2convert::run`](../../crates/rs2convert/src/run.rs). Record ids are listed in full on [Record ids](../pack/record-ids.md).

| Subcommand | `run` function | Cache source | Packs and records written |
| --- | --- | --- | --- |
| `materials` | `materials` | every model's untextured face colours | `materials`: palette record `0xFFFFFFFF`, palette material 0; side files `palette.json`, `palette.png` |
| `textures` | `textures` | textures 0–49 | `textures`: atlas `0x80000000`, one rect per texture id; `materials`: materials 1–50 |
| `models` | `models` | one model, or every model in index 1 | `models`: record id = model id |
| `npc` | `npc` | `npc.dat` entry, its models, its sequences and their frame packs | `models`: `0x10000000 \| npc`; `rigs`: `npc`; `anims` (with `--with-anims`): `npc << 16 \| seq` |
| `player-kit` | `player_kit` | `idk.dat`, their models, seven fixed stance sequences | `models`: `0x20000000 \| idk` per part, `0x30000000` for the default body, manifest extra `kit_parts`; `rigs`: `0xFFFF`; `anims`: `0xFFFF << 16 \| seq` |
| `sprites` | `sprites` | media and title sprites, the four fonts | `sprites`: `sprite << 8 \| sub`, glyphs `0x01000000 \| font << 8 \| glyph`, metrics `0x02000000 \| font` |
| `floors` | `floors` | `flo.dat` | `materials`: `1000 + flo`, manifest extra `floors`; `strings`: `0x100` |
| `objects` | `objects` | `loc.dat` (plus morph children), their models | `models`: `0x40000000 \| loc << 4 \| n`; `objects`: `loc`; `kits`: `loc`; `strings`: `0x200` |
| `items` | `items` | `obj.dat`, its inventory and wear models | `models`: `0x50000000 \| item` and `0x60000000 \| item << 1 \| body`; `items`: `item`; `strings`: `0x300` |

Every run that writes palette indices (`models`, `npc`, `player-kit`, `floors`, `objects`, `items`) builds the palette from scratch first, the same way `materials` does. That's why indices written by separate runs agree with each other.

Material ids share one space: 0 is the palette material, `1 + t` is texture `t`, `1000 + f` is floor `f` (constants in `rs2convert::material_id`). Wall kits name material 0.

## Entry points and `Outcome`

Each `run` function takes a [`Records`](../crates/rs2cache.md) reader and the source cache CRC, and returns an `Outcome`:

| Field | Holds |
| --- | --- |
| `packs` | the `PackWriter`s to bake |
| `files` | side files as `(relative path, bytes)`, for example `palette.json` |
| `images` | RGBA images as `(relative path, width, height, pixels)`, saved as PNG |
| `creatures` | converted creatures keyed for their report directory (`npc_<id>`, `player`) |
| `log` | one line per notable result |

`Outcome::flagged()` is true when any converted clip exceeds the [validation threshold](validation.md).

`run::write(outcome, dir, png)` puts an outcome on disk:

1. each pack at `<dir>/<kind>/<sha256>.pack` (see [Container](../pack/container.md));
2. each side file, and each image through the `png` callback;
3. for each creature, `<dir>/<key>/validation.json`, then `<dir>/validation_summary.json`, merged into whatever summary is already there.

It returns `(kind, record count, path)` for every pack written. A creature gets its `validation.json` even without `--with-anims`. It just has no clips in it.

The source cache CRC (`rs2convert::source_cache_crc`) is the CRC32 of the stored versionlist archive's raw bytes. Every pack manifest carries it.

The explorer's convert action calls the same functions and the same `write`, so its packs match the command line's byte for byte. See [Determinism](determinism.md).

## See also

- [Pack commands](../tools/pack-commands.md): the command-line flags for each subcommand.
- [Pack format](../pack/README.md): kinds and record layouts.
- [Record ids](../pack/record-ids.md): every id namespace.
- [`rs2convert` crate](../crates/rs2convert.md): module overview.
