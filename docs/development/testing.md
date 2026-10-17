[Home](../README.md) › [Development](README.md) › Testing

# Testing

How the tests are organised, which ones need a cache, and the tooling they use. Read this before adding a test, so it lands in the right place and doesn't check in anything it shouldn't.

## Two kinds of test

**Unit tests** live in `#[cfg(test)]` modules next to the code. They never touch a real cache: every input is built inside the test. They run anywhere, with or without `RS2_CACHE`.

**Integration tests** live in `crates/<crate>/tests/`. Most of them read a real cache from `RS2_CACHE` and check a decoder or converter against every record of a type, or drive the built binary or the explorer. Without `RS2_CACHE` they skip.

Nothing derived from a cache is checked into the repository: no fixture files, no sample records, not even a single model. If a test needs real data, it's an integration test that reads `RS2_CACHE` at run time.

## Synthetic inputs

The decoders are tested on bytes built in the test. Where a format is involved enough to need it, the module has a test-only `synth` submodule (`#[cfg(test)] pub(crate) mod synth`) that encodes the format, so tests can write a structure and decode it back:

| Module | Its `synth` builds |
| --- | --- |
| `rs2cache::cache` | A data file and index files from `(id, bytes)` lists, laid out in 520-byte sector chains with sector 0 left unused. |
| `rs2cache::compress` | gzip and headerless bzip2 streams. |
| `rs2cache::archive` | Jag archives, compressed per entry or as a whole. |
| `rs2cache::model` | A model file from a `Model`, sections and trailer included. |
| `rs2cache::anim` | A frame pack from a frame base and raw frames. |
| `rs2cache::config` | `Ops`, a builder for config opcode streams, used by the tests of every config decoder. |

The map, sprite, interface and versionlist tests are small enough to build their bytes inline. Corruption tests start from a synthetic layout and break one thing: point a sector's next pointer at 0, change a type byte, truncate the data file, and check the error that comes back names the fault.

In `contentpack`, every record type round-trips through `encode` and `decode`, and the pack tests cover sorting by id, an uncompressed codec, and CRC and magic failures. The loose-directory test writes a small directory under the system temp directory and bakes it. Unit tests in `rs2convert`, `rs2classic` and `rs2ool` follow the same rule: synthetic models, frames and images only.

## Cache-gated integration tests

A test that needs the cache opens it through a helper in `tests/common/mod.rs` (in `rs2cache` and `rs2convert`), or the same check written inline (in `rs2classic` and `rs2ool`). With `RS2_CACHE` unset the test prints `skipping: RS2_CACHE is not set` to stderr and returns, so it passes. With `RS2_CACHE` set to something that won't open, it fails. Run with `--nocapture` if you want to see the skip notes.

These tests only read the cache. Anything they write goes under Cargo's per-target temp directory (`CARGO_TARGET_TMPDIR`).

### `rs2cache`

| File | Checks |
| --- | --- |
| `tests/a1_raw.rs` | Every file in every index reads without a sector error, at the size its index record gives. |
| `tests/a2_inflate.rs` | Every non-empty index-1 and index-2 file is gzip and inflates. |
| `tests/a3_archives.rs` | Every archive decodes, and the config archive holds its named entries. |
| `tests/a4_config.rs` | Every definition of every config type decodes, unknown opcodes are reported with their definition ids, and each definition ends exactly where the next begins; plus spot checks on individual definitions. |
| `tests/a5_models.rs` | Every model decodes with its sections filling the file exactly; plus bounds checks on individual models. |
| `tests/a6_frames.rs` | Every frame pack decodes with exact section lengths and in-range transform slots, every frame a sequence references is defined by exactly one pack, and a sequence moves only labels its model has. |
| `tests/a7_textures.rs` | The 50 textures decode as single sprites that pad to 64 × 64 or 128 × 128, with visible content. |
| `tests/a8_maps.rs` | Every region in `map_index` decodes with its tile file consumed exactly, and every location type is 0–22; plus a spot check of one region's planes. |
| `tests/a9_sprites.rs` | Every media and title sprite entry decodes, the four fonts have 256 glyphs each, and interfaces decode fully with every layer child present; plus spot checks on individual records. |
| `tests/a10_remaining.rs` | Varps and varbits decode exactly with no unknown opcodes, every MIDI file inflates to MIDI, and models and frame packs carry the version and CRC their versionlist tables give. |
| `tests/a_records.rs` | Every record type lists its ids, decodes its first and last id, and produces a sidecar that survives a JSON round trip; model sidecar counts match the model. |

### `contentpack`, `rs2classic` and `rs2convert`

| File | Checks |
| --- | --- |
| `contentpack/tests/c1_pack.rs` | A pack holding one synthetic model reads back with the record byte-identical. This one needs no cache. |
| `rs2classic/tests/render_models.rs` | Renders a smooth-shaded model with priorities and a textured model; checks screen coverage and that every drawn pixel comes from the HSL table or a texture. Writes PPM frames under the target temp directory for a look. |
| `rs2convert/tests/c2_palette.rs` | The palette quantises the model colours with a mean error under 6 and a max under 24 RGB units. |
| `rs2convert/tests/c3_models.rs` | Every model converts with matching counts, a pack of all of them reads back one record per model, and a converted model comes out upright (y up). |
| `rs2convert/tests/c4_c7_creatures.rs` | Creature assembly into one labelled mesh, rig inference, clips covering every frame, validation of humanoid and non-humanoid creatures, a parent override taking effect, and the player kit's parts and stance. |
| `rs2convert/tests/c8_textures_sprites.rs` | The texture atlas round-trips every texture pixel-exact, and every sprite round-trips through its pack record. |
| `rs2convert/tests/c9_floors.rs` | One floor material per `flo.dat` entry, textured floors reference existing texture materials, and the manifest maps every floor. |
| `rs2convert/tests/c10_objects.rs` | Every `loc.dat` definition converts, footprints carry over, definitions with a morph table get more than one state, and wall definitions produce kits entries. |

