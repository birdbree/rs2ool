# cachedump

Lists, extracts, decodes and verifies a RuneScape 2 revision-317 cache. It only reads the cache and writes to the output directory you give it.

Every command takes the cache directory (the one holding `main_file_cache.dat` and `.idx0`–`.idx4`) as `--cache <dir>`, or from the `RS2_CACHE` environment variable. The examples assume `RS2_CACHE` is set.

Exit codes: `0` success, `1` usage error, `2` cache error, `3` partial failure (some records failed; see the manifest or the verify report).

## `list`

The five indices with file counts, and every archive with its entry names:

```
cachedump list
```

One index's ids and sizes, one archive's entries, or one record type's ids (with names for `loc`, `npc`, `obj`, `sprite`, `font` and `map`):

```
cachedump list --index 4
cachedump list --archive config
cachedump list --type npc
```

## `dump`

Pick exactly one source: `--index N`, `--archive NAME`, `--type T`, `--region RX_RY`, `--all-regions` or `--group G`. Pick ids for `--index` and `--type` with `--id M`, `--range A-B` or `--all`.

Inflated files of an index, one per id, at `<out>/idx<N>/<id>.bin` (`--raw` writes the stored bytes, version trailer included):

```
cachedump dump --index 1 --all -o out/
```

An archive's entries at `<out>/archive<N>/<entry>` (unknown names appear as `#hash`), or the archive decoded into its record types with `--decode`:

```
cachedump dump --archive config --entry loc.dat -o out/
cachedump dump --archive media --decode -o out/
```

Decoded records of one type at `<out>/<type>/<id>.json`. Textures, sprites and fonts are written as PNG, fonts also as `<name>.metrics.json`, and MIDI as `.mid`. A full interface dump also writes `interface/tree.json`, with children nested under each root.

```
cachedump dump --type model --range 0-99 -o out/
cachedump dump --type sprite --all -o out/
```

A map region's tile and location files, or the decoded region with `--decode`:

```
cachedump dump --region 50_50 --decode -o out/
cachedump dump --all-regions -o out/
```

A record and everything it depends on: `npc:<id>` (models, sequences, frame packs), `item:<id>` (inventory, equip and chat-head models) or `player-kit` (identity-kit parts and the standard stance sequences):

```
cachedump dump --group npc:1 -o out/
```

Output options, valid on every `dump` and `decode`:

- `--meta` writes a `<id>.meta.json` sidecar beside each output: source, sector chain, sizes, CRC, compression and a summary for the type.
- `--meta-only` writes only the sidecars.
- `--xref` adds reverse references to sidecars (models → textures, textures → models). They are computed once and kept in `<out>/xref.json`, which also lists object → model, sequence → frame pack and region → object references.
- `--jsonl` writes `<out>/<type>.jsonl`, one record per line, instead of a file per record.

Every run writes `<out>/manifest.json`, which lists each file with its source, sizes and CRC32, plus absent ids and failures. It is sorted, so two runs over the same input produce identical manifests.

## `decode`

Shorthand for `dump --type`:

```
cachedump decode npc --id 1 -o out/
```

## `info`

Print one record's metadata sidecar to stdout:

```
cachedump info --type model --id 0
```

## `verify`

Follow every sector chain and check every header, inflate every file, decode every archive, and compare indices 1–4 with the versionlist's versions and CRCs:

```
cachedump verify
```

It reports, per index, files that are present, absent, truncated, bad-sector or inflate-failed, with ids, and exits `3` if any are damaged. Files that differ from the versionlist are listed as *modified*, which is not an error.
