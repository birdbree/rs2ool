[Home](../README.md) › [Tools](README.md) › Dump output

# Dump output

Where `rs2ool cache dump` and `decode` put things, and what is in `manifest.json`, the `.meta.json` sidecars, `xref.json` and `interface/tree.json`. Read this when you script against a dump directory or open one in the explorer.

The writer is [`cache/output.rs`](../../crates/rs2ool/src/cache/output.rs) and the paths come from [`cache/dump.rs`](../../crates/rs2ool/src/cache/dump.rs). Sidecars are `rs2cache::types::record::Meta` ([`types/record.rs`](../../crates/rs2cache/src/types/record.rs)). All paths below are relative to the `-o` directory and use `/`.

## Output paths

| What | Path |
| --- | --- |
| index file (`--index N`) | `idx<N>/<id>.bin` |
| archive entry (`--archive`) | `archive<N>/<entry>`, the entry's name or `#hash`, no extension added |
| whole archive (`--archive --raw`) | `archive<N>/<N>.jag` |
| region files (`--region`, `--all-regions`) | `map/<x>_<y>.m.bin` (tiles) and `map/<x>_<y>.l.bin` (locations) |
| decoded record | `<type>/<id>.json` |
| decoded region | `map/<x>_<y>.json` |
| texture | `texture/<id>.png` |
| sprite sheet | `sprite/<id>-<archive>-<entry>-<n>.png`, one per sub-sprite `n` |
| font | `font/<name>.png` and `font/<name>.metrics.json` |
| MIDI | `midi/<id>.mid` |
| versionlist (`--archive versionlist --decode`) | `versionlist.json` |
| `--jsonl` | `<type>.jsonl` |
| every run | `manifest.json` |

`<N>` is the index or archive number. In sprite paths, `<archive>` is the archive's name and `<entry>` is the sprite's entry name with `.dat` removed (or `#hash`). Font names are the four font entry names (`p11_full`, `p12_full`, `b12_full`, `q8_full`).

Bytes files (`.bin`, archive entries) are inflated unless you pass `--raw`. Decoded JSON is the record's own structure, pretty-printed with a trailing newline. Image outputs are RGBA PNG:

- A **texture** is written once, on its untrimmed canvas.
- A **sprite** sheet writes each sub-sprite trimmed to its own size. Sub-sprites with zero width or height are skipped.
- A **font** writes its 256 glyphs as one 16 × 16 sheet. Each cell is the widest glyph plus 2 pixels wide and as tall as the largest `height + y offset`; each glyph sits at its x draw offset (negative offsets count as 0) and its y offset within its cell.

### Font metrics

`font/<name>.metrics.json`:

```json
{
  "name": "p12_full",
  "advance": [ … 256 numbers … ],
  "draw_offset_x": [ … 256 … ],
  "draw_offset_y": [ … 256 … ],
  "glyph_sizes": [[w, h], … 256 pairs …]
}
```

See [Fonts](../records/font.md) for what each field means.

### JSON Lines

With `--jsonl`, each record type in the run goes into one `<type>.jsonl` file instead of a file per record: one line per record, in id order, each line the type-tagged form `{"type": "<type>", "record": { … }}`. No PNG, `.mid` or metrics files are written. Sidecars are still written when you ask for them.

## manifest.json

Every run that gets past its usage checks writes `manifest.json`, even when nothing else was written:

```json
{
  "files": [
    {
      "path": "model/0.json",
      "sources": [{ "kind": "index", "index": 1, "id": 0 }],
      "raw_size": 1234,
      "inflated_size": 5678,
      "crc32": 305419896
    }
  ],
  "absent": 0,
  "failures": [
    { "record": "model 12", "error": "…" }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `files` | every file written (sidecars included), sorted by `path`, one entry per path |
| `path` | relative to the output directory |
| `sources` | the stored files it came from: `{"kind": "index", "index", "id"}` or `{"kind": "archive", "archive", "entry"}` (entry name or `#hash`) |
| `raw_size`, `inflated_size` | stored and inflated sizes of the sources, summed |
| `crc32` | CRC32 of the file as written |
| `absent` | how many requested ids were absent |
| `failures` | records that couldn't be written, sorted by `record`, each with its error |

Files that don't come from one stored file (`xref.json`, `<type>.jsonl`) have empty `sources` and zero sizes. Because everything is sorted, two runs over the same input produce the same manifest byte for byte. Any failure makes the run exit 3.

## Sidecars

With `--meta`, every output gets a sidecar named after its stem: `model/0.meta.json`, `idx1/0.meta.json`, `sprite/<id>-<archive>-<entry>.meta.json` (one per sheet, not per PNG), `map/48_48.meta.json`, `map/48_48.m.meta.json`, `archive2/loc.dat.meta.json`.

A record's sidecar names the record, its stored files, and a summary for its type:

```json
{
  "record_type": "model",
  "id": 0,
  "files": [
    {
      "source": { "kind": "index", "index": 1, "id": 0 },
      "sector_chain": [101, 102],
      "raw_size": 1234,
      "inflated_size": 5678,
      "crc32": 305419896,
      "compression": "gzip"
    }
  ],
  "summary": "model",
  "vertex_count": 120,
  "face_count": 200,
  "…": "…"
}
```

