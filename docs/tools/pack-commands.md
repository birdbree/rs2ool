[Home](../README.md) › [Tools](README.md) › Pack commands

# Pack commands

Every `rs2ool pack` subcommand: what it converts, which packs it writes and where, and the side files and reports that come with them. The record layouts inside the packs are in the [Pack](../pack/README.md) section.

The definitions are in [`cli/pack.rs`](../../crates/rs2ool/src/cli/pack.rs) and [`cli/convert.rs`](../../crates/rs2ool/src/cli/convert.rs), which are thin wrappers over [`rs2convert::run`](../../crates/rs2convert/src/run.rs). The explorer's Actions menu calls the same functions, so a conversion from the GUI writes the same bytes.

## Where packs go

Every converting command takes `-o, --out DIR` and writes each pack to:

```text
DIR/<kind>/<hash>.pack
```

`<kind>` is the pack kind's name (`models`, `materials`, `textures`, `sprites`, `objects`, `kits`, `rigs`, `anims`, `strings`) and `<hash>` is the pack's SHA-256 in hex (see [Container](../pack/container.md)). Converting the same cache twice writes byte-identical packs with the same names, so re-running a command never piles up copies. Different inputs give different names, and old packs aren't removed.

For each pack written, the command prints one line, then any notes and the per-clip validation results:

```text
models	1 records	packs/models/<hash>.pack
```

Every pack records the source cache's CRC (the CRC32 of the stored versionlist archive).

## The commands

| Command | Converts | Writes |
| --- | --- | --- |
| `materials` | the palette, built from every model colour | `materials` (palette record and default palette material), `palette.json`, `palette.png` |
| `textures` | the 50 textures | `textures` (one atlas), `materials` (one material per texture) |
| `models (--id N \| --all)` | cache models | `models` |
| `npc --id N` | an NPC's merged model and inferred rig | `models`, `rigs`, plus `anims` with `--with-anims`; `npc_<N>/validation.json`, `validation_summary.json` |
| `player-kit` | every identity-kit part, and the default body rigged with the player stance sequences | `models` (parts and body), `rigs`, plus `anims` with `--with-anims`; `player/validation.json`, `validation_summary.json` |
| `sprites` | media and title sprites, and the fonts | `sprites` |
| `floors` | floor types | `materials` (floor materials), `strings` |
| `objects (--id N \| --all)` | location definitions | `models`, `objects`, `kits` (wall-class definitions), `strings`; an empty pack is skipped |

Every command except `textures` and `sprites` first builds the palette from every model colour, so even a one-model run reads all models. The palette comes out the same every time, so colour indices agree across separate runs.

### models

```text
rs2ool pack models --id 0 -o packs/
rs2ool pack models --all -o packs/
```

One of `--id` or `--all` is required. Record ids are the cache model ids.

### npc

```text
rs2ool pack npc --id 1 -o packs/
rs2ool pack npc --id 1 --with-anims -o packs/
rs2ool pack npc --id 1 --with-anims --seq 100,101 --override parents.json -o packs/
```

| Flag | Effect |
| --- | --- |
| `--id N` | the NPC (required) |
| `--with-anims` | also convert the NPC's sequences into clips and validate each one |
| `--seq A,B,…` | extra sequence ids to convert (comma-separated, or repeat the flag), since an NPC definition only names its stand and walk sequences |
| `--override FILE` | a parent table that replaces rig inference's parent choice |

The merged model goes in as model record `0x10000000 | N`, the rig as rig `N`, and each clip as anim `N << 16 | seq` (see [Record ids](../pack/record-ids.md)).

### player-kit

```text
rs2ool pack player-kit --with-anims -o packs/
```

Takes `--with-anims` and `--override` like `npc`. Each identity-kit part is model `0x20000000 | idk`; the default body is model `0x30000000` with rig `0xFFFF`. The models pack's manifest lists each part's identity-kit id, body part, body type and whether it's selectable under `kit_parts`.

### sprites

```text
rs2ool pack sprites -o packs/
rs2ool pack sprites --archive media -o packs/
```

With no flag, or with `--all`, it converts both the media and title archives, and the fonts with the title archive. `--archive media` or `--archive title` converts just one. Any other archive is an error (exit 2).

### textures, materials, floors, objects

```text
rs2ool pack materials -o packs/
rs2ool pack textures -o packs/
rs2ool pack floors -o packs/
rs2ool pack objects --id 1 -o packs/
rs2ool pack objects --all -o packs/
```

`objects --id N` also converts the models of the definitions in its morph table, since its states draw them.

## palette.json and palette.png

`materials` writes two side files next to the kind directories.

`palette.json` reports how well the palette fits the colours it was built from, in RGB units:

```json
{
  "colours": 1234,
  "faces": 567890,
  "mean": 1.23,
  "max": 9.87,
  "worst_hsl": 12345
}
```

| Field | Meaning |
| --- | --- |
| `colours` | distinct face colours in the corpus |
| `faces` | faces counted |
| `mean` | mean distance from each face's colour to its palette entry, over faces |
| `max` | the largest distance for any distinct colour |
| `worst_hsl` | the 16-bit HSL colour with that largest distance |

