[Home](../README.md) › [Conversion](README.md) › Determinism

# Determinism

Converting the same cache twice gives byte-identical packs, and so the same pack names, whether the conversion runs from the command line or the explorer. This page lists what makes that hold and the tests that check it.

The general rules are on [Conventions](../development/conventions.md). The container, and how pack names are derived, are on [Container](../pack/container.md).

## Sorted ids

`PackWriter` keeps its records in a `BTreeMap` keyed by id. The index and blob are always written in ascending id order, whatever order records were inserted in. Inserting the same id twice keeps the last record. The manifest's `extra` tables are a `BTreeMap` too, and the JSON maps inside them have sorted keys.

The pack name is the hex SHA-256 of the whole file, and records are compressed with zstd at a fixed level (9), so equal records and an equal manifest give an equal name.

## Ordered collections, no clock

`rs2convert` and `contentpack` use `BTreeMap` and `BTreeSet` throughout and no hash maps, so no iteration order depends on hashing. Nothing reads the clock and nothing is random. The HSL→RGB conversion even leaves out the small random jitter the original client adds at startup (see [Palette](palette.md)). Manifests hold no timestamps.

## Parallel work, collected in order

Several steps run on `rayon`: the palette corpus, palette priming, model conversion, and object shaping and conversion. Each uses `par_iter().map(…).collect()`, which returns results in input order regardless of which thread finished first. The results then go into sorted maps or `PackWriter`s. Per-model colour counts are added as integers, so the order of the sum doesn't matter either.

## The palette

k-means is deterministic by construction: the first centre is the heaviest point, later centres are picked by a fixed score with ties going to the earliest point, and there's no random restart. The entries are then sorted and deduplicated, so the palette's order depends only on the set of colours, not on how they were seeded. Every run that writes palette indices rebuilds the palette the same way, so separate runs agree on every index. [Palette](palette.md) has the details.

## Same code path from the explorer

The explorer's convert action ([`explorer/actions.rs`](../../src/explorer/actions.rs)) computes the same source cache CRC, calls the same [`rs2convert::run`](../../crates/rs2convert/src/run.rs) function, and writes through the same `run::write` as `rs2ool pack`. Its packs are therefore byte-identical to the command line's. It doesn't take every command-line option. NPC and player-kit conversions always include animations, use no extra sequences and no overrides, and sprites always cover both archives. With the `caption` feature they also describe their clips, as `--describe` does.

`--describe` is the one input that isn't the cache: it reads the local [caption file](../tools/captions.md#where-captions-go). Two runs with the same cache and the same caption file write the same packs, and with no captions for the clips the packs are exactly what they'd be without the flag.

## How it's checked

| Test | Checks |
| --- | --- |
| [`tests/c_determinism.rs`](../../tests/c_determinism.rs) | runs `rs2ool pack npc --id 1 --with-anims`, `pack materials` and `pack textures` twice each into fresh directories, and compares the sorted lists of pack paths. The names are hashes of the whole file, so equal lists mean byte-identical packs. |
| [`tests/d8_actions.rs`](../../tests/d8_actions.rs) | converts one NPC through the explorer's action and through the `rs2ool` binary, then requires the same pack paths, identical pack bytes and the same set of output files |
| `baking_is_deterministic_and_order_independent` in [`contentpack/src/pack.rs`](../../crates/contentpack/src/pack.rs) | the same records and extras, inserted in a different order, bake to the same bytes and name |
| `kmeans_is_deterministic_and_separates_clusters` in [`contentpack/src/palette.rs`](../../crates/contentpack/src/palette.rs) | two k-means runs on the same points return identical centres |

The first two need a cache. They read `RS2_CACHE` and skip, rather than fail, when it isn't set. The explorer test also needs the `explorer` feature. See [Testing](../development/testing.md).

## See also

- [Container](../pack/container.md): pack layout, naming and CRCs.
- [Conventions](../development/conventions.md): the determinism rules for new code.
- [Conversion overview](README.md): the `run` entry points.
- [Explorer](../tools/explorer.md): the convert action.