| Field | Meaning |
| --- | --- |
| `record_type`, `id` | the record |
| `files[].source` | as in the manifest |
| `files[].sector_chain` | sector numbers in chain order, for index 1–4 files; left out for archive entries |
| `files[].raw_size`, `inflated_size` | stored and unpacked sizes |
| `files[].crc32` | CRC32 of the stored bytes for index files; for archive entries, of the entry's unpacked bytes |
| `files[].compression` | `gzip` or `none` for index files; `bzip2` (entry compressed on its own), `bzip2-archive` (archive compressed whole) or `none` for archive entries |
| `summary` | which summary follows; its fields sit beside it at the top level |

Raw index 0 and 4 files, region `.m`/`.l` files and archive entries aren't records on their own, so their sidecars are just one `files` element (`source`, `sector_chain`, sizes, `crc32`, `compression`) without `record_type` or a summary. Index 1–3 file sidecars are full record sidecars.

### TypeSummary variants

| `summary` | For | Fields |
| --- | --- | --- |
| `model` | model | `vertex_count`, `face_count`, `textured_face_count`, `vertex_label_count` and `face_label_count` (distinct labels), `bounds` (`[[min x, y, z], [max x, y, z]]`, or null), `textures` (with `--xref`) |
| `anim` | frame pack | `frame_ids`, `transform_count` (base slots), `labels` (every label the base names, sorted, once each) |
| `map` | region | `region` (`x << 8 \| y`), `members`, `height_range` (`[lowest, highest]` per plane), `object_count` (placed locations) |
| `config` | `loc`, `npc`, `obj`, `idk`, `seq`, `spotanim`, `flo`, `varp`, `varbit` | `definition`: the whole decoded definition, type-tagged |
| `sprite` | texture, sprite sheet | `width`, `height` (the canvas), `palette_size`, `sub_sprites`, `used_by_models` (textures, with `--xref`) |
| `font` | font | `glyphs`, `max_height` |
| `interface` | interface | `kind`, `width`, `height`, `children` (direct children of a layer, else 0) |
| `midi` | MIDI | `size` (inflated bytes) |

`rs2ool cache info` prints the same sidecar for one record.

## --meta-only

`--meta-only` writes the sidecars and `manifest.json` and nothing else: no `.bin`, JSON, PNG, `.mid` or `.jsonl` payloads. The manifest then lists only the sidecars. It's the quick way to get sizes, sector chains and summaries for a whole type. It can't be combined with `--meta`. `xref.json` isn't written in this mode either, so `--meta-only --xref` rebuilds the references on every run unless an earlier run left the file there.

## --xref and xref.json

`--xref` adds reverse references to sidecars: model sidecars get `textures` (the texture ids their textured faces use) and texture sidecars get `used_by_models`. Building them means decoding every model, every location, NPC, item and sequence definition, and every region, so the result is kept in `xref.json` in the output directory. A later run with `--xref` into the same directory reads that file instead of rebuilding it.

```json
{
  "model_textures":  { "<model>": [<texture>, …] },
  "texture_models":  { "<texture>": [<model>, …] },
  "object_models":   { "loc:<id>": [<model>, …], "npc:<id>": […], "obj:<id>": […] },
  "sequence_packs":  { "<seq>": [<frame pack>, …] },
  "region_objects":  { "<region>": [<loc id>, …] }
}
```

| Field | Holds |
| --- | --- |
| `model_textures` | per model, the texture ids its textured faces use (a textured face's colour is its texture id) |
| `texture_models` | the inverse: per texture, the models that use it |
| `object_models` | per location, NPC and item definition, the models it names; items list the inventory model and the equipment models |
| `sequence_packs` | per sequence, the frame packs its primary and secondary frames live in |
| `region_objects` | per region (`x << 8 \| y`), the distinct location ids placed in it |

JSON has no numeric keys, so every id used as a key arrives as a string; parse it back before comparing. Only models and textures are written back into sidecars; the other three tables are there for scripts.

## Groups

`--group` dumps a record together with what it depends on, decoded, one record type after another:

| Group | Resolves to |
| --- | --- |
| `npc:<id>` | the NPC definition; its models and chat-head models; its stand, walk, walk-back, walk-left and walk-right sequences; and the frame packs those sequences use |
| `item:<id>` | the item definition; its inventory model, its three male and three female equipment models, and its two male and two female chat-head models |
| `player-kit` | every identity-kit definition with its models and chat-head models; seven fixed player stance sequences and their frame packs |

Each record appears once. A sequence that doesn't decode is left out of the group rather than failing it. Frame packs include those that hold a sequence's secondary frames.

## interface/tree.json

Dumping more than one interface (for example `--type interface --all`) also writes `interface/tree.json`. It's an array with one node per root, a component whose parent is itself. A node is the component's decoded JSON; a layer's node also gets a `children` array of its child nodes, each with the child's `x` and `y` offset added, nested all the way down. A child that isn't in the dump appears as `{"id": <id>, "missing": true}`. Nesting stops at depth 32, which only a malformed cache would reach.

## See also

- [Cache commands](cache-commands.md): the flags that select what's written.
- [Data and index files](../cache/data-and-index-files.md): sector chains and the version trailer.
- [Archives](../cache/archives.md): entry names, hashes and the two compression modes.
- [Explorer](explorer.md#sources): opening a dump directory as a source.
