[Home](../README.md) › [Tools](README.md) › Examples

# Examples

One example per `rs2ool` subcommand, to copy and adapt. [Cache commands](cache-commands.md), [pack commands](pack-commands.md) and [the explorer](explorer.md) have every flag. The tools only read the cache and write to the output directory you give them. No cache is included in the repository; point them at your own copy.

- `rs2ool cache …` lists, extracts, decodes and verifies the cache.
- `rs2ool pack …` converts records into packs, and inspects and builds packs.
- `rs2ool cache explorer` and `rs2ool pack explorer` open the GUI.

Every command that reads the cache takes the cache directory (the one holding `main_file_cache.dat` and `.idx0`–`.idx4`) as `--cache <dir>`, or from the `RS2_CACHE` environment variable. The examples assume `RS2_CACHE` is set.

Exit codes: `0` success, `1` usage error, `2` cache or I/O error, `3` partial failure (some records failed, as the manifest or the verify report lists, or a converted clip is over the validation threshold).

## `rs2ool cache`

### `list`

The five indices with file counts, and every archive with its entry names:

```
rs2ool cache list
```

One index's ids and sizes, one archive's entries, or one record type's ids (with names for `loc`, `npc`, `obj`, `sprite`, `font` and `map`):

```
rs2ool cache list --index 4
rs2ool cache list --archive config
rs2ool cache list --type npc
```

### `dump`

Pick exactly one source: `--index N`, `--archive NAME`, `--type T`, `--region RX_RY`, `--all-regions` or `--group G`. Pick ids for `--index` and `--type` with `--id M`, `--range A-B` or `--all`.

Inflated files of an index, one per id, at `<out>/idx<N>/<id>.bin` (`--raw` writes the stored bytes, version trailer included):

```
rs2ool cache dump --index 1 --all -o out/
```

An archive's entries at `<out>/archive<N>/<entry>` (unknown names appear as `#hash`), or the archive decoded into its record types with `--decode`:

```
rs2ool cache dump --archive config --entry loc.dat -o out/
rs2ool cache dump --archive media --decode -o out/
```

Decoded records of one type at `<out>/<type>/<id>.json`. Textures, sprites and fonts are written as PNG, fonts also as `<name>.metrics.json`, and MIDI as `.mid`. A full interface dump also writes `interface/tree.json`, with children nested under each root.

```
rs2ool cache dump --type model --range 0-99 -o out/
rs2ool cache dump --type sprite --all -o out/
```

A map region's tile and location files, or the decoded region with `--decode`:

```
rs2ool cache dump --region 50_50 --decode -o out/
rs2ool cache dump --all-regions -o out/
```

A record and everything it depends on: `npc:<id>` (models, sequences, frame packs), `item:<id>` (inventory, equip and chat-head models) or `player-kit` (identity-kit parts and the standard stance sequences):

```
rs2ool cache dump --group npc:1 -o out/
```

Output options, valid on every `dump` and `decode`:

- `--meta` writes a `<id>.meta.json` sidecar beside each output: source, sector chain, sizes, CRC, compression and a summary for the type.
- `--meta-only` writes only the sidecars.
- `--xref` adds reverse references to sidecars (models → textures, textures → models). They are computed once and kept in `<out>/xref.json`, which also lists object → model, sequence → frame pack and region → object references.
- `--jsonl` writes `<out>/<type>.jsonl`, one record per line, instead of a file per record.

Every run writes `<out>/manifest.json`, which lists each file with its source, sizes and CRC32, plus absent ids and failures. It is sorted, so two runs over the same input produce identical manifests.

### `decode`

Shorthand for `dump --type`:

```
rs2ool cache decode npc --id 1 -o out/
```

### `info`

Print one record's metadata sidecar to stdout:

```
rs2ool cache info --type model --id 0
```

### `verify`

Follow every sector chain and check every header, inflate every file, decode every archive, and compare indices 1–4 with the versionlist's versions and CRCs:

```
rs2ool cache verify
```

It reports, per index, files that are present, absent, truncated, bad-sector or inflate-failed, with ids, and exits `3` if any are damaged. Files that differ from the versionlist are listed as *modified*, which is not an error.

### `render`

