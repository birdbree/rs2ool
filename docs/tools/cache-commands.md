[Home](../README.md) › [Tools](README.md) › Cache commands

# Cache commands

Every `rs2ool cache` subcommand with its selectors and flags, and the rules for combining them. The files `dump` writes are on [Dump output](dump-output.md).

The definitions are in [`cli/cache.rs`](../../src/cli/cache.rs) and the commands in [`cache/`](../../src/cache/). The examples assume `RS2_CACHE` is set (see [Tools](README.md#pointing-at-a-cache)).

## list

Prints what the cache holds. With no flag it prints a table of the five indices (`index`, `files`, `present`, `absent`), a blank line, then every archive of index 0 with its entry count and entry names.

```text
rs2ool cache list
rs2ool cache list --index 4
rs2ool cache list --archive config
rs2ool cache list --type npc
```

| Flag | Prints |
| --- | --- |
| `--index N` | `index N: K ids`, then `id` and size in bytes for every present file |
| `--archive NAME` | `archive N name: K entries` (plus `, compressed whole` when it is), then per entry its name, hash, unpacked size and stored size |
| `--type TYPE` | `TYPE: K records`, then each id, with a name for `loc`, `npc` and `obj` (the definition's name), `sprite` (its archive entry), `font` (its name) and `map` (`RX_RY`) |

Give at most one of the three. Archives can be named or numbered: `title` (1), `config` (2), `interface` (3), `media` (4), `versionlist` (5), `textures` (6), `wordenc` (7), `sounds` (8). Entries whose names aren't in the built-in table of known names show as `#hash`, the signed decimal name hash.

## dump

Extracts records to files, raw or decoded. A dump takes one **source**, usually some **ids**, an output directory, and optional flags.

### Source (exactly one)

| Flag | Takes |
| --- | --- |
| `--index N` | files of index 0–4, inflated (or decoded with `--decode`) |
| `--archive NAME` | an archive's entries, or its decoded record types with `--decode` |
| `--type TYPE` | decoded records of one type |
| `--region RX_RY` | one map region's tile and location files, e.g. `48_48` |
| `--all-regions` | every region in `map_index` |
| `--group G` | a record and what it depends on: `npc:<id>`, `item:<id>` or `player-kit` |

### Ids (at most one)

| Flag | Takes |
| --- | --- |
| `--id N` | one id |
| `--range A-B` | every id from A to B inclusive that the source has |
| `--all` | every id |

`--index` and `--type` need one of these. `--archive`, `--region`, `--all-regions` and `--group` choose their own records and refuse them. An id that turns out to be absent isn't an error: it's counted in the manifest's `absent` total. Indices have empty slots, and an empty slot is just that, not damage.

### Flags

| Flag | Effect |
| --- | --- |
| `--entry NAME` | with `--archive`, only this entry (a name, or `#hash`) |
| `--raw` | write stored bytes, still compressed, instead of inflated ones; for index 1–4 files that includes the 2-byte version trailer, and for an archive it writes the whole archive file |
| `--decode` | decode instead of writing bytes: `--index 1`, `2` and `3` decode as `model`, `anim` and `midi`; `--archive` decodes the record types the archive holds; `--region` and `--all-regions` decode the region |
| `-o, --out DIR` | output directory (required) |
| `--meta` | a `.meta.json` sidecar beside every output |
| `--meta-only` | only the sidecars |
| `--xref` | reverse references in sidecars, cached in `DIR/xref.json` |
| `--jsonl` | one `TYPE.jsonl` file per record type instead of a file per record |

`--type` and `--group` always decode, so `--decode` changes nothing there. With `--archive --decode`, the config archive decodes into `loc`, `npc`, `obj`, `idk`, `seq`, `spotanim`, `flo`, `varp` and `varbit`; `title` into fonts and title sprites; `interface`, `media` and `textures` into their types; and `versionlist` into one `versionlist.json`.

### Examples

```text
rs2ool cache dump --index 1 --all -o out/
rs2ool cache dump --index 2 --range 0-99 --decode -o out/
rs2ool cache dump --archive config --entry loc.dat -o out/
rs2ool cache dump --archive media --decode --meta -o out/
rs2ool cache dump --type model --range 0-99 --meta --xref -o out/
rs2ool cache dump --type sprite --all -o out/
rs2ool cache dump --type npc --all --jsonl -o out/
rs2ool cache dump --region 48_48 --decode -o out/
rs2ool cache dump --all-regions --raw -o out/
rs2ool cache dump --group npc:1 -o out/
```

### Contradictory selectors

The command line refuses combinations that can't mean one thing. All of these exit with code 1 without writing any records:

- no source, or more than one source;
- more than one of `--id`, `--range`, `--all`;
- `--index` or `--type` without any of them;
- `--id`, `--range` or `--all` with `--archive`, `--region`, `--all-regions` or `--group`;
- `--range A-B` with A greater than B, or not in `A-B` form;
- `--entry` without `--archive`;
- `--raw` with `--type`, `--group` or `--decode`;
- `--meta` together with `--meta-only`;
- `--entry` with `--archive … --decode` (decoding works on a whole archive);
- `--index 0 --decode` (index 0 holds archives: use `--archive NAME --decode`) and `--index 4 --decode` (index 4 files are per region: use `--region` or `--all-regions`);
- `--archive --decode` on an archive with no decoder (`wordenc`, `sounds`);
- an archive name or number that doesn't exist, an index outside 0–4, or an unknown group.

`--region` takes `RX_RY` with both parts from 0 to 255. Asking for an entry the archive doesn't have isn't a usage error; it's recorded as a failure in the manifest, and the run exits 3.

## decode

Shorthand for `dump --type TYPE`, with the type as a positional argument:

```text
rs2ool cache decode npc --id 1 -o out/
rs2ool cache decode model --all --meta -o out/
```

It takes the same ids (one of `--id`, `--range`, `--all`, required) and the same output flags (`-o`, `--meta`, `--meta-only`, `--xref`, `--jsonl`).

## info

Prints one record's metadata sidecar to standard output as pretty JSON, without writing files:

```text
rs2ool cache info --type model --id 0
```

Both `--type` and `--id` are required. The sidecar format is on [Dump output](dump-output.md#sidecars); `info` doesn't add cross-references.

## verify

Reads every file of every index and checks it:

```text
rs2ool cache verify
```

For each file it follows the sector chain and checks every header, inflates index 1–4 files, and decodes index 0 files as archives. Where the versionlist can be read, it compares each index 1–4 file's version trailer and CRC with the versionlist's tables (see [Versionlist](../cache/versionlist.md)).

The report is a table, one row per index:

```text
index	present	absent	truncated	bad-sector	inflate-failed	modified
```

Then, per index, the ids in each damaged class (the first 20, then a count), and the ids that differ from the versionlist. A file is *truncated* when its chain runs out of range or a sector is short, and *bad-sector* for any other chain fault. *Modified* files have a different version or CRC from the versionlist; that isn't damage, and it doesn't change the exit code. The last line is either `no damaged files` (exit 0) or `N damaged files` (exit 3). If the versionlist can't be read, the report says so and skips the comparison.

## render

Draws a record's model with the classic renderer ([`rs2classic`](../crates/rs2classic.md), a reimplementation of the original client's software renderer) into an image file, at rest or posed by a sequence:

```text
rs2ool cache render npc --id 1 -o npc1.png
rs2ool cache render npc --id 1 --seq -o npc1-stand.gif
rs2ool cache render kit --seq 819 --sheet 6 --compare -o walk.png
```

`TYPE` is a record type with a model, or `kit`. Each draws the model the explorer shows for it:

| `TYPE` | Draws |
| --- | --- |
| `model` | the model as it is |
| `npc` | the NPC's models merged, recoloured and scaled |
| `obj` | the item's inventory model, recoloured and scaled |
| `idk` | the identity-kit part's models merged and recoloured |
| `spotanim` | the spot animation's model, recoloured and scaled |
| `loc` | the location's placement-type 10 model (or its first), mirrored, scaled and offset |
| `kit` | the default male player body, or with `--female` the female one, as `pack player-kit` builds it |

| Flag | Effect |
| --- | --- |
| `--id N` | the record (required, except for `kit`, which takes none) |
| `--female` | with `kit`, the female body |
| `--seq [ID]` | pose the model with sequence `ID`; with no id, the record's own: an NPC's stand sequence, a spot animation's or location's sequence, or for `kit` the player's stand sequence (808) |
| `--frame N` | draw just frame `N` of the sequence (its index, from 0) |
| `--sheet [N]` | draw a grid of `N` frames (9 with no `N`) spaced evenly in time, first and last included |
| `--compare` | draw the converted rig and clip beside the original, and print the clip's validation |
| `--moving SEQ` | play the sequence over movement sequence `SEQ`, as the creature acts while it moves |
| `--size N` or `--size WxH` | the image size, or each tile's with `--sheet`, 1–4096; default 512, or 256 with `--sheet` |
| `--yaw A` | the camera's turn about the model, 2048 a turn; default 256, three-quarters on |
| `--pitch A` | the camera's tilt, 2048 a turn, positive looking down; default 48 |
| `--distance D` | the camera's distance from the model's centre, in cache units; fitted when not given |
| `--background RRGGBB` | the background colour; default `8a9ba8` |
| `-o, --out FILE` | the file to write (required) |

`--frame`, `--sheet`, `--compare` and `--moving` need `--seq`. Without `--frame` or `--sheet`, a sequence is drawn as an animation, every frame shown for its own delay at 20 ms a client cycle, looping forever.

**The file.** The extension picks the format: `.gif` writes a GIF, anything else a PNG. A PNG with more than one frame is an animated PNG (APNG), lossless; viewers that don't play APNG show its first frame. A GIF holds 256 colours per frame. The command prints one line saying what it wrote:

```text
walk.png	1536×512 PNG
stand.gif	512×512 GIF, 12 frames, 1.44 s
```

**The camera** is the original client's model-viewer camera, turned about the centre of the box around every pose drawn, so all frames share one camera and a model that moves, falls or shrinks does so on screen. The fitted distance keeps the box's bounding sphere inside a margin of a sixteenth of the shorter side. Faces are one-sided, as in the original client, so a flat model only shows from its front; turn the camera (`--yaw 1280`, `--pitch -48`) if one comes out empty. Everything is lit with the NPC lighting, like the explorer's classic view.

**Posing.** Each frame is posed exactly as the original client deforms the model, the same reference [validation](../conversion/validation.md) measures against, then scaled. A location is posed first and shaped after, as the original client does it. Faces fade as the original client fades them ([Animation](../client/animation.md#alpha)), and with `--compare` the converted side fades through its clip's fades. A band of ground is drawn at the lowest point of the first frame shown.

**`--compare`** draws every frame twice: as the original client deforms it, and as the converted rig and clip pose it. An animation or a single frame shows the two side by side, the original on the left; a sheet shows a strip of the original frames over a strip of the converted ones. The conversion is the one `rs2ool pack` writes, from the same assembly: an NPC with its stance and walk sequences plus the one drawn, as `pack npc --seq` converts it, the kit as `pack player-kit` with the sequence added, and a spot animation as `pack spotanims`. The command then prints the clip's validation, and, like the pack commands, ends the line in `FLAGGED` and exits 3 when the clip is over the threshold:

```text
sequence 819: max error 2.08, mean 0.66
```

**`--moving SEQ`** plays a sequence with an interleave list over a movement sequence (walk, run or turn) the way the original client draws a creature acting while it moves ([Animation](../client/animation.md#interleaving)): the movement drives the legs and the body's travel, the sequence the rest, each on its own clock from the start, the movement looping. An animation steps whenever either sequence changes frame, over one run of the sequence; a sheet or a single frame pairs each frame with the movement frame showing when it starts. With `--compare`, the converted side mixes the two clips through the clip's moving bones, and a second line gives the mix's validation (flagged over 8.0, [Mixes](../conversion/validation.md#mixes)):

```text
rs2ool cache render kit --seq 439 --moving 819 --sheet 6 --compare -o spin-walk.png
sequence 439: max error 2.07, mean 0.75
sequence 439 while moving (seq 819): max error 1.77, mean 0.65
```

A sequence without an interleave list plays alone, and the command says so.

A record type with no model, a record without one, a missing or unwanted `--id`, a bare `--seq` on a record with no sequence of its own, and a frame past the end are usage errors (exit 1).

## structures

Finds whole structures in the map, with their rooms, doors, openings, walls, roofs and contents, and lists them or writes them as JSON:

```text
rs2ool cache structures --region 50_50
rs2ool cache structures --all -o out/
```

| Flag | Effect |
| --- | --- |
| `--region RX_RY` | search that region and its eight neighbours, and keep the structures whose south-west corner is in it |
| `--all` | search the whole map at once |
| `-o DIR` | write `DIR/structures/RX_RY.json` for each region with structures, instead of listing them |

Without `-o`, each structure prints as one line: its region, origin, size, planes, rooms, doors (and how many lead outside), open edges, and its objects with the commonest named ones:

```text
50_50  at 3238,3204  10×12  plane 0  1 rooms  3 doors (3 to outside)  0 open edges  82 objects (69 unnamed): Church pew ×6, Candles ×4, Altar, Church organ, Stool
```

Each file holds `region` (`RX_RY`) and `structures`, the blueprints of the structures whose origin is in it, with every field described on [Structures](../conversion/structures.md#the-blueprint). A structure belongs to the region its south-west corner is in, so each comes out once, whole, even when it crosses into a neighbour. One that goes on past the regions searched is marked `clipped`. The whole map takes under a second in a release build. Regions that won't decode are reported on stderr and skipped, and the exit code is then 3.

## caption

Draws an NPC's candidate sequences and asks a vision model on a server you run what each one shows, keeping the answers locally:

```text
rs2ool cache caption --npc 1 --dry-run --images frames/
rs2ool cache caption --npc 1 --enable --model <a vision model>
```

It's off until you turn it on, and nothing is sent anywhere on a dry run. Setup, settings, flags, the images and where captions go are on [Captions](captions.md).

## classify

Learns sequence roles from reviewed captions (and a labels file, if given), and compares them with the hand-tuned rules:

```text
rs2ool cache classify --verbose
rs2ool cache classify --save
```

The flags and the report are on [Captions](captions.md#learning-roles-from-reviews), and the method is in [Inference](../conversion/inference.md#learning-roles-from-labels).

## explorer

`rs2ool cache explorer` opens the GUI on the cache. Its startup options are on [Explorer](explorer.md#command-line-startup).

## See also

- [Dump output](dump-output.md): the files, the manifest, sidecars, xref and groups.
- [Data and index files](../cache/data-and-index-files.md): the sector chains `verify` walks.
- [Archives](../cache/archives.md): entry names, hashes and compression.
- [Records](../records/README.md): what each record type decodes to.