## CLI tests

Some `rs2ool` integration tests run the built binary, as you would, through `Command::new(env!("CARGO_BIN_EXE_rs2ool"))`:

| File | Checks |
| --- | --- |
| `rs2ool/tests/b_cache.rs` | `cache list` output, raw dumps (the manifest is identical across two runs, every file has the size the manifest gives, and a one-id dump matches the full one), decoded output, `--meta-only` writing only sidecars, region and image dumps, `interface/tree.json`, a dependency group listing each dependency once, and `verify`. Contradictory selectors exit 1. |
| `rs2ool/tests/c_determinism.rs` | The determinism test, below. |
| `rs2ool/tests/d8_actions.rs` | The explorer's convert action yields packs byte-identical to `rs2ool pack npc --id 1 --with-anims`, and its export writes the same files as `rs2ool cache dump`. |

The `verify` test also builds a damaged cache: it copies the index files into the temp directory, writes a truncated copy of the data file next to them, and checks that `verify` on the copy exits 3 and names what's truncated. The real cache is never touched.

## The determinism test

`rs2ool/tests/c_determinism.rs` runs `rs2ool pack npc --id 1 --with-anims`, `rs2ool pack materials` and `rs2ool pack textures` twice each, into separate directories, and compares the sorted lists of pack paths. Packs are named by the SHA-256 of their index and blob, so equal names mean byte-identical records. In `contentpack`, the unit test `baking_is_deterministic_and_order_independent` checks the same thing at the container level: inserting records and manifest extras in a different order bakes the same bytes. See [determinism](../conversion/determinism.md) for what makes the output stable.

## Headless explorer tests

The explorer tests (`rs2ool/tests/d1_skeleton.rs` to `d7_maps.rs`, plus `d8_actions.rs`) build a Bevy `App` with `MinimalPlugins` and the explorer's own plugins, then call `app.update()` in a loop until background tasks finish. There's no window and no GPU. The camera tests set `TimeUpdateStrategy::ManualDuration` so each update advances a fixed 16 ms. Each file starts with `#![cfg(feature = "explorer")]`, so with `--no-default-features` they compile to empty test binaries.

| File | Checks |
| --- | --- |
| `d1_skeleton.rs` | Opening a cache runs on the task pool while the update loop keeps going, and the tree then lists the five indices with their file counts. |
| `d2_browser.rs` | Selecting a definition decodes it off the UI thread and fills the inspector; the npc list loads with names and filters by name; definitions resolve to drawable models. |
| `d3_viewport.rs` | A mesh has a vertex per face corner, finite normals and the model's bounds (y up); arrow keys orbit; WASD flies and the camera stays bounded; switching lighting swaps prebuilt meshes. |
| `d4_compare.rs` | A cache model and its converted pack record resolve from both sources, pair up and draw at the same size, while a deliberately mis-scaled record doesn't; compare panes share one orbit and split the viewport. |
| `d5_viewport2d.rs` | A sprite drawn at 4× is crisp (each texel exactly four physical pixels across), a font lays out 256 glyphs, and an interface nests its children with their sprites. |
| `d6_kit.rs` | The kit's default slots rebuild the converter's default body; an item in an empty slot adds its equipment model, and one in a kit-backed slot replaces that part. |
| `d6_playback.rs` | A sequence plays at the original client's speed (frame delays at 20 ms per cycle), and the converted creature stays in step: every frame, the skinned pack vertices sit where the reference deformation puts the cache vertices. |
| `d6_states.rs` | A location with a morph table lists its default and child states, each with its own model, and the converted objects pack lists the same states and options. |
| `d7_maps.rs` | A region builds terrain for planes 0–2 and models for its locations, including those on upper planes. |

Several of these (the orbit, fly, compare-camera, viewport-split and lighting-switch tests) need no cache and always run.

## Property tests and snapshots

`proptest` covers the cursor in `rs2cache::io`:

- `smart_round_trips`: every value in 0–32767 encodes and reads back through `Cursor::smart`, consuming exactly its bytes.
- `signed_smart_round_trips`: the same for −16384–16383 through `Cursor::signed_smart`.
- `reads_never_panic`: random byte strings of up to 16 bytes, fed to the smart, string and fixed-width readers, produce errors at worst, never panics.

`insta` (with its JSON feature) is a dev-dependency of `rs2cache` for decoded-record snapshots. No snapshot tests are checked in at the moment, so `cargo insta review` has nothing to review until one is added. A snapshot of a real record would be cache-derived, so a snapshot test has to build its input synthetically too.

## See also

- [Development](README.md): the build, test and lint commands.
- [Conventions](conventions.md): the rules the tests enforce.
- [Primitives](../cache/primitives.md): the smart encodings the property tests cover.
- [Explorer](../tools/explorer.md): the GUI the headless tests drive.
- [rs2ool](../crates/rs2ool.md): why the explorer's plugins are public.