Draw a model with the classic renderer: an NPC at rest, its stand sequence as a GIF, and the player's walk as six frames, the converted clip under the original:

```
rs2ool cache render npc --id 1 -o npc1.png
rs2ool cache render npc --id 1 --seq -o npc1-stand.gif
rs2ool cache render kit --seq 819 --sheet 6 --compare -o walk.png
```

`--yaw`, `--pitch`, `--distance` and `--size` set the camera and image. Any other extension than `.gif` writes a PNG, animated when there's more than one frame. See [Cache commands](cache-commands.md#render).

### `caption`

Draw NPC 1's candidate sequences as frame grids without sending anything, then caption five of them with a vision model on a local Ollama server:

```
rs2ool cache caption --npc 1 --dry-run --images frames/
rs2ool cache caption --npc 1 --enable --model <a vision model> --limit 5
```

Each caption prints as `npc 1 seq 2820: [death] Collapses forward and lies flat` and is kept in your data directory, outside the repository. `--check` lists the server's models. Captioning is off until it's turned on in the settings or with `--enable`. See [Captions](captions.md).

### `classify`

Learn roles from the captions you've reviewed and see how they compare with the hand-tuned rules, then keep the tree for the explorer's sequence picker:

```
rs2ool cache classify --verbose
rs2ool cache classify --save
```

It prints each side's score, a confusion table for each and the tree itself. See [Captions](captions.md#learning-roles-from-reviews).

### `explorer`

Open the GUI on the cache. `--select` picks a record at startup (`TYPE:ID`, or `kit` for the player kit), `--compare` shows its converted version beside it, `--classic` starts in the classic renderer, and `--pack` opens packs too:

```
rs2ool cache explorer --select npc:1 --compare --pack packs/models/<hash>.pack
```

The explorer needs the default `explorer` feature. `cargo build -p rs2ool --no-default-features` builds the command line without Bevy and without the `explorer` subcommands.

## `rs2ool pack`

Converters write `<dir>/<kind>/<hash>.pack`. Two runs on the same cache write byte-identical packs.

### `materials`

The client palette, built from every model colour, as a materials pack, plus `palette.png`:

```
rs2ool pack materials -o packs/
```

### `textures`

The 50 textures as one atlas, with a material per texture:

```
rs2ool pack textures -o packs/
```

### `models`

Models into a models pack, one (`--id`) or every one (`--all`):

```
rs2ool pack models --id 0 -o packs/
```

### `npc`

An NPC's merged model and inferred rig. `--with-anims` also converts and validates its clips, writing `validation.json`; `--seq` adds sequences the NPC record does not name, and `--override` replaces rig inference's parent choice:

```
rs2ool pack npc --id 1 --with-anims -o packs/
```

### `player-kit`

Every identity-kit part, plus the default body rigged with the player stance sequences:

```
rs2ool pack player-kit --with-anims -o packs/
```

### `sprites`

Media and title sprites, and the fonts, into a sprites pack (`--archive media` or `--archive title` for one archive):

```
rs2ool pack sprites --all -o packs/
```

### `floors`

Floor types into floor materials and a string table:

```
rs2ool pack floors -o packs/
```

### `objects`

World objects into models, objects, wall kits and strings:

```
rs2ool pack objects --all -o packs/
```

### `items`

Items into items records, their inventory and wear models, and strings, each with the slot inferred from its wear models:

```
rs2ool pack items --all -o packs/
```

### `spotanims`

Spot animations into models, rigs and validated clips, with each one's turn and lighting in the models pack's manifest:

```
rs2ool pack spotanims --all -o packs/
```

### `inspect`

Print a pack's manifest and index:

```
rs2ool pack inspect packs/models/<hash>.pack
```

### `build`

Bake a directory of loose records (`pack.json` plus `<id>.bin`) into a hashed pack:

```
rs2ool pack build loose/ -o out/
```

### `explorer`

Open the GUI on packs. The cache opens too when `--cache` or `RS2_CACHE` gives one, so compare mode can pair the packs with their originals:

```
rs2ool pack explorer packs/models/<hash>.pack packs/materials/<hash>.pack
```

## See also

- [Using rs2ool](README.md): the command tree and exit codes.
- [Dump output](dump-output.md): what the cache commands write.
- [Getting started](../getting-started.md): building and installing.