`palette.png` is a 256 × 256 swatch: a 32 × 32 grid of 8-pixel cells, entry `i` at column `i % 32` and row `i / 32`. Cells past the last entry are transparent. See [Palette](../conversion/palette.md) for how the palette is built.

## validation.json

`npc` writes `DIR/npc_<N>/validation.json`, and `player-kit` writes `DIR/player/validation.json`. They're written with or without `--with-anims`; without it, `clips` is empty.

```json
{
  "creature": "<name>",
  "bones": [
    { "name": "root", "parent": null, "labels": [] },
    { "name": "…", "parent": 0, "labels": [0, 1] }
  ],
  "max": 1.5,
  "flagged": false,
  "clips": [
    {
      "sequence": 100,
      "max": 1.5,
      "mean": 0.2,
      "flagged": false,
      "frames": [
        {
          "frame": 0,
          "max": 1.5,
          "mean": 0.2,
          "worst": [ { "vertex": 12, "label": 3, "bone": 2, "error": 1.5 } ]
        }
      ]
    }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `creature` | the NPC's name, or `player` |
| `bones` | the inferred bones, parents first: name, parent bone index and the vertex labels the bone stands for; bone 0 is the root, with no labels and a `null` parent |
| `max` | the worst vertex error over every clip |
| `flagged` | whether any clip is flagged |
| `clips[].sequence` | the sequence id |
| `clips[].max`, `mean` | the clip's worst vertex error, and the mean of its frames' mean errors |
| `clips[].flagged` | `max` is over the threshold, 4.0 cache units |
| `frames[].frame` | the frame's index in the sequence |
| `frames[].worst` | the five worst vertices: vertex index, its label, the bone it's skinned to, and its error |

An error is the distance between a vertex skinned by the converted rig and clip records and the same vertex deformed by the reference deformation. See [Validation](../conversion/validation.md).

## validation_summary.json

`DIR/validation_summary.json` has one entry per converted creature. Each run merges its creatures into the file already there, so converting several NPCs into one directory builds up one summary:

```json
{
  "npc_1": { "name": "<name>", "max": 1.5, "flagged": false, "bones": 12 },
  "player": { "name": "player", "max": 0.8, "flagged": false, "bones": 20 }
}
```

`bones` here is the number of bones in the rig record.

## Exit code 3

When any converted clip is flagged, the packs and reports are still written in full, the clip's line on standard output ends in `FLAGGED`, and the command exits with code 3. A flagged clip is a conversion that came out measurably wrong, so the exit code says so even though nothing failed.

## --override files

`--override` takes a JSON parent table. Each element pairs a bone candidate's label list with its parent's label list:

```json
[
  [[4, 5], [1, 2, 3, 4, 5]],
  [[1, 2, 3, 4, 5], []]
]
```

A bone candidate is a set of labels some frame transform moves (see [Rig inference](../conversion/rig-inference.md)). Each list has to match a candidate's labels exactly, sorted ascending. An empty parent list attaches the bone to the root bone. Pairs apply after inference and replace its choice for the bones they name; bones not named keep the inferred parent. A list that matches no candidate stops the conversion with an error. `validation.json` lists the candidates' label lists under `bones[].labels`, which is the easiest place to copy them from.

## inspect

```text
rs2ool pack inspect packs/models/<hash>.pack
```

Prints a pack's hash, its manifest and its index, tab-separated:

```text
hash	<sha-256 hex>
manifest	{"kind":"models","record_count":1,"source_cache_crc":…,"codec":…}
id	offset	length	crc
0	0	1234	89abcdef
```

| Column | Meaning |
| --- | --- |
| `hash` | the SHA-256 of the pack's index and blob, recomputed when the pack is opened |
| `manifest` | the manifest JSON on one line |
| `id` | the record id, decimal |
| `offset`, `length` | where the record sits in the blob and how long it is |
| `crc` | the record's CRC, 8 hex digits |

`inspect` doesn't need a cache. The fields are described on [Container](../pack/container.md).

## build

```text
rs2ool pack build loose/ -o out/
rs2ool pack build loose/ -o out/custom.pack
```

Bakes a loose directory into one hashed pack. The directory holds:

- `pack.json`: `{"kind": "<kind>", "source_cache_crc": <n>, "extra": { … }}`, where `extra` is optional and becomes the manifest's kind-specific tables;
- one `<id>.bin` per record, the file name being the decimal record id.

Other files are ignored, but a `.bin` whose name isn't a number is an error. If `-o` ends in `.pack`, that's the file written; otherwise it's a directory and the pack goes in it as `<hash>.pack` (directly, without a `<kind>/` subdirectory). The command prints the path it wrote. It doesn't need a cache. See [Container](../pack/container.md) for loose directories.

## explorer

`rs2ool pack explorer PACK…` opens the GUI on one or more packs, and on the cache too when `--cache` or `RS2_CACHE` gives one. Its options are on [Explorer](explorer.md#command-line-startup).

## See also

- [Packs](../pack/README.md): the container and every record kind.
- [Conversion](../conversion/README.md): what each converter does to the records.
- [Determinism](../conversion/determinism.md): why repeated runs give identical packs.
- [Creatures](../conversion/creatures.md): the NPC and player-kit pipeline.
